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
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};

use nautilus_ls_calendar::reconcile::reconcile;
use nautilus_ls_calendar::schema::{
    Alert, Authorization, CalendarScope, Coverage, DayRow, DayStatus, EvidenceRecord, Freshness,
    Snapshot, Source, SourceAvailabilityBound, SourceKind,
};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, SCHEMA_VERSION};

use super::port::{uncovered_within, DateRange, RefreshInputs, RefreshScope};

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
///
/// This is the FROM-PRIOR entry: it wraps the [`BuildBase::from_prior`] base and the shared
/// [`build_from_base`] core so the pre-genesis behavior is preserved verbatim (byte-identical).
/// The predecessor-less genesis path shares the identical core through [`build_genesis`].
pub fn build_candidate(
    prior: &Snapshot,
    inputs: &RefreshInputs,
    scope: &RefreshScope,
    mode: RefreshMode,
    as_of: DateTime<Utc>,
) -> Snapshot {
    build_from_base(&BuildBase::from_prior(prior), inputs, scope, mode, as_of)
}

/// The shared candidate-recompute core, run over a typed [`BuildBase`] (KTD1). Whether the
/// base is a real predecessor (from-prior) or the genesis identity (no predecessor), the body
/// below is branch-free: for genesis the base's seeds are identities for every merge (empty
/// evidence/sources retain nothing; coverage seeds equal the genesis window so `min`/`max`
/// are no-ops), so the only genesis-specific difference is `predecessor_artifact_id == None`.
fn build_from_base(
    base: &BuildBase,
    inputs: &RefreshInputs,
    scope: &RefreshScope,
    mode: RefreshMode,
    as_of: DateTime<Utc>,
) -> Snapshot {
    let all_sources_ok = inputs.outcomes.iter().all(|o| o.is_ok());

    // Sources refreshed SUCCESSFULLY, each mapped to its covered-range claim (KTD2):
    // `None` = legacy scope-wide replacement; `Some(ranges)` = replacement gated to
    // `ranges ∩ scope`. A failed (or unmentioned) source is absent here and keeps its prior
    // evidence (retention).
    let ok_sources: HashMap<&str, Option<&[DateRange]>> = inputs
        .outcomes
        .iter()
        .filter(|o| o.is_ok())
        .map(|o| (o.source_id.as_str(), o.covered()))
        .collect();

    // The (source, date) pairs the fresh gather re-attested with a VALID record. A date a
    // source did NOT freshly witness — an empty/non-evidence response emits nothing, and an
    // explicitly-recorded absence marker is `valid == false` — is absent here, so the
    // never-retract-by-absence rule keeps the prior record even inside a covered range (KTD2).
    let fresh_dates: HashSet<(&str, NaiveDate)> = inputs
        .evidence
        .iter()
        .filter(|e| e.valid)
        .map(|e| (e.source_id.as_str(), e.date))
        .collect();

    // Merge evidence by id: retained prior (from non-refreshed / failed sources) + fresh. A
    // successful source's prior record is REPLACED (dropped in favor of the fresh gather)
    // ONLY when the gather actually re-attested that source+date. A partial re-gather (e.g.
    // an incremental refresh of a single date) that marks a source "ok" must NEVER retract
    // that source's prior positive witnesses on dates it did not re-cover; dropping them
    // wholesale would revert a proven-open day back to an inferred Closed ("absence never
    // retracts a prior positive witness", enforced here at the BUILD layer).
    //
    // A LEGACY (absent covered) source keeps the historical scope-wide replacement verbatim.
    // A source WITH covered ranges only replaces within `ranges ∩ scope`, and even there only
    // on a date it freshly witnessed — so a mis-windowed or empty-in-range response can never
    // silently retract a prior witness (KTD2).
    let mut evidence_by_id: BTreeMap<String, _> = BTreeMap::new();
    for e in &base.prior_evidence {
        let re_covered = match ok_sources.get(e.source_id.as_str()) {
            Some(covered) => {
                let in_scope = e.date >= scope.from && e.date <= scope.through;
                match covered {
                    // Legacy: scope-wide replacement (semantics unchanged).
                    None => in_scope,
                    // Gated: within a covered range AND freshly re-attested on this date.
                    Some(ranges) => {
                        in_scope
                            && ranges.iter().any(|r| r.contains(e.date))
                            && fresh_dates.contains(&(e.source_id.as_str(), e.date))
                    }
                }
            }
            None => false, // not a successful source → retained
        };
        if re_covered {
            continue; // replaced by the successful source's fresh gather for this date
        }
        evidence_by_id.insert(e.id.clone(), e.clone());
    }
    for e in &inputs.evidence {
        evidence_by_id.insert(e.id.clone(), e.clone());
    }
    let evidence: Vec<_> = evidence_by_id.into_values().collect();

    // Merge sources by id: base sources updated with the successful sources' records.
    let mut sources_by_id: BTreeMap<String, Source> =
        base.prior_sources.iter().map(|s| (s.id.clone(), s.clone())).collect();
    for s in &inputs.sources {
        sources_by_id.insert(s.id.clone(), s.clone());
    }
    let sources: Vec<Source> = sources_by_id.into_values().collect();

    // Coverage window: a failed source cannot expand coverage by absence. (For a genesis base
    // the seeds equal the genesis window, so these `min`/`max` are no-ops.)
    let materialized_from = if all_sources_ok {
        min(base.materialized_from, scope.from)
    } else {
        base.materialized_from
    };
    let materialized_through = if all_sources_ok {
        max(base.materialized_through, scope.through)
    } else {
        base.materialized_through
    };
    let retrospectively_checked_through = if all_sources_ok {
        materialized_through
    } else {
        base.retrospectively_checked_through
    };
    let scheduled_closure_evaluated_through = if all_sources_ok {
        materialized_through
    } else {
        base.scheduled_closure_evaluated_through
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
            base.freshness.holiday_facts_checked_at
        },
        full_history_reconciled_at: if mode == RefreshMode::FullHistory && all_sources_ok {
            Some(as_of)
        } else {
            base.freshness.full_history_reconciled_at
        },
        forward_readiness_through: base.freshness.forward_readiness_through,
        last_incremental_at: if mode == RefreshMode::Incremental && all_sources_ok {
            Some(as_of)
        } else {
            base.freshness.last_incremental_at
        },
    };

    let mut candidate = Snapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: base.predecessor_artifact_id.clone(),
        scope: base.scope.clone(),
        authorization: base.authorization.clone(),
        coverage: Coverage {
            materialized_from,
            materialized_through,
            retrospectively_checked_through,
            scheduled_closure_evaluated_through,
            source_availability: base.source_availability.clone(),
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

/// Everything [`build_from_base`] reads about the base a candidate is built ON (KTD1) —
/// extracted so the core runs branch-free for both the from-prior and genesis cases. For the
/// from-prior base every field mirrors the prior snapshot, so the core stays byte-identical to
/// the pre-genesis behavior; for genesis every seed is a merge identity ([`genesis_base`]).
struct BuildBase {
    /// The predecessor identity (`Some` from-prior, `None` for genesis — the one difference).
    predecessor_artifact_id: Option<String>,
    /// The calendar scope carried onto the candidate.
    scope: CalendarScope,
    /// The recorded authorization carried onto the candidate.
    authorization: Authorization,
    /// The per-source availability bounds carried onto the candidate coverage.
    source_availability: Vec<SourceAvailabilityBound>,
    /// The base's retained evidence (empty for genesis → retains nothing).
    prior_evidence: Vec<EvidenceRecord>,
    /// The base's sources (empty for genesis).
    prior_sources: Vec<Source>,
    /// Coverage seed: first materialized date (genesis: the window start → `min` no-op).
    materialized_from: NaiveDate,
    /// Coverage seed: last materialized date (genesis: the window end → `max` no-op).
    materialized_through: NaiveDate,
    /// Coverage seed used only on the not-all-ok branch (genesis is all-ok).
    retrospectively_checked_through: NaiveDate,
    /// Coverage seed used only on the not-all-ok branch (genesis is all-ok).
    scheduled_closure_evaluated_through: NaiveDate,
    /// Freshness seeds the aging falls back to (genesis: fully-stamped; `forward_readiness`
    /// and `last_incremental` pass through unchanged).
    freshness: Freshness,
}

impl BuildBase {
    /// The from-prior base — mirrors `prior` exactly so the shared core reproduces today's
    /// candidate byte-for-byte.
    fn from_prior(prior: &Snapshot) -> Self {
        BuildBase {
            predecessor_artifact_id: Some(prior.artifact_id.clone()),
            scope: prior.scope.clone(),
            authorization: prior.authorization.clone(),
            source_availability: prior.coverage.source_availability.clone(),
            prior_evidence: prior.evidence.clone(),
            prior_sources: prior.sources.clone(),
            materialized_from: prior.coverage.materialized_from,
            materialized_through: prior.coverage.materialized_through,
            retrospectively_checked_through: prior.coverage.retrospectively_checked_through,
            scheduled_closure_evaluated_through: prior.coverage.scheduled_closure_evaluated_through,
            freshness: prior.freshness.clone(),
        }
    }
}

/// The first civil date of the consumer window R12 guards (the #118 universe-capture start).
/// Genesis refuses any Unknown weekday from here through the last closed session at build
/// as-of. Exposed as a default the `calendar-genesis` bin passes; the window end is the build
/// as-of's last closed session, supplied per run.
pub const CONSUMER_WINDOW_START: (i32, u32, u32) = (2026, 5, 18);

/// The explicit inputs to a predecessor-less genesis build (KTD1). Enumerates the real scope,
/// authorization, source-availability, the full materialization window (history floor →
/// operating horizon), the KRX witness horizon (last closed session — the furthest KRX can
/// witness), and the R12 consumer window. Freshness seeds are derived deterministically from
/// `as_of` and `window.through` inside [`build_genesis`].
#[derive(Debug, Clone)]
pub struct GenesisParams {
    /// The real (non-synthetic) calendar scope.
    pub scope: CalendarScope,
    /// The real recorded authorization (authority label, granted/expires from the agreement).
    pub authorization: Authorization,
    /// Per-source availability bounds.
    pub source_availability: Vec<SourceAvailabilityBound>,
    /// The full materialization window: history floor (2010-01-04) → operating horizon end.
    pub window: DateRange,
    /// The last date KRX can witness (last closed session at build as-of). KRX coverage is
    /// required only through here; KASI/rule coverage is required through `window.through`.
    pub krx_through: NaiveDate,
    /// The consumer window R12 enforces zero-Unknown-weekday over (start → last closed session).
    pub consumer_window: DateRange,
}

/// A typed reason a genesis build was refused (never a partial or dishonest snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenesisRefusal {
    /// One or more consumer-window weekdays have no witness/closure evidence (R12, KTD6). The
    /// build refuses in code and names every uncovered date.
    UnknownConsumerWeekday {
        /// The uncovered consumer-window weekday dates, ascending.
        dates: Vec<NaiveDate>,
    },
    /// A source's covered claim does not span its required genesis window (KTD2 / AE9) — a
    /// mis-windowed or partial fetch can never be built into a loader-valid snapshot with an
    /// all-Unknown history. Names the source and the uncovered ranges.
    IncompleteCoverage {
        /// The source whose coverage fell short.
        source_id: String,
        /// The uncovered sub-ranges of the required window, ascending.
        uncovered: Vec<DateRange>,
    },
}

