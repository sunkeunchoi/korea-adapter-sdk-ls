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
}
