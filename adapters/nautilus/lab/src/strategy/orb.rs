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
use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveTime, Utc};
use nautilus_model::data::{Bar, BarType};
use nautilus_model::enums::{OrderSide, TimeInForce};
use nautilus_model::identifiers::{InstrumentId, StrategyId};
use nautilus_model::types::{Price, Quantity};
use nautilus_trading::nautilus_strategy;
use nautilus_trading::strategy::{Strategy, StrategyConfig, StrategyCore};
use nautilus_common::actor::DataActor;

use nautilus_ls::reference::universe_metadata::ConditionerTags;

use crate::agent::envelope::{
    Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger, SignalKind,
};
use crate::agent::sink::DecisionSink;
use crate::artifacts::performance::EntryRisk;
use crate::params::{OrbParams, StopMode};

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

/// A candidate's reference-data join state (plan 2026-07-10-003, U4). A
/// metadata-less run gates nothing (`Untagged`, the legacy path); a
/// metadata-driven run either joined a record (`Tagged` — gate + tags) or did
/// not (`Missing` — non-selectable and recorded, never silently defaulted, R4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CandidateMeta {
    /// No metadata artifact was supplied — legacy selection, no gate, no tags.
    Untagged,
    /// The artifact was supplied but carries no record for this symbol.
    Missing,
    /// Joined metadata: the hard tradability verdict + the R9 conditioner tags.
    Tagged {
        /// The surveillance gate's verdict (R3) — `false` excludes (AE3).
        tradable: bool,
        /// The five conditioner tags that ride the accept envelope (R9).
        tags: ConditionerTags,
    },
}

/// Canonical integer prices defining a symbol-session's opening gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionGapPrices {
    /// Prior-session close in integer KRW/ticks.
    pub prior_close: i64,
    /// Current-session open in integer KRW/ticks.
    pub today_open: i64,
}

impl SessionGapPrices {
    /// Construct the canonical price pair.
    pub const fn new(prior_close: i64, today_open: i64) -> Self {
        Self { prior_close, today_open }
    }

    /// The opening gap versus the prior close, in percent.
    fn gap_pct(self) -> f64 {
        if self.prior_close <= 0 {
            return 0.0;
        }
        (self.today_open - self.prior_close) as f64 / self.prior_close as f64 * 100.0
    }
}

/// A universe candidate assembled from prior-session daily context.
#[derive(Debug, Clone, PartialEq)]
pub struct UniverseCandidate {
    /// `{shcode}.XKRX` instrument id string.
    pub symbol: String,
    /// Canonical integer prices defining this symbol-session's opening gap.
    pub gap_prices: SessionGapPrices,
    /// Prior-session turnover (value traded) used for ranking.
    pub prior_turnover: f64,
    /// The reference-data join state (U4).
    pub meta: CandidateMeta,
    /// Prior-daily ATR(`atr_window`) strictly before the session (KTD5), or
    /// `None` when fewer than `atr_window`+1 prior sessions exist. Read only by
    /// the stop / OR-width gates in the strategy — never by universe selection
    /// (R4), so its presence is selection-neutral.
    pub prior_atr: Option<f64>,
    /// Mean opening-window volume over up to `rvol_window_sessions` prior in-range
    /// sessions (KTD9), or `None` below `rvol_min_history` samples. The RVOL
    /// gate's baseline; never read by selection (R4).
    pub prior_open_vol_mean: Option<f64>,
    /// Prior-session Amihud illiquidity over the `atr_window` sessions strictly before
    /// the session (plan 2026-07-16-003), or `None` when under-covered. Read only by the
    /// liquidity budget tilt in the strategy — never by universe selection (R4).
    pub prior_illiq: Option<f64>,
}

impl UniverseCandidate {
    /// The gap versus the prior close, in percent.
    pub fn gap_pct(&self) -> f64 {
        self.gap_prices.gap_pct()
    }
}

