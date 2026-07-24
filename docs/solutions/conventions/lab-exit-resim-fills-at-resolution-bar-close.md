---
title: "Offline ORB exit re-sims must fill at the resolution-bar CLOSE, not the limit_price orb.rs submits — else ror_base won't reproduce"
date: 2026-07-24
category: conventions
module: adapters/nautilus lab strategy loop — offline exit re-simulation of orb.rs resolutions from a run's performance.json / decisions.jsonl
problem_type: convention
component: strategy-loop
severity: medium
applies_when:
  - "Offline re-simulating an ORB backtest's exit resolutions (stop / target / timeflat) over minute bars — a diagnostic screen, a materiality re-sim, a candidate probe"
  - "You need the re-sim to reproduce the head run's realized per-trade fill / realized_r / size-invariant ror_base (not just an additive shift where a fill bias would cancel)"
  - "Tempted to fill the exit at the price orb.rs's OrbAction::Exit carries (the bar low for stop/timeflat, the target price for target), or at the raw stop/range-low price"
tags:
  - strategy-loop
  - performance-json
  - offline-reconstruction
  - exit-engine
  - fill-model
  - diagnostic-first-probe
  - nautilus-backtest
---

## Context

When re-simulating an ORB backtest's exit resolutions offline (a Turn-N diagnostic screen, a
displacement/reallocation re-sim, a candidate probe), you walk each trade's minute bars and detect
its resolution — stop (`low <= stop`), target (`high >= target`), or timeflat (`t >= 15:00`) — then
book a fill price. The natural guess is to fill at the price `orb.rs` submits on the exit order:

```
orb.rs:638  OrbAction::Exit { limit_price: low,    reason: ExitReason::TimeFlat }
orb.rs:721  OrbAction::Exit { limit_price: low,    reason: ExitReason::Stop }
orb.rs:730  OrbAction::Exit { limit_price: target, reason: ExitReason::Target }
```

So a stop or timeflat exit *looks like* it fills at the bar **low**, and a target exit at the
**target price**. That guess is wrong for reproducing the head. `performance.json` is written by the
**nautilus backtest ledger**, not by `orb.rs`'s telemetry: the `OrbAction::Exit` is a
**marketable-limit order**, and the ledger fills it at the **resolution bar's CLOSE**, not at the
submitted `limit_price` wick.

The gap is not a rounding tick — it silently destroys the whole edge. In the
`orb-concurrency-slot-ranking` Phase-A turn (plan `2026-07-24-002`), a bar-**low** fill engine
computed a size-invariant `ror_base` of **-0.0001** against the true head value of **0.0398** — a
pessimistic stop always books the worst wick. Switching to the resolution-bar-**close** fill
reproduced **108/119** closed trades' `avg_px_close` exactly and an engine `ror_base` of **0.0386**
(within 0.005 of the true 0.0398).

Concrete case (v34 head, symbol `279570.XKRX`): the position dips to its range-low stop `5610` at
10:53, but that bar **closes at 5660**. The ledger fills at `5660` — `performance.json`
`avg_px_close = 5660`, `realized_r = -0.615` — not the `-1.0` a stop-price/bar-low fill implies.
The stop *detects* on the wick; it *fills* on the close.

## Guidance

**In an offline exit re-sim that must reproduce the head, detect the resolution on the bar
wick/geometry but fill at that bar's CLOSE — for all three exit reasons.** Keep the resolution
geometry faithful to `orb.rs::on_bar` (stop-first pessimism when a bar breaches both stop and
target; close-confirm entry so the entry bar is skipped and no same-bar stop fires; breakeven
ratchet to entry once high-water reaches `breakeven_trigger_r * r_denom`), but book the fill at the
close of whichever bar resolves:

```python
def simulate(bars, entry, r_denom, entry_ts, stop):
    target = entry + round(1.0 * r_denom) if r_denom > 0 else None
    be     = entry + round(0.41 * r_denom) if r_denom > 0 else None
    hw = entry
    for ts, high, low, close in bars:
        if ts <= entry_ts:                 # close-confirm: skip the entry bar
            continue
        if kst_time(ts) >= FLAT:  return "timeflat", close, ts   # <- CLOSE, not low
        if low <= stop:           return "stop",     close, ts   # <- CLOSE, not low/stop_px
        if target is not None and high >= target:
                                  return "target",   close, ts   # <- CLOSE, not target_px
        hw = max(hw, high)
        if be is not None and hw >= be:
            stop = max(stop, entry)        # breakeven ratchet, binds next bar
    ...
```

Then **calibrate before trusting it** on any un-ledgered cohort (blocked / displaced / hypothetical
trades that have no `performance.json` fill): assert the engine reproduces the head's closed trades
— e.g. `>= 90%` of closed trades fill at exactly the resolved bar close, and the aggregate engine
`ror_base` lands within a tight tolerance of the run's reported value. Only a calibrated engine
should score trades that never existed in the run.

A mark-to-market exit (booking a position early, e.g. a displacement) uses the **same convention**:
the displacement bar's `close`.

## Why This Matters

The fill model sets the *sign and magnitude* of every re-simulated trade's return, so an offline
screen that models resolutions is worthless if the fill is wrong. Two traps:

1. **A bar-low stop fill is not conservative, it is broken.** It books every stop at the worst wick
   of the breach bar, which on real KRX minute bars is often far below the recovered close. That
   drove the head's reproduced `ror_base` to ~0 — a re-sim that "reproduces" a zero edge would fail
   any calibration self-check, or worse, pass a poorly-chosen one and feed a garbage verdict.

2. **The "additive shift cancels fill bias" escape hatch does not generalize.** The
   `failed-break-reversal` diagnostic (plan `2026-07-22-001`) used bar-low pessimistic fills and was
   still correct *because* it reported an **additive** shift `RoR(base + new_population) - RoR(base)`
   where a uniform fill bias cancels to first order. Any re-sim that must reproduce the head's own
   realized `ror_base` — a reallocation/displacement screen, a base-vs-counterfactual comparison
   where the base is the head cohort itself — has no such cancellation and needs the faithful
   close-fill.

## When to Apply

- Any offline reconstruction that walks minute bars to resolve ORB trades and needs per-trade fills
  faithful to `performance.json` (not just an additive delta): displacement/reallocation screens,
  base-cohort reproductions, calibration self-checks.
- Whenever an exit re-sim's reproduced `ror_base` diverges wildly from the run's reported value —
  suspect the fill convention (bar-low vs bar-close) **first**, before the resolution geometry.
- Pair this with the denominator convention in
  `docs/solutions/conventions/performance-json-realized-r-is-risk-normalized-not-internal-r-denom.md`
  — that file is the *denominator* side (`realized_r = pnl / risk_capital`, and internal `r_denom`
  vs `risk_per_share`); this file is the *fill-price* side. Both must be right for a re-sim to
  reproduce the head.

## Examples

Wrong — fills at the submitted `limit_price` (bar low for stop/timeflat); a stop books the worst
wick, and the head's edge evaporates:

```python
if low <= stop:   return "stop", low, ts          # -0.0001 aggregate ror_base vs true 0.0398
```

Correct — detect on the wick, fill at the resolution bar's close (matches the nautilus ledger):

```python
if low <= stop:   return "stop", close, ts         # 108/119 exact fills; ror_base 0.0386 ~= 0.0398
```

Related: the reallocation NO-BUILD turn that surfaced this
(`adapters/nautilus/lab/candidates/orb-concurrency-slot-ranking/`), and the denominator-side
convention `docs/solutions/conventions/performance-json-realized-r-is-risk-normalized-not-internal-r-denom.md`.
