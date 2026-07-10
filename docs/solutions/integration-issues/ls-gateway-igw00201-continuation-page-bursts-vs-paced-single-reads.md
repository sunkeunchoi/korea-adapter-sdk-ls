---
title: "IGW00201 trips on continuation-page bursts and behind large responses while paced single-page reads always serve — pace per page, order big fetches last, and never record a transient throttle as paper-incompatibility"
date: 2026-07-11
category: integration-issues
module: "adapters/nautilus reference capture (src/reference/capture.rs, src/bin/capture-universe-metadata.rs) + crates/ls-sdk/src/paginated/mod.rs market_cap_top_all (t1444)"
problem_type: integration_issue
component: tooling
symptoms:
  - "SDK market_cap_top_all (t1444, ~10 continuation pages back-to-back for a 200-row board) returns rsp_cd=IGW00201 on every attempt — at 600ms and even 2000ms inter-CALL pacing, after a 10-minute cooldown, and after a 120s backoff-retry of the whole walk"
  - "Single-page raw probes and the typed make live-smoke-t1444 serve 200/00000 every time against the same credential — including 2 minutes after a failed multi-page walk"
  - "Reads dispatched immediately after the ~812KB t8430 master fetch throttle, even though the same reads serve when the master is fetched last"
  - "A throttled t1405 designation-category walk got recorded in artifact provenance as paper_incompatible, silently under-excluding ~120 halted symbols from the tradability gate"
root_cause: wrong_api
resolution_type: code_fix
severity: high
tags:
  - igw00201
  - rate-limit
  - ls-gateway
  - t1444
  - t8430
  - t1405
  - pagination
  - universe-metadata
  - fail-closed
related:
  - docs/solutions/integration-issues/ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md
  - docs/solutions/integration-issues/ls-gateway-t8412-chart-all-pagination-burst-and-silent-truncation.md
  - docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md
  - docs/solutions/integration-issues/ls-gateway-t1444-header-pagination-not-body-idx.md
---

# IGW00201 trips on continuation-page bursts and behind large responses while paced single-page reads always serve — pace per page, order big fetches last, and never record a transient throttle as paper-incompatibility

## Problem

The reference-data universe capture (PR #116, unmerged as of this writing;
`adapters/nautilus/src/reference/capture.rs`) needs a ~26-call live sweep over
six TRs: the ~812KB `t8430` master, `t2522`, `t1904`×2, two multi-page `t1444`
ranked cap boards, and eleven `t1405`/`t1404` designation-category walks. The
first implementation fetched the cap boards via the SDK's
`market_cap_top_all` (`crates/ls-sdk/src/paginated/mod.rs:495`), which walks
continuation pages in a tight `for _ in 0..MAX_PAGES` loop
(`mod.rs:500,508`) with **no inter-page sleep** — each page dispatch goes
straight back to the gateway as fast as the client-side category limiter
allows. Against the live paper gateway (2026-07-10 closed-window rehearsal,
3 iterations) that burst tripped `IGW00201` (호출 거래건수를 초과하였습니다)
on **every** attempt, while every paced single-page read on the same
credential served throughout.

This is a sharper characterization of the gateway's short-window budget than
the two prior IGW00201 docs. The t8412 doc
(`ls-gateway-t8412-chart-all-pagination-burst-and-silent-truncation.md`)
established that a `collect_all`-style continuation burst trips the throttle —
one prior instance, fixed by pacing that one fetcher. The drip-feed doc
(`ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md`) established that
IGW00201 is a large, warm-sensitive **cumulative** budget, not a pure rate.
This rehearsal adds three facts neither doc had, plus one artifact-integrity
rule:

1. **The single-page-vs-burst asymmetry, proven live side by side.** The same
   TR, same credential, same minute: a multi-page walk throttles on every
   attempt while a single-page read serves 200/00000 — including two minutes
   after a failed walk. The window punishes back-to-back *continuation*
   dispatch specifically, not the call count a paced sweep accumulates.
2. **Response SIZE weighs in the window.** The ~812KB `t8430` master fetch
   drains the same short-window budget and throttles the reads dispatched
   behind it. Call *count* is not the only spend axis; bytes served count too.
3. **Fetch ORDER is a free lever.** Reordering the capture so the big master
   runs LAST — with no other change — made the full ~26-call sweep serve end
   to end.
