//! Daily-resolution, multi-session-hold backtest path (P7, U1) — the **second**,
//! additive runner beside [`crate::runner::backtest`] (KTD2). Where the ORB path
//! resets structurally every session (a fresh `BacktestEngine` per day over that
//! day's minute bars), this path drives **one** engine over a *streaming* daily bar
//! stream so an open position survives session boundaries (KTD1, R1).
//!
//! The loop splits into two phases:
//!
//! 1. **Selection (pure, no engine — KTD11).** Index the catalog once, derive the
//!    in-range session dates, build each session's candidates with the *shared*
//!    [`build_candidates`] assembly, and rank them with a caller-supplied rule.
//!    `select_universe` is deliberately NOT called: it encodes ORB's gap/top-N
//!    hypothesis, not this one (KTD15). Ranking must happen here rather than in a
//!    bar callback because `run_impl` delivers one datum at a time, so a
//!    cross-sectional rank computed on bar *k* of *N* is silently partial.
//! 2. **Engine (one engine, streaming).** `clear_data()` → `add_data(batch, sort)` →
//!    `run(.., streaming = true)` per session, `end()` once at the end, then a single
//!    cache read (R4). The venue is `OmsType::Hedging` (KTD12) so a symbol re-entered
//!    after a completed hold mints a *distinct* position instead of silently
//!    snapshotting the earlier round trip out of the live index (R19).
//!
//! The already-held set is engine state, and R4 forbids a per-session position-report
//! read, so the runner clones an [`OpenPositionBook`] handle off the strategy *before*
//! mounting it — the same shared-handle pattern `run_engine` uses for the entry-risk
//! ledger — and reads it between batches (KTD16). Because
//! `BacktestEngine::add_strategy` is a **sized generic**, the runner is generic over
//! the strategy type rather than taking a boxed factory; that is also what lets this
//! unit land and be tested before the daily strategy exists.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nautilus_backtest::config::{BacktestEngineConfig, SimulatedVenueConfig};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_common::actor::DataActorNative;
use nautilus_common::component::Component;
use nautilus_ls::ingest::{kst_to_unix_nanos, read_all_bars, read_all_instruments, BarKind};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::rules::KRX_REGULAR_OPEN;
use nautilus_model::data::{Bar, BarType, Data};
use nautilus_model::enums::{AccountType, BookType, OmsType};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, PositionId, Venue};
use nautilus_model::instruments::{Instrument, InstrumentAny};
use nautilus_model::position::Position;
use nautilus_model::types::{Currency, Money};
use nautilus_trading::strategy::{Strategy, StrategyNative};

use crate::agent::envelope::{Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger};
use crate::agent::sink::DecisionSink;
use crate::artifacts::data_quality::DataQualityReport;
use crate::artifacts::manifest::{
    range_fingerprint, universe_sequence_hash, DailyManifestParts, DataRange, Manifest,
};
use crate::artifacts::observation::{ObservationParts, RunObservation};
use crate::artifacts::performance::{ClientOrderEntryRiskLedger, EntryRisk, PerformanceReport};
use crate::artifacts::{run_id, RunSource, RunWriter};
use crate::params::OrbParams;
use crate::params_daily::DailyParams;
use crate::runner::backtest::{build_candidates, is_daily, kst_date_of};
use crate::strategy::daily::{
    rank_by_placeholder_signal, AdjustmentBasisShifts, DailyStrategy, PLACEHOLDER_RANKING_SIGNAL,
};
use crate::strategy::orb::UniverseCandidate;

// ---------------------------------------------------------------------------
// The shared open-position handle (KTD16)
// ---------------------------------------------------------------------------

/// The shared open-position book (KTD16) — the single authority for both the
/// already-held exclusion and per-session batch membership.
///
/// The strategy writes it from its position callbacks; the runner clones a handle
/// off the strategy **before** `add_strategy` consumes it and reads the handle
/// between batches. This is not the R4 position report: it is a live view of
/// *which symbols are open right now*, which the cumulative cache read cannot
/// answer between batches without a per-session read.
///
/// Deriving the held set statically from entry date + hold length was rejected: it
/// blocks re-entry after an early stop-out and so violates R10.
#[derive(Debug, Clone, Default)]
pub struct OpenPositionBook {
    inner: Arc<Mutex<BookState>>,
}

#[derive(Debug, Default)]
struct BookState {
    /// The instruments currently holding an open position.
    open: BTreeSet<InstrumentId>,
    /// Every position id observed opening across the stream, in observation order.
    /// The runner compares this against the single post-`end()` cache read, which is
    /// the check that catches a Netting venue silently snapshotting earlier round
    /// trips out of the live index (KTD12).
    opened: Vec<PositionId>,
}

impl OpenPositionBook {
    /// A fresh, empty book.
    #[must_use]
    pub fn new() -> Self {
        OpenPositionBook::default()
    }

    /// Record a position opening on `instrument_id`.
    pub fn record_opened(&self, instrument_id: InstrumentId, position_id: PositionId) {
        let mut st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        st.open.insert(instrument_id);
        st.opened.push(position_id);
    }

    /// Record the position on `instrument_id` closing.
    pub fn record_closed(&self, instrument_id: &InstrumentId) {
        let mut st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        st.open.remove(instrument_id);
    }

    /// The instruments currently holding an open position.
    #[must_use]
    pub fn held(&self) -> BTreeSet<InstrumentId> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).open.clone()
    }

    /// Whether `instrument_id` currently holds an open position.
    #[must_use]
    pub fn is_held(&self, instrument_id: &InstrumentId) -> bool {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).open.contains(instrument_id)
    }

    /// Every position id observed opening across the stream, in observation order.
    #[must_use]
    pub fn opened_position_ids(&self) -> Vec<PositionId> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).opened.clone()
    }
}

// ---------------------------------------------------------------------------
// The shared per-session signal handle (U4 — KTD9, KTD11, KTD13, R22)
// ---------------------------------------------------------------------------

/// What the loop resolved for one session, published to the strategy before that
/// session's batch runs.
///
/// A daily strategy cannot derive any of this from its own bar stream. The batch
/// carries only the session's *taken* and *held* symbols, so a freshly taken symbol
/// arrives with no prior bars at all — and the stop's prior ATR is computed strictly
/// **before** the session (KTD9). Re-deriving it inside the strategy would need a
/// second full-catalog index, which R5 forbids; the ranked/taken/held triple and the
/// ordered session calendar are likewise loop state, not stream state.
#[derive(Debug, Clone, PartialEq)]
pub struct DailySessionContext {
    /// The session's ordinal in the run's in-range session list. **This** is the
    /// clock hold elapsed is counted on (R23) — never a bar-callback counter, so a
    /// duplicate bar cannot shorten a frozen hold.
    pub index: usize,
    /// The KST session date.
    pub date: NaiveDate,
    /// The session's ranked candidates, best first, in instrument order.
    pub ranked: Vec<InstrumentId>,
    /// The symbols this session's pre-batch step actually took (R10).
    pub taken: Vec<InstrumentId>,
    /// The symbols already holding an open position at the pre-batch step.
    pub held: Vec<InstrumentId>,
    /// Each candidate's prior ATR for this session. An absent key was not a
    /// candidate; a `None` value was a candidate with no derivable prior ATR. The
    /// stop fails closed on both (KTD9).
    pub prior_atr: HashMap<InstrumentId, Option<f64>>,
}

