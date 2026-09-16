//! Unit tests for the queue store and its item model.
//!
//! Lifted out of `mod.rs` when that file crossed 1,000 lines. A child module
//! rather than an integration test, because these exercise the store through
//! [`QUEUE_PATH_ENV`]-pointed tempdirs and read the crate-visible path helpers
//! (`anchored`, `artifact_witnesses`) that an integration test cannot see.
//!
//! `chrono` is imported explicitly here: the split moved the item model and the
//! store into their own files, so `super::*` no longer re-exports `mod.rs`'s
//! former chrono import.

use chrono::{DateTime, Utc};

use super::*;

fn item(id: &str, window: Window) -> QueueItem {
    QueueItem::new(id, format!("title {id}"), window, CompletionSignal::Explicit, "2026-07-29T00:00:00Z")
}

#[test]
fn absent_file_reads_empty_and_save_roundtrips_in_order() {
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("queue/items.jsonl"));
    assert!(q.read_all().unwrap().is_empty(), "absent file reads empty");
    q.add(item("a", Window::Closed)).unwrap();
    q.add(item("b", Window::Any)).unwrap();
    let back = q.read_all().unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(back[0].id, "a", "file order preserved");
    assert_eq!(back[1].id, "b");
    // No tmp sibling survives a completed save.
    let names: Vec<_> = std::fs::read_dir(tmp.path().join("queue"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["items.jsonl"], "no tmp residue: {names:?}");
}

#[test]
fn unknown_schema_version_is_refused_naming_the_line() {
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("a", Window::Any)).unwrap();
    let mut bumped = serde_json::to_value(item("b", Window::Any)).unwrap();
    bumped["schema_version"] = serde_json::json!(999);
    let mut text = std::fs::read_to_string(q.path()).unwrap();
    text.push_str(&format!("{bumped}\n"));
    std::fs::write(q.path(), text).unwrap();
    let err = q.read_all().unwrap_err();
    assert!(err.to_string().contains("line 2"), "{err}");
    assert!(err.to_string().contains("schema version 999"), "{err}");
}

#[test]
fn staleness_respects_deadline_supersede_and_the_sequence_exemption() {
    let now: DateTime<Utc> = "2026-07-29T12:00:00Z".parse().unwrap();
    let mut past = item("past", Window::Closed);
    past.deadline = Some("2026-07-28T00:00:00Z".into());
    assert!(past.is_stale(now).unwrap());
    assert!(!past.is_actionable(now).unwrap());

    let mut seq = past.clone();
    seq.sequence = Some("turn".into());
    assert!(!seq.is_stale(now).unwrap(), "a paused in-flight sequence entry is never stale");
    assert!(seq.is_actionable(now).unwrap());

    let mut superseded = item("old", Window::Closed);
    superseded.superseded_by = Some("new".into());
    assert!(!superseded.is_actionable(now).unwrap());

    let mut flagged = item("flagged", Window::Closed);
    flagged.reconcile = Some("done refused: …".into());
    assert!(flagged.is_actionable(now).unwrap(), "a reconcile flag keeps the item actionable");
}

#[test]
fn relative_artifact_paths_anchor_to_the_repo_root_not_the_invoking_cwd() {
    // The anchoring rule itself: relative → repo-root-joined, absolute →
    // pass-through. Baked from CARGO_MANIFEST_DIR, so it is cwd-invariant.
    let root = repo_root().unwrap();
    assert_eq!(anchored("AGENTS.md"), root.join("AGENTS.md"));
    let abs = root.join("AGENTS.md");
    assert_eq!(anchored(abs.to_str().unwrap()), abs, "absolute paths pass through");

    // `done` witnesses a RELATIVE artifact via the same anchoring: a
    // repo-root-relative path to a tracked non-empty file completes
    // whatever the test process cwd happens to be (the harness resets it
    // per invocation — exactly the drift the anchoring exists to absorb).
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    let mut it = item("rel", Window::Any);
    it.completion = CompletionSignal::ToolEvent {
        event: "gate-green".into(),
        artifact: Some("AGENTS.md".into()), // repo-root-relative (documented contract)
    };
    q.add(it).unwrap();
    assert_eq!(
        q.done("rel", "2026-07-30T00:00:00Z").unwrap(),
        TransitionOutcome::Completed,
        "a repo-root-relative artifact witnesses regardless of the invoking cwd"
    );
}

