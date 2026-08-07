# Strategy-loop turn log

Committed record of each loop turn's verdict + the bar conditions it held. The
full artifacts (`analysis.md`, manifests, performance) live in the gitignored data
home; this file is the durable, reviewable outcome trail.

## Head lineage (STANDING) — post-#118 real-data head = v34; pin `LS_TURN_EXPECT_VERSION=34` (2026-07-24)

> **AMENDED 2026-07-31 (orb-transaction-cost-model): the documented head is now `v35`
> (`20260731T023138Z-backtest-orb-v35`, `strategy_code_hash 7571abef…`) — pin
> `LS_TURN_EXPECT_VERSION=35`.** v35 is the SAME head identity (the v32-lineage governed
> params) re-measured honestly with the transaction-cost model armed — the same
> identity-preserving relationship v34 had to v32 at the #213 re-baseline, not a new
> strategy. Everything below this note remains true of the v34 era; see the 2026-07-31
> turn entry for the cost-aware numbers and the catalog-drift finding.

Canonical answer to "which of the two 2026-07-24 runs is THE head?", resolving the
`v33` vs `v34` ambiguity in one place (deferred item #2 from plan `2026-07-24-001`;
the "pin EXPECT_VERSION" operator TODO from #118). This is a documentation +
version-pin decision only — **no backtest, no `orb.rs`/`params.rs` edit, head
*identity* unchanged**.

- **THE documented real-data head = `v34`** — as of the #213 re-baseline, the head *run* is
  `20260725T112423Z-backtest-orb-v34` (`strategy_code_hash e5bc2ae8…`); it superseded
  `20260724T014752Z-backtest-orb-v34` (`d7a9820b…`) when the live-session driver's per-bar
  wiring landed in `orb.rs` and moved the file-scoped head identity. The two runs are
  **byte-identical** in `performance.json` and `data_quality.json`, and their manifests differ
  only in `run_id` / `strategy_code_hash` / `lab_src_fingerprint` / `created_utc` — so the
  strategy is unchanged and every v34 number below still holds: catalog fingerprint
  `363f199d`, 119 closed trades, size-invariant real-data RoR **0.0398**. It is what the
  profit-target-075 turn already anchored its KEEP baseline on (KTD5 of plan
  `2026-07-24-001`). The re-baseline is why the frozen pre-registration's v34 band citation
  (which names the superseded run) remains valid.
- **`v33` is the #118 tier-power GATE reference, NOT the head**
  (`20260724T014624Z-backtest-orb-v33`, 259 closed trades, VERDICT 🟢 GREEN — ≥30
  trades in ≥2 tiers). It answers "does the universe engine yield enough trades per
  tier?", not "what is the head strategy's real-data return."
- **Why the trade counts differ (the confusion this closes): the two runs do NOT
  share params.** They share only the *ingested catalog* (fp `363f199d`). The #118
  gate adopted **v9** params (`LS_BT_PARAMS_FROM_RUN=…-backtest-orb-v9
  LS_BT_VERSION=33` — the metadata-driven count identity) while the twin adopted the
  **v32 head** params (`…-backtest-orb-v32 LS_BT_VERSION=34`). v9 is less selective
  (259 trades); the v32 head is more selective (119 trades). So `v34` is the faithful
  real-data measurement of the *actual head strategy* (v32 params on real bars) —
  which is exactly why it, not `v33`, is the head anchor. (Note: `v32`
  `20260717T094841Z-backtest-orb-v32`, RoR 0.1876, remains the head *identity* in
  old-data terms; `v34` is that identity re-measured on real data.)
- **Machine constraint that makes v34 both correct and cheap.**
  `latest_finalized_run()` (`src/runner/research.rs:105`) returns the newest run by
  `run_order_key` = **v34** and cannot return `v33` without a *new* finalized run
  (out of this task's scope — that would be a data/breadth turn, not a doc decision).
  Per the count-run warning at `src/runner/backtest.rs:944-964`, an adopted-params
  count run finalizes under a distinct version and "finalizes as the LATEST run (vN)
  — pin `LS_TURN_EXPECT_VERSION` on the next turn accordingly." Here the *twin* (v34)
  is latest-finalized, so the pin must name it explicitly.
- **PIN for the next governed turn: `LS_TURN_EXPECT_VERSION=34`.** The turn's seed
  assertion (`src/runner/research.rs:1910`) will stop hard on a mismatch rather than
  silently resolving its baseline from whichever run is newest by accident. KEEP/REVERT
  comparisons are unaffected by v34's #118 "RED" *power*-label — a KEEP is a relative
  comparison against v34's `0.0398`, and the power-label speaks only to per-tier trade
  counts (KTD5).

## Turn — paired power (measurement axis): the PAIRED question is ALSO unanswerable at this sample — 0 of 6 off-flip arms attributable; the ORB stand-down now closes on MEASUREMENT, head stays v35 (2026-08-07) — plan 2026-08-07-001

- **What did NOT change.** No strategy code, no governed param, no ingest, no gateway call,
  no backtest re-run, no acquisition. `strategy_code_hash` unchanged at `7571abef…`; head
  stays **v35**; `config/preregistration.json` byte-identical at `abdb90a1…`. The turn's
  output is a new verb (`lab-research report paired`), a committed fixture, and this record.

- **Why this turn existed.** The 2026-08-06 verdict measures **absolute** detectability — is
  the head's edge distinguishable from zero (no, by ~9×). Every lever turn asks a *different*
  question: does armed beat off-flip **over the same sessions**. That is a paired comparison
  with its own standard error, and nothing in the tree had measured it. So "at this effective
  sample size nothing is attributable to anything" was established for absolute attribution
  and **untested** for paired attribution. This turn tests it. It was a hypothesis that the
  paired SE might be small enough to rescue lever work; it is not.

- **The measurement.** Head `20260731T023138Z-backtest-orb-v35` (111 closed trades over 24
  KST sessions, range `20260518..20260722`) against the six 2026-07-31 cost-aware off-flip
  arms, all seven sharing catalog `ac026541…`, universe `2dfc00d7…` and code `7571abef…`.
  Paired session-block bootstrap of the **net-RoR difference**, blocks over the **union** of
  the sessions either arm traded, 10,000 replicates, seed 20260805, 95% confidence:

  | arm | lever flipped | union / ∩ | delta | paired SE | z·SE | min. detectable paired diff | verdict |
  |---|---|---|---|---|---|---|---|
  | v92 | `entry_confirm` 1.0→0.0 | 25 / 24 | +0.031857 | 0.037306 | 0.073118 | +0.104515 | NOT attributable |
  | v93 | `or_width_max_atr` 0.666→0.0 | 26 / 24 | +0.023715 | 0.018408 | 0.036078 | +0.051570 | NOT attributable |
  | v94 | `breakeven_trigger_r` 0.41→0.0 | 24 / 24 | +0.081045 | 0.070970 | 0.139100 | +0.198830 | NOT attributable |
  | v95 | `risk_per_trade_krw` **and** `ratio_atr_alpha` — **CONFOUNDED** | 24 / 24 | +0.047258 | 0.026605 | 0.052144 | +0.074535 | NOT attributable |
  | v96 | `ratio_atr_alpha` 1.0→0.0 | 24 / 24 | +0.026933 | 0.018911 | 0.037065 | +0.052981 | NOT attributable |
  | v97 | `gap_retention_min` 0.5→1.0 | 41 / 24 | +0.058478 | 0.067473 | 0.132244 | +0.189030 | NOT attributable |

  **0 of 6 per-arm; 0 of 6 family-wide** (Bonferroni over six arms, z 2.6383). Every arm's
  own difference lands between 0.44× and 0.91× its bar — near-misses, not a degenerate
  standard error, which is asserted as a band in `tests/paired_power.rs` so a future estimator
  returning an enormous SE could not pass this as "not attributable" for the wrong reason.

- **VERDICT ROUTING: the "no, for every arm" exit. The ORB arc IS sample-blocked for lever
  work, and the stand-down now rests on a measurement of the right question rather than on a
  calculation about a different one.** The stand-down recommendation is **not** withdrawn.

- **The KTD11 scope gate is what makes that conclusion narrow enough to be true.** These six
  arms are whole-lever-**OFF** flips — by construction the largest effects the design space
  contains, deltas of 0.024 to 0.081 against a head gross edge of +0.028422. The smallest
  minimum detectable paired difference across them is **+0.0516**, which is **1.8× the head's
  entire gross edge**. A marginal lever turn moves a fraction of that edge, so this sample
  cannot resolve one even in principle. Detecting a whole-lever flip would not have
  established that a marginal turn is measurable; failing to detect one settles it.

- **At the reachable supply, 5 of 6 would flip — and it still does not buy lever work.**
  Scaling the paired SE by `sqrt(45 / 237)` = 0.435745 (the head's in-range **calendar**
  sessions over the vendor's reachable ceiling — 45, *not* the 24 trade-producing ones):

  | | at the 45 sessions held | projected to 237 |
  |---|---|---|
  | arms attributable, per-arm | **0 of 6** | **5 of 6** |
  | smallest minimum detectable paired difference | +0.051570 | **+0.022472** |

  So a max-depth pull *would* buy paired attributability for lever-**OFF**-sized effects,
  which is more than it buys for absolute detectability (where it changes no decision at all).
  But at 237 sessions the paired detection floor is still **+0.0225 — roughly the size of the
  entire head gross edge (+0.0284)**. The arc would remain unable to adjudicate a marginal
  lever turn. v92 is the one arm that stays unattributable, and by 5×10⁻⁶ (+0.031857 against a
  bar of +0.031861) — a coin flip, reported as a figure and not as a finding.
  This is a **projection** under an unchanged clustering structure and an unchanged effect,
  not a measurement.

- **Attributability here means out-of-sample replication over the session-generating
  process** — the delta would survive a different draw of sessions from the same regime. The
  arms are deterministic re-simulations on identical bars, so **no part of this entry is a
  causal claim** about a lever on the sessions actually held (R9).

- **v95 is reported as CONFOUNDED, not dropped (KTD6).** Its manifest flips two params —
  `risk_per_trade_krw` 299,340→0.0 **and** `ratio_atr_alpha` 1.0→0.0 — while
  `sample-margin.json` labels it by the first alone. The verb derives every arm's label from
  the manifest param diff against the head (excluding `strategy_version`, which differs on
  every arm by construction), so the confound is printed rather than assumed away. Dropping
  the arm would have silently moved the frozen record's arm count from six to five.

- **Three corrections to what the queue item recorded**, verified against the artifacts on
  disk: there are **six** off-flip arms and not seven (`cross_trial_arms[0]` is the head
  itself); the head run lives under `data/turn4-fresh/`, not `turn4-cost-scratch`; and the
  arms are **not** uniform at 113 trade records — they carry 104 to 254 closed trades over 24
  to 41 sessions against the head's 111 over 24. `20260731T022007Z-…-v90` and `-v91` are
  excluded: their `performance.json` files carry no cost-model fields, so pairing them would
  confound the lever flip with the cost model.

- **What guards this.** `tests/paired_power.rs` recomputes every figure above from
  `tests/fixtures/paired-arms-closed-trades.json` through `stats.rs` — hermetic, so it runs in
  CI where `data/` does not exist — and the fixture path reproduces this live run to six
  decimals. Verified by mutation, since a fixture-derived test passes before and after a
  behavior change: replacing the paired draw with two independent draws reds two assertions,
  and rooting the projection in 24 sessions instead of 45 reds a third.

- **Queue.** `orb-paired-power-measurement` closes with this entry.
  **`orb-sample-acquisition-decision` stays OPEN** — this measurement is evidence for it, and
  the arm selection is the operator's. What it now knows that it did not: arm B (the
  ~237-session max-depth pull) buys paired attributability for whole-lever flips and still
  does not buy lever work, so the case for it is weaker than "it changes no decision" implied
  and still not a case for spending a multi-day gateway budget.
  `report-sample-catalog-read-metadata-only`'s priority falls: its stated justification is the
  65× catalog growth this arc now says will not happen.

## Turn — sample sufficiency (measurement axis): the head's edge is BELOW ITS OWN DETECTION FLOOR; required sample UNREACHABLE, acquisition STANDS DOWN, head stays v35 (2026-08-06) — plan 2026-08-05-001

- **Verdict: the sample question gates every lever question, and the answer is that this
  sample cannot resolve this edge.** No strategy code, no governed param, no ingest, no
  run. `strategy_code_hash` unchanged at `7571abef…`; head stays **v35**. The turn's output
  is a report (`lab-research report sample`), a frozen margin, and this record.

- **The near-miss framing was wrong.** v35 reads a 22.8 bps gross edge against a 23 bps
  round-trip hurdle — a ~0.9% shortfall, which invites "find 1% more edge and the sign
  flips". The head's own artifact says otherwise. Measured on
  `20260731T023138Z-backtest-orb-v35` (catalog `ac026541`, 111 closed trades over 24 KST
  sessions):

  | | |
  |---|---|
  | per-trade net r | mean **−0.033320**, sd **0.641523** |
  | per-trade gross r | mean **+0.028422** |
  | intra-session correlation (one-way ANOVA) | **0.327334** at Kish cluster size **4.5374** |
  | design effect | **2.1579** → effective n **51.44** of 111 |
  | minimum detectable edge (95% / 80%) | **+0.2506 R** |
  | required closed trades at the gross edge | **8,629** (naive 3,999 × design effect) |
  | required sessions at 2.4667 trades per **calendar** session | **3,499** (~14.0 years) |

  > **Unit correction, same turn (review finding).** An earlier draft of this entry — and the
  > plan's own Problem Frame — divided the required trade count by **4.625 trades per
  > *trade-producing* session** and compared the result against *calendar* coverage, giving
  > ~1,870 sessions (~7.5 years). Those are different units. The head trades on only 24 of the
  > **45** calendar sessions its data range covers, so the honest rate is **2.4667 per calendar
  > session** and the requirement is **3,499 sessions**, not 1,866. The verdict direction is
  > unchanged — it is a stand-down either way — but the shortfall is roughly **double** what
  > the first reading said, and one band row flips (see below). `report sample` now prints both
  > rates and uses only the calendar one for a verdict.

  The smallest edge this sample can distinguish from zero is **roughly nine times the
  entire gross edge** — and that is with costs switched *off*. **The problem is not the
  cost model.** Two consequences: any lever reporting net RoR > 0 here has about even odds
  of doing so by chance (session-block bootstrap: 95% interval [−0.181, +0.165], share of
  replicates above zero **0.4955**), and the previous unblock condition `net RoR > 0` was
  therefore satisfiable by luck half the time. The standing claim that the negative edge
  "is not attributable to any kept lever" is correct, but not because the levers are
  individually sound — **at this effective sample size nothing is attributable to
  anything.**

- **SUPPLY: STAND DOWN.** 3,499 sessions required against **54** distinct KST daily-bar
  sessions in the catalog — a shortfall of **3,445 sessions (~13.8 years)**. Reported across
  the gross edge's own 95% interval rather than at one point, because required n scales as
  the inverse square of the target:

  | target effect | required n | sessions | years | within coverage |
  |---|---|---|---|---|
  | +0.204107 R (CI upper, design-effect corrected) | 168 | 68 | 0.3 | NO |
  | +0.148018 R (CI upper, naive) | 319 | 129 | 0.5 | NO |
  | **+0.028422 R (the pinned target)** | **8,629** | **3,499** | **14.0** | **NO** |
  | −0.091 R / −0.147 R (CI lower) | undetectable at any sample size | | | |

  **No row is reachable.** On the earlier trade-producing-session rate the top row read 37
  sessions and "yes" — i.e. the most optimistic end of the interval looked already satisfied.
  It is not: at 68 calendar sessions it exceeds the 54 covered. That flip is the practical
  cost of the unit error and the reason the correction was worth making.

  An unreachable sample is a **valid completion**, not a failure. Nothing was acquired and
  no ingest ran; a test asserts on `report_sample`'s source that no branch reaches an
  acquisition entry point, because an ingest call could run without printing anything.

- **The recommendation is HISTORY, not breadth — and it is a fresh catalog, not an
  extension.** At ICC 0.327 and cluster size 4.54, added sessions raise effective n roughly
  in proportion while added breadth adds trades inside blocks already held; `max_concurrent
  7` caps how much breadth converts at all. And the history cannot be extended
  incrementally — `accumulate` never fetches below the watermark — so the acquisition is a
  fresh catalog at a wider lookback, whose cost includes a moved fingerprint, a moved
  universe hash and a moved data range, and with them the loss of comparability with every
  prior measurement, this one included. That is its own budget-bound decision; **this turn
  stops at the verdict** (KTD7).

- **Pre-registered margin FROZEN — `config/sample-margin.json` + `SAMPLE-MARGIN.md`.**
  Replaces the gameable `net RoR > 0`:

      net RoR  >  E[max of 29 null trials]  +  z(95%) · SE(candidate)

  Bailey & López de Prado's False Strategy Theorem at the frozen trial count (29, every
  record in `ledger/trials.jsonl` via `trials::count_trials`) and cross-trial dispersion
  (0.02636794, the sample sd of the seven same-catalog net-RoR arms in the 2026-07-31
  off-flip table). **A rule, not a level:** a scalar scaled at 111 trades would be
  unclearable at any sample size, which strands a viable strategy permanently — only the
  *selection*-bias inputs are frozen, and the sampling term shrinks as the candidate's own
  sample grows. Frozen **before** any candidate is read, and it **refuses the current
  head**: v35's net RoR −0.000607 against a threshold of +0.224823 at its own SE 0.087002.

  Empirically calibrated, not asserted (KTD10): over 1,000 max-of-29 null blocks built by
  permuting the centred per-trade R-multiples and session-block resampling, the realized
  clearance rate is **0.0140** against a **0.0250** nominal — and a bar set at 2·SE instead
  clears at **0.1060**, four times nominal, so the calibration discriminates a bar set too
  low. Disarming the comparison through an in-process seam reds the assertion (1.0000).

- **`config/preregistration.json` is byte-identical** (KTD3, now pinned by SHA-256 in
  `tests/sample_margin.rs`). The margin gets its own container: the amendment protocol's
  no-consumer test forbids re-deriving a frozen artifact whose honest value would forbid
  the activity it gates, and the ladder is already stood down.

- **KTD2 deviation, recorded.** It asks for the trial count scoped to the v35 catalog
  lineage. That scoping is unavailable against the committed ledger: v35's fingerprint
  appears in no record and none links into it (roots are `era-167` 19, `3b6be31b` 8,
  `363f199d` 2). The 2-record slice would set E[max] at 0.0137 instead of 0.0543 — its
  lowest available value — and an undercounted trial count setting the bar too low is a
  named risk. The strict whole-ledger count is frozen; the reason is recorded in the record
  itself and guarded by a test.

- **Queue.** `rung1-ladder-reentry-net-positive-head` is superseded by
  `rung1-ladder-reentry-margin-clearing-head`, whose unblock condition carries the margin
  rather than a bare sign test. The verdict's successor —
  `orb-sample-acquisition-decision` — is staged so neither branch terminates without a
  `make next`-visible next step.

## Falsified candidates — two pieces of in-tree guidance retired so the next agent is not sent at an already-dead lever (2026-08-06) — plan 2026-08-05-001 (U7)

Documentation and one doc comment only. **No behavioral edit**: `strategy_code_hash` is
unchanged before and after (`7571abef…`), so head identity does not move.

- **FALSIFIED BY MEASUREMENT — a minimum opening-range-width entry filter. It has no
  population to cut.** The idea is that a breakout whose range `R` is narrow relative to
  the round-trip cost cannot pay for itself, so those entries should be filtered out.
  Measured on the v35 run (`20260731T023138Z-backtest-orb-v35`, catalog `ac026541`,
  139 breakout envelopes), under the head's armed `stop_mode = RangeLow` — where `R` *is*
  the opening-range width — the range width as a fraction of the breakout price is:

  | | min | p05 | p25 | p50 | p75 | max |
  |---|---|---|---|---|---|---|
  | OR width | 127.0 bps | 158.5 bps | 250.0 bps | 358.7 bps | 443.2 bps | 1,224.9 bps |
  | × the 23 bps round-trip hurdle | **5.5×** | 6.9× | 10.9× | 15.6× | 19.3× | 53.3× |

  **Zero of 139** breakouts sit below the hurdle, and only two below six times it. The
  *narrowest* range in the whole sample already clears the statutory + brokerage cost by
  5.5×. A minimum-width floor set anywhere near the hurdle cuts nothing; set high enough to
  cut anything, it is no longer a cost argument but an ordinary width sweep — and
  `or_width_max_atr` (kept at 0.666) already governs the width axis from the other side.
  The hurdle is 20 bps statutory sell tax + 1.5 bps/side commission
  (`cost_sell_tax_rate 0.002`, `cost_commission_rate_per_side 0.00015`).

  Do not spend a turn on it. If it is re-proposed, the answer is this table.

- **CORRECTED — the `profit_target_r` doc comment no longer advertises 1.5 as an
  unexplored optimum.** `lab/src/params.rs` described **1.5** as "the Step-0 sim optimum
  reserved for a later param-turn sweep". That sweep already ran. Turn 9 swept
  `profit_target_r` off v9 in both directions and **both legs were worse** — 1.5 gave
  expectancy −4,406 KRW/trade against v9's −3,157, and 1.05 gave −35,969; turn 11's 0.75
  leg was a further Phase-A STOP. The winner-MFE cluster peaks just above 1.0R, so 1.0 sits
  on the peak. The comment now records that, and points at
  `docs/solutions/conventions/strategy-loop-turn-9-profit-target-sweep-and-mfe-distribution.md`.

## Governance — rung-1 ladder STAND-DOWN recorded: net-negative cost-aware head; frozen prereg v2 left untouched as the historical record (2026-07-31) — queue rung1-prereg-band-zero-cost-inheritance

- **Verdict: STAND DOWN (the stand-down arm of the queue item's re-derive-or-stand-down
  choice), the recorded resolution of the zero-cost band inheritance.** The documented head is net-negative after honest costs
  (v35, net RoR **−0.0006**, entry below), and the ladder's economic gate cites the frozen
  v2 rung-1 band **[−148k, +266k]** derived from the v34 **zero-cost** distribution — a
  band whose center is ~1.69M KRW too optimistic over a backtest-length window. With a
  net-negative expected edge, attended sessions buy expected losses; **no attended
  session is authorized** until the re-entry condition below is met. This is a
  governance dispatch — no strategy code, no backtest, no band arithmetic.
- **Why stand-down and not amendment (v3) — the operator choice, argued and recorded.**
  The case for keeping rung-1 alive as an execution-calibration instrument fails on
  inspection: (1) the rung-2 tracking band that rung-1 live fills would calibrate has no
  consumer — rung-2 can never be authorized on a net-negative head; (2) the frozen
  `code_change_resets_to_rung_1` rule means the head change a net-positive cost-aware
  head requires resets the ladder to rung 1 and discards the v35-epoch rung-1 evidence
  anyway; (3) the cost model's live validation rides the first rung-1 session of the
  eventual net-positive head at no extra cost. The expected loss at 0.10× is trivial
  (≈ −1.7k KRW per 24 sessions) — the cost being refused is operator attention and
  governance integrity, not money. A v3 band centered on a negative edge would exist
  only to authorize sessions this stand-down forbids.
- **The frozen file is deliberately untouched.** `config/preregistration.json` remains
  **v2** byte-for-byte, so every existing dispatch citation (SHA-256) stays valid and
  the v2 values stand as the historical record. No cost-aware band was derived. The
  recorded stand-down lives in `config/PREREGISTRATION.md` § Stand-down (status note),
  this entry, the RUNBOOK/PREFLIGHT banners, and the queue — per the amendment
  protocol's "recorded, never implied" discipline (KTD1).
- **Re-entry condition (the unblock):** a net-positive cost-aware head exists — net
  RoR > 0 with the armed transaction-cost model on a current catalog. That is
  necessarily a code-hash move (`code_change_resets_to_rung_1`), so re-entry begins
  with a fresh re-registration (v3+): bands re-derived from that head's closed-trade
  distribution via the identical Protective formula, derivation reproduced in
  `lab/tests/prereg_derivation.rs`, before any genesis dispatch.
- **Doc sweep (this dispatch):** `RUNBOOK-rung1.md` and `RUNG1-PREFLIGHT.md` now open
  with the suspension banner (PREFLIGHT's step-3 v2/SHA-256 citation stays valid — the
  file did not change); `README.md` § rung-1 ladder records the stand-down;
  `config/PREREGISTRATION.md` carries the status line + § Stand-down. Live wiring, the
  dispatch gates, and the rung-2 fail-closed tracking band are untouched (out of scope),
  as are the frozen cost rates in `config/transaction-costs.json`.
- **Queue:** `rung1-prereg-band-zero-cost-inheritance` closes with this entry.
  `rung1-attended-session-v35` is superseded by `rung1-ladder-reentry-net-positive-head`
  (parked; unblock condition in its notes). The sibling
  `session-morning-window-gap-robustness` is untouched.

## Turn — transaction-cost model (measurement axis): head re-measured NET-NEGATIVE, head → v35, all six kept levers survive the cost-aware re-read (2026-07-31) — queue orb-transaction-cost-model

- **Verdict: the measurement executed and ships as read — the cost-aware head is NET-NEGATIVE.**
  `20260731T023138Z-backtest-orb-v35` (v34 governed params, real catalog, sourced 2026 costs):
  gross P&L **+1,669,120 KRW**, modeled cost **1,686,034 KRW**, net P&L **−16,914 KRW**,
  size-invariant net RoR **−0.0006** (gross RoR +0.0599), **111 closed trades** over
  20260518..20260722. The measured gross edge (22.8 bps of mean round-trip notional) is
  smaller than the statutory + commission term (23 bps), so the sign flips. This is a
  MEASUREMENT, not an optimization: no threshold was softened, no rate shaded, no lever
  retuned after the reading — the no-override clause holds.
- **The rates are sourced and cited, never assumed** — committed as
  `config/transaction-costs.json` (schema-gated loader, plausibility-gated by
  `OrbParams::validate()`, pinned by `tests/strategy.rs::committed_config_artifact_parses_and_validates`):
  - **Sell-side statutory tax 0.0020 (20 bps of sell notional)** — 2026-01-01 rates
    (2025 세법개정, 금투세 폐지 패키지의 2023-수준 환원): KOSPI 증권거래세 0.05% + 농어촌특별세
    0.15%; KOSDAQ 증권거래세 0.20% (농특세 없음). Both boards total 20 bps, so one uniform
    rate is exact for the equities-only universe over this data range. **Sell-side only —
    the model is asymmetric by construction.**
  - **Commission 0.00015/side (1.5 bps)** — LS증권 xing API / OPEN API channel (this
    repo's actual access channel), KRX orders 0.015%/side. 유관기관수수료 (0.0036396%)
    is under a temporary exemption (2025-10-27..2026-10-26) spanning the whole data
    range and is not modeled. Full citations (URLs, retrieval date) in the artifact.
- **Placement & provenance (the gotcha, resolved deliberately):** the cost model
  (`TransactionCostModel` + config loader) lives in **`src/strategy/orb.rs`**, applied at
  backtest trade-booking (`performance.rs::trade_from_position` via
  `from_positions_with_risk`, backtest assembly path ONLY — live-session reports stay
  zero-cost because the frozen rung-1 band is zero-cost-derived, see the flag below). Both
  fingerprints moved, as code-turn semantics require:
  - `strategy_code_hash`: `e5bc2ae8…` (v34) → **`7571abef…`** (v35) — cost-aware and
    zero-cost runs can never be treated as one code lineage by `head_governed_params_pinned`.
  - `lab_src_fingerprint`: `ebfd72e2…` (pre-model HEAD; v34's own was `f476ac7e…`) → **`42aad1c7…`**.
  - The armed rates ride in the run manifest's params (`cost_commission_rate_per_side`,
    `cost_sell_tax_rate`); zero rates serialize as ABSENT (`skip_serializing_if`), so
    zero-cost manifests and `governed_params_hash` stay byte-identical to the pre-model schema.
- **Reconciliation proof (rate=0 reproduces the zero-cost number exactly):** in a scratch
  data home (`data/turn4-cost-scratch`, symlinked catalog), v34-params runs with the
  pre-model binary (v90 `20260731T022007Z`) and the cost-model binary at rate=0 (v91
  `20260731T022847Z`) produced **byte-identical `performance.json` and `data_quality.json`**
  (`cmp` clean). The cost model changed nothing it should not have.
- **Catalog-drift finding (independent of this turn, discovered by the baseline):** the
  v34 artifact is no longer bit-reproducible on the current data home — in-range catalog
  content grew as post-07-25 morning-chain ingests backfilled history for newly mounted
  symbols. Catalog fingerprint `363f199d…` (v34) → `ac026541…` (today); same code, same
  params, same metadata artifact now yields **111** closed trades vs v34's 119 (universe
  selection shifted on the 2026-07-14 and 2026-07-21 sessions; every common trade is
  byte-identical, `universe_hash` moved, `universe_metadata_hash` identical). All
  comparisons in this turn are therefore same-catalog: v35 vs the v90/v91 zero-cost
  baseline. v35's trade set is **identical** to that baseline's (111 closed) — costs are
  booked at fill time and touch no admission decision, so the count delta vs v34's 119 is
  catalog drift, not the cost model.
- **Cost-aware re-read of the six kept levers — single-param off-flips from the v35
  baseline (scratch home, seeded param-source manifests; same catalog, rates armed).
  NO lever's sign contribution flips: every off-flip makes net RoR WORSE, so all six
  KEEPs survive the cost-aware re-read and no new governed turn spawns from this table.**
  The head's negative net edge is not attributable to any kept lever — the levers are
  each net-positive; the residual gross edge is simply smaller than the statutory term.

  | off-flip (from v35 baseline) | closed | net P&L (KRW) | net RoR | Δ net RoR vs v35 |
  |---|---|---|---|---|
  | v35 baseline (all six ON) | 111 | −16,914 | −0.0006 | — |
  | `entry_confirm` 1.0→0.0 | 126 | −1,025,654 | −0.0325 | −0.0319 |
  | `or_width_max_atr` 0.666→0.0 | 128 | −807,135 | −0.0243 | −0.0237 |
  | `breakeven_trigger_r` 0.41→0.0 | 104 | −2,147,179 | −0.0817 | −0.0810 |
  | `risk_per_trade_krw` 299,340→0.0 (†) | 111 | −2,236,005 | −0.0479 | −0.0473 |
  | `ratio_atr_alpha` 1.0→0.0 | 111 | −838,043 | −0.0275 | −0.0269 |
  | `gap_retention_min` 0.5→1.0 (OFF) | 254 | −3,824,479 | −0.0591 | −0.0585 |

  († not a pure single-param flip: `validate()` forbids an armed `ratio_atr_alpha` with a
  zero risk budget, so the tilt goes off with the budget — the flip removes the whole
  risk-sizing family, which is the correct marginal question for that lever.)
- **FLAGGED, NOT FIXED — the rung-1 pre-registration inherits the zero-cost distribution.**
  `config/preregistration.json` (v2, frozen) derives its rung-1 expectation band
  **[−148k, +266k]** from the v34 ZERO-COST closed-trade distribution. Expectation bands
  are legitimately backtest-derivable — but this one derives from a distribution missing a
  cost term large enough to flip its sign. Amending a frozen prereg is its own governed
  act (`config/PREREGISTRATION.md`); queued as `rung1-prereg-band-zero-cost-inheritance`.
  Until that amendment lands, any attended rung-1 session gates live P&L against a band
  whose center is ~1.69M KRW too optimistic over a full backtest-length window.
- **Doc amendment:** `docs/solutions/conventions/backtest-derivable-vs-live-calibrated-bands.md`
  now carves statutory/brokerage costs (published rate × known notional — model up-front
  from cited rates) out of the live-calibrated family (slippage etc. — unchanged, still
  fail-closed at rung 2). Head-hash literals moved v34→v35 in `live.rs` (`--head`,
  `--rung-report`), `dispatch_cli.rs`, `README.md`, `RUNBOOK-rung1.md`, `RUNG1-PREFLIGHT.md`.
- **Queue re-rank:** `orb-transaction-cost-model` closes with this entry.
  `rung1-prereg-band-zero-cost-inheritance` is added and now outranks any new lever turn:
  the ladder's economic gate cites a band the head can no longer clear in expectation, so
  re-deriving it (or standing down the ladder) precedes further strategy exploration. Any
  future candidate lever must clear the ~23 bps round-trip hurdle **net**, and the four
  recent NO-BUILD readings (−0.063 … −0.00096) were all measured gross — their verdicts
  stand a fortiori under costs.

## Turn — max_concurrent slot-ranking (reallocation axis): Phase-A STOP, NO-BUILD, head stays v34 (2026-07-24) — plan 2026-07-24-002

- **Verdict: STOP at the pre-code Phase-A gate — no strategy code written, no run; head stays v34**
  (`20260724T014752Z-backtest-orb-v34`, catalog fingerprint `363f199d`, real-data RoR **0.0398**,
  119 closed trades). The diagnose CLI signalled it with typed **exit 11 (threshold-fail)** and wrote
  the STOP `gate-verdict.json`; the NO-BUILD is a complete outcome of the turn, not a failure of it. The
  lever is the queue's first genuine **reallocation** mechanism: today v34 fills its scarce
  `max_concurrent = 7` slots first-come-first-served by breakout time (`sizing_allows(open) =
  open_positions < max_concurrent`, `params.rs:875`); the candidate screens whether re-ranking *which*
  breakouts win those slots — by opening-range tightness `range_R / prior_ATR` (narrower ranks higher),
  with a full book **displacing** its widest-ranked open position for a strictly-tighter new breakout —
  would raise the head's size-invariant return-on-risk, *before any admission code is written*.
- **The frozen gate is additive/reallocation — population + additive RoR shift, no collinearity**
  (`candidate.json`, freeze commit `3eb44db`; pre-register hash `e3142076`). A slot reallocation produces
  no per-trade weight vector to correlate against, so the sizing dual-gate does not apply (the same
  reasoning that dropped collinearity for the failed-break additive stream, KTD1); the screen keeps two
  STOP gates — `population_count ≥ 12` and the additive **`ror_shift ≥ 0.005`** (`RoR(ranked book) −
  RoR(FIFO book)`). Displaced positions book **mark-to-market at the displacement bar's close** (KTD8),
  so the screen *pays* the displacement cost and `ror_shift` is conservative. The entry-local
  `diagnostic.py` and the catalog-wide, parallel-array-book `twin.py` agreed **bit-for-bit** on every
  reading. The exit engine was validated against the head first (KTD2): resolution-bar-close fills
  reproduce **108/119** closed trades' realized exit exactly and an engine `ror_base` of 0.0386 vs the
  performance.json 0.0398, and a FIFO occupancy replay reproduces **exactly the 20 logged
  `max_concurrent` rejects** — so the base→blocked hand-off carries no material fill bias.
- **The measurement** (v34 cohort; 148 breakout envelopes = 128 placed + 20 blocked; 99/148 rankable —
  49 near the 2026-05-18 catalog start lack the 15 prior daily sessions for `prior_ATR` and keep exact
  FIFO behaviour, KTD4):

  | reading | value | gate | result |
  |---|---|---|---|
  | `population_count` | 20 | ≥ 12 | PASS |
  | `ror_base` (FIFO book) | 0.039806 | — | — |
  | `ror_prime` (Policy-D ranked book) | 0.038847 | — | — |
  | **`ror_shift` (signed)** | **−0.000959** | ≥ 0.005 | **STOP** |

  The budget genuinely binds (20 blocked breakouts ≥ 12), so the population gate passes — but the
  **shift** gate STOPs: reranking *lowers* RoR by 0.00096, the wrong sign.
- **Why NO-BUILD (the crux).** The ranked replay admits **9** previously-blocked breakouts and, to seat
  them, **displaces 8** held positions (booked mark-to-market at the displacement bar), the book never
  exceeding 7. The net effect on size-invariant RoR is marginally **negative**: on this cohort,
  time-priority admission was already about as good as the frozen OR-width rank key — the tighter-range
  breakouts it promotes do not, net of the displacement cost paid on the positions they evict, out-earn
  the trades they replace. This corroborates the turn-4 entry-timing falsification (late breakouts were
  net winners) from the opposite side: breakout *time* is not adverse to quality here, so reordering by a
  one-step-removed geometry key reallocates the fixed budget without an edge. Because the screen pays the
  displacement cost (conservative), the near-zero-but-negative reading is a robust NO-BUILD.
- **No override, no tuning-to-escape.** The reading is **negative** — the opposite sign of the floor —
  so there is no operator-override rationale, and softening the pre-registered `0.005` floor after seeing
  −0.00096 would be the forbidden overfit (Definition of Done: no threshold softened after the reading).
  A sweep over rank keys or policies is out of scope by the plan (a fit, not a governed single screen);
  exactly one rank key + one displacement policy were frozen before the reading.
- **Registry state.** Head unchanged: **v34**; `LS_TURN_EXPECT_VERSION=34` holds. No `params.rs` /
  `orb.rs` edit → the head-identity gate is untouched, and the GO-only downstream (a default-off
  `slot_rank_mode` sentinel armed `0.0 → 1.0` via seed-and-rerun — the arming flip exceeds
  `PROPOSAL_BOUNDS_CAP = 0.5` so it never traverses the governed param path — then a KEEP read at RoR >
  0.0398 with risk-cap dominance ≤ 0.40) does **not** execute. The frozen candidate package
  (`candidate.json` + `diagnostic.py` + `twin.py`), the tool-written `gate-verdict.json`, and the
  `reallocation`-family gate-reading ledger trial are committed together. Offline throughout; no gateway.

## Turn — profit_target_r 1.00 → 0.75 (exit-geometry axis): direction Phase-A STOP, NO-BUILD, head stays v34 (2026-07-24) — plan 2026-07-24-001

- **Verdict: STOP at the pre-flip Phase-A gate — no flip run, head stays v34**
  (`20260724T014752Z-backtest-orb-v34`, catalog fingerprint `363f199d`, real-data RoR **0.0398**).
  The diagnose CLI signalled it with typed **exit 11 (threshold-fail)** and wrote the STOP
  `gate-verdict.json`; the NO-BUILD is a complete outcome of the turn, not a failure of it. The
  lever would flip the ORB exit-geometry param `profit_target_r` `1.00 → 0.75` through the
  governed `turn` command, grounded in `report mfe`'s give-back diagnosis on v34 (`stop_hit` 48 %
  of exits, target-exits only 23 %, ~10 % of trades ever exceeding 1.0R; the report's own leg-2
  reading `p70(mfe_r>0)=0.73 → 0.75`).
