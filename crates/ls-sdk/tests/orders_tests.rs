//! Orders dependency-class tests (`CSPAT00601` domestic cash-equity submit).
//!
//! These prove the order TR's structure and that `submit()` routes through the
//! order dispatch path (`post_order`), all credential-free against a MOCK token +
//! MOCK response on the dummy `TEST_ACCOUNT_NO`. NO live order is ever submitted —
//! live evidence is the guarded, out-of-band paper-order run (order-safety §4),
//! never the unit suite. The broader order-logic gate (no-retry, dedup, kill
//! switch, reconciliation) lives in the dedicated mock gate.

use std::sync::Arc;

use ls_core::{Inner, LsError};
use ls_sdk::orders::{
    CSPAT00601Request, CSPAT00601Response, OrderIntent, OrderState, T0425Request,
};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The REST path for the order endpoint.
const ORDER_PATH: &str = "/stock/order";

/// The REST path for the `t0425` reconciliation read.
const T0425_PATH: &str = "/stock/accno";

/// A spec-grounded `t0425` response with one open order row (`ordno=32004`).
const T0425_ONE_ROW: &str = r#"{
    "rsp_cd": "00000",
    "t0425OutBlock": { "tqty": 1, "tcheqty": 0, "tordrem": 1, "cts_ordno": "" },
    "t0425OutBlock1": [
        { "ordno": 32004, "expcode": "005930", "medosu": "매수", "qty": 1, "price": 60000,
          "cheqty": 0, "ordrem": 1, "status": "접수", "orgordno": 0, "ordtime": "153257702" }
    ]
}"#;

/// A spec-grounded buy-ack response (`rsp_cd=00040`, `OrdNo=32004`), modeled on
/// the raw `CSPAT00601` response example.
const BUY_ACK: &str = r#"{
    "CSPAT00601OutBlock1": { "RecCnt": 1, "AcntNo": "20*********", "IsuNo": "A005930",
        "OrdQty": 1, "OrdPrc": "60000.00", "BnsTpCode": "2" },
    "CSPAT00601OutBlock2": { "RecCnt": 1, "OrdNo": 32004, "OrdTime": "153257702",
        "OrdMktCode": "10", "ShtnIsuNo": "A005930", "OrdAmt": 60000,
        "SpareOrdNo": 32004, "RsvOrdNo": 0, "AcntNm": "***", "IsuNm": "삼성전자" },
    "rsp_cd": "00040",
    "rsp_msg": "매수 주문이 완료되었습니다."
}"#;

fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

/// Edge (IGW40011 guard): the numeric request fields `OrdQty`/`OrdPrc` must
/// serialize as JSON **numbers**, not quoted strings. A quoted numeric field
/// makes the gateway return `IGW40011`.
#[test]
fn request_serializes_numeric_fields_as_json_numbers() {
    let req = CSPAT00601Request::limit("005930", "1", "60000", "2", "NXT");
    let wire = serde_json::to_string(&req).expect("serialize");
    // Wrapped under the InBlock1 key.
    assert!(wire.contains("\"CSPAT00601InBlock1\""), "wire: {wire}");
    // Unquoted numbers — the IGW40011 guard.
    assert!(wire.contains("\"OrdQty\":1"), "OrdQty must be a JSON number: {wire}");
    assert!(
        wire.contains("\"OrdPrc\":60000"),
        "OrdPrc must be a JSON number: {wire}"
    );
    // String fields stay quoted.
    assert!(wire.contains("\"IsuNo\":\"005930\""), "wire: {wire}");
    assert!(wire.contains("\"BnsTpCode\":\"2\""), "wire: {wire}");
    assert!(wire.contains("\"MbrNo\":\"NXT\""), "wire: {wire}");
}

/// Happy (offline): a captured buy-ack deserializes and `OrdNo` is read from
/// `OutBlock2`.
#[test]
fn response_deserializes_buy_ack_and_reads_ordno() {
    let res: CSPAT00601Response = serde_json::from_str(BUY_ACK).expect("deserialize buy-ack");
    assert_eq!(res.rsp_cd, "00040");
    assert_eq!(res.order_no(), "32004");
    assert_eq!(res.outblock1.isuno, "A005930");
    // string_or_number tolerates the response OrdPrc arriving as a string.
    assert_eq!(res.outblock1.ordprc, "60000.00");
    // Auxiliary order numbers are distinct from the live one.
    assert_eq!(res.outblock2.spareordno, "32004");
    assert_eq!(res.outblock2.rsvordno, "0");
}

