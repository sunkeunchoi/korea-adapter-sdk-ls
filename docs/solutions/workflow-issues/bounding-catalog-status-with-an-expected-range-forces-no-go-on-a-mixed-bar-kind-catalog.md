---
title: "`catalog status` with an expected range is a whole-catalog span assertion — on a mixed-bar-kind catalog it is a guaranteed NO-GO that says nothing about catalog health"
date: 2026-07-30
category: workflow-issues
module: "adapters/nautilus lab-research catalog status (lab/src/runner/research.rs: catalog_status_gated, StatusConfig::expected_range, env_range/LS_STATUS_SDATE/LS_STATUS_EDATE) + the morning chain (lab/RUNBOOK-session-morning.md Step 4, scripts/session-morning.sh step [9])"
problem_type: workflow_issue
component: tooling
severity: medium
applies_when:
  - "Running `lab-research catalog status` on `data/turn4-fresh` or any catalog holding both 1-DAY and 1-MINUTE series"
  - "Deciding whether to set LS_STATUS_SDATE / LS_STATUS_EDATE on a go/no-go check"
  - "A catalog status run flags `front truncation` / `tail undershoot` on every minute series while the daily series are clean"
  - "Following the session-morning runbook's Step 4 'Optional sanity check' or the `[9] catalog status` step of scripts/session-morning.sh"
  - "Interpreting a NO-GO verdict before deciding whether an ingest is incomplete"
related_components:
  - lab-research
  - ls-ingest
  - krx-calendar
  - session-morning-runbook
tags:
  - catalog-status
  - go-no-go
  - expected-range
  - mixed-bar-kind-catalog
  - false-negative
  - session-morning
---

## Context

`lab-research catalog status` is the ingest→backtest go/no-go. It reads the catalog's bars,
groups them by `(instrument, bar-kind)`, and per group compares the observed span against a
boundary the KRX calendar can *prove*
(`adapters/nautilus/lab/src/runner/research.rs:1214` onward). It has two check modes, and the
difference between them is the whole of this learning.

**Without an expected range** the only check is the tail check against the group's *own*
checkpoint watermark: `checkpoint.watermark(&instrument, &bar_kind)` → `last_session_on_or_before(wm)`
→ flag only if `last < sess` (`research.rs:1226-1234`). Each series is judged against the
progress the ingestor recorded *for that series*. A series that is deliberately frozen — its
watermark and its data agree — is clean by construction, whatever the rest of the catalog is
doing.

**With an expected range** (`LS_STATUS_SDATE` + `LS_STATUS_EDATE`, parsed into
`StatusConfig::expected_range` at `research.rs:1968` via `env_range`, which requires both vars
together or neither, `research.rs:1659-1673`) a second, *both-direction* block runs:
`front truncation` via `first_session_on_or_after(exp_start)` (`research.rs:1252-1277`) and
`tail undershoot` via `last_session_on_or_before(exp_end)` (`research.rs:1278-1302`). This block
sits inside the same `for ((instrument, bar_kind), group) in groups` loop and **does not filter
by bar kind** — the operator-supplied window is asserted against *every* series in the catalog.
Any non-empty `flags` sets `go = false` (`research.rs:1304-1306`), and the run prints
`status: NO-GO` (`research.rs:1351`).

So the expected range is not a *query filter* and not a hint. It is a global assertion: "every
(instrument, bar-kind) series in this catalog spans this window." On `data/turn4-fresh`, which
holds 150 watermarked series — 75 `1-DAY` at `20260729` and 75 `1-MINUTE` deliberately frozen at
`20260710` (35) and `20260722` (40), verified in
`data/turn4-fresh/catalog/ingest-checkpoint.json` — that assertion is false by design, because
the daily catch-up ingest advances only the daily series
(`adapters/nautilus/scripts/session-morning.sh:478`, `LS_INGEST_KIND=daily`). Supplying an expected range
built from the *daily* window therefore guarantees NO-GO, and the NO-GO carries no information
about whether the ingest did its job.

## Guidance

**On this repo's mixed-bar-kind catalog, run `catalog status` unbounded.** The unbounded form is the
one whose verdict is about catalog health:

```sh
cd adapters/nautilus
LS_DATA_HOME=/ABSOLUTE/path/to/data-home \
LS_CALENDAR_SNAPSHOT=/ABSOLUTE/path/to/krx.calendar.json \
  ./target/debug/lab-research catalog status
```

