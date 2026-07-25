//! U3 — the `lab-live --dispatch` phase-1 gate (R1–R4).
//!
//! Verdicts are exercised through the library `run_dispatch`; the bin is exercised
//! end-to-end via `CARGO_BIN_EXE_lab-live`. Every attempt against a valid chain leaves a
//! record; a throttle records nothing (re-run); no chain directs to `--genesis`. No live
//! calls — the gateway probes are stubbed (the documented stubbed-binary seam).

use chrono::{TimeZone, Utc};
use nautilus_ls_calendar::schema::Citation;
use nautilus_ls_calendar::CalendarAdoption;
use nautilus_ls_lab::dispatch::chain::{DispatchChain, DispatchOutcome, Escalation, RecordKind};
use nautilus_ls_lab::dispatch::checks::{CalendarDateFact, GatewayProbe, GateResult, LanePosture};
use nautilus_ls_lab::dispatch::UnknownOverride;
use nautilus_ls_lab::runner::live::{run_dispatch, DispatchCliConfig};
use tempfile::TempDir;

/// A weekday, mid-session KST instant (2026-07-16 Thu 10:00 KST = 01:00 UTC).
fn weekday_ts() -> i64 {
    Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap().timestamp()
}

fn seed_genesis(home: &std::path::Path) {
    let chain = DispatchChain::open(home).unwrap();
    let now = Utc.timestamp_opt(weekday_ts(), 0).unwrap();
    chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();
}

fn green_cfg(home: &std::path::Path) -> DispatchCliConfig {
    let lane_env = home.join("lane.env");
    std::fs::write(&lane_env, "APPKEY=x\n").unwrap();
    DispatchCliConfig {
        data_home: home.to_path_buf(),
        requested_rung: 1,
        lane: LanePosture::Paper,
        lane_env_path: lane_env,
        trading_env: Some("paper".into()),
        deferrals: Vec::new(),
        nonce: None,
        now_unix: weekday_ts(),
        catalog_stub: Some("ok".into()),
        probe_stub: Some((GatewayProbe::Clear, GatewayProbe::Clear)),
        budget_stub: Some("ok".into()),
        budget_plan: 5,
        attended_override: None,
        readiness_stub: None,
        prereg_path: None,
        // U12: a stubbed proven Trading Session so the base green path is unchanged; the
        // stub is the Enforced offline seam (it wins over adoption resolution).
        adoption: CalendarAdoption::Enforced,
        run_id: Some("run-cli-1".into()),
        date_fact_stub: Some(CalendarDateFact::TradingSession),
        unknown_override: None,
    }
}

fn last_dispatch(home: &std::path::Path) -> Option<(DispatchOutcome, usize)> {
    let state = DispatchChain::open(home).unwrap().load();
    state.records.iter().rev().find_map(|r| match &r.body.kind {
        RecordKind::SessionDispatch(s) => Some((s.outcome, s.checks.len())),
        _ => None,
    })
}

#[test]
fn green_path_appends_a_record_with_per_check_outcomes() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let out = run_dispatch(&green_cfg(tmp.path())).unwrap();
    assert_eq!(out.result, GateResult::Green);
    assert!(out.appended);
    let (outcome, checks) = last_dispatch(tmp.path()).expect("a session-dispatch record");
    assert_eq!(outcome, DispatchOutcome::Green);
    assert!(checks >= 9, "every check recorded, got {checks}");
}

#[test]
fn non_deferrable_red_refuses_and_records_naming_the_checks() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.trading_env = None; // non-deferrable interlock red
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Refused);
    assert!(out.appended, "a refusal is chain history");
    assert!(out.lines.iter().any(|l| l.contains("trading_env_interlock")));
    assert_eq!(last_dispatch(tmp.path()).unwrap().0, DispatchOutcome::Refused);
}

#[test]
fn deferral_applies_only_when_attended_with_a_fresh_nonce() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());

    // Stranded (deferrable) blocked; named + fresh nonce + attended -> Green, recorded.
    let mut cfg = green_cfg(tmp.path());
    cfg.probe_stub = Some((GatewayProbe::Clear, GatewayProbe::Blocked("a resting order".into())));
    cfg.deferrals = vec!["stranded_orders".into()];
    cfg.nonce = Some(cfg.now_unix.to_string());
    cfg.attended_override = Some(true);
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Green);
    assert!(out.lines.iter().any(|l| l.contains("stranded_orders") && l.contains("DEFERRED")));
    // The record carries the deferral.
    let state = DispatchChain::open(tmp.path()).unwrap().load();
    let deferred = state.records.iter().rev().find_map(|r| match &r.body.kind {
        RecordKind::SessionDispatch(s) => Some(!s.deferrals.is_empty()),
        _ => None,
    });
    assert_eq!(deferred, Some(true));
}

