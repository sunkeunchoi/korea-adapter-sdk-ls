//! Freshness staleness evaluation at the as-of instant (U7, AC8/KTD).
//!
//! [`AsOfView::freshness`] reports, per dimension, whether the maintained evidence is
//! stale AT the view's as-of instant. Staleness is **strictly separate from day status**:
//! this evaluation reads ONLY the snapshot's [`Freshness`](crate::schema::Freshness) block
//! plus the view's as-of instant, and it NEVER reads, touches, or rewrites any
//! [`DayStatus`](crate::schema::DayStatus). A stale calendar still answers the exact same
//! tri-state day facts it answered fresh — staleness is an out-of-band operational signal,
//! not a status input (AC8).
//!
//! # Threshold arithmetic (deterministic)
//!
//! Every instant-based dimension uses the **identical inclusive-at / stale-after** rule the
//! loader uses for authorization (see `load.rs`): an anchor is fresh AT `anchor + threshold`
//! and stale STRICTLY AFTER it —
//!
//! ```text
//! stale  ⇔  as_of > anchor + threshold
//! ```
//!
//! - **KASI holiday facts** — threshold [`KASI_STALE_AFTER_DAYS`] (14 days) on
//!   `holiday_facts_checked_at`.
//! - **Full history** — threshold [`FULL_HISTORY_STALE_AFTER_DAYS`] (120 days) on
//!   `full_history_reconciled_at`.
//! - **Incremental** — two missed daily post-close opportunities. Each civil day offers one
//!   post-close refresh opportunity 24h after the last refresh; two opportunities span
//!   [`INCREMENTAL_STALE_AFTER_DAYS`] (2) days, so `stale ⇔ as_of > last_incremental_at + 2
//!   days` — fresh right through the second opportunity's instant, stale only once that
//!   second opportunity has passed unmet.
//! - **Forward readiness** — date-granular, not instant-granular. With `as_of_date` the UTC
//!   civil date of the as-of instant, `remaining = forward_readiness_through - as_of_date`
//!   and `stale ⇔ remaining < `[`FORWARD_READINESS_MIN_DAYS`] (45). Because the horizon is a
//!   civil date, this dimension's "tick" is one day (crossing UTC midnight), where the
//!   others' tick is one second.
//!
//! A dimension whose anchor is absent (`None`) is [`DimensionStaleness::Unevaluated`]: never
//! silently "fresh", never "stale", and — like every other outcome here — never affecting a
//! status.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;

use crate::query::AsOfView;

/// KASI holiday-facts staleness threshold: stale strictly after this many days.
pub const KASI_STALE_AFTER_DAYS: i64 = 14;
/// Full-history reconciliation staleness threshold: stale strictly after this many days.
pub const FULL_HISTORY_STALE_AFTER_DAYS: i64 = 120;
/// Incremental staleness threshold: two missed daily post-close opportunities = 2 days.
pub const INCREMENTAL_STALE_AFTER_DAYS: i64 = 2;
/// Forward-readiness minimum: stale once fewer than this many evaluated days remain.
pub const FORWARD_READINESS_MIN_DAYS: i64 = 45;

/// The staleness of one freshness dimension at the as-of instant. Deliberately does NOT
/// carry or imply a [`DayStatus`](crate::schema::DayStatus) (AC8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionStaleness {
    /// Within its threshold at the as-of instant.
    Fresh,
    /// Past its threshold at the as-of instant.
    Stale,
    /// The dimension's anchor is absent in the snapshot — nothing to evaluate against.
    Unevaluated,
}

impl DimensionStaleness {
    /// `true` iff this dimension is [`Stale`](DimensionStaleness::Stale) (an `Unevaluated`
    /// or `Fresh` dimension is not stale).
    pub fn is_stale(self) -> bool {
        matches!(self, DimensionStaleness::Stale)
    }
}

/// A per-dimension freshness verdict computed at a fixed as-of instant. Purely advisory —
/// computing it neither reads nor mutates any [`DayStatus`](crate::schema::DayStatus).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FreshnessReport {
    /// The instant this report was evaluated at.
    pub as_of: DateTime<Utc>,
    /// KASI holiday-facts input freshness (14-day threshold).
    pub kasi_holiday_facts: DimensionStaleness,
    /// Full-history reconciliation freshness (120-day threshold).
    pub full_history: DimensionStaleness,
    /// Incremental-refresh freshness (two-missed-opportunity / 2-day threshold).
    pub incremental: DimensionStaleness,
    /// Forward-readiness horizon freshness (45-evaluated-days-remaining threshold).
    pub forward_readiness: DimensionStaleness,
}

impl FreshnessReport {
    /// `true` iff ANY evaluated dimension is stale.
    pub fn any_stale(&self) -> bool {
        self.kasi_holiday_facts.is_stale()
            || self.full_history.is_stale()
            || self.incremental.is_stale()
            || self.forward_readiness.is_stale()
    }
}

/// Instant-based staleness: `Unevaluated` when the anchor is absent, otherwise the shared
/// inclusive-at / stale-after rule (`stale ⇔ as_of > anchor + threshold_days`).
fn instant_staleness(
    anchor: Option<DateTime<Utc>>,
    as_of: DateTime<Utc>,
    threshold_days: i64,
) -> DimensionStaleness {
    match anchor {
        None => DimensionStaleness::Unevaluated,
        Some(anchor) => {
            if as_of > anchor + Duration::days(threshold_days) {
                DimensionStaleness::Stale
            } else {
                DimensionStaleness::Fresh
            }
        }
    }
}

/// Date-granular forward-readiness staleness: `Unevaluated` when the horizon is absent,
/// otherwise stale once fewer than [`FORWARD_READINESS_MIN_DAYS`] evaluated days remain
/// between the as-of civil date and the established horizon.
fn forward_readiness_staleness(
    through: Option<NaiveDate>,
    as_of: DateTime<Utc>,
) -> DimensionStaleness {
    match through {
        None => DimensionStaleness::Unevaluated,
        Some(through) => {
            let remaining = (through - as_of.date_naive()).num_days();
            if remaining < FORWARD_READINESS_MIN_DAYS {
                DimensionStaleness::Stale
            } else {
                DimensionStaleness::Fresh
            }
        }
    }
}

impl AsOfView<'_> {
    /// Evaluate every freshness dimension at this view's as-of instant (AC8). Reads only the
    /// snapshot's freshness block; never reads or rewrites any day status.
    pub fn freshness(&self) -> FreshnessReport {
        let freshness = &self.calendar().snapshot().freshness;
        let as_of = self.as_of();
        FreshnessReport {
            as_of,
            kasi_holiday_facts: instant_staleness(
                freshness.holiday_facts_checked_at,
                as_of,
                KASI_STALE_AFTER_DAYS,
            ),
            full_history: instant_staleness(
                freshness.full_history_reconciled_at,
                as_of,
                FULL_HISTORY_STALE_AFTER_DAYS,
            ),
            incremental: instant_staleness(
                freshness.last_incremental_at,
                as_of,
                INCREMENTAL_STALE_AFTER_DAYS,
            ),
            forward_readiness: forward_readiness_staleness(
                freshness.forward_readiness_through,
                as_of,
            ),
        }
    }
}
