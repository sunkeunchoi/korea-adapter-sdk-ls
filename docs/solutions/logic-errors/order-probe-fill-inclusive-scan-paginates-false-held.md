---
title: "Order negative-probe gap-(b) fill-inclusive chegb=0 scan paginates on a traded-history symbol → false-HELD"
date: 2026-07-06
category: logic-errors
module: ls-sdk negative_probe (run_order_negative_probe / scan_symbol_working_orders)
problem_type: logic_error
tags: [negative-probe, t0425, pagination, chegb, flat-verify, order-probe, re-cert, fail-closed]
---

## Problem

The re-cert wave (plan 2026-07-06-001) closed order-probe gap (b) — "flat-verify must not
be blind to a filled order" — by widening the shared working-orders scan in
`run_order_negative_probe` from `chegb="2"` (unfilled-only) to `chegb="0"` (all states, so a
fully-filled `ordrem==0` row surfaces and `flat_verdict` can return `Fill`). Offline twins
pass. But **live, every order negative probe HELD before placing anything**:

```
NEG-PROBE target=CSPAT00601-negative HELD: pre-assert-flat scan failed
[traded-symbol t0425 working-order scan is paginated (tr_cont=0) — a single page cannot
positively confirm flat] — no placement, no variants
```

The book was genuinely flat — the `live-smoke-order-chain` control run the same session
confirmed `flat=confirmed … [zero live rows]` with its own `chegb="2"` scan. Only the
fill-inclusive `chegb="0"` scan fail-closed.

## Root cause

`chegb="0"` returns the symbol's **entire accumulated order history** (all filled and
canceled rows across the account's life for that symbol), not just the small currently-working
set. On a heavily-tested paper account, 005930's history is large enough that the gateway sets
a continuation (`tr_cont`), and `scan_symbol_working_orders`'s single-page guard correctly
treats any non-empty non-`N` `tr_cont` as "cannot positively confirm flat" → `Err` → HELD.

Confirmed by a credential-safe `make raw-probe LS_PROBE_TR_CD=t0425` A/B on the same symbol:

| `chegb` | meaning | `body_len` |
|---------|---------|-----------|
| `"2"`   | unfilled-only (resting + partial fills) | **63** (empty/flat) |
| `"0"`   | all states (incl. fully-filled history) | **1186** (~19×, paginates) |

This is the exact trap `ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md` warned about:
`chegb="0"` "returns the account's entire order history" and overran the page cap; `chegb="2"`
was chosen precisely to keep the working set to a single page.

## The tension (unresolved)

Gap (b) is defense-in-depth (the primary order-safety is a non-marketable band-floor control +
type/required-only variants that cannot reprice it to a marketable level). Making the fill
*visible* needs `chegb="0"`; keeping the flat scan *single-page* needs `chegb="2"`. The two
requirements conflict on a traded-history symbol, and paginating the `chegb="0"` scan
(`collect_all`) is the non-terminating-cursor trap the sibling doc warns against.

## Disposition / prevention

> **FIXED** — plan `2026-07-06-002` (re-cert wave 2). The flatness scan is reverted to
> single-page `chegb="2"`; fill detection is decoupled into a bounded post-cancel ordno-scoped
> check (`classify_control_disposition`) that reads the cancel response rather than a whole-symbol
> all-states history walk, and teardown surfaces the accepted WAVE-BLOCKED OrdNo into an owned set
> so it cancels owned rows only (closing the foreign-cancel residual below) with a
> cancel-every-resting-row fallback when the owned set is incomplete. The `Fill`-visibility
> reduction is an accepted, documented residual (the non-marketable band-floor control + the
> WAVE-BLOCKED tripwire carry fill-safety). See `crates/ls-sdk/tests/negative_probe.rs`.

- The re-cert wave (§26) left `CSPAT00601/00701/00801` **HELD** (fail-closed, no order placed) — the
  order-chain control certified the happy path (submit `00040` / modify `00462` / cancel
  `00463`), but the required differential probe could not certify. Ledger §26. Reopened and
  fixed offline by plan 2026-07-06-002; live promotion is the attended §27 tail.
- A correct fix must decouple **fill detection** from the **single-page flatness scan**. Options
  for follow-up (not attempted mid-wave — never patch a live-order path hastily):
  - Detect the control's fill by an **ordno-targeted** lookup (the control's exact `OrgOrdNo`),
    not a whole-symbol all-states scan — bounded, no pagination.
  - Keep `chegb="2"` for flatness and add a separate bounded fill check only for the control's
    ordno.
- Do NOT re-widen a shared symbol-scoped flat scan to `chegb="0"` without a pagination story —
  the single-page guard will fail-closed on any actively-traded symbol.

## Related follow-up surfaced in the same review (teardown ownership)

The unconditional teardown (cancel *every* resting `005930` row, no ownership set — KTD3) was
chosen because `fire_inblock` returns only `(http, rsp_cd)` and drops the response body, so an
accepted WAVE-BLOCKED submit variant's OrdNo is never surfaced and an owned-only teardown would
strand it. The accepted residual is that a **foreign** order arriving mid-probe (between
pre-assert-flat and teardown) would also be canceled. When the order probe is reworked (above),
surface the accepted variant's OrdNo by parsing it out of the `fire_inblock` body and maintain
an owned set — that **simultaneously** cancels the un-surfaced WAVE-BLOCKED order *and* stops the
unconditional teardown from canceling foreign rows, closing both gaps at once. This is only worth
doing together with the flat-scan rework, since the order probe currently HELDs at pre-assert-flat
before any control is placed.
