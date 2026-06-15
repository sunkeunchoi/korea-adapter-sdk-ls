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
use ls_sdk::paginated::{T8412OutBlock1, T8412Request, T8412Response};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// The spec-derived `t8412` response fixture (`fixtures/t8412_resp.json`).
const T8412_FIXTURE: &str = include_str!("fixtures/t8412_resp.json");

/// `T8412_POLICY.path` — the mounted endpoint for the chart TR.
const T8412_PATH: &str = "/stock/chart";

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
