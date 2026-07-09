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

/// The decisiveness bar (R1), **re-registered for turn 4 as a function of the
/// pinned universe size** (KTD-2). A keep/revert verdict requires **all three**
/// conditions; any failure means insufficient-evidence. The verdict word stays
/// hand-authored (KTD-2) — this type only computes the machine-checkable bar.
///
/// The two count floors scale linearly with `N = universe_top_n`, generalizing the
/// turn-3 constants (`TRADE_FLOOR = 30`, `BREADTH_SYMBOL_FLOOR = 6` at N = 20):
/// `trade_floor(N) = round_half_up(1.5·N)`, `breadth_floor(N) = round_half_up(0.30·N)`.
/// `SYMBOL_TRADE_FLOOR` and `DOMINANCE_CAP` are unchanged (dominance is a
/// scale-invariant ratio). The generalization reduces to the turn-3 bar at N = 20
/// (`trade_floor(20) = 30`, `breadth_floor(20) = 6`; R1b, AE5) — asserted in tests.
pub mod bar {
    /// Condition (a): realized trades required **per symbol** of universe size.
    /// `trade_floor(N) = round_half_up(1.5·N)` (turn-3: 30 at N = 20).
    pub const TRADE_FLOOR_PER_SYMBOL: f64 = 1.5;
    /// Condition (b): fraction of the universe that must show breadth.
    /// `breadth_floor(N) = round_half_up(0.30·N)` (turn-3: 6 at N = 20).
    pub const BREADTH_FRACTION: f64 = 0.30;
    /// Condition (b): a symbol counts toward breadth only with at least this many trades.
    pub const SYMBOL_TRADE_FLOOR: usize = 2;
    /// Condition (c): max single-symbol share of aggregate |P&L| (inclusive pass at 40.0%).
    /// Scale-invariant — unchanged from turn 3.
    pub const DOMINANCE_CAP: f64 = 0.40;

    /// Round-half-up, deterministic (`.5` always rounds away from zero, upward for
    /// the non-negative floors we compute here). At the pinned N = 40 the products
    /// are integers so rounding is moot; the rule is fixed for any N (KTD-2).
    pub fn round_half_up(x: f64) -> usize {
        (x + 0.5).floor().max(0.0) as usize
    }

    /// Condition (a) floor for a universe of `n` symbols: `round_half_up(1.5·n)`.
    /// N = 20 → 30 (turn-3 backward-compat, R1b); N = 40 → 60.
    pub fn trade_floor(n: usize) -> usize {
        round_half_up(TRADE_FLOOR_PER_SYMBOL * n as f64)
    }

