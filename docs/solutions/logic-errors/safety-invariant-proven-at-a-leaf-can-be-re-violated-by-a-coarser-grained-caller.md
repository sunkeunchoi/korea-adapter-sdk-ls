---
title: "A safety invariant proven at a pure leaf function can be re-violated by a coarser-grained caller that operates whole-set instead of per-item"
date: 2026-07-19
category: logic-errors
module: "nautilus-ls-calendar witness rule (adapters/nautilus/nautilus-ls-calendar/src/witness.rs) + reconciliation (src/reconcile.rs); refresh candidate builder (adapters/nautilus/src/calendar_refresh/candidate.rs)"
problem_type: logic_error
component: tooling
symptoms:
  - "A single-date incremental KRX calendar refresh dropped EVERY prior positive Trading-Session witness across all historical dates, not just the re-gathered date"
  - "Days previously proven open (a KRX witness overriding an inferred closure) silently reverted to inferred Closed in the candidate snapshot"
  - "witness.rs unit tests were all green — they prove 'absence never retracts a prior witness' for the witness rule in isolation, which is exactly where the invariant was NOT violated"
  - "Only reachable through the refresh build layer with a real partial re-gather, not through any single-function test"
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - nautilus-ls-calendar
  - calendar-refresh
  - evidence-reconciliation
tags:
  - safety-invariant
  - layering
  - evidence-merge
  - absence-never-retracts
  - offline-krx-calendar
---

# A safety invariant proven at a pure leaf function can be re-violated by a coarser-grained caller that operates whole-set instead of per-item

## Problem

The KRX calendar's core safety property — **a non-qualifying or empty KRX response never retracts a prior positive Trading-Session witness by absence** — was proven at the leaf (`witness.rs` returns `NonEvidence` for degenerate responses, and `reconcile.rs` preserves an existing witness). But the maintainer refresh builder that *assembles* candidate snapshots re-violated the same invariant one layer up, because it merged evidence at **whole-source** granularity instead of **per-(source, date)**. A single-date incremental refresh therefore retracted the entire history of positive witnesses.

## Symptoms

- An incremental refresh that fetches one date (`scope.through`) marked the whole KRX source `ok`, then dropped all prior KRX evidence across every date and re-added only the single re-gathered record.
- Historical days whose only proof of being open was a KRX positive witness (reconciliation row 1: witness overrides an inferred closure) reverted to inferred `Closed` in the candidate.
- Under Enforced ingest, such a mislabeled `Closed` then advances coverage without a gateway fetch — false coverage — so the build-layer bug fed a runtime-safety hazard downstream.
- Every leaf test stayed green: the invariant holds in `witness.rs`/`reconcile.rs`; the violation lived only in the caller.

## What Didn't Work

- **Trusting the leaf tests.** `witness.rs` proves the property in isolation and passed throughout. It was the wrong altitude — the bug was in the code that calls reconcile over a merged evidence set, not in the rule the leaf encodes.
- **Reasoning about a full-history refresh only.** A full-history re-gather re-covers every date, so the wholesale per-source drop is harmless there and looks correct. The defect is only visible on a *partial* re-gather (the incremental mode), which the initial tests did not exercise end-to-end.

## Solution

Scope the drop of prior evidence to the exact window the successful gather actually re-covered — drop a prior record only when its source is `ok` **and** its date falls inside the re-gathered scope; retain prior evidence for any date the gather did not re-attest.

```rust
// candidate.rs — before (wrong: whole-source drop)
if ok_source_ids.contains(e.source_id.as_str()) {
    // skip ALL prior evidence from this source, across ALL dates
    continue;
}

// after (right: per-(source, date), scoped to the re-covered window)
let re_covered = ok_source_ids.contains(e.source_id.as_str())
    && e.date >= scope.from && e.date <= scope.through;
if re_covered {
    // only this window is being replaced by the fresh gather
    continue;
}
```

A full-history scope spans every prior date, so its behavior is unchanged (still wholesale within the window) — the fix narrows only the partial-re-gather case.

Fixed on branch `feat/shared-offline-krx-calendar` (PR #190), with a regression test in `adapters/nautilus/tests/calendar_refresh.rs` that builds a prior snapshot with witnesses on multiple historical dates, re-gathers only one date, and asserts the non-re-gathered witnesses (and their reconciled `TradingSession` status) survive.

## Why This Works

The invariant "absence never retracts" is a statement about a *date*: no new evidence for date D cannot flip D. A whole-source drop treats absence-of-re-fetch for date D as license to remove D's existing evidence, which is precisely the retraction the invariant forbids. Re-scoping the merge to per-(source, date) makes the caller honor the same granularity the invariant is stated in.

## Prevention

- **Re-assert a safety invariant at the merge/assembly layer, not only at the leaf that first proves it.** When a pure function guarantees a property per item, any caller that batches those items (per-source, per-page, per-file) can re-violate it; add a test at the caller's altitude that exercises the *partial* case (a re-gather that covers fewer items than exist).
- **Match the merge granularity to the invariant's granularity.** If the property is "per date," the replace/drop step must key on date, never on a coarser bucket (source, page, file) that can sweep untouched items.
- **Test the partial path, not just the full path.** A full re-gather masks a whole-set drop bug because it happens to re-cover everything; the incremental/partial mode is where the defect lives.

## Related Issues

- Built during the shared offline KRX calendar work (#185, parent spec #184); fix surfaced by the code-review adversarial pass.
- Downstream sibling hazard (false coverage via a range endpoint): docs/solutions/logic-errors/per-date-gate-on-a-range-op-silently-advances-over-unchecked-dates.md
- Related domain concept: `Calendar Adoption State` (CONCEPTS.md) — the Enforced posture where a mislabeled `Closed` becomes a coverage-advance hazard.
