---
date: 2026-06-21
topic: field-type-repin-clean-baseline-refresh
---

# Field-Type Re-Pin — Clean Baseline Refresh

## Summary

Run a clean `api-drift` fetch and a reviewed, gated Baseline Promotion that resolves the HTTP-500-seeded field-type provisionality: re-derive the 36 normalized shapes' field `type` values from a live `system-codes` mapping instead of the hardcoded fallback the seed snapshot was stuck with. A new **type-only promotion gate** enforces that only a clean, type-scoped refresh can promote; the per-facet ledger retirement stays a human review act. This lands before the `implemented → recommended` (`Promotion: ready`) flips, which lean on trusting the type-level baseline.

## Problem Frame

The committed API Drift raw snapshot was fetched exactly once (the `70e093b` U6 seed) while the `system-codes` endpoint returned HTTP 500. Under that failure, the normalizer falls back to a hardcoded property-type table to resolve field `type` values (`crates/ls-trackers/src/fetch.rs:430-481`, `api_drift.rs:203-209`). Every one of the 36 normalized baselines therefore inherits a field `type` that was typed by a hardcoded guess, not a live authoritative source. `metadata/PROVISIONALITY-LEDGER.md` §4 records this for the whole batch and rules: "do not treat any of the 36 shapes' field `type` values as type-level ground truth."

That provisionality blocks the step that depends on type-level trust: the `implemented → recommended` flip (the `Promotion: ready` signal in `.agents/skills/promote-tr/references/smoke-map.md`), which rests on the Structural API Shape baseline being trustworthy at the field-`type` level. It does not block `tracked → implemented` expansion — the 2026-06-21 close-out (ledger §5) promoted 11 TRs to Implemented with the §4 field-type provisionality explicitly unchanged. Until a clean fetch re-derives those types from live `system-codes`, recommendation-tier work carries unnecessary caution.

The blocker that previously parked this work is gone. Until recently there was no maintained command to move a fetched raw snapshot into the committed baseline — the refresh lifecycle did not exist. The `api-drift promote --attest` capability (see `docs/brainstorms/2026-06-20-api-drift-baseline-promotion-requirements.md`) now provides the reviewed, attested mutation path. This slice is the first real use of that lifecycle.

## Key Decisions

- **Resolve via clean fetch + whole-raw promotion, not a targeted patch.** Field `type` lives in normalized shapes derived from the raw at normalize time; the only maintained way to refresh it is to refresh the raw and re-derive. Hand-editing types is exactly what the promote capability exists to replace. Whole-raw promotion (the whole-raw rule from `docs/brainstorms/2026-06-20-api-drift-baseline-promotion-requirements.md`) is the deliberate mechanism.

- **Type-only gate, else stop.** Because promotion is whole-raw, any unrelated structural drift accumulated since the seed would be committed into the Reviewed Baseline as a side effect. That is a separate Maintenance Review Decision, not something to smuggle through a type-cleanup slice. The promotion proceeds only when the maintained-shape drift is the expected field-`type` wave plus the one benign kind below; everything else stops the slice.

- **Benign noise is `DescriptionChanged` only.** The only non-type drift kind tolerated alongside the type wave is `DescriptionChanged` (human-readable text, no identity/type/order/contract change). `FieldReordered`, `RateLimitChanged`, and `FieldChanged` whose detail is a length or required-flag change all block — position can matter for LS block semantics, rate is a real operational fact, and length/required are semantic contract changes, not fallback cleanup.

- **Provisionality is about type source, not drift magnitude.** A clean fetch may produce little or even no field-`type` drift if the hardcoded fallback happened to match live `system-codes`. Resolution still has full value: the types become backed by a live authoritative source. Retirement is gated on a clean (non-fallback) fetch, not on drift appearing.

- **Per-facet retirement, coupled to the promotion.** Retire ledger field-`type` provisionality for exactly what the clean fetch proves, split explicitly into Retired (resolved by a non-fallback `system-codes` mapping) and Still-provisional (untyped, raw-coded after the clean fetch, or affected by a blocked/degraded path), each residual carrying its reason. This refines §4's current batch-wide framing. Retirement is not decoupled from promotion — the ledger is where the promoted baseline's proof is recorded — but it is not all-or-nothing either.

