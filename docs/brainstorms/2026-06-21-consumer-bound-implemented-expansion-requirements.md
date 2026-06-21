---
date: 2026-06-21
topic: consumer-bound-implemented-expansion
---

# Consumer-Bound Implemented Expansion Wave

## Summary

Promote 11 consumer-bound read-only stock TRs from **Tracked TR** to **Implemented TR**, and in the same wave establish the reusable `tracked → implemented` recipe that does not exist yet. Each promoted TR becomes callable Rust SDK behavior, confirmed by a paper smoke, but stays non-recommended — no recorded evidence, no recommendation claim. The recipe is the durable deliverable; the 11 are its first real exercise.

---

## Problem Frame

After PR #27 the maintained surface tracks 44 TRs but only 7 are callable. The 36 read-only stock TRs landed as tracked-only: structural drift visibility without any callable behavior. The shared path every roadmap promotion needs — a `tracked → implemented` recipe — was deferred and still does not exist; the existing recipe (`.agents/skills/promote-tr/`) only covers `implemented → recommended`.

Until that path exists, no tracked-only TR can become callable and the candidate pool PR #27 seeded cannot be drawn down. Building the recipe in the abstract risks designing for cases that never arrive. This wave builds the path and proves it on the subset that has named callers in the PR #27 roadmap, so the recipe is exercised by real SDK-facing work the first time it runs.

---

## Key Decisions

- **Consumer-bound 11, not all 36.** Becoming Implemented means hand-written per-TR Rust (request struct, response struct, public SDK method, dependency-class registration) — non-uniform and risk-bearing per TR, unlike the uniform snapshot-derived tracked-only batch. Only ~13 of the 36 have a named callable destination; promoting the ~23 consumer-less TRs would ship callable-but-uncalled surface that still needs maintenance. The 11 are the predecessor roadmap's consumer-bound Items 2–4 (12 codes) minus `t8430` (blocked). `t1481`/`t1482` are read-only list/screen TRs from the same 36-TR batch that were considered for this wave but deferred separately (see Scope Boundaries) — they were never part of the Items 2–4 roadmap set.
- **The recipe is a co-equal deliverable, not a by-product.** The wave's durable output is a proven, repeatable `tracked → implemented` path. Coverage of the 11 is how the path is proven, not the point of the wave.
- **Paper smoke gates each TR but is not recorded as Focused Evidence.** This is the line between Implemented and Recommended: Implemented means a maintainer proved a representative paper call builds, sends, and deserializes; Recommended means recorded, fresh evidence justifies a user-facing recommendation. Recording stays a recommended-tier act.
- **Block-and-drop on smoke failure.** A failed TR-level paper smoke drops that TR back to tracked-only for this wave rather than letting it claim implemented with a caveat — which would reintroduce the "shape-only implemented" risk. A failure proven to reproduce *outside* that TR (environmental, credential, or gateway outage) is not the TR's defect: the TR stays a candidate but does **not** flip to implemented until a retry confirms the deserialize. No deserialize confirmation, no Implemented TR — the tier boundary holds even for environmental failures.
- **Variable-size wave by design.** Because of block-and-drop, "done" is the recipe proven plus each of the 11 in a decided end state — not a fixed count of 11 implemented. Coverage is an outcome, not a target.

---

## Requirements

**The conversion recipe**

- R1. A documented `tracked → implemented` recipe exists, repeatable per TR and distinct from the existing `implemented → recommended` recipe.
- R2. The recipe defines a per-TR Implemented gate: callable request/response types exist; the request constructs through the public SDK path; a paper LS call returns a recognized success `rsp_cd` and a **non-empty** result; the response deserializes into the hand-written response type. A successful deserialize of an empty result set (e.g. `rsp_cd` `00707`) confirms callability but not response shape — such a TR is callable-but-shape-unconfirmed and recorded pending, not flipped to implemented on that smoke alone.
- R3. The smoke result is recorded only as promotion notes or gate output, never as a Focused Evidence record.
- R3a. Any committed record of a smoke result (promotion notes, gate output, or a drop/pending reason) must first pass the same credential-freedom check the `promote-tr` recipe applies before recording a LIVE-SMOKE line: no OAuth token, appkey, secret, or account number may appear — only lengths, business `rsp_cd`, public tickers/dates/ports, and structural counts. The `tracked → implemented` recipe cites this check explicitly rather than treating it as implicit.
- R4. The recipe sets `support.implemented: true` and leaves `support.recommended: false`, creating no recommendation block and no evidence-freshness record.

**Per-TR promotion (applied to the 11)**

