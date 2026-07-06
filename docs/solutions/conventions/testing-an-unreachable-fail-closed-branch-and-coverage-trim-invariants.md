---
title: "Testing an unreachable fail-closed branch (nautilus ingest) + the coverage-trim / migration-chaining invariants"
date: 2026-07-07
problem_type: convention
module: nautilus adapter ingest (adapters/nautilus/src/ingest)
component: ingest-accumulate
severity: medium
tags:
  - nautilus
  - ingest
  - accumulate
  - testing
  - defense-in-depth
applies_when:
  - "Adding a fail-closed / defensive branch whose trigger the production path makes unreachable, and needing a deterministic test for it"
  - "Reasoning about the accumulate coverage-trust trim (completed_intervals_above / subtract_covered) or asserting t8410 fetch counts in accumulate integration tests"
  - "Writing to a ParquetDataCatalog from inside a wiremock respond_with closure"
---

## Context

Plan `2026-07-06-003` (PR #105) added three write-side hardening branches to the nautilus
re-ingest path. Two of them exposed non-obvious testing and invariant facts that will bite the
next person working in `adapters/nautilus/src/ingest`.

## Guidance

### 1. A heal re-pull append can never *naturally* overlap — the branch is pure defense-in-depth

`heal_daily` wipes the series (`delete_bar_series` → `delete_data_range("bars", id, None, None)`)
**before** re-pulling and appending. `delete_data_range` with a `None`/`None` range removes **every**
parquet leaf for that series identifier (verified empirically: write two disjoint leaves →
`stored_bar_intervals` returns both → after delete it returns `[]`). So the re-pull append is disjoint
by construction and `append_bars_checked` can never refuse it in normal operation. The `#104`
`HealOutcome::AppendRefused` catch is defense-in-depth against a delete that silently failed to clear.

To test such a branch you must **inject the anomaly at a runtime seam the production path cannot
reach**. Here: make the wiremock `respond_with` closure, on the *post-wipe re-pull* call, write an
overlapping stored leaf just before responding — so by the time `append_bars_checked` runs, a leaf
exists to overlap. Distinguish the re-pull call from the detect/re-verify calls by pre-marking the
symbol shifted (a pre-marked triple skips `detect_shift`, so the first `t8410` call in the run *is*
the re-pull) and arming the injection with an `AtomicBool` the test flips between runs.

### 2. Writing to a ParquetDataCatalog from a mock closure needs a fresh OS thread + fresh runtime

`ParquetDataCatalog` drives an internal `block_on`. Calling it from inside the wiremock closure (which
runs on the async worker) panics with a nested-runtime error. Do the injecting write on a dedicated
`std::thread::spawn` with its own `tokio::runtime::Runtime::new().block_on(...)`, and `.join()` it so
the leaf is on disk before the response returns. (Same root cause as
[nautilus-parquet-catalog-block-on-from-async](../integration-issues/nautilus-parquet-catalog-block-on-from-async.md).)

### 3. `detect_shift` adds one extra t8410 fetch per daily append triple

On the daily append path, `detect_shift` issues one overlap-window fetch (KTD-3) *before* the append
fetch. So accumulate integration `count_t8410` assertions are **`1 (detect) + N (sub-range fetches)`**,
not `N`. A pre-marked (healing) triple skips detect, so its first call is the re-pull.

### 4. Migration chaining guarantees a far `completed` range always has a leading gap

The `#102` coverage-trim (`subtract_covered` over `completed_intervals_above`) relies on this: a
`completed` range that survives *above* the watermark as a separate span always has a non-trading gap
before it. Why — `migrate_completed_watermarks` **chains** any range with no weekday strictly between
the running chain-max and the next range's start; a range abutting `watermark+1` has no weekday
between, so it chains *into* the watermark instead of surviving as a remainder. Consequences you can
rely on:

- Steady state (no far coverage) always trims to the single segment `[watermark+1, last_closed]` —
  behavior identical to pre-trim.
- `subtract_covered` never yields an empty sub-range list via a migration-produced checkpoint (there's
  always at least the leading gap), so that defensive path is effectively unreachable.
- The `wm.is_none()` + stored-bars case is **not** the `#102` stall — it's the pre-existing
  fail-closed-net case (un-attested pollution), remediated by `lab-research catalog compact`, and the
  trim intentionally does not engage there.

## Why This Matters

Without (1)–(2) you cannot write a green, deterministic test for the heal-overlap branch and may
conclude it's untestable (or worse, delete it). Without (3) every accumulate count assertion is off by
one and looks like a real bug. Without (4) you'd add dead handling for "empty sub-ranges" or try to
make the trim cover the no-watermark pollution case that is deliberately out of scope.

## When to Apply

Any change to `run_accumulate`'s append/heal path, the coverage trim, or accumulate integration tests
in `adapters/nautilus/tests/ingest.rs`; and generally when a fail-closed branch's own guard makes its
trigger unreachable from the public API.
