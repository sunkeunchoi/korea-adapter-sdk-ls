use super::*;


/// Covers R10. The request serializes to exactly `{"t8412InBlock":{...}}` with the
/// `cts_*` continuation echoed in the BODY but the transport `tr_cont`/
/// `tr_cont_key` tokens ABSENT from the body (they ride as HTTP headers).
#[test]
fn request_serializes_cts_in_body_and_no_tr_cont_anywhere() {
    let mut req = pinned_req();
    // Even with the transport continuation set, it must not leak into the body.
    req.tr_cont = "Y".into();
    req.tr_cont_key = "page2key".into();
    // And the body-level continuation IS part of the query.
    req.inblock.cts_date = PINNED_TRADE_DATE.into();
    req.inblock.cts_time = "120000".into();

    let value = serde_json::to_value(&req).expect("serialize t8412 request");

    // Exactly one top-level key: t8412InBlock.
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t8412InBlock"), "missing t8412InBlock key");

    // Transport continuation NEVER serializes into the body (top level or inblock).
    assert!(
        value.get("tr_cont").is_none(),
        "tr_cont must not be in the body"
    );
    assert!(
        value.get("tr_cont_key").is_none(),
        "tr_cont_key must not be in the body"
    );

    let inblock = &value["t8412InBlock"];
    assert!(
        inblock.get("tr_cont").is_none(),
        "tr_cont must not be in the inblock"
    );
    assert!(
        inblock.get("tr_cont_key").is_none(),
        "tr_cont_key must not be in the inblock"
    );

    // cts_* ARE body fields the server echoes — they must serialize.
    assert_eq!(
        inblock["cts_date"], PINNED_TRADE_DATE,
        "cts_date rides in the body"
    );
    assert_eq!(inblock["cts_time"], "120000", "cts_time rides in the body");

    // The pinned trade date is present (never empty-defaults-to-today).
    assert_eq!(inblock["sdate"], PINNED_TRADE_DATE);
    assert_eq!(inblock["edate"], PINNED_TRADE_DATE);

    // ncnt/qrycnt serialize as JSON numbers (string_as_number).
    assert!(
        inblock["ncnt"].is_number(),
        "ncnt must serialize as a number"
    );
    assert!(
        inblock["qrycnt"].is_number(),
        "qrycnt must serialize as a number"
    );
}

/// Happy path: a single page deserializes from the spec-derived fixture with the
/// pinned trade date echoed and key candle fields asserted. Dispatch sends the
/// first-page `tr_cont: N` header.
#[tokio::test]
async fn chart_page_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T8412_PATH))
        .and(header("tr_cd", "t8412"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8412_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .paginated()
        .chart_page(&pinned_req())
        .await
        .expect("t8412 chart page should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.shcode, "078020");
    // The summary echoes the pinned trade date in its body cts_* fields.
    assert_eq!(resp.outblock.cts_date, PINNED_TRADE_DATE);
    assert_eq!(resp.outblock.cts_time, "153000");
    // Two candle rows; numeric fields coerced to String regardless of wire type.
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].date, PINNED_TRADE_DATE);
    assert_eq!(resp.outblock1[0].close, "4540");
    assert_eq!(resp.outblock1[1].close, "4550");
}

/// Happy path: `collect_all` walks TWO pages via the response `tr_cont`/
/// `tr_cont_key` headers and concatenates rows. Page 1's `tr_cont: Y` header drives
/// a second call; page 2's `tr_cont: N` stops the loop.
#[tokio::test]
async fn chart_all_walks_two_pages_via_response_headers() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path(T8412_PATH))
        .respond_with(TwoPageResponder { hits: hits.clone() })
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let pages = sdk
        .paginated()
        .chart_all(pinned_req())
        .await
        .expect("collect_all should walk two pages");

    assert_eq!(pages.len(), 2, "two pages collected");
    assert_eq!(hits.load(Ordering::SeqCst), 2, "exactly two HTTP calls");

    // Page 1's continuation header was injected into the JSON so the getter works.
    assert_eq!(pages[0].tr_cont, "Y");
    assert_eq!(pages[0].tr_cont_key, "page2key");
    assert_eq!(pages[1].tr_cont, "N");

    // Rows concatenate across the pages.
    let rows: Vec<&T8412OutBlock1> = pages.iter().flat_map(|p| p.outblock1.iter()).collect();
    assert_eq!(rows.len(), 3, "2 rows from page 1 + 1 row from page 2");
    assert_eq!(rows[0].close, "4540");
    assert_eq!(rows[1].close, "4550");
    assert_eq!(rows[2].close, "4560");
}

