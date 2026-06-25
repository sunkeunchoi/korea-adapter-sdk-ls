---
title: "Order-dispatch reconciliation safety guards: dedup in-flight reservation, complete-query gate, and action-aware matching"
date: 2026-06-25
last_updated: 2026-06-25
category: architecture-patterns
module: crates/ls-core, crates/ls-sdk
problem_type: architecture_pattern
component: orders
severity: high
applies_when:
  - "Building or reviewing an OrderDeduplicator (or any idempotency cache for an irreversible action)"
  - "Writing reconciliation that queries broker/exchange state to decide whether an ambiguous send is safe to retry"
  - "Caching only completed responses to dedup repeat requests"
  - "Concluding 'no matching record found, safe to retry' from a paginated read"
  - "Reconciling an action that references an EXISTING record (a modify/cancel) by scanning rows and classifying state"
tags:
  - orders
  - order-safety
  - deduplication
  - reconciliation
  - double-fill
  - wrong-verdict
  - concurrency
  - pagination
  - ls-core
  - ls-sdk
---

# Order-dispatch reconciliation safety guards

## Context

The order runtime's cardinal sin is a **double fill** — placing the same
irreversible order twice. The first order package (`CSPAT00601` submit +
`t0425` reconciliation read, `crates/ls-sdk/src/orders/`) shipped a no-retry
`post_order`, an `OrderDeduplicator`, and a six-state reconciliation matcher.
A code review then found **two independent double-fill vectors** that the
obvious implementations of dedup and reconciliation both contain (guards 1–2
below). Both are subtle, both passed the first round of tests, and both
generalize to any system that guards an irreversible action with a cache + a
state-query.

