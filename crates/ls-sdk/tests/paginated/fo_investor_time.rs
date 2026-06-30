use super::*;


/// `t2541` request: `cts_idx`/`cnt` serialize as JSON numbers (IGW40011 guard);
/// `cts_time`/`upcode` stay strings; header cursors skipped.
#[test]
fn t2541_request_serializes_numeric_cursors_and_count() {
    let value = serde_json::to_value(T2541Request::new("001")).expect("serialize t2541");
    let ib = &value["t2541InBlock"];
    assert!(ib["cts_idx"].is_number(), "cts_idx is a JSON number");
    assert!(ib["cnt"].is_number(), "cnt is a JSON number");
    assert!(ib["cts_time"].is_string(), "cts_time cursor stays a string");
    assert_eq!(ib["upcode"], "001");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped");
}

/// `t2541` response: the per-time net-buy array round-trips through dispatch with a
/// real non-default witness (`sv_17`).
#[tokio::test]
async fn t2541_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_INVESTOR_PATH))
        .and(header("tr_cd", "t2541"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"","t2541OutBlock":{"eitem":"01","cts_time":"100000","ms_08":"123"},"t2541OutBlock1":[{"time":"100000","sv_08":"-50","sv_17":"320","sv_18":"-270"}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .fo_investor_by_time(&T2541Request::new("001"))
        .await
        .expect("t2541 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].sv_17, "320", "foreign net-buy witness round-trips");
}

/// `t2541` tolerates single-or-array + empty (pending) forms.
#[test]
fn t2541_single_or_array_and_empty() {
    let single: T2541Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2541OutBlock": { "eitem": "01" },
        "t2541OutBlock1": { "time": "100000", "sv_17": 320 }
    }))
    .expect("single tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].sv_17, "320", "number wire value via string_or_number");

    let empty: T2541Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2541OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty page is the pending case");
}

/// `T2541_POLICY` is a registered paginated F/O-investor policy.
#[test]
fn t2541_policy_is_registered_and_paginated() {
    assert_eq!(T2541_POLICY.tr_code, "t2541");
    assert_eq!(T2541_POLICY.path, "/futureoption/investor");
    assert!(T2541_POLICY.has_pagination, "t2541 self-paginates (cts_time/cts_idx)");
    assert!(!T2541_POLICY.is_order, "t2541 is a non-order read");
}
