//! U6 — `lab-next probe` (R14 probe gate; KTD5): per R10 sequence (turn,
//! ladder prep, ingest, gate run) the probe verifies a readable checkpoint, a
//! derivable stage, and a printable resume command against live-shaped
//! fixtures, and records the per-sequence result the cutover verdict consumes.
//!
//! Pinned verdict semantics: an ABSENT or UNREADABLE store is a probe FAILURE
//! naming what is missing (the probe demonstrates resumability — nothing to
//! read is not demonstrated); a READABLE store is `ok` whatever it says,
//! including the rung-0 fail-closed chain verdict and not-in-flight states.
//! The probe never mutates any sequence store — its only write is the summary
//! JSON (`queue/probe-report.json`, `LS_PROBE_REPORT_PATH` override), written
//! atomically (tmp+rename). Exit 0 = all four pass, 1 otherwise.
//!
//! Every run is the compiled bin as a subprocess (the `next_cli.rs` idiom) so
//! env is isolated; fixtures are built with the real store machinery (the
//! `next_sequences.rs` idiom): a real `DispatchChain`, a real `TrialsLedger`,
//! a live-shaped ingest checkpoint, and pre-captured `gate-run.sh --status`
//! output via `LS_GATE_STATUS_FILE`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::{DateTime, TimeZone, Utc};
use nautilus_ls_lab::dispatch::chain::{
    DispatchChain, DispatchOutcome, RecordKind, SessionDispatch,
};
use nautilus_ls_lab::trials::{LookKind, SampleLineage, TrialRecord, TrialsLedger};
use tempfile::TempDir;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lab-next"))
}

/// The fixture instant: 10:00 KST on 2026-07-16 — the same trading date the
/// chain fixtures dispatch on, so a green dispatch reads as same-day Ready.
fn a_day() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap()
}

fn green_dispatch() -> SessionDispatch {
    SessionDispatch {
        outcome: DispatchOutcome::Green,
        checks: Vec::new(),
        deferrals: Vec::new(),
        readiness: None,
        unknown_override: None,
    }
}

/// A good chain: genesis + a same-day green unconsumed dispatch (Ready →
/// resumable at the mount step).
fn write_good_chain(home: &Path) {
    let chain = DispatchChain::open(home).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();
}

/// A defective chain: a good chain with a hashed body byte tampered while the
/// JSON stays valid (the `dispatch_chain.rs` idiom) — the machinery's verdict
/// is fail-closed rung 0, which is a READABLE state.
fn write_defective_chain(home: &Path) {
    let chain = DispatchChain::open(home).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();
    let text = std::fs::read_to_string(chain.chain_path()).unwrap();
    let tampered = text.replacen("\"chain_rung\":1", "\"chain_rung\":4", 1);
    assert_ne!(text, tampered, "the tamper must have taken");
    std::fs::write(chain.chain_path(), tampered).unwrap();
}

/// A live-shaped ingest checkpoint with watermarks and a basis-shift mark.
fn write_checkpoint(home: &Path) {
    let catalog = home.join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(
        catalog.join("ingest-checkpoint.json"),
        r#"{
            "completed": [],
            "gaps": [],
            "watermarks": {
                "KR7005930003.KRX|1-DAY-LAST": "20260725",
                "KR7000660001.KRX|1-DAY-LAST": "20260728"
            },
            "shifted": { "KR7005930003.KRX|1-DAY-LAST": "20260720" },
            "adjusted_prices": true
        }"#,
    )
    .unwrap();
}

/// A mid-way turn: a stage log recorded through `rebaseline` plus a trials
/// ledger whose last look names the candidate. Returns (stage_log, ledger).
fn write_turn_fixtures(dir: &Path) -> (PathBuf, PathBuf) {
    let stage_log = dir.join("stagelog.txt");
    std::fs::write(&stage_log, "bump\nrebaseline\n").unwrap();
    let ledger_path = dir.join("ledger/trials.jsonl");
    TrialsLedger::new(&ledger_path)
        .append(&TrialRecord::new(
            "orb-slot-ranking",
            "class-b",
            LookKind::GateReading,
            SampleLineage { catalog_fingerprint: "f".repeat(64), parent_fingerprint: None },
            BTreeMap::new(),
            "GO",
            "2026-07-16T00:30:00+00:00",
        ))
        .unwrap();
    (stage_log, ledger_path)
}

