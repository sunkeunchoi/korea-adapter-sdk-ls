# Strategy-loop turn log

Committed record of each loop turn's verdict + the bar conditions it held. The
full artifacts (`analysis.md`, manifests, performance) live in the gitignored data
home; this file is the durable, reviewable outcome trail.

## Turn — ATR volatility-target sizing (CLASS B lever 2): PREDICTED-INERT at the Phase-A gate, NO-BUILD (2026-07-14) — plan 2026-07-14-002

- **Verdict: PREDICTED-INERT — STOP at the pre-code Phase-A collinearity gate; no lever
  code written, no run, v26 stays head** (hash `d199d124`, RoR 0.1171). The R6 re-rank of
  the deferred CLASS B sizing candidates — a second sizing lever `atr_vol_target_krw` that
  would replace the risk denominator with an **external** prior-daily ATR instead of the
  kept lever's **internal** stop distance (`risk_per_share = entry − stop`). The plan gated
  the whole build on a cheap diagnostic-first probe: measure whether ATR is orthogonal to
  the stop distance *before writing any code*. It is not.
- **The pre-registered gate (frozen before the number was read,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-atr-vol-target.md`).** GO to Phase B iff
  `|Pearson r(atr_price, risk_per_share)| < 0.70` (R² < 0.49 — a materially independent
  reallocation axis); `≥ 0.70` → predicted-INERT stop. `atr_price` recomputed offline by
  the exact `backtest.rs::prior_atr` (14-session frozen window) over v26's closed trades;
  paired with the stop-based `risk_per_share = risk_capital / qty`.
- **The measurement (v26, n = 103 ATR-available of 167 closed; the 64 excluded are the
  early-range sessions with < 15 daily priors — catalog starts 2026-05-18, ATR live from
  2026-06-12, exactly where the `or_width_max_atr = 0.666` gate is active).**

  | statistic | value | role |
  |---|---|---|
  | **Pearson `r(atr_price, risk_per_share)`** | **0.9593** (R² **0.9202**) | **PRIMARY — the gate** |
  | Spearman `ρ` | 0.9785 | diagnostic |
  | top-quartile cohort Jaccard overlap | 0.7931 (23/29) | diagnostic |

- **Why INERT (the crux).** `|r| = 0.9593 ≥ 0.70` fires the stop unambiguously: ATR shares
  **92%** of its variance with the stop distance the kept lever already normalizes on, ranks
  near-perfectly (ρ = 0.98), and de-risks a ~79%-overlapping cohort. Both quantities are
  absolute-KRW measures dominated by the same cross-sectional price/volatility scale — a
  prior-daily-ATR vol-target substitution merely **re-expresses v26's one-sided de-risking of
  the wide-`risk_per_share` cohort** on a duplicate axis, so RoR cannot move. This is the
  plan's explicitly-predicted headline risk ("Likely INERT via collinearity"), realized.
- **No override, no tuning-to-escape.** At R² = 0.92 there is no rationale for an operator
  override, and softening the frozen 0.70 threshold to proceed would be the forbidden overfit
  the plan names (R2/R7). Kelly (INERT-global / P&L-fit-conditional) and mark-to-market
  compounding (needs the deferred account seam) remain deferred CLASS B items; alternative
  vol estimators (EWMA / realized-vol) or a **cross-sectionally-normalized** ATR (ATR/price,
  which would break the price-scale collinearity) are the only live directions left for this
  family — a *future* turn, not this one.
- **Registry state.** Head unchanged: **v26** (`20260712T080054Z-backtest-orb-v26`, hash
  `d199d124`, RoR 0.1171). No `params.rs`/`orb.rs`/`performance.rs` edit → gate untouched.
  Diagnostic reproducer + reading archived under
  `data/turn4-fresh/sizing-archive/u5-collinearity-diagnostic/`; the frozen threshold and
  full decision in `PRE-REGISTER-vNEXT-atr-vol-target.md`. Offline throughout; no gateway.

## Turn — `risk_per_trade_krw` governed sweep: re-KEEP 299,340 as v26 (2026-07-12) — plan 2026-07-12-002

- **Verdict: re-KEEP v26 — the sweep DENIES that 348k was near-optimal and re-KEEPs the
  tighter LOWER leg `risk_per_trade_krw = 299,340`** (p33 of v24's closed-trade
  `risk_capital` distribution) as the new registry head, superseding v25's 348,000 (p50).
  A **GOVERNED THREE-LEG PARAM FAN-OUT off v25**, not a code turn: no `orb.rs` / `params.rs`
  / `performance.rs` edit → `strategy_code_hash d199d124…` fixed on every leg. Pre-registered
  values + keep/confirm rule + bind signature before any run (R1/R8,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-sizing-sweep.md`). Anchor = v25
  (`20260712T065730Z-backtest-orb-v25`, RoR **0.1139**, the bar to beat).
- **The three legs (percentile neighbours of v24 `risk_capital`, n=167, linear/R-7; NOT a
  P&L fit).** Each a governed `LS_TURN_PARAM=risk_per_trade_krw` turn seeded from v25
  (`EXPECT_VERSION=25 → v26`), each archived out of `runs/` before the next so v25 stayed
  `latest_finalized` (fan-out discipline, KTD-B). All three `runs compare` **param mode**
  vs v25 PASS with diff exactly `{risk_per_trade_krw, strategy_version}`, `strategy_code_hash`
  equal (`d199d124…`):

  | Leg | budget | percentile | **RoR (crux)** | mean-R | Σrisk_capital | risk-dom | risk-budget-bound | exp (diag) |
  |---|---|---|---|---|---|---|---|---|
  | HIGHER | 392,000 | p66 | 0.1106 | 0.1129 | 53.36M | 5.3% | 76/184 | +38,585 |
  | _v25 (bar)_ | _348,000_ | _p50_ | _**0.1139**_ | _0.1129_ | _49.65M_ | _5.3%_ | _98/184_ | _+36,959_ |
  | **LOWER ← re-KEEP** | **299,340** | **p33** | **0.1171** | 0.1129 | 44.64M | 5.5% | 130/184 | +34,178 |
  | TIGHT | 238,000 | p15 | 0.1168 | 0.1140 | 36.80M | 5.6% | 158/183 | +28,274 |

