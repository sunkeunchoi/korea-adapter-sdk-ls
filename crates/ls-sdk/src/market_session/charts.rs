//! Time-bucket / price-band / minute-bar / conclusion chart reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use charts::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// t1308 — 주식시간대별체결조회챠트 (time-bucketed trade chart). market_session
// read; a single `t1308OutBlock` (the exchange short code) plus a repeated
// `t1308OutBlock1` time-bucket array (tolerated single-or-array via
// `ls_core::de_vec_or_single`); path /stock/market-data. 5-field request
// (shcode/starttime/endtime/bun_term/exchgubun) — all serialize as STRINGS.
// ---------------------------------------------------------------------------

/// Input block for `t1308` — short code (`shcode`), start/end time
/// (`starttime`/`endtime`, may be empty for the full session), bucket interval
/// (`bun_term`, e.g. `"1"` minute), and exchange distinction (`exchgubun`, may
/// be empty for integrated). All fields are caller-supplied Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T1308InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Start time / 시작시간 (HHMM; empty for the full session).
    pub starttime: String,
    /// End time / 종료시간 (HHMM; empty for the full session).
    pub endtime: String,
    /// Bucket interval (minutes) / 분간격 (e.g. `"1"`).
    pub bun_term: String,
    /// Exchange distinction / 거래소구분코드 (empty for integrated).
    pub exchgubun: String,
}

/// `t1308` request — serializes to
/// `{"t1308InBlock":{"shcode":...,"starttime":...,"endtime":...,"bun_term":...,"exchgubun":...}}`.
/// Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1308Request {
    #[serde(rename = "t1308InBlock")]
    pub inblock: T1308InBlock,
}

impl T1308Request {
    /// Build a `t1308` time-bucketed trade-chart request. `starttime`/`endtime`
    /// may be empty (`""`) for the full session; `exchgubun` may be empty for the
    /// integrated venue.
    pub fn new(
        shcode: impl Into<String>,
        starttime: impl Into<String>,
        endtime: impl Into<String>,
        bun_term: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1308Request {
            inblock: T1308InBlock {
                shcode: shcode.into(),
                starttime: starttime.into(),
                endtime: endtime.into(),
                bun_term: bun_term.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1308OutBlock` — the summary block (the exchange-distinguished short code).
/// `#[serde(default)]` so a sparse/absent block deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1308OutBlock {
    /// Exchange-distinguished short code / 거래소별단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// `t1308OutBlock1` — one time-bucket trade row (a representative, spec-grounded
/// subset): bucket time, current price, sign/change/rate, the bucket trade volume,
/// the bid/ask trade volumes, and the bucket OHLC. Every numeric-bearing field
/// uses [`ls_core::string_or_number`] (the gateway sends numbers or strings);
/// `#[serde(default)]` lets a sparse row deserialize, and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1308OutBlock1 {
    /// Bucket time / 시간 (HHMMSS).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Bucket trade volume / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Cumulative trade volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Sell trade volume / 매도체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// Buy trade volume / 매수체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
}

/// `t1308` response envelope — the summary block under the `t1308OutBlock` key,
/// plus the time-bucket rows under the `t1308OutBlock1` key (tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]). All `#[serde(default)]`
/// so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1308Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1308OutBlock", default)]
    pub outblock: T1308OutBlock,
    #[serde(
        rename = "t1308OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1308OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1449 — 가격대별매매비중조회 (price-band trade-weight). market_session read;
// a single `t1449OutBlock` summary (current price + cumulative volume split) plus
// a repeated `t1449OutBlock1` price-band array (tolerated single-or-array via
// `ls_core::de_vec_or_single`); path /stock/market-data. 2-field request
// (shcode/dategb) — both serialize as STRINGS. `dategb` MUST be non-empty
// (e.g. "1") or the board comes back empty.
// ---------------------------------------------------------------------------

/// Input block for `t1449` — short code (`shcode`) and day-distinction
/// (`dategb`, e.g. `"1"`). Both caller-supplied Strings; `dategb` must be
/// non-empty (an empty `dategb` returns an empty board).
#[derive(Serialize, Debug, Clone)]
pub struct T1449InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Day distinction / 일자구분 (e.g. `"1"`; must be non-empty).
    pub dategb: String,
}

/// `t1449` request — serializes to
/// `{"t1449InBlock":{"shcode":...,"dategb":...}}`. Not paginated
/// (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1449Request {
    #[serde(rename = "t1449InBlock")]
    pub inblock: T1449InBlock,
}

impl T1449Request {
    /// Build a `t1449` price-band trade-weight request. `dategb` should be a
    /// non-empty day-distinction (e.g. `"1"`).
    pub fn new(shcode: impl Into<String>, dategb: impl Into<String>) -> Self {
        T1449Request {
            inblock: T1449InBlock {
                shcode: shcode.into(),
                dategb: dategb.into(),
            },
        }
    }
}

/// `t1449OutBlock` — the summary block: current price, sign/change/rate, the
/// cumulative volume and its buy/sell split. Every numeric-bearing field uses
/// [`ls_core::string_or_number`]; `#[serde(default)]` so a sparse/absent block
/// deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1449OutBlock {
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Cumulative volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Buy trade volume / 매수체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// Sell trade volume / 매도체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
}

