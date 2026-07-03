//! U6 offline integration: the execution client's flat-start gate, ambiguous-submit
//! reconciliation, and drop-count reconciliation against wiremock. Covers AE1, AE5,
//! AE6. The exhaustive `LsError`-variant mapping lives in the `orders::map` unit
//! tests. No live calls.

use std::time::Duration;

use ls_sdk::orders::{CSPAT00601Request, OrderIntent};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_common::clients::ExecutionClient;
use nautilus_common::live::runner::replace_exec_event_sender;
use nautilus_common::messages::execution::{CancelOrder, ModifyOrder, SubmitOrder};
use nautilus_common::messages::ExecutionEvent;
use nautilus_core::{UnixNanos, UUID4};
use nautilus_ls::execution::LsExecClient;
use nautilus_ls::orders::map::{classify_submit_error, SubmitAction};
use nautilus_model::enums::{AccountType, OrderSide, OrderType, TimeInForce};
use nautilus_model::events::OrderEventAny;
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, StrategyId, TraderId};
use nautilus_model::orders::{OrderAny, OrderTestBuilder};
use nautilus_model::types::{Price, Quantity};
use tokio::sync::mpsc;
use tokio::time::timeout;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCNO_PATH: &str = "/stock/accno";
const ORDER_PATH: &str = "/stock/order";

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

async fn mount_t0425(server: &MockServer, cts_ordno: &str, rows: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0425"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0425OutBlock": { "tqty": "0", "tcheqty": "0", "tordrem": "0", "cts_ordno": cts_ordno },
            "t0425OutBlock1": rows
        })))
        .mount(server)
        .await;
}

async fn mount_t0424(server: &MockServer, holdings: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0424OutBlock": {},
            "t0424OutBlock1": holdings
        })))
        .mount(server)
        .await;
}

async fn client_and_sdk(server: &MockServer) -> (LsExecClient, LsSdk) {
    mount_token(server).await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let client = LsExecClient::new(
        "LS-KRX",
        "LS-TRADER-001",
        "00000000-01",
        sdk.clone(),
        AccountType::Cash,
    );
    (client, sdk)
}

async fn count_requests(server: &MockServer, tr_cd: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.headers.get("tr_cd").and_then(|v| v.to_str().ok()) == Some(tr_cd))
        .count()
}

