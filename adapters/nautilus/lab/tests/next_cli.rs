//! `lab-next` queue edit-surface tests (U1, R6/R8/R9; KTD2/KTD3/KTD6; AE3)
//! plus the default window-aware report (U5, R1/R2/R4/R5/R12/R13; KTD1).
//! Every run is the compiled bin as a subprocess so env is isolated; the queue
//! file is a tempdir path via `LS_QUEUE_PATH` (KTD2's test-time override).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::{TimeZone, Utc};
use nautilus_ls_lab::queue::Queue;
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

// ---------------------------------------------------------------------------
// Priority + blocked mutator verbs (U2, R5/R20/R21; KTD6; AE7/AE10)
// ---------------------------------------------------------------------------

/// The ids currently holding the priority marker, read straight off the store.
fn holders(queue: &Path) -> Vec<String> {
    Queue::new(queue)
        .read_all()
        .unwrap()
        .into_iter()
        .filter(|i| i.priority)
        .map(|i| i.id)
        .collect()
}

#[test]
fn setting_priority_on_a_second_item_clears_the_first() {
    // AE7: exactly one queue item holds priority at a time — scarce by
    // construction, not by discipline.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "first", "First head", "any", &[]);
    add_item(&queue, "second", "Second head", "any", &[]);

    let out = run(&queue, &["priority", "first"]);
    assert_eq!(out.status.code(), Some(0), "priority failed: {}", stderr(&out));
    assert!(stdout(&out).contains("priority: first"), "{}", stdout(&out));
    assert_eq!(holders(&queue), ["first"]);

    let out = run(&queue, &["priority", "second"]);
    assert_eq!(out.status.code(), Some(0), "priority failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("priority: second"), "{text}");
    assert!(text.contains("cleared: first"), "the displaced holder is named: {text}");
    assert_eq!(holders(&queue), ["second"], "exactly one holder at a time");

    // The raw store: the marker is a real key on the holder and absent
    // everywhere else — `skip_serializing_if` keeps unmarked lines clean.
    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(!raw.lines().next().unwrap().contains("priority"), "{raw}");
    assert!(raw.lines().nth(1).unwrap().contains("\"priority\":true"), "{raw}");
}

#[test]
fn the_priority_verb_repairs_a_store_an_older_binary_left_with_several_holders() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    for id in ["a", "b", "c"] {
        add_item(&queue, id, &format!("item {id}"), "any", &[]);
    }
    // Seed what a binary that ignored the field could leave behind: TWO
    // holders. The set path must CONVERGE the store, not assume it is clean.
    let store = Queue::new(&queue);
    let mut items = store.read_all().unwrap();
    items[0].priority = true;
    items[1].priority = true;
    store.save(&items).unwrap();

    let out = run(&queue, &["priority", "c"]);
    assert_eq!(out.status.code(), Some(0), "priority failed: {}", stderr(&out));
    assert!(
        stdout(&out).contains("cleared: a, b"),
        "both displaced holders are named: {}",
        stdout(&out)
    );
    assert_eq!(holders(&queue), ["c"], "converged to exactly one holder");
}

