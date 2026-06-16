//! The Specification Document Tracker — example drift and advisory artifact
//! pointers, built parallel to [`crate::api_drift`] over the **same** staged raw
//! snapshot.
//!
//! The API Drift normalizer never reads the `req_example`/`res_example` fields
//! carried on every [`RawTr`](crate::fetch::RawTr); this tracker projects exactly
//! that latent facet. One staged snapshot feeds two non-overlapping lenses: API
//! Drift owns structural shape plus description/`korean_name`, and this tracker
//! owns request/response examples.
//!
//! Three layers live here:
//!
//! * [`normalize_example_run`] (U1) projects every TR carrying an example into a
//!   per-payload-class [`ExampleShape`] under a dedicated
//!   [`EXAMPLE_NORMALIZER_VERSION`]. The projection is **full-inventory** (KTD8)
//!   so an untracked TR's in-place example change is detectable, but it stores
//!   only structural descriptors — never a raw example value (KTD7).
//! * [`compare_examples`] (U2) diffs a staged projection against the Reviewed
//!   Baseline into support-aware, **advisory (never-gating)** [`SpecFinding`]s.
//! * [`spec_targets`] (U3) resolves a Tracked TR to the maintained SDK artifacts
//!   (reference + dependency docs) a changed example should prompt review of.

use std::collections::{BTreeMap, BTreeSet};

use ls_metadata::TrMetadata;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fetch::RawInventory;
use crate::types::{
    Direction, ExampleFacet, ExampleShape, FieldShape, Severity, ShapePathChange, SpecChange,
    SpecFinding, SupportState,
};

/// The example-normalizer version recorded in the [`ExampleManifest`] (R8/KTD2).
/// It starts independent of [`NORMALIZER_VERSION`](crate::api_drift::NORMALIZER_VERSION)
/// so the example projection has its own re-baseline cadence; bump it when the
/// example normalization rules change so an example-hash shift is auditable.
pub const EXAMPLE_NORMALIZER_VERSION: u32 = 1;

/// The normalized projection of one fetched run's **examples** (U1): the
/// full-inventory code-set, an [`ExampleManifest`], and per-TR [`ExampleShape`]s
/// for every TR carrying a non-empty example (KTD8). Mirrors
/// [`NormalizedRun`](crate::api_drift::NormalizedRun) but for the example facet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleRun {
    pub code_set: crate::types::CodeSet,
    pub manifest: ExampleManifest,
    /// Example shapes keyed by `tr_code` (sorted, deterministic).
    pub shapes: BTreeMap<String, ExampleShape>,
}

/// Inventory facts for an example projection (R8): full code-set size, the count
/// of TRs carrying an example, the source URLs, and the example-normalizer
/// version (so a normalizer change is auditable and a cross-version compare is
/// refused, U4). Distinct from the API Drift [`Manifest`](crate::types::Manifest)
/// so the two baselines version independently (KTD2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleManifest {
    /// Total upstream TR count for this snapshot (full inventory).
    pub upstream_tr_count: usize,
    /// Number of TRs carrying a non-empty request or response example.
    pub example_tr_count: usize,
    /// Source URLs the snapshot was scraped from.
    #[serde(default)]
    pub source_urls: Vec<String>,
    /// The example-normalizer version that produced these shapes.
    pub normalizer_version: u32,
}

/// Project a fetched [`RawInventory`] into an [`ExampleRun`]: the full-inventory
/// code-set plus an [`ExampleShape`] for **every** TR carrying a non-empty
/// request or response example (KTD8). A TR with no example contributes only its
/// code to the code-set, never a shape. `provisional` flags the code-set as a
/// not-yet-attested seed (KTD-5), mirroring
/// [`normalize_run`](crate::api_drift::normalize_run).
///
/// Support state is **not** used to filter the projection — the example baseline
/// is full-inventory so an untracked TR's in-place example change is detectable
/// (R3, R6/AE2). Severity weighting by support happens later, at compare time.
pub fn normalize_example_run(inventory: &RawInventory, provisional: bool) -> ExampleRun {
    let code_set = inventory.code_set(provisional);

    let mut shapes = BTreeMap::new();
    for group in &inventory.groups {
        for raw in &group.trs {
            let code = raw.code.trim().to_string();
            if code.is_empty() {
                continue;
            }
            let shape = ExampleShape {
                tr_code: code.clone(),
                req: classify_example(&raw.req_example),
                res: classify_example(&raw.res_example),
            };
            // Skip TRs whose examples are both empty — they carry no facet to
            // baseline. Key by TR code, last-writer-wins, with the same same-code
            // dedup caution as the API Drift normalizer (carried R-3).
            if shape.is_absent() {
                continue;
            }
            shapes.insert(code, shape);
        }
    }

    let manifest = ExampleManifest {
        upstream_tr_count: code_set.len(),
        example_tr_count: shapes.len(),
        source_urls: inventory.source_urls.clone(),
        normalizer_version: EXAMPLE_NORMALIZER_VERSION,
    };

    ExampleRun {
        code_set,
        manifest,
        shapes,
    }
}

