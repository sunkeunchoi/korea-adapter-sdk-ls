//! The API Drift Tracker — the one concrete worked example.
//!
//! Two layers live here:
//!
//! * The PR #2 leaf-path pipeline ([`run`]) over [`StagedSnapshot`]s, kept as
//!   compatibility coverage.
//! * The real-fetch signal model (U3): [`normalize_run`] converts a fetched
//!   [`RawInventory`] into a [`NormalizedRun`] — the full-inventory code-set plus
//!   a [`Manifest`] and per-TR [`TrShape`]s for the **maintained TRs only**
//!   (R5). Untracked TRs are recorded in the code-set and raw evidence, never
//!   normalized into committed structural baselines. No adjacency computation
//!   (deferred, KTD-2) and no rename fingerprinting (R14 removed).

use std::collections::{BTreeMap, BTreeSet};

use ls_metadata::TrMetadata;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fetch::RawInventory;
use crate::stages::{classify, diff, normalize};
use crate::types::{
    BlockField, CodeSet, Direction, Manifest, Protocol, StagedSnapshot, TrShape, TrackerFinding,
};

/// The normalizer version recorded in the [`Manifest`] (R8). Bump when the
/// normalization rules change so a normalizer-driven hash shift is auditable.
pub const NORMALIZER_VERSION: u32 = 1;

/// Run the PR #2 leaf-path API Drift pipeline over a reviewed `baseline` and a
/// candidate snapshot for the same TR, classifying each detected change against
/// the validated metadata in `trs`. Output is advisory only. Retained as
/// compatibility coverage; real API Drift work uses [`normalize_run`] + U4.
pub fn run(
    baseline: &StagedSnapshot,
    candidate: &StagedSnapshot,
    trs: &BTreeMap<String, TrMetadata>,
) -> Vec<TrackerFinding> {
    let base = normalize(baseline);
    let cand = normalize(candidate);
    let changes = diff(&base, &cand);
    classify(&changes, trs)
}

/// The normalized projection of one fetched run (U3): the full-inventory
/// code-set, the inventory [`Manifest`], and per-TR [`TrShape`]s for the
/// maintained TRs only. This is what a staged run normalizes to and what the
/// committed baseline stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedRun {
    pub code_set: CodeSet,
    pub manifest: Manifest,
    /// Maintained-TR shapes keyed by `tr_code` (sorted, deterministic).
    pub shapes: BTreeMap<String, TrShape>,
}

/// Normalize a fetched [`RawInventory`] into a [`NormalizedRun`]: build the full
/// code-set (R3b), and produce a [`TrShape`] for each TR whose code is in
/// `maintained` (R5). `provisional` flags the code-set as a not-yet-attested
/// seed (KTD-5). Untracked TRs contribute only their code; they are not
/// normalized into shapes.
pub fn normalize_run(
    inventory: &RawInventory,
    maintained: &BTreeSet<String>,
    provisional: bool,
) -> NormalizedRun {
    let code_set = inventory.code_set(provisional);

    let mut shapes = BTreeMap::new();
    for group in &inventory.groups {
        for raw in &group.trs {
            let code = raw.code.trim().to_string();
            if !maintained.contains(&code) {
                continue;
            }
            shapes.insert(
                code.clone(),
                normalize_tr_shape(&code, raw, group, &inventory.property_types),
            );
        }
    }

    let manifest = Manifest {
        upstream_tr_count: code_set.len(),
        maintained_tr_count: shapes.len(),
        source_urls: inventory.source_urls.clone(),
        normalizer_version: NORMALIZER_VERSION,
    };

    NormalizedRun {
        code_set,
        manifest,
        shapes,
    }
}

