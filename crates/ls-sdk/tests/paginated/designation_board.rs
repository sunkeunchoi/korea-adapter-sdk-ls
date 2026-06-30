use super::*;


/// Covers R3. The `t1404` request serializes every in-block field as a JSON
/// **string** (no `string_as_number` slot), with the `cts_shcode` cursor at its
/// first-page `" "` convention and the header continuation skipped.
#[test]
fn t1404_request_serializes_all_fields_as_strings() {
    let value = serde_json::to_value(T1404Request::new()).expect("serialize t1404 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1404InBlock at the top level");
    let inblock = &value["t1404InBlock"];
    for f in ["gubun", "jongchk", "cts_shcode"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["gubun"], "0");
    assert_eq!(inblock["jongchk"], "1");
    assert_eq!(inblock["cts_shcode"], " ", "first-page cts_shcode cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1404OutBlock` summary cursor + the TOP-LEVEL sibling `t1404OutBlock1` array
/// round-trip, and a modeled non-key field (`hname`) holds a real non-default value.
#[tokio::test]
async fn t1404_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1404"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1404_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .designation_board(&T1404Request::new())
        .await
        .expect("t1404 designation_board should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "designation rows round-trip");
    assert_eq!(resp.outblock1[0].hname, "흥국화재2우B", "real non-default Korean name");
    assert_eq!(resp.outblock1[0].reason, "5102", "real non-default reason code");
    assert_eq!(resp.outblock1[0].price, "16500", "price from JSON number");
    assert_eq!(resp.outblock1[0].tprice, "16200", "designation-date price from JSON number");
}

/// Covers R4. The designation array tolerates single-or-array + empty (the
/// concrete `t1404` empty-board risk, R7), and `string_or_number` parses
/// `price`/`change`/`volume` from BOTH string and number.
#[test]
fn t1404_response_round_trips_single_or_array_and_empty() {
    let single: T1404Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1404OutBlock": { "cts_shcode": "000547" },
        "t1404OutBlock1": { "hname": "JTC", "shcode": "950170", "price": "3920", "volume": "5492" }
    }))
    .expect("single designation row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock.cts_shcode, "000547", "next-page cursor round-trips (non-empty)");
    assert_eq!(single.outblock1[0].price, "3920", "price from JSON string");

    let number: T1404Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1404OutBlock": { "cts_shcode": "" },
        "t1404OutBlock1": [{ "hname": "JTC", "shcode": "950170", "price": 3920, "volume": 5492 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "5492", "volume from JSON number");

    let empty: T1404Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1404OutBlock": { "cts_shcode": "" }, "t1404OutBlock1": []
    }))
    .expect("empty board deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case (R7)");
}

/// Registration guard (R3/R8): `T1404_POLICY` must be a real paginated policy.
#[test]
fn t1404_policy_is_registered_and_paginated() {
    assert_eq!(T1404_POLICY.tr_code, "t1404");
    assert_eq!(T1404_POLICY.path, "/stock/market-data");
    assert!(
        T1404_POLICY.has_pagination,
        "t1404 self-paginates (cts_shcode) — policy must thread continuation"
    );
    assert!(!T1404_POLICY.is_order, "t1404 is a non-order read");
}
