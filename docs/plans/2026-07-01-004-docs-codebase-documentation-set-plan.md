---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
date: 2026-07-01
---

# Codebase Documentation Set - Plan

## Goal Capsule

- **Objective:** Produce four maintainer/contributor-facing markdown docs capturing
  the current state of the codebase — `README.md` (refresh in place), `USER_GUIDE.md`,
  `ARCHITECTURE.md`, and `TR_LIFECYCLE.md` — posture-tuned to *point at* canonical
  sources rather than duplicate them, so they resist the drift this project already
  guards against.
- **Product authority:** Maintainers/contributors are the primary readers (not external
  SDK consumers). Docs teach the workspace, the maintenance flow, and how a TR climbs
  its support ladder.
- **Open blockers:** None. Placement of `ARCHITECTURE.md` / `TR_LIFECYCLE.md` (root vs
  `docs/`) defaulted to root; see Outstanding Questions.

## Product Contract

### Problem

The raw material describing "how this codebase works" already exists but is scattered
and shaped for machines or narrow audiences: `CONTEXT.md` and `CONCEPTS.md` are two
overlapping glossaries, `AGENTS.md` is agent guidance, `docs/reference/` +
`docs/tr-dependencies/` are metadata-projected, and `.agents/skills/` holds frozen
recipes. There is no human-readable narrative that orients a new maintainer/contributor
to the workspace, the maintenance flow, or the TR support ladder. The existing
`README.md` is a good *positioning* doc (selective-by-design, Implemented≠Recommended)
but not an entry point into the codebase's structure or workflow.

### Users & value

Maintainers and contributing agents/humans. Value: a fast, drift-resistant orientation
layer — narrative "shape and why" that links to the authoritative source for detail,
so onboarding does not require reverse-engineering the glossaries and recipes.

### Deliverables

Four docs. Posture is **hybrid**: README + USER_GUIDE are thin orienting layers;
ARCHITECTURE + TR_LIFECYCLE are richer and more self-contained because that structure
changes slowly. Every doc opens by stating its audience and naming what it does **not**
own, linking to the canonical source for that.

1. **`README.md`** (repo root — refresh in place, do NOT overwrite its framing)
   - Keep the existing selective-by-design positioning (Implemented≠Recommended,
     advisory trackers, standalone, migration-source decommissioned).
   - Add a "Start here" doc-map section linking `USER_GUIDE.md`, `ARCHITECTURE.md`,
     `TR_LIFECYCLE.md`, plus `AGENTS.md` and `CONCEPTS.md`.
   - Owns: positioning + navigation. Does not own: workflow steps, architecture detail,
     live counts (point at generated `docs/reference/`).

2. **`USER_GUIDE.md`** (repo root — new, thin/orienting)
   - Contributor workflow: clone → build → the gate (`make docs`, `cargo test`,
     `cargo test -p ls-core`, `make docs-check`, `make lane-check`) → where the skills
     live (`.agents/skills/`) → live-smoke + named per-lane env-file basics
     (`.env.<lane>`, `LS_TRADING_ENV=paper`).
   - Links into `AGENTS.md` (gate, gotchas) and `.agents/skills/*/SKILL.md` for the
     authoritative steps rather than restating them.
   - Owns: the "how do I work in this repo" narrative. Does not own: per-recipe step
     detail (links to the SKILL.md files), gateway gotchas beyond a pointer.

