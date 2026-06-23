---
date: 2026-06-23
topic: capability-closed-tr-expansion-waves
---

# Capability-Closed Implemented Expansion Waves

## Summary

Promote tracked-only read-only TRs to **Implemented** across three serial PRs, each bounded by one named market-data capability: an **ELW universe/instrument surface** (Wave 1, up to 7 TRs), a **market-flow analytics surface** (Wave 2, up to 6 TRs), and **ThinQ Q-click search** (Wave 3, the t1826→t1825 pair). Each TR ends Implemented-not-Recommended, gated on a passing Paper Live Smoke through the frozen `implement-tr` recipe. No Focused Evidence, no Recommended promotion.

---

## Problem Frame

The saved-condition screening wave (2026-06-22) held back exactly these TRs as consumer-less — "stay tracked-only until a real caller or a drift incident pulls them in" — and set the bar that a promotion is justified only when each TR composes a named capability, not by completing drift coverage or amortizing the recipe. That hold still stands.

These three waves clear it not by relaxing the bar but by meeting it: each is a coherent, named user-facing capability. Wave 1 is the ELW surface — underlying-asset and expiry-month discovery, the ELW symbol/master lists, and the board and comparison screens that read against them. Wave 2 is a market-flow analytics surface — investor-type and program-trading aggregates plus financial ranking. Wave 3 is the ThinQ Q-click smart search, where `t1826` produces the `search_cd` that `t1825` consumes. Promoting by capability is what distinguishes this from "implement these because they are tracked"; coverage completion and recipe reuse are benefits, not the reason.

---

## Key Decisions

- **Capability-Closed Implemented Expansion Wave.** A wave whose membership is bounded by one named user-facing market-data capability, not by backlog coverage. This is the unit of the campaign and the canonical name for it. The predecessor's consumer-less objection is answered per wave, by the capability each composes.

- **Serial campaign, three PRs — not a mixed mega-PR.** Each capability ships and closes out independently, matching every prior wave (consumer-bound, bulk-tracked, saved-condition screening). The waves have no cross-wave ordering dependency; sequence by readiness.

- **t1958/t1964 are ELW members, not deferred caller-input TRs.** The predecessor roadmap filed them with the producer-blocked `t3102`/`t3320`. But their inputs are caller-suppliable instrument codes (`shcode1`/`shcode2`; `item`/`issuercd`) — a smoke can supply a representative ELW ticker without an in-wave producer, which is what clears the producer-blocked classification. A producer such as `t8431`/`t9942` can also emit these codes; whether to source them that way is a deferred smoke-design choice (see Open Questions), not a membership blocker. They belong in Wave 1.

- **No known session-gate exposure.** No member is a call-auction/after-hours screen like the predecessor's `t1481`/`t1482`, so `krx_regular` is the working assumption and no in-session live window is expected. This is not pre-settled, though: the ledger still marks each member's `venue_session` as best-effort ("re-verify before implementation"), so each close-out records a per-member `venue_session` disposition (R12) rather than asserting the facet resolved up front.

- **`t1988` and `t9905` are distinct, not a duplicate.** They share the display name 기초자산리스트조회 but carry different `source_spec_hash` values and different request blocks: `t9905` takes a single `dummy` input (full list), while `t1988` takes a 15-field filter InBlock (`mkt_gb`, `chk_price`, `from_price`/`to_price`, `chk_vol`, … — a price/volume-filtered list). Both stay in Wave 1; they are not collapsed.

- **Drift-readiness is a benefit, not the justification.** Promoting makes future API drift detectable against callable, exercised structs rather than tracked metadata alone — a supporting effect, explicitly not the reason a TR enters a wave.

---

## Requirements

**Campaign structure**

- R1. The expansion ships as three separate PRs, one per capability wave, run as a serial campaign rather than one mixed PR. There is no cross-wave dependency; each wave completes with its own close-out.
- R2. Each wave is a Capability-Closed Implemented Expansion Wave: membership is bounded by one named market-data capability, not by coverage or recipe amortization. A TR is in a wave only if it composes that wave's capability.

