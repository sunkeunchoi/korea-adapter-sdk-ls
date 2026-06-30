use super::*;


// --- t1637 — 프로그램매매추이(종목별) (per-stock program flow; self-paginated on cts_idx) ---

/// Covers R3. `t1637` serializes `cts_idx` as a JSON **number** (string form
/// returns IGW40011), the remaining fields stay strings, header cursors skipped.
#[test]
fn t1637_request_serializes_cts_idx_as_number() {
    let value = serde_json::to_value(T1637Request::new("0", "0", "005930", "20260629", "1"))
        .expect("serialize t1637 request");
    let inblock = &value["t1637InBlock"];
    assert!(inblock["cts_idx"].is_number(), "cts_idx is a JSON number");
    assert!(inblock["shcode"].is_string(), "shcode stays a string");
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["date"], "20260629");
    assert_eq!(inblock["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. An inline first-page body deserializes through REAL paginated dispatch;
/// a modeled non-key field (`price`) holds a real non-default value.
#[tokio::test]
async fn t1637_deserializes_first_page_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_PROGRAM_PATH))
        .and(header("tr_cd", "t1637"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"ok","t1637OutBlock":{"cts_idx":5},"t1637OutBlock1":[{"date":"20260629","time":"100000","price":71500,"sign":"2","change":500,"volume":1234567,"svolume":4200,"shcode":"005930"}]}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .paginated()
        .program_trade_flow(&T1637Request::new("0", "0", "005930", "20260629", "1"))
        .await
        .expect("t1637 program_trade_flow should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page program-flow rows round-trip");
    assert_eq!(resp.outblock1[0].price, "71500", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock.cts_idx, "5", "next-page cursor round-trips");
}

/// Covers R4. The program-flow array tolerates single-or-array + empty;
/// `price`/`volume`/`svolume` parse from BOTH string and number.
#[test]
fn t1637_response_round_trips_single_or_array_and_empty() {
    let single: T1637Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1637OutBlock": { "cts_idx": 5 },
        "t1637OutBlock1": { "date": "20260629", "time": "100000", "price": "71500", "volume": "1234567", "svolume": "4200" }
    }))
    .expect("single program-flow row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "71500", "price from JSON string");

    let number: T1637Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1637OutBlock": { "cts_idx": 5 },
        "t1637OutBlock1": [{ "date": "20260629", "time": "100000", "price": 71500, "volume": 1234567, "svolume": 4200 }]
    }))
    .expect("numeric price/volume/svolume tolerated");
    assert_eq!(number.outblock1[0].svolume, "4200", "svolume from JSON number");

    let empty: T1637Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1637OutBlock": { "cts_idx": 0 }, "t1637OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1637_POLICY` is a real paginated non-order policy.
#[test]
fn t1637_policy_is_registered_and_paginated() {
    assert_eq!(T1637_POLICY.tr_code, "t1637");
    assert_eq!(T1637_POLICY.path, "/stock/program");
    assert!(T1637_POLICY.has_pagination, "t1637 self-paginates (cts_idx)");
    assert!(!T1637_POLICY.is_order, "t1637 is a non-order read");
}
