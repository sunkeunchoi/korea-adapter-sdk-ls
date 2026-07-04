---
title: "Constraint-schema `required` must match what the certified struct actually sends, not the baseline — declare `required: false` for any field the SDK sends empty, or preflight false-rejects certified flows (orders included)"
date: 2026-07-03
category: conventions
module: "ls-core preflight seam (crates/ls-core/src/preflight.rs, inner.rs), constraint schemas (metadata/constraints), ls-sdk request structs (crates/ls-sdk/src/orders, market_session, account)"
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Authoring a per-TR error-resilience-gate constraint schema (metadata/constraints/<tr>.yaml) for the recommended re-certification gate"
  - "A field is `required: true` on the normalized baseline but the certified SDK request struct legitimately sends it empty (a caller-optional loan date, an all-symbols filter, a first-page continuation cursor)"
  - "An existing order or read unit/integration test starts failing with `LsError::Invalid { field, reason: \"is required but was missing or empty\" }` after a constraint schema was added for its TR"
  - "Deciding the `required` flag when the baseline and the certified request struct disagree"
tags:
  - constraint-schema
  - preflight
  - error-resilience-gate
  - required-flag
  - ls-core
  - ls-sdk
  - orders
  - grounding
---

# Constraint-schema `required` must match the certified struct, not the baseline

## Context

The error-resilience gate (plan 2026-07-01-004, PR #83) added per-TR request
field-constraint schemas (`metadata/constraints/<tr>.yaml`) that drive a
**preflight** check at `crates/ls-core/src/inner.rs` — the **single dispatch
seam** every owner class funnels through (`post` / `post_paginated` /
**`post_order`**), not just reads. Preflight enforces `required` and `type`
unconditionally (only `enum`/`range`/`format` are gated on `confirmed`), and it
treats an **empty string as absent** (`present()` returns false for `""`, since
LS encodes an omitted field as `""`).

Grounding (`crates/ls-core/tests/constraint_grounding.rs`) checks each field's
`type` + `required` against the normalized baseline, but only in the **permissive
direction**: you may declare a baseline-`required` field caller-**optional**,
never the reverse. That permissive direction exists precisely for this
convention. The recommended re-certification wave (plan 2026-07-03-003, PR #92)
authored nine of these schemas and hit the trap directly.

## Guidance

**Set `required` to what the certified request struct actually sends, not to what
the baseline marks.** If the live-certified SDK request struct (or its
constructor) can send a field empty in a normal, certified flow, declare it
`required: false` in the constraint schema — even though the wire/baseline marks
it required. The struct wins on disagreement, and the grounding gate permits the
less-required declaration.

Declaring `required: true` for a field the certified flow sends empty makes
preflight reject that certified request with `LsError::Invalid { field, reason:
"is required but was missing or empty" }` **before any HTTP call** — a
false-rejection of a proven-good flow, and for orders it blocks a real order path.

Concrete `required: false` cases from the wave:

- **`CSPAT00601.LoanDt`** — the certified `CSPAT00601Request::limit()` cash-order
  constructor passes `""` for the loan date (loan date is only meaningful for a
  credit order). `required: true` would reject every certified cash order.
- **`t0425.expcode`** — `T0425Request::for_symbol()` documents empty `expcode` as
  "queries all symbols"; a symbol filter, not a mandatory key.
- **`t0425.cts_ordno`** — the pagination continuation cursor, `" "` on the first
  page; caller-optional by construction.

## Why This Matters

Preflight is a fail-closed guard: an `Invalid` error is non-retryable and short-
circuits before the token fetch or the network call. That is exactly what you
want for a genuinely malformed request, but a mis-declared `required: true` turns
it into a silent regression of a certified capability — most dangerously on the
**order path**, where the schema addition can block live order submission that
previously worked. Because grounding only checks the permissive direction, an
over-strict `required: true` **passes grounding** and only surfaces as a runtime
false-rejection (or a suddenly-red order test), not a compile or gate failure at
authoring time.

## When to Apply

- Whenever you author or review a `metadata/constraints/<tr>.yaml`, cross-check
  each `required: true` field against the certified request struct's constructors
  (`crates/ls-sdk/src/**`): does any certified path send it empty? If yes →
  `required: false`.
- **Corollary — adding a schema for a real order `tr_code` breaks empty-body
  classification tests.** ls-core order-mechanics unit tests reuse a real order
  `tr_code` with deliberately-empty synthetic bodies to exercise *post-preflight*
  response classification (ack codes, dedup, retry). Once that `tr_code` gains a
  constraint schema, those bodies trip preflight. Repoint the test helper to a
  **schema-less** `tr_code` so it still reaches the mock server — e.g.
  `inner.rs::order_policy` was repointed `"CSPAT00601"` → `"ORDER_TEST"`. Do not
  populate the synthetic bodies; the point of those tests is the post-preflight
  path, and a schema-less code is the honest way to reach it.

## Examples

Baseline marks `LoanDt` required; the certified constructor sends it empty:

```rust
// crates/ls-sdk/src/orders/mod.rs — the certified cash-order constructor
CSPAT00601Request::limit(isuno, ordqty, ordprc, bnstpcode, mbrno)
//   -> new(.., loandt = "", ..)   // LoanDt is empty for a cash order
```

```yaml
# metadata/constraints/CSPAT00601.yaml — the struct wins over the baseline
- name: LoanDt
  type: string          # baseline: String, required on the wire — but the
  required: false        # certified cash-order flow sends it EMPTY (permissive).
  enum: { applicable: false }
  range: { applicable: false }
  format: { applicable: false }
```

The failure a `required: true` here would produce (a certified flow, rejected
before any HTTP call):

```
LsError::Invalid { field: "LoanDt", reason: "is required but was missing or empty" }
```

Related learnings:

- [[normalized-baseline-can-underreport-request-block]] — the sibling half of
  "the certified struct wins": that doc is about the baseline **omitting fields
  entirely** (field presence); this doc is about the **required-emptiness**
  direction false-rejecting at the preflight seam. Complementary, not duplicative.
- [[order-error-classifier-placed-nothing-vs-may-rest]] — order-path fail-closed
  classification, the same preflight/order-safety area.
- `docs/plans/2026-07-01-004-feat-recommended-error-resilience-gate-plan.md` and
  `metadata/PROVISIONALITY-LEDGER.md` §25 — the gate mechanism and the re-cert
  wave that surfaced this.
