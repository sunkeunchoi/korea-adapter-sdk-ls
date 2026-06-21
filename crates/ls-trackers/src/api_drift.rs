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
    gates_for, AttributeDelta, BlockField, CodeSet, CoverageSummary, Direction, DriftChange,
    DriftFinding, FieldAttribute, Manifest, Protocol, Severity, StagedSnapshot, SupportState,
    TrShape, TrackerFinding,
};

/// The normalizer version recorded in the [`Manifest`] (R8). Bump when the
/// normalization rules change so a normalizer-driven hash shift is auditable.
///
/// v2 (U1): [`ParsedProp::is_block_header`] now also requires a null `length`, so
/// a real field whose compact code equals its Korean label (e.g. `token`'s
/// `scope`, length 256) is no longer dropped as a block delimiter.
pub const NORMALIZER_VERSION: u32 = 2;

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
        // Pure projection: the impure baseline-update path injects the date (R9a).
        refreshed: String::new(),
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
    ///
    /// A genuine delimiter row carries a null `length`; a real field carries a
    /// length. Requiring `length.is_none()` keeps a field whose code happens to
    /// equal its label (e.g. `token`'s `scope`, length 256) from being dropped as
    /// a phantom block header (U1, NORMALIZER_VERSION v2).
    fn is_block_header(&self) -> bool {
        self.length.is_none() && self.korean_name.as_deref().is_some_and(|k| k == self.name)
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

// ---------------------------------------------------------------------------
// U4 — compare a staged run against the committed bounded baselines + code-set
// ---------------------------------------------------------------------------

/// The full output of one comparison (U4): support-aware findings plus the
/// metadata-coverage summary. Coverage is a separate section that **never**
/// affects exit codes (R11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftReport {
    pub findings: Vec<DriftFinding>,
    pub coverage: CoverageSummary,
}

impl DriftReport {
    /// Whether any finding crossed the exit-`1` threshold (R17b). The exit-code
    /// mapping (U5) reads this aggregate of stored `gates` flags — coverage is
    /// not consulted.
    pub fn gates(&self) -> bool {
        self.findings.iter().any(|f| f.gates)
    }
}

/// The support-aware facts-outage decision (U5, R3), kept as a single-sourced
/// pure function distinct from [`gates_for`] (KTD-3). It decides the exit
/// *before* `compare` runs, so degraded facts never turn into spurious
/// Structural API Shape changes.
///
/// Membership joins on TR **code**, not group id: a degraded group's protocol
/// UUID is exactly the field that went missing, so the committed
/// `TrShape.api_group_id` is unusable as a join key (KTD-4a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactsOutage {
    /// No facts dependency degraded — proceed to the normal compare path.
    None,
    /// A degraded facts dependency touches a maintained/baselined TR — exit `2`,
    /// because degraded facts could corrupt a comparison the tracker gates on.
    MaintainedAffected(String),
    /// Degradation is confined to untracked inventory — exit `0` plus a visible
    /// finding (the string describes the degraded dependency).
    UntrackedOnly(String),
}

/// Decide the facts-outage exit (U5). `degraded_tr_codes` are the codes under an
/// endpoint/rate-degraded group (per-group, KTD-4a); `property_type_fallback`
/// marks a whole-inventory mapping outage (KTD-5). `maintained` is the maintained
/// TR-code set; `maintained_present_in_run` is whether any maintained TR appears
/// in the run at all (the whole-inventory branch needs a maintained TR to be at
/// risk before it gates).
pub fn facts_outage_decision(
    degraded_tr_codes: &BTreeSet<String>,
    maintained: &BTreeSet<String>,
    property_type_fallback: bool,
    maintained_present_in_run: bool,
) -> FactsOutage {
    // Property-type fallback is whole-inventory (KTD-5): it substitutes raw type
    // codes for every TR, with no per-group granularity, so it does not route
    // through the untracked-only branch — if any maintained TR is in the run, its
    // fields could diff as false `FieldChanged`.
    if property_type_fallback && maintained_present_in_run {
        return FactsOutage::MaintainedAffected(
            "property-type mapping served the hardcoded fallback (whole-inventory)".to_string(),
        );
    }

    // Endpoint/rate degradation is per-group: gate only when a maintained TR's
    // group is degraded; degradation confined to untracked codes is a notice.
    let maintained_degraded: Vec<&str> = degraded_tr_codes
        .iter()
        .filter(|c| maintained.contains(*c))
        .map(String::as_str)
        .collect();
    if !maintained_degraded.is_empty() {
        return FactsOutage::MaintainedAffected(format!(
            "endpoint/rate facts degraded for maintained TR(s): {}",
            maintained_degraded.join(", ")
        ));
    }
    if !degraded_tr_codes.is_empty() {
        return FactsOutage::UntrackedOnly(format!(
            "endpoint/rate facts degraded for {} untracked TR(s)",
            degraded_tr_codes.len()
        ));
    }

    // Property-type fallback served but no maintained TR present: still a visible
    // notice (the whole inventory degraded), but nothing gated is at risk.
    if property_type_fallback {
        return FactsOutage::UntrackedOnly(
            "property-type mapping served the hardcoded fallback (no maintained TR in run)"
                .to_string(),
        );
    }

    FactsOutage::None
}

/// The decision of the type-only promotion gate (U2): whether the
/// maintained-shape drift in a checked report is admissible for an opt-in
/// `--type-only` promote (R1–R3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeOnlyDecision {
    /// Every maintained finding is a pure-type `FieldChanged` or a
    /// `DescriptionChanged` — the clean type-wave refresh the gate exists to admit.
    Admit,
    /// At least one maintained finding is inadmissible; the string names the first
    /// blocker for the operator's refusal message.
    Block(String),
}

/// The **single source** of the type-only promotion gate rule (U2, R1–R3): the
/// maintained-shape drift is admissible for a `--type-only` promote iff every
/// finding on a maintained TR is either a [`DescriptionChanged`] (benign text
/// noise, admitted by explicit rule) or a [`FieldChanged`] whose every attribute
/// delta is a pure type change. Any other [`DriftChange`] kind — or a
/// `FieldChanged` carrying a length/required component — blocks (R2).
///
/// Untracked-TR findings (including the appended [`FactsDegraded`] notice) never
/// constrain the gate (R1): only `support_state.is_maintained()` findings are
/// considered. The fallback precondition is **not** re-checked here — it is left
/// to the upstream facts-outage gate (R4/KTD3), which exit-2s a fallback-served
/// run with a maintained TR before promote proceeds.
///
/// Pure and total: both the [`DriftChange`] kinds and the [`FieldAttribute`] kinds
/// are matched exhaustively (no wildcard), so a future variant forces a deliberate
/// admit/block choice here rather than slipping through. Mirrors the single-source
/// convention of [`gates_for`](crate::gates_for) / [`facts_outage_decision`].
///
/// [`DescriptionChanged`]: DriftChange::DescriptionChanged
/// [`FieldChanged`]: DriftChange::FieldChanged
/// [`FactsDegraded`]: DriftChange::FactsDegraded
pub fn type_only_gate(findings: &[DriftFinding]) -> TypeOnlyDecision {
    for f in findings {
        // Only maintained-TR drift constrains the gate (R1); untracked drift —
        // including the synthetic `(facts)` FactsDegraded notice — is ignored.
        if !f.support_state.is_maintained() {
            continue;
        }
        let blocker = match &f.change {
            // Benign text noise, admitted by explicit rule (R3/R5). `gates_for`
            // already returns false for it, so it must be admitted here rather
            // than skipped by filter omission.
            DriftChange::DescriptionChanged { .. } => None,
            // Admitted only when every attribute delta is a type change (R2). The
            // attribute kinds are matched exhaustively so a future attribute kind
            // forces a deliberate decision.
            DriftChange::FieldChanged { attributes, .. } => {
                let carries_non_type = attributes.iter().any(|d| match d.attribute {
                    FieldAttribute::Type => false,
                    FieldAttribute::Length | FieldAttribute::Required => true,
                });
                if carries_non_type {
                    Some(format!(
                        "maintained TR `{}` has a field change carrying a non-type component \
                         ({}); only pure-type field changes are admissible",
                        f.tr_code,
                        crate::types::render_attribute_deltas(attributes)
                    ))
                } else {
                    None
                }
            }
            // Every other structural kind blocks a type-only promotion (R4). Listed
            // exhaustively (no wildcard) so a new variant is a compile error here.
            DriftChange::TrAdded
            | DriftChange::TrRemoved
            | DriftChange::FieldAdded { .. }
            | DriftChange::FieldRemoved { .. }
            | DriftChange::FieldReordered { .. }
            | DriftChange::FieldMovedAcrossBlock { .. }
            | DriftChange::EndpointChanged { .. }
            | DriftChange::ProtocolChanged { .. }
            | DriftChange::RateLimitChanged { .. }
            | DriftChange::FactsDegraded { .. } => Some(format!(
                "maintained TR `{}` has a `{}` change, which a type-only promotion does not admit",
                f.tr_code,
                crate::types::change_kind(&f.change)
            )),
        };
        if let Some(reason) = blocker {
            return TypeOnlyDecision::Block(reason);
        }
    }
    TypeOnlyDecision::Admit
}

