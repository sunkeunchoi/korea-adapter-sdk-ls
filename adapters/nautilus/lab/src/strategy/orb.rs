//! ORB v0 — the opening-range-breakout starter strategy (R2, KTD6). Two pure,
//! test-first cores plus the nautilus wrapper that mounts them:
//!
//! - [`select_universe`] — the stocks-in-play scan over prior-session daily bars
//!   (gap filter + turnover rank + top-N cap), emitting a universe decision
//!   envelope per candidate (AE2).
//! - [`OrbState`] — the per-symbol range/entry/exit state machine (the unit most
//!   likely to hide off-by-one session-time bugs, so it is built and tested in
//!   isolation of the engine).
//! - [`OrbStrategy`] — the nautilus `Strategy` that feeds bars into per-symbol
//!   [`OrbState`]s, translates actions into marketable-limit orders, and emits one
//!   telemetry [`DecisionEnvelope`] per decision (R5, R6).

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, NaiveTime, Utc};
use nautilus_model::data::{Bar, BarType};
use nautilus_model::enums::{OrderSide, TimeInForce};
use nautilus_model::identifiers::{InstrumentId, StrategyId};
use nautilus_model::types::{Price, Quantity};
use nautilus_trading::nautilus_strategy;
use nautilus_trading::strategy::{Strategy, StrategyConfig, StrategyCore};
use nautilus_common::actor::DataActor;

use crate::agent::envelope::{
    Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger, SignalKind,
};
use crate::agent::sink::DecisionSink;
use crate::params::OrbParams;

/// Convert a UTC unix-nanosecond timestamp to a KST wall-clock time (KST is a fixed
/// UTC+09:00 with no DST — the adapter's `KST_UTC_OFFSET_HOURS`).
pub fn kst_time_from_nanos(ns: u64) -> NaiveTime {
    let dt = DateTime::<Utc>::from_timestamp_nanos(ns as i64);
    let kst = dt + chrono::Duration::hours(nautilus_ls::rules::KST_UTC_OFFSET_HOURS as i64);
    kst.time()
}

// ---------------------------------------------------------------------------
// Universe scan (stocks-in-play)
// ---------------------------------------------------------------------------

/// A universe candidate assembled from prior-session daily context.
#[derive(Debug, Clone, PartialEq)]
pub struct UniverseCandidate {
    /// `{shcode}.XKRX` instrument id string.
    pub symbol: String,
    /// Prior-session close price (KRW).
    pub prior_close: f64,
    /// The session-open price the gap is measured against (KRW).
    pub today_open: f64,
    /// Prior-session turnover (value traded) used for ranking.
    pub prior_turnover: f64,
}

impl UniverseCandidate {
    /// The gap versus the prior close, in percent.
    pub fn gap_pct(&self) -> f64 {
        if self.prior_close <= 0.0 {
            return 0.0;
        }
        (self.today_open - self.prior_close) / self.prior_close * 100.0
    }
}

/// Run the stocks-in-play scan (KTD6): keep candidates whose gap ≥ `gap_min_pct`,
/// rank the survivors by prior-session turnover, and cap at `universe_top_n`. Emits
/// one universe decision envelope per candidate — accept for a selected symbol,
/// reject naming the filter (`gap` or `turnover_rank`) for the rest (R6, AE2).
/// Returns the selected symbols in rank order.
pub fn select_universe(
    candidates: &[UniverseCandidate],
    params: &OrbParams,
    sink: &DecisionSink,
    ts_event: u64,
) -> Vec<String> {
    // Partition on the gap filter first.
    let mut passed: Vec<&UniverseCandidate> = Vec::new();
    for c in candidates {
        let gap = c.gap_pct();
        if gap < params.gap_min_pct {
            emit_telemetry(
                sink,
                params,
                ts_event,
                universe_trigger(),
                DecisionDetail::universe(
                    c.symbol.clone(),
                    Decision::Reject,
                    Some("gap".to_string()),
                    vals(&[("gap_pct", gap), ("prior_close", c.prior_close), ("today_open", c.today_open)]),
                ),
            );
        } else {
            passed.push(c);
        }
    }
    // Rank survivors by prior-session turnover, descending. Ties break by symbol so
    // the selection is deterministic across runs (KTD8 comparability).
    passed.sort_by(|a, b| {
        b.prior_turnover
            .partial_cmp(&a.prior_turnover)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });

    let mut selected = Vec::new();
    for (rank, c) in passed.iter().enumerate() {
        if rank < params.universe_top_n {
            emit_telemetry(
                sink,
                params,
                ts_event,
                universe_trigger(),
                DecisionDetail::universe(
                    c.symbol.clone(),
                    Decision::Accept,
                    None,
                    vals(&[("gap_pct", c.gap_pct()), ("prior_turnover", c.prior_turnover), ("rank", rank as f64)]),
                ),
            );
            selected.push(c.symbol.clone());
        } else {
            emit_telemetry(
                sink,
                params,
                ts_event,
                universe_trigger(),
                DecisionDetail::universe(
                    c.symbol.clone(),
                    Decision::Reject,
                    Some("turnover_rank".to_string()),
                    vals(&[("prior_turnover", c.prior_turnover), ("rank", rank as f64)]),
                ),
            );
        }
    }
    selected
}

