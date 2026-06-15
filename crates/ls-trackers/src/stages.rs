//! The five tracker pipeline stages as explicit boundaries (R8): fetch,
//! normalize, diff, classify, promote.
//!
//! `fetch` is stubbed this round (R12) — snapshots are placed by hand, not
//! retrieved over the network. `normalize`/`diff` land in U6 and `classify`/
//! `promote` in U7; this scaffold declares the contract with placeholder bodies
//! so the crate compiles and re-exports cleanly.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ls_metadata::TrMetadata;
use serde_json::Value;

use crate::types::{
    Change, FieldShape, NormalizedArtifact, PromoteReport, StagedSnapshot, TrackerFinding,
};

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
/// Walks the JSON payload to a sorted map of field path → leaf [`FieldShape`].
/// Array indices are collapsed to `[]`, so list reordering and length are not
/// drift — only field additions, removals, and type changes are. Normalization
/// is idempotent and deterministic (the artifact's `BTreeMap` sorts by path
/// regardless of source key order).
pub fn normalize(snapshot: &StagedSnapshot) -> NormalizedArtifact {
    let mut fields = BTreeMap::new();
    collect_paths(&snapshot.payload, String::new(), &mut fields);
    NormalizedArtifact {
        tr_code: snapshot.tr_code.clone(),
        fields,
    }
}

/// Recurse into `value`, recording each leaf's path → shape. Objects descend by
/// key (`a.b`); arrays descend element-wise with a collapsed `[]` segment
/// (`a[].b`). Empty containers are recorded as a single `Array`/`Object` leaf so
/// an empty-but-present field is still tracked.
fn collect_paths(value: &Value, path: String, fields: &mut BTreeMap<String, FieldShape>) {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                if !path.is_empty() {
                    fields.insert(path, FieldShape::Object);
                }
            } else {
                for (key, child) in map {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    collect_paths(child, child_path, fields);
                }
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                if !path.is_empty() {
                    fields.insert(path, FieldShape::Array);
                }
            } else {
                let child_path = format!("{path}[]");
                for item in items {
                    collect_paths(item, child_path.clone(), fields);
                }
            }
        }
        Value::Null => {
            fields.insert(path, FieldShape::Null);
        }
        Value::Bool(_) => {
            fields.insert(path, FieldShape::Bool);
        }
        Value::Number(_) => {
            fields.insert(path, FieldShape::Number);
        }
        Value::String(_) => {
            fields.insert(path, FieldShape::String);
        }
    }
}

/// Stage 3 — diff a reviewed `baseline` against a `candidate` artifact into a
/// path-sorted set of [`Change`]s. Each change is tagged with the candidate's
/// `tr_code` (both snapshots are for the same TR — the payload carries none).
pub fn diff(baseline: &NormalizedArtifact, candidate: &NormalizedArtifact) -> Vec<Change> {
    let tr = candidate.tr_code.clone();
    let mut paths: BTreeSet<&String> = baseline.fields.keys().collect();
    paths.extend(candidate.fields.keys());

    let mut changes = Vec::new();
    for path in paths {
        match (baseline.fields.get(path), candidate.fields.get(path)) {
            (Some(_), None) => changes.push(Change::FieldRemoved {
                tr_code: tr.clone(),
                path: path.clone(),
            }),
            (None, Some(_)) => changes.push(Change::FieldAdded {
                tr_code: tr.clone(),
                path: path.clone(),
            }),
            (Some(&from), Some(&to)) if from != to => changes.push(Change::FieldChanged {
                tr_code: tr.clone(),
                path: path.clone(),
                from,
                to,
            }),
            _ => {}
        }
    }
    changes
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
