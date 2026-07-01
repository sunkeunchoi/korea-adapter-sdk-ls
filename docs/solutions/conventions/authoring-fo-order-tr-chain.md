---
title: Authoring the domestic F/O order chain (CFOAT) — wire + harness gotchas
date: 2026-06-30
last_updated: 2026-07-01
problem_type: convention
module: orders
component: futureoption-order-chain
tags:
  - orders
  - futures-options
  - cfoat
  - igw40011
  - string-as-decimal
  - order-smoke
  - fail-closed
  - reconciliation
  - live-certification
related:
  - "docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md"
  - "docs/solutions/architecture-patterns/autonomous-order-smoke-fail-closed-contract.md"
  - "docs/solutions/conventions/implement-tr-registration-sites.md"
  - "docs/solutions/integration-issues/ls-paper-01491-account-not-order-capable.md"
---

# Authoring the domestic F/O order chain (CFOAT) — wire + harness gotchas

## Context

The domestic stock order chain (`CSPAT00601` submit / `CSPAT00701` modify /
`CSPAT00801` cancel) is the template every future order surface copies. When the
first NON-stock order chain was authored — the domestic futures/options chain
`CFOAT00100` / `CFOAT00200` / `CFOAT00300` (`crates/ls-sdk/src/orders/futureoption.rs`,
plan 2026-06-30-003) — five things differed from the stock template in ways that a
copy-paste-from-CSPAT gets silently wrong. Three are wire-shape traps that would
only surface as a live-gateway rejection (or, worse, a mis-parsed order number); two
are order-smoke safety gaps that a cross-model review caught and that the stock
harness's `t0425` working-orders backstop had been quietly papering over. This doc is
the checklist for the next non-stock order chain (overseas futures `CIDBT`, overseas
stock `COSAT`/`COSMT`, night-derivative `CCENT`).

## Guidance

### 1. A fractional order-price field uses `string_as_decimal`, not `string_as_number`

Numeric **request-body** fields must serialize as JSON numbers or the gateway returns
`IGW40011` (the standing gotcha). The stock chain's `OrdPrc` is integer-valued, so it
uses `ls_core::string_as_number` — but that helper is **i64-only**: given a fractional
string like `"342.25"` it falls through to `serialize_str`, emitting a quoted string,
which is exactly the `IGW40011`-triggering shape.

F/O prices ARE fractional (`FnoOrdPrc`, e.g. `342.25`). It must use
`ls_core::string_as_decimal`, which emits an integer as `i64` and a decimal as `f64`
(both unquoted JSON numbers). Integer numeric fields (`OrdQty`, `OrgOrdNo`, `MdfyQty`,
`CancQty`) stay on `string_as_number`.

```rust
// F/O order price is fractional -> string_as_decimal (NOT string_as_number)
#[serde(rename = "FnoOrdPrc", serialize_with = "ls_core::string_as_decimal")]
pub fnoordprc: String,
// quantity / order-number fields are integral -> string_as_number
#[serde(rename = "OrdQty", serialize_with = "ls_core::string_as_number")]
pub ordqty: String,
```

Read the wire type from the normalized baseline: `propertyType` `A0004` = Number, and
the `req_example` shows whether it carries decimals. Decide per field, not per struct.
Keep the doc comment honest — a comment claiming `string_as_number` over a field that
actually uses `string_as_decimal` is its own review finding.

### 2. Per-venue order success codes: the raw `res_example` seed can be WRONG — only the live run pins it

**Live correction (2026-07-01 certification, plan 2026-07-01-001):** the offline
authoring seeded F/O modify `00132` and cancel `00156` from the `CFOAT00200`/`CFOAT00300`
raw `res_example`s. Both were **wrong**. The live paper gateway returned **modify `00462`
and cancel `00463`** — i.e. the F/O family uses the **domestic-stock ack codes exactly**
(submit `00040`/`00039`, modify `00462`, cancel `00463`). The first operator run showed
cancel `00463` ("취소주문 완료"), and once the modify was fixed (see the qty rule below)
the second and third runs both acked modify `00462`.

```rust
fn rsp_cd_is_order_success(code: &str) -> bool {
    matches!(code, "00039" | "00040" | "00462" | "00463")
    //  F/O shares the stock-family codes; the 00132/00156 raw-example seeds were disproven live
}
```

Lesson: the raw `res_example` is a **seed, not ground truth** — a per-leg code stays
provisional until a live run acks it. Mark seed codes as seed-only in the code comment
(`crates/ls-sdk/tests/order/fo.rs` labels each ack set with its confirmation state), and
never flip a leg to Implemented off an unobserved code. When a sibling family (here:
stock) is already live-confirmed, prefer *its* codes over an unproven raw seed.

### 2a. F/O 정정 (modify) can only REDUCE quantity — an increase is rejected `01442`

