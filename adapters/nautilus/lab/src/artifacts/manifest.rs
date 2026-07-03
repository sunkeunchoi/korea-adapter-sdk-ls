//! The run manifest (KTD8, R8) — pins strategy id/version, the full parameter set,
//! the explicit bar data range, a range-scoped catalog fingerprint, and the universe
//! snapshot hash, so any two runs are comparable and any run is reproducible.

use nautilus_model::data::Bar;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::artifacts::RunSource;
use crate::params::OrbParams;

/// The explicit bar-data range a run pinned (inclusive `YYYYMMDD` trading days). A
/// comparison re-run pins the same range so the range-scoped fingerprint is stable
/// across accumulate days (KTD8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataRange {
    /// Range start, `YYYYMMDD`.
    pub start: String,
    /// Range end, `YYYYMMDD`.
    pub end: String,
}

/// The run manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// The run id (`<UTC stamp>-<source>-<strategy_id>-v<version>`).
    pub run_id: String,
    /// Whether this run was a backtest or a live paper session.
    pub source: RunSource,
    /// Strategy identifier.
    pub strategy_id: String,
    /// Strategy version.
    pub strategy_version: u32,
    /// The full ORB parameter set (every value the strategy used, R3/R8).
    pub params: OrbParams,
    /// The explicit bar-data range.
    pub data_range: DataRange,
    /// A fingerprint over the catalog bar content that intersects the pinned range
    /// (KTD8): identical in-range data yields an identical fingerprint across
    /// accumulate days; a changed fingerprint means real in-range drift.
    pub catalog_fingerprint: String,
    /// A hash of the resolved universe symbol list (the composition lives in the
    /// data-quality report; the manifest carries only its hash, KTD8).
    pub universe_hash: String,
    /// A hash of the strategy's source code. `strategy_version` is operator-set and
    /// can drift from the actual logic; this fingerprint changes whenever the strategy
    /// code changes, so two runs claiming the same version but differing in code are
    /// still distinguishable from the manifests alone (AE1).
    pub strategy_code_hash: String,
    /// The ingest checkpoint's content hash, ridden along as a secondary informational
    /// field (a whole-checkpoint hash differs on every accumulate day, so it is NOT
    /// the comparability authority — the range-scoped fingerprint is, KTD8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_hash: Option<String>,
    /// The instant the run started (UTC, RFC3339-like stamp).
    pub created_utc: String,
}

/// Hash the catalog bar content that intersects `[start_ns, end_ns]` (inclusive) into
/// a stable hex fingerprint (KTD8). Bars outside the range are excluded, so an
/// accumulate-forward ingest that only appends later bars does not change the
/// fingerprint; a change means real in-range drift.
pub fn range_fingerprint(bars: &[Bar], start_ns: u64, end_ns: u64) -> String {
    // Collect an ordered, canonical line per in-range bar so the hash is independent
    // of catalog read order.
    let mut lines: Vec<String> = bars
        .iter()
        .filter(|b| {
            let ts = b.ts_event.as_u64();
            ts >= start_ns && ts <= end_ns
        })
        .map(|b| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}",
                b.bar_type,
                b.ts_event.as_u64(),
                b.open,
                b.high,
                b.low,
                b.close,
                b.volume
            )
        })
        .collect();
    lines.sort();
    hash_lines(&lines)
}

/// The strategy-source fingerprint recorded in the manifest (hash of the embedded
/// ORB source). Changes whenever the strategy logic changes.
pub fn strategy_code_hash() -> String {
    hash_bytes(crate::strategy::ORB_SOURCE.as_bytes())
}

/// Hash a sorted list of universe symbols into a stable hex digest.
pub fn universe_hash(symbols: &[String]) -> String {
    let mut sorted = symbols.to_vec();
    sorted.sort();
    hash_lines(&sorted)
}

/// SHA-256 of the newline-joined lines, hex-encoded.
fn hash_lines(lines: &[String]) -> String {
    let mut hasher = Sha256::new();
    for l in lines {
        hasher.update(l.as_bytes());
        hasher.update(b"\n");
    }
    hex(&hasher.finalize())
}

/// SHA-256 of arbitrary bytes (used for the checkpoint content hash), hex-encoded.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
