//! `turn diagnose` tests (U5, R2/R3; AE3). The diagnose library function is
//! driven directly with stub `/bin/sh` scripts — no `uv`, no network — and an
//! injected freeze commit (the git freeze check is covered in the candidates
//! unit tests). Asserts twin-mismatch / threshold / GO verdicts, script-failure
//! typing, the recorded hashes, and the ledger-first append.

use std::path::Path;

use nautilus_ls_lab::artifacts::manifest::hash_bytes;
use nautilus_ls_lab::runner::diagnose::{diagnose, read_gate_verdict, DiagnoseConfig, GateExit};
use nautilus_ls_lab::trials::TrialsLedger;
use serde_json::json;
use tempfile::TempDir;

/// Author a candidate whose diagnostic + twin are `/bin/sh` stubs emitting the
/// given shell bodies (the wrapper appends the readings output path as `$1`).
/// The single threshold gates `collinearity_r comparator value`.
fn author(dir: &Path, diag_body: &str, twin_body: &str, comparator: &str, value: f64) {
    std::fs::create_dir_all(dir).unwrap();
    let diag = format!("#!/bin/sh\n{diag_body}\n");
    let twin = format!("#!/bin/sh\n{twin_body}\n");
    std::fs::write(dir.join("diag.sh"), &diag).unwrap();
    std::fs::write(dir.join("twin.sh"), &twin).unwrap();
    let v = json!({
        "schema_version": 1,
        "slug": "cand",
        "family": "class-b",
        "phase_a": "bespoke",
        "flip_param": "ratio_atr_alpha",
        "flip_value": 0.5,
        "diagnostic": { "argv": ["sh", "diag.sh"], "file": "diag.sh", "content_hash": hash_bytes(diag.as_bytes()) },
        "twin": { "argv": ["sh", "twin.sh"], "file": "twin.sh", "content_hash": hash_bytes(twin.as_bytes()) },
        "readings": { "collinearity_r": { "tolerance": 0.01, "precision": 4 } },
        "thresholds": [ { "reading": "collinearity_r", "comparator": comparator, "value": value } ],
        "keep_anchor": "return-on-risk strict flip PASS"
    });
    std::fs::write(dir.join("candidate.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

fn cfg(tmp: &TempDir, dir: &Path) -> DiagnoseConfig {
    DiagnoseConfig {
        candidate_dir: dir.to_path_buf(),
        ledger: TrialsLedger::new(tmp.path().join("ledger/trials.jsonl")),
        anchor_fingerprint: "fp-anchor".to_string(),
        parent_fingerprint: None,
        freeze_commit: Some("commit-abc123".to_string()),
        recorded_utc: "2026-07-16T00:00:00+00:00".to_string(),
    }
}

fn emit(value: &str) -> String {
    format!("echo '{{\"collinearity_r\": {value}}}' > \"$1\"")
}

#[test]
fn ae3_twin_mismatch_stops_with_the_discrepancy_and_appends_a_trial() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    // Diagnostic -0.36, twin -0.50: |diff| = 0.14 > tolerance 0.01 → STOP.
    author(&dir, &emit("-0.36"), &emit("-0.50"), "lt", 0.7);
    let c = cfg(&tmp, &dir);
    let out = diagnose(&c).unwrap();

    assert!(!out.go, "twin mismatch is a STOP");
    assert_eq!(out.exit, GateExit::TwinMismatch);
    // The gate verdict records STOP (no GO), with the discrepancy named.
    let verdict = read_gate_verdict(&dir).unwrap().unwrap();
    assert_eq!(verdict.decision, "STOP");
    assert_eq!(verdict.stop_gate.as_deref(), Some("twin-mismatch"));
    assert!(verdict.stop_reason.unwrap().contains("disagrees"), "names the discrepancy");
    // The trial appears in the ledger (AE3).
    assert_eq!(c.ledger.read_all().unwrap().len(), 1, "the stopped look is still a trial");
}

#[test]
fn agreeing_but_failing_a_threshold_stops_naming_the_threshold() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    // Both 0.90; threshold requires < 0.7 → fails.
    author(&dir, &emit("0.90"), &emit("0.90"), "lt", 0.7);
    let out = diagnose(&cfg(&tmp, &dir)).unwrap();
    assert_eq!(out.exit, GateExit::ThresholdFail);
    let verdict = read_gate_verdict(&dir).unwrap().unwrap();
    assert_eq!(verdict.stop_gate.as_deref(), Some("threshold-fail"));
    assert!(verdict.stop_reason.unwrap().contains("collinearity_r"), "names the failing reading");
}

#[test]
fn agreeing_and_passing_writes_a_go_with_all_recorded_hashes() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    author(&dir, &emit("-0.36"), &emit("-0.36"), "lt", 0.7);
    let c = cfg(&tmp, &dir);
    let out = diagnose(&c).unwrap();
    assert!(out.go, "agreeing + passing is a GO: {:?}", out.lines);
    assert_eq!(out.exit, GateExit::Ok);
    let verdict = read_gate_verdict(&dir).unwrap().unwrap();
    assert_eq!(verdict.decision, "GO");
    // All the recorded hashes are present.
    assert_eq!(verdict.catalog_fingerprint, "fp-anchor");
    assert_eq!(verdict.freeze_commit.as_deref(), Some("commit-abc123"));
    assert_eq!(verdict.pre_register_hash.len(), 64, "the pre-register content hash rides the verdict");
    assert!(verdict.flip_param.is_some());
    assert_eq!(c.ledger.read_all().unwrap().len(), 1, "a GO reading is a trial too");
}

