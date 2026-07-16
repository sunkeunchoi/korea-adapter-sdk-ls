//! TRIALS ledger CLI tests (U3) — the `trials record` / `trials count` arms
//! exercised through the compiled bin with `LS_TRIALS_LEDGER` pointed at a
//! tempdir. The library mechanics (append/read/count/scrub) are unit-tested in
//! `src/trials.rs`; these assert the env wiring, exit codes, and structured
//! output. Offline; no credentials.

use std::process::Command;

use tempfile::tempdir;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lab-research"))
}

#[test]
fn record_then_count_through_the_bin_reports_totals() {
    let dir = tempdir().unwrap();
    let ledger = dir.path().join("ledger/trials.jsonl");

    // Record a gate-reading look.
    let out = bin()
        .args(["trials", "record"])
        .env("LS_TRIALS_LEDGER", &ledger)
        .env("LS_TRIAL_CANDIDATE", "ratio-atr-tilt")
        .env("LS_TRIAL_FAMILY", "class-b")
        .env("LS_TRIAL_LOOK", "gate-reading")
        .env("LS_TRIAL_FINGERPRINT", "a".repeat(64))
        .env("LS_TRIAL_VERDICT", "GO")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    // Record a flip look for the same family/lineage.
    let out = bin()
        .args(["trials", "record"])
        .env("LS_TRIALS_LEDGER", &ledger)
        .env("LS_TRIAL_CANDIDATE", "ratio-atr-tilt")
        .env("LS_TRIAL_FAMILY", "class-b")
        .env("LS_TRIAL_LOOK", "flip")
        .env("LS_TRIAL_FINGERPRINT", "a".repeat(64))
        .env("LS_TRIAL_VERDICT", "KEEP v30")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    // Count.
    let out = bin()
        .args(["trials", "count"])
        .env("LS_TRIALS_LEDGER", &ledger)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("trials total: 2"), "{stdout}");
    assert!(stdout.contains("family class-b: 2"), "{stdout}");
    // The 64-hex fingerprint renders verbatim (structured, not masked).
    assert!(stdout.contains(&"a".repeat(64)), "lineage fingerprint verbatim: {stdout}");
    assert!(!stdout.contains("***"), "structured count is not masked: {stdout}");
}

#[test]
fn count_on_an_absent_ledger_reports_zero() {
    let dir = tempdir().unwrap();
    let out = bin()
        .args(["trials", "count"])
        .env("LS_TRIALS_LEDGER", dir.path().join("nope/trials.jsonl"))
        .output()
        .unwrap();
    assert!(out.status.success(), "an absent ledger is a clean zero, not an error");
    assert!(String::from_utf8_lossy(&out.stdout).contains("trials total: 0"));
}

#[test]
fn record_with_a_missing_required_var_refuses_and_appends_nothing() {
    let dir = tempdir().unwrap();
    let ledger = dir.path().join("trials.jsonl");
    // Omit LS_TRIAL_VERDICT.
    let out = bin()
        .args(["trials", "record"])
        .env("LS_TRIALS_LEDGER", &ledger)
        .env("LS_TRIAL_CANDIDATE", "c")
        .env("LS_TRIAL_FAMILY", "class-b")
        .env("LS_TRIAL_LOOK", "flip")
        .env("LS_TRIAL_FINGERPRINT", "fp")
        .env_remove("LS_TRIAL_VERDICT")
        .output()
        .unwrap();
    assert!(!out.status.success(), "a missing required var is a loud refusal");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("LS_TRIAL_VERDICT"), "names the missing var: {stderr}");
    assert!(!ledger.exists(), "nothing appended on refusal");
}

#[test]
fn record_with_an_unknown_look_kind_refuses() {
    let dir = tempdir().unwrap();
    let out = bin()
        .args(["trials", "record"])
        .env("LS_TRIALS_LEDGER", dir.path().join("trials.jsonl"))
        .env("LS_TRIAL_CANDIDATE", "c")
        .env("LS_TRIAL_FAMILY", "class-b")
        .env("LS_TRIAL_LOOK", "bogus-look")
        .env("LS_TRIAL_FINGERPRINT", "fp")
        .env("LS_TRIAL_VERDICT", "GO")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("gate-reading"), "names the valid kinds");
}

#[test]
fn unknown_trials_subcommand_is_enumerated() {
    let out = bin().args(["trials", "bogus"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("trials count") && stderr.contains("trials record"), "{stderr}");
}

// ===========================================================================
// U8 — the committed backfill ledger (R11, AE4).
// ===========================================================================

#[test]
fn the_committed_backfill_ledger_parses_and_counts_cleanly() {
    use nautilus_ls_lab::trials::{count_trials, LookKind, TrialsLedger};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ledger/trials.jsonl");
    let ledger = TrialsLedger::new(&path);
    // Parses fully — no typed per-line errors.
    let records = ledger.read_all().expect("the committed ledger parses");

    // Hand tally from TURN-LOG: 19 statistical looks (v3→v30), including sweep
    // legs and the ATR-vol-target Phase-A STOP. Disagreement is a stop condition.
    assert_eq!(records.len(), 19, "backfill look count matches the TURN-LOG tally");

    // Every backfill record carries the marker + a source pointer.
    for r in &records {
        assert!(r.backfill, "record is marked backfill: {r:?}");
        assert!(r.source.as_deref().is_some_and(|s| s.contains("TURN-LOG.md")), "source pointer: {r:?}");
    }

    let count = count_trials(&ledger).unwrap();
    assert_eq!(count.total, 19);

    // AE4: the CLASS B family count includes the ATR-vol-target Phase-A STOP —
    // a stopped candidate that built nothing still counts as a trial.
    assert!(*count.per_family.get("class-b").unwrap() >= 1);
    assert!(
        records.iter().any(|r| r.family == "class-b"
            && matches!(r.look, LookKind::GateReading)
            && r.candidate == "atr-vol-target"
            && r.verdict.contains("STOP")),
        "the CLASS B family includes the ATR-vol-target STOP",
    );

    // Per-lineage counts split across the declared era links: three distinct era
    // fingerprints (166/157/167) merge, via their parent links, into ONE lineage.
    let distinct_fingerprints: std::collections::BTreeSet<_> =
        records.iter().map(|r| r.lineage.catalog_fingerprint.clone()).collect();
    assert_eq!(distinct_fingerprints.len(), 3, "three catalog eras present");
    assert_eq!(count.per_lineage.len(), 1, "the parent links merge the eras into one lineage");
    assert_eq!(count.per_lineage.values().next(), Some(&19), "the merged lineage counts every look");
}
