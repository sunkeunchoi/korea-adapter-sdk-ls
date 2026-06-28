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
    CCENQ10100Request, CCENQ10100Response, CCENQ90200Request, CCENQ90200Response, CFOAQ10100Request,
    CFOAQ10100Response, CFOBQ10500Request, CFOBQ10500Response, CSPAQ12200Request, CSPAQ12200Response,
    CSPAQ12300Request, CSPAQ12300Response, CSPAQ22200Request, CSPAQ22200Response,
    CSPBQ00200Request, CSPBQ00200Response, T0424Request, T0424Response,
    CLNAQ00100Request, CLNAQ00100Response, CFOEQ11100Request, CFOEQ11100Response,
    T0441Request, T0441Response, CIDBQ01400Request, CIDBQ01400Response,
    CIDBQ03000Request, CIDBQ03000Response, CIDBQ05300Request, CIDBQ05300Response,
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

/// The spec-derived, SYNTHETIC `CCENQ90200` response fixture.
const CCENQ90200_FIXTURE: &str = include_str!("fixtures/CCENQ90200_resp.json");

/// The spec-derived, SYNTHETIC `CFOAQ10100` response fixture.
const CFOAQ10100_FIXTURE: &str = include_str!("fixtures/CFOAQ10100_resp.json");

/// The spec-derived, SYNTHETIC `CCENQ10100` response fixture.
const CCENQ10100_FIXTURE: &str = include_str!("fixtures/CCENQ10100_resp.json");

/// The spec-derived, SYNTHETIC `t0424` response fixture (cash summary + one holding).
const T0424_FIXTURE: &str = include_str!("fixtures/t0424_resp.json");

/// The spec-derived, SYNTHETIC `CSPBQ00200` response fixture (capacity block).
const CSPBQ00200_FIXTURE: &str = include_str!("fixtures/CSPBQ00200_resp.json");

/// The spec-derived, SYNTHETIC `CLNAQ00100` response fixture (loanable-stock list).
const CLNAQ00100_FIXTURE: &str = include_str!("fixtures/CLNAQ00100_resp.json");

/// The spec-derived, SYNTHETIC `CFOEQ11100` response fixture (F/O deposit detail).
const CFOEQ11100_FIXTURE: &str = include_str!("fixtures/CFOEQ11100_resp.json");

/// The spec-derived, SYNTHETIC `t0441` response fixture (F/O balance valuation).
const T0441_FIXTURE: &str = include_str!("fixtures/t0441_resp.json");

/// The spec-derived, SYNTHETIC `CIDBQ01400` response fixture (overseas order-qty).
const CIDBQ01400_FIXTURE: &str = include_str!("fixtures/CIDBQ01400_resp.json");

/// The spec-derived, SYNTHETIC `CIDBQ03000` response fixture (overseas deposit/balance).
const CIDBQ03000_FIXTURE: &str = include_str!("fixtures/CIDBQ03000_resp.json");

/// The spec-derived, SYNTHETIC `CIDBQ05300` response fixture (overseas deposited assets).
const CIDBQ05300_FIXTURE: &str = include_str!("fixtures/CIDBQ05300_resp.json");

/// The shared REST path for the `/futureoption/accno` account TRs (`CFOBQ10500`,
/// `CCENQ90200`, `CFOAQ10100`, `CCENQ10100`) — they mount the same endpoint and
/// discriminate on the `tr_cd` header.
const FUTUREOPTION_ACCNO_PATH: &str = "/futureoption/accno";

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

// ---------------------------------------------------------------------------
// CCENQ90200 — KRX야간파생 잔고조회 (read-only night-derivatives account balance,
// krx_extended). InBlock1: RecCnt (JSON number) + two evaluation-shape enums.
// THREE out-blocks: OutBlock1 single + OutBlock2 + OutBlock3 (a true JSON array).
// ---------------------------------------------------------------------------

