//! The run manifest (KTD8, R8) — pins strategy id/version, the full parameter set,
//! the explicit bar data range, a range-scoped catalog fingerprint, and the universe
//! snapshot hash, so any two runs are comparable and any run is reproducible.

use chrono::{DateTime, Utc};
use nautilus_model::data::Bar;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::artifacts::{run_id, RunSource};
use crate::params::OrbParams;
use crate::params_daily::{DailyParams, DAILY_STRATEGY_ID};

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
    /// The declared build-input fingerprint embedded by `build.rs` and recorded
    /// here so a run is attributable to the certified root SDK/core boundary.
    /// The historical field name remains stable. `None` on a run whose binary
    /// predates the fingerprint; absent from prior manifests, hence the
    /// `serde(default)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lab_src_fingerprint: Option<String>,
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
    /// The dispatch↔run linkage (KTD3): binds this run to the dispatch that authorized
    /// it, plus the rung metadata the reducers key on. `None` for a backtest/research
    /// run or a pre-U5 artifact; absent from prior manifests, hence the `serde(default)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<DispatchLink>,
    /// The daily-resolution multi-session-hold path's parameter set (P7/U3, KTD4).
    /// `Some` on a daily run, `None` on every ORB run — including every manifest
    /// already committed — so an ORB manifest serializes byte-identically to its
    /// pre-P7 form and `governed_params_hash`, which hashes the untouched
    /// [`Self::params`], is undisturbed. Retyping `params` into a strategy enum was
    /// rejected for exactly that reason.
    ///
    /// The pairing is enforced, not conventional:
    /// [`Manifest::validate_strategy_identity`] refuses an ORB manifest that carries
    /// daily params (a silent "ignore the extra field" would let an ORB-identified run
    /// claim daily terms) and refuses a daily-identified manifest that carries none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_params: Option<DailyParams>,
    /// The instant the run started (UTC, RFC3339-like stamp).
    pub created_utc: String,
}

/// The inputs [`Manifest::new_daily`] cannot derive for itself. A struct rather than a
/// dozen positional arguments, and it deliberately has **no** `strategy_id`,
/// `strategy_code_hash`, `run_id` or `source` field: those are the four values a daily
/// run must not be able to get wrong (KTD14), so the constructor derives every one of
/// them.
#[derive(Debug, Clone)]
pub struct DailyManifestParts<'a> {
    /// The daily parameter set. Validated by the constructor; the source of the
    /// manifest's `strategy_id`, `strategy_version`, and run id.
    pub daily: DailyParams,
    /// The `OrbParams` the *shared* candidate assembly ran with (`atr_window` and
    /// friends). Recorded verbatim for reproducibility; its `strategy_id` is
    /// deliberately **ignored** — that field defaults to `"orb"` and reading it here is
    /// precisely how a daily run would come to look like an ORB run.
    pub assembly_params: OrbParams,
    /// The daily strategy's embedded source, hashed into
    /// [`Manifest::strategy_code_hash`] via [`daily_strategy_code_hash`]. U4 passes its
    /// `include_str!` const.
    pub daily_source: &'a str,
    /// When the run started; the run id's stamp and `created_utc` both come from it.
    pub started_utc: DateTime<Utc>,
    /// The explicit pinned bar-data range.
    pub data_range: DataRange,
    /// The range-scoped catalog fingerprint.
    pub catalog_fingerprint: String,
    /// The per-session selection-sequence hash ([`universe_sequence_hash`]).
    pub universe_hash: String,
    /// The running binary's embedded declared build-input fingerprint.
    pub lab_src_fingerprint: Option<String>,
    /// The ingest checkpoint's content hash.
    pub checkpoint_hash: Option<String>,
    /// The universe-metadata artifact hash for a metadata-driven run.
    pub universe_metadata_hash: Option<String>,
}

impl Manifest {
    /// Construct a **daily-path** run manifest (P7/U3) — the daily path's run-construction
    /// point, and the call site of [`DailyParams::validate`].
    ///
    /// Everything identity-bearing is derived here rather than accepted:
    ///
    /// - `strategy_id` and `strategy_version` come from `parts.daily`, **never** from
    ///   `parts.assembly_params` (whose `strategy_id` defaults to `"orb"`). This is the
    ///   registry discriminator (KTD14): the ORB path derives both from its parameter
    ///   set at `runner::backtest`, so a daily runner reusing that derivation would emit
    ///   a manifest no strategy filter could tell apart from an ORB run.
    /// - `run_id` is built from the same discriminator, so the id and the field agree.
    /// - `strategy_code_hash` is [`daily_strategy_code_hash`] of the daily source — the
    ///   *sibling* of [`strategy_code_hash`], which stays untouched (KTD5).
    /// - `source` is [`RunSource::Backtest`]: this path has no live runner.
    /// - `dispatch` is `None`: a research backtest is not ladder-authorized.
    ///
    /// # Errors
    ///
    /// Returns the offending message when [`DailyParams::validate`] rejects the frozen
    /// terms, or when the assembled manifest fails
    /// [`Manifest::validate_strategy_identity`].
    pub fn new_daily(parts: DailyManifestParts<'_>) -> Result<Manifest, String> {
        // The run-construction gate: a parameter set off a frozen term never reaches the
        // engine, and never reaches the registry.
        parts.daily.validate()?;
        let manifest = Manifest {
            run_id: run_id(
                parts.started_utc,
                RunSource::Backtest,
                &parts.daily.strategy_id,
                parts.daily.strategy_version,
            ),
            source: RunSource::Backtest,
            strategy_id: parts.daily.strategy_id.clone(),
            strategy_version: parts.daily.strategy_version,
            params: parts.assembly_params,
            data_range: parts.data_range,
            catalog_fingerprint: parts.catalog_fingerprint,
            universe_hash: parts.universe_hash,
            strategy_code_hash: daily_strategy_code_hash(parts.daily_source),
            lab_src_fingerprint: parts.lab_src_fingerprint,
            checkpoint_hash: parts.checkpoint_hash,
            universe_metadata_hash: parts.universe_metadata_hash,
            dispatch: None,
            daily_params: Some(parts.daily),
            created_utc: parts.started_utc.to_rfc3339(),
        };
        manifest.validate_strategy_identity()?;
        Ok(manifest)
    }

