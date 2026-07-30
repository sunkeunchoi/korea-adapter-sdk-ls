//! Sequence state readers (U3, R10; KTD7) — read-only adapters that turn the
//! heterogeneous existing stores into one "in-flight sequence + stage + resume
//! command" report for the entry point (U5) and the resume probe (U6).
//!
//! Three legs, each over machinery that already exists:
//!
//! - **Ladder / session prep** — `dispatch/chain.jsonl` under the data home,
//!   read through [`DispatchChain::load`]. The verdict is surfaced **as the
//!   chain machinery reports it**: a defective chain is the fail-closed rung-0
//!   verdict, never re-derived and never an error. A green unconsumed dispatch
//!   resumes at the mount step; a consumed dispatch whose mounted run never
//!   finalized is the AE5 "consumed-but-unfinished" prep.
//! - **Ingest** — `catalog/ingest-checkpoint.json`, summarized generically
//!   (watermark frontier + basis-shift marks, the append-refusal state) without
//!   loading it through the migrating [`Checkpoint::load`] path — loading can
//!   rewrite legacy shapes in memory and this reader must stay inert.
//! - **Turn** — the run registry (a `.tmp-` staging dir is an aborted run,
//!   reported and NEVER deleted, per `artifacts::aborted_runs`), the trials
//!   ledger (the last recorded look names the candidate), and the optional
//!   `LS_GOVERNED_STAGELOG` file (the last recorded stage). Turns stay one-shot
//!   (KTD7): the resume command is the recorded next `turn governed`
//!   invocation, never a mid-run checkpoint.
//!
//! Everything here is **read-only**: report, never delete, never repair. A
//! store that has never existed reads as "no in-flight sequence" (`None`), so
//! the entry point works before any sequence ever ran. [`SequenceKind::Gate`]
//! is reserved for the U4 gate-run state (`.gate-run/state.json`), which the
//! U6 probe composes — no reader for it lives here.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::artifacts::aborted_runs;
use crate::dispatch::chain::{kst_trading_date, ChainStatus, DispatchChain, MountAuthz};
use crate::trials::TrialsLedger;

/// Which R10 sequence a report row describes. `Gate` is reserved for U4's
/// gate-run driver state so the report type stays open to the fourth leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceKind {
    /// A strategy turn (run registry + trials ledger + stage log).
    Turn,
    /// Ladder session prep (the dispatch chain).
    Ladder,
    /// Catalog ingest (the ingest checkpoint).
    Ingest,
    /// A gate run (`.gate-run/state.json`, U4 — no reader here).
    Gate,
}

impl SequenceKind {
    /// The kebab-case tag rendered in reports.
    pub fn tag(self) -> &'static str {
        match self {
            SequenceKind::Turn => "turn",
            SequenceKind::Ladder => "ladder",
            SequenceKind::Ingest => "ingest",
            SequenceKind::Gate => "gate-run",
        }
    }
}

/// One in-flight sequence: which sequence, where it stands, and the exact
/// resume command/step (R5 — never a runbook pointer alone). "No in-flight
/// sequence" is the ABSENCE of a row (the leg readers return `Option`), so an
/// empty [`read_sequences`] vec means "no in-flight sequences".
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceReport {
    /// Which sequence.
    pub kind: SequenceKind,
    /// The recorded stage, as the owning machinery reports it.
    pub stage: String,
    /// The executable resume command or exact next step.
    pub resume: String,
    /// Supplementary report-only lines (aborted residue, shift marks,
    /// kill-switch state, ledger context).
    pub detail: Vec<String>,
}

/// Where the sequence stores live. Tests construct this directly with tempdir
/// paths; the CLI resolves it once via [`SequenceStores::from_env`].
#[derive(Debug, Clone, Default)]
pub struct SequenceStores {
    /// `LS_DATA_HOME` — hosts `dispatch/chain.jsonl`, `runs/`, and
    /// `catalog/ingest-checkpoint.json`. `None` = never configured: every
    /// data-home-backed leg reads as not in flight.
    pub data_home: Option<PathBuf>,
    /// The trials ledger path (`LS_TRIALS_LEDGER` override, else the tracked
    /// `ledger/trials.jsonl` under the lab crate root).
    pub trials_ledger: Option<PathBuf>,
    /// The optional governed stage log (`LS_GOVERNED_STAGELOG`).
    pub stage_log: Option<PathBuf>,
}

