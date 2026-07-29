//! `lab-next` queue edit-surface tests (U1, R6/R8/R9; KTD2/KTD3/KTD6; AE3)
//! plus the default window-aware report (U5, R1/R2/R4/R5/R12/R13; KTD1).
//! Every run is the compiled bin as a subprocess so env is isolated; the queue
//! file is a tempdir path via `LS_QUEUE_PATH` (KTD2's test-time override).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::{TimeZone, Utc};
use tempfile::TempDir;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lab-next"))
}

/// Pin the queue path and scrub every ambient seam the report reads: the
/// calendar env, the sequence stores, the clock override, and the gate leg
/// (pointed at a nonexistent status file so the developing tree's real
/// `.gate-run/state.json` can never leak into a test).
fn hermetic(cmd: &mut Command, queue_path: &Path) {
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
fn run(queue_path: &Path, args: &[&str]) -> Output {
    let mut cmd = bin();
    cmd.args(args);
    hermetic(&mut cmd, queue_path);
    cmd.output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `add` a minimal explicit-signal item and return its id (parsed from output).
fn add_item(queue: &Path, id: &str, title: &str, window: &str, extra: &[&str]) {
    let mut args = vec!["add", "--id", id, "--title", title, "--window", window];
    args.extend_from_slice(extra);
    let out = run(queue, &args);
    assert_eq!(out.status.code(), Some(0), "add {id} failed: {}", stderr(&out));
}

// ---------------------------------------------------------------------------
// Happy path (R6, R8)
// ---------------------------------------------------------------------------

#[test]
fn add_then_list_shows_the_item_with_its_window_tag() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("queue/items.jsonl");
    add_item(&queue, "gate-rerun", "Re-run the commit gate", "closed", &[]);

    let out = run(&queue, &["list"]);
    assert_eq!(out.status.code(), Some(0), "list failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("actionable: 1"), "{text}");
    assert!(text.contains("gate-rerun"), "{text}");
    assert!(text.contains("[closed]"), "window tag rendered: {text}");
    assert!(text.contains("Re-run the commit gate"), "{text}");
}

#[test]
fn done_removes_an_explicit_item_from_the_actionable_view() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "adjudicate-skills", "Adjudicate skills dirs", "any", &[]);

    let out = run(&queue, &["done", "adjudicate-skills"]);
    assert_eq!(out.status.code(), Some(0), "done failed: {}", stderr(&out));

    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 0"), "{text}");
    assert!(!text.contains("adjudicate-skills"), "done item out of the view: {text}");
    // The item is retained in the store (tool-owned history), not erased.
    let all = stdout(&run(&queue, &["list", "--all"]));
    assert!(all.contains("adjudicate-skills"), "history retained: {all}");
}

#[test]
fn a_failed_write_leaves_no_partial_file_and_the_queue_unchanged() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("q");
    std::fs::create_dir_all(&dir).unwrap();
    let queue = dir.join("items.jsonl");
    add_item(&queue, "first", "First item", "any", &[]);
    let before = std::fs::read_to_string(&queue).unwrap();

    // Simulate a write failure: the queue's parent refuses new files, so the
    // sibling tmp write fails before any rename could land.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let out = run(&queue, &["add", "--id", "second", "--title", "Second", "--window", "any"]);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_ne!(out.status.code(), Some(0), "the failed write must not exit 0");
    assert_eq!(std::fs::read_to_string(&queue).unwrap(), before, "queue bytes unchanged");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n != "items.jsonl")
        .collect();
    assert!(leftovers.is_empty(), "no partial/tmp file left behind: {leftovers:?}");
}

// ---------------------------------------------------------------------------
// AE3 + destructive-transition hygiene (KTD6)
// ---------------------------------------------------------------------------