/// `t1449OutBlock1` — one price-band trade-weight row (a representative,
/// spec-grounded subset): the band price, sign/change/tick-rate, the band trade
/// quantity, the buy/sell trade volumes, the band weight, and the buy ratio.
/// Every numeric-bearing field uses [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a sparse row deserialize, and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1449OutBlock1 {
    /// Band trade price / 체결가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Tick rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tickdiff: String,
    /// Band trade quantity / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Band weight (%) / 비중.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Sell trade volume / 매도체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// Buy trade volume / 매수체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// Buy ratio (%) / 매수비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msdiff: String,
}

/// `t1449` response envelope — the summary block under the `t1449OutBlock` key,
/// plus the price-band rows under the `t1449OutBlock1` key (tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]). All `#[serde(default)]`
/// so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1449Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1449OutBlock", default)]
    pub outblock: T1449OutBlock,
    #[serde(
        rename = "t1449OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1449OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1621 — 업종별분별투자자매매동향(챠트용) (by-time, by-sector investor trading).
// market_session read; a single `t1621OutBlock` header (investor-class codes +
// the base-index name/code) plus a repeated `t1621OutBlock1` by-time array
// (tolerated single-or-array via `ls_core::de_vec_or_single`); path
// /stock/investor. 5-field request — `upcode`/`bgubun`/`exchgubun` are plain
// Strings; `nmin` and `cnt` are numeric REQUEST slots that MUST serialize as JSON
// NUMBERS via `ls_core::string_as_number` (KTD3 — the string form returns
// IGW40011).
// ---------------------------------------------------------------------------

/// Input block for `t1621` — sector code (`upcode`), the N-minute bucket
/// (`nmin`), the requested row count (`cnt`), the previous-day flag (`bgubun`)
/// and the exchange code (`exchgubun`).
///
/// `nmin` and `cnt` are held as `String` but serialize as JSON NUMBERS via
/// [`ls_core::string_as_number`] (KTD3 — the string form returns `IGW40011`).
/// `upcode`/`bgubun`/`exchgubun` serialize as ordinary Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T1621InBlock {
    /// Sector code / 업종코드 (e.g. `"001"` KOSPI).
    pub upcode: String,
    /// N-minute bucket / N분 (numeric request slot, KTD3).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub nmin: String,
    /// Requested row count / 조회건수 (numeric request slot, KTD3).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Previous-day flag / 전일분 (e.g. `"0"`).
    pub bgubun: String,
    /// Exchange code / 거래소구분코드 (e.g. `""`).
    pub exchgubun: String,
}

/// `t1621` request — serializes to
/// `{"t1621InBlock":{"upcode":...,"nmin":0,"cnt":20,"bgubun":...,"exchgubun":...}}`
/// (`nmin`/`cnt` as JSON numbers). Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1621Request {
    #[serde(rename = "t1621InBlock")]
    pub inblock: T1621InBlock,
}

