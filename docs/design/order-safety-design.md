# Order-Safety Design Contract

**Status:** evidence ran — Implemented + Recommended (updated 2026-06-30). The
order package built the runtime this contract requires — `Inner::post_order`
(no-retry, §1), the `OrderDeduplicator` (§2), the global kill switch (§1), the
order success predicate (§1), the redaction/tracing contract (§5), the six-state
reconciliation matcher (§3), the re-added `LsError::DuplicateOrder` variant, and
the guarded paper-order evidence harness (§4, `make live-smoke-order` /
`make live-smoke-order-chain`) — and the guarded **live paper order** runs that
gate Implemented and Recommended have now both happened in-window:
`CSPAT00601` / `CSPAT00701` / `CSPAT00801` + `t0425` are **Implemented**
(plan 2026-06-25-005, observed `00040`/`00462`/`00463`) and **Recommended**
(plan 2026-06-30-002, a fresh clean in-window order-chain with the account
positively confirmed flat afterward). The §1 accepted codes are confirmed
against observed live codes, no longer seed-only. ADR 0008's deferral is retired.
The evidence was sourced from the autonomous `live-smoke-order-chain` (the
`live-smoke-order` matrix's marketable scenario fills on an open market and would
leave a position needing an out-of-band paper reset). The order logic also stays
proven against mocks (`crates/ls-sdk/tests/order_logic_gate.rs`).

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

The authoritative runtime seam is:

- `EndpointPolicy.is_order` distinguishes order endpoints from non-order
  endpoints. It is the structured fact runtime guards consume; a future macro
  or code generator can produce it, but neither macro text nor generated-source
  layout is the safety contract.
- A dedicated `post_order` dispatch path issues one HTTP attempt, uses the
  `Orders` rate bucket, and checks the order policy before dispatch.
- `post_order` rejects a non-order policy before any HTTP call. This is a
  defense-in-depth guard against accidentally routing market data or account
  inquiries through the order path.

A runtime kill switch is also required before the first order runtime package:
`set_orders_enabled(false)` or its equivalent MUST halt all order dispatch
before dedup lookup, rate limiting, or HTTP I/O. Non-order dispatch remains
unaffected. The kill switch is the operator's emergency halt; reconciliation
must not silently re-enable it.

Order response classification is separate from read-only query classification.
The read-only success predicate (`00000`, empty, `00136`, `00707`) is
insufficient for orders. The Migration Source observed domestic-stock order
acknowledgements where successful `CSPAT00601` requests returned `00039`
(sell) or `00040` (buy). The future order runtime MUST define an explicit
order success predicate before implementation and MUST keep rejected orders as
`LsError::ApiError` with the broker code/message preserved.

## 2. Deduplication cache with opportunistic expired-entry sweeps

Idempotency is enforced by an `OrderDeduplicator`: a key built from
`account_no + tr_code + the strong order fields` (per TR metadata
`dependencies.strong_order_fields` — for `CSPAT00601`: `IsuNo`, `BnsTpCode`,
`OrdQty`, `OrdPrc`) maps to the cached response within a TTL window. A second
submission of the same order inside the window returns the cached result
instead of hitting the exchange again.

The concrete migrated contract is stricter than "some cache":

- The key is `SHA256(account_no + ":" + tr_code + ":" + canonical request
  JSON)`. Account identity and TR code are part of the key, so different
  accounts and different order TRs do not collide.
- The default TTL is 300 seconds. This is a duplicate-submission safety window,
  not a server-side idempotency guarantee.
- The cache is per SDK client instance. Multiple clients for the same account
  do not share dedup state; applications should use one client per account order
  flow when duplicate protection matters.
- Serialization failure while building the key is fail-closed. It returns an
  error and no order is dispatched.
- A cache hit returns the cached response and bypasses rate limiting, HTTP
  dispatch, and broker processing.
- Different request fields, even small changes such as quantity or price, are
  different orders and intentionally miss the cache.

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
- **The sweep holds no per-entry guard.** The `retain` pass must run with **no
  DashMap entry guard held** — the deadlock-avoidance rule carried over from
  exact-key eviction. Running the retain pass while holding a per-entry guard
  would deadlock. This is a decision-relevant concurrency-correctness constraint,
  not implementation taste.

(Grounded in the Migration Source learning
`docs/solutions/performance-issues/order-dedup-cache-opportunistic-eviction.md`.)

## 3. Reconciliation

Because dispatch is no-retry (§1), ambiguous failures are expected and must be
*resolved*, not swallowed. Before the order runtime ships it must pair with a
reconciliation path that, after an ambiguous send, queries order/execution
state from the broker and reconciles the local intent against what the exchange
actually recorded — so an order that "failed" locally but landed at the venue is
detected rather than silently resubmitted.

The maintained order state model is:

| State | Meaning | Evidence |
|---|---|---|
| Accepted | LS accepted the order for processing | Order success `rsp_cd` and populated order response |
| Rejected | LS rejected the order | `LsError::ApiError` with broker code/message |
| Duplicate | SDK returned a cached response for an identical request | `dedup_hit=true`, no new dispatch |
| Modified | A pending order was changed by a modify TR | Successful modify response plus follow-up inquiry |
| Canceled | A pending order was canceled by a cancel TR | Successful cancel response plus follow-up inquiry |
| Unknown | SDK cannot prove accepted versus not accepted | transport failure, timeout, crash, or interrupted/decode-failed response |

The recommended reconciliation flow is:

