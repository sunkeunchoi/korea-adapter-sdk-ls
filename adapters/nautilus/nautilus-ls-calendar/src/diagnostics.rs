//! Normalized diagnostic records + stable human/JSON rendering with FIELD-LEVEL
//! redaction (U8, AC10).
//!
//! A [`CalendarDiagnostic`] is the one normalized record every diagnostic surface
//! (`calendar status`, the startup record) renders. It is built by a constructor
//! that DROPS or MASKS every credential/authorization identity at construction time,
//! so BOTH render forms ([`render_human`], [`render_json`]) are safe by construction —
//! there is no raw identity anywhere in the struct to leak.
//!
//! ## Why field-level redaction (not a token heuristic)
//!
//! The adapter's `scrub.rs` masks only account-number (6+ digit) and long-token
//! (20+ alphanumeric) shapes. A granting-authority or maintainer identity — e.g. a
//! short human name or an agreement label — has neither shape and would pass the
//! heuristic straight through. So the redaction here is structural: the raw
//! [`Authorization::authority`] never enters the record; only a non-reversible
//! [`mask_identity`] fingerprint does.
//!
//! ## Outcomes
//!
//! Every diagnostic classifies into one [`DiagnosticOutcome`]: `Healthy`, `Stale`,
//! `Unknown`, `Conflict`, `OutOfRange`, or a typed [`Load`](DiagnosticOutcome::Load)
//! failure — covering the healthy / stale / Unknown / conflict / coverage / load /
//! use / query surface the preflight must report.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::freshness::FreshnessReport;
use crate::load::CalendarLoadError;
use crate::query::{AsOfView, QueryError};
use crate::schema::{Authorization, Coverage, DayStatus};

/// The top-level classification a diagnostic reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticOutcome {
    /// Loaded, authorized, fresh; the queried day resolved to a definite status
    /// (Trading Session or Closed) with no retained conflict.
    Healthy,
    /// Loaded and usable, but at least one freshness dimension is stale at the as-of
    /// instant. The day status is unaffected (AC8).
    Stale,
    /// The queried day resolved to a successful `Unknown` factual result.
    Unknown,
    /// The queried day carries retained conflicting evidence / reconciliation alerts.
    Conflict,
    /// A day/range query target fell outside the materialized coverage window.
    OutOfRange,
    /// The snapshot could not be loaded or used — a typed load/use failure.
    Load(LoadFailure),
}

impl DiagnosticOutcome {
    /// `true` iff this is a clean, usable outcome (`Healthy`/`Stale`/`Unknown`/`Conflict`
    /// are all successful factual results; `OutOfRange` and `Load` are failures).
    pub fn is_usable(self) -> bool {
        matches!(
            self,
            DiagnosticOutcome::Healthy
                | DiagnosticOutcome::Stale
                | DiagnosticOutcome::Unknown
                | DiagnosticOutcome::Conflict
        )
    }

    /// The stable lower-case token used in the human/JSON rendering.
    pub fn token(self) -> String {
        match self {
            DiagnosticOutcome::Healthy => "healthy".to_string(),
            DiagnosticOutcome::Stale => "stale".to_string(),
            DiagnosticOutcome::Unknown => "unknown".to_string(),
            DiagnosticOutcome::Conflict => "conflict".to_string(),
            DiagnosticOutcome::OutOfRange => "out_of_range".to_string(),
            DiagnosticOutcome::Load(kind) => format!("load:{}", kind.token()),
        }
    }
}

/// A typed load/use failure class (mirrors the distinct [`CalendarLoadError`] variants,
/// collapsed to the classes an operator acts on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadFailure {
    /// No snapshot at the configured path.
    Missing,
    /// The path exists but its bytes could not be read.
    Unreadable,
    /// The bytes are not a well-formed snapshot document.
    Corrupt,
    /// The snapshot declares a schema version this crate cannot understand.
    Incompatible,
    /// A recomputed deterministic identity did not match the declared one.
    Integrity,
    /// The recorded grant does not authorize use.
    Unauthorized,
    /// Authorization has expired or been terminated at the as-of instant.
    Expired,
    /// A structural coverage invariant failed (gap, duplicate, dangling ref, impossible).
    Coverage,
}

