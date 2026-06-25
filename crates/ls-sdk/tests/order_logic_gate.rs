//! Order-logic gate (order-safety §4 / R9) — proves the order runtime's safety
//! logic ENTIRELY against mocks, and NEVER submits a live order.
//!
//! The automated Change-Scoped Gate proves order *logic* — no-retry dispatch,
//! deduplication, the order success predicate, the kill switch, and the six-state
//! reconciliation — through the public [`LsSdk`] surface against a wiremock
//! server. Live evidence is the separate, operator-initiated guarded paper-order
//! run (the live smoke), never part of this gate.
//!
//! Deeper, lower-level coverage lives alongside the implementation: the dedup
//! eviction/no-deadlock contract in `ls-core`'s `order_dedup` unit suite, the
//! predicate's per-code classification in `ls-core`'s `inner` suite, and the
//! matcher's per-state classification in `ls-sdk`'s `orders::reconcile` suite.
//! This file is the consolidated end-to-end gate those feed into.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ls_core::{Inner, LsError};
use ls_sdk::orders::{
    CSPAT00601Request, CSPAT00701Request, CSPAT00801Request, OrderIntent, OrderState, T0425Request,
};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const ORDER_PATH: &str = "/stock/order";
const T0425_PATH: &str = "/stock/accno";

const BUY_ACK: &str = r#"{ "rsp_cd": "00040", "rsp_msg": "ack",
    "CSPAT00601OutBlock2": { "OrdNo": 32004 } }"#;

const T0425_ROW: &str = r#"{ "rsp_cd": "00000",
    "t0425OutBlock1": [ { "ordno": 32004, "expcode": "005930", "medosu": "매수",
        "qty": 1, "price": 60000, "status": "접수" } ] }"#;

fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

const MODIFY_ACK: &str = r#"{ "rsp_cd": "00462", "rsp_msg": "modify ack",
    "CSPAT00701OutBlock2": { "OrdNo": 84007, "PrntOrdNo": 84005 } }"#;

const CANCEL_ACK: &str = r#"{ "rsp_cd": "00156", "rsp_msg": "cancel ack",
    "CSPAT00801OutBlock2": { "OrdNo": 84006, "PrntOrdNo": 84005 } }"#;

fn buy() -> CSPAT00601Request {
    CSPAT00601Request::limit("005930", "1", "60000", "2", "NXT")
}

fn modify() -> CSPAT00701Request {
    CSPAT00701Request::limit("84005", "005930", "1", "8400")
}

fn cancel() -> CSPAT00801Request {
    CSPAT00801Request::new("84005", "005930", "1")
}

/// A 503-forever responder that counts every hit.
struct Counting(Arc<AtomicUsize>, u16);
impl wiremock::Respond for Counting {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(self.1)
    }
}

/// R9 / no-retry: an order 5xx is dispatched exactly ONCE — never the up-to-4
/// retries the read path uses (a blind order retry risks a double fill).
#[tokio::test]
async fn order_5xx_is_a_single_attempt_no_retry() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(Counting(hits.clone(), 503))
        .mount(&server)
        .await;

    let err = sdk_for(&server).orders().submit(&buy()).await.unwrap_err();
    assert!(
        matches!(err, LsError::AmbiguousOrder { .. } | LsError::Http(_)),
        "a 5xx order is ambiguous, got {err:?}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1, "order dispatch must not retry");
}

/// R3 / dedup: an identical submit within the window returns the cached response
/// and never hits HTTP a second time.
#[tokio::test]
async fn identical_submit_is_a_dedup_hit() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_inner = hits.clone();
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(move |_: &Request| {
            hits_inner.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_string(BUY_ACK)
        })
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let a = sdk.orders().submit(&buy()).await.expect("first dispatches");
    let b = sdk.orders().submit(&buy()).await.expect("second is cached");
    assert_eq!(a.order_no(), b.order_no());
    assert_eq!(hits.load(Ordering::SeqCst), 1, "dedup hit must bypass HTTP");
}

