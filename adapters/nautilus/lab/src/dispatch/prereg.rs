//! The pre-registration values store (U4, KTD9) — the machine-readable mirror of the
//! human pre-registration document that freezes the ladder's numbers.
//!
//! A versioned, content-hashed JSON carries rung fractions, N per rung, the readiness
//! window K, exceedance thresholds, per-rung tracking-error and economic-expectation
//! bands, the watchdog heartbeat interval and session max-loss threshold, the rung-0
//! re-qualification terms, and the head-change rules. Every dispatch record cites the
//! content hash it ran under.
//!
//! **Fail-closed exactly where a value is load-bearing** (KTD9). Loading a present file
//! never fails; the accessors fail when a value that is load-bearing for the active
//! phase and rung is absent — a missing heartbeat interval blocks the watchdog from
//! arming (U7), a missing rung-2 band blocks a rung-2 dispatch but not a rung-1 one
//! (KD6, U10). Phase 1 needs none of these, so an absent file is fine there.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::artifacts::manifest::hash_bytes;

/// The frozen tolerance on paper-vs-live divergence for a rung (size-normalized units).
/// Load-bearing from rung 2 (KD6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackingBand {
    /// Max per-share slippage (size-normalized) before a band breach (R14(c)).
    pub max_slippage_per_share: f64,
    /// Max approximated-fill fraction before a band breach.
    pub max_approximated_fraction: f64,
}

/// The per-rung economic expectation band derived from the backtest distribution
/// (R14(e)) — cumulative P&L outside it is a limit event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpectationBand {
    /// The lower bound of acceptable cumulative P&L at this rung.
    pub min_cum_pnl: f64,
    /// The upper bound (a runaway is also worth a second look).
    pub max_cum_pnl: f64,
}

/// One rung's frozen spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RungSpec {
    /// The rung number (1–4).
    pub rung: u8,
    /// The budget-numerator fraction (KTD6). Load-bearing to run at this rung.
    #[serde(default)]
    pub fraction: Option<f64>,
    /// N clean sessions required to escalate FROM this rung (R13). Load-bearing to
    /// escalate.
    #[serde(default)]
    pub n_clean_sessions: Option<u32>,
    /// The tracking-error band (R14(c)). Load-bearing from rung 2 (KD6).
    #[serde(default)]
    pub tracking_band: Option<TrackingBand>,
    /// The economic expectation band (R14(e)). Load-bearing to escalate from this rung.
    #[serde(default)]
    pub expectation_band: Option<ExpectationBand>,
}

/// The exceedance thresholds the readiness reducer trends against (R10/R11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExceedanceThresholds {
    /// Max reconcile-advised conditions in the window before red.
    #[serde(default)]
    pub max_reconcile_advised: Option<u32>,
    /// Max per-check deferrals in the window before red.
    #[serde(default)]
    pub max_deferrals: Option<u32>,
    /// Max coverage-gap conditions in the window before red.
    #[serde(default)]
    pub max_coverage_gaps: Option<u32>,
}

/// The head-change rules (R13): how the ladder responds to a strategy identity change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadChangeRules {
    /// Whether a params-only change re-runs the current rung's N (default true).
    #[serde(default = "default_true")]
    pub params_change_reruns_n: bool,
    /// Whether a strategy-code-hash change returns the ladder to rung 1 (default true).
    #[serde(default = "default_true")]
    pub code_change_resets_to_rung_1: bool,
}

fn default_true() -> bool {
    true
}

impl Default for HeadChangeRules {
    fn default() -> Self {
        HeadChangeRules { params_change_reruns_n: true, code_change_resets_to_rung_1: true }
    }
}

/// The pre-registered values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreRegistration {
    /// Schema version.
    pub version: u32,
    /// Per-rung specs (rungs 1–4).
    #[serde(default)]
    pub rungs: Vec<RungSpec>,
    /// The readiness trailing-window size K (live-lane sessions).
    #[serde(default)]
    pub k_window: Option<u32>,
    /// Exceedance thresholds.
    #[serde(default)]
    pub exceedance: ExceedanceThresholds,
    /// The watchdog dead-man heartbeat interval (seconds). Load-bearing to arm (U7).
    #[serde(default)]
    pub heartbeat_interval_secs: Option<u64>,
    /// The session max-loss breaker threshold (KRW). Load-bearing to arm (U7).
    #[serde(default)]
    pub session_max_loss_krw: Option<f64>,
    /// The rung-0 re-qualification terms (free text, mirrored from the human doc).
    #[serde(default)]
    pub rung0_requalification: Option<String>,
    /// The head-change rules.
    #[serde(default)]
    pub head_change: HeadChangeRules,
}

