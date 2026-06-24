//! Single-page body-cursor paginated TRs (the implement-tr "second freeze"
//! sub-pattern).
//!
//! These stock rank/screen TRs carry a request-BODY continuation cursor — most
//! a numeric `idx`, plus `t1866` (saved-condition list) whose cursor is the
//! string pair `cont`/`cont_key` — for which `ls-core` has NO multi-page
//! machinery (it only threads the header `tr_cont`/`tr_cont_key` cursor that
//! `t8412` uses). They are therefore promoted at SINGLE-PAGE scope:
//!   - `idx` is an ordinary serialized in-block field (a JSON number on the wire,
//!     via `string_as_number`) at its first-page convention (`"0"`) — NOT
//!     `#[serde(skip)]` (that attribute is only for `t8412`'s header cursors);
//!   - dispatch is ONE `post_paginated` call with EMPTY `tr_cont`/`tr_cont_key`
//!     headers (the request still impls `HasPagination` because `post_paginated`
//!     requires it, but the cursors stay empty);
//!   - out-rows tolerate single-or-array via `de_vec_or_single`.
//!
//! Multi-page collection over body-`idx` (a `chart_all`-equivalent) is deferred
//! follow-up work — it needs a new `ls-core` body-continuation contract.

use serde::{Deserialize, Serialize};

// --- Shared single-page paginated shapes ----------------------------------
// The rank/screen TRs all expose the same representative row subset and (for
// most) the same `idx`-only summary block; only their in-block filters differ.
// Each TR defines its own row/summary types (kept distinct for per-TR doc
// clarity and future field expansion) via these macros so the uniform field
// set stays in one place.

/// One ranked stock row (representative subset; every field via
/// [`ls_core::string_or_number`]). Reused across the rank screens.
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

// --- t1452 — 거래량상위 (top trading volume; single-page) --------------------

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
idx_summary!(
    T1452OutBlock,
    "`t1452OutBlock` — the rank-screen summary block (carries the next-page `idx`)."
);
rank_row!(T1452OutBlock1, "`t1452OutBlock1` — one ranked stock row.");

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
    /// Division / 구분.
    pub gubun1: String,
    /// Up/down / 상승하락.
    pub gubun2: String,
    /// Today/prior-day / 당일전일.
    pub gubun3: String,
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
    /// Secondary exclusion flags / 대상제외2.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num2: String,
    /// Exchange division / 거래소구분코드.
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
    /// Division / 구분.
    pub gubun: String,
    /// Prior-day division / 전일구분.
    pub jnilgubun: String,
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
    /// Secondary exclusion flags / 대상제외2.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num2: String,
    /// Exchange division / 거래소구분코드.
    pub exchgubun: String,
    /// Body continuation cursor / IDX (first page = `"0"`).
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
    /// Division / 구분.
    pub gubun: String,
    /// Prior-day volume basis / 전일거래량.
    pub type1: String,
    /// Surge-rate basis / 거래급등율.
    pub type2: String,
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
    /// Secondary exclusion flags / 대상제외2.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num2: String,
    /// Exchange division / 거래소구분코드.
    pub exchgubun: String,
    /// Body continuation cursor / IDX (first page = `"0"`).
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
    /// Exchange division / 거래소구분.
    pub gubun: String,
    /// Session division / 장구분.
    pub jgubun: String,
    /// Issue-type check / 종목체크.
    pub jongchk: String,
    /// Start expected price / 예상체결시작가격.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub yesprice: String,
    /// End expected price / 예상체결종료가격.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub yeeprice: String,
    /// Min expected volume / 예상체결량.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub yevolume: String,
    /// Body continuation cursor / IDX (first page = `"0"`).
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
    /// Division / 구분.
    pub gubun1: String,
    /// Up/down / 상승하락.
    pub gubun2: String,
    /// Issue-type check / 종목체크.
    pub jongchk: String,
    /// Volume flag / 거래량 (a length-1 flag here; serialized as a string).
    pub volume: String,
    /// Body continuation cursor / IDX (first page = `"0"`).
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

// --- t1866 — 서버저장조건 리스트조회 (server-saved condition list) ------------
// The saved-condition spine PRODUCER: each returned row carries a `query_index`
// that keys a `t1859`/`t1860` condition search. Body-cursor single-page like the
// rank screens, but its cursor is the string pair `cont`/`cont_key` (not a
// numeric `idx`), and it takes caller inputs (`user_id`/`gb`/`group_name`).