`CFOAT00200`'s `MdfyQty` is an **absolute** target quantity, and the F/O gateway rejects
a modify whose target **exceeds** the resting quantity with `01442` (정정수량이 정정가능수량을
초과). So a modify-quantity smoke must submit qty ≥ 2 and modify **down** (e.g. 2 → 1); a
copy-from-stock harness that submits qty 1 and modifies to 2 fails `01442` every run
(this masqueraded as a harness bug for two operator sessions before the direction was
fixed). To exercise modify without extra margin you could instead modify the *price* to
another valid tick, but the daily-limit anchor gives only one on-tick price per side, so
the quantity reduction is the cheaper lever.

### 3. The F/O modify/cancel result block has NO `PrntOrdNo` — the parent is in OutBlock1

The stock modify/cancel returns the **new** order number in `OutBlock2.OrdNo` and the
**parent** in `OutBlock2.PrntOrdNo`. The F/O modify/cancel `OutBlock2` carries **no
`PrntOrdNo`** — the original order number is echoed in `OutBlock1.OrgOrdNo` instead. So:

```rust
impl CFOAT00200Response {
    pub fn order_no(&self) -> &str { &self.outblock2.ordno }          // NEW number, OutBlock2
    pub fn parent_order_no(&self) -> &str { &self.outblock1.orgordno } // parent, OutBlock1 (no PrntOrdNo)
}
```

Read the true block membership of every surfaced field from the raw capture's
`res_b` property list, keyed by block; do not assume CSPAT's layout.

### 4. Price the resting order at the daily limit (`t2111`), never the intraday book

F/O intraday 호가 feeds (`t2112`) are **paper-empty even mid-session**, so the resting
limit must be anchored to the **daily price limit** — `상한가`/`하한가` (`uplmtprice`/
`dnlmtprice`), which are static and reliably populated. These live on the already-
Implemented `t2111` F/O quote, but its SDK struct is a *representative subset* that did
not model them — extend it with the two fields (an additive subset extension; it does
not change the projected baseline). Buy rests at `dnlmtprice` (far below market), sell
at `uplmtprice`. Pass the **verbatim gateway string** as the limit price — it is a
guaranteed valid tick, which sidesteps re-deriving the per-product F/O tick ladder. The
band validator FAILS CLOSED: an unparseable / zero / inverted / empty anchor aborts and
places nothing — a missing anchor must never fall back to a near-market (fillable)
price.

### 5. F/O flatness is two-part and fail-closed — there is no F/O working-orders read

The stock chain confirms post-teardown flatness by scanning `t0425` (a working-orders
미체결 read) account-wide. **No F/O 미체결 read is Implemented.** So F/O flatness has two
independent parts, both fail-closed:

- **No fill** — `t0441` (선물옵션잔고평가) detects a *filled position* (잔고 `jqty`), not a
  resting order. **Live correction (2026-07-01):** the offline authoring made an empty
  `t0441` read `NotFlat` (fail-closed), but a position-less account returns an **empty
  array** (not a `positions=0` row) on *every* successful flat run — so `NotFlat`-on-empty
  made the always-flat chain (unfillable daily-limit rest + clean cancel) **impossible to
  certify**. The verdict must read an **`Ok`-empty (or all-zero) `t0441` as Flat**. This is
  not fail-open: a genuinely *failed* read is the caller's `Err` arm (still panics
  `fo-flat-scan-failed`), and a real *fill* still surfaces as a non-zero `jqty` row →
  `Fill`. Detect a position with `f64 != 0.0`, not a `u64` parse — an F/O position can be
  **short (negative)** and a `u64` parse would silently drop `-1`; an unparseable quantity
  is treated as a position (fail-safe).
- **Resting-order removal** — confirmed ONLY by a **clean cancel response**. There is no
  board read to verify it independently.

