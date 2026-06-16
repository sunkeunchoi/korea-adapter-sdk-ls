---
title: "post-PR5 migration status and WHAT to improve"
type: status
date: 2026-06-16
origin: user-requested post-PR5 migration review
---

# Post-PR5 migration status and WHAT to improve

## Current State

The migration from `korea-broker-sdk-ls` to `korea-adapter-sdk-ls` has crossed
the important architectural boundary: the new repository now owns a maintained
Rust SDK surface, maintained metadata, generated SDK/reference documentation,
paper live-smoke evidence, API Drift tracking, and Specification Document
tracking.

The old repository is already defined here as a **Migration Source** only. It is
still useful historical material, but it is not the source of truth for new SDK
behavior.

## What PR #1-#5 Have Established

### Maintained SDK direction

- The new Rust workspace exists.
- `korea-adapter-sdk-ls` is the maintained SDK direction.
- The old generated all-TR surface is no longer the target architecture.
- The **Maintained SDK Surface** is selective, not a full generated compatibility
  promise.
- SDK behavior is organized by **Dependency Class** and routed by **Facet
  Metadata**.
- Ordinary verification is oriented around **Change-Scoped Gates**, not a broad
  release baseline for every change.
- Old generated-surface and certification vocabulary has been replaced by
  **Focused Evidence**, **Tracker Finding**, **Reviewed Baseline**, and
  **SDK Maintenance Work Item** vocabulary.

### Maintained SDK slice

- Six TRs are implemented:
  - `token`
  - `revoke`
  - `t1102`
  - `t8412`
  - `CSPAQ12200`
  - `S3_`
- One order TR is tracked but not implemented:
  - `CSPAT00601`
- No TR is recommended yet.
- Order runtime dispatch remains intentionally deferred.

### Maintained metadata and generated docs

- Seven TRs are represented in **TR Maintenance Metadata**.
- The metadata index exists as a routing summary, not the source of truth.
- TR Dependency Docs exist for all seven tracked TRs.
- SDK Reference Docs exist for the six implemented TRs.
- Docs drift checking exists.

### Paper live smoke

- Paper credentials and real credentials are distinct environment concepts.
- Paper Live Smoke exists as credentialed operator evidence.
- Default live smoke covers Paper OAuth plus one quote path.
- Separate operator smoke surfaces exist for chart, account, and WebSocket
  behavior.
- Live smoke remains opt-in operator evidence, not default test coverage.

### API Drift Tracker

- The API Drift Tracker can capture the upstream LS API inventory.
- The current reviewed raw inventory covers 365 upstream TR codes across 41
  groups.
- Maintained Structural API Shape baselines exist for the seven tracked TRs.
- The API Drift baseline is at normalizer version `2`.
- Structural changes produce support-aware advisory findings.
- New upstream TR discovery is visible.
- Maintained-TR structural changes gate review.
- Description and `korean_name` changes are informational findings.
- API Drift review remains operator-run and opt-in.
- The code-set seed remains visibly provisional.

### Specification Document Tracker

- Request/response example drift is now tracked as the documentation facet not
  covered by Structural API Shape.
- The example baseline covers the full upstream inventory slice carrying
  examples: 355 of 365 upstream TRs.
- The example baseline has its own normalizer version, currently `1`.
- Example findings are advisory and never gate ordinary verification.
- For tracked TRs, example findings point at maintained artifacts that should be
  reviewed.
- The tracker stores structural descriptors only, not raw examples or secrets.
- Specification Document review remains operator-run and network-free, reusing
  the shared raw API Drift snapshot.

## What Needs Improvement

### 1. Close the migration communication gap

The new repository says `korea-broker-sdk-ls` is a **Migration Source** only, but
the old repository still presents itself as the active full generated SDK. That
creates two sources of direction for future work.

Improve the WHAT by making the old repository explicitly say:

- `korea-adapter-sdk-ls` is the maintained SDK direction.
- The old generated all-TR surface is historical.
- New SDK behavior belongs in the maintained SDK, not the old generated
  architecture.
- Old docs, runtime lessons, and specifications remain available as migration
  reference.

This should be the next smallest improvement because it prevents new work from
continuing in the wrong repository.

### 2. Refresh the evidence-freshness policy now that PR #5 exists

`metadata/EVIDENCE-FRESHNESS.md` and the metadata index still say
change-driven invalidation is inactive because the Specification Document
Tracker does not exist yet. That was true before PR #5 and is now stale.

Improve the WHAT by defining the post-PR5 evidence policy:

- What change-driven invalidation means now that API Drift and example drift
  both exist.
- Which findings can stale **Focused Evidence**.
- Whether informational example findings can ever affect evidence freshness.
- How the 90-day backstop and change-driven invalidation combine.
- What remains intentionally inactive while there are zero **Recommended TRs**.

This is a domain-policy update, not merely a wording cleanup.

### 3. Define the first Recommended TR promotion

The maintained SDK has implemented behavior but no **Recommended TR**. That
keeps the product honest, but it also leaves users without any explicitly
recommended surface.

