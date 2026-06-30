---
title: "LS gateway t0425 — IGW00201 rate throttle + non-terminating pagination defeat an account-wide flat scan"
date: 2026-06-30
category: integration-issues
module: ls-sdk order-chain smoke / t0425 reconciliation read
problem_type: integration_issue
component: tooling
symptoms:
  - "Order-chain smoke hard-fails on the post-run flat assertion with rsp_cd=IGW00201 (호출 거래건수를 초과하였습니다) even though submit/modify/cancel all certified"
  - "Reconciliation reads return recon=unknown (the t0425 collect_all is throttled mid-walk)"
  - "After fixing the throttle, the flat scan instead fails with 'pagination limit reached (100 pages)'"
  - "A single-symbol t0425 inquiry returns fast in one page, but the account-wide (expcode empty) collect_all never terminates"
root_cause: wrong_api
resolution_type: code_fix
severity: high
related_components:
  - tooling
tags: [ls-gateway, igw00201, t0425, rate-limit, pagination, collect-all, order-smoke, flat-assert, market-data-bucket]
---

# LS gateway t0425 — IGW00201 rate throttle + non-terminating pagination defeat an account-wide flat scan

## Problem

The autonomous order-chain smoke (`make live-smoke-order-chain`) places a real paper
submit→modify→cancel lifecycle, then runs a post-run **flat assertion** — an account-wide
`t0425` (주식체결/미체결) scan that must positively confirm no order is left resting before
the run can pass. On a heavily-used paper account the flat assertion could never complete,
so the smoke hard-failed even though every order leg certified cleanly (`00040`/`00462`/`00463`).

## Symptoms

- `ORDER-CHAIN HARD-FAIL kind=flat-scan-failed detail=[... (API error IGW00201: 호출 거래건수를 초과하였습니다.) ...]`
- `recon=unknown` on every leg (the reconcile's own `t0425` read is throttled too).
- After pacing to avoid the throttle: `ORDER-CHAIN HARD-FAIL kind=flat-scan-failed detail=[... pagination limit reached (100 pages) ...]`.

## What Didn't Work

- **Waiting between runs** (90s) — the `t0425` throttle is a *within-run burst*, not cross-run
  accumulation; the very first `t0425` call of each run still failed.
- **Per-call `tokio::time::sleep` sprinkles** before each `reconcile`/`dump`/scan — helped the
  single dump call but not the *paginated* scan: `collect_all` fires per-page `post_paginated`
  calls internally that a pre-call sleep can't reach.
- **Lowering the client `market_data_per_sec` rate to 1/s** (via `config.rate_limits`) — this
  made it WORSE: at 1/s the client never trips the 2/s gateway cap, so `collect_all` happily
  walks the pathological cursor slowly to its 100-page cap → 7-minute timeout per run.
- **Narrowing the scan to the traded symbol but keeping `chegb="0"` (all states) + `collect_all`** —
  still hit the 100-page cap: a single symbol's full filled/cancelled history is itself huge.
- **`chegb="2"` (unfilled) + `collect_all`** — still hit 100 pages: the `cts_ordno` cursor does
  not cleanly terminate for this query, so `collect_all` walks to its cap even when the working
  set is one row.

## Root Cause

Two independent LS-gateway behaviors compounded:

1. **Rate-bucket mismatch.** `t0425`'s gateway cap is `rate_limit_per_sec: 2`, but the SDK's
   client-side limiter is **per-category** (MarketData / Orders / Account / Auth), and
   `T0425_POLICY.category = MarketData` (default **5/s**). The per-TR `rate_limit_per_sec: 2` is
   metadata only — *not enforced client-side*. So a burst of `t0425` reads sails past the client
   limiter at 5/s and the gateway throttles everything after the 2nd call with `IGW00201`. (The
   order TRs are in a *different* gateway rate-source-group, so order placement is unaffected.)

2. **Non-terminating pagination.** On a heavily-used paper account, the account-wide `t0425`
   query (`expcode=""`, `chegb="0"`) returns the account's entire order history and the
   `cts_ordno` continuation cursor does not cleanly signal "done" — `collect_all` walks to its
   `max_pages` (100) cap and returns an error, which the flat assertion (correctly,
   positive-confirmation-only) treats as NOT flat.

A single-page `inquiry()` (one `post_paginated`, no `collect_all`) with `chegb="2"` returns only
the currently-working orders (≈0–1 rows on a flat account) and was the key observation: the few
**working** orders fit one page; it is the **filled history** + the cursor pathology that blow up
`collect_all`.

## Solution

Bound the smoke's flat-scan teardown (test-harness only — the production `reconcile`'s own
`collect_all` is unchanged):

