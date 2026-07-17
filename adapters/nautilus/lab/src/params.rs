//! ORB v0 parameter set (KTD6) — every value the strategy uses, serde-round-tripped
//! so the whole set lands in the run manifest (R3, R8). These are *starter defaults*
//! the loop exists to revise, never tuned claims.

use std::collections::BTreeMap;

use chrono::NaiveTime;
use nautilus_ls::rules::KRX_REGULAR_OPEN;
use serde::{Deserialize, Serialize};

use crate::agent::context::AgentContext;

/// The strategy identifier recorded in every run id + manifest.
pub const STRATEGY_ID: &str = "orb";

/// The opening-range-breakout parameter set. All fields are manifest-recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbParams {
    /// Strategy identifier (stable across versions).
    pub strategy_id: String,
    /// Strategy version — bumped by a loop turn when the strategy changes (KTD8).
    pub strategy_version: u32,
    /// Universe gap filter: today's open must be at least this % above the prior
    /// session close (a "stock in play"). KTD6 default 3.0.
    pub gap_min_pct: f64,
    /// Universe cap: keep the top-N candidates ranked by prior-session turnover.
    pub universe_top_n: usize,
    /// Risk cap: never hold more than this many positions at once.
    pub max_concurrent: usize,
    /// Opening-range window start (KST). Default the adapter's regular-session open
    /// (09:00) — never re-declared here (KTD6).
    #[serde(with = "hhmmss")]
    pub range_open: NaiveTime,
    /// Opening-range window length in minutes (09:00 + 15 = 09:15 default).
    pub range_minutes: i64,
    /// Time-flat deadline (KST): any open position is closed at/after this time.
    #[serde(with = "hhmmss")]
    pub flat_time: NaiveTime,
    /// Fixed notional (KRW) targeted per position; the entry quantity is
    /// `floor(notional / entry_price)`.
    pub notional_per_position: f64,
    /// Fixed profit target in R-multiples of the opening range
    /// (`R = range_high − range_low`): while Long, exit when a bar's high reaches
    /// `entry_price + profit_target_r · R`. Provisional default **1.0**; **1.5** is
    /// the Step-0 sim optimum reserved for a later param-turn sweep. Prior manifests
    /// lacking this key still deserialize (KTD3) — hence the `serde(default)`.
    #[serde(default = "default_profit_target_r")]
    pub profit_target_r: f64,
    /// Entry breakout-strength band-pass floor (turn 10, R1/KTD2). At the
    /// Armed→entry transition, strength `= (breakout_price − range_high) / R`
    /// (`R = range_high − range_low`); the entry proceeds only when
    /// `breakout_strength_min ≤ strength ≤ breakout_strength_max`. The
    /// pass-through default `0.0` leaves entry behavior unchanged when the field
    /// is absent from a manifest — prior runs in `data/turn4-fresh` deserialize
    /// with the filter disabled (KTD2).
    #[serde(default = "default_breakout_strength_min")]
    pub breakout_strength_min: f64,
    /// Entry breakout-strength band-pass ceiling (turn 10, R1/KTD2). See
    /// [`OrbParams::breakout_strength_min`]. The pass-through default `f64::MAX`
    /// keeps every breakout in-band unless a manifest narrows the ceiling, so
    /// legacy manifests deserialize with the filter disabled.
    #[serde(default = "default_breakout_strength_max")]
    pub breakout_strength_max: f64,
    /// The liquidity floor (KRW daily turnover, plan 2026-07-10-003 R5): a
    /// candidate whose daily-bar `prior_turnover` sits below the floor is
    /// excluded from selection before the gap + turnover rank. A **parameter**,
    /// not a hardcoded blue-chip cut, so the engine can reach into gappier
    /// mid/small-cap tiers while still expressing a tradability-safety floor.
    /// The pass-through default `0.0` disables the floor — legacy manifests
    /// deserialize unchanged.
    #[serde(default)]
    pub turnover_floor_krw: f64,
    /// Stop-placement mode (lever 1, KTD1). `f64`-encoded so `turn()` /
    /// `param_diff` / `numeric_summary` all see it: `0.0` = range-low (v9
    /// default), `1.0` = OR-midpoint, `2.0` = ATR-scaled. Filter-off default
    /// `0.0` reproduces v9 exactly; legacy manifests deserialize with it.
    #[serde(default)]
    pub stop_mode: f64,
    /// Entry-confirmation mode (lever 2, KTD1/KTD6): `0.0` = wick-touch (v9
    /// default — enter when a bar's high exceeds the range high), `1.0` =
    /// close-confirmed (enter only when a bar *closes* strictly above the range
    /// high). Filter-off default `0.0` preserves v9 entry.
    #[serde(default)]
    pub entry_confirm: f64,
    /// ATR-stop multiplier (companion to `stop_mode` 2.0, KTD1/KTD5): the
    /// ATR-mode stop sits `stop_atr_mult · ATR` below entry, clamped never wider
    /// than the range low. Inert unless `stop_mode` is 2.0. Default 2.0.
    #[serde(default = "default_stop_atr_mult")]
    pub stop_atr_mult: f64,
    /// ATR lookback in prior daily sessions (companion, KTD1/KTD5): ATR is
    /// computed from the deduped daily slice strictly before the session; a
    /// symbol-session with fewer than `atr_window`+1 priors fails closed
    /// (`atr_unavailable`) in ATR mode. Inert unless `stop_mode` is 2.0. Default 14.0.
    #[serde(default = "default_atr_window")]
    pub atr_window: f64,
    /// OR-width sanity gate (lever 3, KTD1/KTD7): reject the session done-for-day
    /// when range-R > `or_width_max_atr · ATR`. Sentinel `0.0` = off. The ATR
    /// normalizer is OPTIONAL — a session with no positive prior ATR is simply not
    /// width-gated (SKIP, not `atr_unavailable`): the gate is decoupled from ATR
    /// availability so the width signal is not confounded by ATR coverage.
    #[serde(default)]
    pub or_width_max_atr: f64,
    /// Entry cutoff in minutes after range open (lever 4, KTD1/KTD10): no new
    /// entries once a bar's KST time reaches `range_open + entry_cutoff_min`.
    /// Sentinel `0.0` = off. A configured cutoff must satisfy
    /// `range_end < cutoff ≤ flat_time` (validated at backtest start).
    #[serde(default)]
    pub entry_cutoff_min: f64,
    /// Opening-window relative-volume floor (lever 5, KTD1/KTD9): reject the
    /// session done-for-day when today's opening-window volume is below
    /// `rvol_min ·` the prior-session mean over the same window. Sentinel
    /// `0.0` = off.
    #[serde(default)]
    pub rvol_min: f64,
    /// RVOL prior-session window (companion, KTD1/KTD9): how many prior in-range
    /// sessions are averaged for the RVOL baseline. Inert unless `rvol_min` > 0.0.
    /// Default 14.0.
    #[serde(default = "default_rvol_window_sessions")]
    pub rvol_window_sessions: f64,
    /// RVOL minimum history (companion, KTD1/KTD9): fewer than this many prior
    /// opening-window samples fails closed (`rvol_insufficient_history`) rather
    /// than passing on thin history. Inert unless `rvol_min` > 0.0. Default 5.0.
    #[serde(default = "default_rvol_min_history")]
    pub rvol_min_history: f64,
    /// Breakeven-move exit lever (lever 6, KTD1/KTD11): once a held long's
    /// provably-observed MFE reaches `breakeven_trigger_r · R` (R = the entry-fixed
    /// `r_denom`), the stop ratchets up to the entry price for *subsequent* bars —
    /// so a runner that peaks then gives it back books at-or-near breakeven instead
    /// of decaying to the 15:00 time-flat exit (the largest give-back cohort in v21's
    /// `report mfe`). The exit still fills at the pessimistic bar low (the strategy's
    /// marketable-limit convention, as every stop), so a gap-through books slightly
    /// below entry — conservative: the lever can only under-state, never over-state,
    /// its own expectancy. The ratchet never applies on the bar that triggers it
    /// (same-bar stop-first pessimism, KTD2) and only ever tightens the stop
    /// (entry > every stop-mode's initial level). Sentinel `0.0` = off
    /// (byte-identical to v21); legacy manifests deserialize with it.
    #[serde(default)]
    pub breakeven_trigger_r: f64,
    /// Breakeven-trail exit lever (candidate A on top of lever 6, KTD1/KTD12): once
    /// the breakeven ratchet has ARMED (`high_water ≥ breakeven_trigger_r · R`), the
    /// stop trails `trail_frac_r · R` below the high-water mark for *subsequent* bars,
    /// floored at the entry price — so a runner that peaks well past the trigger then
    /// reverts books a **partial win** at the trailed stop, not just a scratch at
    /// breakeven (the give-back cohort that lever 6 currently books ≈entry). It rides
    /// *on top of* the kept breakeven trigger (0.41) — it never engages before the
    /// ratchet arms, only ever tightens, and never loosens below breakeven. The exit
    /// still fills at the pessimistic bar low (as every stop), so a gap-through books
    /// slightly below the trail — conservative (the lever can only under-state its
    /// edge). Sentinel `0.0` = off: the trail term collapses to `high_water` (too
    /// tight), so OFF is an explicit `trail_frac_r > 0` gate that falls back to the
    /// flat-breakeven ratchet — **outcome-identical to v23** (same orders, fills, and
    /// per-trade P&L, so `performance.json` reconciles 1:1; the only telemetry delta is
    /// the always-on `realized_r` exit field, which no verdict metric reads). Legacy
    /// manifests deserialize with it.
    #[serde(default)]
    pub trail_frac_r: f64,
    /// First risk-based **position-sizing** lever (CLASS B, R5): a fixed KRW risk
    /// budget per trade. Sentinel `0.0` = off — sizing stays the fixed
    /// `notional_per_position` qty (`floor(notional / entry)`), byte/outcome-identical
    /// to v23. When `> 0`, the entry quantity is
    /// `min( floor(risk_per_trade_krw / risk_per_share), floor(notional / entry) )`,
    /// where `risk_per_share = entry_price − stop_price` (the entry-fixed initial
    /// stop); the second term is a **notional ceiling** so the lever can only *shift*
    /// size across setups within the existing capital envelope (a clean reallocation),
    /// never blow it up on a tiny-stop setup. Self-contained (no account/equity seam,
    /// KTD-C). `#[serde(default)]` so legacy `data/turn4-fresh` manifests deserialize
    /// with it off; `validate()` rejects a negative value.
    #[serde(default)]
    pub risk_per_trade_krw: f64,
    /// Session-granular realized-equity compounding lever (CLASS B lever 2, candidate
    /// (c), plan 2026-07-15-001 R7/KTD-2). Scales the per-trade risk budget by the
    /// **session-open realized-equity multiplier** `m` (computed by the runner from
    /// prior sessions' realized P&L against the read-only `starting_balance` and
    /// threaded into the strategy at construction). The effective budget becomes
    /// `risk_per_trade_krw × max(0, 1 + equity_compound_frac · (m − 1))`; at
    /// `equity_compound_frac = 1.0` the risked *fraction* of equity is constant — the
    /// canonical fixed-fractional identity. Sentinel `0.0` = off: the factor collapses
    /// to `max(0, 1) = 1.0` for any `m`, so the budget is exactly `risk_per_trade_krw`
    /// and sizing is byte-identical to v26. `#[serde(default)]` so legacy manifests
    /// deserialize with it off. `validate()` rejects a negative value, a value above
    /// `1.0` (super-proportional compounding is out of scope), and a positive value
    /// while `risk_per_trade_krw == 0` (compounding scales the risk budget — with no
    /// budget it would silently do nothing, so fail fast).
    #[serde(default)]
    pub equity_compound_frac: f64,
    /// Cross-sectionally-normalized ATR budget tilt (CLASS B, ratio-ATR axis, plan
    /// 2026-07-15-002 R1/R2/KTD-1). Multiplies the per-trade risk **budget** by a
    /// dimensionless inverse-ratio weight `w = clamp((ratio_atr_ref / v)^alpha, w_lo,
    /// w_hi)` where `v = prior_atr / entry_price` — a relative (not absolute-KRW)
    /// volatility, so the tilt enters the **numerator only** and cannot collapse to the
    /// dead absolute-ATR lever (price never re-enters sizing through `w`). `alpha` is the
    /// tilt strength and this lever's only flip parameter. Sentinel `0.0` = off: `w ≡ 1`,
    /// so sizing is byte-identical to v26. `#[serde(default)]` so legacy manifests
    /// deserialize with it off. `validate()` rejects a negative value and, when positive,
    /// requires a positive risk budget and a valid frozen clamp band (KTD-1).
    #[serde(default)]
    pub ratio_atr_alpha: f64,
    /// The frozen reference relative-volatility for the ratio-ATR tilt (plan R3/KTD-1):
    /// `v_ref`, the pre-registered median of `v = prior_atr / entry_price` over the head's
    /// ATR-available closed trades. A trade at `v = ratio_atr_ref` gets `w = 1` (neutral).
    /// A pre-registered derivation rule, **not** a swept value. Ignored while
    /// `ratio_atr_alpha == 0.0`; `validate()` requires `> 0.0` when the lever is armed.
    #[serde(default)]
    pub ratio_atr_ref: f64,
    /// The frozen lower clamp on the ratio-ATR weight (plan R3/KTD-2): `w_lo = v_ref /
    /// p90(v)`, the smallest weight (most-downweighted, highest-relative-vol trades).
    /// A pre-registered constant, not swept. Ignored while `ratio_atr_alpha == 0.0`;
    /// `validate()` requires `0 < w_lo ≤ 1.0` when the lever is armed.
    #[serde(default)]
    pub ratio_atr_w_lo: f64,
    /// The frozen upper clamp on the ratio-ATR weight (plan R3/KTD-2): `w_hi = v_ref /
    /// p10(v)`, the largest weight (most-upweighted, lowest-relative-vol trades).
    /// A pre-registered constant, not swept. Ignored while `ratio_atr_alpha == 0.0`;
    /// `validate()` requires `w_hi ≥ 1.0` when the lever is armed.
    #[serde(default)]
    pub ratio_atr_w_hi: f64,
    /// Opening-range gap-retention cutoff (#165/#168). The reserved `1.0` value is
    /// exclusively OFF: manifests record it but the strategy never reads a retention
    /// input. The sole armed value is `0.50` (equality passes) — `validate()` rejects
    /// everything else so no sweep, retune, or companion value can arm the gate.
    /// Legacy manifests resolve to the same OFF sentinel.
    #[serde(default = "default_gap_retention_min")]
    pub gap_retention_min: f64,
    /// Amihud-illiquidity budget tilt (CLASS B, liquidity axis, plan 2026-07-16-003
    /// R1/KD1/KD3). Multiplies the per-trade risk **budget** by a dimensionless
    /// inverse-illiquidity weight `w = clamp((liquidity_tilt_ref / illiq)^alpha, w_lo,
    /// w_hi)` where `illiq = mean over prior `atr_window` sessions of |ret_k| /
    /// (close_k · volume_k)` (the Amihud measure) — down-weighting illiquid breakouts.
    /// `illiq` is already dimensionless-in-price (a ratio of a return to a KRW turnover),
    /// so the tilt enters the **numerator only** and cannot collapse to the stop-based
    /// denominator (the anti-collapse invariant). `alpha` is this lever's only flip
    /// parameter. Sentinel `0.0` = off: `w ≡ 1`, byte-identical to v30. `#[serde(default)]`
    /// so legacy manifests deserialize with it off. `validate()` rejects a negative value
    /// and, when positive, requires a positive risk budget and a valid frozen clamp band.
    #[serde(default)]
    pub liquidity_tilt_alpha: f64,
    /// The frozen reference illiquidity for the liquidity tilt (plan KD2): the
    /// pre-registered median of `illiq` over the head's illiq-available closed trades.
    /// A trade at `illiq = liquidity_tilt_ref` gets `w = 1` (neutral). A pre-registered
    /// derivation rule, **not** a swept value. Ignored while `liquidity_tilt_alpha == 0.0`;
    /// `validate()` requires `> 0.0` when armed. Defaults (both serde-missing and the
    /// `Default` impl) to the frozen pre-register value so the lever is **governed-flippable
    /// in one alpha turn**: a lever-predating head manifest (v30) resolves the frozen
    /// clamp band, and the code-turn re-baseline carries it — inert while `alpha == 0.0`,
    /// so byte-identical to v30 (the `default_profit_target_r` pattern).
    #[serde(default = "default_liquidity_tilt_ref")]
    pub liquidity_tilt_ref: f64,
    /// The frozen lower clamp on the liquidity weight (plan KD2): `w_lo = ref / p90(illiq)`,
    /// the smallest weight (most-downweighted, most-illiquid trades). A pre-registered
    /// constant, not swept. Ignored while `liquidity_tilt_alpha == 0.0`; `validate()`
    /// requires `0 < w_lo ≤ 1.0` when armed. Frozen default (see `liquidity_tilt_ref`).
    #[serde(default = "default_liquidity_tilt_w_lo")]
    pub liquidity_tilt_w_lo: f64,
    /// The frozen upper clamp on the liquidity weight (plan KD2): `w_hi = ref / p10(illiq)`,
    /// the largest weight (most-upweighted, most-liquid trades). A pre-registered constant,
    /// not swept. Ignored while `liquidity_tilt_alpha == 0.0`; `validate()` requires
    /// `w_hi ≥ 1.0` when armed. Frozen default (see `liquidity_tilt_ref`).
    #[serde(default = "default_liquidity_tilt_w_hi")]
    pub liquidity_tilt_w_hi: f64,
}

