---
title: "Never re-check `rsp_cd` in a consumer — dispatch already classified it, so `== \"00000\"` can only reject responses the SDK accepted"
date: 2026-07-27
category: conventions
module: "crates/ls-core (inner.rs: rsp_cd_is_success, dispatch_once), any adapter/lab consumer of an ls-sdk facade call"
problem_type: convention
severity: medium
applies_when:
  - "Writing a new consumer of an ls-sdk facade method and considering a defensive rsp_cd check"
  - "A consumer aborts with 'returned rsp_cd 00136' / '00707' / a blank code"
  - "Reviewing code that branches on a response's rsp_cd field"
related_components:
  - ls-core
  - ls-sdk
  - nautilus-ls-lab
tags:
  - rsp-cd
  - dispatch
  - success-classification
  - defensive-programming
  - false-abort
---

## Context

Writing a new gateway consumer, a defensive `if resp.rsp_cd != "00000" { bail!(...) }` looks
like cheap insurance. It is the opposite: it is unreachable for real errors and can only fire on
successes.

`ls_core::Inner::post` → `dispatch_once` classifies `rsp_cd` **before** the typed response is
handed back. Anything outside the success set is already returned as `LsError::ApiError` and
consumed by the caller's `?`. The success predicate is broader than one code
(`crates/ls-core/src/inner.rs`):

```rust
fn rsp_cd_is_success(code: &str) -> bool {
    if code.is_empty() || code == "00000" { return true; }
    matches!(code, "00136" | "00707")
}
```

`00136` is 조회가 완료되었습니다 (inquiry completed — **with** data) and `00707` is
조회할 내역이 없습니다 (completed, empty result). Both are successes: the gateway processed the
query and the response carries valid, possibly-empty blocks. An absent field is a success too —
response structs carry `#[serde(default)]`, so an envelope that omits `rsp_cd` yields `""`.

## Guidance

**Do not re-test `rsp_cd` in a consumer.** By the time you hold an `Ok(resp)`, the code is
already one of `""`, `00000`, `00136`, `00707`. A `!= "00000"` guard therefore rejects three
values the SDK deliberately accepted, and can never catch a genuine gateway error.

If a consumer genuinely needs to distinguish "completed with rows" from "completed empty",
branch on the **data** (`resp.outblock1.is_empty()`), not on the code.

The one real exception is orders, which have their own predicate and must never reuse the read
one — `00000`/empty are the gateway's *generic* success codes and cannot prove the exchange
accepted an order.

## Why This Matters

The failure mode is a **false abort in the worst possible window**. A consumer running in an
attended pre-session slot bails on a response that was fine:

```
t8407 batch 2/3 returned rsp_cd 00136 — the live today_open fetch cannot be completed
```

No artifact is produced, and the operator spends their one window debugging a non-error. The
blank-code variant is worse still: the message renders as `returned rsp_cd ` with nothing after
it, giving the reader no thread to pull.

Because the guard is unreachable for real errors, it has **zero** detection value to trade off
against that risk. It is pure downside.

## When to Apply

- Any new consumer of an `ls-sdk` facade method — read TRs especially
- Reviewing a diff that adds a `rsp_cd` comparison outside `ls-core`
- Diagnosing an abort whose message quotes a code that is in the success set

## Examples

```rust
// WRONG — unreachable for real errors; fires only on 00136 / 00707 / "".
let resp = sdk.market_session().multi_symbol_current_price(&req).await?;
if resp.rsp_cd != "00000" {
    anyhow::bail!("... returned rsp_cd {} ...", resp.rsp_cd);
}

// RIGHT — dispatch already classified it; branch on data if emptiness matters.
// No rsp_cd re-check: `Inner::post` returns LsError::ApiError for anything outside
// the documented read-success set, so the only codes reaching here are successes.
let resp = sdk.market_session().multi_symbol_current_price(&req).await?;
for row in &resp.outblock1 { /* ... */ }
```

Found in review of the `lab-mount-universe` live `today_open` path (2026-07-27), where the guard
would have aborted the producer on an informational `00136` — five independent reviewers flagged
the same line.

## Related

- [`ls-gateway-igw40011-numeric-request-fields`](../integration-issues/ls-gateway-igw40011-numeric-request-fields.md)
  — the request-side counterpart: numeric request fields must serialize as JSON numbers
- [`ls-paper-01491-account-not-order-capable`](../integration-issues/ls-paper-01491-account-not-order-capable.md)
  — an order-side code, classified by the separate order predicate
