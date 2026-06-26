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

// ---------------------------------------------------------------------------
// t1901 — ETF현재가(시세)조회 (ETF current-price snapshot). market_session read,
// single OutBlock object; path /stock/etf. Mirrors t1102's single-object shape.
// ---------------------------------------------------------------------------

/// Input block for `t1901` — the ETF short code (단축코드). `shcode`-only.
#[derive(Serialize, Debug, Clone)]
pub struct T1901InBlock {
    /// Short code / 단축코드 (e.g. `"069500"` KODEX 200).
    pub shcode: String,
}

/// `t1901` request — serializes to `{"t1901InBlock":{"shcode":...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1901Request {
    #[serde(rename = "t1901InBlock")]
    pub inblock: T1901InBlock,
}

impl T1901Request {
    /// Build a `t1901` ETF quote request for one short code.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1901Request {
            inblock: T1901InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1901OutBlock` — the ETF snapshot quote (a representative, spec-grounded subset
/// of the LS `t1901OutBlock`). Numeric-bearing fields use [`ls_core::string_or_number`]
/// (the gateway sends numbers or strings); `#[serde(default)]` lets a sparse out-block
/// deserialize, and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1901OutBlock {
    /// Korean name / 한글 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비 구분.
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
    /// Reference (base) price / 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
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
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Trading value / 누적거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t1901` response envelope — the ETF snapshot under the `t1901OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1901Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1901OutBlock", default)]
    pub outblock: T1901OutBlock,
}

// ---------------------------------------------------------------------------
// t1105 — 주식피봇/디마크조회 (pivot / demark levels). market_session, single
// OutBlock; path /stock/market-data. shcode + exchgubun request.
// ---------------------------------------------------------------------------

/// Input block for `t1105` — short code + exchange distinction.
#[derive(Serialize, Debug, Clone)]
pub struct T1105InBlock {
    pub shcode: String,
    pub exchgubun: String,
}

/// `t1105` request — `{"t1105InBlock":{"shcode":...,"exchgubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1105Request {
    #[serde(rename = "t1105InBlock")]
    pub inblock: T1105InBlock,
}

impl T1105Request {
    /// Build a `t1105` pivot/demark request for one symbol on one exchange.
    pub fn new(shcode: impl Into<String>, exchgubun: impl Into<String>) -> Self {
        T1105Request {
            inblock: T1105InBlock {
                shcode: shcode.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1105OutBlock` — pivot + demark levels (single object). Numeric-bearing fields
/// via [`ls_core::string_or_number`]; `#[serde(default)]` tolerates a sparse block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1105OutBlock {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Pivot / 피봇.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pbot: String,
    /// Pivot 1st resistance / 1차 매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer1: String,
    /// Pivot 1st support / 1차 매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub supp1: String,
    /// Pivot 2nd resistance / 2차 매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer2: String,
    /// Pivot 2nd support / 2차 매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub supp2: String,
    /// Demark standard price / 디마크 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub stdprc: String,
    /// Demark resistance / 디마크 매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerd: String,
    /// Demark support / 디마크 매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub suppd: String,
}

/// `t1105` response envelope — the pivot/demark block under `t1105OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1105Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1105OutBlock", default)]
    pub outblock: T1105OutBlock,
}

// ---------------------------------------------------------------------------
// t1104 — 주식현재가시세메모 (current-price memo). market_session; a summary
// OutBlock plus a memo-row array OutBlock1; path /stock/market-data.
// ---------------------------------------------------------------------------

/// Input block for `t1104` — short code (`code`), row count (`nrec`), exchange.
#[derive(Serialize, Debug, Clone)]
pub struct T1104InBlock {
    pub code: String,
    pub nrec: String,
    pub exchgubun: String,
}

/// `t1104` request — `{"t1104InBlock":{"code":...,"nrec":...,"exchgubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1104Request {
    #[serde(rename = "t1104InBlock")]
    pub inblock: T1104InBlock,
}

impl T1104Request {
    /// Build a `t1104` price-memo request for one symbol on one exchange.
    pub fn new(
        code: impl Into<String>,
        nrec: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1104Request {
            inblock: T1104InBlock {
                code: code.into(),
                nrec: nrec.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1104OutBlock` — the summary block (record count).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1104OutBlock {
    /// Record count / 레코드 수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nrec: String,
}

/// `t1104OutBlock1` — one memo row (index / kind / value).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1104OutBlock1 {
    /// Index / 인덱스.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indx: String,
    /// Kind / 구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubn: String,
    /// Value / 값.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vals: String,
}

/// `t1104` response envelope — summary `t1104OutBlock` + memo-row array
/// `t1104OutBlock1` (tolerated single-or-array via [`ls_core::de_vec_or_single`]).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1104Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1104OutBlock", default)]
    pub outblock: T1104OutBlock,
    #[serde(
        rename = "t1104OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1104OutBlock1>,
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

/// Input block for `t8430` — 주식종목조회 (full stock-issue list). `gubun` selects
/// the market: "0" all, "1" KOSPI, "2" KOSDAQ. The full-list read sends "0".
/// `gubun` is a code string ("0"/"1"/"2"), not numeric — no `string_as_number`.
#[derive(Serialize, Debug, Clone)]
pub struct T8430InBlock {
    /// Market filter / 구분 ("0":전체 "1":코스피 "2":코스닥).
    pub gubun: String,
}

/// `t8430` request — serializes to `{"t8430InBlock":{"gubun":"0"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8430Request {
    #[serde(rename = "t8430InBlock")]
    pub inblock: T8430InBlock,
}
impl T8430Request {
    /// Build a `t8430` stock-issue-list request for a market filter
    /// ("0":전체 "1":코스피 "2":코스닥).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8430Request {
            inblock: T8430InBlock {
                gubun: gubun.into(),
            },
        }
    }
    /// Build a `t8430` request for every market ("0":전체).
    pub fn all() -> Self {
        Self::new("0")
    }
}
impl Default for T8430Request {
    fn default() -> Self {
        Self::all()
    }
}

/// `t8430OutBlock` — one stock-issue row. Numeric-bearing fields via
/// [`ls_core::string_or_number`] (the gateway mixes string and number forms).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8430OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// ETF flag / ETF구분 ("1":ETF).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etfgubun: String,
    /// Upper-limit price / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower-limit price / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous-day close / 전일가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Order-quantity unit / 주문수량단위.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub memedan: String,
    /// Reference price / 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
    /// Market flag / 구분 ("1":코스피 "2":코스닥).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// `t8430` response — the stock-issue array under `t8430OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8430Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8430OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8430OutBlock>,
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
// per row. There is no separate count header. The row is modeled after the
// 9-field `T8435` row-array out-block (T8426 has only 3 fields; the index-option
// row carries the daily limit/close reference prices too).
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
/// `t8433OutBlock` key). A representative, spec-grounded subset modeled after the
/// 9-field [`T8435OutBlock`] row. `hname` (종목명, the index-option contract
/// name) is the canonical identity field, resolved by its `korean_name` from the
/// baseline; `shcode`/`expcode` are the contract codes, and the price fields are
/// the daily limit/close references (상한가/하한가/전일종가/전일고가/전일저가/
/// 기준가). `shcode` and the `Number`-typed price fields use
/// [`ls_core::string_or_number`] for wire-type tolerance (the gateway sends these
/// as JSON strings in the capture but may send numbers); `#[serde(default)]` lets
/// a sparse row deserialize cleanly. Field names mirror the LS spec verbatim.
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
    /// Previous-day high / 전일고가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilhigh: String,
    /// Previous-day low / 전일저가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnillow: String,
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

// ---------------------------------------------------------------------------
// t8435 — 파생종목마스터조회API용 (derivatives master). market_session,
// non-paginated. Keyed by a `gubun` (구분) selector — the LS spec defines these
// as the MINI/weekly segments: `"MF"` 미니선물 (mini futures) / `"MO"` 미니옵션
// (mini options) / `"WK"` 코스피200위클리옵션 / `"SF"` 코스닥150선물 / `"QW"`
// 코스닥150위클리옵션. The response out-block `t8435OutBlock` is itself a ROW
// ARRAY (the raw capture's `res_example` shows `"t8435OutBlock": [ {…}, … ]`,
// one derivatives contract per row, no numbered sub-block — the normalized
// baseline collapses the block, so the true wire shape is read from the raw
// capture per KTD3) — each row carries the contract name + codes plus the daily
// limit/close reference prices. Modeled after `T8433` (single row-array
// out-block) but with a caller `gubun` selector.
// ---------------------------------------------------------------------------

/// Input block for `t8435` — the derivatives-segment selector.
///
/// `gubun` (구분) selects the derivatives segment. The LS spec defines these as
/// the MINI/weekly segments: `"MF"` 미니선물 (mini futures) / `"MO"` 미니옵션
/// (mini options) / `"WK"` 코스피200위클리옵션 (KOSPI200 weekly options) /
/// `"SF"` 코스닥150선물 (KOSDAQ150 futures) / `"QW"` 코스닥150위클리옵션
/// (KOSDAQ150 weekly options). The spec types it `String` (length 2).
/// Caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T8435InBlock {
    /// Segment selector / 구분 (`"MF"` mini futures / `"MO"` mini options /
    /// `"WK"`/`"SF"`/`"QW"` weekly/KOSDAQ150 segments).
    pub gubun: String,
}

/// `t8435` request — wraps the input block under the `t8435InBlock` key.
///
/// Serializes to `{"t8435InBlock":{"gubun":"MF"}}`. `t8435` is not paginated, so
/// there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8435Request {
    #[serde(rename = "t8435InBlock")]
    pub inblock: T8435InBlock,
}

impl T8435Request {
    /// Build a `t8435` derivatives-master request for one segment (`gubun`:
    /// `"MF"` mini futures / `"MO"` mini options / `"WK"`/`"SF"`/`"QW"` weekly/
    /// KOSDAQ150 segments).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8435Request {
            inblock: T8435InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8435OutBlock` — one derivatives-master row.
///
/// The data-bearing repeated block (`t8435OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t8435OutBlock` key). The full 9 fields. `hname` (종목명, the derivatives
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode`/`expcode` are the contract codes, and the
/// `Number`-typed `uplmtprice`/`dnlmtprice`/`jnilclose`/`jnilhigh`/`jnillow`/
/// `recprice` fields are the daily limit/close reference prices. The
/// numeric-bearing fields use [`ls_core::string_or_number`] for wire-type
/// tolerance (the gateway may send a `Number` field as a JSON number);
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8435OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Upper limit price / 상한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous-day close / 전일종가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Previous-day high / 전일고가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilhigh: String,
    /// Previous-day low / 전일저가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnillow: String,
    /// Reference price / 기준가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
}

/// `t8435` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// derivatives-master row array under the `t8435OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8435Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8435OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8435OutBlock>,
}

// ---------------------------------------------------------------------------
// t8467 — 지수선물마스터조회API용 (index-futures master). market_session,
// non-paginated. Keyed by a `gubun` (구분) segment selector — `"V"` 변동성지수선물
// (volatility-index futures) / `"S"` 섹터지수선물 (sector-index futures) / `"Q"`
// 코스닥150지수선물 (KOSDAQ150-index futures) / any other value → 코스피200지수선물
// (KOSPI200-index futures, the default). The response out-block `t8467OutBlock`
// is itself a ROW ARRAY (the raw capture's `res_example` shows
// `"t8467OutBlock": [ {…}, … ]`, propertyType `A0005`/Object Array, one
// index-futures contract per row — the normalized baseline lists the row fields
// flat under the block name, so the true wire shape is read from the raw capture
// per KTD3). Each row carries the contract name + codes plus the daily
// limit/close reference prices. Modeled identically to `T8435` (single row-array
// out-block, the same 9 fields) but with the index-futures `gubun` selector.
// ---------------------------------------------------------------------------

/// Input block for `t8467` — the index-futures segment selector.
///
/// `gubun` (구분) selects the index-futures segment: `"V"` 변동성지수선물 / `"S"`
/// 섹터지수선물 / `"Q"` 코스닥150지수선물 / any other value → 코스피200지수선물
/// (the default). The spec types it `String` (length 1). Caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T8467InBlock {
    /// Segment selector / 구분 (`"V"`/`"S"`/`"Q"` or default → KOSPI200).
    pub gubun: String,
}

/// `t8467` request — wraps the input block under the `t8467InBlock` key.
///
/// Serializes to `{"t8467InBlock":{"gubun":"Q"}}`. `t8467` is not paginated, so
/// there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8467Request {
    #[serde(rename = "t8467InBlock")]
    pub inblock: T8467InBlock,
}

