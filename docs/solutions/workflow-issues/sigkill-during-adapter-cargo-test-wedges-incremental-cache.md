---
title: "SIGKILL-ing a nautilus-adapter cargo test wedges the incremental cache; verify around the slow gate by running pre-built test binaries directly"
date: 2026-07-21
category: workflow-issues
module: "adapters/nautilus (standalone cargo workspace, nautilus 0.60.0 on Rust 1.96); the gate — cargo test --workspace, make adapter-check / make foundation-gate"
problem_type: workflow_issue
component: testing_framework
tags:
  - "cargo"
  - "incremental-cache"
  - "nautilus-adapter"
  - "gate"
  - "test-verification"
  - "target-lock"
applies_when:
  - "Running the adapters/nautilus workspace gate (cargo test --workspace / make adapter-check / make foundation-gate), which is slow (~15-30+ min) because it compiles nautilus 0.60.0 on the pinned Rust 1.96 toolchain"
  - "You killed a long cargo test/check with SIGKILL (pkill -9, a harness timeout, or ctrl-c-then-kill) and a subsequent cargo invocation now hangs 'Compiling' a tiny crate"
  - "You need to confirm a change is green but cannot afford (or cannot get) a clean full-gate run in the current environment"
---

# SIGKILL during an adapter cargo test wedges the incremental cache

## Context

The `adapters/nautilus/` workspace is deliberately outside the root Cargo workspace and pins **nautilus 0.60.0 on Rust 1.96**, so its gate (`cargo test --workspace`, `make adapter-check`, `make foundation-gate`) is a very heavy, slow compile — the full run is ~15-30+ minutes and is known to suffer target-lock contention (never run two cargo invocations against it concurrently; see the memory note "full cargo test gate ~30+ min; never run two concurrently").

During a large teardown (issue #189 U10) this compounded into a wall: a `cargo test --workspace` was **SIGKILL-ed mid-run** (a harness 2-minute timeout, then `pkill -9`). Every subsequent `cargo test`/`cargo check` then **hung "Compiling nautilus-ls-calendar"** — a tiny leaf crate — for 5-8 minutes with **no progress**. Inspection showed a lone `rustc` at **~0% CPU** with a small RSS, and system memory **75% free** with a load average of ~3.5. So it was *not* memory thrashing and *not* genuine compile work: the process was **blocked**, waiting on a corrupted/locked incremental-compile artifact left behind by the `-9` kill.

## Guidance

**1. Un-wedge the incremental cache after any `-9` kill of a cargo run:**

```sh
pkill -9 rustc; pkill -9 -f "cargo test"        # clear the stuck processes
rm -rf adapters/nautilus/target/debug/incremental  # clears the wedge, keeps dep rlibs
# then re-run your cargo command; it recompiles workspace crates but not all of nautilus
```

Removing only `target/debug/incremental` un-sticks the compiler **without** a full `cargo clean` (which would force the entire nautilus dep graph to rebuild from scratch — many more minutes).

**2. Verify a change WITHOUT waiting on the (slow or wedged) compiler by running the already-built test binaries directly.** `cargo test --workspace` (and `cargo check --workspace --tests`) compile every test binary *up front*, before running any of them, and leave them in `target/debug/deps/`. Those binaries reflect the **last successful compile of your current source**, so once a full compile has succeeded you can re-run any suite in **milliseconds** by invoking its binary — no cargo, no recompile, no lock contention:

```sh
cd adapters/nautilus/target/debug/deps
ls -t traceability-*      | grep -v '\.'          # newest runnable binary (skip .d/.rlib/.o)
./traceability-<hash>                              # runs that integration suite instantly
./merge_block-<hash> --include-ignored             # pass test args through as normal
./nautilus_ls_calendar-<hash>                      # a crate's lib unit-tests (adoption.rs etc.)
```

This is how the U10 teardown was finally verified when the gate itself would not run: `cargo check --workspace --tests` (exit 0, proving everything compiles) plus direct runs of the changed suites (`traceability`, `merge_block`, `closeout_scan`, `calendar_composition`, `dispatch_cli`, the leaf-crate lib tests) — all green in seconds each.

**3. Prefer targeted `-p` runs over the full workspace for iteration.** `cargo test -p nautilus-ls-calendar` (a leaf crate, no nautilus dep) is fast; the multi-minute cost is nautilus itself. Reserve `cargo test --workspace` / `make adapter-check` for a final confirmation (and let **CI's `adapter-check` job** be the authoritative full-gate run — the repo has no *required* status checks, so a clean CI pass is the trustworthy green when the local gate is uncooperative).

## Why This Matters

- A wedged compiler looks identical to a slow one — both show "Compiling …" and no output. The distinguishing signal is **`rustc` at ~0% CPU with memory free**: that is *blocked*, not *working*. Reading CPU/RSS (`ps -o etime,%cpu,%mem,rss`) before assuming "just slow" saves many minutes of waiting on a process that will never finish.
- The heavy nautilus compile makes every wrong turn expensive. Knowing you can (a) surgically clear `incremental` and (b) re-run pre-built binaries directly turns a 20-minute dead end into a 30-second recovery.

## When to Apply

- Immediately after you `-9`-kill any cargo process in this workspace and the next cargo run hangs on a trivial crate.
- Whenever you need to confirm specific suites are green but the full gate is too slow, is wedged, or intermittently hangs under session load (the full `cargo test --workspace` was also observed to **hang an *unchanged* suite** — e.g. `calendar_activate` — for many minutes under heavy concurrent compile load; that is environmental contention, not a test failure).

## Examples

**Distinguishing "wedged" from "working" (the key diagnostic):**

```sh
$ ps -o pid,etime,%cpu,rss,comm -p $(pgrep rustc)
  PID ELAPSED  %CPU    RSS COMM
52380   06:31   0.3  63296 rustc     # 6.5 min elapsed, 0.3% CPU, 62 MB — BLOCKED, not compiling
$ vm_stat | head -2                   # memory is fine → not thrashing
```

**A grep-exit-code false alarm to ignore.** A `cargo test … | grep <failure-pattern>` pipeline exits **non-zero (1) when grep finds *no* matching failure lines** — i.e. when everything passed. A background-task harness reports that pipeline as "failed," which is a **false alarm, not a test failure**. Read the actual log (`grep 'test result:' … | grep -v '0 failed'` → empty means all green), or append `|| true`, rather than trusting the pipeline's exit status:

```sh
# EXIT=101 here was our own SIGKILL of the hung suite (signal 9), NOT a red test:
#   process didn't exit successfully: `…/calendar_activate-<hash>` (signal: 9, SIGKILL: kill)
# Confirm by grepping for real failures, which returned zero.
```
