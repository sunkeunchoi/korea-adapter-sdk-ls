//! Sector/index period-trend paginated TR (`t1514`).
//!
//! Unlike the stock rank/screen TRs in `rank_screen.rs` (numeric body-`idx`
//! cursor), `t1514` is a sector/index read whose body continuation cursor is the
//! date string `cts_date`, and whose genuinely-numeric request field is `cnt`
//! (serialized as a JSON number via `string_as_number` — the string form returns
//! `IGW40011`). Single-page scope, like its rank-screen siblings.

use serde::{Deserialize, Serialize};

/// Input block for `t1514` — 업종기간별추이 (one sector's period trend). The body
/// continuation cursor is `cts_date` (a date string, first page = `" "`); `cnt`
/// is the genuinely-numeric request count serialized as a JSON number via
/// [`ls_core::string_as_number`] — the string form returns `IGW40011` (confirmed
/// by the U1 raw-probe A/B). The header `tr_cont`/`tr_cont_key` are skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1514InBlock {
    /// Sector code / 업종코드 (e.g. "001").
    pub upcode: String,
    /// Period division / 주기구분.
    pub gubun1: String,
    /// Sub-division / 구분.
    pub gubun2: String,
    /// Body continuation cursor / 연속일자 (first page = `" "`; a date string).
    pub cts_date: String,
    /// Row count / 요청건수 (genuinely numeric; serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Rate division / 등락구분.
    pub rate_gbn: String,
}

/// `t1514` request (self-paginated; `cts_date` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1514Request {
    #[serde(rename = "t1514InBlock")]
    pub inblock: T1514InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1514Request);
impl T1514Request {
    /// Build a first-page `t1514` period-trend request for one sector with the
    /// spec defaults (`cnt` = 1 per the spec `req_example`, first-page `cts_date`).
    /// Callers wanting more rows per page set `inblock.cnt` directly (it stays a
    /// `String` that serializes as a JSON number).
    pub fn new(upcode: impl Into<String>) -> Self {
        T1514Request {
            inblock: T1514InBlock {
                upcode: upcode.into(),
                gubun1: " ".to_string(),
                gubun2: "1".to_string(),
                cts_date: " ".to_string(),
                cnt: "1".to_string(),
                rate_gbn: "1".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1514OutBlock` — the period-trend summary block (next-page `cts_date` cursor).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1514OutBlock {
    /// Returned continuation cursor / 연속일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
}

/// `t1514OutBlock1` — one period-trend row (representative subset; every field
/// via [`ls_core::string_or_number`]).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1514OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Sector code / 업종코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// Index / 지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// High index / 고가지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highjisu: String,
    /// Open index / 시가지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openjisu: String,
}

/// `t1514` response (single page). `outblock` is the summary (next-page
/// `cts_date`); `outblock1` is the period-trend array under `t1514OutBlock1`,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1514Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1514OutBlock", default)]
    pub outblock: T1514OutBlock,
    #[serde(
        rename = "t1514OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1514OutBlock1>,
}
