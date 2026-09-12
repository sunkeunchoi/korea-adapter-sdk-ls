//! U7 integration: the adapter's order status reports, driven through nautilus's own
//! reconciliation, attribute a CLAIMED instrument to the strategy and an unclaimed one
//! to `EXTERNAL` (KTD14, R29).
//!
//! This is the property the whole unit exists for. Nautilus attributes a position it
//! did not open to `EXTERNAL` unless the strategy claimed the instrument, and a
//! position attributed to `EXTERNAL` is one the day-2 exit silently never fires on —
//! the failure mode is a held leg that is never sold, which no amount of correct
//! strategy code fixes.
//!
//! The repo never drives `node.run` offline, so the test drives the reconciliation
//! manager directly over the reports [`LsExecClient`] produces from a wiremocked
//! t0425. That is the seam that matters: the reports are ours, the attribution is
//! nautilus's, and the contract between them is what can break.

use std::cell::RefCell;
use std::rc::Rc;

use ls_sdk::market_session::T8430OutBlock;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_common::cache::Cache;
use nautilus_common::clients::ExecutionClient;
use nautilus_common::clock::{Clock, TestClock};
use nautilus_common::messages::execution::GenerateOrderStatusReports;
use nautilus_common::msgbus::{set_message_bus, MessageBus};
use nautilus_core::{UnixNanos, UUID4};
use nautilus_execution::engine::ExecutionEngine;
use nautilus_live::execution::manager::{ExecutionManager, ExecutionManagerConfig};
use nautilus_ls::execution::LsExecClient;
use nautilus_ls::instruments::map_equity;
use nautilus_model::enums::AccountType;
use nautilus_model::events::OrderEventAny;
use nautilus_model::identifiers::{AccountId, ClientId, InstrumentId, StrategyId, TraderId, Venue};
use nautilus_model::instruments::InstrumentAny;
use nautilus_model::reports::ExecutionMassStatus;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCNO_PATH: &str = "/stock/accno";
const TRADER_ID: &str = "LS-TRADER-001";
const DAILY_STRATEGY: &str = "daily-ms";
const KRX: &str = "XKRX";

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

/// The exec client over a wiremock reporting one resting BUY on `symbol`.
async fn client_reporting_one_order(server: &MockServer, symbol: &str) -> LsExecClient {
    mount_token(server).await;
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0425"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0425OutBlock": { "tqty": "0", "tcheqty": "0", "tordrem": "0", "cts_ordno": "" },
            "t0425OutBlock1": [
                { "ordno": "7001", "expcode": symbol, "medosu": "매수", "qty": "10",
                  "price": "60000", "cheqty": "0", "ordrem": "10", "status": "접수",
                  "orgordno": "", "ordtime": "0900" }
            ]
        })))
        .mount(server)
        .await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    LsExecClient::new("LS-KRX", TRADER_ID, "00000000-01", sdk, AccountType::Cash)
}

fn equity_for(symbol: &str) -> InstrumentAny {
    let row = T8430OutBlock {
        hname: "테스트종목".to_string(),
        shcode: symbol.to_string(),
        expcode: format!("KR7{symbol}003"),
        etfgubun: "0".to_string(),
        uplmtprice: "78000".to_string(),
        dnlmtprice: "42000".to_string(),
        jnilclose: "60000".to_string(),
        memedan: "1".to_string(),
        recprice: "60000".to_string(),
        gubun: "1".to_string(),
    };
    InstrumentAny::Equity(map_equity(&row, None, UnixNanos::default()).expect("maps"))
}

/// A reconciliation harness: msgbus + cache + engine + manager, with `cached` loaded
/// into the cache and `claims` registered.
struct Harness {
    manager: ExecutionManager,
    engine: Rc<RefCell<ExecutionEngine>>,
}

fn harness(cached: &[&str], claims: &[&str]) -> Harness {
    let trader_id = TraderId::from(TRADER_ID);
    set_message_bus(Rc::new(RefCell::new(MessageBus::new(
        trader_id,
        UUID4::new(),
        None,
        None,
    ))));
    let clock: Rc<RefCell<dyn Clock>> = Rc::new(RefCell::new(TestClock::new()));
    let cache = Rc::new(RefCell::new(Cache::new(None, None)));
    for symbol in cached {
        cache
            .borrow_mut()
            .add_instrument(equity_for(symbol))
            .expect("the instrument caches");
    }
    let engine = Rc::new(RefCell::new(ExecutionEngine::new(
        clock.clone(),
        cache.clone(),
        None,
    )));
    let mut manager = ExecutionManager::new(
        clock,
        cache,
        ExecutionManagerConfig::default().with_trader_id(trader_id),
    );
    for symbol in claims {
        manager
            .claim_external_orders(
                InstrumentId::from(format!("{symbol}.{KRX}").as_str()),
                StrategyId::from(DAILY_STRATEGY),
            )
            .expect("the claim registers");
    }
    Harness { manager, engine }
}