1. Stop automatic retry for the ambiguous order TR.
2. Record local evidence: timestamp, TR code, request hash if available,
   account, symbol, side, quantity, price, and error.
3. Query read-only order/account state.
4. Match possible orders by account, symbol, side, quantity, price, time window,
   and any known order number.
5. Classify the outcome as accepted, rejected, duplicate, modified, canceled, or
   still unknown.
6. Retry only after reconciliation proves no matching order was accepted, or
   after an operator explicitly accepts duplicate-order risk.

For the first domestic-stock package, `CSPAT00601` and `t0425` ship together:
`CSPAT00601` produces the order number and `t0425` is the read-only
order/execution inquiry used to observe state after ambiguous outcomes.

The metadata contract for TRs that consume a required order number is deliberately
coarse for now: record the order-number request field in `strong_order_fields`
and the acceptable prerequisite producer TRs in `prerequisite_producer_trs`.
Field-level edges such as `OrgOrdNo <- CSPAT00601.OrdNo` are deferred until a
consumer needs that precision.

```yaml
dependencies:
  self_continuation_fields: []
  strong_order_fields: [OrgOrdNo]
  prerequisite_producer_trs: [CSPAT00601, t0425]
```

## 4. Guarded manual evidence

Order TRs carry `certification_path: manual`. Their focused evidence is
**guarded manual evidence**: it requires explicit operator confirmation and is
never run as part of the automated Change-Scoped Gate. The gate proves order
*logic* (no-retry semantics, dedup, reconciliation) against mocks; it never
submits a live order. Live evidence is an operator-initiated, deliberately
out-of-band step.

**The Implemented gate for an order TR is a single guarded live paper order**
(not the automated Paper Live Smoke every read-only TR uses, and not the
realtime class's lifecycle-reachability gate). An order TR flips to Implemented
only after an operator runs the guarded paper-order evidence matrix
(`make live-smoke-order`) out-of-band and records a clean, credential-free
result — a resting far-from-market limit buy and sell, one marketable order, and
one deliberate rejection — pinning the §1 accepted-code set from observed real
`rsp_cd` codes. If the paper account cannot place an order in-window, the run
records **Pending** and the TR stays callable-but-unconfirmed; the machinery
still ships. This strengthens the older posture (which reserved manual evidence
for Recommended): a guarded paper order is now the order class's Implemented bar.

Manual evidence must fail closed:

- A selected order TR must be explicit; an unset selection must not default to
  submitting an order in a live/manual path.
- Operator-provided order parameters must be validated before SDK construction
  or dispatch.
- Invalid parameters and missing pending-order numbers produce structured
  "not certified" evidence rather than attempting a best-effort order.
- Production order tests are prohibited. Manual evidence is paper-only unless a
  separate, explicit safety decision changes that policy.
- Evidence must include the order TR, result classification, request summary,
  broker `rsp_cd`/`rsp_msg`, any order number/time, reconciliation/status
  observation when applicable, and a statement that production order testing was
  not run. Credentials and account-sensitive data are excluded or redacted.
- **Freshness:** reconciliation / manual evidence must be no older than **7 days**
  before tagging a release, unless the release owner explicitly accepts the risk.
  (Provenance: `korea-broker-sdk-ls/docs/ORDER_RECONCILIATION_DESIGN.md`.)

## 5. Order redaction and tracing contract

The future order runtime must carry the Migration Source's redaction/tracing
contract; it is a safety constraint, not just observability hygiene. (Provenance:
`korea-broker-sdk-ls/docs/ORDER_SAFETY_DESIGN.md` Redaction section.)

- All public dispatch methods use `instrument(skip_all)`, so no function
  parameters are auto-recorded into spans.
- The order dispatch span records **only** non-credential structural fields
  (`tr_code`, `path`, `category`, `dedup_hit`) and **never** records credential
  field-names (the app key, app credential, account number, access token) nor the
  request body.
- Credential types implement `Debug` with a redacted substitution, and generated
  order request/response types contain no credential fields.
- Order **response** types that may carry account-level data (order numbers,
  account references) are **not** auto-redacted. Operators must review response
  content before logging or persisting order evidence — the redaction guarantee
  covers credentials, not account-level order data.

## What ships now vs later

- **Landed (first order runtime package, 2026-06-25):** the order runtime
  machinery is in `ls-core`/`ls-sdk` — `Inner::post_order` (no-retry), the
  `OrderDeduplicator` (the §2 eviction contract), the global kill switch, the
  order success predicate, the redaction/tracing contract (§5), the six-state
  reconciliation matcher, and the re-added `LsError::DuplicateOrder` variant.
  `CSPAT00601` (submit) and `t0425` (the reconciliation read) are callable
  through the `Orders` handle, and the guarded paper-order evidence harness
  exists (`make live-smoke-order`). The order logic is proven against mocks
  (`order_logic_gate`).
- **Pending (the Implemented flip):** `CSPAT00601` and `t0425` remain
  `tracked: true, implemented: false` in `TR Maintenance Metadata` until a clean
  in-window guarded paper-order run (§4). The §1 accepted codes
  (`00039`/`00040`) are the seed-only/unconfirmed hypothesis until that run
  observes the real surface. A clean run flips both TRs and supersedes ADR 0008.
- **Later (after the first package):** domestic-stock modify/cancel TRs such as
  `CSPAT00701` and `CSPAT00801` can be considered only after the SDK can safely
  produce and reconcile the order numbers they consume — and are implemented via
  the frozen `.agents/skills/implement-order-tr/` recipe.