impl T8467Request {
    /// Build a `t8467` index-futures-master request for one segment (`gubun`:
    /// `"V"`/`"S"`/`"Q"` or any other value → KOSPI200-index futures).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8467Request {
            inblock: T8467InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8467OutBlock` — one index-futures-master row.
///
/// The data-bearing repeated block (`t8467OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t8467OutBlock` key). The full 9 fields. `hname` (종목명, the index-futures
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode`/`expcode` are the contract codes, and the
/// `Number`-typed `uplmtprice`/`dnlmtprice`/`jnilclose`/`jnilhigh`/`jnillow`/
/// `recprice` fields are the daily limit/close reference prices. The
/// numeric-bearing fields use [`ls_core::string_or_number`] for wire-type
/// tolerance (the gateway may send a `Number` field as a JSON number);
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8467OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Upper limit price / 상한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous-day close / 전일종가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Previous-day high / 전일고가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilhigh: String,
    /// Previous-day low / 전일저가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnillow: String,
    /// Reference price / 기준가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
}

/// `t8467` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-futures-master row array under the `t8467OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8467Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8467OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8467OutBlock>,
}

// ---------------------------------------------------------------------------
// t9943 — 지수선물마스터조회API용 (index-futures master). market_session,
// non-paginated. Keyed by a `gubun` (구분) segment selector — `"V"` 변동성지수선물
// (volatility-index futures) / `"S"` 섹터지수선물 (sector-index futures) / any
// other value → 코스피200지수선물 (KOSPI200-index futures, the default). The
// response out-block `t9943OutBlock` is itself a ROW ARRAY: the raw capture's
// `res_example` shows `"t9943OutBlock": [ {…}, … ]` (propertyType `A0005`/Object
// Array), each row a direct element carrying the contract name + codes — the
// normalized baseline collapses the block name to `response_body`, so the true
// wire out-block key `t9943OutBlock` is read from the raw capture per KTD3.
// Modeled after `T8467` (same 지수선물마스터 read, the same `gubun` selector) but
// the spec lists only the 3 identity fields (`hname`/`shcode`/`expcode`), no
// daily limit/close reference prices.
// ---------------------------------------------------------------------------

/// Input block for `t9943` — the index-futures segment selector.
///
/// `gubun` (구분) selects the index-futures segment: `"V"` 변동성지수선물 / `"S"`
/// 섹터지수선물 / any other value → 코스피200지수선물 (the default). The spec types
/// it `String` (length 1). Caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T9943InBlock {
    /// Segment selector / 구분 (`"V"`/`"S"` or default → KOSPI200).
    pub gubun: String,
}

/// `t9943` request — wraps the input block under the `t9943InBlock` key.
///
/// Serializes to `{"t9943InBlock":{"gubun":"V"}}`. `t9943` is not paginated, so
/// there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T9943Request {
    #[serde(rename = "t9943InBlock")]
    pub inblock: T9943InBlock,
}

impl T9943Request {
    /// Build a `t9943` index-futures-master request for one segment (`gubun`:
    /// `"V"`/`"S"` or any other value → KOSPI200-index futures).
    pub fn new(gubun: impl Into<String>) -> Self {
        T9943Request {
            inblock: T9943InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t9943OutBlock` — one index-futures-master row.
///
/// The data-bearing repeated block (`t9943OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t9943OutBlock` key). The 3 spec fields. `hname` (종목명, the index-futures
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode` (단축코드) / `expcode` (확장코드) are the contract
/// codes. `shcode` uses [`ls_core::string_or_number`] for wire-type tolerance
/// (the gateway may send a code field as a JSON number); `#[serde(default)]` lets
/// a sparse row deserialize cleanly. Field names mirror the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9943OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
}

/// `t9943` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-futures-master row array under the `t9943OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9943Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9943OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9943OutBlock>,
}

// ---------------------------------------------------------------------------
// t9944 — 지수옵션마스터조회API용 (index-option master). market_session,
// non-paginated. A no-caller-input read: the spec's `t9944InBlock` carries a
// single length-1 `dummy` placeholder, so callers supply nothing. The response
// out-block `t9944OutBlock` is itself a ROW ARRAY: the raw capture's
// `res_example` shows `"t9944OutBlock": [ {…}, … ]` (propertyType Object Array),
// each row a direct element carrying the contract name + codes — the normalized
// baseline collapses the block name to `response_body`, so the true wire
// out-block key `t9944OutBlock` is read from the raw capture per KTD3. Modeled
// after `T8426`/`T9943` (same dummy-input row-array master read); the spec lists
// only the 3 identity fields (`hname`/`shcode`/`expcode`).
// ---------------------------------------------------------------------------

/// Input block for `t9944` — a no-caller-input read.
///
/// The spec's `t9944InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8426InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T9944InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9944` request — wraps the input block under the `t9944InBlock` key.
///
/// Serializes to `{"t9944InBlock":{"dummy":""}}`. `t9944` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T9944Request {
    #[serde(rename = "t9944InBlock")]
    pub inblock: T9944InBlock,
}

impl T9944Request {
    /// Build a `t9944` index-option master request. Takes no caller input; the
    /// `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T9944Request {
            inblock: T9944InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T9944Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9944OutBlock` — one index-option master row.
///
/// The data-bearing repeated block (`t9944OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t9944OutBlock` key). The 3 spec fields. `hname` (종목명, the index-option
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode` (단축코드) / `expcode` (확장코드) are the contract
/// codes. `shcode` uses [`ls_core::string_or_number`] for wire-type tolerance
/// (the gateway may send a code field as a JSON number); `#[serde(default)]` lets
/// a sparse row deserialize cleanly. Field names mirror the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9944OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
}

/// `t9944` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-option master row array under the `t9944OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9944Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9944OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9944OutBlock>,
}

// ---------------------------------------------------------------------------
// U5 (reach wave) — F/O quote/master reads. All `/futureoption/market-data`,
// `[선물/옵션] 시세`, non-paginated market_session. Out-block keys + array-ness
// read from the RAW capture (KTD5): t2111/t2112/t8402/t8403 carry a SINGLE
// out-block; t2106 carries a single summary + an ARRAY detail block; t8434
// carries an ARRAY out-block (`t8434OutBlock1`). t8434's `qrycnt` is a numeric
// REQUEST field serialized as a JSON number (`string_as_number`, KTD4).
// ---------------------------------------------------------------------------

/// Input block for `t2111` — 선물/옵션현재가(시세)조회 (F/O current-price quote).
///
/// `focode` is the futures/option contract short code (단축코드), a
/// caller-supplied identifier sourced from an F/O master (e.g.
/// [`MarketSession::index_futures_master`]'s `shcode`).
#[derive(Serialize, Debug, Clone)]
pub struct T2111InBlock {
    /// Short code / 단축코드 (F/O contract code).
    pub focode: String,
}

/// `t2111` request — serializes to `{"t2111InBlock":{"focode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T2111Request {
    #[serde(rename = "t2111InBlock")]
    pub inblock: T2111InBlock,
}
impl T2111Request {
    /// Build a `t2111` F/O current-price request for one contract code.
    pub fn new(focode: impl Into<String>) -> Self {
        T2111Request {
            inblock: T2111InBlock {
                focode: focode.into(),
            },
        }
    }
}

/// `t2111OutBlock` — the F/O current-price snapshot (single object).
///
/// A representative, spec-grounded subset of the `t2111OutBlock`; every
/// numeric-bearing field uses [`ls_core::string_or_number`]. `pricejisu`
/// (종합지수) and `kospijisu` (KOSPI200지수) are modeled as DISTINCT index fields
/// (not collapsed) so a fixture can pin each separately (KTD6). All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2111OutBlock {
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
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mgjv: String,
    /// Composite index / 종합지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// KOSPI200 index / KOSPI200지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub kospijisu: String,
    /// Contract code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub focode: String,
}

/// `t2111` response envelope. `outblock` is the snapshot under the
/// `t2111OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2111Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2111OutBlock", default)]
    pub outblock: T2111OutBlock,
}