A later wave added order **modify**/**cancel** (`CSPAT00701`/`CSPAT00801`) and
restructured the matcher to be **action-aware** (a modify/cancel references an
*existing* order by number, unlike a submit's "did a new order appear?"). A
review of that restructure found a third vector of the same family — a
wrong-verdict in the matcher's row classification (guard 3 below).

## Guidance

### 1. A response-cache deduplicator needs an *in-flight reservation*, not just a completed-response cache

The natural dedup design caches the **completed** response under a request key,
so a repeat request returns the cached result instead of re-executing. That
protects **sequential** repeats but has a TOCTOU hole for **concurrent** ones:

```
task A: get(key) -> miss ─┐
task B: get(key) -> miss ─┤  both miss: A hasn't inserted yet (its dispatch is in flight)
task A: dispatch ─────────┤
task B: dispatch ─────────┘  TWO live orders hit the exchange
task A: insert(key, resp)
```

The cache only ever holds *finished* work, so the window between "A's get-miss"
and "A's insert" is wide open. Close it by reserving the key **atomically at
get-miss time, before dispatch**, and releasing it on every exit:

```rust
// after the cache get() misses:
let _reservation = match self.order_dedup.try_reserve(&dedup_key) {
    Some(guard) => guard,                       // we own the in-flight slot
    None => return Err(LsError::DuplicateOrder), // a concurrent identical send is in flight
};
// ... rate limit -> single dispatch -> cache the success ...
// _reservation drops on ANY exit (Ok, rejection, ambiguity, panic) -> releases the key
```

```rust
pub fn try_reserve(&self, key: &str) -> Option<ReservationGuard<'_>> {
    use dashmap::mapref::entry::Entry;
    match self.in_flight.entry(key.to_string()) {   // atomic per shard
        Entry::Occupied(_) => None,                 // already dispatching -> duplicate
        Entry::Vacant(v) => { v.insert(()); Some(ReservationGuard { dedup: self, key: key.to_string() }) }
    }
}
```

Key points: the reservation is a **separate set** from the response cache; the
guard releases via `Drop` so no exit path leaks a stuck reservation; and a
sequential repeat still takes the cache-hit path (the first call cached before
releasing), so only genuinely-concurrent duplicates hit the `DuplicateOrder`
branch.

### 2. Reconciliation may only declare "safe to retry" from a *complete* query

After an ambiguous send (timeout/5xx — the exchange may or may not have filled),
reconciliation queries exchange state and concludes "no matching order found →
safe to retry." That conclusion is only valid over a **complete** result set. A
single page of a paginated read does **not** prove absence:

```rust
// WRONG: a single page can't prove the order isn't on page 2+
let resp = self.inquiry(&T0425Request::for_symbol(symbol)).await; // one page
reconcile(intent, resp.as_ref().ok(), dedup_hit) // no-match -> safe_to_retry=true  ← double fill
```

Two fixes, applied together:

- **Exhaust every page** before matching (drive `collect_all`/the continuation
  cursor), and union all rows.
- **Gate `safe_to_retry` on query completeness.** A failed *or truncated* query
  fails toward `Unknown` + not-safe — proven-absent is reserved for a no-match
  over a query that genuinely finished.

```rust
let pages = self.inner.collect_all(base, |req| /* post_paginated t0425 */).await;
match pages {
    Ok(pages) => reconcile_rows(intent, &union(pages), /* query_complete */ true, false),
    Err(_)    => ReconcileOutcome { state: Unknown, safe_to_retry: false }, // truncated/failed -> not proven
}
```

Two adjacent matcher details that cause the same "false safe-to-retry → double
fill" if wrong:

- **Don't trust a degenerate key.** An empty/zero order number from a partial
  ack must not be used as the match key (it would spuriously equal a blank row);
  fall through to field corroboration.
- **Drop a field predicate that legitimately won't match.** A market order
  submits price `0` while the venue row carries the executed price, so requiring
  price equality wrongly excludes a landed market order. Skip the price check
  when the intent price is zero/empty (match on the remaining fields) —
  dropping a predicate makes the matcher *more* likely to find the order, which
  is the safe direction.

### 3. An action-aware matcher must scan all rows, take the strongest classification, and only count a "child" as a landed witness if the child is itself live

When the action references an *existing* record (a modify/cancel keyed off the
original order number `OrgOrdNo`), the matcher reads broker rows and classifies
the outcome. Two traps, both found in review:

- **Don't early-return on the first matching row.** A landed modify can leave the
  *original* row at `접수` (received), which classifies `Accepted`. A first-row
  early-return would read that as "landed/accepted" and mislabel an un-applied (or
  differently-resolved) transition. **Scan every matching row and take the
  strongest classification** — a `취소`/`정정`/`거부` transition row outranks a
  still-`접수` original.

- **A "child" row is only a landed witness if the child is itself live.** A modify
  creates a *new* order number, so its child row (`orgordno == OrgOrdNo`) is
  evidence the modify landed — but **only if that child's own status is live**
  (`접수`/`체결`/`정정` → Accepted/Modified), never `취소`/`거부`. The naive
  predicate counted *any* child:

```rust
// WRONG: any child of the original counts as a landed modify
let has_child = matched.iter().any(|r| r.orgordno == refno);
// (a) a 취소 child (the order was CANCELED) reads as Modified;
// (b) a landed live child + a 정정거부 sibling -> `has_child && !any_rejected`
//     is false -> else-branch Rejected + safe_to_retry=TRUE while a child rests.
if any_modified || (has_child && !any_rejected) { Modified } else { /* safe_to_retry: true */ }
```

```rust
// RIGHT: a child counts only when it is itself a LIVE order, and the
// strongest transition is decided first (cancel outranks modify).
let has_live_child = matched.iter().any(|r|
    r.orgordno == refno
    && matches!(classify_status(&r.status), OrderState::Accepted | OrderState::Modified));