```rust
// crates/ls-sdk/tests/order_smoke.rs — scan_symbol_working_orders(sdk, symbol)
let req = T0425Request { inblock: T0425InBlock {
    expcode: symbol.into(), // traded symbol only — the chain places on exactly one
    chegb:   "2".into(),    // UNFILLED only — working orders are few; fills excluded (see limits)
    medosu:  "0".into(), sortgb: "2".into(), cts_ordno: " ".into(),
}, ..Default };
tokio::time::sleep(Duration::from_millis(1500)).await; // refill the 2/s t0425 budget before THE decisive read
match sdk.orders().inquiry(&req).await {            // SINGLE page — not collect_all
    Ok(resp) => {
        let cont = resp.tr_cont().trim();           // fail CLOSED on truncation
        if !cont.is_empty() && !cont.eq_ignore_ascii_case("N") {
            return Err("working-order scan is paginated — a single page cannot confirm flat".into());
        }
        Ok(resp.outblock1)
    }
    Err(e) => Err(format!("... {}", scrub_secrets(&e.to_string()))),
}
```

And **do not** pace the reconciles: let their `collect_all` fail-fast to `Unknown` (informational)
under the default 5/s bucket, and pace only the one decisive flat-scan call.

### Accepted, bounded limitations (documented in the code)

- `chegb="2"` returns still-resting orders AND partial fills (both carry `ordrem>0`), so
  `flat_verdict` still flags them. It excludes a **fully-filled** order (`ordrem==0`) — safe here
  because the chain places only **non-marketable band-floor/ceiling** orders that cannot fill, and
  the fill-prone matrix scenario tears down via paper reset, not this scan. A fill-capable caller
  must NOT reuse this helper.
- Symbol-scoping drops other-symbol leftover detection — safe because the chain trades exactly one
  symbol; the account-wide `chegb="0"` form is the very query that overran the page cap.

## Why This Works

- The throttle is avoided because only **one** decisive `t0425` call needs a clean budget; a 1500ms
  pre-pause refills the 2/s bucket after the preceding bursts.
- The pagination blowup is avoided because `chegb="2"` returns only the small working set, which
  fits a single page — and `inquiry()` reads exactly one page instead of walking the pathological
  cursor.
- Safety is preserved via **positive confirmation only**: a throttled read → `Err` → NOT flat
  (hard-fail), and a `tr_cont` continuation signal → `Err` → NOT flat. Flatness is never concluded
  from a failed or truncated read.

## Prevention

- **The per-TR `rate_limit_per_sec` is metadata, not a client-side limiter.** The runtime limiter
  is per-*category* (MarketData 5/s / Orders 3/s / Account 1/s / Auth 1/s). When a single TR's
  gateway cap is tighter than its category bucket (here `t0425`: 2/s gateway vs 5/s MarketData),
  bursts will trip `IGW00201` — pace the *call site*, do not assume the SDK paces per-TR.
- **`IGW00201` (호출 거래건수 초과) is a self-inflicted throttle** — pace it; it is never a TR defect
  (see also `ls-gateway-igw40011-numeric-request-fields.md`).
- **`collect_all` over a polluted/large account history is a trap for paper accounts** — the
  `cts_ordno` cursor may not terminate. For a safety scan that only needs *currently-working*
  orders, prefer a single-page `inquiry()` with a server-side state filter (`chegb="2"`) and fail
  closed on the `tr_cont` continuation header, rather than exhaustively paginating.
- **Lowering the client rate to dodge a throttle can make pagination worse** — a slower client rate
  lets `collect_all` walk a non-terminating cursor all the way to `max_pages` without ever tripping
  the gateway that would have stopped it.
- **Source autonomous order evidence from `live-smoke-order-chain`, not the `live-smoke-order`
  matrix** — the matrix's marketable scenario fills on an open market and leaves a position needing
  an out-of-band paper reset; the chain places only non-marketable orders and leaves the account
  confirmed flat.
