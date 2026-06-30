use super::*;


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

// ---- t1471 시간대별호가잔량추이 (open-window domestic; market_session scalar header + order-book array) ----

/// `t1471` serializes its five caller filters under the renamed `t1471InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1471_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1471Request::new("005930", "0", "", "20", "1"))
        .expect("serialize t1471 request");
    assert_eq!(value["t1471InBlock"]["shcode"], "005930");
    assert_eq!(value["t1471InBlock"]["gubun"], "0");
    assert_eq!(value["t1471InBlock"]["time"], "");
    assert_eq!(value["t1471InBlock"]["cnt"], "20", "cnt is a request String here");
    assert_eq!(value["t1471InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1471InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND modeled non-key fields (the scalar
/// `price` + the order-book row `offerho1`) hold real, non-default values.
/// Numeric-bearing fields tolerate a JSON number or string.
#[test]
fn t1471_success_body_deserializes_with_nondefault_field() {
    let as_string: T1471Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1471OutBlock": { "time": "153000", "price": "00073000", "sign": "2",
                           "change": "00000500", "volume": "000012345678" },
        "t1471OutBlock1": [
            { "time": "153000", "offerrem1": "00001234", "offerho1": "00073100",
              "bidho1": "00073000", "bidrem1": "00005678", "totofferrem": "00012345",
              "totbidrem": "00023456", "close": "00073000" },
            { "time": "152959", "offerrem1": "00001230", "offerho1": "00073100",
              "bidho1": "00073000", "bidrem1": "00005670", "totofferrem": "00012340",
              "totbidrem": "00023450", "close": "00073000" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1471Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1471OutBlock": { "price": 73000 },
        "t1471OutBlock1": [ { "offerho1": 73100, "bidho1": 73000i64 } ]
    }))
    .expect("number body must deserialize");
    assert_eq!(
        as_string.outblock.price, "00073000",
        "scalar header price is non-default"
    );
    assert_eq!(as_string.outblock1.len(), 2, "order-book array round-trips");
    assert_eq!(
        as_string.outblock1[0].offerho1, "00073100",
        "modeled non-key row field is non-default"
    );
    assert_eq!(as_number.outblock.price, "73000");
    assert_eq!(as_number.outblock1[0].offerho1, "73100");
}

/// A single (non-array) `t1471OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1471_single_object_out_block_is_tolerated() {
    let resp: T1471Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1471OutBlock": { "price": "00073000" },
        "t1471OutBlock1": { "time": "153000", "offerho1": "00073100" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].offerho1, "00073100");
}

/// An empty result (`00707`) deserializes cleanly — default header, empty rows.
#[test]
fn t1471_empty_result_deserializes_to_empty() {
    let empty: T1471Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1471 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.price.is_empty(), "no header → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
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
