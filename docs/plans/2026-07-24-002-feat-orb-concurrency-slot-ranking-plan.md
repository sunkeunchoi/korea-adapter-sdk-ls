---
title: "ORB max_concurrent slot-ranking governed Phase-A candidate - Plan"
type: feat
date: 2026-07-24
topic: orb-concurrency-slot-ranking
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

# ORB max_concurrent slot-ranking governed Phase-A candidate - Plan

## Goal Capsule

- **Objective:** Author a governed ORB strategy turn on a genuinely new mechanism
  class — **cross-sectional ranking of breakout entries competing for the
  `max_concurrent` slot budget** — as a diagnostic-first Phase-A candidate against
  the real-data head **v34** (`20260724T014752Z-backtest-orb-v34`, catalog
  fingerprint `363f199d`, size-invariant RoR **0.0398**, 119 closed trades; pin
  `LS_TURN_EXPECT_VERSION=34`). The turn produces an honest GO/STOP from a frozen
  gate before any strategy code is written.
- **Product authority:** This plan owns only the Phase-A candidate (the
  pre-registered screen and its verdict). The Phase-B build (rank-aware admission
  in `orb.rs`) and the KEEP/REVERT flip are downstream and conditional on GO; they
  are not active scope here. A STOP is a complete, documented outcome (NO-BUILD),
  not a failure of the turn.
- **Open blockers:** none. The three product forks (selection policy, rank key,
  displaced-exit rule) are settled in Key Decisions. The feasibility question is
  resolved by planning research (see Dependencies / Assumptions): all 20 of v34's
  `max_concurrent` rejects join a `breakout` envelope carrying entry + stop, so the
  blocked cohort is re-simulable and `population_count = 20 ≥ 12`.

---

## Product Contract

**Product Contract preservation:** unchanged. The one Resolve-Before-Planning
question (screen feasibility) is resolved by planning research and moved to
Dependencies / Assumptions; no Requirement, Key Decision, or Scope Boundary was
altered.

### Summary

Author a governed Phase-A candidate that screens whether **re-ranking which
breakouts win the scarce `max_concurrent` slots** would raise the size-invariant
return-on-risk of head v34. Today a breakout that fires while the book is full is
dropped done-for-day (`sizing_allows(open) = open_positions < max_concurrent`,
`adapters/nautilus/lab/src/params.rs:875`), so slot allocation is pure
time-priority — the loop has repeatedly observed freed slots admitting replacement
entries that are sometimes losers, but has never tried to *choose* which
candidates fill the budget. The candidate freezes a rank key + selection policy +
gate thresholds, runs `turn diagnose` to emit a GO/STOP `gate-verdict.json`, and
only on GO does a later turn build and flip. The gate follows the
**additive/reallocation** class — a **population count** floor plus an
**additive-RoR-shift** floor, with no collinearity sub-gate (a slot reallocation
has no incumbent per-trade weight to correlate against).

### Problem Frame

The lever queue is thinning: the last two turns both STOPped at Phase-A —
`profit_target_r 1.00→0.75` on the direction gate (ror_delta −0.0195, lowering the
target caps winners more than it rescues give-backs) and the failed-break reversal
stream on the additive floor (ror_shift −0.063). Sizing/budget is sweep-settled,
stop geometry is CLASS-B-absorbed and near-inert, and the exit-*level* sub-axis is
falsified twice. Every lever tried to date is either a binary session/entry filter
or a per-trade sizing reweight.

One structural fact recurs across the whole log and has never been treated as a
lever: the `max_concurrent` budget binds, and its allocation is uncontrolled.
"Rejected sessions freed max_concurrent slots for replacement entries"
(gap-retention turn); "the tighter stop fires earlier and frees `max_concurrent`
slots, admitting more losing entries" (midpoint-stop turn); the book grew "34 → 48"
concurrent as more sessions reached breakout (or-width decouple). The strategy
fills those slots first-come-first-served by breakout time. If the time-priority
order is uncorrelated with (or adverse to) trade quality, a rank-aware policy
reallocates the same fixed risk budget toward better trades — a size-invariant RoR
improvement with no new capital, no new entry stream, and no change to any
per-symbol filter. The entry-timing falsification (turn 4: late breakouts were net
winners) is corroborating evidence that time-priority admission is *not* already
selecting for quality.

