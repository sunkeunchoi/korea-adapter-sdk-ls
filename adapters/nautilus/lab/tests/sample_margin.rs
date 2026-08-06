//! Derivation guard for the FROZEN sample margin (`config/sample-margin.json`)
//! (plan 2026-08-05-001, U3; R6, KTD2/KTD3).
//!
//! The frozen numbers must be **derived and auditable, not typed in** (the
//! `prereg_derivation.rs` discipline): every recorded value is reproduced here
//! from the record's own inputs through `stats.rs`. The test stays hermetic —
//! it reads the committed margin record and the committed trials ledger, never
//! a live run dir.

use std::path::PathBuf;

use nautilus_ls_lab::margin::{self, frozen_margin_path};
use nautilus_ls_lab::stats::{expected_max_null, sample_sd, two_sided_z, MarginArm};
use nautilus_ls_lab::trials::{count_trials, TrialsLedger};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn frozen_prereg_path() -> PathBuf {
    crate_dir().join("config").join("preregistration.json")
}

fn committed_ledger() -> TrialsLedger {
    TrialsLedger::new(crate_dir().join("ledger").join("trials.jsonl"))
}

#[test]
fn the_recorded_dispersion_reproduces_from_the_recorded_per_arm_figures() {
    let m = margin::load(&frozen_margin_path()).expect("frozen sample-margin.json parses").values;
    let arms: Vec<f64> = m.cross_trial_arms.iter().map(|a| a.net_ror).collect();
    assert_eq!(arms.len(), 7, "the 2026-07-31 off-flip table has seven arms");
    assert_eq!(
        m.cross_trial_sd,
        sample_sd(&arms).unwrap(),
        "the frozen cross-trial sd is the sample sd of the recorded arms, not a typed-in number"
    );
    assert_eq!(
        m.cross_trial_sd,
        m.derived_cross_trial_sd().unwrap(),
        "and the record re-derives it the same way"
    );
}

#[test]
fn the_recorded_threshold_reproduces_from_the_recorded_inputs() {
    let m = margin::load(&frozen_margin_path()).unwrap().values;
    // E[max] straight from the closed form at the frozen (N, sigma).
    let want = expected_max_null(m.trial_count, m.cross_trial_sd).unwrap();
    assert_eq!(m.expected_max_null, want, "the frozen E[max] is the FST value at N and sigma");
    assert_eq!(m.expected_max_null, m.derived_expected_max_null().unwrap());

    // And the full rule: threshold = E[max] + z(confidence) x candidate SE.
    let candidate_se = 0.087_002;
    assert_eq!(
        m.threshold(candidate_se).unwrap(),
        want + two_sided_z(m.confidence).unwrap() * candidate_se,
        "the threshold is the frozen selection tax plus the candidate's own sampling term"
    );
}

#[test]
fn the_frozen_rule_is_reachable_at_a_larger_sample() {
    // The failure mode a frozen *level* would have: unclearable at any sample
    // size. Because the sampling term shrinks as 1/sqrt(n), the same edge that
    // fails at the head's own standard error clears at a quarter of it.
    let m = margin::load(&frozen_margin_path()).unwrap().values;
    let edge = 0.12;
    let head_se = 0.087_002;
    assert!(
        !m.adjudicate(edge, head_se, MarginArm::Armed).unwrap().clears,
        "a +0.12 net RoR does not clear at the head's own standard error"
    );
    assert!(
        m.adjudicate(edge, head_se / 4.0, MarginArm::Armed).unwrap().clears,
        "the same edge clears once the sample is ~16x larger — the bar is a rule, not a level"
    );
}

#[test]
fn the_recorded_trial_count_equals_the_ledgers_count_at_freeze_time() {
    let m = margin::load(&frozen_margin_path()).unwrap().values;
    let counted = count_trials(&committed_ledger()).expect("the committed ledger reads");
    assert_eq!(
        m.trial_ledger_records, counted.total,
        "the recorded ledger size is the ledger's actual size — sourced from \
         trials::count_trials, never hand-tallied"
    );
    assert_eq!(
        m.trial_count, counted.total,
        "and the correction is taken over every evaluated arm (see the record's \
         trial_count_basis: v35's catalog fingerprint appears in no ledger record, so \
         KTD2's lineage scoping is unavailable and the strict whole-ledger count is frozen)"
    );
    // Guard the reason, not just the number: if a future record starts carrying
    // v35's lineage, this basis note must be revisited rather than inherited.
    assert!(
        m.trial_count_basis.contains("trials::count_trials"),
        "the basis names its source: {}",
        m.trial_count_basis
    );
}