match action {
    Modify => {
        if any_canceled            { Canceled  /* the order is gone */ }
        else if any_modified || has_live_child { Modified /* safe_to_retry: false — an order rests */ }
        else if any_rejected       { Rejected; safe_to_retry: true /* idempotent-by-target re-send */ }
        else                       { Unknown;  safe_to_retry: true }
    }
    // Cancel INVERTS the risk: Canceled ONLY on an explicit 취소 row; every other
    // outcome fails toward still-live (never Accepted, never clears retry).
}
```

The asymmetry is deliberate and must not be "aligned away": a rejected **modify**
is `safe_to_retry: true` (re-applying the same absolute target is idempotent), but
a rejected **cancel** is `safe_to_retry: false` (the order may still rest). A live
child must never be masked by a rejected sibling into a retry-inviting verdict.

## Why This Matters

All three bugs share a shape: **a guard that concludes "didn't happen / safe to
proceed" (or mislabels what *did* happen) from a partial or first-match read.** For an idempotent-but-irreversible
action, the failure is asymmetric — a false "already done" is harmless (you
skip a real action and reconcile), but a false "not done / safe to retry"
causes the irreversible double action. So every guard must **fail toward the
not-safe conclusion**: an unfinished query, a contended cache slot, a degenerate
key, or an unmatchable field is treated as "cannot prove absence," never as
"proven absent." The default implementations (cache-completed-only,
query-first-page, trust-the-key, AND-all-fields, first-match-row,
any-child-counts) all silently fail toward the *dangerous* conclusion — either a
double action or a wrong-verdict that invites one.

## When to Apply

- Any deduplication cache fronting an irreversible/side-effectful operation —
  add an in-flight reservation, not just a completed-response cache.
- Any reconciliation/idempotency check that reads remote state and decides
  whether to (re)issue an action — exhaust pagination and gate the safe verdict
  on query completeness.
- Any matcher whose miss authorizes a retry — audit each match predicate and key
  for inputs that make it *fail to match a record that exists* (degenerate keys,
  fields the two sides represent differently), and make those fall toward
  not-safe.
- Any **action-aware** matcher reconciling an action against an *existing* record
  (modify/cancel/amend) — scan all rows and take the strongest classification
  (never first-match-row), count a derived/child record as a landed witness only
  when the child is itself live, and keep the per-action `safe_to_retry` asymmetry
  (an idempotent action's rejection may be safe to re-send; an inverted-risk
  action's must not).

## Examples

In this repo:

- `crates/ls-core/src/order_dedup.rs` — `try_reserve` / `ReservationGuard`
  (the in-flight set is separate from the response cache); the read-path
  eviction also uses `remove_if` (re-check expiry under the shard lock) rather
  than a bare `remove`, to avoid deleting a freshly-inserted entry in a race.
- `crates/ls-core/src/inner.rs` — `post_order` reserves after the cache miss,
  before rate-limit/dispatch; returns `LsError::DuplicateOrder` on a concurrent
  conflict.
- `crates/ls-sdk/src/orders/reconcile.rs` — `reconcile_rows(intent, rows,
  query_complete, dedup_hit)` gates `safe_to_retry` on `query_complete`;
  `row_matches` drops a degenerate order-number key and the market-order price
  predicate; the action-aware branches scan all matched rows, and `has_live_child`
  counts a child (`orgordno == OrgOrdNo`) only when its own status classifies
  `Accepted`/`Modified` (a `취소`/`거부` child is not a landed-modify witness).
- `crates/ls-sdk/src/orders/mod.rs` — `Orders::reconcile` drives `collect_all`
  over `t0425` and treats a `collect_all` error (failed or `PaginationLimit`
  truncation) as not-safe.

Regression tests pinning each: `post_order_concurrent_identical_submits_dispatch_exactly_once`
(ls-core), `no_match_on_a_non_terminal_page_is_not_safe_to_retry` and
`empty_or_zero_order_number_falls_through_to_field_corroboration` (ls-sdk, guards
1–2); `modify_with_bare_jeopsu_original_is_not_landed_safe_to_retry`,
`cancel_scans_all_rows_and_takes_strongest_canceled`,
`modify_with_canceled_child_classifies_canceled_not_modified`, and
`modify_with_landed_child_and_rejected_sibling_stays_modified` (ls-sdk, guard 3).