/// Run the stocks-in-play scan (KTD6): gate on tradability + the liquidity
/// floor (plan 2026-07-10-003, U4 — before the legacy filters), keep candidates
/// whose gap ≥ `gap_min_pct`, rank the survivors by prior-session turnover, and
/// cap at `universe_top_n`. Emits one universe decision envelope per candidate —
/// accept for a selected symbol (carrying the R9 conditioner tags when the run
/// is metadata-driven), reject naming the filter (`missing_metadata`,
/// `not_tradable`, `turnover_floor`, `gap`, or `turnover_rank`) for the rest
/// (R6, AE2/AE3). Returns the selected symbols in rank order.
pub fn select_universe(
    candidates: &[UniverseCandidate],
    params: &OrbParams,
    sink: &DecisionSink,
    ts_event: u64,
) -> Vec<String> {
    let mut passed: Vec<&UniverseCandidate> = Vec::new();
    for c in candidates {
        let reject = |filter: &str, values: BTreeMap<String, f64>| {
            emit_telemetry(
                sink,
                params,
                ts_event,
                universe_trigger(),
                DecisionDetail::universe(
                    c.symbol.clone(),
                    Decision::Reject,
                    Some(filter.to_string()),
                    values,
                ),
            );
        };
        // Metadata gate first (U4): a symbol the artifact does not cover is
        // non-selectable and recorded (R4); a designated symbol is excluded
        // even when its gap and turnover qualify (R3, AE3).
        match c.meta {
            CandidateMeta::Missing => {
                // Carry the gap diagnostics too (review finding): an operator
                // triaging a missing-metadata reject needs "would it have
                // gapped in" answerable from the envelope alone.
                reject(
                    "missing_metadata",
                    vals(&[
                        ("gap_pct", c.gap_pct()),
                        ("prior_close", c.gap_prices.prior_close as f64),
                        ("today_open", c.gap_prices.today_open as f64),
                        ("prior_turnover", c.prior_turnover),
                    ]),
                );
                continue;
            }
            CandidateMeta::Tagged { tradable: false, .. } => {
                reject("not_tradable", vals(&[("gap_pct", c.gap_pct()), ("prior_turnover", c.prior_turnover)]));
                continue;
            }
            CandidateMeta::Untagged | CandidateMeta::Tagged { tradable: true, .. } => {}
        }
        // Liquidity floor (R5): evaluated on the daily-bar prior_turnover —
        // present for every ingested candidate, so an `Unavailable` capture
        // turnover never silently passes or fails the floor. Default 0.0 = off.
        if params.turnover_floor_krw > 0.0 && c.prior_turnover < params.turnover_floor_krw {
            reject(
                "turnover_floor",
                vals(&[("prior_turnover", c.prior_turnover), ("turnover_floor_krw", params.turnover_floor_krw)]),
            );
            continue;
        }
        let gap = c.gap_pct();
        if gap < params.gap_min_pct {
            reject(
                "gap",
                vals(&[
                    ("gap_pct", gap),
                    ("prior_close", c.gap_prices.prior_close as f64),
                    ("today_open", c.gap_prices.today_open as f64),
                ]),
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
            // The accept envelope carries the full conditioner-tag set (R9,
            // KTD4) so every resulting trade is attributable to its tier via
            // the (symbol, session) join — no artifact re-read at report time.
            let tags = match c.meta {
                CandidateMeta::Tagged { tags, .. } => Some(tags),
                _ => None,
            };
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
                )
                .with_tags(tags),
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

/// Build a per-session gate rejection action (KTD7): the canonical filter name
/// plus the operative gate inputs for the rejection envelope's `values` map.
fn session_reject(filter: &'static str, values: Vec<(&'static str, f64)>) -> OrbAction {
    OrbAction::SessionReject { filter, values }
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

/// The classified opening-range gap-retention observation (#165/#168, KTD3). One
/// variant per frozen #165 rejection class plus the measured ratio. Classification
/// is total and ordered — applicability before availability before the division —
/// so a zero gap never reaches the divide and cannot masquerade as invalid.
#[derive(Debug, Clone, Copy, PartialEq)]
enum GapRetention {
    /// The #165 applicability precondition failed: a non-positive `prior_close`
    /// (including the unthreaded `SessionGapPrices::new(0, 0)` default) or a
    /// non-positive gap (`today_open <= prior_close`).
    NotApplicable,
    /// No valid opening-range low exists: the `i64::MAX` sentinel (no range bar
    /// observed) or a non-positive low.
    Unavailable,
    /// Inconsistent data: retention above `1.0` (`range_low > today_open`) or a
    /// non-finite ratio (defensive — see the classifier's invalid arm).
    Invalid,
    /// A valid retention fraction, signed: `1.0` is full retention, `0.0` is a
    /// prior-close touch, negative means the range crossed below the prior close.
    Measured(f64),
}

/// Classify a session's opening-range gap retention (#165/#168, KTD3):
/// `retention = (range_low − prior_close) / (today_open − prior_close)` where all
/// three inputs are canonical integer KRW/ticks — the subtraction happens on
/// `i64`, and only the final quotient is `f64`. `range_low` must be the frozen
/// opening-range low (never the still-updating session low).
fn classify_gap_retention(gap_prices: SessionGapPrices, range_low: i64) -> GapRetention {
    let SessionGapPrices { prior_close, today_open } = gap_prices;
    if prior_close <= 0 || today_open <= prior_close {
        return GapRetention::NotApplicable;
    }
    if range_low == i64::MAX || range_low <= 0 {
        return GapRetention::Unavailable;
    }
    let retention = (range_low - prior_close) as f64 / (today_open - prior_close) as f64;
    if !retention.is_finite() || retention > 1.0 {
        // Above-one is a real class (range_low > today_open — inconsistent data).
        // Non-finite is defensive only: the not-applicable arm already guarantees
        // a positive denominator, so finite `i64` operands cannot produce it.
        return GapRetention::Invalid;
    }
    GapRetention::Measured(retention)
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
// Not `Eq`: `SessionReject` carries `f64` gate values for the rejection envelope.
#[derive(Debug, Clone, PartialEq)]
pub enum OrbAction {
    /// Enter long via a marketable limit at `limit_price` (the breakout fill).
    Enter { limit_price: i64 },
    /// Exit the long position via a marketable limit at `limit_price`.
    Exit { limit_price: i64, reason: ExitReason },
    /// A per-session gate rejected the symbol done-for-day before any entry
    /// (KTD7): no order is placed, one rejection envelope naming `filter` is
    /// recorded, and the symbol takes no trade that session. `values` are the
    /// operative gate inputs for the envelope's `values` map.
    SessionReject { filter: &'static str, values: Vec<(&'static str, f64)> },
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
    /// The stop price fixed at entry by `stop_mode` (KTD1/KTD4/KTD5): range low
    /// (v9), rounded OR-midpoint, or ATR-clamped. Zero before an entry. The Long
    /// exit checks `low ≤ stop_price` — at mode 0 this equals the v9
    /// `low ≤ range_low`, so the default path is byte-identical.
    stop_price: i64,
    /// The R-denominator fixed at entry for the target and MFE (KTD4): range-R
    /// (`range_high − range_low`) at mode 0 (v9-verbatim), else trade-R
    /// (`entry_price − stop_price`). Zero before an entry.
    r_denom: i64,
    /// Today's accumulated opening-window (`[range_open, range_end)`) volume
    /// (KTD9) — the RVOL gate's numerator. Inert unless the RVOL gate is on.
    open_window_vol: f64,
    /// Canonical gap prices threaded from selection (#167). Read only by the armed
    /// gap-retention gate (#168) — the `1.0` OFF sentinel bypasses every read.
    gap_prices: SessionGapPrices,
    /// The prior-daily ATR for this symbol-session (KTD5), threaded from the
    /// candidate seam (U2). `None` when fewer than `atr_window`+1 priors — the
    /// ATR stop / OR-width gate fail closed rather than silently fall back.
    prior_atr: Option<f64>,
    /// The prior opening-window volume mean for this symbol-session (KTD9),
    /// threaded from the candidate seam (U2). `None` below `rvol_min_history`.
    prior_open_vol_mean: Option<f64>,
    /// The prior-session Amihud illiquidity for this symbol-session (plan
    /// 2026-07-16-003), threaded from the candidate seam. `None` when under-covered —
    /// the liquidity budget tilt then fails closed to the neutral weight.
    prior_illiq: Option<f64>,
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
            stop_price: 0,
            r_denom: 0,
            open_window_vol: 0.0,
            gap_prices: SessionGapPrices::new(0, 0),
            prior_atr: None,
            prior_open_vol_mean: None,
            prior_illiq: None,
        }
    }
}

impl OrbState {
    /// A fresh state for one symbol's session.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh state seeded with the candidate-seam priors (U2): the prior-daily
    /// ATR, the prior opening-window volume mean, and the prior-session Amihud
    /// illiquidity the gates / budget tilts read (KTD5, KTD9, plan 2026-07-16-003).
    pub fn with_priors(
        prior_atr: Option<f64>,
        prior_open_vol_mean: Option<f64>,
        prior_illiq: Option<f64>,
    ) -> Self {
        OrbState { prior_atr, prior_open_vol_mean, prior_illiq, ..Self::default() }
    }

    /// A fresh state seeded from the selected-symbol boundary. Prices stay in
    /// canonical integer KRW/ticks; #167 only carries them and never evaluates
    /// retention while the manifest sentinel is OFF.
    pub fn with_session_inputs(
        gap_prices: SessionGapPrices,
        prior_atr: Option<f64>,
        prior_open_vol_mean: Option<f64>,
        prior_illiq: Option<f64>,
    ) -> Self {
        OrbState {
            gap_prices,
            prior_atr,
            prior_open_vol_mean,
            prior_illiq,
            ..Self::default()
        }
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

    /// The entry-fixed stop price set by `stop_mode` at the Long transition (KTD1).
    /// Zero before an entry.
    pub fn stop_price(&self) -> i64 {
        self.stop_price
    }

    /// The entry-fixed **per-share risk** used by the `risk_per_trade_krw` sizing
    /// lever (R5): `entry_price − stop_price` (the initial stop distance — the money
    /// lost per share if stopped at entry). Zero/non-positive before an entry or on a
    /// degenerate stop, which the sizing path treats as "fall back to notional".
    pub fn risk_per_share(&self) -> i64 {
        self.entry_price - self.stop_price
    }

    /// Per-trade maximum-favorable-excursion in R-multiples (R5, KTD4):
    /// `(high_water − entry_price) / R`, where `R` is the entry-fixed
    /// `r_denom` — range-R at the v9 range-low stop (byte-identical to the old
    /// `range_high − range_low`), trade-R (`entry − stop`) in the non-default
    /// stop modes (KTD4). Returns `0.0` before an entry (`r_denom` is `0`), before
    /// a range (`saw_range` false), or on a degenerate `R ≤ 0`.
    pub fn mfe_r(&self) -> f64 {
        if !self.saw_range {
            return 0.0; // range sentinels are unset — never subtract them
        }
        if self.r_denom <= 0 {
            return 0.0;
        }
        (self.high_water - self.entry_price) as f64 / self.r_denom as f64
    }

    /// The realized exit in R-multiples for a fill at `exit_price` (R5, KTD12):
    /// `(exit_price − entry_price) / R`, the entry-fixed `r_denom` (same denominator
    /// as [`OrbState::mfe_r`]). Pure telemetry that rides every exit envelope so the
    /// breakeven-trail turn can read give-back-cohort realized-R directly — a
    /// breakeven scratch books ≈0, a trailed partial books positive, a stop-out
    /// books negative. Returns `0.0` before an entry, before a range, or on a
    /// degenerate `R ≤ 0` (mirroring `mfe_r`'s guards).
    pub fn realized_exit_r(&self, exit_price: i64) -> f64 {
        if !self.saw_range || self.r_denom <= 0 {
            return 0.0;
        }
        (exit_price - self.entry_price) as f64 / self.r_denom as f64
    }

    /// Force the symbol flat/done — used by the strategy when the sizing gate vetoes
    /// an otherwise-triggered entry (the position was never actually opened).
    pub fn force_done(&mut self) {
        self.phase = Phase::Done;
    }

    /// Feed one bar (KST wall-clock `t`, integer KRW `high`/`low`/`close`, and the
    /// bar `volume`), returning the actions to take. Usually 0 or 1; a whipsaw bar
    /// (breakout that also breaches the stop) returns both an [`OrbAction::Enter`]
    /// and an [`OrbAction::Exit`]; a per-session gate reject returns a single
    /// [`OrbAction::SessionReject`]. `close`/`volume` drive the lever-queue gates
    /// (close-confirmed entry, RVOL); at the filter-off defaults they are unread by
    /// the entry decision (only `volume` accumulates, inertly), so the wick-touch
    /// path is byte-identical to v9 (KTD6, R3).
    pub fn on_bar(
        &mut self,
        t: NaiveTime,
        high: i64,
        low: i64,
        close: i64,
        volume: f64,
        params: &OrbParams,
    ) -> Vec<OrbAction> {
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
            // Inside the opening-range window: accumulate the range and today's
            // opening-window volume (the RVOL gate numerator, KTD9 — inert off).
            if self.saw_range {
                self.range_high = self.range_high.max(high);
                self.range_low = self.range_low.min(low);
            } else {
                self.range_high = high;
                self.range_low = low;
                self.saw_range = true;
            }
            self.open_window_vol += volume;
            self.phase = Phase::InRange;
            return Vec::new();
        }

        // At/after the range end. Without a fixed range (a data gap over 09:00–09:15)
        // there is nothing to trade — never guess a range. When the gap-retention
        // gate is armed, this missingness cannot roll to Done silently (#168, KTD4):
        // the sentinel `range_low` routes through the same classifier, recording
        // `gap_retention_unavailable` (or `gap_retention_not_applicable` when the
        // gap precondition also fails). OFF keeps the silent roll; the terminal
        // outcome (Done, no trade) is identical either way.
        if !self.saw_range {
            self.phase = Phase::Done;
            if let Some(reject) = self.gap_retention_reject(params) {
                return vec![reject];
            }
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
        let mut acts = Vec::new();
        if self.phase == Phase::InRange {
            // Range just fixed → arm, then evaluate the per-session gates (KTD7).
            // A gate rejection ends the day before any entry can occur.
            self.phase = Phase::Armed;
            if let Some(reject) = self.session_gate_reject(params) {
                self.phase = Phase::Done;
                acts.push(reject);
                return acts;
            }
        }

        // Entry cutoff (lever 4, KTD10): once the cutoff time is reached, an Armed
        // symbol takes no new entry — one done-for-day transition, no per-bar spam.
        // Open positions are untouched (they exit at stop/target/flat as today).
        if self.phase == Phase::Armed {
            if let Some(cutoff) = params.entry_cutoff_time() {
                if t >= cutoff {
                    self.phase = Phase::Done;
                    acts.push(OrbAction::SessionReject {
                        filter: "entry_cutoff",
                        values: vec![("entry_cutoff_min", params.entry_cutoff_min)],
                    });
                    return acts;
                }
            }
        }

        // Entry trigger, mode-dependent (KTD6). Wick-touch (default) enters on the
        // bar HIGH exceeding the range high — the v9 path, byte-identical.
        // Close-confirmed enters on the bar CLOSE strictly above the range high, at
        // that close (the confirm bar's above-close wick is not folded into MFE).
        if self.phase == Phase::Armed {
            let entry_price = if params.close_confirm_entry() {
                (close > self.range_high).then_some(close)
            } else {
                (high > self.range_high).then_some(high)
            };
            if let Some(px) = entry_price {
                acts.push(OrbAction::Enter { limit_price: px });
                self.phase = Phase::Long;
                // The fill price is the target's reference (KTD1); seed the
                // high-water mark so MFE starts at the entry (R5, KTD4).
                self.entry_price = px;
                self.high_water = px;
                self.stop_price = self.stop_for_entry(px, params);
                self.r_denom = self.entry_r_denom(px, self.stop_price, params);
                // The entry bar's above-entry wick is not provably post-fill (KTD6 /
                // stop-first pessimism): no target and no fold from it. Only the
                // stop can still fire same-bar (whipsaw / same-bar stop-first) — but
                // only in wick-touch mode, where the fill sits mid-bar at the range
                // high and a lower tick could follow it. In close-confirm mode the
                // fill is anchored at the bar CLOSE (the bar's last event), so the
                // stop-touching low is provably PRE-fill: the position was not open
                // when the low printed. Skip the same-bar stop there (symmetric with
                // not folding the entry bar's high) — a deliberate deviation from
                // KTD6's wick-entry "stop-first wins". The stop still binds from the
                // next bar on.
                if !params.close_confirm_entry() && low <= self.stop_price {
                    acts.push(OrbAction::Exit { limit_price: low, reason: ExitReason::Stop });
                    self.phase = Phase::Done;
                }
                return acts;
            }
        }
        if self.phase == Phase::Long {
            // Determine the exit BEFORE folding the bar into the high-water mark
            // (turn 10, R6 / KTD5): MFE folds only the excursion provably observed
            // while the position was open.
            if low <= self.stop_price {
                // Stop first (KTD2 / R4): when a bar breaches both the stop and the
                // target, Stop wins — intrabar order is unknowable, so fail toward
                // the conservative side (matches KTD6's pessimistic fills). The stop
                // bar's high is NOT folded — under stop-first pessimism it is not
                // provably pre-stop (KTD5). At mode 0 `stop_price == range_low`, so
                // this is the v9 `low ≤ range_low` check verbatim.
                acts.push(OrbAction::Exit { limit_price: low, reason: ExitReason::Stop });
                self.phase = Phase::Done;
            } else if let Some(target) = self.target_price(params) {
                if high >= target {
                    // Bank the move at the target price — a favorable limit, not the
                    // bar wick (R1). Fold capped at the target: price provably reached
                    // it, but the above-target wick is not provably pre-exit (KTD5),
                    // so MFE right-censors at profit_target_r·R.
                    self.high_water = self.high_water.max(target);
                    acts.push(OrbAction::Exit { limit_price: target, reason: ExitReason::Target });
                    self.phase = Phase::Done;
                } else {
                    // No exit this bar → fold the full bar high (unchanged).
                    self.high_water = self.high_water.max(high);
                }
            } else {
                // No target configured, no stop → fold the full bar high (unchanged).
                self.high_water = self.high_water.max(high);
            }
            // Breakeven-move ratchet (lever 6, KTD11) + breakeven-trail (candidate A,
            // KTD12) — evaluated AFTER folding this bar's provably-observed high-water
            // (KTD5) and only when the position is still open (an exit branch above
            // already rolled it to Done). Once the observed MFE reaches
            // `breakeven_trigger_r · R` the ratchet ARMS: the stop is raised for
            // SUBSEQUENT bars to at least the entry price (the flat breakeven, lever 6),
            // and — when the trail is on — up to `high_water − round(trail_frac_r · R)`,
            // floored at entry, so a runner that peaks well past the trigger then reverts
            // books a partial win at the trailed stop rather than a scratch at breakeven.
            //
            // The new stop is deliberately NOT applied to this bar's own stop check
            // above: the low that would hit it may have printed before the high that just
            // raised the high-water (and thus the trail) — same-bar order is unknowable
            // (KTD2), so booking it would be a stop the position never provably reached.
            // It binds only from the next bar. The stop only ever tightens
            // (`stop_price.max(...)`) and, once armed, never loosens below entry. Off
            // (`breakeven_trigger_r == 0.0`) the ratchet never arms; with the ratchet on
            // but the trail off (`trail_frac_r == 0.0`) the new stop is exactly the entry
            // price — byte-identical to v23's flat breakeven move.
            if self.phase == Phase::Long {
                if let Some(trigger) = self.breakeven_trigger_price(params) {
                    if self.high_water >= trigger {
                        // Armed: the breakeven floor is the entry price (lever 6).
                        let mut new_stop = self.entry_price;
                        // Trail arm (candidate A): give back a fixed fraction of R below
                        // the high-water mark. `trail_frac_r == 0.0` (off) skips this so
                        // the stop stays flat at entry (the trail term would otherwise be
                        // `high_water` itself — too tight). A give-back that rounds to 0
                        // (a tiny positive `trail_frac_r`) would also collapse the trail
                        // onto `high_water`; treat it as no trail (flat breakeven) rather
                        // than an accidental peak-tight stop.
                        if params.trail_frac_r > 0.0 {
                            let give_back = (params.trail_frac_r * self.r_denom as f64).round() as i64;
                            if give_back > 0 {
                                new_stop = new_stop.max(self.high_water - give_back);
                            }
                        }
                        // Only ever tighten — never loosen a stop already at/above the
                        // new level (the trail rises monotonically with high_water, but
                        // this guards the general case too).
                        self.stop_price = self.stop_price.max(new_stop);
                    }
                }
            }
        }
        acts
    }

    /// The per-session gates evaluated once at range fix (KTD7): the first failing
    /// gate returns a [`OrbAction::SessionReject`] naming its single canonical
    /// filter and ends the day. `None` when every active gate passes (the all-off
    /// default). Order is pinned — ATR availability, then OR-width, then RVOL, then
    /// gap retention (#168, deterministically last) — so a session failing more than
    /// one gate records only the first (KTD7). A gate whose
    /// input is REQUIRED and missing fails closed (never a silent pass): the ATR-stop
    /// arm and the RVOL-history arm. The OR-width arm is the deliberate exception — its
    /// ATR normalizer is optional, so a no-ATR session is skipped, not rejected (see
    /// the arm's comment).
    fn session_gate_reject(&self, params: &OrbParams) -> Option<OrbAction> {
        // 1. ATR-mode stop needs a *positive* prior ATR (KTD5, AE5). A missing ATR
        //    OR a non-positive one (flat / halted priors dedup to `Some(0.0)`, the
        //    gappier small-cap tiers this strategy reaches) fails closed — never a
        //    silent range-low fallback (that would mix stop modes in one run,
        //    breaking R8) and never an ATR distance that rounds to 0 and collapses
        //    the stop onto the entry (a fabricated same-bar stop-out).
        if params.stop_placement() == StopMode::Atr && self.prior_atr.filter(|a| *a > 0.0).is_none() {
            return Some(session_reject("atr_unavailable", vec![("atr_window", params.atr_window)]));
        }
        // 2. OR-width sanity gate (lever 3, KTD7), DECOUPLED from ATR availability
        //    (code turn): reject when range-R exceeds `or_width_max_atr · ATR`. Unlike
        //    the ATR-STOP arm above — where a stop *needs* its ATR, so a missing one
        //    must fail closed — the width gate is genuinely OPTIONAL for a session
        //    that lacks a positive prior ATR: with nothing to normalize against, the
        //    session is simply not width-gated (SKIP, not reject). This is deliberate:
        //    coupling the width test to ATR availability conflated "too-wide opening
        //    range" with "no ATR history" and swamped the clean width signal with a
        //    winner-rich coverage cull (lever 3 / v18 was reverted for exactly this).
        //    The KTD7 "missing input never silently passes a REQUIRED gate" invariant
        //    is preserved — here the gate is not required when its input is absent.
        if params.or_width_max_atr > 0.0 {
            if let Some(atr) = self.prior_atr.filter(|a| *a > 0.0) {
                let range_r = (self.range_high - self.range_low) as f64;
                let max_width = params.or_width_max_atr * atr;
                if range_r > max_width {
                    return Some(session_reject(
                        "or_width_atr",
                        vec![("range_r", range_r), ("atr", atr), ("or_width_max_atr", params.or_width_max_atr)],
                    ));
                }
            }
        }
        // 3. Opening-window RVOL gate (lever 5, KTD7/KTD9): reject when today's
        //    opening-window volume is below `rvol_min · prior mean`. A missing or
        //    non-positive prior mean fails closed with the insufficient-history
        //    filter (short history / zero-mean guard, KTD7).
        if params.rvol_min > 0.0 {
            let Some(mean) = self.prior_open_vol_mean.filter(|m| *m > 0.0) else {
                return Some(session_reject(
                    "rvol_insufficient_history",
                    vec![("rvol_min_history", params.rvol_min_history)],
                ));
            };
            if self.open_window_vol < params.rvol_min * mean {
                return Some(session_reject(
                    "rvol_min",
                    vec![
                        ("open_window_vol", self.open_window_vol),
                        ("prior_open_vol_mean", mean),
                        ("rvol_min", params.rvol_min),
                    ],
                ));
            }
        }
        // 4. Opening-range gap-retention gate (#165/#168, KTD6): the FINAL arm,
        //    evaluated exactly once here on the frozen opening range. Armed only at
        //    the frozen 0.50 cutoff — the reserved 1.0 sentinel returns before any
        //    retention input is read — and every failure class fails closed under
        //    its own #165 filter.
        self.gap_retention_reject(params)
    }

    /// The armed gap-retention rejection for this session (#165/#168), or `None`
    /// when OFF or the measured retention passes the cutoff (equality passes). The
    /// OFF bypass runs before any retention input is read (KTD6), so the `1.0`
    /// sentinel leaves the head-v30 decision stream untouched. The gate reads the
    /// frozen `range_low` — only ever written inside `[range_open, range_end)`, so
    /// a post-range low can never alter the observation (R2) — and never
    /// `session_low`. Envelope values per KTD5: the cutoff plus every canonical
    /// component that exists; a missing component is an omitted key, never a
    /// numeric sentinel, and a non-finite retention is never inserted.
    fn gap_retention_reject(&self, params: &OrbParams) -> Option<OrbAction> {
        if !params.gap_retention_active() {
            return None;
        }
        let cutoff = params.gap_retention_min;
        let class = classify_gap_retention(self.gap_prices, self.range_low);
        let filter = match class {
            GapRetention::Measured(retention) => {
                // `partial_cmp` fails closed (KTD3): an incomparable retention
                // never passes. Equality passes — 0.50 retention is retained.
                if retention.partial_cmp(&cutoff).is_some_and(|o| o.is_ge()) {
                    return None;
                }
                "gap_retention_min"
            }
            GapRetention::NotApplicable => "gap_retention_not_applicable",
            GapRetention::Unavailable => "gap_retention_unavailable",
            GapRetention::Invalid => "gap_retention_invalid",
        };
        let mut values = vec![("gap_retention_min", cutoff)];
        if let GapRetention::Measured(retention) = class {
            values.push(("retention", retention));
        }
        if self.gap_prices.prior_close > 0 {
            values.push(("prior_close", self.gap_prices.prior_close as f64));
        }
        if self.gap_prices.today_open > 0 {
            values.push(("today_open", self.gap_prices.today_open as f64));
        }
        if self.saw_range && self.range_low > 0 {
            values.push(("range_low", self.range_low as f64));
        }
        Some(session_reject(filter, values))
    }

    /// The stop price fixed at entry for `stop_mode` (KTD1/KTD4/KTD5): range low
    /// (v9), the rounded OR midpoint, or `entry − round(stop_atr_mult · ATR)`
    /// clamped never wider than the range low (ATR only ever narrows the stop).
    /// A *positive* ATR is guaranteed by [`OrbState::session_gate_reject`], so the
    /// ATR arm's `unwrap_or` is an unreachable fail-safe; the distance is floored
    /// at 1 so a tiny `mult · ATR` can never round to 0 and collapse the stop onto
    /// the entry (which would zero trade-R and force a same-bar stop-out).
    fn stop_for_entry(&self, entry_price: i64, params: &OrbParams) -> i64 {
        match params.stop_placement() {
            StopMode::RangeLow => self.range_low,
            StopMode::OrMidpoint => {
                ((self.range_high + self.range_low) as f64 / 2.0).round() as i64
            }
            StopMode::Atr => {
                let atr = self.prior_atr.unwrap_or(0.0);
                let dist = ((params.stop_atr_mult * atr).round() as i64).max(1);
                (entry_price - dist).max(self.range_low)
            }
        }
    }

    /// The R-denominator fixed at entry for target + MFE (KTD4): range-R at mode 0
    /// (v9-verbatim so the R3 reconcile holds), trade-R (`entry − stop`) otherwise.
    fn entry_r_denom(&self, entry_price: i64, stop_price: i64, params: &OrbParams) -> i64 {
        match params.stop_placement() {
            StopMode::RangeLow => self.range_high - self.range_low,
            StopMode::OrMidpoint | StopMode::Atr => entry_price - stop_price,
        }
    }

    /// The fixed profit-target price `entry_price + round(profit_target_r · R)`
    /// (R1, KTD1/KTD4), where `R` is the entry-fixed `r_denom` (range-R at mode 0,
    /// trade-R otherwise). `None` when `R ≤ 0` or the target size is non-positive —
    /// either way the entry bar can never trip the target (a `profit_target_r ≤ 0`
    /// from a hand-seeded manifest must not fire an immediate breakeven exit).
    fn target_price(&self, params: &OrbParams) -> Option<i64> {
        if self.r_denom <= 0 || params.profit_target_r <= 0.0 {
            return None;
        }
        Some(self.entry_price + (params.profit_target_r * self.r_denom as f64).round() as i64)
    }

    /// The breakeven-move trigger price (lever 6, KTD11): the high-water level
    /// `entry_price + round(breakeven_trigger_r · R)` at which the stop ratchets up to
    /// the entry price, where `R` is the entry-fixed `r_denom` (range-R at mode 0,
    /// trade-R otherwise). `None` when the lever is off (`breakeven_trigger_r ≤ 0.0`)
    /// or `R ≤ 0` — either way no ratchet ever arms, so the exit path is unchanged.
    fn breakeven_trigger_price(&self, params: &OrbParams) -> Option<i64> {
        if params.breakeven_trigger_r <= 0.0 || self.r_denom <= 0 {
            return None;
        }
        // A tiny positive trigger whose rounded offset is 0 would place the trigger AT
        // the entry price — and `high_water` seeds at entry and only rises, so it would
        // arm on the FIRST held bar (an instant breakeven), the exact degenerate the
        // `validate()` negative guard rejects. Require the effective (rounded) trigger
        // to sit strictly ABOVE entry, so a round-to-zero offset is treated as off.
        let offset = (params.breakeven_trigger_r * self.r_denom as f64).round() as i64;
        (offset > 0).then_some(self.entry_price + offset)
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

/// One symbol's live market view, published by the strategy as it processes bars and read
/// by the live max-loss breaker's open-position mark (live-session-driver KTD8(b)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymbolMark {
    /// The last streamed bar close (integer KRW).
    pub last_close: i64,
    /// When that bar was observed (unix seconds) — the staleness clock. A mark older
    /// than the feed's tolerance must NOT be used: a market-data gap or symbol halt
    /// accompanies exactly the fast adverse moves the breaker must catch, so a
    /// stale-favorable price would under-report the loss precisely when it matters.
    pub last_bar_unix: i64,
    /// The open leg's current stop level, when the symbol is long. This is the
    /// conservative floor the mark falls back to when the feed is stale or absent.
    pub stop_price: Option<i64>,
}

/// A shared, cloneable per-symbol market view (KTD8(b)). The nautilus engine consumes the
/// strategy, so — exactly like [`DecisionSink`] and [`EmissionGate`] — the runner holds a
/// clone and reads it from the watchdog thread, which has no market-data access of its
/// own. Keyed by bare shcode, matching the fill ledger's symbol key.
#[derive(Debug, Clone, Default)]
pub struct MarkFeed(Arc<Mutex<HashMap<String, SymbolMark>>>);

impl MarkFeed {
    /// A fresh, empty feed.
    pub fn new() -> Self {
        MarkFeed::default()
    }

    /// Publish this symbol's latest observation (last write wins).
    pub fn observe(&self, symbol: &str, mark: SymbolMark) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(symbol.to_string(), mark);
    }

    /// This symbol's latest observation, if the strategy has seen a bar for it.
    pub fn get(&self, symbol: &str) -> Option<SymbolMark> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).get(symbol).copied()
    }

    /// A snapshot of every observed symbol.
    pub fn snapshot(&self) -> HashMap<String, SymbolMark> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// A shared, cloneable ledger of the entry-fixed risk captured at each order
/// placement (R4/U1). The nautilus engine owns the strategy (and consumes it), so —
/// exactly like [`DecisionSink`] — the runner holds a clone and reads it after the
/// engine run to join `risk_capital`/`realized_r` into the trade ledger. Keyed by
/// [`InstrumentId`]: ORB holds at most one open leg per symbol per session, so a
/// per-session symbol key is unambiguous (the runner joins per session).
#[derive(Debug, Clone, Default)]
pub struct EntryRiskLedger(Arc<Mutex<HashMap<InstrumentId, EntryRisk>>>);

impl EntryRiskLedger {
    /// A fresh, empty ledger.
    pub fn new() -> Self {
        EntryRiskLedger::default()
    }

    /// Record the entry-fixed risk for `id` (last write wins — one entry per symbol
    /// per session).
    pub fn record(&self, id: InstrumentId, risk: EntryRisk) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(id, risk);
    }

    /// A snapshot of the captured entry risks (the runner joins this to the session's
    /// positions by instrument id).
    pub fn snapshot(&self) -> HashMap<InstrumentId, EntryRisk> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// A selected symbol and the bar series the strategy trades it on.
#[derive(Debug, Clone)]
pub struct SelectedSymbol {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument_id: InstrumentId,
    /// The bar type to subscribe (typically the 1-minute series).
    pub bar_type: BarType,
    /// Canonical integer prices defining this symbol-session's opening gap.
    pub gap_prices: SessionGapPrices,
    /// The symbol's prior-daily ATR for this session (KTD5), threaded to its
    /// [`OrbState`] for the stop / OR-width gates. `None` when unavailable.
    pub prior_atr: Option<f64>,
    /// The symbol's prior opening-window volume mean for this session (KTD9),
    /// threaded to its [`OrbState`] for the RVOL gate. `None` when below history.
    pub prior_open_vol_mean: Option<f64>,
    /// The symbol's prior-session Amihud illiquidity for this session (plan
    /// 2026-07-16-003), threaded to its [`OrbState`] for the liquidity budget tilt.
    /// `None` when under-covered.
    pub prior_illiq: Option<f64>,
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
    entry_risk: EntryRiskLedger,
    /// The session-open realized-equity multiplier (CLASS B lever 2, R2/R8/KTD-1):
    /// construction-time state supplied by the runner from prior sessions' realized
    /// P&L. `1.0` when the compounding lever is off or on the first session (no prior
    /// P&L) — the value that makes off-sentinel sizing byte-identical to v26. The
    /// strategy holds NO account state (R9): this scalar is all it needs to size.
    session_equity_multiplier: f64,
    /// The capital-ladder rung fraction (production-ladder KTD6): a runner-supplied,
    /// dimensionless budget-numerator multiplier for the authorized rung, composed with
    /// the equity factor and the ratio-ATR tilt (`risk_per_trade_krw × rung_fraction ×
    /// equity factor × tilt`). `1.0` (the default) leaves sizing byte-identical to v30.
    /// It is NEVER an `OrbParams`/manifest field, so a rung move produces zero head-
    /// identity diff — the exactly-one-param compare discipline stays intact.
    rung_fraction: f64,
    /// The live dead-man feeders (live-session-driver R5/KTD5). `None` in a backtest —
    /// the runtime heartbeat only means something when a watchdog is watching it. Touched
    /// on every processed bar, so the dead-man reflects real strategy progress, not just
    /// a live tokio task that has not yet died.
    heartbeats: Option<crate::runner::watchdog::Heartbeats>,
    /// The live per-symbol mark feed (KTD8(b)). `None` in a backtest.
    mark_feed: Option<MarkFeed>,
}

impl OrbStrategy {
    /// Build the strategy for a resolved universe + parameter set, writing its
    /// per-decision telemetry envelopes into `decisions`. `session_equity_multiplier`
    /// is the CLASS B lever 2 session-open scalar (R2/KTD-1): the runner computes it
    /// from prior sessions' realized equity and passes `1.0` when the lever is off, so
    /// an off-sentinel run sizes exactly as v26.
    pub fn new(
        params: OrbParams,
        selected: Vec<SelectedSymbol>,
        decisions: DecisionSink,
        session_equity_multiplier: f64,
    ) -> Self {
        let base = StrategyConfig {
            strategy_id: Some(StrategyId::from(strategy_id_str(&params).as_str())),
            ..Default::default()
        };
        // Thread each selected symbol's prior-daily ATR + opening-window volume
        // mean (U2 candidate seam) onto its fresh state for the gates to read.
        let states = selected
            .iter()
            .map(|s| {
                (
                    s.instrument_id,
                    OrbState::with_session_inputs(
                        s.gap_prices,
                        s.prior_atr,
                        s.prior_open_vol_mean,
                        s.prior_illiq,
                    ),
                )
            })
            .collect();
        OrbStrategy {
            core: StrategyCore::new(base),
            params,
            selected,
            states,
            entered_qty: HashMap::new(),
            decisions,
            emission: EmissionGate::open(),
            entry_risk: EntryRiskLedger::new(),
            session_equity_multiplier,
            rung_fraction: 1.0,
            heartbeats: None,
            mark_feed: None,
        }
    }

    /// Thread the live dead-man feeders + mark feed in (live-session-driver U3/U4). The
    /// runner calls this only on the live path; a backtest leaves both `None`, so bar
    /// processing is byte-identical to today.
    pub fn with_heartbeats(mut self, heartbeats: crate::runner::watchdog::Heartbeats) -> Self {
        self.heartbeats = Some(heartbeats);
        self
    }

    /// Thread the live per-symbol mark feed in (KTD8(b)) — the breaker's price source.
    pub fn with_mark_feed(mut self, feed: MarkFeed) -> Self {
        self.mark_feed = Some(feed);
        self
    }

    /// Set the capital-ladder rung fraction (production-ladder KTD6) — the runner supplies
    /// the authorized rung's pre-registered fraction here; it scales the risk budget
    /// numerator only. Defaults to `1.0` (unscaled), so a caller that never sets it sizes
    /// exactly as v30.
    pub fn with_rung_fraction(mut self, rung_fraction: f64) -> Self {
        self.rung_fraction = rung_fraction;
        self
    }

    /// A clone of the emission gate — the live runner closes it at teardown so a
    /// late signal places no order (KTD7).
    pub fn emission_gate(&self) -> EmissionGate {
        self.emission.clone()
    }

    /// A clone of the entry-risk ledger (U1) — the runner reads it after the engine
    /// run to join per-trade `risk_capital`/`realized_r` into the ledger.
    pub fn entry_risk_ledger(&self) -> EntryRiskLedger {
        self.entry_risk.clone()
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

    /// The entry quantity for `id` breaking out at `limit_price`, under the full sizing
    /// stack: the R5 fixed-KRW risk budget, the CLASS B lever-2 session-open equity
    /// multiplier, and the plan-2026-07-15-002 ratio-ATR budget tilt. All three inputs are
    /// read here so the wiring is unit-testable (KTD-4):
    ///
    /// - `risk_per_share = entry − stop` from this symbol's state (the transition set the
    ///   entry-fixed stop before returning the `Enter` action); a non-positive value falls
    ///   back to notional sizing inside the params helper.
    /// - the session-open equity multiplier (`1.0` when the compounding lever is off).
    /// - the ratio-ATR weight `w`, computed inline from the symbol's threaded `prior_atr`
    ///   and `limit_price`: `v = prior_atr / limit_price` is a *relative* volatility, so the
    ///   tilt enters the budget numerator only and cannot collapse to the dead absolute-ATR
    ///   lever. An absent / `Some(0.0)` `prior_atr` or a non-positive price fails closed to
    ///   `w = 1.0` (skip-not-reject).
    ///
    /// With every lever off-sentinel this is byte-identical to v26. `risk_per_share` and
    /// the session-open equity `multiplier` are passed in (the caller also emits them as
    /// sizing telemetry); the ratio-ATR weight is read here from the symbol's threaded
    /// `prior_atr` so the tilt wiring is unit-testable.
    fn entry_qty(
        &self,
        id: &InstrumentId,
        limit_price: i64,
        risk_per_share: f64,
        multiplier: f64,
    ) -> i64 {
        let prior_atr = self.states.get(id).and_then(|s| s.prior_atr);
        let prior_illiq = self.states.get(id).and_then(|s| s.prior_illiq);
        // The ratio-ATR tilt, the Amihud liquidity tilt, and the ladder rung fraction are
        // all dimensionless, numerator-only multiplicands (the anti-collapse invariant):
        // composing them into one `weight` applies `budget × ratio-tilt × liquidity-tilt ×
        // rung_fraction` without touching the `risk_per_share` denominator or the notional
        // ceiling (production-ladder KTD6, plan 2026-07-16-003). With every tilt off-sentinel
        // and rung_fraction 1.0 this is byte-identical to v30.
        let weight = self.params.ratio_atr_weight(prior_atr, limit_price as f64)
            * self.params.liquidity_tilt_weight(prior_illiq)
            * self.rung_fraction;
        self.params.position_qty_risked_tilted(
            limit_price as f64,
            risk_per_share,
            multiplier,
            weight,
        )
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
                    // Risk-based sizing (R5): the transition already set the entry-fixed
                    // stop on this symbol's state (it is read later by `mfe_r`), so
                    // `risk_per_share = entry − stop` is available now. Off-sentinel →
                    // notional sizing (byte-identical to v23); a degenerate stop falls
                    // back to notional inside `position_qty_risked`.
                    let risk_per_share =
                        self.states.get(&id).map(|s| s.risk_per_share() as f64).unwrap_or(0.0);
                    // Equity-compounding lever (CLASS B lever 2, R8/KTD-1/KTD-2): the
                    // session-open realized-equity multiplier scales the risk budget.
                    let m = self.session_equity_multiplier;
                    // Full sizing (R5 budget × equity multiplier × plan 2026-07-15-002
                    // ratio-ATR tilt): `entry_qty` folds in the tilt weight, computed from
                    // this symbol's threaded prior-daily ATR + the limit price. Off-sentinel
                    // (or absent/`Some(0.0)` ATR) → weight 1.0 → byte-identical to v26. The
                    // notional ceiling and `risk_per_share` denominator are untouched; a
                    // clamped-to-zero budget flows into the qty ≤ 0 rejection below.
                    let qty = self.entry_qty(&id, limit_price, risk_per_share, m);
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
                            tags: None,
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
                            tags: None,
                        });
                        continue;
                    }
                    self.entered_qty.insert(id, qty);
                    // Capture the entry-fixed risk for the ledger join (U1):
                    // risk_capital = qty · risk_per_share. `risk_per_share ≤ 0`
                    // (degenerate) records a zero that `joined_risk` maps to `None`.
                    self.entry_risk.record(id, EntryRisk { risk_per_share, qty: qty as f64 });
                    self.place(id, OrderSide::Buy, limit_price, qty, false)?;
                    // The OrderPlaced envelope carries the sizing basis (R5) so a
                    // post-run bind check can read whether the qty distribution shifted
                    // (tight-stop entries sized up, wide-stop down) without re-joining.
                    // The effective (compounded) risk budget the qty was sized on
                    // (CLASS B lever 2, R14/KTD-5): `risk_per_trade_krw ·
                    // equity_compound_factor(m)`. Rides the envelope alongside the
                    // session `equity_multiplier` so the post-run bind check reads the
                    // per-session budget path + the qty-distribution shift directly from
                    // decisions.jsonl (no re-join). Off-sentinel: factor 1.0, m 1.0.
                    let effective_risk_budget_krw =
                        self.params.risk_per_trade_krw * self.params.equity_compound_factor(m);
                    self.emit_market_data(id, ts, DecisionDetail::transition(
                        symbol.clone(),
                        SignalKind::OrderPlaced,
                        vals(&[
                            ("qty", qty as f64),
                            ("price", limit_price as f64),
                            ("risk_per_share", risk_per_share),
                            ("risk_per_trade_krw", self.params.risk_per_trade_krw),
                            ("equity_multiplier", m),
                            ("effective_risk_budget_krw", effective_risk_budget_krw),
                        ]),
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
                    // exit-tuning turn reads give-back directly; realized-R (KTD12)
                    // rides alongside it so the breakeven-trail bind check reads the
                    // give-back cohort's *booked* R (scratch ≈0 vs trailed partial)
                    // without re-joining entry price across envelopes.
                    let st = self.states.get(&id);
                    let mfe_r = st.map(|s| s.mfe_r()).unwrap_or(0.0);
                    let realized_r = st.map(|s| s.realized_exit_r(limit_price)).unwrap_or(0.0);
                    self.emit_market_data(id, ts, DecisionDetail::transition(
                        symbol.clone(),
                        kind,
                        vals(&[
                            ("qty", qty as f64),
                            ("price", limit_price as f64),
                            ("mfe_r", mfe_r),
                            ("realized_r", realized_r),
                        ]),
                    ));
                    self.entered_qty.remove(&id);
                }
                OrbAction::SessionReject { filter, values } => {
                    // A per-session gate rejected the symbol done-for-day before any
                    // breakout (KTD7): no order, one recorded rejection envelope
                    // naming the canonical filter. The state already rolled to Done.
                    self.emit_market_data(id, ts, DecisionDetail {
                        kind: SignalKind::OrderRejectedSizing,
                        symbol: symbol.clone(),
                        decision: None,
                        filter: Some(filter.to_string()),
                        values: values.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                        tags: None,
                    });
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
        let close = bar.close.as_f64() as i64;
        let volume = bar.volume.as_f64();
        let params = self.params.clone();
        let actions =
            self.states.get_mut(&id).expect("state present").on_bar(t, high, low, close, volume, &params);

        // Live feeders (live-session-driver U3/U4) — updated on EVERY processed bar,
        // including the (common) no-action one, and AFTER the state transition so the
        // published stop reflects any breakeven move this bar made. Both are `None` in a
        // backtest, so the backtest path is unchanged.
        let bar_unix = (bar.ts_event.as_u64() / 1_000_000_000) as i64;
        if let Some(hb) = &self.heartbeats {
            // The runtime dead-man measures real strategy progress, not task liveness.
            hb.touch_runtime(bar_unix);
        }
        if let Some(feed) = &self.mark_feed {
            let st = self.states.get(&id).expect("state present");
            feed.observe(
                id.symbol.as_str(),
                SymbolMark {
                    last_close: close,
                    last_bar_unix: bar_unix,
                    stop_price: st.is_long().then(|| st.stop_price()),
                },
            );
        }

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

#[cfg(test)]
mod gap_retention_classifier_tests {
    //! U2 (#168) — the pure gap-retention classifier: total over every `i64` input
    //! pair, ordered per KTD3 (applicability → availability → divide → validity),
    //! with the frozen #165 value semantics (signed, 0.0 = prior-close touch,
    //! 1.0 = full retention).
    use super::*;

    fn classify(prior_close: i64, today_open: i64, range_low: i64) -> GapRetention {
        classify_gap_retention(SessionGapPrices::new(prior_close, today_open), range_low)
    }

    #[test]
    fn boundary_half_retention_is_exact_from_system_produced_prices() {
        // Canonical KRW/tick prices where the numerator is exactly half the
        // denominator: (61_500 − 60_000) · 2 == 63_000 − 60_000. Both differences
        // are exact in f64, so the quotient is exactly 0.5 — equality at the
        // frozen cutoff passes at the gate.
        assert_eq!(classify(60_000, 63_000, 61_500), GapRetention::Measured(0.5));
        // One tick lower measures strictly below the cutoff.
        match classify(60_000, 63_000, 61_499) {
            GapRetention::Measured(r) => assert!(r < 0.5, "one tick below → {r} < 0.5"),
            other => panic!("expected measured, got {other:?}"),
        }
    }

    #[test]
    fn value_semantics_are_signed_with_exact_anchors() {
        // Full retention: the range never dipped below today's open.
        assert_eq!(classify(60_000, 63_000, 63_000), GapRetention::Measured(1.0));
        // Prior-close touch is exactly zero.
        assert_eq!(classify(60_000, 63_000, 60_000), GapRetention::Measured(0.0));
        // A crossing below the prior close stays signed (negative), never clamped.
        assert_eq!(classify(60_000, 63_000, 58_500), GapRetention::Measured(-0.5));
    }

    #[test]
    fn zero_width_range_remains_a_valid_observation() {
        // Validity is not redefined (#165): a zero-width range's low classifies
        // measured like any other frozen low.
        assert_eq!(classify(60_000, 63_000, 62_250), GapRetention::Measured(0.75));
    }

    #[test]
    fn non_positive_gap_or_prior_close_is_not_applicable_before_any_division() {
        // The unthreaded default pair.
        assert_eq!(classify(0, 0, 61_500), GapRetention::NotApplicable);
        // Non-positive prior close.
        assert_eq!(classify(0, 63_000, 61_500), GapRetention::NotApplicable);
        assert_eq!(classify(-100, 63_000, 61_500), GapRetention::NotApplicable);
        // Zero gap (the would-be 0/0) and a gap-down — classified before the
        // divide, so neither can masquerade as invalid.
        assert_eq!(classify(63_000, 63_000, 61_500), GapRetention::NotApplicable);
        assert_eq!(classify(63_000, 60_000, 61_500), GapRetention::NotApplicable);
        // Applicability outranks availability: a sentinel low on a non-applicable
        // session is still not-applicable (KTD3 ordering).
        assert_eq!(classify(63_000, 60_000, i64::MAX), GapRetention::NotApplicable);
    }

    #[test]
    fn missing_or_non_positive_range_low_is_unavailable() {
        assert_eq!(classify(60_000, 63_000, i64::MAX), GapRetention::Unavailable);
        assert_eq!(classify(60_000, 63_000, 0), GapRetention::Unavailable);
        assert_eq!(classify(60_000, 63_000, -1), GapRetention::Unavailable);
    }

    #[test]
    fn retention_above_one_is_invalid() {
        // range_low above today's open is inconsistent data, never "better than
        // full retention".
        assert_eq!(classify(60_000, 63_000, 63_001), GapRetention::Invalid);
        // Non-finite ratios are defensively invalid but unreachable from valid
        // i64 inputs: the not-applicable arm guarantees a positive denominator
        // and any finite i64 numerator divides to a finite f64. The largest
        // admissible numerator still classifies (here invalid, being above 1.0).
        assert_eq!(classify(1, 2, i64::MAX - 1), GapRetention::Invalid);
    }
}

#[cfg(test)]
mod ratio_atr_wiring_tests {
    //! Strategy-level coverage for the ratio-ATR budget tilt wiring at the Enter handler
    //! (plan 2026-07-15-002 U3): `entry_qty` reads the symbol's threaded `prior_atr` + the
    //! entry-fixed stop from state and composes the R5 budget × equity multiplier × tilt.
    use super::*;
    use nautilus_ls::ingest::BarKind;

    fn tm(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    /// The frozen pre-registered ratio-ATR values (armed), with a slack notional ceiling
    /// so the risk budget binds (isolating the tilt) unless a test overrides it.
    fn armed_params() -> OrbParams {
        OrbParams {
            risk_per_trade_krw: 299_340.0,
            notional_per_position: 1_000_000_000.0,
            ratio_atr_alpha: 1.0,
            ratio_atr_ref: 0.073_157_64,
            ratio_atr_w_lo: 0.702_697_55,
            ratio_atr_w_hi: 1.445_489_56,
            ..Default::default()
        }
    }

    /// Build a one-symbol strategy with `prior_atr` threaded onto the symbol, drive its
    /// state through the opening range into a clean 62_000 breakout (range-low stop 60_000
    /// → risk_per_share 2_000), and return it ready for `entry_qty(&id, 62_000)`.
    fn strategy_at_entry(
        params: OrbParams,
        prior_atr: Option<f64>,
        m: f64,
    ) -> (OrbStrategy, InstrumentId) {
        let id = InstrumentId::from("005930.XKRX");
        let selected = vec![SelectedSymbol {
            instrument_id: id,
            bar_type: BarKind::Minute(1).bar_type(id).unwrap(),
            gap_prices: SessionGapPrices::new(60_000, 63_000),
            prior_atr,
            prior_open_vol_mean: None,
            prior_illiq: None,
        }];
        let mut strategy = OrbStrategy::new(params, selected, DecisionSink::new(), m);
        let p = strategy.params.clone();
        {
            let st = strategy.states.get_mut(&id).unwrap();
            assert!(st.on_bar(tm(9, 0), 61_500, 60_000, 61_500, 0.0, &p).is_empty());
            assert!(st.on_bar(tm(9, 10), 61_500, 60_000, 61_500, 0.0, &p).is_empty());
            let acts = st.on_bar(tm(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
            assert_eq!(acts, vec![OrbAction::Enter { limit_price: 62_000 }]);
            assert_eq!(st.risk_per_share(), 2_000, "range-low stop → rps 2_000");
        }
        (strategy, id)
    }

    #[test]
    fn entry_qty_off_sentinel_is_byte_identical_to_v26() {
        // Covers AE1: with the tilt off, entry_qty matches the untilted risk-sizing path for
        // ANY prior_atr (present, absent, or the Some(0.0) trap) across a spread of equity
        // multipliers — the sentinel decouples the wiring entirely.
        let mut off = armed_params();
        off.ratio_atr_alpha = 0.0;
        for prior_atr in [None, Some(6_000.0), Some(0.0)] {
            for m in [0.95, 1.0, 1.05] {
                let (s, id) = strategy_at_entry(off.clone(), prior_atr, m);
                assert_eq!(
                    s.entry_qty(&id, 62_000, 2_000.0, m),
                    off.position_qty_risked_at(62_000.0, 2_000.0, m),
                    "off sentinel (prior_atr={prior_atr:?}, m={m}) == untilted"
                );
            }
        }
    }

    #[test]
    fn entry_qty_no_prior_atr_sizes_untilted_with_lever_on() {
        // Covers AE2: a symbol with no prior_atr sizes untilted even with the lever armed
        // (skip-not-reject) — the trade is not dropped, just not tilted.
        let p = armed_params();
        let (s, id) = strategy_at_entry(p.clone(), None, 1.0);
        assert_eq!(
            s.entry_qty(&id, 62_000, 2_000.0, 1.0),
            p.position_qty_risked_at(62_000.0, 2_000.0, 1.0),
            "no-ATR trade sizes untilted"
        );
    }

    #[test]
    fn entry_qty_zero_prior_atr_fails_closed_untilted() {
        // KTD-5 at the strategy level: Some(0.0) (flat deduped dailies) fails closed to the
        // neutral weight, not v = 0 → w = ∞.
        let p = armed_params();
        let (s, id) = strategy_at_entry(p.clone(), Some(0.0), 1.0);
        assert_eq!(
            s.entry_qty(&id, 62_000, 2_000.0, 1.0),
            p.position_qty_risked_at(62_000.0, 2_000.0, 1.0),
            "Some(0.0) prior_atr sizes untilted"
        );
    }

    #[test]
    fn entry_qty_downsizes_high_vol_relative_to_low_vol() {
        // Covers AE4 / F1: two entries with identical stop distance but v at the untreated
        // p90 (high vol) vs p10 (low vol). prior_atr = v · limit_price. The high-v trade is
        // downweighted below, the low-v above, the untilted qty — strictly.
        let p = armed_params();
        let (hi, id_h) = strategy_at_entry(p.clone(), Some(0.104_109_72 * 62_000.0), 1.0);
        let (lo, id_l) = strategy_at_entry(p.clone(), Some(0.050_610_98 * 62_000.0), 1.0);
        let q_high = hi.entry_qty(&id_h, 62_000, 2_000.0, 1.0);
        let q_low = lo.entry_qty(&id_l, 62_000, 2_000.0, 1.0);
        let untilted = p.position_qty_risked_at(62_000.0, 2_000.0, 1.0);
        assert!(q_high < q_low, "high-v qty {q_high} strictly below low-v qty {q_low}");
        assert!(q_high < untilted, "high-v {q_high} downsized below untilted {untilted}");
        assert!(q_low > untilted, "low-v {q_low} upsized above untilted {untilted}");
    }

    #[test]
    fn entry_qty_tilt_still_capped_by_notional_ceiling() {
        // An upweighted (deep low-v → w_hi) entry is still bounded by the notional ceiling:
        // the min(risk_qty, floor(notional / price)) cap binds exactly as before the tilt.
        let p = OrbParams { notional_per_position: 10_000_000.0, ..armed_params() };
        let (s, id) = strategy_at_entry(p.clone(), Some(0.02 * 62_000.0), 1.0);
        assert_eq!(
            s.entry_qty(&id, 62_000, 2_000.0, 1.0),
            p.position_qty(62_000.0),
            "notional ceiling binds even when upweighted"
        );
    }

    #[test]
    fn rung_fraction_scales_the_risk_budget_numerator_with_zero_param_diff() {
        // Production-ladder KTD6/AE(U10): the ladder rung fraction is a numerator-only
        // multiplier — half the fraction sizes at half the risked qty (budget binds),
        // byte-identical to v30 at 1.0 — and a rung change never touches OrbParams, so the
        // manifest/head identity is unchanged.
        let (full, id_a) = strategy_at_entry(armed_params(), None, 1.0);
        let (half, id_b) = strategy_at_entry(armed_params(), None, 1.0);
        let half = half.with_rung_fraction(0.5);
        let q_full = full.entry_qty(&id_a, 62_000, 2_000.0, 1.0);
        let q_half = half.entry_qty(&id_b, 62_000, 2_000.0, 1.0);
        assert!(q_full > 0, "risk budget binds at rung_fraction 1.0");
        assert_eq!(q_half, q_full / 2, "rung_fraction 0.5 → half the risked qty (numerator scaled)");
        assert_eq!(full.params, half.params, "the rung fraction never touches the parameter set");
    }

    /// The liquidity tilt threads `prior_illiq` onto the symbol's state, and `entry_qty`
    /// applies `liquidity_tilt_weight` in the budget numerator (plan 2026-07-16-003): an
    /// illiquid name (illiq above the reference → w < 1) sizes strictly smaller than the
    /// untilted path, and the off-sentinel is byte-identical for any illiq.
    #[test]
    fn liquidity_tilt_wiring_downsizes_illiquid_and_off_is_byte_identical() {
        let ref_illiq = 2.0e-13;
        let armed = OrbParams {
            risk_per_trade_krw: 299_340.0,
            notional_per_position: 1_000_000_000.0, // slack ceiling so the budget binds
            liquidity_tilt_alpha: 1.0,
            liquidity_tilt_ref: ref_illiq,
            liquidity_tilt_w_lo: 0.6,
            liquidity_tilt_w_hi: 6.5,
            ..Default::default()
        };
        let build = |params: OrbParams, illiq: Option<f64>| -> (OrbStrategy, InstrumentId) {
            let id = InstrumentId::from("005930.XKRX");
            let selected = vec![SelectedSymbol {
                instrument_id: id,
                bar_type: BarKind::Minute(1).bar_type(id).unwrap(),
                gap_prices: SessionGapPrices::new(60_000, 63_000),
                prior_atr: None,
                prior_open_vol_mean: None,
                prior_illiq: illiq,
            }];
            let mut s = OrbStrategy::new(params, selected, DecisionSink::new(), 1.0);
            let p = s.params.clone();
            {
                let st = s.states.get_mut(&id).unwrap();
                assert!(st.on_bar(tm(9, 0), 61_500, 60_000, 61_500, 0.0, &p).is_empty());
                assert!(st.on_bar(tm(9, 10), 61_500, 60_000, 61_500, 0.0, &p).is_empty());
                let acts = st.on_bar(tm(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
                assert_eq!(acts, vec![OrbAction::Enter { limit_price: 62_000 }]);
            }
            (s, id)
        };
        // Illiquid (illiq above ref → w < 1) sizes strictly smaller than the untilted path.
        let (s_illiq, id) = build(armed.clone(), Some(ref_illiq * 3.0));
        let q_illiq = s_illiq.entry_qty(&id, 62_000, 2_000.0, 1.0);
        let untilted = armed.position_qty_risked_at(62_000.0, 2_000.0, 1.0);
        assert!(q_illiq < untilted, "illiquid name downsized: {q_illiq} < {untilted}");
        // Off-sentinel: byte-identical to the untilted path for ANY illiq (present/absent/zero).
        let mut off = armed.clone();
        off.liquidity_tilt_alpha = 0.0;
        for illiq in [None, Some(ref_illiq * 3.0), Some(0.0)] {
            let (s, id) = build(off.clone(), illiq);
            assert_eq!(
                s.entry_qty(&id, 62_000, 2_000.0, 1.0),
                off.position_qty_risked_at(62_000.0, 2_000.0, 1.0),
                "off sentinel identical (illiq={illiq:?})"
            );
        }
    }
}
