---
date: 2026-06-26
topic: tracked-and-raw-flip-wave
---

# Tracked-and-Raw Flip Wave — Requirements

## Summary

A probe-first wave that flips TRs to Implemented. Phase 1 attempts the small set
of attemptable Tracked standalone/derivatives reads (`t8430`, `t2106`, `t3102`)
and dispositions each faithfully. Phase 2 runs a pre-track classification sweep
over the still-raw master/reference pool already enumerated in plan -004, then
tracks and implements whatever the sweep admits. The KRX trading window is **not**
the organizing constraint — see Problem Frame.

## Problem Frame

This wave began from the premise that an open market window is a scarce resource
unlocking session-dependent reads. Verification against `metadata/trs/*.yaml` and
prior plans showed that premise does not hold for the actual candidates:

- `t2106` is `instrument_domain: futures_options` — an **anytime** F/O price-memo
  read that self-sources its contract from `t8467`. It already ran in-window in
  plan -001 and stayed PENDING on an empty memo array; its emptiness is feed/data
  driven, not window driven.
- `t3102` is `owner_class: standalone`, `뉴스본문` (news body), keyed by
  `caller_supplied_identifiers: [sNewsno]`. Its gate is a valid news number with
  no defined producer — not the trading session.
- `t8430` is `주식종목조회` (stock-issue list), `date_sensitive: false` — a
  standalone read, not session-gated.

So there is no genuinely window-contingent flip set. The KRX window reopens every
trading day, so deferring any contingent leg costs a day, not the opportunity.

