//! Market-session (`t1102`) dependency-class tests.
//!
//! Exercises the `t1102` current-price quote against wiremock through REAL
//! `ls-core` dispatch (the mock config injects `base_url`, so the POST hits the
//! mock server). Covers request body shape (no continuation tokens), serde
//! against the spec-derived fixture, the string-or-number field-semantics
//! regression, and `01900` paper-incompatible classification.

use ls_core::{Inner, LsError};
use ls_sdk::market_session::{
    T1302Request, T1302Response, T2216Request, T2216Response,
    T1532Request, T1532Response, T1533Request, T1533Response, T1926Request, T1926Response, T1764Request, T1764Response, T1903Request, T1903Response,
    T1101OutBlock, T1101Request, T1101Response, T1102OutBlock, T1102Request, T1102Response,
    T1531Request, T1531Response, T1537Request, T1537Response, T1601Request, T1601Response,
    T1615Request, T1615Response, T1640Request, T1640Response, T1662Request, T1662Response,
    T1664Request, T1664Response, T1825OutBlock1, T1825Request, T1825Response, T1826OutBlock,
    T1826Request, T1826Response, T1859OutBlock1, T1859Request, T1859Response, T1958Request,
    T1958Response, T1964OutBlock1, T1964Request, T1964Response, T1485Request, T1485Response,
    T1104Request, T1104Response, T1105Request, T1105Response,
    T1511Request, T1511Response, T1516Request, T1516Response, T1901Request, T1901Response,
    T1906Request, T1906Response,
    T8450Request, T8450Response,
    T1638Request, T1638Response,
    T1308Request, T1308Response,
    T1449Request, T1449Response,
    T1621Request, T1621Response,
    T2545Request, T2545Response,
    T8406Request, T8406Response,
    T8407Request, T8407Response,
    T1631Request, T1631Response,
    T1632Request, T1632Response,
    T1633Request, T1633Response,
    T1959Request, T1959Response,
    T1950Request, T1950Response,
    T1971Request, T1971Response,
    T1972Request, T1972Response,
    T1974Request, T1974Response,
    T1956Request, T1956Response,
    T1969Request, T1969Response,
    T8424Request, T8424Response,
    T2301Request, T2301Response, T2522OutBlock1, T2522Request, T2522Response, T8401OutBlock,
    T8401Request, T8401Response, T8426OutBlock, T8426Request, T8426Response, T8433OutBlock,
    T8433Request, T8433Response, T8435OutBlock, T8435Request, T8435Response, T8467OutBlock,
    T8467Request, T8467Response, T9943OutBlock, T9943Request, T9943Response, T9944OutBlock,
    T9944Request, T9944Response, T8425Request,
    T8425Response, T8430OutBlock, T8430Request, T8430Response, T8431OutBlock, T8431Request,
    T8431Response, T8436Request, T8436Response, T9905OutBlock1, T9905Request, T9905Response,
    T9907Request, T9907Response, T9942Request, T9942Response,
    T2106Request, T2106Response, T2111OutBlock, T2111Request, T2111Response, T2112OutBlock,
    T2112Request, T2112Response, T8402OutBlock, T8402Request, T8402Response, T8403OutBlock,
    T8403Request, T8403Response, T8434OutBlock1, T8434Request, T8434Response,
    T1988OutBlock, T1988Request, T1988Response, T3102Request, T3102Response, T3320OutBlock,
    T3320Request, T3320Response,
    T8455OutBlock, T8455Request, T8455Response, T8460Request, T8460Response, T8463OutBlock,
    T8463Request, T8463Response,
    G3101OutBlock, G3101Request, G3101Response, G3102Request, G3102Response, G3103Request,
    G3103Response, G3104OutBlock, G3104Request, G3104Response, G3106OutBlock, G3106Request,
    G3106Response, G3190Request, G3190Response,
    O3101OutBlock, O3101Request, O3101Response, O3105OutBlock, O3105Request, O3105Response,
    O3106OutBlock, O3106Request, O3106Response, O3121Request, O3121Response, O3125OutBlock,
    O3125Request, O3125Response, O3126OutBlock, O3126Request, O3126Response,
    O3104Request, O3104Response, O3127Request, O3127Response, T8462Request, T8462Response,
    T9945Request, T9945Response, T3202Request, T3202Response, T3521Request, T3521Response,
    T0167Request, T0167Response,
};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The spec-derived `t1102` response fixture (`fixtures/t1102_resp.json`).
const T1102_FIXTURE: &str = include_str!("fixtures/t1102_resp.json");

/// The spec-derived `t0167` server-time fixture (`fixtures/t0167_resp.json`).
const T0167_FIXTURE: &str = include_str!("fixtures/t0167_resp.json");

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

/// Covers AE1. `t8430` stock-issue list round-trips; `gubun` request is a plain
/// code string ("0" all); numeric-bearing fields parse number-or-string;
/// single-or-array tolerated; empty `00707` is the pending case.
#[test]
fn t8430_request_and_response_round_trip() {
    let value = serde_json::to_value(T8430Request::all()).expect("serialize t8430");
    assert_eq!(value["t8430InBlock"]["gubun"], "0", "all-markets code string");

    let resp: T8430Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8430OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfgubun": "0", "uplmtprice": 91900, "dnlmtprice": "49500",
              "jnilclose": 70700, "memedan": "1", "recprice": 70700, "gubun": "1" },
            { "hname": "에코프로", "shcode": "086520", "expcode": "KR7086520004",
              "etfgubun": "0", "uplmtprice": "120000", "dnlmtprice": 64600,
              "jnilclose": "92300", "memedan": "1", "recprice": "92300", "gubun": "2" }
        ]
    }))
    .expect("representative t8430 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].shcode, "005930", "shcode populated");
    assert_eq!(
        resp.outblock[0].uplmtprice, "91900",
        "uplmtprice from JSON number"
    );
    assert_eq!(
        resp.outblock[1].uplmtprice, "120000",
        "uplmtprice from JSON string"
    );
    assert_eq!(resp.outblock[1].gubun, "2", "KOSDAQ market flag");

    let single: T8430Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8430OutBlock": { "shcode": "005930", "hname": "삼성전자" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock.len(), 1);

    let empty: T8430Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8430OutBlock": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock.is_empty(), "empty is the pending case");
}

/// Covers AE1. `T8430OutBlock` numeric-bearing fields parse number-or-string alike.
#[test]
fn t8430_price_number_or_string_yields_same_value() {
    let n: T8430OutBlock =
        serde_json::from_value(serde_json::json!({ "uplmtprice": 91900 })).expect("number");
    let s: T8430OutBlock =
        serde_json::from_value(serde_json::json!({ "uplmtprice": "91900" })).expect("string");
    assert_eq!(n.uplmtprice, "91900");
    assert_eq!(n.uplmtprice, s.uplmtprice);
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

// ---- t1901 ETF현재가 (plan -002 Track 2; market_session single-object read) ----

/// `t1901` serializes to `{"t1901InBlock":{"shcode":"069500"}}` (shcode-only,
/// non-paginated — no tr_cont tokens in the body).
#[test]
fn t1901_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1901Request::new("069500")).expect("serialize t1901 request");
    assert_eq!(value["t1901InBlock"]["shcode"], "069500", "shcode stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// The numeric `price` field tolerates a JSON number or string via
/// `string_or_number` (the gateway sends ETF prices as integers).
#[test]
fn t1901_price_number_or_string_yields_same_value() {
    let as_number: T1901Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1901OutBlock": { "hname": "KODEX 200", "price": 135155 }
    }))
    .expect("number price must deserialize");
    let as_string: T1901Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1901OutBlock": { "hname": "KODEX 200", "price": "135155" }
    }))
    .expect("string price must deserialize");
    assert_eq!(as_number.outblock.price, "135155");
    assert_eq!(as_number.outblock.price, as_string.outblock.price);
}

/// An empty/sparse result (e.g. a `00707` no-data envelope with no out-block)
/// deserializes cleanly to defaults — no panic on a missing `t1901OutBlock`.
#[test]
fn t1901_empty_result_deserializes_to_defaults() {
    let empty: T1901Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1901 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no out-block → default hname");
    assert!(empty.outblock.price.is_empty(), "no out-block → default price");
}

// ---- t1906 ETFLP호가 (closed-window more-flips wave; market_session single-object) ----

/// `t1906` serializes to `{"t1906InBlock":{"shcode":"152100"}}` (shcode-only,
/// non-paginated — no tr_cont tokens in the body).
#[test]
fn t1906_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1906Request::new("152100")).expect("serialize t1906 request");
    assert_eq!(value["t1906InBlock"]["shcode"], "152100", "shcode stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. The numeric `price` tolerates a
/// JSON number or string via `string_or_number`.
#[test]
fn t1906_success_body_deserializes_with_nondefault_field() {
    let as_number: T1906Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1906OutBlock": {
            "hname": "TIGER 코스닥150",
            "price": 3685,
            "offerho1": 3690,
            "bidho1": 3685,
            "lp_offerrem1": 0,
            "shcode": "152100"
        }
    }))
    .expect("number price must deserialize");
    let as_string: T1906Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1906OutBlock": {
            "hname": "TIGER 코스닥150",
            "price": "3685",
            "offerho1": "3690",
            "bidho1": "3685",
            "lp_offerrem1": "0",
            "shcode": "152100"
        }
    }))
    .expect("string price must deserialize");
    assert_eq!(as_number.outblock.price, "3685", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock.offerho1, "3690");
    assert_eq!(as_number.outblock.shcode, "152100");
    assert_eq!(as_number.outblock.price, as_string.outblock.price);
    assert_eq!(as_number.outblock.offerho1, as_string.outblock.offerho1);
}

/// An empty result (`00707`, no out-block) deserializes cleanly to defaults — no
/// panic on a missing `t1906OutBlock`; recognized as the empty/pending case.
#[test]
fn t1906_empty_result_deserializes_to_defaults() {
    let empty: T1906Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1906 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no out-block → default hname");
    assert!(empty.outblock.price.is_empty(), "no out-block → default price");
}

// ---- t8450 (통합)주식현재가호가조회2 (closed-window more-flips; market_session single-object) ----

/// `t8450` serializes to `{"t8450InBlock":{"shcode":"005930","exchgubun":"N"}}`
/// (shcode + exchgubun, non-paginated — no tr_cont tokens in the body).
#[test]
fn t8450_request_serializes_to_inblock() {
    let value =
        serde_json::to_value(T8450Request::new("005930", "N")).expect("serialize t8450 request");
    assert_eq!(value["t8450InBlock"]["shcode"], "005930", "shcode stays a string");
    assert_eq!(value["t8450InBlock"]["exchgubun"], "N", "exchgubun stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. The numeric `price` tolerates a JSON
/// number or string via `string_or_number`.
#[test]
fn t8450_success_body_deserializes_with_nondefault_field() {
    let as_number: T8450Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8450OutBlock": {
            "hname": "S-Oil",
            "price": 60600,
            "offerho1": 60700,
            "bidho1": 60600,
            "open": 60400,
            "shcode": "010950"
        }
    }))
    .expect("number price must deserialize");
    let as_string: T8450Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8450OutBlock": {
            "hname": "S-Oil",
            "price": "60600",
            "offerho1": "60700",
            "bidho1": "60600",
            "open": "60400",
            "shcode": "010950"
        }
    }))
    .expect("string price must deserialize");
    assert_eq!(as_number.outblock.price, "60600", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock.offerho1, "60700");
    assert_eq!(as_number.outblock.shcode, "010950");
    assert_eq!(as_number.outblock.price, as_string.outblock.price);
    assert_eq!(as_number.outblock.offerho1, as_string.outblock.offerho1);
}

/// An empty result (`00707`, no out-block) deserializes cleanly to defaults — no
/// panic on a missing `t8450OutBlock`; recognized as the empty/pending case.
#[test]
fn t8450_empty_result_deserializes_to_defaults() {
    let empty: T8450Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t8450 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no out-block → default hname");
    assert!(empty.outblock.price.is_empty(), "no out-block → default price");
}

// ---- t1638 종목별잔량/사전공시 (closed-window more-flips; market_session OutBlock array) ----

/// `t1638` serializes to
/// `{"t1638InBlock":{"gubun1":"1","shcode":"","gubun2":"1","exchgubun":""}}`
/// (all four String fields, `shcode` empty for the full list; non-paginated — no
/// tr_cont tokens in the body).
#[test]
fn t1638_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1638Request::new("1", "", "1", ""))
        .expect("serialize t1638 request");
    assert_eq!(value["t1638InBlock"]["gubun1"], "1", "gubun1 stays a string");
    assert_eq!(value["t1638InBlock"]["shcode"], "", "shcode stays an empty string (full list)");
    assert_eq!(value["t1638InBlock"]["gubun2"], "1", "gubun2 stays a string");
    assert_eq!(value["t1638InBlock"]["exchgubun"], "", "exchgubun stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. The numeric fields tolerate a JSON
/// number or string via `string_or_number`; the out-block is a repeated array.
#[test]
fn t1638_success_body_deserializes_with_nondefault_field() {
    let as_number: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1638OutBlock": [
            { "rank": 1, "hname": "삼성전자", "price": 60600, "obuyvol": 1200, "buyrem": 5000, "shcode": "005930" },
            { "rank": 2, "hname": "S-Oil", "price": 60400, "obuyvol": -300, "sellrem": 4000, "shcode": "010950" }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1638OutBlock": [
            { "rank": "1", "hname": "삼성전자", "price": "60600", "obuyvol": "1200", "buyrem": "5000", "shcode": "005930" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock.len(), 2, "array out-block round-trips");
    assert_eq!(as_number.outblock[0].price, "60600", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock[0].rank, "1");
    assert_eq!(as_number.outblock[0].shcode, "005930");
    assert_eq!(as_number.outblock[0].price, as_string.outblock[0].price);
    assert_eq!(as_number.outblock[0].rank, as_string.outblock[0].rank);
}

/// A single (non-array) out-block object is tolerated via `de_vec_or_single`.
#[test]
fn t1638_single_object_out_block_is_tolerated() {
    let resp: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1638OutBlock": { "rank": 1, "hname": "삼성전자", "price": 60600, "shcode": "005930" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock[0].shcode, "005930");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1638OutBlock`; recognized as the empty/pending case.
#[test]
fn t1638_empty_result_deserializes_to_defaults() {
    let empty: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1638 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty(), "no out-block → empty Vec");
}

// ---- t1308 주식시간대별체결조회챠트 (closed-window more-flips; market_session, summary + time-bucket array) ----

/// `t1308` serializes to
/// `{"t1308InBlock":{"shcode":"005930","starttime":"","endtime":"","bun_term":"1","exchgubun":""}}`
/// (all five String fields, `starttime`/`endtime`/`exchgubun` empty for the full
/// session; non-paginated — no tr_cont tokens in the body).
#[test]
fn t1308_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1308Request::new("005930", "", "", "1", ""))
        .expect("serialize t1308 request");
    assert_eq!(value["t1308InBlock"]["shcode"], "005930", "shcode stays a string");
    assert_eq!(value["t1308InBlock"]["starttime"], "", "starttime stays an empty string");
    assert_eq!(value["t1308InBlock"]["endtime"], "", "endtime stays an empty string");
    assert_eq!(value["t1308InBlock"]["bun_term"], "1", "bun_term stays a string (not a number)");
    assert_eq!(value["t1308InBlock"]["exchgubun"], "", "exchgubun stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. The numeric fields tolerate a JSON
/// number or string via `string_or_number`; the time-bucket out-block is a
/// repeated array.
#[test]
fn t1308_success_body_deserializes_with_nondefault_field() {
    let as_number: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1308OutBlock": { "ex_shcode": "005930" },
        "t1308OutBlock1": [
            { "chetime": "102700", "price": 3685, "sign": "2", "change": 25, "volume": 321201, "open": 3685, "high": 3685, "low": 3685 },
            { "chetime": "090030", "price": 3660, "sign": "3", "change": 0, "volume": 19857, "open": 3660, "high": 3660, "low": 3660 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1308OutBlock": { "ex_shcode": "005930" },
        "t1308OutBlock1": [
            { "chetime": "102700", "price": "3685", "sign": "2", "change": "25", "volume": "321201", "open": "3685", "high": "3685", "low": "3685" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "time-bucket array round-trips");
    assert_eq!(as_number.outblock.ex_shcode, "005930", "summary block round-trips");
    assert_eq!(as_number.outblock1[0].price, "3685", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].chetime, "102700");
    assert_eq!(as_number.outblock1[0].price, as_string.outblock1[0].price);
    assert_eq!(as_number.outblock1[0].volume, as_string.outblock1[0].volume);
}

/// A single (non-array) `t1308OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1308_single_object_out_block_is_tolerated() {
    let resp: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1308OutBlock1": { "chetime": "102700", "price": 3685, "volume": 321201 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].chetime, "102700");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1308OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1308_empty_result_deserializes_to_defaults() {
    let empty: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1308 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.ex_shcode.is_empty(), "summary block defaults to empty");
}

// ---- t1449 가격대별매매비중조회 (closed-window more-flips; market_session, summary + price-band array) ----

/// `t1449` serializes to `{"t1449InBlock":{"shcode":"005930","dategb":"1"}}`
/// (both String fields; `dategb` non-empty; non-paginated — no tr_cont tokens
/// in the body).
#[test]
fn t1449_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1449Request::new("005930", "1"))
        .expect("serialize t1449 request");
    assert_eq!(value["t1449InBlock"]["shcode"], "005930", "shcode stays a string");
    assert_eq!(value["t1449InBlock"]["dategb"], "1", "dategb stays a string (not a number)");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. The numeric fields tolerate a JSON
