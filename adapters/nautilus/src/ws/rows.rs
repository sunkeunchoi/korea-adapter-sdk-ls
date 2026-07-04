//! Adapter-owned WebSocket frame rows (KTD8).
//!
//! `subscribe_typed::<Res>` is generic, so the adapter defines its own frame
//! structs with `#[serde(default)]` + tolerant string parsing rather than reusing
//! the SDK's. The KOSPI/KOSDAQ trade rows (S3_/K3_) are field-identical, as are the
//! order-book rows (H1_/HA_), so one [`TradeRow`] and one [`BookRow`] cover both
//! segments; the segment is carried by the `tr_cd` the adapter subscribes with, not
//! the row shape. v1 consumes trades + top-of-book. NOTE (corrected 2026-07-02):
//! [`BookRow`] decodes only levels 1–2 + book totals, **not** the full 10-level
//! ladder — full-depth (`OrderBookDeltas`/`Depth10`) is *new* decode work, not a
//! purely-additive extension (the v1 plan's "already decode the full ladder" was an
//! error).

use serde::Deserialize;

use nautilus_common::messages::DataEvent;
use nautilus_core::UnixNanos;
use nautilus_model::data::{Data, QuoteTick, TradeTick};
use nautilus_model::enums::AggressorSide;
use nautilus_model::identifiers::{InstrumentId, TradeId};
use nautilus_model::types::{Price, Quantity};

use crate::orders::ledger::FillObservation;
use crate::parse::lossy_i64;

fn price0(s: &str) -> Price {
    Price::from(lossy_i64(s).max(0).to_string().as_str())
}

fn qty0(s: &str) -> Quantity {
    Quantity::from(lossy_i64(s).max(0))
}

/// An S3_ (KOSPI) or K3_ (KOSDAQ) real-time trade row. All fields are strings on
/// the wire (`string_or_number`); `#[serde(default)]` lets a registration-ACK
/// (all-default) row decode without aborting.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TradeRow {
    /// Trade time / 체결시간 (HHMMSS).
    pub chetime: String,
    /// Trade-side flag / 체결구분.
    pub cgubun: String,
    /// Last price / 현재가.
    pub price: String,
    /// Trade volume for this print / 체결량.
    pub cvolume: String,
    /// Cumulative volume / 누적거래량.
    pub volume: String,
    /// Short code / 단축코드.
    pub shcode: String,
}

impl TradeRow {
    /// Whether this is a registration-ACK / all-default row (filtered from
    /// emission). A real trade always carries a short code.
    pub fn is_ack(&self) -> bool {
        self.shcode.trim().is_empty()
    }

    /// Convert to a nautilus [`TradeTick`] for `instrument_id`, stamped `ts`.
    pub fn to_data(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<Data> {
        if self.is_ack() {
            return None;
        }
        let aggressor = match self.cgubun.trim() {
            "1" | "+" | "매수" => AggressorSide::Buyer,
            "2" | "-" | "매도" => AggressorSide::Seller,
            _ => AggressorSide::NoAggressor,
        };
        let trade_id = TradeId::from(
            format!(
                "{}-{}-{}",
                self.shcode.trim(),
                self.chetime.trim(),
                self.volume.trim()
            )
            .as_str(),
        );
        let tick = TradeTick::new(
            instrument_id,
            price0(&self.price),
            qty0(&self.cvolume),
            aggressor,
            trade_id,
            ts,
            ts,
        );
        Some(Data::Trade(tick))
    }
}

/// An H1_ (KOSPI) or HA_ (KOSDAQ) order-book row. The full top-2 ladder + book
/// totals are decoded; v1 emits top-of-book (level 1) as a [`QuoteTick`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BookRow {
    /// Quote time / 호가시간.
    pub hotime: String,
    /// Best ask price / 매도호가1.
    pub offerho1: String,
    /// Best bid price / 매수호가1.
    pub bidho1: String,
    /// Best ask size / 매도잔량1.
    pub offerrem1: String,
    /// Best bid size / 매수잔량1.
    pub bidrem1: String,
    /// Ask price level 2 / 매도호가2 (decoded; unused in v1 top-of-book).
    pub offerho2: String,
    /// Bid price level 2 / 매수호가2 (decoded; unused in v1 top-of-book).
    pub bidho2: String,
    /// Total ask size / 총매도잔량.
    pub totofferrem: String,
    /// Total bid size / 총매수잔량.
    pub totbidrem: String,
    /// Short code / 단축코드.
    pub shcode: String,
}

