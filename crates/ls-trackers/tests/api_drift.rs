//! Integration coverage for the API Drift signal model (U4) against the real
//! authored metadata — the Acceptance Examples' support-aware exit gating.
//!
//! Support states come from `<repo>/metadata` (t1102 implemented, CSPAT00601
//! tracked-only), so these assert the end-to-end R17b contract, not a synthetic
//! support map.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ls_metadata::{validate_dir, TrMetadata};
use ls_trackers::{
    compare, BlockField, CodeSet, Direction, DriftChange, Manifest, NormalizedRun, Protocol,
    Severity, TrShape,
};

/// The authored, validated metadata map — the real support state classify reads.
fn authored_trs() -> BTreeMap<String, TrMetadata> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("metadata");
    validate_dir(&root)
        .expect("authored metadata must validate clean")
        .trs
}

fn field(block: &str, index: u32, name: &str, ty: &str, len: u32, desc: &str) -> BlockField {
    BlockField {
        direction: Direction::Response,
        block_name: block.to_string(),
        field_index: index,
        field_name: name.to_string(),
        korean_name: Some("라벨".to_string()),
        r#type: Some(ty.to_string()),
        length: Some(len),
        required: true,
        description_hash: if desc.is_empty() {
            None
        } else {
            Some(desc.to_string())
        },
    }
}

fn shape(code: &str, response: Vec<BlockField>) -> TrShape {
    TrShape {
        tr_code: code.to_string(),
        tr_name: Some(code.to_string()),
        protocol: Protocol::Rest,
        is_websocket: false,
        endpoint_path: Some(format!("/{code}")),
        api_group_id: Some("grp".to_string()),
        source_group_name: Some("그룹".to_string()),
        request_blocks: vec![],
        response_blocks: response,
        rate_limit_per_sec: Some(1),
        corp_rate_limit_per_sec: None,
        rate_source_group: Some("grp".to_string()),
        description_hash: None,
    }
}

/// Build a normalized run from explicit shapes plus extra untracked-only codes.
fn run(shapes: Vec<TrShape>, extra_codes: &[&str]) -> NormalizedRun {
    let mut codes: BTreeSet<String> = shapes.iter().map(|s| s.tr_code.clone()).collect();
    codes.extend(extra_codes.iter().map(|c| c.to_string()));
    let shape_map: BTreeMap<String, TrShape> =
        shapes.into_iter().map(|s| (s.tr_code.clone(), s)).collect();
    NormalizedRun {
        code_set: CodeSet {
            codes,
            provisional: false,
        },
        manifest: Manifest {
            upstream_tr_count: 0,
            maintained_tr_count: shape_map.len(),
            source_urls: vec![],
            normalizer_version: 1,
        },
        shapes: shape_map,
    }
}

/// AE2: an identical staged run vs the committed baseline → no gating (exit 0).
#[test]
fn ae2_identical_run_does_not_gate() {
    let s = shape("t1102", vec![field("Out", 0, "price", "Decimal", 8, "")]);
    let committed = run(vec![s.clone()], &[]);
    let staged = run(vec![s], &[]);
    let report = compare(&committed, &staged, &authored_trs());
    assert!(report.findings.is_empty());
    assert!(!report.gates(), "identical run is exit 0");
}

/// AE3: removing an implemented TR's response field → breaking, gates (exit 1).
#[test]
fn ae3_removing_implemented_field_is_breaking_and_gates() {
    let committed = run(
        vec![shape(
            "t1102",
            vec![
                field("Out", 0, "price", "Decimal", 8, ""),
                field("Out", 1, "volume", "Long", 12, ""),
            ],
        )],
        &[],
    );
    let staged = run(
        vec![shape(
            "t1102",
            vec![field("Out", 0, "price", "Decimal", 8, "")],
        )],
        &[],
    );
    let report = compare(&committed, &staged, &authored_trs());
    let removed = report
        .findings
        .iter()
        .find(|f| matches!(&f.change, DriftChange::FieldRemoved { field_name, .. } if field_name == "volume"))
        .expect("the removed field is a finding");
    assert_eq!(removed.severity, Severity::Breaking);
    assert!(removed.gates);
    assert!(report.gates(), "exit 1");
}

