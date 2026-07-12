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

use nautilus_ls::reference::universe_metadata::ConditionerTags;

use crate::agent::envelope::{
    Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger, SignalKind,
};
use crate::agent::sink::DecisionSink;
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
                        ("prior_close", c.prior_close),
                        ("today_open", c.today_open),
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
                vals(&[("gap_pct", gap), ("prior_close", c.prior_close), ("today_open", c.today_open)]),
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
    /// The prior-daily ATR for this symbol-session (KTD5), threaded from the
    /// candidate seam (U2). `None` when fewer than `atr_window`+1 priors — the
    /// ATR stop / OR-width gate fail closed rather than silently fall back.
    prior_atr: Option<f64>,
    /// The prior opening-window volume mean for this symbol-session (KTD9),
    /// threaded from the candidate seam (U2). `None` below `rvol_min_history`.
    prior_open_vol_mean: Option<f64>,
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
            prior_atr: None,
            prior_open_vol_mean: None,
        }
    }
}

impl OrbState {
    /// A fresh state for one symbol's session.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh state seeded with the candidate-seam priors (U2): the prior-daily
    /// ATR and the prior opening-window volume mean the gates read (KTD5, KTD9).
    pub fn with_priors(prior_atr: Option<f64>, prior_open_vol_mean: Option<f64>) -> Self {
        OrbState { prior_atr, prior_open_vol_mean, ..Self::default() }
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
        }
        acts
    }

    /// The per-session gates evaluated once at range fix (KTD7): the first failing
    /// gate returns a [`OrbAction::SessionReject`] naming its single canonical
    /// filter and ends the day. `None` when every active gate passes (the all-off
    /// default). Order is pinned — ATR availability, then OR-width, then RVOL — so a
    /// session failing more than one gate records only the first (KTD7). Every
    /// active gate that needs data it lacks fails closed (never a silent pass).
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
        // 2. OR-width sanity gate (lever 3, KTD7): reject when range-R exceeds
        //    `or_width_max_atr · ATR`. Needs a positive ATR; a missing or
        //    non-positive ATR fails closed (`atr_unavailable`) — a missing input
        //    never passes a gate.
        if params.or_width_max_atr > 0.0 {
            let Some(atr) = self.prior_atr.filter(|a| *a > 0.0) else {
                return Some(session_reject("atr_unavailable", vec![("atr_window", params.atr_window)]));
            };
            let range_r = (self.range_high - self.range_low) as f64;
            let max_width = params.or_width_max_atr * atr;
            if range_r > max_width {
                return Some(session_reject(
                    "or_width_atr",
                    vec![("range_r", range_r), ("atr", atr), ("or_width_max_atr", params.or_width_max_atr)],
                ));
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
        None
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
    /// The symbol's prior-daily ATR for this session (KTD5), threaded to its
    /// [`OrbState`] for the stop / OR-width gates. `None` when unavailable.
    pub prior_atr: Option<f64>,
    /// The symbol's prior opening-window volume mean for this session (KTD9),
    /// threaded to its [`OrbState`] for the RVOL gate. `None` when below history.
    pub prior_open_vol_mean: Option<f64>,
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
        // Thread each selected symbol's prior-daily ATR + opening-window volume
        // mean (U2 candidate seam) onto its fresh state for the gates to read.
        let states = selected
            .iter()
            .map(|s| (s.instrument_id, OrbState::with_priors(s.prior_atr, s.prior_open_vol_mean)))
            .collect();
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
