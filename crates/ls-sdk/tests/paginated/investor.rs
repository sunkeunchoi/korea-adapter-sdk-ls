use super::*;


// --- t1602 — 시간대별투자자매매추이 (time-band investor flow; self-paginated on cts_time) ---

/// Covers R3. `t1602` serializes `cts_idx`/`cnt` as JSON **numbers** (string form
/// returns IGW40011); the `cts_time` cursor + string filters stay strings; header
/// cursors skipped.
#[test]
fn t1602_request_serializes_numeric_slots() {
    let value = serde_json::to_value(T1602Request::new("1", "001", "1", "0", "1"))
        .expect("serialize t1602 request");
    let inblock = &value["t1602InBlock"];
    assert!(inblock["cts_idx"].is_number(), "cts_idx is a JSON number");
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number");
    assert!(inblock["cts_time"].is_string(), "cts_time cursor stays a string");
    assert_eq!(inblock["market"], "1");
    assert_eq!(inblock["upcode"], "001");
    assert_eq!(inblock["cts_time"], "", "first-page cts_time cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`sv_17`) holds a real non-default value.
#[tokio::test]
async fn t1602_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTOR_PATH))
        .and(header("tr_cd", "t1602"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1602OutBlock":{"cts_time":"100000","ms_08":12345,"ms_17":67890,"svolume_17":4200,"ms_18":111},"t1602OutBlock1":[{"time":"100000","sv_08":-1500,"sv_17":4200,"sv_18":-700}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .investor_flow_time_band(&T1602Request::new("1", "001", "1", "0", "1"))
        .await
        .expect("t1602 investor_flow_time_band should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page investor-flow rows round-trip");
    assert_eq!(resp.outblock1[0].sv_17, "4200", "real non-default net-buy (from JSON number)");
    assert_eq!(resp.outblock.cts_time, "100000", "next-page cursor round-trips");
}

/// Covers R4. The flow array tolerates single-or-array + empty, and
/// `string_or_number` parses `sv_17` from BOTH string and number.
#[test]
fn t1602_response_round_trips_single_or_array_and_empty() {
    let single: T1602Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1602OutBlock": { "cts_time": "100000", "svolume_17": 4200 },
        "t1602OutBlock1": { "time": "100000", "sv_17": "4200" }
    }))
    .expect("single flow row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].sv_17, "4200", "net-buy from JSON string");

    let number: T1602Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1602OutBlock": { "cts_time": "100000" },
        "t1602OutBlock1": [{ "time": "100000", "sv_17": 4200 }]
    }))
    .expect("numeric net-buy tolerated");
    assert_eq!(number.outblock1[0].sv_17, "4200", "net-buy from JSON number");

    let empty: T1602Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1602OutBlock": { "cts_time": "" }, "t1602OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1602_POLICY` is a real paginated non-order policy.
#[test]
fn t1602_policy_is_registered_and_paginated() {
    assert_eq!(T1602_POLICY.tr_code, "t1602");
    assert_eq!(T1602_POLICY.path, "/stock/investor");
    assert!(T1602_POLICY.has_pagination, "t1602 self-paginates (cts_time)");
    assert!(!T1602_POLICY.is_order, "t1602 is a non-order read");
}

// --- t1603 — 투자자별매매종목 (investor detail by issue; self-paginated on cts_time) ---

/// Covers R3. `t1603` serializes `cts_idx`/`cnt` as JSON **numbers**; the
/// `cts_time` cursor + string filters stay strings; header cursors skipped.
#[test]
fn t1603_request_serializes_numeric_slots() {
    let value = serde_json::to_value(T1603Request::new("1", "1", "0", "001", "1"))
        .expect("serialize t1603 request");
    let inblock = &value["t1603InBlock"];
    assert!(inblock["cts_idx"].is_number(), "cts_idx is a JSON number");
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number");
    assert!(inblock["cts_time"].is_string(), "cts_time cursor stays a string");
    assert_eq!(inblock["market"], "1");
    assert_eq!(inblock["upcode"], "001");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`msvolume`) holds a real non-default value.
#[tokio::test]
async fn t1603_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTOR_PATH))
        .and(header("tr_cd", "t1603"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1603OutBlock":{"cts_idx":7,"cts_time":"100000"},"t1603OutBlock1":[{"time":"100000","tjjcode":"0000","msvolume":12345,"mdvolume":6789,"svolume":5556}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .investor_detail(&T1603Request::new("1", "1", "0", "001", "1"))
        .await
        .expect("t1603 investor_detail should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page investor rows round-trip");
    assert_eq!(resp.outblock1[0].msvolume, "12345", "real non-default buy quantity (from JSON number)");
    assert_eq!(resp.outblock.cts_time, "100000", "next-page cursor round-trips");
}

/// Covers R4. The flow array tolerates single-or-array + empty, and
/// `string_or_number` parses `msvolume`/`svolume` from BOTH string and number.
#[test]
fn t1603_response_round_trips_single_or_array_and_empty() {
    let single: T1603Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1603OutBlock": { "cts_idx": 7, "cts_time": "100000" },
        "t1603OutBlock1": { "time": "100000", "msvolume": "12345", "svolume": "5556" }
    }))
    .expect("single investor row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].msvolume, "12345", "buy quantity from JSON string");

    let number: T1603Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1603OutBlock": { "cts_idx": 7 },
        "t1603OutBlock1": [{ "time": "100000", "msvolume": 12345, "svolume": 5556 }]
    }))
    .expect("numeric quantities tolerated");
    assert_eq!(number.outblock1[0].svolume, "5556", "net-buy quantity from JSON number");

    let empty: T1603Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1603OutBlock": { "cts_idx": 0 }, "t1603OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1603_POLICY` is a real paginated non-order policy.
