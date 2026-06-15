---
date: 2026-06-15
topic: sdk-first-slice-decisions
---

# Maintained SDK — First-Slice Decisions

## Summary

Resolves the five open questions in `docs/plans/maintained-sdk-migration-plan.md` so planning can proceed without inventing scope: which TRs the first vertical slice implements, how the Rust workspace is organized, what authority validates TR metadata, and how focused-evidence freshness is judged for a Recommended TR. The old-repo obsolete notice is dropped from this project.

## Key Decisions

- **Prove each dependency class with the lightest representative.** The first slice picks one TR per class that exercises a distinct facet at the lowest evidence burden — coverage of *classes*, not of high-priority TRs. This is why the slice avoids `t8430` (known array-shape blocker upstream) and the heavier `t1101`/`t0424` (both need workflow-smoke evidence that proves no new class). The standalone class is the one exception: it is covered only at the auth-primitive level (`token`/`revoke`), because its natural data representative `t8430` is blocked, so a true standalone *data* TR is deferred (see Outstanding Questions).

- **Split crates by tooling concern, not by dependency class.** The shippable library (`ls-core`, `ls-sdk`) is isolated from maintenance tooling (`ls-metadata`, `ls-trackers`) so trackers and metadata never enter a user's dependency tree. Dependency classes live as modules inside `ls-sdk`. Crate-per-class was rejected as boilerplate that buys nothing, because the Change-Scoped Gate routes by metadata facet rather than crate boundary.

- **Rust types are the single schema authority.** The serde structs and validation in `ls-metadata` are what validate per-TR YAML. A JSON Schema, if added later, is *generated* from those types (e.g. `schemars`) purely for editor autocomplete — never a hand-maintained second definition that could drift.

- **Evidence freshness is change-driven first, time-bounded second.** A Recommended TR's focused evidence is invalidated the moment a spec or doc change affects that TR; absent any such change it stays valid up to a 90-day backstop, after which an `evidence` finding fires. The backstop catches behavior drift the trackers can't see (session quirks, account-state edge cases).

## Requirements

**First-slice coverage**

- R1. The first vertical slice implements exactly one TR per representative dependency class: `token` and `revoke` (standalone), `t1102` (market_session), `t8412` (paginated), `CSPAQ12200` (account), and `S3_` (realtime).
- R2. The `orders` class ships as metadata plus safety/reconciliation design only; no order runtime in the slice.
- R3. The slice excludes any TR with a known unresolved upstream blocker, including `t8430`.

**Workspace layout**

- R4. The Rust workspace uses four crates: `ls-core` (auth, config, client, transport, errors, rate buckets), `ls-sdk` (public facade plus dependency-class modules), `ls-metadata` (TR metadata types, index, schema validator), and `ls-trackers` (API Drift and Specification Document trackers). These four are the target workspace shape; the first slice builds `ls-core`, `ls-sdk`, and `ls-metadata`, with `ls-trackers` added when the tracker skeleton lands.
- R5. Dependency classes are modules within `ls-sdk`, not separate crates; the Change-Scoped Gate selects tests by metadata facet, not by touched crate.

**Metadata schema**

- R6. The `ls-metadata` Rust types and validator are the authority that validates per-TR YAML; they are validated first and gate metadata correctness.
- R7. Any JSON Schema is generated from the Rust types for editor tooling; a hand-maintained JSON Schema as a parallel shape definition is disallowed.

**Evidence freshness**

- R8. A Recommended TR's focused evidence is invalidated immediately when a tracked spec or doc change affects that TR.
- R9. Absent an affecting change, focused evidence is valid for 90 days, after which an `evidence`-severity finding fires; the window is a per-class-tightenable default, not a fixed constant.
- R10. Change-driven invalidation (R8) becomes active only once the Specification Document Tracker and its reviewed baselines exist; until then the 90-day backstop (R9) is the sole operative freshness control for the slice.

## Scope Boundaries

- The old repository's obsolete/deprecation notice is out of scope for this project. The old repo remains a readable Migration Source; no notice, archive, or registry action is part of this work.
- Order runtime (no-retry dispatch, deduplication, reconciliation, guarded evidence) stays deferred per ADR 0008.
- A true standalone *data* TR (no caller-supplied identifiers) is not in the first slice; the standalone class is represented by auth primitives only for now.

## Outstanding Questions

**Deferred to planning**

- Whether to add one true standalone *data* TR to the slice once the 38 standalone TRs are enumerated, to prove the standalone class beyond auth primitives.
- The per-class freshness windows that should override the 90-day default, especially the tighter window `orders` will need when its runtime lands.
- When to introduce generated JSON Schema for metadata editor tooling — triggered by observed hand-authoring friction, not scheduled up front.
- Whether the slice exercises facet-routing *composition*, not just per-facet firing — confirm `t8412`'s `self_paginated` + `date_sensitive` profile drives multiple facet-selected test groups, and decide whether a deliberately multi-facet TR is needed.

## Sources

- `docs/plans/maintained-sdk-migration-plan.md` — the five open questions resolved here.
- `docs/adr/0008-defer-order-runtime-until-safety-package-is-complete.md` — order-runtime deferral.
- Old repo `korea-broker-sdk-ls` (Migration Source): `docs/PRIORITY_TR_CERTIFICATION_MATRIX.md` and `docs/TR_DEPENDENCY_REFERENCE.md` grounded the TR facet/dependency shapes (`t1102`, `t8412`, `CSPAQ12200`, `S3_`, `t8430` blocker).
