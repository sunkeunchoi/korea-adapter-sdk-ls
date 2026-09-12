//! U7 offline integration: the market→closing-auction-limit translation (KTD5, R16).
//!
//! `daily.rs` emits MARKET orders live because that is what it emits in the backtest,
//! and the frozen head is only judgeable while the two are the same source. The LS
//! order surface is limit-only. These tests pin the one place that reconciles those:
//! what goes out on the wire when the policy is installed, and that NOTHING changes
//! when it is not.

use std::sync::Arc;

use ls_sdk::market_session::T8430OutBlock;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_common::clients::ExecutionClient;
use nautilus_common::live::runner::replace_exec_event_sender;
use nautilus_common::messages::execution::SubmitOrder;
use nautilus_common::messages::ExecutionEvent;
use nautilus_core::{UnixNanos, UUID4};
use nautilus_ls::execution::{
    LastPriceSource, LsExecClient, MarketableLimitPolicy, SessionInstruments,
};
use nautilus_ls::instruments::map_equity;
use nautilus_ls::rules::TickRegime;
use nautilus_model::enums::{AccountType, OrderSide, OrderType, TimeInForce};
use nautilus_model::events::OrderEventAny;
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, StrategyId, TraderId};
use nautilus_model::instruments::Equity;
use nautilus_model::orders::{OrderAny, OrderTestBuilder};
use nautilus_model::types::{Price, Quantity};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ORDER_PATH: &str = "/stock/order";
const SAMSUNG: &str = "005930.XKRX";

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

/// A price feed that knows one instrument, or none at all.
#[derive(Debug, Default)]
struct FakePrices(Vec<(InstrumentId, i64)>);

impl LastPriceSource for FakePrices {
    fn last_price(&self, instrument_id: &InstrumentId) -> Option<i64> {
        self.0
            .iter()
            .find(|(id, _)| id == instrument_id)
            .map(|(_, px)| *px)
    }
}

/// A session instrument cache holding the equities the runner would have cached.
#[derive(Debug, Default)]
struct FakeInstruments(Vec<Equity>);

impl SessionInstruments for FakeInstruments {
    fn equity(&self, instrument_id: &InstrumentId) -> Option<Equity> {
        self.0.iter().find(|e| e.id == *instrument_id).cloned()
    }
}

/// Build the cached `Equity` production would build: 60,000 KRW KOSPI reference with
/// the session's published 상하한가 (±30%, on the 100-KRW grid).
fn samsung_equity(upper: i64, lower: i64) -> Equity {
    let row = T8430OutBlock {
        hname: "삼성전자".to_string(),
        shcode: "005930".to_string(),
        expcode: "KR7005930003".to_string(),
        etfgubun: "0".to_string(),
        uplmtprice: upper.to_string(),
        dnlmtprice: lower.to_string(),
        jnilclose: "60000".to_string(),
        memedan: "1".to_string(),
        recprice: "60000".to_string(),
        gubun: "1".to_string(),
    };
    map_equity(&row, None, UnixNanos::default()).expect("the sample row maps")
}

fn policy(k: i64, prices: FakePrices, instruments: FakeInstruments) -> MarketableLimitPolicy {
    MarketableLimitPolicy::new(
        k,
        TickRegime::Post2023,
        Arc::new(prices),
        Arc::new(instruments),
    )
}

/// The standard fixture: a mark at 60,000 and a cached equity banded 42,000–78,000.
fn standard_policy(k: i64) -> MarketableLimitPolicy {
    policy(
        k,
        FakePrices(vec![(InstrumentId::from(SAMSUNG), 60_000)]),
        FakeInstruments(vec![samsung_equity(78_000, 42_000)]),
    )
}

async fn client_and_server(policy: Option<MarketableLimitPolicy>) -> (LsExecClient, MockServer) {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let mut client = LsExecClient::new(
        "LS-KRX",
        "LS-TRADER-001",
        "00000000-01",
        sdk,
        AccountType::Cash,
    );
    if let Some(policy) = policy {
        client = client.with_marketable_limit_policy(policy);
    }
    (client, server)
}

async fn mount_submit_ok(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", "CSPAT00601"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00040",
            "rsp_msg": "OK",
            "CSPAT00601OutBlock1": {},
            "CSPAT00601OutBlock2": { "OrdNo": "9001" },
        })))
        .mount(server)
        .await;
}

fn capture_exec_events() -> mpsc::UnboundedReceiver<ExecutionEvent> {
    let (tx, rx) = mpsc::unbounded_channel::<ExecutionEvent>();
    replace_exec_event_sender(tx);
    rx
}

