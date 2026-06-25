//! Investment-opinion paginated TR (`t3401`).
//!
//! 투자의견 — the per-broker investment-opinion history for one ticker. Header
//! `t3401OutBlock` carries the current price/cursor; `t3401OutBlock1` is the
//! opinion-row array (tolerated single-or-array via `de_vec_or_single`).
//! Self-paginated on the body `cts_date` cursor (header `tr_cont`/`tr_cont_key`
//! skipped, mirror `t1514`); single-page facade scope. No numeric request fields.
//! Wire keys + array-ness read from the raw `res_example` (KTD3).

use serde::{Deserialize, Serialize};

/// Input block for `t3401` — 투자의견 (per-broker opinion history for a ticker).
/// `gubun1`/`tradno` are optional filters; `cts_date` is the body cursor.
#[derive(Serialize, Debug, Clone)]
pub struct T3401InBlock {
    /// Short code / 종목코드.
    pub shcode: String,
    /// Division filter / 구분 (empty = all).
    pub gubun1: String,
    /// Broker-firm filter / 회원사코드 (empty = all).
    pub tradno: String,
    /// Body continuation cursor / 연속일자 (first page empty).
    pub cts_date: String,
}

/// `t3401` request (self-paginated; `cts_date` body cursor, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T3401Request {
    #[serde(rename = "t3401InBlock")]
    pub inblock: T3401InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T3401Request);
impl T3401Request {
    /// Build a first-page `t3401` opinion-history request for one ticker (all
    /// brokers, all divisions). Override `inblock` fields to filter.
    pub fn new(shcode: impl Into<String>) -> Self {
        T3401Request {
            inblock: T3401InBlock {
                shcode: shcode.into(),
                gubun1: String::new(),
                tradno: String::new(),
                cts_date: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t3401OutBlock` — the current-price summary header (carries the `cts_date` cursor).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3401OutBlock {
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Echoed continuation date / IDXDATE.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Change sign / 대비속성.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Traded value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t3401OutBlock1` — one investment-opinion row (a broker's opinion on a date).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3401OutBlock1 {
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Broker code / 회원사코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradno: String,
    /// Opinion date / 의견일자 (YYYYMMDD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Broker name / 회원사명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradname: String,
    /// New opinion / 투자의견변경후 (the canonical opinion, e.g. BUY/HOLD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bopn: String,
    /// Prior opinion / 투자의견변경전.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nopn: String,
    /// Prior target price / 목표가변경전.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub boga: String,
    /// New target price / 목표가변경후.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub noga: String,
    /// Close on opinion date / 의견일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
}

/// `t3401` response (single page): summary `outblock` + opinion-row `outblock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3401Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3401OutBlock", default)]
    pub outblock: T3401OutBlock,
    #[serde(
        rename = "t3401OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T3401OutBlock1>,
}
