---
title: "API Drift Tracker — first-slice scope pressure-test"
type: requirements
date: 2026-06-16
status: resolved
supersedes_in_part:
  - docs/plans/2026-06-16-001-feat-api-drift-real-fetch-what.md
  - docs/plans/2026-06-15-004-feat-api-drift-real-fetch-plan.md
---

# API Drift Tracker — first-slice scope pressure-test

## Purpose

The first real API Drift Tracker scope (`docs/plans/2026-06-16-001-feat-api-drift-real-fetch-what.md`)
and its plan (`docs/plans/2026-06-15-004-feat-api-drift-real-fetch-plan.md`)
were adversarially pressure-tested. The pipeline shape survives unchanged:
fetch → stage → normalize into Structural API Shape → compare against a Reviewed
Baseline → report Support-Aware advisory findings, network-free by default, no
SDK/metadata/doc/baseline mutation.

Six decisions sharpen the **signal model** and cut machinery ahead of evidence,
so the tracker is usable against a ~365-TR upstream surface that serves 7
maintained TRs. This document records those deltas. It does not restate the
unchanged scope.

## Problem Frame

The original scope treated the full ~365-TR inventory and the 7-TR maintained
surface identically in its exit contract and its committed baseline. Three
consequences followed:

- Any finding — including an informational typo on an untracked TR — produced
  the same exit `1` as a `breaking` change on an implemented TR. Against ~365
  TRs this is cry-wolf, and the tracker is opt-in, so a distrusted signal is an
  unrun tracker.
- The committed Structural API Shape baseline covered all ~365 TRs, making the
  *human review* surface (per ADR 0005) ~358 TRs of shape that nothing gates on.
- The completeness gate fused "real upstream TR removal" and "truncated scrape"
  into one exit-`2` bucket, leaving the `TR removed` severity rows unreachable
  for baselined TRs.

The migration source (`~/dev/korea-broker-sdk-ls`) is direct evidence here: its
drift detector treats removed TRs as reviewable drift, guarded silent-wipe with
a numeric `MIN_TR_COUNT` floor (not the pinned-code check), and — having lived
through real LS drift — never built rename detection.

## Resolved Decisions

### D1. Severity-tiered, support-aware exit contract

The exit contract gains a notion of severity and support state instead of
"any finding = exit `1`".

- `0`: comparison completed; no finding crossed the gate threshold.
- `1`: a finding touches a tracked / implemented / recommended TR at
  **maintenance or breaking** severity, **or** a new untracked TR was
  discovered.
- `2`: fetch, parse, baseline, staged-run, or internal error (unchanged).

All other untracked-TR changes (shape, field, removal, description) and **all
informational findings** are reported but exit `0`. Severity and support state
become part of the contract, not only the report.

This partially overrides Acceptance Example AE4: new-TR *discovery* still gates
(exit `1`, "should we track this?"), but field-level changes to an untracked TR
do not.

### D2. Bounded structural baseline, full-inventory code-set

The committed **Structural API Shape** baseline covers only **maintained +
adjacent** TRs (~dozen). The full ~365-TR inventory is tracked at **code-set
granularity** only — a cheap list that drives the new-TR gate (D1), the
completeness anchor (D3), and the coverage summary.

Completeness and coverage decouple from structural baselining: they need the
code-set, not per-TR shape. ADR 0004's "complete tracking" is satisfied as full
inventory *awareness*, not full structural diff. The human-reviewed baseline
collapses from ~365 to ~a dozen TRs.

### D3. Removal vs truncation split

The two incompleteness conditions already named in the WHAT scope are separated:

- A baselined TR absent from an **otherwise well-parsed** menu is a real
  `TR removed` finding, exited by severity (maintained → `1`, untracked →
  report-only `0`).
- Exit `2` fires only when the menu / group structure itself fails to parse,
  **or** when absent TRs exceed a **relative proportion** of the code-set
  (suspected mass truncation).

The proportion is relative (scales with inventory), never a fixed count — it
replaces the migration source's `MIN_TR_COUNT` floor as the non-magic
silent-wipe guard.

### D4. Rename detection → minimal report hook

No fingerprinting algorithm and no dedicated rename fixtures in the first slice
(cuts R14 / AE8). When a removal and an addition co-occur in one run, the report
lists them adjacently with a "possible rename?" note. Real fetch data revisits
whether LS renames TRs often enough to justify matching logic.

### D5. Incremental code-set re-attestation

The completeness code-set is re-attested through the new-TR review loop: each
new-TR finding (exit `1`) forces the operator to decide whether to admit that TR
into the reviewed code-set. The bootstrap seed (derived once from the migration
source) is explicitly **provisional**, not permanent authority. The reviewed
commit that updates the code-set is the review-evidence trail; no separate
attestation manifest is added.

### D6. Cadence via an existing checkpoint

The check is bound to an existing recurring human checkpoint (release checklist
or periodic maintenance review) as a documented operator step, rather than new
scheduled automation. This gives the tracker a real trigger at near-zero infra
cost and keeps the default verification network-free. The first slice does not
ship cron/CI scheduling.

## What This Changes vs the WHAT Scope

| Area | Original (`...-what.md`) | After pressure-test |
|---|---|---|
| Exit `1` rule | Any finding, incl. informational | D1: maintained ≥ maintenance, or new-TR discovery |
| Informational findings | Exit `1` | Report-only, exit `0` |
| Structural baseline scope | All ~365 TRs | D2: maintained + adjacent; full code-set for awareness |
| TR removal | Exit `2` (fused with truncation) | D3: real `TR removed` finding; exit `2` only on parse failure / relative mass-absence |
| Rename grouping (R14/AE8) | Fingerprinting + grouping | D4: adjacency note only |
| Code-set authority | One-time seed, never re-attested | D5: incremental via new-TR loop; seed provisional |
| Trigger / cadence | Deferred, unowned | D6: bound to existing operator checkpoint |

## Scope Boundaries

### In scope (this revision)

- The six decisions above, folded into the first real-fetch slice.

### Unchanged from the WHAT scope

- Fetch / stage / normalize / compare / report pipeline.
- Structural API Shape as the only real-fetch comparison model.
- Support-Aware Severity classification (the table still applies; D1 governs only
  which rows cross the exit gate).
- No mutation of SDK code, metadata, docs, or committed baselines.
- Network-free default verification; live fetch is explicit opt-in.

### Out of scope / deferred

- Rename fingerprinting and its fixtures (D4) until real data justifies them.
- Scheduled cron/CI automation (D6).
- Full structural baselining of untracked TRs (D2).

## Open Questions (planning-level, not product)

- The **relative-proportion threshold** for D3's mass-truncation guard — concrete
  value and whether it is operator-tunable.
- The precise definition of **"adjacent"** for D2's bounded baseline (e.g. same
  dependency class as a maintained TR, vs same instrument domain) and how a TR
  enters the adjacency set.

## Dependencies / Assumptions

- D6 assumes a recurring operator checkpoint exists and will host the documented
  step; without an owner running it, the tracker stays inert (the counterfactual
  the pressure-test surfaced).
- D2/D3/D5 assume the full-inventory **code-set** is promoted to a first-class
  reviewed artifact, maintained independently of the bounded structural baseline.
- D1 assumes the description-hash normalizer's residual imperfection now produces
  only report noise, not gate failures — informational findings no longer exit
  `1`, which defuses that robustness concern as a side effect.