### Key Decisions

- **Gate class: additive/reallocation — count + additive-RoR floor, no
  collinearity gate.** A slot reallocation produces no per-trade weight vector to
  correlate against `risk_per_share` or the kept ratio-ATR weight, so the sizing
  dual-gate does not apply (the same reasoning that dropped collinearity for the
  additive failed-break stream). The screen keeps two STOP gates: a **population
  count** floor and an **additive-RoR-shift** floor measured under the screen's own
  re-simulation so any fill bias cancels.
- **Pre-registered thresholds mirror the additive-family precedent.**
  `population_count ≥ 12` and `ror_shift ≥ 0.005`, where `ror_shift = RoR(ranked
  book) − RoR(FIFO book)` under a single consistent re-sim. `0.005` is the
  additive-family floor (below the smallest kept gain, ratio-ATR's +0.0091) and is
  frozen before the reading; it is not softened after the number is seen.
- **KEEP anchor (for the eventual GO-path flip): RoR strictly beats v34's
  0.0398 with risk-capital dominance ≤ 0.40** (`EdgeEvaluation::keeps_over`). This
  is the post-flip bar, distinct from the Phase-A `ror_shift` screen.
- **Population is cheap to read; the counterfactual is not.** Every budget-blocked
  breakout is already logged as `OrderRejectedSizing` with `filter:"max_concurrent"`
  and values `open_positions`, `qty` (`orb.rs:1299-1321`), so `population_count` is
  a direct read of v34's `decisions.jsonl`. The `ror_shift` reading, by contrast,
  requires re-simulating the blocked/displaced cohort's trade lifecycle over the
  minute bars.
- **Freeze-before-reading discipline.** The candidate directory
  (`candidate.json` + `diagnostic.py` + `twin.py`) is committed and its content
  hashes recorded in `candidate.json` before `turn diagnose` runs; the tool writes
  `gate-verdict.json` carrying the decision, both readings, `pre_register_hash`,
  `catalog_fingerprint 363f199d`, and the freeze commit. `diagnostic.py` and an
  independently-authored `twin.py` must agree bit-for-bit on every reading.
- **Arming is a code turn, not a param flip.** Unlike `profit_target_r`, a GO here
  leads to a strategy-code change (rank-aware admission), armed via a new default-off
  param (e.g. a `slot_rank_mode` sentinel). A `0.0 → X` arming flip exceeds
  `PROPOSAL_BOUNDS_CAP = 0.5` (`research.rs:57`), so it traverses seed-and-rerun,
  not the governed param path — as every prior sentinel arming flip did.
- **Selection policy: displacement (Policy D)** (session-settled: user-directed —
  chosen over tie-break-only: tie-break's simultaneous-same-bar population is likely
  too thin to clear the count floor, so it would under-test the thesis). A full book
  may drop its weakest-ranked open position to admit a strictly higher-ranked new
  breakout. The population is every logged `max_concurrent` reject. Displacement fires
  on any strict rank improvement (no tunable margin) and may cascade.
- **Rank key: OR-width tightness** (session-settled: user-directed — chosen over stop
  tightness and gap magnitude). Breakouts are ordered by `range_R / prior_ATR`
  ascending (narrower opening range ranks higher), the key that both ranks the held
  book (its widest member is the displacement target) and ranks contending new
  breakouts. Grounded in the target cohort being narrowest; one step removed from the
  sizing thesis, so the non-redundancy story is cleaner than a `risk_per_share` key.
  Breakout strength (falsified as a filter, turn 10) and opening RVOL (inverted
  signal) are excluded.
- **Displaced-exit rule: mark-to-market at the displacement bar** (session-settled:
  user-directed — chosen over booking W's natural realized exit, and over a
  non-winner-only condition). A displaced position W books its P&L at its price on the
  displacement bar t (it is sold to free the slot); no free parameter. This makes
  `ror_shift` conservative — the screen pays the displacement cost — so a GO is a
  robust signal the real backtest would also clear.

### Requirements

**Candidate scaffold**
- R1. Create `adapters/nautilus/lab/candidates/<slug>/` holding `candidate.json`,
  `diagnostic.py`, and `twin.py`, matching the frozen-candidate contract used by
  `profit-target-075` and `failed-break-reversal`.
- R2. `candidate.json` declares `family` as a reallocation/additive family value,
  the frozen `readings` (with per-reading tolerance + precision), the two
  `thresholds`, the `keep_anchor` string, and the `diagnostic`/`twin` argv +
  `content_hash` pins. The arming `flip_param`/`flip_value` name the new default-off
  admission param, not an existing head param.
- R3. `diagnostic.py` and `twin.py` are authored independently and must produce
  identical readings within the declared tolerances; a mismatch fails the turn
  before any verdict.

**Phase-A screen**
- R4. The screen reads only v34 artifacts (its `decisions.jsonl` + the minute-bar
  catalog at fingerprint `363f199d`); it writes no strategy code and finalizes no
  run.
- R5. The screen computes `population_count` = the number of eligible breakouts
  that the `max_concurrent` budget blocked over the v34 cohort, read from the
  logged `filter:"max_concurrent"` rejects.
- R6. The screen reconstructs the realized FIFO book from v34 exactly (a
  self-check: the reconstructed base RoR reproduces 0.0398), then re-simulates the
  ranked book under Policy D (displace the widest-`range_R/prior_ATR` held position
  for a strictly-tighter new breakout) and reports `ror_base`, `ror_prime`, and
  `ror_shift = ror_prime − ror_base`.
- R7. Blocked/admitted entries are re-simulated through the same
  stop/target/breakeven/flat-time engine as the realized book; a displaced position
  books mark-to-market at the displacement bar. One consistent fill model applies to
  both books so `ror_shift` is a pure allocation effect, not a fill-bias artifact.

**Gate verdict**
- R8. GO requires **both** `population_count ≥ 12` **and** `ror_shift ≥ 0.005`;
  any single failure is a STOP. The tool emits typed exit 11 (threshold-fail) and
  writes a STOP `gate-verdict.json` on failure.
- R9. No threshold is softened, and no operator override is invoked, after the
  reading is seen; a negative or marginal `ror_shift` is a NO-BUILD recorded in the
  candidate package and TURN-LOG.md.
- R10. On GO, the plan records the arming path (new default-off param, seed-and-rerun
  re-baseline vN, then armed flip) and the KEEP anchor for the downstream turn — but
  does not execute the build within this plan.

**Governance discipline**
- R11. The head identity is untouched: no `params.rs`/`orb.rs` edit lands in this
  plan, so v34 stays head and `LS_TURN_EXPECT_VERSION=34` holds.
- R12. The committed outcome (candidate package + `gate-verdict.json` + a TURN-LOG.md
  entry) is offline throughout; no gateway is contacted.

### Acceptance Examples

- AE1. **GO path (Covers R6, R8, R10).** `population_count = 20` and the ranked book
  re-sim yields `ror_prime = 0.0472`, `ror_shift = +0.0074 ≥ 0.005` → GO. The verdict
  records GO; a later turn builds rank-aware admission, re-baselines vN via
  seed-and-rerun, arms the flip, and reads KEEP only if RoR > 0.0398 with dominance
  ≤ 0.40.
- AE2. **STOP on thin population (Covers R5, R8).** `population_count = 7 < 12` → STOP
  regardless of any `ror_shift`; the budget rarely binds on the cohort, so the
  mechanism has too little to act on. NO-BUILD, cheap and honest. (Not v34's case —
  v34 reads 20 — but the gate must STOP here if a future anchor is thin.)
- AE3. **STOP on direction (Covers R6, R8, R9).** `population_count = 20` but
  `ror_shift = −0.011 < 0.005` → STOP; time-priority admission was already
  as-good-or-better than the frozen rank key on this sample. The floor is not
  softened to proceed.

### Scope Boundaries

- Phase-B strategy code (rank-aware admission), the vN re-baseline, and the
  KEEP/REVERT flip are out of scope here — conditional on GO, a separate turn.
- No change to any existing filter, sizing, or exit param; the head armed set is
  frozen. This lever only reorders which eligible breakouts win slots.
- No new entry stream and no new instrument. This is a reallocation of the existing
  breakout population under the existing budget, not new alpha.
- No sweep or fan-out over rank keys or policies — exactly one of each is frozen
  before the reading. Comparing several would be a fit.

#### Deferred to Follow-Up Work

- The Phase-B admission implementation (rank-aware selection hooking into the
  `sizing_allows` gate in `orb.rs`), the new default-off param + serde default, its
  lever tests, the vN re-baseline, and the armed flip — all conditional on GO.

### Dependencies / Assumptions

- **Feasibility — RESOLVED by planning research.** v34's `decisions.jsonl` emits a
  `breakout` transition envelope (`decision_detail.kind == "breakout"`, values
  `range_high`/`range_low`/`breakout_price`/`strength`) immediately before each
  `Enter` action (`orb.rs:1265-1274`), and only then the possible `max_concurrent`
  reject. All **20** of v34's `max_concurrent` rejects join a same-`(symbol, ts)`
  `breakout` envelope, so each blocked candidate's entry limit (`breakout_price`),
  stop (`range_low`, since head `stop_mode = 0` RangeLow), and `range_R` are
  recoverable; `qty` rides the reject record. `population_count = 20 ≥ 12`; blocks
  cluster in 4 high-breakout sessions (8/5/4/3) under `max_concurrent = 7`.
- **Assumption:** the minute bars needed for the exit re-sim live at
  `data/turn4-fresh/catalog/data/bars/{symbol}-1-MINUTE-LAST-EXTERNAL/*.parquet`
  (confirmed present) and the run's catalog fingerprint is `363f199d` (the
  diagnostic re-derives and asserts it, mirroring
  `candidates/failed-break-reversal/diagnostic.py`'s `actual_catalog_fingerprint`).
- **Assumption:** `turn diagnose` discovers a candidate by directory (`load(dir)`),
  needing no slug-allowlist edit — as `profit-target-075` and `failed-break-reversal`
  are. If a registration seam exists, it surfaces at U4 and is a Rust edit that must
  not touch `params.rs`/`orb.rs` (head identity).

### Sources / Research

- `adapters/nautilus/lab/TURN-LOG.md` — full lever ledger; head-lineage standing
  note (v34, lines 7-46); profit-target-075 STOP (48-98); failed-break reversal STOP
  and its additive gate (100-148); recurring `max_concurrent` freed-slot observations
  (gap-retention 210-211, midpoint 1104-1106, or-width decouple 845).
- `adapters/nautilus/lab/src/params.rs:29,356,875` — `max_concurrent` field, default,
  and `sizing_allows(open) = open_positions < max_concurrent`.
- `adapters/nautilus/lab/src/strategy/orb.rs:1265-1274` (breakout envelope) and
  `:1299-1321` (the `max_concurrent` reject emission) — the pair that makes both
  `population_count` and the blocked cohort's entry/stop recoverable.
- `adapters/nautilus/lab/src/runner/diagnose.rs:38-68,133-180,233-322,428-445` — the
  `turn diagnose` contract: typed exits (11 = threshold-fail), `GateVerdict` struct,
  GO/STOP logic, and the script I/O boundary (readings written to `argv[-1]`).
- `adapters/nautilus/lab/src/candidates.rs:55-152` — `Candidate`,
  `ReadingSpec`, `ScriptDecl`, `Threshold`, `Comparator` (ge/le/gt/lt).
- `adapters/nautilus/lab/candidates/profit-target-075/{candidate.json,diagnostic.py,twin.py}`
  — the candidate.json + gate IO contract to mirror.
- `adapters/nautilus/lab/candidates/failed-break-reversal/diagnostic.py` — the
  bar-level re-sim + `actual_catalog_fingerprint` identity guard + entry-local
  parquet loaders to mirror for the exit-engine re-sim.
- `adapters/nautilus/lab/src/runner/backtest.rs:635-665` — `prior_atr` (14-session
  frozen window) to port into Python for the `range_R / prior_ATR` rank key.

---

## Planning Contract

### Key Technical Decisions

- **KTD1. Two templates, split by concern.** Mirror `profit-target-075` for the
  candidate.json shape and the readings-to-`argv[-1]` JSON I/O boundary; mirror
  `failed-break-reversal/diagnostic.py` for the bar-level exit re-sim, the
  `actual_catalog_fingerprint` identity guard, and the entry-local parquet loaders.
  Neither template alone suffices — profit-target-075 never re-simulates exits, and
  failed-break tests a different gate.
- **KTD2. Exit-engine fidelity is validated on ground truth before scoring blocked
  trades.** The 20 blocked candidates have no realized outcome, so the diagnostic
  first re-simulates v34's **119 closed trades** — the cohort `performance.json`
  reports and over which `ror_base = 0.0398` is computed — through the same
  stop/target(1.0R)/breakeven(0.41R→move stop to entry)/flat(15:00) engine and
  asserts the reproduced per-trade `realized_r` matches `performance.json` within
  tolerance. Only a validated engine then scores the blocked cohort. **Reconcile the
  count gap:** v34's `decisions.jsonl` logs **128 `order_placed` events** (and 128
  matching exits) but `performance.json` reports **119 closed trades**; the surplus
  placements (open at backtest end or otherwise unreconciled) still **occupy
  `max_concurrent` slots** during their lifetime. The re-sim therefore models book
  occupancy from all placements while computing RoR only over the 119 closed trades,
  and must reconcile the 128→119 gap explicitly rather than summing `order_placed`.
