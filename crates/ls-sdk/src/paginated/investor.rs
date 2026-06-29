//! Investor-flow paginated stock reads (`t1602`, `t1603`, `t1617`).
//!
//! All three are `[주식] 투자자` market-data reads at `/stock/investor` that walk a
//! per-investor-class trade-flow (투자자매매) series. Each is self-paginated on a
//! body continuation cursor returned in its `{tr}OutBlock` summary block; the row
//! array arrives under `{tr}OutBlock1`. Only single-page scope is promoted
//! (`ls-core` threads no body-cursor collection).
//!
//! The numeric first-page request slots (`t1602.cts_idx`/`t1602.cnt`,
//! `t1603.cts_idx`/`t1603.cnt`) serialize as JSON **numbers** via
//! [`ls_core::string_as_number`] — the string form returns `IGW40011`. `t1617` has
//! no numeric request slot (all-String). The body continuation cursors
//! (`cts_time`/`cts_date`) are ORDINARY in-block string fields at their first-page
//! value; the header `tr_cont`/`tr_cont_key` are `#[serde(skip)]`.

use serde::{Deserialize, Serialize};

// ---- t1602 — 시간대별투자자매매추이 (time-band investor flow by sector) ----------

/// Input block for `t1602`. The body continuation cursor is `cts_time` (first page
/// `""`); `cts_idx` and `cnt` are the genuinely-numeric first-page cursor/count
/// serialized as JSON numbers via [`ls_core::string_as_number`] (first page `0`/`20`).
/// `market`/`upcode`/`gubun1`/`gubun2`/`gubun3`/`exchgubun` are strings. Header
/// cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1602InBlock {
    /// Market division / 시장구분.
    pub market: String,
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Quantity division / 수량구분.
    pub gubun1: String,
    /// Prior-period division / 전일분구분.
    pub gubun2: String,
    /// Body continuation cursor / CTSTIME (first page `""`).
    pub cts_time: String,
    /// Page index / CTSIDX (genuinely numeric; first page `0`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_idx: String,
    /// Row count / 조회건수 (genuinely numeric; first page `20`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Just-prior-vs division / 직전대비구분 (C:직전대비).
    pub gubun3: String,
    /// Exchange division code / 거래소구분코드.
    pub exchgubun: String,
}

/// `t1602` request (self-paginated; `cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1602Request {
    #[serde(rename = "t1602InBlock")]
    pub inblock: T1602InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1602Request);
