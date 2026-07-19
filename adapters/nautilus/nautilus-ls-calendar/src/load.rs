//! Typed loading + validation of a snapshot from an EXPLICIT path (U3, KTD3/KTD5).
//!
//! [`KrxCalendar`] wraps a snapshot that has passed every structural, identity, coverage,
//! reference, and authorization invariant. Construction is the ONLY way to obtain one, so a
//! held `KrxCalendar` is always valid at the as-of instant it was constructed with.
//!
//! Every failure mode is a distinct [`CalendarLoadError`] variant (KTD3). Crucially, a
//! load/validate failure is NEVER a [`schema::DayStatus::Unknown`](crate::schema::DayStatus):
//! `Unknown` is a *successful factual result* of a *loaded* calendar (maintained evidence
//! does not cover a date); a failure is structurally an `Err(CalendarLoadError)`, a value
//! that cannot be confused with a day fact.
//!
//! The loader is deliberately inert about *where* the snapshot lives: it takes an explicit
//! `&Path` — **no** default path, **no** environment read, **no** singleton, **no** file
//! watch, **no** hot reload. Path resolution is a composition-root concern (U8), never the
//! core's.
//!
//! ## Authorization boundary semantics (KTD5)
//!
//! Authorization is evaluated at a caller-supplied UTC instant. A bounded grant is
//! **valid AT the recorded expiry/termination instant and expired STRICTLY AFTER it**
//! (`as_of <= expires_at` is authorized; `as_of > expires_at` is [`CalendarLoadError::Expired`]).
//! Early termination (`terminated_at`) uses the identical inclusive-at / expired-after rule
//! and also surfaces as `Expired`.

use std::io::ErrorKind;
use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};

use crate::canonical::{compute_artifact_id, compute_calendar_id, schema_is_compatible};
use crate::schema::{Authorization, Coverage, Snapshot};

/// A distinct, typed reason a snapshot could not be loaded or validated (KTD3). None of
/// these is ever a [`schema::DayStatus`](crate::schema::DayStatus) — they are `Err` values,
/// structurally separate from a successful day fact.
#[derive(Debug, thiserror::Error)]
pub enum CalendarLoadError {
    /// No file exists at the given path.
    #[error("calendar snapshot file not found")]
    Missing,

    /// The path exists but its bytes could not be read (permissions, is-a-directory, I/O).
    #[error("calendar snapshot file could not be read: {message}")]
    Unreadable {
        /// The underlying I/O error rendering.
        message: String,
    },

    /// The bytes are not a well-formed snapshot JSON document (serde parse failure).
    #[error("calendar snapshot is not valid JSON for the schema: {message}")]
    Corrupt {
        /// The underlying serde error rendering.
        message: String,
    },

    /// The snapshot declares a schema version this crate cannot understand (different MAJOR
    /// or a malformed version string).
    #[error("unsupported snapshot schema version: {found}")]
    UnsupportedSchema {
        /// The version string the snapshot declared.
        found: String,
    },

    /// A recomputed deterministic identity does not match the declared one — the snapshot
    /// was altered after stamping, or was never canonically stamped.
    #[error("recomputed {field} does not match the declared identity")]
    HashMismatch {
        /// Which identity disagreed: `"artifact_id"` or `"calendar_id"`.
        field: String,
    },

    /// The recorded authorization does not authorize use (`authorized == false`).
    #[error("calendar data is not authorized for use")]
    Unauthorized,

    /// Authorization has lapsed at the requested as-of instant (past `expires_at`, or past
    /// an early `terminated_at`).
    #[error("calendar authorization has expired or been terminated as of the requested instant")]
    Expired,

    /// A query target falls outside the materialized coverage window. (Declared here for the
    /// query layer, U4; the loader never produces it.)
    #[error("query target is outside the materialized coverage window")]
    OutOfRange,

    /// The materialized rows are missing a civil date inside the coverage window (a gap, an
    /// out-of-order arrangement that skips a date, or rows that end before the window does).
    #[error("materialized coverage has a gap at {date}")]
    Gapped {
        /// The first civil date that should be present but is not (in canonical order).
        date: NaiveDate,
    },

    /// A civil date is materialized more than once (or a date reappears out of order).
    #[error("materialized coverage duplicates {date}")]
    Duplicated {
        /// The repeated civil date.
        date: NaiveDate,
    },