    /// Condition (b) floor for a universe of `n` symbols: `round_half_up(0.30·n)`.
    /// N = 20 → 6 (turn-3 backward-compat, R1b); N = 40 → 12.
    pub fn breadth_floor(n: usize) -> usize {
        round_half_up(BREADTH_FRACTION * n as f64)
    }
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

/// The shared closed-trade fold + single-symbol dominance derivation used by BOTH
/// the retired R1 bar ([`PerformanceReport::bar_evaluation`]) and the turn-5 edge
/// verdict ([`PerformanceReport::edge_evaluation`]) — condition (c) is identical in
/// each. One implementation keeps the two verdicts from silently drifting if the
/// dominance cap or the degenerate-case handling ever changes.
struct DominanceFold {
    /// Per-symbol `(closed-trade count, summed realized P&L)`, sorted by symbol.
    folded: std::collections::BTreeMap<String, (usize, f64)>,
    /// Total realized (closed) trades.
    total_trades: usize,
    /// `max(|per-symbol P&L|) / Σ|per-symbol P&L|` (0.0 in the degenerate case).
    max_abs_pnl_share: f64,
    /// `max_abs_pnl_share <= DOMINANCE_CAP` and not degenerate.
    dominance_pass: bool,
    /// Aggregate |P&L| is zero (dominance undefined → fail-closed).
    degenerate_zero_pnl: bool,
    /// Per-symbol fold as [`SymbolAggregate`]s, sorted by symbol.
    per_symbol: Vec<SymbolAggregate>,
}

/// The computed R1 decisiveness bar over a run's realized trade ledger (KTD-2):
/// the per-symbol fold, the three per-condition PASS/FAIL flags, and the named
/// failing conditions. `all_pass` gates a keep/revert verdict; otherwise the
/// verdict is insufficient-evidence.
///
/// **Retained but off the turn-5 verdict path.** The turn-5 verdict is
/// [`EdgeEvaluation`]; this frequency/breadth bar is kept for historical and future
/// param-turn analysis (its dominance condition (c) is shared via [`DominanceFold`]),
/// not deleted — the retirement is of its *use in the scaffold*, not the type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarEvaluation {
    /// The pinned universe size the floors were derived from (`universe_top_n`, R3 —
    /// never the realized snapshot). N = 40 this turn.
    pub universe_size: usize,
    /// The derived condition (a) floor for `universe_size` (`round_half_up(1.5·N)`;
    /// 60 at N = 40, 30 at N = 20).
    pub trade_floor: usize,
    /// The derived condition (b) floor for `universe_size` (`round_half_up(0.30·N)`;
    /// 12 at N = 40, 6 at N = 20).
    pub breadth_floor: usize,
    /// Total realized (closed) trades — condition (a) numerator.
    pub total_trades: usize,
    /// Condition (a): `total_trades >= trade_floor`.
    pub trade_floor_pass: bool,
    /// Count of symbols with ≥ `SYMBOL_TRADE_FLOOR` trades — condition (b) numerator.
    pub symbols_meeting_breadth: usize,
    /// Condition (b): `symbols_meeting_breadth >= breadth_floor`.
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

/// The turn-5 **edge-quality** evaluation (R4, KTD-4): the realized edge stats
/// (win-rate, expectancy, total P&L) read from the [`PerformanceReport`] summary,
/// with single-symbol **dominance still capped** but the turn-3/4 frequency and
/// breadth floors **retired** — per-day trading clears those by construction, so the
/// verdict now judges whether the strategy shows a real, evaluable edge rather than a
/// trade-count bar. These are the reset-invariant per-trade stats only (KTD-7); the
/// evaluation deliberately does NOT read the union `max_drawdown` / `equity_curve`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeEvaluation {
    /// Total realized (closed) trades.
    pub num_trades: usize,
    /// Total realized P&L (KRW, signed).
    pub pnl_total: f64,
    /// Win rate over closed trades (`None` when no edge stat was computed — e.g. a
    /// zero-trade run whose summary lacks the analyzer keys).
    pub win_rate: Option<f64>,
    /// Expectancy (KRW per trade) over closed trades (`None` — see [`Self::win_rate`]).
    pub expectancy: Option<f64>,
    /// The dominance metric `max(|per-symbol P&L|) / Σ|per-symbol P&L|` (0.0 in the
    /// degenerate all-zero case).
    pub max_abs_pnl_share: f64,
    /// Condition (c) retained: `max_abs_pnl_share <= DOMINANCE_CAP` and not degenerate.
    pub dominance_pass: bool,
    /// True when aggregate |P&L| is zero (dominance undefined → fail-closed).
    pub degenerate_zero_pnl: bool,
    /// Per-symbol fold, sorted by symbol for deterministic output.
    pub per_symbol: Vec<SymbolAggregate>,
    /// The edge verdict input: a **positive expectancy** with **dominance capped**
    /// over at least one closed trade. `true` ⇒ the strategy advances (R4/R5);
    /// `false` ⇒ a flat/negative/dominated (or zero-trade) run — a recorded finding
    /// with the next lever named, not an auto-pass and not a turn failure (R5).
    pub is_edge: bool,
    /// Human-readable reasons the run is not an edge (empty ⇒ `is_edge` holds).
    pub failing_conditions: Vec<String>,
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

