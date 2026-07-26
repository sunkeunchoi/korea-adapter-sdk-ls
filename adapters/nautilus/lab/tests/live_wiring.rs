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
use nautilus_ls_lab::runner::live::{
    count_approximated, live_guard, record_reconcile, LiveSession, LiveTeardownSession,
};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::strategy::orb::{EmissionGate, OrbStrategy, SelectedSymbol, SessionGapPrices};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, TraderId, TradeId};
use tempfile::tempdir;

use ls_sdk::LsSdk;
use nautilus_ls::execution::LsExecClient;
use nautilus_ls::orders::ledger::{FillLedger, FillObservation};
use nautilus_model::enums::AccountType;

// Nautilus initializes the process-global logger with a non-atomic check-then-set, so two
// `LiveNode::build()` calls racing in parallel test threads intermittently trip "a
// non-Nautilus logger is already registered". The RUNNER owns that lock now
// (`live::node_build_lock`, held across its own `.build()`), so these tests take the SAME
// lock rather than a private one — a second, file-local mutex would not serialize a test
// build against a `build_live_session_node` call.
use nautilus_ls_lab::runner::live::node_build_lock;

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
            .add_exec_client(None, Box::new(LsExecutionClientFactory::new()), Box::new(paper_config()))
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
    authorize_mount, build_live_session_node, parse_mount_universe, record_session_spend,
    resolve_mount_head_params, MountConfig,
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

// ---------------------------------------------------------------------------
// U2 (rung-1 readiness) — the `--mount` session-input seams (KTD5/KTD7).
// The rung-fraction → sizing numerator invariance (zero param diff) is proven in
// `orb.rs::rung_fraction_scales_the_risk_budget_numerator_with_zero_param_diff`.
// ---------------------------------------------------------------------------

/// v34 head governed params — the sized levers a real rung-1 mount trades (never `default()`).
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

/// Stage a minimal finalized backtest run (the head the mount sizes from).
fn stage_finalized_head(data: &std::path::Path, run_id: &str, params: &OrbParams) {
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
        // The head is code-pinned: head_governed_params only trusts a run whose code hash matches
        // the running binary, so the staged head must carry the running binary's hash.
        strategy_code_hash: nautilus_ls_lab::artifacts::manifest::strategy_code_hash(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: None,
        created_utc: "2026-07-24T01:00:00Z".into(),
    };
    writer.write_manifest(&manifest).unwrap();
    writer.finalize().unwrap();
}

#[test]
fn resolve_mount_head_params_refuses_a_zero_size_default_head() {
    // KTD7 fail-closed: an empty data home resolves the head to `default()` (risk 0), which would
    // size every order to zero shares — the mount must refuse rather than trade nothing.
    let dir = tempdir().unwrap();
    assert!(
        resolve_mount_head_params(dir.path()).is_err(),
        "a zero-size (all-levers-off default) head is refused"
    );
}

#[test]
fn resolve_mount_head_params_returns_the_v34_head_when_present() {
    // KTD7: with the v34 head finalized in the data home, the mount sizes from its REAL governed
    // params (risk 299,340), not `default()`.
    let dir = tempdir().unwrap();
    stage_finalized_head(dir.path(), "20260724T014752Z-backtest-orb-v34", &v34_head_params());
    let p = resolve_mount_head_params(dir.path()).unwrap();
    assert_eq!(p.risk_per_trade_krw, 299_340.0, "the mount sizes from v34's real governed params");
    assert_eq!(p.strategy_version, 34);
}

#[test]
fn parse_mount_universe_builds_selected_symbols_and_fails_closed_on_empty() {
    let json = br#"[{"shcode":"005930","prior_close":60000,"today_open":63000,"prior_atr":1500.0}]"#;
    let uni = parse_mount_universe(json).unwrap();
    assert_eq!(uni.len(), 1);
    assert_eq!(uni[0].instrument_id.to_string(), "005930.XKRX");
    assert_eq!(uni[0].prior_atr, Some(1500.0));
    // Empty universe → fail-closed (never mount an empty session).
    assert!(parse_mount_universe(b"[]").is_err());
}