- R5. Each promoted TR gains callable Rust SDK behavior: request struct, response struct, a public method on the SDK facade, and dependency-class registration. A per-TR paper-smoke harness (a make target, a `live_smoke.rs` test function, and a `references/smoke-map.md` row) is built for the TR — none of the 11 has one today, and the recipe (R1) must produce it before R6's gate can run, mirroring how `promote-tr` treats a missing smoke target as a hard hold.
- R6. A failed TR-level paper smoke leaves the TR tracked-only. A failure counts as environmental only when proven to reproduce outside the TR — the raw-HTTP probe for that TR also fails, or other TRs' smokes fail the same way in the same window; a failure where raw HTTP for the TR succeeds but the SDK deserialize fails is a TR defect, never environmental. The environmental classification (raw-HTTP probe result, failing `rsp_cd`/HTTP status) is captured in the credential-safe drop record (R3a). An environmental failure keeps the TR a candidate but does not flip `support.implemented` until a retry confirms request construction, a non-empty success response (R2), and deserialization into the hand-written type; if recovery does not happen in the wave window, the wave ships without it and it stays tracked-only, recorded as pending.
- R7. Provisional facets touched by a TR's implementation are confirmed and retired from `metadata/PROVISIONALITY-LEDGER.md`, or corrected, before promotion — limited to facets a paper call genuinely confirms (caller-supplied identifiers actually accepted, venue/session observed). Field-level `type` retirement is **not** in this wave's scope: a successful deserialize passes on null, absent, or permissively-typed fields, so it does not confirm the HTTP-500-seeded types; `type` retirement stays assigned to the separate clean-fetch re-pin PR, as the ledger records.

**Wave outcome and gates**

- R8. The wave completes when the recipe (R1–R4) is proven and each of the 11 ends in a decided state: implemented-with-passing-smoke, or dropped back to tracked-only with a recorded reason. Drop/pending reasons live in a wave close-out record — a dedicated section in `metadata/PROVISIONALITY-LEDGER.md` listing each non-promoted TR with its failure classification: TR-defect, environmental-pending, or input-unresolved (a representative caller-supplied identifier could not be chosen, distinct from a genuine TR defect).
- R9. The change-scoped gate passes on the wave's promoted TRs, and the metadata validator stays green (support flags, index/per-TR consistency).
- R10. Count-bearing artifacts pinned to the implemented-TR set move to match the number actually promoted: the docgen reference-page count test moves from `reference.len() == 8` (index + 7) toward `reference.len() == 19` (index + 18) if all 11 pass, adjusted down for any dropped TR. The count-bearing test function name (`reference_covers_seven_implemented_with_banner_and_omits_unimplemented`, which embeds `seven`) and any assertion-message string embedding the count are updated to match, so the committed test carries no stale ordinal — following the precedent the predecessor batch set when it renamed the `eight`-embedding function. Each promoted TR gets a reference page carrying the "Implemented, not yet recommended" banner. The tracked-TR-count doc test is unaffected (no new tracked TRs).
- R11. Recommended-tier artifacts stay unchanged: no SDK Reference recommendation claim for any of the 11, and `metadata/EVIDENCE-FRESHNESS.md` stays at six Recommended TRs.

The 11 TRs: `t8436`, `t8425`, `t1531`, `t1537`, `t1403`, `t1441`, `t1452`, `t1463`, `t1466`, `t1489`, `t1492`.

---

## Acceptance Examples

- AE1. **Covers R2, R5, R6, R8.** A TR's request constructs through the public SDK path, a paper LS call returns a recognized success `rsp_cd` with a non-empty result, and the response deserializes into the hand-written type → `support.implemented` flips to true; it gets a reference page with the not-recommended banner. If the call instead returns an empty result set (`00707`), the TR is recorded callable-but-shape-unconfirmed (pending), not flipped.
- AE2. **Covers R6, R8.** A TR's paper smoke fails and the failure is isolated to that TR (other TRs smoke clean, raw HTTP for this TR also fails) → the TR stays tracked-only with a recorded reason; the wave still completes.
- AE3. **Covers R6.** A TR's paper smoke fails but the same failure reproduces across multiple TRs or the raw HTTP path is down → classified environmental; the TR stays a candidate but is not flipped to implemented. A retry after recovery confirms the deserialize and flips it; if recovery never lands in-window, the wave ships without it and it stays tracked-only, recorded as pending.
- AE4. **Covers R7.** A TR's paper call confirms its caller-supplied identifiers are accepted and its venue/session assumption → those facets retire from the ledger. The field-level `type` facets do not retire — a clean deserialize does not prove the HTTP-500-seeded types — and stay flagged for the separate re-pin PR.

---