/// Edge: `t8412OutBlock1` arriving as a SINGLE object (not an array) deserializes
/// via `de_vec_or_single` into a 1-element Vec (the gateway collapses a one-row
/// page to a bare object).
#[test]
fn out_block1_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t8412OutBlock": { "shcode": "078020", "cts_date": "20240105" },
        "t8412OutBlock1": {
            "date": "20240105",
            "time": "090100",
            "close": 4540
        }
    });
    let resp: T8412Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(
        resp.outblock1.len(),
        1,
        "single object becomes a 1-element Vec"
    );
    assert_eq!(resp.outblock1[0].date, "20240105");
    assert_eq!(resp.outblock1[0].close, "4540");
}

/// Edge: truncation at `max_pages` returns `LsError::PaginationLimit`. The mock
/// config's `max_pages` is overridden to 2; the server never stops, so the loop
/// hits the cap.
#[tokio::test]
async fn chart_all_truncates_at_max_pages() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path(T8412_PATH))
        .respond_with(NeverStopResponder { hits: hits.clone() })
        .mount(&server)
        .await;

    let sdk = sdk_with_max_pages(&server, 2);
    let err = sdk
        .paginated()
        .chart_all(pinned_req())
        .await
        .expect_err("must hit the pagination cap");

    match err {
        LsError::PaginationLimit(n) => assert_eq!(n, 2, "cap is the configured max_pages"),
        other => panic!("expected PaginationLimit(2), got {other:?}"),
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "exactly max_pages HTTP calls"
    );
}

// --- t8410 — API전용주식차트(일주월년) ----------------------------------------

/// Covers R8/KTD4. `qrycnt` serializes as a JSON **number** (string → IGW40011);
/// t1305 기간별주가 (plan -002 Track 2): numeric request fields dwmcode/idx/cnt
/// serialize as JSON numbers (IGW40011 guard); date/shcode/exchgubun stay strings;
/// header cursors skipped.
#[test]
fn t1305_request_serializes_numeric_fields_as_numbers() {
    let value = serde_json::to_value(T1305Request::new("005930", "1", "20260626", "10"))
        .expect("serialize t1305 request");
    let inblock = &value["t1305InBlock"];
    assert!(inblock["dwmcode"].is_number(), "dwmcode is a JSON number");
    assert!(inblock["idx"].is_number(), "idx is a JSON number");
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number");
    assert!(inblock["date"].is_string(), "date cursor stays a string");
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["exchgubun"], "K");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
}

/// t1305 candle array tolerates single-or-array + empty (pending) forms; numeric
/// candle fields tolerate number or string.
#[test]
fn t1305_response_round_trips_single_or_array_and_empty() {
    let single: T1305Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1305OutBlock": { "cnt": 1, "ex_shcode": "005930" },
        "t1305OutBlock1": { "date": "20260626", "close": 135155, "open": "134000" }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "135155", "close from JSON number");
    assert_eq!(single.outblock1[0].open, "134000", "open from JSON string");

    let empty: T1305Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1305OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// `cts_date`/`shcode` stay strings; header cursors skipped.
#[test]
fn t8410_request_serializes_qrycnt_as_number() {
    let value = serde_json::to_value(T8410Request::new("078020", "2", "200", "", "99999999"))
        .expect("serialize t8410 request");
    let inblock = &value["t8410InBlock"];
    assert!(inblock["qrycnt"].is_number(), "qrycnt is a JSON number");
    assert!(inblock["cts_date"].is_string(), "cts_date cursor stays a string");
    assert_eq!(inblock["shcode"], "078020");
    assert_eq!(inblock["gubun"], "2");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
}

