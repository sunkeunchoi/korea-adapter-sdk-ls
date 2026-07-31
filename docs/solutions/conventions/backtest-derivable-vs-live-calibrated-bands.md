---
title: Backtest-derivable vs live-calibrated bands — classify a pre-registered band by its data provenance
date: 2026-07-16
category: docs/solutions/conventions
module: adapters/nautilus/lab/src/dispatch
problem_type: convention
component: tooling
severity: medium
applies_when:
  - pre-registering strategy bands for a paper-backtest-gated lab
  - deciding whether a band value can freeze up-front or must be scheduled
  - distinguishing economic-expectation bands from execution-tracking bands
  - designing Production Ladder rung pre-registration freezes
tags:
  - pre-registration
  - backtest
  - live-calibration
  - tracking-band
  - expectation-band
  - slippage
  - transaction-cost
  - commission
  - production-ladder
  - orb-lab
---

# Backtest-derivable vs live-calibrated bands — classify a pre-registered band by its data provenance

## Context

Freezing the ORB Production Ladder's pre-registration (`adapters/nautilus/lab/config/preregistration.json` + `PREREGISTRATION.md`, on branch `feat/production-ladder-prereg-freeze`, commit `a5b9dab`, **unmerged**) meant grounding every rung's numbers in the head **v30** backtest closed-trade distribution — "no invented numbers." Each of the four live rungs needs two pre-registered bands: an **economic expectation band** (cumulative P&L across the rolling 5-session escalation window) and a **tracking-error band** (execution divergence — slippage-per-share / approximated-fill fraction).

Freezing them exposed an asymmetry in data provenance. A paper backtest fills at **bar prices with zero commission and zero slippage**. Inspecting the v30 run (`data/turn4-fresh/runs/20260715T092847Z-backtest-orb-v30/performance.json`), every fill carries `"commission": 0.0` and executes at the bar price — e.g. trade `000150.XKRX`'s two fills at `1752000.0` and `1776000.0`, both `commission 0.0`. So the expectation band (an *outcome* distribution) is fully derivable from the backtest, but the tracking-error band has **no signal at all** in a paper run: slippage is identically zero. Inventing a tolerance from that would be fabricating a number.

## Guidance

**Classify each pre-registered band by its data provenance before you freeze it.**

- **Economic / outcome bands** (cumulative P&L) are properties of the *strategy's* decisions and are backtest-derivable. The v30 expectation bands are `floor = worst rolling-5 cum P&L × fraction`, `ceiling = best × 1.5 × fraction`.
- **Execution-divergence bands** (slippage, approximated-fill fraction) measure *live-vs-decision* execution and are **NOT** backtest-derivable. They must be **live-calibrated** — scheduled to freeze from real fills, not frozen up-front.

**Statutory-cost carve-out (amended 2026-07-31, orb-transaction-cost-model).** An earlier
version of this doc lumped *commission* into the live-calibrated family alongside slippage.
That does not follow. Commission and the securities transaction tax (증권거래세 + 농어촌특별세)
are a **third kind of quantity**: a *published rate times a known notional* — known in
advance, deterministic per fill, needing no live observation at all. Unlike slippage they
have full signal in a backtest the moment they are modeled, and unlike an invented
tolerance they are *sourced*, not fabricated. Deferring them alongside slippage silently
zeroes a cost term that can exceed the measured edge: the ORB head's gross edge measured
**22.8 bps** of round-trip notional while the deferred statutory + commission term was
**~23 bps** — the omitted term was larger than the signal, so the head's *sign* was
unknown until the cost model landed (v35 read net-negative). The rule:

- **Deterministic statutory/brokerage costs** (commission, transaction tax): model them in
  the backtest from **cited published rates** (`adapters/nautilus/lab/config/transaction-costs.json`),
  sell-side-asymmetric where the law is. Never defer them to live calibration — there is
  nothing to calibrate.
- **Stochastic execution divergence** (slippage, partial fills, queue position, market
  impact): live-calibrate exactly as this doc already prescribes. Nothing in the carve-out
  weakens the fail-closed rung-2 tracking band.

The slippage quantity is definitionally a live-minus-decision delta. In `adapters/nautilus/lab/src/dispatch/tracking.rs`:

```rust
let slippage = trade.avg_px_open - paper_price;   // live entry fill − decision price
```

and `SymbolSlippage.slippage_per_share` is documented as "`live_price − paper_price` (per share): positive = paid up vs the decision price." In a paper backtest the live fill *is* the bar/decision price, so this is structurally zero.

The two accessors in `adapters/nautilus/lab/src/dispatch/prereg.rs` **encode and enforce** the classification with deliberately different fail shapes:

- `tracking_band(rung)`: returns `Ok(None)` for `rung <= 1` (rung 1 is the calibration rung, KD6), but from `rung >= 2` it is **fail-closed** — an absent band is `Err(...)`, blocking a rung-2 dispatch until the band is re-registered from live data.
- `expectation_band(rung)`: **always required** — fail-closed if absent at any rung, because it is the backtest-derived economic band available from the start.

`PREREGISTRATION.md` states the reason explicitly: the v30 backtest fills at bar prices with zero commission and **zero slippage**, so a slippage/approximated-fraction band **cannot** be grounded in it. Per KD3/KD6 these are scheduled: each rung's tracking band freezes from the **preceding rung's LIVE data** before the first session at that rung, and `prereg.tracking_band(2)` is **fail-closed** until re-registered.

## Why This Matters

Inventing a slippage band from a zero-slippage backtest would be a **fabricated tolerance with no empirical basis** — a gate that looks rigorous but was never grounded in anything. The fail-closed rung-2 tracking band is the guardrail: it makes the *absence* of live calibration block escalation, so capital cannot scale to real execution risk until a real slippage distribution has been observed and a band frozen from it (the amendment protocol in `PREREGISTRATION.md` — compute the rung-1 live slippage distribution, then re-register before rung 2 opens).

This is the ladder's version of "every safe verdict fails toward not-safe": the rung-1→rung-2 transition is only "safe" when a live-calibrated band exists; when it does not, `tracking_band(2)` returns `Err` and the dispatch is refused rather than waved through with a made-up number.

## When to Apply

- Whenever pre-registering or threshold-setting **any** band, tolerance, or gate value from backtest data — first ask whether the metric even *has signal* in a paper/backtest run, or whether it only exists live.
- Specifically before each Production Ladder rung's tracking band freezes (rung-2 band from rung-1 live data, rung-3 from rung-2, …, KD3).
- Generalizes to **any paper-vs-live execution metric** that is *stochastic*: slippage, partial-fill / approximated-fill fraction, queue position, market impact. If a paper simulation zeroes it out **and no published rate determines it**, it cannot be pre-registered from that simulation — schedule it to calibrate from live data and fail closed until it exists. Commission and statutory transaction tax are **not** in this family (see the statutory-cost carve-out above): a paper simulation zeroing them out is a modeling *omission* to fix with cited rates, not a quantity to live-calibrate.

## Examples

**Frozen `preregistration.json`.** All four rungs carry an `expectation_band`, scaled by the rung fraction — `[−69,000, 533,000]` at rung 1 up to `[−690,000, 5,334,000]` at rung 4. **No rung carries a `tracking_band`**: rungs 2-4 are intentionally absent (scheduled), and rung 1 carries none by design (calibration). The file's `_note` records why: the backtest has zero slippage, so the bands cannot be grounded now, and a rung-2+ dispatch is fail-closed until its band is re-registered.

**Accessor behavior on the frozen file** — the guard test `adapters/nautilus/lab/tests/preregistration_config.rs`:
- `expectation_band(1).unwrap()` loads clean; `tracking_band(1).unwrap().is_none()` — "rung 1 has no tracking band (KD6)".
- `rung_2_tracking_band_is_fail_closed_by_design` asserts `tracking_band(2).is_err()` — "rung-2 tracking band is fail-closed until re-registered".
- `expectation_bands_scale_by_the_rung_fraction` confirms every rung's expectation band loads and scales.

**Design decisions** (plan `docs/plans/2026-07-16-001-feat-production-ladder-plan.md`):
- **KD3** — structure now, values later; "Bands are scheduled per rung: each rung's band freezes from the preceding rung's data before the first session at the new rung … stated in size-normalized units."
- **KD6** — "Rung 1 runs without a tracking-error band … its sessions exist to calibrate the band that freezes before rung 2. Tracking error is computed and reported from session 1, but it is not load-bearing until rung 2."
- **R14(c)** — tracking-error band breach is a limit event only at rung 2+.
- **R14(e)** — cumulative P&L outside "its pre-registered expectation band derived from the backtest distribution" is a limit event; "operational cleanliness alone never authorizes escalation against a bleeding edge."
- **KTD9** makes the fail-closed asymmetry explicit: "a missing rung-2 band blocks a rung-2 dispatch but not a rung-1 one, per KD6."

## Related

- [`reconciled-run-can-falsify-an-approximate-per-bucket-ranking`](reconciled-run-can-falsify-an-approximate-per-bucket-ranking.md) — the same paper-fill-is-not-a-real-fill intuition: approximate paper P&L (limit-price × qty) is a directional read that a reconciled engine run can overturn. That is why an execution/slippage band has no backtest signal.
- [`report-preview-governance-band-must-anchor-on-deciders-run`](report-preview-governance-band-must-anchor-on-deciders-run.md) — a sibling band-provenance convention: a band's validity depends on *which* run/data it derives from (there, the decider's run; here, live vs backtest).