/// The shared per-session signal handle: the runner publishes one
/// [`DailySessionContext`] per session before that session's batch runs, plus the
/// ordered in-range session calendar once before the loop.
///
/// Same shared-handle pattern as [`OpenPositionBook`] (KTD16) and for the same
/// reason — the runner clones it off the strategy before `add_strategy` consumes
/// it. The direction of travel is the opposite one: the runner *writes*, the
/// strategy *reads*.
#[derive(Debug, Clone, Default)]
pub struct DailySessionSignals {
    inner: Arc<Mutex<SignalState>>,
}

#[derive(Debug, Default)]
struct SignalState {
    /// Every in-range session date, in order — the calendar a prospective hold
    /// window is measured on (R22).
    sessions: Vec<NaiveDate>,
    current: Option<DailySessionContext>,
}

impl DailySessionSignals {
    /// A fresh, empty handle.
    #[must_use]
    pub fn new() -> Self {
        DailySessionSignals::default()
    }

    /// Publish the ordered in-range session calendar (once, before the loop).
    pub fn publish_sessions(&self, sessions: Vec<NaiveDate>) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).sessions = sessions;
    }

    /// Publish one session's context (once per session, before its batch runs).
    pub fn publish_session(&self, ctx: DailySessionContext) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).current = Some(ctx);
    }

    /// The session currently being driven, if the loop has published one.
    #[must_use]
    pub fn current(&self) -> Option<DailySessionContext> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).current.clone()
    }

    /// The in-range session date at `index`.
    #[must_use]
    pub fn session_at(&self, index: usize) -> Option<NaiveDate> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).sessions.get(index).copied()
    }

    /// The number of in-range sessions published.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).sessions.len()
    }
}

/// A strategy this runner can drive: it must expose the shared open-position book
/// so the runner can read the held set between batches (KTD16), and the shared
/// entry-risk ledger so the runner can project it into cache-read order (KTD3).
pub trait DailyPathStrategy {
    /// A clone of the shared open-position book.
    fn open_position_book(&self) -> OpenPositionBook;

    /// A clone of the shared, client-order-keyed entry-risk ledger (KTD3, R12).
    /// The runner clones it **before** `add_strategy` consumes the strategy and
    /// projects it into the order of the single post-`end()` cache read.
    fn entry_risk_ledger(&self) -> ClientOrderEntryRiskLedger;

    /// A clone of the shared per-session signal handle the runner publishes each
    /// session's [`DailySessionContext`] into.
    ///
    /// Defaulted to a detached handle so a strategy that drives itself entirely off
    /// batch membership (U1's test-only always-enter strategy) needs no wiring: the
    /// runner still publishes, nothing reads. A strategy whose stop needs the prior
    /// ATR, whose hold is counted in loop-supplied session ordinals, or whose entry
    /// gate reads the prospective hold window must override it and return a clone of
    /// its own handle.
    fn session_signals(&self) -> DailySessionSignals {
        DailySessionSignals::new()
    }
}

// ---------------------------------------------------------------------------
// The entry-risk projection seam (KTD3, R12)
// ---------------------------------------------------------------------------

/// The entry-risk ledger projected into the order of the single post-`end()` cache
/// read (KTD3, R12) — the `Vec<Option<EntryRisk>>` that
/// [`crate::artifacts::performance::PerformanceReport::from_positions_with_risk`]
/// consumes, plus the evidence its three seam assertions need.
///
/// `from_positions_with_risk` is **index-aligned, not keyed** (`risks.get(i)` for
/// `positions[i]`), and its doc records that a short slice silently leaves trailing
/// positions risk-less. Three distinct defects break three different statistics:
///
/// - **Truncation** sets `all_have_risk = false` and collapses `return_on_risk` to
///   `None` entirely.
/// - **Collapse** — several positions on a symbol joining to one risk — makes
///   `Σ risk_capital` and therefore net RoR wrong.
/// - **Mis-ordering** is a permutation: it leaves `Σ risk_capital` invariant and net
///   RoR unaffected, but corrupts per-trade `realized_r`, `mean_realized_r`, and the
///   per-symbol `max_risk_capital_share` fold.
///
/// Length equality catches the first, the `Some`-count catches the second, and only
/// the opening-order-id equality catches the third — a projection built in *ledger*
/// order rather than cache-read order has the right length and every slot filled.
///
/// The risk and the key it came from are held in **one** slot rather than two
/// parallel vectors, so no mutation can permute the risks while leaving the witness
/// keys in place.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryRiskProjection {
    slots: Vec<Option<(ClientOrderId, EntryRisk)>>,
    opened_entries: usize,
    unopened_entries: Vec<ClientOrderId>,
}

impl EntryRiskProjection {
    /// Assemble a projection from already-computed parts.
    ///
    /// Production code calls [`project_entry_risks`]; this constructor exists so the
    /// three seam assertions can be exercised against a deliberately broken
    /// projection (a shortened slice, a collapsed ledger, a permutation) without the
    /// test having to reimplement the join.
    #[must_use]
    pub fn from_parts(
        slots: Vec<Option<(ClientOrderId, EntryRisk)>>,
        opened_entries: usize,
        unopened_entries: Vec<ClientOrderId>,
    ) -> Self {
        EntryRiskProjection { slots, opened_entries, unopened_entries }
    }

    /// The index-aligned slots: slot `i` is the ledger entry that opened
    /// `positions[i]`, paired with the client order id it was keyed by.
    #[must_use]
    pub fn slots(&self) -> &[Option<(ClientOrderId, EntryRisk)>] {
        &self.slots
    }

    /// The index-aligned risk slice `from_positions_with_risk` consumes.
    #[must_use]
    pub fn risks(&self) -> Vec<Option<EntryRisk>> {
        self.slots.iter().map(|s| s.map(|(_, r)| r)).collect()
    }

    /// How many recorded ledger entries the **stream** observed opening a position.
    /// This is the reconciliation basis for the `Some`-count assertion, and it is
    /// read off the ledger's stream-side witness rather than off the cache read or
    /// the slots — so neither a defective projection nor a cache read that has
    /// silently lost positions can move its own expectation.
    #[must_use]
    pub const fn opened_entries(&self) -> usize {
        self.opened_entries
    }

    /// Recorded entries whose order never opened a position — a venue or risk-engine
    /// rejection, or an order that never filled. A **named run-level diagnostic**,
    /// deliberately not a hard failure: the `Some`-count assertion is defined over
    /// the entries that opened a position precisely so a rejection reports rather
    /// than aborting an otherwise valid run.
    #[must_use]
    pub fn unopened_entries(&self) -> &[ClientOrderId] {
        &self.unopened_entries
    }

    /// The three seam assertions (KTD3). Panics — this is a corrupt-join guard on the
    /// denominator of the verdict statistic, not a recoverable condition.
    pub fn assert_aligned(&self, positions: &[Position]) {
        // (1) Truncation. A short slice leaves trailing positions risk-less, which
        // sets `all_have_risk = false` and collapses `return_on_risk` to `None`.
        assert_eq!(
            self.slots.len(),
            positions.len(),
            "entry-risk projection length {} != position count {} (KTD3 assertion 1): a short \
             slice silently leaves trailing positions risk-less and collapses return_on_risk",
            self.slots.len(),
            positions.len()
        );

        // (2) Collapse. Several positions joining to one ledger entry passes (1) but
        // makes Σ risk_capital — the denominator of net RoR — wrong.
        let filled = self.slots.iter().filter(|s| s.is_some()).count();
        assert_eq!(
            filled,
            self.opened_entries,
            "entry-risk projection filled {filled} of {} slots but the stream observed {} \
             recorded ledger entries opening a position (KTD3 assertion 2): a collapsed join or \
             a cache read that lost positions makes Σ risk_capital and therefore net \
             return_on_risk wrong",
            self.slots.len(),
            self.opened_entries
        );

        // (3) Permutation. Right length, every slot filled, every risk on the wrong
        // position: Σ risk_capital is invariant and net RoR unaffected, but per-trade
        // realized_r, mean_realized_r, and the per-symbol max_risk_capital_share fold
        // are all corrupt. The key is already on the read side, so this costs nothing.
        for (i, slot) in self.slots.iter().enumerate() {
            let Some((key, _)) = slot else { continue };
            assert_eq!(
                *key,
                positions[i].opening_order_id,
                "entry-risk projection slot {i} carries client order id {key} but position {} \
                 was opened by {} (KTD3 assertion 3): the projection is a permutation of \
                 cache-read order, which corrupts realized_r and max_risk_capital_share while \
                 leaving Σ risk_capital invariant",
                positions[i].id,
                positions[i].opening_order_id
            );
        }
    }
}

