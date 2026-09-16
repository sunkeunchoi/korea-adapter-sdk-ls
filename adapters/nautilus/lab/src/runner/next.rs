//! `lab-next` CLI (U1/U5/U6, KTD3) — the queue edit surface (`add` / `done` /
//! `supersede` / `priority` / `block` / `unblock` / `list`), the default
//! window-aware entry report, and the
//! `probe` resume-probe gate (U6, R14; KTD5).
//!
//! Mirrors `lab-mount-universe`'s read-only posture: no nonce, no chain
//! append — the only writes are queue-file mutations through
//! [`crate::queue::Queue`]. `main_cli` mirrors `research.rs`: scrub install
//! first, mandatory calendar startup record, scrubbed terminal errors.
//!
//! ## The default report (U5/U3 — R1/R2/R3/R4/R5/R12/R13/R22/R23; KTD1/KTD3/KTD4/KTD5)
//!
//! One `now` is read at the top (test override: [`NOW_UNIX_ENV`], the
//! `LS_DISPATCH_NOW_UNIX` stub idiom), the calendar is resolved ONCE and
//! mapped to [`DateEvidence`], and the window derives via
//! [`derive_window`] (KTD1). In-flight sequences come from the U3 readers
//! plus the U4 gate leg ([`gate_sequence`], parsing `gate-run.sh --status`;
//! test override: [`GATE_STATUS_FILE_ENV`], a file of pre-captured `--status`
//! output). Selection is R4: a current-window-compatible in-flight sequence
//! outranks new items (KTD5 — priority orders ITEMS only, so a resumable
//! sequence never loses the offer slot); remaining eligible items order by
//! the single priority marker (R1), then recorded deadline, then queue order
//! ([`by_priority_then_deadline`], sorted stably); window-incompatible
//! in-flight sequences stay visible as `[paused]` resumable work. Every offer
//! carries an executable command or exact step (R5) and the item's reference
//! paths (R13).
//!
//! Sections print in this order: `window:`, `repair:` (genuinely-unknown
//! only), `attended chain:` (presumed-open only), `in-flight:`, `reconciled:`,
//! `standing:`, `next:`, `queue:`.
//!
//! ## Standing work (R3/R23; KTD4)
//!
//! A blocked item ([`QueueItem::is_blocked`] — the recorded unblock condition
//! IS the state) renders in the `standing:` section between `reconciled:` and
//! `next:`, and NOWHERE else: it is never the `next:` offer and never listed
//! under `queue:`, so one item is one rendering even when it also carries a
//! paused-sequence label. Standing work is sourced BEFORE the window filter
//! (R23) — the external act it waits on does not care which seam we are in,
//! so it renders whatever window is derived; only the offer path stays
//! window-gated. The section is deliberately OUTSIDE the executable-offer
//! guard (R5, enforced by `tests/next_cli.rs`'s `assert_offers_are_executable`
//! over `in-flight:` / `next:` / `queue:`): a blocked head has no executable
//! line by construction, so its detail line is `blocked: <condition>` rather
//! than `run:`. When EVERY open item is blocked, the `next:` line names the
//! head's unblock condition and its `lab-next unblock` command instead of
//! advising `lab-next add`.
//!
//! ## R12 reconciliation rule (documented contract)
//!
//! At each sit-down, BEFORE offering: items whose declared tool-completion
//! artifact now witnesses (same predicate as `done`) are auto-closed with a
//! printed notice. Done-or-not confirmation is asked ONLY for
//! `explicit`-signal items that are actionable and either carry a `reconcile`
//! flag or are past a recorded deadline (only sequence-exempt entries can be
//! past-deadline yet actionable). With a TTY on stdin the ask is a
//! `done? [y/N]` prompt; without one (agent sessions, tests) it is a flagged
//! `confirm:` line and the item stays actionable.
//!
//! Blocked items are excluded from BOTH passes (R4/R22): never auto-closed
//! (a present artifact is not the external party's act) and never prompted.
//! The staleness exemption behind that is wired at BOTH sites — the store's
//! [`QueueItem::is_stale`] and this module's [`deadline_passed`] (KTD3);
//! exempting only the first would keep blocked work in the confirmation pass.
//!
//! ## The resume probe (U6 — R14; KTD5)
//!
//! `lab-next probe` verifies, per R10 sequence (turn, ladder prep, ingest,
//! gate run) against the CURRENT environment's real stores (the same
//! resolution the report uses), three things: the state store is readable, a
//! stage is derivable, and a resume command is printable. The verdict rule is
//! uniform: an ABSENT or UNREADABLE store is a probe FAILURE naming what is
//! missing (the probe demonstrates resumability — nothing to read is not
//! demonstrated); a READABLE store is `ok` whatever it says, including the
//! rung-0 fail-closed chain verdict and not-in-flight states. Output is one
//! `ok[<sequence>] stage: <stage>; resume: <resume>` or `FAIL[<sequence>]
//! missing: <what>` line per sequence, a `probe: PASS|FAIL — report <path>`
//! summary line, and a summary JSON written atomically (tmp+rename) next to
//! the queue (`queue/probe-report.json`; [`PROBE_REPORT_PATH_ENV`] overrides
//! for tests) — the evidence the U7 cutover verdict embeds. Exit 0 = all four
//! pass, 1 otherwise. The probe never mutates any sequence store; the report
//! JSON is its only write.