/// The frozen pre-registered reference illiquidity (plan 2026-07-16-003 KD2): the median
/// Amihud illiquidity over **head v30's** illiq-available closed trades. A serde/`Default`
/// constant (not swept) so the lever's clamp band is present in any lever-predating manifest
/// — inert while `liquidity_tilt_alpha == 0.0`.
///
/// **Deliberate divergence from the sibling `ratio_atr_*` companions** (which default to
/// `0.0`, are seeded into the head manifest at flip time, and so *fail loud* — an armed
/// `ratio_atr_alpha` with a `0.0` band is rejected by `validate()`). These liquidity
/// companions default to a *valid* band instead, which is what makes the lever
/// governed-flippable in one `alpha` turn off the pre-existing v30 head (the manifest need
/// not be re-seeded). The trade-off, by design: arming `liquidity_tilt_alpha` alone sizes
/// against this baked band rather than being rejected — so `alpha` is the single on/off
/// switch and the band is fixed. A future *third* tilt lever should choose consciously
/// between the two patterns, not blindly mirror whichever sibling it sits next to.
///
/// **Staleness contract:** these three constants are a v30-catalog-era snapshot (the
/// derivation cohort). They are inert while `alpha == 0.0`, but if a future turn *arms* a
/// liquidity tilt after the head/catalog era moves, the band must be **re-derived** for the
/// new cohort — a stale code default will not compile-error. (Unlike `default_profit_target_r`,
/// whose `1.0` is an era-independent neutral identity, these are non-neutral sample statistics.)
fn default_liquidity_tilt_ref() -> f64 {
    1.984_881e-13
}

/// The frozen lower clamp `w_lo = ref / p90(illiq)` (plan KD2). See
/// [`default_liquidity_tilt_ref`] for the divergence-from-`ratio_atr` and staleness contract.
fn default_liquidity_tilt_w_lo() -> f64 {
    0.599_572_92
}

/// The frozen upper clamp `w_hi = ref / p10(illiq)` (plan KD2). See
/// [`default_liquidity_tilt_ref`] for the divergence-from-`ratio_atr` and staleness contract.
fn default_liquidity_tilt_w_hi() -> f64 {
    6.541_589_27
}

/// The back-compat default for [`OrbParams::profit_target_r`] (R2, KTD3): a v8
/// manifest written before the field existed deserializes with this value, so
/// every prior run in `data/turn4-fresh` still resolves.
fn default_profit_target_r() -> f64 {
    1.0
}

/// The filter-off default for [`OrbParams::breakout_strength_min`] (R1, KTD2): a
/// pre-turn-10 manifest deserializes with a floor of `0.0`, leaving entry
/// behavior unchanged. Concrete (not `Option`) so `numeric_summary` surfaces the
/// field into every manifest and a governed turn can sweep it.
fn default_breakout_strength_min() -> f64 {
    0.0
}

/// The filter-off default for [`OrbParams::breakout_strength_max`] (R1, KTD2):
/// `f64::MAX` keeps every breakout in-band, so a pre-turn-10 manifest resolves
/// with the band-pass disabled.
fn default_breakout_strength_max() -> f64 {
    f64::MAX
}

/// The companion default for [`OrbParams::stop_atr_mult`] (KTD1/KTD5): inert at
/// 2.0 unless `stop_mode` selects ATR. A pre-field manifest deserializes with it.
fn default_stop_atr_mult() -> f64 {
    2.0
}

/// The companion default for [`OrbParams::atr_window`] (KTD1/KTD5): 14 prior
/// dailies. Inert unless a gate consumes ATR; legacy manifests deserialize with it.
fn default_atr_window() -> f64 {
    14.0
}

/// The companion default for [`OrbParams::rvol_window_sessions`] (KTD1/KTD9): 14
/// prior in-range sessions. Inert unless `rvol_min` > 0.0.
fn default_rvol_window_sessions() -> f64 {
    14.0
}

/// The companion default for [`OrbParams::rvol_min_history`] (KTD1/KTD9): 5
/// prior opening-window samples. Inert unless `rvol_min` > 0.0.
fn default_rvol_min_history() -> f64 {
    5.0
}

/// Back-compatible OFF sentinel for the gap-retention seam (#167).
fn default_gap_retention_min() -> f64 {
    1.0
}

impl Default for OrbParams {
    fn default() -> Self {
        OrbParams {
            strategy_id: STRATEGY_ID.to_string(),
            strategy_version: 0,
            gap_min_pct: 3.0,
            universe_top_n: 20,
            max_concurrent: 5,
            range_open: KRX_REGULAR_OPEN,
            range_minutes: 15,
            // 15:00 KST time-flat (before the 15:30 regular close).
            flat_time: NaiveTime::from_hms_opt(15, 0, 0).expect("valid time"),
            notional_per_position: 10_000_000.0,
            profit_target_r: default_profit_target_r(),
            breakout_strength_min: default_breakout_strength_min(),
            breakout_strength_max: default_breakout_strength_max(),
            turnover_floor_krw: 0.0,
            // Lever-queue gates (KTD1) — all filter-off so v9 behavior is exact.
            stop_mode: 0.0,
            entry_confirm: 0.0,
            stop_atr_mult: default_stop_atr_mult(),
            atr_window: default_atr_window(),
            or_width_max_atr: 0.0,
            entry_cutoff_min: 0.0,
            rvol_min: 0.0,
            rvol_window_sessions: default_rvol_window_sessions(),
            rvol_min_history: default_rvol_min_history(),
            breakeven_trigger_r: 0.0,
            trail_frac_r: 0.0,
            risk_per_trade_krw: 0.0,
            equity_compound_frac: 0.0,
            // Ratio-ATR budget tilt (plan 2026-07-15-002) — sentinel off; the clamp/ref
            // companions stay 0.0 (inert while alpha == 0.0), byte-identical to v26.
            ratio_atr_alpha: 0.0,
            ratio_atr_ref: 0.0,
            ratio_atr_w_lo: 0.0,
            ratio_atr_w_hi: 0.0,
            gap_retention_min: default_gap_retention_min(),
            // Amihud liquidity budget tilt (plan 2026-07-16-003) — alpha sentinel off; the
            // clamp/ref companions carry their frozen pre-register values (inert while alpha
            // == 0.0, so byte-identical to v30), so the lever is governed-flippable in one turn.
            liquidity_tilt_alpha: 0.0,
            liquidity_tilt_ref: default_liquidity_tilt_ref(),
            liquidity_tilt_w_lo: default_liquidity_tilt_w_lo(),
            liquidity_tilt_w_hi: default_liquidity_tilt_w_hi(),
        }
    }
}

/// The decoded stop-placement mode (KTD1). Any unrecognized `stop_mode` value
/// falls back to the v9 range-low stop — an out-of-set float never silently
/// picks a non-default stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    /// v9: stop at the opening-range low.
    RangeLow,
    /// Lever 1: stop at the rounded OR midpoint.
    OrMidpoint,
    /// Lever 1: stop `stop_atr_mult · ATR` below entry, clamped to range low.
    Atr,
}

impl OrbParams {
    /// The opening-range window end (KST): `range_open + range_minutes`.
    pub fn range_end(&self) -> NaiveTime {
        self.range_open + chrono::Duration::minutes(self.range_minutes)
    }

