//! U1 schema contract: serde round-trip of the self-contained snapshot.
//!
//! These tests pin the JSON shape before later units (U2–U7) build on it. They
//! exercise the schema types through the public crate surface only.

use chrono::{NaiveDate, TimeZone, Utc};
use nautilus_ls_calendar::schema::{
    Alert, AlertKind, Authorization, CalendarScope, Citation, Coverage, DayRow, DayStatus,
    EvidenceKind, EvidenceRecord, Freshness, Snapshot, Source, SourceAvailabilityBound, SourceKind,
};
use nautilus_ls_calendar::{
    compute_artifact_id, compute_calendar_id, reconcile, schema_is_compatible, AsOfView,
    CalendarLoadError, DateRange, KrxCalendar, Presence, QueryError, ReconcileAlert, ReconciledDay,
    SessionSearch, SCHEMA_VERSION,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// Build a minimal-but-complete snapshot with one row of every status, one source,
/// one evidence record, one alert. Deliberately synthetic.
fn minimal_snapshot() -> Snapshot {
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
            granted_at: Utc.with_ymd_and_hms(2012, 1, 1, 0, 0, 0).unwrap(),
            expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: d(2010, 1, 1),
            materialized_through: d(2010, 1, 4),
            retrospectively_checked_through: d(2010, 1, 3),
            scheduled_closure_evaluated_through: d(2010, 1, 4),
            source_availability: vec![SourceAvailabilityBound {
                source_id: "krx-daily".to_string(),
                available_from: Some(d(2010, 1, 4)),
                available_through: Some(d(2010, 1, 4)),
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap(),
            holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap()),
            full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2012, 5, 1, 0, 0, 0).unwrap()),
            forward_readiness_through: Some(d(2012, 7, 15)),
            last_incremental_at: Some(Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap()),
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
            citation: Some(Citation {
                reference: "SYNTHETIC-NOTICE-1".to_string(),
                issuer: "SYNTHETIC".to_string(),
                note: None,
            }),
            recorded_at: Utc.with_ymd_and_hms(2010, 1, 5, 0, 0, 0).unwrap(),
        }],
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
                alerts: vec!["al-1".to_string()],
            },
        ],
    }
}

#[test]
fn happy_snapshot_round_trips() {
    let snap = minimal_snapshot();
    let json = serde_json::to_string(&snap).expect("serialize");
    let back: Snapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(snap, back, "round-trip must preserve the full snapshot");
}

#[test]
fn day_status_serializes_the_three_states() {
    // The tri-state enum must round-trip each variant distinctly.
    for status in [
        DayStatus::TradingSession,
        DayStatus::Closed,
        DayStatus::Unknown,
    ] {
        let json = serde_json::to_string(&status).unwrap();
        let back: DayStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, back);
    }
    // And they are mutually distinct.
    assert_ne!(DayStatus::Closed, DayStatus::Unknown);
    assert_ne!(DayStatus::TradingSession, DayStatus::Closed);
}

#[test]
fn materialized_unknown_row_is_distinct_from_an_absent_row() {
    let snap = minimal_snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let back: Snapshot = serde_json::from_str(&json).unwrap();

    // A materialized Unknown row (2010-01-02) with EMPTY evidence refs survives the
    // round-trip and is present in the deserialized snapshot.
    let unknown_row = back
        .rows
        .iter()
        .find(|r| r.date == d(2010, 1, 2))
        .expect("materialized Unknown row must be present");
    assert_eq!(unknown_row.status, DayStatus::Unknown);
    assert!(unknown_row.decisive_evidence.is_empty());
    assert!(unknown_row.conflicting_evidence.is_empty());
    assert!(unknown_row.alerts.is_empty());

    // 2010-01-03 has NO row at all — distinguishable from the materialized Unknown.
    assert!(
        back.rows.iter().all(|r| r.date != d(2010, 1, 3)),
        "an absent date must not be materialized as a row"
    );
}

#[test]
fn snapshot_missing_a_required_section_fails_to_deserialize() {
    // A valid snapshot, then strip the `coverage` section: serde must return an Err,
    // not panic and not silently default.
    let snap = minimal_snapshot();
    let mut value: serde_json::Value = serde_json::to_value(&snap).unwrap();
    value.as_object_mut().unwrap().remove("coverage");
    let bad = serde_json::to_string(&value).unwrap();

    let result = serde_json::from_str::<Snapshot>(&bad);
    assert!(
        result.is_err(),
        "a snapshot missing the required `coverage` section must fail to deserialize"
    );
}

