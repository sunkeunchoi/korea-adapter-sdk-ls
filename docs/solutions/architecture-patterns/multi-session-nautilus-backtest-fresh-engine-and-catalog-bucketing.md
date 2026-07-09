---
title: "Multi-session nautilus backtest — a fresh engine per session is independent; bucket the catalog once"
date: 2026-07-09
category: architecture-patterns
module: adapters/nautilus/lab backtest runner
problem_type: architecture_pattern
component: tooling
applies_when:
  - "Driving more than one trading session through a nautilus BacktestEngine in one process (a per-day loop, a walk-forward, or any multi-window backtest)"
  - "A per-session state reset is wanted (each day starts the strategy fresh) without touching the strategy's own state machine"
  - "A per-session universe/selection reselect scans the loaded bar set inside the loop"
related_components:
  - tooling
tags: [nautilus, nautilus-backtest, backtest-engine, multi-session, spawn-blocking, thread-local-msgbus, catalog-bucketing, performance, orb, strategy-loop]
---

# Multi-session nautilus backtest — a fresh engine per session is independent; bucket the catalog once

## Context

The lab backtest runner (`adapters/nautilus/lab/src/runner/backtest.rs`) originally traded a **single** session — the last in-range daily date — from one `BacktestEngine`. Turning it into a **multi-session** runner (drive every in-range session, reselect the universe per day, reset per-day state) raised two questions that are easy to get wrong:

1. **How do you reset per-day state per session?** The strategy's `OrbState` machine is date-blind and terminal (`Phase::Done` is a lifetime end), so within one engine a symbol trades at most once, ever.
2. **Is it safe to run N `BacktestEngine`s sequentially in one process?** nautilus keeps its message bus in a `thread_local!`, so N fresh engines on a reused blocking-pool thread share/re-populate thread-locals — a plausible cross-session leakage path.

Both were resolved empirically (nautilus `=0.60.0`), and a naive first cut also made the run unusably slow.

## Guidance

**1. Reset per-day state by constructing a fresh engine + fresh strategy per session — do not reach into the strategy.** Loop over the distinct in-range session dates; for each, build a fresh `BacktestEngine` and add a fresh strategy instance (hence fresh per-symbol state). A symbol that reached a terminal phase yesterday starts clean today, for free — the strategy's own state machine is left untouched. Accumulate each session's finished positions into one `Vec<Position>` and assemble the performance report **once** over the union.

**2. A fresh engine per session on one thread IS independent in nautilus 0.60 — no `reset()`/`dispose()` needed.** `BacktestEngine::new` overwrites the thread-local message bus, and positions are read from each engine's own `kernel().cache` (`engine.kernel().cache.borrow().positions(...)`), so sequential fresh engines do not leak cache or handlers across sessions. Run the whole loop inside **one** `spawn_blocking` closure (the engine drives an internal `block_on`, the documented catalog/engine gotcha — see [nautilus ParquetDataCatalog block-on-from-async](../integration-issues/nautilus-parquet-catalog-block-on-from-async.md)), which puts every session on the same blocking-pool thread — the worst case for thread-local leakage. Gate this on a **same-thread independence test** that drives ≥2 sessions where a symbol trades on more than one day and asserts each session's trades belong to that session (test `same_thread_sessions_are_independent`). The documented alternative — one engine + `reset()` + fresh `add_strategy` per session — is unnecessary once the fresh-engine loop passes that test.

**3. Bucket the catalog ONCE before the loop — never re-scan `all_bars` per session.** A per-session universe scan that filters the whole `all_bars` slice per instrument is O(sessions × instruments × bars). At ~27 sessions × ~40 instruments × ~428k bars that is ~4.6e8 bar inspections and turns a backtest into a multi-minute job. Instead, index the catalog in one pass:

```rust
let mut daily_by_inst: HashMap<InstrumentId, Vec<&Bar>> = HashMap::new();
let mut minute_by_date: HashMap<NaiveDate, Vec<&Bar>> = HashMap::new();
for b in all_bars {
    if is_daily(b) {
        daily_by_inst.entry(b.bar_type.instrument_id()).or_default().push(b);
    } else if is_minute(b) && in_range(b, start_ns, end_ns) {
        minute_by_date.entry(kst_date_of(b)).or_default().push(b); // KST conversion once/bar
    }
}
for bars in daily_by_inst.values_mut() { bars.sort_by_key(|b| b.ts_event.as_u64()); }
```

Then the per-session loop indexes the buckets (`daily_by_inst.get(&id)`, `minute_by_date.get(date)`) instead of re-scanning. Also **skip engine construction entirely** when a session selects nothing or has no minute bars, and **mount only the selected instruments** (not the full universe) — the engine only needs instruments whose bars it receives.

**4. Two operational gotchas for the run itself.** nautilus prints its full per-run backtest report through `log::info!` that `BacktestEngineConfig { bypass_logging: true, .. }` and `NAUTILUS_LOG=stdout=Error` do **not** suppress (≈8,900 lines/session) — redirect to a file and ignore it. And a **debug** build is ~5–7 min/session (unusable for ~27 sessions); build the runner `--release` (release did not "die" here, contra the ls-ingest note), which brings a full multi-session window to ~2 min.

## Why This Matters

- **The per-day reset is structural, not a code change to the strategy.** Fresh construction per session means the strategy's state machine never needs a date-aware reset path — the reset is a property of the loop, so the strategy stays simple and unit-testable in isolation.
- **The independence result is load-bearing and non-obvious.** Without the same-thread test it would be tempting to reach for `reset()` (more code, and `reset()` keeps the data stream — you'd also need `clear_data`) or to fear the thread-local msgbus. The empirical test settles it cheaply.
- **The O(sessions × instruments × bars) trap silently turns a correct backtest into an unusable one.** It is not a correctness bug — results are identical — so tests stay green while wall-clock explodes. Bucketing once restores roughly one O(bars) pass plus per-session work proportional to the data actually touched, verified byte-identical against the pre-refactor ledger (only nautilus's instance-random fill `trade_id` differs between runs).

## When to Apply

Any time you loop a nautilus `BacktestEngine` over more than one window in one process and want per-window state isolation, and any time a per-window selection/scan step reads the loaded bar set inside the loop. The independence test is the gate before trusting the fresh-engine-per-window shape; the catalog bucketing is the fix the moment the loop's per-window scan touches the full bar set.

## Examples

**Anti-pattern — per-session full scan (O(sessions × instruments × bars)):**

```rust
for date in &session_dates {
    for inst in instruments {
        // re-walks ALL bars, every instrument, every session
        let daily: Vec<&Bar> = all_bars.iter()
            .filter(|b| is_daily(b) && b.bar_type.instrument_id() == inst.id())
            .collect();
        // ...
    }
    // minute_bars: another full all_bars scan, kst_date_of per bar, per session
}
```

**Pattern — bucket once, index in the loop, skip empty sessions, mount only selected:**

```rust
// (buckets built once, as above)
for date in &session_dates {
    let candidates = build_candidates(instruments, &daily_by_inst, *date); // indexes the bucket
    let selected = select_universe(&candidates, /* ... */);
    let minute_bars = minute_by_date.get(date).map(/* keep selected bar_types */).unwrap_or_default();
    if selected.is_empty() || minute_bars.is_empty() { continue; } // no engine for a no-trade day
    let selected_instruments: Vec<InstrumentAny> = instruments.iter()
        .filter(|i| selected.iter().any(|s| s.instrument_id == i.id()))
        .cloned().collect();
    positions.extend(run_engine(selected_instruments, minute_bars, /* fresh strategy */)?);
}
```

See `run_sessions` in `adapters/nautilus/lab/src/runner/backtest.rs` and the KTD-1 independence test in `adapters/nautilus/lab/tests/backtest_run.rs`.
