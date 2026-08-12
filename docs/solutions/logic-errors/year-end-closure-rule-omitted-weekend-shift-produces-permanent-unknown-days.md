---
title: "KRX year-end closure rule omitted the weekend-Dec-31 shift, leaving four days permanently Unknown"
date: 2026-08-12
category: logic-errors
module: "adapters/nautilus/src/calendar_refresh/normalize.rs fixed_closure_rules (deterministic rule generator)"
problem_type: logic_error
component: tooling
severity: high
symptoms:
  - "Accumulate-forward ingest stops at the first Unknown day, capping a planned 2016-08-01 daily backfill at ~104 sessions instead of the full 2,457"
  - "The weekend-Dec-31 years (2016, 2017, 2022, 2023) each leave their shifted year-end closure day (2016-12-30, 2017-12-29, 2022-12-30, 2023-12-29) permanently KRX trading-date status Unknown"
  - "No evidence source can ever resolve the day: KASI's public-holiday feed never flags exchange-only closures, and an empty KRX daily response can never prove Closed (absence-never-decides Witness gate)"
  - "The rule generator emitted a Dec-31 closure record only when Dec 31 itself fell on a weekday, so no record was ever produced for the shifted day"
root_cause: logic_error
resolution_type: code_fix
related_components:
  - nautilus-ls-calendar
  - calendar-refresh
  - calendar-activate
  - accumulate-forward-ingest
tags:
  - krx-calendar
  - deterministic-rule
  - year-end-closure
  - witness
  - accumulate-forward
  - unknown-day-status
  - incomplete-transcription
---

# KRX year-end closure rule omitted the weekend-Dec-31 shift, leaving four days permanently Unknown

## Problem

`fixed_closure_rules` — the generator for KRX exchange-only closures that no external
feed provides (`adapters/nautilus/src/calendar_refresh/normalize.rs:287-302`) — transcribed
KRX's published year-end closure rule only for the weekday case: it emitted a
`DeterministicRule` record for December 31 whenever December 31 itself was a weekday, and
did the same for May 1 (Labor Day). KRX's actual published rule
(「유가증권시장 업무규정 시행세칙」: *12월 31일이 공휴일 또는 토요일인 경우 직전
매매거래일*) closes the **preceding trading day** when December 31 falls on a weekend — a
shift/substitution clause the original transcription dropped entirely. The generator had no
code path that could ever emit a rule record for that substitute day.

