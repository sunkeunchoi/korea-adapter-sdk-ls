---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
date: 2026-07-09
status: requirements-only
---

# Turn 8 — ORB Exit Geometry (Diagnostic-Gated) - Plan

## Goal Capsule

- **Objective.** Lift the multi-session ORB backtest from a losing edge to a real,
  evaluable one by changing ORB's **exit geometry** in `orb.rs` — the residual that
  turns 6 & 7 localized after falsifying both entry-side param knobs. The mechanism is
  **chosen by a zero-cost diagnostic of the existing v8 artifacts, not picked blind.**
- **Baseline to beat.** v8 (`max_concurrent=7`, `range_minutes=20`): 140 trades,
  WR 42.9%, **expectancy −16,589 KRW/trade** over `data/turn4-fresh`,
  window `20260526..20260703`, 24 sessions. v6/v7 for reference: −7,946 / −16,330.
- **Product authority.** Attended, offline strategy-loop turn driven by the strategy
  researcher. One strategy-logic change; judged on **edge quality vs v8**, not a
  keep/revert on one knob (a code turn is outside param-turn governance — editing
  `orb.rs` bumps `strategy_code_hash`, which is the intended **re-baseline** signal,
  and the run version becomes **v9**).
- **Open blockers.** None to start. The routing threshold and all starter default
  param values are **provisional** (see Outstanding Questions) and are pinned during
  planning/execution once the diagnostic distribution is seen.

## Product Contract

### The problem, source-grounded

`orb.rs` has exactly two exits — `ExitReason::Stop` at the range low
(`orb.rs:327-331`) and `ExitReason::TimeFlat` at `flat_time=15:00`
(`orb.rs:307-314`); the `ExitReason` enum (`orb.rs:196-201`) has **no profit target
and no trailing stop**. Losers travel the full stop distance; winners are cut at the
bell. A 42.9% win rate that still loses ~16.6k/trade means **avg loss ≫ avg win**.

The frame carries a competing/compounding root cause the win-rate diagnostic cannot
see: the **fill model is pessimistic on both sides**. Entry fills at the breakout
bar's `high` (`orb.rs:324`), *above* the `range_high` trigger by the upper wick; the
stop fills at the breaching bar's `low` (`orb.rs:329`), *below* `range_low` by the
lower wick; `TimeFlat` fills at the 15:00 bar's `low` (`orb.rs:310`). Every trade is
bought at a bar-top and sold at a bar-bottom, which **manufactures avg_loss > avg_win
independent of exit geometry**. A profit target or trailing stop only reshapes the
*winner* side — if losses are wick-inflated ≫1R, no winner-side mechanism reaches
positive expectancy. **This fork must be settled before a mechanism is committed.**

### Desired outcome

A single `orb.rs` exit-logic change (run version **v9**) whose re-baselined
multi-session backtest shows **positive expectancy with dominance still capped** —
a real, advanceable edge — with the mechanism selected by data rather than guessed.

### Step 0 — the diagnostic gate (no code, no re-run)

Read and **join existing on-disk artifacts** from the v8 run under `data/turn4-fresh`
— no new run, no `orb.rs` change:

- `performance.json` — per-trade ledger (`TradeRecord`: `avg_px_open`,
  `avg_px_close`, `realized_pnl`, `ts_opened`, `ts_closed`, `symbol`).
- `decisions.jsonl` — telemetry envelopes carrying `range_high`, `range_low`,
  `breakout_price` (Breakout transition, `orb.rs:475-479`) and exit prices
  (`orb.rs:523-527`), keyed by symbol.

**Compute**, per closed trade, `R = range_high − range_low`, then:
`avg_loss_R`, `avg_win_R`, the trade counts in three buckets (stop-loser /
time-flat-winner / time-flat-loser), and the **wick-inflation share of losses**
— `((entry_fill − range_high) + (range_low − stop_fill)) / realized_loss`.

**Routing rule (provisional cut, lock in planning):**

- **Loser arm** — if `avg_loss_R` is large (≈ ≥1.5R) or the wick-inflation share is
  material (≈ ≥0.4): the pessimistic fills dominate the asymmetry → build **stop-fill
  realism**.
- **Winner arm** — otherwise (losses ≈1R): the deficit is that winners are too small
  / cut at 15:00 → grow the winner side. **Give-back sub-rule** picks the mechanism:
  compute how much time-flat winners give back from `session_high` to their exit; if
  winners **hold most of their run** → **trailing stop**; if they **peak early then
  fade** to a small win → **fixed target**.

