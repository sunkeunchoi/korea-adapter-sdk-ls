# nautilus-ls-lab — the strategy-improvement loop

The lab is a **separate crate** from the certified `nautilus-ls` adapter (KTD1). The
adapter ships no strategy — its contract is translation only. All strategy code, the
backtest/live runners, and the artifact writer live here, so strategy churn never
destabilizes the adapter.

The lab exists to turn one loop: **backtest → an agent analyzes the artifacts →
change the strategy → re-backtest → compare**. This README is the recipe an agent
follows to turn it, without reading source.

## What a run produces (the append-only registry)

Runs live beside the catalog under one data home:

```
<data>/catalog/                     # ParquetDataCatalog (ingested bars + instruments)
<data>/runs/<run_id>/               # one immutable directory per run (never overwritten)
  manifest.json                     # strategy id/version, full params, pinned range,
                                    #   range-scoped catalog fingerprint, universe hash
  performance.json                  # trade/fill ledger, per-trade P&L, equity curve, stats
  decisions.jsonl                   # one decision envelope per decision (universe + transitions)
  data_quality.json                 # coverage gaps, adjustment-basis flag, approximated-fill
                                    #   count, reconcile-advised conditions (live), universe
  analysis.md                       # YOU write this (see below) — it co-locates here
<data>/probes/minute-lookback.json  # the max-lookback probe result (adapter README)
```

`run_id = <UTC start stamp>-<source>-<strategy_id>-v<strategy_version>` (second
granularity — two runs started in the same second collide and the second is refused,
by the append-only guard). `source` is `backtest` or `live`. A run writes into
`<data>/runs/.tmp-<run_id>/` and finalizes by atomic rename; a leftover `.tmp-`
directory is an **aborted run** — reported on the next writer construction, never
reused. Artifacts are credential-free by construction (typed enums + counts; the one
free-text field is scrubbed at write time).

Two runs are comparable from their manifests alone (AE1): the manifest pins
`strategy_version` **and** a `strategy_code_hash` (so a logic change without a version
bump is still visible), the full parameter set, the pinned `data_range`, a
range-scoped `catalog_fingerprint`, and the `universe_hash`. A backtest run trades a
**single session** — the last trading day whose daily bar falls in the pinned range.

### Artifact key reference (read artifacts without reading source)

`decisions.jsonl` — each line is one decision envelope
`{ schema_version, envelope_id, ts_event, trigger, context, policy_decision,
capability, guardrail, lowering, decision_detail }`. In-run strategy telemetry
rides `decision_detail` as `{ kind, symbol, decision?, filter?, values }` (the
pipeline stages of a telemetry envelope are explicitly `NotEvaluated`, and its
`context` is the minimal `form: Telemetry` snapshot: strategy id/version, numeric
params, running counts). `decision_detail.values` keys per `kind`:

| `kind` | `decision` / `filter` | `values` keys |
|---|---|---|
| `universe` (accept) | `decision: accept` | `gap_pct`, `prior_turnover`, `rank` |
| `universe` (reject) | `decision: reject`, `filter: gap` \| `turnover_rank` | `gap_pct`,`prior_close`,`today_open` (gap) or `prior_turnover`,`rank` |
| `breakout` | — | `range_high`, `range_low`, `breakout_price` |
| `order_placed` | — | `qty`, `price` |
| `order_rejected_sizing` | `filter: emission_stopped` \| `notional_too_small` \| `max_concurrent` | `open_positions`, `qty` |
| `stop_hit` / `time_exit` | — | `qty`, `price` |
| `session_summary` | — | `session_high`, `session_low` |

`performance.json` `summary` — a flat `{key: number}` map. Lab-computed keys are
snake_case: `pnl_total`, `num_trades`, `max_drawdown` (all KRW / counts). The remaining
keys come from `nautilus-analysis`'s `PortfolioAnalyzer` and are Title-Case with spaces
(`"Win Rate"`, `"Expectancy"`, `"PnL (total)"`, winners/losers); returns-based stats
(Sharpe/Sortino) appear only when ≥2 daily account-balance snapshots exist, else they
are dropped (never emitted as NaN). All monetary values are KRW (single-currency,
domestic KRX).