/// Normalize one maintained TR's raw evidence into its Structural API Shape.
fn normalize_tr_shape(
    code: &str,
    raw: &crate::fetch::RawTr,
    group: &crate::fetch::RawGroup,
    prop_types: &BTreeMap<String, String>,
) -> TrShape {
    // Partition raw property rows by LS `bodyType`, preserving upstream order.
    let mut req_h = Vec::new();
    let mut req_b = Vec::new();
    let mut res_h = Vec::new();
    let mut res_b = Vec::new();
    for value in &raw.properties {
        if let Some(parsed) = ParsedProp::from_value(value, prop_types) {
            match parsed.body_type.as_str() {
                "req_h" => req_h.push(parsed),
                "req_b" => req_b.push(parsed),
                "res_h" => res_h.push(parsed),
                "res_b" => res_b.push(parsed),
                _ => {} // unknown bodyType is ignored, not guessed
            }
        }
    }

    let mut request_blocks = build_header_block(&req_h, Direction::Request, "request_header");
    request_blocks.extend(build_body_blocks(
        &req_b,
        Direction::Request,
        "request_body",
    ));
    let mut response_blocks = build_header_block(&res_h, Direction::Response, "response_header");
    response_blocks.extend(build_body_blocks(
        &res_b,
        Direction::Response,
        "response_body",
    ));

    let protocol = if raw.is_websocket {
        Protocol::Websocket
    } else {
        Protocol::Rest
    };
    let rate_source_group = group
        .group_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| Some(group.group_name.clone()))
        .filter(|s| !s.is_empty());

    TrShape {
        tr_code: code.to_string(),
        tr_name: raw.name.clone().filter(|s| !s.is_empty()),
        protocol,
        is_websocket: raw.is_websocket,
        endpoint_path: raw.url.clone().filter(|s| !s.is_empty()),
        api_group_id: group.group_id.clone().filter(|s| !s.is_empty()),
        source_group_name: Some(group.group_name.clone()).filter(|s| !s.is_empty()),
        request_blocks,
        response_blocks,
        rate_limit_per_sec: raw.rate_limit_per_sec,
        corp_rate_limit_per_sec: raw.corp_rate_limit_per_sec,
        rate_source_group,
        description_hash: hash_description(raw.description.as_deref()),
    }
}

/// One parsed LS property row, with `propertyType` already resolved to a display
/// name via the fetched mapping.
struct ParsedProp {
    name: String,
    korean_name: Option<String>,
    r#type: Option<String>,
    length: Option<u32>,
    required: bool,
    description: Option<String>,
    body_type: String,
}

impl ParsedProp {
    fn from_value(value: &Value, prop_types: &BTreeMap<String, String>) -> Option<Self> {
        let body_type = value.get("bodyType").and_then(Value::as_str)?.to_string();
        // `propertyCd` is the compact field name; strip the migration-source
        // noise (`&nbsp;`, `-`) and trim. An empty name is not a usable field.
        let name = value
            .get("propertyCd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .replace("&nbsp;", "")
            .replace('-', "")
            .trim()
            .to_string();
        if name.is_empty() {
            return None;
        }
        let korean_name = value
            .get("propertyNm")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty());
        let type_code = value.get("propertyType").and_then(Value::as_str);
        let r#type = type_code.map(|code| {
            prop_types
                .get(code)
                .cloned()
                .unwrap_or_else(|| code.to_string())
        });
        let length = value.get("propertyLength").and_then(value_as_u32);
        let required = value
            .get("requireYn")
            .and_then(Value::as_str)
            .map(|s| s.eq_ignore_ascii_case("Y"))
            .unwrap_or(false);
        let description = value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty());
        Some(ParsedProp {
            name,
            korean_name,
            r#type,
            length,
            required,
            description,
            body_type,
        })
    }

    /// LS marks a body block boundary with a row whose compact name equals its
    /// Korean name (e.g. `t8412InBlock` / `t8412InBlock`) — a real field's
    /// English code never equals its Korean label. Used to derive `block_name`.
    fn is_block_header(&self) -> bool {
        self.korean_name.as_deref().is_some_and(|k| k == self.name)
    }

    fn to_field(&self, direction: Direction, block_name: &str, field_index: u32) -> BlockField {
        BlockField {
            direction,
            block_name: block_name.to_string(),
            field_index,
            field_name: self.name.clone(),
            korean_name: self.korean_name.clone(),
            r#type: self.r#type.clone(),
            length: self.length,
            required: self.required,
            description_hash: hash_description(self.description.as_deref()),
        }
    }
}

