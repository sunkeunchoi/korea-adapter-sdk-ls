---
title: "Turn 9: profit-target sweep falsified exit geometry as the lever; the MFE distribution names an entry-side breakout-strength band-pass filter"
date: 2026-07-10
category: conventions
module: adapters/nautilus/lab strategy loop (lab/src/runner/report.rs, lab-research report mfe / turn)
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Planning the entry-side breakout-strength filter code turn — its empirical spec and acceptance criterion live here"
  - "Reading a `lab-research report mfe` output (percentiles, strength quartiles, leg-2 candidate, right-censoring)"
  - "Tempted to re-tune `profit_target_r` — turn 9 swept it in both directions off v9 and both legs made expectancy worse"
tags:
  - strategy-loop
  - param-turn
  - profit-target
  - mfe
  - right-censoring
  - breakout-strength
  - entry-filter
  - orb
  - next-lever
---

## Context

Turn 8 added a fixed 1.0R profit target (v9: expectancy −3,157 KRW/trade, WR
46.9%, PF 0.97 — an 81% improvement over v8 but still NON-PASS) and put
per-trade `mfe_r` on every exit envelope precisely so this turn could read
give-back from data. Turn 9 did two things:

1. **Shipped a durable MFE-distribution report** — `lab-research report mfe`
   (`lab/src/runner/report.rs`) summarizes a run's `decisions.jsonl`: `mfe_r`
   percentiles (nearest-rank), MFE by exit reason, MFE by breakout-strength
   quartile (exit joined to breakout on symbol + KST session date), and a leg-2
   target candidate with a right-censoring verdict. It reads artifacts only —
   the strategy code hash was byte-identical across v9/v10/v11
   (`d54955a8aacf35d9…`), no re-baseline.
2. **Swept `profit_target_r` off v9 in two governed legs** (the turn-9 plan's
   maximum), both param-mode compare **PASS** with the diff exactly
   `{profit_target_r, strategy_version}`.

All runs: offline, release binary, `data/turn4-fresh` (24 sessions,
20260526–20260703, 40-symbol universe), no live gateway.

## The sweep: both legs made expectancy worse

| Run | `profit_target_r` | Trades | WR | Expectancy (KRW/trade) | PF | Dominance | Edge |
|---|---|---|---|---|---|---|---|
| v9 (base) | 1.00 | 162 | 46.9% | −3,157 | 0.973 | 12.7% ✓ | **no** |
| v10 (leg 1) | 1.50 | 148 | 42.6% | −4,406 | 0.961 | 13.0% ✓ | **no** |
| v11 (leg 2) | 1.05 | 145 | 41.4% | **−35,969** | 0.715 | 13.0% ✓ | **no** |

