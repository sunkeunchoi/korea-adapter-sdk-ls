//! Composition-root calendar resolution + the mandatory startup record (U8, KTD8/KTD9).
//!
//! This is the ONLY place that turns an explicit snapshot path + adoption state into
//! an injected [`KrxCalendar`] (or a recorded unavailable state). The calendar core
//! never reads env or picks a default path (KTD5) — that resolution lives here, at the
//! composition root each affected process calls at startup.
//!
//! ## Fail-closed contract (KTD8)
//!
//! After the #189 weekday retirement the sole adoption posture is
//! [`Enforced`](CalendarAdoption::Enforced): the calendar is authoritative and there is
//! no weekday fallback. [`resolve_and_load`] still SURFACES a typed result for every
//! case — a missing path is [`NotConfigured`], a load failure is [`Unavailable`] — and
//! the consumer decides what to do; under Enforced an unusable calendar fails closed.
//!
//! The startup record is emitted to a NON-PERSISTED diagnostic channel (stderr), never a
//! tracked/persisted artifact. Every field is REDACTED via the calendar crate's
//! [`diagnostics`](nautilus_ls_calendar::diagnostics) layer — no credential or
//! authorization identity is ever printed.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDate, Utc};

pub use nautilus_ls_calendar::CalendarAdoption;
use nautilus_ls_calendar::{AsOfView, CalendarDiagnostic, CalendarLoadError, KrxCalendar};

/// The env var naming the explicit snapshot path (composition-root input, KTD5). Unset
/// or empty means "no snapshot configured" — the Shadow degradation contract applies.
pub const SNAPSHOT_PATH_ENV: &str = "LS_CALENDAR_SNAPSHOT";

/// The env var naming the per-process adoption state. Unset/invalid → the default
/// [`CalendarAdoption::Enforced`] (the only posture after the #189 weekday retirement, KTD8).
pub const ADOPTION_ENV: &str = "LS_CALENDAR_ADOPTION";

/// The outcome of resolving + loading a calendar at the composition root.
///
/// This is what each calendar consumer injects: either an [`Available`] calendar or a
/// recorded non-fatal unavailable state. The helper never panics — a load failure is a
/// typed value, not a crash.
#[derive(Debug)]
pub enum LoadedCalendar {
    /// The snapshot loaded and validated at the as-of instant — inject this calendar.
    Available(KrxCalendar),
    /// A snapshot path was configured but loading failed — the typed reason is retained
    /// (the consumer fails closed under Enforced).
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

/// One immutable calendar resolution for an `ls-ingest` invocation.
///
/// The composition root fixes the clock, adoption posture, explicit source path, and
/// loaded snapshot once. Startup admission and every runtime policy view are then
/// derived from this owned value; replacing the source file cannot change the run.
#[derive(Debug)]
pub struct IngestCalendarContext {
    as_of: DateTime<Utc>,
    adoption: CalendarAdoption,
    loaded: LoadedCalendar,
}

impl IngestCalendarContext {
    pub fn resolve(
        snapshot_path: Option<PathBuf>,
        as_of: DateTime<Utc>,
        adoption: CalendarAdoption,
    ) -> Self {
        let loaded = resolve_and_load(snapshot_path.as_deref(), as_of, adoption);
        Self { as_of, adoption, loaded }
    }

    pub fn from_env(as_of: DateTime<Utc>) -> Self {
        Self::resolve(snapshot_path_from_env(), as_of, adoption_from_env())
    }

    pub fn as_of(&self) -> DateTime<Utc> {
        self.as_of
    }

    pub fn adoption(&self) -> CalendarAdoption {
        self.adoption
    }

    pub fn view(&self) -> Option<AsOfView<'_>> {
        self.loaded.calendar().and_then(|calendar| calendar.as_of(self.as_of).ok())
    }

    pub fn startup_record(&self, consumer: &str, target: NaiveDate) -> StartupRecord {
        build_startup_record(consumer, self.adoption, &self.loaded, self.as_of, target)
    }
}

/// Resolve an EXPLICIT snapshot path and load the calendar at `as_of` (KTD5). Returns a
/// typed [`LoadedCalendar`] for every case — a missing path is [`NotConfigured`], a load
/// failure is [`Unavailable`], a success is [`Available`]. Never reads env, never picks a
/// default path, never panics.
///
/// `adoption` is threaded uniformly as the recorded posture (see [`CalendarAdoption`]); it
/// does not affect the load — the helper only SURFACES the typed result, and the consumer
/// decides. Under the sole surviving Enforced posture an unavailable state is fatal (the
/// consumer fails closed).
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

/// The action a process takes as a result of the calendar resolution. Recorded in the
/// startup record; the consumer acts on the injected calendar (Enforced-only, #189).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultingAction {
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
    // The sole surviving posture after the #189 weekday retirement.
    let CalendarAdoption::Enforced = adoption;
    if available {
        ResultingAction::EnforcedActive
    } else {
        ResultingAction::EnforcedFailClosed
    }
}

impl ResultingAction {
    /// The stable token used in the startup record line.
    pub fn token(self) -> &'static str {
        match self {
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

/// Emit the startup record to the non-persisted diagnostic channel (stderr, KTD8). The
/// record never touches stdout or a tracked artifact — it is redacted diagnostics only,
/// so it cannot perturb a consumer's data product.
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

/// Read the adoption state from [`ADOPTION_ENV`], defaulting to
/// [`CalendarAdoption::Enforced`] — the only posture after the #189 weekday retirement (KTD8).
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
    let context = IngestCalendarContext::from_env(as_of);
    // The date the process cares about at startup: today's civil date in KST (KST = UTC+9,
    // no DST). Consumers that care about a specific other date compute it themselves; the
    // startup record just needs a representative in-scope target.
    let target = (as_of + Duration::hours(9)).date_naive();
    context.startup_record(consumer, target)
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
    fn missing_path_is_not_configured_and_fails_closed_under_enforced() {
        let rec = build_startup_record(
            "unit-test",
            CalendarAdoption::Enforced,
            &LoadedCalendar::NotConfigured,
            Utc::now(),
            Utc::now().date_naive(),
        );
        assert_eq!(rec.action, ResultingAction::EnforcedFailClosed);
        assert!(rec.diagnostic.is_none());
        assert!(rec.render_line().contains("snapshot=not-configured"));
    }

    #[test]
    fn unavailable_fails_closed_under_enforced() {
        let loaded = LoadedCalendar::Unavailable(CalendarLoadError::Missing);
        let now = Utc::now();
        let enforced =
            build_startup_record("t", CalendarAdoption::Enforced, &loaded, now, now.date_naive());
        assert_eq!(enforced.action, ResultingAction::EnforcedFailClosed);
    }

    #[test]
    fn out_of_range_query_records_as_unavailable_outcome() {
        // A diagnostic whose outcome is OutOfRange is not a usable factual day, so the
        // resulting action is the fail-closed branch even though a snapshot loaded.
        assert!(!DiagnosticOutcome::OutOfRange.is_usable());
    }
}
