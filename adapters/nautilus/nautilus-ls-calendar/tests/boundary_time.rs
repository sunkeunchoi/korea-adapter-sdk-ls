//! Boundary-time tests (U7, AC8/AC9).
//!
//! Every threshold is proven on BOTH sides with a single FIXED as-of instant: two snapshots
//! that differ ONLY in one freshness anchor (or one authorization instant) are built one
//! tick apart across the threshold, loaded through the REAL validator
//! ([`KrxCalendar::from_snapshot`]), and asserted:
//!
//! - one tick before → NOT stale,
//! - one tick after  → stale,
//! - and the queried [`DayStatus`] is IDENTICAL across the flip (staleness never rewrites a
//!   status — AC8).
//!
//! Authorization expiry reuses the U3 boundary semantics: valid AT the instant, expired
//! (typed [`CalendarLoadError::Expired`]) strictly after.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use nautilus_ls_calendar::schema::{
    Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness, Snapshot,
};
use nautilus_ls_calendar::{
    compute_artifact_id, compute_calendar_id, CalendarLoadError, DimensionStaleness, KrxCalendar,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// The single fixed as-of instant every boundary test evaluates at.
fn fixed_as_of() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2012, 6, 15, 12, 0, 0).unwrap()
}

/// One tick for the instant-based thresholds (KASI / full-history / incremental / auth).
fn tick() -> Duration {
    Duration::seconds(1)
}

/// A minimal, contiguous, loadable synthetic snapshot covering 2012-06-14..=2012-06-16 with
/// one row of each tri-state status. `freshness` and `authorization` are supplied so each
/// boundary test can place a single anchor one tick on either side of its threshold; every
/// other field is held constant so the only thing that moves is the dimension under test.
fn snapshot_with(freshness: Freshness, authorization: Authorization) -> Snapshot {
    let row = |date, status| DayRow {
        date,
        status,
        decisive_evidence: vec![],
        conflicting_evidence: vec![],
        alerts: vec![],
    };
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
        authorization,
        coverage: Coverage {
            materialized_from: d(2012, 6, 14),
            materialized_through: d(2012, 6, 16),
            retrospectively_checked_through: d(2012, 6, 16),
            scheduled_closure_evaluated_through: d(2012, 6, 16),
            source_availability: vec![],
        },
        freshness,
        sources: vec![],
        evidence: vec![],
        alerts: vec![],
        rows: vec![
            row(d(2012, 6, 14), DayStatus::Closed),
            row(d(2012, 6, 15), DayStatus::TradingSession),
            row(d(2012, 6, 16), DayStatus::Unknown),
        ],
    };
    stamp(snap)
}

/// Stamp the deterministic identities so a hand-built snapshot passes the U3 loader.
fn stamp(mut snap: Snapshot) -> Snapshot {
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    snap
}

/// A far-future, always-valid authorization (used by every non-auth freshness test).
fn open_authorization() -> Authorization {
    Authorization {
        authorized: true,
        authority: "SYNTHETIC-MAINTAINER".to_string(),
        granted_at: Utc.with_ymd_and_hms(2012, 1, 1, 0, 0, 0).unwrap(),
        expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
        terminated_at: None,
    }
}

/// A freshness block whose dimensions are all comfortably fresh at [`fixed_as_of`] except
/// the one the caller overrides — start from this and mutate one anchor.
fn fresh_freshness() -> Freshness {
    Freshness {
        evidence_refreshed_at: fixed_as_of(),
        holiday_facts_checked_at: Some(fixed_as_of()),
        full_history_reconciled_at: Some(fixed_as_of()),
        forward_readiness_through: Some(fixed_as_of().date_naive() + Duration::days(365)),
        last_incremental_at: Some(fixed_as_of()),
    }
}

/// Load a snapshot at the fixed as-of, returning the (unchanged) queried status alongside
/// the computed freshness report — the paired outputs every boundary test asserts.
fn load_and_probe(snap: Snapshot) -> (DayStatus, nautilus_ls_calendar::FreshnessReport) {
    let cal = KrxCalendar::from_snapshot(snap, fixed_as_of()).expect("fixture must load");
    let view = cal.as_of(fixed_as_of()).expect("authorized view");
    let status = view.day(d(2012, 6, 15)).expect("in window").status;
    (status, view.freshness())
}

// ---------------------------------------------------------------------------
// KASI holiday facts — 14-day threshold.
// ---------------------------------------------------------------------------

#[test]
fn kasi_14_day_threshold_both_sides_status_unchanged() {
    // Not stale: anchor exactly at the threshold (as_of == anchor + 14d, inclusive).
    let mut f = fresh_freshness();
    f.holiday_facts_checked_at = Some(fixed_as_of() - Duration::days(14));
    let (before_status, before) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(before.kasi_holiday_facts, DimensionStaleness::Fresh);

    // Stale: one tick earlier anchor (as_of > anchor + 14d).
    let mut f = fresh_freshness();
    f.holiday_facts_checked_at = Some(fixed_as_of() - Duration::days(14) - tick());
    let (after_status, after) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(after.kasi_holiday_facts, DimensionStaleness::Stale);

    // Status is unchanged across the staleness flip (AC8).
    assert_eq!(before_status, DayStatus::TradingSession);
    assert_eq!(before_status, after_status);
}

