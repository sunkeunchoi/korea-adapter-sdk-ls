//! WebSocket frame build + S3_ push decode.
//!
//! ## Composite routing key
//!
//! A subscription is keyed by `"<tr_cd>:<tr_key>"` so two TRs that share a
//! `tr_key` value (e.g. `("S3_", "005930")` and `("S2_", "005930")`) occupy
//! distinct entries in the subscription and dispatch maps. `:` is a safe
//! separator — LS TR codes are alphanumeric/underscore and TR keys are numeric /
//! option codes, neither containing `:`.
//!
//! ## Subscribe / unsubscribe frame shape
//!
//! Verified from `specs/ls_openapi_specs.json` (S3_ example):
//! `{"header":{"token":"<bearer>","tr_type":"<n>"},"body":{"tr_cd":...,"tr_key":...}}`.
//!
//! The lane is carried per-subscription as a [`WsLane`]: market-data channels
//! (like S3_) register with `tr_type "3"` (실시간 시세 등록) and deregister with `"4"`
//! (실시간 시세 해제); order-event channels register an account with `tr_type "1"`
//! (실시간 계좌 등록) and deregister with `"2"`. The builders take a [`WsLane`] so both
//! lanes share one frame path and an invalid lane is a compile error; the caller
//! (recipe/smoke) supplies the variant — see `metadata/trs/S3_.yaml`.
//!
//! ## S3_ push decode
//!
//! [`S3Trade`] decodes the KOSPI-trade push row (`body`) with the REAL LS field
//! names (`price`, `cvolume`, `volume`, `cgubun`, …). Every field uses
//! [`ls_core::string_or_number`] because the gateway sends quote fields as either
//! JSON strings or bare numbers, and `#[serde(default)]` lets a registration-ACK
//! frame (which carries an empty/`null` body) surface as a decode that does not
//! abort the stream.

use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

/// Build a composite subscription key: `"<tr_cd>:<tr_key>"`.
pub fn composite_key(tr_cd: &str, tr_key: &str) -> String {
    debug_assert!(
        !tr_cd.contains(':') && !tr_key.contains(':'),
        "tr_cd and tr_key must not contain ':' — composite key separator"
    );
    format!("{}:{}", tr_cd, tr_key)
}

/// Split a composite key back into `(tr_cd, tr_key)`.
///
/// Used in the reconnect replay loop. `unwrap_or((key, ""))` handles any
/// malformed entry without panic.
pub(crate) fn split_composite_key(key: &str) -> (&str, &str) {
    key.split_once(':').unwrap_or((key, ""))
}

/// The realtime subscription lane — selects the register/deregister `tr_type`
/// pair sent on the WebSocket.
///
/// A closed two-variant enum (rather than a raw `tr_type: &str`) makes an invalid
/// lane a compile error at the `subscribe`/`subscribe_typed` boundary and keeps
/// the register/deregister pairing in exactly one place — so a wrong-lane
/// deregister frame can never be emitted silently. The wire values are an LS
/// protocol detail confined to [`WsLane::register`]/[`WsLane::deregister`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsLane {
    /// 실시간 시세 — market-data feeds (S3_, K3_, H1_, …). Register `"3"`, deregister `"4"`.
    MarketData,
    /// 실시간 계좌 — order-event channels (SC0, AS0, …). Register `"1"`, deregister `"2"`.
    OrderEvent,
}

impl WsLane {
    /// The register (등록) `tr_type` wire value.
    pub(crate) fn register(self) -> &'static str {
        match self {
            WsLane::MarketData => "3",
            WsLane::OrderEvent => "1",
        }
    }

    /// The deregister (해제) `tr_type` wire value — the register value's pair.
    pub(crate) fn deregister(self) -> &'static str {
        match self {
            WsLane::MarketData => "4",
            WsLane::OrderEvent => "2",
        }
    }
}

/// Build an LS WebSocket subscribe (register) message for `lane`.
///
/// `{"header":{"token","tr_type":<reg>},"body":{"tr_cd","tr_key"}}` — the register
/// value is `lane.register()` (`"3"` market-data, `"1"` order-event). The lane is
/// stored per-subscription and reused verbatim on reconnect replay.
pub(crate) fn build_subscribe_msg(tr_cd: &str, tr_key: &str, token: &str, lane: WsLane) -> Message {
    build_frame(tr_cd, tr_key, token, lane.register())
}

/// Build an LS WebSocket unsubscribe (deregister) message for `lane`.
///
/// Emits `lane.deregister()` (`"4"` market-data, `"2"` order-event), so a caller
/// threads a single per-subscription [`WsLane`] through both build paths.
pub(crate) fn build_unsubscribe_msg(
    tr_cd: &str,
    tr_key: &str,
    token: &str,
    lane: WsLane,
) -> Message {
    build_frame(tr_cd, tr_key, token, lane.deregister())
}

/// Shared frame constructor — the token rides only in the header, never logged.
fn build_frame(tr_cd: &str, tr_key: &str, token: &str, tr_type: &str) -> Message {
    let json = serde_json::json!({
        "header": { "token": token, "tr_type": tr_type },
        "body":   { "tr_cd": tr_cd, "tr_key": tr_key }
    });
    Message::Text(json.to_string().into())
}

/// Decoded S3_ (KOSPI 체결 / KOSPI trade) realtime push row.
///
/// Field names mirror the LS spec (`specs/ls_openapi_specs.json` → S3_
/// `response_body`) verbatim. A representative, spec-grounded subset of the full
/// push row centered on the trade-tick fields. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and
/// a sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct S3Trade {
    /// Trade time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// Current (last-trade) price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Trade direction / 체결구분 (`+` = buy, `-` = sell).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
    /// This-tick trade volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Accumulated trade value / 누적거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Trade strength / 체결강도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpower: String,
    /// Best offer / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// Decoded K3_ (KOSDAQ 체결 / KOSDAQ trade) realtime push row.
///
/// KOSDAQ 체결 is the schema sibling of KOSPI 체결 (`S3_`); the field set mirrors
/// [`S3Trade`] and is **structurally-verified** against the migration source's
/// `K3Response` (`korea-broker-sdk-ls` `generated/stock.rs`) — a single object
/// out-block (not an array), with these exact LS field names. A representative,
/// spec-grounded subset of the fuller push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct K3Trade {
    /// Trade time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// Current (last-trade) price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Trade direction / 체결구분 (`+` = buy, `-` = sell).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
    /// This-tick trade volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Accumulated trade value / 누적거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Trade strength / 체결강도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpower: String,
    /// Best offer / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// Decoded H1_ (KOSPI 호가잔량 / KOSPI order-book) realtime push row.
///
/// **Structurally-verified** against the migration source's `H1Response`
/// (`korea-broker-sdk-ls` `generated/stock.rs`): a single object out-block (not an
/// array), with these exact LS field names. The full row carries a 10-level
/// bid/ask ladder; this is a representative, spec-grounded subset — the top two
/// levels plus the book totals. Every field is `string_or_number`-coerced and
/// `#[serde(default)]` so both wire shapes and a sparse registration-ACK body
/// deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct H1Trade {
    /// Order-book timestamp / 호가시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
    /// Level-1 offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Level-1 bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Level-1 offer remaining qty / 매도호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Level-1 bid remaining qty / 매수호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Level-2 offer (ask) price / 매도호가2.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho2: String,
    /// Level-2 bid price / 매수호가2.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho2: String,
    /// Total offer remaining qty / 총매도호가잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totofferrem: String,
    /// Total bid remaining qty / 총매수호가잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totbidrem: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// Decoded HA_ (KOSDAQ 호가잔량 / KOSDAQ order-book) realtime push row.
///
/// **Structurally-verified** against the migration source's `HAResponse`
/// (`generated/stock.rs`) — the KOSDAQ schema sibling of `H1_`, field-identical:
/// a single object out-block with these exact LS field names (10-level ladder in
/// the full row; a representative top-level + totals subset here). Every field is
/// `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct HATrade {
    /// Order-book timestamp / 호가시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
    /// Level-1 offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Level-1 bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Level-1 offer remaining qty / 매도호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Level-1 bid remaining qty / 매수호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Level-2 offer (ask) price / 매도호가2.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho2: String,
    /// Level-2 bid price / 매수호가2.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho2: String,
    /// Total offer remaining qty / 총매도호가잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totofferrem: String,
    /// Total bid remaining qty / 총매수호가잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totbidrem: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// Decoded S2_ (KOSPI 우선호가 / KOSPI best-quote) realtime push row.
///
/// **Structurally-verified** against the migration source's `S2Response`
/// (`generated/stock.rs`): a single object out-block carrying only the
/// best-quote pair — three fields, the complete row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct S2Trade {
    /// Best offer (ask) / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// Decoded US3 (통합 체결 / integrated trade) realtime push row.