/// Build the visible, non-gating finding for an untracked-only facts outage
/// ([`FactsOutage::UntrackedOnly`]). `tr_code` is a whole-inventory marker since
/// the degradation is not pinned to a single maintained TR.
pub fn facts_degraded_finding(detail: String) -> DriftFinding {
    DriftFinding {
        tr_code: "(facts)".to_string(),
        change: DriftChange::FactsDegraded { detail },
        severity: Severity::Informational,
        support_state: SupportState::Untracked,
        is_new_tr: false,
        gates: false,
        possible_rename: None,
    }
}

/// Compare a committed bounded baseline against a staged run (U4). Inventory
/// diffs come from code-set comparison (added/removed codes); structural diffs
/// come from per-TR shape comparison over the bounded baseline set. Each change
/// is classified by `ls-metadata` support state and carries a stored `gates`
/// flag per R17b. This function only reads `trs` — it never writes `metadata/`
/// (R10).
pub fn compare(
    committed: &NormalizedRun,
    staged: &NormalizedRun,
    trs: &BTreeMap<String, TrMetadata>,
) -> DriftReport {
    let mut findings = Vec::new();

    // --- Inventory diff (code-set) ----------------------------------------
    // New-TR discovery: a code in the staged inventory but not the reviewed
    // code-set. This is the one untracked event that gates (R9b).
    let mut added_codes = Vec::new();
    let mut removed_codes = Vec::new();
    for code in &staged.code_set.codes {
        if !committed.code_set.contains(code) {
            added_codes.push(code.clone());
            let support = support_state_for(code, trs);
            findings.push(make_finding(
                code,
                DriftChange::TrAdded,
                Severity::Maintenance,
                support,
                /* is_new_tr */ true,
            ));
        }
    }
    // Removal: a code in the reviewed code-set absent from the staged inventory.
    // A maintained/baselined TR's removal gates by support state; an untracked
    // TR's removal is report-only (R12, severity table).
    for code in &committed.code_set.codes {
        if !staged.code_set.contains(code) {
            removed_codes.push(code.clone());
            let support = support_state_for(code, trs);
            let severity = removal_severity(support);
            findings.push(make_finding(
                code,
                DriftChange::TrRemoved,
                severity,
                support,
                false,
            ));
        }
    }

    // --- Structural diff (per-TR shape, bounded baseline set) --------------
    // Diff every TR with a shape on both sides (maintained TRs in production;
    // tests may supply untracked shapes to exercise the classification path).
    for (code, base_shape) in &committed.shapes {
        let Some(cand_shape) = staged.shapes.get(code) else {
            continue; // absence is handled by the code-set removal diff above
        };
        let support = support_state_for(code, trs);
        for change in diff_shapes(base_shape, cand_shape) {
            let severity = change_severity(&change, support);
            findings.push(make_finding(code, change, severity, support, false));
        }
    }

    // --- Rename hook (R14b): co-occurring add + remove get an adjacency note,
    // with no matching logic. Each side references the other side's codes.
    if !added_codes.is_empty() && !removed_codes.is_empty() {
        let added_note = added_codes.join(", ");
        let removed_note = removed_codes.join(", ");
        for f in &mut findings {
            match &f.change {
                DriftChange::TrAdded => f.possible_rename = Some(removed_note.clone()),
                DriftChange::TrRemoved => f.possible_rename = Some(added_note.clone()),
                _ => {}
            }
        }
    }

    // Highest severity first; ties keep insertion (code-set then shape) order.
    findings.sort_by(|a, b| b.severity.cmp(&a.severity));

    let coverage = coverage_summary(&staged.code_set, trs);
    DriftReport { findings, coverage }
}

/// Project a TR's metadata support into a [`SupportState`], or [`Untracked`] when
/// no metadata exists. Shared `pub(crate)` so the Specification Document Tracker
/// ([`crate::spec_doc`]) classifies example findings through the same lookup
/// rather than copying it.
///
/// [`Untracked`]: SupportState::Untracked
pub(crate) fn support_state_for(
    code: &str,
    trs: &BTreeMap<String, TrMetadata>,
) -> SupportState {
    trs.get(code)
        .map(|m| SupportState::from_support(&m.support))
        .unwrap_or(SupportState::Untracked)
}

/// Removal severity by support state (severity table): implemented/recommended →
/// breaking; tracked-only or untracked → maintenance (the untracked case is
/// report-only, decided by [`gates_for`], not by a lower severity).
fn removal_severity(support: SupportState) -> Severity {
    if is_strong(support) {
        Severity::Breaking
    } else {
        Severity::Maintenance
    }
}

fn is_strong(support: SupportState) -> bool {
    matches!(
        support,
        SupportState::Implemented | SupportState::Recommended
    )
}

/// Severity for a structural change, by support state (severity table). Untracked
/// known-TR changes get a non-informational severity where natural, but never
/// gate — gating is decided by [`gates_for`], which reports-only all untracked
/// changes that are not new-TR discoveries.
fn change_severity(change: &DriftChange, support: SupportState) -> Severity {
    let strong = is_strong(support);
    match change {
        DriftChange::TrAdded => Severity::Maintenance,
        DriftChange::TrRemoved => removal_severity(support),
        // Incompatible-tier changes: breaking for the strong surface.
        DriftChange::FieldRemoved { .. }
        | DriftChange::FieldChanged { .. }
        | DriftChange::FieldMovedAcrossBlock { .. }
        | DriftChange::EndpointChanged { .. }
        | DriftChange::ProtocolChanged { .. } => {
            if strong {
                Severity::Breaking
            } else {
                Severity::Maintenance
            }
        }
        // A same-block reorder is maintenance for implemented or tracked.
        DriftChange::FieldReordered { .. } => Severity::Maintenance,
        // A new (optional) field is maintenance for the strong surface,
        // informational otherwise.
        DriftChange::FieldAdded { .. } => {
            if strong {
                Severity::Maintenance
            } else {
                Severity::Informational
            }
        }
        // A rate-limit decrease (or a newly-imposed limit) is maintenance; a
        // relaxation or removal is informational.
        DriftChange::RateLimitChanged { from, to } => {
            if rate_is_more_restrictive(*from, *to) {
                Severity::Maintenance
            } else {
                Severity::Informational
            }
        }
        // Description-only changes are always informational, report-only (R13).
        DriftChange::DescriptionChanged { .. } => Severity::Informational,
        // An untracked-only facts degradation is a visible, non-gating notice.
        DriftChange::FactsDegraded { .. } => Severity::Informational,
    }
}

