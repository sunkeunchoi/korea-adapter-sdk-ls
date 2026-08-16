//! The daily-resolution multi-session-hold parameter set (P7, U3) — the sibling of
//! [`crate::params::OrbParams`], carried in the run manifest as an **optional** field
//! so no ORB identity hash moves (KTD4).
//!
//! This type lives beside [`crate::params`] rather than under `crate::strategy`
//! because [`crate::artifacts::manifest`] already imports `crate::params::OrbParams`;
//! putting a manifest-carried *data* type under `strategy/` would create an
//! `artifacts → strategy` dependency for no reason.
//!
//! **Every default here is a frozen term.** `config/lineage-preregistration.json` owns
//! the hold, the target `m`, the directionality, the stop rule, and the steady-state
//! concurrency; the constants below are the typed image of that artifact and the tests
//! assert them *against* it rather than restating them. Where this file and the frozen
//! artifact disagree, the artifact wins and this file is wrong.
//!
//! **Nothing is inherited from ORB (R27).** `OrbParams` is still carried on a daily run
//! — the *shared* candidate assembly (`build_candidates`) reads `atr_window` and friends
//! off it — but every value the daily strategy itself uses (sizing, hold, stop, breadth)
//! comes from here. In particular ORB's `notional_per_position` (10,000,000 KRW against
//! 5 concurrent positions) and its governed `risk_per_trade_krw` are *not* this path's
//! sizing: 128 concurrent positions is a different capital envelope entirely.

use serde::{Deserialize, Serialize};

use crate::params::STRATEGY_ID;

/// The strategy identifier every daily run records in its manifest and run id — the
/// registry discriminator (KTD14).
///
/// It must never be [`crate::params::STRATEGY_ID`]. Both `Manifest.strategy_id` and the
/// run id derive from a parameter set's `strategy_id`, and a daily runner that filled
/// the non-optional `Manifest.params` with `OrbParams::default()` would emit a manifest
/// indistinguishable from an ORB run — making every strategy filter in the registry a
/// no-op for the wrong reason. [`Manifest::new_daily`] therefore derives both fields
/// from *this* value (never from the carried `OrbParams`), and
/// [`DailyParams::validate`] rejects the ORB id outright.
///
/// The lineage this id belongs to is `daily-resolution-v1`; the id itself carries no
/// version, because `strategy_version` already does.
///
/// [`Manifest::new_daily`]: crate::artifacts::manifest::Manifest::new_daily
pub const DAILY_STRATEGY_ID: &str = "daily-ms";

/// The frozen holding period in sessions (`hypothesis.holding_period_sessions`).
/// Derived in the freeze as `ceil(effect_size_ratio_to_orb_gross²)` under √-time
/// scaling, so it is not a knob: moving it moves the required gross edge.
pub const FROZEN_HOLDING_PERIOD_SESSIONS: usize = 16;

/// The frozen entries-per-session target (`hypothesis.target_m`). Bounded above by
/// measurement: at `m = 10` steady-state concurrency would be 160, which is supply the
/// P4 pit walk never measured.
pub const FROZEN_TARGET_M: usize = 8;

/// The frozen directionality (`hypothesis.directionality`). Long/short would halve the
/// sell-tax drag per unit of gross exposure but adds borrow availability, which the LS
/// SDK cannot answer.
pub const FROZEN_DIRECTIONALITY: &str = "long_only";

/// The frozen steady-state concurrency (`hypothesis.steady_state_concurrency`) —
/// `FROZEN_TARGET_M × FROZEN_HOLDING_PERIOD_SESSIONS`.
pub const FROZEN_STEADY_STATE_CONCURRENCY: usize =
    FROZEN_TARGET_M * FROZEN_HOLDING_PERIOD_SESSIONS;

/// The frozen stop rule **verbatim** (`hypothesis.stop_rule`). The freeze states the
/// stop as prose, not as a number, so the typed [`FROZEN_STOP_ATR_MULT`] and
/// [`FROZEN_ATR_WINDOW_SESSIONS`] are asserted against *this string* rather than parsed
/// out of it — a parser would quietly accept a re-worded freeze.
pub const FROZEN_STOP_RULE: &str = "1.5 x ATR(1 session), per position";

