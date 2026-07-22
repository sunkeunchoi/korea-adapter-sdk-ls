---
title: "Screening an ADDITIVE entry-stream lever — drop the collinearity gates, gate on a count floor + an additive ror_shift"
date: 2026-07-22
category: conventions
module: adapters/nautilus lab strategy loop (lab/src/strategy/orb.rs, lab/src/params.rs), entry-stream levers
problem_type: convention
component: strategy-loop
severity: medium
applies_when:
  - "Proposing an ADDITIVE entry stream in the ORB loop — a new set of trades layered on the head, not a re-weighting/tilt of existing ones (e.g. lever 8 failed-break reversal, plan 2026-07-22-001)"
  - "The head's decision stream contains NO trades from the new mechanism, so there is no incumbent per-trade signal to correlate a candidate weight against"
  - "Deciding whether to build a CODE turn for the stream, or stop at a cheap offline Phase-A screen"
  - "Adapting the stop-geometry / collinearity screen (built for a re-weighting lever) to a stream that ADDS trades"
tags:
  - strategy-loop
  - entry-stream
  - additive-lever
  - collinearity
  - materiality
  - phase-a-screen
  - pre-registration
  - upper-bound
  - failed-break-reversal
---

## Context

The ORB strategy-loop lever queue had, until lever 8, only ever screened **re-weighting**
levers — a tilt or condition applied to trades the head already takes (ratio-ATR tilt, amihud
liquidity tilt, stop-width geometry). Their Phase-A screens are built around a **collinearity
gate**: a candidate weight must be decorrelated (`|Pearson r| < 0.70`) from the risk axis the
kept levers already own, or it is a re-expression, not a new axis
([[pre-code-collinearity-gate-before-a-second-normalizer-lever]]).

Lever 8 (the failed-break reversal, plan `2026-07-22-001`; a row in the lever-queue plan
`2026-07-11-001`) was the first **additive entry stream** — a new, long-only set of trades (buy the recovery after a confirmed
downside break of the opening range) layered on the untouched v32 breakout leg. That structural
difference breaks the re-weighting screen: **the new trades do not exist in the head run**, so
there is no incumbent per-trade signal to correlate a candidate weight against. A collinearity
gate has nothing to compute.

## Guidance

**Screen an additive entry-stream lever on a population-count floor plus an *additive*
`ror_shift`, and drop the collinearity gates entirely.** The screen
(`adapters/nautilus/lab/candidates/failed-break-reversal/`, diagnostic + independent twin)
reconstructs the head's barrier semantics offline and scores the hypothetical stream population
under the head's own sizing, gating on:

1. **`population_count ≥ COUNT_FLOOR`** (its own STOP gate). A thin population is a NO-BUILD
   *regardless of projected shift* — the additive RoR estimate is dominated by a handful of
   trades, and under the shared `max_concurrent` budget the realized post-contention population
   is thinner still. Set it well above the stop-geometry resolution-moved scale (12 here, ~2x).

2. **`ror_shift ≥ 0.005`** where `ror_shift = RoR(base + population) − RoR(base)` and
   `RoR = Σpnl / Σrisk_capital` (the size-invariant crux; note `risk_capital` is the
   entry-fixed risk, not `performance.json`'s risk-normalized `realized_r` —
   [[performance-json-realized-r-is-risk-normalized-not-internal-r-denom]]), **both terms
   under the diagnostic's OWN common re-sim baseline** — never the run's realized RoR. The
   additive stream shares one baseline with the head's re-simmed trades, so the pessimistic
   flat-fill bias cancels to first order (the stop-geometry / amihud precedent). Sizing is
   CLASS-B and **ceiling-aware**: `min(floor(budget·w_ratio / rps), floor(notional / price))` —
   the notional clip whose omission caused the amihud mis-prediction
   ([[first-order-materiality-prediction-ignores-notional-ceiling]]). Anchor the model with a
   hard identity check: reconstruct the head's own trade quantities and `require` an exact match
   (77/77 here) before trusting any hypothetical number.

There is **no collinearity gate** — an additive stream has no incumbent signal to be collinear
with. The floor is the standing 0.005 (below the smallest historically-KEPT gain, above
screen-prediction noise), and it applies to the additive shift, not a per-trade re-weighting.

**Recorded (not gated) primary reading: the fill-price-independent resolution mix.** Which
barrier a trade hits first (target / stop / time-flat) is pure geometry, independent of the
fill price, whereas any qty-weighted P&L stat inherits the flat-fill bias. Emit
`resolution_target_share` (and the full mix) as a twin-agreed **recorded** reading so a reviewer
reads the population's quality alongside the gated numbers — the two STOP gates stay the count
floor and `ror_shift`.