- **Enforce the boundary in code; keep the ledger edit human.** The risky part is objective and easy to get wrong, so the promotion boundary (no fallback + type-only drift) is code-enforced. The per-facet Retired-vs-Still-provisional split is an evidence-interpretation step needing readable judgment and notes, so it stays a manual review act. Assisting it with tooling can come later if it recurs.

## Requirements

**Clean-fetch gate**

- R1. The slice requires a fresh `api-drift fetch` whose fetch report shows `property_type_fallback_served == false` — the `system-codes` mapping was resolved live, not from the hardcoded fallback. A `false` flag proves the mapping *source* was live; it does not guarantee every field resolved, because a live mapping missing a particular property-type code still falls back to the raw code for that field. Per-facet resolution is therefore evaluated at retirement (R9), not assumed from the flag. A fallback-served run must not promote — this is already enforced by the existing facts-outage gate (the drift check returns an outage exit for a fallback-served run containing a maintained TR, before drift compare or attestation), so R1 relies on that gate rather than adding a new block. The slice's new work is reading the flag at retirement, not blocking promotion.
- R2. The no-fallback guarantee must hold for the mapping actually used to produce the promoted normalized shapes. The clean live mapping is captured in the staged run (persisted alongside the raw) and reused at re-derivation; promotion does not re-resolve types against a live endpoint at promote time. The no-fallback signal is read from the staged run's fetch report, so a clean staged run carries the guarantee forward with no fetch-vs-promote TOCTOU.

**Type-only promotion gate**

- R3. Promotion refuses unless the maintained-shape drift consists only of (a) `FieldChanged` whose changed detail is the field `type`, and (b) `DescriptionChanged`. This gate is enforced in code, not left to operator review. It must refuse independently of `--attest`: the existing attestation acknowledges *all* gated findings, and the type wave itself gates (Breaking for implemented/recommended TRs, Maintenance for tracked-only — both block), so the gate cannot be satisfied by attesting. It is a strictly narrower refusal that blocks any non-(type `FieldChanged` | `DescriptionChanged`) drift even when the operator would otherwise attest.
- R4. Any other drift kind blocks the promotion and routes to a separate Maintenance Review Decision: `TrAdded`, `TrRemoved`, `FieldAdded`, `FieldRemoved`, `FieldReordered`, `FieldMovedAcrossBlock`, `EndpointChanged`, `ProtocolChanged`, `RateLimitChanged`, `FactsDegraded`, and `FieldChanged` whose detail is a length or required-flag change. A `FieldChanged` whose detail carries *any* non-type component blocks even when it also carries a type change — the gate admits a finding only when its detail is a pure type change. Because `FieldChanged` today bundles type, length, and required changes into one `detail` string, this is a correctness constraint (not a style preference) and argues for enriching the `DriftChange` representation (e.g. a distinct type-change kind) over string-parsing the detail; the representation choice is deferred to planning.
- R5. "Benign noise" is defined exactly as `DescriptionChanged` and the expected field-`type` changes — no broader tolerance. The gate does not silently widen to admit other cosmetic-seeming changes.

**Promotion act**

- R6. When R1 and R3 hold, the slice performs a reviewed Baseline Promotion via the existing `api-drift promote --attest <operator-or-issue>` path: the committed raw is replaced wholesale by the clean staged run's raw, normalized baselines are re-derived, and one promotion record is appended.
- R7. The promotion is attributable: its justification is "clean `system-codes` resolved the field-type provisionality," not "promoted whatever upstream changed since the seed." The promotion record and the type-only gate together carry that attribution.

**Per-facet ledger retirement**

- R8. After a successful promotion, `metadata/PROVISIONALITY-LEDGER.md` §4 is edited by hand to split the 36 field-`type` facets into Retired (type resolved by a non-fallback `system-codes` mapping) and Still-provisional (untyped, still raw-coded after the clean fetch, or affected by any blocked/degraded path), each Still-provisional entry naming its exact reason.
- R9. Retirement records only what the clean fetch proves. Facets the fetch did not concretely resolve are not retired, even when the promotion otherwise succeeded.
- R10. §4's current batch-wide claim ("resolved batch-wide ... not per-TR work") is corrected to the per-facet model; the ledger is left with no ambiguous mixed blob.

## Key Flows

