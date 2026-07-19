//! Synthetic fixture corpus (U7, AC11) + its reproducible generator.
//!
//! `fixtures/base_2010_2012.json` is a checked-in, EXPLICITLY-SYNTHETIC / counterfactual
//! base snapshot materializing every civil date 2010-01-01..=2012-12-31. Most weekdays are
//! `Unknown` and only the named scenarios carry evidence, so it is deliberately unusable as
//! a production calendar and cannot be mistaken for a real KRX calendar (`scope.synthetic ==
//! true`, sources/citations/evidence all labeled `SYNTHETIC`).
//!
//! Because hand-authoring 1096 contiguous rows plus correct identity hashes is impractical,
//! the fixture is emitted by [`regenerate_base_fixture`] — an `#[ignore]`d test that builds
//! the [`Snapshot`] in code via the PRODUCTION reconciler ([`reconcile`]), stamps its
//! identities via the production `compute_*` fns, and serializes to the fixtures path. Run it
//! with:
//!
//! ```text
//! cargo test -p nautilus-ls-calendar --test fixtures regenerate_base_fixture -- --ignored
//! ```
//!
//! The other tests here load the emitted JSON through the REAL
//! [`KrxCalendar::load_from_path`] loader (no fake calendar, no bypass) and assert each named
//! scenario's expected tri-state status + alerts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use nautilus_ls_calendar::reconcile::reconcile;
use nautilus_ls_calendar::schema::{
    Alert, AlertKind, Authorization, CalendarScope, Citation, Coverage, DayRow, DayStatus,
    EvidenceKind, EvidenceRecord, Freshness, Snapshot, Source, SourceAvailabilityBound, SourceKind,
};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, KrxCalendar};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// The checked-in fixture path, resolved relative to the crate manifest so the generator and
/// the loading tests always agree.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/base_2010_2012.json")
}

// --- synthetic source ids (all `synthetic: true`, all clearly labeled) -------------------

const SRC_KRX_DAILY: &str = "krx-daily-SYNTH";
const SRC_KASI: &str = "kasi-SYNTH";
const SRC_KRX_RULE: &str = "krx-rule-SYNTH";
const SRC_NOTICE: &str = "krx-notice-SYNTH";
const SRC_CORRECTION: &str = "krx-correction-SYNTH";

fn synthetic_sources() -> Vec<Source> {
    let s = |id: &str, kind: SourceKind, label: &str| Source {
        id: id.to_string(),
        kind,
        label: format!("{label} (SYNTHETIC — counterfactual, never a real KRX row)"),
        synthetic: true,
    };
    vec![
        s(SRC_KRX_DAILY, SourceKind::KrxDailyMarket, "KRX stk_bydd_trd"),
        s(SRC_KASI, SourceKind::KasiHoliday, "KASI holiday facts"),
        s(SRC_KRX_RULE, SourceKind::KrxRule, "KRX published rule"),
        s(SRC_NOTICE, SourceKind::FirstPartyNotice, "KRX first-party notice"),
        s(SRC_CORRECTION, SourceKind::Correction, "Maintainer correction"),
    ]
}

/// Midnight UTC of `date` — a deterministic placeholder `recorded_at`.
fn recorded_at(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
}

/// A synthetic evidence record on `date`: valid, un-cited, un-superseded by default.
fn ev(id: &str, source_id: &str, date: NaiveDate, kind: EvidenceKind) -> EvidenceRecord {
    EvidenceRecord {
        id: id.to_string(),
        source_id: source_id.to_string(),
        date,
        kind,
        valid: true,
        superseded_by: None,
        citation: None,
        recorded_at: recorded_at(date),
    }
}

/// A synthetic first-party citation (never real).
fn cite(reference: &str) -> Citation {
    Citation {
        reference: format!("{reference} (SYNTHETIC)"),
        issuer: "KRX (SYNTHETIC)".to_string(),
        note: Some("counterfactual fixture citation — not a real notice".to_string()),
    }
}