fn mass_status_with(reports: Vec<nautilus_model::reports::OrderStatusReport>) -> ExecutionMassStatus {
    let mut mass = ExecutionMassStatus::new(
        ClientId::from("LS-KRX"),
        AccountId::from("LS-00000000-01"),
        Venue::from(KRX),
        UnixNanos::default(),
        None,
    );
    mass.add_order_reports(reports);
    mass
}

fn orders_cmd() -> GenerateOrderStatusReports {
    GenerateOrderStatusReports::new(
        UUID4::new(),
        UnixNanos::default(),
        false,
        None,
        None,
        None,
        None,
        None,
    )
}

/// The distinct strategy ids the reconciliation attributed its generated order events
/// to. Every event for one order carries the same attribution, so a set of one is the
/// healthy shape and an empty set means no order was created at all.
fn attributed_strategies(events: &[OrderEventAny]) -> Vec<String> {
    let mut ids: Vec<String> = events
        .iter()
        .map(|e| e.strategy_id().to_string())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// A CLAIMED instrument's inherited order is attributed to the strategy, so the day-2
/// exit can find it.
#[tokio::test]
async fn a_claimed_instruments_order_is_attributed_to_the_strategy() {
    let server = MockServer::start().await;
    let client = client_reporting_one_order(&server, "005930").await;
    let reports = client
        .generate_order_status_reports(&orders_cmd())
        .await
        .expect("the adapter reports the resting order");
    assert_eq!(reports.len(), 1);

    let Harness { mut manager, engine } = harness(&["005930"], &["005930"]);
    let result = manager
        .reconcile_execution_mass_status(mass_status_with(reports), engine)
        .await;

    assert_eq!(
        attributed_strategies(&result.events),
        vec![DAILY_STRATEGY.to_string()],
        "the claimed instrument's order belongs to the strategy, not to EXTERNAL"
    );
    assert_eq!(
        result
            .external_orders
            .iter()
            .map(|m| m.strategy_id.to_string())
            .collect::<Vec<_>>(),
        vec![DAILY_STRATEGY.to_string()],
        "and the metadata handed back to the client says the same"
    );
}

/// The CONTRAST that makes the claim load-bearing: with no claim, the identical report
/// is attributed to `EXTERNAL`. Recorded here so a future change that quietly drops
/// `external_order_claims` fails a test instead of silently orphaning day-2 exits.
#[tokio::test]
async fn an_unclaimed_instruments_order_is_attributed_to_external() {
    let server = MockServer::start().await;
    let client = client_reporting_one_order(&server, "005930").await;
    let reports = client
        .generate_order_status_reports(&orders_cmd())
        .await
        .expect("the adapter reports the resting order");

    let Harness { mut manager, engine } = harness(&["005930"], &[]);
    let result = manager
        .reconcile_execution_mass_status(mass_status_with(reports), engine)
        .await;

    assert_eq!(
        attributed_strategies(&result.events),
        vec!["EXTERNAL".to_string()],
        "an unclaimed instrument is nobody's — this is what the claim prevents"
    );
}

/// If the KRX `Equity` is not in the cache, reconciliation SKIPS the report entirely —
/// no order, no attribution, and no error either. The rehearsal mount must therefore
/// cache the instruments before the node starts; this test is the reason that step is
/// not optional.
#[tokio::test]
async fn an_uncached_instrument_is_skipped_so_the_mount_must_cache_first() {
    let server = MockServer::start().await;
    let client = client_reporting_one_order(&server, "005930").await;
    let reports = client
        .generate_order_status_reports(&orders_cmd())
        .await
        .expect("the adapter reports the resting order");

    // Same claim, same report — only the cache is empty.
    let Harness { mut manager, engine } = harness(&[], &["005930"]);
    let result = manager
        .reconcile_execution_mass_status(mass_status_with(reports), engine)
        .await;

    assert!(
        attributed_strategies(&result.events).is_empty(),
        "an uncached instrument produces no order at all — the failure is SILENT, which \
         is why the mount asserts the cache before start"
    );
}