/// Project the client-order-keyed entry-risk ledger into the order of the single
/// post-`end()` cache read (KTD3, R12), joining on `Position.opening_order_id`.
///
/// The projection is built by walking `positions` — the cache read defines the order
/// — and never by walking the ledger, which would produce a permutation that both
/// count assertions pass.
///
/// A position with no recorded entry resolves to `None` and takes the legacy P&L path
/// rather than panicking.
#[must_use]
pub fn project_entry_risks(
    positions: &[Position],
    ledger: &ClientOrderEntryRiskLedger,
) -> EntryRiskProjection {
    let by_order: HashMap<ClientOrderId, EntryRisk> = ledger.snapshot().into_iter().collect();

    let slots: Vec<Option<(ClientOrderId, EntryRisk)>> = positions
        .iter()
        .map(|p| by_order.get(&p.opening_order_id).map(|r| (p.opening_order_id, *r)))
        .collect();

    EntryRiskProjection {
        slots,
        // Stream-side, not cache-read-side: see `EntryRiskProjection::opened_entries`.
        opened_entries: ledger.opened_entries().len(),
        unopened_entries: ledger.unopened_entries(),
    }
}

/// One instrument mounted into the engine up front, with the daily bar series it is
/// driven on. Every instrument is mounted before the first batch (KTD11) so the loop
/// never adds an instrument mid-stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedSymbol {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument_id: InstrumentId,
    /// The daily bar type — `BarKind::Daily`, never `BarKind::Minute` (R2).
    pub bar_type: BarType,
}

// ---------------------------------------------------------------------------
// Selection phase (pure — KTD11)
// ---------------------------------------------------------------------------

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
fn index_daily<'a>(
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
fn session_dates_of(daily_by_date: &HashMap<NaiveDate, Vec<&Bar>>) -> Vec<NaiveDate> {
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
fn select_from_index<R>(
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

// ---------------------------------------------------------------------------
// Engine phase (one engine, streaming — KTD1)
// ---------------------------------------------------------------------------

/// The failure modes of the streaming loop that must abort the run rather than
/// finish green over a silently truncated position set.
#[derive(Debug, thiserror::Error)]
pub enum DailyEngineError {
    /// The first batch processed zero items. If the first-iteration block does not
    /// run, it re-runs on the next `run()` — minting a new run id, a new
    /// `backtest_start`, and calling `initialize_account()` mid-stream.
    #[error(
        "the first daily batch on {date} processed zero items (engine.iteration() == 0): the \
         engine's first-iteration block would re-run on the next batch, re-initializing the \
         account mid-stream"
    )]
    FirstBatchNoIteration {
        /// The session whose batch was submitted first.
        date: NaiveDate,
    },
    /// The engine finished mid-stream. A `force_stop` calls `end()` even under
    /// `streaming = true`, and because `iteration != 0` no later `run()` restarts the
    /// trader — the loop would then route bars into a stopped trader and finish green
    /// with a truncated position set.
    #[error(
        "the backtest engine finished mid-stream after session {date} (run_finished = \
         {finished_ns}): a force-stop ends the run even under streaming, and no later run() \
         restarts the trader — later sessions would be routed into a stopped trader"
    )]
    RunFinishedMidStream {
        /// The session after which the engine reported finished.
        date: NaiveDate,
        /// The engine's `run_finished` timestamp.
        finished_ns: u64,
    },
}

/// A duplicate daily bar dropped from a session batch (R23). A surviving
/// value-divergent duplicate would deliver two callbacks for one session, which
/// both shortens the frozen hold and fires the stop check twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateBarDrop {
    /// The session the duplicate was dropped from.
    pub date: NaiveDate,
    /// The instrument the duplicate belonged to.
    pub instrument_id: InstrumentId,
    /// The `ts_event` both copies shared.
    pub ts_event: u64,
    /// Whether the dropped copy's OHLCV differed from the kept one (a genuine
    /// adjustment-basis conflict, not a benign re-ingest overlap).
    pub divergent: bool,
}

/// What one session's pre-batch step resolved and submitted.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionBatch {
    /// The session date.
    pub date: NaiveDate,
    /// The symbols already holding an open position at the session's pre-batch step.
    pub held: Vec<InstrumentId>,
    /// The newly taken symbols: the top `target_m` of the ranked list from those not
    /// already held (R10).
    pub taken: Vec<InstrumentId>,
    /// The number of bars actually submitted after the R23 dedupe.
    pub bars: usize,
    /// Whether the whole `clear_data` / `add_data` / `run` cycle was skipped because
    /// the batch was empty (`add_data` errors on an empty slice).
    pub skipped: bool,
}

/// The outcome of a daily multi-session run.
#[derive(Debug, Clone)]
pub struct DailyRunOutcome {
    /// The single post-`end()` cache read (R4), in cache order.
    pub positions: Vec<Position>,
    /// The pure selection phase's output.
    pub selection: DailySelection,
    /// What each session's pre-batch step resolved.
    pub batches: Vec<SessionBatch>,
    /// Every duplicate bar dropped by the R23 dedupe.
    pub duplicate_drops: Vec<DuplicateBarDrop>,
    /// Every position id observed opening across the stream, in observation order.
    pub observed_position_ids: Vec<PositionId>,
    /// The entry-risk ledger projected into [`Self::positions`]' order (KTD3, R12),
    /// ready to hand straight to `from_positions_with_risk`. Already reconciled by
    /// the three seam assertions.
    pub entry_risks: Vec<Option<EntryRisk>>,
    /// Recorded entry orders that never opened a position — a venue or risk-engine
    /// rejection. A named diagnostic, not a failure (KTD3).
    pub unopened_entry_orders: Vec<ClientOrderId>,
}

/// The daily path's run configuration.
#[derive(Debug, Clone)]
pub struct DailyBacktestConfig {
    /// The data home (`<data>/catalog`, `<data>/runs`, …).
    pub data_home: PathBuf,
    /// The explicit pinned bar-data range.
    pub range: DataRange,
    /// The candidate-assembly parameter set — the knobs the *shared*
    /// [`build_candidates`] reads. The daily *selection rule* is the caller's `rank`
    /// (KTD15), not anything in here, and the sizing term is
    /// [`DailyParams::notional_per_position`], never anything in here (R27).
    ///
    /// Its `atr_window` is **not** authoritative on this path: [`Self::assembly_params`]
    /// forces it from [`DailyParams::atr_window_sessions`]. See that method for why.
    pub params: OrbParams,
    /// The daily parameter set — the frozen terms, the per-session take
    /// ([`DailyParams::target_m`], R10), and the source of the manifest's registry
    /// discriminator (KTD14). This is the *single* home for `target_m`: carrying a second
    /// copy on this struct bought only a runtime check that the two agreed.
    pub daily: DailyParams,
    /// Starting account balance (KRW).
    pub starting_balance: f64,
}

