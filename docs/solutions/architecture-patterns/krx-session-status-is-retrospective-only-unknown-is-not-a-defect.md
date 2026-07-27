---
title: "KRX trading-session status is provable only retrospectively — same-day Unknown is the normal operating state, not a defect"
date: 2026-07-26
category: architecture-patterns
module: "adapters/nautilus/nautilus-ls-calendar reconcile authority matrix + lab dispatch check_calendar_date"
problem_type: architecture_pattern
component: tooling
severity: medium
applies_when:
  - "Investigating why `--dispatch` refuses on a trading morning with a calendar_date=Unknown non-deferrable red"
  - "A same-day (or same-morning) calendar snapshot refresh still reads status unknown for today's date"
  - "Deciding whether an Unknown day-status result is a calendar bug versus expected behavior before that day's KRX data is published"
  - "Extending or reviewing nautilus-ls-calendar's reconcile.rs authority matrix or its consumers"
related_components:
  - nautilus-ls-calendar
  - lab-dispatch-gate
  - calendar-adoption
  - production-ladder
tags:
  - krx-calendar
  - evidence-reconciliation
  - tri-state-day-status
  - retrospective-proof
  - unknown-override
  - dispatch-gate
  - non-deferrable-red
---

## Context

The KRX calendar (`adapters/nautilus/nautilus-ls-calendar/`) answers one civil date with a
tri-state day fact — `TradingSession` / `Closed` / `Unknown` — reconciled from evidence by the
settled authority matrix in `adapters/nautilus/nautilus-ls-calendar/src/reconcile.rs:9-37`. The
matrix is not symmetric, and the asymmetry is deliberate.

**A `TradingSession` can only be produced by an observed KRX positive witness.** Rule 1
(`reconcile.rs:13-15`) is the only rule that *creates* one:

```
//! 1. A KRX positive witness on an otherwise-inferred closure (a [`DeterministicRule`] or
//!    a [`HolidayFact`]) → [`TradingSession`] + [`WitnessOverridesInference`]. Observed
//!    operation wins; the inferred-closure claim is retained as *conflicting*.
```

Rule 3 (`reconcile.rs:19-22`) only *preserves* an already-accepted witness against a later
empty/malformed response ("Absence NEVER retracts"). No other rule in the matrix reaches
`TradingSession`. There is no "weekday and not a holiday ⇒ session" path anywhere in it.

**A `Closed` can be produced from facts that exist in advance.** Rule 4 (`reconcile.rs:23-25`)
is a KASI `HolidayFact` plus an applicable published `DeterministicRule`; rule 5
(`reconcile.rs:26-27`) is pure rule authority —

```
//! 5. Weekend / Labor Day / year-end per a published [`DeterministicRule`] → [`Closed`]
//!    (rule authority).
```

— and rule 6 (`reconcile.rs:28-29`) is an exceptional closure with a *cited* first-party
`ClosureNotice`. All three inputs are publishable ahead of the date.

**Everything uncovered falls to `Unknown`, and that is a success, not an error.** Rule 10
(`reconcile.rs:36-37`):

```
//! 10. No covering evidence (empty, or all-invalid/superseded) → [`Unknown`] (a successful
//!     factual result).
```

Put together: **closure is provable forward; a session is provable only backward.** A holiday or
a Saturday can be asserted months out from published rules. A trading session cannot be asserted
at all until the venue has been observed operating — and the venue is observed via a daily
market record that does not exist while you are still standing in the day. So the *current* day
reads `Unknown` for its entire duration, including the whole live trading morning.

### The forward `Unknown` was chosen, not inherited (session history)

This is not an accident of what the sources happened to provide. The genesis-snapshot
brainstorm settled it as an explicit trade-off — carried into
`docs/plans/2026-07-23-002-feat-krx-calendar-genesis-snapshot-plan.md` as a Key Decision:
*"Materialize through the operating horizon with honest forward Unknowns … without fabricating a
single session."* The same plan's acceptance example AE5 pins the resulting behavior — a
non-holiday weekday two weeks out queries as `Unknown`, and a dispatch gate consulting it "fails
closed by design". The alternative
considered and rejected was exactly the tempting one — infer forward sessions so the calendar
could answer "today" — and it was rejected because a fabricated session is indistinguishable
from a witnessed one once it is in the snapshot.

The design also proved itself against live data during that build. A spot-check expected
`2026-07-17` (제헌절) to be a witness-overrides-holiday case; investigation showed KRX was
*genuinely closed* that day — 제헌절 had been restored as a 2026 public holiday — so there was
no witness at all and `Closed` was correct. The witness-primary model caught a newly-restored
market holiday straight from live data with no hardcoded holiday knowledge. (session history)