/// `::new` serializes the record count (as a JSON number, KTD4) + two enums under
/// `CCENQ90200InBlock1` with NO account number / caller leak. The account is
/// config-supplied, never threaded into the body.
#[test]
fn ccenq90200_request_serializes_inblock_only_no_account_leak() {
    let req = CCENQ90200Request::new("1", "0", "0");
    let value = serde_json::to_value(&req).expect("serialize CCENQ90200 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CCENQ90200InBlock1"),
        "missing CCENQ90200InBlock1 key"
    );

    let inblock = &value["CCENQ90200InBlock1"];
    // RecCnt serializes as a JSON NUMBER, not a string (KTD4 — avoids IGW40011).
    assert!(
        inblock["RecCnt"].is_number(),
        "RecCnt must serialize as a JSON number"
    );
    assert_eq!(inblock["RecCnt"], 1);
    assert_eq!(inblock["BalEvalTp"], "0");
    assert_eq!(inblock["FutsPrcEvalTp"], "0");
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        3,
        "InBlock1 carries only the three shape fields (no account number)"
    );

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CCENQ90200 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`EvalDpsamtTotamt` = 평가예탁금총액, KTD6) holds its exact value. Distinct
/// neighbours carry distinct values so a mislabel cannot be masked. The OutBlock3
/// array deserializes into a Vec.
#[tokio::test]
async fn ccenq90200_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FUTUREOPTION_ACCNO_PATH))
        .and(header("tr_cd", "CCENQ90200"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CCENQ90200_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CCENQ90200Request::new("1", "0", "0");
    let resp = sdk
        .account()
        .night_balance(&req)
        .await
        .expect("CCENQ90200 night-balance inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);
    assert_eq!(resp.outblock1.balevaltp, "2");

    assert_eq!(resp.outblock2.len(), 1, "one balance row");
    let bal = &resp.outblock2[0];
    // Canonical field by Korean name (평가예탁금총액) — exact value, not !is_empty().
    assert_eq!(bal.evaldpsamttotamt, "34399538", "EvalDpsamtTotamt (평가예탁금총액)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(bal.dpsamttotamt, "31925203", "DpsamtTotamt (예탁금총액)");
    assert_eq!(bal.psnoutabletotamt, "20321010", "PsnOutAbleTotAmt (인출가능총금액)");
    assert_eq!(bal.ordabletotamt, "20327175", "OrdAbleTotAmt (주문가능총금액)");
    assert_eq!(bal.mnyordableamt, "20327176", "MnyOrdAbleAmt (현금주문가능금액)");
    assert_eq!(bal.evalpnlsum, "6288000", "EvalPnlSum (평가손익합계)");

    // OutBlock3 is a true JSON array in the raw capture → Vec.
    assert_eq!(resp.outblock3.len(), 1, "one position row");
    assert_eq!(resp.outblock3[0].fnoisuno, "105W6000", "FnoIsuNo");
    assert_eq!(resp.outblock3[0].unsttqty, "2", "UnsttQty (미결제수량)");
    assert_eq!(resp.outblock3[0].evalamt, "40658000", "EvalAmt (평가금액)");
}

/// Numeric-bearing out-block fields parse via `string_or_number` from BOTH string
/// and number JSON.
#[test]
fn ccenq90200_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ90200OutBlock2": { "EvalDpsamtTotamt": 999, "EvalPnlSum": 42 }
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].evaldpsamttotamt, "999");
    assert_eq!(resp.outblock2[0].evalpnlsum, "42");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ90200OutBlock2": { "EvalDpsamtTotamt": "999", "EvalPnlSum": "42" }
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].evaldpsamttotamt, "999");
    assert_eq!(resp.outblock2[0].evalpnlsum, "42");
}

/// An empty result (`rsp_cd 00707`, empty out-blocks) deserializes and is recognized
/// as the empty/pending case (off the krx_extended night window or a position-less
/// account).
#[test]
fn ccenq90200_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CCENQ90200OutBlock2": [],
        "CCENQ90200OutBlock3": []
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(json).expect("empty out-blocks must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty balance Vec (the PENDING case)"
    );
    assert!(resp.outblock3.is_empty(), "00707 yields an empty position Vec");
}

/// Both `CCENQ90200OutBlock2` and `CCENQ90200OutBlock3` arriving as a SINGLE object
/// deserialize via `de_vec_or_single` into 1-element Vecs.
#[test]
fn ccenq90200_out_blocks_single_object_deserialize_to_one_element_vecs() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ90200OutBlock2": { "EvalDpsamtTotamt": 34399538, "EvalPnlSum": 6288000 },
        "CCENQ90200OutBlock3": { "FnoIsuNo": "105W6000", "EvalAmt": 40658000 }
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(json).expect("single-object out-blocks must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "OutBlock2 single → 1-element Vec");
    assert_eq!(resp.outblock2[0].evaldpsamttotamt, "34399538");
    assert_eq!(resp.outblock3.len(), 1, "OutBlock3 single → 1-element Vec");
    assert_eq!(resp.outblock3[0].fnoisuno, "105W6000");
}

// ---------------------------------------------------------------------------
// CFOAQ10100 — 선물옵션 주문가능수량조회 (read-only orderable-quantity INQUIRY, NOT an
// order). InBlock1: four numeric fields (RecCnt/OrdAmt/RatVal/FnoOrdPrc) as JSON
// numbers + caller-supplied order-shape enums incl. FnoIsuNo. OutBlock1 single +
// OutBlock2 (orderable-quantity result).
// ---------------------------------------------------------------------------

