//! Derivation guard for the sample-sufficiency statistics core
//! (plan 2026-08-05-001, U1; R1–R3).
//!
//! Every assertion here reproduces its expected value from named constants and
//! the formula under test — the `prereg_derivation.rs` discipline. A test that
//! snapshotted `stats.rs`'s own output would pass on any formula, including the
//! one this turn exists to prevent (required n taken as the naive count, with
//! the design effect dropped).

use nautilus_ls_lab::stats::{
    block_bootstrap_ratio, clustering, design_effect, expected_max_null, interval_normal,
    interval_t_few_clusters, margin_verdict, mean, minimum_detectable_edge, percentile,
    permute_r_multiples, power_z, probit, ratio_statistic, required_trades, sample_sd, t_quantile,
    trials_corrected_threshold, two_sided_z, wild_cluster_interval, Block, MarginArm, SplitMix64,
    StatsError, EULER_MASCHERONI,
};

/// The confidence and power pinned by KTD11, before any reading.
const CONFIDENCE: f64 = 0.95;
const POWER: f64 = 0.80;

/// Φ⁻¹(0.975) and Φ⁻¹(0.80) to 16 digits — the published normal quantiles, not
/// values read back out of `probit`.
const Z_975: f64 = 1.959_963_984_540_054;
const Z_80: f64 = 0.841_621_233_572_914_4;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

macro_rules! assert_close {
    ($a:expr, $b:expr, $tol:expr, $($msg:tt)+) => {
        let (a, b, tol) = ($a, $b, $tol);
        assert!(close(a, b, tol), "{}: got {a}, want {b} (tol {tol})", format!($($msg)+));
    };
}

// ===========================================================================
// Location and dispersion
// ===========================================================================

#[test]
fn mean_and_sd_match_hand_computed_values() {
    // Σ = 20, n = 5 → mean 4. Deviations −2,−1,0,1,2 → Σd² = 10 → s = √(10/4).
    let xs = [2.0, 3.0, 4.0, 5.0, 6.0];
    assert_close!(mean(&xs).unwrap(), 20.0 / 5.0, 1e-12, "mean of a known series");
    assert_close!(
        sample_sd(&xs).unwrap(),
        (10.0f64 / 4.0).sqrt(),
        1e-12,
        "sample sd is Bessel-corrected"
    );
}

// ===========================================================================
// Clustering and the design effect
// ===========================================================================

#[test]
fn icc_is_zero_without_between_cluster_variance() {
    // Two clusters, identical composition → every cluster mean equals the grand
    // mean → MSB = 0 → the ANOVA point estimate is negative and clamps to 0.
    let values = [1.0, 2.0, 3.0, 1.0, 2.0, 3.0];
    let ids = [0, 0, 0, 1, 1, 1];
    let c = clustering(&values, &ids).unwrap();
    assert_close!(c.icc, 0.0, 1e-12, "no between-cluster variance → ICC 0");
    assert_close!(c.design_effect, 1.0, 1e-12, "and therefore no information lost");
    assert_close!(c.effective_n, 6.0, 1e-12, "effective n is the raw count");
}

#[test]
fn icc_approaches_one_as_within_cluster_variance_vanishes() {
    // Clusters far apart, members nearly identical inside → MSW → 0 → ICC → 1.
    let values = [0.0, 1e-9, 1_000.0, 1_000.000_000_001];
    let ids = [0, 0, 1, 1];
    let c = clustering(&values, &ids).unwrap();
    assert!(c.icc > 0.999_999, "within-cluster variance vanishes → ICC → 1, got {}", c.icc);
    // m₀ = 2 here, so deff → 1 + (2−1)·1 = 2 and effective n halves.
    assert_close!(c.design_effect, 2.0, 1e-5, "deff → 1 + (m₀−1)·ρ at ρ→1");
    assert_close!(c.effective_n, 2.0, 1e-4, "four fully-correlated pairs carry two observations");
}

#[test]
fn design_effect_is_one_when_every_cluster_holds_one_observation() {
    let values = [1.0, 5.0, 9.0, 13.0];
    let ids = [0, 1, 2, 3];
    let c = clustering(&values, &ids).unwrap();
    assert_eq!(c.clusters, 4, "four singleton clusters");
    assert_close!(c.design_effect, 1.0, 1e-12, "singleton clusters lose no information");
    assert_close!(c.effective_n, 4.0, 1e-12, "effective n is the raw count");
}

