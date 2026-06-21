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
    T8425Request, T8425Response,
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