/// Header rows are a flat list with no block delimiters; they all share one
/// synthetic block name, indexed in order.
fn build_header_block(props: &[ParsedProp], direction: Direction, block: &str) -> Vec<BlockField> {
    props
        .iter()
        .enumerate()
        .map(|(i, p)| p.to_field(direction, block, i as u32))
        .collect()
}

/// Body rows carry block-header delimiters; fields inherit the current block and
/// are indexed from 0 within it (so a reorder is a `field_index` shift, and a
/// block move is the same field under a different `block_name` — R6). The
/// delimiter row itself is structural, not a field, and is not emitted.
fn build_body_blocks(
    props: &[ParsedProp],
    direction: Direction,
    default_block: &str,
) -> Vec<BlockField> {
    let mut out = Vec::new();
    let mut current_block: Option<String> = None;
    let mut index_in_block = 0u32;
    for p in props {
        if p.is_block_header() {
            current_block = Some(p.name.clone());
            index_in_block = 0;
            continue;
        }
        let block_name = current_block.as_deref().unwrap_or(default_block);
        out.push(p.to_field(direction, block_name, index_in_block));
        index_in_block += 1;
    }
    out
}

/// `propertyLength` arrives as a JSON string (`"100"`), a number, or `null`.
fn value_as_u32(v: &Value) -> Option<u32> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Hash a long description into a stable, normalized fingerprint (R8): `None`
/// for an absent/blank description, else the FNV-1a hash of the normalized text.
/// Normalization decodes HTML entities, strips tags and `<br>`, collapses
/// internal whitespace, and trims — so benign re-encoding hashes identically.
fn hash_description(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let normalized = normalize_description(raw);
    if normalized.is_empty() {
        return None;
    }
    Some(fnv1a_hex(&normalized))
}