#[test]
fn deferral_refused_without_attendance_even_with_a_nonce() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.probe_stub = Some((GatewayProbe::Clear, GatewayProbe::Blocked("a resting order".into())));
    cfg.deferrals = vec!["stranded_orders".into()];
    cfg.nonce = Some(cfg.now_unix.to_string());
    cfg.attended_override = Some(false); // no-TTY / unattended
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Refused);
    assert!(out.lines.iter().any(|l| l.contains("nonce rejected") || l.contains("unattended")));
}

#[test]
fn no_chain_directs_to_genesis_and_records_nothing() {
    let tmp = TempDir::new().unwrap();
    let out = run_dispatch(&green_cfg(tmp.path())).unwrap();
    assert_eq!(out.result, GateResult::Refused);
    assert!(!out.appended);
    assert!(out.lines.iter().any(|l| l.contains("--genesis")));
}

#[test]
fn throttle_is_a_rerun_never_recorded() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.probe_stub = Some((GatewayProbe::Throttled, GatewayProbe::Clear));
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Throttled);
    assert!(!out.appended, "a throttle is never a terminal record");
    // Only the genesis record exists.
    let state = DispatchChain::open(tmp.path()).unwrap().load();
    assert_eq!(state.records.len(), 1);
}

#[test]
fn planted_secret_never_reaches_output_or_the_record() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.probe_stub = Some((GatewayProbe::Blocked("acct 20187511401 open".into()), GatewayProbe::Clear));
    let out = run_dispatch(&cfg).unwrap();
    assert!(!out.lines.iter().any(|l| l.contains("20187511401")), "output scrubbed: {:?}", out.lines);
    let bytes = std::fs::read_to_string(DispatchChain::open(tmp.path()).unwrap().chain_path()).unwrap();
    assert!(!bytes.contains("20187511401"), "chain record scrubbed");
}

/// Seed a chain authorized at rung 2 (genesis → escalation).
fn seed_rung_2(home: &std::path::Path) {
    let chain = DispatchChain::open(home).unwrap();
    let now = Utc.timestamp_opt(weekday_ts(), 0).unwrap();
    chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();
    chain
        .append(
            now,
            2,
            2,
            None,
            RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: Vec::new() }),
        )
        .unwrap();
}

#[test]
fn red_readiness_forces_rung_1_probation_not_refusal() {
    // Covers R11 through the gate's ACTUAL check list: a red readiness proceeds at
    // effective rung 1 while the record carries both rungs — never a refusal.
    let tmp = TempDir::new().unwrap();
    seed_rung_2(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.requested_rung = 2;
    cfg.readiness_stub = Some("red".into());

    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Green, "probation proceeds, never refuses (R11)");
    assert!(out.lines.iter().any(|l| l.contains("probation")), "{:?}", out.lines);

    let state = DispatchChain::open(tmp.path()).unwrap().load();
    let rec = state
        .records
        .iter()
        .rev()
        .find(|r| matches!(r.body.kind, RecordKind::SessionDispatch(_)))
        .unwrap();
    assert_eq!(rec.body.chain_rung, 2, "chain-authorized rung preserved");
    assert_eq!(rec.body.effective_rung, 1, "forced to rung-1 probation");
    // The readiness summary rode the record.
    if let RecordKind::SessionDispatch(s) = &rec.body.kind {
        assert!(s.readiness.as_deref().unwrap_or("").contains("Red"), "readiness recorded");
    }
}

#[test]
fn green_readiness_runs_at_the_authorized_rung() {
    let tmp = TempDir::new().unwrap();
    seed_rung_2(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.requested_rung = 2;
    cfg.readiness_stub = Some("green".into());

    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Green);
    let state = DispatchChain::open(tmp.path()).unwrap().load();
    let rec = state
        .records
        .iter()
        .rev()
        .find(|r| matches!(r.body.kind, RecordKind::SessionDispatch(_)))
        .unwrap();
    assert_eq!(rec.body.effective_rung, 2, "green readiness → the authorized rung, no probation");
}

// ---------------------------------------------------------------------------
// U12 — Production Ladder date gate + attended Unknown override (end-to-end)
// ---------------------------------------------------------------------------

#[test]
fn u12_enforced_unknown_refuses_with_no_authorized_dispatch() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.adoption = CalendarAdoption::Enforced;
    cfg.date_fact_stub = Some(CalendarDateFact::Unknown);
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Refused);
    assert!(out.appended, "a refusal is chain history");
    assert!(out.lines.iter().any(|l| l.contains("calendar_date")), "{:?}", out.lines);
    assert_eq!(last_dispatch(tmp.path()).unwrap().0, DispatchOutcome::Refused);
}

