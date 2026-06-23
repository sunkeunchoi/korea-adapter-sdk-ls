//! Account dependency-class tests (`CSPAQ12200`, `CSPAQ12300`, `CSPAQ22200`,
//! `CFOBQ10500`).
//!
//! All four are read-only account-state reads sharing the same discipline: the
//! request is built from the CONFIG-supplied account, never a caller identifier,
//! and the account number never appears in the body. They differ only in request
//! shape — `CSPAQ12200` (single `BalCreTp`), `CSPAQ12300` (four query-shape
//! enums), `CSPAQ22200` (single `BalCreTp`), and `CFOBQ10500` (empty in-block,
//! three out-blocks) — and endpoint (`/stock/accno` vs `CFOBQ10500`'s
//! `/futureoption/accno`).
//!
//! The defining facet is `account_state: true`, so the Change-Scoped Gate selects
//! ONLY credential-free request-construction tests for these TRs. These tests prove:
//!   - the request constructs from the CONFIG-supplied account (never a caller
//!     identifier) with `BalCreTp`, serializing to `{"CSPAQ12200InBlock1":{...}}`
//!     WITHOUT a network call,
//!   - the response deserializes from the spec-derived, SYNTHETIC fixture with the
//!     key balance fields (`MnyOrdAbleAmt`, `BalEvalAmt`, …) asserted,
//!   - `CSPAQ12200OutBlock2` tolerates a single object via `de_vec_or_single`,
//!   - and that `01715` (date) and `01900` (paper-incompatible) classify DISTINCTLY
//!     via the structured `rsp_cd`.
//!
//! No credentialed live call is attempted: credentialed evidence is scheduled
//! separately and is out of the unit suite. The wiremock-backed deserialize test
//! exercises real `ls-core` dispatch against a MOCK token + MOCK response — it uses
//! the dummy `TEST_ACCOUNT_NO` from `mock_config`, never a real account.

use std::sync::Arc;

use ls_core::{Inner, LsError};
use ls_sdk::account::{
    CFOBQ10500Request, CFOBQ10500Response, CSPAQ12200Request, CSPAQ12200Response, CSPAQ12300Request,
    CSPAQ12300Response, CSPAQ22200Request, CSPAQ22200Response,
};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token, TEST_ACCOUNT_NO};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The spec-derived, SYNTHETIC `CSPAQ12200` response fixture.
const CSPAQ12200_FIXTURE: &str = include_str!("fixtures/CSPAQ12200_resp.json");

/// The spec-derived, SYNTHETIC `CSPAQ12300` response fixture.
const CSPAQ12300_FIXTURE: &str = include_str!("fixtures/CSPAQ12300_resp.json");

/// The spec-derived, SYNTHETIC `CSPAQ22200` response fixture.
const CSPAQ22200_FIXTURE: &str = include_str!("fixtures/CSPAQ22200_resp.json");

/// The spec-derived, SYNTHETIC `CFOBQ10500` response fixture.
const CFOBQ10500_FIXTURE: &str = include_str!("fixtures/CFOBQ10500_resp.json");

/// The shared REST path for the `/stock/accno` account TRs (`CSPAQ12200`,
/// `CSPAQ12300`, `CSPAQ22200`) — they mount the same endpoint and discriminate
/// on the `tr_cd` header. (`CFOBQ10500` uses `/futureoption/accno`, spelled
/// inline in its test.)
const STOCK_ACCNO_PATH: &str = "/stock/accno";

/// Build an `LsSdk` whose dispatch is pointed at the mock server.
fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

