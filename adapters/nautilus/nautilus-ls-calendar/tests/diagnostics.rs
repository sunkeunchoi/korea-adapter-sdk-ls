//! U8 diagnostic contract: stable human + JSON rendering for every outcome, and
//! the field-level redaction guarantee (AC10).
//!
//! The diagnostic record is built REDACTED by construction — these tests prove no
//! credential or authorization identity ever reaches either render form, including a
//! maintainer/agreement identity shaped to defeat the `scrub.rs` token heuristic.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use nautilus_ls_calendar::diagnostics::{
    render_human, render_json, CalendarDiagnostic, DiagnosticOutcome, LoadFailure,
};
use nautilus_ls_calendar::schema::{
    Alert, AlertKind, Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness,
    Snapshot, Source, SourceAvailabilityBound, SourceKind,
};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, CalendarLoadError, KrxCalendar};

/// A short, human-shaped authorization identity that carries NO 6+-digit run and is
/// under 20 alphanumeric chars — so the `scrub.rs` token heuristic would pass it
/// straight through. Field-level redaction must drop it anyway.
const SECRET_AUTHORITY: &str = "Jane Doe / Agreement-7";

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn as_of() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap()
}

fn fresh_freshness() -> Freshness {
    Freshness {
        evidence_refreshed_at: Utc.with_ymd_and_hms(2012, 5, 31, 0, 0, 0).unwrap(),
        holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2012, 5, 25, 0, 0, 0).unwrap()),
        full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2012, 4, 1, 0, 0, 0).unwrap()),
        forward_readiness_through: Some(d(2012, 8, 1)),
        last_incremental_at: Some(Utc.with_ymd_and_hms(2012, 5, 31, 0, 0, 0).unwrap()),
    }
}

fn stale_freshness() -> Freshness {
    Freshness {
        evidence_refreshed_at: Utc.with_ymd_and_hms(2012, 1, 1, 0, 0, 0).unwrap(),
        // Holiday facts checked 5 months ago → past the 14-day threshold → stale.
        holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2012, 1, 1, 0, 0, 0).unwrap()),
        full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2012, 4, 1, 0, 0, 0).unwrap()),
        forward_readiness_through: Some(d(2012, 8, 1)),
        last_incremental_at: Some(Utc.with_ymd_and_hms(2012, 5, 31, 0, 0, 0).unwrap()),
    }
}

/// A contiguous 2010-01-01..2010-01-04 snapshot: Closed, Closed, Unknown,
/// TradingSession(+alert). The last row carries a reconciliation alert → a retained
/// conflict.
fn snapshot(freshness: Freshness, expires_at: DateTime<Utc>) -> Snapshot {
    let snap = Snapshot {
        schema_version: "1.0.0".to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: None,
        scope: CalendarScope {
            calendar_name: "KRX domestic equity regular session (SYNTHETIC)".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: true,
        },
        authorization: Authorization {
            authorized: true,
            authority: SECRET_AUTHORITY.to_string(),
            granted_at: Utc.with_ymd_and_hms(2010, 1, 1, 0, 0, 0).unwrap(),
            expires_at: Some(expires_at),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: d(2010, 1, 1),
            materialized_through: d(2010, 1, 4),
            retrospectively_checked_through: d(2010, 1, 4),
            scheduled_closure_evaluated_through: d(2010, 1, 4),
            source_availability: vec![SourceAvailabilityBound {
                source_id: "krx-daily".to_string(),
                available_from: Some(d(2010, 1, 4)),
                available_through: Some(d(2010, 1, 4)),
            }],
        },
        freshness,
        sources: vec![Source {
            id: "krx-daily".to_string(),
            kind: SourceKind::KrxDailyMarket,
            // A source label that also carries a maintainer name — must not leak.
            label: format!("KRX stk_bydd_trd maintained by {SECRET_AUTHORITY}"),
            synthetic: true,
        }],
        evidence: vec![],
        alerts: vec![Alert {
            id: "al-1".to_string(),
            date: d(2010, 1, 4),
            kind: AlertKind::WitnessOverridesInference,
            message: "positive witness overrides inferred closure".to_string(),
        }],
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
                status: DayStatus::Closed,
                decisive_evidence: vec![],
                conflicting_evidence: vec![],
                alerts: vec![],
            },
            DayRow {
                date: d(2010, 1, 3),
                status: DayStatus::Unknown,
                decisive_evidence: vec![],
                conflicting_evidence: vec![],
                alerts: vec![],
            },
            DayRow {
                date: d(2010, 1, 4),
                status: DayStatus::TradingSession,
                decisive_evidence: vec![],
                conflicting_evidence: vec![],
                alerts: vec!["al-1".to_string()],
            },
        ],
    };
    stamp(snap)
}

fn stamp(mut snap: Snapshot) -> Snapshot {
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    snap
}

fn calendar(freshness: Freshness) -> KrxCalendar {
    let expires = Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap();
    KrxCalendar::from_snapshot(snapshot(freshness, expires), as_of()).expect("fixture loads")
}

/// Both render forms of a diagnostic, concatenated — every leak assertion runs
/// against BOTH surfaces.
fn both_forms(diagnostic: &CalendarDiagnostic) -> String {
    format!("{}\n{}", render_human(diagnostic), render_json(diagnostic))
}

// ---------------------------------------------------------------------------
// Outcome contract: a stable human + JSON rendering for each case.
// ---------------------------------------------------------------------------