/// Input block for `t1866` — server-saved condition list.
///
/// `user_id` is the LS login id; `gb`/`group_name` select the condition set
/// (`gb = "0"`, empty `group_name` = all groups). `cont`/`cont_key` are the
/// request-BODY continuation cursor (string-typed, empty on the first page).
#[derive(Serialize, Debug, Clone)]
pub struct T1866InBlock {
    /// LS login id / 사용자 ID.
    pub user_id: String,
    /// Division / 구분.
    pub gb: String,
    /// Condition group name / 그룹명 (empty = all groups).
    pub group_name: String,
    /// Body continuation cursor / 연속 (first page = empty).
    pub cont: String,
    /// Body continuation key / 연속키 (first page = empty).
    pub cont_key: String,
}

/// `t1866` request — wraps the in-block under the `t1866InBlock` key.
///
/// `cont`/`cont_key` ride IN the body as the continuation cursor. The
/// `tr_cont`/`tr_cont_key` fields are `#[serde(skip)]` and stay empty for the
/// single-page call; they exist only to satisfy the `HasPagination` bound on
/// `post_paginated`.
#[derive(Serialize, Debug, Clone)]
pub struct T1866Request {
    #[serde(rename = "t1866InBlock")]
    pub inblock: T1866InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1866Request);