/// Happy path — credential-free construction. The request is built from the
/// config-supplied account (the `Account` handle exposes `account_no()` off the
/// `ResolvedConfig`, NOT a caller identifier) plus a caller-chosen `BalCreTp`, and
/// serializes to exactly `{"CSPAQ12200InBlock1":{"BalCreTp":...}}` with NO network
/// call and NO account number in the body.
#[test]
fn request_constructs_from_config_account_and_serializes_inblock_only() {
    // The account is sourced from config (mock_config sets TEST_ACCOUNT_NO), not
    // from any caller-passed identifier. We never thread it into the request.
    let inner = Inner::new(mock_config("http://unused.invalid")).expect("valid mock config");
    let sdk = LsSdk::from_inner(inner);
    let account = sdk.account();
    assert_eq!(
        account.account_no(),
        TEST_ACCOUNT_NO,
        "account number is the config-supplied dummy, not a caller identifier"
    );

    // The only caller-supplied input is BalCreTp.
    let mut req = CSPAQ12200Request::new("1");
    // Even if the transport continuation is set, it must not leak into the body.
    req.tr_cont = "Y".into();
    req.tr_cont_key = "morekey".into();

    let value = serde_json::to_value(&req).expect("serialize CSPAQ12200 request");

    // Exactly one top-level key: CSPAQ12200InBlock1.
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CSPAQ12200InBlock1"),
        "missing CSPAQ12200InBlock1 key"
    );

    let inblock = &value["CSPAQ12200InBlock1"];
    // BalCreTp is present...
    assert_eq!(inblock["BalCreTp"], "1", "BalCreTp rides in the body");
    // ...and it is the ONLY field — no account number, no continuation.
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        1,
        "InBlock1 carries only BalCreTp (no account number, no continuation)"
    );

    // Transport continuation NEVER serializes into the body (top level or inblock).
    assert!(
        value.get("tr_cont").is_none(),
        "tr_cont must not be in the body"
    );
    assert!(
        value.get("tr_cont_key").is_none(),
        "tr_cont_key must not be in the body"
    );
    assert!(
        inblock.get("tr_cont").is_none(),
        "tr_cont must not be in the inblock"
    );
    assert!(
        inblock.get("tr_cont_key").is_none(),
        "tr_cont_key must not be in the inblock"
    );

    // No account number leaked anywhere into the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CSPAQ12200 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// Happy path — the response deserializes from the spec-derived SYNTHETIC fixture
/// with the key balance fields asserted. Exercises REAL `ls-core` dispatch against
/// a mock token + mock response (the mock config injects `base_url` and the dummy
/// `TEST_ACCOUNT_NO`); this is NOT a credentialed live call.
#[tokio::test]
async fn balance_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .and(header("tr_cd", "CSPAQ12200"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CSPAQ12200_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CSPAQ12200Request::new("1");
    let resp = sdk
        .account()
        .balance(&req)
        .await
        .expect("CSPAQ12200 balance inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    // Identity summary block (account number redacted in Debug, present in struct).
    assert_eq!(resp.outblock1.balcretp, "1");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);

    // Balance block — the key orderable-amount / valuation fields, coerced from
    // JSON numbers to String via string_or_number.
    assert_eq!(resp.outblock2.len(), 1, "one balance row");
    let bal = &resp.outblock2[0];
    assert_eq!(bal.mnyordableamt, "1000000", "MnyOrdAbleAmt");
    assert_eq!(bal.balevalamt, "2500000", "BalEvalAmt");
    assert_eq!(bal.dpsasttotamt, "3500000", "DpsastTotamt");
    assert_eq!(bal.pnlrat, "12.345678", "PnlRat (arrives as a string)");
}

/// Edge — `CSPAQ12200OutBlock2` arriving as a SINGLE object (not an array)
/// deserializes via `de_vec_or_single` into a 1-element Vec. The gateway collapses
/// a one-row block to a bare object.
#[test]
fn out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ12200OutBlock1": { "BalCreTp": "1", "AcntNo": "00000000-01" },
        "CSPAQ12200OutBlock2": {
            "MnyOrdAbleAmt": 500000,
            "BalEvalAmt": 750000
        }
    });
    let resp: CSPAQ12200Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(
        resp.outblock2.len(),
        1,
        "single object becomes a 1-element Vec"
    );
    assert_eq!(resp.outblock2[0].mnyordableamt, "500000");
    assert_eq!(resp.outblock2[0].balevalamt, "750000");
}

/// Error — `01715` (date) and `01900` (paper-incompatible) classify DISTINCTLY via
/// the structured `rsp_cd`. `01900` is the SOLE paper-incompatible signal
/// (`is_paper_incompatible()` true); `01715` is a normal `ApiError` with code
/// `"01715"` and `is_paper_incompatible()` false. The two never collapse together.
///
/// Both are driven through REAL `ls-core` dispatch against mock responses — no
/// credentialed call. The assertion is that the two classifications DIFFER.
#[tokio::test]
async fn errors_01715_and_01900_classify_distinctly() {
    // --- 01900: paper-incompatible ---
    let server_1900 = MockServer::start().await;
    mount_token(&server_1900).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01900",
            "rsp_msg": "모의투자 미지원 업무입니다."
        })))
        .mount(&server_1900)
        .await;

    let err_1900 = sdk_for(&server_1900)
        .account()
        .balance(&CSPAQ12200Request::new("1"))
        .await
        .expect_err("01900 must surface as an error");

    // --- 01715: date error, NOT paper-incompatible ---
    let server_1715 = MockServer::start().await;
    mount_token(&server_1715).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01715",
            "rsp_msg": "조회기간 오류입니다."
        })))
        .mount(&server_1715)
        .await;

    let err_1715 = sdk_for(&server_1715)
        .account()
        .balance(&CSPAQ12200Request::new("1"))
        .await
        .expect_err("01715 must surface as an error");

    // Both are ApiErrors carrying their codes verbatim (classified on rsp_cd, not
    // rsp_msg substrings).
    match &err_1900 {
        LsError::ApiError { code, .. } => assert_eq!(code, "01900"),
        other => panic!("expected ApiError 01900, got {other:?}"),
    }
    match &err_1715 {
        LsError::ApiError { code, .. } => assert_eq!(code, "01715"),
        other => panic!("expected ApiError 01715, got {other:?}"),
    }

    // The distinct classification: 01900 is paper-incompatible, 01715 is not.
    assert!(
        err_1900.is_paper_incompatible(),
        "01900 must classify as paper-incompatible"
    );
    assert!(
        !err_1715.is_paper_incompatible(),
        "01715 must NOT classify as paper-incompatible"
    );
    assert_ne!(
        err_1900.is_paper_incompatible(),
        err_1715.is_paper_incompatible(),
        "01900 and 01715 must classify distinctly"
    );
}

