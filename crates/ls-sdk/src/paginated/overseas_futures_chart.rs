//! Overseas-futures(-option) chart + market-data paginated TRs.
//!
//! All-lane closed-window flip wave (plan -003). These nine reads serve the
//! `/overseas-futureoption/chart` and `/overseas-futureoption/market-data`
//! endpoints and persist last-session data on the paper gateway under KRX closure
//! when given a CURRENT front-month contract (e.g. `CUSN26`); the raw
//! `req_example`'s stale 2023 contract (`ADM23`) returns empty — not a feed gap.
//!
//! Each is self-paginated on a body cursor (header `tr_cont`/`tr_cont_key`
//! skipped, mirror `t1514`/`t3401`): 분봉/일주월 charts on `cts_date`(`/cts_time`),
//! tick reads on `cts_seq`, NTick reads on `cts_seq`/`cts_daygb`. Genuinely-numeric
//! request fields (`ncnt`/`readcnt`/`qrycnt`/`cts_seq` numeric form) serialize as
//! JSON numbers via [`ls_core::string_as_number`] (the string form returns
//! `IGW40011`, KTD3); string cursors stay `String`. Wire keys + array-ness read
//! from the raw `res_example`.

use serde::{Deserialize, Serialize};

// =============================================================================
// o3103 — 해외선물차트 분봉 조회 (overseas-futures minute chart; cts_date/cts_time).
// =============================================================================

/// Input block for `o3103` — overseas-futures 분봉 chart. `shcode` selects the
/// contract; `ncnt`/`readcnt` are numeric (JSON numbers); `cts_date`/`cts_time`
/// are the body continuation cursors.
#[derive(Serialize, Debug, Clone)]
pub struct O3103InBlock {
    /// Symbol / 단축코드.
    pub shcode: String,
    /// N-minute bucket / N분 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ncnt: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Body continuation cursor (date) / CTS_DATE.
    pub cts_date: String,
    /// Body continuation cursor (time) / CTS_TIME.
    pub cts_time: String,
}

/// `o3103` request (self-paginated; `cts_date`/`cts_time` in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3103Request {
    #[serde(rename = "o3103InBlock")]
    pub inblock: O3103InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3103Request);