/// R4 / predicate: a `00040` ack is Accepted; a non-ack code is `ApiError` with
/// the broker code preserved.
#[tokio::test]
async fn predicate_accepts_acks_and_rejects_others() {
    // Accept.
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(BUY_ACK))
        .mount(&server)
        .await;
    assert_eq!(
        sdk_for(&server).orders().submit(&buy()).await.unwrap().rsp_cd,
        "00040"
    );

    // Reject.
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "40570", "rsp_msg": "limit"
        })))
        .mount(&server)
        .await;
    match sdk_for(&server).orders().submit(&buy()).await.unwrap_err() {
        LsError::ApiError { code, .. } => assert_eq!(code, "40570"),
        other => panic!("expected ApiError, got {other:?}"),
    }
}

/// R2 / kill switch: disabled orders halt before any HTTP; a market read on the
/// same client is unaffected.
#[tokio::test]
async fn kill_switch_halts_orders_only() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_inner = hits.clone();
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .respond_with(move |_: &Request| {
            hits_inner.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_string(BUY_ACK)
        })
        .mount(&server)
        .await;

    let inner = Inner::new(mock_config(&server.uri())).expect("config");
    inner.set_orders_enabled(false);
    let sdk = LsSdk::from_inner(inner);
    let err = sdk.orders().submit(&buy()).await.unwrap_err();
    assert!(matches!(err, LsError::ApiError { code, .. } if code == "orders-disabled"));
    assert_eq!(hits.load(Ordering::SeqCst), 0, "kill switch halts before HTTP");
}

/// R8 / reconciliation: the six states are reachable. Accepted (matching row),
/// Duplicate (dedup hit), Unknown-safe (clean no-match), Unknown-unsafe (failed
/// query), Canceled and Modified (row status).
#[tokio::test]
async fn reconciliation_reaches_the_six_states() {
    async fn outcome_for(body: serde_json::Value, dedup_hit: bool) -> (OrderState, bool) {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(T0425_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let sdk = sdk_for(&server);
        let intent =
            OrderIntent::submit("00000000-01", "005930", "2", "1", "60000", Some("32004".into()));
        let o = sdk.orders().reconcile(&intent, dedup_hit).await;
        (o.state, o.safe_to_retry)
    }

    let accepted: serde_json::Value = serde_json::from_str(T0425_ROW).unwrap();
    assert_eq!(outcome_for(accepted.clone(), false).await.0, OrderState::Accepted);

    // Duplicate (dedup hit) — body irrelevant, no query issued.
    assert_eq!(
        outcome_for(serde_json::json!({"rsp_cd":"00000"}), true).await.0,
        OrderState::Duplicate
    );

    // Unknown but safe to retry — clean query, no matching row.
    let (state, safe) = outcome_for(serde_json::json!({"rsp_cd":"00000"}), false).await;
    assert_eq!(state, OrderState::Unknown);
    assert!(safe);

    // Unknown, not safe — the query itself failed.
    let (state, safe) = outcome_for(
        serde_json::json!({"rsp_cd":"IGW40013","rsp_msg":"fail"}),
        false,
    )
    .await;
    assert_eq!(state, OrderState::Unknown);
    assert!(!safe);

    // Canceled / Modified by row status.
    let canceled = serde_json::json!({"rsp_cd":"00000","t0425OutBlock1":[
        {"ordno":32004,"expcode":"005930","medosu":"매수","status":"취소"}]});
    assert_eq!(outcome_for(canceled, false).await.0, OrderState::Canceled);
    let modified = serde_json::json!({"rsp_cd":"00000","t0425OutBlock1":[
        {"ordno":32004,"expcode":"005930","medosu":"매수","status":"정정"}]});
    assert_eq!(outcome_for(modified, false).await.0, OrderState::Modified);
}

/// The gate never targets a live gateway: every dispatch goes to a loopback
/// wiremock URI, never an `ls-sec.co.kr` host.
#[tokio::test]
async fn gate_targets_only_a_loopback_mock_never_a_live_gateway() {
    let server = MockServer::start().await;
    let uri = server.uri();
    assert!(
        uri.starts_with("http://127.0.0.1") || uri.starts_with("http://[::1]"),
        "the gate must dispatch only to a loopback mock, got {uri}"
    );
    assert!(
        !uri.contains("ls-sec.co.kr"),
        "the gate must never target the real LS gateway"
    );
    // And a non-order read still works against the mock, confirming the harness
    // is wired (token + dispatch) without any live call.
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T0425_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(T0425_ROW))
        .mount(&server)
        .await;
    let sdk = sdk_for(&server);
    let resp = sdk
        .orders()
        .inquiry(&T0425Request::for_symbol("005930"))
        .await
        .expect("offline inquiry");
    assert_eq!(resp.outblock1.len(), 1);
}

