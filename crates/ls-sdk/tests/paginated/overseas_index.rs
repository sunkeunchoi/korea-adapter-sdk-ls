use super::*;


// --- t3518 — 해외실시간지수 (overseas index time-series) ----------------------

#[test]
fn t3518_request_serializes_numeric_fields_as_numbers() {
    let value =
        serde_json::to_value(T3518Request::new("S", "NAS@IXIC")).expect("serialize t3518 request");
    let inblock = &value["t3518InBlock"];
    assert_eq!(inblock["kind"], "S");
    assert_eq!(inblock["symbol"], "NAS@IXIC");
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number (IGW40011 guard)");
    assert!(inblock["nmin"].is_number(), "nmin is a JSON number (IGW40011 guard)");
    assert!(inblock["jgbn"].is_string(), "jgbn mode flag stays a string");
    assert!(inblock["cts_date"].is_string(), "cts_date cursor stays a string");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
}

#[test]
fn t3518_response_round_trips_single_or_array_and_empty() {
    let single: T3518Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3518OutBlock": { "cts_date": "20230602", "cts_time": "161540" },
        "t3518OutBlock1": { "date": "20230602", "price": "132.4077", "high": "132.5621", "volume": 0 }
    }))
    .expect("single index tick tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "132.4077", "현재지수 round-trips");
    assert_eq!(single.outblock.cts_date, "20230602", "cursor header round-trips");

    let empty: T3518Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3518OutBlock": {}, "t3518OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

#[tokio::test]
async fn t3518_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTINFO_PATH))
        .and(header("tr_cd", "t3518"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","t3518OutBlock":{"cts_date":"20230602","cts_time":"161540"},"t3518OutBlock1":[{"date":"20230602","price":"132.4077","high":"132.5621","low":"131.2586","open":"131.9048","uprate":"0.0107"}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .overseas_index_series(&T3518Request::new("S", "NAS@IXIC"))
        .await
        .expect("t3518 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "132.4077", "현재지수 from dispatch");
}