/// AE5: a flat account (no open orders, no holdings) passes the gate.
#[tokio::test]
async fn flat_account_passes_the_gate() {
    let server = MockServer::start().await;
    let (client, _sdk) = client_and_sdk(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424(&server, serde_json::json!([])).await;

    client.verify_flat().await.expect("a flat account starts");
}

/// AE5: an open unfilled order refuses the start with a reason.
#[tokio::test]
async fn open_order_refuses_start() {
    let server = MockServer::start().await;
    let (client, _sdk) = client_and_sdk(&server).await;
    mount_t0425(
        &server,
        "",
        serde_json::json!([
            { "ordno": "1001", "expcode": "005930", "medosu": "매수", "qty": "10",
              "price": "60000", "cheqty": "0", "ordrem": "10", "status": "접수", "orgordno": "", "ordtime": "0900" }
        ]),
    )
    .await;
    mount_t0424(&server, serde_json::json!([])).await;

    let err = client
        .verify_flat()
        .await
        .expect_err("an open order refuses start");
    assert!(err.to_string().contains("open"), "reason names open orders: {err}");
}

/// AE5: nonzero holdings refuse the start.
#[tokio::test]
async fn holdings_refuse_start() {
    let server = MockServer::start().await;
    let (client, _sdk) = client_and_sdk(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424(
        &server,
        serde_json::json!([{ "expcode": "005930", "janqty": "10", "hname": "삼성전자" }]),
    )
    .await;

    let err = client.verify_flat().await.expect_err("holdings refuse start");
    assert!(err.to_string().contains("holding"), "reason names holdings: {err}");
}

/// AE5: an order row with an UNPARSEABLE `ordrem` fails the gate closed — a garbage
/// remaining-qty must never be read as "0 = filled" and let a resting order through.
#[tokio::test]
async fn unparseable_ordrem_fails_the_gate_closed() {
    let server = MockServer::start().await;
    let (client, _sdk) = client_and_sdk(&server).await;
    mount_t0425(
        &server,
        "",
        serde_json::json!([
            { "ordno": "1001", "expcode": "005930", "medosu": "매수", "qty": "10",
              "price": "60000", "cheqty": "0", "ordrem": "N/A", "status": "접수", "orgordno": "", "ordtime": "0900" }
        ]),
    )
    .await;
    mount_t0424(&server, serde_json::json!([])).await;

    let err = client
        .verify_flat()
        .await
        .expect_err("an unparseable ordrem must refuse start (fail-closed)");
    assert!(err.to_string().contains("open"), "reason names open orders: {err}");
}

/// AE5: a truncated (multi-page) order inquiry fails closed — cannot prove flat.
#[tokio::test]
async fn truncated_inquiry_fails_closed() {
    let server = MockServer::start().await;
    let (client, _sdk) = client_and_sdk(&server).await;
    // A non-empty next-cursor signals more pages the single-page gate did not see.
    mount_t0425(&server, "NEXT", serde_json::json!([])).await;
    mount_t0424(&server, serde_json::json!([])).await;

    let err = client.verify_flat().await.expect_err("truncation fails closed");
    assert!(err.to_string().contains("truncated"), "reason names truncation: {err}");
}

/// AE1: an ambiguous/transport (5xx) submit is NEVER a rejection — it classifies as
/// pending and drives reconciliation.
#[tokio::test]
async fn ambiguous_submit_reconciles_never_rejects() {
    let server = MockServer::start().await;
    let (client, sdk) = client_and_sdk(&server).await;

    // The submit transport fails (5xx) → the SDK yields AmbiguousOrder.
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", "CSPAT00601"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let err = sdk
        .orders()
        .submit(&CSPAT00601Request::limit("005930", "1", "60000", "2", "NXT"))
        .await
        .expect_err("a 5xx submit errors");
    // The KTD6 classification is pending (reconcile), never Reject/Deny.
    let action = classify_submit_error(&err);
    assert_eq!(action, SubmitAction::Pending, "5xx must be pending, got {action:?}");
    assert_ne!(action, SubmitAction::Reject);

    // The pending path drives an order-inquiry reconcile.
    mount_t0425(&server, "", serde_json::json!([])).await;
    let intent = OrderIntent::submit(
        "00000000-01".to_string(),
        "005930".to_string(),
        "2".to_string(),
        "1".to_string(),
        "60000".to_string(),
        None,
    );
    let _ = client.reconcile(&intent).await;
    assert!(
        count_requests(&server, "t0425").await >= 1,
        "the ambiguous submit drove a reconcile inquiry"
    );
}

/// AE6: a drop-count advance on the order lane triggers a reconcile inquiry; no
/// advance issues nothing.
#[tokio::test]
async fn drop_count_advance_triggers_reconcile() {
    let server = MockServer::start().await;
    let (client, _sdk) = client_and_sdk(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;

    let intent = OrderIntent::submit(
        "00000000-01".to_string(),
        "005930".to_string(),
        "2".to_string(),
        "1".to_string(),
        "60000".to_string(),
        None,
    );

    // First observation of a nonzero drop count → reconcile runs.
    assert!(
        client.on_drop_count(1, &intent).await.is_some(),
        "a drop-count advance reconciles"
    );
    let after_first = count_requests(&server, "t0425").await;
    assert!(after_first >= 1);

    // No further advance (same count) → no reconcile, no new inquiry.
    assert!(
        client.on_drop_count(1, &intent).await.is_none(),
        "no advance ⇒ no reconcile"
    );
    assert_eq!(count_requests(&server, "t0425").await, after_first, "no refetch without advance");
}

/// The kill-switch halt hook disables the order path (engaged after a closing
/// action, never before).
#[tokio::test]
async fn halt_hook_disables_orders() {
    let server = MockServer::start().await;
    let (client, _sdk) = client_and_sdk(&server).await;
    assert!(client.orders_enabled(), "orders enabled by default");
    client.halt();
    assert!(!client.orders_enabled(), "halt disarms the order path");
}

// ---------------------------------------------------------------------------
// U3 (poll-derived fills) + U4 (modify/cancel) + the DoD submit→fill→cancel
// round-trip, driven through the real ExecutionClient surface. Emitted
// execution events are captured via the runner's (thread-local) exec sender.
// ---------------------------------------------------------------------------

/// Capture the execution events the client emits (`start()` copies this sender
/// into the emitter; spawned workers emit through that copy).
fn capture_exec_events() -> mpsc::UnboundedReceiver<ExecutionEvent> {
    let (tx, rx) = mpsc::unbounded_channel::<ExecutionEvent>();
    replace_exec_event_sender(tx);
    rx
}

fn test_order(client_id: &str, qty: i64, price: i64) -> OrderAny {
    OrderTestBuilder::new(OrderType::Limit)
        .trader_id(TraderId::from("LS-TRADER-001"))
        .strategy_id(StrategyId::from("S-ORB-1"))
        .instrument_id(InstrumentId::from("005930.XKRX"))
        .client_order_id(ClientOrderId::from(client_id))
        .side(OrderSide::Buy)
        .quantity(Quantity::from(qty))
        .price(Price::from(price.to_string().as_str()))
        .time_in_force(TimeInForce::Day)
        .build()
}

fn submit_cmd(order: &OrderAny) -> SubmitOrder {
    SubmitOrder::from_order(order, TraderId::from("LS-TRADER-001"), None, None, UUID4::new(), UnixNanos::default())
}

/// Mount a successful order ack (`CSPAT006/007/008`) returning `ordno` (+ optional
/// parent) on the OutBlock2. Orders use TR-specific success codes (a generic
/// `00000` is AMBIGUOUS, never accepted): submit `00040` (buy-ack), modify `00462`,
/// cancel `00463`.
async fn mount_order_ok(server: &MockServer, tr_cd: &str, ordno: &str, prnt: &str) {
    let rsp_cd = match tr_cd {
        "CSPAT00601" => "00040", // buy-ack
        "CSPAT00701" => "00462", // modify completed
        "CSPAT00801" => "00463", // cancel completed
        other => panic!("no success code mapped for {other}"),
    };
    let mut ob2 = serde_json::json!({ "OrdNo": ordno });
    if !prnt.is_empty() {
        ob2["PrntOrdNo"] = serde_json::json!(prnt);
    }
    let body = serde_json::json!({
        "rsp_cd": rsp_cd,
        "rsp_msg": "OK",
        format!("{tr_cd}OutBlock1"): {},
        format!("{tr_cd}OutBlock2"): ob2,
    });
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", tr_cd))
        .respond_with(ok_json(body))
        .mount(server)
        .await;
}

/// Mount a clean business rejection (2xx, non-`00000` rsp_cd → `ApiError` → Reject).
async fn mount_order_business_reject(server: &MockServer, tr_cd: &str, code: &str) {
    let body = serde_json::json!({ "rsp_cd": code, "rsp_msg": "rejected" });
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", tr_cd))
        .respond_with(ok_json(body))
        .mount(server)
        .await;
}

/// Mount a transport failure (5xx → `AmbiguousOrder` → Pending → reconcile).
async fn mount_order_ambiguous(server: &MockServer, tr_cd: &str) {
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", tr_cd))
        .respond_with(ResponseTemplate::new(500))
        .mount(server)
        .await;
}

fn t0425_row(ordno: &str, orgordno: &str, cheqty: &str, ordrem: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "ordno": ordno, "expcode": "005930", "medosu": "매수", "qty": "10",
        "price": "60000", "cheqty": cheqty, "ordrem": ordrem, "status": status,
        "orgordno": orgordno, "ordtime": "0900"
    })
}

/// Await the next order event (skipping any non-order execution events).
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

/// Drain submitted+accepted after a submit so the order is registered + resting.
async fn drain_submit_accept(rx: &mut mpsc::UnboundedReceiver<ExecutionEvent>) {
    for _ in 0..2 {
        let _ = next_order_event(rx).await;
    }
}

/// AE3 / U3: an accepted order later shows cheqty>0 on t0425 → OrderFilled emits
/// with the poll-derived quantity at the order's limit price — no SC lane involved.
#[tokio::test]
async fn poll_derived_fill_emits_independently_of_sc() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-POLL-1", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    // t0425 now reports 4 filled of 10 → a single poll pass emits OrderFilled(4).
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "", "4", "6", "체결")])).await;
    let outcome = client.poll_once().await;
    assert_eq!(outcome.deltas.len(), 1);

    match next_order_event(&mut rx).await {
        OrderEventAny::Filled(f) => {
            assert_eq!(f.last_qty, Quantity::from(4));
            assert_eq!(f.last_px, Price::from("60000"), "poll fills emit at the order limit price");
        }
        other => panic!("expected OrderFilled, got {other:?}"),
    }
}

