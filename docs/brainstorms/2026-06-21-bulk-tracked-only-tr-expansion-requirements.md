---
date: 2026-06-21
topic: bulk-tracked-only-tr-expansion
---

# Bulk Tracked-Only TR Expansion

## Summary

Bring 36 named read-only stock TRs into full structural maintenance ownership as a single Tracked-Only Expansion Work Item: each gets a per-TR metadata file, a routing index entry, generated TR Dependency Docs, and a committed API Drift normalized baseline. None becomes callable, recommended, or evidence-backed. The shape data is sourced from the already-committed raw snapshot, not the decommissioned migration source. Items 2–4 (implemented expansions) and the orders deferral are captured as roadmap only.

## Problem Frame

The maintained surface tracks 8 TRs today (7 implemented, 1 tracked-only). The committed API Drift raw snapshot already carries ~365 LS codes, including all 36 targets, so the inventory diff *sees* these TRs — but with no metadata they classify as `Untracked`, which is the report-only, low-gating tier. Their request/response fields are not diffed at all, because structural drift runs only over TRs that have a committed normalized shape. Inventory-level drift already detects add/remove (the code-set lists them) and none of the 36 is callable, so the residual exposure is narrower than a full blind spot: a silent field-level change (renamed, added, or removed response fields) goes undetected and is caught today only when a future promotion re-verifies the shape.

These 36 were selected as the low-risk read-only stock set precisely so coverage can grow without touching order safety or callable behavior. Tracking them converts a large blind spot into structural drift visibility cheaply, and seeds the candidate pool the later implemented expansions draw from.

## Key Decisions

- **Tracked-only means structural ownership, not inventory ownership.** Each TR gets a committed normalized baseline (not just a metadata stub), matching the `CSPAT00601` precedent. Without the baseline, "tracked" would only reclassify add/remove severity and coverage; field-level drift would still be invisible. The cost — the drift tracker now diffs 36 more TRs every run — is accepted because the set was chosen for low risk; if noise becomes a problem, it is solved in tracker severity/routing, not by weakening the definition of tracked-only.
- **Tiered facet correctness bar with recorded provisionality.** Facets that drive routing, severity, docs, and later evidence selection are accurate up front; descriptive facets that don't are allowed to be provisional, but the provisionality is recorded so a later promotion knows what to re-verify. Pure best-effort is too loose; full hand-verification of 36 TRs defeats the point of a bulk batch.
- **Shape data comes from the committed raw snapshot.** Normalized baselines are derived from `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`, which already contains all 36 codes — the decommissioned migration source is not consulted.
- **The deliverable is the bulk batch; promotion is a separate item.** The `tracked → implemented` recipe (which does not exist yet) is real design work and is deliberately out of this work item so the batch stays fast and the goal — grow maintained drift coverage safely — stays unblurred.

## Requirements

**Per-TR maintained artifacts**

- R1. Each of the 36 TRs gets a per-TR metadata file at `metadata/trs/<code>.yaml` following the established schema, with `support.tracked: true`, `support.implemented: false`, `support.recommended: false`, and no `recommendation` block.
- R2. Each TR is routed in `metadata/tr-index.yaml` with the index's duplicated fields (`file`, `owner_class`, `protocol`, `instrument_domain`, `venue_session`); the metadata validator asserts every index field equals its per-TR file.
- R3. Each TR gets a committed API Drift normalized structural baseline at the trackers' per-TR normalized path, derived from the committed raw snapshot, matching the `CSPAT00601` precedent.
- R4. Each TR appears in the generated TR Dependency Docs.

**Facet correctness bar**

- R5. These facets are accurate at commit time: `support`, `owner_class`, `protocol`, `instrument_domain`, `certification_path`, `paper_incompatible`, `account_state`, `self_paginated`, and order/dependency risk fields.
- R6. These facets may be provisional and must be explicitly marked as such: `venue_session`, `caller_supplied_identifiers`, and weak discovery-style relationships. Field-level `type` facets in the baselines are also provisional: the raw snapshot was captured while LS's system-codes endpoint returned HTTP 500, so some field types fall back to raw codes. A future clean fetch resolves them and will emit a one-time wave of `type` field-changed findings across the batch — a planned re-pin, not surprise drift.
- R7. Provisionality is recorded in a committed batch-level provisionality ledger (a file committed alongside the batch, not a metadata-schema change) that lists, per TR, which fields are provisional, so a later `tracked → implemented` promotion can identify exactly which fields to re-verify (caller inputs, venue/session assumptions, weak dependency/discovery relationships, and the field-level `type` facets per R6). A per-TR metadata field is the alternative if the schema is extended at promotion-design time (see Outstanding Questions).

