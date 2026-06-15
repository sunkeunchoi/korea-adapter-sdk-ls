//! The five tracker pipeline stages as explicit boundaries (R8): fetch,
//! normalize, diff, classify, promote.
//!
//! `fetch` is stubbed this round (R12) — snapshots are placed by hand, not
//! retrieved over the network. `normalize`/`diff` land in U6 and `classify`/
//! `promote` in U7; this scaffold declares the contract with placeholder bodies
//! so the crate compiles and re-exports cleanly.

use std::collections::BTreeMap;
use std::fmt;

use ls_metadata::TrMetadata;

use crate::types::{Change, NormalizedArtifact, PromoteReport, StagedSnapshot, TrackerFinding};

/// The explicit not-implemented marker `fetch` returns (R12). fetch does not
/// panic; callers see this and know to place a snapshot by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchNotImplemented;

impl fmt::Display for FetchNotImplemented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "fetch is not implemented this round: place a Staged Snapshot fixture by hand \
             (no network retrieval)",
        )
    }
}

impl std::error::Error for FetchNotImplemented {}

/// Stage 1 — fetch. Stubbed: returns [`FetchNotImplemented`] rather than
/// performing any network retrieval.
pub fn fetch() -> Result<StagedSnapshot, FetchNotImplemented> {
    Err(FetchNotImplemented)
}

/// Stage 2 — normalize a [`StagedSnapshot`] into a canonical [`NormalizedArtifact`].
///
/// Real logic lands in U6; this placeholder returns an empty artifact tagged
/// with the snapshot's TR code so the contract type-checks.
pub fn normalize(snapshot: &StagedSnapshot) -> NormalizedArtifact {
    NormalizedArtifact {
        tr_code: snapshot.tr_code.clone(),
        fields: BTreeMap::new(),
    }
}

/// Stage 3 — diff a baseline against a candidate artifact into a set of
/// [`Change`]s. Real logic lands in U6.
pub fn diff(_baseline: &NormalizedArtifact, _candidate: &NormalizedArtifact) -> Vec<Change> {
    Vec::new()
}

/// Stage 4 — classify each [`Change`] into a Support-Aware Severity using the
/// affected TR's support state, emitting advisory [`TrackerFinding`]s. Real
/// logic lands in U7.
pub fn classify(_changes: &[Change], _trs: &BTreeMap<String, TrMetadata>) -> Vec<TrackerFinding> {
    Vec::new()
}

/// Stage 5 — promote. A dry-run that writes nothing (R13); it enumerates what a
/// real promote would touch. Real logic lands in U7.
pub fn promote(_findings: &[TrackerFinding]) -> PromoteReport {
    PromoteReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_returns_not_implemented_marker_without_panicking() {
        let err = fetch().expect_err("fetch is stubbed this round");
        assert_eq!(err, FetchNotImplemented);
        assert!(err.to_string().contains("not implemented"));
    }
}
