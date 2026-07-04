---
title: "A fail-closed pending-reconcile symbol set that treats a truncated page as 'scanned' silently drops the symbol"
date: 2026-07-04
category: logic-errors
module: nautilus-ls adapter — order poll/reconcile lane (adapters/nautilus/src/orders/poll.rs + ledger.rs)
problem_type: logic_error
component: tooling
symptoms:
  - "An SC-lane fill for an order the ledger doesn't know arms a per-symbol reconcile, but on a flat ledger the reconcile scan of a busy symbol comes back truncated and the symbol is never re-scanned"
  - "The drive reports Resolved (no 'reconcile advised' warning) even though the truncated page concluded nothing — the reconcile intent leaks with no signal"
  - "Only pending-ONLY symbols hit it: a symbol backed by an open order stays in open_symbols and re-scans to exhaustion, masking the gap in most tests"
root_cause: logic_error
resolution_type: code_fix
severity: medium
related_components:
  - order-safety
tags:
  - nautilus-adapter
  - fill-ledger
  - reconcile
  - fail-closed
  - pagination
  - truncation
  - pending-set
  - ls-orders
  - order-safety
---

# A fail-closed pending-reconcile symbol set that treats a truncated page as "scanned" silently drops the symbol

## Problem

The nautilus-ls execution client owns a **pending-reconcile symbol set** inside
`FillLedger`: when an SC0/SC1 fill names an order the ledger has never seen (or a
cancel is skipped), the affected symbol is recorded so the next paced t0425 drive
scans it even on an otherwise-flat ledger. Each pass **drains** the set under the
ledger lock, **unions** it with `open_symbols()`, scans, then **clears** scanned
symbols and **re-inserts** the ones it couldn't conclude. The bug: a completed but
**truncated** t0425 page (non-empty `cts_ordno`) was counted as "scanned", so the
symbol was cleared even though the page proved nothing — defeating the fail-closed
(R4) intent for exactly the lane the set exists to protect.

## Symptoms

- An SC-armed unknown-order symbol on a **flat ledger** whose t0425 result spans
  multiple pages is dropped: page 1 (truncated) marks it scanned, its later pages —
  possibly carrying the very order that armed the reconcile — are never fetched.
- The drive returns `Resolved`, not `Exhausted`, so no `reconcile advised` warning
  fires: the leak is silent.
- Symbols backed by an **open order** never showed the bug (they persist in
  `open_symbols()` and are re-fetched every pass until the page stops truncating),
  so the gap was invisible in the common path and in the existing tests.

## What Didn't Work

The original implementation deliberately ordered the code as:

```rust
scanned.insert(symbol.clone());              // marked scanned FIRST
if !resp.outblock.cts_ordno.trim().is_empty() {
    out.reconcile_needed = true;             // truncation → re-poll the whole drive
    continue;
}
```

The reasoning was "a truncated page is a completed non-error fetch, and the drive
re-polls on `reconcile_needed`, so the symbol gets another chance." That holds for
**open-order** symbols — the re-poll re-derives `poll_set` from `open_symbols()`,
which still contains them. It fails for **pending-only** symbols: the pending set
was already drained and the truncated symbol was marked scanned, so the drive's
re-poll re-derives an **empty** `poll_set` (no open order, no pending entry),
concludes nothing, and reports `Resolved`. "The drive re-polls" was true but
scoped to the wrong lane.

## Solution

Only count a symbol as scanned on a **non-truncated** completed fetch. Move the
`scanned.insert` to **after** the truncation guard, so a truncated (or errored)
pending symbol stays owed a scan and is re-inserted when the pass ends:

```rust
// Fail-closed on truncation: a non-empty next-cursor means this page did not
// show every order for the symbol — do not conclude anything (R4), and do NOT
// count the symbol as scanned. A pending-only symbol whose page truncates must
// stay owed a scan (re-inserted below) or its later pages would never be
// reconciled on a flat ledger.
if !resp.outblock.cts_ordno.trim().is_empty() {
    out.reconcile_needed = true;
    continue;
}
// A complete (non-error, non-truncated) fetch conclusively scans the symbol,
// clearing it from the pending set (R2/KTD2).
scanned.insert(symbol.clone());
for row in &resp.outblock1 {
    apply_row(symbol, row, ledger, &mut out);
}
```

The re-insert at the end of the pass already skips any symbol in `scanned`, so a
truncated symbol now re-enters the pending set. The cadence gate
(`armed || has_open_orders() || has_pending()`) then re-runs the drive next tick
until the page stops truncating — the intended fail-closed loop.

## Why This Works

The pending set's whole purpose is to guarantee a symbol gets a **conclusive**
scan. "Conclusive" means the page was complete (`cts_ordno` empty) — a truncated
page is exactly the inconclusive case the fail-closed rule (R4) was written for. By
gating `scanned` on non-truncation, the set's "clear only on a completed fetch"
invariant (R2) is stated correctly: a truncated fetch is *completed at the HTTP
level* but *not conclusive*, and only conclusiveness may clear the owed-scan
obligation. Errored fetches were already handled correctly (they `continue` before
`scanned.insert`); truncation was the one milder-inconclusive case that leaked.

## Prevention

- **When a set tracks "work still owed", define the clear condition as the
  work's success predicate, not the transport's.** A fetch returning `Ok` is not
  the same as a fetch that *concluded*. Name the exact predicate ("page complete",
  not "request succeeded") in the code and in the invariant comment.
- **Test the drain/re-insert lane in isolation, on a flat ledger.** The bug lived
  only where the pending set was the *sole* symbol source. A regression test seeds
  a pending symbol with no open order and asserts a truncated page leaves it
  pending:

  ```rust
  #[tokio::test]
  async fn truncated_pending_symbol_stays_pending() {
      let led = Mutex::new(FillLedger::new());
      lock(&led).record_pending_symbol("000660");
      let fetcher = RecordingFetcher::new(resp("NEXT", vec![])); // truncated page
      let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
      assert!(out.reconcile_needed, "truncation flags reconcile");
      assert!(lock(&led).has_pending(), "a truncated pending symbol stays owed a scan");
      assert_eq!(lock(&led).take_pending_symbols(), vec!["000660".to_string()]);
  }
  ```

- **A parallel "background" work state layered beside a "foreground" one (here:
  the pending set beside `open_symbols`) needs its full lifecycle exercised
  independently** — the foreground path (open orders re-polling to exhaustion) can
  paper over a bug in the background path's clear/re-insert logic.

## Related

- [[empty-repull-completing-destructive-heal-destroys-history]] — the same
  fail-closed instinct in the ingest heal path: a zero-row re-pull after a wipe
  must not conclude "done" (permanent data loss). Both are cases where an
  *inconclusive* gateway response was wrongly treated as authoritative.
- [[modify-reads-stale-retained-orderany-not-maintained-fields]] — the sibling
  order-lane learning: reading state from the wrong source of truth. Here the
  "source of truth" for *clearing* was the wrong predicate.
