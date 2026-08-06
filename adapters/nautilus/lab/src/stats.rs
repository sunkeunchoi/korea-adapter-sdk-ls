//! Sample-sufficiency statistics — the pure core behind `report sample`
//! (plan 2026-08-05-001, U1; R1–R3, KTD4/KTD5/KTD11).
//!
//! Every function here takes slices and returns plain `f64`s. There is no I/O,
//! no artifact type, and no clock, so `tests/stats_derivation.rs` can assert
//! against hand-computed constants rather than snapshotting the implementation's
//! own output (the `prereg_derivation.rs` discipline).
//!
//! Three things this module exists to prevent:
//!
//! - **Counting clustered trades as independent observations.** Trades cluster
//!   inside a KST session, so the naive `n` overstates information. Every
//!   sample-size answer here runs through a [`design_effect`], never the raw
//!   count (R1).
//! - **Choosing the multiplier once the answer is visible.** Target effect,
//!   confidence, and power are explicit named parameters on every entry point
//!   (KTD11) — there is no place to quietly retune one.
//! - **Reading a bare sign test as evidence.** [`trials_corrected_threshold`]
//!   inflates the bar by the expected maximum of `N` null trials (the False
//!   Strategy Theorem), so a head selected out of a lever search must beat the
//!   luck its own search bought (KTD2).
//!
//! Costs are **not** this module's business: it consumes whatever series it is
//! handed. KTD4 (denominate in the net, cost-aware distribution) is discharged
//! by the caller passing net figures — see `runner::report::report_sample`.

use std::f64::consts::E;

/// Euler–Mascheroni constant, γ — the False Strategy Theorem's mixing weight.
pub const EULER_MASCHERONI: f64 = 0.577_215_664_901_532_9;

/// A refusal from the statistics core. Every degenerate input fails closed with
/// a named reason rather than returning a number that reads as an answer.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StatsError {
    /// The series carried no observations.
    #[error("empty series: {what} needs at least {need} observation(s), got 0")]
    Empty {
        /// The statistic that was asked for.
        what: &'static str,
        /// How many observations it needs.
        need: usize,
    },
    /// The series was too short for the statistic.
    #[error("series too short: {what} needs at least {need} observation(s), got {got}")]
    TooShort {
        /// The statistic that was asked for.
        what: &'static str,
        /// How many observations it needs.
        need: usize,
        /// How many it got.
        got: usize,
    },
    /// A parameter was outside its admissible domain.
    #[error("{what} must be {expected}, got {got}")]
    Domain {
        /// The parameter name.
        what: &'static str,
        /// The admissible domain, in prose.
        expected: &'static str,
        /// What was actually supplied.
        got: String,
    },
    /// Two parallel slices disagreed in length.
    #[error("{a} has {a_len} entries but {b} has {b_len} — they must be parallel")]
    Mismatched {
        /// First slice's name.
        a: &'static str,
        /// First slice's length.
        a_len: usize,
        /// Second slice's name.
        b: &'static str,
        /// Second slice's length.
        b_len: usize,
    },
}

fn domain(what: &'static str, expected: &'static str, got: f64) -> StatsError {
    StatsError::Domain { what, expected, got: format!("{got}") }
}

fn require_finite_positive(what: &'static str, x: f64) -> Result<(), StatsError> {
    if !x.is_finite() || x <= 0.0 {
        return Err(domain(what, "finite and strictly positive", x));
    }
    Ok(())
}

// ===========================================================================
// Location and dispersion
// ===========================================================================

/// Arithmetic mean. An empty series is a refusal, not `0.0` / `NaN`.
///
/// # Errors
///
/// [`StatsError::Empty`] on an empty series.
pub fn mean(xs: &[f64]) -> Result<f64, StatsError> {
    if xs.is_empty() {
        return Err(StatsError::Empty { what: "mean", need: 1 });
    }
    Ok(xs.iter().sum::<f64>() / xs.len() as f64)
}