- **KTD3. Anchor run is a hardcoded literal in the scripts; the verdict fingerprint
  is auto-derived.** `turn diagnose` passes the scripts only the readings out-path;
  the anchor run is not injected. Hardcode
  `RUN = .../runs/20260724T014752Z-backtest-orb-v34` (env override for the fixture
  harness only), exactly as profit-target-075 does. The verdict's
  `catalog_fingerprint` is **auto-derived by the CLI from the latest finalized run**
  (`research.rs` `latest_finalized_run`/`ordered_runs`), not supplied by a flag, and
  `diagnose.rs` does not cross-check it against the script's own fingerprint
  assertion. **Assumption:** no other backtest finalizes in `data/turn4-fresh/runs/`
  between candidate freeze and `turn diagnose`, else the recorded `catalog_fingerprint`
  silently diverges from `363f199d`. The diagnostic's own `actual_catalog_fingerprint()`
  guard (KTD1) is the real integrity check and must assert `== 363f199d`.
- **KTD4. `prior_atr` is ported, not approximated.** Port `backtest.rs:635-665`
  faithfully: dedup one daily bar per KST session (latest wins), strictly-prior
  sessions only, require `window+1` (=15) priors, fail-closed to `None`. **A breakout
  whose symbol lacks a prior ATR (< 15 priors — a live case near the 2026-05-18
  catalog start, and the 2026-06-09 contention session sits close to that boundary)
  is unrankable, so it is excluded from the Policy-D mechanism: it keeps exact
  base/FIFO behavior — admitted if a slot is free, dropped if full, never displacing
  a held position and never displaced by rank.** This is the conservative freeze — it
  manufactures no `ror_shift` where the rank key is undefined; the twin mirrors it.
