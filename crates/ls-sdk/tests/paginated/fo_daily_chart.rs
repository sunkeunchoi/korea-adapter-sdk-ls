use super::*;


/// `t2214` request: `cnt` serializes as a JSON number; `cts_code`/`shcode` stay
/// strings; header cursors skipped.
#[test]
fn t2214_request_serializes_cnt_as_number() {
    let value = serde_json::to_value(T2214Request::new("A0669000", "20260629")).expect("serialize t2214");
    let ib = &value["t2214InBlock"];
    assert!(ib["cnt"].is_number(), "cnt is a JSON number");
    assert!(ib["cts_code"].is_string(), "cts_code cursor stays a string");
    assert_eq!(ib["shcode"], "A0669000");
    assert_eq!(ib["futcheck"], "1", "front-month default");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped");
}

/// `t2214` response: the daily OHLCV array round-trips with a real non-default
/// witness (`close`).
#[tokio::test]
async fn t2214_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MD_PATH))
        .and(header("tr_cd", "t2214"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"","t2214OutBlock":{"date":"20260629","cts_code":"","nowfutyn":"Y"},"t2214OutBlock1":[{"date":"20260627","open":"345.10","high":"346.00","low":"344.00","close":"345.55","volume":12345,"openyak":"6789","value":"100000"}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .fo_daily_chart(&T2214Request::new("A0669000", "20260629"))
        .await
        .expect("t2214 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "345.55", "daily close witness round-trips");
    assert_eq!(resp.outblock1[0].volume, "12345", "volume from JSON number");
}

/// `t2214` tolerates single-or-array + empty (pending) forms.
#[test]
fn t2214_single_or_array_and_empty() {
    let single: T2214Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2214OutBlock": { "date": "20260629" },
        "t2214OutBlock1": { "date": "20260627", "close": 345.55, "volume": 12345 }
    }))
    .expect("single tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "345.55", "number wire value via string_or_number");

    let empty: T2214Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2214OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty page is the pending case");
}

/// `T2214_POLICY` is a registered paginated F/O-market-data policy.
#[test]
fn t2214_policy_is_registered_and_paginated() {
    assert_eq!(T2214_POLICY.tr_code, "t2214");
    assert_eq!(T2214_POLICY.path, "/futureoption/market-data");
    assert!(T2214_POLICY.has_pagination, "t2214 self-paginates (cts_code)");
    assert!(!T2214_POLICY.is_order, "t2214 is a non-order read");
}
