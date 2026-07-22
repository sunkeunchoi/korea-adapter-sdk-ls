---
title: "CSPAT00601 booking-determining fields — governed attended A/B to re-characterize an annotated omission"
date: 2026-07-22
last_updated: 2026-07-22
category: integration-issues
module: crates/ls-sdk, metadata/constraints
problem_type: integration_issue
component: orders
symptoms:
  - "CSPAT00601 differential probe records-not-fires an annotated variant: outcome=booking-determining-skip (never fired by design)"
  - "BnsTpCode omission was ACCEPTED live (§30, 00000) and placed a direction-defaulted REAL order (ordno=17093) — the reject/accept code cannot be trusted as the classifier"
  - "OrdprcPtnCode / OrdCndiTpCode / MgntrnCode are annotated booking-determining PROVISIONALLY (R11) with no live observation — the annotation is a sensor-blind spot with no re-observation path"
root_cause: booking_determining_omission_unobservable_by_differential
resolution_type: attended_ab_probe
severity: high
related_components:
  - orders
tags: [ls-gateway, cspat00601, booking-determining, route-c, bnstpcode, ordprcptncode, ordcnditpcode, mgntrncode, raw-probe, negative-probe, attended, fill-detection, close-out]
---

# CSPAT00601 booking-determining fields — governed attended A/B

## Problem

CSPAT00601 (현물 정규주문 — cash-equity order **submit**) carries four fields
annotated `booking_determining: [required]` in
`metadata/constraints/CSPAT00601.yaml`:

- **`BnsTpCode`** — PROVEN (§30, 2026-07-13): live omission → `00000` "accepted",
  the gateway DEFAULTED the direction and booked a REAL resting order
  (`ordno=17093`).
- **`OrdprcPtnCode` / `OrdCndiTpCode` / `MgntrnCode`** — PROVISIONAL (R11):
  never observed live; annotated fail-closed because each is a mode selector
  whose gateway-defaulting would alter WHAT gets booked.

The Route C skip (`order_variant_may_fire`) makes these variants structurally
unroutable in the differential probe — which is correct, but **sensor-blinding**:
a future gateway change that cleanly rejects the omission can never be observed
by the differential. The governed booking A/B harness is the bounded
re-observation path — **the ONLY sanctioned way to fire a booking-determining
omission** — and a harness-confirmed `rejected` verdict is the ONLY thing that
re-opens (R8) or lifts (R11) an annotation.

## The one-command harness

```bash
LS_ORDER_SMOKE=1 LS_ORDER_SMOKE_NONCE=$(date +%s) \
  [LS_AB_FIELD=OrdprcPtnCode] make live-smoke-cspat00601-booking-ab
```

`LS_AB_FIELD` selects the annotated field (default `BnsTpCode`). The run prints
ONE credential-free line:

```
BOOKING-AB target=CSPAT00601 field=<f> verdict=places-defaulted-order(rested)|places-defaulted-order(filled)|rejected|inconclusive|refused
```

(`run_booking_determining_ab_probe` /
`live_smoke_cspat00601_booking_determining_ab` in
`crates/ls-sdk/tests/negative_probe.rs`.)

## Prerequisites (attended, KRX-open)

- An open KRX window and an order-capable paper account; `LS_TRADING_ENV=paper`
  (from the lane file).
- The `.env.domestic` lane file present (no `.env` fallback — wrong-account
  hazard).
- Operator-run / TTY-gated: the fail-closed autonomy chain refuses on CI/no-TTY,
  a missing/stale nonce (`LS_ORDER_SMOKE=1` + fresh
  `LS_ORDER_SMOKE_NONCE=$(date +%s)`), or a non-paper resolved environment.
- **The governed-field gate** (pure, before any dispatch): an `LS_AB_FIELD` that
  is unknown to the embedded CSPAT00601 schema, or not annotated
  `booking_determining: [required]`, REFUSES with `verdict=refused` and places
  nothing. The gate keys on the SAME `is_booking_determining` lookup as the
  fire-loop skip, so "what the differential refuses" and "what the harness may
  fire" cannot drift.

## The seed → fire → snapshot → teardown cycle

1. **Pre-assert-flat** — symbol-scoped `chegb="2"` `t0425` scan must read flat &
   fill-free (a foreign/stranded row poisons new-order detection); plus a
   **`t0424` position baseline** (`janqty`/`mdposqt` for 005930) — the before
   leg of the fill-check.
2. **Seed** — a 1-lot band-floor non-marketable BUY control
   (`CSPAT00601Request::limit` at `dnlmt`), claimed into the owned set.
3. **S_pre** — trusted `chegb="2"` snapshot of the resting seed.
4. **Fire** — the valid 1-lot submit with EXACTLY `LS_AB_FIELD` blanked (the
   empty-string required-omit encoding `generate_invalid_variants` uses), at
   band floor + 1 tick. Captures `http` / `rsp_cd` / any surfaced child `OrdNo`.
