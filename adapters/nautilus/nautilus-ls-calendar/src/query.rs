//! As-of view + proof-preserving day/range queries (U4, KTD5).
//!
//! This is the immutable factual query surface. Everything is evaluated against a
//! caller-supplied UTC instant (KTD5) — there is NO hidden clock and NO reload. A
//! long-lived process holds one loaded [`KrxCalendar`] and constructs a fresh
//! [`AsOfView`] whenever it needs facts "as of now", re-evaluating authorization at
//! that instant without re-reading the artifact.
//!
//! ## Proof preservation
//!
//! Every aggregate keeps the tri-state distinction intact. An [`DayStatus::Unknown`]
//! inside a span is NEVER silently collapsed into `Closed`/[`Presence::Absent`] or
//! into a [`SessionSearch::None`]:
//!
//! - [`AsOfView::presence`] is [`Present`](Presence::Present) only on a positively
//!   proven session, [`Absent`](Presence::Absent) only when *every* date is proven
//!   `Closed`, and [`Indeterminate`](Presence::Indeterminate) the moment any `Unknown`
//!   sits in the span with no proven session.
//! - [`AsOfView::first_session`] / [`AsOfView::last_session`] return
//!   [`Found`](SessionSearch::Found) only for a proven session, [`None`](SessionSearch::None)
//!   only when the whole span is proven `Closed`, and
//!   [`Indeterminate`](SessionSearch::Indeterminate) whenever an `Unknown` sits where it
//!   could change the answer (i.e. before any proven session in the scan direction).
//!
//! ## No synthesis, no repair
//!
//! Range queries require EVERY civil date in the span to be materialized. There is no
//! truncation, gap-fill, weekday synthesis, or silent repair: a span extending past the
//! materialized window is a typed [`QueryError::OutOfRange`], never a shortened result
//! and never an `Unknown`.

use chrono::{DateTime, NaiveDate, Utc};

use crate::load::{CalendarLoadError, KrxCalendar};
use crate::schema::{Alert, Authorization, DayRow, DayStatus, EvidenceRecord};

/// A distinct, typed reason a day/range query could not be answered (KTD3). None of these
/// is ever a [`DayStatus::Unknown`] — they are `Err` values, structurally separate from a
/// successful day fact.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QueryError {
    /// A query target — a single day, or an endpoint of a range span — falls outside the
    /// materialized coverage window. Never a truncated result and never `Unknown`.
    #[error("query target {date} is outside the materialized coverage window")]
    OutOfRange {
        /// The out-of-window civil date.
        date: NaiveDate,
    },

    /// A range constructor's endpoint conversion (`succ`/`pred`) overflowed the
    /// representable civil-date domain (e.g. strictly-between the maximum representable
    /// date). Returned instead of panicking.
    #[error("range endpoint conversion overflowed the representable date domain")]
    DateOverflow,
}

/// A resolved day fact: the tri-state [`DayStatus`] plus the actual evidence/alert
/// records (resolved from the row's id refs), never raw ids or JSON. Borrows from the
/// underlying [`KrxCalendar`].
#[derive(Debug, Clone, PartialEq)]
pub struct DayFact<'c> {
    /// The civil date this fact bears on.
    pub date: NaiveDate,
    /// The reconciled tri-state status.
    pub status: DayStatus,
    /// The evidence records decisive for this status (resolved, in row order).
    pub decisive_evidence: Vec<&'c EvidenceRecord>,
    /// The evidence records that conflicted but did not decide (resolved, in row order).
    pub conflicting_evidence: Vec<&'c EvidenceRecord>,
    /// The alerts attached to this date (resolved, in row order).
    pub alerts: Vec<&'c Alert>,
}

/// Proof-preserving presence of a Trading Session over a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// At least one date is a positively proven Trading Session.
    Present,
    /// Every date in the span is positively proven `Closed` — a proven absence.
    Absent,
    /// No proven session, but at least one `Unknown` date — the answer cannot be proven
    /// (`Unknown` must NOT collapse to `Absent`).
    Indeterminate,
}

/// Proof-preserving first/last-session search over a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSearch {
    /// The first (or last) positively proven Trading Session in the span.
    Found(NaiveDate),
    /// The span is positively proven all-`Closed` — a proven "no session".
    None,
    /// An `Unknown` sits where it could change the answer (before any proven session in
    /// the scan direction), so no session date can be proven.
    Indeterminate,
}

/// A checked date range, normalized internally to ONE canonical inclusive span
/// `[start, end]` (or empty). Construct via [`inclusive`](Self::inclusive),
/// [`half_open`](Self::half_open), or [`strictly_between`](Self::strictly_between) — each
/// handles endpoint conversion + date overflow up front, so the range is well-formed
/// before any query touches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    /// `Some((start, end))` with `start <= end`, or `None` for an empty span.
    span: Option<(NaiveDate, NaiveDate)>,
}

