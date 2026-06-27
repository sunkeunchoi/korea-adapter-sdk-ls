---
date: 2026-06-26
topic: closed-window-breadth-flip-wave
---

# Closed-Window Breadth Flip Wave — Requirements

## Summary

A maximal closed-window TR expansion wave. First batch-track **all ~60
closure-viable raw TRs** (historical/period, chart, master, static reference,
designation reads) to Tracked in one mechanical PR, then in stacked follow-up
PRs raw-probe-prefilter and flip **every TR that probes non-empty** to
Implemented, grouped by shared build template. There is no fixed flip ceiling —
probe yield sets the count. The wave runs while the KRX market is closed, so its
success is faithful disposition of every attempt — not a target flip count.

## Problem Frame

Most flip waves wait for an open KRX window because the certifying Paper Live
Smoke needs live data. The closed-window flip wave (PR #60, `f2887da`) showed an
exception: t1310 (historical chart) and t1404 (designation board) — two reads
hand-curated per-TR as session-independent — flipped clean on non-empty smokes
*under closure*. That is a suggestive precedent, not proof that ~60 reads
selected by Korean-title heuristic will serve under closure; the premise must be
validated per-TR (see R0). The raw OpenAPI capture carries ~93 unimplemented
reads; ~60 of them are historical, chart, master, or static-reference shapes
that *plausibly* return data regardless of market hours. Today they sit raw —
not callable, not even Tracked — so the SDK's offline-read surface is narrower
than the gateway may support while closed.

## Key Decisions

- **Validate the closure-yield premise before tracking at scale.** A pilot
  probe (R0) raw-probes ~8 representative TRs (chart, master, designation,
  ranking) under closure *before* PR 1 tracks all ~60. If the clean rate is low,
  re-scope the track-all count down rather than tracking ~60 on faith — the
  ~60-TR tracking cost is non-trivial to reverse.

- **Track-all first, then stacked flip-batches.** Batch-tracking is mechanical
  and projectable (metadata + `make api-drift-renormalize` baseline); flipping
  needs per-TR build + smoke. Separating them keeps PR 1 a single reviewable
  count-churn and lets each flip group land as its own reviewable PR, instead of
  one ~60-TR mega-diff with every count site churning at once.

- **Raw-probe gate before every build.** Title heuristics overstate closure-
  viability. Each TR slated for a flip is raw-probed *before* its struct is
  written; only a non-empty body earns the full build. Empty `00707` is recorded
  PENDING at zero build cost. The closed-window clean rate for this specific pool
  is unknown until probed — the prior wave's ~61% rate came from an open-window
  run over an unrelated TR set and does not predict it.

- **Success is faithful disposition, not the flip count — with a yield floor.**
  Every closure-viable TR that probes non-empty is attempted — no cap. Each
  PENDING is a correct, recorded outcome, not a failure to force. But a wave that
  flips near zero means the closure-viability heuristic was wrong, not that the
  wave succeeded: if the R0 pilot clean rate (or the running batch-A rate) falls
  below ~40%, stop tracking the remaining pool and record the heuristic as
  falsified rather than tracking ~60 TRs that mostly cannot flip.

- **Flip by build-template proximity.** The chart/price siblings of already-
  Implemented TRs (t1305, t1310, t1514) flip first because their templates are
  proven; t8411 is itself a batch-A flip target (already Tracked, not yet
  Implemented). Bespoke static reads flip later and are selected by probe yield.

## Requirements

**Pilot (before PR 1)**

- R0. Before tracking at scale, raw-probe ~8 representative closure-viable TRs
  (spanning chart, master, designation, ranking) under closure and record the
  clean (non-empty) rate. If it falls below ~40%, re-scope R1's track-all count
  down and record the closure-viability heuristic as partially falsified rather
  than tracking the full ~60.

**Tracking (PR 1)**

- R1. Batch-track every closure-viable raw TR (count set by R0) from the sweep to
  `support: tracked` via the `track-tr` recipe: author `metadata/trs/<tr>.yaml`
  + `tr-index.yaml` entry, project the normalized baseline with
  `make api-drift-renormalize` (never hand-author it).
- R2. A TR that the sweep classified closure-viable but a quick re-read shows is
  actually intraday/quote/realtime is dropped from the flip set and recorded
  window-gated; if it was already tracked in PR 1 it stays Tracked (not flipped),
  otherwise it is left for the deferred window-gated set. Closure-viability is
  confirmed per TR, not trusted from the title heuristic. (The ~21 already-known
  window-gated reads in Scope Boundaries are a distinct set, not part of the
  closure-viable sweep.)
- R3. PR 1 makes no flips: it bumps the tracking surface only, leaves every
  tracked TR `implemented: false`, and stays green through the full gate. PR 1's
  success criterion is the Raw→Tracked surface bump itself — distinct from the
  flip-disposition criterion that governs the stacked flip PRs.

**Flipping (stacked PRs)**

- R4. Flip batch A is the proven-template chart/price family: t8411 (already
  Tracked), t1302 (↔t1305), t8417/t8418 (↔t8411 sector charts), t8452/t8453
  (integrated 주식챠트 N분/틱).
- R5. Flip batch B is the period-price and derivatives-chart family with no
  implemented sibling: t8405 (주식선물기간별주가), t2216, t8464, t8465, t8466.
- R6. Flip batch C is every static reference/designation read that raw-probes
  non-empty under closure — no curated cap (candidates: t1444 시가총액상위,
  t1422/t1427 상·하한, t1442 신고/신저가, t1405 매매정지/정리매매,
  t1926/t1921 신용정보, t1532/t1533 테마, t1764 회원사리스트,
  t1903/t1954 일별추이, ELW rankings t1960/t1961/t1966). Membership is decided
  by probe yield at build time; a TR that probes empty drops to PENDING per R9.
- R7. Each flip authors callable Rust (request/response structs + facade) gated
  on a closure Paper Live Smoke, with the request struct serializing numeric
  fields as JSON numbers (`string_as_number`) where the baseline shows numbers,
  to avoid `IGW40011`.
- R8. Each new REST `{TR}_POLICY` const registers in **both** crosscheck lists:
  `crates/ls-core/tests/policy_index_crosscheck.rs` and
  `crates/ls-core/src/endpoint_policy.rs::slice_rest_policies_are_non_order_rest`.

**Yield & disposition**

- R9. Before building a TR's struct, raw-probe it (`make raw-probe`). Non-empty
  body → build + flip. HTTP/deserialize failure → classify DEFECT (drop) vs
  ENVIRONMENTAL (retry). Success + empty `00707` → record PENDING with a
  credential-free reason, no build.
- R10. Each `live_smoke_<tr>` asserts its out-block is non-empty *before*
  recording success; an empty `00707` must never record a flip.

## Key Flow

Flows are linear and identical per TR, so one pipeline covers the wave.

- F1. Per-TR flip pipeline
  - **Trigger:** A tracked closure-viable TR enters a flip batch.
  - **Steps:** raw-probe → if non-empty, author struct + policy + facade +
    offline tests → register policy in both crosscheck lists → run closure
    smoke (fire the typed smoke before registrations, since crosscheck lists are
    test-only) → assert non-empty → flip `implemented: true`, bump docgen
    `banner_trs` + `reference.len()`.
  - **Outcome:** Implemented on a non-empty closure smoke, or PENDING on empty
    `00707` / DEFECT-classified failure.
- F2. Wave sequence
  - **Steps:** R0 pilot probe (~8 TRs under closure) → if clean rate ≥ ~40%, PR 1
    tracks the closure-viable pool → stacked PR per flip batch (A, then B, then
    C) → each PR runs the full gate (`make docs`, `cargo test`,
    `cargo test -p ls-core`, `make docs-check`) green before the next stacks.

## Acceptance Examples

- AE1. Closure smoke returns non-empty body
  - **Covers R7, R10, F1.** Given a tracked chart TR (e.g. t8417), when its
    closure smoke returns a populated out-block, then it flips `implemented:
    true` and bumps `reference.len()` by one.
- AE2. Closure smoke returns empty `00707`
  - **Covers R9, R10.** Given a tracked static read whose board is empty under
    closure, when its smoke returns success + empty `00707`, then it is recorded
    PENDING with a credential-free reason and does not flip; `reference.len()` is
    unchanged.
- AE3. Heuristic misclassification caught at probe
  - **Covers R2, R9.** Given a TR the sweep marked closure-viable that a probe
    shows requires an open session, when it returns empty under closure, then it
    is reclassified window-gated and left Tracked — not forced to a flip.
- AE4. Numeric request field
  - **Covers R7.** Given a request struct whose baseline shows a numeric field,
    when it serializes that field as a JSON string, then the gateway returns
    `IGW40011`; serializing via `string_as_number` is required before the smoke
    can pass.

## Scope Boundaries

**Deferred for later (an open KRX window or a producer unblocks them)**

- The ~21 window-gated reads (intraday 시간대별체결, 호가, 현재가 quotes,
  체결강도) — wait for an open window.
- t2106 (선물/옵션현재가시세메모) and t1964 (ELW전광판) — already Tracked,
  need an open window for non-empty data.
- t1852/t1856 (sFileData blob input) and t3102 (sNewsno sourced only from the
  realtime NWS WebSocket) — input-blocked; no window helps until a producer
  exists.

**Out of scope**

- The 12 paper_incompatible reads — overseas (g3101–g3190), night/krx_extended
  (t8455–t8463, CCENQ10100/CCENQ90200), account 잔고/평가. These never flip on
  paper, open or closed.
- t1860 (서버저장조건 실시간검색) — realtime-control subscription, HELD, not a
  plain read.
- Recommended promotion — a separate `promote-tr` pass under ADR 0008; this wave
  stops at Implemented.

## Dependencies / Assumptions

- The `track-tr` and `implement-tr` recipes and the closure-smoke pattern from
  PR #60 are reused as-is; the wave adds no new infrastructure.
- Baseline counts on main: docgen `reference.len()` == 116
  (`crates/ls-docgen/src/lib.rs:1038`); `maintained_tr_count` is computed
  dynamically from `shapes.len()` (`crates/ls-trackers/src/api_drift.rs:91`), so
  tracking ~60 raw bumps it without editing a hardcoded literal — but verify the
  test manifests in `crates/ls-trackers/src/types.rs` and the docgen
  `banner_trs` list at plan time.
- The ~60 closure-viable count is a title-heuristic ceiling; the firm number
  settles per-TR during R2 confirmation and R9 probing.
- Flip yield is uncapped and set by probing. No prior run covers this pool under
  closure, so the yield is genuinely unknown until the R0 pilot probe — do not
  assume a majority will flip.

## Outstanding Questions

**Deferred to planning**

- Whether any sweep-classified closure-viable TR carries a numeric request field
  needing `string_as_number` (per-TR, from each normalized baseline).
- Exemplar-trap check: grep `crates/ls-trackers` and `crates/ls-docgen` for any
  flip-batch TR used as a tracked-only illustration and repoint it before
  flipping.
- Exact enumeration of count-test sites that churn on track-all vs on flips.

### From 2026-06-26 review

These challenge the deliberately-maximal scope (track-all + uncapped flips) and
are recorded for a scope decision rather than applied as edits:

- Maximal breadth is not tied to a named consumer or demand ordering. The pain
  is supply-side ("offline-read surface narrower than the gateway supports");
  consider ordering flip batches by known/likely demand and stopping at the
  subset with an identified caller. (product-lens, P1)
- Track-all banks ~34 TRs (~60 tracked − ~26 flip-batch members) that no flip
  goal of *this* wave serves — surface-banking for future waves. Decide whether
  that banking is in scope or whether PR 1 should track only the flip-batch
  members. (scope-guardian, P1)
- Scaling to ~60 spends the per-flip build cost at ~30× the prior wave's scale
  while that wave's flip-cost/cadence decision is still open. Consider resolving
  the cost decision (or treating batch A as the data point that resolves it)
  before committing to batches B and C. (product-lens, P1)

## Sources

- `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json` — raw
  capture; source of the ~93 unimplemented reads and their Korean titles.
- `metadata/trs/` + `metadata/tr-index.yaml` — current tier of every TR;
  confirms t8411 Tracked, t1305/t1310/t1514 Implemented siblings, and the 14
  named candidates raw.
- `docs/plans/2026-06-26-003-feat-closed-window-flip-wave-plan.md` +
  `docs/brainstorms/2026-06-26-krx-closed-flip-wave-requirements.md` — the
  proven closure-flip pattern and smoke state machine this wave scales.
- `.agents/skills/track-tr/SKILL.md`, `.agents/skills/implement-tr/SKILL.md` —
  the recipes reused for tracking and flipping.
- `crates/ls-core/tests/policy_index_crosscheck.rs:70` +
  `crates/ls-core/src/endpoint_policy.rs:2136` — the two REST-policy crosscheck
  lists every flip must register in.
