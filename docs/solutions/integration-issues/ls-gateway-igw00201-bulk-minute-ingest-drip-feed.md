---
title: "LS gateway IGW00201 is a rolling call-count cap that aborts a bulk multi-symbol minute ingest — drip-feed one symbol at a time"
date: 2026-07-07
last_updated: 2026-07-08
category: integration-issues
module: "adapters/nautilus ls-ingest (src/bin/ls-ingest.rs, LS_INGEST_KIND=minute:*) + lab-research catalog status"
problem_type: integration_issue
component: tooling
symptoms:
  - "A fresh-home bulk minute ingest across ~20 symbols aborts partway with rsp_cd=IGW00201 (호출 거래건수를 초과하였습니다 — call-count exceeded), on t8412 page 1 of some later symbol, after several symbols already ingested cleanly"
  - "The SDK does NOT retry IGW00201 — it propagates as a hard error and the whole ls-ingest run exits non-zero, having ingested only the symbols processed before the trip"
  - "It trips sooner when the call budget is already warm (e.g. an earlier attended live session the same day, plus a max-lookback probe and a daily pass), even though each request is paced at the per-TR 1/s cap"
  - "lab-research catalog status reports GO even when only 1 of 20 symbols has minute coverage — a green status is NOT proof of per-symbol minute completeness"
root_cause: rate_limit
resolution_type: workaround
tags:
  - igw00201
  - rate-limit
  - ls-gateway
  - ingest
  - minute-bars
  - t8412
  - catalog-status
related:
  - docs/solutions/integration-issues/ls-gateway-t8412-chart-all-pagination-burst-and-silent-truncation.md
  - docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md
---

## Problem

A fresh-data-home minute-bar ingest of a ~20-symbol universe (`LS_INGEST_KIND=minute:1`,
`LS_INGEST_SYMBOLS=<20 shcodes>`) aborts partway through with `IGW00201`
(호출 거래건수를 초과하였습니다 — "call transaction count exceeded"), leaving only the
first-ingested symbols with minute coverage. The daily pass over the same 20 symbols
completes fine; only the much larger minute pass trips the cap.

## Symptoms

- `error: ingest gateway error (tr t8412, page 1, paced 1/s): API error IGW00201: 호출 거래건수를 초과하였습니다.`
  — surfaced on the **first** request of a *later* symbol, not the first symbol.
- The run exits non-zero and does not continue; symbols after the trip get no minute data.
- `lab-research catalog status` still prints `status: GO` because it reads **deduped**
  coverage and daily is complete for all 20 — the GO hides that only 1/20 symbols has
  minute bars.

## What Didn't Work

- **Assuming IGW00201 is a per-second bucket the 1/s pacing already respects.** The ingestor
  paces each request at the per-TR 1/s cap, yet the minute pass still tripped. IGW00201 here
  is a **cumulative rolling call-count budget** (per rolling window / day-ish), not a
  per-second rate — so steady 1/s pacing does not avoid it once the window is warm. (This is a
  different failure than the *within-single-request* continuation burst in the t8412 doc, where
  back-to-back `collect_all` pages hit the MarketData category bucket.)
- **A single-shot 20-symbol minute ingest.** One `ls-ingest` invocation over all 20 symbols
  tripped after roughly one symbol's worth of t8412 pages (~13 requests) once the budget was
  warm, losing all symbols after the trip. The SDK propagates IGW00201 as a hard error (no
  retry), so there is no partial-recovery within the run.
- **Trusting `catalog status: GO`.** GO is computed on deduped reads and was green with only
  005930 holding minute bars — it is not a per-symbol minute-completeness check.

## Solution

Drip-feed the minute ingest **one symbol at a time**, retrying on IGW00201 with a fixed
backoff. `range` mode is idempotent per symbol (an already-covered symbol `APPEND REFUSED`s
as a harmless no-op), so the loop is safe to re-run and resumes cleanly after each trip:

```bash
for s in $SYMBOLS; do
  for try in 1 2 3 4 5 6; do
    rm -f "$CAT/.ls-ingest.lock"                 # clear the stale lock a killed run leaves
    out=$(LS_TRADING_ENV=paper LS_INGEST_LANE_FILE="$LANE" \
      LS_INGEST_CATALOG="$CAT" LS_INGEST_SDATE=$SD LS_INGEST_EDATE=$ED \
      LS_INGEST_KIND=minute:1 LS_INGEST_SYMBOLS="$s" \
      ./target/debug/ls-ingest 2>&1)
    if echo "$out" | grep -q "IGW00201"; then sleep 120; continue; fi   # backoff + retry
    echo "$out" | grep -qE "ingest complete|APPEND REFUSED" && break     # done (or already had it)
    sleep 30
  done
  sleep 8
done
```

Then verify **per-symbol** minute coverage, not just the GO:

```bash
LS_DATA_HOME=./data/turn3 LS_STATUS_SDATE=$SD LS_STATUS_EDATE=$ED \
  ./target/debug/lab-research catalog status | grep -c "1-MINUTE:"   # must equal the symbol count
```

Practical notes from the turn-3 run (2026-07-07):

- One symbol ≈ 10,668 one-minute bars over ~28 sessions ≈ ~13 t8412 pages; the drip loop
  averaged ~31s/symbol and recovered cleanly after each 120s backoff.
- Size the minute window from the max-lookback probe first
  (`LS_INGEST_MODE=probe-lookback`, pilot 005930) — LS paper served minute bars from
  `20250711` (depth 361 days) as of 2026-07-07, so a recent ~30-session window has no
  front-truncation.
- Build/run the **debug** `ls-ingest` — a `--release` build repeatedly got killed mid-compile
  in the sandbox; the debug binary is fine for a bounded ingest.

## Why This Works

IGW00201 is a budget that refills over a rolling window. A single bulk run spends the whole
budget in one uninterrupted burst and dies partway; drip-feeding one symbol at a time with a
120s pause between trips keeps each burst under the remaining budget and lets the window refill
between symbols. Because `range`-mode ingest is per-symbol idempotent (already-covered symbols
`APPEND REFUSED` without re-fetching), the loop is restartable and never double-pulls — so
retrying the whole symbol list after a trip only fetches the missing symbols.

## Prevention

- **Never trust `catalog status: GO` as proof of minute completeness.** It reads deduped, so a
  daily-complete catalog with one minute symbol still reports GO. Assert
  `grep -c "1-MINUTE:" == <symbol count>` (and check each span covers the pinned range) before
  running a backtest that depends on breadth.
- **Bulk minute pulls are drip-fed, not single-shot.** Any ingest spanning many symbols ×
  many sessions should iterate per symbol with IGW00201 backoff — treat a mid-run IGW00201 as
  expected backpressure, not a fatal error.
- **Clear the stale lock.** A killed ingest leaves `<catalog>/.ls-ingest.lock`; the next run
  refuses until it is removed (`rm <catalog>/.ls-ingest.lock`) — the loop above does this each
  iteration.
- **Budget awareness across the day.** IGW00201 is shared across everything hitting the gateway
  on the credential; a prior attended live session the same day pre-consumes the window, so a
  later bulk ingest trips sooner. Schedule bulk pulls when the budget is cold, or expect more
  backoffs.

## Budget economy: the drip loop re-loads the whole universe on every invocation (update 2026-07-08)

The drip-feed above runs `ls-ingest` **once per symbol**, and each invocation opens with a
**universe load** — `provider.load_domain(DomesticEquity)` = **3 gateway calls** (`t8430` +
2× `t9945`) — *before* it fetches a single bar. The masters don't change minute-to-minute, so a
20-symbol minute drip burns ~60 redundant universe-load calls per pass (more with retries) against
the same IGW00201 budget it's trying to conserve. On a warm budget this is a large avoidable drain:
a deep max-depth attempt that whole-symbol-retried a stuck symbol ~8× re-loaded the universe each
time, pushing toward ~480 wasted calls.

The `Ingestor` never needs the loaded provider for an **explicit** `LS_INGEST_SYMBOLS` list — the
`InstrumentId`s are built directly from the shcodes; the load only feeds the idempotent
`write_instruments` re-snapshot, which needs to run **once**. So:

- **`LS_INGEST_SKIP_UNIVERSE_LOAD=1`** skips `load_domain` + `write_instruments` when
  `LS_INGEST_SYMBOLS` is explicit and the catalog is already populated (a prior full pass persisted
  the instrument defs). It refuses if the flag is set without an explicit symbol list, or on an
  empty/missing catalog (skipping `write_instruments` there would write bars with no instrument
  defs — a silent failure that only surfaces at backtest time).
- **Drip pattern:** run the **daily pass first, batched, WITHOUT the flag** (it loads the universe
  once and persists instruments), then set `LS_INGEST_SKIP_UNIVERSE_LOAD=1` on the per-symbol
  **minute** passes. `adapters/nautilus/scripts/turn4-ingest.sh` wires this (daily `skip=0`, minute
  `skip=1`).
- **Also mind the mid-symbol restart:** a symbol whose minute fetch trips IGW00201 mid-way used to
  abort and re-fetch from page 1 (re-burning the pages already fetched). `collect_minute` now backs
  off + narrows on IGW00201 and retains completed sub-ranges' bars, so a single invocation makes
  incremental progress instead of restarting — cutting the other big source of re-burn.

**Caveat this does NOT change:** the irreducible cost is the bar fetches themselves (~1 t8412 page
per ~2 sessions/symbol). No code change makes a genuinely deep multi-symbol pull fit one day's
budget — that stays multi-day. Skipping the universe load and the restart re-burn only removes the
*avoidable* waste, which can tip a moderate pull from "just over budget" to "fits one cold budget."
