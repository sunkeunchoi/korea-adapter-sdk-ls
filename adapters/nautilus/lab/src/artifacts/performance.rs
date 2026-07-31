//! The performance report (KTD3, R5) — the trade/fill ledger, per-trade P&L, an
//! equity curve, and summary statistics. Summary stats reuse `nautilus-analysis`'s
//! `PortfolioAnalyzer` rather than reimplementing them; the per-trade ledger and
//! equity curve are assembled from the engine's fill/position events.

use std::collections::BTreeMap;

use nautilus_analysis::analyzer::PortfolioAnalyzer;
use nautilus_model::position::Position;
use nautilus_model::types::{Currency, Money};
use serde::{Deserialize, Serialize};

use crate::strategy::orb::TransactionCostModel;

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
    /// Entry-fixed **risk capital** deployed on this trade (KRW): `qty ·
    /// risk_per_share`, where `risk_per_share = entry_price − stop_price` (the
    /// initial, entry-fixed stop). Joined from the strategy's per-position entry
    /// risk at ledger assembly (R4) — the trade ledger cannot derive it alone (the
    /// stop lives on the strategy's `OrbState`, not the nautilus `Position`).
    /// `None` for a run with no strategy risk map, a legacy artifact, or a
    /// degenerate (`risk_per_share ≤ 0` / `qty ≤ 0`) entry — the R-metrics then
    /// fall back to the legacy P&L path. **Additive**: absent from a pre-field
    /// `performance.json` (serde default), so every legacy artifact still
    /// deserializes and the existing summary keys are byte-unchanged (KTD-D).
    #[serde(default)]
    pub risk_capital: Option<f64>,
    /// The realized **R-multiple** of this trade (R1/R4): `realized_pnl /
    /// risk_capital` = `(exit − entry) / (entry − stop)`, **size-invariant** by
    /// construction (no `qty` term survives the ratio). `None` while open, when no
    /// risk was joined, or on a degenerate risk (mirrors [`Self::risk_capital`]).
    /// Additive — see [`Self::risk_capital`].
    #[serde(default)]
    pub realized_r: Option<f64>,
}

/// Entry-time risk for one opened position (R4), captured by the strategy at order
/// placement and joined into the trade ledger at assembly. `risk_per_share =
/// entry_price − stop_price` (the entry-fixed initial stop — the same per-share
/// risk the `risk_per_trade_krw` sizing lever divides its budget by, R5). The
/// deployed risk capital is `qty · risk_per_share`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntryRisk {
    /// The entry-fixed per-share risk (`entry_price − stop_price`, KRW).
    pub risk_per_share: f64,
    /// The filled entry quantity (shares).
    pub qty: f64,
}

/// Join entry-time risk into a trade's ledger fields (R4): `risk_capital = qty ·
/// risk_per_share`, `realized_r = realized_pnl / risk_capital` (only for a closed
/// trade). Returns `(None, None)` when no risk was captured or the per-share risk /
/// qty is non-positive (degenerate) — the trade then evaluates via the legacy P&L
/// path, and never yields a NaN/Inf from a zero denominator.
fn joined_risk(realized_pnl: f64, closed: bool, risk: Option<EntryRisk>) -> (Option<f64>, Option<f64>) {
    match risk {
        Some(r) if r.risk_per_share > 0.0 && r.qty > 0.0 => {
            let risk_capital = r.qty * r.risk_per_share;
            let realized_r = closed.then_some(realized_pnl / risk_capital);
            (Some(risk_capital), realized_r)
        }
        _ => (None, None),
    }
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
    // ---- Size-invariant risk metrics (R1/R2/R3, U2) ----
    // Present (`Some`/computed) only when **every** closed trade carries
    // `risk_capital` — a run with any risk-less closed trade falls back to the
    // legacy P&L path, so old artifacts and the pre-risk-field re-baseline still
    // evaluate.
    /// Return-on-risk `Σrealized_pnl / Σrisk_capital` — the risk-weighted mean R,
    /// the size-invariant edge crux (R1). `None` when risk info is absent or the
    /// deployed risk is degenerate (zero total).
    return_on_risk: Option<f64>,
    /// Equal-weight mean of per-trade `realized_r` (R2) — the size-invariant
    /// diagnostic invariant. `None` when risk info is absent/degenerate.
    mean_realized_r: Option<f64>,
    /// Total deployed risk capital over closed trades (R1). `None` when absent.
    risk_capital_total: Option<f64>,
    /// `max(per-symbol Σrisk_capital) / Σrisk_capital` (R3). `None` when absent.
    max_risk_capital_share: Option<f64>,
    /// `max_risk_capital_share <= DOMINANCE_CAP` and not degenerate (R3). `None`
    /// when risk info is absent (the verdict then gates on P&L-dominance).
    risk_dominance_pass: Option<bool>,
    /// Deployed risk is present but sums to zero (risk-dominance undefined →
    /// fail-closed). `false` when risk info is absent (there is nothing to gate).
    degenerate_zero_risk: bool,
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
    // ---- Size-invariant edge metrics (R1/R2/R3, CLASS B) ----
    /// **Return-on-risk** `Σrealized_pnl / Σrisk_capital` — the size-invariant edge
    /// crux (R1): flat under a uniform size-up, responsive to risk reallocation. The
    /// KEEP verdict is authored against this (R6). `None` when no closed trade carries
    /// risk info (the legacy P&L path) or deployed risk is degenerate.
    pub return_on_risk: Option<f64>,
    /// Equal-weight mean of per-trade `realized_r` (R2) — a size-invariant diagnostic
    /// *invariant*, inert to a pure sizing change (not a verdict input). `None` when
    /// risk info is absent/degenerate.
    pub mean_realized_r: Option<f64>,
    /// Total deployed risk capital over closed trades (R1). `None` when absent.
    pub risk_capital_total: Option<f64>,
    /// `max(per-symbol Σrisk_capital) / Σrisk_capital` (R3) — the **decisional**
    /// dominance share under variable sizing (cannot be gamed by sizing one symbol
    /// huge). `None` when risk info is absent (verdict gates on `dominance_pass`).
    pub max_risk_capital_share: Option<f64>,
    /// Condition (c) re-grounded (R3): `max_risk_capital_share <= DOMINANCE_CAP` and
    /// not degenerate. `None` when risk info is absent. When present, this — not
    /// `dominance_pass` — is the `is_edge` dominance gate.
    pub risk_dominance_pass: Option<bool>,
    /// Deployed risk is present but sums to zero (risk-dominance undefined →
    /// fail-closed). `false` when risk info is absent.
    pub degenerate_zero_risk: bool,
    /// The edge verdict input: a **positive expectancy** with **dominance capped**
    /// over at least one closed trade. Dominance gates on **risk-capital share** when
    /// present (R3), P&L share otherwise (legacy). `true` ⇒ the strategy advances
    /// (R4/R5); `false` ⇒ a flat/negative/dominated (or zero-trade) run — a recorded
    /// finding with the next lever named, not an auto-pass and not a turn failure (R5).
    pub is_edge: bool,
    /// Human-readable reasons the run is not an edge (empty ⇒ `is_edge` holds).
    pub failing_conditions: Vec<String>,
}