/// `::new` serializes the order-shape inputs under `CFOAQ10100InBlock1` with the
/// four numeric fields as JSON numbers (KTD4) and NO account number / caller leak.
#[test]
fn cfoaq10100_request_serializes_numeric_fields_and_no_account_leak() {
    let req = CFOAQ10100Request::new("1", "1", "0", "0", "101T6000", "1", "0", "00");
    let value = serde_json::to_value(&req).expect("serialize CFOAQ10100 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CFOAQ10100InBlock1"),
        "missing CFOAQ10100InBlock1 key"
    );

    let inblock = &value["CFOAQ10100InBlock1"];
    // The four numeric fields serialize as JSON numbers (KTD4 — avoids IGW40011).
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number");
    assert!(inblock["OrdAmt"].is_number(), "OrdAmt must be a JSON number");
    assert!(inblock["RatVal"].is_number(), "RatVal must be a JSON number");
    assert!(
        inblock["FnoOrdPrc"].is_number(),
        "FnoOrdPrc must be a JSON number"
    );
    // The caller's instrument rides in the body (this read IS keyed on FnoIsuNo).
    assert_eq!(inblock["FnoIsuNo"], "101T6000");
    assert_eq!(inblock["QryTp"], "1");
    assert_eq!(inblock["BnsTpCode"], "1");
    assert_eq!(inblock["FnoOrdprcPtnCode"], "00");

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CFOAQ10100 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`OrdAbleQty` = 주문가능수량, KTD6) holds its exact value. Distinct neighbours
/// carry distinct values so a mislabel cannot be masked.
#[tokio::test]
async fn cfoaq10100_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FUTUREOPTION_ACCNO_PATH))
        .and(header("tr_cd", "CFOAQ10100"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CFOAQ10100_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CFOAQ10100Request::new("1", "1", "0", "0", "101T6000", "1", "0", "00");
    let resp = sdk
        .account()
        .fo_orderable_qty(&req)
        .await
        .expect("CFOAQ10100 orderable-quantity inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);
    assert_eq!(resp.outblock1.fnoisuno, "101T6000");

    assert_eq!(resp.outblock2.len(), 1, "one result row");
    let r = &resp.outblock2[0];
    // Canonical field by Korean name (주문가능수량) — exact value, not !is_empty().
    assert_eq!(r.ordableqty, "38", "OrdAbleQty (주문가능수량)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(r.newordableqty, "36", "NewOrdAbleQty (신규주문가능수량)");
    assert_eq!(r.lqdtordableqty, "2", "LqdtOrdAbleQty (청산주문가능수량)");
    assert_eq!(r.ordableamt, "230782886", "OrdAbleAmt (주문가능금액)");
    assert_eq!(r.mnyordableamt, "230782887", "MnyOrdAbleAmt (현금주문가능금액)");
}

/// Numeric-bearing out-block fields parse via `string_or_number` from BOTH string
/// and number JSON.
#[test]
fn cfoaq10100_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CFOAQ10100OutBlock2": { "OrdAbleQty": 38, "OrdAbleAmt": 42 }
    });
    let resp: CFOAQ10100Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].ordableqty, "38");
    assert_eq!(resp.outblock2[0].ordableamt, "42");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CFOAQ10100OutBlock2": { "OrdAbleQty": "38", "OrdAbleAmt": "42" }
    });
    let resp: CFOAQ10100Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].ordableqty, "38");
    assert_eq!(resp.outblock2[0].ordableamt, "42");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes and is recognized
/// as the empty/pending case (a position-less paper account).
#[test]
fn cfoaq10100_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CFOAQ10100OutBlock2": []
    });
    let resp: CFOAQ10100Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty result Vec (the PENDING case)"
    );
}

/// `CFOAQ10100OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cfoaq10100_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CFOAQ10100OutBlock2": { "OrdAbleQty": 38, "OrdAbleAmt": 230782886 }
    });
    let resp: CFOAQ10100Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].ordableqty, "38");
}

// ---------------------------------------------------------------------------
// CCENQ10100 — KRX야간파생 주문가능수량 조회 (read-only orderable-quantity INQUIRY on
// the night/krx_extended account, NOT an order). Same shape/discipline as
// CFOAQ10100.
// ---------------------------------------------------------------------------

/// `::new` serializes the order-shape inputs under `CCENQ10100InBlock1` with the
/// four numeric fields as JSON numbers (KTD4) and NO account number / caller leak.
#[test]
fn ccenq10100_request_serializes_numeric_fields_and_no_account_leak() {
    let req = CCENQ10100Request::new("1", "1", "0", "0", "101W6000", "1", "0", "00");
    let value = serde_json::to_value(&req).expect("serialize CCENQ10100 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CCENQ10100InBlock1"),
        "missing CCENQ10100InBlock1 key"
    );

    let inblock = &value["CCENQ10100InBlock1"];
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number");
    assert!(inblock["OrdAmt"].is_number(), "OrdAmt must be a JSON number");
    assert!(inblock["RatVal"].is_number(), "RatVal must be a JSON number");
    assert!(
        inblock["FnoOrdPrc"].is_number(),
        "FnoOrdPrc must be a JSON number"
    );
    assert_eq!(inblock["FnoIsuNo"], "101W6000");

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CCENQ10100 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`OrdAbleQty` = 주문가능수량, KTD6) holds its exact value. Distinct neighbours
/// carry distinct values so a mislabel cannot be masked.
#[tokio::test]
async fn ccenq10100_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FUTUREOPTION_ACCNO_PATH))
        .and(header("tr_cd", "CCENQ10100"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CCENQ10100_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CCENQ10100Request::new("1", "1", "0", "0", "101W6000", "1", "0", "00");
    let resp = sdk
        .account()
        .night_orderable_qty(&req)
        .await
        .expect("CCENQ10100 night orderable-quantity inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);
    assert_eq!(resp.outblock1.fnoisuno, "101W6000");

    assert_eq!(resp.outblock2.len(), 1, "one result row");
    let r = &resp.outblock2[0];
    // Canonical field by Korean name (주문가능수량) — exact value, not !is_empty().
    assert_eq!(r.ordableqty, "2", "OrdAbleQty (주문가능수량)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(r.newordableqty, "3", "NewOrdAbleQty (신규주문가능수량)");
    assert_eq!(r.lqdtordableqty, "1", "LqdtOrdAbleQty (청산주문가능수량)");
    assert_eq!(r.ordableamt, "20327175", "OrdAbleAmt (주문가능금액)");
    assert_eq!(r.mnyordableamt, "20327176", "MnyOrdAbleAmt (현금주문가능금액)");
}

/// Numeric-bearing out-block fields parse via `string_or_number` from BOTH string
/// and number JSON.
#[test]
fn ccenq10100_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ10100OutBlock2": { "OrdAbleQty": 2, "OrdAbleAmt": 42 }
    });
    let resp: CCENQ10100Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].ordableqty, "2");
    assert_eq!(resp.outblock2[0].ordableamt, "42");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ10100OutBlock2": { "OrdAbleQty": "2", "OrdAbleAmt": "42" }
    });
    let resp: CCENQ10100Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].ordableqty, "2");
    assert_eq!(resp.outblock2[0].ordableamt, "42");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes and is recognized
