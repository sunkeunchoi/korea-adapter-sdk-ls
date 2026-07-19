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
        adoption: CalendarAdoption::Legacy,
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

#[test]
fn u12_shadow_dispatch_record_is_byte_identical_to_legacy() {
    // With no stub and no configured snapshot, the weekday fact stays authoritative under
    // BOTH Legacy and Shadow; Shadow's calendar recording goes only to stderr (non-persisted),
    // so the persisted chain record is byte-identical.
    fn run_with(adoption: CalendarAdoption) -> Vec<u8> {
        let tmp = TempDir::new().unwrap();
        seed_genesis(tmp.path());
        let mut cfg = green_cfg(tmp.path());
        cfg.adoption = adoption;
        cfg.date_fact_stub = None; // exercise the resolution path
        let out = run_dispatch(&cfg).unwrap();
        assert_eq!(out.result, GateResult::Green);
        std::fs::read(DispatchChain::open(tmp.path()).unwrap().chain_path()).unwrap()
    }
    let legacy = run_with(CalendarAdoption::Legacy);
    let shadow = run_with(CalendarAdoption::Shadow);
    assert_eq!(legacy, shadow, "Shadow dispatch record is byte-identical to Legacy");
}

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
        .env_remove("LS_DISPATCH_NONCE");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

#[test]
fn bin_green_dispatch_exits_zero_and_records() {
    let tmp = TempDir::new().unwrap();
    seed_genesis(tmp.path());
    let out = bin_dispatch(tmp.path(), &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout={stdout} stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("DISPATCH green"), "{stdout}");
}

#[test]
fn bin_no_chain_exits_nonzero_and_names_genesis() {
    let tmp = TempDir::new().unwrap();
    let out = bin_dispatch(tmp.path(), &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("--genesis"));
}
