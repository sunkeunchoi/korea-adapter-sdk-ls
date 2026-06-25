//! `t8412` 주식차트(N분) — header-cursor pagination (multi-page).
//!
//! SELF-paginated market-data that threads an LS continuation through the
//! `tr_cont`/`tr_cont_key` HTTP headers. `t8412` returns a chart whose candle rows
//! (`t8412OutBlock1[]`) span more pages than fit one response; the gateway signals
//! "more rows available" via the `tr_cont`/`tr_cont_key` HTTP response headers, and
//! the caller walks pages until that header goes empty.
//!
//! ## Continuation rides as HTTP headers, never in the body
//!
//! The continuation tokens are transport headers, surfaced through the
//! [`ls_core::HasPagination`] trait implemented on [`T8412Request`]. The wrapper
//! carries `tr_cont`/`tr_cont_key` as `#[serde(skip)]` fields so they NEVER
//! serialize into the request body — `dispatch_once` reads them via the trait and
//! injects them as `tr_cont`/`tr_cont_key` HTTP request headers. `collect_all`
//! reads the matching RESPONSE headers (injected into the JSON by `dispatch_once`)
//! and copies them onto the next page request.
//!
//! ## `cts_*` body fields vs. `tr_cont`/`tr_cont_key` headers — distinct
//!
//! These are two unrelated continuation mechanisms and the port keeps them apart:
//!
//! - `cts_date`/`cts_time` are BODY fields the server echoes in both the in-block
//!   and the summary out-block. They are part of the TR's own query semantics.
//! - `tr_cont`/`tr_cont_key` are the TRANSPORT continuation, carried as HTTP
//!   headers, and are what `collect_all` walks. They never touch the body.
//!
//! ## Pin an explicit trading day
//!
//! Date fields (`sdate`/`edate`/`cts_date`) must be pinned to a real weekday.
//! Empty date fields default to "today" on the LS gateway and fail on weekends
//! with a misleading `01715`, so the tests and fixtures pin `20240105` (a Friday).
//!
//! ## Wire-compat: string-or-number, single-or-array
//!
//! Numeric chart fields arrive as either JSON numbers or strings, so every such
//! field uses [`ls_core::string_or_number`]. The `t8412OutBlock1[]` row array is
//! tolerated as either a single object or an array via
//! [`ls_core::de_vec_or_single`] (the gateway collapses a one-row page to a bare
//! object). Both are the load-bearing behaviors R10 preserves.

use serde::{Deserialize, Serialize};

/// Input block for `t8412` — the chart query parameters.
///
/// Field names mirror the LS spec (`specs/ls_openapi_specs.json` → `t8412InBlock`)
/// verbatim. `shcode` is the caller-supplied symbol; `cts_date`/`cts_time` are the
/// body-level continuation the server echoes (distinct from the `tr_cont` HTTP
/// header continuation that drives the page loop).
#[derive(Serialize, Debug, Clone)]
pub struct T8412InBlock {
    /// Short code / 단축코드 (e.g. `"078020"`).
    pub shcode: String,
    /// N-minute interval / N분 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ncnt: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Number of days / 조회영업일수.
    pub nday: String,
    /// Start date / 시작일자 (YYYYMMDD; pin a weekday).
    pub sdate: String,
    /// Start time / 시작시간.
    pub stime: String,
    /// End date / 종료일자 (YYYYMMDD; pin a weekday).
    pub edate: String,
    /// End time / 종료시간.
    pub etime: String,
    /// Continuation date echoed by the server / 연속일자 (body field, not the header).
    pub cts_date: String,
    /// Continuation time echoed by the server / 연속시간 (body field, not the header).
    pub cts_time: String,
    /// Compression flag / 압축여부 (`"Y"`/`"N"`).
    pub comp_yn: String,
}

