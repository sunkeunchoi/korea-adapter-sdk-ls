---
title: "post-PR4 migration status and next WHAT"
type: status
date: 2026-06-16
origin: docs/plans/maintained-sdk-migration-plan.md
---

# Post-PR4 migration status and next WHAT

## Current State

The migration from `korea-broker-sdk-ls` to `korea-adapter-sdk-ls` has moved
past the first maintained SDK slice, past real API Drift staged review, and past
the first API Drift baseline truthfulness closeout.

`korea-adapter-sdk-ls` is the maintained SDK direction.

`korea-broker-sdk-ls` is a **Migration Source** only. It remains useful as
historical reference material, but it is not the source of truth for new SDK
behavior.

## What Is Done So Far

### Maintained SDK direction

- The new Rust workspace exists.
- The maintained SDK vocabulary is documented.
- The old generated all-TR surface is no longer the target architecture.
- SDK behavior is organized around **Dependency Class** and **Facet Metadata**.
- Ordinary verification is oriented around **Change-Scoped Gates**.
- Old certification vocabulary has been replaced by **Focused Evidence** and
  maintenance-oriented support states.

### Maintained SDK slice

- Six TRs are implemented in the maintained SDK surface:
  - `token`
  - `revoke`
  - `t1102`
  - `t8412`
  - `CSPAQ12200`
  - `S3_`
- One order TR is tracked but not implemented:
  - `CSPAT00601`
- No TR is marked recommended yet.
- Order runtime behavior remains intentionally deferred.

### Maintained metadata and docs

- Seven TRs are represented in maintained metadata.
- The metadata index exists as a routing summary.
- Per-TR dependency docs exist for all seven tracked TRs.
- SDK reference docs exist for the six implemented TRs.
- Docs drift checking is available and clean.

### Paper live smoke

- Paper live smoke exists as credential-gated operator evidence.
- Default live smoke covers Paper OAuth and one market-session quote.
- Separate operator smoke surfaces exist for chart, account, and WebSocket checks.
- Live smoke remains operator evidence, not default test coverage.

### PR #2

PR #2 is done.

It delivered:

- Maintained metadata documentation projection.
- SDK reference documentation generation.
- Docs drift checking.
- API Drift tracker skeleton.
- Fixture-backed tracker compatibility coverage.
- Support-aware finding vocabulary.
- Dry-run promotion reporting.

PR #2 did not deliver real upstream watching.

### PR #3

PR #3 is done.

It delivered:

- Real API Drift staged review.
- Rust-native LS public API inventory awareness.
- Full upstream TR code-set awareness.
- Maintained-only Structural API Shape baselines.
- Reviewed raw API Drift evidence.
- Provisional first baseline seed.
- Support-aware advisory findings.
- Tiered exit outcomes for clean, review-needed, and error states.
- New-TR discovery as a review-needed finding.
- Maintained-TR structural change detection.
- Description-only changes as informational findings.
- Metadata coverage summary.
- Same-block reorder detection.
- Co-occurring add/remove rename hint.
- Network-free ordinary verification.
- Opt-in API Drift review targets.
- Maintenance runbook coverage for operator-run API Drift checks.

The PR #3 baseline established:

- 365 upstream TR codes.
- 41 upstream API groups.
- 7 maintained Structural API Shape baselines.
- 6 implemented TRs.
- 1 tracked-only order TR.
- 358 upstream TRs outside maintained metadata.

### PR #4

PR #4 is implemented and merged.

It delivered:

- API Drift baseline truthfulness closeout.
- `token` Structural API Shape coverage for the `scope` field.
- A refreshed maintained API Drift baseline at normalizer version `2`.
- A clean self-diff for the committed API Drift baseline.
- Support-aware facts-outage handling before drift comparison.
- Maintained-TR facts degradation as an error outcome.
- Untracked-only facts degradation as a visible non-gating finding.
- Property-type mapping fallback as a visible whole-inventory degradation.
- A preserved provisional code-set seed marker.
- A documented governance stance for the provisional seed.
- Closure of accepted residuals R-1 and R-2.
- Explicit carry-forward of residuals R-3 and R-4.
- Network-free default verification kept clean.
- Operator-run API Drift review kept opt-in.

## Current Verification

Verified on 2026-06-16:

- `cargo test --workspace`
  - 237 passed
  - 4 ignored
- `make docs-check`
  - clean
- `api-drift check --staged crates/ls-trackers/baselines/api-drift`
  - exit `0`
  - no drift findings
  - coverage: 365 upstream, 7 metadata, 6 implemented, 1 tracked-only

## What Is Not Done Yet

- Specification Document Tracker is not implemented.
- No TR is marked recommended yet.
- Recommended TR focused evidence promotion is not done.
- Order runtime dispatch is not implemented.
- Order safety focused evidence is not complete.
- Mutating API Drift baseline promotion is not implemented.
- Full structural baselining of untracked TRs is not done.
- Tracker findings do not automatically mutate SDK code, metadata, docs, or
  baselines.
- The API Drift code-set seed remains visibly provisional.
- Duplicate same-code / same-group API Drift handling remains carried debt.
- Serde forward-compatibility hardening remains carried debt.
- The old repository itself has not been formally marked obsolete.

## Recommended Next Item

The next primary product item should be:

**PR #5: Specification Document Tracker.**

The expected WHAT:

- LS documentation changes become tracked maintenance signals.
- Documentation findings remain advisory.
- SDK docs stay generated from maintained SDK behavior and metadata.
- Upstream documentation is not mirrored directly as product documentation.
- Documentation drift can identify stale SDK examples, metadata, operations
  notes, and focused-evidence claims.
- Documentation findings can become SDK Maintenance Work Items after review.
- Ordinary repository verification remains network-free.
- Operator-run document review remains opt-in.

This is the right next item because PR #3 and PR #4 made API Drift trustworthy
enough for structural upstream API changes. The remaining upstream-change blind
spot is documentation drift.

## Secondary Closeout Item

A small migration closeout still remains:

**Mark `korea-broker-sdk-ls` obsolete / migration-source-only inside the old
repository itself.**

The expected WHAT:

- The old repository states that `korea-adapter-sdk-ls` is the maintained SDK
  direction.
- The old repository states that its generated all-TR surface is historical.
- The old repository directs new SDK behavior to the maintained SDK.
- Historical docs and runtime lessons remain available as migration reference.

This is separate from PR #5. It closes migration communication, but it does not
add new upstream-change tracking capability.

## Next Item Summary

Primary next item:

**Build the Specification Document Tracker.**

Secondary closeout:

**Mark the old repository as obsolete / migration-source-only inside
`korea-broker-sdk-ls`.**

## Success Definition For The Next Primary Item

PR #5 is successful when a maintainer can answer, from the new repository alone:

- What LS documentation changed since the last reviewed document state?
- Which documentation changes affect implemented or tracked SDK behavior?
- Which documentation changes affect examples, operations, metadata, or evidence
  claims?
- Which documentation changes are informational only?
- Which documentation changes need human review before any SDK maintenance work
  proceeds?

It is not successful merely because documents can be fetched. It is successful
when documentation changes become reviewed, staged, support-aware maintenance
signals.
