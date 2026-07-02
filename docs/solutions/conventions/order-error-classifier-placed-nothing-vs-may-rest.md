---
title: "Order-error classifier must branch on the LsError variant (placed-nothing vs may-rest), not just the rsp_cd"
date: 2026-07-02
category: conventions
module: crates/ls-sdk, crates/ls-core
problem_type: convention
component: orders
severity: high
applies_when:
  - "Layering a capability/verdict classifier on top of a live order submit error (e.g. mapping a rejection to a PENDING code)"
  - "Deciding whether a failed order submit placed nothing (safe to record + return) or may have left a resting order (must fail closed)"
  - "Classifying an order gateway rsp_cd (01491, 01900, 01458, IGW40011, unknown) into buckets"
  - "Authoring the next order chain (e.g. the overseas US-stock COSAT/COSMT quad) that reuses the CIDBT/CFOAT classifier shape"
---

# Order-error classifier must branch on the LsError variant, not just the rsp_cd

## Context

An order submit that returns an `Err` splits into two safety-critical outcomes:

- **Placed nothing** — the gateway received the order and *cleanly rejected it*
  (a 2xx response carrying a rejection `rsp_cd`). Nothing rests. Safe to record a
  PENDING/verdict and `return`.
- **May rest** — the outcome is *ambiguous*: the order may have reached the
  exchange and be resting **now**, with no order number to cancel it. This must
  **fail closed** (engage the kill switch, then a loud panic for a manual board
  check), never a silent PENDING that returns.

`ls-core` dispatch already encodes this in the `LsError` **variant** (see
[`autonomous-order-smoke-fail-closed-contract`](../architecture-patterns/autonomous-order-smoke-fail-closed-contract.md)
§4): on the order path a **clean 2xx business rejection** surfaces as
`LsError::ApiError { code, .. }` (placed nothing), whereas a **non-2xx** HTTP, a
transport error, or an undecodable 2xx response surfaces as
`LsError::AmbiguousOrder { code, .. }` / `LsError::Http(_)` / `LsError::Decode(_)`
(may rest). `AmbiguousOrder`'s `code` is **empty** when the failure was a
transport-level 5xx on the order path (`crates/ls-core/src/error.rs`,
`crates/ls-core/src/inner.rs:344`).

