//! Derivation guard for the PAIRED-power measurement
//! (plan 2026-08-07-001, U5; R8, R9, AE5).
//!
//! Hermetic: it reads the committed fixture
//! `tests/fixtures/paired-arms-closed-trades.json` and the committed
//! `config/sample-margin.json`, and reaches no run directory and no data home.
//! `data/` is gitignored, so the fixture is the only way CI can hold the
//! distribution the verdict rests on.
//!
//! **This is the unit whose green means nothing on its own.** A fixture-derived
//! test passes before and after a behavior change, so every assertion here is
//! made falsifiable by construction: each figure is reproduced from named
//! constants and the formula under test, never asserted against a stored
//! verdict string or a snapshot of the implementation's own output. See
//! `docs/solutions/conventions/coverage-only-change-is-verified-by-mutation-not-by-the-gate.md`
//! and
//! `docs/solutions/conventions/assert-on-a-fact-the-parent-emits-not-the-childs-own-marker.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use nautilus_ls_lab::margin::{self, frozen_margin_path};
use nautilus_ls_lab::runner::report::{
    PAIRED_HEAD_CALENDAR_SESSIONS, PAIRED_REACHABLE_CALENDAR_SESSIONS, SAMPLE_CONFIDENCE,
    SAMPLE_POWER, SAMPLE_REPLICATES, SAMPLE_SEED,
};
use nautilus_ls_lab::stats::{
    block_bootstrap_ratio, paired_block_bootstrap_difference, power_z, two_sided_z, Block,
    PairedBlock,
};
use serde::Deserialize;

// ===========================================================================
// The fixture
// ===========================================================================

#[derive(Debug, Deserialize)]
struct Fixture {
    head: Run,
    arms: BTreeMap<String, Run>,
}

#[derive(Debug, Deserialize)]
struct Run {
    run_id: String,
    strategy_version: u32,
    catalog_fingerprint: String,
    universe_hash: String,
    strategy_code_hash: String,
    closed_trades: usize,
    trades: Vec<Trade>,
    #[serde(default)]
    param_diff: BTreeMap<String, ParamMove>,
    frozen_record: FrozenRef,
}

