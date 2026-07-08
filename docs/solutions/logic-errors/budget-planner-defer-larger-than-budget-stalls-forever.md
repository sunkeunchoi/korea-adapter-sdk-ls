---
title: "A stop-before-cliff budget planner must PROCEED when a task exceeds the whole budget, not defer it forever"
date: 2026-07-08
category: logic-errors
module: "adapters/nautilus ingest (src/ingest/budget.rs plan_dispatch, src/ingest/mod.rs run/run_accumulate)"
problem_type: logic_error
component: ingest
severity: high
symptoms:
  - "A per-task pre-dispatch budget check that compares estimated cost against remaining budget silently skips a task forever once the estimate exceeds the whole budget — no cold window ever resumes it"
  - "The stop-before-cliff planner was also wired into only one of the two run modes, so the acceptance path (range mode) never consulted it"
root_cause: logic_error
resolution_type: code_fix
tags:
  - budget
  - rate-limit
  - igw00201
  - planner
  - defer
  - ingest
  - spend-ledger
related:
  - docs/solutions/integration-issues/ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md
---

## Problem

The budget-aware ingest (KTD-3 / R8) added a pre-dispatch planner: before fetching a
symbol, compare its estimated page cost against the remaining IGW00201 budget window; if
it doesn't fit, **defer** (skip, schedule the remainder for a cold window) instead of
provoking a throttle. The first cut deferred whenever `estimated > remaining`. Two defects
hid in that one line of logic.

## Symptoms

- **Permanent stall:** a symbol whose estimate exceeds the *entire* `budget_calls` (not
  just the remaining slice) defers on every run — including a fully **cold** budget, where
  `remaining == budget`. `estimated > remaining` is always true, so no cold window ever
  resumes it. The symbol becomes a silent, unrecoverable coverage gap.
- **Feature inert in the acceptance path:** the planner lived only in `run_accumulate`, but
  the turn-4 acceptance script drives `ls-ingest` in the default **range** mode
  (`run()`), which never called it. The headline "stop before the cliff" behavior could
  never fire in the documented usage.

Both were latent — the committed model ships `budget_calls: null` (plan-ahead off), so the
defect only bites once measured numbers are promoted.

## What Didn't Work

The intuitive guard `if estimated_pages > remaining { Defer }` reads correctly ("not enough
budget left → wait") but conflates two different questions: *"does it fit the remaining
window?"* and *"can it EVER fit a window?"*. A task bigger than the whole budget answers
"no" to the second — and for that task, deferring is not "wait for later," it is "never."

## Solution

Defer only when the task would fit a **fresh** budget but not the **current** remaining one.
If it exceeds the whole budget, proceed and let the in-process recovery arms
(backoff-and-narrow) chip away at it — the drip resumes idempotently.

```rust
// adapters/nautilus/src/ingest/budget.rs — plan_dispatch
let remaining = budget.saturating_sub(spent.min(u32::MAX as u64) as u32);
// Defer ONLY when the triple would fit a fresh/cold budget window
// (`estimated <= budget`) but not the remaining one. If it exceeds the whole
// budget, no cold window can ever fit it — deferring would stall that symbol
// forever — so proceed and let the in-process IGW00201 recovery arms narrow it.
if estimated_pages > remaining && estimated_pages <= budget {
    BudgetDecision::Defer { estimated: estimated_pages, remaining }
} else {
    BudgetDecision::Proceed
}
```

And wire the same guard into **both** run modes (`run()` range mode and
`run_accumulate`), so the acceptance path is actually budget-aware:

```rust
// run() (range mode) — mirror the accumulate-mode planner, inert when budget_calls is None
let decision = match self.ledger.lock() {
    Ok(led) => budget::plan_dispatch(&self.budget_model, &led, &self.cred_hash, now_unix(), estimated),
    Err(_) => budget::BudgetDecision::Proceed, // advisory: a poisoned ledger never blocks ingest
};
```

## Why This Works

A "defer" is a promise that a later run will pick the task up. That promise is only
keepable when some reachable state (an emptier budget window) makes the task dispatchable.
For a task larger than the maximum budget, no such state exists, so "defer" degenerates into
"drop." Gating the defer on `estimated <= budget` restores the invariant: **defer is
reachable-work-postponed, never work-abandoned.** Oversized tasks fall through to the
existing bounded recovery machinery, which makes partial forward progress every run.

## Prevention

- **Any throttle/quota planner that can *skip* work needs a liveness invariant:** every
  skipped item must have a reachable future state where it runs. Write the test that
  asserts it — a cold-budget item at or above the budget ceiling must `Proceed`, not
  `Defer`:

  ```rust
  #[test]
  fn triple_too_big_for_the_whole_budget_proceeds_not_defers_forever() {
      let model = BudgetModel { budget_calls: Some(10), window_secs: 1000, ..Default::default() };
      let led = SpendLedger::default(); // cold: remaining == budget == 10
      assert_eq!(plan_dispatch(&model, &led, "h", 0, 25), BudgetDecision::Proceed);
      assert_eq!(plan_dispatch(&model, &led, "h", 0, 10), BudgetDecision::Proceed); // boundary
  }
  ```

- **When a guard is added to one code path, grep for its siblings.** A planner that only
  runs in `run_accumulate` but not `run()` is a half-wired feature; the acceptance path is
  the one that matters. Wire shared guards through a shared helper and call it from every
  mode.
- **Keep advisory data advisory.** The planner reads a best-effort spend ledger; a poisoned
  lock must fall through to `Proceed`, never `.expect()`-panic the run — the gateway stays
  ground truth.
