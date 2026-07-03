# Loop turn 1 — ORB v0 baseline analysis

_This is the committed fixture demonstrating the R15 co-location convention: an
agent reads a finalized run's four artifacts and writes `analysis.md` **into that
run's directory**, so the next loop turn builds on this analysis instead of
restarting. It analyzes the baseline backtest fixture run._

## Run under analysis

- **Source:** backtest
- **Strategy:** `orb` v0 (starter defaults, KTD6 — not tuned)
- **Data range:** 20240102 – 20240105 (pinned)
- Read from `manifest.json`, `performance.json`, `decisions.jsonl`,
  `data_quality.json` in this same directory.

## What the artifacts show

- **Performance (`performance.json`):** one completed long trade on `005930.XKRX`
  (`num_trades = 1`, `pnl_total > 0`). The single opening-range breakout entered at
  the breakout bar's marketable high and time-flatted at 15:00 KST for a small gain.
  With one trade the win-rate/expectancy statistics are not yet meaningful — the loop
  needs more trades before the summary stats carry signal.
- **Decisions (`decisions.jsonl`):** the universe scan accepted `005930` on a +5% gap and
  emitted `breakout → order_placed → time_exit`, then an end-of-session summary. No
  candidate was rejected in this fixture, so the gap filter's selectivity is untested
  here.
- **Data quality (`data_quality.json`):** `adjustment_basis_splice = true` — the
  daily catalog is on an adjusted-price basis, a documented limitation to discount if
  a splice falls in-range. `price_approximated_fills = 0` (a backtest emits exact
  fills). No reconcile-advised conditions (backtest).

## Proposed change for turn 2

The strategy only ever finds one candidate because the fixture universe is a single
symbol. Before touching entry/exit logic, **widen the universe filter** so the loop
generates more trades to analyze:

- Raise `gap_min_pct` selectivity experiments aside; first **lower the gap floor**
  (e.g. `gap_min_pct: 3.0 → 2.0`) as the turn-2 change so more "stocks in play"
  qualify once a real multi-symbol catalog is backfilled. This is a pure parameter
  change (bump `strategy_version`), so the two runs stay manifest-comparable: their
  manifests differ only in `strategy_version` and `gap_min_pct`, and nothing else.

That comparability — param-delta visible from the manifests alone, no re-run or source
diff — is the property the permanent loop-turn test asserts (AE1).