## Scope Boundaries

**Deferred for later**

- `t8430` — promotion gated on the unresolved array-shape blocker; stays tracked-only.
- `t1481`, `t1482` — read-only, but pull in the provisional `krx_extended` vs `krx_regular` session question, a different validation problem; a wave-2 session-semantics cluster.
- The ~23 consumer-less tracked-only TRs — stay tracked-only until a real caller or a drift incident pulls them in.

**Outside this wave's identity**

- Focused Evidence and Recommended TR promotion for any of the 11.
- Any change to `metadata/EVIDENCE-FRESHNESS.md`.
- Orders, account-state, realtime/WebSocket, paper-incompatible, and overseas/futures TRs.
- The clean system-codes re-fetch and field-`type` re-pin (a separate later step per PR #27 sequencing, which would also rewrite existing implemented baselines).

---

## Dependencies / Assumptions

- The `tracked → implemented` recipe does not exist yet; it is built in this wave (R1) and is a prerequisite for R5.
- The 11 TRs are currently tracked-only with committed metadata and normalized baselines from PR #27. Their structs are derived from the same provisional snapshot (captured while LS's system-codes endpoint returned HTTP 500, so some field types fell back to raw codes) — which is precisely why the paper-smoke gate exists.
- The 11 are assumed paper-callable read-only stock TRs (the original 36 excluded paper-incompatible TRs). A TR that turns out non-paper-callable surfaces through R6 block-and-drop rather than being assumed in.
- An Implemented-not-Recommended TR's value: a callable, reference-documented surface carrying the "not yet recommended" banner, and the prerequisite for later recommendation and for composing stock-query workflows.

---

## Outstanding Questions

**Deferred to planning**

- `owner_class` (dependency class) assignment per TR for the 11 — a hard-accurate facet (R5) that drives index routing and the validator cross-check (R9). Scope it as a discrete, bounded plan unit (ideally the first): produce a first-pass assignment for all 11 from the snapshot's `instrument_domain`/`self_paginated` signals, confirmed or corrected during the work, so per-TR domain research can't silently balloon mid-wave.
- Whether the recipe ships as a skill/recipe doc mirroring `promote-tr`, and whether it drives a reusable per-TR helper.
- Representative request inputs (caller-supplied identifiers) for each TR's smoke — some are provisional per the ledger.
- Whether the 11 land as one PR or clustered batches.

---

## Sources / Research

- Support model and three-boolean schema: `crates/ls-metadata/src/schema.rs`; vocabulary (Tracked / Implemented / Recommended TR, Facet Metadata) in `CONTEXT.md`.
- Implemented-TR mechanics (request/response structs, public method, registration): `crates/ls-sdk/src/market_session/mod.rs`, `crates/ls-sdk/src/lib.rs`.
- Existing promotion recipe (implemented → recommended only): `.agents/skills/promote-tr/SKILL.md`.
- Docgen reference-page count test asserting `reference.len() == 8` and the not-recommended banner: `crates/ls-docgen/src/lib.rs` (`reference_covers_seven_implemented_with_banner_and_omits_unimplemented`, ~line 869).
- Recommended-count statement (six Recommended TRs): `metadata/EVIDENCE-FRESHNESS.md`.
- Provisionality ledger from the PR #27 batch: `metadata/PROVISIONALITY-LEDGER.md`.
- Predecessor brainstorm (the tracked-only batch and its roadmap Items 2–4): `docs/brainstorms/2026-06-21-bulk-tracked-only-tr-expansion-requirements.md`.
- `t8430` array-shape blocker: `docs/brainstorms/2026-06-15-sdk-first-slice-decisions-requirements.md`.

---

## Deferred / Open Questions

### From 2026-06-21 review

- **In-wave recipe-stability checkpoint** — the recipe (R1–R4) is authored and validated against the same 11 it first runs on, so a recipe defect discovered on a late TR could unsettle the already-decided states of earlier ones. Consider a sequencing gate: prove the recipe end-to-end on one TR and freeze R1–R4 before promoting the rest. Compatible with keeping all 11 in one wave — it sequences, it doesn't split. (product-lens, adversarial; P2, confidence 75)

- **Interim consumer of an Implemented-but-not-Recommended TR** — each promoted TR is a publicly callable method carrying a "not yet recommended" banner, built from provisional snapshot data. The value statement names "prerequisite for later recommendation and workflow composition" but not who calls these methods in the interim or what they expect; naming the interim consumer (e.g., internal workflow composition vs. external callers expected to wait for Recommended) makes the banner a deliberate positioning choice rather than callable surface with no named consumer. (product-lens; P2, confidence 75)
