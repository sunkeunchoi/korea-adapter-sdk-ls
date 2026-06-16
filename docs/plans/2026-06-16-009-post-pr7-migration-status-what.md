---
title: "post-PR7 migration status and WHAT to improve"
type: status
date: 2026-06-16
origin: user-requested post-PR7 migration review
---

# Post-PR7 migration status and WHAT to improve

## Current State

The migration from `korea-broker-sdk-ls` to `korea-adapter-sdk-ls` has moved
past the initial maintained SDK slice, upstream change tracking, documentation
change tracking, and the first **Recommended TR** promotion.

`korea-adapter-sdk-ls` is now the maintained SDK direction.
`korea-broker-sdk-ls` is a **Migration Source** only, but that is still mostly a
new-repository claim: the old repository still presents itself as an active
full generated SDK.

PR #7 implemented the highest-value recommendation from the previous WHAT
review: prove the Implemented-to-Recommended lifecycle on one narrow TR
(`token`). That changes the next question. The next improvement is no longer
"create the first Recommended TR." The next improvement is making the
recommendation contract, evidence freshness, migration ownership, and reviewed
maintenance workflow strong enough to carry more recommended behavior.

## What PR #1-#5 Established

### Maintained SDK direction

- The Rust workspace exists.
- The **Maintained SDK Surface** is selective, not a rebuilt full generated
  compatibility surface.
- SDK behavior is organized by **Dependency Class** and routed by
  **Facet Metadata**.
- Ordinary verification is oriented around **Change-Scoped Gates**.
- Old generated-surface and certification vocabulary has been replaced by
  **Focused Evidence**, **Tracker Finding**, **Reviewed Baseline**, and
  **SDK Maintenance Work Item** vocabulary.
- The old repository is treated here as a **Migration Source**, not a permanent
  dependency.

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
- Order runtime dispatch remains intentionally deferred.

### Maintained metadata and generated docs

- Seven TRs are represented in **TR Maintenance Metadata**.
- The metadata index exists as a routing summary, not the source of truth.
- TR Dependency Docs exist for all seven tracked TRs.
- SDK Reference Docs exist for the six implemented TRs.
- Docs drift checking exists.

### Paper live smoke

- Paper credentials and real credentials are distinct environment concepts.
- **Paper Live Smoke** exists as credentialed operator evidence.
- Default live smoke covers Paper OAuth plus one quote path.
- Separate operator smoke surfaces exist for chart, account, and WebSocket
  behavior.
- Live smoke remains opt-in operator evidence, not default test coverage.

### API Drift Tracker

- The API Drift Tracker can capture the upstream LS API inventory.
- The current reviewed raw inventory covers 365 upstream TR codes across 41
  groups.
- Maintained **Structural API Shape** baselines exist for the seven tracked TRs.
- The API Drift baseline is at normalizer version `2`.
- Structural changes produce support-aware advisory findings.
- New upstream TR discovery is visible.
- Maintained-TR structural changes gate review.
- Description and `korean_name` changes are informational findings.
- API Drift review remains operator-run and opt-in.
- The code-set seed remains visibly provisional.

### Specification Document Tracker

- Request/response example drift is tracked as the documentation facet not
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

## What PR #7 Changed

PR #7 promoted `token` to the first **Recommended TR**.

It established:

- `token` is now tracked, implemented, and recommended.
- `token` has a committed, credential-free **Focused Evidence** record.
- `token`'s `maintenance.last_reviewed` date is anchored to that evidence date.
- Generated reference and dependency docs show `token` as recommended.
- The stale claim that change-driven invalidation is inactive because the
  Specification Document Tracker does not exist has been removed.
- The freshness policy now states the truth: the trackers exist and see changes,
  but evidence invalidation and the 90-day backstop are not enforced by code.

This is the right first recommendation because `token` is the narrowest claim:
standalone, OAuth-only, paper-compatible, no market session, no account state,
no caller-supplied identifier, no pagination, no WebSocket lifecycle, and no
order risk.

## What PR #7 Did Not Change

PR #7 did not make the recommendation model complete.

- No code computes the 90-day evidence backstop.
- No code emits `Severity::Evidence`.
- No tracker automatically stales **Focused Evidence**.
- Specification Document findings remain advisory and do not stale evidence.
- The `token` evidence record is linked by convention, not by a schema field.
- The SDK Reference page for `token` still gives only a thin recommendation
  surface; it does not yet carry a richer user-facing contract for what the
  recommendation means.
- The API Drift and Specification Document seeds remain provisional.
- Tracker findings still do not have a concrete reviewed path into
  **SDK Maintenance Work Items**.
