//! `turn diagnose` — the Phase-A wrapper + gate-verdict (U5, R2/R3; AE3).
//!
//! Runs a candidate's diagnostic and its independent twin, compares their
//! canonical readings **reading-by-reading within the pre-registered per-reading
//! tolerance** (raw-stdout byte comparison is deliberately not the gate — two
//! independently-authored twins never produce byte-identical output), evaluates
//! the agreed readings against the frozen thresholds, and records the verdict.
//!
//! **Ledger-first (KTD2):** the gate-reading trial record is appended BEFORE the
//! `gate-verdict.json` artifact is written, so a crash between the two leaves an
//! orphan trial record (the overcount-safe direction), never an uncounted GO.
//!
//! This module also owns the **typed gate-exit registry** ([`GateExit`]) that U6
//! and U7 route their refusals through: every governed halt maps to a stable,
//! distinct exit code so a caller distinguishes WHICH gate stopped the turn from
//! the exit alone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::{Deserialize, Serialize};

use crate::candidates::{load, LoadedCandidate, PhaseAClass, ScriptDecl, GATE_VERDICT_FILE};
use crate::trials::{LookKind, SampleLineage, TrialRecord, TrialsLedger};

/// The gate-verdict schema version.
pub const GATE_VERDICT_SCHEMA_VERSION: u32 = 1;

// ===========================================================================
// The typed gate-exit registry (U5). Append-only: new gates get new codes;
// existing codes NEVER move, so the skill and downstream tooling key on them.
// ===========================================================================

/// A stable, distinct process exit code per governed halt (U5/KTD6). `Ok` is a
/// completed evaluation (KEEP/REVERT); `Generic` is a bare untyped error; every
/// other variant names a specific gate. Documented beside the verdict grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateExit {
    /// A completed evaluation reached a verdict (KEEP/REVERT) — success.
    Ok = 0,
    /// A generic/untyped failure (a bare `anyhow` bail with no gate class).
    Generic = 1,
    /// Diagnose STOP: the twin's readings disagree beyond the per-reading tolerance.
    TwinMismatch = 10,
    /// Diagnose STOP: an agreed reading failed a frozen threshold.
    ThresholdFail = 11,
    /// A diagnostic/twin script failed to run, or emitted malformed / incomplete readings.
    ScriptFailure = 12,
    /// Flip guard: the gate verdict is absent, or records STOP (U6).
    NoGoVerdict = 20,
    /// Flip guard: the pre-register content hash changed after the verdict (U6, AE1).
    PreRegisterHashMismatch = 21,
    /// Flip guard: the flip param/value (or sweep leg) does not match the candidate (U6).
    FlipMismatch = 22,
    /// Flip guard: catalog-fingerprint drift between the verdict and the anchor run (U6).
    FingerprintDrift = 23,
    /// Flip guard: a GO verdict with no matching gate-reading ledger record (U6).
    MissingLedgerRecord = 24,
    /// Flip guard: an override param was set without naming a candidate (U6).
    UngovernedFlip = 25,
    /// Build/fingerprint: a binary's embedded fingerprint mismatches the recomputed tree (U7, AE2).
    StaleBinary = 30,
    /// Build failure (U7).
    BuildFailure = 31,
    /// Freeze: a frozen input is git-dirty (U4/U5).
    FrozenInputDirty = 40,
}

impl GateExit {
    /// The numeric exit code.
    pub fn code(self) -> u8 {
        self as u8
    }

    /// The process exit code.
    pub fn exit_code(self) -> ExitCode {
        ExitCode::from(self as u8)
    }

    /// Whether this is a completed (non-halted) evaluation.
    pub fn is_ok(self) -> bool {
        matches!(self, GateExit::Ok)
    }

