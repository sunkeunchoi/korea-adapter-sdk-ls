//! `nautilus-ls-calendar` — the shared offline KRX domestic-equity trading calendar.
//!
//! A pure domain LEAF crate (KTD1): it depends on nothing in the standalone Nautilus
//! adapter workspace, so both `nautilus_ls` and `lab` can depend on it without a cycle.
//! It owns the self-contained snapshot schema, deterministic identities, typed loading,
//! evidence reconciliation, and proof-preserving day/range queries.
//!
//! This unit (U1) delivers only the serde snapshot schema and the tri-state
//! [`schema::DayStatus`]. Behavior (identities, loading, queries, reconciliation) is
//! layered on by later units.

pub mod adoption;
pub mod canonical;
pub mod diagnostics;
pub mod freshness;
pub mod load;
pub mod query;
pub mod reconcile;
pub mod schema;
pub mod witness;

pub use adoption::CalendarAdoption;
pub use canonical::{compute_artifact_id, compute_calendar_id, schema_is_compatible, SCHEMA_VERSION};
pub use diagnostics::{
    mask_identity, render_human, render_json, AuthorizationView, CalendarDiagnostic,
    CoverageSummary, DiagnosticOutcome, LoadFailure,
};
pub use freshness::{
    DimensionStaleness, FreshnessReport, FORWARD_READINESS_MIN_DAYS, FULL_HISTORY_STALE_AFTER_DAYS,
    INCREMENTAL_STALE_AFTER_DAYS, KASI_STALE_AFTER_DAYS,
};
pub use load::{CalendarLoadError, KrxCalendar};
pub use query::{AsOfView, DateRange, DayFact, Presence, QueryError, SessionSearch};
pub use reconcile::{reconcile, ReconcileAlert, ReconciledDay};
pub use witness::{
    build_witness_record, default_witness_id, witness_from_response, KrxDailyMarketResponse,
    KrxDailyRow, NonWitnessReason, WitnessOutcome, KRX_DAILY_MARKET_SOURCE_HINT, MIN_WITNESS_DATE,
};
