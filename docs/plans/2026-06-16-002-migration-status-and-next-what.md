---
title: "migration status and next WHAT"
type: status
date: 2026-06-16
origin: docs/plans/maintained-sdk-migration-plan.md
---

# Migration status and next WHAT

## Current State

The migration from `korea-broker-sdk-ls` to `korea-adapter-sdk-ls` has moved
from architecture setup into maintained-SDK operation.

The old repository is now a **Migration Source**: useful for reference material,
runtime lessons, endpoint knowledge, and historical drift-review behavior, but
not the source of truth for the new SDK.

The new repository owns the maintained Rust SDK surface, metadata, generated
maintainer docs, live-smoke harness, and the first staged-snapshot tracker
skeleton.

## What Is Done So Far

### Maintained SDK foundation

- The maintained Rust workspace exists.
- Core LS runtime behavior has been ported into the new maintained architecture.
- The SDK exposes a selective maintained surface instead of the old generated
  all-TR surface.
- Dependency classes are the ownership model for SDK behavior.
- Old generated-surface and certification vocabulary has been replaced with the
  new maintenance vocabulary.

### Representative SDK slice

- Standalone OAuth behavior is represented by `token` and `revoke`.
- Market-session REST behavior is represented by `t1102`.
- Paginated REST behavior is represented by `t8412`.
- Account-state REST behavior is represented by `CSPAQ12200`.
- Realtime WebSocket behavior is represented by `S3_`.
- Order behavior is represented as metadata and safety design only, not runtime
  order dispatch.

### Maintained metadata

- Seven TRs are tracked in maintained metadata.
- Six tracked TRs are implemented.
- One order TR is tracked but not implemented.
- No tracked TR is recommended yet.
- Metadata records owner class, protocol, facets, support state, dependency
  fields, review hash, and review date.
- The metadata index exists as a routing summary, not as the source of truth.

### Change-scoped verification

- The project has a metadata-driven concept of which tests matter for a changed
  TR or dependency class.
- Default maintenance is oriented around **Change-Scoped Gates**, not a broad
  release-style baseline for every change.
- Focused Evidence exists as the vocabulary for targeted proof, but broad
  evidence promotion remains conservative.

### Paper Live Smoke

- The SDK uses two environment concepts: Paper and Real.
- Paper Live Smoke exists as credential-gated operator evidence.
- Default live smoke proves Paper OAuth plus one market-session quote.
- Separate operator targets exist for chart, account, and WebSocket smoke.
- Paper Live Smoke is explicit operator evidence, not default test coverage.

### PR #2: metadata docs and tracker skeleton

PR #2 is done.

It delivered the first maintained documentation projection:

- TR Dependency Docs for all seven tracked TRs.
- SDK Reference Docs for the six implemented TRs.
- A docs drift check so committed docs can be compared with maintained metadata.
- Clear caveats that implemented TRs are not yet recommended.

It also delivered the staged-snapshot tracker skeleton:

- Tracker vocabulary and advisory finding types.
- The five tracker stages as a product contract: fetch, normalize, diff,
  classify, promote.
- A fixture-backed API Drift worked example.
- Support-aware finding classification over fixture changes.
- A dry-run promotion report that writes nothing.

The PR #2 tracker is intentionally not a real upstream watcher yet. Its fetch
stage is still stubbed, and its sample-payload fixture model is compatibility
coverage rather than the real reviewed API Drift baseline model.

## What Is Not Done Yet

- The API Drift Tracker does not yet fetch the live LS public API inventory.
- There is no committed reviewed raw API Drift baseline.
- There is no committed normalized Structural API Shape baseline.
- The tracker does not yet compare live staged upstream inventory against
  reviewed baselines.
- Metadata coverage reporting against the full upstream inventory is not yet a
  real operator report.
- The Specification Document Tracker is not implemented.
- No tracker finding automatically becomes SDK maintenance work.
- No tracker output mutates SDK code, metadata, generated docs, or baselines.
- Recommended TR promotion is not done.
- Order runtime behavior remains intentionally deferred.
- The old repository has not yet been formally marked obsolete from the new
  repository's point of view.

## Recommended Next Item

The next primary product item should be **PR #3: real API Drift staged review**.

The outcome should be:

- The tracker can capture the full upstream LS API inventory.
- Captured upstream state is staged for review before any committed baseline
  changes.
- Reviewed baselines exist for real API Drift comparison.
- Real API Drift comparison uses **Structural API Shape**, not PR #2
  sample-payload leaf paths.
- Structural changes produce support-aware advisory findings.
- Same-block field reorder is distinguishable from field removal and addition.
- Metadata coverage is visible to maintainers without automatically creating new
  metadata.
- Fetch or parse incompleteness is treated as an error, not ordinary drift.
- Ordinary repository verification remains network-free.
- Baseline seeding is explicitly human-reviewed because it requires a live LS
  network fetch and maintainer acceptance.

This item is the smallest meaningful step from "tracker skeleton" to "upstream
change watcher."

## Migration Closeout Item

There is also a small migration closeout item: mark `korea-broker-sdk-ls` as
obsolete or migration-source-only from the new project perspective.

That closeout should state:

- `korea-adapter-sdk-ls` is the maintained SDK direction.
- `korea-broker-sdk-ls` remains useful historical reference material.
- The old generated all-TR surface is not the future source of truth.
- New SDK behavior should be added to the maintained SDK, not to the generated
  predecessor architecture.

This can be a small separate documentation change. It does not replace PR #3,
because it does not give the maintained SDK real upstream drift visibility.

## Next Item Summary

Primary next item:

**Build real API Drift staged review.**

Secondary closeout:

**Mark the old repository as migration-source-only / obsolete for new work.**

## Success Definition For The Next Primary Item

The next primary item is successful when a maintainer can answer, from the new
repository alone:

- What does LS publish upstream now?
- What reviewed upstream API state did we previously accept?
- What changed structurally since that reviewed state?
- Which changes matter for implemented or tracked SDK behavior?
- Which upstream TRs are outside maintained metadata?
- What needs human review before any baseline or SDK maintenance work proceeds?

It is not successful merely because the tracker can fetch data. It is successful
when fetched upstream data becomes a reviewed, staged, support-aware maintenance
signal.
