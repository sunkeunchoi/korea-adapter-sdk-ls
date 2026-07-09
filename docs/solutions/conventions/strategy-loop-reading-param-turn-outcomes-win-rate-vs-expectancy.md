---
title: "Reading a strategy-loop param-turn outcome: win rate moves but expectancy flat means the lever is exit geometry, not the entry param"
date: 2026-07-09
category: conventions
module: adapters/nautilus/lab strategy loop (lab/src/strategy/orb.rs, lab-research turn/analyze)
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "A governed single-variable param turn (`lab-research turn`) finished and you must author its edge-quality verdict and name the next lever"
  - "A knob demonstrably changes trade behaviour (trade count and/or win rate) but expectancy stays flat or negative"
  - "Deciding whether the next strategy-loop turn is another param turn or an escalation to a strategy-logic (`orb.rs`) turn"
tags:
  - strategy-loop
  - param-turn
  - edge-quality
  - expectancy
  - win-rate
  - orb
  - exit-geometry
  - next-lever
---

## Context

A strategy-loop param turn changes one governed knob, re-runs the multi-session
backtest, and authors a keep / revert / insufficient-evidence verdict on **edge
quality** (expectancy with single-symbol dominance capped — the trade-count /
breadth frequency bar was retired in Turn 5). The mechanics of running a turn are
covered elsewhere ([param-turn governance and fresh-home
seeding](strategy-loop-param-turn-governance-and-fresh-home-seeding.md); the
[multi-session runner](../architecture-patterns/multi-session-nautilus-backtest-fresh-engine-and-catalog-bucketing.md)).
This note is about the step *after* a turn runs: **reading the outcome to pick the
next lever**, so the loop doesn't waste turns twisting a knob that cannot move the
number that matters.

Turns 6 and 7 (2026-07-09, offline over `data/turn4-fresh`, off the v6 baseline of
118 trades / expectancy −7,946 KRW/trade / win rate 39.8%) each falsified a
different knob and, together, isolated where the edge actually lives.

## Guidance

**Classify a param-turn outcome by *which* metric moved, not by whether
expectancy improved.** Two knobs can both leave expectancy negative for opposite
reasons, and the reason dictates the next lever:

1. **The knob doesn't even move win rate → it is not an edge lever at all; stop
   turning it.** Turn 6 loosened `max_concurrent` 5→7 (motivated by v6's
   `decisions.jsonl` showing 149 of 270 breakouts rejected at
   `order_rejected_sizing/max_concurrent`). It admitted the queued breakouts
   (118→153 trades) but they were net-losers: expectancy −7,946→−16,330 (≈2×
   worse), win rate essentially flat (39.8→38.6%). Win rate is *invariant* to
   concurrency because concurrency changes *how many* breakouts trade, not
   *whether a given breakout wins*. A knob whose win rate doesn't respond is
   falsified as a quality lever — don't keep probing it.

2. **The knob moves win rate but not expectancy → the entry side is tunable, but
   the residual is payoff geometry; escalate to a strategy-logic turn.** Turn 7
   widened `range_minutes` 15→20 (a wider opening range demands a stronger move to
   trigger, so entries should be higher-conviction). It worked *on win rate*:
   38.6→42.9% (+4.3 pp) at fewer trades (153→140) — proving `range_minutes` is a
   live, effective entry-quality knob. But expectancy stayed pinned negative
   (−16,330→−16,589, flat). **A rising win rate that doesn't lift expectancy is
   the signature of asymmetric payoff geometry (average loss ≫ average win).** No
   entry-timing knob repairs that shape.

