//! U5: the `calendar-genesis` bin + shared genesis description artifact. The library path is
//! tested directly (small window); the bin is spawned end-to-end over the real history-floor
//! window to prove the refusal exit path writes no candidate. All synthetic, offline.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use nautilus_ls::calendar_refresh::{
    build_genesis, describe_genesis, write_genesis, DateRange, GenesisParams, RefreshInputs,
    SourceOutcome,
};
use nautilus_ls_calendar::schema::{
    Authorization, CalendarScope, EvidenceKind, EvidenceRecord, SourceAvailabilityBound, SourceKind,
    Snapshot,
};
use nautilus_ls_calendar::KrxCalendar;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn t(y: i32, m: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, day, 0, 0, 0).unwrap()
}

fn ev(id: &str, source_id: &str, date: NaiveDate, kind: EvidenceKind) -> EvidenceRecord {
    EvidenceRecord {
        id: id.to_string(),
        source_id: source_id.to_string(),
        date,
        kind,
        valid: true,
        superseded_by: None,
        citation: None,
        recorded_at: t(2026, 1, 1),
    }
}

fn source(id: &str, kind: SourceKind) -> nautilus_ls_calendar::schema::Source {
    nautilus_ls_calendar::schema::Source {
        id: id.to_string(),
        kind,
        label: id.to_string(),
        synthetic: false,
    }
}

/// Genesis inputs over `window`: a witness per `witnessed` date, weekend rules, KASI holiday
/// fact + rule per `holidays`. KRX covers `[from, krx_through]`; KASI/rule cover the window.
fn genesis_inputs(
    window: DateRange,
    krx_through: NaiveDate,
    witnessed: &[NaiveDate],
    holidays: &[NaiveDate],
) -> RefreshInputs {
    use chrono::Datelike;
    let mut evidence = Vec::new();
    let mut cur = window.from;
    while cur <= window.through {
        if matches!(cur.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
            evidence.push(ev(&format!("rule-{cur}"), "krx-rule", cur, EvidenceKind::DeterministicRule));
        }
        cur = cur.succ_opt().unwrap();
    }
    for &w in witnessed {
        evidence.push(ev(&format!("witness-{w}"), "krx-daily", w, EvidenceKind::PositiveWitness));
    }
    for &h in holidays {
        evidence.push(ev(&format!("kasi-{h}"), "kasi", h, EvidenceKind::HolidayFact));
        evidence.push(ev(&format!("rule-{h}"), "krx-rule", h, EvidenceKind::DeterministicRule));
    }
    RefreshInputs {
        sources: vec![
            source("krx-daily", SourceKind::KrxDailyMarket),
            source("kasi", SourceKind::KasiHoliday),
            source("krx-rule", SourceKind::KrxRule),
        ],
        evidence,
        outcomes: vec![
            SourceOutcome::ok_covering("krx-daily", SourceKind::KrxDailyMarket, vec![DateRange::new(window.from, krx_through)]),
            SourceOutcome::ok_covering("kasi", SourceKind::KasiHoliday, vec![window]),
            SourceOutcome::ok_covering("krx-rule", SourceKind::KrxRule, vec![window]),
        ],
    }
}

fn genesis_params(window: DateRange, krx_through: NaiveDate, consumer: DateRange) -> GenesisParams {
    GenesisParams {
        scope: CalendarScope {
            calendar_name: "KRX domestic equity regular session".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: false,
        },
        authorization: Authorization {
            authorized: true,
            authority: "KRX Open API Agreement".to_string(),
            granted_at: t(2026, 1, 1),
            expires_at: Some(t(2027, 1, 1)),
            terminated_at: None,
        },
        source_availability: vec![SourceAvailabilityBound {
            source_id: "krx-daily".to_string(),
            available_from: Some(d(2010, 1, 4)),
            available_through: None,
        }],
        window,
        krx_through,
        consumer_window: consumer,
    }
}

#[test]
fn genesis_candidate_and_description_are_written_consistent_and_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let window = DateRange::new(d(2026, 6, 13), d(2026, 6, 19));
    let consumer = DateRange::new(d(2026, 6, 15), d(2026, 6, 19));
    let inputs = genesis_inputs(
        window,
        d(2026, 6, 19),
        &[d(2026, 6, 15), d(2026, 6, 17), d(2026, 6, 18), d(2026, 6, 19)],
        &[d(2026, 6, 16)],
    );
    let candidate = build_genesis(&genesis_params(window, d(2026, 6, 19), consumer), &inputs, t(2026, 6, 20)).unwrap();
    let description = describe_genesis(&candidate, consumer);

    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cal.json.candidate");
    let artifacts = write_genesis(&out, &candidate, &description).unwrap();

    // Both files are owner-only 0o600.
    for path in [&artifacts.candidate_path, &artifacts.description_path] {
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{} must be 0o600", path.display());
    }

    // The candidate reloads through the real loader.
    let candidate_back: Snapshot =
        serde_json::from_slice(&std::fs::read(&artifacts.candidate_path).unwrap()).unwrap();
    KrxCalendar::from_snapshot(candidate_back.clone(), t(2026, 6, 20)).expect("candidate loads");

    // The description reloads and is consistent with the candidate.
    let desc_back: nautilus_ls::calendar_refresh::GenesisDescription =
        serde_json::from_slice(&std::fs::read(&artifacts.description_path).unwrap()).unwrap();
    assert_eq!(desc_back.candidate_artifact_id, candidate.artifact_id, "description names the exact candidate (U6 linkage)");
    assert_eq!(
        desc_back.trading_session_rows + desc_back.closed_rows + desc_back.unknown_rows,
        candidate.rows.len(),
        "per-status counts sum to the row count"
    );
    assert_eq!(desc_back.consumer_window_unknown_weekdays, 0, "R12: zero Unknown consumer weekdays");
    assert_eq!(desc_back.authority, "KRX Open API Agreement");
}