///
/// **Structurally-verified** against the migration source's `US3Response`
/// (`generated/stock.rs`): a single object out-block. The integrated (KRX+NXT)
/// trade frame; this is a representative, spec-grounded trade-tick subset (it
/// additionally carries the integrated `exchname`/`ex_shcode` venue tags). Every
/// field is `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct US3Trade {
    /// Trade time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// Current (last-trade) price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Trade direction / 체결구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
    /// This-tick trade volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Accumulated trade value / 누적거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Trade strength / 체결강도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpower: String,
    /// Exchange name (KRX / NXT) / 거래소명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exchname: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Per-exchange short code / 거래소별단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// Decoded UH1 (통합 호가잔량 / integrated order-book) realtime push row.
///
/// **Structurally-verified** against the migration source's `UH1Response`
/// (`generated/stock.rs`): a single object out-block. The integrated 10-level
/// ladder splits remaining qty into KRX / NXT / 통합(unt) buckets; this is a
/// representative subset — level-1 prices plus the integrated totals. Every field
/// is `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct UH1Trade {
    /// Level-1 offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Level-1 bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Level-1 integrated offer remaining qty / 통합매도호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub unt_offerrem1: String,
    /// Level-1 integrated bid remaining qty / 통합매수호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub unt_bidrem1: String,
    /// Integrated total offer remaining qty / 통합총매도호가잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub unt_totofferrem: String,
    /// Integrated total bid remaining qty / 통합총매수호가잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub unt_totbidrem: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Per-exchange short code / 거래소별단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// Decoded US2 (통합 우선호가 / integrated best-quote) realtime push row.
///
/// **Structurally-verified** against the migration source's `US2Response`
/// (`generated/stock.rs`): a single object out-block — the integrated best-quote
/// pair (S2_ plus `ex_shcode`), four fields, the complete row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct US2Trade {
    /// Best offer (ask) / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Per-exchange short code / 거래소별단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// Decoded GSC (해외주식 체결 / overseas-stock trade) realtime push row.
///
/// **Structurally-verified** against the migration source's `GSCResponse`
/// (`generated/overseas_stock.rs`): a single object out-block keyed by `symbol`.
/// A representative, spec-grounded trade-tick subset. Note `lseq` rides the wire
/// as `lSeq` (kept via `#[serde(rename)]`). Every field is `string_or_number`-
/// coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct GSCTrade {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Trade time (local) / 체결시간(현지).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trdtm: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Trade price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rate: String,
    /// This-tick trade qty / 건별체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trdq: String,
    /// Accumulated trade qty / 누적체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totq: String,
    /// Trade direction / 체결구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
    /// Per-second sequence / 초당시퀀스 (wire key `lSeq`).
    #[serde(rename = "lSeq", deserialize_with = "ls_core::string_or_number")]
    pub lseq: String,
}

/// Decoded GSH (해외주식 호가 / overseas-stock order-book) realtime push row.
///
/// **Structurally-verified** against the migration source's `GSHResponse`
/// (`generated/overseas_stock.rs`): a single object out-block keyed by `symbol`.
/// The full row carries a 10-level ladder with per-level counts (`offerno`/
/// `bidno`); this is a representative top-level + totals subset. Every field is
/// `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct GSHTrade {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Order-book time (local) / 현지호가시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub loctime: String,
    /// Level-1 offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Level-1 bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Level-1 offer remaining qty / 매도호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Level-1 bid remaining qty / 매수호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Total offer remaining qty / 매도호가총수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totofferrem: String,
    /// Total bid remaining qty / 매수호가총수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totbidrem: String,
}

/// Decoded OVC (해외선물 체결 / overseas-futures trade) realtime push row.
///
/// **Structurally-verified** against the migration source's `OVCResponse`
/// (`generated/overseas_futures.rs`): a single object out-block keyed by `symbol`.
/// A representative, spec-grounded trade-tick subset. Note `ydiffsign` rides the
/// wire as `ydiffSign` (kept via `#[serde(rename)]`). Every field is
/// `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OVCTrade {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Trade time (local) / 체결시간(현지).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trdtm: String,
    /// Trade price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub curpr: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ydiffpr: String,
    /// Sign vs. previous close / 전일대비구분 (wire key `ydiffSign`).
    #[serde(rename = "ydiffSign", deserialize_with = "ls_core::string_or_number")]
    pub ydiffsign: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chgrate: String,
    /// This-tick trade qty / 건별체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trdq: String,
    /// Accumulated trade qty / 누적체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totq: String,
    /// Trade direction / 체결구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
}

/// Decoded OVH (해외선물 호가 / overseas-futures order-book) realtime push row.
///
/// **Structurally-verified** against the migration source's `OVHResponse`
/// (`generated/overseas_futures.rs`): a single object out-block keyed by `symbol`.
/// The full row carries a 5-level ladder with per-level counts; this is a
/// representative top-level + totals subset. Every field is `string_or_number`-
/// coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OVHTrade {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Order-book time / 호가시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
    /// Level-1 offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Level-1 bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Level-1 offer remaining qty / 매도호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Level-1 bid remaining qty / 매수호가잔량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Total offer remaining qty / 매도호가총수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totofferrem: String,
    /// Total bid remaining qty / 매수호가총수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totbidrem: String,
}

/// Decoded OC0 (KOSPI200 옵션 체결 / option trade) realtime push row.
///
/// **Structurally-verified** against the migration source's `OC0Response`
/// (`generated/futures_options.rs`): a single object out-block keyed by `optcode`.
/// A representative, spec-grounded subset — trade-tick fields plus the
/// option-specific greeks/지수 (`k200jisu`, `theoryprice`, `impv`, `openyak`).
/// Every field is `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OC0Trade {
    /// Trade time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// Current (last-trade) price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Trade direction / 체결구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
    /// This-tick trade volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제약정수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openyak: String,
    /// KOSPI200 index / KOSPI200지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k200jisu: String,
    /// Theoretical price / 이론가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub theoryprice: String,
    /// Implied volatility / 내재변동성.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub impv: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
}

/// Decoded OH0 (KOSPI200 옵션 호가 / option order-book) realtime push row.
///
/// **Structurally-verified** against the migration source's `OH0Response`
/// (`generated/futures_options.rs`): a single object out-block keyed by `optcode`.
/// The full row carries a 5-level ladder with per-level counts (`offercnt`/
/// `bidcnt`); this is a representative top-level + totals subset. Every field is
/// `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OH0Trade {
    /// Order-book time / 호가시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
    /// Level-1 offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Level-1 bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Level-1 offer remaining qty / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Level-1 bid remaining qty / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Total offer remaining qty / 총매도호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totofferrem: String,
    /// Total bid remaining qty / 총매수호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totbidrem: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
}

/// Decoded FC9 (KOSPI200 선물 체결 / futures trade) realtime push row.
///
/// **Structurally-verified** against the migration source's `FC9Response`
/// (`generated/futures_options.rs`): a single object out-block keyed by `futcode`.
/// A representative, spec-grounded subset — trade-tick fields plus the
/// futures-specific 지수/basis (`k200jisu`, `theoryprice`, `openyak`). Every field
/// is `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct FC9Trade {
    /// Trade time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Sign vs. previous close / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// Current (last-trade) price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Trade direction / 체결구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cgubun: String,
    /// This-tick trade volume / 체결량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제약정수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openyak: String,
    /// KOSPI200 index / KOSPI200지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k200jisu: String,
    /// Theoretical price / 이론가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub theoryprice: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
}

/// Decoded FH9 (KOSPI200 선물 호가 / futures order-book) realtime push row.
///
/// **Structurally-verified** against the migration source's `FH9Response`
/// (`generated/futures_options.rs`): a single object out-block keyed by `futcode`.
/// The full row carries a 5-level ladder with per-level counts (`offercnt`/
/// `bidcnt`); this is a representative top-level + totals subset. Every field is
/// `string_or_number`-coerced and `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct FH9Trade {
    /// Order-book time / 호가시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
    /// Level-1 offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Level-1 bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Level-1 offer remaining qty / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Level-1 bid remaining qty / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Total offer remaining qty / 총매도호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totofferrem: String,
    /// Total bid remaining qty / 총매수호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totbidrem: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
}

// =============================================================================
// P2 order-event lane: 주문/체결 event push rows (observation-only).
//
// These channels register with tr_type "1" (계좌등록) / deregister "2"
// (WsLane::OrderEvent). Per KTD6 they are NOT-OBSERVABLE on bare paper, so a
// clean lifecycle proves connection reachability only. Every <Base>Event struct
// is a SINGLE-object out-block (verified against its migration-source
// <Base>Response — each is a bare struct, not a Vec) and carries a
// representative, spec-grounded subset of order/execution fields with the real
// LS wire names (+ any #[serde(rename)] the source used). Account number and
// password fields are DELIBERATELY OMITTED (never test-surface a secret-shaped
// field). Every field is string_or_number-coerced and #[serde(default)] so a
// sparse registration-ACK body decodes without aborting the stream.
// =============================================================================

/// Decoded SC0 (주식 주문접수 / stock order-accept) realtime push row.
///
/// **Structurally-verified** against the migration source's `SC0Response`
/// (`korea-broker-sdk-ls` `generated/stock.rs`): a single-object out-block.
/// Account/password header fields are omitted; this is the order-identity subset.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC0Event {
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub orgordno: String,
    /// Order/execution classification / 주문체결구분 (01:주문 11:체결 14:거부 …).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordchegb: String,
    /// Order classification / 주문구분 (01:현금매도 02:현금매수 …).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordgb: String,
    /// Short stock code / 단축종목번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shtcode: String,
    /// Stock name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordprice: String,
}