#[test]
fn ae3_a_tool_event_done_with_its_artifact_present_leaves_the_view_with_no_hand_edit() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    let artifact = tmp.path().join("ingest-checkpoint.json");
    std::fs::write(&artifact, "{\"completed\":[\"x\"]}").unwrap();
    add_item(
        &queue,
        "ingest-window",
        "Ingest the 0728 window",
        "closed",
        &["--event", "ingest-complete", "--artifact", artifact.to_str().unwrap()],
    );

    let out = run(&queue, &["done", "ingest-window"]);
    assert_eq!(out.status.code(), Some(0), "tool-event done: {}", stderr(&out));
    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 0"), "item left the view via the tool: {text}");
}

#[test]
fn done_with_the_declared_artifact_absent_stays_actionable_flagged_for_reconcile() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    let artifact = tmp.path().join("never-written.json");
    add_item(
        &queue,
        "ingest-window",
        "Ingest the 0728 window",
        "closed",
        &["--event", "ingest-complete", "--artifact", artifact.to_str().unwrap()],
    );

    let out = run(&queue, &["done", "ingest-window"]);
    assert_ne!(out.status.code(), Some(0), "an absent artifact never completes done");
    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 1"), "item stays actionable: {text}");
    assert!(text.contains("reconcile"), "flagged for reconcile: {text}");
}

#[test]
fn done_with_the_declared_artifact_empty_stays_actionable_flagged_for_reconcile() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    let artifact = tmp.path().join("empty.json");
    std::fs::write(&artifact, "").unwrap();
    add_item(
        &queue,
        "ingest-window",
        "Ingest the 0728 window",
        "closed",
        &["--event", "ingest-complete", "--artifact", artifact.to_str().unwrap()],
    );

    let out = run(&queue, &["done", "ingest-window"]);
    assert_ne!(out.status.code(), Some(0), "an empty artifact never completes done");
    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 1"), "{text}");
    assert!(text.contains("reconcile"), "{text}");
}

// ---------------------------------------------------------------------------
// Staleness (R9)
// ---------------------------------------------------------------------------

#[test]
fn an_item_past_its_deadline_is_stale_and_leaves_the_actionable_view() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "old", "Expired chore", "closed", &["--deadline", "2020-01-01T00:00:00Z"]);
    add_item(&queue, "live", "Current chore", "closed", &["--deadline", "2100-01-01T00:00:00Z"]);

    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 1"), "{text}");
    assert!(!text.contains("Expired chore"), "past-deadline item out of the view: {text}");
    assert!(text.contains("Current chore"), "future-deadline item stays: {text}");
}

#[test]
fn a_paused_in_flight_sequence_entry_is_never_stale() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(
        &queue,
        "turn-resume",
        "Resume the governed turn",
        "closed",
        &["--deadline", "2020-01-01T00:00:00Z", "--sequence", "turn"],
    );

    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 1"), "a paused sequence never goes stale: {text}");
    assert!(text.contains("turn-resume"), "{text}");
}

// ---------------------------------------------------------------------------
// Supersede (R9, KTD6)
// ---------------------------------------------------------------------------

#[test]
fn supersede_by_an_existing_item_sets_superseded_by_and_leaves_the_view() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "plan-a", "Old plan", "closed", &[]);
    add_item(&queue, "plan-b", "New plan", "closed", &[]);

    let out = run(&queue, &["supersede", "plan-a", "--by", "plan-b"]);
    assert_eq!(out.status.code(), Some(0), "supersede failed: {}", stderr(&out));

    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 1"), "{text}");
    assert!(!text.contains("Old plan"), "superseded item out of the view: {text}");
    // The link is recorded on the target.
    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(raw.contains("\"superseded_by\":\"plan-b\""), "{raw}");
}

#[test]
fn supersede_by_a_nonexistent_item_leaves_the_target_actionable_flagged_for_reconcile() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "plan-a", "Old plan", "closed", &[]);

    let out = run(&queue, &["supersede", "plan-a", "--by", "plan-z"]);
    assert_ne!(out.status.code(), Some(0), "a missing superseder never completes the transition");

    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 1"), "target stays actionable: {text}");
    assert!(text.contains("reconcile"), "flagged for reconcile: {text}");
    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(!raw.contains("\"superseded_by\""), "no link recorded: {raw}");
}

// ---------------------------------------------------------------------------
// Malformed queue (typed error, no rewrite)
// ---------------------------------------------------------------------------