/// U2 / AE5: an order whose t0425 fill row carries a `cheprice` differing from the
/// order limit emits OrderFilled at the execution price, end-to-end through
/// `poll_once` → ledger → event.
#[tokio::test]
async fn poll_fill_emits_at_cheprice() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-CHEPX", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    // The fill row reports cheprice=60050 (execution price) ≠ limit 60000.
    let row = serde_json::json!({
        "ordno": "1001", "expcode": "005930", "medosu": "매수", "qty": "10",
        "price": "60000", "cheqty": "10", "cheprice": "60050", "ordrem": "0",
        "status": "체결", "orgordno": "", "ordtime": "0900"
    });
    mount_t0425(&server, "", serde_json::json!([row])).await;
    let outcome = client.poll_once().await;
    assert_eq!(outcome.deltas.len(), 1);
    assert!(!outcome.deltas[0].price_approximated, "a first fill at cheprice is exact");

    match next_order_event(&mut rx).await {
        OrderEventAny::Filled(f) => {
            assert_eq!(f.last_qty, Quantity::from(10));
            assert_eq!(f.last_px, Price::from("60050"), "the fill emits at cheprice, not the limit");
        }
        other => panic!("expected OrderFilled, got {other:?}"),
    }
}

/// DoD round-trip: submit → (partial) fill → cancel, entirely through the client.
#[tokio::test]
async fn submit_fill_cancel_round_trip() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-RT-1", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    // Partial fill (4 of 10) via the poll lane.
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "", "4", "6", "체결")])).await;
    client.poll_once().await;
    assert!(matches!(next_order_event(&mut rx).await, OrderEventAny::Filled(_)));

    // Cancel the resting remainder.
    mount_order_ok(&server, "CSPAT00801", "1002", "1001").await;
    client
        .cancel_order(CancelOrder::new(
            TraderId::from("LS-TRADER-001"),
            None,
            StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"),
            ClientOrderId::from("O-RT-1"),
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
            None,
        ))
        .unwrap();
    assert!(
        matches!(next_order_event(&mut rx).await, OrderEventAny::Canceled(_)),
        "the resting remainder cancels"
    );
}

