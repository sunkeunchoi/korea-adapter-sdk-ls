//! The daily-resolution, multi-session-hold strategy (P7, U4) — the payload the
//! [`crate::runner::backtest_daily`] loop drives. It is the **sibling** of
//! [`crate::strategy::orb`], never a generalization of it: nothing here is reachable
//! from the ORB path, and nothing in `orb.rs` is edited, so `strategy_code_hash()`
//! stays byte-identical (R6). The items this module *reads* out of `orb.rs`
//! ([`UniverseCandidate`]) are read at their current signatures.
//!
//! # What it does
//!
//! Each session the runner hands it a batch containing exactly the symbols that are
//! either already held or newly taken. The strategy therefore does not re-derive the
//! take: the runner resolved it as "the top `target_m` of the ranked list from those
//! **not already held**" (R10, KTD16), using [`rank_by_placeholder_signal`] as the
//! ranking rule. `select_universe` is deliberately never called — its `gap_min_pct`
//! gate and `universe_top_n` cap are ORB's hypothesis, not this one (KTD15).
//!
//! For a taken symbol the strategy enters long, fixes a stop `stop_atr_mult ×
//! ATR(atr_window_sessions)` below the fill, records the entry-fixed risk capital,
//! and holds until either the stop is breached or `holding_period_sessions` distinct
//! **loop-supplied session ordinals** have elapsed (R23, KTD13). Long only, always.
//!
//! # The fill mechanic (U4 step 5) — fixed and stated
//!
//! `run_impl` routes each bar to the exchange *first* (which walks its O→H→L→C ticks
//! and leaves the L1 book at the close), then to the data engine, which is what fires
//! [`DataActor::on_bar`]. An order submitted inside that callback is drained and
//! settled at the **same** bar's `ts_init`, against a book that already sits at that
//! session's close. So:
//!
//! - **Entry: a market BUY submitted on the session the symbol is taken, filled at
//!   that session's daily close.** A market order is used rather than a marketable
//!   limit precisely because its fill price cannot be substituted: the matching
//!   engine's limit path rewrites a marketable fill to the *limit* price on the MAKER
//!   branch, whereas the market path returns the book level untouched. It also has no
//!   price to put off the instrument's `price_increment`, and an off-grid price is
//!   skipped with a WARN rather than an error.
//! - **Exit: a market SELL at the session close of the session the exit fires on**,
//!   via [`Strategy::close_position`], which threads `Some(position.id)` (mandatory —
//!   see KTD12 below). Both the stop exit and the hold-expiry exit use it.
//!
//! **The exit is deliberately NOT a resting stop order matched against the bar's OHLC
//! path.** The consequence is explicit and material: a session whose low breaches the
//! stop books its realized P&L at that session's *close*, not at the stop price. At
//! daily resolution those two differ by roughly a full ATR, and the difference lands
//! directly in the numerator of the frozen verdict statistic
//! (`Σ realized_pnl / Σ risk_capital`). The choice is that a daily-resolution
//! observer cannot fill intrabar at a level it never observed, so modelling the exit
//! at the observed close is the honest reading rather than the flattering one; it is
//! also unbiased in neither direction (a limit-down session exits *below* the stop, a
//! wick-and-recover session exits *above* it). Revisit this before the lineage's
//! first judged turn, not before.
//!
//! # The Hedging exit trap (KTD12)
//!
//! The daily venue is `OmsType::Hedging`. Under it, a fill whose client order id has
//! no cached position mints a **fresh** position and the netting fallback that would
//! otherwise match the open long is disabled — so an exit submitted *without* a
//! position id opens an opposite-side short instead of closing the long, and the
//! account type does not reject it. Every exit here therefore goes through
//! [`Strategy::close_position`], which submits with `Some(position.id)`. ORB's
//! `submit_order(order, None, …)` plus `reduce_only` exit is a **Netting-only**
//! pattern and is not copied.
//!
//! # The two fail-closed gates
//!
//! Both emit their decision record **on the refusal path**; the record is the only
//! evidence the gate ran, so its absence is itself a defect (AE3).
//!
//! - **The stop's ATR (KTD9, R11).** Refused when the prior ATR is unavailable *or*
//!   non-positive. A KRX limit-locked session prints `O=H=L=C`, so `ATR(1)` can be
//!   exactly zero — *available*, and it passes an `is_some` check. `joined_risk`
//!   returns `(None, None)` on a non-positive `risk_per_share`, which sets
//!   `all_have_risk = false` for the **whole run** and collapses `return_on_risk` to
//!   `None`: one bad entry in 837 sessions silently downgrades the run to a P&L
//!   number under a verdict that names a risk-normalized one.
//! - **The adjustment basis (R22).** Refused when a recorded adjustment-basis shift
//!   on the symbol falls inside the prospective hold window. The catalog is on the
//!   vendor's *adjusted* basis, so a corporate action inside a hold puts entry and
//!   exit on different bases and corrupts both the realized P&L and the entry-fixed
//!   risk capital — a 2:1 rewrite books a −50% "realized" loss on a flat position.
//!   [`crate::artifacts::data_quality::DataQualityReport::adjustment_basis_shift_symbols`]
//!   *reports* these symbols and never refuses; this is that report's fail-closed
//!   consumer, built on the same shape as the ATR refusal.
//!
//! # The ranking signal is a placeholder
//!
//! [`PLACEHOLDER_RANKING_SIGNAL`] names it in code and carries `placeholder: true`
//! into every decision record's context (`ranking_signal_placeholder = 1.0`). The
//! frozen artifact says the real signal is "frozen on the specification window" —
//! that is turn one's act, not this plan's. U6 makes the marker structural at the run
//! observation; U4 only has to carry it (R26, KTD6).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, NaiveDate};
use nautilus_common::actor::{DataActor, DataActorNative};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_model::data::Bar;
use nautilus_model::enums::{OrderSide, PositionSide, TimeInForce};
use nautilus_model::events::{PositionClosed, PositionOpened};
use nautilus_model::identifiers::{InstrumentId, PositionId, StrategyId};
use nautilus_model::orders::Order;
use nautilus_model::types::Quantity;
use nautilus_trading::nautilus_strategy;
use nautilus_trading::strategy::{Strategy, StrategyConfig, StrategyCore};