The diagnostic writes its bucket table + the routed arm as a recorded finding (the
turn's evidence), regardless of outcome.

### The three contingent arms (one is built; all are specified)

All arms keep the `flat_time=15:00` flatten as a **hard backstop** and keep the
whipsaw same-bar enter+stop path intact. Any new `OrbParams` field threads through the
param set (`+ Default`), the run manifest (auto via `numeric_summary`'s serde), and the
analyze scaffold, and bumps the strategy to **v9**.

**Winner arm A — Trailing stop.**
New `ExitReason::TrailStop`; new params `trail_activate_r` (gain in R before the trail
arms; provisional default ~1.0) and `trail_giveback_r` (distance below the running
high-water the stop trails; provisional ~0.5). State tracks the post-entry high-water;
once `high ≥ entry + trail_activate_r·R`, the stop ratchets to
`high_water − trail_giveback_r·R` and **never lowers**. Operates within
`range_end..flat_time`; **holding past 15:00 is out of scope** (see Non-Goals).

**Winner arm B — Fixed profit target (N×R).**
New `ExitReason::Target`; new param `profit_target_r` (provisional ~2.0). When a bar's
`high ≥ entry + profit_target_r·R`, exit at the **target price** (a favorable limit
fill at `entry + profit_target_r·R`, not the bar wick). Reverts to `Stop`/`TimeFlat`
if the target is never reached.

**Loser arm — Stop-fill realism.**
No winner-side mechanism. Change the exit `limit_price` derivation so a stop fills at
the **stop level (`range_low`)** rather than the breaching bar's `low`, taking the
worse of `range_low` and the bar open **only on a true gap-through** (bar opened below
`range_low`); model entry fill at `range_high` (+ a small configurable slippage) rather
than the breakout bar's `high`. May add a `slippage_ticks`/`slippage_r` param. **Guard
against phantom edge:** this makes fills rosier, so the change must be defensible as
*more faithful to a resting limit/stop*, not merely *more optimistic*; the success bar
is read skeptically here.

### Success criteria (re-baseline vs v8)

Run v9 over `data/turn4-fresh`, window `20260526..20260703`, 24 sessions, **release
binary** (`cargo build --release -p nautilus-ls-lab --bin lab-research`; engine noise
is STDOUT → `>LOG 2>&1`).

- **PASS (edge advances):** `expectancy > 0` **AND** dominance capped
  (`max_abs_pnl_share ≤ 0.40`, non-degenerate) **AND** ≥1 closed trade.
- **Report alongside the verdict:** WR and `avgWin_R` / `avgLoss_R` vs v8, to confirm
  the built mechanism moved the *intended* quantity (winner arm should raise
  `avgWin_R`; loser arm should lower `avgLoss_R`).
- **Not a keep/revert.** A code turn is outside param governance: even if expectancy
  stays negative, the outcome is a recorded finding with the next lever named — not a
  turn failure. "Success" is nonetheless `expectancy > 0`.
- `runs compare` must PASS (version-diff enforced; the re-baseline is the expected
  signal for a hash-bumping turn).

### Test scenarios (orb.rs state-machine unit tests, per built arm)

Keep the `OrbState` machine unit-testable in isolation (fresh-engine-per-session gives
the per-day reset for free).

- **Trailing:** (1) winner runs past `activate_r` → trail arms, ratchets up, exits on
  giveback below the trailed stop (`TrailStop`); (2) winner never reaches `activate_r`
  → falls through to `Stop` or `TimeFlat`; (3) trail never lowers the stop; (4)
  `flat_time` backstop still fires when the trail never triggers; (5) whipsaw same-bar
  enter+stop unaffected.
- **Fixed target:** (1) `high` reaches target → `Target` exit at the target price; (2)
  approaches but misses target then reverts → `Stop`; (3) misses target, holds →
  `TimeFlat`; (4) whipsaw.
- **Stop-fill realism:** (1) stop fills at `range_low` when the bar low < `range_low`
  but the bar did not gap open below it; (2) gap-through — bar opens below `range_low`
  → fills at the worse (open); (3) entry fills at `range_high` (+slippage), not the
  breakout bar high; (4) whipsaw with the new fill prices.

### Non-goals / out of scope

- **Breakout-strength entry filter** (require the breakout bar to clear `range_high`
  by a min margin) — a separate *entry*-quality turn; one strategy-logic change per
  turn.
- **Partial scale-out** — more moving parts than a single-change turn warrants.
- **Holding past the 15:00 flat backstop / overnight exposure** — trailing respects
  `flat_time`; "let winners run past 15:00" would require a separate overnight-risk
  policy decision.
- **Anything live:** no gateway, no `LS_TRADING_ENV`, no re-ingest, no order placement.

### Outstanding questions (resolved in planning/execution)

- **Routing threshold.** The `avg_loss_R ≥ 1.5R` / wick-share `≥ 0.4` cuts are
  provisional; may become data-relative (e.g. median-based) once the diagnostic's
  actual distribution is seen.
- **Starter default param values.** `trail_activate_r`, `trail_giveback_r`,
  `profit_target_r`, and any slippage param are provisional starter defaults the loop
  exists to revise — never tuned claims.
- **Multi-session artifact shape.** Confirm whether `performance.json` /
  `decisions.jsonl` are one aggregate pair or per-session for the 24-session v8 run,
  and join per `(symbol, session)` accordingly (data is on disk either way).
