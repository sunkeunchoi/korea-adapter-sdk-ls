---
title: "LS gateway t1444 (and body-idx rank boards) paginate via the tr_cont:Y header, not the body idx cursor alone"
date: 2026-07-08
category: integration-issues
module: "ls-sdk paginated boards (crates/ls-sdk/src/paginated/) + adapters/nautilus capture-universe"
problem_type: integration_issue
component: tooling
symptoms:
  - "market_cap_top / a t1444 page-walk returns the same top-~20 rows on every page — a top-N > 20 universe is unreachable"
  - "Echoing the returned outblock.idx back as the request idx does NOT advance the board: page 2 returns the identical rows and the identical non-terminal outblock.idx"
  - "The board reports a non-terminal cursor (outblock.idx = a non-zero offset) yet re-serves page 1, so a naive body-idx loop either caps at one page or spins on a repeat cursor"
root_cause: wrong_api
resolution_type: code_fix
tags:
  - ls-gateway
  - t1444
  - pagination
  - tr-cont
  - body-cursor
  - rank-boards
  - market-cap-top
related:
  - docs/solutions/integration-issues/ls-gateway-t8412-chart-all-pagination-burst-and-silent-truncation.md
  - docs/solutions/integration-issues/ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md
---

## Problem

`t1444` (시가총액상위 / KOSPI market-cap top) serves ~20 names per page. A caller that
walks its **body `idx` cursor** — echoing the returned `t1444OutBlock.idx` back as the
next request's `idx` — never gets past page 1: the gateway re-serves the same top-20.
So a top-N universe with N > ~20 (e.g. a top-40 sample) is unreachable, even though the
board obviously holds hundreds of names.

This contradicted the codebase's own `crates/ls-sdk/src/paginated/mod.rs` module doc,
which classified body-`idx` rank/screen TRs (`t1444`, `t1422`, `t1442`, `t1452`, `t1463`,
…) as **"single-page only … ls-core has no multi-page machinery."** They are not
single-page; the walk just needs the header the doc omitted.

## Symptoms

- Page 1 (`idx="0"`) → 20 rows + `t1444OutBlock.idx="20"` (a non-terminal cursor).
- Page 2 with `idx="20"` → the **identical 20 rows** + `t1444OutBlock.idx="20"` again.
- A body-idx loop therefore either stops at 20 (treating any returned cursor as terminal)
  or detects a `repeat`/`no_progress` cursor and stops at 20 anyway.

## What Didn't Work

- **Echoing the body `idx` cursor alone.** The gateway ignores it and re-serves page 1.
  This is the whole trap: the response advertises a next cursor it will not honor without
  the header.
- **`collect_all` (the generic header-only walk).** It threads the `tr_cont`/`tr_cont_key`
  HTTP headers but is not wired for these boards, and — as the `t8412` doc notes — the
  live gateway can terminate the header walk after page 1 while more in-range rows exist.
  The reliable walk threads **both** the body cursor and the header.

## Solution

Continuation requires **BOTH** the request-body cursor **AND** the `tr_cont: Y` request
header (plus `tr_cont_key` echoed from the previous response). This is the exact behavior
the `t8412` minute fetcher already documents (`adapters/nautilus/src/ingest/mod.rs`, the
`fetch_minute_chunk` walk): *"A continuation needs BOTH the body cursor and the `tr_cont: Y`
request header — the gateway re-serves the newest page when the header is absent, even with
the cts cursor threaded."*

Concretely (`market_cap_top_all` in `crates/ls-sdk/src/paginated/mod.rs`):

1. Expose the response continuation on the typed response: add `tr_cont` / `tr_cont_key`
   `#[serde(default)]` fields to `T1444Response` and `impl_has_pagination!` for it —
   `dispatch_once` injects the response HTTP headers into the deserialized JSON, so the
   getters read a real value.
2. Walk pages manually (NOT `collect_all`): on each continuation set
   `req.inblock.idx = resp.outblock.idx`, `req.set_tr_cont("Y")`, and
   `req.set_tr_cont_key(resp.tr_cont_key)`.
3. Terminate on `idx` empty/`"0"`, response `tr_cont == "N"`, a cursor that does not
   advance (`repeat`), or a page that adds no new shcodes (`no_progress`); on hitting the
   page cap with the cursor still live, surface `PaginationLimit` rather than silently
   returning a short list (the `t8412` silent-truncation lesson).

```rust
req.inblock.idx = next_idx;          // body cursor
req.set_tr_cont("Y".to_string());    // <-- the missing piece
req.set_tr_cont_key(next_key);
```

## Why This Works

The LS gateway treats a request with an empty/`"N"` `tr_cont` header as a **first-page**
request regardless of the body `idx` — so without `tr_cont: Y` it always answers with
page 1. Setting `tr_cont: Y` (and returning the paired `tr_cont_key`) is what tells the
gateway "continue the prior query," and only then does the body `idx` cursor select the
next slice. The response advertises a next cursor unconditionally, which is why the naive
loop looks like it should work.

## Prevention

- **Treat "single-page" claims about LS body-idx boards as unverified.** The
  `paginated/mod.rs` module doc's "single-page only" classification was wrong for `t1444`;
  the same is likely true for the sibling rank/screen boards (`t1422`, `t1442`, `t1452`,
  `t1463`, …). Before promoting any of them to multi-page, verify against the live gateway
  with `tr_cont: Y` set, not just the body cursor.
- **Diagnostic signature to recognize this fast:** page 2 (body cursor echoed, no header)
  returns byte-identical rows and the same non-terminal `outblock.idx`. That is the
  "header missing" fingerprint, not a genuinely single-page board.
- **Test the walk with a wiremock that gates page 2 on the `tr_cont: Y` header** (assert
  page 2 is reached only when the header is present), plus a stuck-cursor case (terminates,
  no hang) and a never-terminating case (surfaces `PaginationLimit`, no silent truncation).
- **Do not reach for `collect_all` for these boards** — its header-only walk is the path
  the gateway truncates. Mirror the `t8412` fetcher's body-cursor-plus-header loop instead.