    /// Check that this manifest's strategy identity and its carried parameter sets
    /// agree (KTD14). Enforced at construction by [`Manifest::new_daily`]; also usable
    /// on a manifest read back from the registry.
    ///
    /// The rules, all of which exist because the alternative is a *silent* misread:
    ///
    /// 1. An ORB-identified manifest carrying daily params is **refused**, not ignored.
    ///    Ignoring it would let a run whose engine, OMS, and hold semantics are ORB's
    ///    advertise the daily lineage's frozen terms.
    /// 2. A manifest carrying daily params must agree with them on `strategy_id`, and
    ///    those params must themselves validate.
    /// 3. A daily-identified manifest carrying **no** daily params is refused: the
    ///    discriminator would partition the registry while the terms it partitions on
    ///    are absent.
    ///
    /// # Errors
    ///
    /// Returns the offending message on any of the three.
    pub fn validate_strategy_identity(&self) -> Result<(), String> {
        match &self.daily_params {
            Some(daily) => {
                if self.strategy_id == crate::params::STRATEGY_ID {
                    return Err(format!(
                        "manifest {} is identified as ORB (strategy_id {:?}) but carries daily \
                         params — refused rather than ignored: an ORB run's engine, OMS, and \
                         hold semantics are not this lineage's, so the carried terms would be a \
                         false claim (KTD14)",
                        self.run_id, self.strategy_id
                    ));
                }
                daily.validate()?;
                if self.strategy_id != daily.strategy_id {
                    return Err(format!(
                        "manifest {} records strategy_id {:?} but its daily params say {:?} — \
                         the registry discriminator and the parameter set must agree, or a \
                         strategy filter and the terms it selects describe different runs",
                        self.run_id, self.strategy_id, daily.strategy_id
                    ));
                }
                Ok(())
            }
            None => {
                if self.strategy_id == DAILY_STRATEGY_ID {
                    return Err(format!(
                        "manifest {} is identified as the daily path (strategy_id {:?}) but \
                         carries no daily params — the frozen terms the discriminator exists to \
                         partition on would be absent from the run's own record (KTD4/KTD14)",
                        self.run_id, self.strategy_id
                    ));
                }
                Ok(())
            }
        }
    }
}

/// The dispatch↔run linkage recorded in a live run's manifest (KTD3). Lets a reducer
/// bind every session to its authorization and require live-lane provenance plus
/// matching identity hashes before counting a session toward a rung.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchLink {
    /// The `dispatch_id` (session-dispatch record id) that authorized this run.
    pub dispatch_id: String,
    /// The chain-authorized rung this run ran at.
    pub rung: u8,
    /// The budget-numerator fraction applied at this rung (KTD6).
    pub rung_fraction: f64,
    /// The credential lane hash (SHA-256 of the appkey, spend-ledger precedent) — never
    /// the raw key or account number.
    pub lane: String,
    /// The resolved trading environment (`"paper"` | `"live"`) — closes the gap where
    /// `RunSource::Live` means paper-live today (KTD3).
    pub trading_env: String,
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

/// The **daily** path's strategy-source fingerprint — the sibling of
/// [`strategy_code_hash`], not a generalization of it (KTD5).
///
/// [`strategy_code_hash`] hashes `ORB_SOURCE` and keeps its zero-argument signature:
/// every one of its production call sites is ORB-domain with no strategy id in scope, so
/// dispatching it on a strategy id would buy a fan of edits that all pass the literal
/// `"orb"` on the most identity-critical function in the crate. A daily run's code hash
/// is therefore *this* function's, and it can never move the ORB digest.
///
/// The source is a parameter rather than a baked const because the daily strategy
/// module lands in a later unit; U4 passes its `include_str!` const, exactly as
/// [`strategy_code_hash`] passes `ORB_SOURCE`. Callers should not pass an ad-hoc string:
/// [`Manifest::new_daily`] is the only production path, and it feeds this straight into
/// `Manifest.strategy_code_hash`.
pub fn daily_strategy_code_hash(daily_source: &str) -> String {
    hash_bytes(daily_source.as_bytes())
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
