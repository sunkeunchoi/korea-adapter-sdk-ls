//! Market-session dependency class — `t1102` current-price (시세) quote.
//!
//! This is the *market_session* class: market-data queries scoped to a trading
//! session, credentialed but with no account state and — for `t1102` —
//! structurally **non-paginated**. The LS `t1102` TR (주식현재가(시세)조회)
//! returns a single snapshot quote for one symbol, so there is no continuation
//! to thread and no `HasPagination` impl: dispatch is a plain
//! [`ls_core::Inner::post`].
//!
//! ## Wire-compat: string-or-number coercion
//!
//! The LS gateway is inconsistent about whether numeric quote fields arrive as
//! JSON numbers (`"price": 4535`) or JSON strings (`"price": "4535"`) — the
//! captured spec example shows `price`/`volume` as bare numbers while `sign`
//! arrives as a string. Every numeric-bearing field therefore uses
//! [`ls_core::string_or_number`] so both shapes deserialize to the same `String`
//! without a panic. This is the load-bearing behavior R10 preserves; the
//! `market_session_tests` regression pins it against the spec-derived shape.
//!
//! ## No `tr_cont`/`tr_cont_key` in the body — by construction
//!
//! Because `t1102` is not paginated, the request carries NO continuation fields
//! at all. [`T1102Request`] serializes to exactly `{"t1102InBlock":{...}}`, so
//! the continuation tokens can never leak into the request body.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ls_core::{Inner, LsResult};

/// Input block for `t1102` — the symbol to quote.
///
/// `shcode` is the 6-digit short code (단축코드). `exchgubun` is the exchange
/// distinction (거래소 구분). Both are caller-supplied identifiers.
#[derive(Serialize, Debug, Clone)]
pub struct T1102InBlock {
    /// Short code / 단축코드 (e.g. `"078020"`).
    pub shcode: String,
    /// Exchange distinction / 거래소 구분.
    pub exchgubun: String,
}

/// `t1102` request — wraps the input block under the `t1102InBlock` key.
///
/// Serializes to `{"t1102InBlock":{"shcode":...,"exchgubun":...}}`. There are no
/// `tr_cont`/`tr_cont_key` fields: `t1102` is not paginated, so the continuation
/// tokens are structurally absent from the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1102Request {
    #[serde(rename = "t1102InBlock")]
    pub inblock: T1102InBlock,
}

impl T1102Request {
    /// Build a `t1102` request for one symbol on one exchange.
    pub fn new(shcode: impl Into<String>, exchgubun: impl Into<String>) -> Self {
        T1102Request {
            inblock: T1102InBlock {
                shcode: shcode.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1102OutBlock` — the snapshot quote.
///
/// A representative, spec-grounded subset of the LS `t1102OutBlock`: the core
/// quote fields plus the level-1 bid/offer aggregates. Every numeric-bearing
/// field uses [`ls_core::string_or_number`] because the gateway sends them as
/// either JSON numbers or JSON strings; `#[serde(default)]` on the struct lets a
/// sparse/empty out-block deserialize cleanly. Field names mirror the LS spec
/// (`specs/ls_openapi_specs.json` → `t1102OutBlock`) verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1102OutBlock {
    /// Korean name / 한글 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비 구분 (e.g. `"2"` = up). Arrives as a string in the spec.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Upper limit price / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous day's volume / 전일거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilvolume: String,
    /// Volume difference vs. previous day / 거래량 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volumediff: String,
}

/// `t1102` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in
/// `ls-core` dispatch before this struct is built); `outblock` is the snapshot
/// quote under the `t1102OutBlock` key. All three are `#[serde(default)]` so a
/// terse or partial envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1102Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1102OutBlock", default)]
    pub outblock: T1102OutBlock,
}

/// Input block for `t1101` — the symbol to look up.
///
/// `shcode` is the 6-digit short code (단축코드). Unlike `t1102`, the `t1101`
/// request carries no `exchgubun`: the spec's `t1101InBlock` is `shcode`-only.
#[derive(Serialize, Debug, Clone)]
pub struct T1101InBlock {
    /// Short code / 단축코드 (e.g. `"078020"`).
    pub shcode: String,
}

/// `t1101` request — wraps the input block under the `t1101InBlock` key.
///
/// Serializes to `{"t1101InBlock":{"shcode":...}}`. `t1101` is a single snapshot
/// (current price + order book), not paginated, so there are no
/// `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1101Request {
    #[serde(rename = "t1101InBlock")]
    pub inblock: T1101InBlock,
}

impl T1101Request {
    /// Build a `t1101` request for one symbol.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1101Request {
            inblock: T1101InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1101OutBlock` — current-price header plus the 10-level order book.
///
/// A representative, spec-grounded subset of the LS `t1101OutBlock`: the
/// current-price header fields plus all ten offer/bid price+quantity levels.
/// Every numeric-bearing field uses [`ls_core::string_or_number`] because the
/// gateway sends them as either JSON numbers or JSON strings; `#[serde(default)]`
/// on the struct lets a sparse/empty out-block deserialize cleanly. Field names
/// mirror the LS spec (`t1101OutBlock`) verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1101OutBlock {
    /// Korean name / 한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Offer (ask) prices, levels 1–10 / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho10: String,
    /// Bid prices, levels 1–10 / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho10: String,
    /// Offer (ask) quantities, levels 1–10 / 매도호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem10: String,
    /// Bid quantities, levels 1–10 / 매수호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem10: String,
    /// Total offer quantity / 총매도호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// Total bid quantity / 총매수호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bid: String,
}

/// `t1101` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in `ls-core`
/// dispatch before this struct is built); `outblock` is the snapshot under the
/// `t1101OutBlock` key. All three are `#[serde(default)]` so a terse or partial
/// envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1101Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1101OutBlock", default)]
    pub outblock: T1101OutBlock,
}

/// Input block for `t8425` — 전체테마 (all themes).
///
/// `t8425` is a no-caller-input read: the spec's `t8425InBlock` carries a single
/// length-1 `dummy` placeholder (단축코드-style filler), so callers supply
/// nothing. Modeled after `T1102InBlock` *minus* every caller identifier.
#[derive(Serialize, Debug, Clone)]
pub struct T8425InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8425` request — wraps the input block under the `t8425InBlock` key.
///
/// Serializes to `{"t8425InBlock":{"dummy":""}}`. `t8425` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8425Request {
    #[serde(rename = "t8425InBlock")]
    pub inblock: T8425InBlock,
}

impl T8425Request {
    /// Build a `t8425` all-themes request. Takes no caller input; the `dummy`
    /// placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8425Request {
            inblock: T8425InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8425Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8425OutBlock` — one theme row.
///
/// The `t8425OutBlock` response block is a repeated array of theme rows (the spec
/// marks the block itself `Binary`, the array marker), so [`T8425Response`] holds
/// a `Vec` of these tolerated as single-or-array via [`ls_core::de_vec_or_single`].
/// Both fields use [`ls_core::string_or_number`] for wire-type tolerance and
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8425OutBlock {
    /// Theme name / 테마명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
    /// Theme code / 테마코드 (the representative caller input for `t1531`/`t1537`).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcode: String,
}

/// `t8425` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in `ls-core`
/// dispatch before this struct is built); `outblock` is the all-themes array
/// under the `t8425OutBlock` key, tolerated as a single object OR an array via
/// [`ls_core::de_vec_or_single`]. All three are `#[serde(default)]` so a terse or
/// empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8425Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8425OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8425OutBlock>,
}

/// Input block for `t8436` — 주식종목조회 (stock master list).
///
/// `gubun` is a market-segment FILTER (구분: `"0"` 전체 / `"1"` 코스피 /
/// `"2"` 코스닥), not an instrument identifier — the read returns the whole list
/// for the chosen segment.
#[derive(Serialize, Debug, Clone)]
pub struct T8436InBlock {
    /// Market-segment filter / 구분 (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ).
    pub gubun: String,
}

