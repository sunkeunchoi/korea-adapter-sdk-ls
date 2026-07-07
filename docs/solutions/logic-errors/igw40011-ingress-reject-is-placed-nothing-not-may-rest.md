---
title: "IGW40011-as-500 is a placed-nothing ingress reject, not a may-rest 5xx — classify it as such at both order seams"
date: 2026-07-07
category: logic-errors
module: ls-core dispatch_once (order non-2xx branch) + ls-sdk negative_probe (run_order_negative_probe)
problem_type: logic_error
tags: [order-safety, igw40011, may-rest, fail-closed, negative-probe, dispatch, classification, order-probe, re-cert]
---

## Problem

The order safety model treats a `5xx` on an order dispatch as **may-have-rested** (the request
might have reached the exchange before the failure), so it never collapses a `5xx` to a clean
rejection — it holds the order pending and reconciles. That default is correct for a genuine
transport/gateway failure, but it is **wrong for `IGW40011`**, which the LS gateway returns as
`http=500`. `IGW40011` is an **ingress input-validation reject** (a numeric request field sent as a
quoted string): the gateway refuses it *before* routing to the exchange, so it structurally placed
nothing and cannot coexist with a resting order. Treating it as may-rest caused two failures:

1. **Offline order negative-probe halts the differential.** `run_order_negative_probe` fires a
   `type` variant that *deliberately* malforms a numeric field; the gateway correctly rejects it
   with `IGW40011 (500)`; the fire loop's `http >= 500 => Held-may-rest halt=true` arm halted the
   whole differential before it completed, so the §27 order quartet (`CSPAT00601/00701/00801`) could
   never certify for `recommended`.
2. **Live order path mis-classifies as Pending.** `ls-core` `dispatch_once`'s order non-2xx branch
   mapped every non-2xx order outcome — including `IGW40011@500` — to `LsError::AmbiguousOrder`,
   which `adapters/nautilus` `classify_submit_error` maps to `SubmitAction::Pending` (may-rest →
   reconcile), instead of the correct `Reject` (placed-nothing).

## Symptoms

```
NEG-PROBE target=CSPAT00601-negative variant field=OrdQty class=type
result=[http=500 rsp_cd=IGW40011] outcome=Held-may-rest halt=true
```

Observed live (attended open-KRX re-probe, §28): `CSPAT00601 OrdQty/type → IGW40011 (500) → halt`;
`CSPAT00701/00801 OrgOrdNo/type → IGW40011 (500) → halt`. The quartet's happy path (submit/modify/
cancel) certified, but the required negative-probe differential halted on its first `type` variant.

## What didn't work

- **The prior §27 fix was a different bug.** `order-probe-fill-inclusive-scan-paginates-false-held.md`
  fixed a `chegb=0` pagination false-HELD at *pre-assert-flat*. That fix was confirmed working live
  in the same §28 re-probe — controls placed and torn down flat, no pagination HELD — which is
  exactly what surfaced *this* distinct blocker downstream, at the variant-firing stage.
- **Fixing it in `classify_submit_error` would violate that module's core rule.** The obvious-looking
  patch — special-case `IGW40011` inside `adapters/nautilus` `classify_submit_error` — is wrong.
  That function is deliberately keyed on the `LsError` **variant**, never `rsp_cd` (its header calls
  `rsp_cd`-sniffing "the documented fail-open trap two reviewers caught"). Sniffing the code inside a
  variant-keyed classifier re-introduces exactly that trap.

## Solution

Encode "which `rsp_cd` is a placed-nothing ingress reject" **once** in `ls-core`, and consume it at
the two seams that make the classification decision.

**Single source of truth** (`crates/ls-core/src/error_catalog.rs`):

```rust
/// `true` iff `rsp_cd` is a gateway INGRESS input-validation reject — refused before
/// routing to the exchange, so it placed nothing. Deliberately narrow: only `IGW40011`.
pub fn is_ingress_validation_reject(rsp_cd: &str) -> bool {
    rsp_cd == "IGW40011"
}
```

**Live path — fix at the variant-SELECTION seam, not the variant-keyed classifier**
(`crates/ls-core/src/inner.rs` `dispatch_once`, order non-2xx branch):

```rust
if policy.is_order {
    // NEW: an ingress reject placed nothing → clean ApiError (→ SubmitAction::Reject),
    // not AmbiguousOrder (→ may-rest). classify_submit_error stays untouched.
    if crate::error_catalog::is_ingress_validation_reject(code) {
        return Err(LsError::ApiError { code: code.to_string(), message });
    }
    return Err(LsError::AmbiguousOrder { code: code.to_string(), message }); // every other non-2xx
}
```