/// `t8412` request — wraps the input block under the `t8412InBlock` key.
///
/// Serializes to `{"t8412InBlock":{...}}`. The `tr_cont`/`tr_cont_key` fields are
/// `#[serde(skip)]`, so they NEVER appear in the body; they are carried as HTTP
/// headers via the [`ls_core::HasPagination`] impl below and walked by
/// `collect_all`.
#[derive(Serialize, Debug, Clone)]
pub struct T8412Request {
    #[serde(rename = "t8412InBlock")]
    pub inblock: T8412InBlock,
    /// Pagination continuation token (set by `collect_all`; injected as HTTP header).
    #[serde(skip)]
    pub tr_cont: String,
    /// Pagination continuation key (set by `collect_all`; injected as HTTP header).
    #[serde(skip)]
    pub tr_cont_key: String,
}

// Continuation tokens ride as HTTP headers via this trait; the macro is exported
// from `ls-core` (`#[macro_export]`) because paginated request structs live in
// `ls-sdk`, so it is invoked by its crate-qualified path.
ls_core::impl_has_pagination!(T8412Request);

impl T8412Request {
    /// Build a `t8412` chart request for one symbol over a pinned date range.
    ///
    /// Continuation fields (`tr_cont`/`tr_cont_key`) start empty (first page); the
    /// body `cts_date`/`cts_time` default empty unless the caller threads them.
    /// Callers MUST pass a real trading day for `sdate`/`edate` — empty dates
    /// default to today on the gateway and fail on weekends with `01715`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shcode: impl Into<String>,
        ncnt: impl Into<String>,
        qrycnt: impl Into<String>,
        nday: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
        comp_yn: impl Into<String>,
    ) -> Self {
        T8412Request {
            inblock: T8412InBlock {
                shcode: shcode.into(),
                ncnt: ncnt.into(),
                qrycnt: qrycnt.into(),
                nday: nday.into(),
                sdate: sdate.into(),
                stime: String::new(),
                edate: edate.into(),
                etime: String::new(),
                cts_date: String::new(),
                cts_time: String::new(),
                comp_yn: comp_yn.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t8412OutBlock` — the chart summary block.
///
/// Carries the prior-day/current OHLC aggregates plus the echoed `cts_date`/
/// `cts_time` (the body-level continuation, distinct from the transport headers).
/// Every numeric-bearing field uses [`ls_core::string_or_number`]; `#[serde(default)]`
/// lets a sparse block deserialize cleanly. Field names mirror the spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8412OutBlock {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Prior-day open / 전일시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisiga: String,
    /// Prior-day high / 전일고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jihigh: String,
    /// Prior-day low / 전일저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jilow: String,
    /// Prior-day close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jiclose: String,
    /// Prior-day volume / 전일거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jivolume: String,
    /// Current open / 당일시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub disiga: String,
    /// Current high / 당일고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dihigh: String,
    /// Current low / 당일저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dilow: String,
    /// Current close / 당일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diclose: String,
    /// Upper limit / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highend: String,
    /// Lower limit / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lowend: String,
    /// Echoed continuation date / 연속일자 (body field, not the `tr_cont` header).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Echoed continuation time / 연속시간 (body field, not the `tr_cont` header).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
    /// Session start time / 장 시작시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub s_time: String,
    /// Session end time / 장 종료시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub e_time: String,
    /// Minutes per candle / 분틱 단위.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dshmin: String,
    /// Returned row count / 레코드 카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `t8412OutBlock1` — one chart candle row.
///
/// The repeated row block; `t8412OutBlock1[]` is the array `collect_all`
/// concatenates across pages. Field names mirror the spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8412OutBlock1 {
    /// Candle date / 날짜 (YYYYMMDD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Candle time / 시간.
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
    /// Close / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jdiff_vol: String,
    /// Traded value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Issue check / 수정주가 구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jongchk: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rate: String,
    /// Sign / 전일대비 구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
}

/// `t8412` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in `ls-core`
/// dispatch before this struct is built). `outblock` is the summary block;
/// `outblock1` is the candle row array, tolerated as a single object OR an array
/// via [`ls_core::de_vec_or_single`].
///
/// `tr_cont`/`tr_cont_key` are NOT part of the LS wire body — `dispatch_once`
/// injects the matching RESPONSE headers into the JSON before this struct is
/// built, and the [`ls_core::HasPagination`] impl exposes them so `collect_all`
/// can walk pages.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8412Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8412OutBlock", default)]
    pub outblock: T8412OutBlock,
    #[serde(
        rename = "t8412OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8412OutBlock1>,
    /// Continuation token from the response header (injected by `dispatch_once`).
    #[serde(default)]
    pub tr_cont: String,
    /// Continuation key from the response header (injected by `dispatch_once`).
    #[serde(default)]
    pub tr_cont_key: String,
}

// The response also implements `HasPagination`: `collect_all` requires `Res:
// HasPagination` so it can read the continuation off each page and decide whether
// to fetch another.
ls_core::impl_has_pagination!(T8412Response);

// ===========================================================================
// Domestic stock / sector master/reference charts (plan -004). Same shape as
// `t8412`: a header summary out-block + a `de_vec_or_single` candle-row array,
// self-paginated on the body `cts_date` cursor (header `tr_cont`/`tr_cont_key`
// skipped, mirror `t1514`). Single-page facade scope. Numeric request counts
// (`qrycnt`/`ncnt`) serialize as JSON numbers via `string_as_number` (IGW40011
// guard). Wire keys + array-ness read from the raw `res_example` (KTD3).
// ===========================================================================

/// Input block for `t8410` — API전용주식차트(일주월년) (stock day/week/month/year
/// chart). `gubun` selects the period (2:일 3:주 4:월 5:년); `qrycnt` is the
/// genuinely-numeric request count (JSON number); `cts_date` is the body
/// continuation cursor.
#[derive(Serialize, Debug, Clone)]
pub struct T8410InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Period division / 주기구분 (2:일 3:주 4:월 5:년).
    pub gubun: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Start date / 시작일자 (YYYYMMDD; pin a weekday).
    pub sdate: String,
    /// End date / 종료일자 (YYYYMMDD; pin a weekday).
    pub edate: String,
    /// Body continuation cursor / 연속일자 (first page empty).
    pub cts_date: String,
    /// Compression flag / 압축여부 (Y/N).
    pub comp_yn: String,
    /// Adjusted-price flag / 수정주가여부 (Y/N).
    pub sujung: String,
}