The dispatch gate treats that `Unknown` as a hard refusal.
`adapters/nautilus/lab/src/dispatch/checks.rs:294-329` (`check_calendar_date`) is
`Tier::NonDeferrable` on every non-green arm — the blunt `LS_DISPATCH_DEFER` surface can never
proceed it (`checks.rs:295-297`) — and the only thing that proceeds an `Unknown` is a bound,
audited attended override:

```rust
CalendarDateFact::Unknown => match &ctx.unknown_override {
    Some(ov) if ov.covers(&ctx.today_kst, &ctx.run_id) => CheckOutcome::new(
        CHECK_CALENDAR_DATE,
        Tier::NonDeferrable,
        CheckStatus::GreenWithNote,
        format!(
            "Unknown KST date proceeded by a bound, audited attended override (run {}, citation {}/{}) — calendar status unchanged",
            ov.run_id, ov.citation.issuer, ov.citation.reference
        ),
    ),
    _ => red(
        "calendar date is Unknown (no covering evidence) — refused by default; only a bound, audited attended Unknown override proceeds",
    ),
},
```

`covers` is exact-match on both KST date *and* run id
(`adapters/nautilus/lab/src/dispatch/mod.rs:133-137`), and requires a well-formed structured
citation (`adapters/nautilus/lab/src/dispatch/mod.rs:124-131`) — one override authorizes one run on one day, never a standing
waiver.

`Unknown` is also kept typed and distinct from a broken calendar: no injected view, or any query
failure, maps to `CalendarDateFact::Unavailable`, never `Unknown`
(`checks.rs:91-107`), and `Unavailable` has **no** override path (`checks.rs:311-313`).

### The snapshot corroborates it

`adapters/nautilus/state/krx.calendar.json` (6090 rows, `2010-01-04 … 2026-09-06`):

- `freshness.evidence_refreshed_at` = `2026-07-23T12:37:23Z` — 21:37 KST on the 23rd, roughly
  six hours after that day's 15:30 KST close.
- The row for `2026-07-23` — the very day of that refresh — is
  `{"status": "unknown", "decisive_evidence": [], "conflicting_evidence": [], "alerts": []}`.

A refresh executed *after* the close still did not witness its own day. The last
`trading_session` row in the whole 6090-row snapshot is `2026-07-22`, and the last
`positive_witness` evidence record is `krx-witness-2026-07-22`.

Contrast the three row shapes:

| date | status | `decisive_evidence` | how it got there |
|---|---|---|---|
| `2026-07-22` (Wed) | `trading_session` | `["krx-witness-2026-07-22"]` | observed, backward |
| `2026-07-23` (Thu) | `unknown` | `[]` | refresh day itself — no witness yet |
| `2026-07-25` (Sat) | `closed` | `["rule-2026-07-25"]` | rule authority, forward |
| `2026-08-15` (Sat, 광복절) | `closed` | `["kasi-2026-08-15", "rule-2026-08-15"]` | holiday fact + connecting rule, forward |

And the forward horizon makes the asymmetry undeniable. `coverage.materialized_through` is
`2026-09-06`, so the snapshot has a row for every date out to then. Of the 46 rows after the last
witness (`2026-07-22`): **15 `closed`, 31 `unknown`, and zero `trading_session`.** Weekdays
`2026-07-27` … `2026-07-31` all read `unknown`; the weekend `2026-08-01`/`2026-08-02` reads
`closed` on `rule-…`. The calendar confidently schedules closures a month and a half out
(`deterministic_rule` records run through `2026-12-25`) and cannot name a single future trading
day.

## Guidance

**Treat an `Unknown` on the current KST date as the normal, correct operating state of a live
trading morning. Do not "fix" it.**

1. **It is not staleness, and refreshing will not help.** The witness for today does not exist
   yet; re-running the refresh tooling regenerates the same `unknown` row. This is exactly what
   the `2026-07-23` row demonstrates — refreshed at 21:37 KST *on* the 23rd, still `unknown` for
   the 23rd.