impl DailyBacktestConfig {
    /// A config over `data_home` for `[start, end]` (YYYYMMDD), taking the frozen daily
    /// terms with `target_m` overridden — a fixture may run *fewer* than the frozen 8,
    /// never more, and [`DailyParams::validate`] enforces that ceiling.
    pub fn new(data_home: impl Into<PathBuf>, start: &str, end: &str, target_m: usize) -> Self {
        DailyBacktestConfig {
            data_home: data_home.into(),
            range: DataRange { start: start.to_string(), end: end.to_string() },
            params: OrbParams::default(),
            daily: DailyParams { target_m, ..DailyParams::default() },
            starting_balance: 100_000_000.0,
        }
    }

    /// The candidate-assembly parameters actually handed to [`build_candidates`], with
    /// `atr_window` **forced** from [`DailyParams::atr_window_sessions`].
    ///
    /// This bridge is not a convenience. `OrbParams::atr_window` defaults to 14 — ORB's
    /// term, needing 15 prior sessions — while the frozen daily stop is `ATR(1)`. The two
    /// live in different structs and nothing else connects them, so an unbridged config
    /// derives a prior ATR that is absent for the first 14 in-range sessions of every
    /// symbol. Under the fail-closed stop (KTD9) that is not a visible misconfiguration:
    /// every entry is refused `atr_unavailable`, the run finalizes green with zero
    /// positions, and `return_on_risk` is vacuous. Forcing it here means the value is
    /// wrong in exactly one place or none.
    ///
    /// The forced value is what the manifest records as `params`, because it is what
    /// assembly ran with.
    #[must_use]
    pub fn assembly_params(&self) -> OrbParams {
        OrbParams { atr_window: self.daily.atr_window_sessions, ..self.params.clone() }
    }
}

/// Run the daily multi-session path to a position set.
///
/// The catalog read is async; **everything else** — selection and the whole engine
/// lifecycle — runs inside one [`tokio::task::spawn_blocking`] closure with owned
/// data moved in (R5), because the catalog and the engine both drive an internal
/// `block_on` and panic from an async context.
///
/// `make_strategy` receives every mounted symbol and returns the strategy; the runner
/// clones its [`OpenPositionBook`] before `add_strategy` consumes it (KTD16).
pub async fn run_daily<S, R, F>(
    cfg: DailyBacktestConfig,
    sink: DecisionSink,
    rank: R,
    make_strategy: F,
) -> anyhow::Result<DailyRunOutcome>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    R: Fn(&[UniverseCandidate]) -> Vec<String> + Send + 'static,
    F: FnOnce(&[MountedSymbol]) -> S + Send + 'static,
{
    let (_catalog_path, _guard) = acquire_catalog_guard(&cfg.data_home)?;
    run_daily_locked(cfg, sink, rank, make_strategy)
        .await
        .map(|locked| locked.outcome)
        .map_err(DailyRunFailure::into_error)
}

/// Check the catalog exists and take the ingest advisory lock over it.
///
/// Shared by the two entry points so they cannot drift on the refusal message, but
/// deliberately returning the guard rather than holding it: the whole difference between
/// [`run_daily`] and [`run_inner`] is the guard's *scope*. `run_daily` needs it for the
/// engine phase; `run_inner` must hold one continuous guard across the engine phase and
/// the finalize re-check, or it opens the very window that re-check exists to detect.
fn acquire_catalog_guard(data_home: &Path) -> anyhow::Result<(PathBuf, AdvisoryLock)> {
    let catalog_path = data_home.join("catalog");
    if !catalog_path.exists() {
        anyhow::bail!("no catalog at {} — ingest first", catalog_path.display());
    }
    // Own-guard: refuse if ingest is running, and hold the ingest lock so the catalog
    // cannot be mutated mid-run. Released on drop / at end of the run.
    let guard = AdvisoryLock::acquire(&catalog_path, LockKind::Ingest)
        .map_err(|e| anyhow::anyhow!("daily backtest refused — ingest/live in progress: {e}"))?;
    Ok((catalog_path, guard))
}

/// [`run_daily`]'s body with the advisory lock **already held by the caller**, plus the
/// range-scoped catalog fingerprint the finalize re-check compares against.
///
/// Split out because the lock's scope differs between the two entry points. [`run_daily`]
/// only needs it for the engine phase, but [`run_inner`] must hold one continuous guard
/// across the engine phase *and* the finalize re-check — re-acquiring it in a nested call
/// would either deadlock or, worse, open a window in which the catalog can be mutated
/// between the run and the re-check that exists to detect exactly that.
async fn run_daily_locked<S, R, F>(
    cfg: DailyBacktestConfig,
    sink: DecisionSink,
    rank: R,
    make_strategy: F,
) -> Result<LockedDailyRun, DailyRunFailure>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    R: Fn(&[UniverseCandidate]) -> Vec<String> + Send + 'static,
    F: FnOnce(&[MountedSymbol]) -> S + Send + 'static,
{
    let catalog_path = cfg.data_home.join("catalog");
    let start_date = parse_date(&cfg.range.start).map_err(DailyRunFailure::Refused)?;
    let end_date = parse_date(&cfg.range.end).map_err(DailyRunFailure::Refused)?;
    let start_ns = kst_to_unix_nanos(start_date, midnight())
        .map_err(|error| DailyRunFailure::Refused(error.into()))?
        .as_u64();
    let end_ns = kst_to_unix_nanos(end_date, end_of_day())
        .map_err(|error| DailyRunFailure::Refused(error.into()))?
        .as_u64();

    let instruments = read_all_instruments(&catalog_path)
        .await
        .map_err(|error| DailyRunFailure::Refused(error.into()))?;
    let all_bars = read_all_bars(&catalog_path)
        .await
        .map_err(|error| DailyRunFailure::Refused(error.into()))?;

    // The range-scoped catalog fingerprint at start. Taken here, before any engine work,
    // so the finalize re-check compares against the catalog the run actually read.
    let fingerprint_start = range_fingerprint(&all_bars, start_ns, end_ns);

    // The ATR bridge (see `DailyBacktestConfig::assembly_params`) — the frozen daily ATR
    // window reaches the shared assembly here, and nowhere else.
    let params = cfg.assembly_params();
    let starting_balance = cfg.starting_balance;
    let target_m = cfg.daily.target_m;
    let blocking = tokio::task::spawn_blocking(move || {
        run_daily_blocking(
            &instruments,
            &all_bars,
            &params,
            &sink,
            starting_balance,
            target_m,
            start_ns,
            end_ns,
            &rank,
            make_strategy,
        )
    })
    .await;
    let outcome = match blocking {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => return Err(DailyRunFailure::Aborted(error)),
        Err(error) => {
            return Err(DailyRunFailure::Aborted(anyhow::anyhow!(
                "daily blocking task aborted: {error}"
            )));
        }
    };

    Ok(LockedDailyRun { outcome, fingerprint_start, start_ns, end_ns })
}

/// Whether a failure occurred before the blocking engine started (a refusal) or after
/// ownership crossed that boundary (an abort). Only refusals remove run staging.
enum DailyRunFailure {
    Refused(anyhow::Error),
    Aborted(anyhow::Error),
}

