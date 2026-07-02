//! U6 offline integration: the execution client's flat-start gate, ambiguous-submit
//! reconciliation, and drop-count reconciliation against wiremock. Covers AE1, AE5,
//! AE6. The exhaustive `LsError`-variant mapping lives in the `orders::map` unit
//! tests. No live calls.

use ls_sdk::orders::{CSPAT00601Request, OrderIntent};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::execution::LsExecClient;
use nautilus_ls::orders::map::{classify_submit_error, SubmitAction};
use nautilus_model::enums::AccountType;
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