fn one_symbol() -> Vec<SelectedSymbol> {
    let id = InstrumentId::from("005930.XKRX");
    vec![SelectedSymbol {
        instrument_id: id,
        bar_type: BarKind::Minute(1).bar_type(id).unwrap(),
        gap_prices: SessionGapPrices::new(60_000, 63_000),
        prior_atr: None,
        prior_open_vol_mean: None,
        prior_illiq: None,
    }]
}

/// Build the live mount offline. `build_live_session_node` holds the runner's OWN
/// `NODE_BUILD_LOCK` across `.build()`, so these tests serialize with each other and with
/// the runner without taking this file's lock as well.
fn build_mount(fraction: f64) -> nautilus_ls_lab::runner::live::LiveMount {
    build_live_session_node(
        paper_config(),
        OrbParams::default(),
        one_symbol(),
        DecisionSink::new(),
        fraction,
        mount_ts(),
    )
    .expect("the live session node builds and mounts the strategy")
}

#[test]
fn build_live_session_node_mounts_the_strategy() {
    // The offline seam the operator command drives after a green dispatch (node.run stays
    // live-only): the node builds and the ORB strategy mounts.
    let _mount = build_mount(0.1);
}

/// **The plan's highest-severity failure mode (R3, KTD3).** `halt()` on the retained
/// teardown handle must engage the kill switch that gates the IN-NODE client's order
/// path. A teardown that built its own client would flip a different `AtomicBool` — a
/// silent no-op on exactly the orders that matter.
///
/// Asserted against what the node actually received (the factory's record of the client
/// it handed over), not against what the caller intended to give it.
#[test]
fn halt_on_the_retained_handle_disables_the_in_node_clients_order_dispatch() {
    let sdk = LsSdk::new(paper_config().build_config().unwrap()).unwrap();
    let ledger = std::sync::Arc::new(std::sync::Mutex::new(FillLedger::new()));
    let client = LsExecClient::new_with_ledger(
        "LS-EXEC",
        "LS-LAB-001",
        "00000000-01",
        sdk.clone(),
        AccountType::Cash,
        std::sync::Arc::clone(&ledger),
    );
    let session = LiveTeardownSession::new(
        EmissionGate::open(),
        sdk.clone(),
        std::sync::Arc::clone(&ledger),
        client.order_tasks(),
    );
    let factory = std::sync::Arc::new(LsExecutionClientFactory::with_client(client));

    // The node builder takes the pre-built client through the stateful factory. The test
    // keeps a handle on the SAME factory (see `SharedFactory`), so what follows is read
    // off the client the node really received.
    let _node = {
        let _guard = node_build_lock();
        LiveNode::builder(TraderId::from("LS-LAB-001"), Environment::Live)
            .expect("builder")
            .with_name("ls-lab-live")
            .add_data_client(None, Box::new(LsDataClientFactory), Box::new(paper_config()))
            .expect("data client")
            .add_exec_client(
                None,
                Box::new(SharedFactory(std::sync::Arc::clone(&factory))),
                Box::new(paper_config()),
            )
            .expect("exec client")
            .build()
            .expect("node builds")
    };
    let handed = factory.handed().expect("the node received the pre-built client");

    assert!(handed.sdk.inner().orders_enabled(), "the in-node client dispatches before the halt");
    session.halt();
    assert!(
        !handed.sdk.inner().orders_enabled(),
        "halt() on the RETAINED handle disabled the IN-NODE client's order dispatch"
    );
    assert!(
        std::sync::Arc::ptr_eq(handed.sdk.inner(), sdk.inner()),
        "one Arc<Inner> — one kill switch"
    );
}