- F1. Clean type-only promotion
  - **Trigger:** Operator runs a fresh fetch with `system-codes` resolving live, then promotes.
  - **Steps:** Fetch → confirm `property_type_fallback_served == false` (R1) → run drift check → gate confirms drift is type-wave + `DescriptionChanged` only (R3) → `promote --attest` replaces raw, re-derives normalized, appends record (R6) → operator hand-edits ledger §4 into Retired / Still-provisional split (R8).
  - **Outcome:** Baseline types backed by live `system-codes`; field-type provisionality retired per facet. This removes one of several recommendation gates — `Promotion: ready` additionally requires the §1–§3 live-behavior verification this slice does not perform.
  - **Covers:** R1, R3, R6, R8.

- F2. Fallback-served fetch
  - **Trigger:** A fresh fetch where `system-codes` failed again and the fallback was served.
  - **Steps:** Fetch report shows `property_type_fallback_served == true` → slice stops before promotion (R1).
  - **Outcome:** No promotion, no ledger change; the operator retries the fetch when `system-codes` is healthy.
  - **Covers:** R1.

- F3. Non-type structural drift present
  - **Trigger:** A clean fetch carries a blocking drift kind alongside the type wave (e.g. a new/removed field, a new TR, a required-flag change).
  - **Steps:** No-fallback gate passes → type-only gate detects a blocking kind (R4) → promotion refuses → operator opens a separate Maintenance Review Decision for that drift.
  - **Outcome:** Baseline unchanged; the type re-pin waits behind a separate reviewed decision; nothing rides along silently.
  - **Covers:** R3, R4.

- F4. Clean fetch, minimal drift
  - **Trigger:** A clean fetch where the live `system-codes` mapping matches the hardcoded fallback, so few or no field `type` values change.
  - **Steps:** No-fallback gate passes (R1) → type-only gate passes with little/no drift (R3) → promote → retire facets the clean fetch proves (R8, R9).
  - **Outcome:** Provisionality retired despite near-zero drift, because the types are now live-sourced.
  - **Covers:** R1, R8, R9.

## Acceptance Examples

- AE1. **Covers R1 (refuse-branch).** Given a fresh fetch whose report has `property_type_fallback_served == true`, when the slice runs, then it stops before promotion and the ledger is unchanged. R1's clean-path precondition (`== false` → promotion proceeds) is exercised by AE2.
- AE2. **Covers R3, R6.** Given a clean fetch whose only maintained-shape drift is field-`type` changes plus `DescriptionChanged`, when promote runs with `--attest`, then it proceeds, replaces the raw, re-derives normalized shapes, and appends one promotion record.
- AE3. **Covers R4.** Given a clean fetch that also adds a new field to a maintained TR, when the type-only gate evaluates the drift, then promotion refuses and the change is routed to a separate Maintenance Review Decision.
- AE4. **Covers R4.** Given a clean fetch whose only extra change is a `FieldChanged` with a required-flag detail, when the gate evaluates it, then promotion refuses — required-flag is not benign noise.
- AE5. **Covers R8, R9.** Given a successful promotion where 30 of 36 facets resolved to a live type and 6 remained raw-coded, when the ledger is edited, then 30 facets move to Retired and 6 stay Still-provisional, each of the 6 naming its reason.
- AE6. **Covers R8, R10.** Given the ledger after retirement, when a reader scans §4, then it shows an explicit Retired / Still-provisional split with no batch-wide "all 36 provisional" claim and no mixed-state ambiguity.

## Scope Boundaries

**Deferred for later**
- Assisted ledger retirement — tooling that emits the per-facet Retired / Still-provisional split from the drift + fallback report. Manual for now; revisit if it recurs.
- The downstream payoff itself — actual `implemented → recommended` (`Promotion: ready`) flips. This slice removes the field-type gate holding them back; it does not perform them, and it is not a prerequisite for `tracked → implemented` expansion (which already proceeds under provisionality).
- Scheduling the `spec-doc check` cadence. The capability this slice exercises also unblocks that parked brainstorm, but it remains separate work. The untracked `docs/brainstorms/2026-06-20-schedule-spec-doc-cadence-requirements.md` should not be committed as-is.