impl SequenceStores {
    /// Resolve the stores from the environment: `LS_DATA_HOME` (optional —
    /// absent means no data-home-backed sequences), `LS_TRIALS_LEDGER`
    /// (defaulting to the tracked lab-crate ledger, the `trials_ledger_from_env`
    /// idiom), and `LS_GOVERNED_STAGELOG` (optional).
    pub fn from_env() -> Self {
        let non_empty = |key: &str| {
            std::env::var(key).ok().filter(|s| !s.trim().is_empty()).map(PathBuf::from)
        };
        SequenceStores {
            data_home: non_empty("LS_DATA_HOME"),
            trials_ledger: Some(
                crate::runner::research::trials_ledger_from_env().path().to_path_buf(),
            ),
            stage_log: non_empty("LS_GOVERNED_STAGELOG"),
        }
    }
}

/// Read every in-flight sequence, in the fixed R10 leg order (turn, ladder,
/// ingest; the gate leg is U4's `--status` and composes at U5/U6). Empty =
/// no in-flight sequences — the legitimate pre-first-sequence state, never an
/// error. Purely read-only.
pub fn read_sequences(stores: &SequenceStores, now: DateTime<Utc>) -> Vec<SequenceReport> {
    let mut out = Vec::new();
    if let Some(report) = turn_sequence(stores) {
        out.push(report);
    }
    if let Some(home) = &stores.data_home {
        if let Some(report) = ladder_sequence(home, now) {
            out.push(report);
        }
        if let Some(report) = ingest_sequence(home) {
            out.push(report);
        }
    }
    out
}

// ===========================================================================
// Turn leg (KTD7 — one-shot; resume = the recorded next `turn` invocation)
// ===========================================================================

/// The turn leg: in flight when the run registry carries `.tmp-` aborted-run
/// residue or the stage log recorded a stage. The stage is the last recorded
/// stage-log line; the resume command is the recorded next `turn governed`
/// invocation carrying the trials ledger's last recorded candidate (turns are
/// one-shot — a governed run re-runs from the top, KTD7). Residue is reported,
/// never deleted.
pub fn turn_sequence(stores: &SequenceStores) -> Option<SequenceReport> {
    let aborted: Vec<String> =
        stores.data_home.as_deref().map(aborted_runs).unwrap_or_default();
    let last_stage = stores.stage_log.as_deref().and_then(last_stage_line);

    if aborted.is_empty() && last_stage.is_none() {
        return None; // no recorded turn state anywhere — not in flight
    }

    let stage = match &last_stage {
        Some(s) => format!("last recorded stage: {s} (stage log)"),
        None => "aborted mid-run before finalize (no stage log recorded)".to_string(),
    };

    let mut detail: Vec<String> = aborted
        .iter()
        .map(|id| format!("aborted run residue (report-only, never deleted): runs/.tmp-{id}"))
        .collect();

    // The trials ledger names the candidate the recorded next invocation runs.
    let mut candidate: Option<String> = None;
    if let Some(path) = &stores.trials_ledger {
        match TrialsLedger::new(path).read_all() {
            Ok(records) => {
                if let Some(last) = records.last() {
                    candidate = Some(last.candidate.clone());
                    detail.push(format!(
                        "last trials-ledger look: candidate '{}', verdict '{}'",
                        last.candidate, last.verdict
                    ));
                }
            }
            Err(e) => detail.push(format!("trials ledger unreadable (report-only): {e}")),
        }
    }

    let resume = match candidate {
        Some(slug) => format!(
            "LS_TURN_CANDIDATE={slug} lab-research turn governed  # one-shot: re-runs from the top (KTD7)"
        ),
        None => "lab-research turn governed (set LS_TURN_CANDIDATE=<slug>; one-shot — re-runs from the top, KTD7)"
            .to_string(),
    };

    Some(SequenceReport { kind: SequenceKind::Turn, stage, resume, detail })
}

