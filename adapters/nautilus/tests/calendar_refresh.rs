//! U14 refresh tooling: candidate build + deterministic categorized diff + source-failure
//! retention + credential/no-raw-rows boundary. All synthetic, offline, fixed-clock.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use nautilus_ls::calendar_refresh::{
    build_candidate, diff_against_predecessor, merge_ranges, refresh, refresh_incremental,
    strip_url_credentials, uncovered_within, write_candidate, CategorizedDiff, DateRange,
    DiffCategory, EvidenceInputPort, LiveEvidencePort, MaintainerCredentials, RefreshInputs,
    RefreshMode, RefreshScope, SourceOutcome, StaticEvidencePort,
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

/// FIX 1 (safety): an incremental refresh re-gathers only ONE new date but marks KRX_DAILY
/// "ok". Prior KRX positive witnesses on the NON-re-gathered historical dates MUST be
/// retained — a partial re-gather must never retract a prior positive witness (which would
/// revert a proven-open day back to an inferred Closed). Before FIX 1 the build dropped
/// ALL of a successful source's prior evidence wholesale, reverting 2012-06-01 to Closed.
#[test]
fn incremental_partial_regather_retains_prior_witnesses_on_untouched_dates() {
    // Prior over 2012-06-01..2012-06-05:
    //  - 2012-06-01: holiday+rule (inferred Closed) OVERRIDDEN by a KRX witness -> TradingSession
    //  - 2012-06-03: a bare KRX witness -> TradingSession
    let from = d(2012, 6, 1);
    let through = d(2012, 6, 5);
    let sources = vec![
        src("krx-daily", SourceKind::KrxDailyMarket),
        src("kasi", SourceKind::KasiHoliday),
        src("krx-rule", SourceKind::KrxRule),
    ];
    let evidence = vec![
        ev("kasi-0601", "kasi", d(2012, 6, 1), EvidenceKind::HolidayFact, false),
        ev("rule-0601", "krx-rule", d(2012, 6, 1), EvidenceKind::DeterministicRule, false),
        ev("witness-0601", "krx-daily", d(2012, 6, 1), EvidenceKind::PositiveWitness, false),
        ev("witness-0603", "krx-daily", d(2012, 6, 3), EvidenceKind::PositiveWitness, false),
    ];
    let mut rows = Vec::new();
    let mut cursor = from;
    while cursor <= through {
        let (status, decisive) = match cursor {
            x if x == d(2012, 6, 1) => (DayStatus::TradingSession, vec!["witness-0601".to_string()]),
            x if x == d(2012, 6, 3) => (DayStatus::TradingSession, vec!["witness-0603".to_string()]),
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
    let base = prior_snapshot();
    let prior = stamp(Snapshot {
        sources,
        evidence,
        rows,
        ..base
    });

    // Incremental refresh gathering ONLY the single new date 2012-06-06, KRX_DAILY ok.
    let inputs = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![ev(
            "witness-0606",
            "krx-daily",
            d(2012, 6, 6),
            EvidenceKind::PositiveWitness,
            false,
        )],
        outcomes: vec![SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket)],
    };
    let out = refresh_incremental(
        &prior,
        &StaticEvidencePort::new(inputs),
        refresh_now(),
        d(2012, 6, 6),
    );
    let candidate = &out.candidate;

    // Prior witnesses on the NON-re-gathered dates are RETAINED.
    assert!(
        candidate.evidence.iter().any(|e| e.id == "witness-0601"),
        "a prior positive witness on a non-re-gathered date must be retained"
    );
    assert!(
        candidate.evidence.iter().any(|e| e.id == "witness-0603"),
        "a prior positive witness on a non-re-gathered date must be retained"
    );

    // The row-1 override date stays TradingSession — NOT reverted to inferred Closed.
    let row_0601 = candidate
        .rows
        .iter()
        .find(|r| r.date == d(2012, 6, 1))
        .unwrap();
    assert_eq!(
        row_0601.status,
        DayStatus::TradingSession,
        "a proven-open (witness-overrides-inference) day must not revert to Closed on a partial re-gather"
    );
    let row_0603 = candidate
        .rows
        .iter()
        .find(|r| r.date == d(2012, 6, 3))
        .unwrap();
    assert_eq!(row_0603.status, DayStatus::TradingSession);

    // The freshly re-gathered date is materialized as proven-open.
    let row_0606 = candidate
        .rows
        .iter()
        .find(|r| r.date == d(2012, 6, 6))
        .unwrap();
    assert_eq!(row_0606.status, DayStatus::TradingSession);
}