/// `true` when the candidate rate limit is stricter than the baseline: a smaller
/// positive limit, or a limit newly imposed where there was none.
fn rate_is_more_restrictive(from: Option<u32>, to: Option<u32>) -> bool {
    match (from, to) {
        (Some(a), Some(b)) => b < a,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn make_finding(
    code: &str,
    change: DriftChange,
    severity: Severity,
    support: SupportState,
    is_new_tr: bool,
) -> DriftFinding {
    DriftFinding {
        tr_code: code.to_string(),
        change,
        severity,
        support_state: support,
        is_new_tr,
        gates: gates_for(severity, support, is_new_tr),
        possible_rename: None,
    }
}

/// Diff two Structural API Shapes into structural changes (no severity yet).
/// Top-level facts first (endpoint/protocol/rate/description), then per-field.
///
/// `pub(crate)` (not `pub`) so the change-driven freshness evaluator
/// ([`crate::freshness`]) can diff a frozen attested shape against the current
/// committed baseline shape and filter the result through [`crate::is_qualifying`]
/// (KTD1) — reusing this engine's reorder/move/description reconciliation rather
/// than re-deriving it, without widening the crate's public surface.
pub(crate) fn diff_shapes(base: &TrShape, cand: &TrShape) -> Vec<DriftChange> {
    let mut changes = Vec::new();

    if base.endpoint_path != cand.endpoint_path {
        changes.push(DriftChange::EndpointChanged {
            from: base.endpoint_path.clone(),
            to: cand.endpoint_path.clone(),
        });
    }
    if base.protocol != cand.protocol || base.is_websocket != cand.is_websocket {
        changes.push(DriftChange::ProtocolChanged {
            from: protocol_label(base.protocol, base.is_websocket),
            to: protocol_label(cand.protocol, cand.is_websocket),
        });
    }
    // Retail and corporate limits are diffed independently so a corp-only change
    // is not collapsed into a meaningless `None→None` retail finding and its
    // restrictiveness (which drives severity) is judged on the pair that moved.
    if base.rate_limit_per_sec != cand.rate_limit_per_sec {
        changes.push(DriftChange::RateLimitChanged {
            from: base.rate_limit_per_sec,
            to: cand.rate_limit_per_sec,
        });
    }
    if base.corp_rate_limit_per_sec != cand.corp_rate_limit_per_sec {
        changes.push(DriftChange::RateLimitChanged {
            from: base.corp_rate_limit_per_sec,
            to: cand.corp_rate_limit_per_sec,
        });
    }

    diff_fields(base, cand, &mut changes);

    // A TR-level description change with no structural change is informational.
    if base.description_hash != cand.description_hash {
        changes.push(DriftChange::DescriptionChanged {
            location: "tr".to_string(),
        });
    }

    changes
}

fn protocol_label(protocol: Protocol, is_websocket: bool) -> String {
    if is_websocket {
        "websocket".to_string()
    } else {
        match protocol {
            Protocol::Rest => "rest".to_string(),
            Protocol::Websocket => "websocket".to_string(),
        }
    }
}

/// All fields of a shape, both directions, in stable order.
fn all_fields(shape: &TrShape) -> Vec<&BlockField> {
    shape
        .request_blocks
        .iter()
        .chain(shape.response_blocks.iter())
        .collect()
}

/// Field-level diff with reorder/move reconciliation (R6). Exact identity
/// `(direction, block, index, name)` matches first (so a type/length/required or
/// description change on a stable field is detected); the leftovers are
/// reconciled into same-block reorders and cross-block moves, with the
/// duplicate-name guard falling back to raw add/remove.
fn diff_fields(base: &TrShape, cand: &TrShape, changes: &mut Vec<DriftChange>) {
    type Ident = (Direction, String, u32, String);
    let ident = |f: &BlockField| -> Ident {
        (
            f.direction,
            f.block_name.clone(),
            f.field_index,
            f.field_name.clone(),
        )
    };

    let base_fields = all_fields(base);
    let cand_fields = all_fields(cand);
    let base_by: BTreeMap<Ident, &BlockField> =
        base_fields.iter().map(|f| (ident(f), *f)).collect();
    let cand_by: BTreeMap<Ident, &BlockField> =
        cand_fields.iter().map(|f| (ident(f), *f)).collect();

    // Exact-identity matches → attribute/description changes.
    for (key, bf) in &base_by {
        if let Some(cf) = cand_by.get(key) {
            let attributes = field_attribute_deltas(bf, cf);
            if !attributes.is_empty() {
                changes.push(DriftChange::FieldChanged {
                    direction: bf.direction,
                    block_name: bf.block_name.clone(),
                    field_index: bf.field_index,
                    field_name: bf.field_name.clone(),
                    attributes,
                });
            } else if bf.description_hash != cf.description_hash || bf.korean_name != cf.korean_name
            {
                changes.push(DriftChange::DescriptionChanged {
                    location: format!("{}.{}", bf.block_name, bf.field_name),
                });
            }
        }
    }

    // Leftovers (identity present on only one side) → reconcile.
    let mut removed: Vec<&BlockField> = base_fields
        .iter()
        .filter(|f| !cand_by.contains_key(&ident(f)))
        .copied()
        .collect();
    let mut added: Vec<&BlockField> = cand_fields
        .iter()
        .filter(|f| !base_by.contains_key(&ident(f)))
        .copied()
        .collect();

    reconcile_reorders(
        &mut removed,
        &mut added,
        &base_fields,
        &cand_fields,
        changes,
    );
    reconcile_moves(
        &mut removed,
        &mut added,
        &base_fields,
        &cand_fields,
        changes,
    );

    for f in removed {
        changes.push(DriftChange::FieldRemoved {
            direction: f.direction,
            block_name: f.block_name.clone(),
            field_index: f.field_index,
            field_name: f.field_name.clone(),
        });
    }
    for f in added {
        changes.push(DriftChange::FieldAdded {
            direction: f.direction,
            block_name: f.block_name.clone(),
            field_index: f.field_index,
            field_name: f.field_name.clone(),
        });
    }
}

/// The type / length / required changes on an identity-stable field, as
/// structured [`AttributeDelta`]s in the canonical type→length→required order.
/// Empty when those attributes are unchanged. The structured form is the single
/// source the gate classifies on; the legacy human-readable detail is its derived
/// view via [`render_attribute_deltas`] (KTD1).
fn field_attribute_deltas(base: &BlockField, cand: &BlockField) -> Vec<AttributeDelta> {
    let mut deltas = Vec::new();
    if base.r#type != cand.r#type {
        deltas.push(AttributeDelta {
            attribute: FieldAttribute::Type,
            from: base.r#type.as_deref().unwrap_or("?").to_string(),
            to: cand.r#type.as_deref().unwrap_or("?").to_string(),
        });
    }
    if base.length != cand.length {
        deltas.push(AttributeDelta {
            attribute: FieldAttribute::Length,
            from: opt_u32(base.length),
            to: opt_u32(cand.length),
        });
    }
    if base.required != cand.required {
        deltas.push(AttributeDelta {
            attribute: FieldAttribute::Required,
            from: base.required.to_string(),
            to: cand.required.to_string(),
        });
    }
    deltas
}

fn opt_u32(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string())
}

