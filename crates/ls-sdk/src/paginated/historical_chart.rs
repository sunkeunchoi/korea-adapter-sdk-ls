//! Historical tick/min chart paginated TR (`t1310` — 주식당일전일분틱조회).
//!
//! A single-page self-paginated read like `t1514` (`sector_index.rs`): the body
//! continuation cursor is the time string `cts_time` (first page `""`), and the
//! request carries no genuinely-numeric field — every in-block field serializes as
//! a JSON string (no `string_as_number` / IGW40011 fix). The summary block
//! `t1310OutBlock` holds the next-page `cts_time`; the tick/min rows arrive as a
//! top-level sibling array `t1310OutBlock1` (shape read from the raw `res_example`,
//! not the flattened normalized baseline).

use serde::{Deserialize, Serialize};

/// Input block for `t1310` — 주식당일전일분틱조회 (one symbol's today/prev tick or
/// minute bars). All fields are strings; the body continuation cursor is
/// `cts_time` (first page `""`). The header `tr_cont`/`tr_cont_key` are skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1310InBlock {
    /// Today/previous division / 당일전일구분 (`"0"` per the spec `req_example`).
    pub daygb: String,
    /// Minute/tick division / 분틱구분 (`"0"` per the spec `req_example`).
    pub timegb: String,
    /// Short code / 단축코드 (caller-supplied symbol).
    pub shcode: String,
    /// End time / 종료시간 (first page `""`).
    pub endtime: String,
    /// Body continuation cursor / 시간CTS (first page `""`; a time string).
    pub cts_time: String,
    /// Exchange division / 거래소구분코드 (`"K"` = KRX).
    pub exchgubun: String,
}

/// `t1310` request (self-paginated; `cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1310Request {
    #[serde(rename = "t1310InBlock")]
    pub inblock: T1310InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1310Request);
impl T1310Request {
    /// Build a first-page `t1310` tick/min-chart request for one symbol with the
    /// spec defaults (`daygb`/`timegb` = `"0"`, empty `endtime`/`cts_time`,
    /// `exchgubun` = `"K"`). Callers wanting previous-day or minute bars set
    /// `inblock.daygb`/`inblock.timegb` directly (they stay strings).
    pub fn new(shcode: impl Into<String>) -> Self {
        T1310Request {
            inblock: T1310InBlock {
                daygb: "0".to_string(),
                timegb: "0".to_string(),
                shcode: shcode.into(),
                endtime: String::new(),
                cts_time: String::new(),
                exchgubun: "K".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1310OutBlock` — the chart summary block (next-page `cts_time` cursor).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1310OutBlock {
    /// Returned continuation cursor / 시간CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
}

/// `t1310OutBlock1` — one tick/min bar (representative subset; every field via
/// [`ls_core::string_or_number`] to tolerate JSON string OR number wire types).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1310OutBlock1 {
    /// Time / 시간.
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
    /// Change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Trade quantity / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Trade strength / 체결강도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chdegree: String,
    /// Cumulative volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Exchange name / 거래소명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exchname: String,
}

/// `t1310` response (single page). `outblock` is the summary (next-page
/// `cts_time`); `outblock1` is the tick/min-bar array under `t1310OutBlock1`,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1310Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1310OutBlock", default)]
    pub outblock: T1310OutBlock,
    #[serde(
        rename = "t1310OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1310OutBlock1>,
}
