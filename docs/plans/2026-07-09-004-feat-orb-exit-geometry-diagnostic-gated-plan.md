---
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
date: 2026-07-09
enriched: 2026-07-10
status: implementation-ready
---

# Turn 8 — ORB Fixed Profit Target (Diagnostic-Routed) - Plan

**Target repo:** this repo · all paths under `adapters/nautilus/lab/`.

## Goal Capsule

- **Objective.** Lift the multi-session ORB backtest from a losing edge to a real,
  evaluable one by adding a **fixed profit target** to `orb.rs` — the mechanism the
  Step 0 diagnostic selected after falsifying both the fill-model and the
  "avg loss ≫ avg win" hypotheses.
- **Baseline to beat.** v8 (`max_concurrent=7`, `range_minutes=20`): 140 trades,
  WR 42.9%, **expectancy −16,589 KRW/trade** over `data/turn4-fresh`,
  window `20260526..20260703`, 24 sessions.
- **Why a fixed target (diagnostic-grounded).** Winners peak at **MFE 1.46R** then fade
  to **0.81R** by the 15:00 bell (give-back/MFE median 0.508 — peak-then-fade, not a
  sustained trend). Stop-losers cost a clean **1.006R** (wick-inflation share −0.066), so
  the loss side is healthy and no fill fix is warranted. A target that banks ~1.0–1.5R
  flips expectancy positive in the what-if (target 1.0R → +0.027R, 1.5R → +0.060R).
  Full evidence: `docs/plans/2026-07-09-004-turn8-step0-diagnostic-finding.md`.
- **Product authority.** Attended, offline strategy-loop turn. This is a **code turn**:
  editing `orb.rs` bumps `strategy_code_hash` — the intended **re-baseline** signal — so
  the turn is judged on **edge quality vs v8**, not a keep/revert on one knob.
- **Product Contract preservation.** Changed: the three contingent arms collapse to the
  one the diagnostic routed to (fixed profit target). The loser/stop-fill-realism and
  trailing arms are dropped per the recorded finding — a routing outcome, not a
  scope change.

---

## Product Contract

### The problem (as re-grounded by Step 0)

`orb.rs` exits are `ExitReason::Stop` at the range low (orb.rs:327-331) and
`ExitReason::TimeFlat` at 15:00 (orb.rs:307-314); the enum (orb.rs:196-201) has **no
profit target**. The diagnostic overturned the original framing: in R-multiples
avg_win_R (0.809) *exceeds* avg_loss_R (0.651), and stops are a clean 1R — so the fill
model is fine and the loss side is fine. The defect is narrow: **60 time-flat winners
are cut at 0.81R by the bell after peaking near 1.46R.** A fixed target locks that move.

### Desired outcome

A single `orb.rs` exit-logic addition (run version **v9**) whose re-baselined
multi-session backtest shows **positive expectancy with dominance still capped** — a
real, advanceable edge — plus per-trade MFE telemetry so the next exit-tuning turn reads
give-back directly instead of reconstructing it.

### Requirements