- **KTD5. `family = "reallocation"`** — a new family string (the field is free-form).
  `flip_param = "slot_rank_mode"`, `flip_value = 1.0` record the downstream arming
  param declaratively; the Phase-A screen does not exercise them.
- **KTD6. `population_count` is an exact integer reading.** Its `ReadingSpec`
  tolerance is `0` (precision `0`) so the twin must agree exactly; `ror_base`,
  `ror_prime`, `ror_shift` use tolerance `0.0005`, precision `6` (the RoR-family
  convention).
- **KTD7. No Rust behavior change.** The candidate is Python + JSON + a TURN-LOG
  entry. `turn diagnose` discovers a candidate by directory (`load(dir)`) with no
  slug allowlist (confirmed), so no Rust edit is expected; should one surface, it is
  confined to the candidate registry and must not touch `params.rs`/`orb.rs` (head
  identity, R11).
- **KTD8. The displaced position books at the displacement bar's close.** The
  mark-to-market exit (a Product Contract Key Decision) uses the bar's `close`,
  matching the codebase's close-confirm convention and the field precedents in
  `failed-break-reversal/diagnostic.py`'s `simulate()`; `twin.py` mirrors the field.

### High-Level Technical Design

The diagnostic is a deterministic replay of v34's slot allocation under two
policies. Directional sketch (not implementation spec):

