---
title: Amihud Liquidity Budget Tilt (CLASS B, liquidity axis) - Plan
type: feat
date: 2026-07-16
topic: amihud-liquidity-tilt
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Amihud Liquidity Budget Tilt (CLASS B, liquidity axis) - Plan

## Goal Capsule

- **Objective:** Run the next merit-bearing ORB strategy-loop turn through the governed
  command (PR #155): a **new dimensionless sizing axis** — an Amihud-illiquidity budget tilt
  — that down-weights the per-trade risk budget of *illiquid* breakouts. It is the first
  candidate after the CLASS B sizing family closed (head v30, RoR 0.1262), and it faces the
  frozen dual Phase-A gate that forces a genuinely new axis.
- **Product authority:** This document for scope; gate thresholds and the KEEP rule stay
  frozen per-candidate in `candidate.json`, not set here.
- **Execution home:** `adapters/nautilus/lab/` (Rust 1.96 standalone workspace); the Phase-A
  diagnostic + twin ride `uv run --with pyarrow python3`. `make adapter-check` is the gate.
- **Open blockers:** None. Offline and deterministic throughout — no gateway, credentials, or
  market window. A Phase-A GO turns this into a code turn (a new `orb.rs` sizing param); a STOP
  completes the turn at diagnose (the ATR-vol-target precedent).

## Product Contract

### Summary

The kept sizing levers reallocate risk on two axes: **stop distance** (`risk_per_share`, the
budget lever `risk_per_trade_krw`) and **relative volatility** (`prior_atr/entry_price`, the
kept ratio-ATR tilt `ratio_atr_alpha`). This candidate proposes a third, economically distinct
axis — **liquidity** — hypothesizing that illiquid breakouts realize worse P&L-per-unit-risk
(they gap through the entry-fixed stop and slip more), so down-weighting their risk budget
lifts the size-invariant return-on-risk exactly as the ratio-ATR tilt did.

### Problem Frame

A same-axis `ratio_atr_alpha` exponent sweep would be highly collinear with the kept tilt and
auto-STOP at the frozen collinearity gate; the entry/exit resurrection levers (RVOL,
entry-cutoff) don't produce the integer-qty materiality the sizing gate measures. The gate is
therefore engineered to admit only a genuinely new, dimensionless sizing axis orthogonal to the
two already levered. Liquidity is the strongest such economic candidate, and its orthogonality
to `risk_per_share` is genuinely uncertain (a raw-KRW turnover measure would re-introduce the
price-scale collinearity that STOPPED absolute ATR at r=0.96) — which is precisely what the
pre-code gate exists to measure before any lever code is written.

### Key Decisions

- **KD1 — Dimensionless Amihud axis, numerator-only tilt.** The axis is Amihud illiquidity
  `illiq = mean over prior 14 sessions of |ret_k| / turnover_k` (`turnover_k = close_k ·
  volume_k`, KRW). The tilt `w = clamp((illiq_ref/illiq)^alpha, w_lo, w_hi)` multiplies the
  risk **budget** only (never the `risk_per_share` denominator or the notional ceiling), so it
  cannot collapse into a duplicate of the stop-based lever (the anti-collapse rule the
  ratio-ATR tilt established). High illiq (illiquid) → `w < 1`; low illiq (liquid) → `w > 1`.
- **KD2 — Structural, not fitted, derivation.** `alpha = 1.0` (the structural inverse ratio);
  `illiq_ref = median(illiq)`, `w_lo = illiq_ref/p90(illiq)`, `w_hi = illiq_ref/p10(illiq)`,
  over the illiquidity-available closed-trade cohort (≥15 daily priors, matching the ATR
  cohort). Every value is derived from the untreated distribution, never chosen to move the
  result.
- **KD3 — The dual gate, plus a redundancy check.** Collinearity `|r(w, risk_per_share)| <
  0.70` (the primary gate, consistent with every prior sizing turn) **and** `|r(w,
  w_ratio_atr)| < 0.70` (the new tilt must not be collinear with the *kept* ratio-ATR tilt —
  the "axis the kept lever already sizes on" now includes the ratio-ATR weight). Materiality:
  predicted-RoR-shift `≥ 0.00065` AND integer-qty-change fraction `≥ 0.05`. A GO does not
  guarantee a KEEP.
- **KD4 — Governed, discipline-preserving.** The candidate is frozen + committed before the
  reading; the governed command runs the gate, and a GO advances to the code turn (new
  default-off `liquidity_tilt_alpha`, re-baseline + flip). No threshold is softened after the
  reading; the flip refuses without a matching committed GO.

### Requirements

- R1. A frozen `candidate.json` declaring the Amihud axis, the dual-gate thresholds, and the
  flip `liquidity_tilt_alpha = 1.0`, plus a bespoke diagnostic and an independent twin.
- R2. The diagnostic recomputes Amihud from the catalog daily OHLCV (mirroring the entry-safe
  prior-window discipline of `backtest.rs::prior_atr`) over the head v30 closed-trade cohort,
  and emits **absolute-value** collinearity readings so the `< 0.70` gate is correct on a
  signed statistic.
- R3. The twin recomputes every gated reading by an independent code path; diagnose STOPs on
  any disagreement beyond the frozen per-reading tolerance.
- R4. The turn runs through `turn governed` (Phase-A first); the machine verdict is recorded
  verbatim, the gate reading lands in `ledger/trials.jsonl`, and a TURN-LOG entry captures the
  outcome. On GO, a code turn builds and flips the lever; on STOP, the turn completes at
  diagnose.

### Success Criteria

- One governed turn runs end-to-end with zero hand-run diagnostic, comparison, or freshness
  steps; the verdict is echoed, never re-derived.
- The dual gate is frozen and twin-verified before the reading; no threshold moves after it.
- The gate reading appends to the TRIALS ledger and a TURN-LOG entry is written; `make
  adapter-check` is green; the turn is committed.

### Scope Boundaries

- **Not here:** tuning the exponent off 1.0 (a future sweep); alternative liquidity estimators
  (Kyle's lambda, bid-ask spread — the catalog has no quote data); the batch/cron KEEP-margin
  deflation (blocked until separately pre-registered).
- **Deferred to the turn:** whether Phase-A GOs (and thus whether the code turn runs) is the
  honest output of the gate, not a precondition of this plan.

### Dependencies / Assumptions

- The catalog daily bars carry `volume` (verified: fixed-point raw/1e9 = share count; daily
  turnover ~1e10 KRW), so Amihud is recomputable exactly as `prior_atr` is.
- Diagnostics ride `uv run --with pyarrow` (pyarrow absent from local python).
- Head v30 is the anchor (`data/turn4-fresh`, gitignored; `catalog_fingerprint` via
  `latest_finalized_run`).

### Outstanding Questions

- Whether Amihud is orthogonal to `risk_per_share` (the price/turnover channel could couple
  them) — resolved by the gate reading, not assumed.
- The deferred structural-attestation of the flip guard (freeze-on-flip / re-derive readings) —
  surfaced by this turn, not folded in (it is the plan's own deferred question).
</content>
</invoke>
