---
title: "Latch a stop request driver-side — nautilus CLEARS its own stop flag on every transition to Running"
date: 2026-07-26
category: conventions
module: "adapters/nautilus/lab — runner/live.rs StopRequest + stop_requested_then_grace; vendored nautilus-live 0.60.0 node/state.rs"
problem_type: convention
component: development_workflow
severity: high
applies_when:
  - "Writing anything that observes `LiveNodeHandle::should_stop()` to decide the node has been asked to stop"
  - "Adding a timeout, backstop, or supervisor that must arm when a stop is requested"
  - "More than one party can request the stop (a session timer, a watchdog thread, a liveness loop)"
  - "Polling any third-party flag as a level, when the edge is what you actually care about"
tags:
  - nautilus
  - live-session-driver
  - stop-flag
  - level-vs-edge
  - vendored-api
---

# Latch a stop request driver-side — nautilus clears its own stop flag

## Context

`LiveNodeHandle` looks like a durable record of "someone asked the node to stop": `stop()` sets a
flag, `should_stop()` reads it, and both the node and the driver can see it. Polling it is the
obvious way to notice a stop request, and it is what the hard-stop backstop in
`adapters/nautilus/lab/src/runner/live.rs` originally did.

It is not durable. In `nautilus-live` 0.60.0, `LiveNodeHandle::set_state` **clears** `stop_flag`
whenever the new state is `Running`:

```rust
// nautilus-live-0.60.0/src/node/state.rs
pub(crate) fn set_state(&self, state: NodeState) {
    self.state.store(state.as_u8(), Ordering::Relaxed);
    if state == NodeState::Running {
        self.stop_flag.store(false, Ordering::Relaxed);   // <-- your request, erased
    }
}
```

and `LiveNode::run` makes that transition **after** client connection and reconciliation — see the
`set_state(NodeState::Running)` following the engine connect in the vendored crate at
`~/.cargo/registry/src/*/nautilus-live-0.60.0/src/node/mod.rs` (not a repo path; read it there).
Nautilus never *sets* the flag — only the caller does — so every write nautilus performs to it is
a clear.

## Guidance

**Latch the stop request on your own side, and pair it with `handle.stop()` in one place.**

```rust
#[derive(Clone)]
struct StopRequest {
    latch: Arc<AtomicBool>,
}

impl StopRequest {
    /// Ask the node to stop AND record that we asked. Latch FIRST, so an observer that
    /// sees the node's flag has already seen the latch.
    fn request(&self, handle: &LiveNodeHandle) {
        self.latch.store(true, Ordering::SeqCst);
        handle.stop();
    }
}
```

Then arm on the latch, reading the node's flag only as a belt-and-braces second condition:

```rust
while !stop_request.requested() && !handle.should_stop() {
    tokio::time::sleep(poll).await;
}
```

Routing every requester through one `request()` is the load-bearing half. The driver has three
(the session timer, the watchdog thread, the session-side liveness loop), and a fourth added later
that calls `handle.stop()` directly would silently un-arm the backstop with no test failing.

## Why this matters

The window is small but it is exactly the window that matters. A stop requested during node
**startup** — while the gateway connection or reconciliation is dragging, which is precisely when
the dead-man fires — is erased when startup completes. A backstop polling the flag at, say, a
5-second cadence can miss the transient entirely, never arm, and let the driver block forever:
the failure the backstop exists to prevent, reintroduced by the mechanism meant to prevent it.

Generalizing past this API: **polling a level to detect an edge is only sound if you own the
level.** When a third party can reset the flag you are sampling, sampling is not detection. Either
latch the event yourself at the moment you cause it, or subscribe to an edge the other side
guarantees. `LiveNodeHandle` exposes no notify, so latching is the only option here.

## When to apply

Any read of `should_stop()` that is deciding *whether a stop was requested*, rather than deciding
*whether to keep working right now* (the node's own loop condition is the latter, and is fine).

More broadly: before polling any flag owned by a dependency, read the dependency's writes to it.
`rg 'stop_flag' ~/.cargo/registry/src/*/nautilus-live-*/` was a two-minute check that would have
caught this at authoring time; it was found in code review instead.

## Testing note

`stop_flag` is `pub(crate)` in nautilus, so no test in this repo can clear a live handle to
reproduce the failure directly. The regression test in `live.rs` models the cleared state with a
fresh handle carrying the same latch, and proves the property that matters: the deadline still
fires while `should_stop()` reads false for the whole wait. State the modeling in the test's own
comment — a test that silently substitutes a model for the real adversary invites a future reader
to over-trust it.

## Related

- `docs/solutions/architecture-patterns/making-a-failure-graceful-can-delete-the-signal-that-detected-it.md`
  — the other learning from the same turn; both were found by review of a change that had already
  passed a green gate.
- `docs/solutions/architecture-patterns/live-session-teardown-must-share-the-nodes-arcs-and-capture-handles-before-build.md`
  — the driver's other vendored-API traps, all the same shape: the code runs and protects nothing.