`classify_submit_error` needs **no change** — it already maps `ApiError → Reject`; making
`IGW40011@500` surface as `ApiError` at the seam that *chooses* the variant does the rest.

**Offline probe** (`crates/ls-sdk/tests/negative_probe.rs`): the inline `http >= 500` halt arm
becomes a pure, unit-tested `classify_fired_variant(http, rsp_cd) -> PlacedNothing | MayHaveRested |
Accepted` that routes `IGW40011@500` to `PlacedNothing` (Clean, continue) via the same shared
predicate; every other `5xx` stays `MayHaveRested` (halt), the `None` transport-failure arm is
unchanged, and a 2xx ack still trips `WAVE BLOCKED`.

## Why this works

`IGW40011` is refused at the gateway's ingress before the order is routed, so no order can exist to
reconcile — the may-rest conservatism is protecting against an impossibility. The exemption is
exact-string-narrow (`== "IGW40011"`), so every other `5xx`, every non-2xx with an empty/other body,
and every transport failure keep the fail-closed may-rest default: the safety property is preserved,
only the one provably-impossible case is exempted. Fixing at the **variant-selection** seam (where
`ApiError` vs `AmbiguousOrder` is chosen) keeps `classify_submit_error` purely variant-keyed and lets
one predicate serve both the live path and the offline probe, so they cannot drift on which codes are
placed-nothing — the same drift-prevention discipline the codebase already uses for the order-ack set.

## Prevention

- **`IGW40011` is NOT the rate-limit code — that is `IGW00201`.** A natural confusion (both can
  appear around a rejected order), but they are semantically opposite for reconcile purposes:
  `IGW40011` = ingress validation (placed nothing, definitive); `IGW00201` = throttle (paced away,
  stays may-rest if it ever surfaced as a 5xx). Do not lump them.
- **Classify an order failure by whether it could have RESTED, not by its HTTP status.** A `5xx` is a
  proxy for "may have reached the exchange," not the actual question. An ingress-validation reject is
  a `5xx` that provably didn't. When adding a new gateway code to the placed-nothing set, require
  per-code evidence that it is a *pre-routing* reject; default everything else to may-rest.
- **Fix a classification at the seam that CHOOSES the type, not at a downstream consumer keyed on the
  type.** Adding `rsp_cd` inspection to a deliberately variant-keyed classifier
  (`classify_submit_error`) re-opens the fail-open trap that keying on the variant was designed to
  close. See `adapters/nautilus/src/orders/map.rs` header.
- **Share the leaf predicate; don't mirror a constant.** When two paths (offline probe + live
  dispatch) must agree on a wire-code decision, have both call one `ls-core` function rather than
  copying a code list — a direct shared call is stronger than the mirrored-constant crosscheck test
  (`is_order_placement_success_recognizes_the_ls_core_ack_set`) it replaces.
- **Regression coverage** (`crates/ls-core/src/inner.rs`, `crates/ls-sdk/tests/negative_probe.rs`,
  `adapters/nautilus/src/orders/map.rs`): `IGW40011@500 → ApiError{code:"IGW40011"}` on the live
  path; a non-`IGW40011` 5xx and an empty-body 5xx both stay `AmbiguousOrder`; the predicate is
  `IGW40011`-only; `classify_fired_variant` covers the four `(http, rsp_cd)` cases incl. a 5xx
  carrying an ack code → still `MayHaveRested`; and the end-to-end
  `classify_submit_error(ApiError{IGW40011}) == Reject`.

## Related

- `order-probe-fill-inclusive-scan-paginates-false-held.md` — the sibling §27 order-probe false-HELD
  (a *different* root cause: `chegb=0` pagination at pre-assert-flat). Its live confirmation is what
  surfaced this blocker. Consider consolidating the two order-probe HELD learnings if a third appears.
- `../integration-issues/ls-gateway-igw40011-numeric-request-fields.md` — how to *avoid producing*
  `IGW40011` (serialize numeric request fields as JSON numbers). This doc is the complement: how to
  *classify* it when the gateway returns one.
- Ledger `metadata/PROVISIONALITY-LEDGER.md` §29 (this fix), §27/§28 (the re-cert wave that surfaced it).
- Plan `docs/plans/2026-07-07-002-fix-igw40011-placed-nothing-order-differential-plan.md`.
