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
pub mod cli;
pub mod fetch;
pub mod stages;
pub mod types;

pub use api_drift::{compare, normalize_run, DriftReport, NormalizedRun, NORMALIZER_VERSION};
pub use cli::{run_cli, Command, Exit, Paths};
pub use fetch::{
    completeness_gate, parse_menu, FetchClient, FetchError, FetchInventoryError, GateOutcome,
    MenuGroup, MenuParseError, RawGroup, RawInventory, RawTr, RetryConfig,
    DEFAULT_TRUNCATION_PROPORTION,
};
pub use stages::{classify, diff, normalize, promote, FetchNotImplemented};
pub use types::{
    gates_for, BlockField, Change, CodeSet, CoverageSummary, Direction, DriftChange, DriftFinding,
    FetchReport, FieldShape, Manifest, NormalizedArtifact, PromoteReport, Protocol, Severity,
    StagedSnapshot, SupportState, TrShape, TrackerFinding,
};