#[test]
fn a_malformed_jsonl_line_is_a_typed_error_naming_the_line_and_the_queue_is_not_rewritten() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "ok-item", "Fine item", "any", &[]);
    let mut raw = std::fs::read_to_string(&queue).unwrap();
    raw.push_str("{\"this is\": not json\n");
    std::fs::write(&queue, &raw).unwrap();

    for args in [vec!["list"], vec!["done", "ok-item"], vec!["add", "--id", "x", "--title", "X", "--window", "any"]] {
        let out = run(&queue, &args);
        assert_ne!(out.status.code(), Some(0), "{args:?} must refuse a malformed queue");
        let err = stderr(&out);
        assert!(err.contains("line 2"), "{args:?} names the malformed line: {err}");
        assert_eq!(std::fs::read_to_string(&queue).unwrap(), raw, "{args:?} must not rewrite the queue");
    }
}

// ---------------------------------------------------------------------------
// Edit-surface errors
// ---------------------------------------------------------------------------

#[test]
fn done_and_supersede_on_an_unknown_id_are_errors() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "only", "Only item", "any", &[]);

    let out = run(&queue, &["done", "ghost"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("ghost"), "{}", stderr(&out));

    let out = run(&queue, &["supersede", "ghost", "--by", "only"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("ghost"), "{}", stderr(&out));
}

#[test]
fn add_refuses_a_duplicate_id_and_an_unparseable_deadline_or_window() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "dup", "First", "any", &[]);

    let out = run(&queue, &["add", "--id", "dup", "--title", "Again", "--window", "any"]);
    assert_ne!(out.status.code(), Some(0), "duplicate id refused");
    assert!(stderr(&out).contains("dup"), "{}", stderr(&out));

    let out = run(&queue, &["add", "--id", "w", "--title", "T", "--window", "sideways"]);
    assert_ne!(out.status.code(), Some(0), "unknown window refused");

    let out =
        run(&queue, &["add", "--id", "d", "--title", "T", "--window", "any", "--deadline", "tomorrow"]);
    assert_ne!(out.status.code(), Some(0), "unparseable deadline refused");

    // Nothing after the first item ever landed.
    assert_eq!(std::fs::read_to_string(&queue).unwrap().lines().count(), 1);
}

#[test]
fn the_default_invocation_is_the_report_and_unknown_subcommands_are_usage_errors() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");

    // Bare invocation = the report; with no snapshot configured the report is
    // the genuinely-unknown fail-closed shape (R3), exit 0.
    let out = run(&queue, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("window: genuinely-unknown"), "{}", stdout(&out));

    let out = run(&queue, &["frobnicate"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("usage"), "{}", stderr(&out));
}

// ---------------------------------------------------------------------------
// U5 — the default window-aware report (R1/R2/R4/R5/R12/R13; KTD1/KTD3)
// ---------------------------------------------------------------------------

/// A hermetic `report` invocation (see [`hermetic`]).
fn report_cmd(queue: &Path) -> Command {
    let mut cmd = bin();
    cmd.arg("report");
    hermetic(&mut cmd, queue);
    cmd
}

/// 10:00 KST on 2026-07-16 (a Thursday inside the fixture snapshot's coverage).
fn open_window_ts() -> i64 {
    Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap().timestamp()
}

/// 21:00 KST on 2026-07-16 — outside the 09:00–15:30 seam: known-closed.
fn closed_window_ts() -> i64 {
    Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 0).unwrap().timestamp()
}