**The scored population is an unconstrained UPPER BOUND.** The diagnostic scores every candidate
event; at the flip the stream contends for the same `max_concurrent` slots and risk budget as
the incumbent leg, so displacement of existing trades and slot starvation shrink the realized
population — and those are visible **only at the flip**, never in Phase A (the same
Phase-A-optimism-vs-realized gap that makes an offline per-bucket ranking a hypothesis, not a
verdict — [[reconciled-run-can-falsify-an-approximate-per-bucket-ranking]]). Record this caveat
in `keep_anchor`. A negative screen is therefore a *robust* NO-BUILD: the real effect can only
be smaller.

**Dual-grammar screens use `winning_grammar_id` at tolerance 0, and a secondary-grammar GO is an
operator gate, not an auto-build.** When one bar sweep measures more than one candidate grammar,
emit `winning_grammar_id` (tolerance 0) so the independent twin must agree on the argmax. If the
flip param implements only the primary grammar, a machine GO whose winner is a *secondary*
grammar is a RETURN-TO-PLANNING signal, not a build authorization — the build units do not cover
it. The threshold gate alone cannot distinguish the two (the secondary's readings clear the same
floors), so carry the distinction in `winning_grammar_id` inside the verdict's `agreed_readings`
(machine-readable) and inspect it before running `turn governed`, which reuses any recorded GO.

## Why This Matters

The lever-8 screen falsified the mechanism decisively and cheaply, before any state-machine code
was written. v32 baseline re-sim RoR **0.1522** (sizing reconstructed **77/77 exactly** — the
barrier model is faithful, and it independently matches the stop-geometry re-sim of the same
head):

| grammar | n | `ror_shift` | target / stop / flat |
|---|---|---|---|
| 1 — breakdown-recovery (primary) | 53 | **−0.063198** | 0.038 / **0.736** / 0.226 |
| 2 — post-stop re-entry (secondary) | 14 | −0.006241 | 0.286 / 0.643 / 0.071 |

The primary grammar has an ample population (count gate passes) but is **an order of magnitude
below the floor and negative**: a long that buys the recovery **stops out 73.6 % of the time and
reaches target only 3.8 %**. On the large-cap KRX universe a confirmed downside break *continues*
— buying the recovery buys into a down-trend. The secondary grammar is also negative. Neither
clears → **NO-BUILD** (`ror_shift` STOP, typed diagnose exit 11).

Two lessons compound here:

- **Methodology.** The re-weighting screen does not transfer to an additive stream — swap the
  collinearity gates for a count floor, and read materiality as an *additive* `RoR(base+pop) −
  RoR(base)` under one common baseline. This is the sibling of the stop-geometry convention
  ([[stop-geometry-lever-is-class-b-absorbed-and-near-inert]]): that one *added* a
  fill-independent materiality gate to the collinearity screen for a re-weighting lever; this one
  *drops* collinearity entirely for a stream that adds trades. Both keep the fill-independent
  resolution mix as the trusted primary reading and the ceiling-aware additive `ror_shift`.

- **Domain.** A long-only failed-break reversal is not an edge on this universe. The event grammar
  the lever queue armed it against (late entries win, low-RVOL winners, breakout-strength band
  falsified) does not imply that buying a *failed downside break* pays — the continuation
  dominates the recovery for these large caps. A future turn should not re-open the failed-break
  *long* without a materially different universe or the long-only constraint lifted (shorting the
  continuation is the untested inverse).

## When to Apply

- Any proposed **additive entry stream** in the ORB loop (a new set of trades, not a tilt on
  existing ones): drop the collinearity gates, gate on a pre-registered population-count floor
  plus an additive `ror_shift` under a common re-sim baseline with ceiling-aware CLASS-B qty, and
  treat the scored population as an upper bound.
- Not for a **re-weighting / tilt** lever — those keep the collinearity gate
  ([[pre-code-collinearity-gate-before-a-second-normalizer-lever]]); an incumbent signal exists to
  correlate against.
- Whenever a dual/multi-grammar screen's flip param implements only one grammar: gate the build on
  `winning_grammar_id`, not just the threshold pass, or a secondary-grammar GO can silently
  authorize the wrong build.

## Examples

Lever 8 (plan `2026-07-22-001`; lever-queue plan `2026-07-11-001`) — **documented NO-BUILD**. Screen at
`adapters/nautilus/lab/candidates/failed-break-reversal/` (entry-local `diagnostic.py` +
catalog-wide independent `twin.py`, both agreeing byte-identically; `gate-verdict.json` records
`STOP threshold-fail` on `ror_shift`, freeze commit + pre-register hash recorded). No `orb.rs` /
`params.rs` edit; head v32 stays (`strategy_code_hash d7a9820b…`, RoR 0.1876). Reproduce:

```
cd adapters/nautilus
LS_DATA_HOME=$PWD/../../data/turn4-fresh LS_TURN_CANDIDATE=failed-break-reversal \
  ./target/release/lab-research turn diagnose
# -> winner grammar 2, ror_shift -0.006241 < 0.005 -> STOP threshold-fail (typed exit 11)
```