impl T1866Request {
    /// Build a single-page `t1866` saved-condition-list request for `user_id`,
    /// listing all groups (`gb = "0"`, empty `group_name`). Cursors start empty.
    pub fn new(user_id: impl Into<String>) -> Self {
        T1866Request {
            inblock: T1866InBlock {
                user_id: user_id.into(),
                gb: "0".to_string(),
                group_name: String::new(),
                cont: String::new(),
                cont_key: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1866OutBlock` — summary block (result count + returned continuation cursor).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1866OutBlock {
    /// Result count / 건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub result_count: String,
    /// Returned continuation cursor / 연속.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cont: String,
    /// Returned continuation key / 연속키.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cont_key: String,
}

/// `t1866OutBlock1` — one server-saved condition. `query_index` keys the
/// `t1859`/`t1860` condition search (the modeled cross-TR discovery edge).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1866OutBlock1 {
    /// Saved-condition index / 질의 인덱스 — keys the t1859/t1860 search.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub query_index: String,
    /// Condition group name / 그룹명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub group_name: String,
    /// Condition name / 질의명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub query_name: String,
}

/// `t1866` response (single page). `outblock1` is the saved-condition array.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1866Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1866OutBlock", default)]
    pub outblock: T1866OutBlock,
    #[serde(
        rename = "t1866OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1866OutBlock1>,
}

// --- t3341 — 재무순위종합 (financial ranking; single-page body-idx) ------------

/// Input block for `t3341` — 재무순위종합 (financial ranking). `idx` is the body
/// continuation cursor serialized as a JSON number (first page = `"0"`); the
/// header `tr_cont`/`tr_cont_key` are skipped (single-page scope, KTD-5).
#[derive(Serialize, Debug, Clone)]
pub struct T3341InBlock {
    /// Market / 시장구분 (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ).
    pub gubun: String,
    /// Rank metric / 순위구분 (`"1"` sales-growth … per spec).
    pub gubun1: String,
    /// Comparison division / 대비구분 (`"1"` fixed per spec).
    pub gubun2: String,
    /// Body continuation cursor / IDX (first page = `"0"`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t3341` request (single-page; `idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T3341Request {
    #[serde(rename = "t3341InBlock")]
    pub inblock: T3341InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T3341Request);
impl T3341Request {
    /// Build a single-page `t3341` financial-ranking request with documented
    /// defaults (all markets, sales-growth rank, fixed comparison). `idx` defaults
    /// to the first-page convention (`"0"`, serialized as a number).
    pub fn new() -> Self {
        T3341Request {
            inblock: T3341InBlock {
                gubun: "0".to_string(),
                gubun1: "1".to_string(),
                gubun2: "1".to_string(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}
impl Default for T3341Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t3341OutBlock` — the financial-ranking summary block (count + next-page `idx`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3341OutBlock {
    /// Row count / CNT.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
    /// Returned continuation cursor / IDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t3341OutBlock1` — one financial-ranking row (representative subset; every
/// field via [`ls_core::string_or_number`]).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3341OutBlock1 {
    /// Rank / 순위.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rank: String,
    /// Company name / 기업명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Sales growth rate / 매출액증가율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub salesgrowth: String,
    /// EPS / EPS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub eps: String,
    /// ROE / ROE.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub roe: String,
    /// PER / PER.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub per: String,
}

/// `t3341` response (single page). `outblock` is the summary (count + next-page
/// `idx`); `outblock1` is the ranked-row array under `t3341OutBlock1`, tolerated
/// as single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3341Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3341OutBlock", default)]
    pub outblock: T3341OutBlock,
    #[serde(
        rename = "t3341OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T3341OutBlock1>,
}

// --- t1481 — 시간외등락율상위 (after-hours top change rate; single-page) -------
// Single-page body-`idx` sub-pattern: `idx` is an ordinary in-block field
// serialized as a JSON number (first page = `"0"`), NOT a `#[serde(skip)]` header
// cursor. The summary `t1481OutBlock` carries only the next-page `idx`; the row
// array rides under `t1481OutBlock1` (out-block shape read from the raw capture).

/// Input block for `t1481` — 시간외등락율상위 (after-hours top change rate).
///
/// A rank-screen filter. `gubun1`/`gubun2`/`jongchk` are division flags and
/// `volume` a length-1 min-volume flag (all length-1 strings); `idx` is the body
/// continuation cursor serialized as a JSON number (first page = `"0"`).
#[derive(Serialize, Debug, Clone)]
pub struct T1481InBlock {
    /// Division / 구분.
    pub gubun1: String,
    /// Up/down / 상승하락.
    pub gubun2: String,
    /// Issue-type check / 종목체크.
    pub jongchk: String,
    /// Min-volume flag / 거래량 (a length-1 flag here; serialized as a string).
    pub volume: String,
    /// Body continuation cursor / IDX (first page = `"0"`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1481` request (single-page; `idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1481Request {
    #[serde(rename = "t1481InBlock")]
    pub inblock: T1481InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1481Request);
impl T1481Request {
    /// Build a single-page `t1481` after-hours top-change-rate request. `idx`
    /// defaults to the first-page convention (`"0"`); header cursors start empty.
    pub fn new(
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        jongchk: impl Into<String>,
        volume: impl Into<String>,
    ) -> Self {
        T1481Request {
            inblock: T1481InBlock {
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
idx_summary!(T1481OutBlock, "`t1481OutBlock` — summary (next-page `idx`).");
rank_row!(
    T1481OutBlock1,
    "`t1481OutBlock1` — one after-hours ranked stock row."
);

/// `t1481` response (single page). `outblock` is the summary (next-page `idx`);
/// `outblock1` is the ranked-row array under `t1481OutBlock1`, tolerated as
/// single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1481Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1481OutBlock", default)]
    pub outblock: T1481OutBlock,
    #[serde(
        rename = "t1481OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1481OutBlock1>,
}

// --- t1482 — 시간외거래량상위 (after-hours top volume; single-page) ------------
// Same single-page body-`idx` sub-pattern as t1481. The in-block carries a
// numeric `sort_gbn` sort flag (serialized as a number) alongside the `idx`
// cursor; the summary `t1482OutBlock` carries only the next-page `idx`, and the
// row array rides under `t1482OutBlock1` (out-block shape read from the raw
// capture).

/// Input block for `t1482` — 시간외거래량상위 (after-hours top volume).
///
/// `sort_gbn` is a numeric sort flag (serialized as a JSON number); `gubun` and
/// `jongchk` are length-1 string flags; `idx` is the body continuation cursor
/// serialized as a JSON number (first page = `"0"`).
#[derive(Serialize, Debug, Clone)]
pub struct T1482InBlock {
    /// Sort division / 정렬구분 (numeric flag; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sort_gbn: String,
    /// Division / 구분.
    pub gubun: String,
    /// Issue-type check / 종목체크 (a length-1 flag here; serialized as a string).
    pub jongchk: String,
    /// Body continuation cursor / IDX (first page = `"0"`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1482` request (single-page; `idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1482Request {
    #[serde(rename = "t1482InBlock")]
    pub inblock: T1482InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1482Request);
impl T1482Request {
    /// Build a single-page `t1482` after-hours top-volume request. `idx` defaults
    /// to the first-page convention (`"0"`); header cursors start empty.
    pub fn new(
        sort_gbn: impl Into<String>,
        gubun: impl Into<String>,
        jongchk: impl Into<String>,
    ) -> Self {
        T1482Request {
            inblock: T1482InBlock {
                sort_gbn: sort_gbn.into(),
                gubun: gubun.into(),
                jongchk: jongchk.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}
idx_summary!(T1482OutBlock, "`t1482OutBlock` — summary (next-page `idx`).");
rank_row!(
    T1482OutBlock1,
    "`t1482OutBlock1` — one after-hours top-volume stock row."
);

/// `t1482` response (single page). `outblock` is the summary (next-page `idx`);
/// `outblock1` is the ranked-row array under `t1482OutBlock1`, tolerated as
/// single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1482Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1482OutBlock", default)]
    pub outblock: T1482OutBlock,
    #[serde(
        rename = "t1482OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1482OutBlock1>,
}