/// `t8410` request (self-paginated; `cts_date` body cursor, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T8410Request {
    #[serde(rename = "t8410InBlock")]
    pub inblock: T8410InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T8410Request);
impl T8410Request {
    /// Build a first-page `t8410` chart request for one symbol over a pinned date
    /// range. `comp_yn`="N" (uncompressed), `sujung`="Y" (adjusted); callers may
    /// set `inblock` fields directly to override.
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        qrycnt: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        T8410Request {
            inblock: T8410InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                qrycnt: qrycnt.into(),
                sdate: sdate.into(),
                edate: edate.into(),
                cts_date: String::new(),
                comp_yn: "N".to_string(),
                sujung: "Y".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t8410OutBlock` — the chart summary (prior/current OHLC + limits + echoed cursor).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8410OutBlock {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Prior-day open / 전일시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisiga: String,
    /// Prior-day high / 전일고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jihigh: String,
    /// Prior-day low / 전일저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jilow: String,
    /// Prior-day close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jiclose: String,
    /// Prior-day volume / 전일거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jivolume: String,
    /// Current open / 당일시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub disiga: String,
    /// Current high / 당일고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dihigh: String,
    /// Current low / 당일저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dilow: String,
    /// Current close / 당일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diclose: String,
    /// Upper limit / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highend: String,
    /// Lower limit / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lowend: String,
    /// Echoed continuation date / 연속일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Returned row count / 레코드카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
    /// Static-VI upper / 정적VI상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svi_uplmtprice: String,
    /// Static-VI lower / 정적VI하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svi_dnlmtprice: String,
}

/// `t8410OutBlock1` — one daily/weekly/monthly candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8410OutBlock1 {
    /// Candle date / 날짜 (YYYYMMDD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
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
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jdiff_vol: String,
    /// Traded value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Adjust flag / 수정구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jongchk: String,
    /// Adjust ratio / 수정비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rate: String,
    /// Adjusted item / 수정주가반영항목.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricechk: String,
    /// Adjusted-ratio value / 수정비율반영거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ratevalue: String,
    /// Close sign / 종가등락구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
}

/// `t8410` response (single page): header `outblock` + `outblock1` candle array.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8410Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8410OutBlock", default)]
    pub outblock: T8410OutBlock,
    #[serde(
        rename = "t8410OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8410OutBlock1>,
}

/// Input block for `t8451` — (통합)주식챠트(일주월년) (integrated stock D/W/M/Y
/// chart). Like `t8410` plus an `exchgubun` exchange selector.
#[derive(Serialize, Debug, Clone)]
pub struct T8451InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Period division / 주기구분 (2:일 3:주 4:월 5:년).
    pub gubun: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Start date / 시작일자.
    pub sdate: String,
    /// End date / 종료일자.
    pub edate: String,
    /// Body continuation cursor / 연속일자.
    pub cts_date: String,
    /// Compression flag / 압축여부 (N:비압축).
    pub comp_yn: String,
    /// Adjusted-price flag / 수정주가여부 (Y/N).
    pub sujung: String,
    /// Exchange selector / 거래소구분코드.
    pub exchgubun: String,
}

/// `t8451` request (self-paginated; `cts_date` body cursor, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T8451Request {
    #[serde(rename = "t8451InBlock")]
    pub inblock: T8451InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T8451Request);
impl T8451Request {
    /// Build a first-page `t8451` chart request. `comp_yn`="N", `sujung`="N",
    /// `exchgubun`="N"; override `inblock` fields directly as needed.
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        qrycnt: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        T8451Request {
            inblock: T8451InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                qrycnt: qrycnt.into(),
                sdate: sdate.into(),
                edate: edate.into(),
                cts_date: String::new(),
                comp_yn: "N".to_string(),
                sujung: "N".to_string(),
                exchgubun: "N".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t8451OutBlock` — chart summary (OHLC + limits + NXT pre/after-market times).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8451OutBlock {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Prior-day open / 전일시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisiga: String,
    /// Prior-day high / 전일고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jihigh: String,
    /// Prior-day low / 전일저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jilow: String,
    /// Prior-day close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jiclose: String,
    /// Prior-day volume / 전일거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jivolume: String,
    /// Current close / 당일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diclose: String,
    /// Upper limit / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highend: String,
    /// Lower limit / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lowend: String,
    /// Echoed continuation date / 연속일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Returned row count / 레코드카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `t8451OutBlock1` — one candle row (same layout as `t8410OutBlock1`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8451OutBlock1 {
    /// Candle date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
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
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jdiff_vol: String,
    /// Traded value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Adjust flag / 수정구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jongchk: String,
    /// Close sign / 종가등락구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
}

/// `t8451` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8451Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8451OutBlock", default)]
    pub outblock: T8451OutBlock,
    #[serde(
        rename = "t8451OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8451OutBlock1>,
}

/// Input block for `t8419` — 업종차트(일주월) (sector day/week/month chart).
/// `shcode` carries the sector code (e.g. "001"); no adjusted-price flag.
#[derive(Serialize, Debug, Clone)]
pub struct T8419InBlock {
    /// Sector code / 단축코드 (e.g. "001").
    pub shcode: String,
    /// Period division / 주기구분 (2:일 3:주 4:월).
    pub gubun: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Start date / 시작일자.
    pub sdate: String,
    /// End date / 종료일자.
    pub edate: String,
    /// Body continuation cursor / 연속일자.
    pub cts_date: String,
    /// Compression flag / 압축여부 (Y/N).
    pub comp_yn: String,
}

/// `t8419` request (self-paginated; `cts_date` body cursor, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T8419Request {
    #[serde(rename = "t8419InBlock")]
    pub inblock: T8419InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T8419Request);