/// Reconcile a scenario date's evidence through the PRODUCTION reconciler, stamp alert ids,
/// record the resulting row, and fold the evidence into the corpus.
fn commit_scenario(
    date: NaiveDate,
    evs: Vec<EvidenceRecord>,
    evidence: &mut Vec<EvidenceRecord>,
    alerts: &mut Vec<Alert>,
    rows: &mut BTreeMap<NaiveDate, DayRow>,
) {
    let reconciled = reconcile(date, &evs);
    let mut alert_ids = Vec::new();
    for (i, ra) in reconciled.alerts.iter().enumerate() {
        let id = format!("al-{date}-{i}");
        alerts.push(Alert {
            id: id.clone(),
            date: ra.date,
            kind: ra.kind,
            message: ra.message.clone(),
        });
        alert_ids.push(id);
    }
    rows.insert(
        date,
        DayRow {
            date,
            status: reconciled.status,
            decisive_evidence: reconciled.decisive_evidence.clone(),
            conflicting_evidence: reconciled.conflicting_evidence.clone(),
            alerts: alert_ids,
        },
    );
    evidence.extend(evs);
}

/// Build the full synthetic base snapshot in code (the reproducible generator body).
fn build_base_snapshot() -> Snapshot {
    let from = d(2010, 1, 1);
    let through = d(2012, 12, 31);

    let mut evidence: Vec<EvidenceRecord> = Vec::new();
    let mut alerts: Vec<Alert> = Vec::new();
    let mut scenario_rows: BTreeMap<NaiveDate, DayRow> = BTreeMap::new();

    // Convenience macro-free helper closures would borrow-conflict; call the free fn instead.

    // --- Scenario 9: first materialization boundary — New Year's Day closure (holiday+rule).
    commit_scenario(
        d(2010, 1, 1),
        vec![
            ev("ev-newyear-holiday", SRC_KASI, d(2010, 1, 1), EvidenceKind::HolidayFact),
            ev("ev-newyear-rule", SRC_KRX_RULE, d(2010, 1, 1), EvidenceKind::DeterministicRule),
        ],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );

    // --- Scenario 3: weekday election closure (holiday+rule).
    commit_scenario(
        d(2010, 6, 2),
        vec![
            ev("ev-election-holiday", SRC_KASI, d(2010, 6, 2), EvidenceKind::HolidayFact),
            ev("ev-election-rule", SRC_KRX_RULE, d(2010, 6, 2), EvidenceKind::DeterministicRule),
        ],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );

    // --- Scenario 1 + 7: ordinary sessions bracketing an isolated Unknown.
    // 06-15 session, 06-16 left Unknown (no evidence — the isolated Unknown), 06-17 session.
    commit_scenario(
        d(2010, 6, 15),
        vec![ev("ev-ordinary-witness-1", SRC_KRX_DAILY, d(2010, 6, 15), EvidenceKind::PositiveWitness)],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );
    commit_scenario(
        d(2010, 6, 17),
        vec![ev("ev-ordinary-witness-2", SRC_KRX_DAILY, d(2010, 6, 17), EvidenceKind::PositiveWitness)],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );

    // --- Scenario 2: named weekend closures (deterministic weekend rule).
    for (id, date) in [
        ("ev-weekend-sat", d(2010, 6, 19)),
        ("ev-weekend-sun", d(2010, 6, 20)),
    ] {
        commit_scenario(
            date,
            vec![ev(id, SRC_KRX_RULE, date, EvidenceKind::DeterministicRule)],
            &mut evidence,
            &mut alerts,
            &mut scenario_rows,
        );
    }

    // --- Scenario 8: year-end closure Dec 31 (year-end rule).
    commit_scenario(
        d(2010, 12, 31),
        vec![ev("ev-yearend-2010", SRC_KRX_RULE, d(2010, 12, 31), EvidenceKind::DeterministicRule)],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );

    // --- Scenario 5: multi-day holiday cluster — Lunar New Year (holiday+rule each day).
    for (n, date) in [(2, d(2011, 2, 2)), (3, d(2011, 2, 3)), (4, d(2011, 2, 4))] {
        commit_scenario(
            date,
            vec![
                ev(&format!("ev-lny-holiday-{n}"), SRC_KASI, date, EvidenceKind::HolidayFact),
                ev(&format!("ev-lny-rule-{n}"), SRC_KRX_RULE, date, EvidenceKind::DeterministicRule),
            ],
            &mut evidence,
            &mut alerts,
            &mut scenario_rows,
        );
    }

    // --- Scenario 10: inferred-source disagreement — positive witness overrides an inferred
    // (rule) closure → Trading Session + WitnessOverridesInference alert.
    commit_scenario(
        d(2011, 6, 15),
        vec![
            ev("ev-override-witness", SRC_KRX_DAILY, d(2011, 6, 15), EvidenceKind::PositiveWitness),
            ev("ev-override-rule", SRC_KRX_RULE, d(2011, 6, 15), EvidenceKind::DeterministicRule),
        ],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );

    // --- Scenario 6: exceptional closure with a cited first-party notice.
    {
        let mut notice = ev("ev-exceptional-notice", SRC_NOTICE, d(2011, 9, 21), EvidenceKind::ClosureNotice);
        notice.citation = Some(cite("KRX-NOTICE-EXCEPTIONAL-2011-09-21"));
        commit_scenario(
            d(2011, 9, 21),
            vec![notice],
            &mut evidence,
            &mut alerts,
            &mut scenario_rows,
        );
    }

    // --- Scenario 11: first-party disagreement — two conflicting cited notices → Unknown +
    // FirstPartyConflict alert.
    {
        let mut n1 = ev("ev-conflict-notice-a", SRC_NOTICE, d(2011, 10, 5), EvidenceKind::ClosureNotice);
        n1.citation = Some(cite("KRX-NOTICE-CONFLICT-A"));
        let mut n2 = ev("ev-conflict-notice-b", SRC_NOTICE, d(2011, 10, 5), EvidenceKind::ClosureNotice);
        n2.citation = Some(cite("KRX-NOTICE-CONFLICT-B"));
        commit_scenario(
            d(2011, 10, 5),
            vec![n1, n2],
            &mut evidence,
            &mut alerts,
            &mut scenario_rows,
        );
    }

    // --- Scenario 12: retrospective correction pair — a correction supersedes ONLY the
    // identified stale notice; the sibling governs → Closed + Superseded alert.
    {
        let mut superseded = ev("ev-correction-stale", SRC_NOTICE, d(2012, 3, 14), EvidenceKind::ClosureNotice);
        superseded.citation = Some(cite("KRX-NOTICE-STALE-2012-03-14"));
        superseded.superseded_by = Some("ev-correction-fix".to_string());
        let mut sibling = ev("ev-correction-governing", SRC_NOTICE, d(2012, 3, 14), EvidenceKind::ClosureNotice);
        sibling.citation = Some(cite("KRX-NOTICE-GOVERNING-2012-03-14"));
        let mut correction = ev("ev-correction-fix", SRC_CORRECTION, d(2012, 3, 14), EvidenceKind::Correction);
        correction.citation = Some(cite("KRX-CORRECTION-2012-03-14"));
        commit_scenario(
            d(2012, 3, 14),
            vec![superseded, sibling, correction],
            &mut evidence,
            &mut alerts,
            &mut scenario_rows,
        );
    }

    // --- Scenario 4: Labor Day (May 1) weekday closure (rule authority).
    commit_scenario(
        d(2012, 5, 1),
        vec![ev("ev-laborday-2012", SRC_KRX_RULE, d(2012, 5, 1), EvidenceKind::DeterministicRule)],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );

    // --- Scenario 9b: last materialization boundary — year-end closure Dec 31 (rule).
    commit_scenario(
        d(2012, 12, 31),
        vec![ev("ev-yearend-2012", SRC_KRX_RULE, d(2012, 12, 31), EvidenceKind::DeterministicRule)],
        &mut evidence,
        &mut alerts,
        &mut scenario_rows,
    );

    // --- Materialize EVERY civil date contiguously: scenario rows where present, otherwise a
    // bare `Unknown` (no evidence). This "most weekdays Unknown" fill is exactly what makes
    // the fixture unusable as a production calendar.
    let mut rows: Vec<DayRow> = Vec::new();
    let mut cursor = from;
    loop {
        let row = scenario_rows.get(&cursor).cloned().unwrap_or(DayRow {
            date: cursor,
            status: DayStatus::Unknown,
            decisive_evidence: vec![],
            conflicting_evidence: vec![],
            alerts: vec![],
        });
        rows.push(row);
        if cursor == through {
            break;
        }
        cursor = cursor.succ_opt().expect("no civil-date overflow within 2010-2012");
    }

    let snap = Snapshot {
        schema_version: "1.0.0".to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: None,
        scope: CalendarScope {
            calendar_name: "SYNTHETIC counterfactual KRX-shaped calendar — NOT A REAL CALENDAR"
                .to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: true,
        },
        authorization: Authorization {
            authorized: true,
            authority: "SYNTHETIC-MAINTAINER (counterfactual)".to_string(),
            granted_at: Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap(),
            expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: from,
            materialized_through: through,
            retrospectively_checked_through: through,
            scheduled_closure_evaluated_through: through,
            source_availability: vec![SourceAvailabilityBound {
                source_id: SRC_KRX_DAILY.to_string(),
                available_from: Some(d(2010, 1, 4)),
                available_through: Some(through),
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap(),
            holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap()),
            full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap()),
            forward_readiness_through: Some(d(2013, 3, 1)),
            last_incremental_at: Some(Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap()),
        },
        sources: synthetic_sources(),
        evidence,
        alerts,
        rows,
    };

    // Stamp deterministic identities via the PRODUCTION fns so the fixture loads.
    let mut snap = snap;
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    snap
}

