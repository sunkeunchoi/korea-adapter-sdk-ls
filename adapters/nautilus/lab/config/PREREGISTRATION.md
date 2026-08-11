# Production Ladder — Pre-Registration

**Status:** RE-REGISTERED to **v2** (head v34) · **LADDER STOOD DOWN 2026-07-31** (recorded suspension — see § Stand-down below; v2 values unchanged) · **RE-ENTRY CONDITION TIGHTENED 2026-08-06** — the unblock is no longer `net RoR > 0` but the pre-registered [sample margin](SAMPLE-MARGIN.md) · **LINEAGE CLOSED 2026-08-10 (declared 2026-08-11)** — the ORB lineage this ladder served is CLOSED under the pre-registered Lineage-closure rule; rule evaluation and admissibility basis in the TURN-LOG 2026-08-11 closure-declaration entry (all frozen values below unchanged) · **Date:** 2026-07-16 (v1, head v30) → 2026-07-24 (v2, head v34)
**Machine mirror:** [`preregistration.json`](preregistration.json)
**Governs:** the capital ladder shipped in PR #154 (`docs/plans/2026-07-16-001-feat-production-ladder-plan.md`).

This is the human pre-registration the plan's Dependencies section calls for. It freezes the
ladder's numbers **before** the first live rung and is amendable **only** by an explicit,
recorded re-registration dispatch (KTD1). Every value below is a backtest-derived figure —
no invented numbers. The **current, load-bearing** economics are the **v2 (head v34)** section
immediately below; the original **v1 (head v30)** derivation is retained beneath it as the
historical record of the frozen-then-superseded values.

---

## Stand-down — 2026-07-31 (recorded suspension; v2 values unchanged)

**The ladder is suspended.** The documented head moved v34 → v35 (`strategy_code_hash
e5bc2ae8… → 7571abef…`, run `20260731T023138Z-backtest-orb-v35`) — a
`code_change_resets_to_rung_1` event — and the v35 re-measurement with the sourced
transaction-cost model armed reads **net-negative** (net RoR **−0.0006**, 111 closed
trades; TURN-LOG 2026-07-31). The v2 rung-1 expectation band below derives from the v34
**zero-cost** distribution, so the economic gate cites a band the head can no longer clear
in expectation.

**Disposition: stand-down, not amendment.** No v3 band is derived. A band centered on a
negative edge would exist only to authorize sessions this suspension forbids; rung-2 can
never be authorized on a net-negative head, so rung-1's calibration output has no
consumer; and the head change a net-positive edge requires resets to rung 1
(`code_change_resets_to_rung_1`), discarding v35-epoch rung-1 evidence anyway. The v2
values below are retained **unchanged** as the historical record, and
[`preregistration.json`](preregistration.json) is deliberately untouched — every existing
dispatch citation (SHA-256) remains valid.

**Re-entry protocol:** a candidate head that **clears the pre-registered
[sample margin](SAMPLE-MARGIN.md)** triggers a fresh re-registration (v3+): bands
re-derived from that head's closed-trade distribution via the identical Protective
formula, reproduced in `lab/tests/prereg_derivation.rs`, before any genesis dispatch.
Parked as queue `rung1-ladder-reentry-margin-clearing-head`.

> **AMENDED 2026-08-06 (sample-sufficiency turn, plan 2026-08-05-001; TURN-LOG 2026-08-06).**
> The re-entry condition above used to read *"net RoR > 0 with the armed cost model on a
> current catalog"*. That condition is **satisfiable by luck**: on the v35 sample the
> session-block bootstrap puts the share of null replicates above zero at 0.4955, so a
> coin-flip head meets it about half the time. It is replaced by the frozen margin —
> `net RoR > E[max of 29 null trials] + z(95%) · SE(candidate)` — which corrects for the
> arms already evaluated against this data and scales its sampling term to the candidate's
> own sample. This tightens the gate only; it derives no new band, and
> [`preregistration.json`](preregistration.json) stays byte-identical (a test pins its
> SHA-256). The margin's own package is `sample-margin.json`, deliberately **not** this
> file: the no-consumer test forbids re-deriving a frozen artifact whose honest value would
> forbid the activity it gates.
>
> Note what the same turn established about the sample: the head's gross edge sits about
> nine times below this sample's detection floor, and ~8,600 closed trades (~3,499 calendar
> sessions, ~14 years) would be needed to resolve it, against 54 covered. Re-entry is therefore
> **not** expected to arrive by a lever search on the current catalog.

---

## Re-registration v2 (head v34)

**Reason for re-registration:** the strategy code hash changed from v30 (`6ae7b9f1…`) to
v34 (`d7a9820b…`). Under the frozen `head_change` rules that is a
`code_change_resets_to_rung_1` event, so the economic expectation bands are re-derived from
the head **v34** backtest — the same discipline (backtest-derived, zero-slippage, identical
Protective formula) v1's original freeze used. This is a **legitimate re-registration, not
band-fitting to live data**: no live chain exists yet for the v34 epoch, and the derivation
inputs are a backtest, not realized fills. The genesis dispatch for the v34 epoch cites this
file's content hash.

