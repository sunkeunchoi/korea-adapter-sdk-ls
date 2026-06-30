//! Market-data realtime trade/quote push rows (`S2`/`S3`/`K3`/`H1`/`FC9`/… Trade).
//!
//! Wave-2a split out of `realtime/frame.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use ws_trades::*;` from `frame.rs`, so the explicit `pub use frame::{…}`
//! name list in `realtime/mod.rs` resolves transitively.
use super::*;


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