/// The stop multiple named in [`FROZEN_STOP_RULE`]. Frozen because
/// `cost_R = 0.0023 / stop_pct`: changing the multiple changes the required gross edge
/// and therefore every figure in the freeze.
pub const FROZEN_STOP_ATR_MULT: f64 = 1.5;

/// The ATR window in **sessions** named in [`FROZEN_STOP_RULE`]: ATR(1 session).
///
/// This is a distinct term from ORB's `atr_window`, whose default of 14 needs 15 prior
/// daily bars before a symbol is ever tradable. The daily path's stop is one session of
/// true range, so the window is 1 and the shared `prior_atr` helper — which takes
/// `params.atr_window` as an `f64` — is driven from here.
pub const FROZEN_ATR_WINDOW_SESSIONS: f64 = 1.0;

/// The default per-position notional (KRW): the daily runner's default starting balance
/// (100,000,000 KRW, [`crate::runner::backtest_daily::DailyBacktestConfig::new`])
/// divided by [`FROZEN_STEADY_STATE_CONCURRENCY`] — the notional that deploys the book
/// exactly once at steady state.
///
/// **Deliberately not ORB's 10,000,000 (R27).** ORB sizes 5 concurrent positions; this
/// path holds 128. Inheriting ORB's notional would over-deploy the account by ~12.8×
/// and the run would read as a real result.
pub const DEFAULT_NOTIONAL_PER_POSITION_KRW: f64 = 781_250.0;

/// The daily-path parameter set. Carried in the manifest as
/// `Manifest.daily_params: Option<DailyParams>` (KTD4), so an ORB manifest is
/// byte-identical to its pre-P7 form.
///
/// Every field carries `#[serde(default …)]` per the `OrbParams` convention, so a
/// manifest written before a field existed still deserializes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyParams {
    /// The registry discriminator recorded in the manifest and the run id (KTD14).
    /// Defaults to [`DAILY_STRATEGY_ID`]; [`DailyParams::validate`] rejects
    /// [`crate::params::STRATEGY_ID`] and the empty string.
    #[serde(default = "default_strategy_id")]
    pub strategy_id: String,
    /// Strategy version — bumped when the daily strategy changes.
    #[serde(default)]
    pub strategy_version: u32,
    /// The multi-session hold in sessions. Frozen at
    /// [`FROZEN_HOLDING_PERIOD_SESSIONS`]; `validate()` rejects any other value in
    /// **both** directions (a shorter hold under-accrues the registered effect; a
    /// longer one exceeds the frozen 16-session bootstrap block length, which would
    /// understate the standard error).
    #[serde(default = "default_holding_period_sessions")]
    pub holding_period_sessions: usize,
    /// Entries taken per session: the top `target_m` of the ranked candidates from
    /// those not already held (R10). Frozen ceiling [`FROZEN_TARGET_M`] — a fixture may
    /// run *fewer*, never more (above 8 the implied concurrency is supply the pit walk
    /// never measured).
    #[serde(default = "default_target_m")]
    pub target_m: usize,
    /// The concurrency cap. This path's throttle is `target_m` × the hold, so the cap is
    /// an **assertion**, not a second selection rule: `validate()` rejects a cap below
    /// [`DailyParams::transient_peak_concurrency`], because a binding cap would silently
    /// truncate the frozen hold and the run would still read as a real result. ORB's
    /// `max_concurrent` (5) is emphatically not inherited (R27).
    ///
    /// The bound is the **transient peak**, `target_m × (hold + 1)`, not the steady state
    /// `target_m × hold`. The strategy tests `open + pending` per bar, and within a session
    /// the expiring cohort has not exited yet when that session's entries are evaluated —
    /// so the committed count legitimately reaches one full cohort above the steady state
    /// before settling back. Capping at the steady state made the cap bind *transiently*
    /// and refuse real entries in instrument-id order (measured: up to `target_m` spurious
    /// `concurrency_cap` refusals per session at the frozen terms, 136 against 128), under
    /// a reason that claims the take over-issued when it did not.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Directionality. Frozen at [`FROZEN_DIRECTIONALITY`]; recorded rather than
    /// inferred so a manifest states it, and `validate()` rejects anything else.
    #[serde(default = "default_directionality")]
    pub directionality: String,
    /// The stop multiple: the stop sits `stop_atr_mult × ATR(atr_window_sessions)`
    /// below entry, fixed at entry, per position. Frozen at [`FROZEN_STOP_ATR_MULT`].
    #[serde(default = "default_stop_atr_mult")]
    pub stop_atr_mult: f64,
    /// The ATR window in prior daily sessions, `f64`-encoded to match the `atr_window`
    /// the shared `prior_atr` helper takes. Frozen at [`FROZEN_ATR_WINDOW_SESSIONS`]
    /// (= 1); `validate()` rejects a non-positive or non-finite window, which would make
    /// ATR permanently unavailable and — under the fail-closed stop (KTD9) — reject
    /// every entry for a whole run with no config error.
    #[serde(default = "default_atr_window_sessions")]
    pub atr_window_sessions: f64,
    /// The sizing term: fixed notional (KRW) targeted per position; the entry quantity
    /// is `floor(notional / entry_price)`. Defaults to
    /// [`DEFAULT_NOTIONAL_PER_POSITION_KRW`]. `validate()` rejects a non-positive or
    /// non-finite value, which would floor every entry to a zero quantity.
    #[serde(default = "default_notional_per_position")]
    pub notional_per_position: f64,
}

