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
}