impl DailyRunFailure {
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Refused(error) | Self::Aborted(error) => error,
        }
    }
}

/// [`run_daily_locked`]'s output: the run plus the finalize re-check's inputs.
struct LockedDailyRun {
    outcome: DailyRunOutcome,
    fingerprint_start: String,
    start_ns: u64,
    end_ns: u64,
}

// ---------------------------------------------------------------------------
// The finalized entry point (U5)
// ---------------------------------------------------------------------------

/// A finalized daily run: the registry artifacts plus the in-memory objects they were
/// written from, so a caller need not read the run back off disk to inspect it.
#[derive(Debug, Clone)]
pub struct DailyRunResult {
    /// The finalized run directory.
    pub run_dir: PathBuf,
    /// The run id — derived from the *daily* discriminator, never from
    /// `assembly_params.strategy_id` (KTD14).
    pub run_id: String,
    /// The manifest as written.
    pub manifest: Manifest,
    /// The performance report as written.
    pub performance: PerformanceReport,
    /// The typed run observation as written (U6).
    pub observation: RunObservation,
    /// The engine phase's full outcome.
    pub outcome: DailyRunOutcome,
}

/// Run the daily multi-session path to a **finalized registry run** (R18).
///
/// This is the daily sibling of [`crate::runner::backtest::run`], and it deliberately
/// duplicates that path's preamble and tail rather than generalizing it (KTD2): the
/// advisory lock, the range-fingerprint assert-and-re-check, and the artifact-writing
/// tail. The shared *candidate assembly* is reused at its current signature; the
/// selection rule, the venue, the OMS, and the hold semantics are this path's own.
pub async fn run(cfg: DailyBacktestConfig, start: DateTime<Utc>) -> anyhow::Result<DailyRunResult> {
    run_inner(cfg, start, std::future::ready(())).await
}

/// [`run`] with a hook awaited between the engine run and the finalize fingerprint
/// re-check — the library seam a test uses to simulate a mid-run catalog mutation. It is
/// deliberately **not** reachable through [`main_cli`]; the public [`run`] passes a no-op.
pub async fn run_inner<F: std::future::Future<Output = ()>>(
    cfg: DailyBacktestConfig,
    start: DateTime<Utc>,
    before_finalize: F,
) -> anyhow::Result<DailyRunResult> {
    // Fail fast on a parameter set off a frozen term, before any catalog or engine work.
    //
    // `Manifest::new_daily` is the *construction-point* gate and calls `validate()` again
    // — that one is what makes an invalid set unable to reach the registry through any
    // caller. This call is what keeps it from reaching the **engine**: without it a bad
    // config burns the whole run and only errors at manifest assembly, which for the
    // 837-session window is hours. Two named call sites, two different jobs.
    cfg.daily
        .validate()
        .map_err(|e| anyhow::anyhow!("invalid daily parameter set: {e}"))?;

    // ONE guard spanning the engine phase and the finalize re-check (see
    // `run_daily_locked`). Released on drop, at end of the run.
    let (catalog_path, _guard) = acquire_catalog_guard(&cfg.data_home)?;

    let assembly_params = cfg.assembly_params();
    let data_home = cfg.data_home.clone();
    let data_range = cfg.range.clone();
    let daily_params = cfg.daily.clone();
    let starting_balance = cfg.starting_balance;
    let expected_run_id = run_id(
        start,
        RunSource::Backtest,
        &daily_params.strategy_id,
        daily_params.strategy_version,
    );
    let writer = RunWriter::new(&data_home, &expected_run_id)?;
    let sink = match writer.stream_decisions() {
        Ok(sink) => sink,
        Err(error) => return Err(discard_staging(writer, error)),
    };

    // The R22 gate's input. A catalog with no checkpoint genuinely has no recorded shift
    // ledger, which `AdjustmentBasisShifts::none` states explicitly rather than implying.
    let shifts = match crate::runner::backtest::load_checkpoint(&catalog_path) {
        Some(cp) => AdjustmentBasisShifts::from_checkpoint(&cp),
        None => AdjustmentBasisShifts::none(),
    };
    let make_strategy =
        DailyStrategy::factory(daily_params.clone(), sink.clone(), shifts);

    // The placeholder ranking signal (KTD6). It is named and marked, not scaffolding: the
    // signal that carries the hypothesis is turn one's act, and U6's observation refuses to
    // yield judgment arguments while the marker is set.
    let locked = run_daily_locked(
        cfg,
        sink.clone(),
        rank_by_placeholder_signal,
        make_strategy,
    )
    .await;
    let (writer, locked) = resolve_locked_run(writer, locked)?;

    // Test hook: simulate any mid-run catalog mutation before the finalize re-check.
    before_finalize.await;

    // Re-check the fingerprint at finalize: a mid-run catalog mutation invalidates the run
    // and leaves NO registry residue. The decision stream opened staging before selection,
    // so this graceful refusal explicitly removes it; a crash still leaves the normal
    // `.tmp-` aborted-run marker.
    let all_bars_end = match read_all_bars(&catalog_path).await {
        Ok(bars) => bars,
        Err(error) => return Err(discard_staging(writer, error.into())),
    };
    let fingerprint_end = range_fingerprint(&all_bars_end, locked.start_ns, locked.end_ns);
    if fingerprint_end != locked.fingerprint_start {
        return Err(discard_staging(
            writer,
            anyhow::anyhow!(
                "catalog changed in-range during the daily run — aborting with no registry residue"
            ),
        ));
    }

    finalize_daily_run(FinalizeDaily {
        writer,
        started_utc: start,
        outcome: locked.outcome,
        fingerprint_start: locked.fingerprint_start,
        assembly_params,
        daily_params,
        data_range,
        starting_balance,
        catalog_path: &catalog_path,
    })
}

fn resolve_locked_run(
    writer: RunWriter,
    result: Result<LockedDailyRun, DailyRunFailure>,
) -> anyhow::Result<(RunWriter, LockedDailyRun)> {
    match result {
        Ok(locked) => Ok((writer, locked)),
        Err(DailyRunFailure::Refused(error)) => Err(discard_staging(writer, error)),
        Err(DailyRunFailure::Aborted(error)) => Err(error),
    }
}

fn discard_staging(writer: RunWriter, error: anyhow::Error) -> anyhow::Error {
    match writer.discard() {
        Ok(()) => error,
        Err(discard_error) => anyhow::anyhow!(
            "{error}; additionally failed to remove the refused run's staging directory: {discard_error}"
        ),
    }
}

/// Everything [`finalize_daily_run`] needs. A struct rather than eleven positional
/// arguments, matching [`crate::artifacts::manifest::DailyManifestParts`]'s reasoning.
struct FinalizeDaily<'a> {
    writer: RunWriter,
    started_utc: DateTime<Utc>,
    outcome: DailyRunOutcome,
    fingerprint_start: String,
    assembly_params: OrbParams,
    daily_params: DailyParams,
    data_range: DataRange,
    starting_balance: f64,
    catalog_path: &'a Path,
}

