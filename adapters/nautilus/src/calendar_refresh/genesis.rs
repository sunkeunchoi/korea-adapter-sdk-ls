//! The genesis description artifact (U5) — the reviewable summary the first-install ceremony
//! reads (U6). It is a shared `calendar_refresh` module type (mirroring how the diff artifact's
//! type + `diff_path_for` live in module code), so U6's activation leg consumes the SAME type,
//! never a re-declared shape.
//!
//! The description carries what the operator reviews before superseding nothing with a chain
//! root: the exact candidate identity, coverage endpoints, per-status and per-source counts, the
//! consumer-window Unknown-weekday count (R12 guarantees zero — surfaced so the operator can
//! confirm the code refusal was not overridden), and the candidate's stamped authorization terms
//! (authority + granted/expires) so the ceremony's agreement-term check is mechanical.

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};
use serde::{Deserialize, Serialize};

use nautilus_ls_calendar::schema::{DayStatus, Snapshot};

use super::port::DateRange;

/// One source's evidence-record count in a genesis candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEvidenceCount {
    /// The evidence source id.
    pub source_id: String,
    /// How many evidence records the candidate carries for that source.
    pub count: usize,
}

/// The reviewable description of a genesis candidate (U5). Serde-round-trippable so the bin can
/// write it and the first-install ceremony (U6) can read it back and check it names the exact
/// candidate and matches its authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisDescription {
    /// The EXACT candidate `artifact_id` this description is for (the reviewed-artifact linkage).
    pub candidate_artifact_id: String,
    /// The candidate `calendar_id`.
    pub calendar_id: String,
    /// The first materialized civil date.
    pub coverage_from: NaiveDate,
    /// The last materialized civil date.
    pub coverage_through: NaiveDate,
    /// The consumer window start (R12 guard).
    pub consumer_window_from: NaiveDate,
    /// The consumer window end (R12 guard).
    pub consumer_window_through: NaiveDate,
    /// Consumer-window weekdays still Unknown — R12 guarantees this is 0; surfaced for review.
    pub consumer_window_unknown_weekdays: usize,
    /// Count of `TradingSession` rows.
    pub trading_session_rows: usize,
    /// Count of `Closed` rows.
    pub closed_rows: usize,
    /// Count of `Unknown` rows.
    pub unknown_rows: usize,
    /// Per-source evidence-record counts (ascending by source id).
    pub source_evidence_counts: Vec<SourceEvidenceCount>,
    /// The stamped granting authority / agreement identity (KTD7).
    pub authority: String,
    /// When authorization was granted.
    pub granted_at: DateTime<Utc>,
    /// When authorization expires, if bounded (the agreement term — KTD7).
    pub expires_at: Option<DateTime<Utc>>,
}

/// Build the [`GenesisDescription`] for `candidate` with the R12 `consumer_window`.
pub fn describe_genesis(candidate: &Snapshot, consumer_window: DateRange) -> GenesisDescription {
    let mut trading_session_rows = 0;
    let mut closed_rows = 0;
    let mut unknown_rows = 0;
    let mut consumer_window_unknown_weekdays = 0;
    for row in &candidate.rows {
        match row.status {
            DayStatus::TradingSession => trading_session_rows += 1,
            DayStatus::Closed => closed_rows += 1,
            DayStatus::Unknown => {
                unknown_rows += 1;
                if consumer_window.contains(row.date) && is_weekday(row.date) {
                    consumer_window_unknown_weekdays += 1;
                }
            }
        }
    }

    // Per-source counts, ascending by source id (deterministic).
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for e in &candidate.evidence {
        *counts.entry(e.source_id.clone()).or_insert(0) += 1;
    }
    let source_evidence_counts = counts
        .into_iter()
        .map(|(source_id, count)| SourceEvidenceCount { source_id, count })
        .collect();

    GenesisDescription {
        candidate_artifact_id: candidate.artifact_id.clone(),
        calendar_id: candidate.calendar_id.clone(),
        coverage_from: candidate.coverage.materialized_from,
        coverage_through: candidate.coverage.materialized_through,
        consumer_window_from: consumer_window.from,
        consumer_window_through: consumer_window.through,
        consumer_window_unknown_weekdays,
        trading_session_rows,
        closed_rows,
        unknown_rows,
        source_evidence_counts,
        authority: candidate.authorization.authority.clone(),
        granted_at: candidate.authorization.granted_at,
        expires_at: candidate.authorization.expires_at,
    }
}

fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}
