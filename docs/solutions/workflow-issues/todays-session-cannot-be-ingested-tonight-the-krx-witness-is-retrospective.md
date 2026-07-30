---
title: "Today's session cannot be ingested tonight — the accumulate ingest is gated on a KRX witness that does not exist until the next day"
date: 2026-07-27
category: workflow-issues
module: "adapters/nautilus ls-ingest (src/ingest/mod.rs: CalendarGate::range_action / scan_inclusive), nautilus-ls-calendar reconcile authority matrix, lab-research catalog status"
problem_type: workflow_issue
component: tooling
severity: high
applies_when:
  - "Planning to 'ingest the catalog forward through the previous session' the night before an attended window"
  - "An `ls-ingest` accumulate run completes exit 0 with `0 bars across 0 triples (N skipped)` and makes no gateway calls"
  - "`lab-research catalog status` reports `NO-GO — calendar indeterminate ... Unknown at the boundary`"
  - "Deciding whether a catalog is current enough for tomorrow's mount-universe producer"
related_components:
  - ls-ingest
  - nautilus-ls-calendar
  - lab-mount-universe
  - production-ladder
tags:
  - ls-ingest
  - accumulate
  - krx-calendar
  - retrospective-witness
  - catalog-freshness
  - attended-window
---

## Problem

"Ingest the catalog forward through the previous session" reads like a night-before chore. It is
not runnable the night before, because the session you want is **today's**, and the ingest
refuses to advance a watermark into a date the calendar cannot prove was a trading session.

`ACCUMULATE_CLOSE_BUFFER` (16:30 KST) governs only `last_closed_session` — which date the run
*targets*. A second, independent gate decides whether that target may be fetched:
`CalendarGate::range_action(watermark+1, last_closed)` scans the frozen snapshot. Today's row is
`unknown` (a KRX positive witness is the only thing that creates `trading_session`, and it is
published retrospectively), so the scan returns `Indeterminate` → `GateAction::Stop`.

The run then completes **successfully**, having done nothing:

```
skipping universe load (LS_INGEST_SKIP_UNIVERSE_LOAD): 75 explicit symbols (catalog already populated)
ingest complete: 0 bars across 0 triples (75 skipped), 0 coverage gaps, 0 refused pending heal
budget: 75 symbols x 1 bar-kinds, paced to 1/s (>= 150s wall clock)
```

Exit code `0`. No refusals. No gaps. Nothing in the output says "the calendar blocked me" — the
only hint is the startup line, `day=2026-07-27:Unknown outcome=unknown action=enforced-active`,
which reads as routine banner noise. The `budget:` line even advertises a 150s pace for a run
that finished in seconds without touching the gateway.

## Symptoms

Observed 2026-07-27 (Monday), 23:07 KST — well past the 16:30 close buffer:

- 75 daily watermarks sat at `20260726` (Sunday) and would not advance to `20260727`.
- The bounded accumulate run exited `0`, skipped all 75 triples, and issued zero gateway calls.
- The checkpoint was **byte-identical** before and after.
- `lab-research catalog status` through `20260727` said
  `NO-GO — calendar indeterminate: ... last session at/before expected end 2026-07-27 cannot be
  proven (Unknown at the boundary)`.
- The same command through `20260724` showed all 75 daily series complete and uniform at
  `2026-05-18..2026-07-24` with zero warnings.

The catalog was already **exactly as current as the calendar permitted**. There was no work to do.

## What Didn't Work

**Reasoning from the close buffer.** `adapters/nautilus/README.md` — *"Last closed session
includes today only once now-KST is past 16:30 KST"* — is true and irrelevant to whether the
fetch is *allowed*. It sets the target, not the permission. Being past 16:30 licenses nothing.

**Refreshing the calendar snapshot.** The witness for today does not exist yet;
[`krx-session-status-is-retrospective-only-unknown-is-not-a-defect`](../architecture-patterns/krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md)
records a refresh run at 21:37 KST *on* 2026-07-23 that still produced `unknown` for the 23rd. A
refresh also needs `LS_KRX_APPKEY` + `LS_KASI_SERVICE_KEY`, which are owner-local and absent from
the lane env files — so this is not an agent-runnable escape hatch either.

**Reading `catalog status` without the calendar.** Run without `LS_CALENDAR_SNAPSHOT`, it reports
`NO-GO — calendar unavailable: ... outside calendar coverage` for every symbol — a *different*
NO-GO with the same headline, caused by the missing env var rather than by coverage. Diagnosing
freshness from that output sends you chasing the wrong fault.

## Solution

