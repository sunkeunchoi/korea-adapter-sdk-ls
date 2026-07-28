---
title: "t8410 daily-chart window degenerates when sdate == edate — the gateway ignores sdate and returns qrycnt bars, and collect_daily's untrimmed rows hit APPEND REFUSED"
date: 2026-07-28
category: integration-issues
module: "adapters/nautilus ingest (collect_daily, src/ingest/mod.rs) + ingest-checkpoint.json 1-DAY watermarks"
problem_type: integration_issue
component: tooling
severity: high
symptoms:
  - "One-session morning daily accumulate fails APPEND REFUSED (overlap) on t8410 for every symbol, even though only the single new session bar is expected — the standard shape after a weekend watermark skip-advance makes start = watermark+1 == last_closed, a single-day window"
  - "A/B raw-probe proof of the degenerate filter: a t8410 request with sdate == edate and qrycnt=900 returns ~500 rows / 87,710 bytes (qrycnt bars ending at edate, sdate ignored), while a week-wide window returns ~6 rows / 1,503 bytes"
  - "collect_daily fetches ~2 years of bars for a one-day request because it never trims returned rows to the requested [sdate, edate] window, then collides with multi-range stored coverage"
  - "catalog compact reports all-clean while the accumulate still refuses — the on-disk state is coverage fragmentation, not duplicate-bar pollution, so dedup-based checks cannot see it"
  - "ls-ingest exits 0 with 0 bars and zero watermark movement, silently — it has no tracing subscriber, so background runs show nothing beyond the startup lines"
root_cause: wrong_api
resolution_type: config_change
related_components:
  - "tooling"
tags:
  - "ls-gateway"
  - "t8410"
  - "single-day-window"
  - "append-refused"
  - "coverage-fragmentation"
  - "watermark"
  - "daily-ingest"
  - "collect-daily"
---

# t8410 daily-chart window degenerates when sdate == edate — the gateway ignores sdate and returns qrycnt bars, and collect_daily's untrimmed rows hit APPEND REFUSED

## Problem

A session-morning catalog advance (bounded daily accumulate over the 75-symbol
universe via `ls-ingest`) silently stalled: runs exited cleanly but **zero
watermarks moved**. The accumulate window was the normal morning-after shape —
`start = watermark + 1`, `edate = last_closed` — but a weekend skip-advance had
left the watermark at Sunday `20260726`, so the request collapsed to a
**single-day window** `sdate == edate == 20260727`.

When `sdate == edate`, the LS gateway's t8410 window filter **degenerates**: it
ignores `sdate` and returns `qrycnt` daily bars ending at `edate`. With
`daily_qrycnt: 900` (`adapters/nautilus/src/ingest/mod.rs:781`), that is ~500
trading days (`20240703..20260727`) instead of the one requested day.
`collect_daily` keeps every returned row with no `[sdate, edate]` trim — the
row loop at `mod.rs:1006-1010` (`for row in &resp.outblock1 {
build_daily_bar(...) }`) has no date filter — so the oversized batch reaches
`append_bars_checked` (`mod.rs:2832`), whose disjointness guard refuses the
write with `AdapterError::OverlapRefused` (`mod.rs:2849`). The watermark is
withheld, and every subsequent run re-derives the identical degenerate request:
a permanent, self-renewing stall.

This is the `qrycnt=900` sibling of the already-documented `qrycnt=1`
degeneracy (probe-level, observed 2026-07-27) in
[`mount-universe-producer-cannot-be-fed-on-a-session-morning`](../architecture-patterns/mount-universe-producer-cannot-be-fed-on-a-session-morning.md).
Per prior-session verification, the ingest path had never issued a degenerate
single-day window before — 2026-07-28 was the first morning-after
single-session catch-up under the new session-morning runbook, which is why the
failure had no earlier occurrence to learn from. (session history)

At refusal time the stored coverage was multi-range
(`[20260518..20260703, 20260706..20260710, 20260713..20260722, 20260723..20260724]`),
which makes the ~500-day batch a genuine non-identical overlap. The
fragmentation's precise origin is not established: 2026-07-27's forced
range-mode fragment was reportedly absorbed by that day's bounded catch-up and
coverage read uniform that evening, so it was either reintroduced by the
2026-07-28 partial (killed) runs or recorded at weekend boundaries during the
catch-up — the sessions cannot distinguish which. (session history) The
mechanism does not depend on the origin: any multi-range coverage turns the
over-fetch into a refused overlap.

## Symptoms

- Two 10-minute `ls-ingest` runs with **zero watermark movement** and no output
  beyond the startup lines. `ls-ingest` has **no tracing subscriber** —
  `RUST_LOG` produces nothing, so the refusals were invisible in background runs.
