use super::*;


/// Covers R3. The `t1410` request serializes every in-block field as a JSON
/// **string**, with the `cts_shcode` cursor as an ORDINARY in-block field at its
/// first-page `""` convention (NOT `#[serde(skip)]`) and the header continuation
/// skipped from the body.
#[test]
fn t1410_request_serializes_cts_shcode_as_ordinary_empty_in_block_field() {
    let value = serde_json::to_value(T1410Request::new()).expect("serialize t1410 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1410InBlock at the top level");
    let inblock = &value["t1410InBlock"];
    for f in ["gubun", "cts_shcode"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["gubun"], "0");
    // The cursor is present as an ordinary in-block field, empty on the first page —
    // NOT skipped. A `#[serde(skip)]` cursor would make this key absent.
    assert!(
        inblock.get("cts_shcode").is_some(),
        "cts_shcode is an ordinary in-block field, not skipped"
    );
    assert_eq!(inblock["cts_shcode"], "", "first-page cts_shcode cursor is empty, not absent");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1410OutBlock` summary cursor + the TOP-LEVEL sibling `t1410OutBlock1` array
/// round-trip, and a modeled non-key field (`hname`) holds a real non-default value.
#[tokio::test]
async fn t1410_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1410"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1410_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .low_liquidity_board(&T1410Request::new())
        .await
        .expect("t1410 low_liquidity_board should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "low-liquidity rows round-trip");
    assert_eq!(resp.outblock1[0].hname, "흥국화재우", "real non-default Korean name");
    assert_eq!(resp.outblock1[0].shcode, "000545", "real non-default short code");
    assert_eq!(resp.outblock1[0].price, "5620", "price from JSON number");
    assert_eq!(resp.outblock1[0].volume, "22", "volume from JSON number");
}

/// Covers R4. The low-liquidity array tolerates single-or-array + empty (the
/// concrete `t1410` empty-board risk, R7), and `string_or_number` parses
/// `price`/`change`/`volume` from BOTH string and number.
#[test]
fn t1410_response_round_trips_single_or_array_and_empty() {
    let single: T1410Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1410OutBlock": { "cts_shcode": "000545" },
        "t1410OutBlock1": { "hname": "흥국화재우", "shcode": "000545", "price": "5620", "volume": "22" }
    }))
    .expect("single low-liquidity row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock.cts_shcode, "000545", "next-page cursor round-trips (non-empty)");
    assert_eq!(single.outblock1[0].price, "5620", "price from JSON string");

    let number: T1410Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1410OutBlock": { "cts_shcode": "" },
        "t1410OutBlock1": [{ "hname": "흥국화재우", "shcode": "000545", "price": 5620, "change": 50, "volume": 22 }]
    }))
    .expect("numeric price/change/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "22", "volume from JSON number");
    assert_eq!(number.outblock1[0].change, "50", "change from JSON number");

    let empty: T1410Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1410OutBlock": { "cts_shcode": "" }, "t1410OutBlock1": []
    }))
    .expect("empty board deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case (R7)");
}

/// Registration guard (R3/R8): `T1410_POLICY` must be a real paginated policy.
#[test]
fn t1410_policy_is_registered_and_paginated() {
    assert_eq!(T1410_POLICY.tr_code, "t1410");
    assert_eq!(T1410_POLICY.path, "/stock/market-data");
    assert!(
        T1410_POLICY.has_pagination,
        "t1410 self-paginates (cts_shcode) — policy must thread continuation"
    );
    assert!(!T1410_POLICY.is_order, "t1410 is a non-order read");
}
