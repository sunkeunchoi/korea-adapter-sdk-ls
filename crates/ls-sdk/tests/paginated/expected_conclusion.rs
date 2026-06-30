use super::*;


// --- t1486 — 예상체결가등락율 (expected-conclusion; self-paginated on cts_time) ---

/// Covers R3. `t1486` serializes `cnt` as a JSON **number** (string form returns
/// IGW40011), the `cts_time` cursor + identifiers stay strings, header cursors
/// skipped.
#[test]
fn t1486_request_serializes_cnt_as_number() {
    let value =
        serde_json::to_value(T1486Request::new("005930", "1")).expect("serialize t1486 request");
    let inblock = &value["t1486InBlock"];
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number");
    assert!(inblock["cts_time"].is_string(), "cts_time cursor stays a string");
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["exchgubun"], "1");
    assert_eq!(inblock["cts_time"], "", "first-page cts_time cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`price`) holds a real non-default value.
#[tokio::test]
async fn t1486_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1486"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1486OutBlock":{"cts_time":"090000","ex_shcode":"005930"},"t1486OutBlock1":[{"chetime":"085900","price":71600,"sign":"2","change":600,"cvolume":340}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .expected_ticks(&T1486Request::new("005930", "1"))
        .await
        .expect("t1486 expected_ticks should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page expected rows round-trip");
    assert_eq!(resp.outblock1[0].price, "71600", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock.cts_time, "090000", "next-page cursor round-trips");
}

/// Covers R4. The expected-conclusion array tolerates single-or-array + empty;
/// `price`/`cvolume` parse from BOTH string and number.
#[test]
fn t1486_response_round_trips_single_or_array_and_empty() {
    let single: T1486Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1486OutBlock": { "cts_time": "090000", "ex_shcode": "005930" },
        "t1486OutBlock1": { "chetime": "085900", "price": "71600", "cvolume": "340" }
    }))
    .expect("single expected row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "71600", "price from JSON string");

    let number: T1486Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1486OutBlock": { "cts_time": "090000" },
        "t1486OutBlock1": [{ "chetime": "085900", "price": 71600, "cvolume": 340 }]
    }))
    .expect("numeric price/cvolume tolerated");
    assert_eq!(number.outblock1[0].cvolume, "340", "cvolume from JSON number");

    let empty: T1486Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1486OutBlock": { "cts_time": "" }, "t1486OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1486_POLICY` is a real paginated non-order policy.
#[test]
fn t1486_policy_is_registered_and_paginated() {
    assert_eq!(T1486_POLICY.tr_code, "t1486");
    assert_eq!(T1486_POLICY.path, "/stock/market-data");
    assert!(T1486_POLICY.has_pagination, "t1486 self-paginates (cts_time)");
    assert!(!T1486_POLICY.is_order, "t1486 is a non-order read");
}
