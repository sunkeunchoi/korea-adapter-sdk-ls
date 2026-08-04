---
title: "A status-only gate is not evidence — and `all(is_ok)` over an empty outcome set is vacuously true"
date: 2026-08-04
category: logic-errors
module: "nautilus-ls calendar refresh candidate builder (adapters/nautilus/src/calendar_refresh/candidate.rs, build_from_base)"
problem_type: logic_error
component: tooling
symptoms:
  - "`forward_readiness_through` — the freshness dimension that drives the operator-facing `freshness=stale` verdict — advanced whenever every evidence source merely reported status Ok, without any check that a source's `covered` ranges reached the dates being claimed"
  - "`SourceOutcome::is_ok()` tests only `matches!(self.status, SourceFetchStatus::Ok)`; it never inspects `covered()` against the scope"
  - "`RefreshInputs::empty()` carries `outcomes: Vec::new()`, and `.all()` over an empty iterator returns true — so an input set with zero evidence satisfied the gate vacuously"
  - "The same weak test already governed `materialized_through` and `scheduled_closure_evaluated_through`, where it moved only bookkeeping; the defect was introduced by wiring a NEW consumer onto that pre-existing weakness"
  - "Caught in pre-merge review by three independent reviewers plus a cross-model adversarial pass — never shipped"
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - nautilus-ls-calendar
  - calendar-refresh
  - calendar-genesis
tags:
  - status-vs-evidence
  - vacuous-truth
  - empty-set-all
  - silent-refusal
  - freshness-signal
  - guard-granularity
  - offline-krx-calendar
---

# A status-only gate is not evidence — and `all(is_ok)` over an empty outcome set is vacuously true

## Problem

`forward_readiness_through` is the calendar snapshot's forward-readiness claim: the date through
which scheduled closures have been evaluated. It drives the operator-facing `freshness=stale`
verdict that `make next` and `catalog status` print.

A change made that field advanceable by an ordinary `calendar-refresh` (previously only genesis
could write it). The advance was gated on `all_sources_ok`:

```rust
let all_sources_ok = inputs.outcomes.iter().all(|o| o.is_ok());
```

That gate is not evidence of anything. Two independent holes:

1. **Status is not coverage.** `SourceOutcome::is_ok()` checks only the status enum. A source can
   report `Ok` while its `covered` ranges span far less than the requested scope — and the coverage
   fields widen to `scope.through` regardless. The snapshot would then report `fresh` for a forward
   span nobody evaluated.
2. **`all()` over an empty set is `true`.** `RefreshInputs::empty()` has `outcomes: Vec::new()`,
   so an inputs artifact carrying no evidence at all passed the gate.

Genesis already guards exactly this shape — `check_coverage_completeness` calls `uncovered_within`
per source and refuses on a short claim. The from-prior refresh path had no equivalent;
`uncovered_within` had exactly one call site, inside the genesis-only function.

## What Didn't Work

**Reasoning from the sibling fields.** The gate looked correct because coverage used the identical
test one block earlier, so it read as consistent with established behavior. It *was* consistent —
with a check whose weakness had never mattered. The pre-existing gap only became a defect when a
consumer with a stronger claim was attached to it.

**Trusting a green gate.** `make adapter-check` passed — 70 result lines, 1364 tests, zero failures
— with the defect present. The tests exercised only fixtures where every source covered the whole
window, so the gap was unreachable from the suite.

**The first regression test was vacuous.** A test named
`a_failed_source_cannot_advance_the_forward_horizon_by_absence` asserted a value that the monotone
`max` alone already produced: the shared fixture's horizon sat *ahead* of coverage, so deleting the
gate entirely left the assertion green. Compare
[per-date-gate-on-a-range-op-silently-advances-over-unchecked-dates](per-date-gate-on-a-range-op-silently-advances-over-unchecked-dates.md),
where a pre-existing test likewise encoded the bug it was meant to catch.

## Solution