/// Decoded SC1 (주식 주문체결 / stock order-fill) realtime push row.
///
/// **Structurally-verified** against the migration source's `SC1Response`
/// (`generated/stock.rs`): a single-object out-block. Preserves the source's
/// `shtnIsuno`/`Isunm` wire renames. Account fields omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC1Event {
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Execution number / 체결번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execno: String,
    /// Short stock code / 단축종목번호.
    #[serde(rename = "shtnIsuno", deserialize_with = "ls_core::string_or_number")]
    pub shtnisuno: String,
    /// Stock name / 종목명.
    #[serde(rename = "Isunm", deserialize_with = "ls_core::string_or_number")]
    pub isunm: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordprc: String,
    /// Execution qty / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execqty: String,
    /// Execution price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub execprc: String,
}

/// Decoded SC2 (주식 주문정정 / stock order-amend) realtime push row.
///
/// **Structurally-verified, no documented body fields** — the migration source's
/// `SC2Response` is a bare struct (single-object out-block; the gateway carries
/// the registration-ACK header only). Decodes any sparse body without aborting.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC2Event {}

/// Decoded SC3 (주식 주문취소 / stock order-cancel) realtime push row.
///
/// **Structurally-verified, no documented body fields** — bare-struct sibling of
/// `SC2Event`, mirroring the migration source's empty `SC3Response`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC3Event {}

/// Decoded SC4 (주식 주문거부 / stock order-reject) realtime push row.
///
/// **Structurally-verified, no documented body fields** — bare-struct sibling of
/// `SC2Event`, mirroring the migration source's empty `SC4Response`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct SC4Event {}

/// Decoded C01 (선물 주문체결 / F-O order-fill) realtime push row.
///
/// **Structurally-verified** against the migration source's `C01Response`
/// (`generated/futures_options.rs`): a single-object out-block. Account fields
/// omitted; this is the order/execution-identity subset.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct C01Event {
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordordno: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Execution price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cheprice: String,
    /// Execution qty / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chevol: String,
    /// Execution date / 체결일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chedate: String,
    /// Execution time / 체결시각.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// Sell/buy classification / 매도수구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dosugb: String,
}

/// Decoded O01 (선물 접수 / F-O order-accept) realtime push row.
///
/// **Structurally-verified, no documented body fields** — the migration source's
/// `O01Response` is a bare struct (single-object out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct O01Event {}

/// Decoded H01 (선물 주문정정취소 / F-O order-amend-cancel) realtime push row.
///
/// **Structurally-verified, no documented body fields** — the migration source's
/// `H01Response` is a bare struct (single-object out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct H01Event {}

/// Decoded AS0 (해외주식 주문접수(미국) / overseas-stock order-accept) push row.
///
/// **Structurally-verified** against the migration source's `AS0Response`
/// (`generated/overseas_stock.rs`): a single-object out-block carrying `s`-prefixed
/// wire names (renames preserved). Account fields omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS0Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Short item number / 단축종목번호.
    #[serde(rename = "sShtnIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sshtnisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded AS1 (해외주식 주문체결(미국) / overseas-stock order-fill) push row.
///
/// **Structurally-verified** against the migration source's `AS1Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; `s`-prefixed wire
/// renames preserved. Adds the execution fields over `AS0Event`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS1Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Short item number / 단축종목번호.
    #[serde(rename = "sShtnIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sshtnisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
    /// Execution number / 체결번호.
    #[serde(rename = "sExecNO", deserialize_with = "ls_core::string_or_number")]
    pub sexecno: String,
    /// Execution qty / 체결수량.
    #[serde(rename = "sExecQty", deserialize_with = "ls_core::string_or_number")]
    pub sexecqty: String,
    /// Execution price / 체결가.
    #[serde(rename = "sExecPrc", deserialize_with = "ls_core::string_or_number")]
    pub sexecprc: String,
}

/// Decoded AS2 (해외주식 주문정정(미국) / overseas-stock order-amend) push row.
///
/// **Structurally-verified** against the migration source's `AS2Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; `s`-prefixed renames
/// preserved (note `sIsuNo`, not `sShtnIsuNo`, on the amend/cancel/reject feeds).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS2Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Item number / 종목번호.
    #[serde(rename = "sIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded AS3 (해외주식 주문취소(미국) / overseas-stock order-cancel) push row.
///
/// **Structurally-verified** against the migration source's `AS3Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; schema-sibling of
/// `AS2Event` (`sIsuNo` short-code rename).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS3Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Item number / 종목번호.
    #[serde(rename = "sIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded AS4 (해외주식 주문거부(미국) / overseas-stock order-reject) push row.
///
/// **Structurally-verified** against the migration source's `AS4Response`
/// (`generated/overseas_stock.rs`): a single-object out-block; schema-sibling of
/// `AS2Event` (`sIsuNo` short-code rename).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct AS4Event {
    /// Order number / 주문번호.
    #[serde(rename = "sOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sordno: String,
    /// Original order number / 원주문번호.
    #[serde(rename = "sOrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub sorgordno: String,
    /// Item number / 종목번호.
    #[serde(rename = "sIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub sisuno: String,
    /// Item name / 종목명.
    #[serde(rename = "sIsuNm", deserialize_with = "ls_core::string_or_number")]
    pub sisunm: String,
    /// Order qty / 주문수량.
    #[serde(rename = "sOrdQty", deserialize_with = "ls_core::string_or_number")]
    pub sordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "sOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub sordprc: String,
}

/// Decoded TC1 (해외선물 주문접수 / overseas-futures order-accept) push row.
///
/// **Structurally-verified** against the migration source's `TC1Response`
/// (`generated/overseas_futures.rs`): a single-object out-block with snake_case
/// wire names (no renames). Account number (`ac_no`) omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct TC1Event {
    /// Service id / 서비스ID (HO01:주문ACK HO04:주문Pending).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svc_id: String,
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_no: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub orgn_ordr_no: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub is_cd: String,
    /// Sell/buy type / 매도매수유형 (1:매도 2:매수).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub s_b_ccd: String,
    /// Amend/cancel type / 정정취소유형 (1:신규 2:정정 3:취소).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_ccd: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_prc: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_q: String,
    /// Order time / 주문시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_tm: String,
}

/// Decoded TC2 (해외선물 주문응답 / overseas-futures order-response) push row.
///
/// **Structurally-verified** against the migration source's `TC2Response`
/// (`generated/overseas_futures.rs`): a single-object out-block; adds the
/// confirm/reject fields over `TC1Event`. Account number omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct TC2Event {
    /// Service id / 서비스ID (HO02:확인 HO03:거부).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svc_id: String,
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_no: String,
    /// Original order number / 원주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub orgn_ordr_no: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub is_cd: String,
    /// Amend/cancel type / 정정취소유형.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_ccd: String,
    /// Order price / 주문가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_prc: String,
    /// Order qty / 주문수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_q: String,
    /// Quote-confirm qty / 호가확인수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnfr_q: String,
    /// Quote-reject reason code / 호가거부사유코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rfsl_cd: String,
}

/// Decoded TC3 (해외선물 주문체결 / overseas-futures order-fill) push row.
///
/// **Structurally-verified** against the migration source's `TC3Response`
/// (`generated/overseas_futures.rs`): a single-object out-block; the execution
/// (체결) subset. Account number omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct TC3Event {
    /// Service id / 서비스ID (CH01).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svc_id: String,
    /// Order number / 주문번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ordr_no: String,
    /// Item code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub is_cd: String,
    /// Sell/buy type / 매도매수유형.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub s_b_ccd: String,
    /// Execution qty / 체결수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_q: String,
    /// Execution price / 체결가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_prc: String,
    /// Execution number / 체결번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_no: String,
    /// Execution time / 체결시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ccls_tm: String,
    /// Current price / 현재가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub now_prc: String,
}

// === Closure-flip WS batch (plan -004): 31 connection-reachable-only realtime
// push rows. Structurally-unverified, provisional — modelled from raw res_example
// single-object bodies; no live row observed (KTD1). ===

/// Decoded `NS3` ((NXT)체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ns3Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `mdchecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdchecnt: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `mschecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mschecnt: String,
    /// `mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// `w_avrg` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub w_avrg: String,
    /// `cpower` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpower: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
}

/// Decoded `NH1` ((NXT)호가잔량) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Nh1Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `offerho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    /// `offerho3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    /// `offerho6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho6: String,
    /// `offerho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho5: String,
    /// `offerho8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho8: String,
    /// `offerho7` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho7: String,
    /// `offerho9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho9: String,
}

/// Decoded `NS2` ((NXT)우선호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ns2Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bidho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// `ex_shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ex_shcode: String,
}

/// Decoded `NK1` ((NXT)거래원) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Nk1Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `tradmdrate1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate1: String,
    /// `tradmdvol5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol5: String,
    /// `tradmdvol3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol3: String,
    /// `tradmdrate3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate3: String,
    /// `tradmdrate2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate2: String,
    /// `tradmdvol4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol4: String,
    /// `offerno2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerno2: String,
}