#[test]
fn a_moved_catalog_fingerprint_triggers_re_derivation_rather_than_binding_silently() {
    let m = margin::load(&frozen_margin_path()).unwrap().values;
    assert!(
        !margin::requires_rederivation(&m, &m.provenance.catalog_fingerprint),
        "the calibration catalog binds as recorded"
    );
    assert!(
        margin::requires_rederivation(&m, "363f199d4357bf665d3bed9c97c36e37551e24c89e89b0bad0b00de50d8908f4"),
        "the PRIOR catalog era does not — in-range content growth changes the trade set (AE3)"
    );
    assert!(
        !m.rederivation_trigger.is_empty(),
        "and the record states the trigger in prose for a human reader"
    );
}

#[test]
fn the_frozen_ladder_preregistration_is_byte_identical() {
    // KTD3: the sample margin lives in its own package precisely so the frozen
    // ladder pre-registration is left alone. This pins the exact bytes as of
    // 813a9e0 (the v34 re-registration), so a later edit cannot slip in under
    // this turn's cover.
    const PREREG_SHA256: &str =
        "abdb90a1f15b73d6180864e3e0c707f3be10e56b324a7d744a5bddf8122342e9";
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(frozen_prereg_path()).expect("the frozen prereg is readable");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        PREREG_SHA256,
        "config/preregistration.json must stay byte-identical (KTD3)"
    );
}

#[test]
fn the_record_carries_the_pinned_confidence_and_power_and_a_stated_rule() {
    let m = margin::load(&frozen_margin_path()).unwrap().values;
    assert_eq!(m.confidence, 0.95, "KTD11's two-sided 95%");
    assert_eq!(m.power, 0.80, "KTD11's 80% power");
    assert_eq!(m.schema_version, 1);
    for needle in ["net RoR", "E[max", "SE"] {
        assert!(m.rule.contains(needle), "the rule states {needle:?}: {}", m.rule);
    }
    assert!(m.closed_form.contains("gamma"), "the closed form is recorded: {}", m.closed_form);
    assert_eq!(m.provenance.sessions, 24, "the calibration span");
    assert_eq!(m.provenance.closed_trades, 111);
}

#[test]
fn loading_cites_the_exact_bytes() {
    // Citation integrity, mirroring the prereg loader: a byte change to the
    // frozen record changes the SHA-256 a verdict cites, so a silent edit
    // cannot masquerade under the old citation.
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    let original = std::fs::read(frozen_margin_path()).unwrap();
    let a = tmp.path().join("a.json");
    let b = tmp.path().join("b.json");
    std::fs::write(&a, &original).unwrap();
    let mutated =
        String::from_utf8(original).unwrap().replace("\"trial_count\": 29", "\"trial_count\": 3");
    std::fs::write(&b, mutated).unwrap();
    let ha = margin::load(&a).unwrap().content_hash;
    let hb = margin::load(&b).unwrap().content_hash;
    assert_ne!(ha, hb, "editing the record changes the citation");
    assert_eq!(ha, margin::load(&a).unwrap().content_hash, "same bytes → same citation");
    // …and the mutation is not cosmetic: a smaller trial count is a lower bar.
    let lowered = margin::load(&b).unwrap().values;
    assert!(
        lowered.derived_expected_max_null().unwrap()
            < margin::load(&a).unwrap().values.derived_expected_max_null().unwrap(),
        "fewer declared trials buys a lower selection tax — which is why the count is frozen"
    );
}

// ===========================================================================
// U4 — empirical calibration of the margin (R6, KTD10)
// ===========================================================================
//
// KTD10's point: a single permuted-label refusal is satisfied by any bar above
// roughly two standard errors, including one set far too low, and the
// threshold is a max-of-N-trials quantity that one draw cannot exercise. So the
// calibration generates null replicates, groups them into max-of-N blocks
// matching the threshold's own construction, and measures the rate at which a
// null block clears the margin.
//
// These tests live here rather than in `research_cli.rs` (which the plan names)
// because U3's container decision put the margin in its own package: they
// exercise the margin record and the statistics core directly, with no CLI in
// the path. `data/` is gitignored, so the v35 distribution they calibrate
// against is committed at `tests/fixtures/v35-closed-trades.json`.