fn vals(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// Breakout strength in R-multiples: `(breakout_price − range_high) / R`, where
/// `R = range_high − range_low` (turn 10, R2/KTD6). `None` for a degenerate
/// range (`R ≤ 0`): the division would be `x/0`, and the q3 evidence carved
/// degenerate ranges out — so the caller bypasses the band-pass filter and
/// preserves legacy entry (KTD6). For a real breakout (`breakout_price >
/// range_high`) the result is strictly positive.
pub fn breakout_strength(breakout_price: i64, range_high: i64, range_low: i64) -> Option<f64> {
    let r = range_high - range_low;
    (r > 0).then(|| (breakout_price - range_high) as f64 / r as f64)
}

/// The universe scan's decision trigger: universe selection happens at session
/// open, keyed to the scan — an internal state change, not a bar (R5).
fn universe_trigger() -> DecisionTrigger {
    DecisionTrigger::StateChange { description: "universe selection scan".to_string() }
}

/// Emit one pure-telemetry [`DecisionEnvelope`] carrying `detail` (R5, R6): the
/// trigger, the strategy decision detail, and the minimal telemetry context —
/// `params`' numeric summary plus a running `decisions` count from the sink.
/// The single emission seam for the scan and the engine-thread strategy.
fn emit_telemetry(
    sink: &DecisionSink,
    params: &OrbParams,
    ts_event: u64,
    trigger: DecisionTrigger,
    detail: DecisionDetail,
) {
    let counts = BTreeMap::from([("decisions".to_string(), sink.len() as u64)]);
    sink.emit(DecisionEnvelope::telemetry(
        ts_event,
        trigger,
        detail,
        params.telemetry_context(counts),
    ));
}

// ---------------------------------------------------------------------------
// Per-symbol state machine
// ---------------------------------------------------------------------------

/// The lifecycle phase of one selected symbol over a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before the opening-range window has been observed.
    PreRange,
    /// Inside the 09:00–09:15 opening-range window.
    InRange,
    /// Range fixed, waiting for a breakout.
    Armed,
    /// A long position is held.
    Long,
    /// The symbol is finished for the session (stopped, exited, or never traded).
    Done,
}

/// Why an exit fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    /// The stop (range low) was breached.
    Stop,
    /// The time-flat deadline was reached with a position still open.
    TimeFlat,
    /// The fixed profit target (`entry_price + profit_target_r · R`) was reached
    /// while Long — banked at the target price, a favorable limit (R1).
    Target,
}

/// An action the state machine asks the strategy to take on a bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrbAction {
    /// Enter long via a marketable limit at `limit_price` (the range high).
    Enter { limit_price: i64 },
    /// Exit the long position via a marketable limit at `limit_price`.
    Exit { limit_price: i64, reason: ExitReason },
}

/// Per-symbol opening-range-breakout state (pure — no engine, no I/O).
#[derive(Debug, Clone)]
pub struct OrbState {
    phase: Phase,
    range_high: i64,
    range_low: i64,
    saw_range: bool,
    session_high: i64,
    session_low: i64,
    /// The breakout fill price (the emitted Enter limit) — the target's reference
    /// (KTD1). Zero before an entry.
    entry_price: i64,
    /// The post-entry high-water mark, updated each Long bar — the basis for
    /// per-trade MFE (KTD4). Zero before an entry.
    high_water: i64,
}

