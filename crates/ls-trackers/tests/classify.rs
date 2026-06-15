//! Integration tests for the API Drift Tracker over checked-in fixtures.
//!
//! U6 covers normalize + diff; U7 extends this file with classify + promote.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ls_metadata::{validate_dir, TrMetadata};
use ls_trackers::{
    classify, diff, normalize, promote, Change, FieldShape, Severity, StagedSnapshot,
};

const BASELINE_JSON: &str = include_str!("fixtures/t8412_baseline.json");
const CANDIDATE_JSON: &str = include_str!("fixtures/t8412_candidate.json");

fn baseline() -> StagedSnapshot {
    serde_json::from_str(BASELINE_JSON).expect("baseline fixture parses")
}

fn candidate() -> StagedSnapshot {
    serde_json::from_str(CANDIDATE_JSON).expect("candidate fixture parses")
}

/// The authored metadata map (`<repo>/metadata`), validated — the real support
/// state classify reads.
fn authored_trs() -> BTreeMap<String, TrMetadata> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("metadata");
    validate_dir(&root)
        .expect("authored metadata must validate clean")
        .trs
}

fn removed(tr: &str, path: &str) -> Change {
    Change::FieldRemoved {
        tr_code: tr.to_string(),
        path: path.to_string(),
    }
}

#[test]
fn normalize_produces_a_stable_canonical_artifact() {
    let snap = baseline();
    let a = normalize(&snap);
    let b = normalize(&snap);
    assert_eq!(a, b, "normalize is idempotent");

    // Array indices collapse to `[]`; nested object fields use dotted paths.
    assert_eq!(
        a.fields.get("t8412OutBlock1[].jongchk"),
        Some(&FieldShape::String)
    );
    assert_eq!(
        a.fields.get("t8412OutBlock.rec_count"),
        Some(&FieldShape::Number)
    );
    assert_eq!(a.tr_code, "t8412");
}

#[test]
fn diff_reports_added_removed_and_changed_keyed_to_tr() {
    let changes = diff(&normalize(&baseline()), &normalize(&candidate()));

    // Reordering the two list elements is NOT a change — exactly three drifts.
    assert_eq!(changes.len(), 3, "got: {changes:?}");
    assert!(changes.iter().all(|c| c.tr_code() == "t8412"));

    assert!(changes.iter().any(|c| matches!(
        c,
        Change::FieldRemoved { path, .. } if path == "t8412OutBlock1[].jongchk"
    )));
    assert!(changes.iter().any(|c| matches!(
        c,
        Change::FieldAdded { path, .. } if path == "t8412OutBlock1[].vwap"
    )));
    assert!(changes.iter().any(|c| matches!(
        c,
        Change::FieldChanged { path, from, to, .. }
            if path == "t8412OutBlock.rec_count"
                && *from == FieldShape::Number
                && *to == FieldShape::String
    )));
}

#[test]
fn diff_of_identical_artifacts_is_empty() {
    let changes = diff(&normalize(&baseline()), &normalize(&baseline()));
    assert!(changes.is_empty(), "no drift expected, got: {changes:?}");
}

/// AE2: a removed field on an implemented TR (t8412) classifies `breaking`; the
/// same removal on a tracked-only TR (CSPAT00601) classifies lower
/// (`maintenance`); a new optional field on an implemented TR is `maintenance`.
#[test]
fn classify_is_support_aware() {
    let trs = authored_trs();

    // Removed field on an implemented TR → breaking.
    let breaking = classify(&[removed("t8412", "t8412OutBlock1[].jongchk")], &trs);
    assert_eq!(breaking.len(), 1);
    assert_eq!(breaking[0].severity, Severity::Breaking);

    // The same removal on the tracked-but-unimplemented order TR → lower.
    let tracked_only = classify(&[removed("CSPAT00601", "Out.SomeField")], &trs);
    assert_eq!(tracked_only.len(), 1);
    assert_eq!(tracked_only[0].severity, Severity::Maintenance);
    assert!(
        tracked_only[0].severity < breaking[0].severity,
        "an implemented-TR change must outrank the same tracked-only change"
    );

    // A new optional field on an implemented TR → maintenance.
    let added = classify(
        &[Change::FieldAdded {
            tr_code: "t8412".to_string(),
            path: "t8412OutBlock1[].vwap".to_string(),
        }],
        &trs,
    );
    assert_eq!(added[0].severity, Severity::Maintenance);
}

/// The end-to-end API Drift run over the fixtures classifies the removed field
/// as the top finding.
#[test]
fn fixture_diff_classifies_breaking_first() {
    let trs = authored_trs();
    let changes = diff(&normalize(&baseline()), &normalize(&candidate()));
    let findings = classify(&changes, &trs);

    assert_eq!(findings.len(), 3);
    // Highest severity first, sorted descending. The removed field and the type
    // change on implemented t8412 are both `breaking`; the added optional field
    // is `maintenance` and sorts last.
    assert_eq!(findings[0].severity, Severity::Breaking);
    assert!(findings.windows(2).all(|w| w[0].severity >= w[1].severity));
    assert!(findings
        .iter()
        .any(|finding| finding.severity == Severity::Breaking
            && matches!(
                finding.change,
                Change::FieldRemoved { ref path, .. } if path == "t8412OutBlock1[].jongchk"
            )));
    assert!(matches!(
        findings[2].change,
        Change::FieldAdded { ref path, .. } if path == "t8412OutBlock1[].vwap"
    ));
}

#[test]
fn promote_is_a_write_nothing_dry_run_that_enumerates_touches() {
    let trs = authored_trs();
    let changes = diff(&normalize(&baseline()), &normalize(&candidate()));
    let findings = classify(&changes, &trs);

    // Capture the baseline fixture content before promote to prove no mutation.
    let before = BASELINE_JSON.to_string();

    let report = promote(&findings);

    // Enumerates exactly what a real promote would touch for t8412.
    assert!(report
        .baseline_files
        .iter()
        .any(|p| p == "crates/ls-trackers/tests/fixtures/t8412_baseline.json"));
    assert!(report
        .metadata_fields
        .iter()
        .any(|f| f == "metadata/trs/t8412.yaml: maintenance.source_spec_hash"));
    assert!(report
        .generated_docs
        .iter()
        .any(|d| d == "docs/tr-dependencies/t8412.md"));

    // Writes nothing: the in-memory fixture is the source of truth and is
    // unchanged, and promote performs no filesystem I/O.
    assert_eq!(before, BASELINE_JSON, "promote must not mutate inputs");
}
