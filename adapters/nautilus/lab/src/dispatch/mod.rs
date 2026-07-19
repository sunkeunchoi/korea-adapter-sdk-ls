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

use nautilus_ls_calendar::schema::Citation;
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

/// A narrowly-audited attended override of an **Unknown** calendar date (U12, KTD8). It
/// flips ONLY an Unknown-date refusal — never Closed, Unavailable, authorization, integrity,
/// schema, availability, coverage, or the time-of-day window — and it NEVER changes the
/// calendar status. It is bound to the exact KST date and the current run, and carries the
/// full audit a reviewer needs: operator, run id, authorization instant, snapshot identity,
/// the relevant alerts, a reason, and a STRUCTURED first-party [`Citation`] (the same shape
/// the reconciliation layer accepts for notices — never free text, so an operator cannot
/// authorize dispatch on a real closure with an unverifiable justification).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownOverride {
    /// The exact KST civil date (`YYYY-MM-DD`) this override authorizes.
    pub kst_date: String,
    /// The exact dispatch run this override is bound to.
    pub run_id: String,
    /// The operator who authorized the override (audit).
    pub operator: String,
    /// The unix-seconds instant the override was authorized (audit).
    pub authorized_at_unix: i64,
    /// The snapshot `artifact_id` in force when the override was authorized (audit).
    pub snapshot_artifact_id: String,
    /// The snapshot `calendar_id` in force when the override was authorized (audit).
    pub snapshot_calendar_id: String,
    /// The relevant calendar alert ids/messages the operator reviewed (audit).
    #[serde(default)]
    pub alerts: Vec<String>,
    /// The operator-supplied reason — scrubbed before the record lands.
    pub reason: String,
    /// The structured, verifiable first-party basis (never free text).
    pub citation: Citation,
}

impl UnknownOverride {
    /// Whether every required audit field is present (a blank operator, reason, run id,
    /// KST date, or citation reference/issuer makes the override non-authorizing — a
    /// well-formed structured citation is mandatory, so an unverifiable basis cannot pass).
    pub fn is_well_formed(&self) -> bool {
        !self.kst_date.trim().is_empty()
            && !self.run_id.trim().is_empty()
            && !self.operator.trim().is_empty()
            && !self.reason.trim().is_empty()
            && !self.citation.reference.trim().is_empty()
            && !self.citation.issuer.trim().is_empty()
    }

    /// Whether this override covers the given KST date + run exactly (and is well-formed).
    /// A different date or a different run is NOT covered.
    pub fn covers(&self, kst_date: &str, run_id: &str) -> bool {
        self.is_well_formed() && self.kst_date == kst_date && self.run_id == run_id
    }
}
