//! U6 — live paper runner wiring tests. Offline, credential-free: the strategy mounts
//! in a built LiveNode (no `node.run` — the repo never drives a full LiveNode
//! offline), a scripted fill flows through the exec-client seam (FillDelta) into a
//! live registry run, and the fail-closed teardown + reconcile-advised flag are
//! exercised without touching the gateway. The full live session is the operator's to
//! run (see the README recipe).

use chrono::{TimeZone, Utc};
use ls_core::{Environment as LsEnvironment, LsConfig};
use nautilus_common::enums::Environment;
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::factories::{LsDataClientFactory, LsExecutionClientFactory};
use nautilus_ls::ingest::BarKind;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::orders::ledger::FillDelta;
use nautilus_ls::orders::poll::{DriveTerminal, DrivenOutcome};
use nautilus_live::node::LiveNode;
use nautilus_ls_lab::artifacts::data_quality::DataQualityReport;
use nautilus_ls_lab::artifacts::manifest::{universe_hash, DataRange, Manifest};
use nautilus_ls_lab::artifacts::performance::{FillRecord, PerformanceReport, TradeRecord};
use nautilus_ls_lab::artifacts::{run_id, RunSource, RunWriter, DATA_QUALITY_FILE, MANIFEST_FILE, PERFORMANCE_FILE};
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::runner::live::{count_approximated, live_guard, record_reconcile};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::strategy::orb::{OrbStrategy, SelectedSymbol, SessionGapPrices};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, TraderId, TradeId};
use std::sync::{Mutex, MutexGuard};
use tempfile::tempdir;

/// Serializes the tests that build a Nautilus `LiveNode` (each initializes the process-global
/// logger via a non-atomic check-then-set). Two builds racing in parallel test threads trip
/// "a non-Nautilus logger is already registered" intermittently — deterministically red on some
/// CI schedulers, green on others. Holding this lock across each `.build()` makes the logger
/// init atomic across them, so a serialized second build sees the (own) logger already set and
/// tolerates it. Poison-tolerant: a panicking node-building test must not wedge the others.
static NODE_BUILD_LOCK: Mutex<()> = Mutex::new(());

fn node_build_lock() -> MutexGuard<'static, ()> {
    NODE_BUILD_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn paper_config() -> LsAdapterConfig {
    let ls = LsConfig {
        appkey: "test-appkey".into(),
        appsecretkey: "test-secret".into(),
        account_no: "00000000-01".into(),
        environment: LsEnvironment::Paper,
        rate_limits: None,
        base_url: None,
        ws_base_url: None,
        max_pages: None,
        connect_timeout_secs: None,
        request_timeout_secs: None,
        ws_connect_timeout_secs: None,
        allow_insecure_localhost: false,
        ws_channel_capacity: None,
        ws_overflow_policy: None,
    };
    LsAdapterConfig::explicit(ls)
}

fn delta(ord_no: &str, price: i64, approximated: bool) -> FillDelta {
    FillDelta {
        client_order_id: ClientOrderId::from("O-LIVE-1"),
        ord_no: ord_no.into(),
        qty: 10,
        price,
        price_approximated: approximated,
        trade_id: TradeId::from(format!("POLL-{ord_no}-10").as_str()),
        terminal: true,
    }
}

/// Happy path (offline): the ORB strategy mounts in a built LiveNode (add_strategy
/// succeeds). No `node.run` — the repo never drives a full LiveNode offline.
#[tokio::test(flavor = "current_thread")]
async fn strategy_mounts_in_a_built_live_node() {
    // Serialize the logger-initializing build (see NODE_BUILD_LOCK). The guard covers only
    // the synchronous builder chain and drops before any await — safe on any test flavor.
    let mut node = {
        let _guard = node_build_lock();
        LiveNode::builder(TraderId::from("LS-LAB-001"), Environment::Live)
            .expect("builder")
            .with_name("ls-lab-live")
            .add_data_client(None, Box::new(LsDataClientFactory), Box::new(paper_config()))
            .expect("data client")
            .add_exec_client(None, Box::new(LsExecutionClientFactory), Box::new(paper_config()))
            .expect("exec client")
            .build()
            .expect("node builds")
    };

    let id = InstrumentId::from("005930.XKRX");
    let selected = vec![SelectedSymbol {
        instrument_id: id,
        bar_type: BarKind::Minute(1).bar_type(id).unwrap(),
        gap_prices: SessionGapPrices::new(60_000, 63_000),
        prior_atr: None,
        prior_open_vol_mean: None,
        prior_illiq: None,
    }];
    // Off-identity multiplier 1.0 (CLASS B lever 2, R8/KTD-1): the live-wiring smoke
    // exercises the default (non-compounding) sizing path.
    let strategy = OrbStrategy::new(OrbParams::default(), selected, DecisionSink::new(), 1.0);
    node.add_strategy(strategy).expect("the ORB strategy mounts in the live node");
}