use std::io::IsTerminal;
use std::process::ExitCode;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Asia::Seoul;

use nautilus_ls::calendar::LoadedCalendar;

use crate::dispatch::checks::date_fact_from_view;
use crate::queue::sequences::{
    read_sequences, SequenceKind, SequenceReport, SequenceStores,
};
use crate::queue::window::{
    derive_window, ClosedReason, DateEvidence, NextBoundary, UnknownReason, WindowReport,
    WindowState,
};
use crate::queue::{
    anchored, artifact_witnesses, CompletionSignal, Queue, QueueItem, TransitionOutcome, Window,
};

/// Test-time clock override (seconds since the epoch) for the report's single
/// `now` read, mirroring the `LS_DISPATCH_NOW_UNIX` stub idiom — window-state
/// tests pin the KST instant here; unset means the wall clock.
pub const NOW_UNIX_ENV: &str = "LS_NEXT_NOW_UNIX";

/// Gate-leg override: a file holding pre-captured `scripts/gate-run.sh
/// --status` OUTPUT. Set → the file is parsed instead of running the script
/// (a missing/unreadable file reads as "no gate state" — the hermetic-test
/// escape hatch). Unset → the real script runs, but only when
/// `.gate-run/state.json` exists at the repo root.
pub const GATE_STATUS_FILE_ENV: &str = "LS_GATE_STATUS_FILE";

mod probe;
mod verbs;

pub use probe::{PROBE_REPORT_PATH_ENV, PROBE_REPORT_RELPATH};

/// A usage string enumerating the valid subcommands (KTD3). It stays in the
/// parent rather than moving beside the verbs that cite it, because `dispatch`
/// and `list` cite it too: keeping it here avoids the parent importing its own
/// CLI contract back out of a child.
const USAGE: &str = "usage: lab-next [report] | probe | list [--all] | add --id <id> --title <t> --window <open-attended|closed|any> [--event <name> [--artifact <path>]] [--deadline <rfc3339>] [--sequence <name>] [--note <text>] [--ref <path>]... | done <id> | supersede <id> --by <id> | priority <id> | priority --clear | block <id> --until <condition> | unblock <id>";

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
        None => run_report(&[]),
        Some("report") => run_report(&args[1..]),
        Some("probe") => probe::run_probe(&args[1..]),
        Some("list") => run_list(&args[1..]),
        Some("add") => verbs::run_add(&args[1..]),
        Some("done") => verbs::run_done(&args[1..]),
        Some("supersede") => verbs::run_supersede(&args[1..]),
        Some("priority") => verbs::run_priority(&args[1..]),
        Some("block") => verbs::run_block(&args[1..]),
        Some("unblock") => verbs::run_unblock(&args[1..]),
        Some(other) => anyhow::bail!("unknown subcommand {other:?}\n{USAGE}"),
    }
}

