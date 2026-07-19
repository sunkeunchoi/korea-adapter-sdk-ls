//! U14 refresh tooling: candidate build + deterministic categorized diff + source-failure
//! retention + credential/no-raw-rows boundary. All synthetic, offline, fixed-clock.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use nautilus_ls::calendar_refresh::{
    build_candidate, diff_against_predecessor, refresh, strip_url_credentials, write_candidate,
    CategorizedDiff, DiffCategory, EvidenceInputPort, LiveEvidencePort, MaintainerCredentials,
    RefreshInputs, RefreshMode, RefreshScope, SourceOutcome, StaticEvidencePort,
};
use nautilus_ls_calendar::schema::{
    Authorization, CalendarScope, Citation, Coverage, DayRow, DayStatus, EvidenceKind,
    EvidenceRecord, Freshness, Snapshot, Source, SourceAvailabilityBound, SourceKind,
};
use nautilus_ls_calendar::witness::{witness_from_response, KrxDailyMarketResponse, KrxDailyRow};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, KrxCalendar, WitnessOutcome};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn t(y: i32, m: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, day, 0, 0, 0).unwrap()
}

fn ev(id: &str, source_id: &str, date: NaiveDate, kind: EvidenceKind, cited: bool) -> EvidenceRecord {
    EvidenceRecord {
        id: id.to_string(),
        source_id: source_id.to_string(),
        date,
        kind,
        valid: true,
        superseded_by: None,
        citation: cited.then(|| Citation {
            reference: "NOTICE-1".to_string(),
            issuer: "KRX".to_string(),
            note: None,
        }),
        recorded_at: t(2012, 5, 1),
    }
}

fn src(id: &str, kind: SourceKind) -> Source {
    Source {
        id: id.to_string(),
        kind,
        label: format!("{id} (SYNTHETIC)"),
        synthetic: true,
    }
}

fn stamp(mut snap: Snapshot) -> Snapshot {
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    snap
}

/// A prior (active predecessor) snapshot over 2012-06-01..2012-06-05, with one Closed
/// (rule) date, one TradingSession (witness) date, rest Unknown.
fn prior_snapshot() -> Snapshot {
    let from = d(2012, 6, 1);
    let through = d(2012, 6, 5);
    let sources = vec![
        src("krx-daily", SourceKind::KrxDailyMarket),
        src("kasi", SourceKind::KasiHoliday),
        src("krx-rule", SourceKind::KrxRule),
    ];
    // 2012-06-01 Closed via holiday+rule; 2012-06-04 TradingSession via witness.
    let evidence = vec![
        ev("kasi-0601", "kasi", d(2012, 6, 1), EvidenceKind::HolidayFact, false),
        ev("rule-0601", "krx-rule", d(2012, 6, 1), EvidenceKind::DeterministicRule, false),
        ev("witness-0604", "krx-daily", d(2012, 6, 4), EvidenceKind::PositiveWitness, false),
    ];
    let mut rows = Vec::new();
    let mut cursor = from;
    while cursor <= through {
        let (status, decisive) = match cursor {
            x if x == d(2012, 6, 1) => (
                DayStatus::Closed,
                vec!["kasi-0601".to_string(), "rule-0601".to_string()],
            ),
            x if x == d(2012, 6, 4) => (DayStatus::TradingSession, vec!["witness-0604".to_string()]),
            _ => (DayStatus::Unknown, vec![]),
        };
        rows.push(DayRow {
            date: cursor,
            status,
            decisive_evidence: decisive,
            conflicting_evidence: vec![],
            alerts: vec![],
        });
        cursor = cursor.succ_opt().unwrap();
    }
    stamp(Snapshot {
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
            authority: "Synthetic Authority".to_string(),
            granted_at: t(2010, 1, 1),
            expires_at: Some(t(2099, 1, 1)),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: from,
            materialized_through: through,
            retrospectively_checked_through: through,
            scheduled_closure_evaluated_through: through,
            source_availability: vec![SourceAvailabilityBound {
                source_id: "krx-daily".to_string(),
                available_from: Some(d(2010, 1, 4)),
                available_through: None,
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: t(2012, 5, 20),
            holiday_facts_checked_at: Some(t(2012, 5, 20)),
            full_history_reconciled_at: Some(t(2012, 5, 1)),
            forward_readiness_through: Some(d(2012, 7, 15)),
            last_incremental_at: Some(t(2012, 5, 20)),
        },
        sources,
        evidence,
        alerts: vec![],
        rows,
    })
}

/// Inputs where the KRX daily source succeeds and adds a witness for an in-window date.
fn ok_inputs_flip_0603_to_session() -> RefreshInputs {
    RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![ev(
            "witness-0603",
            "krx-daily",
            d(2012, 6, 3),
            EvidenceKind::PositiveWitness,
            false,
        )],
        outcomes: vec![SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket)],
    }
}

