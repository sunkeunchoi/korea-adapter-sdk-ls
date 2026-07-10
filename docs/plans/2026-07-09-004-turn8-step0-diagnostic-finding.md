# Turn 8 · Step 0 Diagnostic — Recorded Finding

**Date:** 2026-07-09 · **Gate for:** `2026-07-09-004-feat-orb-exit-geometry-diagnostic-gated-plan.md`
**Source (gitignored):** `data/turn4-fresh/runs/20260709T124752Z-backtest-orb-v8/`
(one aggregate `performance.json` + `decisions.jsonl` per version — **not** per-session dirs;
joined per `(symbol, KST-session-date)`). Zero code, no re-run.

## Verdict

**Route → WINNER ARM → FIXED PROFIT TARGET (provisional `profit_target_r = 1.0`, sim-optimum ~1.5R).**

The wick/fill hypothesis and the "avg loss ≫ avg win" framing are **both falsified**. The
edge is only marginally negative (−0.016 R/trade); the loss on the table is that
**time-flat winners peak at ~1.46R then fade to 0.81R by the 15:00 bell** — a payoff-capture
problem, not a fill or entry problem.

## Bucket table (140 closed trades · WR 42.9% · exp −16,589 KRW = −0.016 R/trade)

| Bucket | n | avg R |
|---|---|---|
| Time-flat winners | 60 | **+0.809** (captured at exit; MFE +1.456) |
| Stop-losers | 33 | **−1.006** |
| Time-flat losers | 45 | −0.390 |
| Scratch (pnl 0) | 2 | 0 |
| avg_win_R (all winners) | 60 | +0.809 |
| avg_loss_R (all losers) | 78 | −0.651 |

`R = range_high − range_low`; R-multiple = `(exit_fill − entry_fill) / R`.

## Falsification 1 — the fill model is NOT the culprit

- **Stop-losers cost 1.006R** — almost exactly one range-height.
- **Wick-inflation share of stop losses = −0.066** (≈0): entry overshoots `range_high` by
  only **0.018R**, stops fill essentially **at** `range_low` (exit slip −0.012R).
- 1-minute bars on these liquid KRX names have negligible wicks vs the ~2% range, so
  "buy the bar-high / sell the bar-low" pessimism is immaterial. **No loser-arm work is warranted.**

Routing rule check: `avg_loss_R(stop) 1.006 < 1.5` and `wick-share −0.066 < 0.40` → winner arm.

## Falsification 2 — "avg loss ≫ avg win" is backwards

In raw KRW, avgWin **267k > avgLoss 235k** (ratio 1.14); in R, avg_win_R **0.809 > avg_loss_R 0.651**.
The deficit is the **win rate**: at 42.9% WR the payoff ratio must reach 1.33 to break even;
it sits at 1.14. Winners are decent but **cut short**.

## Sub-fork (give-back rule) — winners peak-then-fade → FIXED TARGET

Time-flat winners (n=60): **MFE 1.456R → captured 0.809R → gave back 0.647R** (median 0.525R).
**Give-back / MFE = 0.462 mean, 0.508 median** — winners surrender ~half their peak to the bell,
and **32/60 leave >0.5R on the table**. This is the "peak early then fade" signature the plan's
give-back rule assigns to a **fixed target** (lock the move), not a sustained-trend trail.

## What-if (OPTIMISTIC upper bounds — see caveat)

| Mechanism | Result (R/trade) |
|---|---|
| baseline | −0.016 |
| fixed target 0.75R | −0.002 |
| **fixed target 1.0R** | **+0.027** |
| **fixed target 1.5R** | **+0.060** (sim optimum) |
| fixed target 2.0R | +0.028 |
| trailing (giveback 0.3–0.75R) | +0.13 … +0.48 |

Trailing scores higher on paper but is the **more optimistic / fragile** estimate (it assumes the
trail catches `peak − G`, which depends on intrabar path). Fixed target banks at a fixed level and
the peak-then-fade signature argues the runs are **not** sustained enough to trail profitably.

**Caveat (applies to every what-if above):** these use `session_high` as the MFE proxy (a session
extreme, reconstructed via an ordered file-walk because `session_summary` is emitted with `ts=0`)
and assume the target fills before any intrabar stop. They **route the decision**; they do **not**
predict the v9 number. The re-baselined v9 backtest remains the arbiter of the `expectancy > 0` bar.

## Recommendations into ce-plan

1. **Build the winner arm → fixed profit target.** New `ExitReason::Target`; new `OrbParams`
   field `profit_target_r`, **provisional default 1.0** (robust; most winners reach it). Exit at
   `entry + profit_target_r · R`.
2. **Flag `profit_target_r = 1.5` as the sim optimum** for a follow-up param-turn sweep once the
   code lands (the loop's param machinery owns tuning).
3. **Add per-position MFE / high-water telemetry** (emit the post-entry high-water in R at exit) so
   the *next* exit-tuning turn reads give-back cleanly instead of reconstructing it from a ts=0
   session extreme. Cheap; optional for Turn 8's mechanism but high-leverage for the loop.
4. Loser arm and trailing arm from the plan are **not** selected this turn.