// ---------------------------------------------------------------------------
// CSPAQ12300 — BEP단가조회 (read-only account BEP/balance inquiry).
// ---------------------------------------------------------------------------

/// `::new` serializes the four query-shape enums under `CSPAQ12300InBlock1` with
/// NO account number / caller leak. The account is config-supplied, never threaded
/// into the body.
#[test]
fn cspaq12300_request_serializes_inblock_only_no_account_leak() {
    let req = CSPAQ12300Request::new("1", "0", "0", "0");
    let value = serde_json::to_value(&req).expect("serialize CSPAQ12300 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CSPAQ12300InBlock1"),
        "missing CSPAQ12300InBlock1 key"
    );

    let inblock = &value["CSPAQ12300InBlock1"];
    assert_eq!(inblock["BalCreTp"], "1", "BalCreTp rides in the body");
    assert_eq!(inblock["CmsnAppTpCode"], "0");
    assert_eq!(inblock["D2balBaseQryTp"], "0");
    assert_eq!(inblock["UprcTpCode"], "0");
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        4,
        "InBlock1 carries only the four enum selectors (no account number)"
    );

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CSPAQ12300 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`MnyOrdAbleAmt` = 현금주문가능금액, KTD4) holds its exact value. Distinct fields
/// carry distinct values so a mislabel cannot be masked.
#[tokio::test]
async fn cspaq12300_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .and(header("tr_cd", "CSPAQ12300"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CSPAQ12300_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CSPAQ12300Request::new("1", "0", "0", "0");
    let resp = sdk
        .account()
        .bep(&req)
        .await
        .expect("CSPAQ12300 BEP inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.balcretp, "1");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);

    assert_eq!(resp.outblock2.len(), 1, "one balance row");
    let bal = &resp.outblock2[0];
    // Canonical field by Korean name (현금주문가능금액) — exact value, not !is_empty().
    assert_eq!(bal.mnyordableamt, "1234567", "MnyOrdAbleAmt (현금주문가능금액)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(bal.mnyoutableamt, "1200000", "MnyoutAbleAmt");
    assert_eq!(bal.balevalamt, "2500000", "BalEvalAmt");
    assert_eq!(bal.dpsasttotamt, "3500000", "DpsastTotamt");
    assert_eq!(bal.dps, "1100000", "Dps");
    assert_eq!(bal.ordableamt, "1300000", "OrdAbleAmt");
}

/// Numeric-bearing fields parse via `string_or_number` from BOTH string and number
/// JSON.
#[test]
fn cspaq12300_numeric_fields_parse_from_string_and_number() {
    // Numbers as JSON numbers.
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ12300OutBlock2": { "MnyOrdAbleAmt": 999, "BalEvalAmt": 42 }
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].mnyordableamt, "999");
    assert_eq!(resp.outblock2[0].balevalamt, "42");

    // Same fields as JSON strings.
    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ12300OutBlock2": { "MnyOrdAbleAmt": "999", "BalEvalAmt": "42" }
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].mnyordableamt, "999");
    assert_eq!(resp.outblock2[0].balevalamt, "42");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes and is recognized
/// as the empty/pending case.
#[test]
fn cspaq12300_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CSPAQ12300OutBlock2": []
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty balance Vec (the PENDING case)"
    );
}