### Backtest source of truth

```
data/turn4-fresh/runs/20260724T014752Z-backtest-orb-v34/performance.json
  strategy_version 34 · strategy_code_hash d7a9820b… · risk_per_trade_krw 299,340 (full size)
```

Per-**session** distribution (**24** distinct KST trading sessions), computed at full v34 size
over `trades[].realized_pnl` grouped by KST trading date. The session count is recorded here
so the rolling-5 constants' fragility is auditable — a worst rolling-5 window over 24 sessions
is a high-variance tail order-statistic:

| statistic | value (KRW, full size) |
|---|---|
| per-session P&L: mean / median | +49,292 / +35,000 |
| worst single session | **−1,360,330** |
| **rolling 5-session cum P&L: worst / best** (over 24 sessions) | **−1,483,240 / +1,772,900** |
| cumulative (24 sessions) / RoR | +1,183,010 / 0.0398 |

The rolling **5-session** window is the escalation denominator (N = 5 clean sessions escalate a
rung; the expectation band checks cumulative P&L across exactly those 5 sessions).

### Economic expectation bands (v34 — Protective formula, re-frozen)

Method (**Protective**, unchanged from v1): `floor_r = (worst rolling-5 cum P&L) × fraction_r`,
`ceil_r = (best rolling-5 cum P&L) × 1.5 × fraction_r`, rounded to the nearest 1,000.
Reproduced by `lab/tests/prereg_derivation.rs` from the rolling-5 constants above.

| rung | fraction | floor = −1,483,240 × f | ceiling = 1,772,900 × 1.5 × f | frozen band (v34) |
|---|---|---|---|---|
| 1 | 0.10 | −148,324 | 265,935 | **[−148,000, +266,000]** |
| 2 | 0.25 | −370,810 | 664,838 | **[−371,000, +665,000]** |
| 3 | 0.50 | −741,620 | 1,329,675 | **[−742,000, +1,330,000]** |
| 4 | 1.00 | −1,483,240 | 2,659,350 | **[−1,483,000, +2,659,000]** |

### Notes on the v34 re-freeze

- **The floor loosens ~2× (−69k → −148k).** v34's worse-and-wider distribution means a
  normal-variance rung-1 five-session streak (down to −148k at 0.10×) no longer false-breaches
  the floor. This is not masking a worse edge: v34's RoR (0.0398) is genuinely lower than v30's
  (0.1248), the two are **not directly comparable** (different real universes), and the band is a
  runaway/bleeding-edge guard, not the edge itself. Rung 1 is the calibration rung — its LIVE
  data (not this backtest band) sets the rung-2 tracking band.
- **The ceiling HALVES (+533k → +266k) — a symmetric effect.** A strongly-profitable rung-1
  streak (cumulative > +266k) now also blocks escalation as "outside band / runaway." This is
  intended: escalation should pause and re-inspect on an anomalously good streak as much as a bad
  one.
- **Zero-slippage backtest vs live-P&L gate (conservative / floor-biased).** `verify_escalation`
  sums **live, slippage-laden** realized P&L against a band derived from a **zero-slippage**
  backtest, so live rung-1 cumulative P&L will sit systematically below the band once fills/fees/
  slippage are real — biasing the rung-1→2 gate toward false floor breaches. This is a
  deliberately conservative gate; the rung-2 decision may need a live-cost allowance (the same
  live-grounding the tracking bands already get). Not a launch risk — the band binds only at the
  session-5 escalation decision, never at per-session clean/limit classification or launch.

### Breaker (`session_max_loss_krw`) — stands at 300,000 for rung 1 (KTD3)

Kept unchanged. **Target headroom:** 300,000 is ~2.2× v34's worst rung-1 session
(−1,360,330 worst full-size session × 0.10 = −136,033), so it remains real protection for the
sessions actually being run now. It is a single flat scalar the watchdog reads regardless of
rung; **re-register a larger breaker before rungs 3–4** (same scheduled idiom as the tracking
bands), or it goes effectively inert at full size. The cushion rests on a *zero-slippage* worst
session, so live give-back could erode it — a second reason the rung-3 re-derivation is scheduled.

---

## v1 (head v30) — historical record (superseded by v2 above)

The values below froze the ladder before the v30 epoch and are retained as history. The
economic expectation bands here are **superseded** by the v2 (head v34) section above; the
Protective method, the watchdog/readiness/exceedance/head-change frozen values, and the
amendment protocol are unchanged and still current. Every value below was grounded in the head
**v30** backtest's closed-trade distribution — no invented numbers.

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