impl Default for OrbState {
    fn default() -> Self {
        OrbState {
            phase: Phase::PreRange,
            range_high: i64::MIN,
            range_low: i64::MAX,
            saw_range: false,
            session_high: i64::MIN,
            session_low: i64::MAX,
            entry_price: 0,
            high_water: 0,
        }
    }
}

impl OrbState {
    /// A fresh state for one symbol's session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a long position is currently held.
    pub fn is_long(&self) -> bool {
        self.phase == Phase::Long
    }

    /// The current phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The fixed opening range, once observed.
    pub fn range(&self) -> Option<(i64, i64)> {
        self.saw_range.then_some((self.range_high, self.range_low))
    }

    /// The session extremes observed (high, low) for the end-of-session summary
    /// (KTD9). `None` before any bar.
    pub fn session_extremes(&self) -> Option<(i64, i64)> {
        (self.session_high != i64::MIN).then_some((self.session_high, self.session_low))
    }

    /// The breakout fill price the target is measured from (KTD1). Zero before an
    /// entry.
    pub fn entry_price(&self) -> i64 {
        self.entry_price
    }

    /// Per-trade maximum-favorable-excursion in R-multiples (R5, KTD4):
    /// `(high_water − entry_price) / R`, where `R = range_high − range_low`.
    /// Returns `0.0` before an entry or when the range is degenerate (`R ≤ 0`).
    pub fn mfe_r(&self) -> f64 {
        if !self.saw_range {
            return 0.0; // range sentinels are unset — never subtract them
        }
        let r = self.range_high - self.range_low;
        if r <= 0 {
            return 0.0;
        }
        (self.high_water - self.entry_price) as f64 / r as f64
    }

    /// Force the symbol flat/done — used by the strategy when the sizing gate vetoes
    /// an otherwise-triggered entry (the position was never actually opened).
    pub fn force_done(&mut self) {
        self.phase = Phase::Done;
    }

    /// Feed one bar (KST wall-clock `t`, integer KRW `high`/`low`), returning the
    /// actions to take. Usually 0 or 1; a whipsaw bar (breakout that also breaches the
    /// stop) returns both an [`OrbAction::Enter`] and an [`OrbAction::Exit`].
    pub fn on_bar(&mut self, t: NaiveTime, high: i64, low: i64, params: &OrbParams) -> Vec<OrbAction> {
        // Track session extremes on every bar.
        self.session_high = self.session_high.max(high);
        self.session_low = self.session_low.min(low);

        if self.phase == Phase::Done {
            return Vec::new();
        }
        if t < params.range_open {
            return Vec::new(); // pre-open (should not occur with session bars)
        }

        let range_end = params.range_end();
        if t < range_end {
            // Inside the opening-range window: accumulate the range.
            if self.saw_range {
                self.range_high = self.range_high.max(high);
                self.range_low = self.range_low.min(low);
            } else {
                self.range_high = high;
                self.range_low = low;
                self.saw_range = true;
            }
            self.phase = Phase::InRange;
            return Vec::new();
        }

        // At/after the range end. Without a fixed range (a data gap over 09:00–09:15)
        // there is nothing to trade — never guess a range.
        if !self.saw_range {
            self.phase = Phase::Done;
            return Vec::new();
        }

        // Time-flat deadline: close any open position at a marketable limit.
        if t >= params.flat_time {
            let mut acts = Vec::new();
            if self.phase == Phase::Long {
                // The flat bar is still part of the hold — fold its high into the
                // high-water mark so a TimeFlat exit's MFE matches the Stop/Target
                // exits (R5: MFE covers the whole trade, including the exit bar).
                self.high_water = self.high_water.max(high);
                acts.push(OrbAction::Exit { limit_price: low, reason: ExitReason::TimeFlat });
            }
            self.phase = Phase::Done;
            return acts;
        }

        // Trading window (range_end ≤ t < flat_time).
        if self.phase == Phase::InRange {
            self.phase = Phase::Armed;
        }
        let mut acts = Vec::new();
        if self.phase == Phase::Armed && high > self.range_high {
            // Enter with a *marketable* buy limit at the breakout bar's high (the
            // range high is the trigger; the fill price must be marketable, KTD6).
            acts.push(OrbAction::Enter { limit_price: high });
            self.phase = Phase::Long;
            // The fill price is the target's reference (KTD1); seed the high-water
            // mark so MFE starts at the entry (R5, KTD4).
            self.entry_price = high;
            self.high_water = high;
        }
        if self.phase == Phase::Long {
            // Track the post-entry peak for per-trade MFE. On the entry bar this is
            // a no-op (high_water was just set to high).
            self.high_water = self.high_water.max(high);
            if low <= self.range_low {
                // Stop first (KTD2 / R4): when a bar breaches both the stop and the
                // target, Stop wins — intrabar order is unknowable, so fail toward
                // the conservative side (matches KTD6's pessimistic fills). This is
                // also the whipsaw same-bar enter+stop path (R3), unchanged.
                acts.push(OrbAction::Exit { limit_price: low, reason: ExitReason::Stop });
                self.phase = Phase::Done;
            } else if let Some(target) = self.target_price(params) {
                if high >= target {
                    // Bank the move at the target price — a favorable limit, not the
                    // bar wick (R1). The entry bar can never reach here: its high
                    // equals entry_price < target.
                    acts.push(OrbAction::Exit { limit_price: target, reason: ExitReason::Target });
                    self.phase = Phase::Done;
                }
            }
        }
        acts
    }