4. **A transient IGW00201 must never be recorded as paper-incompatibility in a
   hard-gate artifact.** One throttled `t1405` category walk was initially
   recorded in provenance as `paper_incompatible` — wrong on both counts: the
   throttle is transient (the same read serves minutes later), and the
   artifact's tradability gate then silently under-excludes ~120 halted
   symbols. Fail the capture closed and re-run instead.

## Symptoms

- `market_cap_top_all("001", 200)` (10 continuation pages for a 200-row t1444
  board) → `IGW00201` on every attempt: at 600ms inter-CALL pacing, at 2000ms
  inter-CALL pacing, after a 10-minute cooldown, and after a 120s
  backoff-retry of the whole walk. Note "inter-CALL": the pacing sat *between
  top-level fetches*, not between the walk's internal pages — the SDK loop has
  no seam to pace (`crates/ls-sdk/src/paginated/mod.rs:508` dispatches
  `post_paginated` back-to-back).
- Single-page raw probes (`make raw-probe LS_PROBE_TR_CD=t1444 ...`) and the
  typed `make live-smoke-t1444` served 200/00000 **every time**, including 2
  minutes after a failed walk — same credential, same window.
- With `t8430` fetched early, the reads dispatched right behind its ~812KB
  response throttled; with it fetched last, the identical read sequence
  served.
- The one throttled `t1405` category walk landed in
  `provenance.paper_incompatible` as `{tr: "t1405", code: "IGW00201"}`, and
  the tradability gate — which hard-excludes designated symbols — quietly
  passed ~120 halted symbols as tradable because their designation rows were
  never fetched.

## What Didn't Work

1. **Pacing between top-level calls while using `market_cap_top_all`.** 600ms
   and 2000ms inter-call sleeps both throttled, because the burst is *inside*
   the SDK walk: `market_cap_top_all` fires its ~10 continuation pages
   back-to-back regardless of how the caller paces around it. The client-side
   runtime limiter enforces only the MarketData *category* bucket; `t1444`'s
   own `rate_limit_per_sec: Some(2)`
   (`crates/ls-core/src/endpoint_policy/chart_reference.rs:175-186`) is
   metadata, not a limiter — the same metadata-only trap as t0425 and t8412.
2. **Cooldown + whole-walk retry.** A 10-minute cooldown and a 120s
   backoff-then-retry of the full walk both re-throttled — the retried walk
   re-burst its pages and re-tripped the window. Backoff without changing the
   dispatch shape just replays the failure.
3. **Treating IGW00201 as pure call-count spend.** The paced single-page reads
   totalling similar call counts always served; what tripped the window was
   burst shape and the ~812KB master's byte weight, not the count. (The
   drip-feed doc's probe already showed t8412 serving ≥600 paced calls cold.)
4. **Recording the throttled t1405 walk as `paper_incompatible` and carrying
   on.** The capture "succeeded" with a provenance failure entry, and the
   tradability gate silently under-excluded ~120 halted symbols.
   `paper_incompatible` is a *permanent capability* classification (see the
   §14 runtime distinction between `00707`/`01900`); IGW00201 is a
   *transient budget* state. Writing the latter into the former poisons a
   hard-gate artifact with a hole no consumer can see.

## Solution

Four coordinated changes in PR #116, all live-verified by the rehearsal's
final iteration (the full ~26-call capture served end to end):

**1. Self-paced per-page walk instead of the SDK burst walk.**
`walk_t1444` (`adapters/nautilus/src/reference/capture.rs:534-594`) replaces
`market_cap_top_all` for the capture: same termination protocol (body `idx`
cursor + `tr_cont: Y` header, terminal/repeat/no-progress cursor checks,
32-page cap), but it sleeps `pace` **between pages**
(`capture.rs:544-547`) and gives each throttled page one `backoff`-then-retry
(`capture.rs:551-560`) — the short-window budget refills in ~2 minutes, so a
single page retried after 120s serves where a replayed burst does not. At the
page cap with a live cursor it returns `"pagination_limit"` rather than
silently truncating (`capture.rs:583-589`), because a truncated board
mis-tiers every below-cutoff symbol into the exclusion stratum.

**2. Defaults that encode the rehearsal numbers.** `CaptureConfig::new`
pins `pace: Duration::from_millis(2000)` and
`throttle_backoff: Duration::from_secs(120)` (`capture.rs:158-161`), with the
comment trail explaining why 600ms is not enough behind the big master read.
Operator overrides: `LS_CAPTURE_PACE_MS` / `LS_CAPTURE_BACKOFF_MS`
(`adapters/nautilus/src/bin/capture-universe-metadata.rs:85-92`).

