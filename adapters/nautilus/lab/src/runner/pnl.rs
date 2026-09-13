//! live-session-driver U4 — the session P&L accounting seam the max-loss breaker reads
//! (R5; KTD8(a), KTD8(b)).
//!
//! Two facts the breaker needs each watchdog tick, neither of which existed:
//!
//! **(a) Realized P&L is not a `FillLedger` sum.** The ledger stores per-`OrdNo`
//! exactly-once emission watermarks — no cost basis, no realized P&L. So realized session
//! P&L is *accounting*: match offsetting fills against a running average cost basis over
//! the ledger's fill journal ([`nautilus_ls::orders::ledger::LedgerFill`]).
//!
//! **(b) The open-position mark must never under-report the loss.** The watchdog thread
//! owns its own runtime and has no market-data access, so it marks from the strategy's
//! published [`MarkFeed`]. A stale-favorable last price is the trap: a market-data gap or
//! a symbol halt accompanies exactly the fast adverse moves the breaker exists to catch.
//! So [`mark_open_pnl`] marks at the **adverse edge** and falls back to a floor — the
//! position's stop level, else a configured worst-case adverse bound — whenever the feed
//! is stale or absent. It is never allowed to resolve to a last-seen favorable price.
//!
//! Pure and clock-injected, so both are provable offline against scripted fixtures.

use std::collections::HashMap;
use std::sync::Mutex;

use nautilus_ls::orders::ledger::{FillLedger, LedgerFill};
use nautilus_model::enums::OrderSide;
use nautilus_model::identifiers::TradeId;

use crate::artifacts::performance::{FillRecord, TradeRecord};
use crate::strategy::orb::SymbolMark;

/// A symbol's still-open (unmatched) position after the offsetting match.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPosition {
    /// The bare shcode.
    pub symbol: String,
    /// Signed quantity: positive long, negative short. Never zero (a flat symbol is
    /// dropped).
    pub qty: i64,
    /// The average cost basis (KRW per share) of the open quantity.
    pub avg_cost: f64,
}

/// The session's P&L split into what is booked and what is still at risk.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionPnl {
    /// Realized (matched-and-closed) session P&L in KRW.
    pub realized_krw: f64,
    /// The still-open positions, to be marked (never realized).
    pub open: Vec<OpenPosition>,
}

/// How [`mark_open_pnl`] resolves a mark price when the feed is stale or absent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkPolicy {
    /// Maximum age (seconds) a published bar close may have and still be used. A 1-minute
    /// bar series with a small allowance for arrival jitter; beyond it the feed counts as
    /// stale and the floor takes over.
    pub max_mark_age_secs: i64,
    /// The worst-case adverse move, as a fraction of cost basis, used ONLY when neither a
    /// fresh price nor a stop level is known. KRX's ±30% daily band is the hard bound; the
    /// default is deliberately conservative within it.
    pub worst_case_adverse_fraction: f64,
}

impl Default for MarkPolicy {
    fn default() -> Self {
        MarkPolicy {
            // Two 1-minute bars: one missed bar is tolerated, a gap is not.
            max_mark_age_secs: 120,
            // The KRX daily price band — the true worst case for an unmarkable position.
            worst_case_adverse_fraction: 0.30,
        }
    }
}

/// Accumulate realized P&L and the open position for one symbol.
#[derive(Debug, Default, Clone, Copy)]
struct Book {
    /// Signed open quantity.
    qty: i64,
    /// Average cost of the open quantity.
    avg_cost: f64,
    /// Realized P&L booked so far on this symbol.
    realized: f64,
}

impl Book {
    /// Apply one signed fill (positive = bought, negative = sold) at `price`.
    ///
    /// Same-direction fills extend the position and re-average the basis. An opposing fill
    /// **matches against the basis** and books the difference — this is the accounting the
    /// ledger does not do. A fill that overshoots flat flips the position and re-bases the
    /// remainder at the fill price.
    fn apply(&mut self, signed_qty: i64, price: f64) {
        if signed_qty == 0 {
            return;
        }
        let same_direction = self.qty == 0 || (self.qty > 0) == (signed_qty > 0);
        if same_direction {
            let held = self.qty.unsigned_abs() as f64;
            let added = signed_qty.unsigned_abs() as f64;
            self.avg_cost = (self.avg_cost * held + price * added) / (held + added);
            self.qty += signed_qty;
            return;
        }
        // Opposing: book the matched portion against the basis.
        let matched = self.qty.abs().min(signed_qty.abs());
        // Long closed by a sell: (exit − basis) × qty. Short closed by a buy: (basis − exit).
        let direction = if self.qty > 0 { 1.0 } else { -1.0 };
        self.realized += (price - self.avg_cost) * matched as f64 * direction;
        self.qty += signed_qty;
        if self.qty == 0 {
            self.avg_cost = 0.0;
        } else if (self.qty > 0) == (signed_qty > 0) {
            // Overshot flat and flipped — the remainder is a NEW position at this price.
            self.avg_cost = price;
        }
    }
}

