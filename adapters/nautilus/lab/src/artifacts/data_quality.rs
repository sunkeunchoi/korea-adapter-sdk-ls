//! The data-quality report (KTD2/KTD4, R7, R14, AE3, AE4) — coverage gaps,
//! shallow-history symbols, the adjustment-basis splice flag, approximated-fill
//! counts, reconcile-advised conditions (live), and the resolved universe snapshot.
//! Every field is typed (enums + counts); the one free-text carrier (`observations`)
//! is scrubbed at write time (KTD2).

use serde::{Deserialize, Serialize};

/// Why a coverage gap exists (mirrors the ingest checkpoint's typed reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReasonKind {
    /// The feed returned no data for the range (empty / unsupported on paper).
    EmptyFeed,
    /// The range was only partially served.
    Partial,
    /// A candidate's prior-session daily bar was absent at universe-scan time.
    MissingPriorDaily,
}

/// A recorded coverage gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageGapRecord {
    /// Instrument the gap concerns (`{shcode}.XKRX`).
    pub instrument: String,
    /// Bar series label (`1-DAY`, `1-MINUTE`), empty for a universe-scan gap.
    pub bar_type: String,
    /// The range the gap covers (`YYYYMMDD..YYYYMMDD`), empty when not range-scoped.
    pub range: String,
    /// Why the gap exists.
    pub reason: GapReasonKind,
}

/// A reconcile-advised condition observed during a live session (R7, AE3). The
/// poll lane's inconclusive-pass reasons, surfaced as typed conditions so the agent
/// can treat the run's accounting as suspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileConditionKind {
    /// A t0425 poll page was truncated (a non-empty continuation cursor).
    PollTruncated,
    /// A poll row's OrdNo could not be resolved or adopted.
    UnresolvedRow,
    /// A poll cumulative regressed below the OrdNo watermark.
    CumulativeRegression,
    /// A poll request failed outright.
    PollFailed,
    /// A poll pass was inconclusive for an unspecified reason — the poll lane
    /// collapses its specific causes (truncation / unresolved row / regression /
    /// failure) into a single `reconcile_needed` flag, so a lab observer that sees
    /// only the flag records this rather than guessing a specific cause.
    PollInconclusive,
}

/// One reconcile-advised observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileCondition {
    /// The kind of inconclusive condition.
    pub kind: ReconcileConditionKind,
    /// The symbol the poll pass concerned (`{shcode}`), when known.
    pub symbol: String,
}

/// The data-quality report artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataQualityReport {
    /// Recorded coverage gaps.
    pub coverage_gaps: Vec<CoverageGapRecord>,
    /// Symbols whose minute history is shallower than the requested range (AE4).
    pub shallow_history_symbols: Vec<String>,
    /// Whether the daily catalog used adjusted prices — a documented adjustment-basis
    /// splice limitation the agent should discount (flag-only this increment).
    pub adjustment_basis_splice: bool,
    /// The number of fills emitted at an approximated price (KTD4/R14): limit-price
    /// fallbacks plus beyond-first poll partials. The agent never reads these as exact.
    pub price_approximated_fills: u64,
    /// Reconcile-advised conditions observed during a live session (empty for a
    /// backtest).
    pub reconcile_advised: Vec<ReconcileCondition>,
    /// The resolved universe symbol list used (its hash rides on the manifest; the
    /// composition lives here so the agent can compare runs, R7/KTD8).
    pub universe_snapshot: Vec<String>,
    /// Free-form observations (scrubbed at write time — the one free-text carrier).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<String>,
}

impl DataQualityReport {
    /// A backtest data-quality report: no live-only fields.
    pub fn backtest(universe_snapshot: Vec<String>, adjustment_basis_splice: bool) -> Self {
        DataQualityReport {
            coverage_gaps: Vec::new(),
            shallow_history_symbols: Vec::new(),
            adjustment_basis_splice,
            price_approximated_fills: 0,
            reconcile_advised: Vec::new(),
            universe_snapshot,
            observations: Vec::new(),
        }
    }
}