    /// Evaluate the re-registered decisiveness bar (R1, KTD-2) over the realized
    /// ledger, with the two count floors scaled to the **pinned** `universe_size`
    /// (`universe_top_n`, R3 — never the realized snapshot).
    ///
    /// A trade is *realized* iff it is closed (`ts_closed.is_some()`) — matching the
    /// `num_trades` summary. The three conditions (at N = 40 the floors are 60/12):
    /// - (a) `total_trades >= trade_floor(N)` (`round_half_up(1.5·N)`);
    /// - (b) `>= breadth_floor(N)` (`round_half_up(0.30·N)`) symbols each with `>= 2`
    ///   realized trades;
    /// - (c) no single symbol exceeds 40% of aggregate |P&L|, where the share is
    ///   `max(|per-symbol realized P&L|) / Σ|per-symbol realized P&L|` — an
    ///   absolute-magnitude share, well-defined under mixed signs (a signed
    ///   share-of-net can exceed 100% or go negative).
    ///
    /// Boundaries pass inclusively (exactly the floor / exactly 40.0% all pass). The
    /// degenerate all-zero-P&L case (denominator 0) fails closed to
    /// insufficient-evidence with a named condition. Reduces to the turn-3 bar at
    /// N = 20 (floors 30/6, R1b).
    /// The shared closed-trade fold + dominance derivation (condition (c)) — see
    /// [`DominanceFold`]. Both [`Self::bar_evaluation`] and [`Self::edge_evaluation`]
    /// build on this so the dominance metric can never drift between the two verdicts.
    fn dominance_fold(&self) -> DominanceFold {
        use std::collections::BTreeMap;

        let mut folded: BTreeMap<String, (usize, f64)> = BTreeMap::new();
        for t in self.trades.iter().filter(|t| t.ts_closed.is_some()) {
            let e = folded.entry(t.symbol.clone()).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += t.realized_pnl;
        }
        let total_trades: usize = folded.values().map(|(c, _)| *c).sum();
        let sum_abs: f64 = folded.values().map(|(_, p)| p.abs()).sum();
        let max_abs: f64 = folded.values().map(|(_, p)| p.abs()).fold(0.0, f64::max);
        let degenerate_zero_pnl = sum_abs == 0.0;
        let max_abs_pnl_share = if degenerate_zero_pnl { 0.0 } else { max_abs / sum_abs };
        let dominance_pass = !degenerate_zero_pnl && max_abs_pnl_share <= bar::DOMINANCE_CAP;
        let per_symbol: Vec<SymbolAggregate> = folded
            .iter()
            .map(|(symbol, (trades, realized_pnl))| SymbolAggregate {
                symbol: symbol.clone(),
                trades: *trades,
                realized_pnl: *realized_pnl,
                abs_pnl_share: if degenerate_zero_pnl { 0.0 } else { realized_pnl.abs() / sum_abs },
            })
            .collect();
        DominanceFold {
            folded,
            total_trades,
            max_abs_pnl_share,
            dominance_pass,
            degenerate_zero_pnl,
            per_symbol,
        }
    }