/// Assemble and write the run's artifacts, then finalize the run directory.
fn finalize_daily_run(p: FinalizeDaily<'_>) -> anyhow::Result<DailyRunResult> {
    let checkpoint = crate::runner::backtest::load_checkpoint(p.catalog_path);

    // Zero-rate assembly params yield `None` — the pre-model path. A daily-path
    // transaction-cost model is not this plan's scope; an explicitly-rated config is
    // still honoured rather than silently zeroed.
    let cost_model = crate::strategy::orb::TransactionCostModel::from_params(&p.assembly_params);
    let performance = PerformanceReport::from_positions_with_risk(
        &p.outcome.positions,
        &p.outcome.entry_risks,
        p.starting_balance,
        cost_model.as_ref(),
    );

    // The candidate union is this path's universe snapshot. Intersecting it with the
    // checkpoint's unhealed shift marks is what makes an adjustment-basis rewrite inside a
    // hold *visible* on the run — the risk R22 refuses per position, reported here per run.
    let candidate_union = p.outcome.selection.candidate_union.clone();
    // The in-range session calendar, from the selection phase rather than from the trades:
    // a session with no activity must still appear as a zero row in the series, or a
    // session-block bootstrap resamples a shortened series and understates the error.
    let session_dates: Vec<chrono::NaiveDate> =
        p.outcome.selection.sessions.iter().map(|s| s.date).collect();
    let shift_symbols: Vec<String> = checkpoint
        .as_ref()
        .map(|c| {
            c.shifted_instruments(crate::strategy::daily::DAILY_BAR_TYPE_LABEL)
                .into_iter()
                .filter(|s| candidate_union.contains(s))
                .collect()
        })
        .unwrap_or_default();
    let mut data_quality = DataQualityReport::backtest(candidate_union, shift_symbols);
    // R23's dropped duplicates have to reach a FINALIZED artifact, not just the in-memory
    // outcome. A value-divergent duplicate is the adjustment-basis conflict this path is
    // most exposed to — two bars for one instrument at one `ts_event` whose OHLCV disagree,
    // where the kept copy is simply whichever the catalog yielded first. That choice can
    // move an entry, a stop, and therefore the run's statistic. Recorded here so a run
    // carrying one cannot read as clean; `dedup_hits` is deliberately NOT reused, because it
    // is documented as the live path's ORDER-dedup counter and overloading it would make two
    // different quantities indistinguishable to a reader.
    if !p.outcome.duplicate_drops.is_empty() {
        let divergent: Vec<&DuplicateBarDrop> =
            p.outcome.duplicate_drops.iter().filter(|d| d.divergent).collect();
        data_quality.observations.push(format!(
            "R23 duplicate daily bars dropped: {} total, {} value-divergent",
            p.outcome.duplicate_drops.len(),
            divergent.len()
        ));
        // Name the divergent ones individually — they are the ones that change prices.
        const MAX_LISTED: usize = 20;
        for d in divergent.iter().take(MAX_LISTED) {
            data_quality.observations.push(format!(
                "value-divergent duplicate bar: {} on {} (ts_event {}) — the kept copy is the \
                 first in catalog order; re-ingest may have spliced two adjustment bases",
                d.instrument_id, d.date, d.ts_event
            ));
        }
        if divergent.len() > MAX_LISTED {
            data_quality.observations.push(format!(
                "... and {} further value-divergent duplicates not listed",
                divergent.len() - MAX_LISTED
            ));
        }
    }

    // Every identity-bearing field is derived by the constructor, not passed: `strategy_id`,
    // `strategy_version`, `run_id`, and `strategy_code_hash` all come off `daily_params` and
    // `DAILY_SOURCE`. `assembly_params` is recorded verbatim in the non-optional
    // `Manifest.params` because that field cannot express "this run has no OrbParams" —
    // it is the parameter set the *shared candidate assembly* ran with, and nothing else.
    // Its `strategy_id` still reads "orb" and is deliberately ignored here; U8's filters key
    // on `Manifest.strategy_id`, which `new_daily` takes from the daily discriminator, so
    // this recorded set can never be selected as an ORB baseline.
    let manifest = match Manifest::new_daily(DailyManifestParts {
        daily: p.daily_params,
        assembly_params: p.assembly_params,
        daily_source: crate::strategy::DAILY_SOURCE,
        started_utc: p.started_utc,
        data_range: p.data_range,
        catalog_fingerprint: p.fingerprint_start,
        universe_hash: universe_sequence_hash(&p.outcome.selection.selection_sequence()),
        lab_src_fingerprint: Some(crate::fingerprint::EMBEDDED.to_string()),
        checkpoint_hash: crate::runner::backtest::checkpoint_hash(p.catalog_path),
        universe_metadata_hash: None,
    }) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Err(discard_staging(
                p.writer,
                anyhow::anyhow!("daily manifest refused: {error}"),
            ));
        }
    };

    // The fifth artifact (U6). An R25 refusal removes the streaming staging directory:
    // a refusal is not an aborted run, while an actual crash still leaves the marker.
    let observation = match RunObservation::build(ObservationParts {
        run_id: &manifest.run_id,
        data_range: &manifest.data_range,
        catalog_fingerprint: &manifest.catalog_fingerprint,
        performance: &performance,
        session_dates: &session_dates,
        ranking_signal: PLACEHOLDER_RANKING_SIGNAL.name,
        ranking_signal_is_placeholder: PLACEHOLDER_RANKING_SIGNAL.placeholder,
    }) {
        Ok(observation) => observation,
        Err(error) => {
            return Err(discard_staging(p.writer, anyhow::anyhow!("{error}")));
        }
    };

    debug_assert_eq!(p.writer.run_id(), manifest.run_id);
    p.writer.write_manifest(&manifest)?;
    p.writer.write_performance(&performance)?;
    p.writer.write_data_quality(&data_quality)?;
    p.writer.write_observation(&observation)?;
    let run_dir = p.writer.finalize()?;

    Ok(DailyRunResult {
        run_dir,
        run_id: manifest.run_id.clone(),
        manifest,
        performance,
        observation,
        outcome: p.outcome,
    })
}

/// Parse a required environment variable, naming it in both failure modes.
///
/// The silent-default anti-pattern this refuses is `parse().unwrap_or(1)` at
/// `backtest.rs:1049`, pinned as an anti-pattern by `research_cli.rs:430`: a typo'd
/// `LS_BTD_TARGET_M=8x` would there become a *valid* run at a concurrency nobody chose,
/// and the manifest would record the substituted value as though it were intended.
fn env_parsed<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T> {
    match std::env::var(key) {
        Err(_) => Ok(default),
        Ok(raw) => raw.trim().parse::<T>().map_err(|_| {
            anyhow::anyhow!(
                "{key} must parse as {}, got {raw:?} — refusing rather than defaulting",
                std::any::type_name::<T>()
            )
        }),
    }
}

/// CLI entry point for the `lab-backtest-daily` bin (R18). Reads config from env:
/// `LS_DATA_HOME`, `LS_BTD_SDATE`, `LS_BTD_EDATE` (required); `LS_BTD_TARGET_M`,
/// `LS_BTD_BALANCE`, `LS_BTD_NOTIONAL`, `LS_BTD_VERSION` (optional).
///
/// Every optional numeric variable hard-errors on a malformed value rather than
/// defaulting. The frozen terms — hold, directionality, stop multiple, ATR window — take
/// **no** environment override at all: they are frozen, and a run that could move one from
/// the shell is a run that can drift off the pre-registration without a code change.
///
/// The `before_finalize` seam of [`run_inner`] is deliberately unreachable from here.
pub fn main_cli() -> anyhow::Result<()> {
    nautilus_ls::scrub::install();
    let data_home =
        std::env::var("LS_DATA_HOME").map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required"))?;
    let sdate =
        std::env::var("LS_BTD_SDATE").map_err(|_| anyhow::anyhow!("LS_BTD_SDATE is required"))?;
    let edate =
        std::env::var("LS_BTD_EDATE").map_err(|_| anyhow::anyhow!("LS_BTD_EDATE is required"))?;

    let target_m = env_parsed("LS_BTD_TARGET_M", crate::params_daily::FROZEN_TARGET_M)?;
    let mut cfg = DailyBacktestConfig::new(&data_home, &sdate, &edate, target_m);
    cfg.starting_balance = env_parsed("LS_BTD_BALANCE", cfg.starting_balance)?;
    cfg.daily.notional_per_position =
        env_parsed("LS_BTD_NOTIONAL", cfg.daily.notional_per_position)?;
    cfg.daily.strategy_version = env_parsed("LS_BTD_VERSION", cfg.daily.strategy_version)?;

    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(run(cfg, Utc::now()))?;
    // A trailing summary printed AFTER the engine logs, so the only operator-relevant
    // output never scrolls away under nautilus's ~8,900 INFO lines per session.
    print!("{}", daily_summary_block(&result));
    Ok(())
}