use crate::agent::context::AgentContext;
use crate::agent::envelope::{
    Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger, SignalKind,
};
use crate::agent::sink::DecisionSink;
use crate::artifacts::performance::{ClientOrderEntryRiskLedger, EntryRisk};
use crate::params_daily::DailyParams;
use crate::runner::backtest_daily::{
    DailyPathStrategy, DailySessionContext, DailySessionSignals, MountedSymbol, OpenPositionBook,
};
use crate::strategy::orb::UniverseCandidate;

/// The daily bar-type label the catalog records adjustment-basis shifts under.
pub const DAILY_BAR_TYPE_LABEL: &str = "1-DAY";

// ---------------------------------------------------------------------------
// The ranking signal (U4 step 8 — R26, KTD6)
// ---------------------------------------------------------------------------

/// A named ranking signal, carrying whether it is a placeholder.
///
/// A bare name would be a naming convention, which R26 explicitly rejects as too
/// weak. The `placeholder` flag is the value that travels: the strategy writes it into
/// every decision record's context, and U6 lifts it into the typed run observation
/// where it becomes a fail-closed edge against the judgment entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankingSignal {
    /// The signal's name in code.
    pub name: &'static str,
    /// Whether this signal is a placeholder and its runs are therefore not judgeable.
    pub placeholder: bool,
}

/// The placeholder ranking signal this unit ships: **prior-session turnover,
/// descending, symbol-ascending on ties**.
///
/// It is deliberately the plainest liquidity proxy that produces a total order over
/// the session's candidates, and it carries no hypothesis at all. The signal that
/// carries the lineage's hypothesis is frozen on the specification window in turn one
/// and is out of this plan's scope; shipping a *plausible-looking* placeholder without
/// this marker is exactly how a placeholder run gets judged as a real one.
pub const PLACEHOLDER_RANKING_SIGNAL: RankingSignal =
    RankingSignal { name: "prior_turnover_desc", placeholder: true };

/// Rank a session's candidates by [`PLACEHOLDER_RANKING_SIGNAL`], best first.
///
/// This is the *whole* ranked list, never a take: the take is
/// `target_m`-minus-already-held and is resolved per session in the runner, because
/// the held set is engine state (KTD16). Truncating here would block re-entry into a
/// slot freed by an early stop-out and so violate R10.
///
/// `select_universe` (`orb.rs`) is not called and must not be: its `gap_min_pct` gate
/// and `universe_top_n` cap (default 20, against a frozen `target_m` of 8) are ORB's
/// hypothesis (KTD15). Candidate *assembly* is shared; the selection *rule* is this.
#[must_use]
pub fn rank_by_placeholder_signal(candidates: &[UniverseCandidate]) -> Vec<String> {
    let mut ranked: Vec<&UniverseCandidate> = candidates.iter().collect();
    ranked.sort_by(|a, b| {
        b.prior_turnover
            .partial_cmp(&a.prior_turnover)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    ranked.into_iter().map(|c| c.symbol.clone()).collect()
}

// ---------------------------------------------------------------------------
// Refusal reasons (U4 steps 3 and 7)
// ---------------------------------------------------------------------------

/// Why an entry was refused, or a ranked candidate never taken. Typed rather than
/// free text so a refusal can be counted and asserted, and so the two fail-closed
/// gates cannot be told apart from an ordinary "not selected today".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryRefusal {
    /// No prior ATR could be derived for the symbol on this session (KTD9).
    AtrUnavailable,
    /// A prior ATR exists but is zero, negative, or non-finite — the limit-locked
    /// `O=H=L=C` session that passes an `is_some` check (KTD9).
    AtrNonPositive,
    /// A recorded adjustment-basis shift falls inside the prospective hold window
    /// (R22).
    AdjustmentBasisShift,
    /// The stop would sit at or below zero — an unreachable stop, so the position
    /// would carry a nominal risk capital it can never realize.
    NonPositiveStop,
    /// The sizing term buys nothing at this price (`floor(notional / price) == 0`).
    ZeroQuantity,
    /// The concurrency cap is already met. On this path the cap is an assertion, not
    /// a second selection rule, so a refusal here means the take over-issued.
    ConcurrencyCap,
    /// The symbol already holds an open position, so it is not takeable this session
    /// however it ranks (R10).
    AlreadyHeld,
    /// Ranked, not held, but outside this session's entry budget of `target_m`.
    RankBeyondEntryBudget,
}

impl EntryRefusal {
    /// The reason's wire name, recorded as the decision record's `filter`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            EntryRefusal::AtrUnavailable => "atr_unavailable",
            EntryRefusal::AtrNonPositive => "atr_non_positive",
            EntryRefusal::AdjustmentBasisShift => "adjustment_basis_shift",
            EntryRefusal::NonPositiveStop => "non_positive_stop",
            EntryRefusal::ZeroQuantity => "zero_quantity",
            EntryRefusal::ConcurrencyCap => "concurrency_cap",
            EntryRefusal::AlreadyHeld => "already_held",
            EntryRefusal::RankBeyondEntryBudget => "rank_beyond_entry_budget",
        }
    }
}

