---
title: "Account-state capacity/deposit reads smoke a deserializable ALL-DEFAULT row (not empty 00707) under closure on a cash-only paper account — they are session/funding-dependent, not persistent"
date: 2026-06-28
category: conventions
module: ls-sdk account owner_class, implement-tr recipe, closed-window flip waves
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Scoping a closed-window flip wave over the account lane (account_state reads: balance, deposit, margin order-qty, open-interest)"
  - "A make live-smoke-<tr> returns rsp_cd=00136 or 00000 with a deserializable 1-row out-block whose modeled numeric fields are ALL zero/empty"
  - "Deciding whether an account capacity/deposit read flips Implemented or defers PENDING under the R5 non-default-witness gate"
  - "Judging whether the paper account holds securities / F/O positions before queuing positions-dependent reads (the U2 holdings gate)"
tags:
  - paper-live-smoke
  - closed-window
  - account-state
  - implement-tr
  - flip-wave
  - r5-witness-gate
related_components:
  - tooling
---

# Closed-window account capacity/deposit reads smoke all-default, not empty

## Context

A closed-window flip wave certifies a Tracked TR by smoking it under KRX closure
and flipping it Implemented only when the paper response **deserializes AND a
modeled non-key field holds a non-default value** (the R5/KTD5 witness gate). For
*market-data* reads the closure failure mode is well known — a session-dependent
quote/board returns empty `00707` and defers PENDING (see
[[market-hours-read-empty-result-disposition]]), while a session-independent
historical/persistent shape returns non-empty and flips (see
[[closed-window-reachable-read-shapes]]).

The account-lane flip wave (plan `2026-06-28-001-feat-closed-window-account-lane-flip`)
prospected `account_state` reads — balance, deposit, margin order-quantity,
open-interest — on the premise that account state *persists* regardless of market
hours. That premise holds for some reads and **fails in a non-obvious way for the
"capacity/deposit computation" reads**, which is the learning here.

## Guidance

**An account capacity/deposit read can "succeed" with a deserializable, non-empty
body whose every substantive field is zero/default. That is NOT a flip — it fails
the R5 witness gate, exactly like an empty `00707`, but it does not *look* empty.**

The wave probed and smoked seven candidates. Three flipped; four came back with
this all-default signature on a cash-only, position-less paper account:

| TR | read | smoke under closure | disposition |
|---|---|---|---|
| `CSPBQ00200` | 증거금률별 주문가능수량 (margin order-qty capacity) | `00136`, 1 row, `Dps`/`SeOrdAbleAmt`/`PrsmptDpsD1` all 0 (tried OrdPrc 0/75000/10000, two ISINs) | PENDING |
| `CFOEQ11100` | 선물옵션 가정산예탁금상세 (F/O deposit detail) | `00136`, 1 row, `Dps`/`OpnmkDps…`/`CsgnMgn` all 0 | PENDING |
| `t0441` | 선물/옵션 잔고평가 (F/O balance valuation) | `00000`, positions=0, `tappamt`=0 | PENDING |
| `CIDBQ01400` | 해외선물 주문가능수량 (overseas-futures order-qty) | `00136`, 1 row, `OrdAbleQty` default | PENDING |

Contrast the three that flipped, which read **persistent reference / cash-summary**
data rather than a computed capacity:

| TR | read | smoke under closure | disposition |
|---|---|---|---|
| `t0424` | 주식잔고2 (stock cash summary) | `00000`, holdings=0, **`sunamt` (추정순자산) non-default** | FLIPPED (cash-summary) |
| `CLNAQ00100` | 예탁담보융자가능종목 (loanable-stock list) | `00136`, **20 stocks, `IsuNm` non-default** | FLIPPED (reference list) |
| `t0167` | 서버시간조회 (server time) | `00000`, **`time` non-default** | FLIPPED (utility) |

