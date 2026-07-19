//! U8 composition-root smokes: explicit path resolution → load → injection → startup
//! record → adoption-state reporting, plus the Shadow missing-snapshot degradation
//! contract (KTD8).

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use nautilus_ls::calendar::{
    build_startup_record, resolve_and_load, IngestCalendarContext, LoadedCalendar,
    ResultingAction,
};
use nautilus_ls_calendar::diagnostics::{DiagnosticOutcome, LoadFailure};
use nautilus_ls_calendar::schema::{
    Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness, Snapshot,
    SourceAvailabilityBound,
};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, CalendarAdoption};

/// A short human-shaped authority identity the token heuristic would pass through —
/// the startup record must not leak it.
const SECRET_AUTHORITY: &str = "Jane Doe / Agreement-7";

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn as_of() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap()
}

/// A valid, contiguous, fresh, authorized snapshot written to disk as JSON.
fn write_snapshot(dir: &std::path::Path) -> std::path::PathBuf {
    let snap = stamp(Snapshot {
        schema_version: "1.0.0".to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: None,
        scope: CalendarScope {
            calendar_name: "KRX domestic equity (SYNTHETIC)".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: true,
        },
        authorization: Authorization {
            authorized: true,
            authority: SECRET_AUTHORITY.to_string(),
            granted_at: Utc.with_ymd_and_hms(2010, 1, 1, 0, 0, 0).unwrap(),
            expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: d(2010, 1, 1),
            materialized_through: d(2010, 1, 2),
            retrospectively_checked_through: d(2010, 1, 2),
            scheduled_closure_evaluated_through: d(2010, 1, 2),
            source_availability: vec![SourceAvailabilityBound {
                source_id: "s".to_string(),
                available_from: None,
                available_through: None,
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: Utc.with_ymd_and_hms(2012, 5, 31, 0, 0, 0).unwrap(),
            holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2012, 5, 25, 0, 0, 0).unwrap()),
            full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2012, 4, 1, 0, 0, 0).unwrap()),
            forward_readiness_through: Some(d(2012, 8, 1)),
            last_incremental_at: Some(Utc.with_ymd_and_hms(2012, 5, 31, 0, 0, 0).unwrap()),
        },
        sources: vec![],
        evidence: vec![],
        alerts: vec![],
        rows: vec![
            DayRow {
                date: d(2010, 1, 1),
                status: DayStatus::Closed,
                decisive_evidence: vec![],
                conflicting_evidence: vec![],
                alerts: vec![],
            },
            DayRow {
                date: d(2010, 1, 2),
                status: DayStatus::TradingSession,
                decisive_evidence: vec![],
                conflicting_evidence: vec![],
                alerts: vec![],
            },
        ],
    });
    // The source-availability bound refs a source id that need not resolve to a Source
    // (loader only validates evidence/alert refs, not availability bounds); keep sources
    // empty to keep the fixture minimal.
    let path = dir.join("calendar.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&snap).unwrap()).unwrap();
    path
}

fn stamp(mut snap: Snapshot) -> Snapshot {
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    snap
}

#[test]
fn composition_root_resolves_loads_injects_and_reports_adoption() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_snapshot(dir.path());

    // Explicit path resolution → load → injection.
    let loaded = resolve_and_load(Some(&path), as_of(), CalendarAdoption::Shadow);
    assert!(loaded.is_available(), "a valid snapshot must inject");
    assert!(loaded.calendar().is_some());

    // Startup diagnostic + adoption-state reporting: Shadow over a healthy day records.
    let rec = build_startup_record(
        "smoke",
        CalendarAdoption::Shadow,
        &loaded,
        as_of(),
        d(2010, 1, 1),
    );
    assert_eq!(rec.action, ResultingAction::ShadowRecorded);
    let diag = rec.diagnostic.as_ref().unwrap();
    assert_eq!(diag.outcome, DiagnosticOutcome::Healthy);
    let line = rec.render_line();
    assert!(line.contains("adoption=shadow"), "{line}");
    assert!(line.contains("action=shadow-recorded"), "{line}");
    assert!(line.contains("artifact_id="), "{line}");

    // Redaction holds at the composition level too.
    assert!(!line.contains(SECRET_AUTHORITY), "authority leaked into the startup line: {line}");
    assert!(!line.contains("Jane Doe"), "{line}");

    // Enforced over the same injected calendar reports the calendar as authoritative.
    let enforced = build_startup_record(
        "smoke",
        CalendarAdoption::Enforced,
        &loaded,
        as_of(),
        d(2010, 1, 2),
    );
    assert_eq!(enforced.action, ResultingAction::EnforcedActive);
}

#[test]
fn ingest_context_keeps_one_loaded_snapshot_for_startup_and_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_snapshot(dir.path());
    let context = IngestCalendarContext::resolve(
        Some(path.clone()),
        as_of(),
        CalendarAdoption::Enforced,
    );

    std::fs::remove_file(path).unwrap();

    let startup = context.startup_record("ls-ingest", d(2010, 1, 2));
    assert_eq!(startup.action, ResultingAction::EnforcedActive);
    assert_eq!(context.adoption(), CalendarAdoption::Enforced);
    assert_eq!(context.as_of(), as_of());
    assert_eq!(
        context.view().unwrap().day(d(2010, 1, 2)).unwrap().status,
        DayStatus::TradingSession
    );
}

#[test]
fn shadow_missing_snapshot_starts_clean_and_reports_unavailable() {
    // A path is configured, but NO snapshot exists there.
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.json");

    // Non-fatal: resolve_and_load returns a typed Unavailable, never panics.
    let loaded = resolve_and_load(Some(&missing), as_of(), CalendarAdoption::Shadow);
    match &loaded {
        LoadedCalendar::Unavailable(_) => {}
        other => panic!("expected Unavailable for a missing snapshot, got {other:?}"),
    }
    assert!(!loaded.is_available());

    // The startup record reports Shadow with the weekday path authoritative (non-fatal).
    let rec = build_startup_record(
        "smoke",
        CalendarAdoption::Shadow,
        &loaded,
        as_of(),
        d(2010, 1, 1),
    );
    assert_eq!(rec.action, ResultingAction::ShadowUnavailable);
    let diag = rec.diagnostic.as_ref().unwrap();
    assert_eq!(diag.outcome, DiagnosticOutcome::Load(LoadFailure::Missing));
    let line = rec.render_line();
    assert!(line.contains("action=shadow-unavailable"), "{line}");
    assert!(line.contains("snapshot=unavailable"), "{line}");
}

#[test]
fn no_snapshot_configured_is_not_configured_and_non_fatal() {
    let loaded = resolve_and_load(None, as_of(), CalendarAdoption::Shadow);
    assert!(matches!(loaded, LoadedCalendar::NotConfigured));
    let rec = build_startup_record(
        "smoke",
        CalendarAdoption::Shadow,
        &loaded,
        as_of(),
        d(2010, 1, 1),
    );
    assert_eq!(rec.action, ResultingAction::ShadowUnavailable);
    assert!(rec.diagnostic.is_none());
    assert!(rec.render_line().contains("snapshot=not-configured"));
}
