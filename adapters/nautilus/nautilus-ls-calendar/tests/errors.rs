//! U3 loader contract: typed load/validation failures that NEVER masquerade as an
//! Unknown day fact (KTD3), evaluated at an explicit caller-supplied as-of instant (KTD5).
//!
//! Proof-first, characterization-per-variant: one scenario per [`CalendarLoadError`]
//! variant, each asserting its own typed variant. A successful `Unknown` day status is a
//! *factual* result of a *loaded* calendar — it can never be the outcome of a load/validate
//! failure, which is structurally an `Err(CalendarLoadError)` (see
//! `no_error_path_is_ever_a_day_status`).

use std::fs;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use nautilus_ls_calendar::schema::{
    Authorization, CalendarScope, Coverage, DayRow, DayStatus, EvidenceKind, EvidenceRecord,
    Freshness, Snapshot, Source, SourceKind,
};
use nautilus_ls_calendar::{
    compute_artifact_id, compute_calendar_id, CalendarLoadError, KrxCalendar,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn ts(y: i32, m: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, day, 0, 0, 0).unwrap()
}

/// The recorded authorization expiry used by the boundary tests.
const EXPIRES_AT: fn() -> DateTime<Utc> = || Utc.with_ymd_and_hms(2015, 6, 1, 0, 0, 0).unwrap();

/// A comfortably-in-window as-of instant for the non-authorization scenarios.
fn as_of_valid() -> DateTime<Utc> {
    ts(2014, 1, 1)
}

/// Build a fully-canonical, contiguous, reference-clean snapshot spanning
/// 2010-01-01..=2010-01-05 with identities left UNSTAMPED. Deliberately synthetic.
fn valid_unstamped() -> Snapshot {
    let from = d(2010, 1, 1);
    let through = d(2010, 1, 5);

    // One evidence record + one row that cites it; the rest are Unknown with no refs.
    let rows = vec![
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
            decisive_evidence: vec!["ev-1".to_string()],
            conflicting_evidence: vec![],
            alerts: vec![],
        },
        DayRow {
            date: d(2010, 1, 5),
            status: DayStatus::Unknown,
            decisive_evidence: vec![],
            conflicting_evidence: vec![],
            alerts: vec![],
        },
    ];

    Snapshot {
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
            authority: "SYNTHETIC-MAINTAINER".to_string(),
            granted_at: ts(2010, 1, 1),
            expires_at: Some(EXPIRES_AT()),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: from,
            materialized_through: through,
            retrospectively_checked_through: d(2010, 1, 4),
            scheduled_closure_evaluated_through: through,
            source_availability: vec![],
        },
        freshness: Freshness {
            evidence_refreshed_at: ts(2012, 6, 1),
            holiday_facts_checked_at: None,
            full_history_reconciled_at: None,
            forward_readiness_through: None,
            last_incremental_at: None,
        },
        sources: vec![Source {
            id: "krx-daily".to_string(),
            kind: SourceKind::KrxDailyMarket,
            label: "KRX stk_bydd_trd (SYNTHETIC)".to_string(),
            synthetic: true,
        }],
        evidence: vec![EvidenceRecord {
            id: "ev-1".to_string(),
            source_id: "krx-daily".to_string(),
            date: d(2010, 1, 4),
            kind: EvidenceKind::PositiveWitness,
            valid: true,
            superseded_by: None,
            citation: None,
            recorded_at: ts(2010, 1, 5),
        }],
        alerts: vec![],
        rows,
    }
}

/// Stamp both deterministic identities so the snapshot passes identity recompute.
fn stamp(mut snap: Snapshot) -> Snapshot {
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    snap
}

/// A fully-valid, stamped snapshot.
fn valid_stamped() -> Snapshot {
    stamp(valid_unstamped())
}

/// Write a snapshot to a fresh tempfile and return the handle (kept alive by the caller).
fn write_snapshot(snap: &Snapshot) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().expect("create tempfile");
    fs::write(file.path(), serde_json::to_vec_pretty(snap).unwrap()).expect("write snapshot");
    file
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[test]
fn happy_valid_snapshot_loads_and_exposes_identities_and_coverage() {
    let snap = valid_stamped();
    let file = write_snapshot(&snap);

    let cal = KrxCalendar::load_from_path(file.path(), as_of_valid())
        .expect("a fully-valid canonical snapshot must load");

    assert_eq!(cal.artifact_id(), snap.artifact_id);
    assert_eq!(cal.calendar_id(), snap.calendar_id);
    assert_eq!(cal.coverage().materialized_from, d(2010, 1, 1));
    assert_eq!(cal.coverage().materialized_through, d(2010, 1, 5));
    assert_eq!(cal.schema_version(), "1.0.0");
}

