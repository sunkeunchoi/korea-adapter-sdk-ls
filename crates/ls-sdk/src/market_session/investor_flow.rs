//! Investor-type, program-trade, foreign/institution, credit & short-selling flow reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use investor_flow::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// t1716 — 외인기관종목별동향 (foreign/institution by-issue trend). market_session
// domestic-stock read; path /stock/frgr-itt, group [주식] 외인/기관. 9-field request —
// shcode, gubun, fromdt/todt date range, prapp (PR감산적용율 — a Number, serialized as
// a JSON number via `string_as_number`), prgubun, orggubun, frggubun, exchgubun.
// Response: a single repeated `t1716OutBlock` date-series ARRAY (one row per date:
// close/volume + the foreign/institution/program flows) tolerated single-or-array
// via `ls_core::de_vec_or_single`. No secondary block.
// ---------------------------------------------------------------------------

/// Input block for `t1716` — the foreign/institution by-issue trend filters.
/// `prapp` (PR감산적용율) is a spec `Number` and serializes as a JSON number via
/// [`ls_core::string_as_number`] (else the gateway returns `IGW40011`); every other
/// field is an ordinary request String. See [`T1716Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1716InBlock {
    /// Issue code / 종목코드.
    pub shcode: String,
    /// Division / 구분 (`0`:일간순매수, `1`:기간누적순매수).
    pub gubun: String,
    /// Start date / 시작일자 (YYYYMMDD).
    pub fromdt: String,
    /// End date / 종료일자 (YYYYMMDD).
    pub todt: String,
    /// PR-reduction rate / PR감산적용율 (a `Number` — serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub prapp: String,
    /// PR-apply division / PR적용구분 (`0`:적용안함, `1`:적용).
    pub prgubun: String,
    /// Institution-apply division / 기관적용.
    pub orggubun: String,
    /// Foreign-apply division / 외인적용.
    pub frggubun: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1716` request — serializes to `{"t1716InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1716Request {
    #[serde(rename = "t1716InBlock")]
    pub inblock: T1716InBlock,
}

impl T1716Request {
    /// Build a `t1716` foreign/institution by-issue trend request from the caller
    /// filters. `prapp` is a numeric request field (e.g. `"0"`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        fromdt: impl Into<String>,
        todt: impl Into<String>,
        prapp: impl Into<String>,
        prgubun: impl Into<String>,
        orggubun: impl Into<String>,
        frggubun: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1716Request {
            inblock: T1716InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                fromdt: fromdt.into(),
                todt: todt.into(),
                prapp: prapp.into(),
                prgubun: prgubun.into(),
                orggubun: orggubun.into(),
                frggubun: frggubun.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1716OutBlock` — one foreign/institution by-issue trend row (representative
/// subset): the date, the close + change + cumulative volume, and the per-exchange
/// individual/institution/foreign + program flows. Every numeric-bearing field via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1716OutBlock {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Close / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Prior-day change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Exchange-individual / 거래소_개인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_0008: String,
    /// Exchange-institution / 거래소_기관.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_0018: String,
    /// Exchange-foreign / 거래소_외국인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub krx_0009: String,
    /// Program / 프로그램.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pgmvol: String,
}

/// `t1716` response envelope — the repeated `t1716OutBlock` date-series rows
/// (tolerated single-or-array via [`ls_core::de_vec_or_single`]). All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1716Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1716OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1716OutBlock>,
}

// ---------------------------------------------------------------------------
// t1927 — 공매도일별추이 (short-selling daily trend). market_session domestic-stock
// read; path /stock/etc, group [주식] 기타. 4-field request — shcode, date (일자CTS),
// sdate/edate (시작/종료일자). All-String request — no numeric request slot.
// Response: a single `t1927OutBlock` cursor (date CTS) + a repeated `t1927OutBlock1`
// daily-series ARRAY (one row per date: price/volume + the short-sale flows)
// tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1927` — the short-selling daily-trend filters: the issue code,
/// a `date` CTS continuation token (`""` for the first page), and the `sdate`/`edate`
/// date range. All ordinary request Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T1927InBlock {
    /// Issue code / 종목코드.
    pub shcode: String,
    /// Date CTS / 일자 (`""` for the first page).
    pub date: String,
    /// Start date / 시작일자 (YYYYMMDD).
    pub sdate: String,
    /// End date / 종료일자 (YYYYMMDD).
    pub edate: String,
}

/// `t1927` request — serializes to `{"t1927InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1927Request {
    #[serde(rename = "t1927InBlock")]
    pub inblock: T1927InBlock,
}

impl T1927Request {
    /// Build a `t1927` short-selling daily-trend request from the caller filters.
    pub fn new(
        shcode: impl Into<String>,
        date: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        T1927Request {
            inblock: T1927InBlock {
                shcode: shcode.into(),
                date: date.into(),
                sdate: sdate.into(),
                edate: edate.into(),
            },
        }
    }
}

/// `t1927OutBlock` — the continuation cursor (date CTS). String via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1927OutBlock {
    /// Date CTS / 일자CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
}

