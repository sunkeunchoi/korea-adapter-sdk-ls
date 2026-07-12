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
    /// when range-R > `or_width_max_atr · ATR`. Sentinel `0.0` = off. Needs ATR;
    /// when ATR is unavailable the gate fails closed (`atr_unavailable`).
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
    pub fn validate(&self) -> Result<(), String> {
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
        // The decoded helpers agree with the filter-off defaults.
        assert_eq!(p.stop_placement(), StopMode::RangeLow);
        assert!(!p.close_confirm_entry());
        assert!(!p.cutoff_active());
        assert_eq!(p.entry_cutoff_time(), None);
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