    /// The full registry (append-only). Used by the numeric/name reverse maps.
    fn all() -> [GateExit; 14] {
        [
            GateExit::Ok,
            GateExit::Generic,
            GateExit::TwinMismatch,
            GateExit::ThresholdFail,
            GateExit::ScriptFailure,
            GateExit::NoGoVerdict,
            GateExit::PreRegisterHashMismatch,
            GateExit::FlipMismatch,
            GateExit::FingerprintDrift,
            GateExit::MissingLedgerRecord,
            GateExit::UngovernedFlip,
            GateExit::StaleBinary,
            GateExit::BuildFailure,
            GateExit::FrozenInputDirty,
        ]
    }

    /// Reverse-map a numeric code to its gate (U7: the parent maps the child's
    /// exit code back to a typed gate).
    pub fn from_code(code: u8) -> Option<GateExit> {
        Self::all().into_iter().find(|g| g.code() == code)
    }

    /// Reverse-map a diagnose STOP gate name (as written in the gate verdict) to
    /// its gate (U7: the parent short-circuits a reused STOP verdict).
    pub fn from_name(name: &str) -> Option<GateExit> {
        match name {
            "twin-mismatch" => Some(GateExit::TwinMismatch),
            "threshold-fail" => Some(GateExit::ThresholdFail),
            "script-failure" => Some(GateExit::ScriptFailure),
            _ => None,
        }
    }
}

// ===========================================================================
// Gate verdict artifact
// ===========================================================================

/// The gate-verdict artifact written into a candidate dir (R2). It records the
/// GO/STOP decision, both reading sets, the pre-register content hash it was
/// evaluated against, the anchor run's catalog fingerprint, the freeze commit,
/// the flip target (so U6 can match it), and — for R5 disclosure — the ledger's
/// prior trials for the same lever family and sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateVerdict {
    /// Schema version.
    pub schema_version: u32,
    /// The candidate slug.
    pub slug: String,
    /// The lever family.
    pub family: String,
    /// `"GO"` or `"STOP"`.
    pub decision: String,
    /// The gate that stopped the turn (a [`GateExit`] name), when STOP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_gate: Option<String>,
    /// The human-facing stop reason (the failing reading / threshold), when STOP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// The diagnostic's readings.
    pub diagnostic_readings: BTreeMap<String, f64>,
    /// The twin's readings.
    pub twin_readings: BTreeMap<String, f64>,
    /// The agreed readings the thresholds were evaluated against.
    pub agreed_readings: BTreeMap<String, f64>,
    /// The pre-register content hash this verdict was evaluated against (U6 matches it).
    pub pre_register_hash: String,
    /// The anchor run's catalog fingerprint (U6 checks it against the flip anchor).
    pub catalog_fingerprint: String,
    /// The freeze commit R2 records (git evidence the freeze predates the reading).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze_commit: Option<String>,
    /// The single flip param, when a single-flip candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_param: Option<String>,
    /// The single flip value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_value: Option<f64>,
    /// The sweep param, when a sweep candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep_param: Option<String>,
    /// The enumerated sweep legs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sweep_legs: Vec<f64>,
    /// R5 disclosure: the ledger's prior trials for the same family + sample, so a
    /// post-STOP re-registration is a disclosed, reviewable event.
    #[serde(default)]
    pub prior_trials: Vec<TrialRecord>,
    /// When this verdict was written.
    pub recorded_utc: String,
}

// ===========================================================================
// Config + outcome
// ===========================================================================

/// `turn diagnose` config. The freeze commit is pre-computed by the caller (the
/// CLI runs [`crate::candidates::freeze_check`] and bails on a dirty input; tests
/// inject a commit), keeping this function pure of git.
#[derive(Debug, Clone)]
pub struct DiagnoseConfig {
    /// The candidate directory.
    pub candidate_dir: PathBuf,
    /// The trials ledger.
    pub ledger: TrialsLedger,
    /// The anchor run's catalog fingerprint (the sample the look ran against).
    pub anchor_fingerprint: String,
    /// The lineage parent fingerprint, when declared.
    pub parent_fingerprint: Option<String>,
    /// The freeze commit (from `freeze_check`; injected in tests).
    pub freeze_commit: Option<String>,
    /// The record timestamp.
    pub recorded_utc: String,
}

