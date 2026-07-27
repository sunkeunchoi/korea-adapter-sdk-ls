---
title: "The mount-universe producer cannot be fed on a session morning — t8410 serves an in-progress daily bar, but every catalog ingest path refuses to store one"
date: 2026-07-27
category: architecture-patterns
module: "adapters/nautilus lab mount-universe producer (lab/src/runner/mount_universe.rs), lab backtest candidate builder (lab/src/runner/backtest.rs: build_candidates, select_prior_today), adapter ingest (src/ingest/mod.rs: last_closed_session, calendar_decision)"
problem_type: architecture_pattern
component: tooling
severity: high
applies_when:
  - "Preparing an attended rung-1 live session and needing LS_MOUNT_UNIVERSE_FILE resolved for TODAY's KST date"
  - "lab-mount-universe refuses for a PAST session date whose daily bar is not in the catalog"
  - "Deciding whether an in-session today_open must come from the catalog or from a live quote"
  - "ls-ingest reports `ingest complete: 0 bars` and exits 0 on a trading morning"
  - "Catalog bars are present for a date but the checkpoint watermark has not advanced"
related_components:
  - lab-mount-universe
  - ls-ingest
  - nautilus-ls-calendar
  - production-ladder
  - lab-live-mount
tags:
  - mount-universe
  - today-open
  - in-session-ingest
  - catalog-watermark
  - t8410
  - t8407
  - calendar-gate
  - coverage-fragmentation
  - silent-no-op
---

## Context

Rung-1 of the production ladder needs `--mount` to trade a universe file resolved for the
current KST session date. `lab-mount-universe` produces that file from the catalog — offline for
a past date — and every row carries a `today_open`.

Before building anything on that path, one assumption had never been tested: **does t8410
(`gubun="2"`, the daily chart TR) serve a daily bar for a session that is still in progress?**
The worry was that it might not, in which case `today_open` would need a different source.

The answer turned out to be yes — and irrelevant on its own, because the blocker sits one
layer further out than the question assumed.

## Guidance

**t8410 does serve a daily bar mid-session.** Verified 2026-07-27 with KRX open, credential-safe
via `make raw-probe` (which prints only `http`/`rsp_cd`/`body_len`). A single-day window is
*not* a valid test — with `qrycnt=1` the gateway returns the latest row regardless of `sdate`,
so a Sunday, a Friday, and today all returned an identical 626 bytes. The decisive shape is a
**wide window varying only `edate`**, reading the one-row step out of `body_len`:

| symbol | edate | body_len | delta |
|---|---|---|---|
| 005930 | 2026-07-23 (Thu) | 3276 | — |
| 005930 | 2026-07-24 (Fri) | 3453 | +177 = one row |
| 005930 | 2026-07-26 (Sun) | 3453 | +0 — weekend control |
| 005930 | 2026-07-27 (today, mid-session) | 3629 | **+176 = one row** |
| 000660 | 2026-07-24 (Fri) | 3527 | — |
| 000660 | 2026-07-27 (today, mid-session) | 3706 | **+179 = one row** |

A known-real trading-day row measures ~177 bytes; today's row measures the same. The Sunday
control adding zero bytes proves the window filter is actually live rather than ignored.

**But the producer reads the catalog, not the gateway.** `today_open` comes from the session
date's daily bar via `build_candidates` → `select_prior_today`
(`build_candidates` / `select_prior_today` in `adapters/nautilus/lab/src/runner/backtest.rs`), and **no supported ingest path
writes an in-session day**:

1. **`accumulate` is calendar-gated.** Today is `Unknown` (status is retrospective-only), which
   maps to `GateAction::Stop` — zero gateway requests, checkpoint preserved.
2. **`accumulate` is independently blocked by the close buffer.** `last_closed_session`
   (`adapters/nautilus/src/ingest/mod.rs:92`) applies `ACCUMULATE_CLOSE_BUFFER` = 16:30 KST
   (`:82`). Its own test asserts the intent in as many words: *"the watermark must not advance
   into an in-session day"* (KTD7).
3. **`range` mode for today hit `APPEND REFUSED (overlap)`** — it attempted `20240703..20260727`
   against fragmented stored coverage.

So the gap is **not** that the gateway lacks the data. It is that the offline catalog path
deliberately refuses in-session writes, and the producer has no other source.

### The fix (implemented 2026-07-27)

`today_open` is sourced from a live **t8407** `multi_symbol_current_price` quote when
`session_date == today (KST)`, and from the catalog for any past date. The seam is
`build_candidates_with_today_open`, which takes an optional per-instrument open map;
`build_candidates` delegates to it with `None`, so the backtest path is unchanged.

It stayed small because `today_open` is the *only* field that needs it — `prior_close`,
`prior_atr`, `prior_open_vol_mean` and `prior_illiq` are all prior-session data already in the
catalog. Three guards came out of review and are the load-bearing parts:

- **The override map is authoritative.** An instrument absent from it is not a candidate.
  Falling back to the catalog would mix live and catalog opens inside one file — and would do it
  for exactly the symbol the operator was told had no quote.
- **A same-day resolve is refused before 09:00 KST.** t8407 is a 현재가 board and answers
  outside the session with the *previous* session's snapshot, whose `open` is a perfectly
  positive integer — so `open > 0` cannot distinguish it and the clock has to.