/// Decoded `NBT` ((NXT)시간대별투자자매매추이) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NbtRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `mdvalue0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue0: String,
    /// `mdvalue1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue1: String,
    /// `msvolume8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume8: String,
    /// `msvolume9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume9: String,
    /// `msvolume4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume4: String,
    /// `mdvalue6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue6: String,
    /// `msvolume5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume5: String,
}

/// Decoded `KS_` (KOSDAQ우선호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct KsRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bidho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
}

/// Decoded `OK_` (KOSDAQ거래원) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OkRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `tradmdrate1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate1: String,
    /// `tradmdvol5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol5: String,
    /// `tradmdvol3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol3: String,
    /// `tradmdrate3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate3: String,
    /// `tradmdrate2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate2: String,
    /// `tradmdvol4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol4: String,
    /// `offerno2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerno2: String,
}

/// Decoded `KH_` (KOSDAQ프로그램매매종목별) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct KhRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bshrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bshrem: String,
    /// `cshvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cshvolume: String,
    /// `swcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub swcvolume: String,
    /// `tsvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tsvolume: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `dwcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dwcvolume: String,
    /// `djcvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub djcvalue: String,
}

/// Decoded `KM_` (KOSDAQ프로그램매매전체집계) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct KmRow {
    /// `gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
    /// `sjvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sjvalue: String,
    /// `p_bdvalcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_bdvalcha: String,
    /// `p_cdvalcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_cdvalcha: String,
    /// `k50sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub k50sign: String,
    /// `cwval` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cwval: String,
    /// `csjvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub csjvolume: String,
    /// `p_cvolcha` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_cvolcha: String,
}

/// Decoded `PH_` (KOSPI프로그램매매종목별) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct PhRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `bshrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bshrem: String,
    /// `cshvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cshvolume: String,
    /// `swcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub swcvolume: String,
    /// `tsvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tsvolume: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `dwcvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dwcvolume: String,
    /// `djcvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub djcvalue: String,
}

/// Decoded `K1_` (KOSPI거래원) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct K1Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `tradmdrate1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate1: String,
    /// `tradmdvol5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol5: String,
    /// `tradmdvol3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol3: String,
    /// `tradmdrate3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate3: String,
    /// `tradmdrate2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdrate2: String,
    /// `tradmdvol4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradmdvol4: String,
    /// `offerno2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerno2: String,
}

/// Decoded `IJ_` (지수) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct IjRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `cvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// `jisu` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// `highjisu` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highjisu: String,
    /// `upjo` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upjo: String,
    /// `highjo` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highjo: String,
    /// `value` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// Decoded `YS3` (KOSPI예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ys3Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `yofferrem0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yofferrem0: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `ybidho0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ybidho0: String,
    /// `yevolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yevolume: String,
    /// `hotime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
}

/// Decoded `YK3` (KOSDAQ예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Yk3Row {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `yofferrem0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yofferrem0: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `ybidho0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ybidho0: String,
    /// `yevolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yevolume: String,
    /// `hotime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hotime: String,
}

/// Decoded `VI_` (VI발동해제) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ViRow {
    /// `shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// `svi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svi_recprice: String,
    /// `vi_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vi_gubun: String,
    /// `time` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// `vi_trgprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vi_trgprice: String,
    /// `dvi_recprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dvi_recprice: String,
    /// `ref_shcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ref_shcode: String,
}

/// Decoded `JC0` (주식선물체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Jc0Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `mdchecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdchecnt: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `mschecnt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mschecnt: String,
    /// `ibasis` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ibasis: String,
    /// `mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// `cpower` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpower: String,
    /// `cvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
}

/// Decoded `JH0` (주식선물호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Jh0Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `offerho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    /// `offerho3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    /// `offerho6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho6: String,
    /// `offerho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho5: String,
    /// `offerho8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho8: String,
    /// `offerho7` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho7: String,
    /// `offerho9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho9: String,
}

/// Decoded `JD0` (주식선물실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Jd0Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `dy_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_gubun: String,
    /// `dy_uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_uplmtprice: String,
    /// `dy_dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_dnlmtprice: String,
    /// `gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// Decoded `FD0` (KOSPI200선물실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Fd0Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `dy_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_gubun: String,
    /// `dy_uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_uplmtprice: String,
    /// `dy_dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_dnlmtprice: String,
    /// `gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// Decoded `OD0` (KOSPI200옵션실시간상하한가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Od0Row {
    /// `opttcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub opttcode: String,
    /// `dy_gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_gubun: String,
    /// `dy_uplmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_uplmtprice: String,
    /// `dy_dnlmtprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dy_dnlmtprice: String,
    /// `gubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// Decoded `OMG` (KOSPI200옵션민감도) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OmgRow {
    /// `optcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
    /// `ceta` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ceta: String,
    /// `bidimpv` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidimpv: String,
    /// `fut200jisu` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fut200jisu: String,
    /// `delt` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub delt: String,
    /// `rhox` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rhox: String,
    /// `chetime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
    /// `price` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
}

/// Decoded `YF9` (지수선물예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Yf9Row {
    /// `futcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub futcode: String,
    /// `ychetime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ychetime: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `jnilydrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilydrate: String,
    /// `expct_ccls_q` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expct_ccls_q: String,
}

/// Decoded `YOC` (지수옵션예상체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct YocRow {
    /// `optcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
    /// `ychetime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ychetime: String,
    /// `jnilysign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilysign: String,
    /// `jnilchange` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilchange: String,
    /// `yeprice` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub yeprice: String,
    /// `jnilydrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilydrate: String,
    /// `expct_ccls_q` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expct_ccls_q: String,
}

/// Decoded `BM_` (업종별투자자별매매현황) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct BmRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `p_msval` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msval: String,
    /// `tjjtime` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjtime: String,
    /// `p_msvol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub p_msvol: String,
    /// `mdvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue: String,
    /// `msvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume: String,
    /// `tjjcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tjjcode: String,
    /// `msvalue` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvalue: String,
}

/// Decoded `WOC` (해외옵션 체결) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct WocRow {
    /// `symbol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// `chgrate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chgrate: String,
    /// `kordate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub kordate: String,
    /// `trdtm` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub trdtm: String,
    /// `curpr` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub curpr: String,
    /// `ovsdate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ovsdate: String,
    /// `mdvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvolume: String,
    /// `ydiffpr` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ydiffpr: String,
}

/// Decoded `WOH` (해외옵션 호가) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct WohRow {
    /// `symbol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// `offerrem2` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem2: String,
    /// `offerho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    /// `bidho5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho5: String,
    /// `offerho3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    /// `offerrem3` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem3: String,
    /// `bidho4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho4: String,
    /// `bidno1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidno1: String,
}

/// Decoded `JIF` (장운영정보) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct JifRow {
    /// `jangubun` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jangubun: String,
    /// `jstatus` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jstatus: String,
}

/// Decoded `NWS` (실시간뉴스제목패킷) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NwsRow {
    /// `code` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub code: String,
    /// `date` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// `realkey` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub realkey: String,
    /// `bodysize` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bodysize: String,
    /// `time` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// `id` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub id: String,
    /// `title` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub title: String,
}

/// Decoded `BMT` (시간대별투자자매매추이) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct BmtRow {
    /// `upcode` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
    /// `mdvalue0` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue0: String,
    /// `mdvalue1` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue1: String,
    /// `msvolume8` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume8: String,
    /// `msvolume9` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume9: String,
    /// `msvolume4` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume4: String,
    /// `mdvalue6` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mdvalue6: String,
    /// `msvolume5` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub msvolume5: String,
}

/// Decoded `CUR` (현물정보USD실시간) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct CurRow {
    /// `base_id` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub base_id: String,
    /// `offer` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// `high` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// `drate` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub drate: String,
    /// `low` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// `price` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
}