    /// Validate the gate configuration at backtest start (KTD10). A configured
    /// entry cutoff must land strictly after the range end and no later than the
    /// time-flat deadline (`range_end < cutoff ≤ flat_time`); an out-of-range
    /// cutoff is a config error, not a silently-inert gate. Returns the offending
    /// message on failure. Off-sentinel gates (`0.0`) impose no constraint.
    ///
    /// Also rejects a companion window that would make an *active* gate silently
    /// trade nothing (review hardening): a non-positive `atr_window` /
    /// `stop_atr_mult` under ATR mode or the OR-width gate, or a non-positive
    /// `rvol_window_sessions` / `rvol_min_history` under the RVOL gate, otherwise
    /// fails *every* session closed with no config error — a whole run of zero
    /// trades that reads as a real result. A negative `entry_cutoff_min` is a
    /// config error, not the `0.0`-off sentinel.
    pub fn validate(&self) -> Result<(), String> {
        if self.gap_retention_min != default_gap_retention_min() && self.gap_retention_min != 0.50 {
            return Err(format!(
                "gap_retention_min {} is not a governed value — 1.0 is reserved as OFF and 0.50 \
                 is the sole armed cutoff (#165/#168); no sweep or retune is permitted",
                self.gap_retention_min
            ));
        }
        if self.entry_cutoff_min < 0.0 {
            return Err(format!(
                "entry_cutoff_min {} is negative — use 0.0 to disable the cutoff, a positive \
                 minute offset to enable it (KTD10)",
                self.entry_cutoff_min
            ));
        }
        if let Some(cutoff) = self.entry_cutoff_time() {
            let range_end = self.range_end();
            if cutoff <= range_end {
                return Err(format!(
                    "entry_cutoff_min {} places the cutoff at {} ≤ the range end {} — no \
                     trading window before the cutoff (KTD10)",
                    self.entry_cutoff_min, cutoff, range_end
                ));
            }
            if cutoff > self.flat_time {
                return Err(format!(
                    "entry_cutoff_min {} places the cutoff at {} > flat_time {} — the cutoff \
                     can never bind (KTD10)",
                    self.entry_cutoff_min, cutoff, self.flat_time
                ));
            }
        }
        // ATR is consumed by the ATR stop mode and by the OR-width gate; its window
        // must be positive or ATR is never available — every ATR-stop session then
        // fails closed on `atr_unavailable`, and the OR-width gate silently skips every
        // session (inert). Reject the misconfiguration either way.
        let atr_active = self.stop_placement() == StopMode::Atr || self.or_width_max_atr > 0.0;
        if atr_active && self.atr_window <= 0.0 {
            return Err(format!(
                "atr_window {} must be positive when an ATR-consuming gate is active \
                 (stop_mode=ATR or or_width_max_atr>0) — else ATR is never available \
                 (ATR-stop fails closed / OR-width never fires)",
                self.atr_window
            ));
        }
        if self.stop_placement() == StopMode::Atr && self.stop_atr_mult <= 0.0 {
            return Err(format!(
                "stop_atr_mult {} must be positive under ATR stop mode — a non-positive \
                 multiplier collapses the stop onto the entry (an instant stop-out)",
                self.stop_atr_mult
            ));
        }
        if self.rvol_min > 0.0 && (self.rvol_window_sessions <= 0.0 || self.rvol_min_history <= 0.0) {
            return Err(format!(
                "rvol_window_sessions {} and rvol_min_history {} must both be positive under \
                 the RVOL gate — else every session fails rvol_insufficient_history",
                self.rvol_window_sessions, self.rvol_min_history
            ));
        }
        // Breakeven-move trigger (lever 6): 0.0 disables it, a positive R-multiple
        // enables it. A negative trigger would ratchet the stop to breakeven on the
        // first held bar (MFE starts at 0 ≥ a negative threshold) — a near-instant
        // breakeven stop-out, not the intended "off". Reject it as a config error.
        if self.breakeven_trigger_r < 0.0 {
            return Err(format!(
                "breakeven_trigger_r {} is negative — use 0.0 to disable the breakeven move, \
                 a positive R-multiple to arm it (KTD11)",
                self.breakeven_trigger_r
            ));
        }
        // Breakeven-trail give-back (candidate A): 0.0 disables the trail (flat
        // breakeven), a positive R-multiple sets the give-back below the high-water
        // mark. A negative give-back would trail the stop ABOVE the high water — a
        // stop the price has never reached, an instant same-bar stop-out. Reject it.
        if self.trail_frac_r < 0.0 {
            return Err(format!(
                "trail_frac_r {} is negative — use 0.0 to disable the trail (flat breakeven), \
                 a positive R-multiple to trail the stop below the high-water mark (KTD12)",
                self.trail_frac_r
            ));
        }
        // Risk-based sizing lever (CLASS B, R5): 0.0 disables it (fixed notional), a
        // positive KRW budget enables it. A negative budget is a config error — there
        // is no meaningful "negative risk per trade", and it would floor to a negative
        // qty that the sizing gate reads as an instant rejection rather than "off".
        if self.risk_per_trade_krw < 0.0 {
            return Err(format!(
                "risk_per_trade_krw {} is negative — use 0.0 to disable risk sizing (fixed \
                 notional), a positive KRW budget to size each trade to that risk (R5)",
                self.risk_per_trade_krw
            ));
        }
        // Equity-compounding lever (CLASS B lever 2, R7/KTD-2): 0.0 disables it, a
        // positive fraction up to 1.0 enables it. A negative fraction would invert the
        // multiplier (larger equity → smaller size); a fraction above 1.0 is
        // super-proportional compounding, out of scope. And a positive fraction with no
        // risk budget scales nothing — the multiplier applies to `risk_per_trade_krw`,
        // so with a zero budget the param would silently do nothing; fail fast instead
        // of shipping an inert-by-misconfiguration run.
        if self.equity_compound_frac < 0.0 {
            return Err(format!(
                "equity_compound_frac {} is negative — use 0.0 to disable equity compounding, \
                 a positive fraction (≤ 1.0) to scale the risk budget by session-open equity \
                 (R7/KTD-2)",
                self.equity_compound_frac
            ));
        }
        if self.equity_compound_frac > 1.0 {
            return Err(format!(
                "equity_compound_frac {} exceeds 1.0 — super-proportional compounding is out of \
                 scope; 1.0 is the fixed-fractional identity (R7/KTD-2)",
                self.equity_compound_frac
            ));
        }
        if self.equity_compound_frac > 0.0 && self.risk_per_trade_krw == 0.0 {
            return Err(format!(
                "equity_compound_frac {} is active but risk_per_trade_krw is 0.0 — compounding \
                 scales the risk budget, so with no budget it would silently do nothing; enable \
                 risk sizing or disable compounding (R7/KTD-2)",
                self.equity_compound_frac
            ));
        }
        // Ratio-ATR budget tilt (CLASS B, plan 2026-07-15-002 R2/KTD-1): 0.0 disables it;
        // a positive alpha arms the inverse-ratio tilt. A negative alpha would invert the
        // frozen downweight-high-vol direction (out of scope). When armed the lever needs a
        // positive risk budget (it multiplies `risk_per_trade_krw` — with none it does
        // nothing), a positive reference `v_ref` (the ratio the weight normalizes against),
        // and a valid clamp band straddling 1.0 (`0 < w_lo ≤ 1.0 ≤ w_hi`, since `v_ref` is
        // the median so the neutral weight 1.0 must lie inside the band). Fail fast on an
        // inert-by-misconfiguration run, mirroring the equity cross-guard.
        // A non-finite alpha (NaN/±∞) would slip past the sign checks below (`NaN < 0.0`
        // and `NaN > 0.0` are both false → neither branch) yet make `ratio_atr_weight`
        // compute a NaN weight → a silent qty-0 book. Reject it up front so the finite
        // sign/branch logic that follows is total. (Unreachable from a JSON manifest —
        // serde_json rejects NaN/∞ on parse — but validate() is the safety gate.)
        if !self.ratio_atr_alpha.is_finite() {
            return Err(format!(
                "ratio_atr_alpha {} is not finite — use 0.0 to disable the ratio-ATR tilt or a \
                 finite positive exponent (R2/KTD-1)",
                self.ratio_atr_alpha
            ));
        }
        if self.ratio_atr_alpha < 0.0 {
            return Err(format!(
                "ratio_atr_alpha {} is negative — use 0.0 to disable the ratio-ATR tilt, a \
                 positive exponent to downweight high relative-vol names (R2/KTD-1)",
                self.ratio_atr_alpha
            ));
        }
        if self.ratio_atr_alpha > 0.0 {
            if self.risk_per_trade_krw <= 0.0 {
                return Err(format!(
                    "ratio_atr_alpha {} is active but risk_per_trade_krw is {} — the tilt scales \
                     the risk budget, so with no budget it would silently do nothing; enable risk \
                     sizing or disable the tilt (R2/KTD-1)",
                    self.ratio_atr_alpha, self.risk_per_trade_krw
                ));
            }
            // `is_finite()` guards below are load-bearing, not decorative: a NaN `v_ref`
            // passes `<= 0.0` (NaN comparisons are false) → NaN weight → silent qty 0, and a
            // NaN/∞ `w_hi` passes `< 1.0` → `raw.clamp(w_lo, w_hi)` PANICS (clamp panics on a
            // NaN bound). `w_lo`'s compound `(> 0.0 && <= 1.0)` already rejects NaN/∞.
            if !self.ratio_atr_ref.is_finite() || self.ratio_atr_ref <= 0.0 {
                return Err(format!(
                    "ratio_atr_alpha is active but ratio_atr_ref is {} — the weight normalizes \
                     against a finite positive reference v_ref (R3/KTD-1)",
                    self.ratio_atr_ref
                ));
            }
            if !(self.ratio_atr_w_lo > 0.0 && self.ratio_atr_w_lo <= 1.0) {
                return Err(format!(
                    "ratio_atr_alpha is active but ratio_atr_w_lo is {} — the lower clamp must be \
                     in (0, 1.0] (v_ref = median → the neutral weight 1.0 is the band's top-side; \
                     R3/KTD-2)",
                    self.ratio_atr_w_lo
                ));
            }
            if !self.ratio_atr_w_hi.is_finite() || self.ratio_atr_w_hi < 1.0 {
                return Err(format!(
                    "ratio_atr_alpha is active but ratio_atr_w_hi is {} — the upper clamp must be \
                     a finite value ≥ 1.0 so the neutral weight 1.0 lies inside the band (R3/KTD-2)",
                    self.ratio_atr_w_hi
                ));
            }
        }
        // Amihud liquidity budget tilt (CLASS B, plan 2026-07-16-003): same shape as the
        // ratio-ATR guard above — 0.0 disables it, a positive alpha arms the
        // inverse-illiquidity tilt, and when armed it needs a positive risk budget, a
        // positive reference illiquidity, and a valid clamp band straddling 1.0. The
        // `is_finite()` guards are equally load-bearing (a NaN bound panics `clamp`).
        if !self.liquidity_tilt_alpha.is_finite() {
            return Err(format!(
                "liquidity_tilt_alpha {} is not finite — use 0.0 to disable the liquidity tilt \
                 or a finite positive exponent (KD1/KD3)",
                self.liquidity_tilt_alpha
            ));
        }
        if self.liquidity_tilt_alpha < 0.0 {
            return Err(format!(
                "liquidity_tilt_alpha {} is negative — use 0.0 to disable the liquidity tilt, a \
                 positive exponent to downweight illiquid names (KD1/KD3)",
                self.liquidity_tilt_alpha
            ));
        }
        if self.liquidity_tilt_alpha > 0.0 {
            if self.risk_per_trade_krw <= 0.0 {
                return Err(format!(
                    "liquidity_tilt_alpha {} is active but risk_per_trade_krw is {} — the tilt \
                     scales the risk budget, so with no budget it would silently do nothing; \
                     enable risk sizing or disable the tilt (KD1)",
                    self.liquidity_tilt_alpha, self.risk_per_trade_krw
                ));
            }
            if !self.liquidity_tilt_ref.is_finite() || self.liquidity_tilt_ref <= 0.0 {
                return Err(format!(
                    "liquidity_tilt_alpha is active but liquidity_tilt_ref is {} — the weight \
                     normalizes against a finite positive reference illiquidity (KD2)",
                    self.liquidity_tilt_ref
                ));
            }
            if !(self.liquidity_tilt_w_lo > 0.0 && self.liquidity_tilt_w_lo <= 1.0) {
                return Err(format!(
                    "liquidity_tilt_alpha is active but liquidity_tilt_w_lo is {} — the lower clamp \
                     must be in (0, 1.0] (ref = median → the neutral weight 1.0 is the band's \
                     top-side; KD2)",
                    self.liquidity_tilt_w_lo
                ));
            }
            if !self.liquidity_tilt_w_hi.is_finite() || self.liquidity_tilt_w_hi < 1.0 {
                return Err(format!(
                    "liquidity_tilt_alpha is active but liquidity_tilt_w_hi is {} — the upper clamp \
                     must be a finite value ≥ 1.0 so the neutral weight 1.0 lies inside the band \
                     (KD2)",
                    self.liquidity_tilt_w_hi
                ));
            }
        }
        Ok(())
    }

    /// The decoded stop-placement mode (KTD1): `1.0` → OR-midpoint, `2.0` → ATR,
    /// anything else (default `0.0`) → the v9 range-low stop.
    pub fn stop_placement(&self) -> StopMode {
        if self.stop_mode == 1.0 {
            StopMode::OrMidpoint
        } else if self.stop_mode == 2.0 {
            StopMode::Atr
        } else {
            StopMode::RangeLow
        }
    }

    /// Whether close-confirmed entry is active (lever 2, KTD6): `entry_confirm`
    /// `1.0` = enter only on a bar close strictly above the range high. The
    /// filter-off default `0.0` keeps v9 wick-touch entry.
    pub fn close_confirm_entry(&self) -> bool {
        self.entry_confirm == 1.0
    }

    /// Whether the entry cutoff gate is active (lever 4, KTD10): a positive
    /// `entry_cutoff_min`. The sentinel `0.0` disables it.
    pub fn cutoff_active(&self) -> bool {
        self.entry_cutoff_min > 0.0
    }

    /// The wall-clock entry cutoff (`range_open + entry_cutoff_min`), or `None`
    /// when the gate is off (KTD10). No new entry is taken at/after this time.
    pub fn entry_cutoff_time(&self) -> Option<NaiveTime> {
        self.cutoff_active()
            .then(|| self.range_open + chrono::Duration::minutes(self.entry_cutoff_min as i64))
    }

    /// The number of shares to buy for a `notional_per_position` budget at `price`
    /// (floored). Zero when the price exceeds the notional — the sizing gate then
    /// rejects the entry rather than placing a zero-quantity order.
    pub fn position_qty(&self, price: f64) -> i64 {
        if price <= 0.0 {
            return 0;
        }
        (self.notional_per_position / price).floor() as i64
    }

    /// Whether risk-based position sizing is active (CLASS B, R5): a positive
    /// `risk_per_trade_krw`. The sentinel `0.0` keeps the fixed-notional v23 sizing.
    pub fn risk_sizing_active(&self) -> bool {
        self.risk_per_trade_krw > 0.0
    }