Improve the WHAT by choosing the first promotion candidate and its evidence bar.
The most conservative candidate is `token` or `revoke`; the most user-visible
read-only candidate is `t1102`.

The promotion should answer:

- Which implemented TR becomes recommended first?
- What focused evidence is sufficient for that claim?
- How fresh must that evidence stay?
- Which tracker findings stale the claim?
- What user-facing statement becomes true after promotion?

Do not promote all six implemented TRs together. The first promotion should prove
the recommendation model with one narrow claim.

### 4. Decide whether the next product move is recommendation depth or SDK breadth

After PR #5, the project has enough tracker machinery to maintain selective
coverage. The next product decision is no longer "build another tracker"; it is
whether to deepen confidence in the current slice or expand the maintained
surface.

Recommended answer: deepen first.

Improve the WHAT by making one narrow implemented TR recommended before adding a
large set of new TRs. That validates the evidence model and prevents the new
repo from recreating the old problem: broad callability without clear
recommendation quality.

### 5. Keep order runtime deferred until it can ship as a complete safety package

The order safety design is written, and `CSPAT00601` is tracked, but no order
runtime ships. That is the right posture today.

Improve the WHAT only when the order package can include:

- No-retry order dispatch.
- Duplicate-order prevention.
- Ambiguous-outcome reconciliation.
- Guarded manual evidence.
- A clear recommended/not-recommended stance for order behavior.

Do not implement order placement merely to increase coverage. Order behavior is
the highest-risk SDK boundary and should move only as a complete safety package.

### 6. Promote tracker findings into maintenance work without automating SDK mutation

Both trackers now emit findings, but the review-to-work-item path is still mostly
a concept.

Improve the WHAT by defining the human workflow:

- What makes a **Tracker Finding** become an **SDK Maintenance Work Item**?
- Who accepts or rejects a finding?
- Where accepted work items are recorded?
- How baseline promotion is reviewed?
- How rejected or informational findings remain visible without becoming noise?

The tracker should still not mutate SDK code, metadata, docs, or baselines
automatically.

### 7. Re-attest the provisional baselines

Both the API Drift code-set seed and the Specification Document example seed
remain visibly provisional.

Improve the WHAT by deciding the attestation policy:

- What evidence clears `provisional: true`?
- Is exact parity with the old migration source enough?
- Is a second live operator fetch required?
- How are future upstream additions admitted?
- When does provisional status become unacceptable for recommendation claims?

The current provisional stance is acceptable for migration bootstrap. It should
not silently become permanent.

### 8. Expand maintained inventory only after the evidence loop is closed

The old repository covered all 365 typed TRs; the new maintained SDK intentionally
does not. That is the core migration trade-off.

Improve the WHAT by defining admission criteria for the next tracked or
implemented TRs:

- Which user workflow does the TR unlock?
- Which dependency class owns it?
- Which facets route evidence and docs?
- Is it tracked-only, implemented, or recommended?
- What focused evidence would eventually make it recommendable?

Candidate expansion areas should be workflow-led, not inventory-led:

- Quote/instrument discovery expansion.
- Account inquiry expansion.
- Pagination-heavy market data expansion.
- Realtime quote/event expansion.
- Order safety package, only when ready.

### 9. Add a new-repo public orientation document

The new repository currently has strong internal planning docs, but no top-level
README-style product orientation. That makes the repo harder to evaluate without
reading migration plans.

Improve the WHAT by adding a top-level public orientation that states:

- This is a maintained Rust SDK for LS Open API.
- The SDK surface is selective.
- Trackers cover upstream API and example drift.
- Implemented does not mean recommended.
- Order runtime is deferred by design.
- The old repository is historical migration source material.

This should not revive the old "365 typed TRs" promise.

## Recommended Next Sequence

1. **Migration closeout notice in `korea-broker-sdk-ls`.**
   Stop ambiguity about which repo owns future SDK behavior.

2. **Post-PR5 evidence-policy refresh.**
   Update the project stance now that both API Drift and Specification Document
   tracking exist.

3. **First Recommended TR promotion.**
   Pick one narrow implemented TR and prove the recommendation lifecycle.

4. **Tracker-finding to maintenance-work-item workflow.**
   Define how findings become reviewed work without automatic mutation.

5. **Selective SDK expansion.**
   Add new maintained TRs only after the recommendation and evidence loop is
   proven.

## Non-Goals For The Next Step

- Do not rebuild the old full generated all-TR surface.
- Do not make tracker output mutate SDK code or metadata automatically.
- Do not promote every implemented TR to recommended as a batch.
- Do not ship order runtime before the full order safety package is ready.
- Do not make upstream LS documentation mirrors into SDK product docs.

## Bottom Line

PR #1-#5 successfully moved the project from generated-surface migration into a
maintained-SDK operating model. The next improvement is not another tracker. The
next improvement is tightening the product contract around migration ownership,
evidence freshness, recommendation, and reviewed maintenance work.