#[test]
fn the_committed_repo_queue_parses_at_the_default_path() {
    // Guards the tracked seed file (and every future migration) against a
    // line this build cannot read.
    let q = Queue::new(default_queue_path().unwrap());
    q.read_all().unwrap();
}

#[test]
fn a_blocked_item_round_trips_with_its_unblock_condition() {
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    let mut blocked = item("parked", Window::OpenAttended);
    blocked.unblock_condition =
        Some("the operator authorizes a session on a margin-clearing head".into());
    q.add(blocked).unwrap();
    let back = q.read_all().unwrap();
    assert_eq!(back.len(), 1);
    assert!(back[0].is_blocked(), "the blocked state survives the round trip");
    assert_eq!(
        back[0].unblock_condition.as_deref(),
        Some("the operator authorizes a session on a margin-clearing head"),
        "the unblock condition survives verbatim"
    );
}

#[test]
fn priority_round_trips_and_both_new_fields_are_skipped_when_absent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    let mut top = item("top", Window::Any);
    top.priority = true;
    q.add(top).unwrap();
    q.add(item("plain", Window::Any)).unwrap();
    let back = q.read_all().unwrap();
    assert!(back[0].priority, "the priority marker survives the round trip");
    assert!(!back[1].priority, "an unmarked item stays unmarked");
    assert!(!back[1].is_blocked());
    // Absent fields are skipped on write, so the committed store never
    // churns with `"priority":false` / `"unblock_condition":null` (KTD1).
    let text = std::fs::read_to_string(q.path()).unwrap();
    let plain = text.lines().nth(1).unwrap();
    assert!(!plain.contains("priority"), "absent priority is skipped: {plain}");
    assert!(!plain.contains("unblock_condition"), "absent blocked state is skipped: {plain}");
}

#[test]
fn a_mistyped_priority_key_is_refused_naming_the_line() {
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("a", Window::Any)).unwrap();
    // R31: serde reads a MISSING field as the default, so without
    // `deny_unknown_fields` a mistyped key would drop silently and the item
    // would load clean as "neither priority nor blocked".
    let mut typo = serde_json::to_value(item("b", Window::Any)).unwrap();
    typo["prioriti"] = serde_json::json!(true);
    let mut text = std::fs::read_to_string(q.path()).unwrap();
    text.push_str(&format!("{typo}\n"));
    std::fs::write(q.path(), text).unwrap();
    let err = q.read_all().unwrap_err().to_string();
    assert!(err.contains("line 2"), "the refusal names the line: {err}");
    assert!(err.contains("prioriti"), "the refusal names the offending key: {err}");
}

#[test]
fn a_pre_existing_line_with_neither_new_field_parses_at_version_1() {
    // A verbatim committed line shape (queue/items.jsonl): neither new key.
    // The fields are ADDITIVE at version 1 — a bump would make every
    // committed line unreadable and the queue unrewritable (KTD1).
    assert_eq!(QUEUE_SCHEMA_VERSION, 1, "the additive fields must NOT bump the version");
    let line = r#"{"schema_version":1,"id":"session-morning-root-manifest-freshness","title":"Add root ls-sdk and ls-core manifests to the morning freshness preflight","window":"any","completion":{"kind":"explicit"},"added_utc":"2026-08-19T12:04:34.452122+00:00","notes":"Separate shell-preflight residual.","refs":["adapters/nautilus/scripts/session-morning.sh"]}"#;
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    std::fs::write(q.path(), format!("{line}\n")).unwrap();
    let back = q.read_all().unwrap();
    assert_eq!(back.len(), 1);
    assert!(!back[0].priority, "a pre-existing line defaults to unmarked");
    assert!(!back[0].is_blocked(), "a pre-existing line defaults to unblocked");
}