    /// A row references an evidence or alert id that does not resolve to a real record.
    #[error("dangling reference to unknown id: {reference}")]
    DanglingReference {
        /// The unresolved evidence/alert id.
        reference: String,
    },

    /// The coverage claims are internally impossible (e.g. `materialized_from` after
    /// `materialized_through`, or `retrospectively_checked_through` beyond
    /// `materialized_through`, or rows extending past the declared window).
    #[error("impossible coverage: {detail}")]
    ImpossibleCoverage {
        /// A human-readable description of the contradiction.
        detail: String,
    },
}

/// A validated, immutable KRX trading calendar: a [`Snapshot`] that has passed every U3
/// invariant at a caller-supplied as-of instant. The only way to obtain one is through a
/// checked constructor, so holding a `KrxCalendar` is proof the snapshot is well-formed,
/// canonically identified, contiguous, reference-clean, and authorized as of that instant.
#[derive(Debug, Clone)]
pub struct KrxCalendar {
    snapshot: Snapshot,
}

impl KrxCalendar {
    /// Load and validate a snapshot from an EXPLICIT filesystem path, evaluating
    /// authorization/coverage at `as_of`. Takes the path verbatim — no default, no env, no
    /// singleton, no watch, no hot reload (KTD3/KTD5).
    ///
    /// Returns a distinct [`CalendarLoadError`] for every failure; none is ever a
    /// [`DayStatus::Unknown`](crate::schema::DayStatus).
    pub fn load_from_path(path: &Path, as_of: DateTime<Utc>) -> Result<Self, CalendarLoadError> {
        let bytes = std::fs::read(path).map_err(|err| match err.kind() {
            ErrorKind::NotFound => CalendarLoadError::Missing,
            _ => CalendarLoadError::Unreadable {
                message: err.to_string(),
            },
        })?;

        let snapshot: Snapshot =
            serde_json::from_slice(&bytes).map_err(|err| CalendarLoadError::Corrupt {
                message: err.to_string(),
            })?;

        Self::from_snapshot(snapshot, as_of)
    }

    /// Validate an already-parsed [`Snapshot`] value at `as_of` (for tests/fixtures that
    /// hold a value directly). Runs every invariant except the file read + JSON parse that
    /// only [`load_from_path`](Self::load_from_path) performs.
    pub fn from_snapshot(
        snapshot: Snapshot,
        as_of: DateTime<Utc>,
    ) -> Result<Self, CalendarLoadError> {
        validate(&snapshot, as_of)?;
        Ok(Self { snapshot })
    }

    /// The validated underlying snapshot.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// The declared (and identity-verified) `artifact_id`.
    pub fn artifact_id(&self) -> &str {
        &self.snapshot.artifact_id
    }

    /// The declared (and identity-verified) `calendar_id`.
    pub fn calendar_id(&self) -> &str {
        &self.snapshot.calendar_id
    }

    /// The declared schema version (already confirmed compatible).
    pub fn schema_version(&self) -> &str {
        &self.snapshot.schema_version
    }

    /// The distinct coverage claims.
    pub fn coverage(&self) -> &Coverage {
        &self.snapshot.coverage
    }
}

/// Run every U3 invariant over `snapshot` at `as_of`. Order is deliberate so the most
/// fundamental faults (schema, identity) surface before finer ones.
fn validate(snapshot: &Snapshot, as_of: DateTime<Utc>) -> Result<(), CalendarLoadError> {
    // (a) Schema compatibility — before anything trusts the shape's meaning.
    if !schema_is_compatible(&snapshot.schema_version) {
        return Err(CalendarLoadError::UnsupportedSchema {
            found: snapshot.schema_version.clone(),
        });
    }

    // (b) Deterministic identity recompute — the declared ids must match content.
    if compute_artifact_id(snapshot) != snapshot.artifact_id {
        return Err(CalendarLoadError::HashMismatch {
            field: "artifact_id".to_string(),
        });
    }
    if compute_calendar_id(snapshot) != snapshot.calendar_id {
        return Err(CalendarLoadError::HashMismatch {
            field: "calendar_id".to_string(),
        });
    }

    // (f) Coverage invariants — reject impossible coverage before trusting the window.
    validate_coverage(&snapshot.coverage)?;

    // (c)+(d) Canonical ascending order, unique, contiguous over the coverage window.
    validate_rows(snapshot)?;

    // (e) Evidence/alert reference integrity — no dangling refs.
    validate_references(snapshot)?;

    // (g) Authorization current at the as-of instant.
    validate_authorization(&snapshot.authorization, as_of)?;

    Ok(())
}