// ---------------------------------------------------------------------------------------
// End-to-end bin over the real history-floor window (2010-01-04 → horizon).
// ---------------------------------------------------------------------------------------

/// Inputs spanning the full history floor with witnesses for exactly the consumer weekdays.
fn floor_inputs(krx_through: NaiveDate, horizon: NaiveDate, consumer_weekdays: &[NaiveDate]) -> RefreshInputs {
    let floor = d(2010, 1, 4);
    let window = DateRange::new(floor, horizon);
    let mut inputs = genesis_inputs(window, krx_through, consumer_weekdays, &[]);
    // Override coverage: KRX spans floor..krx_through, KASI/rule span the whole window.
    inputs.outcomes = vec![
        SourceOutcome::ok_covering("krx-daily", SourceKind::KrxDailyMarket, vec![DateRange::new(floor, krx_through)]),
        SourceOutcome::ok_covering("kasi", SourceKind::KasiHoliday, vec![window]),
        SourceOutcome::ok_covering("krx-rule", SourceKind::KrxRule, vec![window]),
    ];
    inputs
}

fn write_inputs(path: &Path, inputs: &RefreshInputs) {
    std::fs::write(path, serde_json::to_vec_pretty(inputs).unwrap()).unwrap();
}

const BIN: &str = env!("CARGO_BIN_EXE_calendar-genesis");

#[test]
fn bin_builds_a_genesis_candidate_over_the_full_history_floor() {
    let dir = tempfile::tempdir().unwrap();
    let inputs_path = dir.path().join("inputs.json");
    // Consumer window 2026-06-01..2026-06-05 (Mon..Fri), all witnessed.
    let consumer_weekdays = [d(2026, 6, 1), d(2026, 6, 2), d(2026, 6, 3), d(2026, 6, 4), d(2026, 6, 5)];
    write_inputs(&inputs_path, &floor_inputs(d(2026, 6, 5), d(2026, 6, 30), &consumer_weekdays));

    let out = dir.path().join("cal.json.candidate");
    let status = Command::new(BIN)
        .args([
            "--inputs", inputs_path.to_str().unwrap(),
            "--out", out.to_str().unwrap(),
            "--as-of", "2026-06-06T00:00:00Z",
            "--authority", "KRX Open API Agreement",
            "--granted", "2026-01-01T00:00:00Z",
            "--expires", "2027-01-01T00:00:00Z",
            "--krx-through", "2026-06-05",
            "--horizon-through", "2026-06-30",
            "--consumer-from", "2026-06-01",
            "--state-root", dir.path().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success(), "genesis over the full floor succeeds");
    assert!(out.exists(), "candidate written");
    assert!(
        dir.path().join("cal.json.candidate.genesis-description.json").exists(),
        "description artifact written"
    );
}

#[test]
fn bin_refuses_and_writes_no_candidate_when_a_consumer_weekday_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let inputs_path = dir.path().join("inputs.json");
    // Omit 2026-06-03 (Wed) → an Unknown consumer weekday → R12 refusal.
    let consumer_weekdays = [d(2026, 6, 1), d(2026, 6, 2), d(2026, 6, 4), d(2026, 6, 5)];
    write_inputs(&inputs_path, &floor_inputs(d(2026, 6, 5), d(2026, 6, 30), &consumer_weekdays));

    let out = dir.path().join("cal.json.candidate");
    let output = Command::new(BIN)
        .args([
            "--inputs", inputs_path.to_str().unwrap(),
            "--out", out.to_str().unwrap(),
            "--as-of", "2026-06-06T00:00:00Z",
            "--authority", "KRX Open API Agreement",
            "--granted", "2026-01-01T00:00:00Z",
            "--expires", "2027-01-01T00:00:00Z",
            "--krx-through", "2026-06-05",
            "--horizon-through", "2026-06-30",
            "--consumer-from", "2026-06-01",
            "--state-root", dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success(), "an Unknown consumer weekday refuses");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("refused"), "stderr names the refusal: {stderr}");
    assert!(stderr.contains("2026-06-03"), "the offending date is named: {stderr}");
    assert!(!out.exists(), "no candidate is written on refusal");
}