// ---------------------------------------------------------------------------
// The adjustment-basis shift ledger (R22)
// ---------------------------------------------------------------------------

/// The recorded per-symbol adjustment-basis shift dates a hold must not straddle
/// (R22).
///
/// **Where the dates come from.** The ingest checkpoint is the recording authority:
/// [`Checkpoint::shifted_instruments`] plus [`Checkpoint::shifted_detected`] give the
/// *unhealed* marks (exactly the set
/// `DataQualityReport::adjustment_basis_shift_symbols` reports), and
/// [`Checkpoint::rebase_events`] gives the *healed* ones, whose `detected` date is
/// still the session the basis changed on. Both are folded in by
/// [`AdjustmentBasisShifts::from_checkpoint`], because a healed re-base moved the
/// catalog's basis just as surely as an unhealed one — healing rewrites history on
/// the new basis, it does not make the old and new bases comparable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdjustmentBasisShifts {
    by_symbol: BTreeMap<String, BTreeSet<NaiveDate>>,
}

impl AdjustmentBasisShifts {
    /// No recorded shifts — a clean catalog.
    #[must_use]
    pub fn none() -> Self {
        AdjustmentBasisShifts::default()
    }

    /// Build from `(instrument id, shift date)` pairs.
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, NaiveDate)>) -> Self {
        let mut by_symbol: BTreeMap<String, BTreeSet<NaiveDate>> = BTreeMap::new();
        for (symbol, date) in pairs {
            by_symbol.entry(symbol).or_default().insert(date);
        }
        AdjustmentBasisShifts { by_symbol }
    }

    /// Build from an ingest checkpoint: unhealed daily shift marks **and** completed
    /// daily re-base events, both keyed on their detection session.
    #[must_use]
    pub fn from_checkpoint(checkpoint: &Checkpoint) -> Self {
        let mut pairs: Vec<(String, NaiveDate)> = Vec::new();
        for instrument in checkpoint.shifted_instruments(DAILY_BAR_TYPE_LABEL) {
            if let Some(date) = checkpoint
                .shifted_detected(&instrument, DAILY_BAR_TYPE_LABEL)
                .and_then(parse_yyyymmdd)
            {
                pairs.push((instrument, date));
            }
        }
        for event in checkpoint.rebase_events() {
            if event.bar_type == DAILY_BAR_TYPE_LABEL {
                if let Some(date) = parse_yyyymmdd(&event.detected) {
                    pairs.push((event.instrument.clone(), date));
                }
            }
        }
        AdjustmentBasisShifts::from_pairs(pairs)
    }

    /// The first recorded shift on `symbol` inside the inclusive window
    /// `[start, end]`, if any.
    #[must_use]
    pub fn straddling(&self, symbol: &str, start: NaiveDate, end: NaiveDate) -> Option<NaiveDate> {
        self.by_symbol
            .get(symbol)?
            .range(start..=end)
            .next()
            .copied()
    }

    /// Whether nothing is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_symbol.is_empty()
    }
}

fn parse_yyyymmdd(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y%m%d").ok()
}

// ---------------------------------------------------------------------------
// The strategy
// ---------------------------------------------------------------------------

/// One open leg's entry-fixed state. Everything here is set once, at the open, and
/// never re-derived from a later bar (R12).
#[derive(Debug, Clone, Copy, PartialEq)]
struct OpenLeg {
    position_id: PositionId,
    /// The realized fill price (`PositionOpened.avg_px_open`), not the assumed one.
    entry_price: f64,
    /// `entry_price − stop_atr_mult × ATR`, fixed here for the whole hold.
    stop: f64,
    /// `stop_atr_mult × ATR` — the per-share risk recorded in the ledger at submit.
    risk_per_share: f64,
    qty: f64,
    /// The loop-supplied session ordinal the leg opened on. Hold elapsed is
    /// `current_index − entry_index`, never a bar-callback count (R23).
    entry_index: usize,
}

