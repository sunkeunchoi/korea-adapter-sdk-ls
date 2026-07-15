---
title: "Gate a second sizing/normalizer lever on a pre-code collinearity check against the lever it competes with"
date: 2026-07-14
category: conventions
module: adapters/nautilus lab strategy loop (lab/src/strategy/orb.rs, lab/src/params.rs), CLASS B sizing levers
problem_type: convention
component: strategy-loop
severity: medium
applies_when:
  - "Proposing a SECOND lever that re-normalizes an already-normalized quantity (a new risk denominator, a new position-sizing basis, a second volatility/vol-target estimator)"
  - "The new lever's axis might duplicate the kept lever's axis (e.g. external ATR vs internal stop distance; both absolute-KRW and price-scale-dominated)"
  - "Deciding whether to build+run a CODE turn at all, or to stop at a cheap offline diagnostic"
  - "You want to avoid an INERT flip that only re-expresses the existing lever's reallocation"
tags:
  - strategy-loop
  - sizing-lever
  - collinearity
  - diagnostic-first-probe
  - inert-prediction
  - pre-registration
  - class-b
---

## Context

The strategy loop's kept CLASS B lever `risk_per_trade_krw` sizes each trade to a fixed
KRW risk budget against the **internal** stop distance `risk_per_share = entry − stop`
(≈ OR width under the range-low head). The natural next lever (plan `2026-07-14-002`,
the R6 re-rank) was ATR **volatility-target** sizing: replace the risk denominator with
an **external** ex-ante estimate (prior-daily ATR). It looked like a genuinely different
reallocation axis — but a second *normalizer* of the same underlying quantity is at high
risk of being **collinear** with the lever it competes against, in which case the flip
can only re-express the kept lever's de-risking → **INERT on the keep metric (RoR)**, and
the whole build/run is wasted.

The convention: when a proposed lever re-normalizes an already-normalized quantity,
**gate the entire build on a pre-code collinearity diagnostic** — measure how orthogonal
the new axis is to the existing one *before writing any lever code*, with a
pre-registered GO/NO-GO threshold. This is the repo's diagnostic-first-probe convention
(normally used against a *coverage* confound) applied to the **redundancy** risk unique
to a second normalizer.

## Guidance

1. **Recompute the new lever's per-trade axis offline over the current head run's closed
   trades**, reusing the *exact* production estimator so the reading is faithful. For the
   ATR turn this was `backtest.rs::prior_atr(daily, session_date, window)` — dedup daily
   bars to one per KST session, require ≥ `window+1` priors, `TR = max(h−l, |h−pc|,
   |l−pc|)`, ATR = mean of the last `window` TRs. Re-derive the kept lever's axis from the
   ledger (`risk_per_share = risk_capital / quantity`). Trades where the new estimator is
   `None` (fail-closed boundary) are excluded — in-strategy they take the fallback and
   carry no reallocation signal.

2. **Pre-register the orthogonality threshold and the GO/NO-GO rule in the PRE-REGISTER
   doc BEFORE reading the number.** For the ATR turn:
   `GO iff |Pearson r(new_axis, existing_axis)| < 0.70` (R² < 0.49 — the new axis shares
   less than half its variance with the existing one); `≥ 0.70` → predicted-INERT, STOP.
   Report Spearman ρ and top-quartile cohort overlap as color, but keep the gate on the
   single pre-registered statistic. Do **not** soften the threshold after seeing the value
   — that is the forbidden overfit.

3. **On a near-collinear reading, record predicted-INERT and stop — no lever code, no
   run.** This is a *complete, valid* turn outcome (the analogue of a CONFIRM's no-build
   stop), not a failure. Log the verdict in `TURN-LOG.md`, keep the current head, archive
   the diagnostic reproducer.

4. **Adversarially re-verify a load-bearing reading.** Because the reading alone decides
   the turn, cross-check the estimator port bit-for-bit against an independent recompute,
   and confirm the collinearity survives log-log Pearson (structural, not a scale
   artifact), an outlier-trimmed Pearson, and Spearman. If every variant clears the
   threshold, the reading stands.

## Why This Matters

A pure sizing/normalizer lever can only move a size-invariant keep metric like
`RoR = Σrealized_pnl / Σrisk_capital` by **reallocating risk across trades**. If the new
normalizer's axis is collinear with the axis the kept lever already reallocates on, it
shifts the *same* trades in the *same* direction — no independent reallocation, no RoR
movement. Spending a CODE turn (new param + helper + telemetry + re-baseline + flip +
bind analysis) to discover that is expensive; the offline correlation is minutes. The
gate turns a likely-wasted build into a recorded prediction.

The concrete finding from the ATR turn: **absolute-KRW prior-daily ATR is near-collinear
with the stop-based `risk_per_share`** — Pearson r = 0.96 (R² = 0.92), log-log 0.98,
Spearman 0.98, top-quartile cohort overlap 0.79 over v26's 103 ATR-available closed
trades — because both are absolute-KRW measures dominated by the same cross-sectional
price/volatility scale. Any absolute-KRW second normalizer inherits this collinearity.
The escape, if the family is revisited, is a **ratio form** (e.g. `ATR / price`) that
divides out the price-scale term and can reallocate on a genuinely independent axis.

## When to Apply

- Any proposed lever that introduces a *second* way to normalize a quantity the strategy
  already normalizes (risk denominator, position size, vol target, exposure weight).
- Especially when both axes are in the same absolute units (KRW, shares) and therefore
  likely share a dominant scale factor (price level, notional).
- Not needed for a lever that reallocates on a demonstrably independent signal
  (a time-of-day filter, a fundamentals gate) — there the coverage-confound diagnostic,
  not the collinearity gate, is the relevant pre-check.

## Examples

**ATR vol-target turn (plan 2026-07-14-002) — predicted-INERT, no build.**

```
# offline, over the head run's closed trades — BEFORE any lever code
risk_per_share_i = risk_capital_i / quantity_i          # kept lever's axis (stop-based)
atr_price_i      = prior_atr(daily, session_date_i, 14) # new lever's axis (external ATR)

Pearson r(atr_price, risk_per_share) = 0.9593  (R^2 = 0.9202)   # PRIMARY gate
Spearman rho                         = 0.9785                    # color
top-quartile cohort Jaccard overlap  = 0.7931                    # color

pre-registered rule: GO iff |r| < 0.70   ->   0.96 >= 0.70   ->   PREDICTED-INERT, STOP
```

Outcome: no `params.rs`/`orb.rs`/`performance.rs` edit, no v27/v28 runs; head v26 stays
(`strategy_code_hash d199d124`, RoR 0.1171). Verdict + reading recorded in
`adapters/nautilus/lab/TURN-LOG.md` and
`data/turn4-fresh/PRE-REGISTER-vNEXT-atr-vol-target.md`; reproducer archived under
`data/turn4-fresh/sizing-archive/u5-collinearity-diagnostic/`.

Contrast with the kept-lever turn (plan `2026-07-12-001`), where the lever's axis was the
stop distance itself — no *second* normalizer, so no collinearity gate applied, and the
flip legitimately moved RoR (see
`strategy-loop-reading-param-turn-outcomes-win-rate-vs-expectancy.md` for the RoR keep
crux).