**Support boundary**

- R8. No TR in the batch gains callable SDK behavior, an SDK Reference page, a recommendation claim, or a Focused Evidence requirement.

**Batch verification**

- R9. The batch passes the metadata validator (schema + index/per-TR consistency) and the API Drift tracker accepts the 36 new baselines.
- R10. Artifacts whose pinned counts track the *tracked-TR set* are updated so the batch lands green — the docgen dependency-doc count assertion and its hardcoded tracked-TR fixture move from 8 to 44. The count-bearing test function name (which embeds `eight`) and its assertion message string (which embeds `8 pages`) are updated to match, so the committed test carries no stale count. Assertions pinned to implemented/recommended counts must stay unchanged, since no TR becomes implemented or recommended (the SDK Reference banner test stays at 7 implemented pages; the `EVIDENCE-FRESHNESS.md` "six Recommended TRs" count is untouched).

## Scope Boundaries

**Deferred for later**

- The `tracked → implemented` promotion recipe — the shared path items 2–4 all need.
- Items 2–4 themselves (see Roadmap): instrument discovery, market list/discovery cluster, ranking/list screens.
- Orders runtime (`CSPAT00601`) stays deferred until the full order-safety package is the target work item.

**Outside this work item's identity**

- Callable SDK APIs, SDK Reference pages, recommendation claims, and Focused Evidence for any of the 36 — tracking them now is structural ownership only.

## Roadmap (captured, not in scope)

These are the prioritized follow-ons that draw their targets from this batch. Each requires the `tracked → implemented` recipe to exist first, and each inherits the R7 duty to re-verify the promoted TR's provisional facets before it becomes callable.

- **Item 2 — instrument discovery:** `t8436` first, then `t8430` only if its known upstream array-shape blocker is no longer valid. Unlocks symbol/code discovery for quote workflows.
- **Item 3 — market list / discovery cluster:** promote `t8425`, `t1531`, `t1537`, `t1403` into callable APIs.
- **Item 4 — ranking / list screens:** promote the high-user-value read-only list TRs `t1441`, `t1452`, `t1463`, `t1466`, `t1489`, `t1492`.

The 36 codes in this batch: `t3102`, `t3320`, `t3341`, `t1640`, `t1662`, `t1601`, `t1615`, `t1664`, `t1958`, `t1964`, `t1988`, `t8431`, `t9905`, `t9907`, `t9942`, `t1531`, `t1537`, `t8425`, `t1825`, `t1826`, `t1852`, `t1856`, `t1866`, `t1859`, `t1860`, `t1441`, `t1452`, `t1463`, `t1466`, `t1481`, `t1482`, `t1489`, `t1492`, `t1403`, `t8430`, `t8436`.

## Dependencies / Assumptions

- The committed raw snapshot contains all 36 codes (verified) — it is the shape-data source for R3.
- The committed code-set already lists all 36, so inventory-level drift already detects their add/remove. Tracking changes their support classification (`Untracked` → `tracked`) and adds field-level structural diffing; it does not "discover" them.
- `t8430` is recorded as blocked by an unresolved upstream array-shape issue. It is in this batch as tracked-only, but its implemented promotion (Item 2) is gated on that blocker being re-checked.
- Increased drift-diff surface on these 36 is an accepted assumption, but the consequence is a gate, not just a report: tracked TRs are *maintained*, so a field-level change scores `Maintenance` severity and `gates_for` returns true — the API Drift run exits non-zero on real upstream field drift in any of the 36. The accepted noise must therefore be absorbed by a real tracker severity/routing change, not merely tolerated; its magnitude depends on LS's change rate on these read-only stock TRs.
- The committed raw snapshot and its `code-set.json` are themselves marked `provisional: true` — the inventory is not independently attested as complete. The 36 codes are individually present, but the "verified" framing means present-in-snapshot; the batch inherits that provisionality.
- Sequencing constraint: this batch must be normalized and land against the *current* committed snapshot. `api-drift renormalize` rewrites every maintained shape from the raw, so it stays additive (the existing 8 shapes unchanged) only while the raw is unchanged. If a clean system-codes re-fetch — the R6 `type` re-pin — lands a new raw first, the same pass would also rewrite the existing implemented-TR baselines, mixing unrelated drift into this tracked-only PR. The re-pin is therefore a separate, later step that lands after this batch.

