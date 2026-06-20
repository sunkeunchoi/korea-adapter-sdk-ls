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
//! Two layers coexist:
//!
//! * The real-fetch **API Drift** signal model: [`fetch`] is a live Rust-native
//!   blocking client ([`FetchClient`]) that scrapes the LS Open API into a
//!   staged run, [`normalize_run`] projects the maintained TRs into Structural
//!   API Shapes, [`compare`] emits support-aware findings against a committed
//!   bounded baseline, and the [`cli`] maps them to a tiered exit. The live
//!   client is exercised against a local `httpmock` server in tests and against
//!   the LS Open API only under the operator seed (`make api-drift-fetch`);
//!   default `cargo test` is network-free.
//! * The PR #2 leaf-path walking skeleton ([`stages`]: `normalize`/`diff`/
//!   `classify` over checked-in fixtures, with `stages::fetch` an explicit
//!   not-implemented stub and `promote` a write-nothing dry-run), retained as
//!   compatibility coverage.
//!
//! Nothing here mutates SDK code, metadata, or baselines (R10, R15). The
//! Specification Document Tracker exists only as the shared stage and type
//! contract.

pub mod api_drift;
pub mod cli;
pub mod fetch;
pub mod freshness;
pub mod spec_doc;
pub mod stages;
pub mod types;

pub use freshness::{evaluate_recommended, today, FreshnessFinding, FreshnessReport};

pub use api_drift::{
    compare, facts_outage_decision, normalize_run, DriftReport, FactsOutage, NormalizedRun,
    NORMALIZER_VERSION,
};
pub use cli::{
    freshness_exit_for, load_example_baseline, renormalize_examples, run_cli, run_freshness_check,
    run_spec_check, spec_exit_for, write_example_baseline, Command, Exit, Paths,
};
pub use fetch::{
    completeness_gate, parse_menu, FetchClient, FetchError, FetchInventoryError, FetchOutcome,
    GateOutcome, MenuGroup, MenuParseError, RawGroup, RawInventory, RawTr, RetryConfig,
    DEFAULT_TRUNCATION_PROPORTION,
};
pub use spec_doc::{
    compare_examples, normalize_example_run, spec_targets, ExampleManifest, ExampleRun, SpecCoverage,
    SpecReport, EXAMPLE_NORMALIZER_VERSION,
};
pub use stages::{classify, diff, normalize, promote, FetchNotImplemented};
pub use types::{
    gates_for, ArtifactKind, ArtifactRef, BlockField, Change, CodeSet, CoverageSummary, Direction,
    DriftChange, DriftFinding, ExampleFacet, ExampleShape, FetchReport, FieldShape, Manifest,
    NormalizedArtifact, PromoteReport, Protocol, Severity, ShapePathChange, SpecChange, SpecFinding,
    StagedSnapshot, SupportState, TrShape, TrackerFinding,
};
