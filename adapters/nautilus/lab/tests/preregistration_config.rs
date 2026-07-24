//! Guard tests for the FROZEN pre-registration mirror (`config/preregistration.json`).
//!
//! These pin the DoD contract: the shipped file loads with NO fail-closed bail for rung 1,
//! the watchdog can arm from it, and the backtest-grounded values match
//! `config/PREREGISTRATION.md`. A rung-2 tracking band stays fail-closed BY DESIGN (KD6) —
//! scheduled to freeze from rung-1 live data — so that arm is asserted too. If a future
//! edit drifts a load-bearing rung-1 value out, this reddens instead of surfacing live.

use std::path::PathBuf;

use nautilus_ls_lab::dispatch::prereg::load;
use nautilus_ls_lab::runner::watchdog::WatchdogLimits;

fn frozen_prereg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config").join("preregistration.json")
}

#[test]
fn frozen_config_loads_and_rung_1_is_not_fail_closed() {
    let loaded = load(&frozen_prereg_path()).expect("frozen preregistration.json parses");
    let v = &loaded.values;

    // Rung 1 is fully runnable: fraction + N + expectation band all present (no bail).
    assert_eq!(v.rung_fraction(1).unwrap(), 0.10, "rung-1 dose (KTD6)");
    assert_eq!(v.n_for_rung(1).unwrap(), 5, "N clean sessions to escalate rung 1");
    // Re-registered to v2 (head v34): economic bands are v34-derived (see prereg_derivation.rs).
    let band = v.expectation_band(1).unwrap();
    assert_eq!(band.min_cum_pnl, -148_000.0);
    assert_eq!(band.max_cum_pnl, 266_000.0);

    // Rung 1 carries NO tracking band by design (calibration, KD6) — Ok(None), not an error.
    assert!(v.tracking_band(1).unwrap().is_none(), "rung 1 has no tracking band (KD6)");

    // The readiness window is frozen.
    assert_eq!(v.k_window().unwrap(), 5);

    // A non-empty citation hash is produced.
    assert_eq!(loaded.content_hash.len(), 64, "SHA-256 hex citation");
}

#[test]
fn watchdog_arms_from_the_frozen_config() {
    let loaded = load(&frozen_prereg_path()).unwrap();
    let limits = WatchdogLimits::from_prereg(&loaded.values).expect("watchdog arms (KTD9/U7)");
    assert_eq!(limits.heartbeat_interval_secs, 90, "generous dead-man window (KD5)");
    assert_eq!(limits.max_loss_krw, 300_000.0, "rung-1-tight breaker");
}

#[test]
fn expectation_bands_scale_by_the_rung_fraction() {
    // The Protective method (head v34): floor = -1_483_240 * fraction,
    // ceil = 1_772_900 * 1.5 * fraction (rounded to the nearest 1,000). Confirm rungs 2-4 track
    // the same backtest basis. The full per-band derivation lives in prereg_derivation.rs.
    let v = load(&frozen_prereg_path()).unwrap().values;
    for (rung, frac, min, max) in [
        (2u8, 0.25, -371_000.0, 665_000.0),
        (3u8, 0.50, -742_000.0, 1_330_000.0),
        (4u8, 1.00, -1_483_000.0, 2_659_000.0),
    ] {
        assert_eq!(v.rung_fraction(rung).unwrap(), frac, "rung {rung} dose");
        let band = v.expectation_band(rung).unwrap();
        assert_eq!(band.min_cum_pnl, min, "rung {rung} band floor");
        assert_eq!(band.max_cum_pnl, max, "rung {rung} band ceiling");
    }
}

#[test]
fn rung_2_tracking_band_is_fail_closed_by_design() {
    // Scheduled, not frozen now (KD3/KD6): the backtest has zero slippage, so a rung-2+
    // tracking band is intentionally absent and must fail closed — blocking a rung-2
    // dispatch until it is re-registered from rung-1 LIVE data.
    let v = load(&frozen_prereg_path()).unwrap().values;
    assert!(v.tracking_band(2).is_err(), "rung-2 tracking band is fail-closed until re-registered");
}
