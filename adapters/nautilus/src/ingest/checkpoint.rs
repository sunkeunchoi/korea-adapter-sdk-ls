//! Resumable ingest checkpoints (R5, AE2).
//!
//! A JSON state file beside the catalog records the completed
//! `(instrument, bar type, date range)` triples plus coverage gaps (empty history,
//! `01715`, paper-thin feeds), so an interrupted or repeated run **skips and
//! reports** rather than refetching. The checkpoint is written after each triple
//! completes, so a crash loses at most the in-flight triple.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AdapterError, AdapterResult};

/// Why an `(instrument, bar type, range)` triple produced no bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    /// The gateway returned no rows for the range (short/empty history).
    EmptyHistory,
    /// The gateway returned `01715` (a non-trading-day / bad-date range).
    NonTradingDay,
    /// A paper-thin feed (rows present but below a usable threshold).
    PaperThin,
}

/// A recorded coverage gap: a triple that yielded no usable bars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageGap {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`, `1-MINUTE`).
    pub bar_type: String,
    /// The requested date range (`sdate..edate`).
    pub range: String,
    /// Why the gap was recorded.
    pub reason: GapReason,
}

/// The persisted ingest state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Completed `(instrument, bar type, range)` keys (see [`Self::key`]).
    completed: BTreeSet<String>,
    /// Recorded coverage gaps.
    gaps: Vec<CoverageGap>,
    /// Whether daily bars were ingested with adjusted prices (`sujung="Y"`,
    /// KTD5). Recorded as catalog metadata so downstream knows the price basis.
    pub adjusted_prices: bool,
}

impl Checkpoint {
    /// The canonical key for a triple.
    pub fn key(instrument: &str, bar_type: &str, range: &str) -> String {
        format!("{instrument}|{bar_type}|{range}")
    }

    /// Load a checkpoint from `path`, returning an empty checkpoint if the file
    /// does not exist.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Ingest`] if the file exists but cannot be read/parsed.
    pub fn load(path: &Path) -> AdapterResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s)
                .map_err(|e| AdapterError::Ingest(format!("corrupt checkpoint {}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Checkpoint::default()),
            Err(e) => Err(AdapterError::Ingest(format!(
                "cannot read checkpoint {}: {e}",
                path.display()
            ))),
        }
    }

    /// Persist the checkpoint to `path` (pretty JSON).
    ///
    /// # Errors
    ///
    /// [`AdapterError::Ingest`] on a write/serialize failure.
    pub fn save(&self, path: &Path) -> AdapterResult<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| AdapterError::Ingest(format!("serialize checkpoint: {e}")))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AdapterError::Ingest(format!("mkdir {}: {e}", parent.display())))?;
        }
        std::fs::write(path, json)
            .map_err(|e| AdapterError::Ingest(format!("write checkpoint {}: {e}", path.display())))
    }

    /// Whether a triple is already done.
    pub fn is_done(&self, instrument: &str, bar_type: &str, range: &str) -> bool {
        self.completed.contains(&Self::key(instrument, bar_type, range))
    }

    /// Mark a triple done.
    pub fn mark_done(&mut self, instrument: &str, bar_type: &str, range: &str) {
        self.completed.insert(Self::key(instrument, bar_type, range));
    }

    /// Record a coverage gap and mark the triple done (so a re-run skips it rather
    /// than refetching a known-empty feed).
    pub fn record_gap(&mut self, instrument: &str, bar_type: &str, range: &str, reason: GapReason) {
        self.mark_done(instrument, bar_type, range);
        self.gaps.push(CoverageGap {
            instrument: instrument.to_string(),
            bar_type: bar_type.to_string(),
            range: range.to_string(),
            reason,
        });
    }

    /// The recorded coverage gaps.
    pub fn gaps(&self) -> &[CoverageGap] {
        &self.gaps
    }

    /// The number of completed triples.
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempdir().unwrap();
        let cp = Checkpoint::load(&dir.path().join("nope.json")).unwrap();
        assert_eq!(cp.completed_count(), 0);
    }

    #[test]
    fn round_trips_completed_and_gaps() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut cp = Checkpoint::default();
        cp.adjusted_prices = true;
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20241231");
        cp.record_gap("000660.XKRX", "1-MINUTE", "20240101..20240105", GapReason::EmptyHistory);
        cp.save(&path).unwrap();

        let loaded = Checkpoint::load(&path).unwrap();
        assert!(loaded.is_done("005930.XKRX", "1-DAY", "20240101..20241231"));
        assert!(loaded.is_done("000660.XKRX", "1-MINUTE", "20240101..20240105")); // gap marks done
        assert_eq!(loaded.gaps().len(), 1);
        assert_eq!(loaded.gaps()[0].reason, GapReason::EmptyHistory);
        assert!(loaded.adjusted_prices);
    }
}
