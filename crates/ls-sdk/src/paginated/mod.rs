//! Paginated dependency class — `t8412` 주식차트(N분) (N-minute stock chart).
//!
//! This is the *paginated* class: SELF-paginated market-data TRs that thread an
//! LS continuation through `tr_cont`/`tr_cont_key`. `t8412` returns a chart whose
//! candle rows (`t8412OutBlock1[]`) span more pages than fit one response; the
//! gateway signals "more rows available" via the `tr_cont`/`tr_cont_key` HTTP
//! response headers, and the caller walks pages until that header goes empty.
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

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ls_core::{Inner, LsResult};

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
// Single-page body-`idx` paginated TRs (the implement-tr "second freeze"
// sub-pattern).
//
// These stock rank/screen TRs carry a request-BODY `idx` continuation cursor,
// for which `ls-core` has NO multi-page machinery (it only threads the header
// `tr_cont`/`tr_cont_key` cursor that `t8412` uses). They are therefore promoted
// at SINGLE-PAGE scope:
//   - `idx` is an ordinary serialized in-block field (a JSON number on the wire,
//     via `string_as_number`) at its first-page convention (`"0"`) — NOT
//     `#[serde(skip)]` (that attribute is only for `t8412`'s header cursors);
//   - dispatch is ONE `post_paginated` call with EMPTY `tr_cont`/`tr_cont_key`
//     headers (the request still impls `HasPagination` because `post_paginated`
//     requires it, but the cursors stay empty);
//   - out-rows tolerate single-or-array via `de_vec_or_single`.
// Multi-page collection over body-`idx` (a `chart_all`-equivalent) is deferred
// follow-up work — it needs a new `ls-core` body-continuation contract.
// ===========================================================================

/// Input block for `t1452` — 거래량상위 (top trading volume).
///
/// A rank-screen filter. Numeric fields serialize as JSON numbers
/// (`string_as_number`) per the spec's request shape; `idx` is the body
/// continuation cursor (first page = `"0"`).
#[derive(Serialize, Debug, Clone)]
pub struct T1452InBlock {
    /// Market division / 구분.
    pub gubun: String,
    /// Prior-day division / 전일구분.
    pub jnilgubun: String,
    /// Start change-rate / 시작등락율.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sdiff: String,
    /// End change-rate / 종료등락율.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ediff: String,
    /// Exclusion flags / 대상제외.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num: String,
    /// Start price / 시작가격.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    /// End price / 종료가격.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    /// Min volume / 거래량.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    /// Body continuation cursor / IDX (first page = `"0"`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1452` request — wraps the input block under the `t1452InBlock` key.
///
/// `idx` rides IN the body (an ordinary in-block field). The
/// `tr_cont`/`tr_cont_key` fields are `#[serde(skip)]` and stay empty for the
/// single-page call; they exist only to satisfy the `HasPagination` bound on
/// `post_paginated`.
#[derive(Serialize, Debug, Clone)]
pub struct T1452Request {
    #[serde(rename = "t1452InBlock")]
    pub inblock: T1452InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}

ls_core::impl_has_pagination!(T1452Request);

impl T1452Request {
    /// Build a single-page `t1452` top-volume request. `idx` defaults to the
    /// first-page convention (`"0"`); `tr_cont`/`tr_cont_key` start empty.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gubun: impl Into<String>,
        jnilgubun: impl Into<String>,
        sdiff: impl Into<String>,
        ediff: impl Into<String>,
        jc_num: impl Into<String>,
        sprice: impl Into<String>,
        eprice: impl Into<String>,
        volume: impl Into<String>,
    ) -> Self {
        T1452Request {
            inblock: T1452InBlock {
                gubun: gubun.into(),
                jnilgubun: jnilgubun.into(),
                sdiff: sdiff.into(),
                ediff: ediff.into(),
                jc_num: jc_num.into(),
                sprice: sprice.into(),
                eprice: eprice.into(),
                volume: volume.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1452OutBlock` — the rank-screen summary block (carries the next-page `idx`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1452OutBlock {
    /// Returned continuation cursor / IDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1452OutBlock1` — one ranked stock row. Representative subset; every field
/// via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1452OutBlock1 {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1452` response envelope (single page).
///
/// `outblock` is the summary (with the next-page `idx`); `outblock1` is the
/// ranked-row array under the `t1452OutBlock1` key, tolerated as single-or-array
/// via [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1452Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1452OutBlock", default)]
    pub outblock: T1452OutBlock,
    #[serde(
        rename = "t1452OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1452OutBlock1>,
}

// --- Shared single-page paginated row shape -------------------------------
// The six remaining rank/screen TRs all expose the same representative row
// subset; only their in-block filters and summary blocks differ. Each defines
// its own row type (kept distinct for per-TR doc clarity and future field
// expansion) but the field set is uniform.

/// One ranked stock row (representative subset; every field via
/// [`ls_core::string_or_number`]). Reused conceptually across the rank screens.
macro_rules! rank_row {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Serialize, Deserialize, Debug, Clone, Default)]
        #[serde(default)]
        pub struct $name {
            /// Korean name / 종목명.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub hname: String,
            /// Short code / 종목코드.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub shcode: String,
            /// Current price / 현재가.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub price: String,
            /// Sign / 전일대비구분.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub sign: String,
            /// Change vs. previous close / 전일대비.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub change: String,
            /// Rate of change / 등락율.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub diff: String,
            /// Accumulated volume / 누적거래량.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub volume: String,
        }
    };
}