impl EdgeEvaluation {
    /// The strategy-loop KEEP crux, as ONE definition (the governed turn's verdict
    /// reads this — never re-expresses the rule inline): a flip KEEPs iff its
    /// size-invariant **return-on-risk strictly exceeds** the prior head's **and**
    /// risk-cap dominance holds. When either run carries no return-on-risk (a
    /// legacy / pre-CLASS-B head that predates the metric), fall back to the edge
    /// flag — the size-honest RoR comparison is undefined without both sides.
    ///
    /// `risk_dominance_pass == None` is "risk info absent" (a legacy run, not a
    /// failed check), so it is **not** a dominance failure here — such a run's
    /// dominance was already gated on P&L-share inside [`Self::is_edge`]; only an
    /// explicit `Some(false)` (a computed, tripped risk-cap dominance) blocks KEEP.
    pub fn keeps_over(&self, prior: Option<&EdgeEvaluation>) -> bool {
        match (self.return_on_risk, prior.and_then(|p| p.return_on_risk)) {
            (Some(new_ror), Some(prior_ror)) => {
                new_ror > prior_ror && self.risk_dominance_pass != Some(false)
            }
            _ => self.is_edge,
        }
    }
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
    /// live path): each position becomes a [`TradeRecord`] with its fill ledger. No
    /// per-position risk is joined — every `risk_capital`/`realized_r` is `None` (the
    /// legacy path; the R-metrics fall back to P&L).
    pub fn from_positions(positions: &[Position], starting_balance: f64) -> Self {
        Self::from_positions_with_risk(positions, &[], starting_balance, None)
    }