Because the rule source is the only evidence class the reconcile authority matrix assigns
to a scheduled exchange-only closure (`adapters/nautilus/nautilus-ls-calendar/src/reconcile.rs:26-27`, row 5:
*"Weekend / Labor Day / year-end per a published `DeterministicRule` -> `Closed` (rule
authority)"*), the four in-range years where December 31 fell on a weekend
(2016, 2017, 2022, 2023) had no evidence record of any kind bearing on their substitute
closure day. No other source can supply it: KASI's `getRestDeInfo` feed reports only
national public holidays and never flags an exchange-only closure, so it never fires row 4
(holiday fact + rule). A KRX daily-market witness on the actual closed day returns an empty
response, and the KTD7 witness gate (`normalize.rs:15-21`, `witness_evidence` at
`normalize.rs:119-135`) deliberately yields `None` for any non-qualifying response — "no
record ever proves `Closed`" — so absence of trading can never be read as a positive
closure signal (reconcile row 10: no covering evidence -> `Unknown`, "a successful factual
result"). With the rule silent and every other source structurally unable to speak to the
date, reconciliation had nothing decisive and the day landed on `Unknown` — permanently,
since no future fetch of any of the three sources could ever change the outcome.

## Symptoms

The four affected days — 2016-12-30, 2017-12-29, 2022-12-30, 2023-12-29 — sat with
`status: "unknown"` and empty `decisive_evidence` in the live calendar snapshot since
genesis, with no error surfaced anywhere in the pipeline. The failure mode is silent
permanence: re-running the fetch and refresh ceremony against those dates would reproduce
the identical empty witness and the identical rule-less generator output every time,
because the KTD8 determinism guarantee (`normalize.rs:9-13`) makes every generated record a
pure function of the window — there was no transient or flaky element to retry.

Operationally this blocked the planned 2016-08-01 daily backfill: accumulate-forward
ingestion plans only the established prefix and stops before the first Unknown day
(`established_prefix`, `adapters/nautilus/src/ingest/mod.rs:287`; pinned by
`enforced_later_session_does_not_cross_the_first_unknown` and
`enforced_range_with_intervening_unknown_stops_and_preserves_state` in
`adapters/nautilus/tests/ingest.rs:3074` and `:3127`), so the backfill would have capped at
roughly 104 sessions instead of the intended 2,457 (the P1 sizing in
`docs/plans/2026-08-10-001-docs-next-strategy-lineage-scope-plan.md`). Nothing in the
gate, in `cargo test`, or in `calendar-status` flagged the four days as wrong; they simply
never resolved.

## What Didn't Work

1. **Re-fetching evidence for the affected days.** Running the standard fetch ceremony
   against 2016-12-30 (or any of the other three) returns the same empty KRX daily response
   and no KASI holiday fact — the gap was never in what the live sources returned, it was in
   the generated-rule source, which is produced deterministically and not fetched at all
   (`assemble_inputs`, `adapters/nautilus/src/calendar_refresh/fetch_state.rs:277-280`:
   *"The generated rule evidence (weekends + fixed closures) always spans the whole window —
   it is produced deterministically, not fetched."*). No amount of re-fetching the two live
   sources could ever populate it.

2. **A cited first-party `ClosureNotice`.** Reconcile row 6 (`reconcile.rs:23-24`) allows an
   exceptional closure with a cited first-party notice to establish `Closed` directly, which
   would sidestep the rule gap entirely. But no injection tooling exists for that evidence
   kind — the fetch binary produces exactly three sources (`krx-daily`, `kasi`, `krx-rule`;
   `normalize.rs:39-44`), and none of them is a notice-citation path. Building that tooling
   for a one-off historical gap would have been substantially more code than completing the
   rule transcription.

3. **Hand-editing the snapshot to mark the four days `Closed`.** Forbidden by design: Unknown
   is never treated as Closed, and snapshots flow only through the candidate/activation
   ceremony (fetch -> refresh -> diff-gate -> approval -> activate), never direct edits. This
   would also have produced a record with no decisive evidence backing it, which the reconcile
   invariants don't allow to originate outside the ceremony.

## Solution

Two parts: complete the rule, then run each historical day through the standard resolution
ceremony (PR #270).

**(a) Complete the transcription.** `year_end_closure_day(year)` now walks backward from
December 31 over Saturday/Sunday to the preceding weekday
(`normalize.rs:266-279`):

```rust
fn year_end_closure_day(year: i32) -> Option<NaiveDate> {
    let mut date = NaiveDate::from_ymd_opt(year, 12, 31)?;
    while !is_weekday(date) {
        date = date.pred_opt()?;
    }
    Some(date)
}
```

`fixed_closure_rules` (`normalize.rs:287-302`) calls this instead of using December 31
directly, so the emitted `rule-<date>` record lands on the shifted day when December 31 is
a weekend and on December 31 itself otherwise. Labor Day (May 1) is deliberately left
unshifted — KRX publishes no substitute closure when May 1 falls on a weekend, a case the
weekend rule (`weekend_rules`, `normalize.rs:248-264`) already covers on its own.

The shift is a documented, bounded under-approximation of "preceding trading day": if the
weekday landed on happened to itself be a public holiday, the true closure would sit one day
earlier still. The comment at `normalize.rs:266-272` and *Why This Works* below spell out
why that gap can never produce a wrong `Closed` claim. A new test,
`year_end_closure_shifts_to_the_preceding_weekday_when_dec_31_is_a_weekend`
(`adapters/nautilus/tests/calendar_normalize.rs:168-196`), pins exactly the four affected
years (2016, 2017, 2022, 2023) against their expected shifted dates, plus the unshifted
weekday-December-31 case (2026) and a 1-day-window case (`DateRange::new(d(2016,12,30),
d(2016,12,30))`) that exercises the exact window shape the per-day ceremony below uses.

**(b) Resolve each historical day live**, one at a time, using the reusable 1-day-window
ceremony:

```
calendar-fetch-inputs --window <D>..<D> --krx-through <D>   # ~2 live calls: 1 KRX daily + KASI year pages
calendar-refresh --mode incremental --through <D>           # a past --through is safe (see below)
# gate the diff: expect exactly 1 additive status_established entry
#   Unknown -> Closed with decisive rule-<D>; 0 high-risk; 0 candidate alerts
# author the approval JSON, then:
calendar-activate
calendar-status --day <D>
```

A past `--through` is safe because coverage growth is `max(base.materialized_through,
scope.through)` (`adapters/nautilus/src/calendar_refresh/candidate.rs:155-156`) — a
`--through` behind the already-materialized frontier can never retract coverage, it is
simply a no-op on the `max`. The diff gates clean because `before == DayStatus::Unknown &&
after != DayStatus::Unknown` routes to the additive `DiffCategory::StatusEstablished`, not
the high-risk `HistoricalStatusChange` (`adapters/nautilus/src/calendar_refresh/diff.rs:138-146`),
and `NearTermClosureChange` only fires when `in_horizon(date)` is true
(`diff.rs:123,158-163`) — the operating horizon is bounded near today, so a historical date
years in the past cannot trip it regardless of `involves_closed`. Each cycle ran in roughly
two minutes; four chained cycles produced four snapshot generations, one per resolved day.

## Why This Works

The deterministic-rule source is exactly the evidence class the reconcile authority matrix
assigns to a scheduled exchange-only closure (row 5, `reconcile.rs:26-27`). Completing the
published rule's transcription doesn't add a new evidence class or bypass the matrix — it
makes the rule source able to say what only it is authorized to say, for the one case it
previously couldn't reach.

The safety argument for the bounded under-approximation is structural, not
probabilistic. Two failure directions matter:

- **A wrong `Closed` claim on a day the exchange actually traded.** This can only happen if
  a real KRX daily witness exists for that day — but a positive witness on an otherwise-
  inferred closure overrides the rule (row 1, `WitnessOverridesInference`,
  `reconcile.rs:14-16`) and surfaces as a retained conflict alert rather than a silent wrong
  answer. So the only days where a wrong `Closed` claim could go undetected are days with no
  witness at all — and for every in-window weekend-December-31 year (2011, 2016, 2017, 2022,
  2023) the claimed day was checked against KRX's actual historical closures and confirmed
  correct.

- **The residual approximation gap** (the shifted weekday itself being a holiday, pushing the
  true closure one day earlier) can never produce a false `Closed`: whichever day is the
  *actual* closure, the day the rule claims is closed is Closed either way (it's within the
  holiday-adjacent closure period per KRX's rule), so the approximation can under-shoot the
  precise date in a rare compound case but cannot mark a genuinely-open trading day as
  Closed.

Combined with the ceremony's structural safety (coverage is monotone,
`StatusEstablished` is additive by construction, and the operating horizon bounds
`NearTermClosureChange` away from historical dates), resolving each day was a
zero-risk, gate-verified operation rather than a judgment call.

## Prevention

1. **When transcribing a published exchange rule into a deterministic generator, transcribe
   the whole rule — including its shift/substitution clauses — and write a test for the
   shifted case specifically**, not just the common case. An under-transcribed rule doesn't
   fail loudly: it fails as a permanent `Unknown` that no later evidence fetch can ever
   cure, and the gap only surfaces when a range operation (a backfill, a coverage sweep)
   happens to hit it.

2. **When a day is stuck `Unknown` and re-fetching does not resolve it, ask which evidence
   class could ever decide it.** If the reconcile authority matrix (`reconcile.rs`) assigns
   the day's closure type to a generated/deterministic source rather than a fetched one, the
   gap is in the generator, not in the data — re-fetching is a dead end and the fix belongs
   in the rule function.

3. **The per-day 1-day-window resolution ceremony is the reusable mechanic for any future
   historical Unknown.** 2011-12-30 (a fifth weekend-December-31 year, below the 2016-08-01
   floor and deliberately out of the P1 scope) resolves the same way the moment a fetch
   window covers it, with no code change needed. Days whose gap is a missing *holiday fact*
   rather than a missing rule (2010-06-02, 2015-08-14) are a different defect class and stay
   gated on the floor-deepening item together with the `KRX_REGULAR_CLOSE` effective-date
   switch.

## Related Issues

- Fix PR: [#270](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/pull/270) (open,
  unmerged as of this writing — this doc travels on the same branch). The P1 queue
  item (`calendar-resolve-4-historical-unknown-days`) was staged by the next-lineage scope
  plan (`docs/plans/2026-08-10-001-docs-next-strategy-lineage-scope-plan.md`, PR #267),
  which also carries the KD2 re-derivation trigger — had 2016-12-30 or 2017-12-29 proved
  Open, `S_max = 2,457` and the derived sample split would have re-derived; both proved
  Closed, so KD2 did not fire. (session history)
- [`../conventions/exchange-rule-constants-need-an-effective-date-switch-before-history-is-acquired.md`](../conventions/exchange-rule-constants-need-an-effective-date-switch-before-history-is-acquired.md)
  — the adjacent defect class in the same neighborhood: a rule constant incomplete over
  *time* (missing effective-date switch), where this one was incomplete over the
  *weekday space* (missing shift clause). Different root cause and fix; both fail silently
  until the uncovered input is reached.
- [`../architecture-patterns/krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md`](../architecture-patterns/krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md)
  — the tri-state epistemology and reconcile authority matrix this fix conforms to.
- [`../workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md`](../workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md)
  and [`../logic-errors/per-date-gate-on-a-range-op-silently-advances-over-unchecked-dates.md`](per-date-gate-on-a-range-op-silently-advances-over-unchecked-dates.md)
  — sibling causes of the same halt symptom (the range gate stopping on Unknown), from
  witness lag and gate granularity respectively.
- [`../integration-issues/krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md`](../integration-issues/krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md)
  — a third distinct cause of a date reading Unknown (transport failure fabricating a dead
  source); this doc adds the fourth to that family (a rule that structurally cannot fire).