/// Input block for `t2112` — 선물/옵션현재가호가조회 (F/O current-price order book).
///
/// `shcode` is the F/O contract short code (단축코드), a caller-supplied
/// identifier sourced from an F/O master.
#[derive(Serialize, Debug, Clone)]
pub struct T2112InBlock {
    /// Short code / 단축코드 (F/O contract code).
    pub shcode: String,
}

/// `t2112` request — serializes to `{"t2112InBlock":{"shcode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T2112Request {
    #[serde(rename = "t2112InBlock")]
    pub inblock: T2112InBlock,
}
impl T2112Request {
    /// Build a `t2112` F/O order-book request for one contract code.
    pub fn new(shcode: impl Into<String>) -> Self {
        T2112Request {
            inblock: T2112InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t2112OutBlock` — the F/O current-price + 5-level order book (single object).
///
/// A representative subset of the `t2112OutBlock`: the price header plus the
/// level-1 bid/offer aggregates. Every numeric-bearing field uses
/// [`ls_core::string_or_number`]; all `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2112OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best offer (ask) / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Best offer quantity / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Best bid quantity / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// `t2112` response envelope. `outblock` is the order book under the
/// `t2112OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2112Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2112OutBlock", default)]
    pub outblock: T2112OutBlock,
}

/// Input block for `t2106` — 선물/옵션현재가시세메모 (F/O price-memo read).
///
/// `code` is the F/O contract code (종목코드); `nrec` is the requested memo
/// count (건수). The spec's `t2106InBlock` carries `code` + `nrec`; the optional
/// `t2106InBlock1` condition array is not modeled (the read is keyed by `code`).
#[derive(Serialize, Debug, Clone)]
pub struct T2106InBlock {
    /// Contract code / 종목코드 (F/O contract code).
    pub code: String,
    /// Requested count / 건수 (empty = default).
    pub nrec: String,
}

/// `t2106` request — serializes to `{"t2106InBlock":{"code":...,"nrec":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T2106Request {
    #[serde(rename = "t2106InBlock")]
    pub inblock: T2106InBlock,
}
impl T2106Request {
    /// Build a `t2106` price-memo request for one contract code (`nrec` defaults
    /// to empty — the gateway returns the default memo set).
    pub fn new(code: impl Into<String>) -> Self {
        T2106Request {
            inblock: T2106InBlock {
                code: code.into(),
                nrec: String::new(),
            },
        }
    }
}

/// `t2106OutBlock` — the price-memo summary block (single object).
///
/// `nrec` (출력건수) is the modeled non-key signal. Via
/// [`ls_core::string_or_number`]; `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2106OutBlock {
    /// Output count / 출력건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nrec: String,
}

/// `t2106OutBlock1` — one price-memo row (`t2106OutBlock1[]`, an ARRAY block).
///
/// The repeated detail block (the spec marks `t2106OutBlock1` an array); each
/// row is index/condition/value. Every field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2106OutBlock1 {
    /// Index / 인덱스.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indx: String,
    /// Condition distinction / 조건구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubn: String,
    /// Output value / 출력값.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vals: String,
}

/// `t2106` response envelope.
///
/// `outblock` is the memo summary; `outblock1` is the memo-row array under the
/// `t2106OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2106Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2106OutBlock", default)]
    pub outblock: T2106OutBlock,
    #[serde(
        rename = "t2106OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2106OutBlock1>,
}

/// Input block for `t8402` — 주식선물현재가조회(API용) (stock-futures current price).
///
/// `focode` is the stock-futures contract short code (단축코드), a
/// caller-supplied identifier sourced from the stock-futures master
/// ([`MarketSession::stock_futures_master`]'s `shcode`).
#[derive(Serialize, Debug, Clone)]
pub struct T8402InBlock {
    /// Short code / 단축코드 (stock-futures contract code).
    pub focode: String,
}

/// `t8402` request — serializes to `{"t8402InBlock":{"focode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8402Request {
    #[serde(rename = "t8402InBlock")]
    pub inblock: T8402InBlock,
}
impl T8402Request {
    /// Build a `t8402` stock-futures current-price request for one contract code.
    pub fn new(focode: impl Into<String>) -> Self {
        T8402Request {
            inblock: T8402InBlock {
                focode: focode.into(),
            },
        }
    }
}

/// `t8402OutBlock` — the stock-futures current-price snapshot (single object).
///
/// A representative subset; every numeric field via
/// [`ls_core::string_or_number`]. `basehname` (기초자산한글명) is a DISTINCT
/// underlying-name string modeled separately from the futures `hname` so a
/// fixture can pin each (KTD6). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8402OutBlock {
    /// Korean name / 한글명 (the stock-futures contract name).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mgjv: String,
    /// Underlying short code / 기초자산단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Underlying Korean name / 기초자산한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub basehname: String,
    /// Underlying current price / 기초자산현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub baseprice: String,
}

/// `t8402` response envelope. `outblock` is the snapshot under the
/// `t8402OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8402Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8402OutBlock", default)]
    pub outblock: T8402OutBlock,
}

/// Input block for `t8403` — 주식선물호가조회(API용) (stock-futures order book).
///
/// `shcode` is the stock-futures contract short code (단축코드), a
/// caller-supplied identifier sourced from the stock-futures master.
#[derive(Serialize, Debug, Clone)]
pub struct T8403InBlock {
    /// Short code / 단축코드 (stock-futures contract code).
    pub shcode: String,
}

/// `t8403` request — serializes to `{"t8403InBlock":{"shcode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8403Request {
    #[serde(rename = "t8403InBlock")]
    pub inblock: T8403InBlock,
}
impl T8403Request {
    /// Build a `t8403` stock-futures order-book request for one contract code.
    pub fn new(shcode: impl Into<String>) -> Self {
        T8403Request {
            inblock: T8403InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t8403OutBlock` — the stock-futures current-price + 10-level order book
/// (single object).
///
/// A representative subset: the price header plus the level-1 bid/offer
/// aggregates. Every numeric-bearing field via [`ls_core::string_or_number`];
/// all `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8403OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best offer (ask) / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Best offer quantity / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Best bid quantity / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// `t8403` response envelope. `outblock` is the order book under the
/// `t8403OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8403Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8403OutBlock", default)]
    pub outblock: T8403OutBlock,
}

/// Input block for `t8434` — 선물/옵션멀티현재가조회 (F/O multi current-price).
///
/// `qrycnt` is the requested contract COUNT (건수), a numeric REQUEST field
/// serialized as a JSON number via [`ls_core::string_as_number`] (KTD4 — the
/// string form risks `IGW40011`). `focode` is a comma-joined list of F/O
/// contract codes (단축코드, up to length 400).
#[derive(Serialize, Debug, Clone)]
pub struct T8434InBlock {
    /// Requested count / 건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Short code(s) / 단축코드 (one or more F/O contract codes).
    pub focode: String,
}

/// `t8434` request — serializes to `{"t8434InBlock":{"qrycnt":1,"focode":...}}`
/// (`qrycnt` as a JSON number).
#[derive(Serialize, Debug, Clone)]
pub struct T8434Request {
    #[serde(rename = "t8434InBlock")]
    pub inblock: T8434InBlock,
}
impl T8434Request {
    /// Build a `t8434` multi current-price request for `qrycnt` contracts keyed
    /// by `focode` (a single code or a comma-joined list).
    pub fn new(qrycnt: impl Into<String>, focode: impl Into<String>) -> Self {
        T8434Request {
            inblock: T8434InBlock {
                qrycnt: qrycnt.into(),
                focode: focode.into(),
            },
        }
    }
}

/// `t8434OutBlock1` — one F/O current-price row (`t8434OutBlock1[]`, an ARRAY
/// block).
///
/// The multi-quote response is a repeated row array (the spec marks
/// `t8434OutBlock1` an array). Every numeric-bearing field via
/// [`ls_core::string_or_number`]; `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8434OutBlock1 {
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
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub focode: String,
}

/// `t8434` response envelope.
///
/// `outblock1` is the multi-quote row array under the `t8434OutBlock1` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8434Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8434OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8434OutBlock1>,
}

// ---------------------------------------------------------------------------
// Standalone-lane reads (reach wave U3). These carry a placeholder
// `owner_class: standalone`, but the `standalone` module is OAuth-only
// (token/revoke) and cannot host a data read — they route through
// `market_session` (non-paginated, MarketData). KTD3.
// ---------------------------------------------------------------------------

/// Input block for `t1988` — 기초자산리스트조회 (ELW underlying-asset list). A
/// filter screen: `mkt_gb` selects the market and the `chk_*` flags toggle the
/// price/volume/amount/rate conditions (`"0"` = all). `from_rate`/`to_rate` are
/// the only Number-typed request fields — they MUST serialize as JSON numbers
/// (`string_as_number`, KTD4) or the gateway rejects the call with `IGW40011`.
#[derive(Serialize, Debug, Clone)]
pub struct T1988InBlock {
    /// Market / 시장구분 (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ).
    pub mkt_gb: String,
    /// Price filter / 가격설정 (`"0"` all).
    pub chk_price: String,
    /// Price lower bound / 가격1.
    pub from_price: String,
    /// Price upper bound / 가격2.
    pub to_price: String,
    /// Volume filter / 거래량설정 (`"0"` all).
    pub chk_vol: String,
    /// Volume lower bound / 거래량1.
    pub from_vol: String,
    /// Volume upper bound / 거래량2.
    pub to_vol: String,
    /// Rate filter / 등락율설정 (`"0"` all).
    pub chk_rate: String,
    /// Rate lower bound / 등락율1 (numeric request slot, KTD4).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub from_rate: String,
    /// Rate upper bound / 등락율2 (numeric request slot, KTD4).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub to_rate: String,
    /// Amount filter / 거래대금설정 (`"0"` all).
    pub chk_amt: String,
    /// Amount lower bound / 거래대금1.
    pub from_amt: String,
    /// Amount upper bound / 거래대금2.
    pub to_amt: String,
    /// Bullish-candle filter / 양봉설정 (`"0"` all).
    pub chk_up: String,
    /// Bearish-candle filter / 음봉설정 (`"0"` all).
    pub chk_down: String,
}