/// `t8436` request — wraps the input block under the `t8436InBlock` key.
///
/// Serializes to `{"t8436InBlock":{"gubun":"0"}}`. `t8436` is not paginated, so
/// there are no continuation fields in the body; `gubun` is a filter selector.
#[derive(Serialize, Debug, Clone)]
pub struct T8436Request {
    #[serde(rename = "t8436InBlock")]
    pub inblock: T8436InBlock,
}

impl T8436Request {
    /// Build a `t8436` stock-list request for one market segment (`gubun`).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8436Request {
            inblock: T8436InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8436OutBlock` — one stock-master row.
///
/// The `t8436OutBlock` response block is a repeated array (the spec marks the
/// block `Binary`), so [`T8436Response`] holds a `Vec` tolerated as single-or-
/// array via [`ls_core::de_vec_or_single`]. A representative, spec-grounded
/// subset; every field uses [`ls_core::string_or_number`] for wire-type
/// tolerance and `#[serde(default)]` lets a sparse row deserialize cleanly.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8436OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드 (6-digit).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// ETF distinction / ETF구분 (`"1"` ETF / `"2"` ETN).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etfgubun: String,
    /// Upper limit price / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous close / 전일가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Market segment / 구분 (`"1"` KOSPI / `"2"` KOSDAQ).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// `t8436` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// stock-master array under the `t8436OutBlock` key, tolerated as single-or-array
/// via [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8436Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8436OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8436OutBlock>,
}

/// Input block for `t1531` — 테마별종목 (stocks in a theme).
///
/// The spec marks BOTH `tmname` (테마명) and `tmcode` (테마코드) required, so
/// callers pass a matched name+code pair (e.g. a row from [`MarketSession::all_themes`]).
#[derive(Serialize, Debug, Clone)]
pub struct T1531InBlock {
    /// Theme name / 테마명.
    pub tmname: String,
    /// Theme code / 테마코드 (4-digit).
    pub tmcode: String,
}

/// `t1531` request — wraps the input block under the `t1531InBlock` key.
#[derive(Serialize, Debug, Clone)]
pub struct T1531Request {
    #[serde(rename = "t1531InBlock")]
    pub inblock: T1531InBlock,
}

impl T1531Request {
    /// Build a `t1531` request for one theme (name + code, both required).
    pub fn new(tmname: impl Into<String>, tmcode: impl Into<String>) -> Self {
        T1531Request {
            inblock: T1531InBlock {
                tmname: tmname.into(),
                tmcode: tmcode.into(),
            },
        }
    }
}

/// `t1531OutBlock` — one theme-constituent row.
///
/// The `t1531OutBlock` response block is a repeated array (the spec marks it
/// `Binary`), so [`T1531Response`] holds a `Vec` tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. Every field uses [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1531OutBlock {
    /// Theme name / 테마명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
    /// Average rate of change / 평균등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub avgdiff: String,
    /// Theme code / 테마코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcode: String,
}

/// `t1531` response envelope. `outblock` is the theme-row array under the
/// `t1531OutBlock` key, tolerated as single-or-array. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1531Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1531OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1531OutBlock>,
}

/// Input block for `t1537` — 테마종목별시세조회 (quotes for a theme's stocks).
///
/// Keyed by `tmcode` (테마코드) alone.
#[derive(Serialize, Debug, Clone)]
pub struct T1537InBlock {
    /// Theme code / 테마코드 (4-digit).
    pub tmcode: String,
}

/// `t1537` request — wraps the input block under the `t1537InBlock` key.
#[derive(Serialize, Debug, Clone)]
pub struct T1537Request {
    #[serde(rename = "t1537InBlock")]
    pub inblock: T1537InBlock,
}

impl T1537Request {
    /// Build a `t1537` request for one theme code.
    pub fn new(tmcode: impl Into<String>) -> Self {
        T1537Request {
            inblock: T1537InBlock {
                tmcode: tmcode.into(),
            },
        }
    }
}

/// `t1537OutBlock` — the theme summary block (single object).
///
/// Carries the theme-level aggregates; the per-stock rows are in
/// [`T1537OutBlock1`]. Every field uses [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1537OutBlock {
    /// Advancing-issue count / 상승종목수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcnt: String,
    /// Theme issue count / 테마종목수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcnt: String,
    /// Advancing-issue ratio / 상승종목비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uprate: String,
    /// Theme name / 테마명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
}

/// `t1537OutBlock1` — one per-stock quote row within the theme.
///
/// The repeated row block (`t1537OutBlock1[]`); a representative subset of the
/// spec fields, every one via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1537OutBlock1 {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1537` response envelope.
///
/// `outblock` is the theme summary; `outblock1` is the per-stock quote array
/// under the `t1537OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1537Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1537OutBlock", default)]
    pub outblock: T1537OutBlock,
    #[serde(
        rename = "t1537OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1537OutBlock1>,
}

/// Input block for `t1859` — 서버저장조건 조건검색 (server-saved condition search).
///
/// Keyed by `query_index` (서버저장인덱스), the saved-condition index produced by
/// `t1866` (`t1866OutBlock1.query_index`) — the modeled cross-TR discovery edge.
/// The caller never fabricates it; it is self-sourced from a `t1866` list call.
#[derive(Serialize, Debug, Clone)]
pub struct T1859InBlock {
    /// Server-saved condition index / 서버저장인덱스 (from `t1866`).
    pub query_index: String,
}

/// `t1859` request — wraps the input block under the `t1859InBlock` key.
///
/// Serializes to `{"t1859InBlock":{"query_index":...}}`. `t1859` is not paginated,
/// so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1859Request {
    #[serde(rename = "t1859InBlock")]
    pub inblock: T1859InBlock,
}

impl T1859Request {
    /// Build a `t1859` condition-search request for one saved-condition
    /// `query_index` (source it from [`crate::paginated::T1866Response`]).
    pub fn new(query_index: impl Into<String>) -> Self {
        T1859Request {
            inblock: T1859InBlock {
                query_index: query_index.into(),
            },
        }
    }
}

/// `t1859OutBlock` — the condition-search summary block (single object).
///
/// `result_count` (검색종목수) is the modeled non-key signal proving a populated
/// response. Every field uses [`ls_core::string_or_number`] for wire-type
/// tolerance; `#[serde(default)]` lets a sparse/empty out-block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1859OutBlock {
    /// Matched-issue count / 검색종목수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub result_count: String,
    /// Capture time / 포착시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub result_time: String,
    /// Strategy description / 전략설명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub text: String,
}

/// `t1859OutBlock1` — one matched-issue row.
///
/// The repeated row block (`t1859OutBlock1[]`); a representative subset of the
/// spec fields, every one via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1859OutBlock1 {
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1859` response envelope.
///
/// `outblock` is the search summary; `outblock1` is the matched-issue array under
/// the `t1859OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1859Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1859OutBlock", default)]
    pub outblock: T1859OutBlock,
    #[serde(
        rename = "t1859OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1859OutBlock1>,
}

/// Input block for `t1826` — 종목Q클릭검색리스트조회 (ThinQ Q-click search-list
/// inquiry; the Wave 3 producer).
///
/// `search_gb` selects which search catalog to list (검색구분):
/// `"0"` 핵심검색 / `"1"` 지표검색 / `"2"` 시세동향 / `"3"` 투자자동향. It is a
/// documented filter enum, not an instrument identifier. The response carries the
/// `search_cd` catalog keys that `t1825` consumes (the modeled discovery edge).
#[derive(Serialize, Debug, Clone)]
pub struct T1826InBlock {
    /// Search catalog / 검색구분 (`"0"`–`"3"`).
    pub search_gb: String,
}