/// Covers R6. The first-page fixture deserializes through REAL paginated dispatch:
/// the header summary + the candle array round-trip with exact values.
#[tokio::test]
async fn t8410_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T8412_PATH))
        .and(header("tr_cd", "t8410"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8410_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .stock_chart_period(&T8410Request::new("078020", "2", "200", "", "99999999"))
        .await
        .expect("t8410 stock_chart_period should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.shcode, "078020", "header 단축코드");
    assert!(!resp.outblock1.is_empty(), "candle rows round-trip");
    assert_eq!(resp.outblock1[0].date, "20230605", "first candle date");
    assert_eq!(resp.outblock1[0].close, "4530", "first candle close");
}

/// Covers R8. The candle array tolerates single-or-array + empty (pending) forms.
#[test]
fn t8410_response_round_trips_single_or_array_and_empty() {
    let single: T8410Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8410OutBlock": { "shcode": "078020" },
        "t8410OutBlock1": { "date": "20230605", "close": 4530, "open": 4550 }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "4530", "close from JSON number");

    let empty: T8410Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8410OutBlock": { "shcode": "" }, "t8410OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

// --- t8451 — (통합)주식챠트(일주월년) ----------------------------------------

#[test]
fn t8451_request_serializes_qrycnt_as_number() {
    let value = serde_json::to_value(T8451Request::new("010950", "2", "10", "", "99999999"))
        .expect("serialize t8451 request");
    let inblock = &value["t8451InBlock"];
    assert!(inblock["qrycnt"].is_number(), "qrycnt is a JSON number");
    assert!(inblock["cts_date"].is_string());
    assert_eq!(inblock["exchgubun"], "N");
    assert!(value.get("tr_cont").is_none());
}

#[tokio::test]
async fn t8451_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T8412_PATH))
        .and(header("tr_cd", "t8451"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8451_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .stock_chart_period_unified(&T8451Request::new("010950", "2", "10", "", "99999999"))
        .await
        .expect("t8451 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.shcode, "010950");
    assert_eq!(resp.outblock.disiga, "60400", "current-day open from header");
    assert_eq!(resp.outblock.svi_uplmtprice, "66300", "static-VI upper limit");
    assert!(resp.outblock1.len() >= 2, "candle rows round-trip");
    assert_eq!(resp.outblock1[0].date, "20250304");
}

#[test]
fn t8451_response_round_trips_single_or_array_and_empty() {
    let single: T8451Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8451OutBlock": { "shcode": "010950" },
        "t8451OutBlock1": { "date": "20250304", "close": 56000 }
    }))
    .expect("single candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].close, "56000");

    let empty: T8451Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8451OutBlock": { "shcode": "" }, "t8451OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

// --- t8419 — 업종차트(일주월) -------------------------------------------------

#[test]
fn t8419_request_serializes_qrycnt_as_number() {
    let value = serde_json::to_value(T8419Request::new("001", "2", "5", "", "99999999"))
        .expect("serialize t8419 request");
    let inblock = &value["t8419InBlock"];
    assert!(inblock["qrycnt"].is_number(), "qrycnt is a JSON number");
    assert!(inblock["cts_date"].is_string());
    assert_eq!(inblock["shcode"], "001", "sector code stays a string");
    assert!(value.get("tr_cont").is_none());
}

#[tokio::test]
async fn t8419_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_CHART_PATH))
        .and(header("tr_cd", "t8419"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8419_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .sector_chart_period(&T8419Request::new("001", "2", "5", "", "99999999"))
        .await
        .expect("t8419 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.shcode, "001");
    assert_eq!(resp.outblock.disiga, "2617.43", "current-day open index from header");
    assert!(resp.outblock1.len() >= 2, "sector candle rows round-trip");
    assert_eq!(resp.outblock1[0].close, "2585.52", "index close as string");
}

#[test]
fn t8419_response_round_trips_single_or_array_and_empty() {
    let single: T8419Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8419OutBlock": { "shcode": "001" },
        "t8419OutBlock1": { "date": "20230530", "close": "2585.52" }
    }))
    .expect("single sector candle tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T8419Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8419OutBlock": { "shcode": "" }, "t8419OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

// --- t4203 — 업종차트(종합) --------------------------------------------------

#[test]
fn t4203_request_serializes_ncnt_and_qrycnt_as_numbers() {
    let value = serde_json::to_value(T4203Request::new("001", "2", "1", "1", "", ""))
        .expect("serialize t4203 request");
    let inblock = &value["t4203InBlock"];
    assert!(inblock["ncnt"].is_number(), "ncnt is a JSON number");
    assert!(inblock["qrycnt"].is_number(), "qrycnt is a JSON number");
    assert!(inblock["cts_date"].is_string());
    assert!(inblock["cts_time"].is_string());
    assert!(value.get("tr_cont").is_none());
}

#[tokio::test]
async fn t4203_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_CHART_PATH))
        .and(header("tr_cd", "t4203"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T4203_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .sector_chart_composite(&T4203Request::new("001", "2", "1", "1", "", ""))
        .await
        .expect("t4203 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.shcode, "001");
    assert_eq!(resp.outblock.disiga, "2617.43", "current-day open index from header");
    assert!(!resp.outblock1.is_empty(), "composite rows round-trip");
    assert_eq!(resp.outblock1[0].time, "102800", "row carries an intraday time");
}

#[test]
fn t4203_response_round_trips_single_or_array_and_empty() {
    let single: T4203Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t4203OutBlock": { "shcode": "001" },
        "t4203OutBlock1": { "date": "20230605", "time": "102800", "close": "2610.85" }
    }))
    .expect("single composite row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T4203Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t4203OutBlock": { "shcode": "" }, "t4203OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

// === plan -004 batch A — chart/price family offline coverage =================
// Covers AE1 (representative body round-trips with a real value), AE4 (numeric
// request field is a JSON number), AE2 (empty 00707 recognized).

/// t8417 — 업종차트(틱/n틱). ncnt/qrycnt as numbers; cursor strings; no header leak.
#[test]
fn t8417_request_and_response_round_trip() {
    let v = serde_json::to_value(T8417Request::new("001", "1", "20", "0", "", "99999999", "N"))
        .expect("serialize t8417");
    let ib = &v["t8417InBlock"];
    assert!(ib["ncnt"].is_number(), "ncnt is a JSON number (IGW40011 guard)");
    assert!(ib["qrycnt"].is_number(), "qrycnt is a JSON number");
    assert!(ib["cts_date"].is_string(), "cursor stays a string");
    assert_eq!(ib["shcode"], "001", "sector code stays a string");
    assert!(v.get("tr_cont").is_none(), "header cursor skipped from body");

    let resp: T8417Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8417OutBlock": { "shcode": "001", "diclose": "2610.85" },
        "t8417OutBlock1": { "date": "20230605", "close": "2610.85", "jdiff_vol": 215 }
    })).expect("single candle tolerated as array");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock.diclose, "2610.85", "real summary value round-trips");
    assert_eq!(resp.outblock1[0].jdiff_vol, "215", "volume from JSON number");

    let empty: T8417Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8417OutBlock": {}, "t8417OutBlock1": []
    })).expect("empty 00707 deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case");
}