/// (f) The coverage claims must be internally consistent.
fn validate_coverage(coverage: &Coverage) -> Result<(), CalendarLoadError> {
    if coverage.materialized_from > coverage.materialized_through {
        return Err(CalendarLoadError::ImpossibleCoverage {
            detail: format!(
                "materialized_from {} is after materialized_through {}",
                coverage.materialized_from, coverage.materialized_through
            ),
        });
    }
    if coverage.retrospectively_checked_through > coverage.materialized_through {
        return Err(CalendarLoadError::ImpossibleCoverage {
            detail: format!(
                "retrospectively_checked_through {} is beyond materialized_through {}",
                coverage.retrospectively_checked_through, coverage.materialized_through
            ),
        });
    }
    Ok(())
}

/// (c)+(d) The rows must be in canonical ascending order and cover EVERY civil date in
/// `[materialized_from, materialized_through]` exactly once — no gaps, no dupes, no
/// out-of-order arrangement, no rows outside the window.
///
/// A single forward walk enforces all of this: any non-sorted permutation of a contiguous
/// set necessarily breaks the "each row is the running cursor" expectation, so it surfaces
/// as a [`Duplicated`](CalendarLoadError::Duplicated) (a date at/behind the cursor) or a
/// [`Gapped`](CalendarLoadError::Gapped) (a date ahead of the cursor).
fn validate_rows(snapshot: &Snapshot) -> Result<(), CalendarLoadError> {
    let coverage = &snapshot.coverage;
    let mut cursor = coverage.materialized_from;

    for row in &snapshot.rows {
        if row.date > coverage.materialized_through {
            return Err(CalendarLoadError::ImpossibleCoverage {
                detail: format!(
                    "row {} is past materialized_through {}",
                    row.date, coverage.materialized_through
                ),
            });
        }
        if row.date < cursor {
            // A date at or behind the running cursor: an outright duplicate, or a date
            // re-appearing after we already advanced past it (out-of-order).
            return Err(CalendarLoadError::Duplicated { date: row.date });
        }
        if row.date > cursor {
            // The expected next date is missing at this position.
            return Err(CalendarLoadError::Gapped { date: cursor });
        }
        // row.date == cursor: advance to the next expected civil date.
        cursor = cursor
            .succ_opt()
            .ok_or_else(|| CalendarLoadError::ImpossibleCoverage {
                detail: "civil-date overflow while walking coverage".to_string(),
            })?;
    }

    // The walk must have consumed exactly through `materialized_through`.
    let expected_end = coverage.materialized_through.succ_opt().ok_or_else(|| {
        CalendarLoadError::ImpossibleCoverage {
            detail: "materialized_through is the maximum representable date".to_string(),
        }
    })?;
    if cursor != expected_end {
        // Rows ended before the window did → `cursor` is the first missing date.
        return Err(CalendarLoadError::Gapped { date: cursor });
    }

    Ok(())
}

/// (e) Every evidence/alert id a row references must resolve to a real record.
fn validate_references(snapshot: &Snapshot) -> Result<(), CalendarLoadError> {
    let evidence_ids: std::collections::HashSet<&str> =
        snapshot.evidence.iter().map(|e| e.id.as_str()).collect();
    let alert_ids: std::collections::HashSet<&str> =
        snapshot.alerts.iter().map(|a| a.id.as_str()).collect();

    for row in &snapshot.rows {
        for reference in row
            .decisive_evidence
            .iter()
            .chain(row.conflicting_evidence.iter())
        {
            if !evidence_ids.contains(reference.as_str()) {
                return Err(CalendarLoadError::DanglingReference {
                    reference: reference.clone(),
                });
            }
        }
        for reference in &row.alerts {
            if !alert_ids.contains(reference.as_str()) {
                return Err(CalendarLoadError::DanglingReference {
                    reference: reference.clone(),
                });
            }
        }
    }

    Ok(())
}

/// (g) Authorization must be current at `as_of`: granted, not expired, not terminated.
/// Boundary is inclusive AT the recorded instant; lapsed STRICTLY after it (see module doc).
fn validate_authorization(
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
