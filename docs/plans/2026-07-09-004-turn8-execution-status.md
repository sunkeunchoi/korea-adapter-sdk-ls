# Turn 8 · Execution Status — Recorded Finding

**Date:** 2026-07-10 · **Plan:** `2026-07-09-004-feat-orb-exit-geometry-diagnostic-gated-plan.md`
**Scope:** offline code turn (no gateway, no `LS_TRADING_ENV`, no re-ingest, no order placement).

## Verdict — NON-PASS (recorded finding, not a revert)

The fixed profit target **improved the edge dramatically but did not flip it positive**.
A code turn is outside param governance: a negative outcome is a recorded finding naming the
next lever, not a revert.

### Edge bar — v9 vs v8 (`data/turn4-fresh`, `20260526..20260703`, 24 sessions, release)

| metric | v8 (baseline) | v9 (fixed target 1.0R) | Δ |
|---|---|---|---|
| **Expectancy (KRW/trade)** | −16,589 | **−3,157** | **+13,432 (81% less negative)** |
| Win rate | 42.9% | **46.9%** | +4.0 pp |
| P&L total (KRW) | −2,289,350 | **−502,050** | +1,787,300 (78% smaller loss) |
| Profit factor | 0.85 | **0.97** | → toward break-even |
| Closed trades | 140 | 162 | +22 |
| Avg winner (KRW) | 267,197 | 255,854 | −11,343 |
| Avg loser (KRW) | −234,887 | −240,325 | −5,438 |
| `max_abs_pnl_share` | 0.123 | **0.127** | both ≤ 0.40 (capped, 40 syms) |

- **PASS condition** = `expectancy > 0` AND `max_abs_pnl_share ≤ 0.40` (non-degenerate) AND ≥1 trade.
- **Result:** dominance capped ✅ (0.127), trades ✅ (162), **expectancy −3,157 < 0 ✗** → **NOT PASS**.

**Why it improved yet stayed negative:** the target banks the give-back the Step-0 diagnostic
found (winners peaking ~1.46R then fading to 0.81R), cutting the loss 81% and raising WR +4pp —
but the *average winner fell slightly* (the target caps the occasional runner at 1.0R). The gain
came from **win frequency + avoided give-back**, not a bigger average winner. The crude what-if
(1.0R → +0.027R) over-counted because it could not see KTD2 stop-first precedence or same-bar
target-and-stop resolving to Stop — the conservative paths the plan deliberately failed toward.

### Next lever

**Param turn: sweep `profit_target_r` 1.0 → 1.5** (the Step-0 sim optimum; what-if `1.5R → +0.060R`
vs `1.0R → +0.027R`). The avg-winner drop under a 1.0R cap is the direct signal to let winners run
further — 1.5R should recover winner size while keeping most of the give-back protection. This is a
governed single-param turn (`LS_TURN_PARAM=profit_target_r LS_TURN_VALUE=1.5`), now unblocked because
the code (and `profit_target_r`) has landed. The new per-trade `mfe_r` telemetry lets that turn read
give-back directly instead of reconstructing it.

## What landed (U1–U3)

- **U1** `src/params.rs` — `OrbParams::profit_target_r: f64`, `#[serde(default = "default_profit_target_r")]`
  (1.0), in `numeric_summary`. Back-compat: v8-era manifests without the key deserialize to 1.0.
- **U2** `src/strategy/orb.rs` — `ExitReason::Target`; `OrbState` `entry_price` + post-entry `high_water`;
  target branch evaluated **after** the stop (stop-first precedence, R4/KTD2); favorable-limit fill
  `entry_price + round(profit_target_r · R)`; `mfe_r()`/`entry_price()` accessors. Whipsaw / stop /
  time-flat paths byte-unchanged (the entry bar can never trip the target).
- **U3** `src/agent/envelope.rs` + `orb.rs` — `SignalKind::Target` (wire `"target"`); `handle_actions`
  maps `ExitReason::Target → SignalKind::Target` and attaches `mfe_r` to **every** exit envelope.

## Verification Contract — all steps executed

1. **Unit gate** — `cargo test -p nautilus-ls-lab`: **190 passed**.
2. **Workspace gate** — `cargo test --workspace`: **490 passed**.
3. **Re-baseline run** — release `lab-research`; v9 = `20260710T013757Z-backtest-orb-v9`,
   `strategy_version = 9`, `profit_target_r = 1.0`, over `data/turn4-fresh` (catalog **GO**, 40 syms).
   Seeded a v9 param-authority manifest then reran (a code turn is outside the governed `turn()`
   path — mirrors prior fresh-home seeding); the seed was removed after finalizing.
4. **Edge bar** — recorded above: NON-PASS on expectancy; dominance capped; +81% expectancy vs v8.
5. **`runs compare` (param mode, v8 → v9)** — **FAIL** as intended (the re-baseline signal):
   ```
   param diff: ["strategy_version"]
   FAIL: param diff must be exactly {strategy_version, one param}, got ["strategy_version"]
   FAIL: strategy_code_hash differs
   verdict: FAIL
   ```
   `strategy_code_hash` moved `3389039272173d74…` → `d54955a8aacf35d9…` (hashes `include_str!("orb.rs")`).
   `profit_target_r`'s default (1.0 == the v9 run value) keeps it out of `param_diff` until the 1.5 sweep,
   exactly as KTD5 notes. No `runs compare` mode PASSes a code turn — the verdict rests on the edge bar.
6. **Offline only** — no gateway, no `LS_TRADING_ENV`, no re-ingest, no order placement.
