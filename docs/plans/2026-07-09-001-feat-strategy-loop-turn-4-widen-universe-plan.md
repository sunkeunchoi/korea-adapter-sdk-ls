---
title: "feat: Strategy loop turn 4 — widen universe 20→40 (v3→v4 param turn)"
date: 2026-07-09
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
product_contract_source: ce-plan-bootstrap
type: feat
branch: feat/strategy-loop-turn-4-widen-param-flip
origin_context:
  - docs/plans/2026-07-07-003 (turn-3 broaden-sample plan)
  - memory: strategy-loop-turn-3-broaden-sample-2026-07-07
  - memory: igw00201-budget-characterization-2026-07-08 (U6/U7 data work, committed 507ed6c)
---

# feat: Strategy loop turn 4 — widen universe 20→40 (v3→v4 param turn)

## Summary

Execute the turn-4 turn of the ORB strategy loop: widen the traded universe from 20
to 40 KOSPI top-cap names to beat turn-3's **insufficient-evidence-at-n=6** verdict.
Change exactly one governed variable — `universe_top_n` 20→40 — over the *same* data
window as turn-3 (`2026-05-26..2026-07-03`), so the result is directly comparable. The
U7 data is already ingested and verified (`data/turn4-fresh`, GO + 40/40 minute
coverage). This turn is **offline** (the backtest reads the local catalog; no paper
gateway) and attended-optional.

This is a governed **param turn** (v3→v4), not a held-v3 rerun: widening
`universe_top_n` goes through `LS_TURN_PARAM`, which bumps the strategy version and
asserts the seed. The R1 decisiveness bar **scales with universe size** — at N=40 it is
**60 trades / 12 breadth-symbols** (the `1.5·N` / `0.30·N` re-registration already
sitting as uncommitted WIP on the branch).

---

## Problem Frame

Turn-3 held strategy v3 (gap 0.6) over 20 symbols × 28 sessions and produced **6 trades
/ 6 symbols / +320k** — below the decisiveness bar (trade floor FAIL, breadth
borderline), so its verdict was **insufficient evidence at n=6**. The loop's next move
is to widen the sample. U6/U7 (committed `507ed6c`) delivered the enabling data: a
frozen 40-symbol universe (t1444 pagination live-verified) drip-ingested into
`data/turn4-fresh` over turn-3's exact window, every series 10,668 bars.

What remains is the **turn itself**: land the scaled-bar re-registration, seed the
fresh home with the v3 identity, run the v3→v4 param turn, and author a go/no-go verdict
against the scaled bar. Three facts make this non-trivial and must not be skipped:

1. The branch carries **uncommitted turn-4 WIP** (`performance.rs`, `research.rs`,
   `CONCEPTS.md`) that re-registers the bar to scale with `universe_top_n`. This is the
   turn's bar; it must land first.
2. A fresh data home has **no seeded v3 manifest**, so the KTD-5 seed-assertion
   (`EXPECT_VERSION=3`/`EXPECT_GAP=0.6`) will **refuse** until a v3 run manifest is
   copied into its registry — this is a deliberate guard against a silent default-param
   (v0, gap 3.0) backtest.
3. `catalog status: GO` is necessary but **not sufficient** — it reads deduped coverage.
   The turn already has 40/40 verified (U7), but the verdict must still be authored
   against the computed bar, not eyeballed.

---

## Requirements

- **R1 — Decisiveness bar (scaled, pre-registered).** The keep/revert verdict is valid
  only if the computed bar is cleared. At the pinned N=40: **(a)** total realized trades
  ≥ `trade_floor(40) = 60`; **(b)** breadth ≥ `breadth_floor(40) = 12` symbols each with
  ≥ `SYMBOL_TRADE_FLOOR` trades; **(c)** dominance ≤ `DOMINANCE_CAP` (scale-invariant,
  unchanged). The bar is a function of the **pinned** `universe_top_n`, never the
  realized snapshot.
- **R2 — One variable.** Only `universe_top_n` changes (20→40). `gap_min_pct` stays 0.6;
  `strategy_id` and all other params unchanged. The seed-assertion enforces this.
