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

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDate, Utc};

pub use nautilus_ls_calendar::CalendarAdoption;
use nautilus_ls_calendar::{AsOfView, CalendarDiagnostic, CalendarLoadError, KrxCalendar};

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

/// The classified disagreement between a consumer's WEEKDAY decision and the calendar's
/// [`DayStatus`] at a boundary (U3, KTD6). A small closed set so a Shadow window's divergences
/// can be reviewed and signed off before that consumer is enforced (R5, AC7). The axis is
/// "does the weekday path treat the boundary as an open/trading day, and does the calendar
/// agree" — every consumer reduces its decision to that axis (a range/continuity consumer maps
/// "a real session breaks the chain" to a `TradingSession`, "all-closed" to `Closed`,
/// "uncertain" to `Unknown`, "unavailable" to `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceClass {
    /// The weekday and calendar decisions agree.
    Agree,
    /// The calendar proves the day Closed where the weekday path treats it as open — the
    /// safety-relevant case (the weekday path would act on a proven non-session).
    CalendarClosedWeekdayOpen,
    /// The calendar cannot prove the day (Unknown) where the weekday path treats it as open.
    CalendarUnknownWeekdayOpen,
    /// The calendar proves a session where the weekday path treats the day as closed — the
    /// weekday path would have skipped a real session.
    CalendarOpenWeekdayClosed,
    /// The calendar cannot prove the day (Unknown) where the weekday path treats it as closed.
    CalendarUnknownWeekdayClosed,
    /// The calendar is unavailable/indeterminate for the query — no comparison is possible;
    /// the weekday path stays authoritative (non-fatal in Shadow).
    Unavailable,
}

impl DivergenceClass {
    /// The stable token used in the divergence observation line.
    pub fn token(self) -> &'static str {
        match self {
            DivergenceClass::Agree => "agree",
            DivergenceClass::CalendarClosedWeekdayOpen => "calendar-closed-weekday-open",
            DivergenceClass::CalendarUnknownWeekdayOpen => "calendar-unknown-weekday-open",
            DivergenceClass::CalendarOpenWeekdayClosed => "calendar-open-weekday-closed",
            DivergenceClass::CalendarUnknownWeekdayClosed => "calendar-unknown-weekday-closed",
            DivergenceClass::Unavailable => "unavailable",
        }
    }

    /// `true` iff the weekday and calendar decisions actually disagree (everything but
    /// [`Agree`](DivergenceClass::Agree)). [`Unavailable`](DivergenceClass::Unavailable) counts
    /// as a divergence to review — the calendar could not confirm the weekday call.
    pub fn is_divergent(self) -> bool {
        !matches!(self, DivergenceClass::Agree)
    }
}

/// Classify the disagreement between a weekday "is this an open/trading day" decision and the
/// calendar's tri-state [`DayStatus`] (`None` = the calendar was unavailable/out-of-range for
/// the query). The single source of truth every consumer boundary reduces its decision to.
pub fn classify_divergence(
    weekday_open: bool,
    calendar: Option<nautilus_ls_calendar::schema::DayStatus>,
) -> DivergenceClass {
    use nautilus_ls_calendar::schema::DayStatus;
    match calendar {
        None => DivergenceClass::Unavailable,
        Some(DayStatus::TradingSession) => {
            if weekday_open {
                DivergenceClass::Agree
            } else {
                DivergenceClass::CalendarOpenWeekdayClosed
            }
        }
        Some(DayStatus::Closed) => {
            if weekday_open {
                DivergenceClass::CalendarClosedWeekdayOpen
            } else {
                DivergenceClass::Agree
            }
        }
        Some(DayStatus::Unknown) => {
            if weekday_open {
                DivergenceClass::CalendarUnknownWeekdayOpen
            } else {
                DivergenceClass::CalendarUnknownWeekdayClosed
            }
        }
    }
}

/// A structured, testable Shadow-divergence observation for one consumer boundary (KTD6):
/// the consumer, the boundary civil date, the human renderings of both decisions, and the
/// classified [`DivergenceClass`]. Redacted by construction — it carries only calendar
/// decisions and a date (never an authority/credential identity) — and non-persisted: it is
/// emitted on the stderr diagnostic channel, never into checkpoint/watermark state or a stdout
/// data product, so Shadow stays byte-identical to Legacy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivergenceObservation {
    /// Which consumer boundary observed the divergence.
    pub consumer: String,
    /// The boundary civil date the decisions bear on.
    pub date: NaiveDate,
    /// The weekday path's decision, human-rendered (e.g. `open`/`closed`, or a debug rendering).
    pub weekday_decision: String,
    /// The calendar's decision, human-rendered (e.g. a `DayStatus`, or `unavailable`).
    pub calendar_decision: String,
    /// The classified relationship between the two decisions.
    pub class: DivergenceClass,
}