/// Same-block reorder reconciliation (R6): group leftover removed/added by
/// `(direction, block_name, field_name)`. A group whose **full** multiplicity is
/// exactly one on each side (measured over all fields, not just leftovers), with
/// differing indices, is one reorder. A group with multiplicity > 1 on either
/// side (duplicate names) is left untouched → raw add/remove.
fn reconcile_reorders(
    removed: &mut Vec<&BlockField>,
    added: &mut Vec<&BlockField>,
    base_fields: &[&BlockField],
    cand_fields: &[&BlockField],
    changes: &mut Vec<DriftChange>,
) {
    reconcile_pairs(
        removed,
        added,
        base_fields,
        cand_fields,
        // Group by (direction, block, name): a reorder stays within its block.
        |f| (f.direction, f.block_name.clone(), f.field_name.clone()),
        // A pair is a reorder only when the index actually shifted.
        |rf, af| rf.field_index != af.field_index,
        |rf, af, changes| {
            changes.push(DriftChange::FieldReordered {
                direction: rf.direction,
                block_name: rf.block_name.clone(),
                field_name: rf.field_name.clone(),
                from_index: rf.field_index,
                to_index: af.field_index,
            });
            // A field that both moved and changed type/length/required would
            // otherwise miss exact-identity matching (its index differs); emit
            // the attribute change too so an incompatible change is not
            // understated to a bare reorder.
            let attributes = field_attribute_deltas(rf, af);
            if !attributes.is_empty() {
                changes.push(DriftChange::FieldChanged {
                    direction: af.direction,
                    block_name: af.block_name.clone(),
                    field_index: af.field_index,
                    field_name: af.field_name.clone(),
                    attributes,
                });
            }
        },
        changes,
    );
}

/// Cross-block move reconciliation: among the remaining leftovers, group by
/// `(direction, field_name)`. A group whose full multiplicity is one on each
/// side, in different blocks, is a move; duplicates fall back to raw add/remove.
fn reconcile_moves(
    removed: &mut Vec<&BlockField>,
    added: &mut Vec<&BlockField>,
    base_fields: &[&BlockField],
    cand_fields: &[&BlockField],
    changes: &mut Vec<DriftChange>,
) {
    reconcile_pairs(
        removed,
        added,
        base_fields,
        cand_fields,
        // Group by (direction, name): a move spans blocks, so block is excluded.
        |f| (f.direction, f.field_name.clone()),
        // A pair is a move only when it changed block.
        |rf, af| rf.block_name != af.block_name,
        |rf, af, changes| {
            changes.push(DriftChange::FieldMovedAcrossBlock {
                direction: rf.direction,
                field_name: rf.field_name.clone(),
                from_block: rf.block_name.clone(),
                to_block: af.block_name.clone(),
            });
        },
        changes,
    );
}

/// Shared 1:1 leftover-reconciliation skeleton for both reorder and move passes.
/// Groups leftovers by `key`; a group with full multiplicity 1 on each side
/// (the duplicate-name guard, measured over `base_fields`/`cand_fields`, R6) and
/// a single removed+added leftover that satisfies `is_match` is reconciled via
/// `emit` and dropped from the raw add/remove leftovers. Groups that fail the
/// guard or `is_match` fall through to raw add/remove.
fn reconcile_pairs<K: Ord>(
    removed: &mut Vec<&BlockField>,
    added: &mut Vec<&BlockField>,
    base_fields: &[&BlockField],
    cand_fields: &[&BlockField],
    key: impl Fn(&BlockField) -> K,
    is_match: impl Fn(&BlockField, &BlockField) -> bool,
    emit: impl Fn(&BlockField, &BlockField, &mut Vec<DriftChange>),
    changes: &mut Vec<DriftChange>,
) {
    let mut reconciled_removed = Vec::new();
    let mut reconciled_added = Vec::new();

    let groups: BTreeSet<_> = removed.iter().map(|f| key(f)).collect();
    for g in groups {
        // Duplicate-name guard: never positionally match a group with more than
        // one member on either side in the full field set (R6).
        let base_mult = base_fields.iter().filter(|f| key(f) == g).count();
        let cand_mult = cand_fields.iter().filter(|f| key(f) == g).count();
        if base_mult > 1 || cand_mult > 1 {
            continue;
        }
        let r = indices_matching(removed, &g, &key);
        let a = indices_matching(added, &g, &key);
        if r.len() == 1 && a.len() == 1 {
            let rf = removed[r[0]];
            let af = added[a[0]];
            if is_match(rf, af) {
                emit(rf, af, changes);
                reconciled_removed.push(r[0]);
                reconciled_added.push(a[0]);
            }
        }
    }
    retain_except(removed, &reconciled_removed);
    retain_except(added, &reconciled_added);
}

/// Positions in `fields` whose grouping key equals `g`.
fn indices_matching<K: PartialEq>(
    fields: &[&BlockField],
    g: &K,
    key: &impl Fn(&BlockField) -> K,
) -> Vec<usize> {
    fields
        .iter()
        .enumerate()
        .filter(|(_, f)| key(f) == *g)
        .map(|(i, _)| i)
        .collect()
}

/// Drop the elements at `drop_indices` from `v` (order-preserving).
fn retain_except<T>(v: &mut Vec<T>, drop_indices: &[usize]) {
    let drop: BTreeSet<usize> = drop_indices.iter().copied().collect();
    let mut i = 0;
    v.retain(|_| {
        let keep = !drop.contains(&i);
        i += 1;
        keep
    });
}

/// Metadata-coverage summary (R11), driven by the code-set, not structural
/// shape. Never affects exit codes.
fn coverage_summary(staged: &CodeSet, trs: &BTreeMap<String, TrMetadata>) -> CoverageSummary {
    let mut implemented_count = 0;
    let mut tracked_only_count = 0;
    let mut metadata_missing_upstream = Vec::new();
    for (code, meta) in trs {
        let s = SupportState::from_support(&meta.support);
        match s {
            SupportState::Implemented => implemented_count += 1,
            SupportState::Tracked => tracked_only_count += 1,
            _ => {}
        }
        if !staged.contains(code) {
            metadata_missing_upstream.push(code.clone());
        }
    }
    let upstream_missing_metadata = staged
        .codes
        .iter()
        .filter(|c| !trs.contains_key(*c))
        .cloned()
        .collect();

    CoverageSummary {
        upstream_count: staged.len(),
        metadata_count: trs.len(),
        implemented_count,
        tracked_only_count,
        metadata_missing_upstream,
        upstream_missing_metadata,
    }
}

/// FNV-1a 64-bit, rendered as zero-padded hex. Deterministic across machines and
/// Rust versions (unlike `DefaultHasher`), which is what a *committed* baseline
/// hash needs. A collision would at worst miss an informational description
/// change — never a gating finding.
fn fnv1a_hex(s: &str) -> String {
    fnv1a_hex_bytes(s.as_bytes())
}