mod calibration {
    use nautilus_ls_lab::margin::{self, frozen_margin_path, SampleMargin};
    use nautilus_ls_lab::stats::{
        block_bootstrap_ratio, mean, permute_r_multiples, ratio_statistic, Block, MarginArm,
        SplitMix64,
    };

    use super::crate_dir;

    /// Bootstrap settings matching `report sample`'s defaults, so the standard
    /// error here is the one the report prints.
    const REPLICATES: usize = 10_000;
    const SEED: u64 = 20_260_805;
    /// Max-of-N blocks drawn for the calibration. Fixed seed → deterministic.
    const NULL_BLOCKS: usize = 1_000;

    struct Fixture {
        /// `(realized_r, risk_capital)` grouped by KST session.
        r_blocks: Vec<Block>,
        /// `(realized_pnl, risk_capital)` grouped by KST session.
        pnl_blocks: Vec<Block>,
        sessions: usize,
        closed_trades: usize,
        risk_capital_total: f64,
        catalog_fingerprint: String,
    }

    fn v35() -> Fixture {
        let path = crate_dir().join("tests").join("fixtures").join("v35-closed-trades.json");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture is readable"))
                .expect("fixture parses");
        let mut by_session: std::collections::BTreeMap<String, (Block, Block)> =
            std::collections::BTreeMap::new();
        let mut risk_capital_total = 0.0;
        let mut closed_trades = 0;
        for t in v["trades"].as_array().expect("trades array") {
            let session = t["session"].as_str().expect("session").to_string();
            let r = t["realized_r"].as_f64().expect("realized_r");
            let rc = t["risk_capital"].as_f64().expect("risk_capital");
            let pnl = t["realized_pnl"].as_f64().expect("realized_pnl");
            let e = by_session.entry(session).or_default();
            e.0.push((r, rc));
            e.1.push((pnl, rc));
            risk_capital_total += rc;
            closed_trades += 1;
        }
        let sessions = by_session.len();
        let (r_blocks, pnl_blocks): (Vec<Block>, Vec<Block>) = by_session.into_values().unzip();
        Fixture {
            r_blocks,
            pnl_blocks,
            sessions,
            closed_trades,
            risk_capital_total,
            catalog_fingerprint: v["catalog_fingerprint"]
                .as_str()
                .expect("fingerprint")
                .to_string(),
        }
    }

    fn frozen() -> SampleMargin {
        margin::load(&frozen_margin_path()).expect("frozen margin parses").values
    }

    /// The v35 head's own session-block bootstrap standard error — the sampling
    /// term the margin is evaluated at.
    fn v35_standard_error(f: &Fixture) -> f64 {
        block_bootstrap_ratio(&f.pnl_blocks, REPLICATES, SEED, 0.95)
            .expect("the fixture bootstraps")
            .standard_error
    }

    /// The v35 blocks with the per-trade edge removed: the R-multiple multiset
    /// is centred on zero, so a replicate built from it has a **true** edge of
    /// exactly zero while keeping the observed dispersion, cluster sizes and
    /// risk-capital distribution.
    fn centred_r_blocks(f: &Fixture) -> Vec<Block> {
        let all: Vec<f64> = f.r_blocks.iter().flatten().map(|(r, _)| *r).collect();
        let m = mean(&all).expect("non-empty");
        f.r_blocks.iter().map(|b| b.iter().map(|(r, rc)| (r - m, *rc)).collect()).collect()
    }

    /// One null replicate: permute the centred R-multiples across trades, then
    /// draw one session-block resample of the result. The permutation breaks
    /// the outcome↔risk-capital pairing; the resample reproduces the sampling
    /// variation a real re-measurement would carry.
    fn null_replicate(centred: &[Block], rng: &mut SplitMix64) -> f64 {
        let permuted = permute_r_multiples(centred, rng).expect("non-empty");
        let mut num = 0.0;
        let mut den = 0.0;
        for _ in 0..permuted.len() {
            for (a, b) in &permuted[rng.below(permuted.len())] {
                num += a;
                den += b;
            }
        }
        num / den
    }