#[test]
fn design_effect_reproduces_the_documented_formula() {
    // Kish: deff = 1 + (m − 1)·ρ. Checked against the v35 figures the plan
    // records: m₀ = 4.5374, ρ = 0.327334 → 2.1579.
    assert_close!(
        design_effect(4.537_4, 0.327_334),
        1.0 + (4.537_4 - 1.0) * 0.327_334,
        1e-12,
        "deff is 1 + (m−1)ρ"
    );
    assert_close!(design_effect(4.537_4, 0.327_334), 2.157_9, 1e-4, "the v35 design effect");
    assert_close!(design_effect(10.0, 0.0), 1.0, 1e-12, "no correlation → no penalty");
    assert!(design_effect(4.0, -0.5) >= 1.0, "clustering never manufactures information");
}

// ===========================================================================
// Quantiles
// ===========================================================================

#[test]
fn probit_reproduces_the_published_normal_quantiles() {
    assert_close!(probit(0.975).unwrap(), Z_975, 1e-12, "Φ⁻¹(0.975)");
    assert_close!(probit(0.80).unwrap(), Z_80, 1e-12, "Φ⁻¹(0.80)");
    assert_close!(probit(0.5).unwrap(), 0.0, 1e-15, "Φ⁻¹(0.5)");
    assert_close!(probit(0.025).unwrap(), -Z_975, 1e-12, "symmetry");
    assert_close!(two_sided_z(CONFIDENCE).unwrap(), Z_975, 1e-12, "two-sided 95%");
    assert_close!(power_z(POWER).unwrap(), Z_80, 1e-12, "80% power");
    assert!(probit(0.0).is_err(), "0 is outside the open unit interval");
    assert!(probit(1.0).is_err(), "1 is outside the open unit interval");
}

#[test]
fn t_quantile_reproduces_published_critical_values() {
    // Published two-sided 97.5% Student-t critical values.
    for (df, want) in [(10.0, 2.228_139), (23.0, 2.068_658), (30.0, 2.042_272), (100.0, 1.983_972)]
    {
        assert_close!(
            t_quantile(0.975, df).unwrap(),
            want,
            1e-4,
            "t(0.975, {df}) — the Cornish–Fisher expansion"
        );
    }
    // Wider than the normal at every finite df, converging down toward it.
    assert!(t_quantile(0.975, 23.0).unwrap() > Z_975, "t is wider than z at 23 df");
    assert!(
        t_quantile(0.975, 23.0).unwrap() > t_quantile(0.975, 100.0).unwrap(),
        "fewer degrees of freedom → wider critical value"
    );
}

// ===========================================================================
// Detectability (R1, R2)
// ===========================================================================

#[test]
fn minimum_detectable_edge_scales_as_inverse_root_effective_n() {
    let sd = 0.641_523;
    let base = minimum_detectable_edge(sd, 50.0, CONFIDENCE, POWER).unwrap();
    let quad = minimum_detectable_edge(sd, 200.0, CONFIDENCE, POWER).unwrap();
    assert_close!(quad, base / 2.0, 1e-12, "quadrupling effective n halves the detectable edge");
    // And the value itself is (z_{1−α/2} + z_power)·sd/√n_eff, hand-computed.
    assert_close!(
        base,
        (Z_975 + Z_80) * sd / 50.0f64.sqrt(),
        1e-12,
        "MDE is the closed form, not a fit"
    );
}

#[test]
fn minimum_detectable_edge_reproduces_the_v35_reading() {
    // The plan's Problem Frame: net sd 0.641523 at an effective sample of about
    // 51 detects roughly +0.25 R — nine times the whole gross edge.
    let mde = minimum_detectable_edge(0.641_523, 111.0 / 2.157_914, CONFIDENCE, POWER).unwrap();
    assert_close!(mde, 0.250_6, 1e-3, "the v35 minimum detectable edge");
    assert!(mde > 8.0 * 0.028_422, "the detection floor is many times the gross edge");
}

#[test]
fn required_trades_scales_linearly_with_the_design_effect() {
    let one = required_trades(0.05, 0.64, 1.0, CONFIDENCE, POWER).unwrap();
    let two = required_trades(0.05, 0.64, 2.0, CONFIDENCE, POWER).unwrap();
    assert_close!(two, 2.0 * one, 1e-9, "doubling the design effect doubles required n");
}