The discriminator is whether the field is **stored account state / reference data**
(deposit cash `sunamt`, a loanable-stock list, server time — all populated
regardless of market hours) versus a **computed capacity** (margin order-quantity,
provisional-settlement deposit, balance valuation — all need either a live
valuation context or an account that actually carries margin/F-O funding). On a
never-funded, position-less paper account the computed-capacity reads return their
envelope with zeros. `00136` ("inquiry completed, possibly with empty data") is the
gateway's own tell that the row carries no substantive content.

### The U2 holdings gate (KTD3)

Determine cash-only-vs-positions from a **typed out-block array length, not the
raw-probe body length**. `make raw-probe` reports only `http`/`rsp_cd`/`body_len`,
and a cash-summary block is always populated, so `body_len` cannot distinguish a
cash-only account from one holding positions. Model the holdings TR first (here
`t0424`), read its position array (`t0424OutBlock1`) from the typed smoke: an empty
array on a non-default cash summary means **cash-only**. That single determination
then **cascades** — it downgrades the entire F/O positions cohort
(`t0441`, `CFOEQ11100`, open-interest) to expected-empty before they are smoked,
so their all-default results are predicted, not surprising.

## Why This Matters

A future closed-window wave that re-prospects `CSPBQ00200`/`CFOEQ11100`/`t0441`
expecting a cash-field flip will repeat a dry prospect — the raw-probe shows a
non-empty body (1076–1943 bytes), which *looks* like data but is field names with
zero values. Recording these as PENDING with the reason "all-default on a cash-only
account; re-test funded/margin-enabled account or open window" (done in
PROVISIONALITY-LEDGER §16) is what stops the re-prospect. It also corrects a
planning assumption: account capacity reads are **not** "best odds via cash fields"
under closure; only stored cash-summary and reference reads are.

## When to Apply

- Any closed-window wave over the account lane (`owner_class: account`).
- Whenever a paper smoke returns `00136`/`00000` with a deserializable 1-row
  out-block — assert the **substantive** field is non-default before flipping;
  do not treat "deserialized + 1 row" as a certify. The offline fixture test must
  assert a real value (e.g. `assert_eq!(cap.seordableamt, "265866666")`), never
  `!is_empty()`, so an all-zero live row is visibly a non-flip.
- Before queuing any positions-dependent account read, run the U2 holdings gate.

## Examples

The flip witness must key on stored state, not a computed/echoed field. The smoke
records a credential-free witness boolean; the human reads it to flip:

```rust
// t0424 (FLIPPED): sunamt is stored deposit cash — non-default on a funded account.
let cash_nondefault = is_non_default_str(&resp.outblock.sunamt);
// CSPBQ00200 (PENDING): SeOrdAbleAmt is a COMPUTED capacity — 0 without a
// valuation context / margin funding, even though the row deserializes.
let cap_nondefault = resp.outblock2.first()
    .map(|c| is_non_default_str(&c.seordableamt)).unwrap_or(false); // -> false here
```

## Prevention

- **Raw-probe `body_len` is not a substance signal for account reads.** A 1KB+
  body can be all-zero field names. Confirm substance at the typed smoke, not the
  probe.
- **`00136` ≠ data.** It is a success code meaning "possibly empty data"; treat it
  like `00707` for the witness gate.
- Two code-level learnings from the same wave's review, captured for cross-reference:
  - Fractional request prices need a **decimal-tolerant** serializer. The i64-only
    `ls_core::string_as_number` quotes a decimal (`"75.50"` → a JSON string) and
    the gateway then returns `IGW40011`; overseas-futures prices use
    `ls_core::string_as_decimal` (i64 → f64 → string). See
    [[ls-gateway-igw40011-numeric-request-fields]].
  - An autonomous account smoke's **panic payload** is not covered by the
    fail-closed dispatch-log suppressor (it scrubs `tracing` events, not panic
    text). `panic!("…: {e}")` re-leaks `LsError::ApiError`'s `rsp_msg` (an
    account-bearing string) to stderr — interpolate no error into the panic; the
    `SMOKE-FAIL` stderr line already classifies the failure credential-free.
