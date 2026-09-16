//! The queue **edit surface** (U1, R5/R8/R9/R20/R24; KTD3/KTD6) — the six
//! subcommands that write: `add`, `done`, `supersede`, `priority`, `block`,
//! `unblock`.
//!
//! These are the only paths in `lab-next` that mutate the store, which is the
//! seam they are split on: everything else in the parent module reads. Each one
//! is the same three-step shape — parse argv into the store's own vocabulary,
//! call exactly one [`Queue`] method, print what happened — so the argv surface
//! and the store's invariants stay legible side by side rather than buried in
//! the report renderer.
//!
//! Two behaviours are load-bearing enough to state here, because the printing
//! is what an operator actually sees:
//!
//! - A **hygiene refusal** is not an error. `done` and `supersede` can come back
//!   as [`TransitionOutcome::Reconcile`] when the declared completion artifact
//!   is absent/empty or the superseder is not yet in the queue (KTD6); that
//!   prints a `reconcile:` line and exits non-zero **without** an `error:`
//!   prefix, so a caller can tell "refused, and here is the condition" from
//!   "the tool broke".
//! - `unblock` on an item that is not blocked is a reported no-op, mirroring
//!   `done`'s idempotence — it prints and exits zero rather than refusing, so a
//!   session that cannot remember whether it already unblocked does not have to
//!   read the store to find out.
//!
//! The usage string these all cite on a parse failure lives in the parent
//! ([`super::USAGE`]) rather than here: `dispatch` and `list` cite it too, so
//! keeping it in the parent avoids an inverted dependency where the parent
//! imports its own CLI contract back out of a child.

use std::process::ExitCode;

use chrono::Utc;

use crate::queue::{CompletionSignal, Queue, QueueItem, TransitionOutcome, Window};

use super::USAGE;

/// `add`: create one item, declaring its completion signal at creation (R8).
pub(super) fn run_add(rest: &[String]) -> anyhow::Result<ExitCode> {
    let mut id = None;
    let mut title = None;
    let mut window = None;
    let mut event = None;
    let mut artifact = None;
    let mut deadline = None;
    let mut sequence = None;
    let mut note = None;
    let mut refs = Vec::new();

    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| -> anyhow::Result<String> {
            it.next().cloned().ok_or_else(|| anyhow::anyhow!("{flag} requires a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--id" => id = Some(value("--id")?),
            "--title" => title = Some(value("--title")?),
            "--window" => window = Some(Window::parse(&value("--window")?)?),
            "--event" => event = Some(value("--event")?),
            "--artifact" => artifact = Some(value("--artifact")?),
            "--deadline" => deadline = Some(value("--deadline")?),
            "--sequence" => sequence = Some(value("--sequence")?),
            "--note" => note = Some(value("--note")?),
            "--ref" => refs.push(value("--ref")?),
            other => anyhow::bail!("unknown add argument {other:?}\n{USAGE}"),
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("add requires --id\n{USAGE}"))?;
    let title = title.ok_or_else(|| anyhow::anyhow!("add requires --title\n{USAGE}"))?;
    let window = window.ok_or_else(|| anyhow::anyhow!("add requires --window\n{USAGE}"))?;
    if artifact.is_some() && event.is_none() {
        anyhow::bail!("--artifact needs --event (the artifact witnesses a named tool event)");
    }
    let completion = match event {
        Some(event) => CompletionSignal::ToolEvent { event, artifact },
        None => CompletionSignal::Explicit,
    };

    let mut item = QueueItem::new(&id, &title, window, completion, Utc::now().to_rfc3339());
    item.deadline = deadline;
    item.sequence = sequence;
    item.notes = note;
    item.refs = refs;

    let queue = Queue::from_env()?;
    queue.add(item)?;
    println!("added: {id} [{}] {title}", window.tag());
    Ok(ExitCode::SUCCESS)
}

/// `done <id>` (R8/R9): complete, or refuse with a reconcile flag when the
/// declared completion artifact is absent or empty (KTD6 — a hygiene refusal
/// exits non-zero without being an error).
pub(super) fn run_done(rest: &[String]) -> anyhow::Result<ExitCode> {
    let [id] = rest else {
        anyhow::bail!("done takes exactly one <id>\n{USAGE}");
    };
    let queue = Queue::from_env()?;
    match queue.done(id, &Utc::now().to_rfc3339())? {
        TransitionOutcome::Completed => {
            println!("done: {id}");
            Ok(ExitCode::SUCCESS)
        }
        TransitionOutcome::Reconcile(flag) => {
            println!("reconcile: {id} — {flag}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// `supersede <id> --by <id>` (R9): record the replacement, or refuse with a
/// reconcile flag when the superseder is not yet in the queue (KTD6).
pub(super) fn run_supersede(rest: &[String]) -> anyhow::Result<ExitCode> {
    let (id, by) = match rest {
        [id, flag, by] if flag == "--by" => (id, by),
        other => anyhow::bail!("supersede takes <id> --by <id>, got {other:?}\n{USAGE}"),
    };
    let queue = Queue::from_env()?;
    match queue.supersede(id, by)? {
        TransitionOutcome::Completed => {
            println!("superseded: {id} by {by}");
            Ok(ExitCode::SUCCESS)
        }
        TransitionOutcome::Reconcile(flag) => {
            println!("reconcile: {id} — {flag}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// `priority <id>` | `priority --clear` (R20): move the single-item priority
/// marker, or leave no holder. Setting it clears every other holder, so the
/// store converges to exactly one even when it arrived with several (KTD6).
pub(super) fn run_priority(rest: &[String]) -> anyhow::Result<ExitCode> {
    let target = match rest {
        [flag] if flag == "--clear" => None,
        [id] if !id.starts_with("--") => Some(id.as_str()),
        other => anyhow::bail!("priority takes <id> or --clear, got {other:?}\n{USAGE}"),
    };
    let queue = Queue::from_env()?;
    let cleared = match target {
        Some(id) => {
            let cleared = queue.set_priority(id)?;
            println!("priority: {id}");
            cleared
        }
        None => {
            let cleared = queue.clear_priority()?;
            println!("priority: none");
            cleared
        }
    };
    if !cleared.is_empty() {
        println!("cleared: {}", cleared.join(", "));
    }
    Ok(ExitCode::SUCCESS)
}

/// `block <id> --until <condition>` (R5/R20): record the blocked state with the
/// act that would unblock it. The condition is mandatory — a blocked item that
/// names no reachable act is refused by the store (R24).
pub(super) fn run_block(rest: &[String]) -> anyhow::Result<ExitCode> {
    let (id, condition) = match rest {
        [id, flag, condition] if flag == "--until" => (id, condition),
        other => anyhow::bail!("block takes <id> --until <condition>, got {other:?}\n{USAGE}"),
    };
    let queue = Queue::from_env()?;
    queue.block(id, condition)?;
    println!("blocked: {id} — until {condition}");
    Ok(ExitCode::SUCCESS)
}

/// `unblock <id>` (R20): clear the blocked state. Unblocking an item that is not
/// blocked is a reported no-op, mirroring `done`'s idempotence.
pub(super) fn run_unblock(rest: &[String]) -> anyhow::Result<ExitCode> {
    let [id] = rest else {
        anyhow::bail!("unblock takes exactly one <id>\n{USAGE}");
    };
    let queue = Queue::from_env()?;
    if queue.unblock(id)? {
        println!("unblocked: {id}");
    } else {
        println!("unblocked: {id} (was not blocked)");
    }
    Ok(ExitCode::SUCCESS)
}
