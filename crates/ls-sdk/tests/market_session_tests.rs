//! Market-session (`t1102`) dependency-class tests.
//!
//! Exercises the `t1102` current-price quote against wiremock through REAL
//! `ls-core` dispatch (the mock config injects `base_url`, so the POST hits the
//! mock server). Covers request body shape (no continuation tokens), serde
//! against the spec-derived fixture, the string-or-number field-semantics
//! regression, and `01900` paper-incompatible classification.

use ls_core::{Inner, LsError};
use ls_sdk::market_session::{
    T1101OutBlock, T1101Request, T1101Response, T1102OutBlock, T1102Request, T1102Response,
    T1531Request, T1531Response, T1537Request, T1537Response, T1601Request, T1601Response,
    T1615Request, T1615Response, T1640Request, T1640Response, T1662Request, T1662Response,
    T1664Request, T1664Response, T1825OutBlock1, T1825Request, T1825Response, T1826OutBlock,
    T1826Request, T1826Response, T1859OutBlock1, T1859Request, T1859Response, T1958Request,
    T1958Response, T1964OutBlock1, T1964Request, T1964Response, T1485Request, T1485Response,
    T1511Request, T1511Response, T1516Request, T1516Response, T8424Request, T8424Response,
    T2301Request, T2301Response, T2522OutBlock1, T2522Request, T2522Response, T8401OutBlock,
    T8401Request, T8401Response, T8426OutBlock, T8426Request, T8426Response, T8425Request,
    T8425Response, T8431OutBlock, T8431Request,
    T8431Response, T8436Request, T8436Response, T9905OutBlock1, T9905Request, T9905Response,
    T9907Request, T9907Response, T9942Request, T9942Response,
};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The spec-derived `t1102` response fixture (`fixtures/t1102_resp.json`).
const T1102_FIXTURE: &str = include_str!("fixtures/t1102_resp.json");

/// The spec-derived `t1101` response fixture (`fixtures/t1101_resp.json`).
const T1101_FIXTURE: &str = include_str!("fixtures/t1101_resp.json");

/// The spec-derived `t8425` all-themes response fixture (`fixtures/t8425_resp.json`).
const T8425_FIXTURE: &str = include_str!("fixtures/t8425_resp.json");

/// `T8425_POLICY.path` — the mounted endpoint for the all-themes read.
const T8425_PATH: &str = "/stock/sector";

/// The spec-derived `t8436` stock-list response fixture (`fixtures/t8436_resp.json`).
const T8436_FIXTURE: &str = include_str!("fixtures/t8436_resp.json");

/// `T8436_POLICY.path` — the mounted endpoint for the stock-master read.
const T8436_PATH: &str = "/stock/etc";

/// The spec-derived `t1531` response fixture (`fixtures/t1531_resp.json`).
const T1531_FIXTURE: &str = include_str!("fixtures/t1531_resp.json");

/// The spec-derived `t1537` response fixture (`fixtures/t1537_resp.json`).
const T1537_FIXTURE: &str = include_str!("fixtures/t1537_resp.json");

/// `T1531_POLICY.path` / `T1537_POLICY.path` — both theme reads share the sector
/// endpoint (distinguished on the wire by the `tr_cd` header), like `t8425`.
const SECTOR_PATH: &str = "/stock/sector";

/// `T1102_POLICY.path` — the mounted endpoint for the quote TR.
const T1102_PATH: &str = "/stock/market-data";

/// `T1101_POLICY.path` — the mounted endpoint for the order-book TR (shared with
/// `t1102`; the `tr_cd` header distinguishes them).
const T1101_PATH: &str = "/stock/market-data";

/// Build an `LsSdk` whose dispatch is pointed at the mock server.
fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

/// Covers R10. The request serializes to exactly `{"t1102InBlock":{...}}` with
/// NO `tr_cont`/`tr_cont_key` keys — `t1102` is not paginated, so the
/// continuation tokens are structurally absent from the body.
#[test]
fn request_serializes_to_inblock_with_no_continuation_fields() {
    let req = T1102Request::new("078020", "K");
    let value = serde_json::to_value(&req).expect("serialize t1102 request");

    // Exactly one top-level key: t1102InBlock.
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1102InBlock"), "missing t1102InBlock key");

    // No continuation tokens anywhere in the serialized body.
    assert!(
        value.get("tr_cont").is_none(),
        "tr_cont must not be in the body"
    );
    assert!(
        value.get("tr_cont_key").is_none(),
        "tr_cont_key must not be in the body"
    );

    let inblock = &value["t1102InBlock"];
    assert_eq!(inblock["shcode"], "078020");
    assert_eq!(inblock["exchgubun"], "K");
    assert!(
        inblock.get("tr_cont").is_none(),
        "tr_cont must not be in the inblock"
    );
    assert!(
        inblock.get("tr_cont_key").is_none(),
        "tr_cont_key must not be in the inblock"
    );
}