fn market_order(client_id: &str, side: OrderSide, qty: i64) -> OrderAny {
    OrderTestBuilder::new(OrderType::Market)
        .trader_id(TraderId::from("LS-TRADER-001"))
        .strategy_id(StrategyId::from("daily-ms"))
        .instrument_id(InstrumentId::from(SAMSUNG))
        .client_order_id(ClientOrderId::from(client_id))
        .side(side)
        .quantity(Quantity::from(qty))
        .time_in_force(TimeInForce::Day)
        .build()
}

fn submit_cmd(order: &OrderAny) -> SubmitOrder {
    SubmitOrder::from_order(
        order,
        TraderId::from("LS-TRADER-001"),
        None,
        None,
        UUID4::new(),
        UnixNanos::default(),
    )
}

async fn next_order_event(rx: &mut mpsc::UnboundedReceiver<ExecutionEvent>) -> OrderEventAny {
    loop {
        let ev = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("an execution event arrives")
            .expect("channel open");
        if let ExecutionEvent::Order(o) = ev {
            return o;
        }
    }
}

/// The `CSPAT00601` bodies the client actually sent, newest last.
async fn submitted_orders(server: &MockServer) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.headers.get("tr_cd").and_then(|v| v.to_str().ok()) == Some("CSPAT00601"))
        .filter_map(|r| serde_json::from_slice::<serde_json::Value>(&r.body).ok())
        .collect()
}

/// A BUY market order goes out as a limit `k` ticks ABOVE the mark, on the grid.
/// 60,000 KRW sits in the 100-KRW band, so k=3 is 60,300.
#[tokio::test]
async fn a_market_buy_goes_out_as_a_crossing_limit_k_ticks_up() {
    let (mut client, server) = client_and_server(Some(standard_policy(3))).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = market_order("O-BUY-1", OrderSide::Buy, 10);
    client.submit_order(submit_cmd(&order)).unwrap();
    // submitted + accepted prove the order was SENT, not denied.
    let _ = next_order_event(&mut rx).await;
    let _ = next_order_event(&mut rx).await;

    let sent = submitted_orders(&server).await;
    assert_eq!(sent.len(), 1, "exactly one order went out");
    let block = &sent[0]["CSPAT00601InBlock1"];
    assert_eq!(block["OrdPrc"], 60_300, "60,000 + 3 × 100-KRW tick");
    assert_eq!(block["BnsTpCode"], "2", "buy");
    assert_eq!(block["OrdQty"], 10);
    assert_eq!(block["IsuNo"], "A005930");
    assert_eq!(block["OrdprcPtnCode"], "00", "a plain limit — the auction takes it at the close");
}

/// A SELL market order — the `close_position` exit shape — takes the identical
/// conversion, `k` ticks BELOW the mark. One policy, not one per emission site.
#[tokio::test]
async fn a_market_sell_from_a_close_position_exit_goes_out_k_ticks_down() {
    let (mut client, server) = client_and_server(Some(standard_policy(3))).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = market_order("O-SELL-1", OrderSide::Sell, 10);
    client.submit_order(submit_cmd(&order)).unwrap();
    let _ = next_order_event(&mut rx).await;
    let _ = next_order_event(&mut rx).await;

    let sent = submitted_orders(&server).await;
    assert_eq!(sent.len(), 1);
    let block = &sent[0]["CSPAT00601InBlock1"];
    assert_eq!(block["OrdPrc"], 59_700, "60,000 − 3 × 100-KRW tick");
    assert_eq!(block["BnsTpCode"], "1", "sell");
}

/// A buy whose crossing offset would clear the 상한가 is clamped to it — the order is
/// still sent, just at the highest price the exchange accepts.
#[tokio::test]
async fn a_crossing_buy_is_clamped_to_the_daily_upper_limit() {
    let tight = policy(
        5,
        FakePrices(vec![(InstrumentId::from(SAMSUNG), 60_000)]),
        // A 상한가 only two ticks above the mark.
        FakeInstruments(vec![samsung_equity(60_200, 42_000)]),
    );
    let (mut client, server) = client_and_server(Some(tight)).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = market_order("O-BUY-CLAMP", OrderSide::Buy, 1);
    client.submit_order(submit_cmd(&order)).unwrap();
    let _ = next_order_event(&mut rx).await;
    let _ = next_order_event(&mut rx).await;

    let block = &submitted_orders(&server).await[0]["CSPAT00601InBlock1"];
    assert_eq!(block["OrdPrc"], 60_200, "clamped to the 상한가, not 60,500");
}

