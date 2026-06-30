use super::*;


/// Covers R3. The `t1310` request serializes every in-block field as a JSON
/// **string** (no `string_as_number` / IGW40011 slot), with the `cts_time` cursor
/// kept a string and the header continuation skipped from the body.
#[test]
fn t1310_request_serializes_all_fields_as_strings() {
    let value = serde_json::to_value(T1310Request::new("005930")).expect("serialize t1310 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1310InBlock at the top level");
    let inblock = &value["t1310InBlock"];
    for f in ["daygb", "timegb", "shcode", "endtime", "cts_time", "exchgubun"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["cts_time"], "", "first-page cts_time cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1310OutBlock` cursor + the `t1310OutBlock1` array round-trip, and a modeled
/// non-key field (`price`) holds a real non-default value.
#[tokio::test]
async fn t1310_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1310"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1310_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .daily_tick_chart(&T1310Request::new("005930"))
        .await
        .expect("t1310 daily_tick_chart should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "tick/min bars round-trip");
    assert_eq!(resp.outblock1[0].price, "3685", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock1[0].chetime, "102700", "real non-default time");
    assert_eq!(resp.outblock.cts_time, "100700", "next-page cursor round-trips");
}

/// Covers R4. The tick array tolerates single-or-array + empty (pending) forms,
/// and `string_or_number` parses `price`/`volume` from BOTH string and number.
#[test]
fn t1310_response_round_trips_single_or_array_and_empty() {
    let single: T1310Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1310OutBlock": { "cts_time": "100700" },
        "t1310OutBlock1": { "chetime": "100800", "price": "3685", "volume": "300647" }
    }))
    .expect("single tick row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "3685", "price from JSON string");
    assert_eq!(single.outblock1[0].volume, "300647", "volume from JSON string");

    let number: T1310Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1310OutBlock": { "cts_time": "100700" },
        "t1310OutBlock1": [{ "chetime": "100800", "price": 3685, "volume": 300647 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock1[0].price, "3685", "price from JSON number");
    assert_eq!(number.outblock1[0].volume, "300647", "volume from JSON number");

    let empty: T1310Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1310OutBlock": { "cts_time": "" }, "t1310OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1310_POLICY` must be a real paginated policy — a
/// self-paginating TR shipping `has_pagination: false` would dispatch single-page
/// silently.
#[test]
fn t1310_policy_is_registered_and_paginated() {
    assert_eq!(T1310_POLICY.tr_code, "t1310");
    assert_eq!(T1310_POLICY.path, "/stock/market-data");
    assert!(
        T1310_POLICY.has_pagination,
        "t1310 self-paginates (cts_time) — policy must thread continuation"
    );
    assert!(!T1310_POLICY.is_order, "t1310 is a non-order read");
}
