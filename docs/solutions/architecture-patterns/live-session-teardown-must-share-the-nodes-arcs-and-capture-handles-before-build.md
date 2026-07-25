---
title: "A live-session teardown must SHARE the node's Arcs, and every handle must be captured before the builder"
date: 2026-07-25
category: architecture-patterns
module: adapters/nautilus (nautilus-ls, nautilus-ls-lab)
problem_type: architecture
component: live-session-driver
severity: critical
applies_when:
  - "Authoring a fail-closed teardown (kill switch / cancel-all / flatness) for a running nautilus LiveNode"
  - "Wiring a risk monitor or max-loss breaker that must read the node's real fills"
  - "Adding a supervisor that can end a session while the session can also end itself"
  - "Reviewing anything that calls set_orders_enabled(false) or reads a FillLedger from outside the exec client"
---

# A live-session teardown must share the node's `Arc`s, and its handles must be captured before the builder

## Context

`lab-live --mount` drives an attended rung-1 session: `authorize_mount` (consume the green
dispatch) → `node.run` → fail-closed teardown (`stop_emission → quiesce → cancel_all_resting →
is_flat → halt`, halt LAST) → `finalize_session`. All four safety acts are performed by a
**teardown handle** that lives outside the node, and by a watchdog that lives on its own OS
thread. Four things about that arrangement are silently wrong if you build them the obvious way.
Each failure mode is a **fail-open in a fail-closed system**: the code runs, reports success, and
protects nothing.

## The four traps

### 1. `halt()` on a separately-built client is a silent no-op

`halt()` is `sdk.inner().set_orders_enabled(false)` — it flips an `AtomicBool` on the SDK's
`Arc<Inner>`, and `post_order` checks that switch **first** for every order. Any clone of the same
`LsSdk` shares it (`LsSdk` is `#[derive(Clone)]` over `Arc<Inner>`).

But `LsExecutionClientFactory::create` used to build a **fresh `LsSdk`** from the config, so the
node's in-trader client held a *different* `Arc<Inner>`. A teardown that built its own client (the
`resolve_real_probes` shape) would therefore disable a switch nothing consults, while the node kept
placing orders. Nothing errors. The kill switch simply does not exist.

**Fix:** build ONE `LsSdk` outside the builder, construct the exec client yourself, and hand it to
a **stateful** factory (`LsExecutionClientFactory::with_client`, interior mutability because the
nautilus trait takes `&self`). Retain the same `LsSdk` on the teardown handle.

### 2. `sdk.clone()` does NOT carry the `FillLedger` — the breaker reads an empty ledger

The kill switch and the fill ledger are **different `Arc`s**. `Arc<Mutex<FillLedger>>` is created
fresh inside `LsExecClient::new`, so cloning the SDK shares the switch but not the fills. A max-loss
breaker built on a rebuilt handle reads an empty ledger, computes zero P&L, and never trips —
the same silent no-op, in the other safety act.

