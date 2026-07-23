//! The injectable evidence input port (U14, KTD9).
//!
//! Refresh never touches transport directly: it pulls NORMALIZED evidence through an
//! [`EvidenceInputPort`]. The offline gate feeds a [`StaticEvidencePort`] with synthetic
//! inputs; the maintainer-run live transport
//! ([`LiveEvidencePort`](crate::calendar_refresh::transport::LiveEvidencePort)) is a
//! separate impl that yields the same [`RefreshInputs`] shape. Raw KRX/KASI bodies never
//! reach the port's output — a port yields already-normalized [`EvidenceRecord`]s + typed
//! per-source fetch outcomes, so a failed source is recorded (retention) rather than
//! silently dropping accepted evidence.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use nautilus_ls_calendar::schema::{EvidenceRecord, Source, SourceKind};

/// The civil-date span a refresh recomputes. Both endpoints inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshScope {
    /// First civil date in scope.
    pub from: NaiveDate,
    /// Last civil date in scope.
    pub through: NaiveDate,
}

/// An inclusive `[from, through]` civil-date range — the unit a per-source covered claim is
/// expressed in (KTD2). Both endpoints inclusive; a single-date range has `from == through`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    /// First civil date in the range (inclusive).
    pub from: NaiveDate,
    /// Last civil date in the range (inclusive).
    pub through: NaiveDate,
}

impl DateRange {
    /// A range `[from, through]`. Callers pass a well-ordered pair (`from <= through`); a
    /// reversed pair is an empty range (`contains` is always false, `intersect` yields `None`).
    pub fn new(from: NaiveDate, through: NaiveDate) -> Self {
        DateRange { from, through }
    }

    /// A single-date range.
    pub fn single(date: NaiveDate) -> Self {
        DateRange {
            from: date,
            through: date,
        }
    }

    /// `true` iff `date` falls within `[from, through]` (inclusive).
    pub fn contains(&self, date: NaiveDate) -> bool {
        self.from <= date && date <= self.through
    }

    /// The overlap of two ranges, or `None` when they are disjoint.
    pub fn intersect(&self, other: &DateRange) -> Option<DateRange> {
        let from = self.from.max(other.from);
        let through = self.through.min(other.through);
        (from <= through).then_some(DateRange { from, through })
    }
}

/// Merge `ranges` into a canonical, ascending, non-overlapping set: overlapping OR adjacent
/// (`through` immediately precedes the next `from`) ranges coalesce; a genuine gap stays a
/// separate range. Empty (reversed) inputs are dropped. This is the union operation a
/// covered-range claim is normalized through.
pub fn merge_ranges(ranges: &[DateRange]) -> Vec<DateRange> {
    let mut sorted: Vec<DateRange> = ranges.iter().copied().filter(|r| r.from <= r.through).collect();
    sorted.sort_by(|a, b| a.from.cmp(&b.from).then(a.through.cmp(&b.through)));
    let mut merged: Vec<DateRange> = Vec::new();
    for r in sorted {
        match merged.last_mut() {
            // Coalesce when `r` overlaps or is adjacent to the running range (its start is at
            // or before the running end's next civil date).
            Some(last) if r.from <= next_day(last.through) => {
                if r.through > last.through {
                    last.through = r.through;
                }
            }
            _ => merged.push(r),
        }
    }
    merged
}