/// Pre-captured in-flight `gate-run.sh --status` output: 2/6 done.
fn write_gate_status(dir: &Path) -> PathBuf {
    let status = dir.join("gate-status.txt");
    std::fs::write(
        &status,
        "step=1 name=docs status=done fingerprint=aaaa\n\
         step=2 name=cargo-test status=done fingerprint=bbbb\n\
         step=3 name=cargo-test-ls-core status=pending fingerprint=-\n\
         step=4 name=docs-check status=pending fingerprint=-\n\
         step=5 name=lane-check status=pending fingerprint=-\n\
         step=6 name=adapter-check status=pending fingerprint=-\n\
         next=cargo-test-ls-core\n",
    )
    .unwrap();
    status
}

/// One hermetic `lab-next probe` invocation: every seam pinned, every ambient
/// `LS_*` the bin reads scrubbed.
struct ProbeEnv {
    report: PathBuf,
    gate_status: PathBuf,
    trials_ledger: PathBuf,
    data_home: Option<PathBuf>,
    stage_log: Option<PathBuf>,
}

impl ProbeEnv {
    fn run(&self) -> Output {
        let mut cmd = bin();
        cmd.arg("probe")
            .env("LS_PROBE_REPORT_PATH", &self.report)
            .env("LS_GATE_STATUS_FILE", &self.gate_status)
            .env("LS_TRIALS_LEDGER", &self.trials_ledger)
            .env("LS_NEXT_NOW_UNIX", a_day().timestamp().to_string())
            .env("LS_QUEUE_PATH", self.report.with_file_name("unused-items.jsonl"))
            .env_remove("LS_CALENDAR_SNAPSHOT")
            .env_remove("LS_CALENDAR_ADOPTION")
            .env_remove("LS_DATA_HOME")
            .env_remove("LS_GOVERNED_STAGELOG");
        if let Some(home) = &self.data_home {
            cmd.env("LS_DATA_HOME", home);
        }
        if let Some(log) = &self.stage_log {
            cmd.env("LS_GOVERNED_STAGELOG", log);
        }
        cmd.output().unwrap()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The all-good fixture set: every store present and readable.
fn all_good(tmp: &TempDir) -> ProbeEnv {
    let home = tmp.path().join("home");
    write_good_chain(&home);
    write_checkpoint(&home);
    let (stage_log, ledger) = write_turn_fixtures(tmp.path());
    ProbeEnv {
        report: tmp.path().join("report/probe-report.json"),
        gate_status: write_gate_status(tmp.path()),
        trials_ledger: ledger,
        data_home: Some(home),
        stage_log: Some(stage_log),
    }
}

fn parse_report(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

// ---------------------------------------------------------------------------
// All fixtures good → four ok lines, report written atomically, exit 0.
// ---------------------------------------------------------------------------

#[test]
fn all_fixtures_good_prints_four_ok_lines_and_writes_the_report_atomically() {
    let tmp = TempDir::new().unwrap();
    let env = all_good(&tmp);
    let out = env.run();
    assert_eq!(out.status.code(), Some(0), "all pass → exit 0: {}", stderr(&out));

    let text = stdout(&out);
    // One ok[...] line per R10 sequence, in the fixed order, no FAIL lines.
    let ok_at = |seq: &str| {
        text.find(&format!("ok[{seq}] ")).unwrap_or_else(|| panic!("no ok[{seq}] line:\n{text}"))
    };
    let (t, l, i, g) = (ok_at("turn"), ok_at("ladder"), ok_at("ingest"), ok_at("gate-run"));
    assert!(t < l && l < i && i < g, "R10 sequence order (turn, ladder, ingest, gate-run):\n{text}");
    assert!(!text.contains("FAIL["), "no FAIL lines when everything reads:\n{text}");
    assert!(text.contains("probe: PASS"), "the summary verdict line:\n{text}");

    // Each ok line carries the derived stage and the printable resume command.
    let line = |seq: &str| text.lines().find(|l| l.starts_with(&format!("ok[{seq}]"))).unwrap();
    assert!(line("turn").contains("rebaseline"), "turn stage from the stage log:\n{text}");
    assert!(line("turn").contains("turn governed"), "turn resume (KTD7 one-shot):\n{text}");
    assert!(line("ladder").contains("unconsumed"), "ladder stage:\n{text}");
    assert!(line("ladder").contains("--mount"), "ladder resume:\n{text}");
    assert!(line("ingest").contains("20260728"), "ingest watermark frontier:\n{text}");
    assert!(line("ingest").contains("ls-ingest"), "ingest resume:\n{text}");
    assert!(line("gate-run").contains("cargo-test-ls-core"), "gate stage names the next step:\n{text}");
    assert!(line("gate-run").contains("make gate-run"), "gate resume:\n{text}");

    // The summary JSON: written, correct shape, no tmp residue beside it.
    let report = parse_report(&env.report);
    assert_eq!(report["version"], 1, "{report}");
    assert_eq!(report["all_pass"], true, "{report}");
    assert!(report["probed_utc"].as_str().unwrap().starts_with("2026-07-16T01:00:00"), "{report}");
    let rows = report["sequences"].as_array().unwrap();
    let names: Vec<&str> = rows.iter().map(|r| r["sequence"].as_str().unwrap()).collect();
    assert_eq!(names, ["turn", "ladder", "ingest", "gate-run"], "{report}");
    for row in rows {
        assert_eq!(row["verdict"], "ok", "{row}");
        assert!(!row["stage"].as_str().unwrap().is_empty(), "{row}");
        assert!(!row["resume"].as_str().unwrap().is_empty(), "{row}");
        assert!(row.get("missing").is_none(), "an ok row carries no missing field: {row}");
    }
    let siblings: Vec<String> = std::fs::read_dir(env.report.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(siblings, ["probe-report.json"], "no .tmp residue after the atomic write");
}

// ---------------------------------------------------------------------------
// Gate state absent → FAIL[gate-run] naming the missing state; others probe.
// ---------------------------------------------------------------------------

#[test]
fn absent_gate_state_fails_the_gate_leg_naming_it_while_the_others_still_probe() {
    let tmp = TempDir::new().unwrap();
    let mut env = all_good(&tmp);
    env.gate_status = tmp.path().join("no-such-gate-status.txt"); // never written

    let out = env.run();
    assert_eq!(out.status.code(), Some(1), "a failed leg → exit 1: {}", stderr(&out));

    let text = stdout(&out);
    let fail = text
        .lines()
        .find(|l| l.starts_with("FAIL[gate-run]"))
        .unwrap_or_else(|| panic!("no FAIL[gate-run] line:\n{text}"));
    assert!(
        fail.contains("no-such-gate-status.txt"),
        "the FAIL line names the missing state:\n{text}"
    );
    // The other three legs still probed and read fine.
    for seq in ["turn", "ladder", "ingest"] {
        assert!(text.contains(&format!("ok[{seq}] ")), "ok[{seq}] still probes:\n{text}");
    }
    assert!(text.contains("probe: FAIL"), "the summary verdict line:\n{text}");

    // The report JSON records the fail.
    let report = parse_report(&env.report);
    assert_eq!(report["all_pass"], false, "{report}");
    let gate = report["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["sequence"] == "gate-run")
        .unwrap();
    assert_eq!(gate["verdict"], "fail", "{gate}");
    assert!(gate["missing"].as_str().unwrap().contains("no-such-gate-status.txt"), "{gate}");
}

// ---------------------------------------------------------------------------
// Defective chain → ok[ladder] noting the rung-0 fail-closed verdict.
// ---------------------------------------------------------------------------

#[test]
fn a_defective_chain_probes_ok_noting_the_rung0_fail_closed_verdict() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    write_defective_chain(&home);
    write_checkpoint(&home);
    let (stage_log, ledger) = write_turn_fixtures(tmp.path());
    let env = ProbeEnv {
        report: tmp.path().join("report/probe-report.json"),
        gate_status: write_gate_status(tmp.path()),
        trials_ledger: ledger,
        data_home: Some(home),
        stage_log: Some(stage_log),
    };

    let out = env.run();
    // Fail-closed is a READABLE state, not a probe failure: all four pass.
    assert_eq!(out.status.code(), Some(0), "{}\n{}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    let ladder = text
        .lines()
        .find(|l| l.starts_with("ok[ladder]"))
        .unwrap_or_else(|| panic!("no ok[ladder] line:\n{text}"));
    assert!(
        ladder.contains("fail-closed rung 0") && ladder.contains("chain defective"),
        "the rung-0 fail-closed verdict is noted on the ok line:\n{text}"
    );
    assert!(ladder.contains("--reregister"), "the repair resume step is printable:\n{text}");
    assert_eq!(parse_report(&env.report)["all_pass"], true);
}

// ---------------------------------------------------------------------------
// Fresh env (no stores anywhere) → all four FAIL naming what's missing.
// ---------------------------------------------------------------------------

#[test]
fn stores_absent_entirely_fail_all_four_legs_naming_whats_missing_without_erroring() {
    let tmp = TempDir::new().unwrap();
    let env = ProbeEnv {
        report: tmp.path().join("report/probe-report.json"),
        gate_status: tmp.path().join("no-gate-status.txt"),
        trials_ledger: tmp.path().join("ledger/never-written.jsonl"),
        data_home: Some(tmp.path().join("empty-home")), // exists as a path, hosts nothing
        stage_log: None,
    };

    let out = env.run();
    assert_eq!(out.status.code(), Some(1), "all-fail is exit 1, never a crash: {}", stderr(&out));
    assert!(!stderr(&out).contains("panicked"), "the probe never panics: {}", stderr(&out));

    let text = stdout(&out);
    for seq in ["turn", "ladder", "ingest", "gate-run"] {
        assert!(text.contains(&format!("FAIL[{seq}]")), "FAIL[{seq}] expected:\n{text}");
    }
    let fail_line = |seq: &str| {
        text.lines().find(|l| l.starts_with(&format!("FAIL[{seq}]"))).unwrap().to_string()
    };
    assert!(fail_line("turn").contains("never-written.jsonl"), "turn names the absent ledger:\n{text}");
    assert!(fail_line("ladder").contains("chain.jsonl"), "ladder names the absent chain:\n{text}");
    assert!(
        fail_line("ingest").contains("ingest-checkpoint.json"),
        "ingest names the absent checkpoint:\n{text}"
    );
    assert!(
        fail_line("gate-run").contains("no-gate-status.txt"),
        "gate names the absent state:\n{text}"
    );
    assert!(text.contains("probe: FAIL"), "{text}");

    let report = parse_report(&env.report);
    assert_eq!(report["all_pass"], false, "{report}");
    for row in report["sequences"].as_array().unwrap() {
        assert_eq!(row["verdict"], "fail", "{row}");
        assert!(!row["missing"].as_str().unwrap().is_empty(), "{row}");
    }
}

// ---------------------------------------------------------------------------
// The probe never mutates any sequence store.
// ---------------------------------------------------------------------------

#[test]
fn the_probe_never_mutates_any_sequence_store() {
    let tmp = TempDir::new().unwrap();
    let env = all_good(&tmp);
    let home = env.data_home.clone().unwrap();

    let store_files = [
        home.join("dispatch/chain.jsonl"),
        home.join("catalog/ingest-checkpoint.json"),
        env.trials_ledger.clone(),
        env.stage_log.clone().unwrap(),
        env.gate_status.clone(),
    ];
    let before: Vec<Vec<u8>> = store_files.iter().map(|p| std::fs::read(p).unwrap()).collect();
    let dir_listing = |dir: &Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    };
    let dispatch_before = dir_listing(&home.join("dispatch"));
    let catalog_before = dir_listing(&home.join("catalog"));

    let out = env.run();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    for (path, bytes) in store_files.iter().zip(&before) {
        assert_eq!(
            &std::fs::read(path).unwrap(),
            bytes,
            "store bytes unchanged: {}",
            path.display()
        );
    }
    assert_eq!(dir_listing(&home.join("dispatch")), dispatch_before, "no new dispatch files");
    assert_eq!(dir_listing(&home.join("catalog")), catalog_before, "no new catalog files");
}
