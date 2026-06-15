# Order-Safety Design Contract

**Status:** design-only. No order runtime ships in the first vertical slice
(ADR 0008). This note records the contract the order runtime MUST satisfy
before it lands, so the metadata-tracked order class (`CSPAT00601`, and the
~order TRs that follow) has a written safety bar to build against.

The order class is the one place where a bug is not a stale quote but a real,
irreversible market action. Everything below exists to make a duplicate or
phantom order structurally hard, not merely unlikely.

## 1. No-retry dispatch

Order submission MUST NOT ride the generic `post` retry path. A transport
timeout or 5xx on an order call is **ambiguous** — the exchange may or may not
have accepted the order — so a blind retry risks a double fill. The order
dispatch path issues exactly one network attempt; on an ambiguous failure it
surfaces the ambiguity to the caller (and to reconciliation, §3) rather than
retrying. This is why `Inner::post`/`post_paginated` deliberately omit any
order path in this slice, and why a dedicated `post_order` is required before
any order TR is marked `implemented`.

## 2. Deduplication cache with opportunistic expired-entry sweeps

Idempotency is enforced by an `OrderDeduplicator`: a key built from
`account_no + tr_code + the strong order fields` (per TR metadata
`dependencies.strong_order_fields` — for `CSPAT00601`: `IsuNo`, `BnsTpCode`,
`OrdQty`, `OrdPrc`) maps to the cached response within a TTL window. A second
submission of the same order inside the window returns the cached result
instead of hitting the exchange again.

The eviction contract is specific, and it is the reason this is written down
rather than left to implementation taste:

- **Read-path lazy eviction is necessary but insufficient.** Evicting an
  expired entry only when its exact key is looked up again keeps repeated
  submissions correct, but a long-running client that submits many *distinct*
  orders would retain every expired entry for the life of the process.
- **Count-based sweeping is the wrong trigger.** It bounds high-volume bursts
  but misses the burst-then-idle flow, where stale entries sit untouched after
  activity stops.
- **A background sweeper is rejected.** The order-safety layer has a
  no-background-worker design; a sweeper thread is more moving parts than the
  runtime needs and contradicts that stance.
- **The contract: opportunistic sweep on the write path.** `insert` calls a
  monotonic `sweep_expired_if_due` before inserting. When the sweep interval
  has elapsed, one inserting thread wins an atomic timestamp update and runs a
  single bounded `retain` pass dropping entries outside the same TTL rule the
  read path uses. The read path stays simple; memory is bounded without a
  worker.

(Grounded in the Migration Source learning
`docs/solutions/performance-issues/order-dedup-cache-opportunistic-eviction.md`.)

## 3. Reconciliation

Because dispatch is no-retry (§1), ambiguous failures are expected and must be
*resolved*, not swallowed. Before the order runtime ships it must pair with a
reconciliation path that, after an ambiguous send, queries order/execution
state from the broker and reconciles the local intent against what the exchange
actually recorded — so an order that "failed" locally but landed at the venue is
detected rather than silently resubmitted.

## 4. Guarded manual evidence

Order TRs carry `certification_path: manual`. Their focused evidence is
**guarded manual evidence**: it requires explicit operator confirmation and is
never run as part of the automated Change-Scoped Gate. The gate proves order
*logic* (no-retry semantics, dedup, reconciliation) against mocks; it never
submits a live order. Live evidence is an operator-initiated, deliberately
out-of-band step.

## What ships now vs later

- **Now (this slice):** `CSPAT00601` exists in `TR Maintenance Metadata` as
  `tracked: true, implemented: false, recommended: false`, with
  `strong_order_fields` populated. No order code in `ls-core`/`ls-sdk`.
- **Later (order runtime follow-up):** `post_order` (no-retry),
  `OrderDeduplicator` (with the §2 eviction contract), reconciliation, the
  `LsError::DuplicateOrder` variant re-added, and per-class freshness windows
  tightened for orders. Only then does an order TR become `implemented`.