- **R3 — Comparability.** Same window as turn-3 (`2026-05-26..2026-07-03`), same data
  fidelity (10,668 bars/symbol). The data-turn compare vs turn-3 must **PASS**.
- **R4 — Governance.** The change is executed as a param turn (v3→v4) with the
  refuse-on-mismatch seed-assertion, so the turn cannot silently drift from an
  unexpected base.
- **R5 — Verdict.** Author one of the loop's three verdict words, gated on R1: if the
  bar clears → **keep v4** (evidence now sufficient); if not → **insufficient**, and
  specify the next turn (wider N or a param flip).

---

## Key Technical Decisions

- **KTD-1 — Param turn, not rerun.** `universe_top_n` gates trading breadth
  (`orb.rs`: `if rank < params.universe_top_n`), so it is a governed strategy param.
  Widening it is a `LS_TURN_PARAM=universe_top_n LS_TURN_VALUE=40` turn that bumps v3→v4
  (`research.rs` turn flow, `current_version + 1`). A "held-v3 rerun" would require
  mutating the default and dodges the version-bump governance — rejected.
- **KTD-2 — Scaled bar lands as the turn's first unit.** The `performance.rs` WIP
  generalizes the turn-3 constants (`TRADE_FLOOR=30`, `BREADTH_SYMBOL_FLOOR=6` at N=20)
  to `trade_floor(N)=round_half_up(1.5·N)`, `breadth_floor(N)=round_half_up(0.30·N)`,
  reducing to 30/6 at N=20 (backward-compat, R1b/AE5) and giving 60/12 at N=40. It must
  be committed and gated before the verdict is computed against it.
- **KTD-3 — Seed the fresh home before the turn.** Copy a v3 run's `manifest.json` (from
  `data/turn3`'s newest `*-orb-v3` run) into `data/turn4-fresh`'s `runs/` registry so
  `latest_finalized_run` resolves v3/gap-0.6. Without it the seed-assertion refuses
  (`research.rs:236-241`). This is the KTD-5 guard working as designed, not a workaround.
- **KTD-4 — Offline turn.** The backtest consumes the local `data/turn4-fresh` catalog;
  no `LS_TRADING_ENV=paper`, no gateway. Attended presence is optional.

---

## Implementation Units

### U1. Land the scaled decisiveness bar (commit the turn-4 WIP)

- **Goal:** Commit the uncommitted bar re-registration so the verdict is computed
  against the N-scaled floors (60/12 at N=40), not turn-3's fixed 30/6.
- **Requirements:** R1, R4 (KTD-2).
- **Dependencies:** none (first unit).
- **Files:**
  - `adapters/nautilus/lab/src/artifacts/performance.rs` (WIP: `round_half_up`,
    `trade_floor(n)`, `breadth_floor(n)`, `bar_evaluation(universe_size)`)
  - `adapters/nautilus/lab/src/runner/research.rs` (WIP: passes
    `manifest.params.universe_top_n` into `bar_evaluation`, renders scaled floors)
  - `CONCEPTS.md` (WIP: decisiveness-bar vocabulary update)
  - existing tests in `performance.rs` covering the bar
- **Approach:** The WIP is already gate-green (adapter `cargo test --workspace` 471
  passed with it present). Verify the backward-compat assertions exist and pass
  (N=20→30/6 per R1b/AE5; N=40→60/12), run the gate, then commit **only** these three
  files — do not sweep in any other branch WIP.
- **Test scenarios:**
  - `round_half_up`: `1.5·20=30.0→30`, `0.30·20=6.0→6`, `1.5·40=60.0→60`,
    `0.30·40=12.0→12`; a fractional case (e.g. `round_half_up(4.5)=5`) proves the rule.
  - `trade_floor`/`breadth_floor`: `Covers AE5.` N=20→(30,6) and N=40→(60,12).
  - `bar_evaluation(universe_size)`: an evaluation at N=40 uses 60/12, not 30/6; the
    dominance cap is unchanged.