/// `t1826` request — wraps the input block under the `t1826InBlock` key.
///
/// Serializes to `{"t1826InBlock":{"search_gb":...}}`. `t1826` is not paginated,
/// so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1826Request {
    #[serde(rename = "t1826InBlock")]
    pub inblock: T1826InBlock,
}

impl T1826Request {
    /// Build a `t1826` search-list request for one search catalog (`search_gb`,
    /// `"0"` 핵심검색 being the representative core-search catalog).
    pub fn new(search_gb: impl Into<String>) -> Self {
        T1826Request {
            inblock: T1826InBlock {
                search_gb: search_gb.into(),
            },
        }
    }
}

/// `t1826OutBlock` — one available-search row (`t1826OutBlock[]`).
///
/// `search_cd` (검색코드) is the catalog key fed to `t1825`; `search_nm` (검색명)
/// is its display name. Both via [`ls_core::string_or_number`] for wire-type
/// tolerance; `#[serde(default)]` lets a sparse/empty row deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1826OutBlock {
    /// Search code / 검색코드 (the `t1825` `search_cd` input).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub search_cd: String,
    /// Search name / 검색명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub search_nm: String,
}

/// `t1826` response envelope.
///
/// `outblock` is the available-search array under the `t1826OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1826Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1826OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1826OutBlock>,
}

/// Input block for `t1825` — 종목Q클릭검색 (ThinQ Q-click search; the Wave 3
/// consumer).
///
/// `search_cd` (검색코드) is the catalog key produced by `t1826`
/// (`t1826OutBlock.search_cd`) — the modeled cross-TR discovery edge; the caller
/// never fabricates it, it is self-sourced from a `t1826` list call. `gubun`
/// (구분) is a market filter: `"0"` 전체 / `"1"` 코스피 / `"2"` 코스닥.
#[derive(Serialize, Debug, Clone)]
pub struct T1825InBlock {
    /// Search code / 검색코드 (from `t1826`).
    pub search_cd: String,
    /// Market filter / 구분 (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ).
    pub gubun: String,
}

/// `t1825` request — wraps the input block under the `t1825InBlock` key.
///
/// Serializes to `{"t1825InBlock":{"search_cd":...,"gubun":...}}`. `t1825` is not
/// paginated, so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1825Request {
    #[serde(rename = "t1825InBlock")]
    pub inblock: T1825InBlock,
}

