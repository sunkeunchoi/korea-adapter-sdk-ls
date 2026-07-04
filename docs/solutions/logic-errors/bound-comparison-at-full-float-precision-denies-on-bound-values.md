---
title: "A numeric-bound guardrail comparing at full float precision denies intended on-bound values by accumulated rounding dust"
date: 2026-07-04
category: logic-errors
module: "nautilus-ls-lab agent guardrails (adapters/nautilus/lab/src/agent/guardrails/proposal_bounds.rs); CLI wiring (adapters/nautilus/lab/src/runner/research.rs)"
problem_type: logic_error
component: tooling
symptoms:
  - "A governed loop turn that should approve an exactly-on-bound step (gap floor 2.4 -> 1.2, a 0.5 relative change against a 0.5 cap) was DENIED"
  - "The stored current value was 2.4000000000000004 (turn 1's 3.0 * 0.8), so |1.2 - 2.4000000000000004| / 2.4000000000000004 = 0.5000000000000001 > 0.5"
  - "The rejection reason, formatted at {:.4}, printed the self-contradictory 'relative change 0.5000 exceeds bound 0.5000'"
  - "Invisible to unit tests seeded with exact literals (a hand-written 2.4 gives exactly 0.5); only a value produced BY the loop's own prior arithmetic carried the dust"
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - proposal-bounds-guardrail
  - decision-pipeline
  - lab-research-cli
tags:
  - nautilus-adapter
  - lab
  - guardrail
  - float-comparison
  - epsilon-tolerance
  - governance
  - display-vs-enforcement-precision
  - scrub-heuristic
---

## Problem

A guardrail that gates a proposal by relative change (`|proposed - current| / |current| <= max`) compared at full `f64` precision but was *specified and displayed* at 4 decimals. A value the system produced itself — `3.0 * 0.8 = 2.4000000000000004` stored by a prior loop turn — made an intended clean half-step to `1.2` compute `0.5000000000000001`, one unit-in-the-last-place over a `0.5` cap, so the guardrail denied it. The `{:.4}` reason then printed `relative change 0.5000 exceeds bound 0.5000`, an assertion that is false at the precision it is stated in.

## Symptoms

- The chained-turn path the plan prescribes (`2.4 -> 1.2 -> 0.6`) is blocked on its first leg with no operator-visible cause — the numbers shown are equal.
- Reproducible only from loop-accumulated state: `OrbParams` re-derived by a prior turn carries rounding; a fresh fixture with a literal `2.4` never trips it, so happy-path tests pass while live use fails.
- The denial is *correct* at full precision and *wrong* at the specified precision — the two disagree by ~1e-16.

## What Didn't Work

- **Proposing a "cleaner" absolute target** (e.g. `current / 2 = 1.2000000000000002`) would sneak under the bound but is an ugly workaround that re-accumulates dust every turn and pushes the float-noise problem downstream — it does not fix the guardrail, and the next turn hits the same wall.
- **Weakening the cap** (0.5 -> 0.6) is explicitly forbidden: the first real use of governance must not be loosening it to get an answer through. The bug is precision handling, not the policy value.

## Solution

Enforce the bound at the precision it is specified in by adding a dust-sized tolerance, preserving the fail-closed behavior for NaN and the zero-current `INFINITY` case:

```rust
// A comparison tolerance so float dust at the bound does not deny an intended
// on-bound step. The 0.5 policy is not a 0.5000000000000000-exact policy.
const BOUND_EPSILON: f64 = 1e-9;

let within_bound = matches!(
    relative_change.partial_cmp(&(self.max_relative_change + BOUND_EPSILON)),
    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
);
```

`partial_cmp` returning `None` on any NaN (a NaN bound, a NaN relative change) still falls outside `Less | Equal` and rejects; the zero-current `INFINITY` is `> bound + epsilon` and rejects. A genuinely over-bound change (e.g. relative change `0.5833`) is orders of magnitude past a `1e-9` tolerance and still rejects. Regression test asserts the loop-produced `3.0 * 0.8 -> 1.2` case approves while `2.4 -> 1.0` still rejects.

## Why This Works

The tolerance closes the gap between two precisions that were silently different: the bound is **enforced** at full `f64` precision but **specified and displayed** at `{:.4}`. `1e-9` sits deliberately between float dust (`~1e-16`, the error to absorb) and the display grain (`1e-4`, the smallest difference a human specified) — large enough to swallow accumulated ULP error, far smaller than any intentional proposal delta. Because the comparison is a *relative* change (a dimensionless ratio bounded near 1), an absolute epsilon on it is scale-stable; this reasoning would not hold for an absolute-magnitude comparison, where the epsilon would need to scale with the operands.

The deeper rule: **a value that flows back into a comparison after passing through the system's own arithmetic will not equal the literal you'd write by hand.** Any equality-or-boundary test on such a value must be taken at the precision the value is meaningful to, not at raw `f64` precision.

## Prevention

- **When a bound is displayed/documented at precision P, enforce it at precision P.** If a reason string formats numbers at `{:.4}`, a `<=` at `1e-16` precision can contradict its own message. Either round both sides to P before comparing, or add a tolerance smaller than P and larger than expected dust.
- **Test boundary cases with system-produced values, not hand-written literals.** A guardrail's `== bound` and `just over bound` cases must be fed values that went through the same arithmetic the production path uses (here, a prior turn's `x * factor`), or the dangerous ULP-drift arm never executes under test. A literal `2.4` hides the bug; `3.0 * 0.8` exposes it.
- **Sibling trap — structured identifiers must not travel a free-text scrub route.** The same wave nearly shipped a bug where the CLI routed *all* stdout through the credential scrub, whose account-number heuristic masks any 6+-digit run. Korea (KRX) `shcode`s are 6 digits (`005930`), and run-id timestamps are digit-dense, so `catalog status` would have printed `***.XKRX` for every symbol. The fix: render ids, symbols, paths, and numbers as typed/structured values printed verbatim; route only genuinely free-form prose (an operator-supplied explanation) through the scrub. When a masking heuristic keys on a shape, confirm your legitimate structured tokens don't collide with that shape — and if they do, keep them off the free-text path by type, not by hoping.