/// as the empty/pending case (off the night window or a position-less account).
#[test]
fn ccenq10100_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CCENQ10100OutBlock2": []
    });
    let resp: CCENQ10100Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty result Vec (the PENDING case)"
    );
}

/// `CCENQ10100OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn ccenq10100_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ10100OutBlock2": { "OrdAbleQty": 2, "OrdAbleAmt": 20327175 }
    });
    let resp: CCENQ10100Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].ordableqty, "2");
}

// ---------------------------------------------------------------------------
// t0424 — 주식잔고2 (read-only stock balance: cash summary + per-holding array).
// The wave's U2 holdings gate: the per-holding array length proves whether the
// account carries stock positions (KTD3).
// ---------------------------------------------------------------------------

/// `::new` serializes only the gubun flags under `t0424InBlock` with NO account
/// number; the continuation echo defaults to empty.
#[test]
fn t0424_request_serializes_inblock_only_no_account_leak() {
    let req = T0424Request::new("", "0", "0", "0");
    let value = serde_json::to_value(&req).expect("serialize t0424 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t0424InBlock"), "missing t0424InBlock key");

    let inblock = &value["t0424InBlock"];
    assert_eq!(inblock["chegb"], "0");
    assert_eq!(inblock["cts_expcode"], "", "continuation echo empty on first page");
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        5,
        "InBlock carries only the four gubun flags + continuation (no account number)"
    );

    let serialized = serde_json::to_string(&req).expect("serialize t0424 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes; the substantive cash witness
/// (`sunamt` = 추정순자산) holds a non-default value and the holdings array
/// round-trips one position with distinct field values.
#[tokio::test]
async fn t0424_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T0424_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = T0424Request::new("", "0", "0", "0");
    let resp = sdk
        .account()
        .stock_balance(&req)
        .await
        .expect("t0424 stock-balance inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    // The substantive cash witness is non-default (KTD5).
    assert_eq!(resp.outblock.sunamt, "80030265", "sunamt (추정순자산)");
    assert_eq!(resp.outblock.tappamt, "150283", "tappamt (평가금액)");
    assert_eq!(resp.outblock.tdtsunik, "30271", "tdtsunik distinct from dtsunik");

    assert_eq!(resp.outblock1.len(), 1, "one held position");
    let pos = &resp.outblock1[0];
    assert_eq!(pos.hname, "삼성전자", "issue name");
    assert_eq!(pos.expcode, "005930", "issue code");
    assert_eq!(pos.janqty, "2", "balance quantity (holdings witness)");
    assert_eq!(pos.mdposqt, "1", "sellable quantity distinct from janqty");
    assert_eq!(pos.price, "75300", "current price");
}

/// Numeric-bearing fields parse via `string_or_number` from BOTH string and number
/// JSON (cash-summary `sunamt` + holdings `janqty`).
#[test]
fn t0424_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": 80030265 },
        "t0424OutBlock1": [{ "janqty": 2 }]
    });
    let resp: T0424Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock.sunamt, "80030265");
    assert_eq!(resp.outblock1[0].janqty, "2");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": "80030265" },
        "t0424OutBlock1": [{ "janqty": "2" }]
    });
    let resp: T0424Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock.sunamt, "80030265");
    assert_eq!(resp.outblock1[0].janqty, "2");
}

/// AE2: an empty holdings array on a populated cash summary is the cash-only case —
/// it deserializes, the holdings Vec is empty, but the cash witness is non-default.
#[test]
fn t0424_empty_holdings_is_cash_only_not_a_defect() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": 80030265, "tappamt": 0 },
        "t0424OutBlock1": []
    });
    let resp: T0424Response =
        serde_json::from_value(json).expect("cash-only body must deserialize");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(
        resp.outblock1.is_empty(),
        "empty holdings array = cash-only account (the U2 cash-summary case)"
    );
    assert_eq!(
        resp.outblock.sunamt, "80030265",
        "the cash witness is still non-default — a cash-summary flip, not a positions flip"
    );
}