/// Sample standard deviation (Bessel-corrected, `n − 1`). A single observation
/// has no dispersion to report, so it is a refusal rather than `0.0`.
///
/// # Errors
///
/// [`StatsError::Empty`] / [`StatsError::TooShort`] below two observations.
pub fn sample_sd(xs: &[f64]) -> Result<f64, StatsError> {
    match xs.len() {
        0 => Err(StatsError::Empty { what: "sample standard deviation", need: 2 }),
        1 => Err(StatsError::TooShort {
            what: "sample standard deviation",
            need: 2,
            got: 1,
        }),
        n => {
            let m = mean(xs)?;
            let ss: f64 = xs.iter().map(|x| (x - m) * (x - m)).sum();
            Ok((ss / (n - 1) as f64).sqrt())
        }
    }
}

/// Nearest-rank percentile over an already-sorted slice (`pct` in `0..=100`).
/// Mirrors `runner::report::nearest_rank`, which is private to that module.
#[must_use]
pub fn percentile(sorted: &[f64], pct: f64) -> Option<f64> {
    if sorted.is_empty() || !(0.0..=100.0).contains(&pct) {
        return None;
    }
    let rank = (pct / 100.0 * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted.get(rank.min(sorted.len()) - 1).copied()
}

// ===========================================================================
// Clustering — the design effect (R1, R3)
// ===========================================================================

/// The measured clustering of a series, and the effective sample it implies.
#[derive(Debug, Clone, PartialEq)]
pub struct Clustering {
    /// Observations.
    pub n: usize,
    /// Distinct clusters (KST sessions, for this turn).
    pub clusters: usize,
    /// Naive mean cluster size, `n / clusters` — the reporting figure.
    pub mean_cluster_size: f64,
    /// Kish's unbalanced-corrected mean cluster size,
    /// `(n − Σ mᵍ² / n) / (clusters − 1)` — the figure the design effect uses,
    /// because unequal cluster sizes cost more information than the naive mean
    /// admits.
    pub kish_cluster_size: f64,
    /// One-way random-effects intra-cluster correlation (ANOVA estimator),
    /// clamped to `[0, 1]`: a negative point estimate means "no detectable
    /// between-cluster variance", which is a design effect of 1, not of less
    /// than 1.
    pub icc: f64,
    /// `1 + (kish_cluster_size − 1) · icc`.
    pub design_effect: f64,
    /// `n / design_effect`.
    pub effective_n: f64,
}

/// Measure clustering for `values` grouped by the parallel `cluster_ids`.
///
/// The ICC is the one-way random-effects ANOVA estimator
/// `(MSB − MSW) / (MSB + (m₀ − 1)·MSW)`, and the design effect is Kish's
/// `1 + (m₀ − 1)·ρ` at the unbalanced-corrected cluster size `m₀`. A single
/// cluster cannot separate between- from within-cluster variance, so it is a
/// refusal (R3's fragility is reported, never silently absorbed).
///
/// # Errors
///
/// Refuses an empty series, a series of one, a single cluster, and mismatched
/// slice lengths.
pub fn clustering(values: &[f64], cluster_ids: &[usize]) -> Result<Clustering, StatsError> {
    if values.len() != cluster_ids.len() {
        return Err(StatsError::Mismatched {
            a: "values",
            a_len: values.len(),
            b: "cluster_ids",
            b_len: cluster_ids.len(),
        });
    }
    if values.is_empty() {
        return Err(StatsError::Empty { what: "clustering", need: 2 });
    }
    if values.len() < 2 {
        return Err(StatsError::TooShort { what: "clustering", need: 2, got: values.len() });
    }

    let mut groups: std::collections::BTreeMap<usize, Vec<f64>> = std::collections::BTreeMap::new();
    for (v, c) in values.iter().zip(cluster_ids) {
        groups.entry(*c).or_default().push(*v);
    }
    let k = groups.len();
    if k < 2 {
        return Err(StatsError::TooShort { what: "clustering (distinct clusters)", need: 2, got: k });
    }
    let n = values.len();
    if n == k {
        // Every cluster holds one observation: no within-cluster variance is
        // estimable, and no information is lost to clustering. Design effect 1
        // by construction (not by a divide-by-zero).
        return Ok(Clustering {
            n,
            clusters: k,
            mean_cluster_size: 1.0,
            kish_cluster_size: 1.0,
            icc: 0.0,
            design_effect: 1.0,
            effective_n: n as f64,
        });
    }

    let grand = mean(values)?;
    let msb: f64 = groups
        .values()
        .map(|g| {
            let gm = g.iter().sum::<f64>() / g.len() as f64;
            g.len() as f64 * (gm - grand) * (gm - grand)
        })
        .sum::<f64>()
        / (k - 1) as f64;
    let msw: f64 = groups
        .values()
        .map(|g| {
            let gm = g.iter().sum::<f64>() / g.len() as f64;
            g.iter().map(|x| (x - gm) * (x - gm)).sum::<f64>()
        })
        .sum::<f64>()
        / (n - k) as f64;

    let sum_sq: f64 = groups.values().map(|g| (g.len() * g.len()) as f64).sum();
    let kish = (n as f64 - sum_sq / n as f64) / (k - 1) as f64;

    let denom = msb + (kish - 1.0) * msw;
    let icc = if denom.abs() < f64::EPSILON { 0.0 } else { ((msb - msw) / denom).clamp(0.0, 1.0) };
    let deff = design_effect(kish, icc);

    Ok(Clustering {
        n,
        clusters: k,
        mean_cluster_size: n as f64 / k as f64,
        kish_cluster_size: kish,
        icc,
        design_effect: deff,
        effective_n: n as f64 / deff,
    })
}

/// Kish's design effect, `1 + (m − 1)·ρ`. Floored at 1.0: clustering can only
/// cost information here, never manufacture it.
#[must_use]
pub fn design_effect(mean_cluster_size: f64, icc: f64) -> f64 {
    (1.0 + (mean_cluster_size - 1.0) * icc).max(1.0)
}

// ===========================================================================
// Normal and Student-t quantiles
// ===========================================================================

/// Inverse standard-normal CDF (Wichura's AS241 `PPND16`), accurate to ~1e-16
/// across the whole open unit interval. Hand-rolled rather than pulled in as a
/// dependency: the standalone adapter workspace is pinned to nautilus's
/// toolchain and every added crate is a lockstep liability.
///
/// # Errors
///
/// [`StatsError::Domain`] outside the open interval `(0, 1)`.
#[allow(clippy::excessive_precision)]
pub fn probit(p: f64) -> Result<f64, StatsError> {
    if !p.is_finite() || p <= 0.0 || p >= 1.0 {
        return Err(domain("probability", "strictly inside (0, 1)", p));
    }
    let q = p - 0.5;
    if q.abs() <= 0.425 {
        let r = 0.180_625 - q * q;
        let num = ((((((2.509_080_928_730_122_7e3 * r + 3.343_057_558_358_812_8e4) * r
            + 6.726_577_092_700_870e4)
            * r
            + 4.592_195_393_154_987e4)
            * r
            + 1.373_169_376_550_946_1e4)
            * r
            + 1.971_590_950_306_551_4e3)
            * r
            + 1.331_416_678_917_843_8e2)
            * r
            + 3.387_132_872_796_366_6;
        let den = ((((((5.226_495_278_852_854_6e3 * r + 2.872_908_573_572_194_3e4) * r
            + 3.930_789_580_009_271e4)
            * r
            + 2.121_379_430_158_659_6e4)
            * r
            + 5.394_196_021_424_751e3)
            * r
            + 6.871_870_074_920_579e2)
            * r
            + 4.231_333_070_160_091e1)
            * r
            + 1.0;
        return Ok(q * num / den);
    }
    let tail = if q < 0.0 { p } else { 1.0 - p };
    let mut r = (-tail.ln()).sqrt();
    let v = if r <= 5.0 {
        r -= 1.6;
        let num = ((((((7.745_450_142_783_414e-4 * r + 0.022_723_844_989_269_184) * r
            + 0.241_780_725_177_450_61)
            * r
            + 1.270_458_252_452_368_4)
            * r
            + 3.647_848_324_763_204_6)
            * r
            + 5.769_497_221_460_691)
            * r
            + 4.630_337_846_156_545)
            * r
            + 1.423_437_110_749_683_6;
        let den = ((((((1.050_750_071_644_416_8e-9 * r + 5.475_938_084_995_345e-4) * r
            + 0.015_198_666_563_616_457)
            * r
            + 0.148_103_976_427_480_07)
            * r
            + 0.689_767_334_985_100)
            * r
            + 1.676_384_830_183_803_8)
            * r
            + 2.053_191_626_637_758_8)
            * r
            + 1.0;
        num / den
    } else {
        r -= 5.0;
        let num = ((((((2.010_334_399_292_288e-7 * r + 2.711_555_568_743_487_6e-5) * r
            + 0.001_242_660_947_388_078_4)
            * r
            + 0.026_532_189_526_576_124)
            * r
            + 0.296_560_571_828_504_9)
            * r
            + 1.784_826_539_917_291_3)
            * r
            + 5.463_784_911_164_114)
            * r
            + 6.657_904_643_501_104;
        let den = ((((((2.044_263_103_389_939_8e-15 * r + 1.421_511_758_316_446e-7) * r
            + 1.846_318_317_510_054_7e-5)
            * r
            + 7.868_691_311_456_133e-4)
            * r
            + 0.014_875_361_290_850_615)
            * r
            + 0.136_929_880_922_735_8)
            * r
            + 0.599_832_206_555_888)
            * r
            + 1.0;
        num / den
    };
    Ok(if q < 0.0 { -v } else { v })
}

/// Student-t quantile via the Cornish–Fisher expansion of the normal quantile
/// (four correction terms). Accurate to better than 1e-5 for `df ≥ 10`, which
/// is the whole range this turn uses it in — the few-cluster critical value at
/// `G − 1 = 23` degrees of freedom (KTD5). It is a **diagnostic** critical
/// value, never the gate.
///
/// # Errors
///
/// [`StatsError::Domain`] on a probability outside `(0, 1)` or `df < 1`.
pub fn t_quantile(p: f64, df: f64) -> Result<f64, StatsError> {
    if !df.is_finite() || df < 1.0 {
        return Err(domain("degrees of freedom", "finite and at least 1", df));
    }
    let z = probit(p)?;
    let z2 = z * z;
    let g1 = (z2 * z + z) / 4.0;
    let g2 = (5.0 * z2 * z2 * z + 16.0 * z2 * z + 3.0 * z) / 96.0;
    let g3 = (3.0 * z2 * z2 * z2 * z + 19.0 * z2 * z2 * z + 17.0 * z2 * z - 15.0 * z) / 384.0;
    let g4 = (79.0 * z2 * z2 * z2 * z2 * z + 776.0 * z2 * z2 * z2 * z + 1482.0 * z2 * z2 * z
        - 1920.0 * z2 * z
        - 945.0 * z)
        / 92_160.0;
    Ok(z + g1 / df + g2 / (df * df) + g3 / df.powi(3) + g4 / df.powi(4))
}

/// The two-sided critical value at `confidence` (e.g. `0.95 → 1.959964`).
///
/// # Errors
///
/// [`StatsError::Domain`] outside `(0, 1)`.
pub fn two_sided_z(confidence: f64) -> Result<f64, StatsError> {
    if !confidence.is_finite() || confidence <= 0.0 || confidence >= 1.0 {
        return Err(domain("confidence", "strictly inside (0, 1)", confidence));
    }
    probit(1.0 - (1.0 - confidence) / 2.0)
}

/// The one-sided critical value at `power` (e.g. `0.80 → 0.841621`).
///
/// # Errors
///
/// [`StatsError::Domain`] outside `(0, 1)`.
pub fn power_z(power: f64) -> Result<f64, StatsError> {
    if !power.is_finite() || power <= 0.0 || power >= 1.0 {
        return Err(domain("power", "strictly inside (0, 1)", power));
    }
    probit(power)
}

// ===========================================================================
// Detectability (R1, R2, KTD11)
// ===========================================================================

/// The smallest per-observation effect a sample of `effective_n` can
/// distinguish from zero: `(z_{1−α/2} + z_{power}) · sd / √effective_n`.
///
/// `effective_n` is **already** design-effect deflated — pass
/// [`Clustering::effective_n`], never the raw trade count.
///
/// # Errors
///
/// [`StatsError::Domain`] on a non-positive `sd` or `effective_n`, or a
/// confidence/power outside `(0, 1)`.
pub fn minimum_detectable_edge(
    sd: f64,
    effective_n: f64,
    confidence: f64,
    power: f64,
) -> Result<f64, StatsError> {
    require_finite_positive("standard deviation", sd)?;
    require_finite_positive("effective n", effective_n)?;
    let z = two_sided_z(confidence)? + power_z(power)?;
    Ok(z * sd / effective_n.sqrt())
}

/// The closed-trade count at which `target_effect` becomes distinguishable from
/// zero: the naive two-sided sample size `((z_{1−α/2} + z_power)·sd/δ)²`,
/// **multiplied by the design effect**. Required n is never the naive count
/// alone (R1) — that is the error this whole module exists to make impossible.
///
/// Returns a real-valued count; round up at the reporting seam.
///
/// # Errors
///
/// [`StatsError::Domain`] on a non-positive `sd`, a zero/negative
/// `target_effect`, a design effect below 1, or a confidence/power outside
/// `(0, 1)`.
pub fn required_trades(
    target_effect: f64,
    sd: f64,
    design_effect: f64,
    confidence: f64,
    power: f64,
) -> Result<f64, StatsError> {
    require_finite_positive("standard deviation", sd)?;
    require_finite_positive("target effect", target_effect)?;
    if !design_effect.is_finite() || design_effect < 1.0 {
        return Err(domain("design effect", "finite and at least 1", design_effect));
    }
    let z = two_sided_z(confidence)? + power_z(power)?;
    let naive = (z * sd / target_effect).powi(2);
    Ok(naive * design_effect)
}

// ===========================================================================
// The trials-corrected threshold (R6, KTD2)
// ===========================================================================

/// The expected maximum of `n_trials` independent null draws of a statistic
/// whose cross-trial dispersion is `cross_trial_sd` — Bailey & López de Prado's
/// False Strategy Theorem:
///
/// ```text
/// E[max] = σ · [ (1 − γ)·Φ⁻¹(1 − 1/N) + γ·Φ⁻¹(1 − 1/(N·e)) ]
/// ```
///
/// At `N = 1` there is no selection to correct for, so this is exactly `0.0`
/// (the closed form is undefined there — `Φ⁻¹(0)` diverges — and *reducing to
/// the single-trial case* is the honest reading, not a diverging bar).
///
/// # Errors
///
/// [`StatsError::Domain`] on `n_trials == 0` or a negative `cross_trial_sd`.
pub fn expected_max_null(n_trials: usize, cross_trial_sd: f64) -> Result<f64, StatsError> {
    if n_trials == 0 {
        return Err(domain("trial count", "at least 1", 0.0));
    }
    if !cross_trial_sd.is_finite() || cross_trial_sd < 0.0 {
        return Err(domain("cross-trial standard deviation", "finite and non-negative", cross_trial_sd));
    }
    if n_trials == 1 {
        return Ok(0.0);
    }
    let n = n_trials as f64;
    let a = probit(1.0 - 1.0 / n)?;
    let b = probit(1.0 - 1.0 / (n * E))?;
    Ok(cross_trial_sd * ((1.0 - EULER_MASCHERONI) * a + EULER_MASCHERONI * b))
}

/// The margin a candidate's standardized evidence must exceed: the expected
/// maximum of `n_trials` null draws **plus** the ordinary two-sided critical
/// value at `confidence`.
///
/// The threshold is denominated in standard errors of the candidate's own
/// statistic, so it is a *rule*, not a level: a candidate with more data has a
/// smaller standard error and therefore a reachable bar. Freezing a fixed
/// effect-size level instead would be unclearable at any sample size, which
/// strands a viable strategy permanently.
///
/// Strictly increasing in `n_trials` and in `cross_trial_sd`; at `n_trials = 1`
/// it reduces to the plain two-sided significance test.
///
/// # Errors
///
/// Propagates [`expected_max_null`] and [`two_sided_z`].
pub fn trials_corrected_threshold(
    n_trials: usize,
    cross_trial_sd: f64,
    confidence: f64,
) -> Result<f64, StatsError> {
    Ok(expected_max_null(n_trials, cross_trial_sd)? + two_sided_z(confidence)?)
}

/// Whether the margin comparison is live.
///
/// `Disarmed` exists **only** so a test can prove the comparison is load-bearing
/// without editing and restoring source (`docs/solutions/conventions/
/// coverage-only-change-is-verified-by-mutation-not-by-the-gate.md`: a
/// coverage-only change is verified by mutation, and a one-time manual edit is
/// not a standing falsifier). Production callers pass [`MarginArm::Armed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginArm {
    /// The comparison binds.
    Armed,
    /// The comparison is bypassed — everything clears. Test-only seam.
    Disarmed,
}