impl std::fmt::Display for GenesisRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenesisRefusal::UnknownConsumerWeekday { dates } => {
                let joined: Vec<String> = dates.iter().map(|d| d.to_string()).collect();
                write!(
                    f,
                    "genesis refused: {} consumer-window weekday(s) remain Unknown: {}",
                    dates.len(),
                    joined.join(", ")
                )
            }
            GenesisRefusal::IncompleteCoverage { source_id, uncovered } => {
                let joined: Vec<String> = uncovered
                    .iter()
                    .map(|r| format!("{}..{}", r.from, r.through))
                    .collect();
                write!(
                    f,
                    "genesis refused: source {source_id} coverage is incomplete; uncovered: {}",
                    joined.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for GenesisRefusal {}

/// Build a predecessor-less, non-synthetic, loader-valid genesis candidate from `params` +
/// `inputs` at `as_of` (KTD1, KTD6, R6/R7/R8/R12).
///
/// Refuses (never a partial build) when:
/// 1. a required source's covered ranges do not span its genesis window
///    ([`GenesisRefusal::IncompleteCoverage`], KTD2/AE9), or
/// 2. any consumer-window weekday remains Unknown after reconciliation
///    ([`GenesisRefusal::UnknownConsumerWeekday`], R12/KTD6).
///
/// On success the candidate has `predecessor_artifact_id == None`, `scope.synthetic == false`,
/// the real authorization stamped, and materializes every civil date in `params.window`.
pub fn build_genesis(
    params: &GenesisParams,
    inputs: &RefreshInputs,
    as_of: DateTime<Utc>,
) -> Result<Snapshot, GenesisRefusal> {
    // 1. Inputs completeness (KTD2 / AE9): a mis-windowed or partial fetch is refused BEFORE a
    //    snapshot is ever built — checked against the full genesis window, not just consumer.
    check_coverage_completeness(params, inputs)?;

    // 2. Build through the identical shared core with the genesis identity base.
    let base = genesis_base(params, as_of);
    let scope = RefreshScope {
        from: params.window.from,
        through: params.window.through,
    };
    let candidate = build_from_base(&base, inputs, &scope, RefreshMode::FullHistory, as_of);

    // 3. R12 (KTD6): every consumer-window weekday must be accounted for by a witness or an
    //    official/rule closure — a remaining Unknown weekday refuses, named.
    let mut unknown: Vec<NaiveDate> = candidate
        .rows
        .iter()
        .filter(|r| {
            params.consumer_window.contains(r.date)
                && is_weekday(r.date)
                && r.status == DayStatus::Unknown
        })
        .map(|r| r.date)
        .collect();
    if !unknown.is_empty() {
        unknown.sort();
        return Err(GenesisRefusal::UnknownConsumerWeekday { dates: unknown });
    }

    Ok(candidate)
}

/// The genesis identity base: no predecessor, empty retained evidence/sources, coverage seeds
/// equal to the window (so the core's `min`/`max` are no-ops), and fully-stamped freshness
/// seeds (`forward_readiness_through` at the window end; `last_incremental_at` absent).
fn genesis_base(params: &GenesisParams, as_of: DateTime<Utc>) -> BuildBase {
    BuildBase {
        predecessor_artifact_id: None,
        scope: params.scope.clone(),
        authorization: params.authorization.clone(),
        source_availability: params.source_availability.clone(),
        prior_evidence: Vec::new(),
        prior_sources: Vec::new(),
        materialized_from: params.window.from,
        materialized_through: params.window.through,
        retrospectively_checked_through: params.window.through,
        scheduled_closure_evaluated_through: params.window.through,
        freshness: Freshness {
            evidence_refreshed_at: as_of,
            holiday_facts_checked_at: Some(as_of),
            full_history_reconciled_at: Some(as_of),
            forward_readiness_through: Some(params.window.through),
            last_incremental_at: None,
        },
    }
}

/// Each required source (KRX daily through `krx_through`; KASI + generated rules through the
/// window end) must carry a covered claim that fully spans its required window. Absent (legacy)
/// coverage proves nothing for genesis and is refused. Adjudication/notice/correction sources
/// carry no span requirement.
fn check_coverage_completeness(
    params: &GenesisParams,
    inputs: &RefreshInputs,
) -> Result<(), GenesisRefusal> {
    for outcome in &inputs.outcomes {
        let required = match outcome.kind {
            SourceKind::KrxDailyMarket => DateRange::new(params.window.from, params.krx_through),
            SourceKind::KasiHoliday | SourceKind::KrxRule => {
                DateRange::new(params.window.from, params.window.through)
            }
            _ => continue,
        };
        match outcome.covered() {
            // Absent coverage proves nothing for genesis: the whole window is uncovered.
            None => {
                return Err(GenesisRefusal::IncompleteCoverage {
                    source_id: outcome.source_id.clone(),
                    uncovered: vec![required],
                })
            }
            Some(ranges) => {
                let uncovered = uncovered_within(required, ranges);
                if !uncovered.is_empty() {
                    return Err(GenesisRefusal::IncompleteCoverage {
                        source_id: outcome.source_id.clone(),
                        uncovered,
                    });
                }
            }
        }
    }
    Ok(())
}

/// `true` iff `date` is a Monday–Friday civil date (the R12 weekday test — weekends are Closed
/// by the weekend rule, not subject to the zero-Unknown gate).
fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}