impl T1621Request {
    /// Build a `t1621` by-time investor-trading request. `nmin`/`cnt` are passed
    /// as Strings but wire-serialize as JSON numbers (KTD3).
    pub fn new(
        upcode: impl Into<String>,
        nmin: impl Into<String>,
        cnt: impl Into<String>,
        bgubun: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1621Request {
            inblock: T1621InBlock {
                upcode: upcode.into(),
                nmin: nmin.into(),
                cnt: cnt.into(),
                bgubun: bgubun.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1621OutBlock` — the header block: the base-index code/name and the
/// per-exchange sector code (a representative, spec-grounded subset of the
/// investor-class header). Every modeled field via [`ls_core::string_or_number`];
/// `#[serde(default)]` so a sparse/absent header deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1621OutBlock {
    /// Base-index code / 기준지수코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisucd: String,
    /// Base-index name / 기준지수명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisunm: String,
    /// Per-exchange sector code / 거래소별업종코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_upcode: String,
}

/// `t1621OutBlock1` — one by-time investor-trading row (a representative,
/// spec-grounded subset): the date/time keys, the individual / foreign /
/// institution net-buy volume and amount, and the base-index value. Every
/// numeric-bearing field via [`ls_core::string_or_number`]; `#[serde(default)]`
/// lets a sparse row deserialize and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1621OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Individual net-buy volume / 개인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indmsvol: String,
    /// Individual net-buy amount / 개인순매수거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indmsamt: String,
    /// Foreign net-buy volume / 외국인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub formsvol: String,
    /// Foreign net-buy amount / 외국인순매수거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub formsamt: String,
    /// Institution net-buy volume / 기관계순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sysmsvol: String,
    /// Institution net-buy amount / 기관계순매수거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sysmsamt: String,
    /// Base index value / 기준지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upclose: String,
}

/// `t1621` response envelope — the header under the `t1621OutBlock` key, plus the
/// by-time rows under the `t1621OutBlock1` key (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]` so a terse/empty
/// envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1621Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1621OutBlock", default)]
    pub outblock: T1621OutBlock,
    #[serde(
        rename = "t1621OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1621OutBlock1>,
}

// ---------------------------------------------------------------------------
// t2545 — 상품선물투자자매매동향(챠트용) (F/O by-time, by-sector investor trading).
// market_session read; a single `t2545OutBlock` header (the product id / market
// flag + the base-index name/code) plus a repeated `t2545OutBlock1` by-time array
// (tolerated single-or-array via `ls_core::de_vec_or_single`); path
// /futureoption/investor. 6-field request — `eitem`/`sgubun`/`upcode`/`bgubun`
// are plain Strings; `nmin` and `cnt` are numeric REQUEST slots that MUST
// serialize as JSON NUMBERS via `ls_core::string_as_number` (KTD3 — the string
// form returns IGW40011). Pre-verified smoke uses `bgubun="0"` (`bgubun="1"`
// returns IGW40011/IGW50008).
// ---------------------------------------------------------------------------

/// Input block for `t2545` — the product id (`eitem`), market flag (`sgubun`),
/// sector code (`upcode`), the N-minute bucket (`nmin`), the requested row count
/// (`cnt`) and the previous-day flag (`bgubun`).
///
/// `nmin` and `cnt` are held as `String` but serialize as JSON NUMBERS via
/// [`ls_core::string_as_number`] (KTD3 — the string form returns `IGW40011`).
/// `eitem`/`sgubun`/`upcode`/`bgubun` serialize as ordinary Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T2545InBlock {
    /// Product id / 상품ID (e.g. `"01"`).
    pub eitem: String,
    /// Market flag / 시장구분 (e.g. `"0"`).
    pub sgubun: String,
    /// Sector code / 업종코드 (e.g. `"001"`).
    pub upcode: String,
    /// N-minute bucket / N분 (numeric request slot, KTD3).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub nmin: String,
    /// Requested row count / 조회건수 (numeric request slot, KTD3).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Previous-day flag / 전일분 (use `"0"` — `"1"` returns IGW40011/IGW50008).
    pub bgubun: String,
}

/// `t2545` request — serializes to
/// `{"t2545InBlock":{"eitem":...,"sgubun":...,"upcode":...,"nmin":0,"cnt":10,"bgubun":...}}`
/// (`nmin`/`cnt` as JSON numbers). Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T2545Request {
    #[serde(rename = "t2545InBlock")]
    pub inblock: T2545InBlock,
}

impl T2545Request {
    /// Build a `t2545` F/O by-time investor-trading request. `nmin`/`cnt` are
    /// passed as Strings but wire-serialize as JSON numbers (KTD3).
    pub fn new(
        eitem: impl Into<String>,
        sgubun: impl Into<String>,
        upcode: impl Into<String>,
        nmin: impl Into<String>,
        cnt: impl Into<String>,
        bgubun: impl Into<String>,
    ) -> Self {
        T2545Request {
            inblock: T2545InBlock {
                eitem: eitem.into(),
                sgubun: sgubun.into(),
                upcode: upcode.into(),
                nmin: nmin.into(),
                cnt: cnt.into(),
                bgubun: bgubun.into(),
            },
        }
    }
}

/// `t2545OutBlock` — the header block: the product id / market flag and the
/// base-index code/name (a representative, spec-grounded subset of the
/// investor-class header). Every modeled field via [`ls_core::string_or_number`];
/// `#[serde(default)]` so a sparse/absent header deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2545OutBlock {
    /// Product id / 상품ID.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub eitem: String,
    /// Market flag / 시장구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sgubun: String,
    /// Base-index code / 기준지수코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisucd: String,
    /// Base-index name / 기준지수명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisunm: String,
}