impl T1825Request {
    /// Build a `t1825` Q-click search request keyed by one `search_cd` (source it
    /// from [`T1826Response`]) and a `gubun` market filter (`"0"` 전체).
    pub fn new(search_cd: impl Into<String>, gubun: impl Into<String>) -> Self {
        T1825Request {
            inblock: T1825InBlock {
                search_cd: search_cd.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t1825OutBlock` — the Q-click search summary block (single object).
///
/// `jong_cnt` (검색종목수) is the modeled non-key signal proving a populated
/// response. Via [`ls_core::string_or_number`] for wire-type tolerance;
/// `#[serde(default)]` lets a sparse/empty out-block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1825OutBlock {
    /// Matched-issue count / 검색종목수.
    #[serde(rename = "JongCnt", deserialize_with = "ls_core::string_or_number")]
    pub jong_cnt: String,
}

/// `t1825OutBlock1` — one matched-issue row (`t1825OutBlock1[]`).
///
/// A representative subset of the spec fields, every one via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1825OutBlock1 {
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1825` response envelope.
///
/// `outblock` is the search summary; `outblock1` is the matched-issue array under
/// the `t1825OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1825Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1825OutBlock", default)]
    pub outblock: T1825OutBlock,
    #[serde(
        rename = "t1825OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1825OutBlock1>,
}

// ---------------------------------------------------------------------------
// Wave 1 — ELW universe & instrument surface. No-caller-input `dummy` reads
// (t9905, t9907, t8431, t9942) modeled after `t8425`; each returns a list keyed
// by a code field. All `/stock/elw`, `[주식] ELW`, non-paginated market_session.
// ---------------------------------------------------------------------------

/// Input block for `t9905` — 기초자산리스트조회 (full underlying-asset list). A
/// no-caller-input read: a single length-1 `dummy` placeholder.
#[derive(Serialize, Debug, Clone)]
pub struct T9905InBlock {
    /// Dummy placeholder / DUMMY (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9905` request — serializes to `{"t9905InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9905Request {
    #[serde(rename = "t9905InBlock")]
    pub inblock: T9905InBlock,
}
impl T9905Request {
    /// Build a `t9905` underlying-list request (no caller input).
    pub fn new() -> Self {
        T9905Request {
            inblock: T9905InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T9905Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9905OutBlock1` — one underlying-asset row. `shcode` (단축코드) is the
/// underlying-asset code consumed by `t1964` (`item`). All via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9905OutBlock1 {
    /// Short code / 단축코드 (the underlying-asset code; `t1964` `item` input).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Standard code / 표준코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
}

/// `t9905` response — underlying-asset array under `t9905OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9905Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9905OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T9905OutBlock1>,
}

/// Input block for `t9907` — 만기월조회 (ELW expiry-month list). No caller input.
#[derive(Serialize, Debug, Clone)]
pub struct T9907InBlock {
    /// Dummy placeholder / DUMMY (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9907` request — serializes to `{"t9907InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9907Request {
    #[serde(rename = "t9907InBlock")]
    pub inblock: T9907InBlock,
}
impl T9907Request {
    /// Build a `t9907` expiry-month request (no caller input).
    pub fn new() -> Self {
        T9907Request {
            inblock: T9907InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T9907Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9907OutBlock1` — one expiry-month row. All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9907OutBlock1 {
    /// Expiry month / 만기월 (`YYYYMM`).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastym: String,
    /// Expiry-month name / 만기월명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastnm: String,
}

/// `t9907` response — expiry-month array under `t9907OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9907Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9907OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T9907OutBlock1>,
}

/// Input block for `t8431` — ELW종목조회 (ELW symbol list; the Wave 1 spine
/// producer for `t1958`). No caller input.
#[derive(Serialize, Debug, Clone)]
pub struct T8431InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8431` request — serializes to `{"t8431InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8431Request {
    #[serde(rename = "t8431InBlock")]
    pub inblock: T8431InBlock,
}
impl T8431Request {
    /// Build a `t8431` ELW-symbol-list request (no caller input).
    pub fn new() -> Self {
        T8431Request {
            inblock: T8431InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T8431Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8431OutBlock` — one ELW symbol row. `shcode` (단축코드) is the ELW code fed
/// to `t1958` (the comparison pair). All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8431OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드 (the ELW code; `t1958` `shcode1`/`shcode2` input).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Reference price / 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
}

/// `t8431` response — ELW symbol array under `t8431OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8431Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8431OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8431OutBlock>,
}

/// Input block for `t9942` — ELW마스터조회API용 (ELW master list). No caller input.
#[derive(Serialize, Debug, Clone)]
pub struct T9942InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9942` request — serializes to `{"t9942InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9942Request {
    #[serde(rename = "t9942InBlock")]
    pub inblock: T9942InBlock,
}
impl T9942Request {
    /// Build a `t9942` ELW-master request (no caller input).
    pub fn new() -> Self {
        T9942Request {
            inblock: T9942InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T9942Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9942OutBlock` — one ELW master row. All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9942OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
}

/// `t9942` response — ELW master array under `t9942OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9942Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9942OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9942OutBlock>,
}

/// Input block for `t1958` — ELW종목비교 (ELW symbol comparison; the Wave 1
/// comparison member). Keyed by two ELW codes (`shcode1`/`shcode2`) self-sourced
/// from `t8431` (`t8431OutBlock.shcode`) — the modeled discovery edge; never
/// fabricated.
#[derive(Serialize, Debug, Clone)]
pub struct T1958InBlock {
    /// First ELW code / 종목코드1 (from `t8431`).
    pub shcode1: String,
    /// Second ELW code / 종목코드2 (from `t8431`).
    pub shcode2: String,
}

/// `t1958` request — serializes to `{"t1958InBlock":{"shcode1":...,"shcode2":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1958Request {
    #[serde(rename = "t1958InBlock")]
    pub inblock: T1958InBlock,
}
impl T1958Request {
    /// Build a `t1958` comparison request for two ELW codes (source both from
    /// [`T8431Response`]).
    pub fn new(shcode1: impl Into<String>, shcode2: impl Into<String>) -> Self {
        T1958Request {
            inblock: T1958InBlock {
                shcode1: shcode1.into(),
                shcode2: shcode2.into(),
            },
        }
    }
}

/// `t1958OutBlock` / `t1958OutBlock1` — one ELW symbol's detail (single object;
/// the two compared symbols). A representative subset, every field via
/// [`ls_core::string_or_number`]; `hname` is the modeled non-key signal of a
/// populated comparison.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1958Detail {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Underlying asset / 기초자산.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub item1: String,
    /// Issuer / 발행사.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub issuernmk: String,
    /// Call/put / 콜풋구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwopt: String,
    /// Price / 가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
}

/// `t1958OutBlock2` — the comparison block (the `…cmp` fields; single object). A
/// representative subset via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1958Compare {
    /// Compared name / 종목명비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hnamecmp: String,
    /// Compared underlying / 기초자산비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub item1cmp: String,
    /// Compared price / 가격비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricecmp: String,
    /// Compared volume / 거래량비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volumecmp: String,
    /// Compared rate of change / 등락율비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diffcmp: String,
}

/// `t1958` response — the first symbol (`outblock`), the second (`outblock1`),
/// and the comparison block (`outblock2`); all single objects, all
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1958Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1958OutBlock", default)]
    pub outblock: T1958Detail,
    #[serde(rename = "t1958OutBlock1", default)]
    pub outblock1: T1958Detail,
    #[serde(rename = "t1958OutBlock2", default)]
    pub outblock2: T1958Compare,
}

/// Input block for `t1964` — ELW전광판 (ELW board; the Wave 1 board member).
/// `item` (기초자산코드) is the underlying-asset code self-sourced from `t9905`
/// (`t9905OutBlock1.shcode`) — the modeled discovery edge; the remaining fields
/// are broad/default filters.
#[derive(Serialize, Debug, Clone)]
pub struct T1964InBlock {
    /// Underlying-asset code / 기초자산코드 (from `t9905`).
    pub item: String,
    /// Issuer / 발행사 (broad: empty = all).
    pub issuercd: String,
    /// Expiry month / 만기월물 (broad: empty = all).
    pub lastmonth: String,
    /// Call/put / 콜풋구분 (broad: `"0"`).
    pub elwopt: String,
    /// Moneyness / 머니구분 (broad: `"0"`).
    pub atmgubun: String,
    /// Exercise type / 권리행사방식 (broad: `"0"`).
    pub elwtype: String,
    /// Settlement / 결제방법 (broad: `"0"`).
    pub settletype: String,
    /// Exercise underlying class / 행사기초자산구분 (broad: `"0"`).
    pub elwexecgubun: String,
    /// Ratio range start / 시작비율 (broad: `"0"`).
    pub fromrat: String,
    /// Ratio range end / 종료비율 (broad: `"0"`).
    pub torat: String,
    /// Volume filter / 거래량 (broad: `"0"`).
    pub volume: String,
}

/// `t1964` request — serializes to `{"t1964InBlock":{...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1964Request {
    #[serde(rename = "t1964InBlock")]
    pub inblock: T1964InBlock,
}
impl T1964Request {
    /// Build a `t1964` board request for one underlying-asset code (source it from
    /// [`T9905Response`]) with broad/default filters for the remaining fields.
    pub fn new(item: impl Into<String>) -> Self {
        T1964Request {
            inblock: T1964InBlock {
                item: item.into(),
                issuercd: String::new(),
                lastmonth: String::new(),
                elwopt: "0".into(),
                atmgubun: "0".into(),
                elwtype: "0".into(),
                settletype: "0".into(),
                elwexecgubun: "0".into(),
                fromrat: "0".into(),
                torat: "0".into(),
                volume: "0".into(),
            },
        }
    }
}

/// `t1964OutBlock1` — one ELW board row. `shcode` (ELW코드) and `item1`
/// (기초자산코드) via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1964OutBlock1 {
    /// ELW code / ELW코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Underlying-asset code / 기초자산코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub item1: String,
    /// Underlying-asset name / 기초자산명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemnm: String,
    /// Issuer / 발행사.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub issuernmk: String,
}

/// `t1964` response — ELW board array under `t1964OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1964Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1964OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1964OutBlock1>,
}

// ---------------------------------------------------------------------------
// Wave 2 — market-flow analytics surface. Investor-flow (t1601/t1615/t1664) and
// program-trading (t1640/t1662) aggregates; gubun-filter screens with documented
// default inputs. All non-paginated market_session reads.
// ---------------------------------------------------------------------------

/// Input block for `t1601` — 투자자별종합 (investor-by-type aggregate). All-gubun
/// filter screen; `::new()` bakes documented defaults (amount basis, KRX).
#[derive(Serialize, Debug, Clone)]
pub struct T1601InBlock {
    /// Stock amount/quantity / 주식금액수량구분1 (`"1"` qty / `"2"` amount).
    pub gubun1: String,
    /// Option amount/quantity / 옵션금액수량구분2.
    pub gubun2: String,
    /// Amount unit / 금액단위 (unused; `"0"`).
    pub gubun3: String,
    /// Futures amount/quantity / 선물금액수량구분4.
    pub gubun4: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1601` request — wraps the in-block under `t1601InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1601Request {
    #[serde(rename = "t1601InBlock")]
    pub inblock: T1601InBlock,
}
impl T1601Request {
    /// Build a `t1601` request with documented broad defaults (amount basis, KRX).
    pub fn new() -> Self {
        T1601Request {
            inblock: T1601InBlock {
                gubun1: "2".into(),
                gubun2: "2".into(),
                gubun3: "0".into(),
                gubun4: "2".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1601Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1601OutBlock1` — the investor-by-type aggregate (single summary object; a
/// representative subset of net-buy columns). All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1601OutBlock1 {
    /// Personal net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_08: String,
    /// Foreign net-buy / 외국인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_17: String,
    /// Institutional net-buy / 기관계순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svolume_18: String,
}

/// `t1601` response — the investor aggregate under `t1601OutBlock1` (single object).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1601Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1601OutBlock1", default)]
    pub outblock1: T1601OutBlock1,
}

/// Input block for `t1615` — 투자자매매종합1 (investor trading aggregate).
#[derive(Serialize, Debug, Clone)]
pub struct T1615InBlock {
    /// Stock division / 주식구분 (`"1"` qty / `"2"` amount).
    pub gubun1: String,
    /// Option division / 옵션구분.
    pub gubun2: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1615` request — wraps the in-block under `t1615InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1615Request {
    #[serde(rename = "t1615InBlock")]
    pub inblock: T1615InBlock,
}
impl T1615Request {
    /// Build a `t1615` request with documented broad defaults (amount basis, KRX).
    pub fn new() -> Self {
        T1615Request {
            inblock: T1615InBlock {
                gubun1: "2".into(),
                gubun2: "2".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1615Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1615OutBlock` — the trading summary (single object). Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1615OutBlock {
    /// Total quantity / 합계수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sum_volume: String,
    /// Total amount / 합계금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sum_value: String,
}

/// `t1615OutBlock1` — one per-market investor row. Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1615OutBlock1 {
    /// Market name / 시장명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Personal / 개인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_08: String,
    /// Foreign / 외국인.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_17: String,
    /// Institutional / 기관계.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sv_18: String,
}

/// `t1615` response — summary + per-market array (single-or-array tolerated).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1615Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1615OutBlock", default)]
    pub outblock: T1615OutBlock,
    #[serde(
        rename = "t1615OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1615OutBlock1>,
}

/// Input block for `t1640` — 프로그램매매종합조회(미니) (program-trading aggregate).
#[derive(Serialize, Debug, Clone)]
pub struct T1640InBlock {
    /// Division / 구분 (`"11"` exchange-all).
    pub gubun: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1640` request — wraps the in-block under `t1640InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1640Request {
    #[serde(rename = "t1640InBlock")]
    pub inblock: T1640InBlock,
}
impl T1640Request {
    /// Build a `t1640` request with documented broad defaults (exchange-all, KRX).
    pub fn new() -> Self {
        T1640Request {
            inblock: T1640InBlock {
                gubun: "11".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1640Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1640OutBlock` — the program-trading summary (single object). Subset via
/// [`ls_core::string_or_number`]; `value` is the modeled non-key signal.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1640OutBlock {
    /// Net-buy quantity / 순매수수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Net-buy amount / 순매수금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Basis / 베이시스.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub basis: String,
}

/// `t1640` response — the program summary (single object).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1640Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1640OutBlock", default)]
    pub outblock: T1640OutBlock,
}

/// Input block for `t1662` — 시간대별프로그램매매추이(차트) (by-time program-trading
/// chart).
#[derive(Serialize, Debug, Clone)]
pub struct T1662InBlock {
    /// Market / 구분 (`"0"` KOSPI / `"1"` KOSDAQ).
    pub gubun: String,
    /// Amount/quantity / 금액수량구분 (`"0"` amount / `"1"` qty).
    pub gubun1: String,
    /// Day / 전일구분 (`"0"` today / `"1"` prior).
    pub gubun3: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1662` request — wraps the in-block under `t1662InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1662Request {
    #[serde(rename = "t1662InBlock")]
    pub inblock: T1662InBlock,
}
impl T1662Request {
    /// Build a `t1662` request with documented broad defaults (KOSPI, amount,
    /// today, KRX).
    pub fn new() -> Self {
        T1662Request {
            inblock: T1662InBlock {
                gubun: "0".into(),
                gubun1: "0".into(),
                gubun3: "0".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1662Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1662OutBlock` — one by-time program-trading row. Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1662OutBlock {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// KOSPI200 index / KP200.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k200jisu: String,
    /// Total net-buy / 전체순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tot3: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1662` response — the by-time array under `t1662OutBlock` (single-or-array).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1662Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1662OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1662OutBlock>,
}

/// Input block for `t1664` — 투자자매매종합(챠트) (investor trading chart). `cnt`
/// is a numeric count serialized as a JSON number.
#[derive(Serialize, Debug, Clone)]
pub struct T1664InBlock {
    /// Market / 시장구분 (`"1"` KOSPI).
    pub mgubun: String,
    /// Amount/quantity / 금액수량구분 (`"2"` amount).
    pub vagubun: String,
    /// Time/day / 시간일별구분 (`"1"` by-time).
    pub bdgubun: String,
    /// Row count / 조회건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Exchange / 거래소구분코드 (`"K"` KRX).
    pub exchgubun: String,
}

/// `t1664` request — wraps the in-block under `t1664InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1664Request {
    #[serde(rename = "t1664InBlock")]
    pub inblock: T1664InBlock,
}
impl T1664Request {
    /// Build a `t1664` request with documented broad defaults (KOSPI, amount,
    /// by-time, 20 rows, KRX).
    pub fn new() -> Self {
        T1664Request {
            inblock: T1664InBlock {
                mgubun: "1".into(),
                vagubun: "2".into(),
                bdgubun: "1".into(),
                cnt: "20".into(),
                exchgubun: "K".into(),
            },
        }
    }
}
impl Default for T1664Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t1664OutBlock1` — one investor-chart row. Subset via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1664OutBlock1 {
    /// Date/time / 일자시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dt: String,
    /// Personal net-buy / 개인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj08: String,
    /// Foreign net-buy / 외국인순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj17: String,
    /// Institutional net-buy / 기관순매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjj18: String,
}

/// `t1664` response — the investor-chart array under `t1664OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1664Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1664OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1664OutBlock1>,
}

// ---------------------------------------------------------------------------
// [업종] 시세 — sector/index cluster (Wave A). All on `/indtp/market-data`,
// instrument_domain `sector_index`. `upcode` (업종코드, e.g. "001"=코스피종합) is a
// fixed-width sector code → stays string-serialized; never `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t8424` — 전체업종 (all-sectors list). `gubun1` is an optional
/// filter; the all-sectors read sends it empty.
#[derive(Serialize, Debug, Clone)]
pub struct T8424InBlock {
    /// Filter / 구분 (empty = all sectors).
    pub gubun1: String,
}

/// `t8424` request — serializes to `{"t8424InBlock":{"gubun1":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8424Request {
    #[serde(rename = "t8424InBlock")]
    pub inblock: T8424InBlock,
}
impl T8424Request {
    /// Build a `t8424` all-sectors request (no meaningful caller input).
    pub fn new() -> Self {
        T8424Request {
            inblock: T8424InBlock {
                gubun1: String::new(),
            },
        }
    }
}
impl Default for T8424Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8424OutBlock` — one sector row: the `upcode` (업종코드) fed to the four
/// consumers (`t1511`/`t1514`/`t1516`/`t1485`) and its Korean name.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8424OutBlock {
    /// Sector name / 업종명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Sector code / 업종코드 (the `upcode` consumer key; string, never numeric).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
}

/// `t8424` response — the sector array under `t8424OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8424Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8424OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8424OutBlock>,
}

/// Input block for `t1511` — 업종현재가 (index snapshot for one sector).
#[derive(Serialize, Debug, Clone)]
pub struct T1511InBlock {
    /// Sector code / 업종코드 (e.g. "001"; from `t8424` or a literal sector code).
    pub upcode: String,
}

/// `t1511` request — serializes to `{"t1511InBlock":{"upcode":"001"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1511Request {
    #[serde(rename = "t1511InBlock")]
    pub inblock: T1511InBlock,
}
impl T1511Request {
    /// Build a `t1511` index-snapshot request for one sector code.
    pub fn new(upcode: impl Into<String>) -> Self {
        T1511Request {
            inblock: T1511InBlock {
                upcode: upcode.into(),
            },
        }
    }
}

/// `t1511OutBlock` — the index snapshot. A representative, spec-grounded subset
/// of the ~65-field `t1511OutBlock`; every numeric-bearing field via
/// [`ls_core::string_or_number`] (the gateway mixes string and number forms).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1511OutBlock {
    /// Sector name / 업종명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current index / 현재지수 — the canonical composite index value.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// First comparison sub-index / 첫번째지수 (distinct from `pricejisu`; for
    /// KOSPI composite the two coincide, but they diverge for other sectors).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub firstjisu: String,
    /// Previous-day index / 전일지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jniljisu: String,
    /// Open index / 시가지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openjisu: String,
    /// High index / 고가지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highjisu: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t1511` response — single snapshot under `t1511OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1511Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1511OutBlock", default)]
    pub outblock: T1511OutBlock,
}

/// Input block for `t1485` — 예상지수 (expected/auction index for one sector).
#[derive(Serialize, Debug, Clone)]
pub struct T1485InBlock {
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Mode / 구분.
    pub gubun: String,
}

/// `t1485` request — serializes to `{"t1485InBlock":{"upcode":"001","gubun":"1"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1485Request {
    #[serde(rename = "t1485InBlock")]
    pub inblock: T1485InBlock,
}
impl T1485Request {
    /// Build a `t1485` expected-index request for one sector and mode.
    pub fn new(upcode: impl Into<String>, gubun: impl Into<String>) -> Self {
        T1485Request {
            inblock: T1485InBlock {
                upcode: upcode.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t1485OutBlock` — expected-index summary. Numerics via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1485OutBlock {
    /// Expected index / 예상지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1485OutBlock1` — one expected-index time row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1485OutBlock1 {
    /// Index / 지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Time / 체결시간 (may be a label like "장 전").
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
}

/// `t1485` response — summary `t1485OutBlock` + the time array `t1485OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1485Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1485OutBlock", default)]
    pub outblock: T1485OutBlock,
    #[serde(
        rename = "t1485OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1485OutBlock1>,
}

/// Input block for `t1516` — 업종별종목시세 (per-sector stock board). Carries two
/// caller-supplied identifiers: the sector `upcode` and a `shcode` ticker.
#[derive(Serialize, Debug, Clone)]
pub struct T1516InBlock {
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Mode / 구분.
    pub gubun: String,
    /// Stock short code / 종목코드 (a 6-char ticker; empty returns the full board).
    pub shcode: String,
}

/// `t1516` request — `{"t1516InBlock":{"upcode":"001","gubun":"1","shcode":"005930"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1516Request {
    #[serde(rename = "t1516InBlock")]
    pub inblock: T1516InBlock,
}
impl T1516Request {
    /// Build a `t1516` per-sector stock-board request.
    pub fn new(
        upcode: impl Into<String>,
        gubun: impl Into<String>,
        shcode: impl Into<String>,
    ) -> Self {
        T1516Request {
            inblock: T1516InBlock {
                upcode: upcode.into(),
                gubun: gubun.into(),
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1516OutBlock` — sector-board summary header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1516OutBlock {
    /// Echoed stock short code / 종목코드 (confirms which board was returned).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Sector index / 지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Index change vs previous / 지수대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jdiff: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
}

/// `t1516OutBlock1` — one stock row within the sector board.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1516OutBlock1 {
    /// Stock short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Stock name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t1516` response — summary `t1516OutBlock` + per-stock array `t1516OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1516Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1516OutBlock", default)]
    pub outblock: T1516OutBlock,
    #[serde(
        rename = "t1516OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1516OutBlock1>,
}

// ---------------------------------------------------------------------------
// t2301 — 옵션전광판 (option board). market_session, non-paginated. Keyed by a
// contract month `yyyymm` (월물) + a `gubun` mini/regular selector
// (미니구분 — `"M"` mini / `"G"` regular). The out-block is a single struct: the
// board header carries the near-month futures snapshot (`gm*` fields) plus the
// representative call-option leg; the deeper per-strike legs are nested object
// arrays the recipe models as a representative subset, not in full.
// ---------------------------------------------------------------------------

/// Input block for `t2301` — the contract month + mini/regular selector.
///
/// `yyyymm` (월물) is the contract month, `YYYYMM` (e.g. `"202609"`); the spec
/// types it `String` (length 6). `gubun` (미니구분) selects mini vs regular:
/// `"M"` 미니 / `"G"` 정규. Both are caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T2301InBlock {
    /// Contract month / 월물 (`YYYYMM`, e.g. `"202609"`).
    pub yyyymm: String,
    /// Mini/regular selector / 미니구분 (`"M"` mini / `"G"` regular).
    pub gubun: String,
}

/// `t2301` request — wraps the input block under the `t2301InBlock` key.
///
/// Serializes to `{"t2301InBlock":{"yyyymm":...,"gubun":...}}`. `t2301` is not
/// paginated, so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T2301Request {
    #[serde(rename = "t2301InBlock")]
    pub inblock: T2301InBlock,
}

impl T2301Request {
    /// Build a `t2301` option-board request for one contract month + selector.
    pub fn new(yyyymm: impl Into<String>, gubun: impl Into<String>) -> Self {
        T2301Request {
            inblock: T2301InBlock {
                yyyymm: yyyymm.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t2301OutBlock` — the option-board header (single object).
///
/// A representative, spec-grounded subset of the 76-field `t2301OutBlock`: the
/// near-month futures snapshot (`gm*` — the board's headline current value) and
/// the representative call-option leg. `gmprice` (근월물현재가, near-month
/// futures current price) is the canonical current-value field, resolved by its
/// `korean_name` from the baseline. Every numeric-bearing field uses
/// [`ls_core::string_or_number`] for wire-type tolerance; `#[serde(default)]`
/// lets a sparse/empty out-block deserialize cleanly. Field names mirror the LS
/// spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2301OutBlock {
    /// Historical volatility / 역사적변동성.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub histimpv: String,
    /// Option days-to-expiry / 옵션잔존일.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jandatecnt: String,
    /// Near-month futures current price / 근월물현재가 (the canonical current value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmprice: String,
    /// Near-month sign vs. previous close / 근월물전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmsign: String,
    /// Near-month change vs. previous close / 근월물전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmchange: String,
    /// Near-month rate of change / 근월물등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmdiff: String,
    /// Near-month volume / 근월물거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmvolume: String,
    /// Near-month futures code / 근월물선물코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmshcode: String,
    /// Call-option representative IV / 콜옵션대표IV.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cimpv: String,
    /// Put-option representative IV / 풋옵션대표IV.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pimpv: String,
}

/// `t2301` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the board
/// header under the `t2301OutBlock` key. All `#[serde(default)]` so a terse or
/// empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2301Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2301OutBlock", default)]
    pub outblock: T2301OutBlock,
}

// ---------------------------------------------------------------------------
// t2522 — 주식선물기초자산조회 (stock-futures underlying-asset master). market_session,
// non-paginated. A no-caller-input read: the spec's `t2522InBlock` carries a single
// length-1 `dummy` placeholder, so callers supply nothing. The response is a count
// header (`t2522OutBlock`, single) plus the underlying-asset rows
// (`t2522OutBlock1`, an object array) — the data-bearing block where each row's
// 기초자산명 lives.
// ---------------------------------------------------------------------------

/// Input block for `t2522` — a no-caller-input read.
///
/// The spec's `t2522InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8425InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T2522InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t2522` request — wraps the input block under the `t2522InBlock` key.
///
/// Serializes to `{"t2522InBlock":{"dummy":""}}`. `t2522` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T2522Request {
    #[serde(rename = "t2522InBlock")]
    pub inblock: T2522InBlock,
}

impl T2522Request {
    /// Build a `t2522` stock-futures underlying-asset request. Takes no caller
    /// input; the `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T2522Request {
            inblock: T2522InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T2522Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t2522OutBlock` — the count header (single object).
///
/// Carries the row count (`cnt` / 건수); the underlying-asset rows themselves are
/// in [`T2522OutBlock1`]. `cnt` uses [`ls_core::string_or_number`] (the gateway
/// sends it as a JSON number); `#[serde(default)]` lets a sparse/empty header
/// deserialize cleanly.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2522OutBlock {
    /// Row count / 건수 (arrives as a JSON number).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
}

/// `t2522OutBlock1` — one stock-futures underlying-asset row.
///
/// The data-bearing repeated block (`t2522OutBlock1[]`). `bsc_asts_nm`
/// (기초자산명, the underlying-asset name) is the canonical identity field,
/// resolved by its `korean_name` from the baseline; the remaining fields are the
/// underlying codes. Every field uses [`ls_core::string_or_number`] for wire-type
/// tolerance; `#[serde(default)]` lets a sparse row deserialize cleanly. Field
/// names mirror the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2522OutBlock1 {
    /// Underlying-asset name / 기초자산명 (the canonical identity field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsc_asts_nm: String,
    /// Underlying-asset issue code / 기초자산종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsc_asts_is_cd: String,
    /// Underlying-asset ID / 기초자산ID.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsc_asts_id: String,
    /// Near-month issue code / 최근월물종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nmc_is_shrt_cd: String,
}

/// `t2522` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the count
/// header under the `t2522OutBlock` key; `outblock1` is the underlying-asset row
/// array under the `t2522OutBlock1` key, tolerated as a single object OR an array
/// via [`ls_core::de_vec_or_single`]. All `#[serde(default)]` so a terse or empty
/// envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2522Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2522OutBlock", default)]
    pub outblock: T2522OutBlock,
    #[serde(
        rename = "t2522OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2522OutBlock1>,
}

// ---------------------------------------------------------------------------
// t8401 — 주식선물마스터조회 (stock-futures master). market_session, non-paginated.
// A no-caller-input read: the spec's `t8401InBlock` carries a single length-1
// `dummy` placeholder, so callers supply nothing. The response is a single
// out-block `t8401OutBlock` that is itself the data-bearing ROW ARRAY (the raw
// capture's `res_example` shows `"t8401OutBlock": [ {…}, … ]`, propertyType
// A0005 / propertyOrder 002.00x children) — one stock-futures contract per row.
// There is no separate count header. Modeled after `T8425` (single row-array
// out-block).
// ---------------------------------------------------------------------------

/// Input block for `t8401` — a no-caller-input read.
///
/// The spec's `t8401InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8425InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T8401InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8401` request — wraps the input block under the `t8401InBlock` key.
///
/// Serializes to `{"t8401InBlock":{"dummy":""}}`. `t8401` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8401Request {
    #[serde(rename = "t8401InBlock")]
    pub inblock: T8401InBlock,
}

impl T8401Request {
    /// Build a `t8401` stock-futures master request. Takes no caller input; the
    /// `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8401Request {
            inblock: T8401InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8401Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8401OutBlock` — one stock-futures master row.
///
/// The data-bearing repeated block (`t8401OutBlock[]`). `hname` (종목명, the
/// stock-futures contract name) is the canonical identity field, resolved by its
/// `korean_name` from the baseline; the remaining fields are the contract codes.
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim. All fields are spec `String` types; no numeric coercion
/// is required here.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8401OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드.
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Underlying-asset code / 기초자산코드.
    pub basecode: String,
}

/// `t8401` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// stock-futures master row array under the `t8401OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8401Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8401OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8401OutBlock>,
}

// ---------------------------------------------------------------------------
// t8426 — 상품선물마스터조회 (commodity-futures master). market_session,
// non-paginated. A no-caller-input read: the spec's `t8426InBlock` carries a
// single length-1 `dummy` placeholder, so callers supply nothing. The response
// is a single out-block `t8426OutBlock` that is itself the data-bearing ROW
// ARRAY (the raw capture's `res_example` shows `"t8426OutBlock": [ {…}, … ]`) —
// one commodity-futures contract per row. There is no separate count header.
// Modeled after `T8401` (single row-array out-block).
// ---------------------------------------------------------------------------

/// Input block for `t8426` — a no-caller-input read.
///
/// The spec's `t8426InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8401InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T8426InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8426` request — wraps the input block under the `t8426InBlock` key.
///
/// Serializes to `{"t8426InBlock":{"dummy":""}}`. `t8426` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8426Request {
    #[serde(rename = "t8426InBlock")]
    pub inblock: T8426InBlock,
}

impl T8426Request {
    /// Build a `t8426` commodity-futures master request. Takes no caller input;
    /// the `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8426Request {
            inblock: T8426InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8426Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8426OutBlock` — one commodity-futures master row.
///
/// The data-bearing repeated block (`t8426OutBlock[]`, confirmed from the raw
/// capture's `res_example` array). `hname` (종목명, the commodity-futures
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; the remaining fields are the contract codes. `shcode`
/// (단축코드) uses [`ls_core::string_or_number`] for wire-type tolerance (the
/// gateway may send a numeric-looking code as a JSON number);
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8426OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
}

/// `t8426` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// commodity-futures master row array under the `t8426OutBlock` key, tolerated
/// as a single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8426Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8426OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8426OutBlock>,
}

// ---------------------------------------------------------------------------
// t8433 — 지수옵션마스터조회API용 (index-option master). market_session,
// non-paginated. A no-caller-input read: the spec's `t8433InBlock` carries a
// single length-1 `dummy` placeholder, so callers supply nothing. The response
// is a single out-block `t8433OutBlock` that is itself the data-bearing ROW
// ARRAY (the raw capture's `res_example` shows `"t8433OutBlock": [ {…}, … ]`,
// rows direct under the key, no numbered sub-block) — one index-option contract
// per row. There is no separate count header. Modeled after `T8426` (single
// row-array out-block).
// ---------------------------------------------------------------------------

/// Input block for `t8433` — a no-caller-input read.
///
/// The spec's `t8433InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8426InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T8433InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8433` request — wraps the input block under the `t8433InBlock` key.
///
/// Serializes to `{"t8433InBlock":{"dummy":""}}`. `t8433` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8433Request {
    #[serde(rename = "t8433InBlock")]
    pub inblock: T8433InBlock,
}

impl T8433Request {
    /// Build a `t8433` index-option master request. Takes no caller input; the
    /// `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8433Request {
            inblock: T8433InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8433Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8433OutBlock` — one index-option master row.
///
/// The data-bearing repeated block (`t8433OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t8433OutBlock` key). `hname` (종목명, the index-option contract name) is the
/// canonical identity field, resolved by its `korean_name` from the baseline;
/// `shcode`/`expcode` are the contract codes, and the price fields are the
/// daily limit/close references. `shcode` and the `Number`-typed price fields
/// use [`ls_core::string_or_number`] for wire-type tolerance (the gateway sends
/// these as JSON strings in the capture but may send numbers);
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8433OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Upper limit price / 상한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hprice: String,
    /// Lower limit price / 하한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lprice: String,
    /// Previous-day close / 전일종가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Reference price / 기준가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
}

/// `t8433` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-option master row array under the `t8433OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8433Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8433OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8433OutBlock>,
}

/// Market-session operations, backed by the shared runtime core.
///
/// Cheap to clone — shares `Arc<Inner>` (and therefore the token cache and rate
/// limiter) with the rest of the SDK.
#[derive(Clone)]
pub struct MarketSession {
    inner: Arc<Inner>,
}

impl MarketSession {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        MarketSession { inner }
    }

    /// Fetch the current-price (시세) snapshot for one symbol via `t1102`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t1102` is not paginated, so this is a single,
    /// non-continuation POST. A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn quote(&self, req: &T1102Request) -> LsResult<T1102Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1102_POLICY, req)
            .await
    }

    /// Fetch the current-price + order-book (호가) snapshot for one symbol via
    /// `t1101`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t1101` is not paginated, so this is a single,
    /// non-continuation POST. A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn order_book(&self, req: &T1101Request) -> LsResult<T1101Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1101_POLICY, req)
            .await
    }

    /// Fetch the full theme list (전체테마) via `t8425`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t8425` is not paginated and takes no caller input, so
    /// this is a single, non-continuation POST returning every theme's
    /// name + code. The returned `tmcode` values are the representative caller
    /// inputs for theme-keyed reads (`t1531`/`t1537`). A `01900` business code
    /// surfaces as [`ls_core::LsError::ApiError`] and classifies as
    /// paper-incompatible.
    pub async fn all_themes(&self, req: &T8425Request) -> LsResult<T8425Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8425_POLICY, req)
            .await
    }

    /// Fetch the stock master list (주식종목조회) for one market segment via
    /// `t8436`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t8436` is not paginated; `gubun` is a market-segment
    /// filter (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ), not an instrument
    /// identifier. A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn stock_list(&self, req: &T8436Request) -> LsResult<T8436Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8436_POLICY, req)
            .await
    }

    /// Fetch the constituent stocks of one theme (테마별종목) via `t1531`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). The theme is
    /// identified by a matched `tmname`+`tmcode` pair (both required by the spec);
    /// source one from [`MarketSession::all_themes`].
    pub async fn theme_stocks(&self, req: &T1531Request) -> LsResult<T1531Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1531_POLICY, req)
            .await
    }

    /// Fetch per-stock quotes for one theme (테마종목별시세조회) via `t1537`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). Keyed by
    /// `tmcode`; the response carries a theme summary plus a per-stock quote array.
    pub async fn theme_quotes(&self, req: &T1537Request) -> LsResult<T1537Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1537_POLICY, req)
            .await
    }

    /// Run a server-saved condition search (서버저장조건 조건검색) via `t1859`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). Keyed by a
    /// `query_index` produced by `t1866` ([`crate::paginated::Paginated::saved_conditions`]);
    /// the response carries a search summary plus the matched-issue array. A
    /// `01900` business code surfaces as [`ls_core::LsError::ApiError`] and
    /// classifies as paper-incompatible.
    pub async fn condition_search(&self, req: &T1859Request) -> LsResult<T1859Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1859_POLICY, req)
            .await
    }

    /// List the available ThinQ Q-click searches (종목Q클릭검색리스트조회) via
    /// `t1826` — the Wave 3 producer.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). `search_gb`
    /// selects the search catalog (`"0"` 핵심검색 being representative); the
    /// response carries the `search_cd` catalog keys consumed by `t1825`
    /// ([`MarketSession::qclick_search`]). A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn qclick_search_list(&self, req: &T1826Request) -> LsResult<T1826Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1826_POLICY, req)
            .await
    }

    /// Run one ThinQ Q-click search (종목Q클릭검색) via `t1825` — the Wave 3
    /// consumer.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). Keyed by a
    /// `search_cd` produced by `t1826` ([`MarketSession::qclick_search_list`]);
    /// the response carries a search summary plus the matched-issue array. A
    /// `01900` business code surfaces as [`ls_core::LsError::ApiError`] and
    /// classifies as paper-incompatible.
    pub async fn qclick_search(&self, req: &T1825Request) -> LsResult<T1825Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1825_POLICY, req)
            .await
    }

    /// List the full underlying-asset universe (기초자산리스트조회) via `t9905`.
    ///
    /// Non-paginated, no caller input. The returned `shcode` values are the
    /// underlying-asset codes consumed by `t1964` (`item`).
    pub async fn underlying_list(&self, req: &T9905Request) -> LsResult<T9905Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9905_POLICY, req)
            .await
    }

    /// List the ELW expiry months (만기월조회) via `t9907`. Non-paginated, no
    /// caller input.
    pub async fn elw_expiry_months(&self, req: &T9907Request) -> LsResult<T9907Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9907_POLICY, req)
            .await
    }

    /// List the ELW symbol universe (ELW종목조회) via `t8431` — the Wave 1 spine
    /// producer. Non-paginated, no caller input; the returned `shcode` values are
    /// the ELW codes consumed by `t1958` ([`MarketSession::elw_compare`]).
    pub async fn elw_symbols(&self, req: &T8431Request) -> LsResult<T8431Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8431_POLICY, req)
            .await
    }

    /// List the ELW master universe (ELW마스터조회) via `t9942`. Non-paginated,
    /// no caller input.
    pub async fn elw_master(&self, req: &T9942Request) -> LsResult<T9942Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9942_POLICY, req)
            .await
    }

    /// Compare two ELW symbols (ELW종목비교) via `t1958`. Non-paginated; keyed by
    /// two `shcode`s sourced from `t8431` ([`MarketSession::elw_symbols`]); the
    /// response carries each symbol's detail plus a comparison block.
    pub async fn elw_compare(&self, req: &T1958Request) -> LsResult<T1958Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1958_POLICY, req)
            .await
    }

    /// Read the ELW board (ELW전광판) for one underlying via `t1964`.
    /// Non-paginated; keyed by an `item` underlying-asset code sourced from
    /// `t9905` ([`MarketSession::underlying_list`]), with broad/default filters.
    pub async fn elw_board(&self, req: &T1964Request) -> LsResult<T1964Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1964_POLICY, req)
            .await
    }

    /// Read the investor-by-type aggregate (투자자별종합) via `t1601`. Non-paginated.
    pub async fn investor_aggregate(&self, req: &T1601Request) -> LsResult<T1601Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1601_POLICY, req)
            .await
    }

    /// Read the investor trading aggregate (투자자매매종합1) via `t1615`.
    /// Non-paginated.
    pub async fn investor_trading(&self, req: &T1615Request) -> LsResult<T1615Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1615_POLICY, req)
            .await
    }

    /// Read the program-trading aggregate (프로그램매매종합, mini) via `t1640`.
    /// Non-paginated.
    pub async fn program_aggregate(&self, req: &T1640Request) -> LsResult<T1640Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1640_POLICY, req)
            .await
    }

    /// Read the by-time program-trading chart (시간대별프로그램매매추이) via `t1662`.
    /// Non-paginated.
    pub async fn program_chart(&self, req: &T1662Request) -> LsResult<T1662Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1662_POLICY, req)
            .await
    }

    /// Read the investor trading chart (투자자매매종합 챠트) via `t1664`.
    /// Non-paginated.
    pub async fn investor_chart(&self, req: &T1664Request) -> LsResult<T1664Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1664_POLICY, req)
            .await
    }

    /// List every sector (전체업종) via `t8424`. Non-paginated; the anchor and
    /// `upcode` source for the sector cluster.
    pub async fn sectors(&self, req: &T8424Request) -> LsResult<T8424Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8424_POLICY, req)
            .await
    }

    /// Read one sector's index snapshot (업종현재가) via `t1511`. Non-paginated.
    pub async fn sector_quote(&self, req: &T1511Request) -> LsResult<T1511Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1511_POLICY, req)
            .await
    }

    /// Read one sector's expected/auction index (예상지수) via `t1485`.
    /// Non-paginated.
    pub async fn sector_expected_index(&self, req: &T1485Request) -> LsResult<T1485Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1485_POLICY, req)
            .await
    }

    /// Read the per-sector stock board (업종별종목시세) via `t1516`. Non-paginated;
    /// needs both `upcode` and a `shcode` ticker.
    pub async fn sector_stocks(&self, req: &T1516Request) -> LsResult<T1516Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1516_POLICY, req)
            .await
    }

    /// Read the option board (옵션전광판) via `t2301`. Non-paginated; keyed by a
    /// contract month `yyyymm` (월물) and a `gubun` mini/regular selector.
    pub async fn option_board(&self, req: &T2301Request) -> LsResult<T2301Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2301_POLICY, req)
            .await
    }

    /// Read the stock-futures underlying-asset master (주식선물기초자산조회) via
    /// `t2522`. Non-paginated, no caller input; returns the underlying-asset
    /// header (name + codes).
    pub async fn stock_futures_underlying(
        &self,
        req: &T2522Request,
    ) -> LsResult<T2522Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2522_POLICY, req)
            .await
    }

    /// Read the stock-futures master (주식선물마스터조회) via `t8401`.
    /// Non-paginated, no caller input; returns the stock-futures contract rows
    /// (name + codes).
    pub async fn stock_futures_master(&self, req: &T8401Request) -> LsResult<T8401Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8401_POLICY, req)
            .await
    }

    /// Read the commodity-futures master (상품선물마스터조회) via `t8426`.
    /// Non-paginated, no caller input; returns the commodity-futures contract
    /// rows (name + codes).
    pub async fn commodity_futures_master(
        &self,
        req: &T8426Request,
    ) -> LsResult<T8426Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8426_POLICY, req)
            .await
    }

    /// Read the index-option master (지수옵션마스터조회) via `t8433`.
    /// Non-paginated, no caller input; returns the index-option contract rows
    /// (name + codes + daily limit/close references).
    pub async fn index_option_master(
        &self,
        req: &T8433Request,
    ) -> LsResult<T8433Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8433_POLICY, req)
            .await
    }
}