/// t8418 — 업종차트(N분).
#[test]
fn t8418_request_and_response_round_trip() {
    let v = serde_json::to_value(T8418Request::new("001", "1", "20", "0", "", "99999999", "N"))
        .expect("serialize t8418");
    assert!(v["t8418InBlock"]["qrycnt"].is_number());
    let resp: T8418Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8418OutBlock": { "shcode": "001", "disvalue": 3886266 },
        "t8418OutBlock1": [{ "date": "20230605", "close": "2610.97", "value": 19176 }]
    })).expect("t8418 body round-trips");
    assert_eq!(resp.outblock1[0].close, "2610.97");
    assert_eq!(resp.outblock.disvalue, "3886266", "traded value from JSON number");
    let empty: T8418Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8418OutBlock": {}, "t8418OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t8411 — 주식차트(틱/n틱).
#[test]
fn t8411_request_and_response_round_trip() {
    let v = serde_json::to_value(T8411Request::new("005930", "1", "20", "0", "", "99999999", "N"))
        .expect("serialize t8411");
    assert!(v["t8411InBlock"]["ncnt"].is_number());
    assert!(v.get("tr_cont").is_none());
    let resp: T8411Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8411OutBlock": { "shcode": "005930", "diclose": 60500 },
        "t8411OutBlock1": { "date": "20250312", "close": 60600, "jdiff_vol": 288 }
    })).expect("single candle tolerated as array");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "60600", "close from JSON number");
    let empty: T8411Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8411OutBlock": {}, "t8411OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t8452 — (통합)주식챠트(N분). Carries the exchgubun selector.
#[test]
fn t8452_request_and_response_round_trip() {
    let v = serde_json::to_value(T8452Request::new("010950", "1", "20", "0", "", "99999999", "N", "K"))
        .expect("serialize t8452");
    assert!(v["t8452InBlock"]["qrycnt"].is_number());
    assert_eq!(v["t8452InBlock"]["exchgubun"], "K");
    let resp: T8452Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8452OutBlock": { "shcode": "010950", "diclose": 60500 },
        "t8452OutBlock1": [{ "date": "20250312", "time": "141900", "close": 60600, "sign": "2" }]
    })).expect("t8452 body round-trips");
    assert_eq!(resp.outblock1[0].close, "60600");
    let empty: T8452Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8452OutBlock": {}, "t8452OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t8453 — (통합)주식챠트(틱/N틱).
