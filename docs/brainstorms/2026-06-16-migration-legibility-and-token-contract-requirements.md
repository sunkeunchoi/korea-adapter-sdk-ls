---
title: "Migration legibility and the token recommendation contract"
date: 2026-06-16
topic: migration-legibility-and-token-contract
type: requirements
origin: brainstorm from docs/plans/2026-06-16-009-post-pr7-migration-status-what.md (improvements #1, #2, #3)
---

# Migration legibility and the `token` recommendation contract

## Summary

Make the post-PR7 state legible to anyone who arrives without reading the
internal plans. Three coupled documentation moves: (1) a migration closeout
notice in `korea-broker-sdk-ls` so the old repository stops presenting itself as
the active full SDK; (2) a top-level README in `korea-adapter-sdk-ls` stating the
maintained, selective, evidence-based SDK contract; (3) a real **Recommended TR**
contract for `token`, whose generated reference page still says request/response
schemas are deferred until recommended status — which `token` has now reached.
The audience is a future maintainer, reviewer, or agent re-entering the project,
not external SDK consumers. No SDK behavior, tracker logic, or freshness
enforcement changes here.

## Problem Frame

PR #7 made `token` the first Recommended TR and rewrote the freshness policy to
state the truth. But the project's *outward-facing* state still lies in three
places, and each lie can misdirect future work:

- **Ownership is ambiguous.** `korea-adapter-sdk-ls` calls `korea-broker-sdk-ls`
  a **Migration Source** only, but the old repository still reads like `ls-sdk`,
  an active full SDK with 365 typed TRs and release-roadmap language. A future
  contributor can still restart in the wrong repository with the wrong product
  promise.
- **The new repository has no front door.** There is no top-level `README.md`
  (verified). A maintainer or reviewer has to read migration plans to learn the
  current product stance — selective surface, implemented≠recommended, advisory
  trackers, order runtime deferred.
- **The first recommendation has no contract.** `docs/reference/token.md` still
  says "Request/response schemas and verified examples are deferred until this TR
  reaches recommended status or a real consumer exists." `token` reached
  recommended status in PR #7. The page label is now true but the surface is
  thin: it does not tell a reader what is recommended, what evidence backs it, or
  what would revoke it.

These are one theme — legibility — and all three are low carrying cost. The doc
that produced them sequences them ahead of the heavier durability engines
(freshness enforcement, tracker→work-item workflow) precisely because legibility
should land before more recommended behavior is built on top of it.

## Key Decisions

- **The three items are one requirements unit, sequenced #1 → #2 → #3.** They
  share the legibility theme and are individually small. Item #1 lands in a
  *different repository* (`korea-broker-sdk-ls`) and is therefore its own change
  there; #2 and #3 land in this repository. Planning may split execution by repo,
  but the contract for all three lives here.

- **The `token` recommendation contract is expressed in metadata + the generator,
  not by hand-editing the generated page.** `docs/reference/token.md` is
  generated from `ls-metadata` via `make docs`. The contract surface is added by
  giving the generator the fields it needs (evidence, env level, freshness date,
  revocation conditions, non-claims) and a Recommended-TR template, so the page
  regenerates correctly rather than drifting from metadata.

- **Revocation conditions are stated as policy, not as enforced behavior.** The
  contract says what *would* stale or revoke `token`'s claim (a maintained-TR
  Structural API Shape change; the 90-day backstop). It must not imply code
  enforces this, because the freshness loop (#4) is deferred and PR #7
  deliberately established truthful "not enforced by code" wording. The contract
  inherits that truthfulness.

- **`token`'s claim stays narrow.** Paper OAuth token issuance is recommended
  based on current Focused Evidence (`metadata/evidence/token.yaml`, `env: paper`,
  `2026-06-16`). The contract must not imply broader auth semantics, production
  credential evidence, all OAuth edge cases, or stronger freshness automation than
  exists.

- **Neither orientation revives the old promise.** The closeout notice and the
  README both state the selective, evidence-based stance and explicitly do not
  re-assert the "365 typed TRs" full-generated-surface promise.

## Requirements

### Item 1 — migration closeout notice in `korea-broker-sdk-ls`

- R1. The old repository carries a prominent notice stating: `korea-adapter-sdk-ls`
  is the maintained SDK direction; the old generated all-TR surface is historical;
  new SDK behavior belongs in the maintained SDK, not the old generated
  architecture.
- R2. The notice states that old docs, runtime lessons, and specifications remain
  **Migration Source** reference material, and that the old repository's existing
  promises are not the future SDK contract.
- R3. The notice does not re-assert the active "365 typed TRs / full generated
  SDK" promise as a current commitment.

### Item 2 — top-level README orientation in `korea-adapter-sdk-ls`

- R4. A top-level `README.md` states the product stance without requiring the
  reader to open internal plans: a maintained Rust SDK for the LS Open API; the
  SDK surface is selective; **Implemented** does not mean **Recommended**.
- R5. The README states the current recommendation truth: `token` is currently the
  only Recommended TR.
- R6. The README states that the API Drift Tracker and Specification Document
  Tracker cover upstream API shape drift and example drift, and that the trackers
  are advisory — they do not mutate SDK code, metadata, docs, or baselines.
- R7. The README states that order runtime is deferred by design, and that
  `korea-broker-sdk-ls` is historical **Migration Source** material.
- R8. The README does not revive the "365 typed TRs" promise.

### Item 3 — `token` Recommended TR contract

- R9. A Recommended TR's reference page tells a reader six things: what behavior
  is recommended; what evidence backs the claim; what environment level the claim
  covers; what freshness date applies; what would stale or revoke the claim; and
  what the recommendation does not claim.
- R10. For `token`, the page states the narrow claim: Paper OAuth token issuance
  is recommended based on current **Focused Evidence**, citing the evidence record
  (`metadata/evidence/token.yaml`), `env: paper`, and the `last_reviewed` /
  evidence date.
- R11. The page states the revocation conditions as policy/intent (a maintained-TR
  Structural API Shape change or the 90-day backstop would stale the claim), and
  does not imply those conditions are enforced by code.
- R12. The page states what the recommendation does not claim: not broader auth
  semantics, not production credential evidence, not all OAuth edge cases, not
  stronger freshness automation than currently exists.
- R13. The contract surface is produced by metadata + the `make docs` generator
  (Recommended-TR template), so the page regenerates from the source of truth
  rather than being hand-edited; the stale "deferred until recommended status"
  line is removed for `token`.

## Acceptance Examples

- AE1. **Covers R1, R3.** **Given** a contributor lands on `korea-broker-sdk-ls`
  intending to add new SDK behavior, **when** they read the top of the repository,
  **then** they are told to work in `korea-adapter-sdk-ls` and are not led to
  treat the old generated all-TR surface as the future contract.
- AE2. **Covers R4, R5, R6, R7.** **Given** a reviewer opens `korea-adapter-sdk-ls`
  for the first time and reads only the README, **then** they can state the SDK is
  selective, that `token` is the only Recommended TR, that the trackers are
  advisory, and that order runtime is deferred — without opening `docs/plans/`.
- AE3. **Covers R9, R10, R12.** **Given** the regenerated `docs/reference/token.md`,
  **when** a reader reads it, **then** they learn Paper OAuth issuance is the
  recommended behavior, that paper Focused Evidence dated 2026-06-16 backs it, and
  that production/edge-case/broader-auth claims are explicitly excluded.
- AE4. **Covers R11.** **Given** the same page, **when** a reader looks for the
  revocation conditions, **then** they are described as the stated policy (a
  qualifying structural change or the 90-day backstop) without any claim that code
  currently enforces them.
- AE5. **Covers R13.** **Given** `make docs` is re-run after the change, **then**
  `token`'s page regenerates with the contract surface and no longer contains the
  "deferred until this TR reaches recommended status" line.

## Scope Boundaries

### Deferred for later

- Evidence freshness enforcement for `token` (#4) — the 90-day backstop and
  structural-change staling as live code, not policy text. This contract describes
  the intended behavior; it does not build it.
- Tracker Finding → **SDK Maintenance Work Item** workflow (#5).
- Provisional-baseline re-attestation (#6) and the standing baseline-quality bar
  (#7).
- The second Recommended TR (`t1102`, #8) and selective inventory expansion (#9).

### Outside this move's identity

- No change to SDK behavior, tracker logic, severity emission, or any baseline.
- No new `Severity::Evidence` emission and no automatic staling of Focused
  Evidence — those are #4, deferred.
- No revival of the full generated all-TR surface or the "365 typed TRs" promise
  in either repository.
- No upstream LS documentation mirror turned into SDK product docs.

## Outstanding Questions

### Deferred to planning

- Q1. Exact placement and format of the closeout notice in `korea-broker-sdk-ls`
  (top of its README, a dedicated `MIGRATION.md`, or a repository description) —
  and how that cross-repo change is committed and sequenced relative to #2/#3.
- Q2. Which metadata fields and generator template changes produce the
  Recommended-TR contract surface (R13), and whether the six contract elements
  (R9) are added generically for all Recommended TRs or scoped to `token` first.
- Q3. Whether the README's tracker/coverage figures (e.g. 365 upstream TR codes,
  the example-baseline slice) are restated or linked to avoid a second source of
  truth that can drift from `metadata/`.

## Dependencies / Assumptions

- Assumes write access to `korea-broker-sdk-ls` (a separate repository) to land
  item #1; its current "active full SDK" self-presentation is taken from the
  origin status doc, not verified from this working tree.
- Assumes `docs/reference/token.md` is generated by `make docs` from `ls-metadata`
  and that the generator can be extended with a Recommended-TR template.
- Assumes the existing `metadata/evidence/token.yaml` record and `token`'s
  `support.recommended: true` / `maintenance.last_reviewed: 2026-06-16` are the
  authoritative inputs the contract cites.
- Assumes there are no external SDK consumers in the relevant horizon, so all
  three documents are internal honesty/orientation artifacts rather than published
  guarantees.

## Sources

- `docs/plans/2026-06-16-009-post-pr7-migration-status-what.md` — origin status
  doc (improvements #1, #2, #3 and the recommended sequence).
- `docs/reference/token.md` — current thin/stale `token` reference page.
- `metadata/trs/token.yaml`, `metadata/evidence/token.yaml` — `token`'s recommended
  state, `last_reviewed` date, and the durable paper Focused Evidence record.
- `CONTEXT.md` — vocabulary (Maintained SDK Surface, Recommended TR, Migration
  Source, Focused Evidence, Change Tracker advisory invariant).
- Repository root — confirmed absence of `README.md`.