// ---------------------------------------------------------------------------
// U2: deterministic identities (artifact_id / calendar_id) + SemVer compat.
// ---------------------------------------------------------------------------

#[test]
fn identities_are_deterministic_for_identical_content() {
    // Compute twice on the same value: byte-identical ids.
    let snap = minimal_snapshot();
    assert_eq!(compute_artifact_id(&snap), compute_artifact_id(&snap));
    assert_eq!(compute_calendar_id(&snap), compute_calendar_id(&snap));

    // Two independently-built equal snapshots agree.
    let other = minimal_snapshot();
    assert_eq!(
        compute_artifact_id(&snap),
        compute_artifact_id(&other),
        "equal content must yield an equal artifact_id"
    );
    assert_eq!(
        compute_calendar_id(&snap),
        compute_calendar_id(&other),
        "equal content must yield an equal calendar_id"
    );
}

#[test]
fn identities_ignore_the_identity_fields_themselves() {
    // Setting artifact_id/calendar_id must NOT feed back into the artifact hash
    // (KTD4: the identity fields are excluded from canonicalization).
    let base = minimal_snapshot();
    let mut stamped = base.clone();
    stamped.artifact_id = "deadbeef".to_string();
    stamped.calendar_id = "cafef00d".to_string();
    assert_eq!(
        compute_artifact_id(&base),
        compute_artifact_id(&stamped),
        "artifact_id must exclude the identity fields themselves"
    );
    assert_eq!(
        compute_calendar_id(&base),
        compute_calendar_id(&stamped),
        "calendar_id must exclude the identity fields themselves"
    );
}

#[test]
fn retrieval_mechanics_move_artifact_id_but_not_calendar_id() {
    let base = minimal_snapshot();

    // Change ONLY an evidence retrieval timestamp.
    let mut ts = base.clone();
    ts.evidence[0].recorded_at = Utc.with_ymd_and_hms(2011, 1, 1, 0, 0, 0).unwrap();
    ts.freshness.evidence_refreshed_at = Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap();
    assert_ne!(
        compute_artifact_id(&base),
        compute_artifact_id(&ts),
        "a retrieval-timestamp change must move artifact_id"
    );
    assert_eq!(
        compute_calendar_id(&base),
        compute_calendar_id(&ts),
        "a retrieval-timestamp change must NOT move calendar_id"
    );

    // Change ONLY a source_availability bound.
    let mut avail = base.clone();
    avail.coverage.source_availability[0].available_through = Some(d(2010, 1, 5));
    assert_ne!(
        compute_artifact_id(&base),
        compute_artifact_id(&avail),
        "a source_availability change must move artifact_id"
    );
    assert_eq!(
        compute_calendar_id(&base),
        compute_calendar_id(&avail),
        "a source_availability change must NOT move calendar_id"
    );
}

#[test]
fn an_effective_status_change_moves_both_identities() {
    let base = minimal_snapshot();
    let mut flipped = base.clone();
    // Flip the TradingSession row (2010-01-04) to Closed.
    let row = flipped
        .rows
        .iter_mut()
        .find(|r| r.date == d(2010, 1, 4))
        .expect("row present");
    row.status = DayStatus::Closed;

    assert_ne!(
        compute_artifact_id(&base),
        compute_artifact_id(&flipped),
        "an effective status change must move artifact_id"
    );
    assert_ne!(
        compute_calendar_id(&base),
        compute_calendar_id(&flipped),
        "an effective status change must move calendar_id"
    );
}

#[test]
fn a_decisive_claim_identity_change_moves_both_identities() {
    let base = minimal_snapshot();
    let mut invalidated = base.clone();
    // ev-1 is decisive for the 2010-01-04 TradingSession row; flip its validity.
    invalidated.evidence[0].valid = false;

    assert_ne!(
        compute_artifact_id(&base),
        compute_artifact_id(&invalidated),
        "a decisive-claim validity change must move artifact_id"
    );
    assert_ne!(
        compute_calendar_id(&base),
        compute_calendar_id(&invalidated),
        "a decisive-claim validity change must move calendar_id"
    );
}

