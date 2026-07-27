---
title: "Byte-identical parquet writes overwrite the same file — reproducing catalog duplicate-row pollution needs overlapping-but-NOT-identical ranges"
date: 2026-07-06
category: conventions
module: "nautilus adapter ingest/catalog (adapters/nautilus/src/ingest/mod.rs: write_bars, stored_bar_intervals, compact_catalog), tests/ingest.rs"
problem_type: convention
component: adapter
severity: medium
applies_when:
  - "Writing a test/fixture that must stage duplicate ROWS in a ParquetDataCatalog to exercise dedup, the overlap write-guard, or catalog compaction"
  - "Reasoning about how re-ingesting an already-covered range pollutes the catalog on disk"
  - "A compact/dedup test unexpectedly reports a series as `Clean` (no duplicates) when you wrote the same bars twice"
tags:
  - nautilus-adapter
  - parquet-catalog
  - write_to_parquet
  - duplicate-bars
  - test-fixtures
  - re-ingest
  - dedup
  - compaction
---

## Context

`ParquetDataCatalog::write_to_parquet` names each output file by the bars'
`{min_ts}_{max_ts}` timestamp span. A second write of the **exact same range**
produces the **same filename** and silently **overwrites** the first file — it does
NOT create a duplicate. So the intuitive way to stage duplicate rows in a test —
"write the same `Vec<Bar>` twice" — produces a single clean file with no duplicates,
and any dedup/compaction assertion over it sees nothing to collapse (outcome
`Clean`, not `Compacted`). This surfaced while writing the U5 catalog-compaction
tests: `write_bars(s); write_bars(s.clone())` reported the series clean, because the
second write replaced the first file byte-for-byte.

The real production pollution (documented in
`../logic-errors/re-ingesting-an-overlapping-range-duplicates-catalog-bars.md`) comes
from **overlapping but non-identical** ranges — e.g. accumulate re-fetches
`0601..0703` beside an existing `0618..0703`. Those are two *different* filenames
whose row sets overlap, so both files persist and the shared sessions are duplicated
on the aggregate read.

## Guidance

To stage genuine duplicate rows in a catalog fixture, write two files whose date
ranges **overlap but differ at a boundary** (so their `{min}_{max}` filenames
differ). The rows in the overlapping interval then appear in both files.

```rust
// WRONG — identical range → same filename → the 2nd write overwrites the 1st.
// The series reads back clean; a compact/dedup test asserts nothing.
write_bars(&catalog, series(bt, &[(jan3, 100), (jan4, 101), (jan5, 102)])).await?;
write_bars(&catalog, series(bt, &[(jan3, 100), (jan4, 101), (jan5, 102)])).await?;
// -> ONE file 0103_0105.parquet, 3 rows, 0 duplicates.

// RIGHT — overlapping, non-identical ranges → two distinct filenames.
write_bars(&catalog, series(bt, &[(jan3, 100), (jan4, 101), (jan5, 102)])).await?; // 0103_0105
write_bars(&catalog, series(bt, &[(jan4, 101), (jan5, 102), (jan8, 103)])).await?; // 0104_0108
// -> TWO files; jan4 & jan5 appear in both -> 6 raw rows, 4 distinct sessions.
```

The same trick stages a **value-divergent** series (for the compaction refusal
path): two overlapping-range files that disagree on a shared timestamp's OHLCV.

```rust
write_bars(&catalog, series(bt, &[(jan3, 200), (jan4, 201)])).await?; // 0103_0104
write_bars(&catalog, series(bt, &[(jan3, 999), (jan6, 203)])).await?; // 0103_0106, jan3 close differs
// -> jan3 carries two DIFFERENT rows -> value-divergent -> compaction refuses it.
```

To make N copies of one session, write N files that each contain that session under a
distinct `{min}_{max}` span (e.g. `0105_0105`, `0104_0105`, `0103_0105` all contain
Jan5).

## Why This Matters

A test that "writes the same bars twice" and asserts a dedup/compaction *no-op*
passes for the wrong reason — it proves the overwrite, not the dedup. Worse, a
compaction or overlap-guard test built that way silently exercises none of its
target behavior (the guard never sees an overlap; compaction reports `Clean`), so a
real regression in that path would ship green. Getting the fixture shape right is
what makes these tests actually load-bearing. It also clarifies the production
model: on-disk duplication requires *distinct filenames*, which is exactly why
`append_bars_checked` keys its overlap guard on `stored_bar_intervals` (the parquet
filename spans) rather than on row content.

## When to Apply

- Any adapter test staging catalog state to exercise `read_all_bars` dedup,
  `append_bars_checked` overlap refusal, `compact_catalog`, or `detect_shift`
  distinct-session gating.
- When a compaction/dedup test reports a series `Clean`/no-op you expected to be
  `Compacted`/deduped — check whether both writes used the identical range (same
  filename → overwrite → no duplicate).
- When reasoning about how much disk a re-widen actually costs: only
  non-coincident ranges add files; an exact re-pull of the same span is an
  in-place overwrite.

## Examples

Real fixtures using this shape live in `adapters/nautilus/tests/ingest.rs`:
`ae6_compact_collapses_duplicates_and_refuses_divergent` (overlapping-range
duplicate + value-divergent series), `compact_is_idempotent`, and
`duplicate_polluted_tail_still_detects_a_real_shift` (three distinct-span files to
triple one session so the pre-fix length-based overlap tail would dilute below the
minimum). The pre-existing `read_all_bars_dedups_an_overlapping_reingest` test
already relied on this shape (`[Jan3,Jan4,Jan5]` then `[Jan4,Jan5,Jan8]`) — worth
mirroring rather than re-deriving.