/// Write a valid TempDir-only snapshot bracketing 2026-07-16 whose mid row
/// carries `mid_status` (the trimmed `dispatch_cli.rs` fixture — synthetic,
/// no production snapshot, no network).
fn write_report_snapshot(
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
fn assert_offers_are_executable(text: &str) {
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

#[test]
fn report_genuinely_unknown_offers_only_any_items_plus_the_repair_action() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "any-chore", "Adjudicate the skills dirs", "any", &[]);
    add_item(&queue, "open-work", "Mount the rung-1 session", "open-attended", &[]);
    add_item(&queue, "closed-work", "Run the offline gate", "closed", &[]);

    // No LS_CALENDAR_SNAPSHOT at all → genuinely-unknown, fail closed (R3).
    let out = report_cmd(&queue).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("window: genuinely-unknown"), "{text}");
    assert!(text.contains("LS_CALENDAR_SNAPSHOT"), "the repair action is printed: {text}");
    assert!(text.contains("any-chore"), "any-tagged items stay eligible: {text}");
    assert!(!text.contains("open-work"), "open-attended items are NEVER offered when unknown: {text}");
    assert!(!text.contains("closed-work"), "closed items are not offered when unknown: {text}");
    // R5: the plain queue item's handoff is the done transition after the work.
    assert!(text.contains("lab-next done any-chore"), "{text}");
    assert_offers_are_executable(&text);
}

#[test]
fn report_selection_is_deterministic_deadline_ordered_and_a_sequence_outranks_items() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    // Added out of deadline order; `undated` has none (orders after dated ones).
    add_item(&queue, "dl-late", "Late chore", "closed", &["--deadline", "2026-07-20T00:00:00Z"]);
    add_item(&queue, "dl-early", "Early chore", "closed", &["--deadline", "2026-07-17T00:00:00Z"]);
    add_item(&queue, "undated", "Undated chore", "closed", &[]);

    // An in-flight turn (stage log only — no data home needed).
    let stage_log = tmp.path().join("stagelog.txt");
    std::fs::write(&stage_log, "bump\nrebaseline\n").unwrap();
    let ledger = tmp.path().join("trials.jsonl"); // never written — hermetic

    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);
    let run_once = || {
        let mut cmd = report_cmd(&queue);
        cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
            .env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string())
            .env("LS_GOVERNED_STAGELOG", &stage_log)
            .env("LS_TRIALS_LEDGER", &ledger);
        cmd.output().unwrap()
    };

    let first = run_once();
    let second = run_once();
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert_eq!(stdout(&first), stdout(&second), "identical state → identical output");

    let text = stdout(&first);
    assert!(text.contains("window: known-closed"), "{text}");
    // R4: the window-compatible in-flight sequence outranks every new item.
    let next_at = text.find("\nnext:").expect("a next section");
    let after_next = &text[next_at + "\nnext:".len()..];
    let top = after_next.lines().nth(1).unwrap_or("");
    assert!(top.trim_start().starts_with("turn"), "the in-flight turn is the top offer: {text}");
    assert!(text.contains("turn governed"), "the turn resume command is offered: {text}");
    // R4: remaining eligible items by recorded deadline, then queue order.
    let queue_at = text.find("\nqueue:").expect("a queue section");
    let tail = &text[queue_at..];
    let early = tail.find("dl-early").expect("dl-early listed");
    let late = tail.find("dl-late").expect("dl-late listed");
    let undated = tail.find("undated").expect("undated listed");
    assert!(early < late && late < undated, "deadline order then queue order: {text}");
    assert_offers_are_executable(&text);
}

#[test]
fn report_open_window_lists_closed_sequences_paused_and_offers_the_attended_step() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(
        &queue,
        "mount-session",
        "Mount the attended rung-1 session",
        "open-attended",
        &["--ref", "adapters/nautilus/lab/RUNBOOK-rung1.md"],
    );

    // A closed-window in-flight turn during the open window → paused, visible.
    let stage_log = tmp.path().join("stagelog.txt");
    std::fs::write(&stage_log, "diagnose\n").unwrap();
    let ledger = tmp.path().join("trials.jsonl");

    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);
    let mut cmd = report_cmd(&queue);
    cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
        .env("LS_NEXT_NOW_UNIX", open_window_ts().to_string())
        .env("LS_GOVERNED_STAGELOG", &stage_log)
        .env("LS_TRIALS_LEDGER", &ledger);
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    let text = stdout(&out);
    assert!(text.contains("window: presumed-open"), "{text}");
    // R2: the attended morning-chain pointer (10:00 KST → the mount step).
    assert!(text.contains("attended chain: mount-universe"), "{text}");
    // R4: the window-incompatible turn stays visible as paused resumable work.
    assert!(text.contains("[paused] turn"), "{text}");
    assert!(text.contains("resume: "), "the paused sequence still names its resume: {text}");
    // The open-attended item is the top offer, with its refs (R13).
    let next_at = text.find("\nnext:").expect("a next section");
    let after_next = &text[next_at..];
    assert!(after_next.contains("mount-session"), "{text}");
    assert!(after_next.contains("refs: adapters/nautilus/lab/RUNBOOK-rung1.md"), "{text}");
    assert!(text.contains("lab-next done mount-session"), "R5 executable handoff: {text}");
    assert_offers_are_executable(&text);
}