/// The generator: build the snapshot in code and (re)emit the checked-in JSON fixture. Kept
/// `#[ignore]`d so the normal suite reads the committed file; run it to regenerate.
#[test]
#[ignore = "regenerates the checked-in fixture; run explicitly with --ignored"]
fn regenerate_base_fixture() {
    let snap = build_base_snapshot();
    let json = serde_json::to_string_pretty(&snap).expect("snapshot serializes");
    let path = fixture_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixtures dir");
    }
    std::fs::write(&path, json).expect("write fixture");

    // Sanity: what we just wrote loads through the real loader.
    let as_of = Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap();
    KrxCalendar::load_from_path(&fixture_path(), as_of).expect("emitted fixture must load");
}

/// The as-of used to load the fixture in the assertions below (within authorization).
fn fixture_as_of() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap()
}

fn load_fixture() -> KrxCalendar {
    KrxCalendar::load_from_path(&fixture_path(), fixture_as_of()).unwrap_or_else(|e| {
        panic!(
            "the checked-in fixture must load through the real loader (regenerate with \
             `--test fixtures regenerate_base_fixture -- --ignored`): {e:?}"
        )
    })
}

#[test]
fn base_fixture_loads_through_the_real_loader_with_correct_identities() {
    // Loading itself proves the declared identities recompute (the loader rejects a mismatch).
    let cal = load_fixture();
    assert!(!cal.artifact_id().is_empty());
    assert!(!cal.calendar_id().is_empty());
    // Contiguous 2010-01-01..=2012-12-31 = 1096 rows.
    assert_eq!(cal.snapshot().rows.len(), 1096);
    assert_eq!(cal.coverage().materialized_from, d(2010, 1, 1));
    assert_eq!(cal.coverage().materialized_through, d(2012, 12, 31));
}