/// The shared item head line: `  <id> [<window>] <title>` plus the deadline
/// and paused-sequence suffixes.
fn item_head(item: &QueueItem) -> String {
    let mut head = format!("  {} [{}] {}", item.id, item.window.tag(), item.title);
    if let Some(d) = &item.deadline {
        head.push_str(&format!(" (deadline {d})"));
    }
    if let Some(seq) = &item.sequence {
        head.push_str(&format!(" (paused sequence {seq})"));
    }
    head
}

/// Render one item line for the `list` views: the head plus the reconcile
/// suffix (`list` never runs the sit-down reconcile pass, so the flag rides
/// the line itself).
fn render(item: &QueueItem) -> String {
    let mut line = item_head(item);
    if let Some(flag) = &item.reconcile {
        line.push_str(&format!(" [reconcile: {flag}]"));
    }
    line
}

/// The item's recorded deadline as a UTC instant (`None` when absent or — in
/// the report's tolerant paths — unparseable).
fn parse_deadline(item: &QueueItem) -> Option<DateTime<Utc>> {
    item.deadline
        .as_deref()
        .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|d| d.with_timezone(&Utc))
}

// ===========================================================================
// U5 — the default window-aware report (R1/R2/R4/R5/R12/R13; KTD1/KTD3)
// ===========================================================================