- **Exit code 0 with 0 bars appended** — indistinguishable from "nothing to
  do". Never read success from the exit code; this is also the fully-blocked
  signature.
- A foreground single-symbol run (`LS_INGEST_SYMBOLS=005930`) exposed it
  instantly (message source: `adapters/nautilus/src/bin/ls-ingest.rs:399`):

  ```
  APPEND REFUSED (overlap): 005930.XKRX 1-DAY — attempted 20240703..20260727
  overlaps stored coverage [20260518..20260703, 20260706..20260710,
  20260713..20260722, 20260723..20260724]; run `lab-research catalog compact`
  (duplicate pollution) or wipe + full re-pull / fresh catalog (disjoint
  coverage). Watermark not advanced.
  ```

- The tell inside that message: the attempted range spans **~500 trading days**
  (a full `qrycnt` page) while the derived request window was a single day
  (`start == last_closed == 20260727`, from the watermark derivation at
  `mod.rs:2169-2172`, the watermark having skip-advanced to Sunday `20260726`).

## What Didn't Work

1. **Diagnosing it as an IGW00201 throttle.** A raw-probe at 09:45 did return
   `IGW00201` — the gateway budget was genuinely dead at market open and
   refilled later — but that was a compounding, *separate* issue. Waiting out
   the budget changed nothing: the stall is deterministic, not rate-limited.
2. **`lab-research catalog compact`** (the remediation the refusal message
   itself suggests first). It reported **every series clean**: the multi-range
   coverage is legitimate fragmentation, not duplicate pollution, and compact
   does not merge disjoint fragments. Compact-reports-clean is itself
   diagnostic — it rules out the duplicate-pollution branch of the message.
3. **Re-running / letting the next session retry.** The request window is
   re-derived identically from the unchanged watermark every run, so the
   degenerate single-day request recurs forever. (A killed run also leaves a
   0-byte `.ls-ingest.lock` dotfile in the runtime catalog directory
   (`$LS_INGEST_CATALOG`) — remove it before resuming.)

## Solution

**Decisive A/B raw-probe** (credential-safe classifier; prints only
http/rsp_cd/body_len), varying **only** `sdate` against a wide-window control:

```bash
# Degenerate: sdate == edate → gateway IGNORES sdate, returns a full qrycnt page
make raw-probe LS_PROBE_TR_CD=t8410 LS_PROBE_PATH=/stock/chart \
  LS_PROBE_BODY='{"t8410InBlock":{"shcode":"005930","gubun":"2","qrycnt":900,"sdate":"20260727","edate":"20260727","cts_date":" ","comp_yn":"N"}}'
# → http=200 rsp_cd=00000 body_len=87710   (~500 rows: sdate IGNORED)

# Control: >=2-day window → filter LIVE
make raw-probe LS_PROBE_TR_CD=t8410 LS_PROBE_PATH=/stock/chart \
  LS_PROBE_BODY='{"t8410InBlock":{"shcode":"005930","gubun":"2","qrycnt":900,"sdate":"20260720","edate":"20260727","cts_date":" ","comp_yn":"N"}}'
# → http=200 rsp_cd=00000 body_len=1503    (~6 rows: window filter LIVE)
```

**Verified workaround — widen the window by rolling the watermark back to the
last real bar date.** Back up first (precedent name:
`ingest-checkpoint.json.bak-<date>-preadvance`), then roll every `*|1-DAY`
watermark from `20260726` (skip-advanced Sunday) to `20260724` (last real bar
date). The watermark values are plain `YYYYMMDD` strings:

```python
import json
p = "data/turn4-fresh/catalog/ingest-checkpoint.json"
d = json.load(open(p))
for k, v in d["watermarks"].items():
    if k.endswith("|1-DAY") and v == "20260726":
        d["watermarks"][k] = "20260724"
json.dump(d, open(p, "w"), indent=2)
```

This widens the accumulate window to `20260725..20260727` (multi-day) → the
gateway filter engages → only the `20260727` bar returns → it appends
disjointly above stored coverage → the watermark advances to `20260727` with
calendar-proven continuity. Verified across all 75 symbols; `gaps`/`shifted`
empty.

**Verification — watermark census, never exit code:**

```python
import json, collections
d = json.load(open("data/turn4-fresh/catalog/ingest-checkpoint.json"))
print(collections.Counter(
    v for k, v in d["watermarks"].items() if k.endswith("|1-DAY")
))
print("gaps", d["gaps"], "shifted", d["shifted"])
# want: every 1-DAY watermark at the just-closed session date; gaps/shifted empty
```