/// Happy path: the spec-derived fixture deserializes with the key quote fields
/// asserted. Grounded in `specs/ls_openapi_specs.json` → `t1102OutBlock`:
/// `price`/`volume` arrive as JSON numbers, `sign` as a JSON string.
#[tokio::test]
async fn quote_deserializes_spec_fixture_with_key_quote_fields() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1102_PATH))
        .and(header("tr_cd", "t1102"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1102_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = T1102Request::new("078020", "K");
    let resp = sdk
        .market_session()
        .quote(&req)
        .await
        .expect("t1102 quote should succeed");

    // Key quote fields, coerced to String regardless of wire type.
    assert_eq!(resp.outblock.price, "4535", "price (was JSON number)");
    assert_eq!(resp.outblock.volume, "6929", "volume (was JSON number)");
    assert_eq!(resp.outblock.sign, "2", "sign (was JSON string)");
    assert_eq!(resp.outblock.hname, "LS증권");
    assert_eq!(resp.rsp_cd, "00000");
}

/// Edge: a numeric field arriving as a JSON number (not string) still
/// deserializes. This is the field-semantics regression that
/// `ls_core::string_or_number` guarantees — proven directly against the
/// `T1102OutBlock` deserializer with `price`/`volume` as bare numbers and `sign`
/// as a string, exactly as the spec example sends them.
#[test]
fn numeric_field_as_json_number_deserializes() {
    let json = serde_json::json!({
        "hname": "LS증권",
        "price": 4535,
        "sign": "2",
        "volume": 6929
    });
    let out: T1102OutBlock = serde_json::from_value(json).expect("number fields must deserialize");
    assert_eq!(out.price, "4535");
    assert_eq!(out.volume, "6929");
    assert_eq!(out.sign, "2");

    // And the string form yields the identical value (the round-trip guarantee).
    let json_str = serde_json::json!({
        "price": "4535",
        "volume": "6929",
        "sign": "2"
    });
    let out_str: T1102OutBlock =
        serde_json::from_value(json_str).expect("string fields must deserialize");
    assert_eq!(out_str.price, out.price);
    assert_eq!(out_str.volume, out.volume);
}

/// Error: a `01900` response classifies as paper-incompatible. The mounted body
/// carries `rsp_cd: "01900"`; dispatch preserves the exact code and the runtime
/// helper classifies it specifically as paper-incompatible (not a generic
/// failure).
#[tokio::test]
async fn code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1102_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01900",
            "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = T1102Request::new("078020", "K");
    let err = sdk
        .market_session()
        .quote(&req)
        .await
        .expect_err("01900 must surface as an error");

    match &err {
        LsError::ApiError { code, .. } => {
            assert_eq!(code, "01900", "exact code preserved, not collapsed");
            assert!(
                ls_core::is_paper_incompatible(code),
                "01900 must classify as paper-incompatible"
            );
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
    assert!(
        err.is_paper_incompatible(),
        "LsError::is_paper_incompatible() must be true for 01900"
    );
}

/// Compile-time guard: `T1102Response` is constructible with its public fields,
/// keeping the envelope shape stable for downstream callers.
#[test]
fn response_envelope_default_is_empty() {
    let resp = T1102Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert_eq!(resp.outblock.price, "");
}

// ---------------------------------------------------------------------------
// t1101 — current-price + order-book (호가) quote. Second TR in the
// market_session class; same dispatch shape as t1102 (single non-paginated
// POST), distinguished on the wire by the `tr_cd` header.
// ---------------------------------------------------------------------------

/// Covers R6. The `t1101` request serializes to exactly `{"t1101InBlock":{...}}`
/// — `shcode` only (no `exchgubun`, unlike `t1102`), and no `tr_cont`/
/// `tr_cont_key` since `t1101` is not paginated.
#[test]
fn t1101_request_serializes_to_inblock_with_only_shcode() {
    let req = T1101Request::new("078020");
    let value = serde_json::to_value(&req).expect("serialize t1101 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1101InBlock"), "missing t1101InBlock key");

    let inblock = &value["t1101InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t1101InBlock carries only shcode");
    assert_eq!(inblock["shcode"], "078020");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Happy path: the spec-derived fixture deserializes with the price header and
/// the level-1 order book asserted. The fixture mixes wire types — `price`/
/// `offerho1` as JSON numbers, `sign` and `offerrem1` as JSON strings — so this
/// exercises `string_or_number` across the order-book fields.
#[tokio::test]
async fn t1101_order_book_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1101_PATH))
        .and(header("tr_cd", "t1101"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1101_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = T1101Request::new("078020");
    let resp = sdk
        .market_session()
        .order_book(&req)
        .await
        .expect("t1101 order_book should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.price, "4535", "price (was JSON number)");
    assert_eq!(resp.outblock.sign, "2", "sign (was JSON string)");
    assert_eq!(resp.outblock.offerho1, "4540", "offerho1 (was JSON number)");
    assert_eq!(resp.outblock.bidho1, "4535");
    assert_eq!(
        resp.outblock.offerrem1, "1200",
        "offerrem1 (was JSON string)"
    );
    assert_eq!(resp.outblock.bidho10, "4490", "deepest bid level parsed");
}

/// Edge: order-book numeric fields deserialize whether they arrive as JSON
/// numbers or strings, and a sparse out-block (missing levels) defaults cleanly.
#[test]
fn t1101_order_book_fields_number_or_string_and_sparse_default() {
    let as_numbers = serde_json::json!({
        "price": 4535,
        "offerho1": 4540,
        "bidho1": 4535,
        "offerrem1": 1200
    });
    let out: T1101OutBlock =
        serde_json::from_value(as_numbers).expect("number fields must deserialize");
    assert_eq!(out.price, "4535");
    assert_eq!(out.offerho1, "4540");
    assert_eq!(out.offerrem1, "1200");

    let as_strings = serde_json::json!({
        "price": "4535",
        "offerho1": "4540",
        "bidho1": "4535",
        "offerrem1": "1200"
    });
    let out_str: T1101OutBlock =
        serde_json::from_value(as_strings).expect("string fields must deserialize");
    assert_eq!(out_str.offerho1, out.offerho1);
    assert_eq!(out_str.offerrem1, out.offerrem1);

    // Sparse: an empty out-block defaults every field to "" without error.
    let sparse: T1101OutBlock =
        serde_json::from_value(serde_json::json!({})).expect("empty out-block must default");
    assert_eq!(sparse.price, "");
    assert_eq!(sparse.bidho10, "");
}

/// Error: a `01900` response from the order-book TR classifies as
/// paper-incompatible — the AE2 fallback path. The exact code is preserved and
/// the runtime helper classifies it specifically (not a generic failure).
#[tokio::test]
async fn t1101_code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1101_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01900",
            "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = T1101Request::new("078020");
    let err = sdk
        .market_session()
        .order_book(&req)
        .await
        .expect_err("01900 must surface as an error");

    match &err {
        LsError::ApiError { code, .. } => {
            assert_eq!(code, "01900", "exact code preserved, not collapsed");
            assert!(
                ls_core::is_paper_incompatible(code),
                "01900 must classify as paper-incompatible"
            );
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
    assert!(err.is_paper_incompatible());
}

/// Compile-time guard: `T1101Response` default envelope is empty.
#[test]
fn t1101_response_envelope_default_is_empty() {
    let resp = T1101Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert_eq!(resp.outblock.offerho1, "");
}

// ---------------------------------------------------------------------------
// t8425 — 전체테마 (all-themes) read. Third TR in the market_session class and
// the implement-tr pilot: non-paginated, NO caller input, an array out-block.
// ---------------------------------------------------------------------------

/// Covers R5. The `t8425` request serializes to exactly `{"t8425InBlock":{...}}`
/// with only the `dummy` placeholder — no caller-supplied fields leak, and no
/// `tr_cont`/`tr_cont_key` (t8425 is not paginated).
#[test]
fn t8425_request_serializes_to_inblock_with_only_dummy() {
    let req = T8425Request::new();
    let value = serde_json::to_value(&req).expect("serialize t8425 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t8425InBlock"), "missing t8425InBlock key");

    let inblock = &value["t8425InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t8425InBlock carries only the dummy field");
    assert_eq!(inblock["dummy"], "", "dummy is an empty placeholder (no caller input)");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers R2, R5. The spec-derived fixture deserializes through REAL dispatch:
/// the all-themes array round-trips, a real (non-default) `tmname`/`tmcode` is
/// populated, and `tmcode` arriving as a JSON number (`1234`) still parses via
/// `string_or_number` — proving the representative subset round-trips, not just
/// that `serde(default)` returned `Ok`.
#[tokio::test]
async fn all_themes_deserializes_spec_fixture_with_real_values() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T8425_PATH))
        .and(header("tr_cd", "t8425"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8425_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .all_themes(&T8425Request::new())
        .await
        .expect("t8425 all_themes should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 3, "all three theme rows round-trip");
    assert_eq!(resp.outblock[0].tmname, "2차전지", "real non-default tmname");
    assert_eq!(resp.outblock[0].tmcode, "0050", "tmcode (was JSON string)");
    assert_eq!(
        resp.outblock[1].tmcode, "1234",
        "tmcode coerced from a JSON number"
    );
}

/// Covers R2, R5. `tmcode` deserializes whether it arrives as a JSON string or a
/// JSON number — the `string_or_number` round-trip guarantee, proven directly.
#[test]
fn t8425_tmcode_number_or_string_yields_same_value() {
    let as_number: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8425OutBlock": [{ "tmname": "반도체", "tmcode": 1234 }]
    }))
    .expect("number tmcode must deserialize");
    let as_string: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8425OutBlock": [{ "tmname": "반도체", "tmcode": "1234" }]
    }))
    .expect("string tmcode must deserialize");
    assert_eq!(as_number.outblock[0].tmcode, "1234");
    assert_eq!(as_number.outblock[0].tmcode, as_string.outblock[0].tmcode);
}

/// Covers R2. A single out-block object (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single` — the gateway collapses a one-row
/// result to a bare object.
#[test]
fn t8425_single_out_row_tolerated_as_array() {
    let single: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8425OutBlock": { "tmname": "단일", "tmcode": "0001" }
    }))
    .expect("single out-block object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].tmcode, "0001");
}

/// Covers R2. An empty result set (`rsp_cd 00707`, empty out-block array)
/// deserializes without error and is recognized as the empty/pending case — the
/// implement-tr gate records this as PENDING (callable but shape-unconfirmed),
/// never a flip.
#[test]
fn t8425_empty_result_set_deserializes_as_empty() {
    let empty: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t8425OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.is_empty(),
        "an empty out-block is the pending case, not a flip"
    );
}

/// Compile-time guard: `T8425Response` default envelope is empty.
#[test]
fn t8425_response_envelope_default_is_empty() {
    let resp = T8425Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t8436 — 주식종목조회 (stock master list). market_session, non-paginated, takes
// a `gubun` market-segment filter; array out-block.
// ---------------------------------------------------------------------------

/// Covers R5. The `t8436` request serializes to exactly `{"t8436InBlock":{...}}`
/// with only the `gubun` filter — no continuation fields.
#[test]
fn t8436_request_serializes_to_inblock_with_only_gubun() {
    let req = T8436Request::new("0");
    let value = serde_json::to_value(&req).expect("serialize t8436 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    let inblock = &value["t8436InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t8436InBlock carries only gubun");
    assert_eq!(inblock["gubun"], "0");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5. The spec-derived fixture deserializes through REAL dispatch:
/// the stock-master array round-trips with real `hname`/`shcode` values, and
/// numeric fields arriving as JSON numbers (row 0) or strings (row 1) both parse
/// via `string_or_number`.
#[tokio::test]
async fn stock_list_deserializes_spec_fixture_with_real_values() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T8436_PATH))
        .and(header("tr_cd", "t8436"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8436_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .stock_list(&T8436Request::new("0"))
        .await
        .expect("t8436 stock_list should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "both stock rows round-trip");
    assert_eq!(resp.outblock[0].hname, "삼성전자", "real non-default hname");
    assert_eq!(resp.outblock[0].shcode, "005930");
    assert_eq!(
        resp.outblock[0].uplmtprice, "92900",
        "uplmtprice coerced from a JSON number"
    );
    assert_eq!(
        resp.outblock[1].uplmtprice, "300000",
        "uplmtprice parsed from a JSON string"
    );
}

/// Covers R2. An empty result set (`00707`, empty array) deserializes and is
/// recognized as the empty/pending case.
#[test]
fn t8436_empty_result_set_deserializes_as_empty() {
    let empty: T8436Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t8436OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty());
}

/// Covers R2. A single out-block object (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single` (the gateway collapses a one-row
/// result to a bare object).
#[test]
fn t8436_single_out_row_tolerated_as_array() {
    let single: T8436Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8436OutBlock": { "hname": "단일", "shcode": "000660" }
    }))
    .expect("single out-block object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].shcode, "000660");
}

