//! Paginated (`t8412`) dependency-class tests.
//!
//! Exercises the SELF-paginated `t8412` chart against wiremock through REAL
//! `ls-core` dispatch (the mock config injects `base_url`). Covers:
//!   - the request body shape (NO `tr_cont`/`tr_cont_key`; `cts_*` ARE in the body),
//!   - `collect_all` walking two pages via response `tr_cont`/`tr_cont_key` headers,
//!   - the single-object-or-array tolerance on `t8412OutBlock1` (`de_vec_or_single`),
//!   - truncation at `max_pages` surfacing `LsError::PaginationLimit`,
//!   - and an explicitly PINNED trading day (no empty-date-defaults-to-today).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ls_core::{Inner, LsConfig, LsError};
use ls_sdk::paginated::{
    T1403Request, T1403Response, T1441Request, T1441Response, T1452Request, T1452Response,
    T1463Request, T1463Response, T1466Request, T1466Response, T1481Request, T1481Response,
    T1482Request, T1482Response, T1489Request, T1489Response, T1492Request, T1492Response,
    T1514Request, T1514Response, T1866Request, T1866Response, T3341Request, T3341Response,
    T8412OutBlock1, T8412Request, T8412Response,
    T1305Request, T1305Response,
    T8410Request, T8410Response, T8451Request, T8451Response, T8419Request, T8419Response,
    T4203Request, T4203Response, T3401Request, T3401Response,
    T1310Request, T1310Response, T1404Request, T1404Response,
};
use ls_core::endpoint_policy::{T1310_POLICY, T1404_POLICY, T1514_POLICY};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// The spec-derived `t8412` response fixture (`fixtures/t8412_resp.json`).
const T8412_FIXTURE: &str = include_str!("fixtures/t8412_resp.json");

/// `T8412_POLICY.path` — the mounted endpoint for the chart TR.
const T8412_PATH: &str = "/stock/chart";

/// The spec-derived `t1452` single-page response fixture (`fixtures/t1452_resp.json`).
const T1452_FIXTURE: &str = include_str!("fixtures/t1452_resp.json");

/// `T1452_POLICY.path` — the mounted endpoint for the rank-screen TRs.
const HIGH_ITEM_PATH: &str = "/stock/high-item";

/// Build a single-page `t1452` top-volume request with permissive filters.
fn t1452_req() -> T1452Request {
    T1452Request::new("0", "0", "0", "0", "0", "0", "0", "0")
}

/// An explicitly pinned trading day (a Friday). Empty date fields default to
/// "today" on the gateway and fail on weekends with a misleading `01715`, so every
/// date-bearing test pins this real weekday.
const PINNED_TRADE_DATE: &str = "20240105";

/// Build a `t8412` request over the pinned date range.
fn pinned_req() -> T8412Request {
    T8412Request::new(
        "078020",
        "1",
        "500",
        "1",
        PINNED_TRADE_DATE,
        PINNED_TRADE_DATE,
        "N",
    )
}

/// Build an `LsSdk` whose dispatch is pointed at the mock server.
fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

/// Build an `LsSdk` with a custom `max_pages` cap (for the truncation test).
fn sdk_with_max_pages(server: &MockServer, max_pages: usize) -> LsSdk {
    let cfg = LsConfig {
        max_pages: Some(max_pages),
        ..mock_config(&server.uri())
    };
    let inner = Inner::new(cfg).expect("valid mock config");
    LsSdk::from_inner(inner)
}

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

/// Two-page responder: page 1 returns `tr_cont: Y` + a `tr_cont_key`, page 2
/// returns `tr_cont: N`. Sequential by hit count (mirrors the `ls-core`
/// pagination test pattern).
struct TwoPageResponder {
    hits: Arc<AtomicUsize>,
}

impl Respond for TwoPageResponder {
    fn respond(&self, _req: &Request) -> ResponseTemplate {
        let n = self.hits.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            ResponseTemplate::new(200)
                .insert_header("tr_cont", "Y")
                .insert_header("tr_cont_key", "page2key")
                .set_body_json(serde_json::json!({
                    "rsp_cd": "00000",
                    "t8412OutBlock": { "shcode": "078020", "cts_date": "20240105" },
                    "t8412OutBlock1": [
                        { "date": "20240105", "time": "090100", "close": 4540 },
                        { "date": "20240105", "time": "090200", "close": 4550 }
                    ]
                }))
        } else {
            ResponseTemplate::new(200)
                .insert_header("tr_cont", "N")
                .insert_header("tr_cont_key", "")
                .set_body_json(serde_json::json!({
                    "rsp_cd": "00000",
                    "t8412OutBlock": { "shcode": "078020", "cts_date": "20240105" },
                    "t8412OutBlock1": [
                        { "date": "20240105", "time": "090300", "close": 4560 }
                    ]
                }))
        }
    }
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

