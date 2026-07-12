---
title: "feat: Strategy loop turn 5 — diagnose the 6-trade constraint, then run the param flip it selects"
date: 2026-07-09
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
product_contract_source: ce-plan-bootstrap
type: feat
origin_context:
  - memory: strategy-loop-turn-4-widen-universe-2026-07-09 (turn-4 outcome + gotchas)
  - docs/solutions/conventions/strategy-loop-param-turn-governance-and-fresh-home-seeding.md
  - shipped: PR #110 (merged bd78c1f) — scaled bar + probe retarget
---

# feat: Strategy loop turn 5 — diagnose the 6-trade constraint, then run the param flip it selects

## Summary

Turn 4 falsified universe width as the trade-frequency lever (trades flat at 6
across N=20/30/40) and pointed at a parameter flip. Before spending a governed turn
on a guess, **diagnose why trades are flat at exactly 6** from the existing v5 run
artifacts (no new backtest), then **run turn 5 — the single param flip the diagnosis
selects** — and author the verdict against the scaled bar. Fully offline (reads the
local `data/turn4-fresh` catalog; no gateway). Attended.

The diagnosis is the point: a gap flip only helps if the gap filter is the
bottleneck. If the backtest effectively trades one session, a gap flip is the wrong
turn and the lever is `range_minutes`/sessions instead.

---

## Problem Frame

Turn 4 (v5, `universe_top_n=40`, `gap_min_pct=0.6`) produced 6 realized trades, each
a single fill on a distinct symbol — the same 6 as N=20 and N=30. The N=40 bar (60
trades / 12 breadth-symbols) failed hard. The turn-4 verdict named a `gap_min_pct`
flip as the primary next lever *and* flagged a competing hypothesis (single-session
backtest) that would make that flip useless. This turn resolves which is true, then
acts.

Three hypotheses for the flat-6:
- **(a) Gap filter too tight** — only ~6 symbols clear the 0.6% gap each session.
- **(b) Single-session backtest** — the harness effectively trades one session, so
  trade count is bounded by that day's setups regardless of universe or gap
  (turn-2b noted "backtest trades LAST in-range session" — unconfirmed at v5).
- **(c) `max_concurrent=5` cap** — positions capped (6 total trades observed, so
  likely not binding; verify and dismiss or confirm).

---

## Requirements

- **R1 — Evidence-first lever choice.** The param flipped in turn 5 is chosen from
  the diagnosis, not assumed. The diagnosis is read from the v5 run artifacts (no
  new backtest).
- **R2 — Correct hypothesis discrimination.** Distinguish (a)/(b)/(c) with concrete
  artifact evidence: gap-rejection counts, distinct trade/decision dates, and
  concurrent-position counts — each named, not eyeballed.
- **R3 — Governed single-param turn.** Turn 5 changes exactly one param, ≤0.5
  relative (proposal-bounds cap), with the seed-assertion pinned to the v5 base
  (`EXPECT_VERSION=5`, `EXPECT_GAP=0.6`), same window `2026-05-26..2026-07-03`, on
  `data/turn4-fresh`. Bumps v5→v6.
- **R4 — Verdict against the scaled bar.** Evaluate the computed bar (still 60/12 at
  N=40) from the v6 analysis scaffold; author keep/revert/insufficient-evidence per
  R1 of the loop (verdict valid only if the bar clears).
- **R5 — Stop condition honored.** If the diagnosis shows (b) single-session, a gap
  flip is the *wrong* turn — surface it and flip `range_minutes` (or escalate the
  session question) instead.

---

## Key Technical Decisions

- **KTD-1 — Diagnose from `decisions.jsonl`, not a re-run.** It's a per-candidate
  envelope stream recording every candidate evaluated plus the rejecting
  filter/signal values (`gap_pct`, `rank`, etc.). The gap-rejection rate and the
  distinct decision dates are already in the v5 run — reading them is cheaper and
  more direct than instrumenting a new backtest.