    /// The realized rate at which the maximum of `trial_count` null replicates
    /// clears `arm`'s margin verdict, over `NULL_BLOCKS` such blocks.
    /// Returns `(realized, nominal, threshold)`.
    fn null_clearance_rate(arm: MarginArm) -> (f64, f64, f64) {
        let f = v35();
        let m = frozen();
        let se = v35_standard_error(&f);
        let centred = centred_r_blocks(&f);
        let mut rng = SplitMix64::new(SEED ^ 0xC0FF_EE00);
        let mut cleared = 0usize;
        for _ in 0..NULL_BLOCKS {
            let mut best = f64::NEG_INFINITY;
            for _ in 0..m.trial_count {
                best = best.max(null_replicate(&centred, &mut rng));
            }
            if m.adjudicate(best, se, arm).expect("adjudicates").clears {
                cleared += 1;
            }
        }
        let nominal = (1.0 - m.confidence) / 2.0;
        (cleared as f64 / NULL_BLOCKS as f64, nominal, m.threshold(se).unwrap())
    }

    /// The figures `config/SAMPLE-MARGIN.md` records as measured facts. An
    /// inequality assertion alone would let the generator, seed or fixture drift
    /// while the suite stays green and the governance document goes stale, so
    /// the exact values are pinned here — the document and the test move
    /// together or the test reds.
    const DOCUMENTED_NULL_CLEARANCE: f64 = 0.0140;
    const DOCUMENTED_THRESHOLD: f64 = 0.224_823;
    const DOCUMENTED_TWO_SE_CLEARANCE: f64 = 0.1060;
    const DOCUMENTED_TWO_SE_BAR: f64 = 0.174_004;

    #[test]
    fn null_blocks_clear_the_margin_at_or_below_the_nominal_rate() {
        let (realized, nominal, threshold) = null_clearance_rate(MarginArm::Armed);
        println!(
            "null clearance: realized {realized:.4} vs nominal {nominal:.4} \
             (threshold {threshold:+.6} net RoR, {NULL_BLOCKS} max-of-N blocks)"
        );
        assert!(
            realized <= nominal,
            "realized null clearance {realized:.4} exceeds the nominal {nominal:.4}"
        );
        assert!(
            (realized - DOCUMENTED_NULL_CLEARANCE).abs() < 5e-4,
            "config/SAMPLE-MARGIN.md records a realized null clearance of \
             {DOCUMENTED_NULL_CLEARANCE:.4}; this run measured {realized:.4}. Re-measure and \
             move the document, or find out what changed the null."
        );
        assert!(
            (threshold - DOCUMENTED_THRESHOLD).abs() < 5e-6,
            "config/SAMPLE-MARGIN.md records a threshold of {DOCUMENTED_THRESHOLD:+.6} at the \
             head's own SE; this run computed {threshold:+.6}"
        );
    }

    #[test]
    fn disarming_the_margin_comparison_reds_the_null_rate_assertion() {
        // The standing falsifier. The margin comparison takes an explicit arm
        // rather than being hardwired, so a test can bypass it IN PROCESS and
        // show that the assertion above is load-bearing — no edit-and-restore,
        // which is a one-time check rather than a standing guard.
        let (realized, nominal, _) = null_clearance_rate(MarginArm::Disarmed);
        assert!(
            realized > nominal,
            "with the comparison disarmed the null-rate assertion MUST fail; it reported \
             {realized:.4} against a nominal {nominal:.4}, so the armed assertion proves nothing"
        );
        assert_eq!(realized, 1.0, "a disarmed comparison clears everything");
    }

    #[test]
    fn the_calibration_discriminates_a_bar_set_too_low() {
        // KTD10's actual concern: a single permuted-label refusal is satisfied
        // by any bar above roughly two standard errors. Measure that bar
        // directly — if it also came in under nominal, the test above would be
        // passing for free.
        let f = v35();
        let m = frozen();
        let se = v35_standard_error(&f);
        let centred = centred_r_blocks(&f);
        let too_low = 2.0 * se;
        let mut rng = SplitMix64::new(SEED ^ 0xC0FF_EE00);
        let mut cleared = 0usize;
        for _ in 0..NULL_BLOCKS {
            let mut best = f64::NEG_INFINITY;
            for _ in 0..m.trial_count {
                best = best.max(null_replicate(&centred, &mut rng));
            }
            if best > too_low {
                cleared += 1;
            }
        }
        let realized = cleared as f64 / NULL_BLOCKS as f64;
        let nominal = (1.0 - m.confidence) / 2.0;
        println!("a 2-SE bar ({too_low:+.6}) clears at {realized:.4} against nominal {nominal:.4}");
        assert!(
            realized > nominal,
            "a 2-SE bar must FAIL the calibration ({realized:.4} vs {nominal:.4}) — otherwise \
             the frozen margin's pass is not evidence about the frozen margin"
        );
        assert!(m.threshold(se).unwrap() > too_low, "and the frozen threshold sits above it");
        assert!(
            (too_low - DOCUMENTED_TWO_SE_BAR).abs() < 5e-6
                && (realized - DOCUMENTED_TWO_SE_CLEARANCE).abs() < 5e-4,
            "config/SAMPLE-MARGIN.md records a 2-SE bar of {DOCUMENTED_TWO_SE_BAR:+.6} clearing \
             at {DOCUMENTED_TWO_SE_CLEARANCE:.4}; this run measured {too_low:+.6} at \
             {realized:.4}"
        );
    }