- **The frozen gate is exit-geometry-specific — direction + materiality, no collinearity**
  (`candidate.json`, freeze commit `8e925d3`; pre-register hash `6d705844`). `profit_target_r`
  reallocates exit *timing*, not the risk budget, so the sizing dual-gate (collinearity vs
  `risk_per_share`) is meaningless (KTD3); the screen keeps two STOP gates — the load-bearing
  **direction** gate `ror_delta ≥ 0.00065` and the **materiality** gate `exit_change_frac ≥ 0.05`,
  read off an MFE counterfactual (`r_new = 0.75 if mfe_r ≥ 0.75 else realized_r`) that is
  conservative by construction (KTD2 — the marketable-limit fill books **at or above** 0.75R on a
  gap-through, so the real flip RoR is ≥ this counterfactual). The `diagnostic.py` and the
  independently-authored `twin.py` agreed **bit-for-bit** on every reading (n=119, all closed
  trades joined an `mfe_r` exit envelope).
- **The measurement** (v34 cohort, 119 closed trades, joined on `(symbol, KST session date)` per
  `report_mfe`):

  | reading | value | gate | result |
  |---|---|---|---|
  | `ror_base` (target 1.00) | 0.039806 | — | — |
  | `ror_prime` (target 0.75) | 0.020336 | — | — |
  | **`ror_delta` (signed)** | **−0.019471** | ≥ 0.00065 | **STOP** |
  | `exit_change_frac` | 0.2773 (33/119) | ≥ 0.05 | PASS |

  Materiality passes — lowering the target changes 28 % of trades' booked outcome — but the
  **direction** gate STOPs: the counterfactual RoR *falls* by 0.0195, the wrong sign entirely.