/// `t1927OutBlock1` — one short-selling daily-trend row (representative subset): the
/// date, the close + cumulative volume + value, and the short-sale volume / value /
/// average price. Every numeric-bearing field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1927OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prior-day change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Short-sale volume / 공매도수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gm_vo: String,
    /// Short-sale value / 공매도대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gm_va: String,
}

/// `t1927` response envelope — the single cursor `t1927OutBlock` + the repeated
/// `t1927OutBlock1` daily-series rows (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1927Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1927OutBlock", default)]
    pub outblock: T1927OutBlock,
    #[serde(
        rename = "t1927OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1927OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1941 — 종목별대차거래일간추이 (per-issue stock-loan/대차 daily trend). market_session
// domestic-stock read; path /stock/etc, group [주식] 기타. 3-field request — shcode,
// sdate/edate (시작/종료일자). All-String request — no numeric request slot. Response:
// the out-rows are carried under a `response_body` wrapper as `t1941OutBlock1` — a
// repeated daily-series ARRAY (one row per date: close/volume + the loan
// execute/repay/balance flows) tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1941` — the per-issue stock-loan daily-trend filters: the issue
/// code and the `sdate`/`edate` date range. All ordinary request Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T1941InBlock {
    /// Issue code / 종목코드.
    pub shcode: String,
    /// Start date / 시작일자 (YYYYMMDD).
    pub sdate: String,
    /// End date / 종료일자 (YYYYMMDD).
    pub edate: String,
}

/// `t1941` request — serializes to `{"t1941InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1941Request {
    #[serde(rename = "t1941InBlock")]
    pub inblock: T1941InBlock,
}

impl T1941Request {
    /// Build a `t1941` per-issue stock-loan daily-trend request from the caller
    /// filters.
    pub fn new(
        shcode: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        T1941Request {
            inblock: T1941InBlock {
                shcode: shcode.into(),
                sdate: sdate.into(),
                edate: edate.into(),
            },
        }
    }
}

/// `t1941OutBlock1` — one per-issue stock-loan daily-trend row (representative
/// subset): the date, the close + volume, and the loan execute/repay/balance flows
/// + balance amount + loan delta. Every numeric-bearing field via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1941OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Close / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change sign / 대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Same-day execute / 당일체결.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upvolume: String,
    /// Same-day repay / 당일상환.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnvolume: String,
    /// Same-day balance / 당일잔고.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tovolume: String,
    /// Balance amount / 잔고금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tovalue: String,
}

/// `t1941` response envelope — the repeated `t1941OutBlock1` daily-series rows
/// (carried under the `response_body` wrapper; serde renames straight to the
/// `t1941OutBlock1` key), tolerated single-or-array via [`ls_core::de_vec_or_single`].
/// All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1941Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1941OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1941OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1631 — 프로그램매매종합조회 (program-trade综合). market_session domestic-stock
// program-trade read; path /stock/program, group [주식] 프로그램. 5-field request —
// gubun (구분), dgubun (일자구분), sdate/edate (시작/종료일자), exchgubun (거래소구분).
// All-String request — no numeric request slot, so no `string_as_number`. The
// response carries TWO single-object out-blocks: `t1631OutBlock` (the program-trade
// open-order remainders / order quantities) and `t1631OutBlock1` (the offer/bid
// volume + value totals). Both modeled as tolerant single-or-array Vecs via
// `ls_core::de_vec_or_single` (mirror t1950's main+array shape) to be robust to a
// future repeated shape.
// ---------------------------------------------------------------------------

/// Input block for `t1631` — the program-trade综合 filters. All ordinary request
/// Strings (no numeric request slot): `gubun` (구분), `dgubun` (일자구분), the
/// `sdate`/`edate` date range (YYYYMMDD), and `exchgubun` (거래소구분코드 — `"1"` =
/// KRX). See [`T1631Request::new`] for a sensible market-wide default.
#[derive(Serialize, Debug, Clone)]
pub struct T1631InBlock {
    /// Division / 구분.
    pub gubun: String,
    /// Date division / 일자구분.
    pub dgubun: String,
    /// Start date / 시작일자 (YYYYMMDD).
    pub sdate: String,
    /// End date / 종료일자 (YYYYMMDD).
    pub edate: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1631` request — serializes to `{"t1631InBlock":{...}}`. Not paginated
/// (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1631Request {
    #[serde(rename = "t1631InBlock")]
    pub inblock: T1631InBlock,
}

impl T1631Request {
    /// Build a `t1631` program-trade综合 request from the caller-supplied filters.
    pub fn new(
        gubun: impl Into<String>,
        dgubun: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1631Request {
            inblock: T1631InBlock {
                gubun: gubun.into(),
                dgubun: dgubun.into(),
                sdate: sdate.into(),
                edate: edate.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1631OutBlock` — the program-trade open-order remainders / order quantities
/// (a representative, spec-grounded subset). Every field a spec `Number`; via
/// [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse block
/// deserialize and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1631OutBlock {
    /// Sell-arbitrage open-order remainder / 매도차익미체결잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cdhrem: String,
    /// Sell-non-arbitrage open-order remainder / 매도비차익미체결잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bdhrem: String,
    /// Sell-arbitrage order quantity / 매도차익주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tcdrem: String,
    /// Sell-non-arbitrage order quantity / 매도비차익주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tbdrem: String,
    /// Buy-arbitrage open-order remainder / 매수차익미체결잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cshrem: String,
    /// Buy-non-arbitrage open-order remainder / 매수비차익미체결잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bshrem: String,
    /// Buy-arbitrage order quantity / 매수차익주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tcsrem: String,
    /// Buy-non-arbitrage order quantity / 매수비차익주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tbsrem: String,
}

