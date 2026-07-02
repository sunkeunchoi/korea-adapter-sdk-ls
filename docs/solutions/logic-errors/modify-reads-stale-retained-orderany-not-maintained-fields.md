---
title: "A modify that reads qty/price off a retained OrderAny (never rewritten on modify) resurrects a prior size reduction"
date: 2026-07-02
category: logic-errors
module: nautilus-ls adapter — execution client modify/cancel path + fill ledger (adapters/nautilus)
problem_type: logic_error
component: tooling
symptoms:
  - "A price-only re-modify (Nautilus ModifyOrder with quantity=None) re-sends the ORIGINAL order quantity, silently undoing an earlier quantity reduction and re-increasing live exposure"
  - "Fill emissions carry the correct current limit price (from ledger.limit_price()), but the modify request's OrdQty/OrdPrc lag — the two paths read from different sources of truth"
  - "No test caught it: the first modify (which sets cmd.quantity=Some) works; only a SECOND, quantity-omitting modify exposes the stale read"
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - order-safety
tags:
  - nautilus-adapter
  - order-modify
  - fill-ledger
  - order-any
  - stale-state
  - source-of-truth
  - ls-orders
  - order-safety
---

# A modify that reads qty/price off a retained OrderAny (never rewritten on modify) resurrects a prior size reduction

## Problem

The nautilus-ls execution client's fill ledger retains a Nautilus `OrderAny` per
order **purely as emission identity** (every `ExecutionEventEmitter::emit_*` method
takes `&OrderAny`). The retained `OrderAny` is **never rewritten on a modify** —
current quantity/price live in separate ledger fields (`order_qty` / `limit_price`)
maintained by `note_modify`. The `modify_order`/`cancel_order` workers built their
outbound request off `snapshot()`, which read qty/price from the **retained
`OrderAny`** instead of those maintained fields. A price-only re-modify (the normal
Nautilus reprice shape: `ModifyOrder.quantity == None`) therefore fell back to the
**original** quantity and silently reverted a prior size reduction — an order-safety
defect that re-increases live exposure.

## Symptoms

- Submit qty 10 → modify to qty 5 (works) → modify price-only → the venue request
  carries `OrdQty = 10`, not 5.
- `emit_order_filled` used the *correct* current price (`led.limit_price()`), while
  the modify request lagged — the two paths disagreeing was the tell that one read a
  stale source of truth.
- Caught in code review, not by the suite: existing tests only exercised a **single**
  modify (which always passes `cmd.quantity = Some(..)`), so the fallback branch was
  never hit.

## What Didn't Work

The original design *looked* self-consistent: the ledger stored `order_qty` /
`limit_price` AND the `OrderAny`, and `note_modify` dutifully updated the two scalar
fields. The trap was assuming the retained `OrderAny` also reflected the modify — it
does not, because rebuilding an `OrderAny` from a Nautilus `OrderUpdated` event is
non-trivial and the ledger deliberately keeps it as immutable emission identity only.
Two sources of truth for the same fact (order qty), one of them frozen, guarantees
eventual divergence.

## Solution

Read current qty/price from the ledger's `note_modify`-maintained fields, with the
retained `OrderAny` only as a first-modify fallback. Added an `order_qty()` accessor
to mirror the existing `limit_price()`:

```rust
// adapters/nautilus/src/orders/ledger.rs
/// Current quantity (maintained by `note_modify`); the retained OrderAny does not
/// reflect a modify.
pub fn order_qty(&self, client_order_id: &ClientOrderId) -> Option<i64> {
    self.entries.get(client_order_id).map(|e| e.order_qty)
}
```

```rust
// adapters/nautilus/src/execution.rs — snapshot()
// BEFORE (stale — reads the frozen retained OrderAny):
qty:   order.quantity().as_f64() as i64,
price: order.price().map(|p| p.as_f64() as i64).unwrap_or(0),

// AFTER (reads the note_modify-maintained fields; OrderAny only as fallback):
qty:   led.order_qty(client_order_id).unwrap_or_else(|| order.quantity().as_f64() as i64),
price: led.limit_price(client_order_id).unwrap_or_else(|| order.price().map(|p| p.as_f64() as i64).unwrap_or(0)),
```

## Why This Works

`note_modify` is the single writer of the order's current qty/price, so making it the
single **read** path removes the divergence. The retained `OrderAny` reverts to what
it was always meant to be — emission identity (strategy/instrument/side/client id),
which a modify never changes — and is used only if no ledger entry exists yet
(defensive fallback). The poll lane already did the right thing (`led.limit_price()`
for the fill basis), so aligning the modify path just makes the two consistent.

## Prevention

- **One mutable fact, one read path.** When a struct keeps an immutable "template /
  identity" object alongside separately-mutated scalar state, never read the mutable
  fact off the template. Either refresh the template on every mutation, or (cheaper)
  make the maintained field the *only* accessor and treat the template as
  identity-only. Here the template (`OrderAny`) is expensive to rebuild, so the
  field is authoritative.
- **Test the SECOND mutation, not just the first.** The bug was invisible to a
  single-modify test because the first modify always supplies the new value
  explicitly; only a follow-up mutation that *omits* the field exercises the stale
  fallback. Regression test added — submit qty 10 → modify to 5 → **price-only**
  re-modify → assert the outbound `CSPAT00701` `OrdQty == 5`:

  ```rust
  // tests/execution_client.rs
  let body = last_request_body(&server, "CSPAT00701").await;
  assert_eq!(body["CSPAT00701InBlock1"]["OrdQty"].as_i64(), Some(5),
      "a price-only re-modify must keep the reduced qty, not resurrect the original 10");
  ```
- **A cross-path disagreement is a smell.** Two code paths that should agree on a
  fact (fill emission vs. modify request, both needing the current price) reading it
  from different places is the signature of a stale-source bug — chase it.