/// A margin adjudication.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarginVerdict {
    /// The candidate's standardized evidence (statistic ÷ its standard error).
    pub statistic: f64,
    /// The trials-corrected threshold it had to beat.
    pub threshold: f64,
    /// Whether it cleared.
    pub clears: bool,
    /// Which arm produced this verdict.
    pub arm: MarginArm,
}

/// Adjudicate one candidate against the margin. `Disarmed` clears everything —
/// that is the mutation the U4 falsifier flips to prove the assertion binds.
#[must_use]
pub fn margin_verdict(statistic: f64, threshold: f64, arm: MarginArm) -> MarginVerdict {
    let clears = match arm {
        MarginArm::Armed => statistic > threshold,
        MarginArm::Disarmed => true,
    };
    MarginVerdict { statistic, threshold, clears, arm }
}

// ===========================================================================
// The session-block resampler (KTD5, Q1)
// ===========================================================================

/// SplitMix64 — a seeded, dependency-free PRNG. Reproducibility is the whole
/// point: two calls with one seed must return byte-identical output, so a
/// recorded interval can be re-derived years later without pinning a crate
/// version.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    /// Seed the generator.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next raw 64-bit draw.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform draw on `0..bound` (Lemire's multiply-shift; `bound > 0`).
    pub fn below(&mut self, bound: usize) -> usize {
        debug_assert!(bound > 0);
        ((u128::from(self.next_u64()) * bound as u128) >> 64) as usize
    }

    /// A Rademacher draw (`−1.0` or `+1.0`), for the wild-cluster bootstrap.
    pub fn rademacher(&mut self) -> f64 {
        if self.next_u64() & 1 == 0 {
            -1.0
        } else {
            1.0
        }
    }
}