- **Why NO-BUILD (the crux).** The give-back cohort the report flagged is real, but lowering the
  target to 0.75 **caps the winners more than it rescues the losers**: the ~10 % of trades that
  ran past 0.75R (former target-exits and time-exits) get booked at +0.75R instead of their higher
  realized R, and that lost upside outweighs the give-back trades the lower target now saves. Net
  size-invariant RoR drops from 0.0398 to 0.0203. Because the counterfactual **under-states** the
  flip's edge (KTD2), a STOP here is a *robust* signal the real v35 backtest would also fail —
  cheap and honest. The report's `p70 → 0.75` reading is a **distribution statistic** (RUNNABLE
  band membership), never an improvement — exactly the trap the direction gate exists to catch,
  and the **turn-9 profit-target falsification** repeating on the real cohort.
- **No override, no tuning-to-escape.** The reading is not marginal and not merely small — it is
  **negative**, the opposite sign of the floor. There is no operator-override rationale, and
  softening the pre-registered `0.00065` floor after seeing −0.0195 would be the forbidden overfit
  (Definition of Done: no threshold softened after the reading). A `profit_target_r` *sweep* is out
  of scope by the plan (a fit, not a governed single-flip).
- **Registry state.** Head unchanged: **v34**. No `params.rs` / `orb.rs` edit → the head-identity
  gate is untouched; the flip (U4's GO branch) does not execute. The frozen candidate package
  (`candidate.json` + `diagnostic.py` + `twin.py` + `fixture_check.py`), the tool-written
  `gate-verdict.json`, and the `exit-geometry`-family gate-reading ledger trial are committed
  together. Offline throughout; no gateway.

## Turn — failed-break reversal entry stream (lever 8, new-alpha axis): dual-grammar Phase-A STOP, NO-BUILD, head stays v32 (2026-07-22) — plan 2026-07-22-001

- **Verdict: STOP at the pre-code Phase-A gate — no strategy code written, no run; head stays
  v32** (`20260717T094841Z-backtest-orb-v32`, hash `d7a9820b`, RoR 0.1876). The diagnose CLI
  signalled it with typed **exit 11 (threshold-fail)** and wrote the STOP `gate-verdict.json`;
  the NO-BUILD is a complete outcome of the turn, not a failure of it. Lever 8 — the queue's one
  genuinely new alpha mechanism — would add a second, **long-only** entry stream trading the
  *failure* of a confirmed downside break of the opening range (breakdown then recovery back into
  the range), leaving the v32 breakout leg untouched. The plan gated the whole build on a
  diagnostic-first dual-grammar screen over the existing v32 bars *before any state-machine code*.
- **The frozen gate** (`candidate.json`, freeze commit `eaee1cb`; pre-register hash
  `e1494d02`). An additive stream has no incumbent signal to correlate against, so the
  stop-geometry collinearity gates are dropped (KTD3); the screen keeps two STOP gates —
  `population_count ≥ 12` and the ceiling-aware **additive** `ror_shift ≥ 0.005`
  (`RoR(base+winner) − RoR(base)`, both under the screen's own flat-fill re-sim so the fill bias
  cancels). `resolution_target_share` is the recorded fill-independent primary reading;
  `winning_grammar_id` / `stop_anchor_id` (tolerance 0) force the independently-authored twin to
  agree on the argmax. The entry-local `diagnostic.py` and catalog-wide `twin.py` agreed
  byte-identically on every reading.
- **The measurement** (v32 baseline re-sim RoR **0.1522**, sizing reconstructed **77/77 exactly**
  — the barrier + CLASS-B model is faithful; matches the independent stop-geometry re-sim):

  | grammar | n | `ror_shift` | target / stop / flat share | anchor |
  |---|---|---|---|---|
  | **1 — breakdown-recovery (PRIMARY)** | 53 | **−0.063198** | 0.038 / **0.736** / 0.226 | breakdown-low |
  | 2 — post-stop re-entry (secondary) | 14 | −0.006241 | 0.286 / 0.643 / 0.071 | range-low |

  Both grammars have ample population (count gate passes) but both are RoR-negative; the winner
  (grammar 2, the least-negative, best-by-`ror_shift`) fails `ror_shift ≥ 0.005` at −0.006241 →
  STOP threshold-fail.
- **Why NO-BUILD (the crux).** The PRIMARY hypothesis is decisively **falsified**: over the pure
  additive population (selected symbol-sessions that took no v32 trade, same session gates as a
  breakout entry), the failed-break reversal long **stops out 73.6 % of the time and reaches
  target only 3.8 %** — on this large-cap KRX universe a confirmed breakdown tends to *continue*,
  not recover into a sustained move, so a long that buys the recovery is buying into a
  down-trend. Grammar A's `ror_shift −0.063` is an order of magnitude below the floor; grammar B
  (re-entering after a stop-out) is also negative. Because the diagnostic scores an *unconstrained*
  population (an upper bound — the realized flip population would be thinner still under the shared
  `max_concurrent 7` budget), the negative screen is a **robust** NO-BUILD.
- **No override, no tuning-to-escape.** With the primary grammar an order of magnitude below the
  floor and negative, there is no operator-override rationale, and softening the pre-registered
  0.005 floor to proceed would be the forbidden overfit (Stop conditions). The long-only inverse
  (shorting the continuation) is out of scope by the standing constraint — a *future* direction,
  not this turn.
- **Registry state.** Head unchanged: **v32**. No `params.rs` / `orb.rs` edit → the head-identity
  gate is untouched; U3–U7 do not execute. The frozen candidate package (`candidate.json` +
  `diagnostic.py` + `twin.py` + `README.md`), the tool-written `gate-verdict.json`, and the
  `entry-stream`-family gate-reading ledger trial are committed together. Offline throughout; no
  gateway.

## Turn — opening-range gap-retention session gate (entry-filter axis): Phase-A GO, armed via the governed command, KEEP → v32 (2026-07-17) — plan 2026-07-17-001

- **Verdict: KEEP — the gap-retention session gate `gap_retention_min 1.0 → 0.50` strictly
  improves the size-invariant RoR crux; the flip run is the new head v32** (run
  `20260717T094841Z-backtest-orb-v32`, hash `d7a9820b…`, RoR **0.1876**). Verbatim governed
  verdict line: `KEEP v32 dad62e9663873af0a57a5014fa92f990b8c0212d8ad81402ca67e6899e9d6641`
  (the hash is the run's `lab_src_fingerprint`). The first entry-filter KEEP since the
  or-width decouple, and the **first candidate whose ARMING flip traversed the governed
  param path natively**: `1.0 → 0.50` is a finite 50% relative change, exactly on the
  `PROPOSAL_BOUNDS_CAP = 0.5` inclusive bound — the on-bound epsilon fix admitted it
  (`approved: gap_retention_min 1.0000 -> 0.5000, strategy v31 -> v32`), so no
  seed-and-rerun was needed (unlike every `0 → X` sentinel arming flip before it).
- **Phase A — committed GO, echoed from the frozen candidate package (never re-derived).**
  Frozen candidate `adapters/nautilus/lab/candidates/opening-range-gap-retention/`, freeze
  commit `403b6c9`, catalog fingerprint `3b6be31b…`, pre-register hash `1a9af3ee…`;
  diagnostic + twin agree bit-for-bit on all readings. Over v30's 167 closed trades:
  retained 69 / rejected 98 (17 / 22 sessions), `predicted_ror_shift 0.10496394`
  (161× the `0.00065` floor, GO), `retained_max_risk_capital_share 0.09742712` (≤ 0.40, GO),
  `retained_ror 0.23120334` vs `head_ror 0.1262394`. The ledger carries two identical
  `gate-reading` GO lines (04:24:46 / 04:45:32 UTC 2026-07-17) — benign append-only history,
  one gate verdict.
- **Re-baseline — v31 via `turn governed` `LS_TURN_CODE_BUMP=1` (invocation 1, unarmed).**
  Stage lines verbatim: `parent fingerprint OK` → `reusing GO for candidate
  'opening-range-gap-retention'` → `build OK` → `built binary fingerprint OK` → `code turn:
  strategy v30 -> v31, params unchanged` → `finalized run 20260717T094646Z-backtest-orb-v31`
  → `REVERT ror-negative`. That printed REVERT is the **identity outcome** (a perfect
  re-baseline equals v30's RoR, failing the strictly-greater rule by construction — the
  amihud precedent), NOT this turn's verdict; it landed no ledger line (identity checks
  never do). `strategy_code_hash 6ae7b9f1… → d7a9820b…` (the #167 OFF seam + #168 armed
  gate moved `orb.rs`); catalog/universe/range identical to v30.
- **One-to-one reconciliation vs pinned head v30 — the fail-closed gate before arming.**
  (a) `performance.json` v30 vs v31: **the entire artifact is byte-identical** (`cmp` clean)
  — all 171 trade rows (167 closed) identical on every field incl. symbol/qty/pnl/
  risk_capital, summary equal on every field. (b) `runs compare` code mode v30 → v31:
  `param diff: ["strategy_version"]` / `version-only delta: strategy_version (code turn)` /
  `strategy_code_hash delta: expected (code-turn re-baseline)` / `verdict: PASS`. The OFF
  seam + armed-at-sentinel gate are verifiably behavior-preserving.
- **Flip — v32 via `turn governed` `LS_TURN_PARAM=gap_retention_min LS_TURN_VALUE=0.5`
  (invocation 2, armed).** Same parent gates green; guardrail approved the exact-on-cap
  change; `runs compare` param mode v31 → v32 PASS with diff exactly
  `["gap_retention_min", "strategy_version"]`; exactly one trials line appended
  (`look: flip`, `verdict: "flip approved v32"` — the KEEP/REVERT verdict is recorded here
  and in the run artifacts, not in the ledger, by construction).
- **The flip result (v32 vs v31, KEEP rule = `EdgeEvaluation::keeps_over`: RoR strictly
  beats 0.1262394 AND risk-capital dominance ≤ 0.40):**

  | metric | v31 (=v30) | v32 (flip) | KEEP gate |
  |---|---|---|---|
  | **Return-on-risk** (the crux) | 0.1262394 | **0.1875966** | **PASS** (strict; +0.0613572) |
  | equal-weight mean-R | 0.112946 | 0.173689 | improved (real cohort change, not a reweight) |
  | closed trades | 167 | 77 | — |
  | Σ risk_capital | 43,060,250 | 19,676,800 (−54.3%) | — |
  | risk-capital dominance | 0.0593 | 0.0979 | PASS (≤ 0.40) |
  | pnl_total (KRW) | 5,435,900 | 3,691,300 (−1,744,600) | — |

- **Bind — the gate cuts risk far faster than P&L.** The armed session gate rejected
  **301 distinct symbol-sessions** on measured retention `< 0.50` plus **2 fail-closed
  `gap_retention_invalid` sessions** (068270.XKRX 20260526/20260529); zero
  `gap_retention_*` records in the unarmed v31 stream, confirming the seam. The realized
  cohort is 77 closed trades — **above** Phase-A's static 69, because rejected sessions
  freed `max_concurrent` slots for replacement entries (the turn-10 caveat, here working
  in the gate's favor). Realized ΔRoR **+0.0614** vs the first-order predicted **+0.1050**:
  the static retained-cohort projection over-predicted (replacements dilute the retained
  cohort's 0.2312 RoR) but direction and materiality held — 41% less P&L on 54% less
  deployed risk is a strictly better use of capital under the size-invariant crux.
- **Registry state.** Head is now **v32** (`20260717T094841Z-backtest-orb-v32`,
  `strategy_code_hash d7a9820b…`, `lab_src_fingerprint dad62e96…`, RoR 0.1876,
  `gap_retention_min 0.50` — the sole armed value; `1.0` stays the reserved OFF sentinel
  per `OrbParams::validate`'s exact-set rule). Per the KEEP settlement both new runs stay
  in `runs/`: v30 (prior head) → v31 (re-baseline) → v32 (head). No archives moved.
  Offline throughout; no gateway. Root workspace untouched (evidence-only diff).
- **Family status.** Entry-filter axis reopened and productive: gap-retention is the first
  session-classifier gate (applicability → availability → divide → validity, equality
  passes) and the first KEEP on the axis since or-width 0.666. The #159–#168 chain
  (observable → cutoff → cohort → Phase-A gate → OFF seam → armed gate) closes with a
  merit-bearing KEEP; `strategy_code_hash d7a9820b…` is the new re-baseline anchor for
  any future turn.

## Turn — Amihud liquidity budget tilt (CLASS B, liquidity axis): Phase-A DUAL GO, built + flipped via the governed command, REVERT (2026-07-16) — plan 2026-07-16-003

- **Verdict: REVERT — the Amihud-illiquidity budget tilt `liquidity_tilt_alpha` is
  material but does NOT improve the size-invariant RoR crux; v30 stays head** (hash
  `6ae7b9f1`, RoR **0.1262**). The first candidate after CLASS B sizing closed, and the
  **first turn driven through the governed command** that shipped in PR #155. A genuinely
  new **dimensionless** sizing axis — Amihud illiquidity `illiq = mean over prior 14
  sessions of |ret|/(close·volume)` — multiplying the risk **budget** by
  `w = clamp((illiq_ref/illiq)^alpha, w_lo, w_hi)` (numerator-only, anti-collapse),
  down-weighting illiquid breakouts. The economic thesis (illiquid names gap through the
  stop → worse P&L-per-risk, mirroring the ratio-ATR tilt that KEPT) did **not** hold in
  the RoR-improving direction on this sample.
- **Phase A — pre-code DUAL gate, run through `turn diagnose` (frozen candidate
  `adapters/nautilus/lab/candidates/amihud-liquidity-tilt/`, committed at `2b414372`
  BEFORE the reading; twin-verified bit-for-bit).** Over v30's 167 closed trades (103
  illiq-available, ≥15 daily priors; 64 sized `w = 1` skip-not-reject). Frozen derivation
  over the untreated `illiq` distribution: `illiq_ref = median = 1.984881e-13`,
  `w_lo = ref/p90 = 0.599573`, `w_hi = ref/p10 = 6.541589`, `alpha = 1.0`. The gate emits
  **absolute-value** collinearity readings so `< 0.70` is correct on a signed statistic:

  | gate | reading | rule | result |
  |---|---|---|---|
  | **Collinearity** vs `risk_per_share` | **\|r\| = 0.4989** (Spearman 0.3115) | `\|r\| < 0.70` | **GO** (the dimensionless Amihud escapes the price-scale collinearity that STOPPED absolute ATR at 0.9593) |
  | **Collinearity** vs the KEPT ratio-ATR weight | **\|r\| = 0.2885** (Spearman −0.3664) | `\|r\| < 0.70` | **GO** (a genuinely new reallocation, not a re-expression of the kept tilt) |
  | **Materiality (a)** predicted RoR shift | **0.030882** (first-order, positive) | `≥ 0.00065` | **GO** (47×) |
  | **Materiality (b)** integer qty-change frac | **0.4491** (75/167) | `≥ 0.05` | **GO** (9×) |

  Diagnostic + independent twin agree bit-for-bit (all four readings). The gate reading
  landed in `ledger/trials.jsonl` (look `gate-reading`, verdict `GO`, fingerprint
  `3b6be31b`), and `gate-verdict.json` records the freeze commit `2b414372` predating it.
- **Phase B — built (default-off `liquidity_tilt_alpha` + three frozen ref/clamp companions,
  an Amihud `prior_illiq` threaded through `UniverseCandidate → SelectedSymbol → OrbState →
  entry_qty` with the exact `prior_atr` window discipline; 6 new lever tests) and flipped.**
  - **v31 re-baseline via `turn governed` `LS_TURN_CODE_BUMP=1`** (the real cargo build +
    real-child rehearsal the CI stubs skipped): `strategy_code_hash 6ae7b9f1… → d11b7a41…`;
    `performance.json` reconciles **1:1** to v30 (167 trades, summary + per-trade
    sym/qty/pnl/risk_capital byte-identical) — the lever is verifiably default-off. Governed
    verdict `REVERT ror-negative` (a re-baseline reconciles → no improvement, the expected
    identity outcome).
  - **v32 flip (`liquidity_tilt_alpha 0.0 → 1.0`)** via seed-and-rerun: the 0→X sentinel
    arming flip is an infinite relative change, which `ProposalBoundsGuardrail` (cap 0.5,
    not env-configurable) fail-closes — so it cannot traverse a governed param turn (the same
    reason the ratio-ATR/trail arming flips used seed-and-rerun). `runs compare` v31→v32
    **PASS**, diff exactly `{liquidity_tilt_alpha, strategy_version}`.
- **The flip result (v32 vs v31, n = 167 closed, KEEP rule = `EdgeEvaluation::keeps_over`:
  RoR strictly beats 0.1262 AND risk-cap dominance ≤ 0.40).** Computed via the exact ported
  `dominance_fold`/`keeps_over` logic (verified: v31 RoR = 0.1262394, matching v30's known
  head value) because the governed command structurally cannot run the sentinel arming flip.

  | metric | v31 (=v30) | v32 (flip) | KEEP gate |
  |---|---|---|---|
  | **Return-on-risk** (the crux) | 0.1262394 | **0.1146893** | **FAIL** (≤ 0.1262, strict; −0.0116) |
  | equal-weight mean-R (invariant) | 0.112946 | 0.112946 | unchanged (pure reweight) |
  | Σ risk_capital | 43,060,250 | 44,883,450 (+4.2%) | — |
  | risk-capital dominance | 0.0593 | 0.0616 | PASS (≤ 0.40) |
  | pnl_total (KRW) | 5,435,900 | 5,147,650 (−288,250) | — |

- **Bind — material, pure reallocation.** mean-R is unchanged (0.112946), so the flip is a
  pure sizing reweight (75/167 integer-qty changes as Phase A predicted), not an edge change.
  But it deploys **more** risk (+1.82M) for **less** P&L (−288,250), so RoR falls hard.
- **Why REVERT (the crux, and a KTD).** The positive first-order Phase-A prediction
  (RoR′ 0.157) **reversed sign** at the flip because the first-order reweighting ignores the
  **notional ceiling**: the large `w_hi = 6.54` upsizing of liquid names is clipped by
  `floor(notional/px)`, so the predicted upside never materializes, while down-weighting the
  illiquid cohort (which carried at/above-average P&L-per-risk here) still cuts return.
  Net RoR-negative. There is no operator override — softening the KEEP gate after seeing
  0.1147 is the forbidden overfit. **KTD: a numerator-only tilt with a large upper clamp is
  notional-ceiling-bound, so the first-order Phase-A RoR shift over-predicts (even mis-signs)
  the flip; a future Phase-A materiality gate for a wide-clamp tilt should model the ceiling.**
- **Registry state.** Head unchanged: **v30** (`20260715T092847Z-backtest-orb-v30`, hash
  `6ae7b9f1`, RoR 0.1262). The lever ships **default-off** (`liquidity_tilt_alpha: 0.0`
  sentinel, byte-identical to v30; the ref/clamp companions default to their frozen values so
  the lever is governed-flippable in one alpha turn, inert while alpha == 0.0). v31
  (re-baseline) + v32 (flip) archived under
  `data/turn4-fresh/sizing-archive/liquidity-tilt-archive/` so v30 stays `latest_finalized`.
  Frozen candidate (pre-register + diagnostic + twin + gate-verdict) git-tracked at
  `adapters/nautilus/lab/candidates/amihud-liquidity-tilt/`. Offline throughout; no gateway.
- **CLASS B family status.** Budget axis sweep-settled; ratio-ATR tilt KEPT (v30); ATR-vol-target
  STOPPED; Kelly retired; equity-compounding REVERTED; **Amihud liquidity tilt REVERTED
  (RoR-negative)** — a new dimensionless axis that GATED GO but flipped negative. The
  liquidity axis is characterized (material but RoR-negative on this sample); a future turn
  could re-rank a ceiling-aware liquidity variant or a new mechanism class.
- **Governed-command findings surfaced (not folded in).** This first real drive exposed two
  structural gaps beyond the flip-guard hardening the code review flagged: (1) the sentinel
  arming flip (0→X) cannot traverse a governed param turn (bounds cap), so a new lever's
  merit flip still needs seed-and-rerun; (2) frozen non-default companions must be encoded as
  serde defaults for a new lever to be governed-flippable. See the PR/handoff notes.

## Turn — cross-sectionally-normalized ATR budget tilt (CLASS B, ratio-ATR axis): Phase-A DUAL GO, built + flipped, KEEP → v30 (2026-07-15) — plan 2026-07-15-002

- **Verdict: KEEP — the ratio-ATR budget tilt `ratio_atr_alpha` improves the size-invariant
  RoR crux; v30 is the new head** (hash `6ae7b9f1`, RoR **0.1262**, `ratio_atr_alpha = 1.0`).
  The one CLASS B sizing direction the prior post-mortems left live — a **dimensionless**
  inverse-ratio tilt `w = clamp((v_ref/v)^alpha, w_lo, w_hi)` on `v = prior_atr/entry_price`
  that multiplies the per-trade risk **budget** (numerator only, so it cannot collapse to the
  dead absolute-ATR lever), downweighting high relative-vol names. Absolute-ATR vol-target
  (candidate (a)) STOPPED collinear, Kelly (b) RETIRED, equity-compounding (c) REVERTED
  RoR-negative; this is the **first CLASS B sizing lever to raise RoR** — the honest outcome
  of measuring rather than assuming the family was closed.
- **Phase A — pre-code DUAL gate (frozen before any reading,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-ratio-atr-budget-tilt.md`; adversarially re-verified by
  an independent-recompute twin).** Over v26's 167 closed trades (103 ATR-available; 64 sized
  `w = 1` skip-not-reject). Frozen derivation over the untreated `v` distribution:
  `v_ref = median = 0.073158`, `w_lo = v_ref/p90 = 0.702698`, `w_hi = v_ref/p10 = 1.445490`,
  `alpha = 1.0`:

  | gate | reading | rule | result |
  |---|---|---|---|
  | **Collinearity** `r(w(v), risk_per_share)` | **−0.3617** (R² 0.1308; Spearman −0.4571, 10%-trim −0.2719) | `\|r\| < 0.70` | **GO** (near-orthogonal — the ratio escapes the price-scale collinearity that killed absolute ATR at 0.9593) |
  | **Materiality (a)** predicted RoR shift | **0.018278** (RoR 0.11714 → RoR′ 0.13542, *positive*) | `≥ 0.00065` | **GO** (28×) |
  | **Materiality (b)** integer qty-change frac | **0.3413** (57/167) | `≥ 0.05` | **GO** (6.8×) |

  The gate re-measures this lever's exact axis `w(v)` (not the raw ratio `v`, whose evidence
  reading `r(v, rps) = 0.2949`/ρ 0.4579 the twin reproduces). Both gate scripts + the
  independent twin agree bit-for-bit (`v_ref`, p10/p90, clamps to 8 dp; `prior_atr(005930,
  2026-06-12) = 23232.142857` matches u5). The first-order shift is **positive** — opposite the
  equity lever's negative foreshadowing — and the flip confirmed the direction.
- **Phase B — built (default-off `ratio_atr_alpha` + three frozen clamp/ref params, an inline
  `ratio_atr_weight` at the Enter handler, no runner threading) and flipped via seed-and-rerun.**
  - **v29 re-baseline** (`ratio_atr_alpha: 0.0`, frozen ref/clamps seeded, rerun, seed removed):
    `performance.json` reconciles **1:1** to v26 (167 trades, per-trade qty + P&L, RoR
    **0.1171398010** exact); `strategy_code_hash` moved `d199d124…` → `6ae7b9f1…`; `runs
    compare` v26→v29 **FAIL** (`strategy_code_hash differs`) — the intended re-baseline evidence.
  - **v30 flip** (`ratio_atr_alpha: 1.0` seeded on v29, rerun): `runs compare` v29→v30 **PASS**,
    diff exactly `{ratio_atr_alpha, strategy_version}`.
- **The flip result (v30 vs v29, n = 167 closed, KEEP rule R12: `RoR > 0.1171` strict AND
  risk-cap dominance ≤ 0.40).**

  | metric | v29 (=v26) | v30 (flip) | KEEP gate |
  |---|---|---|---|
  | **Return-on-risk** (the crux) | 0.1171398 | **0.1262394** | **PASS** (> 0.1171, strict; +0.0091) |
  | equal-weight mean-R (invariant) | 0.112946 | 0.112946 | unchanged (size-invariant) |
  | Σ risk_capital | 44,640,250 | 43,060,250 (−3.5%) | — |
  | risk-capital dominance | 0.0545 | 0.0593 | PASS (≤ 0.40) |
  | pnl_total (KRW) | 5,229,150 | 5,435,900 (+206,750) | non-decisional |
  | expectancy (KRW/trade) | 31,312 | 32,550 (> 0) | `is_edge` TRUE |

- **Bind — CONFIRMED material (R11, v29→v30 per-trade qty deltas matched on (symbol, session)).**
  **57/167 = 34.1% of closed trades shift integer qty** (19 upsized, 38 downsized) — matching
  the Phase-A 57/167 prediction. High-`v` (high relative-vol) trades downsize toward `w_lo`,
  low-`v` upsize toward `w_hi`, exactly the frozen tilt direction. Cohort stable (167 → 167; no
  qty→0 eliminations, consistent with the Phase-A "0 floored to 0" reading).
- **Why KEEP (the crux).** The tilt earns **more** total P&L (+206,750 KRW) on **less**
  deployed risk (−1.58M), so the size-invariant return-on-risk rises 0.1171 → **0.1262**.
  mean-R is unchanged (0.112946) — the same trades at the same per-trade R, reweighted — so
  this is a **pure sizing reallocation** that lifts the risk-adjusted rate: high relative-vol
  names carried worse P&L-per-risk (the vol-parity / gap-through-stop thesis, frozen a-priori),
  and downweighting them is RoR-positive. The KEEP rule fires deterministically; no operator
  override, no threshold softening.
- **Registry state.** New head: **v30** (`20260715T092847Z-backtest-orb-v30`, hash `6ae7b9f1`,
  RoR 0.1262, `ratio_atr_alpha = 1.0`, `ratio_atr_ref = 0.073158`, `ratio_atr_w_lo = 0.702698`,
  `ratio_atr_w_hi = 1.445490`). The lever's code default stays the `0.0` sentinel
  (byte-identical to v26 for any legacy manifest); the head arms it. v29 (re-baseline) + v30
  (armed head) are the head lineage in `runs/`; no non-KEEP runs to archive (KEEP, no fan-out).
  Phase-A diagnostic + independent twin under `sizing-archive/ratio-atr-tilt-diagnostic/`; frozen
  gate + full decision in `PRE-REGISTER-vNEXT-ratio-atr-budget-tilt.md`. Offline throughout; no gateway.
- **CLASS B family status.** Candidate (a) ATR-vol-target: STOPPED (collinear). Candidate (b)
  Kelly: RETIRED. Candidate (c) equity-compounding: BUILT + REVERTED (RoR-negative). Ratio-ATR
  tilt: **BUILT + KEPT (RoR-positive)** — the axis the equity turn named as the live direction
  delivered. The sizing-budget axis is sweep-settled; `ratio_atr_alpha` is now a sweepable head
  param (a future governed turn could tune the exponent off its structural 1.0).

## Turn — session-granular realized-equity compounding sizing (CLASS B lever 2, candidate (c)): Phase-A DUAL GO, built + flipped, REVERT (2026-07-15) — plan 2026-07-15-001

- **Verdict: REVERT — the equity-compounding lever `equity_compound_frac` is material
  (both Phase-A gates GO, the flip binds) but does NOT improve the size-invariant RoR
  crux; v26 stays head** (hash `d199d124`, RoR **0.1171**). The final deferred CLASS B
  sizing candidate: scale the per-trade risk budget by the **session-open realized-equity
  multiplier** `m = 1 + Σ(prior-session realized P&L)/starting_balance`, flipped to the
  fixed-fractional identity `equity_compound_frac = 1.0`. Kelly-fraction sizing (candidate
  (b)) is **retired** this turn with the recorded four-class examination (plan KD1). Unlike
  the ATR turn (candidate (a), which STOPPED at Phase A), this one **passed** the dual gate
  and was built through the flip — the honest outcome of measuring rather than assuming.
- **Phase A — pre-code DUAL gate (frozen before any reading,
  `data/turn4-fresh/PRE-REGISTER-vNEXT-equity-compounding.md`; adversarially re-verified by
  an independent-recompute twin).** Over v26's 167 closed trades (sum realized P&L
  +5,229,150 KRW vs 100M `starting_balance` → multiplier span 1.000–1.0557):

  | gate | reading | rule | result |
  |---|---|---|---|
  | **Collinearity** `r(m, risk_per_share)` | **−0.0175** (R² 0.0003; Spearman −0.0508, 10%-trim −0.0824) | `\|r\| < 0.70` | **GO** (near-orthogonal — the *opposite* failure mode from ATR's 0.9593) |
  | **Materiality (a)** predicted RoR shift | **0.002078** (RoR 0.11714 → RoR′ 0.11506) | `≥ 0.00065` | **GO** (3.2×) |
  | **Materiality (b)** integer qty-change frac | **0.3293** (55/167) | `≥ 0.05` | **GO** (6.6×) |

  The materiality gate — added precisely to catch the near-constant-axis blind spot a plain
  collinearity gate misses — reads the axis **material** despite the ~5.6% span: a small
  multiplier still reallocates a third of integer position sizes. Note the predicted shift
  was **negative** (RoR′ < RoR), foreshadowing the flip's degradation. Both gate scripts +
  the independent twin agree bit-for-bit (multiplier max\|Δ\| = 0; `prior_atr`
  `23232.142857` matches u5). Piggybacked R6 ratio-axis reading (evidence only):
  `r(prior_atr/avg_px_open, risk_per_share) = 0.2949` (Spearman 0.4579) — the
  cross-sectionally-normalized ATR is far more orthogonal than the absolute-KRW ATR
  (0.9593), carried as the live direction for the next CLASS B re-rank.
- **Phase B — built (default-off `equity_compound_frac`, runner accumulator, threaded
  scalar) and flipped via seed-and-rerun.**
  - **v27 re-baseline** (`equity_compound_frac: 0.0` seeded on v26, rerun, seed removed):
    `performance.json` reconciles **1:1** to v26 (trades, equity_curve, every summary key);
    `strategy_code_hash` moved `d199d124…` → `023b8087…`; `runs compare` v26→v27
    **FAIL** (`param diff ["strategy_version"]`, `strategy_code_hash differs`) — the
    intended re-baseline evidence.
  - **v28 flip** (`equity_compound_frac: 1.0` seeded on v27, rerun): `runs compare` v27→v28
    **PASS**, diff exactly `{equity_compound_frac, strategy_version}`.
- **The flip result (v28 vs v27, n = 167 closed, KEEP rule R13: `is_edge` AND
  `RoR > 0.1171` AND risk-dominance ≤ 0.40).**

  | metric | v27 (=v26) | v28 (flip) | KEEP gate |
  |---|---|---|---|
  | **Return-on-risk** (the crux) | 0.117140 | **0.116304** | **FAIL** (≤ 0.1171, strict) |
  | equal-weight mean-R (invariant) | 0.112946 | 0.112946 | unchanged (size-invariant) |
  | Σ risk_capital | 44,640,250 | 45,548,050 (+2.0%) | — |
  | risk-capital dominance | 0.0545 | 0.0544 | PASS (≤ 0.40) |
  | pnl_total (KRW) | 5,229,150 | 5,297,400 | non-decisional |
  | expectancy (KRW/trade) | — | 34,624 (> 0) | `is_edge` TRUE |

- **Bind — CONFIRMED material (R14, read from v28 `decisions.jsonl`).** Per-session
  `equity_multiplier` spans 23 distinct values 1.000000 → 1.056465 (22/23 sessions > 1);
  effective risk budget varies 299,340 → 316,242; **59/184 = 32% of order placements shift
  integer qty** vs v27 (matching Phase-A's 33% prediction). The lever is **not** inert — it
  reallocates — so this is an **edge verdict, not INERT**.
- **Why REVERT (the crux).** `is_edge` holds (positive expectancy, risk-dominance capped),
  but RoR slips 0.1171 → **0.1163**, failing the strict-inequality KEEP gate. Compounding
  sizes **up** later, higher-equity sessions whose average per-trade R is marginally lower,
  so total P&L rises (+68,250 KRW) while the *size-invariant* return-on-risk falls — a
  larger book earning a slightly worse rate. mean-R is unchanged (0.112946), confirming a
  pure sizing reweight with no edge change. Fixed-fractional compounding is real but
  **RoR-negative** on this sample; there is no operator override (softening the KEEP gate
  after seeing 0.1163 is the forbidden overfit).
- **Registry state.** Head unchanged: **v26** (`20260712T080054Z-backtest-orb-v26`, hash
  `d199d124`, RoR 0.1171). The lever ships default-off (`equity_compound_frac: 0.0`
  sentinel, byte-identical to v26) and stays in the code as a sweepable param
  (`numeric_summary`), but is **not** activated. v27 + v28 archived under
  `data/turn4-fresh/sizing-archive/` (non-KEEP, per FAN-OUT); Phase-A diagnostic +
  independent twin under `sizing-archive/equity-compounding-diagnostic/`; frozen gate and
  full decision in `PRE-REGISTER-vNEXT-equity-compounding.md`. Offline throughout; no gateway.
- **CLASS B family status.** Candidate (a) ATR-vol-target: STOPPED (collinear). Candidate
  (b) Kelly: RETIRED (KD1). Candidate (c) equity-compounding: BUILT + REVERTED (RoR-negative).
  The sizing-budget axis is sweep-settled. The only live direction left is the
  **ratio-ATR** (ATR/price, `r = 0.2949`) recorded above — a future re-rank, not this turn.

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
