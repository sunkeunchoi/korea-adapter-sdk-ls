---
title: "A bucket-derived entry cut must be confirmed by a reconciled run before it is trusted — turn 10 falsified the q3 breakout-strength band"
date: 2026-07-10
category: conventions
module: adapters/nautilus/lab strategy loop (lab/src/runner/report.rs report mfe; lab/src/strategy/orb.rs; lab/src/artifacts/performance.rs edge eval; operator data home data/turn4-fresh)
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "A `report mfe` (or any) per-bucket table ranks a sub-population as the profitable one and you are tempted to ship that cut as a strategy lever"
  - "Designing an entry-side filter (breakout-strength band, gap band, any keep-the-good-bucket cut) off an approximate per-bucket read"
  - "Reading a filtered-run trade count and expecting it to equal the naive kept-fraction of the unfiltered breakouts"
  - "Seeding a code-turn re-baseline whose new param takes a NON-default value (extends the default-value seed recipe)"
tags:
  - strategy-loop
  - reconciled-vs-approximate
  - entry-filter
  - breakout-strength
  - band-pass
  - falsification
  - max-concurrent
  - seed-and-rerun
---

## Context

Turn 9's MFE-distribution report split v9's entry population into breakout-strength
quartiles and computed an **approximate** per-bucket expectancy (entry/exit limit
prices × qty — a directional read, not reconciled engine P&L). The q3 bucket
`[0.067, 0.125)` was the only positive bucket and best under all three exit
geometries, so it read as strong evidence for an entry-side band-pass cut (see
[[strategy-loop-turn-9-profit-target-sweep-and-mfe-distribution]]).

Turn 10 implemented the band-pass at `[0.06, 0.13]` and ran it as a governed v12
code turn on the same 24-session × 40-symbol sample. **The reconciled run
disagreed decisively with the approximate ranking.**

## Guidance

**Treat an approximate per-bucket ranking as a hypothesis, never a verdict.
Confirm any bucket-derived entry cut with a reconciled run before you trust it or
ship it as a lever.** The reconciled run is the real test; the bucket read only
tells you where to look.

Two mechanisms make the reconciled result diverge from the bucket read, and both
must be reasoned about up front:

1. **The filtered population is not the bucket subset.** A rejected entry calls
   `force_done()`, which frees its `max_concurrent` slot — so the filtered run
   admits in-band breakouts the unfiltered run's concurrency gate had refused. The
   trade count therefore runs **above** the naive kept-fraction. Turn 10's band
   kept ~34% of breakouts (87/254 in-band) but trades only **halved** (162 → 82),
   not quartered, because freed slots pulled in previously-refused in-band breaks.
   Those extra trades are not in any bucket the diagnostic measured.

2. **The structure recurs one level down.** The reconciled `report mfe` on the
   filtered run showed the same non-monotonicity *inside* `[0.06, 0.13]`: the
   middle sub-band `[0.083, 0.107]` won 57.1% while both edges lost (q1
   `[0.062, 0.067]` 35.0%, q4 `[0.107, 0.128]` 28.6%). A "profitable region," if
   one exists, is narrower and more centred than the diagnostic's band — and would
   rest on ~21-trade sub-buckets, an overfit risk.

## Why This Matters

The band that looked like the one positive bucket in the approximate read came
back **decisively negative** when reconciled: expectancy **−20,735 KRW/trade** vs
v9's baseline **−3,157** (~6.6× worse), win rate flat (45.1% vs 46.9%), profit
factor 0.835 vs 0.973, on an **adequate** sample (82 trades, ~2× the ~43-trade
floor) with dominance comfortably passing (8.9% ≤ 40%). Shipping the cut on the
strength of the bucket ranking alone would have advanced a lever that makes the
strategy worse.

Because the sample was adequate and dominance passed, this is **not** an
insufficient-evidence / data-expansion outcome (that escape is reserved for a
positive-but-thin or dominance-miss-from-low-count result) — it is a genuine
**falsification** of the band as an expectancy lever, recorded as a `revert`
verdict on the v12 run's `analysis.md`.

## When to Apply

- Any time a per-bucket table (strength, gap, time-of-day, any partition) nominates
  a profitable sub-population and you are about to cut on it. Build the filter, then
  judge it on the **reconciled** run's expectancy/dominance, not on the bucket
  numbers.
- When estimating a filtered run's trade count: account for freed `max_concurrent`
  slots — the count will exceed the kept-fraction of unfiltered breakouts.
- Do **not** respond to this kind of falsification by re-tuning the band edges to
  the winning sub-band — that is band-edge tuning (a deferred governed param turn)
  and it overfits 21-trade buckets. When win rate is flat and expectancy stays
  pinned negative across the exit-geometry turns (8–9) and this entry-strength cut,
  the next lever is a **different strategy-logic turn** (e.g. stop placement / risk
  re-scaling — turn 10's avg loser −248,831 ≳ avg winner +238,185, so the loss tail
  binds), not band re-tuning or live data expansion.

## Examples

**Seed-and-rerun with a NON-default param value.** The v12 code turn was produced
by the seed-and-rerun recipe
([[code-turn-rebaseline-run-via-manifest-seed-and-rerun]]), which documents the
case where the new param holds its **default**. Turn 10 extended it: the band
values are non-default, so they must be **carried in the seed manifest** (the
pass-through defaults `0.0` / `f64::MAX` would run the filter disabled). Copy the
v9 manifest, set `strategy_version: 12`, `breakout_strength_min: 0.06`,
`breakout_strength_max: 0.13`, keep `profit_target_r: 1.0`, place it as a
manifest-only run dir timestamped after the latest finalized run, then
`lab-research turn` in rerun mode reads it as the current authority. The
`runs compare` v9→v12 param-mode FAIL then shows a **3-key** diff
`{breakout_strength_min, breakout_strength_max, strategy_version}` plus
`strategy_code_hash differs` — that FAIL is the re-baseline evidence.

**Approximate vs reconciled, side by side (turn 10):**

| | approximate q3 read (turn 9) | reconciled band run (turn 10, v12) |
|---|---|---|
| verdict on the cut | only positive bucket | expectancy −20,735, worse than baseline |
| trade count | ~1/4 of breakouts implied | 82 (freed slots → ~1/2, not 1/4) |
| within-band structure | (not visible) | recurs: middle wins, both edges lose |
