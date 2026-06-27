//! Signal-search paginated TR (`t1809` — 신호조회).
//!
//! A single-page self-paginated read like `t1410` (`low_liquidity.rs`): the body
//! continuation cursor is the string `cts` (first page `"1"`), and every in-block
//! field serializes as a JSON string (no `string_as_number` / IGW40011 fix). The
//! summary block `t1809OutBlock` holds only the next-page `cts`; the signal rows
//! arrive as a top-level sibling array `t1809OutBlock1` (read from the raw
//! `res_example` / old-SDK shape, NOT the flattened normalized baseline, which
//! collapses the row fields under `t1809OutBlock`).
//!
//! Note the exact wire casing of the 종목구분 filter: the request key is `jmGb`
//! (capital `G`), carried via `#[serde(rename = "jmGb")]`.

use serde::{Deserialize, Serialize};

/// Input block for `t1809` — 신호조회 (signal search). Every field is a string;
/// the body continuation cursor is `cts` (an ordinary in-block field at its
/// first-page `"1"` convention, NOT skipped). `jmGb` carries the exact wire
/// casing (capital `G`). The header `tr_cont`/`tr_cont_key` are skipped.
#[derive(Serialize, Debug, Clone)]
pub struct T1809InBlock {
    /// Signal division / 신호구분.
    pub gubun: String,
    /// Item division / 종목구분 (exact wire key `jmGb`, capital `G`).
    #[serde(rename = "jmGb")]
    pub jm_gb: String,
    /// Short code / 종목코드.
    pub jmcode: String,
    /// Body continuation cursor / NEXTKEY (first page `"1"`).
    pub cts: String,
}

/// `t1809` request (self-paginated; `cts` in the body, header cursors skipped).
#[derive(Serialize, Debug, Clone)]
pub struct T1809Request {
    #[serde(rename = "t1809InBlock")]
    pub inblock: T1809InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1809Request);
impl T1809Request {
    /// Build a first-page `t1809` signal-search request. `cts` defaults to the
    /// first-page convention (`"1"`); `tr_cont`/`tr_cont_key` start empty. Every
    /// field stays a string.
    pub fn new(
        gubun: impl Into<String>,
        jm_gb: impl Into<String>,
        jmcode: impl Into<String>,
    ) -> Self {
        T1809Request {
            inblock: T1809InBlock {
                gubun: gubun.into(),
                jm_gb: jm_gb.into(),
                jmcode: jmcode.into(),
                cts: "1".to_string(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t1809OutBlock` — the signal-search summary block (next-page `cts` cursor
/// only; the row fields are a sibling array, not nested here).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1809OutBlock {
    /// Returned continuation cursor / NEXTKEY.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts: String,
}

/// `t1809OutBlock1` — one signal row (representative subset of the wire fields;
/// numeric-bearing fields via [`ls_core::string_or_number`] to tolerate JSON
/// string OR number).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1809OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Signal id / 신호ID.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub signal_id: String,
    /// Signal name / 신호명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub signal_desc: String,
    /// Signal division / 신호구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
    /// Signal short code / 신호종목.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jmcode: String,
    /// Item price / 종목가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign vs. previous close / 종목등락부호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change rate / 대비등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chgrate: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Signal datetime / 신호일시.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub datetime: String,
}

/// `t1809` response (single page). `outblock` is the summary (next-page `cts`);
/// `outblock1` is the signal array under `t1809OutBlock1`, tolerated as
/// single-or-array via [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1809Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1809OutBlock", default)]
    pub outblock: T1809OutBlock,
    #[serde(
        rename = "t1809OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1809OutBlock1>,
}
