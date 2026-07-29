---
title: "An operator shell's exported LS_* env makes the adapter suite look red on pristine main"
date: 2026-07-29
category: test-failures
module: adapters/nautilus lab test suite (adapters/nautilus/lab/tests/mount_universe.rs), ladder head-version pin (adapters/nautilus/lab/src/dispatch/ladder.rs head_version_pin)
problem_type: test_failure
component: testing_framework
severity: medium
symptoms:
  - "Two mount_universe tests fail: `a_metadata_driven_head_refuses_a_universe_built_without_the_artifact` and `a_metadata_artifact_that_is_not_the_heads_is_refused`"
  - "Both fail with the head-size guard, not the guard they assert: `mount refused: the resolved head governed params size to ZERO (risk_per_trade_krw=0) ... check LS_DATA_HOME points at the v34 epoch`"
  - "Reproduces on a pristine checkout of a green commit, so `git stash` does not clear it and it reads as a real regression on main"
  - "CI is green on the same commit — CI runs with a clean environment; the operator's shell does not"
root_cause: test_isolation
resolution_type: workflow_improvement
applies_when:
  - "Running `cargo test` under adapters/nautilus/ from a shell that also drives a live or paper session"
  - "Diagnosing an adapter-check failure that reproduces locally on a known-green commit but not in CI"
  - "Adding a test that constructs an OrbParams fixture and depends on head resolution"
tags:
  - nautilus
  - test-isolation
  - environment-leak
  - process-env
  - mount-universe
  - head-version-pin
  - adapter-check
  - false-regression
---

# An operator shell's exported LS_* env makes the adapter suite look red on pristine main

## Problem

`cargo test --workspace` under `adapters/nautilus/` fails two `mount_universe` tests when run
from a shell that has an attended session's `LS_*` variables exported. The failures survive a
`git stash` onto a known-green commit, so they present as a regression on `main` that CI
cannot see.

## Symptoms

Two tests fail, and the message names a guard neither test is about:

```
test a_metadata_driven_head_refuses_a_universe_built_without_the_artifact ... FAILED
test a_metadata_artifact_that_is_not_the_heads_is_refused ... FAILED

panicked at lab/tests/mount_universe.rs:77:
names the cause: mount refused: the resolved head governed params size to ZERO
(risk_per_trade_krw=0) — the data home's latest finalized run must be the v34 head
(risk 299,340), never the all-levers-off default; check LS_DATA_HOME points at the
v34 epoch
```

Both tests assert on the *metadata* guard (`METADATA-DRIVEN`, `hash mismatch`); they never
reach it, because an earlier guard fires first.

## What Didn't Work

- **Stashing the working changes and re-running.** The failure reproduced on pristine
  `5f38144`, which is what made it look like main was red rather than the branch.
- **Reading the message literally and checking `LS_DATA_HOME`.** The refusal text points at
  the data home, but the tests build their own `TempDir` data home and never read
  `LS_DATA_HOME`. The message describes the observed state (a zero-size head), not the cause.
- **Looking for an ordering bug between tests.** The test file's own header comment already
  anticipates that vector and defends against it — see below — which is exactly why it reads
  as ruled out.

## Solution

Run the adapter gate with the `LS_*` variables cleared:

```sh
cd adapters/nautilus
env $(env | grep -oE '^LS_[A-Z_]+' | grep -v LS_COLORS | sed 's/^/-u /' | tr '\n' ' ') \
  cargo test --workspace
```

`LS_COLORS` is excluded deliberately — it is the unrelated GNU `ls` colour variable and
happens to share the prefix.

Green is every `test result:` line ending `0 failed`:

```sh
cargo test --workspace 2>&1 | grep -E '^test result' | grep -v '0 failed'
```

## Why This Works

`head_version_pin()` reads the process-wide `LS_TURN_EXPECT_VERSION`
(`adapters/nautilus/lab/src/dispatch/ladder.rs:119`), and the mount-universe producer passes
it into head resolution (`adapters/nautilus/lab/src/runner/mount_universe.rs:402-405`). The
rung-1 operator sets `LS_TURN_EXPECT_VERSION=34`.

The test fixture builds its head from `OrbParams::default()`, whose `strategy_version` is
`0` (`adapters/nautilus/lab/src/params.rs:353`), overriding only `risk_per_trade_krw`
(`adapters/nautilus/lab/tests/mount_universe.rs:25-31`). So with the pin exported:

1. The pin demands `strategy_version == 34`.
2. The fixture's only run is version `0` — no match.
3. Head resolution falls back to `OrbParams::default()`, which sizes to zero.
4. The zero-size head guard fires and refuses, before the metadata guard the tests assert on.

Nothing about the test or the code is wrong. The pin is doing exactly its job on a fixture
that was never built to satisfy it.

The reason this is easy to misdiagnose: the test file already knows the pin is process-wide
and defends against one leak vector, which makes the other look impossible
(`adapters/nautilus/lab/tests/mount_universe.rs:7-9`):

```rust
//! Its own test binary, deliberately: `head_version_pin()` reads the process-wide
//! `LS_TURN_EXPECT_VERSION`, so sharing a binary with tests that set it would make these
//! order-dependent.
```

That comment covers a *sibling test* setting the variable. It does not cover the ambient
shell exporting it. Isolating the test binary does not isolate the process environment it
inherits.

## Prevention

- **Never run the gate from a session shell.** The variables that drive an attended run
  (`LS_TURN_EXPECT_VERSION`, `LS_DATA_HOME`, `LS_MOUNT_UNIVERSE_*`, `LS_DISPATCH_*`,
  `LS_TRADING_ENV`, …) are exported for the session's lifetime and silently become test
  input. Use a fresh shell, or the `env -u` form above.
- **Suspect the environment before the code when local and CI disagree.** CI runs clean. A
  failure that reproduces on a known-green commit locally but not in CI is an environment
  difference until proven otherwise — check `env | grep '^LS_'` first, before bisecting.
- **A fixture that depends on head resolution should pin its own version.** Setting
  `strategy_version` explicitly on the fixture params, rather than inheriting `default()`'s
  `0`, would make the ambient pin irrelevant to these tests. Not applied here — the tests are
  correct as written and the `env -u` form is the smaller change — but it is the durable fix
  if this recurs.
- **Read a fail-closed refusal as "which guard fired", not "what is wrong".** Both messages
  here are accurate about the state they observed and misleading about the cause. When a
  refusal names a variable the test never reads, the state was reached some other way.

## Related

- [`head-identity-hash-is-file-scoped-so-live-only-wiring-forces-a-rebaseline`](../architecture-patterns/head-identity-hash-is-file-scoped-so-live-only-wiring-forces-a-rebaseline.md)
  — covers `LS_TURN_EXPECT_VERSION` as the head-pin mechanism; this doc covers the same
  variable as a test-environment hazard.
- [`nautilus-livenode-tests-race-on-the-global-logger-init`](nautilus-livenode-tests-race-on-the-global-logger-init.md)
  — the other process-global-state failure in this suite. Same shape: state outside the test
  decides its outcome.