/// One resampling block. For the net-RoR statistic each entry is
/// `(realized_pnl, risk_capital)` for one trade, and the block is one KST
/// session.
///
/// **Q1 — the block length is one session.** The session is where the
/// clustering the design effect measures actually lives, and it is the only
/// length this sample can support: an automatic selector (Politis & White)
/// needs far more blocks than 24 to be consistent, and a multi-session block
/// would have to assume a session-to-session dependence nothing here measures.
/// The choice is recorded with the verdict rather than left implicit.
pub type Block = Vec<(f64, f64)>;

/// A block-bootstrap outcome for a ratio statistic `Σnum / Σden`.
#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapOutcome {
    /// The point estimate on the observed blocks.
    pub point: f64,
    /// Bootstrap standard error (sd of the replicate statistics).
    pub standard_error: f64,
    /// Lower percentile bound.
    pub lo: f64,
    /// Upper percentile bound.
    pub hi: f64,
    /// Share of replicates strictly above zero.
    pub p_positive: f64,
    /// Replicates drawn.
    pub replicates: usize,
    /// The seed that produced them.
    pub seed: u64,
    /// Blocks resampled (the cluster count).
    pub blocks: usize,
}

/// The observed ratio statistic over `blocks`.
///
/// # Errors
///
/// Refuses an empty block set or a non-positive denominator total.
pub fn ratio_statistic(blocks: &[Block]) -> Result<f64, StatsError> {
    if blocks.is_empty() {
        return Err(StatsError::Empty { what: "ratio statistic", need: 1 });
    }
    let num: f64 = blocks.iter().flatten().map(|(a, _)| *a).sum();
    let den: f64 = blocks.iter().flatten().map(|(_, b)| *b).sum();
    require_finite_positive("denominator total", den)?;
    Ok(num / den)
}