**Sequence the ingest for the morning of the session, not the night before.** The chain is:

1. **(operator, needs KRX/KASI credentials)** `calendar-refresh` — once KRX has published the
   previous day's daily-market record, its row flips `unknown` → `trading_session`.
2. **(agent)** re-run the bounded accumulate ingest; the watermarks now advance.
3. **(agent, after 09:00 KST)** `lab-mount-universe` for the session date.

The bounded ingest itself is unchanged and safe to re-run at any time — it is idempotent and
fails closed:

```sh
cd adapters/nautilus
LS_TRADING_ENV=paper \
LS_INGEST_LANE_FILE=/ABS/repo/.env.domestic \
LS_CALENDAR_SNAPSHOT=/ABS/repo/adapters/nautilus/state/krx.calendar.json \
LS_SPEND_LEDGER_FILE=/ABS/repo/data/turn4-fresh/state/spend-ledger.json \
LS_INGEST_CATALOG=/ABS/repo/data/turn4-fresh/catalog \
LS_NODE_LOCK_DIR=/ABS/repo/data/turn4-fresh/catalog \
LS_INGEST_KIND=daily LS_INGEST_MODE=accumulate \
LS_INGEST_SKIP_UNIVERSE_LOAD=1 LS_INGEST_LOOKBACK=20260518 \
LS_INGEST_SYMBOLS="<the catalog's existing shcodes>" \
  ./target/debug/ls-ingest
```

**If the refresh cannot happen before the window, the session is not necessarily blocked.** The
producer's eligibility rule is staleness, not same-day currency:
`(session_date - prior_date).num_days() <= MAX_PRIOR_STALENESS_DAYS` (10), and `select_prior`
takes the latest daily bar *strictly before* the session date. A Tuesday session with only
Friday on disk is 4 days — eligible, not refused.

That is a **fidelity** decision, not an availability one: `prior_close` and `prior_atr` would
come from Friday, so the head's overnight-gap term becomes a multi-session return that clears the
gap floor too easily. Decide it deliberately; do not discover it from the fill log.

## Why This Works

The two gates answer different questions and neither subsumes the other. The close buffer answers
*"which date is the last one whose session has ended?"* — a clock fact, available immediately. The
calendar gate answers *"is that date proven to have traded?"* — an evidence fact, available only
once the venue has been observed operating. Closure is provable forward; a session is provable
only backward. So the newest date the ingest can ever reach is the last **witnessed** session,
which lags the last **closed** session by roughly a day.

Skipping rather than erroring is correct: advancing a watermark over an unproven date would mark
it covered with zero bars, which is exactly the advance-on-empty the Enforced posture refuses.

## Prevention

**Read the watermark, never the exit code.** `0 bars / N skipped / exit 0` is the signature of a
fully-blocked run and of a fully-up-to-date one. Only the checkpoint distinguishes them.

**Always pass `LS_CALENDAR_SNAPSHOT` to `lab-research catalog status`.** Without it every symbol
reports `calendar unavailable`, which masks the real verdict.

**Do not let the status query evaluate through today.** Today's boundary cannot be proven yet, so
it yields `calendar indeterminate` — a NO-GO that says nothing about the catalog. Note this does
*not* mean "set `LS_STATUS_SDATE`/`LS_STATUS_EDATE`": on a mixed-bar-kind catalog an expected range is
a whole-catalog span assertion that the frozen minute series fail by design, which is its own
guaranteed false NO-GO. Run the watermark-gated form (`LS_DATA_HOME` + `LS_CALENDAR_SNAPSHOT`,
no `LS_STATUS_*`) — it keys each series to its own watermark and never reaches today's unprovable
boundary. See
[`bounding-catalog-status-with-an-expected-range-forces-no-go-on-a-mixed-bar-kind-catalog`](./bounding-catalog-status-with-an-expected-range-forces-no-go-on-a-mixed-bar-kind-catalog.md).

**Write the plan against the witness horizon, not the wall clock.** Any runbook step of the form
"ingest forward through the previous session, the night before" is unsatisfiable as written when
"the previous session" is today.

## Related

- [`krx-session-status-is-retrospective-only-unknown-is-not-a-defect`](../architecture-patterns/krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md)
  — the authority matrix that makes today `Unknown`, and the dispatch-gate consequence
- [`unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe`](unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe.md)
  — why the catch-up ingest must be bounded to the catalog's existing symbols
- [`mount-universe-producer-cannot-be-fed-on-a-session-morning`](../architecture-patterns/mount-universe-producer-cannot-be-fed-on-a-session-morning.md)
  — the consumer whose prior-session inputs this ingest supplies
