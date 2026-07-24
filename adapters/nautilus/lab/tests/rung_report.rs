//! U4 (rung-1 readiness) — `--rung-report` read-only post-session verification.
//!
//! Deterministic fixtures over a data home: clean/limit-event/head-mismatched classification, the
//! cumulative rung-1 P&L against the v34 band, N-progress, and the read-only invariant (the chain
//! + registry bytes are byte-identical before and after the report).

use chrono::{TimeZone, Utc};

use nautilus_ls_lab::artifacts::data_quality::DataQualityReport;
use nautilus_ls_lab::artifacts::manifest::{universe_hash, DataRange, DispatchLink, Manifest};
use nautilus_ls_lab::artifacts::performance::{PerformanceReport, TradeRecord};
use nautilus_ls_lab::artifacts::{RunSource, RunWriter};
use nautilus_ls_lab::dispatch::chain::{
    DispatchChain, DispatchOutcome, RecordKind, SessionDispatch,
};
use nautilus_ls_lab::dispatch::ladder::{build_rung_report, EscalationCheck};
use nautilus_ls_lab::dispatch::prereg::PreRegistration;
use nautilus_ls_lab::params::OrbParams;
use tempfile::TempDir;

fn now() -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_752_600_000, 0).unwrap()
}

/// The v34 head governed params (the sized levers a real rung-1 session carries).
fn v34_head_params() -> OrbParams {
    OrbParams {
        strategy_version: 34,
        risk_per_trade_krw: 299_340.0,
        entry_confirm: 1.0,
        or_width_max_atr: 0.666,
        breakeven_trigger_r: 0.41,
        gap_retention_min: 0.5,
        ..OrbParams::default()
    }
}

/// The v34 rung-1 pre-registration (band [-148k, +266k], N=3 for a compact fixture, K window 5).
fn v34_prereg(n: u32) -> PreRegistration {
    serde_json::from_value(serde_json::json!({
        "version": 2,
        "k_window": 5,
        "session_max_loss_krw": 300000.0,
        "exceedance": { "max_reconcile_advised": 1, "max_deferrals": 3, "max_coverage_gaps": 1 },
        "rungs": [
            { "rung": 1, "fraction": 0.10, "n_clean_sessions": n,
              "expectation_band": { "min_cum_pnl": -148000.0, "max_cum_pnl": 266000.0 } }
        ]
    }))
    .unwrap()
}

fn seed_chain(home: &std::path::Path) -> DispatchChain {
    let chain = DispatchChain::open(home).unwrap();
    chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain
}

fn green_dispatch(chain: &DispatchChain, rung: u8) -> String {
    chain
        .append(
            now(),
            rung,
            rung,
            None,
            RecordKind::SessionDispatch(SessionDispatch {
                outcome: DispatchOutcome::Green,
                checks: Vec::new(),
                deferrals: Vec::new(),
                readiness: None,
                unknown_override: None,
            }),
        )
        .unwrap()
        .body
        .record_id
}