#[test]
fn t8453_request_and_response_round_trip() {
    let v = serde_json::to_value(T8453Request::new("010950", "1", "20", "0", "", "99999999", "N", "K"))
        .expect("serialize t8453");
    assert!(v["t8453InBlock"]["ncnt"].is_number());
    assert_eq!(v["t8453InBlock"]["exchgubun"], "K");
    let resp: T8453Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8453OutBlock": { "shcode": "010950", "diclose": 60600 },
        "t8453OutBlock1": [{ "date": "20250312", "time": "142127", "close": 60700, "pricechk": 0 }]
    })).expect("t8453 body round-trips");
    assert_eq!(resp.outblock1[0].close, "60700");
    let empty: T8453Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8453OutBlock": {}, "t8453OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

// === plan -004 batch B — F/O chart/period family offline coverage ============

/// t8464 — 선물옵션차트(틱/n틱). ncnt/qrycnt numbers; openyak row field round-trips.
#[test]
fn t8464_request_and_response_round_trip() {
    let v = serde_json::to_value(T8464Request::new("A0669000", "1", "20", "0", "", "99999999", "N"))
        .expect("serialize t8464");
    assert!(v["t8464InBlock"]["ncnt"].is_number());
    assert!(v["t8464InBlock"]["qrycnt"].is_number());
    assert!(v.get("tr_cont").is_none());
    let resp: T8464Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8464OutBlock": { "shcode": "A0669000", "diclose": 41945 },
        "t8464OutBlock1": [{ "date": "20260626", "close": 41945, "openyak": 312345 }]
    })).expect("t8464 body round-trips");
    assert_eq!(resp.outblock1[0].close, "41945");
    assert_eq!(resp.outblock1[0].openyak, "312345", "open-interest from JSON number");
    let empty: T8464Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8464OutBlock": {}, "t8464OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t8465 — 선물/옵션차트(N분).
#[test]
fn t8465_request_and_response_round_trip() {
    let v = serde_json::to_value(T8465Request::new("A0669000", "1", "20", "0", "", "99999999", "N"))
        .expect("serialize t8465");
    assert!(v["t8465InBlock"]["qrycnt"].is_number());
    let resp: T8465Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8465OutBlock": { "shcode": "A0669000", "diclose": 41945 },
        "t8465OutBlock1": { "date": "20260626", "time": "141900", "close": 41945, "value": 17, "openyak": 1 }
    })).expect("single candle tolerated as array");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "41945");
    let empty: T8465Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8465OutBlock": {}, "t8465OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t8466 — 선물/옵션차트(일주월). gubun-based; qrycnt numeric.
#[test]
fn t8466_request_and_response_round_trip() {
    let v = serde_json::to_value(T8466Request::new("A0669000", "2", "20", "", "99999999"))
        .expect("serialize t8466");
    assert!(v["t8466InBlock"]["qrycnt"].is_number());
    assert_eq!(v["t8466InBlock"]["gubun"], "2");
    assert!(v["t8466InBlock"]["cts_date"].is_string());
    let resp: T8466Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8466OutBlock": { "shcode": "A0669000", "diclose": 41945 },
        "t8466OutBlock1": [{ "date": "20260626", "close": 41945, "value": 100, "openyak": 5 }]
    })).expect("t8466 body round-trips");
    assert_eq!(resp.outblock1[0].close, "41945");
    let empty: T8466Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8466OutBlock": {}, "t8466OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}

/// t8405 — 주식선물기간별주가. cnt numeric; cts_code body cursor; openyak row field.
#[test]
fn t8405_request_and_response_round_trip() {
    let v = serde_json::to_value(T8405Request::new("A0A67000", "20")).expect("serialize t8405");
    assert!(v["t8405InBlock"]["cnt"].is_number(), "cnt is a JSON number");
    assert_eq!(v["t8405InBlock"]["futcheck"], "0");
    assert!(v["t8405InBlock"]["cts_code"].is_string(), "cursor stays a string");
    assert!(v.get("tr_cont").is_none());
    let resp: T8405Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8405OutBlock": { "date": "20260626", "nowfutyn": "Y" },
        "t8405OutBlock1": [{ "date": "20260626", "close": 41945, "volume": 12345, "openyak": 678 }]
    })).expect("t8405 body round-trips");
    assert_eq!(resp.outblock1[0].close, "41945");
    assert_eq!(resp.outblock1[0].openyak, "678", "open-interest from JSON number");
    let empty: T8405Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8405OutBlock": {}, "t8405OutBlock1": []
    })).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}