fn refresh_now() -> DateTime<Utc> {
    t(2012, 6, 6)
}

fn horizon() -> (NaiveDate, NaiveDate) {
    (d(2012, 5, 30), d(2012, 7, 20))
}

#[test]
fn candidate_is_created_and_active_snapshot_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let active_path = dir.path().join("calendar.json");
    let prior = prior_snapshot();
    std::fs::write(&active_path, serde_json::to_vec_pretty(&prior).unwrap()).unwrap();
    let active_bytes_before = std::fs::read(&active_path).unwrap();

    let inputs = ok_inputs_flip_0603_to_session();
    let scope = RefreshScope {
        from: d(2012, 6, 1),
        through: d(2012, 6, 5),
    };
    let outcome = refresh(
        &prior,
        &StaticEvidencePort::new(inputs),
        scope,
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );

    // predecessor identity is stamped to the EXACT active predecessor.
    assert_eq!(
        outcome.candidate.predecessor_artifact_id.as_deref(),
        Some(prior.artifact_id.as_str())
    );

    let artifacts = write_candidate(&active_path, &outcome).unwrap();

    // The active file bytes are UNCHANGED.
    let active_bytes_after = std::fs::read(&active_path).unwrap();
    assert_eq!(
        active_bytes_before, active_bytes_after,
        "refresh must never overwrite the active snapshot"
    );

    // The candidate + diff were written to separate paths, distinct from active.
    assert!(artifacts.candidate_path.exists());
    assert!(artifacts.diff_path.exists());
    assert_ne!(artifacts.candidate_path, active_path);

    // The candidate revalidates through the real loader — U15 can revalidate.
    let candidate_bytes = std::fs::read(&artifacts.candidate_path).unwrap();
    let candidate: Snapshot = serde_json::from_slice(&candidate_bytes).unwrap();
    KrxCalendar::from_snapshot(candidate, refresh_now())
        .expect("candidate is a valid loadable snapshot");
}

#[test]
fn diff_is_deterministic_same_inputs_identical_categories() {
    let prior = prior_snapshot();
    let scope = RefreshScope {
        from: d(2012, 6, 1),
        through: d(2012, 6, 5),
    };
    let build = || {
        refresh(
            &prior,
            &StaticEvidencePort::new(ok_inputs_flip_0603_to_session()),
            RefreshScope {
                from: scope.from,
                through: scope.through,
            },
            RefreshMode::Incremental,
            refresh_now(),
            horizon(),
        )
    };
    let a = build();
    let b = build();
    assert_eq!(a.candidate.artifact_id, b.candidate.artifact_id);
    assert_eq!(a.diff, b.diff, "same inputs must produce an identical diff");
    assert_eq!(a.diff.categories(), b.diff.categories());
}