impl DivergenceObservation {
    /// Build an observation, classifying the weekday-vs-calendar decision. `weekday_open` is
    /// the weekday path's open/trading verdict for `date`; `calendar` is the calendar's tri-state
    /// verdict (`None` when unavailable/out-of-range).
    pub fn new(
        consumer: &str,
        date: NaiveDate,
        weekday_open: bool,
        calendar: Option<nautilus_ls_calendar::schema::DayStatus>,
    ) -> Self {
        Self {
            consumer: consumer.to_string(),
            date,
            weekday_decision: if weekday_open { "open".to_string() } else { "closed".to_string() },
            calendar_decision: match calendar {
                Some(status) => format!("{status:?}"),
                None => "unavailable".to_string(),
            },
            class: classify_divergence(weekday_open, calendar),
        }
    }

    /// Render the observation as ONE concise, redacted line for the diagnostic channel.
    pub fn render_line(&self) -> String {
        format!(
            "calendar-divergence consumer={} date={} weekday={} calendar={} class={}",
            self.consumer,
            self.date,
            self.weekday_decision,
            self.calendar_decision,
            self.class.token()
        )
    }
}

/// Emit a Shadow-divergence observation to the non-persisted diagnostic channel (stderr, KTD6).
/// Like [`emit_startup_record`], a Shadow recording never touches stdout or a tracked artifact,
/// so the byte-identical-to-Legacy guarantee holds — this is the durable review corpus each
/// Consumer Retirement Gate signs off against once the operator captures the channel (U5).
pub fn emit_divergence(observation: &DivergenceObservation) {
    eprintln!("{}", observation.render_line());
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
    let context = IngestCalendarContext::from_env(as_of);
    // The date the process cares about at startup: today's civil date in KST (KST = UTC+9,
    // no DST). Consumers that care about a specific other date compute it themselves in
    // Phase C; the startup record just needs a representative in-scope target.
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

    #[test]
    fn divergence_observation_is_redacted_and_classified() {
        use nautilus_ls_calendar::schema::DayStatus;

        // The full classification matrix over the (weekday_open, calendar) axis.
        assert_eq!(
            classify_divergence(true, Some(DayStatus::Closed)),
            DivergenceClass::CalendarClosedWeekdayOpen
        );
        assert_eq!(
            classify_divergence(true, Some(DayStatus::Unknown)),
            DivergenceClass::CalendarUnknownWeekdayOpen
        );
        assert_eq!(
            classify_divergence(false, Some(DayStatus::TradingSession)),
            DivergenceClass::CalendarOpenWeekdayClosed
        );
        assert_eq!(
            classify_divergence(false, Some(DayStatus::Unknown)),
            DivergenceClass::CalendarUnknownWeekdayClosed
        );
        assert_eq!(
            classify_divergence(true, Some(DayStatus::TradingSession)),
            DivergenceClass::Agree
        );
        assert_eq!(
            classify_divergence(false, Some(DayStatus::Closed)),
            DivergenceClass::Agree
        );
        assert_eq!(classify_divergence(true, None), DivergenceClass::Unavailable);

        // The observation renders a concise, classified, redacted line — a calendar-closed
        // day the weekday path treats as open is the safety-relevant divergence.
        let obs = DivergenceObservation::new(
            "unit-test",
            NaiveDate::from_ymd_opt(2011, 9, 21).unwrap(),
            true,
            Some(DayStatus::Closed),
        );
        assert_eq!(obs.class, DivergenceClass::CalendarClosedWeekdayOpen);
        assert!(obs.class.is_divergent());
        let line = obs.render_line();
        assert!(line.contains("class=calendar-closed-weekday-open"), "{line}");
        assert!(line.contains("consumer=unit-test date=2011-09-21"), "{line}");
        // Redacted by construction: the observation has no authority/credential field, so no
        // identity can appear in either the struct or its render line.
        assert!(!line.to_lowercase().contains("authority"), "{line}");
    }
}