/// AE2 / U4: a modify acks a new OrdNo (1002); a later poll fill keyed on the
/// ORIGINAL OrdNo (1001) still resolves and emits on the original order.
#[tokio::test]
async fn modify_ack_then_fill_on_original_ordno_resolves() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-MOD-1", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    // Modify → new OrdNo 1002 (parent 1001).
    mount_order_ok(&server, "CSPAT00701", "1002", "1001").await;
    client
        .modify_order(ModifyOrder::new(
            TraderId::from("LS-TRADER-001"),
            None,
            StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"),
            ClientOrderId::from("O-MOD-1"),
            None,
            Some(Quantity::from(10)),
            Some(Price::from("60100")),
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
            None,
        ))
        .unwrap();
    match next_order_event(&mut rx).await {
        OrderEventAny::Updated(_) => {}
        other => panic!("expected OrderUpdated, got {other:?}"),
    }

    // A fill still keyed on the ORIGINAL OrdNo 1001 resolves + emits (chain intact).
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "", "5", "5", "체결")])).await;
    client.poll_once().await;
    match next_order_event(&mut rx).await {
        OrderEventAny::Filled(f) => assert_eq!(f.last_qty, Quantity::from(5)),
        other => panic!("expected OrderFilled on the original client order, got {other:?}"),
    }
}

/// The most-recent request body the mock received for `tr_cd`.
async fn last_request_body(server: &MockServer, tr_cd: &str) -> serde_json::Value {
    let reqs = server.received_requests().await.unwrap_or_default();
    let r = reqs
        .iter()
        .rev()
        .find(|r| r.headers.get("tr_cd").and_then(|v| v.to_str().ok()) == Some(tr_cd))
        .expect("a request for tr_cd was received");
    serde_json::from_slice(&r.body).expect("request body is JSON")
}

/// U4 regression: a price-only re-modify (`cmd.quantity == None`) after a quantity
/// reduction must keep the REDUCED quantity — not resurrect the original from the
/// stale retained OrderAny (which would silently re-increase live exposure).
#[tokio::test]
async fn price_only_remodify_keeps_the_reduced_quantity() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-RM-1", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    mount_order_ok(&server, "CSPAT00701", "1002", "1001").await;
    // Modify 1: reduce quantity 10 -> 5.
    client
        .modify_order(ModifyOrder::new(
            TraderId::from("LS-TRADER-001"), None, StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"), ClientOrderId::from("O-RM-1"), None,
            Some(Quantity::from(5)), Some(Price::from("60000")), None,
            UUID4::new(), UnixNanos::default(), None, None,
        ))
        .unwrap();
    assert!(matches!(next_order_event(&mut rx).await, OrderEventAny::Updated(_)));

    // Modify 2: PRICE ONLY (quantity None) — must reuse the reduced qty 5.
    client
        .modify_order(ModifyOrder::new(
            TraderId::from("LS-TRADER-001"), None, StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"), ClientOrderId::from("O-RM-1"), None,
            None, Some(Price::from("60100")), None,
            UUID4::new(), UnixNanos::default(), None, None,
        ))
        .unwrap();
    assert!(matches!(next_order_event(&mut rx).await, OrderEventAny::Updated(_)));

    let body = last_request_body(&server, "CSPAT00701").await;
    let ordqty = body["CSPAT00701InBlock1"]["OrdQty"].as_i64();
    assert_eq!(
        ordqty,
        Some(5),
        "a price-only re-modify must keep the reduced qty (5), not the original 10; got {ordqty:?}"
    );
}

