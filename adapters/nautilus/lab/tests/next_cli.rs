//! `lab-next` queue edit-surface tests (U1, R6/R8/R9; KTD2/KTD3/KTD6; AE3).
//! Every run is the compiled bin as a subprocess so env is isolated; the queue
//! file is a tempdir path via `LS_QUEUE_PATH` (KTD2's test-time override).

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lab-next"))
}

/// Run `lab-next` against `queue_path` with the given args.
fn run(queue_path: &Path, args: &[&str]) -> Output {
    bin().args(args).env("LS_QUEUE_PATH", queue_path).output().unwrap()
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
fn the_default_report_is_a_stub_notice_until_u5_and_unknown_subcommands_are_usage_errors() {
    let tmp = TempDir::new().unwrap();
    let queue = tmp.path().join("items.jsonl");

    let out = run(&queue, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("not yet implemented"), "{}", stdout(&out));

    let out = run(&queue, &["frobnicate"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("usage"), "{}", stderr(&out));
}