/// Never-stopping responder: always returns `tr_cont: Y`, so `collect_all` runs to
/// the `max_pages` cap.
struct NeverStopResponder {
    hits: Arc<AtomicUsize>,
}

impl Respond for NeverStopResponder {
    fn respond(&self, _req: &Request) -> ResponseTemplate {
        self.hits.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(200)
            .insert_header("tr_cont", "Y")
            .insert_header("tr_cont_key", "more")
            .set_body_json(serde_json::json!({
                "rsp_cd": "00000",
                "t8412OutBlock": { "shcode": "078020", "cts_date": "20240105" },
                "t8412OutBlock1": [ { "date": "20240105", "time": "090100", "close": 4540 } ]
            }))
    }
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

// ---------------------------------------------------------------------------
// t1452 — 거래량상위 (top trading volume). The single-page body-`idx` paginated
// sub-pattern: `idx` is an ordinary in-block field (a JSON number), NOT a
// `#[serde(skip)]` header cursor; dispatch is one `post_paginated` with empty
// `tr_cont`/`tr_cont_key` headers; out-rows tolerate single-or-array.
// ---------------------------------------------------------------------------

/// Covers R5. The `t1452` request serializes the body `idx` cursor INSIDE
/// `t1452InBlock` as a JSON number at the first-page convention (`0`), and the
/// `tr_cont`/`tr_cont_key` header cursors are `#[serde(skip)]` — absent from the
/// body (the divergence from `t8412` the single-page sub-pattern depends on).
#[test]
fn t1452_request_serializes_idx_in_block_and_no_continuation_in_body() {
    let value = serde_json::to_value(t1452_req()).expect("serialize t1452 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key (the in-block)");
    let inblock = &value["t1452InBlock"];

    // idx rides IN the body, as a JSON number, at the first-page convention.
    assert_eq!(inblock["idx"], 0, "idx serializes as a number in the in-block");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    // The header cursors never serialize into the body.
    assert!(value.get("tr_cont").is_none(), "tr_cont not in the body");
    assert!(value.get("tr_cont_key").is_none(), "tr_cont_key not in the body");
    assert!(inblock.get("tr_cont").is_none(), "tr_cont not in the in-block");
}

/// Covers R2, R5. A single page deserializes through REAL `post_paginated`
/// dispatch: the request sends `tr_cont: N` (empty cursor) and the response's
/// summary `idx` + ranked-row array round-trip with mixed number/string wire types.
#[tokio::test]
async fn top_volume_deserializes_single_page() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(HIGH_ITEM_PATH))
        .and(header("tr_cd", "t1452"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1452_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .paginated()
        .top_volume(&t1452_req())
        .await
        .expect("t1452 top_volume single page should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.idx, "20", "summary next-page idx round-trips");
    assert_eq!(resp.outblock1.len(), 2, "both ranked rows round-trip");
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[0].price, "71500", "price (from number)");
    assert_eq!(resp.outblock1[1].price, "185000", "price (from string)");
}

/// Covers R2. A single ranked row (not an array) is tolerated as a one-element
/// Vec via `de_vec_or_single`; an empty result set (`00707`) deserializes as the
/// pending case.
#[test]
fn t1452_single_or_array_and_empty_pending() {
    let single: T1452Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1452OutBlock": { "idx": 1 },
        "t1452OutBlock1": { "hname": "단일", "shcode": "000660", "price": 100 }
    }))
    .expect("single row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "000660");

    let empty: T1452Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1452OutBlock": { "idx": 0 },
        "t1452OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty is the pending case, not a flip");
}

// ---------------------------------------------------------------------------
// t1866 — 서버저장조건 리스트조회 (saved-condition spine producer). Body-cursor
// single-page; the cursor is the STRING pair cont/cont_key (not a numeric idx),
// and it takes caller inputs (user_id/gb/group_name). Its out-rows carry the
// query_index that keys the t1859/t1860 condition search.
// ---------------------------------------------------------------------------

/// Covers R5/R7. `t1866::new` serializes its caller inputs and the body cursor
/// INSIDE `t1866InBlock`, with the `tr_cont`/`tr_cont_key` header cursors
/// `#[serde(skip)]` — absent from the body (the single-page convention).
#[test]
fn t1866_request_serializes_inputs_in_block_and_skips_header_cursors() {
    let value = serde_json::to_value(T1866Request::new("d00000")).expect("serialize t1866 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key (the in-block)");
    let inblock = &value["t1866InBlock"];

    assert_eq!(inblock["user_id"], "d00000", "user_id rides in the in-block");
    assert_eq!(inblock["gb"], "0", "gb defaults to list-all");
    assert_eq!(inblock["group_name"], "", "group_name empty = all groups");
    // Body cursor present and EMPTY on the first page.
    assert_eq!(inblock["cont"], "", "first-page cont is empty");
    assert_eq!(inblock["cont_key"], "", "first-page cont_key is empty");
    // Header cursors never serialize into the body.
    assert!(value.get("tr_cont").is_none(), "tr_cont not in the body");
    assert!(inblock.get("tr_cont").is_none(), "tr_cont not in the in-block");
}