**Wave membership**

- R3. Wave 1 (ELW universe & instrument surface) promotes up to 7 TRs: `t1988` (filtered underlying-asset list), `t9905` (full underlying-asset list), `t9907` (expiry month), `t8431` (ELW symbol), `t9942` (ELW master), `t1964` (ELW board), `t1958` (ELW comparison). `t1988` and `t9905` share the display name 기초자산리스트조회 but are distinct lookups (different `source_spec_hash` and request blocks — see Key Decisions); both stay in.
- R4. Wave 2 (market-flow analytics surface) promotes up to 6 TRs: `t1601`, `t1615` (investor aggregates), `t1640`, `t1662` (program-trading aggregates), `t1664` (investor chart), `t3341` (financial ranking).
- R5. Wave 3 (ThinQ Q-click search) promotes up to 2 TRs: `t1826` (producer — search list, yields `search_cd`) and `t1825` (consumer — requires `search_cd`).
- R6. `t1958` and `t1964` enter Wave 1 as ELW instrument TRs. Their caller-supplied identifiers are recorded as caller inputs (an ELW ticker), correcting any classification that treats them as producer-blocked.

**Per-TR promotion (frozen recipe)**

- R7. Each promoted TR gains callable Rust via the frozen `implement-tr` recipe: request struct, response struct, public SDK method, dependency-class registration, and a per-TR paper-smoke harness. The recipe core is not modified.
- R8. Promotion sets `support.implemented: true`, leaves `support.recommended: false`, writes no recommendation block and no evidence record; each member gets a reference page carrying the "Implemented, not yet recommended" banner.
- R9. The Implemented gate per TR: the request constructs through the public SDK path, a paper LS call returns a recognized success `rsp_cd` with a non-empty result, and the response deserializes into the hand-written type. An empty result (e.g. `00707`) confirms callability but not shape — that TR ships pending, not flipped.

**Smoke strategy**

- R10. Wave 1 and Wave 2 members smoke as standalone reads, each supplying its full required InBlock field set with representative values — not just the headline identifier. `t1958` needs `shcode1`/`shcode2`; `t1964` needs all 11 required fields (`item`, `issuercd`, `lastmonth`, `elwopt`, `atmgubun`, `elwtype`, `settletype`, `elwexecgubun`, `fromrat`, `torat`, `volume`), not only `item`/`issuercd`; `t1988` needs its full price/volume filter InBlock. Where a member's representative values cannot be pinned in-window, it ships `input-unresolved` (pending) under R14. `t3341` (paginated, `self_continuation_fields: [idx]`) smokes under the recipe's paginated path, not the standalone-read path; a non-empty first-page success satisfies the gate. No chained producer smoke for Wave 1 or Wave 2.
- R11. Wave 3 uses a chained smoke: a live `t1826` call (supplying a representative `search_gb`, its required InBlock field — the mode determines which search list, and thus which `search_cd`, is returned) supplies the `search_cd` that `t1825`'s request consumes (alongside a representative `gubun`, t1825's other required InBlock field), mirroring the predecessor's `t1866 → t1859` spine. `t1825` is never smoked with a fabricated `search_cd`. The discovery edge `search_cd ← t1826OutBlock.search_cd` does not yet exist in the ledger; Wave 3 first authors it as a §3 row (mirroring the existing `t1860` `query_index` edge), then retires it only when the chained smoke confirms acceptance. On confirmation, t1825's metadata `caller_supplied_identifiers: [search_cd]` is corrected to `[]` and its §2 ledger row retired alongside the §3 edge; if `t1826` returns empty, both rows are retained and t1825 ships pending.

**Provisionality & counts**

