# stop-width-geometry — Turn 11 pre-register (plan 2026-07-21-001, issue #119)

A **stop-out-geometry** lever, not a risk-sizing lever. CLASS B sizing
(`qty = budget·w_ratio / risk_per_share`) already owns the risk axis: re-scale the initial stop
by any factor `w` and `qty` re-sizes inversely, so `risk_capital = qty·(w·rps)` stays pinned at
budget. A stop re-scale is invisible to the RoR denominator — the only surviving effect is
which fraction of trades resolve **stop / target / timeout**. This screen tests whether any of
four conditioning signals, decorrelated from the two KEPT risk levers, predicts *when* more or
less stop room pays.

## Head + stop mode (R6)

v32 head `20260717T094841Z-backtest-orb-v32`, RoR **0.1876**, `stop_mode = 0.0` = **RangeLow**:
stop at the opening-range low, `r_denom = range_high − range_low` (OR-width) **decoupled** from
the stop. A stop-width weight moves the stop (and `risk_per_share`) but leaves target/breakeven
fixed — it changes reward:risk, not barrier-scaling (AE4). `diagnostic.py`/`twin.py` assert this
mode from the manifest before any reading.

## The four signals (documented ids)

| id | signal | definition |
|----|--------|------------|
| 1 | `orwidth_atr` | OR-width / prior-ATR ratio (`(RH−RL)/ATR`) |
| 2 | `minutes` | minutes since 09:00 at entry |
| 3 | `gap` | overnight-gap magnitude (`today_open/prior_close − 1`) |
| 4 | `orposition` | entry location in the range (`(entry−RL)/(RH−RL)`) |

Each forms the ratio-ATR tilt weight `w = clamp((ref/signal)^0.5, w_lo, w_hi)`, `ref = median`,
`w_lo = (ref/p90)^0.5`, `w_hi = (ref/p10)^0.5` (band straddles 1.0, one direction per signal —
high signal → tighter stop). The **screen `alpha = 0.5` equals the arm's `flip_value`**, so the
projection is read at the value that would be armed.

## Gates (frozen in candidate.json)

- **1a** `|Pearson r(w, risk_per_share)| < 0.70` — new axis, not a re-expression of the stop.
- **1b** `|Pearson r(w, w_ratio_atr)| < 0.70` — not a re-expression of the KEPT ratio-ATR tilt.
- **2a** `ror_shift ≥ 0.005` — projected RoR improvement, ceiling-aware, from an offline geometry
  re-sim (baseline = sim at `w=1`, so systematic fill bias cancels). Floor set **below the
  smallest historically-KEPT lever gain** (ratio-ATR +0.0091) and **above the screen's absolute
  sim error** (sim(w=1) RoR ≈ 0.152 vs the run's 0.1876 = favorable gap-through-limit fills) so a
  pass is genuinely build-worthy, not noise.
- **2b** `resolution_mix_shift ≥ 0.05` — fraction of trades whose stop/target/timeout class the
  re-scaled stop moves. **Fill-price-independent** (pure geometry) — the primary materiality
  reading (KTD3), replacing amihud's `qty_change_frac` which a geometry lever changes by
  construction. Floor mirrors amihud's `0.05`.

## Winner selection (KTD7)

The gate contract is single-signal, so the argmax happens inside the scripts: among signals
clearing all four gates, the largest `ror_shift`; if none clears, the best-by-`ror_shift` among
all four (its readings then fail a threshold → the tool records STOP = NO-BUILD). The winner's
four canonical readings feed the gate; `winning_signal_id` (tolerance 0) forces the twin to agree
on the winner, and `stop_width_ref`/`w_lo`/`w_hi` ride along as the arm's companion seeds.

`gate-verdict.json` is a command output, not a frozen input.