The load-bearing consequence (a cross-model review caught this twice — once for the stock
harness's `t0425` backstop, once for the flatness relaxation below): **any state where the
resting order's removal is unconfirmable must HARD-FAIL, never print a success line.** Four
such states, all funneled to one loud `set_orders_enabled(false)` + `panic!`
operator-action-required signal:

1. An **accepted submit that returns no usable order number** — an order may rest that
   the harness cannot reference to cancel. Do not silently record Pending and return.
2. A **submit that fails AMBIGUOUSLY** (transport / `AmbiguousOrder` error) — the order
   MAY have reached the gateway and be resting, with no order number to cancel. This one
   is the subtle trap: since `t0441` sees fills only, and (after the correction above) an
   empty `t0441` now reads **Flat**, an ambiguous-submit path that leans on the flatness
   check would pass `1 passed` with a real resting order alive. It must hard-fail on its
   own — do not route resting-order safety through the fill verdict.
3. An **`Ok` modify that is not cleanly certified** (wrong-leg/unrecognized code or
   empty new order number) — it is ambiguous which order is now live.
4. A **non-clean cancel** — removal is unconfirmed and `t0441` (fills only) cannot see
   the survivor.

**The evolved principle:** one fail-closed check often serves two jobs. The old
`NotFlat`-on-empty verdict was simultaneously (a) the happy-path flatness confirmation and
(b) the catch-all for a resting order after an ambiguous placement. Relaxing it for (a)
(so the flat chain can certify) silently removed (b). When you relax or remove a
fail-closed guard, **enumerate every path that depended on it** and give each its own
explicit guard — here, the placement-error paths (#1, #2) hard-fail independently of the
fill verdict, so `empty t0441 = Flat` is only ever trusted *after a clean cancel has
proven removal*.

Gate the new-order-number adoption on clean certification: a cleanly-certified modify
transitioned the order to the new number (cancel that); an ambiguous modify best-effort
cancels the known order but forces the teardown hard-fail.

### 6. Certify per-leg with leg-specific code sets, not the coarse runtime union

The runtime `classify_order_rsp_cd` accept gate is intentionally a union (it only
decides retry/dedup). The **flip decision** (which legs the operator promotes) needs the
stricter per-leg check, keyed on the business code + order number, **never status text**
(PR #74 shipped a status-text cert bug that only review caught):

```rust
const FO_SUBMIT_OK: [&str; 2] = ["00040", "00039"]; // confirmed live 2026-07-01
const FO_MODIFY_OK: [&str; 1] = ["00462"];          // confirmed live (was seed 00132)
const FO_CANCEL_OK: [&str; 1] = ["00463"];          // confirmed live (was seed 00156)
fn fo_leg_certified(expected: &[&str], rsp_cd: &str, order_no: &str) -> bool {
    let o = order_no.trim();
    expected.contains(&rsp_cd.trim()) && !o.is_empty() && o != "0"
}
```

A submit that returns a modify/cancel code is a gateway anomaly and must NOT certify as
a clean submit — leg-specific sets enforce that; a single shared set would not.

### Registration (unchanged from the order template)

Order `{TR}_POLICY` consts (`is_order: true`) register in the policy-index crosscheck
`policies` array **only** — never in `slice_rest_policies_are_non_order_rest`. The new
F/O order module gets a sibling facade handle (`FoOrders` via `sdk.fo_orders()`) that
routes every leg through `post_order`. See `implement-tr-registration-sites.md` for the
full count-site map.

## Why This Matters

A fractional price on `string_as_number` (#1) and a wrong success code (#2) each cost a
live-gateway round-trip to discover, on a credential-gated paper account that may not
even be reachable that session. A mis-read order number (#3) is worse — it silently
reconciles or cancels the wrong order. The harness gaps (#5, #6) are the dangerous ones:
F/O is margin-bearing and a fill cannot be undone, so a teardown that prints "success"
while a resting order survives is precisely the failure the safety harness exists to
prevent — and the stock template's `t0425` backstop meant the gap was invisible until a
venue without a working-orders read (F/O) copied it.

## When to Apply

Authoring any non-stock order chain: overseas futures (`CIDBT`), overseas US stock
(`COSAT`/`COSMT`), KRX night derivatives (`CCENT`), or any future order venue. Each gets
its own fractional-price audit, its own success-code read, its own block-membership
read, its own price anchor, and — if it lacks an Implemented working-orders read — the
two-part fail-closed flatness with the teardown-uncertain hard-fail.

## Examples

The reference implementation is the CFOAT chain itself:
`crates/ls-sdk/src/orders/futureoption.rs` (structs, serializers, `order_no`/
`parent_order_no`, `FoOrders`), `crates/ls-core/src/endpoint_policy/order.rs` (the three
`is_order: true` policies), and `crates/ls-sdk/tests/order/fo.rs` (the F/O harness after
the test-file decomposition: `fo_order_chained_smoke`, `validate_fo_band`,
`fo_flat_verdict`, `fo_qty_is_position`, `fo_leg_certified`, the `teardown_uncertain`
hard-fail, and the ack-code constants). The operator runs it via `make live-smoke-fo-order`
— which now self-sources the contract from the `t8467` index-futures master (front-month),
so no `LS_FO_ORDER_SMOKE_SHCODE` is needed (it stays an optional override).

**Harness gotcha (test decomposition):** when the order-smoke tests were split into
`#[path]` submodules (`order/fo.rs`, `order/chain.rs`), their exact test names gained a
`fo::`/`chain::` prefix, but the Makefile smoke targets still passed the bare names to
`cargo test -- --ignored --exact`. `--exact <bare-name>` then matched **0 tests**, and the
`grep -q "1 passed"` guard failed with "did not run (0 tests)" — a silent green-to-broken
drift the offline gate never catches (the live smokes are `#[ignore]`). After any
test-file decomposition, re-derive the `--exact` filters from `cargo test <bin> -- --list`
(via `rtk proxy cargo …` if a summarizing proxy strips the list output).