/// The realized-P&L accounting seam (KTD8(a)): match offsetting fills against a running
/// average cost basis over the ledger's fill journal, per symbol, in arrival order.
///
/// An open (unmatched) fill contributes **zero** realized P&L — it is marked, not
/// realized. Fill prices may be approximations (the poll seam's limit-price fallback);
/// that is the same approximation the run's performance report carries, and it is
/// recorded as such on each [`LedgerFill`].
pub fn account_fills(fills: &[LedgerFill]) -> SessionPnl {
    let mut books: HashMap<&str, Book> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for fill in fills {
        let signed = match fill.side {
            OrderSide::Buy => fill.qty,
            OrderSide::Sell => -fill.qty,
            // An unclassifiable side cannot be booked either way; skipping it would
            // silently under-report, so it is impossible by construction upstream
            // (`side_code` refuses anything but Buy/Sell before an order is ever sent).
            _ => continue,
        };
        let book = books.entry(fill.symbol.as_str()).or_insert_with(|| {
            order.push(fill.symbol.as_str());
            Book::default()
        });
        book.apply(signed, fill.price as f64);
    }
    let realized_krw = books.values().map(|b| b.realized).sum();
    let open = order
        .into_iter()
        .filter_map(|sym| {
            let b = books.get(sym)?;
            (b.qty != 0).then(|| OpenPosition {
                symbol: sym.to_string(),
                qty: b.qty,
                avg_cost: b.avg_cost,
            })
        })
        .collect();
    SessionPnl { realized_krw, open }
}

/// The session P&L read through the **shared** fill-ledger `Arc` (KTD3) — the node's real
/// fills, not a separately-built client's empty ledger.
pub fn account_shared(ledger: &Mutex<FillLedger>) -> SessionPnl {
    let guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
    account_fills(guard.fills())
}

/// One inherited overnight book leg, as the breaker must see it (U8, KTD13).
///
/// This is the accounting projection of a `rehearsal/book.json` leg, not the leg itself:
/// the book is the broker-confirmed record with entry ordinals and `entered_under`
/// (KTD11, U9's to read and write). What the breaker needs is three numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct BookLeg {
    /// The bare shcode.
    pub symbol: String,
    /// Held quantity. Positive — the daily lineage is long-only.
    pub qty: i64,
    /// The PRIOR session's close, in integer KRW. Deliberately the day basis rather than
    /// the leg's entry price: see [`seed_book_legs`].
    pub prior_close: i64,
    /// The leg's stop price, if the book carries one. Becomes the [`MarkPolicy`] floor for
    /// this symbol when the mark feed is stale or absent, so a halted overnight holding is
    /// marked at its stop rather than at a stale-favorable last price.
    pub stop_price: Option<i64>,
}

/// Seed an inherited overnight book into a fresh session's fill ledger as synthetic fills
/// (U8, KTD13) — the node-build step that lets the breaker see yesterday's exposure.
///
/// **Why the prior close and not the entry price.** Seeding at each leg's original cost
/// basis would make [`mark_open_pnl`] report the position's *inception-to-date* P&L, and
/// the breaker would then compare a multi-session drawdown against `session_max_loss_krw`
/// — tripping a healthy session for losses it did not cause, and (worse, since the
/// threshold is one-sided) masking a real one-day collapse inside a large paper profit.
/// Seeding at the prior close makes every derived quantity a *session* quantity with no
/// new arithmetic: the open mark becomes `(mark − prior close) × qty`, and a same-day exit
/// books `(exit − prior close) × qty` as realized through the ordinary offsetting match.
/// That is exactly KTD13's basis, obtained by feeding the tested accounting the right
/// number rather than by adding a second P&L path beside it.
///
/// Without it, the failure is silent in both directions: the overnight position is invisible
/// to the breaker, and a day-2 exit sell with no preceding buy is booked by
/// [`account_fills`] as a **short** — a position the account does not hold, marked with the
/// sign reversed.
///
/// **Stamp `observed_ns` at the PRIOR session, not at this one.** The stamp becomes the
/// leg's `ts_opened` in [`session_trades`], and a run observation counts `entries` by
/// opening date: a seed stamped today would report an inherited position as an entry the
/// session never made. Exit attribution — the half the verdict statistic is computed from —
/// is unaffected either way, which is exactly why the miscount would be quiet.
///
/// Returns the number of legs seeded. Zero-quantity legs are skipped (a t0424 zero-balance
/// row reads as an open holding otherwise — see
/// `docs/solutions/logic-errors/t0424-zero-balance-row-reads-as-open-holding.md`).
pub fn seed_book_legs(ledger: &Mutex<FillLedger>, legs: &[BookLeg], observed_ns: u64) -> usize {
    let mut guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
    let mut seeded = 0;
    for leg in legs.iter().filter(|l| l.qty > 0) {
        guard.seed_fill(LedgerFill {
            symbol: leg.symbol.clone(),
            side: OrderSide::Buy,
            qty: leg.qty,
            price: leg.prior_close,
            // The prior close is not an execution price, and no artifact may present it as
            // one. A rehearsal day that inherits a book therefore reports its seeded legs
            // among `price_approximated_fills`, which is the true statement.
            price_approximated: true,
            // A scheme that cannot collide with an `execno` or a `POLL-…` synthetic.
            trade_id: TradeId::new(&format!("SEED-{}-{observed_ns}", leg.symbol)),
            observed_ns,
        });
        seeded += 1;
    }
    seeded
}