/// Decoded `MK2` (US지수) realtime push row.
///
/// **Structurally-unverified, provisional.** Modelled from the raw `res_example`
/// (a single-object body) for the closure-flip WS batch (plan -004); this channel
/// is flipped connection-reachable-only, so no live row is observed. A
/// representative, spec-grounded subset of the push row. Every field is
/// `string_or_number`-coerced and `#[serde(default)]` so both wire shapes — and a
/// sparse registration-ACK body — deserialize without a panic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Mk2Row {
    /// `xsymbol` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub xsymbol: String,
    /// `date` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// `change` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// `sign` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// `bidrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem: String,
    /// `offerho` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho: String,
    /// `cvolume` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cvolume: String,
    /// `offerrem` (raw wire field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_msg(msg: Message) -> serde_json::Value {
        let text = match msg {
            Message::Text(s) => s,
            _ => panic!("expected Message::Text"),
        };
        serde_json::from_str(&text).expect("valid json")
    }

    #[test]
    fn market_data_lane_register_deregister_pair() {
        assert_eq!(WsLane::MarketData.register(), "3");
        assert_eq!(WsLane::MarketData.deregister(), "4");
    }

    #[test]
    fn order_event_lane_register_deregister_pair() {
        assert_eq!(WsLane::OrderEvent.register(), "1");
        assert_eq!(WsLane::OrderEvent.deregister(), "2");
    }

    #[test]
    fn subscribe_msg_uses_tr_type_3_for_market_data() {
        let v = parse_msg(build_subscribe_msg("S3_", "005930", "tok_abc", WsLane::MarketData));
        assert_eq!(v["header"]["token"], "tok_abc");
        assert_eq!(v["header"]["tr_type"], "3");
        assert_eq!(v["body"]["tr_cd"], "S3_");
        assert_eq!(v["body"]["tr_key"], "005930");
    }

    #[test]
    fn unsubscribe_msg_uses_tr_type_4_for_market_data() {
        let v = parse_msg(build_unsubscribe_msg("S3_", "005930", "tok_abc", WsLane::MarketData));
        assert_eq!(v["header"]["tr_type"], "4");
        assert_eq!(v["body"]["tr_cd"], "S3_");
        assert_eq!(v["body"]["tr_key"], "005930");
    }

    #[test]
    fn subscribe_msg_uses_tr_type_1_for_order_event() {
        // Order-event channels (P2 lane) register with tr_type "1".
        let v = parse_msg(build_subscribe_msg("SC0", "", "tok_abc", WsLane::OrderEvent));
        assert_eq!(v["header"]["tr_type"], "1");
        assert_eq!(v["body"]["tr_cd"], "SC0");
    }

    #[test]
    fn unsubscribe_msg_uses_tr_type_2_for_order_event() {
        // The order-event deregister pair is "2".
        let v = parse_msg(build_unsubscribe_msg("SC0", "", "tok_abc", WsLane::OrderEvent));
        assert_eq!(v["header"]["tr_type"], "2");
        assert_eq!(v["body"]["tr_cd"], "SC0");
    }

    #[test]
    fn composite_key_round_trips() {
        let k = composite_key("S3_", "005930");
        assert_eq!(k, "S3_:005930");
        assert_eq!(split_composite_key(&k), ("S3_", "005930"));
    }

    #[test]
    fn s3_trade_decodes_real_fields_from_spec_example() {
        // The S3_ example body from specs/ls_openapi_specs.json (strings on wire).
        let body = serde_json::json!({
            "price": "55550",
            "cvolume": "1",
            "volume": "10887",
            "cgubun": "+",
            "shcode": "005930",
            "chetime": "090851",
            "sign": "2",
            "cpower": "332.56",
        });
        let row: S3Trade = serde_json::from_value(body).expect("decode S3_ body");
        assert_eq!(row.price, "55550");
        assert_eq!(row.cvolume, "1");
        assert_eq!(row.volume, "10887");
        assert_eq!(row.cgubun, "+");
        assert_eq!(row.shcode, "005930");
    }

    #[test]
    fn s3_trade_decodes_numeric_wire_shape() {
        // Regression for field-semantics: a numeric `price` (not a string) still
        // deserializes via string_or_number.
        let body = serde_json::json!({ "price": 55550, "volume": 10887 });
        let row: S3Trade = serde_json::from_value(body).expect("decode numeric S3_ body");
        assert_eq!(row.price, "55550");
        assert_eq!(row.volume, "10887");
    }

    #[test]
    fn k3_trade_decodes_single_object_body_from_migration_source_shape() {
        // K3_ (KOSDAQ 체결) out-block is a SINGLE object (verified against the
        // migration source's K3Response), with the same 체결 field names as S3_.
        let body = serde_json::json!({
            "price": "5550",
            "cvolume": "3",
            "volume": "204881",
            "value": "1136",
            "cgubun": "-",
            "shcode": "035720",
            "chetime": "091200",
            "sign": "5",
            "cpower": "88.21",
            "offerho": "5560",
            "bidho": "5550",
        });
        let row: K3Trade = serde_json::from_value(body).expect("decode K3_ body");
        assert_eq!(row.price, "5550");
        assert_eq!(row.cvolume, "3");
        assert_eq!(row.cgubun, "-");
        assert_eq!(row.shcode, "035720");
        assert_eq!(row.offerho, "5560");
    }

    #[test]
    fn k3_trade_decodes_numeric_wire_shape() {
        // string_or_number coercion: numeric price/volume still decode.
        let body = serde_json::json!({ "price": 5550, "volume": 204881 });
        let row: K3Trade = serde_json::from_value(body).expect("decode numeric K3_ body");
        assert_eq!(row.price, "5550");
        assert_eq!(row.volume, "204881");
    }

    // --- P1 market-data wave: per-TR single-object decode tests ---------------
    // Each TR's out-block is a SINGLE object (verified against its migration-
    // source `<Base>Response`), with these exact LS field names.

    #[test]
    fn h1_trade_decodes_single_object_order_book() {
        let body = serde_json::json!({
            "hotime": "090012",
            "offerho1": "55600", "bidho1": "55500",
            "offerrem1": "120", "bidrem1": "200",
            "offerho2": "55700", "bidho2": "55400",
            "totofferrem": "5000", "totbidrem": "4800",
            "volume": "10887", "shcode": "005930",
        });
        let row: H1Trade = serde_json::from_value(body).expect("decode H1_ body");
        assert_eq!(row.offerho1, "55600");
        assert_eq!(row.bidho1, "55500");
        assert_eq!(row.totofferrem, "5000");
        assert_eq!(row.shcode, "005930");
    }

    #[test]
    fn ha_trade_decodes_single_object_order_book() {
        let body = serde_json::json!({
            "hotime": "090013",
            "offerho1": "5560", "bidho1": "5550",
            "offerrem1": "30", "bidrem1": "40",
            "totofferrem": "900", "totbidrem": "800",
            "volume": "204881", "shcode": "035720",
        });
        let row: HATrade = serde_json::from_value(body).expect("decode HA_ body");
        assert_eq!(row.offerho1, "5560");
        assert_eq!(row.bidho1, "5550");
        assert_eq!(row.shcode, "035720");
    }

    #[test]
    fn s2_trade_decodes_single_object_best_quote() {
        let body = serde_json::json!({
            "offerho": "55600", "bidho": "55500", "shcode": "005930",
        });
        let row: S2Trade = serde_json::from_value(body).expect("decode S2_ body");
        assert_eq!(row.offerho, "55600");
        assert_eq!(row.bidho, "55500");
        assert_eq!(row.shcode, "005930");
    }

    #[test]
    fn us3_trade_decodes_single_object_trade() {
        let body = serde_json::json!({
            "chetime": "090851", "sign": "2", "price": "55550",
            "cgubun": "+", "cvolume": "1", "volume": "10887",
            "exchname": "KRX", "shcode": "005930", "ex_shcode": "005930",
        });
        let row: US3Trade = serde_json::from_value(body).expect("decode US3 body");
        assert_eq!(row.price, "55550");
        assert_eq!(row.exchname, "KRX");
        assert_eq!(row.shcode, "005930");
        assert_eq!(row.ex_shcode, "005930");
    }

    #[test]
    fn uh1_trade_decodes_single_object_integrated_order_book() {
        let body = serde_json::json!({
            "offerho1": "55600", "bidho1": "55500",
            "unt_offerrem1": "120", "unt_bidrem1": "200",
            "unt_totofferrem": "5000", "unt_totbidrem": "4800",
            "volume": "10887", "shcode": "005930", "ex_shcode": "005930",
        });
        let row: UH1Trade = serde_json::from_value(body).expect("decode UH1 body");
        assert_eq!(row.offerho1, "55600");
        assert_eq!(row.unt_totofferrem, "5000");
        assert_eq!(row.ex_shcode, "005930");
    }

    #[test]
    fn us2_trade_decodes_single_object_integrated_best_quote() {
        let body = serde_json::json!({
            "offerho": "55600", "bidho": "55500",
            "shcode": "005930", "ex_shcode": "005930",
        });
        let row: US2Trade = serde_json::from_value(body).expect("decode US2 body");
        assert_eq!(row.offerho, "55600");
        assert_eq!(row.ex_shcode, "005930");
    }

    #[test]
    fn gsc_trade_decodes_single_object_with_lseq_rename() {
        // GSC carries per-second seq on the wire as `lSeq` (note the capital S).
        let body = serde_json::json!({
            "symbol": "TSLA", "trdtm": "153000", "sign": "2",
            "price": "250.55", "diff": "1.20", "rate": "0.48",
            "trdq": "10", "totq": "1000000", "cgubun": "+", "lSeq": "42",
        });
        let row: GSCTrade = serde_json::from_value(body).expect("decode GSC body");
        assert_eq!(row.symbol, "TSLA");
        assert_eq!(row.price, "250.55");
        assert_eq!(row.lseq, "42", "lSeq wire key must map to lseq");
    }

    #[test]
    fn gsh_trade_decodes_single_object_order_book() {
        let body = serde_json::json!({
            "symbol": "TSLA", "loctime": "153000",
            "offerho1": "250.60", "bidho1": "250.50",
            "offerrem1": "100", "bidrem1": "120",
            "totofferrem": "5000", "totbidrem": "4800",
        });
        let row: GSHTrade = serde_json::from_value(body).expect("decode GSH body");
        assert_eq!(row.symbol, "TSLA");
        assert_eq!(row.offerho1, "250.60");
        assert_eq!(row.totbidrem, "4800");
    }

    #[test]
    fn ovc_trade_decodes_single_object_with_ydiffsign_rename() {
        // OVC carries sign on the wire as `ydiffSign` (note the capital S).
        let body = serde_json::json!({
            "symbol": "CLZ25", "trdtm": "153000", "curpr": "78.50",
            "ydiffpr": "0.30", "ydiffSign": "2", "chgrate": "0.38",
            "trdq": "5", "totq": "200000", "cgubun": "+",
        });
        let row: OVCTrade = serde_json::from_value(body).expect("decode OVC body");
        assert_eq!(row.symbol, "CLZ25");
        assert_eq!(row.curpr, "78.50");
        assert_eq!(row.ydiffsign, "2", "ydiffSign wire key must map to ydiffsign");
    }

    #[test]
    fn ovh_trade_decodes_single_object_order_book() {
        let body = serde_json::json!({
            "symbol": "CLZ25", "hotime": "153000",
            "offerho1": "78.51", "bidho1": "78.49",
            "offerrem1": "20", "bidrem1": "30",
            "totofferrem": "500", "totbidrem": "480",
        });
        let row: OVHTrade = serde_json::from_value(body).expect("decode OVH body");
        assert_eq!(row.symbol, "CLZ25");
        assert_eq!(row.offerho1, "78.51");
        assert_eq!(row.totbidrem, "480");
    }

    #[test]
    fn oc0_trade_decodes_single_object_option_trade() {
        let body = serde_json::json!({
            "chetime": "090851", "sign": "2", "price": "2.55",
            "cgubun": "+", "cvolume": "1", "volume": "1000",
            "openyak": "5000", "k200jisu": "350.50",
            "theoryprice": "2.60", "impv": "18.5", "optcode": "201TC325",
        });
        let row: OC0Trade = serde_json::from_value(body).expect("decode OC0 body");
        assert_eq!(row.price, "2.55");
        assert_eq!(row.k200jisu, "350.50");
        assert_eq!(row.optcode, "201TC325");
    }

    #[test]
    fn oh0_trade_decodes_single_object_option_order_book() {
        let body = serde_json::json!({
            "hotime": "090012",
            "offerho1": "2.56", "bidho1": "2.54",
            "offerrem1": "100", "bidrem1": "120",
            "totofferrem": "5000", "totbidrem": "4800", "optcode": "201TC325",
        });
        let row: OH0Trade = serde_json::from_value(body).expect("decode OH0 body");
        assert_eq!(row.offerho1, "2.56");
        assert_eq!(row.optcode, "201TC325");
    }

    #[test]
    fn fc9_trade_decodes_single_object_futures_trade() {
        let body = serde_json::json!({
            "chetime": "090851", "sign": "2", "price": "350.55",
            "cgubun": "+", "cvolume": "1", "volume": "100000",
            "openyak": "200000", "k200jisu": "350.50",
            "theoryprice": "350.60", "futcode": "101TC000",
        });
        let row: FC9Trade = serde_json::from_value(body).expect("decode FC9 body");
        assert_eq!(row.price, "350.55");
        assert_eq!(row.k200jisu, "350.50");
        assert_eq!(row.futcode, "101TC000");
    }

    #[test]
    fn fh9_trade_decodes_single_object_futures_order_book() {
        let body = serde_json::json!({
            "hotime": "090012",
            "offerho1": "350.60", "bidho1": "350.50",
            "offerrem1": "10", "bidrem1": "12",
            "totofferrem": "500", "totbidrem": "480", "futcode": "101TC000",
        });
        let row: FH9Trade = serde_json::from_value(body).expect("decode FH9 body");
        assert_eq!(row.offerho1, "350.60");
        assert_eq!(row.futcode, "101TC000");
    }

    #[test]
    fn p1_market_data_rows_coerce_numeric_wire_shape() {
        // string_or_number across the lane: numeric (not string) fields still
        // decode, covering each struct's numeric path once.
        let h1: H1Trade = serde_json::from_value(serde_json::json!({ "offerho1": 55600, "volume": 10887 })).unwrap();
        assert_eq!(h1.offerho1, "55600");
        let us3: US3Trade = serde_json::from_value(serde_json::json!({ "price": 55550 })).unwrap();
        assert_eq!(us3.price, "55550");
        let gsc: GSCTrade = serde_json::from_value(serde_json::json!({ "lSeq": 42 })).unwrap();
        assert_eq!(gsc.lseq, "42");
        let ovc: OVCTrade = serde_json::from_value(serde_json::json!({ "ydiffSign": 2 })).unwrap();
        assert_eq!(ovc.ydiffsign, "2");
        let oc0: OC0Trade = serde_json::from_value(serde_json::json!({ "k200jisu": 350 })).unwrap();
        assert_eq!(oc0.k200jisu, "350");
    }

    // --- P2 order-event wave: per-TR single-object decode tests --------------
    // Each TR's out-block is a SINGLE object (verified against its migration-
    // source `<Base>Response`). The empty bare-struct feeds (SC2/SC3/SC4/O01/H01)
    // decode any sparse/empty registration-ACK body without aborting the stream.

    #[test]
    fn sc0_event_decodes_single_object_order_accept() {
        let body = serde_json::json!({
            "ordno": "0000012345", "orgordno": "0000000000",
            "ordchegb": "01", "ordgb": "02",
            "shtcode": "005930", "hname": "삼성전자",
            "ordqty": "10", "ordprice": "55500",
        });
        let row: SC0Event = serde_json::from_value(body).expect("decode SC0 body");
        assert_eq!(row.ordno, "0000012345");
        assert_eq!(row.ordchegb, "01");
        assert_eq!(row.shtcode, "005930");
        assert_eq!(row.ordprice, "55500");
    }

    #[test]
    fn sc1_event_decodes_single_object_fill_with_renames() {
        // SC1 carries short-code as `shtnIsuno` and name as `Isunm` on the wire.
        let body = serde_json::json!({
            "ordno": "0000012345", "execno": "0000099999",
            "shtnIsuno": "005930", "Isunm": "삼성전자",
            "ordqty": "10", "ordprc": "55500",
            "execqty": "5", "execprc": "55500",
        });
        let row: SC1Event = serde_json::from_value(body).expect("decode SC1 body");
        assert_eq!(row.shtnisuno, "005930", "shtnIsuno wire key must map to shtnisuno");
        assert_eq!(row.isunm, "삼성전자", "Isunm wire key must map to isunm");
        assert_eq!(row.execprc, "55500");
        assert_eq!(row.execqty, "5");
    }

    #[test]
    fn empty_order_event_feeds_decode_sparse_body() {
        // SC2/SC3/SC4/O01/H01 are bare structs — a sparse or empty ACK body
        // (and unknown extra fields) decode without aborting the stream.
        let body = serde_json::json!({ "anything": "ignored" });
        let _sc2: SC2Event = serde_json::from_value(body.clone()).expect("SC2");
        let _sc3: SC3Event = serde_json::from_value(body.clone()).expect("SC3");
        let _sc4: SC4Event = serde_json::from_value(body.clone()).expect("SC4");
        let _o01: O01Event = serde_json::from_value(body.clone()).expect("O01");
        let _h01: H01Event = serde_json::from_value(serde_json::json!({})).expect("H01");
        assert_eq!(SC2Event::default(), _sc2);
    }

    #[test]
    fn c01_event_decodes_single_object_fo_fill() {
        let body = serde_json::json!({
            "ordno": "0000012345", "ordordno": "0000000000",
            "expcode": "101S6000", "cheprice": "350.50", "chevol": "3",
            "chedate": "20260624", "chetime": "153000", "dosugb": "2",
        });
        let row: C01Event = serde_json::from_value(body).expect("decode C01 body");
        assert_eq!(row.cheprice, "350.50");
        assert_eq!(row.chevol, "3");
        assert_eq!(row.expcode, "101S6000");
    }

    #[test]
    fn as0_event_decodes_single_object_with_s_renames() {
        // AS0 carries `s`-prefixed wire names.
        let body = serde_json::json!({
            "sOrdNo": "0000012345", "sOrgOrdNo": "0000000000",
            "sShtnIsuNo": "TSLA", "sIsuNm": "TESLA INC",
            "sOrdQty": "10", "sOrdPrc": "250.55",
        });
        let row: AS0Event = serde_json::from_value(body).expect("decode AS0 body");
        assert_eq!(row.sordno, "0000012345", "sOrdNo must map to sordno");
        assert_eq!(row.sshtnisuno, "TSLA");
        assert_eq!(row.sordprc, "250.55");
    }

    #[test]
    fn as1_event_decodes_single_object_fill() {
        let body = serde_json::json!({
            "sOrdNo": "0000012345", "sShtnIsuNo": "TSLA", "sIsuNm": "TESLA INC",
            "sOrdQty": "10", "sOrdPrc": "250.55",
            "sExecNO": "0000099999", "sExecQty": "5", "sExecPrc": "250.55",
        });
        let row: AS1Event = serde_json::from_value(body).expect("decode AS1 body");
        assert_eq!(row.sexecno, "0000099999", "sExecNO must map to sexecno");
        assert_eq!(row.sexecprc, "250.55");
    }

    #[test]
    fn as2_amend_event_decodes_single_object_with_sisuno() {
        // The amend/cancel/reject feeds carry the short-code as `sIsuNo`.
        let body = serde_json::json!({
            "sOrdNo": "0000012345", "sOrgOrdNo": "0000011111",
            "sIsuNo": "TSLA", "sIsuNm": "TESLA INC",
            "sOrdQty": "10", "sOrdPrc": "250.55",
        });
        let row: AS2Event = serde_json::from_value(body).expect("decode AS2 body");
        assert_eq!(row.sisuno, "TSLA", "sIsuNo must map to sisuno");
        assert_eq!(row.sorgordno, "0000011111");
        // AS3/AS4 share AS2's schema.
        let r3: AS3Event = serde_json::from_value(serde_json::json!({ "sIsuNo": "AAPL" })).unwrap();
        assert_eq!(r3.sisuno, "AAPL");
        let r4: AS4Event = serde_json::from_value(serde_json::json!({ "sIsuNo": "AAPL" })).unwrap();
        assert_eq!(r4.sisuno, "AAPL");
    }

    #[test]
    fn tc1_event_decodes_single_object_ovfut_accept() {
        let body = serde_json::json!({
            "svc_id": "HO01", "ordr_no": "0000012345", "orgn_ordr_no": "0000000000",
            "is_cd": "CLZ25", "s_b_ccd": "2", "ordr_ccd": "1",
            "ordr_prc": "75.55000000000", "ordr_q": "1", "ordr_tm": "153000",
        });
        let row: TC1Event = serde_json::from_value(body).expect("decode TC1 body");
        assert_eq!(row.svc_id, "HO01");
        assert_eq!(row.is_cd, "CLZ25");
        assert_eq!(row.ordr_q, "1");
    }

    #[test]
    fn tc2_event_decodes_single_object_ovfut_response() {
        let body = serde_json::json!({
            "svc_id": "HO02", "ordr_no": "0000012345", "orgn_ordr_no": "0000000000",
            "is_cd": "CLZ25", "ordr_ccd": "1", "ordr_prc": "75.55000000000",
            "ordr_q": "1", "cnfr_q": "1", "rfsl_cd": "0000",
        });
        let row: TC2Event = serde_json::from_value(body).expect("decode TC2 body");
        assert_eq!(row.svc_id, "HO02");
        assert_eq!(row.cnfr_q, "1");
        assert_eq!(row.rfsl_cd, "0000");
    }

    #[test]
    fn tc3_event_decodes_single_object_ovfut_fill() {
        let body = serde_json::json!({
            "svc_id": "CH01", "ordr_no": "0000012345", "is_cd": "CLZ25",
            "s_b_ccd": "2", "ccls_q": "1", "ccls_prc": "75.55000000000",
            "ccls_no": "0000099999", "ccls_tm": "153000", "now_prc": "75.60000000000",
        });
        let row: TC3Event = serde_json::from_value(body).expect("decode TC3 body");
        assert_eq!(row.ccls_prc, "75.55000000000");
        assert_eq!(row.ccls_no, "0000099999");
        assert_eq!(row.is_cd, "CLZ25");
    }

    #[test]
    fn p2_order_event_rows_coerce_numeric_wire_shape() {
        // string_or_number across the P2 lane: numeric (not string) wire fields
        // still decode, covering renamed and snake_case slots.
        let sc0: SC0Event = serde_json::from_value(serde_json::json!({ "ordqty": 10 })).unwrap();
        assert_eq!(sc0.ordqty, "10");
        let sc1: SC1Event = serde_json::from_value(serde_json::json!({ "execqty": 5 })).unwrap();
        assert_eq!(sc1.execqty, "5");
        let as0: AS0Event = serde_json::from_value(serde_json::json!({ "sOrdQty": 10 })).unwrap();
        assert_eq!(as0.sordqty, "10");
        let tc1: TC1Event = serde_json::from_value(serde_json::json!({ "ordr_q": 1 })).unwrap();
        assert_eq!(tc1.ordr_q, "1");
    }

    // === Closure-flip WS batch (plan -004) decode coverage ===
    #[test]
    fn ns3_row_decodes_single_object_body() {
        // NS3 ((NXT)체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "mdchecnt": "0", "sign": "0", "mschecnt": "0", "mdvolume": "0", "w_avrg": "0", "cpower": "0", "offerho": "0" });
        let row: Ns3Row = serde_json::from_value(body).expect("decode NS3 body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: Ns3Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn nh1_row_decodes_single_object_body() {
        // NH1 ((NXT)호가잔량) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "offerho4": "0", "offerho3": "0", "offerho6": "0", "offerho5": "0", "offerho8": "0", "offerho7": "0", "offerho9": "0" });
        let row: Nh1Row = serde_json::from_value(body).expect("decode NH1 body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: Nh1Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn ns2_row_decodes_single_object_body() {
        // NS2 ((NXT)우선호가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "bidho": "0", "offerho": "0", "ex_shcode": "0" });
        let row: Ns2Row = serde_json::from_value(body).expect("decode NS2 body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: Ns2Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn nk1_row_decodes_single_object_body() {
        // NK1 ((NXT)거래원) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "tradmdrate1": "0", "tradmdvol5": "0", "tradmdvol3": "0", "tradmdrate3": "0", "tradmdrate2": "0", "tradmdvol4": "0", "offerno2": "0" });
        let row: Nk1Row = serde_json::from_value(body).expect("decode NK1 body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: Nk1Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn nbt_row_decodes_single_object_body() {
        // NBT ((NXT)시간대별투자자매매추이) — single-object body from the raw res_example.
        let body = serde_json::json!({ "upcode": "0", "mdvalue0": "0", "mdvalue1": "0", "msvolume8": "0", "msvolume9": "0", "msvolume4": "0", "mdvalue6": "0", "msvolume5": "0" });
        let row: NbtRow = serde_json::from_value(body).expect("decode NBT body");
        assert_eq!(row.upcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "upcode": 0 });
        let r2: NbtRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.upcode, "0");
    }

    #[test]
    fn ks_row_decodes_single_object_body() {
        // KS_ (KOSDAQ우선호가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "bidho": "0", "offerho": "0" });
        let row: KsRow = serde_json::from_value(body).expect("decode KS_ body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: KsRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn ok_row_decodes_single_object_body() {
        // OK_ (KOSDAQ거래원) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "tradmdrate1": "0", "tradmdvol5": "0", "tradmdvol3": "0", "tradmdrate3": "0", "tradmdrate2": "0", "tradmdvol4": "0", "offerno2": "0" });
        let row: OkRow = serde_json::from_value(body).expect("decode OK_ body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: OkRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn kh_row_decodes_single_object_body() {
        // KH_ (KOSDAQ프로그램매매종목별) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "bshrem": "0", "cshvolume": "0", "swcvolume": "0", "tsvolume": "0", "sign": "0", "dwcvolume": "0", "djcvalue": "0" });
        let row: KhRow = serde_json::from_value(body).expect("decode KH_ body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: KhRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn km_row_decodes_single_object_body() {
        // KM_ (KOSDAQ프로그램매매전체집계) — single-object body from the raw res_example.
        let body = serde_json::json!({ "gubun": "0", "sjvalue": "0", "p_bdvalcha": "0", "p_cdvalcha": "0", "k50sign": "0", "cwval": "0", "csjvolume": "0", "p_cvolcha": "0" });
        let row: KmRow = serde_json::from_value(body).expect("decode KM_ body");
        assert_eq!(row.gubun, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "gubun": 0 });
        let r2: KmRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.gubun, "0");
    }

    #[test]
    fn ph_row_decodes_single_object_body() {
        // PH_ (KOSPI프로그램매매종목별) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "bshrem": "0", "cshvolume": "0", "swcvolume": "0", "tsvolume": "0", "sign": "0", "dwcvolume": "0", "djcvalue": "0" });
        let row: PhRow = serde_json::from_value(body).expect("decode PH_ body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: PhRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn k1_row_decodes_single_object_body() {
        // K1_ (KOSPI거래원) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "tradmdrate1": "0", "tradmdvol5": "0", "tradmdvol3": "0", "tradmdrate3": "0", "tradmdrate2": "0", "tradmdvol4": "0", "offerno2": "0" });
        let row: K1Row = serde_json::from_value(body).expect("decode K1_ body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: K1Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn ij_row_decodes_single_object_body() {
        // IJ_ (지수) — single-object body from the raw res_example.
        let body = serde_json::json!({ "upcode": "0", "sign": "0", "cvolume": "0", "jisu": "0", "highjisu": "0", "upjo": "0", "highjo": "0", "value": "0" });
        let row: IjRow = serde_json::from_value(body).expect("decode IJ_ body");
        assert_eq!(row.upcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "upcode": 0 });
        let r2: IjRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.upcode, "0");
    }

    #[test]
    fn ys3_row_decodes_single_object_body() {
        // YS3 (KOSPI예상체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "jnilysign": "0", "yofferrem0": "0", "jnilchange": "0", "yeprice": "0", "ybidho0": "0", "yevolume": "0", "hotime": "0" });
        let row: Ys3Row = serde_json::from_value(body).expect("decode YS3 body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: Ys3Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn yk3_row_decodes_single_object_body() {
        // YK3 (KOSDAQ예상체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "jnilysign": "0", "yofferrem0": "0", "jnilchange": "0", "yeprice": "0", "ybidho0": "0", "yevolume": "0", "hotime": "0" });
        let row: Yk3Row = serde_json::from_value(body).expect("decode YK3 body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: Yk3Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn vi_row_decodes_single_object_body() {
        // VI_ (VI발동해제) — single-object body from the raw res_example.
        let body = serde_json::json!({ "shcode": "005930", "svi_recprice": "0", "vi_gubun": "0", "time": "0", "vi_trgprice": "0", "dvi_recprice": "0", "ref_shcode": "0" });
        let row: ViRow = serde_json::from_value(body).expect("decode VI_ body");
        assert_eq!(row.shcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "shcode": 0 });
        let r2: ViRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.shcode, "0");
    }

    #[test]
    fn jc0_row_decodes_single_object_body() {
        // JC0 (주식선물체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "futcode": "0", "mdchecnt": "0", "sign": "0", "mschecnt": "0", "ibasis": "0", "mdvolume": "0", "cpower": "0", "cvolume": "0" });
        let row: Jc0Row = serde_json::from_value(body).expect("decode JC0 body");
        assert_eq!(row.futcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "futcode": 0 });
        let r2: Jc0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.futcode, "0");
    }

    #[test]
    fn jh0_row_decodes_single_object_body() {
        // JH0 (주식선물호가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "futcode": "0", "offerho4": "0", "offerho3": "0", "offerho6": "0", "offerho5": "0", "offerho8": "0", "offerho7": "0", "offerho9": "0" });
        let row: Jh0Row = serde_json::from_value(body).expect("decode JH0 body");
        assert_eq!(row.futcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "futcode": 0 });
        let r2: Jh0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.futcode, "0");
    }

    #[test]
    fn jd0_row_decodes_single_object_body() {
        // JD0 (주식선물실시간상하한가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "futcode": "0", "dy_gubun": "0", "dy_uplmtprice": "0", "dy_dnlmtprice": "0", "gubun": "0" });
        let row: Jd0Row = serde_json::from_value(body).expect("decode JD0 body");
        assert_eq!(row.futcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "futcode": 0 });
        let r2: Jd0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.futcode, "0");
    }

    #[test]
    fn fd0_row_decodes_single_object_body() {
        // FD0 (KOSPI200선물실시간상하한가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "futcode": "0", "dy_gubun": "0", "dy_uplmtprice": "0", "dy_dnlmtprice": "0", "gubun": "0" });
        let row: Fd0Row = serde_json::from_value(body).expect("decode FD0 body");
        assert_eq!(row.futcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "futcode": 0 });
        let r2: Fd0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.futcode, "0");
    }

    #[test]
    fn od0_row_decodes_single_object_body() {
        // OD0 (KOSPI200옵션실시간상하한가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "opttcode": "0", "dy_gubun": "0", "dy_uplmtprice": "0", "dy_dnlmtprice": "0", "gubun": "0" });
        let row: Od0Row = serde_json::from_value(body).expect("decode OD0 body");
        assert_eq!(row.opttcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "opttcode": 0 });
        let r2: Od0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.opttcode, "0");
    }

    #[test]
    fn omg_row_decodes_single_object_body() {
        // OMG (KOSPI200옵션민감도) — single-object body from the raw res_example.
        let body = serde_json::json!({ "optcode": "0", "ceta": "0", "bidimpv": "0", "fut200jisu": "0", "delt": "0", "rhox": "0", "chetime": "0", "price": "0" });
        let row: OmgRow = serde_json::from_value(body).expect("decode OMG body");
        assert_eq!(row.optcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "optcode": 0 });
        let r2: OmgRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.optcode, "0");
    }

    #[test]
    fn yf9_row_decodes_single_object_body() {
        // YF9 (지수선물예상체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "futcode": "0", "ychetime": "0", "jnilysign": "0", "jnilchange": "0", "yeprice": "0", "jnilydrate": "0", "expct_ccls_q": "0" });
        let row: Yf9Row = serde_json::from_value(body).expect("decode YF9 body");
        assert_eq!(row.futcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "futcode": 0 });
        let r2: Yf9Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.futcode, "0");
    }

    #[test]
    fn yoc_row_decodes_single_object_body() {
        // YOC (지수옵션예상체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "optcode": "0", "ychetime": "0", "jnilysign": "0", "jnilchange": "0", "yeprice": "0", "jnilydrate": "0", "expct_ccls_q": "0" });
        let row: YocRow = serde_json::from_value(body).expect("decode YOC body");
        assert_eq!(row.optcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "optcode": 0 });
        let r2: YocRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.optcode, "0");
    }

    #[test]
    fn bm_row_decodes_single_object_body() {
        // BM_ (업종별투자자별매매현황) — single-object body from the raw res_example.
        let body = serde_json::json!({ "upcode": "0", "p_msval": "0", "tjjtime": "0", "p_msvol": "0", "mdvalue": "0", "msvolume": "0", "tjjcode": "0", "msvalue": "0" });
        let row: BmRow = serde_json::from_value(body).expect("decode BM_ body");
        assert_eq!(row.upcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "upcode": 0 });
        let r2: BmRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.upcode, "0");
    }

    #[test]
    fn woc_row_decodes_single_object_body() {
        // WOC (해외옵션 체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "symbol": "0", "chgrate": "0", "kordate": "0", "trdtm": "0", "curpr": "0", "ovsdate": "0", "mdvolume": "0", "ydiffpr": "0" });
        let row: WocRow = serde_json::from_value(body).expect("decode WOC body");
        assert_eq!(row.symbol, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "symbol": 0 });
        let r2: WocRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.symbol, "0");
    }

    #[test]
    fn woh_row_decodes_single_object_body() {
        // WOH (해외옵션 호가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "symbol": "0", "offerrem2": "0", "offerho4": "0", "bidho5": "0", "offerho3": "0", "offerrem3": "0", "bidho4": "0", "bidno1": "0" });
        let row: WohRow = serde_json::from_value(body).expect("decode WOH body");
        assert_eq!(row.symbol, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "symbol": 0 });
        let r2: WohRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.symbol, "0");
    }

    #[test]
    fn jif_row_decodes_single_object_body() {
        // JIF (장운영정보) — single-object body from the raw res_example.
        let body = serde_json::json!({ "jangubun": "0", "jstatus": "0" });
        let row: JifRow = serde_json::from_value(body).expect("decode JIF body");
        assert_eq!(row.jangubun, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "jangubun": 0 });
        let r2: JifRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.jangubun, "0");
    }

    #[test]
    fn nws_row_decodes_single_object_body() {
        // NWS (실시간뉴스제목패킷) — single-object body from the raw res_example.
        let body = serde_json::json!({ "code": "0", "date": "0", "realkey": "0", "bodysize": "0", "time": "0", "id": "0", "title": "0" });
        let row: NwsRow = serde_json::from_value(body).expect("decode NWS body");
        assert_eq!(row.code, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "code": 0 });
        let r2: NwsRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.code, "0");
    }

    #[test]
    fn bmt_row_decodes_single_object_body() {
        // BMT (시간대별투자자매매추이) — single-object body from the raw res_example.
        let body = serde_json::json!({ "upcode": "0", "mdvalue0": "0", "mdvalue1": "0", "msvolume8": "0", "msvolume9": "0", "msvolume4": "0", "mdvalue6": "0", "msvolume5": "0" });
        let row: BmtRow = serde_json::from_value(body).expect("decode BMT body");
        assert_eq!(row.upcode, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "upcode": 0 });
        let r2: BmtRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.upcode, "0");
    }

    #[test]
    fn cur_row_decodes_single_object_body() {
        // CUR (현물정보USD실시간) — single-object body from the raw res_example.
        let body = serde_json::json!({ "base_id": "0", "offer": "0", "high": "0", "drate": "0", "low": "0", "price": "0", "change": "0", "sign": "0" });
        let row: CurRow = serde_json::from_value(body).expect("decode CUR body");
        assert_eq!(row.base_id, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "base_id": 0 });
        let r2: CurRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.base_id, "0");
    }

    #[test]
    fn mk2_row_decodes_single_object_body() {
        // MK2 (US지수) — single-object body from the raw res_example.
        let body = serde_json::json!({ "xsymbol": "0", "date": "0", "change": "0", "sign": "0", "bidrem": "0", "offerho": "0", "cvolume": "0", "offerrem": "0" });
        let row: Mk2Row = serde_json::from_value(body).expect("decode MK2 body");
        assert_eq!(row.xsymbol, "0");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "xsymbol": 0 });
        let r2: Mk2Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.xsymbol, "0");
    }

}