impl DateRange {
    /// An explicitly empty span (no dates).
    pub fn empty() -> Self {
        Self { span: None }
    }

    /// Inclusive `[start, end]`. `start > end` normalizes to empty (not an error).
    pub fn inclusive(start: NaiveDate, end: NaiveDate) -> Result<Self, QueryError> {
        if start > end {
            Ok(Self::empty())
        } else {
            Ok(Self {
                span: Some((start, end)),
            })
        }
    }

    /// Half-open `[start, end)` — the canonical inclusive last date is `end - 1 day`.
    /// `start >= end` (including `start == end`) normalizes to empty.
    pub fn half_open(start: NaiveDate, end: NaiveDate) -> Result<Self, QueryError> {
        if start >= end {
            return Ok(Self::empty());
        }
        // start < end guarantees end > MIN, so pred is Some; guarded for total safety.
        let last = end.pred_opt().ok_or(QueryError::DateOverflow)?;
        Ok(Self {
            span: Some((start, last)),
        })
    }

    /// Strictly-between `(start, end)` — the canonical inclusive span is
    /// `[start + 1 day, end - 1 day]`. Adjacent or overlapping endpoints (where
    /// `start + 1 > end - 1`) normalize to empty; endpoint conversion past the
    /// representable date domain is a typed [`QueryError::DateOverflow`].
    pub fn strictly_between(start: NaiveDate, end: NaiveDate) -> Result<Self, QueryError> {
        let first = start.succ_opt().ok_or(QueryError::DateOverflow)?;
        let last = end.pred_opt().ok_or(QueryError::DateOverflow)?;
        if first > last {
            Ok(Self::empty())
        } else {
            Ok(Self {
                span: Some((first, last)),
            })
        }
    }

    /// `true` iff the canonical span contains no dates.
    pub fn is_empty(&self) -> bool {
        self.span.is_none()
    }

    /// The canonical inclusive `(start, end)` bounds, or `None` for an empty span.
    pub fn bounds(&self) -> Option<(NaiveDate, NaiveDate)> {
        self.span
    }
}

/// A factual view of a loaded [`KrxCalendar`] at one explicit UTC instant (KTD5).
///
/// Constructing a view re-evaluates authorization at `as_of` (a long-lived process makes
/// a fresh view as time passes, and starts getting [`CalendarLoadError::Expired`] once the
/// grant lapses) — but never re-reads the artifact. Day/range queries below never touch
/// the network, never load secrets, and never synthesize or repair missing dates.
#[derive(Debug, Clone, Copy)]
pub struct AsOfView<'c> {
    calendar: &'c KrxCalendar,
    as_of: DateTime<Utc>,
}

impl<'c> AsOfView<'c> {
    /// Build a view of `calendar` as of `instant`, re-evaluating authorization at that
    /// instant (no reload). Returns [`CalendarLoadError::Unauthorized`] /
    /// [`CalendarLoadError::Expired`] if the grant does not authorize use at `instant`.
    pub fn new(
        calendar: &'c KrxCalendar,
        instant: DateTime<Utc>,
    ) -> Result<Self, CalendarLoadError> {
        evaluate_authorization(&calendar.snapshot().authorization, instant)?;
        Ok(Self {
            calendar,
            as_of: instant,
        })
    }