#[test]
fn superseding_the_priority_holder_transfers_the_marker_to_the_superseder() {
    // AE10 / R21: the arc's frontier survives the transition its head takes.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "plan-a", "Old head", "closed", &[]);
    add_item(&queue, "plan-b", "New head", "closed", &[]);
    let out = run(&queue, &["priority", "plan-a"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    let out = run(&queue, &["supersede", "plan-a", "--by", "plan-b"]);
    assert_eq!(out.status.code(), Some(0), "supersede failed: {}", stderr(&out));
    assert_eq!(holders(&queue), ["plan-b"], "the marker followed the work");

    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(raw.contains("\"superseded_by\":\"plan-b\""), "{raw}");
    assert!(!raw.lines().next().unwrap().contains("priority"), "the old head dropped it: {raw}");
}

#[test]
fn a_refused_supersede_of_the_priority_holder_keeps_the_marker_where_it_was() {
    // The refusal branch: the work did not transition, so neither does the
    // marker (and a missing superseder has nothing to receive it).
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "plan-a", "Old head", "closed", &[]);
    let out = run(&queue, &["priority", "plan-a"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    let out = run(&queue, &["supersede", "plan-a", "--by", "plan-z"]);
    assert_ne!(out.status.code(), Some(0), "a missing superseder never completes");
    assert_eq!(holders(&queue), ["plan-a"], "the marker stays with the un-transitioned work");
}

#[test]
fn completing_the_priority_holder_clears_the_marker_and_leaves_no_holder() {
    // R21's other half: a done head must not keep the frontier marker.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "head", "Head item", "closed", &[]);
    add_item(&queue, "other", "Other item", "closed", &[]);
    let out = run(&queue, &["priority", "head"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    let out = run(&queue, &["done", "head"]);
    assert_eq!(out.status.code(), Some(0), "done failed: {}", stderr(&out));
    assert!(holders(&queue).is_empty(), "no holder after the head completes");
    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(!raw.contains("priority"), "the marker leaves the store entirely: {raw}");
}

#[test]
fn block_records_the_condition_and_unblock_restores_a_plain_actionable_item() {
    // R5/R20: parkedness becomes queue state carrying its unblock condition,
    // instead of prose in `notes`.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "parked", "Externally blocked work", "open-attended", &[]);

    let condition = "the operator authorizes a session on a margin-clearing head";
    let out = run(&queue, &["block", "parked", "--until", condition]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));
    assert!(stdout(&out).contains("blocked: parked"), "{}", stdout(&out));
    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(raw.contains(&format!("\"unblock_condition\":\"{condition}\"")), "{raw}");

    let out = run(&queue, &["unblock", "parked"]);
    assert_eq!(out.status.code(), Some(0), "unblock failed: {}", stderr(&out));
    assert!(stdout(&out).contains("unblocked: parked"), "{}", stdout(&out));
    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(!raw.contains("unblock_condition"), "the state left the line: {raw}");

    // Unblocking an item that is not blocked is a reported no-op, mirroring
    // `done`'s idempotence — never an error.
    let out = run(&queue, &["unblock", "parked"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("was not blocked"), "{}", stdout(&out));

    // The item stayed in the actionable view throughout; withholding a blocked
    // item from the report's `next:` is the report's job, not the store's.
    let text = stdout(&run(&queue, &["list"]));
    assert!(text.contains("actionable: 1"), "{text}");
}

#[test]
fn the_new_verbs_refuse_bad_input_and_leave_the_store_byte_identical() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "only", "Only item", "any", &[]);
    let before = std::fs::read_to_string(&queue).unwrap();

    for args in [
        vec!["priority", "ghost"],
        vec!["block", "ghost", "--until", "the operator acts"],
        vec!["unblock", "ghost"],
    ] {
        let out = run(&queue, &args);
        assert_ne!(out.status.code(), Some(0), "{args:?} must refuse an unknown id");
        assert!(stderr(&out).contains("ghost"), "{args:?} names the id: {}", stderr(&out));
    }

    // R24: a blocked item must NAME the act that would unblock it, so a missing
    // or blank `--until` is refused — and the refusal names the item.
    for args in [
        vec!["block", "only"],
        vec!["block", "only", "--until"],
        vec!["block", "only", "--until", "   "],
    ] {
        let out = run(&queue, &args);
        assert_ne!(out.status.code(), Some(0), "{args:?} must be refused");
        assert!(stderr(&out).contains("only"), "{args:?} names the item: {}", stderr(&out));
    }

    // `priority` takes <id> or --clear and nothing else.
    let out = run(&queue, &["priority", "--bogus"]);
    assert_ne!(out.status.code(), Some(0), "an unknown flag is not an id");
    assert!(stderr(&out).contains("usage"), "{}", stderr(&out));

    assert_eq!(
        std::fs::read_to_string(&queue).unwrap(),
        before,
        "no refusal mutated the store"
    );
}

#[test]
fn priority_clear_leaves_no_holder() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "head", "Head item", "any", &[]);
    let out = run(&queue, &["priority", "head"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    let out = run(&queue, &["priority", "--clear"]);
    assert_eq!(out.status.code(), Some(0), "priority --clear failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("priority: none"), "{text}");
    assert!(text.contains("cleared: head"), "the released holder is named: {text}");
    assert!(holders(&queue).is_empty(), "no holder remains");

    // Clearing an unheld marker is a no-op, not an error.
    let out = run(&queue, &["priority", "--clear"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("priority: none"), "{}", stdout(&out));
}

#[test]
fn a_hermetic_run_drives_set_transfer_clear_and_unblock_end_to_end() {
    // U2's stated Verification: the compiled bin walks the whole arc against a
    // tempdir queue — set the marker, transfer it through a supersede, clear
    // it, then block and unblock an item — with the store agreeing at each step.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("queue/items.jsonl");
    add_item(&queue, "head", "Arc head", "closed", &[]);
    add_item(&queue, "successor", "Arc successor", "closed", &[]);
    add_item(&queue, "parked", "Externally blocked work", "open-attended", &[]);

    let step = |args: &[&str], expect: &str| {
        let out = run(&queue, args);
        assert_eq!(out.status.code(), Some(0), "{args:?} failed: {}", stderr(&out));
        assert!(stdout(&out).contains(expect), "{args:?}: {}", stdout(&out));
    };

    step(&["priority", "head"], "priority: head");
    assert_eq!(holders(&queue), ["head"], "set");

    step(&["supersede", "head", "--by", "successor"], "superseded: head by successor");
    assert_eq!(holders(&queue), ["successor"], "transfer");

    step(&["priority", "--clear"], "priority: none");
    assert!(holders(&queue).is_empty(), "clear");

    step(&["block", "parked", "--until", "the operator authorizes a session"], "blocked: parked");
    assert!(
        Queue::new(&queue).read_all().unwrap().iter().any(|i| i.is_blocked()),
        "the blocked state is recorded"
    );

    step(&["unblock", "parked"], "unblocked: parked");
    assert!(
        Queue::new(&queue).read_all().unwrap().iter().all(|i| !i.is_blocked()),
        "the blocked state is cleared"
    );

    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(raw.contains("\"superseded_by\":\"successor\""), "{raw}");
    assert!(!raw.contains("priority"), "no marker key survives the arc: {raw}");
    assert!(!raw.contains("unblock_condition"), "no blocked state survives: {raw}");
}

// ---------------------------------------------------------------------------
// U3 — standing work, priority selection, and the blocked exemptions
// (R1/R3/R4/R22/R23; KTD3/KTD4/KTD5)
// ---------------------------------------------------------------------------

/// One report section: its un-indented header line plus every indented line
/// under it, up to the next header (`""` when the section is absent).
fn section(text: &str, name: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if !line.starts_with(' ') {
            inside = line.split(':').next() == Some(name);
        } else if !inside {
            continue;
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The recurring external act these tests park work on.
const VENDOR: &str = "the vendor returns the countersigned data licence";

#[test]
fn a_blocked_open_attended_item_is_standing_work_under_both_derived_windows() {
    // AE12 / R23: standing work is sourced BEFORE the window filter, so a
    // blocked `open-attended` item renders whatever window is derived; only
    // the offer stays window-gated.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "vendor-licence", "Countersign the data licence", "open-attended", &[]);
    add_item(&queue, "offline-chore", "Run the offline gate", "closed", &[]);
    let out = run(&queue, &["block", "vendor-licence", "--until", VENDOR]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));

    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);
    let report = |ts: i64| {
        let mut cmd = report_cmd(&queue);
        cmd.env("LS_CALENDAR_SNAPSHOT", &snap).env("LS_NEXT_NOW_UNIX", ts.to_string());
        let out = cmd.output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
        stdout(&out)
    };

    // Known-closed does NOT admit open-attended work — the item stands anyway.
    let closed = report(closed_window_ts());
    assert!(closed.contains("window: known-closed"), "{closed}");
    let standing = section(&closed, "standing");
    assert!(standing.contains("vendor-licence"), "the blocked item still renders: {closed}");
    assert!(standing.contains(VENDOR), "with its unblock condition: {closed}");
    // KTD4: the section sits between `reconciled:` and `next:`.
    let at_standing = closed.find("\nstanding:").expect("a standing section");
    let at_next = closed.find("\nnext:").expect("a next section");
    assert!(at_standing < at_next, "standing renders before the offer: {closed}");
    // The offer stays window-gated and never names the blocked item.
    assert!(section(&closed, "next").contains("offline-chore"), "{closed}");
    assert!(!section(&closed, "next").contains("vendor-licence"), "{closed}");
    assert!(!section(&closed, "queue").contains("vendor-licence"), "{closed}");
    assert_offers_are_executable(&closed);

    // Presumed-open: identical standing rendering, unchanged by the window.
    let open = report(open_window_ts());
    assert!(open.contains("window: presumed-open"), "{open}");
    let standing = section(&open, "standing");
    assert!(standing.contains("vendor-licence"), "{open}");
    assert!(standing.contains(VENDOR), "{open}");
    assert!(!section(&open, "next").contains("vendor-licence"), "{open}");
    assert_offers_are_executable(&open);
}

#[test]
fn a_blocked_item_whose_artifact_already_exists_is_not_auto_closed() {
    // AE11 / R22: the auto-close pass skips blocked items. The declared
    // artifact is present, but the external act the item waits on is not
    // done — and the unblock condition still renders.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    let artifact = tmp.path().join("ingest-checkpoint.json");
    std::fs::write(&artifact, "{\"completed\":[\"x\"]}").unwrap();
    add_item(
        &queue,
        "ingest-parked",
        "Ingest the 0716 window",
        "closed",
        &["--event", "ingest-complete", "--artifact", artifact.to_str().unwrap()],
    );
    add_item(&queue, "offline-chore", "Run the offline gate", "closed", &[]);
    let cond = "the vendor re-issues the corrupted 0716 tick file";
    let out = run(&queue, &["block", "ingest-parked", "--until", cond]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));

    let out = report_cmd(&queue).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(!text.contains("auto-closed"), "a blocked item is never auto-closed: {text}");
    let raw = std::fs::read_to_string(&queue).unwrap();
    assert!(!raw.contains("done_utc"), "the store records no close: {raw}");
    let standing = section(&text, "standing");
    assert!(standing.contains("ingest-parked"), "{text}");
    assert!(standing.contains(cond), "the unblock condition still renders: {text}");
    assert_offers_are_executable(&text);
}

#[test]
fn a_blocked_item_past_its_deadline_is_neither_dropped_nor_prompted() {
    // AE2 / KTD3: the exemption is wired at BOTH staleness sites —
    // `QueueItem::is_stale` (so the item is not dropped) and the report's
    // `deadline_passed` (so it is not prompted). R22 covers the other
    // confirmation route too: a reconcile-flagged blocked item is silent.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    let cond = "the exchange publishes the revised 0716 schedule";
    add_item(&queue, "overdue", "Overdue parked work", "any", &["--deadline", "2026-07-15T00:00:00Z"]);
    let out = run(&queue, &["block", "overdue", "--until", cond]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));

    // A reconcile flag from a refused supersede, then blocked.
    add_item(&queue, "flagged", "Flagged parked work", "any", &[]);
    let out = run(&queue, &["supersede", "flagged", "--by", "ghost"]);
    assert_ne!(out.status.code(), Some(0), "the flag-setting supersede must refuse");
    let out = run(&queue, &["block", "flagged", "--until", cond]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));

    let mut cmd = report_cmd(&queue);
    cmd.env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string());
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(!text.contains("confirm:"), "blocked work is never asked to confirm it is done: {text}");
    let standing = section(&text, "standing");
    assert!(standing.contains("overdue"), "the past-deadline item is not dropped: {text}");
    assert!(standing.contains("flagged"), "the reconcile-flagged item stands too: {text}");
    assert_offers_are_executable(&text);
}

#[test]
fn when_every_open_item_is_blocked_the_offer_names_the_unblock_condition() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "vendor-licence", "Countersign the data licence", "any", &[]);
    let out = run(&queue, &["block", "vendor-licence", "--until", VENDOR]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));

    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);
    let mut cmd = report_cmd(&queue);
    cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
        .env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string());
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    let next = section(&text, "next");
    assert!(next.contains(VENDOR), "the offer line names the unblock condition: {text}");
    assert!(next.contains("lab-next unblock vendor-licence"), "R5 executable: {text}");
    assert!(!next.contains("lab-next add"), "standing work is not advice to queue more: {text}");
    assert_offers_are_executable(&text);
}