/// Integration: `submit()` flows through `post_order` and a `00040` ack
/// classifies Accepted, returning the order number.
#[tokio::test]
async fn submit_dispatches_via_post_order_and_returns_ordno() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(BUY_ACK))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CSPAT00601Request::limit("005930", "1", "60000", "2", "NXT");
    let res = sdk.orders().submit(&req).await.expect("buy-ack is Accepted");
    assert_eq!(res.rsp_cd, "00040");
    assert_eq!(res.order_no(), "32004");
}

/// Error: a rejection (`rsp_cd` not in the order-success set) surfaces as
/// `ApiError` with the broker code/message preserved.
#[tokio::test]
async fn submit_rejection_surfaces_apierror_with_code() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "40570",
            "rsp_msg": "주문가격이 상하한가를 벗어났습니다."
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CSPAT00601Request::limit("005930", "1", "99999999", "2", "NXT");
    let err = sdk
        .orders()
        .submit(&req)
        .await
        .expect_err("a rejection must surface as ApiError");
    match err {
        LsError::ApiError { code, message } => {
            assert_eq!(code, "40570");
            assert!(message.contains("상하한가"));
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

/// Integration: `submit()` routes through `post_order`, NOT `post` — proven by
/// the kill switch, which only the order path consults. With orders disabled the
/// submit halts before any HTTP call.
#[tokio::test]
async fn submit_routes_through_post_order_observable_via_kill_switch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits_inner = hits.clone();
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(move |_req: &wiremock::Request| {
            hits_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_string(BUY_ACK)
        })
        .mount(&server)
        .await;

    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    inner.set_orders_enabled(false);
    let sdk = LsSdk::from_inner(inner);
    let req = CSPAT00601Request::limit("005930", "1", "60000", "2", "NXT");
    let err = sdk
        .orders()
        .submit(&req)
        .await
        .expect_err("kill switch must halt the order");
    match err {
        LsError::ApiError { code, .. } => assert_eq!(code, "orders-disabled"),
        other => panic!("expected orders-disabled, got {other:?}"),
    }
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "the order path must halt before HTTP — proving submit() uses post_order"
    );
}

/// The `t0425` read dispatches through `post_paginated` (is_order: false) and
/// deserializes its rows.
#[tokio::test]
async fn t0425_inquiry_dispatches_and_deserializes_rows() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T0425_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(T0425_ONE_ROW))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .orders()
        .inquiry(&T0425Request::for_symbol("005930"))
        .await
        .expect("t0425 query succeeds");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].ordno, "32004");
    assert_eq!(resp.outblock1[0].status, "접수");
}

/// AE2: after an ambiguous send, `reconcile()` queries `t0425` and a matching
/// order classifies Accepted (proving the order landed — it is not resubmitted).
#[tokio::test]
async fn reconcile_after_ambiguous_finds_accepted_order() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T0425_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(T0425_ONE_ROW))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let intent = OrderIntent {
        account_no: "00000000-01".into(),
        symbol: "005930".into(),
        side: "2".into(),
        qty: "1".into(),
        price: "60000".into(),
        order_no: Some("32004".into()),
    };
    let outcome = sdk.orders().reconcile(&intent, false).await;
    assert_eq!(outcome.state, OrderState::Accepted);
    assert!(!outcome.safe_to_retry, "an accepted order is never retried");
}

/// A `dedup_hit` short-circuits reconciliation to Duplicate with no `t0425`
/// query at all.
#[tokio::test]
async fn reconcile_dedup_hit_is_duplicate_without_query() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits_inner = hits.clone();
    Mock::given(method("POST"))
        .and(path(T0425_PATH))
        .respond_with(move |_req: &wiremock::Request| {
            hits_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_string(T0425_ONE_ROW)
        })
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let intent = OrderIntent {
        account_no: "00000000-01".into(),
        symbol: "005930".into(),
        side: "2".into(),
        qty: "1".into(),
        price: "60000".into(),
        order_no: Some("32004".into()),
    };
    let outcome = sdk.orders().reconcile(&intent, true).await;
    assert_eq!(outcome.state, OrderState::Duplicate);
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a dedup hit must not issue a t0425 query"
    );
}

/// A failed `t0425` query fails toward Unknown (never silent Accepted) and is
/// not safe to retry.
#[tokio::test]
async fn reconcile_failed_query_is_unknown_not_safe() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T0425_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "IGW40013",
            "rsp_msg": "조회 실패"
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let intent = OrderIntent {
        account_no: "00000000-01".into(),
        symbol: "005930".into(),
        side: "2".into(),
        qty: "1".into(),
        price: "60000".into(),
        order_no: Some("32004".into()),
    };
    let outcome = sdk.orders().reconcile(&intent, false).await;
    assert_eq!(outcome.state, OrderState::Unknown);
    assert!(!outcome.safe_to_retry);
}