/// A `turn diagnose` outcome.
#[derive(Debug, Clone)]
pub struct DiagnoseOutcome {
    /// True only for a GO.
    pub go: bool,
    /// The typed gate exit.
    pub exit: GateExit,
    /// The gate-verdict artifact path, when one was written (GO or STOP; not a
    /// script failure).
    pub gate_verdict_path: Option<PathBuf>,
    /// The report lines (last line is the verdict).
    pub lines: Vec<String>,
}

// ===========================================================================
// diagnose
// ===========================================================================

/// Run the diagnose stage for a candidate (R2, R3; AE3). Loads + validates the
/// candidate, runs the diagnostic + twin, compares their readings within
/// tolerance, evaluates thresholds, appends the gate-reading trial (ledger-first),
/// and writes the gate-verdict artifact.
///
/// # Errors
///
/// On a candidate that cannot be loaded/validated, or an I/O failure creating the
/// scratch output dir. Script failures and gate STOPs are *outcomes* (typed exit),
/// not errors.
pub fn diagnose(cfg: &DiagnoseConfig) -> anyhow::Result<DiagnoseOutcome> {
    let loaded = load(&cfg.candidate_dir)?;
    let candidate = &loaded.values;

    // Minimal-class candidates (independent-signal levers) declare a
    // freshness/reconcile-only Phase-A — no bespoke diagnostic to run. Record a
    // GO on that basis (R4).
    if candidate.phase_a == PhaseAClass::Minimal {
        return finish(
            cfg,
            &loaded,
            GateOutcome::Go,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            Some("minimal Phase-A (freshness/reconcile-only)".to_string()),
        );
    }

    // Bespoke: run both scripts into scratch readings files.
    let scratch = tempfile::TempDir::new()?;
    let diag_decl = candidate.diagnostic.as_ref().expect("bespoke validated has a diagnostic");
    let twin_decl = candidate.twin.as_ref().expect("bespoke validated has a twin");

    let diag_readings = match run_script(&cfg.candidate_dir, diag_decl, &scratch.path().join("diagnostic.json")) {
        Ok(m) => m,
        Err(e) => return Ok(script_failure("diagnostic", &e.to_string())),
    };
    let twin_readings = match run_script(&cfg.candidate_dir, twin_decl, &scratch.path().join("twin.json")) {
        Ok(m) => m,
        Err(e) => return Ok(script_failure("twin", &e.to_string())),
    };

    // Every declared reading key must be present in BOTH artifacts.
    for key in candidate.readings.keys() {
        for (label, map) in [("diagnostic", &diag_readings), ("twin", &twin_readings)] {
            if !map.contains_key(key) {
                return Ok(script_failure(
                    label,
                    &format!("readings omit the declared key '{key}'"),
                ));
            }
        }
    }

    // Twin comparison: reading-by-reading within the per-reading tolerance.
    for (key, spec) in &candidate.readings {
        let d = diag_readings[key];
        let t = twin_readings[key];
        if (d - t).abs() > spec.tolerance {
            let reason = format!(
                "reading '{key}' disagrees: diagnostic {d} vs twin {t} (tolerance {})",
                spec.tolerance
            );
            return finish(
                cfg,
                &loaded,
                GateOutcome::Stop { gate: GateExit::TwinMismatch, name: "twin-mismatch" },
                diag_readings.clone(),
                twin_readings.clone(),
                diag_readings.clone(),
                Some(reason),
            );
        }
    }

    // Agreed readings = the diagnostic's (they agree within tolerance). Evaluate
    // thresholds.
    let agreed = diag_readings.clone();
    for t in &candidate.thresholds {
        let reading = agreed[&t.reading];
        if !t.comparator.passes(reading, t.value) {
            let reason = format!(
                "threshold on '{}' failed: {reading} not {:?} {}",
                t.reading, t.comparator, t.value
            );
            return finish(
                cfg,
                &loaded,
                GateOutcome::Stop { gate: GateExit::ThresholdFail, name: "threshold-fail" },
                diag_readings.clone(),
                twin_readings.clone(),
                agreed.clone(),
                Some(reason),
            );
        }
    }

    finish(cfg, &loaded, GateOutcome::Go, diag_readings, twin_readings, agreed, None)
}

