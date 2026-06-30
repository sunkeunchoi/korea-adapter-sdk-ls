use super::*;


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