/// number or string via `string_or_number`; the price-band out-block is a
/// repeated array. Body shape is the captured raw `res_example`.
#[test]
fn t1449_success_body_deserializes_with_nondefault_field() {
    let as_number: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1449OutBlock": { "volume": 322192, "price": 3685, "change": 25, "msvolume": 195607, "sign": "2", "diff": "0.68", "mdvolume": 120522 },
        "t1449OutBlock1": [
            { "price": 3750, "change": 90, "msvolume": 22107, "sign": "2", "msdiff": "100.00", "diff": "6.86", "tickdiff": "2.46", "mdvolume": 0, "cvolume": 22107 },
            { "price": 3645, "change": -15, "msvolume": 0, "sign": "5", "msdiff": "0.00", "diff": "0.05", "tickdiff": "-0.41", "mdvolume": 147, "cvolume": 147 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1449OutBlock": { "volume": "322192", "price": "3685", "change": "25", "msvolume": "195607", "sign": "2", "diff": "0.68", "mdvolume": "120522" },
        "t1449OutBlock1": [
            { "price": "3750", "change": "90", "msvolume": "22107", "sign": "2", "msdiff": "100.00", "diff": "6.86", "tickdiff": "2.46", "mdvolume": "0", "cvolume": "22107" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "price-band array round-trips");
    assert_eq!(as_number.outblock.price, "3685", "summary block round-trips");
    assert_eq!(as_number.outblock1[0].price, "3750", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].cvolume, "22107");
    assert_eq!(as_number.outblock1[0].price, as_string.outblock1[0].price);
    assert_eq!(as_number.outblock.volume, as_string.outblock.volume);
}

/// A single (non-array) `t1449OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1449_single_object_out_block_is_tolerated() {
    let resp: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1449OutBlock1": { "price": 3750, "cvolume": 22107 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "3750");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1449OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1449_empty_result_deserializes_to_defaults() {
    let empty: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1449 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.price.is_empty(), "summary block defaults to empty");
}

// ---- t1621 업종별분별투자자매매동향 (closed-window more-flips; market_session, header + by-time array) ----

/// `t1621` serializes to
/// `{"t1621InBlock":{"upcode":"001","nmin":0,"cnt":20,"bgubun":"0","exchgubun":""}}`
/// — `upcode`/`bgubun`/`exchgubun` stay strings, but `nmin` and `cnt` MUST
/// serialize as JSON NUMBERS (KTD3 — the string form returns IGW40011 at the
/// gateway). Non-paginated — no tr_cont tokens in the body.
#[test]
fn t1621_request_serializes_nmin_and_cnt_as_numbers() {
    let value = serde_json::to_value(T1621Request::new("001", "0", "20", "0", ""))
        .expect("serialize t1621 request");
    assert_eq!(value["t1621InBlock"]["upcode"], "001", "upcode stays a string");
    assert_eq!(value["t1621InBlock"]["bgubun"], "0", "bgubun stays a string");
    assert_eq!(value["t1621InBlock"]["exchgubun"], "", "exchgubun stays a string");
    assert_eq!(
        value["t1621InBlock"]["nmin"], 0,
        "nmin serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert_eq!(
        value["t1621InBlock"]["cnt"], 20,
        "cnt serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert!(value["t1621InBlock"]["cnt"].is_number(), "cnt is a JSON number");
    assert!(value["t1621InBlock"]["nmin"].is_number(), "nmin is a JSON number");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`indmsvol`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. Numeric fields tolerate a JSON
/// number or string via `string_or_number`; the by-time out-block is a repeated
/// array.
#[test]
fn t1621_success_body_deserializes_with_nondefault_field() {
    let as_number: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1621OutBlock": { "jisucd": "001", "jisunm": "KOSPI", "ex_upcode": "001" },
        "t1621OutBlock1": [
            { "date": "20260627", "time": "153000", "indmsvol": 12345, "indmsamt": -67890, "formsvol": -222, "formsamt": 333, "sysmsvol": 444, "sysmsamt": -555, "upclose": 2580 },
            { "date": "20260627", "time": "152900", "indmsvol": -10, "indmsamt": 20, "formsvol": 30, "formsamt": -40, "sysmsvol": -50, "sysmsamt": 60, "upclose": 2579 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1621OutBlock": { "jisucd": "001", "jisunm": "KOSPI", "ex_upcode": "001" },
        "t1621OutBlock1": [
            { "date": "20260627", "time": "153000", "indmsvol": "12345", "indmsamt": "-67890", "formsvol": "-222", "formsamt": "333", "sysmsvol": "444", "sysmsamt": "-555", "upclose": "2580" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "by-time array round-trips");
    assert_eq!(as_number.outblock.jisunm, "KOSPI", "header block round-trips");
    assert_eq!(as_number.outblock1[0].indmsvol, "12345", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].indmsamt, "-67890");
    assert_eq!(as_number.outblock1[0].indmsvol, as_string.outblock1[0].indmsvol);
    assert_eq!(as_number.outblock1[0].upclose, as_string.outblock1[0].upclose);
}

/// A single (non-array) `t1621OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1621_single_object_out_block_is_tolerated() {
    let resp: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1621OutBlock1": { "date": "20260627", "time": "153000", "indmsvol": 12345 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].indmsvol, "12345");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1621OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1621_empty_result_deserializes_to_defaults() {
    let empty: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1621 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.jisunm.is_empty(), "header block defaults to empty");
}

// ---- t2545 상품선물투자자매매동향 (closed-window more-flips; market_session, header + by-time array) ----

/// `t2545` serializes to
/// `{"t2545InBlock":{"eitem":"01","sgubun":"0","upcode":"001","nmin":0,"cnt":10,"bgubun":"0"}}`
/// — `eitem`/`sgubun`/`upcode`/`bgubun` stay strings, but `nmin` and `cnt` MUST
/// serialize as JSON NUMBERS (KTD3 — the string form returns IGW40011 at the
/// gateway). Non-paginated — no tr_cont tokens in the body.
#[test]
fn t2545_request_serializes_nmin_and_cnt_as_numbers() {
    let value = serde_json::to_value(T2545Request::new("01", "0", "001", "0", "10", "0"))
        .expect("serialize t2545 request");
    assert_eq!(value["t2545InBlock"]["eitem"], "01", "eitem stays a string");
    assert_eq!(value["t2545InBlock"]["sgubun"], "0", "sgubun stays a string");
    assert_eq!(value["t2545InBlock"]["upcode"], "001", "upcode stays a string");
    assert_eq!(value["t2545InBlock"]["bgubun"], "0", "bgubun stays a string");
    assert_eq!(
        value["t2545InBlock"]["nmin"], 0,
        "nmin serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert_eq!(
        value["t2545InBlock"]["cnt"], 10,
        "cnt serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert!(value["t2545InBlock"]["cnt"].is_number(), "cnt is a JSON number");
    assert!(value["t2545InBlock"]["nmin"].is_number(), "nmin is a JSON number");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`indmsvol`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. Numeric fields tolerate a JSON
/// number or string via `string_or_number`; the by-time out-block is a repeated
/// array.
#[test]
fn t2545_success_body_deserializes_with_nondefault_field() {
    let as_number: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2545OutBlock": { "eitem": "01", "sgubun": "0", "jisucd": "001", "jisunm": "KOSPI200" },
        "t2545OutBlock1": [
            { "date": "20260627", "time": "153000", "datetime": "20260627153000", "indmsvol": 12345, "indmsamt": -67890, "formsvol": -222, "formsamt": 333, "sysmsvol": 444, "sysmsamt": -555, "upclose": 358 },
            { "date": "20260627", "time": "152900", "datetime": "20260627152900", "indmsvol": -10, "indmsamt": 20, "formsvol": 30, "formsamt": -40, "sysmsvol": -50, "sysmsamt": 60, "upclose": 357 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2545OutBlock": { "eitem": "01", "sgubun": "0", "jisucd": "001", "jisunm": "KOSPI200" },
        "t2545OutBlock1": [
            { "date": "20260627", "time": "153000", "datetime": "20260627153000", "indmsvol": "12345", "indmsamt": "-67890", "formsvol": "-222", "formsamt": "333", "sysmsvol": "444", "sysmsamt": "-555", "upclose": "358" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "by-time array round-trips");
    assert_eq!(as_number.outblock.jisunm, "KOSPI200", "header block round-trips");
    assert_eq!(as_number.outblock1[0].indmsvol, "12345", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].indmsamt, "-67890");
    assert_eq!(as_number.outblock1[0].indmsvol, as_string.outblock1[0].indmsvol);
    assert_eq!(as_number.outblock1[0].upclose, as_string.outblock1[0].upclose);
}

/// A single (non-array) `t2545OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t2545_single_object_out_block_is_tolerated() {
    let resp: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2545OutBlock1": { "date": "20260627", "time": "153000", "indmsvol": 12345 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].indmsvol, "12345");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t2545OutBlock1`; recognized as the empty/pending case.
#[test]
fn t2545_empty_result_deserializes_to_defaults() {
    let empty: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t2545 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.jisunm.is_empty(), "header block defaults to empty");
}

// ---- t8406 주식선물틱분별체결조회 (closed-window more-flips; market_session, conclusion array) ----

/// `t8406` serializes to
/// `{"t8406InBlock":{"focode":"101TC000","cgubun":"1","bgubun":0,"cnt":10}}`
/// — `focode`/`cgubun` stay strings, but `bgubun` and `cnt` MUST serialize as
/// JSON NUMBERS (KTD3 — the string form returns IGW40011 at the gateway).
/// Non-paginated — no tr_cont tokens in the body.
#[test]
fn t8406_request_serializes_bgubun_and_cnt_as_numbers() {
    let value = serde_json::to_value(T8406Request::new("101TC000", "1", "0", "10"))
        .expect("serialize t8406 request");
    assert_eq!(value["t8406InBlock"]["focode"], "101TC000", "focode stays a string");
    assert_eq!(value["t8406InBlock"]["cgubun"], "1", "cgubun stays a string");
    assert_eq!(
        value["t8406InBlock"]["bgubun"], 0,
        "bgubun serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert_eq!(
        value["t8406InBlock"]["cnt"], 10,
        "cnt serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert!(value["t8406InBlock"]["bgubun"].is_number(), "bgubun is a JSON number");
    assert!(value["t8406InBlock"]["cnt"].is_number(), "cnt is a JSON number");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. Numeric fields tolerate a JSON
/// number or string via `string_or_number`; the conclusion out-block is a
/// repeated array. Body shape taken from the raw capture `res_example`.
#[test]
fn t8406_success_body_deserializes_with_nondefault_field() {
    let as_number: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8406OutBlock1": [
            { "chetime": "151949", "price": 70700, "sign": "5", "change": 500, "open": 0, "high": 0, "low": 0, "volume": 811347, "value": 570684700000i64, "openyak": 291595, "cvolume": 197 },
            { "chetime": "151947", "price": 70700, "sign": "5", "change": 500, "open": 0, "high": 0, "low": 0, "volume": 811150, "value": 570545421000i64, "openyak": 291595, "cvolume": 3 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8406OutBlock1": [
            { "chetime": "151949", "price": "70700", "sign": "5", "change": "500", "open": "0", "high": "0", "low": "0", "volume": "000000811347", "value": "570684700000", "openyak": "291595", "cvolume": "197" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "conclusion array round-trips");
    assert_eq!(as_number.outblock1[0].price, "70700", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].chetime, "151949");
    assert_eq!(as_number.outblock1[0].price, as_string.outblock1[0].price);
    assert_eq!(as_number.outblock1[0].openyak, as_string.outblock1[0].openyak);
}

/// A single (non-array) `t8406OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t8406_single_object_out_block_is_tolerated() {
    let resp: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8406OutBlock1": { "chetime": "151949", "price": 70700, "cvolume": 197 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "70700");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t8406OutBlock1`; recognized as the empty/pending case.
#[test]
fn t8406_empty_result_deserializes_to_defaults() {
    let empty: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t8406 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
}

// ---- t8407 API용주식멀티현재가조회 (closed-window more-flips; market_session, per-symbol array) ----

/// `t8407` serializes to
/// `{"t8407InBlock":{"nrec":3,"shcode":"005930000660001200"}}` — `shcode` stays a
/// (concatenated) String, but `nrec` MUST serialize as a JSON NUMBER (KTD3 — the
/// string form returns IGW40011 at the gateway). Non-paginated — no tr_cont tokens.
#[test]
fn t8407_request_serializes_nrec_as_number() {
    let value = serde_json::to_value(T8407Request::new("3", "005930000660001200"))
        .expect("serialize t8407 request");
    assert_eq!(
        value["t8407InBlock"]["shcode"], "005930000660001200",
        "shcode stays a concatenated string"
    );
    assert_eq!(
        value["t8407InBlock"]["nrec"], 3,
        "nrec serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert!(value["t8407InBlock"]["nrec"].is_number(), "nrec is a JSON number");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. Numeric fields tolerate a JSON number
/// or string via `string_or_number`; the per-symbol out-block is a repeated array.
#[test]
fn t8407_success_body_deserializes_with_nondefault_field() {
    let as_number: T8407Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8407OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "price": 58000, "sign": "2", "change": 100, "diff": 0.17, "volume": 12345678, "open": 57900, "high": 58200, "low": 57800 },
            { "shcode": "000660", "hname": "SK하이닉스", "price": 120000, "sign": "5", "change": -500, "diff": -0.41, "volume": 2345678, "open": 120500, "high": 121000, "low": 119500 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T8407Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8407OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "price": "58000", "sign": "2", "change": "100", "diff": "0.17", "volume": "12345678", "open": "57900", "high": "58200", "low": "57800" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "per-symbol array round-trips");
    assert_eq!(as_number.outblock1[0].shcode, "005930", "modeled key round-trips");
    assert_eq!(as_number.outblock1[0].price, "58000", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].price, as_string.outblock1[0].price);
    assert_eq!(as_number.outblock1[0].volume, as_string.outblock1[0].volume);
}

/// A single (non-array) `t8407OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t8407_single_object_out_block_is_tolerated() {
    let resp: T8407Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8407OutBlock1": { "shcode": "005930", "hname": "삼성전자", "price": 58000 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "58000");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t8407OutBlock1`; recognized as the empty/pending case.
#[test]
fn t8407_empty_result_deserializes_to_defaults() {
    let empty: T8407Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t8407 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
}

// ---- t1631 프로그램매매종합조회 (open-window domestic program-trade; market_session, two single-or-array out-blocks) ----

/// `t1631` serializes its five caller filters under the renamed `t1631InBlock` key
/// with no leaked fields; non-paginated — no tr_cont tokens.
#[test]
fn t1631_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1631Request::new("0", "0", "20260629", "20260629", "1"))
        .expect("serialize t1631 request");
    assert_eq!(value["t1631InBlock"]["gubun"], "0");
    assert_eq!(value["t1631InBlock"]["dgubun"], "0");
    assert_eq!(value["t1631InBlock"]["sdate"], "20260629");
    assert_eq!(value["t1631InBlock"]["edate"], "20260629");
    assert_eq!(value["t1631InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1631InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key field
/// (`bidvolume`) holds a real, non-default value. Numeric-bearing fields tolerate a
/// JSON number or string via `string_or_number`; both out-blocks are tolerant Vecs.
#[test]
fn t1631_success_body_deserializes_with_nondefault_field() {
    let as_string: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1631OutBlock": {
            "cdhrem": "000012345", "bdhrem": "000067890", "tcdrem": "000011111",
            "tbdrem": "000022222", "cshrem": "000033333", "bshrem": "000044444",
            "tcsrem": "000055555", "tbsrem": "000066666"
        },
        "t1631OutBlock1": {
            "offervolume": "000001234", "offervalue": "000000123456",
            "bidvolume": "000005678", "bidvalue": "000000567890",
            "volume": "000004444", "value": "000000444434"
        },
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1631OutBlock": { "cdhrem": 12345 },
        "t1631OutBlock1": { "bidvolume": 5678, "value": 444434i64 }
    }))
    .expect("number body must deserialize");
    assert_eq!(as_string.outblock.len(), 1, "remainder block → one-element Vec");
    assert_eq!(as_string.outblock[0].cdhrem, "000012345");
    assert_eq!(as_string.outblock1.len(), 1, "totals block → one-element Vec");
    assert_eq!(
        as_string.outblock1[0].bidvolume, "000005678",
        "modeled non-key field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock[0].cdhrem, "12345");
    assert_eq!(as_number.outblock1[0].bidvolume, "5678");
}

/// A single (non-array) out-block object is tolerated via `de_vec_or_single` — this
/// is exactly the spec's single-object shape for both t1631 blocks.
#[test]
fn t1631_single_object_out_blocks_are_tolerated() {
    let resp: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1631OutBlock": { "cdhrem": "000012345" },
        "t1631OutBlock1": { "bidvolume": "000005678" }
    }))
    .expect("single-object out-blocks must deserialize");
    assert_eq!(resp.outblock.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].bidvolume, "000005678");
}

/// An empty result (`00707`, no out-blocks) deserializes cleanly to empty Vecs.
#[test]
fn t1631_empty_result_deserializes_to_empty() {
    let empty: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1631 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty(), "no remainder block → empty Vec");
    assert!(empty.outblock1.is_empty(), "no totals block → empty Vec");
}

// ---- t1632 프로그램매매추이(시간) (open-window domestic program-trade; cursor + intraday-trend array) ----