#[test]
fn healthy_case_renders_stably() {
    let cal = calendar(fresh_freshness());
    let view = cal.as_of(as_of()).unwrap();
    let diag = CalendarDiagnostic::from_view(&view, d(2010, 1, 1));
    assert_eq!(diag.outcome, DiagnosticOutcome::Healthy);
    let text = render_human(&diag);
    assert!(text.contains("calendar diagnostic"), "{text}");
    assert!(text.contains("outcome: healthy"), "{text}");
    let json = render_json(&diag);
    assert!(json.contains("\"outcome\": \"healthy\""), "{json}");
    assert!(json.contains("\"day_status\": \"closed\""), "{json}");
}

#[test]
fn stale_case_is_stale_not_a_status_rewrite() {
    let cal = calendar(stale_freshness());
    let view = cal.as_of(as_of()).unwrap();
    let diag = CalendarDiagnostic::from_view(&view, d(2010, 1, 1));
    assert_eq!(diag.outcome, DiagnosticOutcome::Stale);
    // Staleness never rewrites the day status (AC8).
    assert_eq!(diag.day_status, Some(DayStatus::Closed));
    assert!(render_human(&diag).contains("outcome: stale"));
}

#[test]
fn unknown_case_reports_unknown() {
    let cal = calendar(fresh_freshness());
    let view = cal.as_of(as_of()).unwrap();
    let diag = CalendarDiagnostic::from_view(&view, d(2010, 1, 3));
    assert_eq!(diag.outcome, DiagnosticOutcome::Unknown);
    assert_eq!(diag.day_status, Some(DayStatus::Unknown));
}

#[test]
fn conflict_case_reports_conflict_with_alerts() {
    let cal = calendar(fresh_freshness());
    let view = cal.as_of(as_of()).unwrap();
    let diag = CalendarDiagnostic::from_view(&view, d(2010, 1, 4));
    assert_eq!(diag.outcome, DiagnosticOutcome::Conflict);
    assert_eq!(diag.alerts.len(), 1);
    assert!(render_human(&diag).contains("alerts: 1"));
}

#[test]
fn out_of_range_case_reports_out_of_range() {
    let cal = calendar(fresh_freshness());
    let view = cal.as_of(as_of()).unwrap();
    let diag = CalendarDiagnostic::from_view(&view, d(2011, 1, 1));
    assert_eq!(diag.outcome, DiagnosticOutcome::OutOfRange);
    assert!(diag.day_status.is_none());
}

#[test]
fn load_failure_cases_each_map_to_a_typed_outcome() {
    let cases = [
        (CalendarLoadError::Missing, LoadFailure::Missing),
        (
            CalendarLoadError::Unreadable { message: "io".into() },
            LoadFailure::Unreadable,
        ),
        (
            CalendarLoadError::Corrupt { message: "bad json".into() },
            LoadFailure::Corrupt,
        ),
        (
            CalendarLoadError::UnsupportedSchema { found: "9.0.0".into() },
            LoadFailure::Incompatible,
        ),
        (
            CalendarLoadError::HashMismatch { field: "artifact_id".into() },
            LoadFailure::Integrity,
        ),
        (CalendarLoadError::Unauthorized, LoadFailure::Unauthorized),
        (CalendarLoadError::Expired, LoadFailure::Expired),
        (
            CalendarLoadError::Gapped { date: d(2010, 1, 3) },
            LoadFailure::Coverage,
        ),
    ];
    for (err, expected) in cases {
        let diag = CalendarDiagnostic::from_load_error(as_of(), &err);
        assert_eq!(
            diag.outcome,
            DiagnosticOutcome::Load(expected),
            "{err:?} should classify as {expected:?}"
        );
        // A load failure has no snapshot facts and prints stably in both forms.
        assert!(diag.artifact_id.is_none());
        assert!(render_human(&diag).contains("snapshot: unavailable"));
        assert!(render_json(&diag).contains("\"outcome\""));
    }
}

#[test]
fn incompatible_schema_names_the_version_but_stays_typed() {
    let err = CalendarLoadError::UnsupportedSchema { found: "9.0.0".into() };
    let diag = CalendarDiagnostic::from_load_error(as_of(), &err);
    assert_eq!(diag.outcome, DiagnosticOutcome::Load(LoadFailure::Incompatible));
    assert!(diag.detail.contains("9.0.0"));
}

// ---------------------------------------------------------------------------
// Redaction: no credential / authorization identity in EITHER form.
// ---------------------------------------------------------------------------

#[test]
fn view_diagnostic_never_leaks_the_authority_identity() {
    let cal = calendar(fresh_freshness());
    let view = cal.as_of(as_of()).unwrap();
    let diag = CalendarDiagnostic::from_view(&view, d(2010, 1, 4));
    let combined = both_forms(&diag);
    assert!(
        !combined.contains(SECRET_AUTHORITY),
        "authority identity leaked into a diagnostic form:\n{combined}"
    );
    assert!(!combined.contains("Jane Doe"), "maintainer name leaked:\n{combined}");
    assert!(!combined.contains("Agreement-7"), "agreement id leaked:\n{combined}");
    // The authorization is still reported — as authorized + a masked fingerprint.
    assert!(combined.contains("redacted-sha256:"), "expected a masked fingerprint:\n{combined}");
    assert!(combined.contains("authorized"), "auth state must still be reported:\n{combined}");
}

#[test]
fn masked_fingerprint_is_deterministic_but_non_reversible() {
    let cal = calendar(fresh_freshness());
    let view = cal.as_of(as_of()).unwrap();
    let a = CalendarDiagnostic::from_view(&view, d(2010, 1, 1));
    let b = CalendarDiagnostic::from_view(&view, d(2010, 1, 1));
    let fa = a.authorization.as_ref().unwrap().authority_fingerprint.clone();
    let fb = b.authorization.as_ref().unwrap().authority_fingerprint.clone();
    assert_eq!(fa, fb, "same identity → same fingerprint (deterministic)");
    assert!(!fa.contains("Jane"), "the fingerprint must not embed the raw identity: {fa}");
}