/// U4 / KTD6: a cleanly-rejected cancel emits **cancel-rejected**, not canceled —
/// the order stays open.
#[tokio::test]
async fn cancel_business_reject_emits_cancel_rejected() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-CR-1", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    mount_order_business_reject(&server, "CSPAT00801", "40580").await;
    client
        .cancel_order(CancelOrder::new(
            TraderId::from("LS-TRADER-001"),
            None,
            StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"),
            ClientOrderId::from("O-CR-1"),
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
            None,
        ))
        .unwrap();
    match next_order_event(&mut rx).await {
        OrderEventAny::CancelRejected(_) => {}
        other => panic!("a rejected cancel must emit CancelRejected (order stays open), got {other:?}"),
    }
}

/// U4: an ambiguous (5xx) cancel reconciles; a t0425 취소 row → OrderCanceled.
#[tokio::test]
async fn ambiguous_cancel_reconciles_to_canceled() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-AC-1", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    mount_order_ambiguous(&server, "CSPAT00801").await;
    // The reconcile inquiry shows an explicit 취소 (canceled) row for OrdNo 1001.
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "", "0", "0", "취소")])).await;
    client
        .cancel_order(CancelOrder::new(
            TraderId::from("LS-TRADER-001"),
            None,
            StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"),
            ClientOrderId::from("O-AC-1"),
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
            None,
        ))
        .unwrap();
    match next_order_event(&mut rx).await {
        OrderEventAny::Canceled(_) => {}
        other => panic!("an ambiguous cancel reconciled to 취소 must emit Canceled, got {other:?}"),
    }
}

/// U4: an ambiguous cancel whose reconcile is inconclusive (still-접수, Unknown)
/// stays pending — NO canceled event is emitted (never guessed).
#[tokio::test]
async fn ambiguous_cancel_reconcile_unknown_stays_pending() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    mount_order_ok(&server, "CSPAT00601", "1001", "").await;
    let order = test_order("O-AU-1", 10, 60_000);
    client.submit_order(submit_cmd(&order)).unwrap();
    drain_submit_accept(&mut rx).await;

    mount_order_ambiguous(&server, "CSPAT00801").await;
    // The order is still 접수 (accepted) — a cancel cannot be proven, so it stays
    // pending and never emits a canceled/rejected event.
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "", "0", "10", "접수")])).await;
    client
        .cancel_order(CancelOrder::new(
            TraderId::from("LS-TRADER-001"),
            None,
            StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"),
            ClientOrderId::from("O-AU-1"),
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
            None,
        ))
        .unwrap();
    // No terminal event should arrive within the window (stays pending).
    let got = timeout(Duration::from_millis(600), rx.recv()).await;
    assert!(got.is_err(), "an unprovable cancel must NOT emit a canceled/rejected event");
}

/// U4: a modify of an unknown/never-accepted order is denied — nothing is sent to
/// the venue and no updated/rejected event is emitted.
#[tokio::test]
async fn modify_unknown_order_emits_nothing() {
    let server = MockServer::start().await;
    let mut rx = capture_exec_events();
    let (mut client, _sdk) = client_and_sdk(&server).await;
    client.start().unwrap();

    client
        .modify_order(ModifyOrder::new(
            TraderId::from("LS-TRADER-001"),
            None,
            StrategyId::from("S-ORB-1"),
            InstrumentId::from("005930.XKRX"),
            ClientOrderId::from("O-NEVER"),
            None,
            Some(Quantity::from(10)),
            Some(Price::from("60000")),
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
            None,
        ))
        .unwrap();
    let got = timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(got.is_err(), "a modify of an unknown order emits nothing (denied)");
}