/// `t1631OutBlock1` — the program-trade offer/bid volume + value totals (a
/// representative subset). Every field a spec `Number` via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1631OutBlock1 {
    /// Offer (sell) volume / 매도수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offervolume: String,
    /// Offer (sell) value / 매도금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offervalue: String,
    /// Bid (buy) volume / 매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidvolume: String,
    /// Bid (buy) value / 매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidvalue: String,
    /// Net-buy volume / 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Net-buy value / 순매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t1631` response envelope — the two program-trade out-blocks (`t1631OutBlock`
/// remainders/quantities + `t1631OutBlock1` volume/value totals), each tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]. All `#[serde(default)]` so a
/// terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1631Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1631OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1631OutBlock>,
    #[serde(
        rename = "t1631OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1631OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1632 — 프로그램매매추이(시간) (program-trade trend, intraday/time-series).
// market_session domestic-stock program-trade read; path /stock/program, group
// [주식] 프로그램. 7-field request — gubun, gubun1 (금액수량구분), gubun2 (직전대비
// 증감), gubun3 (전일구분), date (일자), time (시간), exchgubun. All-String request —
// no numeric request slot. Response: a single `t1632OutBlock` cursor (date/time/idx
// CTS) + a repeated `t1632OutBlock1` time-series ARRAY (one row per timestamp:
// KP200 index, change, the program-trade buy/sell/net totals) tolerated
// single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1632` — the program-trade intraday-trend filters. All ordinary
/// request Strings: `gubun` (구분), `gubun1` (금액수량구분), `gubun2` (직전대비증감),
/// `gubun3` (전일구분), `date` (일자 YYYYMMDD), `time` (시간 HHMMSS — `""` for the
/// latest), `exchgubun` (`"1"` = KRX). See [`T1632Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1632InBlock {
    /// Division / 구분.
    pub gubun: String,
    /// Amount/quantity division / 금액수량구분.
    pub gubun1: String,
    /// Vs-prior change division / 직전대비증감.
    pub gubun2: String,
    /// Prior-day division / 전일구분.
    pub gubun3: String,
    /// Date / 일자 (YYYYMMDD).
    pub date: String,
    /// Time / 시간 (HHMMSS — `""` for the latest).
    pub time: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1632` request — serializes to `{"t1632InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1632Request {
    #[serde(rename = "t1632InBlock")]
    pub inblock: T1632InBlock,
}