#[test]
fn required_trades_reproduces_a_hand_computed_constant_and_rises_with_power() {
    // Naive: ((z_{1−α/2} + z_power)·sd/δ)². At sd = 0.641523, δ = 0.028422,
    // deff = 2.157914 this is the plan's ≈8,600 closed trades.
    let want_naive = ((Z_975 + Z_80) * 0.641_523 / 0.028_422).powi(2);
    let got = required_trades(0.028_422, 0.641_523, 2.157_914, CONFIDENCE, POWER).unwrap();
    assert_close!(got, want_naive * 2.157_914, 1e-6, "required n is naive × design effect");
    assert_close!(got, 8_628.0, 5.0, "the plan's ~8,600 closed trades");

    // Power is a real parameter, not decoration: 50% power sets z_power = 0.
    let at_50 = required_trades(0.028_422, 0.641_523, 2.157_914, CONFIDENCE, 0.50).unwrap();
    assert!(at_50 < got, "80% power needs more trades than 50%: {at_50} vs {got}");
    assert_close!(
        at_50,
        (Z_975 * 0.641_523 / 0.028_422).powi(2) * 2.157_914,
        1e-6,
        "at 50% power the z_power term vanishes"
    );
}

// ===========================================================================
// The trials-corrected threshold (R6, KTD2)
// ===========================================================================

#[test]
fn expected_max_null_is_the_false_strategy_theorem() {
    let sigma = 0.3;
    let n = 29usize;
    let want = sigma
        * ((1.0 - EULER_MASCHERONI) * probit(1.0 - 1.0 / 29.0).unwrap()
            + EULER_MASCHERONI * probit(1.0 - 1.0 / (29.0 * std::f64::consts::E)).unwrap());
    assert_close!(expected_max_null(n, sigma).unwrap(), want, 1e-12, "the FST closed form");
}

/// A representative candidate standard error, in the statistic's own units.
const SE: f64 = 0.087_002;

#[test]
fn the_threshold_is_strictly_increasing_in_the_trial_count() {
    let sigma = 0.3;
    let mut prev = trials_corrected_threshold(1, sigma, CONFIDENCE, SE).unwrap();
    for n in 2..=60usize {
        let cur = trials_corrected_threshold(n, sigma, CONFIDENCE, SE).unwrap();
        assert!(cur > prev, "threshold rises from {} trials to {n}: {prev} → {cur}", n - 1);
        prev = cur;
    }
}

#[test]
fn the_threshold_is_increasing_in_the_cross_trial_variance() {
    let a = trials_corrected_threshold(29, 0.10, CONFIDENCE, SE).unwrap();
    let b = trials_corrected_threshold(29, 0.30, CONFIDENCE, SE).unwrap();
    assert!(b > a, "more dispersed trials buy more luck: {a} → {b}");
}

#[test]
fn the_threshold_is_every_term_in_the_statistics_own_units() {
    // The whole rule, written out longhand: selection tax plus sampling term,
    // both in net-RoR units. A threshold that mixed a unitless z into a
    // RoR-denominated selection tax would be meaningless — and would silently
    // pass a candidate whose evidence is orders of magnitude smaller.
    let sigma = 0.026_367_936_878_680_807;
    let got = trials_corrected_threshold(29, sigma, CONFIDENCE, SE).unwrap();
    let want = expected_max_null(29, sigma).unwrap() + Z_975 * SE;
    assert_close!(got, want, 1e-15, "threshold = E[max] + z x candidate SE");
    // The sampling term shrinks with the candidate's own sample; the selection
    // tax does not. That asymmetry is what keeps the bar reachable.
    let quarter = trials_corrected_threshold(29, sigma, CONFIDENCE, SE / 4.0).unwrap();
    assert_close!(
        quarter,
        expected_max_null(29, sigma).unwrap() + Z_975 * SE / 4.0,
        1e-15,
        "a 16x larger sample quarters only the sampling term"
    );
    assert!(quarter < got, "more data lowers the bar: {quarter} < {got}");
    assert!(
        quarter > expected_max_null(29, sigma).unwrap(),
        "but never below the selection tax the search already spent"
    );
    assert!(
        trials_corrected_threshold(29, sigma, CONFIDENCE, -1.0).is_err(),
        "a negative standard error is a refusal"
    );
}

