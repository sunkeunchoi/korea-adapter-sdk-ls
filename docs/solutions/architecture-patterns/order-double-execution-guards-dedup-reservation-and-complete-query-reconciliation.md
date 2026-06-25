---
title: "Two double-fill guards for order dispatch: the dedup in-flight reservation and the complete-query reconciliation gate"
date: 2026-06-25
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
tags:
  - orders
  - order-safety
  - deduplication
  - reconciliation
  - double-fill
  - concurrency
  - pagination
  - ls-core
  - ls-sdk
---

# Two double-fill guards for order dispatch

## Context

The order runtime's cardinal sin is a **double fill** — placing the same
irreversible order twice. The first order package (`CSPAT00601` submit +
`t0425` reconciliation read, `crates/ls-sdk/src/orders/`) shipped a no-retry
`post_order`, an `OrderDeduplicator`, and a six-state reconciliation matcher.
A code review then found **two independent double-fill vectors** that the
obvious implementations of dedup and reconciliation both contain. Both are
subtle, both passed the first round of tests, and both generalize to any system
that guards an irreversible action with a cache + a state-query.

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

## Why This Matters

Both bugs share a shape: **a guard that concludes "didn't happen / safe to
proceed" from incomplete information.** For an idempotent-but-irreversible
action, the failure is asymmetric — a false "already done" is harmless (you
skip a real action and reconcile), but a false "not done / safe to retry"
causes the irreversible double action. So every guard must **fail toward the
not-safe conclusion**: an unfinished query, a contended cache slot, a degenerate
key, or an unmatchable field is treated as "cannot prove absence," never as
"proven absent." The default implementations (cache-completed-only,
query-first-page, trust-the-key, AND-all-fields) all silently fail toward the
*dangerous* conclusion.

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
  predicate.
- `crates/ls-sdk/src/orders/mod.rs` — `Orders::reconcile` drives `collect_all`
  over `t0425` and treats a `collect_all` error (failed or `PaginationLimit`
  truncation) as not-safe.

Regression tests pinning each: `post_order_concurrent_identical_submits_dispatch_exactly_once`
(ls-core), `no_match_on_a_non_terminal_page_is_not_safe_to_retry` and
`empty_or_zero_order_number_falls_through_to_field_corroboration` (ls-sdk).