/// What the strategy recorded at submit time, pending the open.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingLeg {
    risk_per_share: f64,
    qty: f64,
    entry_index: usize,
}

/// The daily multi-session-hold strategy.
///
/// See the module documentation for the fill mechanic, the Hedging exit contract, and
/// the two fail-closed gates.
pub struct DailyStrategy {
    core: StrategyCore,
    params: DailyParams,
    decisions: DecisionSink,
    shifts: AdjustmentBasisShifts,
    mounted: Vec<MountedSymbol>,
    /// KTD16: the runner reads the held set off this between batches.
    book: OpenPositionBook,
    /// The runner publishes each session's context into this before its batch runs.
    signals: DailySessionSignals,
    /// KTD3/R12: keyed by `ClientOrderId`, the only identity available at submit.
    entry_risk: ClientOrderEntryRiskLedger,
    /// Entry orders submitted but not yet opened — the single authority for what is in
    /// flight. A parallel `pending: BTreeSet<InstrumentId>` was removed: two
    /// collections cleared on different paths is exactly how a denied order kept a
    /// concurrency slot for the rest of the run.
    pending_leg: HashMap<InstrumentId, PendingLeg>,
    open: HashMap<InstrumentId, OpenLeg>,
    /// The last session ordinal whose take refusals were recorded — the per-session
    /// record is emitted exactly once, on the session's first bar callback.
    last_recorded_session: Option<usize>,
}