#[test]
fn from_snapshot_validates_an_in_memory_value() {
    let cal = KrxCalendar::from_snapshot(valid_stamped(), as_of_valid())
        .expect("from_snapshot must accept a valid value");
    assert_eq!(cal.coverage().materialized_through, d(2010, 1, 5));
}

// ---------------------------------------------------------------------------
// Typed error, one per variant — NONE is ever an Unknown day fact.
// ---------------------------------------------------------------------------

#[test]
fn missing_file_is_typed_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist.json");
    let err = KrxCalendar::load_from_path(&path, as_of_valid()).unwrap_err();
    assert!(matches!(err, CalendarLoadError::Missing), "got {err:?}");
}

#[test]
fn unreadable_path_is_typed_unreadable() {
    // A directory exists but is not a readable snapshot file → Unreadable, not Missing.
    let dir = tempfile::tempdir().unwrap();
    let err = KrxCalendar::load_from_path(dir.path(), as_of_valid()).unwrap_err();
    assert!(
        matches!(err, CalendarLoadError::Unreadable { .. }),
        "got {err:?}"
    );
}

#[test]
fn corrupt_json_is_typed_corrupt() {
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), b"{ this is not valid json ").unwrap();
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    assert!(
        matches!(err, CalendarLoadError::Corrupt { .. }),
        "got {err:?}"
    );
}

#[test]
fn unsupported_schema_major_is_typed_unsupported_schema() {
    let mut snap = valid_unstamped();
    snap.schema_version = "2.0.0".to_string();
    let snap = stamp(snap); // stamp so identity is NOT the failing invariant
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    match err {
        CalendarLoadError::UnsupportedSchema { found } => assert_eq!(found, "2.0.0"),
        other => panic!("expected UnsupportedSchema, got {other:?}"),
    }
}

#[test]
fn recomputed_hash_mismatch_is_typed_hash_mismatch() {
    // Stamp correctly, THEN tamper a hashed field without re-stamping.
    let mut snap = valid_stamped();
    snap.scope.calendar_name = "TAMPERED".to_string();
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    match err {
        CalendarLoadError::HashMismatch { field } => assert_eq!(field, "artifact_id"),
        other => panic!("expected HashMismatch, got {other:?}"),
    }
}

#[test]
fn unauthorized_grant_is_typed_unauthorized() {
    let mut snap = valid_unstamped();
    snap.authorization.authorized = false;
    let snap = stamp(snap);
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    assert!(matches!(err, CalendarLoadError::Unauthorized), "got {err:?}");
}

#[test]
fn expired_authorization_is_typed_expired() {
    // as_of strictly after expiry → Expired.
    let snap = valid_stamped();
    let after = EXPIRES_AT() + Duration::days(30);
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), after).unwrap_err();
    assert!(matches!(err, CalendarLoadError::Expired), "got {err:?}");
}

#[test]
fn gapped_dates_are_typed_gapped() {
    // Remove the 2010-01-03 row but keep coverage 01-01..=01-05 → a gap at 01-03.
    let mut snap = valid_unstamped();
    snap.rows.retain(|r| r.date != d(2010, 1, 3));
    let snap = stamp(snap);
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    match err {
        CalendarLoadError::Gapped { date } => assert_eq!(date, d(2010, 1, 3)),
        other => panic!("expected Gapped, got {other:?}"),
    }
}

#[test]
fn duplicated_date_is_typed_duplicated() {
    let mut snap = valid_unstamped();
    // Duplicate the 2010-01-02 row.
    let dup = snap.rows.iter().find(|r| r.date == d(2010, 1, 2)).unwrap().clone();
    snap.rows.insert(2, dup);
    let snap = stamp(snap);
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    match err {
        CalendarLoadError::Duplicated { date } => assert_eq!(date, d(2010, 1, 2)),
        other => panic!("expected Duplicated, got {other:?}"),
    }
}

#[test]
fn dangling_evidence_reference_is_typed_dangling_reference() {
    let mut snap = valid_unstamped();
    // Point the decisive ref at an evidence id that does not exist.
    let row = snap.rows.iter_mut().find(|r| r.date == d(2010, 1, 4)).unwrap();
    row.decisive_evidence = vec!["ev-DOES-NOT-EXIST".to_string()];
    let snap = stamp(snap);
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    match err {
        CalendarLoadError::DanglingReference { reference } => {
            assert_eq!(reference, "ev-DOES-NOT-EXIST")
        }
        other => panic!("expected DanglingReference, got {other:?}"),
    }
}

