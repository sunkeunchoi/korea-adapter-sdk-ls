//! The dispatch gate + capital ladder (Production Ladder plan).
//!
//! `lab-live --dispatch` machine-checks every session precondition and records the
//! attempt in an append-only, hash-chained dispatch chain that also carries the
//! capital-ladder rung state (KTD1, KTD2). This module owns:
//!
//! - [`chain`] — the hash-chained record store, its record types, and fail-closed
//!   verification (U1).
//!
//! Downstream units (checks, pre-registration, tracking, readiness, ladder) slot in
//! beside `chain`, all sharing the rung bounds and check-outcome vocabulary declared
//! here.

pub mod chain;
pub mod checks;
pub mod ladder;
pub mod nonce;
pub mod prereg;
pub mod readiness;
pub mod tracking;

use serde::{Deserialize, Serialize};

/// The suspended state below rung 1: paper-only, no live sessions.
pub const RUNG_SUSPENDED: u8 = 0;
/// Rung 1 — minimum live size, the first live rung.
pub const RUNG_MIN: u8 = 1;
/// Rung 4 — the full pre-registered budget, the top live rung.
pub const RUNG_MAX: u8 = 4;

/// The enforcement tier of a precondition check (R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Red aborts the session; no override (trading-env interlock, kill-switch state,
    /// account flat-start, rung authorization).
    NonDeferrable,
    /// Red can be overridden only by an explicit, named, recorded per-item deferral —
    /// never silently.
    Deferrable,
    /// Advisory only (e.g. rung authorization on a paper-lane pre-check, which does not
    /// consume rungs) — never blocks a dispatch.
    Informational,
}

/// The status a precondition check resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// Precondition satisfied.
    Green,
    /// Precondition satisfied with a recorded caveat (e.g. no live-session history yet,
    /// so an absent kill-switch store reads green-with-note).
    GreenWithNote,
    /// Precondition failed.
    Red,
    /// A gateway throttle (IGW00201) during a live-touching read: the check is a
    /// re-run, never a terminal outcome (KTD5). Never written as a terminal red.
    Throttled,
}

/// One precondition check's recorded outcome, carried verbatim into a session-dispatch
/// record so a reviewer can reconstruct why a session was authorized or refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckRecord {
    /// The check's stable name (e.g. `"flat_start"`, `"stranded_orders"`).
    pub name: String,
    /// Its enforcement tier.
    pub tier: Tier,
    /// The status it resolved to.
    pub status: CheckStatus,
    /// Free-text detail — scrubbed of credential-like tokens before the record lands.
    pub detail: String,
    /// Whether an explicit operator deferral overrode a deferrable red on this item.
    pub deferred: bool,
}

/// An explicit, operator-attributed deferral of a deferrable red (R3). Per-session,
/// never sticky; the counts are exceedance-catalog entries (R10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deferral {
    /// The named check item being deferred.
    pub item: String,
    /// The operator-supplied reason — scrubbed before the record lands.
    pub reason: String,
}
