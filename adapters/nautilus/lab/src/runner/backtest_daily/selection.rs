//! The daily path's **pure** selection phase (KTD11): a function of the catalog, the
//! candidate-assembly parameters, and the ranking rule, with no engine dependency at
//! all. That independence is the point — its output is identical whether the engine
//! phase runs after it or not, which is what makes a selection regression separable
//! from an execution one.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use nautilus_ls::ingest::kst_to_unix_nanos;
use nautilus_ls::rules::KRX_REGULAR_OPEN;
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::instruments::InstrumentAny;

use crate::agent::envelope::{Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger};
use crate::agent::sink::DecisionSink;
use crate::params::OrbParams;
use crate::runner::backtest::{build_candidates, is_daily, kst_date_of};
use crate::strategy::orb::UniverseCandidate;

use super::in_range;

/// One session's pure selection output.
#[derive(Debug, Clone, PartialEq)]
pub struct DailySessionPlan {
    /// The KST session date.
    pub date: NaiveDate,
    /// The session-open `ts_event` the envelopes are stamped at.
    pub session_ts: u64,
    /// The ranked candidate symbols, best first. This is **not** a take: the
    /// take-top-`target_m`-minus-held step needs the held set and therefore runs
    /// per session in the engine phase (KTD16).
    pub ranked: Vec<String>,
    /// Each candidate's prior ATR for this session, keyed by symbol — the entry
    /// stop's only input (KTD9), carried on the plan because it is derived in the
    /// pure selection phase from the single catalog index (R5) and consumed in the
    /// engine phase, where the candidates no longer exist. An absent key was not a
    /// candidate this session; a `None` value was a candidate with no derivable
    /// prior ATR.
    pub prior_atr: BTreeMap<String, Option<f64>>,
    /// The session-open equity multiplier — fixed at exactly `1.0` on this path
    /// (KTD7). Compounding is on the no-build list, and preserving the ORB path's
    /// realized-P&L feedback edge would force the daily loop back into a
    /// per-session engine round trip for no registered benefit.
    pub equity_multiplier: f64,
}

/// The selection phase's whole output — a pure function of the catalog, the
/// candidate-assembly parameters, and the ranking rule. It has no engine dependency.
#[derive(Debug, Clone, PartialEq)]
pub struct DailySelection {
    /// Every in-range session, in date order.
    pub sessions: Vec<DailySessionPlan>,
    /// The deduped union of every symbol that was a candidate on any session.
    pub candidate_union: Vec<String>,
}

impl DailySelection {
    /// The chronological per-session ranked sequence.
    #[must_use]
    pub fn selection_sequence(&self) -> Vec<(NaiveDate, Vec<String>)> {
        self.sessions.iter().map(|s| (s.date, s.ranked.clone())).collect()
    }
}

/// The session-open equity multiplier on the daily path. Fixed at exactly `1.0`
/// (KTD7) and asserted by the selection phase, so no realized-P&L feedback edge can
/// creep back in and force a per-session engine round trip.
pub const DAILY_EQUITY_MULTIPLIER: f64 = 1.0;

/// Index the catalog once (R5): daily bars bucketed per instrument (ts-sorted, for
/// the prior/today lookup) and the in-range daily bars bucketed by KST date.
pub(super) fn index_daily<'a>(
    all_bars: &'a [Bar],
    start_ns: u64,
    end_ns: u64,
) -> (HashMap<InstrumentId, Vec<&'a Bar>>, HashMap<NaiveDate, Vec<&'a Bar>>) {
    let mut daily_by_inst: HashMap<InstrumentId, Vec<&Bar>> = HashMap::new();
    let mut daily_by_date: HashMap<NaiveDate, Vec<&Bar>> = HashMap::new();
    for b in all_bars {
        if !is_daily(b) {
            continue;
        }
        daily_by_inst.entry(b.bar_type.instrument_id()).or_default().push(b);
        if in_range(b, start_ns, end_ns) {
            daily_by_date.entry(kst_date_of(b)).or_default().push(b);
        }
    }
    for bars in daily_by_inst.values_mut() {
        bars.sort_by_key(|b| b.ts_event.as_u64());
    }
    (daily_by_inst, daily_by_date)
}

/// The distinct in-range daily session dates, in order — the same derivation the ORB
/// runner uses, read off the daily index rather than a second full-catalog scan.
pub(super) fn session_dates_of(daily_by_date: &HashMap<NaiveDate, Vec<&Bar>>) -> Vec<NaiveDate> {
    let mut dates: Vec<NaiveDate> = daily_by_date.keys().copied().collect();
    dates.sort();
    dates
}

