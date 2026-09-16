//! The resume probe (U6, R14; KTD5) — `lab-next probe`.
//!
//! One question, asked of all four R10 sequences at once: **is in-flight work
//! resumable from what is on disk right now?** Each leg answers with three
//! checks — the store reads, a stage derives from it, and a resume command
//! prints — and the run writes a summary JSON beside the queue file.
//!
//! The verdict rule is what earns this its own module, because it is the
//! opposite of the entry report's. The report is allowed to *describe* a
//! damaged store as a row an operator can act on; the probe is not:
//!
//! - **Absent or unreadable is a FAIL that names what is missing.** Never a
//!   pass, never silence. [`probe_ingest`] re-parses the ingest checkpoint for
//!   exactly this reason — [`ingest_sequence`] reports an unreadable checkpoint
//!   as a row, which is right for the report and wrong here.
//! - **"Nothing in flight" is a PASS, not a gap**, as long as the store that
//!   would have held it is readable: `no in-flight turn` is a derivable stage.
//!   A probe that failed on an idle repo would be a liveness check, not a
//!   resumability one.
//! - **A failing leg is a verdict, not an error.** The probe prints, records,
//!   and exits 1; it must not crash on a fresh environment, so every leg
//!   returns a [`ProbeOutcome`] rather than propagating.
//!
//! The report write is this subcommand's ONLY mutation, and it uses the queue's
//! tmp+rename idiom with a PID-suffixed staging file so two concurrent probes
//! cannot clobber each other.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::dispatch::chain::{ChainStatus, DispatchChain};
use crate::queue::sequences::{ingest_sequence, ladder_sequence, turn_sequence, SequenceStores};
use crate::trials::TrialsLedger;

use super::{parse_gate_status, report_now, resolve_gate_status, USAGE};

/// Probe-report path override (tests point this at a tempdir so a probe run
/// never writes into the real repo). Unset → the tracked-location default,
/// `<repo root>/queue/probe-report.json` (next to the queue file, KTD2/KTD5).
pub const PROBE_REPORT_PATH_ENV: &str = "LS_PROBE_REPORT_PATH";

/// The tracked probe-report file, relative to the repo root.
pub const PROBE_REPORT_RELPATH: &str = "queue/probe-report.json";

/// One probed sequence: either all three checks held (store readable, stage
/// derivable, resume printable), or the store was absent/unreadable and the
/// failure names exactly what is missing.
enum ProbeOutcome {
    /// The store read; the derived stage and resume command are noted.
    Ok { stage: String, resume: String },
    /// The store is absent or unreadable — resumability is not demonstrated.
    Fail { missing: String },
}

fn probe_fail(missing: impl Into<String>) -> ProbeOutcome {
    ProbeOutcome::Fail { missing: missing.into() }
}

