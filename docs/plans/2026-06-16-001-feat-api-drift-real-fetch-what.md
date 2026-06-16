---
title: "feat: API Drift real fetch WHAT scope"
type: requirements
date: 2026-06-16
origin: docs/plans/2026-06-15-004-feat-api-drift-real-fetch-plan.md
---

# feat: API Drift real fetch WHAT scope

## Purpose

Build the first real API Drift Tracker capability for LS Open API: an opt-in
operator tool that captures the upstream API inventory, stages it for review,
normalizes it into reviewed structural facts, compares staged facts with an
accepted baseline, and reports advisory findings.

This document describes what the product must do. It intentionally avoids
implementation mechanics such as crate boundaries, parsing libraries, endpoint
selectors, data-structure names, and function-level design.

## Product Outcome

The repository gains an API Drift Tracker that can answer:

- What LS TRs exist upstream now?
- Which upstream TRs are already represented in maintained metadata?
- Which reviewed upstream structural facts changed since the last accepted
  baseline?
- Which changes matter more because the affected TR is implemented,
  recommended, tracked-only, or absent from metadata?
- What baseline, metadata, or documentation review would be needed next, without
  automatically mutating SDK behavior?

The tracker is advisory. It does not generate SDK code, rewrite metadata, rewrite
documentation, or promote baselines without review.

## Resolved Scope Decisions

### PR #2 sample-payload model

The PR #2 sample-payload leaf-path fixture model remains a walking-skeleton
compatibility artifact. It proves the shared tracker vocabulary and pipeline
shape over checked-in sample payload fixtures.

Real API Drift baselines must use **Structural API Shape**, not sample payload
leaf paths. Structural API Shape is the only accepted artifact model for
reviewed real-fetch API Drift comparison.

The new work must not silently reinterpret existing PR #2 fixtures as real API
baselines. Existing PR #2 fixtures and tests may continue to exist as
compatibility coverage, but real-fetch API Drift findings must come from
Structural API Shape artifacts.

### Field identity and reorder detection

Structural API Shape must distinguish field identity from field position.

A field's position is a tracked structural fact. A same-block reorder is a
position change for an otherwise matched field, not a remove-plus-add event.

Same-block field reorder must be reportable as its own finding when the field
can be matched unambiguously before and after the change. Field removal,
addition, block movement, duplicate-field ambiguity, and incompatible field
attribute changes must remain separately reportable.

Duplicate field names must not force the tracker to lose signal. When duplicate
fields can be matched with reviewable confidence, their position and attributes
are compared. When they cannot be matched unambiguously, the tracker reports an
ambiguous duplicate-field structural change requiring review instead of
pretending the change is a clean reorder.

### Session execution scope

The autonomous build scope is the tracker capability: fetch, stage, normalize,
compare, classify, report, dry-run promotion reporting, and network-free test
coverage.

Initial baseline seeding is a separate operator-reviewed activity. It requires a
live LS network fetch and human review before committing raw or normalized
baselines. This session must not claim completion of reviewed baseline seeding
unless that live fetch and human review have actually happened.

The deliverable for this session can therefore be complete without committing
the initial reviewed baseline files, provided it clearly documents the remaining
operator review step.

## Requirements

- The tracker captures the full LS public API inventory, not only TRs currently
  represented in `metadata/`.
- Captured upstream data is written as a **Staged Snapshot** before any reviewed
  baseline changes.
- Staged runs are isolated from committed source data until reviewed.
- Reviewed API Drift comparison uses committed **Reviewed Baselines**.
- The committed reviewed API Drift baseline consists of raw upstream evidence
  plus normalized Structural API Shape artifacts.
- Raw baseline evidence preserves upstream text and facts for audit.
- Normalized Structural API Shape artifacts preserve compact structural facts
  directly and represent long descriptive text in stable comparable form.
- Structural API Shape includes TR identity, API grouping facts, protocol facts,
  endpoint facts, request blocks, response blocks, field positions, field names,
  field attributes, and rate-limit facts when available.
- Structural comparison detects TR additions, TR removals, endpoint/protocol
  changes, rate-limit changes, block additions/removals, block moves, field
  additions/removals, same-block field reorders, cross-block field moves,
  required-flag changes, length changes, type changes, and description-only
  changes.
- Description-only changes are informational for this first real-fetch slice.
- Newly discovered upstream TRs are visible findings, but they do not
  automatically create `metadata/trs/*.yaml` or edit `metadata/tr-index.yaml`.
- Metadata coverage is reported as a summary: upstream inventory count,
  metadata count, implemented count, tracked-only count, metadata missing
  upstream, and upstream missing metadata.
- Metadata coverage summaries alone do not make a check fail.
- Actual upstream drift findings do make a check fail for review, even when the
  finding is informational.