    pub fn bar_evaluation(&self, universe_size: usize) -> BarEvaluation {
        let trade_floor = bar::trade_floor(universe_size);
        let breadth_floor = bar::breadth_floor(universe_size);

        let DominanceFold {
            folded,
            total_trades,
            max_abs_pnl_share,
            dominance_pass,
            degenerate_zero_pnl,
            per_symbol,
        } = self.dominance_fold();

        let symbols_meeting_breadth = folded
            .values()
            .filter(|(c, _)| *c >= bar::SYMBOL_TRADE_FLOOR)
            .count();

        let trade_floor_pass = total_trades >= trade_floor;
        let breadth_pass = symbols_meeting_breadth >= breadth_floor;
        // `dominance_pass` (condition (c)) comes from the shared fold, already
        // fail-closed on the degenerate all-zero case.

        let mut failing_conditions = Vec::new();
        if !trade_floor_pass {
            failing_conditions.push(format!(
                "trade-count floor not met ({total_trades} < {trade_floor})"
            ));
        }
        if !breadth_pass {
            failing_conditions.push(format!(
                "symbol-breadth floor not met ({symbols_meeting_breadth} < {breadth_floor})"
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
            universe_size,
            trade_floor,
            breadth_floor,
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

    /// Evaluate the turn-5 **edge-quality** verdict (R4/R5, KTD-4): read the realized
    /// edge stats (win-rate, expectancy, total P&L) already computed into the summary
    /// by [`Self::assemble`], keep the single-symbol **dominance** guard (condition
    /// (c)), and **retire** the trade-count (a) and breadth (b) floors — per-day
    /// trading clears those by construction, so a measurable edge implicitly proves the
    /// per-day reset fires (there is no separate frequency floor gate).
    ///
    /// `is_edge` holds iff there is at least one closed trade with a **positive
    /// expectancy** and dominance under the cap. The degenerate all-zero-P&L case fails
    /// closed (dominance undefined). A zero-trade run is not an edge (edge undefined) —
    /// the fail-closed/insufficient branch. Reads only the reset-invariant per-trade
    /// stats; never the union drawdown / equity curve (KTD-7).
    pub fn edge_evaluation(&self) -> EdgeEvaluation {
        // The shared closed-trade fold + dominance (condition (c)), kept identical to
        // the R1 bar via `dominance_fold`.
        let DominanceFold {
            folded,
            total_trades: num_trades,
            max_abs_pnl_share,
            dominance_pass,
            degenerate_zero_pnl,
            per_symbol,
        } = self.dominance_fold();

        // Edge stats read from the already-assembled summary (KTD-4). A raw report
        // whose summary was never assembled (no analyzer keys) falls back to the fold.
        let pnl_total = self
            .summary
            .get("pnl_total")
            .copied()
            .unwrap_or_else(|| folded.values().map(|(_, p)| *p).sum());
        let win_rate = self.summary.get("Win Rate").copied();
        let expectancy = self.summary.get("Expectancy").copied();

        let mut failing_conditions = Vec::new();
        if num_trades == 0 {
            failing_conditions
                .push("no closed trades — edge undefined (insufficient evidence)".to_string());
        } else {
            let positive_expectancy = expectancy.map(|e| e > 0.0).unwrap_or(false);
            if !positive_expectancy {
                failing_conditions.push(match expectancy {
                    Some(e) => format!("expectancy not positive ({e:.2} KRW/trade)"),
                    None => "expectancy unavailable (no analyzer stat)".to_string(),
                });
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
        }

        let is_edge = num_trades > 0
            && expectancy.map(|e| e > 0.0).unwrap_or(false)
            && dominance_pass;

        EdgeEvaluation {
            num_trades,
            pnl_total,
            win_rate,
            expectancy,
            max_abs_pnl_share,
            dominance_pass,
            degenerate_zero_pnl,
            per_symbol,
            is_edge,
            failing_conditions,
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

    /// The R1 bar over a raw ledger at a pinned universe size `n` (no analyzer
    /// round-trip needed — the bar folds the trades directly).
    fn eval(trades: Vec<TradeRecord>, n: usize) -> BarEvaluation {
        PerformanceReport { trades, equity_curve: vec![], summary: Default::default() }
            .bar_evaluation(n)
    }

    // --- Floor scaling / backward-compat (KTD-2, R1b, AE5) -------------------

    #[test]
    fn floors_scale_with_universe_and_reduce_to_turn3_bar_at_n20() {
        // AE5 / R1b: the generalization reproduces the turn-3 bar (30, 6) at N = 20.
        assert_eq!(bar::trade_floor(20), 30, "turn-3 trade floor");
        assert_eq!(bar::breadth_floor(20), 6, "turn-3 breadth floor");
        // Pinned N = 40 → (60, 12).
        assert_eq!(bar::trade_floor(40), 60);
        assert_eq!(bar::breadth_floor(40), 12);
    }

    #[test]
    fn floors_round_half_up_deterministically_for_odd_n() {
        // Odd N = 37: 1.5·37 = 55.5 → round-half-up 56; 0.30·37 = 11.1 → 11.
        assert_eq!(bar::trade_floor(37), 56, "round(55.5) half-up → 56");
        assert_eq!(bar::breadth_floor(37), 11, "round(11.1) → 11");
        // The output is a pure function of N — deterministic for any N (KTD-2).
        // (Exact half-way cases on 0.30·N are not exercised: 0.30 is not
        // representable in f64, so no 0.30·N lands exactly on x.5; the rule is fixed
        // and deterministic regardless, and at the pinned N = 40 the floors are
        // integers so rounding is moot.)
        assert_eq!(bar::trade_floor(37), bar::trade_floor(37), "deterministic");
    }

    #[test]
    fn backward_compat_full_eval_at_n20_matches_turn3_boundary() {
        // The turn-3 boundary vector (exactly 30 trades / 6 breadth-symbols / 40.0%)
        // still clears at N = 20 — the generalization is behavior-preserving there.
        let mut trades = trades_for("D.XKRX", 5, 40_000.0 / 5.0); // 40k dominant
        for i in 0..5 {
            trades.extend(trades_for(&format!("N{i}.XKRX"), 5, 12_000.0 / 5.0)); // 12k each → 60k
        }
        let b = eval(trades, 20);
        assert_eq!(b.universe_size, 20);
        assert_eq!(b.trade_floor, 30);
        assert_eq!(b.breadth_floor, 6);
        assert_eq!(b.total_trades, 30);
        assert!(b.trade_floor_pass && b.breadth_pass && b.dominance_pass);
        assert!(b.all_pass, "turn-3 boundary clears at N = 20: {:?}", b.failing_conditions);
    }

    // --- N = 40 acceptance vectors (AE1–AE4, floors 60/12) -------------------

    #[test]
    fn ae1_bar_cleared_all_three_conditions_pass_n40() {
        // AE1: 72 trades across 16 symbols (each ≥ 2), spread P&L → max share well
        // under 40% → all three PASS against the N = 40 floors (60, 12).
        // Counts: 8 symbols × 5 + 8 symbols × 4 = 40 + 32 = 72, all ≥ 2.
        let mut trades = Vec::new();
        for i in 0..8 {
            trades.extend(trades_for(&format!("A{i}.XKRX"), 5, 100.0 + i as f64 * 10.0));
        }
        for i in 0..8 {
            trades.extend(trades_for(&format!("B{i}.XKRX"), 4, 100.0 + i as f64 * 10.0));
        }
        let b = eval(trades, 40);
        assert_eq!(b.trade_floor, 60);
        assert_eq!(b.breadth_floor, 12);
        assert_eq!(b.total_trades, 72);
        assert!(b.trade_floor_pass, "(a) 72 ≥ 60");
        assert_eq!(b.symbols_meeting_breadth, 16);
        assert!(b.breadth_pass, "(b) 16 ≥ 12");
        assert!(b.max_abs_pnl_share <= 0.40, "share {} ≤ 40%", b.max_abs_pnl_share);
        assert!(b.dominance_pass, "(c) passes");
        assert!(b.all_pass, "bar cleared: {:?}", b.failing_conditions);
        assert!(b.failing_conditions.is_empty());
    }

    #[test]
    fn ae2_trade_floor_missed_n40() {
        // AE2: 51 total trades (17 symbols × 3) → (a) FAILs against the 60 floor.
        let mut trades = Vec::new();
        for i in 0..17 {
            trades.extend(trades_for(&format!("S{i}.XKRX"), 3, 100.0 + i as f64));
        }
        let b = eval(trades, 40);
        assert_eq!(b.total_trades, 51);
        assert!(!b.trade_floor_pass);
        assert!(!b.all_pass);
        assert!(
            b.failing_conditions.iter().any(|m| m == "trade-count floor not met (51 < 60)"),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn ae3_breadth_floor_missed_n40() {
        // AE3: 63 trades but only 10 symbols with ≥ 2 (10 × 6 = 60, plus 3 singletons
        // = 63); (a) passes (63 ≥ 60), (b) FAILs against the 12 floor.
        let mut trades = Vec::new();
        // Per-symbol totals spread so no single symbol dominates.
        for i in 0..10 {
            trades.extend(trades_for(&format!("B{i}.XKRX"), 6, 900.0 + i as f64 * 20.0));
        }
        for i in 0..3 {
            trades.extend(trades_for(&format!("X{i}.XKRX"), 1, 50.0 + i as f64));
        }
        let b = eval(trades, 40);
        assert_eq!(b.total_trades, 63);
        assert!(b.trade_floor_pass, "(a) 63 ≥ 60");
        assert_eq!(b.symbols_meeting_breadth, 10);
        assert!(!b.breadth_pass);
        assert!(!b.all_pass);
        assert!(
            b.failing_conditions.iter().any(|m| m == "symbol-breadth floor not met (10 < 12)"),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn ae4_dominance_guard_tripped_n40() {
        // AE4: 65 trades across 13 symbols (5 each); one symbol carries 47% of
        // aggregate |P&L|. (a) 65 ≥ 60 and (b) 13 ≥ 12 pass; (c) FAILs.
        // Dominant 47,000; 11 symbols × 4,000 = 44,000; 1 symbol × 9,000 = 9,000.
        // Σ|P&L| = 47k + 44k + 9k = 100k → share 0.47.
        let mut trades = trades_for("DOM.XKRX", 5, 47_000.0 / 5.0);
        for i in 0..11 {
            trades.extend(trades_for(&format!("R{i}.XKRX"), 5, 4_000.0 / 5.0));
        }
        trades.extend(trades_for("Q.XKRX", 5, 9_000.0 / 5.0));
        let b = eval(trades, 40);
        assert_eq!(b.total_trades, 65);
        assert_eq!(b.symbols_meeting_breadth, 13);
        assert!(b.trade_floor_pass && b.breadth_pass, "(a)+(b) hold");
        assert!((b.max_abs_pnl_share - 0.47).abs() < 1e-9, "share {}", b.max_abs_pnl_share);
        assert!(!b.dominance_pass);
        assert!(!b.all_pass);
        assert!(
            b.failing_conditions.iter().any(|m| m == "single-symbol dominance (47% > 40%)"),
            "messages: {:?}",
            b.failing_conditions
        );
    }

    #[test]
    fn boundary_exact_thresholds_all_pass_n40() {
        // Exactly 60 trades / exactly 12 breadth-symbols / exactly 40.0% abs-share.
        // 12 symbols × 5 = 60. Dominant 40,000; 10 symbols × 5,000 = 50,000; 1 symbol
        // × 10,000 = 10,000 → Σ = 100,000 → dominant share exactly 0.40.
        let mut trades = trades_for("D.XKRX", 5, 40_000.0 / 5.0); // 40k
        for i in 0..10 {
            trades.extend(trades_for(&format!("N{i}.XKRX"), 5, 5_000.0 / 5.0)); // 5k each → 50k
        }
        trades.extend(trades_for("M.XKRX", 5, 10_000.0 / 5.0)); // 10k
        let b = eval(trades, 40);
        assert_eq!(b.total_trades, 60);
        assert!(b.trade_floor_pass, "(a) exactly 60 passes");
        assert_eq!(b.symbols_meeting_breadth, 12);
        assert!(b.breadth_pass, "(b) exactly 12 passes");
        assert!((b.max_abs_pnl_share - 0.40).abs() < 1e-9, "share {}", b.max_abs_pnl_share);
        assert!(b.dominance_pass, "(c) exactly 40.0% passes inclusively");
        assert!(b.all_pass, "boundary clears: {:?}", b.failing_conditions);
    }

    // --- Scale-invariant mechanics (unchanged from turn 3) -------------------

    #[test]
    fn mixed_sign_abs_share_never_exceeds_100_or_goes_negative() {
        // A +200k winner against −100k of losers (net +100k). Abs-share of the winner
        // is 200k / 300k = 67% — a signed share-of-net would read 200%.
        let mut trades = trades_for("WIN.XKRX", 2, 100_000.0); // +200k
        trades.extend(trades_for("LOSS.XKRX", 2, -50_000.0)); // −100k
        let b = eval(trades, 20);
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
    fn degenerate_all_zero_pnl_fails_closed() {
        // 30 trades across 6 symbols, all P&L zero → denominator 0 → dominance
        // undefined → fail-closed, even though (a)+(b) hold at N = 20 (floors 30/6).
        let mut trades = Vec::new();
        for i in 0..6 {
            trades.extend(trades_for(&format!("Z{i}.XKRX"), 5, 0.0));
        }
        let b = eval(trades, 20);
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
    fn empty_ledger_fails_all_n40() {
        let b = eval(vec![], 40);
        assert_eq!(b.total_trades, 0);
        assert!(!b.trade_floor_pass && !b.breadth_pass && !b.dominance_pass);
        assert!(!b.all_pass);
        // Trade floor + breadth both named against the N = 40 floors.
        assert!(b.failing_conditions.iter().any(|m| m == "trade-count floor not met (0 < 60)"));
        assert!(b.failing_conditions.iter().any(|m| m == "symbol-breadth floor not met (0 < 12)"));
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
        let b = eval(trades, 20);
        assert_eq!(b.total_trades, 2, "open leg excluded from the trade count");
        assert_eq!(b.per_symbol.len(), 1, "only the closed symbol is folded");
        assert_eq!(b.per_symbol[0].symbol, "A.XKRX");
        assert!(!b.per_symbol.iter().any(|s| s.symbol == "OPEN.XKRX"), "open symbol absent");
        // The open leg's 9,999,999 P&L did not enter the dominance denominator:
        // the sole closed symbol carries 100% of aggregate |P&L|.
        assert!((b.max_abs_pnl_share - 1.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // U3 — the turn-5 edge-quality evaluation (R4/R5, KTD-4): dominance kept,
    // frequency/breadth retired.
    // -----------------------------------------------------------------------

    #[test]
    fn edge_positive_expectancy_with_dominance_capped_is_an_edge() {
        // 4 symbols × 3 winning trades, equal per-symbol P&L → 25% dominance share
        // each → (c) passes, positive expectancy → a real edge (strategy advances).
        let mut trades = Vec::new();
        for i in 0..4 {
            trades.extend(trades_for(&format!("S{i}.XKRX"), 3, 100.0));
        }
        let report = PerformanceReport::assemble(trades, 1_000_000.0);
        let e = report.edge_evaluation();
        assert_eq!(e.num_trades, 12);
        assert!(e.expectancy.unwrap() > 0.0, "expectancy positive");
        assert!(e.win_rate.unwrap() > 0.0, "win rate surfaced");
        assert!(e.dominance_pass, "equal shares → dominance under cap");
        assert!(e.is_edge, "positive expectancy + dominance capped: {:?}", e.failing_conditions);
        assert!(e.failing_conditions.is_empty());
    }

    #[test]
    fn edge_retires_the_frequency_bar_high_count_negative_expectancy_is_no_edge() {
        // >60 closed trades but a net-negative expectancy → NOT an auto-pass: the
        // retired trade-count floor cannot rescue a losing strategy (R4/R5).
        let mut trades = Vec::new();
        for i in 0..12 {
            trades.extend(trades_for(&format!("L{i}.XKRX"), 4, -100.0)); // 48 losers
            trades.extend(trades_for(&format!("W{i}.XKRX"), 2, 50.0)); // 24 winners
        }
        let report = PerformanceReport::assemble(trades, 10_000_000.0);
        let e = report.edge_evaluation();
        assert!(e.num_trades > 60, "trade count clears the retired floor: {}", e.num_trades);
        assert!(e.expectancy.unwrap() < 0.0, "net expectancy negative");
        assert!(e.dominance_pass, "spread P&L → dominance under cap");
        assert!(!e.is_edge, "a high trade count does not auto-pass a losing edge");
        assert!(
            e.failing_conditions.iter().any(|c| c.contains("expectancy not positive")),
            "expectancy named as failing: {:?}",
            e.failing_conditions
        );
    }

    #[test]
    fn edge_single_symbol_dominance_trips_even_when_winning() {
        // One symbol carries all the P&L → 100% dominance → not an edge, even winning.
        let report = PerformanceReport::assemble(trades_for("SOLE.XKRX", 3, 100.0), 1_000_000.0);
        let e = report.edge_evaluation();
        assert!(e.expectancy.unwrap() > 0.0);
        assert!((e.max_abs_pnl_share - 1.0).abs() < 1e-9);
        assert!(!e.dominance_pass);
        assert!(!e.is_edge, "dominance caps a single-symbol edge");
        assert!(e.failing_conditions.iter().any(|c| c.contains("single-symbol dominance")));
    }

    #[test]
    fn edge_degenerate_all_zero_pnl_fails_closed() {
        let mut trades = Vec::new();
        for i in 0..3 {
            trades.extend(trades_for(&format!("Z{i}.XKRX"), 3, 0.0));
        }
        let report = PerformanceReport::assemble(trades, 1_000_000.0);
        let e = report.edge_evaluation();
        assert_eq!(e.num_trades, 9);
        assert!(e.degenerate_zero_pnl);
        assert!(!e.dominance_pass, "degenerate dominance fails closed");
        assert!(!e.is_edge);
        assert!(
            e.failing_conditions.iter().any(|c| c.contains("dominance undefined")),
            "degenerate named: {:?}",
            e.failing_conditions
        );
    }

    #[test]
    fn edge_zero_trades_is_not_an_edge() {
        let report = PerformanceReport::assemble(vec![], 1_000_000.0);
        let e = report.edge_evaluation();
        assert_eq!(e.num_trades, 0);
        assert!(!e.is_edge);
        assert!(e.win_rate.is_none(), "no edge stat on a zero-trade run");
        assert!(e.failing_conditions.iter().any(|c| c.contains("no closed trades")));
    }
}