impl BookRow {
    /// Whether this is a registration-ACK / all-default row.
    pub fn is_ack(&self) -> bool {
        self.shcode.trim().is_empty()
    }

    /// Convert to a nautilus top-of-book [`QuoteTick`] for `instrument_id`.
    pub fn to_data(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<Data> {
        if self.is_ack() {
            return None;
        }
        let quote = QuoteTick::new(
            instrument_id,
            price0(&self.bidho1),
            price0(&self.offerho1),
            qty0(&self.bidrem1),
            qty0(&self.offerrem1),
            ts,
            ts,
        );
        Some(Data::Quote(quote))
    }
}

/// A WS row that decodes to a lane event `E` (blanket over the row types), so the
/// supervisor's reader task is generic over the market-data lane (`E = DataEvent`)
/// and the order-event lane (`E = OrderEventMsg`) alike (KTD3, R5).
pub trait ToEvent<E>: for<'de> Deserialize<'de> + Send + 'static {
    /// Whether this row is a registration-ACK (filtered from emission).
    fn is_ack(&self) -> bool;
    /// Convert to a lane event `E`, or `None` if it is an ACK / carries nothing.
    fn to_event(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<E>;
}

impl ToEvent<DataEvent> for TradeRow {
    fn is_ack(&self) -> bool {
        TradeRow::is_ack(self)
    }
    fn to_event(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<DataEvent> {
        TradeRow::to_data(self, instrument_id, ts).map(DataEvent::Data)
    }
}

impl ToEvent<DataEvent> for BookRow {
    fn is_ack(&self) -> bool {
        BookRow::is_ack(self)
    }
    fn to_event(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<DataEvent> {
        BookRow::to_data(self, instrument_id, ts).map(DataEvent::Data)
    }
}

/// A decoded order-event-lane message (KTD3). SC1 fills become [`FillObservation`]s
/// for the ledger; SC0 accepts are a chain cross-check signal (the modify/cancel
/// chain is authoritatively driven by the synchronous REST acks, KTD4).
#[derive(Debug, Clone)]
pub enum OrderEventMsg {
    /// SC0 — an order was accepted at the venue: its OrdNo (+ parent OrdNo, if this
    /// is a modify/cancel acceptance).
    Accept {
        /// The accepted order number.
        ord_no: String,
        /// The parent (original) order number, if any.
        org_ord_no: String,
    },
    /// SC1 — an execution (fill) observation for the ledger.
    Fill(FillObservation),
}

/// SC0 — order-accept (주식 주문접수) row on the OrderEvent lane. Wired in this
/// increment (U2): an accept is a chain cross-check signal ([`OrderEventMsg::Accept`]).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Sc0Row {
    /// Order number / 주문번호.
    pub ordno: String,
    /// Original order number / 원주문번호.
    pub orgordno: String,
    /// Order/fill flag / 주문체결구분.
    pub ordchegb: String,
    /// Symbol / 단축코드.
    pub shtcode: String,
    /// Order quantity / 주문수량.
    pub ordqty: String,
    /// Order price / 주문가격.
    pub ordprice: String,
}

impl Sc0Row {
    /// Whether this is a registration-ACK / all-default row (no order number).
    pub fn is_ack(&self) -> bool {
        self.ordno.trim().is_empty()
    }
}

impl ToEvent<OrderEventMsg> for Sc0Row {
    fn is_ack(&self) -> bool {
        Sc0Row::is_ack(self)
    }
    fn to_event(&self, _instrument_id: InstrumentId, _ts: UnixNanos) -> Option<OrderEventMsg> {
        if self.is_ack() {
            return None;
        }
        Some(OrderEventMsg::Accept {
            ord_no: self.ordno.trim().to_string(),
            org_ord_no: self.orgordno.trim().to_string(),
        })
    }
}

/// Normalize an SC issue-code field to the bare short code the ledger keys on
/// (U1, KTD3): trim, drop a leading `A` exchange prefix (the inverse of the form
/// the cancel path builds with `format!("A{shcode}")`), and treat a
/// blank/whitespace value as **absent** — an empty symbol must never seed a
/// pending-reconcile t0425 call (the flat-scan/IGW00201 class R3 bans).
pub(crate) fn normalize_symbol(raw: &str) -> Option<String> {
    let t = raw.trim();
    let t = t.strip_prefix('A').unwrap_or(t);
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// SC1 — order-fill (주식 주문체결) row on the OrderEvent lane. Wired in this
/// increment (U2): a fill becomes a [`FillObservation`] fed to the ledger.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Sc1Row {
    /// Order number / 주문번호.
    pub ordno: String,
    /// Execution number / 체결번호.
    pub execno: String,
    /// Order quantity / 주문수량.
    pub ordqty: String,
    /// Order price / 주문가격.
    pub ordprc: String,
    /// Executed (filled) quantity / 체결수량.
    pub execqty: String,
    /// Executed (fill) price / 체결가격.
    pub execprc: String,
    /// Short issue code / 단축종목번호 — the traded symbol (U1, KTD3). May carry
    /// an `A` prefix; normalized to the bare short code the ledger keys on, so an
    /// unknown-order fill can record its symbol as pending reconciliation (R1).
    #[serde(rename = "shtnIsuno")]
    pub shtn_isuno: String,
}

impl Sc1Row {
    /// Whether this is a registration-ACK / all-default row.
    pub fn is_ack(&self) -> bool {
        self.ordno.trim().is_empty()
    }

