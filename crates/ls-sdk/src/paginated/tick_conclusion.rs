//! Intraday tick/conclusion paginated stock reads (`t1109`, `t1301`, `t8454`).
//!
//! All three are `[주식] 시세` market-data reads at `/stock/market-data` that walk
//! an intraday conclusion (체결) series. Each is self-paginated on a body
//! continuation cursor returned in its `{tr}OutBlock` summary block; the row array
//! arrives under `{tr}OutBlock1`. Like the rank/screen TRs, only single-page scope
//! is promoted (`ls-core` threads no body-cursor collection).
//!
//! The first-page numeric request slots (`t1109.idx`, `t1301.cvolume`,
//! `t8454.cvolume`) serialize as JSON **numbers** via [`ls_core::string_as_number`]
//! — the string form returns `IGW40011`. The body continuation cursors
//! (`dan_chetime`/`cts_time`) are ORDINARY in-block string fields at their
//! first-page value; the header `tr_cont`/`tr_cont_key` are `#[serde(skip)]`.

use serde::{Deserialize, Serialize};

// ---- t1109 — 시간외체결량 (after-hours tick conclusion) -----------------------

/// Input block for `t1109` — 시간외체결량. The body continuation cursor is
/// `dan_chetime` (a 체결 cts string, first page `""`); `idx` is the genuinely-numeric
/// first-page cursor serialized as a JSON number via [`ls_core::string_as_number`]
/// (first page `0`). The header `tr_cont`/`tr_cont_key` are skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1109InBlock {
    /// Short code / 종목코드.
    pub shcode: String,
    /// Body continuation cursor / 체결cts (first page `""`).
    pub dan_chetime: String,
    /// Page index / IDX (genuinely numeric; first page `0`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1109` request (self-paginated; `dan_chetime`/`idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1109Request {
    #[serde(rename = "t1109InBlock")]
    pub inblock: T1109InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1109Request);
impl T1109Request {
    /// Build a first-page `t1109` after-hours tick request for one stock
    /// (`dan_chetime` empty, `idx` = `0`).
    pub fn new(shcode: impl Into<String>) -> Self {
        T1109Request {
            inblock: T1109InBlock {
                shcode: shcode.into(),
                dan_chetime: String::new(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1109OutBlock` — the summary/cursor block (next-page `ctsshcode`/`ctschetime`/`idx`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1109OutBlock {
    /// Returned continuation short-code cursor / 종목cts.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ctsshcode: String,
    /// Returned continuation conclusion-time cursor / 체결cts.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ctschetime: String,
    /// Returned page index / IDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1109OutBlock1` — one after-hours tick row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1109OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_chetime: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_sign: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_change: String,
    /// Conclusion volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_cvolume: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dan_volume: String,
}

/// `t1109` response (single page). `outblock` is the summary/cursor;
/// `outblock1` is the tick array, tolerated single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1109Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1109OutBlock", default)]
    pub outblock: T1109OutBlock,
    #[serde(
        rename = "t1109OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1109OutBlock1>,
}

// ---- t1301 — 시간대별체결조회 (time-band tick conclusion) ----------------------

/// Input block for `t1301`. The body continuation cursor is `cts_time` (a 시간 cts
/// string, first page `""`); `cvolume` is the genuinely-numeric special-volume
/// filter serialized as a JSON number via [`ls_core::string_as_number`] (first page
/// `0`). `starttime`/`endtime` are string window bounds. Header cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1301InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Special volume filter / 특이거래량 (genuinely numeric; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cvolume: String,
    /// Start time / 시작시간.
    pub starttime: String,
    /// End time / 종료시간.
    pub endtime: String,
    /// Body continuation cursor / 시간CTS (first page `""`).
    pub cts_time: String,
}

/// `t1301` request (self-paginated; `cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1301Request {
    #[serde(rename = "t1301InBlock")]
    pub inblock: T1301InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1301Request);
impl T1301Request {
    /// Build a first-page `t1301` time-band tick request for one stock over a
    /// `starttime`/`endtime` window (`cvolume` = `0`, first-page `cts_time` empty).
    pub fn new(
        shcode: impl Into<String>,
        starttime: impl Into<String>,
        endtime: impl Into<String>,
    ) -> Self {
        T1301Request {
            inblock: T1301InBlock {
                shcode: shcode.into(),
                cvolume: "0".to_string(),
                starttime: starttime.into(),
                endtime: endtime.into(),
                cts_time: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1301OutBlock` — the summary/cursor block (next-page `cts_time`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1301OutBlock {
    /// Returned continuation cursor / 시간CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
}

/// `t1301OutBlock1` — one time-band tick row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1301OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Conclusion quantity / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1301` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1301Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1301OutBlock", default)]
    pub outblock: T1301OutBlock,
    #[serde(
        rename = "t1301OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1301OutBlock1>,
}

// ---- t8454 — 시간대별체결조회 (exchange-qualified time-band tick conclusion) -----

/// Input block for `t8454`. Like `t1301` plus an `exchgubun` exchange-division
/// code. The body continuation cursor is `cts_time` (first page `""`); `cvolume`
/// is the genuinely-numeric special-volume filter serialized as a JSON number
/// (first page `0`). Header cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T8454InBlock {
    /// Short code / 단축코드.
    pub shcode: String,
    /// Special volume filter / 특이거래량 (genuinely numeric; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cvolume: String,
    /// Start time / 시작시간.
    pub starttime: String,
    /// End time / 종료시간.
    pub endtime: String,
    /// Body continuation cursor / 시간CTS (first page `""`).
    pub cts_time: String,
    /// Exchange division code / 거래소구분코드.
    pub exchgubun: String,
}

/// `t8454` request (self-paginated; `cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T8454Request {
    #[serde(rename = "t8454InBlock")]
    pub inblock: T8454InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T8454Request);
impl T8454Request {
    /// Build a first-page `t8454` time-band tick request for one stock over a
    /// `starttime`/`endtime` window on the given exchange (`cvolume` = `0`,
    /// first-page `cts_time` empty).
    pub fn new(
        shcode: impl Into<String>,
        starttime: impl Into<String>,
        endtime: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T8454Request {
            inblock: T8454InBlock {
                shcode: shcode.into(),
                cvolume: "0".to_string(),
                starttime: starttime.into(),
                endtime: endtime.into(),
                cts_time: String::new(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t8454OutBlock` — the summary/cursor block (next-page `cts_time`/`ex_shcode`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8454OutBlock {
    /// Returned continuation cursor / 시간CTS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
    /// Exchange-qualified short code / 거래소별단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// `t8454OutBlock1` — one time-band tick row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8454OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Conclusion quantity / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t8454` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8454Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8454OutBlock", default)]
    pub outblock: T8454OutBlock,
    #[serde(
        rename = "t8454OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8454OutBlock1>,
}