    /// The fixed profit-target price `entry_price + round(profit_target_r · R)`
    /// (R1, KTD1), or `None` when the range is degenerate (`R ≤ 0`) or the target
    /// size is non-positive — either way the entry bar can never trip the target
    /// (a `profit_target_r ≤ 0` from a hand-seeded manifest must not fire an
    /// immediate same-bar breakeven exit).
    fn target_price(&self, params: &OrbParams) -> Option<i64> {
        let r = self.range_high - self.range_low;
        if r <= 0 || params.profit_target_r <= 0.0 {
            return None;
        }
        Some(self.entry_price + (params.profit_target_r * r as f64).round() as i64)
    }
}

// ---------------------------------------------------------------------------
// nautilus Strategy wrapper
// ---------------------------------------------------------------------------

/// A shared switch that gates the strategy's order emission. Open by default; the
/// live runner closes it at teardown so a signal arriving mid-teardown places no
/// order (KTD7: stop the strategy's order emission first). Thread-safe and cloneable.
#[derive(Debug, Clone)]
pub struct EmissionGate(Arc<AtomicBool>);

impl EmissionGate {
    /// An open gate — order emission allowed.
    pub fn open() -> Self {
        EmissionGate(Arc::new(AtomicBool::new(true)))
    }
    /// Whether order emission is currently allowed.
    pub fn allowed(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    /// Close the gate — no further orders are emitted.
    pub fn stop(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Default for EmissionGate {
    fn default() -> Self {
        Self::open()
    }
}

/// A selected symbol and the bar series the strategy trades it on.
#[derive(Debug, Clone)]
pub struct SelectedSymbol {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument_id: InstrumentId,
    /// The bar type to subscribe (typically the 1-minute series).
    pub bar_type: BarType,
}

/// The ORB v0 nautilus strategy. Mounts one [`OrbState`] per selected symbol, feeds
/// each incoming bar to its state, and translates actions into marketable-limit
/// orders while emitting one telemetry decision envelope per decision (KTD9, R6).
/// Runnable unchanged in both the backtest engine and a live node (R2).
pub struct OrbStrategy {
    core: StrategyCore,
    params: OrbParams,
    selected: Vec<SelectedSymbol>,
    states: HashMap<InstrumentId, OrbState>,
    entered_qty: HashMap<InstrumentId, i64>,
    decisions: DecisionSink,
    emission: EmissionGate,
}

impl OrbStrategy {
    /// Build the strategy for a resolved universe + parameter set, writing its
    /// per-decision telemetry envelopes into `decisions`.
    pub fn new(params: OrbParams, selected: Vec<SelectedSymbol>, decisions: DecisionSink) -> Self {
        let base = StrategyConfig {
            strategy_id: Some(StrategyId::from(strategy_id_str(&params).as_str())),
            ..Default::default()
        };
        let states = selected.iter().map(|s| (s.instrument_id, OrbState::new())).collect();
        OrbStrategy {
            core: StrategyCore::new(base),
            params,
            selected,
            states,
            entered_qty: HashMap::new(),
            decisions,
            emission: EmissionGate::open(),
        }
    }

    /// A clone of the emission gate — the live runner closes it at teardown so a
    /// late signal places no order (KTD7).
    pub fn emission_gate(&self) -> EmissionGate {
        self.emission.clone()
    }

    /// The number of symbols currently holding a long position, excluding `except`.
    fn open_positions_excluding(&self, except: &InstrumentId) -> usize {
        self.states
            .iter()
            .filter(|(id, st)| *id != except && st.is_long())
            .count()
    }

    /// Submit a marketable limit order for one selected symbol. `reduce_only` is set
    /// on exits so an exit can only flatten the long — never flip it short if the
    /// entry has not filled yet (the whipsaw same-bar enter+stop case).
    fn place(
        &mut self,
        id: InstrumentId,
        side: OrderSide,
        price: i64,
        qty: i64,
        reduce_only: bool,
    ) -> anyhow::Result<()> {
        let order = self.order().limit(
            id,
            side,
            Quantity::from(qty),
            Price::from(price.to_string().as_str()),
            Some(TimeInForce::Day),
            None,               // expire_time
            None,               // post_only
            Some(reduce_only),  // reduce_only
            None, None, None, None, None, None, None, None,
        );
        self.submit_order(order, None, None, None)
    }

    /// Emit one intraday telemetry envelope for a bar-driven decision on `id`
    /// (the [`DecisionTrigger::MarketData`] trigger, R5).
    fn emit_market_data(&self, id: InstrumentId, ts: u64, detail: DecisionDetail) {
        emit_telemetry(
            &self.decisions,
            &self.params,
            ts,
            DecisionTrigger::MarketData { instrument_id: id },
            detail,
        );
    }

    /// Translate one bar's state-machine actions into orders + telemetry envelopes.
    fn handle_actions(
        &mut self,
        id: InstrumentId,
        symbol: String,
        ts: u64,
        actions: Vec<OrbAction>,
    ) -> anyhow::Result<()> {
        for action in actions {
            match action {
                OrbAction::Enter { limit_price } => {
                    let open = self.open_positions_excluding(&id);
                    let qty = self.params.position_qty(limit_price as f64);
                    let range = self.states.get(&id).and_then(|s| s.range()).unwrap_or((0, 0));
                    // Breakout strength = (breakout_price − range_high) / R (R2,
                    // KTD3). `None` for a degenerate range (R ≤ 0), which bypasses
                    // the band-pass; the `Breakout` envelope records 0.0 for it.
                    let strength = breakout_strength(limit_price, range.0, range.1);
                    self.emit_market_data(id, ts, DecisionDetail::transition(
                        symbol.clone(),
                        SignalKind::Breakout,
                        vals(&[
                            ("range_high", range.0 as f64),
                            ("range_low", range.1 as f64),
                            ("breakout_price", limit_price as f64),
                            ("strength", strength.unwrap_or(0.0)),
                        ]),
                    ));
                    // Breakout-strength band-pass (turn 10, R2/R3, KTD3/KTD4/KTD6):
                    // the q3 evidence is a band — both the marginal (q2) and the
                    // strongest (q4) breakouts lose. An out-of-band strength rejects
                    // the entry done-for-day, ahead of the emission/sizing/qty
                    // composite so the label stays truthful. A degenerate range
                    // (strength `None`) has no evidence basis to reject, so it enters.
                    if strength.is_some_and(|s| !self.params.strength_in_band(s)) {
                        if let Some(st) = self.states.get_mut(&id) {
                            st.force_done();
                        }
                        self.emit_market_data(id, ts, DecisionDetail {
                            kind: SignalKind::OrderRejectedSizing,
                            symbol: symbol.clone(),
                            decision: None,
                            filter: Some("breakout_strength_band".to_string()),
                            values: vals(&[
                                ("strength", strength.unwrap_or(0.0)),
                                ("breakout_strength_min", self.params.breakout_strength_min),
                                ("breakout_strength_max", self.params.breakout_strength_max),
                            ]),
                        });
                        continue;
                    }
                    if !self.emission.allowed() || !self.params.sizing_allows(open) || qty <= 0 {
                        // Emission stopped (teardown), or the sizing / concurrency gate
                        // vetoes the entry: the position is never opened, so roll the
                        // state to Done and record why.
                        if let Some(st) = self.states.get_mut(&id) {
                            st.force_done();
                        }
                        let filter = if !self.emission.allowed() {
                            "emission_stopped"
                        } else if qty <= 0 {
                            "notional_too_small"
                        } else {
                            "max_concurrent"
                        };
                        self.emit_market_data(id, ts, DecisionDetail {
                            kind: SignalKind::OrderRejectedSizing,
                            symbol: symbol.clone(),
                            decision: None,
                            filter: Some(filter.to_string()),
                            values: vals(&[("open_positions", open as f64), ("qty", qty as f64)]),
                        });
                        continue;
                    }
                    self.entered_qty.insert(id, qty);
                    self.place(id, OrderSide::Buy, limit_price, qty, false)?;
                    self.emit_market_data(id, ts, DecisionDetail::transition(
                        symbol.clone(),
                        SignalKind::OrderPlaced,
                        vals(&[("qty", qty as f64), ("price", limit_price as f64)]),
                    ));
                }
                OrbAction::Exit { limit_price, reason } => {
                    let qty = self.entered_qty.get(&id).copied().unwrap_or(0);
                    // During teardown the runner flattens out-of-band; the strategy
                    // must not also emit an exit order.
                    if qty <= 0 || !self.emission.allowed() {
                        continue;
                    }
                    self.place(id, OrderSide::Sell, limit_price, qty, true)?;
                    let kind = match reason {
                        ExitReason::Stop => SignalKind::StopHit,
                        ExitReason::TimeFlat => SignalKind::TimeExit,
                        ExitReason::Target => SignalKind::Target,
                    };
                    // Per-trade MFE (R5) rides every exit envelope so the next
                    // exit-tuning turn reads give-back directly.
                    let mfe_r = self.states.get(&id).map(|s| s.mfe_r()).unwrap_or(0.0);
                    self.emit_market_data(id, ts, DecisionDetail::transition(
                        symbol.clone(),
                        kind,
                        vals(&[("qty", qty as f64), ("price", limit_price as f64), ("mfe_r", mfe_r)]),
                    ));
                    self.entered_qty.remove(&id);
                }
            }
        }
        Ok(())
    }
}

/// The `{id}-v{version}` strategy id string (stable, manifest-recorded).
pub fn strategy_id_str(params: &OrbParams) -> String {
    format!("{}-v{}", params.strategy_id, params.strategy_version)
}

nautilus_strategy!(OrbStrategy);

impl Debug for OrbStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrbStrategy")
            .field("selected", &self.selected.len())
            .field("params", &self.params.strategy_version)
            .finish()
    }
}

impl DataActor for OrbStrategy {
    fn on_start(&mut self) -> anyhow::Result<()> {
        for s in self.selected.clone() {
            self.subscribe_bars(s.bar_type, None, None);
        }
        Ok(())
    }

