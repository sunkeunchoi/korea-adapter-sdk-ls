//! The performance report (KTD3, R5) — the trade/fill ledger, per-trade P&L, an
//! equity curve, and summary statistics. Summary stats reuse `nautilus-analysis`'s
//! `PortfolioAnalyzer` rather than reimplementing them; the per-trade ledger and
//! equity curve are assembled from the engine's fill/position events.

use std::collections::BTreeMap;

use nautilus_analysis::analyzer::PortfolioAnalyzer;
use nautilus_model::position::Position;
use nautilus_model::types::{Currency, Money};
use serde::{Deserialize, Serialize};

/// One fill within a trade (the fill ledger, R5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillRecord {
    /// Fill time (UTC unix nanoseconds).
    pub ts_event: u64,
    /// `BUY` / `SELL`.
    pub side: String,
    /// Filled quantity.
    pub qty: f64,
    /// Fill price.
    pub price: f64,
    /// The (globally unique) trade id stamped on the fill.
    pub trade_id: String,
    /// Commission charged (0.0 on paper).
    pub commission: f64,
}

/// One closed (or open) trade — a nautilus position over its lifecycle (R5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeRecord {
    /// Instrument (`{shcode}.XKRX`).
    pub symbol: String,
    /// Opening side (`BUY` for an ORB long).
    pub entry_side: String,
    /// Peak quantity held.
    pub quantity: f64,
    /// Average entry price.
    pub avg_px_open: f64,
    /// Average exit price (None while open).
    pub avg_px_close: Option<f64>,
    /// Realized P&L (KRW).
    pub realized_pnl: f64,
    /// Position open time (UTC unix nanoseconds).
    pub ts_opened: u64,
    /// Position close time (None while open).
    pub ts_closed: Option<u64>,
    /// The fills that make up this trade.
    pub fills: Vec<FillRecord>,
}

/// One point on the equity curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquityPoint {
    /// Timestamp (UTC unix nanoseconds).
    pub ts: u64,
    /// Cumulative account equity at this point (starting balance + realized P&L).
    pub equity: f64,
}

/// The performance report artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceReport {
    /// The per-trade ledger.
    pub trades: Vec<TradeRecord>,
    /// The equity curve over closed trades.
    pub equity_curve: Vec<EquityPoint>,
    /// Summary statistics (sorted keys → deterministic output). Sourced from the
    /// PortfolioAnalyzer plus lab-computed `pnl_total`, `num_trades`, `max_drawdown`.
    pub summary: BTreeMap<String, f64>,
}

impl PerformanceReport {
    /// Assemble a report from a per-trade ledger and a starting balance. Summary
    /// stats come from a `PortfolioAnalyzer` (KTD3) fed the realized P&Ls; the equity
    /// curve and drawdown are computed from the ledger.
    pub fn assemble(trades: Vec<TradeRecord>, starting_balance: f64) -> Self {
        let krw = Currency::KRW();

        // Feed realized P&Ls into the analyzer for summary stats (win rate,
        // expectancy, winners/losers, …). Only closed trades carry a realized P&L.
        let mut analyzer = PortfolioAnalyzer::default();
        let mut closed: Vec<&TradeRecord> = trades.iter().filter(|t| t.ts_closed.is_some()).collect();
        closed.sort_by_key(|t| t.ts_closed.unwrap_or(0));
        for (i, t) in closed.iter().enumerate() {
            let pid = nautilus_model::identifiers::PositionId::from(format!("LEDGER-{i}").as_str());
            analyzer.add_trade(
                &pid,
                nautilus_core::UnixNanos::from(t.ts_closed.unwrap_or(0)),
                &Money::new(t.realized_pnl, krw),
            );
            analyzer.add_position_return(
                nautilus_core::UnixNanos::from(t.ts_closed.unwrap_or(0)),
                if starting_balance > 0.0 { t.realized_pnl / starting_balance } else { 0.0 },
            );
        }

        let mut summary: BTreeMap<String, f64> = BTreeMap::new();
        if !closed.is_empty() {
            if let Ok(pnl_stats) = analyzer.get_performance_stats_pnls(Some(&krw), None) {
                for (k, v) in pnl_stats {
                    summary.insert(k, v);
                }
            }
            for (k, v) in analyzer.get_performance_stats_returns() {
                summary.insert(k, v);
            }
        }

        // Drop non-finite analyzer outputs (Sharpe/Sortino/… are NaN without enough
        // data): serde_json renders NaN as `null`, which then fails to round-trip back
        // into an `f64` map. The agent only ever sees meaningful, finite stats.
        summary.retain(|_, v| v.is_finite());

        // Lab-computed additions the analyzer's default set does not provide.
        let pnl_total: f64 = closed.iter().map(|t| t.realized_pnl).sum();
        let equity_curve = build_equity_curve(&closed, starting_balance);
        summary.insert("pnl_total".to_string(), pnl_total);
        summary.insert("num_trades".to_string(), closed.len() as f64);
        summary.insert("max_drawdown".to_string(), max_drawdown(&equity_curve));

        PerformanceReport { trades, equity_curve, summary }
    }