/// The sub-ranges of `window` NOT covered by `ranges` (empty iff `ranges` fully cover
/// `window`). This is both the containment test (fully covered == empty result) and the
/// carrier for a refusal that names exactly which dates a covered claim leaves uncovered
/// (KTD2 / R12 / F3). `ranges` need not be pre-merged.
pub fn uncovered_within(window: DateRange, ranges: &[DateRange]) -> Vec<DateRange> {
    if window.from > window.through {
        return Vec::new();
    }
    let merged = merge_ranges(ranges);
    let mut gaps: Vec<DateRange> = Vec::new();
    let mut cursor = window.from;
    for r in &merged {
        if r.through < cursor {
            continue; // entirely before the remaining window
        }
        if r.from > cursor {
            // A gap `[cursor, r.from - 1]` (clamped to the window end).
            let gap_end = prev_day(r.from).min(window.through);
            if cursor <= gap_end {
                gaps.push(DateRange::new(cursor, gap_end));
            }
        }
        if let Some(after) = next_day_opt(r.through) {
            cursor = cursor.max(after);
        } else {
            return gaps; // covered through the maximum representable date
        }
        if cursor > window.through {
            return gaps;
        }
    }
    if cursor <= window.through {
        gaps.push(DateRange::new(cursor, window.through));
    }
    gaps
}

fn next_day(date: NaiveDate) -> NaiveDate {
    date.succ_opt().unwrap_or(date)
}

fn next_day_opt(date: NaiveDate) -> Option<NaiveDate> {
    date.succ_opt()
}

fn prev_day(date: NaiveDate) -> NaiveDate {
    date.pred_opt().unwrap_or(date)
}

/// Whether a source's fetch during a refresh succeeded or failed. A failed status carries
/// a CREDENTIAL-SAFE reason (the transport strips query-string credentials before it ever
/// reaches this field — see [`strip_url_credentials`](crate::calendar_refresh::transport::strip_url_credentials)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum SourceFetchStatus {
    /// The source responded and its evidence is in [`RefreshInputs::evidence`].
    Ok,
    /// The source failed. Its accepted evidence + coverage are RETAINED (never removed by
    /// absence); this reason is credential-safe.
    Failed {
        /// A credential-safe description of the failure.
        reason: String,
    },
}

/// The per-source outcome of a refresh attempt. Source-failure retention is driven off
/// these: a [`Failed`](SourceFetchStatus::Failed) source keeps its prior evidence/coverage
/// and ages its freshness dimension, and can never expand coverage by absence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceOutcome {
    /// The [`Source::id`] this outcome is for.
    pub source_id: String,
    /// The kind of source (drives which freshness dimension ages).
    pub kind: SourceKind,
    /// Whether the fetch succeeded or failed.
    pub status: SourceFetchStatus,
    /// The date ranges this source actually covered this fetch (KTD2). The distinction is
    /// load-bearing and drives evidence replacement in [`build_candidate`](crate::calendar_refresh::build_candidate):
    ///
    /// - **absent** (`None`, legacy inputs with no field) → SCOPE-WIDE replacement, today's
    ///   semantics preserved verbatim (a re-covered ok source replaces prior evidence across
    ///   the whole refresh scope);
    /// - **present-but-empty** (`Some([])`, fetched nothing) → replace NOTHING;
    /// - **present** (`Some(ranges)`) → replacement is gated to `ranges ∩ scope`, and even
    ///   inside a covered range a date whose fresh response was empty (no fresh record) never
    ///   retracts a prior positive witness.
    ///
    /// A plain `#[serde(default)]` on a bare `Vec` would collapse absent into
    /// present-but-empty and INVERT legacy semantics, so the field is `Option` and only its
    /// presence (not emptiness) means "gated".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered: Option<Vec<DateRange>>,
}

impl SourceOutcome {
    /// A successful outcome for `source_id` with legacy (absent) coverage — scope-wide
    /// replacement.
    pub fn ok(source_id: impl Into<String>, kind: SourceKind) -> Self {
        SourceOutcome {
            source_id: source_id.into(),
            kind,
            status: SourceFetchStatus::Ok,
            covered: None,
        }
    }

    /// A successful outcome that records exactly which `ranges` it re-covered (KTD2). An
    /// EMPTY `ranges` means fetched-nothing (replace nothing); a non-empty set gates
    /// replacement to `ranges ∩ scope`.
    pub fn ok_covering(
        source_id: impl Into<String>,
        kind: SourceKind,
        ranges: Vec<DateRange>,
    ) -> Self {
        SourceOutcome {
            source_id: source_id.into(),
            kind,
            status: SourceFetchStatus::Ok,
            covered: Some(ranges),
        }
    }

