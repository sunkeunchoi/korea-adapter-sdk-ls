//! Overseas index time-series paginated TR (`t3518`).
//!
//! 해외실시간지수 — an overseas equity-index time-series (NASDAQ, DJI, …) served
//! through the domestic `/stock/investinfo` endpoint. Header `t3518OutBlock`
//! carries the `cts_date`/`cts_time` continuation cursor; `t3518OutBlock1` is the
//! index-tick row array (tolerated single-or-array via `de_vec_or_single`).
//! Self-paginated on the body `cts_date`/`cts_time` cursor (header
//! `tr_cont`/`tr_cont_key` skipped, mirror `t1514`/`t3401`); single-page facade
//! scope. Genuinely-numeric request fields `cnt`/`nmin` serialize as JSON numbers
//! via `string_as_number` (the string form returns `IGW40011`, KTD3). Wire keys +
//! array-ness read from the raw `res_example`.

use serde::{Deserialize, Serialize};

/// Input block for `t3518` — 해외실시간지수 (one overseas index's time-series).
/// `kind`/`symbol` select the index (e.g. `kind="S"`, `symbol="NAS@IXIC"`); `cnt`
/// and `nmin` are genuinely numeric (JSON numbers via [`ls_core::string_as_number`]);
/// `jgbn` is a mode flag; `cts_date`/`cts_time` are the body continuation cursors.
#[derive(Serialize, Debug, Clone)]
pub struct T3518InBlock {
    /// Symbol kind / 종목종류 (e.g. "S").
    pub kind: String,
    /// Index symbol / SYMBOL (e.g. "NAS@IXIC").
    pub symbol: String,
    /// Row count / 입력건수 (genuinely numeric; serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Query division / 조회구분.
    pub jgbn: String,
    /// N-minute bucket / N분 (genuinely numeric; serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub nmin: String,
    /// Body continuation cursor (date) / CTS_DATE (first page = `" "`).
    pub cts_date: String,
    /// Body continuation cursor (time) / CTS_TIME (first page = `" "`).
    pub cts_time: String,
}

/// `t3518` request (self-paginated; `cts_date`/`cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T3518Request {
    #[serde(rename = "t3518InBlock")]
    pub inblock: T3518InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T3518Request);
impl T3518Request {
    /// Build a first-page `t3518` overseas-index time-series request for one index
    /// (`kind`/`symbol`), spec defaults (`cnt=20`, `jgbn="4"`, `nmin=0`, first-page
    /// cursors). Override `inblock` fields to page or rebucket; `cnt`/`nmin` stay
    /// `String`s that serialize as JSON numbers.
    pub fn new(kind: impl Into<String>, symbol: impl Into<String>) -> Self {
        T3518Request {
            inblock: T3518InBlock {
                kind: kind.into(),
                symbol: symbol.into(),
                cnt: "20".to_string(),
                jgbn: "4".to_string(),
                nmin: "0".to_string(),
                cts_date: " ".to_string(),
                cts_time: " ".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t3518OutBlock` — the continuation-cursor header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3518OutBlock {
    /// Echoed continuation cursor (date) / CTS_DATE.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Echoed continuation cursor (time) / CTS_TIME.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
}

/// `t3518OutBlock1` — one overseas-index tick row (modeled from the raw `res_example`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3518OutBlock1 {
    /// Trade date / 일자 (YYYYMMDD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Symbol / SYMBOL.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Exchange id / 거래소.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exid: String,
    /// Symbol kind / 종목종류.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub kind: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Change sign / 대비속성.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Offer price / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// Bid remainder / 매수잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem: String,
    /// Offer remainder / 매도잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Bid price / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// Korea date / 한국일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub kodate: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Current index / 현재지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Korea time / 한국시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub kotime: String,
    /// Local time / 현지시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uprate: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `t3518` response (single page): cursor `outblock` + index-tick `outblock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3518Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3518OutBlock", default)]
    pub outblock: T3518OutBlock,
    #[serde(
        rename = "t3518OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T3518OutBlock1>,
}