**Fix:** create the ledger outside too (`LsExecClient::new_with_ledger`) and retain that `Arc`.
Prove both with tests that mutate through the *in-node* handle and read through the *feeder* handle
(get an independent witness of what the node actually received — a shared factory the test still
holds — rather than asserting the caller's intent).

### 3. After `LiveNode::build()` there is no way back in

The exec client is type-erased into `Vec<LiveExecutionClient>` with no downcast, and the
`ExecutionClient` trait exposes neither `halt` nor cancel-all; `add_strategy` **moves** the
strategy into the trader. So the emission gate, the SDK, the ledger, the order-dispatch task set
and the heartbeat feeders must all be cloned **before** the builder — there is no retrieval path
afterwards. Make the build function return them (`LiveMount`), so forgetting one is a compile
error rather than a runtime discovery.

Also: `LsExecClient` is **not** `Clone` (it owns `JoinHandle`s), and its `Send + Sync` would ride on
unverified `ExecutionEventEmitter`/`WsSupervisor` bounds. The teardown handle must therefore be a
**purpose-built struct** over `Arc`-shared state, not a client clone — which is also what makes it
`Send + Sync` for the watchdog thread (assert that with a compile-time bound in a test).

### 4. `stop_emission` does not drain in-flight submits

`submit_order`/`modify_order`/`cancel_order` spawn their worker and **drop** the `JoinHandle`.
Closing the strategy's `EmissionGate` stops *new* signals; it does nothing about a submission
already in flight. That submission can reach the gateway *after* the cancel scan and *before*
`halt` — it passes the kill-switch check (still enabled at that point, correctly, because halt is
last) and rests an order the scan never saw. If `is_flat` also races ahead of the fill, the run
finalizes NORMAL with a live resting order. Sharpest on the watchdog-trip path, where the teardown
runs before `node.run`'s own drain.

**Fix:** retain the dispatch `JoinHandle`s (`OrderDispatchTasks`) and **quiesce** them — await with
a bounded budget, abandon past it — between `stop_emission` and the cancel scan. The budget matters:
a wedged dispatch must never stall the halt. Anything it rested past the budget is caught by the
scan, and an unconfirmed cancel fails the teardown closed (→ abnormal finalize).

## The arbiter must be the atomic claim, not a read

Two paths can end a session: `node.run` returning (timer/market close) and a watchdog trip. Both
must converge on **exactly one** teardown. The tempting post-run check is

```rust
if !latch.is_tripped() { run_teardown(...).await }   // WRONG
```

`is_tripped()` is a non-atomic load: it races the watchdog's claim, and both paths tear down. Use
the **same** `compare_exchange` the watchdog uses:

```rust
if latch.try_claim() { run_teardown(...).await }     // exactly one winner, however close the race
```

Two corollaries:

- A trip must also call `node.handle().stop()`, or `node.run` blocks forever after its remediation.
- Whoever wins the claim owns the report. Do **not** re-run a teardown to obtain one (that breaks
  exactly-once) and do not reconstruct one after the fact — give the tick function a reporting
  projection (`watchdog_tick_reporting`) so the winner hands its real `TeardownReport` back.
- Never `abort()` a supervisor task that might be mid-claim: a claimed latch with no teardown is
  precisely the state this design exists to prevent. Signal a stand-down flag and await it.

## The breaker's mark must have a floor, not a last price

`FillLedger` has no cost basis and no P&L — realized session P&L is a **new accounting seam**
(match offsetting fills against a running average basis), not a sum. And the watchdog thread has no
market-data access, so open positions are marked from a feed the strategy publishes. Marking at the
last-seen price is the trap: a market-data gap or a symbol halt accompanies exactly the fast adverse
moves the breaker exists to catch, so a stale-**favorable** mark under-reports the loss precisely
when it matters. Mark at the adverse edge with an explicit precedence:

| fresh close | stop level | mark (long) |
|---|---|---|
| yes | yes | `min(close, stop)` |
| no  | yes | `stop` (the floor takes over) |
| yes | no  | `close` |
| no  | no  | a configured worst-case adverse bound off the basis |

Test it against a **stale-favorable** fixture, not just a known-loss one.

## Where this lives

- `adapters/nautilus/src/execution.rs` — `new_with_ledger`, `OrderDispatchTasks` (+ `quiesce`),
  `cancel_all_resting_on`, `verify_flat_on`.
- `adapters/nautilus/src/factories.rs` — the stateful `LsExecutionClientFactory` (+ `handed()`).
- `adapters/nautilus/lab/src/runner/live.rs` — `LiveTeardownSession`, `LiveMount`,
  `build_live_session_node`, `run_live_session`, `prepare_mount`.
- `adapters/nautilus/lab/src/runner/pnl.rs` — the realized-P&L accounting seam + the adverse mark.
- Tests: `lab/tests/live_session.rs`, `lab/tests/live_wiring.rs`, `lab/tests/live_driver.rs`.

## Related

- [`kill-switch-ordering-in-order-placing-teardown.md`](../conventions/kill-switch-ordering-in-order-placing-teardown.md)
  — why halt is last, and why that is only safe when the teardown never *places*.
- [`safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md`](../logic-errors/safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md)
  — why the driver re-runs the fail-closed teardown after `node.run`'s own graceful shutdown, and
  why the ordering invariant is re-proven against the **real** session, not only the fake.
- [`order-error-classifier-placed-nothing-vs-may-rest.md`](../conventions/order-error-classifier-placed-nothing-vs-may-rest.md)
  — the classification `cancel_all_resting` fails closed on.
- [`ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md`](../integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md)
  — why the cancel scan is single-page and paced.
- [`nautilus-livenode-tests-race-on-the-global-logger-init.md`](../test-failures/nautilus-livenode-tests-race-on-the-global-logger-init.md)
  — why `LiveNode::build()` is serialized behind one process-wide lock (now owned by the runner, so
  tests must take *that* lock, not a private one).