#[test]
fn priority_outranks_an_earlier_deadline_and_an_in_flight_sequence_outranks_both() {
    // R1 + KTD5: the marker orders ITEMS ahead of the deadline sort; a
    // window-compatible in-flight sequence keeps the `next:` slot, so the
    // resume-safety property the sequence offer exists for survives pinning.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "dl-early", "Early chore", "closed", &["--deadline", "2026-07-17T00:00:00Z"]);
    add_item(&queue, "pinned", "The pinned arc head", "closed", &[]);
    add_item(&queue, "undated", "Undated chore", "closed", &[]);
    let out = run(&queue, &["priority", "pinned"]);
    assert_eq!(out.status.code(), Some(0), "priority failed: {}", stderr(&out));

    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);
    let ledger = tmp.path().join("trials.jsonl"); // never written — hermetic
    let report = |stage_log: Option<&Path>| {
        let mut cmd = report_cmd(&queue);
        cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
            .env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string())
            .env("LS_TRIALS_LEDGER", &ledger);
        if let Some(log) = stage_log {
            cmd.env("LS_GOVERNED_STAGELOG", log);
        }
        let out = cmd.output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
        stdout(&out)
    };

    // Nothing in flight: the marker beats the earlier recorded deadline.
    let text = report(None);
    assert!(text.contains("window: known-closed"), "{text}");
    let next = section(&text, "next");
    assert!(next.contains("pinned"), "the priority item is the offer: {text}");
    assert!(!next.contains("dl-early"), "{text}");
    let rest = section(&text, "queue");
    let early = rest.find("dl-early").expect("dl-early listed");
    let undated = rest.find("undated").expect("undated listed");
    assert!(early < undated, "behind the marker, deadline then queue order holds: {text}");
    assert_offers_are_executable(&text);

    // KTD5: an in-flight, window-compatible turn still takes the offer slot.
    let stage_log = tmp.path().join("stagelog.txt");
    std::fs::write(&stage_log, "bump\nrebaseline\n").unwrap();
    let text = report(Some(&stage_log));
    let next = section(&text, "next");
    assert!(next.contains("turn"), "the in-flight sequence keeps the offer: {text}");
    assert!(!next.contains("pinned"), "priority never displaces a resumable sequence: {text}");
    let rest = section(&text, "queue");
    let pinned = rest.find("pinned").expect("pinned listed");
    let early = rest.find("dl-early").expect("dl-early listed");
    assert!(pinned < early, "the marker still orders the remaining items: {text}");
    assert_offers_are_executable(&text);
}