**Durable fix (recommended; NOT implemented as of this writing):** trim fetched
daily bars to the requested `[sdate, edate]` in `collect_daily`
(`adapters/nautilus/src/ingest/mod.rs:956`, row loop at `mod.rs:1006-1010`).
Until it lands, every single-session catch-up — the *normal* morning-after
shape — requires the watermark-rollback workaround.

## Why This Works

- The gateway behavior is proven by the A/B probe, not inferred: identical
  request bodies except `sdate`, 87,710 bytes vs 1,503 bytes. `sdate == edate`
  degenerates the filter; a ≥2-day window filters correctly. Widening the
  window therefore attacks the actual trigger, not a symptom.
- The client trusts the gateway's windowing: `fetch_daily_page`
  (`mod.rs:821-841`) passes `sdate`/`edate` straight into `T8410Request` and
  `collect_daily` keeps every row of `outblock1` untrimmed. With the filter
  live, the response *is* the requested window, so the batch is disjoint from
  stored coverage and passes `append_bars_checked`'s guard.
- Rolling the watermark to the **last real bar date** is safe because
  re-fetching `20260725..20260726` (weekend) returns nothing to overlap; the
  fail-closed guard (`mod.rs:2832-2859`) still protects against any surprise.
- The refusal path itself was working as designed — it correctly stopped ~500
  days of overlapping rows from polluting the catalog. The bug is upstream
  (untrimmed fetch meeting a degenerate gateway filter), which is why the fix
  belongs in `collect_daily`, not in loosening the guard.

## Prevention

1. **Land the durable trim** (pending): filter `collect_daily`'s kept rows to
   `[sdate, edate]` before they reach `append_bars_checked`. This closes the
   whole class regardless of gateway windowing quirks.
2. **Recognize the degenerate-window shape**: after a weekend/holiday
   skip-advance, `start == last_closed` (watermark + 1 == last closed session)
   means every daily accumulate issues a single-day t8410 request — assume the
   window filter is dead and expect a full-qrycnt page. The same family of
   degeneracy was first seen at probe level with `qrycnt=1` (see Related).
3. **Verify with the watermark census, never the exit code**: `ls-ingest` exits
   0 with 0 bars both when fully caught up and when fully blocked, and it has
   no tracing subscriber so background runs are silent. Run the `Counter`
   check over `ingest-checkpoint.json` after every advance.
4. **Compact-reports-clean means fragmentation, not duplicates**: when the
   APPEND REFUSED message's first remediation (`catalog compact`) finds
   nothing, the stored multi-range coverage is legitimate — stop pursuing the
   duplicate-pollution branch and inspect the *attempted* range instead. An
   attempted range vastly wider than the derived request window is the
   degenerate-filter signature.
5. **A/B probe discipline**: classify gateway behavior with `make raw-probe`
   pairs that vary exactly one field against a wide-window control, and include
   a known-closed-day control when dates are in play. Body length alone
   (87,710 vs 1,503) separated "filter dead" from "filter live" without ever
   printing credentialed payloads.

## Related Issues

- [`mount-universe-producer-cannot-be-fed-on-a-session-morning`](../architecture-patterns/mount-universe-producer-cannot-be-fed-on-a-session-morning.md)
  — the `qrycnt=1` flavor of the same t8410 window degeneracy, the
  wide-window/edate-step probe methodology, and the same-era range-mode
  APPEND REFUSED observation
- [`re-ingesting-an-overlapping-range-duplicates-catalog-bars`](../logic-errors/re-ingesting-an-overlapping-range-duplicates-catalog-bars.md)
  — the pre-guard era of the same overlap class (silent duplication instead of
  refusal) and the read-side dedup that masks it
- [`ls-gateway-t8412-chart-all-pagination-burst-and-silent-truncation`](ls-gateway-t8412-chart-all-pagination-burst-and-silent-truncation.md)
  — the sibling chart-TR gateway-contract deviation (tr_cont header vs body
  cursor) in the same collect path
- [`unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe`](../workflow-issues/unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe.md)
  — the bounded catch-up invocation the failing run uses, and the 2026-07-27
  incident that preceded this one
- [`todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective`](../workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md)
  — why the catch-up is forced onto the session morning at all
- [`ls-gateway-igw00201-bulk-minute-ingest-drip-feed`](ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md)
  — a drip-feed loop that treats APPEND REFUSED as a done-marker, which is
  unsafe under degenerate windows
- GitHub #104 (closed) — the per-triple refusal contract (`OverlapRefused` /
  APPEND REFUSED) whose guard this bug trips spuriously