/// `t1632` serializes its seven caller filters under the renamed `t1632InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1632_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1632Request::new(
        "0", "1", "0", "0", "20260629", "", "1",
    ))
    .expect("serialize t1632 request");
    assert_eq!(value["t1632InBlock"]["gubun"], "0");
    assert_eq!(value["t1632InBlock"]["gubun1"], "1");
    assert_eq!(value["t1632InBlock"]["date"], "20260629");
    assert_eq!(value["t1632InBlock"]["time"], "");
    assert_eq!(value["t1632InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1632InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`k200jisu`) holds a real, non-default value. Numeric-bearing fields tolerate a
/// JSON number or string; the time-series out-block is a repeated array.
#[test]
fn t1632_success_body_deserializes_with_nondefault_field() {
    let as_string: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1632OutBlock": { "date": "20260629", "time": "153000", "idx": "100", "ex_gubun": "1" },
        "t1632OutBlock1": [
            { "time": "09000000", "k200jisu": "00000350.25", "sign": "2", "change": "00000001.10",
              "tot3": "000012345678", "tot1": "000023456789", "tot2": "000011111111", "cha3": "000001234567" },
            { "time": "09010000", "k200jisu": "00000351.00", "sign": "2", "change": "00000001.85",
              "tot3": "000013345678", "tot1": "000024456789", "tot2": "000011111111", "cha3": "000001334567" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1632OutBlock1": [
            { "k200jisu": 350.25, "change": 1.10, "tot3": 12345678i64 }
        ]
    }))
    .expect("number body (k200jisu as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.date, "20260629", "cursor round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "time-series array round-trips");
    assert_eq!(
        as_string.outblock1[0].k200jisu, "00000350.25",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].k200jisu, "350.25");
    assert_eq!(as_number.outblock1[0].change, "1.1");
}

/// A single (non-array) `t1632OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1632_single_object_out_block_is_tolerated() {
    let resp: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1632OutBlock1": { "time": "09000000", "k200jisu": "00000350.25" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].k200jisu, "00000350.25");
}

/// An empty result (`00707`) deserializes cleanly — default cursor, empty rows.
#[test]
fn t1632_empty_result_deserializes_to_empty() {
    let empty: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1632 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.date.is_empty(), "no cursor → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1633 프로그램매매추이(일별) (open-window domestic program-trade; cursor + daily-trend array) ----

/// `t1633` serializes its nine caller filters under the renamed `t1633InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1633_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1633Request::new(
        "0", "1", "0", "0", "20260601", "20260629", "0", "20260629", "1",
    ))
    .expect("serialize t1633 request");
    assert_eq!(value["t1633InBlock"]["gubun"], "0");
    assert_eq!(value["t1633InBlock"]["fdate"], "20260601");
    assert_eq!(value["t1633InBlock"]["tdate"], "20260629");
    assert_eq!(value["t1633InBlock"]["gubun4"], "0");
    assert_eq!(value["t1633InBlock"]["date"], "20260629");
    assert_eq!(value["t1633InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1633InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`jisu`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the daily-series out-block is a repeated array.
#[test]
fn t1633_success_body_deserializes_with_nondefault_field() {
    let as_string: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1633OutBlock": { "date": "20260629", "idx": "30" },
        "t1633OutBlock1": [
            { "date": "20260627", "jisu": "00000349.10", "sign": "5", "change": "-0000000.40",
              "tot3": "000012345678", "cha3": "000001234567", "bcha3": "000011111111", "volume": "000099999999" },
            { "date": "20260629", "jisu": "00000350.25", "sign": "2", "change": "00000001.15",
              "tot3": "000013345678", "cha3": "000001334567", "bcha3": "000012011111", "volume": "000088888888" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1633OutBlock1": [
            { "jisu": 350.25, "change": 1.15, "volume": 88888888i64 }
        ]
    }))
    .expect("number body (jisu as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.date, "20260629", "cursor round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "daily-series array round-trips");
    assert_eq!(
        as_string.outblock1[0].jisu, "00000349.10",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].jisu, "350.25");
    assert_eq!(as_number.outblock1[0].volume, "88888888");
}

/// A single (non-array) `t1633OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1633_single_object_out_block_is_tolerated() {
    let resp: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1633OutBlock1": { "date": "20260629", "jisu": "00000350.25" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].jisu, "00000350.25");
}

/// An empty result (`00707`) deserializes cleanly — default cursor, empty rows.
#[test]
fn t1633_empty_result_deserializes_to_empty() {
    let empty: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1633 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.date.is_empty(), "no cursor → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1959 LP대상종목정보조회 (closed-window more-flips; market_session ELW, per-issue array) ----

/// `t1959` serializes to `{"t1959InBlock":{"shcode":""}}` — `::new()` defaults
/// `shcode` to the empty string (the full LP-target list). No numeric request slot,
/// so no number coercion; non-paginated — no tr_cont tokens.
#[test]
fn t1959_new_serializes_empty_shcode_under_inblock() {
    let value = serde_json::to_value(T1959Request::new()).expect("serialize t1959 request");
    assert_eq!(
        value["t1959InBlock"]["shcode"], "",
        "::new() defaults shcode to the empty string (full LP-target list)"
    );
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    // for_shcode narrows to one issue, still under the renamed in-block key.
    let one = serde_json::to_value(T1959Request::for_shcode("000250")).expect("serialize");
    assert_eq!(one["t1959InBlock"]["shcode"], "000250");
    assert!(one["t1959InBlock"].get("nrec").is_none(), "no leaked caller fields");
}

/// A representative success body (the spec res_example: 삼천당제약 000250 / 이녹스
/// 088390) deserializes AND a modeled non-key field (`price`) holds a real,
/// non-default value — proving the subset round-trips, not just `serde(default)`
/// returning `Ok`. Numeric-bearing fields (incl. the spec-`Number` `rate`) tolerate
/// a JSON number or string via `string_or_number`; the per-issue out-block is a
/// repeated array.
#[test]
fn t1959_success_body_deserializes_with_nondefault_field() {
    let as_string: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1959OutBlock1": [
            { "shcode": "000250", "hname": "삼천당제약", "price": "000000061900", "sign": "5", "change": "-00000000200", "rate": "-0.32", "volume": "000000097361", "value": "006010435800", "lp_gb": "가능" },
            { "shcode": "088390", "hname": "이녹스", "price": "000000035950", "sign": "5", "change": "-00000000150", "rate": "-0.42", "volume": "000000019443", "value": "000704468650", "lp_gb": "가능" }
        ],
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1959OutBlock1": [
            { "shcode": "000250", "hname": "삼천당제약", "price": 61900, "sign": "5", "change": -200, "rate": -0.32, "volume": 97361, "value": 6010435800i64, "lp_gb": "가능" }
        ]
    }))
    .expect("number body (rate as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock1.len(), 2, "per-issue array round-trips");
    assert_eq!(as_string.outblock1[0].shcode, "000250", "modeled key round-trips");
    assert_eq!(
        as_string.outblock1[0].hname, "삼천당제약",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock1[0].lp_gb, "가능");
    // `rate` is a spec Number — tolerated from BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].rate, "-0.32");
    assert_eq!(as_number.outblock1[0].shcode, "000250");
}

/// A single (non-array) `t1959OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1959_single_object_out_block_is_tolerated() {
    let resp: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1959OutBlock1": { "shcode": "000250", "hname": "삼천당제약", "price": "000000061900" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].shcode, "000250");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1959OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1959_empty_result_deserializes_to_defaults() {
    let empty: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1959 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
}

// ---- t1950 ELW현재가(시세)조회 (closed-window more-flips; market_session ELW single-instrument quote + basket array) ----

/// `t1950` serializes to `{"t1950InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1950_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1950Request::new("520012")).expect("serialize t1950 request");
    assert_eq!(value["t1950InBlock"]["shcode"], "520012");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1950InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1950Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1950InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. Numeric-bearing fields tolerate a JSON number
/// or string via `string_or_number`; the main quote is ONE object, the basket is a
/// repeated array.
#[test]
fn t1950_success_body_deserializes_with_nondefault_field() {
    let as_string: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1950OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": "000000000150", "sign": "5",
            "change": "-00000000010", "diff": "-6.25", "volume": "000000123456",
            "value": "000001234567", "bcode": "005930", "bprice": "000000061900"
        },
        "t1950OutBlock1": [
            { "bskcode": "005930", "bskbno": "100", "bskprice": "000000061900" }
        ],
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1950OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": 150, "sign": "5", "change": -10,
            "diff": -6.25, "volume": 123456, "value": 1234567i64, "bcode": "005930",
            "bprice": 61900
        }
    }))
    .expect("number body (numeric fields as JSON Numbers) must deserialize");
    assert_eq!(
        as_string.outblock.hname, "KBJ05삼성전자콜",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock.price, "000000000150");
    assert_eq!(as_string.outblock1.len(), 1, "basket array round-trips");
    assert_eq!(as_string.outblock1[0].bskcode, "005930");
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.price, "150");
    assert_eq!(as_number.outblock.diff, "-6.25");
    assert_eq!(as_number.outblock.hname, "KBJ05삼성전자콜");
}

/// A single (non-array) `t1950OutBlock1` basket object is tolerated via
/// `de_vec_or_single`.
#[test]
fn t1950_single_basket_out_block_is_tolerated() {
    let resp: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1950OutBlock": { "hname": "KBJ05삼성전자콜", "price": "000000000150" },
        "t1950OutBlock1": { "bskcode": "005930", "bskbno": "100", "bskprice": "000000061900" }
    }))
    .expect("single-object basket out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].bskcode, "005930");
}

/// An empty result (`00707`, no out-blocks) deserializes cleanly — the main quote
/// is its `Default`, the basket array is empty; recognized as the empty/pending
/// case.
#[test]
fn t1950_empty_result_deserializes_to_defaults() {
    let empty: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1950 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no quote → default object");
    assert!(empty.outblock1.is_empty(), "no basket → empty Vec");
}

// ---- t1971 ELW현재가호가조회 (closed-window more-flips; market_session ELW current-price + quote board, single object) ----

/// `t1971` serializes to `{"t1971InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1971_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1971Request::new("520012")).expect("serialize t1971 request");
    assert_eq!(value["t1971InBlock"]["shcode"], "520012");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1971InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1971Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1971InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. Numeric-bearing fields tolerate a JSON number
/// or string via `string_or_number`; the quote-board is ONE object (no array
/// secondary block per the baseline).
#[test]
fn t1971_success_body_deserializes_with_nondefault_field() {
    let as_string: T1971Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1971OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": "000000000150", "sign": "5",
            "change": "-00000000010", "diff": "-6.25", "volume": "000000123456",
            "offerho1": "000000000155", "bidho1": "000000000150",
            "offerrem1": "000000010000", "bidrem1": "000000020000",
            "open": "000000000160", "high": "000000000170", "low": "000000000145",
            "invidx": "1", "koba_stdprc": "000000080000", "koba_acc_rt": "12.50",
            "koba_yn": "N"
        },
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1971Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1971OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": 150, "sign": "5", "change": -10,
            "diff": -6.25, "volume": 123456, "offerho1": 155, "bidho1": 150,
            "offerrem1": 10000, "bidrem1": 20000, "open": 160, "high": 170,
            "low": 145, "invidx": 1, "koba_stdprc": 80000, "koba_acc_rt": 12.5,
            "koba_yn": "N"
        }
    }))
    .expect("number body (numeric fields as JSON Numbers) must deserialize");
    assert_eq!(
        as_string.outblock.hname, "KBJ05삼성전자콜",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock.price, "000000000150");
    assert_eq!(as_string.outblock.offerho1, "000000000155");
    assert_eq!(as_string.outblock.koba_yn, "N");
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.price, "150");
    assert_eq!(as_number.outblock.diff, "-6.25");
    assert_eq!(as_number.outblock.bidho1, "150");
    assert_eq!(as_number.outblock.hname, "KBJ05삼성전자콜");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the quote-board
/// is its `Default`; recognized as the empty/pending case.
#[test]
fn t1971_empty_result_deserializes_to_defaults() {
    let empty: T1971Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1971 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.hname.is_empty(),
        "no quote → default object"
    );
}

// ---- t1972 ELW현재가(거래원)조회 (closed-window more-flips; market_session ELW current-price + trading-member board, single object) ----

/// `t1972` serializes to `{"t1972InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1972_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1972Request::new("52HAAM")).expect("serialize t1972 request");
    assert_eq!(value["t1972InBlock"]["shcode"], "52HAAM");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1972InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1972Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1972InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. Numeric-bearing fields tolerate a JSON number
/// or string via `string_or_number` (the gateway sends the ratios as strings and
/// the volumes/increments as JSON numbers); the board is ONE object (no array
/// secondary block per the baseline).
#[test]
fn t1972_success_body_deserializes_with_nondefault_field() {
    let as_string: T1972Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1972OutBlock": {
            "hname": "미래HAAM네이버콜", "expcode": "KRA521138CB0", "shcode": "J52HAAM",
            "offerno1": "미래에", "bidno1": "미래에",
            "dvol1": "2820", "svol1": "2130", "dcha1": "0", "scha1": "0",
            "ddiff1": "99.65", "sdiff1": "75.27",
            "fwdvl": "0", "fwsvl": "0", "fwddiff": "0.00", "fwsdiff": "0.00"
        },
        "rsp_msg": "정상적으로 조회가 완료되었습니다."
    }))
    .expect("string body must deserialize");
    let as_number: T1972Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1972OutBlock": {
            "hname": "미래HAAM네이버콜", "expcode": "KRA521138CB0", "shcode": "J52HAAM",
            "offerno1": "미래에", "bidno1": "미래에",
            "dvol1": 2820, "svol1": 2130, "dcha1": 0, "scha1": 0,
            "ddiff1": "99.65", "sdiff1": "75.27",
            "fwdvl": 0, "fwsvl": 0, "fwddiff": "0.00", "fwsdiff": "0.00"
        }
    }))
    .expect("number body (volume fields as JSON Numbers) must deserialize");
    assert_eq!(
        as_string.outblock.hname, "미래HAAM네이버콜",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock.shcode, "J52HAAM");
    assert_eq!(as_string.outblock.offerno1, "미래에");
    assert_eq!(as_string.outblock.ddiff1, "99.65");
    assert_eq!(as_string.outblock.dvol1, "2820");
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.dvol1, "2820");
    assert_eq!(as_number.outblock.svol1, "2130");
    assert_eq!(as_number.outblock.fwdvl, "0");
    assert_eq!(as_number.outblock.hname, "미래HAAM네이버콜");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the board is its
/// `Default`; recognized as the empty/pending case.
#[test]
fn t1972_empty_result_deserializes_to_defaults() {
    let empty: T1972Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1972 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.hname.is_empty(),
        "no board → default object"
    );
}

// ---- t1974 ELW기초자산동일종목 (closed-window more-flips; market_session ELWs sharing a base asset, cnt summary + per-issue array) ----

/// `t1974` serializes to `{"t1974InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1974_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1974Request::new("52HAAM")).expect("serialize t1974 request");
    assert_eq!(value["t1974InBlock"]["shcode"], "52HAAM");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1974InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1974Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1974InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key array field
/// (`outblock1[0].hname`) holds a real, non-default value — proving the subset
/// round-trips, not just `serde(default)` returning `Ok`. Numeric-bearing fields
/// tolerate a JSON number or string via `string_or_number` (the gateway sends
/// `price`/`change` as JSON numbers and `volume`/`diff` as strings). The body
/// mirrors the captured gateway example: a `t1974OutBlock` summary (`cnt`) plus the
/// `t1974OutBlock1` sibling-issue array.
#[test]
fn t1974_success_body_deserializes_with_nondefault_field() {
    let body: T1974Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1974OutBlock": { "cnt": 76 },
        "t1974OutBlock1": [
            { "volume": "000000002830", "price": 250, "shcode": "52HAAM",
              "change": 0, "sign": "3", "cpgubun": "01", "diff": "0.00",
              "hname": "미래HAAM네이버콜" },
            { "volume": "000000000000", "price": 15, "shcode": "52HALF",
              "change": 0, "sign": "3", "cpgubun": "02", "diff": "0.00",
              "hname": "미래HALF네이버풋" }
        ],
        "rsp_msg": "정상적으로 조회가 완료되었습니다."
    }))
    .expect("success body must deserialize");
    assert_eq!(body.outblock.cnt, "76", "summary count round-trips (JSON number)");
    assert_eq!(body.outblock1.len(), 2, "two sibling issues");
    assert_eq!(
        body.outblock1[0].hname, "미래HAAM네이버콜",
        "modeled non-key array field is non-default"
    );
    assert_eq!(body.outblock1[0].shcode, "52HAAM");
    assert_eq!(body.outblock1[0].cpgubun, "01");
    // numeric-bearing fields tolerate BOTH a JSON number (price) and a string (volume).
    assert_eq!(body.outblock1[0].price, "250");
    assert_eq!(body.outblock1[0].volume, "000000002830");
    assert_eq!(body.outblock1[1].hname, "미래HALF네이버풋");
}

/// The `t1974OutBlock1` array tolerates a LONE object (not just a list) via
/// `de_vec_or_single` — a single-sibling base asset still deserializes into a
/// one-element Vec.
#[test]
fn t1974_single_object_array_deserializes_as_vec() {
    let body: T1974Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1974OutBlock": { "cnt": "1" },
        "t1974OutBlock1": {
            "volume": "5", "price": "100", "shcode": "52SOLO",
            "change": "0", "sign": "3", "cpgubun": "01", "diff": "0.00",
            "hname": "단독종목콜"
        }
    }))
    .expect("a lone t1974OutBlock1 object must deserialize as a one-element Vec");
    assert_eq!(body.outblock1.len(), 1, "lone object → one-element Vec");
    assert_eq!(body.outblock1[0].shcode, "52SOLO");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the blocks take
/// their `Default`; recognized as the empty/pending case.
#[test]
fn t1974_empty_result_deserializes_to_defaults() {
    let empty: T1974Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1974 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no siblings → empty Vec");
    assert!(empty.outblock.cnt.is_empty(), "no summary → default cnt");
}

