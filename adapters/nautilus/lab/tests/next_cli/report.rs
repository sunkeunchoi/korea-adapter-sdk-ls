//! U5 — the default window-aware report (R1/R2/R4/R5/R12/R13; KTD1/KTD3).
//!
//! The section ordering, window derivation, in-flight selection, and the R5
//! guarantee that every offered line is executable. Read-only: nothing here
//! mutates the queue except to stage the rows a report is then asserted on.

use tempfile::TempDir;

use crate::fixture::*;

// ---------------------------------------------------------------------------
// U5 — the default window-aware report (R1/R2/R4/R5/R12/R13; KTD1/KTD3)
// ---------------------------------------------------------------------------

/// A hermetic `report` invocation (see [`hermetic`]).
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

