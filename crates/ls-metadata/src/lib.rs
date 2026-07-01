//! `ls-metadata` — Rust-owned TR maintenance metadata.
//!
//! The serde structs and validator here are the single authority that validates
//! per-TR YAML; `metadata/tr-index.yaml` duplicates only routing fields and is
//! checked against `metadata/trs/*.yaml`. The change-scoped planner selects the
//! verification set from changed TRs, owning dependency classes, and facets.
//! No hand-maintained JSON Schema (ADR 0012).

pub mod constraints;
pub mod freshness;
pub mod planner;
pub mod schema;
pub mod shape;
pub mod validator;

pub use constraints::{
    baseline_request_fields, ground_constraints, BaselineField, GroundingError,
};
pub use freshness::{evaluate, review_by, FreshnessError, FreshnessState, DEFAULT_WINDOW_DAYS};
pub use shape::{BlockField, Direction, TrShape};
pub use planner::{plan_changes, plan_with_metadata, ChangeSet, PlanError, TestGroup};
pub use schema::{
    CatalogEntry, CertificationPath, ClassCoverage, ConstraintSchema, CrossFieldRule, Dependencies,
    EnumRule, ErrorCatalog, ErrorCoverage, EvidenceRecord, Facets, FieldConstraint, FieldType,
    FormatKind, FormatRule, IndexEntry, InstrumentDomain, Maintenance, OwnerClass, ProbeStatus,
    Protocol, RangeRule, RateBucket, Recommendation, Support, TrIndex, TrMetadata, VenueSession,
};
pub use validator::{
    check_artifacts, check_recommendation, check_routing, parse_tr_metadata, validate_dir,
    ValidationError, ValidationReport, CATALOG_FILE_NAME, INDEX_FILE_NAME,
};