#[test]
fn the_threshold_reduces_to_the_single_trial_case_at_one() {
    assert_close!(expected_max_null(1, 0.3).unwrap(), 0.0, 1e-15, "no selection to correct at N=1");
    assert_close!(
        trials_corrected_threshold(1, 0.3, CONFIDENCE, SE).unwrap(),
        Z_975 * SE,
        1e-15,
        "at N=1 the margin is the plain two-sided test"
    );
    // Cross-trial dispersion is irrelevant when nothing was selected.
    assert_close!(
        trials_corrected_threshold(1, 9.9, CONFIDENCE, SE).unwrap(),
        Z_975 * SE,
        1e-15,
        "σ does not move the single-trial bar"
    );
}

#[test]
fn the_margin_comparison_binds_when_armed_and_clears_everything_when_disarmed() {
    let threshold = trials_corrected_threshold(29, 0.3, CONFIDENCE, SE).unwrap();
    let below = margin_verdict(threshold - 0.001, threshold, MarginArm::Armed);
    assert!(!below.clears, "armed: evidence at the threshold does not clear it");
    let above = margin_verdict(threshold + 0.001, threshold, MarginArm::Armed);
    assert!(above.clears, "armed: evidence above the threshold clears");
    let disarmed = margin_verdict(threshold - 99.0, threshold, MarginArm::Disarmed);
    assert!(disarmed.clears, "the disarm seam bypasses the comparison — U4's falsifier");
}

// ===========================================================================
// The resampler (Q1, KTD5)
// ===========================================================================

fn two_by_two() -> Vec<Block> {
    vec![vec![(1.0, 10.0), (-2.0, 10.0)], vec![(3.0, 10.0), (4.0, 10.0)]]
}

/// Six sessions of unequal size and sign — rich enough that two seeds cannot
/// coincide on the replicate distribution by accident.
fn six_sessions() -> Vec<Block> {
    vec![
        vec![(1.0, 10.0), (-2.0, 12.0)],
        vec![(3.0, 11.0), (4.0, 9.0), (-5.0, 13.0)],
        vec![(-1.5, 8.0)],
        vec![(2.5, 10.0), (0.5, 10.0)],
        vec![(-4.0, 15.0), (6.0, 7.0), (1.0, 10.0), (-0.5, 11.0)],
        vec![(0.25, 9.0), (-3.0, 14.0)],
    ]
}

#[test]
fn the_block_resampler_is_reproducible_across_two_calls() {
    let blocks = six_sessions();
    let a = block_bootstrap_ratio(&blocks, 500, 20_260_805, CONFIDENCE).unwrap();
    let b = block_bootstrap_ratio(&blocks, 500, 20_260_805, CONFIDENCE).unwrap();
    assert_eq!(a, b, "one seed → byte-identical output");
    let c = block_bootstrap_ratio(&blocks, 500, 20_260_806, CONFIDENCE).unwrap();
    assert_ne!(a.standard_error, c.standard_error, "a different seed draws a different sample");

    let two = two_by_two();
    let d = block_bootstrap_ratio(&two, 500, 20_260_805, CONFIDENCE).unwrap();
    assert_close!(
        d.point,
        ratio_statistic(&two).unwrap(),
        1e-15,
        "the point estimate is the observed ratio, not a replicate average"
    );
    assert_close!(d.point, 6.0 / 40.0, 1e-15, "Σnum/Σden over the whole block set");
}

#[test]
fn the_few_cluster_interval_is_wider_than_the_naive_one() {
    let naive = interval_normal(0.1, 0.05, CONFIDENCE).unwrap();
    let few = interval_t_few_clusters(0.1, 0.05, CONFIDENCE, 24).unwrap();
    assert_close!(naive.critical_value, Z_975, 1e-12, "the naive interval uses z");
    assert_close!(few.critical_value, 2.068_658, 1e-4, "the corrected one uses t at G−1 = 23");
    assert!(few.hi > naive.hi && few.lo < naive.lo, "the correction widens, never tightens");
}