- **Leg 1 (1.0 → 1.5, the turn-8 sim's optimum):** un-clipped the runners as
  predicted (avg winner 255,854 → 285,225) but the trades whose MFE lives in
  the 1.0–1.5R band stopped banking at 1.0R and mostly gave it back
  (target-exit share 28.5% → 17.6%, WR −4.3pp). Net worse.
- **Leg 2 (1.5 → 1.05, the v10 report's KTD6 candidate — p70 of positive MFE
  = 1.0476 → 1.05, in-band [0.75, 2.25], not censored):** decisively worse.
  The winner-MFE cluster peaks *just above* 1.0R (v9 percentiles: p70 0.95,
  p75 1.00, p90 1.05); nudging the target from 1.00 to 1.05 pushes it past
  that cluster, so the ~5% of trades with MFE ∈ [1.00, 1.05) flip from banked
  +1R wins into stop/time-flat losses. **The payoff cliff at the target
  boundary dominates the extra 0.05R banked on hits.**

Read together with turns 6–7 (max_concurrent, range_minutes — see
[reading param-turn outcomes](strategy-loop-reading-param-turn-outcomes-win-rate-vs-expectancy.md)):
v9's 1.0R sits at a local optimum of the exit-geometry knob, and no
`profit_target_r` value fixes a trade population whose entries are the
problem. **Exit geometry is falsified as the expectancy lever.**

## The MFE distribution (why 1.0R was already the sweet spot)

`report mfe` on v9 (172 mfe-bearing exits, 254 breakouts, 82 sizing-rejected
orphans, 0 degenerate ranges):

| | p25 | p50 | p70 | p75 | p90 |
|---|---|---|---|---|---|
| v9 `mfe_r` (n=172) | 0.19 | 0.47 | 0.95 | 1.00 | 1.05 |
| v10 `mfe_r` (n=159, censored at 1.5) | 0.19 | 0.46 | 0.93 | 1.15 | 1.52 |

By exit reason (v9): stop_hit n=39 median 0.15 · target n=49 median 1.03 ·
time_exit n=84 median 0.38. The time-flat population's median MFE of 0.38R
means the *typical* non-target trade never gets close to any target — no
target value monetizes it.

**Right-censoring caveat (bake this into every future read):** every
`mfe_r`-bearing run is truncated at its own `profit_target_r` — v9's p70/p75
pinned at ~1.0 partly *because* the 1.0R target exits there (target-exit share
28.5%). The v10 run (censored at 1.5 instead) shows the uncensored region:
p75 1.15, p90 1.52 — winners do run past 1.0, but not enough of them (leg 1's
verdict is the empirical proof). The report prints the source run's target and
target-exit share on every invocation so this truncation stays visible, and it
declares a candidate within one 0.05 step of the source target RIGHT-CENSORED
(v9's and v11's own reports both do).

## The deliverable: breakout-strength filter empirical spec (next lever)

Strength = `(breakout_price − range_high) / R`, `R = range_high − range_low`,
computed per trade by joining each exit to its breakout envelope. Entries are
identical across v9/v10/v11 (same 254 breakouts; only exits differ), so the
three runs are three independent exit-geometry views of the same entry
population. Win share and per-bucket approximate expectancy (entry/exit
**limit** prices × qty — directional, not reconciled to engine P&L which
includes actual fills; use it to rank buckets, not to project totals):

| Strength quartile | v9 win / exp | v10 win / exp | v11 win / exp |
|---|---|---|---|
| q1 [0.002, 0.038) | 48.8% / −1,000 | 38.5% / −16,866 | 47.6% / +3,695 |
| q2 [0.038, 0.067) | 34.9% / −80,627 | 37.5% / −52,832 | 35.7% / −79,202 |
| **q3 [0.067, 0.125)** | **53.5% / +9,711** | **47.5% / −2,653** | **54.8% / +27,032** |
| q4 [0.125, 0.471] | 39.5% / −67,909 | 40.0% / −42,224 | 37.2% / −70,229 |

The cut is **non-monotonic — a band-pass, not a threshold**:

- **q3 (strength ≈ 0.067–0.125) is the best bucket under all three exit
  geometries** — highest win share (47.5–54.8%), highest median MFE
  (0.57–0.67 vs 0.38–0.47 elsewhere), and the only bucket with positive
  approximate expectancy in two of three runs.
- **q2 and q4 carry essentially all the losses** in every run. Marginal
  breakouts (q2) fail obviously; but the *strongest* breakouts (q4) also
  underperform — consistent with overextended pops that exhaust into the entry.
- q1 is roughly flat.

**Recommended spec for the entry-filter code turn:** accept a breakout only
when strength ∈ [0.06, 0.13] (widen/tune edges on that turn's own run), i.e. a
`breakout_strength_min` + `breakout_strength_max` pair on `OrbParams`, applied
at the Armed→entry transition in `orb.rs`. Acceptance criterion: edge verdict
(expectancy > 0, dominance ≤ 0.40) on the filtered run, judged against the
sample-size cost — the band keeps only ~1/4 of current entries (~43 trades /
24 sessions), so pair it with the widened universe or a longer range if the
trade count gets thin. This is a strategy-logic turn: it moves the code hash
and re-baselines like turn 8.

## Turn-9 terminal state (R9)

**insufficient-evidence.** Both legs' expectancy ≤ 0; per the plan's
stop-after-two-legs bound no further target values run; v9's 1.0R stands
(v11 is the latest *finalized* run, so note: **the next turn's seed assertion
must pin `LS_TURN_EXPECT_VERSION=11` with `profit_target_r 1.05` resolved**, or
explicitly re-seed off the v9 manifest and document the reversion). The named
next lever is the **entry-side breakout-strength band-pass filter** specified
above. No operator override was used; the leg-2 candidate came from the KTD6
default rule.

Runs and verdicts (gitignored data home `data/turn4-fresh`):
`20260710T013757Z-backtest-orb-v9` → `20260710T024131Z-backtest-orb-v10`
(compare PASS) → `20260710T024248Z-backtest-orb-v11` (compare PASS); both new
runs carry an authored `analysis.md` with the insufficient-evidence verdict.