## Outstanding Questions

**Deferred to planning**

- Whether the 36 land as one PR or as smaller clustered batches.
- The generation mechanism is not greenfield: `api-drift renormalize` (`renormalize_committed`) already re-derives per-TR normalized shapes for all maintained codes from the committed raw, so once the 36 metadata files are authored a single renormalize pass produces the baselines (R3). Open: whether to wrap this in a reusable tracked-only-add helper or run it directly — a `tracked → implemented` recipe remains separate and out of scope.
- How provisionality (R7) is recorded — a per-TR field/marker vs. a batch-level ledger.
- The `owner_class` (dependency class) assignment for each of the 36, which is an R5 hard-accurate facet and must be decided per TR during the work.

## Sources / Research

- TR metadata schema and the three-boolean support model: `crates/ls-metadata/src/schema.rs`.
- Tracked-only precedent (metadata + normalized baseline, not implemented/recommended): `metadata/trs/CSPAT00601.yaml`, `crates/ls-trackers/baselines/api-drift/normalized/trs/CSPAT00601.json`.
- Drift comparison: inventory diff keys off support state, structural diff runs only over committed per-TR shapes — `crates/ls-trackers/src/api_drift.rs` (`compare`, `support_state_for`, `removal_severity`).
- Committed baseline layout (raw snapshot, code-set, per-TR normalized shapes): `crates/ls-trackers/baselines/api-drift/`.
- Docgen dependency-doc count test and its hardcoded fixture (breaks on count change, must be updated per R10): `crates/ls-docgen/src/lib.rs` (`every_tracked_tr_gets_a_page_and_the_index_lists_all_eight`, the `TRACKED_TRS` array).
- Docgen SDK Reference banner test and `EVIDENCE-FRESHNESS.md` recommended count (must stay unchanged per R10): `crates/ls-docgen/src/lib.rs` (`reference_covers_seven_implemented_with_banner_and_omits_unimplemented`), `metadata/EVIDENCE-FRESHNESS.md`.
- Existing promotion recipe (implemented → recommended only; no tracked → implemented path): `.agents/skills/promote-tr/SKILL.md`.
- `t8430` upstream array-shape blocker: `docs/brainstorms/2026-06-15-sdk-first-slice-decisions-requirements.md`.
- Domain vocabulary (Tracked TR, Tracked-Only Expansion Work Item, Implemented/Recommended TR, Facet Metadata, Reviewed Baseline): `CONTEXT.md`.

## Deferred / Open Questions

### From 2026-06-21 review

- **owner_class hard-accurate but per-TR assignment unresolved for all 36** — R5 / Outstanding Questions (P1, scope-guardian, confidence 75)

  R5 makes `owner_class` accurate-at-commit, and it drives index routing (R2), dependency-doc rendering (R4), and the validator cross-check (R9) — but assigning the dependency class for each of 36 TRs is non-trivial per-TR domain research the doc neither bounds nor sketches. Decide whether to add a non-binding classification sketch (mapping the 36 to the existing class vocabulary via instrument_domain/self_paginated signals from the snapshot) to size the batch, or to call per-TR owner_class research out as a discrete unit so the "36 at once" decision can be revisited if the cost is large.

- **Drift-visibility value loop undefined for tracked-only TRs** — Problem Frame (P2, product-lens, confidence 75)

  The stated value is "structural drift visibility," but visibility only pays off if a field-level finding on a non-callable, non-evidence-backed TR drives an action. For an implemented/recommended TR the consumer is obvious; for a tracked-only TR with no caller, who reviews the finding and what response it triggers is unnamed. Define the intended consumer/response (e.g., "a tracked-only drift finding routes to the next promotion review as a re-verify trigger, not to immediate maintenance") so the value is a real loop rather than asserted visibility.

- **All-36-now vs. roadmap-bound subset (~23 have no named consumer)** — Roadmap / Scope Boundaries (P2, product-lens, confidence 75)

  Only ~13 of the 36 have a named item 2–4 destination; the other ~23 are pure standing drift surface with no callable destination on the current roadmap. A cheaper alternative is to track only the roadmap-bound subset now and let the rest stay Untracked until a promotion or a real drift incident pulls them in. Either justify why the consumer-less TRs earn structural ownership now (drift-history completeness, batch amortization), or scope the batch to the consumer-bound set.
