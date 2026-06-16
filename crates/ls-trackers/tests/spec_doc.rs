//! Integration coverage for the Specification Document Tracker (U6) against the
//! real authored metadata — the Acceptance Examples AE1–AE5 plus the
//! clean-self-diff bar, all network-free.
//!
//! Support states come from `<repo>/metadata` (t1102 implemented, CSPAT00601
//! tracked-only), so these assert the end-to-end advisory contract, not a
//! synthetic support map. Example inputs for the headline cases load from
//! `tests/fixtures/{tr}_example_{baseline,candidate}.json`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ls_metadata::{
    validate_dir, CertificationPath, Facets, InstrumentDomain, Maintenance, OwnerClass,
    Protocol as MetaProtocol, RateBucket, Support, TrMetadata, VenueSession,
};
use ls_trackers::{
    compare_examples, load_example_baseline, normalize_example_run, run_spec_check, spec_exit_for,
    spec_targets, ArtifactKind, Direction, ExampleRun, Exit, Paths, RawGroup, RawInventory, RawTr,
    Severity, SpecChange,
};
use serde_json::Value;

// --- fixtures (raw example values per TR) ----------------------------------

const T1102_BASELINE: &str = include_str!("fixtures/t1102_example_baseline.json");
const T1102_CANDIDATE: &str = include_str!("fixtures/t1102_example_candidate.json");
const TOKEN_BASELINE: &str = include_str!("fixtures/token_example_baseline.json");
const TOKEN_CANDIDATE: &str = include_str!("fixtures/token_example_candidate.json");

#[derive(serde::Deserialize)]
struct ExampleFixture {
    #[serde(default)]
    req_example: Value,
    #[serde(default)]
    res_example: Value,
}

fn fixture(json: &str) -> ExampleFixture {
    serde_json::from_str(json).expect("fixture parses")
}

// --- helpers ----------------------------------------------------------------

fn metadata_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("metadata")
}

fn authored_trs() -> BTreeMap<String, TrMetadata> {
    validate_dir(&metadata_root())
        .expect("authored metadata must validate clean")
        .trs
}

fn committed_spec_baseline() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("baselines")
        .join("spec-doc")
}

fn committed_raw() -> RawInventory {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("baselines/api-drift/raw/ls-openapi-full.json");
    serde_json::from_slice(&std::fs::read(path).expect("committed raw present"))
        .expect("committed raw parses")
}

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

fn run_of(trs: Vec<RawTr>) -> ExampleRun {
    let inv = RawInventory {
        source_urls: vec![],
        property_types: BTreeMap::new(),
        groups: vec![RawGroup {
            category_name: "c".to_string(),
            group_id: Some("g".to_string()),
            group_name: "그룹".to_string(),
            is_websocket_group: false,
            trs,
        }],
    };
    normalize_example_run(&inv, false)
}

fn str_val(s: &str) -> Value {
    Value::String(s.to_string())
}

/// A synthetic metadata entry with all-false support — resolves no artifact.
fn synthetic_unmaintained(code: &str) -> TrMetadata {
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
            tracked: false,
            implemented: false,
            recommended: false,
        },
        maintenance: Maintenance {
            source_spec_hash: "x".to_string(),
            last_reviewed: "2026-06-16".to_string(),
        },
        recommendation: None,
    }
}

// --- AE1 / AE4 --------------------------------------------------------------