/// KTD3, the ledger analogue: a fill applied to the **in-node** client's `FillLedger` is
/// visible through the teardown/feeder handle's ledger `Arc`. `sdk.clone()` does NOT carry
/// the ledger (it is a separate `Arc` created inside `LsExecClient::new`), so a naively
/// rebuilt handle would read an empty ledger and the max-loss breaker would never trip —
/// a silent no-op just like the kill-switch trap.
#[test]
fn the_breaker_feeder_and_the_in_node_client_share_one_fill_ledger() {
    let sdk = LsSdk::new(paper_config().build_config().unwrap()).unwrap();
    let ledger = std::sync::Arc::new(std::sync::Mutex::new(FillLedger::new()));
    let client = LsExecClient::new_with_ledger(
        "LS-EXEC",
        "LS-LAB-001",
        "00000000-01",
        sdk.clone(),
        AccountType::Cash,
        std::sync::Arc::clone(&ledger),
    );
    let feeder = LiveTeardownSession::new(
        EmissionGate::open(),
        sdk,
        std::sync::Arc::clone(&ledger),
        client.order_tasks(),
    )
    .ledger();
    let factory = std::sync::Arc::new(LsExecutionClientFactory::with_client(client));

    let _node = {
        let _guard = node_build_lock();
        LiveNode::builder(TraderId::from("LS-LAB-001"), Environment::Live)
            .expect("builder")
            .with_name("ls-lab-live")
            .add_data_client(None, Box::new(LsDataClientFactory), Box::new(paper_config()))
            .expect("data client")
            .add_exec_client(
                None,
                Box::new(SharedFactory(std::sync::Arc::clone(&factory))),
                Box::new(paper_config()),
            )
            .expect("exec client")
            .build()
            .expect("node builds")
    };
    let in_node = factory.handed().expect("the node received the pre-built client").ledger;

    assert!(feeder.lock().unwrap().fills().is_empty(), "a fresh session has no fills");

    // Apply a fill through the ledger the NODE's client holds.
    {
        let mut led = in_node.lock().unwrap();
        led.register(limit_order("O-LIVE-1", 10, 60_000), "1001");
        let outcome = led.apply(FillObservation::poll("1001", 10, 60_000, false));
        assert_eq!(outcome.deltas.len(), 1, "the fill emitted on the in-node ledger");
    }

    // Read it back through the FEEDER handle — the breaker's own view.
    let guard = feeder.lock().unwrap();
    assert_eq!(guard.fills().len(), 1, "the breaker feeder sees the node's real fill");
    assert_eq!(guard.fills()[0].qty, 10);
    assert_eq!(guard.fills()[0].price, 60_000);
}

fn limit_order(client_id: &str, qty: i64, price: i64) -> nautilus_model::orders::OrderAny {
    use nautilus_model::enums::{OrderSide, OrderType, TimeInForce};
    use nautilus_model::identifiers::StrategyId;
    use nautilus_model::orders::OrderTestBuilder;
    use nautilus_model::types::{Price, Quantity};
    OrderTestBuilder::new(OrderType::Limit)
        .trader_id(TraderId::from("LS-LAB-001"))
        .strategy_id(StrategyId::from("S-ORB-1"))
        .instrument_id(InstrumentId::from("005930.XKRX"))
        .client_order_id(ClientOrderId::from(client_id))
        .side(OrderSide::Buy)
        .quantity(Quantity::from(qty as u64))
        .price(Price::from(price.to_string().as_str()))
        .time_in_force(TimeInForce::Day)
        .build()
}

/// KTD4: the returned emission gate IS the mounted strategy's live gate — closing it
/// through the teardown handle flips the same `Arc<AtomicBool>` the strategy reads.
#[test]
fn the_returned_gate_is_the_mounted_strategys_live_gate() {
    let mount = build_mount(0.1);
    let gate = mount.handles.session.emission_gate();
    assert!(gate.allowed(), "the strategy emits before teardown");
    mount.handles.session.stop_emission();
    assert!(!gate.allowed(), "stop_emission closed the strategy's own gate");
}

/// KTD5: `node.handle()` is grabbable BEFORE `run` and is the stop the driver's timer and
/// a watchdog trip both use. (`node.run` itself is never driven offline.)
#[test]
fn the_node_stop_handle_is_captured_before_run() {
    let mount = build_mount(0.1);
    assert!(!mount.handles.handle.should_stop(), "a fresh node is not asked to stop");
    mount.handles.handle.stop();
    assert!(mount.handles.handle.should_stop(), "the captured handle stops the node");
}

