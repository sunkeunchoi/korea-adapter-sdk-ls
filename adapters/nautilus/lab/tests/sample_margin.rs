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
