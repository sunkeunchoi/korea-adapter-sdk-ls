---
title: "Screening an EXIT-GEOMETRY lever — drop the collinearity gate, gate on direction (signed ror_delta) + materiality"
date: 2026-07-24
category: conventions
module: adapters/nautilus lab strategy loop (lab/src/strategy/orb.rs, lab/src/params.rs), exit-geometry levers
problem_type: convention
component: strategy-loop
severity: medium
applies_when:
  - "Proposing an EXIT-GEOMETRY re-parametrization in the ORB loop — a change to exit timing (profit target, breakeven trigger, time-exit, stop mode), not a re-weighting/tilt of trade size (e.g. profit_target_r 1.00->0.75, plan 2026-07-24-001)"
  - "The lever acts on the head's EXISTING trades (an incumbent per-trade signal exists — unlike an additive stream) but reallocates when you BOOK, not the risk budget"
  - "Adapting a sizing lever's collinearity screen to an exit-geometry lever and finding there is no risk axis to correlate against"
  - "Deciding whether a report-mfe RUNNABLE band reading is evidence to flip, or just distribution membership"
tags:
  - strategy-loop
  - exit-geometry
  - profit-target
  - collinearity
  - materiality
  - phase-a-screen
  - mfe-counterfactual
  - pre-registration
---

## Context

The ORB strategy-loop's pre-code Phase-A screens split by **what the lever reallocates**, and
each lever class gets a gate matched to that:

- **Re-weighting / sizing levers** (ratio-ATR tilt, amihud liquidity tilt, stop-width geometry)
  gate on a **collinearity gate** — a candidate weight must be decorrelated (`|Pearson r| < 0.70`)
  from the risk axis the kept levers already own, or it is a re-expression, not a new axis
  ([[pre-code-collinearity-gate-before-a-second-normalizer-lever]]).
- **Additive entry streams** (a new set of trades layered on the head) **drop** collinearity —
  the new trades do not exist in the head run, so there is no incumbent per-trade signal to
  correlate against — and gate on a count floor plus an additive `ror_shift`
  ([[additive-entry-stream-screen-drops-collinearity-gates]]).

An **exit-geometry lever** is a third class. Lowering `profit_target_r` `1.00 -> 0.75` (plan
`2026-07-24-001`) is a re-parametrization of **exit timing** on the head's own existing trades.
Unlike an additive stream, there *is* an incumbent per-trade signal (each trade's `realized_r` /
`mfe_r`). But the lever reallocates **when you book**, not the risk budget — so
collinearity-vs-`risk_per_share` is meaningless: there is no budget axis for exit timing to be
redundant with. The sizing screen does not transfer.

## Guidance

**Screen an exit-geometry lever on a direction + materiality dual gate; drop collinearity
entirely.** The screen (`adapters/nautilus/lab/candidates/profit-target-075/`, `diagnostic.py` +
independent `twin.py`) re-books the head's existing trades under the new geometry from telemetry
and gates on:

1. **`ror_delta >= FLOOR`** (the load-bearing STOP) where `ror_delta = RoR_prime - RoR_base` is
   **signed**: `RoR_base` is the size-invariant return-on-risk at the head param, `RoR_prime` the
   counterfactual RoR at the new param, `RoR = Σ(risk_capital·r) / Σ risk_capital` (size-invariant;
   `risk_capital` is the entry-fixed risk, not `performance.json`'s risk-normalized `realized_r` —
   [[performance-json-realized-r-is-risk-normalized-not-internal-r-denom]]). The gate STOPs unless
   the counterfactual RoR *strictly improves*. **This is where the exit-geometry gate spends the
   power the sizing gate spends on collinearity**: exit timing is orthogonal to sizing by
   construction, so axis-novelty is a given — the gate instead tests **direction**, the exact
   question the sizing gate deliberately ignores (it only asks "is this a new axis?", never "does
   it help?").

2. **`exit_change_frac >= FLOOR`** (materiality) — the fraction of trades whose booked outcome
   changes under the new param. Guards against a `ror_delta` dominated by a handful of trades.

**Compute the counterfactual from telemetry, never a bar replay.** Read per-exit `mfe_r` from the
run's `decisions.jsonl` and join it to `performance.json` closed trades on `(symbol, KST session
date)` exactly as `report_mfe` joins (`lab/src/runner/report.rs`), then re-book each trade under
the new geometry (for a profit-target lower: `r_new = 0.75 if mfe_r >= 0.75 else realized_r`). No
minute-bar replay, so the diagnostic is plain JSON.

**The counterfactual is one-sided (conservative) — which is what makes a STOP trustworthy.** For a
profit-target *lower*, the engine's marketable-limit fill books **at or above** the new target on a
gap-through, so the real flip RoR is `>=` the counterfactual: the gate can only **under-state** the
edge. Unlike the amihud *sizing* counterfactual, which *over*-predicted because the notional
ceiling clipped the upside ([[first-order-materiality-prediction-ignores-notional-ceiling]]), exit
geometry has no reversing term. So a STOP is a **robust** NO-BUILD; a GO is only necessary, not
sufficient — the actual flip backtest still decides (the same lower-bound / robust-NO-BUILD shape
as [[stop-geometry-lever-is-class-b-absorbed-and-near-inert]] and the additive-stream screen).

**No Rust change is warranted for a new reading family.** The candidate framework is generic over
reading names (`lab/src/candidates.rs` `Threshold { reading, comparator, value }` + `Comparator::passes`),
so the new `ror_delta` / `exit_change_frac` keys are declared entirely in `candidate.json` — the
diagnose stage reads them by name.

## Why This Matters

**The precedent this gate exists to catch:** `report mfe`'s own leg-2 candidate reading
(`p70(mfe_r>0) = 0.73 -> 0.75`) is a **distribution statistic** — where the positive-MFE mass
sits — **not** evidence that lowering the target improves RoR. Treating that "RUNNABLE" band
membership as a build signal is exactly the trap that
[[strategy-loop-turn-9-profit-target-sweep-and-mfe-distribution]] falsified on old data (the
turn-9 profit-target sweep). This convention is the **governed re-confirmation** of that finding:
turn 9 built-and-measured a sweep; this gate STOPs the same lever cheaply at Phase-A, before any
`orb.rs`/`params.rs` edit, and the direction gate is the mechanism that refuses the band reading.

`profit-target-075` STOPped exactly there. Over the real-data head cohort (v34,
`20260724T014752Z-backtest-orb-v34`, catalog fingerprint `363f199d`, 119 closed trades):

| reading | value | gate | result |
|---|---|---|---|
| `ror_base` (target 1.00) | 0.039806 | — | — |
| `ror_prime` (target 0.75) | 0.020336 | — | — |
| **`ror_delta`** | **-0.019471** | `>= 0.00065` | **STOP** (typed diagnose exit 11) |
| `exit_change_frac` | 0.2773 (33/119) | `>= 0.05` | PASS |

Materiality passes — 28 % of trades re-book — but the **direction** gate STOPs: lowering the
target **caps the winners more than it rescues the give-back cohort** (the ~10 % of trades that
run past the new target book lower instead of their higher realized R), so RoR *falls*. Because the
counterfactual under-states, the real flip is even worse — a robust NO-BUILD.

**The family pattern:** *collinearity is the SIZING gate; every non-sizing lever class needs a
bespoke Phase-A gate matched to what it reallocates.* Two different reasons collinearity is
dropped, one per non-sizing class:

- **Additive stream** — no incumbent per-trade signal exists to correlate against
  ([[additive-entry-stream-screen-drops-collinearity-gates]]).
- **Exit-geometry** — an incumbent signal *does* exist, but the lever does not touch the risk
  budget, so collinearity-vs-`risk_per_share` is the wrong axis; the gate tests RoR direction
  instead.

## When to Apply

- Any **exit-geometry re-parametrization** in the ORB loop (profit target, breakeven trigger,
  time-exit, stop mode) where `mfe_r` (or the exit envelope) lets you re-book the SAME trades under
  the new geometry from telemetry: gate on signed `ror_delta` + `exit_change_frac`, drop
  collinearity, and treat the one-sided counterfactual as making a STOP trustworthy (a GO still
  needs the real flip backtest).
- **Not** for a sizing/tilt lever (keep the collinearity gate —
  [[pre-code-collinearity-gate-before-a-second-normalizer-lever]]) or an additive entry stream
  (count floor + additive `ror_shift` — [[additive-entry-stream-screen-drops-collinearity-gates]]).
- **Staleness contract.** The counterfactual is denominated in `R = range_high - range_low` at the
  head's stop mode (`stop_mode = 0.0`, the range-low stop), so `mfe_r` and the target compare
  directly. A future head that changes the stop mode shifts the `mfe_r` denominator and invalidates
  the cohort — freeze the candidate as a stop-mode-era snapshot, and re-derive if the head moves.

## Examples

`profit-target-075` (plan `2026-07-24-001`) — **documented NO-BUILD**. Candidate at
`adapters/nautilus/lab/candidates/profit-target-075/` (`diagnostic.py` + independent `twin.py`
agreeing bit-for-bit on the four readings; `gate-verdict.json` records `STOP threshold-fail` on
`ror_delta`, freeze commit predating the reading + pre-register hash recorded). No `orb.rs` /
`params.rs` edit; head stays v34. Reproduce:

```
cd adapters/nautilus
LS_DATA_HOME=$PWD/../../data/turn4-fresh LS_TURN_CANDIDATE=profit-target-075 \
LS_CALENDAR_SNAPSHOT=$PWD/state/krx.calendar.json \
  ./target/debug/lab-research turn diagnose
# -> ror_delta -0.019471 < 0.00065 -> STOP threshold-fail (typed exit 11)
```

## Related

- [[additive-entry-stream-screen-drops-collinearity-gates]] — the closest sibling: also drops
  collinearity for a lever class where it is undefined, but replaces it with a count floor +
  additive `ror_shift` (no incumbent signal) rather than direction + materiality.
- [[strategy-loop-turn-9-profit-target-sweep-and-mfe-distribution]] — the same `profit_target_r`
  lever, falsified as an expectancy lever by a live sweep; this gate is the governed Phase-A
  re-confirmation, and its direction gate is what refuses report-mfe's RUNNABLE band reading.
- [[pre-code-collinearity-gate-before-a-second-normalizer-lever]] — the collinearity gate this
  screen is explicitly *not* using; the contrast baseline (what collinearity is for).
- [[stop-geometry-lever-is-class-b-absorbed-and-near-inert]] — the other "add a materiality gate;
  the conservative reading makes a NO-BUILD robust" sibling.
- [[first-order-materiality-prediction-ignores-notional-ceiling]] — the amihud counterfactual that
  *over*-predicted (notional ceiling); the contrast that makes exit geometry's one-sided
  under-prediction notable.
- [[performance-json-realized-r-is-risk-normalized-not-internal-r-denom]] — why the RoR fold uses
  `risk_capital`, not `performance.json`'s risk-normalized `realized_r`.
