//! Exchange-broker paginated stock reads (`t1752`, `t1771`).
//!
//! Both are `[주식] 거래원` market-data reads at `/stock/exchange` that walk a
//! per-issue trading-member (거래원) series. Each is self-paginated on the body
//! `cts_idx` continuation cursor returned in its `{tr}OutBlock` summary block.
//! `t1752`'s row array arrives under `t1752OutBlock1`; `t1771`'s arrives under
//! `t1771OutBlock`**`2`** (note the `2`, not `1`). Only single-page scope is
//! promoted (`ls-core` threads no body-cursor collection).
//!
//! `cts_idx` is the genuinely-numeric first-page cursor (first page `0`) serialized
//! as a JSON **number** via [`ls_core::string_as_number`] — the string form returns
//! `IGW40011`; `t1771` additionally carries the numeric `cnt` count. The remaining
//! request fields are strings. The header `tr_cont`/`tr_cont_key` are
//! `#[serde(skip)]`.

use serde::{Deserialize, Serialize};

// ---- t1752 — 거래원별종목별동향 (broker-by-issue) ------------------------------

/// Input block for `t1752`. `cts_idx` is the genuinely-numeric body continuation
/// cursor serialized as a JSON number via [`ls_core::string_as_number`] (first page
/// `0`); `shcode`/`traddate1`/`traddate2`/`fwgubun1`/`exchgubun` are strings. Header
/// cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1752InBlock {
    /// Short code / 종목코드.
    pub shcode: String,
    /// Query date 1 / 조회날짜1 (YYYYMMDD).
    pub traddate1: String,
    /// Query date 2 / 조회날짜2 (YYYYMMDD).
    pub traddate2: String,
    /// Foreign-member division / 외국계구분.
    pub fwgubun1: String,
    /// Body continuation cursor / CTSIDX (genuinely numeric; first page `0`; number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_idx: String,
    /// Exchange division code / 거래소구분코드.
    pub exchgubun: String,
}

/// `t1752` request (self-paginated; `cts_idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1752Request {
    #[serde(rename = "t1752InBlock")]
    pub inblock: T1752InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1752Request);
impl T1752Request {
    /// Build a first-page `t1752` broker-by-issue request for one stock over a
    /// `traddate1`/`traddate2` window (`fwgubun1` caller-supplied, first-page
    /// `cts_idx` = `0`).
    pub fn new(
        shcode: impl Into<String>,
        traddate1: impl Into<String>,
        traddate2: impl Into<String>,
        fwgubun1: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1752Request {
            inblock: T1752InBlock {
                shcode: shcode.into(),
                traddate1: traddate1.into(),
                traddate2: traddate2.into(),
                fwgubun1: fwgubun1.into(),
                cts_idx: "0".to_string(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1752OutBlock` — the summary/cursor block (foreign-member totals + next-page `cts_idx`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1752OutBlock {
    /// Foreign-member sell / 외국계매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fwdvl: String,
    /// Foreign-member buy / 외국계매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fwsvl: String,
    /// Returned page index / CTSIDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_idx: String,
}

/// `t1752OutBlock1` — one broker row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1752OutBlock1 {
    /// Member firm / 회원사.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradname: String,
    /// Sell quantity / 매도수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol: String,
    /// Buy quantity / 매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmsvol: String,
    /// Net-buy / 순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmssvol: String,
    /// Member firm code / 회원사코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradno: String,
}

/// `t1752` response (single page). `outblock1` tolerated single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1752Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1752OutBlock", default)]
    pub outblock: T1752OutBlock,
    #[serde(
        rename = "t1752OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1752OutBlock1>,
}

// ---- t1771 — 거래원별시간대별추이 (broker time-series by issue) -----------------

/// Input block for `t1771`. `cts_idx` is the genuinely-numeric body continuation
/// cursor and `cnt` the genuinely-numeric count, both serialized as JSON numbers
/// via [`ls_core::string_as_number`] (first page `0`/`20`);
/// `shcode`/`tradno`/`gubun1`/`traddate1`/`traddate2`/`exchgubun` are strings.
/// Header cursors skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1771InBlock {
    /// Short code / 종목코드.
    pub shcode: String,
    /// Broker code / 거래원코드.
    pub tradno: String,
    /// Division 1 / 구분1.
    pub gubun1: String,
    /// Broker date 1 / 거래원날짜1 (YYYYMMDD).
    pub traddate1: String,
    /// Broker date 2 / 거래원날짜2 (YYYYMMDD).
    pub traddate2: String,
    /// Body continuation cursor / CTSIDX (genuinely numeric; first page `0`; number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_idx: String,
    /// Request count / 요청건수 (genuinely numeric; first page `20`; serialized as a number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Exchange division / 거래소구분.
    pub exchgubun: String,
}

/// `t1771` request (self-paginated; `cts_idx` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1771Request {
    #[serde(rename = "t1771InBlock")]
    pub inblock: T1771InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1771Request);
impl T1771Request {
    /// Build a first-page `t1771` broker time-series request for one stock over a
    /// `traddate1`/`traddate2` window (`tradno` caller-supplied, first-page
    /// `cts_idx` = `0`, `cnt` = `20`).
    pub fn new(
        shcode: impl Into<String>,
        tradno: impl Into<String>,
        gubun1: impl Into<String>,
        traddate1: impl Into<String>,
        traddate2: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1771Request {
            inblock: T1771InBlock {
                shcode: shcode.into(),
                tradno: tradno.into(),
                gubun1: gubun1.into(),
                traddate1: traddate1.into(),
                traddate2: traddate2.into(),
                cts_idx: "0".to_string(),
                cnt: "20".to_string(),
                exchgubun: exchgubun.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1771OutBlock` — the summary/cursor block (next-page `cts_idx`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1771OutBlock {
    /// Returned page index / CTSIDX.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_idx: String,
}

/// `t1771OutBlock2` — one broker-time row (representative subset). NOTE the block
/// suffix is `2`, not `1` — `t1771` has no `OutBlock1` row array.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1771OutBlock2 {
    /// Date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub traddate: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradtime: String,
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
    /// Net-buy / 순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmsscha: String,
}

/// `t1771` response (single page). The row array arrives under `t1771OutBlock2`
/// (not `OutBlock1`); tolerated single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1771Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1771OutBlock", default)]
    pub outblock: T1771OutBlock,
    #[serde(
        rename = "t1771OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<T1771OutBlock2>,
}
