---
title: "A per-date gate evaluated on one representative date but guarding a whole-range operation silently advances coverage over the un-checked dates"
date: 2026-07-19
category: logic-errors
module: "nautilus-ls ingest accumulate gate (adapters/nautilus/src/ingest/mod.rs, CalendarGate)"
problem_type: logic_error
component: tooling
symptoms:
  - "Under the Enforced calendar adoption state, an accumulate run whose range endpoint last_closed is a proven Closed date advanced the watermark to last_closed, marking every intervening proven Trading Session covered with ZERO bars written"
  - "Reachable on the first Enforced backfill (no watermark, start = lookback_floor), so the whole [lookback_floor, last_closed] span could be skipped on a Closed endpoint"
  - "The gate was consulted as gate.action(last_closed) — a single date — while the fetch it replaces covers the whole range [start, last_closed]"
  - "A pre-existing U9 test was silently ENCODING the bug: its 'single-date proven-Closed advance' case used lookback_floor=2010-06-14 / last_closed=2010-06-19, a range that actually spanned the 06-15 and 06-17 Trading Sessions"
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - nautilus-ls-calendar
  - ingest-accumulate
  - calendar-gate
tags:
  - false-coverage
  - range-vs-point
  - gate-granularity
  - test-encoding-the-bug
  - offline-krx-calendar
---

# A per-date gate evaluated on one representative date but guarding a whole-range operation silently advances coverage over the un-checked dates

## Problem

The whole point of the KRX calendar is that a proven `Closed` date may skip its gateway fetch and still advance coverage (there is nothing to fetch), while an un-attested date must never be marked covered. The Enforced ingest accumulate gate evaluated the calendar on only the **single endpoint** date `last_closed`, but the fetch it was replacing covers the whole **range** `[start, last_closed]`. When the endpoint was `Closed`, the skip-and-advance branch advanced the watermark past every intervening Trading Session in the range — the exact false-coverage hazard the calendar exists to prevent, produced via the range endpoint rather than via `Unknown`.

## Symptoms

- Enforced accumulate with `last_closed` proven `Closed` set `checkpoint.set_watermark(&instrument, &label, last_closed)` and `continue`d, so `(start, last_closed)` was never fetched and became permanently covered (next run starts at `last_closed + 1`).
- Worst on the initial backfill: with no watermark, `start = lookback_floor`, so a multi-week span could be skipped because its last day happened to be a weekend/holiday.
- The single-date provenance guard (only `Closed` advances, never `Unknown`) held for `last_closed` itself but said nothing about the interior of the range.
- A pre-existing test was asserting the buggy behavior — its "single-date" scenario actually spanned two Trading Sessions, so it green-lit the skip.

## What Didn't Work

- **Gating on the endpoint as a proxy for the range.** `gate.action(last_closed)` answers "is *this date* skippable," not "is *every date from start to here* skippable." A range operation needs a range answer.
- **Relying on the existing accumulate test suite.** One test was structurally encoding the defect (a "single-date" case whose date span was actually multi-day), so it passed and masked the bug rather than catching it.

## Solution

Gate the skip-and-advance on a scan of the **whole** range, reusing the continuity machinery already built for the checkpoint-merge path (`scan_continuity` / `ContinuityDecision`):

```rust
// A new range-aware gate method, instead of gate.action(last_closed):
pub fn range_action(&self, start: NaiveDate, last_closed: NaiveDate) -> GateAction {
    match self.scan_inclusive(start, last_closed) {   // scans [start, last_closed]
        // every date proven Closed (incl. the single-date case) -> safe to skip+advance
        ContinuityDecision::AllClosed   => GateAction::SkipAdvance,
        // any proven Trading Session in the range -> do NOT skip; fetch it
        ContinuityDecision::TradingPresent => GateAction::Proceed,
        // any Unknown/unavailable -> stop, preserve state, no advance
        ContinuityDecision::Indeterminate  => GateAction::Stop,
    }
}
```

The caller computes `start` identically to the fetch start (`watermark.succ_opt()` or `lookback_floor`) and dispatches on `range_action(start, last_closed)`. Legacy/Shadow are unaffected — they never produce `SkipAdvance`. The masking test was corrected to a genuine single-date case (seed the watermark so `start == last_closed`). Fixed on branch `feat/shared-offline-krx-calendar` (PR #190) with three new range tests (all-Closed advances; intervening Trading Session fetches; intervening Unknown stops).

## Why This Works

A gate that authorizes an *advance over a span* must be evaluated over that whole span. Scanning `[start, last_closed]` and skip-advancing only when it is entirely `Closed` makes the coverage claim exactly as strong as the evidence: a single un-attested Trading Session anywhere in the range forces a real fetch (or a stop on `Unknown`), so coverage never runs ahead of proof.

## Prevention

- **When a gate guards a range operation, scan the range — never a single representative date.** A point check on an endpoint (or start) is a silent proxy that skips the interior.
- **Audit tests that assert the pre-fix behavior.** A "single-date" or "one representative case" test that actually spans multiple units will pass *because* of the bug. When adding a range-aware gate, re-derive what each existing test's inputs really cover.
- **Reuse the existing span-classifier.** This codebase already had `scan_continuity`/`ContinuityDecision` for the checkpoint-merge gate; a second range gate should share it rather than re-deriving an endpoint shortcut.

## Related Issues

- Built during the shared offline KRX calendar work (#185, parent spec #184); surfaced by the code-review correctness pass.
- Sibling build-layer safety bug from the same review: docs/solutions/logic-errors/safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md
- Related domain concept: `Calendar Adoption State` (CONCEPTS.md) — the Enforced posture where this false coverage becomes reachable.
