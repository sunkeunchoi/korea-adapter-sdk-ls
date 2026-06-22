---
date: 2026-06-22
topic: discovery-screening-implemented-expansion
---

# Saved-Condition Screening Implemented Expansion Wave

## Summary

Promote 7 read-only stock TRs from **Tracked TR** to **Implemented TR** as a single **saved-condition screening** capability: 5 core TRs (the target) plus 2 session-gated ranking screens (variable). The core is the `t1866 → t1859/t1860` server-saved-condition `query_index` spine plus the `t1852`/`t1856` file-saved screening screens; `t1481`/`t1482` (after-hours ranking) join only if their session facet resolves. The wave is variable-size and block-and-drop, reusing the frozen `implement-tr` recipe plus a one-time chained-smoke extension for the spine; each TR ends Implemented-not-Recommended. Two smoke preconditions are named up front (a seeded server-saved condition for the spine, a representative screening-condition file for `t1852`/`t1856`), because the core's smoke-feasibility depends on them.

---

## Problem Frame

After the consumer-bound wave the maintained surface tracks 44 TRs with 18 callable, leaving ~26 tracked-only. That wave deliberately held back the consumer-less read-only TRs — "stay tracked-only until a real caller or a drift incident pulls them in" — because promoting tracked-but-uncalled TRs ships callable surface that still needs maintenance with no one to consume it.

That objection is valid against "implement these because they are tracked," and it is not answered by drift-coverage completeness or recipe amortization — those are engineering benefits, not callers. It is answered only when each promoted TR composes a real capability. The saved-condition screening surface is that capability, in two execution forms: the **server-saved** spine, where `t1866` lists server-saved screening conditions and returns a `query_index`, `t1859` executes a condition search against that index, and `t1860` attaches a realtime search to it; and the **file-saved** screens `t1852`/`t1856`, which run a caller-supplied saved screening-condition set. `t1481`/`t1482` rank the resulting instrument set (after-hours movers). Together they are one screening workflow, not standing drift surface.

A larger tracked-only pool (~25 candidates) was considered and tightened against this capability. A TR is excluded when it produces an identifier no wave member consumes, or emits analytics. That rules out `t3341` (financial ranking), the ELW/underlying discovery cluster, and `t1826` (Q-click search-list — it produces a `search_cd` consumed only by the deferred `t1825`). Those are useful but are different capability stories and move to their own roadmap waves rather than entering here on topical adjacency.

---

## Key Decisions

- **Named capability over coverage.** The wave is justified by the saved-condition screening use case, not by completing drift coverage or amortizing the recipe. Every member must compose that capability (see the membership rule), which is what clears the predecessor's consumer-less hold.

- **Membership rule: composes the screening surface.** A TR is in only if it (a) is on the server-saved spine (`t1866` produces the `query_index`; `t1859`/`t1860` consume it), (b) is a file-saved screening screen running a caller-supplied saved screening-condition set (`t1852`/`t1856`), or (c) ranks the screened instrument set (`t1481`/`t1482`, behind the session gate). A TR that only produces an identifier no wave member consumes, or that emits analytics, is out — which excludes `t3341`, the ELW/underlying cluster, and `t1826` (its `search_cd` consumer `t1825` is deferred).

