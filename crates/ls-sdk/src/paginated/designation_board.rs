//! Administrative-designation board paginated TR (`t1404` —
//! 관리/불성실/투자유의조회).
//!
//! A single-page self-paginated read like `t1514` (`sector_index.rs`): the body
//! continuation cursor is the code string `cts_shcode` (first page `" "`), and
//! every in-block field serializes as a JSON string (no `string_as_number` /
//! IGW40011 fix). The summary block `t1404OutBlock` holds only the next-page
//! `cts_shcode`; the designation rows (management / unfaithful-disclosure /
//! investment-caution — 관리/불성실/투자유의) arrive as a top-level sibling array
//! `t1404OutBlock1` (shape read from the raw `res_example`, NOT the flattened
//! normalized baseline, which collapses the row fields under `t1404OutBlock`).

use serde::{Deserialize, Serialize};

/// Input block for `t1404` — 관리/불성실/투자유의조회 (the designation board). All
/// fields are strings; the body continuation cursor is `cts_shcode` (first page
/// `" "`). The header `tr_cont`/`tr_cont_key` are skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1404InBlock {
    /// Division / 구분 (`"0"` per the spec `req_example`).
    pub gubun: String,
    /// Issue check / 종목체크 (`"1"` per the spec `req_example`).
    pub jongchk: String,
    /// Body continuation cursor / 종목코드_CTS (first page `" "`).
    pub cts_shcode: String,
}

/// `t1404` request (self-paginated; `cts_shcode` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1404Request {
    #[serde(rename = "t1404InBlock")]
    pub inblock: T1404InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1404Request);
impl T1404Request {
    /// Build a first-page `t1404` designation-board request with the spec defaults
    /// (`gubun` = `"0"`, `jongchk` = `"1"`, first-page `cts_shcode` = `" "`).
    /// Callers narrowing the board set `inblock.gubun`/`inblock.jongchk` directly
    /// (they stay strings).
    pub fn new() -> Self {
        T1404Request {
            inblock: T1404InBlock {
                gubun: "0".to_string(),
                jongchk: "1".to_string(),
                cts_shcode: " ".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

impl Default for T1404Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1404OutBlock` — the designation-board summary block (next-page `cts_shcode`
/// cursor only; the row fields are a sibling array, not nested here).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1404OutBlock {
    /// Returned continuation cursor / 종목코드_CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_shcode: String,
}

/// `t1404OutBlock1` — one designation row (representative subset; numeric-bearing
/// fields via [`ls_core::string_or_number`] to tolerate JSON string OR number).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1404OutBlock1 {
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
    /// Designation date / 지정일.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Designation reason code / 사유.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub reason: String,
    /// Release date / 해제일.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub edate: String,
}

/// `t1404` response (single page). `outblock` is the summary (next-page
/// `cts_shcode`); `outblock1` is the designation array under `t1404OutBlock1`,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1404Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1404OutBlock", default)]
    pub outblock: T1404OutBlock,
    #[serde(
        rename = "t1404OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1404OutBlock1>,
}