- **KTD-2 — Gap flip is the default, single-session is the trap.** If (a) holds,
  `gap_min_pct` 0.6→0.4 (+0.33 relative, legal). The single-session check (b) is the
  gate that must clear *first* — a legal, well-formed gap turn that runs against a
  one-session harness would still land flat and waste the turn.
- **KTD-3 — The base is v5, already seeded.** `data/turn4-fresh` already has the v3
  seed manifest and the v4/v5 runs; `latest_finalized_run` resolves v5. No
  re-seeding. `EXPECT_VERSION=5`/`EXPECT_GAP=0.6` asserts that base.
- **KTD-4 — Offline.** No `LS_TRADING_ENV`, no gateway; the turn reads the local
  catalog.

---

## Implementation Units

### U1. Diagnose the flat-6 constraint from the v5 run artifacts

- **Goal:** Determine which of (a)/(b)/(c) bounds the trade count, producing a named
  lever for U2.
- **Requirements:** R1, R2 (KTD-1).
- **Dependencies:** none.
- **Files (read-only):**
  - `data/turn4-fresh/runs/<v5-run>/decisions.jsonl` (per-candidate envelopes)
  - `data/turn4-fresh/runs/<v5-run>/performance.json`
  - `adapters/nautilus/lab/src/strategy/orb.rs` — stocks-in-play scan (`orb.rs:70`),
    universe cap (`orb.rs:113`), the opening-range breakout phase machine
  - `adapters/nautilus/lab/src/runner/research.rs` — how the backtest drives sessions
- **Approach:** Three evidence reads, each answering one hypothesis:
  - **(a) gap filter:** count candidates rejected at the gap filter vs passing, from
    `decisions.jsonl` (the envelope carries `gap_pct` and the rejecting filter). A
    high reject-at-gap rate with few passers → gap is the bottleneck.
  - **(b) single-session:** extract the distinct dates the strategy actually *traded*
    (fills), not just evaluated, across `decisions.jsonl` `ts_event` — one date → the
    harness trades a single session; many → it spans the window. Cross-check against
    the session-drive loop in `research.rs`.
  - **(c) concurrency:** the max simultaneously-open positions vs `max_concurrent=5`.
    If peak < 5, the cap is not binding — dismiss.
- **Execution note:** This is analysis, not a change — the deliverable is a named
  lever + the evidence for it. Do not run a backtest in this unit.
- **Test scenarios:** `Test expectation: none -- read-only diagnosis of existing
  artifacts.` Correctness is the evidence itself (counts + dates), recorded in the
  turn's notes.
- **Verification:** a written one-paragraph diagnosis naming the binding constraint
  and the lever it implies (gap / range_minutes / concurrency), each backed by a
  concrete count or date-set from the artifacts.

### U2. Run turn 5 — the single param flip the diagnosis selects

- **Goal:** Execute the governed v5→v6 param turn on the lever U1 identified.
- **Requirements:** R3, R5 (KTD-2, KTD-3, KTD-4).
- **Dependencies:** U1.
- **Files:** produces `data/turn4-fresh/runs/<stamp>-backtest-orb-v6/` (manifest,
  performance, decisions, data_quality).
- **Approach:** One `lab-research turn` with the selected override, seed-assertion
  pinned to the v5 base. If the diagnosis is **(a)** — the default —
  `LS_TURN_PARAM=gap_min_pct LS_TURN_VALUE=0.4` (0.6→0.4, +0.33, legal). If **(b)** —
  `LS_TURN_PARAM=range_minutes LS_TURN_VALUE=10` (15→10, −0.33, legal), OR stop and
  escalate the multi-session question if the harness is structurally single-session.
  If **(c)** — raise `max_concurrent` within the bound. In every case
  `LS_TURN_EXPECT_VERSION=5 LS_TURN_EXPECT_GAP=0.6` (asserts the *base*; gap is only
  the flipped value when the lever is gap), window `20260526..20260703`,
  `LS_DATA_HOME=<repo>/data/turn4-fresh`.
