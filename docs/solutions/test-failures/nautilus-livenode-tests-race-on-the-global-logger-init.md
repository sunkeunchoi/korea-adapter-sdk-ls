---
title: "Nautilus LiveNode-building tests race on the process-global logger init (\"a non-Nautilus logger is already registered\")"
date: 2026-07-16
category: test-failures
module: adapters/nautilus/lab test suite (adapters/nautilus/lab/tests/live_wiring.rs), Nautilus LiveNode builder
problem_type: test_failure
component: testing_framework
severity: medium
symptoms:
  - "adapter-check CI red on a test that builds a Nautilus LiveNode: `node build: A non-Nautilus logger is already registered; cannot initialize Nautilus logging`"
  - "Intermittent: passes locally (and on some CI runs), fails on others — main's adapter-check history is ~15-30% red on the same check"
  - "Deterministic per-SHA in a given CI environment: re-running the SAME commit reproduces the same pass/fail (3/3 failures observed), so 'just re-run' cannot flip it without a new SHA"
root_cause: test_isolation
resolution_type: test_fix
applies_when:
  - "Adding or reviewing a test under adapters/nautilus/ that builds a Nautilus LiveNode (`LiveNode::builder(...).build()`) or calls `build_live_session_node(...)`"
  - "Diagnosing an intermittent / per-SHA-deterministic adapter-check failure that mentions logger registration"
tags:
  - nautilus
  - livenode
  - global-logger
  - test-isolation
  - flaky-test
  - toctou-race
  - adapter-check
---

# Nautilus LiveNode-building tests race on the process-global logger init

## Problem

Two tests in the single `adapters/nautilus/lab/tests/live_wiring.rs` binary each build a Nautilus `LiveNode`, and
building a node initializes Nautilus's **process-global** logger through a non-atomic
check-then-set. Run concurrently by the default test-thread pool, they race and one intermittently
fails with `A non-Nautilus logger is already registered; cannot initialize Nautilus logging`.

## Symptoms

- `adapter-check` (`cd adapters/nautilus && cargo test --workspace`) fails on
  `build_live_session_node_mounts_the_strategy` (`live_wiring.rs`) with the logger error above.
- **Intermittent and environment-sensitive:** green locally (5/5) and on many CI runs, red on
  others. `main` itself is ~15-30% red on this check independent of any given branch.
- **Deterministic per (SHA, CI-env):** re-running the *same* commit reproduces the same outcome
  (observed 3/3 red on one SHA), because the scheduler resolves the race identically each run. A
  *new* SHA reshuffles the test-binary layout and can flip it — which is why the flake masquerades
  as "random" across commits but is fixed within one.

## What Didn't Work

- **Re-running the failed CI job.** The race is deterministic per-SHA, so all three attempts on the
  same commit failed identically. Re-run is only useful when a *new* commit changes the SHA.
- **Blaming the diff under review.** The turn that surfaced this only added a default-off sizing
  lever and `prior_illiq: None` to the two tests' `SelectedSymbol` literals — causally incapable of
  a logger-registration conflict. The flake predates the branch (visible in `main`'s CI history).

## Solution

Serialize the logger-initializing builds with a shared, poison-tolerant lock so the check-then-set
is atomic across them. A serialized second build then sees the (own) logger already registered and
tolerates it — only the *concurrent* interleaving produces the "non-Nautilus" partial state.

```rust
use std::sync::{Mutex, MutexGuard};

static NODE_BUILD_LOCK: Mutex<()> = Mutex::new(());

fn node_build_lock() -> MutexGuard<'static, ()> {
    // Poison-tolerant: a panicking node-building test must not wedge the others.
    NODE_BUILD_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
```

Wrap **every** `LiveNode` build site, holding the guard only across the synchronous `.build()` and
dropping it before any `.await` (so it is safe on any `#[tokio::test]` flavor, including the default
multi-thread one):

```rust
let node = {
    let _guard = node_build_lock();
    build_live_session_node(paper_config(), OrbParams::default(), selected, DecisionSink::new(), 0.1)
}; // guard dropped here — subsequent non-logger work runs unlocked
```

Shipped in PR #156 (`fix(test): serialize LiveNode-building tests …`).

## Why This Works

Nautilus initializes its global logger with a non-atomic "is a logger already registered? if not,
register mine" sequence. Two threads both passing the check before either sets leaves the global in
a state the loser reads as *foreign* → "a non-Nautilus logger is already registered". The lock makes
the check-and-set indivisible: the first builder fully registers before the second even checks, and
the second's check now sees Nautilus's own logger and no-ops. Because a global logger cannot be
un-registered, **separate processes** (a distinct `tests/*.rs` file per node-building test) is the
alternative fix — each process gets a fresh global — but in-binary serialization is the smaller
change when the tests already share a file.

## Prevention

- **Any new test that builds a `LiveNode` (or calls `build_live_session_node`) in an existing test
  binary must take `NODE_BUILD_LOCK` around the build** — otherwise it reintroduces the race against
  the existing node-building tests. This is the load-bearing rule: the lock only works if *every*
  builder in the binary respects it.
- Prefer a per-file (per-process) split if a suite accumulates many node-building tests — process
  isolation is race-proof by construction, at the cost of more test binaries.
- When an adapter-check failure is deterministic per-SHA yet passes locally, suspect a global-state
  / parallel-test-isolation issue before suspecting the diff — and check whether `main` itself
  flakes on the same check (a quick `gh run list --branch main` on the workflow).