The snapshot is mandatory — see the sibling trap below.

**Supply an expected range only when every series in the catalog is meant to cover that exact
window.** The bounded form is:

```sh
LS_DATA_HOME=/ABSOLUTE/path/to/data-home \
LS_CALENDAR_SNAPSHOT=/ABSOLUTE/path/to/krx.calendar.json \
LS_STATUS_SDATE=20240110 LS_STATUS_EDATE=20240216 \
  ./target/debug/lab-research catalog status
```

That is legitimate in exactly two shapes:

1. **A single-bar-kind catalog** — only daily, or only minute. There is one bar-kind family, so the
   window is unambiguous.
2. **A catalog where every series is meant to be current for the same span** — the fresh-home turn
   recipe. `adapters/nautilus/scripts/turn3-ingest.sh` ingests daily and then bounded minute over the frozen symbol
   set, and only then prints the status invocation with
   `LS_STATUS_SDATE=$LS_TURN3_SDATE LS_STATUS_EDATE=$LS_TURN3_EDATE`
   (`adapters/nautilus/scripts/turn3-ingest.sh:50-66`; the same recipe appears in `adapters/nautilus/lab/README.md:236-238`). Note
   what it picks: `LS_TURN3_SDATE` is the **minute** window start, not the earlier daily
   `daily_sdate`. The expected range there is the *intersection* of what all series cover — the
   only correct way to choose it on a multi-series catalog.

The general rule: **the expected range must be a window every series in the catalog is supposed to
span. If the series disagree, either narrow the range to their intersection or drop it entirely.**
On `data/turn4-fresh` today there is no useful intersection — the minute series stop weeks before
the daily ones — so drop it.

**Read the verdict line, not the shell's opinion of the run.** The `status: GO` / `status: NO-GO`
line (`research.rs:1351`) is the channel. At the current tree the CLI *does* map the verdict to
the process exit code — `Ok(ok_fail(out.go))` at `research.rs:1821`, with `ok_fail` returning
`ExitCode::FAILURE` on `false` (`research.rs:1709-1715`) — so a NO-GO should exit non-zero. In
the 2026-07-30 session the bounded NO-GO run was *observed* to leave `$?` at 0; the cause was a
pipeline (`| tail`) reporting the last stage's status rather than `lab-research`'s. Either way
the operational habit is the same one the runbook already teaches for ingest —
*"Verify with the watermark, never the exit code"* (`adapters/nautilus/lab/RUNBOOK-session-morning.md`) — and
`session-morning.sh` itself hedges with
`|| say "WARNING: catalog status returned non-zero — read its verdict below"`. Do not infer the
verdict from `$?`; read the line.

**Distinguish the three NO-GO families before reacting.** They print differently and mean
different things:

- `[front truncation: ...]` / `[tail undershoot: ...]` **per series** — a span assertion failed.
  If an expected range was supplied, first ask whether the assertion was even appropriate for
  that series.
- `NO-GO — calendar indeterminate: ...` — the calendar cannot *prove* the boundary (an `Unknown`
  sits where the boundary would be, `research.rs:136-138`). Says nothing about the catalog.
- `NO-GO — calendar unavailable: ...` — no snapshot injected, or the boundary date is outside the
  materialized coverage window (`research.rs:166-172`, `research.rs:189-195`). Also says nothing
  about the catalog.

Only the first family is ever about data, and only when the range was chosen correctly.

## Why This Matters

The trap is not the code — the both-direction check is doing exactly what its doc comment says
it is for, *"an optional operator-supplied expected range, turning on both-direction span checks
(front truncation is undetectable from the checkpoint alone)"* (`research.rs:1129-1131`), and
`adapters/nautilus/lab/README.md:192` describes it the same way. The trap is that the morning chain steers you
into supplying it on a catalog where it cannot hold.

The runbook's Step 4 "Optional sanity check" previously read: *"Bound the query to the last
proven session; querying through today guarantees `calendar indeterminate` on a boundary that
cannot be proven yet."* The intent of "bound the query" is *don't evaluate through today* — a
warning about the `calendar indeterminate` family. But the imperative reads as an instruction to
**set** `LS_STATUS_SDATE`/`LS_STATUS_EDATE`, and there is no other knob it could mean. Follow
that reading on `data/turn4-fresh` and you get a guaranteed false NO-GO with 75 flagged minute
series. The same wording appeared as a prevention bullet in
[`todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective`](./todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md).