#[test]
fn report_auto_closes_artifact_witnessed_items_and_flags_explicit_items_without_a_tty() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");

    // A tool-event item whose declared completion artifact NOW exists (R12).
    let artifact = tmp.path().join("ingest-checkpoint.json");
    std::fs::write(&artifact, "{\"completed\":[\"x\"]}").unwrap();
    add_item(
        &queue,
        "ingest-done",
        "Ingest the 0716 window",
        "closed",
        &["--event", "ingest-complete", "--artifact", artifact.to_str().unwrap()],
    );
    // An explicit-signal item carrying a reconcile flag (failed supersede).
    add_item(&queue, "stuck", "Stuck chore", "any", &[]);
    let out = run(&queue, &["supersede", "stuck", "--by", "ghost"]);
    assert_ne!(out.status.code(), Some(0), "the flag-setting supersede must refuse");

    let out = report_cmd(&queue).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    // Auto-close with a printed notice; the queue file records the done.
    assert!(text.contains("auto-closed: ingest-done"), "{text}");
    let raw = std::fs::read_to_string(&queue).unwrap();
    let line = raw.lines().find(|l| l.contains("\"ingest-done\"")).unwrap();
    assert!(line.contains("done_utc"), "the auto-close is persisted: {line}");
    // Non-TTY: the explicit item gets a flagged confirm line, never a prompt,
    // and stays actionable (still offered).
    assert!(text.contains("confirm: stuck"), "{text}");
    assert!(text.contains("lab-next done stuck"), "the confirm line names the close command: {text}");
    let next_at = text.find("\nnext:").expect("a next section");
    assert!(text[next_at..].contains("stuck"), "the flagged item is still offered: {text}");
}

#[test]
fn report_gate_status_override_surfaces_an_in_flight_gate_run() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);

    // Pre-captured `gate-run.sh --status` output: two steps done, next=step 3.
    let status = tmp.path().join("gate-status.txt");
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

    let mut cmd = report_cmd(&queue);
    cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
        .env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string())
        .env("LS_GATE_STATUS_FILE", &status);
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("gate-run"), "{text}");
    assert!(text.contains("cargo-test-ls-core"), "the stage names the next step: {text}");
    assert!(text.contains("(2/6 done)"), "{text}");
    assert!(text.contains("resume: make gate-run (resumes at cargo-test-ls-core)"), "{text}");
    assert_offers_are_executable(&text);

    // A green gate (next=none) is NOT an in-flight sequence.
    std::fs::write(
        &status,
        "step=1 name=docs status=done fingerprint=aaaa\n\
         step=2 name=cargo-test status=done fingerprint=bbbb\n\
         step=3 name=cargo-test-ls-core status=done fingerprint=cccc\n\
         step=4 name=docs-check status=done fingerprint=dddd\n\
         step=5 name=lane-check status=done fingerprint=eeee\n\
         step=6 name=adapter-check status=done fingerprint=ffff\n\
         next=none\n",
    )
    .unwrap();
    let mut cmd = report_cmd(&queue);
    cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
        .env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string())
        .env("LS_GATE_STATUS_FILE", &status);
    let out = cmd.output().unwrap();
    let text = stdout(&out);
    assert!(!text.contains("gate-run"), "a green gate is not in flight: {text}");
    assert!(text.contains("in-flight: none"), "{text}");
}
