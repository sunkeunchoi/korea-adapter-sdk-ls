---
title: "Order negative-probe policy — live `required`-variant probes are permitted for modify legs, excluded for submit legs with booking-determining fields"
date: 2026-07-14
category: conventions
module: crates/ls-sdk, metadata/constraints
problem_type: convention
component: orders
severity: high
applies_when:
  - "Deciding whether an order TR's negative differential probe may fire a live `required`-omit variant against the paper gateway"
  - "Authoring a new order TR's constraint schema and choosing whether it is safely probeable"
  - "Reviewing a HELD order TR to decide if a re-probe can conclusively certify it or if it stays permanently HELD"
tags: [negative-probe, order-safety, required-variant, modify-vs-submit, cspat00701, cspat00601, bnstpcode, placed-nothing]
---

# Order negative-probe policy — modify legs are probeable, submit legs with booking-determining fields are not

## Context

The order negative differential probe (`crates/ls-sdk/tests/negative_probe.rs`,
`run_order_negative_probe`) fires mechanically-generated invalid variants against
the **real paper gateway**. For an order endpoint the `required`-class variant
**omits** a required field — and omission at a live order endpoint is not a neutral
act: the gateway may reject it (placed nothing, the probe's intended signal) or it
may **default the omitted field and place a real resting order** (may-rest).

Whether that risk is acceptable depends on **what kind of order leg** is being
probed. This policy codifies the split and resolves pending.13 item #5.

## Policy

**Live `required`-variant probes are PERMITTED for _modify_ legs** (and _cancel_
legs). A modify is keyed by an `OrgOrdNo` we **seed and control**: the probe places
a known non-fillable seed order first, snapshots it via `t0425`, fires the omitted
variant against *that* seed, re-snapshots, and cancels the seed in teardown. Every
outcome is **reversible and observable** — a mutated or filled seed is caught by the
fill-inclusive snapshot comparison, and a stranded order is caught + cancelled by
the reconcile teardown. The seed makes the blast radius bounded.

**Live `required`-variant probes are EXCLUDED for _submit_ legs with
booking-determining fields** — the **`BnsTpCode` class** (buy/sell direction, and
any field whose omission changes *what order gets booked* rather than *whether one
is rejected*). A submit has **no seed to snapshot against**: omitting a
booking-determining field creates an **uncontrolled new resting order** with a
direction/price the gateway defaulted, and no pre-image to compare it to. There is
no reversible, observable A/B — only a live order placed on a guess.

## The two poles

- **`CSPAT00701`** (현물정정주문 — cash-equity modify, keyed by `OrgOrdNo`) —
  **probeable.** A seed + teardown make its `OrdprcPtnCode`/required omit variant
  reversible; it is characterized by the attended A/B in
  [`igw00000-cspat00701-placed-nothing-ab-probe`](../integration-issues/igw00000-cspat00701-placed-nothing-ab-probe.md).
- **`CSPAT00601`** (현물주문 — cash-equity submit) — **permanently HELD, not
  probeable.** Operator order-book confirmation (re-cert wave 3, 2026-07-13) proved
  that omitting `BnsTpCode` does **not** fail closed: the gateway defaulted the
  direction and placed a **real** resting order (`ordno=17093`, 005930 confirmed
  flat only after the fallback cancel). `BnsTpCode` therefore stays
  `required: true`, **unmarked and never `gateway_tolerant`** — a hard caller
  contract exists precisely because its omission silently books a direction-defaulted
  order. See `metadata/PROVISIONALITY-LEDGER.md` (§30 BnsTpCode posture,
  lines ~1977-1989).

## Why this is a convention, not a per-TR judgment

The temptation is to treat every HELD order TR as "re-probe and it might clear."
This policy draws the line once: the question is not "is the field required?" but
"**does omitting it place an uncontrolled order?**" A modify's omission mutates or
rejects an order we already hold; a submit's booking-field omission conjures a new
one. Only the former is safe to fire live. A submit-leg booking-determining field
is characterized (if ever) by a probe-**design** change that provably cannot route,
never by relaxing its schema.

## See also

- [`order-error-classifier-placed-nothing-vs-may-rest`](./order-error-classifier-placed-nothing-vs-may-rest.md)
  — the runtime classifier this policy's safety rests on.
- [`igw00000-cspat00701-placed-nothing-ab-probe`](../integration-issues/igw00000-cspat00701-placed-nothing-ab-probe.md)
  — the attended A/B runbook for the probeable (modify) pole.
