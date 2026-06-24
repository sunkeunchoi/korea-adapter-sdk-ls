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
}