/// The last non-empty line of the stage log, if the file exists and has one.
fn last_stage_line(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string())
}

// ===========================================================================
// Ladder leg (the chain machinery's verdict, verbatim)
// ===========================================================================

/// The ladder / session-prep leg, surfaced exactly as [`DispatchChain::load`]
/// reports it. `None` when no chain file exists (pre-genesis — nothing in
/// flight) or the prep sequence completed (green dispatch consumed by a
/// finalized run) — EXCEPT with the kill switch engaged, which is always a
/// report (an ABNORMAL-finalized session leaves the switch engaged and every
/// dispatch refused until it is deliberately cleared). A defective chain is
/// the fail-closed rung-0 verdict as a REPORT, never an error and never
/// re-derived here.
pub fn ladder_sequence(data_home: &Path, now: DateTime<Utc>) -> Option<SequenceReport> {
    // Existence probe BEFORE `open`: `DispatchChain::open` mkdirs the dispatch
    // dir, and this reader must not create anything.
    if !data_home.join("dispatch").join("chain.jsonl").exists() {
        return None;
    }
    let chain = DispatchChain::open(data_home).ok()?; // dir exists — no mkdir happens
    let state = chain.load();

    let mut detail = Vec::new();
    if state.kill_switch_engaged {
        detail.push(
            "kill switch ENGAGED — clear via lab-live --clear-killswitch (LS_DISPATCH_REASON required)"
                .to_string(),
        );
    }

    match &state.status {
        ChainStatus::NoChain => None,
        ChainStatus::Defective(why) => Some(SequenceReport {
            kind: SequenceKind::Ladder,
            // The chain machinery's own verdict: defect => authorized rung 0.
            stage: format!("fail-closed rung 0 — chain defective: {why}"),
            resume: "repair by epoch rollover: lab-live --reregister (LS_DISPATCH_RUNG + LS_DISPATCH_REASON; attended — archives the defective epoch, never rewrites it)"
                .to_string(),
            detail,
        }),
        ChainStatus::Valid => {
            detail.push(format!("chain authorizes rung {}", state.authorized_rung));
            // An ENGAGED kill switch must never vanish behind "nothing in
            // flight": an ABNORMAL-finalized session (exit 72) leaves a
            // finalized runs/<id> dir WITH the switch engaged, and the
            // non-deferrable kill-switch check reds every dispatch until it is
            // deliberately cleared. Every arm below that would otherwise
            // report no sequence falls back to this report instead.
            let engaged_fallback = |detail: Vec<String>| {
                if state.kill_switch_engaged {
                    Some(SequenceReport {
                        kind: SequenceKind::Ladder,
                        stage: "kill switch ENGAGED — session dispatches refused until cleared"
                            .to_string(),
                        resume: "clear deliberately after understanding the trip: lab-live --clear-killswitch (nonce-gated, attended; LS_DISPATCH_REASON required)"
                            .to_string(),
                        detail,
                    })
                } else {
                    None
                }
            };
            match state.mount_authz(&kst_trading_date(now)) {
                MountAuthz::Ready { record_id, chain_rung, effective_rung } => {
                    Some(SequenceReport {
                        kind: SequenceKind::Ladder,
                        stage: format!(
                            "session-dispatch {record_id} green and unconsumed (chain rung {chain_rung}, effective rung {effective_rung})"
                        ),
                        resume: "mount the attended session: lab-live --mount (single-use — consumes this dispatch; RUNBOOK-rung1.md)"
                            .to_string(),
                        detail,
                    })
                }
                MountAuthz::Consumed => {
                    let Some(last) = state.last_session_dispatch.as_ref() else {
                        return engaged_fallback(detail);
                    };
                    let run = last.consumed_run_id.as_deref();
                    // A finalized run under the registry means the mounted
                    // session finished — the prep sequence is complete (but an
                    // engaged switch is still a reportable state).
                    if let Some(run_id) = run {
                        if data_home.join("runs").join(run_id).is_dir() {
                            return engaged_fallback(detail);
                        }
                        if data_home.join("runs").join(format!(".tmp-{run_id}")).exists() {
                            detail.push(format!(
                                "aborted run residue (report-only, never deleted): runs/.tmp-{run_id}"
                            ));
                        }
                    }
                    Some(SequenceReport {
                        kind: SequenceKind::Ladder,
                        stage: format!(
                            "session-dispatch {} consumed by run {} — no finalized run recorded (session aborted or still mounted)",
                            last.record_id,
                            run.unwrap_or("<unrecorded>")
                        ),
                        resume: "inspect the mounted session outcome, then lab-live --rung-report; a new session needs a fresh lab-live --dispatch (a green dispatch is single-use)"
                            .to_string(),
                        detail,
                    })
                }
                MountAuthz::Expired => {
                    let Some(last) = state.last_session_dispatch.as_ref() else {
                        return engaged_fallback(detail);
                    };
                    Some(SequenceReport {
                        kind: SequenceKind::Ladder,
                        stage: format!(
                            "session-dispatch {} green but expired (dispatched {}, never consumed — same-day single-use)",
                            last.record_id, last.kst_trading_date
                        ),
                        resume: "re-dispatch today: lab-live --dispatch (RUNBOOK-rung1.md)".to_string(),
                        detail,
                    })
                }
                // No green dispatch in flight; a valid chain at rest is not a
                // resumable sequence (the queue owns "start a session" items)
                // — unless the kill switch is engaged, which IS one.
                MountAuthz::None => engaged_fallback(detail),
            }
        }
    }
}

