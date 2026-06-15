//! The five tracker pipeline stages as explicit boundaries (R8): fetch,
//! normalize, diff, classify, promote.
//!
//! `fetch` is stubbed this round (R12) — snapshots are placed by hand, not
//! retrieved over the network. `normalize`/`diff` land in U6 and `classify`/
//! `promote` in U7; this scaffold declares the contract with placeholder bodies
//! so the crate compiles and re-exports cleanly.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ls_metadata::{Support, TrMetadata};
use serde_json::Value;

use crate::types::{
    Change, FieldShape, NormalizedArtifact, PromoteReport, Severity, StagedSnapshot, TrackerFinding,
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

/// The Support-Aware Severity ladder (R11). A change to an implemented or
/// recommended TR always outranks the same change to a tracked-only TR:
///
/// | change            | implemented / recommended | tracked-only   |
/// |-------------------|---------------------------|----------------|
/// | removed / changed | `breaking`                | `maintenance`  |
/// | added (optional)  | `maintenance`             | `informational`|
///
/// `critical` (auth/order-safety) and `evidence` (stale evidence on a
/// recommended TR) are unreachable this round — no TR is recommended and
/// change-driven evidence invalidation is inactive.
fn severity_for(change: &Change, support: &Support) -> Severity {
    let strong = support.recommended || support.implemented;
    match change {
        Change::FieldRemoved { .. } | Change::FieldChanged { .. } => {
            if strong {
                Severity::Breaking
            } else {
                Severity::Maintenance
            }
        }
        Change::FieldAdded { .. } => {
            if strong {
                Severity::Maintenance
            } else {
                Severity::Informational
            }
        }
    }
}

/// Stage 4 — classify each [`Change`] into a Support-Aware Severity using the
/// affected TR's support state, emitting advisory [`TrackerFinding`]s sorted
/// highest-severity first. Output is advisory only — classify mutates nothing
/// (R15); the metadata map is read-only.
///
/// A change whose TR is absent from `trs` (not expected — you only snapshot
/// tracked TRs) is surfaced at `informational` rather than dropped, so signal is
/// never silently lost.
pub fn classify(changes: &[Change], trs: &BTreeMap<String, TrMetadata>) -> Vec<TrackerFinding> {
    let mut findings: Vec<TrackerFinding> = changes
        .iter()
        .map(|change| {
            let severity = match trs.get(change.tr_code()) {
                Some(meta) => severity_for(change, &meta.support),
                None => Severity::Informational,
            };
            TrackerFinding {
                tr_code: change.tr_code().to_string(),
                change: change.clone(),
                severity,
            }
        })
        .collect();

    // Highest severity first; ties keep diff's path-sorted order (stable sort).
    findings.sort_by(|a, b| b.severity.cmp(&a.severity));
    findings
}

/// Stage 5 — promote. A **dry-run** that writes nothing (R13): it enumerates the
/// reviewed baseline files, metadata fields, and generated docs a real,
/// mutating promote would touch for the affected TRs. This report is the
/// contract a future mutating promote must satisfy.
///
/// `promote` has no metadata access (only the findings), so it lists each
/// affected TR's Dependency Doc plus the shared index pages; whether a TR also
/// has a Reference page is a metadata question deferred to the real promote.
pub fn promote(findings: &[TrackerFinding]) -> PromoteReport {
    let affected: BTreeSet<&str> = findings.iter().map(|f| f.tr_code.as_str()).collect();

    let mut report = PromoteReport::default();
    for tr in &affected {
        report.baseline_files.push(format!(
            "crates/ls-trackers/tests/fixtures/{tr}_baseline.json"
        ));
        report.metadata_fields.push(format!(
            "metadata/trs/{tr}.yaml: maintenance.source_spec_hash"
        ));
        report
            .metadata_fields
            .push(format!("metadata/trs/{tr}.yaml: maintenance.last_reviewed"));
        report
            .generated_docs
            .push(format!("docs/tr-dependencies/{tr}.md"));
    }
    if !affected.is_empty() {
        report
            .generated_docs
            .push("docs/tr-dependencies/index.md".to_string());
        report
            .generated_docs
            .push("docs/reference/index.md".to_string());
    }
    report
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
