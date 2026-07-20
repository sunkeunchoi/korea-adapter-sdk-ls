//! Composition-root calendar resolution + the mandatory startup record (U8, KTD8/KTD9).
//!
//! This is the ONLY place that turns an explicit snapshot path + adoption state into
//! an injected [`KrxCalendar`] (or a recorded unavailable state). The calendar core
//! never reads env or picks a default path (KTD5) — that resolution lives here, at the
//! composition root each affected process calls at startup.
//!
//! ## Shadow degradation contract (KTD8)
//!
//! At slice-deploy time the production snapshot is deferred (a maintainer U14/U15 run),
//! so the configured path is normally ABSENT. In [`Shadow`](CalendarAdoption::Shadow)
//! (and [`Legacy`](CalendarAdoption::Legacy)) a missing path or ANY typed
//! [`CalendarLoadError`] is NON-FATAL: [`resolve_and_load`] returns a recorded
//! unavailable state, the weekday path stays authoritative, and the process starts
//! cleanly. Only [`Enforced`](CalendarAdoption::Enforced) fails closed — and even then
//! this helper just SURFACES the typed result; the consumer (U9–U13) decides.
//!
//! The startup record is emitted to a NON-PERSISTED diagnostic channel (stderr) so a
//! Shadow recording never touches a tracked/persisted artifact a Legacy reader consumes
//! (KTD8 byte-identical guarantee). Every field is REDACTED via the calendar crate's
//! [`diagnostics`](nautilus_ls_calendar::diagnostics) layer — no credential or
//! authorization identity is ever printed.

use std::path::Path;

use chrono::{DateTime, Duration, NaiveDate, Utc};

pub use nautilus_ls_calendar::CalendarAdoption;
use nautilus_ls_calendar::{CalendarDiagnostic, CalendarLoadError, KrxCalendar};

/// The env var naming the explicit snapshot path (composition-root input, KTD5). Unset
/// or empty means "no snapshot configured" — the Shadow degradation contract applies.
pub const SNAPSHOT_PATH_ENV: &str = "LS_CALENDAR_SNAPSHOT";

/// The env var naming the per-process adoption state. Unset/invalid → the composed
/// default [`CalendarAdoption::Shadow`] (KTD8).
pub const ADOPTION_ENV: &str = "LS_CALENDAR_ADOPTION";

/// The outcome of resolving + loading a calendar at the composition root.
///
/// This is what the Phase C consumers (U9–U13) inject: either an [`Available`] calendar
/// or a recorded non-fatal unavailable state. The helper never panics — a load failure
/// is a typed value, not a crash.
#[derive(Debug)]
pub enum LoadedCalendar {
    /// The snapshot loaded and validated at the as-of instant — inject this calendar.
    Available(KrxCalendar),
    /// A snapshot path was configured but loading failed — the typed reason is retained
    /// (non-fatal in Shadow/Legacy; the consumer fails closed in Enforced).
    Unavailable(CalendarLoadError),
    /// No snapshot path was configured at all (the normal slice-deploy state).
    NotConfigured,
}

impl LoadedCalendar {
    /// The injected calendar, if one loaded.
    pub fn calendar(&self) -> Option<&KrxCalendar> {
        match self {
            LoadedCalendar::Available(cal) => Some(cal),
            _ => None,
        }
    }

    /// Whether a usable calendar was injected.
    pub fn is_available(&self) -> bool {
        matches!(self, LoadedCalendar::Available(_))
    }
}

/// Resolve an EXPLICIT snapshot path and load the calendar at `as_of` (KTD5). Returns a
/// typed [`LoadedCalendar`] for every case — a missing path is [`NotConfigured`], a load
/// failure is [`Unavailable`], a success is [`Available`]. Never reads env, never picks a
/// default path, never panics.
///
/// `adoption` is accepted so the composition root's intent is explicit at the call site
/// (and so this signature matches what Phase C consumers wire); the load itself is
/// adoption-independent — the helper only SURFACES the typed result, and the consumer
/// decides whether an unavailable state is fatal (Enforced) or not (Shadow/Legacy).
pub fn resolve_and_load(
    path: Option<&Path>,
    as_of: DateTime<Utc>,
    adoption: CalendarAdoption,
) -> LoadedCalendar {
    // Adoption does not change the load; it is recorded by the caller's startup record.
    let _ = adoption;
    match path {
        None => LoadedCalendar::NotConfigured,
        Some(path) => match KrxCalendar::load_from_path(path, as_of) {
            Ok(cal) => LoadedCalendar::Available(cal),
            Err(err) => LoadedCalendar::Unavailable(err),
        },
    }
}

/// The action a process takes as a result of the calendar resolution + adoption state.
/// Recorded in the startup record; drives nothing in this unit (the decision migrations
/// are U9–U13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultingAction {
    /// Legacy: the weekday path is authoritative; the calendar is not consulted.
    WeekdayAuthoritative,
    /// Shadow: the calendar decision is recorded; the weekday path stays authoritative.
    ShadowRecorded,
    /// Shadow/Legacy: the calendar is unavailable (non-fatal); the weekday path acts.
    ShadowUnavailable,
    /// Enforced: the calendar is authoritative.
    EnforcedActive,
    /// Enforced: the calendar failed to load → fail closed (the consumer refuses).
    EnforcedFailClosed,
}

