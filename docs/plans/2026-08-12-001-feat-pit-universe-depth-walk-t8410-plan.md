---
title: Pit-Universe Depth Walk over t8410 (P4) - Plan
type: feat
date: 2026-08-12
topic: pit-universe-depth-walk-t8410
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: docs/plans/2026-08-10-001-docs-next-strategy-lineage-scope-plan.md
execution: code
---

# Pit-Universe Depth Walk over t8410 (P4) - Plan

## Goal Capsule

- **Objective.** Execute ladder rung **P4** (`pit-universe-depth-walk-t8410`): a
  TR-parameterized backward window walk over `t8410` daily bars that produces,
  for a frozen several-hundred-symbol universe, (a) per-symbol listing evidence,
  (b) the universe's effective `S_max`, realized `m` and participation, and the
  margin re-derived from them before turn one, and (c) an upgrade of the 500-row
  page cap and the per-symbol floor from *inferred* (`body_len` discrimination,
  2026-08-10) to *measured* (row-level `cts_date` walk).
- **Product authority.** The parent scope plan
  (`2026-08-10-001`, KD8/R14/R15/Q4) owns every definition this plan consumes:
  the 2016-08-01 floor, the pilot `S_max` 2,457, the spec/holdout split, the
  concurrency identity (`concurrent = m × hold`, ≥70 at `m = 5`, 140 at
  `m = 10`), and the rule that delisting is never inferred from an empty page.
  This plan owns only the harness, the walk protocol, the artifact, and the
  derivation arithmetic.