/// The rank-screen summary block carrying only the next-page `idx` cursor.
macro_rules! idx_summary {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Serialize, Deserialize, Debug, Clone, Default)]
        #[serde(default)]
        pub struct $name {
            /// Returned continuation cursor / IDX.
            #[serde(deserialize_with = "ls_core::string_or_number")]
            pub idx: String,
        }
    };
}

// --- t1403 — 신규상장종목조회 (newly-listed stocks; date-range, single-page) ----

/// Input block for `t1403` — newly-listed stocks over a listing-month range.
#[derive(Serialize, Debug, Clone)]
pub struct T1403InBlock {
    /// Division / 구분.
    pub gubun: String,
    /// Start listing month / 시작상장월 (YYYYMM).
    pub styymm: String,
    /// End listing month / 종료상장월 (YYYYMM).
    pub enyymm: String,
    /// Body continuation cursor / IDX (first page = `"0"`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1403` request (single-page; `idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1403Request {
    #[serde(rename = "t1403InBlock")]
    pub inblock: T1403InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1403Request);
impl T1403Request {
    /// Build a single-page `t1403` request over a `[styymm, enyymm]` month range.
    pub fn new(
        gubun: impl Into<String>,
        styymm: impl Into<String>,
        enyymm: impl Into<String>,
    ) -> Self {
        T1403Request {
            inblock: T1403InBlock {
                gubun: gubun.into(),
                styymm: styymm.into(),
                enyymm: enyymm.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}
idx_summary!(T1403OutBlock, "`t1403OutBlock` — summary (next-page `idx`).");
rank_row!(T1403OutBlock1, "`t1403OutBlock1` — one newly-listed stock row.");

/// `t1403` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1403Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1403OutBlock", default)]
    pub outblock: T1403OutBlock,
    #[serde(
        rename = "t1403OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1403OutBlock1>,
}

// --- t1441 — 등락율상위 (top change rate; single-page) ----------------------

/// Input block for `t1441` — top change-rate screen filter.
#[derive(Serialize, Debug, Clone)]
pub struct T1441InBlock {
    pub gubun1: String,
    pub gubun2: String,
    pub gubun3: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num2: String,
    pub exchgubun: String,
    /// Body continuation cursor / IDX (first page = `"0"`).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1441` request (single-page).
#[derive(Serialize, Debug, Clone)]
pub struct T1441Request {
    #[serde(rename = "t1441InBlock")]
    pub inblock: T1441InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1441Request);
impl T1441Request {
    /// Build a single-page `t1441` top-change-rate request.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        gubun3: impl Into<String>,
        jc_num: impl Into<String>,
        sprice: impl Into<String>,
        eprice: impl Into<String>,
        volume: impl Into<String>,
        jc_num2: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1441Request {
            inblock: T1441InBlock {
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                gubun3: gubun3.into(),
                jc_num: jc_num.into(),
                sprice: sprice.into(),
                eprice: eprice.into(),
                volume: volume.into(),
                jc_num2: jc_num2.into(),
                exchgubun: exchgubun.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}
idx_summary!(T1441OutBlock, "`t1441OutBlock` — summary (next-page `idx`).");
rank_row!(T1441OutBlock1, "`t1441OutBlock1` — one ranked stock row.");

/// `t1441` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1441Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1441OutBlock", default)]
    pub outblock: T1441OutBlock,
    #[serde(
        rename = "t1441OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1441OutBlock1>,
}

// --- t1463 — 거래대금상위 (top trading value; single-page) -------------------

/// Input block for `t1463` — top trading-value screen filter.
#[derive(Serialize, Debug, Clone)]
pub struct T1463InBlock {
    pub gubun: String,
    pub jnilgubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num2: String,
    pub exchgubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1463` request (single-page).
#[derive(Serialize, Debug, Clone)]
pub struct T1463Request {
    #[serde(rename = "t1463InBlock")]
    pub inblock: T1463InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1463Request);
impl T1463Request {
    /// Build a single-page `t1463` top-trading-value request.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gubun: impl Into<String>,
        jnilgubun: impl Into<String>,
        jc_num: impl Into<String>,
        sprice: impl Into<String>,
        eprice: impl Into<String>,
        volume: impl Into<String>,
        jc_num2: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1463Request {
            inblock: T1463InBlock {
                gubun: gubun.into(),
                jnilgubun: jnilgubun.into(),
                jc_num: jc_num.into(),
                sprice: sprice.into(),
                eprice: eprice.into(),
                volume: volume.into(),
                jc_num2: jc_num2.into(),
                exchgubun: exchgubun.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}
idx_summary!(T1463OutBlock, "`t1463OutBlock` — summary (next-page `idx`).");
rank_row!(T1463OutBlock1, "`t1463OutBlock1` — one ranked stock row.");

/// `t1463` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1463Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1463OutBlock", default)]
    pub outblock: T1463OutBlock,
    #[serde(
        rename = "t1463OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1463OutBlock1>,
}

// --- t1466 — 전일동시간대비거래급증 (volume surge; single-page) --------------

/// Input block for `t1466` — volume-surge screen filter.
#[derive(Serialize, Debug, Clone)]
pub struct T1466InBlock {
    pub gubun: String,
    pub type1: String,
    pub type2: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num2: String,
    pub exchgubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1466` request (single-page).
#[derive(Serialize, Debug, Clone)]
pub struct T1466Request {
    #[serde(rename = "t1466InBlock")]
    pub inblock: T1466InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1466Request);
impl T1466Request {
    /// Build a single-page `t1466` volume-surge request.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gubun: impl Into<String>,
        type1: impl Into<String>,
        type2: impl Into<String>,
        jc_num: impl Into<String>,
        sprice: impl Into<String>,
        eprice: impl Into<String>,
        volume: impl Into<String>,
        jc_num2: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1466Request {
            inblock: T1466InBlock {
                gubun: gubun.into(),
                type1: type1.into(),
                type2: type2.into(),
                jc_num: jc_num.into(),
                sprice: sprice.into(),
                eprice: eprice.into(),
                volume: volume.into(),
                jc_num2: jc_num2.into(),
                exchgubun: exchgubun.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1466OutBlock` — summary block carrying `hhmm` and the next-page `idx`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1466OutBlock {
    /// Reference time / 기준시각.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hhmm: String,
    /// Returned continuation cursor / IDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}
rank_row!(T1466OutBlock1, "`t1466OutBlock1` — one ranked stock row.");

/// `t1466` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1466Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1466OutBlock", default)]
    pub outblock: T1466OutBlock,
    #[serde(
        rename = "t1466OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1466OutBlock1>,
}

// --- t1489 — 예상체결량상위조회 (top expected-execution volume; single-page) --

/// Input block for `t1489` — expected-execution-volume screen filter.
#[derive(Serialize, Debug, Clone)]
pub struct T1489InBlock {
    pub gubun: String,
    pub jgubun: String,
    pub jongchk: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub yesprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub yeeprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub yevolume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1489` request (single-page).
#[derive(Serialize, Debug, Clone)]
pub struct T1489Request {
    #[serde(rename = "t1489InBlock")]
    pub inblock: T1489InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1489Request);
impl T1489Request {
    /// Build a single-page `t1489` expected-execution-volume request.
    pub fn new(
        gubun: impl Into<String>,
        jgubun: impl Into<String>,
        jongchk: impl Into<String>,
        yesprice: impl Into<String>,
        yeeprice: impl Into<String>,
        yevolume: impl Into<String>,
    ) -> Self {
        T1489Request {
            inblock: T1489InBlock {
                gubun: gubun.into(),
                jgubun: jgubun.into(),
                jongchk: jongchk.into(),
                yesprice: yesprice.into(),
                yeeprice: yeeprice.into(),
                yevolume: yevolume.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}
idx_summary!(T1489OutBlock, "`t1489OutBlock` — summary (next-page `idx`).");
rank_row!(T1489OutBlock1, "`t1489OutBlock1` — one ranked stock row.");

/// `t1489` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1489Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1489OutBlock", default)]
    pub outblock: T1489OutBlock,
    #[serde(
        rename = "t1489OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1489OutBlock1>,
}

// --- t1492 — 단일가예상등락율상위 (single-price expected change rate) ---------

/// Input block for `t1492` — single-price expected-change-rate screen filter.
#[derive(Serialize, Debug, Clone)]
pub struct T1492InBlock {
    pub gubun1: String,
    pub gubun2: String,
    pub jongchk: String,
    /// Volume flag / 거래량 (a length-1 flag here; serialized as a string).
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1492` request (single-page).
#[derive(Serialize, Debug, Clone)]
pub struct T1492Request {
    #[serde(rename = "t1492InBlock")]
    pub inblock: T1492InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1492Request);
impl T1492Request {
    /// Build a single-page `t1492` single-price expected-change-rate request.
    pub fn new(
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        jongchk: impl Into<String>,
        volume: impl Into<String>,
    ) -> Self {
        T1492Request {
            inblock: T1492InBlock {
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                jongchk: jongchk.into(),
                volume: volume.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}
idx_summary!(T1492OutBlock, "`t1492OutBlock` — summary (next-page `idx`).");
rank_row!(T1492OutBlock1, "`t1492OutBlock1` — one ranked stock row.");

/// `t1492` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1492Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1492OutBlock", default)]
    pub outblock: T1492OutBlock,
    #[serde(
        rename = "t1492OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1492OutBlock1>,
}

/// Paginated operations, backed by the shared runtime core.
///
/// Cheap to clone — shares `Arc<Inner>` (and therefore the token cache and rate
/// limiter) with the rest of the SDK.
#[derive(Clone)]
pub struct Paginated {
    inner: Arc<Inner>,
}

impl Paginated {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        Paginated { inner }
    }

    /// Fetch a SINGLE page of the `t8412` chart.
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`], which reads the
    /// request's `tr_cont`/`tr_cont_key` via [`ls_core::HasPagination`] and sends
    /// them as HTTP headers. The returned response carries the continuation from
    /// the response headers; the caller may thread it onto a follow-up request, or
    /// use [`Paginated::chart_all`] to walk every page.
    pub async fn chart_page(&self, req: &T8412Request) -> LsResult<T8412Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T8412_POLICY, req)
            .await
    }

    /// Fetch the FULL range of the `t8412` chart, walking every page.
    ///
    /// Drives [`ls_core::Inner::collect_all`], which loops until the response
    /// `tr_cont` header is empty/`"N"` or `max_pages` is hit (returning
    /// [`ls_core::LsError::PaginationLimit`] at the cap). Each page's `tr_cont`/
    /// `tr_cont_key` are copied onto the next request. Returns the accumulated
    /// pages in order; callers concatenate `outblock1` across them.
    pub async fn chart_all(&self, req: T8412Request) -> LsResult<Vec<T8412Response>> {
        let inner = Arc::clone(&self.inner);
        self.inner
            .collect_all(req, move |r| {
                let inner = Arc::clone(&inner);
                async move {
                    inner
                        .post_paginated::<T8412Request, T8412Response>(
                            &ls_core::endpoint_policy::T8412_POLICY,
                            &r,
                        )
                        .await
                }
            })
            .await
    }

    /// Fetch a SINGLE page of the `t1452` top-volume rank screen.
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`] with empty
    /// `tr_cont`/`tr_cont_key` headers; the body `idx` cursor carries the page
    /// position. Single-page scope only — no multi-page body-`idx` collection
    /// (deferred follow-up work).
    pub async fn top_volume(&self, req: &T1452Request) -> LsResult<T1452Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1452_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1403` newly-listed stocks (date-range screen).
    pub async fn new_listings(&self, req: &T1403Request) -> LsResult<T1403Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1403_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1441` top change-rate.
    pub async fn top_change_rate(&self, req: &T1441Request) -> LsResult<T1441Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1441_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1463` top trading value.
    pub async fn top_value(&self, req: &T1463Request) -> LsResult<T1463Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1463_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1466` volume-surge screen.
    pub async fn volume_surge(&self, req: &T1466Request) -> LsResult<T1466Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1466_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1489` top expected-execution volume.
    pub async fn top_expected_volume(&self, req: &T1489Request) -> LsResult<T1489Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1489_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1492` single-price expected change rate.
    pub async fn single_price_expected(&self, req: &T1492Request) -> LsResult<T1492Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1492_POLICY, req)
            .await
    }
}
