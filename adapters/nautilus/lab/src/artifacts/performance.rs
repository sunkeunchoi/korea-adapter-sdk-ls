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

/// The turn-3 decisiveness bar (R1). A keep/revert verdict requires **all three**
/// conditions; any failure means insufficient-evidence. The verdict word stays
/// hand-authored (KTD-2) — this type only computes the machine-checkable bar.
pub mod bar {
    /// Condition (a): minimum total realized trades.
    pub const TRADE_FLOOR: usize = 30;
    /// Condition (b): minimum count of symbols each carrying ≥ [`SYMBOL_TRADE_FLOOR`] trades.
    pub const BREADTH_SYMBOL_FLOOR: usize = 6;
    /// Condition (b): a symbol counts toward breadth only with at least this many trades.
    pub const SYMBOL_TRADE_FLOOR: usize = 2;
    /// Condition (c): max single-symbol share of aggregate |P&L| (inclusive pass at 40.0%).
    pub const DOMINANCE_CAP: f64 = 0.40;
}

/// One symbol's fold of the realized trade ledger (KTD-2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolAggregate {
    /// Instrument (`{shcode}.XKRX`).
    pub symbol: String,
    /// Number of realized (closed) trades on this symbol.
    pub trades: usize,
    /// Summed realized P&L (KRW; signed).
    pub realized_pnl: f64,
    /// Share of aggregate |P&L|: `|realized_pnl| / Σ|per-symbol realized_pnl|`
    /// (0.0 when the aggregate is zero — the degenerate case, see [`BarEvaluation`]).
    pub abs_pnl_share: f64,
}