```
load v34 decisions.jsonl + performance.json + minute/daily bars
assert actual_catalog_fingerprint() == 363f199d          # identity guard

# --- base book (FIFO, ground-truth calibration) ---
closed   = performance.json closed trades  # 119 (the ror_base cohort)
placed   = order_placed records            # 128 slot-occupancy events (reconcile 128->119)
resim(t) = walk t's session minute bars from breakout bar:
             stop=range_low, target=entry+1.0*R,
             breakeven at +0.41R -> stop:=entry, flat at 15:00 -> realized_r
assert resim(each closed) ~= performance.json realized_r  # KTD2 self-check
ror_base = sum(rc*r) / sum(rc)  over closed                # == 0.0398

# --- ranked book (Policy D: displacement by OR-width tightness) ---
events = breakouts(placed + blocked) sorted by ts
book   = []                                # live positions, <= max_concurrent (7)
for e in events:
    key(e) = range_R(e) / prior_atr(e.symbol, e.session)   # ascending = tighter
    if len(book) < 7:            admit e
    elif key(e) < max(key(h) for h in book):               # strictly tighter
        W = argmax key(h)                                   # widest held
        book_MTM_exit(W, at=e.ts)                           # mark-to-market at bar close (KTD8)
        replace W with e                                    # cascades naturally
    else:                        drop e (as today)
ror_prime = sum(rc*r) / sum(rc)  over ranked book members (with MTM exits)

population_count = count(filter=="max_concurrent")         # 20 on v34
ror_shift        = ror_prime - ror_base
write {population_count, ror_base, ror_prime, ror_shift} -> argv[-1]
```