/// The [`MarkPolicy`] floors an inherited book contributes (U8, KTD13): symbol → stop
/// price, for every leg whose book row carries one.
///
/// The feed-stale fallback in [`mark_price`] reads the stop off the strategy's published
/// [`SymbolMark`], which a restored leg has none of until the strategy publishes one — and
/// a symbol halted all session never gets there. These are the book's own stops, so an
/// overnight holding that never prints is still marked at a bound rather than at the
/// configured worst case.
pub fn book_stop_floors(legs: &[BookLeg]) -> HashMap<String, i64> {
    legs.iter()
        .filter_map(|l| l.stop_price.map(|s| (l.symbol.clone(), s)))
        .collect()
}

/// The breaker's session basis for a rehearsal (U8, KTD13): realized session P&L plus the
/// conservatively-marked open position, with the inherited book's stops available as
/// [`MarkPolicy`] floors for legs the strategy has not published a mark for.
///
/// The ladder path reaches the same two numbers through [`account_shared`] +
/// [`mark_open_pnl`] with no floors, because every position it holds was opened this
/// session and therefore has a published mark. This is that computation with the one
/// input a restored leg needs.
pub fn rehearsal_breaker_basis(
    session: &SessionPnl,
    marks: &HashMap<String, SymbolMark>,
    floors: &HashMap<String, i64>,
    now_unix: i64,
    policy: &MarkPolicy,
) -> (f64, f64) {
    let marked = session
        .open
        .iter()
        .map(|p| {
            let published = marks.get(&p.symbol).copied();
            let fresh = published
                .is_some_and(|m| (now_unix - m.last_bar_unix) <= policy.max_mark_age_secs);
            // KTD13 makes the book's stop a FALLBACK, not a co-bound: a fresh published
            // mark stands on its own terms (its own stop still co-bounds it, as it always
            // has). Folding the book's stop in beside a fresh close would mark every
            // inherited leg at its stop for the whole session — the breaker would read a
            // stop-loss-sized drawdown on a position that never moved.
            let mark = if fresh {
                published
            } else {
                floors
                    .get(&p.symbol)
                    .copied()
                    .map(|stop| SymbolMark {
                        last_close: published.map_or(0, |m| m.last_close),
                        // An absent mark is stamped stale on purpose: `last_close: 0` must
                        // never be reachable as a price, only the stop floor may be.
                        last_bar_unix: published.map_or(i64::MIN / 2, |m| m.last_bar_unix),
                        stop_price: published.and_then(|m| m.stop_price).or(Some(stop)),
                    })
                    .or(published)
            };
            let px = mark_price(p, mark, now_unix, policy);
            (px - p.avg_cost) * p.qty as f64
        })
        .sum();
    (session.realized_krw, marked)
}

/// The mark price for one open position at the **adverse edge** (KTD8(b)).
///
/// Precedence, for a long (mirrored for a short):
///
/// | fresh close | stop level | mark |
/// |---|---|---|
/// | yes | yes | `min(close, stop)` — the adverse edge of the two known bounds; a bar close is itself up to a bar stale, so a position can be through its stop inside it |
/// | no  | yes | `stop` — the feed is stale/absent, so the floor takes over |
/// | yes | no  | `close` — no stop is known; the fresh price is the only bound |
/// | no  | no  | the configured worst-case adverse bound off the cost basis |
///
/// A stale last-seen price is never a candidate. That is the whole point: a data gap
/// accompanies the moves the breaker must catch.
pub fn mark_price(
    position: &OpenPosition,
    mark: Option<SymbolMark>,
    now_unix: i64,
    policy: &MarkPolicy,
) -> f64 {
    let long = position.qty > 0;
    let fresh_close = mark.and_then(|m| {
        ((now_unix - m.last_bar_unix) <= policy.max_mark_age_secs).then_some(m.last_close as f64)
    });
    let stop = mark.and_then(|m| m.stop_price).map(|p| p as f64);
    let worst_case = if long {
        position.avg_cost * (1.0 - policy.worst_case_adverse_fraction)
    } else {
        position.avg_cost * (1.0 + policy.worst_case_adverse_fraction)
    };
    // The adverse edge is the minimum for a long, the maximum for a short.
    let adverse = |a: f64, b: f64| if long { a.min(b) } else { a.max(b) };
    match (fresh_close, stop) {
        (Some(c), Some(s)) => adverse(c, s),
        (None, Some(s)) => s,
        (Some(c), None) => c,
        (None, None) => worst_case,
    }
}

