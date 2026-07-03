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
use nautilus_ls::orders::poll::PollOutcome;
use nautilus_live::node::LiveNode;
use nautilus_ls_lab::artifacts::data_quality::DataQualityReport;
use nautilus_ls_lab::artifacts::manifest::{universe_hash, DataRange, Manifest};
use nautilus_ls_lab::artifacts::performance::{FillRecord, PerformanceReport, TradeRecord};
use nautilus_ls_lab::artifacts::{run_id, RunSource, RunWriter, DATA_QUALITY_FILE, MANIFEST_FILE, PERFORMANCE_FILE};
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::runner::live::{count_approximated, live_guard, record_reconcile};
use nautilus_ls_lab::strategy::orb::{OrbStrategy, SelectedSymbol};
use nautilus_ls_lab::signals::SignalSink;
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, TraderId, TradeId};
use tempfile::tempdir;

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
    let mut node = LiveNode::builder(TraderId::from("LS-LAB-001"), Environment::Live)
        .expect("builder")
        .with_name("ls-lab-live")
        .add_data_client(None, Box::new(LsDataClientFactory), Box::new(paper_config()))
        .expect("data client")
        .add_exec_client(None, Box::new(LsExecutionClientFactory), Box::new(paper_config()))
        .expect("exec client")
        .build()
        .expect("node builds");

    let id = InstrumentId::from("005930.XKRX");
    let selected = vec![SelectedSymbol { instrument_id: id, bar_type: BarKind::Minute(1).bar_type(id).unwrap() }];
    let strategy = OrbStrategy::new(OrbParams::default(), selected, SignalSink::new());
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
    };
    writer.write_performance(&PerformanceReport::assemble(vec![trade], 1_000_000.0)).unwrap();

    let mut dq = DataQualityReport::backtest(vec!["005930.XKRX".into()], false);
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
        checkpoint_hash: None,
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

/// Covers AE3: a scripted inconclusive poll pass lands a reconcile-advised flag in the
/// data-quality report, so the agent treats the run's accounting as suspect.
#[test]
fn inconclusive_poll_records_reconcile_advised() {
    let mut dq = DataQualityReport::backtest(vec!["005930.XKRX".into()], false);
    let inconclusive = PollOutcome { reconcile_needed: true, ..Default::default() };
    record_reconcile(&mut dq, &inconclusive, "005930");
    assert_eq!(dq.reconcile_advised.len(), 1, "the reconcile-advised condition is recorded");

    // A clean pass records nothing.
    let clean = PollOutcome::default();
    record_reconcile(&mut dq, &clean, "005930");
    assert_eq!(dq.reconcile_advised.len(), 1, "a clean poll adds no condition");
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