/// Classify one raw `req_example`/`res_example` [`Value`] into a per-class
/// [`ExampleFacet`] (KTD3). LS stores examples as opaque strings of mixed
/// encoding (JSON-as-text, form-encoded, or free text), with `null`/empty for
/// TRs that carry none.
///
/// * `null` / empty / whitespace-only → [`ExampleFacet::Absent`].
/// * JSON-parseable (a JSON string, or an already-structured value) → reduce to a
///   field-path → leaf [`FieldShape`] map, discarding scalar values (R2, AE4).
/// * form-encoded (`k=v&k=v`, no JSON braces) → reduce to its key set, discarding
///   values so a secret-only change is invisible (R2, AE5).
/// * anything else (single-quoted pseudo-JSON, truncated payloads, free text) →
///   [`ExampleFacet::Opaque`], carrying no structure (R2, R9).
///
/// No raw value or scalar is ever returned — only paths, keys, and shapes (KTD7).
fn classify_example(value: &Value) -> ExampleFacet {
    match value {
        Value::Null => ExampleFacet::Absent,
        // An example stored as an already-structured JSON value (object/array):
        // project its shape directly.
        Value::Object(_) | Value::Array(_) | Value::Bool(_) | Value::Number(_) => {
            ExampleFacet::Json {
                shape: json_shape(value),
            }
        }
        Value::String(s) => classify_example_str(s),
    }
}

/// Classify a string example by payload class. Trims leading/trailing whitespace
/// first (LS examples are wrapped in `\r\n` noise) before attempting each class.
fn classify_example_str(raw: &str) -> ExampleFacet {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ExampleFacet::Absent;
    }
    // JSON-as-text: the dominant class. `from_str` skips leading/trailing
    // whitespace, so the `\r\n`-wrapped examples parse cleanly.
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return ExampleFacet::Json {
            shape: json_shape(&value),
        };
    }
    // Form-encoded: `k=v(&k=v)*` with no JSON braces. Only `token`/`revoke`
    // requests take this path upstream.
    if let Some(keys) = form_keys(trimmed) {
        return ExampleFacet::Form { keys };
    }
    // Free text, single-quoted pseudo-JSON, or truncated payloads: opaque. Never
    // silently dropped — marked explicitly so a class transition is observable
    // (the R-1-class trap), but carries no structure to compare.
    ExampleFacet::Opaque
}

/// Reduce a parsed JSON example to a sorted field-path → leaf [`FieldShape`] map,
/// reusing the API Drift leaf walker (KTD3) so example shape-diffing is identical
/// to the structural leaf model. Scalar sample values are discarded — only the
/// leaf shape per collapsed path survives, so value churn is not drift (AE4).
fn json_shape(value: &Value) -> BTreeMap<String, FieldShape> {
    let mut fields = BTreeMap::new();
    crate::stages::collect_paths(value, String::new(), &mut fields);
    fields
}

/// Parse a form-encoded example into its sorted key set, or `None` when the
/// string is not form-encoded. A form example is `k=v` pairs joined by `&`, with
/// no JSON object brace; each pair's key is the text before its first `=`, so a
/// value containing `=` (e.g. base64 padding in a JWT) does not corrupt the key.
/// Values are discarded entirely — only keys are retained (KTD7, AE5).
fn form_keys(trimmed: &str) -> Option<BTreeSet<String>> {
    if trimmed.contains('{') || !trimmed.contains('=') {
        return None;
    }
    let mut keys = BTreeSet::new();
    for pair in trimmed.split('&') {
        let (key, _value) = pair.split_once('=')?;
        let key = key.trim();
        if key.is_empty() {
            return None;
        }
        keys.insert(key.to_string());
    }
    Some(keys)
}

// ---------------------------------------------------------------------------
// U2 — compare a staged example projection against the Reviewed Baseline
// ---------------------------------------------------------------------------

/// The full output of one example comparison (U2): support-aware **advisory**
/// findings plus an example-coverage summary. Mirrors
/// [`DriftReport`](crate::api_drift::DriftReport), but
/// [`gates`](SpecReport::gates) is `false` for every example finding by
/// construction (KTD4) — only a snapshot-level fetch/parse/version error (U4)
/// ever exits non-zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecReport {
    pub findings: Vec<SpecFinding>,
    pub coverage: SpecCoverage,
}