/// The computed R1 decisiveness bar over a run's realized trade ledger (KTD-2):
/// the per-symbol fold, the three per-condition PASS/FAIL flags, and the named
/// failing conditions. `all_pass` gates a keep/revert verdict; otherwise the
/// verdict is insufficient-evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarEvaluation {
    /// Total realized (closed) trades — condition (a) numerator.
    pub total_trades: usize,
    /// Condition (a): `total_trades >= TRADE_FLOOR`.
    pub trade_floor_pass: bool,
    /// Count of symbols with ≥ `SYMBOL_TRADE_FLOOR` trades — condition (b) numerator.
    pub symbols_meeting_breadth: usize,
    /// Condition (b): `symbols_meeting_breadth >= BREADTH_SYMBOL_FLOOR`.
    pub breadth_pass: bool,
    /// The dominance metric `max(|per-symbol P&L|) / Σ|per-symbol P&L|` (0.0 in the
    /// degenerate all-zero case).
    pub max_abs_pnl_share: f64,
    /// Condition (c): `max_abs_pnl_share <= DOMINANCE_CAP` and not degenerate.
    pub dominance_pass: bool,
    /// True when aggregate |P&L| is zero (dominance undefined → fail-closed).
    pub degenerate_zero_pnl: bool,
    /// Per-symbol fold, sorted by symbol for deterministic output.
    pub per_symbol: Vec<SymbolAggregate>,
    /// Human-readable messages for each failing condition (empty ⇒ bar cleared).
    pub failing_conditions: Vec<String>,
    /// True iff all three conditions hold — the only state eligible for keep/revert.
    pub all_pass: bool,
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

    /// Evaluate the turn-3 decisiveness bar (R1, KTD-2) over the realized ledger.
    ///
    /// A trade is *realized* iff it is closed (`ts_closed.is_some()`) — matching the
    /// `num_trades` summary. The three conditions:
    /// - (a) `total_trades >= 30`;
    /// - (b) `>= 6` symbols each with `>= 2` realized trades;
    /// - (c) no single symbol exceeds 40% of aggregate |P&L|, where the share is
    ///   `max(|per-symbol realized P&L|) / Σ|per-symbol realized P&L|` — an
    ///   absolute-magnitude share, well-defined under mixed signs (a signed
    ///   share-of-net can exceed 100% or go negative).
    ///
    /// Boundaries pass inclusively (exactly 30 trades / exactly 6 symbols / exactly
    /// 40.0% all pass). The degenerate all-zero-P&L case (denominator 0) fails
    /// closed to insufficient-evidence with a named condition.
    pub fn bar_evaluation(&self) -> BarEvaluation {
        use std::collections::BTreeMap;

        // Fold closed trades by symbol → (count, summed realized P&L).
        let mut folded: BTreeMap<String, (usize, f64)> = BTreeMap::new();
        for t in self.trades.iter().filter(|t| t.ts_closed.is_some()) {
            let e = folded.entry(t.symbol.clone()).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += t.realized_pnl;
        }

        let total_trades: usize = folded.values().map(|(c, _)| *c).sum();
        let symbols_meeting_breadth = folded
            .values()
            .filter(|(c, _)| *c >= bar::SYMBOL_TRADE_FLOOR)
            .count();

        let sum_abs: f64 = folded.values().map(|(_, p)| p.abs()).sum();
        let max_abs: f64 = folded.values().map(|(_, p)| p.abs()).fold(0.0, f64::max);
        let degenerate_zero_pnl = sum_abs == 0.0;
        let max_abs_pnl_share = if degenerate_zero_pnl { 0.0 } else { max_abs / sum_abs };

        let per_symbol: Vec<SymbolAggregate> = folded
            .iter()
            .map(|(symbol, (trades, realized_pnl))| SymbolAggregate {
                symbol: symbol.clone(),
                trades: *trades,
                realized_pnl: *realized_pnl,
                abs_pnl_share: if degenerate_zero_pnl { 0.0 } else { realized_pnl.abs() / sum_abs },
            })
            .collect();

        let trade_floor_pass = total_trades >= bar::TRADE_FLOOR;
        let breadth_pass = symbols_meeting_breadth >= bar::BREADTH_SYMBOL_FLOOR;
        // Fail closed on the degenerate case: dominance is undefined, so it cannot pass.
        let dominance_pass = !degenerate_zero_pnl && max_abs_pnl_share <= bar::DOMINANCE_CAP;

        let mut failing_conditions = Vec::new();
        if !trade_floor_pass {
            failing_conditions.push(format!(
                "trade-count floor not met ({total_trades} < {})",
                bar::TRADE_FLOOR
            ));
        }
        if !breadth_pass {
            failing_conditions.push(format!(
                "symbol-breadth floor not met ({symbols_meeting_breadth} < {})",
                bar::BREADTH_SYMBOL_FLOOR
            ));
        }
        if degenerate_zero_pnl {
            failing_conditions.push(
                "all per-symbol P&L is zero — dominance undefined (fail-closed to insufficient-evidence)"
                    .to_string(),
            );
        } else if !dominance_pass {
            failing_conditions.push(format!(
                "single-symbol dominance ({:.0}% > {:.0}%)",
                max_abs_pnl_share * 100.0,
                bar::DOMINANCE_CAP * 100.0
            ));
        }

        let all_pass = trade_floor_pass && breadth_pass && dominance_pass;
        BarEvaluation {
            total_trades,
            trade_floor_pass,
            symbols_meeting_breadth,
            breadth_pass,
            max_abs_pnl_share,
            dominance_pass,
            degenerate_zero_pnl,
            per_symbol,
            failing_conditions,
            all_pass,
        }
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

    // -----------------------------------------------------------------------
    // U2 — the R1 decisiveness bar (AE1–AE4 + mixed-sign / boundary / degenerate).
    // -----------------------------------------------------------------------

    /// Build `n` closed trades on `symbol`, each with the given per-trade P&L, at
    /// distinct timestamps so they are all realized.
    fn trades_for(symbol: &str, n: usize, pnl_each: f64) -> Vec<TradeRecord> {
        (0..n).map(|i| trade(symbol, pnl_each, (i as u64) * 2 + 1, (i as u64) * 2 + 2)).collect()
    }

    /// The R1 bar over a raw ledger (no analyzer round-trip needed — the bar folds
    /// the trades directly).
    fn eval(trades: Vec<TradeRecord>) -> BarEvaluation {
        PerformanceReport { trades, equity_curve: vec![], summary: Default::default() }.bar_evaluation()
    }

    #[test]
    fn ae1_bar_cleared_all_three_conditions_pass() {
        // 42 trades across 9 symbols, each ≥ 2, spread P&L → max share well under 40%.
        // 9 symbols × varied counts summing to 42; distinct magnitudes keep no single
        // symbol dominant.
        let mut trades = Vec::new();
        // counts: 6+6+6+6+4+4+4+3+3 = 42, all ≥ 2.
        let plan = [("A", 6usize), ("B", 6), ("C", 6), ("D", 6), ("E", 4), ("F", 4), ("G", 4), ("H", 3), ("I", 3)];
        for (i, (sym, n)) in plan.iter().enumerate() {
            // Per-symbol totals spread so the max abs-share stays < 40%.
            trades.extend(trades_for(&format!("{sym}.XKRX"), *n, 100.0 + i as f64 * 10.0));
        }
        let b = eval(trades);
        assert_eq!(b.total_trades, 42);
        assert!(b.trade_floor_pass, "(a) 42 ≥ 30");
        assert_eq!(b.symbols_meeting_breadth, 9);
        assert!(b.breadth_pass, "(b) 9 ≥ 6");
        assert!(b.max_abs_pnl_share <= 0.40, "share {} ≤ 40%", b.max_abs_pnl_share);
        assert!(b.dominance_pass, "(c) passes");
        assert!(b.all_pass, "bar cleared: {:?}", b.failing_conditions);
        assert!(b.failing_conditions.is_empty());
    }

    #[test]
    fn ae2_trade_floor_missed() {
        // 24 trades across 8 symbols (3 each) → (a) FAILs, others irrelevant.
        let mut trades = Vec::new();
        for i in 0..8 {
            trades.extend(trades_for(&format!("S{i}.XKRX"), 3, 100.0 + i as f64));
        }
        let b = eval(trades);
        assert_eq!(b.total_trades, 24);
        assert!(!b.trade_floor_pass);
        assert!(!b.all_pass);
        assert!(
            b.failing_conditions.iter().any(|m| m == "trade-count floor not met (24 < 30)"),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn ae3_breadth_floor_missed() {
        // 33 trades but only 5 symbols with ≥ 2 trades (5 symbols × 6 trades = 30,
        // plus 3 singletons = 33 total, only 5 clear the ≥2 breadth gate).
        let mut trades = Vec::new();
        for i in 0..5 {
            trades.extend(trades_for(&format!("B{i}.XKRX"), 6, 100.0 + i as f64));
        }
        for i in 0..3 {
            trades.extend(trades_for(&format!("X{i}.XKRX"), 1, 50.0 + i as f64));
        }
        let b = eval(trades);
        assert_eq!(b.total_trades, 33);
        assert!(b.trade_floor_pass, "(a) 33 ≥ 30");
        assert_eq!(b.symbols_meeting_breadth, 5);
        assert!(!b.breadth_pass);
        assert!(!b.all_pass);
        assert!(
            b.failing_conditions.iter().any(|m| m == "symbol-breadth floor not met (5 < 6)"),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn ae4_dominance_guard_tripped() {
        // 35 trades across 7 symbols; one symbol carries 58% of aggregate |P&L|.
        // Dominant symbol: 5 trades summing to 58,000; the other 6 symbols sum to
        // 42,000 |P&L| across 30 trades (5 each). Σ|P&L| = 100,000 → share 58%.
        let mut trades = Vec::new();
        trades.extend(trades_for("DOM.XKRX", 5, 58_000.0 / 5.0)); // 58k over 5 trades
        for i in 0..6 {
            trades.extend(trades_for(&format!("R{i}.XKRX"), 5, 7_000.0 / 5.0)); // 7k each → 42k
        }
        let b = eval(trades);
        assert_eq!(b.total_trades, 35);
        assert_eq!(b.symbols_meeting_breadth, 7);
        assert!((b.max_abs_pnl_share - 0.58).abs() < 1e-9, "share {}", b.max_abs_pnl_share);
        assert!(!b.dominance_pass);
        assert!(!b.all_pass);
        assert!(
            b.failing_conditions.iter().any(|m| m == "single-symbol dominance (58% > 40%)"),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn mixed_sign_abs_share_never_exceeds_100_or_goes_negative() {
        // A +200k winner against −100k of losers (net +100k). Abs-share of the winner
        // is 200k / 300k = 67% — a signed share-of-net would read 200%.
        let mut trades = trades_for("WIN.XKRX", 2, 100_000.0); // +200k
        trades.extend(trades_for("LOSS.XKRX", 2, -50_000.0)); // −100k
        let b = eval(trades);
        assert!(b.max_abs_pnl_share > 0.0 && b.max_abs_pnl_share <= 1.0, "share {}", b.max_abs_pnl_share);
        assert!((b.max_abs_pnl_share - (200_000.0 / 300_000.0)).abs() < 1e-9);
        assert!(!b.dominance_pass, "67% > 40% → dominance fails");
        assert!(
            b.failing_conditions.iter().any(|m| m == "single-symbol dominance (67% > 40%)"),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn boundary_exact_thresholds_all_pass() {
        // Exactly 30 trades / exactly 6 symbols with ≥2 / exactly 40.0% abs-share.
        // Dominant symbol 40,000 |P&L|; the other 5 sum to 60,000 → Σ = 100,000 →
        // share exactly 0.40. Counts: 5 symbols × 5 + 1 symbol × 5 = 30, 6 symbols.
        let mut trades = trades_for("D.XKRX", 5, 40_000.0 / 5.0); // 40k
        for i in 0..5 {
            trades.extend(trades_for(&format!("N{i}.XKRX"), 5, 12_000.0 / 5.0)); // 12k each → 60k
        }
        let b = eval(trades);
        assert_eq!(b.total_trades, 30);
        assert!(b.trade_floor_pass, "(a) exactly 30 passes");
        assert_eq!(b.symbols_meeting_breadth, 6);
        assert!(b.breadth_pass, "(b) exactly 6 passes");
        assert!((b.max_abs_pnl_share - 0.40).abs() < 1e-9, "share {}", b.max_abs_pnl_share);
        assert!(b.dominance_pass, "(c) exactly 40.0% passes inclusively");
        assert!(b.all_pass, "boundary clears: {:?}", b.failing_conditions);
    }

    #[test]
    fn degenerate_all_zero_pnl_fails_closed() {
        // 30 trades across 6 symbols, all P&L zero → denominator 0 → dominance
        // undefined → fail-closed to insufficient-evidence, even though (a) and (b) hold.
        let mut trades = Vec::new();
        for i in 0..6 {
            trades.extend(trades_for(&format!("Z{i}.XKRX"), 5, 0.0));
        }
        let b = eval(trades);
        assert_eq!(b.total_trades, 30);
        assert!(b.trade_floor_pass && b.breadth_pass, "(a)+(b) hold");
        assert!(b.degenerate_zero_pnl);
        assert!(!b.dominance_pass, "degenerate dominance fails closed");
        assert!(!b.all_pass);
        assert!(
            b.failing_conditions.iter().any(|m| m.contains("dominance undefined")),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn empty_ledger_fails_all() {
        let b = eval(vec![]);
        assert_eq!(b.total_trades, 0);
        assert!(!b.trade_floor_pass && !b.breadth_pass && !b.dominance_pass);
        assert!(!b.all_pass);
        // Trade floor + breadth both named; dominance is degenerate (zero aggregate).
        assert!(b.failing_conditions.iter().any(|m| m.contains("trade-count floor")));
        assert!(b.failing_conditions.iter().any(|m| m.contains("symbol-breadth floor")));
    }

    #[test]
    fn open_trades_are_excluded_from_the_bar() {
        // The bar folds only realized (closed) trades. An OPEN leg (ts_closed=None,
        // no realized P&L) must not touch total_trades, breadth, or the dominance
        // denominator — matching `num_trades = closed.len()`.
        let mut trades = trades_for("A.XKRX", 2, 100.0); // 2 closed
        trades.push(TradeRecord {
            symbol: "OPEN.XKRX".to_string(),
            entry_side: "BUY".to_string(),
            quantity: 10.0,
            avg_px_open: 60_000.0,
            avg_px_close: None,
            realized_pnl: 9_999_999.0, // must be ignored — position not closed
            ts_opened: 100,
            ts_closed: None,
            fills: vec![],
        });
        let b = eval(trades);
        assert_eq!(b.total_trades, 2, "open leg excluded from the trade count");
        assert_eq!(b.per_symbol.len(), 1, "only the closed symbol is folded");
        assert_eq!(b.per_symbol[0].symbol, "A.XKRX");
        assert!(!b.per_symbol.iter().any(|s| s.symbol == "OPEN.XKRX"), "open symbol absent");
        // The open leg's 9,999,999 P&L did not enter the dominance denominator:
        // the sole closed symbol carries 100% of aggregate |P&L|.
        assert!((b.max_abs_pnl_share - 1.0).abs() < 1e-9);
    }
}