#[test]
fn a_script_exiting_nonzero_is_a_typed_failure_with_no_verdict() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    author(&dir, "exit 1", &emit("-0.36"), "lt", 0.7);
    let c = cfg(&tmp, &dir);
    let out = diagnose(&c).unwrap();
    assert_eq!(out.exit, GateExit::ScriptFailure);
    assert!(out.gate_verdict_path.is_none(), "no verdict written on a script failure");
    assert!(read_gate_verdict(&dir).unwrap().is_none(), "no gate-verdict.json");
    assert!(c.ledger.read_all().unwrap().is_empty(), "nothing measured → no trial");
    assert!(out.lines.iter().any(|l| l.contains("diagnostic script failure")), "names the stage");
}

#[test]
fn malformed_json_or_a_missing_key_is_a_typed_failure() {
    let tmp = TempDir::new().unwrap();
    // Malformed JSON.
    let bad = tmp.path().join("bad");
    author(&bad, "echo 'not json' > \"$1\"", &emit("-0.36"), "lt", 0.7);
    assert_eq!(diagnose(&cfg(&tmp, &bad)).unwrap().exit, GateExit::ScriptFailure);

    // Omits the declared reading key.
    let omit = tmp.path().join("omit");
    author(&omit, "echo '{}' > \"$1\"", &emit("-0.36"), "lt", 0.7);
    let out = diagnose(&cfg(&tmp, &omit)).unwrap();
    assert_eq!(out.exit, GateExit::ScriptFailure);
    assert!(out.lines.iter().any(|l| l.contains("omit")), "names the omission: {:?}", out.lines);
}

#[test]
fn re_running_after_editing_the_pre_register_records_the_new_hash() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    author(&dir, &emit("-0.36"), &emit("-0.36"), "lt", 0.7);
    diagnose(&cfg(&tmp, &dir)).unwrap();
    let first_hash = read_gate_verdict(&dir).unwrap().unwrap().pre_register_hash;

    // Edit the pre-register (soften the threshold) and re-run.
    author(&dir, &emit("-0.36"), &emit("-0.36"), "lt", 0.9);
    diagnose(&cfg(&tmp, &dir)).unwrap();
    let second_hash = read_gate_verdict(&dir).unwrap().unwrap().pre_register_hash;
    assert_ne!(first_hash, second_hash, "the edited pre-register has a new content hash (U6 asserts the refusal)");
}

#[test]
fn a_minimal_phase_a_candidate_is_an_immediate_go_with_no_scripts() {
    // A minimal-class (independent-signal) candidate needs no diagnostic/twin: the
    // freshness/reconcile-only Phase-A records a GO directly (R4).
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    std::fs::create_dir_all(&dir).unwrap();
    let v = json!({
        "schema_version": 1, "slug": "cand", "family": "class-b", "phase_a": "minimal",
        "flip_param": "ratio_atr_alpha", "flip_value": 0.5,
        "keep_anchor": "return-on-risk strict flip PASS"
    });
    std::fs::write(dir.join("candidate.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let c = cfg(&tmp, &dir);
    let out = diagnose(&c).unwrap();
    assert!(out.go, "minimal Phase-A is an immediate GO");
    assert_eq!(out.exit, GateExit::Ok);
    let verdict = read_gate_verdict(&dir).unwrap().unwrap();
    assert_eq!(verdict.decision, "GO");
    assert!(verdict.agreed_readings.is_empty(), "no readings for a minimal Phase-A");
    assert_eq!(c.ledger.read_all().unwrap().len(), 1, "the minimal GO is still a trial");
}

#[test]
fn a_gate_verdict_with_an_unsupported_schema_version_is_refused_on_read() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    author(&dir, &emit("-0.36"), &emit("-0.36"), "lt", 0.7);
    diagnose(&cfg(&tmp, &dir)).unwrap();
    // Bump the written verdict's schema version and confirm the reader rejects it.
    let path = dir.join("gate-verdict.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["schema_version"] = json!(999);
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
    let err = read_gate_verdict(&dir).unwrap_err();
    assert!(err.to_string().contains("schema version 999"), "{err}");
}

#[test]
fn a_planted_env_secret_never_appears_in_the_verdict_or_ledger_bytes() {
    // Uniform scrub discipline: diagnose never leaks ambient env into its
    // artifacts, and the one free-text carrier (stop_reason) routes through the
    // scrub. Here a twin-mismatch reason is written; assert no account-like token
    // from the environment reaches disk.
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("cand");
    author(&dir, &emit("-0.36"), &emit("-0.50"), "lt", 0.7);
    std::env::set_var("LS_PLANTED_SECRET", "acct 20187511401");
    let c = cfg(&tmp, &dir);
    diagnose(&c).unwrap();
    std::env::remove_var("LS_PLANTED_SECRET");

    let verdict_bytes = std::fs::read_to_string(dir.join("gate-verdict.json")).unwrap();
    let ledger_bytes = std::fs::read_to_string(c.ledger.path()).unwrap();
    assert!(!verdict_bytes.contains("20187511401"), "no env secret in the verdict: {verdict_bytes}");
    assert!(!ledger_bytes.contains("20187511401"), "no env secret in the ledger: {ledger_bytes}");
}