#[test]
fn the_wild_cluster_interval_is_seed_reproducible_and_centred_on_the_point() {
    // The third KTD5 diagnostic. It is printed beside the other two, so a
    // silent change in it would misreport how optimistic the naive interval is.
    let blocks = six_sessions();
    let point = ratio_statistic(&blocks).unwrap();
    let se = 0.05;
    let a = wild_cluster_interval(&blocks, se, CONFIDENCE, 400, 20_260_806).unwrap();
    let b = wild_cluster_interval(&blocks, se, CONFIDENCE, 400, 20_260_806).unwrap();
    assert_eq!(a, b, "one seed → byte-identical output");
    let c = wild_cluster_interval(&blocks, se, CONFIDENCE, 400, 20_260_807).unwrap();
    assert_ne!(a.critical_value, c.critical_value, "a different seed draws differently");

    assert_close!(a.point, point, 1e-15, "centred on the observed ratio, not a replicate mean");
    assert!(a.lo < a.point && a.point < a.hi, "the interval brackets the point estimate");
    assert!(a.critical_value > 0.0, "the reported critical value is the wider tail");
    // Rademacher weights are symmetric, so the percentile-t tails are roughly
    // balanced — a one-sided implementation would fail this.
    let half_width_lo = a.point - a.lo;
    let half_width_hi = a.hi - a.point;
    assert!(
        (half_width_lo - half_width_hi).abs() < 0.6 * half_width_hi.max(half_width_lo),
        "Rademacher tails are roughly symmetric: [{}, {}]",
        a.lo,
        a.hi
    );
    assert_eq!(a.label, "wild-cluster (Rademacher, percentile-t)");

    assert!(
        wild_cluster_interval(&blocks, 0.0, CONFIDENCE, 400, 1).is_err(),
        "a zero standard error is a refusal, not a division"
    );
    assert!(
        wild_cluster_interval(&blocks[..1], 0.05, CONFIDENCE, 400, 1).is_err(),
        "a single block cannot be resampled"
    );
    assert!(
        wild_cluster_interval(&blocks, 0.05, CONFIDENCE, 0, 1).is_err(),
        "zero replicates is a refusal"
    );
}

#[test]
fn percentile_reproduces_nearest_rank_at_odd_and_even_counts() {
    // `report.rs` aliases its `nearest_rank` to this function, so the six
    // existing MFE-report call sites depend on exactly these values.
    let odd = [1.0, 2.0, 3.0, 4.0, 5.0];
    assert_eq!(percentile(&odd, 50.0), Some(3.0));
    assert_eq!(percentile(&odd, 70.0), Some(4.0), "rank ceil(3.5)=4");
    assert_eq!(percentile(&odd, 90.0), Some(5.0));
    let even = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(percentile(&even, 50.0), Some(2.0));
    assert_eq!(percentile(&even, 75.0), Some(3.0));
    assert_eq!(percentile(&even, 0.0), Some(1.0), "rank floors at 1, never 0");
    assert_eq!(percentile(&even, 100.0), Some(4.0));
    assert_eq!(percentile(&[7.0], 25.0), Some(7.0));
    assert_eq!(percentile(&[], 50.0), None, "an empty sample has no percentile");
    assert_eq!(percentile(&even, -1.0), None, "out-of-range pct is None, not a panic");
    assert_eq!(percentile(&even, 101.0), None);
}

#[test]
fn permutation_preserves_the_block_structure_and_every_denominator() {
    // R-multiple blocks: (realized_r, risk_capital), unequal risk capital so
    // the re-pairing actually bites.
    let blocks: Vec<Block> =
        vec![vec![(0.4, 10.0), (-0.2, 30.0)], vec![(0.1, 20.0), (-0.5, 40.0)]];
    let mut rng = SplitMix64::new(7);
    let permuted = permute_r_multiples(&blocks, &mut rng).unwrap();
    assert_eq!(permuted.len(), blocks.len(), "session count is invariant");
    for (a, b) in blocks.iter().zip(&permuted) {
        assert_eq!(a.len(), b.len(), "cluster sizes are invariant");
        for (x, y) in a.iter().zip(b) {
            assert_close!(x.1, y.1, 1e-15, "risk capital never moves");
        }
    }
    let total_before: f64 = blocks.iter().flatten().map(|(_, d)| *d).sum();
    let total_after: f64 = permuted.iter().flatten().map(|(_, d)| *d).sum();
    assert_close!(total_after, total_before, 1e-12, "the risk-capital total is invariant");

    // The R-multiple multiset is a permutation, not a resample.
    let mut before: Vec<f64> = blocks.iter().flatten().map(|(r, _)| *r).collect();
    let mut after: Vec<f64> =
        permuted.iter().flatten().map(|(num, den)| num / den).collect();
    before.sort_by(|a, b| a.partial_cmp(b).unwrap());
    after.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (x, y) in before.iter().zip(&after) {
        assert_close!(*y, *x, 1e-12, "the R-multiple multiset is preserved");
    }
}