/// Two back-to-back builds do not race nautilus's non-atomic global-logger init — the
/// runner holds `NODE_BUILD_LOCK` across `.build()` itself.
#[test]
fn back_to_back_builds_do_not_race_the_global_logger() {
    let _a = build_mount(0.1);
    let _b = build_mount(1.0);
}

/// `add_exec_client` takes `Box<dyn ExecutionClientFactory>` by value, so the test would
/// lose its handle on the factory the node actually used. This thin newtype shares ONE
/// factory between the builder and the test, which is what makes
/// [`halt_on_the_retained_handle_disables_the_in_node_clients_order_dispatch`] a genuine
/// in-node witness rather than an assertion about the caller's intent.
#[derive(Debug)]
struct SharedFactory(std::sync::Arc<LsExecutionClientFactory>);

impl nautilus_common::factories::ExecutionClientFactory for SharedFactory {
    fn create(
        &self,
        name: &str,
        config: &dyn nautilus_common::factories::ClientConfig,
        cache: nautilus_common::cache::CacheView,
    ) -> anyhow::Result<Box<dyn nautilus_common::clients::ExecutionClient>> {
        self.0.create(name, config, cache)
    }
    fn name(&self) -> &str {
        self.0.name()
    }
    fn config_type(&self) -> &str {
        self.0.config_type()
    }
}

// ---------------------------------------------------------------------------
// live-session-driver U5 — the pre-consume prechecks (R6).
//
// A green dispatch is SINGLE-USE. Burning it on a recoverable config error costs the
// operator a whole `--dispatch` cycle, so every fail-closed precheck must run before
// `authorize_mount` consumes. `prepare_mount` takes NO `DispatchChain`, so that property
// is structural: however it fails, it cannot have consumed anything. These cover the
// arms; `run_mount` calls it before `authorize_mount`, the only consumer.
// ---------------------------------------------------------------------------

use nautilus_ls_lab::runner::live::{prepare_mount, MountInputs};

fn mount_inputs(dir: &std::path::Path, prereg: serde_json::Value, universe: &str) -> MountInputs {
    let prereg_path = dir.join("prereg.json");
    std::fs::write(&prereg_path, prereg.to_string()).unwrap();
    let universe_path = dir.join("universe.json");
    std::fs::write(&universe_path, universe).unwrap();
    let keepalive_path = dir.join("op.keepalive");
    std::fs::write(&keepalive_path, b"alive").unwrap();
    let lane_env_path = dir.join("lane.env");
    std::fs::write(&lane_env_path, "APPKEY=x\n").unwrap();
    MountInputs {
        prereg_path,
        keepalive_path,
        lane_env_path,
        universe_path,
        session_secs: 60,
        stop_grace_secs: 10,
        watchdog_tick_secs: 5,
        starting_balance: 10_000_000.0,
    }
}

/// A complete rung-1 pre-registration: a fraction for rung 1 AND both envelope values.
fn armable_prereg() -> serde_json::Value {
    serde_json::json!({
        "version": 2,
        "rungs": [{ "rung": 1, "fraction": 0.1, "n": 5,
                    "expectation": { "low": -148000.0, "high": 266000.0 } }],
        "heartbeat_interval_secs": 30,
        "session_max_loss_krw": 500000.0
    })
}

const ONE_SYMBOL_UNIVERSE: &str =
    r#"[{"shcode":"005930","prior_close":60000,"today_open":63000,"prior_atr":1500.0}]"#;

/// Every precheck failure leaves the chain UNTOUCHED — no consumption marker, so the green
/// dispatch survives for a corrected re-run.
fn assert_nothing_consumed(dir: &std::path::Path, before: usize) {
    let after = DispatchChain::open(dir).unwrap().load().records.len();
    assert_eq!(after, before, "a pre-consume refusal appends nothing to the chain");
    let today = nautilus_ls_lab::dispatch::chain::kst_trading_date(
        Utc.timestamp_opt(mount_ts(), 0).unwrap(),
    );
    assert!(
        matches!(DispatchChain::open(dir).unwrap().load().mount_authz(&today), MountAuthz::Ready { .. }),
        "the green dispatch is still mountable after the refusal"
    );
}

