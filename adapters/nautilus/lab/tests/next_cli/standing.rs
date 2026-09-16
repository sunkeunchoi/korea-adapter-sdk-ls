//! U3 — standing work, priority selection, and the blocked exemptions.
//!
//! Where a blocked item renders (the `standing:` section and nowhere else) and
//! the exemptions that keep waiting work out of the staleness and confirmation
//! passes.

use std::path::Path;

use tempfile::TempDir;

use crate::fixture::*;

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
