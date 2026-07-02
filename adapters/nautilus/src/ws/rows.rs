//! Adapter-owned WebSocket frame rows (KTD8).
//!
//! `subscribe_typed::<Res>` is generic, so the adapter defines its own frame
//! structs with `#[serde(default)]` + tolerant string parsing rather than reusing
//! the SDK's. The KOSPI/KOSDAQ trade rows (S3_/K3_) are field-identical, as are the
//! order-book rows (H1_/HA_), so one [`TradeRow`] and one [`BookRow`] cover both
//! segments; the segment is carried by the `tr_cd` the adapter subscribes with, not
//! the row shape. v1 consumes trades + top-of-book; the full ladder is decoded so
//! depth is additive later.

use serde::Deserialize;

use nautilus_core::UnixNanos;
use nautilus_model::data::{Data, QuoteTick, TradeTick};
use nautilus_model::enums::AggressorSide;
use nautilus_model::identifiers::{InstrumentId, TradeId};
use nautilus_model::types::{Price, Quantity};

fn i64_from(s: &str) -> i64 {
    let t = s.trim();
    if t.is_empty() {
        0
    } else if let Ok(i) = t.parse::<i64>() {
        i
    } else if let Ok(f) = t.parse::<f64>() {
        f.trunc() as i64
    } else {
        0
    }
}

fn price0(s: &str) -> Price {
    Price::from(i64_from(s).max(0).to_string().as_str())
}

fn qty0(s: &str) -> Quantity {
    Quantity::from(i64_from(s).max(0))
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

/// A row that decodes to a nautilus [`Data`] event (blanket over the WS row types),
/// so the reader task is generic over trade vs quote rows.
pub trait ToData: for<'de> Deserialize<'de> + Send + 'static {
    /// Whether this row is a registration-ACK (filtered from emission).
    fn is_ack(&self) -> bool;
    /// Convert to a nautilus data event, or `None` if it is an ACK.
    fn to_data(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<Data>;
}

impl ToData for TradeRow {
    fn is_ack(&self) -> bool {
        TradeRow::is_ack(self)
    }
    fn to_data(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<Data> {
        TradeRow::to_data(self, instrument_id, ts)
    }
}

impl ToData for BookRow {
    fn is_ack(&self) -> bool {
        BookRow::is_ack(self)
    }
    fn to_data(&self, instrument_id: InstrumentId, ts: UnixNanos) -> Option<Data> {
        BookRow::to_data(self, instrument_id, ts)
    }
}

/// SC0 — order-accept (주식 주문접수) row on the OrderEvent lane.
///
/// **Staged, not wired in v1.** The order-event (SC) lane is not observable on the
/// bare paper gateway (no counterparty fills), so v1 recovers order state via
/// `Orders::reconcile` + t0425 polling (the repo's KTD6 stance); this decode struct
/// is here for the follow-on live fill/modify-chain wave.
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

/// SC1 — order-fill (주식 주문체결) row on the OrderEvent lane. **Staged, not wired
/// in v1** (see [`Sc0Row`]): it *will* drive `emit_order_filled` once the OrderEvent
/// lane is subscribed in the live fill wave; v1 recovers fills via reconcile+t0425.
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
}

impl Sc1Row {
    /// Whether this is a registration-ACK / all-default row.
    pub fn is_ack(&self) -> bool {
        self.ordno.trim().is_empty()
    }

    /// The filled quantity as an integer (0 if unparseable/blank).
    pub fn exec_qty(&self) -> i64 {
        i64_from(&self.execqty).max(0)
    }

    /// The fill price as an integer KRW (0 if unparseable/blank).
    pub fn exec_price(&self) -> i64 {
        i64_from(&self.execprc).max(0)
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