impl SpecReport {
    /// Whether any finding gates. Always `false`: example changes are advisory
    /// (KTD4). Kept for parity with the API Drift exit contract so the CLI maps
    /// both reports through the same `exit_for` shape.
    pub fn gates(&self) -> bool {
        self.findings.iter().any(|f| f.gates)
    }
}

/// Example-projection coverage for a staged run (informational; never gates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpecCoverage {
    /// Distinct upstream TR codes in the staged inventory.
    pub upstream_count: usize,
    /// TRs carrying a projected example shape.
    pub example_tr_count: usize,
}

/// Compare a committed example baseline against a staged example projection (U2),
/// emitting support-aware **advisory** findings. Every finding is non-gating
/// (KTD4). This function only reads `trs` — it never writes metadata (R8).
///
/// A TR carrying an example on only one side is handled by diffing against a
/// synthetic [`ExampleShape::absent`], so an example's appearance/disappearance
/// surfaces as `ExampleAdded`/`ExampleRemoved` without a special case.
pub fn compare_examples(
    committed: &ExampleRun,
    staged: &ExampleRun,
    trs: &BTreeMap<String, TrMetadata>,
) -> SpecReport {
    let mut findings = Vec::new();

    // TRs in the staged projection: diff against the committed shape, or against
    // an absent shape when the TR newly carries an example.
    for (code, staged_shape) in &staged.shapes {
        let support = support_state_for(code, trs);
        let absent = ExampleShape::absent(code);
        let base_shape = committed.shapes.get(code).unwrap_or(&absent);
        for change in diff_example_shapes(base_shape, staged_shape) {
            let severity = example_change_severity(&change, support);
            findings.push(make_spec_finding(code, change, severity, support, trs));
        }
    }

    // TRs whose committed example disappeared from the staged projection.
    for (code, base_shape) in &committed.shapes {
        if staged.shapes.contains_key(code) {
            continue;
        }
        let support = support_state_for(code, trs);
        let absent = ExampleShape::absent(code);
        for change in diff_example_shapes(base_shape, &absent) {
            let severity = example_change_severity(&change, support);
            findings.push(make_spec_finding(code, change, severity, support, trs));
        }
    }

    // Highest severity first; ties keep insertion order.
    findings.sort_by(|a, b| b.severity.cmp(&a.severity));

    let coverage = SpecCoverage {
        upstream_count: staged.code_set.len(),
        example_tr_count: staged.shapes.len(),
    };
    SpecReport { findings, coverage }
}

fn support_state_for(code: &str, trs: &BTreeMap<String, TrMetadata>) -> SupportState {
    trs.get(code)
        .map(|m| SupportState::from_support(&m.support))
        .unwrap_or(SupportState::Untracked)
}

/// Diff two example shapes per direction into example changes (no severity yet).
fn diff_example_shapes(base: &ExampleShape, staged: &ExampleShape) -> Vec<SpecChange> {
    let mut out = Vec::new();
    if let Some(change) = diff_facet(Direction::Request, &base.req, &staged.req) {
        out.push(change);
    }
    if let Some(change) = diff_facet(Direction::Response, &base.res, &staged.res) {
        out.push(change);
    }
    out
}

/// Diff one direction's facet pair. Same-class facets are structurally compared;
/// `Absent`/`Opaque` are handled by class. A class transition that cannot be
/// structurally compared (parseable ↔ opaque, or JSON ↔ form) is surfaced as an
/// informational `ExampleUnparseable` rather than a guessed shape diff (R9, AE5).
/// Two `Opaque` facets compare equal (no finding), which keeps the self-diff
/// clean over the ~13 naturally-opaque examples.
fn diff_facet(
    direction: Direction,
    base: &ExampleFacet,
    staged: &ExampleFacet,
) -> Option<SpecChange> {
    match (base, staged) {
        (ExampleFacet::Absent, ExampleFacet::Absent) => None,
        (ExampleFacet::Absent, _) => Some(SpecChange::ExampleAdded { direction }),
        (_, ExampleFacet::Absent) => Some(SpecChange::ExampleRemoved { direction }),
        (ExampleFacet::Opaque, ExampleFacet::Opaque) => None,
        (ExampleFacet::Json { shape: b }, ExampleFacet::Json { shape: s }) => {
            diff_json(direction, b, s)
        }
        (ExampleFacet::Form { keys: b }, ExampleFacet::Form { keys: s }) => {
            diff_form(direction, b, s)
        }
        _ => Some(SpecChange::ExampleUnparseable { direction }),
    }
}