#[test]
fn t1603_policy_is_registered_and_paginated() {
    assert_eq!(T1603_POLICY.tr_code, "t1603");
    assert_eq!(T1603_POLICY.path, "/stock/investor");
    assert!(T1603_POLICY.has_pagination, "t1603 self-paginates (cts_time)");
    assert!(!T1603_POLICY.is_order, "t1603 is a non-order read");
}

// --- t1617 — 투자자별일별매매추이 (investor time/daily flow; all-String request) ---

/// Covers R3. `t1617` has NO numeric request slot — all fields serialize as JSON
/// strings, including the `cts_date`/`cts_time` cursors; header cursors skipped.
#[test]
fn t1617_request_is_all_string() {
    let value = serde_json::to_value(T1617Request::new("1", "1", "1", "1"))
        .expect("serialize t1617 request");
    let inblock = &value["t1617InBlock"];
    assert!(inblock["gubun1"].is_string(), "gubun1 stays a string");
    assert!(inblock["cts_date"].is_string(), "cts_date cursor stays a string");
    assert!(inblock["cts_time"].is_string(), "cts_time cursor stays a string");
    assert_eq!(inblock["cts_date"], "", "first-page cts_date cursor");
    assert_eq!(inblock["cts_time"], "", "first-page cts_time cursor");
    assert_eq!(inblock["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`sv_17`) holds a real non-default value.
#[tokio::test]
async fn t1617_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTOR_PATH))
        .and(header("tr_cd", "t1617"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1617OutBlock":{"cts_date":"20260629","cts_time":"100000","sv_08":-1500,"sv_17":4200,"sv_18":-700},"t1617OutBlock1":[{"date":"20260629","time":"100000","sv_08":-1500,"sv_17":4200,"sv_18":-700}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .investor_flow_daily(&T1617Request::new("1", "1", "1", "1"))
        .await
        .expect("t1617 investor_flow_daily should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page flow rows round-trip");
    assert_eq!(resp.outblock1[0].sv_17, "4200", "real non-default net-buy (from JSON number)");
    assert_eq!(resp.outblock.cts_date, "20260629", "next-page cursor round-trips");
}

/// Covers R4. The flow array tolerates single-or-array + empty, and
/// `string_or_number` parses `sv_17` from BOTH string and number.
#[test]
fn t1617_response_round_trips_single_or_array_and_empty() {
    let single: T1617Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1617OutBlock": { "cts_date": "20260629", "sv_17": 4200 },
        "t1617OutBlock1": { "date": "20260629", "time": "100000", "sv_17": "4200" }
    }))
    .expect("single flow row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].sv_17, "4200", "net-buy from JSON string");

    let number: T1617Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1617OutBlock": { "cts_date": "20260629" },
        "t1617OutBlock1": [{ "date": "20260629", "time": "100000", "sv_17": 4200 }]
    }))
    .expect("numeric net-buy tolerated");
    assert_eq!(number.outblock1[0].sv_17, "4200", "net-buy from JSON number");

    let empty: T1617Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1617OutBlock": { "cts_date": "" }, "t1617OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1617_POLICY` is a real paginated non-order policy.
#[test]
fn t1617_policy_is_registered_and_paginated() {
    assert_eq!(T1617_POLICY.tr_code, "t1617");
    assert_eq!(T1617_POLICY.path, "/stock/investor");
    assert!(T1617_POLICY.has_pagination, "t1617 self-paginates (cts_date/cts_time)");
    assert!(!T1617_POLICY.is_order, "t1617 is a non-order read");
}