#[test]
fn blocked_with_a_blank_unblock_condition_is_refused() {
    // R24: a blocked item must name an act a reachable actor can perform.
    // The state and its condition are ONE field, so blocked-without-a-
    // condition is unrepresentable; a BLANK condition is the residual hole,
    // refused at `save` — the single funnel every mutator writes through.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    let mut blank = item("blank", Window::Any);
    blank.unblock_condition = Some("   ".into());
    let err = q.add(blank).unwrap_err().to_string();
    assert!(err.contains("blank"), "the refusal names the item: {err}");
    assert!(err.contains("unblock condition"), "{err}");
    assert!(!q.path().exists(), "the refused write never created the queue file");
}

#[test]
fn a_blocked_item_is_never_stale_and_stays_actionable() {
    // KTD3, the `QueueItem` half: a blocked item past its deadline is not
    // abandoned work — its recorded unblock condition is the whole point of
    // keeping it. `is_actionable` keeps its three clauses; withholding a
    // blocked item from the report's `next:` is the report's job (U3).
    let now: DateTime<Utc> = "2026-07-29T12:00:00Z".parse().unwrap();
    let mut blocked = item("parked", Window::OpenAttended);
    blocked.deadline = Some("2026-07-28T00:00:00Z".into());
    assert!(blocked.is_stale(now).unwrap(), "unblocked and past its deadline: stale");
    blocked.unblock_condition = Some("a certified head clears its frozen margin".into());
    assert!(!blocked.is_stale(now).unwrap(), "a blocked item is never stale");
    assert!(blocked.is_actionable(now).unwrap());
}

#[test]
fn setting_priority_moves_the_marker_and_repairs_a_multi_holder_store() {
    // R20 + R1's scarcity: the set path must NOT assume the store already
    // holds at most one marker. An older binary ignored the field entirely,
    // so a store can arrive with several holders; a set converges it to
    // exactly one in the same read-mutate-save.
    //
    // Seeded by writing the JSONL DIRECTLY, because `Queue::save` now refuses
    // to persist two holders — the fixture has to reproduce what a
    // field-blind writer left behind, not route through the invariant the
    // funnel enforces.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    let mut seeded: Vec<QueueItem> =
        ["a", "b", "c"].iter().map(|id| item(id, Window::Any)).collect();
    seeded[0].priority = true;
    seeded[1].priority = true; // what an older binary could leave behind
    let raw: String =
        seeded.iter().map(|i| format!("{}\n", serde_json::to_string(i).unwrap())).collect();
    std::fs::write(q.path(), raw).unwrap();
    assert_eq!(
        q.read_all().unwrap().iter().filter(|i| i.priority).count(),
        2,
        "the fixture really does start ambiguous"
    );

    let cleared = q.set_priority("c").unwrap();
    assert_eq!(cleared, ["a", "b"], "every OTHER holder is cleared, in file order");
    let back = q.read_all().unwrap();
    let held: Vec<&str> =
        back.iter().filter(|i| i.priority).map(|i| i.id.as_str()).collect();
    assert_eq!(held, ["c"], "exactly one holder after a set");

    // Re-setting the current holder is a no-op that clears nothing.
    assert!(q.set_priority("c").unwrap().is_empty(), "no other holder to clear");
    let back = q.read_all().unwrap();
    assert_eq!(back.iter().filter(|i| i.priority).count(), 1, "still exactly one holder");
}