/// `t2545OutBlock1` — one by-time investor-trading row (a representative,
/// spec-grounded subset): the date/time keys, the individual / foreign /
/// institution net-buy volume and amount, and the base-index value. Every
/// numeric-bearing field via [`ls_core::string_or_number`]; `#[serde(default)]`
/// lets a sparse row deserialize and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2545OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Date+time / 일자시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub datetime: String,
    /// Individual net-buy volume / 개인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indmsvol: String,
    /// Individual net-buy amount / 개인순매수거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indmsamt: String,
    /// Foreign net-buy volume / 외국인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub formsvol: String,
    /// Foreign net-buy amount / 외국인순매수거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub formsamt: String,
    /// Institution net-buy volume / 기관계순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sysmsvol: String,
    /// Institution net-buy amount / 기관계순매수거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sysmsamt: String,
    /// Base index value / 기준지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upclose: String,
}

/// `t2545` response envelope — the header under the `t2545OutBlock` key, plus the
/// by-time rows under the `t2545OutBlock1` key (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]` so a terse/empty
/// envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2545Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2545OutBlock", default)]
    pub outblock: T2545OutBlock,
    #[serde(
        rename = "t2545OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2545OutBlock1>,
}

// ---------------------------------------------------------------------------
// t8406 — 주식선물틱분별체결조회(API용) (F/O by-tick/by-minute conclusion board).
// market_session read; a repeated `t8406OutBlock1` conclusion array (tolerated
// single-or-array via `ls_core::de_vec_or_single`); path
// /futureoption/market-data. 4-field request — `focode` (F/O short code) and
// `cgubun` (chart flag) are plain Strings; `bgubun` (minute flag) and `cnt`
// (row count) are numeric REQUEST slots that MUST serialize as JSON NUMBERS via
// `ls_core::string_as_number` (KTD3 — the string form returns IGW40011). The
// example `focode="101TC000"`/`"111T6000"` is a stale contract; a live smoke
// self-sources a front-month contract from the t8467 index-futures master.
// ---------------------------------------------------------------------------

/// Input block for `t8406` — the F/O short code (`focode`), the chart flag
/// (`cgubun`), the minute-bucket flag (`bgubun`) and the requested row count
/// (`cnt`).
///
/// `bgubun` and `cnt` are held as `String` but serialize as JSON NUMBERS via
/// [`ls_core::string_as_number`] (KTD3 — the string form returns `IGW40011`).
/// `focode`/`cgubun` serialize as ordinary Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T8406InBlock {
    /// F/O short code / 단축코드 (e.g. a front-month index-futures contract).
    pub focode: String,
    /// Chart flag / 챠트구분 (e.g. `"1"`).
    pub cgubun: String,
    /// Minute-bucket flag / 분구분 (numeric request slot, KTD3).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub bgubun: String,
    /// Requested row count / 조회건수 (numeric request slot, KTD3).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
}

/// `t8406` request — serializes to
/// `{"t8406InBlock":{"focode":...,"cgubun":...,"bgubun":0,"cnt":10}}`
/// (`bgubun`/`cnt` as JSON numbers). Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T8406Request {
    #[serde(rename = "t8406InBlock")]
    pub inblock: T8406InBlock,
}

impl T8406Request {
    /// Build a `t8406` F/O by-tick/minute conclusion request. `bgubun`/`cnt` are
    /// passed as Strings but wire-serialize as JSON numbers (KTD3).
    pub fn new(
        focode: impl Into<String>,
        cgubun: impl Into<String>,
        bgubun: impl Into<String>,
        cnt: impl Into<String>,
    ) -> Self {
        T8406Request {
            inblock: T8406InBlock {
                focode: focode.into(),
                cgubun: cgubun.into(),
                bgubun: bgubun.into(),
                cnt: cnt.into(),
            },
        }
    }
}

/// `t8406OutBlock1` — one conclusion row (a representative, spec-grounded
/// subset): the conclusion time, current price, prior-day sign/change, the OHLC
/// quartet, traded volume/value, open-interest, and the conclusion quantity.
/// Every numeric-bearing field via [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a sparse row deserialize and unknown fields are
/// ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8406OutBlock1 {
    /// Conclusion time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prior-day sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Traded volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Traded value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Open interest / 미결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openyak: String,
    /// Conclusion quantity / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
}