- **R1.** Add a fixed profit-target exit: while Long, exit when a bar's `high ≥ entry_price + profit_target_r · R`, where `R = range_high − range_low` and `entry_price` is the breakout fill. Fill at the **target price** `entry_price + profit_target_r · R` (a favorable limit), not the bar wick.
- **R2.** `profit_target_r` is an `OrbParams` field (provisional default **1.0**; 1.5 is the sim optimum reserved for a later param-turn sweep), manifest-recorded via `numeric_summary`, and **back-compat**: prior manifests lacking the field must still deserialize.
- **R3.** Preserve the range-low `Stop` and the 15:00 `TimeFlat` hard backstop unchanged; preserve the whipsaw same-bar enter+stop path.
- **R4.** Same-bar target-and-stop precedence resolves to **Stop** (pessimistic — intrabar order is unknowable; consistent with KTD6's pessimistic fills).
- **R5.** Emit per-trade **MFE in R** (`mfe_r`, post-entry high-water) on every exit telemetry envelope, carrying a real per-trade value (not the ts=0 session extreme).
- **R6.** The run is labeled **v9** (manifest `strategy_version = 9` + bumped `strategy_code_hash`). Because this is a **code** turn, `runs compare` in **param mode is EXPECTED to FAIL** on the `strategy_code_hash` change — that failure *is* the re-baseline signal (no compare mode PASSes a code turn); the turn is judged on the edge bar, not a green compare.

### Success criteria (re-baseline vs v8)

Run v9 over `data/turn4-fresh`, window `20260526..20260703`, 24 sessions, **release
binary**. **PASS** = `expectancy > 0` AND dominance capped (`max_abs_pnl_share ≤ 0.40`,
non-degenerate) AND ≥1 closed trade. Report WR + avgWin_R/avgLoss_R vs v8 to confirm the
target raised the winner side. A code turn is outside param governance — a negative
outcome is a recorded finding with the next lever named, not a revert; "success" is
still `expectancy > 0`.

### Scope Boundaries

**In scope:** fixed profit target in the ORB state machine + its param, telemetry, and
tests; the v9 re-baseline run.

**Deferred to Follow-Up Work:**
- Sweeping `profit_target_r` (1.0 → 1.5, the sim optimum) — a later **param** turn once the code lands.

**Outside this turn (diagnostic said no / one-change-per-turn):**
- Trailing-stop and stop-fill-realism arms (the diagnostic falsified their premises).
- Breakout-strength entry filter; partial scale-out; holding past the 15:00 backstop.
- Anything live: no gateway, no `LS_TRADING_ENV`, no re-ingest, no order placement.

---

## Planning Contract

### Key Technical Decisions

- **KTD1 — Target reference is the breakout fill.** `entry_price` = the breakout bar's high (the emitted Enter limit, orb.rs:324). Target level = `entry_price + round(profit_target_r · R)`. This is what the strategy actually paid, so the target is a true R-multiple of realized entry.
- **KTD2 — Stop-first precedence (R4).** In `on_bar`, for a held Long, evaluate the stop before the target. When both trigger on one bar, Stop wins. Rationale: matches the codebase's pessimistic fill philosophy and avoids optimistically banking a target that may not have filled first intrabar. This is the one path the crude what-if could *not* see, so fail it toward the conservative side.
- **KTD3 — `serde(default)` for back-compat (R2).** `profit_target_r` gets `#[serde(default = "default_profit_target_r")]` so the existing v8 manifest (no such key) still deserializes when a run resolves params from the latest finalized manifest (research.rs:226). Without it, every run in `data/turn4-fresh` breaks.
- **KTD4 — MFE rides the exit envelope (R5).** Add `mfe_r` to the exit telemetry `values` map via a new `OrbState::mfe_r()` accessor, rather than a new `SignalKind`. `OrbState` gains `entry_price` and post-entry `high_water` (updated each Long bar) — both already required for the target, so MFE is nearly free. `mfe_r = (high_water − entry_price) / R`.
- **KTD5 — v9 labeling / re-baseline.** The v9 run executes from a manifest carrying `profit_target_r` and `strategy_version = 9`. The `orb.rs` edit bumps `strategy_code_hash`, and **param-mode `runs compare` hard-fails on a code-hash change** (research.rs) — that FAIL is the intended re-baseline signal, exactly turn-5's pattern; **no `runs compare` mode PASSes a code turn**, so the verdict rests on the edge bar. Note: because `profit_target_r`'s default (1.0) equals the v9 run value, `param_diff` would surface only `{strategy_version}` even absent the code-hash change — the field stays invisible to param-mode compare until the later 1.5 sweep changes it. The manifest-seed step mirrors prior fresh-home seeding (memory: turn-4 seeded a v3 manifest) — see Deferred.

### High-Level Technical Design

`OrbState::on_bar` decision order for a bar at/after `range_end` and before `flat_time`,
once Long (new `Target` branch shown ▸):

```
t >= flat_time ............... Exit TimeFlat @ bar low   (unchanged backstop)
else, in trading window:
  Armed & high > range_high .. Enter @ high (=entry_price); high_water := high
  Long:
    high_water := max(high_water, high)          ▸ track post-entry peak (MFE)
    low <= range_low ........... Exit Stop @ bar low          (evaluated FIRST — KTD2)
  ▸ else high >= entry_price + target_r*R
                              .. Exit Target @ entry_price + target_r*R   (favorable limit)
```

Entry bar can never hit the target (`target = entry_price + target_r·R > high = entry_price`),
so the whipsaw enter+stop path is byte-unchanged.

### Implementation Units

### U1. Add `profit_target_r` to `OrbParams`

- **Goal:** Introduce the target size param with a back-compat default (R2).
- **Requirements:** R2.
- **Dependencies:** none.
- **Files:** `src/params.rs` (modify; struct + `Default` + a `default_profit_target_r()` fn + tests in the existing `#[cfg(test)] mod tests`).
- **Approach:** Add `pub profit_target_r: f64` with `#[serde(default = "default_profit_target_r")]`; `Default` sets 1.0; `default_profit_target_r()` returns 1.0. `numeric_summary()` picks it up automatically (serde value walk) — verify it appears. Document 1.5 as the sim optimum in the field doc-comment.
- **Patterns to follow:** the existing numeric fields (`gap_min_pct`, `notional_per_position`) and the `hhmmss` serde helper module already in `src/params.rs`.
- **Test scenarios:**
  - Default value is 1.0 (`defaults_match_ktd6`-style assertion).
  - Round-trips through JSON with the key present (extend `params_round_trip_through_json`).
  - **Back-compat:** a manifest JSON string *without* `profit_target_r` deserializes into `OrbParams` with the field = 1.0 (Covers R2).
  - `numeric_summary()` contains `profit_target_r`.
- **Verification:** `cargo test -p nautilus-ls-lab params` green; the field is present in a serialized manifest.

### U2. Fixed-target exit in the `OrbState` state machine

- **Goal:** Add `ExitReason::Target`, post-entry tracking, target logic, stop-first precedence, and the MFE accessor (R1, R3, R4, R5). **This unit bumps `strategy_code_hash` → the v9 re-baseline signal.**
- **Requirements:** R1, R3, R4, R5, R6 (this unit's `orb.rs` edit is what bumps `strategy_code_hash` → the v9 re-baseline).
- **Dependencies:** U1.
- **Files:** `src/strategy/orb.rs` (modify: `ExitReason`, `OrbState` fields + `on_bar` + new `mfe_r()`/`entry_price` accessors; extend the existing `#[cfg(test)] mod tests`).
- **Approach:** Add `ExitReason::Target` (orb.rs:196-201). Add `OrbState` fields `entry_price: i64` and `high_water: i64`, set at Enter (`entry_price = high`, `high_water = high`). Each Long bar: `high_water = high_water.max(high)`. Insert the target branch **after** the stop check (KTD2): `if self.phase == Long && high >= entry_price + (profit_target_r * R).round() → Exit { limit_price: entry_price + round(target_r*R), reason: Target }; phase = Done`. `R = range_high − range_low`. Add `pub fn mfe_r(&self) -> f64` = `(high_water − entry_price) as f64 / R` (guard R>0; returns 0.0 pre-entry). Keep Stop, TimeFlat, entry, and whipsaw paths unchanged.
- **Execution note:** Test-first — add the six scenarios below and watch the target cases (1, 5, 6) fail before wiring the target branch; the stop/time-flat/whipsaw cases (2, 3, 4) are regression guards that must stay green.
- **Patterns to follow:** the existing `on_bar` action-vec construction and the `range()`/`session_extremes()` accessor style in `src/strategy/orb.rs`.
- **Test scenarios:**
  - **(1) Target hit:** Long, a later bar's `high` reaches `entry_price + target_r·R` → one `Exit{reason: Target, limit_price = entry_price + round(target_r·R)}`; phase → Done.
  - **(2) Approach-then-revert:** high nears but misses target, later `low ≤ range_low` → `Exit{Stop}`, never Target.
  - **(3) Miss-and-hold:** target never reached, position held to 15:00 → `Exit{TimeFlat}`.
  - **(4) Whipsaw:** breakout bar also breaches range_low → same-bar `Enter` + `Exit{Stop}`, no Target (regression — path unchanged).
  - **(5) Same-bar target+stop:** one Long bar with `high ≥ target` AND `low ≤ range_low` → `Exit{Stop}` (Covers R4 precedence), not Target.
  - **(6) MFE:** after a run to `high_water`, `mfe_r()` = `(high_water − entry_price)/R` (Covers R5).
- **Verification:** `cargo test -p nautilus-ls-lab orb` (or the state-machine module) green; scenarios 1–6 pass; scenarios 2–4 confirm no regression.

### U3. Thread the Target exit + MFE through the strategy wrapper & envelope

- **Goal:** Surface `ExitReason::Target` as telemetry and attach `mfe_r` to every exit envelope (R1, R5).
- **Requirements:** R1, R5.
- **Dependencies:** U2.
- **Files:** `src/agent/envelope.rs` (modify: add `SignalKind::Target`), `src/strategy/orb.rs` (modify: `handle_actions` exit arm — map `ExitReason::Target → SignalKind::Target`, add `mfe_r` to the exit `values`).
- **Approach:** Add `Target` to `SignalKind` (envelope.rs:34) — snake_case serde renders it `"target"`, matching the existing `stop_hit`/`time_exit` wire names (verify the enum's `rename_all`). In `handle_actions` (orb.rs:511-529), extend the `reason` match with `ExitReason::Target => SignalKind::Target`, and add `("mfe_r", self.states.get(&id).map(|s| s.mfe_r()).unwrap_or(0.0))` to the exit transition `values` for all exit kinds (Stop, TimeFlat, Target).
- **Test scenarios:**
  - A Target exit emits an envelope whose `decision_detail.kind == "target"` and whose `values` include `mfe_r`, `qty`, `price`.
  - Stop and TimeFlat exit envelopes also carry `mfe_r` (Covers R5).
  - `Test expectation:` extend an existing strategy/wrapper test that already asserts exit telemetry, rather than a new harness, if one exists (`src/strategy/orb.rs` tests or `tests/strategy.rs`).
- **Verification:** `cargo test -p nautilus-ls-lab` green; a target-exit telemetry line is emitted with `mfe_r`.

---

## Verification Contract

1. **Unit gate.** `cargo test -p nautilus-ls-lab` green (U1–U3 scenarios).
2. **Workspace gate.** `cargo test --workspace` green (adapter tests need `--workspace`).
3. **Re-baseline run (the edge verdict).** Build release
   (`cargo build --release -p nautilus-ls-lab --bin lab-research`; ~3.5 min/run; engine
   noise is STDOUT → `>LOG 2>&1`). Produce the **v9** run over `data/turn4-fresh`,
   window `20260526..20260703`, 24 sessions, from a manifest carrying `profit_target_r`
   and `strategy_version = 9`. Catalog is GO (40 syms).
4. **Edge bar.** `expectancy > 0` AND `max_abs_pnl_share ≤ 0.40` (non-degenerate) AND
   ≥1 closed trade, vs v8 (−16,589). Record WR + avgWin_R/avgLoss_R vs v8.
5. **`runs compare`** — for this **code** turn the param-mode compare **FAILs on the `strategy_code_hash` change**; that FAIL is the expected re-baseline signal, not a gate to turn green. Capture its output as re-baseline evidence. (No compare mode PASSes a code turn.)
6. Offline only — no gateway, no `LS_TRADING_ENV`, no re-ingest, no order placement.

## Definition of Done

- U1–U3 landed; `profit_target_r` default 1.0 threaded through params → manifest → analyze scaffold.
- `orb.rs` hash bumped; v9 run executed; edge bar evaluated and **recorded** (PASS flips expectancy positive; a non-PASS is a recorded finding naming the next lever, e.g. sweep `profit_target_r` to 1.5).
- Full gate green (`cargo test --workspace`). The `runs compare` param-mode FAIL on the code-hash change is captured as the re-baseline signal (a code turn has no green compare). No live surface touched.

---

## Deferred to Implementation

- **v9 manifest seed (KTD5).** The exact command to produce a v9 manifest carrying
  `profit_target_r` + `strategy_version=9` — a code turn is outside the governed `turn()`
  param path, so this mirrors prior fresh-home manifest seeding rather than a governed
  single-param flip. Resolve against the current `runner/research.rs` turn/seed surface at
  execution time.
- **Target-price tick alignment.** U2's approach is decided — `round(profit_target_r·R)` on
  integer-KRW `i64` prices. Left open: confirm this matches how existing `orb.rs` prices are
  handled and that no exchange tick-size/lot constraint needs coarser alignment (no sub-won
  artifact). This is alignment granularity only, not whether to round.
- **Target fill timing.** Whether the nautilus 0.60 backtest fills a reduce-only Sell LIMIT at
  `entry_price + target_r·R` on the *same* bar whose high reached it (vs a later bar) is an
  engine-timing detail the plan does not pin; it mirrors the existing marketable-limit exits so
  likely fine, but the realized target-fill rate is only known once the run executes.
- **`analyze --scaffold`** picks params from the manifest's `numeric_summary` automatically
  (research.rs:986) — confirm `profit_target_r` renders in the scaffold without extra wiring.