/// Covers R5/R8. A success body with one saved condition deserializes under the
/// `t1866OutBlock1` rename key with `query_index` populated (the value the
/// t1859/t1860 chain consumes); an empty `00707` deserializes as the pending
/// (spine-input-unavailable) case, not a flip.
#[test]
fn t1866_deserializes_query_index_rows_and_empty_pending() {
    let single: T1866Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1866OutBlock": { "result_count": 1, "cont": "N", "cont_key": "" },
        "t1866OutBlock1": { "query_index": "000000000001", "group_name": "그룹", "query_name": "조건1" }
    }))
    .expect("single saved-condition row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(
        single.outblock1[0].query_index, "000000000001",
        "query_index populated — the modeled discovery-edge value"
    );

    let empty: T1866Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1866OutBlock": { "result_count": 0, "cont": "N", "cont_key": "" },
        "t1866OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock1.is_empty(),
        "no saved condition = spine-input-unavailable pending, not a flip"
    );
}

// ---------------------------------------------------------------------------
// Remaining single-page paginated TRs (t1403/t1441/t1463/t1466/t1489/t1492).
// They share t1452's sub-pattern; these compact offline tests guard each TR's
// per-TR serde(rename) keys (a typo there silently drops the out-rows) and the
// idx-in-block-as-number request shape.
// ---------------------------------------------------------------------------

/// A representative ranked-row JSON object (mixed wire types).
fn rank_row_json() -> serde_json::Value {
    serde_json::json!({
        "hname": "삼성전자", "shcode": "005930", "price": 71500,
        "sign": "2", "change": 800, "diff": "1.13", "volume": "12345678"
    })
}

/// Each remaining paginated Response deserializes a one-row single-page body
/// under its OWN `txxxxOutBlock1` rename key, with the row's fields populated —
/// guarding against a per-TR rename typo that would silently drop the rows.
#[test]
fn remaining_paginated_responses_deserialize_with_correct_rename_keys() {
    let r = rank_row_json();

    let t1403: T1403Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1403OutBlock": { "idx": 10 }, "t1403OutBlock1": [r.clone()]
    })).expect("t1403 body");
    assert_eq!(t1403.outblock1.len(), 1);
    assert_eq!(t1403.outblock1[0].shcode, "005930");
    assert_eq!(t1403.outblock.idx, "10");

    let t1441: T1441Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1441OutBlock": { "idx": 10 }, "t1441OutBlock1": [r.clone()]
    })).expect("t1441 body");
    assert_eq!(t1441.outblock1[0].price, "71500");

    let t1463: T1463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1463OutBlock": { "idx": 10 }, "t1463OutBlock1": [r.clone()]
    })).expect("t1463 body");
    assert_eq!(t1463.outblock1[0].volume, "12345678");

    let t1466: T1466Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1466OutBlock": { "hhmm": "1530", "idx": 10 },
        "t1466OutBlock1": [r.clone()]
    })).expect("t1466 body");
    assert_eq!(t1466.outblock.hhmm, "1530", "t1466 summary carries hhmm");
    assert_eq!(t1466.outblock1[0].shcode, "005930");

    let t1489: T1489Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1489OutBlock": { "idx": 10 }, "t1489OutBlock1": [r.clone()]
    })).expect("t1489 body");
    assert_eq!(t1489.outblock1[0].hname, "삼성전자");

    let t1492: T1492Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1492OutBlock": { "idx": 10 }, "t1492OutBlock1": [r.clone()]
    })).expect("t1492 body");
    assert_eq!(t1492.outblock1[0].shcode, "005930");

    // A single out-row object (not array) is tolerated, and an empty set is the
    // pending case — spot-checked on t1441.
    let single: T1441Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1441OutBlock": { "idx": 0 }, "t1441OutBlock1": r
    })).expect("single row");
    assert_eq!(single.outblock1.len(), 1);
    let empty: T1492Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1492OutBlock1": []
    })).expect("empty");
    assert!(empty.outblock1.is_empty());
}

/// Each remaining paginated request serializes its `idx` cursor as a JSON number
/// INSIDE its in-block, with the header cursors absent from the body.
#[test]
fn remaining_paginated_requests_serialize_idx_in_block() {
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("t1403InBlock", serde_json::to_value(T1403Request::new("0", "202401", "202612")).unwrap()),
        ("t1441InBlock", serde_json::to_value(T1441Request::new("0","1","1","0","0","0","0","0","1")).unwrap()),
        ("t1463InBlock", serde_json::to_value(T1463Request::new("0","0","0","0","0","0","0","1")).unwrap()),
        ("t1466InBlock", serde_json::to_value(T1466Request::new("0","1","1","0","0","0","0","0","1")).unwrap()),
        ("t1489InBlock", serde_json::to_value(T1489Request::new("0","0","000000000000","0","0","0")).unwrap()),
        ("t1492InBlock", serde_json::to_value(T1492Request::new("0","1","0","0")).unwrap()),
    ];
    for (key, value) in cases {
        let obj = value.as_object().expect("request object");
        assert_eq!(obj.len(), 1, "{key}: exactly one top-level key");
        let inblock = &value[key];
        assert!(inblock["idx"].is_number(), "{key}: idx serializes as a number");
        assert_eq!(inblock["idx"], 0, "{key}: idx at first-page convention");
        assert!(value.get("tr_cont").is_none(), "{key}: no tr_cont in body");
        assert!(value.get("tr_cont_key").is_none(), "{key}: no tr_cont_key in body");
    }
}

