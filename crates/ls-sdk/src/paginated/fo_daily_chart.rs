//! F/O daily-chart paginated TR (`t2214`).
//!
//! `t2214` (선물옵션 기간별주가) is a self-paginated F/O daily OHLCV read whose body
//! continuation cursor is `cts_code` (a code string); `cnt` is the genuinely-numeric
//! request count serialized as a JSON number via `string_as_number` (the string
//! form returns `IGW40011`). Single-page scope.

use serde::{Deserialize, Serialize};

/// Input block for `t2214` — F/O daily OHLCV. `shcode` is the contract, `futcheck`
/// selects the front-month (`"1"`), `date` the anchor day; the body continuation
/// cursor is `cts_code` (first page = `""`). `cnt` serializes as a JSON number via
/// [`ls_core::string_as_number`].
#[derive(Serialize, Debug, Clone)]
pub struct T2214InBlock {
    /// Contract short code / 단축코드.
    pub shcode: String,
    /// Front-month flag / 선물최근월물 (`"1"` = nearest).
    pub futcheck: String,
    /// Anchor date / 날짜 (YYYYMMDD).
    pub date: String,
    /// Continuation code cursor / CTS종목코드 (first page = `""`).
    pub cts_code: String,
    /// Expiry of the prior contract / 전종목만기일.
    pub lastdate: String,
    /// Requested row count / 조회요청건수 (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
}

/// `t2214` request (self-paginated; `cts_code` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T2214Request {
    #[serde(rename = "t2214InBlock")]
    pub inblock: T2214InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T2214Request);
impl T2214Request {
    /// Build a first-page `t2214` daily-OHLCV request for one contract on `date`
    /// (front-month `futcheck="1"`, first-page empty `cts_code`/`lastdate`,
    /// `cnt="20"`).
    pub fn new(shcode: impl Into<String>, date: impl Into<String>) -> Self {
        T2214Request {
            inblock: T2214InBlock {
                shcode: shcode.into(),
                futcheck: "1".to_string(),
                date: date.into(),
                cts_code: String::new(),
                lastdate: String::new(),
                cnt: "20".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t2214OutBlock` — the daily-chart summary block (next-page `cts_code` cursor).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2214OutBlock {
    /// Anchor date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Returned continuation cursor / CTS종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_code: String,
    /// Nearest-month flag / 최근월선물여부.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nowfutyn: String,
}

/// `t2214OutBlock1` — one daily OHLCV row (representative subset; every field via
/// [`ls_core::string_or_number`]). `close`/`volume` are the substantive witnesses.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2214OutBlock1 {
    /// Date / 날짜.
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
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openyak: String,
    /// Trade value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t2214` response (single page). `outblock` is the summary (next-page `cts_code`);
/// `outblock1` is the daily OHLCV array under `t2214OutBlock1`, tolerated as
/// single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2214Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2214OutBlock", default)]
    pub outblock: T2214OutBlock,
    #[serde(
        rename = "t2214OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2214OutBlock1>,
}