#[test]
fn high_risk_flags_fire_for_each_class() {
    let prior = prior_snapshot();
    let scope = RefreshScope {
        from: d(2012, 6, 1),
        through: d(2012, 6, 5),
    };

    // (1) historical status change + (2) near-term closure change: 2012-06-04 was a
    // TradingSession; a cited closure notice flips it to Closed inside the horizon.
    let closure_inputs = RefreshInputs {
        sources: vec![src("krx-notice", SourceKind::FirstPartyNotice)],
        evidence: vec![
            // supersede the prior witness so the closure decides
            EvidenceRecord {
                superseded_by: Some("notice-0604".to_string()),
                ..ev("witness-0604", "krx-daily", d(2012, 6, 4), EvidenceKind::PositiveWitness, false)
            },
            ev("notice-0604", "krx-notice", d(2012, 6, 4), EvidenceKind::ClosureNotice, true),
        ],
        outcomes: vec![SourceOutcome::ok("krx-notice", SourceKind::FirstPartyNotice)],
    };
    let out = refresh(
        &prior,
        &StaticEvidencePort::new(closure_inputs),
        RefreshScope { from: scope.from, through: scope.through },
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );
    let cats = out.diff.categories();
    assert!(cats.contains(&DiffCategory::HistoricalStatusChange), "cats={cats:?}");
    assert!(cats.contains(&DiffCategory::NearTermClosureChange), "cats={cats:?}");
    assert!(out.diff.requires_review());

    // (3) transition to Unknown: correction invalidates the 2012-06-04 witness with no
    // replacement → the date becomes Unknown.
    let to_unknown = RefreshInputs {
        sources: vec![src("krx-corr", SourceKind::Correction)],
        evidence: vec![
            EvidenceRecord {
                valid: false,
                superseded_by: Some("corr-0604".to_string()),
                ..ev("witness-0604", "krx-daily", d(2012, 6, 4), EvidenceKind::PositiveWitness, false)
            },
            ev("corr-0604", "krx-corr", d(2012, 6, 4), EvidenceKind::Correction, true),
        ],
        outcomes: vec![SourceOutcome::ok("krx-corr", SourceKind::Correction)],
    };
    let out = refresh(
        &prior,
        &StaticEvidencePort::new(to_unknown),
        RefreshScope { from: scope.from, through: scope.through },
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );
    assert!(out.diff.categories().contains(&DiffCategory::TransitionToUnknown));

    // (4) evidence removal: the krx-daily source succeeds but returns NOTHING for its
    // prior witness → the witness-0604 record is removed.
    let removal = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![],
        outcomes: vec![SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket)],
    };
    let out = refresh(
        &prior,
        &StaticEvidencePort::new(removal),
        RefreshScope { from: scope.from, through: scope.through },
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );
    assert!(out.diff.categories().contains(&DiffCategory::EvidenceRemoval));

    // (5) first-party conflict: a witness and a cited closure notice on the same date.
    let conflict = RefreshInputs {
        sources: vec![src("krx-notice", SourceKind::FirstPartyNotice)],
        evidence: vec![ev(
            "notice-0603",
            "krx-notice",
            d(2012, 6, 3),
            EvidenceKind::ClosureNotice,
            true,
        )],
        outcomes: vec![SourceOutcome::ok("krx-notice", SourceKind::FirstPartyNotice)],
    };
    // Combine with the surviving witness-0604 conflict scenario by targeting 2012-06-04:
    let conflict = RefreshInputs {
        evidence: vec![ev(
            "notice-0604b",
            "krx-notice",
            d(2012, 6, 4),
            EvidenceKind::ClosureNotice,
            true,
        )],
        ..conflict
    };
    let out = refresh(
        &prior,
        &StaticEvidencePort::new(conflict),
        RefreshScope { from: scope.from, through: scope.through },
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );
    assert!(
        out.diff.categories().contains(&DiffCategory::FirstPartyConflict),
        "cats={:?}",
        out.diff.categories()
    );

    // (6) coverage contraction: a candidate whose materialized_through is BEFORE the
    // predecessor's is a contraction (direct diff, no source-driven build needed).
    let mut shrunk = prior.clone();
    shrunk.coverage.materialized_through = d(2012, 6, 4);
    shrunk.coverage.retrospectively_checked_through = d(2012, 6, 4);
    shrunk.coverage.scheduled_closure_evaluated_through = d(2012, 6, 4);
    shrunk.rows.retain(|r| r.date <= d(2012, 6, 4));
    shrunk.predecessor_artifact_id = Some(prior.artifact_id.clone());
    let shrunk = stamp(shrunk);
    let diff = diff_against_predecessor(&prior, &shrunk, horizon(), false);
    assert!(diff.categories().contains(&DiffCategory::CoverageContraction));
}