Worse, the pre-staged script baked the same reading in executably: step `[9] catalog status` set
`LS_STATUS_SDATE="$lookback" LS_STATUS_EDATE="$session_compact"`, where `lookback` defaults to
`20260518` and `session_compact` is today's daily target. Both endpoints come from the **daily**
ingest's geometry; neither has anything to do with the frozen minute series. That chain merged in
PR #231, so every scripted morning run would have printed NO-GO at step `[9]`. All three sites
were corrected alongside this entry.

The cost is misdirected operator attention at the worst moment. A NO-GO in the morning chain,
minutes before the universe deadline, reads as "the ingest is incomplete — resume, do not
proceed" (the runbook's own instruction for a partial run). Chasing a phantom incomplete ingest
burns the pace budget the gate at step `[8]` exists to protect. The correct read is: the daily
catch-up succeeded, the minute series are frozen on purpose, and the check was asked a question it
could only answer "no" to.

**This had already been diagnosed once and never written down (session history).** On 2026-07-27
a session ran the bounded form against this same catalog, got NO-GO, and correctly decomposed it
— proving the 75 daily series uniform through the last calendar-permitted session and concluding
*"every NO-GO through 07-24 came from minute undershoot only."* No workaround was applied and no
`LS_STATUS_*` value was tuned to force green, which is the right instinct. But the finding stayed
in that session, so 2026-07-30 re-derived it from scratch. That is the specific waste this entry
exists to stop.

Session history also surfaces an **ordering risk** worth naming: on both 2026-07-23 and
2026-07-27, a `catalog status` NO-GO was diagnosed as a missing `LS_CALENDAR_SNAPSHOT` within the
first turn — the documented sibling cause is available and correct often enough to short-circuit
investigation before anyone looks at series coverage. When the snapshot *is* present and the
verdict is still NO-GO, series geometry is the next thing to check, not the last.

This belongs to a recognized failure family in this repo: **a `catalog status` verdict that is
not about the catalog.** The sibling is documented in the same runbook section (Step 4) — running
without `LS_CALENDAR_SNAPSHOT` makes every symbol report `calendar unavailable`, "a different
NO-GO with the same headline." Same shape: a red verdict produced by how the check was invoked,
not by what is on disk. And the family has a mirror on the GO side — `catalog status` has twice
been recorded printing GO on a catalog that was not fit: GO with only 1 of 20 symbols holding
minute bars, and GO on a catalog whose on-disk files were duplicate-polluted, because the check
reads deduped coverage (see Related). The durable lesson across all four: **`catalog status`
answers a narrow, invocation-dependent question. Neither its GO nor its NO-GO is a general
statement about catalog fitness.**

## When to Apply

Apply this before typing `LS_STATUS_SDATE=` — the decision point is invocation, not
interpretation.

- **Reach for it when** you are about to run `catalog status` on a catalog holding more than one
  bar kind, or on any catalog where different series are advanced by different ingests. Check the
  series first (see Examples); if the watermarks disagree by bar kind, run unbounded.
- **Reach for it when** a status run comes back NO-GO with per-series `front truncation` /
  `tail undershoot` flags concentrated on one bar kind. Before treating it as a data problem,
  re-run the identical command with `LS_STATUS_SDATE`/`LS_STATUS_EDATE` unset. If the verdict
  flips to GO seconds later on the same catalog, the flags were the assertion, not the data.
- **Reach for it when** the snapshot is present and the verdict is still NO-GO — the missing-snapshot
  explanation is the one that short-circuits diagnosis, so rule it in or out first and then move to
  series geometry rather than stopping.
- **Do not apply it** to a genuine single-series catalog or a fresh-home turn where daily and minute
  were both ingested over the same window. There the expected range is the *point*: front
  truncation is undetectable from the checkpoint alone, and dropping the range would hide a
  real short-front catalog. The unit test
  `front_truncation_is_flagged_only_with_an_expected_range`
  (`adapters/nautilus/lab/tests/research_cli.rs`) pins exactly that asymmetry — the fixture is a
  no-go with the range and a GO without it.