// ---- t1956 ELW현재가(확정지급액)조회 (closed-window more-flips; market_session ELW current-price/payout snapshot, single OutBlock + basket array) ----

/// `t1956` serializes to `{"t1956InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1956_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1956Request::new("52HAAM")).expect("serialize t1956 request");
    assert_eq!(value["t1956InBlock"]["shcode"], "52HAAM");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1956InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1956Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1956InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`outblock.hname`, the NAME witness) holds a real, non-default value — proving the
/// subset round-trips, not just `serde(default)` returning `Ok`. Numeric-bearing
/// fields tolerate a JSON number or string via `string_or_number` (the gateway sends
/// `price`/`givemoney` as JSON numbers and `volume`/`diff` as strings). The body
/// mirrors the captured gateway example: a single `t1956OutBlock` snapshot plus the
/// `t1956OutBlock1` basket array.
#[test]
fn t1956_success_body_deserializes_with_nondefault_field() {
    let body: T1956Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1956OutBlock": {
            "hname": "미래HAAM네이버콜", "price": 250, "diff": "1.23",
            "volume": "000000002830", "elwexec": 35000, "impv": "0.18",
            "delt": "0.42", "bcode": "035420", "givemoney": 0
        },
        "t1956OutBlock1": [
            { "bskcode": "035420", "bskbno": "1", "bskprice": 198500, "bskvolume": "120000" }
        ],
        "rsp_msg": "정상적으로 조회가 완료되었습니다."
    }))
    .expect("success body must deserialize");
    assert_eq!(
        body.outblock.hname, "미래HAAM네이버콜",
        "modeled non-key NAME field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number (price) and a string (volume).
    assert_eq!(body.outblock.price, "250");
    assert_eq!(body.outblock.volume, "000000002830");
    assert_eq!(body.outblock.elwexec, "35000");
    assert_eq!(body.outblock.bcode, "035420");
    assert_eq!(body.outblock1.len(), 1, "one basket constituent");
    assert_eq!(body.outblock1[0].bskcode, "035420");
    assert_eq!(body.outblock1[0].bskprice, "198500");
}

/// The `t1956OutBlock1` basket array tolerates a LONE object (not just a list) via
/// `de_vec_or_single` — a single-constituent basket still deserializes into a
/// one-element Vec.
#[test]
fn t1956_single_object_array_deserializes_as_vec() {
    let body: T1956Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1956OutBlock": { "hname": "단독종목콜", "price": "100" },
        "t1956OutBlock1": {
            "bskcode": "005930", "bskbno": "1", "bskprice": "70000", "bskvolume": "5"
        }
    }))
    .expect("a lone t1956OutBlock1 object must deserialize as a one-element Vec");
    assert_eq!(body.outblock1.len(), 1, "lone object → one-element Vec");
    assert_eq!(body.outblock1[0].bskcode, "005930");
    assert_eq!(body.outblock.hname, "단독종목콜");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the blocks take
/// their `Default`; recognized as the empty/pending case.
#[test]
fn t1956_empty_result_deserializes_to_defaults() {
    let empty: T1956Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1956 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no basket → empty Vec");
    assert!(empty.outblock.hname.is_empty(), "no snapshot → default hname");
}

// ---- t1969 ELW지표검색 (closed-window more-flips; market_session ELW screener, summary + per-issue array) ----

/// `t1969::new()` serializes the all-ELWs screen under `t1969InBlock`. The numeric
/// range bounds MUST serialize as JSON NUMBERS (not strings) — the string form
/// returns `IGW40011` at the gateway. The chk*/cb*/date filters stay JSON strings.
/// Non-paginated — no tr_cont tokens, no leaked caller fields.
#[test]
fn t1969_new_serializes_numeric_ranges_as_json_numbers() {
    let value = serde_json::to_value(T1969Request::new()).expect("serialize t1969 request");
    let inblock = &value["t1969InBlock"];
    // Numeric range bounds → JSON numbers.
    for key in [
        "elwexecs", "elwexece", "volumes", "volumee", "rates", "ratee", "premiums", "premiume",
        "paritys", "paritye", "berates", "beratee", "capts", "capte", "egearings", "egearinge",
        "gearings", "gearinge", "deltas", "deltae", "thetas", "thetae",
    ] {
        assert!(
            inblock[key].is_number(),
            "{key} must serialize as a JSON number (string form → IGW40011)"
        );
        assert_eq!(inblock[key], 0, "{key} defaults to 0 in the all-ELWs screen");
    }
    // chk*/cb*/date filters stay JSON strings.
    assert_eq!(inblock["chkitem"], "0");
    assert_eq!(inblock["cbitem"], "000000000000");
    assert_eq!(inblock["duedates"], "000000");
    assert_eq!(inblock["duedatee"], "999999");
    assert_eq!(inblock["cbexec"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. The summary `t1969OutBlock.cnt` and the repeated
/// `t1969OutBlock1` array both round-trip; numeric-bearing fields tolerate a JSON
/// number or string via `string_or_number`.
#[test]
fn t1969_success_body_deserializes_with_nondefault_field() {
    let as_string: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1969OutBlock": { "cnt": "2" },
        "t1969OutBlock1": [
            { "hname": "한국SK001콜", "shcode": "5XX001", "issuernmk": "SK증권", "itemcode": "000000000001", "cpgubun": "01", "price": "000000000050", "sign": "5", "change": "-00000000005", "diff": "-9.09", "volume": "000000100000", "elwexec": "000000030000", "item": "KOSPI200", "lastdate": "20260619", "lpname": "SK증권" },
            { "hname": "미래001풋", "shcode": "5YY002", "issuernmk": "미래에셋", "itemcode": "000000000002", "cpgubun": "02", "price": "000000000035", "sign": "2", "change": "00000000003", "diff": "9.38", "volume": "000000050000", "elwexec": "000000031000", "item": "KOSPI200", "lastdate": "20260619", "lpname": "미래에셋" }
        ],
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1969OutBlock": { "cnt": 1 },
        "t1969OutBlock1": [
            { "hname": "한국SK001콜", "shcode": "5XX001", "price": 50, "change": -5, "diff": -9.09, "volume": 100000, "elwexec": 30000 }
        ]
    }))
    .expect("number body (numeric fields as JSON Numbers) must deserialize");
    assert_eq!(as_string.outblock.cnt, "2", "summary count round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "per-issue array round-trips");
    assert_eq!(as_string.outblock1[0].shcode, "5XX001", "modeled key round-trips");
    assert_eq!(
        as_string.outblock1[0].hname, "한국SK001콜",
        "modeled non-key field is non-default"
    );
    // Numeric out-block fields tolerated from BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.cnt, "1");
    assert_eq!(as_number.outblock1[0].price, "50");
    assert_eq!(as_number.outblock1[0].diff, "-9.09");
}

/// A single (non-array) `t1969OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1969_single_object_out_block_is_tolerated() {
    let resp: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1969OutBlock": { "cnt": "1" },
        "t1969OutBlock1": { "hname": "한국SK001콜", "shcode": "5XX001", "price": "000000000050" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].shcode, "5XX001");
}

/// An empty result (`00707`, no out-blocks) deserializes cleanly — no panic on a
/// missing `t1969OutBlock`/`t1969OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1969_empty_result_deserializes_to_defaults() {
    let empty: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1969 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert_eq!(empty.outblock.cnt, "", "absent summary → default");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
}

// ---- t1105 피봇/디마크 + t1104 현재가시세메모 (plan -002 Track 2) ----

#[test]
fn t1105_request_serializes_to_inblock() {
    let value =
        serde_json::to_value(T1105Request::new("005930", "K")).expect("serialize t1105 request");
    assert_eq!(value["t1105InBlock"]["shcode"], "005930");
    assert_eq!(value["t1105InBlock"]["exchgubun"], "K");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

#[test]
fn t1105_pbot_number_or_string_yields_same_value() {
    let as_number: T1105Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1105OutBlock": { "shcode": "005930", "pbot": 357666 }
    }))
    .expect("number pbot must deserialize");
    let as_string: T1105Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1105OutBlock": { "shcode": "005930", "pbot": "357666" }
    }))
    .expect("string pbot must deserialize");
    assert_eq!(as_number.outblock.pbot, "357666");
    assert_eq!(as_number.outblock.pbot, as_string.outblock.pbot);
}

#[test]
fn t1105_empty_result_deserializes_to_defaults() {
    let empty: T1105Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("empty t1105 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.pbot.is_empty());
}

#[test]
fn t1104_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1104Request::new("005930", "1", "K"))
        .expect("serialize t1104 request");
    assert_eq!(value["t1104InBlock"]["code"], "005930");
    assert_eq!(value["t1104InBlock"]["nrec"], "1");
    assert_eq!(value["t1104InBlock"]["exchgubun"], "K");
}

#[test]
fn t1104_outblock1_array_and_numeric_tolerance_deserialize() {
    // The memo-row array round-trips; `indx` tolerates number or string.
    let resp: T1104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1104OutBlock": { "nrec": "1" },
        "t1104OutBlock1": [ { "indx": 1, "gubn": "1", "vals": "135155" } ]
    }))
    .expect("t1104 array response must deserialize");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].indx, "1");
    // A single-object OutBlock1 is tolerated as a one-element vec.
    let single: T1104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1104OutBlock1": { "indx": "0", "gubn": "1", "vals": "x" }
    }))
    .expect("single-object t1104OutBlock1 must deserialize");
    assert_eq!(single.outblock1.len(), 1);
}

#[test]
fn t1104_empty_result_deserializes_to_defaults() {
    let empty: T1104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "자료없음"
    }))
    .expect("empty t1104 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty());
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
        .stock_futures_underlying_master(&T2522Request::new())
        .await
        .expect("t2522 stock_futures_underlying_master should succeed");
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

// ---------------------------------------------------------------------------
// t8433 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input. A single ROW-ARRAY out-block `t8433OutBlock`
// (confirmed from the raw capture's `res_example`: rows are direct elements
// under the key, no separate count header / no numbered sub-block): one
// index-option contract per row. The wire out-block key is the literal
// `t8433OutBlock` — the normalized baseline collapses it to `response_body`, so
// the rename was taken from the raw capture, not the baseline.
// ---------------------------------------------------------------------------

const T8433_FIXTURE: &str = include_str!("fixtures/t8433_resp.json");

/// Covers R4. `t8433` serializes to exactly `{"t8433InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t8433_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8433Request::new()).expect("serialize t8433 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8433InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t8433InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the row array round-trips and the canonical identity field `hname`
/// (종목명, the index-option contract name) holds its EXACT value. The fixture's
/// neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode`/a price field instead would surface here.
#[tokio::test]
async fn t8433_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8433"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8433_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_option_master(&T8433Request::new())
        .await
        .expect("t8433 index_option_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "two index-option master rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "C 2307 185.0",
        "종목명 index-option contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "201T7185", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4201T71852", "확장코드 (distinct)");
    assert_eq!(row.hprice, "175.80", "상한가 (distinct)");
    assert_eq!(row.lprice, "102.90", "하한가 (distinct)");
    assert_eq!(row.jnilclose, "127.95", "전일종가 (distinct)");
    assert_eq!(row.jnilhigh, "131.40", "전일고가 (distinct)");
    assert_eq!(row.jnillow, "124.10", "전일저가 (distinct)");
    // recprice (기준가) is distinct from jnilclose — a 기준가/전일종가 mislabel surfaces.
    assert_eq!(row.recprice, "127.90", "기준가 (distinct from 전일종가)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "C 2406 330.0", "second row distinct");
}

/// Covers shared contract item 2. `hprice` (상한가) parses via
/// `ls_core::string_or_number` from BOTH a string and a JSON number — the gateway
/// may send a numeric-looking price either way.
#[test]
fn t8433_hprice_number_or_string_yields_same_value() {
    // Use a value whose JSON-number form and string form normalize identically
    // (no trailing-zero divergence), so the two forms cross-assert equal — the
    // same round-trip guarantee the sibling TRs' number-or-string tests prove.
    let as_number: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8433OutBlock": [{ "hname": "C 2307 185.0", "hprice": 175.5 }]
    }))
    .expect("numeric hprice deserializes");
    let as_string: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8433OutBlock": [{ "hname": "C 2307 185.0", "hprice": "175.5" }]
    }))
    .expect("string hprice deserializes");
    assert_eq!(as_number.outblock[0].hprice, "175.5");
    assert_eq!(
        as_number.outblock[0].hprice, as_string.outblock[0].hprice,
        "both wire forms normalize to the same string"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8433OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8433_single_object_row_deserializes_to_one_element_vec() {
    let single: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8433OutBlock": { "hname": "C 2307 185.0" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "C 2307 185.0");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8433OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8433` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8433_empty_result_deserializes_as_pending() {
    let empty: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// t8435 — 파생종목마스터조회API용 (derivatives master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"MF"` futures / `"MO"`
// options). The out-block is itself a ROW ARRAY (confirmed from the raw
// capture's `res_example`, KTD3) — one derivatives contract per row, the full
// 9 fields.
// ---------------------------------------------------------------------------

const T8435_FIXTURE: &str = include_str!("fixtures/t8435_resp.json");

/// Covers R4. `t8435` serializes to exactly `{"t8435InBlock":{"gubun":"MF"}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t8435_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8435Request::new("MF")).expect("serialize t8435 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8435InBlock"]["gubun"], "MF", "gubun selector serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the derivatives contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`recprice` instead would surface here.
#[tokio::test]
async fn t8435_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8435"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8435_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .derivatives_master(&T8435Request::new("MF"))
        .await
        .expect("t8435 derivatives_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two derivatives rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "KQF 2306",
        "종목명 derivatives contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "106T6000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4106T60005", "확장코드 (distinct)");
    assert_eq!(row.uplmtprice, "1456.5", "상한가 (distinct)");
    assert_eq!(row.dnlmtprice, "1240.9", "하한가 (distinct)");
    assert_eq!(row.jnilclose, "1348.7", "전일종가 (distinct)");
    assert_eq!(row.jnilhigh, "1349.8", "전일고가 (distinct)");
    assert_eq!(row.jnillow, "1323.9", "전일저가 (distinct)");
    // recprice (기준가) is distinct from jnilclose — a 기준가/전일종가 mislabel surfaces.
    assert_eq!(row.recprice, "1348.6", "기준가 (distinct from 전일종가)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "KQF 2309", "second row distinct");
}

/// Covers shared contract item 2. `uplmtprice` (상한가) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// types it `Number` and may send either form.
#[test]
fn t8435_numeric_number_or_string_yields_same_value() {
    let as_number: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8435OutBlock": [{ "hname": "KQF 2306", "uplmtprice": 1456.5 }]
    }))
    .expect("numeric uplmtprice deserializes");
    let as_string: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8435OutBlock": [{ "hname": "KQF 2306", "uplmtprice": "1456.5" }]
    }))
    .expect("string uplmtprice deserializes");
    assert_eq!(as_number.outblock[0].uplmtprice, "1456.5");
    assert_eq!(as_string.outblock[0].uplmtprice, "1456.5");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8435OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8435_single_object_row_deserializes_to_one_element_vec() {
    let single: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8435OutBlock": { "hname": "KQF 2306" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "KQF 2306");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8435OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8435` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8435_empty_result_deserializes_as_pending() {
    let empty: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// t8467 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"`/`"S"`/`"Q"` or any
// other value → KOSPI200). The out-block is itself a ROW ARRAY (confirmed from
// the raw capture's `res_example`, propertyType `A0005`/Object Array, KTD3) —
// one index-futures contract per row, the full 9 fields.
// ---------------------------------------------------------------------------

const T8467_FIXTURE: &str = include_str!("fixtures/t8467_resp.json");

/// Covers R4. `t8467` serializes to exactly `{"t8467InBlock":{"gubun":"Q"}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t8467_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8467Request::new("Q")).expect("serialize t8467 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8467InBlock"]["gubun"], "Q", "gubun selector serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the index-futures contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`recprice`/`jnilclose` instead would surface here.
#[tokio::test]
async fn t8467_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8467"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8467_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index_futures_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two index-futures rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "F 2606",
        "종목명 index-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "A0166000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4A01660005", "확장코드 (distinct)");
    assert_eq!(row.uplmtprice, "1214.75", "상한가 (distinct)");
    assert_eq!(row.dnlmtprice, "1034.85", "하한가 (distinct)");
    assert_eq!(row.jnilclose, "1124.80", "전일종가 (distinct)");
    assert_eq!(row.jnilhigh, "1125.65", "전일고가 (distinct)");
    assert_eq!(row.jnillow, "1124.55", "전일저가 (distinct)");
    // recprice (기준가) is distinct from jnilclose — a 기준가/전일종가 mislabel surfaces.
    assert_eq!(row.recprice, "1124.70", "기준가 (distinct from 전일종가)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "F 2609", "second row distinct");
}

/// Covers shared contract item 2. `uplmtprice` (상한가) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// types it `Number` and may send either form.
#[test]
fn t8467_numeric_number_or_string_yields_same_value() {
    let as_number: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8467OutBlock": [{ "hname": "F 2606", "uplmtprice": 1214.75 }]
    }))
    .expect("numeric uplmtprice deserializes");
    let as_string: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8467OutBlock": [{ "hname": "F 2606", "uplmtprice": "1214.75" }]
    }))
    .expect("string uplmtprice deserializes");
    assert_eq!(as_number.outblock[0].uplmtprice, "1214.75");
    assert_eq!(as_string.outblock[0].uplmtprice, "1214.75");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8467OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8467_single_object_row_deserializes_to_one_element_vec() {
    let single: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8467OutBlock": { "hname": "F 2606" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "F 2606");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8467OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8467` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8467_empty_result_deserializes_as_pending() {
    let empty: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// t9943 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"`/`"S"` or any other
// value → KOSPI200). The out-block is itself a ROW ARRAY (confirmed from the raw
// capture's `res_example`, propertyType `A0005`/Object Array, the true wire key
// `t9943OutBlock` per KTD3) — one index-futures contract per row, the 3 spec
// fields (hname/shcode/expcode).
// ---------------------------------------------------------------------------

const T9943_FIXTURE: &str = include_str!("fixtures/t9943_resp.json");

/// Covers R4. `t9943` serializes to exactly `{"t9943InBlock":{"gubun":"V"}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t9943_request_serializes_to_inblock() {
    let value = serde_json::to_value(T9943Request::new("V")).expect("serialize t9943 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t9943InBlock"]["gubun"], "V", "gubun selector serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the index-futures contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode` instead would surface here.
#[tokio::test]
async fn t9943_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t9943"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T9943_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_futures_master_codes(&T9943Request::new("V"))
        .await
        .expect("t9943 index_futures_master_codes should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two index-futures rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "VF 2306",
        "종목명 index-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "104T6000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4104T60000", "확장코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "VF 2307", "second row distinct");
    assert_eq!(resp.outblock[1].shcode, "104T7000", "second row 단축코드 distinct");
}

