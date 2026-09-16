//! The priority marker and the blocked mutator verbs (U2, R5/R20/R21; KTD6;
//! AE7/AE10).
//!
//! Both are single-field state whose invariant is enforced ON WRITE, so their
//! tests pair naturally: setting priority clears every other holder, and a
//! blocked item always carries the act that frees it.

use std::path::Path;

use nautilus_ls_lab::queue::Queue;
use tempfile::TempDir;

use crate::fixture::*;

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
    // Seed what a binary that ignored the field could leave behind: TWO holders.
    // The set path must CONVERGE the store, not assume it is clean. Written as raw
    // JSONL because `Queue::save` now refuses to persist an ambiguous frontier —
    // the fixture has to reproduce a field-blind writer's output rather than route
    // through the invariant the funnel enforces.
    let store = Queue::new(&queue);
    let mut items = store.read_all().unwrap();
    items[0].priority = true;
    items[1].priority = true;
    let raw: String =
        items.iter().map(|i| format!("{}\n", serde_json::to_string(i).unwrap())).collect();
    std::fs::write(&queue, raw).unwrap();
    assert_eq!(holders(&queue).len(), 2, "the fixture really does start ambiguous");

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