#[test]
fn schema_compat_accepts_same_major_and_rejects_unsupported_major() {
    assert!(SCHEMA_VERSION.starts_with("1."), "test assumes MAJOR 1");
    assert!(schema_is_compatible(SCHEMA_VERSION));
    assert!(schema_is_compatible("1.5.0"), "same major is compatible");
    assert!(schema_is_compatible("1.0.99"));
    assert!(
        !schema_is_compatible("2.0.0"),
        "an unsupported (higher) major must be incompatible"
    );
    assert!(
        !schema_is_compatible("0.9.0"),
        "an unsupported (lower) major must be incompatible"
    );
    assert!(!schema_is_compatible("not-a-version"));
    assert!(!schema_is_compatible("1.0"), "a malformed SemVer must be rejected");
}

// ---------------------------------------------------------------------------
// U4: as-of view + proof-preserving day/range queries.
// ---------------------------------------------------------------------------

/// A contiguous, loadable synthetic snapshot covering 2010-01-01..=2010-01-10 with a
/// deliberate mix of statuses to exercise every query branch:
///
/// ```text
/// 01  Closed
/// 02  Closed
/// 03  Unknown
/// 04  TradingSession  (decisive ev-1, alert al-1, conflicting ev-2)
/// 05  TradingSession
/// 06  Closed
/// 07  Unknown
/// 08  Closed
/// 09  Closed
/// 10  TradingSession
/// ```
///
/// `expires_at` is the caller-supplied argument so authorization-boundary tests can vary
/// it; the identities are stamped so it passes the U3 loader.
fn queryable_snapshot(expires_at: chrono::DateTime<Utc>) -> Snapshot {
    let row = |date, status, decisive: &[&str], conflicting: &[&str], alerts: &[&str]| DayRow {
        date,
        status,
        decisive_evidence: decisive.iter().map(|s| s.to_string()).collect(),
        conflicting_evidence: conflicting.iter().map(|s| s.to_string()).collect(),
        alerts: alerts.iter().map(|s| s.to_string()).collect(),
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
        authorization: Authorization {
            authorized: true,
            authority: "SYNTHETIC-MAINTAINER".to_string(),
            granted_at: Utc.with_ymd_and_hms(2009, 1, 1, 0, 0, 0).unwrap(),
            expires_at: Some(expires_at),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: d(2010, 1, 1),
            materialized_through: d(2010, 1, 10),
            retrospectively_checked_through: d(2010, 1, 10),
            scheduled_closure_evaluated_through: d(2010, 1, 10),
            source_availability: vec![SourceAvailabilityBound {
                source_id: "krx-daily".to_string(),
                available_from: Some(d(2010, 1, 4)),
                available_through: Some(d(2010, 1, 10)),
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: Utc.with_ymd_and_hms(2010, 1, 11, 0, 0, 0).unwrap(),
            holiday_facts_checked_at: None,
            full_history_reconciled_at: None,
            forward_readiness_through: None,
            last_incremental_at: None,
        },
        sources: vec![
            Source {
                id: "krx-daily".to_string(),
                kind: SourceKind::KrxDailyMarket,
                label: "KRX stk_bydd_trd (SYNTHETIC)".to_string(),
                synthetic: true,
            },
            Source {
                id: "krx-rule".to_string(),
                kind: SourceKind::KrxRule,
                label: "KRX rule (SYNTHETIC)".to_string(),
                synthetic: true,
            },
        ],
        evidence: vec![
            EvidenceRecord {
                id: "ev-1".to_string(),
                source_id: "krx-daily".to_string(),
                date: d(2010, 1, 4),
                kind: EvidenceKind::PositiveWitness,
                valid: true,
                superseded_by: None,
                citation: None,
                recorded_at: Utc.with_ymd_and_hms(2010, 1, 5, 0, 0, 0).unwrap(),
            },
            EvidenceRecord {
                id: "ev-2".to_string(),
                source_id: "krx-rule".to_string(),
                date: d(2010, 1, 4),
                kind: EvidenceKind::DeterministicRule,
                valid: true,
                superseded_by: None,
                citation: None,
                recorded_at: Utc.with_ymd_and_hms(2010, 1, 5, 0, 0, 0).unwrap(),
            },
        ],
        alerts: vec![Alert {
            id: "al-1".to_string(),
            date: d(2010, 1, 4),
            kind: AlertKind::WitnessOverridesInference,
            message: "positive witness overrides inferred closure".to_string(),
        }],
        rows: vec![
            row(d(2010, 1, 1), DayStatus::Closed, &[], &[], &[]),
            row(d(2010, 1, 2), DayStatus::Closed, &[], &[], &[]),
            row(d(2010, 1, 3), DayStatus::Unknown, &[], &[], &[]),
            row(
                d(2010, 1, 4),
                DayStatus::TradingSession,
                &["ev-1"],
                &["ev-2"],
                &["al-1"],
            ),
            row(d(2010, 1, 5), DayStatus::TradingSession, &[], &[], &[]),
            row(d(2010, 1, 6), DayStatus::Closed, &[], &[], &[]),
            row(d(2010, 1, 7), DayStatus::Unknown, &[], &[], &[]),
            row(d(2010, 1, 8), DayStatus::Closed, &[], &[], &[]),
            row(d(2010, 1, 9), DayStatus::Closed, &[], &[], &[]),
            row(d(2010, 1, 10), DayStatus::TradingSession, &[], &[], &[]),
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

/// A loaded, valid calendar over the fixture, authorized well past the as-of instant.
fn queryable_calendar() -> KrxCalendar {
    let as_of = Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap();
    let snap = queryable_snapshot(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap());
    KrxCalendar::from_snapshot(snap, as_of).expect("fixture must load")
}

fn view(cal: &KrxCalendar) -> AsOfView<'_> {
    cal.as_of(Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap())
        .expect("authorized view")
}

#[test]
fn day_facts_return_each_status_with_resolved_refs() {
    let cal = queryable_calendar();
    let view = view(&cal);

    // Trading Session with resolved evidence + alert records (not raw ids).
    let ts = view.day(d(2010, 1, 4)).expect("in window");
    assert_eq!(ts.status, DayStatus::TradingSession);
    assert_eq!(ts.decisive_evidence.len(), 1);
    assert_eq!(ts.decisive_evidence[0].id, "ev-1");
    assert_eq!(ts.conflicting_evidence.len(), 1);
    assert_eq!(ts.conflicting_evidence[0].id, "ev-2");
    assert_eq!(ts.alerts.len(), 1);
    assert_eq!(ts.alerts[0].id, "al-1");

    // Closed and Unknown are both SUCCESSFUL day facts.
    assert_eq!(view.day(d(2010, 1, 1)).unwrap().status, DayStatus::Closed);
    let unknown = view.day(d(2010, 1, 3)).expect("Unknown is a success");
    assert_eq!(unknown.status, DayStatus::Unknown);
    assert!(unknown.decisive_evidence.is_empty());
}

#[test]
fn a_day_outside_the_window_is_out_of_range_not_unknown() {
    let cal = queryable_calendar();
    let view = view(&cal);
    assert_eq!(
        view.day(d(2009, 12, 31)),
        Err(QueryError::OutOfRange {
            date: d(2009, 12, 31)
        }),
        "a date before the window must be a typed OutOfRange, NOT Unknown"
    );
    assert_eq!(
        view.day(d(2010, 1, 11)),
        Err(QueryError::OutOfRange {
            date: d(2010, 1, 11)
        }),
        "a date after the window must be a typed OutOfRange, NOT Unknown"
    );
}

#[test]
fn presence_is_present_absent_or_indeterminate_proof_preserving() {
    let cal = queryable_calendar();
    let view = view(&cal);

    // A span with a Trading Session → Present.
    let span = DateRange::inclusive(d(2010, 1, 1), d(2010, 1, 5)).unwrap();
    assert_eq!(view.presence(&span).unwrap(), Presence::Present);

    // An all-Closed span → proven Absent.
    let closed = DateRange::inclusive(d(2010, 1, 8), d(2010, 1, 9)).unwrap();
    assert_eq!(view.presence(&closed).unwrap(), Presence::Absent);

    // A span with an Unknown and no proven session → Indeterminate, NEVER Absent.
    let unknown = DateRange::inclusive(d(2010, 1, 6), d(2010, 1, 9)).unwrap();
    assert_eq!(view.presence(&unknown).unwrap(), Presence::Indeterminate);

    // A proven session outranks a co-present Unknown → still Present (Unknown never
    // downgrades a positively-proven presence).
    let mixed = DateRange::inclusive(d(2010, 1, 3), d(2010, 1, 4)).unwrap();
    assert_eq!(view.presence(&mixed).unwrap(), Presence::Present);
}

#[test]
fn first_and_last_session_search_preserve_proof() {
    let cal = queryable_calendar();
    let view = view(&cal);

    // Found on a real session.
    let hit = DateRange::inclusive(d(2010, 1, 4), d(2010, 1, 5)).unwrap();
    assert_eq!(
        view.first_session(&hit).unwrap(),
        SessionSearch::Found(d(2010, 1, 4))
    );
    assert_eq!(
        view.last_session(&hit).unwrap(),
        SessionSearch::Found(d(2010, 1, 5))
    );

    // Proven None only when the whole span is Closed.
    let closed = DateRange::inclusive(d(2010, 1, 8), d(2010, 1, 9)).unwrap();
    assert_eq!(view.first_session(&closed).unwrap(), SessionSearch::None);
    assert_eq!(view.last_session(&closed).unwrap(), SessionSearch::None);

    // Indeterminate when an Unknown sits before any proven session in the scan
    // direction: 01-01 Closed, 01-02 Closed, 01-03 Unknown → the first session could be
    // the Unknown day, so first_session is Indeterminate.
    let front_unknown = DateRange::inclusive(d(2010, 1, 1), d(2010, 1, 5)).unwrap();
    assert_eq!(
        view.first_session(&front_unknown).unwrap(),
        SessionSearch::Indeterminate
    );
    // Scanning backward over 01-06 Closed, 01-07 Unknown hits the Unknown first.
    let back_unknown = DateRange::inclusive(d(2010, 1, 6), d(2010, 1, 7)).unwrap();
    assert_eq!(
        view.last_session(&back_unknown).unwrap(),
        SessionSearch::Indeterminate
    );
}

#[test]
fn range_forms_normalize_to_the_expected_canonical_spans() {
    let (a, b) = (d(2010, 1, 4), d(2010, 1, 6));

    // Inclusive [a, b] keeps both endpoints.
    assert_eq!(
        DateRange::inclusive(a, b).unwrap().bounds(),
        Some((d(2010, 1, 4), d(2010, 1, 6)))
    );
    // Half-open [a, b) drops the last endpoint.
    assert_eq!(
        DateRange::half_open(a, b).unwrap().bounds(),
        Some((d(2010, 1, 4), d(2010, 1, 5)))
    );
    // Strictly-between (a, b) drops both → single interior day here.
    assert_eq!(
        DateRange::strictly_between(a, b).unwrap().bounds(),
        Some((d(2010, 1, 5), d(2010, 1, 5)))
    );

    // Single-day inclusive span.
    assert_eq!(
        DateRange::inclusive(a, a).unwrap().bounds(),
        Some((d(2010, 1, 4), d(2010, 1, 4)))
    );
    // Empty half-open span (start == end) and empty strictly-between (adjacent endpoints).
    assert!(DateRange::half_open(a, a).unwrap().is_empty());
    assert!(DateRange::strictly_between(d(2010, 1, 4), d(2010, 1, 5))
        .unwrap()
        .is_empty());
    // Inverted inclusive normalizes to empty (not an error).
    assert!(DateRange::inclusive(b, a).unwrap().is_empty());
}

#[test]
fn empty_spans_aggregate_to_proven_absent_and_none() {
    let cal = queryable_calendar();
    let view = view(&cal);
    let empty = DateRange::empty();
    assert!(empty.is_empty());
    assert_eq!(view.presence(&empty).unwrap(), Presence::Absent);
    assert_eq!(view.first_session(&empty).unwrap(), SessionSearch::None);
    assert_eq!(view.last_session(&empty).unwrap(), SessionSearch::None);
}

#[test]
fn range_endpoint_conversion_overflow_is_a_typed_error() {
    // succ past the maximum representable date.
    assert_eq!(
        DateRange::strictly_between(NaiveDate::MAX, NaiveDate::MAX),
        Err(QueryError::DateOverflow)
    );
    // pred past the minimum representable date.
    assert_eq!(
        DateRange::strictly_between(NaiveDate::MIN, NaiveDate::MIN),
        Err(QueryError::DateOverflow)
    );
}

#[test]
fn a_range_past_the_materialized_window_is_out_of_range_not_truncated() {
    let cal = queryable_calendar();
    let view = view(&cal);
    // Span [08, 15] extends past materialized_through (10): must be OutOfRange at the
    // offending endpoint, NOT a truncated all-Closed Absent.
    let past = DateRange::inclusive(d(2010, 1, 8), d(2010, 1, 15)).unwrap();
    assert_eq!(
        view.presence(&past),
        Err(QueryError::OutOfRange {
            date: d(2010, 1, 15)
        })
    );
    assert_eq!(
        view.first_session(&past),
        Err(QueryError::OutOfRange {
            date: d(2010, 1, 15)
        })
    );
    // And a span starting before the window is caught at the start endpoint.
    let before = DateRange::inclusive(d(2009, 12, 30), d(2010, 1, 3)).unwrap();
    assert_eq!(
        view.presence(&before),
        Err(QueryError::OutOfRange {
            date: d(2009, 12, 30)
        })
    );
}

#[test]
fn an_unknown_cannot_be_collapsed_by_aggregation_ae_us27() {
    // Proof: the Unknown at 01-07 is SOLELY responsible for the Indeterminate verdict.
    // Flipping only that row to Closed turns the same span into a proven Absent — showing
    // the aggregate never silently treated the Unknown as Closed.
    let cal = queryable_calendar();
    let base_view = view(&cal);
    let span = DateRange::inclusive(d(2010, 1, 6), d(2010, 1, 9)).unwrap();
    assert_eq!(base_view.presence(&span).unwrap(), Presence::Indeterminate);

    let as_of = Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap();
    let mut snap = queryable_snapshot(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap());
    snap.rows
        .iter_mut()
        .find(|r| r.date == d(2010, 1, 7))
        .unwrap()
        .status = DayStatus::Closed;
    let snap = stamp(snap);
    let flipped = KrxCalendar::from_snapshot(snap, as_of).unwrap();
    let flipped_view = view(&flipped);
    assert_eq!(
        flipped_view.presence(&span).unwrap(),
        Presence::Absent,
        "with the sole Unknown proven Closed, the span is now a proven Absent"
    );
}

#[test]
fn as_of_view_re_evaluates_authorization_at_the_supplied_instant() {
    // A calendar authorized through 2012-12-31, loaded at a valid instant.
    let loaded_at = Utc.with_ymd_and_hms(2012, 6, 1, 0, 0, 0).unwrap();
    let expiry = Utc.with_ymd_and_hms(2012, 12, 31, 0, 0, 0).unwrap();
    let cal = KrxCalendar::from_snapshot(queryable_snapshot(expiry), loaded_at)
        .expect("valid at load time");

    // A later view, past expiry, re-evaluates authorization WITHOUT reloading → Expired.
    let later = Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 1).unwrap();
    assert!(matches!(
        cal.as_of(later),
        Err(CalendarLoadError::Expired)
    ));
    // Exactly at the expiry instant is still authorized (inclusive-at boundary).
    assert!(cal.as_of(expiry).is_ok());
    // Strictly after is expired.
    let just_after = Utc.with_ymd_and_hms(2012, 12, 31, 0, 0, 1).unwrap();
    assert!(matches!(
        AsOfView::new(&cal, just_after),
        Err(CalendarLoadError::Expired)
    ));
}

// ---------------------------------------------------------------------------
// U5: evidence reconciliation authority matrix (KTD6).
//
// One test per matrix row. Every scenario runs through the pure `reconcile` fn.
// ---------------------------------------------------------------------------

/// The date every U5 scenario reconciles.
fn rd() -> NaiveDate {
    d(2010, 5, 4)
}

/// A synthetic evidence record on [`rd`] with the given id + kind, valid + un-cited +
/// un-superseded by default. Mutate the returned value for citations/validity/supersession.
fn ev(id: &str, kind: EvidenceKind) -> EvidenceRecord {
    EvidenceRecord {
        id: id.to_string(),
        source_id: "src".to_string(),
        date: rd(),
        kind,
        valid: true,
        superseded_by: None,
        citation: None,
        recorded_at: Utc.with_ymd_and_hms(2010, 5, 5, 0, 0, 0).unwrap(),
    }
}

/// A synthetic first-party citation (never real).
fn cite(reference: &str) -> Citation {
    Citation {
        reference: reference.to_string(),
        issuer: "KRX (SYNTHETIC)".to_string(),
        note: None,
    }
}

fn has_alert(r: &ReconciledDay, kind: AlertKind) -> bool {
    r.alerts.iter().any(|a| a.kind == kind)
}

/// Row 1 — a KRX positive witness on an otherwise-inferred (rule) closure wins:
/// Trading Session, the inferred closure retained as conflicting, + override alert.
#[test]
fn row1_witness_overrides_inferred_closure() {
    let evs = [
        ev("wit", EvidenceKind::PositiveWitness),
        ev("rule", EvidenceKind::DeterministicRule),
    ];
    let out = reconcile(rd(), &evs);
    assert_eq!(out.status, DayStatus::TradingSession);
    assert_eq!(out.decisive_evidence, vec!["wit".to_string()]);
    assert_eq!(out.conflicting_evidence, vec!["rule".to_string()]);
    assert!(has_alert(&out, AlertKind::WitnessOverridesInference));
    // Observed operation wins even over an inferred HolidayFact closure.
    let evs2 = [
        ev("wit", EvidenceKind::PositiveWitness),
        ev("hol", EvidenceKind::HolidayFact),
    ];
    let out2 = reconcile(rd(), &evs2);
    assert_eq!(out2.status, DayStatus::TradingSession);
    assert_eq!(out2.conflicting_evidence, vec!["hol".to_string()]);
    assert!(has_alert(&out2, AlertKind::WitnessOverridesInference));
}

/// Row 2 — a positive witness vs. a direct cited first-party closure notice is an
/// unresolved first-party conflict: Unknown + alert, both claims retained as conflicting.
#[test]
fn row2_witness_vs_closure_notice_is_unknown() {
    let mut notice = ev("notice", EvidenceKind::ClosureNotice);
    notice.citation = Some(cite("KRX-NOTICE-2010-05-04"));
    let evs = [ev("wit", EvidenceKind::PositiveWitness), notice];
    let out = reconcile(rd(), &evs);
    assert_eq!(out.status, DayStatus::Unknown);
    assert!(out.decisive_evidence.is_empty());
    assert!(out.conflicting_evidence.contains(&"wit".to_string()));
    assert!(out.conflicting_evidence.contains(&"notice".to_string()));
    assert!(has_alert(&out, AlertKind::WitnessVsClosureNotice));
}

/// Row 3 — a later empty/malformed KRX response (a non-qualifying, `valid == false`
/// witness record) never retracts an accepted witness: Trading Session preserved + alert.
#[test]
fn row3_later_absence_never_retracts_accepted_witness() {
    let mut empty = ev("empty", EvidenceKind::PositiveWitness);
    empty.valid = false; // recorded empty/malformed later response = non-evidence
    let evs = [ev("wit", EvidenceKind::PositiveWitness), empty];
    let out = reconcile(rd(), &evs);
    assert_eq!(out.status, DayStatus::TradingSession);
    assert_eq!(out.decisive_evidence, vec!["wit".to_string()]);
    assert!(has_alert(&out, AlertKind::AbsenceIgnored));
}

/// Row 4 — a KASI holiday fact + an applicable published KRX rule → Closed (both decisive).
#[test]
fn row4_holiday_plus_connecting_rule_is_closed() {
    let evs = [
        ev("hol", EvidenceKind::HolidayFact),
        ev("rule", EvidenceKind::DeterministicRule),
    ];
    let out = reconcile(rd(), &evs);
    assert_eq!(out.status, DayStatus::Closed);
    assert!(out.decisive_evidence.contains(&"hol".to_string()));
    assert!(out.decisive_evidence.contains(&"rule".to_string()));
}

/// Row 4 (negative) — a holiday fact with NO connecting rule is NOT Closed → Unknown.
#[test]
fn row4_holiday_without_connecting_rule_is_not_closed() {
    let evs = [ev("hol", EvidenceKind::HolidayFact)];
    let out = reconcile(rd(), &evs);
    assert_eq!(out.status, DayStatus::Unknown);
    assert!(out.decisive_evidence.is_empty());
}

/// Row 5 — weekend / Labor Day / year-end per a published KRX rule → Closed (rule authority).
#[test]
fn row5_deterministic_rule_is_closed() {
    let evs = [ev("rule", EvidenceKind::DeterministicRule)];
    let out = reconcile(rd(), &evs);
    assert_eq!(out.status, DayStatus::Closed);
    assert_eq!(out.decisive_evidence, vec!["rule".to_string()]);
}

/// Row 6 — an exceptional closure with a CITED first-party notice → Closed; an UN-cited
/// closure notice is rejected (cannot create a bare status) → Unknown.
#[test]
fn row6_cited_closure_notice_is_closed_uncited_rejected() {
    let mut cited = ev("notice", EvidenceKind::ClosureNotice);
    cited.citation = Some(cite("KRX-NOTICE-EXCEPTIONAL"));
    let out = reconcile(rd(), &[cited]);
    assert_eq!(out.status, DayStatus::Closed);
    assert_eq!(out.decisive_evidence, vec!["notice".to_string()]);

    // Bare (un-cited) closure notice: rejected — no bare status.
    let bare = ev("bare", EvidenceKind::ClosureNotice);
    let out2 = reconcile(rd(), &[bare]);
    assert_eq!(out2.status, DayStatus::Unknown);
    assert!(out2.decisive_evidence.is_empty());
}

/// Row 7 — two distinct effective cited first-party closure notices, neither superseding
/// the other, are an unresolved first-party conflict → Unknown + alert.
#[test]
fn row7_conflicting_first_party_claims_is_unknown() {
    let mut n1 = ev("n1", EvidenceKind::ClosureNotice);
    n1.citation = Some(cite("KRX-NOTICE-A"));
    let mut n2 = ev("n2", EvidenceKind::ClosureNotice);
    n2.citation = Some(cite("KRX-NOTICE-B"));
    let out = reconcile(rd(), &[n1, n2]);
    assert_eq!(out.status, DayStatus::Unknown);
    assert!(out.decisive_evidence.is_empty());
    assert!(out.conflicting_evidence.contains(&"n1".to_string()));
    assert!(out.conflicting_evidence.contains(&"n2".to_string()));
    assert!(has_alert(&out, AlertKind::FirstPartyConflict));
}

/// Row 8 — an explicit correction supersedes ONLY the identified evidence: the sibling
/// closure notice is untouched and decides Closed; the superseded one drops out + alert.
#[test]
fn row8_correction_supersedes_only_the_identified_evidence() {
    // Without the correction, two notices would be a first-party conflict (row 7).
    let mut superseded = ev("n1", EvidenceKind::ClosureNotice);
    superseded.citation = Some(cite("KRX-NOTICE-STALE"));
    superseded.superseded_by = Some("corr".to_string());
    let mut sibling = ev("n2", EvidenceKind::ClosureNotice);
    sibling.citation = Some(cite("KRX-NOTICE-GOVERNING"));
    let mut correction = ev("corr", EvidenceKind::Correction);
    correction.citation = Some(cite("KRX-CORRECTION-1"));

    let out = reconcile(rd(), &[superseded, sibling, correction]);
    // Only the identified n1 is superseded; the sibling n2 governs → Closed.
    assert_eq!(out.status, DayStatus::Closed);
    assert_eq!(out.decisive_evidence, vec!["n2".to_string()]);
    assert!(!out.decisive_evidence.contains(&"n1".to_string()));
    assert!(has_alert(&out, AlertKind::Superseded));
}

/// Row 9 — a human adjudication changes only validity/supersession; it cannot write a
/// status. An adjudication-invalidated witness leaves no covering evidence → Unknown.
#[test]
fn row9_adjudication_invalidates_but_cannot_set_status() {
    let mut invalidated = ev("wit", EvidenceKind::PositiveWitness);
    invalidated.valid = false; // adjudication flipped its validity
    invalidated.superseded_by = Some("adj".to_string());
    let mut adjudication = ev("adj", EvidenceKind::Adjudication);
    adjudication.citation = Some(cite("HUMAN-ADJ-1"));

    let out = reconcile(rd(), &[invalidated, adjudication]);
    // The adjudication itself is never status-bearing; the only witness is gone → Unknown.
    assert_eq!(out.status, DayStatus::Unknown);
    assert!(out.decisive_evidence.is_empty());
    assert!(has_alert(&out, AlertKind::Adjudicated));
}

/// Row 10 — no covering evidence (empty, or all-invalid) → Unknown, a successful factual
/// result (never an error, never Closed).
#[test]
fn row10_no_covering_evidence_is_unknown() {
    let out = reconcile(rd(), &[]);
    assert_eq!(
        out,
        ReconciledDay {
            status: DayStatus::Unknown,
            decisive_evidence: vec![],
            conflicting_evidence: vec![],
            alerts: vec![],
        }
    );

    // All-invalid evidence is likewise not covering → Unknown.
    let mut dead = ev("dead", EvidenceKind::PositiveWitness);
    dead.valid = false;
    let out2 = reconcile(rd(), &[dead]);
    assert_eq!(out2.status, DayStatus::Unknown);
    assert!(out2.decisive_evidence.is_empty());
}

/// The alert carries the reconciled date + a message so the caller (U14) only stamps ids.
#[test]
fn reconcile_alert_carries_kind_message_and_date() {
    let evs = [
        ev("wit", EvidenceKind::PositiveWitness),
        ev("rule", EvidenceKind::DeterministicRule),
    ];
    let out = reconcile(rd(), &evs);
    let alert: &ReconcileAlert = out
        .alerts
        .iter()
        .find(|a| a.kind == AlertKind::WitnessOverridesInference)
        .expect("override alert present");
    assert_eq!(alert.date, rd());
    assert!(!alert.message.is_empty());
}
