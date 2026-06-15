//! Integration tests for the API Drift Tracker over checked-in fixtures.
//!
//! U6 covers normalize + diff; U7 extends this file with classify + promote.

use ls_trackers::{diff, normalize, Change, FieldShape, StagedSnapshot};

const BASELINE_JSON: &str = include_str!("fixtures/t8412_baseline.json");
const CANDIDATE_JSON: &str = include_str!("fixtures/t8412_candidate.json");

fn baseline() -> StagedSnapshot {
    serde_json::from_str(BASELINE_JSON).expect("baseline fixture parses")
}

fn candidate() -> StagedSnapshot {
    serde_json::from_str(CANDIDATE_JSON).expect("candidate fixture parses")
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
    assert_eq!(a.fields.get("t8412OutBlock.rec_count"), Some(&FieldShape::Number));
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
