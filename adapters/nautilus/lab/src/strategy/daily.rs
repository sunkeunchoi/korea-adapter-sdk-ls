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
//! # Live Netting and backtest Hedging (KTD12)
//!
//! The attended live strategy explicitly registers `OmsType::Netting`, matching the
//! LS account: an exit reduces the existing quantity to zero, and a later entry
//! increases/reopens that instrument rather than minting a new hedged position. The
//! historical backtest venue deliberately remains Hedging so repeated round trips
//! retain distinct position records. Both paths exit through
//! [`Strategy::close_position`] with `reduce_only = true`; therefore the restored
//! live leg cannot cross zero into a short, while the unchanged backtest still closes
//! the exact `PositionId` it opened.
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
use nautilus_model::enums::{OmsType, OrderSide, PositionSide, TimeInForce};
use nautilus_model::events::{PositionClosed, PositionOpened};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, PositionId, StrategyId};
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
use crate::params_daily::{DailyParams, RankingSignalKind};
use crate::runner::backtest_daily::{
    DailyPathStrategy, DailySessionContext, DailySessionSignals, MountedSymbol, OpenPositionBook,
};
use crate::strategy::orb::UniverseCandidate;
use crate::strategy::hooks::{EmissionGate, Heartbeats, MarkFeed};
use crate::strategy::orb::SymbolMark;

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

impl RankingSignal {
    /// Derive the recorded signal identity from the governed parameter variant.
    #[must_use]
    pub const fn from_kind(kind: RankingSignalKind) -> Self {
        RankingSignal { name: kind.name(), placeholder: kind.is_placeholder() }
    }
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
    RankingSignal::from_kind(RankingSignalKind::Placeholder);

/// One signal dispatch's complete result.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalRanking {
    /// Scored symbols, best first.
    pub ranked: Vec<String>,
    /// Candidates the selected signal could not score, in symbol order.
    pub unavailable: Vec<String>,
}