impl Default for ExceedanceThresholds {
    fn default() -> Self {
        ExceedanceThresholds { max_reconcile_advised: None, max_deferrals: None, max_coverage_gaps: None }
    }
}

/// A loaded pre-registration file plus the content hash records cite (KTD9).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPreReg {
    /// The parsed values.
    pub values: PreRegistration,
    /// SHA-256 hex of the raw file bytes — the citation each dispatch records.
    pub content_hash: String,
}

/// Load a pre-registration file, returning its values and content hash. A present file
/// that parses never fails; malformed JSON is an error.
///
/// # Errors
///
/// If the file is absent or malformed.
pub fn load(path: &Path) -> anyhow::Result<LoadedPreReg> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("reading pre-registration {}: {e}", path.display()))?;
    let content_hash = hash_bytes(&bytes);
    let values: PreRegistration = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("parsing pre-registration {}: {e}", path.display()))?;
    Ok(LoadedPreReg { values, content_hash })
}

/// Load a pre-registration file if it exists (phase 1 needs none, so absence is fine).
///
/// # Errors
///
/// If the file exists but is malformed.
pub fn load_optional(path: &Path) -> anyhow::Result<Option<LoadedPreReg>> {
    if path.exists() {
        Ok(Some(load(path)?))
    } else {
        Ok(None)
    }
}

impl PreRegistration {
    fn rung_spec(&self, rung: u8) -> Option<&RungSpec> {
        self.rungs.iter().find(|r| r.rung == rung)
    }

    /// The budget-numerator fraction for `rung` (KTD6). Fail-closed if absent — a rung
    /// cannot run at an unfrozen size.
    ///
    /// # Errors
    ///
    /// If the rung's fraction is not pre-registered.
    pub fn rung_fraction(&self, rung: u8) -> anyhow::Result<f64> {
        self.rung_spec(rung)
            .and_then(|r| r.fraction)
            .ok_or_else(|| anyhow::anyhow!("pre-registration missing rung {rung} fraction (load-bearing to run)"))
    }

    /// N clean sessions to escalate from `rung` (R13). Fail-closed if absent.
    ///
    /// # Errors
    ///
    /// If N for the rung is not pre-registered.
    pub fn n_for_rung(&self, rung: u8) -> anyhow::Result<u32> {
        self.rung_spec(rung)
            .and_then(|r| r.n_clean_sessions)
            .ok_or_else(|| anyhow::anyhow!("pre-registration missing rung {rung} N (load-bearing to escalate)"))
    }