/// `t0424OutBlock1` arriving as a SINGLE object deserializes via `de_vec_or_single`
/// into a 1-element Vec.
#[test]
fn t0424_out_block1_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": 80030265 },
        "t0424OutBlock1": { "hname": "삼성전자", "janqty": 5 }
    });
    let resp: T0424Response =
        serde_json::from_value(json).expect("single-object holdings must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock1[0].janqty, "5");
}

// ---------------------------------------------------------------------------
// CSPBQ00200 — 현물계좌증거금률별주문가능수량조회 (order-capacity by margin rate).
// Numeric request slots (RecCnt, OrdPrc) serialize as JSON numbers (KTD4); the
// account-identity echo block (AcntNo/InptPwd) is NOT modeled.
// ---------------------------------------------------------------------------

/// `::new` serializes only the in-block under `CSPBQ00200InBlock1` with the numeric
/// slots as JSON NUMBERS (KTD4) and NO account number.
#[test]
fn cspbq00200_request_serializes_numeric_slots_as_numbers_no_account_leak() {
    let req = CSPBQ00200Request::new("1", "KR7005930003", "0", "41");
    let value = serde_json::to_value(&req).expect("serialize CSPBQ00200 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    let inblock = &value["CSPBQ00200InBlock1"];
    // KTD4: numeric request fields ride as JSON numbers, not quoted strings.
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number");
    assert!(inblock["OrdPrc"].is_number(), "OrdPrc must be a JSON number");
    assert_eq!(inblock["IsuNo"], "KR7005930003");
    assert_eq!(inblock["BnsTpCode"], "1");

    let serialized = serde_json::to_string(&req).expect("serialize CSPBQ00200 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes; the substantive capacity witness
/// (`SeOrdAbleAmt` = 거래소주문가능금액) holds a non-default value.
#[tokio::test]
async fn cspbq00200_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .and(header("tr_cd", "CSPBQ00200"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CSPBQ00200_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CSPBQ00200Request::new("1", "KR7005930003", "0", "41");
    let resp = sdk
        .account()
        .order_capacity(&req)
        .await
        .expect("CSPBQ00200 capacity inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00136");
    assert_eq!(resp.outblock2.len(), 1, "one capacity row");
    let cap = &resp.outblock2[0];
    assert_eq!(cap.seordableamt, "265866666", "SeOrdAbleAmt (거래소주문가능금액)");
    assert_eq!(cap.kdqordableamt, "265866000", "KdqOrdAbleAmt distinct");
    assert_eq!(cap.dps, "80000000", "Dps");
    assert_eq!(cap.mnyoutableamt, "79759742", "MnyoutAbleAmt");
}

/// Numeric-bearing out-block fields parse via `string_or_number` from BOTH string
/// and number JSON.
#[test]
fn cspbq00200_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00136",
        "CSPBQ00200OutBlock2": { "SeOrdAbleAmt": 265866666i64, "Dps": 80000000i64 }
    });
    let resp: CSPBQ00200Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].seordableamt, "265866666");
    assert_eq!(resp.outblock2[0].dps, "80000000");

    let as_string = serde_json::json!({
        "rsp_cd": "00136",
        "CSPBQ00200OutBlock2": { "SeOrdAbleAmt": "265866666", "Dps": "80000000" }
    });
    let resp: CSPBQ00200Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].seordableamt, "265866666");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes as the empty case.
#[test]
fn cspbq00200_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({ "rsp_cd": "00707", "CSPBQ00200OutBlock2": [] });
    let resp: CSPBQ00200Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(resp.outblock2.is_empty(), "00707 yields an empty Vec");
}

/// `CSPBQ00200OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cspbq00200_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00136",
        "CSPBQ00200OutBlock2": { "SeOrdAbleAmt": 500000, "Dps": 750000 }
    });
    let resp: CSPBQ00200Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].seordableamt, "500000");
}

// ---------------------------------------------------------------------------
// CLNAQ00100 — 예탁담보융자가능종목현황조회 (loanable-collateral stock list).
// Persistent reference data on /stock/etc; RecCnt is a numeric request slot.
// ---------------------------------------------------------------------------

/// `::full_list` serializes the in-block under `CLNAQ00100InBlock1` with `RecCnt`
/// as a JSON NUMBER (KTD4), empty `IsuNo` (full list), and NO account number.
#[test]
fn clnaq00100_request_serializes_numeric_reccnt_full_list() {
    let req = CLNAQ00100Request::full_list();
    let value = serde_json::to_value(&req).expect("serialize CLNAQ00100 request");
    let inblock = &value["CLNAQ00100InBlock1"];
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number (KTD4)");
    assert_eq!(inblock["QryTp"], "0", "full-list query shape");
    assert_eq!(inblock["IsuNo"], "", "empty IsuNo = full list");
    let serialized = serde_json::to_string(&req).expect("serialize CLNAQ00100 request");
    assert!(!serialized.contains(TEST_ACCOUNT_NO), "no account number in the body");
}