    /// Whether the equity-compounding lever is active (CLASS B lever 2, R7/KTD-2): a
    /// positive `equity_compound_frac`. The sentinel `0.0` keeps the flat
    /// (non-compounding) v26 risk budget. `validate()` guarantees this is only true
    /// alongside an active risk budget.
    pub fn equity_compounding_active(&self) -> bool {
        self.equity_compound_frac > 0.0
    }

    /// The session-open equity-compounding factor for a realized-equity multiplier `m`
    /// (R8/KTD-2): `max(0, 1 + equity_compound_frac · (m − 1))`. At the off sentinel
    /// (`equity_compound_frac = 0.0`) this is `max(0, 1) = 1.0` for **any** `m`, so a
    /// flat budget results; at `m = 1.0` (a session with no prior realized P&L) it is
    /// `1.0` for any fraction. The `max(0, …)` clamp floors a deep-drawdown path at a
    /// zero budget rather than a negative one — a zero budget flows into the existing
    /// qty-0 rejection, never a negative qty or a divide artifact.
    pub fn equity_compound_factor(&self, multiplier: f64) -> f64 {
        (1.0 + self.equity_compound_frac * (multiplier - 1.0)).max(0.0)
    }

    /// Whether the ratio-ATR budget tilt is active (CLASS B, plan 2026-07-15-002 R2): a
    /// positive `ratio_atr_alpha`. The sentinel `0.0` keeps the untilted v26 risk budget.
    /// `validate()` guarantees this is only true alongside an active risk budget and a
    /// valid frozen clamp band.
    pub fn ratio_atr_active(&self) -> bool {
        self.ratio_atr_alpha > 0.0
    }

    /// Whether the gap-retention session gate is armed (#165/#168): any value other
    /// than the reserved `1.0` OFF sentinel. `validate()` guarantees the only such
    /// value is the frozen `0.50` cutoff; while OFF the strategy bypasses every
    /// retention read so the head-v30 stream is untouched.
    pub fn gap_retention_active(&self) -> bool {
        self.gap_retention_min != default_gap_retention_min()
    }

    /// The dimensionless ratio-ATR budget multiplier `w` for a trade with prior-daily
    /// `prior_atr` entering at `entry_price` (plan R1/R4/KTD-5). Computes
    /// `w = clamp((ratio_atr_ref / v)^alpha, ratio_atr_w_lo, ratio_atr_w_hi)` on the
    /// **relative** volatility `v = prior_atr / entry_price`, so `w` depends on price only
    /// through the ratio — the anti-collapse property (doubling `prior_atr` and
    /// `entry_price` together leaves `w` unchanged).
    ///
    /// Fails **closed** to the neutral `w = 1.0` (KTD-5) when: the lever is off
    /// (`alpha == 0.0`, the bit-identical sentinel); `prior_atr` is `None` **or** `≤ 0.0`
    /// (the documented `Some(0.0)` flat-deduped-dailies trap — a zero ATR must never make
    /// `v = 0 → w = ∞`); or `entry_price ≤ 0.0`. A `None`/degenerate `prior_atr` is
    /// skip-not-reject: the trade sizes untilted, it is not dropped.
    pub fn ratio_atr_weight(&self, prior_atr: Option<f64>, entry_price: f64) -> f64 {
        if self.ratio_atr_alpha == 0.0 {
            return 1.0;
        }
        let atr = match prior_atr {
            Some(a) if a > 0.0 => a,
            _ => return 1.0,
        };
        if entry_price <= 0.0 {
            return 1.0;
        }
        let v = atr / entry_price;
        let raw = (self.ratio_atr_ref / v).powf(self.ratio_atr_alpha);
        raw.clamp(self.ratio_atr_w_lo, self.ratio_atr_w_hi)
    }

    /// Whether the Amihud liquidity budget tilt is active (CLASS B, plan 2026-07-16-003):
    /// a positive `liquidity_tilt_alpha`. The sentinel `0.0` keeps the untilted v30 budget.
    /// `validate()` guarantees this is only true alongside an active risk budget and a valid
    /// frozen clamp band.
    pub fn liquidity_tilt_active(&self) -> bool {
        self.liquidity_tilt_alpha > 0.0
    }

    /// The dimensionless Amihud-liquidity budget multiplier `w` for a trade whose
    /// prior-session illiquidity is `prior_illiq` (plan KD1/KD3). Computes
    /// `w = clamp((liquidity_tilt_ref / illiq)^alpha, liquidity_tilt_w_lo, liquidity_tilt_w_hi)`.
    /// `illiq` is already a ratio (a return over a KRW turnover), so `w` never re-introduces
    /// the absolute price scale — the anti-collapse property that keeps this orthogonal to
    /// the stop-based `risk_per_share`.
    ///
    /// Fails **closed** to the neutral `w = 1.0` when: the lever is off (`alpha == 0.0`, the
    /// bit-identical sentinel); or `prior_illiq` is `None` **or** `≤ 0.0` (an under-covered
    /// symbol-session, or a degenerate zero — a zero illiq must never make `w = ∞`). A
    /// `None`/degenerate illiq is skip-not-reject: the trade sizes untilted, it is not dropped.
    pub fn liquidity_tilt_weight(&self, prior_illiq: Option<f64>) -> f64 {
        if self.liquidity_tilt_alpha == 0.0 {
            return 1.0;
        }
        let illiq = match prior_illiq {
            Some(x) if x > 0.0 => x,
            _ => return 1.0,
        };
        let raw = (self.liquidity_tilt_ref / illiq).powf(self.liquidity_tilt_alpha);
        raw.clamp(self.liquidity_tilt_w_lo, self.liquidity_tilt_w_hi)
    }

    /// The entry quantity under the risk-based sizing lever (R5): when the lever is
    /// off (or the per-share risk is non-positive — a degenerate stop the risk path
    /// can't divide by), this is exactly [`OrbParams::position_qty`] (the fixed
    /// `notional_per_position` qty, byte-identical to v23). When active with a
    /// positive `risk_per_share = entry_price − stop_price`, it is
    /// `min( floor(risk_per_trade_krw / risk_per_share), floor(notional / price) )` —
    /// the risk-budget qty capped at the fixed-notional qty (the capital ceiling, so
    /// a tiny stop can only *shift* size within the envelope, never blow it up).
    ///
    /// This is the equity multiplier `1.0` case of [`OrbParams::position_qty_risked_at`]
    /// (no compounding), preserved as the v26-identical call path.
    pub fn position_qty_risked(&self, price: f64, risk_per_share: f64) -> i64 {
        self.position_qty_risked_at(price, risk_per_share, 1.0)
    }

    /// The entry quantity under the risk-based sizing lever with the session-open
    /// realized-equity multiplier `multiplier` applied (CLASS B lever 2, R8/KTD-2).
    /// The risk budget is scaled by [`OrbParams::equity_compound_factor`] before the
    /// same `min( floor(budget / risk_per_share), floor(notional / price) )` sizing —
    /// so the notional ceiling and every downstream sizing guard are unchanged. The
    /// off/no-prior-P&L path (`equity_compound_frac = 0.0` **or** `multiplier = 1.0`)
    /// yields a factor of exactly `1.0`, so the budget is `risk_per_trade_krw` and this
    /// is byte-identical to [`OrbParams::position_qty_risked`]. When risk sizing is off
    /// (or the per-share risk is non-positive) the multiplier is irrelevant and this
    /// falls back to the fixed-notional qty. A clamped-to-zero budget (deep drawdown)
    /// floors `floor(0 / rps) = 0`, so `min(0, notional_qty) = 0` → the existing
    /// zero-qty rejection, never a negative qty.
    pub fn position_qty_risked_at(&self, price: f64, risk_per_share: f64, multiplier: f64) -> i64 {
        self.position_qty_risked_tilted(price, risk_per_share, multiplier, 1.0)
    }

    /// The entry quantity under risk sizing with **both** the equity-compounding
    /// multiplier and the ratio-ATR budget tilt applied (CLASS B, plan 2026-07-15-002
    /// R1/KTD-4). The risk budget becomes
    /// `risk_per_trade_krw · equity_compound_factor(multiplier) · weight` before the same
    /// `min( floor(budget / risk_per_share), floor(notional / price) )` sizing — so the
    /// notional ceiling and the `risk_per_share` denominator are untouched (the tilt is a
    /// numerator-only multiplicand, the anti-collapse invariant). `weight` comes from
    /// [`OrbParams::ratio_atr_weight`]; the neutral `weight = 1.0` makes this byte-identical
    /// to [`OrbParams::position_qty_risked_at`] (and, at `multiplier = 1.0`, to
    /// [`OrbParams::position_qty_risked`] — the v26 path). When risk sizing is off (or the
    /// per-share risk is non-positive) both the multiplier and the weight are irrelevant and
    /// this falls back to the fixed-notional qty. A tilt low enough to floor the budget qty
    /// to 0 flows into the existing zero-qty rejection (a downweighted setup freed from the
    /// book), never a negative qty.
    pub fn position_qty_risked_tilted(
        &self,
        price: f64,
        risk_per_share: f64,
        multiplier: f64,
        weight: f64,
    ) -> i64 {
        if !self.risk_sizing_active() || risk_per_share <= 0.0 {
            return self.position_qty(price);
        }
        let budget = self.risk_per_trade_krw * self.equity_compound_factor(multiplier) * weight;
        let risked = (budget / risk_per_share).floor() as i64;
        risked.min(self.position_qty(price))
    }

    /// Whether a new position may be opened given the current open-position count
    /// (the `max_concurrent` risk cap, KTD6).
    pub fn sizing_allows(&self, open_positions: usize) -> bool {
        open_positions < self.max_concurrent
    }

    /// Whether a breakout of the given `strength` passes the band-pass filter
    /// (turn 10, R2/KTD6): the **inclusive** band `[min, max]`. Strength is
    /// `(breakout_price − range_high) / R`; a degenerate range (`R ≤ 0`) is the
    /// caller's concern — it bypasses the filter and never reaches here (KTD6).
    /// With the filter-off defaults (`0.0`, `f64::MAX`) every positive-strength
    /// breakout is in-band, so legacy entry behavior is preserved.
    pub fn strength_in_band(&self, strength: f64) -> bool {
        strength >= self.breakout_strength_min && strength <= self.breakout_strength_max
    }

    /// The numeric (f64-able) fields of this parameter set, keyed by serde
    /// field name. String-typed fields (strategy id, `HH:MM:SS` times) are
    /// omitted — context params maps are `f64`-valued.
    pub fn numeric_summary(&self) -> BTreeMap<String, f64> {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(map)) => map
                .into_iter()
                .filter_map(|(k, v)| v.as_f64().map(|n| (k, n)))
                .collect(),
            _ => BTreeMap::new(),
        }
    }

    /// The minimal in-run telemetry context (R5) for a decision made under this
    /// parameter set: strategy id + version, the [`OrbParams::numeric_summary`]
    /// as the params summary, and the caller's running counts. Constructible at
    /// the universe scan (before the engine) and inside the engine thread — no
    /// account or position state (R9).
    pub fn telemetry_context(&self, counts: BTreeMap<String, u64>) -> AgentContext {
        AgentContext::telemetry(
            self.strategy_id.clone(),
            self.strategy_version,
            self.numeric_summary(),
            counts,
        )
    }
}

/// Serialize/deserialize a `NaiveTime` as `"HH:MM:SS"` so the manifest is readable
/// and diff-friendly (chrono's default is the same, but pinning the format keeps the
/// manifest stable across chrono versions).
mod hhmmss {
    use chrono::NaiveTime;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FMT: &str = "%H:%M:%S";