impl T8419Request {
    /// Build a first-page `t8419` sector-chart request. `comp_yn`="N".
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        qrycnt: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        T8419Request {
            inblock: T8419InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                qrycnt: qrycnt.into(),
                sdate: sdate.into(),
                edate: edate.into(),
                cts_date: String::new(),
                comp_yn: "N".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t8419OutBlock` — sector-chart summary (index OHLC + current traded value).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8419OutBlock {
    /// Sector code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Prior-day open index / 전일시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisiga: String,
    /// Prior-day high index / 전일고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jihigh: String,
    /// Prior-day low index / 전일저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jilow: String,
    /// Prior-day close index / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jiclose: String,
    /// Prior-day volume / 전일거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jivolume: String,
    /// Current close index / 당일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diclose: String,
    /// Current traded value / 당일거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub disvalue: String,
    /// Echoed continuation date / 연속일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Returned row count / 레코드카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `t8419OutBlock1` — one sector candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8419OutBlock1 {
    /// Candle date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Open index / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High index / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low index / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Close index / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jdiff_vol: String,
    /// Traded value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t8419` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8419Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8419OutBlock", default)]
    pub outblock: T8419OutBlock,
    #[serde(
        rename = "t8419OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8419OutBlock1>,
}

/// Input block for `t4203` — 업종차트(종합) (sector composite chart). Carries the
/// tick count `ncnt`, the `tdgb` today-only selector, and a three-field body
/// continuation (`cts_date`/`cts_time`/`cts_daygb`). Both `ncnt` and `qrycnt` are
/// numeric (JSON numbers).
#[derive(Serialize, Debug, Clone)]
pub struct T4203InBlock {
    /// Sector code / 단축코드.
    pub shcode: String,
    /// Period division / 주기구분 (0:틱 1:분 2:일 3:주 4:월).
    pub gubun: String,
    /// Tick count / 틱개수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ncnt: String,
    /// Requested row count / 건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Today-only selector / 당일구분 (0:전체 1:당일만).
    pub tdgb: String,
    /// Start date / 시작일자.
    pub sdate: String,
    /// End date / 종료일자.
    pub edate: String,
    /// Body continuation date / 연속일자.
    pub cts_date: String,
    /// Body continuation time / 연속시간.
    pub cts_time: String,
    /// Continuation today-division / 연속당일구분.
    pub cts_daygb: String,
}

/// `t4203` request (self-paginated; `cts_date`/`cts_time` body cursors, header
/// cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T4203Request {
    #[serde(rename = "t4203InBlock")]
    pub inblock: T4203InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T4203Request);