- **Execution profile.** Two turns (a third is contingency). **Turn 1 (this
  plan's implementation scope): offline only** — walk module + bin + tests +
  gate; zero gateway calls. **Turn 2: the attended live walk** — paced,
  budget-gated, ~400–700 calls (measured, that measurement itself sizing P3's
  ~2,000-call pull) — plus the `derive` pass, the committed artifact, the
  TURN-LOG entry, and the queue close.
- **Stop conditions.** If the live walk finds the 500-row page-cap inference
  wrong in a direction that changes P3's call arithmetic materially, or finds
  `NoServedRows` anomalies on more than a handful of frozen-set members, stop
  and report before deriving — those change what P3 and P6 may assume.
- **Tail ownership.** Queue transition via `lab-next done
  pit-universe-depth-walk-t8410` (after the turn-2 gate), never by editing
  `queue/items.jsonl`.

## Key Technical Decisions

- **KTD1 — The frozen walk set is the board-ranked slice of the existing
  capture, minus numeric-coded preferreds.** From
  `lab/config/universe-metadata-20260723.json` (2,689 records, content-hashed):
  `cap_tier ∈ {top, mid}` = 355 records — the "several hundred" the concurrency
  floor requires (Q4: ≥70–140 concurrent positions; the board-ranked set is the
  only slice with resolved cap evidence, and `liquidity_tier` is `unknown` for
  all 2,689 so no finer liquidity cut exists to take). Two filters apply at
  freeze: the P5 issue-sequence rule (6th digit ≠ 0 → drop; P4 applies the
  *rule* so no budget is spent walking symbols P5 will exclude, while P5 still
  owns the capture-side code), and **no designation gate** (KD7: it is a live
  gate, not a research one). The artifact records the source capture's content
  hash and the resulting shcode list, so the set is frozen by content.
- **KTD2 — Screening protocol: calendar-partitioned windows walked
  oldest-first, one cursor-walk per window.** Partition `[floor, anchor]` into
  windows of ≤ 450 *proven sessions* each (boundaries snapped to proven
  sessions from the calendar snapshot — chart date fields must be pinned to
  real trading days), and probe them oldest-first. Each window is a full
  `cts_date` cursor walk with `chart_all`'s termination discipline (empty
  cursor = clean completion; zero-row page with a live cursor or a repeated
  cursor = suspect, fail closed; page cap bound) — for a ≤450-session window
  this is one page in the expected case, so the fallback *is* the primary and
  there is a single code path. Verdicts per symbol:
  - first window non-empty with earliest row on the window's first proven
    session → **PreFloor** (listing ≤ floor; the exact date is not needed —
    participation is identical);
  - first non-empty window's earliest row later than that → **Listed(date)**,
    the listing date, per the 323410 proof that `t8410` serves from listing;
  - all windows empty → **NoServedRows** — surfaced as an anomaly, *never*
    read as delisting or non-listing (the outcome enum has **no delisting
    variant**, making the forbidden inference unrepresentable — KD8/R14).
  Expected cost: 1 call for pre-floor symbols (the majority), ≤ 6 for
  post-floor listings → ~400–700 calls at the settled 1000 ms pace ≈ 10–15 min,
  versus ~2,000 for naive full walks (which would pre-spend P3's budget).
- **KTD3 — The measurement subset upgrades inferred → measured.** Full
  multi-page walks, per-page row counts and first/last dates recorded: pilot
  `005930` *unbounded* (sdate 19800101 — measures the true per-symbol vendor
  floor against the 1985 `body_len` inference, ~21 pages) plus 3 in-set symbols
  (one KOSDAQ, one known post-floor listing, one mid-tier) over
  `[floor, anchor]` (~6 pages each). This measures: the server page cap (the
  500-row claim, via observed per-page row counts), `rec_count` echo fidelity,
  cursor mechanics across many pages, and calls-per-symbol at the measured cap
  — the figure that sizes P3.
- **KTD4 — TR-parameterization is a fetcher trait, not an enum of TRs.** The
  walk is generic over the existing `ingest::DailyFetcher` trait — the same
  seam the certified daily ingest drives (body `cts_date` cursor only) — with
  a bin-local production impl over `Paginated::stock_chart_period` (t8410). A
  later t8465/o3103/t8418 walk (the arm-D residue) is a new trait impl, not a
  new harness. Offline tests fake the trait, keyed by the full request tuple
  so a swapped symbol or window cannot pass silently. Walk errors carry the
  gateway calls already spent (the capture bin's `CaptureError` pattern), so
  a failed symbol's spend still reaches the shared ledger.
- **KTD5 — Reuse the capture bin's live-run spine.** New bin
  `pit-universe-walk` mirrors `capture-universe-metadata`: `scrub::install()`,
  `LS_TRADING_ENV=paper` refusal, lane file via env, sleep pacing (default
  1000 ms), bounded IGW00201 backoff-and-retry (budget is cumulative and
  warm-sensitive — treat throttles as transient, degrade the *symbol* not the
  run), `budget_gate` + `SpendLedger` wiring so the morning ingest's planner
  sees the walk's spend. Anchor = the calendar snapshot's most recent proven
  session (proof-preserving; refuses rather than stepping past an Unknown).
- **KTD6 — `derive` is an offline subcommand, mechanical, formulas cited.**
  Reads the walk artifact + calendar snapshot; emits per-session listed-count
  `N(s)` over the proven sessions of `[floor, anchor]`, the effective `S_max`
  at the stated concurrency thresholds (70 and 140), each symbol's session
  participation, and the margin bars re-derived with the parent plan's own
  scaling (`SE(S) = 0.087002 × √(45/S)`; projection caveat carried verbatim).
  `N(s)` is an **upper bound** on tradable count (delisting unmeasurable);
  the R14 haircut owns that bias — restated in the artifact, not resolved here.
- **KTD7 — One committed artifact, no gitignored-only evidence.** Turn 2 writes
  `lab/config/pit-universe-<anchor>.json`: provenance (source capture hash,
  anchor, floor, pace, calls made, probed_at, every freeze exclusion), the
  per-symbol outcomes, the measurement subset's per-page records, and the
  `derive` block. `data/` is gitignored, so the durable form is the committed
  record + TURN-LOG (the parent plan's own convention). Two integrity guards:
  a **restricted** run (`LS_PIT_SYMBOLS`, the re-run/repair tool) writes to a
  `-partial` path and never carries a derived block — a subset's `N(s)` must
  not read as the frozen universe — and `derive` refuses when the current
  calendar's session structure (windows + unknown days, not just the count)
  disagrees with walk-time provenance.

## Implementation Scope (turn 1)

1. `adapters/nautilus/src/reference/pit_walk.rs` — the walk module:
   `ChartDayFetcher` trait + t8410 impl, window partitioning from the calendar
   snapshot, the per-window cursor walk with `chart_all` termination
   discipline, the outcome enum (`PreFloor` / `Listed` / `NoServedRows` — no
   delisting variant), per-page measurement records, artifact schema
   (serde), and the `derive` arithmetic. Registered in `reference/mod.rs`.
2. `adapters/nautilus/src/bin/pit-universe-walk.rs` — `walk` and `derive`
   subcommands on the KTD5 spine.
3. Offline tests (fake fetcher): pre-floor / in-window-1 / later-window /
   all-empty verdicts; window snapping to proven sessions; cap-guard
   fail-closed arms (zero-row live cursor, repeated cursor, page-cap
   exhaustion); per-page recording; derive arithmetic against a synthetic
   calendar (including the effective-`S_max` threshold crossings).
4. Gate: root `cargo test` + `make adapter-check` (adapter code changed;
   `env -u` every `LS_*`; redirect, never pipe to `tail`), `make lane-check`,
   `make docs-check` (no metadata change expected), `make todo-check`.

## Explicitly out of scope (turn 1)

- Any gateway call. The live walk, the artifact, TURN-LOG, and the queue close
  are turn 2.
- P3's pull, P5's capture-side filter code, P6's prereg fields — separate
  rungs; this plan only feeds them numbers.
- `session-morning.sh` / preflight-registry changes — the walk is not on the
  morning chain.

## Verification Contract

- The forbidden-inference invariant is structural: no test, and no code path,
  can produce a "delisted" reading — reviewers should look for the *absence*
  of the variant.
- A window walk that exhausts `MAX` pages, repeats a cursor, or sees a
  zero-row page with a live cursor **fails closed** (error, symbol degraded and
  surfaced) — silence is never completion.
- The measured page cap is reported, not assumed: derive refuses to run if the
  measurement subset's records are absent from the artifact.

## Open questions (defaults recorded, none block turn 1)

- Q1. Exact measurement-subset membership beyond `005930`? Default: chosen at
  turn 2 from the frozen set (one KOSDAQ top-tier, one post-floor listing
  identified by the screening pass itself, one mid-tier).
- Q2. Anchor for turn 2? Default: the snapshot's most recent proven session on
  the run date; recorded in the artifact.
- Q3. Concurrency thresholds to tabulate in `derive`? Default: 70 and 140 (the
  parent plan's two operative cells), plus the full `N(s)` minimum/median so
  P6 can pick differently without a re-run.