/// `CSPAQ12300OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cspaq12300_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ12300OutBlock2": { "MnyOrdAbleAmt": 500000, "BalEvalAmt": 750000 }
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(
        resp.outblock2.len(),
        1,
        "single object becomes a 1-element Vec"
    );
    assert_eq!(resp.outblock2[0].mnyordableamt, "500000");
    assert_eq!(resp.outblock2[0].balevalamt, "750000");
}

// ---------------------------------------------------------------------------
// CSPAQ22200 — 현물계좌예수금 주문가능금액 총평가2 (read-only account orderable
// amount / valuation inquiry).
// ---------------------------------------------------------------------------

/// `::new` serializes only `BalCreTp` under `CSPAQ22200InBlock1` with NO account
/// number / caller leak. The account is config-supplied, never threaded into the
/// body.
#[test]
fn cspaq22200_request_serializes_inblock_only_no_account_leak() {
    let req = CSPAQ22200Request::new("1");
    let value = serde_json::to_value(&req).expect("serialize CSPAQ22200 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CSPAQ22200InBlock1"),
        "missing CSPAQ22200InBlock1 key"
    );

    let inblock = &value["CSPAQ22200InBlock1"];
    assert_eq!(inblock["BalCreTp"], "1", "BalCreTp rides in the body");
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        1,
        "InBlock1 carries only BalCreTp (no account number)"
    );

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CSPAQ22200 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`MnyOrdAbleAmt` = 현금주문가능금액, KTD4) holds its exact value. Distinct fields
/// carry distinct values so a mislabel cannot be masked.
#[tokio::test]
async fn cspaq22200_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .and(header("tr_cd", "CSPAQ22200"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CSPAQ22200_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CSPAQ22200Request::new("1");
    let resp = sdk
        .account()
        .orderable(&req)
        .await
        .expect("CSPAQ22200 orderable inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.balcretp, "1");
    assert_eq!(resp.outblock1.mgmtbrnno, "001");

    assert_eq!(resp.outblock2.len(), 1, "one valuation row");
    let bal = &resp.outblock2[0];
    // Canonical field by Korean name (현금주문가능금액) — exact value, not !is_empty().
    assert_eq!(bal.mnyordableamt, "1234567", "MnyOrdAbleAmt (현금주문가능금액)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(bal.substordableamt, "2222222", "SubstOrdAbleAmt (대용주문가능금액)");
    assert_eq!(bal.seordableamt, "3333333", "SeOrdAbleAmt (거래소금액)");
    assert_eq!(bal.kdqordableamt, "4444444", "KdqOrdAbleAmt (코스닥금액)");
    assert_eq!(bal.dps, "1100000", "Dps (예수금)");
    assert_eq!(bal.d2dps, "1250000", "D2Dps (D2예수금)");
}

/// Numeric-bearing fields parse via `string_or_number` from BOTH string and number
/// JSON.
#[test]
fn cspaq22200_numeric_fields_parse_from_string_and_number() {
    // Numbers as JSON numbers.
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ22200OutBlock2": { "MnyOrdAbleAmt": 999, "Dps": 42 }
    });
    let resp: CSPAQ22200Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].mnyordableamt, "999");
    assert_eq!(resp.outblock2[0].dps, "42");

    // Same fields as JSON strings.
    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ22200OutBlock2": { "MnyOrdAbleAmt": "999", "Dps": "42" }
    });
    let resp: CSPAQ22200Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].mnyordableamt, "999");
    assert_eq!(resp.outblock2[0].dps, "42");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes and is recognized
/// as the empty/pending case.
#[test]
fn cspaq22200_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CSPAQ22200OutBlock2": []
    });
    let resp: CSPAQ22200Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty valuation Vec (the PENDING case)"
    );
}

/// `CSPAQ22200OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cspaq22200_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ22200OutBlock2": { "MnyOrdAbleAmt": 500000, "Dps": 750000 }
    });
    let resp: CSPAQ22200Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(
        resp.outblock2.len(),
        1,
        "single object becomes a 1-element Vec"
    );
    assert_eq!(resp.outblock2[0].mnyordableamt, "500000");
    assert_eq!(resp.outblock2[0].dps, "750000");
}

// ---------------------------------------------------------------------------
// CFOBQ10500 — 선물옵션 계좌예탁금증거금조회 (read-only F/O account deposit / margin
// inquiry). Header-only: no request-body fields, a no-argument `::new()`. THREE
// out-blocks: OutBlock1 single + OutBlock2 + OutBlock3 each as a `Vec`.
// ---------------------------------------------------------------------------