/// `t8406` response envelope — the conclusion rows under the `t8406OutBlock1`
/// key (tolerated single-or-array via [`ls_core::de_vec_or_single`]). All
/// `#[serde(default)]` so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8406Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8406OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8406OutBlock1>,
}

/// Input block for `g3102` — 해외주식 시간대별 (overseas time-series tick read).
/// `readcnt` is the requested row COUNT and `cts_seq` the continuation
/// sequence — both numeric REQUEST fields serialized as JSON numbers
/// (`string_as_number`, KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct G3102InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Continuation sequence / 연속조회키 (serialized as a JSON number; `"0"`
    /// first page).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_seq: String,
}

/// `g3102` request — serializes to `{"g3102InBlock":{...}}` with `readcnt` /
/// `cts_seq` as JSON numbers.
#[derive(Serialize, Debug, Clone)]
pub struct G3102Request {
    #[serde(rename = "g3102InBlock")]
    pub inblock: G3102InBlock,
}
impl G3102Request {
    /// Build a `g3102` time-series request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
        readcnt: impl Into<String>,
        cts_seq: impl Into<String>,
    ) -> Self {
        G3102Request {
            inblock: G3102InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
                readcnt: readcnt.into(),
                cts_seq: cts_seq.into(),
            },
        }
    }
}

/// `g3102OutBlock` — the time-series header (single object): the echo + the
/// row count.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3102OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Continuation sequence / 연속조회키.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_seq: String,
    /// Returned row count / 레코드카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `g3102OutBlock1` — one time-series tick row (`g3102OutBlock1[]`, an ARRAY
/// block). `price` (현재가) is the canonical price field (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3102OutBlock1 {
    /// Local date / 현지일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub locdate: String,
    /// Local time / 현지시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub loctime: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Execution volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exevol: String,
}

/// `g3102` response envelope: header out-block + the row array under the
/// `g3102OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3102Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3102OutBlock", default)]
    pub outblock: G3102OutBlock,
    #[serde(
        rename = "g3102OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<G3102OutBlock1>,
}

/// Input block for `g3103` — 해외주식 일주월 조회 (overseas daily/weekly/monthly
/// chart). `gubun` selects the period (`"4"` = monthly) and `date` is the
/// reference date (조회일자).
#[derive(Serialize, Debug, Clone)]
pub struct G3103InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
    /// Period distinction / 주기구분 (`"4"` = monthly).
    pub gubun: String,
    /// Reference date / 조회일자 (`YYYYMMDD`).
    pub date: String,
}

/// `g3103` request — serializes to `{"g3103InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3103Request {
    #[serde(rename = "g3103InBlock")]
    pub inblock: G3103InBlock,
}
impl G3103Request {
    /// Build a `g3103` period-chart request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
        gubun: impl Into<String>,
        date: impl Into<String>,
    ) -> Self {
        G3103Request {
            inblock: G3103InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
                gubun: gubun.into(),
                date: date.into(),
            },
        }
    }
}

/// `g3103OutBlock` — the chart header (single object): the symbol echo + the
/// reference date.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3103OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Period distinction / 주기구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
    /// Reference date / 조회일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
}

/// `g3103OutBlock1` — one chart bar row (`g3103OutBlock1[]`, an ARRAY block).
/// `price` (현재가) is the canonical price field (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3103OutBlock1 {
    /// Business date / 영업일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chedate: String,
    /// Current (close) price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
}

/// `g3103` response envelope: header out-block + the bar array under the
/// `g3103OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3103Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3103OutBlock", default)]
    pub outblock: G3103OutBlock,
    #[serde(
        rename = "g3103OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<G3103OutBlock1>,
}

// ---------------------------------------------------------------------------
// o3104 — 해외선물 일별체결 조회 (overseas-futures daily executions). Non-paginated
// market-data read keyed by `shcode` + a `date`; an `o3104OutBlock1[]` array of
// daily rows (de_vec_or_single). All-lane closed-window flip wave (plan -003);
// a CURRENT front-month contract (CUSN26) persists rows under closure.
// ---------------------------------------------------------------------------

/// Input block for `o3104` — overseas-futures daily executions. `gubun` mode,
/// `shcode` contract, `date` selects the day. No numeric request fields.
#[derive(Serialize, Debug, Clone)]
pub struct O3104InBlock {
    /// Division / 구분.
    pub gubun: String,
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Date / 일자.
    pub date: String,
}