**When no param knob lifts expectancy above zero, the finding escalates out of
param-turn governance to a strategy-logic (`orb.rs`) turn** — which edits the
strategy itself, bumps `strategy_code_hash`, and re-baselines the loop like Turn 5
(a code change, not a keep/revert param verdict). For ORB the escalation target is
**exit geometry**: the state machine has exactly two exits — `ExitReason::Stop` at
the opening-range low and `ExitReason::TimeFlat` at `flat_time` (`orb.rs:196-201`,
`orb.rs:307-331`) — **no profit target and no trailing stop**. Losers travel the
full stop distance; winners are cut at the 15:00 bell regardless of how far the
move was still running. The next lever is a profit-target / trailing-stop (enforce
a favorable reward:risk, let winners run past `flat_time`) and/or a
breakout-strength filter (require the breakout bar to clear the range by a minimum
margin — compounds the win-rate gain `range_minutes` already proved is reachable).

**Every finding is a valid recorded outcome (R5), not a turn failure.** A knob
falsified as a lever is signal: it removes a hypothesis and narrows the search.
Record it in the run's `analysis.md` verdict with the metric evidence and the
named next lever.

## Why This Matters

- **It prevents burning turns on the wrong knob.** Without the win-rate-vs-
  expectancy read, the obvious response to Turn 6 (loosening concurrency made
  things worse) is "tighten concurrency instead" — but that only trims the loss
  tail; it never manufactures positive expectancy, because concurrency was never a
  quality lever. The diagnostic says so in one turn.
- **It distinguishes a tunable-but-insufficient knob from a dead one.**
  `range_minutes` *is* worth keeping in the toolbox (it moves win rate); it just
  can't finish the job alone. Discarding it as "also failed" would lose a real
  lever. The rule separates "no effect" (drop it) from "right axis, wrong stage"
  (keep it, add the missing stage).
- **It routes correctly between governance regimes.** Param turns are single-
  variable and governed by the 0.5 proposal-bounds cap and the `runs compare`
  param-mode verdict. A strategy-logic turn is a different regime (code hash
  changes, re-baseline). Misreading an exit-geometry problem as an entry-param
  problem keeps the loop in the wrong regime indefinitely.

## When to Apply

At the verdict-authoring step of any strategy-loop param turn, and whenever a
sequence of param turns has left expectancy negative. Use the metric that moved to
decide the next lever: win rate unmoved → drop the knob; win rate up but
expectancy flat → escalate to a strategy-logic turn targeting payoff/exit
geometry.

## Examples

The Turn 6 / Turn 7 sweep, off the v6 baseline (`max_concurrent=5`,
`range_minutes=15`; expectancy −7,946, win rate 39.8%). Every run is offline, no
code changed (`strategy_code_hash` is identical across v6/v7/v8), and both
`runs compare` param-mode verdicts PASS (single-variable governance held):

| Turn | Knob | Trades | Win rate | Expectancy (KRW/trade) | Read → next lever |
|---|---|---|---|---|---|
| — | v6 baseline | 118 | 39.8% | −7,946 | — |
| 6 | `max_concurrent` 5→7 | 153 | 38.6% | −16,330 | win rate unmoved → **not an edge lever; drop it** |
| 7 | `range_minutes` 15→20 | 140 | **42.9%** | −16,589 | win rate up, expectancy flat → **escalate to a strategy-logic (`orb.rs`) exit-geometry turn** |

**Anti-pattern (what the rule prevents):** reading Turn 6's worse expectancy as
"concurrency is too loose, tighten it next turn." That treats a non-lever as a
lever and spends the loop's evidence budget on a knob that cannot move win rate.

**Pattern:** read Turn 7's +4.3 pp win rate with flat expectancy as proof the
entry axis is tunable but the loss is in the exit — and make the next turn an
`orb.rs` change (profit target / trailing stop / breakout-strength filter) rather
than a fourth entry-param probe.

Verdicts authored in-run at
`data/turn4-fresh/runs/20260709T124557Z-backtest-orb-v7/analysis.md` (Turn 6) and
`.../20260709T124752Z-backtest-orb-v8/analysis.md` (Turn 7); both runs are
gitignored under `/data/`.
