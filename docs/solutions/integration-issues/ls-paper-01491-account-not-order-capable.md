---
title: "LS Paper 01491 — order TRs need an order-enabled paper account (read/inquiry-only accounts reject all orders)"
date: 2026-06-25
last_updated: 2026-06-25
category: integration-issues
module: ls-core error classification / order Paper Live Smoke
problem_type: integration_issue
component: tooling
symptoms:
  - "Paper order TR (CSPAT00601/00701/00801) returns API error 01491: 모의투자 주문이 불가한 계좌입니다"
  - "order_smoke / live-smoke-order-chain records cert=pending with no order placed (production_not_run=true)"
  - "Inquiry/balance TRs (CSPAQ12200 etc.) on the same account succeed, but every order submit fails"
tags: [orders, paper-trading, account-provisioning, error-classification, cspat00601, order-smoke]
---

## Problem

The domestic-stock order TRs (`CSPAT00601` submit, `CSPAT00701` modify, `CSPAT00801`
cancel) are machinery-complete and merged (Order Waves 1 & 2, PRs #52/#53) but sit at
`implemented: false`. The flip to Implemented is gated on a clean in-window paper run of
`make live-smoke-order-chain`. Running it against the default paper account fails at the
first leg:

```
ORDER-SMOKE tr=CSPAT00601 scenario=submit_resting_buy cert=pending rsp_cd=
  order_no=- recon=- production_not_run=true
  msg=[submit failed: API error 01491: 모의투자 주문이 불가한 계좌입니다.]
```

`01491` = "this is an account on which simulated/paper orders are not possible." The
configured paper account is provisioned **read/inquiry-only** and is not enabled for
paper order placement — which is why the inquiry/balance TRs on the same credentials
flip fine while every order submit is rejected.

## Root cause

This is **not** a session-window problem and **not** an SDK defect: the request reached
the gateway and got a clean business rejection; nothing was placed. It is an LS
**account-provisioning** gate. Order placement requires a paper account explicitly
enabled for 모의투자 주문; not all paper accounts are.

`01491` is distinct from `01900` (모의투자에서는 해당업무가 제공되지 않습니다 — a service
that Paper *never* provides, per-service). `01491` is per-*account*: swapping in an
order-enabled paper account clears it, where `01900` cannot be cleared by any account.

## Fix / resolution

Two parts — one operational, one classification:

1. **Operational (clears the blocker):** provision/enable an LS paper account with paper
   order placement enabled, put its credentials in the gitignored `.env`, then re-run
   `make live-smoke-order-chain` during an open KRX regular session (09:00–15:30 KST,
   weekday). One clean chained run flips all four TRs (gate 1: `CSPAT00601` + `t0425`;
   gate 2: `CSPAT00701` + `CSPAT00801`).

2. **Classification (names the blocker precisely next time):** `01491` is now a
   first-class runtime signal, mirroring the `01900` pattern:
   - `ls_core::PAPER_ORDER_INCAPABLE_CODE` (`"01491"`) +
     `ls_core::is_paper_order_incapable(code)` + `LsError::is_paper_order_incapable()`
     (`crates/ls-core/src/inner.rs`, `error.rs`, `lib.rs`).
   - The order smoke harness (`crates/ls-sdk/tests/order_smoke.rs`) records `01491` as
     **Pending** (not `Certified`) in both the matrix and chained paths, and names the
     actual code in the recorded reason. Before this fix the chained path only special-
     cased `01900` (so `01491` fell through to a generic "submit failed" message), and
     the matrix path mis-classified any non-`01900` broker code — including `01491` — as
     `Certified`, which would have falsely counted an account-incapable rejection as
     order-capability evidence.

## How to detect / verify

- `make raw-probe LS_PROBE_TR_CD=CSPAT00601 LS_PROBE_PATH=/stock/order ...` against the
  account returns `rsp_cd=01491` → the account is not order-capable; no amount of
  in-window timing changes this.
- Inquiry TRs succeeding while every order TR returns `01491` is the signature.

## Related

- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md` —
  another gateway business-code that looked environmental but was deterministic.
- `metadata/PROVISIONALITY-LEDGER.md` §12 — the `01900` night-session blockers
  (CCENQ90200/10100): same "machinery-complete, evidence-blocked externally" shape, for
  a per-service rather than per-account reason.
