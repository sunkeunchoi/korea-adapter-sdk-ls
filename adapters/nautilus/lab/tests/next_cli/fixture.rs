//! The hermetic scaffold every `lab-next` CLI scenario builds on: a tempdir
//! queue path, the env-scrubbed `Command`, and the `add` shorthand.
//!
//! Split out of the crate root when this suite crossed 1,000 lines. `hermetic`
//! is the load-bearing one — it strips the operator-shell `LS_*` variables that
//! would otherwise reach the child process and change what the report derives.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::{TimeZone, Utc};

pub fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lab-next"))
}

/// Pin the queue path and scrub every ambient seam the report reads: the
/// calendar env, the sequence stores, the clock override, and the gate leg
/// (pointed at a nonexistent status file so the developing tree's real
/// `.gate-run/state.json` can never leak into a test).
pub fn hermetic(cmd: &mut Command, queue_path: &Path) {
    cmd.env("LS_QUEUE_PATH", queue_path)
        .env("LS_GATE_STATUS_FILE", "/nonexistent/gate-status.txt")
        .env_remove("LS_CALENDAR_SNAPSHOT")
        .env_remove("LS_CALENDAR_ADOPTION")
        .env_remove("LS_DATA_HOME")
        .env_remove("LS_GOVERNED_STAGELOG")
        .env_remove("LS_TRIALS_LEDGER")
        .env_remove("LS_NEXT_NOW_UNIX");
}

/// Run `lab-next` against `queue_path` with the given args.
pub fn run(queue_path: &Path, args: &[&str]) -> Output {
    let mut cmd = bin();
    cmd.args(args);
    hermetic(&mut cmd, queue_path);
    cmd.output().unwrap()
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `add` a minimal explicit-signal item and return its id (parsed from output).
pub fn add_item(queue: &Path, id: &str, title: &str, window: &str, extra: &[&str]) {
    let mut args = vec!["add", "--id", id, "--title", title, "--window", window];
    args.extend_from_slice(extra);
    let out = run(queue, &args);
    assert_eq!(out.status.code(), Some(0), "add {id} failed: {}", stderr(&out));
}

pub fn report_cmd(queue: &Path) -> Command {
    let mut cmd = bin();
    cmd.arg("report");
    hermetic(&mut cmd, queue);
    cmd
}

/// 10:00 KST on 2026-07-16 (a Thursday inside the fixture snapshot's coverage).
pub fn open_window_ts() -> i64 {
    Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap().timestamp()
}

/// 21:00 KST on 2026-07-16 — outside the 09:00–15:30 seam: known-closed.
pub fn closed_window_ts() -> i64 {
    Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 0).unwrap().timestamp()
}

/// Write a valid TempDir-only snapshot bracketing 2026-07-16 whose mid row
/// carries `mid_status` (the trimmed `dispatch_cli.rs` fixture — synthetic,
/// no production snapshot, no network).
pub fn write_report_snapshot(
    dir: &Path,
    mid_status: nautilus_ls_calendar::schema::DayStatus,
) -> PathBuf {
    use nautilus_ls_calendar::schema::{
        Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness, Snapshot,
        SourceAvailabilityBound,
    };
    use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id};
    let d = |y, m, day| chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap();
    let mut snap = Snapshot {
        schema_version: "1.0.0".to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: None,
        scope: CalendarScope {
            calendar_name: "KRX domestic equity (SYNTHETIC)".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: true,
        },
        authorization: Authorization {
            authorized: true,
            authority: "synthetic-fixture".to_string(),
            granted_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
            expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: d(2026, 7, 15),
            materialized_through: d(2026, 7, 17),
            retrospectively_checked_through: d(2026, 7, 17),
            scheduled_closure_evaluated_through: d(2026, 7, 17),
            source_availability: vec![SourceAvailabilityBound {
                source_id: "s".to_string(),
                available_from: None,
                available_through: None,
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap(),
            holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap()),
            full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap()),
            forward_readiness_through: Some(d(2026, 12, 31)),
            last_incremental_at: Some(Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap()),
        },
        sources: vec![],
        evidence: vec![],
        alerts: vec![],
        rows: vec![
            DayRow { date: d(2026, 7, 15), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
            DayRow { date: d(2026, 7, 16), status: mid_status, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
            DayRow { date: d(2026, 7, 17), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
        ],
    };
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    let path = dir.join("calendar.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&snap).unwrap()).unwrap();
    path
}

/// R5 shape contract: inside the offer sections (`in-flight:` / `next:` /
/// `queue:`) every head line (two-space indent) is immediately followed by an
/// executable line (`resume:` or `run:` at four-space indent) — a runbook name
/// alone is never the handoff.
pub fn assert_offers_are_executable(text: &str) {
    let lines: Vec<&str> = text.lines().collect();
    let mut section = "";
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with(' ') {
            section = line.split(':').next().unwrap_or("");
            continue;
        }
        if !matches!(section, "in-flight" | "next" | "queue") {
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("    ") {
            let follow = lines.get(i + 1).copied().unwrap_or("");
            assert!(
                follow.starts_with("    resume: ") || follow.starts_with("    run: "),
                "offer head {line:?} not followed by an executable line (got {follow:?}):\n{text}"
            );
        }
    }
}
