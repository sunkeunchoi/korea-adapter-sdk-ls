---
title: "Re-ingesting an overlapping date range duplicates catalog bars — the parquet write skips the disjoint check and the accumulate append never wipes, so the aggregate read double-counts and the universe scan self-gaps"
date: 2026-07-05
category: logic-errors
module: "nautilus adapter ingest/catalog (adapters/nautilus/src/ingest/mod.rs: read_all_bars, write_bars, accumulate append), lab backtest runner (adapters/nautilus/lab/src/runner/backtest.rs: build_candidates)"
problem_type: data-integrity
component: adapter
severity: high
applies_when:
  - "Widening or re-pulling an already-covered date range for a symbol already in the catalog (the turn-2b data-turn workflow: accumulate with an earlier LS_INGEST_LOOKBACK)"
  - "Running accumulate on a catalog whose ingest-checkpoint predates the watermark format (legacy `completed` ranges, empty `watermarks`) — accumulate reads only `watermarks`, sees every triple as never-seen, and re-fetches from the floor"
  - "`lab-research catalog status` reports bar counts far larger than the trading-day span, or a backtest over a WIDER slice empties the universe (universe_hash = SHA256 of empty) and trades 0 where the narrower slice traded"
  - "Any code path that writes a second parquet file for a date range overlapping an existing file for the same (instrument, bar kind)"
tags:
  - nautilus-adapter
  - ingest
  - parquet-catalog
  - accumulate
  - re-ingest
  - duplicate-bars
  - universe-scan
  - checkpoint-format
  - dedup
  - data-integrity
---

## Problem

`ParquetDataCatalog::write_to_parquet` writes one file per call, named by the bars'
min..max timestamp, and **skips the disjoint check** (see `write_bars`). The normal
accumulate *append* path assumes every write is a disjoint forward range
(`watermark+1 ..`), so it never wipes. When a run instead re-fetches a range that
**overlaps** already-stored bars, it writes a second file (e.g. `0601..0703`) beside
the original (`0618..0703`); both stay readable, and the aggregate read
(`read_all_bars`) returns every overlapping bar twice.

The load-bearing trigger is a checkpoint-format mismatch: a catalog first built in a
mode that populates the `completed` range list (empty `watermarks`) is then widened
with `accumulate`, which keys only on `watermarks`. It sees every triple as
never-seen, re-fetches from `LS_INGEST_LOOKBACK`, and overlaps the existing files.
Modes that use different checkpoint fields do not share state.

The corruption bites the backtest's universe scan. `build_candidates` reads the last
two in-range daily bars as prior→today; with duplicates it picks two copies of the
**same** final session, so `prior_close` and `today_open` are drawn from one day and
the "gap" becomes a nonsensical intraday move (open vs its own close, always
negative). Every symbol is rejected and the universe empties.

## Symptoms

- `lab-research catalog status` bar counts are ~double the trading-day span (e.g. 36
  daily bars for a ~24-session range; 13,716 minute bars where ~9,144 are real).
- A backtest over a **wider** slice — a strict superset of a slice that traded — has
  `num_trades = 0`, `universe_snapshot = []`, and `universe_hash = e3b0c442…` (the
  SHA-256 of the empty string).
- The recorded universe decisions show `prior_close`/`today_open` drawn from the same
  session and a large negative `gap_pct`, rejecting every candidate on the `gap`
  filter.
- No basis-shift is flagged (`adjustment_basis_shift_symbols: []`) — the duplicates
  are byte-identical, so it is not an adjustment problem.

## What Didn't Work

- **Overwrite-and-tolerate.** `write_to_parquet` with the disjoint check skipped does
  not replace an overlapping file; it adds a new one. Only `delete_bar_series` (the
  heal path's wipe) removes stale files, and the append path does not call it.
- **Relying on the finalize fingerprint.** The run's start-vs-end `range_fingerprint`
  re-check catches a *mid-run* catalog mutation, but a pre-existing overlap is stable
  across the run — `fingerprint_start == fingerprint_end`, no abort, corrupt scan used.

## Solution

Two layers, both in the read/consume path (leaving the accumulate-forward write
semantics untouched):

1. **Read-side dedup** in `read_all_bars` (`dedup_bars`): drop bars that are
   *byte-identical* to one already seen, via a `HashSet<Bar>`. Bars are built with
   deterministic timestamps (`ts_event == ts_init`, from the candle's own KST
   date/time — `build_bar(… ts, ts)`, never a wall clock), so a redundant re-pull is
   exactly equal and collapses. Dedup is on the **whole bar**, not a `(series, ts)`
   key: a same-timestamp bar whose OHLCV differs is a genuine conflict (adjustment
   shift or an in-range mutation) and must survive so the heal path / mid-run
   fingerprint check still fire.

2. **Distinct-session selection** in `build_candidates` (`select_prior_today`): choose
   `prior`/`today` as the last two **distinct** `ts_event` sessions, not the last two
   sorted indices. This defends the scan even against a value-divergent same-session
   duplicate that dedup deliberately keeps — a no-op for a clean catalog, and the only
   structural guarantee the scan cannot self-gap.

## Why This Works

Deterministic `ts_event`/`ts_init` make an overlap re-pull byte-identical, so
whole-bar dedup collapses exactly the redundant copies and nothing else. Keeping
value-divergent same-timestamp bars preserves the adjustment-shift and mid-run-mutation
signals. Distinct-session selection means the gap is always computed across two real
sessions regardless of what the catalog read returns, closing the corruption class
completely rather than only its byte-identical subset. No `read_all_bars` consumer
depends on the dropped duplicates (all re-sort by `ts_event` or take min/max), so the
dedup is transparent.

## Prevention

- **Treat a re-ingest overlap as a first-class case**, not an impossibility. Any write
  path that can emit a file overlapping an existing one for the same triple needs a
  consuming-side dedup or a write-side wipe; `write_to_parquet`'s skipped disjoint
  check makes silent duplication the default.
- **Mind checkpoint-format compatibility across ingest modes.** A `completed`-range
  catalog and `accumulate` (watermark-keyed) do not share coverage state; mixing them
  re-fetches from the floor and overlaps. Migrating `completed`→`watermarks` on load
  (so accumulate doesn't re-fetch covered ranges) is the deeper fix.
- **Residuals left open** (documented, not fixed here): read-side dedup is a mask — the
  write path still accumulates overlapping files on each re-widen (disk/latency
  growth); and the heal path's `detect_shift` reads its overlap tail via a *non-deduped*
  `read_bars_scoped`, so accumulated duplicate rows can dilute the unique-date count
  below `MIN_OVERLAP_DATES` and suppress genuine basis-shift detection. Prefer a
  write-side or unique-date-based fix when addressing these.
