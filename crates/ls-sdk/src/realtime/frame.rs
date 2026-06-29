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

// ---- Per-channel-family struct modules (Wave-2a decomposition; pure
// relocation, re-exported so `realtime/mod.rs`'s explicit name list resolves). ----
mod ws_events;
pub use ws_events::*;
mod ws_trades;
pub use ws_trades::*;
mod ws_rows_batch;
pub use ws_rows_batch::*;
mod ws_rows_open;
pub use ws_rows_open::*;

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

    // === Open-window WS track-flip wave (plan 2026-06-29-001) decode coverage ===
    #[test]
    fn afr_row_decodes_single_object_body() {
        // AFR (API사용자조건검색실시간) — single-object body from the raw res_example.
        let body = serde_json::json!({ "gsJobFlag": "005930", "gsVolume": "0", "gsPrice": "0", "gsSign": "0", "gshname": "0", "gsChange": "0", "gsChgRate": "0", "gsCode": "0" });
        let row: AfrRow = serde_json::from_value(body).expect("decode AFR body");
        assert_eq!(row.gs_job_flag, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "gsJobFlag": 0 });
        let r2: AfrRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.gs_job_flag, "0");
    }

    #[test]
    fn b7_row_decodes_single_object_body() {
        // B7_ (ETF호가잔량) — single-object body from the raw res_example.
        let body = serde_json::json!({ "offerho4": "005930", "offerho3": "0", "offerho6": "0", "offerho5": "0", "offerho8": "0", "offerho7": "0", "offerho9": "0", "lp_bidho5": "0" });
        let row: B7Row = serde_json::from_value(body).expect("decode B7_ body");
        assert_eq!(row.offerho4, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "offerho4": 0 });
        let r2: B7Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.offerho4, "0");
    }

    #[test]
    fn c02_row_decodes_single_object_body() {
        // C02 (KRX야간파생 선물체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "mem_filler": "005930", "sihogagb": "0", "trcode": "0", "spdprc1": "0", "boardid": "0", "spdprc2": "0", "seq": "0", "yakseq": "0" });
        let row: C02Row = serde_json::from_value(body).expect("decode C02 body");
        assert_eq!(row.mem_filler, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "mem_filler": 0 });
        let r2: C02Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.mem_filler, "0");
    }

    #[test]
    fn cd0_row_decodes_single_object_body() {
        // CD0 (상품선물실시간상하한가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "futcode": "005930", "dy_gubun": "0", "dy_uplmtprice": "0", "dy_dnlmtprice": "0", "gubun": "0" });
        let row: Cd0Row = serde_json::from_value(body).expect("decode CD0 body");
        assert_eq!(row.futcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "futcode": 0 });
        let r2: Cd0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.futcode, "0");
    }

    #[test]
    fn dbm_row_decodes_single_object_body() {
        // DBM (KRX야간파생 투자자매매현황) — single-object body from the raw res_example.
        let body = serde_json::json!({ "p_msval": "005930", "tjjtime": "0", "p_msvol": "0", "mdvalue": "0", "fottjjcode": "0", "msvolume": "0", "tjjcode": "0", "msvalue": "0" });
        let row: DbmRow = serde_json::from_value(body).expect("decode DBM body");
        assert_eq!(row.p_msval, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "p_msval": 0 });
        let r2: DbmRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.p_msval, "0");
    }

    #[test]
    fn dbt_row_decodes_single_object_body() {
        // DBT (KRX야간파생 투자자별현황) — single-object body from the raw res_example.
        let body = serde_json::json!({ "mdvalue0": "005930", "mdvalue1": "0", "msvolume8": "0", "msvolume9": "0", "msvolume4": "0", "mdvalue6": "0", "msvolume5": "0", "mdvalue7": "0" });
        let row: DbtRow = serde_json::from_value(body).expect("decode DBT body");
        assert_eq!(row.mdvalue0, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "mdvalue0": 0 });
        let r2: DbtRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.mdvalue0, "0");
    }

    #[test]
    fn dc0_row_decodes_single_object_body() {
        // DC0 (KRX야간파생 체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "date": "005930", "futcode": "0", "mdchecnt": "0", "sign": "0", "mschecnt": "0", "ibasis": "0", "mdvolume": "0", "cpower": "0" });
        let row: Dc0Row = serde_json::from_value(body).expect("decode DC0 body");
        assert_eq!(row.date, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "date": 0 });
        let r2: Dc0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.date, "0");
    }

    #[test]
    fn dd0_row_decodes_single_object_body() {
        // DD0 (KRX야간파생 실시간상하한가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "futcode": "005930", "dy_gubun": "0", "dy_uplmtprice": "0", "dy_dnlmtprice": "0", "gubun": "0" });
        let row: Dd0Row = serde_json::from_value(body).expect("decode DD0 body");
        assert_eq!(row.futcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "futcode": 0 });
        let r2: Dd0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.futcode, "0");
    }

    #[test]
    fn dh0_row_decodes_single_object_body() {
        // DH0 (KRX야간파생 호가) — single-object body from the raw res_example.
        let body = serde_json::json!({ "offerrem2": "005930", "offerho4": "0", "bidho5": "0", "offerho3": "0", "offerrem3": "0", "bidho4": "0", "futcode": "0", "offerrem4": "0" });
        let row: Dh0Row = serde_json::from_value(body).expect("decode DH0 body");
        assert_eq!(row.offerrem2, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "offerrem2": 0 });
        let r2: Dh0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.offerrem2, "0");
    }

    #[test]
    fn dh1_row_decodes_single_object_body() {
        // DH1 (KOSPI시간외단일가호가잔량) — single-object body from the raw res_example.
        let body = serde_json::json!({ "dan_bidrem2": "005930", "dan_bidrem1": "0", "dan_preychange": "0", "dan_totbidrem": "0", "dan_jnilychange": "0", "dan_bidrem5": "0", "dan_totofferrem": "0", "dan_bidrem4": "0" });
        let row: Dh1Row = serde_json::from_value(body).expect("decode DH1 body");
        assert_eq!(row.dan_bidrem2, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "dan_bidrem2": 0 });
        let r2: Dh1Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.dan_bidrem2, "0");
    }

    #[test]
    fn dha_row_decodes_single_object_body() {
        // DHA (KOSDAQ시간외단일가호가잔량) — single-object body from the raw res_example.
        let body = serde_json::json!({ "dan_bidrem2": "005930", "dan_bidrem1": "0", "dan_preychange": "0", "dan_totbidrem": "0", "dan_jnilychange": "0", "dan_bidrem5": "0", "dan_totofferrem": "0", "dan_bidrem4": "0" });
        let row: DhaRow = serde_json::from_value(body).expect("decode DHA body");
        assert_eq!(row.dan_bidrem2, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "dan_bidrem2": 0 });
        let r2: DhaRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.dan_bidrem2, "0");
    }

    #[test]
    fn dk3_row_decodes_single_object_body() {
        // DK3 (KOSDAQ시간외단일가체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "dan_value": "005930", "dan_high": "0", "dan_mdvolume": "0", "dan_hightime": "0", "dan_mdchecnt": "0", "shcode": "0", "dan_precvolume": "0", "dan_price": "0" });
        let row: Dk3Row = serde_json::from_value(body).expect("decode DK3 body");
        assert_eq!(row.dan_value, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "dan_value": 0 });
        let r2: Dk3Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.dan_value, "0");
    }

    #[test]
    fn ds3_row_decodes_single_object_body() {
        // DS3 (KOSPI시간외단일가체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "dan_value": "005930", "dan_high": "0", "dan_mdvolume": "0", "dan_hightime": "0", "dan_mdchecnt": "0", "shcode": "0", "dan_precvolume": "0", "dan_price": "0" });
        let row: Ds3Row = serde_json::from_value(body).expect("decode DS3 body");
        assert_eq!(row.dan_value, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "dan_value": 0 });
        let r2: Ds3Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.dan_value, "0");
    }

    #[test]
    fn dvi_row_decodes_single_object_body() {
        // DVI (시간외단일가VI발동해제) — single-object body from the raw res_example.
        let body = serde_json::json!({ "svi_recprice": "005930", "vi_gubun": "0", "shcode": "0", "time": "0", "vi_trgprice": "0", "dvi_recprice": "0", "ref_shcode": "0" });
        let row: DviRow = serde_json::from_value(body).expect("decode DVI body");
        assert_eq!(row.svi_recprice, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "svi_recprice": 0 });
        let r2: DviRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.svi_recprice, "0");
    }

    #[test]
    fn esn_row_decodes_single_object_body() {
        // ESN (뉴ELW투자지표민감도) — single-object body from the raw res_example.
        let body = serde_json::json!({ "date": "005930", "ceta": "0", "elwclose": "0", "delt": "0", "shcode": "0", "change": "0", "sign": "0", "rhox": "0" });
        let row: EsnRow = serde_json::from_value(body).expect("decode ESN body");
        assert_eq!(row.date, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "date": 0 });
        let r2: EsnRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.date, "0");
    }

    #[test]
    fn fx9_row_decodes_single_object_body() {
        // FX9 (KOSPI200선물가격제한폭확대) — single-object body from the raw res_example.
        let body = serde_json::json!({ "upstep": "005930", "futcode": "0", "uplmtprice": "0", "dnstep": "0", "dnlmtprice": "0" });
        let row: Fx9Row = serde_json::from_value(body).expect("decode FX9 body");
        assert_eq!(row.upstep, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "upstep": 0 });
        let r2: Fx9Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.upstep, "0");
    }

    #[test]
    fn h02_row_decodes_single_object_body() {
        // H02 (KRX야간파생 선물정정취소) — single-object body from the raw res_example.
        let body = serde_json::json!({ "creditcode": "005930", "mem_filler": "0", "qty2": "0", "trcode": "0", "mocagb": "0", "price": "0", "boardid": "0", "accgb": "0" });
        let row: H02Row = serde_json::from_value(body).expect("decode H02 body");
        assert_eq!(row.creditcode, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "creditcode": 0 });
        let r2: H02Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.creditcode, "0");
    }

    #[test]
    fn h2_row_decodes_single_object_body() {
        // H2_ (KOSPI장전시간외호가잔량) — single-object body from the raw res_example.
        let body = serde_json::json!({ "tmbidrem": "005930", "shcode": "0", "pretmoffercha": "0", "pretmbidcha": "0", "tmofferrem": "0", "hotime": "0" });
        let row: H2Row = serde_json::from_value(body).expect("decode H2_ body");
        assert_eq!(row.tmbidrem, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "tmbidrem": 0 });
        let r2: H2Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.tmbidrem, "0");
    }

    #[test]
    fn hb_row_decodes_single_object_body() {
        // HB_ (KOSDAQ장전시간외호가잔량) — single-object body from the raw res_example.
        let body = serde_json::json!({ "tmbidrem": "005930", "shcode": "0", "pretmoffercha": "0", "pretmbidcha": "0", "tmofferrem": "0", "hotime": "0" });
        let row: HbRow = serde_json::from_value(body).expect("decode HB_ body");
        assert_eq!(row.tmbidrem, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "tmbidrem": 0 });
        let r2: HbRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.tmbidrem, "0");
    }

    #[test]
    fn i5_row_decodes_single_object_body() {
        // I5_ (코스피ETF종목실시간NAV) — single-object body from the raw res_example.
        let body = serde_json::json!({ "jirate": "005930", "nav": "0", "navchange": "0", "change": "0", "grate": "0", "shcode": "0", "sign": "0", "navdiff": "0" });
        let row: I5Row = serde_json::from_value(body).expect("decode I5_ body");
        assert_eq!(row.jirate, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "jirate": 0 });
        let r2: I5Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.jirate, "0");
    }

    #[test]
    fn jx0_row_decodes_single_object_body() {
        // JX0 (주식선물가격제한폭확대) — single-object body from the raw res_example.
        let body = serde_json::json!({ "upstep": "005930", "futcode": "0", "uplmtprice": "0", "dnstep": "0", "dnlmtprice": "0" });
        let row: Jx0Row = serde_json::from_value(body).expect("decode JX0 body");
        assert_eq!(row.upstep, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "upstep": 0 });
        let r2: Jx0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.upstep, "0");
    }

    #[test]
    fn nbm_row_decodes_single_object_body() {
        // NBM ((NXT)업종별투자자별매매현황) — single-object body from the raw res_example.
        let body = serde_json::json!({ "p_msval": "005930", "tjjtime": "0", "p_msvol": "0", "mdvalue": "0", "msvolume": "0", "upcode": "0", "ex_upcode": "0", "tjjcode": "0" });
        let row: NbmRow = serde_json::from_value(body).expect("decode NBM body");
        assert_eq!(row.p_msval, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "p_msval": 0 });
        let r2: NbmRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.p_msval, "0");
    }

    #[test]
    fn npm_row_decodes_single_object_body() {
        // NPM ((NXT)프로그램매매전체집계) — single-object body from the raw res_example.
        let body = serde_json::json!({ "sjvalue": "005930", "ex_gubun": "0", "p_bdvalcha": "0", "p_cdvalcha": "0", "cwval": "0", "csjvolume": "0", "k200basis": "0", "p_cvolcha": "0" });
        let row: NpmRow = serde_json::from_value(body).expect("decode NPM body");
        assert_eq!(row.sjvalue, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "sjvalue": 0 });
        let r2: NpmRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.sjvalue, "0");
    }

    #[test]
    fn nvi_row_decodes_single_object_body() {
        // NVI ((NXT)VI 발동 해제) — single-object body from the raw res_example.
        let body = serde_json::json!({ "svi_recprice": "005930", "vi_gubun": "0", "shcode": "0", "time": "0", "vi_trgprice": "0", "exchname": "0", "ex_shcode": "0", "dvi_recprice": "0" });
        let row: NviRow = serde_json::from_value(body).expect("decode NVI body");
        assert_eq!(row.svi_recprice, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "svi_recprice": 0 });
        let r2: NviRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.svi_recprice, "0");
    }

    #[test]
    fn o02_row_decodes_single_object_body() {
        // O02 (KRX야간파생 선물접수) — single-object body from the raw res_example.
        let body = serde_json::json!({ "grpId": "005930", "execprc2": "0", "execprc1": "0", "trchno": "0", "fnoIsuptntp": "0", "trcode": "0", "fnobalevaltp": "0", "avrprc_2": "0" });
        let row: O02Row = serde_json::from_value(body).expect("decode O02 body");
        assert_eq!(row.grp_id, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "grpId": 0 });
        let r2: O02Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.grp_id, "0");
    }

    #[test]
    fn ox0_row_decodes_single_object_body() {
        // OX0 (KOSPI200옵션가격제한폭확대) — single-object body from the raw res_example.
        let body = serde_json::json!({ "upstep": "005930", "opttcode": "0", "uplmtprice": "0", "dnstep": "0", "dnlmtprice": "0" });
        let row: Ox0Row = serde_json::from_value(body).expect("decode OX0 body");
        assert_eq!(row.upstep, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "upstep": 0 });
        let r2: Ox0Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.upstep, "0");
    }

    #[test]
    fn shc_row_decodes_single_object_body() {
        // SHC (상/하한가근접진입) — single-object body from the raw res_example.
        let body = serde_json::json!({ "wgubun": "005930", "dishonest": "0", "change": "0", "shcode": "0", "sign": "0", "tgubun": "0", "volume": "0", "sijanggubun": "0" });
        let row: ShcRow = serde_json::from_value(body).expect("decode SHC body");
        assert_eq!(row.wgubun, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "wgubun": 0 });
        let r2: ShcRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.wgubun, "0");
    }

    #[test]
    fn shd_row_decodes_single_object_body() {
        // SHD (상/하한가근접이탈) — single-object body from the raw res_example.
        let body = serde_json::json!({ "wgubun": "005930", "dishonest": "0", "change": "0", "shcode": "0", "sign": "0", "tgubun": "0", "volume": "0", "sijanggubun": "0" });
        let row: ShdRow = serde_json::from_value(body).expect("decode SHD body");
        assert_eq!(row.wgubun, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "wgubun": 0 });
        let r2: ShdRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.wgubun, "0");
    }

    #[test]
    fn shi_row_decodes_single_object_body() {
        // SHI (상/하한가진입) — single-object body from the raw res_example.
        let body = serde_json::json!({ "wgubun": "005930", "dishonest": "0", "change": "0", "shcode": "0", "sign": "0", "updnlmtstime": "0", "tgubun": "0", "volume": "0" });
        let row: ShiRow = serde_json::from_value(body).expect("decode SHI body");
        assert_eq!(row.wgubun, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "wgubun": 0 });
        let r2: ShiRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.wgubun, "0");
    }

    #[test]
    fn sho_row_decodes_single_object_body() {
        // SHO (상/하한가이탈) — single-object body from the raw res_example.
        let body = serde_json::json!({ "wgubun": "005930", "dishonest": "0", "change": "0", "shcode": "0", "sign": "0", "tgubun": "0", "volume": "0", "sijanggubun": "0" });
        let row: ShoRow = serde_json::from_value(body).expect("decode SHO body");
        assert_eq!(row.wgubun, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "wgubun": 0 });
        let r2: ShoRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.wgubun, "0");
    }

    #[test]
    fn ubm_row_decodes_single_object_body() {
        // UBM ((통합) 업종별투자자별매매현황) — single-object body from the raw res_example.
        let body = serde_json::json!({ "p_msval": "005930", "tjjtime": "0", "p_msvol": "0", "mdvalue": "0", "msvolume": "0", "upcode": "0", "ex_upcode": "0", "tjjcode": "0" });
        let row: UbmRow = serde_json::from_value(body).expect("decode UBM body");
        assert_eq!(row.p_msval, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "p_msval": 0 });
        let r2: UbmRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.p_msval, "0");
    }

    #[test]
    fn ubt_row_decodes_single_object_body() {
        // UBT ((통합)시간대별투자자매매추이) — single-object body from the raw res_example.
        let body = serde_json::json!({ "mdvalue0": "005930", "mdvalue1": "0", "msvolume8": "0", "msvolume9": "0", "msvolume4": "0", "mdvalue6": "0", "msvolume5": "0", "mdvalue7": "0" });
        let row: UbtRow = serde_json::from_value(body).expect("decode UBT body");
        assert_eq!(row.mdvalue0, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "mdvalue0": 0 });
        let r2: UbtRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.mdvalue0, "0");
    }

    #[test]
    fn uk1_row_decodes_single_object_body() {
        // UK1 ((통합)거래원) — single-object body from the raw res_example.
        let body = serde_json::json!({ "tradmdrate1": "005930", "tradmdvol5": "0", "tradmdvol3": "0", "tradmdrate3": "0", "tradmdrate2": "0", "tradmdvol4": "0", "offerno2": "0", "tradmdrate5": "0" });
        let row: Uk1Row = serde_json::from_value(body).expect("decode UK1 body");
        assert_eq!(row.tradmdrate1, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "tradmdrate1": 0 });
        let r2: Uk1Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.tradmdrate1, "0");
    }

    #[test]
    fn uvi_row_decodes_single_object_body() {
        // UVI ((통합)VI발동해제) — single-object body from the raw res_example.
        let body = serde_json::json!({ "krx_time": "005930", "shcode": "0", "krx_svi_recprice": "0", "nxt_svi_recprice": "0", "ex_shcode": "0", "krx_vi_gubun": "0", "krx_dvi_recprice": "0", "krx_vi_trgprice": "0" });
        let row: UviRow = serde_json::from_value(body).expect("decode UVI body");
        assert_eq!(row.krx_time, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "krx_time": 0 });
        let r2: UviRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.krx_time, "0");
    }

    #[test]
    fn uys_row_decodes_single_object_body() {
        // UYS ((통합)예상체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "jnilysign": "005930", "ybidho0": "0", "shcode": "0", "yevolume": "0", "ex_shcode": "0", "ybidrem0": "0", "jnilydrate": "0", "yofferho0": "0" });
        let row: UysRow = serde_json::from_value(body).expect("decode UYS body");
        assert_eq!(row.jnilysign, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "jnilysign": 0 });
        let r2: UysRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.jnilysign, "0");
    }

    #[test]
    fn yc3_row_decodes_single_object_body() {
        // YC3 (상품선물예상체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "ychetime": "005930", "jnilysign": "0", "jnilchange": "0", "yeprice": "0", "shcode": "0", "yevolume": "0", "jnilydrate": "0" });
        let row: Yc3Row = serde_json::from_value(body).expect("decode YC3 body");
        assert_eq!(row.ychetime, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "ychetime": 0 });
        let r2: Yc3Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.ychetime, "0");
    }

    #[test]
    fn yjc_row_decodes_single_object_body() {
        // YJC (주식선물예상체결) — single-object body from the raw res_example.
        let body = serde_json::json!({ "ychetime": "005930", "jnilysign": "0", "futcode": "0", "jnilchange": "0", "yeprice": "0", "jnilydrate": "0", "expct_ccls_q": "0" });
        let row: YjcRow = serde_json::from_value(body).expect("decode YJC body");
        assert_eq!(row.ychetime, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "ychetime": 0 });
        let r2: YjcRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.ychetime, "0");
    }

    #[test]
    fn yj_row_decodes_single_object_body() {
        // YJ_ (예상지수) — single-object body from the raw res_example.
        let body = serde_json::json!({ "jisu": "005930", "volume": "0", "drate": "0", "change": "0", "upcode": "0", "sign": "0", "time": "0", "value": "0" });
        let row: YjRow = serde_json::from_value(body).expect("decode YJ_ body");
        assert_eq!(row.jisu, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "jisu": 0 });
        let r2: YjRow = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.jisu, "0");
    }

    #[test]
    fn h3_row_decodes_single_object_body() {
        // h3_ (ELW호가잔량) — single-object body from the raw res_example.
        let body = serde_json::json!({ "offerho4": "005930", "offerho3": "0", "offerho6": "0", "offerho5": "0", "offerho8": "0", "offerho7": "0", "offerho9": "0", "lp_bidho5": "0" });
        let row: H3Row = serde_json::from_value(body).expect("decode h3_ body");
        assert_eq!(row.offerho4, "005930");
        // numeric wire shape also coerces.
        let numbody = serde_json::json!({ "offerho4": 0 });
        let r2: H3Row = serde_json::from_value(numbody).expect("numeric coerces");
        assert_eq!(r2.offerho4, "0");
    }


}