- **A prior session older than 10 calendar days is refused.** The catalog path got this free
  (requiring a session-date bar proved ingest reached that date); the live path has to state it,
  or `gap_pct` silently becomes a multi-session return measured against an overnight-gap floor.

## Why This Matters

Two of the three refusals above are **silent**:

- `accumulate` on an Unknown date prints `ingest complete: 0 bars across 0 triples (1 skipped)`
  and **exits 0**. Nothing about that reads as "the pipeline refused"; a wrapper script checking
  only the exit code sees success.
- `range` mode is **not** calendar-gated the way `accumulate` is. Forcing 07-23..07-24 through it
  did write real bars (005930 went to 47 daily bars through 07-24), but the **watermark stayed at
  07-22** and coverage fragmented into a separate `[20260723..20260724]` range, because the
  calendar could not prove contiguity (`ContinuityDecision::Indeterminate` → ranges stay
  separate). Bars-present-but-watermark-stale is a genuinely misleading state:
  `lab-research catalog status` still prints **GO**.

The combination — data visible, watermark stale, status GO — is exactly the shape that gets a
session mounted against stale `prior_close`/`prior_atr` without anyone noticing.

### Stale snapshot makes PAST trading days read Unknown

Distinct from the documented retrospective-Unknown case
([`krx-session-status-is-retrospective-only-unknown-is-not-a-defect`](krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md)):
when the in-force snapshot's last KRX witness predates a closed trading day, that **past** day
also reads `unknown`. Observed on this run: the two most recent closed sessions before the
session date were both `unknown`, because the snapshot's evidence had last been refreshed before
they closed.

That is refresh staleness, not a design invariant. The remedy is a calendar refresh
(`calendar-fetch-inputs` → `calendar-refresh` → `calendar-activate`), which requires maintainer
`LS_KRX_APPKEY` / `LS_KASI_SERVICE_KEY` credentials and is therefore operator-attended, not
agent-runnable. Refreshing changes `artifact_id`, so any attended Unknown-override must be
authored **after** activation — it binds to the in-force snapshot identity.

## When to Apply

- Any time `lab-mount-universe` refuses for a session date, before assuming the producer is broken
- Before trusting `ls-ingest` exit code 0 as evidence that bars landed
- Before treating `lab-research catalog status: GO` as evidence that coverage is current
- When a session morning needs a universe file and the catalog stops at the prior session
- When choosing between "make the catalog accept in-session writes" (fights two deliberate
  guards) and "give the producer a live source for the one field that needs it" (narrow)

## Examples

Probe shape that actually discriminates — wide window, vary only `edate`:

```sh
# WRONG: qrycnt=1 returns the latest row regardless of sdate — Sunday and today
# both come back 626 bytes and the test proves nothing.

# RIGHT: fixed wide window, edate is the only variable; read the one-row step.
for D in 20260724 20260726 20260727; do
  make raw-probe LS_PROBE_TR_CD=t8410 LS_PROBE_PATH=/stock/chart \
    LS_PROBE_BODY="{\"t8410InBlock\":{\"shcode\":\"005930\",\"gubun\":\"2\",\"qrycnt\":20,\
\"sdate\":\"20260701\",\"edate\":\"$D\",\"cts_date\":\"\",\"comp_yn\":\"N\",\"sujung\":\"Y\"}}"
  sleep 3
done
```

Always include a **known-closed control** (a Sunday). If the control changes `body_len`, the
window filter is not being applied and the whole comparison is void.

The silent no-op to recognize:

```
calendar-startup consumer=ls-ingest adoption=enforced ... day=2026-07-27:Unknown
  outcome=unknown alerts=0 action=enforced-active
ingest complete: 0 bars across 0 triples (1 skipped), 0 coverage gaps, 0 refused pending heal
# exit 0
```

`0 bars` plus `day=<today>:Unknown` is a calendar refusal, not a completed ingest.

Empty-string numeric request fields are a separate trap that looks similar from the outside — a
shell quoting bug that left `edate` empty returned `http=500 rsp_cd=IGW40011` on every variant
(see [`ls-gateway-igw40011-numeric-request-fields`](../integration-issues/ls-gateway-igw40011-numeric-request-fields.md)).
Identical `rsp_cd` across all arms of an A/B usually means the request is malformed, not that the
arms are equivalent.

## Related

- [`krx-session-status-is-retrospective-only-unknown-is-not-a-defect`](krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md)
  — why today always reads Unknown, and the attended-override path
- [`re-ingesting-an-overlapping-range-duplicates-catalog-bars`](../logic-errors/re-ingesting-an-overlapping-range-duplicates-catalog-bars.md)
  — the overlap/append refusal that blocks a widened range re-pull
- [`ls-gateway-igw40011-numeric-request-fields`](../integration-issues/ls-gateway-igw40011-numeric-request-fields.md)
  — numeric request fields must serialize as JSON numbers
- `adapters/nautilus/lab/RUNG1-PREFLIGHT.md` §0.7 — the agent-runnable universe-resolution step
- `adapters/nautilus/lab/RUNBOOK-rung1.md` §4 — the attended mount and its universe preconditions