/// Covers shared contract item 2. `shcode` (단축코드) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// may send a code field as either form.
#[test]
fn t9943_numeric_number_or_string_yields_same_value() {
    let as_number: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9943OutBlock": [{ "hname": "VF 2306", "shcode": 1046000 }]
    }))
    .expect("numeric shcode deserializes");
    let as_string: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9943OutBlock": [{ "hname": "VF 2306", "shcode": "1046000" }]
    }))
    .expect("string shcode deserializes");
    assert_eq!(as_number.outblock[0].shcode, "1046000");
    assert_eq!(as_string.outblock[0].shcode, "1046000");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t9943OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t9943_single_object_row_deserializes_to_one_element_vec() {
    let single: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9943OutBlock": { "hname": "VF 2306" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "VF 2306");
    // The standalone row struct also default-constructs cleanly.
    assert!(T9943OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t9943` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t9943_empty_result_deserializes_as_pending() {
    let empty: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// t9944 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). The out-block
// is itself a ROW ARRAY (confirmed from the raw capture's `res_example`,
// propertyType Object Array, the true wire key `t9944OutBlock` per KTD3) — one
// index-option contract per row, the 3 spec fields (hname/shcode/expcode).
// ---------------------------------------------------------------------------

const T9944_FIXTURE: &str = include_str!("fixtures/t9944_resp.json");

/// Covers R4. `t9944` serializes to exactly `{"t9944InBlock":{"dummy":""}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t9944_request_serializes_to_inblock() {
    let value = serde_json::to_value(T9944Request::new()).expect("serialize t9944 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t9944InBlock"]["dummy"], "", "dummy placeholder serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the index-option contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode` instead would surface here.
#[tokio::test]
async fn t9944_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t9944"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T9944_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_option_master_codes(&T9944Request::new())
        .await
        .expect("t9944 index_option_master_codes should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two index-option rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "C 2306 160.0",
        "종목명 index-option contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "201T6160", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4201T61606", "확장코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "C 2306 162.5", "second row distinct");
    assert_eq!(resp.outblock[1].shcode, "201T6162", "second row 단축코드 distinct");
}

/// Covers shared contract item 2. `shcode` (단축코드) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// may send a code field as either form.
#[test]
fn t9944_numeric_number_or_string_yields_same_value() {
    let as_number: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9944OutBlock": [{ "hname": "C 2306 160.0", "shcode": 2016160 }]
    }))
    .expect("numeric shcode deserializes");
    let as_string: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9944OutBlock": [{ "hname": "C 2306 160.0", "shcode": "2016160" }]
    }))
    .expect("string shcode deserializes");
    assert_eq!(as_number.outblock[0].shcode, "2016160");
    assert_eq!(as_string.outblock[0].shcode, "2016160");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t9944OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t9944_single_object_row_deserializes_to_one_element_vec() {
    let single: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9944OutBlock": { "hname": "C 2306 160.0" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "C 2306 160.0");
    // The standalone row struct also default-constructs cleanly.
    assert!(T9944OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t9944` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t9944_empty_result_deserializes_as_pending() {
    let empty: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// U5 (reach wave) — F/O quote/master reads. All `/futureoption/market-data`,
// `[선물/옵션] 시세`, non-paginated. Out-block keys + array-ness read from the RAW
// capture (KTD5): t2111/t2112/t8402/t8403 SINGLE; t2106 summary + ARRAY detail;
// t8434 ARRAY. The canonical field is pinned by baseline `korean_name` (KTD6) to
// an EXACT value from a fixture whose distinct numerics do not collapse.
// ---------------------------------------------------------------------------

/// `t2111` request rename + single out-block round-trips; the canonical 종합지수
/// (`pricejisu`) and KOSPI200지수 (`kospijisu`) hold DISTINCT exact values so a
/// mislabel of either is caught (KTD6). Numeric fields deserialize from a number.
#[test]
fn t2111_request_renames_and_single_out_block_round_trips() {
    let value = serde_json::to_value(T2111Request::new("A0166000")).expect("serialize t2111");
    assert_eq!(value["t2111InBlock"]["focode"], "A0166000");
    assert!(
        value.get("t2111OutBlock").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T2111Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2111OutBlock": {
            "hname": "F 202509",
            "price": 350.25,
            "sign": "2",
            "volume": 123456,
            "mgjv": 9876,
            "pricejisu": 2650.42,
            "kospijisu": 351.18,
            "focode": "A0166000"
        }
    }))
    .expect("representative t2111 success must deserialize");
    // Canonical 종합지수 / KOSPI200지수 pinned to DISTINCT exact values (KTD6).
    assert_eq!(resp.outblock.pricejisu, "2650.42", "종합지수 (composite index)");
    assert_eq!(resp.outblock.kospijisu, "351.18", "KOSPI200지수");
    assert_eq!(resp.outblock.focode, "A0166000");
    assert_eq!(resp.outblock.price, "350.25");
}

/// `t2111` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t2111_numeric_field_string_or_number() {
    let from_num: T2111OutBlock =
        serde_json::from_value(serde_json::json!({ "pricejisu": 2650.42 }))
            .expect("number form deserializes");
    let from_str: T2111OutBlock =
        serde_json::from_value(serde_json::json!({ "pricejisu": "2650.42" }))
            .expect("string form deserializes");
    assert_eq!(from_num.pricejisu, "2650.42");
    assert_eq!(from_str.pricejisu, "2650.42");
}

/// `t2111` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn t2111_empty_result_deserializes_as_pending() {
    let empty: T2111Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t2112` request rename + single order-book out-block round-trips; the canonical
/// 매도호가1 (`offerho1`) and 매수호가1 (`bidho1`) hold DISTINCT exact values (KTD6).
#[test]
fn t2112_request_renames_and_order_book_round_trips() {
    let value = serde_json::to_value(T2112Request::new("A0166000")).expect("serialize t2112");
    assert_eq!(value["t2112InBlock"]["shcode"], "A0166000");

    let resp: T2112Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2112OutBlock": {
            "hname": "F 202509",
            "price": 350.25,
            "volume": 123456,
            "offerho1": 350.50,
            "bidho1": 350.00,
            "offerrem1": 12,
            "bidrem1": 34,
            "shcode": "A0166000"
        }
    }))
    .expect("representative t2112 success must deserialize");
    assert_eq!(resp.outblock.offerho1, "350.5", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "350", "매수호가1");
    assert_eq!(resp.outblock.shcode, "A0166000");
}