/// `o3104` request — serializes to `{"o3104InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3104Request {
    #[serde(rename = "o3104InBlock")]
    pub inblock: O3104InBlock,
}
impl O3104Request {
    /// Build an `o3104` daily-executions request (`shcode`, `date`); `gubun`
    /// defaults to `"0"`.
    pub fn new(shcode: impl Into<String>, date: impl Into<String>) -> Self {
        O3104Request {
            inblock: O3104InBlock {
                gubun: "0".to_string(),
                shcode: shcode.into(),
                date: date.into(),
            },
        }
    }
}

/// `o3104OutBlock1` — one daily-execution row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3104OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chedate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Price / 체결가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3104` response — daily rows under `o3104OutBlock1` (single-or-array).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3104Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "o3104OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3104OutBlock1>,
}

// ---------------------------------------------------------------------------
// F-O market-data reads (plan -001 open-window flip wave). Non-paginated reads on
// /futureoption/market-data, keyed by a contract code self-sourced at runtime from
// the t8467 index-futures master (front-month codes expire — never hard-coded).
// Numeric request slots (actprice/cvolume/nmin/cnt) serialize as JSON numbers via
// `string_as_number` (the string form returns IGW40011).
// ---------------------------------------------------------------------------

/// Input block for `t8427` — 선물옵션 N분주가 (F/O minute/day chart). `actprice` is a
/// genuinely-numeric request slot (JSON number; IGW40011 guard); the rest are
/// request Strings. `focode` is the contract; `dt_gbn`/`min_term` select day vs.
/// minute granularity.
#[derive(Serialize, Debug, Clone)]
pub struct T8427InBlock {
    /// F/O division / 선물옵션구분 (`"F"` futures).
    pub fo_gbn: String,
    /// Query year / 조회년도 (YYYY).
    pub yyyy: String,
    /// Query month / 조회월 (MM).
    pub mm: String,
    /// Call/put division / 옵션콜풋구분.
    pub cp_gbn: String,
    /// Option strike / 옵션행사가 (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub actprice: String,
    /// Contract code / 선물옵션코드.
    pub focode: String,
    /// Day/minute division / 일분구분.
    pub dt_gbn: String,
    /// Minute interval / 분간격.
    pub min_term: String,
    /// Anchor date / 날짜 (YYYYMMDD).
    pub date: String,
    /// Time / 시간 (HHMMSS).
    pub time: String,
}

/// `t8427` request — serializes to `{"t8427InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T8427Request {
    #[serde(rename = "t8427InBlock")]
    pub inblock: T8427InBlock,
}
impl T8427Request {
    /// Build a `t8427` F/O day-chart request for one futures `focode` on `date`
    /// (`fo_gbn="F"`, `dt_gbn="1"` daily, `actprice=0`, empty call/put + minute
    /// fields). `yyyy`/`mm` bound the query month.
    pub fn new(
        focode: impl Into<String>,
        yyyy: impl Into<String>,
        mm: impl Into<String>,
        date: impl Into<String>,
    ) -> Self {
        T8427Request {
            inblock: T8427InBlock {
                fo_gbn: "F".to_string(),
                yyyy: yyyy.into(),
                mm: mm.into(),
                cp_gbn: String::new(),
                actprice: "0".to_string(),
                focode: focode.into(),
                dt_gbn: "1".to_string(),
                min_term: String::new(),
                date: date.into(),
                time: String::new(),
            },
        }
    }
}

/// `t8427OutBlock1` — one OHLCV chart row (representative subset). `close`/`volume`
/// are the substantive witnesses.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8427OutBlock1 {
    /// Date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openyak: String,
}

/// `t8427` response — chart rows under `t8427OutBlock1` (single-or-array). The
/// `t8427OutBlock` header carries the echo `focode`/`date`/`time`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8427Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8427OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8427OutBlock1>,
}

/// Input block for `t2210` — 선물옵션 특이거래량 (F/O unusual-volume conclusion
/// counts) over a time window. `cvolume` (특이거래량 threshold) is a genuinely-numeric
/// request slot (JSON number; IGW40011 guard); the rest are request Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T2210InBlock {
    /// Contract code / 단축코드.
    pub focode: String,
    /// Unusual-volume threshold / 특이거래량 (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cvolume: String,
    /// Window start / 시작시간 (HHMM).
    pub stime: String,
    /// Window end / 종료시간 (HHMM).
    pub etime: String,
}

/// `t2210` request — serializes to `{"t2210InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T2210Request {
    #[serde(rename = "t2210InBlock")]
    pub inblock: T2210InBlock,
}
impl T2210Request {
    /// Build a `t2210` unusual-volume request for one `focode` over `stime`..`etime`
    /// (`cvolume=0` = no threshold filter).
    pub fn new(
        focode: impl Into<String>,
        stime: impl Into<String>,
        etime: impl Into<String>,
    ) -> Self {
        T2210Request {
            inblock: T2210InBlock {
                focode: focode.into(),
                cvolume: "0".to_string(),
                stime: stime.into(),
                etime: etime.into(),
            },
        }
    }
}

