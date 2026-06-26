//! Plan -004 batch C — static reference / designation / ranking boards
//! (시가총액상위, 상·하한, 신고/신저가, 매매정지, ELW rankings, 신용거래동향).
//!
//! Self-paginated single-page reads on a body `idx`/`cts_shcode` cursor (header
//! `tr_cont`/`tr_cont_key` skipped). Numeric request fields (idx, jc_num, sprice,
//! eprice, volume, …) serialize as JSON numbers via `ls_core::string_as_number`
//! (IGW40011 guard); response fields via `ls_core::string_or_number`; row arrays
//! via `ls_core::de_vec_or_single`. Wire keys read from each raw `res_example`.

use serde::{Deserialize, Serialize};

/// Input block for `t1444`.
#[derive(Serialize, Debug, Clone)]
pub struct T1444InBlock {
    pub upcode: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1444` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1444Request {
    #[serde(rename = "t1444InBlock")]
    pub inblock: T1444InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1444Request);
impl T1444Request {
    /// Build a `t1444` request.
    pub fn new(upcode: impl Into<String>) -> Self {
        T1444Request {
            inblock: T1444InBlock {
                upcode: upcode.into(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1444OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1444OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1444OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1444OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub total: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub for_rate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vol_rate: String,
}

/// `t1444` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1444Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1444OutBlock", default)]
    pub outblock: T1444OutBlock,
    #[serde(rename = "t1444OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1444OutBlock1>,
}

/// Input block for `t1422`.
#[derive(Serialize, Debug, Clone)]
pub struct T1422InBlock {
    pub qrygb: String,
    pub gubun: String,
    pub jnilgubun: String,
    pub sign: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
    pub exchgubun: String,
}

/// `t1422` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1422Request {
    #[serde(rename = "t1422InBlock")]
    pub inblock: T1422InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1422Request);
impl T1422Request {
    /// Build a `t1422` request.
    pub fn new() -> Self {
        T1422Request {
            inblock: T1422InBlock {
                qrygb: "1".to_string(),
                gubun: "0".to_string(),
                jnilgubun: "0".to_string(),
                sign: "1".to_string(),
                jc_num: "8".to_string(),
                sprice: "0".to_string(),
                eprice: "0".to_string(),
                volume: "0".to_string(),
                idx: "0".to_string(),
                exchgubun: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1422OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1422OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1422OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1422OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff_vol: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub last: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lmtdaycnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilvolume: String,
}

/// `t1422` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1422Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1422OutBlock", default)]
    pub outblock: T1422OutBlock,
    #[serde(rename = "t1422OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1422OutBlock1>,
}

/// Input block for `t1427`.
#[derive(Serialize, Debug, Clone)]
pub struct T1427InBlock {
    pub qrygb: String,
    pub gubun: String,
    pub signgubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub diff: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
    pub jshex: String,
}

/// `t1427` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1427Request {
    #[serde(rename = "t1427InBlock")]
    pub inblock: T1427InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1427Request);
impl T1427Request {
    /// Build a `t1427` request.
    pub fn new() -> Self {
        T1427Request {
            inblock: T1427InBlock {
                qrygb: "1".to_string(),
                gubun: "0".to_string(),
                signgubun: "1".to_string(),
                diff: "0".to_string(),
                jc_num: "0".to_string(),
                sprice: "0".to_string(),
                eprice: "0".to_string(),
                volume: "0".to_string(),
                idx: "0".to_string(),
                jshex: "c".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1427OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1427OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1427OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1427OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub total: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lmtprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lmtdaycnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff_vol: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilvolume: String,
}

/// `t1427` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1427Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1427OutBlock", default)]
    pub outblock: T1427OutBlock,
    #[serde(rename = "t1427OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1427OutBlock1>,
}

/// Input block for `t1442`.
#[derive(Serialize, Debug, Clone)]
pub struct T1442InBlock {
    pub gubun: String,
    pub type1: String,
    pub type2: String,
    pub type3: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub jc_num2: String,
}

/// `t1442` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1442Request {
    #[serde(rename = "t1442InBlock")]
    pub inblock: T1442InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1442Request);
impl T1442Request {
    /// Build a `t1442` request.
    pub fn new() -> Self {
        T1442Request {
            inblock: T1442InBlock {
                gubun: "0".to_string(),
                type1: "0".to_string(),
                type2: "0".to_string(),
                type3: "0".to_string(),
                jc_num: "8".to_string(),
                sprice: "0".to_string(),
                eprice: "0".to_string(),
                volume: "0".to_string(),
                idx: "0".to_string(),
                jc_num2: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1442OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1442OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1442OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1442OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pastprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pastchange: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pastsign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pastdiff: String,
}

/// `t1442` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1442Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1442OutBlock", default)]
    pub outblock: T1442OutBlock,
    #[serde(rename = "t1442OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1442OutBlock1>,
}

/// Input block for `t1405`.
#[derive(Serialize, Debug, Clone)]
pub struct T1405InBlock {
    pub gubun: String,
    pub jongchk: String,
    pub cts_shcode: String,
}

/// `t1405` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1405Request {
    #[serde(rename = "t1405InBlock")]
    pub inblock: T1405InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1405Request);
impl T1405Request {
    /// Build a `t1405` request.
    pub fn new(gubun: impl Into<String>, jongchk: impl Into<String>) -> Self {
        T1405Request {
            inblock: T1405InBlock {
                gubun: gubun.into(),
                jongchk: jongchk.into(),
                cts_shcode: String::new(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1405OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1405OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_shcode: String,
}

/// `t1405OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1405OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub edate: String,
}