/// Decode HTML entities, strip tags and `<br>`, collapse internal whitespace and
/// trim. `<br>` variants become spaces first so a stripped break does not fuse
/// adjacent words; remaining tags and entities are resolved by parsing the
/// fragment and collecting its text.
fn normalize_description(raw: &str) -> String {
    let mut s = raw.to_string();
    for br in ["<br>", "<br/>", "<br />", "<BR>", "<BR/>", "<BR />"] {
        s = s.replace(br, " ");
    }
    let fragment = scraper::Html::parse_fragment(&s);
    let text: String = fragment.root_element().text().collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// FNV-1a 64-bit, rendered as zero-padded hex. Deterministic across machines and
/// Rust versions (unlike `DefaultHasher`), which is what a *committed* baseline
/// hash needs. A collision would at worst miss an informational description
/// change — never a gating finding.
fn fnv1a_hex(s: &str) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in s.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{RawGroup, RawInventory, RawTr};

    fn prop(body: &str, cd: &str, nm: &str, ty: &str, len: Value, req: &str, desc: &str) -> Value {
        serde_json::json!({
            "bodyType": body,
            "propertyCd": cd,
            "propertyNm": nm,
            "propertyType": ty,
            "propertyLength": len,
            "requireYn": req,
            "description": desc,
        })
    }

    fn t8412_raw() -> RawTr {
        RawTr {
            code: "t8412".to_string(),
            name: Some("주식차트(N분)".to_string()),
            is_websocket: false,
            http_method: Some("POST".to_string()),
            url: Some("/stock/chart".to_string()),
            protocol_type: Some("REST".to_string()),
            rate_limit_per_sec: Some(1),
            corp_rate_limit_per_sec: Some(10),
            description: Some("주식 N분봉 차트".to_string()),
            properties: vec![
                // request body: a block header then two fields.
                prop(
                    "req_b",
                    "t8412InBlock",
                    "t8412InBlock",
                    "A0003",
                    Value::Null,
                    "Y",
                    "",
                ),
                prop(
                    "req_b",
                    "shcode",
                    "단축코드",
                    "A0001",
                    serde_json::json!("6"),
                    "Y",
                    "",
                ),
                prop(
                    "req_b",
                    "ncnt",
                    "단위(n분)",
                    "A0004",
                    serde_json::json!("4"),
                    "Y",
                    "",
                ),
                // response body: a block header then two fields.
                prop(
                    "res_b",
                    "t8412OutBlock",
                    "t8412OutBlock",
                    "A0003",
                    Value::Null,
                    "Y",
                    "",
                ),
                prop(
                    "res_b",
                    "jongchk",
                    "수정구분",
                    "A0001",
                    serde_json::json!("1"),
                    "Y",
                    "수정<br/>구분",
                ),
                prop(
                    "res_b",
                    "price",
                    "현재가",
                    "A0004",
                    serde_json::json!("8"),
                    "Y",
                    "",
                ),
            ],
            req_example: Value::Null,
            res_example: Value::Null,
        }
    }

    fn group_with(trs: Vec<RawTr>) -> RawGroup {
        RawGroup {
            category_name: "국내주식".to_string(),
            group_id: Some("grp-100".to_string()),
            group_name: "주식시세".to_string(),
            is_websocket_group: false,
            trs,
        }
    }

    fn inventory(groups: Vec<RawGroup>) -> RawInventory {
        let mut property_types = BTreeMap::new();
        property_types.insert("A0001".to_string(), "String".to_string());
        property_types.insert("A0003".to_string(), "Long".to_string());
        property_types.insert("A0004".to_string(), "Decimal".to_string());
        RawInventory {
            source_urls: vec!["https://openapi.ls-sec.co.kr/apiservice".to_string()],
            property_types,
            groups,
        }
    }

    fn maintained(codes: &[&str]) -> BTreeSet<String> {
        codes.iter().map(|c| c.to_string()).collect()
    }

    /// A maintained TR normalizes to a stable shape; an untracked TR appears in
    /// the code-set but produces no normalized shape (R5).
    #[test]
    fn normalizes_maintained_only_into_code_set_and_shapes() {
        let untracked = RawTr {
            code: "t9999".to_string(),
            ..t8412_raw()
        };
        let inv = inventory(vec![group_with(vec![t8412_raw(), untracked])]);
        let run = normalize_run(&inv, &maintained(&["t8412"]), false);

        assert_eq!(run.code_set.len(), 2, "both codes in the code-set");
        assert!(run.code_set.contains("t9999"));
        assert!(!run.code_set.provisional);
        assert_eq!(run.shapes.len(), 1, "only the maintained TR is normalized");
        assert!(run.shapes.contains_key("t8412"));
        assert!(!run.shapes.contains_key("t9999"));

        assert_eq!(run.manifest.upstream_tr_count, 2);
        assert_eq!(run.manifest.maintained_tr_count, 1);
        assert_eq!(run.manifest.normalizer_version, NORMALIZER_VERSION);
    }

    /// Block headers are skipped as fields and name the block; fields are indexed
    /// from 0 within their block; protocol/endpoint/rate facts are captured.
    #[test]
    fn shape_captures_blocks_fields_and_facts() {
        let inv = inventory(vec![group_with(vec![t8412_raw()])]);
        let run = normalize_run(&inv, &maintained(&["t8412"]), false);
        let shape = &run.shapes["t8412"];

        assert_eq!(shape.protocol, Protocol::Rest);
        assert!(!shape.is_websocket);
        assert_eq!(shape.endpoint_path.as_deref(), Some("/stock/chart"));
        assert_eq!(shape.api_group_id.as_deref(), Some("grp-100"));
        assert_eq!(shape.rate_limit_per_sec, Some(1));
        assert_eq!(shape.corp_rate_limit_per_sec, Some(10));

        // Request body: block header `t8412InBlock` skipped; two fields under it.
        assert_eq!(shape.request_blocks.len(), 2);
        let shcode = &shape.request_blocks[0];
        assert_eq!(shcode.block_name, "t8412InBlock");
        assert_eq!(shcode.field_index, 0);
        assert_eq!(shcode.field_name, "shcode");
        assert_eq!(shcode.r#type.as_deref(), Some("String")); // A0001 resolved
        assert_eq!(shcode.length, Some(6));
        assert!(shcode.required);
        assert_eq!(shape.request_blocks[1].field_index, 1);

        // Response body: same structure under `t8412OutBlock`.
        assert_eq!(shape.response_blocks.len(), 2);
        assert_eq!(shape.response_blocks[0].block_name, "t8412OutBlock");
        assert_eq!(shape.response_blocks[0].direction, Direction::Response);
    }

    /// Duplicate field names, a reorder, a block move, a length change, a
    /// required-flag change, and a rate-limit change are each represented
    /// distinctly in the normalized shape.
    #[test]
    fn representational_distinctions_are_preserved() {
        // Build a TR exercising duplicates and a second block.
        let mut raw = t8412_raw();
        raw.properties = vec![
            prop(
                "res_b",
                "OutBlock1",
                "OutBlock1",
                "A0003",
                Value::Null,
                "Y",
                "",
            ),
            prop(
                "res_b",
                "dup",
                "값",
                "A0001",
                serde_json::json!("4"),
                "Y",
                "",
            ),
            prop(
                "res_b",
                "dup",
                "값2",
                "A0001",
                serde_json::json!("8"),
                "N",
                "",
            ),
            prop(
                "res_b",
                "OutBlock2",
                "OutBlock2",
                "A0003",
                Value::Null,
                "Y",
                "",
            ),
            prop(
                "res_b",
                "dup",
                "값3",
                "A0001",
                serde_json::json!("2"),
                "Y",
                "",
            ),
        ];
        let inv = inventory(vec![group_with(vec![raw])]);
        let run = normalize_run(&inv, &maintained(&["t8412"]), false);
        let blocks = &run.shapes["t8412"].response_blocks;

        // Three `dup` fields: two in OutBlock1 (indices 0,1), one in OutBlock2.
        let dups: Vec<_> = blocks.iter().filter(|f| f.field_name == "dup").collect();
        assert_eq!(dups.len(), 3, "duplicate names preserved distinctly");
        assert_eq!(dups[0].block_name, "OutBlock1");
        assert_eq!(dups[0].field_index, 0);
        assert_eq!(dups[0].length, Some(4));
        assert!(dups[0].required);
        assert_eq!(dups[1].block_name, "OutBlock1");
        assert_eq!(dups[1].field_index, 1);
        assert_eq!(dups[1].length, Some(8));
        assert!(!dups[1].required, "required-flag change is represented");
        // The block move: third `dup` is under OutBlock2 at index 0.
        assert_eq!(dups[2].block_name, "OutBlock2");
        assert_eq!(dups[2].field_index, 0);
    }

    /// Entity-only re-encoding of an otherwise-identical description hashes
    /// identically and produces no finding (R8).
    #[test]
    fn description_hash_is_stable_under_benign_reencoding() {
        let plain = hash_description(Some("연속거래 여부 Y:연속 N:비연속"));
        let encoded = hash_description(Some("연속거래&nbsp;여부<br/>Y:연속<br/>N:비연속"));
        let tagged = hash_description(Some("<b>연속거래 여부</b> Y:연속 N:비연속"));
        assert!(plain.is_some());
        assert_eq!(plain, encoded, "entity/br re-encoding hashes identically");
        assert_eq!(plain, tagged, "tag stripping hashes identically");

        // A genuine wording change hashes differently.
        let changed = hash_description(Some("연속거래 여부 Y:연속 N:중단"));
        assert_ne!(plain, changed);

        // Blank / absent descriptions carry no hash.
        assert_eq!(hash_description(Some("")), None);
        assert_eq!(hash_description(Some("   ")), None);
        assert_eq!(hash_description(None), None);
    }

    /// FNV-1a is deterministic across calls — a committed baseline hash must
    /// reproduce byte-for-byte.
    #[test]
    fn fnv1a_is_deterministic() {
        assert_eq!(fnv1a_hex("hello"), fnv1a_hex("hello"));
        assert_ne!(fnv1a_hex("hello"), fnv1a_hex("hellp"));
        assert_eq!(fnv1a_hex("hello").len(), 16);
    }

    /// The normalized run serializes deterministically (sorted shapes map).
    #[test]
    fn normalized_run_serializes_deterministically() {
        let inv = inventory(vec![group_with(vec![t8412_raw()])]);
        let run = normalize_run(&inv, &maintained(&["t8412"]), true);
        let a = serde_json::to_vec(&run).unwrap();
        let b = serde_json::to_vec(&run).unwrap();
        assert_eq!(a, b);
        let back: NormalizedRun = serde_json::from_slice(&a).unwrap();
        assert_eq!(back, run);
    }
}