/// The report's single `now`: the [`NOW_UNIX_ENV`] override when set and
/// parseable, else the wall clock. Read ONCE at the top of the report.
fn report_now() -> DateTime<Utc> {
    std::env::var(NOW_UNIX_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .and_then(|secs| Utc.timestamp_opt(secs, 0).single())
        .unwrap_or_else(Utc::now)
}

/// The queue order (R1; KTD5): the single priority holder first, then recorded
/// deadline (dated before undated), then queue file order — the callers sort
/// stably, so equal keys keep the order the store was read in. Priority is a
/// PRE-key on the existing deadline sort: it orders ITEMS only, and never
/// reaches the sequence-versus-item precedence the offer slot is decided by.
fn by_priority_then_deadline(a: &&QueueItem, b: &&QueueItem) -> std::cmp::Ordering {
    b.priority.cmp(&a.priority).then_with(|| match (parse_deadline(a), parse_deadline(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    })
}

/// The default subcommand: reconcile (R12), derive the window (KTD1), read the
/// in-flight sequences (U3 + the gate leg), select (R4), and print the
/// line-oriented report — every offer executable (R5) with refs (R13).
fn run_report(rest: &[String]) -> anyhow::Result<ExitCode> {
    if !rest.is_empty() {
        anyhow::bail!("report takes no arguments, got {rest:?}\n{USAGE}");
    }
    let now = report_now();
    let queue = Queue::from_env()?;

    // R12 — reconcile the queue against reality BEFORE offering anything
    // (prompts, if any, happen here; the report body prints after).
    let reconciled = reconcile(&queue, now)?;

    // KTD1 — resolve the calendar ONCE and map the resolution to DateEvidence.
    let snapshot = nautilus_ls::calendar::snapshot_path_from_env();
    let loaded = nautilus_ls::calendar::resolve_and_load(
        snapshot.as_deref(),
        now,
        nautilus_ls::calendar::adoption_from_env(),
    );
    let view = loaded.calendar().and_then(|cal| cal.as_of(now).ok());
    let kst_today = now.with_timezone(&Seoul).date_naive();
    let today = match &loaded {
        LoadedCalendar::NotConfigured => DateEvidence::NotConfigured,
        LoadedCalendar::Unavailable(_) => DateEvidence::Unavailable,
        // Loaded but unusable at this instant (as-of out of range) fails
        // closed as Unavailable — never a fabricated date fact.
        LoadedCalendar::Available(_) => match &view {
            Some(v) => DateEvidence::Fact(date_fact_from_view(Some(v), kst_today)),
            None => DateEvidence::Unavailable,
        },
    };
    let window = derive_window(now, today, |d| date_fact_from_view(view.as_ref(), d));

    // In-flight sequences: the R10 legs (turn, ladder, ingest) + the gate leg.
    let mut sequences = read_sequences(&SequenceStores::from_env(), now);
    if let Some(gate) = gate_sequence() {
        sequences.push(gate);
    }

    // R4 — window compatibility splits in-flight work into active / paused.
    let (active, paused): (Vec<&SequenceReport>, Vec<&SequenceReport>) =
        sequences.iter().partition(|s| window.state.admits(sequence_window(s)));

    // Standing work (R3/R23) and eligible work, from ONE read. Standing is
    // sourced BEFORE the window filter — a blocked item renders whatever
    // window is derived, because the external act it waits on does not care
    // which seam we are in; only the offer stays window-gated. Eligible items
    // are actionable, unblocked AND admitted by the current window (R3 falls
    // out of `admits` — genuinely-unknown admits only `any`). Both orders are
    // [`by_priority_then_deadline`]; `open_unblocked` records whether any
    // actionable unblocked work exists at all, in any window.
    let items = queue.read_all()?;
    let mut standing: Vec<&QueueItem> = Vec::new();
    let mut eligible: Vec<&QueueItem> = Vec::new();
    let mut open_unblocked = false;
    for item in &items {
        if !item.is_actionable(now)? {
            continue;
        }
        // A blocked item is standing work and nothing else: withholding it
        // from the offer sections is what keeps one item to one rendering.
        if item.is_blocked() {
            standing.push(item);
            continue;
        }
        open_unblocked = true;
        if window.state.admits(item.window) {
            eligible.push(item);
        }
    }
    standing.sort_by(by_priority_then_deadline);
    eligible.sort_by(by_priority_then_deadline);

    // ---- render ----------------------------------------------------------
    let mut out: Vec<String> = Vec::new();
    out.push(window_line(&window));
    if let WindowState::GenuinelyUnknown(reason) = window.state {
        out.push(format!("repair: {}", reason.repair_action()));
    }
    if let Some(step) = window.next_attended_step {
        out.push(format!(
            "attended chain: {} — {} (deadline {:02}:{:02} KST)",
            step.name,
            step.action,
            step.deadline_min / 60,
            step.deadline_min % 60
        ));
    }

    if sequences.is_empty() {
        out.push("in-flight: none".to_string());
    } else {
        out.push("in-flight:".to_string());
        for seq in &active {
            push_sequence(&mut out, seq, None);
        }
        for seq in &paused {
            push_sequence(&mut out, seq, Some(sequence_window(seq)));
        }
    }

    if !reconciled.is_empty() {
        out.push("reconciled:".to_string());
        out.extend(reconciled);
    }

    // KTD4 — standing work renders in its own section between `reconciled:`
    // and `next:`: visible, never offered. It is deliberately OUTSIDE the
    // executable-offer guard (a blocked head has no executable line by
    // construction — the act that would unblock it is not ours to run), so
    // the detail line is the unblock condition instead.
    if !standing.is_empty() {
        out.push("standing:".to_string());
        for item in &standing {
            push_standing(&mut out, item);
        }
    }

    // R4 top offer: the first current-window-compatible in-flight sequence,
    // else the first eligible item; R5 even when nothing is eligible.
    let mut remaining = eligible;
    if let Some(seq) = active.first() {
        out.push("next:".to_string());
        push_sequence(&mut out, seq, None);
    } else if !remaining.is_empty() {
        let top = remaining.remove(0);
        out.push("next:".to_string());
        push_item(&mut out, top);
    } else if let (false, Some(head)) = (open_unblocked, standing.first()) {
        // Every open item is blocked: the repair action and `lab-next add`
        // are both the wrong advice — name the act that would free the work.
        out.push(format!(
            "next: none offerable — every open item is standing work; when {}: lab-next unblock {}",
            head.unblock_reason(),
            head.id
        ));
    } else if matches!(window.state, WindowState::GenuinelyUnknown(_)) {
        out.push(
            "next: none eligible — run the repair action above, or queue window-agnostic work: lab-next add --window any"
                .to_string(),
        );
    } else {
        out.push("next: none eligible in this window — queue work with: lab-next add".to_string());
    }

    if remaining.is_empty() {
        out.push("queue: none".to_string());
    } else {
        out.push("queue:".to_string());
        for item in &remaining {
            push_item(&mut out, item);
        }
    }

    for line in out {
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The `window:` head line: state tag + reason detail + next boundary.
fn window_line(report: &WindowReport) -> String {
    let boundary = match report.next_boundary {
        Some(NextBoundary::Close(t)) => {
            format!("; next boundary close {}", t.to_rfc3339_opts(SecondsFormat::Secs, true))
        }
        Some(NextBoundary::Open(t)) => {
            format!("; next boundary open {}", t.to_rfc3339_opts(SecondsFormat::Secs, true))
        }
        None => String::new(),
    };
    match report.state {
        WindowState::PresumedOpen => {
            format!("window: presumed-open (inside the 09:00-15:30 KST session window{boundary})")
        }
        WindowState::KnownClosed(ClosedReason::ClosureDate) => {
            format!("window: known-closed (proven closure date{boundary})")
        }
        WindowState::KnownClosed(ClosedReason::OutsideHours) => {
            format!("window: known-closed (outside the 09:00-15:30 KST session window{boundary})")
        }
        WindowState::GenuinelyUnknown(reason) => {
            let why = match reason {
                UnknownReason::NotConfigured => "no calendar snapshot configured",
                UnknownReason::Unavailable => "the configured calendar snapshot failed to load",
                UnknownReason::OutOfCoverage => "today is outside snapshot coverage",
            };
            format!("window: genuinely-unknown ({why}; fail closed — only any-window items are eligible)")
        }
    }
}

/// The window a sequence's resume step needs (R4's sequence side): every
/// sequence is closed-window work EXCEPT the ladder resume at the mount step
/// (a green unconsumed dispatch → `lab-live --mount`), which is open-attended.
fn sequence_window(seq: &SequenceReport) -> Window {
    match seq.kind {
        SequenceKind::Ladder if seq.resume.contains("--mount") => Window::OpenAttended,
        _ => Window::Closed,
    }
}

/// One sequence offer block: head (`[paused]`-tagged when window-incompatible),
/// then the executable resume line (R5), then report-only notes.
fn push_sequence(out: &mut Vec<String>, seq: &SequenceReport, paused_needs: Option<Window>) {
    match paused_needs {
        None => out.push(format!("  {} {}", seq.kind.tag(), seq.stage)),
        Some(req) => out.push(format!(
            "  [paused] {} {} (needs {} window)",
            seq.kind.tag(),
            seq.stage,
            req.tag()
        )),
    }
    out.push(format!("    resume: {}", seq.resume));
    for d in &seq.detail {
        out.push(format!("    note: {d}"));
    }
}

/// One queue-item offer block: head, the executable handoff (R5 — for plain
/// items the handoff is `lab-next done <id>` after doing the titled work),
/// then refs (R13) and notes.
fn push_item(out: &mut Vec<String>, item: &QueueItem) {
    out.push(item_head(item));
    let run = match &item.completion {
        CompletionSignal::Explicit => {
            format!("do the titled work, then close it: lab-next done {}", item.id)
        }
        CompletionSignal::ToolEvent { event, artifact: Some(path) } => format!(
            "do the titled work; auto-closes once event {event:?} writes {path} (or close it: lab-next done {})",
            item.id
        ),
        CompletionSignal::ToolEvent { event, artifact: None } => format!(
            "do the titled work, then close it after event {event:?}: lab-next done {}",
            item.id
        ),
    };
    out.push(format!("    run: {run}"));
    push_refs_and_notes(out, item);
}

/// One standing-work block (R3; KTD4): the same head every other section uses
/// — so a blocked item that also carries a paused-sequence label keeps that
/// label rather than being claimed twice — then the unblock condition in the
/// executable line's slot, then refs (R13) and notes. There is no `run:` here
/// by construction: the act that would unblock the item is not ours to run.
fn push_standing(out: &mut Vec<String>, item: &QueueItem) {
    out.push(item_head(item));
    out.push(format!("    blocked: {}", item.unblock_reason()));
    push_refs_and_notes(out, item);
}

/// The detail tail both offer and standing blocks carry: reference paths (R13)
/// then notes, each omitted when empty. Shared so the R13 contract the module
/// doc states for BOTH sections has one edit site rather than two.
fn push_refs_and_notes(out: &mut Vec<String>, item: &QueueItem) {
    if !item.refs.is_empty() {
        out.push(format!("    refs: {}", item.refs.join(", ")));
    }
    if let Some(n) = &item.notes {
        out.push(format!("    note: {n}"));
    }
}

/// Whether the item's recorded deadline has passed (unlike
/// [`QueueItem::is_stale`], WITHOUT the sequence exemption — reconciliation
/// asks about exactly the entries the exemption keeps actionable).
///
/// KTD3 — the BLOCKED exemption is wired here as well as in
/// [`QueueItem::is_stale`]: this is the second staleness site, so exempting
/// only the first would leave blocked work asked to confirm it is done (R4).
fn deadline_passed(item: &QueueItem, now: DateTime<Utc>) -> bool {
    !item.is_blocked() && parse_deadline(item).is_some_and(|d| now > d)
}

/// R12 sit-down reconciliation (see the module doc for the confirmation rule).
/// Returns the printed `reconciled:` section lines; the only writes are
/// ordinary `done` transitions through the queue's edit surface.
fn reconcile(queue: &Queue, now: DateTime<Utc>) -> anyhow::Result<Vec<String>> {
    let mut lines = Vec::new();

    // One read serves both passes: auto-close only mutates tool-event items,
    // and nothing ever rewrites an item's completion kind, so the explicit
    // items the second pass filters are invariant across the first.
    let items = queue.read_all()?;

    // Auto-close: a declared tool-completion artifact that now witnesses the
    // event completes the item through the ordinary hygiene-checked `done`.
    // R22 — a BLOCKED item is excluded: the artifact may sit there while the
    // external act the item waits on has not happened, and a witnessed file is
    // not the operator's authorization.
    for item in &items {
        if !item.is_actionable(now)? || item.is_blocked() {
            continue;
        }
        let CompletionSignal::ToolEvent { event, artifact: Some(path) } = &item.completion else {
            continue;
        };
        // Same predicate + repo-root anchoring as `done` itself (a relative
        // artifact path is repo-root-relative), so the pre-check and the
        // transition can never disagree.
        if !artifact_witnesses(&anchored(path)) {
            continue;
        }
        match queue.done(&item.id, &now.to_rfc3339())? {
            TransitionOutcome::Completed => lines.push(format!(
                "  auto-closed: {} (artifact {path} present — event {event:?})",
                item.id
            )),
            // Unreachable in practice (the artifact was just witnessed);
            // surfaced rather than swallowed if a race ever lands here.
            TransitionOutcome::Reconcile(flag) => {
                lines.push(format!("  confirm: {} — {flag}", item.id));
            }
        }
    }

    // Done-or-not confirmation for explicit-signal items (reconcile-flagged
    // or past a recorded deadline — the documented rule). R4/R22 — blocked
    // items are excluded here too: work awaiting an external party is never
    // asked to confirm it is done.
    let tty = std::io::stdin().is_terminal();
    for item in &items {
        if !item.is_actionable(now)?
            || item.is_blocked()
            || item.completion != CompletionSignal::Explicit
        {
            continue;
        }
        let why = if let Some(flag) = &item.reconcile {
            format!("reconcile flag: {flag}")
        } else if deadline_passed(item, now) {
            format!("past its recorded deadline {}", item.deadline.as_deref().unwrap_or("?"))
        } else {
            continue;
        };
        if tty {
            print!("confirm: {} — done? [y/N] ", item.id);
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer)?;
            if answer.trim().eq_ignore_ascii_case("y") {
                queue.done(&item.id, &now.to_rfc3339())?;
                lines.push(format!("  closed: {} (operator confirmed done)", item.id));
            } else {
                lines.push(format!("  kept: {} (operator says still open; {why})", item.id));
            }
        } else {
            // Agent sessions (non-TTY): a flagged line, never a prompt; the
            // item stays actionable.
            lines.push(format!(
                "  confirm: {} — done? close with `lab-next done {}` or leave queued ({why})",
                item.id, item.id
            ));
        }
    }
    Ok(lines)
}

// ---------------------------------------------------------------------------
// Gate leg (U4 `--status` composition; the U6 probe reuses these)
// ---------------------------------------------------------------------------

/// The gate leg as a sequence report: parse `gate-run.sh --status` output —
/// from the [`GATE_STATUS_FILE_ENV`] override when set, else by running the
/// real script (only when `.gate-run/state.json` exists, so a repo that never
/// ran the gate driver stays silent). `None` = no gate run in flight.
pub fn gate_sequence() -> Option<SequenceReport> {
    parse_gate_status(&gate_status_text()?)
}

/// The raw `--status` text (see [`gate_sequence`] for the source rules).
fn gate_status_text() -> Option<String> {
    resolve_gate_status().ok()
}

/// Resolve the raw `--status` text from the [`GATE_STATUS_FILE_ENV`] override
/// or the real script. `Err` carries the what's-missing text: the report
/// discards it into "no gate sequence", the probe surfaces it as the FAIL
/// reason.
fn resolve_gate_status() -> Result<String, String> {
    if let Some(path) =
        std::env::var(GATE_STATUS_FILE_ENV).ok().filter(|s| !s.trim().is_empty())
    {
        // Override: pre-captured output; missing/unreadable = no gate state.
        return std::fs::read_to_string(&path).map_err(|e| {
            format!("gate status file {path} ({GATE_STATUS_FILE_ENV}) absent or unreadable: {e}")
        });
    }
    let root = crate::queue::repo_root()
        .map_err(|e| format!("no repo root to locate .gate-run: {e}"))?;
    let state = root.join(".gate-run").join("state.json");
    if !state.exists() {
        return Err(format!(
            "gate state {} absent — no gate run recorded (start one: make gate-run)",
            state.display()
        ));
    }
    let out = std::process::Command::new("bash")
        .arg(root.join("scripts").join("gate-run.sh"))
        .arg("--status")
        .current_dir(&root)
        .output()
        .map_err(|e| format!("running gate-run.sh --status: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "gate state {} present but gate-run.sh --status failed (exit {:?})",
            state.display(),
            out.status.code()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parse the STABLE `--status` contract (`step=<n> name=<name>
/// status=done|failed|pending fingerprint=<hex64|->` lines + `next=<name|none>`).
/// `next=none` (gate green) and all-pending (never started, or a fully
/// invalidated recording) are NOT in-flight; a failed step or a done prefix is.
pub fn parse_gate_status(text: &str) -> Option<SequenceReport> {
    let mut total = 0usize;
    let mut done = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let mut next: Option<&str> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("next=") {
            next = Some(rest.trim());
            continue;
        }
        if !line.starts_with("step=") {
            continue;
        }
        total += 1;
        let mut name = "";
        let mut status = "";
        for field in line.split_whitespace() {
            if let Some(v) = field.strip_prefix("name=") {
                name = v;
            } else if let Some(v) = field.strip_prefix("status=") {
                status = v;
            }
        }
        match status {
            "done" => done += 1,
            "failed" => failed.push(name.to_string()),
            _ => {}
        }
    }
    let next = next?;
    if next == "none" || (done == 0 && failed.is_empty()) {
        return None;
    }
    let detail = failed
        .iter()
        .map(|n| format!("step {n} recorded FAILED — the resume re-runs from it"))
        .collect();
    Some(SequenceReport {
        kind: SequenceKind::Gate,
        stage: format!("resumable at step {next} ({done}/{total} done)"),
        resume: format!("make gate-run (resumes at {next})"),
        detail,
    })
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