/// `t1988` request — wraps the in-block under `t1988InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1988Request {
    #[serde(rename = "t1988InBlock")]
    pub inblock: T1988InBlock,
}
impl T1988Request {
    /// Build a `t1988` all-underlyings request: every filter off (`"0"`),
    /// numeric rate bounds `0`, blank string bounds. Returns the unfiltered
    /// underlying-asset universe for one market segment.
    pub fn new(mkt_gb: impl Into<String>) -> Self {
        T1988Request {
            inblock: T1988InBlock {
                mkt_gb: mkt_gb.into(),
                chk_price: "0".into(),
                from_price: String::new(),
                to_price: String::new(),
                chk_vol: "0".into(),
                from_vol: String::new(),
                to_vol: String::new(),
                chk_rate: "0".into(),
                from_rate: "0".into(),
                to_rate: "0".into(),
                chk_amt: "0".into(),
                from_amt: String::new(),
                to_amt: String::new(),
                chk_up: "0".into(),
                chk_down: "0".into(),
            },
        }
    }
}
impl Default for T1988Request {
    fn default() -> Self {
        Self::new("0")
    }
}

/// `t1988OutBlock1` — one underlying-asset row (the Object-Array detail block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1988OutBlock1 {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Standard code / 표준코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 부호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 누적거래량(주).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1988OutBlock` — summary header: KOSPI/KOSDAQ counts plus the per-asset row
/// array under `t1988OutBlock1` (single-or-array via [`ls_core::de_vec_or_single`]).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1988OutBlock {
    /// KOSPI issue count / 코스피종목건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ksp_cnt: String,
    /// KOSDAQ issue count / 코스닥종목건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ksd_cnt: String,
}

/// `t1988` response — summary `t1988OutBlock` + the per-asset array
/// `t1988OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1988Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1988OutBlock", default)]
    pub outblock: T1988OutBlock,
    #[serde(
        rename = "t1988OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1988OutBlock1>,
}

/// Input block for `t3102` — 뉴스본문 (news body). Keyed by `sNewsno`, a news
/// number sourced ONLY from the realtime `NWS` WebSocket title feed — there is
/// no REST producer of a news number, so the caller input is unresolved in this
/// (REST-only) wave (HELD).
#[derive(Serialize, Debug, Clone)]
pub struct T3102InBlock {
    /// News number / 뉴스번호.
    #[serde(rename = "sNewsno")]
    pub news_no: String,
}

/// `t3102` request — wraps the in-block under `t3102InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T3102Request {
    #[serde(rename = "t3102InBlock")]
    pub inblock: T3102InBlock,
}
impl T3102Request {
    /// Build a `t3102` news-body request for one news number.
    pub fn new(news_no: impl Into<String>) -> Self {
        T3102Request {
            inblock: T3102InBlock {
                news_no: news_no.into(),
            },
        }
    }
}

/// `t3102OutBlock2` — the news title block (single Object in the raw capture).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3102OutBlock2 {
    /// News title / 뉴스타이틀.
    #[serde(rename = "sTitle", deserialize_with = "ls_core::string_or_number")]
    pub title: String,
}

/// `t3102` response — the title block under `t3102OutBlock2`. The body/issue
/// blocks (`t3102OutBlock`/`t3102OutBlock1`, Object Arrays in the raw capture)
/// are not modeled: this read ships HELD (input-unresolved), so only the title
/// block is pinned for the offline round-trip.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3102Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3102OutBlock2", default)]
    pub outblock2: T3102OutBlock2,
}

/// Input block for `t3320` — FNG_요약 (FnGuide company summary). Keyed by
/// `gicode`, a stock code (종목코드). The paper gateway accepts the bare 6-digit
/// ticker (e.g. `"005930"` for 삼성전자), confirmed on a live paper smoke.
#[derive(Serialize, Debug, Clone)]
pub struct T3320InBlock {
    /// Stock code / 종목코드 (bare 6-digit ticker, e.g. `"005930"`).
    pub gicode: String,
}

/// `t3320` request — wraps the in-block under `t3320InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T3320Request {
    #[serde(rename = "t3320InBlock")]
    pub inblock: T3320InBlock,
}
impl T3320Request {
    /// Build a `t3320` company-summary request for one FnGuide company code.
    pub fn new(gicode: impl Into<String>) -> Self {
        T3320Request {
            inblock: T3320InBlock {
                gicode: gicode.into(),
            },
        }
    }
}

/// `t3320OutBlock` — the company-summary header (single Object in the raw
/// capture). A representative, spec-grounded subset; numeric-bearing fields via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3320OutBlock {
    /// Korean company name / 한글기업명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub company: String,
    /// Market segment name / 시장구분명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub marketnm: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Market capitalization / 시가총액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sigavalue: String,
}

/// `t3320OutBlock1` — the financial-ratios block (single Object in the raw
/// capture). A representative subset (PER/EPS/PBR/BPS); numerics via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3320OutBlock1 {
    /// Company code / 기업코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gicode: String,
    /// Price-to-earnings ratio / PER.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub per: String,
    /// Earnings per share / EPS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub eps: String,
    /// Price-to-book ratio / PBR.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pbr: String,
    /// Book value per share / BPS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bps: String,
}

/// `t3320` response — the summary `t3320OutBlock` + ratios `t3320OutBlock1`
/// (both single Objects per the raw capture).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3320Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3320OutBlock", default)]
    pub outblock: T3320OutBlock,
    #[serde(rename = "t3320OutBlock1", default)]
    pub outblock1: T3320OutBlock1,
}

// ---------------------------------------------------------------------------
// Night-derivatives lane (reach wave U6) — KRX야간파생 market-data reads, routed
// through `market_session` (KTD3). These are `venue_session: krx_extended`: the
// data is only meaningful in the night session (~18:00–05:00 KST), NOT the
// regular ~09:00–15:30 clock (KTD7). Members flip Implemented venue-provisional
// on a reachable in-window probe; an off-window empty result is not a valid
// attempt. Out-block shape from the raw capture (KTD5): t8455 master is an array
// (A0005); t8460 carries a single near-month header (A0003) + call/put option
// arrays (A0005); t8463 carries a single investor-code header (A0003) + a
// time-series row array (A0005). Canonical field by baseline `korean_name`
// (KTD6); t8463's `cnt` request field serializes as a JSON number (KTD4).
// ---------------------------------------------------------------------------

/// Input block for `t8455` — KRX야간파생 마스터조회(API용) (night-derivatives master).
///
/// `gubun` selects the instrument class (구분: `"NF"` 야간선물 / `"NC"` 야간콜옵션 /
/// `"NM"` 야간미니 / `"NO"` 야간풋옵션), a caller-supplied selector — not an
/// instrument identifier.
#[derive(Serialize, Debug, Clone)]
pub struct T8455InBlock {
    /// Class selector / 구분 (`"NF"`/`"NC"`/`"NM"`/`"NO"`).
    pub gubun: String,
}