2. **Do not make `Unknown` fail-open**, and do not add an inference like "weekday and not a KASI
   holiday ⇒ `TradingSession`". The matrix deliberately has no such rule: rules 4 and 5 turn
   deterministic rules and holiday facts into `Closed` only, never into a session. An inferred
   session would authorize live order flow on a day the venue may never have opened — precisely
   the failure mode `checks.rs:74-77` records as retired ("a KRX holiday used to read as a
   Trading Session here").

3. **The intended path is the bound, audited attended override** — `UnknownOverride`, matched on
   date + run id, carrying an operator, a reason, and a structured first-party citation. Note
   the message it emits: *"calendar status unchanged"*. The override authorizes the run; it does
   **not** rewrite the day fact, and no belief about today leaks into the snapshot.

4. **`Closed` is never overridable.** `CalendarDateFact::Closed` reds with no override arm
   (`checks.rs:308-310`) — "no override proceeds a proven closure". Only the *unproven* state
   is attendable.

5. **Use the right question.** Ask the calendar *"is this date scheduled closed?"* and it answers
   forward, for months. Ask *"did this date trade?"* and it answers only after the fact. If a
   diagnostic reads `unknown` for a future weekday, that is the calendar working — it means "no
   scheduled closure applies and no witness exists yet", not "data missing".

6. **Freshness is an orthogonal dimension and never moves a day status.**
   `adapters/nautilus/nautilus-ls-calendar/src/freshness.rs:3-9` states it outright: staleness is *"**strictly
   separate from day status**"*, reads only the `Freshness` block plus the as-of instant, and
   "NEVER reads, touches, or rewrites any `DayStatus`". Correspondingly
   `DiagnosticOutcome::is_usable()` (`adapters/nautilus/nautilus-ls-calendar/src/diagnostics.rs:58-66`) counts
   `Healthy`, `Stale`, `Unknown` **and** `Conflict` as usable; only `OutOfRange` and `Load(_)`
   are failures. So a `stale` diagnostic is not the reason today is `Unknown`, and clearing the
   staleness will not clear the `Unknown`.

## Why This Matters

Any Enforced consumer gated on a proven `TradingSession` **cannot self-serve on the current
day**. That is a structural property of the design, not a gap to be engineered away. The
production-ladder dispatch gate is the concrete case: `check_calendar_date` runs early in
`run_checks` (`adapters/nautilus/lab/src/dispatch/checks.rs:503` — third, after
`check_trading_env` and `check_advisory_lock`), it is non-deferrable, and on today's date it will
read `Unknown` and refuse — so an unattended agent can never mount a live
session by itself. Attendance is enforced by the calendar's epistemology, not by a flag.

The tempting "fixes" are all safety regressions dressed as bug fixes:

- Making `Unknown` fail-open converts the single non-deferrable gate protecting live order flow
  into a no-op.
- Inferring a session from weekday-and-no-holiday reintroduces exactly the defect the Ladder
  cutover retired: a KRX-specific closure (an exceptional venue closure, a substitute holiday not
  in the KASI feed, a system halt) reads as a green trading day.
- Widening the override from `covers(date, run_id)` to a standing waiver turns a one-run,
  one-day, cited authorization into a permanent bypass with no audit trail.

Each of those looks like a small local change, and each of them silently deletes the property
that makes the calendar trustworthy: **a `TradingSession` in this system always means somebody
observed the venue trading.** The cost of the design is the daily attended override; the benefit
is that no status in the snapshot is ever a guess.

The schema is built to prevent the adjacent conflation too: `Coverage` keeps
`materialized_through`, `retrospectively_checked_through` and
`scheduled_closure_evaluated_through` as separate fields specifically "so no caller conflates
'we have positive daily rows through X' with 'we retrospectively re-checked through Y' or 'we
evaluated scheduled closures through Z'" (`adapters/nautilus/nautilus-ls-calendar/src/schema.rs:103-105`). A row
existing for a date says nothing about whether that date was witnessed.

## When to Apply

- A dispatch gate, readiness report, or diagnostic reads `unknown` for **today** during KST
  market hours and you are about to investigate a stale snapshot, a broken loader, or a missed
  refresh.
- A future weekday reads `unknown` in `adapters/nautilus/state/krx.calendar.json` while a nearby weekend or
  holiday reads `closed`, and it looks like partial materialization.
- You are considering a change to `reconcile.rs`'s authority matrix, to
  `check_calendar_date`'s `Unknown` arm, or to `UnknownOverride::covers`.
- You are wiring a new Enforced consumer on the calendar and need to decide what it does when
  the day fact is `Unknown` (answer: refuse, and require an attended override — never infer).
- A `stale` freshness dimension appears alongside an `Unknown` day and the two look causally
  linked (they are not).

## Examples

**The refresh that could not witness its own day.** `adapters/nautilus/state/krx.calendar.json`,
`freshness.evidence_refreshed_at = 2026-07-23T12:37:23Z` (21:37 KST, ~6 h post-close), yet:

```json
{"date": "2026-07-22", "status": "trading_session", "decisive_evidence": ["krx-witness-2026-07-22"], "conflicting_evidence": [], "alerts": []}
{"date": "2026-07-23", "status": "unknown",         "decisive_evidence": [],                          "conflicting_evidence": [], "alerts": []}
```

The 22nd is proven because a witness exists. The 23rd — the refresh's own day — is `Unknown` by
rule 10, with an empty `decisive_evidence`. Nothing is broken.

**Rule-closed vs. witness-proven, side by side.** The weekend after that refresh:

```json
{"date": "2026-07-25", "status": "closed", "decisive_evidence": ["rule-2026-07-25"], "conflicting_evidence": [], "alerts": []}
{"date": "2026-07-26", "status": "closed", "decisive_evidence": ["rule-2026-07-26"], "conflicting_evidence": [], "alerts": []}
```

`decisive_evidence` is a `deterministic_rule` record (`source_id: "krx-rule"`), not a witness.
The rule was assertable before the date arrived — that is what "provable forward" means. A
holiday adds a second decisive id: `2026-08-15` carries
`["kasi-2026-08-15", "rule-2026-08-15"]` — the KASI holiday fact plus the connecting rule of
matrix rule 4. Note the matrix's own guard on that pairing (`reconcile.rs:24-25`): "A holiday
fact with NO connecting rule is NOT `Closed` → `Unknown`."

**The forward horizon.** Rows exist all the way to `coverage.materialized_through = 2026-09-06`,
but past the last witness they split cleanly:

```
2026-07-27 .. 2026-07-31  (Mon–Fri)   unknown   decisive_evidence: []
2026-08-01, 2026-08-02    (Sat, Sun)  closed    decisive_evidence: ["rule-2026-08-01"], ["rule-2026-08-02"]
```

Across all 46 rows after `2026-07-22`: 15 `closed`, 31 `unknown`, **zero** `trading_session` —
while `deterministic_rule` evidence records extend to `2026-12-25`. The calendar can schedule
closures five months out and cannot name one future trading day. That is the asymmetry, in data.

**What a refusal looks like, and what proceeds it.** With no override, today's `Unknown` yields
a `Tier::NonDeferrable` red:

> `calendar date is Unknown (no covering evidence) — refused by default; only a bound, audited attended Unknown override proceeds`

With an `UnknownOverride` whose `kst_date` and `run_id` both match this run
(`adapters/nautilus/lab/src/dispatch/mod.rs:135-136`) and whose operator/reason/citation fields are all non-blank
(`adapters/nautilus/lab/src/dispatch/mod.rs:124-131`), the same arm returns `CheckStatus::GreenWithNote` — and says so explicitly:
*"calendar status unchanged"*. The run proceeds; the day is still `Unknown`.

**The orthogonal signal.** A snapshot can be simultaneously `stale` on a freshness dimension and
perfectly correct on day status: `freshness.rs:3-9` forbids staleness from touching `DayStatus`,
and `DiagnosticOutcome::is_usable()` (`diagnostics.rs:58-66`) returns `true` for `Stale`. If you
see `stale` and `unknown` together, they are two independent facts — fix the refresh cadence if
the cadence is wrong, but do not expect it to change today's day fact.

## Related

- [A safety escape hatch wired to `None` at the composition root is dead code](a-safety-escape-hatch-wired-to-none-at-the-composition-root-is-dead-code-its-unit-tests-still-pass.md)
  — the attended `UnknownOverride` this doc names as "the intended path" shipped **unreachable**;
  that doc is the defect, this one is the epistemology it depends on.
- [Legacy → Shadow → Enforced adoption gate playbook](legacy-shadow-enforced-adoption-gate-playbook.md)
  — the rollout vehicle that makes `Unknown` a live, non-deferrable red for Enforced consumers.
- [A per-date gate on a range op silently advances over unchecked dates](../logic-errors/per-date-gate-on-a-range-op-silently-advances-over-unchecked-dates.md)
  — a distinct hazard on the same gate, also turning on what the calendar can and cannot prove.
- [A safety invariant proven at a leaf can be re-violated by a coarser-grained caller](../logic-errors/safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md)
  — the leaf-level witness machinery this evidence model rests on.
- [A composition root must emit before any fallible parse](../conventions/composition-root-always-emit-before-fallible-parse.md)
  — what a consumer owes at the moment it resolves the calendar.