/// Session-block bootstrap of the ratio statistic: draw `blocks.len()` whole
/// blocks with replacement, `replicates` times, from `seed`.
///
/// # Errors
///
/// Refuses fewer than two blocks, zero replicates, and a degenerate
/// denominator.
pub fn block_bootstrap_ratio(
    blocks: &[Block],
    replicates: usize,
    seed: u64,
    confidence: f64,
) -> Result<BootstrapOutcome, StatsError> {
    if blocks.len() < 2 {
        return Err(StatsError::TooShort {
            what: "block bootstrap (blocks)",
            need: 2,
            got: blocks.len(),
        });
    }
    if replicates == 0 {
        return Err(domain("replicates", "at least 1", 0.0));
    }
    if !confidence.is_finite() || confidence <= 0.0 || confidence >= 1.0 {
        return Err(domain("confidence", "strictly inside (0, 1)", confidence));
    }
    let point = ratio_statistic(blocks)?;

    // Pre-fold each block to its (Σnum, Σden) pair: the ratio statistic only
    // ever sees the block totals, so folding once keeps the replicate loop
    // linear in the block count rather than in the trade count.
    let folded: Vec<(f64, f64)> = blocks
        .iter()
        .map(|b| (b.iter().map(|(a, _)| *a).sum::<f64>(), b.iter().map(|(_, d)| *d).sum::<f64>()))
        .collect();

    let mut rng = SplitMix64::new(seed);
    let mut draws = Vec::with_capacity(replicates);
    for _ in 0..replicates {
        let mut num = 0.0;
        let mut den = 0.0;
        for _ in 0..folded.len() {
            let (a, b) = folded[rng.below(folded.len())];
            num += a;
            den += b;
        }
        if den > 0.0 {
            draws.push(num / den);
        }
    }
    if draws.len() < 2 {
        return Err(StatsError::TooShort {
            what: "block bootstrap (usable replicates)",
            need: 2,
            got: draws.len(),
        });
    }
    let p_positive = draws.iter().filter(|x| **x > 0.0).count() as f64 / draws.len() as f64;
    let standard_error = sample_sd(&draws)?;
    let mut sorted = draws;
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("bootstrap draws are finite"));
    let tail = (1.0 - confidence) / 2.0 * 100.0;
    let lo = percentile(&sorted, tail).expect("non-empty");
    let hi = percentile(&sorted, 100.0 - tail).expect("non-empty");

    Ok(BootstrapOutcome {
        point,
        standard_error,
        lo,
        hi,
        p_positive,
        replicates: sorted.len(),
        seed,
        blocks: blocks.len(),
    })
}