#[test]
fn u12_enforced_unknown_override_greens_and_records_full_audit() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.adoption = CalendarAdoption::Enforced;
    cfg.date_fact_stub = Some(CalendarDateFact::Unknown);
    cfg.run_id = Some("run-cli-1".into());
    cfg.nonce = Some(cfg.now_unix.to_string());
    cfg.attended_override = Some(true);
    cfg.unknown_override = Some(UnknownOverride {
        kst_date: "2026-07-16".into(),
        run_id: "run-cli-1".into(),
        operator: "operator-alice".into(),
        authorized_at_unix: cfg.now_unix,
        snapshot_artifact_id: "artifact-abc".into(),
        snapshot_calendar_id: "calendar-abc".into(),
        alerts: vec!["alert-witness-vs-closure".into()],
        reason: "reviewed the cited first-party basis".into(),
        citation: Citation { reference: "KRX-NOTICE-01".into(), issuer: "KRX".into(), note: None },
    });
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Green, "{:?}", out.lines);
    assert!(out.lines.iter().any(|l| l.contains("override applied")), "{:?}", out.lines);
    // The full audit rode the persisted chain record.
    let state = DispatchChain::open(tmp.path()).unwrap().load();
    let ov = state
        .records
        .iter()
        .rev()
        .find_map(|r| match &r.body.kind {
            RecordKind::SessionDispatch(s) => s.unknown_override.clone(),
            _ => None,
        })
        .expect("the override audit is recorded on the session-dispatch");
    assert_eq!(ov.kst_date, "2026-07-16");
    assert_eq!(ov.run_id, "run-cli-1");
    assert_eq!(ov.operator, "operator-alice");
    assert_eq!(ov.snapshot_artifact_id, "artifact-abc");
    assert_eq!(ov.snapshot_calendar_id, "calendar-abc");
    assert_eq!(ov.citation.reference, "KRX-NOTICE-01");
    assert_eq!(ov.citation.issuer, "KRX");
}

#[test]
fn u12_unknown_override_refused_when_unattended() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.adoption = CalendarAdoption::Enforced;
    cfg.date_fact_stub = Some(CalendarDateFact::Unknown);
    cfg.run_id = Some("run-cli-1".into());
    cfg.nonce = Some(cfg.now_unix.to_string());
    cfg.attended_override = Some(false); // no-TTY / unattended → the override cannot apply
    cfg.unknown_override = Some(UnknownOverride {
        kst_date: "2026-07-16".into(),
        run_id: "run-cli-1".into(),
        operator: "operator-alice".into(),
        authorized_at_unix: cfg.now_unix,
        snapshot_artifact_id: "artifact-abc".into(),
        snapshot_calendar_id: "calendar-abc".into(),
        alerts: Vec::new(),
        reason: "reviewed the cited first-party basis".into(),
        citation: Citation { reference: "KRX-NOTICE-01".into(), issuer: "KRX".into(), note: None },
    });
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Refused, "{:?}", out.lines);
    assert!(out.lines.iter().any(|l| l.contains("override rejected")), "{:?}", out.lines);
}

// (The Shadow==Legacy byte-identity test was retired with the Ladder Enforced-only cutover —
//  the date gate no longer has a Legacy/Shadow path.)

// ---------------------------------------------------------------------------
// Bin-level dispatch (CARGO_BIN_EXE_lab-live)
// ---------------------------------------------------------------------------

fn bin_dispatch(home: &std::path::Path, extra: &[(&str, &str)]) -> std::process::Output {
    let lane_env = home.join("lane.env");
    std::fs::write(&lane_env, "APPKEY=x\n").unwrap();
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_lab-live"));
    cmd.arg("--dispatch")
        .env("LS_DATA_HOME", home)
        .env("LS_TRADING_ENV", "paper")
        .env("LS_DISPATCH_LANE", "paper")
        .env("LS_DISPATCH_LANE_ENV", &lane_env)
        .env("LS_DISPATCH_STUB_PROBES", "clear,clear")
        .env("LS_DISPATCH_STUB_CATALOG", "ok")
        .env("LS_DISPATCH_STUB_BUDGET", "ok")
        .env("LS_DISPATCH_NOW_UNIX", weekday_ts().to_string())
        .env_remove("LS_DISPATCH_DEFER")
        .env_remove("LS_DISPATCH_NONCE")
        // Hermetic calendar env: each case sets adoption/snapshot explicitly via `extra`.
        .env_remove("LS_CALENDAR_SNAPSHOT")
        .env_remove("LS_CALENDAR_ADOPTION");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

#[test]
fn bin_green_dispatch_exits_zero_and_records() {
    use nautilus_ls_calendar::schema::DayStatus;
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    // Enforced-only: a green dispatch needs a snapshot proving the KST date a Trading Session
    // (no weekday fallback). 2026-07-16 is a proven session in the fixture.
    let snap = write_now_relative_snapshot(tmp.path(), DayStatus::TradingSession);
    let out = bin_dispatch(tmp.path(), &[("LS_CALENDAR_SNAPSHOT", snap.to_str().unwrap())]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout={stdout} stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("DISPATCH green"), "{stdout}");
}

#[test]
fn bin_no_chain_exits_nonzero_and_names_genesis() {
    let tmp = TempDir::new().unwrap();
    let out = bin_dispatch(tmp.path(), &[("LS_CALENDAR_ADOPTION", "shadow")]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("--genesis"));
    // The mandatory startup diagnostic fires on the no-chain refusal path too — the emit is
    // hoisted ABOVE run_dispatch's NoChain early return, so exactly one calendar-startup line
    // still appears (the retired main_cli emit used to be the only thing covering this path).
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("calendar-startup").count(),
        1,
        "the startup record fires even when the chain is absent: {stderr}"
    );
    assert!(stderr.contains("consumer=lab-live-dispatch"), "{stderr}");
}