/// Dispatch the governed ranking signal over one session's candidates.
///
/// `prior_closes` contains each symbol's closes strictly before the decision
/// session, oldest-to-newest. Keeping that history out of `UniverseCandidate`
/// avoids moving ORB's identity-bearing source merely to serve this sibling.
#[must_use]
pub fn rank_by_signal(
    kind: RankingSignalKind,
    candidates: &[UniverseCandidate],
    prior_closes: &BTreeMap<String, Vec<f64>>,
) -> SignalRanking {
    let mut scored: Vec<(&UniverseCandidate, f64)> = Vec::new();
    let mut unavailable = Vec::new();
    for candidate in candidates {
        let score = match kind {
            RankingSignalKind::Placeholder | RankingSignalKind::PriorTurnoverDesc => {
                crate::strategy::daily_signal::prior_turnover_desc(candidate)
            }
            RankingSignalKind::Momentum12x1 => prior_closes
                .get(&candidate.symbol)
                .and_then(|closes| crate::strategy::daily_signal::momentum_12x1(closes)),
        };
        match score {
            Some(score) => scored.push((candidate, score)),
            None => unavailable.push(candidate.symbol.clone()),
        }
    }
    scored.sort_by(|(a, a_score), (b, b_score)| {
        b_score
            .partial_cmp(a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    unavailable.sort();
    SignalRanking {
        ranked: scored.into_iter().map(|(candidate, _)| candidate.symbol.clone()).collect(),
        unavailable,
    }
}

/// Emit one selection record per candidate under the selected signal, including
/// the fail-closed `signal_unavailable` rejection.
pub fn record_signal_decisions(
    sink: &DecisionSink,
    params: &DailyParams,
    ts: u64,
    candidates: &[UniverseCandidate],
    prior_closes: &BTreeMap<String, Vec<f64>>,
    ranking: &SignalRanking,
) {
    let signal = RankingSignal::from_kind(params.ranking_signal);
    let ranks: BTreeMap<&str, usize> = ranking
        .ranked
        .iter()
        .enumerate()
        .map(|(rank, symbol)| (symbol.as_str(), rank))
        .collect();
    let unavailable: BTreeSet<&str> = ranking.unavailable.iter().map(String::as_str).collect();
    let mut ordered: Vec<&UniverseCandidate> = candidates.iter().collect();
    ordered.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    for candidate in ordered {
        let instrument_id = InstrumentId::from(candidate.symbol.as_str());
        let (decision, filter, values) = if unavailable.contains(candidate.symbol.as_str()) {
            (
                Decision::Reject,
                Some("signal_unavailable".to_string()),
                BTreeMap::from([
                    (
                        "available_prior_bars".to_string(),
                        prior_closes.get(&candidate.symbol).map_or(0, Vec::len) as f64,
                    ),
                    (
                        "required_prior_bars".to_string(),
                        params.ranking_signal.warmup_bars() as f64,
                    ),
                ]),
            )
        } else {
            (
                Decision::Accept,
                None,
                BTreeMap::from([
                    ("prior_turnover".to_string(), candidate.prior_turnover),
                    ("rank".to_string(), ranks[candidate.symbol.as_str()] as f64),
                ]),
            )
        };
        let detail = DecisionDetail {
            kind: SignalKind::Universe,
            symbol: candidate.symbol.clone(),
            decision: Some(decision),
            filter,
            values,
            tags: None,
        };
        sink.emit(DecisionEnvelope::telemetry(
            ts,
            DecisionTrigger::MarketData { instrument_id },
            detail,
            AgentContext::telemetry(
                params.strategy_id.clone(),
                params.strategy_version,
                BTreeMap::from([
                    (format!("ranking_signal_{}", signal.name), 1.0),
                    (
                        "ranking_signal_placeholder".to_string(),
                        f64::from(u8::from(signal.placeholder)),
                    ),
                ]),
                BTreeMap::from([("decisions".to_string(), sink.len() as u64)]),
            ),
        ));
    }
}

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
    /// Restored holdings remain joinable until reconciliation emits their open event;
    /// ordinary same-session entry pendings are swept at the next session.
    seeded: bool,
}

/// One broker-confirmed overnight holding restored from the rehearsal book.
#[derive(Debug, Clone, PartialEq)]
pub struct BookLeg {
    /// Mounted instrument held long.
    pub instrument_id: InstrumentId,
    /// Entry fill's client order id, used to seed the risk ledger join.
    pub opening_order_id: ClientOrderId,
    /// Realized entry price.
    pub entry_price: f64,
    /// Fixed stop price carried across sessions.
    pub stop_price: f64,
    /// Positive share quantity.
    pub quantity: f64,
    /// Calendar/session ordinal on which the leg opened.
    pub entry_session_ordinal: usize,
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
    /// Live-only order-emission interlock. Open by default so an unset hook leaves
    /// backtest behavior unchanged.
    emission: EmissionGate,
    /// Live dead-man feeder; absent in backtests.
    heartbeats: Option<Heartbeats>,
    /// Live per-symbol mark feed; absent in backtests.
    mark_feed: Option<MarkFeed>,
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
            emission: EmissionGate::open(),
            heartbeats: None,
            mark_feed: None,
            last_recorded_session: None,
        }
    }

    /// Replace the live order-emission gate.
    #[must_use]
    pub fn with_emission_gate(mut self, emission: EmissionGate) -> Self {
        self.emission = emission;
        self
    }

    /// Thread the live dead-man feeder into bar processing.
    #[must_use]
    pub fn with_heartbeats(mut self, heartbeats: Heartbeats) -> Self {
        self.heartbeats = Some(heartbeats);
        self
    }

    /// Thread the live per-symbol mark feed into bar processing.
    #[must_use]
    pub fn with_mark_feed(mut self, mark_feed: MarkFeed) -> Self {
        self.mark_feed = Some(mark_feed);
        self
    }

    /// Configure the live strategy to claim every mounted instrument under an
    /// explicit Netting OMS. Backtests never call this builder, leaving both
    /// `StrategyConfig` fields `None` exactly as before.
    #[must_use]
    pub fn with_external_order_claims(mut self, instrument_ids: Vec<InstrumentId>) -> Self {
        self.core.config.oms_type = Some(OmsType::Netting);
        self.core.config.external_order_claims = Some(instrument_ids);
        self
    }

    /// Seed broker-confirmed overnight holdings before live reconciliation emits
    /// their `PositionOpened` events.
    ///
    /// # Errors
    ///
    /// Refuses internally inconsistent legs; callers must repair/adopt the rehearsal
    /// book rather than inventing entry-fixed risk or a stop.
    pub fn seed_open_legs(&mut self, legs: &[BookLeg]) -> Result<(), String> {
        for leg in legs {
            let risk_per_share = leg.entry_price - leg.stop_price;
            if !leg.entry_price.is_finite()
                || !leg.stop_price.is_finite()
                || leg.stop_price <= 0.0
                || risk_per_share <= 0.0
                || !leg.quantity.is_finite()
                || leg.quantity <= 0.0
            {
                return Err(format!(
                    "seeded daily leg {} is inconsistent: entry={}, stop={}, qty={}",
                    leg.instrument_id, leg.entry_price, leg.stop_price, leg.quantity
                ));
            }
            let position_id = PositionId::from(
                format!("{}-{}", leg.instrument_id, self.params.strategy_id).as_str(),
            );
            let pending = PendingLeg {
                risk_per_share,
                qty: leg.quantity,
                entry_index: leg.entry_session_ordinal,
                seeded: true,
            };
            self.pending_leg.insert(leg.instrument_id, pending);
            self.open.insert(
                leg.instrument_id,
                OpenLeg {
                    position_id,
                    entry_price: leg.entry_price,
                    stop: leg.stop_price,
                    risk_per_share,
                    qty: leg.quantity,
                    entry_index: leg.entry_session_ordinal,
                },
            );
            self.entry_risk.record(
                leg.opening_order_id,
                EntryRisk { risk_per_share, qty: leg.quantity },
            );
            self.book.seed_held(leg.instrument_id);
        }
        Ok(())
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
        let mut summary = BTreeMap::from([
            ("holding_period_sessions".to_string(), self.params.holding_period_sessions as f64),
            ("target_m".to_string(), self.params.target_m as f64),
            ("max_concurrent".to_string(), self.params.max_concurrent as f64),
            ("stop_atr_mult".to_string(), self.params.stop_atr_mult),
            ("atr_window_sessions".to_string(), self.params.atr_window_sessions),
            ("notional_per_position".to_string(), self.params.notional_per_position),
            (
                "ranking_signal_placeholder".to_string(),
                f64::from(u8::from(self.params.ranking_signal.is_placeholder())),
            ),
        ]);
        summary.insert(
            format!("ranking_signal_{}", self.params.ranking_signal.name()),
            1.0,
        );
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
        self.pending_leg.retain(|_, leg| leg.seeded);
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

        if !self.emission.allowed() {
            return Ok(None);
        }

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
        self.pending_leg.insert(
            id,
            PendingLeg {
                risk_per_share,
                qty: qty as f64,
                entry_index: ctx.index,
                seeded: false,
            },
        );

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
        if !self.emission.allowed() {
            return;
        }
        self.close_position(&position, None, None, Some(TimeInForce::Gtc), Some(true), None)
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
        if let Some(heartbeats) = &self.heartbeats {
            heartbeats.touch_runtime(chrono::Utc::now().timestamp());
        }
        if let Some(mark_feed) = &self.mark_feed {
            mark_feed.observe(
                id.symbol.as_str(),
                SymbolMark {
                    last_close: bar.close.as_f64() as i64,
                    last_bar_unix: (ts / 1_000_000_000) as i64,
                    stop_price: self.open.get(&id).map(|leg| leg.stop as i64),
                },
            );
        }
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
    fn governed_signal_variants_rank_differently_and_fail_closed_on_short_history() {
        let candidates = [
            candidate("000660.XKRX", 30.0),
            candidate("005930.XKRX", 20.0),
            candidate("035720.XKRX", 10.0),
        ];
        let histories = BTreeMap::from([
            (
                "000660.XKRX".to_string(),
                vec![100.0, 99.0, 98.0, 97.0, 96.0, 95.0, 94.0, 93.0, 92.0, 91.0, 90.0, 89.0, 88.0],
            ),
            (
                "005930.XKRX".to_string(),
                vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0, 112.0],
            ),
            ("035720.XKRX".to_string(), vec![100.0; 12]),
        ]);
        let turnover = rank_by_signal(
            RankingSignalKind::PriorTurnoverDesc,
            &candidates,
            &histories,
        );
        let momentum = rank_by_signal(
            RankingSignalKind::Momentum12x1,
            &candidates,
            &histories,
        );
        assert_eq!(turnover.ranked, vec!["000660.XKRX", "005930.XKRX", "035720.XKRX"]);
        assert_eq!(momentum.ranked, vec!["005930.XKRX", "000660.XKRX"]);
        assert_eq!(momentum.unavailable, vec!["035720.XKRX"]);
        assert_ne!(turnover.ranked, momentum.ranked);

        let sink = DecisionSink::new();
        let params = DailyParams {
            ranking_signal: RankingSignalKind::Momentum12x1,
            ..DailyParams::default()
        };
        record_signal_decisions(&sink, &params, 1, &candidates, &histories, &momentum);
        let records = sink.snapshot();
        assert_eq!(records.len(), candidates.len());
        for record in &records {
            let AgentContext::Telemetry { params_hash_or_summary, .. } = &record.context else {
                panic!("selection decisions use telemetry context")
            };
            assert_eq!(params_hash_or_summary.get("ranking_signal_momentum_12x1"), Some(&1.0));
        }
        let unavailable = records
            .iter()
            .find(|record| {
                record
                    .decision_detail
                    .as_ref()
                    .is_some_and(|detail| detail.symbol == "035720.XKRX")
            })
            .unwrap()
            .decision_detail
            .as_ref()
            .unwrap();
        assert_eq!(unavailable.filter.as_deref(), Some("signal_unavailable"));
    }

    #[test]
    fn live_claims_are_netting_while_the_backtest_config_is_unchanged() {
        let claims = vec![InstrumentId::from("005930.XKRX"), InstrumentId::from("000660.XKRX")];
        let backtest = DailyStrategy::new(
            Vec::new(),
            DailyParams::default(),
            DecisionSink::new(),
            AdjustmentBasisShifts::none(),
        );
        assert_eq!(backtest.config().oms_type, None);
        assert_eq!(backtest.config().external_order_claims, None);

        let live = backtest.with_external_order_claims(claims.clone());
        assert_eq!(live.config().oms_type, Some(OmsType::Netting));
        assert_eq!(live.config().external_order_claims, Some(claims));
    }

    #[test]
    fn live_hooks_feed_on_a_bar_and_a_closed_emission_gate_places_nothing() {
        use nautilus_ls::ingest::BarKind;
        use nautilus_model::types::Price;

        let id = InstrumentId::from("005930.XKRX");
        let gate = EmissionGate::open();
        gate.stop();
        let heartbeats = Heartbeats::new(1);
        let marks = MarkFeed::new();
        let mut strategy = DailyStrategy::new(
            Vec::new(),
            DailyParams { target_m: 1, ..DailyParams::default() },
            DecisionSink::new(),
            AdjustmentBasisShifts::none(),
        )
        .with_emission_gate(gate)
        .with_heartbeats(heartbeats.clone())
        .with_mark_feed(marks.clone());
        let date = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        strategy.signals.publish_sessions(vec![date]);
        strategy.signals.publish_session(DailySessionContext {
            index: 0,
            date,
            ranked: vec![id],
            taken: vec![id],
            held: Vec::new(),
            prior_atr: HashMap::from([(id, Some(1_000.0))]),
        });
        let ts = 1_704_265_200_000_000_000u64;
        let bar = Bar::new(
            BarKind::Daily.bar_type(id).unwrap(),
            Price::new(70_000.0, 0),
            Price::new(71_000.0, 0),
            Price::new(69_000.0, 0),
            Price::new(70_500.0, 0),
            Quantity::from(1_000),
            ts.into(),
            ts.into(),
        );
        DataActor::on_bar(&mut strategy, &bar).unwrap();

        assert!(strategy.pending_leg.is_empty(), "closed emission gate submitted no entry");
        assert!(heartbeats.runtime_unix() > 1, "the runtime heartbeat was fed");
        let mark = marks.get("005930").expect("the bare-shcode mark was published");
        assert_eq!(mark.last_close, 70_500);
        assert_eq!(mark.last_bar_unix, (ts / 1_000_000_000) as i64);
        assert_eq!(mark.stop_price, None);
    }

    #[test]
    fn restored_legs_seed_risk_and_accept_their_open_events() {
        use nautilus_core::{UnixNanos, UUID4};
        use nautilus_model::identifiers::{AccountId, TraderId};
        use nautilus_model::types::{Currency, Price};

        let ids = [InstrumentId::from("005930.XKRX"), InstrumentId::from("000660.XKRX")];
        let orders = [
            ClientOrderId::from("O-20240909-000000-001-001-1"),
            ClientOrderId::from("O-20240909-000000-001-001-2"),
        ];
        let legs = [
            BookLeg {
                instrument_id: ids[0],
                opening_order_id: orders[0],
                entry_price: 70_000.0,
                stop_price: 68_500.0,
                quantity: 10.0,
                entry_session_ordinal: 100,
            },
            BookLeg {
                instrument_id: ids[1],
                opening_order_id: orders[1],
                entry_price: 200_000.0,
                stop_price: 195_000.0,
                quantity: 3.0,
                entry_session_ordinal: 101,
            },
        ];
        let params = DailyParams::default();
        let strategy_id = StrategyId::from(params.strategy_id.as_str());
        let mut strategy = DailyStrategy::new(
            Vec::new(),
            params,
            DecisionSink::new(),
            AdjustmentBasisShifts::none(),
        );
        strategy.seed_open_legs(&legs).unwrap();
        assert_eq!(strategy.open.len(), 2);
        assert_eq!(strategy.pending_leg.len(), 2);
        strategy.discard_stale_pendings();
        assert_eq!(
            strategy.pending_leg.len(),
            2,
            "session rollover cannot erase the later reconciliation join"
        );
        assert_eq!(strategy.book.held(), ids.into_iter().collect());
        assert_eq!(strategy.entry_risk.get(&orders[0]).unwrap().risk_per_share, 1_500.0);

        for leg in &legs {
            let event = PositionOpened {
                trader_id: TraderId::from("TRADER-001"),
                strategy_id,
                instrument_id: leg.instrument_id,
                position_id: PositionId::from(
                    format!("{}-daily-ms", leg.instrument_id).as_str(),
                ),
                account_id: AccountId::from("XKRX-001"),
                opening_order_id: leg.opening_order_id,
                entry: OrderSide::Buy,
                side: PositionSide::Long,
                signed_qty: leg.quantity,
                quantity: Quantity::from(leg.quantity as i64),
                last_qty: Quantity::from(leg.quantity as i64),
                last_px: Price::new(leg.entry_price, 0),
                currency: Currency::KRW(),
                avg_px_open: leg.entry_price,
                event_id: UUID4::default(),
                ts_event: UnixNanos::from(1_000_000_000),
                ts_init: UnixNanos::from(1_000_000_000),
            };
            Strategy::on_position_opened(&mut strategy, event);
        }
        assert!(strategy.pending_leg.is_empty());
        assert_eq!(strategy.open.len(), 2);
        assert_eq!(strategy.book.opened_position_ids().len(), 2);
        assert_eq!(strategy.entry_risk.opened_entries(), orders);
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