`twin.py` computes the same four readings by an independent path (catalog-wide
preload rather than entry-local loaders; parallel-array book state; no shared
functions), and must agree within tolerance.

### U1. diagnostic.py — Policy-D displacement re-sim and gate readings

- **Requirements:** R4, R5, R6, R7.
- **Dependencies:** none (reads committed v34 artifacts).
- **Files:** `adapters/nautilus/lab/candidates/orb-concurrency-slot-ranking/diagnostic.py`
- **Approach:** Follow KTD1–KTD4. Hardcode the v34 `RUN` path (env override for
  fixtures). Re-derive and assert the catalog fingerprint. From `decisions.jsonl`
  extract `breakout` envelopes (per `(symbol, ts)`: `range_high`, `range_low`,
  `breakout_price`), `order_placed` (128 slot-occupancy events: `symbol`, `ts`,
  `qty`, entry, `risk_capital`), the `performance.json` closed trades (119 — the
  `ror_base` cohort), and `max_concurrent` rejects (20 blocked: `symbol`, `ts`,
  `qty`). Build the exit-engine re-sim over minute bars; calibrate against the 119
  closed trades and reconcile the 128→119 placement gap (KTD2). Replay Policy-D
  admission with cascades and MTM-at-close displaced exits (KTD8). Emit the four
  readings to `sys.argv[-1]` as `{key: number}`; write a human report to stdout
  (the gate ignores stdout).
