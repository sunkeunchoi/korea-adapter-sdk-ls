use super::*;


// --- t1109 — 시간외체결량 (after-hours tick conclusion; self-paginated on idx) ---

/// Covers R3. `t1109` serializes `idx` as a JSON **number** (string form returns
/// IGW40011), the `dan_chetime` cursor + identifiers stay strings, header cursors
/// skipped.
#[test]
fn t1109_request_serializes_idx_as_number() {
    let value = serde_json::to_value(T1109Request::new("005930")).expect("serialize t1109 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1109InBlock at the top level");
    let inblock = &value["t1109InBlock"];
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");
    assert!(inblock["shcode"].is_string(), "shcode stays a string");
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["dan_chetime"], "", "first-page dan_chetime cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`dan_price`) holds a real non-default value.
#[tokio::test]
async fn t1109_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1109"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1109OutBlock":{"ctsshcode":"005930","ctschetime":"153000","idx":3},"t1109OutBlock1":[{"dan_chetime":"153000","dan_price":71500,"dan_sign":"2","dan_change":500,"dan_cvolume":120,"dan_volume":9876543}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .after_hours_ticks(&T1109Request::new("005930"))
        .await
        .expect("t1109 after_hours_ticks should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page tick rows round-trip");
    assert_eq!(resp.outblock1[0].dan_price, "71500", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock.ctschetime, "153000", "next-page cursor round-trips");
}

/// Covers R4. The tick array tolerates single-or-array + empty, and
/// `string_or_number` parses `dan_price`/`dan_volume` from BOTH string and number.
#[test]
fn t1109_response_round_trips_single_or_array_and_empty() {
    let single: T1109Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1109OutBlock": { "ctsshcode": "005930", "ctschetime": "153000", "idx": 3 },
        "t1109OutBlock1": { "dan_chetime": "153000", "dan_price": "71500", "dan_volume": "9876543" }
    }))
    .expect("single tick row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].dan_price, "71500", "price from JSON string");

    let number: T1109Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1109OutBlock": { "ctschetime": "153000", "idx": 3 },
        "t1109OutBlock1": [{ "dan_chetime": "153000", "dan_price": 71500, "dan_volume": 9876543 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock1[0].dan_volume, "9876543", "volume from JSON number");

    let empty: T1109Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1109OutBlock": { "ctschetime": "" }, "t1109OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1109_POLICY` is a real paginated non-order policy.
#[test]
fn t1109_policy_is_registered_and_paginated() {
    assert_eq!(T1109_POLICY.tr_code, "t1109");
    assert_eq!(T1109_POLICY.path, "/stock/market-data");
    assert!(T1109_POLICY.has_pagination, "t1109 self-paginates (idx)");
    assert!(!T1109_POLICY.is_order, "t1109 is a non-order read");
}

// --- t1301 — 시간대별체결조회 (time-band tick conclusion; self-paginated on cts_time) ---

/// Covers R3. `t1301` serializes `cvolume` as a JSON **number** (string form
/// returns IGW40011), the `cts_time` cursor + window bounds stay strings, header
/// cursors skipped.
#[test]
fn t1301_request_serializes_cvolume_as_number() {
    let value = serde_json::to_value(T1301Request::new("005930", "0900", "1530"))
        .expect("serialize t1301 request");
    let inblock = &value["t1301InBlock"];
    assert!(inblock["cvolume"].is_number(), "cvolume is a JSON number");
    assert!(inblock["cts_time"].is_string(), "cts_time cursor stays a string");
    assert!(inblock["starttime"].is_string(), "starttime stays a string");
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["cts_time"], "", "first-page cts_time cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`price`) holds a real non-default value.
#[tokio::test]
async fn t1301_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1301"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1301OutBlock":{"cts_time":"100700"},"t1301OutBlock1":[{"chetime":"100700","price":71500,"sign":"2","change":500,"cvolume":120,"volume":1234567}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .time_band_ticks(&T1301Request::new("005930", "0900", "1530"))
        .await
        .expect("t1301 time_band_ticks should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page tick rows round-trip");
    assert_eq!(resp.outblock1[0].price, "71500", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock.cts_time, "100700", "next-page cursor round-trips");
}

/// Covers R4. The tick array tolerates single-or-array + empty; `price`/`volume`
/// parse from BOTH string and number.
#[test]
fn t1301_response_round_trips_single_or_array_and_empty() {
    let single: T1301Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1301OutBlock": { "cts_time": "100700" },
        "t1301OutBlock1": { "chetime": "100700", "price": "71500", "volume": "1234567" }
    }))
    .expect("single tick row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "71500", "price from JSON string");

    let number: T1301Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1301OutBlock": { "cts_time": "100700" },
        "t1301OutBlock1": [{ "chetime": "100700", "price": 71500, "volume": 1234567 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "1234567", "volume from JSON number");

    let empty: T1301Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1301OutBlock": { "cts_time": "" }, "t1301OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1301_POLICY` is a real paginated non-order policy.
#[test]
fn t1301_policy_is_registered_and_paginated() {
    assert_eq!(T1301_POLICY.tr_code, "t1301");
    assert_eq!(T1301_POLICY.path, "/stock/market-data");
    assert!(T1301_POLICY.has_pagination, "t1301 self-paginates (cts_time)");
    assert!(!T1301_POLICY.is_order, "t1301 is a non-order read");
}

// --- t8454 — 시간대별체결조회 (exchange-qualified time-band; self-paginated on cts_time) ---

/// Covers R3. `t8454` serializes `cvolume` as a JSON **number** (string form
/// returns IGW40011), the `cts_time` cursor + window/exchange stay strings, header
/// cursors skipped.
#[test]
fn t8454_request_serializes_cvolume_as_number() {
    let value = serde_json::to_value(T8454Request::new("005930", "0900", "1530", "1"))
        .expect("serialize t8454 request");
    let inblock = &value["t8454InBlock"];
    assert!(inblock["cvolume"].is_number(), "cvolume is a JSON number");
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
async fn t8454_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8454"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t8454OutBlock":{"cts_time":"100700","ex_shcode":"005930"},"t8454OutBlock1":[{"chetime":"100700","price":71500,"sign":"2","change":500,"cvolume":120,"volume":1234567}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .time_band_ticks_ex(&T8454Request::new("005930", "0900", "1530", "1"))
        .await
        .expect("t8454 time_band_ticks_ex should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page tick rows round-trip");
    assert_eq!(resp.outblock1[0].price, "71500", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock.cts_time, "100700", "next-page cursor round-trips");
}

/// Covers R4. The tick array tolerates single-or-array + empty; `price`/`volume`
/// parse from BOTH string and number.
#[test]
fn t8454_response_round_trips_single_or_array_and_empty() {
    let single: T8454Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8454OutBlock": { "cts_time": "100700", "ex_shcode": "005930" },
        "t8454OutBlock1": { "chetime": "100700", "price": "71500", "volume": "1234567" }
    }))
    .expect("single tick row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "71500", "price from JSON string");

    let number: T8454Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8454OutBlock": { "cts_time": "100700" },
        "t8454OutBlock1": [{ "chetime": "100700", "price": 71500, "volume": 1234567 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "1234567", "volume from JSON number");

    let empty: T8454Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8454OutBlock": { "cts_time": "" }, "t8454OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T8454_POLICY` is a real paginated non-order policy.
#[test]
fn t8454_policy_is_registered_and_paginated() {
    assert_eq!(T8454_POLICY.tr_code, "t8454");
    assert_eq!(T8454_POLICY.path, "/stock/market-data");
    assert!(T8454_POLICY.has_pagination, "t8454 self-paginates (cts_time)");
    assert!(!T8454_POLICY.is_order, "t8454 is a non-order read");
}