- R12. Each wave's close-out is a dedicated section in `metadata/PROVISIONALITY-LEDGER.md` listing every member's end state (implemented / pending / dropped) with a credential-free disposition. Provisional facets a paper call genuinely confirms are retired; pending and dropped TRs keep their ledger rows so none stays live with a stale "re-verify before implementation" instruction. Each implemented member records a `venue_session` disposition — confirmed against live behavior, or explicitly annotated unconfirmable-by-smoke — so no TR flips to implemented while its ledger `venue_session` row silently stays live (the predecessor's R11 discipline).
- R13. Each wave updates the docgen count-bearing test (`reference_covers_implemented_with_banner_and_omits_unimplemented`): bump `reference.len()` (currently 21) by the number actually promoted and add each newly-implemented TR to the `banner_trs` array (currently 14 entries). The Recommended-tier artifacts (six Recommended TRs, `metadata/EVIDENCE-FRESHNESS.md`) and the tracked-TR count stay untouched.

**Wave outcome**

- R14. Each wave is block-and-drop: done means the capability is proven and every member is in a decided end state. A TR-isolated smoke failure drops that TR to tracked-only with a recorded reason; an empty result or unresolved input ships pending; an environmental failure keeps candidacy without flipping `support.implemented`. The member counts (7 / 6 / 2) are ceilings, not guarantees. A wave merits its own PR only if at least one member flips Implemented with a passing smoke that exercises the named capability; if zero members flip, the wave re-scopes rather than shipping an empty PR.

---

## Acceptance Examples

- AE1. **Covers R9, R10, R14.** A Wave 1/2 standalone read constructs through the public SDK path; a paper call returns a success `rsp_cd` with a non-empty result that deserializes → `support.implemented` flips true and it gets the not-recommended banner. An empty `00707` → recorded callable-but-shape-unconfirmed (pending), not flipped.
- AE2. **Covers R11.** A live `t1826` call returns a `search_cd`; that value feeds `t1825`'s request, which constructs, sends, and deserializes → the `search_cd` discovery edge retires from the ledger and `t1825` flips implemented.
- AE3. **Covers R6, R10.** `t1958` smokes with a representative caller-supplied ELW ticker for `shcode1`/`shcode2`; a paper call returns a non-empty success → it flips implemented as a Wave 1 ELW member, never treated as producer-blocked.
- AE4. **Covers R14.** A member's smoke fails, isolated to that TR (other smokes clean, its raw HTTP also fails) → it stays tracked-only with a recorded reason; the capability is still proven on the remaining members and the wave completes.

---

## Scope Boundaries

**Deferred for later (still tracked-only, blocker-gated)**

- `t1852` / `t1856` — require a caller-supplied `sFileData` screening-condition blob; promotable once a representative file can be sourced.
- `t1481` / `t1482` — promotable only when their `venue_session` facet resolves under an in-session live window.
- `t3102` / `t3320` — caller-input TRs with no defined producer for their required inputs.
- `t1860` (realtime condition search) and `t8430` (upstream array-shape blocker) — picked up by a future realtime wave and an upstream-clarification fix, respectively.

**Outside this campaign's identity**

- Recommended tier, Focused Evidence, and any `metadata/EVIDENCE-FRESHNESS.md` edit for any member — these waves are Implemented-only.
- `CSPAT00601` and any order-capable surface — out until the order-safety package is the target work item.
- Revising the `implement-tr` recipe core — it is frozen; Wave 3's chained smoke is a harness extension on top of it, not a recipe change.

---

## Dependencies / Assumptions

- All 15 members are currently tracked-only with committed metadata and normalized baselines, with structs deriving from clean-pinned field-type shapes (post the field-type re-pin), not the HTTP-500-seeded snapshot.
- The `implement-tr` recipe exists and is frozen; it is the prerequisite for R7. The current implemented count is 20; the docgen test asserts `reference.len() == 21` with a 14-element `banner_trs` array.
- Wave 1 smoke feasibility assumes a representative ELW ticker can be sourced for `t1958`/`t1964` (and for any other member that takes an instrument code), likely from an in-wave discovery read (`t8431`/`t9942`) or a known ELW code. If none can be sourced in-window, the affected members ship `input-unresolved` (pending).
- Wave 3 assumes a live `t1826` returns a non-empty `search_cd`. If `t1826` returns empty, the pair ships pending without failing the wave.
- All members read against `krx_regular` and carry no `account_state` / `paper_incompatible` flags, so they are assumed paper-callable read-only stock TRs; any that turns out otherwise surfaces through R9/R14 block-and-drop rather than being assumed in.

---

## Outstanding Questions

**Deferred to planning**

- Whether `t1958`/`t1964` source their ELW codes from an in-wave producer read (modeling an optional `t8431`/`t9942` → `t1958`/`t1964` discovery edge) or from a caller-supplied representative ticker. The default is caller-supplied; the producer-edge is a planning refinement.
- The concrete artifact for Wave 3's modeled `search_cd` discovery edge (per-TR `dependencies` block vs. an edge registry) — currently the ledger records it only as prose.
- `owner_class` confirmation per member against the metadata validator (most are `standalone`; `t3341` is `paginated`).
- Whether each wave lands as a single PR or as clustered batches within the PR.

---

## Deferred / Open Questions

### From 2026-06-23 review

- **t1958/t1964 — caller-supplied vs producer-chained inputs (premise).** Review found `t8431` (ELW 종목조회) emits the `shcode` that `t1958` consumes, and `t1964` requires 11 input fields — so these inputs are producer-sourceable, not simply free caller-supplied codes. Decide whether `t1958`/`t1964` are smoked standalone with representative values (current R6/R10) or modeled as discovery-chained members (`t8431`/`t9942` → `t1958`/`t1964`) like Wave 3's `t1826`→`t1825`. This bears on the doc's override of the predecessor's "caller-input TRs need a producer" classification.
- **Capability framing vs the consumer-less hold for Waves 1 & 2 (premise).** The capability justification was a deliberate decision, but review notes three pressures: (a) the predecessor's membership rule excluded `t3341` by name for "emitting analytics," yet Wave 2 is "the analytics surface" including `t3341`; (b) Wave 1/2's named consumers (the board/comparison screens) are themselves promoted members, so the consumer test is satisfied circularly; (c) only Wave 3 has a genuine producer→consumer edge. Decide whether to hold the framing as-is, name an interim consumer per wave (as the predecessor did for the screening workflow), or ship Wave 3 first to test whether ELW/analytics callers materialize before committing Wave 1/2's maintenance cost.

---

## Sources / Research

- Support model and three-boolean schema: `crates/ls-metadata/src/schema.rs`; vocabulary (Tracked / Implemented / Recommended TR, capability terms) in `CONTEXT.md`.
- Frozen `tracked → implemented` recipe: `.agents/skills/implement-tr/`.
- Per-TR metadata for all 15 members: `metadata/trs/<code>.yaml` (all `support: {tracked: true, implemented: false, recommended: false}`, all `venue_session: krx_regular`; `t1958` `caller_supplied_identifiers: [shcode1, shcode2]`, `t1964` `[item, issuercd]`, `t1825` `[search_cd]`).
- Docgen count-bearing test (`reference.len() == 21`, 14-element `banner_trs`): `crates/ls-docgen/src/lib.rs`.
- Recommended-count (six Recommended TRs): `metadata/EVIDENCE-FRESHNESS.md`.
- Cross-TR discovery-edge precedent and the chained-smoke mechanic: `metadata/PROVISIONALITY-LEDGER.md` (`query_index ← t1866OutBlock1.query_index`).
- Predecessor wave and its roadmap (which named these three waves and misfiled `t1958`/`t1964`): `docs/brainstorms/2026-06-22-discovery-screening-implemented-expansion-requirements.md`. Earlier waves: `docs/brainstorms/2026-06-21-consumer-bound-implemented-expansion-requirements.md`, `docs/brainstorms/2026-06-21-bulk-tracked-only-tr-expansion-requirements.md`.
