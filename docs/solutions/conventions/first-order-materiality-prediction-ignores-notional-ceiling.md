---
title: "A budget-tilt's first-order Phase-A materiality prediction ignores the notional ceiling — it over-predicts and can mis-sign the flip"
date: 2026-07-16
category: conventions
module: adapters/nautilus lab strategy loop (lab/src/strategy/orb.rs entry_qty, lab/src/params.rs *_tilt_weight), CLASS B sizing levers
problem_type: convention
component: strategy-loop
severity: medium
applies_when:
  - "Designing a Phase-A materiality gate for a numerator-only budget tilt (a dimensionless weight w multiplying risk_per_trade_krw, e.g. the ratio-ATR or Amihud liquidity tilt)"
  - "The tilt's clamp band has a WIDE upper clamp (w_hi well above 1.0 — here w_hi = 6.54, vs the ratio-ATR tilt's 1.44)"
  - "Predicting the flip's return-on-risk shift from the closed-trade sample before building/running"
tags:
  - strategy-loop
  - sizing-lever
  - materiality-gate
  - notional-ceiling
  - phase-a-prediction
  - pre-registration
---

# A budget-tilt's first-order Phase-A materiality prediction ignores the notional ceiling — it over-predicts and can mis-sign the flip

## Context

The CLASS B sizing levers size with `qty = min(floor(budget·w / risk_per_share), floor(notional / price))` — a
risk-budget quantity **capped by a fixed-notional ceiling** (`orb.rs` `entry_qty` →
`params.rs::position_qty_risked_tilted`). The Phase-A materiality gate predicts the flip's
return-on-risk (RoR) shift with a **first-order reweighting** over the closed-trade sample:

```
RoR' = Σ (w_i · rc_i · r_i) / Σ (w_i · rc_i)
```

where `rc_i` is the trade's realized risk_capital and `r_i` its realized R. This treats `qty`
(and thus `rc`) as scaling **continuously** with the tilt weight `w`. It does not — the
`floor(notional / price)` ceiling clips any upsizing once `budget·w / rps` exceeds the
fixed-notional quantity.

On the 2026-07-16 Amihud liquidity tilt (plan 2026-07-16-003), the first-order prediction read
`RoR' − RoR = +0.030882` (a large *positive* shift, RoR 0.1262 → predicted 0.157). The actual
flip landed at **RoR 0.1147 — negative** (−0.0116). The prediction was wrong not just in
magnitude but in **sign**.

## Guidance

When a numerator-only budget tilt has a **wide upper clamp** (`w_hi` materially above 1.0), the
first-order `RoR'` prediction is unreliable and can mis-sign the flip. The Phase-A materiality
statistic for such a lever should model the **notional ceiling**, not the continuous reweight:

- Use the **ceiling-aware qty** in the materiality prediction — recompute
  `qty_i = min(floor(budget·w_i / rps_i), floor(notional / px_i))` per trade and derive the
  predicted RoR from the resulting realized-risk deployment, exactly as the *integer-qty-change*
  materiality sub-gate already does (that sub-gate correctly counts `floor`ed clips; the RoR-shift
  sub-gate did not).
- Treat the first-order `RoR'` as a **direction-and-magnitude hint that a wide upper clamp
  invalidates**, never as a KEEP predictor. A wide-`w_hi` tilt up-weights the most-liquid /
  lowest-signal names hardest, and those are exactly the high-price names the notional ceiling
  clips — so the predicted upside is the part most likely to evaporate, while the down-weighting
  of the opposite cohort still bites in full.

The pre-registered threshold (`RoR-shift ≥ 0.00065`) stays as a *materiality floor* (is the lever
non-trivial?), but a GO on it must not be read as a positive-RoR forecast. **A GO does not
guarantee a KEEP** — this is one concrete reason why.

## Why This Matters

The materiality gate exists to catch inert levers *before* a build. If its RoR-shift reading is
taken as a KEEP forecast, a wide-clamp tilt looks like a strong positive edge (here 47× the floor,
positive sign) and invites over-confidence in a flip that in fact degrades RoR. The honest read is:
the gate confirmed the lever is *material* (mean-R invariant, 75/167 integer-qty changes — a real
reallocation), and the flip then measured it RoR-negative. Conflating "material" with "beneficial"
is the trap.

## When to Apply

Designing or reviewing any Phase-A materiality prediction for a numerator-only budget tilt whose
clamp band reaches well above 1.0. Narrow-band tilts (e.g. the ratio-ATR tilt's `w_hi = 1.44`) clip
far less, so the first-order prediction is closer — but the ceiling-aware qty is the robust default
regardless.

## Examples

Amihud liquidity tilt (v30 → v32, n = 167), first-order vs actual:

| reading | first-order prediction | actual flip |
|---|---|---|
| return-on-risk | 0.157 (RoR' ) | **0.1147** |
| RoR shift vs v30 (0.1262) | **+0.0309** | **−0.0116** (opposite sign) |
| clamp band | `w_lo = 0.60`, `w_hi = 6.54` | — |

The `w_hi = 6.54` up-weighting of low-illiquidity (liquid) names was clipped by
`floor(notional / price)` for the high-price names it hit hardest, so the predicted upside never
deployed; the `w_lo = 0.60` down-weighting of illiquid names (which carried at/above-average
P&L-per-risk here) still cut return in full. Net: more risk deployed for less P&L → RoR fell.

## Related

- `docs/solutions/conventions/pre-code-collinearity-gate-before-a-second-normalizer-lever.md` — the sibling Phase-A gate (collinearity, the other half of the dual gate).
- `docs/solutions/conventions/strategy-loop-reading-param-turn-outcomes-win-rate-vs-expectancy.md` — reading param-turn outcomes on the right (size-invariant) metric.
- TURN-LOG `Amihud liquidity budget tilt` entry (2026-07-16), plan `docs/plans/2026-07-16-003-feat-amihud-liquidity-tilt-plan.md`.
