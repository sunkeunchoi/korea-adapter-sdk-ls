use super::*;


/// Covers R3. The `t1636` request serializes the string filters as JSON strings,
/// with the body `cts_idx` cursor serialized as a JSON NUMBER via
/// `string_as_number` (a string form would risk IGW40011). `cts_idx` is an
/// ORDINARY in-block field at its first-page convention (`0`) — NOT
/// `#[serde(skip)]`. The header continuation is skipped from the body.
#[test]
fn t1636_request_serializes_cts_idx_cursor_as_number() {
    let value = serde_json::to_value(T1636Request::new("0", "0", "0", "005930", ""))
        .expect("serialize t1636");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1636InBlock at the top level");
    let inblock = &value["t1636InBlock"];
    // String-typed filters serialize as JSON strings.
    for f in ["gubun", "gubun1", "gubun2", "shcode", "exchgubun"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["shcode"], "005930");
    // The Number-typed cursor serializes as a JSON number (string_as_number).
    assert!(
        inblock["cts_idx"].is_number(),
        "cts_idx serializes as a JSON number (string_as_number) to avoid IGW40011"
    );
    assert_eq!(inblock["cts_idx"], 0, "first-page cts_idx cursor is the number 0");
    // The cursor is present (NOT skipped) as an ordinary in-block field.
    assert!(
        inblock.get("cts_idx").is_some(),
        "cts_idx is an ordinary in-block field, not skipped"
    );
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1636OutBlock` summary (next-page `cts_idx`) + the sibling `t1636OutBlock1`
/// array round-trip, and modeled non-key fields hold real (non-default) values.
#[tokio::test]
async fn t1636_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_PROGRAM_PATH))
        .and(header("tr_cd", "t1636"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1636_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .program_trade_trend_by_stock(&T1636Request::new("0", "0", "0", "005930", ""))
        .await
        .expect("t1636 program_trade_trend_by_stock should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "program-trading rows round-trip");
    assert_eq!(resp.outblock1[0].hname, "삼성전자", "real non-default Korean name");
    assert_eq!(resp.outblock1[0].shcode, "005930", "real non-default short code");
    assert_eq!(resp.outblock1[0].price, "66100", "price from JSON number");
    assert_eq!(resp.outblock1[0].volume, "12345678", "volume from JSON number");
    assert_eq!(resp.outblock.cts_idx, "40", "next-page cursor from the summary block");
}

/// Covers R4. The program-trading array tolerates single-or-array + empty (the
/// empty case, R7), and `string_or_number` parses numeric fields from BOTH string
/// and number.
#[test]
fn t1636_response_round_trips_single_or_array_and_empty() {
    let single: T1636Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1636OutBlock": { "cts_idx": "40" },
        "t1636OutBlock1": { "rank": "1", "hname": "삼성전자", "shcode": "005930", "price": "66100", "volume": "12345678" }
    }))
    .expect("single program-trading row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock.cts_idx, "40", "next-page cursor round-trips (non-empty)");
    assert_eq!(single.outblock1[0].price, "66100", "price from JSON string");

    let number: T1636Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1636OutBlock": { "cts_idx": 0 },
        "t1636OutBlock1": [{ "rank": 1, "hname": "삼성전자", "shcode": "005930", "price": 66100, "change": 1200, "volume": 12345678 }]
    }))
    .expect("numeric price/change/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "12345678", "volume from JSON number");
    assert_eq!(number.outblock1[0].price, "66100", "price from JSON number");

    let empty: T1636Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1636OutBlock": { "cts_idx": "0" }, "t1636OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty result is the pending case (R7)");
}

/// Registration guard (R3/R8): `T1636_POLICY` must be a real paginated policy.
#[test]
fn t1636_policy_is_registered_and_paginated() {
    assert_eq!(T1636_POLICY.tr_code, "t1636");
    assert_eq!(T1636_POLICY.path, "/stock/program");
    assert!(
        T1636_POLICY.has_pagination,
        "t1636 self-paginates (body cts_idx) — policy must thread continuation"
    );
    assert!(!T1636_POLICY.is_order, "t1636 is a non-order read");
}

// === plan -004 batch C — paginated reference/ranking offline coverage =======

/// t1444 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1444_request_and_response_round_trip() {
    let v = serde_json::to_value(T1444Request::new("001")).expect("serialize t1444");
    assert!(v["t1444InBlock"]["idx"].is_number(), "idx numeric");
    let _ = &v;
    let resp: T1444Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1444OutBlock": {"idx": "x"}, "t1444OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1444 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1444Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1444OutBlock":{},"t1444OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1422 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1422_request_and_response_round_trip() {
    let v = serde_json::to_value(T1422Request::new()).expect("serialize t1422");
    assert!(v["t1422InBlock"]["jc_num"].is_number(), "jc_num numeric");
    assert!(v["t1422InBlock"]["sprice"].is_number(), "sprice numeric");
    assert!(v["t1422InBlock"]["eprice"].is_number(), "eprice numeric");
    assert!(v["t1422InBlock"]["volume"].is_number(), "volume numeric");
    assert!(v["t1422InBlock"]["idx"].is_number(), "idx numeric");
    let _ = &v;
    let resp: T1422Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1422OutBlock": {"cnt": "x", "idx": "x"}, "t1422OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1422 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1422Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1422OutBlock":{},"t1422OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1427 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1427_request_and_response_round_trip() {
    let v = serde_json::to_value(T1427Request::new()).expect("serialize t1427");
    assert!(v["t1427InBlock"]["diff"].is_number(), "diff numeric");
    assert!(v["t1427InBlock"]["jc_num"].is_number(), "jc_num numeric");
    assert!(v["t1427InBlock"]["sprice"].is_number(), "sprice numeric");
    assert!(v["t1427InBlock"]["eprice"].is_number(), "eprice numeric");
    assert!(v["t1427InBlock"]["volume"].is_number(), "volume numeric");
    assert!(v["t1427InBlock"]["idx"].is_number(), "idx numeric");
    let _ = &v;
    let resp: T1427Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1427OutBlock": {"cnt": "x", "idx": "x"}, "t1427OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1427 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1427Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1427OutBlock":{},"t1427OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1442 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1442_request_and_response_round_trip() {
    let v = serde_json::to_value(T1442Request::new()).expect("serialize t1442");
    assert!(v["t1442InBlock"]["jc_num"].is_number(), "jc_num numeric");
    assert!(v["t1442InBlock"]["sprice"].is_number(), "sprice numeric");
    assert!(v["t1442InBlock"]["eprice"].is_number(), "eprice numeric");
    assert!(v["t1442InBlock"]["volume"].is_number(), "volume numeric");
    assert!(v["t1442InBlock"]["idx"].is_number(), "idx numeric");
    assert!(v["t1442InBlock"]["jc_num2"].is_number(), "jc_num2 numeric");
    let _ = &v;
    let resp: T1442Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1442OutBlock": {"idx": "x"}, "t1442OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1442 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1442Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1442OutBlock":{},"t1442OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1405 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1405_request_and_response_round_trip() {
    let v = serde_json::to_value(T1405Request::new("0", "1")).expect("serialize t1405");
    let _ = &v;
    let resp: T1405Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1405OutBlock": {"cts_shcode": "x"}, "t1405OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1405 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1405Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1405OutBlock":{},"t1405OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1960 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1960_request_and_response_round_trip() {
    let v = serde_json::to_value(T1960Request::new()).expect("serialize t1960");
    assert!(v["t1960InBlock"]["sprice"].is_number(), "sprice numeric");
    assert!(v["t1960InBlock"]["eprice"].is_number(), "eprice numeric");
    assert!(v["t1960InBlock"]["volume"].is_number(), "volume numeric");
    assert!(v["t1960InBlock"]["sjanday"].is_number(), "sjanday numeric");
    assert!(v["t1960InBlock"]["ejanday"].is_number(), "ejanday numeric");
    assert!(v["t1960InBlock"]["idx"].is_number(), "idx numeric");
    let _ = &v;
    let resp: T1960Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1960OutBlock": {"idx": "x"}, "t1960OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1960 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1960Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1960OutBlock":{},"t1960OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1961 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1961_request_and_response_round_trip() {
    let v = serde_json::to_value(T1961Request::new()).expect("serialize t1961");
    assert!(v["t1961InBlock"]["sprice"].is_number(), "sprice numeric");
    assert!(v["t1961InBlock"]["eprice"].is_number(), "eprice numeric");
    assert!(v["t1961InBlock"]["volume"].is_number(), "volume numeric");
    assert!(v["t1961InBlock"]["sjanday"].is_number(), "sjanday numeric");
    assert!(v["t1961InBlock"]["ejanday"].is_number(), "ejanday numeric");
    assert!(v["t1961InBlock"]["idx"].is_number(), "idx numeric");
    let _ = &v;
    let resp: T1961Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1961OutBlock": {"idx": "x"}, "t1961OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1961 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1961Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1961OutBlock":{},"t1961OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1966 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1966_request_and_response_round_trip() {
    let v = serde_json::to_value(T1966Request::new()).expect("serialize t1966");
    assert!(v["t1966InBlock"]["sprice"].is_number(), "sprice numeric");
    assert!(v["t1966InBlock"]["eprice"].is_number(), "eprice numeric");
    assert!(v["t1966InBlock"]["volume"].is_number(), "volume numeric");
    assert!(v["t1966InBlock"]["sjanday"].is_number(), "sjanday numeric");
    assert!(v["t1966InBlock"]["ejanday"].is_number(), "ejanday numeric");
    assert!(v["t1966InBlock"]["idx"].is_number(), "idx numeric");
    let _ = &v;
    let resp: T1966Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1966OutBlock": {"idx": "x"}, "t1966OutBlock1": [{"hname": "X1", "price": 41945}]}"#).expect("t1966 body round-trips");
    assert_eq!(resp.outblock1[0].hname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1966Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1966OutBlock":{},"t1966OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t1921 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1921_request_and_response_round_trip() {
    let v = serde_json::to_value(T1921Request::new("005930")).expect("serialize t1921");
    assert!(v["t1921InBlock"]["idx"].is_number(), "idx numeric");
    let _ = &v;
    let resp: T1921Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1921OutBlock": {"date": "x", "cnt": "x", "idx": "x"}, "t1921OutBlock1": [{"mmdate": "X1", "close": 41945}]}"#).expect("t1921 body round-trips");
    assert_eq!(resp.outblock1[0].mmdate, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "41945", "close from JSON number via string_or_number");
    let empty: T1921Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1921OutBlock":{},"t1921OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}