- **Verification:** adapter `cargo test --workspace` green; the three files committed on
  `feat/strategy-loop-turn-4-widen-param-flip`; `git status` shows the other pre-existing
  WIP still uncommitted and untouched.

### U2. Seed `data/turn4-fresh` with the v3 identity manifest

- **Goal:** Give the fresh home a v3 run in its registry so the turn's seed-assertion
  resolves v3/gap-0.6 instead of refusing (or, worse, silently running v0/gap-3.0).
- **Requirements:** R2, R4 (KTD-3).
- **Dependencies:** none (independent of U1; can run in parallel).
- **Files:** `data/turn4-fresh/runs/` (gitignored data home — seeding only, no source
  change).
- **Approach:** Locate the newest v3 run in `data/turn3` (`runs/*-backtest-orb-v3/`),
  confirm its `manifest.json` reports `strategy_version: 3`, `universe_top_n: 20`,
  `gap_min_pct: 0.6`, and copy that run's manifest into `data/turn4-fresh/runs/` in the
  shape `latest_finalized_run` expects (mirror the turn-3 registry layout). Follow the
  exact instruction the KTD-5 refusal prints if the shape is unclear.
- **Test scenarios:** `Test expectation: none -- data-home seeding, no source change.`
  Correctness is proven by U3's dry seed-assertion.
