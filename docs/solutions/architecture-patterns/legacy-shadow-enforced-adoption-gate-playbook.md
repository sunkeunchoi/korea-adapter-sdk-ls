---
title: "Certify → Enforce → Retire: a per-consumer Legacy→Shadow→Enforced adoption-gate playbook for cutting a shared safety primitive into many consumers without a global flip"
date: 2026-07-21
category: architecture-patterns
module: "nautilus adapter — shared nautilus-ls-calendar core + its six consumer seams (ingest, checkpoint, backward-widen, catalog, budget-probe, Production Ladder)"
problem_type: architecture_pattern
component: tooling
severity: high
applies_when:
  - "introducing one shared, safety-bearing fact source (calendar, clock, entitlement, feature capability) that must replace an ad-hoc approximation scattered across several independent consumers"
  - "a global cutover is unacceptable because one consumer regressing would take down all of them, and the new source's live behavior cannot be fully proven offline"
  - "each consumer has its own action policy on the shared fact (fetch vs skip, GO vs NO-GO, dispatch vs refuse) and must migrate on its own schedule"
  - "the old approximation is a manual/operator-compensated protection that must stay authoritative until the replacement is proven per consumer"
related_components:
  - nautilus-ls-calendar
  - calendar-adoption
  - consumer-retirement-gate
  - merge-block
tags:
  - adoption-gate
  - legacy-shadow-enforced
  - certify-enforce-retire
  - per-consumer-migration
  - merge-block-scaffold
  - shadow-mode
  - safe-cutover
  - fail-closed
---

# Certify → Enforce → Retire: a per-consumer adoption-gate playbook

## Context

The `#185`–`#189` arc replaced Monday–Friday weekday arithmetic (an operator-compensated
approximation scattered across six calendar-dependent consumers) with one shared,
credential-free `nautilus-ls-calendar` core. The hard part was never the calendar code —
it was **cutting it into six independent consumers without a global flip and without
retiring the manual holiday protection before the replacement was proven per consumer.**
The pattern that made this safe is reusable whenever you introduce one shared safety-bearing
fact source that must displace a scattered approximation.

## The playbook

### 1. Model adoption as a per-consumer enum, not a boolean

`CalendarAdoption` had three arms: **`Legacy`** (old approximation authoritative),
**`Shadow`** (new source *computes and records* its decision but the legacy action still
wins), **`Enforced`** (new source decides; no silent fallback). Each consumer carries its own
adoption state — one consumer reaching Enforced never forces a global cutover. The shared core
stays pure (parse/validate/reconcile/query); *action policy* (fetch, GO/NO-GO, dispatch/refuse)
stays consumer-owned.

### 2. Ship defaulting to a pass-through arm (Shadow), so the merge is byte-identical

Shadow is engineered to be **byte-identical** to the old path in production output — it only
observes and records divergences. This lets the new source's code, composition roots, and
startup diagnostics land on `main` and run against real operating conditions **before** it
has authority. The re-baseline across the Shadow-default merge should be byte-identical; if it
isn't, Shadow isn't actually a pass-through and the "safe to merge" premise is false.

### 3. Two-tier gate: one shared Foundation Gate, then one Consumer Retirement Gate each

- The **Calendar Foundation Gate** proves the shared core once: full offline suite green with
  no production snapshot, no credentials, no network, fixed clocks — plus rollback rehearsal
  and divergence classification. It must pass before *any* consumer retires its workaround.
- Each consumer then has its own **Consumer Retirement Gate**: an attended live/owner-local
  canary, restart-after-activation, and rehearsed rollback, recorded as a verdict artifact
  (`gate-verdicts/<consumer>.json`, verdict `PASS`). Only after its gate passes does that
  consumer flip Legacy/Shadow → **Enforced** and delete its weekday primitive.

### 4. Enforce fail-closed at every seam

When the shared source is unavailable, out-of-coverage, or Unknown, the Enforced arm **stops
and preserves state** rather than falling back to the old approximation — that fallback is
exactly what you are retiring. Prove this with failure-inversion tests at the *real consumer
boundary*: Unknown emits no authorized action; changing only the fact to a positive value makes
the action observable.

### 5. Keep a merge-block scaffold as the retirement audit trail

A `merge-block` test + `make merge-block-check` refuse to delete a consumer's legacy path until
its verdict record says `PASS`. The scaffold is **kept after the teardown** as an audit trail —
it records *why* each retirement was authorized, not just that it happened.

### 6. Tear the scaffold down last, collapsing to a single arm

Once every consumer is Enforced and proven, collapse the enum to a single `Enforced` variant
and remove the Legacy/Shadow machinery + divergence classification in one teardown
(`#189` U10). Retired tokens (`legacy`/`shadow`) should no longer parse — a test asserts this
so the collapse can't silently regress.

## Why this beats a global flip

- **Blast radius is one consumer.** A regression in the ladder cutover can't redden ingest.
- **The new source runs in production before it has authority** (Shadow), so live divergences
  are classified as evidence *before* enforcement, not discovered after a cutover.
- **The manual protection stays authoritative until per-consumer proof exists** — the gate,
  not a calendar date, decides when it's safe to remove.

## Gotchas (cross-linked)

- Collapsing the enum makes the surviving arm's behavior **newly live, not newly written** —
  audit the newly-authoritative arm, not just the diff. See
  `retiring-a-feature-flag-arm-makes-its-behavior-newly-live.md`.
- A per-date safety fact gated inside a range/aggregate op can be silently bypassed by a
  coarser-grained caller. See
  `../logic-errors/safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md`.
- The offline gate never runs live smokes; a green gate is *certified-offline*, not
  live-certified — that's exactly why each consumer needs an attended live canary before its
  retirement gate passes.

## Evidence

Delivered by PRs #190 (build slice, closes #185's build) → #194 (Foundation Gate + rollback +
divergence + merge-block) → #196/#195/#199/#197 (catalog/budget-probe/ingest/ladder Enforced
cutovers) → #200 (U10 teardown, closed #189) → #202 (bounded empty-proven-session re-fetch).
Tracking issues #185/#184/#120 closed 2026-07-21 with per-AC evidence.