/// FIX 2 (security): the candidate + diff carry the same license-restricted KRX/KASI-derived
/// facts as the production snapshot, so both must be written owner-only (`0o600`), never at
/// the umask-default world-readable `0o644`.
#[test]
fn write_candidate_writes_owner_only_0600_files() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let active_path = dir.path().join("calendar.json");
    let prior = prior_snapshot();
    std::fs::write(&active_path, serde_json::to_vec_pretty(&prior).unwrap()).unwrap();

    let out = refresh(
        &prior,
        &StaticEvidencePort::new(ok_inputs_flip_0603_to_session()),
        RefreshScope {
            from: d(2012, 6, 1),
            through: d(2012, 6, 5),
        },
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );
    let artifacts = write_candidate(&active_path, &out).unwrap();

    let cand_mode = std::fs::metadata(&artifacts.candidate_path)
        .unwrap()
        .permissions()
        .mode();
    let diff_mode = std::fs::metadata(&artifacts.diff_path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(
        cand_mode & 0o777,
        0o600,
        "candidate must be owner-only 0o600, got {:o}",
        cand_mode & 0o777
    );
    assert_eq!(
        diff_mode & 0o777,
        0o600,
        "diff must be owner-only 0o600, got {:o}",
        diff_mode & 0o777
    );
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

// ---------------------------------------------------------------------------------------
// U1 (KTD2): per-source covered date-ranges gate evidence replacement. `SourceOutcome`
// gains an optional covered claim; absent = legacy scope-wide replacement, present-but-empty
// = replace nothing, present = replacement gated to `ranges ∩ scope` and never-retract-in-range.
// ---------------------------------------------------------------------------------------

/// The reconciled status of `date` in `snap`'s rows (panics if the date is not materialized).
fn row_status(snap: &Snapshot, date: NaiveDate) -> DayStatus {
    snap.rows
        .iter()
        .find(|r| r.date == date)
        .unwrap_or_else(|| panic!("no row for {date}"))
        .status
}

fn has_evidence(snap: &Snapshot, id: &str) -> bool {
    snap.evidence.iter().any(|e| e.id == id)
}

fn build_scope_full() -> RefreshScope {
    RefreshScope { from: d(2012, 6, 1), through: d(2012, 6, 5) }
}

#[test]
fn legacy_absent_covered_field_round_trips_and_omits_from_json() {
    // A legacy outcome serializes with NO `covered` key (byte-shape preserved for existing
    // `--inputs` files) and round-trips back to an absent (legacy) claim.
    let legacy = SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket);
    let json = serde_json::to_string(&legacy).unwrap();
    assert!(!json.contains("covered"), "legacy outcome must not emit a covered field: {json}");
    // A legacy inputs document (no `covered` field) still deserializes — to the absent
    // (legacy scope-wide) claim, never to present-but-empty.
    let back: SourceOutcome = serde_json::from_str(&json).unwrap();
    assert_eq!(back, legacy);
    assert!(back.covered().is_none(), "absent means legacy scope-wide semantics");

    // A present-but-empty claim is DISTINCT from absent and round-trips as such.
    let empty = SourceOutcome::ok_covering("krx-daily", SourceKind::KrxDailyMarket, vec![]);
    let empty_json = serde_json::to_string(&empty).unwrap();
    assert!(empty_json.contains("\"covered\":[]"), "present-but-empty must emit []: {empty_json}");
    let empty_back: SourceOutcome = serde_json::from_str(&empty_json).unwrap();
    assert_eq!(empty_back.covered(), Some(&[][..]));
}

