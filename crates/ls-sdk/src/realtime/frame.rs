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
//! **S3_ is a market-data (실시간 시세) channel**, so it registers with
//! `tr_type "3"` (실시간 시세 등록) and deregisters with `tr_type "4"` (실시간 시세
//! 해제). (Order-event channels register an account with `tr_type "1"/"2"`
//! instead; this slice ships only the market-data S3_ TR, so the builders use the
//! market-data `tr_type` directly — see `metadata/trs/S3_.yaml`.)
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

/// Build an LS WebSocket subscribe message for a market-data channel.
///
/// `{"header":{"token","tr_type":"3"},"body":{"tr_cd","tr_key"}}` — `tr_type "3"`
/// is 실시간 시세 등록 (realtime-quote register), the shape S3_ uses.
pub(crate) fn build_subscribe_msg(tr_cd: &str, tr_key: &str, token: &str) -> Message {
    build_frame(tr_cd, tr_key, token, "3")
}

/// Build an LS WebSocket unsubscribe message for a market-data channel.
///
/// `tr_type "4"` is 실시간 시세 해제 (realtime-quote deregister).
pub(crate) fn build_unsubscribe_msg(tr_cd: &str, tr_key: &str, token: &str) -> Message {
    build_frame(tr_cd, tr_key, token, "4")
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
    fn subscribe_msg_uses_tr_type_3_for_market_data() {
        let v = parse_msg(build_subscribe_msg("S3_", "005930", "tok_abc"));
        assert_eq!(v["header"]["token"], "tok_abc");
        assert_eq!(v["header"]["tr_type"], "3");
        assert_eq!(v["body"]["tr_cd"], "S3_");
        assert_eq!(v["body"]["tr_key"], "005930");
    }

    #[test]
    fn unsubscribe_msg_uses_tr_type_4_for_market_data() {
        let v = parse_msg(build_unsubscribe_msg("S3_", "005930", "tok_abc"));
        assert_eq!(v["header"]["tr_type"], "4");
        assert_eq!(v["body"]["tr_cd"], "S3_");
        assert_eq!(v["body"]["tr_key"], "005930");
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
}