impl T1602Request {
    /// Build a first-page `t1602` time-band investor-flow request for one sector
    /// (`cts_time` empty, `cts_idx` = `0`, `cnt` = `20`).
    pub fn new(
        market: impl Into<String>,
        upcode: impl Into<String>,
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1602Request {
            inblock: T1602InBlock {
                market: market.into(),
                upcode: upcode.into(),
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                cts_time: String::new(),
                cts_idx: "0".to_string(),
                cnt: "20".to_string(),
                gubun3: String::new(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1602OutBlock` — the current-flow summary/cursor block (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1602OutBlock {
    /// Returned continuation cursor / CTSTIME.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
    /// Individual buy / 개인매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ms_08: String,
    /// Foreign buy / 외국인매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ms_17: String,
    /// Foreign net-buy / 외국인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_17: String,
    /// Institution buy / 기관계매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ms_18: String,
}

/// `t1602OutBlock1` — one time-band investor-flow row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1602OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Individual net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Foreign net-buy / 외국인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Institution net-buy / 기관계순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
}

/// `t1602` response (single page). `outblock1` tolerated single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1602Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1602OutBlock", default)]
    pub outblock: T1602OutBlock,
    #[serde(
        rename = "t1602OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1602OutBlock1>,
}

// ---- t1603 — 투자자별매매종목 (investor detail by issue) -----------------------

/// Input block for `t1603`. The body continuation cursor is `cts_time` (first page
/// `""`); `cts_idx` and `cnt` are the genuinely-numeric first-page cursor/count
/// serialized as JSON numbers via [`ls_core::string_as_number`] (first page
/// `0`/`20`). `market`/`gubun1`/`gubun2`/`upcode`/`exchgubun` are strings. Header
/// cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1603InBlock {
    /// Market division / 시장구분.
    pub market: String,
    /// Investor division / 투자자구분.
    pub gubun1: String,
    /// Prior-period division / 전일분구분.
    pub gubun2: String,
    /// Body continuation cursor / CTSTIME (first page `""`).
    pub cts_time: String,
    /// Page index / CTSIDX (genuinely numeric; first page `0`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_idx: String,
    /// Row count / 조회건수 (genuinely numeric; first page `20`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Exchange division code / 거래소구분코드.
    pub exchgubun: String,
}

/// `t1603` request (self-paginated; `cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1603Request {
    #[serde(rename = "t1603InBlock")]
    pub inblock: T1603InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1603Request);
impl T1603Request {
    /// Build a first-page `t1603` investor-detail request (`cts_time` empty,
    /// `cts_idx` = `0`, `cnt` = `20`).
    pub fn new(
        market: impl Into<String>,
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        upcode: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1603Request {
            inblock: T1603InBlock {
                market: market.into(),
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                cts_time: String::new(),
                cts_idx: "0".to_string(),
                cnt: "20".to_string(),
                upcode: upcode.into(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1603OutBlock` — the summary/cursor block (next-page `cts_idx`/`cts_time`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1603OutBlock {
    /// Returned page index / CTSIDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_idx: String,
    /// Returned continuation cursor / CTSTIME.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
}

/// `t1603OutBlock1` — one investor-detail row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1603OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Investor division / 투자자구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjcode: String,
    /// Buy quantity / 매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// Sell quantity / 매도수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// Net-buy quantity / 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume: String,
}

/// `t1603` response (single page). `outblock1` tolerated single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1603Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1603OutBlock", default)]
    pub outblock: T1603OutBlock,
    #[serde(
        rename = "t1603OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1603OutBlock1>,
}

// ---- t1617 — 투자자별일별매매추이 (investor time/daily flow) --------------------

/// Input block for `t1617`. All-String (no numeric request slot). The body
/// continuation cursors are `cts_date`/`cts_time` (first page `""`); the header
/// `tr_cont`/`tr_cont_key` are skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1617InBlock {
    /// Market division / 시장구분.
    pub gubun1: String,
    /// Quantity/amount division / 수량금액구분 (1:수량 2:금액).
    pub gubun2: String,
    /// Time/daily division / 일자구분 (1:시간대별 2:일별).
    pub gubun3: String,
    /// Body continuation date cursor / CTSDATE (first page `""`).
    pub cts_date: String,
    /// Body continuation time cursor / CTSTIME (first page `""`).
    pub cts_time: String,
    /// Exchange division code / 거래소구분코드.
    pub exchgubun: String,
}

/// `t1617` request (self-paginated; `cts_date`/`cts_time` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1617Request {
    #[serde(rename = "t1617InBlock")]
    pub inblock: T1617InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1617Request);
impl T1617Request {
    /// Build a first-page `t1617` investor time/daily-flow request
    /// (`cts_date`/`cts_time` empty).
    pub fn new(
        gubun1: impl Into<String>,
        gubun2: impl Into<String>,
        gubun3: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1617Request {
            inblock: T1617InBlock {
                gubun1: gubun1.into(),
                gubun2: gubun2.into(),
                gubun3: gubun3.into(),
                cts_date: String::new(),
                cts_time: String::new(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1617OutBlock` — the current-flow summary/cursor block (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1617OutBlock {
    /// Returned continuation date cursor / CTSDATE.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    /// Returned continuation time cursor / CTSTIME.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
    /// Individual net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Foreign net-buy / 외국인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Institution net-buy / 기관계순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
}

/// `t1617OutBlock1` — one investor time/daily-flow row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1617OutBlock1 {
    /// Date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Individual / 개인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Foreign / 외국인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Institution / 기관계.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
}

/// `t1617` response (single page). `outblock1` tolerated single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1617Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1617OutBlock", default)]
    pub outblock: T1617OutBlock,
    #[serde(
        rename = "t1617OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1617OutBlock1>,
}
