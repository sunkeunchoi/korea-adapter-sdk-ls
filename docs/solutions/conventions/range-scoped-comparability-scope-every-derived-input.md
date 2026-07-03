---
title: "Range-scoped comparability: scope EVERY derived input to the pinned window, not just the content fingerprint"
date: 2026-07-03
category: conventions
module: adapters/nautilus/lab
problem_type: convention
component: run-registry
severity: high
tags:
  - comparability
  - fingerprint
  - determinism
  - accumulate-forward
  - backtest
applies_when:
  - "A run pins an explicit data range and hashes a range-scoped fingerprint so two runs are 'comparable' (KTD8-style)"
  - "The same catalog/dataset grows over time (accumulate-forward, append-only ingest) while runs re-pin the SAME range"
  - "A run derives a SELECTION or parameter (a universe, a top-N, a 'latest' row) from the dataset in addition to hashing it"
  - "Reviewing a manifest-comparison / reproducibility claim where a fingerprint is asserted stable across days"
---

# Range-scoped comparability: scope every derived input, not just the fingerprint

## Context

The nautilus strategy-lab (`adapters/nautilus/lab`) makes two runs comparable from
their manifests alone (AE1/KTD8): each run pins an explicit `data_range` and records a
**range-scoped** `catalog_fingerprint` — a hash over only the catalog bars whose
`ts_event` falls inside the pinned window. The intent: because accumulate-forward grows
the catalog every day, a comparison re-run pins the *same* range, so identical in-range
data yields an identical fingerprint across accumulate days, and a *changed* fingerprint
means real in-range drift.

The trap: the backtest runner also **derived a selection** from the catalog — the
stocks-in-play universe — via `build_candidates(&instruments, &all_bars)`, reading the
newest two daily bars per symbol (`daily[daily.len()-2]` / `daily[-1]`) over the FULL
`read_all_bars(...)` result, with no range filter. The fingerprint was range-scoped; the
selection that drove the whole run was not.

## Guidance

**Any value a run derives from a growing dataset must be scoped to the SAME pinned
window the comparability fingerprint uses.** A range-scoped fingerprint only guarantees
reproducibility of the *hashed bytes* — it says nothing about a selection computed from
data outside the window. If a derived input reads unbounded/newest data, two runs can
produce byte-identical fingerprints while the derived input (and therefore the entire
result) silently diverges as the dataset grows.

```rust
// WRONG — fingerprint is range-scoped, but the universe scan reads the newest bars:
let fingerprint = range_fingerprint(&all_bars, start_ns, end_ns);      // scoped
let (candidates, _) = build_candidates(&instruments, &all_bars);       // UNscoped!
// build_candidates: daily = all_bars.filter(is_daily && id==...).collect();
//                   prior = daily[daily.len()-2]; today = daily[daily.len()-1];
// → an accumulate-forward day appends a newer daily bar OUTSIDE the range,
//   daily[-1] becomes that bar, the universe drifts, but the fingerprint is unchanged.

// RIGHT — scope the derived selection to the same [start_ns, end_ns] window:
let fingerprint = range_fingerprint(&all_bars, start_ns, end_ns);
let (candidates, _) = build_candidates(&instruments, &all_bars, start_ns, end_ns);
// build_candidates now filters: is_daily && id==... && in_range(b, start_ns, end_ns)
// → out-of-range growth is excluded from the scan; the universe is reproducible.
```

Two supporting moves made the guarantee airtight and honest:

1. **Pin the derived "session" too.** The run trades a single session — the last
   trading day whose daily bar is *in range* — and feeds only that session's minute
   bars. Deriving "which session" from unbounded data has the same drift bug as the
   universe; pinning it to the range closes it and documents the run's actual scope.

2. **Regression-test the drift, not just the fingerprint.** The original drift test
   only asserted `catalog_fingerprint` was unchanged after an out-of-range ingest — it
   would have stayed green while the universe silently drifted. The fix adds
   `assert_eq!(m1.universe_hash, m3.universe_hash)` after appending an out-of-range
   daily bar, so the *derived* selection's stability is guarded, not just the hash's.

## Why This Matters

Comparability is the whole point of the run registry — an agent compares two runs from
their manifests and attributes any difference to the change it made (a strategy/param
delta), not to invisible data drift. A range-scoped fingerprint that coexists with an
unscoped derived selection is worse than no fingerprint: it *actively asserts* two runs
are data-equivalent while their selections differ, teaching the agent to trust a broken
signal. It is also a plain correctness bug — a historical-range backtest silently uses
the newest session's universe instead of the pinned range's.

This class of bug is easy to miss because the fingerprint is obviously scoped, so review
attention stops there; the derived read three functions away looks like an innocent
"get the latest bars." Three independent reviewers (in-process correctness + adversarial
+ a cross-model Codex pass) converged on it, which is the tell: a fingerprint and a
selection that must agree, computed from the same source by different scoping rules, is a
recurring shape worth checking directly.

## When to Apply

- Any run/artifact that pins a range and claims cross-run comparability or
  reproducibility from a range-scoped hash.
- Any "select newest / top-N / latest row" over a dataset that grows between runs
  (accumulate-forward ingest, append-only logs, a rolling catalog) — scope the select
  to the pinned window, or the selection drifts under a stable fingerprint.
- When reviewing: for each comparability fingerprint, enumerate every *other* value the
  run derives from the same source and confirm each is scoped identically. Then assert
  the derived value's stability in the drift test, not only the fingerprint's.

## Examples

The manifest already carried a `universe_hash` (the composition's fingerprint) — the fix
was to make the *scan that produces it* range-scoped so the hash is actually stable, then
prove it:

```rust
// Regression guard: out-of-range accumulate-forward growth must not move the universe.
let o1 = run(cfg(dir.path()), s1).await.unwrap();               // baseline
write_bars(&catalog, vec![daily_20240108]).await.unwrap();     // OUT of the pinned range
let o3 = run(cfg(dir.path()), s3).await.unwrap();               // same pinned range
assert_eq!(m1.catalog_fingerprint, m3.catalog_fingerprint);    // hash stable (was already true)
assert_eq!(m1.universe_hash, m3.universe_hash);                // derived selection stable (the real fix)
```

Related: [[normalized-baseline-can-underreport-request-block]] is a different "the source
of truth under-covers what you derive from it" shape; this one is about *scoping* a
derivation to match its fingerprint rather than *trusting* an incomplete source.
