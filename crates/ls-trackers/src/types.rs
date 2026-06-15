//! Core change-tracking types, using `CONTEXT.md` vocabulary.
//!
//! [`StagedSnapshot`] is a captured upstream artifact; [`NormalizedArtifact`] is
//! its canonical projection; [`Change`] is one structural difference; [`Severity`]
//! is the Support-Aware Severity tier; and [`TrackerFinding`] pairs a change with
//! its classified severity. These define the shared contract both Change Trackers
//! (the API Drift Tracker and, later, the Specification Document Tracker) speak.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A captured upstream LS API artifact, tagged with the TR it belongs to.
///
/// LS response payloads are keyed by block names (e.g. `CSPAQ12200OutBlock1`)
/// and carry no TR code, so `tr_code` is an **explicit** snapshot field rather
/// than something derived from block-name prefixes. fetch is stubbed this round
/// (R12), so snapshots are placed by hand as fixtures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StagedSnapshot {
    pub tr_code: String,
    pub payload: serde_json::Value,
}

/// The canonical leaf shape recorded for each field path — enough to flag an
/// incompatible type change without storing volatile sample values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldShape {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

impl fmt::Display for FieldShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            FieldShape::Null => "null",
            FieldShape::Bool => "bool",
            FieldShape::Number => "number",
            FieldShape::String => "string",
            FieldShape::Array => "array",
            FieldShape::Object => "object",
        };
        f.write_str(s)
    }
}

/// A [`StagedSnapshot`] reduced to a canonical, sorted map of field path → leaf
/// shape. Array indices are collapsed to `[]`, so reordering within a list is
/// **not** a change — only field additions, removals, and type changes are
/// (resolving the U6 open question: some LS list orderings are semantically
/// meaningful, but the structural diff this round watches shape, not order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedArtifact {
    pub tr_code: String,
    pub fields: BTreeMap<String, FieldShape>,
}

/// A single structural difference between two normalized artifacts.
///
/// Each variant carries the `tr_code` propagated from the snapshot — the lookup
/// key [`classify`](crate::stages::classify) uses, since the payload itself has
/// no TR code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A field present in the candidate but not the baseline.
    FieldAdded { tr_code: String, path: String },
    /// A field present in the baseline but not the candidate.
    FieldRemoved { tr_code: String, path: String },
    /// A field present in both but with an incompatible leaf shape.
    FieldChanged {
        tr_code: String,
        path: String,
        from: FieldShape,
        to: FieldShape,
    },
}

impl Change {
    /// The TR this change belongs to.
    pub fn tr_code(&self) -> &str {
        match self {
            Change::FieldAdded { tr_code, .. }
            | Change::FieldRemoved { tr_code, .. }
            | Change::FieldChanged { tr_code, .. } => tr_code,
        }
    }

    /// The field path this change concerns.
    pub fn path(&self) -> &str {
        match self {
            Change::FieldAdded { path, .. }
            | Change::FieldRemoved { path, .. }
            | Change::FieldChanged { path, .. } => path,
        }
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Change::FieldAdded { path, .. } => write!(f, "added field `{path}`"),
            Change::FieldRemoved { path, .. } => write!(f, "removed field `{path}`"),
            Change::FieldChanged { path, from, to, .. } => {
                write!(f, "field `{path}` changed shape {from} → {to}")
            }
        }
    }
}

/// Support-Aware Severity — the classification of a [`TrackerFinding`] (R11).
///
/// Variants are declared in **ascending** severity, so the derived `Ord` makes
/// `Breaking > Informational` etc. true as comparisons; findings are presented
/// highest-first by sorting in reverse. The migration plan's ladder is
/// critical / breaking / maintenance / evidence / informational (most to least
/// severe); this enum is that ladder reversed for `Ord`.
///
/// This round's fixtures reach only `Informational` / `Maintenance` / `Breaking`.
/// `Evidence` (stale focused evidence on a Recommended TR) and `Critical`
/// (auth/order-safety changes) are defined but unreachable here — no TR is
/// recommended and change-driven evidence invalidation is inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Informational,
    Evidence,
    Maintenance,
    Breaking,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Severity::Informational => "informational",
            Severity::Evidence => "evidence",
            Severity::Maintenance => "maintenance",
            Severity::Breaking => "breaking",
            Severity::Critical => "critical",
        };
        f.write_str(s)
    }
}

/// A severity-classified observation emitted by a Change Tracker before it
/// becomes SDK work. Advisory only — nothing auto-promotes it (R15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerFinding {
    pub tr_code: String,
    pub change: Change,
    pub severity: Severity,
}

impl fmt::Display for TrackerFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.tr_code, self.change)
    }
}

/// What a real `promote` would touch, enumerated by the dry-run (R13). The
/// dry-run writes nothing; this report is the contract a future mutating
/// promote must satisfy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PromoteReport {
    /// Reviewed baseline fixture files a real promote would rewrite.
    pub baseline_files: Vec<String>,
    /// Metadata fields a real promote would update (e.g. `source_spec_hash`).
    pub metadata_fields: Vec<String>,
    /// Generated docs a real promote would regenerate.
    pub generated_docs: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derived `Ord` ranks the tiers by severity: breaking outranks
    /// informational, critical is the top.
    #[test]
    fn severity_orders_by_tier() {
        assert!(Severity::Breaking > Severity::Informational);
        assert!(Severity::Maintenance > Severity::Informational);
        assert!(Severity::Critical > Severity::Breaking);
        assert!(Severity::Maintenance > Severity::Evidence);

        let mut tiers = [
            Severity::Breaking,
            Severity::Informational,
            Severity::Critical,
            Severity::Maintenance,
        ];
        tiers.sort();
        assert_eq!(
            tiers,
            [
                Severity::Informational,
                Severity::Maintenance,
                Severity::Breaking,
                Severity::Critical,
            ]
        );
    }

    #[test]
    fn change_accessors_expose_tr_and_path() {
        let c = Change::FieldRemoved {
            tr_code: "t8412".to_string(),
            path: "t8412OutBlock.shcode".to_string(),
        };
        assert_eq!(c.tr_code(), "t8412");
        assert_eq!(c.path(), "t8412OutBlock.shcode");
        assert!(c.to_string().contains("removed field"));
    }
}
