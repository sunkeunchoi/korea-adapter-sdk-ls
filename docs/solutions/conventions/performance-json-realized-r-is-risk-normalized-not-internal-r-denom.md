---
title: "performance.json realized_r is P&L-over-risk_capital, not (exit-entry)/internal_r_denom — don't use it to infer r_denom"
date: 2026-07-21
category: conventions
module: adapters/nautilus lab strategy loop — run performance.json trade records; offline reconstruction of orb.rs internals
problem_type: convention
component: strategy-loop
severity: medium
applies_when:
  - "Reconstructing an ORB strategy-loop backtest's internal geometry offline from a run's performance.json (a diagnostic screen, a materiality re-sim, a candidate probe)"
  - "You need to know the strategy's INTERNAL r_denom (RangeLow OR-width = range_high - range_low, vs the entry-stop trade-R) — e.g. to model where the profit target / breakeven trigger sit"
  - "Tempted to derive r_denom from a trade's realized_r as (avg_px_close - avg_px_open) / realized_r"
tags:
  - strategy-loop
  - performance-json
  - offline-reconstruction
  - realized-r
  - risk-per-share
  - diagnostic-first-probe
  - class-b
---

## Context

A run's `performance.json` trade record carries `realized_r` alongside `realized_pnl`,
`risk_capital`, `quantity`, `avg_px_open`, and `avg_px_close`. It is tempting — when
reconstructing the strategy offline (a Turn-N diagnostic screen, a materiality re-sim) — to
recover the strategy's **internal** R-denominator from it: `r_denom = (avg_px_close −
avg_px_open) / realized_r`. That derivation is **circular** and will mislead you.

`realized_r` in `performance.json` is the trade's P&L normalized by its **risk capital**, not by
the strategy's internal `r_denom`:

```
realized_r == realized_pnl / risk_capital
           == (avg_px_close − avg_px_open) / risk_per_share      (since realized_pnl = qty·(C−P)
                                                                   and risk_capital = qty·risk_per_share)
```

where `risk_per_share = entry − stop` (`orb.rs:530`). Verified on the v32 head run: `realized_r
== realized_pnl / risk_capital` for all 77 closed trades (0 mismatch). So
`(C−P)/realized_r` tautologically returns `risk_per_share` for **every** trade, regardless of
the strategy's actual `r_denom`.

The strategy's internal R (`orb.rs::entry_r_denom`, line 928) is a **different** quantity: under
the default RangeLow stop mode it is `range_high − range_low` (OR-width), **decoupled** from the
stop; under the OrMidpoint/Atr modes it is `entry − stop` (which coincides with `risk_per_share`).
The internal `r_denom` is what sets the profit target and breakeven trigger — it is the telemetry
`realized_exit_r` (line 557) divides by, NOT the `performance.json` `realized_r` field.

The trap bites hard: `(C−P)/realized_r` always equals `risk_per_share`, so a RangeLow run (whose
internal `r_denom` is OR-width, *not* `risk_per_share`) *looks like* it is using the entry-stop
`r_denom` — appearing to **contradict** the RangeLow premise. In Turn 11 (plan
`2026-07-21-001`) this nearly triggered a false "the data contradicts the plan's RangeLow claim"
blocker before the circularity was spotted.

## Guidance

**Never infer internal `r_denom` from `realized_r`.** To settle which `r_denom` a run used, use
**clean target-hit exits**: for a trade that exits at its profit target (`profit_target_r = 1.0`),
`avg_px_close − avg_px_open == r_denom` (modulo the backtest's favorable gap-through-limit fills,
which land the exit a tick or few *above* the target). Match that per-trade delta against the
candidate `r_denom` hypotheses and count which one wins:

```python
# for each closed trade
dC   = round(avg_px_close - avg_px_open)          # exit move
orw  = range_high - range_low                     # RangeLow hypothesis (OR-width)
es   = entry - stop  (== risk_capital/quantity)   # entry-stop hypothesis
# tally: does dC match orw or es on the clean target hits?
```

In Turn 11 this gave **11/77 exits matching OR-width vs 4/77 matching entry-stop**, confirming the
RangeLow decoupled `r_denom` per `orb.rs::entry_r_denom` — the opposite of what the circular
`realized_r` derivation implied.

`risk_per_share` itself, by contrast, *is* cleanly recoverable from `performance.json`:
`risk_per_share = risk_capital / quantity` (this is what the amihud/gap-retention diagnostics use).
Just don't confuse it with the internal `r_denom`.

## Why This Matters

The internal `r_denom` determines where the target and breakeven barriers sit, so any offline
re-simulation that models trade resolutions (stop/target/timeout) needs the *right* one. Deriving
it circularly from `realized_r` silently substitutes `risk_per_share` for the true (possibly
decoupled) `r_denom`, mis-placing every barrier — and worse, it reads as a contradiction of a
correct premise, which can abort a turn on a phantom blocker. The reconstruction cost of catching
this mid-turn is high; the rule is cheap.

## When to Apply

- Any offline reconstruction of ORB strategy internals from a run's `performance.json` — a
  diagnostic screen, a materiality re-sim, a candidate probe, a reconciliation check.
- Whenever a reconstruction appears to contradict the head's declared stop mode / `r_denom` — check
  for this circularity **first**, before treating it as a real data-vs-plan contradiction.

## Examples

Circular (wrong) — always returns `risk_per_share`, so a RangeLow run looks like entry-stop:

```python
r_denom = (t["avg_px_close"] - t["avg_px_open"]) / t["realized_r"]   # == risk_capital/quantity, ALWAYS
```

Correct — settle `r_denom` from clean target hits, recover `risk_per_share` directly:

```python
rps = t["risk_capital"] / t["quantity"]            # risk_per_share — clean
# internal r_denom: match (C-P) on target exits against OR-width vs entry-stop; don't divide realized_r
```

Related: the Turn 11 NO-BUILD outcome and the CLASS-B-absorption result in
`docs/solutions/conventions/stop-geometry-lever-is-class-b-absorbed-and-near-inert.md`.
