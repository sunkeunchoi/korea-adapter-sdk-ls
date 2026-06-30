use super::*;


// --- t1752 — 거래원별종목별동향 (broker-by-issue; self-paginated on cts_idx) ---

/// Covers R3. `t1752` serializes `cts_idx` as a JSON **number** (string form
/// returns IGW40011); the identifiers + window bounds stay strings; header cursors
/// skipped.
#[test]
fn t1752_request_serializes_cts_idx_as_number() {
    let value = serde_json::to_value(T1752Request::new("005930", "20260629", "20260629", "0", "1"))
        .expect("serialize t1752 request");
    let inblock = &value["t1752InBlock"];
    assert!(inblock["cts_idx"].is_number(), "cts_idx is a JSON number");
    assert!(inblock["shcode"].is_string(), "shcode stays a string");
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["traddate1"], "20260629");
    assert_eq!(inblock["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`tradmsvol`) holds a real non-default value.
#[tokio::test]
async fn t1752_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_EXCHANGE_PATH))
        .and(header("tr_cd", "t1752"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1752OutBlock":{"fwdvl":1000,"fwsvl":2000,"cts_idx":4},"t1752OutBlock1":[{"tradname":"미래에셋","tradmdvol":3000,"tradmsvol":5000,"tradmssvol":2000,"tradno":"063"}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .broker_by_issue(&T1752Request::new("005930", "20260629", "20260629", "0", "1"))
        .await
        .expect("t1752 broker_by_issue should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page broker rows round-trip");
    assert_eq!(resp.outblock1[0].tradmsvol, "5000", "real non-default buy quantity (from JSON number)");
    assert_eq!(resp.outblock.cts_idx, "4", "next-page cursor round-trips");
}

/// Covers R4. The broker array tolerates single-or-array + empty, and
/// `string_or_number` parses `tradmsvol` from BOTH string and number.
#[test]
fn t1752_response_round_trips_single_or_array_and_empty() {
    let single: T1752Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1752OutBlock": { "cts_idx": 4 },
        "t1752OutBlock1": { "tradname": "미래에셋", "tradmsvol": "5000" }
    }))
    .expect("single broker row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].tradmsvol, "5000", "buy quantity from JSON string");

    let number: T1752Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1752OutBlock": { "cts_idx": 4 },
        "t1752OutBlock1": [{ "tradname": "미래에셋", "tradmsvol": 5000 }]
    }))
    .expect("numeric quantity tolerated");
    assert_eq!(number.outblock1[0].tradmsvol, "5000", "buy quantity from JSON number");

    let empty: T1752Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1752OutBlock": { "cts_idx": 0 }, "t1752OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1752_POLICY` is a real paginated non-order policy.
#[test]
fn t1752_policy_is_registered_and_paginated() {
    assert_eq!(T1752_POLICY.tr_code, "t1752");
    assert_eq!(T1752_POLICY.path, "/stock/exchange");
    assert!(T1752_POLICY.has_pagination, "t1752 self-paginates (cts_idx)");
    assert!(!T1752_POLICY.is_order, "t1752 is a non-order read");
}

// --- t1771 — 거래원별시간대별추이 (broker time-series; row array under OutBlock2) ---

/// Covers R3. `t1771` serializes `cts_idx`/`cnt` as JSON **numbers**; the
/// identifiers + window bounds stay strings; header cursors skipped.
#[test]
fn t1771_request_serializes_numeric_slots() {
    let value = serde_json::to_value(T1771Request::new("005930", "", "0", "20260629", "20260629", "1"))
        .expect("serialize t1771 request");
    let inblock = &value["t1771InBlock"];
    assert!(inblock["cts_idx"].is_number(), "cts_idx is a JSON number");
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number");
    assert!(inblock["shcode"].is_string(), "shcode stays a string");
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["tradno"], "", "first-page (all brokers) tradno empty");
    assert_eq!(inblock["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// the row array arrives under `t1771OutBlock2` and a modeled non-key field
/// (`price`) holds a real non-default value.
#[tokio::test]
async fn t1771_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_EXCHANGE_PATH))
        .and(header("tr_cd", "t1771"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1771OutBlock":{"cts_idx":9},"t1771OutBlock2":[{"traddate":"20260629","tradtime":"100000","price":71500,"sign":"2","change":500,"volume":1234567,"tradmsscha":4200}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .broker_time_series(&T1771Request::new("005930", "", "0", "20260629", "20260629", "1"))
        .await
        .expect("t1771 broker_time_series should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock2.is_empty(), "first-page broker-time rows round-trip (OutBlock2)");
    assert_eq!(resp.outblock2[0].price, "71500", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock.cts_idx, "9", "next-page cursor round-trips");
}

/// Covers R4. The OutBlock2 array tolerates single-or-array + empty, and
/// `string_or_number` parses `price`/`volume` from BOTH string and number.
#[test]
fn t1771_response_round_trips_single_or_array_and_empty() {
    let single: T1771Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1771OutBlock": { "cts_idx": 9 },
        "t1771OutBlock2": { "traddate": "20260629", "tradtime": "100000", "price": "71500", "volume": "1234567" }
    }))
    .expect("single broker-time row tolerated as array");
    assert_eq!(single.outblock2.len(), 1);
    assert_eq!(single.outblock2[0].price, "71500", "price from JSON string");

    let number: T1771Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1771OutBlock": { "cts_idx": 9 },
        "t1771OutBlock2": [{ "traddate": "20260629", "tradtime": "100000", "price": 71500, "volume": 1234567 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock2[0].volume, "1234567", "volume from JSON number");

    let empty: T1771Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1771OutBlock": { "cts_idx": 0 }, "t1771OutBlock2": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock2.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1771_POLICY` is a real paginated non-order policy.
#[test]
fn t1771_policy_is_registered_and_paginated() {
    assert_eq!(T1771_POLICY.tr_code, "t1771");
    assert_eq!(T1771_POLICY.path, "/stock/exchange");
    assert!(T1771_POLICY.has_pagination, "t1771 self-paginates (cts_idx)");
    assert!(!T1771_POLICY.is_order, "t1771 is a non-order read");
}
