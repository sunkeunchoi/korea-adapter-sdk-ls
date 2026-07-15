---
title: A modify reassigns the order number — seed/fire/teardown harnesses must re-key onto the modify child
date: 2026-07-15
category: docs/solutions/logic-errors
module: ls-sdk order negative-probe (crates/ls-sdk/tests/negative_probe.rs)
problem_type: logic_error
component: testing_framework
symptoms:
  - "Attended CSPAT00701 IGW00000 A/B returns verdict=inconclusive deterministically on every run"
  - "seed-cancel error [code=01433 ...] — cancel of an already-replaced order number"
  - "Fallback reconcile cancels a resting order the owned set never held (ordno=6625 / 4840)"
root_cause: logic_error
resolution_type: code_fix
severity: high
tags: [order-modify, ordno-reassignment, negative-probe, ab-probe, teardown, korea-ls]
---

# A modify reassigns the order number — seed/fire/teardown harnesses must re-key onto the modify child

## Problem

A live-order A/B harness (`run_igw00000_ab_probe`) that seeds an order, performs a
valid **modify**, then snapshots / fires a violation / cancels — keyed every step
off the **seed submit's** order number. A KRX **modify (정정, CSPAT00701) is absolute
and creates a NEW order number**: `CSPAT00701OutBlock2.OrdNo` is a fresh number and
the original is replaced (modify-cancel plan KTD4). So after the valid control
modify, the tracked number was stale, and every downstream leg operated on an order
that no longer existed.

## Symptoms

- The A/B returned `verdict=inconclusive` **deterministically** — two identical
  attended runs, same output. Not gateway flakiness.
- `seed-cancel error [code=01433 ...] (gateway_rejected=true)` — the teardown cancel
  targeted the stale submit number, which the gateway no longer recognized.
- The final `order_reconcile_teardown` fired its **cancel-EVERY-row fallback**
  ("owned set incomplete") and canceled a resting row (`ordno=6625` / `4840`) that
  the `owned` set never contained — i.e. the live order had a number the harness
  never learned.

## What Didn't Work

- **Re-running the probe.** Identical `inconclusive` + `01433` both times — the tell
  that this was a harness defect, not a warm-budget throttle or a session quirk.
- **Reading it as a real may-rest / gateway issue.** The classifier (`classify_igw00000_ab`,
  `negative_probe.rs`) short-circuited to `Inconclusive` because
  `seed_snapshot_from(rows, submit_no)` read the seed **absent** (`s_pre.present=false`)
  — the order was resting under the modify child, not the submit number.

## Solution

After the valid control modify acks, **re-key the tracked order number and the
`owned` set onto the modify's child `OrdNo`**, taken from the modify response
(`fire_inblock` already returns it via `extract_ord_no`, which matches only the
`OrdNo` key — `CSPAT00701OutBlock2.OrdNo` — never the echoed `OrgOrdNo`):

```rust
// After the valid control modify acks (run_igw00000_ab_probe):
match child_no.filter(|n| !n.trim().is_empty() && n.trim() != "0") {
    Some(child) => {
        owned.remove(&ordno);      // drop the stale submit number
        ordno = child.trim().to_string();
        owned.insert(ordno.clone());
    }
    None => {
        // Acked but surfaced no child number — fail safe, do NOT reuse the stale one.
        println!("{tag} verdict=inconclusive [control modify acked but surfaced no child \
                  order number — cannot re-key onto the live order] — reconciling");
        order_reconcile_teardown(&sdk, symbol, &owned, false).await;
        return;
    }
}
```

`let ordno` becomes `let mut ordno`. Now `S_pre`/`S_post`/the violation fire/the
seed-cancel all target the live resting order. (Shipped in PR #146.)

## Why This Works

A modify replaces the order: the old number is gone, the child is what rests. Keying
off the submit number means snapshotting and canceling a ghost — the cancel returns
`01433` and the snapshot reads absent, which the classifier (correctly, per #137)
treats as untrusted → `Inconclusive`, never `PlacedNothing`. The re-key points every
leg at the order that actually exists. The child number comes from **our** modify
response, so it is never a foreign order.

## Prevention

- **Any seed → (modify) → snapshot → fire → teardown harness against a MODIFY TR
  must re-key onto the modify child, never the submit number.** The submit number is
  valid only until the first successful modify.
- **Fail closed on a missing child number** — never silently fall back to the stale
  number (that reproduces the bug); route to inconclusive + reconcile.
- **Keep every post-re-key teardown `cancel-all` (`owned_fully_constructed=false`).**
  The no-strand guarantee depends on it: the None-branch reconciles while `owned`
  still holds the stale number, and a took-effect fire's grandchild is never added to
  `owned` — both resting orders are caught only by the symbol-scoped cancel-all. A
  future edit flipping any of these to owned-only would strand a live order.
- **Add an offline twin** asserting `extract_ord_no` on a realistic multi-block
  modify response (`OutBlock1.OrgOrdNo` + `OutBlock2.OrdNo`/`PrntOrdNo`) selects the
  child `OrdNo` — the exact property the "never foreign" claim rests on. The re-key
  branch is currently exercised only under the `#[ignore]` live legs.

## Related Issues

- [[modify-reads-stale-retained-orderany-not-maintained-fields]] — the sibling
  modify-staleness trap on the nautilus adapter (stale retained `OrderAny`).
- [[igw40011-ingress-reject-is-placed-nothing-not-may-rest]] — the order-fire
  placed-nothing-vs-may-rest classifier this harness feeds.
- [[order-probe-fill-inclusive-scan-paginates-false-held]] — another order-probe
  snapshot gotcha (fill-inclusive scan).
- `docs/solutions/integration-issues/igw00000-cspat00701-placed-nothing-ab-probe.md`
  — the attended A/B runbook (carries the same re-key caveat at the control leg).
- `docs/solutions/conventions/order-negative-probe-modify-vs-submit-policy.md`
  — why modify legs are probeable (seed + teardown) while submit legs are not.