/// Compile-time guard: `T8436Response` default envelope is empty.
#[test]
fn t8436_response_envelope_default_is_empty() {
    let resp = T8436Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t1531 — 테마별종목 (stocks in a theme). market_session, non-paginated; keyed by
// a required tmname+tmcode pair (AE4 caller-supplied identifiers).
// ---------------------------------------------------------------------------

/// Covers R5, AE4. The `t1531` request serializes to `{"t1531InBlock":{...}}`
/// carrying BOTH required identifiers `tmname` and `tmcode` in the correct block.
#[test]
fn t1531_request_serializes_with_tmname_and_tmcode() {
    let req = T1531Request::new("2차전지", "0050");
    let value = serde_json::to_value(&req).expect("serialize t1531 request");

    let inblock = &value["t1531InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 2, "tmname + tmcode");
    assert_eq!(inblock["tmname"], "2차전지");
    assert_eq!(inblock["tmcode"], "0050");
    assert!(value.get("tr_cont").is_none());
}

/// Covers R2. The fixture deserializes through REAL dispatch; rows round-trip and
/// `tmcode`/`avgdiff` parse whether they arrive as JSON strings (row 0) or
/// numbers (row 1).
#[tokio::test]
async fn theme_stocks_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(SECTOR_PATH))
        .and(header("tr_cd", "t1531"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1531_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .theme_stocks(&T1531Request::new("2차전지", "0050"))
        .await
        .expect("t1531 theme_stocks should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].tmcode, "0050", "tmcode (string form)");
    assert_eq!(resp.outblock[1].tmcode, "50", "tmcode coerced from number");
    assert_eq!(resp.outblock[0].avgdiff, "1.23");
}

/// Covers R2. An empty result set (`00707`) deserializes as the pending case.
#[test]
fn t1531_empty_result_set_deserializes_as_empty() {
    let empty: T1531Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1531OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty());
}

/// Covers R2. A single `t1531` out-block object is tolerated as a one-element Vec
/// via `de_vec_or_single`.
#[test]
fn t1531_single_out_row_tolerated_as_array() {
    let single: T1531Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1531OutBlock": { "tmname": "단일", "tmcode": "0001" }
    }))
    .expect("single out-block object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].tmcode, "0001");
}

// ---------------------------------------------------------------------------
// t1537 — 테마종목별시세조회 (per-stock quotes for a theme). market_session,
// non-paginated; keyed by tmcode. Summary out-block + per-stock row array.
// ---------------------------------------------------------------------------

/// Covers R5. The `t1537` request serializes to `{"t1537InBlock":{"tmcode":...}}`.
#[test]
fn t1537_request_serializes_with_only_tmcode() {
    let req = T1537Request::new("0050");
    let value = serde_json::to_value(&req).expect("serialize t1537 request");
    let inblock = &value["t1537InBlock"];
    assert_eq!(inblock.as_object().expect("object").len(), 1);
    assert_eq!(inblock["tmcode"], "0050");
}

/// Covers R2. The fixture deserializes through REAL dispatch: the summary block
/// and the per-stock row array both round-trip, with mixed number/string wire
/// types parsed via `string_or_number`.
#[tokio::test]
async fn theme_quotes_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(SECTOR_PATH))
        .and(header("tr_cd", "t1537"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1537_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .theme_quotes(&T1537Request::new("0050"))
        .await
        .expect("t1537 theme_quotes should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.tmcnt, "20", "summary tmcnt (from number)");
    assert_eq!(resp.outblock.tmname, "2차전지");
    assert_eq!(resp.outblock1.len(), 2, "both per-stock rows round-trip");
    assert_eq!(resp.outblock1[0].shcode, "247540");
    assert_eq!(resp.outblock1[0].price, "231000", "price (from number)");
    assert_eq!(resp.outblock1[1].price, "150000", "price (from string)");
}

/// Covers R2. A single per-stock row (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single`.
#[test]
fn t1537_single_out_row_tolerated_as_array() {
    let single: T1537Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1537OutBlock": { "tmname": "단일", "tmcnt": 1 },
        "t1537OutBlock1": { "hname": "종목", "shcode": "000660", "price": 100 }
    }))
    .expect("single out-row object must deserialize as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "000660");
}

/// Compile-time guard: `T1537Response` default envelope is empty.
#[test]
fn t1537_response_envelope_default_is_empty() {
    let resp = T1537Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock1.is_empty());
    assert_eq!(resp.outblock.tmname, "");
}

// ---------------------------------------------------------------------------
// t1859 — 서버저장조건 조건검색 (server-saved condition search). market_session,
// non-paginated; the saved-condition spine CONSUMER. Keyed by a `query_index`
// self-sourced from t1866 (the modeled cross-TR discovery edge — never
// fabricated). Summary out-block + matched-issue row array.
// ---------------------------------------------------------------------------

/// Covers R5, R8. The `t1859` request serializes to exactly
/// `{"t1859InBlock":{"query_index":...}}` — the `query_index` rides in the
/// in-block under the correct key, and no `tr_cont`/`tr_cont_key` leak (t1859 is
/// not paginated).
#[test]
fn t1859_request_serializes_with_query_index_in_inblock() {
    let req = T1859Request::new("000000000123");
    let value = serde_json::to_value(&req).expect("serialize t1859 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1859InBlock"), "missing t1859InBlock key");

    let inblock = &value["t1859InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t1859InBlock carries only query_index");
    assert_eq!(inblock["query_index"], "000000000123");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers R5. A representative success response deserializes through the typed
/// path: the summary `result_count` (a modeled non-key field) holds a real
/// non-default value, the matched-issue array round-trips, and numeric fields
/// parse whether they arrive as JSON numbers (row 0) or strings (row 1) via
/// `string_or_number` — proving the subset round-trips, not just `serde(default)`.
#[test]
fn t1859_deserializes_success_with_real_values() {
    let resp: T1859Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1859OutBlock": { "result_count": 2, "result_time": "153000", "text": "전략" },
        "t1859OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "price": 71000, "sign": "2",
              "change": 500, "diff": 0.71, "volume": 1000000 },
            { "shcode": "000660", "hname": "SK하이닉스", "price": "150000", "sign": "5",
              "change": "-1000", "diff": "-0.66", "volume": "500000" }
        ]
    }))
    .expect("representative t1859 success must deserialize");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.result_count, "2", "non-key summary field populated");
    assert_eq!(resp.outblock1.len(), 2, "both matched-issue rows round-trip");
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[0].price, "71000", "price (from JSON number)");
    assert_eq!(resp.outblock1[1].price, "150000", "price (from JSON string)");
}

/// Covers R5. An empty result set (`rsp_cd 00707`, empty out-block) deserializes
/// and is recognized as the empty/pending case — the implement-tr gate records
/// this as PENDING, never a flip.
#[test]
fn t1859_empty_result_set_deserializes_as_empty() {
    let empty: T1859Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1859OutBlock": { "result_count": 0 },
        "t1859OutBlock1": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock1.is_empty(),
        "an empty matched-issue array is the pending case, not a flip"
    );
}

/// Covers R5. A single matched-issue row (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single` (the gateway collapses a one-row result
/// to a bare object).
#[test]
fn t1859_single_out_row_tolerated_as_array() {
    let single: T1859Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1859OutBlock": { "result_count": 1 },
        "t1859OutBlock1": { "shcode": "005930", "hname": "삼성전자", "price": 71000 }
    }))
    .expect("single out-row object must deserialize as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "005930");
}