    /// The filled quantity as an integer (0 if unparseable/blank).
    pub fn exec_qty(&self) -> i64 {
        lossy_i64(&self.execqty).max(0)
    }

    /// The fill price as an integer KRW (0 if unparseable/blank).
    pub fn exec_price(&self) -> i64 {
        lossy_i64(&self.execprc).max(0)
    }
}

impl ToEvent<OrderEventMsg> for Sc1Row {
    fn is_ack(&self) -> bool {
        Sc1Row::is_ack(self)
    }
    fn to_event(&self, _instrument_id: InstrumentId, _ts: UnixNanos) -> Option<OrderEventMsg> {
        // An ACK (no order number) or a zero-quantity / execno-less frame is not a
        // fill — filtered from emission exactly like a market-data ACK row.
        if self.is_ack() || self.execno.trim().is_empty() || self.exec_qty() == 0 {
            return None;
        }
        Some(OrderEventMsg::Fill(
            FillObservation::sc(
                self.ordno.trim(),
                self.exec_qty(),
                self.exec_price(),
                self.execno.trim(),
            )
            // Carry the traded symbol (U1, KTD3) so the ledger can record an
            // unknown order's symbol as pending reconciliation. A blank/whitespace
            // issue code degrades to no symbol (the unknown-order arm falls back to
            // today's bare armed wakeup).
            .with_symbol(normalize_symbol(&self.shtn_isuno)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sc1_fill_row_extracts_qty_and_price() {
        let row = Sc1Row {
            ordno: "1001".to_string(),
            execqty: "5".to_string(),
            execprc: "60500".to_string(),
            ..Default::default()
        };
        assert!(!row.is_ack());
        assert_eq!(row.exec_qty(), 5);
        assert_eq!(row.exec_price(), 60500);
        assert!(Sc1Row::default().is_ack());
    }

    /// U1: an SC1 fill frame carrying the baseline `shtnIsuno` symbol key
    /// deserializes and the observation exposes the bare symbol.
    #[test]
    fn sc1_frame_with_shtn_isuno_carries_bare_symbol() {
        let row: Sc1Row = serde_json::from_value(serde_json::json!({
            "ordno": "1001", "execno": "E1", "execqty": "5", "execprc": "60500",
            "shtnIsuno": "005930"
        }))
        .unwrap();
        assert_eq!(row.shtn_isuno, "005930");
        let ev = row.to_event(InstrumentId::from("SC.XKRX"), UnixNanos::default()).unwrap();
        match ev {
            OrderEventMsg::Fill(obs) => assert_eq!(obs.symbol.as_deref(), Some("005930")),
            other => panic!("expected a fill, got {other:?}"),
        }
    }

    /// U1: an `A`-prefixed issue code normalizes to the bare short code (the
    /// inverse of the form the cancel path builds).
    #[test]
    fn sc1_a_prefixed_issue_code_normalizes() {
        assert_eq!(normalize_symbol("A005930").as_deref(), Some("005930"));
        assert_eq!(normalize_symbol("  A005930 ").as_deref(), Some("005930"));
        let row = Sc1Row {
            ordno: "1001".to_string(),
            execno: "E1".to_string(),
            execqty: "5".to_string(),
            execprc: "60500".to_string(),
            shtn_isuno: "A005930".to_string(),
            ..Default::default()
        };
        match row.to_event(InstrumentId::from("SC.XKRX"), UnixNanos::default()).unwrap() {
            OrderEventMsg::Fill(obs) => assert_eq!(obs.symbol.as_deref(), Some("005930")),
            other => panic!("expected a fill, got {other:?}"),
        }
    }

    /// U1 (KTD3 guard): a blank/whitespace symbol yields an observation carrying
    /// no symbol — the ledger then records nothing pending and no empty-expcode
    /// t0425 call can occur.
    #[test]
    fn sc1_blank_symbol_yields_no_observation_symbol() {
        assert_eq!(normalize_symbol("   "), None);
        assert_eq!(normalize_symbol(""), None);
        let row = Sc1Row {
            ordno: "1001".to_string(),
            execno: "E1".to_string(),
            execqty: "5".to_string(),
            execprc: "60500".to_string(),
            shtn_isuno: "   ".to_string(),
            ..Default::default()
        };
        match row.to_event(InstrumentId::from("SC.XKRX"), UnixNanos::default()).unwrap() {
            OrderEventMsg::Fill(obs) => assert!(obs.symbol.is_none(), "a blank symbol is absent"),
            other => panic!("expected a fill, got {other:?}"),
        }
    }

    #[test]
    fn trade_row_converts_to_trade_tick() {
        let row = TradeRow {
            chetime: "090001".to_string(),
            cgubun: "1".to_string(),
            price: "60500".to_string(),
            cvolume: "10".to_string(),
            volume: "12345".to_string(),
            shcode: "005930".to_string(),
        };
        let ts = UnixNanos::from(1_700_000_000_000_000_000u64);
        let data = row.to_data(InstrumentId::from("005930.XKRX"), ts).unwrap();
        match data {
            Data::Trade(t) => {
                assert_eq!(t.price, Price::from("60500"));
                assert_eq!(t.size, Quantity::from(10));
                assert_eq!(t.aggressor_side, AggressorSide::Buyer);
                assert_eq!(t.ts_event, ts);
            }
            other => panic!("expected a trade, got {other:?}"),
        }
    }

    #[test]
    fn ack_row_yields_nothing() {
        let row = TradeRow::default();
        assert!(row.is_ack());
        assert!(row
            .to_data(InstrumentId::from("005930.XKRX"), UnixNanos::default())
            .is_none());
    }

    #[test]
    fn book_row_converts_to_top_of_book_quote() {
        let row = BookRow {
            hotime: "090002".to_string(),
            offerho1: "60600".to_string(),
            bidho1: "60500".to_string(),
            offerrem1: "100".to_string(),
            bidrem1: "200".to_string(),
            shcode: "005930".to_string(),
            ..Default::default()
        };
        let data = row
            .to_data(InstrumentId::from("005930.XKRX"), UnixNanos::from(1u64))
            .unwrap();
        match data {
            Data::Quote(q) => {
                assert_eq!(q.bid_price, Price::from("60500"));
                assert_eq!(q.ask_price, Price::from("60600"));
                assert_eq!(q.bid_size, Quantity::from(200));
                assert_eq!(q.ask_size, Quantity::from(100));
            }
            other => panic!("expected a quote, got {other:?}"),
        }
    }
}