#[test]
fn base_fixture_cannot_be_mistaken_for_a_real_krx_calendar() {
    let cal = load_fixture();
    let snap = cal.snapshot();

    // Explicitly synthetic scope + sources.
    assert!(snap.scope.synthetic, "scope must be marked synthetic");
    assert!(snap.sources.iter().all(|s| s.synthetic), "every source synthetic");

    // The overwhelming majority of weekdays are Unknown — no real KRX calendar looks like
    // this (real calendars positively resolve nearly every weekday).
    let mut weekdays = 0usize;
    let mut unknown_weekdays = 0usize;
    for row in &snap.rows {
        let weekday = row.date.weekday();
        let is_weekend = matches!(
            weekday,
            chrono::Weekday::Sat | chrono::Weekday::Sun
        );
        if !is_weekend {
            weekdays += 1;
            if row.status == DayStatus::Unknown {
                unknown_weekdays += 1;
            }
        }
    }
    assert!(weekdays > 700, "sanity: ~782 weekdays in the corpus");
    let unknown_fraction = unknown_weekdays as f64 / weekdays as f64;
    assert!(
        unknown_fraction > 0.95,
        "most weekdays must be Unknown (was {unknown_fraction:.3})"
    );
}

/// Every named scenario resolves to its expected tri-state status through the real query
/// surface (loaded via `load_from_path`).
#[test]
fn named_scenarios_resolve_to_their_expected_status() {
    let cal = load_fixture();
    let view = cal.as_of(fixture_as_of()).expect("authorized view");
    let status = |date: NaiveDate| view.day(date).expect("in window").status;

    // Ordinary sessions (positive witness).
    assert_eq!(status(d(2010, 6, 15)), DayStatus::TradingSession);
    assert_eq!(status(d(2010, 6, 17)), DayStatus::TradingSession);
    // Isolated Unknown between two sessions.
    assert_eq!(status(d(2010, 6, 16)), DayStatus::Unknown);

    // Named weekend closures.
    assert_eq!(status(d(2010, 6, 19)), DayStatus::Closed);
    assert_eq!(status(d(2010, 6, 20)), DayStatus::Closed);

    // Weekday election closure.
    assert_eq!(status(d(2010, 6, 2)), DayStatus::Closed);
    // Labor Day.
    assert_eq!(status(d(2012, 5, 1)), DayStatus::Closed);
    // Lunar New Year multi-day cluster.
    assert_eq!(status(d(2011, 2, 2)), DayStatus::Closed);
    assert_eq!(status(d(2011, 2, 3)), DayStatus::Closed);
    assert_eq!(status(d(2011, 2, 4)), DayStatus::Closed);
    // Exceptional (cited) closure.
    assert_eq!(status(d(2011, 9, 21)), DayStatus::Closed);
    // Year-end closures.
    assert_eq!(status(d(2010, 12, 31)), DayStatus::Closed);
    assert_eq!(status(d(2012, 12, 31)), DayStatus::Closed);
    // First materialization boundary (New Year closure).
    assert_eq!(status(d(2010, 1, 1)), DayStatus::Closed);
}

