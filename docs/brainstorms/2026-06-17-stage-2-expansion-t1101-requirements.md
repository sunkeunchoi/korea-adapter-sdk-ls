---
date: 2026-06-17
topic: stage-2-expansion-t1101
---

# Stage-2 Expansion: Adopt TR t1101 to prove the Maintenance Flow

## Summary

Adopt upstream TR `t1101` (주식현재가호가조회 — stock current-price + order-book) as the
first real SDK-facing expansion item, and take it to `support:recommended` via a Paper
Live Smoke. This is Stage 2 of the two-stage Foundation Complete gate: it exercises the
full Maintenance Flow on a real TR — new metadata, Baseline Promotion, SDK code, a
Change-Scoped Gate, and Focused Evidence. Foundation Complete is claimed only when this
item closes.

## Problem Frame

Stage 1 (issue #9) proved the queue plumbing is self-consistent but nulled out every
load-bearing field, so it cannot prove the flow carries weight. Both trackers were run for
grounding and offered no candidate: `api-drift-check` is blocked by an upstream
`property_type` 500 outage, and `spec-doc-check` is clean. So Stage 2 must be a deliberate
expansion choice. The maintained SDK currently owns exactly seven TRs — one per dependency
class, almost entirely domestic stock — so adopting one more real TR is both the cleanest
proof and a genuine coverage gain.

## Key Decisions

- **Candidate is `t1101`.** Closest sibling to the already-owned `t1102` (same `[주식] 시세`
  group), read-only, lowest incidental complexity.
- **It becomes the second TR in `market_session`.** Until now every dependency class holds
  exactly one TR; a second TR is the first real test that the class grouping generalizes.
- **Target support state is `recommended`**, reached only if the Paper Live Smoke succeeds —
  paper-workability is the binding selection constraint.
- **A "→ recommended" candidate must be self-contained.** It must be verifiable on the
  paper gateway with only OAuth plus caller-supplied identifiers, no prerequisite TR
  implemented first. This is why `CSPAT00601` (orders) is excluded — orders need additional
  TRs and the unshipped order-safety slice (ADR 0008).
- **The first expansion stays domestic stock.** Overseas / futures / overseas-futures paper
  support is unverified, so the coverage-gap domains are out of reach for a paper-proven
  first item.

## Requirements

**Candidate and classification**

- R1. The work item adopts upstream TR `t1101` as a new `queue:expansion` item; it is not
  one of the seven currently maintained TRs.
- R2. `t1101` is classified `owner_class: market_session`, `instrument_domain: stock`,
  `protocol: rest`, `venue_session: krx_regular`.
- R3. The candidate must be callable on the paper gateway with only OAuth plus
  caller-supplied identifiers, with no prerequisite TR required first.

**Maintained artifacts the item must produce**

- R4. New TR Maintenance Metadata for `t1101`, plus its TR Metadata Index entry, validated.
- R5. A Baseline Promotion admitting `t1101` into the Reviewed Baseline and reviewed
  code-set, performed as a separate review act.
- R6. SDK behavior for `t1101` added to the Maintained SDK Surface in the market-session
  area.
- R7. SDK Reference Docs / TR Dependency Docs regenerated from the maintained behavior and
  metadata.

**Verification and support state**

- R8. A Change-Scoped Gate naming runnable checks scoped to `t1101` and the market-session
  class; the gate must pass for completion.
- R9. Focused Evidence in the form of a Paper Live Smoke targeting `t1101`, recorded as a
  durable, credential-free evidence record (`env: paper`), captured during an open KRX
  regular session.
- R10. Target support state is `recommended` (tracked + implemented + recommended), reached
  only on a successful Paper Live Smoke; otherwise the item completes at `implemented` (see
  AE2).

**Queue mechanics**

- R11. The item is opened with the SDK work item template and labelled `queue:expansion`,
  `source:manual`, `class:market-session`, the achieved support state, `gate:change-scoped`,
  `baseline:promotion-needed`, and `evidence:needed`.
- R12. Completion requires the template checklist satisfied: maintained artifacts updated,
  Change-Scoped Gate passed, Baseline Promotion decision recorded, Focused Evidence decision
  recorded.

## Acceptance Examples

- AE1. **Covers R9, R10.** Paper Live Smoke returns a valid current-price / order-book
  payload → record the evidence, set `recommended`, close as a recommended expansion.
- AE2. **Covers R3, R10.** Paper Live Smoke returns the `01900` paper-incompatible signal →
  reclassify the TR as paper-incompatible, stop at `implemented`, record the discovery as the
  evidence decision. This still closes as a valid Stage-2 proof — it proves the flow handles
  a paper-incompatible discovery.
- AE3. **Covers R9.** Smoke attempted outside KRX regular hours → a session-closed response
  is not valid Focused Evidence; rerun during an open session.

## Scope Boundaries

- Overseas / futures / overseas-futures expansion — deferred; paper support unverified.
- The single-select dependency-class / support-state template question (carried from
  `docs/brainstorms/2026-06-17-maintenance-queue-pressure-test-requirements.md`) — stays
  deferred; `t1101` is cleanly single-class and does not exercise it.
- `CSPAT00601` (orders) promotion — excluded by the self-contained criterion.
- Alternate candidates `t1305` (paginated) and `t8407` (multi-quote) — considered, not
  chosen for the first item.

## Dependencies / Assumptions

- Paper gateway and paper credentials are available and working — proven by the `token`
  Paper Live Smoke on 2026-06-16 (`metadata/evidence/token.yaml`).
- **Assumption:** `t1101` is paper-compatible. The Paper Live Smoke validates this directly;
  AE2 covers the failure path.
- The upstream `property_type` api-drift outage does not block this item — the paper TR path
  is independent of that endpoint.
- Foundation Complete is claimed only after this item closes (two-stage gate, per the prior
  pressure-test doc and `docs/MAINTENANCE_RUNBOOK.md`).

## Outstanding Questions

**Deferred to planning**

- Exact `t1101` request identifiers and response shape (e.g. order-book depth modeling).
- Exact Change-Scoped Gate command names for the market-session class.
- Whether the second TR in `market_session` reveals any shared-module refactor in the
  maintained surface.

## Sources / Research

- `CONTEXT.md` — glossary and relationships (dependency class, support states, Focused
  Evidence, Baseline Promotion, Foundation Complete).
- `docs/MAINTENANCE_RUNBOOK.md`, `docs/maintenance-labels.md`,
  `.github/ISSUE_TEMPLATE/sdk_work_item.yml` — queue contract.
- `docs/brainstorms/2026-06-17-maintenance-queue-pressure-test-requirements.md` — Stage 1 /
  Stage 2 framing and the deferred multi-class question.
- `metadata/tr-index.yaml`, `metadata/trs/*.yaml` — current 7-TR inventory and support map.
- `metadata/evidence/token.yaml` — Paper Live Smoke evidence shape precedent.
- `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json` — upstream catalog
  (`t1101` name and `[주식] 시세` grouping).