/// AE4: a new untracked TR gates (new-TR discovery); a field-level change to an
/// already-known untracked TR is report-only; no metadata is created either way.
#[test]
fn ae4_new_tr_gates_but_known_untracked_field_change_is_report_only() {
    // New-TR discovery.
    let committed = run(
        vec![shape(
            "t1102",
            vec![field("Out", 0, "price", "Decimal", 8, "")],
        )],
        &[],
    );
    let staged = run(
        vec![shape(
            "t1102",
            vec![field("Out", 0, "price", "Decimal", 8, "")],
        )],
        &["BRANDNEW"],
    );
    let report = compare(&committed, &staged, &authored_trs());
    let new_tr = report
        .findings
        .iter()
        .find(|f| f.tr_code == "BRANDNEW")
        .unwrap();
    assert!(
        new_tr.is_new_tr && new_tr.gates,
        "new-TR discovery gates (R9b)"
    );
    assert_eq!(new_tr.severity, Severity::Maintenance);

    // Known untracked TR with a field change (shape supplied both sides to
    // exercise the classification path) → report-only.
    let committed = run(
        vec![shape(
            "UNTRACKED1",
            vec![field("Out", 0, "a", "String", 4, "")],
        )],
        &[],
    );
    let staged = run(
        vec![shape(
            "UNTRACKED1",
            vec![field("Out", 0, "a", "String", 8, "")],
        )],
        &[],
    );
    let report = compare(&committed, &staged, &authored_trs());
    assert!(!report.findings.is_empty(), "the change is observed");
    assert!(
        report.findings.iter().all(|f| !f.gates),
        "a known untracked TR's change is report-only (R9)"
    );
    assert!(!report.gates(), "exit 0");
}

/// AE5: a large upstream code-set with a small metadata set surfaces missing
/// coverage and never flips the exit code.
#[test]
fn ae5_coverage_surfaces_but_never_gates() {
    // Staged inventory omits the authored `revoke` TR → metadata_missing_upstream.
    let trs = authored_trs();
    let committed = run(
        vec![shape(
            "t1102",
            vec![field("Out", 0, "price", "Decimal", 8, "")],
        )],
        &[],
    );
    let staged = run(
        vec![shape(
            "t1102",
            vec![field("Out", 0, "price", "Decimal", 8, "")],
        )],
        &["UP_A", "UP_B"], // untracked upstream codes (coverage gaps)
    );
    let report = compare(&committed, &staged, &trs);

    assert!(
        report
            .coverage
            .metadata_missing_upstream
            .contains(&"revoke".to_string()),
        "an authored TR absent upstream surfaces in coverage"
    );
    assert!(report
        .coverage
        .upstream_missing_metadata
        .contains(&"UP_A".to_string()));
    assert!(report.coverage.metadata_count >= 7);
    // New untracked codes DO gate (discovery), but coverage itself never does;
    // strip discovery to assert coverage independence directly.
    let only_coverage = compare(&committed, &committed, &trs);
    assert!(!only_coverage.gates(), "coverage alone never gates");
    assert!(!only_coverage.coverage.metadata_missing_upstream.is_empty());
}

/// AE7: a description-only change on an implemented TR is informational (exit 0).
#[test]
fn ae7_description_only_change_is_informational() {
    let committed = run(
        vec![shape(
            "t1102",
            vec![field("Out", 0, "price", "Decimal", 8, "hash-old")],
        )],
        &[],
    );
    let staged = run(
        vec![shape(
            "t1102",
            vec![field("Out", 0, "price", "Decimal", 8, "hash-new")],
        )],
        &[],
    );
    let report = compare(&committed, &staged, &authored_trs());
    assert!(report
        .findings
        .iter()
        .any(|f| matches!(f.change, DriftChange::DescriptionChanged { .. })));
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.severity == Severity::Informational),
        "description-only is informational"
    );
    assert!(!report.gates(), "exit 0 (R13)");
}

/// Real removal via code-set diff: an untracked TR's absence is report-only; a
/// maintained (tracked-only) baselined TR's absence gates (exit 1).
#[test]
fn removal_via_code_set_is_support_aware() {
    let trs = authored_trs();

    // Untracked removal → report-only.
    let committed = run(vec![shape("t1102", vec![])], &["GONE_UNTRACKED"]);
    let staged = run(vec![shape("t1102", vec![])], &[]);
    let report = compare(&committed, &staged, &trs);
    let removed = report
        .findings
        .iter()
        .find(|f| f.tr_code == "GONE_UNTRACKED")
        .unwrap();
    assert!(matches!(removed.change, DriftChange::TrRemoved));
    assert!(!removed.gates, "an untracked removal is report-only (R12)");
    assert!(!report.gates());

    // A maintained tracked-only TR (CSPAT00601) absent from the staged inventory
    // → real removal, gates.
    let committed = run(vec![shape("t1102", vec![])], &["CSPAT00601"]);
    let staged = run(vec![shape("t1102", vec![])], &[]);
    let report = compare(&committed, &staged, &trs);
    let removed = report
        .findings
        .iter()
        .find(|f| f.tr_code == "CSPAT00601")
        .unwrap();
    assert_eq!(removed.severity, Severity::Maintenance);
    assert!(
        removed.gates,
        "a maintained baselined TR removal gates (exit 1)"
    );
    assert!(report.gates());
}