The trap this convention guards: when you add a **capability-verdict classifier**
on top (mapping the gateway `rsp_cd` to buckets like "account not order-capable
`01491`" vs "some other rejection"), it is natural to key the classifier on the
`rsp_cd` **string alone**. Doing so silently **re-collapses** the variant's
placed-nothing/may-rest distinction — because a transport/5xx `AmbiguousOrder`
carries an empty code, it falls into the catch-all "other rejection" bucket
alongside a clean band/tick `ApiError` rejection, and both then record a benign
PENDING and return. That is **fail-OPEN**: a possibly-resting, uncancelable order
is left behind. It bites hardest on chains with **no transient-position read**
(overseas futures `CIDBT`) where a stray fill cannot even be detected afterward.

## Guidance

A live-order-submit error classifier must branch on the **`LsError` variant**, and
only *within* the clean-rejection variant classify by `rsp_cd`:

```rust
fn classify_submit_error(err: &LsError) -> CapabilityVerdict {
    let code = order_error_code(err).unwrap_or(""); // trims; None for Invalid/transport

    // 1. Pre-execution serde defect (IGW40011). Placed nothing, but a LOUD verdict,
    //    never a capability PENDING. Checked FIRST because on the order path it rides
    //    an HTTP-500 AmbiguousOrder that the may-rest arm below would otherwise swallow.
    if code == "IGW40011" {
        return CapabilityVerdict::RequestShapeDefect;
    }
    match err {
        // 2. Clean 2xx business rejection — the gateway rejected the order, placing
        //    nothing. NOW classify by rsp_cd.
        LsError::ApiError { .. } => {
            if ls_core::is_paper_order_incapable(code) { CapabilityVerdict::PaperOrderIncapable }   // 01491
            else if ls_core::is_paper_incompatible(code) { CapabilityVerdict::PaperIncompatible }   // 01900
            else if VENUE_CLOSED_CODES.contains(&code) { CapabilityVerdict::VenueClosed(code.into()) } // 01458
            else { CapabilityVerdict::OtherRejection(code.into()) }
        }
        // 3. Non-2xx / transport / undecodable-2xx — the order MAY be resting. Fail closed.
        LsError::AmbiguousOrder { .. } | LsError::Http(_) | LsError::Decode(_) =>
            CapabilityVerdict::AmbiguousPlacement,
        // 4. Client-side preflight / config — never reached the wire, placed nothing.
        _ => CapabilityVerdict::OtherRejection(code.into()),
    }
}
```

The harness then routes `AmbiguousPlacement` to the fail-closed path — engage
`set_orders_enabled(false)` **after** the (impossible-here) teardown, then
`panic!(loud_failure(...))` for a manual board check — exactly as the domestic
`fo.rs` ambiguous-submit catch-all does. `OtherRejection`/`VenueClosed` record a
PENDING and return (placed nothing); only `01491` is a capability PENDING.

Two `rsp_cd` facts that make the variant branch load-bearing:

- **`IGW40011` (a request-shape / serde defect) surfaces as
  `LsError::AmbiguousOrder { code: "IGW40011" }` on the order path**, not
  `ApiError` and not the client-side `LsError::Invalid` — because a non-2xx HTTP
  on an order is ambiguous (`inner.rs:344`), and IGW40011 is an HTTP-500
  ([`ls-gateway-igw40011-numeric-request-fields`](../integration-issues/ls-gateway-igw40011-numeric-request-fields.md)).
  It is genuinely placed-nothing (rejected pre-execution), so it must be matched
  on the **code, before** the may-rest variant arm, and raised as a loud serde
  defect — never binned as a capability PENDING.
- **`01491` (account-not-order-capable) arrives on a 2xx** as
  `LsError::ApiError { code: "01491" }`
  ([`ls-paper-01491-account-not-order-capable`](../integration-issues/ls-paper-01491-account-not-order-capable.md)),
  so it lives inside the `ApiError` (placed-nothing) arm.

## Why This Matters

Order placement is irreversible. A classifier keyed on `rsp_cd` alone conflates
"the exchange cleanly said no" with "we don't know — an order may be resting," and
records both as a benign PENDING that returns. On a chain with no position
read-back, that resting order is invisible and uncancelable. Branching on the
variant preserves the one distinction that decides between a safe record-and-return
and a mandatory fail-closed halt. It also keeps the two non-capability loud cases
(IGW40011 serde defect, ambiguous placement) from ever poisoning the capability
ledger fact the wave produces (never a false `01491`).

Both an in-process reliability reviewer and an independent cross-model (Codex)
adversarial pass flagged the fail-OPEN version of this independently — the
strongest possible signal that the rsp_cd-only shortcut is a real trap, not a
theoretical one.

## When to Apply

- Any live-order-submit error handler that maps an `Err` to a verdict/PENDING.
- Especially when the chain has **no transient-position read** to detect a stray
  fill after the fact (overseas futures `CIDBT`; the deferred US-stock
  `COSAT`/`COSMT` quad will reuse this shape).
- Whenever a capability classifier is layered over the order-dispatch error — keep
  the placed-nothing/may-rest branch on the variant, and only classify `rsp_cd`
  inside the `ApiError` arm.

## Examples

Fail-OPEN (the bug — do not do this): classify on the code string only.

```rust
// A 503 / dropped connection → AmbiguousOrder{code:""} → order_error_code == "" →
// falls through every code check to the catch-all → OtherRejection("") →
// records a PENDING and RETURNS. An order may be resting uncancelable. FAIL-OPEN.
fn classify(err: &LsError) -> Verdict {
    let code = order_error_code(err).unwrap_or("");
    if code == "IGW40011" { return Verdict::RequestShapeDefect; }
    if is_paper_order_incapable(code) { return Verdict::PaperOrderIncapable; }
    // ... other code checks ...
    Verdict::OtherRejection(code.into())   // <-- swallows the may-rest transport error
}
```

Fail-closed (correct): branch on the variant first (see Guidance). Regression test
the exact transport case so a future refactor cannot silently reopen it:

```rust
let transport_5xx = LsError::AmbiguousOrder { code: String::new(), message: "502".into() };
assert_eq!(classify_submit_error(&transport_5xx), CapabilityVerdict::AmbiguousPlacement);
assert_ne!(classify_submit_error(&transport_5xx), CapabilityVerdict::OtherRejection(String::new()));
```

## Related

- [`autonomous-order-smoke-fail-closed-contract`](../architecture-patterns/autonomous-order-smoke-fail-closed-contract.md)
  — the broader order-smoke fail-closed contract; §4 states the underlying
  `AmbiguousOrder`=may-rest / `ApiError`=placed-nothing rule this convention
  operationalizes for a classifier.
- [`kill-switch-ordering-in-order-placing-teardown`](kill-switch-ordering-in-order-placing-teardown.md)
  — engage the kill switch AFTER the teardown; the fail-closed arm here obeys it.
- [`ls-paper-01491-account-not-order-capable`](../integration-issues/ls-paper-01491-account-not-order-capable.md)
  — the `01491` capability code this classifier isolates.
- [`ls-gateway-igw40011-numeric-request-fields`](../integration-issues/ls-gateway-igw40011-numeric-request-fields.md)
  — why IGW40011 is an HTTP-500 (hence an `AmbiguousOrder` on the order path).
- [`authoring-fo-order-tr-chain`](authoring-fo-order-tr-chain.md) — the F/O order
  chain authoring recipe this classifier plugs into.