#[test]
fn source_failure_retains_evidence_ages_freshness_and_forms_partial_candidate() {
    let prior = prior_snapshot();
    // KASI FAILS; the KRX daily source succeeds and adds an in-window witness (independent
    // additive evidence). Retention: KASI evidence retained, coverage NOT expanded,
    // freshness aged, but the additive KRX evidence forms a PARTIAL candidate for review.
    let inputs = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![ev(
            "witness-0603",
            "krx-daily",
            d(2012, 6, 3),
            EvidenceKind::PositiveWitness,
            false,
        )],
        outcomes: vec![
            SourceOutcome::failed("kasi", SourceKind::KasiHoliday, "KASI timeout"),
            SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket),
        ],
    };
    let out = refresh(
        &prior,
        &StaticEvidencePort::new(inputs),
        // Attempt to expand forward to 2012-06-08.
        RefreshScope { from: d(2012, 6, 1), through: d(2012, 6, 8) },
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );

    // Retained: the KASI holiday fact for 2012-06-01 survives.
    assert!(
        out.candidate.evidence.iter().any(|e| e.id == "kasi-0601"),
        "a failed source must retain its accepted evidence"
    );
    // Status unchanged where the failed source decided: 2012-06-01 still Closed.
    let row_0601 = out
        .candidate
        .rows
        .iter()
        .find(|r| r.date == d(2012, 6, 1))
        .unwrap();
    assert_eq!(row_0601.status, DayStatus::Closed);

    // No absence-driven coverage expansion: through stays at the predecessor's.
    assert_eq!(
        out.candidate.coverage.materialized_through,
        prior.coverage.materialized_through,
        "a failed source cannot claim expanded coverage by absence"
    );

    // Freshness aged: the KASI dimension is UNCHANGED from the predecessor.
    assert_eq!(
        out.candidate.freshness.holiday_facts_checked_at,
        prior.freshness.holiday_facts_checked_at,
        "a failed source ages (does not advance) its freshness dimension"
    );

    // The independent additive KRX evidence still applied (partial candidate) and it
    // requires review.
    assert!(out.candidate.evidence.iter().any(|e| e.id == "witness-0603"));
    let row_0603 = out
        .candidate
        .rows
        .iter()
        .find(|r| r.date == d(2012, 6, 3))
        .unwrap();
    assert_eq!(row_0603.status, DayStatus::TradingSession);
    assert!(out.diff.requires_review(), "a partial candidate still requires review");
    assert!(out.diff.partial, "the diff records the partial (source-failure) provenance");
}

#[test]
fn no_raw_krx_response_is_persisted_in_candidate_or_diff() {
    // Build a witness through the REAL positive-witness rule from a raw response whose
    // rows carry a marker only present in raw KRX bodies.
    let resp = KrxDailyMarketResponse {
        success: true,
        requested_date: d(2012, 6, 3),
        rows: vec![KrxDailyRow {
            date: d(2012, 6, 3),
            market: "KOSPI".to_string(),
        }],
        error_code: None,
    };
    let witness = match witness_from_response(&resp) {
        WitnessOutcome::Witness(mut w) => {
            w.id = "witness-0603".to_string();
            w.source_id = "krx-daily".to_string();
            w
        }
        other => panic!("expected a witness, got {other:?}"),
    };
    let inputs = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![witness],
        outcomes: vec![SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket)],
    };
    let prior = prior_snapshot();
    let out = refresh(
        &prior,
        &StaticEvidencePort::new(inputs),
        RefreshScope { from: d(2012, 6, 1), through: d(2012, 6, 5) },
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );
    let candidate_json = serde_json::to_string(&out.candidate).unwrap();
    let diff_json = serde_json::to_string(&out.diff).unwrap();
    // Raw-response-only field names must never appear (the port yields normalized
    // EvidenceRecords, not raw bodies).
    for marker in ["\"market\"", "\"requested_date\"", "\"error_code\"", "KrxDailyRow"] {
        assert!(!candidate_json.contains(marker), "raw marker {marker} leaked into candidate");
        assert!(!diff_json.contains(marker), "raw marker {marker} leaked into diff");
    }
}