impl DailyStrategy {
    /// Build the strategy over the runner's mounted universe.
    ///
    /// `shifts` is the recorded adjustment-basis ledger the R22 gate reads; pass
    /// [`AdjustmentBasisShifts::from_checkpoint`] on a real run and
    /// [`AdjustmentBasisShifts::none`] only when the catalog genuinely has no
    /// checkpoint.
    #[must_use]
    pub fn new(
        mounted: Vec<MountedSymbol>,
        params: DailyParams,
        decisions: DecisionSink,
        shifts: AdjustmentBasisShifts,
    ) -> Self {
        let strategy_id = StrategyId::from(params.strategy_id.as_str());
        DailyStrategy {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(strategy_id),
                ..Default::default()
            }),
            params,
            decisions,
            shifts,
            mounted,
            book: OpenPositionBook::new(),
            signals: DailySessionSignals::new(),
            entry_risk: ClientOrderEntryRiskLedger::new(),
            pending_leg: HashMap::new(),
            open: HashMap::new(),
            last_recorded_session: None,
        }
    }

    /// A factory for [`crate::runner::backtest_daily::run_daily`]'s `make_strategy`
    /// argument.
    pub fn factory(
        params: DailyParams,
        decisions: DecisionSink,
        shifts: AdjustmentBasisShifts,
    ) -> impl Fn(&[MountedSymbol]) -> DailyStrategy + Send + 'static {
        move |mounted: &[MountedSymbol]| {
            DailyStrategy::new(
                mounted.to_vec(),
                params.clone(),
                decisions.clone(),
                shifts.clone(),
            )
        }
    }

    /// The parameter set this strategy runs under.
    #[must_use]
    pub fn params(&self) -> &DailyParams {
        &self.params
    }

    // -- telemetry ----------------------------------------------------------

    /// The decision context every record rides: the daily parameter set as numbers,
    /// plus the placeholder marker (R26 — U6 makes it structural, this carries it).
    fn context(&self) -> AgentContext {
        let summary = BTreeMap::from([
            ("holding_period_sessions".to_string(), self.params.holding_period_sessions as f64),
            ("target_m".to_string(), self.params.target_m as f64),
            ("max_concurrent".to_string(), self.params.max_concurrent as f64),
            ("stop_atr_mult".to_string(), self.params.stop_atr_mult),
            ("atr_window_sessions".to_string(), self.params.atr_window_sessions),
            ("notional_per_position".to_string(), self.params.notional_per_position),
            (
                "ranking_signal_placeholder".to_string(),
                f64::from(u8::from(PLACEHOLDER_RANKING_SIGNAL.placeholder)),
            ),
        ]);
        let counts =
            BTreeMap::from([("decisions".to_string(), self.decisions.len() as u64)]);
        AgentContext::telemetry(
            self.params.strategy_id.clone(),
            self.params.strategy_version,
            summary,
            counts,
        )
    }

    /// Emit a refusal record. **This is the only evidence the gate ran** (AE3), so it
    /// is emitted on the refusal path itself and never inferred from an absent trade.
    fn record_refusal(
        &self,
        id: InstrumentId,
        ts: u64,
        reason: EntryRefusal,
        values: BTreeMap<String, f64>,
    ) {
        let detail = DecisionDetail {
            kind: SignalKind::OrderRejectedSizing,
            symbol: id.to_string(),
            decision: Some(Decision::Reject),
            filter: Some(reason.as_str().to_string()),
            values,
            tags: None,
        };
        self.decisions.emit(DecisionEnvelope::telemetry(
            ts,
            DecisionTrigger::MarketData { instrument_id: id },
            detail,
            self.context(),
        ));
    }

    /// Emit an accept / transition record (entry placed, stop hit, hold expiry).
    fn record_transition(
        &self,
        id: InstrumentId,
        ts: u64,
        kind: SignalKind,
        values: BTreeMap<String, f64>,
    ) {
        let detail = DecisionDetail {
            kind,
            symbol: id.to_string(),
            decision: Some(Decision::Accept),
            filter: None,
            values,
            tags: None,
        };
        self.decisions.emit(DecisionEnvelope::telemetry(
            ts,
            DecisionTrigger::MarketData { instrument_id: id },
            detail,
            self.context(),
        ));
    }

    /// Record, once per session, why each ranked candidate was **not** taken.
    ///
    /// The runner resolved the take, so this is the only place the two non-take
    /// reasons are distinguishable: a symbol excluded because it is already held
    /// (R10 — it may rank first and still not be takeable) versus one that simply
    /// fell outside the session's entry budget of `target_m`.
    fn record_take_refusals(&self, ctx: &DailySessionContext, ts: u64) {
        let taken: BTreeSet<InstrumentId> = ctx.taken.iter().copied().collect();
        let held: BTreeSet<InstrumentId> = ctx.held.iter().copied().collect();
        for (rank, id) in ctx.ranked.iter().enumerate() {
            if taken.contains(id) {
                continue;
            }
            let reason = if held.contains(id) {
                EntryRefusal::AlreadyHeld
            } else {
                EntryRefusal::RankBeyondEntryBudget
            };
            self.record_refusal(
                *id,
                ts,
                reason,
                BTreeMap::from([
                    ("rank".to_string(), rank as f64),
                    ("target_m".to_string(), self.params.target_m as f64),
                    ("held".to_string(), held.len() as f64),
                ]),
            );
        }
    }

    /// Drop entry orders that were submitted but never opened a position.
    ///
    /// An entry submitted inside `on_bar` is drained and settled at the **same** bar's
    /// `ts_init` (see the module doc's fill mechanic), so anything still in flight when
    /// a new session ordinal arrives never opened and never will: the risk engine
    /// denied it, the venue rejected it, or it did not fill. Nothing else ever removes
    /// it — `pending_leg` is otherwise cleared only by the position callbacks — so it
    /// would hold one of `max_concurrent` slots for the rest of the run *and* make the
    /// symbol permanently un-re-enterable, because `on_bar` returns early on an
    /// in-flight id.
    ///
    /// No decision record is emitted here. The run-level diagnostic for exactly this
    /// population already exists as
    /// [`ClientOrderEntryRiskLedger::unopened_entries`], surfaced as
    /// `DailyRunOutcome::unopened_entry_orders`, and it is keyed by the client order id
    /// this side does not carry.
    fn discard_stale_pendings(&mut self) {
        self.pending_leg.clear();
    }

    // -- the entry path -----------------------------------------------------

    /// The last in-range session a hold opened on `ctx.index` could still be open on
    /// — `entry + holding_period_sessions`, clamped to the run's final session.
    ///
    /// The window is measured on the loop's own session calendar, never on calendar
    /// days: a hold of 16 sessions spans more than 16 days and the gap is not
    /// constant.
    fn hold_window_end(&self, ctx: &DailySessionContext) -> NaiveDate {
        let last = self.signals.session_count().saturating_sub(1);
        let end = ctx.index.saturating_add(self.params.holding_period_sessions).min(last);
        self.signals.session_at(end).unwrap_or(ctx.date)
    }

    /// Evaluate a taken symbol for entry, refusing with a recorded reason at every
    /// gate. Returns the refusal, or `None` when the entry was submitted.
    fn evaluate_entry(
        &mut self,
        bar: &Bar,
        ctx: &DailySessionContext,
    ) -> anyhow::Result<Option<EntryRefusal>> {
        let id = bar.bar_type.instrument_id();
        let ts = bar.ts_event.as_u64();
        let symbol = id.to_string();
        let entry_price = bar.close.as_f64();

        // The concurrency cap is an assertion on this path, not a second selection
        // rule: `target_m × hold` is the throttle, so reaching the cap means the take
        // over-issued.
        let committed = self.open.len() + self.pending_leg.len();
        if committed >= self.params.max_concurrent {
            let reason = EntryRefusal::ConcurrencyCap;
            self.record_refusal(
                id,
                ts,
                reason,
                BTreeMap::from([
                    ("open".to_string(), self.open.len() as f64),
                    ("pending".to_string(), self.pending_leg.len() as f64),
                    ("max_concurrent".to_string(), self.params.max_concurrent as f64),
                ]),
            );
            return Ok(Some(reason));
        }

        // R22, fail closed: a corporate action inside the hold puts entry and exit on
        // different bases and corrupts BOTH the realized P&L and the entry-fixed risk
        // capital.
        let window_end = self.hold_window_end(ctx);
        if let Some(shift) = self.shifts.straddling(&symbol, ctx.date, window_end) {
            let reason = EntryRefusal::AdjustmentBasisShift;
            self.record_refusal(
                id,
                ts,
                reason,
                BTreeMap::from([
                    ("hold_sessions".to_string(), self.params.holding_period_sessions as f64),
                    ("shift_ordinal".to_string(), shift.num_days_from_ce() as f64),
                    ("window_start_ordinal".to_string(), ctx.date.num_days_from_ce() as f64),
                    ("window_end_ordinal".to_string(), window_end.num_days_from_ce() as f64),
                ]),
            );
            return Ok(Some(reason));
        }

        // KTD9, fail closed on BOTH arms. `flatten()` collapses "not a candidate" and
        // "candidate with no derivable ATR" onto the same unavailable arm.
        let Some(atr) = ctx.prior_atr.get(&id).copied().flatten() else {
            let reason = EntryRefusal::AtrUnavailable;
            self.record_refusal(
                id,
                ts,
                reason,
                BTreeMap::from([
                    ("atr_window_sessions".to_string(), self.params.atr_window_sessions),
                    ("entry_price".to_string(), entry_price),
                ]),
            );
            return Ok(Some(reason));
        };
        if !atr.is_finite() || atr <= 0.0 {
            let reason = EntryRefusal::AtrNonPositive;
            self.record_refusal(
                id,
                ts,
                reason,
                BTreeMap::from([
                    ("prior_atr".to_string(), atr),
                    ("atr_window_sessions".to_string(), self.params.atr_window_sessions),
                    ("entry_price".to_string(), entry_price),
                ]),
            );
            return Ok(Some(reason));
        }

        let risk_per_share = self.params.stop_atr_mult * atr;
        let stop = entry_price - risk_per_share;
        if !stop.is_finite() || stop <= 0.0 {
            let reason = EntryRefusal::NonPositiveStop;
            self.record_refusal(
                id,
                ts,
                reason,
                BTreeMap::from([
                    ("prior_atr".to_string(), atr),
                    ("stop".to_string(), stop),
                    ("entry_price".to_string(), entry_price),
                ]),
            );
            return Ok(Some(reason));
        }

        // R27: sized from the DAILY term. ORB's notional sizes 5 concurrent
        // positions; this path holds `target_m × hold`.
        let qty = self.params.position_qty(entry_price);
        if qty <= 0 {
            let reason = EntryRefusal::ZeroQuantity;
            self.record_refusal(
                id,
                ts,
                reason,
                BTreeMap::from([
                    ("entry_price".to_string(), entry_price),
                    ("notional_per_position".to_string(), self.params.notional_per_position),
                ]),
            );
            return Ok(Some(reason));
        }

        // Long only (frozen `directionality`), market BUY — see the module doc for
        // why a market order rather than a marketable limit.
        let order = self.order().market(
            id,
            OrderSide::Buy,
            Quantity::from(qty),
            Some(TimeInForce::Gtc),
            Some(false), // reduce_only — an ENTRY must never be reduce-only
            None,        // quote_quantity
            None,        // exec_algorithm_id
            None,        // exec_algorithm_params
            None,        // tags
            None,        // client_order_id
        );
        // KTD3: capture the entry-fixed risk keyed by CLIENT ORDER ID, the only
        // identity available here and exactly the key the read side carries as
        // `Position.opening_order_id`. `risk_per_share` is `stop_atr_mult × ATR` and
        // is therefore independent of the realized fill price, so it is exact even
        // though the fill has not happened yet (R12).
        self.entry_risk.record(
            order.client_order_id(),
            EntryRisk { risk_per_share, qty: qty as f64 },
        );
        self.submit_order(order, None, None, None)?;
        self.pending_leg
            .insert(id, PendingLeg { risk_per_share, qty: qty as f64, entry_index: ctx.index });

        self.record_transition(
            id,
            ts,
            SignalKind::OrderPlaced,
            BTreeMap::from([
                ("entry_price".to_string(), entry_price),
                ("prior_atr".to_string(), atr),
                ("stop".to_string(), stop),
                ("risk_per_share".to_string(), risk_per_share),
                ("qty".to_string(), qty as f64),
                ("risk_capital".to_string(), risk_per_share * qty as f64),
                ("session_index".to_string(), ctx.index as f64),
            ]),
        );
        Ok(None)
    }

    // -- the exit path ------------------------------------------------------

    /// Close `id`'s open leg **with its position id** (KTD12).
    ///
    /// [`Strategy::close_position`] threads `Some(position.id)` through submission.
    /// Under the Hedging venue an exit without one mints a fresh opposite-side
    /// position instead of closing the long, and nothing rejects it.
    fn exit(&mut self, id: InstrumentId, ts: u64, kind: SignalKind, values: BTreeMap<String, f64>) {
        let Some(leg) = self.open.get(&id).copied() else {
            return;
        };
        let position = {
            let cache = self.core.cache_rc();
            let cache = cache.borrow();
            cache.position(&leg.position_id).map(|p| p.cloned())
        };
        let Some(position) = position else {
            return;
        };
        if position.is_closed() {
            return;
        }
        self.close_position(&position, None, None, Some(TimeInForce::Gtc), None, None)
            .expect("close_position must submit with the position id (KTD12)");
        self.record_transition(id, ts, kind, values);
    }
}