// ===========================================================================
// Modify/cancel order-logic gate (U5) — same safety contract, against mocks.
// ===========================================================================

/// R9 / no-retry: a modify AND a cancel 5xx are each dispatched exactly ONCE.
#[tokio::test]
async fn modify_and_cancel_5xx_are_single_attempt_no_retry() {
    for path_under_test in ["modify", "cancel"] {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = Arc::new(AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path(ORDER_PATH))
            .respond_with(Counting(hits.clone(), 503))
            .mount(&server)
            .await;

        let sdk = sdk_for(&server);
        let err = match path_under_test {
            "modify" => sdk.orders().modify(&modify()).await.unwrap_err(),
            _ => sdk.orders().cancel(&cancel()).await.unwrap_err(),
        };
        assert!(
            matches!(err, LsError::AmbiguousOrder { .. } | LsError::Http(_)),
            "{path_under_test} 5xx must be ambiguous, got {err:?}"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "{path_under_test} dispatch must not retry"
        );
    }
}

/// R3 / dedup: an identical modify and an identical cancel within the window each
/// return the cached response and never hit HTTP a second time.
#[tokio::test]
async fn identical_modify_and_cancel_are_dedup_hits() {
    for (path_under_test, body) in [("modify", MODIFY_ACK), ("cancel", CANCEL_ACK)] {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_inner = hits.clone();
        Mock::given(method("POST"))
            .and(path(ORDER_PATH))
            .respond_with(move |_: &Request| {
                hits_inner.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_string(body)
            })
            .mount(&server)
            .await;

        let sdk = sdk_for(&server);
        match path_under_test {
            "modify" => {
                sdk.orders().modify(&modify()).await.expect("first dispatches");
                sdk.orders().modify(&modify()).await.expect("second is cached");
            }
            _ => {
                sdk.orders().cancel(&cancel()).await.expect("first dispatches");
                sdk.orders().cancel(&cancel()).await.expect("second is cached");
            }
        }
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "{path_under_test} dedup hit must bypass HTTP"
        );
    }
}

/// R8 / widened predicate: the modify ack `00462` and the cancel acks
/// `00463`/`00156` classify Accepted; an unrecognized 2xx (incl. `00000`) is
/// Ambiguous → reconciliation (never silently Accepted); a reject code is ApiError.
#[tokio::test]
async fn widened_predicate_classifies_modify_cancel_acks() {
    async fn modify_rsp(body: serde_json::Value) -> LsResultStub {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(ORDER_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        match sdk_for(&server).orders().modify(&modify()).await {
            Ok(r) => LsResultStub::Ok(r.rsp_cd),
            Err(LsError::AmbiguousOrder { .. }) => LsResultStub::Ambiguous,
            Err(LsError::ApiError { code, .. }) => LsResultStub::Api(code),
            Err(e) => panic!("unexpected {e:?}"),
        }
    }

    // 00462 modify ack -> Accepted.
    assert!(matches!(
        modify_rsp(serde_json::json!({"rsp_cd":"00462","CSPAT00701OutBlock2":{"OrdNo":84007}})).await,
        LsResultStub::Ok(c) if c == "00462"
    ));
    // 00000 / unrecognized 2xx -> Ambiguous (the double-fill guard), NEVER Accepted.
    assert!(matches!(
        modify_rsp(serde_json::json!({"rsp_cd":"00000"})).await,
        LsResultStub::Ambiguous
    ));
    // A reject code -> ApiError, code preserved.
    assert!(matches!(
        modify_rsp(serde_json::json!({"rsp_cd":"03181","rsp_msg":"band"})).await,
        LsResultStub::Api(c) if c == "03181"
    ));

    // Cancel acks 00463 and 00156 both Accepted.
    for ack in ["00463", "00156"] {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(ORDER_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": ack, "CSPAT00801OutBlock2": {"OrdNo": 84006}
            })))
            .mount(&server)
            .await;
        assert_eq!(
            sdk_for(&server).orders().cancel(&cancel()).await.unwrap().rsp_cd,
            ack
        );
    }
}

/// A tiny result discriminator for the predicate matrix.
enum LsResultStub {
    Ok(String),
    Ambiguous,
    Api(String),
}

