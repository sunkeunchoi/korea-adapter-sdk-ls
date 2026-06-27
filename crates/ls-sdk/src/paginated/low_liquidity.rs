//! Ultra-low-liquidity board paginated TR (`t1410` — 초저유동성조회).
//!
//! A single-page self-paginated read like `t1404` (`designation_board.rs`): the
//! body continuation cursor is the code string `cts_shcode` (first page `""`), and
//! every in-block field serializes as a JSON string (no `string_as_number` /
//! IGW40011 fix). The summary block `t1410OutBlock` holds only the next-page
//! `cts_shcode`; the low-liquidity rows arrive as a top-level sibling array
//! `t1410OutBlock1` (shape read from the raw `res_example`, NOT the flattened
//! normalized baseline, which collapses the row fields under `t1410OutBlock`).

use serde::{Deserialize, Serialize};

/// Input block for `t1410` — 초저유동성조회 (the ultra-low-liquidity board). Both
/// fields are strings; the body continuation cursor is `cts_shcode` (an ordinary
/// in-block field at its first-page `""` convention, NOT skipped). The header
/// `tr_cont`/`tr_cont_key` are skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1410InBlock {
    /// Division / 구분 (`"0"` per the spec `req_example`).
    pub gubun: String,
    /// Body continuation cursor / 종목코드_CTS (first page `""`).
    pub cts_shcode: String,
}

/// `t1410` request (self-paginated; `cts_shcode` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1410Request {
    #[serde(rename = "t1410InBlock")]
    pub inblock: T1410InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1410Request);
impl T1410Request {
    /// Build a first-page `t1410` low-liquidity board request with the spec default
    /// (`gubun` = `"0"`, first-page `cts_shcode` = `""`). Callers narrowing the
    /// board set `inblock.gubun` directly (it stays a string).
    pub fn new() -> Self {
        T1410Request {
            inblock: T1410InBlock {
                gubun: "0".to_string(),
                cts_shcode: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

impl Default for T1410Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1410OutBlock` — the low-liquidity board summary block (next-page `cts_shcode`
/// cursor only; the row fields are a sibling array, not nested here).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1410OutBlock {
    /// Returned continuation cursor / 종목코드_CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_shcode: String,
}

/// `t1410OutBlock1` — one low-liquidity row (representative subset of the wire
/// fields; numeric-bearing fields via [`ls_core::string_or_number`] to tolerate
/// JSON string OR number).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1410OutBlock1 {
    /// Korean name / 한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1410` response (single page). `outblock` is the summary (next-page
/// `cts_shcode`); `outblock1` is the low-liquidity array under `t1410OutBlock1`,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1410Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1410OutBlock", default)]
    pub outblock: T1410OutBlock,
    #[serde(
        rename = "t1410OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1410OutBlock1>,
}