/// Direct-drive: a scripted exec-client fill (FillDelta, with cheprice + the
/// approximated flag) flows into a LIVE registry run — the performance report carries
/// the exec price and the data-quality report counts the approximated fill.
#[tokio::test]
async fn scripted_fill_flows_into_a_live_run() {
    let dir = tempdir().unwrap();
    let data = dir.path();

    // Two fills: one exact at cheprice 60_050, one approximated fallback at 60_000.
    let deltas = vec![delta("1001", 60_050, false), delta("1002", 60_000, true)];
    let approx = count_approximated(&deltas);
    assert_eq!(approx, 1);

    let params = OrbParams::default();
    let rid = run_id(Utc.with_ymd_and_hms(2024, 1, 5, 6, 0, 0).unwrap(), RunSource::Live, &params.strategy_id, params.strategy_version);
    let writer = RunWriter::new(data, &rid).unwrap();

    // Performance from the (exact) fill — the exec price lands in the report (AE5).
    let trade = TradeRecord {
        symbol: "005930.XKRX".into(),
        entry_side: "BUY".into(),
        quantity: 10.0,
        avg_px_open: deltas[0].price as f64,
        avg_px_close: Some(deltas[0].price as f64),
        realized_pnl: 500.0,
        ts_opened: 1,
        ts_closed: Some(2),
        fills: vec![FillRecord {
            ts_event: 1,
            side: "BUY".into(),
            qty: 10.0,
            price: deltas[0].price as f64,
            trade_id: deltas[0].trade_id.to_string(),
            commission: 0.0,
        }],
        risk_capital: None,
        realized_r: None,
    };
    writer.write_performance(&PerformanceReport::assemble(vec![trade], 1_000_000.0)).unwrap();

    let mut dq = DataQualityReport::backtest(vec!["005930.XKRX".into()], Vec::new());
    dq.price_approximated_fills = approx;
    writer.write_data_quality(&dq).unwrap();

    let manifest = Manifest {
        run_id: rid.clone(),
        source: RunSource::Live,
        strategy_id: params.strategy_id.clone(),
        strategy_version: params.strategy_version,
        params: params.clone(),
        data_range: DataRange { start: "20240105".into(), end: "20240105".into() },
        catalog_fingerprint: String::new(),
        universe_hash: universe_hash(&["005930.XKRX".to_string()]),
        strategy_code_hash: String::new(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: None,
        created_utc: "2024-01-05T06:00:00Z".into(),
    };
    writer.write_manifest(&manifest).unwrap();
    let run_dir = writer.finalize().unwrap();

    let m: Manifest = serde_json::from_str(&std::fs::read_to_string(run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(m.source, RunSource::Live, "recorded as a live run");
    let p: PerformanceReport = serde_json::from_str(&std::fs::read_to_string(run_dir.join(PERFORMANCE_FILE)).unwrap()).unwrap();
    assert_eq!(p.trades[0].fills[0].price, 60_050.0, "exec price in the live performance report");
    let d: DataQualityReport = serde_json::from_str(&std::fs::read_to_string(run_dir.join(DATA_QUALITY_FILE)).unwrap()).unwrap();
    assert_eq!(d.price_approximated_fills, 1, "the approximated fill is counted (R14)");
}

/// Covers AE3/AE4: only an EXHAUSTED reconcile drive lands a reconcile-advised
/// flag in the data-quality report — a drive that resolved (even after re-polls)
/// records nothing, so transient poll flakiness no longer discounts a run.
#[test]
fn only_an_exhausted_drive_records_reconcile_advised() {
    let mut dq = DataQualityReport::backtest(vec!["005930.XKRX".into()], Vec::new());
    let exhausted = DrivenOutcome { deltas: Vec::new(), terminal: DriveTerminal::Exhausted };
    record_reconcile(&mut dq, &exhausted, "005930");
    assert_eq!(dq.reconcile_advised.len(), 1, "an exhausted drive is recorded");

    // A resolved drive (transient flakiness self-healed) records nothing.
    let resolved = DrivenOutcome { deltas: Vec::new(), terminal: DriveTerminal::Resolved };
    record_reconcile(&mut dq, &resolved, "005930");
    assert_eq!(dq.reconcile_advised.len(), 1, "a resolved drive adds no condition");
}

/// Error path: startup is refused while the ingest advisory lock is held (a backfill
/// and a live session cannot run concurrently).
#[test]
fn live_refused_while_ingest_lock_held() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    let _ingest = AdvisoryLock::acquire(&catalog, LockKind::Ingest).unwrap();

    let err = live_guard(dir.path()).unwrap_err();
    assert!(err.to_string().contains("refused"), "err: {err}");
}

// ---------------------------------------------------------------------------
// U6 — the LiveNode mounter behind a green dispatch (R5, R8; KTD2, KTD3; AE2).
// Offline: node.run is never driven; these exercise the mount authorization, the
// consumption marker, the dispatch↔run manifest linkage, and the spend-ledger seam.
// ---------------------------------------------------------------------------

use nautilus_ls::ingest::budget::{spend_ledger_path, SpendLedger};
use nautilus_ls_lab::artifacts::manifest::DispatchLink;
use nautilus_ls_lab::dispatch::chain::{
    DispatchChain, DispatchOutcome, MountAuthz, RecordKind, SessionDispatch,
};
use nautilus_ls_lab::runner::live::{
    authorize_mount, build_live_session_node, record_session_spend, MountConfig,
};

/// A weekday, mid-session KST instant (2026-07-16 Thu 10:00 KST = 01:00 UTC).
fn mount_ts() -> i64 {
    Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap().timestamp()
}

/// Seed a genesis + a green session-dispatch and return the dispatch record id.
fn seed_green_dispatch(home: &std::path::Path, now_unix: i64) -> String {
    let chain = DispatchChain::open(home).unwrap();
    let now = Utc.timestamp_opt(now_unix, 0).unwrap();
    chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();
    let rec = chain
        .append(
            now,
            1,
            1,
            None,
            RecordKind::SessionDispatch(SessionDispatch {
                outcome: DispatchOutcome::Green,
                checks: Vec::new(),
                deferrals: Vec::new(),
                readiness: None,
                unknown_override: None,
            }),
        )
        .unwrap();
    rec.body.record_id
}

fn mount_cfg(home: &std::path::Path) -> MountConfig {
    MountConfig {
        data_home: home.to_path_buf(),
        requested_rung: 1,
        lane_hash: "cafef00d".into(),
        trading_env: "paper".into(),
        rung_fraction: 1.0,
        nonce: Some(mount_ts().to_string()),
        now_unix: mount_ts(),
        attended_override: Some(true),
    }
}

#[test]
fn authorize_mount_consumes_a_green_dispatch_and_links_the_run() {
    let dir = tempdir().unwrap();
    let dispatch_id = seed_green_dispatch(dir.path(), mount_ts());
    let chain = DispatchChain::open(dir.path()).unwrap();

    let (auth, _lock) = authorize_mount(&chain, &mount_cfg(dir.path()), "orb", 30).unwrap();
    assert_eq!(auth.dispatch_record_id, dispatch_id);
    assert_eq!(auth.chain_rung, 1);
    assert_eq!(auth.effective_rung, 1);
    assert_eq!(auth.lane_hash, "cafef00d");
    assert_eq!(auth.trading_env, "paper");
    assert!(auth.run_id.contains("live-orb-v30"), "run id: {}", auth.run_id);

    // The dispatch↔run linkage the manifest carries (KTD3).
    let link: DispatchLink = auth.dispatch_link();
    assert_eq!(link.dispatch_id, dispatch_id);
    assert_eq!(link.rung, 1);
    assert_eq!(link.rung_fraction, 1.0);
    assert_eq!(link.trading_env, "paper");

    // The chain now shows the green dispatch consumed, carrying the mounted run id.
    let today = nautilus_ls_lab::dispatch::chain::kst_trading_date(Utc.timestamp_opt(mount_ts(), 0).unwrap());
    assert_eq!(chain.load().mount_authz(&today), MountAuthz::Consumed);
    let recorded_run = chain.load().last_session_dispatch.unwrap().consumed_run_id;
    assert_eq!(recorded_run, Some(auth.run_id));
}

#[test]
fn authorize_mount_refuses_without_a_fresh_nonce() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    let chain = DispatchChain::open(dir.path()).unwrap();

    // No nonce → refused; nothing consumed.
    let mut cfg = mount_cfg(dir.path());
    cfg.nonce = None;
    assert!(authorize_mount(&chain, &cfg, "orb", 30).is_err());

    // Stale nonce → refused.
    let mut cfg = mount_cfg(dir.path());
    cfg.nonce = Some((mount_ts() - 100_000).to_string());
    assert!(authorize_mount(&chain, &cfg, "orb", 30).is_err());

    // No-TTY / unattended, even with a fresh nonce → refused.
    let mut cfg = mount_cfg(dir.path());
    cfg.attended_override = Some(false);
    assert!(authorize_mount(&chain, &cfg, "orb", 30).is_err());

    // The green dispatch is still mountable — no refusal consumed it.
    let today = nautilus_ls_lab::dispatch::chain::kst_trading_date(Utc.timestamp_opt(mount_ts(), 0).unwrap());
    assert!(matches!(chain.load().mount_authz(&today), MountAuthz::Ready { .. }));
}

#[test]
fn authorize_mount_refuses_an_already_consumed_dispatch() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    let chain = DispatchChain::open(dir.path()).unwrap();

    let (_auth, lock) = authorize_mount(&chain, &mount_cfg(dir.path()), "orb", 30).unwrap();
    drop(lock); // release the Live lock so the second attempt fails on consumption, not the lock

    let err = authorize_mount(&chain, &mount_cfg(dir.path()), "orb", 30).unwrap_err();
    assert!(err.to_string().contains("consumed"), "err: {err}");
}

