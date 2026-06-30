use super::*;


// --- o3103 — 해외선물차트 분봉 (cts_date/cts_time) -----------------------------

#[test]
fn o3103_request_serializes_numeric_fields_as_numbers() {
    let value = serde_json::to_value(O3103Request::new("CUSN26")).expect("serialize o3103");
    let ib = &value["o3103InBlock"];
    assert_eq!(ib["shcode"], "CUSN26");
    assert!(ib["ncnt"].is_number(), "ncnt is a JSON number (IGW40011 guard)");
    assert!(ib["readcnt"].is_number(), "readcnt is a JSON number (IGW40011 guard)");
    assert!(ib["cts_date"].is_string(), "cts_date cursor stays a string");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
}

#[test]
fn o3103_response_round_trips_single_or_array_and_empty() {
    let single: O3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3103OutBlock": { "cts_date": "20230612", "cts_time": "234700" },
        "o3103OutBlock1": { "date": "20230613", "close": "0.67670", "volume": 51 }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "0.67670");

    let empty: O3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3103OutBlock": {}, "o3103OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

#[tokio::test]
async fn o3103_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(OVS_FO_CHART_PATH))
        .and(header("tr_cd", "o3103"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rsp_cd":"00000","o3103OutBlock":{"cts_date":"20230612","cts_time":"234700"},"o3103OutBlock1":[{"date":"20230613","volume":51,"high":"0.67680","low":"0.67670","time":"000600","close":"0.67670","open":"0.67675"}]}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .overseas_futures_minute_chart(&O3103Request::new("CUSN26"))
        .await
        .expect("o3103 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1[0].close, "0.67670", "종가 from dispatch");
}

// --- o3108 — 해외선물차트(일주월) (cts_date) -----------------------------------

#[test]
fn o3108_request_serializes_numeric_fields_as_numbers() {
    let value =
        serde_json::to_value(O3108Request::new("CUSN26", "0", "20260101", "20260626"))
            .expect("serialize o3108");
    let ib = &value["o3108InBlock"];
    assert_eq!(ib["shcode"], "CUSN26");
    assert!(ib["qrycnt"].is_number(), "qrycnt is a JSON number (IGW40011 guard)");
    assert!(ib["gubun"].is_string(), "gubun mode flag stays a string");
}

#[test]
fn o3108_response_round_trips_single_or_array_and_empty() {
    let single: O3108Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3108OutBlock": { "shcode": "CUSN26", "rec_count": 6 },
        "o3108OutBlock1": { "date": "20230505", "close": "0.67660" }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "0.67660");

    let empty: O3108Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3108OutBlock": {}, "o3108OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// --- o3116 — 해외선물 tick (cts_seq numeric) -----------------------------------

#[test]
fn o3116_request_serializes_numeric_fields_as_numbers() {
    let value = serde_json::to_value(O3116Request::new("0", "CUSN26")).expect("serialize o3116");
    let ib = &value["o3116InBlock"];
    assert_eq!(ib["shcode"], "CUSN26");
    assert!(ib["readcnt"].is_number(), "readcnt is a JSON number (IGW40011 guard)");
    assert!(ib["cts_seq"].is_number(), "cts_seq is a JSON number (IGW40011 guard)");
}

#[test]
fn o3116_response_round_trips_single_or_array_and_empty() {
    let single: O3116Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3116OutBlock": { "cts_seq": 4826 },
        "o3116OutBlock1": { "price": "0.67670", "volume": 18844, "cvolume": 1 }
    }))
    .expect("single tick tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "0.67670");

    let empty: O3116Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3116OutBlock": {}, "o3116OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

#[tokio::test]
async fn o3116_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(OVS_FO_MKT_PATH))
        .and(header("tr_cd", "o3116"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rsp_cd":"00000","o3116OutBlock":{"cts_seq":4826},"o3116OutBlock1":[{"volume":18844,"ovstime":"000533","price":"0.67670","change":"0.00135","sign":"2","ovsdate":"20230613","diff":"0.20","cvolume":1}]}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .overseas_futures_tick(&O3116Request::new("0", "CUSN26"))
        .await
        .expect("o3116 should succeed");
    assert_eq!(resp.outblock1[0].price, "0.67670", "체결가 from dispatch");
}

// --- o3117 — 해외선물 NTick (cts_seq/cts_daygb) --------------------------------

#[test]
fn o3117_request_serializes_numeric_fields_as_numbers() {
    let value = serde_json::to_value(O3117Request::new("CUSN26")).expect("serialize o3117");
    let ib = &value["o3117InBlock"];
    assert!(ib["ncnt"].is_number(), "ncnt is a JSON number (IGW40011 guard)");
    assert!(ib["qrycnt"].is_number(), "qrycnt is a JSON number (IGW40011 guard)");
    assert!(ib["cts_seq"].is_string(), "cts_seq cursor stays a string");
    assert!(ib["cts_daygb"].is_string(), "cts_daygb cursor stays a string");
}

#[test]
fn o3117_response_round_trips_single_or_array_and_empty() {
    let single: O3117Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3117OutBlock": { "shcode": "CUSN26", "cts_seq": "4826", "cts_daygb": "0" },
        "o3117OutBlock1": { "date": "20230613", "close": "0.67670" }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "0.67670");
    assert_eq!(single.outblock.cts_seq, "4826");

    let empty: O3117Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3117OutBlock": {}, "o3117OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// --- o3123 — 해외선물옵션 분봉 (cts_date/cts_time) -----------------------------

#[test]
fn o3123_request_serializes_numeric_fields_as_numbers() {
    let value = serde_json::to_value(O3123Request::new("F", "CUSN26")).expect("serialize o3123");
    let ib = &value["o3123InBlock"];
    assert_eq!(ib["mktgb"], "F");
    assert!(ib["ncnt"].is_number(), "ncnt is a JSON number (IGW40011 guard)");
    assert!(ib["readcnt"].is_number(), "readcnt is a JSON number (IGW40011 guard)");
}

#[test]
fn o3123_response_round_trips_single_or_array_and_empty() {
    let single: O3123Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3123OutBlock": { "cts_date": "20230612", "cts_time": "234700" },
        "o3123OutBlock1": { "date": "20230613", "close": "0.67670" }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "0.67670");

    let empty: O3123Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3123OutBlock": {}, "o3123OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// --- o3128 — 해외선물옵션 일주월 (cts_date) ------------------------------------

#[test]
fn o3128_request_serializes_numeric_fields_as_numbers() {
    let value =
        serde_json::to_value(O3128Request::new("F", "CUSN26", "1", "20250525", "20260626"))
            .expect("serialize o3128");
    let ib = &value["o3128InBlock"];
    assert_eq!(ib["mktgb"], "F");
    assert!(ib["qrycnt"].is_number(), "qrycnt is a JSON number (IGW40011 guard)");
}

#[test]
fn o3128_response_round_trips_single_or_array_and_empty() {
    let single: O3128Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3128OutBlock": { "shcode": "CUSN26", "rec_count": 6, "diclose": "0.67670" },
        "o3128OutBlock1": { "date": "20230505", "close": "0.67660" }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "0.67660");
    assert_eq!(single.outblock.diclose, "0.67670");

    let empty: O3128Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3128OutBlock": {}, "o3128OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// --- o3136 — 해외선물옵션 tick (cts_seq numeric) -------------------------------

#[test]
fn o3136_request_serializes_numeric_fields_as_numbers() {
    let value =
        serde_json::to_value(O3136Request::new("0", "F", "CUSN26")).expect("serialize o3136");
    let ib = &value["o3136InBlock"];
    assert_eq!(ib["mktgb"], "F");
    assert!(ib["readcnt"].is_number(), "readcnt is a JSON number (IGW40011 guard)");
    assert!(ib["cts_seq"].is_number(), "cts_seq is a JSON number (IGW40011 guard)");
}

#[test]
fn o3136_response_round_trips_single_or_array_and_empty() {
    let single: O3136Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3136OutBlock": { "cts_seq": 4826 },
        "o3136OutBlock1": { "price": "0.67670", "volume": 18844 }
    }))
    .expect("single tick tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "0.67670");

    let empty: O3136Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3136OutBlock": {}, "o3136OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// --- o3137 — 해외선물옵션 NTick (cts_seq/cts_daygb) ----------------------------

#[test]
fn o3137_request_serializes_numeric_fields_as_numbers() {
    let value = serde_json::to_value(O3137Request::new("F", "CUSN26")).expect("serialize o3137");
    let ib = &value["o3137InBlock"];
    assert!(ib["ncnt"].is_number(), "ncnt is a JSON number (IGW40011 guard)");
    assert!(ib["qrycnt"].is_number(), "qrycnt is a JSON number (IGW40011 guard)");
    assert!(ib["cts_seq"].is_string(), "cts_seq cursor stays a string");
}

#[test]
fn o3137_response_round_trips_single_or_array_and_empty() {
    let single: O3137Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3137OutBlock": { "shcode": "CUSN26", "cts_seq": "0", "cts_daygb": "0" },
        "o3137OutBlock1": { "date": "20230613", "close": "0.67670" }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "0.67670");

    let empty: O3137Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3137OutBlock": {}, "o3137OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// --- o3139 — 해외선물옵션 NTick 고정형 (cts_seq/cts_daygb) ---------------------

#[test]
fn o3139_request_serializes_numeric_fields_as_numbers() {
    let value = serde_json::to_value(O3139Request::new("F", "CUSN26")).expect("serialize o3139");
    let ib = &value["o3139InBlock"];
    assert!(ib["ncnt"].is_number(), "ncnt is a JSON number (IGW40011 guard)");
    assert!(ib["qrycnt"].is_number(), "qrycnt is a JSON number (IGW40011 guard)");
    assert!(ib["cts_daygb"].is_string(), "cts_daygb cursor stays a string");
}

#[test]
fn o3139_response_round_trips_single_or_array_and_empty() {
    let single: O3139Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3139OutBlock": { "shcode": "CUSN26", "cts_seq": "0" },
        "o3139OutBlock1": { "date": "20230613", "close": "0.67670" }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "0.67670");

    let empty: O3139Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3139OutBlock": {}, "o3139OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}
