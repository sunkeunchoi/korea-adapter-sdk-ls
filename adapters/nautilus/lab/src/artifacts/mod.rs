//! Run artifacts + the append-only run registry (U4, KTD2, R4–R9).
//!
//! Every run — backtest or live — emits the same four artifacts (`manifest.json`,
//! `performance.json`, `decisions.jsonl`, `data_quality.json`) into a per-run directory
//! under `<data>/runs/<run_id>/`. A run writes into `<data>/runs/.tmp-<run_id>/` and
//! finalizes by atomic rename (mirroring the ingest checkpoint's atomic-save pattern);
//! a leftover `.tmp-` directory marks an aborted run and is reported, never silently
//! reused. Runs are never overwritten (append-only). The agent later writes
//! `analysis.md` into a finalized run dir (R15, co-location).

pub mod data_quality;
pub mod manifest;
pub mod observation;
pub mod performance;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::envelope::{to_scrubbed_jsonl_line, DecisionEnvelope};
use data_quality::DataQualityReport;
use manifest::Manifest;
use observation::RunObservation;
use performance::PerformanceReport;

/// Whether a run was a backtest or a live paper session (R9). Recorded in the run id
/// and the manifest so live sessions are just another run the agent can analyze.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunSource {
    Backtest,
    Live,
}

impl RunSource {
    /// The stable string form used in the run id.
    pub fn as_str(&self) -> &'static str {
        match self {
            RunSource::Backtest => "backtest",
            RunSource::Live => "live",
        }
    }
}

/// The canonical run id: `<UTC start stamp>-<source>-<strategy_id>-v<version>` (KTD2).
pub fn run_id(start: DateTime<Utc>, source: RunSource, strategy_id: &str, version: u32) -> String {
    format!(
        "{}-{}-{}-v{}",
        start.format("%Y%m%dT%H%M%SZ"),
        source.as_str(),
        strategy_id,
        version
    )
}

/// The four artifact file names every run emits.
pub const MANIFEST_FILE: &str = "manifest.json";
pub const PERFORMANCE_FILE: &str = "performance.json";
/// The per-run decision-envelope stream (one telemetry envelope per decision, R6).
pub const DECISIONS_FILE: &str = "decisions.jsonl";
pub const DATA_QUALITY_FILE: &str = "data_quality.json";
/// The typed run observation (P7/U6) — the **fifth**, conditional artifact, written only
/// by the daily path and only when the run carries a `return_on_risk` (R25). An ORB run
/// never writes it.
pub const OBSERVATION_FILE: &str = "observation.json";
/// The agent-written analysis file (co-located into a finalized run dir, R15).
pub const ANALYSIS_FILE: &str = "analysis.md";

/// Scrub a free-text string of account/secret-like tokens (KTD2), delegating to the
/// adapter's scrub. Applied to free-text fields at write time so aborted `.tmp-`
/// directories are clean too.
pub fn scrub(s: &str) -> String {
    nautilus_ls::scrub::scrub_secrets(s)
}

/// The append-only run writer. Owns a `.tmp-<run_id>` staging directory; artifacts are
/// written into it and the run finalizes by atomic rename to `<run_id>`.
#[derive(Debug)]
pub struct RunWriter {
    run_id: String,
    tmp_dir: PathBuf,
    final_dir: PathBuf,
    finalized: bool,
}

impl RunWriter {
    /// Open a writer for `run_id` under `<data>/runs/`. Creates the staging directory
    /// and refuses to reuse an existing one (an aborted run of the same id) or to
    /// overwrite a finalized run (append-only, R9).
    ///
    /// # Errors
    ///
    /// Errors if the runs directory cannot be created, the finalized run already
    /// exists, or a staging directory for this run id already exists.
    pub fn new(data_home: &Path, run_id: &str) -> anyhow::Result<Self> {
        let runs_dir = data_home.join("runs");
        std::fs::create_dir_all(&runs_dir)?;
        // Surface any leftover aborted runs (a `.tmp-` staging dir from a crashed run)
        // so an operator/agent notices them — reported here, on writer construction.
        for aborted in aborted_runs(data_home) {
            tracing::warn!(run_id = %aborted, "found an aborted run staging directory (.tmp-); a prior run did not finalize");
        }
        let final_dir = runs_dir.join(run_id);
        if final_dir.exists() {
            anyhow::bail!("run {run_id} already exists — the registry is append-only");
        }
        let tmp_dir = runs_dir.join(format!(".tmp-{run_id}"));
        if tmp_dir.exists() {
            anyhow::bail!(
                "a staging directory for run {run_id} already exists ({}) — an aborted run is never silently reused",
                tmp_dir.display()
            );
        }
        std::fs::create_dir_all(&tmp_dir)?;
        Ok(RunWriter {
            run_id: run_id.to_string(),
            tmp_dir,
            final_dir,
            finalized: false,
        })
    }