    /// The calendar this view reads.
    pub fn calendar(&self) -> &'c KrxCalendar {
        self.calendar
    }

    /// The instant this view is evaluated at.
    pub fn as_of(&self) -> DateTime<Utc> {
        self.as_of
    }

    /// The tri-state day fact for `date`, with evidence + alert records resolved.
    ///
    /// A date outside the materialized window is a typed [`QueryError::OutOfRange`] —
    /// NEVER an `Unknown`. `Unknown` is a *successful* result: a materialized row whose
    /// maintained evidence does not cover the date.
    pub fn day(&self, date: NaiveDate) -> Result<DayFact<'c>, QueryError> {
        let coverage = self.calendar.coverage();
        if date < coverage.materialized_from || date > coverage.materialized_through {
            return Err(QueryError::OutOfRange { date });
        }
        // Contiguity is a load invariant: a date inside the window always has a row.
        let row = self
            .calendar
            .snapshot()
            .rows
            .iter()
            .find(|r| r.date == date)
            .ok_or(QueryError::OutOfRange { date })?;
        Ok(self.resolve(row))
    }

    /// Proof-preserving presence of a Trading Session over `range`. See [`Presence`].
    /// An empty span is a proven [`Absent`](Presence::Absent) (vacuously no session,
    /// no `Unknown`).
    pub fn presence(&self, range: &DateRange) -> Result<Presence, QueryError> {
        let rows = self.rows_in_range(range)?;
        let mut saw_unknown = false;
        for row in &rows {
            match row.status {
                // A proven session wins outright, regardless of any Unknown elsewhere.
                DayStatus::TradingSession => return Ok(Presence::Present),
                DayStatus::Unknown => saw_unknown = true,
                DayStatus::Closed => {}
            }
        }
        if saw_unknown {
            Ok(Presence::Indeterminate)
        } else {
            Ok(Presence::Absent)
        }
    }

    /// The FIRST proven Trading Session in `range` (proof-preserving). Scanning forward,
    /// an `Unknown` reached before any proven session yields
    /// [`Indeterminate`](SessionSearch::Indeterminate) — it could have been the first
    /// session. An empty span is a proven [`None`](SessionSearch::None).
    pub fn first_session(&self, range: &DateRange) -> Result<SessionSearch, QueryError> {
        let rows = self.rows_in_range(range)?;
        for row in &rows {
            match row.status {
                DayStatus::TradingSession => return Ok(SessionSearch::Found(row.date)),
                DayStatus::Unknown => return Ok(SessionSearch::Indeterminate),
                DayStatus::Closed => {}
            }
        }
        Ok(SessionSearch::None)
    }

    /// The LAST proven Trading Session in `range` (proof-preserving). Scanning backward,
    /// an `Unknown` reached before any proven session yields
    /// [`Indeterminate`](SessionSearch::Indeterminate). An empty span is a proven
    /// [`None`](SessionSearch::None).
    pub fn last_session(&self, range: &DateRange) -> Result<SessionSearch, QueryError> {
        let rows = self.rows_in_range(range)?;
        for row in rows.iter().rev() {
            match row.status {
                DayStatus::TradingSession => return Ok(SessionSearch::Found(row.date)),
                DayStatus::Unknown => return Ok(SessionSearch::Indeterminate),
                DayStatus::Closed => {}
            }
        }
        Ok(SessionSearch::None)
    }

    /// Resolve a row's id refs into a [`DayFact`]. Reference integrity is a load invariant
    /// (no dangling refs), so every id resolves; a defensively-dropped id would only ever
    /// under-report, never fabricate.
    fn resolve(&self, row: &'c DayRow) -> DayFact<'c> {
        let snapshot = self.calendar.snapshot();
        let evidence = |ids: &[String]| -> Vec<&'c EvidenceRecord> {
            ids.iter()
                .filter_map(|id| snapshot.evidence.iter().find(|e| &e.id == id))
                .collect()
        };
        let alerts = row
            .alerts
            .iter()
            .filter_map(|id| snapshot.alerts.iter().find(|a| &a.id == id))
            .collect();
        DayFact {
            date: row.date,
            status: row.status,
            decisive_evidence: evidence(&row.decisive_evidence),
            conflicting_evidence: evidence(&row.conflicting_evidence),
            alerts,
        }
    }

    /// Collect the rows covering `range`, REQUIRING every civil date to be materialized.
    /// An empty span yields no rows. A span endpoint outside the materialized window is a
    /// typed [`QueryError::OutOfRange`] — no truncation, no gap-fill, no synthesis. Because
    /// the loaded window is contiguous + ascending, an in-window `[start, end]` yields
    /// exactly one ascending row per date.
    fn rows_in_range(&self, range: &DateRange) -> Result<Vec<&'c DayRow>, QueryError> {
        let (start, end) = match range.bounds() {
            None => return Ok(Vec::new()),
            Some(bounds) => bounds,
        };
        let coverage = self.calendar.coverage();
        if start < coverage.materialized_from {
            return Err(QueryError::OutOfRange { date: start });
        }
        if end > coverage.materialized_through {
            return Err(QueryError::OutOfRange { date: end });
        }
        Ok(self
            .calendar
            .snapshot()
            .rows
            .iter()
            .filter(|r| r.date >= start && r.date <= end)
            .collect())
    }
}

impl KrxCalendar {
    /// Build an [`AsOfView`] of this calendar at `instant` (KTD5). Convenience for
    /// `AsOfView::new(self, instant)`; re-evaluates authorization at `instant`, no reload.
    pub fn as_of(&self, instant: DateTime<Utc>) -> Result<AsOfView<'_>, CalendarLoadError> {
        AsOfView::new(self, instant)
    }
}

/// Re-evaluate a recorded authorization at `as_of`, using the identical inclusive-at /
/// expired-after boundary rule the loader applies (see `load.rs` module doc).
fn evaluate_authorization(
    authorization: &Authorization,
    as_of: DateTime<Utc>,
) -> Result<(), CalendarLoadError> {
    if !authorization.authorized {
        return Err(CalendarLoadError::Unauthorized);
    }
    if let Some(expires_at) = authorization.expires_at {
        if as_of > expires_at {
            return Err(CalendarLoadError::Expired);
        }
    }
    if let Some(terminated_at) = authorization.terminated_at {
        if as_of > terminated_at {
            return Err(CalendarLoadError::Expired);
        }
    }
    Ok(())
}
