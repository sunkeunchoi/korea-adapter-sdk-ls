---
title: "A stop-WIDTH lever is CLASS-B-absorbed on risk and near-inert on RoR — screen its geometry, not just its collinearity"
date: 2026-07-21
category: conventions
module: adapters/nautilus lab strategy loop (lab/src/strategy/orb.rs, lab/src/params.rs), CLASS B sizing levers
problem_type: convention
component: strategy-loop
severity: medium
applies_when:
  - "Proposing to re-scale or condition the INITIAL STOP distance in the ORB loop (a 'stop-width' / 'stop-geometry' lever, issue #119 / plan 2026-07-21-001)"
  - "The head runs CLASS B risk sizing (`risk_per_trade_krw` active) so `risk_capital = qty · risk_per_share` is pinned at budget"
  - "Deciding whether to build+run a CODE turn to move the stop location, or stop at a cheap offline geometry screen"
  - "A collinearity pre-check (the sibling convention) PASSES — the conditioning signal is genuinely decorrelated — and you are tempted to build on that alone"
tags:
  - strategy-loop
  - stop-geometry
  - class-b
  - collinearity
  - diagnostic-first-probe
  - inert-prediction
  - resolution-mix
  - pre-registration
---

## Context

Turn 11 (issue #119) asked whether the initial stop *location* carries independent edge in
the ORB loop. It does not, and the reason is structural — worth recording so a future turn
does not re-litigate it.

Under CLASS B sizing (`qty = budget · w_ratio / risk_per_share`, `risk_per_share = entry −
stop`), re-scaling the stop distance by any factor `w` re-sizes `qty` inversely, so
`risk_capital = qty · (w · risk_per_share)` stays pinned at the budget. **A stop re-scale is
invisible to the RoR denominator** (`RoR = Σpnl / Σrisk_capital`). The only surviving effect
is stop-out *geometry*: which fraction of trades resolve stop / target / timeout.

The v32 head runs `stop_mode = 0.0` = **RangeLow**, where `r_denom = range_high − range_low`
(OR-width) is **decoupled** from the stop (`orb.rs::entry_r_denom`). So a stop-width weight
moves the stop but leaves the target and the breakeven trigger **fixed** — it changes
reward:risk, not barrier-scaling. In R-multiple space a target still books ≈+1R and a stop
≈−1R regardless of `w`; the geometry only bites when the moved stop crosses a *different*
first barrier, and the breakeven ratchet (to entry) caps the downside independent of `w`
once armed.

## Guidance

**Screen a stop-geometry lever on GEOMETRY materiality, not on collinearity alone.** The
sibling collinearity gate ([[pre-code-collinearity-gate-before-a-second-normalizer-lever]])
is necessary but *not sufficient* here: a stop-geometry signal can be perfectly decorrelated
from the risk axis and still be inert, because the effect it modulates (resolution mix) is
tiny under a governed stop move.

The Turn 11 screen (`adapters/nautilus/lab/candidates/stop-width-geometry/`) gates each
candidate signal on FOUR readings, two collinearity + two materiality:

1. **`collin_abs_rps < 0.70`** — `|Pearson r(weight, risk_per_share)|` (not a re-expression of the stop).
2. **`collin_abs_ratio_atr < 0.70`** — `|Pearson r(weight, w_ratio_atr)|` (not a re-expression of the KEPT ratio-ATR tilt).
3. **`resolution_mix_shift ≥ 0.05`** — the fraction of trades whose stop/target/timeout class the
   re-scaled stop moves, from an offline RangeLow geometry re-sim. This is the **primary**
   materiality reading and it is **fill-price-independent** (pure barrier crossing) — it
   replaces amihud's `qty_change_frac`, which a geometry lever changes *by construction* (CLASS
   B re-sizes qty for any `w ≠ 1`) and so would false-pass.
4. **`ror_shift ≥ 0.005`** — the projected RoR improvement, ceiling-aware, baseline = the same
   re-sim at `w = 1` so systematic fill bias cancels ([[first-order-materiality-prediction-ignores-notional-ceiling]]).
   Floor set **below the smallest historically-KEPT lever gain** (ratio-ATR +0.0091) and
   **above the screen's absolute sim error** (`sim(w=1)` RoR ≈ 0.152 vs the run's 0.1876 — the
   run books favorable gap-through-limit fills the screen does not) so a pass is genuinely
   build-worthy, not noise. The result is robust for any ror_shift floor ≥ 0.002.

Winner = largest `ror_shift` among signals clearing all four gates; none clearing → NO-BUILD.

## Why This Matters

**All four candidate signals PASSED both collinearity gates** — the naive "collinear → dead"
prediction did NOT fire here. The kill was **materiality**:

| id | signal | \|r(w,rps)\| | \|r(w,w_ratio)\| | ror_shift | resolution_mix_shift |
|----|--------|------|------|-----------|----------------------|
| 1 | OR-width/ATR | 0.041 | 0.055 | −0.0024 | 0.000 |
| 2 | entry-minutes | 0.247 | 0.134 | **+0.0008** | **0.078** |
| 3 | gap magnitude | 0.207 | 0.246 | −0.0106 | 0.026 |
| 4 | OR-position | 0.255 | 0.023 | −0.0001 | 0.000 |

The best signal (entry-minutes) is genuinely a new axis AND actually moves the resolution mix
(7.8% of trades), yet its projected RoR shift is +0.0008 — 6× below the build floor, and three
of four signals *degrade* RoR. A governed stop move within CAP simply reshuffles which trades
stop out without net benefit: the decoupled RangeLow target is fixed, the breakeven ratchet
caps the downside independent of `w`, and CLASS B pins risk_capital. **No decorrelated
stop-geometry signal that materially improves RoR exists in `data/turn4-fresh` in the tested
`(ref/signal)^alpha` direction.**

**Honest bounds (Turn 11 code review — the NO-BUILD is a screen verdict, not a reconciled one).**
The verdict hinges on a single reading (`ror_shift 0.000827 < 0.005`); it survives scrutiny but
only with these caveats stated:

- **One direction only.** Each signal is screened as high-signal→tighter-stop; an *inverse*-direction
  edge reads negative and is discarded. So this is "no edge in the tested direction," and the
  inverse direction + asymmetric stop-vs-target levers were unscreened (out of scope).
- **`ror_shift` is a conservative lower bound.** The offline re-sim books flat target fills and
  omits the run's favorable gap-through-limit fills; sizing is exact (0/77) so the whole
  sim 0.152 vs run 0.1876 gap is fills. Because the lever *converts* resolutions, the bias does
  not fully cancel in the shift — it pushes the decisive reading DOWN (bounded ~0.0016 over ≤6
  converted trades). Even fully corrected the best signal is ~0.002–0.003 — still below the floor
  and within the amihud-demonstrated ~0.04 screen-prediction noise.
- **Floor is precedent-anchored, not "above sim error."** `0.005` is justified because the amihud
  precedent cleared the looser `0.00065` `ror_shift` floor, built, and REVERTED (a +0.0309 screen
  shift landed −0.0116) — sub-0.001 projected shifts do not predict a KEEP. Under the raw `0.00065`
  floor the minutes signal nominally clears, which is exactly why that floor is inappropriate here.
- **The twin certifies reproducibility, not barrier fidelity** (it shares the sim semantics by
  design). Correctness review cross-checked the sim against `orb.rs` and found it faithful; that
  fidelity is the main residual assumption.

The one weakly-positive result (entry-minutes) is the seed for any future stop-geometry turn:
start there, screen BOTH directions, and model favorable fills — do not re-derive from scratch.

So the honest lesson is two-part: (a) the collinearity gate is not the whole story for a
geometry lever — add a **fill-independent resolution-mix** materiality gate; and (b) the
stop-location seat is spent — CLASS B owns the risk axis, and the surviving geometry effect is
too small to clear a build-worthy bar. A future turn should not re-open "re-scale/condition the
stop" without a *new* mechanism that escapes CLASS-B absorption (e.g. an asymmetric stop-vs-target
move that changes reward:risk by *design*, the lever explicitly deferred in the Turn 11 scope).

## When to Apply

- Any proposed lever that re-scales or conditions the **initial stop distance** while CLASS B
  risk sizing is active — the risk axis is absorbed, so screen the geometry (resolution-mix)
  materiality before building.
- Not for an asymmetric stop-vs-target lever (move stop, hold target *by design*) — that is a
  distinct axis that is NOT absorbed and warrants its own screen.

## Examples

Turn 11 (plan `2026-07-21-001`, issue #119) — **documented NO-BUILD**. Screen at
`adapters/nautilus/lab/candidates/stop-width-geometry/` (diagnostic + independent twin,
gate-verdict `STOP threshold-fail` on `ror_shift`, freeze commit recorded). No `orb.rs` /
`params.rs` edit; head v32 stays (`strategy_code_hash d7a9820b…`, RoR 0.1876). Reproduce:

```
cd adapters/nautilus
LS_DATA_HOME=$PWD/../../data/turn4-fresh LS_TURN_CANDIDATE=stop-width-geometry \
  ./target/release/lab-research turn diagnose
# -> clearing signals: NONE -> STOP threshold-fail (ror_shift 0.000827 < 0.005)
```