    /// The run id this writer stages.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Write the run manifest.
    pub fn write_manifest(&self, manifest: &Manifest) -> anyhow::Result<()> {
        self.write_json(MANIFEST_FILE, manifest)
    }

    /// Write the performance report.
    pub fn write_performance(&self, report: &PerformanceReport) -> anyhow::Result<()> {
        self.write_json(PERFORMANCE_FILE, report)
    }

    /// Write the typed run observation (P7/U6). Only the daily path calls this, and only
    /// for a run whose statistic exists — [`RunObservation::build`] is what refuses the
    /// other case, before there is anything to write.
    pub fn write_observation(&self, observation: &RunObservation) -> anyhow::Result<()> {
        self.write_json(OBSERVATION_FILE, observation)
    }

    /// Write the data-quality report, scrubbing its free-text observations first.
    pub fn write_data_quality(&self, report: &DataQualityReport) -> anyhow::Result<()> {
        let mut scrubbed = report.clone();
        scrubbed.observations = scrubbed.observations.iter().map(|s| scrub(s)).collect();
        self.write_json(DATA_QUALITY_FILE, &scrubbed)
    }

    /// Write the per-decision envelope stream as JSONL (`decisions.jsonl`),
    /// scrubbing each envelope's free-text fields at write time via the shared
    /// [`to_scrubbed_jsonl_line`] seam (R9 — the same free-text-only
    /// discipline as the cross-run recorder, so UUIDs stay intact and every
    /// line parses back). The telemetry free text here (filter names, trigger
    /// descriptions) is compile-time literals from our own code, scrubbed
    /// anyway for consistency.
    pub fn write_decisions(&self, envelopes: &[DecisionEnvelope]) -> anyhow::Result<()> {
        let mut text = String::new();
        for e in envelopes {
            text.push_str(&to_scrubbed_jsonl_line(e)?);
        }
        std::fs::write(self.tmp_dir.join(DECISIONS_FILE), text)?;
        Ok(())
    }

    /// Finalize the run by atomically renaming the staging directory to `<run_id>`.
    /// After this the run is immutable (append-only). Consumes the writer.
    ///
    /// # Errors
    ///
    /// Errors if the finalized run already exists (a race) or the rename fails.
    pub fn finalize(mut self) -> anyhow::Result<PathBuf> {
        if self.final_dir.exists() {
            anyhow::bail!("run {} already exists — the registry is append-only", self.run_id);
        }
        std::fs::rename(&self.tmp_dir, &self.final_dir)?;
        self.finalized = true;
        Ok(self.final_dir.clone())
    }

    fn write_json<T: Serialize>(&self, name: &str, value: &T) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(value)?;
        std::fs::write(self.tmp_dir.join(name), json)?;
        Ok(())
    }
}

impl Drop for RunWriter {
    fn drop(&mut self) {
        // A writer dropped without `finalize` (a crash/abort mid-run) intentionally
        // leaves the `.tmp-` staging directory behind as the aborted-run marker; the
        // next writer construction reports it via `aborted_runs`.
        if !self.finalized {
            tracing::debug!(run_id = %self.run_id, "run writer dropped un-finalized; staging dir left as aborted-run marker");
        }
    }
}

/// Report the aborted staging directories under `<data>/runs/` (leftover `.tmp-*`),
/// as their bare run ids (KTD2: leftover tmp = aborted run, reported on next writer
/// construction).
pub fn aborted_runs(data_home: &Path) -> Vec<String> {
    let runs_dir = data_home.join("runs");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&runs_dir) {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if let Some(id) = name.strip_prefix(".tmp-") {
                    out.push(id.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// List the finalized run ids under `<data>/runs/` (append-only registry contents),
/// sorted. Staging directories (`.tmp-*`) are excluded.
pub fn list_runs(data_home: &Path) -> Vec<String> {
    let runs_dir = data_home.join("runs");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&runs_dir) {
        for e in entries.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            if let Some(name) = e.file_name().to_str() {
                if !name.starts_with(".tmp-") {
                    out.push(name.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// Whether a finalized run directory carries an agent-written `analysis.md` (R15
/// co-location convention).
pub fn run_has_analysis(data_home: &Path, run_id: &str) -> bool {
    data_home.join("runs").join(run_id).join(ANALYSIS_FILE).exists()
}