/// R2 / kill switch: a disabled order halts the modify AND the cancel before HTTP.
#[tokio::test]
async fn kill_switch_halts_modify_and_cancel() {
    for path_under_test in ["modify", "cancel"] {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_inner = hits.clone();
        Mock::given(method("POST"))
            .and(path(ORDER_PATH))
            .respond_with(move |_: &Request| {
                hits_inner.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_string(BUY_ACK)
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("config");
        inner.set_orders_enabled(false);
        let sdk = LsSdk::from_inner(inner);
        let err = match path_under_test {
            "modify" => sdk.orders().modify(&modify()).await.unwrap_err(),
            _ => sdk.orders().cancel(&cancel()).await.unwrap_err(),
        };
        assert!(matches!(err, LsError::ApiError { code, .. } if code == "orders-disabled"));
        assert_eq!(hits.load(Ordering::SeqCst), 0, "{path_under_test} halts before HTTP");
    }
}

/// R6/R7 / order-state reconciliation: the modify/cancel classifications are
/// reachable through the public `reconcile()` surface against mocks — Modified
/// (정정 + child), Canceled, still-live-not-canceled, safe-to-retry-on-no-match,
/// and Unknown-not-safe on a failed query.
#[tokio::test]
async fn modify_cancel_reconciliation_classifications_via_public_surface() {
    async fn outcome(intent: OrderIntent, body: serde_json::Value) -> (OrderState, bool) {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(T0425_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let o = sdk_for(&server).orders().reconcile(&intent, false).await;
        (o.state, o.safe_to_retry)
    }

    let modify_intent = || modify().reconcile_intent("00000000-01");
    let cancel_intent = || cancel().reconcile_intent("00000000-01");

    // Modify landed: the original OrgOrdNo row shows 정정.
    let row = |ordno: u64, orgordno: u64, status: &str| {
        serde_json::json!({"ordno":ordno,"orgordno":orgordno,"expcode":"005930",
            "medosu":"매수","qty":1,"price":8400,"status":status})
    };
    assert_eq!(
        outcome(modify_intent(), serde_json::json!({"rsp_cd":"00000",
            "t0425OutBlock1":[row(84005,0,"정정")]})).await.0,
        OrderState::Modified
    );
    // Modify landed via a child row (orgordno == OrgOrdNo), original still 접수.
    assert_eq!(
        outcome(modify_intent(), serde_json::json!({"rsp_cd":"00000",
            "t0425OutBlock1":[row(84005,0,"접수"), row(84007,84005,"접수")]})).await.0,
        OrderState::Modified
    );
    // Modify NOT landed: bare 접수 original, no 정정, no child -> safe to retry.
    let (state, safe) = outcome(modify_intent(), serde_json::json!({"rsp_cd":"00000",
        "t0425OutBlock1":[row(84005,0,"접수")]})).await;
    assert_eq!(state, OrderState::Unknown);
    assert!(safe, "an un-applied absolute modify is safe to re-send");

    // Cancel still-live: the original rests at 접수 -> NOT canceled, never safe.
    let (state, safe) = outcome(cancel_intent(), serde_json::json!({"rsp_cd":"00000",
        "t0425OutBlock1":[row(84005,0,"접수")]})).await;
    assert_eq!(state, OrderState::Unknown);
    assert_ne!(state, OrderState::Canceled);
    assert!(!safe, "a still-resting order must never clear retry on a cancel");
    // Cancel landed: a 취소 child row outranks the still-접수 original.
    assert_eq!(
        outcome(cancel_intent(), serde_json::json!({"rsp_cd":"00000",
            "t0425OutBlock1":[row(84005,0,"접수"), row(84006,84005,"취소")]})).await.0,
        OrderState::Canceled
    );
    // No match over a clean query -> safe to retry.
    let (_, safe) = outcome(cancel_intent(), serde_json::json!({"rsp_cd":"00000",
        "t0425OutBlock1":[row(99999,0,"접수")]})).await;
    assert!(safe);
    // A failed query -> Unknown, not safe (cannot prove absence).
    let (state, safe) = outcome(cancel_intent(),
        serde_json::json!({"rsp_cd":"IGW40013","rsp_msg":"fail"})).await;
    assert_eq!(state, OrderState::Unknown);
    assert!(!safe);
}