/// Run the **pure** selection phase over the whole range (KTD11): per in-range
/// session build the candidates with the shared [`build_candidates`] assembly and
/// hand them to `rank`, emitting one universe envelope per candidate.
///
/// `select_universe` is deliberately not called — its gap gate and `universe_top_n`
/// cap are ORB's hypothesis (KTD15). Candidate *assembly* is shared so the two paths
/// cannot derive `prior_atr` differently; the *selection rule* is the caller's.
///
/// This phase touches no engine, which is what makes its output identical whether the
/// engine phase runs after it or not.
pub fn select_daily_sessions<R>(
    instruments: &[InstrumentAny],
    all_bars: &[Bar],
    params: &OrbParams,
    sink: &DecisionSink,
    start_ns: u64,
    end_ns: u64,
    rank: &R,
) -> anyhow::Result<DailySelection>
where
    R: Fn(&[UniverseCandidate]) -> Vec<String> + ?Sized,
{
    let (daily_by_inst, daily_by_date) = index_daily(all_bars, start_ns, end_ns);
    let session_dates = session_dates_of(&daily_by_date);
    select_from_index(instruments, &daily_by_inst, &session_dates, params, sink, rank)
}

/// [`select_daily_sessions`] over an already-built index — the form the combined
/// run uses so the catalog is indexed exactly once (R5).
pub(super) fn select_from_index<R>(
    instruments: &[InstrumentAny],
    daily_by_inst: &HashMap<InstrumentId, Vec<&Bar>>,
    session_dates: &[NaiveDate],
    params: &OrbParams,
    sink: &DecisionSink,
    rank: &R,
) -> anyhow::Result<DailySelection>
where
    R: Fn(&[UniverseCandidate]) -> Vec<String> + ?Sized,
{
    // The daily path mounts no minute bars (R2), so there is no opening-window volume
    // series to derive an RVOL baseline from. `build_candidates` reads the map only to
    // fill `prior_open_vol_mean`, which resolves to `None` — ORB's RVOL gate is not
    // this path's rule anyway (KTD15).
    let open_vol_by_inst: HashMap<InstrumentId, BTreeMap<NaiveDate, f64>> = HashMap::new();

    let mut sessions = Vec::with_capacity(session_dates.len());
    let mut candidate_union: BTreeSet<String> = BTreeSet::new();

    for date in session_dates {
        let session_ts = kst_to_unix_nanos(*date, KRX_REGULAR_OPEN)?.as_u64();
        let candidates = build_candidates(
            instruments,
            daily_by_inst,
            &open_vol_by_inst,
            params,
            *date,
            None,
        );
        for c in &candidates {
            candidate_union.insert(c.symbol.clone());
        }
        let ranked = rank(&candidates);
        let prior_atr: BTreeMap<String, Option<f64>> =
            candidates.iter().map(|c| (c.symbol.clone(), c.prior_atr)).collect();
        emit_universe_envelopes(sink, params, session_ts, &candidates, &ranked);

        // KTD7: fixed at exactly 1.0, and asserted here so a future edit that
        // reintroduces the realized-P&L feedback edge fails loudly rather than
        // quietly forcing the loop back into a per-session engine round trip.
        let equity_multiplier = DAILY_EQUITY_MULTIPLIER;
        assert_eq!(
            equity_multiplier, 1.0,
            "the daily path's session-open equity multiplier is fixed at exactly 1.0 (KTD7)"
        );

        sessions.push(DailySessionPlan {
            date: *date,
            session_ts,
            ranked,
            prior_atr,
            equity_multiplier,
        });
    }

    Ok(DailySelection { sessions, candidate_union: candidate_union.into_iter().collect() })
}

/// Emit one universe envelope per candidate for a session: `Accept` carrying the
/// rank for a ranked symbol, `Reject` naming the `unranked` filter for the rest.
/// Emission is symbol-ordered so the stream is byte-deterministic across runs
/// regardless of the catalog's instrument read order.
fn emit_universe_envelopes(
    sink: &DecisionSink,
    params: &OrbParams,
    session_ts: u64,
    candidates: &[UniverseCandidate],
    ranked: &[String],
) {
    let rank_of: HashMap<&str, usize> =
        ranked.iter().enumerate().map(|(i, s)| (s.as_str(), i)).collect();
    let mut ordered: Vec<&UniverseCandidate> = candidates.iter().collect();
    ordered.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    for c in ordered {
        let detail = match rank_of.get(c.symbol.as_str()) {
            Some(rank) => DecisionDetail::universe(
                c.symbol.clone(),
                Decision::Accept,
                None,
                BTreeMap::from([
                    ("rank".to_string(), *rank as f64),
                    ("prior_turnover".to_string(), c.prior_turnover),
                ]),
            ),
            None => DecisionDetail::universe(
                c.symbol.clone(),
                Decision::Reject,
                Some("unranked".to_string()),
                BTreeMap::from([("prior_turnover".to_string(), c.prior_turnover)]),
            ),
        };
        let counts = BTreeMap::from([("decisions".to_string(), sink.len() as u64)]);
        sink.emit(DecisionEnvelope::telemetry(
            session_ts,
            DecisionTrigger::StateChange {
                description: "daily universe selection scan".to_string(),
            },
            detail,
            params.telemetry_context(counts),
        ));
    }
}
