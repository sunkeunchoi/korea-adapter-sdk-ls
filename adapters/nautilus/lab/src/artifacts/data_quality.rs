//! The data-quality report (KTD2/KTD4, R7, R14, AE3, AE4) — coverage gaps,
//! shallow-history symbols, the detected per-symbol adjustment-basis shifts,
//! approximated-fill counts, reconcile-advised conditions (live), and the resolved
//! universe snapshot.
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

/// One stratum's composition in a metadata-driven run (plan 2026-07-10-003,
/// U6/R7): how many selected symbols and joined trades the tier carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierCompositionEntry {
    /// The stratum label (`Stratum::label`).
    pub stratum: String,
    /// Selected symbols attributed to the tier (the union across sessions).
    pub symbols: u64,
    /// Joined trades attributed to the tier.
    pub trades: u64,
}

/// The data-quality report artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataQualityReport {
    /// Recorded coverage gaps.
    pub coverage_gaps: Vec<CoverageGapRecord>,
    /// Symbols whose minute history is shallower than the requested range (AE4).
    pub shallow_history_symbols: Vec<String>,
    /// Symbols in the run's selected universe with a DETECTED, unhealed
    /// adjustment-basis shift (the ingest checkpoint's shifted marks intersected
    /// with the run's selection). A clean catalog reports none — the agent
    /// discounts only runs whose universe intersects this list, never blanket.
    /// Replaces the old catalog-wide `adjustment_basis_splice` bool.
    pub adjustment_basis_shift_symbols: Vec<String>,
    /// The number of fills emitted at an approximated price (KTD4/R14): limit-price
    /// fallbacks plus beyond-first poll partials. The agent never reads these as exact.
    pub price_approximated_fills: u64,
    /// Reconcile-advised conditions observed during a live session (empty for a
    /// backtest).
    pub reconcile_advised: Vec<ReconcileCondition>,
    /// The fail-closed teardown's cancel-retry count (R5): more than one retry is a
    /// limit event (R14(d)). `None` for a backtest or a pre-U5 artifact — absent, not
    /// zero (a real zero-retry live teardown records `Some(0)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teardown_retries: Option<u64>,
    /// The order-dedup hit count over the session (R5): a non-zero count on a real
    /// emission is a limit event (R14(d)). `None` for a backtest or a pre-U5 artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedup_hits: Option<u64>,
    /// Whether the driver had to HARD-STOP the node — it did not return from `run` within
    /// the stop grace and was abandoned. A TYPED carrier, not just an observation line,
    /// because the scans that used to catch this failure read structure, never free text:
    /// before the hard-stop existed, a wedged node ended in `.tmp-` residue, which
    /// `scan_limit_events` and `readiness_verdict` both treat as a safety signal. Finalizing
    /// the run removes that residue, so without this field an abandoned-node session would
    /// score as one of the trailing-K CLEAN sessions and could help promote the rung.
    /// `None` for a backtest or a pre-hard-stop artifact — absent, not `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hard_stopped: Option<bool>,
    /// Whether this run was a paper rehearsal (U8, KTD2) — mirrored from the run's
    /// manifest so a reader holding only the data-quality report can tell, with the same
    /// tri-state reading: `None` predates the label and is **not** a rehearsal. The
    /// manifest remains the authority; this is the copy the artifact scans read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rehearsal: Option<bool>,
    /// Whether this run carried the post-judgment paper-stage label (U8, KTD2). Same
    /// tri-state reading as [`Self::rehearsal`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper_stage: Option<bool>,
    /// The resolved universe symbol list used (its hash rides on the manifest; the
    /// composition lives here so the agent can compare runs, R7/KTD8).
    pub universe_snapshot: Vec<String>,
    /// Per-tier symbol + trade counts for a metadata-driven run (plan
    /// 2026-07-10-003, U6): typed alongside the flat `universe_snapshot`.
    /// `None` for a legacy run; absent from prior artifacts (`serde(default)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_composition: Option<Vec<TierCompositionEntry>>,
    /// Held symbols that received no bar on a rehearsal session (U9, KTD12). A TYPED row
    /// rather than an observation line, because the backtest's policy for the same
    /// condition is to ABORT (`HeldSymbolMissingBar`) while live keeps the holding — so the
    /// comparison report has to be able to count the divergence, not grep for it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held_symbol_gaps: Vec<HeldSymbolGap>,
    /// Measured backtest-vs-live divergences from a rehearsal session (U9, KTD4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rehearsal_divergences: Vec<RehearsalDivergence>,
    /// Free-form observations (scrubbed at write time — the one free-text carrier).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<String>,
}