// ---------------------------------------------------------------------------
// U3 (#188) — dispatch composition-root smoke: explicit config → single load →
// injection → startup diagnostic → adoption reporting, with NO production-snapshot
// dependency in CI (fixture-only + not-configured). The fixture lives only in a TempDir;
// no test reads any path under `adapters/nautilus/state/` or a committed snapshot.
// ---------------------------------------------------------------------------

/// A human-shaped granting authority the redaction guard must never see reach stderr
/// (mirrors `calendar_composition.rs` SECRET_AUTHORITY).
const SECRET_AUTHORITY: &str = "Jane Doe / Agreement-7";

/// Write a valid snapshot bracketing the pinned `weekday_ts()` KST date (2026-07-16) whose
/// mid row carries `mid_status`, re-dated to load in-range/authorized at the harness's 2026
/// `now` (the illustrative 2010/2012 `write_snapshot` in `calendar_composition.rs` would load
/// out-of-range → EnforcedFailClosed). Returns the TempDir-only path.
fn write_now_relative_snapshot(
    dir: &std::path::Path,
    mid_status: nautilus_ls_calendar::schema::DayStatus,
) -> std::path::PathBuf {
    // Fresh horizon: well past the 45-day forward-readiness threshold from the pinned now.
    write_snapshot_with_horizon(dir, mid_status, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap())
}