/// The alert-bearing scenarios resolve to the expected status AND the expected alert kind.
#[test]
fn alert_bearing_scenarios_carry_their_alerts() {
    let cal = load_fixture();
    let view = cal.as_of(fixture_as_of()).expect("authorized view");

    let has_alert = |date: NaiveDate, kind: AlertKind| {
        view.day(date)
            .expect("in window")
            .alerts
            .iter()
            .any(|a| a.kind == kind)
    };

    // Inferred-source disagreement — witness overrides inferred closure.
    let over = view.day(d(2011, 6, 15)).unwrap();
    assert_eq!(over.status, DayStatus::TradingSession);
    assert!(has_alert(d(2011, 6, 15), AlertKind::WitnessOverridesInference));
    // The inferred rule is retained as conflicting evidence.
    assert!(!over.conflicting_evidence.is_empty());

    // First-party disagreement — two conflicting notices → Unknown.
    let conflict = view.day(d(2011, 10, 5)).unwrap();
    assert_eq!(conflict.status, DayStatus::Unknown);
    assert!(has_alert(d(2011, 10, 5), AlertKind::FirstPartyConflict));

    // Retrospective correction pair → Closed with a Superseded alert.
    let corrected = view.day(d(2012, 3, 14)).unwrap();
    assert_eq!(corrected.status, DayStatus::Closed);
    assert!(has_alert(d(2012, 3, 14), AlertKind::Superseded));
    // The governing sibling notice decides; the superseded stale notice is not decisive.
    assert_eq!(corrected.decisive_evidence.len(), 1);
    assert_eq!(corrected.decisive_evidence[0].id, "ev-correction-governing");
}

/// Both materialization boundaries are the first and last rows; one step past either end is a
/// typed out-of-range, not an Unknown.
#[test]
fn materialization_boundaries_are_first_and_last_rows() {
    let cal = load_fixture();
    let view = cal.as_of(fixture_as_of()).expect("authorized view");

    // First row is 2010-01-01; the day before is out of range.
    assert_eq!(cal.snapshot().rows.first().unwrap().date, d(2010, 1, 1));
    assert!(view.day(d(2010, 1, 1)).is_ok());
    assert!(view.day(d(2009, 12, 31)).is_err());

    // Last row is 2012-12-31; the day after is out of range.
    assert_eq!(cal.snapshot().rows.last().unwrap().date, d(2012, 12, 31));
    assert!(view.day(d(2012, 12, 31)).is_ok());
    assert!(view.day(d(2013, 1, 1)).is_err());
}