/// The decided gate outcome before it is recorded.
enum GateOutcome {
    Go,
    Stop { gate: GateExit, name: &'static str },
}

/// Append the gate-reading trial (ledger-first), write the gate-verdict artifact,
/// and build the outcome.
fn finish(
    cfg: &DiagnoseConfig,
    loaded: &LoadedCandidate,
    outcome: GateOutcome,
    diagnostic_readings: BTreeMap<String, f64>,
    twin_readings: BTreeMap<String, f64>,
    agreed_readings: BTreeMap<String, f64>,
    reason: Option<String>,
) -> anyhow::Result<DiagnoseOutcome> {
    let candidate = &loaded.values;
    let (decision, stop_gate, exit, go) = match &outcome {
        GateOutcome::Go => ("GO".to_string(), None, GateExit::Ok, true),
        GateOutcome::Stop { gate, name } => {
            ("STOP".to_string(), Some((*gate, *name)), *gate, false)
        }
    };
    let verdict_str = match &stop_gate {
        None => "GO".to_string(),
        Some((_, name)) => format!("STOP {name}"),
    };

    // R5 disclosure: read the ledger's prior trials for this family + sample
    // BEFORE appending the current one, so the embedded list is truly "prior".
    let prior_trials = prior_trials_for(&cfg.ledger, &candidate.family, &cfg.anchor_fingerprint)?;

    // Ledger-first: append the gate-reading trial, then write the artifact.
    let lineage = SampleLineage {
        catalog_fingerprint: cfg.anchor_fingerprint.clone(),
        parent_fingerprint: cfg.parent_fingerprint.clone(),
    };
    let trial = TrialRecord::new(
        candidate.slug.clone(),
        candidate.family.clone(),
        LookKind::GateReading,
        lineage,
        agreed_readings.clone(),
        verdict_str.clone(),
        cfg.recorded_utc.clone(),
    );
    cfg.ledger.append(&trial)?;

    let verdict = GateVerdict {
        schema_version: GATE_VERDICT_SCHEMA_VERSION,
        slug: candidate.slug.clone(),
        family: candidate.family.clone(),
        decision: decision.clone(),
        stop_gate: stop_gate.map(|(_, name)| name.to_string()),
        stop_reason: reason.clone(),
        diagnostic_readings,
        twin_readings,
        agreed_readings,
        pre_register_hash: loaded.content_hash.clone(),
        catalog_fingerprint: cfg.anchor_fingerprint.clone(),
        freeze_commit: cfg.freeze_commit.clone(),
        flip_param: candidate.flip_param.clone(),
        flip_value: candidate.flip_value,
        sweep_param: candidate.sweep_param.clone(),
        sweep_legs: candidate.sweep_legs.clone(),
        prior_trials,
        recorded_utc: cfg.recorded_utc.clone(),
    };
    let path = loaded.dir.join(GATE_VERDICT_FILE);
    write_gate_verdict(&path, &verdict)?;

    let mut lines = vec![
        format!("candidate {} (family {})", candidate.slug, candidate.family),
        format!("pre-register hash {}", loaded.content_hash),
    ];
    if let Some(commit) = &cfg.freeze_commit {
        lines.push(format!("freeze commit {commit}"));
    }
    if let Some(r) = &reason {
        lines.push(r.clone());
    }
    lines.push(format!("wrote {}", path.display()));
    lines.push(verdict_str);

    Ok(DiagnoseOutcome { go, exit, gate_verdict_path: Some(path), lines })
}

/// Build a script-failure outcome: a typed failure naming the stage, no verdict
/// artifact, no trial appended (nothing was measured).
fn script_failure(stage: &str, detail: &str) -> DiagnoseOutcome {
    DiagnoseOutcome {
        go: false,
        exit: GateExit::ScriptFailure,
        gate_verdict_path: None,
        lines: vec![
            format!("{stage} script failure: {detail}"),
            "STOP script-failure".to_string(),
        ],
    }
}

/// Run one declared script from the candidate dir, appending the readings output
/// path as the final argv entry, and parse the readings it writes.
fn run_script(dir: &Path, decl: &ScriptDecl, out_path: &Path) -> anyhow::Result<BTreeMap<String, f64>> {
    if decl.argv.is_empty() {
        anyhow::bail!("empty argv");
    }
    let status = Command::new(&decl.argv[0])
        .args(&decl.argv[1..])
        .arg(out_path)
        .current_dir(dir)
        .status()
        .map_err(|e| anyhow::anyhow!("spawning {:?}: {e}", decl.argv))?;
    if !status.success() {
        anyhow::bail!("command {:?} exited {}", decl.argv, status);
    }
    let text = std::fs::read_to_string(out_path)
        .map_err(|e| anyhow::anyhow!("reading readings from {:?}: {e}", decl.argv))?;
    serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("command {:?} wrote malformed readings JSON: {e}", decl.argv))
}