**3. The ~812KB t8430 master is fetched LAST.** The capture's fetch order is
t2522 → t1904×2 → t1444 boards → t1405/t1404 categories → t8430
(`capture.rs:414-421`), with the rationale in the comment: the master's
~800KB response drains the short-window budget and throttled the very next
reads; the join is in-memory, so fetch order is free.

**4. Transient throttle = fatal, never provenance.** If a cap board still
throttles after the per-page retry, the capture **aborts** with
`CaptureError` rather than recording a `TrFailure`
(`capture.rs:312-325`) — a missing board would mis-tier its whole market.
Same for every `t1405`/`t1404` designation category
(`capture.rs:364-372`, `capture.rs:400-408`): "a transient hole in the hard
tradability gate must not be baked into the artifact; re-run when the budget
is colder." Only genuinely non-transient failure codes reach
`provenance.paper_incompatible`.

Two budget-accounting complements (KTD6, shared attended window): the
category walks count attempt 1's real pages before retrying
(`capture.rs:344-351`), and `CaptureError` carries `calls_made` so the
`capture-universe-metadata` binary records spend into the shared ledger **on
both outcomes** (`capture.rs:177-188`;
`capture-universe-metadata.rs:118-134`) — a failed run spends real budget,
and dropping it would make the minute ingest's planner over-optimistic.

The SDK's `market_cap_top_all` itself is unchanged in PR #116 — it remains
correct for shallow pulls but is now known-unsafe for deep walks on a warm
window (see Prevention).

## Why This Works

The IGW00201 window is not a simple rolling call counter. The rehearsal's
asymmetry pins three properties:

- **It penalizes dispatch density, not just volume.** Ten pages fired
  back-to-back trip it; the same ten pages 2s apart do not. Sleeping between
  *pages* (the only place the burst exists) converts the walk into the shape
  the gateway always serves — pacing between top-level calls never touched
  the burst, which is why it couldn't work.
- **It weighs response bytes.** The ~812KB master is one call but drains the
  short window like many; anything dispatched into that hole throttles.
  Moving it last means nothing queues behind the drain — the window refills
  before the next capture would run, for free.
- **It refills fast (~2 minutes).** That is why a *single-page* 120s
  backoff-retry succeeds where a *whole-walk* 120s retry fails: the retried
  single page is one call into a refilled window; the retried walk is a fresh
  burst that re-drains it.

And the fail-closed rule works because it matches the semantics of the two
states: `paper_incompatible` asserts "this TR can never serve on paper" —
a permanent fact consumers may cache; IGW00201 asserts "not right now."
A capture abort is cheap (re-run on a colder window, spend already recorded
in the ledger); a hard-gate artifact with an invisible ~120-symbol hole in
its tradability filter is not.

## Prevention

- **Any multi-page continuation walk against the LS gateway must pace between
  pages, at the walk's own seam.** The client-side category limiter and
  inter-call pacing around the walk do not reach the burst. This is now the
  fourth documented instance of the metadata-only per-TR cap trap (t0425,
  t8412, the bulk-minute drip, now t1444) — assume every `has_pagination`
  TR's SDK `*_all` convenience walk is burst-unsafe on a warm window until
  its call sites pace per page, and prefer a self-paced walk (the
  `walk_t1444` shape) in anything that runs inside a shared attended window.
- **Order big-response fetches last (or isolate them).** Response size spends
  the same short window as call count. When a sweep mixes an ~800KB master
  with many small reads and the join is in-memory, fetch order is a free
  reliability lever — put the drain where nothing queues behind it.
- **Backoff-retry at page granularity, never walk granularity.** Retrying a
  whole burst replays the failure; retrying the one throttled page after
  `refill_secs` (120s, field-proven twice now) succeeds.
- **Never write a transient gateway state into a permanent artifact
  classification.** IGW00201 (and any other budget/throttle code) must map to
  abort-and-re-run, not to `paper_incompatible` or any other cached
  capability verdict — especially when the classification feeds a hard gate
  whose failure mode is silent under-exclusion. Grep for throttle codes in
  anything that populates provenance/failure fields.
- **Record spend on failure paths too.** A capture that dies mid-walk spent
  real budget; error types that cross the recording boundary should carry
  `calls_made` (the `CaptureError` pattern) so the shared ledger never
  under-counts.