/// `t2112` numeric out-block field parses from BOTH string and number JSON; empty
/// result deserializes as pending.
#[test]
fn t2112_numeric_string_or_number_and_empty_pending() {
    let from_num: T2112OutBlock =
        serde_json::from_value(serde_json::json!({ "offerho1": 350.5 }))
            .expect("number form deserializes");
    let from_str: T2112OutBlock =
        serde_json::from_value(serde_json::json!({ "offerho1": "350.5" }))
            .expect("string form deserializes");
    assert_eq!(from_num.offerho1, "350.5");
    assert_eq!(from_str.offerho1, "350.5");

    let empty: T2112Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t2106` request rename + summary block + ARRAY memo block round-trips; a
/// single memo object collapses to a one-element Vec via `de_vec_or_single`
/// (KTD5). Canonical 출력값 (`vals`) pinned to its exact value (KTD6).
#[test]
fn t2106_request_renames_and_memo_array_round_trips() {
    let value = serde_json::to_value(T2106Request::new("101T6000")).expect("serialize t2106");
    assert_eq!(value["t2106InBlock"]["code"], "101T6000");
    assert_eq!(value["t2106InBlock"]["nrec"], "", "default count is empty");

    let resp: T2106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2106OutBlock": { "nrec": "2" },
        "t2106OutBlock1": [
            { "indx": "1", "gubn": "1", "vals": "12345" },
            { "indx": "2", "gubn": "2", "vals": "67890" }
        ]
    }))
    .expect("representative t2106 success must deserialize");
    assert_eq!(resp.outblock.nrec, "2", "출력건수");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].vals, "12345", "출력값");
    assert_eq!(resp.outblock1[1].indx, "2");

    // single memo object → one-element Vec (KTD5 single-or-array).
    let single: T2106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2106OutBlock1": { "indx": "1", "gubn": "1", "vals": "12345" }
    }))
    .expect("single memo object deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: T2106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2106OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty memo array is the pending case");
}

/// `t8402` request rename + single out-block round-trips; the futures 한글명
/// (`hname`) and underlying 기초자산한글명 (`basehname`) hold DISTINCT exact
/// values so a mislabel of either is caught (KTD6).
#[test]
fn t8402_request_renames_and_single_out_block_round_trips() {
    let value = serde_json::to_value(T8402Request::new("111T6000")).expect("serialize t8402");
    assert_eq!(value["t8402InBlock"]["focode"], "111T6000");

    let resp: T8402Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8402OutBlock": {
            "hname": "삼성전자 F 202509",
            "price": 9100,
            "sign": "2",
            "volume": 5000,
            "mgjv": 1200,
            "shcode": "005930",
            "basehname": "삼성전자",
            "baseprice": 91000
        }
    }))
    .expect("representative t8402 success must deserialize");
    assert_eq!(resp.outblock.basehname, "삼성전자", "기초자산한글명 (underlying name)");
    assert_eq!(resp.outblock.hname, "삼성전자 F 202509", "한글명 (futures name)");
    assert_eq!(resp.outblock.shcode, "005930", "기초자산단축코드");

    let from_num: T8402OutBlock =
        serde_json::from_value(serde_json::json!({ "baseprice": 91000 }))
            .expect("number form deserializes");
    let from_str: T8402OutBlock =
        serde_json::from_value(serde_json::json!({ "baseprice": "91000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.baseprice, "91000");
    assert_eq!(from_str.baseprice, "91000");

    let empty: T8402Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t8403` request rename + single order-book out-block round-trips; 매도호가1 and
/// 매수호가1 hold DISTINCT exact values (KTD6); numeric field string-or-number.
#[test]
fn t8403_request_renames_and_order_book_round_trips() {
    let value = serde_json::to_value(T8403Request::new("111T6000")).expect("serialize t8403");
    assert_eq!(value["t8403InBlock"]["shcode"], "111T6000");

    let resp: T8403Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8403OutBlock": {
            "hname": "삼성전자 F 202509",
            "price": 9100,
            "volume": 5000,
            "offerho1": 9105,
            "bidho1": 9095,
            "offerrem1": 7,
            "bidrem1": 9,
            "shcode": "005930"
        }
    }))
    .expect("representative t8403 success must deserialize");
    assert_eq!(resp.outblock.offerho1, "9105", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "9095", "매수호가1");

    let from_str: T8403OutBlock =
        serde_json::from_value(serde_json::json!({ "offerho1": "9105" }))
            .expect("string form deserializes");
    assert_eq!(from_str.offerho1, "9105");

    let empty: T8403Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t8434` request: `qrycnt` serializes as a JSON NUMBER (KTD4 — `string_as_number`,
/// avoids IGW40011); the ARRAY out-block round-trips and a single row collapses to
/// a one-element Vec (KTD5). Canonical 단축코드 (`focode`) pinned exactly (KTD6).
#[test]
fn t8434_request_serializes_qrycnt_as_number_and_array_round_trips() {
    let value = serde_json::to_value(T8434Request::new("1", "101T6000")).expect("serialize t8434");
    assert_eq!(value["t8434InBlock"]["qrycnt"], 1, "qrycnt serializes as a JSON number");
    assert!(
        value["t8434InBlock"]["qrycnt"].is_number(),
        "qrycnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["t8434InBlock"]["focode"], "101T6000");

    let resp: T8434Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8434OutBlock1": [
            { "hname": "F 202509", "price": 350.25, "sign": "2", "change": 1.50, "volume": 123, "focode": "101T6000" },
            { "hname": "F 202512", "price": "352.00", "sign": "2", "change": "2.00", "volume": "456", "focode": "101T9000" }
        ]
    }))
    .expect("representative t8434 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].focode, "101T6000", "단축코드");
    assert_eq!(resp.outblock1[1].price, "352.00", "price from string preserved verbatim");

    // single row object → one-element Vec (KTD5).
    let single: T8434Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8434OutBlock1": { "hname": "F 202509", "price": 350.25, "focode": "101T6000" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].focode, "101T6000");
    assert!(T8434OutBlock1::default().hname.is_empty());

    let empty: T8434Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8434OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty array is the pending case");
}

// ---------------------------------------------------------------------------
// Standalone-lane reads (reach wave U3), routed through `market_session` (KTD3).
// Out-block shape from the raw capture (KTD5): t1988 summary + Object-Array
// detail; t3102 title (Object); t3320 summary + ratios (both Object). Canonical
// field pinned by baseline `korean_name` (KTD6). t1988's numeric request fields
// `from_rate`/`to_rate` assert `.is_number()` (KTD4).
// ---------------------------------------------------------------------------

/// `t1988` request: the numeric rate bounds serialize as JSON NUMBERS (KTD4 —
/// `string_as_number`, avoids IGW40011); the summary + Object-Array detail
/// round-trips and a single detail row collapses to a one-element Vec (KTD5).
/// Canonical 코스피종목건수 (`ksp_cnt`) pinned exactly (KTD6).
#[test]
fn t1988_request_serializes_rate_bounds_as_numbers_and_round_trips() {
    let value = serde_json::to_value(T1988Request::new("0")).expect("serialize t1988");
    assert_eq!(value["t1988InBlock"]["mkt_gb"], "0");
    assert!(
        value["t1988InBlock"]["from_rate"].is_number(),
        "from_rate is a JSON number, not a string (IGW40011 guard)"
    );
    assert!(
        value["t1988InBlock"]["to_rate"].is_number(),
        "to_rate is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["t1988InBlock"]["from_rate"], 0);
    // String filter flags stay quoted (genuine strings).
    assert!(value["t1988InBlock"]["chk_rate"].is_string());
    assert!(
        value.get("t1988OutBlock").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T1988Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1988OutBlock": { "ksp_cnt": "120", "ksd_cnt": 45 },
        "t1988OutBlock1": [
            { "shcode": "005930", "expcode": "KR7005930003", "hname": "삼성전자", "price": 71000, "sign": "2", "volume": "1234567" },
            { "shcode": "000660", "expcode": "KR7000660001", "hname": "SK하이닉스", "price": "128000", "sign": "5", "volume": 987654 }
        ]
    }))
    .expect("representative t1988 success must deserialize");
    // Canonical 코스피종목건수 pinned exactly (KTD6); 코스닥종목건수 from a number form.
    assert_eq!(resp.outblock.ksp_cnt, "120", "코스피종목건수");
    assert_eq!(resp.outblock.ksd_cnt, "45", "코스닥종목건수 from number");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "005930", "단축코드");
    assert_eq!(resp.outblock1[1].price, "128000", "price from string preserved verbatim");

    // single row object → one-element Vec (KTD5).
    let single: T1988Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1988OutBlock": { "ksp_cnt": "1", "ksd_cnt": "0" },
        "t1988OutBlock1": { "shcode": "005930", "hname": "삼성전자", "price": 71000 }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].shcode, "005930");
}

/// `t1988` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t1988_numeric_field_string_or_number() {
    let from_num: T1988OutBlock =
        serde_json::from_value(serde_json::json!({ "ksp_cnt": 120 }))
            .expect("number form deserializes");
    let from_str: T1988OutBlock =
        serde_json::from_value(serde_json::json!({ "ksp_cnt": "120" }))
            .expect("string form deserializes");
    assert_eq!(from_num.ksp_cnt, "120");
    assert_eq!(from_str.ksp_cnt, "120");
}

/// `t1988` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn t1988_empty_result_deserializes_as_pending() {
    let empty: T1988Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.ksp_cnt.is_empty(), "empty summary is the pending case");
    assert!(empty.outblock1.is_empty(), "empty detail array is the pending case");
}

/// `t3102` request rename + title block round-trips. This read ships HELD
/// (input-unresolved: `sNewsno` is sourced only from the realtime `NWS`
/// WebSocket feed — no REST producer), so only the offline shape is pinned;
/// no live smoke flips it.
#[test]
fn t3102_request_renames_and_title_round_trips() {
    // The in-block serializes `sNewsno` under its exact wire key.
    let value = serde_json::to_value(T3102Request::new("20260624123456")).expect("serialize t3102");
    assert_eq!(value["t3102InBlock"]["sNewsno"], "20260624123456");
    assert!(
        value.get("t3102OutBlock2").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3102OutBlock2": { "sTitle": "삼성전자, 신규 투자 발표" }
    }))
    .expect("representative t3102 success must deserialize");
    assert_eq!(resp.outblock2.title, "삼성전자, 신규 투자 발표", "뉴스타이틀");
}

/// `t3102` input-unresolved HELD path: with no REST producer of a news number,
/// the caller input cannot be discovered, so the read is dispositioned HELD —
/// the empty result still deserializes (the pending/empty case).
#[test]
fn t3102_empty_result_deserializes_as_pending() {
    let empty: T3102Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock2.title.is_empty(), "empty title is the held/pending case");
}

/// `t3320` request rename + summary + ratios round-trip; the canonical 한글기업명
/// (`company`) and 현재가 (`price`) hold DISTINCT exact values so a mislabel is
/// caught (KTD6). `gicode` echoes back in the ratios block.
#[test]
fn t3320_request_renames_and_summary_round_trips() {
    let value = serde_json::to_value(T3320Request::new("005930")).expect("serialize t3320");
    assert_eq!(value["t3320InBlock"]["gicode"], "005930");
    assert!(
        value.get("t3320OutBlock").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T3320Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3320OutBlock": {
            "company": "삼성전자",
            "marketnm": "코스피",
            "price": 71000,
            "jnilclose": "70500",
            "sigavalue": 4238000
        },
        "t3320OutBlock1": {
            "gicode": "A005930",
            "per": 12.34,
            "eps": "5700",
            "pbr": 1.45,
            "bps": "49000"
        }
    }))
    .expect("representative t3320 success must deserialize");
    // Canonical 한글기업명 / 현재가 pinned to DISTINCT exact values (KTD6).
    assert_eq!(resp.outblock.company, "삼성전자", "한글기업명");
    assert_eq!(resp.outblock.price, "71000", "현재가");
    assert_eq!(resp.outblock.jnilclose, "70500", "전일종가 from string preserved");
    assert_eq!(resp.outblock1.gicode, "A005930", "기업코드 echoes the caller gicode");
    assert_eq!(resp.outblock1.per, "12.34", "PER from number");
}

/// `t3320` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t3320_numeric_field_string_or_number() {
    let from_num: T3320OutBlock =
        serde_json::from_value(serde_json::json!({ "price": 71000 }))
            .expect("number form deserializes");
    let from_str: T3320OutBlock =
        serde_json::from_value(serde_json::json!({ "price": "71000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.price, "71000");
    assert_eq!(from_str.price, "71000");
}

/// `t3320` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn t3320_empty_result_deserializes_as_pending() {
    let empty: T3320Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.company.is_empty(), "empty summary is the pending case");
}

// ---------------------------------------------------------------------------
// Night-derivatives lane (reach wave U6), routed through `market_session` (KTD3).
// `venue_session: krx_extended` — the night session (~18:00–05:00 KST), NOT the
// regular clock (KTD7): an off-window empty result is NOT a valid attempt, so it
// is a re-run-in-window disposition (not a flip, not a DROP). Out-block shape
// from the raw capture (KTD5): t8455 master is an ARRAY (A0005); t8460 carries a
// single near-month header (A0003) + call/put option ARRAYS (A0005); t8463
// carries a single investor-code header (A0003) + a time-series row ARRAY
// (A0005). Canonical field by baseline `korean_name` (KTD6); t8463's `cnt`
// request field serializes as a JSON number (KTD4).
// ---------------------------------------------------------------------------

/// `t8455` request rename + ARRAY master out-block round-trips; a single row
/// collapses to a one-element Vec via `de_vec_or_single` (KTD5). Canonical 종목명
/// (`hname`) pinned exactly (KTD6).
#[test]
fn t8455_request_renames_and_master_array_round_trips() {
    let value = serde_json::to_value(T8455Request::new("NF")).expect("serialize t8455");
    assert_eq!(value["t8455InBlock"]["gubun"], "NF");
    assert_eq!(value.as_object().expect("object").len(), 1, "exactly one top-level key");

    let resp: T8455Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8455OutBlock": [
            { "hname": "야간 F 202509", "shcode": "111VC000", "expcode": "KR4111VC0001", "tradeunit": 250000 },
            { "hname": "야간 F 202512", "shcode": "111VF000", "expcode": "KR4111VF0008", "tradeunit": "250000" }
        ]
    }))
    .expect("representative t8455 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].hname, "야간 F 202509", "종목명");
    assert_eq!(resp.outblock[1].shcode, "111VF000", "종목코드");
    assert_eq!(resp.outblock[0].tradeunit, "250000", "거래승수 from number");

    // single row object → one-element Vec (KTD5 single-or-array).
    let single: T8455Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8455OutBlock": { "hname": "야간 F 202509", "shcode": "111VC000" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "야간 F 202509");
}

/// `t8455` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t8455_numeric_field_string_or_number() {
    let from_num: T8455OutBlock =
        serde_json::from_value(serde_json::json!({ "tradeunit": 250000 }))
            .expect("number form deserializes");
    let from_str: T8455OutBlock =
        serde_json::from_value(serde_json::json!({ "tradeunit": "250000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.tradeunit, "250000");
    assert_eq!(from_str.tradeunit, "250000");
}

/// `t8455` off-window empty (`00707`, empty array) deserializes — the night
/// session (~18:00–05:00 KST) is closed (KTD7), so this is a RE-RUN-IN-WINDOW
/// disposition (NOT a flip, NOT a DROP), recognized as the empty/pending case.
#[test]
fn t8455_off_window_empty_is_rerun_disposition_not_flip_not_drop() {
    let empty: T8455Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8455OutBlock": []
    }))
    .expect("off-window empty still deserializes (the night window is closed)");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.is_empty(),
        "an empty master array off the night window is the re-run case, not Implemented"
    );
}

/// `t8460` request rename + single header + call/put option ARRAYS round-trip; a
/// single option row collapses to a one-element Vec via `de_vec_or_single`
/// (KTD5). Canonical 근월물현재가 (`gmprice`) pinned exactly (KTD6).
#[test]
fn t8460_request_renames_and_header_plus_option_arrays_round_trip() {
    let value = serde_json::to_value(T8460Request::new("202509", "G")).expect("serialize t8460");
    assert_eq!(value["t8460InBlock"]["yyyymm"], "202509");
    assert_eq!(value["t8460InBlock"]["gubun"], "G");

    let resp: T8460Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8460OutBlock": { "gmprice": 350.25, "gmchange": "1.50", "gmvolume": 12345, "gmshcode": "111VC000" },
        "t8460OutBlock1": [
            { "actprice": 350.0, "optcode": "201VC350", "price": 4.55, "offerho1": 4.60, "bidho1": 4.50 },
            { "actprice": "352.5", "optcode": "201VC352", "price": "3.10", "offerho1": "3.15", "bidho1": "3.05" }
        ],
        "t8460OutBlock2": [
            { "actprice": 350.0, "optcode": "301VC350", "price": 3.20, "offerho1": 3.25, "bidho1": 3.15 }
        ]
    }))
    .expect("representative t8460 success must deserialize");
    assert_eq!(resp.outblock.gmprice, "350.25", "근월물현재가");
    assert_eq!(resp.outblock.gmshcode, "111VC000", "근월물선물코드");
    assert_eq!(resp.outblock1.len(), 2, "call-option array");
    assert_eq!(resp.outblock1[0].optcode, "201VC350", "콜옵션코드");
    assert_eq!(resp.outblock1[1].price, "3.10", "price from string preserved verbatim");
    assert_eq!(resp.outblock2.len(), 1, "put-option array");
    assert_eq!(resp.outblock2[0].optcode, "301VC350", "풋옵션코드");

    // single call-option row → one-element Vec (KTD5).
    let single: T8460Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8460OutBlock1": { "actprice": 350.0, "optcode": "201VC350", "price": 4.55 }
    }))
    .expect("single option row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
}

/// `t8460` off-window empty (`00707`, empty arrays) deserializes — the night
/// window is closed (KTD7), so this is a RE-RUN-IN-WINDOW disposition (NOT a
/// flip, NOT a DROP).
#[test]
fn t8460_off_window_empty_is_rerun_disposition_not_flip_not_drop() {
    let empty: T8460Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8460OutBlock1": [], "t8460OutBlock2": []
    }))
    .expect("off-window empty still deserializes (the night window is closed)");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.gmprice.is_empty(), "empty header is the re-run case");
    assert!(empty.outblock1.is_empty() && empty.outblock2.is_empty(), "empty boards");
}

/// `t8463` request: `cnt` serializes as a JSON NUMBER (KTD4 — `string_as_number`,
/// avoids IGW40011); the single header + ARRAY time-series block round-trip and a
/// single row collapses to a one-element Vec (KTD5). Canonical 일자 (`date`)
/// pinned exactly (KTD6).
#[test]
fn t8463_request_serializes_cnt_as_number_and_header_plus_array_round_trips() {
    let value = serde_json::to_value(T8463Request::new("N", "F", "101")).expect("serialize t8463");
    assert_eq!(value["t8463InBlock"]["tm_rng"], "N");
    assert_eq!(value["t8463InBlock"]["fot_clsf_cd"], "F");
    assert_eq!(value["t8463InBlock"]["bsc_asts_id"], "101");
    assert!(
        value["t8463InBlock"]["cnt"].is_number(),
        "cnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["t8463InBlock"]["cnt"], 20);
    // bgubun stays a genuine string.
    assert!(value["t8463InBlock"]["bgubun"].is_string());

    let resp: T8463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8463OutBlock": { "tm_rng": "N", "indcode": "1000", "forcode": "2000" },
        "t8463OutBlock1": [
            { "date": "20260624", "time": "190000", "indmsvol": 1234, "formsvol": "5678" },
            { "date": "20260624", "time": "191000", "indmsvol": "4321", "formsvol": 8765 }
        ]
    }))
    .expect("representative t8463 success must deserialize");
    assert_eq!(resp.outblock.tm_rng, "N", "시간대");
    assert_eq!(resp.outblock.indcode, "1000", "개인투자자코드");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].date, "20260624", "일자");
    assert_eq!(resp.outblock1[1].indmsvol, "4321", "개인순매수거래량 from string preserved");

    // single row object → one-element Vec (KTD5).
    let single: T8463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8463OutBlock1": { "date": "20260624", "time": "190000", "indmsvol": 1234 }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].date, "20260624");
}

/// `t8463` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t8463_numeric_field_string_or_number() {
    let from_num: T8463OutBlock =
        serde_json::from_value(serde_json::json!({ "indcode": 1000 }))
            .expect("number form deserializes");
    let from_str: T8463OutBlock =
        serde_json::from_value(serde_json::json!({ "indcode": "1000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.indcode, "1000");
    assert_eq!(from_str.indcode, "1000");
}

/// `t8463` off-window empty (`00707`, empty array) deserializes — the night
/// window is closed (KTD7), so this is a RE-RUN-IN-WINDOW disposition (NOT a
/// flip, NOT a DROP).
#[test]
fn t8463_off_window_empty_is_rerun_disposition_not_flip_not_drop() {
    let empty: T8463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8463OutBlock1": []
    }))
    .expect("off-window empty still deserializes (the night window is closed)");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty time-series array is the re-run case");
}

// ---------------------------------------------------------------------------
// Overseas-stock reads (reach wave U7). Domain `overseas_stock` (`g`-prefix).
// Out-block keys/array-ness from the raw capture (KTD5): g3101/g3104/g3106 are
// single Object out-blocks; g3102/g3103/g3190 carry a header Object + an
// Object-Array detail (`…OutBlock1`). Canonical price/name field pinned by
// baseline `korean_name` from a NON-COLLAPSING fixture (price≠open≠high≠low),
// KTD6. Numeric request counts (`readcnt`/`cts_seq`) assert `.is_number()`,
// KTD4. The `01900` paper-incompatible disposition is covered explicitly on
// g3101 (representative): the member stays Tracked, no flip.
// ---------------------------------------------------------------------------

/// `g3101` request rename (no caller leak) + a NON-COLLAPSING success body:
/// `price` (현재가, canonical KTD6) is pinned exactly and is distinct from
/// open/high/low so a mislabel cannot hide.
#[test]
fn g3101_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(G3101Request::new("R", "82TSLA", "82", "TSLA"))
        .expect("serialize g3101");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["g3101InBlock"]["exchcd"], "82");
    assert_eq!(value["g3101InBlock"]["symbol"], "TSLA");
    assert!(value.get("g3101OutBlock").is_none(), "no out-block leaks into the request");

    let resp: G3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3101OutBlock": {
            "korname": "테슬라", "symbol": "TSLA", "price": "283.8200", "sign": "5",
            "diff": "1.1300", "volume": 414175, "open": "285.0900", "high": "285.3100",
            "low": "281.8400", "currency": "USD"
        }
    }))
    .expect("representative g3101 success must deserialize");
    // Canonical 현재가 pinned exactly, distinct from open/high/low (KTD6).
    assert_eq!(resp.outblock.price, "283.8200", "현재가");
    assert_eq!(resp.outblock.open, "285.0900", "시가");
    assert_eq!(resp.outblock.high, "285.3100", "고가");
    assert_eq!(resp.outblock.low, "281.8400", "저가");
    assert_ne!(resp.outblock.price, resp.outblock.open, "non-collapsing: price≠open");
    assert_eq!(resp.outblock.korname, "테슬라", "한글종목명");
}

/// `g3101` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn g3101_numeric_field_string_or_number() {
    let from_num: G3101OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 414175 })).expect("number form");
    let from_str: G3101OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "414175" })).expect("string form");
    assert_eq!(from_num.volume, "414175");
    assert_eq!(from_str.volume, "414175");
}

/// `g3101` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn g3101_empty_result_deserializes_as_pending() {
    let empty: G3101Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty snapshot is the pending case");
}

/// `g3101` `01900` classifies as paper-incompatible — the member stays Tracked,
/// no flip (KTD5/disposition state machine). Representative for the lane.
#[tokio::test]
async fn g3101_code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-stock/market-data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01900",
            "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let err = sdk
        .market_session()
        .overseas_quote(&G3101Request::new("R", "82TSLA", "82", "TSLA"))
        .await
        .expect_err("01900 must surface as an error");
    match &err {
        LsError::ApiError { code, .. } => {
            assert_eq!(code, "01900", "exact code preserved");
            assert!(ls_core::is_paper_incompatible(code), "01900 paper-incompatible");
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

/// `g3104` rename + canonical 한글종목명 (`korname`, KTD6) pinned exactly.
#[test]
fn g3104_request_renames_and_korname_round_trips() {
    let value = serde_json::to_value(G3104Request::new("R", "82TSLA", "82", "TSLA"))
        .expect("serialize g3104");
    assert_eq!(value["g3104InBlock"]["symbol"], "TSLA");
    assert!(value.get("g3104OutBlock").is_none(), "no out-block leaks");

    let resp: G3104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3104OutBlock": {
            "korname": "테슬라", "engname": "TESLA INC", "symbol": "TSLA",
            "exchange_name": "나스닥", "nation_name": "미국", "currency": "USD",
            "share": 3216520000i64, "pcls": "284.9500"
        }
    }))
    .expect("representative g3104 success must deserialize");
    assert_eq!(resp.outblock.korname, "테슬라", "한글종목명");
    assert_eq!(resp.outblock.engname, "TESLA INC", "영문종목명");
    assert_ne!(resp.outblock.korname, resp.outblock.engname, "non-collapsing: kor≠eng");
}

/// `g3104` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn g3104_numeric_field_string_or_number() {
    let from_num: G3104OutBlock =
        serde_json::from_value(serde_json::json!({ "share": 3216520000i64 })).expect("number");
    let from_str: G3104OutBlock =
        serde_json::from_value(serde_json::json!({ "share": "3216520000" })).expect("string");
    assert_eq!(from_num.share, "3216520000");
    assert_eq!(from_str.share, "3216520000");
}

/// `g3104` empty result (00707) deserializes as the pending case.
#[test]
fn g3104_empty_result_deserializes_as_pending() {
    let empty: G3104Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.korname.is_empty(), "empty master is the pending case");
}

/// `g3106` rename + canonical 현재가 (`price`, KTD6) from a non-collapsing body.
#[test]
fn g3106_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(G3106Request::new("R", "82TSLA", "82", "TSLA"))
        .expect("serialize g3106");
    assert_eq!(value["g3106InBlock"]["symbol"], "TSLA");
    assert!(value.get("g3106OutBlock").is_none(), "no out-block leaks");

    let resp: G3106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3106OutBlock": {
            "korname": "테슬라", "symbol": "TSLA", "price": "283.0200", "sign": "5",
            "diff": "1.9300", "volume": 431173, "offerho1": "283.1100", "bidho1": "283.0200"
        }
    }))
    .expect("representative g3106 success must deserialize");
    assert_eq!(resp.outblock.price, "283.0200", "현재가");
    assert_eq!(resp.outblock.offerho1, "283.1100", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "283.0200", "매수호가1");
    assert_ne!(resp.outblock.offerho1, resp.outblock.bidho1, "non-collapsing: offer≠bid");
}

/// `g3106` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn g3106_numeric_field_string_or_number() {
    let from_num: G3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 431173 })).expect("number");
    let from_str: G3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "431173" })).expect("string");
    assert_eq!(from_num.volume, "431173");
    assert_eq!(from_str.volume, "431173");
}

/// `g3106` empty result (00707) deserializes as the pending case.
#[test]
fn g3106_empty_result_deserializes_as_pending() {
    let empty: G3106Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty order book is the pending case");
}

/// `g3102` numeric request fields serialize as JSON NUMBERS (KTD4); the header +
/// Object-Array detail round-trips; canonical 현재가 (`price`) pinned exactly from
/// a non-collapsing row (KTD6); a single detail row collapses to a one-element
/// Vec (KTD5).
#[test]
fn g3102_request_serializes_counts_as_numbers_and_array_round_trips() {
    let value = serde_json::to_value(G3102Request::new("R", "82TSLA", "82", "TSLA", "30", "0"))
        .expect("serialize g3102");
    assert!(
        value["g3102InBlock"]["readcnt"].is_number(),
        "readcnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert!(
        value["g3102InBlock"]["cts_seq"].is_number(),
        "cts_seq is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["g3102InBlock"]["readcnt"], 30);
    assert_eq!(value["g3102InBlock"]["cts_seq"], 0);

    let resp: G3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3102OutBlock": { "symbol": "TSLA", "cts_seq": 20250428014018000i64, "rec_count": 30 },
        "g3102OutBlock1": [
            { "locdate": "20250428", "loctime": "014101", "price": "283.9500", "open": "285.0900", "high": "285.3100", "low": "281.8400", "exevol": 20 },
            { "locdate": "20250428", "loctime": "014055", "price": "284.0000", "open": "285.0900", "high": "285.3100", "low": "281.8400", "exevol": 10 }
        ]
    }))
    .expect("representative g3102 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].price, "283.9500", "현재가");
    assert_ne!(resp.outblock1[0].price, resp.outblock1[0].open, "non-collapsing: price≠open");
    assert_eq!(resp.outblock.rec_count, "30", "레코드카운트");

    // single row object → one-element Vec (KTD5).
    let single: G3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3102OutBlock": { "symbol": "TSLA", "rec_count": "1" },
        "g3102OutBlock1": { "locdate": "20250428", "price": "283.9500" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: G3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "g3102OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty array is the pending case");
}

/// `g3103` rename + header/bar Object-Array round-trips; canonical 현재가
/// (`price`) pinned exactly from a non-collapsing bar (KTD6); single → Vec (KTD5).
#[test]
fn g3103_request_renames_and_bar_array_round_trips() {
    let value = serde_json::to_value(G3103Request::new("R", "82TSLA", "82", "TSLA", "4", "20250120"))
        .expect("serialize g3103");
    assert_eq!(value["g3103InBlock"]["gubun"], "4");
    assert_eq!(value["g3103InBlock"]["date"], "20250120");
    assert!(value.get("g3103OutBlock").is_none(), "no out-block leaks");

    let resp: G3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3103OutBlock": { "symbol": "TSLA", "gubun": "4", "date": "20221031" },
        "g3103OutBlock1": [
            { "chedate": "20250428", "price": "283.4300", "volume": 2568819717i64, "open": "263.8000", "high": "286.8500", "low": "214.2500" },
            { "chedate": "20250331", "price": "259.1600", "volume": 2721582212i64, "open": "300.3400", "high": "303.9400", "low": "217.0200" }
        ]
    }))
    .expect("representative g3103 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].chedate, "20250428", "영업일자");
    assert_eq!(resp.outblock1[0].price, "283.4300", "현재가");
    assert_ne!(resp.outblock1[0].price, resp.outblock1[0].high, "non-collapsing: price≠high");

    let single: G3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3103OutBlock": { "symbol": "TSLA" },
        "g3103OutBlock1": { "chedate": "20250428", "price": "283.4300" }
    }))
    .expect("single bar deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: G3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "g3103OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty bar array is the pending case");
}

/// `g3190` numeric request field serializes as a JSON NUMBER (KTD4); header +
/// master Object-Array round-trips; canonical 한글종목명 (`korname`) pinned exactly
/// (KTD6); single → Vec (KTD5).
#[test]
fn g3190_request_serializes_count_as_number_and_array_round_trips() {
    let value = serde_json::to_value(G3190Request::new("R", "US", "2", "10", ""))
        .expect("serialize g3190");
    assert!(
        value["g3190InBlock"]["readcnt"].is_number(),
        "readcnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["g3190InBlock"]["readcnt"], 10);
    assert_eq!(value["g3190InBlock"]["natcode"], "US");
    // cts_value is a genuine string token (first page = "").
    assert!(value["g3190InBlock"]["cts_value"].is_string());

    let resp: G3190Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3190OutBlock": { "natcode": "US", "cts_value": "0000000000000011", "rec_count": 10 },
        "g3190OutBlock1": [
            { "keysymbol": "82AACB", "symbol": "AACB", "korname": "ARTIUS II ACQUISITION INC", "engname": "ARTIUS II ACQUISITION INC", "currency": "USD", "pcls": "9.9200" },
            { "keysymbol": "82AACG", "symbol": "AACG", "korname": "ATA 크리에티비티 글로벌(ADR)", "engname": "ATA CREATIVITY GLOBAL", "currency": "USD", "pcls": "0.9050" }
        ]
    }))
    .expect("representative g3190 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[1].korname, "ATA 크리에티비티 글로벌(ADR)", "한글종목명");
    assert_eq!(resp.outblock.rec_count, "10", "레코드카운트");

    let single: G3190Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3190OutBlock": { "natcode": "US", "rec_count": "1" },
        "g3190OutBlock1": { "keysymbol": "82AACB", "symbol": "AACB", "korname": "ARTIUS II ACQUISITION INC" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: G3190Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "g3190OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty master array is the pending case");
}

// ---------------------------------------------------------------------------
// Overseas-futures (`o`-prefix) reads — U8 reach wave.
//
// Surface `/overseas-futureoption/market-data`, group `[해외선물] 시세`,
// instrument_domain overseas_futures, venue_session unspecified. One `o`-probe +
// KTD9 A/B (wrong path → http=404, wrong tr_cd → http=500 IGW00215, intended →
// http=200; NO 01900) confirmed the domain REACHABLE and our contract CORRECT.
// The two MASTER reads (o3101/o3121) return non-empty data on paper → IMPLEMENT;
// the four live quote/order-book reads (o3105/o3106/o3125/o3126) answer empty on
// paper → PENDING. Canonical fields by baseline `korean_name`, pinned exactly
// from a NON-COLLAPSING fixture (KTD6); numeric out fields via `string_or_number`
// from BOTH string and number JSON (KTD4); array out-blocks single→one-element
// Vec via `de_vec_or_single` (KTD5). The `01900` disposition is covered
// explicitly on o3101 (representative). No numeric REQUEST fields in this lane.
// ---------------------------------------------------------------------------

/// `o3101` request rename (no caller leak) + a NON-COLLAPSING master row array:
/// `symbol_nm` (종목명, canonical KTD6) pinned exactly and distinct from the
/// base-product name so a mislabel cannot hide; the ARRAY out-block round-trips
/// (KTD5).
#[test]
fn o3101_request_renames_and_master_array_round_trips() {
    let value = serde_json::to_value(O3101Request::new("")).expect("serialize o3101");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["o3101InBlock"]["gubun"], "");
    assert!(value.get("o3101OutBlock").is_none(), "no out-block leaks into the request");

    let resp: O3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3101OutBlock": [
            { "Symbol": "ADM23", "SymbolNm": "Australian Dollar(2023.06)", "BscGdsCd": "AD",
              "BscGdsNm": "Australian Dollar", "ExchCd": "CME", "CrncyCd": "USD",
              "UntPrc": "0.000050000", "DotGb": 5 },
            { "Symbol": "M6EZ23", "SymbolNm": "E-micro EUR/USD(2023.12)", "BscGdsCd": "M6E",
              "BscGdsNm": "E-micro EUR/USD", "ExchCd": "CME", "CrncyCd": "USD",
              "UntPrc": "0.000100000", "DotGb": 5 }
        ]
    }))
    .expect("representative o3101 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    // Canonical 종목명 pinned exactly, distinct from 기초상품명 (KTD6).
    assert_eq!(resp.outblock[0].symbol_nm, "Australian Dollar(2023.06)", "종목명");
    assert_eq!(resp.outblock[0].bsc_gds_nm, "Australian Dollar", "기초상품명");
    assert_ne!(
        resp.outblock[0].symbol_nm, resp.outblock[0].bsc_gds_nm,
        "non-collapsing: 종목명≠기초상품명"
    );

    // single row object → one-element Vec (KTD5).
    let single: O3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3101OutBlock": { "Symbol": "ADM23", "SymbolNm": "Australian Dollar(2023.06)" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
}

/// `o3101` numeric out-block field (`DotGb`/`dot_gb`) parses from BOTH string and
/// number JSON (KTD4).
#[test]
fn o3101_numeric_field_string_or_number() {
    let from_num: O3101OutBlock =
        serde_json::from_value(serde_json::json!({ "DotGb": 5 })).expect("number form");
    let from_str: O3101OutBlock =
        serde_json::from_value(serde_json::json!({ "DotGb": "5" })).expect("string form");
    assert_eq!(from_num.dot_gb, "5");
    assert_eq!(from_str.dot_gb, "5");
}

/// `o3101` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn o3101_empty_result_deserializes_as_pending() {
    let empty: O3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3101OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty master array is the pending case");
}

/// `o3101` `01900` classifies as paper-incompatible — the member stays Tracked,
/// no flip (disposition state machine). Representative for the lane.
#[tokio::test]
async fn o3101_code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-futureoption/market-data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01900",
            "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let err = sdk
        .market_session()
        .overseas_futures_master(&O3101Request::new(""))
        .await
        .expect_err("01900 must surface as an error");
    match &err {
        LsError::ApiError { code, .. } => {
            assert_eq!(code, "01900", "exact code preserved");
            assert!(ls_core::is_paper_incompatible(code), "01900 paper-incompatible");
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

/// `o3121` rename (no caller leak) + a NON-COLLAPSING option-master row array:
/// `bsc_gds_nm` (기초상품명, canonical KTD6) pinned exactly; ARRAY out-block
/// round-trips (KTD5).
#[test]
fn o3121_request_renames_and_master_array_round_trips() {
    let value = serde_json::to_value(O3121Request::new("O", "")).expect("serialize o3121");
    assert_eq!(value["o3121InBlock"]["MktGb"], "O");
    assert_eq!(value["o3121InBlock"]["BscGdsCd"], "");
    assert!(value.get("o3121OutBlock").is_none(), "no out-block leaks");

    let resp: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3121OutBlock": [
            { "Symbol": "", "BscGdsCd": "O_E1A", "BscGdsNm": "W1 Monday E-mini S&P 500 Option",
              "ExchCd": "CME", "XrcPrc": "", "OptTpCode": "", "DotGb": 0 }
        ]
    }))
    .expect("representative o3121 success must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].bsc_gds_nm, "W1 Monday E-mini S&P 500 Option", "기초상품명");
    assert_eq!(resp.outblock[0].exch_cd, "CME", "거래소코드");
    assert_ne!(
        resp.outblock[0].bsc_gds_nm, resp.outblock[0].exch_cd,
        "non-collapsing: 기초상품명≠거래소코드"
    );

    let single: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3121OutBlock": { "BscGdsCd": "O_E1A", "BscGdsNm": "W1 Monday E-mini S&P 500 Option" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
}

/// `o3121` numeric out-block field (`DotGb`) parses from BOTH string and number.
#[test]
fn o3121_numeric_field_string_or_number() {
    let from_num: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "o3121OutBlock": [{ "DotGb": 0 }]
    }))
    .expect("number form");
    let from_str: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "o3121OutBlock": [{ "DotGb": "0" }]
    }))
    .expect("string form");
    assert_eq!(from_num.outblock[0].dot_gb, "0");
    assert_eq!(from_str.outblock[0].dot_gb, "0");
}

/// `o3121` empty result (00707) deserializes as the pending case.
#[test]
fn o3121_empty_result_deserializes_as_pending() {
    let empty: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3121OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty option-master array is the pending case");
}

/// `o3105` rename + canonical 체결가격 (`trd_p`, KTD6) from a non-collapsing
/// single out-block; numeric out fields parse from BOTH forms (KTD4).
#[test]
fn o3105_request_renames_and_trade_price_round_trips() {
    let value = serde_json::to_value(O3105Request::new("CUSN23  ")).expect("serialize o3105");
    assert_eq!(value["o3105InBlock"]["symbol"], "CUSN23  ");
    assert!(value.get("o3105OutBlock").is_none(), "no out-block leaks");

    let resp: O3105Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3105OutBlock": {
            "Symbol": "CUSN23", "SymbolNm": "Renminbi_USD/CNH(2023.07)", "TrdP": "7.2011",
            "OpenP": "7.2081", "HighP": "7.2081", "LowP": "7.1907", "TotQ": 1011, "TrdQ": 1,
            "SeqNo": 1, "CrncyCd": "CNY"
        }
    }))
    .expect("representative o3105 success must deserialize");
    assert_eq!(resp.outblock.trd_p, "7.2011", "체결가격");
    assert_eq!(resp.outblock.open_p, "7.2081", "시가");
    assert_eq!(resp.outblock.low_p, "7.1907", "저가");
    assert_ne!(resp.outblock.trd_p, resp.outblock.open_p, "non-collapsing: 체결가≠시가");
}

/// `o3105` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3105_numeric_field_string_or_number() {
    let from_num: O3105OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": 1011 })).expect("number form");
    let from_str: O3105OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": "1011" })).expect("string form");
    assert_eq!(from_num.tot_q, "1011");
    assert_eq!(from_str.tot_q, "1011");
}

/// `o3105` empty result (00707) deserializes as the pending case.
#[test]
fn o3105_empty_result_deserializes_as_pending() {
    let empty: O3105Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.trd_p.is_empty(), "empty snapshot is the pending case");
}

/// `o3106` rename + canonical 현재가 (`price`, KTD6) from a non-collapsing single
/// out-block (book level-1 distinct from price).
#[test]
fn o3106_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(O3106Request::new("ADM23")).expect("serialize o3106");
    assert_eq!(value["o3106InBlock"]["symbol"], "ADM23");
    assert!(value.get("o3106OutBlock").is_none(), "no out-block leaks");

    let resp: O3106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3106OutBlock": {
            "symbol": "ADM23", "symbolname": "Australian Dollar(2023.06)", "price": "0.67670",
            "change": "0.00135", "volume": 18844, "offerho1": "0.67670", "bidho1": "0.67665",
            "offer": 149, "bid": 220
        }
    }))
    .expect("representative o3106 success must deserialize");
    assert_eq!(resp.outblock.price, "0.67670", "현재가");
    assert_eq!(resp.outblock.offerho1, "0.67670", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "0.67665", "매수호가1");
    assert_ne!(resp.outblock.offerho1, resp.outblock.bidho1, "non-collapsing: offer≠bid");
}

/// `o3106` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3106_numeric_field_string_or_number() {
    let from_num: O3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 18844 })).expect("number form");
    let from_str: O3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "18844" })).expect("string form");
    assert_eq!(from_num.volume, "18844");
    assert_eq!(from_str.volume, "18844");
}

/// `o3106` empty result (00707) deserializes as the pending case.
#[test]
fn o3106_empty_result_deserializes_as_pending() {
    let empty: O3106Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty order book is the pending case");
}

/// `o3125` rename + canonical 체결가격 (`trd_p`, KTD6) from a non-collapsing
/// single out-block; the two-field in-block round-trips.
#[test]
fn o3125_request_renames_and_trade_price_round_trips() {
    let value =
        serde_json::to_value(O3125Request::new("F", "HSIM23          ")).expect("serialize o3125");
    assert_eq!(value["o3125InBlock"]["mktgb"], "F");
    assert_eq!(value["o3125InBlock"]["symbol"], "HSIM23          ");
    assert!(value.get("o3125OutBlock").is_none(), "no out-block leaks");

    let resp: O3125Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3125OutBlock": {
            "Symbol": "HSIM23", "SymbolNm": "Hang Seng(2023.06)", "TrdP": "18922.0",
            "OpenP": "18877.0", "HighP": "19022.0", "LowP": "18676.0", "TotQ": 93965, "TrdQ": 3,
            "SeqNo": 1, "CrncyCd": "HKD"
        }
    }))
    .expect("representative o3125 success must deserialize");
    assert_eq!(resp.outblock.trd_p, "18922.0", "체결가격");
    assert_eq!(resp.outblock.high_p, "19022.0", "고가");
    assert_eq!(resp.outblock.low_p, "18676.0", "저가");
    assert_ne!(resp.outblock.trd_p, resp.outblock.high_p, "non-collapsing: 체결가≠고가");
}

/// `o3125` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3125_numeric_field_string_or_number() {
    let from_num: O3125OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": 93965 })).expect("number form");
    let from_str: O3125OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": "93965" })).expect("string form");
    assert_eq!(from_num.tot_q, "93965");
    assert_eq!(from_str.tot_q, "93965");
}

/// `o3125` empty result (00707) deserializes as the pending case.
#[test]
fn o3125_empty_result_deserializes_as_pending() {
    let empty: O3125Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.trd_p.is_empty(), "empty snapshot is the pending case");
}

/// `o3126` rename + canonical 현재가 (`price`, KTD6) from a non-collapsing single
/// out-block.
#[test]
fn o3126_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(O3126Request::new("F", "ADM23")).expect("serialize o3126");
    assert_eq!(value["o3126InBlock"]["mktgb"], "F");
    assert_eq!(value["o3126InBlock"]["symbol"], "ADM23");
    assert!(value.get("o3126OutBlock").is_none(), "no out-block leaks");

    let resp: O3126Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3126OutBlock": {
            "symbol": "ADM23", "symbolname": "Australian Dollar(2023.06)", "price": "0.67670",
            "change": "0.00135", "volume": 18844, "offerho1": "0.67670", "bidho1": "0.67665",
            "offer": 150, "bid": 220
        }
    }))
    .expect("representative o3126 success must deserialize");
    assert_eq!(resp.outblock.price, "0.67670", "현재가");
    assert_eq!(resp.outblock.offerho1, "0.67670", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "0.67665", "매수호가1");
    assert_ne!(resp.outblock.offerho1, resp.outblock.bidho1, "non-collapsing: offer≠bid");
}

/// `o3126` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3126_numeric_field_string_or_number() {
    let from_num: O3126OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 18844 })).expect("number form");
    let from_str: O3126OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "18844" })).expect("string form");
    assert_eq!(from_num.volume, "18844");
    assert_eq!(from_str.volume, "18844");
}

/// `o3126` empty result (00707) deserializes as the pending case.
#[test]
fn o3126_empty_result_deserializes_as_pending() {
    let empty: O3126Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty order book is the pending case");
}

// ---------------------------------------------------------------------------
// Domestic stock master/reference breadth wave (plan -004). market_session,
// non-paginated, single Object-Array out-blocks via `de_vec_or_single`.
// ---------------------------------------------------------------------------

/// `T9945_POLICY.path` — stock-master endpoint (shared with t1102; tr_cd
/// distinguishes). `T3202_POLICY.path` — the investinfo endpoint.
const STOCK_INVESTINFO_PATH: &str = "/stock/investinfo";

/// The spec-derived fixtures.
const T9945_FIXTURE: &str = include_str!("fixtures/t9945_resp.json");
const T3202_FIXTURE: &str = include_str!("fixtures/t3202_resp.json");

/// Covers R4, R7. `t9945` serializes to `{"t9945InBlock":{"gubun":"1"}}`; no
/// continuation tokens (non-paginated).
#[test]
fn t9945_request_serializes_to_inblock() {
    let value = serde_json::to_value(T9945Request::new("1")).expect("serialize t9945 request");
    assert_eq!(value["t9945InBlock"]["gubun"], "1", "gubun stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// Covers R4, R6. The stock-master array deserializes through REAL dispatch; the
/// canonical fields read their exact expected values (cross-checked vs korean_name).
#[tokio::test]
async fn t9945_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1102_PATH))
        .and(header("tr_cd", "t9945"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T9945_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_master(&T9945Request::new("1"))
        .await
        .expect("t9945 stock_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock.len() >= 3, "master rows round-trip");
    assert_eq!(resp.outblock[0].shcode, "000020", "first 단축코드");
    assert_eq!(resp.outblock[0].hname, "동화약품", "first 종목명");
    assert_eq!(resp.outblock[2].etfchk, "1", "ETF flag on the ETF row");
}

/// Covers R4. A single-object `t9945OutBlock` (one ticker) still deserializes via
/// `de_vec_or_single` — guards the array-vs-single mis-model.
#[test]
fn t9945_single_object_outblock_deserializes() {
    let resp: T9945Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9945OutBlock": { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003" }
    }))
    .expect("single-object t9945OutBlock must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].shcode, "005930");
}

/// Covers R6. An empty `t9945` master list (00707) deserializes as the pending case.
#[test]
fn t9945_empty_result_set_deserializes_as_pending() {
    let empty: T9945Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9945OutBlock": []
    }))
    .expect("empty master list deserializes");
    assert!(empty.outblock.is_empty(), "empty list is the pending case");
}

/// Error: a `01900` response surfaces as `LsError::ApiError` with the exact
/// broker code preserved, classified paper-incompatible.
#[tokio::test]
async fn t9945_code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1102_PATH))
        .and(header("tr_cd", "t9945"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"rsp_cd\":\"01900\",\"rsp_msg\":\"모의투자 미지원\"}")
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let err = sdk_for(&server)
        .market_session()
        .stock_master(&T9945Request::new("1"))
        .await
        .expect_err("01900 must surface as an error");
    match err {
        LsError::ApiError { ref code, .. } => {
            assert_eq!(code, "01900", "exact code preserved");
            assert!(err.is_paper_incompatible(), "01900 is paper-incompatible");
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

/// Covers R4, R7. `t3202` serializes to `{"t3202InBlock":{"shcode":"...","date":""}}`.
#[test]
fn t3202_request_serializes_to_inblock() {
    let value = serde_json::to_value(T3202Request::new("001200")).expect("serialize t3202 request");
    assert_eq!(value["t3202InBlock"]["shcode"], "001200");
    assert_eq!(value["t3202InBlock"]["date"], "", "date defaults empty (all)");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// Covers R4, R6. The schedule array deserializes through REAL dispatch; the
/// canonical event label reads its exact expected value.
#[tokio::test]
async fn t3202_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTINFO_PATH))
        .and(header("tr_cd", "t3202"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T3202_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_schedule(&T3202Request::new("001200"))
        .await
        .expect("t3202 stock_schedule should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock.len() >= 2, "schedule rows round-trip");
    assert_eq!(resp.outblock[0].upunm, "주주총회", "canonical 업무명 event label");
    assert_eq!(resp.outblock[0].custnm, "유진투자증권(주)", "발행회사명");
}

/// Covers R4. A single-object `t3202OutBlock` still deserializes via `de_vec_or_single`.
#[test]
fn t3202_single_object_outblock_deserializes() {
    let resp: T3202Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3202OutBlock": { "shcode": "001200", "upunm": "배당", "recdt": "20240101" }
    }))
    .expect("single-object t3202OutBlock must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].upunm, "배당");
}

/// Covers R6. An empty `t3202` schedule (00707) deserializes as the pending case.
#[test]
fn t3202_empty_result_set_deserializes_as_pending() {
    let empty: T3202Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3202OutBlock": []
    }))
    .expect("empty schedule deserializes");
    assert!(empty.outblock.is_empty(), "empty schedule is the pending case");
}

// === plan -003 all-lane wave — o3104 / o3127 / t8462 (market_session) =========

// --- o3104 — 해외선물 일별체결 (daily executions) ------------------------------

#[test]
fn o3104_request_serializes_to_inblock() {
    let value =
        serde_json::to_value(O3104Request::new("CUSN26", "20260626")).expect("serialize o3104");
    assert_eq!(value["o3104InBlock"]["shcode"], "CUSN26");
    assert_eq!(value["o3104InBlock"]["date"], "20260626");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

#[tokio::test]
async fn o3104_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-futureoption/market-data"))
        .and(header("tr_cd", "o3104"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rsp_cd":"00000","o3104OutBlock1":[{"volume":57123,"chedate":"20230501","high":"0.66820","low":"0.66215","price":"0.66435","change":"0.00150","sign":"2","diff":"0.23","cgubun":"","open":"0.66300"}],"rsp_msg":"조회완료"}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .market_session()
        .overseas_futures_daily(&O3104Request::new("CUSN26", "20260626"))
        .await
        .expect("o3104 should succeed");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "0.66435", "체결가 round-trips");
}

#[test]
fn o3104_single_or_array_and_empty_deserialize() {
    let single: O3104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3104OutBlock1": { "price": "0.66435", "volume": 57123 }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "0.66435");

    let empty: O3104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3104OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

// --- o3127 — 해외선물옵션 관심종목 (watchlist board) --------------------------

#[test]
fn o3127_request_serializes_nrec_as_number() {
    let value = serde_json::to_value(O3127Request::new("20")).expect("serialize o3127");
    assert!(value["o3127InBlock"]["nrec"].is_number(), "nrec is a JSON number (IGW40011 guard)");
}

#[test]
fn o3127_single_or_array_and_empty_deserialize() {
    let single: O3127Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3127OutBlock": { "symbol": "CUSN26", "price": "0.66435", "volume": 100 }
    }))
    .expect("single board row tolerated as array");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].price, "0.66435");

    let empty: O3127Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3127OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty board is the pending case");
}

// --- t8462 — KRX야간파생 투자자기간별 (investor-period table) -----------------

#[test]
fn t8462_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8462Request::new("K2I", "20260601", "20260626"))
        .expect("serialize t8462");
    let ib = &value["t8462InBlock"];
    assert_eq!(ib["bsc_asts_id"], "K2I");
    assert_eq!(ib["from_date"], "20260601");
    assert_eq!(ib["tm_rng"], "N", "night time-range default");
}

#[tokio::test]
async fn t8462_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/futureoption/investor"))
        .and(header("tr_cd", "t8462"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"t8462OutBlock":{"tm_rng":"N","fot_clsf_cd":"F","bsc_asts_id":"K2I"},"t8462OutBlock1":[{"date":"20250610","sv_08":-299,"sv_17":335,"sv_18":-69,"sv_01":-69,"sa_08":"-287","sa_17":"321","sa_18":"-66","sa_01":"-66"}],"rsp_cd":"00000","rsp_msg":"정상적으로 조회가 완료되었습니다."}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .market_session()
        .night_derivatives_investor_period(&T8462Request::new("K2I", "20250609", "20250610"))
        .await
        .expect("t8462 should succeed");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].sv_01, "-69", "개인 순매수수량 round-trips (numeric→string)");
    assert_eq!(resp.outblock.bsc_asts_id, "K2I");
}

#[test]
fn t8462_single_or_array_and_empty_deserialize() {
    let single: T8462Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8462OutBlock": { "bsc_asts_id": "K2I" },
        "t8462OutBlock1": { "date": "20250610", "sv_01": -69 }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].sv_01, "-69");

    let empty: T8462Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8462OutBlock": {}, "t8462OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

// --- t3521 — 해외지수조회 (overseas index snapshot) --------------------------

/// `t3521` serializes to `{"t3521InBlock":{"kind":"...","symbol":"..."}}`; no numeric
/// request fields, non-paginated.
#[test]
fn t3521_request_serializes_to_inblock() {
    let value = serde_json::to_value(T3521Request::new("S", "DJI@DJI")).expect("serialize t3521");
    assert_eq!(value["t3521InBlock"]["kind"], "S");
    assert_eq!(value["t3521InBlock"]["symbol"], "DJI@DJI");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// The snapshot out-block deserializes through REAL dispatch; the substantive
/// `close` (현재지수) reads its exact value.
#[tokio::test]
async fn t3521_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTINFO_PATH))
        .and(header("tr_cd", "t3521"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"조회완료","t3521OutBlock":{"date":"20230602","symbol":"DJI@DJI","change":"701.19","sign":"2","diff":"2.12","close":"33762.76","hname":"다우 산업"}}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .overseas_index_quote(&T3521Request::new("S", "DJI@DJI"))
        .await
        .expect("t3521 overseas_index_quote should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.close, "33762.76", "현재지수 round-trips");
    assert_eq!(resp.outblock.hname, "다우 산업", "지수명 round-trips");
}

/// A numeric `close` from a JSON number still decodes (string_or_number tolerance);
/// an empty snapshot is the pending case.
#[test]
fn t3521_numeric_close_and_empty_deserialize() {
    let numeric: T3521Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3521OutBlock": { "symbol": "DJI@DJI", "close": 33762.76 }
    }))
    .expect("numeric close tolerated");
    assert_eq!(numeric.outblock.close, "33762.76");

    let empty: T3521Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3521OutBlock": {}
    }))
    .expect("empty snapshot deserializes");
    assert!(empty.outblock.close.is_empty(), "empty close is the pending case");
}

// === plan -004 batch A — t1302 분별주가 offline coverage =====================

/// t1302 — 주식분별주가. `cnt` is a JSON number (IGW40011 guard); a representative
/// minute-row body round-trips with a real value; empty 00707 recognized.
#[test]
fn t1302_request_and_response_round_trip() {
    let v = serde_json::to_value(T1302Request::new("001200", "0", "20")).expect("serialize t1302");
    let ib = &v["t1302InBlock"];
    assert!(ib["cnt"].is_number(), "cnt is a JSON number (IGW40011 guard)");
    assert_eq!(ib["shcode"], "001200", "shcode stays a string");
    assert_eq!(ib["exchgubun"], "K");

    let resp: T1302Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1302OutBlock": { "cts_time": "101700" },
        "t1302OutBlock1": [{ "chetime": "102700", "close": 3685, "volume": 321201, "sign": "2" }]
    })).expect("t1302 body round-trips");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "3685", "close from JSON number");
    assert_eq!(resp.outblock1[0].volume, "321201", "volume from JSON number");

    let single: T1302Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1302OutBlock": { "cts_time": "" },
        "t1302OutBlock1": { "chetime": "102700", "close": 3685 }
    })).expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T1302Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1302OutBlock": { "cts_time": "" }, "t1302OutBlock1": []
    })).expect("empty 00707 deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case");
}

// === plan -004 batch B — t2216 F/O tick chart offline coverage ===============

/// t2216 — 선물옵션틱분별체결조회차트. bgubun/cnt numbers; single trade-row array
/// round-trips with a real value; empty 00707 recognized.
#[test]
fn t2216_request_and_response_round_trip() {
    let v = serde_json::to_value(T2216Request::new("A0669000", "T", "20")).expect("serialize t2216");
    let ib = &v["t2216InBlock"];
    assert!(ib["bgubun"].is_number(), "bgubun is a JSON number");
    assert!(ib["cnt"].is_number(), "cnt is a JSON number");
    assert_eq!(ib["focode"], "A0669000", "focode stays a string");

    let resp: T2216Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2216OutBlock1": [{ "chetime": "152000", "price": 41945, "volume": 12, "openyak": 678 }]
    })).expect("t2216 body round-trips");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number");

    let single: T2216Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2216OutBlock1": { "chetime": "152000", "price": 41945 }
    })).expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T2216Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2216OutBlock1": []
    })).expect("empty 00707 deserializes");
    assert!(empty.outblock1.is_empty());
}

// === plan -004 batch C — market_session reference offline coverage ==========

/// t1532 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1532_request_and_response_round_trip() {
    let v = serde_json::to_value(T1532Request::new("078020")).expect("serialize t1532");
    let _ = &v;
    let resp: T1532Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1532OutBlock": [{"tmname": "X1", "avgdiff": 41945}]}"#).expect("t1532 body round-trips");
    assert_eq!(resp.outblock[0].tmname, "X1");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].avgdiff, "41945", "avgdiff from JSON number via string_or_number");
    let empty: T1532Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1532OutBlock":[]}"#).expect("empty deserializes");
    assert!(empty.outblock.is_empty());
}

/// t1533 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1533_request_and_response_round_trip() {
    let v = serde_json::to_value(T1533Request::new("1")).expect("serialize t1533");
    assert!(v["t1533InBlock"]["chgdate"].is_number(), "chgdate numeric");
    let _ = &v;
    let resp: T1533Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1533OutBlock": {"bdate": "x"}, "t1533OutBlock1": [{"tmname": "X1", "avgdiff": 41945}]}"#).expect("t1533 body round-trips");
    assert_eq!(resp.outblock1[0].tmname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].avgdiff, "41945", "avgdiff from JSON number via string_or_number");
    let empty: T1533Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1533OutBlock":{},"t1533OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1926 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1926_request_and_response_round_trip() {
    let v = serde_json::to_value(T1926Request::new("005930")).expect("serialize t1926");
    let _ = &v;
    let resp: T1926Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1926OutBlock": {"mmdate": "X1", "close": 41945}}"#).expect("t1926 body round-trips");
    assert_eq!(resp.outblock.mmdate, "X1");
    assert_eq!(resp.outblock.close, "41945", "close from JSON number via string_or_number");
    let empty: T1926Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1926OutBlock":{}}"#).expect("empty deserializes");
    assert!(empty.outblock.mmdate.is_empty());
}

/// t1764 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1764_request_and_response_round_trip() {
    let v = serde_json::to_value(T1764Request::new("001200")).expect("serialize t1764");
    let _ = &v;
    let resp: T1764Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1764OutBlock": [{"tradno": "X1", "rank": 41945}]}"#).expect("t1764 body round-trips");
    assert_eq!(resp.outblock[0].tradno, "X1");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].rank, "41945", "rank from JSON number via string_or_number");
    let empty: T1764Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1764OutBlock":[]}"#).expect("empty deserializes");
    assert!(empty.outblock.is_empty());
}

/// t1903 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1903_request_and_response_round_trip() {
    let v = serde_json::to_value(T1903Request::new("448330")).expect("serialize t1903");
    let _ = &v;
    let resp: T1903Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1903OutBlock": {"date": "x", "upname": "x", "hname": "x"}, "t1903OutBlock1": [{"date": "X1", "price": 41945}]}"#).expect("t1903 body round-trips");
    assert_eq!(resp.outblock1[0].date, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1903Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1903OutBlock":{},"t1903OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// ---------------------------------------------------------------------------
// t0167 — 서버시간조회 (server-time utility read). Stateless, closure-viable.
// ---------------------------------------------------------------------------

/// `::new` serializes to exactly `{"t0167InBlock":{"id":""}}` (no caller input).
#[test]
fn t0167_request_serializes_inblock_only() {
    let req = T0167Request::new();
    let value = serde_json::to_value(&req).expect("serialize t0167 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert!(obj.contains_key("t0167InBlock"), "missing t0167InBlock key");
    assert_eq!(value["t0167InBlock"]["id"], "", "id slot is empty");
}

/// The spec-derived fixture deserializes; the substantive `time` witness holds a
/// non-default value and `dt` is the server date.
#[tokio::test]
async fn t0167_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/etc/time-search"))
        .and(header("tr_cd", "t0167"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T0167_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .server_time(&T0167Request::new())
        .await
        .expect("t0167 server-time should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.time, "102652926435", "time (substantive witness)");
    assert_eq!(resp.outblock.dt, "20260628", "server date");
}

/// `dt`/`time` parse via `string_or_number` from BOTH string and number JSON.
#[test]
fn t0167_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "t0167OutBlock": { "dt": 20260628i64, "time": 102652926435i64 }
    });
    let resp: T0167Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock.dt, "20260628");
    assert_eq!(resp.outblock.time, "102652926435");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "t0167OutBlock": { "dt": "20260628", "time": "102652926435" }
    });
    let resp: T0167Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock.time, "102652926435");
}