3. **`ARCHITECTURE.md`** (repo root — new, richer/self-contained)
   - Workspace/crate map: `ls-core` (dispatch: `Inner::post` / `post_paginated`,
     endpoint policies, serde helpers), `ls-sdk` (per-TR request/response structs +
     facade handles; `market_session/`, `paginated/`, `account/`, `realtime/`, order
     modules), `ls-metadata` (schema + validator over `metadata/trs/*.yaml`,
     `tr-index.yaml`), `ls-trackers` (API-drift + spec trackers, normalized baselines
     as wire-shape source of truth), `ls-docgen` (projects `docs/reference/` +
     `docs/tr-dependencies/` from metadata), `ls-sdk-test-support` (wiremock helpers).
   - The dispatch runtime and `owner_class` routing (market-session / paginated /
     account / realtime / order).
   - The metadata → docgen projection and the two **advisory** change trackers
     (API Drift, Specification Document) and why they never mutate SDK state.
   - Owns: structural map + data-flow narrative. Does not own: exhaustive per-TR
     contracts (generated `docs/reference/`), full vocabulary (`CONTEXT.md`).

4. **`TR_LIFECYCLE.md`** (repo root — new, richer/self-contained; the "how the TR
   transition works + requirements" doc)
   - The support ladder Raw → Tracked → Implemented → Recommended, with the
     **requirement/gate at each rung**:
     - Raw → Tracked: author `metadata/trs/<tr>.yaml` + `tr-index.yaml` entry, project
       the normalized baseline (`make api-drift-renormalize`; baseline is projected,
       never hand-authored). Driven by the `track-tr` recipe.
     - Tracked → Implemented: hand-authored callable Rust + a passing **Paper Live
       Smoke** (constructs, success code, non-empty result, deserializes). Register each
       `{TR}_POLICY` in the required crosscheck list(s). Driven by `implement-tr`
       (`implement-realtime-tr` for WebSocket).
     - Implemented → Recommended: recorded **Focused Evidence** + a recommendation block;
       90-day freshness backstop. Driven by `promote-tr`.
   - The realtime variant: WebSocket **lifecycle (Transport) reachability** as the gate
     (connect → subscribe → unsubscribe), row contents provisional; connection-reachable-
     only calibration and the negative control.
   - Adjacent states: **Pending**, **Paper-incompatible**, **Finish-the-flip**, and the
     Provisionality Ledger's role.
   - Owns: the narrative of the lifecycle and its requirements, cross-linked to the
     driving skill recipe for each transition. Does not own: the recipe steps themselves
     (`.agents/skills/`), term-precise definitions (`CONCEPTS.md`).

### Success criteria

- All four docs exist; README refreshed without losing its positioning content.
- Each doc states audience + what it does not own, and links to the canonical source
  for delegated detail.
- No live tracked/implemented/recommended **counts** are hard-coded in prose; such
  claims point at generated `docs/reference/`.
- Every rung transition in `TR_LIFECYCLE.md` names its driving skill recipe and its gate.
- Internal links resolve; file references are repo-relative.

### Scope boundaries

**In scope:** the four docs above; a doc-map/"Start here" addition to README; repo-relative
cross-links to existing canonical sources.

**Deferred for later (out of scope here):**
- Merging or de-duplicating the `CONTEXT.md` ↔ `CONCEPTS.md` glossary overlap — note it
  as a follow-up in the docs, do not resolve it.
- Regenerating or editing `docs/reference/` / `docs/tr-dependencies/` (owned by
  `make docs`).
- Any change to code, metadata, skills, or the gate.

**Outside this doc set's identity:** external-SDK-consumer "how to call the API" tutorials;
these docs address maintainers/contributors.

### Dependencies / assumptions

- Assumes the current `README.md` positioning content stays valid and is refreshed, not
  replaced.
- Assumes root placement for the three new docs (matching `README.md` discoverability);
  revisit if the project prefers `docs/`.
- Content is sourced from current repo state: `AGENTS.md`, `CONCEPTS.md`, `CONTEXT.md`,
  crate layout, and `.agents/skills/*/SKILL.md`. No new facts invented.

### Outstanding questions

- **Placement:** root vs `docs/` for `ARCHITECTURE.md` and `TR_LIFECYCLE.md`. Defaulted
  to **root**; low-cost to move.
- **README refresh extent:** minimal doc-map addition (assumed) vs a broader rewrite.
  Assumed minimal to preserve the existing framing.