    /// A failed outcome for `source_id` with a credential-safe `reason` and legacy (absent)
    /// coverage.
    pub fn failed(source_id: impl Into<String>, kind: SourceKind, reason: impl Into<String>) -> Self {
        SourceOutcome {
            source_id: source_id.into(),
            kind,
            status: SourceFetchStatus::Failed {
                reason: reason.into(),
            },
            covered: None,
        }
    }

    /// A failed outcome that still records the `ranges` it managed to cover before failing
    /// (KTD2 / R4 honesty carrier — a quota-bounded run ends here). A failed source never
    /// expands coverage, so the ranges are informational, but they keep partial acquisition
    /// from being silently presented as complete.
    pub fn failed_covering(
        source_id: impl Into<String>,
        kind: SourceKind,
        reason: impl Into<String>,
        ranges: Vec<DateRange>,
    ) -> Self {
        SourceOutcome {
            source_id: source_id.into(),
            kind,
            status: SourceFetchStatus::Failed {
                reason: reason.into(),
            },
            covered: Some(ranges),
        }
    }

    /// The date ranges this source covered, if recorded (KTD2). `None` is legacy scope-wide;
    /// `Some(&[])` is fetched-nothing.
    pub fn covered(&self) -> Option<&[DateRange]> {
        self.covered.as_deref()
    }

    /// `true` iff the fetch succeeded.
    pub fn is_ok(&self) -> bool {
        matches!(self.status, SourceFetchStatus::Ok)
    }

    /// The credential-safe failure reason, if this outcome failed.
    pub fn failure_reason(&self) -> Option<&str> {
        match &self.status {
            SourceFetchStatus::Failed { reason } => Some(reason.as_str()),
            SourceFetchStatus::Ok => None,
        }
    }
}

/// The normalized inputs a port yields for a refresh scope: the source records, the
/// normalized evidence the SUCCESSFUL sources produced, and the per-source outcomes. Never
/// carries a raw response body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshInputs {
    /// The normalized [`Source`] records the successful sources correspond to.
    pub sources: Vec<Source>,
    /// The normalized evidence records the successful sources produced.
    pub evidence: Vec<EvidenceRecord>,
    /// The per-source fetch outcomes (retention is driven off these).
    pub outcomes: Vec<SourceOutcome>,
}

impl RefreshInputs {
    /// An empty input set (every source absent).
    pub fn empty() -> Self {
        RefreshInputs {
            sources: Vec::new(),
            evidence: Vec::new(),
            outcomes: Vec::new(),
        }
    }
}

/// The injectable evidence input port. `gather` yields already-normalized evidence for the
/// scope — no raw bodies, no persisted responses. Implementations: [`StaticEvidencePort`]
/// (synthetic, offline) and the maintainer live transport (separate impl).
pub trait EvidenceInputPort {
    /// Gather normalized evidence + per-source outcomes for `scope`.
    fn gather(&self, scope: &RefreshScope) -> RefreshInputs;
}

/// A port over a precomputed [`RefreshInputs`] — the offline/synthetic driver, and the
/// shape the maintainer CLI uses when fed a reviewed normalized-inputs file.
#[derive(Debug, Clone)]
pub struct StaticEvidencePort {
    inputs: RefreshInputs,
}

impl StaticEvidencePort {
    /// Wrap precomputed `inputs`.
    pub fn new(inputs: RefreshInputs) -> Self {
        StaticEvidencePort { inputs }
    }
}

impl EvidenceInputPort for StaticEvidencePort {
    fn gather(&self, _scope: &RefreshScope) -> RefreshInputs {
        self.inputs.clone()
    }
}