/// AE1 + AE4: an implemented-TR JSON response example gains a field (AE1) while
/// its scalar sample values churn (AE4). The tracker emits exactly one advisory
/// shape-change finding naming both the reference and dependency docs, at exit 0
/// — the value churn produces nothing.
#[test]
fn ae1_implemented_shape_change_points_at_docs_and_ae4_ignores_churn() {
    let base = fixture(T1102_BASELINE);
    let cand = fixture(T1102_CANDIDATE);
    let committed = run_of(vec![raw_tr("t1102", base.req_example, base.res_example)]);
    let staged = run_of(vec![raw_tr("t1102", cand.req_example, cand.res_example)]);

    let report = compare_examples(&committed, &staged, &authored_trs());
    assert_eq!(
        spec_exit_for(&Ok(report.clone())),
        Exit::Ok,
        "advisory findings never gate"
    );
    assert!(!report.gates());

    // Exactly one finding: the response shape change (request is identical; the
    // churned scalar values on `price`/`hname`/`per` add nothing — AE4).
    assert_eq!(report.findings.len(), 1, "got: {:?}", report.findings);
    let finding = &report.findings[0];
    match &finding.change {
        SpecChange::ExampleShapeChanged {
            direction,
            added_paths,
            removed_paths,
            changed_paths,
        } => {
            assert_eq!(*direction, Direction::Response);
            assert!(added_paths.iter().any(|p| p == "t1102OutBlock.newfield"));
            assert!(removed_paths.is_empty());
            assert!(changed_paths.is_empty(), "value churn is not a shape change");
        }
        other => panic!("expected ExampleShapeChanged, got {other:?}"),
    }
    assert_eq!(finding.severity, Severity::Maintenance, "maintained → visible");

    // The pointer names both the reference doc (t1102 is implemented) and the
    // dependency doc.
    assert!(finding
        .pointers
        .iter()
        .any(|p| p.kind == ArtifactKind::ReferenceDoc && p.path == "docs/reference/t1102.md"));
    assert!(finding
        .pointers
        .iter()
        .any(|p| p.kind == ArtifactKind::DependencyDoc && p.path == "docs/tr-dependencies/t1102.md"));
}