/// Covers R5. The matched-issue row fields parse whether `price` arrives as a
/// JSON number or string — the `string_or_number` round-trip guarantee proven
/// directly against `T1859OutBlock1`.
#[test]
fn t1859_row_price_number_or_string_yields_same_value() {
    let as_number: T1859OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "price": 71000
    }))
    .expect("number price must deserialize");
    let as_string: T1859OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "price": "71000"
    }))
    .expect("string price must deserialize");
    assert_eq!(as_number.price, "71000");
    assert_eq!(as_number.price, as_string.price);
}

/// Compile-time guard: `T1859Response` default envelope is empty.
#[test]
fn t1859_response_envelope_default_is_empty() {
    let resp = T1859Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock1.is_empty());
    assert_eq!(resp.outblock.result_count, "");
}

// ---------------------------------------------------------------------------
// t1826 — 종목Q클릭검색리스트조회 (ThinQ Q-click search-list; Wave 3 producer).
// market_session, non-paginated; takes a `search_gb` catalog filter and returns
// the `search_cd` keys consumed by `t1825`.
// ---------------------------------------------------------------------------

/// Covers AE2. `T1826Request::new` serializes the `search_gb` filter under the
/// `t1826InBlock` key, with no `tr_cont`/`tr_cont_key` leak (t1826 is not
/// paginated).
#[test]
fn t1826_request_serializes_with_search_gb_in_inblock() {
    let req = T1826Request::new("0");
    let value = serde_json::to_value(&req).expect("serialize t1826 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1826InBlock"), "missing t1826InBlock key");

    let inblock = &value["t1826InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t1826InBlock carries only search_gb");
    assert_eq!(inblock["search_gb"], "0");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers AE2. A representative success response deserializes through the typed
/// path: the `search_cd` catalog keys round-trip (the `t1825` discovery-edge
/// input), and `search_cd` parses whether it arrives as a JSON number (row 0) or
/// string (row 1) via `string_or_number` — proving the subset round-trips, not
/// just `serde(default)`.
#[test]
fn t1826_deserializes_success_with_real_values() {
    let resp: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1826OutBlock": [
            { "search_cd": "0001", "search_nm": "거래량급증" },
            { "search_cd": 2, "search_nm": "외국인순매수" }
        ]
    }))
    .expect("representative t1826 success must deserialize");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "both available-search rows round-trip");
    assert_eq!(resp.outblock[0].search_cd, "0001", "search_cd (from string)");
    assert_eq!(resp.outblock[1].search_cd, "2", "search_cd (from JSON number)");
}

/// Covers AE2. An empty result set (`rsp_cd 00707`, empty out-block) deserializes
/// and is recognized as the empty/pending case — the implement-tr gate records
/// this as PENDING, never a flip.
#[test]
fn t1826_empty_result_set_deserializes_as_empty() {
    let empty: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1826OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.is_empty(),
        "an empty search-list is the pending case, not a flip"
    );
}

/// Covers AE2. A single available-search row (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single`.
#[test]
fn t1826_single_out_row_tolerated_as_array() {
    let single: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1826OutBlock": { "search_cd": "0001", "search_nm": "거래량급증" }
    }))
    .expect("single out-row object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].search_cd, "0001");
}

/// Covers AE2. `search_cd` parses whether it arrives as a JSON number or string —
/// the `string_or_number` round-trip guarantee proven directly against
/// `T1826OutBlock`.
#[test]
fn t1826_search_cd_number_or_string_yields_same_value() {
    let as_number: T1826OutBlock = serde_json::from_value(serde_json::json!({
        "search_cd": 1
    }))
    .expect("number search_cd must deserialize");
    let as_string: T1826OutBlock = serde_json::from_value(serde_json::json!({
        "search_cd": "1"
    }))
    .expect("string search_cd must deserialize");
    assert_eq!(as_number.search_cd, "1");
    assert_eq!(as_number.search_cd, as_string.search_cd);
}

/// Compile-time guard: `T1826Response` default envelope is empty.
#[test]
fn t1826_response_envelope_default_is_empty() {
    let resp = T1826Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t1825 — 종목Q클릭검색 (ThinQ Q-click search; Wave 3 consumer). market_session,
// non-paginated; keyed by a `search_cd` self-sourced from t1826 (the discovery
// edge), plus a `gubun` market filter.
// ---------------------------------------------------------------------------

/// Covers AE2. `T1825Request::new` serializes both `search_cd` and `gubun` under
/// the `t1825InBlock` key, with no `tr_cont`/`tr_cont_key` leak (t1825 is not
/// paginated).
#[test]
fn t1825_request_serializes_with_search_cd_and_gubun_in_inblock() {
    let req = T1825Request::new("0001", "0");
    let value = serde_json::to_value(&req).expect("serialize t1825 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1825InBlock"), "missing t1825InBlock key");

    let inblock = &value["t1825InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(
        inblock_obj.len(),
        2,
        "t1825InBlock carries only search_cd and gubun"
    );
    assert_eq!(inblock["search_cd"], "0001");
    assert_eq!(inblock["gubun"], "0");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers AE2. A representative success response deserializes through the typed
/// path: the summary `jong_cnt` (a modeled non-key field) holds a real
/// non-default value, the matched-issue array round-trips, and numeric fields
/// parse whether they arrive as JSON numbers (row 0) or strings (row 1) via
/// `string_or_number` — proving the subset round-trips, not just `serde(default)`.
#[test]
fn t1825_deserializes_success_with_real_values() {
    let resp: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1825OutBlock": { "JongCnt": 2 },
        "t1825OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "close": 71000, "change": 500,
              "diff": 0.71, "volume": 1000000 },
            { "shcode": "000660", "hname": "SK하이닉스", "close": "150000", "change": "-1000",
              "diff": "-0.66", "volume": "500000" }
        ]
    }))
    .expect("representative t1825 success must deserialize");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.jong_cnt, "2", "non-key summary field populated");
    assert_eq!(resp.outblock1.len(), 2, "both matched-issue rows round-trip");
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[0].close, "71000", "close (from JSON number)");
    assert_eq!(resp.outblock1[1].close, "150000", "close (from JSON string)");
}

/// Covers AE2. An empty result set (`rsp_cd 00707`, empty out-block) deserializes
/// and is recognized as the empty/pending case.
#[test]
fn t1825_empty_result_set_deserializes_as_empty() {
    let empty: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1825OutBlock": { "JongCnt": 0 },
        "t1825OutBlock1": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock1.is_empty(),
        "an empty matched-issue array is the pending case, not a flip"
    );
}

/// Covers AE2. A single matched-issue row (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single`.
#[test]
fn t1825_single_out_row_tolerated_as_array() {
    let single: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1825OutBlock": { "JongCnt": 1 },
        "t1825OutBlock1": { "shcode": "005930", "hname": "삼성전자", "close": 71000 }
    }))
    .expect("single out-row object must deserialize as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "005930");
}

/// Covers AE2. The matched-issue row fields parse whether `close` arrives as a
/// JSON number or string — proven directly against `T1825OutBlock1`.
#[test]
fn t1825_row_close_number_or_string_yields_same_value() {
    let as_number: T1825OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "close": 71000
    }))
    .expect("number close must deserialize");
    let as_string: T1825OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "close": "71000"
    }))
    .expect("string close must deserialize");
    assert_eq!(as_number.close, "71000");
    assert_eq!(as_number.close, as_string.close);
}

/// Compile-time guard: `T1825Response` default envelope is empty.
#[test]
fn t1825_response_envelope_default_is_empty() {
    let resp = T1825Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock1.is_empty());
    assert_eq!(resp.outblock.jong_cnt, "");
}

