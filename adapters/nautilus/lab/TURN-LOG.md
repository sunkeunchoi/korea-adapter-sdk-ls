# Strategy-loop turn log

Committed record of each loop turn's verdict + the bar conditions it held. The
full artifacts (`analysis.md`, manifests, performance) live in the gitignored data
home; this file is the durable, reviewable outcome trail.

## Turn — mechanism-harness all-off baseline (2026-07-12) — plan 2026-07-11-001 (U6)

- **Verdict: re-baseline PASS (reconciled to v9).** The harness code turn adds
  queue levers 1–5 to `orb.rs` as default-off gates (`stop_mode`, `entry_confirm`,
  `or_width_max_atr`, `entry_cutoff_min`, `rvol_min` + companions), plus the U2
  candidate seam (prior-daily ATR, opening-window RVOL) and the U5 stop-mode report label.
- **Type:** code turn — `strategy_code_hash e3812d4f…` (v9 was `d54955a8…`); re-baselined
  via seed-and-rerun (KTD2), all gate params at filter-off defaults, `strategy_version = 13`.
- **R3 / AE1 reconcile:** per-trade ledger vs `…-v9` is **166/166 trades byte-identical**;
  summary equal on every field (`num_trades 162`, `Expectancy −3157.547`, `WR 0.4691`,
  `PF 0.9729`, `pnl_total −502050`, `max_drawdown 9149100`). No gate fires at defaults.
- **Re-baseline evidence:** `runs compare` param mode (pinned v9 → v13) FAILs as expected —
  `param diff ["strategy_version"]`, `FAIL: strategy_code_hash differs`. No compare mode
  passes a code turn (KTD3); the FAIL *is* the evidence.
- **Provenance:** run `20260712T012320Z-backtest-orb-v13` (home `data/turn4-fresh`, gitignored).
  Zero `orb.rs` edits after this run (KTD8).

## Turn — first flip: OR-midpoint stop (2026-07-12) — plan 2026-07-11-001 (U7)

- **Verdict: REVERT (lever 1 leg 1 falsified).** `stop_mode` 0.0 → 1.0 (OR-midpoint) as a
  single-param seed-and-rerun flip from the v13 baseline.
- **AE2 attribution:** `runs compare` param mode (v13 → v14) **PASS**, diff exactly
  `{stop_mode, strategy_version}` — clean single-lever attribution.
- **Edge gate (`EdgeEvaluation`, unchanged): NOT cleared.** Expectancy −28,983 KRW/trade
  (baseline −3,157), WR 37.7% (−9.2 pp), PF 0.72 (from 0.97), pnl −5,159,050. Dominance
  8.3% (≤ 40% passes, but expectancy is deep-negative). Trades 162 → **183** — the tighter
  stop fires earlier and frees `max_concurrent` slots, admitting more losing entries (turn-10
  caveat), not more winners.
- **Read:** the midpoint stop is the *noise* branch of the brainstorm frame — these breakouts
  pull back through the OR midpoint before running, so a midpoint stop converts winners/time-exits
  into losses. R-metrics labeled trade-R (AE3), not compared against v9/v13 range-R.
- **Queue re-rank (R6):** the tighter-stop hypothesis is falsified, demoting the sibling
  ATR-scaled stop (leg 2, also narrows the stop). New head = **lever 2 (close-confirmed entry)**,
  an orthogonal entry-quality mechanism the midpoint result implicates. Baseline stays v13 (== v9).
- **Provenance:** run `20260712T012616Z-backtest-orb-v14` (gitignored). Falsified run retained.

## Turn 3 — broaden-sample data turn (2026-07-07) — plan 2026-07-07-003

- **Verdict: insufficient-evidence.** The pre-registered R1 decisiveness bar was not cleared.
- **Type:** pure data turn — v3 params held exactly (`gap_min_pct = 0.6`, `strategy_version = 3`); zero param diff.
- **Sample:** 20 KOSPI top-market-cap names (frozen `t1444` capture, upcode 001, `lab/config/turn3-universe.json`) over 28 sessions `2026-05-26..2026-07-03`, fresh data home, daily + 1-minute bars (all 20 symbols, `catalog status` GO, no front-truncation).
- **Result:** 6 realized trades across 6 distinct symbols (1 each); `pnl_total` +320,000 KRW (Profit Factor 1.85).
- **R1 bar (computed, not eyeballed):**
  - (a) trade-count floor (≥ 30): **6 → FAIL**
  - (b) symbol-breadth floor (≥ 6 symbols each ≥ 2 trades): **0 → FAIL**
  - (c) single-symbol dominance (≤ 40% of aggregate |P&L|): **33.7% → PASS**
- **Reproducibility:** data-mode `runs compare` PASS ("no data deltas") vs an identical v3-wide rerun — determinism confirmed; run manifest carries `gap_min_pct = 0.6` / `strategy_version = 3` (v3 identity, KTD-3/KTD-5).
- **Bar integrity (R3):** the bar was fixed in the plan before the run and was not adjusted to the result.
- **Next (deferred):** the param turn lowering `gap_min_pct` from 0.6 toward ~0.3 (governed relative-change step within the 0.5 bounds cap) to admit more sessions, and/or a deeper/wider sample. Do not tune against this 6-trade sample.
- **Provenance:** run `20260707T075947Z-backtest-orb-v3` (fresh home `data/turn3`, gitignored).

### Context — turn 2 (prior)

v3 (`gap_min_pct = 0.6`) was the first floor to admit a fill: 1 trade on `005930` over a 12-session pinned range → verdict insufficient-evidence at n=1. Turn 3 broadened the sample to move the verdict off "insufficient by construction"; the broadened read still misses the trade-count and breadth floors, so the class holds — but now as a measured result, not an n=1 artifact.
