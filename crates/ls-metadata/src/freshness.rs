//! Evidence-freshness computation — the single, pure source of the 90-day
//! backstop rule (the operative half of the Focused-Evidence freshness policy,
//! `metadata/EVIDENCE-FRESHNESS.md`).
//!
//! This module is deliberately leaf-level and **pure**: it parses a
//! `maintenance.last_reviewed` date string and answers two questions against an
//! **injected** as-of date — is the evidence stale, and what is the review-by
//! date. It reads no clock of its own, so the two consumers stay consistent and
//! testable without wall-clock waiting:
//!
//! * `ls-trackers` (the freshness evaluator) injects `as_of = today` to produce
//!   the live stale verdict and a `Severity::Evidence` finding.
//! * `ls-docgen` injects nothing time-varying — it renders [`review_by`], a pure
//!   derivation of `last_reviewed`, so generated docs stay byte-deterministic.
//!
//! There is exactly one copy of the 90-day rule, mirroring the single-source
//! discipline of the trackers' `gates_for`.

use chrono::{Duration, NaiveDate};
use std::fmt;

/// The default freshness window for a Recommended TR: Focused Evidence stays
/// valid for 90 days from `maintenance.last_reviewed` absent any qualifying
/// change. Per-class tightening is deferred (see `metadata/EVIDENCE-FRESHNESS.md`).
pub const DEFAULT_WINDOW_DAYS: i64 = 90;

/// The freshness verdict for a single TR's Focused Evidence, as of an injected
/// date. `Stale` carries the age in days past `last_reviewed` so the evaluator
/// can report "N days past review".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessState {
    /// Evidence is within the window: `age_days <= window_days`.
    Fresh,
    /// Evidence is past the window: `age_days > window_days`.
    Stale { age_days: i64 },
}

impl FreshnessState {
    /// Whether this verdict is stale (the case that warrants a finding).
    pub fn is_stale(self) -> bool {
        matches!(self, FreshnessState::Stale { .. })
    }
}

/// A located freshness failure. The only failure mode is an unparseable
/// `last_reviewed` date. The validator cross-checks `evidence.date ==
/// last_reviewed` by string equality but does not parse either as a date, so a
/// malformed-but-equal pair can reach here — this is the guard that turns it into
/// a loud error, never a silent "fresh".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreshnessError {
    /// `last_reviewed` was not a parseable `YYYY-MM-DD` date.
    UnparseableDate { value: String, message: String },
    /// The review-by date arithmetic overflowed the representable range.
    OutOfRange { value: String, window_days: i64 },
}

impl fmt::Display for FreshnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FreshnessError::UnparseableDate { value, message } => {
                write!(f, "unparseable last_reviewed date `{value}`: {message}")
            }
            FreshnessError::OutOfRange { value, window_days } => write!(
                f,
                "review-by date for `{value}` + {window_days} days is out of range"
            ),
        }
    }
}

impl std::error::Error for FreshnessError {}

/// Parse a `last_reviewed` / evidence date string (`YYYY-MM-DD`, dashed ISO).
fn parse_date(last_reviewed: &str) -> Result<NaiveDate, FreshnessError> {
    NaiveDate::parse_from_str(last_reviewed, "%Y-%m-%d").map_err(|e| {
        FreshnessError::UnparseableDate {
            value: last_reviewed.to_string(),
            message: e.to_string(),
        }
    })
}

/// Evaluate freshness of `last_reviewed` against an **injected** `as_of` date.
///
/// Evidence is stale when `(as_of - last_reviewed) > window_days` — exactly
/// `window_days` is still fresh (boundary is `>`, not `>=`). `as_of` is always
/// injected so tests are deterministic and production passes today's UTC date.
pub fn evaluate(
    last_reviewed: &str,
    as_of: NaiveDate,
    window_days: i64,
) -> Result<FreshnessState, FreshnessError> {
    let reviewed = parse_date(last_reviewed)?;
    let age_days = (as_of - reviewed).num_days();
    if age_days > window_days {
        Ok(FreshnessState::Stale { age_days })
    } else {
        Ok(FreshnessState::Fresh)
    }
}

/// The deterministic review-by date: `last_reviewed + window_days`. Pure
/// function of stored metadata (no clock), so `ls-docgen` can render it into
/// committed docs without breaking byte-determinism.
pub fn review_by(last_reviewed: &str, window_days: i64) -> Result<NaiveDate, FreshnessError> {
    let reviewed = parse_date(last_reviewed)?;
    reviewed
        .checked_add_signed(Duration::days(window_days))
        .ok_or_else(|| FreshnessError::OutOfRange {
            value: last_reviewed.to_string(),
            window_days,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    #[test]
    fn fresh_within_window() {
        // AE1: last_reviewed 2026-06-17, as-of 2026-08-01 (45 days) → Fresh.
        let state = evaluate("2026-06-17", date(2026, 8, 1), DEFAULT_WINDOW_DAYS).unwrap();
        assert_eq!(state, FreshnessState::Fresh);
    }

    #[test]
    fn boundary_exactly_ninety_is_fresh() {
        // AE2: 2026-06-17 + 90 days = 2026-09-15 → still Fresh.
        let state = evaluate("2026-06-17", date(2026, 9, 15), DEFAULT_WINDOW_DAYS).unwrap();
        assert_eq!(state, FreshnessState::Fresh);
    }

    #[test]
    fn boundary_ninety_one_is_stale() {
        // AE2: 2026-09-16 is 91 days → Stale { age_days: 91 }.
        let state = evaluate("2026-06-17", date(2026, 9, 16), DEFAULT_WINDOW_DAYS).unwrap();
        assert_eq!(state, FreshnessState::Stale { age_days: 91 });
        assert!(state.is_stale());
    }

    #[test]
    fn stale_reports_age() {
        let state = evaluate("2026-06-17", date(2026, 12, 1), DEFAULT_WINDOW_DAYS).unwrap();
        match state {
            FreshnessState::Stale { age_days } => assert_eq!(age_days, 167),
            other => panic!("expected stale, got {other:?}"),
        }
    }

    #[test]
    fn unparseable_date_errors_not_panics() {
        let err = evaluate("not-a-date", date(2026, 9, 16), DEFAULT_WINDOW_DAYS).unwrap_err();
        assert!(matches!(err, FreshnessError::UnparseableDate { .. }));
        // And it does NOT silently read as fresh.
        assert!(evaluate("2026/06/17", date(2026, 9, 16), DEFAULT_WINDOW_DAYS).is_err());
    }

    #[test]
    fn review_by_is_deterministic_derivation() {
        let first = review_by("2026-06-17", DEFAULT_WINDOW_DAYS).unwrap();
        let second = review_by("2026-06-17", DEFAULT_WINDOW_DAYS).unwrap();
        assert_eq!(first, date(2026, 9, 15));
        assert_eq!(first, second);
    }
}