- Fetch or parse incompleteness is an error, not ordinary drift.
- A fetch is incomplete when it cannot account for the upstream group/TR
  structure or when a TR present in the current reviewed baseline is absent from
  the staged inventory.
- The tracker has stable operator-facing exit meanings:
  - `0`: comparison completed and no drift findings were emitted.
  - `1`: comparison completed and one or more drift findings need review.
  - `2`: fetch, parse, baseline, staged-run, or internal error.
- Ordinary repository verification remains network-free.
- Live fetch and check commands are explicit opt-in operator actions.
- Dry-run promotion reports what would need review without writing baselines,
  metadata, docs, or SDK code.

## Severity Requirements

The tracker classifies findings with **Support-Aware Severity**.

Required first-slice severity behavior:

| Change | Metadata state | Severity |
|---|---|---|
| TR added | no metadata | maintenance |
| TR removed | no metadata | maintenance |
| TR shape changed | no metadata | informational |
| Description-only change | any state | informational |
| Same-block field reorder | implemented or tracked | maintenance |
| Field moved across block | implemented or recommended | breaking |
| Field removed or incompatible field changed | implemented or recommended | breaking |
| Field removed or incompatible field changed | tracked-only | maintenance |
| Endpoint or protocol changed | implemented or recommended | breaking |
| Endpoint or protocol changed | tracked-only | maintenance |
| Rate limit decreased | implemented or recommended | maintenance, unless reviewed as runtime-breaking |
| Rate limit changed | tracked-only or untracked | informational or maintenance |

Auth-wide structural changes and order-runtime structural changes are not
required first-slice detections unless the fetched upstream facts explicitly
support them. If they are detected, they may be classified as critical. If they
are not detected, the first-slice tracker must not imply that those risks are
covered.

## Operator Workflow

### Normal drift review

An operator runs an API Drift check. The tracker captures or loads a staged run,
compares it with the reviewed baseline, prints findings and metadata coverage,
and exits with the appropriate status.

No source files are changed during check.

### Reviewed baseline seeding

An operator runs a live fetch, reviews the staged raw and normalized output, and
commits the accepted baseline as a human-reviewed project artifact.

The first committed baseline is not self-validating merely because a staged run
matches itself. Review evidence must show that the captured inventory is
plausibly complete and that the normalized Structural API Shape represents the
upstream facts intended for future comparison.

### Dry-run promotion

An operator runs dry-run promotion against a staged run. The tracker reports
which baseline artifacts and related maintained project data would need review.

Dry-run promotion does not write those changes.

## Acceptance Examples

- A full-inventory fetch creates a staged run containing raw upstream evidence,
  normalized Structural API Shape artifacts, a manifest, and a fetch report.
- Checking a staged run against an identical reviewed baseline exits `0`.
- Removing an implemented TR response field in a fixture emits a `breaking`
  finding and exits `1`.
- Adding a previously unknown upstream TR emits a visible maintenance finding,
  exits `1`, and creates no metadata file.
- An unchanged upstream inventory with many upstream TRs and a small maintained
  metadata set prints coverage counts and exits `0`.
- A structurally incomplete fetch exits `2` and does not surface as mass TR
  removal drift.
- A same-block field reorder emits a reorder finding rather than paired
  add/remove findings when fields can be matched unambiguously.
- A description-only change emits an informational finding.
- A duplicate-field change that cannot be matched unambiguously emits an
  explicit review-required structural finding.
- Dry-run promotion lists review targets but leaves the working tree unchanged.

## In Scope

- API Drift Tracker capability.
- Full upstream API inventory capture.
- Staged run output.
- Raw and normalized reviewed baseline artifact contract.
- Structural API Shape comparison.
- Support-aware advisory findings.
- Metadata coverage summary.
- Operator-facing check, fetch, and dry-run promotion commands.
- Network-free tests using fixtures or local mocks.
- Clear documentation of the human-reviewed initial baseline seeding step.

## Out of Scope

- Specification Document Tracker behavior.
- SDK code generation.
- Automatic SDK code mutation.
- Automatic metadata creation or mutation.
- Automatic documentation regeneration from tracker output.
- Mutating baseline promotion.
- Treating PR #2 sample-payload fixtures as real API Drift baselines.
- Claiming the initial reviewed baseline is complete without live fetch evidence
  and human review.
- Default network access during ordinary repository verification.
- A fixed minimum TR-count floor as a completeness guarantee.
- Guaranteed detection of auth-wide or order-runtime critical changes unless
  those structures are explicitly represented in the fetched upstream facts.

## Open Follow-Up Decisions

- Whether likely TR rename grouping belongs in the first real-fetch slice or in
  a follow-up after core drift comparison is stable.
- What operational cadence will cause someone or something to run the opt-in
  tracker commands.
- How future accepted baselines should record review evidence so the initial
  bootstrap evidence does not become the permanent authority by default.