    pub fn serialize<S: Serializer>(t: &NaiveTime, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&t.format(FMT).to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveTime, D::Error> {
        let raw = String::deserialize(d)?;
        NaiveTime::parse_from_str(&raw, FMT).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_ktd6() {
        let p = OrbParams::default();
        assert_eq!(p.strategy_id, "orb");
        assert_eq!(p.strategy_version, 0);
        assert_eq!(p.gap_min_pct, 3.0);
        assert_eq!(p.universe_top_n, 20);
        assert_eq!(p.max_concurrent, 5);
        assert_eq!(p.range_open, KRX_REGULAR_OPEN);
        assert_eq!(p.range_end(), NaiveTime::from_hms_opt(9, 15, 0).unwrap());
        assert_eq!(p.flat_time, NaiveTime::from_hms_opt(15, 0, 0).unwrap());
        assert_eq!(p.profit_target_r, 1.0);
        // Turn 10: filter-off band defaults leave entry behavior unchanged.
        assert_eq!(p.breakout_strength_min, 0.0);
        assert_eq!(p.breakout_strength_max, f64::MAX);
        // Lever-queue gates (KTD1): every gate default-off, companions inert.
        assert_eq!(p.stop_mode, 0.0, "stop mode defaults to v9 range-low");
        assert_eq!(p.entry_confirm, 0.0, "entry defaults to v9 wick-touch");
        assert_eq!(p.stop_atr_mult, 2.0);
        assert_eq!(p.atr_window, 14.0);
        assert_eq!(p.or_width_max_atr, 0.0, "OR-width gate off");
        assert_eq!(p.entry_cutoff_min, 0.0, "cutoff off");
        assert_eq!(p.rvol_min, 0.0, "RVOL gate off");
        assert_eq!(p.rvol_window_sessions, 14.0);
        assert_eq!(p.rvol_min_history, 5.0);
        assert_eq!(p.breakeven_trigger_r, 0.0, "breakeven move off");
        assert_eq!(p.trail_frac_r, 0.0, "breakeven trail off");
        assert_eq!(p.risk_per_trade_krw, 0.0, "risk sizing off");
        assert_eq!(p.equity_compound_frac, 0.0, "equity compounding off");
        assert_eq!(p.ratio_atr_alpha, 0.0, "ratio-ATR tilt off");
        assert_eq!(p.ratio_atr_ref, 0.0, "ratio-ATR ref unset while off");
        assert_eq!(p.ratio_atr_w_lo, 0.0, "ratio-ATR w_lo unset while off");
        assert_eq!(p.ratio_atr_w_hi, 0.0, "ratio-ATR w_hi unset while off");
        // The decoded helpers agree with the filter-off defaults.
        assert_eq!(p.stop_placement(), StopMode::RangeLow);
        assert!(!p.close_confirm_entry());
        assert!(!p.cutoff_active());
        assert_eq!(p.entry_cutoff_time(), None);
        assert!(!p.risk_sizing_active());
        assert!(!p.equity_compounding_active());
        assert!(!p.ratio_atr_active());
    }

    #[test]
    fn gap_retention_off_seam_is_manifest_recorded_and_legacy_safe() {
        let mut legacy = serde_json::to_value(OrbParams::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("gap_retention_min");

        let resolved: OrbParams = serde_json::from_value(legacy).unwrap();
        assert_eq!(resolved.gap_retention_min, 1.0, "legacy manifests resolve to OFF");
        assert_eq!(
            resolved.numeric_summary().get("gap_retention_min"),
            Some(&1.0),
            "new manifests record the OFF sentinel"
        );
        assert!(resolved.validate().is_ok(), "the reserved OFF sentinel validates");
        assert!(!resolved.gap_retention_active(), "the OFF sentinel never arms the gate");

        // #168: the frozen 0.50 cutoff is the sole armed value (equality passes at the
        // gate); everything else — near-misses, zero, negatives, NaN — stays rejected
        // so no sweep or retune can arm the gate.
        let armed = OrbParams { gap_retention_min: 0.50, ..resolved.clone() };
        assert!(armed.validate().is_ok(), "0.50 is the sole armed cutoff");
        assert!(armed.gap_retention_active(), "0.50 arms the gate");
        for off_set in [0.49, 0.51, 0.75, 0.0, -0.5, f64::NAN] {
            let p = OrbParams { gap_retention_min: off_set, ..resolved.clone() };
            assert!(p.validate().is_err(), "{off_set} is not a governed gap_retention_min");
        }
    }

    #[test]
    fn validate_accepts_off_and_in_bounds_cutoff() {
        // Off by default → no constraint.
        assert!(OrbParams::default().validate().is_ok());
        // range_end 09:15 < 12:00 ≤ flat 15:00 → valid.
        let p = OrbParams { entry_cutoff_min: 180.0, ..Default::default() };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validate_rejects_out_of_bounds_cutoff() {
        // Cutoff at/inside the range end (09:00 + 5 = 09:05 ≤ 09:15) → error.
        let too_early = OrbParams { entry_cutoff_min: 5.0, ..Default::default() };
        assert!(too_early.validate().is_err(), "cutoff ≤ range end must be rejected");
        // Cutoff after flat_time (09:00 + 400 = 15:40 > 15:00) → error.
        let too_late = OrbParams { entry_cutoff_min: 400.0, ..Default::default() };
        assert!(too_late.validate().is_err(), "cutoff > flat_time must be rejected");
        // A negative cutoff is a config error, not the 0.0-off sentinel.
        let neg = OrbParams { entry_cutoff_min: -10.0, ..Default::default() };
        assert!(neg.validate().is_err(), "negative cutoff must be rejected");
    }

    #[test]
    fn validate_rejects_companion_windows_that_zero_out_an_active_gate() {
        // ATR window must be positive when ATR is consumed (stop mode or OR-width).
        let atr_stop_zero_window =
            OrbParams { stop_mode: 2.0, atr_window: 0.0, ..Default::default() };
        assert!(atr_stop_zero_window.validate().is_err(), "ATR mode needs a positive window");
        let or_width_zero_window =
            OrbParams { or_width_max_atr: 3.0, atr_window: 0.0, ..Default::default() };
        assert!(or_width_zero_window.validate().is_err(), "OR-width gate needs a positive ATR window");
        // A non-positive ATR multiplier collapses the stop onto the entry.
        let bad_mult = OrbParams { stop_mode: 2.0, stop_atr_mult: 0.0, ..Default::default() };
        assert!(bad_mult.validate().is_err(), "non-positive stop_atr_mult must be rejected");
        let neg_mult = OrbParams { stop_mode: 2.0, stop_atr_mult: -1.0, ..Default::default() };
        assert!(neg_mult.validate().is_err(), "negative stop_atr_mult must be rejected");
        // RVOL companions must be positive when the gate is active.
        let rvol_zero_window =
            OrbParams { rvol_min: 1.0, rvol_window_sessions: 0.0, ..Default::default() };
        assert!(rvol_zero_window.validate().is_err(), "RVOL gate needs a positive window");
        let rvol_zero_history =
            OrbParams { rvol_min: 1.0, rvol_min_history: 0.0, ..Default::default() };
        assert!(rvol_zero_history.validate().is_err(), "RVOL gate needs positive min history");
        // Inert companions (gate off) impose no constraint even at 0.0.
        let inert = OrbParams { atr_window: 0.0, rvol_window_sessions: 0.0, ..Default::default() };
        assert!(inert.validate().is_ok(), "companions inert when their gate is off");
        // A valid ATR / RVOL config passes.
        let ok = OrbParams { stop_mode: 2.0, rvol_min: 1.0, ..Default::default() };
        assert!(ok.validate().is_ok(), "default companions are valid when gates are on");
    }

    #[test]
    fn validate_breakeven_trigger_r_rejects_negative_arms_positive() {
        // 0.0 (off) and a positive R-multiple both validate; a negative trigger is a
        // config error (it would ratchet the stop to breakeven on the first held bar).
        assert!(OrbParams::default().validate().is_ok(), "off by default");
        let armed = OrbParams { breakeven_trigger_r: 0.4, ..Default::default() };
        assert!(armed.validate().is_ok(), "a positive breakeven trigger is valid");
        let neg = OrbParams { breakeven_trigger_r: -0.1, ..Default::default() };
        assert!(neg.validate().is_err(), "negative breakeven_trigger_r must be rejected");
    }

    #[test]
    fn validate_trail_frac_r_rejects_negative_arms_positive() {
        // 0.0 (off) and a positive R-multiple both validate; a negative give-back is a
        // config error (it would trail the stop above the high water — an instant
        // stop-out at a level the price never reached).
        assert!(OrbParams::default().validate().is_ok(), "off by default");
        let armed = OrbParams { trail_frac_r: 0.25, ..Default::default() };
        assert!(armed.validate().is_ok(), "a positive breakeven trail is valid");
        let neg = OrbParams { trail_frac_r: -0.1, ..Default::default() };
        assert!(neg.validate().is_err(), "negative trail_frac_r must be rejected");
    }

    #[test]
    fn stop_placement_decodes_ktd1_encoding() {
        let mut p = OrbParams::default();
        assert_eq!(p.stop_placement(), StopMode::RangeLow);
        p.stop_mode = 1.0;
        assert_eq!(p.stop_placement(), StopMode::OrMidpoint);
        p.stop_mode = 2.0;
        assert_eq!(p.stop_placement(), StopMode::Atr);
        // An out-of-set value never silently selects a non-default stop.
        p.stop_mode = 3.0;
        assert_eq!(p.stop_placement(), StopMode::RangeLow);
    }

    #[test]
    fn entry_cutoff_time_is_range_open_plus_minutes_when_active() {
        let mut p = OrbParams::default(); // range_open 09:00
        assert_eq!(p.entry_cutoff_time(), None, "off by default");
        p.entry_cutoff_min = 180.0; // 09:00 + 180min = 12:00
        assert!(p.cutoff_active());
        assert_eq!(p.entry_cutoff_time(), NaiveTime::from_hms_opt(12, 0, 0));
    }

    #[test]
    fn params_round_trip_through_json() {
        let p = OrbParams::default();
        let json = serde_json::to_string(&p).unwrap();
        let back: OrbParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
        // Time fields are human-readable in the manifest.
        assert!(json.contains("\"09:00:00\""), "json: {json}");
        assert!(json.contains("\"15:00:00\""), "json: {json}");
        // The profit target rides the manifest so a param-turn can sweep it.
        assert!(json.contains("\"profit_target_r\":1.0"), "json: {json}");
        // The band edges ride the manifest too (turn 10) — the filter-off
        // ceiling f64::MAX round-trips as the largest finite f64.
        let back_band: OrbParams = serde_json::from_str(&json).unwrap();
        assert_eq!(back_band.breakout_strength_min, 0.0);
        assert_eq!(back_band.breakout_strength_max, f64::MAX);
    }

    #[test]
    fn band_params_round_trip_explicit_values() {
        // Turn 10: an explicit band [0.06, 0.13] (the filtered-run values)
        // serializes and deserializes unchanged.
        let mut p = OrbParams::default();
        p.breakout_strength_min = 0.06;
        p.breakout_strength_max = 0.13;
        let back: OrbParams = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.breakout_strength_min, 0.06);
        assert_eq!(back.breakout_strength_max, 0.13);
    }

    #[test]
    fn band_params_deserialize_from_pre_field_manifest() {
        // R1 / KTD2: a v9-era manifest predates the band fields. Its JSON has no
        // such keys, yet must still deserialize — the serde defaults supply the
        // filter-off band (0.0, f64::MAX) so every prior run keeps resolving with
        // unchanged entry behavior.
        let legacy = serde_json::json!({
            "strategy_id": "orb",
            "strategy_version": 9,
            "gap_min_pct": 0.6,
            "universe_top_n": 40,
            "max_concurrent": 7,
            "range_open": "09:00:00",
            "range_minutes": 20,
            "flat_time": "15:00:00",
            "notional_per_position": 10_000_000.0,
            "profit_target_r": 1.0,
        })
        .to_string();
        let p: OrbParams = serde_json::from_str(&legacy).unwrap();
        assert_eq!(p.breakout_strength_min, 0.0, "missing floor defaults to 0.0");
        assert_eq!(p.breakout_strength_max, f64::MAX, "missing ceiling defaults to f64::MAX");
        assert_eq!(p.strategy_version, 9);
    }

    #[test]
    fn gate_params_deserialize_from_pre_field_manifest() {
        // R2 / KTD1: a v9-era manifest predates every lever-queue gate field. Its
        // JSON has none of the keys, yet must deserialize to the exact filter-off
        // defaults so pre-field runs in `data/turn4-fresh` produce no param_diff
        // (the numeric_summary is what param_diff diffs — proved equal below).
        let legacy = serde_json::json!({
            "strategy_id": "orb",
            "strategy_version": 9,
            "gap_min_pct": 0.6,
            "universe_top_n": 40,
            "max_concurrent": 7,
            "range_open": "09:00:00",
            "range_minutes": 20,
            "flat_time": "15:00:00",
            "notional_per_position": 10_000_000.0,
            "profit_target_r": 1.0,
        })
        .to_string();
        let p: OrbParams = serde_json::from_str(&legacy).unwrap();
        assert_eq!(p.stop_mode, 0.0);
        assert_eq!(p.entry_confirm, 0.0);
        assert_eq!(p.stop_atr_mult, 2.0);
        assert_eq!(p.atr_window, 14.0);
        assert_eq!(p.or_width_max_atr, 0.0);
        assert_eq!(p.entry_cutoff_min, 0.0);
        assert_eq!(p.rvol_min, 0.0);
        assert_eq!(p.rvol_window_sessions, 14.0);
        assert_eq!(p.rvol_min_history, 5.0);
        assert_eq!(p.breakeven_trigger_r, 0.0);
        assert_eq!(p.trail_frac_r, 0.0);
        assert_eq!(p.risk_per_trade_krw, 0.0);
        // Empty param_diff proxy: the deserialized legacy set's numeric summary
        // equals a freshly-defaulted set carrying the same version — no gate key
        // diverges, so a pre-field manifest yields no spurious param_diff (KTD1).
        let mut fresh = OrbParams { strategy_version: 9, ..Default::default() };
        // Match the legacy set's genuinely-set (non-gate) fields.
        fresh.gap_min_pct = 0.6;
        fresh.universe_top_n = 40;
        fresh.max_concurrent = 7;
        fresh.range_minutes = 20;
        assert_eq!(p.numeric_summary(), fresh.numeric_summary());
    }

    #[test]
    fn numeric_summary_includes_gate_fields() {
        // KTD1: every gate param is f64-typed so the serde value-walk surfaces it —
        // a governed turn reads them to flip; an Option/enum would vanish.
        let summary = OrbParams::default().numeric_summary();
        for key in [
            "stop_mode",
            "entry_confirm",
            "stop_atr_mult",
            "atr_window",
            "or_width_max_atr",
            "entry_cutoff_min",
            "rvol_min",
            "rvol_window_sessions",
            "rvol_min_history",
            "breakeven_trigger_r",
            "trail_frac_r",
            "risk_per_trade_krw",
        ] {
            assert!(summary.contains_key(key), "numeric_summary missing {key}");
        }
    }

    #[test]
    fn gate_params_round_trip_explicit_values() {
        // Guards the serde default fns from shadowing real manifest values: a
        // fully-flipped set serializes and deserializes each field unchanged.
        let p = OrbParams {
            stop_mode: 2.0,
            entry_confirm: 1.0,
            stop_atr_mult: 1.5,
            atr_window: 10.0,
            or_width_max_atr: 3.0,
            entry_cutoff_min: 180.0,
            rvol_min: 1.2,
            rvol_window_sessions: 20.0,
            rvol_min_history: 3.0,
            breakeven_trigger_r: 0.5,
            trail_frac_r: 0.25,
            risk_per_trade_krw: 500_000.0,
            equity_compound_frac: 1.0,
            ..Default::default()
        };
        let back: OrbParams = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn numeric_summary_includes_band_fields() {
        // The band edges are f64-typed so the serde value-walk surfaces them into
        // the params summary — where a governed turn reads them to sweep (KTD2:
        // Option = None fields would vanish and never be sweepable).
        let mut p = OrbParams::default();
        p.breakout_strength_min = 0.06;
        p.breakout_strength_max = 0.13;
        let summary = p.numeric_summary();
        assert_eq!(summary.get("breakout_strength_min"), Some(&0.06));
        assert_eq!(summary.get("breakout_strength_max"), Some(&0.13));
    }

    #[test]
    fn profit_target_r_deserializes_from_pre_field_manifest() {
        // R2 / KTD3: a v8-era manifest predates `profit_target_r`. Its JSON has no
        // such key, yet must still deserialize — the serde default supplies 1.0 so
        // every prior run in `data/turn4-fresh` keeps resolving.
        let legacy = serde_json::json!({
            "strategy_id": "orb",
            "strategy_version": 8,
            "gap_min_pct": 3.0,
            "universe_top_n": 20,
            "max_concurrent": 7,
            "range_open": "09:00:00",
            "range_minutes": 20,
            "flat_time": "15:00:00",
            "notional_per_position": 10_000_000.0,
        })
        .to_string();
        let p: OrbParams = serde_json::from_str(&legacy).unwrap();
        assert_eq!(p.profit_target_r, 1.0, "missing key defaults to 1.0");
        assert_eq!(p.strategy_version, 8);
        assert_eq!(p.range_minutes, 20);
    }

    #[test]
    fn numeric_summary_includes_profit_target_r() {
        // The field is f64-typed so the serde value-walk surfaces it into the
        // params summary — where `analyze --scaffold` reads it (KTD1/KTD5).
        let summary = OrbParams::default().numeric_summary();
        assert_eq!(summary.get("profit_target_r"), Some(&1.0));
    }

    #[test]
    fn position_qty_floors_and_guards_zero() {
        let mut p = OrbParams { notional_per_position: 1_000_000.0, ..Default::default() };
        assert_eq!(p.position_qty(60_000.0), 16); // 1_000_000 / 60_000 = 16.6 → 16
        assert_eq!(p.position_qty(0.0), 0);
        p.notional_per_position = 100.0;
        assert_eq!(p.position_qty(60_000.0), 0, "price above notional → zero shares");
    }

    #[test]
    fn position_qty_risked_off_matches_notional_sizing() {
        // R5: with the lever off (0.0), risk sizing is byte-identical to the fixed
        // notional qty for a range of prices/stops (→ v23 exactly).
        let p = OrbParams::default(); // notional 10M, risk_per_trade_krw 0.0
        for (price, rps) in [(60_000.0, 3_000.0), (12_345.0, 500.0), (100.0, 10.0)] {
            assert_eq!(
                p.position_qty_risked(price, rps),
                p.position_qty(price),
                "off-sentinel sizing == notional sizing"
            );
        }
    }

    #[test]
    fn position_qty_risked_scales_qty_inversely_with_stop_distance() {
        // R5: budget 300000 / risk_per_share 3000 → 100 shares; a tighter stop
        // (rps 1500) → 200 shares — capped at the notional ceiling.
        let p = OrbParams {
            risk_per_trade_krw: 300_000.0,
            notional_per_position: 100_000_000.0, // high ceiling so the cap doesn't bind
            ..Default::default()
        };
        assert_eq!(p.position_qty_risked(60_000.0, 3_000.0), 100, "300k/3k = 100");
        assert_eq!(p.position_qty_risked(60_000.0, 1_500.0), 200, "tighter stop → larger qty");
    }

    #[test]
    fn position_qty_risked_notional_cap_binds_on_a_tiny_stop() {
        // A tiny per-share risk makes the risk-budget qty enormous; the notional
        // ceiling clamps it to floor(notional / price) (R5 / KTD-C).
        let p = OrbParams {
            risk_per_trade_krw: 1_000_000.0,
            notional_per_position: 10_000_000.0,
            ..Default::default()
        };
        // risk-budget qty = 1_000_000 / 10 = 100_000; notional cap = 10M/60k = 166.
        assert_eq!(p.position_qty_risked(60_000.0, 10.0), 166, "clamped to notional qty");
        assert_eq!(p.position_qty_risked(60_000.0, 10.0), p.position_qty(60_000.0));
    }

    #[test]
    fn position_qty_risked_degenerate_stop_falls_back_to_notional() {
        // A non-positive per-share risk cannot be divided by — fall back to notional
        // sizing rather than divide by zero / go infinite (R5).
        let p = OrbParams { risk_per_trade_krw: 300_000.0, ..Default::default() };
        assert_eq!(p.position_qty_risked(60_000.0, 0.0), p.position_qty(60_000.0));
        assert_eq!(p.position_qty_risked(60_000.0, -5.0), p.position_qty(60_000.0));
    }

    #[test]
    fn validate_risk_per_trade_krw_rejects_negative_arms_positive() {
        // 0.0 (off) and a positive KRW budget both validate; a negative budget is a
        // config error, not the off sentinel.
        assert!(OrbParams::default().validate().is_ok(), "off by default");
        let armed = OrbParams { risk_per_trade_krw: 500_000.0, ..Default::default() };
        assert!(armed.validate().is_ok(), "a positive risk budget is valid");
        assert!(armed.risk_sizing_active());
        let neg = OrbParams { risk_per_trade_krw: -1.0, ..Default::default() };
        assert!(neg.validate().is_err(), "negative risk_per_trade_krw must be rejected");
    }

    #[test]
    fn risk_per_trade_krw_deserializes_from_pre_field_manifest() {
        // A pre-CLASS-B manifest predates the field — it must deserialize to 0.0 (off)
        // so every prior run in `data/turn4-fresh` keeps resolving unchanged.
        let legacy = serde_json::json!({
            "strategy_id": "orb",
            "strategy_version": 23,
            "gap_min_pct": 0.6,
            "universe_top_n": 40,
            "max_concurrent": 7,
            "range_open": "09:00:00",
            "range_minutes": 20,
            "flat_time": "15:00:00",
            "notional_per_position": 10_000_000.0,
            "profit_target_r": 1.0,
            "breakeven_trigger_r": 0.41,
        })
        .to_string();
        let p: OrbParams = serde_json::from_str(&legacy).unwrap();
        assert_eq!(p.risk_per_trade_krw, 0.0, "missing key defaults to off");
        assert_eq!(p.numeric_summary().get("risk_per_trade_krw"), Some(&0.0));
    }

    #[test]
    fn position_qty_risked_at_off_matches_notional_sizing() {
        // Covers AE3 (param layer): with risk sizing off, the compounding multiplier is
        // irrelevant — qty is the fixed-notional qty across a grid of prices/stops/m.
        let p = OrbParams::default(); // risk_per_trade_krw 0.0, equity_compound_frac 0.0
        for (price, rps) in [(60_000.0, 3_000.0), (12_345.0, 500.0), (100.0, 10.0)] {
            for m in [0.90, 1.0, 1.05, 1.5] {
                assert_eq!(
                    p.position_qty_risked_at(price, rps, m),
                    p.position_qty(price),
                    "risk-off sizing == notional sizing at m={m}"
                );
            }
        }
    }

    #[test]
    fn position_qty_risked_at_multiplier_one_matches_uncompounded() {
        // Covers AE3 (param layer): m = 1.0 (a first session with no prior P&L) yields a
        // factor of exactly 1.0 for ANY fraction, so the compounded path is byte-identical
        // to the uncompounded position_qty_risked across a grid.
        let p = OrbParams {
            risk_per_trade_krw: 299_340.0,
            notional_per_position: 10_000_000.0,
            ..Default::default()
        };
        for f in [0.0, 0.5, 1.0] {
            let pf = OrbParams { equity_compound_frac: f, ..p.clone() };
            for (price, rps) in [(60_000.0, 3_000.0), (12_345.0, 1_500.0), (1_752_000.0, 103_000.0)] {
                assert_eq!(
                    pf.position_qty_risked_at(price, rps, 1.0),
                    p.position_qty_risked(price, rps),
                    "m=1.0 with f={f} == uncompounded"
                );
            }
        }
    }

    #[test]
    fn equity_compound_factor_interpolates() {
        // KTD-2: factor = max(0, 1 + f·(m − 1)). f = 1.0 is the identity in (m − 1);
        // a fractional f interpolates; the off sentinel is flat at 1.0 for any m.
        let full = OrbParams { equity_compound_frac: 1.0, ..Default::default() };
        assert!((full.equity_compound_factor(1.05) - 1.05).abs() < 1e-12, "f=1 → +5%");
        assert!((full.equity_compound_factor(0.95) - 0.95).abs() < 1e-12, "f=1 → −5%");
        let half = OrbParams { equity_compound_frac: 0.5, ..Default::default() };
        assert!((half.equity_compound_factor(1.04) - 1.02).abs() < 1e-12, "f=0.5, m=1.04 → +2%");
        let off = OrbParams::default(); // f = 0.0
        for m in [0.5, 1.0, 1.05, 2.0] {
            assert!((off.equity_compound_factor(m) - 1.0).abs() < 1e-12, "off → flat 1.0 at m={m}");
        }
    }

    #[test]
    fn position_qty_risked_at_scales_budget_with_the_multiplier() {
        // R8 / AE4 (param layer): f = 1.0, budget 300_000. m = 1.05 → budget 315_000 →
        // 105 shares at rps 3_000; m = 0.95 → 285_000 → 95 shares. Notional ceiling high
        // so the risk budget binds.
        let p = OrbParams {
            risk_per_trade_krw: 300_000.0,
            equity_compound_frac: 1.0,
            notional_per_position: 1_000_000_000.0,
            ..Default::default()
        };
        assert_eq!(p.position_qty_risked_at(60_000.0, 3_000.0, 1.0), 100, "m=1 → 300k/3k = 100");
        assert_eq!(p.position_qty_risked_at(60_000.0, 3_000.0, 1.05), 105, "m=1.05 → +5% budget");
        assert_eq!(p.position_qty_risked_at(60_000.0, 3_000.0, 0.95), 95, "m=0.95 → −5% budget");
        // Still capped by the notional ceiling regardless of the multiplier.
        let capped = OrbParams { notional_per_position: 10_000_000.0, ..p };
        // risk-budget qty at m=1.05 = 315_000/100 = 3_150; notional cap = 10M/60k = 166.
        assert_eq!(capped.position_qty_risked_at(60_000.0, 100.0, 1.05), 166, "notional ceiling binds");
        assert_eq!(capped.position_qty_risked_at(60_000.0, 100.0, 1.05), capped.position_qty(60_000.0));
    }

    #[test]
    fn position_qty_risked_at_clamps_deep_drawdown_to_zero() {
        // KTD-2: a multiplier low enough that 1 + f·(m − 1) ≤ 0 clamps the factor (and
        // thus the budget) to 0 → floor(0 / rps) = 0 → qty 0 (the existing zero-qty
        // rejection), never a negative qty or a divide artifact.
        let p = OrbParams {
            risk_per_trade_krw: 300_000.0,
            equity_compound_frac: 1.0,
            notional_per_position: 1_000_000_000.0,
            ..Default::default()
        };
        // m = 0.0 → factor max(0, 1 + 1·(−1)) = 0 → budget 0 → qty 0.
        assert_eq!(p.equity_compound_factor(0.0), 0.0);
        assert_eq!(p.position_qty_risked_at(60_000.0, 3_000.0, 0.0), 0, "zero budget → zero qty");
        // m below zero clamps too (would be a negative factor otherwise).
        assert_eq!(p.equity_compound_factor(-0.5), 0.0);
        assert_eq!(p.position_qty_risked_at(60_000.0, 3_000.0, -0.5), 0);
    }

    #[test]
    fn position_qty_risked_at_degenerate_stop_falls_back_to_notional() {
        // A non-positive per-share risk cannot be divided by — fall back to notional
        // sizing regardless of the multiplier (R8, mirrors position_qty_risked).
        let p = OrbParams {
            risk_per_trade_krw: 300_000.0,
            equity_compound_frac: 1.0,
            ..Default::default()
        };
        assert_eq!(p.position_qty_risked_at(60_000.0, 0.0, 1.05), p.position_qty(60_000.0));
        assert_eq!(p.position_qty_risked_at(60_000.0, -5.0, 1.05), p.position_qty(60_000.0));
    }

    #[test]
    fn validate_equity_compound_frac_bounds_and_budget_coupling() {
        // R7 / KTD-2: 0.0 (off) and 1.0 (the fixed-fractional identity) with a positive
        // budget both validate; a negative fraction, a fraction > 1.0, and a positive
        // fraction with no risk budget are all config errors.
        let base = OrbParams { risk_per_trade_krw: 299_340.0, ..Default::default() };
        assert!(OrbParams { equity_compound_frac: 0.0, ..base.clone() }.validate().is_ok(), "off");
        let armed = OrbParams { equity_compound_frac: 1.0, ..base.clone() };
        assert!(armed.validate().is_ok(), "f=1.0 with a positive budget is valid");
        assert!(armed.equity_compounding_active());
        assert!(
            OrbParams { equity_compound_frac: 0.5, ..base.clone() }.validate().is_ok(),
            "a fractional f with a positive budget is valid"
        );
        let neg = OrbParams { equity_compound_frac: -0.1, ..base.clone() };
        assert!(neg.validate().is_err(), "negative equity_compound_frac must be rejected");
        let over = OrbParams { equity_compound_frac: 1.5, ..base.clone() };
        assert!(over.validate().is_err(), "equity_compound_frac > 1.0 must be rejected");
        // A positive fraction with no risk budget scales nothing → fail fast.
        let no_budget = OrbParams { equity_compound_frac: 1.0, risk_per_trade_krw: 0.0, ..Default::default() };
        assert!(no_budget.validate().is_err(), "compounding with no risk budget must be rejected");
    }

    #[test]
    fn equity_compound_frac_deserializes_from_pre_field_manifest() {
        // A v26-era manifest predates the field — it must deserialize to 0.0 (off) so
        // every prior run in `data/turn4-fresh` keeps resolving unchanged, and the field
        // surfaces into numeric_summary so a later governed sweep can move it.
        let legacy = serde_json::json!({
            "strategy_id": "orb",
            "strategy_version": 26,
            "gap_min_pct": 0.6,
            "universe_top_n": 40,
            "max_concurrent": 7,
            "range_open": "09:00:00",
            "range_minutes": 20,
            "flat_time": "15:00:00",
            "notional_per_position": 10_000_000.0,
            "profit_target_r": 1.0,
            "breakeven_trigger_r": 0.41,
            "risk_per_trade_krw": 299_340.0,
        })
        .to_string();
        let p: OrbParams = serde_json::from_str(&legacy).unwrap();
        assert_eq!(p.equity_compound_frac, 0.0, "missing key defaults to off");
        assert_eq!(p.numeric_summary().get("equity_compound_frac"), Some(&0.0));
    }

    #[test]
    fn numeric_summary_includes_equity_compound_frac() {
        // The field is f64-typed so the serde value-walk surfaces it — a later governed
        // sweep reads it to move the lever (KTD-5).
        let summary = OrbParams::default().numeric_summary();
        assert_eq!(summary.get("equity_compound_frac"), Some(&0.0));
        let on = OrbParams { equity_compound_frac: 1.0, risk_per_trade_krw: 299_340.0, ..Default::default() };
        assert_eq!(on.numeric_summary().get("equity_compound_frac"), Some(&1.0));
    }

    // ================= ratio-ATR budget tilt (CLASS B, plan 2026-07-15-002) =================

    /// An armed tilt with the frozen pre-registered values (PRE-REGISTER-vNEXT-ratio-atr-
    /// budget-tilt.md): alpha 1.0, v_ref = median(v), clamps = v_ref/p90 .. v_ref/p10.
    fn armed_ratio() -> OrbParams {
        OrbParams {
            risk_per_trade_krw: 299_340.0,
            notional_per_position: 10_000_000.0,
            ratio_atr_alpha: 1.0,
            ratio_atr_ref: 0.073_157_64,
            ratio_atr_w_lo: 0.702_697_55,
            ratio_atr_w_hi: 1.445_489_56,
            ..Default::default()
        }
    }

    #[test]
    fn ratio_atr_weight_off_is_exactly_one_and_sizing_unchanged() {
        // Covers AE1 (helper layer): the 0.0 sentinel returns weight exactly 1.0 for any
        // inputs, and the tilted sizing path is byte-identical to the untilted one across a
        // grid of prices / stops / multipliers.
        let p = OrbParams {
            risk_per_trade_krw: 299_340.0,
            notional_per_position: 10_000_000.0,
            ratio_atr_ref: 0.07,
            ratio_atr_w_lo: 0.7,
            ratio_atr_w_hi: 1.4,
            ..Default::default() // ratio_atr_alpha 0.0 (off)
        };
        assert!(!p.ratio_atr_active());
        for atr in [None, Some(0.0), Some(1_000.0), Some(50_000.0)] {
            for px in [100.0, 12_345.0, 1_752_000.0] {
                assert_eq!(p.ratio_atr_weight(atr, px), 1.0, "off sentinel → weight 1.0");
            }
        }
        for (price, rps) in [(60_000.0, 3_000.0), (12_345.0, 500.0), (100.0, 10.0)] {
            for m in [0.95, 1.0, 1.05] {
                assert_eq!(
                    p.position_qty_risked_tilted(price, rps, m, 1.0),
                    p.position_qty_risked_at(price, rps, m),
                    "weight 1.0 tilted == untilted"
                );
            }
        }
    }

    #[test]
    fn ratio_atr_weight_fails_closed_on_bad_inputs() {
        // Covers AE2 + KTD-5: an absent, zero, or negative prior_atr (the Some(0.0)
        // flat-deduped-dailies trap) and a non-positive entry price all fail CLOSED to the
        // neutral weight 1.0 — a zero ATR must never make v = 0 → w = ∞.
        let p = armed_ratio();
        assert_eq!(p.ratio_atr_weight(None, 60_000.0), 1.0, "no prior_atr → neutral");
        assert_eq!(p.ratio_atr_weight(Some(0.0), 60_000.0), 1.0, "Some(0.0) → neutral (trap)");
        assert_eq!(p.ratio_atr_weight(Some(-1.0), 60_000.0), 1.0, "negative atr → neutral");
        assert_eq!(p.ratio_atr_weight(Some(5_000.0), 0.0), 1.0, "zero price → neutral");
        assert_eq!(p.ratio_atr_weight(Some(5_000.0), -10.0), 1.0, "negative price → neutral");
        // AE2 at the sizing layer: an untiltable trade sizes exactly as the untilted path.
        let w = p.ratio_atr_weight(None, 60_000.0);
        assert_eq!(
            p.position_qty_risked_tilted(60_000.0, 3_000.0, 1.0, w),
            p.position_qty_risked(60_000.0, 3_000.0),
            "no-ATR trade sizes untilted"
        );
    }

    #[test]
    fn ratio_atr_weight_is_one_at_the_reference() {
        // v == v_ref → (v_ref/v)^alpha = 1 → weight exactly 1.0 for ANY alpha (inside the
        // band, which straddles 1.0 by construction). entry_price 1.0 so v = prior_atr.
        for alpha in [0.5, 1.0, 2.0] {
            let p = OrbParams { ratio_atr_alpha: alpha, ..armed_ratio() };
            let w = p.ratio_atr_weight(Some(p.ratio_atr_ref), 1.0);
            assert!((w - 1.0).abs() < 1e-12, "v = v_ref → w = 1.0 at alpha={alpha}, got {w}");
        }
    }

    #[test]
    fn ratio_atr_weight_monotone_and_price_scale_invariant() {
        // The anti-collapse regression test (KD-1): weight is non-increasing in v, and
        // doubling BOTH prior_atr and entry_price leaves v — and thus w — unchanged (price
        // cannot re-enter sizing through the ratio).
        let p = armed_ratio();
        let mut last = f64::INFINITY;
        for i in 0..200 {
            let v = 0.02 + (i as f64) * 0.001; // 0.02 .. 0.219, spanning the clamps
            let w = p.ratio_atr_weight(Some(v), 1.0);
            assert!(w <= last + 1e-15, "weight must be non-increasing in v (v={v})");
            last = w;
        }
        // Price-scale invariance: (atr, px) and (2·atr, 2·px) share v, so share w.
        for (atr, px) in [(3_000.0, 60_000.0), (500.0, 12_345.0), (90_000.0, 1_500_000.0)] {
            let w1 = p.ratio_atr_weight(Some(atr), px);
            let w2 = p.ratio_atr_weight(Some(2.0 * atr), 2.0 * px);
            assert!((w1 - w2).abs() < 1e-15, "w is price-scale invariant (atr={atr}, px={px})");
        }
    }

    #[test]
    fn ratio_atr_weight_clamps_bind_at_system_values() {
        // Clamps bind: v far above v_ref saturates at w_lo, far below at w_hi. Boundary
        // inputs are DERIVED from the params (v = ref/w_lo is the exact w_lo knee), never
        // hand literals — per the bound-comparison-at-full-float-precision learning. Asserts
        // against the system-produced clamp fields, not decimal literals.
        let p = armed_ratio();
        // v well past the low knee (ref/w_lo) → raw < w_lo → clamped to w_lo.
        let v_lo_knee = p.ratio_atr_ref / p.ratio_atr_w_lo;
        assert_eq!(p.ratio_atr_weight(Some(v_lo_knee * 2.0), 1.0), p.ratio_atr_w_lo, "saturates at w_lo");
        // v well below the high knee (ref/w_hi) → raw > w_hi → clamped to w_hi.
        let v_hi_knee = p.ratio_atr_ref / p.ratio_atr_w_hi;
        assert_eq!(p.ratio_atr_weight(Some(v_hi_knee * 0.5), 1.0), p.ratio_atr_w_hi, "saturates at w_hi");
        // Exactly at each knee the weight equals the clamp value (system-produced).
        assert!((p.ratio_atr_weight(Some(v_lo_knee), 1.0) - p.ratio_atr_w_lo).abs() < 1e-12);
        assert!((p.ratio_atr_weight(Some(v_hi_knee), 1.0) - p.ratio_atr_w_hi).abs() < 1e-12);
    }

    #[test]
    fn ratio_atr_tilt_downsizes_high_vol_relative_to_low_vol() {
        // Covers AE4 (helper layer): two trades with equal risk_per_share but v at the
        // untreated p90 (high vol) vs p10 (low vol). The high-v trade weights to w_lo, the
        // low-v to w_hi, so with the notional ceiling slack the high-v qty is STRICTLY lower.
        let p = OrbParams { notional_per_position: 1_000_000_000.0, ..armed_ratio() };
        let p90 = 0.104_109_72; // untreated 90th pct of v (frozen reading)
        let p10 = 0.050_610_98; // untreated 10th pct of v (frozen reading)
        let rps = 3_000.0;
        let w_hi_v = p.ratio_atr_weight(Some(p90), 1.0);
        let w_lo_v = p.ratio_atr_weight(Some(p10), 1.0);
        assert!(w_hi_v < 1.0, "high-vol trade downweighted");
        assert!(w_lo_v > 1.0, "low-vol trade upweighted");
        let q_high = p.position_qty_risked_tilted(60_000.0, rps, 1.0, w_hi_v);
        let q_low = p.position_qty_risked_tilted(60_000.0, rps, 1.0, w_lo_v);
        assert!(q_high < q_low, "high-v qty {q_high} strictly below low-v qty {q_low}");
    }

    #[test]
    fn ratio_atr_upweighted_trade_still_capped_by_notional_ceiling() {
        // An upweighted (w_hi) trade cannot exceed the notional ceiling: min(risk_qty,
        // floor(notional/price)) still binds at the notional cap.
        let p = armed_ratio(); // notional 10M
        let w = p.ratio_atr_weight(Some(0.02), 1.0); // deep low-v → w_hi (1.4454)
        assert_eq!(w, p.ratio_atr_w_hi, "deep low-v saturates to w_hi");
        // rps tiny so the risk budget qty is huge → the notional ceiling must bind.
        assert_eq!(
            p.position_qty_risked_tilted(60_000.0, 10.0, 1.0, w),
            p.position_qty(60_000.0),
            "notional ceiling binds even when upweighted"
        );
    }

    #[test]
    fn ratio_atr_tilt_floors_to_zero_on_a_downweighted_thin_budget() {
        // KTD-3 / R11a: a downweight low enough that floor(budget·w / rps) = 0 sizes the
        // trade to qty 0 (the existing zero-qty rejection), never a negative qty.
        let p = OrbParams { notional_per_position: 1_000_000_000.0, ..armed_ratio() };
        // rps set so the untilted budget qty is 1 (budget/rps = 1.1), but w_lo·1.1 < 1
        // (0.70·1.1 = 0.77) floors to 0.
        let rps = p.risk_per_trade_krw / 1.1; // untilted floor(1.1) = 1
        assert_eq!(p.position_qty_risked_tilted(60_000.0, rps, 1.0, 1.0), 1, "untilted qty 1");
        let w = p.ratio_atr_w_lo; // 0.70 downweight → 0.70·1.1 = 0.77 → floor 0
        assert_eq!(p.position_qty_risked_tilted(60_000.0, rps, 1.0, w), 0, "downweight floors to 0");
    }

    #[test]
    fn validate_ratio_atr_bounds_and_couplings() {
        // R2/KTD-1: off (0.0) validates unconditionally; armed requires a positive budget,
        // a positive v_ref, and a clamp band straddling 1.0. Each violation is rejected.
        assert!(OrbParams::default().validate().is_ok(), "off validates");
        assert!(armed_ratio().validate().is_ok(), "armed with frozen values validates");
        assert!(armed_ratio().ratio_atr_active());
        // negative alpha
        assert!(OrbParams { ratio_atr_alpha: -0.1, ..armed_ratio() }.validate().is_err(), "negative alpha");
        // armed with no risk budget
        assert!(
            OrbParams { risk_per_trade_krw: 0.0, ..armed_ratio() }.validate().is_err(),
            "armed with zero budget"
        );
        // armed with non-positive v_ref
        assert!(OrbParams { ratio_atr_ref: 0.0, ..armed_ratio() }.validate().is_err(), "ref = 0");
        // clamp band violations
        assert!(OrbParams { ratio_atr_w_lo: 0.0, ..armed_ratio() }.validate().is_err(), "w_lo = 0");
        assert!(OrbParams { ratio_atr_w_lo: 1.1, ..armed_ratio() }.validate().is_err(), "w_lo > 1.0");
        assert!(OrbParams { ratio_atr_w_hi: 0.9, ..armed_ratio() }.validate().is_err(), "w_hi < 1.0");
    }

    #[test]
    fn validate_ratio_atr_rejects_non_finite_params() {
        // Non-finite armed companions must be rejected: NaN slips past `<`/`<=` (all NaN
        // comparisons are false) and a NaN/∞ w_hi would make `raw.clamp(w_lo, w_hi)` panic.
        // Unreachable via a JSON manifest (serde_json rejects NaN/∞) but validate() is the gate.
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(OrbParams { ratio_atr_alpha: bad, ..armed_ratio() }.validate().is_err(), "alpha {bad}");
            assert!(OrbParams { ratio_atr_ref: bad, ..armed_ratio() }.validate().is_err(), "ref {bad}");
            assert!(OrbParams { ratio_atr_w_lo: bad, ..armed_ratio() }.validate().is_err(), "w_lo {bad}");
            assert!(OrbParams { ratio_atr_w_hi: bad, ..armed_ratio() }.validate().is_err(), "w_hi {bad}");
        }
        // And the armed head with all-finite frozen values still validates and never panics.
        let p = armed_ratio();
        assert!(p.validate().is_ok());
        let _ = p.ratio_atr_weight(Some(6_000.0), 62_000.0); // no panic on the happy path
    }

    #[test]
    fn ratio_atr_fields_deserialize_from_pre_field_manifest() {
        // A v26-era manifest predates the four fields — they must deserialize to 0.0 (lever
        // off, byte-identical sizing) and surface into numeric_summary for a later sweep.
        let legacy = serde_json::json!({
            "strategy_id": "orb",
            "strategy_version": 26,
            "gap_min_pct": 0.6,
            "universe_top_n": 40,
            "max_concurrent": 7,
            "range_open": "09:00:00",
            "range_minutes": 20,
            "flat_time": "15:00:00",
            "notional_per_position": 10_000_000.0,
            "profit_target_r": 1.0,
            "risk_per_trade_krw": 299_340.0,
        })
        .to_string();
        let p: OrbParams = serde_json::from_str(&legacy).unwrap();
        assert_eq!(p.ratio_atr_alpha, 0.0, "missing alpha defaults to off");
        assert_eq!(p.ratio_atr_ref, 0.0);
        assert_eq!(p.ratio_atr_w_lo, 0.0);
        assert_eq!(p.ratio_atr_w_hi, 0.0);
        assert!(!p.ratio_atr_active());
        let s = p.numeric_summary();
        assert_eq!(s.get("ratio_atr_alpha"), Some(&0.0));
        assert_eq!(s.get("ratio_atr_ref"), Some(&0.0));
        assert_eq!(s.get("ratio_atr_w_lo"), Some(&0.0));
        assert_eq!(s.get("ratio_atr_w_hi"), Some(&0.0));
    }

    // ============= Amihud liquidity budget tilt (CLASS B, plan 2026-07-16-003) =============

    /// An armed liquidity tilt with the frozen pre-registered values from the candidate's
    /// dual-gate diagnostic: alpha 1.0, ref = median(illiq), clamps = ref/p90 .. ref/p10 over
    /// the head v30 illiq-available cohort.
    fn armed_liquidity() -> OrbParams {
        // Source the frozen band from the default fns (single source of truth) so a
        // re-derivation of the constants never leaves the fixture asserting against stale values.
        OrbParams {
            risk_per_trade_krw: 299_340.0,
            notional_per_position: 10_000_000.0,
            liquidity_tilt_alpha: 1.0,
            liquidity_tilt_ref: default_liquidity_tilt_ref(),
            liquidity_tilt_w_lo: default_liquidity_tilt_w_lo(),
            liquidity_tilt_w_hi: default_liquidity_tilt_w_hi(),
            ..Default::default()
        }
    }

    #[test]
    fn liquidity_tilt_weight_off_is_exactly_one() {
        // The 0.0 sentinel returns weight exactly 1.0 for any illiq (present, absent, zero) —
        // byte-identical to v30.
        let mut off = armed_liquidity();
        off.liquidity_tilt_alpha = 0.0;
        for illiq in [None, Some(1e-13), Some(0.0), Some(5e-12)] {
            assert_eq!(off.liquidity_tilt_weight(illiq), 1.0, "off sentinel → 1.0 (illiq={illiq:?})");
        }
        assert!(!off.liquidity_tilt_active());
    }

    #[test]
    fn liquidity_tilt_weight_fails_closed_on_bad_inputs() {
        // An absent, zero, or negative illiq fails CLOSED to the neutral weight 1.0 — a zero
        // illiq must never make ref/illiq → ∞. Skip-not-reject: the trade sizes untilted.
        let p = armed_liquidity();
        assert_eq!(p.liquidity_tilt_weight(None), 1.0, "no illiq → neutral");
        assert_eq!(p.liquidity_tilt_weight(Some(0.0)), 1.0, "zero illiq → neutral");
        assert_eq!(p.liquidity_tilt_weight(Some(-1e-13)), 1.0, "negative illiq → neutral");
    }

    #[test]
    fn liquidity_tilt_weight_is_one_at_the_reference_and_clamps_bind() {
        // illiq == ref → (ref/illiq)^alpha = 1 → weight exactly 1.0. Far above ref (illiquid)
        // saturates at w_lo; far below (liquid) at w_hi. Knees derived from the params, never
        // hand literals (bound-comparison-at-full-precision learning).
        let p = armed_liquidity();
        let w_ref = p.liquidity_tilt_weight(Some(p.liquidity_tilt_ref));
        assert!((w_ref - 1.0).abs() < 1e-12, "illiq = ref → w = 1.0, got {w_ref}");
        let lo_knee = p.liquidity_tilt_ref / p.liquidity_tilt_w_lo; // illiq past this → w_lo
        assert_eq!(p.liquidity_tilt_weight(Some(lo_knee * 2.0)), p.liquidity_tilt_w_lo, "illiquid → w_lo");
        let hi_knee = p.liquidity_tilt_ref / p.liquidity_tilt_w_hi; // illiq below this → w_hi
        assert_eq!(p.liquidity_tilt_weight(Some(hi_knee * 0.5)), p.liquidity_tilt_w_hi, "liquid → w_hi");
    }

    #[test]
    fn validate_liquidity_tilt_bounds_and_couplings() {
        // Off validates unconditionally; armed requires a positive budget, a positive ref, and
        // a clamp band straddling 1.0. Non-finite armed companions are rejected (a NaN bound
        // panics `clamp`), and the armed head with frozen values validates without panic.
        assert!(OrbParams::default().validate().is_ok(), "off validates");
        assert!(armed_liquidity().validate().is_ok(), "armed with frozen values validates");
        assert!(armed_liquidity().liquidity_tilt_active());
        assert!(OrbParams { liquidity_tilt_alpha: -0.1, ..armed_liquidity() }.validate().is_err(), "negative alpha");
        assert!(OrbParams { risk_per_trade_krw: 0.0, ..armed_liquidity() }.validate().is_err(), "zero budget");
        assert!(OrbParams { liquidity_tilt_ref: 0.0, ..armed_liquidity() }.validate().is_err(), "ref = 0");
        assert!(OrbParams { liquidity_tilt_w_lo: 0.0, ..armed_liquidity() }.validate().is_err(), "w_lo = 0");
        assert!(OrbParams { liquidity_tilt_w_lo: 1.1, ..armed_liquidity() }.validate().is_err(), "w_lo > 1.0");
        assert!(OrbParams { liquidity_tilt_w_hi: 0.9, ..armed_liquidity() }.validate().is_err(), "w_hi < 1.0");
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(OrbParams { liquidity_tilt_alpha: bad, ..armed_liquidity() }.validate().is_err(), "alpha {bad}");
            assert!(OrbParams { liquidity_tilt_w_hi: bad, ..armed_liquidity() }.validate().is_err(), "w_hi {bad}");
        }
    }

    #[test]
    fn liquidity_tilt_fields_deserialize_from_pre_field_manifest() {
        // A v30-era manifest predates the four fields — they must deserialize to 0.0 (lever
        // off, byte-identical sizing) and surface into numeric_summary for a later sweep.
        let legacy = serde_json::json!({
            "strategy_id": "orb",
            "strategy_version": 30,
            "gap_min_pct": 0.6,
            "universe_top_n": 40,
            "max_concurrent": 7,
            "range_open": "09:00:00",
            "range_minutes": 20,
            "flat_time": "15:00:00",
            "notional_per_position": 10_000_000.0,
            "profit_target_r": 1.0,
            "risk_per_trade_krw": 299_340.0,
        })
        .to_string();
        let p: OrbParams = serde_json::from_str(&legacy).unwrap();
        // alpha defaults OFF (sentinel 0.0 → byte-identical sizing), but the clamp/ref
        // companions resolve to their FROZEN pre-register values so a lever-predating head
        // carries the band (the lever is governed-flippable in one alpha turn). They are inert
        // while alpha == 0.0.
        assert_eq!(p.liquidity_tilt_alpha, 0.0, "missing alpha defaults to off");
        assert!(!p.liquidity_tilt_active(), "off while alpha == 0.0 despite frozen companions");
        assert_eq!(p.liquidity_tilt_weight(Some(5e-13)), 1.0, "companions inert while alpha off");
        let s = p.numeric_summary();
        assert_eq!(s.get("liquidity_tilt_alpha"), Some(&0.0));
        assert_eq!(s.get("liquidity_tilt_ref"), Some(&default_liquidity_tilt_ref()));
        assert_eq!(s.get("liquidity_tilt_w_lo"), Some(&default_liquidity_tilt_w_lo()));
        assert_eq!(s.get("liquidity_tilt_w_hi"), Some(&default_liquidity_tilt_w_hi()));
        // The frozen default band is itself valid (straddles 1.0) so an armed flip validates.
        assert!(OrbParams { liquidity_tilt_alpha: 1.0, ..p }.validate().is_ok());
    }

    #[test]
    fn telemetry_context_carries_numeric_params_only() {
        let p = OrbParams::default();
        let summary = p.numeric_summary();
        assert_eq!(summary.get("gap_min_pct"), Some(&3.0));
        assert!(summary.contains_key("notional_per_position"));
        assert!(!summary.contains_key("strategy_id"), "string fields omitted");
        assert!(!summary.contains_key("range_open"), "HH:MM:SS fields omitted");

        let counts = BTreeMap::from([("decisions".to_string(), 7u64)]);
        let ctx = p.telemetry_context(counts.clone());
        let AgentContext::Telemetry {
            strategy_id, strategy_version, params_hash_or_summary, counts: got,
        } = ctx
        else {
            panic!("expected the Telemetry form");
        };
        assert_eq!(strategy_id, "orb");
        assert_eq!(strategy_version, 0);
        assert_eq!(params_hash_or_summary, summary);
        assert_eq!(got, counts);
    }

    #[test]
    fn sizing_gate_caps_concurrency() {
        let p = OrbParams::default(); // max_concurrent 5
        assert!(p.sizing_allows(4));
        assert!(!p.sizing_allows(5));
        assert!(!p.sizing_allows(6));
    }

    #[test]
    fn filter_off_defaults_pass_every_positive_strength() {
        // R1: the pass-through band [0.0, f64::MAX] admits any breakout (strength
        // is always > 0 for a real break), so legacy entry behavior is preserved.
        let p = OrbParams::default();
        assert!(p.strength_in_band(0.001));
        assert!(p.strength_in_band(0.5));
        assert!(p.strength_in_band(42.0));
    }

    #[test]
    fn strength_band_is_inclusive_on_both_edges() {
        // R2 / KTD6: in-band means min ≤ strength ≤ max — both edges pass.
        let mut p = OrbParams::default();
        p.breakout_strength_min = 0.06;
        p.breakout_strength_max = 0.13;
        assert!(p.strength_in_band(0.06), "the floor is inclusive");
        assert!(p.strength_in_band(0.13), "the ceiling is inclusive");
        assert!(p.strength_in_band(0.09), "a mid-band breakout passes");
        assert!(!p.strength_in_band(0.03), "below the floor is rejected");
        assert!(!p.strength_in_band(0.20), "above the ceiling is rejected");
        // Just outside the edges (float-adjacent) is rejected.
        assert!(!p.strength_in_band(0.06 - 1e-9));
        assert!(!p.strength_in_band(0.13 + 1e-9));
    }
}