#[test]
fn clear_priority_leaves_no_holder_and_reports_what_it_cleared() {
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("a", Window::Any)).unwrap();
    q.add(item("b", Window::Any)).unwrap();
    q.set_priority("a").unwrap();

    assert_eq!(q.clear_priority().unwrap(), ["a"], "the cleared holder is named");
    assert!(q.read_all().unwrap().iter().all(|i| !i.priority), "no holder remains");
    assert!(q.clear_priority().unwrap().is_empty(), "clearing an unheld marker is a no-op");
    // The cleared marker leaves the line entirely (skip_serializing_if).
    let raw = std::fs::read_to_string(q.path()).unwrap();
    assert!(!raw.contains("priority"), "a cleared marker writes no key: {raw}");
}

#[test]
fn the_field_editing_mutators_refuse_an_unknown_id_without_touching_the_store() {
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("only", Window::Any)).unwrap();
    let before = std::fs::read_to_string(q.path()).unwrap();

    let errs = [
        q.set_priority("ghost").unwrap_err(),
        q.block("ghost", "the operator acts").unwrap_err(),
        q.unblock("ghost").unwrap_err(),
    ];
    for err in &errs {
        assert!(err.to_string().contains("ghost"), "the refusal names the id: {err}");
    }
    assert_eq!(
        std::fs::read_to_string(q.path()).unwrap(),
        before,
        "a refused mutator leaves the store byte-identical"
    );
}

#[test]
fn block_records_the_condition_and_unblock_restores_the_plain_item() {
    // R5/R20: the verb the U4 migration runs through. Blocked-ness and its
    // condition are ONE field, so recording the state records the act.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("parked", Window::OpenAttended)).unwrap();

    q.block("parked", "the operator authorizes a margin-clearing session").unwrap();
    let back = q.read_all().unwrap();
    assert!(back[0].is_blocked(), "the item is blocked");
    assert_eq!(
        back[0].unblock_condition.as_deref(),
        Some("the operator authorizes a margin-clearing session"),
        "the condition is recorded verbatim"
    );

    // Re-blocking REPLACES the condition rather than stacking a second one.
    q.block("parked", "a certified head clears its frozen margin").unwrap();
    assert_eq!(
        q.read_all().unwrap()[0].unblock_condition.as_deref(),
        Some("a certified head clears its frozen margin")
    );

    assert!(q.unblock("parked").unwrap(), "the item was blocked");
    assert!(!q.read_all().unwrap()[0].is_blocked(), "unblocked");
    assert!(
        !q.unblock("parked").unwrap(),
        "unblocking an unblocked item is a reported no-op, not an error"
    );
    let raw = std::fs::read_to_string(q.path()).unwrap();
    assert!(!raw.contains("unblock_condition"), "the cleared state writes no key: {raw}");
}

#[test]
fn block_with_a_blank_condition_is_refused_naming_the_item() {
    // R24 through the ONE funnel: `save` refuses before touching any path,
    // so `block` needs no second check and the store stays byte-identical.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("parked", Window::Any)).unwrap();
    let before = std::fs::read_to_string(q.path()).unwrap();

    let err = q.block("parked", "   ").unwrap_err().to_string();
    assert!(err.contains("parked"), "the refusal names the item: {err}");
    assert!(err.contains("unblock condition"), "{err}");
    assert_eq!(std::fs::read_to_string(q.path()).unwrap(), before, "store untouched");
}

#[test]
fn done_clears_the_priority_marker_only_when_it_completes() {
    // R21: the marker is the arc's frontier, so a COMPLETED head must not
    // keep it. A reconcile refusal did not complete the work — the holder
    // keeps the marker and stays actionable.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    let mut refused = item("refused", Window::Any);
    refused.completion = CompletionSignal::ToolEvent {
        event: "gate-green".into(),
        artifact: Some(tmp.path().join("absent.json").to_string_lossy().into_owned()),
    };
    q.add(refused).unwrap();
    q.set_priority("refused").unwrap();
    assert!(
        matches!(
            q.done("refused", "2026-07-30T00:00:00Z").unwrap(),
            TransitionOutcome::Reconcile(_)
        ),
        "an absent completion artifact refuses the transition"
    );
    assert!(
        q.read_all().unwrap()[0].priority,
        "a refused done keeps the marker — the work is not done"
    );

    q.add(item("real", Window::Any)).unwrap();
    q.set_priority("real").unwrap();
    assert_eq!(q.done("real", "2026-07-30T00:00:00Z").unwrap(), TransitionOutcome::Completed);
    assert!(
        q.read_all().unwrap().iter().all(|i| !i.priority),
        "a completed done leaves no holder"
    );
}

