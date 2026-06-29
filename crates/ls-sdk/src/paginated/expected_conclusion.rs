//! Expected-conclusion paginated stock read (`t1486`).
//!
//! A `[주식] 시세` market-data read at `/stock/market-data` that walks an intraday
//! expected-conclusion (예상체결) series — populated mainly during auction phases.
//! Self-paginated on the body `cts_time` cursor returned in the `t1486OutBlock`
//! summary block; the row array arrives under `t1486OutBlock1`. Single-page scope.
//!
//! The request count `cnt` is a genuinely-numeric slot (baseline `type: Object`)
//! serialized as a JSON **number** via [`ls_core::string_as_number`]; the string
//! form returns `IGW40011`. `cts_time` is an ORDINARY in-block string cursor at its
//! first-page `""` value; the header `tr_cont`/`tr_cont_key` are `#[serde(skip)]`.

use serde::{Deserialize, Serialize};

/// Input block for `t1486` — 예상체결가등락율. The body continuation cursor is
/// `cts_time` (first page `""`); `cnt` is the genuinely-numeric request count
/// serialized as a JSON number via [`ls_core::string_as_number`]. `exchgubun` is
/// the exchange-division code. Header cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1486InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Body continuation cursor / 시간CTS (first page `""`).
    pub cts_time: String,
    /// Row count / 조회건수 (genuinely numeric; serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Exchange division code / 거래소구분코드.
    pub exchgubun: String,
}

/// `t1486` request (self-paginated; `cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1486Request {
    #[serde(rename = "t1486InBlock")]
    pub inblock: T1486InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1486Request);
impl T1486Request {
    /// Build a first-page `t1486` expected-conclusion request for one stock on the
    /// given exchange (`cnt` = `20`, first-page `cts_time` empty).
    pub fn new(shcode: impl Into<String>, exchgubun: impl Into<String>) -> Self {
        T1486Request {
            inblock: T1486InBlock {
                shcode: shcode.into(),
                cts_time: String::new(),
                cnt: "20".to_string(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1486OutBlock` — the summary/cursor block (next-page `cts_time`/`ex_shcode`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1486OutBlock {
    /// Returned continuation cursor / 시간CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
    /// Exchange-qualified short code / 거래소별단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// `t1486OutBlock1` — one expected-conclusion row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1486OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Expected conclusion price / 예상체결가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Expected conclusion volume / 예상체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
}

/// `t1486` response (single page). `outblock1` tolerated single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1486Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1486OutBlock", default)]
    pub outblock: T1486OutBlock,
    #[serde(
        rename = "t1486OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1486OutBlock1>,
}