/// Covers AE2 / KTD-3 contingency. The OFFLINE captured-chain fixture: validates
/// the `t1826 → t1825` chained-smoke harness *logic* independently of live data.
/// A recorded `t1826` body deserializes, its first `search_cd` is extracted, that
/// value builds a `t1825` request (proving the self-source wiring), and a recorded
/// `t1825` body deserializes — so harness correctness does not depend on the paper
/// account having seeded data (decouples "is the chain code correct" from "does
/// this account have data").
#[test]
fn t1825_chained_off_t1826_offline_fixture() {
    // Stage 1: a recorded t1826 search-list body deserializes.
    let producer: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1826OutBlock": [
            { "search_cd": "0001", "search_nm": "거래량급증" },
            { "search_cd": "0002", "search_nm": "외국인순매수" }
        ]
    }))
    .expect("recorded t1826 producer body must deserialize");
    assert!(
        !producer.outblock.is_empty(),
        "non-empty producer is the precondition for chaining"
    );

    // Stage 2: self-source the search_cd from the producer (never fabricated) and
    // build the consumer request — the exact wiring live_smoke_t1825 performs.
    let search_cd = producer.outblock[0].search_cd.clone();
    let req = T1825Request::new(&search_cd, "0");
    let value = serde_json::to_value(&req).expect("serialize chained t1825 request");
    assert_eq!(
        value["t1825InBlock"]["search_cd"], "0001",
        "the consumer request carries the self-sourced search_cd"
    );

    // Stage 3: a recorded t1825 body for that search deserializes.
    let consumer: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1825OutBlock": { "JongCnt": 1 },
        "t1825OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "close": 71000, "change": 500,
              "diff": 0.71, "volume": 1000000 }
        ]
    }))
    .expect("recorded t1825 consumer body must deserialize");
    assert_eq!(consumer.outblock1.len(), 1, "the chained consumer body round-trips");
    assert_eq!(consumer.outblock1[0].shcode, "005930");
}

// ---------------------------------------------------------------------------
// Wave 1 — ELW universe/list reads (t9905, t9907, t8431, t9942). No-caller-input
// `dummy` reads; each returns a code-keyed list. Covers AE1.
// ---------------------------------------------------------------------------

/// Covers AE1. `t9905` request serializes only `dummy`; a representative success
/// deserializes with the underlying-asset `shcode` (the `t1964` `item` source)
/// populated, single-or-array tolerated.
#[test]
fn t9905_request_and_response_round_trip() {
    let value = serde_json::to_value(T9905Request::new()).expect("serialize t9905");
    assert_eq!(value["t9905InBlock"]["dummy"], "");

    let resp: T9905Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9905OutBlock1": [
            { "shcode": "005930", "expcode": "KR7005930003", "hname": "삼성전자" },
            { "shcode": 660, "expcode": "KR7000660001", "hname": "SK하이닉스" }
        ]
    }))
    .expect("representative t9905 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "005930", "underlying code populated");
    assert_eq!(resp.outblock1[1].shcode, "660", "shcode from JSON number");

    let single: T9905Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9905OutBlock1": { "shcode": "005930", "hname": "삼성전자" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T9905Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9905OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

/// Covers AE1. `T9905OutBlock1.shcode` parses from JSON number or string alike.
#[test]
fn t9905_shcode_number_or_string_yields_same_value() {
    let n: T9905OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": 5930 })).expect("number");
    let s: T9905OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": "5930" })).expect("string");
    assert_eq!(n.shcode, "5930");
    assert_eq!(n.shcode, s.shcode);
}

/// Covers AE1. `t9907` expiry-month list round-trips; empty is the pending case.
#[test]
fn t9907_request_and_response_round_trip() {
    let value = serde_json::to_value(T9907Request::new()).expect("serialize t9907");
    assert_eq!(value["t9907InBlock"]["dummy"], "");

    let resp: T9907Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9907OutBlock1": [
            { "lastym": "202606", "lastnm": "2026년 06월" },
            { "lastym": 202609, "lastnm": "2026년 09월" }
        ]
    }))
    .expect("representative t9907 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].lastym, "202606");
    assert_eq!(resp.outblock1[1].lastym, "202609", "lastym from JSON number");

    let empty: T9907Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9907OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty());
}

/// Covers AE1. `t8431` ELW-symbol list round-trips with the `shcode` (the `t1958`
/// pair source) populated; the numeric `recprice` parses number-or-string;
/// single-or-array tolerated; empty is the pending case.
#[test]
fn t8431_request_and_response_round_trip() {
    let value = serde_json::to_value(T8431Request::new()).expect("serialize t8431");
    assert_eq!(value["t8431InBlock"]["dummy"], "");

    let resp: T8431Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8431OutBlock": [
            { "hname": "삼성전자콜ELW", "shcode": "57J123", "expcode": "KR4500001234",
              "recprice": 105 },
            { "hname": "SK하이닉스풋ELW", "shcode": "57J456", "expcode": "KR4500005678",
              "recprice": "210" }
        ]
    }))
    .expect("representative t8431 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].shcode, "57J123", "ELW code populated");
    assert_eq!(resp.outblock[0].recprice, "105", "recprice from JSON number");
    assert_eq!(resp.outblock[1].recprice, "210", "recprice from JSON string");

    let single: T8431Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8431OutBlock": { "shcode": "57J123", "hname": "삼성전자콜ELW" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock.len(), 1);

    let empty: T8431Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8431OutBlock": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock.is_empty(), "empty is the pending case");
}

/// Covers AE1. `T8431OutBlock.recprice` parses number-or-string alike.
#[test]
fn t8431_recprice_number_or_string_yields_same_value() {
    let n: T8431OutBlock =
        serde_json::from_value(serde_json::json!({ "recprice": 105 })).expect("number");
    let s: T8431OutBlock =
        serde_json::from_value(serde_json::json!({ "recprice": "105" })).expect("string");
    assert_eq!(n.recprice, "105");
    assert_eq!(n.recprice, s.recprice);
}

/// Covers AE1. `t9942` ELW master list round-trips; single-or-array tolerated;
/// empty is the pending case.
#[test]
fn t9942_request_and_response_round_trip() {
    let value = serde_json::to_value(T9942Request::new()).expect("serialize t9942");
    assert_eq!(value["t9942InBlock"]["dummy"], "");

    let resp: T9942Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9942OutBlock": [
            { "hname": "삼성전자콜ELW", "shcode": "57J123", "expcode": "KR4500001234" },
            { "hname": "SK하이닉스풋ELW", "shcode": "57J456", "expcode": "KR4500005678" }
        ]
    }))
    .expect("representative t9942 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].shcode, "57J123");

    let single: T9942Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9942OutBlock": { "shcode": "57J123", "hname": "삼성전자콜ELW" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock.len(), 1);

    let empty: T9942Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9942OutBlock": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t1958 — ELW종목비교 (ELW comparison; Wave 1). Two ELW shcodes self-sourced
// from t8431; three single-object out-blocks (two details + a comparison block).
// ---------------------------------------------------------------------------

/// Covers AE3. `T1958Request::new` serializes both shcodes under `t1958InBlock`,
/// no continuation leak.
#[test]
fn t1958_request_serializes_with_both_shcodes() {
    let value = serde_json::to_value(T1958Request::new("57J123", "57J456"))
        .expect("serialize t1958 request");
    let inblock = value["t1958InBlock"].as_object().expect("inblock object");
    assert_eq!(inblock.len(), 2, "t1958InBlock carries only shcode1 and shcode2");
    assert_eq!(value["t1958InBlock"]["shcode1"], "57J123");
    assert_eq!(value["t1958InBlock"]["shcode2"], "57J456");
    assert!(value.get("tr_cont").is_none());
}

/// Covers AE3. A representative success deserializes: both symbol detail blocks
/// and the comparison block round-trip, with `hname` (the modeled non-key signal)
/// populated and numeric fields parsing number-or-string.
#[test]
fn t1958_deserializes_success_with_real_values() {
    let resp: T1958Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1958OutBlock": { "hname": "삼성전자콜ELW", "item1": "삼성전자", "elwopt": "2",
            "price": 105, "volume": 100000, "diff": 1.5 },
        "t1958OutBlock1": { "hname": "SK하이닉스풋ELW", "item1": "SK하이닉스", "elwopt": "3",
            "price": "210", "volume": "50000", "diff": "-0.7" },
        "t1958OutBlock2": { "hnamecmp": "비교", "item1cmp": "기초", "pricecmp": 5,
            "volumecmp": 1000, "diffcmp": 0.1 }
    }))
    .expect("representative t1958 success must deserialize");
    assert_eq!(resp.outblock.hname, "삼성전자콜ELW", "symbol 1 detail populated");
    assert_eq!(resp.outblock.price, "105", "price from JSON number");
    assert_eq!(resp.outblock1.price, "210", "price from JSON string");
    assert_eq!(resp.outblock2.pricecmp, "5", "comparison block populated");
}

/// Covers AE3. An empty/degenerate result (unpopulated detail blocks) deserializes
/// and is recognized as the pending case (no comparison payload).
#[test]
fn t1958_empty_result_deserializes_as_empty() {
    let empty: T1958Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1958OutBlock": {},
        "t1958OutBlock1": {},
        "t1958OutBlock2": {}
    }))
    .expect("empty detail blocks must deserialize");
    assert!(
        empty.outblock.hname.is_empty(),
        "an unpopulated symbol-1 block is the pending case, not a flip"
    );
}

