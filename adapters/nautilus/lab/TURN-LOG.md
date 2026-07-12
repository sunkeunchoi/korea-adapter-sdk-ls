# Strategy-loop turn log

Committed record of each loop turn's verdict + the bar conditions it held. The
full artifacts (`analysis.md`, manifests, performance) live in the gitignored data
home; this file is the durable, reviewable outcome trail.

## Turn — lever 2: close-confirmed entry (2026-07-12) — plan 2026-07-11-001

- **Verdict: KEEP (lever 2 clears the edge gate — the loop's first positive edge).**
  `entry_confirm` 0.0 → 1.0 (wick-touch → close-confirmed) as a single-param
  seed-and-rerun flip from the v15 all-off baseline, riding the F1/F2 flip-precondition
  fixes landed this same code turn.
- **Code turn — two flip-preconditions landed** (`docs/solutions/logic-errors/orb-atr-and-close-confirm-flip-preconditions.md`),
  re-baselining `strategy_code_hash` to `fa7733f6df76ca39…` (was `e3812d4f…`):
  - **F1 (mechanical):** non-positive prior ATR (`Some(0.0)` from flat/halted priors)
    treated as unavailable in **both** the ATR-stop and OR-width arms of
    `session_gate_reject`; ATR stop distance floored at 1 (`.max(1)`) so a tiny
    `mult·ATR` can't round to 0 and collapse the stop onto the entry.
  - **F2 (decision, approved):** in close-confirm mode the fill is close-anchored, so
    the entry bar's stop-touching low is provably **pre-fill** — the same-bar stop check
    is skipped there (wick mode unchanged). A deliberate deviation from KTD6's
    wick-entry "same-bar stop-first wins"; without it the flip books phantom same-bar
    stops on confirm bars and the verdict biases toward revert.
- **Re-baseline (v15, all-off) — verdict-neutral:** seed-and-rerun from v13 (KTD2),
  all gates off, `strategy_version = 15`. Per-trade ledger vs `…-v9` is **166/166
  byte-identical**; summary EQUAL on every field (`num_trades 162`,
  `Expectancy −3157.547`, `WR 0.4691`, `PF 0.9729`, `pnl_total −502050`,
  `max_drawdown 9149100`). The F1/F2 fixes touch only the ATR/close-confirm paths, so
  the all-off baseline is unchanged. Re-baseline signal captured: `runs compare`
  param mode (v9 → v15) FAILs `strategy_code_hash differs`, `param diff
  ["strategy_version"]` (KTD3 — the FAIL *is* the evidence).
- **AE2 attribution:** `runs compare` param mode (v15 → v16) **PASS**, diff exactly
  `{entry_confirm, strategy_version}`. v15 and v16 share `strategy_code_hash
  fa7733f6…` — a pure param flip (F1/F2 in both, verdict-neutral for all-off).
- **Edge gate (`EdgeEvaluation`, unchanged): CLEARED (`is_edge = true`).** Expectancy
  **+4,812.74** KRW/trade (baseline −3,157.55), WR 49.4% (+2.4 pp), PF **1.044**
  (from 0.973), pnl_total **+755,600** (from −502,050), Sharpe +0.45 / Sortino +0.75.
  Dominance **11.5%** (≤ 40%), top-|P&L| symbol is a *loser* (`034730.XKRX`
  −1,865,000) — the edge is not one-winner-carried. Trades 162 → 158: close-confirm
  trims only 4 wick-only breakouts yet flips aggregate P&L positive — those entries
  were net-losing fakes. R-metrics range-R (stop_mode 0, per `report mfe`).
- **Read:** the mirror of the U7 midpoint-stop falsification — these breakouts pull
  back through the OR mid before running, so tightening the *stop* converts winners to
  losses, but tightening the *entry* (demanding a confirmed close) avoids the fakes
  without surrendering the runners. Entry quality, not stop geometry, is the lever.
- **Queue re-rank (R6):** baseline advances to **v16** (`entry_confirm = 1.0`; future
  turns seed from it). The kept entry-quality lever validates the entry-quality class
  over stop-geometry. New head = **lever 4 (entry cutoff)** — the `report mfe`
  `time_exit` bucket (n=88, median give-back 0.40R) is the give-back-to-flat cohort a
  cutoff targets; then lever 3 (OR-width, now ATR-hardened by F1), lever 5 (RVOL).
  Lever 1 leg 2 (ATR-scaled stop) stays **demoted** (a stop-narrowing sibling of the
  falsified midpoint), though F1 now makes it runnable.
- **Provenance:** baseline run `20260712T022149Z-backtest-orb-v15`, flip run
  `20260712T022255Z-backtest-orb-v16` (home `data/turn4-fresh`, gitignored). Zero
  `orb.rs` edits after the v15 baseline run (KTD8). Offline, no gateway.

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
- **Flip preconditions (post-review):** code review found two latent modeling bugs in the
  default-off ATR-stop and close-confirm paths (unreachable by v13/v14) that must be fixed —
  riding their flip's re-baseline — before those levers run, or the flip verdict is biased.
  See `docs/solutions/logic-errors/orb-atr-and-close-confirm-flip-preconditions.md`.
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