/// `t2210OutBlock` — the buy/sell conclusion counts. `msvolume`/`mdvolume` (매수/매도
/// 체결수량) are the substantive witnesses (a NON-ZERO count proves real flow).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2210OutBlock {
    /// Sell conclusion volume / 매도체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// Sell conclusion count / 매도체결건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdchecnt: String,
    /// Buy conclusion volume / 매수체결수량 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// Buy conclusion count / 매수체결건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mschecnt: String,
}

/// `t2210` response — single conclusion-count block under `t2210OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2210Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2210OutBlock", default)]
    pub outblock: T2210OutBlock,
}

/// Input block for `t2424` — 선물옵션 N분봉 (F/O N-minute bars). `nmin` (N분) and `cnt`
/// (조회건수) are genuinely-numeric request slots (JSON numbers; IGW40011 guard).
#[derive(Serialize, Debug, Clone)]
pub struct T2424InBlock {
    /// Contract code / 종목코드.
    pub focode: String,
    /// Day/minute division / 분일구분.
    pub bdgubun: String,
    /// N-minute interval / N분 (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub nmin: String,
    /// Same-day continuation division / 당일연결구분.
    pub tcgubun: String,
    /// Requested bar count / 조회건수 (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
}

/// `t2424` request — serializes to `{"t2424InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T2424Request {
    #[serde(rename = "t2424InBlock")]
    pub inblock: T2424InBlock,
}
impl T2424Request {
    /// Build a `t2424` N-minute-bar request for one `focode` (`bdgubun="1"`,
    /// `tcgubun="0"`, `nmin=1`, `cnt="20"`).
    pub fn new(focode: impl Into<String>) -> Self {
        T2424Request {
            inblock: T2424InBlock {
                focode: focode.into(),
                bdgubun: "1".to_string(),
                nmin: "1".to_string(),
                tcgubun: "0".to_string(),
                cnt: "20".to_string(),
            },
        }
    }
}

/// `t2424OutBlock` — the current-price header (`price` 현재가 is the substantive
/// witness; the volume/open-interest aggregates).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2424OutBlock {
    /// Current price / 현재가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Conclusion volume / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openyak: String,
}

/// `t2424OutBlock1` — one N-minute bar (representative subset). `close` is a
/// substantive witness.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2424OutBlock1 {
    /// Date-time / 일자시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dt: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
}

/// `t2424` response — header under `t2424OutBlock`; bars under `t2424OutBlock1`
/// (single-or-array).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2424Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2424OutBlock", default)]
    pub outblock: T2424OutBlock,
    #[serde(
        rename = "t2424OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2424OutBlock1>,
}

// ---------------------------------------------------------------------------
// t8428 — 투자자별 예탁금추이 (deposit-balance trend by investor). Non-paginated
// /stock/investinfo read over a from/to date range; a `t8428OutBlock1[]` row array
// (de_vec_or_single). `cnt` is the genuinely-numeric request count.
// ---------------------------------------------------------------------------

/// Input block for `t8428` — deposit-balance trend. `cnt` is a genuinely-numeric
/// request slot (JSON number; IGW40011 guard); the rest are request Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T8428InBlock {
    /// From date / from일자 (YYYYMMDD).
    pub fdate: String,
    /// To date / to일자 (YYYYMMDD).
    pub tdate: String,
    /// Division / 구분.
    pub gubun: String,
    /// Cursor date / 날짜 (continuation; first page = `""`).
    pub key_date: String,
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Requested row count / 조회건수 (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
}

/// `t8428` request — serializes to `{"t8428InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T8428Request {
    #[serde(rename = "t8428InBlock")]
    pub inblock: T8428InBlock,
}
impl T8428Request {
    /// Build a `t8428` deposit-trend request over `fdate`..`tdate` for one `upcode`
    /// (`gubun="1"`, empty first-page `key_date`, `cnt="20"`).
    pub fn new(
        fdate: impl Into<String>,
        tdate: impl Into<String>,
        upcode: impl Into<String>,
    ) -> Self {
        T8428Request {
            inblock: T8428InBlock {
                fdate: fdate.into(),
                tdate: tdate.into(),
                gubun: "1".to_string(),
                key_date: String::new(),
                upcode: upcode.into(),
                cnt: "20".to_string(),
            },
        }
    }
}