## Examples

**Check the series before choosing a form.** One command tells you whether an expected range can
possibly hold:

```sh
python3 -c "
import json, collections
d = json.load(open('/ABSOLUTE/path/to/data-home/catalog/ingest-checkpoint.json'))
w = d['watermarks']
c = collections.Counter()
for k, v in w.items():
    kind = '1-DAY' if '1-DAY' in k else ('1-MINUTE' if '1-MINUTE' in k else k)
    c[(kind, v)] += 1
print('total series', len(w))
for k, n in sorted(c.items()): print(k, n)
print('gaps', d.get('gaps'), 'shifted', d.get('shifted'))"
```

On `data/turn4-fresh` on 2026-07-30 this printed:

```
total series 150
('1-DAY', '20260729') 75
('1-MINUTE', '20260710') 35
('1-MINUTE', '20260722') 40
gaps [] shifted {}
```

Two bar kinds, three distinct watermarks, no common window. Run unbounded.

**The false NO-GO (observed 2026-07-30).** With the daily-derived range supplied:

```sh
LS_CALENDAR_SNAPSHOT=... LS_STATUS_SDATE=20260518 LS_STATUS_EDATE=20260729 \
  lab-research catalog status
```

every minute series picked up both flags —

```
360750.XKRX 1-MINUTE: 12573 bars, 2026-05-26..2026-07-10  [front truncation: first 2026-05-26 > first session 2026-05-18 (expected start 2026-05-18); tail undershoot: last 2026-07-10 < last session 2026-07-29 (expected end 2026-07-29)]
...
status: NO-GO
```

Both flags are literally true and both are irrelevant. The minute backfill started 2026-05-26,
not 2026-05-18; the minute series are frozen at 2026-07-10, not advanced to 2026-07-29. Neither
was ever supposed to match the daily window.

**The same catalog, seconds later, unbounded:**

```sh
LS_DATA_HOME=... LS_CALENDAR_SNAPSHOT=... lab-research catalog status
...
status: GO
```

Zero flags. The daily series reached their watermark (`20260729`, `gaps []`, `shifted {}`) and the
frozen minute series match theirs — which is the actual question the morning chain needed
answered.

**The legitimate bounded form**, from the fresh-home turn-3 recipe (`adapters/nautilus/lab/README.md:236-238`),
where daily and minute were both ingested to the same edate and the sdate is the minute series's
start:

```sh
LS_DATA_HOME=./data-turn3 LS_STATUS_SDATE=20240110 LS_STATUS_EDATE=20240216 \
  cargo run --bin lab-research catalog status   # must be GO before proceeding
```

**If you must bound a mixed-bar-kind catalog**, choose the intersection, not the daily window: the
latest series start and the earliest series end across all bar kinds. On the 2026-07-30 state that
would be `LS_STATUS_SDATE=20260526 LS_STATUS_EDATE=20260710` — an assertion so narrow it proves
almost nothing, which is itself the signal that the unbounded form is the right tool here.

## Related

- `adapters/nautilus/lab/RUNBOOK-session-morning.md` Step 4, "Optional sanity check" — the section
  that steered into this, now rewritten to forbid `LS_STATUS_*` here. It also carries the sibling
  trap (missing `LS_CALENDAR_SNAPSHOT` → every symbol reports `calendar unavailable`, the same
  headline from a different cause).
- [`ls-gateway-igw00201-bulk-minute-ingest-drip-feed`](../integration-issues/ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md)
  — the GO-side mirror: `catalog status` printed GO with only 1 of 20 symbols holding minute bars.
- [`re-ingesting-an-overlapping-range-duplicates-catalog-bars`](../logic-errors/re-ingesting-an-overlapping-range-duplicates-catalog-bars.md)
  — the other GO-side mirror: GO on a duplicate-polluted catalog, because the check reads deduped
  coverage; bar counts far above the trading-day span are the real tell.
- [`unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe`](./unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe.md)
  — why the morning ingest is bounded to the catalog's existing symbols in the first place, and
  another instance of "verify with the consumer, not the status verdict".
- [`todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective`](./todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md)
  — the neighbouring calendar-proof constraint on the same post-close catch-up run; its
  "bound the status query" prevention bullet was corrected alongside this entry.