fn default_strategy_id() -> String {
    DAILY_STRATEGY_ID.to_string()
}

fn default_holding_period_sessions() -> usize {
    FROZEN_HOLDING_PERIOD_SESSIONS
}

fn default_target_m() -> usize {
    FROZEN_TARGET_M
}

fn default_max_concurrent() -> usize {
    // The transient peak, `target_m × (hold + 1)` = 136 at the frozen terms — NOT the
    // frozen steady state of 128. See `DailyParams::transient_peak_concurrency`: a cap at
    // the steady state binds within a session, before the expiring cohort's exits settle.
    FROZEN_TARGET_M * (FROZEN_HOLDING_PERIOD_SESSIONS + 1)
}

fn default_directionality() -> String {
    FROZEN_DIRECTIONALITY.to_string()
}

fn default_stop_atr_mult() -> f64 {
    FROZEN_STOP_ATR_MULT
}

fn default_atr_window_sessions() -> f64 {
    FROZEN_ATR_WINDOW_SESSIONS
}

fn default_notional_per_position() -> f64 {
    DEFAULT_NOTIONAL_PER_POSITION_KRW
}

impl Default for DailyParams {
    fn default() -> Self {
        DailyParams {
            strategy_id: default_strategy_id(),
            strategy_version: 0,
            holding_period_sessions: default_holding_period_sessions(),
            target_m: default_target_m(),
            max_concurrent: default_max_concurrent(),
            directionality: default_directionality(),
            stop_atr_mult: default_stop_atr_mult(),
            atr_window_sessions: default_atr_window_sessions(),
            notional_per_position: default_notional_per_position(),
        }
    }
}