- **7, not the larger pool.** Of the ~25 tracked-only candidates considered, this wave promotes 7 by the strict membership rule. The rule drops `t3341` and the ELW/underlying cluster (`t1988`/`t9905`/`t9907`/`t8431`/`t9942`) to analytics/discovery roadmap waves, and `t1826` to the caller-input wave alongside its consumer `t1825`. TRs deferred for *other* blockers (caller-input producers, `t8430`'s array-shape issue) are separate from the rule-driven drops — see Scope Boundaries.

- **5 core + 2 session-gated.** The 5 core TRs (spine + file-saved screens) are the target. They are subject to the two smoke preconditions (R6, R7): a spine with no seeded condition, or a file-saved screen with no sourced condition file, ships pending rather than implemented. `t1481`/`t1482` are variable: they promote only if the session gate resolves, and may drop or stay pending as `session-unresolved` without failing the wave.

- **`t1481`/`t1482` enter behind a hard session gate.** They rank after-hours movers, but the wave attempts the `krx_extended` vs `krx_regular` session-semantics work the predecessor deferred. The gate is evidentiary, not "returns data": an off-session smoke that merely deserializes does not resolve the facet. No SDK/core response field carries session phase, and the predecessor's smokes on the same call-auction family (`t1489`/`t1492`) ran off-session and could not resolve it — so absent an in-session live-run window, `session-unresolved` is the expected default, not a failure.

- **Variable-size, block-and-drop, inherited from the consumer-bound wave.** "Done" is the capability proven and each of the 7 in a decided end state — implemented-with-passing-smoke, or dropped/pending with a recorded reason. A failed TR-level smoke isolated to that TR drops it to tracked-only; an environmental or precondition failure keeps it a candidate but does not flip `support.implemented` without a confirming retry.

- **Type provisionality is already retired; session and caller-input are not.** The field-type re-pin rewrote the affected baselines from the clean fetch (`t1852`/`t1860` carried no affected field types and were unchanged), so the HTTP-500-seeded `type` caveat the predecessor carried does not apply here. The provisional facets this wave confirms or retires are session (`venue_session`) and caller-supplied identifiers, plus modeling the cross-TR discovery edge.

---

## Requirements

**Wave membership**

- R1. The wave promotes exactly these 7 TRs, grouped by role in the screening capability:
  - *Server-saved spine (core):* `t1866` (producer — server-saved condition list, yields `query_index`), `t1859` (consumer — condition search), `t1860` (consumer — realtime condition search).
  - *File-saved screening screens (core):* `t1852`, `t1856`.
  - *After-hours ranking screens (session-gated, variable):* `t1481`, `t1482` (see R10).
- R2. Each promoted TR satisfies the membership rule (Key Decisions): it is on the spine, runs a file-saved screening set, or ranks the screened instrument set. A TR that produces an identifier no wave member consumes, or emits analytics, is not promoted here.

**Per-TR promotion (the recipe applied)**

- R3. Each promoted TR gains callable Rust SDK behavior via the frozen `implement-tr` recipe: request struct, response struct, a public SDK method, dependency-class registration, and a per-TR paper-smoke harness. The per-TR path is not modified — this wave exercises the existing recipe at larger scale — but the spine's chained smoke (R8) is a net-new harness extension this wave authors on top of it, so the wave is not a pure recipe replay.
- R4. The promotion sets `support.implemented: true` and leaves `support.recommended: false`, creating no recommendation block and no evidence-freshness record. Each promoted TR gets a reference page carrying the "Implemented, not yet recommended" banner.
- R5. The Implemented gate per TR: the request constructs through the public SDK path, a paper LS call returns a recognized success `rsp_cd` with a non-empty result, and the response deserializes into the hand-written type. An empty result set (e.g. `00707`) confirms callability but not shape — that TR is recorded callable-but-shape-unconfirmed (pending), not flipped. Any committed smoke record passes the `promote-tr` credential-freedom check first.

**Smoke preconditions**

- R6. The spine's chained smoke (R8) requires `t1866` to return a non-empty `query_index`, which requires at least one server-saved screening condition to exist on the paper account. The wave seeds one such condition as a precondition; if none can be created on a paper account, the spine is recorded `spine-input-unavailable` and all three spine TRs ship pending without failing the wave. The "core implements" expectation is conditional on this precondition holding.
- R7. `t1852` and `t1856` each require a caller-supplied screening-condition file (`sFileData`, a ~26.8 KB serialized blob) as a required request input — the provisionality ledger's `caller_supplied_identifiers: []` classification for both is incorrect and is fixed to record this input. The smoke depends on sourcing a representative `sFileData`; if none can be produced in-window, each is recorded `input-unresolved` (pending) under block-and-drop rather than assumed smokeable.

**The discovery spine**

- R8. The wave models the cross-TR discovery edge `query_index ← t1866OutBlock1.query_index` for `t1859` and `t1860` — currently recorded in the ledger as an unmodeled discovery dependency. The `t1859`/`t1860` gate is satisfied by a chained smoke: a live `t1866` call supplies the `query_index` their requests consume. They have no standalone smoke input.
- R9. The discovery edge is retired from the provisionality ledger only when the chained smoke confirms `t1866`'s `query_index` is accepted by `t1859`/`t1860`; until then it stays flagged.

**Session gate**

- R10. `t1481` and `t1482` promote only if live behavior resolves their `venue_session` facet (`krx_extended` vs `krx_regular`). A smoke that returns data off-session without resolving the facet does not satisfy the gate. No response field carries session phase, so resolution requires running the smoke inside a live after-hours window and diffing against a regular-session run; absent that, `session-unresolved` is the expected outcome. If the session stays unresolved in the wave window, each is recorded `session-unresolved` in the close-out and stays tracked-only; this does not fail the wave.

**Provisionality**

- R11. Provisional facets a paper call genuinely confirms are retired from `metadata/PROVISIONALITY-LEDGER.md` or corrected before promotion — limited to session (`venue_session`) and caller-supplied identifiers actually accepted. Every TR that reaches implemented records a `venue_session` disposition: confirmed against live behavior for the session-gated `t1481`/`t1482`, or explicitly annotated unconfirmable-by-smoke for the non-session core TRs — so no TR flips to implemented while its ledger `venue_session` row silently stays live with its "re-verify before implementation" instruction. TRs that stay tracked-only record their final status (`session-unresolved`, `input-unresolved`, `spine-input-unavailable`) in the close-out. Field-level `type` retirement is not in scope: it was already handled by the clean-fetch re-pin.

**Wave outcome and gates**

- R12. The wave completes when each of the 7 ends in a decided state — implemented-with-passing-smoke, or dropped/pending with a recorded reason (TR-defect, environmental-pending, input-unresolved, spine-input-unavailable, or session-unresolved) in a wave close-out section of `metadata/PROVISIONALITY-LEDGER.md`. The core 5 are the target subject to the R6/R7 preconditions; the 2 session-gated may end pending without failing the wave.
- R13. Count-bearing artifacts pinned to the implemented-TR set move to match the number actually promoted: from 18 toward 23 (the core 5) or 25 (if both session-gated promote), adjusted down for any dropped/pending TR. The docgen reference-page count test currently asserts a hard `reference.len() == 19` plus a 12-element `banner_trs` array — both must be updated (bump the count, add each promoted TR code). The tracked-TR-count test is unaffected (no new tracked TRs). The metadata validator stays green (support flags, index/per-TR consistency).
- R14. Recommended-tier artifacts stay unchanged: no SDK Reference recommendation claim for any of the 7, and `metadata/EVIDENCE-FRESHNESS.md` stays at six Recommended TRs.

---

## Acceptance Examples

- AE1. **Covers R5, R7, R12.** A file-saved screen (`t1852`/`t1856`) with a sourced representative `sFileData` constructs through the public SDK path, a paper call returns a success `rsp_cd` with a non-empty result, and the response deserializes → `support.implemented` flips true; it gets a reference page with the not-recommended banner. If no `sFileData` can be sourced → recorded `input-unresolved` (pending), not flipped.
- AE2. **Covers R6, R8, R9.** With a seeded server-saved condition, a live `t1866` call returns a `query_index`; that value is fed into `t1859`'s request, which constructs, sends, and deserializes → the discovery edge retires from the ledger and `t1859` flips implemented. `t1859`/`t1860` are never smoked with a fabricated index.
- AE3. **Covers R6, R12.** No server-saved condition can be created on the paper account, so `t1866` returns `00707` with no `query_index` → the spine is recorded `spine-input-unavailable`; `t1866`/`t1859`/`t1860` all ship pending and the wave still completes on its other TRs.
- AE4. **Covers R10, R12.** A `t1481` paper smoke returns data but does not resolve `krx_extended` vs `krx_regular` → `t1481` is recorded `session-unresolved` and stays tracked-only; the wave still completes with the core decided.
- AE5. **Covers R5, R12.** A TR's smoke fails, isolated to that TR (other smokes clean, raw HTTP for it also fails) → it stays tracked-only with a recorded reason. The same failure reproducing across multiple TRs → classified environmental; the TR stays a candidate, not flipped, and ships pending if recovery never lands in-window.

---

## Scope Boundaries

**Deferred for later (roadmap waves)**

- `t1826` (Q-click ThinQ-smart search list) and its consumer `t1825` — `t1826` produces a `search_cd` that only `t1825` consumes; the producer→consumer pair belongs together in a later caller-input wave, not split across waves.
- ELW / underlying instrument-universe discovery cluster (`t1988`, `t9905`, `t9907`, `t8431`, `t9942`) — a different instrument universe with no in-wave consumer; its own discovery wave.
- Financial ranking `t3341` and the investor/program-trading aggregates (`t1601`, `t1615`, `t1664`, `t1640`, `t1662`) — analytics, not screening; their own wave(s).
- Other caller-input TRs with no in-wave producer (`t1958`, `t1964`, `t3102`, `t3320`) — promotable only once reliable producer inputs are defined for each.
- `t8430` — promotion gated on its unresolved upstream array-shape blocker.

**Outside this wave's identity**

- `CSPAT00601` and any order-safety surface — out until the full order-safety package is the target work item.
- Focused Evidence and Recommended promotion for any of the 7, and any change to `metadata/EVIDENCE-FRESHNESS.md`.
- Authoring or revising the `tracked → implemented` recipe core — it is frozen; this wave consumes the per-TR path unchanged, adding only the spine's chained-smoke harness (R8).

---

## Roadmap (captured, not in scope)

These draw from the same tracked-only pool but are distinct capability stories, each its own wave:

- **ThinQ-smart / caller-input wave** — `t1826` (produces `search_cd`) with its consumer `t1825`, plus `t1958`, `t1964`, `t3102`, `t3320` once each has a named producer for its required inputs. This wave's spine becomes `t1826`'s confirmed producer for `t1825`.
- **ELW / underlying discovery wave** — `t1988`, `t9905`, `t9907`, `t8431`, `t9942`. Note: `t1988` and `t9905` share the display name 기초자산리스트조회 and both carry `instrument_domain: stock`, so they are not distinguished by domain — confirm they are distinct lookups (request shape / output block) or collapse to one when this wave is scoped.
- **Financial-ranking / investor-aggregate analytics wave** — `t3341` (financial ranking) plus the investor/program-trading aggregates `t1601`, `t1615`, `t1664`, `t1640`, `t1662`.

---

## Dependencies / Assumptions

- All 7 are currently tracked-only with committed metadata and normalized baselines, re-pinned from the clean field-type fetch where field types were affected (`t1852`/`t1860` carried no affected types and were unchanged), so their structs derive from clean-pinned shapes rather than the HTTP-500-seeded snapshot.
- The `implement-tr` recipe exists and is frozen; it is the prerequisite for R3 and its per-TR path is not modified here. The spine's chained smoke (R8) is the one net-new harness mechanic the wave authors on top of it.
- The `t1866 → t1859/t1860` discovery edge is recorded in `metadata/PROVISIONALITY-LEDGER.md` as a documented but unmodeled cross-TR dependency; R8 models it for the first time, which is a new mechanic relative to the predecessor wave's independent-TR smokes.
- **Spine seeding (R6):** whether a paper account can create a server-saved screening condition determines if the spine is smokeable at all. If the saved-condition store is read-only on paper, the spine ships `spine-input-unavailable` regardless of TR correctness.
- **`sFileData` sourcing (R7):** `t1852`/`t1856` need a representative ~26.8 KB screening-condition file. If the LS ThinQ desktop application is the only generator and no blob can be reconstructed from the spec or a prior capture, both end `input-unresolved`.
- The 7 are assumed paper-callable read-only stock TRs; a TR that turns out non-paper-callable or precondition-blocked surfaces through R5/R6/R7/R12 block-and-drop rather than being assumed in.
- The interim consumer of an Implemented-not-Recommended member is the saved-condition screening workflow itself: the spine and file-saved screens are two execution forms of one screening surface, and the ranking screens surface the after-hours movers a screening user consults next (workflow adjacency, not data consumption). The banner marks them callable-and-composable but not yet evidence-backed for a user-facing recommendation.

---

## Outstanding Questions

**Resolve before planning**

- Can a paper account create a server-saved screening condition (R6) and can a representative `sFileData` blob be sourced for `t1852`/`t1856` (R7)? These two answers determine how much of the core 5 is actually smoke-feasible — if both are no, the wave reduces to the session-gated pair and should be re-scoped before planning.

**Deferred to planning**

- `owner_class` (dependency class) assignment per TR for the 7 — a hard-accurate facet driving index routing and the validator cross-check. Scope it as the first bounded unit: a first-pass assignment from each TR's `instrument_domain`/`self_paginated` signals, confirmed or corrected during the work.
- The concrete artifact for R8's modeled discovery edge (per-TR `dependencies` block, a new schema field, or an edge registry) — currently the ledger records it only as prose.
- Whether the 7 land as one PR or as clustered batches (spine, file-saved screens, then the session-gated pair).
- Whether to sequence an in-wave recipe-stability checkpoint (prove one TR end-to-end before the rest) — inherited concern from the consumer-bound wave, though the recipe is now frozen, which lowers the risk.

---

## Sources / Research

- Support model and three-boolean schema: `crates/ls-metadata/src/schema.rs`; vocabulary (Tracked / Implemented / Recommended TR, Facet Metadata) in `CONTEXT.md`.
- Frozen `tracked → implemented` recipe: `.agents/skills/implement-tr/`.
- Existing implemented-TR mechanics (request/response structs, public method, registration): `crates/ls-sdk/src/`.
- Cross-TR discovery edge and remaining provisional facets for the 7: `metadata/PROVISIONALITY-LEDGER.md` (`query_index ← t1866OutBlock1.query_index`; per-TR `venue_session` and caller-input rows). The `t1852`/`t1856` `sFileData` input is mis-recorded there as `caller_supplied_identifiers: []` (R7).
- Field-type re-pin that rewrote the affected baselines from the clean fetch: commit `1d7215f`.
- Docgen reference-page count test — currently asserts a hard `reference.len() == 19` plus a 12-element `banner_trs` array, both of which R13 must update: `crates/ls-docgen/src/lib.rs` (`reference_covers_implemented_with_banner_and_omits_unimplemented`).
- Recommended-count statement (six Recommended TRs): `metadata/EVIDENCE-FRESHNESS.md`.
- Predecessor waves: `docs/brainstorms/2026-06-21-consumer-bound-implemented-expansion-requirements.md`, `docs/brainstorms/2026-06-21-bulk-tracked-only-tr-expansion-requirements.md`.
- `t8430` array-shape blocker: `docs/brainstorms/2026-06-15-sdk-first-slice-decisions-requirements.md`.