impl T4203Request {
    /// Build a first-page `t4203` composite sector-chart request. `tdgb`="0"
    /// (all), `ncnt` defaults from the caller, continuation cursors empty.
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        ncnt: impl Into<String>,
        qrycnt: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        T4203Request {
            inblock: T4203InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                ncnt: ncnt.into(),
                qrycnt: qrycnt.into(),
                tdgb: "0".to_string(),
                sdate: sdate.into(),
                edate: edate.into(),
                cts_date: String::new(),
                cts_time: String::new(),
                cts_daygb: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t4203OutBlock` — composite sector-chart summary (index OHLC + 3-field cursor).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T4203OutBlock {
    /// Sector code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Prior-day open index / 전일시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisiga: String,
    /// Prior-day high index / 전일고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jihigh: String,
    /// Prior-day low index / 전일저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jilow: String,
    /// Prior-day close index / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jiclose: String,
    /// Prior-day volume / 전일거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jivolume: String,
    /// Current close index / 당일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diclose: String,
    /// Current traded value / 당일거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub disvalue: String,
    /// Echoed continuation date / 연속일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Echoed continuation time / 연속시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
    /// Echoed continuation today-division / 연속당일구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_daygb: String,
}

/// `t4203OutBlock1` — one composite sector candle row (carries an intraday `time`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T4203OutBlock1 {
    /// Candle date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Candle time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Open index / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High index / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low index / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Close index / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jdiff_vol: String,
    /// Traded value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t4203` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T4203Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t4203OutBlock", default)]
    pub outblock: T4203OutBlock,
    #[serde(
        rename = "t4203OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T4203OutBlock1>,
}