/// A representative non-empty list deserializes; the substantive witnesses
/// (`IsuNm` + `LoanAbleRat`) hold non-default values for distinct stocks.
#[tokio::test]
async fn clnaq00100_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/stock/etc"))
        .and(header("tr_cd", "CLNAQ00100"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CLNAQ00100_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .account()
        .loanable_stocks(&CLNAQ00100Request::full_list())
        .await
        .expect("CLNAQ00100 loanable-stock list should succeed");

    assert_eq!(resp.rsp_cd, "00136");
    assert_eq!(resp.outblock2.len(), 2, "two loanable stocks");
    assert_eq!(resp.outblock2[0].isunm, "삼성전자", "issue name (substantive witness)");
    assert_eq!(resp.outblock2[0].loanablerat, "60.000000000", "loanable rate");
    assert_eq!(resp.outblock2[1].isunm, "삼아알미늄", "distinct second issue");
    assert_eq!(resp.outblock3.reccnt, "2", "summary record count");
}

/// `CLNAQ00100OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn clnaq00100_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00136",
        "CLNAQ00100OutBlock2": { "IsuNm": "삼성전자", "LoanAbleRat": "60.0" }
    });
    let resp: CLNAQ00100Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].isunm, "삼성전자");
}

/// An empty result (`rsp_cd 00707`, empty list) deserializes as the empty case.
#[test]
fn clnaq00100_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({ "rsp_cd": "00707", "CLNAQ00100OutBlock2": [] });
    let resp: CLNAQ00100Response =
        serde_json::from_value(json).expect("empty list must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(resp.outblock2.is_empty(), "00707 yields an empty Vec");
}

// ---------------------------------------------------------------------------
// CFOEQ11100 — 선물옵션가정산예탁금상세 (F/O provisional-settlement deposit detail).
// RecCnt is a numeric request slot; AcntNm is NOT modeled (no PII).
// ---------------------------------------------------------------------------

/// `::new` serializes the in-block with `RecCnt` as a JSON NUMBER (KTD4) and NO
/// account number.
#[test]
fn cfoeq11100_request_serializes_numeric_reccnt_no_account_leak() {
    let req = CFOEQ11100Request::new("20260626");
    let value = serde_json::to_value(&req).expect("serialize CFOEQ11100 request");
    let inblock = &value["CFOEQ11100InBlock1"];
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number (KTD4)");
    assert_eq!(inblock["BnsDt"], "20260626");
    let serialized = serde_json::to_string(&req).expect("serialize CFOEQ11100 request");
    assert!(!serialized.contains(TEST_ACCOUNT_NO), "no account number in the body");
}

/// A representative success body deserializes; the substantive deposit witness
/// (`Dps` = 예수금) holds a non-default value.
#[tokio::test]
async fn cfoeq11100_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FUTUREOPTION_ACCNO_PATH))
        .and(header("tr_cd", "CFOEQ11100"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CFOEQ11100_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .account()
        .fo_deposit_detail(&CFOEQ11100Request::new("20260626"))
        .await
        .expect("CFOEQ11100 deposit detail should succeed");

    assert_eq!(resp.rsp_cd, "00136");
    assert_eq!(resp.outblock2.len(), 1, "one deposit row");
    let d = &resp.outblock2[0];
    assert_eq!(d.dps, "262500611", "Dps (예수금, substantive witness)");
    assert_eq!(d.mnyordableamt, "148316801", "MnyOrdAbleAmt distinct");
    assert_eq!(d.csgnmgn, "114183810", "CsgnMgn distinct");
}

/// Numeric out-block fields parse via `string_or_number` from BOTH string and number.
#[test]
fn cfoeq11100_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00136",
        "CFOEQ11100OutBlock2": { "Dps": 262500611i64 }
    });
    let resp: CFOEQ11100Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].dps, "262500611");

    let as_string = serde_json::json!({
        "rsp_cd": "00136",
        "CFOEQ11100OutBlock2": { "Dps": "262500611" }
    });
    let resp: CFOEQ11100Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].dps, "262500611");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes as the empty case.
#[test]
fn cfoeq11100_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({ "rsp_cd": "00707", "CFOEQ11100OutBlock2": [] });
    let resp: CFOEQ11100Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(resp.outblock2.is_empty(), "00707 yields an empty Vec");
}

/// `CFOEQ11100OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec (the gateway always sends a bare object).
#[test]
fn cfoeq11100_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00136",
        "CFOEQ11100OutBlock2": { "Dps": 262500611, "MnyOrdAbleAmt": 148316801 }
    });
    let resp: CFOEQ11100Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].dps, "262500611");
}

// ---------------------------------------------------------------------------
// t0441 — 선물/옵션잔고평가(이동평균) (F/O balance valuation: position array + summary).
// No numeric request slots; on a position-less account both blocks are empty/zero.
// ---------------------------------------------------------------------------

