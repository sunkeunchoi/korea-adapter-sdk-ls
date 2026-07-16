# Production Ladder — Pre-Registration (head v30)

**Status:** FROZEN before rung 1 · **Date:** 2026-07-16 · **Machine mirror:** [`preregistration.json`](preregistration.json)
**Governs:** the capital ladder shipped in PR #154 (`docs/plans/2026-07-16-001-feat-production-ladder-plan.md`).

This is the human pre-registration the plan's Dependencies section calls for. It freezes the
ladder's numbers **before** the first live rung and is amendable **only** by an explicit,
recorded re-registration dispatch (KTD1). Every value below is grounded in the head **v30**
backtest's closed-trade distribution — no invented numbers.

## Backtest source of truth

All economic values derive from the v30 head run:

```
data/turn4-fresh/runs/20260715T092847Z-backtest-orb-v30/performance.json
  strategy_version 30 · strategy_code_hash 6ae7b9f1… · risk_per_trade_krw 299,340 (full size)
```

Per-**session** distribution (23 distinct KST trading sessions, 2026-05-26 → 2026-07-03),
computed at full v30 size over `trades[].realized_pnl` grouped by KST trading date:

| statistic | value (KRW, full size) |
|---|---|
| per-session P&L: mean / median / σ | +236,343 / +260,000 / 741,389 |
| worst / best single session | −1,200,450 / +2,180,700 |
| session P&L p10 / p05 | −456,400 / −863,010 |
| **rolling 5-session cum P&L: worst / best** | **−689,900 / +3,555,800** |
| session-equity max drawdown | −1,592,450 |
| cumulative (23 sessions) / RoR | +5,435,900 / 0.1248 (head 0.1262) |

The rolling **5-session** window is the escalation denominator: N = 5 clean sessions escalate
a rung, and the expectation band checks cumulative P&L across exactly those 5 sessions.

## Frozen values and their derivation

### Dose-escalation ladder (KTD6 — budget-numerator fraction, never a param)

| rung | fraction | ≈ risk/trade | N clean | rationale |
|---|---|---|---|---|
| 1 | 0.10 | 29,934 | 5 | Minimum viable dose; a rung-1 bad session ≈ −120k (worst × 0.1). |
| 2 | 0.25 | 74,835 | 5 | ~2.5× step (FDA dose-escalation shape). |
| 3 | 0.50 | 149,670 | 5 | ~2× step. |
| 4 | 1.00 | 299,340 | 5 | Full v30 budget. |

Full ladder = **20 clean sessions minimum**. The fraction enters sizing as one dimensionless
multiplier on the budget numerator (`risk_per_trade_krw × rung_fraction × equity × tilt`),
so a rung move produces **zero** manifest/params diff — head identity stays stable (KTD6).

### Economic expectation bands (R14(e) — backtest-derived)

Method (**Protective**): `floor_r = (worst rolling-5 cum P&L) × fraction_r`,
`ceil_r = (best rolling-5 cum P&L) × 1.5 × fraction_r`, rounded to the nearest 1,000.
The floor is the "don't escalate against a bleeding edge" guard; the ceiling is a runaway
check. It blocks escalation only during a **worse-than-ever** 5-session streak — normal
variance still climbs.

| rung | floor = −689,900 × f | ceiling = 5,333,700 × f | frozen band |
|---|---|---|---|
| 1 | −68,990 | 533,370 | **[−69,000, 533,000]** |
| 2 | −172,475 | 1,333,425 | **[−172,000, 1,333,000]** |
| 3 | −344,950 | 2,666,850 | **[−345,000, 2,667,000]** |
| 4 | −689,900 | 5,333,700 | **[−690,000, 5,334,000]** |

### Tracking-error bands (R14(c) — SCHEDULED, not frozen here)

**Intentionally absent** from rungs 2–4. The v30 backtest fills at bar prices with zero
commission and **zero slippage**, so a slippage/approximated-fraction band **cannot** be
grounded in it. Per KD3/KD6 these are scheduled: each rung's tracking band freezes from the
**preceding rung's LIVE data** before the first session at that rung (rung-2 band from rung-1
data, …), via a re-registration dispatch. Rung 1 carries none by design — it is the
calibration rung. Consequence: `prereg.tracking_band(2)` is **fail-closed** until re-registered,
so a rung-2 dispatch is correctly blocked; rung 1 loads clean.

### Watchdog envelope (KTD10, U7)

| value | frozen | rationale |
|---|---|---|
| `heartbeat_interval_secs` | **90** | Generous dead-man window. Under KD5 a benign trip still de-escalates, and at rung 1 that means immediate rung-0 **suspension** — so favor few nuisance trips over fast stall detection. Still bounds a true stall to 1.5 min. |
| `session_max_loss_krw` | **300,000** | ~2.5× the worst historical **rung-1** session (−120k at 0.1×) — real protection for the sessions actually being run now. It is a single flat scalar the watchdog reads regardless of rung; **re-register a higher breaker before rungs 3–4** (same schedule idiom as the tracking bands), or it goes effectively inert at full size. |

### Readiness + exceedance (R10/R11, U9)

| value | frozen | rationale |
|---|---|---|
| `k_window` | 5 | Trailing live-lane window; matches N so a rung's evidence and its readiness window align. |
| `exceedance.max_reconcile_advised` | 1 | A single reconcile-advised across the window trends to a red (probation) verdict. |
| `exceedance.max_deferrals` | 3 | A few operator overrides tolerated before habitual deferral forces probation. |
| `exceedance.max_coverage_gaps` | 1 | Data-coverage gaps trend to probation quickly. |

A red readiness verdict forces **rung-1 probation** (effective_rung = 1), never a refusal (R11).

### Head-change rules (R13) & rung-0 re-qualification

- `params_change_reruns_n = true` — a governed-params change re-runs the current rung's N.
- `code_change_resets_to_rung_1 = true` — a strategy-code-hash change returns the ladder to rung 1.
- `rung0_requalification` — re-entry to rung 1 requires (1) the limit event's **root cause**
  written into the re-registration reason (elapsed time alone never qualifies; a benign
  watchdog trip still counts), (2) **≥ 3 clean paper sessions** through the gate since
  suspension, (3) attended re-registration with a fresh nonce. This is the program's stopping
  rule made re-openable only deliberately.

## Amendment protocol

1. **Rung-2+ tracking band** — before rung 2 opens, compute the slippage / approximated-fill
   distribution from the finalized rung-1 live sessions, choose a size-normalized band, and
   re-register (bump `version`, add `tracking_band` to the rung, record the reason).
2. **Breaker per rung** — before rung 3, re-register a `session_max_loss_krw` sized to the
   larger dose.
3. Any change is a recorded re-registration dispatch; the old file is archived content-hashed,
   never edited in place mid-epoch. Every dispatch cites the file hash it ran under.