/// AE4 in isolation: a JSON example with churned scalar values but identical
/// structure yields no finding at all.
#[test]
fn ae4_pure_value_churn_yields_no_finding() {
    let committed = run_of(vec![raw_tr(
        "t1102",
        Value::Null,
        str_val(r#"{"blk":{"price":4535,"hname":"LS증권"}}"#),
    )]);
    let churned = run_of(vec![raw_tr(
        "t1102",
        Value::Null,
        str_val(r#"{"blk":{"price":1,"hname":"다른값"}}"#),
    )]);
    let report = compare_examples(&committed, &churned, &authored_trs());
    assert!(report.findings.is_empty(), "got: {:?}", report.findings);
    assert!(!report.gates());
}

// --- AE2 --------------------------------------------------------------------

/// AE2: an example change confined to an untracked TR emits a visible finding
/// with no artifact pointer, and does not gate.
#[test]
fn ae2_untracked_example_change_is_visible_non_gating_without_pointer() {
    let committed = run_of(vec![raw_tr("UNTRACKED_TR", Value::Null, str_val(r#"{"a":1}"#))]);
    let staged = run_of(vec![raw_tr("UNTRACKED_TR", Value::Null, str_val(r#"{"a":1,"b":2}"#))]);
    let report = compare_examples(&committed, &staged, &authored_trs());

    let finding = report
        .findings
        .iter()
        .find(|f| f.tr_code == "UNTRACKED_TR")
        .expect("the untracked change is observed");
    assert_eq!(finding.severity, Severity::Informational);
    assert!(!finding.gates);
    assert!(finding.pointers.is_empty(), "no pointer for an untracked TR (R7)");
    assert_eq!(spec_exit_for(&Ok(report.clone())), Exit::Ok);
    assert!(!report.gates());
}

// --- AE3 --------------------------------------------------------------------

/// AE3: a Tracked TR whose metadata resolves no artifact (synthetic all-false
/// support) → informational-only, no pointer.
#[test]
fn ae3_metadata_with_no_resolvable_artifact_is_informational_only() {
    // The resolver returns nothing for an unmaintained metadata entry.
    assert!(spec_targets("SYNTH", &synthetic_unmaintained("SYNTH")).is_empty());

    let mut trs = authored_trs();
    trs.insert("SYNTH".to_string(), synthetic_unmaintained("SYNTH"));

    let committed = run_of(vec![raw_tr("SYNTH", Value::Null, str_val(r#"{"a":1}"#))]);
    let staged = run_of(vec![raw_tr("SYNTH", Value::Null, str_val(r#"{"a":1,"b":2}"#))]);
    let report = compare_examples(&committed, &staged, &trs);
    let finding = report.findings.iter().find(|f| f.tr_code == "SYNTH").unwrap();
    assert!(finding.pointers.is_empty(), "no pointer when resolution is empty (R7)");
    assert!(!finding.gates);
}

// --- AE5 --------------------------------------------------------------------

/// AE5: the `token` form-encoded request. A new key emits an
/// `ExampleKeySetChanged`; the secret-value rotation on existing keys and the
/// response token-value rotation produce nothing. Exit 0.
#[test]
fn ae5_token_form_key_added_secret_rotation_ignored() {
    let base = fixture(TOKEN_BASELINE);
    let cand = fixture(TOKEN_CANDIDATE);
    let committed = run_of(vec![raw_tr("token", base.req_example, base.res_example)]);
    let staged = run_of(vec![raw_tr("token", cand.req_example, cand.res_example)]);

    let report = compare_examples(&committed, &staged, &authored_trs());
    assert_eq!(spec_exit_for(&Ok(report.clone())), Exit::Ok);
    assert!(!report.gates());

    // Exactly one finding: the request key-set change (a new `audience` key). The
    // appkey/appsecretkey value rotation and the response access_token rotation
    // are secret-only churn and emit nothing.
    assert_eq!(report.findings.len(), 1, "got: {:?}", report.findings);
    match &report.findings[0].change {
        SpecChange::ExampleKeySetChanged {
            direction,
            added_keys,
            removed_keys,
        } => {
            assert_eq!(*direction, Direction::Request);
            assert_eq!(added_keys, &vec!["audience".to_string()]);
            assert!(removed_keys.is_empty());
        }
        other => panic!("expected ExampleKeySetChanged, got {other:?}"),
    }
}

/// AE5 (non-parseable arm): an example that becomes non-parseable emits an
/// informational `ExampleUnparseable` finding, without gating.
#[test]
fn ae5_non_parseable_example_is_informational() {
    let committed = run_of(vec![raw_tr("t1102", Value::Null, str_val(r#"{"a":1}"#))]);
    // Single-quoted pseudo-JSON does not parse → opaque.
    let staged = run_of(vec![raw_tr("t1102", Value::Null, str_val("{ 'a': 1 }"))]);
    let report = compare_examples(&committed, &staged, &authored_trs());
    let finding = report.findings.iter().find(|f| f.tr_code == "t1102").unwrap();
    assert!(matches!(finding.change, SpecChange::ExampleUnparseable { .. }));
    assert_eq!(finding.severity, Severity::Informational);
    assert!(!finding.gates);
    assert_eq!(spec_exit_for(&Ok(report)), Exit::Ok);
}

// --- clean self-diff --------------------------------------------------------

/// The committed `spec-doc` baseline vs a fresh projection of the shared raw →
/// zero findings, exit 0, across all 355 example shapes and every payload class.
/// This is the load-bearing full-inventory clean-self-diff bar.
#[test]
fn committed_baseline_self_diff_is_clean() {
    let committed = load_example_baseline(&committed_spec_baseline())
        .expect("committed spec-doc baseline loads");
    assert_eq!(committed.shapes.len(), 355, "full-inventory example baseline");
    assert_eq!(committed.manifest.upstream_tr_count, 365);

    let staged = normalize_example_run(&committed_raw(), committed.code_set.provisional);
    let report = compare_examples(&committed, &staged, &authored_trs());
    assert!(report.findings.is_empty(), "self-diff must be clean: {:?}", report.findings);
    assert!(!report.gates());
}

/// The same clean self-diff through the CLI orchestration (`run_spec_check`),
/// driving real committed paths — exit 0, no findings, no network.
#[test]
fn run_spec_check_against_committed_baseline_exits_zero() {
    let paths = Paths {
        baseline_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baselines/api-drift"),
        run_root: std::env::temp_dir().join("ls-trackers-spec-u6"),
        metadata_dir: metadata_root(),
        spec_baseline_dir: committed_spec_baseline(),
    };
    let result = run_spec_check(&paths, None);
    assert_eq!(spec_exit_for(&result), Exit::Ok);
    let report = result.expect("check runs");
    assert!(report.findings.is_empty(), "got: {:?}", report.findings);
    assert_eq!(report.coverage.example_tr_count, 355);
}