#[test]
fn a_blocked_priority_head_stands_while_the_top_unblocked_item_is_offered() {
    // AE1 / R3: the arc is pinned and its head is blocked awaiting the vendor.
    // The head renders as standing work with its unblock condition, and the
    // top unblocked item is offered — never an unreachable offer.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "arc-head", "Sign the vendor arc head", "closed", &[]);
    add_item(&queue, "follow-up", "Draft the ingest note", "closed", &[]);
    let out = run(&queue, &["priority", "arc-head"]);
    assert_eq!(out.status.code(), Some(0), "priority failed: {}", stderr(&out));
    let out = run(&queue, &["block", "arc-head", "--until", VENDOR]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));

    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);
    let mut cmd = report_cmd(&queue);
    cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
        .env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string());
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);

    let standing = section(&text, "standing");
    assert!(standing.contains("arc-head"), "the pinned head stands: {text}");
    assert!(standing.contains(VENDOR), "with its unblock condition: {text}");
    let next = section(&text, "next");
    assert!(next.contains("follow-up"), "the top unblocked item is offered: {text}");
    assert!(!next.contains("arc-head"), "a blocked item is never offered: {text}");
    assert!(text.contains("queue: none"), "nothing else remains: {text}");
    assert_offers_are_executable(&text);
}

