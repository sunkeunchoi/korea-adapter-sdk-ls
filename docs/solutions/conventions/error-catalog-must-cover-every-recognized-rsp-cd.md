---
title: "A gateway rsp_cd recognized at the test/classifier layer must also be in the shared error-catalog.yaml, or explain() silently falls through to UNKNOWN"
date: 2026-07-14
category: conventions
module: crates/ls-core, metadata
problem_type: convention
component: error_handling
severity: medium
related_components:
  - crates/ls-docgen
  - crates/ls-sdk
applies_when:
  - "Adding recognition for a new gateway rsp_cd anywhere in the tree (a test sentinel, a VENUE_CLOSED/PAPER_* const, a classifier set, a smoke skip)"
  - "A runtime error surfaces the generic UNKNOWN_CODE_EXPLANATION even though the code is clearly understood elsewhere in the codebase"
  - "Deciding where a newly observed gateway code needs to be registered so callers get a human-readable reason"
tags:
  - error-catalog
  - rsp-cd
  - error-explanation
  - single-source-of-truth
  - docgen
---

# Test-layer recognition of a gateway rsp_cd is not runtime catalog coverage

## Context

`metadata/error-catalog.yaml` is the **single source of truth** mapping every
gateway response code the runtime knows how to explain to a credential-free,
human-readable reason. It is consumed twice:

- **Runtime** — `ls-core` embeds the file at build time; `LsError::explain()`
  (`crates/ls-core/src/error.rs:125`) routes an `ApiError`/`AmbiguousOrder`
  `code` through `error_catalog::explain_or_default`
  (`crates/ls-core/src/error_catalog.rs:60`), so a caller gets a reason instead
  of a bare code.
- **Docs** — `ls-docgen` reads the same file (`report.error_catalog`,
  `crates/ls-docgen/src/lib.rs:724`) to project the per-TR "Errors & validation"
  Reference section (R11).

The trap: a gateway code can be **recognized somewhere else in the tree** — a
test sentinel, a classifier set, a smoke-skip guard — while being **absent from
`error-catalog.yaml`**. When that code reaches the runtime,
`explain_or_default` finds no entry and returns the generic
`UNKNOWN_CODE_EXPLANATION` ("An unrecognized gateway response code…",
`error_catalog.rs:39`). Nothing errors; the caller just gets a useless message.
**Test-layer (or classifier-layer) recognition ≠ runtime catalog coverage.**

Concrete case (PR #140, merged to `main` as commit `3572cf5`): `01458`
(모의투자 장종료 / paper order-session-closed) was recognized only at the test
layer —`VENUE_CLOSED_CODES` in `crates/ls-sdk/tests/order/overseas_fo.rs`, the
`is_market_closed` sentinel in `crates/ls-sdk/tests/order_smoke.rs`, and the
paper-reset skip in `crates/ls-sdk/tests/order/chain.rs` — but was never added
to `error-catalog.yaml`. It was observed live falling through to
`UNKNOWN_CODE_EXPLANATION` on the `igw00000-ab` order leg, 2026-07-14 15:36.

## Guidance

When you make the runtime recognize a new gateway `rsp_cd` **anywhere** — a test
constant, a classifier bucket, a smoke-skip guard — also register it in the
shared catalog so `explain()` has a real reason to return:

1. Add one entry to `metadata/error-catalog.yaml` with a `kind` (mirror the
   nearest existing entry of the same class — e.g. `session_closed` for `904`)
   and a **credential-free** `explanation` that never echoes the gateway
   `rsp_msg` or account data.
2. Consider adding the code to the coverage test
   `error_catalog.rs::catalog_parses_and_covers_every_known_code`, which asserts
   the catalog maps every code the runtime classifies. Adding it there first
   gives you a red-before-green proof (`catalog must map <code>`) that the entry
   is actually wired.

Registering a code **for explanation** is independent of admitting it to the
runtime **placed-nothing seam**. `is_ingress_validation_reject`
(`error_catalog.rs:82`) is deliberately narrow — `IGW40011` only — and decides
whether the order path treats a reject as "placed nothing" vs "may rest." A
session/venue code like `01458` belongs in the catalog for a readable message
but must **not** be added to that seam: it is not a pre-routing ingress reject,
and the probe/runtime divergence (e.g. the scoped order-probe allowlist vs the
seam) is intentional. Cataloguing a code and admitting it to the seam are two
separate decisions.

## Why This Matters

The catalog exists precisely so a caller sees "the order session is closed —
retry inside the KRX order window" instead of a raw `01458`. A code that the
codebase demonstrably understands, yet still explains as "unrecognized," is a
silent regression in the one surface built to prevent it — and because
`explain_or_default` is total (it never errors on a miss), nothing fails loudly
to flag the gap. The docgen consumer compounds the cost: an uncatalogued code is
also invisible in the projected R11 Reference tables. The fix is one YAML entry;
the gap is only expensive because it hides.

## When to Apply

- Any time a new gateway `rsp_cd` gains recognition in a test sentinel, a
  `VENUE_CLOSED`/`PAPER_*` const, a classifier set, or a smoke-skip guard.
- When a live run or smoke prints `UNKNOWN_CODE_EXPLANATION` for a code that is
  clearly handled elsewhere — the catalog is the missing link.
- Not for a code the runtime should stay ignorant of (a truly unknown code
  should fall through to the generic fallback by design).

## Examples

Symptom — a recognized code explained as unknown:

```rust
// 01458 lives in VENUE_CLOSED_CODES and drives is_market_closed(), yet:
assert!(ls_core::error_catalog::explain("01458").is_none()); // BEFORE #140
// => LsError::explain() returns UNKNOWN_CODE_EXPLANATION at runtime.
```

Fix — one catalog entry mirroring the sibling `session_closed` code, plus the
coverage-test line that proves it (red first, then green):

```yaml
# metadata/error-catalog.yaml
  "01458":
    kind: session_closed
    explanation: >-
      The order session is closed (모의투자 장종료 / paper order session closed).
      Reads and quotes still flow after the KRX order-window close, but an order
      submit is rejected with this code. A session-timing environment condition,
      not a request defect: the same order succeeds inside the KRX order window.
      Retry at the next open.
```

```rust
// crates/ls-core/src/error_catalog.rs — coverage list gains "01458";
// run it BEFORE adding the YAML entry to witness the red:
//   catalog must map `01458`; it is a code the runtime classifies
for code in ["00000", /* … */ "904", "01458", "IGW40011", /* … */] {
    assert!(explain(code).is_some());
}
```

## Related

- [`order-error-classifier-placed-nothing-vs-may-rest`](order-error-classifier-placed-nothing-vs-may-rest.md)
  — the same `01458`/`IGW40011` codes seen from the classifier angle; that doc
  governs the placed-nothing vs may-rest **variant** branch, this one governs
  **catalog coverage** for the explanation surface. The
  `is_ingress_validation_reject` seam referenced above is the boundary between
  the two concerns.
- [`ls-gateway-igw40011-numeric-request-fields`](../integration-issues/ls-gateway-igw40011-numeric-request-fields.md)
  — another catalogued gateway code (`IGW40011`), the numeric-request-field
  defect.
