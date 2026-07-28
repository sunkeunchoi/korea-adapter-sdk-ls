---
title: "t8412 chart_all pagination is doubly broken — per-TR rate cap burst plus tr_cont header continuation silently truncates multi-page windows"
date: 2026-07-04
last_updated: 2026-07-28
category: integration-issues
module: "adapters/nautilus ingest (SdkFetcher::fetch_minute_chunk, src/ingest/mod.rs) + crates/ls-sdk/src/paginated/chart.rs (t8412 chart_all)"
problem_type: integration_issue
component: tooling
symptoms:
  - "collect_all fires t8412 continuation pages back-to-back and hits the per-TR gateway cap, returning rsp_cd=IGW00201, even though the SDK runtime limiter only enforces the MarketData category bucket (5/s) and never t8412's own 1/s EndpointPolicy.rate_limit_per_sec, which is metadata-only and unenforced"
  - "chart_all walks the tr_cont HTTP response header for continuation, but the live gateway terminates tr_cont after page 1 while in-range rows remain, so any window wider than one page (~2.3 trading days of 1-minute bars at qrycnt=900) is silently truncated to its newest page"
  - "The ingest checkpoint marks the triple done with 0 gaps recorded even though only the newest page was ever fetched — a live 2-symbol 12-day backfill returned 1024 bars (truncated, 0 gaps reported) instead of the expected 9168"
  - "crates/ls-sdk/src/paginated/chart.rs module docs still document header-based (tr_cont) continuation as correct for t8412, contradicting the working body cts_date/cts_time cursor mechanism"
root_cause: wrong_api
resolution_type: code_fix
severity: high
related_components:
  - "tooling"
tags:
  - "ls-gateway"
  - "t8412"
  - "chart-pagination"
  - "igw00201"
  - "rate-limit"
  - "silent-truncation"
  - "nautilus-ingest"
  - "tr-continuation"
---

# t8412 chart_all pagination is doubly broken — per-TR rate cap burst plus tr_cont header continuation silently truncates multi-page windows

## Problem

`t8412` (minute chart) historical-bar ingestion in `adapters/nautilus`
(`SdkFetcher::fetch_minute_chunk`, `adapters/nautilus/src/ingest/mod.rs`) drove
multi-page pulls through the SDK's generic `chart_all` delegation, which itself
sits on `ls-core`'s `Inner::collect_all` (`crates/ls-core/src/inner.rs`, the
`for _ in 0..max { ... }` loop starting at line 641). Against the real paper
gateway this was doubly broken, and both defects were confirmed live (PR #95;
the confirming commits were squashed into it):

- **Defect A (burst → throttle).** `collect_all` fires each continuation page
  back-to-back with no pacing of its own — it only calls `f(req.clone())` in a
  tight loop. The runtime rate limiter is per-*category* (`MarketData` = 5/s),
  but `t8412`'s own gateway cap is 1/s. `EndpointPolicy.rate_limit_per_sec` is
  metadata only; nothing at the `collect_all` call site enforces the *per-TR*
  cap. Page 2 of any multi-page minute window tripped `IGW00201`
  ("호출 거래건수를 초과하였습니다"). This is the third occurrence of the same
  trap documented for `t0425`
  (`docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md`):
  *"The per-TR `rate_limit_per_sec` is metadata, not a client-side limiter... a
  single TR's gateway cap tighter than its category bucket will trip
  `IGW00201`."*

