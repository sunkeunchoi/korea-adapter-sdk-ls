---
title: "IGW00000 on CSPAT00701 — attended seed/fire/teardown A/B to classify placed-nothing vs may-rest"
date: 2026-07-14
last_updated: 2026-07-14
category: integration-issues
module: crates/ls-sdk, metadata/constraints
problem_type: integration_issue
component: orders
symptoms:
  - "CSPAT00701 negative differential probe halts at OrdprcPtnCode/required with http=500 rsp_cd=IGW00000 → Held-may-rest halt=true"
  - "IGW00000 is undocumented — absent from metadata/error-catalog.yaml and the rest of the codebase"
  - "IGW00000 is success-SHAPED (00000/00136 are the gateway's success families) so the code alone cannot classify the outcome"
root_cause: undocumented_gateway_code
resolution_type: attended_ab_probe
severity: high
related_components:
  - orders
tags: [ls-gateway, igw00000, cspat00701, placed-nothing, may-rest, raw-probe, negative-probe, route-b, ordprcptncode, attended]
---

# IGW00000 on CSPAT00701 — attended A/B to classify placed-nothing vs may-rest

## Problem

CSPAT00701 (현물정정주문 — cash-equity order **modify**, keyed by `OrgOrdNo`) is
HELD. Its negative differential probe fires the `OrdprcPtnCode`/required omit
variant and hits `http=500 rsp_cd=IGW00000 → Held-may-rest halt=true`. `IGW00000`
is **undocumented** (surfaced live for the first time in re-cert wave 3, 2026-07-13).

Two facts make naive classification unsafe:

1. **`IGW00000` is success-SHAPED.** `00000`/`00136` are the gateway's success
   families; a reject reading like `0000` may instead mean *accepted*. The code
   cannot be the classifier — only a **post-order read** can.