impl DailyPathStrategy for DailyStrategy {
    fn open_position_book(&self) -> OpenPositionBook {
        self.book.clone()
    }

    fn entry_risk_ledger(&self) -> ClientOrderEntryRiskLedger {
        self.entry_risk.clone()
    }

    fn session_signals(&self) -> DailySessionSignals {
        self.signals.clone()
    }
}

impl std::fmt::Debug for DailyStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DailyStrategy")
            .field("strategy_id", &self.params.strategy_id)
            .field("mounted", &self.mounted.len())
            .field("open", &self.open.len())
            .finish()
    }
}

nautilus_strategy!(DailyStrategy, core, {
    fn on_position_opened(&mut self, event: PositionOpened) {
        // Long only, and asserted rather than assumed: under Hedging an exit
        // submitted without a position id mints a fresh SHORT and the account type
        // does not reject it, so a short here is the signature of the KTD12 trap
        // rather than a strategy bug — fail loudly instead of booking it.
        assert_eq!(
            event.side,
            PositionSide::Long,
            "the daily path is long only (frozen directionality): a {:?} position on {} means an \
             exit was submitted without its position id under the Hedging venue (KTD12)",
            event.side,
            event.instrument_id
        );
        // `pending_leg`'s removal below is what clears the in-flight record.
        // U2/KTD3 assertion 2's stream-side witness: which recorded entries actually
        // opened a position, known independently of the cache read. Recording only at
        // submit would make the runner's reconciliation tautological.
        self.entry_risk.record_opened(event.opening_order_id);
        self.book.record_opened(event.instrument_id, event.position_id);

        // Hard failure, not a fallback. `risk_per_share` is the stop distance: absent
        // it, `unwrap_or(0.0)` placed the stop AT the fill price, which both flattens
        // the position on the next session that trades at or below its entry — killing
        // the frozen hold — and books zero risk capital, which makes `joined_risk`
        // return `(None, None)` and collapses `return_on_risk` to `None` for the WHOLE
        // run. Same reasoning as the long-only guard above: a missing leg means this
        // callback fired for an order this strategy did not submit, which is a defect
        // to surface rather than a number to invent.
        let leg = self.pending_leg.remove(&event.instrument_id).unwrap_or_else(|| {
            panic!(
                "position opened on {} with no pending entry leg recorded at submit: the \
                 entry-fixed stop distance and the entry session ordinal are both \
                 unrecoverable here, and substituting zero would place the stop at the fill \
                 price and collapse return_on_risk for the whole run",
                event.instrument_id
            )
        });
        let entry_index = leg.entry_index;
        let risk_per_share = leg.risk_per_share;
        // The stop is fixed off the REALIZED fill, not the assumed one, and never
        // moves again (R12). `risk_per_share` was recorded at submit and is exact
        // regardless of the fill, because it is `stop_atr_mult × ATR`.
        let entry_price = event.avg_px_open;
        self.open.insert(
            event.instrument_id,
            OpenLeg {
                position_id: event.position_id,
                entry_price,
                stop: entry_price - risk_per_share,
                risk_per_share,
                qty: event.quantity.as_f64(),
                entry_index,
            },
        );
    }

    fn on_position_closed(&mut self, event: PositionClosed) {
        self.book.record_closed(&event.instrument_id);
        self.open.remove(&event.instrument_id);
        self.pending_leg.remove(&event.instrument_id);
    }
});