/// `::new()` takes no args and serializes an EMPTY in-block under `CFOBQ10500InBlock`
/// with no caller fields leaking (no account number, no body fields at all).
#[test]
fn cfobq10500_request_serializes_empty_inblock_no_account_leak() {
    let req = CFOBQ10500Request::new();
    let value = serde_json::to_value(&req).expect("serialize CFOBQ10500 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CFOBQ10500InBlock"),
        "missing CFOBQ10500InBlock key"
    );

    let inblock = &value["CFOBQ10500InBlock"];
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        0,
        "InBlock carries no caller fields (header-only read)"
    );

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CFOBQ10500 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`DpsamtTotamt` = 예탁금총액, KTD4) holds its exact value. Distinct neighbours
/// carry distinct values so a mislabel cannot be masked.
#[tokio::test]
async fn cfobq10500_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/futureoption/accno"))
        .and(header("tr_cd", "CFOBQ10500"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CFOBQ10500_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CFOBQ10500Request::new();
    let resp = sdk
        .account()
        .fo_deposit(&req)
        .await
        .expect("CFOBQ10500 F/O deposit inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);

    assert_eq!(resp.outblock2.len(), 1, "one deposit row");
    let dep = &resp.outblock2[0];
    // Canonical field by Korean name (예탁금총액) — exact value, not !is_empty().
    assert_eq!(dep.dpsamttotamt, "5500000", "DpsamtTotamt (예탁금총액)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(dep.dps, "5000000", "Dps (예수금)");
    assert_eq!(dep.substamt, "500000", "SubstAmt (대용금액)");
    assert_eq!(dep.wthdwableamt, "4800000", "WthdwAbleAmt (인출가능금액)");
    assert_eq!(dep.mgn, "1200000", "Mgn (증거금액)");
    assert_eq!(dep.ordableamt, "3700000", "OrdAbleAmt (주문가능금액)");

    assert_eq!(resp.outblock3.len(), 1, "one margin-breakdown row");
    let mgn = &resp.outblock3[0];
    assert_eq!(mgn.pdgrpcodenm, "KOSPI200선물", "PdGrpCodeNm (상품군코드명)");
    assert_eq!(mgn.netriskmgn, "1100000", "NetRiskMgn (순위험증거금액)");
    assert_eq!(mgn.maintmgn, "1050000", "MaintMgn (유지증거금액)");
}

/// Numeric-bearing fields parse via `string_or_number` from BOTH string and number
/// JSON.
#[test]
fn cfobq10500_numeric_fields_parse_from_string_and_number() {
    // Numbers as JSON numbers.
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CFOBQ10500OutBlock2": { "DpsamtTotamt": 999, "Dps": 42 }
    });
    let resp: CFOBQ10500Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].dpsamttotamt, "999");
    assert_eq!(resp.outblock2[0].dps, "42");

    // Same fields as JSON strings.
    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CFOBQ10500OutBlock2": { "DpsamtTotamt": "999", "Dps": "42" }
    });
    let resp: CFOBQ10500Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].dpsamttotamt, "999");
    assert_eq!(resp.outblock2[0].dps, "42");
}

/// The empty-deposit case: a `rsp_cd 00707` with empty out-blocks deserializes and
/// is recognized as the empty/pending case (a position-less paper account). This is
/// the expected PENDING outcome for CFOBQ10500, not a defect.
#[test]
fn cfobq10500_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CFOBQ10500OutBlock2": [],
        "CFOBQ10500OutBlock3": []
    });
    let resp: CFOBQ10500Response =
        serde_json::from_value(json).expect("empty out-blocks must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty deposit Vec (the PENDING case)"
    );
    assert!(
        resp.outblock3.is_empty(),
        "00707 yields an empty margin Vec (the PENDING case)"
    );
}

/// Both `CFOBQ10500OutBlock2` and `CFOBQ10500OutBlock3` arriving as a SINGLE object
/// (not an array) deserialize via `de_vec_or_single` into 1-element Vecs.
#[test]
fn cfobq10500_out_blocks_single_object_deserialize_to_one_element_vecs() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CFOBQ10500OutBlock2": { "DpsamtTotamt": 5500000, "Dps": 5000000 },
        "CFOBQ10500OutBlock3": { "PdGrpCodeNm": "KOSPI200선물", "MaintMgn": 1050000 }
    });
    let resp: CFOBQ10500Response =
        serde_json::from_value(json).expect("single-object out-blocks must deserialize");
    assert_eq!(
        resp.outblock2.len(),
        1,
        "OutBlock2 single object becomes a 1-element Vec"
    );
    assert_eq!(resp.outblock2[0].dpsamttotamt, "5500000");
    assert_eq!(
        resp.outblock3.len(),
        1,
        "OutBlock3 single object becomes a 1-element Vec"
    );
    assert_eq!(resp.outblock3[0].maintmgn, "1050000");
}