/// `t1405` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1405Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1405OutBlock", default)]
    pub outblock: T1405OutBlock,
    #[serde(rename = "t1405OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1405OutBlock1>,
}

/// Input block for `t1960`.
#[derive(Serialize, Debug, Clone)]
pub struct T1960InBlock {
    pub gubun: String,
    pub ggubun: String,
    pub itemcode: String,
    pub lastdate: String,
    pub exgubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sjanday: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ejanday: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1960` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1960Request {
    #[serde(rename = "t1960InBlock")]
    pub inblock: T1960InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1960Request);
impl T1960Request {
    /// Build a `t1960` request.
    pub fn new() -> Self {
        T1960Request {
            inblock: T1960InBlock {
                gubun: "0".to_string(),
                ggubun: "01".to_string(),
                itemcode: "".to_string(),
                lastdate: "".to_string(),
                exgubun: "0".to_string(),
                sprice: "0".to_string(),
                eprice: "0".to_string(),
                volume: "0".to_string(),
                sjanday: "0".to_string(),
                ejanday: "0".to_string(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1960OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1960OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1960OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1960OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwshcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemshcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemsign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemchange: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemdiff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub convrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bepoint: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwexec: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastdate: String,
}

/// `t1960` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1960Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1960OutBlock", default)]
    pub outblock: T1960OutBlock,
    #[serde(rename = "t1960OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1960OutBlock1>,
}

/// Input block for `t1961`.
#[derive(Serialize, Debug, Clone)]
pub struct T1961InBlock {
    pub gubun: String,
    pub ggubun: String,
    pub itemcode: String,
    pub lastdate: String,
    pub exgubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sjanday: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ejanday: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1961` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1961Request {
    #[serde(rename = "t1961InBlock")]
    pub inblock: T1961InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1961Request);
impl T1961Request {
    /// Build a `t1961` request.
    pub fn new() -> Self {
        T1961Request {
            inblock: T1961InBlock {
                gubun: "0".to_string(),
                ggubun: "01".to_string(),
                itemcode: "".to_string(),
                lastdate: "".to_string(),
                exgubun: "0".to_string(),
                sprice: "0".to_string(),
                eprice: "0".to_string(),
                volume: "0".to_string(),
                sjanday: "0".to_string(),
                ejanday: "0".to_string(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1961OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1961OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1961OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1961OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwshcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilvolume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemshcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemsign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemchange: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemdiff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub convrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwexec: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastdate: String,
}

/// `t1961` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1961Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1961OutBlock", default)]
    pub outblock: T1961OutBlock,
    #[serde(rename = "t1961OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1961OutBlock1>,
}

/// Input block for `t1966`.
#[derive(Serialize, Debug, Clone)]
pub struct T1966InBlock {
    pub gubun: String,
    pub ggubun: String,
    pub itemcode: String,
    pub lastdate: String,
    pub exgubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub eprice: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volume: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub sjanday: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ejanday: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1966` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1966Request {
    #[serde(rename = "t1966InBlock")]
    pub inblock: T1966InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1966Request);
impl T1966Request {
    /// Build a `t1966` request.
    pub fn new() -> Self {
        T1966Request {
            inblock: T1966InBlock {
                gubun: "0".to_string(),
                ggubun: "01".to_string(),
                itemcode: "".to_string(),
                lastdate: "".to_string(),
                exgubun: "0".to_string(),
                sprice: "0".to_string(),
                eprice: "0".to_string(),
                volume: "0".to_string(),
                sjanday: "0".to_string(),
                ejanday: "0".to_string(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1966OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1966OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1966OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1966OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwshcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilvalue: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemshcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemprice: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemsign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemchange: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemdiff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub convrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwexec: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastdate: String,
}

/// `t1966` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1966Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1966OutBlock", default)]
    pub outblock: T1966OutBlock,
    #[serde(rename = "t1966OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1966OutBlock1>,
}

/// Input block for `t1921`.
#[derive(Serialize, Debug, Clone)]
pub struct T1921InBlock {
    pub shcode: String,
    pub gubun: String,
    pub date: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,
}

/// `t1921` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1921Request {
    #[serde(rename = "t1921InBlock")]
    pub inblock: T1921InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1921Request);
impl T1921Request {
    /// Build a `t1921` request.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1921Request {
            inblock: T1921InBlock {
                shcode: shcode.into(),
                gubun: "1".to_string(),
                date: "".to_string(),
                idx: "0".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1921OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1921OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub idx: String,
}

/// `t1921OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1921OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mmdate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jchange: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jkrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gyrate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nvolume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jvolume: String,
}

/// `t1921` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1921Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1921OutBlock", default)]
    pub outblock: T1921OutBlock,
    #[serde(rename = "t1921OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1921OutBlock1>,
}
