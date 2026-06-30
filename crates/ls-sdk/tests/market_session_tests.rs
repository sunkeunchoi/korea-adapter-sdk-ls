//! Market-session (`t1102`) dependency-class tests.
//!
//! Exercises the `t1102` current-price quote against wiremock through REAL
//! `ls-core` dispatch (the mock config injects `base_url`, so the POST hits the
//! mock server). Covers request body shape (no continuation tokens), serde
//! against the spec-derived fixture, the string-or-number field-semantics
//! regression, and `01900` paper-incompatible classification.

use ls_core::{Inner, LsError};
use ls_sdk::market_session::{
    T1302Request, T1302Response, T2216Request, T2216Response,
    T1532Request, T1532Response, T1533Request, T1533Response, T1926Request, T1926Response, T1764Request, T1764Response, T1903Request, T1903Response,
    T1101OutBlock, T1101Request, T1101Response, T1102OutBlock, T1102Request, T1102Response,
    T1531Request, T1531Response, T1537Request, T1537Response, T1601Request, T1601Response,
    T1615Request, T1615Response, T1640Request, T1640Response, T1662Request, T1662Response,
    T1664Request, T1664Response, T1825OutBlock1, T1825Request, T1825Response, T1826OutBlock,
    T1826Request, T1826Response, T1859OutBlock1, T1859Request, T1859Response, T1958Request,
    T1958Response, T1964OutBlock1, T1964Request, T1964Response, T1485Request, T1485Response,
    T1104Request, T1104Response, T1105Request, T1105Response,
    T1511Request, T1511Response, T1516Request, T1516Response, T1901Request, T1901Response,
    T1906Request, T1906Response,
    T8450Request, T8450Response,
    T1638Request, T1638Response,
    T1308Request, T1308Response,
    T1449Request, T1449Response,
    T1621Request, T1621Response,
    T2545Request, T2545Response,
    T8406Request, T8406Response,
    T8407Request, T8407Response,
    T1631Request, T1631Response,
    T1632Request, T1632Response,
    T1633Request, T1633Response,
    T1716Request, T1716Response,
    T1902Request, T1902Response,
    T1904Request, T1904Response,
    T1927Request, T1927Response,
    T1941Request, T1941Response,
    T1702Request, T1702Response,
    T1717Request, T1717Response,
    T1665Request, T1665Response,
    T1471Request, T1471Response,
    T1475Request, T1475Response,
    T1959Request, T1959Response,
    T1950Request, T1950Response, T1954Request, T1954Response,
    T1971Request, T1971Response,
    T1972Request, T1972Response,
    T1974Request, T1974Response,
    T1956Request, T1956Response,
    T1969Request, T1969Response,
    T8424Request, T8424Response,
    T2301Request, T2301Response, T2522OutBlock1, T2522Request, T2522Response, T8401OutBlock,
    T8401Request, T8401Response, T8426OutBlock, T8426Request, T8426Response, T8433OutBlock,
    T8433Request, T8433Response, T8435OutBlock, T8435Request, T8435Response, T8467OutBlock,
    T8467Request, T8467Response, T9943OutBlock, T9943Request, T9943Response, T9944OutBlock,
    T9944Request, T9944Response, T8425Request,
    T8425Response, T8430OutBlock, T8430Request, T8430Response, T8431OutBlock, T8431Request,
    T8431Response, T8436Request, T8436Response, T9905OutBlock1, T9905Request, T9905Response,
    T9907Request, T9907Response, T9942Request, T9942Response,
    T2106Request, T2106Response, T2111OutBlock, T2111Request, T2111Response, T2112OutBlock,
    T2112Request, T2112Response, T8402OutBlock, T8402Request, T8402Response, T8403OutBlock,
    T8403Request, T8403Response, T8434OutBlock1, T8434Request, T8434Response,
    T1988OutBlock, T1988Request, T1988Response, T3102Request, T3102Response, T3320OutBlock,
    T3320Request, T3320Response,
    T8455OutBlock, T8455Request, T8455Response, T8460Request, T8460Response, T8463OutBlock,
    T8463Request, T8463Response,
    G3101OutBlock, G3101Request, G3101Response, G3102Request, G3102Response, G3103Request,
    G3103Response, G3104OutBlock, G3104Request, G3104Response, G3106OutBlock, G3106Request,
    G3106Response, G3190Request, G3190Response,
    O3101OutBlock, O3101Request, O3101Response, O3105OutBlock, O3105Request, O3105Response,
    O3106OutBlock, O3106Request, O3106Response, O3121Request, O3121Response, O3125OutBlock,
    O3125Request, O3125Response, O3126OutBlock, O3126Request, O3126Response,
    O3104Request, O3104Response, O3127Request, O3127Response, T8462Request, T8462Response,
    T8427Request, T8427Response, T2210Request, T2210Response, T2424Request, T2424Response,
    T8428Request, T8428Response,
    T9945Request, T9945Response, T3202Request, T3202Response, T3521Request, T3521Response,
    T0167Request, T0167Response,
};
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The spec-derived `t1102` response fixture (`fixtures/t1102_resp.json`).
const T1102_FIXTURE: &str = include_str!("fixtures/t1102_resp.json");

