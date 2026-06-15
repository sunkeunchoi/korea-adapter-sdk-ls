//! The API Drift Tracker — the one concrete worked example (R14).
//!
//! It detects LS Open API shape changes (TR field additions, removals, and type
//! changes) by composing the shared stages: normalize a baseline and a candidate
//! Staged Snapshot, diff them, then classify each change against metadata support
//! state. The Specification Document Tracker is represented only by the shared
//! stage and type contract this round — it has no module of its own.

use std::collections::BTreeMap;

use ls_metadata::TrMetadata;

use crate::stages::{classify, diff, normalize};
use crate::types::{StagedSnapshot, TrackerFinding};

/// Run the API Drift pipeline over a reviewed `baseline` and a candidate
/// snapshot for the same TR, classifying each detected change against the
/// validated metadata in `trs`. Output is advisory only (R15).
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