**Outside this product's identity**
- Resolving the other ledger provisionality — `venue_session` (§1), `caller_supplied_identifiers` (§2), and discovery-style relationships (§3). Those require live behavior verification, not a clean fetch, and are out of this slice.
- New-TR admission into `code-set.json` and any non-type structural drift. These are deliberately gated out (R4) into separate reviewed decisions; whole-raw promotion does not get to smuggle them in.

## Dependencies / Assumptions

- Builds on the existing `api-drift promote --attest` lifecycle (`docs/brainstorms/2026-06-20-api-drift-baseline-promotion-requirements.md`), the `property_type_fallback_served` fetch-report signal and its facts-outage gate (U5), and the typed `DriftChange` enum that already distinguishes `FieldChanged` from structural kinds.
- Assumes `system-codes` can be fetched cleanly at least once in the work window. If it stays unhealthy, the slice cannot proceed (F2) and waits.
- Assumes the type-only gate can be expressed over the existing `DriftChange` output, including distinguishing a type-detail `FieldChanged` from a length/required-flag `FieldChanged` (both currently share the variant via a `detail` string).
- The staged run persists both the raw (property-type *codes* per field) and the resolved property-type name-mapping captured at fetch time. Re-derivation at promote reuses that staged mapping rather than re-fetching `system-codes`; the live-vs-fallback distinction is recorded in the staged run's fetch report, not inferable from the raw bytes alone. So R2 holds whenever the staged run was produced by a clean (non-fallback) fetch. The fallback-served promote is blocked by the facts-outage gate only when the run contains a maintained TR — always true for the §4 batch, but a precondition to state rather than assume.
- A type-detail `FieldChanged` gates for every maintained tier: the drift gate fires for any maintained TR (Tracked included) at severity ≥ Maintenance, so the type wave gates as Breaking for implemented/recommended TRs and as Maintenance for tracked-only TRs — only Untracked TRs do not gate. So `--attest` is load-bearing for the whole §4 type wave, not just the implemented/recommended subset.

## Outstanding Questions

**Resolve Before Planning**
- None blocking.

**Deferred to Planning**
- The `DriftChange` representation used to satisfy R4's pure-type-change rule (enrich the enum with a distinct type-change kind vs parse the bundled `detail` string). R4 fixes the correctness constraint; only the representation is open.
- Whether the type-only gate is a new flag/mode on `promote` or a distinct pre-promotion check the operator runs.
- The exact ledger §4 rewrite shape for the Retired / Still-provisional split (table columns, where retired facets are recorded vs removed), and how a facet's "raw-coded after a clean fetch" state is surfaced as a machine-readable signal for R9's per-facet judgment.

### From 2026-06-21 review
- Type-only-else-stop has no backstop. Because a clean fetch may carry accumulated non-type drift that F3 routes to a separate Maintenance Review Decision, the slice can stall behind an unscheduled review while downstream recommendation work waits on it. Decide how that decision is triggered and time-boxed, and whether the type re-pin re-runs automatically once it resolves, so the slice cannot park indefinitely.

## Sources / Research

- `metadata/PROVISIONALITY-LEDGER.md` — §4 (the 36 field-`type` facets, batch-wide framing, "not type-level ground truth"), and §1–§3 (the other provisional facets this slice leaves untouched).
- `crates/ls-trackers/src/fetch.rs:430-481` — `property_type_mapping()` and the fallback-served signal; `crates/ls-trackers/src/types.rs:424-429` — `FetchReport.property_type_fallback_served`.
- `crates/ls-trackers/src/api_drift.rs:203-209` — field `type` resolution from the property-type code (`prop_types.get(code)...unwrap_or_else(|| code.to_string())`).
- `crates/ls-trackers/src/types.rs:460-527` — the `DriftChange` enum (`FieldChanged`, `FieldAdded`, `DescriptionChanged`, `RateLimitChanged`, etc.).
- `crates/ls-trackers/src/cli.rs:745-865` — `promote_committed` flow (gate, whole-raw hash, in-memory re-derive, TOCTOU guard, write, append record).
- `.agents/skills/promote-tr/references/smoke-map.md` — the `Promotion: ready` flip that field-type provisionality blocks.
- `docs/brainstorms/2026-06-20-api-drift-baseline-promotion-requirements.md` — the promote-with-attest capability this slice is the first to exercise.
- `crates/ls-trackers/baselines/api-drift/SEED-ATTESTATION.md` — the HTTP-500 seed narrative that introduced the provisionality.
