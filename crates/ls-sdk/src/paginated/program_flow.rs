//! Program-trade flow paginated stock read (`t1637`).
//!
//! A `[주식] 프로그램` read at `/stock/program` that walks a per-stock
//! program-trade flow (프로그램매매) series. Self-paginated on the body `cts_idx`
//! cursor returned in the `t1637OutBlock` summary block; the row array arrives
//! under `t1637OutBlock1`. Single-page scope.
//!
//! `cts_idx` is the genuinely-numeric first-page cursor (first page `0`) serialized
//! as a JSON **number** via [`ls_core::string_as_number`] — the string form returns
//! `IGW40011`. The remaining request fields are strings. The header
//! `tr_cont`/`tr_cont_key` are `#[serde(skip)]`.

use serde::{Deserialize, Serialize};

/// Input block for `t1637` — 프로그램매매추이(종목별). `cts_idx` is the
/// genuinely-numeric body continuation cursor serialized as a JSON number via
/// [`ls_core::string_as_number`] (first page `0`); `gubun1` (수량/금액), `gubun2`
/// (시간/일자), `date`, `time`, `exchgubun` are strings. Header cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1637InBlock {
    /// Quantity/amount division / 수량금액구분 (0:수량 1:금액).
    pub gubun1: String,
    /// Time/daily division / 시간일별구분 (0:시간 1:일자).
    pub gubun2: String,
    /// Short code / 종목코드.
    pub shcode: String,
    /// Date / 일자 (YYYYMMDD).
    pub date: String,
    /// Time / 시간.
    pub time: String,
    /// Body continuation cursor / IDXCTS (genuinely numeric; first page `0`; number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_idx: String,
    /// Exchange division code / 거래소구분코드.
    pub exchgubun: String,
}

/// `t1637` request (self-paginated; `cts_idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1637Request {
    #[serde(rename = "t1637InBlock")]
    pub inblock: T1637InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1637Request);
impl T1637Request {
    /// Build a first-page `t1637` per-stock program-flow request (`gubun1`/`gubun2`
    /// callers pass; `time` empty, first-page `cts_idx` = `0`).
    pub fn new(
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        shcode: impl Into<String>,
        date: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1637Request {
            inblock: T1637InBlock {
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                shcode: shcode.into(),
                date: date.into(),
                time: String::new(),
                cts_idx: "0".to_string(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1637OutBlock` — the summary/cursor block (next-page `cts_idx`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1637OutBlock {
    /// Returned continuation cursor / IDXCTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_idx: String,
}

/// `t1637OutBlock1` — one program-flow row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1637OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Net-buy quantity / 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// `t1637` response (single page). `outblock1` tolerated single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1637Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1637OutBlock", default)]
    pub outblock: T1637OutBlock,
    #[serde(
        rename = "t1637OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1637OutBlock1>,
}