impl DailyParams {
    /// Validate the daily parameter set at run construction, mirroring
    /// [`crate::params::OrbParams::validate`]: return the offending message rather than
    /// shipping an inert-by-misconfiguration run.
    ///
    /// Called from [`Manifest::new_daily`], which is the daily path's manifest (and
    /// therefore run) construction point — so a run that would drift off a frozen term,
    /// or that could not be told apart from an ORB run, never reaches the engine.
    ///
    /// # Errors
    ///
    /// Returns the offending message when the discriminator collides with ORB's, when a
    /// frozen term (hold, target `m`, directionality, stop multiple, ATR window) is off
    /// its freeze, when the sizing term is non-positive, or when the concurrency cap
    /// would bind below the implied steady state.
    ///
    /// [`Manifest::new_daily`]: crate::artifacts::manifest::Manifest::new_daily
    pub fn validate(&self) -> Result<(), String> {
        // --- the registry discriminator (KTD14) -----------------------------------
        if self.strategy_id.trim().is_empty() {
            return Err(
                "daily strategy_id is empty — the manifest and the run id both derive from \
                 it, so an empty id produces an unfilterable run (KTD14)"
                    .to_string(),
            );
        }
        if self.strategy_id == STRATEGY_ID {
            return Err(format!(
                "daily strategy_id {:?} collides with ORB's — a daily run must be \
                 distinguishable from an ORB run in the registry, or every strategy filter \
                 that keys on strategy_id is silently a no-op (KTD14); use {DAILY_STRATEGY_ID:?}",
                self.strategy_id
            ));
        }
        // --- the frozen terms ------------------------------------------------------
        if self.holding_period_sessions < FROZEN_HOLDING_PERIOD_SESSIONS {
            return Err(format!(
                "holding_period_sessions {} is below the frozen {FROZEN_HOLDING_PERIOD_SESSIONS} \
                 (lineage-preregistration.json hypothesis.holding_period_sessions) — the hold is \
                 derived as ceil(effect_ratio²), so a shorter hold under-accrues the registered \
                 effect and the run is not a measurement of this lineage",
                self.holding_period_sessions
            ));
        }
        if self.holding_period_sessions > FROZEN_HOLDING_PERIOD_SESSIONS {
            return Err(format!(
                "holding_period_sessions {} is above the frozen {FROZEN_HOLDING_PERIOD_SESSIONS} \
                 — the freeze's bootstrap_block_length_sessions is tied to the hold, so a longer \
                 hold spans more than one block and understates the standard error",
                self.holding_period_sessions
            ));
        }
        if self.target_m == 0 {
            return Err(
                "target_m is 0 — no session would ever take an entry, and a whole run of zero \
                 trades reads as a real result"
                    .to_string(),
            );
        }
        if self.target_m > FROZEN_TARGET_M {
            return Err(format!(
                "target_m {} exceeds the frozen {FROZEN_TARGET_M} \
                 (lineage-preregistration.json hypothesis.target_m) — a larger m implies a \
                 steady-state concurrency the P4 pit walk never measured",
                self.target_m
            ));
        }
        if self.directionality != FROZEN_DIRECTIONALITY {
            return Err(format!(
                "directionality {:?} is not the frozen {FROZEN_DIRECTIONALITY:?} — long/short \
                 adds a borrow-availability surface the LS SDK cannot answer",
                self.directionality
            ));
        }
        // The stop's two terms are checked for EQUALITY with their freeze, not merely for
        // positivity. A positive-but-off-freeze value (stop_atr_mult 2.0, atr_window 14.0)
        // is the dangerous case: it validates, reaches the engine, and is recorded by
        // `Manifest::new_daily` and the run observation as though it measured the frozen
        // lineage. The sibling terms above — hold, target_m, directionality — are all bounded
        // against their frozen values; these two were not, and the asymmetry was an
        // oversight rather than a decision. A deliberate change to either is a re-freeze of
        // the pre-registration, not a config edit.
        if !self.stop_atr_mult.is_finite() || self.stop_atr_mult <= 0.0 {
            return Err(format!(
                "stop_atr_mult {} must be a finite positive multiple — a non-positive multiple \
                 collapses the stop onto the entry (an instant stop-out); the frozen value is \
                 {FROZEN_STOP_ATR_MULT} ({FROZEN_STOP_RULE})",
                self.stop_atr_mult
            ));
        }
        if self.stop_atr_mult != FROZEN_STOP_ATR_MULT {
            return Err(format!(
                "stop_atr_mult {} is off the frozen {FROZEN_STOP_ATR_MULT} ({FROZEN_STOP_RULE}) \
                 — the stop rule is a pre-registered term, so a run at another multiple would \
                 be recorded as this lineage while measuring a different hypothesis",
                self.stop_atr_mult
            ));
        }
        if !self.atr_window_sessions.is_finite() || self.atr_window_sessions <= 0.0 {
            return Err(format!(
                "atr_window_sessions {} must be a finite positive session count — a non-positive \
                 window makes ATR permanently unavailable, and the stop fails closed on an \
                 unavailable ATR (KTD9), so every entry is rejected for the whole run; the \
                 frozen value is {FROZEN_ATR_WINDOW_SESSIONS} ({FROZEN_STOP_RULE})",
                self.atr_window_sessions
            ));
        }
        if self.atr_window_sessions != FROZEN_ATR_WINDOW_SESSIONS {
            return Err(format!(
                "atr_window_sessions {} is off the frozen {FROZEN_ATR_WINDOW_SESSIONS} \
                 ({FROZEN_STOP_RULE}) — the ATR window is a pre-registered term, and it also \
                 reaches the shared candidate assembly through \
                 `DailyBacktestConfig::assembly_params`, so an off-freeze window silently \
                 changes which entries are derivable at all",
                self.atr_window_sessions
            ));
        }
        // --- the sizing term (R27) --------------------------------------------------
        if !self.notional_per_position.is_finite() || self.notional_per_position <= 0.0 {
            return Err(format!(
                "notional_per_position {} must be a finite positive KRW notional — a \
                 non-positive sizing term floors every entry quantity to 0 and the run books \
                 nothing while reading as a real result",
                self.notional_per_position
            ));
        }
        // --- the concurrency term ---------------------------------------------------
        if self.max_concurrent == 0 {
            return Err(
                "max_concurrent is 0 — no position could ever open; use a cap of at least the \
                 implied steady-state concurrency (target_m × holding_period_sessions)"
                    .to_string(),
            );
        }
        if self.max_concurrent < self.transient_peak_concurrency() {
            return Err(format!(
                "max_concurrent {} is below the implied transient peak concurrency {} \
                 (target_m {} × (holding_period_sessions {} + 1)) — within a session the \
                 expiring cohort has not exited when that session's entries are evaluated, so \
                 a cap at the steady state {} binds transiently and refuses real entries in \
                 instrument-id order. A binding cap silently truncates the frozen hold, so the \
                 cap is an assertion on this path, not a second selection rule (ORB's default \
                 of 5 must not be inherited, R27)",
                self.max_concurrent,
                self.transient_peak_concurrency(),
                self.target_m,
                self.holding_period_sessions,
                self.steady_state_concurrency()
            ));
        }
        Ok(())
    }

