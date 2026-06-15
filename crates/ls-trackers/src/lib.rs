//! `ls-trackers` — staged-snapshot change tracking over LS API drift.
//!
//! A Change Tracker captures upstream LS artifacts as [`StagedSnapshot`]s,
//! normalizes and diffs them against reviewed baselines, and classifies each
//! difference into a [`Severity`] that is **support-aware**: it weights the
//! change by whether the affected TR is tracked, implemented, or recommended,
//! reading that state from `ls-metadata` (the same source of truth `ls-docgen`
//! projects). Findings are advisory — nothing here mutates SDK code, metadata,
//! or baselines (R13, R15).
//!
//! This round is a walking skeleton: the five stages are explicit boundaries,
//! `normalize`/`diff`/`classify` run for real over checked-in fixtures, while
//! `fetch` is stubbed (no network) and `promote` is a write-nothing dry-run.
//! The [`api_drift`] module is the one worked example; the Specification
//! Document Tracker exists only as the shared stage and type contract.

pub mod api_drift;
pub mod stages;
pub mod types;

pub use stages::{classify, diff, fetch, normalize, promote, FetchNotImplemented};
pub use types::{
    Change, FieldShape, NormalizedArtifact, PromoteReport, Severity, StagedSnapshot, TrackerFinding,
};