/// `t8428OutBlock1` — one deposit-trend row (representative subset). `jisu` (지수) and
/// `custmoney` (고객예탁금) are the substantive witnesses.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8428OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Index / 지수 (a substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// Change sign / 대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Customer deposit (억원) / 고객예탁금 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub custmoney: String,
    /// Deposit change (억원) / 예탁증감.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yecha: String,
}

/// `t8428` response — deposit-trend rows under `t8428OutBlock1` (single-or-array).
/// The `t8428OutBlock` header carries the cursor `date`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8428Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8428OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8428OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1302 — 주식분별주가조회 (minute-by-minute price). Non-paginated; a `cts_time`
// summary out-block + a `t1302OutBlock1[]` minute-row array (de_vec_or_single).
// `cnt` is the genuinely-numeric request count (JSON number / IGW40011 guard);
// `gubun`/`time`/`exchgubun` stay strings. Wire keys from the raw res_example
// (KTD3). Plan -004 batch flip.
// ---------------------------------------------------------------------------

/// Input block for `t1302` — 주식분별주가 query (one symbol, minute interval).
#[derive(Serialize, Debug, Clone)]
pub struct T1302InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Interval division / 분주기 (0:30초 1:1분 ...).
    pub gubun: String,
    /// Base time / 기준시간 (HHMMSS; empty = latest).
    pub time: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Exchange distinction / 거래소구분 (K/N/U).
    pub exchgubun: String,
}

/// `t1302` request — wraps the input block under the `t1302InBlock` key.
#[derive(Serialize, Debug, Clone)]
pub struct T1302Request {
    #[serde(rename = "t1302InBlock")]
    pub inblock: T1302InBlock,
}

impl T1302Request {
    /// Build a `t1302` minute-price request. `time` empty (latest), `exchgubun`="K".
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        cnt: impl Into<String>,
    ) -> Self {
        T1302Request {
            inblock: T1302InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                time: String::new(),
                cnt: cnt.into(),
                exchgubun: "K".to_string(),
            },
        }
    }
}

/// `t1302OutBlock` — the minute-price summary (echoed continuation time).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1302OutBlock {
    /// Echoed continuation time / 연속시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
}

/// `t1302OutBlock1` — one minute-price row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1302OutBlock1 {
    /// Trade time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Close / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Change vs prior / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Interval volume / 분거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Sell-trade volume / 매도체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// Buy-trade volume / 매수체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
}

/// `t1302` response: summary `outblock` + `outblock1` minute-row array
/// (single-or-array tolerant).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1302Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1302OutBlock", default)]
    pub outblock: T1302OutBlock,
    #[serde(
        rename = "t1302OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1302OutBlock1>,
}

// ---------------------------------------------------------------------------
// t2216 — 선물옵션틱분별체결조회차트 (F/O tick/min trade chart). Non-paginated; a
// single `t2216OutBlock1[]` trade-row array (de_vec_or_single), no summary block.
// `bgubun`/`cnt` are numeric request fields (JSON numbers / IGW40011 guard);
// `focode` is a CURRENT contract sourced from a master at smoke time. Plan -004
// batch B.
// ---------------------------------------------------------------------------

/// Input block for `t2216` — F/O tick/min chart query (one contract).
#[derive(Serialize, Debug, Clone)]
pub struct T2216InBlock {
    /// Contract code / 단축코드 (current front-month).
    pub focode: String,
    /// Tick/min selector / 차트구분 (T:틱 ...).
    pub cgubun: String,
    /// Bar interval / 단위 (JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub bgubun: String,
    /// Requested row count / 요청건수 (JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
}

/// `t2216` request — wraps the input block under the `t2216InBlock` key.
#[derive(Serialize, Debug, Clone)]
pub struct T2216Request {
    #[serde(rename = "t2216InBlock")]
    pub inblock: T2216InBlock,
}

impl T2216Request {
    /// Build a `t2216` F/O tick chart request. `bgubun`=0 (default unit).
    pub fn new(focode: impl Into<String>, cgubun: impl Into<String>, cnt: impl Into<String>) -> Self {
        T2216Request {
            inblock: T2216InBlock {
                focode: focode.into(),
                cgubun: cgubun.into(),
                bgubun: "0".to_string(),
                cnt: cnt.into(),
            },
        }
    }
}

/// `t2216OutBlock1` — one F/O trade row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2216OutBlock1 {
    /// Trade time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제약정.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openyak: String,
}

/// `t2216` response: a single `outblock1` trade-row array (single-or-array tolerant).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2216Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t2216OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2216OutBlock1>,
}