#[test]
fn authorize_mount_refuses_when_live_lock_held() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    let chain = DispatchChain::open(dir.path()).unwrap();

    // Another holder took the Live lock between gate and mount (TOCTOU arm, KTD2).
    let catalog = dir.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    let _held = AdvisoryLock::acquire(&catalog, LockKind::Live).unwrap();

    let err = authorize_mount(&chain, &mount_cfg(dir.path()), "orb", 30).unwrap_err();
    assert!(err.to_string().contains("refused"), "err: {err}");
}

#[test]
fn mounted_run_manifest_carries_the_dispatch_link() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    let chain = DispatchChain::open(dir.path()).unwrap();
    let (auth, _lock) = authorize_mount(&chain, &mount_cfg(dir.path()), "orb", 30).unwrap();

    let writer = RunWriter::new(dir.path(), &auth.run_id).unwrap();
    let params = OrbParams::default();
    let manifest = Manifest {
        run_id: auth.run_id.clone(),
        source: RunSource::Live,
        strategy_id: params.strategy_id.clone(),
        strategy_version: params.strategy_version,
        params: params.clone(),
        data_range: DataRange { start: "20260716".into(), end: "20260716".into() },
        catalog_fingerprint: String::new(),
        universe_hash: universe_hash(&["005930.XKRX".to_string()]),
        strategy_code_hash: String::new(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: Some(auth.dispatch_link()),
        created_utc: "2026-07-16T01:00:00Z".into(),
    };
    writer.write_manifest(&manifest).unwrap();
    let run_dir = writer.finalize().unwrap();

    let m: Manifest =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    let link = m.dispatch.expect("live run carries the dispatch link");
    assert_eq!(link.rung, 1);
    assert_eq!(link.lane, "cafef00d");
    assert_eq!(link.trading_env, "paper");
    assert_eq!(link.dispatch_id, auth.dispatch_record_id);
}