5. **Paced S_post** — 1000ms pace, then the `t0425` re-scan (new-resting
   detection) AND a `t0424` position re-read. Any failed read renders the reads
   UNTRUSTED → `inconclusive` (#137: an untrusted read is never evidence).
6. **Fill detection — never assume the defaulted order rests.** An
   omitted-`BnsTpCode` fire can be direction-defaulted to SELL at a
   below-market limit and **EXECUTE**. Fill signals (any one suffices):
   - `t0424` `janqty` delta vs the baseline;
   - a partial-fill row in the scan (`cheqty>0`; the baseline was fill-free);
   - an acceptance ack whose surfaced child `OrdNo` is ABSENT from a trusted
     book (a fully-filled row vanishes from `chegb="2"` by construction).
7. **Teardown** — fail-closed **cancel-all** (`order_reconcile_teardown` with
   `owned_fully_constructed=false`) in EVERY branch; UNEXPECTED-FILL /
   UNOWNED-RESTING alarms preserved; the summary line is credential-free (no
   account numbers, no Korean `rsp_msg`).

## Verdict vocabulary (pure classifier: `classify_booking_ab`)

| Verdict | Condition | Action taken by the harness |
| --- | --- | --- |
| `places-defaulted-order(rested)` | trusted reads + a NEW resting row (the defaulted child) | cancel the child (then cancel-all teardown) — the annotation is CONFIRMED |
| `places-defaulted-order(filled)` | a fill signal (position delta / partial-fill row / ack + absence-from-book) — a fill OUTRANKS a resting row | **sign-aware close-out** (below), verify the position is back at baseline — annotation CONFIRMED |
| `rejected` | a recognized merits reject (placed nothing: a 2xx/4xx business reject or the `IGW40011`-at-500 ingress reject) with NOTHING observed on trusted reads | prints the R8/R11 re-open/lift notice (below) |
| `inconclusive` | throttle (`IGW00201`), transport failure, untrusted reads, an ambiguous ack with nothing observable, any other 5xx | fail-closed cancel-all teardown; re-run |
| `refused` | `LS_AB_FIELD` unknown or not annotated | nothing dispatched |

Observations outrank the fired `rsp_cd` (the book is the truth, not the code): a
reject-shaped code with an observed resting row is still
`places-defaulted-order(rested)`.

### Sign-aware close-out (the filled branch)

Close-only semantics — never oversell, never move beyond the pre-probe
position (`plan_close_out`):

- **The defaulted order BOUGHT** (`janqty` delta positive) → a marketable SELL
  of exactly the delta, capped at the sellable qty (`mdposqt`; an unsettled buy
  with zero sellable places NO close now and is surfaced to the operator),
  priced AT the band floor (the paper-reset flatten pattern).
- **The defaulted order SOLD** (delta negative) → a marketable BUY of exactly
  the delta back, priced AT the band cap — returns to the pre state, never
  beyond flat.
- **No measurable delta** (absence-from-book-only signal) → no close order; the
  operator reconciles by hand.

The harness then re-reads `t0424` and prints `flat=confirmed` /
`flat=NOT-confirmed` / `flat=UNVERIFIED`.

## What a `rejected` verdict triggers (R8 re-open / R11 lift)

A harness-confirmed `rejected` is the ONLY evidence that lifts an annotation:

1. **Constraint annotation** — in `metadata/constraints/CSPAT00601.yaml`, remove
   `required` from the field's `booking_determining` list (drop the key if the
   list empties) and record the harness evidence (date, `http`/`rsp_cd`) in the
   field's annotation comment.
2. **Coverage artifact** — in `metadata/error-coverage/CSPAT00601.yaml`, flip the
   field's `required` row from `status: booking_determining` to
   `status: confirmed` (rejected distinctly) and cite the harness line; add the
   observed reject code to `gateway_codes` if new.
3. **Re-run the differential** — `make live-smoke-cspat00601-negative`
   (in-window): the formerly-skipped variant now fires on the normal
   differential path; expect the same reject → `Clean`.
4. Regenerate docs + gate (`make docs`, `cargo test`, `make docs-check`) — the
   offline pins (`embedded_cspat00601_schema_skips_audited_booking_determining_variants_only`,
   `booking_ab_field_gate_accepts_only_annotated_fields`) assert the annotation
   set and must be updated in the same change.

Either `places-defaulted-order(*)` verdict instead CONFIRMS the annotation:
flip the coverage row's comment from provisional to harness-characterized (the
status stays `booking_determining`); nothing else changes.

## Safety / evidence discipline

Capture **credential-free** evidence only: `http` / `rsp_cd`, order numbers, the
flat verdicts, and the single `BOOKING-AB` line. **Never** capture `rsp_msg` or
account numbers. The order legs are operator-run; the harness refuses to run
unattended and tears down cancel-all in every branch.

## See also

- [`igw00000-cspat00701-placed-nothing-ab-probe`](./igw00000-cspat00701-placed-nothing-ab-probe.md)
  — the attended A/B precedent this harness generalizes (modify leg).
- [`order-negative-probe-modify-vs-submit-policy`](../conventions/order-negative-probe-modify-vs-submit-policy.md)
  — why a submit-leg booking field is never fired by the differential (Route C).
- `metadata/error-coverage/CSPAT00601.yaml` — the per-(field, class) status
  ledger the `rejected` procedure edits.
- `metadata/PROVISIONALITY-LEDGER.md` (§30) — the BnsTpCode origin event
  (`ordno=17093`).
