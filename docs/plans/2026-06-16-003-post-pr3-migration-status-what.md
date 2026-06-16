---
title: "post-PR3 migration status and next WHAT"
type: status
date: 2026-06-16
origin: docs/plans/maintained-sdk-migration-plan.md
---

# Post-PR3 migration status and next WHAT

## Current State

The migration from `korea-broker-sdk-ls` to `korea-adapter-sdk-ls` has moved
past the first maintained SDK slice and past the first real upstream API Drift
watcher.

`korea-broker-sdk-ls` is now a **Migration Source** only. It remains useful as
historical evidence, but the maintained SDK direction is `korea-adapter-sdk-ls`.

## What Is Done So Far

### Maintained SDK direction

- The new Rust workspace exists.
- The maintained SDK vocabulary is documented in `CONTEXT.md`.
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
- Docs drift checking is available and currently clean.

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

PR #3 is implemented and merged.

It delivered:

- Real API Drift staged review.
- Rust-native LS public API inventory fetching.
- Full upstream TR code-set awareness.
- Maintained-only Structural API Shape baselines.
- Reviewed raw API Drift evidence.
- Provisional first baseline seed.
- Support-aware advisory findings.
- Tiered exit contract:
  - `0`: no gating finding
  - `1`: review-needed finding
  - `2`: fetch, parse, baseline, staged-run, or internal error
- New-TR discovery as a review-needed finding.
- Maintained-TR structural change detection.
- Description-only changes as informational findings.
- Metadata coverage summary.
- Same-block reorder detection.
- Co-occurring add/remove rename hint.
- Network-free ordinary verification.
- Opt-in Makefile targets for API Drift review.
- Maintenance runbook entry for operator-run API Drift checks.

The committed PR #3 baseline records:

- 365 upstream TR codes.
- 41 upstream API groups.
- 7 maintained Structural API Shape baselines.
- 6 implemented TRs.
- 1 tracked-only order TR.
- 358 upstream TRs outside maintained metadata.

## PR #3 Test Status

PR #3 is tested for the network-free acceptance surface.

Verified on 2026-06-16:

- `cargo test --workspace`
  - 225 passed
  - 4 ignored
- `make docs-check`
  - clean
- `api-drift check --staged crates/ls-trackers/baselines/api-drift`
  - exit `0`
  - no drift findings
  - coverage: 365 upstream, 7 metadata, 6 implemented, 1 tracked-only

Acceptance coverage present:

- Staged-run artifact writing.
- Clean staged comparison.
- Implemented-TR field removal as breaking drift.
- New untracked TR discovery as review-needed drift.
- Known untracked TR changes as report-only.
- Metadata coverage summary independence.
- Menu parse failure as error.
- Single baselined-TR absence as drift, not fetch failure.
- Description-only change as informational.
- Same-block reorder handling.
- Duplicate-name reorder fallback.
- Rename hint on co-occurring add/remove.
- CLI parsing and tiered exit mapping.
- Baseline/staged-run loading errors.
- Normalizer-version mismatch guard.

## Is PR #3 Fully Tested?

No, not in the absolute sense.

PR #3 is sufficiently tested for the committed network-free acceptance surface,
but it is not fully closed as a long-term maintained watcher.

Remaining test and evidence gaps:

- The live `api-drift check` path is operator-run and was not rerun in this
  verification pass.
- The first code-set seed is still marked provisional.
- Independent inventory attestation is still pending.
- The `token` `scope` field is missing from current Structural API Shape
  coverage.
- A whole-inventory endpoint/rate facts outage is warned at fetch time but does
  not currently fail the gate.
- Some serde forward-compatibility hardening is deferred.
- Duplicate same-code/same-group edge handling is deferred.
- Credentialed live smoke tests remain ignored by default.
- PR #3 does not cover the Specification Document Tracker.
- PR #3 does not promote any TR to recommended.
- PR #3 does not implement order runtime behavior.

## Next Item

The next item should be:

**PR #4: API Drift hardening and baseline attestation closeout.**

The expected WHAT:

- Structural API Shape coverage includes the missing `token` `scope` field.
- The committed API Drift baseline is refreshed after the structural coverage
  correction.
- The provisional code-set seed receives independent operator attestation, or
  the remaining attestation gap is kept explicitly visible.
- Whole-inventory facts degradation has a final maintenance policy.
- Accepted PR #3 residuals are either closed or deliberately carried as named
  follow-up debt.
- Network-free verification remains clean.
- Operator-run API Drift review remains opt-in.

This is the smallest next item because it finishes PR #3's maintenance quality
before expanding tracker scope.

## Following Item

After PR #4, the next product expansion should be:

**Specification Document Tracker.**

The expected WHAT:

- LS documentation changes become tracked maintenance signals.
- Documentation findings remain advisory.
- SDK docs are not replaced by upstream documentation mirrors.
- Specification findings can become SDK Maintenance Work Items after review.

## Still Not Done In The Migration

- Recommended TR promotion.
- Order runtime dispatch.
- Order safety focused evidence.
- Mutating API Drift baseline promotion.
- Specification Document Tracker.
- Full structural baselining of untracked TRs.
- Automatic SDK or metadata mutation from tracker output.
- Clearing the provisional API Drift code-set seed.
- Formal obsolete notice inside the old repository itself.