impl O3103Request {
    /// Build a first-page `o3103` 분봉 request for one contract (`shcode`), spec
    /// defaults (`ncnt=1`, `readcnt=20`, first-page cursors).
    pub fn new(shcode: impl Into<String>) -> Self {
        O3103Request {
            inblock: O3103InBlock {
                shcode: shcode.into(),
                ncnt: "1".to_string(),
                readcnt: "20".to_string(),
                cts_date: String::new(),
                cts_time: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3103OutBlock` — continuation-cursor + chart header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3103OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub readcnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub timediff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
}

/// `o3103OutBlock1` — one minute-candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3103OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3103` response (single page): cursor `outblock` + candle `outblock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3103Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3103OutBlock", default)]
    pub outblock: O3103OutBlock,
    #[serde(
        rename = "o3103OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3103OutBlock1>,
}

// =============================================================================
// o3108 — 해외선물차트(일주월) 조회 (overseas-futures D/W/M chart; cts_date).
// =============================================================================

/// Input block for `o3108` — overseas-futures D/W/M chart. `shcode` selects the
/// contract; `gubun` is the period mode; `qrycnt` is numeric; `sdate`/`edate`
/// bound the range; `cts_date` is the body cursor.
#[derive(Serialize, Debug, Clone)]
pub struct O3108InBlock {
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Period division / 주기구분.
    pub gubun: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Start date / 시작일자.
    pub sdate: String,
    /// End date / 종료일자.
    pub edate: String,
    /// Body continuation cursor (date) / CTS_DATE.
    pub cts_date: String,
}

/// `o3108` request (self-paginated; `cts_date` in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3108Request {
    #[serde(rename = "o3108InBlock")]
    pub inblock: O3108InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3108Request);
impl O3108Request {
    /// Build a first-page `o3108` D/W/M chart request (`shcode`, period `gubun`,
    /// `sdate`/`edate`); spec defaults (`qrycnt=20`, first-page cursor).
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        O3108Request {
            inblock: O3108InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                qrycnt: "20".to_string(),
                sdate: sdate.into(),
                edate: edate.into(),
                cts_date: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3108OutBlock` — chart header / continuation cursor.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3108OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `o3108OutBlock1` — one D/W/M candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3108OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3108` response (single page): cursor `outblock` + candle `outblock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3108Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3108OutBlock", default)]
    pub outblock: O3108OutBlock,
    #[serde(
        rename = "o3108OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3108OutBlock1>,
}

// =============================================================================
// o3116 — 해외선물 시간대별(Tick)체결 조회 (overseas-futures tick; cts_seq).
// =============================================================================

/// Input block for `o3116` — overseas-futures tick. `gubun`/`shcode` select;
/// `readcnt`/`cts_seq` are numeric (JSON numbers).
#[derive(Serialize, Debug, Clone)]
pub struct O3116InBlock {
    /// Division / 구분.
    pub gubun: String,
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Body continuation cursor (seq) / CTS_SEQ (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_seq: String,
}

/// `o3116` request (self-paginated; `cts_seq` numeric in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3116Request {
    #[serde(rename = "o3116InBlock")]
    pub inblock: O3116InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3116Request);
impl O3116Request {
    /// Build a first-page `o3116` tick request (`gubun`, `shcode`); spec defaults
    /// (`readcnt=20`, `cts_seq=0`).
    pub fn new(gubun: impl Into<String>, shcode: impl Into<String>) -> Self {
        O3116Request {
            inblock: O3116InBlock {
                gubun: gubun.into(),
                shcode: shcode.into(),
                readcnt: "20".to_string(),
                cts_seq: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3116OutBlock` — continuation-cursor header (`cts_seq`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3116OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_seq: String,
}

/// `o3116OutBlock1` — one tick row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3116OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ovstime: String,
    /// Price / 체결가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ovsdate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
}

/// `o3116` response (single page): cursor `outblock` + tick `outblock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3116Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3116OutBlock", default)]
    pub outblock: O3116OutBlock,
    #[serde(
        rename = "o3116OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3116OutBlock1>,
}

// =============================================================================
// o3117 — 해외선물 차트 NTick 체결 조회 (overseas-futures NTick; cts_seq/cts_daygb).
// =============================================================================

/// Input block for `o3117` — overseas-futures NTick chart. `shcode` selects;
/// `ncnt`/`qrycnt` numeric; `cts_seq`/`cts_daygb` are the (string) body cursors.
#[derive(Serialize, Debug, Clone)]
pub struct O3117InBlock {
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Tick count / N틱 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ncnt: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Body continuation cursor (seq) / CTS_SEQ.
    pub cts_seq: String,
    /// Body continuation cursor (day-gubun) / CTS_DAYGB.
    pub cts_daygb: String,
}

/// `o3117` request (self-paginated; `cts_seq`/`cts_daygb` in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3117Request {
    #[serde(rename = "o3117InBlock")]
    pub inblock: O3117InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3117Request);
impl O3117Request {
    /// Build a first-page `o3117` NTick request (`shcode`); spec defaults
    /// (`ncnt=0`, `qrycnt=20`, first-page cursors).
    pub fn new(shcode: impl Into<String>) -> Self {
        O3117Request {
            inblock: O3117InBlock {
                shcode: shcode.into(),
                ncnt: "0".to_string(),
                qrycnt: "20".to_string(),
                cts_seq: String::new(),
                cts_daygb: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3117OutBlock` — continuation-cursor header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3117OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_daygb: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_seq: String,
}

/// `o3117OutBlock1` — one NTick candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3117OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3117` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3117Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3117OutBlock", default)]
    pub outblock: O3117OutBlock,
    #[serde(
        rename = "o3117OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3117OutBlock1>,
}

// =============================================================================
// o3123 — 해외선물옵션 차트 분봉 조회 (futopt minute chart; cts_date/cts_time).
// =============================================================================

/// Input block for `o3123` — overseas-futopt 분봉 chart. `mktgb`/`shcode` select;
/// `ncnt`/`readcnt` numeric; `cts_date`/`cts_time` are body cursors.
#[derive(Serialize, Debug, Clone)]
pub struct O3123InBlock {
    /// Market gubun / 시장구분.
    pub mktgb: String,
    /// Symbol / 단축코드.
    pub shcode: String,
    /// N-minute bucket / N분 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ncnt: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Body continuation cursor (date) / CTS_DATE.
    pub cts_date: String,
    /// Body continuation cursor (time) / CTS_TIME.
    pub cts_time: String,
}

/// `o3123` request (self-paginated; `cts_date`/`cts_time` in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3123Request {
    #[serde(rename = "o3123InBlock")]
    pub inblock: O3123InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3123Request);
impl O3123Request {
    /// Build a first-page `o3123` futopt 분봉 request (`mktgb`, `shcode`); spec
    /// defaults (`ncnt=1`, `readcnt=20`, first-page cursors).
    pub fn new(mktgb: impl Into<String>, shcode: impl Into<String>) -> Self {
        O3123Request {
            inblock: O3123InBlock {
                mktgb: mktgb.into(),
                shcode: shcode.into(),
                ncnt: "1".to_string(),
                readcnt: "20".to_string(),
                cts_date: String::new(),
                cts_time: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3123OutBlock` — chart header / continuation cursor.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3123OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub readcnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub timediff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
}

/// `o3123OutBlock1` — one minute-candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3123OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3123` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3123Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3123OutBlock", default)]
    pub outblock: O3123OutBlock,
    #[serde(
        rename = "o3123OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3123OutBlock1>,
}

// =============================================================================
// o3128 — 해외선물옵션 차트 일주월 조회 (futopt D/W/M chart; cts_date).
// =============================================================================

/// Input block for `o3128` — overseas-futopt D/W/M chart. `mktgb`/`shcode` select;
/// `gubun` period; `qrycnt` numeric; `sdate`/`edate` range; `cts_date` body cursor.
#[derive(Serialize, Debug, Clone)]
pub struct O3128InBlock {
    /// Market gubun / 시장구분.
    pub mktgb: String,
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Period division / 주기구분.
    pub gubun: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Start date / 시작일자.
    pub sdate: String,
    /// End date / 종료일자.
    pub edate: String,
    /// Body continuation cursor (date) / CTS_DATE.
    pub cts_date: String,
}

/// `o3128` request (self-paginated; `cts_date` in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3128Request {
    #[serde(rename = "o3128InBlock")]
    pub inblock: O3128InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3128Request);
impl O3128Request {
    /// Build a first-page `o3128` futopt D/W/M chart request (`mktgb`, `shcode`,
    /// period `gubun`, `sdate`/`edate`); spec defaults (`qrycnt=20`, first-page cursor).
    pub fn new(
        mktgb: impl Into<String>,
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        sdate: impl Into<String>,
        edate: impl Into<String>,
    ) -> Self {
        O3128Request {
            inblock: O3128InBlock {
                mktgb: mktgb.into(),
                shcode: shcode.into(),
                gubun: gubun.into(),
                qrycnt: "20".to_string(),
                sdate: sdate.into(),
                edate: edate.into(),
                cts_date: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3128OutBlock` — chart header / continuation cursor.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3128OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
    /// Daily close / 일종가 (substantive header field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diclose: String,
}

/// `o3128OutBlock1` — one D/W/M candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3128OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3128` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3128Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3128OutBlock", default)]
    pub outblock: O3128OutBlock,
    #[serde(
        rename = "o3128OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3128OutBlock1>,
}

// =============================================================================
// o3136 — 해외선물옵션 시간대별 Tick 체결 조회 (futopt tick; cts_seq).
// =============================================================================

/// Input block for `o3136` — overseas-futopt tick. `gubun`/`mktgb`/`shcode`
/// select; `readcnt`/`cts_seq` numeric.
#[derive(Serialize, Debug, Clone)]
pub struct O3136InBlock {
    /// Division / 구분.
    pub gubun: String,
    /// Market gubun / 시장구분.
    pub mktgb: String,
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Body continuation cursor (seq) / CTS_SEQ (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_seq: String,
}

/// `o3136` request (self-paginated; `cts_seq` numeric in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3136Request {
    #[serde(rename = "o3136InBlock")]
    pub inblock: O3136InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3136Request);
impl O3136Request {
    /// Build a first-page `o3136` futopt tick request (`gubun`, `mktgb`, `shcode`);
    /// spec defaults (`readcnt=20`, `cts_seq=0`).
    pub fn new(
        gubun: impl Into<String>,
        mktgb: impl Into<String>,
        shcode: impl Into<String>,
    ) -> Self {
        O3136Request {
            inblock: O3136InBlock {
                gubun: gubun.into(),
                mktgb: mktgb.into(),
                shcode: shcode.into(),
                readcnt: "20".to_string(),
                cts_seq: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3136OutBlock` — continuation-cursor header (`cts_seq`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3136OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_seq: String,
}

/// `o3136OutBlock1` — one tick row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3136OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ovstime: String,
    /// Price / 체결가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ovsdate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
}

/// `o3136` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3136Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3136OutBlock", default)]
    pub outblock: O3136OutBlock,
    #[serde(
        rename = "o3136OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3136OutBlock1>,
}

// =============================================================================
// o3137 — 해외선물옵션 차트 NTick 체결 조회 (futopt NTick; cts_seq/cts_daygb).
// =============================================================================

/// Input block for `o3137` — overseas-futopt NTick chart. `mktgb`/`shcode`
/// select; `ncnt`/`qrycnt` numeric; `cts_seq`/`cts_daygb` string body cursors.
#[derive(Serialize, Debug, Clone)]
pub struct O3137InBlock {
    /// Market gubun / 시장구분.
    pub mktgb: String,
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Tick count / N틱 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ncnt: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Body continuation cursor (seq) / CTS_SEQ.
    pub cts_seq: String,
    /// Body continuation cursor (day-gubun) / CTS_DAYGB.
    pub cts_daygb: String,
}

/// `o3137` request (self-paginated; `cts_seq`/`cts_daygb` in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3137Request {
    #[serde(rename = "o3137InBlock")]
    pub inblock: O3137InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3137Request);
impl O3137Request {
    /// Build a first-page `o3137` futopt NTick request (`mktgb`, `shcode`); spec
    /// defaults (`ncnt=1`, `qrycnt=20`, first-page cursors).
    pub fn new(mktgb: impl Into<String>, shcode: impl Into<String>) -> Self {
        O3137Request {
            inblock: O3137InBlock {
                mktgb: mktgb.into(),
                shcode: shcode.into(),
                ncnt: "1".to_string(),
                qrycnt: "20".to_string(),
                cts_seq: String::new(),
                cts_daygb: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3137OutBlock` — continuation-cursor header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3137OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_daygb: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_seq: String,
}

/// `o3137OutBlock1` — one NTick candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3137OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3137` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3137Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3137OutBlock", default)]
    pub outblock: O3137OutBlock,
    #[serde(
        rename = "o3137OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3137OutBlock1>,
}

// =============================================================================
// o3139 — 해외선물옵션차트용NTick(고정형) (futopt NTick fixed; cts_seq/cts_daygb).
// =============================================================================

/// Input block for `o3139` — overseas-futopt NTick fixed chart. Same shape as
/// `o3137`: `mktgb`/`shcode` select; `ncnt`/`qrycnt` numeric; string body cursors.
#[derive(Serialize, Debug, Clone)]
pub struct O3139InBlock {
    /// Market gubun / 시장구분.
    pub mktgb: String,
    /// Symbol / 단축코드.
    pub shcode: String,
    /// Tick count / N틱 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ncnt: String,
    /// Row count / 조회건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Body continuation cursor (seq) / CTS_SEQ.
    pub cts_seq: String,
    /// Body continuation cursor (day-gubun) / CTS_DAYGB.
    pub cts_daygb: String,
}

/// `o3139` request (self-paginated; `cts_seq`/`cts_daygb` in the body).
#[derive(Serialize, Debug, Clone)]
pub struct O3139Request {
    #[serde(rename = "o3139InBlock")]
    pub inblock: O3139InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(O3139Request);
impl O3139Request {
    /// Build a first-page `o3139` futopt NTick-fixed request (`mktgb`, `shcode`);
    /// spec defaults (`ncnt=1`, `qrycnt=20`, first-page cursors).
    pub fn new(mktgb: impl Into<String>, shcode: impl Into<String>) -> Self {
        O3139Request {
            inblock: O3139InBlock {
                mktgb: mktgb.into(),
                shcode: shcode.into(),
                ncnt: "1".to_string(),
                qrycnt: "20".to_string(),
                cts_seq: String::new(),
                cts_daygb: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `o3139OutBlock` — continuation-cursor header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3139OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub last_count: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_daygb: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_seq: String,
}

/// `o3139OutBlock1` — one NTick candle row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3139OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Close / 종가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3139` response (single page).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3139Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3139OutBlock", default)]
    pub outblock: O3139OutBlock,
    #[serde(
        rename = "o3139OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<O3139OutBlock1>,
}