#[test]
fn an_item_that_is_both_a_paused_sequence_and_blocked_renders_once() {
    // One item, one rendering: blocked owns the section, and the head keeps
    // its paused-sequence label — never two competing claims on one id.
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");
    add_item(&queue, "seq-parked", "Resume the ladder", "closed", &["--sequence", "ladder"]);
    add_item(&queue, "offline-chore", "Run the offline gate", "closed", &[]);
    let cond = "the operator authorizes a session on a margin-clearing head";
    let out = run(&queue, &["block", "seq-parked", "--until", cond]);
    assert_eq!(out.status.code(), Some(0), "block failed: {}", stderr(&out));

    let snap = write_report_snapshot(tmp.path(), nautilus_ls_calendar::schema::DayStatus::Unknown);
    let mut cmd = report_cmd(&queue);
    cmd.env("LS_CALENDAR_SNAPSHOT", &snap)
        .env("LS_NEXT_NOW_UNIX", closed_window_ts().to_string());
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);

    assert_eq!(text.matches("seq-parked").count(), 1, "exactly one rendering: {text}");
    let standing = section(&text, "standing");
    assert!(standing.contains("seq-parked"), "{text}");
    assert!(standing.contains("(paused sequence ladder)"), "the head keeps its label: {text}");
    assert!(standing.contains(cond), "{text}");
    assert!(section(&text, "next").contains("offline-chore"), "{text}");
    assert_offers_are_executable(&text);
}