#[test]
fn absent_covered_replaces_scope_wide_but_present_but_empty_replaces_nothing() {
    let prior = prior_snapshot();
    // A successful krx-daily gather that returns NO evidence for the in-scope witness date.
    let make_inputs = |outcome: SourceOutcome| RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![],
        outcomes: vec![outcome],
    };

    // Absent (legacy) → scope-wide replacement: the prior 2012-06-04 witness is dropped, so
    // the date reverts to Unknown (today's behavior, preserved verbatim).
    let legacy = build_candidate(
        &prior,
        &make_inputs(SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket)),
        &build_scope_full(),
        RefreshMode::Incremental,
        refresh_now(),
    );
    assert!(!has_evidence(&legacy, "witness-0604"), "legacy scope-wide replacement drops the witness");
    assert_eq!(row_status(&legacy, d(2012, 6, 4)), DayStatus::Unknown);

    // Present-but-empty → replace nothing: the prior witness survives, 2012-06-04 stays a
    // TradingSession.
    let empty = build_candidate(
        &prior,
        &make_inputs(SourceOutcome::ok_covering(
            "krx-daily",
            SourceKind::KrxDailyMarket,
            vec![],
        )),
        &build_scope_full(),
        RefreshMode::Incremental,
        refresh_now(),
    );
    assert!(has_evidence(&empty, "witness-0604"), "present-but-empty replaces nothing");
    assert_eq!(row_status(&empty, d(2012, 6, 4)), DayStatus::TradingSession);
}

#[test]
fn covered_ranges_do_not_drop_prior_witness_outside_the_covered_range() {
    // The retraction hazard: a source marked Ok whose covered ranges end before a prior
    // witness must NOT drop that witness. Here krx-daily covers only 2012-06-01..06-03; the
    // prior 2012-06-04 witness is outside the claim and must survive.
    let prior = prior_snapshot();
    let inputs = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![ev("witness-0602", "krx-daily", d(2012, 6, 2), EvidenceKind::PositiveWitness, false)],
        outcomes: vec![SourceOutcome::ok_covering(
            "krx-daily",
            SourceKind::KrxDailyMarket,
            vec![DateRange::new(d(2012, 6, 1), d(2012, 6, 3))],
        )],
    };
    let candidate = build_candidate(&prior, &inputs, &build_scope_full(), RefreshMode::Incremental, refresh_now());
    assert!(has_evidence(&candidate, "witness-0604"), "a witness outside the covered range is never dropped");
    assert_eq!(row_status(&candidate, d(2012, 6, 4)), DayStatus::TradingSession);
}

#[test]
fn prior_witness_survives_empty_response_inside_a_covered_range_but_yields_to_a_valid_refetch() {
    let prior = prior_snapshot();
    // (a) never-retract-in-range: krx-daily covers 2012-06-01..06-05 (INCLUDING 06-04) but the
    // fresh response for 06-04 is an explicit absence marker (valid == false). The prior
    // witness must survive — absence never retracts, even inside a covered range.
    let absence = EvidenceRecord {
        valid: false,
        ..ev("absence-0604", "krx-daily", d(2012, 6, 4), EvidenceKind::PositiveWitness, false)
    };
    let inputs_empty = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![absence],
        outcomes: vec![SourceOutcome::ok_covering(
            "krx-daily",
            SourceKind::KrxDailyMarket,
            vec![DateRange::new(d(2012, 6, 1), d(2012, 6, 5))],
        )],
    };
    let kept = build_candidate(&prior, &inputs_empty, &build_scope_full(), RefreshMode::Incremental, refresh_now());
    assert!(has_evidence(&kept, "witness-0604"), "an in-range empty response never retracts the prior witness");
    assert_eq!(row_status(&kept, d(2012, 6, 4)), DayStatus::TradingSession);

    // (b) a VALID re-attestation on the same in-range date DOES replace the prior record.
    let inputs_valid = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![ev("witness-0604-refetch", "krx-daily", d(2012, 6, 4), EvidenceKind::PositiveWitness, false)],
        outcomes: vec![SourceOutcome::ok_covering(
            "krx-daily",
            SourceKind::KrxDailyMarket,
            vec![DateRange::new(d(2012, 6, 1), d(2012, 6, 5))],
        )],
    };
    let replaced = build_candidate(&prior, &inputs_valid, &build_scope_full(), RefreshMode::Incremental, refresh_now());
    assert!(!has_evidence(&replaced, "witness-0604"), "a valid in-range refetch replaces the prior record");
    assert!(has_evidence(&replaced, "witness-0604-refetch"));
    assert_eq!(row_status(&replaced, d(2012, 6, 4)), DayStatus::TradingSession);
}