/// `::new` serializes to exactly `{"t0441InBlock":{...}}` with empty continuation
/// echoes and NO account number.
#[test]
fn t0441_request_serializes_inblock_only_no_account_leak() {
    let req = T0441Request::new();
    let value = serde_json::to_value(&req).expect("serialize t0441 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    let inblock = &value["t0441InBlock"];
    assert_eq!(inblock["cts_expcode"], "", "continuation empty on first page");
    let serialized = serde_json::to_string(&req).expect("serialize t0441 request");
    assert!(!serialized.contains(TEST_ACCOUNT_NO), "no account number in the body");
}

/// A representative non-empty body deserializes; the position array round-trips one
/// row and the valuation summary witness (`tappamt`) holds a non-default value.
#[tokio::test]
async fn t0441_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FUTUREOPTION_ACCNO_PATH))
        .and(header("tr_cd", "t0441"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T0441_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .account()
        .fo_balance_eval(&T0441Request::new())
        .await
        .expect("t0441 balance valuation should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.len(), 1, "one F/O position");
    assert_eq!(resp.outblock1[0].expcode, "101T9000", "issue code");
    assert_eq!(resp.outblock1[0].jqty, "10", "balance quantity (position witness)");
    assert_eq!(resp.outblock.tappamt, "850000000", "tappamt (summary witness)");
    assert_eq!(resp.outblock.tsunik, "-5625000", "tsunik distinct");
}

/// AE2: an empty position array + zero summary deserializes — the position-less
/// account case (the U2 downgrade), not a defect.
#[test]
fn t0441_empty_positions_is_position_less_not_a_defect() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0441OutBlock1": [],
        "t0441OutBlock": { "tappamt": 0, "tsunik": 0 }
    });
    let resp: T0441Response =
        serde_json::from_value(json).expect("position-less body must deserialize");
    assert!(resp.outblock1.is_empty(), "no F/O positions");
    assert_eq!(resp.outblock.tappamt, "0", "zero valuation summary (expected-empty)");
}

/// `t0441OutBlock1` arriving as a SINGLE object deserializes via `de_vec_or_single`.
#[test]
fn t0441_out_block1_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0441OutBlock1": { "expcode": "101T9000", "jqty": 3 }
    });
    let resp: T0441Response =
        serde_json::from_value(json).expect("single-object position must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock1[0].jqty, "3");
}

// ---------------------------------------------------------------------------
// CIDBQ01400 — 해외선물 주문가능수량 (overseas-futures orderable-quantity inquiry).
// RecCnt/OvrsDrvtOrdPrc are numeric request slots; the AcntNo echo block is NOT
// modeled.
// ---------------------------------------------------------------------------

/// `::new` serializes the in-block with the numeric slots as JSON NUMBERS (KTD4)
/// and NO account number.
#[test]
fn cidbq01400_request_serializes_numeric_slots_no_account_leak() {
    // A DECIMAL overseas-futures price must serialize as a JSON number, not a quoted
    // string — the i64-only string_as_number would quote it (→ IGW40011), so this
    // field uses string_as_decimal.
    let req = CIDBQ01400Request::new("1", "ADM23", "2", "75.50", "1");
    let value = serde_json::to_value(&req).expect("serialize CIDBQ01400 request");
    let inblock = &value["CIDBQ01400InBlock1"];
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number (KTD4)");
    assert!(
        inblock["OvrsDrvtOrdPrc"].is_number(),
        "OvrsDrvtOrdPrc must be a JSON number even when fractional (KTD4)"
    );
    assert_eq!(inblock["OvrsDrvtOrdPrc"], 75.5, "decimal price round-trips as a number");
    assert_eq!(inblock["IsuCodeVal"], "ADM23");
    let serialized = serde_json::to_string(&req).expect("serialize CIDBQ01400 request");
    assert!(!serialized.contains(TEST_ACCOUNT_NO), "no account number in the body");
}

/// A representative success body deserializes; the substantive witness
/// (`OrdAbleQty`) holds a non-default value.
#[tokio::test]
async fn cidbq01400_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-futureoption/accno"))
        .and(header("tr_cd", "CIDBQ01400"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CIDBQ01400_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CIDBQ01400Request::new("1", "ADM23", "2", "1", "1");
    let resp = sdk
        .account()
        .overseas_fo_order_qty(&req)
        .await
        .expect("CIDBQ01400 orderable-qty inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00136");
    assert_eq!(resp.outblock2.len(), 1, "one orderable-qty row");
    assert_eq!(resp.outblock2[0].ordableqty, "992", "OrdAbleQty (substantive witness)");
}

/// `OrdAbleQty` parses via `string_or_number` from BOTH string and number JSON.
#[test]
fn cidbq01400_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ01400OutBlock2": { "OrdAbleQty": 992i64 }
    });
    let resp: CIDBQ01400Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].ordableqty, "992");

    let as_string = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ01400OutBlock2": { "OrdAbleQty": "992" }
    });
    let resp: CIDBQ01400Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].ordableqty, "992");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes as the empty case.
#[test]
fn cidbq01400_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({ "rsp_cd": "00707", "CIDBQ01400OutBlock2": [] });
    let resp: CIDBQ01400Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(resp.outblock2.is_empty(), "00707 yields an empty Vec");
}

/// `CIDBQ01400OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cidbq01400_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ01400OutBlock2": { "OrdAbleQty": 992 }
    });
    let resp: CIDBQ01400Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].ordableqty, "992");
}

// ---------------------------------------------------------------------------
// CIDBQ03000 — 해외선물 예수금/잔고현황 (overseas-futures deposit/balance status).
// RecCnt is a numeric request slot; the AcntNo echo block is NOT modeled.
// ---------------------------------------------------------------------------

