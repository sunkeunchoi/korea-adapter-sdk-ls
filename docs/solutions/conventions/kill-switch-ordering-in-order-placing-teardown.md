---
title: "Order kill switch vs an order-placing teardown: engage the halt AFTER the close, never before"
date: 2026-07-01
category: conventions
module: crates/ls-sdk, crates/ls-core
problem_type: convention
component: orders
severity: high
applies_when:
  - "Writing a fail-closed order-smoke teardown that FLATTENS a position by placing a close order (not just cancelling/reading)"
  - "Calling set_orders_enabled(false) (the kill switch) near a code path that still needs to submit an order"
  - "Reviewing an order harness whose loud failure claims it 'flattened best-effort' but may not have"
  - "Manufacturing a transient F/O position and closing it via an opposite-side marketable order"
---

# Order kill switch vs an order-placing teardown

## Context

`ls_core`'s order kill switch — `sdk.inner().set_orders_enabled(false)` — is the
operator emergency halt. It is checked **first** inside `Inner::post_order`
(`crates/ls-core/src/inner.rs`), before dedup, rate limiting, or any I/O: once
engaged it makes every subsequent order dispatch return
`Err(ApiError { code: "orders-disabled", .. })` and place nothing. It is sticky
(a plain `AtomicBool`, never auto-reset for the rest of the run).

That semantics is exactly right for a **read-or-cancel** teardown (e.g. the flat
chain's `fo_assert_no_fill`, which only *reads* `t0441`): engage the kill switch as a
"no new orders" guard, then read to confirm flatness. Nothing the guard blocks is
needed.

It is a **trap** for a teardown that flattens by **placing a close order**. The
`fo_position_manufacture_smoke` harness (plan 2026-07-01-003) manufactures a real F/O
position and flattens it with an opposite-side *marketable* close — `fo_orders().submit()`,
which routes through `post_order`. Several fail-closed error paths (ambiguous buy,
non-clean ack, mid-poll read failure, no-fill-timeout with a non-clean cancel) engaged
the kill switch **before** calling the flatten helper. The result: the close submit was
guaranteed to be rejected `orders-disabled`, the flatten became a no-op, and the loud
panic still said "flattened best-effort" — telling the operator the position was closed
when the close never dispatched. That is worse than fail-loud: it is fail-loud with a
misleading state claim, on exactly the paths most likely to have a real open position.

## Guidance

**Engage the kill switch AFTER every close/cancel/confirm attempt the teardown needs,
never before.** Let the order-placing teardown run with dispatch still enabled; have the
teardown helper itself engage the kill switch only on its own terminal failure arms
(after the close attempt is exhausted).

```rust
// WRONG — the kill switch blocks the very close the flatten must place.
sdk.inner().set_orders_enabled(false);
fo_flatten_fail_closed(&sdk, &band, &contract, &ordnos).await; // submit() -> orders-disabled

// RIGHT — flatten with dispatch enabled; the helper halts itself only when it
// truly cannot flatten (after its close/cancel/reflatten attempts).
fo_flatten_fail_closed(&sdk, &band, &contract, &ordnos).await;
// (no pre-emptive kill switch here; the helper owns terminal set_orders_enabled(false)+panic!)
```

Corollaries that fell out of the same review:

- **Size and side the close from a SIGNED `t0441` read, not a magnitude.** F/O 잔고 can
  be short. A magnitude-only witness certifies a stray short as a "full fill" and then
  "closes" it with another sell, deepening the short. Certify only a *long* of the
  submitted qty; derive the close side from the sign (sell a long, buy a short).
- **Preflight-assert the account is flat before manufacturing.** The witness/close logic
  assumes any post-buy position is the one you just made; a pre-existing position
  invalidates that. Refuse to manufacture on a non-flat book.
- **"Cancel-then-reflatten" must actually reflatten.** If the close *rests* instead of
  filling, cancelling it and then panicking leaves the original position open. Cancel the
  rested close first (so two closes can't stack into an over-close), then submit one
  genuine second close, re-sizing from a fresh `t0441` each attempt — bounded to at most
  two attempts so it never loops.
- **Never record "confirmed flat" off a non-clean cancel.** `t0441` sees *fills* only,
  never a resting order — so an un-acked cancel of a resting order leaves removal
  unconfirmed. Fail loud (mirror the chain's teardown-uncertain), don't print success.

## Why This Matters

A fail-closed harness earns trust from the invariant "it never exits, or claims flat,
with a position possibly open." The kill-switch-before-close ordering silently breaks
that on its highest-risk paths while *reading* as extra-safe (an emergency halt looks
conservative). The bug is invisible in offline tests — the kill switch and `post_order`
only interact against a live gateway — so it survives to the one in-window operator run
that actually places money-shaped orders. Getting the ordering right is what makes the
auto-flatten real instead of decorative.

## When to Apply

Any order-placing teardown in `crates/ls-sdk/tests/order/` (or future order harnesses):
if a code path both (a) may engage `set_orders_enabled(false)` and (b) later needs to
`submit()`/`modify()`/`cancel()` an order, the halt must come last. A read-only or
cancel-is-not-needed teardown (the flat chain's `fo_assert_no_fill`) may keep the kill
switch first — the distinction is whether the teardown still needs order dispatch.

See also [autonomous order-smoke fail-closed contract](../architecture-patterns/autonomous-order-smoke-fail-closed-contract.md)
(the broader autonomy/scrub/flatness contract) and
[authoring the F/O order TR chain](authoring-fo-order-tr-chain.md) (the two-part F/O
flatness the flat chain uses).