impl T1632Request {
    /// Build a `t1632` program-trade intraday-trend request from the caller filters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gubun: impl Into<String>,
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        gubun3: impl Into<String>,
        date: impl Into<String>,
        time: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1632Request {
            inblock: T1632InBlock {
                gubun: gubun.into(),
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                gubun3: gubun3.into(),
                date: date.into(),
                time: time.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1632OutBlock` — the continuation cursor (date/time CTS + idx). `ex_gubun` is a
/// String; `idx` a spec `Number`. `#[serde(default)]` lets a terse block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1632OutBlock {
    /// Date CTS / 날짜CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time CTS / 시간CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// IDX / IDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
    /// Per-exchange division / 거래소별구분코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_gubun: String,
}

/// `t1632OutBlock1` — one program-trade intraday-trend row (representative subset):
/// the timestamp, the KP200 index + change, and the all/arbitrage/non-arbitrage
/// buy/sell/net totals. Every numeric-bearing field via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1632OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// KP200 index / KP200.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k200jisu: String,
    /// Change sign / 대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Whole-market net buy / 전체순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tot3: String,
    /// Whole-market buy / 전체매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tot1: String,
    /// Whole-market sell / 전체매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tot2: String,
    /// Arbitrage net buy / 차익순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cha3: String,
}

/// `t1632` response envelope — the single cursor `t1632OutBlock` + the repeated
/// `t1632OutBlock1` time-series rows (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1632Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1632OutBlock", default)]
    pub outblock: T1632OutBlock,
    #[serde(
        rename = "t1632OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1632OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1633 — 프로그램매매추이(일별) (program-trade trend, daily series). market_session
// domestic-stock program-trade read; path /stock/program, group [주식] 프로그램.
// 9-field request — gubun (시장구분), gubun1 (금액수량구분), gubun2 (수치누적구분),
// gubun3 (일주월구분), fdate/tdate (from/to일자), gubun4 (직전대비증감구분), date
// (날짜), exchgubun. All-String request — no numeric request slot. Response: a single
// `t1633OutBlock` cursor (date/idx) + a repeated `t1633OutBlock1` daily-series ARRAY
// (one row per date: KP200 index, change, the program-trade buy/sell/net totals +
// volume) tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1633` — the program-trade daily-trend filters. All ordinary
/// request Strings: `gubun` (시장구분), `gubun1` (금액수량구분), `gubun2`
/// (수치누적구분), `gubun3` (일주월구분), `fdate`/`tdate` (from/to일자 YYYYMMDD),
/// `gubun4` (직전대비증감구분), `date` (날짜), `exchgubun` (`"1"` = KRX). See
/// [`T1633Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1633InBlock {
    /// Market division / 시장구분.
    pub gubun: String,
    /// Amount/quantity division / 금액수량구분.
    pub gubun1: String,
    /// Value/accumulation division / 수치누적구분.
    pub gubun2: String,
    /// Day/week/month division / 일주월구분.
    pub gubun3: String,
    /// From date / from일자 (YYYYMMDD).
    pub fdate: String,
    /// To date / to일자 (YYYYMMDD).
    pub tdate: String,
    /// Vs-prior change division / 직전대비증감구분.
    pub gubun4: String,
    /// Date / 날짜 (YYYYMMDD).
    pub date: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1633` request — serializes to `{"t1633InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1633Request {
    #[serde(rename = "t1633InBlock")]
    pub inblock: T1633InBlock,
}

impl T1633Request {
    /// Build a `t1633` program-trade daily-trend request from the caller filters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gubun: impl Into<String>,
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        gubun3: impl Into<String>,
        fdate: impl Into<String>,
        tdate: impl Into<String>,
        gubun4: impl Into<String>,
        date: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1633Request {
            inblock: T1633InBlock {
                gubun: gubun.into(),
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                gubun3: gubun3.into(),
                fdate: fdate.into(),
                tdate: tdate.into(),
                gubun4: gubun4.into(),
                date: date.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1633OutBlock` — the continuation cursor (date/idx). `idx` a spec `Number`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1633OutBlock {
    /// Date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// IDX / IDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1633OutBlock1` — one program-trade daily-trend row (representative subset):
/// the date, the KP200 index + change, the all/arbitrage/non-arbitrage net totals,
/// and the trade volume. Every numeric-bearing field via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1633OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// KP200 index / KP200.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// Change sign / 대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Whole-market net buy / 전체순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tot3: String,
    /// Arbitrage net buy / 차익순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cha3: String,
    /// Non-arbitrage net buy / 비차익순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bcha3: String,
    /// Trade volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1633` response envelope — the single cursor `t1633OutBlock` + the repeated
/// `t1633OutBlock1` daily-series rows (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1633Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1633OutBlock", default)]
    pub outblock: T1633OutBlock,
    #[serde(
        rename = "t1633OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1633OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1702 — 외국인/기관별 일별/누적 매매추이 (foreign/institution by-issue trend).
// market_session domestic-stock 외인/기관 read; path /stock/frgr-itt, group [주식]
// 외인/기관. 7-field all-String request — shcode, fromdt/todt (date range), volvalgb
// (금액/수량/단가), msmdgb (순매수/매수/매도), gubun (일간/누적), exchgubun. Response is
// a single repeated `t1702OutBlock1` date ARRAY (one row per day: close/volume + the
// per-investor net columns) tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1702` — the foreign/institution by-issue trend filters. All
/// ordinary request Strings: `shcode` (종목코드), the `fromdt`/`todt` date range
/// (YYYYMMDD), `volvalgb` (금액수량구분 — `0`금액/`1`수량/`2`단가), `msmdgb` (매수매도구분
/// — `0`순매수/`1`매수/`2`매도), `gubun` (누적구분 — `0`일간/`1`누적), `exchgubun`
/// (`"1"` = KRX). See [`T1702Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1702InBlock {
    /// Issue code / 종목코드.
    pub shcode: String,
    /// Start date / 시작일자 (YYYYMMDD).
    pub fromdt: String,
    /// End date / 종료일자 (YYYYMMDD).
    pub todt: String,
    /// Amount/quantity/unit-price division / 금액수량구분 (`0`금액/`1`수량/`2`단가).
    pub volvalgb: String,
    /// Buy/sell division / 매수매도구분 (`0`순매수/`1`매수/`2`매도).
    pub msmdgb: String,
    /// Cumulative division / 누적구분 (`0`일간/`1`누적).
    pub gubun: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1702` request — serializes to `{"t1702InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1702Request {
    #[serde(rename = "t1702InBlock")]
    pub inblock: T1702InBlock,
}

impl T1702Request {
    /// Build a `t1702` foreign/institution by-issue trend request from the filters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shcode: impl Into<String>,
        fromdt: impl Into<String>,
        todt: impl Into<String>,
        volvalgb: impl Into<String>,
        msmdgb: impl Into<String>,
        gubun: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1702Request {
            inblock: T1702InBlock {
                shcode: shcode.into(),
                fromdt: fromdt.into(),
                todt: todt.into(),
                volvalgb: volvalgb.into(),
                msmdgb: msmdgb.into(),
                gubun: gubun.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1702OutBlock1` — one foreign/institution by-issue trend row (a representative,
/// spec-grounded subset): the trading `date`, the `close`/`change`/`volume`, and a
/// few per-investor net columns. Every numeric field via [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a sparse row deserialize and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1702OutBlock1 {
    /// Trading date / 일자 (YYYYMMDD).
    pub date: String,
    /// Close price / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Vs-prior-day sign / 전일대비구분.
    pub sign: String,
    /// Vs-prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Securities-firm net / 증권.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0001: String,
    /// Insurance net / 보험.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0002: String,
    /// Individual net / 개인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0008: String,
    /// Institution total net / 기관.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0018: String,
    /// Foreign total net (registered + unregistered) / 외인계.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0016: String,
}

/// `t1702` response envelope — the repeated `t1702OutBlock1` date ARRAY tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1702Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1702OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1702OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1717 — 외국인/기관 순매수추이 (foreign/institution net-buy trend by issue).
// market_session domestic-stock 외인/기관 read; path /stock/frgr-itt, group [주식]
// 외인/기관. 5-field all-String request — shcode, gubun (일간/기간누적), fromdt/todt
// (date range), exchgubun. Response is a single repeated `t1717OutBlock` date ARRAY
// (one row per day: close/volume + the per-investor net-buy-quantity columns)
// tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1717` — the foreign/institution net-buy trend filters. All
/// ordinary request Strings: `shcode` (종목코드), `gubun` (구분 — `0`일간순매수/`1`기간
/// 누적순매수), the `fromdt`/`todt` date range (YYYYMMDD; `fromdt` may be space for a
/// single-day query), `exchgubun` (`"1"` = KRX). See [`T1717Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1717InBlock {
    /// Issue code / 종목코드.
    pub shcode: String,
    /// Division / 구분 (`0`일간순매수/`1`기간누적순매수).
    pub gubun: String,
    /// Start date / 시작일자 (YYYYMMDD).
    pub fromdt: String,
    /// End date / 종료일자 (YYYYMMDD).
    pub todt: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1717` request — serializes to `{"t1717InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1717Request {
    #[serde(rename = "t1717InBlock")]
    pub inblock: T1717InBlock,
}

impl T1717Request {
    /// Build a `t1717` foreign/institution net-buy trend request from the filters.
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        fromdt: impl Into<String>,
        todt: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1717Request {
            inblock: T1717InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                fromdt: fromdt.into(),
                todt: todt.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1717OutBlock` — one foreign/institution net-buy trend row (a representative,
/// spec-grounded subset): the trading `date`, the `close`/`change`/`volume`, and a
/// few per-investor net-buy-quantity columns. Every numeric field via
/// [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse row deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1717OutBlock {
    /// Trading date / 일자 (YYYYMMDD).
    pub date: String,
    /// Close price / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Vs-prior-day sign / 전일대비구분.
    pub sign: String,
    /// Vs-prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Securities-firm net-buy quantity / 증권(순매수량).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0001_vol: String,
    /// Individual net-buy quantity / 개인(순매수량).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0008_vol: String,
    /// Institution total net-buy quantity / 기관(순매수량).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0018_vol: String,
    /// Foreign total net-buy quantity / 외인계(순매수량).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj0016_vol: String,
}

/// `t1717` response envelope — the repeated `t1717OutBlock` date ARRAY tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1717Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1717OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1717OutBlock>,
}

// ---------------------------------------------------------------------------
// t1665 — 투자자별 매매추이(업종) (investor-by-sector trend chart). market_session
// domestic-stock 차트 read; path /stock/chart, group [주식] 차트. 7-field all-String
// request — market, upcode (업종코드), gubun2 (수치/누적), gubun3 (일/주/월), from_date/
// to_date (date range), exchgubun. Response: a single `t1665OutBlock` header
// (mcode/mname/ex_upcode) + a repeated `t1665OutBlock1` date ARRAY (one row per
// date: the per-investor sv_*/sa_* quantity/value columns + the market `jisu`)
// tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1665` — the investor-by-sector trend filters. All ordinary
/// request Strings: `market` (시장구분), `upcode` (업종코드 — e.g. `"001"` KOSPI),
/// `gubun2` (수치구분 — `1`수치/`2`누적), `gubun3` (단위구분 — `1`일/`2`주/`3`월), the
/// `from_date`/`to_date` range (YYYYMMDD), `exchgubun` (`"1"` = KRX). See
/// [`T1665Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1665InBlock {
    /// Market division / 시장구분.
    pub market: String,
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Value division / 수치구분 (`1`수치/`2`누적).
    pub gubun2: String,
    /// Unit division / 단위구분 (`1`일/`2`주/`3`월).
    pub gubun3: String,
    /// Start date / 시작날짜 (YYYYMMDD).
    pub from_date: String,
    /// End date / 종료날짜 (YYYYMMDD).
    pub to_date: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1665` request — serializes to `{"t1665InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1665Request {
    #[serde(rename = "t1665InBlock")]
    pub inblock: T1665InBlock,
}

impl T1665Request {
    /// Build a `t1665` investor-by-sector trend request from the caller filters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        market: impl Into<String>,
        upcode: impl Into<String>,
        gubun2: impl Into<String>,
        gubun3: impl Into<String>,
        from_date: impl Into<String>,
        to_date: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1665Request {
            inblock: T1665InBlock {
                market: market.into(),
                upcode: upcode.into(),
                gubun2: gubun2.into(),
                gubun3: gubun3.into(),
                from_date: from_date.into(),
                to_date: to_date.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1665OutBlock` — the sector header (market code / name / exchange sector code).
/// All Strings; `#[serde(default)]` lets a terse header deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1665OutBlock {
    /// Market code / 시장코드.
    pub mcode: String,
    /// Market name / 시장명.
    pub mname: String,
    /// Exchange-specific sector code / 거래소별업종코드.
    pub ex_upcode: String,
}

/// `t1665OutBlock1` — one investor-by-sector trend row (a representative, spec-grounded
/// subset): the `date`, a few per-investor quantity columns (`sv_*`), and the market
/// index `jisu`. Every numeric field via [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a sparse row deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1665OutBlock1 {
    /// Date / 일자 (YYYYMMDD).
    pub date: String,
    /// Individual quantity / 개인수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Foreign total quantity (registered + unregistered) / 외인계수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Institution total quantity / 기관계수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
    /// Market index / 시장지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
}

/// `t1665` response envelope — the single `t1665OutBlock` header + the repeated
/// `t1665OutBlock1` date ARRAY tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1665Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1665OutBlock", default)]
    pub outblock: T1665OutBlock,
    #[serde(
        rename = "t1665OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1665OutBlock1>,
}

// ---------------------------------------------------------------------------
// Wave 2 — market-flow analytics surface. Investor-flow (t1601/t1615/t1664) and
// program-trading (t1640/t1662) aggregates; gubun-filter screens with documented
// default inputs. All non-paginated market_session reads.
// ---------------------------------------------------------------------------

/// Input block for `t1601` — 투자자별종합 (investor-by-type aggregate). All-gubun
/// filter screen; `::new()` bakes documented defaults (amount basis, KRX).
#[derive(Serialize, Debug, Clone)]
pub struct T1601InBlock {
    /// Stock amount/quantity / 주식금액수량구분1 (`"1"` qty / `"2"` amount).
    pub gubun1: String,
    /// Option amount/quantity / 옵션금액수량구분2.
    pub gubun2: String,
    /// Amount unit / 금액단위 (unused; `"0"`).
    pub gubun3: String,
    /// Futures amount/quantity / 선물금액수량구분4.
    pub gubun4: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1601` request — wraps the in-block under `t1601InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1601Request {
    #[serde(rename = "t1601InBlock")]
    pub inblock: T1601InBlock,
}
impl T1601Request {
    /// Build a `t1601` request with documented broad defaults (amount basis, KRX).
    pub fn new() -> Self {
        T1601Request {
            inblock: T1601InBlock {
                gubun1: "2".into(),
                gubun2: "2".into(),
                gubun3: "0".into(),
                gubun4: "2".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1601Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1601OutBlock1` — the investor-by-type aggregate (single summary object; a
/// representative subset of net-buy columns). All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1601OutBlock1 {
    /// Personal net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_08: String,
    /// Foreign net-buy / 외국인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_17: String,
    /// Institutional net-buy / 기관계순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_18: String,
}

/// `t1601` response — the investor aggregate under `t1601OutBlock1` (single object).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1601Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1601OutBlock1", default)]
    pub outblock1: T1601OutBlock1,
}

/// Input block for `t1615` — 투자자매매종합1 (investor trading aggregate).
#[derive(Serialize, Debug, Clone)]
pub struct T1615InBlock {
    /// Stock division / 주식구분 (`"1"` qty / `"2"` amount).
    pub gubun1: String,
    /// Option division / 옵션구분.
    pub gubun2: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1615` request — wraps the in-block under `t1615InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1615Request {
    #[serde(rename = "t1615InBlock")]
    pub inblock: T1615InBlock,
}
impl T1615Request {
    /// Build a `t1615` request with documented broad defaults (amount basis, KRX).
    pub fn new() -> Self {
        T1615Request {
            inblock: T1615InBlock {
                gubun1: "2".into(),
                gubun2: "2".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1615Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1615OutBlock` — the trading summary (single object). Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1615OutBlock {
    /// Total quantity / 합계수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sum_volume: String,
    /// Total amount / 합계금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sum_value: String,
}

/// `t1615OutBlock1` — one per-market investor row. Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1615OutBlock1 {
    /// Market name / 시장명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Personal / 개인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Foreign / 외국인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Institutional / 기관계.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
}

/// `t1615` response — summary + per-market array (single-or-array tolerated).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1615Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1615OutBlock", default)]
    pub outblock: T1615OutBlock,
    #[serde(
        rename = "t1615OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1615OutBlock1>,
}

/// Input block for `t1640` — 프로그램매매종합조회(미니) (program-trading aggregate).
#[derive(Serialize, Debug, Clone)]
pub struct T1640InBlock {
    /// Division / 구분 (`"11"` exchange-all).
    pub gubun: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1640` request — wraps the in-block under `t1640InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1640Request {
    #[serde(rename = "t1640InBlock")]
    pub inblock: T1640InBlock,
}
impl T1640Request {
    /// Build a `t1640` request with documented broad defaults (exchange-all, KRX).
    pub fn new() -> Self {
        T1640Request {
            inblock: T1640InBlock {
                gubun: "11".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1640Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1640OutBlock` — the program-trading summary (single object). Subset via
/// [`ls_core::string_or_number`]; `value` is the modeled non-key signal.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1640OutBlock {
    /// Net-buy quantity / 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Net-buy amount / 순매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Basis / 베이시스.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub basis: String,
}

/// `t1640` response — the program summary (single object).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1640Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1640OutBlock", default)]
    pub outblock: T1640OutBlock,
}

/// Input block for `t1662` — 시간대별프로그램매매추이(차트) (by-time program-trading
/// chart).
#[derive(Serialize, Debug, Clone)]
pub struct T1662InBlock {
    /// Market / 구분 (`"0"` KOSPI / `"1"` KOSDAQ).
    pub gubun: String,
    /// Amount/quantity / 금액수량구분 (`"0"` amount / `"1"` qty).
    pub gubun1: String,
    /// Day / 전일구분 (`"0"` today / `"1"` prior).
    pub gubun3: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1662` request — wraps the in-block under `t1662InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1662Request {
    #[serde(rename = "t1662InBlock")]
    pub inblock: T1662InBlock,
}
impl T1662Request {
    /// Build a `t1662` request with documented broad defaults (KOSPI, amount,
    /// today, KRX).
    pub fn new() -> Self {
        T1662Request {
            inblock: T1662InBlock {
                gubun: "0".into(),
                gubun1: "0".into(),
                gubun3: "0".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1662Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1662OutBlock` — one by-time program-trading row. Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1662OutBlock {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// KOSPI200 index / KP200.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k200jisu: String,
    /// Total net-buy / 전체순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tot3: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1662` response — the by-time array under `t1662OutBlock` (single-or-array).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1662Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1662OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1662OutBlock>,
}

/// Input block for `t1664` — 투자자매매종합(챠트) (investor trading chart). `cnt`
/// is a numeric count serialized as a JSON number.
#[derive(Serialize, Debug, Clone)]
pub struct T1664InBlock {
    /// Market / 시장구분 (`"1"` KOSPI).
    pub mgubun: String,
    /// Amount/quantity / 금액수량구분 (`"2"` amount).
    pub vagubun: String,
    /// Time/day / 시간일별구분 (`"1"` by-time).
    pub bdgubun: String,
    /// Row count / 조회건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1664` request — wraps the in-block under `t1664InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1664Request {
    #[serde(rename = "t1664InBlock")]
    pub inblock: T1664InBlock,
}
impl T1664Request {
    /// Build a `t1664` request with documented broad defaults (KOSPI, amount,
    /// by-time, 20 rows, KRX).
    pub fn new() -> Self {
        T1664Request {
            inblock: T1664InBlock {
                mgubun: "1".into(),
                vagubun: "2".into(),
                bdgubun: "1".into(),
                cnt: "20".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1664Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1664OutBlock1` — one investor-chart row. Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1664OutBlock1 {
    /// Date/time / 일자시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dt: String,
    /// Personal net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj08: String,
    /// Foreign net-buy / 외국인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj17: String,
    /// Institutional net-buy / 기관순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj18: String,
}

/// `t1664` response — the investor-chart array under `t1664OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1664Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1664OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1664OutBlock1>,
}

/// Input block for `t8463` — KRX야간파생 투자자시간대별(API용) (night-derivatives
/// investor-by-timeslot).
///
/// `tm_rng` is the timeslot (시간대: `D`/`N`/`U`); `fot_clsf_cd` is the F/O
/// distinction (선물옵션구분); `bsc_asts_id` is the underlying-asset code
/// (기초자산코드); `cnt` is the requested COUNT (조회건수), a numeric REQUEST field
/// serialized as a JSON number via [`ls_core::string_as_number`] (KTD4 — the
/// string form risks `IGW40011`); `bgubun` is the previous-day flag (전일분).
#[derive(Serialize, Debug, Clone)]
pub struct T8463InBlock {
    /// Timeslot / 시간대 (`"D"`/`"N"`/`"U"`).
    pub tm_rng: String,
    /// F/O distinction / 선물옵션구분.
    pub fot_clsf_cd: String,
    /// Underlying-asset code / 기초자산코드.
    pub bsc_asts_id: String,
    /// Requested count / 조회건수 (serialized as a JSON number, KTD4).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Previous-day flag / 전일분.
    pub bgubun: String,
}

/// `t8463` request — serializes to `{"t8463InBlock":{...,"cnt":<number>,...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8463Request {
    #[serde(rename = "t8463InBlock")]
    pub inblock: T8463InBlock,
}
impl T8463Request {
    /// Build a `t8463` investor-by-timeslot request. `cnt` defaults to `"20"`
    /// (rows requested); `bgubun` to `"0"` (current day).
    pub fn new(
        tm_rng: impl Into<String>,
        fot_clsf_cd: impl Into<String>,
        bsc_asts_id: impl Into<String>,
    ) -> Self {
        T8463Request {
            inblock: T8463InBlock {
                tm_rng: tm_rng.into(),
                fot_clsf_cd: fot_clsf_cd.into(),
                bsc_asts_id: bsc_asts_id.into(),
                cnt: "20".into(),
                bgubun: "0".into(),
            },
        }
    }
}

/// `t8463OutBlock` — the investor-code header (single Object, A0003). A
/// representative subset; the per-investor-type codes. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8463OutBlock {
    /// Timeslot / 시간대 (`"D"`/`"N"`/`"U"`).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tm_rng: String,
    /// Individual-investor code / 개인투자자코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indcode: String,
    /// Foreign-investor code / 외국인투자자코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub forcode: String,
}

/// `t8463OutBlock1` — one investor-by-timeslot row (`t8463OutBlock1[]`, an ARRAY
/// block, A0005). A representative subset; numeric net-buy volumes via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8463OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Individual net-buy volume / 개인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indmsvol: String,
    /// Foreign net-buy volume / 외국인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub formsvol: String,
}

/// `t8463` response — the investor-code header `t8463OutBlock` + the
/// time-series row array `t8463OutBlock1` (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8463Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8463OutBlock", default)]
    pub outblock: T8463OutBlock,
    #[serde(
        rename = "t8463OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8463OutBlock1>,
}

// ---------------------------------------------------------------------------
// t8462 — KRX야간파생 투자자기간별 (KRX night-derivatives investor period table).
// Non-paginated market-data read keyed by basis-asset + a from/to date range; a
// `t8462OutBlock1[]` row array (de_vec_or_single). Plan -003; a recent date range
// returns populated rows under closure.
// ---------------------------------------------------------------------------

/// Input block for `t8462` — KRX night-derivatives investor-period table.
/// `tm_rng` (time-range, e.g. "N" night), `fot_clsf_cd` (F/O class), `bsc_asts_id`
/// (basis asset, e.g. "K2I"), `gubun2`/`gubun3` mode flags, `from_date`/`to_date`
/// bound the range. No numeric request fields.
#[derive(Serialize, Debug, Clone)]
pub struct T8462InBlock {
    /// Time range / 시간범위 (e.g. "N").
    pub tm_rng: String,
    /// Future/option class / 선물옵션구분.
    pub fot_clsf_cd: String,
    /// Basis-asset id / 기초자산.
    pub bsc_asts_id: String,
    /// Division 2 / 구분2.
    pub gubun2: String,
    /// Division 3 / 구분3.
    pub gubun3: String,
    /// Start date / 시작일자.
    pub from_date: String,
    /// End date / 종료일자.
    pub to_date: String,
}

/// `t8462` request — serializes to `{"t8462InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T8462Request {
    #[serde(rename = "t8462InBlock")]
    pub inblock: T8462InBlock,
}
impl T8462Request {
    /// Build a `t8462` investor-period request for one basis asset over a
    /// `from_date`..`to_date` range; spec defaults (`tm_rng="N"`, `fot_clsf_cd="F"`,
    /// `gubun2="1"`, `gubun3="1"`).
    pub fn new(
        bsc_asts_id: impl Into<String>,
        from_date: impl Into<String>,
        to_date: impl Into<String>,
    ) -> Self {
        T8462Request {
            inblock: T8462InBlock {
                tm_rng: "N".to_string(),
                fot_clsf_cd: "F".to_string(),
                bsc_asts_id: bsc_asts_id.into(),
                gubun2: "1".to_string(),
                gubun3: "1".to_string(),
                from_date: from_date.into(),
                to_date: to_date.into(),
            },
        }
    }
}

/// `t8462OutBlock` — query echo header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8462OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tm_rng: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fot_clsf_cd: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsc_asts_id: String,
}

/// `t8462OutBlock1` — one investor-period row (date + per-investor net volume/amount
/// columns `sv_xx`/`sa_xx`). `sv_01` (개인) is the substantive witness.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8462OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Net volume — foreigner / 외국인 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Net volume — institution total / 기관계 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Net volume — total / 전체 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
    /// Net volume — individual / 개인 순매수수량 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_01: String,
    /// Net amount — foreigner / 외국인 순매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sa_08: String,
    /// Net amount — institution total / 기관계 순매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sa_17: String,
    /// Net amount — total / 전체 순매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sa_18: String,
    /// Net amount — individual / 개인 순매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sa_01: String,
}

/// `t8462` response — echo header + investor rows under `t8462OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8462Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8462OutBlock", default)]
    pub outblock: T8462OutBlock,
    #[serde(
        rename = "t8462OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8462OutBlock1>,
}

/// Input block for `t1926`.
#[derive(Serialize, Debug, Clone)]
pub struct T1926InBlock {
    pub shcode: String,
}

/// `t1926` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1926Request {
    #[serde(rename = "t1926InBlock")]
    pub inblock: T1926InBlock,
}
impl T1926Request {
    /// Build a `t1926` request.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1926Request {
            inblock: T1926InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1926OutBlock` — single summary object.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1926OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mmdate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dsprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dsvolume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dgrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub djprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub djvolume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub djrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ysprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ysvolume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ygrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yjprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yjvolume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yjrate: String,
}

/// `t1926` response (single object out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1926Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1926OutBlock", default)]
    pub outblock: T1926OutBlock,
}
