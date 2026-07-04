---
title: "Zero-row re-pull treated as heal completion after a destructive wipe permanently destroys stored history"
date: 2026-07-03
category: logic-errors
module: nautilus-ls adapter — accumulate-forward ingest basis-shift heal (adapters/nautilus/src/ingest/mod.rs, heal_daily)
problem_type: logic_error
component: tooling
symptoms:
  - "A symbol's entire stored daily-bar history vanishes after a basis-shift heal run that coincided with a transient empty page from the LS paper gateway"
  - "Heal mark cleared and watermark set to last_closed even though the re-pull returned zero rows (TripleOutcome::Gap(EmptyHistory)) — so no future accumulate run ever re-pulls the lost range"
  - "The checkpoint looks fully healthy afterward (no mark, current watermark, a recorded re-base event); the loss is silent — no gap, no error"
  - "Invisible to happy-path tests: the wiremock fixture gateway always serves data, so every heal completes and the dangerous arm never executes"
root_cause: logic_error
resolution_type: code_fix
severity: critical
related_components:
  - ingest-checkpoint
  - parquet-catalog
tags:
  - nautilus-adapter
  - ingest
  - basis-shift-heal
  - data-loss
  - wipe-then-refetch
  - empty-response
  - watermark
  - retry-path
---

# Zero-row re-pull treated as heal completion after a destructive wipe permanently destroys stored history

## Problem

The basis-shift heal in `adapters/nautilus/src/ingest/mod.rs` (`heal_daily`) wipes a symbol's daily parquet series and re-pulls its full history from the LS gateway — and treated *any* completed fetch cursor as heal completion, including one that completed with zero rows. Since the LS paper gateway serves transiently empty pages, a single empty response during a heal could permanently destroy years of stored daily bars.

## Symptoms

- A symbol marked shifted goes through the heal: its parquet series is true-deleted (`delete_bar_series`), the re-pull returns `TripleOutcome::Gap(EmptyHistory)` (zero rows, cursor "complete"), and the heal declares success anyway — mark cleared, re-base event recorded, watermark set to `last_closed`.
- The catalog now holds **zero bars** for that symbol, but the checkpoint looks fully healthy: no shifted mark, watermark at `last_closed`.
- Because the watermark is the skip authority for accumulate-forward, every future `run_accumulate` sees the symbol as current and skips it. Nothing ever re-pulls. The history loss is silent and permanent — no gap is reported, no error is logged as a failure.
- The failure never shows up in normal test runs: the wiremock fixture gateway always serves data, so every happy-path test heals cleanly.

## What Didn't Work

The design rule from the plan (KTD-3) was correct on its face:

> heal completion keys on the fetch cursor completing, never on reaching floor-depth bar count.

That rule exists for a real reason: a shallow-history symbol (listed *after* the backfill floor, serving only 2 bars) must still clear its shifted mark, otherwise it stays marked forever and re-heals on every run. Keying completion on "did we get floor-depth bars back?" would break those symbols.

The bug was implementing that rule as its maximal reading: **"any completed cursor = completion"**, including a cursor that completed with zero rows (`TripleOutcome::Gap(EmptyHistory)`). The reasoning conflated two distinct claims:

1. "Bar *count* is not completion evidence" (true — shallow history is legitimate), and
2. "Therefore bar *presence* is not completion evidence either" (false — zero rows for a symbol that demonstrably had years of bars five seconds ago is far more likely a transient gateway hiccup than a genuine delisting).

Combined with the destructive wipe-then-refetch structure, the zero-row acceptance turned a documented paper-gateway quirk (transiently empty pages off-hours) into unrecoverable data loss: the wipe had already happened, and "completing" pinned the watermark over the empty store, removing the only retry path.

## Solution

`heal_daily` already reads the symbol's stored bars **before** the wipe, for the floor-precondition check:

```rust
// Wipe precondition (KTD-2). An already-wiped re-entry has no stored bars
// and passes trivially (there is no history left to truncate).
let stored = read_bars_scoped(&self.config.catalog_path, bar_type, None, None).await?;
```

That pre-wipe read is exactly the signal needed to distinguish "genuinely empty series" from "transient empty page." Before the fix, the re-pull match accepted any completed cursor:

```rust
// BEFORE (buggy): any completed cursor counts as completion,
// including one that returned zero rows.
let pulled = match collect_daily(&self.fetcher, shcode, bar_type, &sdate, &edate).await? {
    TripleOutcome::Bars(bars) => bars,
    TripleOutcome::Gap(GapReason::PaperThin) => {
        return Ok(HealOutcome::Incomplete); // truncated fetch keeps the mark
    }
    TripleOutcome::Gap(_) => Vec::new(),    // zero rows → "healed" over a wiped store
};
```

After the fix, a zero-row completion is only trusted for a series that was already empty pre-wipe:

```rust
    // Cursor completed with zero bars. Trust it as completion ONLY for a
    // series that was already empty before the wipe — for a series that
    // HAD stored bars, an empty page is far more likely a transient
    // gateway hiccup than a genuine delisting, and completing here would
    // pin the watermark over a wiped store: silent, permanent history
    // loss with no retry path. Keep the mark instead; the next run
    // re-enters at the (now no-op) wipe and re-pulls.
    TripleOutcome::Gap(_) => {
        if !stored.is_empty() {
            tracing::warn!(
                instrument,
                "heal re-pull returned no bars for a previously non-empty series; symbol stays marked"
            );
            return Ok(HealOutcome::Incomplete);
        }
        Vec::new()
    }
```

Returning `HealOutcome::Incomplete` keeps the shifted mark and leaves the watermark cleared — the next accumulate run re-enters the heal at the (now no-op) wipe and re-pulls when the gateway recovers.

## Why This Works

The root cause was that the heal used the *fetch outcome alone* as completion evidence for a destructive operation, discarding the local evidence it already held. The checkpoint's shifted mark is the crash-convergence mechanism (KTD-2: any interruption re-enters at the wipe), and the watermark is the skip authority; the one unrecoverable state is *mark cleared + watermark set + store empty*, because nothing ever looks at the symbol again.

The fix closes that state transition by adding one bit of pre-wipe context: `stored.is_empty()`. It preserves both original requirements simultaneously:

- **Shallow-history symbols still heal** (KTD-3 intact): a symbol with 2 bars re-pulls 2 bars — `TripleOutcome::Bars`, not `Gap` — and completes. Even a symbol whose series was *already empty pre-wipe* completes on a zero-row pull, so nothing gets pinned marked forever.
- **Transient empty pages become retries, not data loss**: for a previously non-empty series, zero rows routes to `Incomplete`, which is the designed re-entrant path — mark kept, watermark absent, wipe idempotent.

The three zero-ish outcomes are now handled distinctly: truncated fetch (`Gap(PaperThin)`) → never complete; zero rows + was-empty → complete; zero rows + was-non-empty → retry.

## Prevention

**Generalized rule:** in any destructive wipe-then-refetch sequence (basis re-base, cache rebuild, re-sync), *"the source returned nothing, successfully" is not completion evidence when local state proved data existed pre-wipe.* Capture the pre-wipe emptiness signal — you often already have it for free from a precondition read, as `heal_daily` did — and route zero-row completions through the retry path (keep the resumable marker, don't advance the skip authority). Always distinguish three cases:

1. **Truncated fetch** (page cap, cursor didn't finish) → never completion.
2. **Zero rows, series was empty pre-wipe** → completion (the source and local state agree).
3. **Zero rows, series was non-empty pre-wipe** → retry (the source contradicts durable local evidence; trust the evidence).

**Test strategy:** this bug class is invisible to happy-path tests — the fixture gateway always serves data, so every heal completes and the dangerous arm never executes. You must script a responder that serves *nothing* for exactly one run, then recovers, and assert that the resumable state survived the empty run. The regression test `empty_repull_of_a_nonempty_series_keeps_the_mark` in `adapters/nautilus/tests/ingest.rs` does this with a mutable `SharedSeries`:

```rust
let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

let mut cp = Checkpoint::load(&cp_path).unwrap();
cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5));
cp.save(&cp_path).unwrap();
// A transient gateway hiccup: the server serves NOTHING for the symbol.
shared.set(series(&[]));

let report = ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
let cp = checkpoint_at(&catalog);
assert!(cp.is_shifted(SAMSUNG, "1-DAY"), "an empty re-pull must not complete the heal");
assert!(cp.rebase_events().is_empty());
assert!(cp.watermark(SAMSUNG, "1-DAY").is_none(), "no watermark pinned over the wiped store");
assert_eq!(report.gaps.len(), 1, "the incomplete heal is reported");

// The server recovers → the next run re-pulls and completes.
shared.set(v2());
ing3.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
assert!(!checkpoint_at(&catalog).is_shifted(SAMSUNG, "1-DAY"));
assert_eq!(stored_closes(&catalog).await, vec![30000, 30900, 31000, 31500]);
```

The shape to copy: seed real data (v1), trigger the destructive path, make the source empty for one run, assert *all three* pieces of resumable state (mark kept, no completion event, no watermark) plus the gap report, then restore the source (v2) and assert full recovery. Asserting only "no crash" or only the final recovered state would miss the bug — the middle assertions are the test.

## See Also

- `docs/solutions/architecture-patterns/order-double-execution-guards-dedup-reservation-and-complete-query-reconciliation.md` — the same principle at the order-reconciliation layer: an empty/absent query result only proves absence if the query was provably complete. This doc is the ingest-side instance of that rule.
- `docs/solutions/conventions/market-hours-read-empty-result-disposition.md` — the repo-wide disposition rule that an empty gateway page is ambiguous (session/transient) and never an authoritative terminal state.
- `docs/solutions/integration-issues/nautilus-parquet-catalog-block-on-from-async.md` — catalog write/wipe mechanics (every `ParquetDataCatalog` interaction runs in `spawn_blocking`; mutating entry points `create_dir_all` first).