    /// Assemble a report from finished positions **plus** an index-aligned slice of
    /// entry-time risk (R4): `risks[i]` is the captured [`EntryRisk`] for
    /// `positions[i]` (`None` where none was captured). A shorter/empty `risks`
    /// slice leaves the trailing positions risk-less (the legacy path). Populates
    /// each [`TradeRecord`]'s additive `risk_capital`/`realized_r` without disturbing
    /// any existing summary key.
    ///
    /// `costs` is the transaction-cost model (orb-transaction-cost-model), applied
    /// per fill at trade booking — commission on both sides, statutory tax on sell
    /// notional only — so `realized_pnl`, `realized_r`, the equity curve, and every
    /// summary stat are **net** of modeled costs. `None` (or zero rates upstream)
    /// takes the pre-model path untouched: byte-identical artifacts. This is the
    /// backtest assembly seam only — live-session reports (`assemble` over the fill
    /// ledger) stay zero-cost, because the rung-1 expectation band was frozen from a
    /// zero-cost distribution and re-deriving it is a separate governed act.
    pub fn from_positions_with_risk(
        positions: &[Position],
        risks: &[Option<EntryRisk>],
        starting_balance: f64,
        costs: Option<&TransactionCostModel>,
    ) -> Self {
        let trades = positions
            .iter()
            .enumerate()
            .map(|(i, p)| trade_from_position(p, risks.get(i).copied().flatten(), costs))
            .collect();
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
        // Risk fold (U2): per-symbol Σrisk_capital, Σrealized_pnl and Σrealized_r
        // over closed trades — computed only if EVERY closed trade carries risk.
        let mut risk_by_symbol: BTreeMap<String, f64> = BTreeMap::new();
        let mut all_have_risk = true;
        let mut all_have_realized_r = true;
        let mut risk_pnl_sum = 0.0;
        let mut realized_r_sum = 0.0;
        for t in self.trades.iter().filter(|t| t.ts_closed.is_some()) {
            let e = folded.entry(t.symbol.clone()).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += t.realized_pnl;
            match t.risk_capital {
                Some(rc) => *risk_by_symbol.entry(t.symbol.clone()).or_insert(0.0) += rc,
                None => all_have_risk = false,
            }
            match t.realized_r {
                Some(r) => realized_r_sum += r,
                None => all_have_realized_r = false,
            }
            risk_pnl_sum += t.realized_pnl;
        }
        let total_trades: usize = folded.values().map(|(c, _)| *c).sum();

        // The R-metrics exist only when there is at least one closed trade AND every
        // closed trade carries `risk_capital` (R1/R4 fallback discipline).
        let risk_present = total_trades > 0 && all_have_risk;
        let (
            return_on_risk,
            mean_realized_r,
            risk_capital_total,
            max_risk_capital_share,
            risk_dominance_pass,
            degenerate_zero_risk,
        ) = if risk_present {
            let total: f64 = risk_by_symbol.values().sum();
            let degenerate = total == 0.0;
            let max_risk: f64 = risk_by_symbol.values().copied().fold(0.0, f64::max);
            let share = if degenerate { 0.0 } else { max_risk / total };
            let dom_pass = !degenerate && share <= bar::DOMINANCE_CAP;
            let ror = (!degenerate).then_some(risk_pnl_sum / total);
            // Equal-weight mean-R is the mean of the size-invariant per-trade
            // `realized_r`; `None` if any closed trade lacks it (degenerate join).
            let mean_r = (!degenerate && all_have_realized_r)
                .then_some(realized_r_sum / total_trades as f64);
            (ror, mean_r, Some(total), Some(share), Some(dom_pass), degenerate)
        } else {
            (None, None, None, None, None, false)
        };
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
            return_on_risk,
            mean_realized_r,
            risk_capital_total,
            max_risk_capital_share,
            risk_dominance_pass,
            degenerate_zero_risk,
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
            ..
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
        // The shared closed-trade fold + dominance (condition (c)) + the size-invariant
        // risk metrics (R1/R2/R3), kept identical to the R1 bar via `dominance_fold`.
        let DominanceFold {
            folded,
            total_trades: num_trades,
            max_abs_pnl_share,
            dominance_pass,
            degenerate_zero_pnl,
            per_symbol,
            return_on_risk,
            mean_realized_r,
            risk_capital_total,
            max_risk_capital_share,
            risk_dominance_pass,
            degenerate_zero_risk,
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

        // Dominance is gated on **risk-capital share** when the run carries risk info
        // (R3, size-robust); on the legacy |P&L| share otherwise (old artifacts / the
        // pre-risk-field re-baseline still evaluate).
        let risk_metrics_present = risk_dominance_pass.is_some();
        let effective_dominance_pass = risk_dominance_pass.unwrap_or(dominance_pass);

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
            if risk_metrics_present {
                // Risk-share is the decisional dominance gate (R3).
                if degenerate_zero_risk {
                    failing_conditions.push(
                        "all deployed risk_capital is zero — risk-dominance undefined (fail-closed to insufficient-evidence)"
                            .to_string(),
                    );
                } else if !effective_dominance_pass {
                    failing_conditions.push(format!(
                        "single-symbol risk dominance ({:.0}% > {:.0}%)",
                        max_risk_capital_share.unwrap_or(0.0) * 100.0,
                        bar::DOMINANCE_CAP * 100.0
                    ));
                }
            } else if degenerate_zero_pnl {
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

        // Positivity stays on the sign of expectancy (RoR and expectancy share sign);
        // the dominance clause switches to risk-share when present (R3/R6, U2).
        let is_edge = num_trades > 0
            && expectancy.map(|e| e > 0.0).unwrap_or(false)
            && effective_dominance_pass;

        EdgeEvaluation {
            num_trades,
            pnl_total,
            win_rate,
            expectancy,
            max_abs_pnl_share,
            dominance_pass,
            degenerate_zero_pnl,
            per_symbol,
            return_on_risk,
            mean_realized_r,
            risk_capital_total,
            max_risk_capital_share,
            risk_dominance_pass,
            degenerate_zero_risk,
            is_edge,
            failing_conditions,
        }
    }
}

fn trade_from_position(
    p: &Position,
    risk: Option<EntryRisk>,
    costs: Option<&TransactionCostModel>,
) -> TradeRecord {
    // Modeled transaction costs (orb-transaction-cost-model): per-fill, sell-side
    // asymmetric, additive to any engine-charged commission (always 0.0 today — the
    // instruments carry no fee model). Booked into each fill's `commission` and
    // subtracted from `realized_pnl` BEFORE `joined_risk`, so `realized_r` and every
    // downstream aggregate are net of costs. With `costs == None` the modeled term
    // is exactly 0.0 and the record is byte-identical to the pre-model path.
    let mut modeled_cost_total = 0.0;
    let fills = p
        .events
        .iter()
        .map(|f| {
            let is_sell = matches!(f.order_side, nautilus_model::enums::OrderSide::Sell);
            let notional = f.last_qty.as_f64() * f.last_px.as_f64();
            let modeled = costs.map(|c| c.fill_cost(is_sell, notional)).unwrap_or(0.0);
            modeled_cost_total += modeled;
            FillRecord {
                ts_event: f.ts_event.as_u64(),
                side: format!("{:?}", f.order_side).to_uppercase(),
                qty: f.last_qty.as_f64(),
                price: f.last_px.as_f64(),
                trade_id: f.trade_id.to_string(),
                commission: f.commission.map(|m| m.as_f64()).unwrap_or(0.0) + modeled,
            }
        })
        .collect();
    let realized_pnl = p.realized_pnl.map(|m| m.as_f64()).unwrap_or(0.0) - modeled_cost_total;
    let closed = p.ts_closed.is_some();
    let (risk_capital, realized_r) = joined_risk(realized_pnl, closed, risk);
    TradeRecord {
        symbol: p.instrument_id.to_string(),
        entry_side: format!("{:?}", p.entry).to_uppercase(),
        quantity: p.peak_qty.as_f64(),
        avg_px_open: p.avg_px_open,
        avg_px_close: p.avg_px_close,
        realized_pnl,
        ts_opened: p.ts_opened.as_u64(),
        ts_closed: p.ts_closed.map(|t| t.as_u64()),
        fills,
        risk_capital,
        realized_r,
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

    /// An `EdgeEvaluation` with only the `keeps_over` inputs set (the rest at
    /// harmless defaults) — for testing the KEEP crux in isolation.
    fn edge(return_on_risk: Option<f64>, risk_dominance_pass: Option<bool>, is_edge: bool) -> EdgeEvaluation {
        EdgeEvaluation {
            num_trades: 1,
            pnl_total: 0.0,
            win_rate: None,
            expectancy: None,
            max_abs_pnl_share: 0.0,
            dominance_pass: true,
            degenerate_zero_pnl: false,
            per_symbol: vec![],
            return_on_risk,
            mean_realized_r: None,
            risk_capital_total: None,
            max_risk_capital_share: None,
            risk_dominance_pass,
            degenerate_zero_risk: false,
            is_edge,
            failing_conditions: vec![],
        }
    }

    #[test]
    fn keeps_over_keeps_when_ror_strictly_beats_prior_and_dominance_holds() {
        let new = edge(Some(0.1262), Some(true), true);
        let prior = edge(Some(0.1171), Some(true), true);
        assert!(new.keeps_over(Some(&prior)), "0.1262 > 0.1171 with dominance ok → KEEP");
    }

    #[test]
    fn keeps_over_reverts_when_ror_does_not_strictly_improve() {
        // Equal RoR is not a strict improvement → REVERT (pins the `>`, not `>=`).
        let new = edge(Some(0.1171), Some(true), true);
        let prior = edge(Some(0.1171), Some(true), true);
        assert!(!new.keeps_over(Some(&prior)), "tie is not a strict beat → REVERT");
        // Lower RoR → REVERT even though is_edge is true.
        let lower = edge(Some(0.1100), Some(true), true);
        assert!(!lower.keeps_over(Some(&prior)));
    }

    #[test]
    fn keeps_over_reverts_when_risk_dominance_tripped_even_if_ror_improves() {
        let new = edge(Some(0.20), Some(false), true); // dominance computed-and-failed
        let prior = edge(Some(0.10), Some(true), true);
        assert!(!new.keeps_over(Some(&prior)), "a tripped risk-cap dominance blocks KEEP");
    }

    #[test]
    fn keeps_over_falls_back_to_is_edge_when_ror_absent() {
        // A legacy head (no prior RoR) → decide on the edge flag, not RoR.
        let new_edge_true = edge(None, None, true);
        assert!(new_edge_true.keeps_over(Some(&edge(None, None, true))));
        let new_edge_false = edge(None, None, false);
        assert!(!new_edge_false.keeps_over(Some(&edge(Some(0.1), Some(true), true))));
        // None-dominance (risk info absent) is NOT a failure when RoR is present on
        // both sides.
        let new = edge(Some(0.2), None, true);
        assert!(new.keeps_over(Some(&edge(Some(0.1), Some(true), true))), "None dominance = risk-absent, not a fail");
    }

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
            risk_capital: None,
            realized_r: None,
        }
    }

    /// A closed trade carrying entry-fixed risk (U1/U2): `risk_capital = qty ·
    /// risk_per_share` and `realized_r = pnl / risk_capital`. `realized_r` is set
    /// independently (size-invariant per-trade R) so the reallocation test can hold
    /// it fixed while shifting `risk_capital` across symbols.
    fn trade_risk(
        symbol: &str,
        pnl: f64,
        risk_capital: f64,
        realized_r: f64,
        ts_open: u64,
        ts_close: u64,
    ) -> TradeRecord {
        TradeRecord {
            risk_capital: Some(risk_capital),
            realized_r: Some(realized_r),
            ..trade(symbol, pnl, ts_open, ts_close)
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
            risk_capital: None,
            realized_r: None,
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
        // No risk info on a zero-trade run — R-metrics absent, not degenerate.
        assert!(e.return_on_risk.is_none());
        assert!(e.risk_dominance_pass.is_none());
        assert!(!e.degenerate_zero_risk);
    }

    // -----------------------------------------------------------------------
    // U1 — per-trade risk carried into the ledger (R4): joined_risk + additive.
    // -----------------------------------------------------------------------

    #[test]
    fn joined_risk_happy_path_closed_trade() {
        // qty=100, entry=60000, stop=57000 → risk_per_share=3000, risk_capital=300000;
        // pnl = 100·(exit−60000); with exit=61500 pnl=150000 → realized_r=0.5.
        let risk = EntryRisk { risk_per_share: 3000.0, qty: 100.0 };
        let (rc, rr) = joined_risk(150_000.0, true, Some(risk));
        assert_eq!(rc, Some(300_000.0));
        assert_eq!(rr, Some(0.5));
    }

    #[test]
    fn joined_risk_absent_and_degenerate_yield_none() {
        // No risk captured → legacy path (both None).
        assert_eq!(joined_risk(100.0, true, None), (None, None));
        // Non-positive per-share risk / qty → None, never a divide-by-zero.
        let bad_rps = EntryRisk { risk_per_share: 0.0, qty: 100.0 };
        assert_eq!(joined_risk(100.0, true, Some(bad_rps)), (None, None));
        let bad_qty = EntryRisk { risk_per_share: 3000.0, qty: 0.0 };
        assert_eq!(joined_risk(100.0, true, Some(bad_qty)), (None, None));
    }

    #[test]
    fn joined_risk_open_leg_has_capital_but_no_realized_r() {
        // An open leg carries deployed risk_capital but no realized R (not closed).
        let risk = EntryRisk { risk_per_share: 3000.0, qty: 100.0 };
        let (rc, rr) = joined_risk(0.0, false, Some(risk));
        assert_eq!(rc, Some(300_000.0));
        assert_eq!(rr, None, "realized_r undefined while open");
    }

    #[test]
    fn risk_fields_are_additive_summary_byte_identical() {
        // Assembling the SAME trades with vs without the risk fields set must leave
        // every pre-existing summary key identical (the additive-only guarantee, R4).
        let plain = vec![
            trade("A.XKRX", 100.0, 1, 2),
            trade("B.XKRX", -50.0, 3, 4),
            trade("C.XKRX", 200.0, 5, 6),
        ];
        let with_risk = vec![
            trade_risk("A.XKRX", 100.0, 200_000.0, 0.5, 1, 2),
            trade_risk("B.XKRX", -50.0, 200_000.0, -0.25, 3, 4),
            trade_risk("C.XKRX", 200.0, 200_000.0, 1.0, 5, 6),
        ];
        let a = PerformanceReport::assemble(plain, 1_000_000.0);
        let b = PerformanceReport::assemble(with_risk, 1_000_000.0);
        assert_eq!(a.summary, b.summary, "risk fields never perturb summary keys");
        for k in ["Expectancy", "Win Rate", "pnl_total", "num_trades", "max_drawdown"] {
            assert_eq!(a.summary.get(k), b.summary.get(k), "summary key {k} unchanged");
        }
        assert_eq!(a.equity_curve, b.equity_curve, "equity curve unchanged");
    }

    #[test]
    fn trade_record_risk_none_round_trips() {
        // A legacy artifact predates the risk fields — its JSON has no such keys, yet
        // the additive serde defaults deserialize them to None (R4).
        let legacy = serde_json::json!({
            "symbol": "A.XKRX",
            "entry_side": "BUY",
            "quantity": 10.0,
            "avg_px_open": 60000.0,
            "avg_px_close": 61000.0,
            "realized_pnl": 10000.0,
            "ts_opened": 1,
            "ts_closed": 2,
            "fills": []
        })
        .to_string();
        let t: TradeRecord = serde_json::from_str(&legacy).unwrap();
        assert_eq!(t.risk_capital, None);
        assert_eq!(t.realized_r, None);
        // And a round-trip with the fields set preserves them.
        let with = trade_risk("A.XKRX", 100.0, 300_000.0, 0.5, 1, 2);
        let back: TradeRecord = serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
        assert_eq!(back, with);
    }

    // -----------------------------------------------------------------------
    // U2 — return-on-risk + mean-R + risk-dominance on the edge verdict.
    // -----------------------------------------------------------------------

    /// The RoR / risk-dominance metrics folded from a raw ledger (no analyzer
    /// round-trip needed — the fold reads the trades directly).
    fn risk_edge(trades: Vec<TradeRecord>) -> EdgeEvaluation {
        PerformanceReport { trades, equity_curve: vec![], summary: Default::default() }
            .edge_evaluation()
    }

    #[test]
    fn ror_is_invariant_to_a_uniform_size_up() {
        // The load-bearing test (R1): two ledgers identical except every qty ×k →
        // identical return_on_risk; pnl_total and Σrisk_capital both scale by k.
        let base = vec![
            trade_risk("A.XKRX", 100_000.0, 200_000.0, 0.5, 1, 2),
            trade_risk("B.XKRX", 300_000.0, 200_000.0, 1.5, 3, 4),
        ];
        let k = 3.0;
        // qty×k → risk_capital×k and realized_pnl×k; realized_r (size-invariant) held.
        let scaled = vec![
            trade_risk("A.XKRX", 100_000.0 * k, 200_000.0 * k, 0.5, 1, 2),
            trade_risk("B.XKRX", 300_000.0 * k, 200_000.0 * k, 1.5, 3, 4),
        ];
        let e0 = risk_edge(base);
        let e1 = risk_edge(scaled);
        // RoR = 400000/400000 = 1.0 in both — no free edge from leverage.
        assert!((e0.return_on_risk.unwrap() - 1.0).abs() < 1e-9);
        assert!(
            (e0.return_on_risk.unwrap() - e1.return_on_risk.unwrap()).abs() < 1e-9,
            "RoR invariant to uniform size-up: {:?} vs {:?}",
            e0.return_on_risk,
            e1.return_on_risk
        );
        // mean_realized_r is equally invariant (per-trade R unchanged).
        assert!((e0.mean_realized_r.unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(e0.mean_realized_r, e1.mean_realized_r);
        // The size-contaminated metric DID move: Σrisk_capital scaled by k.
        assert!((e1.risk_capital_total.unwrap() / e0.risk_capital_total.unwrap() - k).abs() < 1e-9);
    }

    #[test]
    fn ror_rises_under_risk_reallocation_while_mean_r_holds() {
        // Reallocation sensitivity (R1/R2): shift risk_capital from a low-realized_r
        // symbol to a high one at constant Σrisk_capital → RoR rises, mean-R ~unchanged.
        // Base: A(r=0.5, rc=100k, pnl=50k) B(r=2.0, rc=100k, pnl=200k). Σrc=200k,
        // Σpnl=250k → RoR=1.25; mean-R=(0.5+2.0)/2=1.25.
        let base = vec![
            trade_risk("A.XKRX", 50_000.0, 100_000.0, 0.5, 1, 2),
            trade_risk("B.XKRX", 200_000.0, 100_000.0, 2.0, 3, 4),
        ];
        // Reallocated: A(rc=50k, pnl=25k) B(rc=150k, pnl=300k). Σrc=200k (held),
        // Σpnl=325k → RoR=1.625; per-trade realized_r held → mean-R still 1.25.
        let realloc = vec![
            trade_risk("A.XKRX", 25_000.0, 50_000.0, 0.5, 1, 2),
            trade_risk("B.XKRX", 300_000.0, 150_000.0, 2.0, 3, 4),
        ];
        let e0 = risk_edge(base);
        let e1 = risk_edge(realloc);
        assert!((e0.return_on_risk.unwrap() - 1.25).abs() < 1e-9, "base RoR {:?}", e0.return_on_risk);
        assert!(e1.return_on_risk.unwrap() > e0.return_on_risk.unwrap(), "RoR rises on reallocation");
        assert!((e1.return_on_risk.unwrap() - 1.625).abs() < 1e-9, "realloc RoR {:?}", e1.return_on_risk);
        assert_eq!(e0.risk_capital_total, e1.risk_capital_total, "Σrisk_capital held constant");
        assert!(
            (e0.mean_realized_r.unwrap() - e1.mean_realized_r.unwrap()).abs() < 1e-9,
            "mean-R inert to reallocation: {:?} vs {:?}",
            e0.mean_realized_r,
            e1.mean_realized_r
        );
    }

    #[test]
    fn risk_dominance_trips_even_when_pnl_share_is_under_cap() {
        // One symbol carries >40% of DEPLOYED RISK while its |P&L| share stays under
        // 40% (it lost, others won) → verdict gates on risk-share and fails.
        // D: rc=600k (60% of 1.0M risk), pnl=-10k. Three winners rc≈133k each,
        // pnl=+120k each (+360k). |P&L| shares: 10/370≈2.7% and 120/370≈32% each — all
        // under 40%, so the legacy P&L guard would PASS.
        let trades = vec![
            trade_risk("D.XKRX", -10_000.0, 600_000.0, -0.017, 1, 2),
            trade_risk("W0.XKRX", 120_000.0, 133_400.0, 0.9, 3, 4),
            trade_risk("W1.XKRX", 120_000.0, 133_300.0, 0.9, 5, 6),
            trade_risk("W2.XKRX", 120_000.0, 133_300.0, 0.9, 7, 8),
        ];
        let e = risk_edge(trades);
        assert!(e.max_abs_pnl_share < 0.40, "P&L share under cap: {}", e.max_abs_pnl_share);
        assert!(e.dominance_pass, "legacy P&L dominance would pass");
        assert!(e.max_risk_capital_share.unwrap() > 0.40, "risk share over cap");
        assert_eq!(e.risk_dominance_pass, Some(false), "risk-dominance trips");
        assert!(!e.is_edge, "verdict gates on risk-share, not P&L-share");
        assert!(
            e.failing_conditions.iter().any(|c| c.contains("risk dominance")),
            "risk-dominance named: {:?}",
            e.failing_conditions
        );
    }

    #[test]
    fn risk_present_edge_reports_both_dominance_shares() {
        // A clean edge: 4 symbols, equal risk + P&L → both shares 25%, positive RoR.
        let trades = vec![
            trade_risk("A.XKRX", 100_000.0, 200_000.0, 0.5, 1, 2),
            trade_risk("B.XKRX", 100_000.0, 200_000.0, 0.5, 3, 4),
            trade_risk("C.XKRX", 100_000.0, 200_000.0, 0.5, 5, 6),
            trade_risk("D.XKRX", 100_000.0, 200_000.0, 0.5, 7, 8),
        ];
        let report = PerformanceReport::assemble(trades, 10_000_000.0);
        let e = report.edge_evaluation();
        assert!((e.max_abs_pnl_share - 0.25).abs() < 1e-9, "P&L share reported");
        assert!((e.max_risk_capital_share.unwrap() - 0.25).abs() < 1e-9, "risk share reported");
        assert_eq!(e.risk_dominance_pass, Some(true));
        assert!(e.return_on_risk.unwrap() > 0.0);
        assert!(e.is_edge, "positive RoR + risk-dominance capped: {:?}", e.failing_conditions);
    }

    #[test]
    fn risk_absent_falls_back_to_legacy_pnl_dominance() {
        // No trade carries risk_capital (`trade` sets None) → R-metrics None and the
        // verdict is byte-for-byte the legacy P&L path (identical to today).
        let mut trades = Vec::new();
        for i in 0..4 {
            trades.extend(trades_for(&format!("S{i}.XKRX"), 3, 100.0));
        }
        let report = PerformanceReport::assemble(trades, 1_000_000.0);
        let e = report.edge_evaluation();
        assert!(e.return_on_risk.is_none(), "no RoR without risk info");
        assert!(e.risk_dominance_pass.is_none(), "no risk-dominance without risk info");
        assert!(!e.degenerate_zero_risk);
        assert!(e.is_edge, "legacy P&L path still evaluates the edge");
        assert!(e.dominance_pass, "legacy dominance gate used");
    }

    #[test]
    fn mixed_risk_presence_falls_back_to_legacy_path() {
        // Any single risk-less closed trade drops the whole run to the legacy path
        // (R4 all-or-nothing) — no partial RoR from a subset.
        let trades = vec![
            trade_risk("A.XKRX", 100_000.0, 200_000.0, 0.5, 1, 2),
            trade("B.XKRX", 100_000.0, 3, 4), // risk_capital None
        ];
        let e = risk_edge(trades);
        assert!(e.return_on_risk.is_none(), "one risk-less trade → no RoR");
        assert!(e.risk_dominance_pass.is_none());
    }

    #[test]
    fn degenerate_zero_deployed_risk_fails_closed() {
        // Every closed trade carries risk_capital but it sums to zero → risk-dominance
        // undefined → fail-closed (R3), named condition, not an edge.
        let trades = vec![
            trade_risk("A.XKRX", 100_000.0, 0.0, 0.0, 1, 2),
            trade_risk("B.XKRX", 100_000.0, 0.0, 0.0, 3, 4),
        ];
        let report = PerformanceReport::assemble(trades, 1_000_000.0);
        let e = report.edge_evaluation();
        assert!(e.degenerate_zero_risk, "zero total deployed risk flagged");
        assert_eq!(e.risk_dominance_pass, Some(false), "fail-closed");
        assert!(e.return_on_risk.is_none(), "RoR undefined on zero risk");
        assert!(!e.is_edge);
        assert!(
            e.failing_conditions.iter().any(|c| c.contains("risk-dominance undefined")),
            "degenerate risk named: {:?}",
            e.failing_conditions
        );
    }

    #[test]
    fn open_leg_contributes_no_risk_to_aggregates() {
        // An OPEN leg (ts_closed=None) with a huge risk_capital must not enter the RoR
        // denominator or the risk-dominance fold — only closed trades aggregate.
        let mut trades = vec![
            trade_risk("A.XKRX", 100_000.0, 200_000.0, 0.5, 1, 2),
            trade_risk("B.XKRX", 100_000.0, 200_000.0, 0.5, 3, 4),
        ];
        trades.push(TradeRecord {
            symbol: "OPEN.XKRX".to_string(),
            entry_side: "BUY".to_string(),
            quantity: 10.0,
            avg_px_open: 60_000.0,
            avg_px_close: None,
            realized_pnl: 9_999_999.0,
            ts_opened: 100,
            ts_closed: None,
            fills: vec![],
            risk_capital: Some(9_999_999.0),
            realized_r: None,
        });
        let e = risk_edge(trades);
        assert_eq!(e.num_trades, 2, "open leg excluded from the trade count");
        assert_eq!(e.risk_capital_total, Some(400_000.0), "open risk_capital excluded");
        assert!((e.return_on_risk.unwrap() - 0.5).abs() < 1e-9, "RoR over closed only");
    }

    /// One two-fill round trip through the REAL cost-application seam
    /// (`trade_from_position`): commission books on both fills, the statutory tax on
    /// the sell fill only, and `realized_pnl`/`realized_r` net BEFORE `joined_risk` —
    /// with `costs: None` the identical position reproduces the gross (pre-model)
    /// record exactly. Guards the wiring the formula-level `fill_cost` tests cannot
    /// see (double-count, wrong-notional, buy/sell inversion).
    #[test]
    fn trade_from_position_nets_sell_asymmetric_costs_before_realized_r() {
        use nautilus_core::{UnixNanos, UUID4};
        use nautilus_model::enums::{LiquiditySide, OrderSide, OrderType};
        use nautilus_model::events::OrderFilled;
        use nautilus_model::identifiers::{
            AccountId, ClientOrderId, InstrumentId, PositionId, StrategyId, TradeId, TraderId,
            VenueOrderId,
        };
        use nautilus_model::instruments::{Equity, InstrumentAny};
        use nautilus_model::position::Position;
        use nautilus_model::types::{Price, Quantity};

        let id = InstrumentId::from("005930.XKRX");
        let equity = InstrumentAny::Equity(
            Equity::new(
                id,
                nautilus_model::identifiers::Symbol::from("005930"),
                None,
                Currency::KRW(),
                0,
                Price::from("1"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                UnixNanos::default(),
                UnixNanos::default(),
            ),
        );
        let fill = |order: &str, trade: &str, side: OrderSide, px: &str, ts: u64| {
            OrderFilled::new(
                TraderId::from("T-1"),
                StrategyId::from("S-1"),
                id,
                ClientOrderId::from(order),
                VenueOrderId::from("V-1"),
                AccountId::from("SIM-1"),
                TradeId::from(trade),
                side,
                OrderType::Market,
                Quantity::from("10"),
                Price::from(px),
                Currency::KRW(),
                LiquiditySide::Taker,
                UUID4::new(),
                UnixNanos::from(ts),
                UnixNanos::from(ts),
                false,
                Some(PositionId::from("P-1")),
                None, // no engine commission — matches the fee-less backtest instruments
            )
        };
        let mut pos = Position::new(&equity, fill("O-1", "T-A", OrderSide::Buy, "50000", 1));
        pos.apply(&fill("O-2", "T-B", OrderSide::Sell, "51000", 2));
        assert!(pos.is_closed(), "round trip closes the position");

        let risk = EntryRisk { risk_per_share: 1000.0, qty: 10.0 };

        // Gross path (costs: None) — the pre-model record: engine P&L, zero commissions.
        let gross = trade_from_position(&pos, Some(risk), None);
        assert_eq!(gross.realized_pnl, 10_000.0, "10 sh x (51000 - 50000)");
        assert_eq!(gross.realized_r, Some(1.0), "10_000 / (1000 x 10)");
        assert!(gross.fills.iter().all(|f| f.commission == 0.0));

        // Cost-aware path: commission both sides, tax on the sell notional only.
        let model =
            TransactionCostModel { commission_rate_per_side: 0.00015, sell_tax_rate: 0.0020 };
        let net = trade_from_position(&pos, Some(risk), Some(&model));
        // Expectations mirror `fill_cost`'s association exactly — rate × (qty × price) —
        // so the comparison is bitwise, not epsilon-fuzzed.
        let buy_cost = 0.00015 * (10.0 * 50_000.0); // 75.0 — commission only
        let sell_cost = (0.00015 + 0.0020) * (10.0 * 51_000.0); // 1_096.5 — commission + tax
        assert_eq!(net.fills[0].commission, buy_cost, "buy fill: no statutory tax");
        assert_eq!(net.fills[1].commission, sell_cost, "sell fill: commission + tax");
        assert_eq!(net.realized_pnl, 10_000.0 - buy_cost - sell_cost);
        assert_eq!(
            net.realized_r,
            Some((10_000.0 - buy_cost - sell_cost) / 10_000.0),
            "realized_r nets costs (subtraction happens before joined_risk)"
        );
        // Everything cost-independent is untouched.
        assert_eq!(net.quantity, gross.quantity);
        assert_eq!(net.avg_px_open, gross.avg_px_open);
        assert_eq!(net.avg_px_close, gross.avg_px_close);
        assert_eq!(net.risk_capital, gross.risk_capital);
    }
}