/// An interval around a point estimate, with the critical value that built it.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    /// How this interval was built (printed verbatim in the report).
    pub label: &'static str,
    /// The point estimate.
    pub point: f64,
    /// The standard error used.
    pub standard_error: f64,
    /// The critical value used.
    pub critical_value: f64,
    /// Lower bound.
    pub lo: f64,
    /// Upper bound.
    pub hi: f64,
}

/// The naive interval: normal critical values. At fewer than ~30 clusters this
/// is the **optimistic** end of the honest range (KTD5) — report it beside a
/// correction, never alone.
///
/// # Errors
///
/// [`StatsError::Domain`] on a confidence outside `(0, 1)`.
pub fn interval_normal(
    point: f64,
    standard_error: f64,
    confidence: f64,
) -> Result<Interval, StatsError> {
    let z = two_sided_z(confidence)?;
    Ok(Interval {
        label: "naive (normal critical value)",
        point,
        standard_error,
        critical_value: z,
        lo: point - z * standard_error,
        hi: point + z * standard_error,
    })
}

/// The few-cluster correction: Student-t critical values at `G − 1` degrees of
/// freedom (KTD5). Wider than the naive interval, and the gap is the size of
/// the few-cluster bias the naive one hides.
///
/// # Errors
///
/// Refuses fewer than two clusters; propagates [`t_quantile`].
pub fn interval_t_few_clusters(
    point: f64,
    standard_error: f64,
    confidence: f64,
    clusters: usize,
) -> Result<Interval, StatsError> {
    if clusters < 2 {
        return Err(StatsError::TooShort {
            what: "few-cluster interval (clusters)",
            need: 2,
            got: clusters,
        });
    }
    let t = t_quantile(1.0 - (1.0 - confidence) / 2.0, (clusters - 1) as f64)?;
    Ok(Interval {
        label: "few-cluster (t, G−1 df)",
        point,
        standard_error,
        critical_value: t,
        lo: point - t * standard_error,
        hi: point + t * standard_error,
    })
}