/// Serialize the gate verdict (pretty, so it is diff-readable in the tracked
/// dir). The one free-text carrier — `stop_reason` — routes through the scrub
/// before hitting disk (uniform scrub discipline); the structured hash/reading
/// fields render verbatim.
fn write_gate_verdict(path: &Path, verdict: &GateVerdict) -> anyhow::Result<()> {
    let mut scrubbed = verdict.clone();
    if let Some(reason) = &scrubbed.stop_reason {
        scrubbed.stop_reason = Some(nautilus_ls::scrub::scrub_secrets(reason));
    }
    let json = serde_json::to_string_pretty(&scrubbed)?;
    std::fs::write(path, json)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
    Ok(())
}

/// Read a gate verdict from a candidate dir (U6 consumes it). `None` if absent.
///
/// # Errors
///
/// On a malformed verdict or an unsupported schema version.
pub fn read_gate_verdict(candidate_dir: &Path) -> anyhow::Result<Option<GateVerdict>> {
    let path = candidate_dir.join(GATE_VERDICT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    let verdict: GateVerdict = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
    if verdict.schema_version != GATE_VERDICT_SCHEMA_VERSION {
        anyhow::bail!(
            "{}: unsupported gate-verdict schema version {}",
            path.display(),
            verdict.schema_version
        );
    }
    Ok(Some(verdict))
}

/// The ledger's prior trials for a lever family + sample (R5 disclosure). Matches
/// on family and on the sample fingerprint (as the trial's fingerprint or its
/// declared parent — a re-registration against an evolved-but-linked sample still
/// discloses).
fn prior_trials_for(
    ledger: &TrialsLedger,
    family: &str,
    fingerprint: &str,
) -> anyhow::Result<Vec<TrialRecord>> {
    Ok(ledger
        .read_all()?
        .into_iter()
        .filter(|r| {
            r.family == family
                && (r.lineage.catalog_fingerprint == fingerprint
                    || r.lineage.parent_fingerprint.as_deref() == Some(fingerprint))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_exit_codes_are_distinct_and_stable() {
        let all = [
            GateExit::Ok,
            GateExit::Generic,
            GateExit::TwinMismatch,
            GateExit::ThresholdFail,
            GateExit::ScriptFailure,
            GateExit::NoGoVerdict,
            GateExit::PreRegisterHashMismatch,
            GateExit::FlipMismatch,
            GateExit::FingerprintDrift,
            GateExit::MissingLedgerRecord,
            GateExit::UngovernedFlip,
            GateExit::StaleBinary,
            GateExit::BuildFailure,
            GateExit::FrozenInputDirty,
        ];
        let mut codes: Vec<u8> = all.iter().map(|g| g.code()).collect();
        let n = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), n, "every gate exit code is distinct");
        // A couple of pinned values (stability — these must never move).
        assert_eq!(GateExit::Ok.code(), 0);
        assert_eq!(GateExit::TwinMismatch.code(), 10);
        assert_eq!(GateExit::PreRegisterHashMismatch.code(), 21);
        assert!(GateExit::Ok.is_ok() && !GateExit::TwinMismatch.is_ok());
    }
}