- **Verification:** a dry `turn`-config resolution (or the start of U3) reports "resolved
  strategy v3 (gap 0.6)" rather than the `expected v3 — fresh home is missing the seeded
  v3 manifest` refusal.

### U3. Execute the v3→v4 param turn on the 40-symbol universe

- **Goal:** Run the backtest that widens `universe_top_n` to 40 over turn-3's window and
  finalizes a v4 run with the scaled bar computed.
- **Requirements:** R1, R2, R3, R4 (KTD-1, KTD-4).
- **Dependencies:** U1 (scaled bar committed), U2 (fresh home seeded).
- **Files:** produces run artifacts under `data/turn4-fresh/runs/<stamp>-backtest-orb-v4/`
  (`manifest.json`, `performance.json`, `decisions.jsonl`, `data_quality.json`).
- **Approach:** Invoke the turn with the universe override and the seed-assertion pinned
  to the v3 base. Run recipe (offline; from `adapters/nautilus`, `lab-research` binary):
  - `LS_DATA_HOME=<repo>/data/turn4-fresh`
  - `LS_TURN_PARAM=universe_top_n LS_TURN_VALUE=40`
  - `LS_TURN_EXPECT_VERSION=3 LS_TURN_EXPECT_GAP=0.6` (assert the v3/0.6 base seed)
  - `LS_TURN_SDATE=20260526 LS_TURN_EDATE=20260703`
  - `./target/debug/lab-research turn`
  The turn refuses-on-mismatch if the base is not v3/0.6 (U2 guarantees it) or if the
  applied override key set diverges from `{universe_top_n}`. On success it bumps v3→v4,
  ranks the 40 ingested names, caps at `universe_top_n=40`, and finalizes the run.
- **Execution note:** This is the one irreversible-ish step (writes a finalized v4 run).
  If it refuses, fix the seed/base (U2) — do **not** drop the EXPECT guards to force it
  through; the guard is the point.
- **Test scenarios:** `Test expectation: none -- execution of committed code over real
  data.` The turn's own refuse-on-mismatch and the U4 checks are the verification.
- **Verification:** exit 0; a `*-orb-v4` run finalized; the run's `manifest.json` shows
  `strategy_version: 4`, `universe_top_n: 40`, `gap_min_pct: 0.6`, data range
  `2026-05-26..2026-07-03`.

### U4. Evaluate the scaled bar + author the go/no-go verdict

- **Goal:** Decide whether widening to 40 clears the decisiveness bar and record the
  loop verdict + next move.
- **Requirements:** R1, R3, R5.
- **Dependencies:** U3.
- **Files:** the v4 run's `analysis.md`/scaffold output under
  `data/turn4-fresh/runs/<v4-run>/`.
- **Approach:**
  1. **Data-turn compare vs turn-3** (R3) must **PASS** — same window/fidelity; any data
     delta needs an explanation (`LS_COMPARE_EXPLANATION`), else the compare FAILs.
  2. Read the **computed** R1 bar from the analysis scaffold: (a) trades ≥ 60,
     (b) breadth ≥ 12 symbols each ≥ `SYMBOL_TRADE_FLOOR`, (c) dominance ≤ cap. Do not
     eyeball — the scaffold renders per-condition PASS/FAIL.
  3. **Author the verdict (R5):**
     - **All three PASS →** evidence sufficient → **keep v4**; the loop advances.
     - **Any FAIL →** **insufficient**; specify the next turn — widen N further (e.g.
       60/80, which needs a fresh ingest turn) or flip a param (gap/opening-range) — and
       note which condition failed and by how much.
- **Test scenarios:** `Test expectation: none -- analysis/verdict authoring.` The
  computed bar and the data-turn compare are the objective gates.
- **Verification:** data-turn compare PASS; a written verdict (keep v4 | insufficient +
  next-turn spec) grounded in the computed bar; if kept, the v4 run is the new registry
  head.

---

## Scope Boundaries

**In scope:** commit the scaled-bar WIP; seed the fresh home; run the v3→v4
`universe_top_n=40` param turn over turn-3's window; evaluate + author the verdict.

**Deferred to follow-up work:**
- Further widening (N=60/80) — requires a fresh U7-style ingest turn (more symbols,
  more gateway calls) before the strategy turn.
- Any **param** flip (gap, opening-range length) — a separate governed turn; this turn
  changes only `universe_top_n`.
- Pushing branch / opening a PR / merging — out of scope for the turn itself.
- The true IGW00201 ceiling/refill measurement (U6 follow-up) — unrelated to this turn.

**Out of scope (not this loop's identity):** live-order execution; promoting t1444
(the universe is a disclosed mild look-ahead, `not promoted`).

---

## Verification Contract / Definition of Done

- U1: scaled bar committed, adapter `cargo test --workspace` green, backward-compat
  (N=20→30/6) and N=40→60/12 asserted.
- U2: fresh home resolves v3/gap-0.6 (no seed-assertion refusal).
- U3: v4 run finalized — `strategy_version: 4`, `universe_top_n: 40`, `gap_min_pct: 0.6`,
  range `2026-05-26..2026-07-03`, exit 0.
- U4: data-turn compare vs turn-3 **PASS**; the R1 scaled bar (60/12) computed, not
  eyeballed; a verdict authored (keep v4 | insufficient + next-turn spec).
- No unrelated branch WIP swept into any commit.

**Done =** v4 run finalized on the 40-symbol universe + computed-bar-grounded verdict +
data-turn compare PASS.

---

## Open Questions (execution-time)

- **Exact seed registry shape (U2):** whether copying just `manifest.json` suffices or
  the whole run dir is needed for `latest_finalized_run` to index it — resolve by reading
  the KTD-5 refusal's printed instruction and mirroring the turn-3 registry layout.
- **If the bar still FAILs at N=40:** whether the next turn widens N or flips a param is
  a loop-strategy call to make *when the numbers are in* (record it in the U4 verdict),
  not pre-decided here.

---

## Sources & Research

- Turn-3 outcome + loop discipline: memory `strategy-loop-turn-3-broaden-sample-2026-07-07`;
  plan `docs/plans/2026-07-07-003-*`.
- U6/U7 data work (this session, committed `507ed6c`): memory
  `igw00201-budget-characterization-2026-07-08`; `data/turn4-fresh` (GO + 40/40).
- Turn mechanics: `adapters/nautilus/lab/src/runner/research.rs` (`turn`,
  seed-assertion `:234-253`, rerun-vs-param `:268-375`, version bump `:313/:374`).
- Scaled bar: `adapters/nautilus/lab/src/artifacts/performance.rs` (WIP: `round_half_up`,
  `trade_floor`/`breadth_floor`, `bar_evaluation(universe_size)`).
- Universe cap: `adapters/nautilus/lab/src/strategy/orb.rs:113` (`rank < universe_top_n`).