#[test]
fn impossible_coverage_is_typed_impossible_coverage() {
    // retrospectively_checked_through beyond materialized_through.
    let mut snap = valid_unstamped();
    snap.coverage.retrospectively_checked_through = d(2010, 1, 20);
    let snap = stamp(snap);
    let file = write_snapshot(&snap);
    let err = KrxCalendar::load_from_path(file.path(), as_of_valid()).unwrap_err();
    assert!(
        matches!(err, CalendarLoadError::ImpossibleCoverage { .. }),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Expiry boundary semantics: VALID AT the recorded expiry instant, EXPIRED
// STRICTLY AFTER. Both sides asserted.
// ---------------------------------------------------------------------------

#[test]
fn authorization_is_valid_exactly_at_the_expiry_instant() {
    let snap = valid_stamped();
    let file = write_snapshot(&snap);
    // as_of == expires_at → still authorized (boundary is inclusive of the expiry instant).
    let cal = KrxCalendar::load_from_path(file.path(), EXPIRES_AT());
    assert!(
        cal.is_ok(),
        "as_of == expires_at must remain authorized, got {:?}",
        cal.err()
    );
}

#[test]
fn authorization_is_expired_one_nanosecond_after_the_expiry_instant() {
    let snap = valid_stamped();
    let file = write_snapshot(&snap);
    let one_ns_after = EXPIRES_AT() + Duration::nanoseconds(1);
    let err = KrxCalendar::load_from_path(file.path(), one_ns_after).unwrap_err();
    assert!(matches!(err, CalendarLoadError::Expired), "got {err:?}");
}

// ---------------------------------------------------------------------------
// KTD3: no error path is ever a Day fact. Every failing input yields an
// Err(CalendarLoadError) — a value structurally distinct from DayStatus.
// ---------------------------------------------------------------------------

#[test]
fn no_error_path_is_ever_a_day_status() {
    // Build one input per failing invariant; each must be an Err, never a value that
    // could be confused with DayStatus::Unknown (or any DayStatus).
    let dir = tempfile::tempdir().unwrap();

    // Missing.
    let missing = KrxCalendar::load_from_path(&dir.path().join("nope.json"), as_of_valid());

    // Unreadable (directory).
    let unreadable = KrxCalendar::load_from_path(dir.path(), as_of_valid());

    // Corrupt.
    let corrupt_file = tempfile::NamedTempFile::new().unwrap();
    fs::write(corrupt_file.path(), b"not json").unwrap();
    let corrupt = KrxCalendar::load_from_path(corrupt_file.path(), as_of_valid());

    // The remaining invariants via from_snapshot (no I/O noise).
    let mut bad_schema = valid_unstamped();
    bad_schema.schema_version = "3.0.0".to_string();
    let bad_schema = KrxCalendar::from_snapshot(stamp(bad_schema), as_of_valid());

    let mut tampered = valid_stamped();
    tampered.scope.venue = "TAMPERED".to_string();
    let tampered = KrxCalendar::from_snapshot(tampered, as_of_valid());

    let mut unauth = valid_unstamped();
    unauth.authorization.authorized = false;
    let unauth = KrxCalendar::from_snapshot(stamp(unauth), as_of_valid());

    let expired =
        KrxCalendar::from_snapshot(valid_stamped(), EXPIRES_AT() + Duration::nanoseconds(1));

    let mut gapped = valid_unstamped();
    gapped.rows.retain(|r| r.date != d(2010, 1, 3));
    let gapped = KrxCalendar::from_snapshot(stamp(gapped), as_of_valid());

    let mut dup = valid_unstamped();
    let dr = dup.rows[1].clone();
    dup.rows.insert(2, dr);
    let dup = KrxCalendar::from_snapshot(stamp(dup), as_of_valid());

    let mut dangling = valid_unstamped();
    dangling.rows[3].alerts = vec!["al-missing".to_string()];
    let dangling = KrxCalendar::from_snapshot(stamp(dangling), as_of_valid());

    let mut impossible = valid_unstamped();
    impossible.coverage.materialized_from = d(2010, 2, 1); // from > through
    let impossible = KrxCalendar::from_snapshot(stamp(impossible), as_of_valid());

    for result in [
        missing, unreadable, corrupt, bad_schema, tampered, unauth, expired, gapped, dup,
        dangling, impossible,
    ] {
        // Structurally an Err — there is no DayStatus anywhere on the error path.
        assert!(
            result.is_err(),
            "every failing invariant must be a typed Err, never a (possibly-Unknown) success"
        );
    }
}
