//! Candidate snapshot recomputation (U14, KTD9).
//!
//! [`build_candidate`] takes the active predecessor snapshot + [`RefreshInputs`] and
//! produces a CANDIDATE snapshot: it merges normalized evidence (honoring source-failure
//! retention), reconciles every in-scope date through the core [`reconcile`], carries the
//! distinct coverage claims forward (never expanding on absence), ages freshness per
//! source, stamps both deterministic identities, and sets `predecessor_artifact_id` to the
//! EXACT active predecessor. It NEVER writes to disk — persistence is
//! [`write_candidate`](crate::calendar_refresh::write_candidate).
//!
//! ## Source-failure retention
//!
//! A source that FAILED this refresh keeps its prior accepted evidence (its records are not
//! replaced), does not advance its freshness dimension, and cannot expand coverage — if any
//! source failed, the materialized window does not grow past the predecessor's. Independent
//! additive evidence from the SUCCESSFUL sources still applies to in-window dates, forming a
//! PARTIAL candidate that the diff flags for review.

use std::cmp::{max, min};
use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};

use nautilus_ls_calendar::reconcile::reconcile;
use nautilus_ls_calendar::schema::{
    Alert, Coverage, DayRow, Freshness, Snapshot, Source, SourceKind,
};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, SCHEMA_VERSION};

use super::port::{RefreshInputs, RefreshScope};

/// Which refresh mode produced a candidate — drives which freshness dimension advances on
/// success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    /// Check elapsed dates after each expected KRX post-close opportunity (advances
    /// `last_incremental_at`).
    Incremental,
    /// Recompute from the history floor (advances `full_history_reconciled_at`).
    FullHistory,
}

/// Build a candidate [`Snapshot`] from `prior` + `inputs` over `scope` at `as_of`.
///
/// Deterministic: the same `prior`/`inputs`/`scope`/`mode`/`as_of` always produce a
/// byte-identical candidate (evidence keyed + sorted by id, rows ascending, alerts stamped
/// in row order). The result is a fully-formed, loadable snapshot with both identities
/// stamped and `predecessor_artifact_id = Some(prior.artifact_id)`.
pub fn build_candidate(
    prior: &Snapshot,
    inputs: &RefreshInputs,
    scope: &RefreshScope,
    mode: RefreshMode,
    as_of: DateTime<Utc>,
) -> Snapshot {
    let all_sources_ok = inputs.outcomes.iter().all(|o| o.is_ok());

    // Source ids that were refreshed SUCCESSFULLY — their prior evidence is replaced by the
    // fresh gather; a failed (or unmentioned) source keeps its prior evidence (retention).
    let ok_source_ids: HashSet<&str> = inputs
        .outcomes
        .iter()
        .filter(|o| o.is_ok())
        .map(|o| o.source_id.as_str())
        .collect();

    // Merge evidence by id: retained prior (from non-refreshed / failed sources) + fresh. A
    // successful source's prior record is REPLACED (dropped in favor of the fresh gather)
    // ONLY when its date falls inside the re-covered scope window — the window the gather
    // actually re-attested. A partial re-gather (e.g. an incremental refresh of a single
    // date) that marks a source "ok" must NEVER retract that source's prior positive
    // witnesses on dates it did not re-cover; dropping them wholesale would revert a
    // proven-open day back to an inferred Closed ("absence never retracts a prior positive
    // witness", enforced here at the BUILD layer). A FULL-history re-gather covers every
    // date, so its replacement stays wholesale — semantics unchanged.
    let mut evidence_by_id: BTreeMap<String, _> = BTreeMap::new();
    for e in &prior.evidence {
        let re_covered = ok_source_ids.contains(e.source_id.as_str())
            && e.date >= scope.from
            && e.date <= scope.through;
        if re_covered {
            continue; // replaced by the successful source's fresh gather for this in-scope date
        }
        evidence_by_id.insert(e.id.clone(), e.clone());
    }
    for e in &inputs.evidence {
        evidence_by_id.insert(e.id.clone(), e.clone());
    }
    let evidence: Vec<_> = evidence_by_id.into_values().collect();

    // Merge sources by id: prior sources updated with the successful sources' records.
    let mut sources_by_id: BTreeMap<String, Source> =
        prior.sources.iter().map(|s| (s.id.clone(), s.clone())).collect();
    for s in &inputs.sources {
        sources_by_id.insert(s.id.clone(), s.clone());
    }
    let sources: Vec<Source> = sources_by_id.into_values().collect();

    // Coverage window: a failed source cannot expand coverage by absence.
    let materialized_from = if all_sources_ok {
        min(prior.coverage.materialized_from, scope.from)
    } else {
        prior.coverage.materialized_from
    };
    let materialized_through = if all_sources_ok {
        max(prior.coverage.materialized_through, scope.through)
    } else {
        prior.coverage.materialized_through
    };
    let retrospectively_checked_through = if all_sources_ok {
        materialized_through
    } else {
        prior.coverage.retrospectively_checked_through
    };
    let scheduled_closure_evaluated_through = if all_sources_ok {
        materialized_through
    } else {
        prior.coverage.scheduled_closure_evaluated_through
    };

    // Reconcile every date in the (possibly grown) window.
    let mut rows: Vec<DayRow> = Vec::new();
    let mut alerts: Vec<Alert> = Vec::new();
    let mut cursor = materialized_from;
    while cursor <= materialized_through {
        let reconciled = reconcile(cursor, &evidence);
        let mut alert_refs = Vec::new();
        for (i, a) in reconciled.alerts.iter().enumerate() {
            let id = format!("alert-{}-{:?}-{i}", a.date, a.kind);
            alerts.push(Alert {
                id: id.clone(),
                date: a.date,
                kind: a.kind,
                message: a.message.clone(),
            });
            alert_refs.push(id);
        }
        rows.push(DayRow {
            date: cursor,
            status: reconciled.status,
            decisive_evidence: reconciled.decisive_evidence,
            conflicting_evidence: reconciled.conflicting_evidence,
            alerts: alert_refs,
        });
        cursor = match cursor.succ_opt() {
            Some(next) => next,
            None => break,
        };
    }

    // Freshness aging: advance a dimension only when its source succeeded (or, for the
    // mode dimensions, only when NO source failed); a failed source keeps its prior value.
    let kasi_ok = inputs
        .outcomes
        .iter()
        .any(|o| o.kind == SourceKind::KasiHoliday && o.is_ok());
    let freshness = Freshness {
        evidence_refreshed_at: as_of,
        holiday_facts_checked_at: if kasi_ok {
            Some(as_of)
        } else {
            prior.freshness.holiday_facts_checked_at
        },
        full_history_reconciled_at: if mode == RefreshMode::FullHistory && all_sources_ok {
            Some(as_of)
        } else {
            prior.freshness.full_history_reconciled_at
        },
        forward_readiness_through: prior.freshness.forward_readiness_through,
        last_incremental_at: if mode == RefreshMode::Incremental && all_sources_ok {
            Some(as_of)
        } else {
            prior.freshness.last_incremental_at
        },
    };

    let mut candidate = Snapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: Some(prior.artifact_id.clone()),
        scope: prior.scope.clone(),
        authorization: prior.authorization.clone(),
        coverage: Coverage {
            materialized_from,
            materialized_through,
            retrospectively_checked_through,
            scheduled_closure_evaluated_through,
            source_availability: prior.coverage.source_availability.clone(),
        },
        freshness,
        sources,
        evidence,
        alerts,
        rows,
    };
    candidate.artifact_id = compute_artifact_id(&candidate);
    candidate.calendar_id = compute_calendar_id(&candidate);
    candidate
}