/// The `lab-backtest-daily` trailing summary block.
#[must_use]
pub fn daily_summary_block(result: &DailyRunResult) -> String {
    let censored = result
        .outcome
        .positions
        .iter()
        .filter(|p| p.is_open())
        .count();
    format!(
        "\n=== lab-backtest-daily summary ===\nrun:       {}\nstrategy:  {} v{}\nsessions:  {}\n\
         positions: {} ({censored} open at range end)\nunopened:  {}\ndir:       {}\n",
        result.run_id,
        result.manifest.strategy_id,
        result.manifest.strategy_version,
        result.outcome.selection.sessions.len(),
        result.outcome.positions.len(),
        result.outcome.unopened_entry_orders.len(),
        result.run_dir.display(),
    )
}

/// The whole daily lifecycle on one blocking thread: index once, run the pure
/// selection phase, then drive the streaming engine phase.
#[allow(clippy::too_many_arguments)]
fn run_daily_blocking<S, R, F>(
    instruments: &[InstrumentAny],
    all_bars: &[Bar],
    params: &OrbParams,
    sink: &DecisionSink,
    starting_balance: f64,
    target_m: usize,
    start_ns: u64,
    end_ns: u64,
    rank: &R,
    make_strategy: F,
) -> anyhow::Result<DailyRunOutcome>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    R: Fn(&[UniverseCandidate]) -> Vec<String> + ?Sized,
    F: FnOnce(&[MountedSymbol]) -> S,
{
    let (daily_by_inst, daily_by_date) = index_daily(all_bars, start_ns, end_ns);
    let session_dates = session_dates_of(&daily_by_date);
    let selection =
        select_from_index(instruments, &daily_by_inst, &session_dates, params, sink, rank)?;

    let engine_out = engine_phase(
        instruments,
        &daily_by_date,
        &selection,
        target_m,
        starting_balance,
        make_strategy,
    )?;

    Ok(DailyRunOutcome {
        positions: engine_out.positions,
        selection,
        batches: engine_out.batches,
        duplicate_drops: engine_out.duplicate_drops,
        observed_position_ids: engine_out.observed_position_ids,
        entry_risks: engine_out.entry_risk_projection.risks(),
        unopened_entry_orders: engine_out.entry_risk_projection.unopened_entries().to_vec(),
    })
}

struct EnginePhase {
    positions: Vec<Position>,
    batches: Vec<SessionBatch>,
    duplicate_drops: Vec<DuplicateBarDrop>,
    observed_position_ids: Vec<PositionId>,
    entry_risk_projection: EntryRiskProjection,
}

/// Drive every in-range session through the engine.
fn engine_phase<S, F>(
    instruments: &[InstrumentAny],
    daily_by_date: &HashMap<NaiveDate, Vec<&Bar>>,
    selection: &DailySelection,
    target_m: usize,
    starting_balance: f64,
    make_strategy: F,
) -> anyhow::Result<EnginePhase>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    F: FnOnce(&[MountedSymbol]) -> S,
{
    let by_symbol: HashMap<String, InstrumentId> =
        instruments.iter().map(|i| (i.id().to_string(), i.id())).collect();

    // ONE engine for the whole stream (KTD1). `clear_data` resets only data-iteration
    // state and leaves the kernel cache intact, so positions survive across batches.
    // A hoisted engine with `reset()` between sessions was rejected: `reset` clears
    // the cache and defeats the purpose.
    let (mut engine, mounted) = build_engine(instruments, starting_balance)?;
    let strategy = make_strategy(&mounted);
    // Clone the open-position handle BEFORE `add_strategy` consumes the strategy
    // (KTD16) — the same shared-handle pattern `run_engine` uses for the entry-risk
    // ledger.
    let book = strategy.open_position_book();
    // Likewise the client-order-keyed entry-risk ledger (KTD3): the runner projects
    // it into cache-read order after the single post-`end()` read.
    let risk_ledger = strategy.entry_risk_ledger();
    // And the per-session signal handle (U4): the runner *writes* this one. The
    // ordered session calendar is published once, before the loop — a prospective
    // hold window is measured on it (R22).
    let signals = strategy.session_signals();
    signals.publish_sessions(selection.sessions.iter().map(|s| s.date).collect());
    engine.add_strategy(strategy)?;

    let mut batches: Vec<SessionBatch> = Vec::new();
    let mut duplicate_drops: Vec<DuplicateBarDrop> = Vec::new();
    let mut ran = 0usize;

    for (index, plan) in selection.sessions.iter().enumerate() {
        // The held set is engine state, read from the shared handle between batches —
        // never a per-session position-report read (R4).
        let held = book.held();
        let taken = resolve_take(&plan.ranked, &held, &by_symbol, target_m);
        let mut wanted: BTreeSet<InstrumentId> = held.clone();
        wanted.extend(taken.iter().copied());

        // Publish what this session resolved BEFORE its batch runs, so the strategy's
        // first bar callback of the session already sees the session ordinal, the
        // take, and the prior ATRs its stop gate needs (KTD9).
        signals.publish_session(DailySessionContext {
            index,
            date: plan.date,
            ranked: plan
                .ranked
                .iter()
                .filter_map(|s| by_symbol.get(s.as_str()).copied())
                .collect(),
            taken: taken.clone(),
            held: held.iter().copied().collect(),
            prior_atr: plan
                .prior_atr
                .iter()
                .filter_map(|(s, a)| by_symbol.get(s.as_str()).map(|id| (*id, *a)))
                .collect(),
        });

        let batch = build_batch(
            daily_by_date.get(&plan.date).map(|v| v.as_slice()).unwrap_or_default(),
            &wanted,
            plan.date,
            &mut duplicate_drops,
        );

        let bars = batch.len();
        let mut record = SessionBatch {
            date: plan.date,
            held: held.into_iter().collect(),
            taken,
            bars,
            skipped: true,
        };

        // Skip the whole cycle on an empty batch — `add_data` errors on an empty slice.
        if batch.is_empty() {
            batches.push(record);
            continue;
        }
        record.skipped = false;

        // `clear_data` is the documented streaming step and is what makes the `None`
        // bounds below resolve to *this* batch's own range. Note for future readers:
        // it is NOT what carries positions across sessions — `add_data` opens a fresh
        // named stream with its own cursor each call, so an exhausted batch is never
        // re-delivered even without it. Its load-bearing effects are the batch-scoped
        // `ts_first`/`ts_last_data` and not retaining every batch's `Vec<Data>` for the
        // whole 837-session run.
        engine.clear_data();
        // `sort = true` on EVERY call: `self.sorted = sort` is an assignment, not an
        // OR, so the last `add_data` of a batch decides whether `run` accepts it.
        engine.add_data(batch.into_iter().map(Data::Bar).collect(), None, true, true)?;
        // `None` bounds, NOT the pinned range: `run_impl` sets `last_ns = start_ns` and
        // calls `set_all_clocks_time` unconditionally *before* the `iteration == 0`
        // gate, so passing the range on every batch would rewind every component clock
        // each time. `clear_data` nulls `ts_first`/`ts_last_data` and `add_data`
        // recomputes them per batch, so `None` resolves to this batch's own bounds.
        // Range pinning lives in which bars are added, not in these arguments.
        engine.run(None, None, None, true)?;
        ran += 1;

        // If the first batch processes zero items the first-iteration block re-runs on
        // the next call — a new run id, a new `backtest_start`, and an
        // `initialize_account()` mid-stream.
        if ran == 1 && engine.iteration() == 0 {
            return Err(DailyEngineError::FirstBatchNoIteration { date: plan.date }.into());
        }
        // A `force_stop` calls `end()` even under `streaming = true`, and because
        // `iteration != 0` no later `run()` restarts the trader: the loop would route
        // later sessions' bars into a stopped trader and finish green over a truncated
        // position set. Abort with a typed error instead.
        if let Some(finished) = engine.run_finished() {
            return Err(DailyEngineError::RunFinishedMidStream {
                date: plan.date,
                finished_ns: finished.as_u64(),
            }
            .into());
        }

        batches.push(record);
    }

    // Finalize the streaming run, then read the cache EXACTLY ONCE (R4). Guarded on at
    // least one batch: with zero batches the trader was never started (the
    // first-iteration block never ran), so there is nothing to end.
    if ran > 0 {
        engine.end();
    }
    let positions: Vec<Position> = engine
        .kernel()
        .cache
        .borrow()
        .positions(None, None, None, None, None)
        .into_iter()
        .map(|p| p.cloned())
        .collect();

    // Project the client-order-keyed ledger into THAT read's order and reconcile it
    // (KTD3). The projection is built here, against the one cache read that defines
    // position order — never in ledger order, which passes both count checks while
    // attaching every risk to the wrong position.
    let entry_risk_projection = project_entry_risks(&positions, &risk_ledger);
    entry_risk_projection.assert_aligned(&positions);

    Ok(EnginePhase {
        positions,
        batches,
        duplicate_drops,
        observed_position_ids: book.opened_position_ids(),
        entry_risk_projection,
    })
}

