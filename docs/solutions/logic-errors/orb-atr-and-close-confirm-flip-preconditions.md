---
title: "Two latent modeling bugs in the ORB lever-queue gates must be fixed before the ATR-stop and close-confirm levers are flipped"
date: 2026-07-12
category: logic-errors
module: adapters/nautilus/lab strategy loop (lab/src/strategy/orb.rs — stop_for_entry ATR arm + session_gate_reject + on_bar entry block; operator data home data/turn4-fresh)
problem_type: logic_error
component: strategy_engine
severity: medium
applies_when:
  - "Flipping the ATR-scaled stop lever (stop_mode 2.0) as a strategy-loop turn"
  - "Flipping the close-confirmed entry lever (entry_confirm 1.0) as a strategy-loop turn"
  - "Any orb.rs harness code turn that re-baselines strategy_code_hash — the natural place to land these fixes"
related_components:
  - "orb.rs OrbState::stop_for_entry / session_gate_reject / on_bar"
  - "plan docs/plans/2026-07-11-001 (lever queue, KTD5/KTD6)"
---

## Context

The mechanism-harness turn (plan `2026-07-11-001`) shipped queue levers 1–5 as
default-off gates in `orb.rs`, re-baselined to v13 (all-off, reconciled 166/166 to
v9), and flipped only lever 1 leg 1 (OR-midpoint stop → falsified). Per KTD8 the
shipped `orb.rs` is **hash-locked** to the verdict-bearing runs (v13/v14), so no
`orb.rs` edit landed post-baseline. Post-baseline code review found two modeling
bugs in the **default-off** paths — reachable only by the *future* ATR-stop and
close-confirm flips, so no shipped run is affected, but each would **bias the very
metric its flip measures** if run as-is.

These fixes must ride the next harness code turn (which re-baselines anyway); do
**not** land them as a standalone `orb.rs` edit on the shipped runs.

## The two bugs

### 1. ATR stop collapses onto the entry when `round(stop_atr_mult · ATR) ≤ 0`

`OrbState::session_gate_reject` fails an ATR-mode session closed only when
`prior_atr.is_none()`. A symbol whose deduped prior dailies are flat (halted /
thin small-cap — exactly the gappier tiers this strategy reaches) yields
`prior_atr = Some(0.0)`, which **passes** the gate. Then in `stop_for_entry`:

```rust
let dist = (params.stop_atr_mult * atr).round() as i64;   // 0 when atr≈0
(entry_price - dist).max(self.range_low)                  // → entry_price
```

`stop_price == entry_price` ⇒ `r_denom = entry − stop = 0` ⇒ no target, and on the
entry bar `low ≤ stop_price` is `low ≤ entry` (always true) ⇒ a guaranteed same-bar
`Stop` exit at the bar low. Every such entry books a fabricated full-range loss,
polluting the ATR-mode backtest. (A misconfigured *negative* `stop_atr_mult` is the
same failure; that subcase is now caught by `OrbParams::validate()`, but the
`atr = 0.0` and tiny-`mult·atr`-rounds-to-0 subcases are not — they need the gate
fix.)

**Fix (lands with the ATR-stop flip's code turn):** treat non-positive ATR as
unavailable — `self.prior_atr.filter(|a| *a > 0.0)` in **both** the ATR-stop arm
and the OR-width arm of `session_gate_reject` — and floor the ATR distance:
`let dist = ((params.stop_atr_mult * atr).round() as i64).max(1);`.

### 2. Close-confirm books a same-bar stop that precedes the fill

In close-confirm mode the fill is anchored at the bar **close** (`entry_price =
close`, the last event of the bar). The entry block still runs the same-bar stop
check `if low ≤ stop_price` — but for it to fire, `low < close = entry`, i.e. the
stop-touching low occurred at an earlier tick, **before** the close-anchored fill
existed. The position was not open when the stop level was touched, yet an
`Exit::Stop` is booked — a loss the trade never incurred. This is asymmetric with
the deliberate pessimism two lines above (the entry bar's high is *not* folded
because it is "not provably post-fill"): by the same logic the entry bar's low is
provably **pre-fill** in close-confirm mode.

Note this **contradicts KTD6's explicit "same-bar stop-first still wins"**, which
was written for wick-touch entry (fill at the bar high, mid-bar). Whether to keep
that pessimism once the fill is close-anchored is a **design decision for the
close-confirm flip**, not a unilateral fix — surface it to the plan owner. The
likely resolution: skip the same-bar stop check when `params.close_confirm_entry()`
(gate it on wick mode only).

## Why This Matters

Both bugs are invisible today (default-off, no shipped run touches them) and pass
all unit tests, because the tests assert the *specified* behavior and the specs
(KTD5's fail-closed, KTD6's same-bar stop) did not anticipate `atr = 0.0` or the
close-anchored-fill ordering. A future flip that enables the lever without these
fixes would run a **biased** backtest and record a wrong keep/revert verdict —
falsifying (or spuriously keeping) the lever for the wrong reason. The harness
exists to attribute edge to a lever cleanly; a fabricated stop-out defeats that.

## When to Apply

Before flipping `stop_mode` 2.0 (ATR) or `entry_confirm` 1.0 (close-confirm): land
the corresponding fix in the same code turn, re-baseline (the fix is verdict-neutral
for the all-off baseline — it only touches the ATR/close-confirm paths), then run
the flip. The OR-width gate shares the ATR-availability arm, so fix #1 also hardens
any future OR-width flip against a flat-history `atr = 0.0`.