/// Diff two JSON leaf-path shape maps into an `ExampleShapeChanged`, or `None`
/// when only scalar values differed (the maps are identical — AE4).
fn diff_json(
    direction: Direction,
    base: &BTreeMap<String, FieldShape>,
    staged: &BTreeMap<String, FieldShape>,
) -> Option<SpecChange> {
    let mut added_paths = Vec::new();
    let mut removed_paths = Vec::new();
    let mut changed_paths = Vec::new();
    let mut paths: BTreeSet<&String> = base.keys().collect();
    paths.extend(staged.keys());
    for path in paths {
        match (base.get(path), staged.get(path)) {
            (None, Some(_)) => added_paths.push(path.clone()),
            (Some(_), None) => removed_paths.push(path.clone()),
            (Some(&from), Some(&to)) if from != to => changed_paths.push(ShapePathChange {
                path: path.clone(),
                from,
                to,
            }),
            _ => {}
        }
    }
    if added_paths.is_empty() && removed_paths.is_empty() && changed_paths.is_empty() {
        None
    } else {
        Some(SpecChange::ExampleShapeChanged {
            direction,
            added_paths,
            removed_paths,
            changed_paths,
        })
    }
}

/// Diff two form key sets into an `ExampleKeySetChanged`, or `None` when the key
/// sets match (a secret-only value rotation — AE5).
fn diff_form(
    direction: Direction,
    base: &BTreeSet<String>,
    staged: &BTreeSet<String>,
) -> Option<SpecChange> {
    let added_keys: Vec<String> = staged.difference(base).cloned().collect();
    let removed_keys: Vec<String> = base.difference(staged).cloned().collect();
    if added_keys.is_empty() && removed_keys.is_empty() {
        None
    } else {
        Some(SpecChange::ExampleKeySetChanged {
            direction,
            added_keys,
            removed_keys,
        })
    }
}

/// Severity for an example change, support-aware (R6) but **always advisory**:
/// `make_spec_finding` stores `gates: false` regardless (KTD4). A change on a
/// maintained TR is surfaced at `Maintenance` for visibility; an untracked-only
/// change, and any unparseable change, is `Informational` (R6, R9).
fn example_change_severity(change: &SpecChange, support: SupportState) -> Severity {
    match change {
        SpecChange::ExampleUnparseable { .. } => Severity::Informational,
        _ => {
            if support.is_maintained() {
                Severity::Maintenance
            } else {
                Severity::Informational
            }
        }
    }
}

/// Build an advisory [`SpecFinding`]. `gates` is always `false` (KTD4). The
/// artifact pointers are resolved in U3 ([`spec_targets`]); until then a finding
/// carries no pointer (informational-only, R7).
fn make_spec_finding(
    code: &str,
    change: SpecChange,
    severity: Severity,
    support: SupportState,
    trs: &BTreeMap<String, TrMetadata>,
) -> SpecFinding {
    let pointers = trs.get(code).map(|m| spec_targets(code, m)).unwrap_or_default();
    SpecFinding {
        tr_code: code.to_string(),
        change,
        severity,
        support_state: support,
        gates: false,
        pointers,
    }
}

// ---------------------------------------------------------------------------
// U3 — TR → maintained-artifact resolver (tier-a pointer)
// ---------------------------------------------------------------------------

/// The maintained-doc directories, mirrored from `ls-docgen`'s
/// `DEPENDENCY_DOCS_DIR` / `REFERENCE_DOCS_DIR` (KTD5). Hardcoded as string
/// literals — like [`promote_targets`](crate::stages::promote_targets) — rather
/// than taking an `ls-docgen` dependency for two path constants.
const DEPENDENCY_DOCS_DIR: &str = "docs/tr-dependencies";
const REFERENCE_DOCS_DIR: &str = "docs/reference";