What is true: current `main` carries 126 maintained TRs — 108 Implemented (6 of
them also Recommended) plus 18 Tracked-but-unimplemented. Of the 18, 11 are
`paper_incompatible: true` (overseas `g31xx` + night-derivatives `t8455`/`t8460`/
`t8463` + `CCENQ10100`/`CCENQ90200`) and never flip on paper. That leaves 7
Tracked candidates, of which 3 are attemptable (`t8430`/`t2106`/`t3102`) and 4
are caveated (`t1852`/`t1856`/`t1860`/`t1964`). The Tracked tier is nearly
exhausted of easy wins; real volume lives in the still-raw master-leaning
`t8450`/`t8452`–`t8466` block that plan -004 already enumerated and that remains
raw on current `main` (only the illustrative subset `t9945`/`t3401`/`t4203`/
`t3202` was consumed by PR #54).

## Key Decisions

- **The window is not the constraint.** Flips here gate on a clean paper smoke
  returning non-empty data, not on trading hours. Sequencing is by readiness.

- **Phase 2 reuses plan -004, not fresh discovery.** This wave does not
  re-author master/reference discovery. It adopts plan -004's already-enumerated
  still-raw pool (`t8450`, `t8452`–`t8466`, excluding the PR #54 flip `t8451`) as
  the Phase 2 probe input. The overseas `t3518`/`t3521` and `01900` `MMDAQ91200`
  candidates from that plan stay excluded. This doc supersedes plan -004's wave
  framing; the pool and its probe procedure carry over.

- **Admission is decided by raw-shape classification, not probe non-emptiness.**
  A single probe shows only non-empty-now; the always-on session-independence
  signal is the out-block key shape (pure reference/master). The probe catches
  wire-health failures (`01900`, `IGW40011`); it does not prove session
  independence. Probe-admitted candidates may still re-PEND in-window and fall
  under the same disposition discipline as Phase 1.

- **Reliable cluster only.** Wider session categories (F&O `t2xxx` beyond the
  already-Tracked `t2106`, ETF/warrant/bond `t19xx`) and a maximal raw sweep
  stay out.

- **Disposition discipline over force-flipping.** A TR flips only on a clean
  smoke with non-empty data; everything else is recorded faithfully (PENDING /
  `paper_incompatible: true`).

- **Recommended promotion is out.** All Implemented TRs (including the order
  chain) stay Implemented; promotion is a separate pass.

## Requirements

**Phase 1 — attemptable Tracked reads (anytime)**

- R1. Attempt a clean paper smoke for `t8430` (standalone stock-issue list). Flip
  to Implemented on a non-empty smoke. Memory flags a possible upstream
  array-shape blocker while its metadata records `certification_path: none`;
  resolve the discrepancy during the smoke and, if genuinely blocked, record
  PENDING rather than flip.
- R2. Attempt `t2106` (anytime F/O price-memo, self-sources its contract via
  `t8467`). Flip only on a populated memo out-block (memo-row count > 0); on an
  empty memo array record PENDING, consistent with its prior disposition.
- R3. Attempt `t3102` (standalone news body) only when a valid `sNewsno` can be
  sourced. If no producer supplies one, leave Tracked with a recorded
  input-blocked PENDING.
- R4. Any Phase 1 read whose smoke returns empty `00707` stays Tracked (PENDING)
  and does not block the wave.

**Phase 2 — master/reference breadth (plan -004 pool)**

- R5. Run a pre-track probe and raw-shape classification sweep over plan -004's
  still-raw master-leaning pool (`t8450`, `t8452`–`t8466`, excluding `t8451`).
  The probe confirms wire-health and screens out `01900`/`IGW40011`; raw-shape
  classification of out-block keys decides admission.
- R6. Admit a candidate only when its out-block key shape is pure reference/master
  (non-emptiness independent of session or account). Probe non-emptiness alone is
  not sufficient — an admitted candidate may still re-PEND in-window under R4
  discipline.
- R7. Batch-track only admitted candidates raw→Tracked, then implement and smoke
  each; flip to Implemented on a clean paper smoke.
- R8. Gate the floor by sequence, in a single unit (admitted candidates), so the
  tracking tax is never paid for a token wave:
  - If the sweep admits fewer than 6, do **not** track. Surface the count to the
    operator and defer the survivors to a future wave (do not broaden into the
    categories excluded by R6); the wave ships Phase 1 dispositions alone.
  - If the sweep admits 6 or more, track-and-smoke them.
  - If post-smoke clean flips then fall below 6, ship the flips that passed and
    record the rest as PENDING — do not hold the whole cluster.

**Cross-cutting**

- R9. Exclude entirely the 11 `paper_incompatible: true` Tracked TRs and the
  caveated Tracked reads `t1852`, `t1856`, `t1860`, `t1964`.
- R10. Keep the gate green: regenerate docs, pass the workspace and `ls-core`
  metadata/policy cross-checks, and `make docs-check` before any commit.
- R11. Recommended status is untouched for every TR in this wave.

## Acceptance Examples

- AE1. **Covers R1.** Given `t8430`'s paper smoke returns a non-empty stock-issue
  list, then `t8430` flips Tracked→Implemented and registers in the count-assertion
  sites and both policy cross-check lists.
- AE2. **Covers R1.** Given `t8430`'s smoke returns the upstream array-shape error,
  then `t8430` stays Tracked with a recorded PENDING disposition and the wave
  proceeds.
- AE3. **Covers R2.** Given `t2106`'s smoke returns an empty memo array, then
  `t2106` stays Tracked (PENDING) — its emptiness is feed-driven, not window-driven.
- AE4. **Covers R3.** Given no producer supplies a valid `sNewsno`, then `t3102`
  stays Tracked with an input-blocked PENDING and is not attempted further.
- AE5. **Covers R5, R8.** Given the sweep admits fewer than 6 candidates, then no
  Phase 2 tracking happens and the wave ships only the Phase 1 dispositions.
- AE6. **Covers R6.** Given a candidate's probe returns non-empty but its out-block
  key shape is session-dependent, then it is not admitted on probe non-emptiness
  alone.
- AE7. **Covers R7, R8, R10.** Given the sweep admits 8 candidates and 6 smoke
  clean, then those 6 flip to Implemented, the other 2 record PENDING, and the full
  gate (docs, tests, cross-checks, docs-check) is green before commit.

## Scope Boundaries

**Deferred for later**
- Recommended promotion for any Implemented TR, including the order chain.
- The caveated Tracked reads `t1852`, `t1856`, `t1860` (realtime/saved-condition),
  and `t1964` (empty-board) — revisit when their blockers clear.

**Outside this wave's identity**
- Wider session categories — additional F&O `t2xxx`, ETF/warrant/bond `t19xx` —
  and a maximal sweep of the remaining raw pool. Phase 1's `t2106` is included
  only because it is already Tracked and attemptable.
- Overseas `t3518`/`t3521` and `01900` `MMDAQ91200` from plan -004's pool.
- The 11 `paper_incompatible: true` overseas/night TRs that return terminal
  `01900` regardless of window.

## Dependencies / Assumptions

- Paper credentials in `.env` are valid. Order-capability is **not** a
  precondition — every read in this wave is credential-safe and places no orders.
- The trading window is not load-bearing; smokes gate on non-empty responses.
- Phase 2's value depends on the sweep admitting ≥6 from plan -004's still-raw
  pool; the `≥6` floor is calibrated against that ~13-candidate block.

## Outstanding Questions

**Resolve before planning**
- After raw-shape classification, do ≥6 of plan -004's still-raw `t845x`/`t846x`
  candidates qualify as pure master/reference? If fewer, Phase 2 defers and the
  wave is Phase 1 only.

**Deferred to planning**
- Is `t8430` genuinely array-shape-blocked (memory) or clean (metadata records no
  blocker)? Resolve at smoke time.
- Can a valid `sNewsno` be sourced for `t3102` within paper, or is it permanently
  input-blocked absent a realtime WS producer?
- Exact count-assertion and policy cross-check registration sites per flipped TR
  (api_drift `maintained_tr_count`, `cli.rs` count literals, docgen
  `TRACKED_TRS`/`banner_trs`/`reference.len`, both policy cross-check lists).
- Smoke-target authoring: `t3102` and `t8430` need new `live-smoke-*` Makefile
  `.PHONY` targets + smoke-map rows; `t2106` already has one.

## Sources / Research

- `metadata/trs/*.yaml`, `metadata/tr-index.yaml` — live support tiers, owner_class,
  `instrument_domain`, `caller_supplied_identifiers`, `paper_incompatible` flags
  (verified on current `main`: 126 maintained = 108 Implemented (6 Recommended) +
  18 Tracked; 11 paper-incompatible; t8430/t2106/t3102 not session-gated; the
  `t845x`/`t846x` master block still raw).
- `docs/plans/2026-06-25-004-feat-domestic-stock-master-reference-breadth-plan.md`
  — the enumerated still-raw pool, probe + raw-shape-classification procedure,
  flip floor, t8424 mirror pattern, and count-assertion sites this wave reuses.
- `docs/plans/2026-06-25-001-feat-night-overseas-elw-implement-wave-plan.md` —
  t2106 anytime F/O, flips on a populated memo out-block; prior in-window PENDING.
- `docs/plans/2026-06-24-001-feat-readonly-rest-reach-wave-plan.md` — t3102/t8430
  classed as caller-input / unblocked standalone reads.
- `.agents/skills/track-tr/SKILL.md`, `.agents/skills/implement-tr/SKILL.md`,
  `.agents/skills/promote-tr/references/smoke-map.md`, `Makefile` — recipes,
  smoke targets, and rows this wave reuses.