/// Covers AE3. `T1958Response` default envelope is empty.
#[test]
fn t1958_response_envelope_default_is_empty() {
    let resp = T1958Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.hname.is_empty());
    assert!(resp.outblock2.hnamecmp.is_empty());
}

// ---------------------------------------------------------------------------
// t1964 — ELW전광판 (ELW board; Wave 1). item (underlying code) self-sourced from
// t9905; broad/default filters for the remaining 10 fields.
// ---------------------------------------------------------------------------

/// `T1964Request::new` serializes the underlying `item` plus the broad/default
/// filters under `t1964InBlock`; no continuation leak.
#[test]
fn t1964_request_serializes_with_item_and_broad_defaults() {
    let value = serde_json::to_value(T1964Request::new("005930"))
        .expect("serialize t1964 request");
    let inblock = value["t1964InBlock"].as_object().expect("inblock object");
    assert_eq!(inblock.len(), 11, "t1964InBlock carries all 11 fields");
    assert_eq!(value["t1964InBlock"]["item"], "005930", "underlying code");
    assert_eq!(value["t1964InBlock"]["elwopt"], "0", "broad call/put filter");
    assert_eq!(value["t1964InBlock"]["issuercd"], "", "broad issuer (all)");
    assert!(value.get("tr_cont").is_none());
}

/// A representative success deserializes: the board rows round-trip with `shcode`
/// (ELW code) and `item1` (underlying code) populated; single-or-array tolerated.
#[test]
fn t1964_deserializes_success_with_real_values() {
    let resp: T1964Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1964OutBlock1": [
            { "shcode": "57J123", "hname": "삼성전자콜ELW", "item1": "005930",
              "itemnm": "삼성전자", "issuernmk": "한국투자" },
            { "shcode": 57456, "hname": "삼성전자풋ELW", "item1": "005930",
              "itemnm": "삼성전자", "issuernmk": "미래에셋" }
        ]
    }))
    .expect("representative t1964 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "57J123", "ELW code populated");
    assert_eq!(resp.outblock1[0].item1, "005930", "underlying code populated");
    assert_eq!(resp.outblock1[1].shcode, "57456", "shcode from JSON number");
}

/// An empty board (`00707`, empty array) deserializes and is the pending case.
#[test]
fn t1964_empty_result_deserializes_as_empty() {
    let empty: T1964Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1964OutBlock1": []
    }))
    .expect("empty board must deserialize");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case");
}

/// A single board row (not an array) is tolerated as a one-element Vec.
#[test]
fn t1964_single_out_row_tolerated_as_array() {
    let single: T1964Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1964OutBlock1": { "shcode": "57J123", "item1": "005930" }
    }))
    .expect("single board row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "57J123");
}

/// `T1964OutBlock1.shcode` parses number-or-string alike.
#[test]
fn t1964_shcode_number_or_string_yields_same_value() {
    let n: T1964OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": 57123 })).expect("number");
    let s: T1964OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": "57123" })).expect("string");
    assert_eq!(n.shcode, "57123");
    assert_eq!(n.shcode, s.shcode);
}

// ---------------------------------------------------------------------------
// Wave 2 — market-flow analytics reads (t1601, t1615, t1640, t1662, t1664).
// gubun-filter screens with documented defaults baked into ::new(). Covers AE1.
// ---------------------------------------------------------------------------

/// Covers AE1. `t1601` bakes documented defaults and deserializes the investor
/// aggregate (single object) with net-buy columns populated.
#[test]
fn t1601_request_and_response_round_trip() {
    let value = serde_json::to_value(T1601Request::new()).expect("serialize t1601");
    assert_eq!(value["t1601InBlock"]["gubun1"], "2", "amount basis");
    assert_eq!(value["t1601InBlock"]["exchgubun"], "K", "KRX");

    let resp: T1601Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1601OutBlock1": { "svolume_08": -1000, "svolume_17": "2000", "svolume_18": 500 }
    }))
    .expect("representative t1601 success must deserialize");
    assert_eq!(resp.outblock1.svolume_08, "-1000", "personal net-buy (number)");
    assert_eq!(resp.outblock1.svolume_17, "2000", "foreign net-buy (string)");
}

/// Covers AE1. `t1615` summary + per-market array round-trip; single-or-array.
#[test]
fn t1615_request_and_response_round_trip() {
    let value = serde_json::to_value(T1615Request::new()).expect("serialize t1615");
    assert_eq!(value["t1615InBlock"]["exchgubun"], "K");

    let resp: T1615Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1615OutBlock": { "sum_volume": 12345, "sum_value": "67890" },
        "t1615OutBlock1": [
            { "hname": "코스피", "sv_08": -100, "sv_17": 200, "sv_18": "-50" },
            { "hname": "코스닥", "sv_08": "10", "sv_17": "-20", "sv_18": 5 }
        ]
    }))
    .expect("representative t1615 success must deserialize");
    assert_eq!(resp.outblock.sum_value, "67890");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].hname, "코스피");

    let single: T1615Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1615OutBlock": {}, "t1615OutBlock1": { "hname": "코스피" }
    }))
    .expect("single market row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
}

/// Covers AE1. `t1640` program summary (single object) round-trips.
#[test]
fn t1640_request_and_response_round_trip() {
    let value = serde_json::to_value(T1640Request::new()).expect("serialize t1640");
    assert_eq!(value["t1640InBlock"]["gubun"], "11", "exchange-all");

    let resp: T1640Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1640OutBlock": { "volume": -500, "value": "12000", "basis": "0.35" }
    }))
    .expect("representative t1640 success must deserialize");
    assert_eq!(resp.outblock.value, "12000", "net-buy amount populated");
    assert_eq!(resp.outblock.volume, "-500");
}

/// Covers AE1. `t1662` by-time array round-trips; single-or-array tolerated.
#[test]
fn t1662_request_and_response_round_trip() {
    let value = serde_json::to_value(T1662Request::new()).expect("serialize t1662");
    assert_eq!(value["t1662InBlock"]["gubun"], "0", "KOSPI");

    let resp: T1662Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1662OutBlock": [
            { "time": "0900", "k200jisu": 350, "tot3": -1000, "volume": 5000 },
            { "time": "0901", "k200jisu": "351", "tot3": "200", "volume": "6000" }
        ]
    }))
    .expect("representative t1662 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].time, "0900");
    assert_eq!(resp.outblock[1].k200jisu, "351", "index from string");

    let empty: T1662Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1662OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty is the pending case");
}

/// Covers AE1. `t1664` cnt serializes as a JSON number; the chart array
/// round-trips.
#[test]
fn t1664_request_serializes_cnt_as_number_and_response_round_trips() {
    let value = serde_json::to_value(T1664Request::new()).expect("serialize t1664");
    assert_eq!(value["t1664InBlock"]["cnt"], 20, "cnt serializes as a JSON number");
    assert!(value["t1664InBlock"]["cnt"].is_number(), "cnt is a number, not a string");
    assert_eq!(value["t1664InBlock"]["mgubun"], "1", "KOSPI");

    let resp: T1664Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1664OutBlock1": [
            { "dt": "20260623", "tjj08": -100, "tjj17": 200, "tjj18": "-50" }
        ]
    }))
    .expect("representative t1664 success must deserialize");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].dt, "20260623");
    assert_eq!(resp.outblock1[0].tjj17, "200", "foreign net-buy");
}

// ---------------------------------------------------------------------------
// [업종] 시세 — sector/index cluster (Wave A). All share `/indtp/market-data`
// (the `tr_cd` header distinguishes them).
// ---------------------------------------------------------------------------

/// Shared sector endpoint path (`T8424_POLICY.path` … `T1516_POLICY.path`).
const INDTP_PATH: &str = "/indtp/market-data";

const T8424_FIXTURE: &str = include_str!("fixtures/t8424_resp.json");
const T1511_FIXTURE: &str = include_str!("fixtures/t1511_resp.json");
const T1485_FIXTURE: &str = include_str!("fixtures/t1485_resp.json");
const T1516_FIXTURE: &str = include_str!("fixtures/t1516_resp.json");