/// Resolve a TR with metadata to its naming-convention-derivable maintained
/// artifacts (R4, KTD5), mirroring `ls-docgen`'s directory + implemented-only
/// conventions:
///
/// * `docs/tr-dependencies/{tr}.md` — for every **Tracked** TR (any maintained
///   support state).
/// * `docs/reference/{tr}.md` — only when `support.implemented`.
///
/// A metadata entry with no maintained support state (all-false support) resolves
/// **nothing** — informational-only, no pointer (R7, AE3). SDK-example and
/// Focused-Evidence artifacts have no derivable path today and are deferred until
/// the first Recommended TR exists (KTD5), so a Recommended TR also resolves only
/// these tier-a docs.
pub fn spec_targets(tr_code: &str, meta: &TrMetadata) -> Vec<crate::types::ArtifactRef> {
    use crate::types::{ArtifactKind, ArtifactRef};

    let mut targets = Vec::new();
    // An untracked metadata entry (all-false support) has no maintained artifact.
    if !SupportState::from_support(&meta.support).is_maintained() {
        return targets;
    }
    targets.push(ArtifactRef {
        kind: ArtifactKind::DependencyDoc,
        path: format!("{DEPENDENCY_DOCS_DIR}/{tr_code}.md"),
    });
    if meta.support.implemented {
        targets.push(ArtifactRef {
            kind: ArtifactKind::ReferenceDoc,
            path: format!("{REFERENCE_DOCS_DIR}/{tr_code}.md"),
        });
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{RawGroup, RawTr};

    fn raw_tr(code: &str, req: Value, res: Value) -> RawTr {
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
            properties: vec![],
            req_example: req,
            res_example: res,
        }
    }

    fn inventory(trs: Vec<RawTr>) -> RawInventory {
        RawInventory {
            source_urls: vec!["https://openapi.ls-sec.co.kr/apiservice".to_string()],
            property_types: BTreeMap::new(),
            groups: vec![RawGroup {
                category_name: "c".to_string(),
                group_id: Some("g".to_string()),
                group_name: "그룹".to_string(),
                is_websocket_group: false,
                trs,
            }],
        }
    }

    fn json_str(s: &str) -> Value {
        Value::String(s.to_string())
    }

    /// AE4: a JSON example whose structure is unchanged but a scalar value differs
    /// (timestamp, account number) normalizes to the identical `ExampleShape` —
    /// value churn is discarded, only the leaf-path shape survives.
    #[test]
    fn json_sample_value_churn_normalizes_identically() {
        let a = classify_example(&json_str(r#"{"blk":{"date":"20240906","price":4535}}"#));
        let b = classify_example(&json_str(r#"{"blk":{"date":"20231201","price":99}}"#));
        assert_eq!(a, b, "scalar value churn must not change the shape");
        match a {
            ExampleFacet::Json { shape } => {
                assert_eq!(shape.get("blk.date"), Some(&FieldShape::String));
                assert_eq!(shape.get("blk.price"), Some(&FieldShape::Number));
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    /// A JSON example with an added field normalizes to a shape carrying the new
    /// path — a structural change is visible.
    #[test]
    fn json_added_field_changes_the_shape() {
        let base = classify_example(&json_str(r#"{"blk":{"price":1}}"#));
        let with_field = classify_example(&json_str(r#"{"blk":{"price":1,"qty":2}}"#));
        assert_ne!(base, with_field);
        if let ExampleFacet::Json { shape } = with_field {
            assert!(shape.contains_key("blk.qty"));
        } else {
            panic!("expected Json");
        }
    }

    /// AE5: the `token` form-encoded request normalizes to a key set; adding a key
    /// changes it; changing only a secret value does not.
    #[test]
    fn form_encoded_normalizes_to_key_set_ignoring_secret_values() {
        let base = classify_example(&json_str(
            "appkey=BSrTOOZNoXtx&appsecretkey=d3HloL6T&grant_type=client_credentials&scope=oob",
        ));
        match &base {
            ExampleFacet::Form { keys } => {
                assert_eq!(
                    keys.iter().cloned().collect::<Vec<_>>(),
                    vec!["appkey", "appsecretkey", "grant_type", "scope"]
                );
            }
            other => panic!("expected Form, got {other:?}"),
        }

        // Changing only a secret value (the appkey/appsecretkey) → identical keys.
        let rotated = classify_example(&json_str(
            "appkey=ROTATED9999&appsecretkey=NEWSECRET&grant_type=client_credentials&scope=oob",
        ));
        assert_eq!(base, rotated, "a secret-only rotation is not a key-set change");

        // Adding a key → a different key set.
        let extra = classify_example(&json_str(
            "appkey=X&appsecretkey=Y&grant_type=client_credentials&scope=oob&extra=1",
        ));
        assert_ne!(base, extra);
    }

    /// A form value containing `=` (base64 padding in a JWT) does not corrupt the
    /// key — only the text before the first `=` is the key.
    #[test]
    fn form_key_extraction_is_robust_to_equals_in_values() {
        let facet = classify_example(&json_str("appkey=X&token=eyJ0eXA==.payload=="));
        if let ExampleFacet::Form { keys } = facet {
            assert_eq!(keys.iter().cloned().collect::<Vec<_>>(), vec!["appkey", "token"]);
        } else {
            panic!("expected Form");
        }
    }

    /// A free-text / non-parseable example normalizes to `Opaque` with no
    /// structure — never silently dropped, but never shape-diffed.
    #[test]
    fn non_parseable_example_is_opaque() {
        let py = classify_example(&json_str(
            "Example Language : Python\r\n----------------------------------",
        ));
        assert_eq!(py, ExampleFacet::Opaque);
        // Single-quoted pseudo-JSON (invalid JSON) is opaque too.
        let single = classify_example(&json_str("{\r\n\t'sysflag': 'U',\r\n\t'flag': 'E'\r\n}"));
        assert_eq!(single, ExampleFacet::Opaque);
    }

    /// `null` / empty / whitespace-only examples are `Absent`.
    #[test]
    fn empty_examples_are_absent() {
        assert_eq!(classify_example(&Value::Null), ExampleFacet::Absent);
        assert_eq!(classify_example(&json_str("")), ExampleFacet::Absent);
        assert_eq!(classify_example(&json_str("   \r\n  ")), ExampleFacet::Absent);
    }

    /// A TR with both examples empty produces no shape; a TR with any example is
    /// projected. The projection is full-inventory: it does not filter by support.
    #[test]
    fn projection_skips_empty_and_keeps_any_example() {
        let inv = inventory(vec![
            raw_tr("withjson", json_str(r#"{"a":1}"#), Value::Null),
            raw_tr("withform", json_str("k=v"), Value::Null),
            raw_tr("noexample", Value::Null, Value::Null),
        ]);
        let run = normalize_example_run(&inv, false);
        assert_eq!(run.code_set.len(), 3, "all codes in the code-set");
        assert_eq!(run.shapes.len(), 2, "only TRs carrying an example get a shape");
        assert!(run.shapes.contains_key("withjson"));
        assert!(run.shapes.contains_key("withform"));
        assert!(!run.shapes.contains_key("noexample"));
        assert_eq!(run.manifest.upstream_tr_count, 3);
        assert_eq!(run.manifest.example_tr_count, 2);
        assert_eq!(run.manifest.normalizer_version, EXAMPLE_NORMALIZER_VERSION);
    }

    /// The example run serializes deterministically and round-trips, and the
    /// committed example values never appear in the serialized shape (KTD7).
    #[test]
    fn example_run_serializes_deterministically_without_values() {
        let inv = inventory(vec![raw_tr(
            "token",
            json_str("appkey=SECRET123&scope=oob"),
            json_str(r#"{"access_token":"eyJSECRETJWT"}"#),
        )]);
        let run = normalize_example_run(&inv, true);
        let a = serde_json::to_vec(&run).unwrap();
        let b = serde_json::to_vec(&run).unwrap();
        assert_eq!(a, b, "serialization is byte-stable");
        let back: ExampleRun = serde_json::from_slice(&a).unwrap();
        assert_eq!(back, run, "example run round-trips");

        let json = String::from_utf8(a).unwrap();
        assert!(!json.contains("SECRET123"), "the form secret must not be stored");
        assert!(!json.contains("eyJSECRETJWT"), "the JWT value must not be stored");
        assert!(json.contains("access_token"), "but the structural key is stored");
        assert!(json.contains("appkey"), "and the form key is stored");
    }

    /// Projection over the committed shared raw snapshot is deterministic and
    /// full-inventory: it yields an example shape for every TR carrying an example
    /// (~355 req / 344 res), including the 7 Tracked TRs with their expected
    /// payload classes (`token`/`revoke` requests are form-encoded; the rest are
    /// JSON). No real example value lands in the projection (KTD7).
    #[test]
    fn projects_committed_raw_full_inventory_without_values() {
        let raw_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("baselines/api-drift/raw/ls-openapi-full.json");
        let raw: RawInventory =
            serde_json::from_slice(&std::fs::read(&raw_path).expect("committed raw present"))
                .expect("committed raw parses");

        let run = normalize_example_run(&raw, true);
        assert_eq!(run.code_set.len(), 365, "full inventory code-set");
        // ~355 TRs carry at least one example (req or res); assert a healthy lower
        // bound rather than an exact count so a benign upstream re-scrape doesn't
        // brittle-fail this guard.
        assert!(
            run.shapes.len() >= 350,
            "most TRs carry an example, got {}",
            run.shapes.len()
        );

        // The 7 Tracked TRs all project.
        for code in ["token", "revoke", "t1102", "t8412", "CSPAQ12200", "S3_", "CSPAT00601"] {
            assert!(run.shapes.contains_key(code), "{code} projects an example");
        }

        // `token`/`revoke` requests are form-encoded; their responses are JSON.
        assert!(matches!(run.shapes["token"].req, ExampleFacet::Form { .. }));
        assert!(matches!(run.shapes["token"].res, ExampleFacet::Json { .. }));
        assert!(matches!(run.shapes["revoke"].req, ExampleFacet::Form { .. }));
        // `t1102` request + response are JSON.
        assert!(matches!(run.shapes["t1102"].req, ExampleFacet::Json { .. }));
        assert!(matches!(run.shapes["t1102"].res, ExampleFacet::Json { .. }));

        // The projection is deterministic (byte-stable across two runs).
        let again = normalize_example_run(&raw, true);
        assert_eq!(run, again, "projection over committed raw is deterministic");

        // No embedded credential reaches the projection: the real `appkey` secret
        // and the JWT prefix from the token examples are absent from the bytes.
        let bytes = serde_json::to_vec(&run).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            !text.contains("appsecretkey=") && !text.contains("eyJ0eXAiOiJKV1Q"),
            "no raw form pair or JWT value is serialized"
        );
    }

    // --- U2 compare + U3 resolver -----------------------------------------

    use ls_metadata::{
        CertificationPath, Facets, InstrumentDomain, Maintenance, OwnerClass,
        Protocol as MetaProtocol, RateBucket, Support, TrMetadata, VenueSession,
    };

    /// A minimal valid `TrMetadata` with the given support flags, for the
    /// support-aware severity + resolver tests.
    fn meta(code: &str, tracked: bool, implemented: bool) -> TrMetadata {
        TrMetadata {
            tr_code: code.to_string(),
            name: None,
            owner_class: OwnerClass::Standalone,
            facets: Facets {
                protocol: MetaProtocol::Rest,
                instrument_domain: InstrumentDomain::Misc,
                venue_session: VenueSession::Unspecified,
                date_sensitive: false,
                self_paginated: false,
                account_state: false,
                paper_incompatible: false,
                certification_path: CertificationPath::None,
                rate_bucket: RateBucket::MarketData,
                caller_supplied_identifiers: vec![],
            },
            dependencies: Default::default(),
            support: Support {
                tracked,
                implemented,
                recommended: false,
            },
            maintenance: Maintenance {
                source_spec_hash: "x".to_string(),
                last_reviewed: "2026-06-16".to_string(),
            },
        }
    }

    fn run_of(trs: Vec<RawTr>, provisional: bool) -> ExampleRun {
        normalize_example_run(&inventory(trs), provisional)
    }

    fn meta_map(entries: Vec<TrMetadata>) -> BTreeMap<String, TrMetadata> {
        entries.into_iter().map(|m| (m.tr_code.clone(), m)).collect()
    }

    /// An identical baseline-vs-staged comparison yields no findings and never
    /// gates — the clean-self-diff property the whole tracker rests on.
    #[test]
    fn identical_projection_yields_no_findings() {
        let trs = raw_tr("t1102", json_str(r#"{"a":1}"#), json_str(r#"{"b":"s"}"#));
        let committed = run_of(vec![trs.clone()], false);
        let staged = run_of(vec![trs], false);
        let report = compare_examples(&committed, &staged, &meta_map(vec![]));
        assert!(report.findings.is_empty(), "got: {:?}", report.findings);
        assert!(!report.gates());
    }

    /// A JSON shape change on an implemented TR is advisory: it carries a finding
    /// at a visible severity but `gates == false` (KTD4).
    #[test]
    fn shape_change_on_implemented_tr_is_advisory_non_gating() {
        let committed = run_of(vec![raw_tr("t1102", Value::Null, json_str(r#"{"blk":{"price":1}}"#))], false);
        let staged = run_of(vec![raw_tr("t1102", Value::Null, json_str(r#"{"blk":{"price":1,"qty":2}}"#))], false);
        let report = compare_examples(&committed, &staged, &meta_map(vec![meta("t1102", true, true)]));

        let finding = report
            .findings
            .iter()
            .find(|f| matches!(f.change, SpecChange::ExampleShapeChanged { .. }))
            .expect("a shape change is emitted");
        assert_eq!(finding.severity, Severity::Maintenance, "maintained → visible");
        assert!(!finding.gates, "example changes never gate (KTD4)");
        assert!(!report.gates(), "the report never gates on example changes");
        // The added path is named; no scalar value is carried.
        if let SpecChange::ExampleShapeChanged { added_paths, direction, .. } = &finding.change {
            assert_eq!(*direction, Direction::Response);
            assert!(added_paths.iter().any(|p| p == "blk.qty"));
        }
    }

    /// AE2: an example change on an untracked TR (no metadata) emits a visible
    /// finding with no pointer; the report does not gate.
    #[test]
    fn untracked_example_change_is_visible_non_gating_without_pointer() {
        let committed = run_of(vec![raw_tr("UNTRACKED", Value::Null, json_str(r#"{"a":1}"#))], false);
        let staged = run_of(vec![raw_tr("UNTRACKED", Value::Null, json_str(r#"{"a":1,"b":2}"#))], false);
        let report = compare_examples(&committed, &staged, &meta_map(vec![]));

        let finding = report
            .findings
            .iter()
            .find(|f| f.tr_code == "UNTRACKED")
            .expect("the untracked change is observed");
        assert_eq!(finding.severity, Severity::Informational, "untracked → informational");
        assert!(!finding.gates);
        assert!(finding.pointers.is_empty(), "no pointer for an untracked TR (R7)");
        assert!(!report.gates(), "exit 0");
    }

    /// AE5: a key added to the `token` form request emits an `ExampleKeySetChanged`
    /// finding; a secret-only value rotation emits nothing.
    #[test]
    fn form_key_set_change_emits_finding_secret_rotation_does_not() {
        let base_req = "appkey=A&appsecretkey=B&grant_type=client_credentials&scope=oob";
        let committed = run_of(vec![raw_tr("token", json_str(base_req), Value::Null)], false);

        // Secret-only rotation → no finding.
        let rotated = run_of(
            vec![raw_tr("token", json_str("appkey=ZZZ&appsecretkey=YYY&grant_type=client_credentials&scope=oob"), Value::Null)],
            false,
        );
        let report = compare_examples(&committed, &rotated, &meta_map(vec![meta("token", true, true)]));
        assert!(report.findings.is_empty(), "a secret rotation is not drift: {:?}", report.findings);

        // Added key → an ExampleKeySetChanged finding.
        let extra = run_of(
            vec![raw_tr("token", json_str(&format!("{base_req}&newparam=1")), Value::Null)],
            false,
        );
        let report = compare_examples(&committed, &extra, &meta_map(vec![meta("token", true, true)]));
        let finding = report
            .findings
            .iter()
            .find(|f| matches!(f.change, SpecChange::ExampleKeySetChanged { .. }))
            .expect("a key-set change is emitted");
        assert!(!finding.gates);
        if let SpecChange::ExampleKeySetChanged { added_keys, .. } = &finding.change {
            assert_eq!(added_keys, &vec!["newparam".to_string()]);
        }
    }

    /// A JSON example that becomes non-parseable surfaces an informational
    /// `ExampleUnparseable` finding without gating (R9, AE5); two opaque examples
    /// compare equal (no finding), keeping the self-diff clean.
    #[test]
    fn parseable_to_opaque_is_informational_and_opaque_self_compares_clean() {
        let committed = run_of(vec![raw_tr("t1", Value::Null, json_str(r#"{"a":1}"#))], false);
        let opaque = run_of(vec![raw_tr("t1", Value::Null, json_str("{ 'a': 1 }"))], false); // single-quoted → opaque
        let report = compare_examples(&committed, &opaque, &meta_map(vec![]));
        let finding = report.findings.iter().find(|f| f.tr_code == "t1").unwrap();
        assert!(matches!(finding.change, SpecChange::ExampleUnparseable { .. }));
        assert_eq!(finding.severity, Severity::Informational);
        assert!(!finding.gates);

        // Opaque vs opaque (same naturally-opaque example) → no finding.
        let report = compare_examples(&opaque, &opaque, &meta_map(vec![]));
        assert!(report.findings.is_empty(), "opaque self-compare is clean");
    }

    /// AE1 resolver: an Implemented TR resolves both its reference and dependency
    /// docs; the finding carries both pointers.
    #[test]
    fn implemented_tr_resolves_reference_and_dependency_docs() {
        use crate::types::ArtifactKind;
        let targets = spec_targets("t1102", &meta("t1102", true, true));
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|t| t.kind == ArtifactKind::DependencyDoc
            && t.path == "docs/tr-dependencies/t1102.md"));
        assert!(targets.iter().any(|t| t.kind == ArtifactKind::ReferenceDoc
            && t.path == "docs/reference/t1102.md"));
    }

    /// A Tracked-only (not implemented) TR resolves its dependency doc only.
    #[test]
    fn tracked_only_tr_resolves_dependency_doc_only() {
        use crate::types::ArtifactKind;
        let targets = spec_targets("CSPAT00601", &meta("CSPAT00601", true, false));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].kind, ArtifactKind::DependencyDoc);
        assert_eq!(targets[0].path, "docs/tr-dependencies/CSPAT00601.md");
    }

    /// AE3: a metadata entry with no maintained support resolves no artifact →
    /// the finding is informational-only, no pointer.
    #[test]
    fn unmaintained_metadata_resolves_no_artifact() {
        let targets = spec_targets("SYNTH", &meta("SYNTH", false, false));
        assert!(targets.is_empty(), "all-false support resolves nothing (R7/AE3)");

        // And a finding for that TR carries no pointer.
        let committed = run_of(vec![raw_tr("SYNTH", Value::Null, json_str(r#"{"a":1}"#))], false);
        let staged = run_of(vec![raw_tr("SYNTH", Value::Null, json_str(r#"{"a":1,"b":2}"#))], false);
        let report = compare_examples(&committed, &staged, &meta_map(vec![meta("SYNTH", false, false)]));
        let finding = report.findings.iter().find(|f| f.tr_code == "SYNTH").unwrap();
        assert!(finding.pointers.is_empty(), "no pointer when resolution is empty");
    }
}