impl LoadFailure {
    /// The stable lower-case token used in rendering.
    pub fn token(self) -> &'static str {
        match self {
            LoadFailure::Missing => "missing",
            LoadFailure::Unreadable => "unreadable",
            LoadFailure::Corrupt => "corrupt",
            LoadFailure::Incompatible => "incompatible",
            LoadFailure::Integrity => "integrity",
            LoadFailure::Unauthorized => "unauthorized",
            LoadFailure::Expired => "expired",
            LoadFailure::Coverage => "coverage",
        }
    }
}

/// The authorization facts safe to render. The raw granting-authority identity
/// ([`Authorization::authority`]) is NEVER carried here — only a non-reversible
/// [`mask_identity`] fingerprint (field-level redaction, U8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorizationView {
    /// Whether the recorded grant authorizes use.
    pub authorized: bool,
    /// When authorization expires, if bounded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// When authorization was terminated early, if it has been.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminated_at: Option<DateTime<Utc>>,
    /// A non-reversible fingerprint of the granting-authority identity — the raw
    /// identity string is dropped at construction and never appears (redaction, U8).
    pub authority_fingerprint: String,
}

impl AuthorizationView {
    /// Build the redacted view — the raw `authority` is masked, never copied.
    fn redacted(auth: &Authorization) -> Self {
        Self {
            authorized: auth.authorized,
            expires_at: auth.expires_at,
            terminated_at: auth.terminated_at,
            authority_fingerprint: mask_identity(&auth.authority),
        }
    }
}

/// The distinct coverage claims, rendered as-is (dates are not identities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CoverageSummary {
    /// First materialized civil date.
    pub materialized_from: NaiveDate,
    /// Last materialized civil date.
    pub materialized_through: NaiveDate,
    /// Last date whose evidence was retrospectively re-checked.
    pub retrospectively_checked_through: NaiveDate,
    /// Last date scheduled closures were evaluated through.
    pub scheduled_closure_evaluated_through: NaiveDate,
}

impl CoverageSummary {
    fn from(coverage: &Coverage) -> Self {
        Self {
            materialized_from: coverage.materialized_from,
            materialized_through: coverage.materialized_through,
            retrospectively_checked_through: coverage.retrospectively_checked_through,
            scheduled_closure_evaluated_through: coverage.scheduled_closure_evaluated_through,
        }
    }
}

/// The one normalized diagnostic record every surface renders. Built REDACTED: no
/// credential or authorization identity is ever a field (U8/AC10).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalendarDiagnostic {
    /// The top-level outcome classification.
    pub outcome: DiagnosticOutcome,
    /// The instant this diagnostic was evaluated at (KTD5).
    pub as_of: DateTime<Utc>,
    /// The snapshot's `artifact_id`, when a snapshot was loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// The snapshot's `calendar_id`, when a snapshot was loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
    /// The redacted authorization facts, when a snapshot was loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<AuthorizationView>,
    /// The distinct coverage claims, when a snapshot was loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSummary>,
    /// The per-dimension freshness verdict, when a snapshot was loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freshness: Option<FreshnessReport>,
    /// The queried day, when a day query was attempted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_day: Option<NaiveDate>,
    /// The resolved tri-state day status, when the query succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_status: Option<DayStatus>,
    /// Reconciliation alert messages attached to the target day.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<String>,
    /// A short human-readable detail for the outcome.
    pub detail: String,
}