/// Covers R4, R6. `t8424` serializes to exactly `{"t8424InBlock":{"gubun1":""}}`
/// with no continuation tokens (non-paginated).
#[test]
fn t8424_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8424Request::new()).expect("serialize t8424 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8424InBlock"]["gubun1"], "", "gubun1 empty placeholder");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5, R6. The spec-derived fixture deserializes through REAL dispatch:
/// the sector array round-trips with a real `upcode`/`hname`, and `upcode` is a
/// string (never coerced numeric).
#[tokio::test]
async fn t8424_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t8424"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8424_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sectors(&T8424Request::new())
        .await
        .expect("t8424 sectors should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock.len() >= 3, "sector rows round-trip");
    assert_eq!(resp.outblock[0].upcode, "001", "first sector upcode (string)");
    assert!(!resp.outblock[0].hname.is_empty(), "real non-default hname");
}

/// Covers R4, R6. A single-object `t8424OutBlock` (one sector) still deserializes
/// via `de_vec_or_single` — not only the array form.
#[test]
fn t8424_single_object_outblock_deserializes() {
    let resp: T8424Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8424OutBlock": { "hname": "종합", "upcode": "001" }
    }))
    .expect("single-object t8424OutBlock must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].upcode, "001");
}

/// Covers R4, R7. `t1511` serializes to `{"t1511InBlock":{"upcode":"001"}}`.
#[test]
fn t1511_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1511Request::new("001")).expect("serialize t1511 request");
    assert_eq!(value["t1511InBlock"]["upcode"], "001", "upcode stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// Covers R2, R5, R7. `t1511` single-OutBlock snapshot deserializes through REAL
/// dispatch; numeric fields tolerate both string and number wire forms.
#[tokio::test]
async fn t1511_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1511"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1511_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sector_quote(&T1511Request::new("001"))
        .await
        .expect("t1511 sector_quote should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock.hname.is_empty(), "real non-default hname");
    assert_eq!(resp.outblock.pricejisu, "2610.62", "현재지수 current index (was a number)");
    assert!(!resp.outblock.firstjisu.is_empty(), "first sub-index populated");
}

/// Covers R4, R5. The `volume` field tolerates a JSON number or string via
/// `string_or_number` (the gateway sends `volume` as an integer).
#[test]
fn t1511_volume_number_or_string_yields_same_value() {
    let as_number: T1511Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1511OutBlock": { "hname": "종합", "volume": 263165 }
    }))
    .expect("number volume must deserialize");
    let as_string: T1511Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1511OutBlock": { "hname": "종합", "volume": "263165" }
    }))
    .expect("string volume must deserialize");
    assert_eq!(as_number.outblock.volume, "263165");
    assert_eq!(as_number.outblock.volume, as_string.outblock.volume);
}

/// Covers R4, R7. `t1485` serializes to `{"t1485InBlock":{"upcode":"001","gubun":"1"}}`.
#[test]
fn t1485_request_serializes_to_inblock() {
    let value =
        serde_json::to_value(T1485Request::new("001", "1")).expect("serialize t1485 request");
    assert_eq!(value["t1485InBlock"]["upcode"], "001");
    assert_eq!(value["t1485InBlock"]["gubun"], "1");
}

/// Covers R2, R5, R7. `t1485` summary block + time-row array round-trip through
/// REAL dispatch; the `t1485OutBlock1` array (and single-object form) deserialize.
#[tokio::test]
async fn t1485_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1485"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1485_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sector_expected_index(&T1485Request::new("001", "1"))
        .await
        .expect("t1485 sector_expected_index should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    // Summary block round-trips (separate struct from the time array — a rename
    // typo on t1485OutBlock would silently zero these without this assertion).
    assert_eq!(resp.outblock.pricejisu, "2610.62", "summary 예상지수");
    assert_eq!(resp.outblock.volume, "263165", "summary volume (was a JSON number)");
    assert!(resp.outblock1.len() >= 2, "expected-index time rows round-trip");
    assert!(!resp.outblock1[0].jisu.is_empty(), "real non-default jisu");
}

/// Covers R4, R7. `t1485OutBlock1` single-object form deserializes via `de_vec_or_single`.
#[test]
fn t1485_single_object_outblock1_deserializes() {
    let resp: T1485Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1485OutBlock1": { "jisu": "2617.03", "volume": 7372, "chetime": "장  전" }
    }))
    .expect("single-object t1485OutBlock1 must deserialize");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].chetime, "장  전", "non-numeric chetime label");
}

/// Covers R4, R7. `t1516` carries TWO caller identifiers — serializes to
/// `{"t1516InBlock":{"upcode":"001","gubun":"1","shcode":"005930"}}`.
#[test]
fn t1516_request_serializes_two_identifiers() {
    let value = serde_json::to_value(T1516Request::new("001", "1", "005930"))
        .expect("serialize t1516 request");
    assert_eq!(value["t1516InBlock"]["upcode"], "001");
    assert_eq!(value["t1516InBlock"]["shcode"], "005930", "second required input");
}

/// Covers R2, R5, R7. `t1516` summary + per-stock array round-trip through REAL
/// dispatch.
#[tokio::test]
async fn t1516_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1516"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1516_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sector_stocks(&T1516Request::new("001", "1", ""))
        .await
        .expect("t1516 sector_stocks should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    // Summary header round-trips (separate struct from the per-stock array).
    assert_eq!(resp.outblock.shcode, "000640", "echoed board shcode");
    assert_eq!(resp.outblock.pricejisu, "000002610.62", "summary 지수");
    assert!(resp.outblock1.len() >= 2, "per-stock rows round-trip");
    assert!(!resp.outblock1[0].shcode.is_empty(), "real per-stock shcode");
    assert!(!resp.outblock1[0].hname.is_empty(), "real per-stock name");
}

/// Covers R5, R7. `t1516OutBlock1` single-object form deserializes via
/// `de_vec_or_single`, and an empty board (00707) is the pending case.
#[test]
fn t1516_single_and_empty_outblock1_deserialize() {
    let single: T1516Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1516OutBlock": { "shcode": "000640", "pricejisu": "2610.62" },
        "t1516OutBlock1": { "shcode": "005930", "hname": "삼성전자", "price": 70000 }
    }))
    .expect("single per-stock row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "70000", "price from JSON number");

    let empty: T1516Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1516OutBlock": { "shcode": "" }, "t1516OutBlock1": []
    }))
    .expect("empty board deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case");
}

/// Covers R5, R6. An empty `t8424` sector list (00707) deserializes as the
/// pending case, mirroring every prior array-bearing TR.
#[test]
fn t8424_empty_result_set_deserializes_as_pending() {
    let empty: T8424Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8424OutBlock": []
    }))
    .expect("empty sector list deserializes");
    assert!(empty.outblock.is_empty(), "empty list is the pending case");
}

// ---------------------------------------------------------------------------
// t2301 — 옵션전광판 (option board; F/O). market_session, non-paginated. Keyed by
// a contract month `yyyymm` (월물) + a `gubun` mini/regular selector. Single
// out-block (a representative subset of the 76-field board header).
// ---------------------------------------------------------------------------

/// `T2301_POLICY.path` — the F/O market-data endpoint.
const FO_MARKET_DATA_PATH: &str = "/futureoption/market-data";

const T2301_FIXTURE: &str = include_str!("fixtures/t2301_resp.json");

/// Covers R4. `t2301` serializes to exactly
/// `{"t2301InBlock":{"yyyymm":"202609","gubun":"G"}}` with no continuation tokens
/// (non-paginated) and `yyyymm` stays a string (no caller fields leak).
#[test]
fn t2301_request_serializes_to_inblock() {
    let value =
        serde_json::to_value(T2301Request::new("202609", "G")).expect("serialize t2301 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t2301InBlock"]["yyyymm"], "202609", "yyyymm stays a string");
    assert_eq!(value["t2301InBlock"]["gubun"], "G", "gubun selector serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5, R6 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the board header round-trips and the canonical current-value field
/// `gmprice` (근월물현재가, near-month futures current price) holds its EXACT
/// value. The fixture's neighbouring fields carry DISTINCT values, so a mislabel
/// that picked `gmchange`/`cimpv` instead would surface here (the Wave A
/// `firstjisu`/`pricejisu` guard).
#[tokio::test]
async fn t2301_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t2301"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T2301_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .option_board(&T2301Request::new("202609", "G"))
        .await
        .expect("t2301 option_board should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    // The canonical current-value field, by Korean name 근월물현재가 — exact value.
    assert_eq!(
        resp.outblock.gmprice, "331.40",
        "근월물현재가 near-month futures current price (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto gmprice's value.
    assert_eq!(resp.outblock.gmchange, "1.85", "근월물전일대비 (distinct from gmprice)");
    assert_eq!(resp.outblock.cimpv, "14.07", "콜옵션대표IV (distinct from gmprice)");
    assert_eq!(resp.outblock.pimpv, "15.92", "풋옵션대표IV (distinct from cimpv)");
    assert_eq!(resp.outblock.gmvolume, "184523", "근월물거래량 (was a JSON number)");
}

/// Covers R4, R5. The `gmvolume` field tolerates a JSON number or string via
/// `string_or_number` (the gateway sends `gmvolume` as an integer).
#[test]
fn t2301_numeric_number_or_string_yields_same_value() {
    let as_number: T2301Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2301OutBlock": { "gmprice": 331, "gmvolume": 184523 }
    }))
    .expect("number gmvolume must deserialize");
    let as_string: T2301Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2301OutBlock": { "gmprice": "331", "gmvolume": "184523" }
    }))
    .expect("string gmvolume must deserialize");
    assert_eq!(as_number.outblock.gmvolume, "184523");
    assert_eq!(as_number.outblock.gmvolume, as_string.outblock.gmvolume);
    assert_eq!(as_number.outblock.gmprice, as_string.outblock.gmprice, "gmprice both forms");
}