    /// Assemble a report directly from finished nautilus positions (the backtest /
    /// live path): each position becomes a [`TradeRecord`] with its fill ledger.
    pub fn from_positions(positions: &[Position], starting_balance: f64) -> Self {
        let trades = positions.iter().map(trade_from_position).collect();
        Self::assemble(trades, starting_balance)
    }
}

fn trade_from_position(p: &Position) -> TradeRecord {
    let fills = p
        .events
        .iter()
        .map(|f| FillRecord {
            ts_event: f.ts_event.as_u64(),
            side: format!("{:?}", f.order_side).to_uppercase(),
            qty: f.last_qty.as_f64(),
            price: f.last_px.as_f64(),
            trade_id: f.trade_id.to_string(),
            commission: f.commission.map(|m| m.as_f64()).unwrap_or(0.0),
        })
        .collect();
    TradeRecord {
        symbol: p.instrument_id.to_string(),
        entry_side: format!("{:?}", p.entry).to_uppercase(),
        quantity: p.peak_qty.as_f64(),
        avg_px_open: p.avg_px_open,
        avg_px_close: p.avg_px_close,
        realized_pnl: p.realized_pnl.map(|m| m.as_f64()).unwrap_or(0.0),
        ts_opened: p.ts_opened.as_u64(),
        ts_closed: p.ts_closed.map(|t| t.as_u64()),
        fills,
    }
}

/// Build the equity curve: an initial point at the starting balance, then one point
/// per closed trade (in close-time order) at the running cumulative equity.
fn build_equity_curve(closed: &[&TradeRecord], starting_balance: f64) -> Vec<EquityPoint> {
    let mut curve = Vec::with_capacity(closed.len() + 1);
    let start_ts = closed
        .iter()
        .map(|t| t.ts_opened)
        .min()
        .unwrap_or(0);
    curve.push(EquityPoint { ts: start_ts, equity: starting_balance });
    let mut equity = starting_balance;
    for t in closed {
        equity += t.realized_pnl;
        curve.push(EquityPoint { ts: t.ts_closed.unwrap_or(0), equity });
    }
    curve
}

/// The maximum peak-to-trough drawdown over the equity curve (absolute KRW, ≥ 0).
fn max_drawdown(curve: &[EquityPoint]) -> f64 {
    let mut peak = f64::MIN;
    let mut max_dd = 0.0_f64;
    for p in curve {
        peak = peak.max(p.equity);
        max_dd = max_dd.max(peak - p.equity);
    }
    if max_dd.is_finite() {
        max_dd
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(symbol: &str, pnl: f64, ts_open: u64, ts_close: u64) -> TradeRecord {
        TradeRecord {
            symbol: symbol.to_string(),
            entry_side: "BUY".to_string(),
            quantity: 10.0,
            avg_px_open: 60_000.0,
            avg_px_close: Some(60_000.0 + pnl / 10.0),
            realized_pnl: pnl,
            ts_opened: ts_open,
            ts_closed: Some(ts_close),
            fills: vec![],
        }
    }

    #[test]
    fn three_trade_fixture_matches_hand_computed_stats() {
        // +100, -50, +200 → 2 winners of 3 → win rate 0.6667; expectancy 83.333.
        let trades = vec![
            trade("A.XKRX", 100.0, 1, 2),
            trade("B.XKRX", -50.0, 3, 4),
            trade("C.XKRX", 200.0, 5, 6),
        ];
        let report = PerformanceReport::assemble(trades, 1_000_000.0);
        assert_eq!(report.summary["num_trades"], 3.0);
        assert_eq!(report.summary["pnl_total"], 250.0);
        let win_rate = report.summary["Win Rate"];
        assert!((win_rate - 0.6667).abs() < 0.001, "win rate: {win_rate}");
        let expectancy = report.summary["Expectancy"];
        assert!((expectancy - 83.333).abs() < 0.01, "expectancy: {expectancy}");
    }

    #[test]
    fn equity_curve_and_drawdown() {
        // +100 then -300 then +50: equity 1000 → 1100 → 800 → 850. Peak 1100, trough
        // 800 → max drawdown 300.
        let trades = vec![
            trade("A.XKRX", 100.0, 1, 2),
            trade("B.XKRX", -300.0, 3, 4),
            trade("C.XKRX", 50.0, 5, 6),
        ];
        let report = PerformanceReport::assemble(trades, 1000.0);
        assert_eq!(report.equity_curve.len(), 4); // start + 3 trades
        assert_eq!(report.equity_curve.last().unwrap().equity, 850.0);
        assert_eq!(report.summary["max_drawdown"], 300.0);
    }

    #[test]
    fn empty_ledger_is_flat() {
        let report = PerformanceReport::assemble(vec![], 1000.0);
        assert_eq!(report.summary["num_trades"], 0.0);
        assert_eq!(report.summary["pnl_total"], 0.0);
        assert_eq!(report.equity_curve.len(), 1);
    }
}