Evidence the claimed span per source, mirroring genesis:

```rust
fn evidenced_forward_horizon(
    prior: Option<NaiveDate>,
    candidate_horizon: NaiveDate,
    inputs: &RefreshInputs,
) -> Option<NaiveDate> {
    let prior_horizon = prior?;
    if candidate_horizon <= prior_horizon {
        return prior;                       // monotone: nothing new is being claimed
    }
    let claim = match prior_horizon.succ_opt() {
        Some(from) => DateRange::new(from, candidate_horizon),
        None => return prior,
    };
    if forward_span_is_evidenced(claim, inputs) { Some(candidate_horizon) } else { prior }
}
```

`forward_span_is_evidenced` requires the KASI holiday source and the generated-rule source to each
be **present**, `ok`, and covering `claim` via `uncovered_within`. Requiring *presence* — not merely
"no failure" — is what makes the empty-outcome case fail closed. `KrxDailyMarket` is exempt: it
witnesses only the past, so requiring it to cover a forward span would make the horizon permanently
unadvanceable.

Only the span *past* the prior horizon needs fresh evidence; everything at or before it was
justified when it was claimed. That also keeps genesis byte-identical, since genesis seeds the prior
horizon at the window end and the `candidate_horizon <= prior_horizon` early return fires.

The fixture was reshaped so the horizon starts *behind* coverage
(`prior_with_lagging_forward_horizon`) — the realistic decayed shape, and the only ordering where
deleting the gate changes the result.

## Why This Works

The claim being made is "closures were evaluated through date X." The evidence for that claim is a
source's `covered` range, not its exit status. Checking status answers "did the fetch return
without error", which is a strictly weaker question than "did the fetch cover the dates I am about
to assert."

The empty-set case is the same error in extreme form: `all()` answers "no counterexample among the
outcomes I have", which is trivially satisfied when there are no outcomes. A presence requirement
converts the universal into the existential the claim actually needs.

## Prevention

**When attaching a new consumer to an existing check, re-derive the check against the new claim.**
The check does not become wrong; it becomes *insufficient*. Ask what the new consumer asserts, then
ask whether the check establishes that — not whether the check was fine before.

**Read `all()`/`any()` calls for their empty case explicitly.** `all()` over an empty collection is
`true` and `any()` is `false`. When the collection is externally supplied, name the empty case in a
test. Prefer a presence requirement over a pure `all()` when absence should fail closed.

**A guard that refuses silently needs a signal.** `evidenced_forward_horizon` returns the prior
value on refusal — indistinguishable from "nothing to extend." `diff_against_predecessor` compares
rows, coverage and evidence but **not** freshness, so an operator following the runbook would see an
unchanged `stale` verdict with no way to tell whether the fetch fell short or the code refused.
`calendar-refresh` now prints an explicit `forward_horizon=... REFUSED (asked for ...)` line. When
adding a guard, check whether the artifact the operator is told to review actually covers the field
being guarded.

**Verify a regression test by mutation, not by the suite being green.** Delete the guard and confirm
the test goes red. Here that check turned three of four new tests red and left one green — the green
one was the vacuous assertion. See
[coverage-only-change-is-verified-by-mutation-not-by-the-gate](../conventions/coverage-only-change-is-verified-by-mutation-not-by-the-gate.md).

**Watch for the fixture that cannot express the defect.** A helper that always builds contiguous,
fully-covering evidence structurally cannot reveal a short-coverage or interior-gap bug. Related:
[safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller](safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md)
— the third defect in this same builder whose root shape is granularity mismatch between a check and
the thing it guards.

## Residual risk

`fetch_kasi_year` treats a parseable zero-holiday KASI response as a fully covered year rather than
"not yet published", so a horizon crossing into an unpublished year can still be stamped. The guard
is only as good as the `covered` claim the fetcher reports. Recorded as an operator check in
`adapters/nautilus/RUNBOOK-calendar-snapshot.md`.