/// FNV-1a 64-bit over raw bytes (the byte-level core of [`fnv1a_hex`]), shared by
/// the whole-raw digest below so both use the one deterministic-hash convention.
pub(crate) fn fnv1a_hex_bytes(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

/// The whole-raw snapshot digest (R14): FNV-1a-hex over the **on-disk bytes** of a
/// staged `raw/ls-openapi-full.json`. Hashes file content rather than a
/// re-serialization of [`RawInventory`] — whose `groups`/`trs` are insertion-ordered
/// `Vec`s, so re-serializing would be order-sensitive — keeping the digest stable
/// regardless of in-memory ordering and equal to the bytes a promote writes
/// (KTD4). Distinct from the per-field `description_hash` and the hand-authored
/// `maintenance.source_spec_hash`; an audit/integrity digest, not a security
/// control.
pub(crate) fn whole_raw_hash(raw_bytes: &[u8]) -> String {
    fnv1a_hex_bytes(raw_bytes)
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

    /// A `BlockField` with the given type / length / required, for exercising
    /// [`field_attribute_deltas`] directly.
    fn block_field(ty: Option<&str>, length: Option<u32>, required: bool) -> BlockField {
        BlockField {
            direction: Direction::Response,
            block_name: "b".to_string(),
            field_index: 0,
            field_name: "f".to_string(),
            korean_name: None,
            r#type: ty.map(str::to_string),
            length,
            required,
            description_hash: None,
        }
    }

    /// U1 happy path: a field whose only change is its type yields a single
    /// type-attribute delta and nothing else, rendering byte-identically to the
    /// legacy `type X→Y` detail.
    #[test]
    fn field_attribute_deltas_marks_type_only() {
        let deltas = field_attribute_deltas(
            &block_field(Some("String"), Some(6), true),
            &block_field(Some("Long"), Some(6), true),
        );
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].attribute, FieldAttribute::Type);
        assert_eq!(
            crate::types::render_attribute_deltas(&deltas),
            "type String→Long"
        );
    }

    /// U1 edge: type and length both change → both attributes, in canonical
    /// type→length order, rendering `type X→Y, length A→B` unchanged.
    #[test]
    fn field_attribute_deltas_marks_type_and_length() {
        let deltas = field_attribute_deltas(
            &block_field(Some("String"), Some(4), true),
            &block_field(Some("Long"), Some(8), true),
        );
        assert_eq!(
            deltas.iter().map(|d| d.attribute).collect::<Vec<_>>(),
            vec![FieldAttribute::Type, FieldAttribute::Length]
        );
        assert_eq!(
            crate::types::render_attribute_deltas(&deltas),
            "type String→Long, length 4→8"
        );
    }

    /// U1 edge: a required-flag flip with the type unchanged marks `required`
    /// only — the discriminator the type-only gate (U2) blocks on.
    #[test]
    fn field_attribute_deltas_marks_required_only() {
        let deltas = field_attribute_deltas(
            &block_field(Some("String"), Some(6), true),
            &block_field(Some("String"), Some(6), false),
        );
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].attribute, FieldAttribute::Required);
        assert_eq!(
            crate::types::render_attribute_deltas(&deltas),
            "required true→false"
        );
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

    /// A body row whose compact code equals its Korean label but carries a
    /// non-null length is a real field, not a block delimiter (U1, R-1). The
    /// field that follows it is filed under the surrounding block, not under a
    /// phantom block named after it (the `token_type` mis-filing regression).
    #[test]
    fn same_code_label_field_with_length_is_a_field_not_a_block_header() {
        // Mirrors the real `token` response body: a normal field, then `scope`
        // (code == label, length 256), then `token_type` following it.
        let mut raw = t8412_raw();
        raw.properties = vec![
            prop(
                "res_b",
                "access_token",
                "접근토큰",
                "A0001",
                serde_json::json!("1000"),
                "Y",
                "",
            ),
            // code == label, but a real field (length 256) — must not be dropped.
            prop("res_b", "scope", "scope", "A0001", serde_json::json!("256"), "Y", ""),
            prop(
                "res_b",
                "token_type",
                "토큰 유형",
                "A0001",
                serde_json::json!("256"),
                "Y",
                "",
            ),
        ];
        let inv = inventory(vec![group_with(vec![raw])]);
        let run = normalize_run(&inv, &maintained(&["t8412"]), false);
        let blocks = &run.shapes["t8412"].response_blocks;

        // `scope` survives as a real field under the default response body block.
        let scope = blocks
            .iter()
            .find(|f| f.field_name == "scope")
            .expect("scope is a real field, not dropped");
        assert_eq!(scope.block_name, "response_body");
        assert_eq!(scope.r#type.as_deref(), Some("String"));
        assert_eq!(scope.length, Some(256));

        // `token_type` is filed under the surrounding block, not a phantom
        // `scope` block (the pre-v2 mis-filing this fix closes).
        let token_type = blocks
            .iter()
            .find(|f| f.field_name == "token_type")
            .expect("token_type present");
        assert_eq!(token_type.block_name, "response_body");

        // All three sit in one block, indexed in order: no phantom block split.
        assert!(blocks.iter().all(|f| f.block_name == "response_body"));
        assert_eq!(blocks.len(), 3);
    }

    /// A body row whose compact code equals its Korean label AND carries a null
    /// length is still treated as a block-header delimiter (existing behavior
    /// preserved): it names the block and is not emitted as a field.
    #[test]
    fn same_code_label_row_with_null_length_is_still_a_block_header() {
        let mut raw = t8412_raw();
        raw.properties = vec![
            // Null length → a genuine delimiter, as before v2.
            prop("res_b", "OutBlock", "OutBlock", "A0003", Value::Null, "Y", ""),
            prop("res_b", "price", "현재가", "A0004", serde_json::json!("8"), "Y", ""),
        ];
        let inv = inventory(vec![group_with(vec![raw])]);
        let run = normalize_run(&inv, &maintained(&["t8412"]), false);
        let blocks = &run.shapes["t8412"].response_blocks;

        // The delimiter is not emitted as a field; `price` sits under it.
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].field_name, "price");
        assert_eq!(blocks[0].block_name, "OutBlock");
        assert_eq!(blocks[0].field_index, 0);
    }

    /// The v2 rule rests on the invariant that a genuine delimiter carries a null
    /// length: a code==label row with ANY parseable length — including a numeric
    /// `0` and a numeric (not string) JSON value — is a real field. Guards the
    /// boundary the block-header heuristic depends on; if upstream ever gave a
    /// real delimiter a numeric length this test would surface the reclassification.
    #[test]
    fn same_code_label_row_with_numeric_length_is_a_field() {
        let mut raw = t8412_raw();
        raw.properties = vec![
            prop("res_b", "OB", "OB", "A0003", Value::Null, "Y", ""),
            // code == label, numeric JSON length 0 → a real field, not a delimiter.
            prop("res_b", "flag", "flag", "A0001", serde_json::json!(0), "Y", ""),
        ];
        let inv = inventory(vec![group_with(vec![raw])]);
        let run = normalize_run(&inv, &maintained(&["t8412"]), false);
        let blocks = &run.shapes["t8412"].response_blocks;
        let flag = blocks
            .iter()
            .find(|f| f.field_name == "flag")
            .expect("a numeric-length code==label row is a field");
        assert_eq!(flag.block_name, "OB", "filed under the surrounding block");
        assert_eq!(flag.length, Some(0));
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

    /// U3 (R14): the whole-raw digest hashes the on-disk file bytes — deterministic,
    /// sensitive to a single byte, and taken from content (not a struct
    /// re-serialization), so a reordered in-memory `RawInventory` that serializes
    /// the same bytes still hashes the same. It is also distinct from a per-field
    /// `description_hash`, documenting that the two hash kinds never collide by
    /// construction of their inputs.
    #[test]
    fn whole_raw_hash_is_content_stable_and_distinct() {
        let bytes = br#"{"source_urls":[],"property_types":{},"groups":[]}"#;
        // Deterministic over the same bytes.
        assert_eq!(whole_raw_hash(bytes), whole_raw_hash(bytes));
        assert_eq!(whole_raw_hash(bytes).len(), 16);

        // Sensitive to a one-byte change.
        let mutated = br#"{"source_urls":[],"property_types":{},"groups":[ ]}"#;
        assert_ne!(whole_raw_hash(bytes), whole_raw_hash(mutated));

        // Content-stable: the hash is over the literal bytes handed in, so two
        // byte-identical files hash identically regardless of how an in-memory
        // `RawInventory` would have ordered its `Vec`s on re-serialization.
        let same_content = br#"{"source_urls":[],"property_types":{},"groups":[]}"#;
        assert_eq!(whole_raw_hash(bytes), whole_raw_hash(same_content));

        // Distinct from a per-field description hash for the same logical TR — the
        // two digests have different inputs (whole file vs. one normalized
        // description), so conflating them is impossible.
        let desc = hash_description(Some("groups")).unwrap();
        assert_ne!(whole_raw_hash(bytes), desc);
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

    // --- U4 compare (structural detection + reconciliation) ----------------

    fn raw_tr_with(code: &str, props: Vec<Value>) -> RawTr {
        RawTr {
            code: code.to_string(),
            name: Some(code.to_string()),
            is_websocket: false,
            http_method: Some("POST".to_string()),
            url: Some(format!("/{code}")),
            protocol_type: Some("REST".to_string()),
            rate_limit_per_sec: Some(1),
            corp_rate_limit_per_sec: None,
            description: None,
            properties: props,
            req_example: Value::Null,
            res_example: Value::Null,
        }
    }

    fn run_of(code: &str, props: Vec<Value>) -> NormalizedRun {
        let inv = inventory(vec![group_with(vec![raw_tr_with(code, props)])]);
        normalize_run(&inv, &maintained(&[code]), false)
    }

    fn no_meta() -> BTreeMap<String, TrMetadata> {
        BTreeMap::new()
    }

    fn rb(name: &str, ty: &str, len: Value) -> Value {
        prop("res_b", name, name_korean(name), ty, len, "Y", "")
    }

    // A distinct Korean label so real fields are never mistaken for block heads.
    fn name_korean(name: &str) -> &str {
        match name {
            "OB" => "OB",
            _ => "라벨",
        }
    }

    #[test]
    fn compare_identical_runs_yields_no_findings() {
        let props = vec![
            rb("OB", "A0003", Value::Null),
            rb("a", "A0001", serde_json::json!("4")),
        ];
        let committed = run_of("t8412", props.clone());
        let staged = run_of("t8412", props);
        let report = compare(&committed, &staged, &no_meta());
        assert!(report.findings.is_empty(), "got: {:?}", report.findings);
        assert!(!report.gates());
    }

    #[test]
    fn reorder_with_unique_names_reconciles_to_reorder_findings() {
        let base = run_of(
            "t8412",
            vec![
                rb("OB", "A0003", Value::Null),
                rb("a", "A0001", serde_json::json!("4")),
                rb("b", "A0001", serde_json::json!("4")),
            ],
        );
        // Same fields, swapped order → indices shift, names unique.
        let staged = run_of(
            "t8412",
            vec![
                rb("OB", "A0003", Value::Null),
                rb("b", "A0001", serde_json::json!("4")),
                rb("a", "A0001", serde_json::json!("4")),
            ],
        );
        let report = compare(&base, &staged, &no_meta());
        let reorders = report
            .findings
            .iter()
            .filter(|f| matches!(f.change, DriftChange::FieldReordered { .. }))
            .count();
        assert_eq!(
            reorders, 2,
            "two unique-name reorders, got: {:?}",
            report.findings
        );
        assert!(
            !report.findings.iter().any(|f| matches!(
                f.change,
                DriftChange::FieldAdded { .. } | DriftChange::FieldRemoved { .. }
            )),
            "a reconciled reorder emits no raw add/remove"
        );
    }

    #[test]
    fn duplicate_name_shift_falls_back_to_raw_add_remove() {
        let base = run_of(
            "t8412",
            vec![
                rb("OB", "A0003", Value::Null),
                rb("dup", "A0001", serde_json::json!("4")),
                rb("dup", "A0001", serde_json::json!("4")),
            ],
        );
        // A new field pushes the second `dup` to a higher index.
        let staged = run_of(
            "t8412",
            vec![
                rb("OB", "A0003", Value::Null),
                rb("dup", "A0001", serde_json::json!("4")),
                rb("other", "A0001", serde_json::json!("4")),
                rb("dup", "A0001", serde_json::json!("4")),
            ],
        );
        let report = compare(&base, &staged, &no_meta());
        assert!(
            !report
                .findings
                .iter()
                .any(|f| matches!(f.change, DriftChange::FieldReordered { .. })),
            "a duplicate-name group is not positionally matched"
        );
        assert!(report
            .findings
            .iter()
            .any(|f| matches!(&f.change, DriftChange::FieldRemoved { field_name, .. } if field_name == "dup")));
        assert!(report
            .findings
            .iter()
            .any(|f| matches!(&f.change, DriftChange::FieldAdded { field_name, .. } if field_name == "dup")));
    }

    #[test]
    fn field_moved_across_block_reconciles_to_a_move() {
        // Block headers (name == korean) delimit OB1 / OB2; `moved` migrates
        // from OB1 into OB2.
        let bh = |n: &str| prop("res_b", n, n, "A0003", Value::Null, "Y", "");
        let base = run_of(
            "t8412",
            vec![
                bh("OB1"),
                rb("moved", "A0001", serde_json::json!("4")),
                bh("OB2"),
                rb("stay", "A0001", serde_json::json!("4")),
            ],
        );
        let staged = run_of(
            "t8412",
            vec![
                bh("OB1"),
                bh("OB2"),
                rb("moved", "A0001", serde_json::json!("4")),
                rb("stay", "A0001", serde_json::json!("4")),
            ],
        );
        let report = compare(&base, &staged, &no_meta());
        assert!(
            report.findings.iter().any(|f| matches!(
                &f.change,
                DriftChange::FieldMovedAcrossBlock { field_name, from_block, to_block, .. }
                    if field_name == "moved" && from_block == "OB1" && to_block == "OB2"
            )),
            "got: {:?}",
            report.findings
        );
    }

    #[test]
    fn new_untracked_tr_discovery_gates() {
        let committed = run_of("t8412", vec![rb("OB", "A0003", Value::Null)]);
        // Staged inventory adds a brand-new code not in the reviewed code-set.
        let mut inv = inventory(vec![group_with(vec![
            raw_tr_with("t8412", vec![rb("OB", "A0003", Value::Null)]),
            raw_tr_with("t9999", vec![]),
        ])]);
        inv.property_types.clear();
        let staged = normalize_run(&inv, &maintained(&["t8412"]), false);

        let report = compare(&committed, &staged, &no_meta());
        let new_tr = report
            .findings
            .iter()
            .find(|f| f.tr_code == "t9999")
            .expect("new TR finding");
        assert!(matches!(new_tr.change, DriftChange::TrAdded));
        assert!(new_tr.is_new_tr);
        assert!(new_tr.gates, "new-TR discovery gates (R9b)");
        assert!(report.gates());
    }

    #[test]
    fn rename_hook_annotates_cooccurring_add_and_remove() {
        // committed has old_tr; staged drops it and introduces new_tr.
        let committed = {
            let inv = inventory(vec![group_with(vec![raw_tr_with("old_tr", vec![])])]);
            normalize_run(&inv, &maintained(&[]), false)
        };
        let staged = {
            let inv = inventory(vec![group_with(vec![raw_tr_with("new_tr", vec![])])]);
            normalize_run(&inv, &maintained(&[]), false)
        };
        let report = compare(&committed, &staged, &no_meta());
        let added = report
            .findings
            .iter()
            .find(|f| f.tr_code == "new_tr")
            .unwrap();
        let removed = report
            .findings
            .iter()
            .find(|f| f.tr_code == "old_tr")
            .unwrap();
        assert_eq!(added.possible_rename.as_deref(), Some("old_tr"));
        assert_eq!(removed.possible_rename.as_deref(), Some("new_tr"));
        // Both underlying findings are preserved.
        assert!(matches!(added.change, DriftChange::TrAdded));
        assert!(matches!(removed.change, DriftChange::TrRemoved));
    }

    #[test]
    fn corp_only_rate_limit_change_emits_a_real_finding_not_none_to_none() {
        // Retail rate unchanged (Some(1)); corp rate tightens 10 -> 5.
        let mut base = run_of("t8412", vec![rb("OB", "A0003", Value::Null)]);
        let mut staged = base.clone();
        base.shapes
            .get_mut("t8412")
            .unwrap()
            .corp_rate_limit_per_sec = Some(10);
        staged
            .shapes
            .get_mut("t8412")
            .unwrap()
            .corp_rate_limit_per_sec = Some(5);

        let report = compare(&base, &staged, &no_meta());
        let rate: Vec<_> = report
            .findings
            .iter()
            .filter(|f| matches!(f.change, DriftChange::RateLimitChanged { .. }))
            .collect();
        assert_eq!(
            rate.len(),
            1,
            "exactly the corp change, no spurious retail finding"
        );
        // The corp pair (10 -> 5) carries the real values, not None -> None.
        assert!(matches!(
            rate[0].change,
            DriftChange::RateLimitChanged {
                from: Some(10),
                to: Some(5)
            }
        ));
        // A tightening (more restrictive) is maintenance, not informational.
        assert_eq!(rate[0].severity, Severity::Maintenance);
    }

    #[test]
    fn endpoint_and_protocol_changes_are_detected() {
        let base = run_of("t8412", vec![rb("OB", "A0003", Value::Null)]);
        let mut staged = base.clone();
        {
            let s = staged.shapes.get_mut("t8412").unwrap();
            s.endpoint_path = Some("/v2/stock/chart".to_string());
            s.protocol = Protocol::Websocket;
            s.is_websocket = true;
        }
        let report = compare(&base, &staged, &no_meta());
        assert!(report.findings.iter().any(|f| matches!(
            &f.change,
            DriftChange::EndpointChanged { to, .. } if to.as_deref() == Some("/v2/stock/chart")
        )));
        assert!(report.findings.iter().any(|f| matches!(
            &f.change,
            DriftChange::ProtocolChanged { from, to } if from == "rest" && to == "websocket"
        )));
    }

    #[test]
    fn rate_limit_relaxation_is_informational_and_does_not_gate() {
        // A loosened retail limit (5 -> 10) and a removed corp limit (5 -> None)
        // are both relaxations → informational, report-only.
        let mut base = run_of("t8412", vec![rb("OB", "A0003", Value::Null)]);
        let mut staged = base.clone();
        {
            let b = base.shapes.get_mut("t8412").unwrap();
            b.rate_limit_per_sec = Some(5);
            b.corp_rate_limit_per_sec = Some(5);
        }
        {
            let s = staged.shapes.get_mut("t8412").unwrap();
            s.rate_limit_per_sec = Some(10);
            s.corp_rate_limit_per_sec = None;
        }
        let report = compare(&base, &staged, &no_meta());
        let rate: Vec<_> = report
            .findings
            .iter()
            .filter(|f| matches!(f.change, DriftChange::RateLimitChanged { .. }))
            .collect();
        assert_eq!(rate.len(), 2, "retail loosen + corp removal");
        assert!(
            rate.iter().all(|f| f.severity == Severity::Informational),
            "a relaxation is informational"
        );
        assert!(!report.gates(), "a rate relaxation does not gate");
    }

    #[test]
    fn reorder_plus_attribute_change_emits_both_a_reorder_and_a_field_change() {
        let base = run_of(
            "t8412",
            vec![
                prop("res_b", "OB", "OB", "A0003", Value::Null, "Y", ""),
                prop(
                    "res_b",
                    "a",
                    "라벨",
                    "A0001",
                    serde_json::json!("4"),
                    "Y",
                    "",
                ),
                prop(
                    "res_b",
                    "b",
                    "라벨",
                    "A0001",
                    serde_json::json!("4"),
                    "Y",
                    "",
                ),
            ],
        );
        // `a` moves to index 1 AND changes length 4 -> 8.
        let staged = run_of(
            "t8412",
            vec![
                prop("res_b", "OB", "OB", "A0003", Value::Null, "Y", ""),
                prop(
                    "res_b",
                    "b",
                    "라벨",
                    "A0001",
                    serde_json::json!("4"),
                    "Y",
                    "",
                ),
                prop(
                    "res_b",
                    "a",
                    "라벨",
                    "A0001",
                    serde_json::json!("8"),
                    "Y",
                    "",
                ),
            ],
        );
        let report = compare(&base, &staged, &no_meta());
        assert!(
            report.findings.iter().any(|f| matches!(
                &f.change,
                DriftChange::FieldReordered { field_name, .. } if field_name == "a"
            )),
            "the reorder is still reported"
        );
        assert!(
            report.findings.iter().any(|f| matches!(
                &f.change,
                DriftChange::FieldChanged { field_name, attributes, .. }
                    if field_name == "a"
                        && crate::types::render_attribute_deltas(attributes).contains("length 4→8")
            )),
            "the attribute change is not swallowed by the reorder: {:?}",
            report.findings
        );
    }

    // --- U2 type-only promotion gate (pure, single-sourced) ----------------

    fn gate_finding(tr: &str, change: DriftChange, support: SupportState) -> DriftFinding {
        DriftFinding {
            tr_code: tr.to_string(),
            change,
            // Severity/gates are irrelevant to the type-only gate (it reads
            // structure, not severity); fixed here to a plausible value.
            severity: Severity::Maintenance,
            support_state: support,
            is_new_tr: false,
            gates: false,
            possible_rename: None,
        }
    }

    fn type_delta() -> Vec<AttributeDelta> {
        vec![AttributeDelta {
            attribute: FieldAttribute::Type,
            from: "String".into(),
            to: "Long".into(),
        }]
    }

    fn pure_type_change(tr: &str, support: SupportState) -> DriftFinding {
        gate_finding(
            tr,
            DriftChange::FieldChanged {
                direction: Direction::Response,
                block_name: "b".into(),
                field_index: 0,
                field_name: "f".into(),
                attributes: type_delta(),
            },
            support,
        )
    }

    fn description_change(tr: &str, support: SupportState) -> DriftFinding {
        gate_finding(
            tr,
            DriftChange::DescriptionChanged {
                location: "tr".into(),
            },
            support,
        )
    }

    /// Happy path: pure-type `FieldChanged` across maintained TRs plus a
    /// `DescriptionChanged` is admitted (R1, R3).
    #[test]
    fn type_only_gate_admits_pure_type_wave_plus_description() {
        let findings = vec![
            pure_type_change("t1481", SupportState::Implemented),
            pure_type_change("t8430", SupportState::Tracked),
            description_change("t1481", SupportState::Implemented),
        ];
        assert_eq!(type_only_gate(&findings), TypeOnlyDecision::Admit);
    }

    /// Edge: zero maintained drift admits — a clean fetch still resolves
    /// provisionality even with no field-type drift (origin F4).
    #[test]
    fn type_only_gate_admits_empty_drift() {
        assert_eq!(type_only_gate(&[]), TypeOnlyDecision::Admit);
    }

    /// Edge: a maintained `DescriptionChanged` alone is admitted by the explicit
    /// rule, not by filter omission.
    #[test]
    fn type_only_gate_admits_maintained_description_alone() {
        let findings = vec![description_change("t1481", SupportState::Recommended)];
        assert_eq!(type_only_gate(&findings), TypeOnlyDecision::Admit);
    }

    /// Edge: an untracked-TR `FieldAdded` co-present with pure-type maintained
    /// drift is admitted — untracked drift never constrains the gate (R1).
    #[test]
    fn type_only_gate_ignores_untracked_structural_drift() {
        let findings = vec![
            pure_type_change("t1481", SupportState::Implemented),
            gate_finding(
                "UNTRACKED",
                DriftChange::FieldAdded {
                    direction: Direction::Request,
                    block_name: "b".into(),
                    field_index: 0,
                    field_name: "new".into(),
                },
                SupportState::Untracked,
            ),
        ];
        assert_eq!(type_only_gate(&findings), TypeOnlyDecision::Admit);
    }

    /// Every non-(type FieldChanged | DescriptionChanged) `DriftChange` kind on a
    /// maintained TR blocks (R4). Each kind is asserted explicitly so the gate's
    /// admit/block decision is pinned per variant.
    #[test]
    fn type_only_gate_blocks_every_structural_kind_on_maintained_tr() {
        let dir = Direction::Response;
        let blocking: Vec<DriftChange> = vec![
            DriftChange::TrAdded,
            DriftChange::TrRemoved,
            DriftChange::FieldAdded {
                direction: dir,
                block_name: "b".into(),
                field_index: 0,
                field_name: "f".into(),
            },
            DriftChange::FieldRemoved {
                direction: dir,
                block_name: "b".into(),
                field_index: 0,
                field_name: "f".into(),
            },
            DriftChange::FieldReordered {
                direction: dir,
                block_name: "b".into(),
                field_name: "f".into(),
                from_index: 0,
                to_index: 1,
            },
            DriftChange::FieldMovedAcrossBlock {
                direction: dir,
                field_name: "f".into(),
                from_block: "a".into(),
                to_block: "b".into(),
            },
            DriftChange::EndpointChanged {
                from: Some("/a".into()),
                to: Some("/b".into()),
            },
            DriftChange::ProtocolChanged {
                from: "rest".into(),
                to: "websocket".into(),
            },
            DriftChange::RateLimitChanged {
                from: Some(1),
                to: Some(2),
            },
            DriftChange::FactsDegraded {
                detail: "x".into(),
            },
        ];
        for change in blocking {
            let label = crate::types::change_kind(&change).to_string();
            let findings = vec![gate_finding("t1481", change, SupportState::Implemented)];
            assert!(
                matches!(type_only_gate(&findings), TypeOnlyDecision::Block(r) if r.contains(&label)),
                "expected `{label}` on a maintained TR to block"
            );
        }
    }

    /// Block: a `FieldChanged` whose detail is a required-flag change blocks —
    /// required is not benign noise (AE4).
    #[test]
    fn type_only_gate_blocks_required_flag_field_change() {
        let findings = vec![gate_finding(
            "t1481",
            DriftChange::FieldChanged {
                direction: Direction::Response,
                block_name: "b".into(),
                field_index: 0,
                field_name: "f".into(),
                attributes: vec![AttributeDelta {
                    attribute: FieldAttribute::Required,
                    from: "true".into(),
                    to: "false".into(),
                }],
            },
            SupportState::Implemented,
        )];
        assert!(matches!(
            type_only_gate(&findings),
            TypeOnlyDecision::Block(_)
        ));
    }

    /// Block: a `FieldChanged` whose detail is a length change blocks — a length
    /// change is a semantic contract change, not fallback cleanup (AE4).
    #[test]
    fn type_only_gate_blocks_length_field_change() {
        let findings = vec![gate_finding(
            "t1481",
            DriftChange::FieldChanged {
                direction: Direction::Response,
                block_name: "b".into(),
                field_index: 0,
                field_name: "f".into(),
                attributes: vec![AttributeDelta {
                    attribute: FieldAttribute::Length,
                    from: "4".into(),
                    to: "8".into(),
                }],
            },
            SupportState::Implemented,
        )];
        assert!(matches!(
            type_only_gate(&findings),
            TypeOnlyDecision::Block(_)
        ));
    }

    /// Block: a combined type+required `FieldChanged` blocks — a finding admits
    /// only when its attribute set is a *pure* type change (R2).
    #[test]
    fn type_only_gate_blocks_combined_type_and_required_change() {
        let findings = vec![gate_finding(
            "t1481",
            DriftChange::FieldChanged {
                direction: Direction::Response,
                block_name: "b".into(),
                field_index: 0,
                field_name: "f".into(),
                attributes: vec![
                    AttributeDelta {
                        attribute: FieldAttribute::Type,
                        from: "String".into(),
                        to: "Long".into(),
                    },
                    AttributeDelta {
                        attribute: FieldAttribute::Required,
                        from: "true".into(),
                        to: "false".into(),
                    },
                ],
            },
            SupportState::Implemented,
        )];
        assert!(matches!(
            type_only_gate(&findings),
            TypeOnlyDecision::Block(r) if r.contains("non-type")
        ));
    }

    // --- U5 facts-outage decision (pure, single-sourced) ------------------

    fn codes(cs: &[&str]) -> BTreeSet<String> {
        cs.iter().map(|c| c.to_string()).collect()
    }

    /// The discriminating per-group case (R3): a degraded group containing no
    /// maintained TR is untracked-only (exit 0 + finding); a co-occurring
    /// degraded group containing a maintained TR forces MaintainedAffected
    /// (exit 2).
    #[test]
    fn facts_outage_discriminates_maintained_from_untracked_endpoint_rate() {
        let maintained = codes(&["t1102", "token"]);

        // Untracked-only degradation → visible notice, not a gate.
        let d = facts_outage_decision(&codes(&["UNTRACKED_X"]), &maintained, false, true);
        assert!(matches!(d, FactsOutage::UntrackedOnly(_)), "got {d:?}");

        // A maintained TR co-occurs in the degraded set → exit 2.
        let d = facts_outage_decision(&codes(&["UNTRACKED_X", "t1102"]), &maintained, false, true);
        assert!(
            matches!(&d, FactsOutage::MaintainedAffected(r) if r.contains("t1102")),
            "got {d:?}"
        );
    }

    /// Property-type fallback is whole-inventory (KTD-5): served + any maintained
    /// TR present → exit 2, with no untracked-only split. Served with no
    /// maintained TR in the run is a visible notice only.
    #[test]
    fn facts_outage_property_type_fallback_is_whole_inventory() {
        let maintained = codes(&["t1102"]);
        let d = facts_outage_decision(&BTreeSet::new(), &maintained, true, true);
        assert!(
            matches!(&d, FactsOutage::MaintainedAffected(r) if r.contains("property-type")),
            "got {d:?}"
        );
        let d = facts_outage_decision(&BTreeSet::new(), &maintained, true, false);
        assert!(matches!(d, FactsOutage::UntrackedOnly(_)), "got {d:?}");
    }

    /// No degradation → None (the clean-fetch path is untouched).
    #[test]
    fn facts_outage_none_when_no_degradation() {
        assert_eq!(
            facts_outage_decision(&BTreeSet::new(), &codes(&["t1102"]), false, true),
            FactsOutage::None
        );
    }

    #[test]
    fn description_only_change_is_informational_and_does_not_gate() {
        let base = run_of(
            "t8412",
            vec![
                rb("OB", "A0003", Value::Null),
                prop(
                    "res_b",
                    "a",
                    "라벨",
                    "A0001",
                    serde_json::json!("4"),
                    "Y",
                    "old text",
                ),
            ],
        );
        let staged = run_of(
            "t8412",
            vec![
                rb("OB", "A0003", Value::Null),
                prop(
                    "res_b",
                    "a",
                    "라벨",
                    "A0001",
                    serde_json::json!("4"),
                    "Y",
                    "new text",
                ),
            ],
        );
        // Identity-stable field `a` with the same type/length/required but a
        // changed description → a field-level description change, no structural
        // diff.
        let report = compare(&base, &staged, &no_meta());
        assert!(report
            .findings
            .iter()
            .all(|f| matches!(f.change, DriftChange::DescriptionChanged { .. })));
        assert!(!report.gates(), "description-only never gates (R13)");
    }
}