/// Stage a finalized live-lane run bound to `dispatch_id` with `params`, `realized_pnl`, and an
/// optional dedup hit (a limit event, R14(d)).
fn stage_live_run(
    data: &std::path::Path,
    run_id: &str,
    rung: u8,
    dispatch_id: &str,
    realized_pnl: f64,
    params: &OrbParams,
    dedup_hits: u64,
) {
    let writer = RunWriter::new(data, run_id).unwrap();
    let manifest = Manifest {
        run_id: run_id.into(),
        source: RunSource::Live,
        strategy_id: params.strategy_id.clone(),
        strategy_version: params.strategy_version,
        params: params.clone(),
        data_range: DataRange { start: "20260724".into(), end: "20260724".into() },
        catalog_fingerprint: String::new(),
        universe_hash: universe_hash(&[]),
        strategy_code_hash: nautilus_ls_lab::artifacts::manifest::strategy_code_hash(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: Some(DispatchLink {
            dispatch_id: dispatch_id.into(),
            rung,
            rung_fraction: 0.10,
            lane: "cafef00d".into(),
            trading_env: "live".into(),
        }),
        created_utc: "2026-07-24T01:00:00Z".into(),
    };
    writer.write_manifest(&manifest).unwrap();
    let trade = TradeRecord {
        symbol: "005930.XKRX".into(),
        entry_side: "BUY".into(),
        quantity: 10.0,
        avg_px_open: 60_000.0,
        avg_px_close: Some(60_000.0 + realized_pnl / 10.0),
        realized_pnl,
        ts_opened: 1,
        ts_closed: Some(2),
        fills: Vec::new(),
        risk_capital: None,
        realized_r: None,
    };
    writer.write_performance(&PerformanceReport::assemble(vec![trade], 1_000_000.0)).unwrap();
    let mut dq = DataQualityReport::backtest(vec![], vec![]);
    dq.teardown_retries = Some(0);
    dq.dedup_hits = Some(dedup_hits);
    writer.write_data_quality(&dq).unwrap();
    writer.finalize().unwrap();
}

/// Stage a finalized backtest run (no dispatch link) — must be excluded from the trailing window.
fn stage_backtest_run(data: &std::path::Path, run_id: &str, params: &OrbParams) {
    let writer = RunWriter::new(data, run_id).unwrap();
    let manifest = Manifest {
        run_id: run_id.into(),
        source: RunSource::Backtest,
        strategy_id: params.strategy_id.clone(),
        strategy_version: params.strategy_version,
        params: params.clone(),
        data_range: DataRange { start: "20260724".into(), end: "20260724".into() },
        catalog_fingerprint: String::new(),
        universe_hash: universe_hash(&[]),
        strategy_code_hash: nautilus_ls_lab::artifacts::manifest::strategy_code_hash(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: None,
        created_utc: "2026-07-24T01:00:00Z".into(),
    };
    writer.write_manifest(&manifest).unwrap();
    writer.write_performance(&PerformanceReport::assemble(vec![], 1_000_000.0)).unwrap();
    writer.write_data_quality(&DataQualityReport::backtest(vec![], vec![])).unwrap();
    writer.finalize().unwrap();
}

/// Snapshot every file under `root` (path → bytes) for a byte-identity comparison.
fn snapshot_tree(root: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    fn walk(dir: &std::path::Path, out: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.insert(p.clone(), std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, &mut out);
    out
}

#[test]
fn rung_report_is_read_only_the_chain_and_registry_bytes_are_unchanged() {
    let tmp = TempDir::new().unwrap();
    let chain = seed_chain(tmp.path());
    let head = v34_head_params();
    for h in ["01", "02"] {
        let d = green_dispatch(&chain, 1);
        stage_live_run(tmp.path(), &format!("20260724T{h}0000Z-live-orb-v34"), 1, &d, 500.0, &head, 0);
    }
    let before = snapshot_tree(tmp.path());
    let _report = build_rung_report(tmp.path(), &chain.load().records, 1, &v34_prereg(3), None);
    let after = snapshot_tree(tmp.path());
    assert_eq!(before, after, "--rung-report appends nothing — the chain + registry bytes are unchanged");
}

#[test]
fn three_clean_v34_sessions_report_n_progress_and_cumulative_pnl() {
    let tmp = TempDir::new().unwrap();
    let chain = seed_chain(tmp.path());
    let head = v34_head_params();
    for (i, h) in ["01", "02", "03"].iter().enumerate() {
        let d = green_dispatch(&chain, 1);
        stage_live_run(tmp.path(), &format!("20260724T{h}0000Z-live-orb-v34"), 1, &d, 500.0 * (i as f64 + 1.0), &head, 0);
    }
    let report = build_rung_report(tmp.path(), &chain.load().records, 1, &v34_prereg(3), None);
    assert_eq!(report.clean.len(), 3, "3 clean rung-1 sessions");
    assert_eq!(report.n_required, 3);
    assert_eq!(report.cum_pnl, 500.0 + 1000.0 + 1500.0, "cumulative clean P&L");
    assert_eq!(report.band, (-148_000.0, 266_000.0), "the v34 rung-1 band");
    assert!(report.in_band, "3,000 is inside [-148k, +266k]");
    assert!(matches!(report.escalation, EscalationCheck::Ready { to_rung: 2, .. }), "N met + in band → ready");
    // The head hash the report evaluated under is populated (KTD6).
    assert_eq!(report.head_code_hash, nautilus_ls_lab::artifacts::manifest::strategy_code_hash());
}

#[test]
fn a_limit_event_session_is_classified_and_excluded_from_the_clean_count() {
    let tmp = TempDir::new().unwrap();
    let chain = seed_chain(tmp.path());
    let head = v34_head_params();
    let d1 = green_dispatch(&chain, 1);
    stage_live_run(tmp.path(), "20260724T010000Z-live-orb-v34", 1, &d1, 500.0, &head, 0); // clean
    let d2 = green_dispatch(&chain, 1);
    stage_live_run(tmp.path(), "20260724T020000Z-live-orb-v34", 1, &d2, 500.0, &head, 1); // dedup hit → limit event
    let report = build_rung_report(tmp.path(), &chain.load().records, 1, &v34_prereg(3), None);
    assert_eq!(report.clean.len(), 1, "only the clean session counts");
    assert_eq!(report.limit_event.len(), 1, "the dedup-hit session is a limit event");
    assert_eq!(report.cum_pnl, 500.0, "the limit-event session's P&L is excluded from the clean cum");
}

#[test]
fn cumulative_pnl_below_the_v34_floor_reads_outside_band() {
    let tmp = TempDir::new().unwrap();
    let chain = seed_chain(tmp.path());
    let head = v34_head_params();
    // One clean session with a P&L below the v34 floor (a normal-variance bad streak).
    let d = green_dispatch(&chain, 1);
    stage_live_run(tmp.path(), "20260724T010000Z-live-orb-v34", 1, &d, -200_000.0, &head, 0);
    let report = build_rung_report(tmp.path(), &chain.load().records, 1, &v34_prereg(3), None);
    assert!(!report.in_band, "-200,000 is below the v34 floor -148,000");
    assert_eq!(report.band.0, -148_000.0, "judged against the v34 band, not v30's -69,000");
}

#[test]
fn interleaved_backtests_are_excluded_from_the_trailing_window() {
    let tmp = TempDir::new().unwrap();
    let chain = seed_chain(tmp.path());
    let head = v34_head_params();
    stage_backtest_run(tmp.path(), "20260724T000000Z-backtest-orb-v34", &head);
    let d = green_dispatch(&chain, 1);
    stage_live_run(tmp.path(), "20260724T010000Z-live-orb-v34", 1, &d, 500.0, &head, 0);
    let report = build_rung_report(tmp.path(), &chain.load().records, 1, &v34_prereg(3), None);
    assert_eq!(report.clean.len(), 1, "only the live-lane session is in scope");
    assert!(report.limit_event.is_empty());
    assert!(report.head_mismatched.is_empty(), "the backtest is excluded, not head-mismatched");
}

#[test]
fn a_session_under_a_different_head_is_head_mismatched_not_counted() {
    let tmp = TempDir::new().unwrap();
    let chain = seed_chain(tmp.path());
    // A live-lane session whose governed params differ from the head (a different-params run).
    let mut other = v34_head_params();
    other.risk_per_trade_krw = 111_111.0;
    let d = green_dispatch(&chain, 1);
    stage_live_run(tmp.path(), "20260724T010000Z-live-orb-v34", 1, &d, 500.0, &other, 0);
    // With the head = the same `other` params (the data home's only finalized run), it matches;
    // so force a head mismatch by pointing the report at a home whose head is the default.
    let report = build_rung_report(tmp.path(), &chain.load().records, 1, &v34_prereg(3), None);
    // The head is resolved from the latest finalized run (this `other` session), so it MATCHES —
    // demonstrating the report keys on the data home's head. Now stage a genuinely different head.
    assert_eq!(report.clean.len(), 1, "keyed on the data home's own head, the session is clean");

    // A second home where the head is v34 but the session ran under a stale (default) head.
    let tmp2 = TempDir::new().unwrap();
    let chain2 = seed_chain(tmp2.path());
    stage_backtest_run(tmp2.path(), "20260724T090000Z-backtest-orb-v34", &v34_head_params()); // head = v34
    let d2 = green_dispatch(&chain2, 1);
    stage_live_run(tmp2.path(), "20260724T010000Z-live-orb-v34", 1, &d2, 500.0, &OrbParams::default(), 0); // stale head
    let report2 = build_rung_report(tmp2.path(), &chain2.load().records, 1, &v34_prereg(3), None);
    assert_eq!(report2.head_mismatched.len(), 1, "the stale-head session is shown head-mismatched");
    assert!(report2.clean.is_empty(), "a head-mismatched session is never silently counted");
}