    /// The tracking-error band for `rung` (R14(c)). Rung 1 has no band (KD6), so this
    /// returns `Ok(None)` for rung ≤ 1; from rung 2 it is load-bearing and fail-closed.
    ///
    /// # Errors
    ///
    /// If `rung ≥ 2` and no band is pre-registered.
    pub fn tracking_band(&self, rung: u8) -> anyhow::Result<Option<&TrackingBand>> {
        if rung <= 1 {
            return Ok(None); // rung 1 is calibration; the band is not load-bearing (KD6)
        }
        let spec = self
            .rung_spec(rung)
            .ok_or_else(|| anyhow::anyhow!("pre-registration has no rung {rung} spec"))?;
        spec.tracking_band
            .as_ref()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!(
                "pre-registration missing rung {rung} tracking-error band (load-bearing from rung 2, KD6)"
            ))
    }

    /// The economic expectation band for `rung` (R14(e)). Load-bearing to escalate.
    ///
    /// # Errors
    ///
    /// If the rung's expectation band is not pre-registered.
    pub fn expectation_band(&self, rung: u8) -> anyhow::Result<&ExpectationBand> {
        self.rung_spec(rung)
            .and_then(|r| r.expectation_band.as_ref())
            .ok_or_else(|| anyhow::anyhow!("pre-registration missing rung {rung} expectation band (load-bearing)"))
    }

    /// The watchdog heartbeat interval (seconds). Load-bearing to arm the envelope (U7).
    ///
    /// # Errors
    ///
    /// If the interval is not pre-registered.
    pub fn heartbeat_interval_secs(&self) -> anyhow::Result<u64> {
        self.heartbeat_interval_secs
            .ok_or_else(|| anyhow::anyhow!("pre-registration missing heartbeat interval — the watchdog cannot arm"))
    }

    /// The session max-loss breaker threshold (KRW). Load-bearing to arm the breaker (U7).
    ///
    /// # Errors
    ///
    /// If the threshold is not pre-registered.
    pub fn session_max_loss_krw(&self) -> anyhow::Result<f64> {
        self.session_max_loss_krw
            .ok_or_else(|| anyhow::anyhow!("pre-registration missing session max-loss threshold — the breaker cannot arm"))
    }

    /// The readiness trailing-window K. Load-bearing to compute the verdict (U9).
    ///
    /// # Errors
    ///
    /// If K is not pre-registered.
    pub fn k_window(&self) -> anyhow::Result<u32> {
        self.k_window
            .ok_or_else(|| anyhow::anyhow!("pre-registration missing readiness window K (load-bearing)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, json: serde_json::Value) -> std::path::PathBuf {
        let p = dir.path().join(name);
        std::fs::write(&p, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        p
    }

    #[test]
    fn absent_file_is_none_phase_1_needs_nothing() {
        let tmp = TempDir::new().unwrap();
        assert!(load_optional(&tmp.path().join("absent.json")).unwrap().is_none());
    }

    #[test]
    fn missing_heartbeat_interval_blocks_arming() {
        let tmp = TempDir::new().unwrap();
        let p = write(&tmp, "prereg.json", serde_json::json!({ "version": 1 }));
        let loaded = load(&p).unwrap();
        assert!(loaded.values.heartbeat_interval_secs().is_err(), "missing heartbeat blocks the watchdog");
    }

    #[test]
    fn rung_1_needs_no_band_but_rung_2_does() {
        let tmp = TempDir::new().unwrap();
        let p = write(
            &tmp,
            "prereg.json",
            serde_json::json!({
                "version": 1,
                "rungs": [
                    { "rung": 1, "fraction": 0.1 },
                    { "rung": 2, "fraction": 0.25 }
                ]
            }),
        );
        let v = load(&p).unwrap().values;
        // Rung 1: no band is fine (calibration, KD6).
        assert!(v.tracking_band(1).unwrap().is_none());
        // Rung 2: a missing band is fail-closed.
        assert!(v.tracking_band(2).is_err());
    }

    #[test]
    fn content_hash_tracks_the_exact_file_bytes() {
        let tmp = TempDir::new().unwrap();
        let p1 = write(&tmp, "a.json", serde_json::json!({ "version": 1 }));
        let p2 = write(&tmp, "b.json", serde_json::json!({ "version": 2 }));
        let h1 = load(&p1).unwrap().content_hash;
        let h2 = load(&p2).unwrap().content_hash;
        assert_ne!(h1, h2, "editing the file changes the citation");
        // Same bytes -> same hash (idempotent citation).
        assert_eq!(h1, load(&p1).unwrap().content_hash);
    }

    #[test]
    fn present_bands_and_fractions_read_back() {
        let tmp = TempDir::new().unwrap();
        let p = write(
            &tmp,
            "prereg.json",
            serde_json::json!({
                "version": 1,
                "k_window": 5,
                "heartbeat_interval_secs": 30,
                "session_max_loss_krw": 500000.0,
                "rungs": [
                    { "rung": 1, "fraction": 0.1, "n_clean_sessions": 5,
                      "expectation_band": { "min_cum_pnl": -100.0, "max_cum_pnl": 1000.0 } },
                    { "rung": 2, "fraction": 0.25, "n_clean_sessions": 5,
                      "tracking_band": { "max_slippage_per_share": 2.0, "max_approximated_fraction": 0.2 },
                      "expectation_band": { "min_cum_pnl": 0.0, "max_cum_pnl": 5000.0 } }
                ]
            }),
        );
        let v = load(&p).unwrap().values;
        assert_eq!(v.rung_fraction(1).unwrap(), 0.1);
        assert_eq!(v.n_for_rung(2).unwrap(), 5);
        assert_eq!(v.k_window().unwrap(), 5);
        assert_eq!(v.heartbeat_interval_secs().unwrap(), 30);
        assert!(v.tracking_band(2).unwrap().is_some());
        assert_eq!(v.expectation_band(1).unwrap().max_cum_pnl, 1000.0);
    }
}
