//! `lab-next` CLI (U1, KTD3) — the queue edit surface: `add` / `done` /
//! `supersede` / `list`, plus the default window-aware report (U5) and `probe`
//! (U6) which land in later units.
//!
//! Mirrors `lab-mount-universe`'s read-only posture: no nonce, no TTY, no chain
//! append — the only writes are queue-file mutations through
//! [`crate::queue::Queue`]. `main_cli` mirrors `research.rs`: scrub install
//! first, mandatory calendar startup record, scrubbed terminal errors.

use std::process::ExitCode;

use chrono::Utc;

use crate::queue::{CompletionSignal, Queue, QueueItem, TransitionOutcome, Window};

/// A usage string enumerating the valid subcommands (KTD3).
const USAGE: &str = "usage: lab-next [report] | list [--all] | add --id <id> --title <t> --window <open-attended|closed|any> [--event <name> [--artifact <path>]] [--deadline <rfc3339>] [--sequence <name>] [--note <text>] [--ref <path>]... | done <id> | supersede <id> --by <id>";

/// The CLI entry point: install scrub, emit the mandatory calendar startup
/// record, dispatch the subcommand, and scrub any terminal error. A hygiene
/// refusal (a reconcile-flagged destructive transition) is a non-zero exit, not
/// an error.
pub fn main_cli() -> ExitCode {
    nautilus_ls::scrub::install();
    // Mandatory startup calendar record: one redacted line to the non-persisted
    // diagnostic channel (stderr). The queue edit surface itself never gates on
    // the calendar — window derivation consumes it in U2/U5.
    nautilus_ls::calendar::emit_startup_from_env("lab-next");
    match dispatch() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn dispatch() -> anyhow::Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // The default report (U5) and probe (U6) land in later units; the stub
        // notice keeps the KTD3 surface honest without pretending to answer.
        None | Some("report") => {
            println!("lab-next report: not yet implemented (U5) — use list | add | done | supersede");
            Ok(ExitCode::SUCCESS)
        }
        Some("probe") => {
            println!("lab-next probe: not yet implemented (U6)");
            Ok(ExitCode::SUCCESS)
        }
        Some("list") => run_list(&args[1..]),
        Some("add") => run_add(&args[1..]),
        Some("done") => run_done(&args[1..]),
        Some("supersede") => run_supersede(&args[1..]),
        Some(other) => anyhow::bail!("unknown subcommand {other:?}\n{USAGE}"),
    }
}

/// Render one item line for the report views.
fn render(item: &QueueItem) -> String {
    let mut line = format!("  {} [{}] {}", item.id, item.window.tag(), item.title);
    if let Some(d) = &item.deadline {
        line.push_str(&format!(" (deadline {d})"));
    }
    if let Some(seq) = &item.sequence {
        line.push_str(&format!(" (paused sequence {seq})"));
    }
    if let Some(flag) = &item.reconcile {
        line.push_str(&format!(" [reconcile: {flag}]"));
    }
    line
}

/// `list [--all]`: the actionable view (R9); `--all` appends the done / stale /
/// superseded history sections.
fn run_list(rest: &[String]) -> anyhow::Result<ExitCode> {
    let all = match rest {
        [] => false,
        [flag] if flag == "--all" => true,
        other => anyhow::bail!("unknown list arguments {other:?}\n{USAGE}"),
    };
    let queue = Queue::from_env()?;
    let items = queue.read_all()?;
    let now = Utc::now();

    let mut actionable = Vec::new();
    let mut done = Vec::new();
    let mut superseded = Vec::new();
    let mut stale = Vec::new();
    for item in &items {
        if item.is_actionable(now)? {
            actionable.push(item);
        } else if item.done_utc.is_some() {
            done.push(item);
        } else if item.superseded_by.is_some() {
            superseded.push(item);
        } else {
            stale.push(item);
        }
    }

    println!("actionable: {}", actionable.len());
    for item in &actionable {
        println!("{}", render(item));
    }
    if all {
        println!("done: {}", done.len());
        for item in &done {
            println!("{} (done {})", render(item), item.done_utc.as_deref().unwrap_or("?"));
        }
        println!("superseded: {}", superseded.len());
        for item in &superseded {
            println!(
                "{} (superseded by {})",
                render(item),
                item.superseded_by.as_deref().unwrap_or("?")
            );
        }
        println!("stale: {}", stale.len());
        for item in &stale {
            println!("{} (deadline passed)", render(item));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `add`: create one item, declaring its completion signal at creation (R8).
fn run_add(rest: &[String]) -> anyhow::Result<ExitCode> {
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
fn run_done(rest: &[String]) -> anyhow::Result<ExitCode> {
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
fn run_supersede(rest: &[String]) -> anyhow::Result<ExitCode> {
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