#[test]
fn permuting_r_multiples_moves_the_ratio_where_permuting_numerators_could_not() {
    // The trap this helper exists to avoid: `sum(num)/sum(den)` is EXACTLY
    // invariant under a permutation of the numerators, so a null built that way
    // has zero dispersion and any bar clears it vacuously. Re-pairing an
    // R-multiple with a different risk capital is what actually moves the
    // statistic.
    let blocks: Vec<Block> =
        vec![vec![(0.4, 10.0), (-0.2, 30.0)], vec![(0.1, 20.0), (-0.5, 40.0)]];
    let observed = ratio_statistic(
        &blocks.iter().map(|b| b.iter().map(|(r, d)| (r * d, *d)).collect()).collect::<Vec<_>>(),
    )
    .unwrap();

    // Permuting the NUMERATORS: the total is unchanged, so the ratio is too.
    let numerators: Vec<Block> =
        blocks.iter().map(|b| b.iter().map(|(r, d)| (r * d, *d)).collect()).collect();
    let shuffled_numerators: Vec<Block> = vec![
        vec![numerators[1][1], numerators[0][0]],
        vec![numerators[1][0], numerators[0][1]],
    ]
    .into_iter()
    .map(|b: Vec<(f64, f64)>| {
        // keep each slot's own denominator, move only the numerator
        b.into_iter().collect()
    })
    .collect();
    let numerator_total: f64 = shuffled_numerators.iter().flatten().map(|(n, _)| *n).sum();
    let original_total: f64 = numerators.iter().flatten().map(|(n, _)| *n).sum();
    assert_close!(
        numerator_total,
        original_total,
        1e-12,
        "permuting numerators cannot change their sum — hence cannot change the ratio"
    );

    // Permuting the R-MULTIPLES does move it, for at least one seed.
    let moved = (0..8u64).any(|seed| {
        let mut rng = SplitMix64::new(seed);
        let p = permute_r_multiples(&blocks, &mut rng).unwrap();
        (ratio_statistic(&p).unwrap() - observed).abs() > 1e-9
    });
    assert!(moved, "re-pairing R-multiples with different risk capital moves the ratio");
}

// ===========================================================================
// Degenerate inputs fail closed (R3's fragility, made mechanical)
// ===========================================================================

#[test]
fn degenerate_inputs_return_an_explicit_error_rather_than_a_value() {
    assert!(matches!(mean(&[]), Err(StatsError::Empty { .. })), "empty series has no mean");
    assert!(matches!(sample_sd(&[]), Err(StatsError::Empty { .. })), "empty series has no sd");
    assert!(
        matches!(sample_sd(&[1.0]), Err(StatsError::TooShort { .. })),
        "a single trade has no dispersion"
    );
    assert!(
        matches!(clustering(&[1.0, 2.0], &[0, 0]), Err(StatsError::TooShort { .. })),
        "a single session cannot separate between- from within-cluster variance"
    );
    assert!(
        matches!(clustering(&[1.0], &[0, 1]), Err(StatsError::Mismatched { .. })),
        "parallel slices must be parallel"
    );
    assert!(
        matches!(
            required_trades(0.0, 0.64, 1.0, CONFIDENCE, POWER),
            Err(StatsError::Domain { .. })
        ),
        "a zero target effect is not detectable at any sample size"
    );
    assert!(
        matches!(
            required_trades(0.05, 0.0, 1.0, CONFIDENCE, POWER),
            Err(StatsError::Domain { .. })
        ),
        "a zero dispersion is not a sample-size question"
    );
    assert!(
        matches!(expected_max_null(0, 0.3), Err(StatsError::Domain { .. })),
        "zero trials is not a trial count"
    );
    assert!(
        matches!(
            block_bootstrap_ratio(&[vec![(1.0, 1.0)]], 10, 1, CONFIDENCE),
            Err(StatsError::TooShort { .. })
        ),
        "a single block cannot be resampled"
    );
    assert!(
        matches!(ratio_statistic(&[vec![(1.0, 0.0)]]), Err(StatsError::Domain { .. })),
        "a zero denominator total is a refusal, not an infinity"
    );
}