## Turning the loop (backtest)

1. **Backfill data** (adapter README: probe → bounded minute backfill → accumulate).
2. **Run the baseline backtest** over a pinned range:

   ```
   LS_DATA_HOME=./data LS_BT_SDATE=20240102 LS_BT_EDATE=20240105 \
     cargo run --bin lab-backtest
   # → finalizes ./data/runs/<run_id>/ with the four artifacts.
   ```

3. **Analyze the run.** Read the four artifacts in the finalized run dir and write
   your findings to `analysis.md` **inside that same run directory**. Co-locating the
   analysis with the runs it analyzed (R15) is the whole point — the next turn builds
   on it. A committed example is `tests/fixtures/analysis.md`.
4. **Change the strategy.** For a parameter turn, bump `strategy_version` and change
   the parameter(s). ORB v0's parameters are all in `params.rs` (gap filter, opening
   range, entry/exit, sizing) and every value is recorded in the manifest.
5. **Re-run** over the **same pinned range** so the two runs are comparable.
6. **Compare via the manifests.** Two runs whose manifests differ only in
   `strategy_version` + the changed parameter are a clean loop turn: the parameter and
   data deltas are visible from the manifests alone, no re-run or source diff. Because
   the range is pinned, the range-scoped `catalog_fingerprint` is identical across
   accumulate days — a *changed* fingerprint means real in-range data drift, not the
   nightly accumulate growth.

The permanent proof of a turn is the test
`loop_turn_manifest_comparison_isolates_param_delta` in `tests/backtest_run.rs`, plus
the committed fixture analysis.

## Live paper session (operator-gated)

The `lab-live` bin runs the **same** ORB against the paper gateway and emits the same
artifacts into the same registry, marked `source = live`. It is **operator-gated** —
never run by the gate — because it needs live paper credentials and an open KRX window.

Safety (KTD7): the runner takes the live advisory lock (refusing while a backfill is
running — they cannot run concurrently), honors the paper-only interlock
(`LS_TRADING_ENV=paper`), and at exit/market-close runs a **fail-closed** teardown:
stop the strategy's order emission first, cancel all resting orders, run a
quantity-keyed t0425 flatness check (positive confirmation only — a truncated read is
not flat), and engage the exec client's kill switch only **after** the closing cancels
complete. Artifacts finalize on teardown; a crash leaves the `.tmp-` directory as the
aborted-run marker.

Live fills report their true execution price: the SDK models `cheprice` on the t0425
row, and the poll lane emits fills at `cheprice` (falling back to the order's limit
price with a `price_approximated` flag counted in `data_quality.json`). Beyond-first
partial fills on one order are also flagged approximate — a row carries one `cheprice`
per order — so the agent never reads an approximated price as exact.

```
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_DATA_HOME=./data \
  cargo run --bin lab-live
```

> **Staging status:** the shipped `lab-live` bin validates the paper-only interlock
> and then exits with a pointer to this recipe — the full LiveNode session (lock,
> mount, `node.run`, teardown, artifact finalize) is **not yet wired**, because it
> needs live credentials and an open KRX window that the offline gate never has. The
> safety-critical pieces exist and are unit-tested: the fail-closed teardown
> (`runner::live::run_teardown`), the `LiveSession` seam, the live advisory-lock guard
> (`runner::live::live_guard`), the emission gate, and the reconcile/approximated-fill
> collectors. An operator wiring the session composes those against a real `LiveNode`.

## Scope

ORB v0 is a **starter, not a deliverable** — its parameters are plan-defined defaults
the loop exists to revise, not tuned claims. Strategy quality is not the goal of this
increment; the loop's existence is.