- The old repository still reads like the active full generated SDK.

That is acceptable for a first promotion. It is not acceptable as the long-term
shape for recommending broader or riskier SDK behavior.

## What Needs Improvement

### 1. Close the migration ownership gap

The new repository says `korea-broker-sdk-ls` is a **Migration Source** only, but
the old repository still presents itself as `ls-sdk`, an active full SDK with
365 typed TRs, generated low-level APIs, convenience helpers, order safety, and
release-roadmap language.

Improve the WHAT by making the old repository explicitly say:

- `korea-adapter-sdk-ls` is the maintained SDK direction.
- The old generated all-TR surface is historical.
- New SDK behavior belongs in the maintained SDK, not the old generated
  architecture.
- Old docs, runtime lessons, and specifications remain migration reference
  material.
- The old repository's existing promises should not be read as the future SDK
  contract.

This should stay near the top of the queue. Without it, future work can still
restart in the wrong repository with the wrong product promise.

### 2. Add a public orientation for the new repository

The new repository has strong internal planning docs, but no top-level README.
That means a maintainer or reviewer has to read migration plans to understand
the current product stance.

Improve the WHAT by adding a concise top-level orientation that states:

- This is a maintained Rust SDK for LS Open API.
- The SDK surface is selective.
- Implemented does not mean recommended.
- `token` is currently the only Recommended TR.
- Trackers cover upstream API shape drift and example drift.
- Trackers are advisory and do not mutate SDK code, metadata, docs, or baselines.
- Order runtime is deferred by design.
- `korea-broker-sdk-ls` is historical migration source material.

This should not revive the old "365 typed TRs" promise.

### 3. Make the `token` recommendation contract more useful

PR #7 made the `token` recommendation label true, but the user-facing reference
surface is still thin. The generated `token` reference page says request/response
schemas and verified examples are deferred until the TR reaches recommended
status or a real consumer exists. `token` has now reached recommended status.

Improve the WHAT by defining what a Recommended TR page must tell a reader:

- What behavior is recommended.
- What evidence backs the claim.
- What environment level the claim covers.
- What freshness date applies.
- What would stale or revoke the claim.
- What the recommendation does not claim.

For `token`, the claim should stay narrow: Paper OAuth token issuance is
recommended based on current Focused Evidence. It should not imply broader auth
semantics, production credential evidence, all OAuth edge cases, or stronger
freshness automation than exists.

### 4. Make evidence freshness operative for the first Recommended TR

The current freshness policy is truthful, but it is still mostly intent. That is
better than a false active claim, but the project now has one Recommended TR and
therefore a real subject for freshness enforcement.

Improve the WHAT by making evidence freshness real for `token` before promoting
many more TRs:

- The 90-day backstop should produce a visible evidence-refresh obligation.
- A maintained-TR Structural API Shape change should stale the affected
  Recommended TR's Focused Evidence.
- Description-only changes should remain informational.
- Specification Document example findings should remain advisory review
  obligations unless a separate decision changes that invariant.
- The Recommended claim should have a clear valid/stale state.

Recommended answer: start with the 90-day backstop and the `token` evidence
record, because that is the smallest meaningful revocation loop. Then connect
Structural API Shape findings to evidence staleness.

### 5. Define the Tracker Finding to SDK Maintenance Work Item workflow

Both trackers now emit findings, but the human review path is still mostly
conceptual.

Improve the WHAT by defining:

- What makes a **Tracker Finding** become an **SDK Maintenance Work Item**.
- Who accepts, rejects, or defers a finding.
- Where accepted work items are recorded.
- How baseline promotion is reviewed.
- How rejected or informational findings remain visible without becoming noise.
- How a Recommended TR's evidence state changes when a finding is accepted.

The tracker should still not mutate SDK code, metadata, docs, evidence, or
baselines automatically.

### 6. Re-attest the provisional baselines

The API Drift code-set seed and the Specification Document example seed remain
visibly provisional. That was a good migration-bootstrap posture. It should not
become permanent by accident.

Improve the WHAT by deciding:

- What evidence clears `provisional: true`.
- Whether exact parity with the old migration source is enough.
- Whether a second live operator fetch is required.
- How future upstream additions are admitted.
- Whether provisional status limits future Recommended TR promotion.
- Whether full-inventory example coverage must be re-attested separately from
  API code-set coverage.

Recommended answer: provisional is acceptable while proving the maintained model,
but any expansion beyond a small number of Recommended TRs should require a clear
baseline attestation policy.

### 7. Treat PR #6's baseline lesson as a standing product bar