/// `probe` (U6): probe the four R10 sequences against the current
/// environment's real stores, print one verdict line each plus the summary
/// line, and write the summary JSON atomically. Exit 0 = all pass, 1 otherwise
/// (a failing leg is a verdict, never an error — the probe itself must not
/// crash on a fresh environment).
pub(super) fn run_probe(rest: &[String]) -> anyhow::Result<ExitCode> {
    if !rest.is_empty() {
        anyhow::bail!("probe takes no arguments, got {rest:?}\n{USAGE}");
    }
    let now = report_now();
    let stores = SequenceStores::from_env();
    let probed: [(&str, ProbeOutcome); 4] = [
        ("turn", probe_turn(&stores)),
        ("ladder", probe_ladder(&stores, now)),
        ("ingest", probe_ingest(&stores)),
        ("gate-run", probe_gate()),
    ];

    let mut all_pass = true;
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for (sequence, outcome) in &probed {
        match outcome {
            ProbeOutcome::Ok { stage, resume } => {
                println!("ok[{sequence}] stage: {stage}; resume: {resume}");
                rows.push(serde_json::json!({
                    "sequence": sequence,
                    "verdict": "ok",
                    "stage": stage,
                    "resume": resume,
                }));
            }
            ProbeOutcome::Fail { missing } => {
                all_pass = false;
                println!("FAIL[{sequence}] missing: {missing}");
                rows.push(serde_json::json!({
                    "sequence": sequence,
                    "verdict": "fail",
                    "missing": missing,
                }));
            }
        }
    }

    let report = serde_json::json!({
        "version": 1,
        "probed_utc": now.to_rfc3339_opts(SecondsFormat::Secs, true),
        "sequences": rows,
        "all_pass": all_pass,
    });
    let path = probe_report_path()?;
    write_probe_report(&path, &report)?;
    println!("probe: {} — report {}", if all_pass { "PASS" } else { "FAIL" }, path.display());
    Ok(if all_pass { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// The probe-report path: [`PROBE_REPORT_PATH_ENV`] overrides; otherwise the
/// tracked repo-root location next to the queue file.
fn probe_report_path() -> anyhow::Result<PathBuf> {
    match std::env::var(PROBE_REPORT_PATH_ENV).ok().filter(|s| !s.trim().is_empty()) {
        Some(p) => Ok(PathBuf::from(p)),
        None => Ok(crate::queue::repo_root()?.join(PROBE_REPORT_RELPATH)),
    }
}

/// Persist the probe report atomically: sibling tmp file, then rename over the
/// target (the queue's tmp+rename idiom) — the probe's ONLY write.
fn write_probe_report(path: &Path, report: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("mkdir {}: {e}", parent.display()))?;
    }
    // PID-suffixed tmp (the gate-run.sh `tmp-$$` idiom): two concurrent
    // writers must never clobber each other's staging file.
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, format!("{}\n", serde_json::to_string_pretty(report)?))
        .map_err(|e| anyhow::anyhow!("write probe report tmp {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| anyhow::anyhow!("commit probe report {}: {e}", path.display()))
}

/// The turn leg (KTD7). An in-flight turn (aborted residue or a stage log) is
/// readable state via [`turn_sequence`]. With no in-flight turn, readability
/// is demonstrated by the trials ledger: readable → ok ("no in-flight turn" IS
/// a derivable stage, with the recorded next one-shot invocation as the resume
/// command); absent or unreadable → FAIL naming it.
fn probe_turn(stores: &SequenceStores) -> ProbeOutcome {
    if let Some(report) = turn_sequence(stores) {
        return ProbeOutcome::Ok { stage: report.stage, resume: report.resume };
    }
    let Some(path) = stores.trials_ledger.as_deref() else {
        return probe_fail(
            "no trials ledger configured, and no stage log or aborted-run residue anywhere",
        );
    };
    if !path.exists() {
        return probe_fail(format!(
            "trials ledger {} absent (and no stage log or aborted-run residue)",
            path.display()
        ));
    }
    match TrialsLedger::new(path).read_all() {
        Ok(records) => match records.last() {
            Some(last) => ProbeOutcome::Ok {
                stage: format!(
                    "no in-flight turn — trials ledger readable, last look candidate '{}' verdict '{}'",
                    last.candidate, last.verdict
                ),
                resume: format!(
                    "LS_TURN_CANDIDATE={} lab-research turn governed  # one-shot: re-runs from the top (KTD7)",
                    last.candidate
                ),
            },
            None => ProbeOutcome::Ok {
                stage: "no in-flight turn — trials ledger readable (no looks recorded)".to_string(),
                resume: "lab-research turn governed (set LS_TURN_CANDIDATE=<slug>; one-shot — re-runs from the top, KTD7)"
                    .to_string(),
            },
        },
        Err(e) => {
            probe_fail(format!("trials ledger {} present but unreadable: {e}", path.display()))
        }
    }
}

/// The ladder leg. The chain file must exist and read; whatever the chain
/// machinery then says — an in-flight prep, the rung-0 fail-closed verdict on
/// a defective chain, or a valid chain at rest — is readable state (ok).
fn probe_ladder(stores: &SequenceStores, now: DateTime<Utc>) -> ProbeOutcome {
    let Some(home) = stores.data_home.as_deref() else {
        return probe_fail("LS_DATA_HOME not configured — dispatch/chain.jsonl unreachable");
    };
    let chain_path = home.join("dispatch").join("chain.jsonl");
    if !chain_path.exists() {
        return probe_fail(format!("dispatch chain {} absent", chain_path.display()));
    }
    // In-flight (including the defective fail-closed verdict): readable state.
    if let Some(report) = ladder_sequence(home, now) {
        return ProbeOutcome::Ok { stage: report.stage, resume: report.resume };
    }
    // Chain present but no prep in flight (at rest, or the last prep
    // completed): still readable state — derive the stage from the chain
    // itself. `open` cannot mkdir here: the chain file's dir exists.
    match DispatchChain::open(home) {
        Ok(chain) => {
            let state = chain.load();
            match state.status {
                ChainStatus::Defective(why) => ProbeOutcome::Ok {
                    // Unreachable via ladder_sequence in practice; kept so a
                    // defect can never read as "at rest".
                    stage: format!("fail-closed rung 0 — chain defective: {why}"),
                    resume: "repair by epoch rollover: lab-live --reregister (attended)".to_string(),
                },
                _ => ProbeOutcome::Ok {
                    stage: format!(
                        "chain readable — authorizes rung {}; no session prep in flight",
                        state.authorized_rung
                    ),
                    resume: "start a new prep: lab-live --dispatch (RUNBOOK-rung1.md)".to_string(),
                },
            }
        }
        Err(e) => probe_fail(format!(
            "dispatch chain {} present but unreadable: {e}",
            chain_path.display()
        )),
    }
}

/// The ingest leg. The checkpoint must exist AND parse — [`ingest_sequence`]
/// reports an unreadable checkpoint as a row (correct for the entry report),
/// but for the probe present-but-unreadable is a FAILURE, so parse here first.
fn probe_ingest(stores: &SequenceStores) -> ProbeOutcome {
    let Some(home) = stores.data_home.as_deref() else {
        return probe_fail(
            "LS_DATA_HOME not configured — catalog/ingest-checkpoint.json unreachable",
        );
    };
    let path = home.join("catalog").join("ingest-checkpoint.json");
    if !path.exists() {
        return probe_fail(format!("ingest checkpoint {} absent", path.display()));
    }
    let parsed = std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string()));
    if let Err(e) = parsed {
        return probe_fail(format!(
            "ingest checkpoint {} present but unreadable: {e}",
            path.display()
        ));
    }
    match ingest_sequence(home) {
        Some(report) => ProbeOutcome::Ok { stage: report.stage, resume: report.resume },
        // Unreachable (the file exists), but never let it read as a pass.
        None => probe_fail(format!("ingest checkpoint {} vanished mid-probe", path.display())),
    }
}