// ---------------------------------------------------------------------------
// Full-history reconciliation — 120-day threshold.
// ---------------------------------------------------------------------------

#[test]
fn full_history_120_day_threshold_both_sides_status_unchanged() {
    let mut f = fresh_freshness();
    f.full_history_reconciled_at = Some(fixed_as_of() - Duration::days(120));
    let (before_status, before) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(before.full_history, DimensionStaleness::Fresh);

    let mut f = fresh_freshness();
    f.full_history_reconciled_at = Some(fixed_as_of() - Duration::days(120) - tick());
    let (after_status, after) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(after.full_history, DimensionStaleness::Stale);

    assert_eq!(before_status, DayStatus::TradingSession);
    assert_eq!(before_status, after_status);
}

// ---------------------------------------------------------------------------
// Incremental — two missed daily post-close opportunities (2-day threshold).
// ---------------------------------------------------------------------------

#[test]
fn incremental_two_missed_opportunities_both_sides_status_unchanged() {
    // Fresh right through the second opportunity's instant (as_of == anchor + 2d).
    let mut f = fresh_freshness();
    f.last_incremental_at = Some(fixed_as_of() - Duration::days(2));
    let (before_status, before) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(before.incremental, DimensionStaleness::Fresh);

    // Stale once that second opportunity has passed unmet (as_of > anchor + 2d).
    let mut f = fresh_freshness();
    f.last_incremental_at = Some(fixed_as_of() - Duration::days(2) - tick());
    let (after_status, after) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(after.incremental, DimensionStaleness::Stale);

    assert_eq!(before_status, DayStatus::TradingSession);
    assert_eq!(before_status, after_status);
}

// ---------------------------------------------------------------------------
// Forward readiness — fewer than 45 evaluated days remaining (date-granular, 1-day tick).
// ---------------------------------------------------------------------------

#[test]
fn forward_readiness_45_day_threshold_both_sides_status_unchanged() {
    let as_of_date = fixed_as_of().date_naive();

    // Not stale: exactly 45 evaluated days remain.
    let mut f = fresh_freshness();
    f.forward_readiness_through = Some(as_of_date + Duration::days(45));
    let (before_status, before) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(before.forward_readiness, DimensionStaleness::Fresh);

    // Stale: one day of the horizon fewer → 44 remain (< 45).
    let mut f = fresh_freshness();
    f.forward_readiness_through = Some(as_of_date + Duration::days(44));
    let (after_status, after) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(after.forward_readiness, DimensionStaleness::Stale);

    assert_eq!(before_status, DayStatus::TradingSession);
    assert_eq!(before_status, after_status);
}

// ---------------------------------------------------------------------------
// An absent anchor is Unevaluated (never silently fresh/stale, never a status input).
// ---------------------------------------------------------------------------

#[test]
fn absent_anchors_are_unevaluated_and_do_not_rewrite_status() {
    let f = Freshness {
        evidence_refreshed_at: fixed_as_of(),
        holiday_facts_checked_at: None,
        full_history_reconciled_at: None,
        forward_readiness_through: None,
        last_incremental_at: None,
    };
    let (status, report) = load_and_probe(snapshot_with(f, open_authorization()));
    assert_eq!(report.kasi_holiday_facts, DimensionStaleness::Unevaluated);
    assert_eq!(report.full_history, DimensionStaleness::Unevaluated);
    assert_eq!(report.incremental, DimensionStaleness::Unevaluated);
    assert_eq!(report.forward_readiness, DimensionStaleness::Unevaluated);
    assert!(!report.any_stale(), "unevaluated dimensions are not stale");
    assert_eq!(status, DayStatus::TradingSession);
}

// ---------------------------------------------------------------------------
// Authorization expiry — valid AT the instant, typed Expired strictly after (U3 semantics).
// ---------------------------------------------------------------------------

#[test]
fn authorization_expiry_both_sides_valid_before_expired_after() {
    // Valid: the grant expires exactly AT the fixed as-of instant (inclusive-at boundary).
    let valid_auth = Authorization {
        expires_at: Some(fixed_as_of()),
        ..open_authorization()
    };
    let (status, _report) = load_and_probe(snapshot_with(fresh_freshness(), valid_auth));
    assert_eq!(
        status,
        DayStatus::TradingSession,
        "authorized AT the expiry instant still answers day facts"
    );

    // Also valid one tick before expiry.
    let still_valid = Authorization {
        expires_at: Some(fixed_as_of() + tick()),
        ..open_authorization()
    };
    assert!(
        KrxCalendar::from_snapshot(
            snapshot_with(fresh_freshness(), still_valid),
            fixed_as_of()
        )
        .is_ok(),
        "one tick before expiry is valid"
    );

    // Expired: the grant lapsed one tick before the as-of instant → typed Expired error,
    // NOT an Unknown/Closed day and NOT a load that then answers stale facts.
    let expired_auth = Authorization {
        expires_at: Some(fixed_as_of() - tick()),
        ..open_authorization()
    };
    let err = KrxCalendar::from_snapshot(snapshot_with(fresh_freshness(), expired_auth), fixed_as_of())
        .expect_err("one tick after expiry must be a typed error");
    assert!(
        matches!(err, CalendarLoadError::Expired),
        "expiry is the typed Expired error, never a day status; got {err:?}"
    );
}