PR #6 did not add a new SDK surface, but it captured a critical change-tracker
quality bar: a tracker baseline must self-diff clean, avoid secret-bearing
stored values by construction, and remain safe across filesystem casing rules.

Improve the WHAT by making that a standing requirement for every future
**Reviewed Baseline**:

- A baseline is not accepted unless it cleanly self-diffs.
- A baseline derived from examples or credentials stores structural descriptors,
  not raw values.
- A full-inventory baseline must not use a layout that loses case-colliding TR
  codes.
- Baseline quality is part of the product contract, not just a one-off
  implementation lesson.

This matters before adding more trackers, more full-inventory projections, or
more evidence records.

### 8. Promote a second Recommended TR only after `token` proves revocation

After PR #7, the next recommendation should not be a batch promotion. The next
recommendation should test a different dependency class.

Recommended answer: promote `t1102` second, after the `token` freshness loop is
credible.

Why `t1102`:

- It is read-only.
- It is user-visible.
- It exercises market-session behavior.
- It is more meaningful than a second auth-only recommendation.
- It does not carry account-state, pagination, WebSocket lifecycle, or order
  risk.

The promotion should answer:

- What focused evidence is sufficient for a market-session quote.
- Whether Paper evidence is enough.
- What venue/session assumptions the claim depends on.
- Which tracker findings stale the claim.
- What user-facing statement becomes true.

Do not promote all six implemented TRs together.

### 9. Expand maintained inventory only after the evidence loop is closed

The old repository covered all 365 typed TRs. The new repository intentionally
does not. That is the core migration trade-off and should remain explicit.

Improve the WHAT by defining admission criteria for the next tracked or
implemented TRs:

- Which user workflow does the TR unlock.
- Which **Dependency Class** owns it.
- Which **Facet Metadata** routes evidence and docs.
- Whether it should be tracked-only, implemented, or recommended.
- What Focused Evidence would eventually make it recommendable.
- Whether its upstream changes are already visible through the trackers.

Candidate expansion areas should stay workflow-led, not inventory-led:

- Quote and instrument discovery.
- Account inquiry expansion.
- Pagination-heavy market data.
- Realtime quote/event expansion.
- Order safety package only when ready.

### 10. Keep order runtime deferred until it can ship as a complete safety package

The order safety design is written and `CSPAT00601` is tracked. No order runtime
ships today. That remains the right posture.

Improve the WHAT only when the order package can include:

- No-retry order dispatch.
- Duplicate-order prevention.
- Ambiguous-outcome reconciliation.
- Guarded manual evidence.
- A clear recommended/not-recommended stance for order behavior.

Do not implement order placement merely to increase coverage. Order behavior is
the highest-risk SDK boundary and should move only as a complete safety package.

## Recommended Next Sequence

1. **Migration closeout notice in `korea-broker-sdk-ls`.**
   Stop ambiguity about which repository owns future SDK behavior.

2. **Top-level orientation in `korea-adapter-sdk-ls`.**
   Make the maintained, selective, evidence-based SDK contract visible without
   reading internal plans.

3. **Recommended TR contract for `token`.**
   Make the first Recommended TR's page and evidence statement useful enough for
   a future maintainer or user to understand the claim.

4. **Evidence freshness revocation loop.**
   Give the first Recommended TR an operative stale/fresh state, starting with
   the 90-day backstop and then maintained-TR structural-change staling.

5. **Tracker Finding to SDK Maintenance Work Item workflow.**
   Define the human review path from advisory findings to accepted maintenance
   work.

6. **Baseline re-attestation policy.**
   Decide when the provisional API Drift and Specification Document seeds become
   fully accepted reviewed baselines.

7. **Second Recommended TR.**
   Promote `t1102` only after `token` proves the recommendation and freshness
   lifecycle.

8. **Selective SDK expansion.**
   Add new maintained TRs by workflow need, not by trying to recreate the old
   full generated surface.

## Non-Goals For The Next Step

- Do not rebuild the old full generated all-TR surface.
- Do not make tracker output mutate SDK code, metadata, docs, evidence, or
  baselines automatically.
- Do not batch-promote implemented TRs.
- Do not claim evidence freshness is enforced until it is actually enforced.
- Do not ship order runtime before the full order safety package is ready.
- Do not make upstream LS documentation mirrors into SDK product docs.

## Bottom Line

PR #1-#5 moved the project from generated-surface migration into a maintained SDK
operating model. PR #7 proved the first narrow recommendation. The next
improvement is to make that recommendation model durable: visible repository
ownership, useful recommended-surface docs, operative evidence freshness,
reviewed tracker-to-work workflow, and non-provisional baseline governance.

