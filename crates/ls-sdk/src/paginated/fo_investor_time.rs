//! F/O investor-by-time paginated TR (`t2541`).
//!
//! `t2541` (선물옵션 투자자별 매매추이) is a self-paginated F/O investor read whose
//! body continuation cursor is `cts_time` (a time string) paired with the numeric
//! `cts_idx`; `cts_idx`/`cnt` are genuinely-numeric request fields serialized as
//! JSON numbers via `string_as_number` (the string form returns `IGW40011`).
//! Single-page scope, like the other paginated reads.

use serde::{Deserialize, Serialize};

/// Input block for `t2541` — F/O investor-by-time. The body continuation cursor is
/// `cts_time` (first page = `""`) paired with the numeric `cts_idx` (first page =
/// `0`); `cts_idx`/`cnt` serialize as JSON numbers via [`ls_core::string_as_number`].
#[derive(Serialize, Debug, Clone)]
pub struct T2541InBlock {
    /// Product id / 상품ID (e.g. `"01"`).
    pub eitem: String,
    /// Market division / 시장구분.
    pub market: String,
    /// Sector code / 업종코드 (e.g. `"001"`).
    pub upcode: String,
    /// Quantity division / 수량구분.
    pub gubun1: String,
    /// Prior-day division / 전일분구분.
    pub gubun2: String,
    /// Continuation time cursor / CTSTIME (first page = `""`).
    pub cts_time: String,
    /// Continuation index cursor / CTSIDX (numeric; first page = `0`).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_idx: String,
    /// Requested row count / 조회건수 (numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
}

/// `t2541` request (self-paginated; `cts_time`/`cts_idx` in the body, header cursors
/// skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T2541Request {
    #[serde(rename = "t2541InBlock")]
    pub inblock: T2541InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T2541Request);
impl T2541Request {
    /// Build a first-page `t2541` investor-by-time request for one sector with the
    /// spec defaults (`eitem="01"`, `market="1"`, `gubun1="1"`, `gubun2="0"`,
    /// first-page `cts_time=""`/`cts_idx=0`, `cnt="20"`).
    pub fn new(upcode: impl Into<String>) -> Self {
        T2541Request {
            inblock: T2541InBlock {
                eitem: "01".to_string(),
                market: "1".to_string(),
                upcode: upcode.into(),
                gubun1: "1".to_string(),
                gubun2: "0".to_string(),
                cts_time: String::new(),
                cts_idx: "0".to_string(),
                cnt: "20".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t2541OutBlock` — the investor summary block (representative subset: the product
/// id, market division, the continuation `cts_time`, and the individual-investor
/// buy/net-buy aggregates). Every numeric-bearing field via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2541OutBlock {
    /// Product id / 상품ID.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub eitem: String,
    /// Market division / 시장구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sgubun: String,
    /// Continuation time cursor / CTSTIME.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_time: String,
    /// Individual buy / 개인매수 (개인 = code 08).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ms_08: String,
    /// Individual net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_08: String,
    /// Foreign net-buy / 외국인순매수 (외국인 = code 17).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_17: String,
}

/// `t2541OutBlock1` — one per-time investor net-buy row (representative subset).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2541OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Individual net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Foreign net-buy / 외국인순매수 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Institution net-buy / 기관계순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
}

/// `t2541` response (single page). `outblock` is the investor summary (next-page
/// `cts_time`); `outblock1` is the per-time net-buy array under `t2541OutBlock1`,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2541Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2541OutBlock", default)]
    pub outblock: T2541OutBlock,
    #[serde(
        rename = "t2541OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2541OutBlock1>,
}