- **Why re-KEEP, not the anticipated CONFIRM (the crux).** The keep rule (R6) is a
  deterministic strict inequality on RoR: re-KEEP iff a leg's RoR strictly beats 0.1139 with
  `is_edge` (positive expectancy, risk-capital dominance ≤ 40%). **Two legs cleared it**
  (LOWER 0.1171, TIGHT 0.1168); LOWER is the argmax. RoR-vs-budget is single-peaked (concave)
  with its **interior maximum at 299,340 — *tighter* than the kept 348k** — so 348k was not
  near-optimal: the risk-adjusted edge kept climbing as the cap tightened below it, turning
  over only between 299,340 and 238,000 (TIGHT 0.1168 < LOWER 0.1171, the predicted
  overshoot-then-decline, but the turnover point sits well below 348k). The plan expected
  CONFIRM but pre-registered this exact re-KEEP branch ("if LOWER or TIGHT strictly beats
  0.1139 with `is_edge`, RoR is still climbing → re-KEEP").
- **Bind validated, monotone, no INERT leg.** Classifying each `order_placed` from its
  `decisions.jsonl` sizing telemetry (`qty = min(floor(budget/risk_per_share),
  floor(notional/price))`, ties → budget-bound per the v25 convention), risk-budget-bound
  count is monotone in the budget (76 → 98 → 130 → 158 of 184) and Σrisk_capital monotone
  (53.36M → 49.65M → 44.64M → 36.80M). v25's split reproduces the KEEP turn's 98/184 exactly.
  Equal-weight **mean-R is invariant at 0.1129** across the three pure-reallocation legs (same
  167-trade set), confirming RoR moved purely from **risk reallocation**, not a size change —
  a genuine risk-adjusted improvement. The mechanism is the same one-sided de-risking v25
  named: tightening the budget de-weights the wide-`risk_per_share` cohort (below-mean
  return-per-unit-risk) further, lifting RoR past the equal-weight mean until ~300k, past
  which the equalization begins trimming the trade set (238k: 166 vs 167 closed) and RoR ticks
  back down.
- **Registry state.** New head **v26** (`20260712T080054Z-backtest-orb-v26`, budget 299,340,
  hash `d199d124…`) in `runs/`; v25 stays in the chain; the losing legs
  (`TIGHT-238000-…-orb-v26`, `HIGHER-392000-…-orb-v26`) and the CLASS B re-baseline v24 sit
  under `sizing-archive/`. Pessimistic bar-low fill makes the +2.8% RoR gain a lower bound.
  Judged on **RoR + risk-capital dominance** only; KRW/trade expectancy is size-contaminated
  and diagnostic. `cargo test -p nautilus-ls-lab` green; `strategy_code_hash` unchanged.
  **Next lever:** a *separate* pre-registered deeper-equalization probe (p10 / p5 of the v24
  `risk_capital` distribution) to find where RoR turns over, or the next CLASS B lever
  (ATR/volatility-scaled notional, Kelly-fraction sizing).

## Turn — CLASS B: normalized edge metric + first risk-sizing lever (2026-07-12) — plan 2026-07-12-001

- **Verdict: KEEP v25 — the first CLASS B (risk / position-sizing) lever, judged on a
  re-grounded size-invariant edge metric.** A CODE turn (return-on-risk + risk-dominance
  metric, additive per-trade risk ledger fields, the default-off `risk_per_trade_krw`
  sizing lever) followed by its single flip `risk_per_trade_krw` 0.0 → **348,000** KRW
  (= median of v24's closed-trade `risk_capital` distribution). Pre-registered value +
  keep rule + bind signature before the run (R8,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-sizing.md`). Baseline for the verdict is the
  re-baseline **v24** (`20260712T065529Z-backtest-orb-v24`), not v23's stored artifacts
  (they carry no per-trade `risk_capital`).
- **Why the metric moved first (the crux).** Every prior lever was judged on KRW/trade
  **expectancy** against a fixed 10M notional — size-invariant only while size is held
  constant. A sizing lever decouples them (uniformly sizing up doubles expectancy with
  zero better edge), so the keep gate is re-grounded on **return-on-risk**
  `RoR = Σrealized_pnl / Σrisk_capital` (the risk-weighted mean R): flat under a uniform
  size-up, responsive only to risk reallocation. Equal-weight `mean_realized_r` rides as
  a size-invariant diagnostic invariant; dominance is re-grounded to **risk-capital
  share** (can't be gamed by sizing one symbol huge), legacy |P&L| share retained as a
  diagnostic. The `> +44,046.41 KRW/trade` clause is **retired**.
- **The code change.** `risk_per_trade_krw` (default-off `f64`, sentinel 0.0 =
  fixed-notional v23): when `> 0`, `qty = min(floor(budget / risk_per_share), floor(notional
  / entry))` where `risk_per_share = entry − stop` (entry-fixed initial stop). The
  **notional ceiling** caps a tiny-stop blow-up, so the lever can only shift size within
  the 10M envelope. Per-trade `risk_capital`/`realized_r` joined into the trade ledger
  (additive; legacy `performance.json` keys byte-unchanged). `validate()` rejects a
  negative budget.
- **Re-baseline evidence (R7, KTD3).** v24 (`risk_per_trade_krw = 0.0`) reconciles
  **1:1** to v23 (`20260712T045403Z-backtest-orb-v23`): summary + equity_curve + all 171
  trades' legacy fields byte-identical; the only delta is the additive risk fields (all
  167 closed trades' `risk_capital` populated — the strategy→ledger join is complete).
  `runs compare` param-mode `v23 → v24` **FAILs** on `strategy_code_hash differs` (param
  diff `["strategy_version"]`) — the expected code-turn re-baseline signal. `v24 → v25`
  PASSes with diff exactly `{risk_per_trade_krw, strategy_version}`.
- **The result (the three pre-registered keep conditions, all met).**
  **RoR 0.10811 (v24) → 0.11389 (v25)** (+5.3%, strictly rises — the KEEP crux);
  `is_edge(v25)` holds (expectancy +36,959 > 0, risk-dominance 5.3% ≤ 40%);
  risk-dominance 5.3% ≤ 40%. **KEEP.**
- **Bind validated + honest deviation (KTD).** The lever BINDS — 98/184 order placements
  risk-budget-bound (wide-`risk_per_share` setups shrink toward the 348k budget), 86
  notional-cap-bound (**identical to v24**). So the flip is a **one-sided de-risking of
  the wide-stop cohort**, not the symmetric reallocation the pre-register predicted:
  median `risk_capital` drifted 348k → 318k, Σrisk_capital −20% (the notional ceiling
  prevents tight-stop upsizing → risk can only be *cut* off wide-stop setups). RoR rises
  because that de-weighted cohort has below-average return-per-unit-risk: v24 fixed-notional
  RoR 0.108 sits **below** the equal-weight mean-R 0.113 (fixed notional over-weights
  low-R wide-stop trades); capping their risk moves RoR to 0.114, at/above the mean.
  Σpnl falls in absolute KRW (6.74M → 5.65M) and the retired expectancy falls (44,046 →
  36,959) — expected and non-decisional (v25 earns more per unit deployed risk).
- **Registry.** v25 (`20260712T065730Z-backtest-orb-v25`) is the new head; v24 (the
  intermediate re-baseline) archived under `data/turn4-fresh/sizing-archive/`. Offline
  throughout; pessimistic bar-low fill → the +5.3% RoR is a lower bound. **FOUR kept
  levers, THREE classes (entry-quality ×2, exit-timing ×1, risk-sizing ×1).**
- **Next lever.** A governed sweep of `risk_per_trade_krw` to percentile neighbours of
  348k (p33/p66 of the v24 risk_capital distribution) — mirrors the breakeven trigger
  sweep — to test whether the risk cap is near-optimal or the edge climbs as the cap
  tightens toward full equalization (RoR → equal-weight mean-R).

## Turn — breakeven-TRAIL exit lever (candidate A) (2026-07-12) — plan 2026-07-11-001

- **Verdict: REVERT v25 — the trailing-stop variant is FALSIFIED. It BINDS exactly as
  designed (books partial wins on the give-back cohort) but the winner-cutting cost
  dominates: expectancy collapses −67% vs v23. Baseline STAYS v23 (flat breakeven move).
  A binding-but-worse falsification (R5), not insufficient-evidence — the mechanism is
  proven to work and proven not to pay.** A CODE turn (add a default-off trailing arm on
  top of the kept breakeven ratchet in `orb.rs`) followed by its single flip `trail_frac_r`
  0.0 → **0.25** (= ½·median of v23's breakeven-armed `stop_hit` cohort peak-MFE 0.524).
  Pre-registered value + keep rule + bind signature before the run (R3,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-breakeven-trail.md`).
- **The code change.** Once the breakeven ratchet has ARMED (`high_water ≥ entry +
  round(0.41·R)`, the sweep-confirmed trigger, untouched), for SUBSEQUENT bars the stop
  trails: `stop = max(prior_stop, entry, high_water − round(trail_frac_r·R))` — floored at
  entry, only ever tightens. So a runner that peaks well past the trigger then reverts
  books a **partial win** at the trailed stop, not just a scratch at breakeven. Respects
  **KTD5** (reads only folded `high_water`) and **KTD2** (never applies the tightened trail
  on the bar that raised `high_water`). Off (`trail_frac_r == 0.0`) the trail term would be
  `high_water` (too tight), so OFF is an explicit `trail_frac_r > 0` gate that falls back to
  the flat breakeven — outcome-identical to v23. A round-to-zero give-back is also treated
  as flat breakeven. `validate()` rejects a negative trail. A new telemetry `realized_r`
  rides every exit envelope (booked R) so the bind check reads give-back-cohort realized-R
  directly. Pre-flip code review (correctness + adversarial, both session-model) found **no
  correctness defect**; one doc-precision fix (off-path is outcome-identical, not
  `decisions.jsonl`-byte-identical). 315 lab tests pass (306 + 9 new trail tests).
- **Type: CODE turn.** `strategy_code_hash a5521c3e…` → **`fd5125c2…`**. Re-baselined via
  seed-and-rerun (KTD2): v24 seeded from v23, `trail_frac_r=0.0`, `strategy_version=24`.
  `performance.json` (trades + equity_curve + summary) vs v23 **reconciled 1:1**
  (expectancy 44,046.41, PF 1.620, 167 closed — identical) — the trail is verifiably
  default-off. Re-baseline signal: `runs compare` param mode (v23 → v24) **FAILs**
  `strategy_code_hash differs`, param diff `["strategy_version"]` (KTD3 — the FAIL *is* the
  evidence). The flip off the `0.0` sentinel is seed-and-rerun, not a governed turn
  (`0.0 → 0.25` is an infinite relative change; `PROPOSAL_BOUNDS_CAP` 0.5 fail-closes).
- **AE2 attribution:** `runs compare` param mode (**v24 → v25**) **PASS**, diff exactly
  `{trail_frac_r, strategy_version}` — clean single-lever flip on the re-baselined code
  (v24 and v25 share `fd5125c2…`).
- **Bind check — mechanism BINDS (pre-registered signature validated):** the breakeven-armed
  `stop_hit` cohort's realized exit-R shifts UP off scratch **−0.034R → +0.242R** (median);
  **97% (99/102)** of the armed cohort now books a positive partial (`realized_r > 0`; v23
  had ≈none). But the **winner-cutting cost is severe**: `target` runners collapse **50 →
  11** (the 0.25R trail stops 39 of v23's 50 would-be 1.0R winners short, re-booking them as
  ~0.24R partials). Exit mix `stop_hit`/`time_exit`/`target`: v23 `77/57/50` → v25
  `131/52/11`.
- **Edge gate (`EdgeEvaluation`, unchanged): `is_edge = yes`** (positive expectancy,
  dominance **9.2%** ≤ 40%), but the keep rule (is_edge AND expectancy > +44,046.41 AND
  dominance ≤ 40%) **FAILS on the middle clause**: expectancy **+14,737.28** KRW/trade
  (v23 +44,046.41 — a **−67%** collapse), pnl_total **2,549,550** (v23 6,739,100), WR 62.5%,
  176 closed. → **REVERT.**
- **Read — the trail works but does not pay.** Converting the peaked-then-reverted give-back
  scratches into ~0.24R partials (99 of them) does not compensate for surrendering ~0.76R
  apiece on 39 would-be-target winners: the 0.25R give-back is **too tight** relative to the
  winners' intraday pullback structure — they routinely dip past 0.25R below an interim high
  on the way to 1.0R and get trailed out. A looser trail would cut fewer winners but (per the
  pre-register) degenerate toward v23's flat breakeven. The pessimistic bar-low fill leaves
  14,737 a lower bound, but the gap to v23 is far too wide to close.
- **Queue re-rank (R6) — exit block now WELL-CHARACTERIZED; baseline unchanged at v23.**
  Three exit-block probes: breakeven **trigger** sweep-confirmed near-optimal (0.41),
  flat-breakeven **move** KEPT (v23), **trail** FALSIFIED (this turn). Still THREE kept
  levers across TWO classes (entry quality ×2 + exit timing ×1 = the flat breakeven move).
  Trailing joins the falsified set (stop geometry U7, entry timing lever 4, entry-quality
  RVOL lever 5). The next motivated turn is a **new class — CLASS B risk/position-sizing**
  (needs `/ce-plan` for a normalized edge metric), not another exit-block probe.
- **Provenance:** baseline `20260712T045403Z-backtest-orb-v23` (registry head, restored),
  re-baseline `20260712T054957Z-backtest-orb-v24` (`trail_frac_r=0.0`, reconciled 1:1),
  flip `20260712T055143Z-backtest-orb-v25` (`trail_frac_r=0.25`) — both archived under
  `data/turn4-fresh/trail-archive/` so v23 stays head (home `data/turn4-fresh`, gitignored).
  Offline, no gateway.

## Turn — breakeven-trigger governed sweep / confirm-or-deny (2026-07-12) — plan 2026-07-11-001

- **Verdict: CONFIRM v23 — the sweep confirms the pre-registered `breakeven_trigger_r
  = 0.41` (p50) is near-optimal; NEITHER percentile neighbor beats it. Baseline STAYS
  v23. A confirm turn, not a failure (R5) — it de-risks the exit block before the next
  code turn.** A GOVERNED PARAM TURN (candidate B), **not a code turn**: two governed
  `LS_TURN_PARAM` sweeps of the kept breakeven-move trigger, each a single admissible
  step off `0.41` (both inside `PROPOSAL_BOUNDS_CAP = 0.5`). No re-baseline, no
  seed-and-rerun, no pre-flip code review. Pre-registered values + directional
  hypotheses + keep rule + monotone bind signature before the run (R3,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-breakeven-sweep.md`).
- **The two points (percentile neighbors of the kept p50, not a fit).** Anchored on the
  v21 (pre-lever) `time_exit` cohort peak-MFE distribution (n=76, recomputed this turn):
  **LOWER `0.25`** (p33 0.2517; −39.0% off 0.41) and **HIGHER `0.52`** (p66 0.5167;
  +26.8% off 0.41). Each is its own governed turn; the two form a **fan-out** around
  v23, not a chain — the bounds cap is measured against the *immediate* base, so
  `0.25 → 0.52` (+108%) is refused, proving the neighbors must each seed from v23 (both
  therefore nominally "v24", distinguished by run_id + trigger value; archived under
  `data/turn4-fresh/sweep-archive/`).
- **Type: PARAM turn.** `strategy_code_hash a5521c3e…` **unchanged** across v23 and both
  sweep runs (zero `orb.rs` edits, hash lock). **AE2 attribution:** `runs compare` param
  mode **v23 → each sweep run** **PASS**, diff exactly `{breakeven_trigger_r,
  strategy_version}` — clean single-lever, code hash identical (the FAIL-on-code-turn is
  absent because there is no code turn).
- **Bind check — BOTH bind monotonically as pre-registered.** As the trigger rises
  `0.25 → 0.41 → 0.52`: `time_exit` **45 → 57 → 64** (monotone ↑), `stop_hit` **111 → 77
  → 56** (↓), breakeven-armed `stop_hit` **92 → 52 → 28** (↓), `target` **36 → 50 → 56**
  (↑), closed trades **174 → 167 → 160** (↓). LOWER arms MORE (protects more give-backs,
  cuts more marginal winners); HIGHER arms FEWER (cuts fewer winners, surrenders the
  p50–p66 give-back band to the 15:00 flat exit). Both directional hypotheses confirmed.
- **Edge gate — both neighbors clear `is_edge` but NEITHER beats v23.** Expectancy is
  **concave with an interior maximum exactly at p50 = 0.41**:

  | trigger | expectancy (KRW/trade) | vs v23 | PF | WR | dominance | closed |
  |---|---|---|---|---|---|---|
  | 0.25 (LOWER, p33) | +29,991.45 | **−14,055** | 1.504 | 47.7% | 9.5% | 174 |
  | **0.41 (v23, p50)** | **+44,046.41** | **— (peak)** | **1.620** | **50.3%** | **9.5%** | **167** |
  | 0.52 (HIGHER, p66) | +39,167.79 | **−4,878** | 1.456 | 51.3% | 9.2% | 160 |

  Keep rule (is_edge AND expectancy > +44,046.41 AND dominance ≤ 40%): **both FAIL on
  the middle clause** — positive edges, dominance capped, but sub-v23. → **CONFIRM.**
- **Read — 0.41 sits at the give-back / marginal-winner trade-off optimum.** Below it
  (0.25) the ratchet over-arms: the extra winner-scratching (target 50 → 36) outweighs
  the extra give-back protection. Above it (0.52) it under-arms: it cuts fewer winners
  but hands the p50–p66 give-back band back to the flat exit. The pre-registered
  percentile (median of the untreated give-back cohort) lands on the interior maximum —
  the sweep validates the *selection method*, not just the value. The pessimistic
  bar-low fill leaves this a lower bound at every trigger.
- **Queue re-rank (R6) — exit-timing trigger is DE-RISKED; baseline unchanged at v23.**
  Still THREE kept levers across TWO classes (entry quality ×2 + exit timing ×1). The
  breakeven trigger is now sweep-confirmed near-optimal, so the next motivated turn is
  **not** another breakeven-trigger param flip (that would be a fit) — it is a **new
  mechanism**: (a) CLASS A **trailing** variant (trail a fraction of R above breakeven
  once armed — books partial wins, not just scratches — a CODE turn); or (c) CLASS B
  **risk/position-sizing** (needs `/ce-plan` for a normalized edge metric). Candidate B
  (governed trigger sweep) is now **spent**.
- **Provenance:** baseline `20260712T045403Z-backtest-orb-v23` (registry head, restored),
  LOWER `20260712T051236Z-backtest-orb-v24` (0.25), HIGHER `20260712T051604Z-backtest-orb-v24`
  (0.52) — both archived under `data/turn4-fresh/sweep-archive/` so v23 stays the head
  (home `data/turn4-fresh`, gitignored). Offline, no gateway.

## Turn — breakeven-move exit lever (2026-07-12) — plan 2026-07-11-001

- **Verdict: KEEP (the breakeven-move exit lever clears the edge gate and MORE THAN
  DOUBLES expectancy over v21 — the loop's FIRST non-entry / exit-timing kept lever).**
  A CODE turn (add a default-off breakeven-move ratchet to `orb.rs`) followed by its
  single flip `breakeven_trigger_r` 0.0 → **0.41** (p50 of the v21 time_exit cohort's
  peak-MFE). Pre-registered value + keep rule + bind signature before the run (R3,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-breakeven-move.md`).
- **The code change.** Once a held long's provably-observed MFE (`high_water`) reaches
  `entry_price + round(breakeven_trigger_r · R)`, the stop ratchets up to `entry_price`
  for **subsequent** bars — a runner that peaks then reverts books at-or-near breakeven
  instead of decaying to the 15:00 flat exit (v21's largest give-back cohort: n=76,
  median 0.41R MFE). Respects **KTD5** (folds only provably-observed MFE, ratchet read
  post-fold) and **KTD2** (never books on the bar that arms it — the low that would hit
  the new stop may precede the high that triggered it). Only ever tightens; arms once;
  `breakeven_trigger_price` requires the rounded R-offset > 0 so a round-to-zero trigger
  is off. Pre-flip code review (correctness + adversarial, both session-model) found **no
  correctness/same-bar/fold defect**; two P3 advisories addressed. 306 lab tests pass.
- **Type: CODE turn.** `strategy_code_hash f8a0f2bf…` → **`a5521c3e…`**. Re-baselined via
  seed-and-rerun (KTD2): v22 seeded from v21, `breakeven_trigger_r=0.0`, `strategy_version=22`.
  Per-trade ledger vs v21 **157/157 byte-identical**; summary byte-identical — the ratchet
  is verifiably default-off. Re-baseline signal: `runs compare` param mode (v21 → v22)
  **FAILs** `strategy_code_hash differs`, param diff `["strategy_version"]` (KTD3 — the FAIL
  *is* the evidence). Baseline advanced to v22 (== v21 behavior, new hash).
- **AE2 attribution:** `runs compare` param mode (**v22 → v23**) **PASS**, diff exactly
  `{breakeven_trigger_r, strategy_version}` — clean single-lever flip on the re-baselined
  code (v22 and v23 share the new hash).
- **Bind check — mechanism BINDS (pre-registered signature validated):** v23
  `decisions.jsonl` shows `time_exit` **76 → 57** (the pre-registered shrink) and a
  breakeven-armed `stop_hit` cohort (mfe_r ≥ 0.41) **11 → 52** (`stop_hit` total 35 → 77,
  median mfe_r 0.286 → 0.458 — the new exits are the high-prior-MFE peaked-then-reverted
  trades). `time_exit` median MFE falls 0.41 → 0.21 (only the low-peak tail that never
  reached the trigger remains). `target` 58 → 50 (−8 winner-cutting cost); total exits
  169 → 184, closed trades 153 → 167 (the pre-registered `max_concurrent` re-admission).
- **Edge gate (`EdgeEvaluation`, unchanged): CLEARED (`is_edge = true`).** Expectancy
  **+44,046.41** KRW/trade (v21 +20,690.73 — **2.13×**), PF **1.620** (from 1.204),
  pnl_total **+6,739,100** (from +3,124,300), Sharpe **+4.49** / Sortino **+9.07**, WR
  50.30%. Max drawdown **HALVED** (6,005,700 → 3,171,350). Dominance **9.5%** (≤ 40%),
  top-|P&L| symbol `035420.XKRX` a **winner** (+1,570,200). Keep rule (is_edge AND
  expectancy > +20,690.73 AND dominance ≤ 40%): **all three cleared.**
- **Read — capturing the give-back beats cutting the winners, decisively.** The 19 fewer
  time_exits and their avoided give-back losses (avg loser −218,935 → −169,209, drawdown
  halved) far outweigh the 8 would-be targets the breakeven stop cut short. The
  `max_concurrent` re-admission (153 → 167) did not drown the benefit (unlike the U7
  midpoint stop). The pessimistic bar-low fill makes this a *lower bound* on the edge.
- **Queue re-rank (R6) — exit-timing is a PROVEN new dimension.** Baseline advances to
  **v23** (`breakeven_trigger_r=0.41`). THREE kept levers now, across TWO classes: entry
  quality (close-confirm v16, decoupled OR-width v21) + **exit timing (breakeven-move v23,
  the first non-entry lever)**. Falsified/exhausted: stop geometry (U7 + demoted leg-2),
  entry timing (lever 4), entry quality via RVOL (lever 5, inverted). Strongest next
  turns: (a) a **trailing** variant (trail a fraction of R above breakeven once armed —
  could book partial wins, not just scratches); (b) a **governed breakeven-trigger sweep**
  (now off the 0.41 sentinel); (c) CLASS B **risk/position-sizing** (via `/ce-plan` for a
  normalized edge metric).
- **Provenance:** baseline `20260712T041102Z-backtest-orb-v21`, re-baseline
  `20260712T045306Z-backtest-orb-v22` (157/157 reconcile), flip
  `20260712T045403Z-backtest-orb-v23` (home `data/turn4-fresh`, gitignored). Offline.

## Turn — OR-width decoupled from ATR availability (2026-07-12) — plan 2026-07-11-001

- **Verdict: KEEP (decoupled OR-width clears the edge gate and improves expectancy 4.3×
  over v16 — the loop's SECOND kept lever).** A CODE turn (decouple the OR-width gate
  from ATR availability) followed by its single flip `or_width_max_atr` 0.0 → **0.666**
  (same p80 threshold as the reverted lever 3 / v18). Pre-registered value + keep rule +
  decouple signature before the run (R3,
  `data/turn4-fresh/PRE-REGISTER-v21-or-width-decoupled.md`).
- **The code change — SKIP-not-reject (design A).** The OR-width arm of
  `session_gate_reject` (`orb.rs`) used to fail closed as `atr_unavailable` on any
  session lacking a positive prior ATR — coupling a width test to ATR coverage. Lever 3
  reverted for exactly that confound (191 `atr_unavailable` culls of the winner-rich
  ATR-uncovered cohort swamped a clean 68-session width tail). The decouple makes a
  no-ATR session simply **not width-gated** (skip, not reject); the ATR-STOP arm keeps
  its fail-closed reject (a stop needs its ATR); the RVOL arm is unchanged. At
  `or_width_max_atr=0.0` the arm is inert. Pre-flip code review (correctness +
  adversarial, lever-2 precondition discipline) found **no defect** — default-off
  byte-identical, ATR-stop arm untouched, tests non-vacuous.
- **Type: CODE turn.** `strategy_code_hash` moved `fa7733f6…` → **`f8a0f2bf78033264…`**.
  Re-baselined via seed-and-rerun (KTD2): v20 seeded from v16, all params identical
  (`or_width_max_atr` still 0.0), `strategy_version=20`. Per-trade ledger vs v16
  **161/161 byte-identical**; all 21 summary fields equal — the decouple is verifiably
  default-off. Re-baseline signal: `runs compare` param mode (v16 → v20) **FAILs**
  `strategy_code_hash differs`, `param diff ["strategy_version"]` (KTD3 — the FAIL *is*
  the evidence). Baseline advanced to v20 (== v16 behavior, new hash).
- **AE2 attribution:** `runs compare` param mode (**v20 → v21**) **PASS**, diff exactly
  `{or_width_max_atr, strategy_version}` — clean single-lever flip on the re-baselined
  code (v20 and v21 share the new hash).
- **Bind check — decouple PROVEN (pre-registered signature validated):** v21
  `decisions.jsonl` carries **0** `atr_unavailable` rejects (v18 had 191 — all converted
  to skips; `stop_mode=0.0` so the ATR-stop arm never fires, and zero confirms no leak)
  and **68** `filter:"or_width_atr"` rejects (identical to v18 — the clean width tail
  still binds). `max_concurrent` 34 → 48 (more sessions reach breakout). Closed trades
  158 → **153** (only 5 removed net, vs v18's −64).
- **Edge gate (`EdgeEvaluation`, unchanged): CLEARED (`is_edge = true`).** Expectancy
  **+20,690.73** KRW/trade (v16 +4,812.74 — 4.3×), PF **1.204** (from 1.044), pnl_total
  **+3,124,300** (from +755,600), Sharpe **+1.89** / Sortino **+3.22**, WR 49.67%.
  Dominance **10.3%** (≤ 40%), top-|P&L| symbol `035420.XKRX` is a **winner**
  (+1,822,200) — not one-winner-carried. Keep rule (is_edge AND expectancy > +4,812.74
  AND dominance ≤ 40%): **all three cleared.**
- **Read — the clean width signal earns its keep once the confound is removed:** the
  decouple isolated exactly what lever 3 could not — removing the 68 wide-OR width-tail
  sessions (drifters+losers, 0 targets) **without** the winner-rich ATR-coverage cull.
  Only 5 net trades leave, yet pnl_total quadruples. Same 0.666 threshold, same 68 width
  kills, opposite verdict from v18 — proving v18 reverted for the *coverage confound*,
  not the width signal. The `max_concurrent` re-admission (34 → 48) did not drown the
  benefit (unlike the U7 midpoint stop). `report mfe`: target share 31.0% → **34.3%**
  (58/169), stop_mode 0 (range-R label).
- **Queue re-rank (R6) — the entry-filter mechanism class is now SPENT.** Baseline
  advances to **v21** (`or_width_max_atr=0.666`). Two kept levers, both entry-quality:
  close-confirm (v16) + decoupled OR-width (v21). Falsified/exhausted: stop geometry (U7
  + demoted leg-2), entry timing (lever 4), entry quality via RVOL (lever 5, inverted).
  The next motivated turn is a **NEW mechanism class** — strongest candidates: a
  risk/position-sizing lever (scale the now-positive edge), or an exit-timing lever (the
  `time_exit` bucket n=76 median 0.41R give-back is still the largest non-target exit).
  A second OR-width percentile sweep is NOT motivated (would be pnl-fit).
- **Provenance:** re-baseline run `20260712T040752Z-backtest-orb-v20`, flip run
  `20260712T041102Z-backtest-orb-v21` (home `data/turn4-fresh`, gitignored). Offline, no
  gateway. Zero `orb.rs` edits after the v20 re-baseline run (KTD8).

## Turn — lever 5: opening-window RVOL (2026-07-12) — plan 2026-07-11-001

- **Verdict: REVERT (lever 5 fails the edge gate — expectancy collapses).**
  `rvol_min` 0.0 → **0.655** (p20 of the RVOL-ratio distribution) as a single-param
  seed-and-rerun flip from the v16 baseline. Pre-registered value + keep rule before the
  run (R3). The gate rejects a session when `open_window_vol < rvol_min ·
  prior_open_vol_mean` — a low-relative-volume opening (a FLOOR trimming the bottom tail).
- **Diagnostic-first — predicted the revert up front (two adverse signals).** A
  throwaway probe (`rvol_min = 1e9`, deleted) measured, over the v16 sessions: **RVOL
  coverage 78.4%** (417/532 have a positive prior mean with ≥5 samples) — the other
  21.6% fail-close as `rvol_insufficient_history` at any threshold, winner-rich (40 v16
  trades there, 16 target = 40% vs 31% overall) — the same lever-3-style coverage cull,
  milder. **AND the hypothesis was INVERTED:** winners (target) have LOWER opening RVOL
  (med 0.898) than losers (stop+time, med 1.064), so a `rvol_min` floor trims the
  winner-enriched bottom tail. A first-order projection showed the kept target-rate
  degrading at every threshold (p20 → 25.9% vs 31.0% baseline). Both predicted a
  confounded revert — said so in the pre-registration.
- **Type: PARAM turn, not a code turn.** `strategy_code_hash fa7733f6df76ca39…`
  unchanged between baseline and flip; zero `orb.rs` edits (hash lock, KTD8). The RVOL
  gate already shipped in the harness code turn (#121); this only moves the param.
  Seed-and-rerun (not a governed `LS_TURN_PARAM` turn) because the 0.0 → 0.655 move is an
  infinite relative change off the 0.0 sentinel (guardrail PROPOSAL_BOUNDS_CAP 0.5
  fail-closes it).
- **Seed from v16, NOT the registry head.** Latest finalized was v18 (reverted lever 3);
  the seed took v16's params (`or_width_max_atr` and `entry_cutoff_min` at 0.0) + the
  flip, dated after v18 so `latest_finalized_run` picked it. Next version = v19.
- **AE2 attribution:** `runs compare` param mode (**v16** → v19) **PASS**, diff exactly
  `{rvol_min, strategy_version}` — clean single-lever attribution, code hash identical.
- **Bind check — BINDS but CONFOUNDED:** v19 `decisions.jsonl` carries **84**
  `filter:"rvol_min"` rejects (the gate firing on volume — not inert) **plus 115**
  `rvol_insufficient_history` rejects. The coverage cull dominates numerically (115 vs
  84) — exactly the pre-registered confound, the same shape as lever 3 (191 vs 68).
  Closed trades 158 → 111.
- **Edge gate (`EdgeEvaluation`, unchanged): NOT cleared (`is_edge = false`).**
  Expectancy **−21,276.15** KRW/trade (v16 +4,812.74), PF 0.82 (from 1.044), pnl_total
  **−2,319,100** (from +755,600), Sharpe −1.59 / Sortino −2.17, WR 46.9% (from 49.4%).
  Dominance 13.0% (≤ 40% passes, but moot — expectancy is deeply negative).
- **Read — coverage confound PLUS an inverted signal (falsification):** target-exit
  share fell 31.0% → 24.6% (`report mfe` 30/122); P&L swung −3,074,700 over 47 fewer
  trades → the removed cohort was net-winning. Win rate barely moved while expectancy
  collapsed — the floor removed winners, not the loser tail. Unlike lever 3 (a *correct*
  width signal swamped by coverage), the RVOL signal is **itself backwards** on this
  sample, compounded by its own history-coverage cull — so no coverage-decoupling fix
  rescues it. Entry *quality via RVOL* joins entry timing (lever 4), entry quality via
  OR-width (lever 3), and stop geometry (U7) as falsified; close-confirm (v16) is the
  only kept lever.
- **Queue re-rank (R6) — the 0.0-sentinel param queue is EXHAUSTED.** Baseline **stays
  v16**. Three consecutive entry-quality param flips (timing, width, RVOL) have now
  reverted; close-confirm remains the sole kept lever. All five queue levers are spent
  (leg-1 midpoint falsified U7; lever 2 kept; levers 3/4/5 reverted; leg-2 ATR stop
  demoted). **The honest call: the next motivated turn is a CODE turn, not another param
  flip** — strongest candidate the noted decouple-OR-width-from-ATR-availability turn
  (lever 3 found a *clean* width signal a param flip couldn't isolate from the coverage
  cull). RVOL is NOT a decoupling candidate — its signal is inverted, so no coverage fix
  helps. Alternatively a new mechanism outside the five-lever frame.
- **Provenance:** baseline run `20260712T022255Z-backtest-orb-v16`, flip run
  `20260712T034649Z-backtest-orb-v19`, diagnostic probe (v900) run+deleted (home
  `data/turn4-fresh`, gitignored). Offline, no gateway.

## Turn — lever 3: OR-width sanity (2026-07-12) — plan 2026-07-11-001

- **Verdict: REVERT (lever 3 fails the edge gate — expectancy collapses).**
  `or_width_max_atr` 0.0 → **0.666** (p80 of the range_R/ATR distribution) as a
  single-param seed-and-rerun flip from the v16 baseline. Pre-registered value +
  keep rule before the run (R3). The gate rejects a session when `range_R >
  or_width_max_atr · prior_ATR` — a too-wide/choppy opening range.
- **Diagnostic-first (no prior OR-width report existed).** A throwaway probe
  (`or_width_max_atr = 0.01`, deleted) measured, over the v16 sessions: **ATR
  coverage 64.1%** (341/532 have a positive prior ATR) and range_R/ATR by outcome
  (target med 0.43 narrowest, time_exit 0.57 widest — wide OR ↔ drift, as
  hypothesized). Chose **0.666 = p80** to trim the widest ~20% tail; on v16 trades
  the *width* kills were 20 time_exit + 3 stop_hit, **0 targets** (clean). The probe
  also predicted the confound: **62 of v16's 174 trades are `atr_unavailable`** and
  winner-rich (39% target rate vs 31% overall) — a cohort the gate fail-closes at any
  threshold > 0.
- **Type: PARAM turn, not a code turn.** `strategy_code_hash fa7733f6df76ca39…`
  unchanged between baseline and flip; zero `orb.rs` edits (hash lock, KTD8). The
  OR-width gate already shipped in the harness code turn (#121); this only moves the
  param. Seed-and-rerun (not a governed `LS_TURN_PARAM` turn) because the 0.0 → 0.666
  move is an infinite relative change off the 0.0 sentinel (guardrail fail-closes it).
- **Seed from v16, NOT the registry head.** Latest finalized was v17 (reverted
  lever 4); the seed took v16's params (`entry_cutoff_min` back to 0.0) + the flip,
  dated after v17 so `latest_finalized_run` picked it. Next version = v18.
- **AE2 attribution:** `runs compare` param mode (**v16** → v18) **PASS**, diff exactly
  `{or_width_max_atr, strategy_version}` — clean single-lever attribution, code hash
  identical (comparing v17 would have shown a spurious `entry_cutoff_min` delta).
- **Bind check — BINDS but CONFOUNDED:** v18 `decisions.jsonl` carries **68**
  `filter:"or_width_atr"` SessionRejects (the gate firing on width — not inert) **plus
  191** `atr_unavailable` rejects. The ATR-coverage cull dominates numerically (191 vs
  68) — exactly the pre-registered confound. Closed trades 158 → 94.
- **Edge gate (`EdgeEvaluation`, unchanged): NOT cleared (`is_edge = false`).**
  Expectancy **−24,074.73** KRW/trade (v16 +4,812.74), PF 0.80 (from 1.044), pnl_total
  **−2,238,950** (from +755,600), Sharpe −1.99 / Sortino −2.91, WR 42.6% (from 49.4%).
  Dominance 6.1% (≤ 40% passes, but moot — expectancy is deeply negative).
- **Read — the ATR-coverage confound swamps the clean width signal (falsification):**
  the gate is inseparable from its `atr_unavailable` fail-closed arm, so turning on
  OR-width took out the winner-rich 36% lacking a positive prior ATR (a *coverage*
  property, not a *width* property) alongside the drift/loser tail. The clean width
  component (68 rejections, drifters+losers) can't compensate; expectancy collapses.
  Genuine falsification of the OR-width gate **as coupled to ATR availability** — on
  this gappy small-cap sample the ATR-normalization confound dominates. Decoupling
  (skip-not-reject on `atr_unavailable`, or an absolute non-ATR width gate) would be a
  **code turn**, out of scope. Entry *quality via OR-width* joins entry *timing*
  (lever 4) and stop *geometry* (U7) as falsified; entry *quality via close-confirm*
  (v16) remains the only kept lever.
- **Queue re-rank (R6):** baseline **stays v16** (revert; future turns seed from v16).
  New head = **lever 5 (opening-window RVOL, `rvol_min`)** — a volume/context filter,
  but it carries its **own coverage confound** (`rvol_insufficient_history`
  fail-closes short-history priors); run the diagnostic-first discipline again before
  pre-registering. Lever 1 leg 2 (ATR stop) stays **demoted**. Noted (not queued): a
  future code turn to decouple OR-width from ATR availability, so the clean width
  signal the diagnostic found can be tested without the confound.
- **Provenance:** baseline run `20260712T022255Z-backtest-orb-v16`, flip run
  `20260712T032737Z-backtest-orb-v18`, diagnostic probe (v900) run+deleted (home
  `data/turn4-fresh`, gitignored). Offline, no gateway.

## Turn — lever 4: entry cutoff (2026-07-12) — plan 2026-07-11-001

- **Verdict: REVERT (lever 4 fails the edge gate — expectancy turns negative).**
  `entry_cutoff_min` 0.0 → 120.0 (11:00 KST = range_open 09:00 + 120) as a
  single-param seed-and-rerun flip from the v16 baseline. Pre-registered value +
  keep rule before the run (R3).
- **Type: PARAM turn, not a code turn.** `strategy_code_hash fa7733f6df76ca39…`
  unchanged between baseline and flip; zero `orb.rs` edits (hash lock, KTD8). The
  entry-cutoff gate already shipped in the harness code turn (#121); this only
  moves the param. Seed-and-rerun (not a governed `LS_TURN_PARAM` turn) because the
  0.0 → 120.0 move is an infinite relative change off the 0.0 sentinel and the
  proposal guardrail (PROPOSAL_BOUNDS_CAP 0.5) fail-closes it.
- **AE2 attribution:** `runs compare` param mode (v16 → v17) **PASS**, diff exactly
  `{entry_cutoff_min, strategy_version}` — clean single-lever attribution, code hash
  identical.
- **Bind check — BINDS (not inert):** v17 `decisions.jsonl` carries **350**
  `filter:"entry_cutoff"` SessionReject decisions (v16: 0). Closed trades 158 → 123
  — the cutoff removed 35 entries. Real selectivity, not an inert filter.
- **Edge gate (`EdgeEvaluation`, unchanged): NOT cleared (`is_edge = false`).**
  Expectancy **−2,658.54** KRW/trade (v16 baseline +4,812.74), PF 0.979 (from
  1.044), pnl_total **−327,000** (from +755,600), Sharpe −0.15 / Sortino −0.23.
  Dominance 6.76% (≤ 40% passes, but moot — expectancy is negative). Win rate ticks
  *up* 49.4% → 50.4% even as expectancy falls.
- **Read — the removed late entries were net winners (falsification):** the cutoff
  did shrink the targeted cohort — `report mfe` `time_exit` bucket 88 → 60 — but
  aggregate P&L swung −1,082,600 over only 35 fewer trades, so the ~35
  late-triggering (post-11:00) entries carried ~+1.08M of net profit. The late
  breakouts were disproportionately the productive runners, not the give-back
  drifters the MFE read implied; refusing them throws out winners with losers.
  Entry *timing* joins stop *geometry* (U7) as a falsified dimension; entry
  *quality* (close-confirm, v16) is still the only kept lever.
- **Queue re-rank (R6):** baseline **stays v16** (revert; future turns seed from
  v16). New head = **lever 3 (OR-width sanity, `or_width_max_atr`)** — an
  entry-quality sibling of the kept close-confirm lever, ATR-hardened by F1, and the
  class the loop has evidence for; then lever 5 (RVOL). Both are 0.0-sentinel moves
  → seed-and-rerun. Lever 1 leg 2 (ATR-scaled stop) stays **demoted**.
- **Provenance:** baseline run `20260712T022255Z-backtest-orb-v16`, flip run
  `20260712T031104Z-backtest-orb-v17` (home `data/turn4-fresh`, gitignored). Offline,
  no gateway.

## Turn — lever 2: close-confirmed entry (2026-07-12) — plan 2026-07-11-001

- **Verdict: KEEP (lever 2 clears the edge gate — the loop's first positive edge).**
  `entry_confirm` 0.0 → 1.0 (wick-touch → close-confirmed) as a single-param
  seed-and-rerun flip from the v15 all-off baseline, riding the F1/F2 flip-precondition
  fixes landed this same code turn.
- **Code turn — two flip-preconditions landed** (`docs/solutions/logic-errors/orb-atr-and-close-confirm-flip-preconditions.md`),
  re-baselining `strategy_code_hash` to `fa7733f6df76ca39…` (was `e3812d4f…`):
  - **F1 (mechanical):** non-positive prior ATR (`Some(0.0)` from flat/halted priors)
    treated as unavailable in **both** the ATR-stop and OR-width arms of
    `session_gate_reject`; ATR stop distance floored at 1 (`.max(1)`) so a tiny
    `mult·ATR` can't round to 0 and collapse the stop onto the entry.
  - **F2 (decision, approved):** in close-confirm mode the fill is close-anchored, so
    the entry bar's stop-touching low is provably **pre-fill** — the same-bar stop check
    is skipped there (wick mode unchanged). A deliberate deviation from KTD6's
    wick-entry "same-bar stop-first wins"; without it the flip books phantom same-bar
    stops on confirm bars and the verdict biases toward revert.
- **Re-baseline (v15, all-off) — verdict-neutral:** seed-and-rerun from v13 (KTD2),
  all gates off, `strategy_version = 15`. Per-trade ledger vs `…-v9` is **166/166
  byte-identical**; summary EQUAL on every field (`num_trades 162`,
  `Expectancy −3157.547`, `WR 0.4691`, `PF 0.9729`, `pnl_total −502050`,
  `max_drawdown 9149100`). The F1/F2 fixes touch only the ATR/close-confirm paths, so
  the all-off baseline is unchanged. Re-baseline signal captured: `runs compare`
  param mode (v9 → v15) FAILs `strategy_code_hash differs`, `param diff
  ["strategy_version"]` (KTD3 — the FAIL *is* the evidence).
- **AE2 attribution:** `runs compare` param mode (v15 → v16) **PASS**, diff exactly
  `{entry_confirm, strategy_version}`. v15 and v16 share `strategy_code_hash
  fa7733f6…` — a pure param flip (F1/F2 in both, verdict-neutral for all-off).
- **Edge gate (`EdgeEvaluation`, unchanged): CLEARED (`is_edge = true`).** Expectancy
  **+4,812.74** KRW/trade (baseline −3,157.55), WR 49.4% (+2.4 pp), PF **1.044**
  (from 0.973), pnl_total **+755,600** (from −502,050), Sharpe +0.45 / Sortino +0.75.
  Dominance **11.5%** (≤ 40%), top-|P&L| symbol is a *loser* (`034730.XKRX`
  −1,865,000) — the edge is not one-winner-carried. Trades 162 → 158: close-confirm
  trims only 4 wick-only breakouts yet flips aggregate P&L positive — those entries
  were net-losing fakes. R-metrics range-R (stop_mode 0, per `report mfe`).
- **Read:** the mirror of the U7 midpoint-stop falsification — these breakouts pull
  back through the OR mid before running, so tightening the *stop* converts winners to
  losses, but tightening the *entry* (demanding a confirmed close) avoids the fakes
  without surrendering the runners. Entry quality, not stop geometry, is the lever.
- **Queue re-rank (R6):** baseline advances to **v16** (`entry_confirm = 1.0`; future
  turns seed from it). The kept entry-quality lever validates the entry-quality class
  over stop-geometry. New head = **lever 4 (entry cutoff)** — the `report mfe`
  `time_exit` bucket (n=88, median give-back 0.40R) is the give-back-to-flat cohort a
  cutoff targets; then lever 3 (OR-width, now ATR-hardened by F1), lever 5 (RVOL).
  Lever 1 leg 2 (ATR-scaled stop) stays **demoted** (a stop-narrowing sibling of the
  falsified midpoint), though F1 now makes it runnable.
- **Provenance:** baseline run `20260712T022149Z-backtest-orb-v15`, flip run
  `20260712T022255Z-backtest-orb-v16` (home `data/turn4-fresh`, gitignored). Zero
  `orb.rs` edits after the v15 baseline run (KTD8). Offline, no gateway.

## Turn — mechanism-harness all-off baseline (2026-07-12) — plan 2026-07-11-001 (U6)

- **Verdict: re-baseline PASS (reconciled to v9).** The harness code turn adds
  queue levers 1–5 to `orb.rs` as default-off gates (`stop_mode`, `entry_confirm`,
  `or_width_max_atr`, `entry_cutoff_min`, `rvol_min` + companions), plus the U2
  candidate seam (prior-daily ATR, opening-window RVOL) and the U5 stop-mode report label.
- **Type:** code turn — `strategy_code_hash e3812d4f…` (v9 was `d54955a8…`); re-baselined
  via seed-and-rerun (KTD2), all gate params at filter-off defaults, `strategy_version = 13`.
- **R3 / AE1 reconcile:** per-trade ledger vs `…-v9` is **166/166 trades byte-identical**;
  summary equal on every field (`num_trades 162`, `Expectancy −3157.547`, `WR 0.4691`,
  `PF 0.9729`, `pnl_total −502050`, `max_drawdown 9149100`). No gate fires at defaults.
- **Re-baseline evidence:** `runs compare` param mode (pinned v9 → v13) FAILs as expected —
  `param diff ["strategy_version"]`, `FAIL: strategy_code_hash differs`. No compare mode
  passes a code turn (KTD3); the FAIL *is* the evidence.
- **Provenance:** run `20260712T012320Z-backtest-orb-v13` (home `data/turn4-fresh`, gitignored).
  Zero `orb.rs` edits after this run (KTD8).

## Turn — first flip: OR-midpoint stop (2026-07-12) — plan 2026-07-11-001 (U7)

- **Verdict: REVERT (lever 1 leg 1 falsified).** `stop_mode` 0.0 → 1.0 (OR-midpoint) as a
  single-param seed-and-rerun flip from the v13 baseline.
- **AE2 attribution:** `runs compare` param mode (v13 → v14) **PASS**, diff exactly
  `{stop_mode, strategy_version}` — clean single-lever attribution.
- **Edge gate (`EdgeEvaluation`, unchanged): NOT cleared.** Expectancy −28,983 KRW/trade
  (baseline −3,157), WR 37.7% (−9.2 pp), PF 0.72 (from 0.97), pnl −5,159,050. Dominance
  8.3% (≤ 40% passes, but expectancy is deep-negative). Trades 162 → **183** — the tighter
  stop fires earlier and frees `max_concurrent` slots, admitting more losing entries (turn-10
  caveat), not more winners.
- **Read:** the midpoint stop is the *noise* branch of the brainstorm frame — these breakouts
  pull back through the OR midpoint before running, so a midpoint stop converts winners/time-exits
  into losses. R-metrics labeled trade-R (AE3), not compared against v9/v13 range-R.
- **Queue re-rank (R6):** the tighter-stop hypothesis is falsified, demoting the sibling
  ATR-scaled stop (leg 2, also narrows the stop). New head = **lever 2 (close-confirmed entry)**,
  an orthogonal entry-quality mechanism the midpoint result implicates. Baseline stays v13 (== v9).
- **Flip preconditions (post-review):** code review found two latent modeling bugs in the
  default-off ATR-stop and close-confirm paths (unreachable by v13/v14) that must be fixed —
  riding their flip's re-baseline — before those levers run, or the flip verdict is biased.
  See `docs/solutions/logic-errors/orb-atr-and-close-confirm-flip-preconditions.md`.
- **Provenance:** run `20260712T012616Z-backtest-orb-v14` (gitignored). Falsified run retained.

## Turn 3 — broaden-sample data turn (2026-07-07) — plan 2026-07-07-003

- **Verdict: insufficient-evidence.** The pre-registered R1 decisiveness bar was not cleared.
- **Type:** pure data turn — v3 params held exactly (`gap_min_pct = 0.6`, `strategy_version = 3`); zero param diff.
- **Sample:** 20 KOSPI top-market-cap names (frozen `t1444` capture, upcode 001, `lab/config/turn3-universe.json`) over 28 sessions `2026-05-26..2026-07-03`, fresh data home, daily + 1-minute bars (all 20 symbols, `catalog status` GO, no front-truncation).
- **Result:** 6 realized trades across 6 distinct symbols (1 each); `pnl_total` +320,000 KRW (Profit Factor 1.85).
- **R1 bar (computed, not eyeballed):**
  - (a) trade-count floor (≥ 30): **6 → FAIL**
  - (b) symbol-breadth floor (≥ 6 symbols each ≥ 2 trades): **0 → FAIL**
  - (c) single-symbol dominance (≤ 40% of aggregate |P&L|): **33.7% → PASS**
- **Reproducibility:** data-mode `runs compare` PASS ("no data deltas") vs an identical v3-wide rerun — determinism confirmed; run manifest carries `gap_min_pct = 0.6` / `strategy_version = 3` (v3 identity, KTD-3/KTD-5).
- **Bar integrity (R3):** the bar was fixed in the plan before the run and was not adjusted to the result.
- **Next (deferred):** the param turn lowering `gap_min_pct` from 0.6 toward ~0.3 (governed relative-change step within the 0.5 bounds cap) to admit more sessions, and/or a deeper/wider sample. Do not tune against this 6-trade sample.
- **Provenance:** run `20260707T075947Z-backtest-orb-v3` (fresh home `data/turn3`, gitignored).

### Context — turn 2 (prior)

v3 (`gap_min_pct = 0.6`) was the first floor to admit a fill: 1 trade on `005930` over a 12-session pinned range → verdict insufficient-evidence at n=1. Turn 3 broadened the sample to move the verdict off "insufficient by construction"; the broadened read still misses the trade-count and breadth floors, so the class holds — but now as a measured result, not an n=1 artifact.