/// The gate leg, via the same mechanism as the report
/// ([`super::GATE_STATUS_FILE_ENV`] override, else the real script when
/// `.gate-run/state.json` exists). Absent or unreadable state → FAIL naming it;
/// readable `--status` output → ok, whether in flight, green (`next=none`), or
/// recorded-but-nothing-done.
fn probe_gate() -> ProbeOutcome {
    let text = match resolve_gate_status() {
        Ok(t) => t,
        Err(missing) => return probe_fail(missing),
    };
    if let Some(report) = parse_gate_status(&text) {
        return ProbeOutcome::Ok { stage: report.stage, resume: report.resume };
    }
    // Readable but not in-flight: gate green, or recorded with nothing done.
    match text.lines().find_map(|l| l.strip_prefix("next=")).map(str::trim) {
        Some("none") => ProbeOutcome::Ok {
            stage: "gate state readable — all steps done (gate green)".to_string(),
            resume: "make gate-run (a fresh run re-verifies against the current tree)".to_string(),
        },
        Some(step) => ProbeOutcome::Ok {
            stage: format!(
                "gate state readable — no steps done (fresh or fully invalidated); next step {step}"
            ),
            resume: format!("make gate-run (runs from {step})"),
        },
        None => probe_fail("gate --status output unparseable (no next= line)"),
    }
}