/// The wild-cluster bootstrap variant (Rademacher weights on cluster-level
/// residuals), reported as an equal-tailed percentile-t interval. The second
/// KTD5 diagnostic: it keeps the cluster structure while breaking the sign of
/// each cluster's contribution, which is the failure mode a 24-cluster
/// normal-approximation interval is blind to.
///
/// # Errors
///
/// Refuses fewer than two blocks, zero replicates, a non-positive standard
/// error, and a degenerate denominator.
pub fn wild_cluster_interval(
    blocks: &[Block],
    standard_error: f64,
    confidence: f64,
    replicates: usize,
    seed: u64,
) -> Result<Interval, StatsError> {
    if blocks.len() < 2 {
        return Err(StatsError::TooShort {
            what: "wild-cluster bootstrap (blocks)",
            need: 2,
            got: blocks.len(),
        });
    }
    if replicates == 0 {
        return Err(domain("replicates", "at least 1", 0.0));
    }
    require_finite_positive("standard error", standard_error)?;
    if !confidence.is_finite() || confidence <= 0.0 || confidence >= 1.0 {
        return Err(domain("confidence", "strictly inside (0, 1)", confidence));
    }
    let point = ratio_statistic(blocks)?;
    let den_total: f64 = blocks.iter().flatten().map(|(_, d)| *d).sum();

    // Cluster-level residual under the null "the ratio equals `point`".
    let residuals: Vec<f64> = blocks
        .iter()
        .map(|b| {
            let num: f64 = b.iter().map(|(a, _)| *a).sum();
            let den: f64 = b.iter().map(|(_, d)| *d).sum();
            num - point * den
        })
        .collect();

    let mut rng = SplitMix64::new(seed);
    let mut ts = Vec::with_capacity(replicates);
    for _ in 0..replicates {
        let shifted: f64 =
            residuals.iter().map(|r| rng.rademacher() * r).sum::<f64>() / den_total;
        ts.push(shifted / standard_error);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).expect("wild draws are finite"));
    let tail = (1.0 - confidence) / 2.0 * 100.0;
    let t_lo = percentile(&ts, tail).expect("non-empty");
    let t_hi = percentile(&ts, 100.0 - tail).expect("non-empty");
    Ok(Interval {
        label: "wild-cluster (Rademacher, percentile-t)",
        point,
        standard_error,
        // The reported critical value is the wider tail — the single number a
        // reader can compare against the naive 1.96.
        critical_value: t_hi.abs().max(t_lo.abs()),
        lo: point - t_hi * standard_error,
        hi: point - t_lo * standard_error,
    })
}

/// Permute the per-trade outcomes across trades while holding the block
/// structure and every denominator in place — the null replicate KTD10 calls
/// for. Only the numerators move, so session count, cluster sizes, and the
/// risk-capital total are all invariant by construction.
///
/// # Errors
///
/// Refuses an empty block set.
pub fn permute_outcomes(blocks: &[Block], rng: &mut SplitMix64) -> Result<Vec<Block>, StatsError> {
    if blocks.is_empty() {
        return Err(StatsError::Empty { what: "permutation", need: 1 });
    }
    let mut pool: Vec<f64> = blocks.iter().flatten().map(|(a, _)| *a).collect();
    // Fisher–Yates, from the top down.
    for i in (1..pool.len()).rev() {
        let j = rng.below(i + 1);
        pool.swap(i, j);
    }
    let mut it = pool.into_iter();
    Ok(blocks
        .iter()
        .map(|b| b.iter().map(|(_, d)| (it.next().expect("pool is the same length"), *d)).collect())
        .collect())
}
