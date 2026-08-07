---
title: "Three traps in a sample-power answer: the calendar denominator, the paired SE nobody measured, and cutting cluster size to buy power"
date: 2026-08-07
category: conventions
module: adapters/nautilus/lab
problem_type: convention
component: statistics
severity: high
tags:
  - statistical-power
  - design-effect
  - intra-cluster-correlation
  - bootstrap
  - standard-error
  - unit-mismatch
  - paired-comparison
  - selection-bias
applies_when:
  - "Answering 'do we have enough data' — converting a required observation count into a required calendar span"
  - "Concluding from an absolute-detectability verdict that no comparison between two variants is measurable"
  - "Proposing to raise power by changing the shape of the sample (more per period, fewer per period) rather than its length"
  - "Reviewing any required-n figure whose numerator and denominator were computed in different places"
---

# Three traps in a sample-power answer

## Context

The ORB strategy loop asked "is the sample big enough to tell whether the head has an
edge, and if not, can we buy more?" Answering it twice — the sample-sufficiency turn
(TURN-LOG 2026-08-06) and the paired-power turn (2026-08-07) — surfaced three errors
that are not specific to that strategy, that arithmetic. Each one moved a conclusion.

## Trap 1 — the required-count denominator must be CALENDAR periods, not productive ones

A power calculation returns a required number of **observations**. Turning that into a
required number of **periods** means dividing by an observation rate, and the rate has
to be denominated in the same unit as the coverage it will later be compared against.

The ORB head produced 111 closed trades. It traded on **24** sessions, but its data
range spanned **45** calendar sessions. Two rates exist:

- 111 / 24 = **4.625 trades per *trade-producing* session** — the clustering unit
- 111 / 45 = **2.4667 trades per *calendar* session** — the acquisition unit

The first is what you need for a design effect. The second is what you need to convert
a required trade count into "how many days of history must we buy". Dividing by the
first and comparing against calendar coverage understated the requirement by the ratio
of the two — here **~1,866 sessions instead of ~3,499**, roughly half, and it flipped a
band row from "already satisfied" to "not reachable".

**The rule.** A period requirement is `required_observations / (observations per
CALENDAR period)`. Periods that produced nothing belong in the denominator: they are
periods you still had to buy. Name the unit at the site — a bare `per_session` field
is exactly the ambiguity that lets the wrong one through.

**The guard.** Make the basis a value, not a comment. `RateBasis` in
`adapters/nautilus/lab/src/runner/report.rs` is an enum with a `CalendarSessions`
variant carrying its own denominator and a `TradeProducingSessionsFallback` variant
that the report labels as an optimistic lower bound wherever it is used. Both rates are
printed; only one drives a verdict.

## Trap 2 — "the edge is undetectable" does NOT answer "is A different from B"

An absolute-detectability verdict asks whether one arm's statistic differs from zero.
A comparison between two variants asks whether arm A's statistic differs from arm B's,
**over the same periods**. These have different standard errors, and the second can be
far smaller: the period-level common shock — the market day both arms traded through —
cancels inside a paired difference and does not cancel inside either arm's own interval.

Concluding "at this sample nothing is attributable to anything" from an absolute verdict
is a category error. It may turn out true, but it is a separate measurement.

**The rule.** Before standing an experiment programme down on a power argument, ask
which question the power was computed for. If the programme is a series of A-versus-B
comparisons, the absolute verdict is not the binding one; measure the paired standard
error before concluding.

**How to measure it.** Resample **paired** blocks: draw one period index per slot and
apply it to *both* arms in every replicate. Drawing each arm's blocks independently
silently reproduces the unpaired standard error — which is the number you already had.
`paired_block_bootstrap_difference` in `adapters/nautilus/lab/src/stats.rs` is that
instrument; the single shared draw is the one line the whole design rests on, and it is
mutation-tested.

Two consequences that are easy to get wrong:

- **Build blocks over the UNION of the periods either arm traded, not the intersection.**
  A period an arm did not trade contributes nothing to that arm's sums, which is exactly
  what makes the paired point estimate equal each arm's recorded whole-run difference. An
  intersection silently measures a differently-scoped quantity and can agree with the
  union by coincidence — so any test of this identity must be built on data where the two
  **disagree**, or it asserts nothing.
- **For an arm that adds periods of its own, the reported SE is a hybrid.** The
  cancellation applies only to shared blocks; arm-only blocks contribute an unpaired
  component. Report the union and intersection counts beside the figure.

In the ORB case the paired answer came back the same as the absolute one — no arm was
attributable — but that was a result, not a foregone conclusion, and the arms it failed
to resolve were the largest effects the design space contained.

## Trap 3 — cutting cluster size makes the requirement WORSE, not better

Clustered observations cost information: the design effect is `1 + (m₀ − 1)·ρ` at
cluster size `m₀` and intra-cluster correlation `ρ`. The intuition "fewer observations
per period means less clustering means more power per observation" is correct and
irrelevant, because the **period requirement** is what you are buying, and cutting
observations per period cuts the observation rate faster than it cuts the design effect.

Holding ρ = 0.327 fixed and varying trades per productive session `m` against
participation `p`, the ORB requirement in calendar sessions:

| m | p = 0.53 (observed) | p = 1.00 |
|---|---|---|
| 1 | 7,498 | 3,999 |
| 4.625 (observed) | **3,499** | 1,891 |
| 10 | 2,959 | 1,578 |
| → ∞ | 2,454 | **1,308** |

One trade per session needs **more than twice** as many sessions as the observed
clustering does. And there is an asymptote: even unbounded observations per period, at
full participation, floors the requirement at 1,308 sessions. Driving ρ to zero at the
observed cluster size still needs 1,622.

**The rule.** Only two things move a period requirement materially: a **larger true
effect** (required n scales as the inverse *square* of the target, so this dominates
everything else), or a lower ρ. Cluster size and participation are bounded levers with
an asymptote you can compute in advance — compute it before proposing either as a fix,
because "reduce the clustering" is ambiguous between the one that helps (ρ) and the one
that backfires (m₀).

## Related

- [`coverage-only-change-is-verified-by-mutation-not-by-the-gate`](coverage-only-change-is-verified-by-mutation-not-by-the-gate.md)
  — a derivation guard over a committed fixture is green before and after a behavior
  change; the paired estimator above is guarded by mutation for exactly that reason.
- [`range-scoped-comparability-scope-every-derived-input`](range-scoped-comparability-scope-every-derived-input.md)
  — why a paired comparison gates on the universe hash and code hash too, not on the
  catalog fingerprint alone.
- [`performance-json-realized-r-is-risk-normalized-not-internal-r-denom`](performance-json-realized-r-is-risk-normalized-not-internal-r-denom.md)
  — why the paired statistic is a difference of ratios of sums rather than a mean of
  per-observation differences.
- `adapters/nautilus/lab/TURN-LOG.md` (2026-08-06 and 2026-08-07 entries) and
  `docs/plans/2026-08-07-001-docs-orb-sample-acquisition-decision-plan.md` carry the
  turn-specific figures. This entry is about the rules, not the numbers.