/// The spec-derived `t0167` server-time fixture (`fixtures/t0167_resp.json`).
const T0167_FIXTURE: &str = include_str!("fixtures/t0167_resp.json");

/// The spec-derived `t1101` response fixture (`fixtures/t1101_resp.json`).
const T1101_FIXTURE: &str = include_str!("fixtures/t1101_resp.json");

/// The spec-derived `t8425` all-themes response fixture (`fixtures/t8425_resp.json`).
const T8425_FIXTURE: &str = include_str!("fixtures/t8425_resp.json");

/// `T8425_POLICY.path` — the mounted endpoint for the all-themes read.
const T8425_PATH: &str = "/stock/sector";

/// The spec-derived `t8436` stock-list response fixture (`fixtures/t8436_resp.json`).
const T8436_FIXTURE: &str = include_str!("fixtures/t8436_resp.json");

/// `T8436_POLICY.path` — the mounted endpoint for the stock-master read.
const T8436_PATH: &str = "/stock/etc";

/// The spec-derived `t1531` response fixture (`fixtures/t1531_resp.json`).
const T1531_FIXTURE: &str = include_str!("fixtures/t1531_resp.json");

/// The spec-derived `t1537` response fixture (`fixtures/t1537_resp.json`).
const T1537_FIXTURE: &str = include_str!("fixtures/t1537_resp.json");

/// `T1531_POLICY.path` / `T1537_POLICY.path` — both theme reads share the sector
/// endpoint (distinguished on the wire by the `tr_cd` header), like `t8425`.
const SECTOR_PATH: &str = "/stock/sector";

/// `T1102_POLICY.path` — the mounted endpoint for the quote TR.
const T1102_PATH: &str = "/stock/market-data";

/// `T1101_POLICY.path` — the mounted endpoint for the order-book TR (shared with
/// `t1102`; the `tr_cd` header distinguishes them).
const T1101_PATH: &str = "/stock/market-data";

/// Build an `LsSdk` whose dispatch is pointed at the mock server.
fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

// ---------------------------------------------------------------------------
// [업종] 시세 — sector/index cluster (Wave A). All share `/indtp/market-data`
// (the `tr_cd` header distinguishes them).
// ---------------------------------------------------------------------------

/// Shared sector endpoint path (`T8424_POLICY.path` … `T1516_POLICY.path`).
const INDTP_PATH: &str = "/indtp/market-data";

const T8424_FIXTURE: &str = include_str!("fixtures/t8424_resp.json");
const T1511_FIXTURE: &str = include_str!("fixtures/t1511_resp.json");
const T1485_FIXTURE: &str = include_str!("fixtures/t1485_resp.json");
const T1516_FIXTURE: &str = include_str!("fixtures/t1516_resp.json");

// ---------------------------------------------------------------------------
// t2301 — 옵션전광판 (option board; F/O). market_session, non-paginated. Keyed by
// a contract month `yyyymm` (월물) + a `gubun` mini/regular selector. Single
// out-block (a representative subset of the 76-field board header).
// ---------------------------------------------------------------------------

/// `T2301_POLICY.path` — the F/O market-data endpoint.
const FO_MARKET_DATA_PATH: &str = "/futureoption/market-data";

const T2301_FIXTURE: &str = include_str!("fixtures/t2301_resp.json");

// ---------------------------------------------------------------------------
// t2522 — 주식선물기초자산조회 (stock-futures underlying-asset master; F/O).
// market_session, non-paginated, no caller input. Single out-block (a
// representative subset of its 6 fields).
// ---------------------------------------------------------------------------

const T2522_FIXTURE: &str = include_str!("fixtures/t2522_resp.json");

// ---------------------------------------------------------------------------
// t8401 — 주식선물마스터조회 (stock-futures master; F/O). market_session,
// non-paginated, no caller input. A single ROW-ARRAY out-block `t8401OutBlock`
// (no separate count header): one stock-futures contract per row. All four
// modeled fields are spec `String` types (no `string_or_number` coercion), so
// the shared contract's number-or-string item does not apply to this TR.
// ---------------------------------------------------------------------------