- **Defect B (silent truncation).** `chart_all` (via `collect_all`) walks the
  `tr_cont`/`tr_cont_key` **HTTP response headers** to decide whether to fetch
  another page — this is exactly what `crates/ls-sdk/src/paginated/chart.rs`
  documents as the loop driver for `T8412Request`/`T8412Response` (module doc,
  lines 1-26: *"the gateway signals 'more rows available' via the
  `tr_cont`/`tr_cont_key` HTTP response headers, and the caller walks pages
  until that header goes empty"*). Live, the gateway terminates that header
  continuation after page 1 even though more in-range candle rows exist. The
  minute range silently truncated to its newest ~2.3 trading days, **and**
  `collect_all`'s normal-termination path (`cont.is_empty()`) made the
  adapter's checkpoint mark the triple `done` with 0 gaps — indistinguishable
  from a genuinely complete backfill.

## Symptoms

- A 12-day, 2-symbol minute backfill produced only 1024 bars (the newest page
  or two), not the expected multi-thousand-bar span.
- No error surfaced for Defect B — the run completed, the checkpoint recorded
  `is_done` with zero coverage gaps, and nothing in the adapter's output
  distinguished it from a correct backfill.
- Defect A, once the burst was fixed by pacing the dispatch, produced a
  *different* symptom: `IGW00201` was gone, but the range
  was still truncated — proving the header-continuation walk (Defect B) was an
  independent bug, not a byproduct of the throttle.

## What Didn't Work

1. **`chart_page` page-by-page carrying only the `tr_cont`/`tr_cont_key`
   HEADER continuation, with a pacer `acquire()` per dispatch.** This fixed
   the burst (Defect A: one paced dispatch per page instead of `collect_all`'s
   back-to-back loop) but *still truncated* — live, 1024 bars, the minute span
   stuck at the newest page. The gateway ends header continuation after page 1
   regardless of how the caller paces it; header continuation is simply not
   the real driver for `t8412`.
2. **Threading the body `cts_date`/`cts_time` cursor WITHOUT the `tr_cont: Y`
   request header.** The gateway re-served the newest page every time
   (detected because the parquet catalog's covered span stopped growing across
   requests that produced 2024 "new" bars — duplicates over the same range,
   not an extension of it).

## Solution

Drive the continuation with **both** the body `cts_date`/`cts_time` cursor
*and* the `tr_cont: Y` request header together, one paced dispatch per page —
mirroring the daily `t8410` walk in `collect_daily`, whose **cursor mechanics**
are the correct exemplar. (Scope note, 2026-07-28: that walk's *window* trust
was later found broken — the gateway ignores `sdate` on degenerate single-day
windows, fixed by a parsed-timestamp trim in PR #228; see
[`ls-gateway-t8410-single-day-window-ignores-sdate-append-refused`](ls-gateway-t8410-single-day-window-ignores-sdate-append-refused.md).
"Already-correct" here means the pagination shape, not window handling.) The
final shape is `SdkFetcher::fetch_minute_chunk` in
`adapters/nautilus/src/ingest/mod.rs` (lines ~320-386):

```rust
// adapters/nautilus/src/ingest/mod.rs
for _ in 0..MINUTE_MAX_PAGES {
    self.minute_pacer.acquire().await;
    let page = self.sdk.paginated().chart_page(&req).await?;
    let next_date = page.outblock.cts_date.trim().to_string();
    let next_time = page.outblock.cts_time.trim().to_string();
    let next_key = page.tr_cont_key().to_string();
    let empty_rows = page.outblock1.is_empty();
    if next_date.is_empty() {
        // A genuinely exhausted cursor is the ONLY clean completion.
        pages.push(page);
        return Ok(pages);
    }
    if empty_rows || !seen.insert((next_date.clone(), next_time.clone())) {
        // Suspect partial, fail closed: empty page with a live cursor, or a
        // re-served (echoed) page. Never pushed, never reported clean.
        return Err(AdapterError::Sdk(LsError::PaginationLimit(MINUTE_MAX_PAGES)));
    }
    pages.push(page);
    // BOTH the body cursor AND the `tr_cont: Y` request header are required —
    // live, the gateway re-serves the newest page when the header is absent,
    // even with the cts cursor threaded.
    req.inblock.cts_date = next_date;
    req.inblock.cts_time = next_time;
    req.set_tr_cont("Y".to_string());
    req.set_tr_cont_key(next_key);
}
```

The fail-closed arms matter as much as the happy path: an exhausted cursor
(`next_date.is_empty()`) is the *only* clean completion; a zero-row page with a
live cursor, or a cursor echo (the `seen` `HashSet` catches a repeated
`(date, time)` pair), returns `LsError::PaginationLimit` instead of `Ok`. That
routes the caller (`collect_minute`) into its existing split-and-requeue path,
which narrows the range and — if it still can't complete a single day —
records a `PaperThin` coverage gap rather than silently marking the range
done. The echoed duplicate page's rows are never pushed into the result.

Live verification (during PR #95, pre-squash): 1024 → 9168 bars over the same
2-symbol, 12-day range, 0 gaps. Re-verified after the fail-closed rework in
the same PR to confirm the new fail-closed arms don't fire on normal
termination — same 9168 bars / 0 gaps.

The regression test itself needed hardening in the same PR: the original
`minute_chunk_drives_continuation_page_by_page` test in
`adapters/nautilus/tests/ingest.rs` (line 1211) couldn't fail for the
regression it was meant to guard — a broken drive that re-serves page 1
produces 2 duplicate bars from 2 dispatches, which satisfies a naive count
assertion. It now keys page 2 on the full cursor (date **and** time) plus the
`tr_cont` header and asserts distinct bar *content*. Two new tests pin the
fail-closed paths: `minute_empty_page_with_live_cursor_fails_closed_as_gap`
(line 1282) and `minute_cursor_echo_drops_duplicate_page_and_fails_closed`
(line 1343).

## Why This Works

LS chart TRs self-paginate on the **body** cursor — `t8410`'s daily walk
already relies on this (`collect_daily` threads `cts_date` alone, no header,
and it works). `t8412`'s module doc in
`crates/ls-sdk/src/paginated/chart.rs` (lines 15-26) documents the header walk
as the continuation mechanism and explicitly distinguishes it from the
`cts_date`/`cts_time` body fields as "two unrelated continuation mechanisms" —
but that documentation is wrong for live multi-page behavior: the header
signal terminates early while the body cursor is what the gateway actually
keeps advancing. Sending `tr_cont: Y` in the request without a real body-cursor
change is also insufficient on its own — the gateway needs the body cursor to
know *which* page to serve, and the header to know the caller intends to
continue at all. Only supplying both together produces distinct, forward-moving
pages.

## Prevention

- **Per-TR gateway caps must be enforced at the dispatch call site** (a pacer
  `acquire()` per page dispatch), never assumed to be covered by the
  per-category runtime limiter. This is the third documented occurrence of the
  same class (see the `t0425` doc above) — treat "per-TR cap tighter than its
  category bucket" as a standing risk for every paginated TR, not a one-off.
- **Never trust a "done, 0 gaps" completion signal that hasn't been
  live-verified end to end.** `collect_all`'s header-empty termination looked
  identical to a real completion; only comparing live bar counts before/after
  (1024 vs 9168) exposed the truncation.
- **Regression tests for pagination must assert bar *content*, not counts.** A
  broken drive that re-serves page 1 can satisfy a count-based assertion while
  silently returning duplicates — key the mock's page-2 response on the full
  cursor tuple and assert the returned bars are distinct.
- **SDK follow-up (RESOLVED — PR #142, commit `899497b`):** `chart_all` was
  ported off the generic header-driven `collect_all` onto a hand-rolled body
  `cts_date`/`cts_time` cursor loop (mirroring this adapter's
  `fetch_minute_chunk`): it completes on an empty `cts_date`, threads
  `tr_cont: Y` + `tr_cont_key` per continuation, and fails closed to
  `PaginationLimit` on a repeated cursor / zero-row live-cursor page / max_pages.
  `chart_all` is now the safe multi-page primitive — direct SDK callers no longer
  have to hand-drive `chart_page`. The new offline tests in
  `crates/ls-sdk/tests/paginated/chart.rs` follow this doc's own prevention
  guidance ("assert bar *content*, not counts — key the mock on the full cursor
  tuple"): they pin body-cursor termination (stop-on-empty-despite-`tr_cont:Y`,
  continue-past-early-`tr_cont:N`, header+cursor threading) and the three
  fail-closed paths.
  - **Pacing caveat still stands:** `chart_all` fires its continuation pages
    back-to-back with no pacing of its own (Defect A above), so it remains
    exposed to the 1/s per-TR `IGW00201` cap on bulk pulls. The adapter's paced
    `fetch_minute_chunk` (a pacer `acquire()` per page) is still the
    IGW00201-safe path for large backfills, and any direct SDK caller pulling
    more than a couple of pages must self-pace.