/// `::new` serializes the in-block with `RecCnt` as a JSON NUMBER (KTD4) and NO
/// account number.
#[test]
fn cidbq03000_request_serializes_numeric_slot_no_account_leak() {
    let req = CIDBQ03000Request::new("1", "20260628");
    let value = serde_json::to_value(&req).expect("serialize CIDBQ03000 request");
    let inblock = &value["CIDBQ03000InBlock1"];
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number (KTD4)");
    assert_eq!(inblock["AcntTpCode"], "1");
    assert_eq!(inblock["TrdDt"], "20260628");
    let serialized = serde_json::to_string(&req).expect("serialize CIDBQ03000 request");
    assert!(!serialized.contains(TEST_ACCOUNT_NO), "no account number in the body");
}

/// A representative success body deserializes; the substantive witness
/// (`EvalAssetAmt`) holds a non-default value.
#[tokio::test]
async fn cidbq03000_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-futureoption/accno"))
        .and(header("tr_cd", "CIDBQ03000"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CIDBQ03000_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CIDBQ03000Request::new("1", "20260628");
    let resp = sdk
        .account()
        .overseas_fo_balance(&req)
        .await
        .expect("CIDBQ03000 deposit/balance inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00136");
    assert_eq!(resp.outblock2.len(), 1, "one per-currency balance row");
    assert_eq!(
        resp.outblock2[0].evalassetamt, "2296849.47",
        "EvalAssetAmt (substantive witness)"
    );
}

/// Money fields parse via `string_or_number` from BOTH string and number JSON.
#[test]
fn cidbq03000_money_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ03000OutBlock2": { "EvalAssetAmt": 2296849i64 }
    });
    let resp: CIDBQ03000Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].evalassetamt, "2296849");

    let as_string = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ03000OutBlock2": { "EvalAssetAmt": "2296849.47" }
    });
    let resp: CIDBQ03000Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].evalassetamt, "2296849.47");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes as the empty case.
#[test]
fn cidbq03000_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({ "rsp_cd": "00707", "CIDBQ03000OutBlock2": [] });
    let resp: CIDBQ03000Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(resp.outblock2.is_empty(), "00707 yields an empty Vec");
}

/// `CIDBQ03000OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cidbq03000_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ03000OutBlock2": { "EvalAssetAmt": "100.00" }
    });
    let resp: CIDBQ03000Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].evalassetamt, "100.00");
}

// ---------------------------------------------------------------------------
// CIDBQ05300 — 해외선물 예탁자산 조회 (overseas-futures deposited-assets inquiry).
// RecCnt is a numeric request slot; the AcntNo echo + summary blocks are NOT modeled.
// ---------------------------------------------------------------------------

/// `::new` serializes the in-block with `RecCnt` as a JSON NUMBER (KTD4), an empty
/// `FcmAcntNo`, and NO account number.
#[test]
fn cidbq05300_request_serializes_numeric_slot_no_account_leak() {
    let req = CIDBQ05300Request::new("1", "ALL");
    let value = serde_json::to_value(&req).expect("serialize CIDBQ05300 request");
    let inblock = &value["CIDBQ05300InBlock1"];
    assert!(inblock["RecCnt"].is_number(), "RecCnt must be a JSON number (KTD4)");
    assert_eq!(inblock["OvrsAcntTpCode"], "1");
    assert_eq!(inblock["CrcyCode"], "ALL");
    assert_eq!(inblock["FcmAcntNo"], "", "FcmAcntNo empty for an individual account");
    let serialized = serde_json::to_string(&req).expect("serialize CIDBQ05300 request");
    assert!(!serialized.contains(TEST_ACCOUNT_NO), "no account number in the body");
}

/// A representative success body deserializes; the per-currency witness
/// (`OvrsFutsDps`) holds a non-default value on the funded currency row.
#[tokio::test]
async fn cidbq05300_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-futureoption/accno"))
        .and(header("tr_cd", "CIDBQ05300"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CIDBQ05300_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CIDBQ05300Request::new("1", "ALL");
    let resp = sdk
        .account()
        .overseas_fo_deposited_assets(&req)
        .await
        .expect("CIDBQ05300 deposited-assets inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00136");
    assert_eq!(resp.outblock2.len(), 2, "two per-currency rows (KRW + USD)");
    assert_eq!(
        resp.outblock2[0].ovrsfutsdps, "3000000000.00",
        "OvrsFutsDps (substantive witness, funded KRW row)"
    );
}

/// Money fields parse via `string_or_number` from BOTH string and number JSON.
#[test]
fn cidbq05300_money_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ05300OutBlock2": { "OvrsFutsDps": 3000000000i64 }
    });
    let resp: CIDBQ05300Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].ovrsfutsdps, "3000000000");

    let as_string = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ05300OutBlock2": { "OvrsFutsDps": "3000000000.00" }
    });
    let resp: CIDBQ05300Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].ovrsfutsdps, "3000000000.00");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes as the empty case.
#[test]
fn cidbq05300_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({ "rsp_cd": "00707", "CIDBQ05300OutBlock2": [] });
    let resp: CIDBQ05300Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(resp.outblock2.is_empty(), "00707 yields an empty Vec");
}

/// `CIDBQ05300OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cidbq05300_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00136",
        "CIDBQ05300OutBlock2": { "OvrsFutsDps": "100.00" }
    });
    let resp: CIDBQ05300Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock2[0].ovrsfutsdps, "100.00");
}