/// The conservatively-marked open P&L (KRW) across every open position (KTD8(b)) — the
/// `open_marked_pnl_krw` half of a [`WatchdogObservation`](crate::runner::watchdog::WatchdogObservation).
pub fn mark_open_pnl(
    open: &[OpenPosition],
    marks: &HashMap<String, SymbolMark>,
    now_unix: i64,
    policy: &MarkPolicy,
) -> f64 {
    open.iter()
        .map(|p| {
            let px = mark_price(p, marks.get(&p.symbol).copied(), now_unix, policy);
            (px - p.avg_cost) * p.qty as f64
        })
        .sum()
}

/// Assemble the session's round-trips from the shared ledger's fill journal — what the
/// live run's `performance.json` is built from.
///
/// One [`TradeRecord`] per position lifecycle: a symbol going flat → non-flat opens a
/// trade, returning to flat closes it (booking the accounting realized P&L), and a
/// position still open when the session ends is emitted **open** (`ts_closed: None`,
/// `realized_pnl: 0.0`) rather than silently marked as if it had closed.
pub fn session_trades(fills: &[LedgerFill]) -> Vec<TradeRecord> {
    let mut by_symbol: HashMap<&str, Vec<&LedgerFill>> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for f in fills {
        by_symbol
            .entry(f.symbol.as_str())
            .or_insert_with(|| {
                order.push(f.symbol.as_str());
                Vec::new()
            })
            .push(f);
    }

    let mut trades = Vec::new();
    for symbol in order {
        let mut book = Book::default();
        let mut leg: Option<OpenLeg> = None;
        for f in by_symbol.get(symbol).into_iter().flatten() {
            let signed = match f.side {
                OrderSide::Buy => f.qty,
                OrderSide::Sell => -f.qty,
                _ => continue,
            };
            let was = book.qty;
            let realized_before = book.realized;
            book.apply(signed, f.price as f64);

            let leg = leg.get_or_insert_with(|| OpenLeg::new(symbol, signed > 0, f.observed_ns));
            leg.push(f, was, signed);
            if book.qty == 0 {
                trades.push(leg.close(book.realized - realized_before, f.observed_ns));
            }
        }
        // Reborrow after the loop: a still-open leg is emitted as an OPEN trade.
        if book.qty != 0 {
            if let Some(l) = leg {
                trades.push(l.into_open());
            }
        }
    }
    trades
}

/// Join entry-fixed risk into a LIVE session's trades (U8, KTD13) so the session's
/// `observation.json` carries the risk-normalized statistic rather than a bare P&L.
///
/// `risk_capital = filled_qty × (avg_px_open − stop)`, and `realized_r` follows for a
/// closed trade. Two deliberate choices:
///
/// - **The filled quantity, not the intended one.** The backtest's join uses the quantity
///   the strategy submitted, because in a backtest they are the same number. At a closing
///   auction they are not: an order can come back short-filled, and sizing the denominator
///   off the intent would report risk the account never actually carried — inflating
///   `Σ risk_capital` and understating net RoR on exactly the sessions where execution went
///   worst. [`session_trades`] already builds `quantity` from the fills.
/// - **The stop off the published mark.** The daily lineage's stop is entry-fixed
///   (`entry − k × ATR`), so reading it at finalize gives the entry-time value; it is not a
///   trailing stop whose late value would differ. A symbol with no published stop, or a
///   non-positive per-share risk, joins nothing — the same `(None, None)` the backtest's
///   `join_entry_risk` returns for a degenerate risk, which routes the run to the legacy
///   P&L path rather than minting a NaN from a zero denominator.
///
/// `marks` is keyed by bare shcode; [`TradeRecord::symbol`] carries the venue suffix, so
/// the join strips it.
pub fn join_live_risk(trades: &mut [TradeRecord], marks: &HashMap<String, SymbolMark>) {
    for t in trades.iter_mut() {
        let shcode = t.symbol.split('.').next().unwrap_or(t.symbol.as_str());
        let Some(stop) = marks.get(shcode).and_then(|m| m.stop_price) else { continue };
        let risk_per_share = t.avg_px_open - stop as f64;
        if !(risk_per_share > 0.0) || !(t.quantity > 0.0) {
            continue;
        }
        let risk_capital = t.quantity * risk_per_share;
        t.risk_capital = Some(risk_capital);
        t.realized_r = t.ts_closed.map(|_| t.realized_pnl / risk_capital);
    }
}