// ===========================================================================
// Ingest leg (checkpoint watermark / refusal state)
// ===========================================================================

/// The ingest leg: the accumulate-forward checkpoint at
/// `<data_home>/catalog/ingest-checkpoint.json`. `None` when no checkpoint
/// exists (never ingested). A present checkpoint is always reportable resume
/// state — the watermarks ARE the resume point. Read generically (serde_json
/// value), never through the migrating `Checkpoint::load`, so this stays a
/// pure report; an unreadable checkpoint is a report row, not an error.
pub fn ingest_sequence(data_home: &Path) -> Option<SequenceReport> {
    let path = data_home.join("catalog").join("ingest-checkpoint.json");
    if !path.exists() {
        return None;
    }
    let parsed: Result<serde_json::Value, String> = std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));

    let value = match parsed {
        Ok(v) => v,
        Err(e) => {
            return Some(SequenceReport {
                kind: SequenceKind::Ingest,
                stage: format!("ingest checkpoint unreadable: {e}"),
                resume: format!(
                    "inspect {} (report-only — never hand-repair; re-run ls-ingest only once the checkpoint reads)",
                    path.display()
                ),
                detail: Vec::new(),
            });
        }
    };

    // Watermarks: the per-(instrument, bar type) coverage frontier.
    let watermarks: Vec<(String, String)> = value
        .get("watermarks")
        .and_then(|w| w.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|d| (k.clone(), d.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let stage = if watermarks.is_empty() {
        "checkpoint present, no coverage watermarks recorded yet".to_string()
    } else {
        let min = watermarks.iter().map(|(_, d)| d.as_str()).min().unwrap_or("");
        let max = watermarks.iter().map(|(_, d)| d.as_str()).max().unwrap_or("");
        format!(
            "{} series watermarked; coverage frontier {min}..{max} (last closed session covered)",
            watermarks.len()
        )
    };

    // Basis-shift marks: the append-refusal state — a marked triple must heal
    // (wipe → re-pull → re-verify) before any append; the mark outranks the
    // watermark as authority.
    let mut detail: Vec<String> = value
        .get("shifted")
        .and_then(|s| s.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    format!(
                        "basis-shift mark pending heal (appends refused): {k} (detected {})",
                        v.as_str().unwrap_or("?")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    detail.sort();

    Some(SequenceReport {
        kind: SequenceKind::Ingest,
        stage,
        resume: "resume accumulate: ls-ingest (reads this checkpoint; watermarks are the skip authority — verify by watermark, never exit code; RUNBOOK-session-morning.md Step 4)"
            .to_string(),
        detail,
    })
}