    /// The steady-state open-position count this parameter set implies:
    /// `target_m × holding_period_sessions`. At the frozen terms this is
    /// [`FROZEN_STEADY_STATE_CONCURRENCY`] (128).
    #[must_use]
    pub fn steady_state_concurrency(&self) -> usize {
        self.target_m.saturating_mul(self.holding_period_sessions)
    }

    /// The highest open-position count reachable at any instant *within* a session:
    /// `target_m × (holding_period_sessions + 1)`. At the frozen terms this is 136.
    ///
    /// The strategy evaluates `open + pending` per bar, and a session's expiring cohort
    /// has not exited when that session's entries are evaluated — so one full cohort of
    /// `target_m` sits above the steady state until the exits settle. This, not
    /// [`Self::steady_state_concurrency`], is the correct floor for `max_concurrent`;
    /// the frozen `steady_state_concurrency` remains the *expectation* the lineage was
    /// sized against, which is a different quantity from a runtime cap.
    #[must_use]
    pub fn transient_peak_concurrency(&self) -> usize {
        self.target_m.saturating_mul(self.holding_period_sessions.saturating_add(1))
    }

    /// The number of shares a `notional_per_position` budget buys at `price` (floored),
    /// mirroring [`crate::params::OrbParams::position_qty`]. Zero at a non-positive
    /// price or when the price exceeds the notional — the caller's sizing gate then
    /// rejects the entry rather than submitting a zero-quantity order.
    #[must_use]
    pub fn position_qty(&self, price: f64) -> i64 {
        if price <= 0.0 || !price.is_finite() {
            return 0;
        }
        (self.notional_per_position / price).floor() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_set_validates_and_carries_the_frozen_steady_state() {
        let p = DailyParams::default();
        assert!(p.validate().is_ok(), "{:?}", p.validate());
        assert_eq!(p.steady_state_concurrency(), FROZEN_STEADY_STATE_CONCURRENCY);
        // The cap is the TRANSIENT PEAK, one full cohort above the steady state — not the
        // steady state itself. The strategy tests `open + pending` per bar, and within a
        // session the expiring cohort has not exited when that session's entries are
        // evaluated, so the committed count legitimately reaches `target_m × (hold + 1)`
        // before settling back. Capping at the steady state made the cap bind transiently
        // and refuse real entries in instrument-id order, under a `concurrency_cap` reason
        // claiming the take over-issued when it had not.
        assert_eq!(p.max_concurrent, p.transient_peak_concurrency());
        assert_eq!(p.max_concurrent, FROZEN_STEADY_STATE_CONCURRENCY + FROZEN_TARGET_M);
        assert!(
            p.max_concurrent > FROZEN_STEADY_STATE_CONCURRENCY,
            "a cap AT the steady state is the defect fixed in 2870a78, not the intent"
        );
    }

    /// The stop's two terms are pinned to their FREEZE, not merely to positivity.
    ///
    /// A positive-but-off-freeze value is the dangerous case: it validates, reaches the
    /// engine, and is recorded by `Manifest::new_daily` and the run observation as though it
    /// had measured the frozen lineage. The three sibling terms were already bounded; these
    /// two were not, and the asymmetry was an oversight.
    #[test]
    fn a_positive_but_off_freeze_stop_term_is_refused() {
        for (label, p) in [
            (
                "stop_atr_mult",
                DailyParams { stop_atr_mult: 2.0, ..DailyParams::default() },
            ),
            (
                "atr_window_sessions",
                DailyParams { atr_window_sessions: 14.0, ..DailyParams::default() },
            ),
        ] {
            let err = p
                .validate()
                .expect_err("a positive value off its freeze must not validate");
            assert!(err.contains(label), "the error names the offending term: {err}");
            assert!(err.contains("frozen"), "and says it is off its freeze: {err}");
        }

        // The freeze itself still validates, so this is not a blanket refusal.
        assert!(DailyParams::default().validate().is_ok());
    }

    #[test]
    fn the_sizing_term_is_not_orbs() {
        // R27: ORB's fixed notional sizes 5 concurrent positions; this path holds 128.
        assert_ne!(
            DailyParams::default().notional_per_position,
            crate::params::OrbParams::default().notional_per_position,
            "the daily sizing term must not be inherited from ORB"
        );
        // The default deploys the runner's default balance exactly once at steady state.
        assert_eq!(
            DEFAULT_NOTIONAL_PER_POSITION_KRW * FROZEN_STEADY_STATE_CONCURRENCY as f64,
            100_000_000.0
        );
    }

    #[test]
    fn position_qty_floors_and_is_zero_on_a_degenerate_price() {
        let p = DailyParams::default();
        assert_eq!(p.position_qty(1_000.0), 781);
        assert_eq!(p.position_qty(0.0), 0);
        assert_eq!(p.position_qty(-1.0), 0);
        assert_eq!(p.position_qty(f64::NAN), 0);
        // A price above the whole budget buys nothing — the qty-0 rejection, never a
        // negative quantity.
        assert_eq!(p.position_qty(DEFAULT_NOTIONAL_PER_POSITION_KRW * 2.0), 0);
    }

    #[test]
    fn a_binding_concurrency_cap_is_rejected() {
        // ORB's max_concurrent default against the frozen steady state.
        let p = DailyParams { max_concurrent: 5, ..DailyParams::default() };
        let err = p.validate().expect_err("a cap of 5 binds against 128");
        assert!(err.contains("max_concurrent"), "{err}");
        assert!(err.contains("128"), "names the implied steady state: {err}");
    }
}
