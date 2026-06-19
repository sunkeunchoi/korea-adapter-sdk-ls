//! `ls-metadata` — Rust-owned TR maintenance metadata.
//!
//! The serde structs and validator here are the single authority that validates
//! per-TR YAML; `metadata/tr-index.yaml` duplicates only routing fields and is
//! checked against `metadata/trs/*.yaml`. The change-scoped planner selects the
//! verification set from changed TRs, owning dependency classes, and facets.
//! No hand-maintained JSON Schema (ADR 0012).

pub mod freshness;
pub mod planner;
pub mod schema;
pub mod validator;

pub use freshness::{evaluate, review_by, FreshnessError, FreshnessState, DEFAULT_WINDOW_DAYS};
pub use planner::{plan_changes, plan_with_metadata, ChangeSet, PlanError, TestGroup};
pub use schema::{
    CertificationPath, Dependencies, EvidenceRecord, Facets, IndexEntry, InstrumentDomain,
    Maintenance, OwnerClass, Protocol, RateBucket, Recommendation, Support, TrIndex, TrMetadata,
    VenueSession,
};
pub use validator::{
    check_recommendation, check_routing, parse_tr_metadata, validate_dir, ValidationError,
    ValidationReport, INDEX_FILE_NAME,
};