impl CalendarDiagnostic {
    /// Build a diagnostic from a typed load/use failure (missing, corrupt,
    /// incompatible, unauthorized, expired, integrity, coverage, out-of-range). The
    /// snapshot facts are absent because there is no loaded snapshot.
    pub fn from_load_error(as_of: DateTime<Utc>, err: &CalendarLoadError) -> Self {
        let (outcome, detail) = match err {
            CalendarLoadError::Missing => (
                DiagnosticOutcome::Load(LoadFailure::Missing),
                "snapshot file not found".to_string(),
            ),
            CalendarLoadError::Unreadable { .. } => (
                DiagnosticOutcome::Load(LoadFailure::Unreadable),
                "snapshot file could not be read".to_string(),
            ),
            CalendarLoadError::Corrupt { .. } => (
                DiagnosticOutcome::Load(LoadFailure::Corrupt),
                "snapshot JSON is not well-formed for the schema".to_string(),
            ),
            CalendarLoadError::UnsupportedSchema { found } => (
                DiagnosticOutcome::Load(LoadFailure::Incompatible),
                format!("unsupported schema version {found}"),
            ),
            CalendarLoadError::HashMismatch { field } => (
                DiagnosticOutcome::Load(LoadFailure::Integrity),
                format!("recomputed {field} does not match the declared identity"),
            ),
            CalendarLoadError::Unauthorized => (
                DiagnosticOutcome::Load(LoadFailure::Unauthorized),
                "calendar data is not authorized for use".to_string(),
            ),
            CalendarLoadError::Expired => (
                DiagnosticOutcome::Load(LoadFailure::Expired),
                "calendar authorization has expired or been terminated".to_string(),
            ),
            CalendarLoadError::OutOfRange => (
                DiagnosticOutcome::OutOfRange,
                "query target is outside the materialized coverage window".to_string(),
            ),
            CalendarLoadError::Gapped { date } => (
                DiagnosticOutcome::Load(LoadFailure::Coverage),
                format!("materialized coverage has a gap at {date}"),
            ),
            CalendarLoadError::Duplicated { date } => (
                DiagnosticOutcome::Load(LoadFailure::Coverage),
                format!("materialized coverage duplicates {date}"),
            ),
            CalendarLoadError::DanglingReference { reference } => (
                DiagnosticOutcome::Load(LoadFailure::Coverage),
                format!("dangling evidence/alert reference: {reference}"),
            ),
            CalendarLoadError::ImpossibleCoverage { detail } => (
                DiagnosticOutcome::Load(LoadFailure::Coverage),
                format!("impossible coverage: {detail}"),
            ),
        };
        Self {
            outcome,
            as_of,
            artifact_id: None,
            calendar_id: None,
            authorization: None,
            coverage: None,
            freshness: None,
            target_day: None,
            day_status: None,
            alerts: Vec::new(),
            detail,
        }
    }

    /// Build a diagnostic from a loaded calendar view + a queried civil date. The
    /// snapshot facts (identities, redacted authorization, coverage, freshness) are
    /// populated, and the outcome reflects the day fact:
    ///
    /// - `Unknown` status → [`DiagnosticOutcome::Unknown`].
    /// - retained conflict (conflicting evidence or alerts) → [`DiagnosticOutcome::Conflict`].
    /// - otherwise a stale freshness dimension → [`DiagnosticOutcome::Stale`].
    /// - otherwise → [`DiagnosticOutcome::Healthy`].
    /// - a query outside the materialized window → [`DiagnosticOutcome::OutOfRange`].
    pub fn from_view(view: &AsOfView<'_>, target: NaiveDate) -> Self {
        let calendar = view.calendar();
        let as_of = view.as_of();
        let freshness = view.freshness();
        let artifact_id = Some(calendar.artifact_id().to_string());
        let calendar_id = Some(calendar.calendar_id().to_string());
        let authorization = Some(AuthorizationView::redacted(
            &calendar.snapshot().authorization,
        ));
        let coverage = Some(CoverageSummary::from(calendar.coverage()));

        match view.day(target) {
            Ok(fact) => {
                let alerts: Vec<String> = fact.alerts.iter().map(|a| a.message.clone()).collect();
                let has_conflict = !fact.conflicting_evidence.is_empty() || !fact.alerts.is_empty();
                let outcome = if fact.status == DayStatus::Unknown {
                    DiagnosticOutcome::Unknown
                } else if has_conflict {
                    DiagnosticOutcome::Conflict
                } else if freshness.any_stale() {
                    DiagnosticOutcome::Stale
                } else {
                    DiagnosticOutcome::Healthy
                };
                let detail = match outcome {
                    DiagnosticOutcome::Unknown => {
                        format!("{target} is Unknown (maintained evidence does not cover it)")
                    }
                    DiagnosticOutcome::Conflict => {
                        format!("{target} carries a retained reconciliation conflict")
                    }
                    DiagnosticOutcome::Stale => {
                        format!("{target} is {:?}; a freshness dimension is stale", fact.status)
                    }
                    _ => format!("{target} is {:?}; calendar healthy", fact.status),
                };
                Self {
                    outcome,
                    as_of,
                    artifact_id,
                    calendar_id,
                    authorization,
                    coverage,
                    freshness: Some(freshness),
                    target_day: Some(target),
                    day_status: Some(fact.status),
                    alerts,
                    detail,
                }
            }
            Err(query_err) => {
                let detail = match query_err {
                    QueryError::OutOfRange { date } => {
                        format!("query target {date} is outside the materialized coverage window")
                    }
                    QueryError::DateOverflow => {
                        "query range endpoint overflowed the representable date domain".to_string()
                    }
                };
                Self {
                    outcome: DiagnosticOutcome::OutOfRange,
                    as_of,
                    artifact_id,
                    calendar_id,
                    authorization,
                    coverage,
                    freshness: Some(freshness),
                    target_day: Some(target),
                    day_status: None,
                    alerts: Vec::new(),
                    detail,
                }
            }
        }
    }
}

