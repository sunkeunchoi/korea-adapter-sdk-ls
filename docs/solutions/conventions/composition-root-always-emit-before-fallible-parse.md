---
title: "A mandatory always-emit side effect at a composition root must fire BEFORE any early exit — a fallible parse OR a deliberate early-return refusal guard — or consolidating the load silently drops it on those paths"
date: 2026-07-20
category: conventions
module: "nautilus adapter composition root — budget-probe (adapters/nautilus/src/bin/budget-probe.rs), lab catalog status (adapters/nautilus/lab/src/runner/research.rs), lab dispatch gate (adapters/nautilus/lab/src/runner/live.rs run_dispatch), calendar scaffold (adapters/nautilus/src/calendar.rs)"
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
  - dispatch-gate
applies_when:
  - "Consolidating a per-invocation resolve/load and moving a mandatory startup/diagnostic emit into a per-consumer branch"
  - "Adding or editing the budget-probe, lab catalog-status, or lab dispatch-gate composition root, or any new calendar-adoption consumer"
  - "Any consumer that must emit a mandatory record on EVERY invocation, including non-paper / parse-error / config-error paths AND early-return refusal branches (e.g. a no-chain / defective-precondition guard that returns before the emit)"
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

This has now bit the same slice three times. `budget-probe` was written carefully (emit before the
paper gate and the stage/ceiling parses). The `catalog status` branch reintroduced the
identical bug — it emitted *after* `status_config_from_env()?` — and it was caught only
in code review, not by the passing test suite (no test exercised the config-error path
until one was added).

Then it bit a **third** consumer (issue #188, the Production Ladder dispatch gate) in a
form the "before the fallible parse" framing does not cover. `run_dispatch` opens the
chain and matches its status, with `ChainStatus::NoChain` / `ChainStatus::Defective` arms
that `return Ok(DispatchGateOutcome { .. })` — a **deliberate early-return refusal guard**,
not a `?`-propagated parse failure. The single-load resolve + `emit_startup_record` were
placed *after* that match, so `lab-live --dispatch` against a missing/defective chain
emitted **zero** `calendar-startup` lines — the retired unconditional `main_cli` emit used
to be the only thing covering that path. Same invariant, same silent drop, but the trap is
control flow (an early `return`), not a fallible parse. Generalize the rule accordingly:
the emit must precede **any** early exit, `?`-propagated or hand-written `return`.

## Guidance

At a composition root, resolve the state the mandatory side effect needs and **emit it
before any early exit** — both `?`-returning fallible work (env parses, `Runtime::new()?`,
config loads) **and** deliberate early-return refusal guards (a no-chain / defective-state /
non-paper branch that `return`s before the emit). Treat the emit as an unconditional
prologue, not a step interleaved with parsing or sequenced after a precondition guard. The
test is not "is this line fallible?" but "can control leave this function before the emit?"
— if any path can, the emit is in the wrong place.

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

- Any edit to `ProbeContext::resolve` (budget-probe), the `catalog status` branch in
  `main_cli`/`dispatch` (lab research), or `run_dispatch` (lab dispatch gate).
- Adding a new calendar-adoption consumer (the ingest / production-ladder consumers under
  parent issue #184 will hit the same shape).
- Any composition root with a "must emit / must record on every invocation" side effect
  that is being moved to share a resolved/loaded value with the main decision.
- **Any function with early-return precondition guards ahead of the emit** — reorder so the
  resolve + emit is the prologue, ABOVE the guards, not below them.

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

Dispatch gate — the bug (emit sequenced after an early-return refusal guard) and the fix:

```rust
// WRONG — the NoChain / Defective arms return before the emit at the bottom of the fn:
pub fn run_dispatch(cfg: &DispatchCliConfig) -> anyhow::Result<DispatchGateOutcome> {
    let chain = DispatchChain::open(&cfg.data_home)?;
    let mut state = chain.load();
    match &state.status {
        ChainStatus::Valid => {}
        ChainStatus::NoChain   => return Ok(refuse("no dispatch chain ...")),   // no record
        ChainStatus::Defective(why) => return Ok(refuse(...)),                   // no record
    }
    // ... 100 lines later ...
    let (date_fact, rec) = resolve_calendar_for_dispatch(cfg, now_dt);
    emit_startup_record(&rec);   // unreachable on the two early returns above
}

// RIGHT — resolve + emit as the prologue, ABOVE the status match:
pub fn run_dispatch(cfg: &DispatchCliConfig) -> anyhow::Result<DispatchGateOutcome> {
    let chain = DispatchChain::open(&cfg.data_home)?;
    let mut state = chain.load();
    let now_dt = Utc.timestamp_opt(cfg.now_unix, 0).single().unwrap_or_else(Utc::now);
    let (date_fact, rec) = resolve_calendar_for_dispatch(cfg, now_dt); // infallible load
    emit_startup_record(&rec);                                          // fires on EVERY path
    match &state.status {
        ChainStatus::Valid => {}
        ChainStatus::NoChain => return Ok(refuse("no dispatch chain ...")), // record already emitted
        ChainStatus::Defective(why) => return Ok(refuse(...)),              // record already emitted
    }
    // ... decision reuses `date_fact` ...
}
```

Regression test for the early-return path (subprocess, no chain seeded):

```rust
// bin_no_chain_exits_nonzero_and_names_genesis
let out = bin_dispatch(tmp.path(), &[("LS_CALENDAR_ADOPTION", "shadow")]); // no genesis
assert!(!out.status.success());
assert_eq!(String::from_utf8_lossy(&out.stderr).matches("calendar-startup").count(), 1);
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
