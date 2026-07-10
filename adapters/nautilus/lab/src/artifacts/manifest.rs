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
    /// A hash of the resolved universe selection. For a multi-session backtest the
    /// runner populates it with [`universe_sequence_hash`] — a **sequence-sensitive**
    /// digest over the chronological per-session `(date, symbols-in-rank-order)` tuples
    /// (not the order-insensitive flat [`universe_hash`]). It is the comparability key
    /// for the run's selection, capturing per-session and out-of-range prior-daily
    /// influence that the range-scoped `catalog_fingerprint` does not; `runs compare`
    /// keys on both. The composition itself lives in the data-quality report; the
    /// manifest carries only the hash (KTD8/KTD-5).
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
    /// The `UniverseMetadata` artifact's content hash for a metadata-driven run
    /// (plan 2026-07-10-003, KTD2). The per-tier report asserts it matches the
    /// ingest pin's hash — a re-capture between ingest and backtest would
    /// silently re-tier symbols and corrupt the per-tier counts. `None` for a
    /// legacy (metadata-less) run; absent from prior manifests, hence the
    /// `serde(default)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub universe_metadata_hash: Option<String>,
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

/// Hash a sorted list of universe symbols into a stable hex digest. Order-insensitive
/// (sorts first) — the single-selection universe fingerprint used before the runner
/// became multi-session. Retained for callers that hash one flat symbol set.
pub fn universe_hash(symbols: &[String]) -> String {
    let mut sorted = symbols.to_vec();
    sorted.sort();
    hash_lines(&sorted)
}

/// Hash the per-session universe *selection sequence* into a stable hex digest
/// (KTD-5): the chronologically-ordered `(session_date, symbols-in-rank-order)`
/// tuples, hashed **without sorting** so the fingerprint is sensitive to both the
/// session order and each session's rank order — unlike [`universe_hash`], which
/// sorts and destroys order. Two multi-session runs with the identical per-session
/// selection sequence produce the identical hash; a run whose selection sequence
/// differs (a different day's symbols, a different rank order, a different set of
/// tradeable sessions) produces a different hash.
pub fn universe_sequence_hash(sessions: &[(chrono::NaiveDate, Vec<String>)]) -> String {
    let lines: Vec<String> = sessions
        .iter()
        .map(|(date, symbols)| format!("{date}|{}", symbols.join(",")))
        .collect();
    // Deliberately NOT sorted — the sequence is the fingerprint.
    hash_lines(&lines)
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