#[test]
fn strip_url_credentials_masks_service_key_and_appkey() {
    let url = "https://apis.data.go.kr/B090041/openapi/service/SpcdeInfoService/getRestDeInfo?serviceKey=SUPERSECRET123&solYear=2012&_type=json";
    let stripped = strip_url_credentials(url);
    assert!(!stripped.contains("SUPERSECRET123"), "serviceKey leaked: {stripped}");
    assert!(stripped.contains("serviceKey=***"), "{stripped}");
    assert!(stripped.contains("solYear=2012"), "non-credential params preserved: {stripped}");

    let krx = "https://open.krx.example/api/stk_bydd_trd?appkey=APPKEY_ABC999&basDd=20120604";
    let stripped = strip_url_credentials(krx);
    assert!(!stripped.contains("APPKEY_ABC999"), "appkey leaked: {stripped}");
    assert!(stripped.contains("appkey=***"), "{stripped}");
}

#[test]
fn transport_request_error_strips_credentials_from_every_surface() {
    // A live transport whose fetch always errors, echoing the full URL (with creds) in the
    // error string — the port must strip the query-param key before it reaches any surface.
    let creds = MaintainerCredentials {
        kasi_service_key: Some("SUPERSECRET123".to_string()),
        krx_appkey: Some("APPKEY_ABC999".to_string()),
    };
    let port = LiveEvidencePort::new(creds, |url: &str| {
        // Simulate a transport error that naively includes the requested URL verbatim.
        Err(format!("connection refused while fetching {url}"))
    });
    let inputs = port.gather(&RefreshScope {
        from: d(2012, 6, 4),
        through: d(2012, 6, 4),
    });
    // Every source failed; no evidence produced.
    assert!(inputs.evidence.is_empty());
    assert!(inputs.outcomes.iter().all(|o| !o.is_ok()));
    // No credential value appears in any recorded failure reason.
    for outcome in &inputs.outcomes {
        let reason = outcome.failure_reason().unwrap_or_default();
        assert!(!reason.contains("SUPERSECRET123"), "serviceKey leaked: {reason}");
        assert!(!reason.contains("APPKEY_ABC999"), "appkey leaked: {reason}");
        assert!(reason.contains("***"), "the credential must be masked: {reason}");
    }
    // The port's Debug never prints credential material either.
    let dbg = format!("{port:?}");
    assert!(!dbg.contains("SUPERSECRET123"), "{dbg}");
    assert!(!dbg.contains("APPKEY_ABC999"), "{dbg}");
}

#[test]
fn build_candidate_stamps_identities_and_predecessor() {
    let prior = prior_snapshot();
    let candidate = build_candidate(
        &prior,
        &ok_inputs_flip_0603_to_session(),
        &RefreshScope { from: d(2012, 6, 1), through: d(2012, 6, 5) },
        RefreshMode::Incremental,
        refresh_now(),
    );
    // Identities recompute to the declared values (loadable).
    assert_eq!(candidate.artifact_id, compute_artifact_id(&candidate));
    assert_eq!(candidate.calendar_id, compute_calendar_id(&candidate));
    assert_eq!(
        candidate.predecessor_artifact_id.as_deref(),
        Some(prior.artifact_id.as_str())
    );
    // A pure diff over the same predecessor/candidate is stable.
    let diff: CategorizedDiff = diff_against_predecessor(&prior, &candidate, horizon(), false);
    assert_eq!(diff, diff_against_predecessor(&prior, &candidate, horizon(), false));
}
