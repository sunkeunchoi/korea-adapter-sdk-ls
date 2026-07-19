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

pub mod schema;