/// The canonical `(adoption, calendar-usable)` → [`ResultingAction`] mapping. This is the
/// SINGLE source of truth for the resulting action: [`build_startup_record_targeted`] derives
/// it from a loaded calendar's usability, and offline seams (e.g. the dispatch gate's stubbed
/// date fact) call it directly so a stub-built record cannot drift from a real one.
pub fn resulting_action(adoption: CalendarAdoption, available: bool) -> ResultingAction {
    match (adoption, available) {
        (CalendarAdoption::Legacy, _) => ResultingAction::WeekdayAuthoritative,
        (CalendarAdoption::Shadow, true) => ResultingAction::ShadowRecorded,
        (CalendarAdoption::Shadow, false) => ResultingAction::ShadowUnavailable,
        (CalendarAdoption::Enforced, true) => ResultingAction::EnforcedActive,
        (CalendarAdoption::Enforced, false) => ResultingAction::EnforcedFailClosed,
    }
}

impl ResultingAction {
    /// The stable token used in the startup record line.
    pub fn token(self) -> &'static str {
        match self {
            ResultingAction::WeekdayAuthoritative => "weekday-authoritative",
            ResultingAction::ShadowRecorded => "shadow-recorded",
            ResultingAction::ShadowUnavailable => "shadow-unavailable",
            ResultingAction::EnforcedActive => "enforced-active",
            ResultingAction::EnforcedFailClosed => "enforced-fail-closed",
        }
    }
}

/// The concise, mandatory startup calendar record (KTD8). Names the consumer, adoption
/// state, snapshot identities, authorization state, coverage, freshness, query result,
/// alerts, and resulting action — ALL redacted (the [`CalendarDiagnostic`] is redacted by
/// construction). `diagnostic` is `None` only when no snapshot was configured.
#[derive(Debug, Clone)]
pub struct StartupRecord {
    /// Which process/consumer emitted this record.
    pub consumer: String,
    /// The adoption state this process runs under.
    pub adoption: CalendarAdoption,
    /// The redacted diagnostic, or `None` when no snapshot path was configured.
    pub diagnostic: Option<CalendarDiagnostic>,
    /// The action the process takes as a result.
    pub action: ResultingAction,
}

impl StartupRecord {
    /// Render the record as ONE concise, redacted line for the diagnostic channel.
    pub fn render_line(&self) -> String {
        let mut parts = vec![
            "calendar-startup".to_string(),
            format!("consumer={}", self.consumer),
            format!("adoption={}", self.adoption.as_str()),
        ];
        match &self.diagnostic {
            None => parts.push("snapshot=not-configured".to_string()),
            Some(diag) => {
                match (&diag.artifact_id, &diag.calendar_id) {
                    (Some(a), Some(c)) => {
                        parts.push(format!("artifact_id={a}"));
                        parts.push(format!("calendar_id={c}"));
                    }
                    _ => parts.push("snapshot=unavailable".to_string()),
                }
                if let Some(auth) = &diag.authorization {
                    parts.push(format!(
                        "auth={}",
                        if auth.authorized { "authorized" } else { "unauthorized" }
                    ));
                }
                if let Some(cov) = &diag.coverage {
                    parts.push(format!(
                        "coverage={}..{}",
                        cov.materialized_from, cov.materialized_through
                    ));
                }
                if let Some(fresh) = &diag.freshness {
                    parts.push(format!(
                        "freshness={}",
                        if fresh.any_stale() { "stale" } else { "fresh" }
                    ));
                }
                match (diag.target_day, diag.day_status) {
                    (Some(day), Some(status)) => parts.push(format!("day={day}:{status:?}")),
                    (Some(day), None) => parts.push(format!("day={day}:none")),
                    _ => {}
                }
                parts.push(format!("outcome={}", diag.outcome.token()));
                parts.push(format!("alerts={}", diag.alerts.len()));
            }
        }
        parts.push(format!("action={}", self.action.token()));
        parts.join(" ")
    }
}

/// Build the startup record from a resolution, an as-of instant, and the civil date the
/// process cares about (KTD8). The diagnostic is built REDACTED; the resulting action is
/// derived from the adoption state + whether a usable calendar was injected.
pub fn build_startup_record(
    consumer: &str,
    adoption: CalendarAdoption,
    loaded: &LoadedCalendar,
    as_of: DateTime<Utc>,
    target: NaiveDate,
) -> StartupRecord {
    build_startup_record_targeted(consumer, adoption, loaded, as_of, Some(target))
}