    #[test]
    fn the_v35_head_is_refused_by_the_frozen_margin() {
        // The plan's Success Criteria names the CURRENT head, not only a
        // synthetic null: the margin is demonstrated by running v35 through it.
        let f = v35();
        let m = frozen();
        let se = v35_standard_error(&f);
        let net_ror = ratio_statistic(&f.pnl_blocks).expect("the observed ratio");
        let verdict = m.adjudicate(net_ror, se, MarginArm::Armed).unwrap();
        assert!(
            !verdict.clears,
            "v35 net RoR {net_ror:+.6} against threshold {:+.6}",
            verdict.threshold
        );
        assert!(net_ror < 0.0, "and it is net-negative to begin with: {net_ror:+.6}");
        assert!(
            !margin::requires_rederivation(&m, &f.catalog_fingerprint),
            "the fixture is the catalog the margin was frozen against, so the refusal binds"
        );
    }

    #[test]
    fn a_synthetic_head_with_a_large_true_edge_and_sufficient_n_clears_the_margin() {
        // The converse, so the refusal above is not vacuous: a head with a real
        // edge and enough data to resolve it must pass.
        let m = frozen();
        let head_se = v35_standard_error(&v35());
        let se_at_scale = head_se / 6.0; // ~36x the sample
        let real_edge = 0.15;
        let verdict = m.adjudicate(real_edge, se_at_scale, MarginArm::Armed).unwrap();
        assert!(
            verdict.clears,
            "a +0.15 net RoR at SE {se_at_scale:.6} must clear threshold {:+.6}",
            verdict.threshold
        );
        // …and the SAME edge on the thin sample must not, so what changed is
        // the evidence, not the edge.
        assert!(
            !m.adjudicate(real_edge, head_se, MarginArm::Armed).unwrap().clears,
            "the same edge on 111 clustered trades is not yet evidence"
        );
    }

    #[test]
    fn the_null_fixture_preserves_the_source_runs_session_count_and_risk_capital_total() {
        // The permutation must change ONLY the outcomes. If it moved a trade
        // between sessions or altered a risk capital, the null would be
        // calibrating a different design than the one being judged.
        let f = v35();
        assert_eq!(f.sessions, 24, "the source run's session count");
        assert_eq!(f.closed_trades, 111, "the source run's closed-trade count");
        assert!(
            (f.risk_capital_total - 27_869_870.0).abs() < 1e-6,
            "the source run's risk-capital total: {}",
            f.risk_capital_total
        );

        let centred = centred_r_blocks(&f);
        let mut rng = SplitMix64::new(11);
        let permuted = permute_r_multiples(&centred, &mut rng).unwrap();
        assert_eq!(permuted.len(), f.sessions, "session count unchanged by the permutation");
        assert_eq!(
            permuted.iter().map(Vec::len).collect::<Vec<_>>(),
            f.r_blocks.iter().map(Vec::len).collect::<Vec<_>>(),
            "every cluster keeps its size"
        );
        let permuted_risk: f64 = permuted.iter().flatten().map(|(_, rc)| *rc).sum();
        assert!(
            (permuted_risk - f.risk_capital_total).abs() < 1e-6,
            "risk-capital total unchanged: {permuted_risk}"
        );
        // The centred multiset really is centred, so the null's true edge is 0.
        let all: Vec<f64> = centred.iter().flatten().map(|(r, _)| *r).collect();
        assert!(mean(&all).unwrap().abs() < 1e-12, "the null's true per-trade edge is zero");
    }
}