/// **Fail-closed arming (KTD8 / ladder KTD9).** A pre-registration with no heartbeat
/// interval cannot arm the envelope, so the mount refuses — BEFORE consuming.
#[test]
fn an_unarmable_pre_registration_refuses_the_mount_and_consumes_nothing() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    stage_finalized_head(dir.path(), "20260724T014752Z-backtest-orb-v34", &v34_head_params());
    let before = DispatchChain::open(dir.path()).unwrap().load().records.len();

    let half = serde_json::json!({
        "version": 2,
        "rungs": [{ "rung": 1, "fraction": 0.1, "n": 5,
                    "expectation": { "low": -148000.0, "high": 266000.0 } }],
        // heartbeat_interval_secs deliberately absent
        "session_max_loss_krw": 500000.0
    });
    let inputs = mount_inputs(dir.path(), half, ONE_SYMBOL_UNIVERSE);
    let err = match prepare_mount(dir.path(), &inputs, 1, mount_ts()) {
        Ok(_) => panic!("a half-envelope must never run a session"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("half-envelope"), "names the cause: {err}");
    assert_nothing_consumed(dir.path(), before);
}

/// A pre-registration with no fraction for the effective rung refuses pre-consume.
#[test]
fn a_missing_rung_fraction_refuses_the_mount_and_consumes_nothing() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    stage_finalized_head(dir.path(), "20260724T014752Z-backtest-orb-v34", &v34_head_params());
    let before = DispatchChain::open(dir.path()).unwrap().load().records.len();

    let no_rungs = serde_json::json!({
        "version": 2, "rungs": [], "heartbeat_interval_secs": 30, "session_max_loss_krw": 500000.0
    });
    let inputs = mount_inputs(dir.path(), no_rungs, ONE_SYMBOL_UNIVERSE);
    assert!(prepare_mount(dir.path(), &inputs, 1, mount_ts()).is_err());
    assert_nothing_consumed(dir.path(), before);
}

/// An absent operator keepalive file refuses pre-consume: its mtime IS the operator
/// dead-man feeder, so mounting without it would trip the envelope on the first tick.
#[test]
fn an_absent_operator_keepalive_refuses_the_mount_and_consumes_nothing() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    stage_finalized_head(dir.path(), "20260724T014752Z-backtest-orb-v34", &v34_head_params());
    let before = DispatchChain::open(dir.path()).unwrap().load().records.len();

    let mut inputs = mount_inputs(dir.path(), armable_prereg(), ONE_SYMBOL_UNIVERSE);
    inputs.keepalive_path = dir.path().join("never-written.keepalive");
    let err = match prepare_mount(dir.path(), &inputs, 1, mount_ts()) {
        Ok(_) => panic!("no keepalive file must refuse the mount"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("keepalive"), "names the cause: {err}");
    assert_nothing_consumed(dir.path(), before);
}

/// A zero-size (all-levers-off `default()`) head refuses pre-consume — it would size every
/// order to zero shares and silently trade nothing.
#[test]
fn a_zero_size_head_refuses_the_mount_and_consumes_nothing() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    // No finalized head staged → the resolved head collapses to default() (risk 0).
    let before = DispatchChain::open(dir.path()).unwrap().load().records.len();

    let inputs = mount_inputs(dir.path(), armable_prereg(), ONE_SYMBOL_UNIVERSE);
    assert!(prepare_mount(dir.path(), &inputs, 1, mount_ts()).is_err());
    assert_nothing_consumed(dir.path(), before);
}

/// An empty universe refuses pre-consume — never mount a session that trades nothing.
#[test]
fn an_empty_universe_refuses_the_mount_and_consumes_nothing() {
    let dir = tempdir().unwrap();
    seed_green_dispatch(dir.path(), mount_ts());
    stage_finalized_head(dir.path(), "20260724T014752Z-backtest-orb-v34", &v34_head_params());
    let before = DispatchChain::open(dir.path()).unwrap().load().records.len();

    let inputs = mount_inputs(dir.path(), armable_prereg(), "[]");
    assert!(prepare_mount(dir.path(), &inputs, 1, mount_ts()).is_err());
    assert_nothing_consumed(dir.path(), before);
}