#[test]
fn supersede_transfers_the_priority_marker_only_when_it_completes() {
    // R21: the arc's frontier survives the transition its head takes. The
    // marker follows the work to the superseder; a reconcile refusal leaves
    // it where it was.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("head", Window::Any)).unwrap();
    q.set_priority("head").unwrap();

    assert!(
        matches!(
            q.supersede("head", "successor").unwrap(),
            TransitionOutcome::Reconcile(_)
        ),
        "a not-yet-existing superseder refuses the transition"
    );
    assert!(q.read_all().unwrap()[0].priority, "a refused supersede keeps the marker");

    q.add(item("successor", Window::Any)).unwrap();
    assert_eq!(q.supersede("head", "successor").unwrap(), TransitionOutcome::Completed);
    let back = q.read_all().unwrap();
    let held: Vec<&str> =
        back.iter().filter(|i| i.priority).map(|i| i.id.as_str()).collect();
    assert_eq!(held, ["successor"], "the marker follows the work to the superseder");

    // Superseding a NON-holder never grants the marker.
    q.add(item("other", Window::Any)).unwrap();
    q.add(item("other-new", Window::Any)).unwrap();
    assert_eq!(q.supersede("other", "other-new").unwrap(), TransitionOutcome::Completed);
    let back = q.read_all().unwrap();
    let held: Vec<&str> =
        back.iter().filter(|i| i.priority).map(|i| i.id.as_str()).collect();
    assert_eq!(held, ["successor"], "still exactly one holder, unchanged");
}

#[test]
fn superseding_onto_terminal_work_releases_the_marker_rather_than_parking_it() {
    // R1/R21. The report renders only actionable items, so transferring the one
    // scarce marker onto a superseder that is itself already done would hide the
    // frontier from every section while reporting success. Releasing it is the
    // honest outcome: no holder, and the operator re-pins the live successor.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("head", Window::Any)).unwrap();
    q.add(item("already-done", Window::Any)).unwrap();
    q.done("already-done", "2026-08-01T00:00:00+00:00").unwrap();
    q.set_priority("head").unwrap();

    assert_eq!(q.supersede("head", "already-done").unwrap(), TransitionOutcome::Completed);
    let back = q.read_all().unwrap();
    assert!(
        back.iter().all(|i| !i.priority),
        "the marker is RELEASED, not parked on work the report cannot show"
    );
}

#[test]
fn the_write_funnel_refuses_a_second_priority_holder() {
    // R1 enforced where every mutator passes, not only in the verb that sets it.
    // `supersede` transfers the marker, so without this guard a store carrying a
    // stale holder could gain a second one through a path `move_priority` never
    // touches. Asserted directly against `save`, because the multi-holder
    // fixtures elsewhere write raw JSONL specifically to bypass it.
    let tmp = tempfile::TempDir::new().unwrap();
    let q = Queue::new(tmp.path().join("items.jsonl"));
    q.add(item("a", Window::Any)).unwrap();
    q.add(item("b", Window::Any)).unwrap();
    let before = std::fs::read(q.path()).unwrap();

    let mut two = q.read_all().unwrap();
    two[0].priority = true;
    two[1].priority = true;
    let err = q.save(&two).unwrap_err().to_string();
    assert!(err.contains("2 priority holders"), "the refusal counts them: {err}");
    assert!(err.contains('a') && err.contains('b'), "and names them: {err}");
    assert_eq!(std::fs::read(q.path()).unwrap(), before, "a refused save leaves the store");
}