/// Mask an identity string to a short, non-reversible fingerprint. Field-level
/// redaction (U8): the raw identity NEVER appears in a diagnostic; only this hashed
/// token does — so a maintainer name / agreement label that a token heuristic would
/// pass through can still not leak.
pub fn mask_identity(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("redacted-sha256:{hex}")
}

/// Render the diagnostic as stable, human-readable multi-line text. Contains no raw
/// credential or authorization identity (the record is redacted by construction).
pub fn render_human(diagnostic: &CalendarDiagnostic) -> String {
    let mut out = String::new();
    out.push_str("calendar diagnostic\n");
    out.push_str(&format!("  outcome: {}\n", diagnostic.outcome.token()));
    out.push_str(&format!("  as_of: {}\n", diagnostic.as_of.to_rfc3339()));
    match (&diagnostic.artifact_id, &diagnostic.calendar_id) {
        (Some(a), Some(c)) => {
            out.push_str(&format!("  artifact_id: {a}\n"));
            out.push_str(&format!("  calendar_id: {c}\n"));
        }
        _ => out.push_str("  snapshot: unavailable\n"),
    }
    if let Some(auth) = &diagnostic.authorization {
        out.push_str(&format!(
            "  authorization: {} (authority {})\n",
            if auth.authorized { "authorized" } else { "unauthorized" },
            auth.authority_fingerprint
        ));
        if let Some(expires) = auth.expires_at {
            out.push_str(&format!("    expires_at: {}\n", expires.to_rfc3339()));
        }
        if let Some(terminated) = auth.terminated_at {
            out.push_str(&format!("    terminated_at: {}\n", terminated.to_rfc3339()));
        }
    }
    if let Some(cov) = &diagnostic.coverage {
        out.push_str(&format!(
            "  coverage: materialized {}..{} · retro-checked {} · scheduled-closures {}\n",
            cov.materialized_from,
            cov.materialized_through,
            cov.retrospectively_checked_through,
            cov.scheduled_closure_evaluated_through
        ));
    }
    if let Some(fresh) = &diagnostic.freshness {
        out.push_str(&format!(
            "  freshness: {} (kasi {:?}, full-history {:?}, incremental {:?}, forward {:?})\n",
            if fresh.any_stale() { "stale" } else { "fresh" },
            fresh.kasi_holiday_facts,
            fresh.full_history,
            fresh.incremental,
            fresh.forward_readiness
        ));
    }
    if let Some(day) = diagnostic.target_day {
        match diagnostic.day_status {
            Some(status) => out.push_str(&format!("  day: {day} → {status:?}\n")),
            None => out.push_str(&format!("  day: {day} → (no status)\n")),
        }
    }
    if !diagnostic.alerts.is_empty() {
        out.push_str(&format!("  alerts: {}\n", diagnostic.alerts.len()));
        for message in &diagnostic.alerts {
            out.push_str(&format!("    - {message}\n"));
        }
    }
    out.push_str(&format!("  detail: {}\n", diagnostic.detail));
    out
}

/// Render the diagnostic as stable, pretty JSON. Safe by construction — the record
/// holds only redacted fields.
pub fn render_json(diagnostic: &CalendarDiagnostic) -> String {
    // The record derives `Serialize` over only redacted fields; serialization cannot
    // fail for these plain data types, but fall back defensively rather than panic.
    serde_json::to_string_pretty(diagnostic)
        .unwrap_or_else(|e| format!("{{\"error\":\"diagnostic serialization failed: {e}\"}}"))
}