/// `t8455` request — serializes to `{"t8455InBlock":{"gubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8455Request {
    #[serde(rename = "t8455InBlock")]
    pub inblock: T8455InBlock,
}
impl T8455Request {
    /// Build a `t8455` night-derivatives master request for one instrument class.
    pub fn new(gubun: impl Into<String>) -> Self {
        T8455Request {
            inblock: T8455InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8455OutBlock` — one night-derivatives master row (`t8455OutBlock[]`, an
/// ARRAY block in the raw capture). A representative subset; numeric `tradeunit`
/// (거래승수) via [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8455OutBlock {
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Standard code / 표준코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Trade multiplier / 거래승수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradeunit: String,
}

/// `t8455` response — the master row array under the `t8455OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`] (KTD5). All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8455Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8455OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8455OutBlock>,
}

/// Input block for `t8460` — KRX야간파생 옵션 전광판 (night-derivatives option board).
///
/// `yyyymm` is the contract month (월물, or `"WN"` for a weekly); `gubun` selects
/// the index variant (`"G"` 원지수 / `"W"` 위클리). Both caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T8460InBlock {
    /// Contract month / 월물 (혹은 주물 `"WN"`).
    pub yyyymm: String,
    /// Index variant / 구분 (`"G"` 원지수 / `"W"` 위클리).
    pub gubun: String,
}

/// `t8460` request — serializes to `{"t8460InBlock":{"yyyymm":...,"gubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8460Request {
    #[serde(rename = "t8460InBlock")]
    pub inblock: T8460InBlock,
}
impl T8460Request {
    /// Build a `t8460` night-option-board request for one contract month + variant.
    pub fn new(yyyymm: impl Into<String>, gubun: impl Into<String>) -> Self {
        T8460Request {
            inblock: T8460InBlock {
                yyyymm: yyyymm.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8460OutBlock` — the near-month futures header (single Object, A0003 in the
/// raw capture). A representative subset; numeric-bearing fields via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8460OutBlock {
    /// Near-month current price / 근월물현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmprice: String,
    /// Near-month change vs. previous / 근월물전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmchange: String,
    /// Near-month volume / 근월물거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmvolume: String,
    /// Near-month futures code / 근월물선물코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmshcode: String,
}

/// `t8460OutBlock1` — one CALL-option board row (`t8460OutBlock1[]`, an ARRAY
/// block, A0005). A representative subset; numerics via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8460OutBlock1 {
    /// Strike price / 행사가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub actprice: String,
    /// Call option code / 콜옵션코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Best offer / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
}

/// `t8460OutBlock2` — one PUT-option board row (`t8460OutBlock2[]`, an ARRAY
/// block, A0005). A representative subset; numerics via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8460OutBlock2 {
    /// Strike price / 행사가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub actprice: String,
    /// Put option code / 풋옵션코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Best offer / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
}

/// `t8460` response — the near-month header `t8460OutBlock` + the call-option
/// array `t8460OutBlock1` + the put-option array `t8460OutBlock2` (each tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8460Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8460OutBlock", default)]
    pub outblock: T8460OutBlock,
    #[serde(
        rename = "t8460OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8460OutBlock1>,
    #[serde(
        rename = "t8460OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<T8460OutBlock2>,
}

/// Input block for `t8463` — KRX야간파생 투자자시간대별(API용) (night-derivatives
/// investor-by-timeslot).
///
/// `tm_rng` is the timeslot (시간대: `D`/`N`/`U`); `fot_clsf_cd` is the F/O
/// distinction (선물옵션구분); `bsc_asts_id` is the underlying-asset code
/// (기초자산코드); `cnt` is the requested COUNT (조회건수), a numeric REQUEST field
/// serialized as a JSON number via [`ls_core::string_as_number`] (KTD4 — the
/// string form risks `IGW40011`); `bgubun` is the previous-day flag (전일분).
#[derive(Serialize, Debug, Clone)]
pub struct T8463InBlock {
    /// Timeslot / 시간대 (`"D"`/`"N"`/`"U"`).
    pub tm_rng: String,
    /// F/O distinction / 선물옵션구분.
    pub fot_clsf_cd: String,
    /// Underlying-asset code / 기초자산코드.
    pub bsc_asts_id: String,
    /// Requested count / 조회건수 (serialized as a JSON number, KTD4).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
    /// Previous-day flag / 전일분.
    pub bgubun: String,
}

/// `t8463` request — serializes to `{"t8463InBlock":{...,"cnt":<number>,...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8463Request {
    #[serde(rename = "t8463InBlock")]
    pub inblock: T8463InBlock,
}
impl T8463Request {
    /// Build a `t8463` investor-by-timeslot request. `cnt` defaults to `"20"`
    /// (rows requested); `bgubun` to `"0"` (current day).
    pub fn new(
        tm_rng: impl Into<String>,
        fot_clsf_cd: impl Into<String>,
        bsc_asts_id: impl Into<String>,
    ) -> Self {
        T8463Request {
            inblock: T8463InBlock {
                tm_rng: tm_rng.into(),
                fot_clsf_cd: fot_clsf_cd.into(),
                bsc_asts_id: bsc_asts_id.into(),
                cnt: "20".into(),
                bgubun: "0".into(),
            },
        }
    }
}

/// `t8463OutBlock` — the investor-code header (single Object, A0003). A
/// representative subset; the per-investor-type codes. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8463OutBlock {
    /// Timeslot / 시간대 (`"D"`/`"N"`/`"U"`).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tm_rng: String,
    /// Individual-investor code / 개인투자자코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indcode: String,
    /// Foreign-investor code / 외국인투자자코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub forcode: String,
}

/// `t8463OutBlock1` — one investor-by-timeslot row (`t8463OutBlock1[]`, an ARRAY
/// block, A0005). A representative subset; numeric net-buy volumes via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8463OutBlock1 {
    /// Date / 일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Individual net-buy volume / 개인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indmsvol: String,
    /// Foreign net-buy volume / 외국인순매수거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub formsvol: String,
}

/// `t8463` response — the investor-code header `t8463OutBlock` + the
/// time-series row array `t8463OutBlock1` (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8463Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8463OutBlock", default)]
    pub outblock: T8463OutBlock,
    #[serde(
        rename = "t8463OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8463OutBlock1>,
}

// ---------------------------------------------------------------------------
// Overseas-stock reads (reach wave U7). Domain `overseas_stock` (`g`-prefix),
// path `/overseas-stock/{market-data,chart}`. Non-paginated market-data reads
// keyed by an exchange code + symbol (e.g. `82`/`TSLA`). `venue_session:
// unspecified` (uncharted). Out-block keys/array-ness from the raw capture
// (KTD5); canonical price/name field by `korean_name` from non-collapsing
// fixtures (KTD6). Numeric request counts serialize as JSON numbers (KTD4).
// ---------------------------------------------------------------------------

/// Input block for `g3101` — 해외주식 현재가 조회 (overseas current-price). Keyed by
/// an exchange code (`exchcd`, e.g. `"82"` = NASDAQ) + `symbol` plus the
/// composite `keysymbol` (= exchcd+symbol). `delaygb` is the realtime/delayed
/// distinction (`"R"` = realtime).
#[derive(Serialize, Debug, Clone)]
pub struct G3101InBlock {
    /// Realtime/delayed distinction / 지연구분 (`"R"` = realtime).
    pub delaygb: String,
    /// Composite key / KEY종목코드 (`exchcd` + `symbol`).
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
}

/// `g3101` request — serializes to `{"g3101InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3101Request {
    #[serde(rename = "g3101InBlock")]
    pub inblock: G3101InBlock,
}
impl G3101Request {
    /// Build a `g3101` current-price request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        G3101Request {
            inblock: G3101InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `g3101OutBlock` — the overseas current-price snapshot (single object).
///
/// A representative subset; every numeric-bearing field via
/// [`ls_core::string_or_number`]. `price` (현재가) is the canonical price field
/// (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3101OutBlock {
    /// Korean name / 한글종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
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
    /// Currency / 통화.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub currency: String,
}

/// `g3101` response envelope. `outblock` is the snapshot under the
/// `g3101OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3101Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3101OutBlock", default)]
    pub outblock: G3101OutBlock,
}

/// Input block for `g3104` — 해외주식 종목정보 조회 (overseas stock-info master).
/// Same key shape as `g3101`.
#[derive(Serialize, Debug, Clone)]
pub struct G3104InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
}

/// `g3104` request — serializes to `{"g3104InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3104Request {
    #[serde(rename = "g3104InBlock")]
    pub inblock: G3104InBlock,
}
impl G3104Request {
    /// Build a `g3104` stock-info request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        G3104Request {
            inblock: G3104InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `g3104OutBlock` — the overseas stock-info master (single object).
///
/// `korname` (한글종목명) is the canonical name field (KTD6). Every
/// numeric-bearing field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3104OutBlock {
    /// Korean name / 한글종목명 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// English name / 영문종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub engname: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Exchange name / 거래소명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exchange_name: String,
    /// Nation name / 국가명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nation_name: String,
    /// Currency / 통화.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub currency: String,
    /// Listed shares / 상장주식수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub share: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pcls: String,
}

/// `g3104` response envelope (single out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3104Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3104OutBlock", default)]
    pub outblock: G3104OutBlock,
}

/// Input block for `g3106` — 해외주식 현재가호가 조회 (overseas current-price +
/// order book). Same key shape as `g3101`.
#[derive(Serialize, Debug, Clone)]
pub struct G3106InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
}

/// `g3106` request — serializes to `{"g3106InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3106Request {
    #[serde(rename = "g3106InBlock")]
    pub inblock: G3106InBlock,
}
impl G3106Request {
    /// Build a `g3106` current-price+order-book request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        G3106Request {
            inblock: G3106InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `g3106OutBlock` — the overseas current-price + level-1 order book (single
/// object).
///
/// `price` (현재가) is the canonical price field (KTD6). Every numeric-bearing
/// field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3106OutBlock {
    /// Korean name / 한글종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
}

/// `g3106` response envelope (single out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3106Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3106OutBlock", default)]
    pub outblock: G3106OutBlock,
}

/// Input block for `g3102` — 해외주식 시간대별 (overseas time-series tick read).
/// `readcnt` is the requested row COUNT and `cts_seq` the continuation
/// sequence — both numeric REQUEST fields serialized as JSON numbers
/// (`string_as_number`, KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct G3102InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Continuation sequence / 연속조회키 (serialized as a JSON number; `"0"`
    /// first page).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cts_seq: String,
}

/// `g3102` request — serializes to `{"g3102InBlock":{...}}` with `readcnt` /
/// `cts_seq` as JSON numbers.
#[derive(Serialize, Debug, Clone)]
pub struct G3102Request {
    #[serde(rename = "g3102InBlock")]
    pub inblock: G3102InBlock,
}
impl G3102Request {
    /// Build a `g3102` time-series request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
        readcnt: impl Into<String>,
        cts_seq: impl Into<String>,
    ) -> Self {
        G3102Request {
            inblock: G3102InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
                readcnt: readcnt.into(),
                cts_seq: cts_seq.into(),
            },
        }
    }
}

/// `g3102OutBlock` — the time-series header (single object): the echo + the
/// row count.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3102OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Continuation sequence / 연속조회키.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_seq: String,
    /// Returned row count / 레코드카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `g3102OutBlock1` — one time-series tick row (`g3102OutBlock1[]`, an ARRAY
/// block). `price` (현재가) is the canonical price field (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3102OutBlock1 {
    /// Local date / 현지일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub locdate: String,
    /// Local time / 현지시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub loctime: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Execution volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exevol: String,
}

/// `g3102` response envelope: header out-block + the row array under the
/// `g3102OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3102Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3102OutBlock", default)]
    pub outblock: G3102OutBlock,
    #[serde(
        rename = "g3102OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<G3102OutBlock1>,
}

/// Input block for `g3103` — 해외주식 일주월 조회 (overseas daily/weekly/monthly
/// chart). `gubun` selects the period (`"4"` = monthly) and `date` is the
/// reference date (조회일자).
#[derive(Serialize, Debug, Clone)]
pub struct G3103InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
    /// Period distinction / 주기구분 (`"4"` = monthly).
    pub gubun: String,
    /// Reference date / 조회일자 (`YYYYMMDD`).
    pub date: String,
}

/// `g3103` request — serializes to `{"g3103InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3103Request {
    #[serde(rename = "g3103InBlock")]
    pub inblock: G3103InBlock,
}
impl G3103Request {
    /// Build a `g3103` period-chart request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
        gubun: impl Into<String>,
        date: impl Into<String>,
    ) -> Self {
        G3103Request {
            inblock: G3103InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
                gubun: gubun.into(),
                date: date.into(),
            },
        }
    }
}

/// `g3103OutBlock` — the chart header (single object): the symbol echo + the
/// reference date.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3103OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Period distinction / 주기구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
    /// Reference date / 조회일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
}

/// `g3103OutBlock1` — one chart bar row (`g3103OutBlock1[]`, an ARRAY block).
/// `price` (현재가) is the canonical price field (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3103OutBlock1 {
    /// Business date / 영업일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chedate: String,
    /// Current (close) price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
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
}

/// `g3103` response envelope: header out-block + the bar array under the
/// `g3103OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3103Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3103OutBlock", default)]
    pub outblock: G3103OutBlock,
    #[serde(
        rename = "g3103OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<G3103OutBlock1>,
}

/// Input block for `g3190` — 해외주식 마스터 조회 (overseas master list). Keyed by a
/// nation code (`natcode`, e.g. `"US"`) + exchange distinction (`exgubun`).
/// `readcnt` is the requested row COUNT, a numeric REQUEST field serialized as a
/// JSON number (`string_as_number`, KTD4). `cts_value` is the (string)
/// continuation token (`""` first page).
#[derive(Serialize, Debug, Clone)]
pub struct G3190InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Nation code / 국가코드 (`"US"`).
    pub natcode: String,
    /// Exchange distinction / 거래소구분.
    pub exgubun: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Continuation token / 연속조회키 (`""` first page).
    pub cts_value: String,
}

