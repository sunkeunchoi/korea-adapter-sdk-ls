//! The pre-registered **sample margin** — the bar a future ORB head must clear
//! before rung-1 re-entry (plan 2026-08-05-001, U3; R6, KTD2/KTD3).
//!
//! # Why this file exists outside `candidates/` and outside `preregistration.json`
//!
//! Two containers were available and neither fits:
//!
//! - `config/preregistration.json` is the **frozen ladder** pre-registration.
//!   The amendment protocol's no-consumer test (`docs/solutions/conventions/
//!   suspend-vs-amend-frozen-governance-artifacts.md`) says not to re-derive a
//!   frozen artifact when the honest value would forbid the activity it gates.
//!   The ladder is stood down; re-deriving its bands to carry a sample margin
//!   would be exactly that. KTD3 leaves it byte-identical.
//! - `candidates/` is the Phase-A candidate registry. `candidates::load` bails
//!   on a candidate declaring neither a flip param nor a sweep-leg set, and
//!   `diagnose` short-circuits a `minimal` candidate to an immediate GO before
//!   thresholds are evaluated — so a margin filed there would never be
//!   enforced. Extending that schema means a version bump that invalidates the
//!   seven committed packages, for a record that is not a candidate.
//!
//! So the margin gets its own home: `config/sample-margin.json` (machine
//! readable) beside `config/SAMPLE-MARGIN.md` (the rationale prose).
//!
//! # Why the frozen thing is a rule, not a level
//!
//! A scalar threshold scaled at the head's own 111 trades would be unclearable
//! at **any** sample size — the failure mode that permanently strands a viable
//! strategy. What is frozen here is therefore the *rule* and its two
//! selection-bias inputs; the sampling term is evaluated against the
//! candidate's own trade count and clustering at judge time:
//!
//! ```text
//! clears  ⟺  net RoR  >  E[max of N null trials]  +  z(confidence) · SE(candidate)
//!
//! E[max]  =  σ_trials · [ (1 − γ)·Φ⁻¹(1 − 1/N)  +  γ·Φ⁻¹(1 − 1/(N·e)) ]
//! ```
//!
//! `N` and `σ_trials` are frozen (they describe the search already spent, which
//! more data cannot un-spend). `SE(candidate)` is the candidate's own
//! session-block bootstrap standard error, so it shrinks as the sample grows
//! and the bar stays reachable.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stats::{
    expected_max_null, margin_verdict, sample_sd, two_sided_z, MarginArm, MarginVerdict,
    StatsError,
};

/// The frozen margin record's filename under `config/`.
pub const MARGIN_FILE: &str = "sample-margin.json";

/// The committed margin record: `<crate>/config/sample-margin.json`, baked from
/// `CARGO_MANIFEST_DIR` so it resolves the same from any working directory.
#[must_use]
pub fn frozen_margin_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join(MARGIN_FILE)
}

/// One evaluated arm and the net RoR it produced — the per-arm figures the
/// cross-trial dispersion is computed over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossTrialArm {
    /// The arm, named as `TURN-LOG.md` names it.
    pub arm: String,
    /// Its net RoR on the recorded catalog.
    pub net_ror: f64,
}

/// Where and when the dispersion inputs were read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarginProvenance {
    /// The catalog fingerprint the figures were read at. A candidate judged on
    /// a different fingerprint triggers re-derivation.
    pub catalog_fingerprint: String,
    /// The run the dispersion came from.
    pub run_id: String,
    /// Its strategy version.
    pub strategy_version: u32,
    /// KST session span, `YYYYMMDD..YYYYMMDD`.
    pub session_span: String,
    /// Distinct KST sessions.
    pub sessions: usize,
    /// Closed trades.
    pub closed_trades: usize,
    /// Per-trade net-r mean.
    pub net_r_mean: f64,
    /// Per-trade net-r sample sd.
    pub net_r_sd: f64,
    /// Measured intra-session correlation.
    pub icc: f64,
    /// Kish unbalanced-corrected cluster size.
    pub kish_cluster_size: f64,
    /// The design effect those two imply.
    pub design_effect: f64,
}