    fn on_bar(&mut self, bar: &Bar) -> anyhow::Result<()> {
        let id = bar.bar_type.instrument_id();
        if !self.states.contains_key(&id) {
            return Ok(());
        }
        let t = kst_time_from_nanos(bar.ts_event.as_u64());
        let high = bar.high.as_f64() as i64;
        let low = bar.low.as_f64() as i64;
        let params = self.params.clone();
        let actions = self.states.get_mut(&id).expect("state present").on_bar(t, high, low, &params);
        if actions.is_empty() {
            return Ok(());
        }
        let symbol = id.to_string();
        self.handle_actions(id, symbol, bar.ts_event.as_u64(), actions)
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        // End-of-session summary per selected symbol (KTD9): the extreme signal
        // values observed over the session.
        let summaries: Vec<(InstrumentId, (i64, i64))> = self
            .selected
            .iter()
            .filter_map(|s| {
                self.states
                    .get(&s.instrument_id)
                    .and_then(|st| st.session_extremes())
                    .map(|ex| (s.instrument_id, ex))
            })
            .collect();
        for (id, (hi, lo)) in summaries {
            // Session summaries fire at strategy stop, not on a bar — the
            // trigger is the stop-time state change (R5), mirroring
            // `universe_trigger()`'s pattern for non-bar-driven cycles.
            emit_telemetry(
                &self.decisions,
                &self.params,
                0,
                DecisionTrigger::StateChange { description: "session end summary".to_string() },
                DecisionDetail::transition(
                    id.to_string(),
                    SignalKind::SessionSummary,
                    vals(&[("session_high", hi as f64), ("session_low", lo as f64)]),
                ),
            );
        }
        Ok(())
    }
}