#[derive(Debug, Deserialize)]
struct ParamMove {
    #[allow(dead_code)]
    head: serde_json::Value,
    #[allow(dead_code)]
    arm: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct FrozenRef {
    cross_trial_arms_index: usize,
    arm: String,
    net_ror: f64,
}

#[derive(Debug, Deserialize, Clone)]
struct Trade {
    session: String,
    #[allow(dead_code)]
    realized_r: f64,
    risk_capital: f64,
    realized_pnl: f64,
}

fn fixture() -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("paired-arms-closed-trades.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// Every arm in a stable order, so the assertions and the printed diagnostics
/// agree run to run.
fn arms_in_order(f: &Fixture) -> Vec<(&String, &Run)> {
    let mut v: Vec<_> = f.arms.iter().collect();
    v.sort_by_key(|(_, r)| r.frozen_record.cross_trial_arms_index);
    v
}

// ===========================================================================
// Derivations — each reproduced from the formula, not read back
// ===========================================================================

/// `Σ realized_pnl / Σ risk_capital` — the net-RoR statistic KTD3 pins, spelled
/// out here rather than called through the implementation, so a change to the
/// implementation's definition fails this test instead of moving with it.
fn net_ror(trades: &[Trade]) -> f64 {
    let num: f64 = trades.iter().map(|t| t.realized_pnl).sum();
    let den: f64 = trades.iter().map(|t| t.risk_capital).sum();
    assert!(den > 0.0, "a run with no risk capital has no net RoR");
    num / den
}

fn sessions_of(trades: &[Trade]) -> BTreeSet<String> {
    trades.iter().map(|t| t.session.clone()).collect()
}

/// One session's `(Σ realized_pnl, Σ risk_capital)`.
fn fold_by_session(trades: &[Trade]) -> BTreeMap<String, (f64, f64)> {
    let mut out: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for t in trades {
        let e = out.entry(t.session.clone()).or_insert((0.0, 0.0));
        e.0 += t.realized_pnl;
        e.1 += t.risk_capital;
    }
    out
}

/// Paired blocks over the UNION of the sessions either arm traded (KTD4).
fn union_blocks(head: &[Trade], arm: &[Trade]) -> Vec<PairedBlock> {
    let (h, a) = (fold_by_session(head), fold_by_session(arm));
    let mut union: Vec<String> = h.keys().chain(a.keys()).cloned().collect();
    union.sort();
    union.dedup();
    union
        .into_iter()
        .map(|s| PairedBlock {
            head: h.get(&s).map(|p| vec![*p]).unwrap_or_default(),
            arm: a.get(&s).map(|p| vec![*p]).unwrap_or_default(),
        })
        .collect()
}

fn one_sided_blocks(blocks: &[PairedBlock], head_side: bool) -> Vec<Block> {
    blocks
        .iter()
        .map(|b| if head_side { b.head.clone() } else { b.arm.clone() })
        .filter(|b| !b.is_empty())
        .collect()
}

// ===========================================================================
// The fixture reproduces the frozen record
// ===========================================================================

#[test]
fn each_arms_recomputed_net_ror_matches_its_frozen_cross_trial_entry() {
    let f = fixture();
    let frozen = margin::load(&frozen_margin_path()).unwrap().values;

    // The head first: `cross_trial_arms[0]` is the v35 baseline.
    let head_ror = net_ror(&f.head.trades);
    let cited = &frozen.cross_trial_arms[f.head.frozen_record.cross_trial_arms_index];
    assert_eq!(f.head.frozen_record.cross_trial_arms_index, 0, "the head is the baseline row");
    assert_eq!(cited.net_ror, f.head.frozen_record.net_ror, "the fixture cites the exact entry");
    assert_eq!(cited.arm, f.head.frozen_record.arm);
    assert_eq!(
        round4(head_ror),
        cited.net_ror,
        "head net RoR recomputed {head_ror} does not reproduce the frozen {}",
        cited.net_ror
    );

    for (version, arm) in arms_in_order(&f) {
        let r = net_ror(&arm.trades);
        let cited = &frozen.cross_trial_arms[arm.frozen_record.cross_trial_arms_index];
        assert_eq!(
            cited.net_ror, arm.frozen_record.net_ror,
            "v{version} cites cross_trial_arms[{}] verbatim",
            arm.frozen_record.cross_trial_arms_index
        );
        assert_eq!(cited.arm, arm.frozen_record.arm);
        assert_eq!(
            round4(r),
            cited.net_ror,
            "v{version} ({}) net RoR recomputed {r} does not reproduce the frozen {}",
            arm.run_id,
            cited.net_ror
        );
    }
    assert_eq!(f.arms.len(), 6, "six off-flip arms, per the plan's corrected count");
    assert_eq!(
        frozen.cross_trial_arms.len(),
        7,
        "the frozen table is the head plus those six"
    );
}

/// The frozen record carries four decimals, so a recomputation is checked at
/// four. Spelled out rather than using a tolerance, so the precision the check
/// actually has is visible.
fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

#[test]
fn the_heads_closed_trade_and_session_counts_reproduce_the_frozen_provenance() {
    let f = fixture();
    let p = margin::load(&frozen_margin_path()).unwrap().values.provenance;
    assert_eq!(f.head.trades.len(), p.closed_trades, "111 closed trades");
    assert_eq!(f.head.closed_trades, p.closed_trades, "and the fixture's own count agrees");
    assert_eq!(sessions_of(&f.head.trades).len(), p.sessions, "over 24 KST sessions");
    assert_eq!(f.head.run_id, p.run_id);
    assert_eq!(f.head.strategy_version, p.strategy_version);
    assert_eq!(f.head.catalog_fingerprint, p.catalog_fingerprint);
    // `cross_trial_arms` records no per-arm trade count, so there is nothing to
    // check the arms' counts against there.
}

#[test]
fn all_seven_runs_share_the_catalog_universe_and_code_hashes() {
    // The comparability precondition (KTD7). If this ever fails the pairing is
    // invalid regardless of what the verb reports.
    let f = fixture();
    let triples: BTreeSet<(String, String, String)> = std::iter::once(&f.head)
        .chain(f.arms.values())
        .map(|r| {
            (
                r.catalog_fingerprint.clone(),
                r.universe_hash.clone(),
                r.strategy_code_hash.clone(),
            )
        })
        .collect();
    assert_eq!(triples.len(), 1, "all seven runs share one (catalog, universe, code) triple");
    let (cat, uni, code) = triples.into_iter().next().unwrap();
    assert!(cat.starts_with("ac026541"), "catalog {cat}");
    assert!(uni.starts_with("2dfc00d7"), "universe {uni}");
    assert!(code.starts_with("7571abef"), "code {code}");
}

#[test]
fn the_risk_sizing_arms_recorded_param_diff_carries_two_entries_not_one() {
    // KTD6. The frozen record labels this arm by `risk_per_trade_krw` alone; the
    // run actually flips `ratio_atr_alpha` too, so its difference is not
    // attributable to either lever. Reported, not dropped — dropping it would
    // silently move the frozen arm count from six to five.
    let f = fixture();
    let confounded: Vec<_> = arms_in_order(&f)
        .into_iter()
        .filter(|(_, a)| a.param_diff.len() > 1)
        .collect();
    assert_eq!(confounded.len(), 1, "exactly one arm is confounded");
    let (version, arm) = confounded[0];
    assert_eq!(version, "95", "it is the risk-sizing arm");
    assert_eq!(arm.param_diff.len(), 2, "{:?}", arm.param_diff.keys().collect::<Vec<_>>());
    assert!(arm.param_diff.contains_key("risk_per_trade_krw"));
    assert!(arm.param_diff.contains_key("ratio_atr_alpha"));
    assert!(
        arm.frozen_record.arm.starts_with("risk_per_trade_krw"),
        "and the frozen record still labels it by the first alone: {}",
        arm.frozen_record.arm
    );
    // Every other arm moved exactly one lever, and none records
    // `strategy_version` — which differs on every arm by construction.
    for (v, a) in arms_in_order(&f) {
        assert!(!a.param_diff.contains_key("strategy_version"), "v{v} records the run's identity");
        if v != "95" {
            assert_eq!(a.param_diff.len(), 1, "v{v}: {:?}", a.param_diff.keys().collect::<Vec<_>>());
        }
    }
}

// ===========================================================================
// AE5 — the identity the union-block choice exists to satisfy
// ===========================================================================

#[test]
fn ae5_each_paired_point_estimate_is_the_head_minus_the_arm_recomputed() {
    let f = fixture();
    let head_ror = net_ror(&f.head.trades);
    let head_sessions = sessions_of(&f.head.trades);

    for (version, arm) in arms_in_order(&f) {
        let blocks = union_blocks(&f.head.trades, &arm.trades);
        let out = paired_block_bootstrap_difference(
            &blocks,
            SAMPLE_REPLICATES,
            SAMPLE_SEED,
            SAMPLE_CONFIDENCE,
        )
        .unwrap_or_else(|e| panic!("v{version}: {e}"));

        let arm_sessions = sessions_of(&arm.trades);
        let union = head_sessions.union(&arm_sessions).count();
        let intersection = head_sessions.intersection(&arm_sessions).count();
        let want = head_ror - net_ror(&arm.trades);
        // Six decimal places, both sides recomputed from the fixture — the
        // frozen record's four decimals could not meet this tolerance, which is
        // exactly why the check is not made against it.
        assert!(
            (out.point - want).abs() < 5e-7,
            "v{version}: point {} != head − arm = {want} (union {union}, head-intersection \
             {intersection})",
            out.point
        );
        assert_eq!(out.blocks, union, "v{version}: the blocks are the union");

        // Which arms this check actually discriminates on: where the union
        // exceeds the intersection, an intersection build measures a different
        // quantity. Where they are equal a green result proves less, and saying
        // so is the point.
        println!(
            "v{version}: union {union} | head-intersection {intersection} | \
             discriminates: {}",
            if union > intersection { "YES" } else { "no — the identity holds either way" }
        );
    }

    // At least one arm must discriminate, or this whole test is decorative.
    let discriminating = arms_in_order(&f)
        .into_iter()
        .filter(|(_, a)| {
            let s = sessions_of(&a.trades);
            head_sessions.union(&s).count() > head_sessions.intersection(&s).count()
        })
        .count();
    assert!(
        discriminating >= 1,
        "no arm has a union wider than its intersection, so AE5 cannot fail under an \
         intersection build and asserts nothing"
    );
}

// ===========================================================================
// The measured property, and the projection
// ===========================================================================

#[test]
fn pairing_reduces_the_standard_error_on_every_arm_of_this_fixture() {
    // A measured fact about these seven runs, NOT a theorem about the estimator:
    // pairing reduces variance only where the two arms' per-block contributions
    // covary, and the arm-only blocks contribute no covariance at all. Recorded
    // here because it is what makes the paired measurement worth taking; the
    // VERDICT it feeds belongs in the turn record, not in a test.
    let f = fixture();
    for (version, arm) in arms_in_order(&f) {
        let blocks = union_blocks(&f.head.trades, &arm.trades);
        let paired = paired_block_bootstrap_difference(
            &blocks,
            SAMPLE_REPLICATES,
            SAMPLE_SEED,
            SAMPLE_CONFIDENCE,
        )
        .unwrap();
        let h = block_bootstrap_ratio(
            &one_sided_blocks(&blocks, true),
            SAMPLE_REPLICATES,
            SAMPLE_SEED,
            SAMPLE_CONFIDENCE,
        )
        .unwrap();
        let a = block_bootstrap_ratio(
            &one_sided_blocks(&blocks, false),
            SAMPLE_REPLICATES,
            SAMPLE_SEED.wrapping_add(1),
            SAMPLE_CONFIDENCE,
        )
        .unwrap();
        let independent = (h.standard_error.powi(2) + a.standard_error.powi(2)).sqrt();
        assert!(
            paired.standard_error < independent,
            "v{version}: paired SE {} is not below the independent {independent}",
            paired.standard_error
        );
    }
}

#[test]
fn the_attributability_verdict_recomputes_from_the_measured_se_and_the_frozen_confidence() {
    // Recomputed from the formula, never asserted against a stored verdict
    // string: `|point| > z(confidence) x SE`.
    //
    // The verdict IS asserted here, per arm, because the fixture is frozen and
    // the answer is therefore deterministic: on these seven runs not one arm
    // clears its own bar. Guarding that is what makes a future change to the
    // estimator that silently flipped an arm fail here instead of quietly
    // rewriting the turn record's conclusion.
    let f = fixture();
    let z = two_sided_z(SAMPLE_CONFIDENCE).unwrap();
    for (version, arm) in arms_in_order(&f) {
        let blocks = union_blocks(&f.head.trades, &arm.trades);
        let out = paired_block_bootstrap_difference(
            &blocks,
            SAMPLE_REPLICATES,
            SAMPLE_SEED,
            SAMPLE_CONFIDENCE,
        )
        .unwrap();
        let bar = z * out.standard_error;
        let attributable = out.point.abs() > bar;
        println!(
            "v{version}: point {:+.6} | SE {:.6} | z x SE {:+.6} | attributable {attributable}",
            out.point, out.standard_error, bar
        );
        assert!(
            !attributable,
            "v{version}: |{:+.6}| clears the bar {bar:+.6} — the measured verdict on this frozen \
             fixture is that NO arm does",
            out.point
        );
        // And it falls short by a margin, not by a mile. Without this the test
        // would also pass on a degenerate estimator that returned an enormous
        // standard error for every arm — which would make "not attributable"
        // true and meaningless.
        let shortfall_ratio = out.point.abs() / bar;
        assert!(
            (0.2..1.0).contains(&shortfall_ratio),
            "v{version}: |point| is {shortfall_ratio:.3} of the bar — outside the band that makes \
             this a genuine near-miss rather than a degenerate SE"
        );
    }
}

#[test]
fn the_reachable_supply_projection_is_the_measured_se_scaled_by_the_session_root() {
    // KTD10: `sqrt(45 / 237)`. 45 is the head run's in-range CALENDAR sessions,
    // NOT its 24 trade-producing ones — the unit slip the 2026-08-06 turn
    // corrected and `RateBasis` guards.
    let f = fixture();
    assert_eq!(PAIRED_HEAD_CALENDAR_SESSIONS, 45.0);
    assert_eq!(PAIRED_REACHABLE_CALENDAR_SESSIONS, 237.0);
    assert_ne!(
        PAIRED_HEAD_CALENDAR_SESSIONS,
        sessions_of(&f.head.trades).len() as f64,
        "the projection must NOT be rooted in the 24 trade-producing sessions"
    );

    let factor = (PAIRED_HEAD_CALENDAR_SESSIONS / PAIRED_REACHABLE_CALENDAR_SESSIONS).sqrt();
    assert!((factor - 0.435_744_9).abs() < 1e-6, "sqrt(45/237) = {factor}");
    assert!(factor < 1.0, "more sessions can only shrink the standard error");

    let z = two_sided_z(SAMPLE_CONFIDENCE).unwrap();
    let z_power = power_z(SAMPLE_POWER).unwrap();
    for (version, arm) in arms_in_order(&f) {
        let blocks = union_blocks(&f.head.trades, &arm.trades);
        let out = paired_block_bootstrap_difference(
            &blocks,
            SAMPLE_REPLICATES,
            SAMPLE_SEED,
            SAMPLE_CONFIDENCE,
        )
        .unwrap();
        let projected = out.standard_error * factor;
        // The root-n law in its INVARIANT form: variance x sessions is
        // conserved. Asserting `projected / se == factor` would be a tautology
        // — `projected` was just defined as that product — and would hold for
        // an inverted ratio too. This form does not: `sqrt(237/45)` conserves
        // nothing and fails here.
        assert!(
            (projected.powi(2) * PAIRED_REACHABLE_CALENDAR_SESSIONS
                - out.standard_error.powi(2) * PAIRED_HEAD_CALENDAR_SESSIONS)
                .abs()
                < 1e-12,
            "v{version}: variance x sessions is not conserved by the projection"
        );
        assert!(projected < out.standard_error, "v{version}: more sessions shrink the SE");
        // The minimum detectable paired difference at each supply level
        // (KTD11): `(z_{1−α/2} + z_power) x SE`.
        let mdd_now = (z + z_power) * out.standard_error;
        let mdd_reachable = (z + z_power) * projected;
        assert!(mdd_reachable < mdd_now, "v{version}: more sessions detect smaller differences");
        println!(
            "v{version}: minimum detectable paired difference now {mdd_now:+.6}, at the \
             reachable supply {mdd_reachable:+.6}"
        );
    }
}

// ===========================================================================
// The fixture is load-bearing, and mutating it moves exactly one arm
// ===========================================================================

#[test]
fn mutating_one_trade_moves_that_arms_net_ror_and_no_others() {
    let f = fixture();
    let baseline: BTreeMap<String, f64> =
        arms_in_order(&f).into_iter().map(|(v, a)| (v.clone(), net_ror(&a.trades))).collect();

    let target = "94";
    let mut mutated = f.arms.get(target).expect("the breakeven arm").trades.clone();
    mutated[0].realized_pnl += 1_000_000.0;
    let moved = net_ror(&mutated);
    assert!(
        (moved - baseline[target]).abs() > 1e-9,
        "a changed realized_pnl must move that arm's net RoR"
    );
    // And nothing else moved: the arms are independent slices of the fixture.
    for (v, a) in arms_in_order(&f) {
        if v != target {
            assert_eq!(
                net_ror(&a.trades),
                baseline[v],
                "v{v} moved when only v{target} was mutated"
            );
        }
    }
    // The mutated arm no longer reproduces the frozen record — which is the
    // whole point of checking the recomputation against it.
    let frozen = margin::load(&frozen_margin_path()).unwrap().values;
    let cited =
        frozen.cross_trial_arms[f.arms[target].frozen_record.cross_trial_arms_index].net_ror;
    assert_ne!(round4(moved), cited, "a mutated fixture must FAIL the frozen cross-check");
}