2. **CSPAT00701 is a _modify_, not a submit.** The violation mutates an *existing
   resting order we control*. A modify's may-rest signature is **mutation of the
   target order** — which the §30 fallback sweep (`ordno=18372`, "no stranded
   order") never checked: it only looked for a *new* stranded order.

The classification is settled by an attended **seed → snapshot → fire →
re-snapshot → teardown** A/B against an order we control and will cancel.

## Prerequisites (attended, KRX-open)

- An open KRX window and an order-capable paper account; `LS_TRADING_ENV=paper`.
- The `.env.domestic` lane file present (no `.env` fallback — wrong-account hazard).
- The order legs are **TTY-gated / operator-run**; they refuse on CI or a stale
  nonce (`LS_ORDER_SMOKE=1` + a fresh `LS_ORDER_SMOKE_NONCE=$(date +%s)`).
- Paper state for the 07-13 event is gone (`make paper-reset` ran) — a fresh
  controlled A/B is required; no read-only post-hoc reclassification is available.

## Procedure

Leg tags: **[order — TTY-gated, operator-run]** vs **[read — agent-runnable]**.
The cycle is **per-field** — run it for each required-omit variant that can surface
IGW00000 (`OrdprcPtnCode` first, then the downstream `OrdCndiTpCode` / `OrdPrc` if
the re-probe reaches them; see the downstream-variant caveat below).

### 1. Seed — [order, operator-run]

Submit **one non-fillable far-off limit order** and capture its `OrgOrdNo`.

> ⚠️ **Price distance is NOT a sufficient non-fillability guarantee.** The fired
> variant omits `OrdprcPtnCode`, the very field that governs limit-vs-marketable. If
> IGW00000 is may-rest *and* the gateway defaults the omitted pattern to a
> marketable form, the far-off seed can fill **from within the modify**, independent
> of market drift. Prefer a price-limit-locked symbol where a marketable default
> still cannot fill, if one exists; otherwise treat a same-tick fill as possible and
> rely on the **fill-inclusive** `S_post` read (step 5) to catch it.

### 2. Snapshot `S_pre` — [read, agent-runnable]

A **fill-inclusive** `t0425` read for the seed `OrgOrdNo`. Record its
price / qty / type / **pattern (`OrdprcPtnCode`)**.

> **Fill-inclusive is mandatory (the bind signature depends on it).** The
> working-orders read (`t0425` with `chegb="2"`) excludes a fully-filled
> (`ordrem==0`) row **by construction**, so a seed that IGW00000 mutated to a
> marketable state and *filled* would simply **vanish** from a `chegb="2"`-only
> `S_post` — the exact may-rest hazard this probe exists to catch. Use a
> fill-inclusive query for the seed `OrgOrdNo` (the `classify_control_disposition`
> fill-check analogue), not a working-orders-only comparison.

### 3. Control leg — [order, operator-run] (recommended)

A **valid** modify (`OrdprcPtnCode` present) → expect a clean ack (`00462`). This
proves the harness before the violation fire. Re-snapshot so `S_pre` reflects the
post-control (pre-violation) state.

> ⚠️ **A modify is absolute and reassigns the order number** (`CSPAT00701`
> `OutBlock2.OrdNo` is a *new* number; modify-cancel plan KTD4). After this control
> leg, the seed's *submit* order number is **stale** — the live resting order is the
> modify child. All subsequent legs (`S_pre`/`S_post` snapshots, the violation fire,
> the seed-cancel teardown) must key off the **child** `OrdNo` returned here, not the
> original submit number. The one-command harness (`run_igw00000_ab_probe`) re-keys
> automatically from the control-modify response; keying the teardown cancel off the
> stale submit number instead surfaces `01433` (cancel of an already-replaced order)
> and every snapshot reads the seed absent → a spurious **inconclusive** verdict.

### 4. Fire variant B — [order, operator-run]

The same modify with `OrdprcPtnCode` **omitted**, via `make raw-probe` (prints only
`http` / `rsp_cd` / `body_len` — credential-free):

```bash
LS_ORDER_SMOKE=1 LS_ORDER_SMOKE_NONCE=$(date +%s) \
  make raw-probe LS_PROBE_TR_CD=CSPAT00701 LS_PROBE_PATH=/stock/order \
  LS_PROBE_BODY='{"CSPAT00701InBlock1":{"OrgOrdNo":<seed OrgOrdNo>,"IsuNo":"005930","OrdQty":1,"OrdprcPtnCode":"","OrdCndiTpCode":"0","OrdPrc":<band-safe price>}}'
```

Expect `http=500 rsp_cd=IGW00000`. (The full modify body shape is
`order_seed_00701`; the violation sets `OrdprcPtnCode` empty — the LS
required-omit encoding.)

### 5. Snapshot `S_post` — [read, agent-runnable]

Re-read **fill-inclusive** `t0425` for the seed `OrgOrdNo`, **paced** to avoid
IGW00201. Positively re-verify the seed's `OrdprcPtnCode` / price **survived** the
fire (not just its presence in the working-orders set).

### 6. Teardown — [order, operator-run]

Cancel the seed `OrgOrdNo`; assert the symbol is **FLAT** (the `chain.rs` reconcile
pattern / `order_reconcile_teardown`).

## Bind signature (the classification rule)

The snapshot reads **must be fill-inclusive** (see step 2).

| Verdict | Condition |
| --- | --- |
| **placed-nothing (conclusive)** | B → `http=500 rsp_cd=IGW00000` **AND** the seed `OrgOrdNo` is **present and byte-identical** in `S_post` vs `S_pre` (fill-inclusive) **AND** no new resting order. |
| **may-rest (conclusive)** | seed order **mutated** (price/type/qty) **OR** the seed `OrgOrdNo` **vanished / filled** (absent from a fill-inclusive `S_post`) **OR** a new order rested. A vanished seed is may-rest, **never** inconclusive. |
| **inconclusive → stays HELD** | `S_post` is throttled / non-clean (`IGW00201`). Per #137 an untrusted read is Held, **never** placed-nothing. "No observed change" is evidence only when the read is trustworthy. Re-pace and retry, or defer. |

## What each verdict triggers

- **placed-nothing** → activate the dormant **Route B** scoped tolerance: annotate
  `metadata/constraints/CSPAT00701.yaml` `OrdprcPtnCode` with
  `placed_nothing_codes: {required: [IGW00000]}`, prove a CLEAN re-probe
  (`make live-smoke-cspat00701-negative` in-window → `outcome=CLEAN`), and promote
  CSPAT00701 to `recommended`. The runtime seam
  (`ls_core::is_ingress_validation_reject`) stays **untouched** — the probe is
  lenient after this controlled A/B, the runtime stays may-rest/reconcile.
- **may-rest** → CSPAT00701 stays **permanently HELD**, mirroring the CSPAT00601
  BnsTpCode posture; no annotation is applied; flip-evidence = a future gateway
  reclassification or a redesigned probe that provably cannot route.

## ⚠️ Downstream-variant caveat (promotion-blocker)

The full fire loop halts on the **first** `MayHaveRested` variant. In wave 3 it
halted at `OrdprcPtnCode`, so the required-omit variants that follow it in the
schema (**`OrdCndiTpCode`, `OrdPrc`**) were **never observed live**. Once
`OrdprcPtnCode` routes past its halt (via the Route B annotation), the probe reaches
those for the first time — and if any *also* surfaces `IGW00000`/5xx, the re-probe
cannot reach CLEAN and promotion is blocked despite a valid `OrdprcPtnCode` verdict.
Treat a downstream halt as **inconclusive, not failure**: A/B-characterize that
field with this same method and, if placed-nothing, annotate it too before
re-probing. Budget the attended window for this.

## Safety / evidence discipline

Capture **credential-free** evidence only: `http` / `rsp_cd` / `body_len`, the
`t0425` flat verdict, and the bind-signature comparison. **Never** capture
`rsp_msg`. Do **not** place, modify, or cancel orders autonomously — the order legs
are operator-run.

## See also

- [`order-negative-probe-modify-vs-submit-policy`](../conventions/order-negative-probe-modify-vs-submit-policy.md)
  — why CSPAT00701 (modify) is probeable and CSPAT00601 (submit, BnsTpCode) is not.
- [`ls-gateway-igw40011-numeric-request-fields`](./ls-gateway-igw40011-numeric-request-fields.md)
  — the raw-probe A/B format this runbook follows.
- [`order-error-classifier-placed-nothing-vs-may-rest`](../conventions/order-error-classifier-placed-nothing-vs-may-rest.md)
  — the placed-nothing / may-rest distinction and its runtime encoding.
- `metadata/PROVISIONALITY-LEDGER.md` (§30) — the IGW00000 origin event.