/// Accumulator for one position lifecycle while [`session_trades`] walks a symbol.
struct OpenLeg {
    symbol: String,
    long: bool,
    ts_opened: u64,
    open_qty: f64,
    open_notional: f64,
    close_qty: f64,
    close_notional: f64,
    fills: Vec<FillRecord>,
}

impl OpenLeg {
    fn new(symbol: &str, long: bool, ts_opened: u64) -> Self {
        OpenLeg {
            symbol: symbol.to_string(),
            long,
            ts_opened,
            open_qty: 0.0,
            open_notional: 0.0,
            close_qty: 0.0,
            close_notional: 0.0,
            fills: Vec::new(),
        }
    }

    /// Attribute a fill to the opening or the closing side of this leg. `was` is the
    /// signed position before the fill, so a fill in the position's own direction extends
    /// the entry and an opposing one reduces it.
    fn push(&mut self, f: &LedgerFill, was: i64, signed: i64) {
        let opening = was == 0 || (was > 0) == (signed > 0);
        if was == 0 {
            // A new lifecycle on a recycled leg: its direction is this fill's, not the
            // previous round-trip's (the reset in `close` cannot know it yet).
            self.long = signed > 0;
        }
        let qty = signed.unsigned_abs() as f64;
        if opening {
            self.open_qty += qty;
            self.open_notional += qty * f.price as f64;
        } else {
            self.close_qty += qty;
            self.close_notional += qty * f.price as f64;
        }
        self.fills.push(FillRecord {
            ts_event: f.observed_ns,
            side: if signed > 0 { "BUY".into() } else { "SELL".into() },
            qty,
            price: f.price as f64,
            trade_id: f.trade_id.to_string(),
            commission: 0.0,
        });
    }

    fn record(&self, realized: f64, ts_closed: Option<u64>) -> TradeRecord {
        TradeRecord {
            // The ledger keys fills by bare shcode, but every other `TradeRecord` producer
            // (`trade_from_position`) writes `instrument_id.to_string()`. Emit the same
            // form so a live run's performance report joins with a backtest's.
            symbol: format!("{}.{}", self.symbol, nautilus_ls::KRX_VENUE),
            entry_side: if self.long { "BUY".into() } else { "SELL".into() },
            quantity: self.open_qty,
            avg_px_open: if self.open_qty > 0.0 { self.open_notional / self.open_qty } else { 0.0 },
            avg_px_close: (self.close_qty > 0.0).then(|| self.close_notional / self.close_qty),
            realized_pnl: realized,
            ts_opened: self.ts_opened,
            ts_closed,
            fills: self.fills.clone(),
            risk_capital: None,
            realized_r: None,
        }
    }

    /// Close the leg and reset it for a possible next lifecycle on the same symbol.
    fn close(&mut self, realized: f64, ts_closed: u64) -> TradeRecord {
        let rec = self.record(realized, Some(ts_closed));
        let symbol = std::mem::take(&mut self.symbol);
        *self = OpenLeg::new(&symbol, self.long, ts_closed);
        rec
    }