/// One held symbol that produced no usable bar this session (U9, KTD12).
///
/// The holding is KEPT: a trading halt is not evidence the position should be exited, and
/// the stop/expiry judgment is deferred to the next valid bar. Recording it typed is what
/// lets the comparison report separate "the live run held through a halt" from "the live
/// run and the backtest disagreed about a price".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldSymbolGap {
    /// The instrument id the gap is for.
    pub instrument_id: String,
    /// The KST session date (`YYYY-MM-DD`).
    pub session_date: String,
    /// Why no bar was usable (an empty t8407 row, an unparseable price, …).
    pub reason: String,
}

/// What kind of backtest-vs-live divergence a rehearsal row records (U9, KTD4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RehearsalDivergenceKind {
    /// The decision was taken on the 15:20 bar but filled at the 15:30 closing price —
    /// one of the two divergences KTD4 bounds the mechanism to, and it is measured on
    /// EVERY order, not only the ones that moved.
    DecisionVsClose,
    /// An entry order that did not fill in the closing auction. The backtest's frozen
    /// mechanism always fills at the close, so an unfilled entry is a live-only outcome —
    /// and the leg that does open carries the FILLED quantity, not the intended one.
    ///
    /// The residue those orders leave is deliberately not a fourth variant: the fail-closed
    /// teardown cancels it and records the fact in `teardown_retries` + the run's ABNORMAL
    /// verdict, so a row here would restate a typed field that already exists.
    UnfilledEntry,
}

/// One measured divergence between the frozen mechanism and what the rehearsal did (U9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RehearsalDivergence {
    /// Which divergence class this row is.
    pub kind: RehearsalDivergenceKind,
    /// The instrument id it concerns.
    pub instrument_id: String,
    /// The 15:20 decision price in integer KRW, when the row has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_price: Option<i64>,
    /// The realized/closing price in integer KRW, when the row has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_price: Option<i64>,
    /// The human-readable specifics (scrubbed at write time with the rest of the report).
    pub detail: String,
}

impl DataQualityReport {
    /// A backtest data-quality report: no live-only fields.
    pub fn backtest(
        universe_snapshot: Vec<String>,
        adjustment_basis_shift_symbols: Vec<String>,
    ) -> Self {
        DataQualityReport {
            coverage_gaps: Vec::new(),
            shallow_history_symbols: Vec::new(),
            adjustment_basis_shift_symbols,
            price_approximated_fills: 0,
            reconcile_advised: Vec::new(),
            teardown_retries: None,
            dedup_hits: None,
            hard_stopped: None,
            rehearsal: None,
            paper_stage: None,
            universe_snapshot,
            tier_composition: None,
            held_symbol_gaps: Vec::new(),
            rehearsal_divergences: Vec::new(),
            observations: Vec::new(),
        }
    }

    /// Stamp the run's KTD2 labels onto the report, mirroring its manifest. Called on the
    /// live path only: a backtest leaves both absent.
    pub fn with_run_labels(mut self, rehearsal: Option<bool>, paper_stage: Option<bool>) -> Self {
        self.rehearsal = rehearsal;
        self.paper_stage = paper_stage;
        self
    }
}
