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
}
