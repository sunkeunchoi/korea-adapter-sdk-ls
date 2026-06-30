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
    T3518Request, T3518Response,
    O3103Request, O3103Response, O3108Request, O3108Response,
    O3116Request, O3116Response, O3117Request, O3117Response,
    O3123Request, O3123Response, O3128Request, O3128Response,
    O3136Request, O3136Response, O3137Request, O3137Response,
    O3139Request, O3139Response,
    T1310Request, T1310Response, T1404Request, T1404Response,
    T1410Request, T1410Response,
    T1411Request, T1411Response,
    T1488Request, T1488Response,
    T1636Request, T1636Response,
    T1809Request, T1809Response,
    T1109Request, T1109Response,
    T1301Request, T1301Response,
    T1486Request, T1486Response,
    T8454Request, T8454Response,
    T1637Request, T1637Response,
    T1602Request, T1602Response,
    T1603Request, T1603Response,
    T1617Request, T1617Response,
    T1752Request, T1752Response,
    T1771Request, T1771Response,
    T8417Request, T8417Response, T8418Request, T8418Response, T8411Request, T8411Response,
    T8452Request, T8452Response, T8453Request, T8453Response,
    T8464Request, T8464Response, T8465Request, T8465Response, T8466Request, T8466Response,
    T8405Request, T8405Response,
    T1444Request, T1444Response, T1422Request, T1422Response, T1427Request, T1427Response, T1442Request, T1442Response, T1405Request, T1405Response, T1960Request, T1960Response, T1961Request, T1961Response, T1966Request, T1966Response, T1921Request, T1921Response,
    T2541Request, T2541Response, T2214Request, T2214Response,
};
use ls_core::endpoint_policy::{T1310_POLICY, T1404_POLICY, T1410_POLICY, T1411_POLICY, T1488_POLICY, T1636_POLICY, T1809_POLICY, T1514_POLICY, T1109_POLICY, T1301_POLICY, T1486_POLICY, T8454_POLICY, T1637_POLICY, T1602_POLICY, T1603_POLICY, T1617_POLICY, T1752_POLICY, T1771_POLICY, T2541_POLICY, T2214_POLICY};
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

// --- t1514 — 업종기간별추이 (sector period-trend; self-paginated on cts_date) ----

const T1514_FIXTURE: &str = include_str!("fixtures/t1514_resp.json");
const INDTP_PATH: &str = "/indtp/market-data";

// --- t1310 — 주식당일전일분틱조회 (today/prev tick/min chart; self-paginated on cts_time) ---

const T1310_FIXTURE: &str = include_str!("fixtures/t1310_resp.json");
/// `T1310_POLICY.path` / `T1404_POLICY.path` — the mounted endpoint for these
/// `[주식] 시세` reads (plan -003, closed-window flip wave).
const STOCK_MARKET_DATA_PATH: &str = "/stock/market-data";

// --- t1404 — 관리/불성실/투자유의조회 (designation board; self-paginated on cts_shcode) ---

const T1404_FIXTURE: &str = include_str!("fixtures/t1404_resp.json");

// --- t1410 — 초저유동성조회 (ultra-low-liquidity board; self-paginated on cts_shcode) ---

const T1410_FIXTURE: &str = include_str!("fixtures/t1410_resp.json");

// --- t1411 — 증거금율별종목조회 (stocks by margin rate; self-paginated on idx) ---

const T1411_FIXTURE: &str = include_str!("fixtures/t1411_resp.json");
const STOCK_ETC_PATH: &str = "/stock/etc";

// --- t1488 — 예상체결가등락율상위조회 (expected-exec top change rate; self-paginated) ---

const T1488_FIXTURE: &str = include_str!("fixtures/t1488_resp.json");

// --- t1636 — 종목별프로그램매매동향 (per-stock program-trading trend; self-paginated) ---

const T1636_FIXTURE: &str = include_str!("fixtures/t1636_resp.json");
/// `T1636_POLICY.path` — the mounted endpoint for the `[주식] 프로그램`
/// program-trading read (plan -001, closed-window more-flips).
const STOCK_PROGRAM_PATH: &str = "/stock/program";
const STOCK_INVESTOR_PATH: &str = "/stock/investor";
const STOCK_EXCHANGE_PATH: &str = "/stock/exchange";

// --- t1809 — 신호조회 (signal search; self-paginated on the string cts cursor) ---

const T1809_FIXTURE: &str = include_str!("fixtures/t1809_resp.json");
/// `T1809_POLICY.path` — the mounted endpoint for the `[주식] 종목검색`
/// signal-search read (plan -001, closed-window more-flips).
const STOCK_ITEM_SEARCH_PATH: &str = "/stock/item-search";

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

// === plan -003 all-lane wave — overseas-futures(-option) chart/market-data ====
// Nine self-paginated reads on /overseas-futureoption/chart|market-data. Numeric
// request fields (ncnt/readcnt/qrycnt/cts_seq numeric form) serialize as JSON
// numbers (IGW40011 guard, KTD3); string cursors stay strings. Out-blocks
// round-trip a real value, tolerate single-or-array, and empty-00707 is pending.

const OVS_FO_CHART_PATH: &str = "/overseas-futureoption/chart";
const OVS_FO_MKT_PATH: &str = "/overseas-futureoption/market-data";

// --- F-O open-window flip wave (plan -001): t2541 + t2214 -------------------

const FO_INVESTOR_PATH: &str = "/futureoption/investor";
const FO_MD_PATH: &str = "/futureoption/market-data";

#[path = "paginated/breadth_board.rs"]
mod breadth_board;
#[path = "paginated/chart.rs"]
mod chart;
#[path = "paginated/designation_board.rs"]
mod designation_board;
#[path = "paginated/exchange_broker.rs"]
mod exchange_broker;
#[path = "paginated/expected_conclusion.rs"]
mod expected_conclusion;
#[path = "paginated/fo_daily_chart.rs"]
mod fo_daily_chart;
#[path = "paginated/fo_investor_time.rs"]
mod fo_investor_time;
#[path = "paginated/historical_chart.rs"]
mod historical_chart;
#[path = "paginated/invest_opinion.rs"]
mod invest_opinion;
#[path = "paginated/investor.rs"]
mod investor;
#[path = "paginated/item_search.rs"]
mod item_search;
#[path = "paginated/low_liquidity.rs"]
mod low_liquidity;
#[path = "paginated/overseas_futures_chart.rs"]
mod overseas_futures_chart;
#[path = "paginated/overseas_index.rs"]
mod overseas_index;
#[path = "paginated/program_flow.rs"]
mod program_flow;
#[path = "paginated/rank_screen.rs"]
mod rank_screen;
#[path = "paginated/sector_index.rs"]
mod sector_index;
#[path = "paginated/tick_conclusion.rs"]
mod tick_conclusion;
