//! U1 schema contract: serde round-trip of the self-contained snapshot.
//!
//! These tests pin the JSON shape before later units (U2–U7) build on it. They
//! exercise the schema types through the public crate surface only.

use chrono::{NaiveDate, TimeZone, Utc};
use nautilus_ls_calendar::schema::{
    Alert, AlertKind, Authorization, CalendarScope, Citation, Coverage, DayRow, DayStatus,
    EvidenceKind, EvidenceRecord, Freshness, Snapshot, Source, SourceAvailabilityBound, SourceKind,
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