/// Covers R5, R6. An empty `t2301` board (00707, empty out-block) deserializes as
/// the pending case — the canonical field defaults to empty.
#[test]
fn t2301_empty_result_deserializes_as_pending() {
    let empty: T2301Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2301OutBlock": {}
    }))
    .expect("empty board deserializes");
    assert!(empty.outblock.gmprice.is_empty(), "empty board is the pending case");
}

// ---------------------------------------------------------------------------
// t2522 — 주식선물기초자산조회 (stock-futures underlying-asset master; F/O).
// market_session, non-paginated, no caller input. Single out-block (a
// representative subset of its 6 fields).
// ---------------------------------------------------------------------------

const T2522_FIXTURE: &str = include_str!("fixtures/t2522_resp.json");

/// Covers R4. `t2522` serializes to exactly `{"t2522InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t2522_request_serializes_to_inblock() {
    let value = serde_json::to_value(T2522Request::new()).expect("serialize t2522 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t2522InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t2522InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the count header round-trips and the canonical identity field
/// `bsc_asts_nm` (기초자산명, underlying-asset name) — which lives in the
/// `t2522OutBlock1` row array, not the count header — holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `bsc_asts_is_cd`/`nmc_is_shrt_cd` instead would surface here.
#[tokio::test]
async fn t2522_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t2522"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T2522_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_futures_underlying(&T2522Request::new())
        .await
        .expect("t2522 stock_futures_underlying should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.cnt, "2", "건수 count header (was a JSON number)");
    assert_eq!(resp.outblock1.len(), 2, "two underlying-asset rows");
    let row = &resp.outblock1[0];
    // The canonical identity field, by Korean name 기초자산명 — exact value.
    assert_eq!(
        row.bsc_asts_nm, "삼성전자",
        "기초자산명 underlying-asset name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto bsc_asts_nm.
    assert_eq!(row.bsc_asts_is_cd, "005930", "기초자산종목코드 (distinct)");
    assert_eq!(row.bsc_asts_id, "KR7", "기초자산ID (distinct)");
    assert_eq!(row.nmc_is_shrt_cd, "111W9000", "최근월물종목코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock1[1].bsc_asts_nm, "SK하이닉스", "second row distinct");
}

/// Covers R4, R5. The numeric fields tolerate a JSON number or string via
/// `string_or_number` (the gateway sends `cnt` as an integer).
#[test]
fn t2522_numeric_number_or_string_yields_same_value() {
    let as_number: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2522OutBlock": { "cnt": 42 },
        "t2522OutBlock1": [{ "bsc_asts_nm": "삼성전자" }]
    }))
    .expect("number cnt must deserialize");
    let as_string: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2522OutBlock": { "cnt": "42" },
        "t2522OutBlock1": [{ "bsc_asts_nm": "삼성전자" }]
    }))
    .expect("string cnt must deserialize");
    assert_eq!(as_number.outblock.cnt, "42");
    assert_eq!(as_number.outblock.cnt, as_string.outblock.cnt);
    assert_eq!(
        as_number.outblock1[0].bsc_asts_nm, as_string.outblock1[0].bsc_asts_nm,
        "bsc_asts_nm both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t2522OutBlock1` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t2522_single_object_row_deserializes_to_one_element_vec() {
    let single: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2522OutBlock": { "cnt": 1 },
        "t2522OutBlock1": { "bsc_asts_nm": "삼성전자" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].bsc_asts_nm, "삼성전자");
    // The standalone row struct also default-constructs cleanly.
    assert!(T2522OutBlock1::default().bsc_asts_nm.is_empty());
}

/// Covers R5. An empty `t2522` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t2522_empty_result_deserializes_as_pending() {
    let empty: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2522OutBlock": {}
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock1.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// t8401 — 주식선물마스터조회 (stock-futures master; F/O). market_session,
// non-paginated, no caller input. A single ROW-ARRAY out-block `t8401OutBlock`
// (no separate count header): one stock-futures contract per row. All four
// modeled fields are spec `String` types (no `string_or_number` coercion), so
// the shared contract's number-or-string item does not apply to this TR.
// ---------------------------------------------------------------------------

const T8401_FIXTURE: &str = include_str!("fixtures/t8401_resp.json");

/// Covers R4. `t8401` serializes to exactly `{"t8401InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t8401_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8401Request::new()).expect("serialize t8401 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8401InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t8401InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the row array round-trips and the canonical identity field `hname`
/// (종목명, the stock-futures contract name) holds its EXACT value. The fixture's
/// neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode`/`basecode` instead would surface here.
#[tokio::test]
async fn t8401_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8401"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8401_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_futures_master(&T8401Request::new())
        .await
        .expect("t8401 stock_futures_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "two stock-futures master rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "삼성전자   F 202307",
        "종목명 stock-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "111T7000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4111T70004", "확장코드 (distinct)");
    assert_eq!(row.basecode, "A005930", "기초자산코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "삼성화재   F 202512", "second row distinct");
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8401OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8401_single_object_row_deserializes_to_one_element_vec() {
    let single: T8401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8401OutBlock": { "hname": "삼성전자   F 202307" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "삼성전자   F 202307");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8401OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8401` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8401_empty_result_deserializes_as_pending() {
    let empty: T8401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// t8426 — 상품선물마스터조회 (commodity-futures master; F/O). market_session,
// non-paginated, no caller input. A single ROW-ARRAY out-block `t8426OutBlock`
// (confirmed from the raw capture's `res_example`; no separate count header):
// one commodity-futures contract per row. The wire out-block key is the literal
// `t8426OutBlock` — the normalized baseline collapses it to `response_body`, so
// the rename was taken from the raw capture, not the baseline.
// ---------------------------------------------------------------------------

const T8426_FIXTURE: &str = include_str!("fixtures/t8426_resp.json");

/// Covers R4. `t8426` serializes to exactly `{"t8426InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t8426_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8426Request::new()).expect("serialize t8426 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8426InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t8426InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the row array round-trips and the canonical identity field `hname`
/// (종목명, the commodity-futures contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode` instead would surface here.
#[tokio::test]
async fn t8426_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8426"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8426_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .commodity_futures_master(&T8426Request::new())
        .await
        .expect("t8426 commodity_futures_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "two commodity-futures master rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "금          F 202306",
        "종목명 commodity-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "175T6000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4175T60003", "확장코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "돈육          F 202309", "second row distinct");
}

/// Covers shared contract item 2. `shcode` (단축코드) parses via
/// `ls_core::string_or_number` from BOTH a string and a JSON number — the gateway
/// may send a numeric-looking code either way.
#[test]
fn t8426_shcode_number_or_string_yields_same_value() {
    let as_number: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8426OutBlock": [{ "hname": "금          F 202306", "shcode": 1756000 }]
    }))
    .expect("numeric shcode deserializes");
    let as_string: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8426OutBlock": [{ "hname": "금          F 202306", "shcode": "1756000" }]
    }))
    .expect("string shcode deserializes");
    assert_eq!(as_number.outblock[0].shcode, "1756000");
    assert_eq!(as_string.outblock[0].shcode, "1756000");
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8426OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8426_single_object_row_deserializes_to_one_element_vec() {
    let single: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8426OutBlock": { "hname": "금          F 202306" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "금          F 202306");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8426OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8426` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8426_empty_result_deserializes_as_pending() {
    let empty: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}