const T8401_FIXTURE: &str = include_str!("fixtures/t8401_resp.json");

// ---------------------------------------------------------------------------
// t8426 — 상품선물마스터조회 (commodity-futures master; F/O). market_session,
// non-paginated, no caller input. A single ROW-ARRAY out-block `t8426OutBlock`
// (confirmed from the raw capture's `res_example`; no separate count header):
// one commodity-futures contract per row. The wire out-block key is the literal
// `t8426OutBlock` — the normalized baseline collapses it to `response_body`, so
// the rename was taken from the raw capture, not the baseline.
// ---------------------------------------------------------------------------

const T8426_FIXTURE: &str = include_str!("fixtures/t8426_resp.json");

// ---------------------------------------------------------------------------
// t8433 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input. A single ROW-ARRAY out-block `t8433OutBlock`
// (confirmed from the raw capture's `res_example`: rows are direct elements
// under the key, no separate count header / no numbered sub-block): one
// index-option contract per row. The wire out-block key is the literal
// `t8433OutBlock` — the normalized baseline collapses it to `response_body`, so
// the rename was taken from the raw capture, not the baseline.
// ---------------------------------------------------------------------------

const T8433_FIXTURE: &str = include_str!("fixtures/t8433_resp.json");

// ---------------------------------------------------------------------------
// t8435 — 파생종목마스터조회API용 (derivatives master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"MF"` futures / `"MO"`
// options). The out-block is itself a ROW ARRAY (confirmed from the raw
// capture's `res_example`, KTD3) — one derivatives contract per row, the full
// 9 fields.
// ---------------------------------------------------------------------------

const T8435_FIXTURE: &str = include_str!("fixtures/t8435_resp.json");

// ---------------------------------------------------------------------------
// t8467 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"`/`"S"`/`"Q"` or any
// other value → KOSPI200). The out-block is itself a ROW ARRAY (confirmed from
// the raw capture's `res_example`, propertyType `A0005`/Object Array, KTD3) —
// one index-futures contract per row, the full 9 fields.
// ---------------------------------------------------------------------------

const T8467_FIXTURE: &str = include_str!("fixtures/t8467_resp.json");

// ---------------------------------------------------------------------------
// t9943 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"`/`"S"` or any other
// value → KOSPI200). The out-block is itself a ROW ARRAY (confirmed from the raw
// capture's `res_example`, propertyType `A0005`/Object Array, the true wire key
// `t9943OutBlock` per KTD3) — one index-futures contract per row, the 3 spec
// fields (hname/shcode/expcode).
// ---------------------------------------------------------------------------

const T9943_FIXTURE: &str = include_str!("fixtures/t9943_resp.json");

// ---------------------------------------------------------------------------
// t9944 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). The out-block
// is itself a ROW ARRAY (confirmed from the raw capture's `res_example`,
// propertyType Object Array, the true wire key `t9944OutBlock` per KTD3) — one
// index-option contract per row, the 3 spec fields (hname/shcode/expcode).
// ---------------------------------------------------------------------------

const T9944_FIXTURE: &str = include_str!("fixtures/t9944_resp.json");

// ---------------------------------------------------------------------------
// Domestic stock master/reference breadth wave (plan -004). market_session,
// non-paginated, single Object-Array out-blocks via `de_vec_or_single`.
// ---------------------------------------------------------------------------

/// `T9945_POLICY.path` — stock-master endpoint (shared with t1102; tr_cd
/// distinguishes). `T3202_POLICY.path` — the investinfo endpoint.
const STOCK_INVESTINFO_PATH: &str = "/stock/investinfo";

/// The spec-derived fixtures.
const T9945_FIXTURE: &str = include_str!("fixtures/t9945_resp.json");
const T3202_FIXTURE: &str = include_str!("fixtures/t3202_resp.json");

// --- F-O open-window flip wave (plan -001): t8427/t2210/t2424/t8428 --------

const INVESTINFO_PATH: &str = "/stock/investinfo";

#[path = "market_session/quote.rs"]
mod quote;
#[path = "market_session/quote_deriv.rs"]
mod quote_deriv;
#[path = "market_session/investor_flow.rs"]
mod investor_flow;
#[path = "market_session/charts.rs"]
mod charts;
#[path = "market_session/etf.rs"]
mod etf;
#[path = "market_session/elw.rs"]
mod elw;
#[path = "market_session/masters.rs"]
mod masters;
#[path = "market_session/reference.rs"]
mod reference;
#[path = "market_session/ranking.rs"]
mod ranking;