#[test]
fn session_gateway_dispatches_land_in_the_spend_ledger() {
    let dir = tempdir().unwrap();
    let lane = "cafef00d";
    // The session records two gateway dispatches (an order call + a t0425 poll).
    record_session_spend(dir.path(), lane, mount_ts()).unwrap();
    record_session_spend(dir.path(), lane, mount_ts() + 1).unwrap();

    let path = spend_ledger_path(&dir.path().join("catalog"));
    let ledger = SpendLedger::load(&path);
    assert!(
        ledger.spent_within(lane, 60, mount_ts() + 1) >= 2,
        "the session's gateway dispatches land in the lane credential's bucket"
    );
    // A different credential's bucket is untouched.
    assert_eq!(ledger.spent_within("beefbeef", 60, mount_ts() + 1), 0);
}

#[test]
fn build_live_session_node_mounts_the_strategy() {
    let selected = vec![SelectedSymbol {
        instrument_id: InstrumentId::from("005930.XKRX"),
        bar_type: BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap(),
        gap_prices: SessionGapPrices::new(60_000, 63_000),
        prior_atr: None,
        prior_open_vol_mean: None,
        prior_illiq: None,
    }];
    // The offline seam the operator command drives after a green dispatch (node.run stays
    // live-only): the node builds and the ORB strategy mounts. Serialize the logger-initializing
    // build against the other LiveNode-building test (see NODE_BUILD_LOCK).
    let node = {
        let _guard = node_build_lock();
        build_live_session_node(paper_config(), OrbParams::default(), selected, DecisionSink::new(), 0.1)
    };
    assert!(node.is_ok(), "the live session node builds and mounts the strategy: {:?}", node.err());
}