// ---------------------------------------------------------------------------
// t3341 — 재무순위종합 (financial ranking; Wave 2). Single-page body-idx
// sub-pattern (KTD-5): idx is an ordinary in-block field serialized as a JSON
// number, NOT #[serde(skip)]; the header cursors are skipped.
// ---------------------------------------------------------------------------

/// Covers KTD-5. The `t3341` request serializes the body `idx` INSIDE the
/// in-block as a JSON number at the first-page convention (`0`), with documented
/// gubun defaults and no header continuation leaking into the body.
#[test]
fn t3341_request_serializes_idx_in_block_as_number() {
    let value = serde_json::to_value(T3341Request::new()).expect("serialize t3341 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t3341InBlock at the top level");
    let inblock = &value["t3341InBlock"];
    assert_eq!(inblock["gubun"], "0", "all markets");
    assert_eq!(inblock["gubun1"], "1", "sales-growth rank");
    assert_eq!(inblock["gubun2"], "1", "fixed comparison");
    assert_eq!(inblock["idx"], 0, "idx serializes as a number at first-page convention");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(value.get("tr_cont_key").is_none(), "no tr_cont_key in the body");
}

/// Covers KTD-5. A representative success deserializes: the summary (count +
/// next-page `idx`) and the ranked-row array round-trip with mixed number/string
/// wire types; single-or-array tolerated; empty is the pending case.
#[test]
fn t3341_response_round_trips_single_or_array_and_empty() {
    let resp: T3341Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3341OutBlock": { "cnt": 2, "idx": "100" },
        "t3341OutBlock1": [
            { "rank": 1, "hname": "삼성전자", "shcode": "005930", "salesgrowth": 12.3,
              "eps": "5000", "roe": 15.1, "per": "10.5" },
            { "rank": "2", "hname": "SK하이닉스", "shcode": 660, "salesgrowth": "8.1",
              "eps": 3000, "roe": "12.0", "per": 8.2 }
        ]
    }))
    .expect("representative t3341 success must deserialize");
    assert_eq!(resp.outblock.cnt, "2", "summary count populated");
    assert_eq!(resp.outblock.idx, "100", "next-page idx captured");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[1].shcode, "660", "shcode from JSON number");
    assert_eq!(resp.outblock1[0].rank, "1", "rank from JSON number");

    let single: T3341Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3341OutBlock": { "cnt": 1, "idx": "0" },
        "t3341OutBlock1": { "rank": "1", "hname": "삼성전자", "shcode": "005930" }
    }))
    .expect("single ranked row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T3341Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3341OutBlock": { "cnt": 0, "idx": "0" }, "t3341OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

// --- t1514 — 업종기간별추이 (sector period-trend; self-paginated on cts_date) ----

const T1514_FIXTURE: &str = include_str!("fixtures/t1514_resp.json");
const INDTP_PATH: &str = "/indtp/market-data";

/// Covers R4, R8. The genuinely-numeric `cnt` serializes as a JSON **number**
/// (the string form returns `IGW40011`, confirmed by the U1 probe), while the
/// `cts_date` cursor and the identifier fields stay **strings**. Header cursors
/// are skipped (self-paginated body cursor).
#[test]
fn t1514_request_serializes_cnt_as_number_cts_date_as_string() {
    let value = serde_json::to_value(T1514Request::new("001")).expect("serialize t1514 request");
    let inblock = &value["t1514InBlock"];
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number, not a string");
    assert!(
        inblock["cts_date"].is_string(),
        "cts_date cursor stays a string"
    );
    assert!(inblock["upcode"].is_string(), "upcode stays a string");
    assert_eq!(inblock["upcode"], "001");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R2, R5, R8. The first-page fixture deserializes through REAL paginated
/// dispatch: the `t1514OutBlock` cursor + the `t1514OutBlock1` array round-trip.
#[tokio::test]
async fn t1514_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1514"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1514_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .sector_trend(&T1514Request::new("001"))
        .await
        .expect("t1514 sector_trend should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page trend rows round-trip");
    assert!(!resp.outblock1[0].date.is_empty(), "real non-default date");
    assert_eq!(resp.outblock1[0].upcode, "001", "per-row sector code (string)");
}

/// Covers R5, R8. The trend array tolerates single-or-array + empty (pending) forms.
#[test]
fn t1514_response_round_trips_single_or_array_and_empty() {
    let single: T1514Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1514OutBlock": { "cts_date": "20230605" },
        "t1514OutBlock1": { "date": "20230605", "jisu": "2610.62", "volume": 263165, "upcode": "001" }
    }))
    .expect("single trend row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].volume, "263165", "volume from JSON number");

    let empty: T1514Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1514OutBlock": { "cts_date": "" }, "t1514OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (origin R8): `T1514_POLICY` must be a real paginated policy
/// — a self-paginating TR shipping `has_pagination: false` would dispatch
/// single-page silently. The `policy_index_crosscheck` test enforces the
/// `self_paginated ⟹ has_pagination` mirror only for consts in its array; this
/// asserts the const's own shape so a regression fails a test, never silently.
#[test]
fn t1514_policy_is_registered_and_paginated() {
    assert_eq!(T1514_POLICY.tr_code, "t1514");
    assert_eq!(T1514_POLICY.path, "/indtp/market-data");
    assert!(
        T1514_POLICY.has_pagination,
        "t1514 self-paginates (cts_date) — policy must thread continuation"
    );
}

// --- t1310 — 주식당일전일분틱조회 (today/prev tick/min chart; self-paginated on cts_time) ---

const T1310_FIXTURE: &str = include_str!("fixtures/t1310_resp.json");
/// `T1310_POLICY.path` / `T1404_POLICY.path` — the mounted endpoint for these
/// `[주식] 시세` reads (plan -003, closed-window flip wave).
const STOCK_MARKET_DATA_PATH: &str = "/stock/market-data";

/// Covers R3. The `t1310` request serializes every in-block field as a JSON
/// **string** (no `string_as_number` / IGW40011 slot), with the `cts_time` cursor
/// kept a string and the header continuation skipped from the body.
#[test]
fn t1310_request_serializes_all_fields_as_strings() {
    let value = serde_json::to_value(T1310Request::new("005930")).expect("serialize t1310 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1310InBlock at the top level");
    let inblock = &value["t1310InBlock"];
    for f in ["daygb", "timegb", "shcode", "endtime", "cts_time", "exchgubun"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["shcode"], "005930");
    assert_eq!(inblock["cts_time"], "", "first-page cts_time cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1310OutBlock` cursor + the `t1310OutBlock1` array round-trip, and a modeled
/// non-key field (`price`) holds a real non-default value.
#[tokio::test]
async fn t1310_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1310"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1310_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .daily_tick_chart(&T1310Request::new("005930"))
        .await
        .expect("t1310 daily_tick_chart should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "tick/min bars round-trip");
    assert_eq!(resp.outblock1[0].price, "3685", "real non-default price (from JSON number)");
    assert_eq!(resp.outblock1[0].chetime, "102700", "real non-default time");
    assert_eq!(resp.outblock.cts_time, "100700", "next-page cursor round-trips");
}

/// Covers R4. The tick array tolerates single-or-array + empty (pending) forms,
/// and `string_or_number` parses `price`/`volume` from BOTH string and number.
#[test]
fn t1310_response_round_trips_single_or_array_and_empty() {
    let single: T1310Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1310OutBlock": { "cts_time": "100700" },
        "t1310OutBlock1": { "chetime": "100800", "price": "3685", "volume": "300647" }
    }))
    .expect("single tick row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "3685", "price from JSON string");
    assert_eq!(single.outblock1[0].volume, "300647", "volume from JSON string");

    let number: T1310Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1310OutBlock": { "cts_time": "100700" },
        "t1310OutBlock1": [{ "chetime": "100800", "price": 3685, "volume": 300647 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock1[0].price, "3685", "price from JSON number");
    assert_eq!(number.outblock1[0].volume, "300647", "volume from JSON number");

    let empty: T1310Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1310OutBlock": { "cts_time": "" }, "t1310OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (R3/R8): `T1310_POLICY` must be a real paginated policy — a
/// self-paginating TR shipping `has_pagination: false` would dispatch single-page
/// silently.
#[test]
fn t1310_policy_is_registered_and_paginated() {
    assert_eq!(T1310_POLICY.tr_code, "t1310");
    assert_eq!(T1310_POLICY.path, "/stock/market-data");
    assert!(
        T1310_POLICY.has_pagination,
        "t1310 self-paginates (cts_time) — policy must thread continuation"
    );
    assert!(!T1310_POLICY.is_order, "t1310 is a non-order read");
}

// --- t1404 — 관리/불성실/투자유의조회 (designation board; self-paginated on cts_shcode) ---

const T1404_FIXTURE: &str = include_str!("fixtures/t1404_resp.json");

/// Covers R3. The `t1404` request serializes every in-block field as a JSON
/// **string** (no `string_as_number` slot), with the `cts_shcode` cursor at its
/// first-page `" "` convention and the header continuation skipped.
#[test]
fn t1404_request_serializes_all_fields_as_strings() {
    let value = serde_json::to_value(T1404Request::new()).expect("serialize t1404 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1404InBlock at the top level");
    let inblock = &value["t1404InBlock"];
    for f in ["gubun", "jongchk", "cts_shcode"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["gubun"], "0");
    assert_eq!(inblock["jongchk"], "1");
    assert_eq!(inblock["cts_shcode"], " ", "first-page cts_shcode cursor");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1404OutBlock` summary cursor + the TOP-LEVEL sibling `t1404OutBlock1` array
/// round-trip, and a modeled non-key field (`hname`) holds a real non-default value.
#[tokio::test]
async fn t1404_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1404"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1404_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .designation_board(&T1404Request::new())
        .await
        .expect("t1404 designation_board should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "designation rows round-trip");
    assert_eq!(resp.outblock1[0].hname, "흥국화재2우B", "real non-default Korean name");
    assert_eq!(resp.outblock1[0].reason, "5102", "real non-default reason code");
    assert_eq!(resp.outblock1[0].price, "16500", "price from JSON number");
    assert_eq!(resp.outblock1[0].tprice, "16200", "designation-date price from JSON number");
}

/// Covers R4. The designation array tolerates single-or-array + empty (the
/// concrete `t1404` empty-board risk, R7), and `string_or_number` parses
/// `price`/`change`/`volume` from BOTH string and number.
#[test]
fn t1404_response_round_trips_single_or_array_and_empty() {
    let single: T1404Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1404OutBlock": { "cts_shcode": "000547" },
        "t1404OutBlock1": { "hname": "JTC", "shcode": "950170", "price": "3920", "volume": "5492" }
    }))
    .expect("single designation row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock.cts_shcode, "000547", "next-page cursor round-trips (non-empty)");
    assert_eq!(single.outblock1[0].price, "3920", "price from JSON string");

    let number: T1404Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1404OutBlock": { "cts_shcode": "" },
        "t1404OutBlock1": [{ "hname": "JTC", "shcode": "950170", "price": 3920, "volume": 5492 }]
    }))
    .expect("numeric price/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "5492", "volume from JSON number");

    let empty: T1404Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1404OutBlock": { "cts_shcode": "" }, "t1404OutBlock1": []
    }))
    .expect("empty board deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case (R7)");
}

/// Registration guard (R3/R8): `T1404_POLICY` must be a real paginated policy.
#[test]
fn t1404_policy_is_registered_and_paginated() {
    assert_eq!(T1404_POLICY.tr_code, "t1404");
    assert_eq!(T1404_POLICY.path, "/stock/market-data");
    assert!(
        T1404_POLICY.has_pagination,
        "t1404 self-paginates (cts_shcode) — policy must thread continuation"
    );
    assert!(!T1404_POLICY.is_order, "t1404 is a non-order read");
}

// ---------------------------------------------------------------------------
// t1481 — 시간외등락율상위 (after-hours top change rate; U2 reach wave). Single-page
// body-`idx` sub-pattern (KTD-5/KTD-8): idx is an ordinary in-block field
// serialized as a JSON number, NOT #[serde(skip)]; the header cursors are skipped.
// Out-block shape (single `idx` summary + `t1481OutBlock1` row ARRAY) read from
// the raw capture.
// ---------------------------------------------------------------------------

/// Covers contract item 4 + KTD-4. The `t1481` request serializes the body `idx`
/// INSIDE `t1481InBlock` as a JSON number at the first-page convention (`0`), with
/// the length-1 string flags kept as strings and no header continuation leaking
/// into the body.
#[test]
fn t1481_request_serializes_idx_in_block_as_number() {
    let value = serde_json::to_value(T1481Request::new("1", "1", "1", "1"))
        .expect("serialize t1481 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1481InBlock at the top level");
    let inblock = &value["t1481InBlock"];
    assert_eq!(inblock["gubun1"], "1", "gubun1 stays a string flag");
    assert_eq!(inblock["gubun2"], "1");
    assert_eq!(inblock["jongchk"], "1");
    assert_eq!(inblock["volume"], "1", "volume is a length-1 string flag");
    assert_eq!(inblock["idx"], 0, "idx serializes as a number at first-page convention");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(value.get("tr_cont_key").is_none(), "no tr_cont_key in the body");
}

/// Covers contract items 1, 2, 6. A representative success (from the raw capture)
/// deserializes through REAL `post_paginated` dispatch: the summary next-page
/// `idx` and the `t1481OutBlock1` row array round-trip with mixed number/string
/// wire types, and the canonical row field `hname` (한글명, KTD-6) holds its EXACT
/// expected value.
#[tokio::test]
async fn t1481_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(HIGH_ITEM_PATH))
        .and(header("tr_cd", "t1481"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                // The exact wire shape from the raw capture: a single `idx` summary
                // object and a two-row `t1481OutBlock1` ARRAY (mixed number/string).
                .set_body_string(
                    r#"{
                        "rsp_cd": "00000",
                        "t1481OutBlock": { "idx": 20 },
                        "t1481OutBlock1": [
                            { "volume": 2136, "bidrem1": 301, "price": 10490, "change": 445,
                              "offerrem1": 764, "shcode": "449180", "sign": "5", "diff": "-04.07",
                              "bidho1": 10305, "value": 22493050, "hname": "KODEX 미국S&P500(H)",
                              "offerho1": 10485 },
                            { "volume": 369875, "bidrem1": 9738, "price": 935, "change": 8,
                              "offerrem1": 248, "shcode": "031820", "sign": "5", "diff": "-00.85",
                              "bidho1": 935, "value": 346240565, "hname": "콤텍시스템",
                              "offerho1": 936 }
                        ]
                    }"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .after_hours_top_change_rate(&T1481Request::new("1", "1", "1", "1"))
        .await
        .expect("t1481 after_hours_top_change_rate should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.idx, "20", "summary next-page idx round-trips");
    assert_eq!(resp.outblock1.len(), 2, "both ranked rows round-trip");
    assert_eq!(
        resp.outblock1[0].hname, "KODEX 미국S&P500(H)",
        "canonical row field hname (한글명) holds its exact value"
    );
    assert_eq!(resp.outblock1[0].shcode, "449180");
    assert_eq!(resp.outblock1[0].price, "10490", "price from JSON number");
    assert_eq!(resp.outblock1[0].diff, "-04.07", "diff from JSON string");
    assert_eq!(resp.outblock1[1].volume, "369875", "volume from JSON number");
}

/// Covers contract items 2, 3, 6. A single out-row object (not an array) is
/// tolerated as a one-element Vec via `de_vec_or_single`; `string_or_number` parses
/// a numeric field from BOTH string and number JSON; an empty result set (`00707`)
/// deserializes as the pending case (not a flip).
#[test]
fn t1481_single_or_array_string_or_number_and_empty_pending() {
    // string-form price (parsed via string_or_number) + single object → one-element Vec.
    let single: T1481Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1481OutBlock": { "idx": 1 },
        "t1481OutBlock1": { "hname": "단일", "shcode": "000660", "price": "100" }
    }))
    .expect("single row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "000660");
    assert_eq!(single.outblock1[0].price, "100", "price parsed from a JSON string");

    // number-form price (the other string_or_number branch).
    let numeric: T1481Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1481OutBlock": { "idx": 2 },
        "t1481OutBlock1": [{ "hname": "수치", "shcode": "005930", "price": 71500 }]
    }))
    .expect("number-form price deserializes");
    assert_eq!(numeric.outblock1[0].price, "71500", "price parsed from a JSON number");

    let empty: T1481Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1481OutBlock": { "idx": 0 },
        "t1481OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty is the pending case, not a flip");
}

// ---------------------------------------------------------------------------
// t1482 — 시간외거래량상위 (after-hours top volume; U2 reach wave). Same single-page
// body-`idx` sub-pattern as t1481; the in-block carries a numeric `sort_gbn` sort
// flag serialized as a JSON number. Out-block shape (single `idx` summary +
// `t1482OutBlock1` row ARRAY) read from the raw capture.
// ---------------------------------------------------------------------------

/// Covers contract item 4 + KTD-4. The `t1482` request serializes BOTH the numeric
/// `sort_gbn` sort flag and the body `idx` cursor INSIDE `t1482InBlock` as JSON
/// numbers (first-page `idx` = `0`), keeps the string flags as strings, and leaks
/// no header continuation into the body.
#[test]
fn t1482_request_serializes_sort_gbn_and_idx_as_numbers() {
    let value =
        serde_json::to_value(T1482Request::new("0", "1", "1")).expect("serialize t1482 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1482InBlock at the top level");
    let inblock = &value["t1482InBlock"];
    assert!(
        inblock["sort_gbn"].is_number(),
        "sort_gbn serializes as a JSON number, not a string"
    );
    assert_eq!(inblock["sort_gbn"], 0, "sort_gbn at the requested value");
    assert_eq!(inblock["gubun"], "1", "gubun stays a string flag");
    assert_eq!(inblock["jongchk"], "1");
    assert_eq!(inblock["idx"], 0, "idx serializes as a number at first-page convention");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(value.get("tr_cont_key").is_none(), "no tr_cont_key in the body");
}

/// Covers contract items 1, 2, 6. A representative success (from the raw capture)
/// deserializes through REAL `post_paginated` dispatch: the summary next-page `idx`
/// and the `t1482OutBlock1` row array round-trip with mixed number/string wire
/// types, and the canonical row field `hname` (종목명, KTD-6) holds its EXACT value.
#[tokio::test]
async fn t1482_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(HIGH_ITEM_PATH))
        .and(header("tr_cd", "t1482"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                // The exact wire shape from the raw capture: a single `idx` summary
                // object and a two-row `t1482OutBlock1` ARRAY (mixed number/string).
                .set_body_string(
                    r#"{
                        "rsp_cd": "00000",
                        "t1482OutBlock": { "idx": 20 },
                        "t1482OutBlock1": [
                            { "volume": 2413264, "vol": "000.29", "price": 2485, "change": 10,
                              "shcode": "252670", "sign": "5", "diff": "-00.40", "value": 5998142760,
                              "hname": "KODEX 200선물인버스2" },
                            { "volume": 116309, "vol": "000.03", "price": 1120, "change": 5,
                              "shcode": "530031", "sign": "2", "diff": "000.45", "value": 130067985,
                              "hname": "삼성 레버리지 WTI원유" }
                        ]
                    }"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .after_hours_top_volume(&T1482Request::new("0", "1", "1"))
        .await
        .expect("t1482 after_hours_top_volume should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.idx, "20", "summary next-page idx round-trips");
    assert_eq!(resp.outblock1.len(), 2, "both ranked rows round-trip");
    assert_eq!(
        resp.outblock1[0].hname, "KODEX 200선물인버스2",
        "canonical row field hname (종목명) holds its exact value"
    );
    assert_eq!(resp.outblock1[0].shcode, "252670");
    assert_eq!(resp.outblock1[0].price, "2485", "price from JSON number");
    assert_eq!(resp.outblock1[1].volume, "116309", "volume from JSON number");
}

/// Covers contract items 2, 3, 6. A single out-row object is tolerated as a
/// one-element Vec; `string_or_number` parses a numeric field from BOTH string and
/// number JSON; an empty result (`00707`) deserializes as the pending case.
#[test]
fn t1482_single_or_array_string_or_number_and_empty_pending() {
    let single: T1482Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1482OutBlock": { "idx": 1 },
        "t1482OutBlock1": { "hname": "단일", "shcode": "000660", "volume": "100" }
    }))
    .expect("single row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "000660");
    assert_eq!(single.outblock1[0].volume, "100", "volume parsed from a JSON string");

    let numeric: T1482Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1482OutBlock": { "idx": 2 },
        "t1482OutBlock1": [{ "hname": "수치", "shcode": "005930", "volume": 9999 }]
    }))
    .expect("number-form volume deserializes");
    assert_eq!(numeric.outblock1[0].volume, "9999", "volume parsed from a JSON number");

    let empty: T1482Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1482OutBlock": { "idx": 0 },
        "t1482OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty is the pending case, not a flip");
}

// ===========================================================================
// Domestic stock / sector master/reference charts + invest-opinion (plan -004).
// Self-paginated on the body cts_* cursor; single-page facade scope. Numeric
// request counts (qrycnt/ncnt) serialize as JSON numbers; header cursors skipped.
// ===========================================================================

const INDTP_CHART_PATH: &str = "/indtp/chart";
const STOCK_INVESTINFO_PATH: &str = "/stock/investinfo";

const T8410_FIXTURE: &str = include_str!("fixtures/t8410_resp.json");
const T8451_FIXTURE: &str = include_str!("fixtures/t8451_resp.json");
const T8419_FIXTURE: &str = include_str!("fixtures/t8419_resp.json");
const T4203_FIXTURE: &str = include_str!("fixtures/t4203_resp.json");
const T3401_FIXTURE: &str = include_str!("fixtures/t3401_resp.json");

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

// --- t3401 — 투자의견 --------------------------------------------------------

#[test]
fn t3401_request_serializes_to_inblock() {
    let value = serde_json::to_value(T3401Request::new("011200")).expect("serialize t3401 request");
    let inblock = &value["t3401InBlock"];
    assert_eq!(inblock["shcode"], "011200");
    assert!(inblock["cts_date"].is_string(), "cts_date cursor stays a string");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
}

#[tokio::test]
async fn t3401_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTINFO_PATH))
        .and(header("tr_cd", "t3401"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T3401_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .investment_opinions(&T3401Request::new("011200"))
        .await
        .expect("t3401 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock1.len() >= 2, "opinion rows round-trip");
    assert_eq!(resp.outblock1[0].bopn, "HOLD", "canonical 투자의견변경후");
    assert_eq!(resp.outblock1[0].tradname, "메리츠", "회원사명");
    assert_eq!(resp.outblock1[0].noga, "24000", "목표가변경후 from JSON number");
}

#[test]
fn t3401_response_round_trips_single_or_array_and_empty() {
    let single: T3401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3401OutBlock": { "price": 17800 },
        "t3401OutBlock1": { "date": "20230209", "bopn": "BUY", "shcode": "011200", "noga": 24000 }
    }))
    .expect("single opinion row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].bopn, "BUY");

    let empty: T3401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3401OutBlock": {}, "t3401OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}