- **Patterns to follow:** `candidates/failed-break-reversal/diagnostic.py`
  (`BARS_HOME`, `actual_catalog_fingerprint`, entry-local loaders, `require(...)`
  fatal-on-violation); `candidates/profit-target-075/diagnostic.py` (the
  `out_path = sys.argv[-1]` + `json.dump(readings)` boundary, size-invariant RoR).
- **Test scenarios** (`uv run` invocable; validated by the self-checks below, not a
  separate suite — KTD2):
  - Covers R6. Base-book self-check: re-simulating v34's 119 closed trades reproduces
    their `performance.json` `realized_r`/pnl within tolerance; `ror_base` rounds to
    `0.039806`. The 128 `order_placed` events reconcile to 119 closed trades + surplus
    placements that occupy slots but are excluded from RoR (KTD2).
  - Covers R5. `population_count` equals the count of
    `decision_detail.filter == "max_concurrent"` rejects (20 on the v34 fixture);
    every reject joins a same-`(symbol, ts)` `breakout` envelope (else FATAL).
  - Covers R7. The live book never exceeds `max_concurrent = 7`; displacement fires
    only when the new breakout's `range_R/prior_ATR` is strictly smaller than the
    widest held; a cascade (X displaces W, then Z displaces X in the same session)
    resolves without exceeding the budget.
  - Covers R7. A displaced position books at the displacement bar's `close`
    (mark-to-market, KTD8), not at its natural stop/target exit.
  - `prior_atr` matches the Rust `backtest.rs:635` result on a spot-checked
    `(symbol, session)` from the v34 window; a symbol with `< 15` priors is handled
    by the frozen `None`-rank rule (KTD4).
  - Identity + integrity guards: FATAL (script-failure) when the catalog fingerprint
    ≠ `363f199d`, or when the max_concurrent→breakout join is incomplete, or when the
    closed-trade self-check diverges beyond tolerance — never a silent GO.

### U2. twin.py — independent recompute

- **Requirements:** R3.
- **Dependencies:** U1 (defines the four readings and their meaning).
- **Files:** `adapters/nautilus/lab/candidates/orb-concurrency-slot-ranking/twin.py`
- **Approach:** Recompute `population_count`, `ror_base`, `ror_prime`, `ror_shift`
  by a deliberately different path — catalog-wide preload rather than entry-local
  loaders, parallel-array book state, no functions shared with `diagnostic.py`. Same
  `sys.argv[-1]` JSON output boundary.
- **Patterns to follow:** `candidates/profit-target-075/twin.py` (independent-path
  discipline, identical I/O boundary).
- **Test scenarios:**
  - Covers R3. Twin readings equal diagnostic readings within `candidate.json`
    tolerances (`population_count` exact; RoR readings within `0.0005`).
  - Structural: `twin.py` imports/defines no function from `diagnostic.py` (the
    independence the twin exists to provide).

### U3. candidate.json — frozen pre-register

- **Requirements:** R1, R2.
- **Dependencies:** U1, U2 (their file bytes back the `content_hash` pins).
- **Files:** `adapters/nautilus/lab/candidates/orb-concurrency-slot-ranking/candidate.json`
- **Approach:** `schema_version 1`; `slug "orb-concurrency-slot-ranking"`;
  `family "reallocation"`; `phase_a "bespoke"`; `flip_param "slot_rank_mode"`,
  `flip_value 1.0` (KTD5); `diagnostic`/`twin` ScriptDecls with
  `argv ["uv","run","--with","pyarrow","python3","<file>.py"]`, `file`, and
  `content_hash` (filled at freeze, U4); `readings` = `population_count`
  {tolerance 0, precision 0}, `ror_base`/`ror_prime`/`ror_shift` {tolerance 0.0005,
  precision 6} (KTD6); `thresholds` = `[{population_count, ge, 12}, {ror_shift, ge,
  0.005}]`; `keep_anchor` = "size-invariant return-on-risk strictly beats real-data
  v34 (0.0398) with risk-cap dominance <= 0.40".