/// Build the startup record with an OPTIONAL decision target (KTD2). When `target` is
/// `Some(date)`, the diagnostic queries that day and the record carries `day=<date>:<status>`;
/// when `target` is `None`, the diagnostic is a posture/coverage summary with no `day=`
/// marker — the honest representation for a consumer that resolves no single decision date
/// (a probe that refused to select any day rather than a misleading anchor). A `None` target
/// never yields an `OutOfRange`, so a posture-only startup over a usable calendar reports the
/// calendar as available.
pub fn build_startup_record_targeted(
    consumer: &str,
    adoption: CalendarAdoption,
    loaded: &LoadedCalendar,
    as_of: DateTime<Utc>,
    target: Option<NaiveDate>,
) -> StartupRecord {
    let (diagnostic, available) = match loaded {
        LoadedCalendar::Available(cal) => match cal.as_of(as_of) {
            Ok(view) => {
                let diag = match target {
                    Some(t) => CalendarDiagnostic::from_view(&view, t),
                    None => CalendarDiagnostic::from_view_untargeted(&view),
                };
                // A usable calendar is one that loaded AND resolves a factual day (a
                // successful Healthy/Stale/Unknown/Conflict outcome). An out-of-range
                // query at startup is a not-yet-usable state.
                let usable = diag.outcome.is_usable();
                (Some(diag), usable)
            }
            Err(err) => (Some(CalendarDiagnostic::from_load_error(as_of, &err)), false),
        },
        LoadedCalendar::Unavailable(err) => {
            (Some(CalendarDiagnostic::from_load_error(as_of, err)), false)
        }
        LoadedCalendar::NotConfigured => (None, false),
    };

    let action = resulting_action(adoption, available);

    StartupRecord {
        consumer: consumer.to_string(),
        adoption,
        diagnostic,
        action,
    }
}

/// Emit the startup record to the non-persisted diagnostic channel (stderr, KTD8). A
/// Shadow/Legacy recording never touches stdout or a tracked artifact, so the
/// byte-identical-to-Legacy guarantee the consumer suites (U9–U13) rely on holds.
pub fn emit_startup_record(record: &StartupRecord) {
    eprintln!("{}", record.render_line());
}

/// Read the explicit snapshot path from [`SNAPSHOT_PATH_ENV`] (`None` when unset/empty).
pub fn snapshot_path_from_env() -> Option<std::path::PathBuf> {
    std::env::var(SNAPSHOT_PATH_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(std::path::PathBuf::from)
}

/// Read the adoption state from [`ADOPTION_ENV`], defaulting to the composed default
/// [`CalendarAdoption::Shadow`] (KTD8).
pub fn adoption_from_env() -> CalendarAdoption {
    std::env::var(ADOPTION_ENV)
        .ok()
        .and_then(|v| CalendarAdoption::parse(&v))
        .unwrap_or_default()
}

/// The composition-root convenience each affected process calls at startup: resolve the
/// env-configured path + adoption, load at "now", target the current KST civil date, and
/// build the startup record. The core still reads no env — this wrapper (the composition
/// root) does, then hands explicit values to [`resolve_and_load`] / [`build_startup_record`].
pub fn startup_from_env(consumer: &str) -> StartupRecord {
    let as_of = Utc::now();
    let adoption = adoption_from_env();
    let path = snapshot_path_from_env();
    // The date the process cares about at startup: today's civil date in KST (KST = UTC+9,
    // no DST). Consumers that care about a specific other date compute it themselves in
    // Phase C; the startup record just needs a representative in-scope target.
    let target = (as_of + Duration::hours(9)).date_naive();
    let loaded = resolve_and_load(path.as_deref(), as_of, adoption);
    build_startup_record(consumer, adoption, &loaded, as_of, target)
}

/// The composition-root one-liner: build the env-driven startup record and emit it. Every
/// affected process calls this once, right after credential-hygiene install.
pub fn emit_startup_from_env(consumer: &str) {
    emit_startup_record(&startup_from_env(consumer));
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_ls_calendar::DiagnosticOutcome;

    #[test]
    fn missing_path_is_not_configured_and_non_fatal_in_shadow() {
        let rec = build_startup_record(
            "unit-test",
            CalendarAdoption::Shadow,
            &LoadedCalendar::NotConfigured,
            Utc::now(),
            Utc::now().date_naive(),
        );
        assert_eq!(rec.action, ResultingAction::ShadowUnavailable);
        assert!(rec.diagnostic.is_none());
        assert!(rec.render_line().contains("snapshot=not-configured"));
    }

    #[test]
    fn unavailable_maps_to_shadow_unavailable_but_enforced_fail_closed() {
        let loaded = LoadedCalendar::Unavailable(CalendarLoadError::Missing);
        let now = Utc::now();
        let shadow = build_startup_record("t", CalendarAdoption::Shadow, &loaded, now, now.date_naive());
        assert_eq!(shadow.action, ResultingAction::ShadowUnavailable);
        let enforced =
            build_startup_record("t", CalendarAdoption::Enforced, &loaded, now, now.date_naive());
        assert_eq!(enforced.action, ResultingAction::EnforcedFailClosed);
    }

    #[test]
    fn out_of_range_query_records_as_unavailable_outcome() {
        // A diagnostic whose outcome is OutOfRange is not a usable factual day, so the
        // resulting action is the unavailable branch even though a snapshot loaded.
        assert!(!DiagnosticOutcome::OutOfRange.is_usable());
    }
}