- **Execution note:** The one write step. If the turn refuses (proposal-bounds or
  seed-assertion), fix the value/base — never drop the `EXPECT_` guards or edit the
  cap to force it (see the conventions doc). A `>0.5` intended change means a legged
  turn, not a bypass.
- **Test scenarios:** `Test expectation: none -- execution of committed code over
  local data.` The turn's refuse-on-mismatch guards + U3 are the verification.
- **Verification:** a `*-orb-v6` run finalized; manifest shows `strategy_version:6`,
  the flipped param at its new value, all other params unchanged, range
  `20260526..20260703`, exit 0.

### U3. Evaluate the scaled bar + author the turn-5 verdict

- **Goal:** Decide keep/revert/insufficient for v6 and name the next lever.
- **Requirements:** R4.
- **Dependencies:** U2.
- **Files:** `data/turn4-fresh/runs/<v6-run>/analysis.md` (scaffold, then verdict).
- **Approach:** Generate the scaffold (`lab-research analyze --scaffold`,
  `LS_ANALYZE_RUN=<v6-run>`), read the computed bar (60/12 at N=40): (a) trades ≥ 60,
  (b) breadth ≥ 12 symbols each ≥ `SYMBOL_TRADE_FLOOR`, (c) dominance ≤ cap. Run a
  param-mode `runs compare` v5→v6 (data consistency). Author the verdict:
  - Bar clears → keep v6 (the lever works); loop advances.
  - Bar fails → insufficient-evidence, naming the failing condition and how far off,
    plus the next lever (further flip, combine levers, or reconsider the harness if
    the single-session finding stands).
- **Execution note:** Read the *computed* bar, don't eyeball trade counts.
- **Test scenarios:** `Test expectation: none -- analysis/verdict authoring.`
- **Verification:** param-compare PASS; a written verdict grounded in the computed
  bar + a named next lever; if kept, v6 is the new registry head.

---

## Scope Boundaries

**In scope:** read-only diagnosis of the v5 artifacts; one governed v5→v6 param turn
on the selected lever; verdict.

**Deferred to follow-up work:**
- Multi-session backtest support, if U1 finds the harness is structurally
  single-session — that's a harness change, a separate feature, not a param turn.
- Combining levers (e.g. gap + range together) — a later turn; this one flips one
  param to keep the signal clean.
- U6 IGW00201 true-ceiling measurement (unrelated, live).

**Out of scope:** live-order execution; any gateway/ingest work (the data is already
in place and this turn is offline).

---

## Verification Contract / Definition of Done

- U1: a named binding constraint + lever, each backed by a concrete count/date-set
  from the v5 artifacts.
- U2: v6 run finalized — `strategy_version:6`, the one flipped param at its new
  value, everything else unchanged, correct window, exit 0; no guard bypassed.
- U3: computed 60/12 bar read (not eyeballed); param-compare PASS; verdict authored
  (keep v6 | insufficient + next lever).

**Done =** diagnosis-selected lever flipped as a governed turn + a computed-bar
verdict + the next lever named.

---

## Open Questions (execution-time)

- **Which lever U1 selects** is the whole point of U1 — not pre-decided. The plan
  carries the command for each branch so U2 is mechanical once U1 lands.
- **If the harness is single-session (b):** whether to flip `range_minutes` as a
  stopgap or escalate to a multi-session harness change is a judgment call to record
  in the U1 diagnosis, informed by how the session loop in `research.rs` actually
  drives days.

---

## Sources & Research

- Turn-4 outcome + the flat-6 finding: memory
  `strategy-loop-turn-4-widen-universe-2026-07-09`.
- Governance (proposal-bounds cap, seed-assertion, offline, runs-compare modes):
  `docs/solutions/conventions/strategy-loop-param-turn-governance-and-fresh-home-seeding.md`.
- Entry logic: `adapters/nautilus/lab/src/strategy/orb.rs` (stocks-in-play `:70`,
  universe cap `:113`).
- Turn flow + seed-assertion: `adapters/nautilus/lab/src/runner/research.rs`
  (`turn`, seed-assertion ~`:234-253`, rerun-vs-param ~`:268-375`).