/// Build the engine and mount every instrument up front (KTD11), returning the
/// mounted universe paired with each instrument's daily bar type (R2).
///
/// The venue is `OmsType::Hedging` (KTD12). Under `Netting` the position id is
/// `{instrument_id}-{strategy_id}` — one constant id per symbol for the whole run —
/// so re-entering a symbol takes the `reopen_position` path, which snapshots the
/// closed position out of the live index that `cache.positions()` reads. Every
/// earlier round trip would disappear from the run with no diagnostic, and
/// concurrent legs would merge, destroying the entry-fixed stop.
fn build_engine(
    instruments: &[InstrumentAny],
    starting_balance: f64,
) -> anyhow::Result<(BacktestEngine, Vec<MountedSymbol>)> {
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        bypass_logging: true,
        ..Default::default()
    })?;
    engine.add_venue(
        SimulatedVenueConfig::builder()
            .venue(Venue::from(nautilus_ls::KRX_VENUE))
            .oms_type(OmsType::Hedging)
            .account_type(AccountType::Margin)
            .base_currency(Currency::KRW())
            .book_type(BookType::L1_MBP)
            .starting_balances(vec![Money::new(starting_balance, Currency::KRW())])
            .build()
            .map_err(|e| anyhow::anyhow!("venue build: {e}"))?,
    )?;
    let mut mounted = Vec::with_capacity(instruments.len());
    for inst in instruments {
        engine.add_instrument(inst)?;
        mounted.push(MountedSymbol {
            instrument_id: inst.id(),
            bar_type: BarKind::Daily.bar_type(inst.id()).map_err(|e| anyhow::anyhow!(e))?,
        });
    }
    Ok((engine, mounted))
}

/// R10: take the top `target_m` of the ranked list **from those not already held**.
/// The already-held exclusion runs first — a held symbol is `Excluded`, and the
/// remaining ranked symbols past `target_m` are `Passed`.
fn resolve_take(
    ranked: &[String],
    held: &BTreeSet<InstrumentId>,
    by_symbol: &HashMap<String, InstrumentId>,
    target_m: usize,
) -> Vec<InstrumentId> {
    ranked
        .iter()
        .filter_map(|s| by_symbol.get(s.as_str()).copied())
        .filter(|id| !held.contains(id))
        .take(target_m)
        .collect()
}

/// Build one session's batch: **every symbol with an open position plus that
/// session's newly taken symbols** (step 4). A held position must receive its daily
/// bar on every session of its hold or the venue cannot price it and the stop never
/// evaluates.
///
/// Deduped to one bar per instrument per distinct `ts_event` (R23), recording each
/// drop and whether it was value-divergent. The first copy in catalog order wins.
fn build_batch(
    session_bars: &[&Bar],
    wanted: &BTreeSet<InstrumentId>,
    date: NaiveDate,
    drops: &mut Vec<DuplicateBarDrop>,
) -> Vec<Bar> {
    let mut kept: BTreeMap<(InstrumentId, u64), Bar> = BTreeMap::new();
    for b in session_bars {
        let id = b.bar_type.instrument_id();
        if !wanted.contains(&id) {
            continue;
        }
        let key = (id, b.ts_event.as_u64());
        match kept.get(&key) {
            None => {
                kept.insert(key, (*b).clone());
            }
            Some(existing) => drops.push(DuplicateBarDrop {
                date,
                instrument_id: id,
                ts_event: b.ts_event.as_u64(),
                divergent: !bars_equal(existing, b),
            }),
        }
    }
    kept.into_values().collect()
}

/// Whether two bars carry identical OHLCV (the divergence test for a dropped
/// duplicate — a differing copy is an adjustment-basis conflict, not a benign
/// re-ingest overlap).
fn bars_equal(a: &Bar, b: &Bar) -> bool {
    a.open == b.open
        && a.high == b.high
        && a.low == b.low
        && a.close == b.close
        && a.volume == b.volume
}

fn in_range(b: &Bar, start_ns: u64, end_ns: u64) -> bool {
    let ts = b.ts_event.as_u64();
    ts >= start_ns && ts <= end_ns
}

fn parse_date(s: &str) -> anyhow::Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s.trim(), "%Y%m%d")?)
}
fn midnight() -> NaiveTime {
    NaiveTime::from_hms_opt(0, 0, 0).unwrap()
}
fn end_of_day() -> NaiveTime {
    NaiveTime::from_hms_opt(23, 59, 59).unwrap()
}

#[cfg(test)]
mod failure_classification_tests {
    use super::*;
    use crate::artifacts::aborted_runs;

    #[test]
    fn abnormal_engine_failure_keeps_the_aborted_run_marker() {
        let data_home = tempfile::tempdir().unwrap();
        let run_id = "classified-engine-abort";
        let writer = RunWriter::new(data_home.path(), run_id).unwrap();
        let _emitter = writer.stream_decisions().unwrap();
        let failure = Err(DailyRunFailure::Aborted(anyhow::anyhow!(
            "engine failed after blocking work began"
        )));

        assert!(resolve_locked_run(writer, failure).is_err());
        assert_eq!(aborted_runs(data_home.path()), vec![run_id.to_string()]);
    }
}