/// `g3190` request — serializes to `{"g3190InBlock":{...}}` with `readcnt` as a
/// JSON number.
#[derive(Serialize, Debug, Clone)]
pub struct G3190Request {
    #[serde(rename = "g3190InBlock")]
    pub inblock: G3190InBlock,
}
impl G3190Request {
    /// Build a `g3190` master-list request for one nation/exchange.
    pub fn new(
        delaygb: impl Into<String>,
        natcode: impl Into<String>,
        exgubun: impl Into<String>,
        readcnt: impl Into<String>,
        cts_value: impl Into<String>,
    ) -> Self {
        G3190Request {
            inblock: G3190InBlock {
                delaygb: delaygb.into(),
                natcode: natcode.into(),
                exgubun: exgubun.into(),
                readcnt: readcnt.into(),
                cts_value: cts_value.into(),
            },
        }
    }
}

/// `g3190OutBlock` — the master-list header (single object): the echo + the
/// continuation token + the row count.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3190OutBlock {
    /// Nation code / 국가코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub natcode: String,
    /// Continuation token / 연속조회키.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_value: String,
    /// Returned row count / 레코드카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `g3190OutBlock1` — one master row (`g3190OutBlock1[]`, an ARRAY block).
/// `korname` (한글종목명) is the canonical name field (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3190OutBlock1 {
    /// Composite key / KEY종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub keysymbol: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Korean name / 한글종목명 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// English name / 영문종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub engname: String,
    /// Currency / 통화.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub currency: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pcls: String,
}

/// `g3190` response envelope: header out-block + the master row array under the
/// `g3190OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3190Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3190OutBlock", default)]
    pub outblock: G3190OutBlock,
    #[serde(
        rename = "g3190OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<G3190OutBlock1>,
}

// ── Overseas-futures (`o`-prefix) reads — U8 reach wave ─────────────────────
//
// Surface: `/overseas-futureoption/market-data`, group `[해외선물] 시세`,
// instrument_domain overseas_futures, venue_session unspecified (uncharted). One
// `o`-probe + KTD9 A/B (wrong path → http=404, wrong tr_cd → http=500 IGW00215,
// intended → http=200; NO 01900) confirms the domain REACHABLE and our contract
// CORRECT. The two MASTER reads (o3101 futures, o3121 option) return non-empty
// data on paper; the four live quote/order-book reads (o3105/o3106/o3125/o3126)
// answer http=200 rsp_cd=00000 with an empty body (the live overseas-futures feed
// is not provisioned on paper) → PENDING per the disposition state machine. All
// request fields are strings (no numeric REQUEST field → no `string_as_number`).

/// Input block for `o3101` — 해외선물마스터조회 (overseas-futures master list). A
/// single `gubun` selector (`""` = all); no instrument identifier.
#[derive(Serialize, Debug, Clone)]
pub struct O3101InBlock {
    /// Distinction / 구분 (`""` = all products).
    pub gubun: String,
}

/// `o3101` request — serializes to `{"o3101InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3101Request {
    #[serde(rename = "o3101InBlock")]
    pub inblock: O3101InBlock,
}
impl O3101Request {
    /// Build an `o3101` futures-master request for one `gubun` selector.
    pub fn new(gubun: impl Into<String>) -> Self {
        O3101Request {
            inblock: O3101InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `o3101OutBlock` — one overseas-futures master row (`o3101OutBlock[]`, an
/// ARRAY block per the raw capture, KTD5). `symbol_nm` (종목명) is the canonical
/// name field (KTD6); `dot_gb` is a numeric out-block field (소수점자리수). Rust
/// fields are snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3101OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명 (canonical field, KTD6).
    #[serde(rename = "SymbolNm", deserialize_with = "ls_core::string_or_number")]
    pub symbol_nm: String,
    /// Base-product code / 기초상품코드.
    #[serde(rename = "BscGdsCd", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_cd: String,
    /// Base-product name / 기초상품명.
    #[serde(rename = "BscGdsNm", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_nm: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCd", deserialize_with = "ls_core::string_or_number")]
    pub exch_cd: String,
    /// Currency / 통화코드.
    #[serde(rename = "CrncyCd", deserialize_with = "ls_core::string_or_number")]
    pub crncy_cd: String,
    /// Unit price / 호가단위.
    #[serde(rename = "UntPrc", deserialize_with = "ls_core::string_or_number")]
    pub unt_prc: String,
    /// Decimal places / 소수점자리수 (numeric out field).
    #[serde(rename = "DotGb", deserialize_with = "ls_core::string_or_number")]
    pub dot_gb: String,
}

/// `o3101` response envelope: the master row array under the `o3101OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`] (KTD5).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3101Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "o3101OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<O3101OutBlock>,
}

/// Input block for `o3121` — 해외선물옵션 마스터 조회 (overseas-future-option master).
/// `MktGb` selects the market (`"O"` = option) and `BscGdsCd` filters by base
/// product (`""` = all).
#[derive(Serialize, Debug, Clone)]
pub struct O3121InBlock {
    /// Market distinction / 시장구분 (`"O"` = option).
    #[serde(rename = "MktGb")]
    pub mkt_gb: String,
    /// Option base-product code / 옵션기초상품코드 (`""` = all).
    #[serde(rename = "BscGdsCd")]
    pub bsc_gds_cd: String,
}

/// `o3121` request — serializes to `{"o3121InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3121Request {
    #[serde(rename = "o3121InBlock")]
    pub inblock: O3121InBlock,
}
impl O3121Request {
    /// Build an `o3121` option-master request for one market + base product.
    pub fn new(mkt_gb: impl Into<String>, bsc_gds_cd: impl Into<String>) -> Self {
        O3121Request {
            inblock: O3121InBlock {
                mkt_gb: mkt_gb.into(),
                bsc_gds_cd: bsc_gds_cd.into(),
            },
        }
    }
}

/// `o3121OutBlock` — one overseas-future-option master row (`o3121OutBlock[]`,
/// an ARRAY block per the raw capture, KTD5). `bsc_gds_nm` (기초상품명) is the
/// canonical name field (KTD6); `dot_gb` is a numeric out-block field. Rust
/// fields are snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3121OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Base-product code / 옵션기초상품코드.
    #[serde(rename = "BscGdsCd", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_cd: String,
    /// Base-product name / 기초상품명 (canonical field, KTD6).
    #[serde(rename = "BscGdsNm", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_nm: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCd", deserialize_with = "ls_core::string_or_number")]
    pub exch_cd: String,
    /// Strike price / 행사가.
    #[serde(rename = "XrcPrc", deserialize_with = "ls_core::string_or_number")]
    pub xrc_prc: String,
    /// Option type code / 콜풋구분.
    #[serde(rename = "OptTpCode", deserialize_with = "ls_core::string_or_number")]
    pub opt_tp_code: String,
    /// Decimal places / 소수점자리수 (numeric out field).
    #[serde(rename = "DotGb", deserialize_with = "ls_core::string_or_number")]
    pub dot_gb: String,
}

/// `o3121` response envelope: the master row array under the `o3121OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`] (KTD5).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3121Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "o3121OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<O3121OutBlock>,
}