/// Without a mark the order is DENIED and nothing is sent. A market order priced off a
/// missing mark is the one failure that would place an order nobody predicted.
#[tokio::test]
async fn a_missing_last_price_denies_and_sends_nothing() {
    let blind = policy(
        3,
        FakePrices::default(),
        FakeInstruments(vec![samsung_equity(78_000, 42_000)]),
    );
    let (mut client, server) = client_and_server(Some(blind)).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = market_order("O-NO-MARK", OrderSide::Buy, 1);
    client.submit_order(submit_cmd(&order)).unwrap();
    let ev = next_order_event(&mut rx).await;
    match ev {
        OrderEventAny::Denied(d) => assert!(
            d.reason.to_string().contains("no last price"),
            "the denial says why: {}",
            d.reason
        ),
        other => panic!("expected a denial, got {other:?}"),
    }
    assert!(
        submitted_orders(&server).await.is_empty(),
        "a denied order reaches the gateway never"
    );
}

/// Without a cached instrument there is no band to clamp to, so the order is denied
/// rather than sent unclamped.
#[tokio::test]
async fn an_uncached_instrument_denies_and_sends_nothing() {
    let bandless = policy(
        3,
        FakePrices(vec![(InstrumentId::from(SAMSUNG), 60_000)]),
        FakeInstruments::default(),
    );
    let (mut client, server) = client_and_server(Some(bandless)).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = market_order("O-NO-BAND", OrderSide::Buy, 1);
    client.submit_order(submit_cmd(&order)).unwrap();
    match next_order_event(&mut rx).await {
        OrderEventAny::Denied(d) => assert!(
            d.reason.to_string().contains("instrument cache"),
            "the denial names the missing cache entry: {}",
            d.reason
        ),
        other => panic!("expected a denial, got {other:?}"),
    }
    assert!(submitted_orders(&server).await.is_empty());
}

/// A cached instrument reporting ZERO daily limits (a halt, a fresh listing) has no
/// usable band, so the order is denied — never priced unclamped off the raw mark.
#[tokio::test]
async fn a_zero_daily_band_denies_rather_than_pricing_unclamped() {
    let unbanded = policy(
        3,
        FakePrices(vec![(InstrumentId::from(SAMSUNG), 60_000)]),
        FakeInstruments(vec![samsung_equity(0, 0)]),
    );
    let (mut client, server) = client_and_server(Some(unbanded)).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = market_order("O-ZERO-BAND", OrderSide::Buy, 1);
    client.submit_order(submit_cmd(&order)).unwrap();
    match next_order_event(&mut rx).await {
        OrderEventAny::Denied(d) => assert!(
            d.reason.to_string().contains("daily price band"),
            "the denial names the band: {}",
            d.reason
        ),
        other => panic!("expected a denial, got {other:?}"),
    }
    assert!(submitted_orders(&server).await.is_empty());
}

/// With NO policy installed a market order keeps v1's exact refusal — the shipped
/// default is unchanged, and installing the policy is what opts a session in.
#[tokio::test]
async fn without_a_policy_a_market_order_keeps_the_v1_refusal() {
    let (mut client, server) = client_and_server(None).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = market_order("O-NO-POLICY", OrderSide::Buy, 1);
    client.submit_order(submit_cmd(&order)).unwrap();
    match next_order_event(&mut rx).await {
        OrderEventAny::Denied(d) => assert!(
            d.reason.to_string().contains("market orders are not supported in v1"),
            "the pre-U7 message is unchanged: {}",
            d.reason
        ),
        other => panic!("expected a denial, got {other:?}"),
    }
    assert!(submitted_orders(&server).await.is_empty());
}

/// A LIMIT order is untouched by the policy: the strategy's own price goes out
/// verbatim. The policy converts market orders; it does not reprice anything.
#[tokio::test]
async fn a_limit_order_is_not_repriced_by_the_policy() {
    let (mut client, server) = client_and_server(Some(standard_policy(3))).await;
    mount_submit_ok(&server).await;
    let mut rx = capture_exec_events();
    client.start().unwrap();

    let order = OrderTestBuilder::new(OrderType::Limit)
        .trader_id(TraderId::from("LS-TRADER-001"))
        .strategy_id(StrategyId::from("daily-ms"))
        .instrument_id(InstrumentId::from(SAMSUNG))
        .client_order_id(ClientOrderId::from("O-LIMIT-1"))
        .side(OrderSide::Buy)
        .quantity(Quantity::from(4))
        .price(Price::from("58000"))
        .time_in_force(TimeInForce::Day)
        .build();
    client.submit_order(submit_cmd(&order)).unwrap();
    let _ = next_order_event(&mut rx).await;
    let _ = next_order_event(&mut rx).await;

    let block = &submitted_orders(&server).await[0]["CSPAT00601InBlock1"];
    assert_eq!(block["OrdPrc"], 58_000, "the strategy's own limit, not 60,300");
}