#[test]
fn a_failed_source_with_covered_ranges_still_takes_the_no_expansion_branch() {
    let prior = prior_snapshot();
    // A failed source carrying covered ranges (the R4 honesty carrier) must not expand
    // coverage — the materialized window stays at the predecessor's.
    let inputs = RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![],
        outcomes: vec![SourceOutcome::failed_covering(
            "krx-daily",
            SourceKind::KrxDailyMarket,
            "quota exhausted",
            vec![DateRange::new(d(2012, 6, 6), d(2012, 6, 8))],
        )],
    };
    // Scope extends past the prior window; a failed source must NOT let it grow.
    let scope = RefreshScope { from: d(2012, 6, 1), through: d(2012, 6, 10) };
    let candidate = build_candidate(&prior, &inputs, &scope, RefreshMode::Incremental, refresh_now());
    assert_eq!(
        candidate.coverage.materialized_through,
        prior.coverage.materialized_through,
        "a failed source cannot expand coverage even when it carries covered ranges"
    );
    // And the prior witness (from the failed source) is retained.
    assert!(has_evidence(&candidate, "witness-0604"));
}

#[test]
fn date_range_arithmetic_handles_gap_adjacent_and_overlapping() {
    let dr = DateRange::new;
    // contains
    let r = dr(d(2010, 1, 4), d(2010, 1, 10));
    assert!(r.contains(d(2010, 1, 4)) && r.contains(d(2010, 1, 10)) && r.contains(d(2010, 1, 7)));
    assert!(!r.contains(d(2010, 1, 3)) && !r.contains(d(2010, 1, 11)));

    // intersect: overlap yields the overlap, disjoint yields None.
    assert_eq!(
        dr(d(2010, 1, 1), d(2010, 1, 10)).intersect(&dr(d(2010, 1, 5), d(2010, 1, 20))),
        Some(dr(d(2010, 1, 5), d(2010, 1, 10)))
    );
    assert_eq!(
        dr(d(2010, 1, 1), d(2010, 1, 4)).intersect(&dr(d(2010, 1, 6), d(2010, 1, 9))),
        None
    );

    // merge_ranges: adjacent (through+1 == next.from) and overlapping coalesce; a gap stays.
    assert_eq!(
        merge_ranges(&[dr(d(2010, 1, 1), d(2010, 1, 5)), dr(d(2010, 1, 6), d(2010, 1, 10))]),
        vec![dr(d(2010, 1, 1), d(2010, 1, 10))],
        "adjacent ranges coalesce"
    );
    assert_eq!(
        merge_ranges(&[dr(d(2010, 1, 1), d(2010, 1, 5)), dr(d(2010, 1, 3), d(2010, 1, 8))]),
        vec![dr(d(2010, 1, 1), d(2010, 1, 8))],
        "overlapping ranges coalesce"
    );
    assert_eq!(
        merge_ranges(&[dr(d(2010, 1, 6), d(2010, 1, 8)), dr(d(2010, 1, 1), d(2010, 1, 3))]),
        vec![dr(d(2010, 1, 1), d(2010, 1, 3)), dr(d(2010, 1, 6), d(2010, 1, 8))],
        "a genuine gap stays two ranges, sorted"
    );

    // uncovered_within: containment (empty == fully covered) and the named-gap carrier.
    let window = dr(d(2010, 1, 1), d(2010, 1, 10));
    assert_eq!(
        uncovered_within(window, &[dr(d(2010, 1, 1), d(2010, 1, 3)), dr(d(2010, 1, 6), d(2010, 1, 8))]),
        vec![dr(d(2010, 1, 4), d(2010, 1, 5)), dr(d(2010, 1, 9), d(2010, 1, 10))],
        "interior and trailing gaps are named"
    );
    assert!(
        uncovered_within(window, &[dr(d(2010, 1, 1), d(2010, 1, 10))]).is_empty(),
        "a fully-covering range leaves no gap"
    );
    assert_eq!(
        uncovered_within(window, &[]),
        vec![window],
        "no coverage leaves the whole window uncovered"
    );
}