/// As [`write_now_relative_snapshot`] but with an explicit forward-readiness horizon, so a
/// caller can build a STALE fixture (horizon within 45 days of the pinned now → `freshness=stale`).
fn write_snapshot_with_horizon(
    dir: &std::path::Path,
    mid_status: nautilus_ls_calendar::schema::DayStatus,
    forward_through: chrono::NaiveDate,
) -> std::path::PathBuf {
    use nautilus_ls_calendar::schema::{
        Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness, Snapshot,
        SourceAvailabilityBound,
    };
    use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id};
    let d = |y, m, day| chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap();
    let mut snap = Snapshot {
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
            granted_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
            expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: d(2026, 7, 15),
            materialized_through: d(2026, 7, 17),
            retrospectively_checked_through: d(2026, 7, 17),
            scheduled_closure_evaluated_through: d(2026, 7, 17),
            source_availability: vec![SourceAvailabilityBound {
                source_id: "s".to_string(),
                available_from: None,
                available_through: None,
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap(),
            holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap()),
            full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap()),
            forward_readiness_through: Some(forward_through),
            last_incremental_at: Some(Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap()),
        },
        sources: vec![],
        evidence: vec![],
        alerts: vec![],
        rows: vec![
            DayRow { date: d(2026, 7, 15), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
            DayRow { date: d(2026, 7, 16), status: mid_status, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
            DayRow { date: d(2026, 7, 17), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
        ],
    };
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    let path = dir.join("calendar.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&snap).unwrap()).unwrap();
    path
}

// (The Shadow-greens-on-weekday-fixture test was retired with the Ladder Enforced-only cutover.)

#[test]
fn bin_enforced_trading_session_proceeds_through_the_calendar_fact() {
    use nautilus_ls_calendar::schema::DayStatus;
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let snap = write_now_relative_snapshot(tmp.path(), DayStatus::TradingSession);
    let out = bin_dispatch(
        tmp.path(),
        &[("LS_CALENDAR_ADOPTION", "enforced"), ("LS_CALENDAR_SNAPSHOT", snap.to_str().unwrap())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "Enforced Trading Session greens: stdout={stdout} stderr={stderr}");
    assert_eq!(stderr.matches("calendar-startup").count(), 1, "{stderr}");
    assert!(stderr.contains("adoption=enforced"), "{stderr}");
    assert!(stderr.contains("action=enforced-active"), "{stderr}");
    assert!(stderr.contains("day=2026-07-16:TradingSession"), "{stderr}");
}

#[test]
fn bin_enforced_closed_refuses_with_calendar_active_diagnostic() {
    use nautilus_ls_calendar::schema::DayStatus;
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    // 2026-07-16 is a weekday, but the calendar proves it Closed — Enforced refuses.
    let snap = write_now_relative_snapshot(tmp.path(), DayStatus::Closed);
    let out = bin_dispatch(
        tmp.path(),
        &[("LS_CALENDAR_ADOPTION", "enforced"), ("LS_CALENDAR_SNAPSHOT", snap.to_str().unwrap())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "Enforced Closed refuses: stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("DISPATCH refused"), "{stdout}");
    assert_eq!(stderr.matches("calendar-startup").count(), 1, "{stderr}");
    assert!(stderr.contains("day=2026-07-16:Closed"), "{stderr}");
    assert!(stderr.contains("action=enforced-active"), "the calendar is authoritative: {stderr}");
}

// (The Legacy weekday-authoritative bin test was retired with the Ladder Enforced-only cutover.)

#[test]
fn bin_enforced_corrupt_snapshot_fails_closed() {
    // AC5: a corrupt snapshot (not just a missing one) → Unavailable → EnforcedFailClosed,
    // no weekday fallback. Exercises a different LoadFailure than the missing-path case.
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let snap = tmp.path().join("calendar.json");
    std::fs::write(&snap, b"{ not valid snapshot json").unwrap();
    let out = bin_dispatch(
        tmp.path(),
        &[("LS_CALENDAR_ADOPTION", "enforced"), ("LS_CALENDAR_SNAPSHOT", snap.to_str().unwrap())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a corrupt snapshot fails closed under Enforced: stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("DISPATCH refused"), "{stdout}");
    assert_eq!(stderr.matches("calendar-startup").count(), 1, "{stderr}");
    assert!(stderr.contains("action=enforced-fail-closed"), "no weekday fallback: {stderr}");
}

#[test]
fn bin_no_snapshot_configured_fails_closed_and_reads_no_production_snapshot() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    // Enforced-only: with no LS_CALENDAR_SNAPSHOT the calendar is Unavailable → the date gate
    // fails closed (refused), with NO weekday fallback and NO production snapshot read (proves
    // the no-production-dependency claim). The startup record still fires exactly once.
    let out = bin_dispatch(tmp.path(), &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "no-snapshot Enforced fails closed: stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("DISPATCH refused"), "{stdout}");
    assert_eq!(stderr.matches("calendar-startup").count(), 1, "{stderr}");
    assert!(stderr.contains("snapshot=not-configured"), "{stderr}");
    assert!(stderr.contains("action=enforced-fail-closed"), "no weekday fallback: {stderr}");
    assert!(stderr.contains("consumer=lab-live-dispatch"), "{stderr}");
}

// ---------------------------------------------------------------------------
// U4 (#188) — AC-to-test coverage-gap closure at the COMPOSITION ROOT (not just the pure
// check): AC11 (paired failure-inversion), AC8/AC12 (override refusal across classes), and
// AC9 (stale surfaced via the diagnostic — `check_calendar_date` drops the freshness
// dimension, so the startup/gate diagnostic is the only surfacing mechanism).
// ---------------------------------------------------------------------------

/// AC11 — failure-inversion at the gate: an Enforced Unknown date authorizes NO dispatch;
/// the same context with ONLY the calendar row flipped to Trading Session greens.
#[test]
fn u188_cli_failure_inversion_unknown_refuses_but_trading_greens() {
    // Enforced Unknown → refused, no authorized (Green) dispatch appended.
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let mut cfg = green_cfg(tmp.path());
    cfg.adoption = CalendarAdoption::Enforced;
    cfg.date_fact_stub = Some(CalendarDateFact::Unknown);
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Refused, "{:?}", out.lines);
    assert_eq!(last_dispatch(tmp.path()).unwrap().0, DispatchOutcome::Refused, "no authorized dispatch");

    // Flip ONLY the calendar fact to Trading Session (window + every other gate unchanged) → Green.
    let tmp2 = TempDir::new().unwrap();
    seed_genesis(tmp2.path());
    let mut cfg = green_cfg(tmp2.path());
    cfg.adoption = CalendarAdoption::Enforced;
    cfg.date_fact_stub = Some(CalendarDateFact::TradingSession);
    let out = run_dispatch(&cfg).unwrap();
    assert_eq!(out.result, GateResult::Green, "the same context with a Trading Session authorizes: {:?}", out.lines);
    assert_eq!(last_dispatch(tmp2.path()).unwrap().0, DispatchOutcome::Green);
}

/// AC8/AC12 — an attended, well-formed `UnknownOverride` cannot green a proven Closed or a
/// Unavailable date at the gate (the override is Unknown-only; refusal across classes).
#[test]
fn u188_cli_override_cannot_green_closed_or_unavailable() {
    fn override_for(kst_date: &str, run_id: &str, now_unix: i64) -> UnknownOverride {
        UnknownOverride {
            kst_date: kst_date.into(),
            run_id: run_id.into(),
            operator: "operator-alice".into(),
            authorized_at_unix: now_unix,
            snapshot_artifact_id: "artifact-abc".into(),
            snapshot_calendar_id: "calendar-abc".into(),
            alerts: Vec::new(),
            reason: "reviewed the cited first-party basis".into(),
            citation: Citation { reference: "KRX-NOTICE-01".into(), issuer: "KRX".into(), note: None },
        }
    }
    for fact in [CalendarDateFact::Closed, CalendarDateFact::Unavailable] {
        let tmp = TempDir::new().unwrap();
        seed_genesis(tmp.path());
        let mut cfg = green_cfg(tmp.path());
        cfg.adoption = CalendarAdoption::Enforced;
        cfg.date_fact_stub = Some(fact);
        cfg.run_id = Some("run-cli-1".into());
        cfg.nonce = Some(cfg.now_unix.to_string());
        cfg.attended_override = Some(true);
        cfg.unknown_override = Some(override_for("2026-07-16", "run-cli-1", cfg.now_unix));
        let out = run_dispatch(&cfg).unwrap();
        assert_eq!(
            out.result,
            GateResult::Refused,
            "an override cannot proceed a {fact:?} date: {:?}",
            out.lines
        );
    }
}

/// AC9 — staleness is surfaced through the diagnostic, independent of the day status:
/// a stale Trading Session still greens (with `freshness=stale` in the diagnostic), a stale
/// Closed still refuses, and a stale Unknown still refuses by default (needs the override).
#[test]
fn u188_cli_stale_freshness_surfaced_independent_of_day_status() {
    use nautilus_ls_calendar::schema::DayStatus;
    // A horizon 2 days past the pinned now → < 45 evaluated days remaining → stale.
    let stale_horizon = chrono::NaiveDate::from_ymd_opt(2026, 7, 18).unwrap();

    // Stale Trading Session (Enforced): gate GREENS, diagnostic carries freshness=stale.
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let snap = write_snapshot_with_horizon(tmp.path(), DayStatus::TradingSession, stale_horizon);
    let out = bin_dispatch(tmp.path(), &[("LS_CALENDAR_ADOPTION", "enforced"), ("LS_CALENDAR_SNAPSHOT", snap.to_str().unwrap())]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "a stale Trading Session still greens: {stderr}");
    assert!(stderr.contains("freshness=stale"), "staleness surfaced in the diagnostic: {stderr}");
    assert!(stderr.contains("day=2026-07-16:TradingSession"), "{stderr}");

    // Stale Closed (Enforced): still refuses; staleness surfaced.
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let snap = write_snapshot_with_horizon(tmp.path(), DayStatus::Closed, stale_horizon);
    let out = bin_dispatch(tmp.path(), &[("LS_CALENDAR_ADOPTION", "enforced"), ("LS_CALENDAR_SNAPSHOT", snap.to_str().unwrap())]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a stale Closed still refuses: {stderr}");
    assert!(stderr.contains("freshness=stale"), "{stderr}");

    // Stale Unknown (Enforced): refuses by default (needs the exact override); staleness surfaced.
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let snap = write_snapshot_with_horizon(tmp.path(), DayStatus::Unknown, stale_horizon);
    let out = bin_dispatch(tmp.path(), &[("LS_CALENDAR_ADOPTION", "enforced"), ("LS_CALENDAR_SNAPSHOT", snap.to_str().unwrap())]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a stale Unknown refuses without the override: {stderr}");
    assert!(stderr.contains("freshness=stale"), "{stderr}");
    assert!(stderr.contains("day=2026-07-16:Unknown"), "{stderr}");
}

// ---------------------------------------------------------------------------
// Bin-level `--mount` — the paper interlock + attended gate fire through the CLI with
// distinct exit codes (the "never look-like-ran" discipline).
//
// `--mount` now DRIVES the session (live-session-driver U5), but the bin can never reach
// the driven path from a test: the attendance gate refuses every no-TTY shell and cannot
// be suppressed from the environment by design. So the bin covers the refusal codes, the
// pre-consume prechecks are covered at the library seam (`live_wiring.rs::prepare_mount`,
// which structurally cannot consume), and the real end-to-end run is the operator-attended
// paper session — outside the gate, which never drives `node.run`.
// ---------------------------------------------------------------------------

fn bin_mount(home: &std::path::Path, extra: &[(&str, &str)]) -> std::process::Output {
    let lane_env = home.join("lane.env");
    std::fs::write(&lane_env, "APPKEY=x\n").unwrap();
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_lab-live"));
    cmd.arg("--mount")
        .env("LS_DATA_HOME", home)
        .env("LS_DISPATCH_LANE_ENV", &lane_env)
        .env("LS_DISPATCH_NOW_UNIX", weekday_ts().to_string())
        .env_remove("LS_TRADING_ENV")
        .env_remove("LS_DISPATCH_NONCE")
        .env_remove("LS_CALENDAR_SNAPSHOT")
        .env_remove("LS_CALENDAR_ADOPTION");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

#[test]
fn bin_mount_refuses_unless_paper_with_a_distinct_exit() {
    // Paper interlock FIRST (R3): LS_TRADING_ENV unset → distinct exit 66, before any chain read.
    let tmp = TempDir::new().unwrap();
    let out = bin_mount(tmp.path(), &[]);
    assert_eq!(out.status.code(), Some(66), "distinct paper-interlock exit code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("LS_TRADING_ENV must be `paper`"), "{stderr}");
}

#[test]
fn bin_mount_refuses_in_a_no_tty_shell_with_a_distinct_exit() {
    // Attended gate (R3): paper set, but a subprocess is a no-TTY/unattended shell → loud refusal
    // with a distinct exit code (77), never a look-like-ran success.
    let tmp = TempDir::new().unwrap();
    let out = bin_mount(tmp.path(), &[("LS_TRADING_ENV", "paper")]);
    assert_eq!(out.status.code(), Some(77), "distinct attended-refusal exit code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("mount refused"), "{stderr}");
}

/// The `MOUNT_PREPARED_DEFERRED` bail is GONE (live-session-driver DoD): no reachable
/// `--mount` path prints the deferred-driver notice or exits 70 any more.
#[test]
fn bin_mount_no_longer_reports_a_deferred_driver() {
    let tmp = TempDir::new().unwrap();
    for extra in [vec![], vec![("LS_TRADING_ENV", "paper")]] {
        let out = bin_mount(tmp.path(), &extra);
        assert_ne!(out.status.code(), Some(70), "the prepared-but-deferred exit code is retired");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!text.contains("deferred"), "the deferred-driver notice is gone: {text}");
        assert!(!text.contains("mount prepared"), "the prepared-not-run notice is gone: {text}");
    }
}

/// The attendance refusal happens BEFORE any mount input is resolved, so a `--mount` with
/// no pre-registration, no universe and no keepalive still exits 77 — never the
/// pre-consume-precheck code. Ordering is the safety property: nothing is consumed and no
/// gateway credential is touched until an operator has confirmed.
#[test]
fn bin_mount_refuses_attendance_before_resolving_any_mount_input() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let out = bin_mount(
        tmp.path(),
        &[("LS_TRADING_ENV", "paper"), ("LS_MOUNT_SESSION_SECS", "1")],
    );
    assert_eq!(out.status.code(), Some(77), "the attendance gate fires first");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("pre-consume"),
        "no precheck ran — the attendance gate refused first: {stderr}"
    );
    // Nothing was appended: the genesis record is still the whole chain.
    let chain = nautilus_ls_lab::dispatch::chain::DispatchChain::open(tmp.path()).unwrap();
    assert_eq!(chain.load().records.len(), 1, "a refused mount appends nothing");
}

#[test]
fn bin_bare_invocation_points_at_mount_not_u6() {
    // The bare-invocation "lands in U6" bail is gone (DoD): paper set, no subcommand → guidance
    // that names --mount and no longer references U6.
    let tmp = TempDir::new().unwrap();
    let lane_env = tmp.path().join("lane.env");
    std::fs::write(&lane_env, "APPKEY=x\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lab-live"))
        .env("LS_TRADING_ENV", "paper")
        .env("LS_DATA_HOME", tmp.path())
        .env_remove("LS_CALENDAR_SNAPSHOT")
        .env_remove("LS_CALENDAR_ADOPTION")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--mount"), "bare guidance names --mount: {stderr}");
    assert!(!stderr.contains("U6"), "the 'lands in U6' bail is gone: {stderr}");
}

// ---------------------------------------------------------------------------
// Bin-level ladder + diagnostic CLI (U3, rung-1 readiness): --head / --escalate /
// --reregister / --clear-killswitch. Nonce-gated arms refuse loudly with distinct exit
// codes in a no-TTY shell; --head is read-only.
// ---------------------------------------------------------------------------

fn bin_lab_live(arg: &str, home: &std::path::Path, extra: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_lab-live"));
    cmd.arg(arg)
        .env("LS_DATA_HOME", home)
        .env("LS_DISPATCH_NOW_UNIX", weekday_ts().to_string())
        .env_remove("LS_DISPATCH_NONCE")
        .env_remove("LS_DISPATCH_REASON")
        .env_remove("LS_DISPATCH_RUNG")
        .env_remove("LS_CALENDAR_SNAPSHOT")
        .env_remove("LS_CALENDAR_ADOPTION");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

#[test]
fn bin_head_prints_the_code_hash_and_is_read_only() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let before = std::fs::read_to_string(tmp.path().join("dispatch").join("chain.jsonl")).unwrap_or_default();
    let out = bin_lab_live("--head", tmp.path(), &[]);
    assert!(out.status.success(), "head is a read-only diagnostic (exit 0)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("strategy_code_hash="), "{stdout}");
    assert!(stdout.contains("d7a9820b"), "frames the check against the documented v34 head: {stdout}");
    assert!(stdout.contains("NOT a v34 confirmation"), "the params-hash line is labeled version-invariant: {stdout}");
    // Read-only: the chain is untouched.
    let after = std::fs::read_to_string(tmp.path().join("dispatch").join("chain.jsonl")).unwrap_or_default();
    assert_eq!(before, after, "--head appends nothing to the chain");
}

#[test]
fn bin_escalate_refuses_in_no_tty_with_a_distinct_code() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let prereg = tmp.path().join("prereg.json");
    std::fs::write(&prereg, br#"{"version":2,"rungs":[{"rung":1,"fraction":0.1,"n_clean_sessions":5,"expectation_band":{"min_cum_pnl":-148000.0,"max_cum_pnl":266000.0}}]}"#).unwrap();
    // No-TTY + no nonce → the nonce gate refuses; distinct exit 78, nothing escalated.
    let out = bin_lab_live("--escalate", tmp.path(), &[("LS_DISPATCH_PREREG", prereg.to_str().unwrap())]);
    assert_eq!(out.status.code(), Some(78), "distinct escalate-refusal exit code");
}

#[test]
fn bin_reregister_refuses_an_upward_jump_past_the_earned_rung() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path()); // genesis at rung 1 → earned rung = 1
    // Target rung 3 > earned 1 → refused BEFORE the nonce gate, distinct exit 79.
    let out = bin_lab_live(
        "--reregister",
        tmp.path(),
        &[("LS_DISPATCH_RUNG", "3"), ("LS_DISPATCH_REASON", "attempted jump")],
    );
    assert_eq!(out.status.code(), Some(79), "distinct reregister-refusal exit code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("exceeds the re-registration ceiling"), "{stderr}");
}

#[test]
fn bin_reregister_cannot_restore_a_de_escalated_peak() {
    // Regression: after escalate 1->2 then de-escalate 2->1, the ceiling is the CURRENT authorized
    // rung (1), NOT the historical peak (2). Restoring rung 2 must be refused — it has to be
    // re-earned through the escalation evidence gate (R15), never handed back by a bare re-register.
    use chrono::{TimeZone, Utc};
    use nautilus_ls_lab::dispatch::chain::{DeEscalation, DispatchChain, Escalation, RecordKind};
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    let t = Utc.timestamp_opt(weekday_ts(), 0).unwrap();
    chain.append(t, 1, 1, None, RecordKind::Genesis).unwrap();
    chain
        .append(t, 2, 2, None, RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec![] }))
        .unwrap();
    chain
        .append(t, 1, 1, None, RecordKind::DeEscalation(DeEscalation { from_rung: 2, to_rung: 1, events: vec!["x".into()], consumed_through: "z".into() }))
        .unwrap();
    assert_eq!(chain.load().authorized_rung, 1, "de-escalated back to rung 1");
    // Target the de-escalated peak (rung 2) -> refused (ceiling is the current rung 1).
    let out = bin_lab_live("--reregister", tmp.path(), &[("LS_DISPATCH_RUNG", "2"), ("LS_DISPATCH_REASON", "restore attempt")]);
    assert_eq!(out.status.code(), Some(79), "restoring a de-escalated peak is refused");
    assert!(String::from_utf8_lossy(&out.stderr).contains("exceeds the re-registration ceiling"));
}

#[test]
fn bin_clear_killswitch_refuses_in_no_tty_with_a_distinct_code() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    // No-TTY + no nonce → refused; distinct exit 80.
    let out = bin_lab_live("--clear-killswitch", tmp.path(), &[("LS_DISPATCH_REASON", "re-arm after reconcile")]);
    assert_eq!(out.status.code(), Some(80), "distinct clear-killswitch-refusal exit code");
}

#[test]
fn bin_rung_report_prints_the_head_hash_and_is_read_only() {
    // U4: --rung-report is an agent-runnable, read-only diagnostic (exit 0) that prints the head
    // hash it evaluated under (KTD6). With an empty rung-1 chain it reports 0 clean sessions.
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let prereg = tmp.path().join("prereg.json");
    std::fs::write(&prereg, br#"{"version":2,"k_window":5,"rungs":[{"rung":1,"fraction":0.1,"n_clean_sessions":5,"expectation_band":{"min_cum_pnl":-148000.0,"max_cum_pnl":266000.0}}]}"#).unwrap();
    let before = std::fs::read_to_string(tmp.path().join("dispatch").join("chain.jsonl")).unwrap_or_default();
    let out = bin_lab_live("--rung-report", tmp.path(), &[("LS_DISPATCH_PREREG", prereg.to_str().unwrap())]);
    assert!(out.status.success(), "rung-report is read-only (exit 0): {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("rung-report head_code_hash="), "{stdout}");
    assert!(stdout.contains("d7a9820b"), "frames against the documented v34 head: {stdout}");
    assert!(stdout.contains("clean=0/5"), "empty rung-1 chain → 0/5 clean: {stdout}");
    let after = std::fs::read_to_string(tmp.path().join("dispatch").join("chain.jsonl")).unwrap_or_default();
    assert_eq!(before, after, "--rung-report appends nothing to the chain");
}