/// The frozen margin record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleMargin {
    /// Record schema version.
    pub schema_version: u32,
    /// When the margin was frozen (UTC, RFC 3339).
    pub frozen_utc: String,
    /// The statistic the margin adjudicates.
    pub statistic: String,
    /// The rule, in prose, so a reader never has to infer it from code.
    pub rule: String,
    /// The False Strategy Theorem closed form.
    pub closed_form: String,
    /// Two-sided confidence level (KTD11's pin).
    pub confidence: f64,
    /// Statistical power (KTD11's pin; carried for provenance — the margin's
    /// own threshold is a confidence statement, power belongs to the
    /// sample-size derivation).
    pub power: f64,
    /// The number of evaluated trials the correction is taken over.
    pub trial_count: usize,
    /// How the trial count was scoped, and why.
    pub trial_count_basis: String,
    /// Records in `ledger/trials.jsonl` at freeze time.
    pub trial_ledger_records: usize,
    /// The per-arm figures the dispersion is computed over.
    pub cross_trial_arms: Vec<CrossTrialArm>,
    /// Their sample standard deviation — reproducible from `cross_trial_arms`.
    pub cross_trial_sd: f64,
    /// `E[max]` at `trial_count` and `cross_trial_sd` — reproducible from both.
    pub expected_max_null: f64,
    /// Where the inputs were read.
    pub provenance: MarginProvenance,
    /// The condition under which this margin must be re-derived before binding.
    pub rederivation_trigger: String,
}

/// A loaded margin plus the SHA-256 of the exact bytes it came from, so a
/// verdict can cite the file it was adjudicated against.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedMargin {
    /// The parsed record.
    pub values: SampleMargin,
    /// Hex SHA-256 of the file's bytes.
    pub content_hash: String,
}

/// Read and parse the frozen margin record.
///
/// # Errors
///
/// On I/O failure or a parse error, both naming the path.
pub fn load(path: &Path) -> anyhow::Result<LoadedMargin> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("reading margin {}: {e}", path.display()))?;
    let values: SampleMargin = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("parsing margin {}: {e}", path.display()))?;
    let content_hash = format!("{:x}", Sha256::digest(&bytes));
    Ok(LoadedMargin { values, content_hash })
}

/// Whether a candidate needs the margin re-derived before it binds: the margin
/// is calibrated against one catalog, and in-range content growth changes the
/// trade set (AE3), so a differing fingerprint invalidates the calibration
/// rather than merely aging it.
#[must_use]
pub fn requires_rederivation(margin: &SampleMargin, candidate_fingerprint: &str) -> bool {
    margin.provenance.catalog_fingerprint != candidate_fingerprint
}

impl SampleMargin {
    /// The cross-trial dispersion re-derived from the recorded per-arm figures,
    /// so the frozen `cross_trial_sd` is auditable rather than typed in.
    ///
    /// # Errors
    ///
    /// Propagates [`sample_sd`] on fewer than two recorded arms.
    pub fn derived_cross_trial_sd(&self) -> Result<f64, StatsError> {
        let arms: Vec<f64> = self.cross_trial_arms.iter().map(|a| a.net_ror).collect();
        sample_sd(&arms)
    }

    /// `E[max]` re-derived from the recorded trial count and dispersion.
    ///
    /// # Errors
    ///
    /// Propagates [`expected_max_null`].
    pub fn derived_expected_max_null(&self) -> Result<f64, StatsError> {
        expected_max_null(self.trial_count, self.cross_trial_sd)
    }

    /// The threshold a candidate with bootstrap standard error
    /// `candidate_standard_error` must exceed.
    ///
    /// # Errors
    ///
    /// Propagates [`expected_max_null`] and [`two_sided_z`].
    pub fn threshold(&self, candidate_standard_error: f64) -> Result<f64, StatsError> {
        Ok(self.derived_expected_max_null()?
            + two_sided_z(self.confidence)? * candidate_standard_error)
    }

    /// Adjudicate one candidate. `arm` is [`MarginArm::Armed`] everywhere except
    /// the U4 falsifier, which disarms it in-process to prove the comparison is
    /// load-bearing.
    ///
    /// # Errors
    ///
    /// Propagates [`Self::threshold`].
    pub fn adjudicate(
        &self,
        candidate_net_ror: f64,
        candidate_standard_error: f64,
        arm: MarginArm,
    ) -> Result<MarginVerdict, StatsError> {
        Ok(margin_verdict(candidate_net_ror, self.threshold(candidate_standard_error)?, arm))
    }
}