/// Input block for `o3105` — 해외선물 현재가(종목정보) 조회 (overseas-futures
/// current price / symbol info). Keyed by one `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3105InBlock {
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3105` request — serializes to `{"o3105InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3105Request {
    #[serde(rename = "o3105InBlock")]
    pub inblock: O3105InBlock,
}
impl O3105Request {
    /// Build an `o3105` symbol-info request for one overseas-futures symbol.
    pub fn new(symbol: impl Into<String>) -> Self {
        O3105Request {
            inblock: O3105InBlock {
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3105OutBlock` — the overseas-futures current-price snapshot (single object
/// per the raw capture, KTD5). `trd_p` (체결가격) is the canonical price field
/// (KTD6); `tot_q`/`trd_q`/`seq_no`/`dot_gb` are numeric. Rust fields are
/// snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3105OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(rename = "SymbolNm", deserialize_with = "ls_core::string_or_number")]
    pub symbol_nm: String,
    /// Trade price / 체결가격 (canonical field, KTD6).
    #[serde(rename = "TrdP", deserialize_with = "ls_core::string_or_number")]
    pub trd_p: String,
    /// Open / 시가.
    #[serde(rename = "OpenP", deserialize_with = "ls_core::string_or_number")]
    pub open_p: String,
    /// High / 고가.
    #[serde(rename = "HighP", deserialize_with = "ls_core::string_or_number")]
    pub high_p: String,
    /// Low / 저가.
    #[serde(rename = "LowP", deserialize_with = "ls_core::string_or_number")]
    pub low_p: String,
    /// Total volume / 누적거래량 (numeric out field).
    #[serde(rename = "TotQ", deserialize_with = "ls_core::string_or_number")]
    pub tot_q: String,
    /// Trade quantity / 체결수량 (numeric out field).
    #[serde(rename = "TrdQ", deserialize_with = "ls_core::string_or_number")]
    pub trd_q: String,
    /// Sequence number / 수신순번 (numeric out field).
    #[serde(rename = "SeqNo", deserialize_with = "ls_core::string_or_number")]
    pub seq_no: String,
    /// Currency / 통화코드.
    #[serde(rename = "CrncyCd", deserialize_with = "ls_core::string_or_number")]
    pub crncy_cd: String,
}

/// `o3105` response envelope. Single out-block under the `o3105OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3105Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3105OutBlock", default)]
    pub outblock: O3105OutBlock,
}

/// Input block for `o3106` — 해외선물 현재가호가 조회 (overseas-futures current
/// price + order book). Keyed by one `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3106InBlock {
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3106` request — serializes to `{"o3106InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3106Request {
    #[serde(rename = "o3106InBlock")]
    pub inblock: O3106InBlock,
}
impl O3106Request {
    /// Build an `o3106` order-book request for one overseas-futures symbol.
    pub fn new(symbol: impl Into<String>) -> Self {
        O3106Request {
            inblock: O3106InBlock {
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3106OutBlock` — the overseas-futures current-price + order-book snapshot
/// (single object per the raw capture, KTD5). `price` (현재가) is the canonical
/// price field (KTD6); the level-1 book + counts are numeric.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3106OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbolname: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best ask price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Total ask volume / 매도호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// Total bid volume / 매수호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bid: String,
}

/// `o3106` response envelope. Single out-block under the `o3106OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3106Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3106OutBlock", default)]
    pub outblock: O3106OutBlock,
}

/// Input block for `o3125` — 해외선물옵션 현재가(종목정보) 조회 (overseas
/// future-option current price / symbol info). Keyed by `mktgb` + `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3125InBlock {
    /// Market distinction / 시장구분 (`"F"` = future, `"O"` = option).
    pub mktgb: String,
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3125` request — serializes to `{"o3125InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3125Request {
    #[serde(rename = "o3125InBlock")]
    pub inblock: O3125InBlock,
}
impl O3125Request {
    /// Build an `o3125` symbol-info request for one market + symbol.
    pub fn new(mktgb: impl Into<String>, symbol: impl Into<String>) -> Self {
        O3125Request {
            inblock: O3125InBlock {
                mktgb: mktgb.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3125OutBlock` — the overseas-future-option current-price snapshot (single
/// object per the raw capture, KTD5). `trd_p` (체결가격) is the canonical price
/// field (KTD6); `tot_q`/`trd_q`/`seq_no`/`dot_gb` are numeric. Rust fields are
/// snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3125OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(rename = "SymbolNm", deserialize_with = "ls_core::string_or_number")]
    pub symbol_nm: String,
    /// Trade price / 체결가격 (canonical field, KTD6).
    #[serde(rename = "TrdP", deserialize_with = "ls_core::string_or_number")]
    pub trd_p: String,
    /// Open / 시가.
    #[serde(rename = "OpenP", deserialize_with = "ls_core::string_or_number")]
    pub open_p: String,
    /// High / 고가.
    #[serde(rename = "HighP", deserialize_with = "ls_core::string_or_number")]
    pub high_p: String,
    /// Low / 저가.
    #[serde(rename = "LowP", deserialize_with = "ls_core::string_or_number")]
    pub low_p: String,
    /// Total volume / 누적거래량 (numeric out field).
    #[serde(rename = "TotQ", deserialize_with = "ls_core::string_or_number")]
    pub tot_q: String,
    /// Trade quantity / 체결수량 (numeric out field).
    #[serde(rename = "TrdQ", deserialize_with = "ls_core::string_or_number")]
    pub trd_q: String,
    /// Sequence number / 수신순번 (numeric out field).
    #[serde(rename = "SeqNo", deserialize_with = "ls_core::string_or_number")]
    pub seq_no: String,
    /// Currency / 통화코드.
    #[serde(rename = "CrncyCd", deserialize_with = "ls_core::string_or_number")]
    pub crncy_cd: String,
}

/// `o3125` response envelope. Single out-block under the `o3125OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3125Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3125OutBlock", default)]
    pub outblock: O3125OutBlock,
}

/// Input block for `o3126` — 해외선물옵션 현재가호가 조회 (overseas future-option
/// current price + order book). Keyed by `mktgb` + `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3126InBlock {
    /// Market distinction / 시장구분 (`"F"` = future, `"O"` = option).
    pub mktgb: String,
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3126` request — serializes to `{"o3126InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3126Request {
    #[serde(rename = "o3126InBlock")]
    pub inblock: O3126InBlock,
}
impl O3126Request {
    /// Build an `o3126` order-book request for one market + symbol.
    pub fn new(mktgb: impl Into<String>, symbol: impl Into<String>) -> Self {
        O3126Request {
            inblock: O3126InBlock {
                mktgb: mktgb.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3126OutBlock` — the overseas-future-option current-price + order-book
/// snapshot (single object per the raw capture, KTD5). `price` (현재가) is the
/// canonical price field (KTD6); the level-1 book + counts are numeric.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3126OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbolname: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best ask price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Total ask volume / 매도호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// Total bid volume / 매수호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bid: String,
}

/// `o3126` response envelope. Single out-block under the `o3126OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3126Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3126OutBlock", default)]
    pub outblock: O3126OutBlock,
}

// ---------------------------------------------------------------------------
// Domestic stock master/reference breadth wave (plan -004). Non-paginated
// `market_session` reads; each out-block is a single Object-Array modeled as a
// `Vec<...>` via `de_vec_or_single` with the literal `<tr>OutBlock` key read from
// the raw `res_example` (KTD3). No numeric request fields here, so no
// `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t9945` — 주식마스터조회 (stock master). `gubun` selects the
/// market: `"1"` = KOSPI (KSP), `"2"` = KOSDAQ (KSD).
#[derive(Serialize, Debug, Clone)]
pub struct T9945InBlock {
    /// Market selector / 구분 (KSP:1 KSD:2).
    pub gubun: String,
}

/// `t9945` request — serializes to `{"t9945InBlock":{"gubun":"1"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9945Request {
    #[serde(rename = "t9945InBlock")]
    pub inblock: T9945InBlock,
}
impl T9945Request {
    /// Build a `t9945` stock-master request for one market (`"1"`=KOSPI, `"2"`=KOSDAQ).
    pub fn new(gubun: impl Into<String>) -> Self {
        T9945Request {
            inblock: T9945InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t9945OutBlock` — one stock-master row: the ticker, its codes, and Korean name.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9945OutBlock {
    /// Stock name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드 (the canonical 6-digit ticker).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드 (ISIN).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// ETF flag / ETF구분 (`"1"` = ETF).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etfchk: String,
    /// NXT-listing flag / NXT상장구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nxt_chk: String,
    /// Reserved filler / filler.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub filler: String,
}

/// `t9945` response — the stock-master array under `t9945OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9945Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9945OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9945OutBlock>,
}

/// Input block for `t3202` — 종목별증시일정 (per-stock market schedule). `date`
/// is an optional filter (empty = the full schedule for the ticker).
#[derive(Serialize, Debug, Clone)]
pub struct T3202InBlock {
    /// Short code / 종목코드.
    pub shcode: String,
    /// Date filter / 일자 (empty = all).
    pub date: String,
}

/// `t3202` request — serializes to `{"t3202InBlock":{"shcode":"...","date":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T3202Request {
    #[serde(rename = "t3202InBlock")]
    pub inblock: T3202InBlock,
}
impl T3202Request {
    /// Build a `t3202` schedule request for one ticker (full schedule, no date filter).
    pub fn new(shcode: impl Into<String>) -> Self {
        T3202Request {
            inblock: T3202InBlock {
                shcode: shcode.into(),
                date: String::new(),
            },
        }
    }
}

/// `t3202OutBlock` — one schedule row: the corporate event for the ticker.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3202OutBlock {
    /// Issuer number / 발행체번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub custno: String,
    /// Issuer name / 발행회사명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub custnm: String,
    /// Reference date / 기준일 (YYYYMMDD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recdt: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Table id / 테이블아이디.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tableid: String,
    /// Event name / 업무명 (the canonical schedule label, e.g. 주주총회).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upunm: String,
    /// Event class / 업무구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upgu: String,
}

/// `t3202` response — the schedule array under `t3202OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3202Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t3202OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T3202OutBlock>,
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

    /// Fetch the ETF current-price (시세) snapshot for one short code via `t1901`.
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket.
    pub async fn etf_quote(&self, req: &T1901Request) -> LsResult<T1901Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1901_POLICY, req)
            .await
    }

    /// Fetch pivot / demark levels for one symbol via `t1105` (non-paginated).
    pub async fn pivot_demark(&self, req: &T1105Request) -> LsResult<T1105Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1105_POLICY, req)
            .await
    }

    /// Fetch the current-price memo rows for one symbol via `t1104` (non-paginated).
    pub async fn price_memo(&self, req: &T1104Request) -> LsResult<T1104Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1104_POLICY, req)
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

    /// List every stock issue (주식종목조회) via `t8430`. Non-paginated; returns the
    /// full KOSPI/KOSDAQ issue array (`shcode`/`hname`/price bounds per issue).
    pub async fn stock_issues(&self, req: &T8430Request) -> LsResult<T8430Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8430_POLICY, req)
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
    pub async fn stock_futures_underlying_master(
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

    /// Read the price-bearing index-option master (지수옵션마스터조회) via `t8433`.
    ///
    /// Each row carries the contract name + codes PLUS the daily limit/close
    /// reference prices (상한가/하한가/전일종가/전일고가/전일저가/기준가) — the
    /// fuller variant. For the codes-only counterpart (3 identity fields, no
    /// price refs) use [`MarketSession::index_option_master_codes`] (`t9944`).
    /// Non-paginated, no caller input; returns the index-option contract rows.
    pub async fn index_option_master(
        &self,
        req: &T8433Request,
    ) -> LsResult<T8433Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8433_POLICY, req)
            .await
    }

    /// Read the derivatives master (파생종목마스터조회) via `t8435`.
    /// Non-paginated; keyed by a `gubun` segment selector — the MINI/weekly
    /// segments (`"MF"` 미니선물 / `"MO"` 미니옵션 / `"WK"` 코스피200위클리옵션 /
    /// `"SF"` 코스닥150선물 / `"QW"` 코스닥150위클리옵션). Returns the master
    /// snapshot (name + codes + daily limit/close reference prices).
    pub async fn derivatives_master(&self, req: &T8435Request) -> LsResult<T8435Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8435_POLICY, req)
            .await
    }

    /// Read the price-bearing index-futures master (지수선물마스터조회) via `t8467`.
    ///
    /// Each row carries the contract name + codes PLUS the daily limit/close
    /// reference prices (상한가/하한가/전일종가/전일고가/전일저가/기준가) — the
    /// fuller variant. For the codes-only counterpart (3 identity fields, no
    /// price refs) use [`MarketSession::index_futures_master_codes`] (`t9943`).
    /// Non-paginated; keyed by a `gubun` segment selector (`"V"` volatility /
    /// `"S"` sector / `"Q"` KOSDAQ150 / any other value → KOSPI200 index
    /// futures).
    pub async fn index_futures_master(&self, req: &T8467Request) -> LsResult<T8467Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8467_POLICY, req)
            .await
    }

    /// Read the codes-only index-futures master (지수선물마스터조회) via `t9943`.
    ///
    /// The lightweight index-futures master: each row carries only the 3 identity
    /// fields (contract name `hname` + short/expanded codes), with NO daily
    /// price references. This is the distinction from
    /// [`MarketSession::index_futures_master`] (`t8467`), whose rows additionally
    /// carry the daily limit/close reference prices (~9 fields). Both accept the
    /// same `gubun` segment selector (`"V"` volatility / `"S"` sector / any other
    /// value → KOSPI200 index futures); pick this one when only the contract
    /// codes are needed. Non-paginated; returns the master snapshot row array.
    pub async fn index_futures_master_codes(
        &self,
        req: &T9943Request,
    ) -> LsResult<T9943Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9943_POLICY, req)
            .await
    }

    /// Read the codes-only index-option master (지수옵션마스터조회) via `t9944`.
    ///
    /// The lightweight index-option master: each row carries only the 3 identity
    /// fields (contract name `hname` + short/expanded codes), with NO daily
    /// price references. This is the distinction from
    /// [`MarketSession::index_option_master`] (`t8433`), whose rows additionally
    /// carry the daily limit/close reference prices. Pick this one when only the
    /// contract codes are needed. Non-paginated, no caller input; returns the
    /// master snapshot row array.
    pub async fn index_option_master_codes(
        &self,
        req: &T9944Request,
    ) -> LsResult<T9944Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9944_POLICY, req)
            .await
    }

    /// Read the F/O current-price (시세) snapshot via `t2111`. Non-paginated;
    /// keyed by a futures/option contract `focode`. Single out-block.
    pub async fn fo_quote(&self, req: &T2111Request) -> LsResult<T2111Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2111_POLICY, req)
            .await
    }

    /// Read the F/O current-price order book via `t2112`. Non-paginated; keyed by
    /// a contract `shcode`. Single out-block (5-level book).
    pub async fn fo_order_book(&self, req: &T2112Request) -> LsResult<T2112Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2112_POLICY, req)
            .await
    }

    /// Read the F/O price-memo (시세메모) via `t2106`. Non-paginated; keyed by a
    /// contract `code`. Returns a summary block + a memo-row array.
    pub async fn fo_price_memo(&self, req: &T2106Request) -> LsResult<T2106Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2106_POLICY, req)
            .await
    }

    /// Read the stock-futures current price via `t8402`. Non-paginated; keyed by
    /// a stock-futures contract `focode`. Single out-block.
    pub async fn stock_futures_quote(&self, req: &T8402Request) -> LsResult<T8402Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8402_POLICY, req)
            .await
    }

    /// Read the stock-futures order book via `t8403`. Non-paginated; keyed by a
    /// stock-futures contract `shcode`. Single out-block (10-level book).
    pub async fn stock_futures_order_book(
        &self,
        req: &T8403Request,
    ) -> LsResult<T8403Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8403_POLICY, req)
            .await
    }

    /// Read the F/O multi current-price via `t8434`. Non-paginated; keyed by a
    /// `qrycnt` count (a JSON number) + one or more `focode` contract codes.
    /// Returns a row array.
    pub async fn fo_multi_quote(&self, req: &T8434Request) -> LsResult<T8434Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8434_POLICY, req)
            .await
    }

    /// Read the ELW underlying-asset list (기초자산리스트조회) via `t1988`.
    /// Non-paginated; `mkt_gb` selects the market segment, all condition filters
    /// off. Routes through `market_session` (KTD3 — the placeholder
    /// `owner_class: standalone` is OAuth-only and cannot host a data read).
    pub async fn elw_underlying_list(&self, req: &T1988Request) -> LsResult<T1988Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1988_POLICY, req)
            .await
    }

    /// Read a news body (뉴스본문) via `t3102`. Non-paginated; keyed by a news
    /// number (`sNewsno`) sourced only from the realtime `NWS` WebSocket feed.
    /// Routes through `market_session` (KTD3).
    pub async fn news_body(&self, req: &T3102Request) -> LsResult<T3102Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T3102_POLICY, req)
            .await
    }

    /// Read the FnGuide company summary (FNG_요약) via `t3320`. Non-paginated;
    /// keyed by a bare 6-digit ticker (`gicode`, e.g. `"005930"`), confirmed on a
    /// live paper smoke. Routes through `market_session` (KTD3).
    pub async fn company_summary(&self, req: &T3320Request) -> LsResult<T3320Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T3320_POLICY, req)
            .await
    }

    /// Read the KRX night-derivatives master (KRX야간파생 마스터조회) via `t8455`.
    /// Non-paginated; `gubun` selects the instrument class. Returns the master
    /// row array. `venue_session: krx_extended` — the data is only meaningful in
    /// the night session (~18:00–05:00 KST), not the regular clock (KTD7).
    pub async fn night_derivatives_master(&self, req: &T8455Request) -> LsResult<T8455Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8455_POLICY, req)
            .await
    }

    /// Read the KRX night-derivatives option board (KRX야간파생 옵션 전광판) via
    /// `t8460`. Non-paginated; keyed by a contract month `yyyymm` + an index
    /// `gubun`. Returns the near-month header + call/put option arrays.
    /// `venue_session: krx_extended` (KTD7).
    pub async fn night_option_board(&self, req: &T8460Request) -> LsResult<T8460Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8460_POLICY, req)
            .await
    }

    /// Read the KRX night-derivatives investor-by-timeslot (KRX야간파생
    /// 투자자시간대별) via `t8463`. Non-paginated; keyed by a timeslot `tm_rng`, an
    /// F/O distinction `fot_clsf_cd`, and an underlying `bsc_asts_id`; `cnt` is a
    /// numeric count (JSON number, KTD4). Returns the investor-code header + a
    /// time-series row array. `venue_session: krx_extended` (KTD7).
    pub async fn night_investor_timeslot(&self, req: &T8463Request) -> LsResult<T8463Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8463_POLICY, req)
            .await
    }

    /// Read the overseas current price (해외주식 현재가) via `g3101`. Non-paginated;
    /// keyed by an exchange code + symbol (e.g. `82`/`TSLA`). Single out-block.
    /// `instrument_domain: overseas_stock`, `venue_session: unspecified`.
    pub async fn overseas_quote(&self, req: &G3101Request) -> LsResult<G3101Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3101_POLICY, req)
            .await
    }

    /// Read the overseas stock-info master (해외주식 종목정보) via `g3104`.
    /// Non-paginated; keyed by an exchange code + symbol. Single out-block.
    pub async fn overseas_stock_info(&self, req: &G3104Request) -> LsResult<G3104Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3104_POLICY, req)
            .await
    }

    /// Read the overseas current price + order book (해외주식 현재가호가) via
    /// `g3106`. Non-paginated; keyed by an exchange code + symbol. Single
    /// out-block (level-1 book).
    pub async fn overseas_order_book(&self, req: &G3106Request) -> LsResult<G3106Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3106_POLICY, req)
            .await
    }

    /// Read the overseas time-series ticks (해외주식 시간대별) via `g3102`.
    /// Non-paginated; keyed by an exchange code + symbol; `readcnt`/`cts_seq` are
    /// numeric request fields (JSON numbers, KTD4). Returns a header + tick array.
    pub async fn overseas_time_series(&self, req: &G3102Request) -> LsResult<G3102Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3102_POLICY, req)
            .await
    }

    /// Read the overseas period chart (해외주식 일주월) via `g3103`. Non-paginated;
    /// keyed by an exchange code + symbol + period `gubun` + `date`. Returns a
    /// header + bar array.
    pub async fn overseas_period_chart(&self, req: &G3103Request) -> LsResult<G3103Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3103_POLICY, req)
            .await
    }

    /// Read the overseas master list (해외주식 마스터) via `g3190`. Non-paginated;
    /// keyed by a nation code + exchange distinction; `readcnt` is a numeric
    /// request field (JSON number, KTD4). Returns a header + master row array.
    pub async fn overseas_master(&self, req: &G3190Request) -> LsResult<G3190Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3190_POLICY, req)
            .await
    }

    /// Read the overseas-futures master list (해외선물마스터) via `o3101`.
    /// Non-paginated; `gubun` filters (`""` = all), no instrument identifier.
    /// Returns a master row array. `instrument_domain: overseas_futures`,
    /// `venue_session: unspecified`.
    pub async fn overseas_futures_master(&self, req: &O3101Request) -> LsResult<O3101Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3101_POLICY, req)
            .await
    }

    /// Read the overseas-future-option master list (해외선물옵션 마스터) via `o3121`.
    /// Non-paginated; keyed by a market distinction + base-product filter. Returns
    /// a master row array. `venue_session: unspecified`.
    pub async fn overseas_option_master(&self, req: &O3121Request) -> LsResult<O3121Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3121_POLICY, req)
            .await
    }

    /// Read the overseas-futures current price / symbol info (해외선물 현재가) via
    /// `o3105`. Non-paginated; keyed by one `symbol`. Single out-block.
    pub async fn overseas_futures_quote(&self, req: &O3105Request) -> LsResult<O3105Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3105_POLICY, req)
            .await
    }

    /// Read the overseas-futures current price + order book (해외선물 현재가호가) via
    /// `o3106`. Non-paginated; keyed by one `symbol`. Single out-block (level-1
    /// book).
    pub async fn overseas_futures_order_book(
        &self,
        req: &O3106Request,
    ) -> LsResult<O3106Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3106_POLICY, req)
            .await
    }

    /// Read the overseas-future-option current price / symbol info (해외선물옵션
    /// 현재가) via `o3125`. Non-paginated; keyed by a market distinction + symbol.
    /// Single out-block.
    pub async fn overseas_option_quote(&self, req: &O3125Request) -> LsResult<O3125Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3125_POLICY, req)
            .await
    }

    /// Read the overseas-future-option current price + order book (해외선물옵션
    /// 현재가호가) via `o3126`. Non-paginated; keyed by a market distinction +
    /// symbol. Single out-block (level-1 book).
    pub async fn overseas_option_order_book(
        &self,
        req: &O3126Request,
    ) -> LsResult<O3126Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3126_POLICY, req)
            .await
    }

    /// Read the stock master (주식마스터조회) via `t9945`. Non-paginated; one
    /// market per call (`"1"`=KOSPI, `"2"`=KOSDAQ). Returns the full ticker
    /// master (code/ISIN/name) array.
    pub async fn stock_master(&self, req: &T9945Request) -> LsResult<T9945Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9945_POLICY, req)
            .await
    }

    /// Read a ticker's market schedule (종목별증시일정) via `t3202`. Non-paginated;
    /// keyed by `shcode`. Returns the corporate-event schedule rows.
    pub async fn stock_schedule(&self, req: &T3202Request) -> LsResult<T3202Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T3202_POLICY, req)
            .await
    }
}