    /// A position still open at session end — never booked as if it had closed.
    fn into_open(self) -> TradeRecord {
        self.record(0.0, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::identifiers::TradeId;

    fn fill(symbol: &str, side: OrderSide, qty: i64, price: i64) -> LedgerFill {
        LedgerFill {
            symbol: symbol.to_string(),
            side,
            qty,
            price,
            price_approximated: false,
            trade_id: TradeId::from(format!("T-{symbol}-{qty}-{price}").as_str()),
            observed_ns: 1,
        }
    }

    #[test]
    fn offsetting_fills_realize_against_the_cost_basis() {
        // Buy 10 @ 60_000, sell 10 @ 61_000 → +10_000 realized, nothing open.
        let pnl = account_fills(&[
            fill("005930", OrderSide::Buy, 10, 60_000),
            fill("005930", OrderSide::Sell, 10, 61_000),
        ]);
        assert_eq!(pnl.realized_krw, 10_000.0);
        assert!(pnl.open.is_empty(), "a fully offset symbol holds nothing open");
    }

    #[test]
    fn an_open_fill_contributes_zero_realized_and_carries_its_basis() {
        let pnl = account_fills(&[fill("005930", OrderSide::Buy, 10, 60_000)]);
        assert_eq!(pnl.realized_krw, 0.0, "an unmatched fill is marked, never realized");
        assert_eq!(
            pnl.open,
            vec![OpenPosition { symbol: "005930".into(), qty: 10, avg_cost: 60_000.0 }]
        );
    }

    #[test]
    fn partial_offsets_re_average_the_basis_and_book_only_the_matched_portion() {
        // Buy 10 @ 60_000, buy 10 @ 62_000 (basis 61_000), sell 5 @ 63_000 → +10_000.
        let pnl = account_fills(&[
            fill("005930", OrderSide::Buy, 10, 60_000),
            fill("005930", OrderSide::Buy, 10, 62_000),
            fill("005930", OrderSide::Sell, 5, 63_000),
        ]);
        assert_eq!(pnl.realized_krw, (63_000.0 - 61_000.0) * 5.0);
        assert_eq!(pnl.open, vec![OpenPosition { symbol: "005930".into(), qty: 15, avg_cost: 61_000.0 }]);
    }

    #[test]
    fn a_loss_is_booked_negative_and_symbols_are_accounted_independently() {
        let pnl = account_fills(&[
            fill("005930", OrderSide::Buy, 10, 60_000),
            fill("005930", OrderSide::Sell, 10, 40_000), // −200_000
            fill("000660", OrderSide::Buy, 5, 100_000),
            fill("000660", OrderSide::Sell, 5, 110_000), // +50_000
        ]);
        assert_eq!(pnl.realized_krw, -150_000.0);
    }

    #[test]
    fn overshooting_flat_flips_the_position_and_rebases_the_remainder() {
        // Long 10 @ 60_000, sell 15 @ 61_000 → +10_000 realized, short 5 based at 61_000.
        let pnl = account_fills(&[
            fill("005930", OrderSide::Buy, 10, 60_000),
            fill("005930", OrderSide::Sell, 15, 61_000),
        ]);
        assert_eq!(pnl.realized_krw, 10_000.0);
        assert_eq!(pnl.open, vec![OpenPosition { symbol: "005930".into(), qty: -5, avg_cost: 61_000.0 }]);
    }

    fn long_position() -> OpenPosition {
        OpenPosition { symbol: "005930".into(), qty: 10, avg_cost: 60_000.0 }
    }

    #[test]
    fn a_fresh_close_below_the_stop_marks_at_the_close_the_adverse_edge() {
        let p = long_position();
        let mark = SymbolMark { last_close: 57_000, last_bar_unix: 1_000, stop_price: Some(58_000) };
        assert_eq!(
            mark_price(&p, Some(mark), 1_030, &MarkPolicy::default()),
            57_000.0,
            "gapped through the stop → the close is the adverse edge"
        );
    }

    #[test]
    fn a_fresh_favorable_close_still_marks_no_better_than_the_stop() {
        let p = long_position();
        let mark = SymbolMark { last_close: 62_000, last_bar_unix: 1_000, stop_price: Some(58_000) };
        assert_eq!(
            mark_price(&p, Some(mark), 1_030, &MarkPolicy::default()),
            58_000.0,
            "a bar close is itself up to a bar stale — the stop bounds the adverse edge"
        );
    }

    #[test]
    fn a_stale_favorable_close_falls_back_to_the_stop_floor() {
        let p = long_position();
        // Favorable last close, but the feed died 10 minutes ago.
        let mark = SymbolMark { last_close: 65_000, last_bar_unix: 1_000, stop_price: Some(58_000) };
        assert_eq!(
            mark_price(&p, Some(mark), 1_600, &MarkPolicy::default()),
            58_000.0,
            "a stale-favorable price is never used — the floor takes over"
        );
    }

    #[test]
    fn an_absent_feed_and_no_stop_falls_back_to_the_worst_case_bound() {
        let p = long_position();
        let policy = MarkPolicy { max_mark_age_secs: 120, worst_case_adverse_fraction: 0.30 };
        assert_eq!(
            mark_price(&p, None, 1_600, &policy),
            60_000.0 * 0.70,
            "nothing known → the configured worst-case adverse bound, never a zero mark"
        );
    }

    #[test]
    fn session_trades_emit_closed_round_trips_and_leave_an_open_leg_open() {
        let trades = session_trades(&[
            fill("005930", OrderSide::Buy, 10, 60_000),
            fill("005930", OrderSide::Sell, 10, 61_000),
            // A second symbol still open at session end.
            fill("000660", OrderSide::Buy, 5, 100_000),
        ]);
        assert_eq!(trades.len(), 2);

        let closed = &trades[0];
        assert_eq!(
            closed.symbol, "005930.XKRX",
            "the symbol matches the form every other TradeRecord producer writes"
        );
        assert_eq!(closed.entry_side, "BUY");
        assert_eq!(closed.quantity, 10.0);
        assert_eq!(closed.avg_px_open, 60_000.0);
        assert_eq!(closed.avg_px_close, Some(61_000.0));
        assert_eq!(closed.realized_pnl, 10_000.0);
        assert!(closed.ts_closed.is_some());
        assert_eq!(closed.fills.len(), 2);

        let open = &trades[1];
        assert_eq!(open.symbol, "000660.XKRX");
        assert_eq!(open.ts_closed, None, "a position still open is never booked as closed");
        assert_eq!(open.realized_pnl, 0.0, "and it realizes nothing");
    }

    #[test]
    fn a_short_position_marks_at_the_upper_adverse_edge() {
        let p = OpenPosition { symbol: "005930".into(), qty: -10, avg_cost: 60_000.0 };
        let mark = SymbolMark { last_close: 58_000, last_bar_unix: 1_000, stop_price: Some(63_000) };
        assert_eq!(
            mark_price(&p, Some(mark), 1_030, &MarkPolicy::default()),
            63_000.0,
            "for a short the adverse edge is the HIGHER price"
        );
        // Marked P&L on a short is (mark − basis) × negative qty → a loss.
        let marked = mark_open_pnl(
            &[p],
            &HashMap::from([("005930".to_string(), mark)]),
            1_030,
            &MarkPolicy::default(),
        );
        assert_eq!(marked, -30_000.0);
    }

    #[test]
    fn marked_open_pnl_sums_the_adverse_edge_across_positions() {
        let open = vec![
            OpenPosition { symbol: "005930".into(), qty: 10, avg_cost: 60_000.0 },
            OpenPosition { symbol: "000660".into(), qty: 5, avg_cost: 100_000.0 },
        ];
        let marks = HashMap::from([
            (
                "005930".to_string(),
                SymbolMark { last_close: 59_000, last_bar_unix: 1_000, stop_price: Some(58_000) },
            ),
            // No mark published for 000660 at all → worst-case bound.
            (
                "000660".to_string(),
                SymbolMark { last_close: 101_000, last_bar_unix: 0, stop_price: None },
            ),
        ]);
        let policy = MarkPolicy { max_mark_age_secs: 120, worst_case_adverse_fraction: 0.30 };
        let marked = mark_open_pnl(&open, &marks, 1_030, &policy);
        // 005930: (58_000 − 60_000) × 10 = −20_000. 000660: stale + no stop → 70_000 basis
        // mark → (70_000 − 100_000) × 5 = −150_000.
        assert_eq!(marked, -170_000.0);
    }

    // -----------------------------------------------------------------------
    // U8/KTD13 — the inherited overnight book.
    // -----------------------------------------------------------------------

    fn legs() -> Vec<BookLeg> {
        vec![
            // Bought long ago at some other price; yesterday it closed at 60_000.
            BookLeg { symbol: "005930".into(), qty: 10, prior_close: 60_000, stop_price: Some(54_000) },
            BookLeg { symbol: "000660".into(), qty: 5, prior_close: 100_000, stop_price: None },
        ]
    }

    /// The breaker must score the DAY, not the position's whole life. Seeding at the prior
    /// close is what makes `mark_open_pnl`'s existing arithmetic produce the session figure:
    /// a −5% mark on both legs is −5% of yesterday's close × qty, and nothing about the
    /// original entry price enters it.
    #[test]
    fn a_seeded_book_makes_the_breaker_see_only_todays_move_on_yesterdays_position() {
        let ledger = Mutex::new(FillLedger::new());
        assert_eq!(seed_book_legs(&ledger, &legs(), 1_000), 2);

        let session = account_shared(&ledger);
        assert_eq!(session.realized_krw, 0.0, "inheriting a book realizes nothing");
        assert_eq!(session.open.len(), 2);
        // The basis IS the prior close — that is the whole trick.
        assert_eq!(session.open[0].avg_cost, 60_000.0);
        assert_eq!(session.open[0].qty, 10);

        // Both marked 5% below yesterday's close, fresh.
        let marks: HashMap<String, SymbolMark> = [
            ("005930".to_string(), SymbolMark { last_close: 57_000, last_bar_unix: 1_000, stop_price: None }),
            ("000660".to_string(), SymbolMark { last_close: 95_000, last_bar_unix: 1_000, stop_price: None }),
        ]
        .into_iter()
        .collect();
        let (realized, marked) = rehearsal_breaker_basis(
            &session,
            &marks,
            &book_stop_floors(&legs()),
            1_000,
            &MarkPolicy::default(),
        );
        assert_eq!(realized, 0.0);
        // −3_000 × 10 + −5_000 × 5 = −55_000.
        assert_eq!(marked, -55_000.0, "the day's move on the inherited legs, not their lifetime P&L");
    }

    /// A restored leg the strategy has published no mark for — a symbol halted all session
    /// — is marked at the BOOK's stop, not at the configured worst case. Without the
    /// floors, the same position falls back to `avg_cost × (1 − 0.30)`.
    #[test]
    fn a_book_stop_floors_an_unmarked_restored_leg() {
        let ledger = Mutex::new(FillLedger::new());
        seed_book_legs(&ledger, &legs(), 1_000);
        let session = account_shared(&ledger);
        let no_marks: HashMap<String, SymbolMark> = HashMap::new();

        let (_, floored) = rehearsal_breaker_basis(
            &session,
            &no_marks,
            &book_stop_floors(&legs()),
            1_000,
            &MarkPolicy::default(),
        );
        // 005930 has a book stop: (54_000 − 60_000) × 10 = −60_000.
        // 000660 has none, so the worst case stands: (70_000 − 100_000) × 5 = −150_000.
        assert_eq!(floored, -210_000.0);

        let (_, unfloored) = rehearsal_breaker_basis(
            &session,
            &no_marks,
            &HashMap::new(),
            1_000,
            &MarkPolicy::default(),
        );
        // Without the floor 005930 falls to the worst case too: (42_000 − 60_000) × 10.
        assert_eq!(unfloored, -330_000.0, "the floor is what keeps the estimate from the band");
    }

    /// The failure seeding exists to prevent, pinned as the CURRENT behaviour of the
    /// unseeded ledger: a day-2 exit with no preceding buy is booked as a SHORT. The sign
    /// is reversed and the account is reported holding a position it does not have.
    #[test]
    fn without_seeding_a_day_two_exit_books_as_a_short_position() {
        let unseeded = account_fills(&[fill("005930", OrderSide::Sell, 10, 57_000)]);
        assert_eq!(unseeded.realized_krw, 0.0, "nothing to match against, so nothing realizes");
        assert_eq!(unseeded.open.len(), 1);
        assert_eq!(unseeded.open[0].qty, -10, "a SHORT the account never opened");

        // Seeded, the same sell books the day's loss and leaves the account flat.
        let ledger = Mutex::new(FillLedger::new());
        seed_book_legs(&ledger, &legs()[..1], 1_000);
        {
            let mut g = ledger.lock().unwrap();
            g.seed_fill(fill("005930", OrderSide::Sell, 10, 57_000));
        }
        let seeded = account_shared(&ledger);
        assert_eq!(seeded.realized_krw, -30_000.0, "(57_000 − 60_000) × 10");
        assert!(seeded.open.is_empty(), "and the leg is closed, not flipped short");
    }

    /// A zero-quantity book row is not an open holding — the t0424 zero-balance trap. It
    /// must not seed a fill, or the session inherits a phantom position.
    #[test]
    fn a_zero_quantity_leg_seeds_nothing() {
        let ledger = Mutex::new(FillLedger::new());
        let rows = vec![BookLeg {
            symbol: "005930".into(),
            qty: 0,
            prior_close: 60_000,
            stop_price: Some(54_000),
        }];
        assert_eq!(seed_book_legs(&ledger, &rows, 1_000), 0);
        assert!(account_shared(&ledger).open.is_empty());
    }

    /// A seeded fill is never presented as an exact execution price.
    #[test]
    fn seeded_fills_are_marked_approximated() {
        let ledger = Mutex::new(FillLedger::new());
        seed_book_legs(&ledger, &legs(), 1_000);
        let g = ledger.lock().unwrap();
        assert!(g.fills().iter().all(|f| f.price_approximated));
        assert!(
            g.fills().iter().all(|f| f.trade_id.to_string().starts_with("SEED-")),
            "and carries a trade id that cannot collide with an execno or a POLL- synthetic"
        );
    }

    /// The live risk join sizes the denominator off the FILLED quantity and the published
    /// stop, and joins nothing where the stop is missing or the risk degenerate.
    #[test]
    fn the_live_risk_join_uses_the_filled_quantity_and_the_published_stop() {
        let mut trades = session_trades(&[
            fill("005930", OrderSide::Buy, 8, 60_000),
            fill("005930", OrderSide::Sell, 8, 61_000),
            // A second symbol with no published stop.
            fill("000660", OrderSide::Buy, 5, 100_000),
            fill("000660", OrderSide::Sell, 5, 99_000),
        ]);
        let marks: HashMap<String, SymbolMark> = [(
            "005930".to_string(),
            SymbolMark { last_close: 61_000, last_bar_unix: 1_000, stop_price: Some(57_000) },
        )]
        .into_iter()
        .collect();
        join_live_risk(&mut trades, &marks);

        let joined = trades.iter().find(|t| t.symbol.starts_with("005930")).unwrap();
        // 8 filled shares × (60_000 − 57_000) per share — the filled quantity, not an intent.
        assert_eq!(joined.risk_capital, Some(24_000.0));
        assert_eq!(joined.realized_r, Some(8_000.0 / 24_000.0));

        let unjoined = trades.iter().find(|t| t.symbol.starts_with("000660")).unwrap();
        assert_eq!(unjoined.risk_capital, None, "no stop published, so no risk is invented");
        assert_eq!(unjoined.realized_r, None);
    }
}