impl DataActor for DailyStrategy {
    fn on_start(&mut self) -> anyhow::Result<()> {
        for m in self.mounted.clone() {
            self.subscribe_bars(m.bar_type, None, None);
        }
        Ok(())
    }

    fn on_bar(&mut self, bar: &Bar) -> anyhow::Result<()> {
        let id = bar.bar_type.instrument_id();
        let ts = bar.ts_event.as_u64();
        // Every clock in here is the loop's, not the stream's (R23).
        let Some(ctx) = self.signals.current() else {
            return Ok(());
        };

        // Session rollover: sweep the previous session's stale pendings, then record the
        // take refusals — both exactly once per session, on its first bar callback. A
        // duplicate bar for the same session re-enters here with the same ordinal and
        // does neither again.
        if self.last_recorded_session != Some(ctx.index) {
            self.last_recorded_session = Some(ctx.index);
            self.discard_stale_pendings();
            self.record_take_refusals(&ctx, ts);
        }

        if let Some(leg) = self.open.get(&id).copied() {
            // Hold elapsed in DISTINCT LOOP-SUPPLIED SESSION ORDINALS (R23). A
            // duplicate bar delivered for the same session date carries the same
            // ordinal, so it cannot shorten a frozen hold; a session on which this
            // symbol has no bar still advances it, so the hold is a calendar of
            // sessions rather than a count of callbacks.
            let elapsed = ctx.index.saturating_sub(leg.entry_index);
            let low = bar.low.as_f64();
            let base = BTreeMap::from([
                ("entry_price".to_string(), leg.entry_price),
                ("stop".to_string(), leg.stop),
                ("risk_per_share".to_string(), leg.risk_per_share),
                ("qty".to_string(), leg.qty),
                ("elapsed_sessions".to_string(), elapsed as f64),
                ("session_index".to_string(), ctx.index as f64),
            ]);
            if low <= leg.stop {
                let mut values = base;
                values.insert("bar_low".to_string(), low);
                values.insert("exit_price".to_string(), bar.close.as_f64());
                self.exit(id, ts, SignalKind::StopHit, values);
            } else if elapsed >= self.params.holding_period_sessions {
                let mut values = base;
                values.insert("exit_price".to_string(), bar.close.as_f64());
                self.exit(id, ts, SignalKind::TimeExit, values);
            }
            return Ok(());
        }

        if self.pending_leg.contains_key(&id) {
            return Ok(());
        }
        // Defensive: the runner's take already excluded held symbols (R10), so a bar
        // for an un-open, un-pending symbol is a take. Refusals are recorded inside.
        self.evaluate_entry(bar, &ctx)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::orb::{CandidateMeta, SessionGapPrices};

    fn candidate(symbol: &str, prior_turnover: f64) -> UniverseCandidate {
        UniverseCandidate {
            symbol: symbol.to_string(),
            gap_prices: SessionGapPrices::new(1_000, 1_000),
            prior_turnover,
            // `Untagged` — the legacy, metadata-less join state — spelled out rather than
            // defaulted. `CandidateMeta` has no `Default` impl and cannot grow one: it
            // lives in `orb.rs`, whose bytes are the head digest (KTD5).
            meta: CandidateMeta::Untagged,
            prior_atr: Some(10.0),
            prior_open_vol_mean: None,
            prior_illiq: None,
        }
    }

    #[test]
    fn the_placeholder_ranking_signal_is_marked_and_total() {
        assert!(
            PLACEHOLDER_RANKING_SIGNAL.placeholder,
            "the shipped signal is a placeholder; R26 forbids a run made with it being judged"
        );
        assert_eq!(PLACEHOLDER_RANKING_SIGNAL.name, "prior_turnover_desc");

        // Ranks EVERY candidate — never a take. Truncating to target_m here would
        // block re-entry into a slot freed by an early stop-out (R10, KTD16).
        let ranked = rank_by_placeholder_signal(&[
            candidate("000660.XKRX", 10.0),
            candidate("005930.XKRX", 30.0),
            candidate("035720.XKRX", 20.0),
        ]);
        assert_eq!(ranked, vec!["005930.XKRX", "035720.XKRX", "000660.XKRX"]);
    }

    #[test]
    fn ties_break_on_symbol_so_the_rank_is_deterministic() {
        let ranked = rank_by_placeholder_signal(&[
            candidate("035720.XKRX", 10.0),
            candidate("000660.XKRX", 10.0),
        ]);
        assert_eq!(ranked, vec!["000660.XKRX", "035720.XKRX"]);
    }

    #[test]
    fn every_refusal_reason_has_a_distinct_wire_name() {
        let all = [
            EntryRefusal::AtrUnavailable,
            EntryRefusal::AtrNonPositive,
            EntryRefusal::AdjustmentBasisShift,
            EntryRefusal::NonPositiveStop,
            EntryRefusal::ZeroQuantity,
            EntryRefusal::ConcurrencyCap,
            EntryRefusal::AlreadyHeld,
            EntryRefusal::RankBeyondEntryBudget,
        ];
        let names: BTreeSet<&str> = all.iter().map(|r| r.as_str()).collect();
        assert_eq!(names.len(), all.len(), "a reason that shares a name cannot be counted");
    }

    #[test]
    fn a_shift_inside_the_window_is_found_and_one_outside_is_not() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y%m%d").unwrap();
        let shifts = AdjustmentBasisShifts::from_pairs([
            ("005930.XKRX".to_string(), d("20240110")),
            ("000660.XKRX".to_string(), d("20240401")),
        ]);
        assert_eq!(
            shifts.straddling("005930.XKRX", d("20240103"), d("20240131")),
            Some(d("20240110"))
        );
        // Boundaries are inclusive on BOTH ends: a shift on the exit session still
        // splits the basis across the hold.
        assert_eq!(
            shifts.straddling("005930.XKRX", d("20240110"), d("20240110")),
            Some(d("20240110"))
        );
        // Outside the window, and an unrecorded symbol.
        assert_eq!(shifts.straddling("005930.XKRX", d("20240111"), d("20240131")), None);
        assert_eq!(shifts.straddling("035720.XKRX", d("20240103"), d("20240131")), None);
        assert!(AdjustmentBasisShifts::none().is_empty());
    }
}