- **Test scenarios:**
  - Loads via `candidates.rs` `load(dir)` without error; `readings` keys are exactly
    those the scripts emit; `thresholds` reference only declared readings.
  - `Test expectation: none` beyond the load/round-trip check — this is a frozen data
    artifact, exercised end-to-end by U4.

### U4. Freeze the package and run `turn diagnose`

- **Requirements:** R8, R9.
- **Dependencies:** U1, U2, U3.
- **Files:** `adapters/nautilus/lab/candidates/orb-concurrency-slot-ranking/candidate.json`
  (content hashes), `adapters/nautilus/lab/candidates/orb-concurrency-slot-ranking/gate-verdict.json`
  (tool output).
- **Approach:** Commit `diagnostic.py` + `twin.py` + `candidate.json`; record their
  `content_hash` + the `freeze_commit` + `pre_register_hash` in `candidate.json`;
  then run `turn diagnose orb-concurrency-slot-ranking`. Capture the GO/STOP, the
  typed exit code, and the tool-written `gate-verdict.json`. GO iff
  `population_count ≥ 12` AND `ror_shift ≥ 0.005`.
- **Execution note:** Freeze-before-reading — the content hashes must be committed
  before `turn diagnose` runs; do not edit any script after seeing the readings, and
  do not soften a threshold to convert a STOP (R9).
- **Test expectation: none** — operational; verified by a non-error tool run that
  writes a well-formed `gate-verdict.json` whose `agreed_readings` match the twin.

### U5. TURN-LOG.md entry and package commit

- **Requirements:** R10, R11, R12.
- **Dependencies:** U4.
- **Files:** `adapters/nautilus/lab/TURN-LOG.md`
- **Approach:** Write the durable verdict entry mirroring the profit-target-075 STOP
  entry shape — verdict line, the frozen gate (thresholds + freeze commit + catalog
  fingerprint), the measurement table (`population_count`, `ror_base`, `ror_prime`,
  `ror_shift`), the why, registry state (head stays v34, no `params.rs`/`orb.rs`
  edit), and family status. On GO, record the downstream arming path (new default-off
  `slot_rank_mode`, seed-and-rerun vN, KEEP anchor RoR > 0.0398 / dominance ≤ 0.40)
  as the next turn — not executed here (R10). Commit the candidate package +
  `gate-verdict.json` + the TURN-LOG entry together.
- **Test expectation: none** — documentation + governance record.

---

## Verification Contract

- `turn diagnose orb-concurrency-slot-ranking` completes without a script-failure
  and writes `gate-verdict.json`; `diagnostic.py` and `twin.py` agree within the
  `candidate.json` tolerances (else exit 10, twin-mismatch).
- The diagnostic's base-book self-check reproduces v34's 119 closed trades' P&L
  within tolerance and `ror_base` rounds to `0.039806` (KTD2).
- `population_count` and `ror_shift` in `gate-verdict.json` drive the GO/STOP per the
  frozen thresholds; on STOP the typed exit is 11 (threshold-fail).
- Head identity untouched: no `params.rs`/`orb.rs` edit; `LS_TURN_EXPECT_VERSION=34`
  still resolves v34 as latest-finalized (R11).
- `cd adapters/nautilus && cargo test --workspace` (`make adapter-check`) stays green
  if any Rust is touched (e.g., a candidate-registry edit at U4); the standalone
  adapter workspace opts out of the root `cargo test`.
- Offline throughout; no gateway is contacted (R12).

## Definition of Done

- The candidate package (`candidate.json` + `diagnostic.py` + `twin.py`) is frozen,
  content-hashed, and committed **before** the reading (freeze-before-reading).
- `turn diagnose` has produced an honest GO or STOP; `gate-verdict.json` is committed
  with matching diagnostic/twin readings, `pre_register_hash`, `freeze_commit`, and
  `catalog_fingerprint 363f199d`.
- No pre-registered threshold was softened and no operator override was invoked after
  the reading (R9).
- A TURN-LOG.md entry records the verdict; head stays **v34** and the pin holds.
- On GO, the downstream arming path + KEEP anchor are recorded but not executed;
  Phase-B build and the flip remain out of scope.
- The whole turn ran offline.
