---
title: "A mandatory always-emit side effect at a composition root must fire BEFORE the fallible env/config parse — consolidating the load silently drops it on error paths"
date: 2026-07-20
category: conventions
module: "nautilus adapter composition root — budget-probe (adapters/nautilus/src/bin/budget-probe.rs), lab catalog status (adapters/nautilus/lab/src/runner/research.rs), calendar scaffold (adapters/nautilus/src/calendar.rs)"
problem_type: convention
component: tooling
severity: medium
tags:
  - composition-root
  - calendar-adoption
  - startup-record
  - always-emit-invariant
  - budget-probe
  - catalog-status
applies_when:
  - "Consolidating a per-invocation resolve/load and moving a mandatory startup/diagnostic emit into a per-consumer branch"
  - "Adding or editing the budget-probe or lab catalog-status composition root, or any new calendar-adoption consumer"
  - "Any consumer that must emit a mandatory record on EVERY invocation, including non-paper / parse-error / config-error paths"
---

## Context

The nautilus calendar-adoption consumers (`budget-probe`, `lab-research catalog
status`) each emit ONE mandatory redacted "startup record" to stderr on every
invocation — the always-emit invariant. Originally this fired unconditionally at
the very top of the process, via `nautilus_ls::calendar::emit_startup_from_env(consumer)`,
before any gate or env parse.

Issue #187 consolidated each consumer onto a single per-invocation calendar load
(one snapshot, one as-of) shared between the startup record and the decision, so the
startup record had to move out of the unconditional top-of-process call and into the
consumer's own resolve path (to reuse the same loaded calendar). That move is where
the trap lives: the emit is now easy to place *after* a fallible env/config parse, so
an early `?` on a bad input returns before the record is ever written — silently
dropping the invariant on exactly the error paths where a diagnostic is most useful.

This bit the same slice twice. `budget-probe` was written carefully (emit before the
paper gate and the stage/ceiling parses). The `catalog status` branch reintroduced the
identical bug — it emitted *after* `status_config_from_env()?` — and it was caught only
in code review, not by the passing test suite (no test exercised the config-error path
until one was added).

## Guidance

At a composition root, resolve the state the mandatory side effect needs and **emit it
before any fallible parse or fallible resource construction** (`?`-returning env parses,
`Runtime::new()?`, config loads). Treat the emit as an unconditional prologue, not a step
interleaved with parsing.

Concretely for the calendar consumers:

- Resolve `adoption` + snapshot `path` + `as_of` and `resolve_and_load(...)` first (all
  infallible — a missing/failed snapshot is a typed non-fatal `LoadedCalendar`, never a
  panic or `?`).
- Build + `emit_startup_record(...)` from that single load.
- **Then** run the fallible parses and the decision.

If the emit's target/content depends on a value that only the fallible parse produces
(e.g. the catalog's representative target uses the operator's expected-range end), read
that one value leniently straight from env for the emit (`std::env::var(...).ok().and_then(parse)`),
falling back to a load-derived default — so the emit never depends on the full fallible
config parse.

Add a regression test that drives the error path (unset a required var / feed a malformed
one) and asserts `stderr.matches("calendar-startup").count() == 1` with a non-zero exit.

## Why This Matters

The always-emit record is the operator's only per-invocation trace of which adoption,
snapshot, and posture the process ran under. Dropping it precisely on the error paths
means a non-paper refusal or a config typo produces no calendar diagnostic at all — the
moment you most want one. Because stdout/exit are unaffected, the happy-path tests stay
green and the gap is invisible without a targeted error-path test. It is a
byte-for-byte-silent regression of a stated invariant.

The deeper reason it recurs: "consolidate the load" and "preserve the always-emit
invariant" are in tension. The consolidation *wants* the emit near the load (to share it);
the invariant *wants* the emit before anything that can fail. Only ordering the emit before
the fallible work satisfies both.

## When to Apply

- Any edit to `ProbeContext::resolve` (budget-probe) or the `catalog status` branch in
  `main_cli`/`dispatch` (lab research).
- Adding a new calendar-adoption consumer (the ingest / production-ladder consumers under
  parent issue #184 will hit the same shape).
- Any composition root with a "must emit / must record on every invocation" side effect
  that is being moved to share a resolved/loaded value with the main decision.

## Examples

Budget-probe — the correct shape (emit before the paper gate and the fallible parses):

```rust
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Resolve the calendar context ONCE and emit the mandatory startup record BEFORE the
    // paper gate and the fallible env parses — always-emit holds on every path.
    let ctx = ProbeContext::resolve(&weekday_anchor, &explicit); // loads + emits startup

    if !paper_ok(std::env::var("LS_TRADING_ENV").ok().as_deref()) {
        return Err("refusing to run: set LS_TRADING_ENV=paper ...".into()); // record already emitted
    }
    let stages = parse_stages(...)?;      // fallible — record already emitted
    let (sdate, edate) = ctx.resolved_range()?;
    // ...
}
```

Catalog status — the bug (record emitted after the fallible parse) and the fix:

```rust
// WRONG — status_config_from_env()? can return before the emit, dropping the record:
let cfg = status_config_from_env()?;                 // Err here → no startup record
let loaded = resolve_and_load(path.as_deref(), as_of, adoption);
emit_startup_record(&build_startup_record_targeted(..., &loaded, ...));

// RIGHT — resolve + emit first, then the fallible parse:
let loaded = resolve_and_load(path.as_deref(), as_of, adoption);
let target = std::env::var("LS_STATUS_EDATE").ok()
    .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y%m%d").ok())
    .or_else(|| loaded.calendar().map(|c| c.coverage().materialized_through));
emit_startup_record(&build_startup_record_targeted("lab-research", adoption, &loaded, as_of, target));

let rt = tokio::runtime::Runtime::new()?;            // fallible — record already emitted
let cfg = status_config_from_env()?;                 // fallible — record already emitted
```

Regression test (subprocess, drives the config-error path):

```rust
// catalog_status_emits_startup_record_even_on_config_error
let out = bin().args(["catalog", "status"])
    .env_remove("LS_DATA_HOME")            // status_config_from_env() will fail
    .env("LS_CALENDAR_ADOPTION", "shadow")
    .env("LS_CALENDAR_SNAPSHOT", &snap)
    .output().unwrap();
assert!(!out.status.success());
assert_eq!(String::from_utf8_lossy(&out.stderr).matches("calendar-startup").count(), 1);
```

See also [normalized-baseline-can-underreport-request-block](normalized-baseline-can-underreport-request-block.md)
and [testing-an-unreachable-fail-closed-branch-and-coverage-trim-invariants](testing-an-unreachable-fail-closed-branch-and-coverage-trim-invariants.md)
for the sibling "prove the invariant with a targeted test, not just the happy path" convention.
