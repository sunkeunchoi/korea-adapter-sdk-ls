---
title: "Seed a merits-reject allowlist from each leg's certified reject evidence — a genuine reject can be an exchange business code, not just an IGW gateway code"
date: 2026-07-14
category: conventions
module: "ls-core preflight negative-probe classifier (crates/ls-core/src/preflight.rs), ls-sdk negative_probe, metadata/error-coverage"
problem_type: convention
component: tooling
severity: high
applies_when:
  - "Inverting a differential negative-probe classifier from reject-any-non-success to a positive merits-reject allowlist (strict inversion with a fail-safe default)"
  - "Seeding an allowlist of accepted reject codes that governs multiple probe/certification legs with independently-certified dispositions"
  - "A live-certified CLEAN chain rejects a variant with an exchange business code (e.g. 00009) rather than an IGW gateway code"
  - "Editing is_read_merits_reject / is_noneval_code / classify_probe in crates/ls-core/src/preflight.rs"
  - "A plan open-question asks to confirm no in-scope leg emits a reject code outside the seed"
tags:
  - negative-probe
  - preflight
  - merits-reject
  - allowlist
  - certification
  - error-coverage
  - business-code
  - re-cert
---

## Context

The differential negative probe (`crates/ls-sdk/tests/negative_probe.rs`) fires a valid control request plus one mechanically-generated invalid variant per declared constraint against the live paper gateway, in one session, and classifies each variant against the control. The classifier is a two-stage function. First `read_variant_verdict` (`negative_probe.rs:242`) maps a `(http, rsp_cd)` pair to a three-way `VariantVerdict` (`crates/ls-core/src/preflight.rs:491`) — `Accepted`, `Rejected`, or `Inconclusive`. Then `classify_probe` (`preflight.rs:513`) folds that verdict against whether the control succeeded into a `ProbeOutcome`: on a passing control, `Accepted → Divergent`, `Rejected → Clean`, `Inconclusive → Held`.

The read leg was reworked from a loose rule — "any non-success code is a rejection, therefore `Clean`" — to a **strict merits-allowlist inversion**: a variant is `Rejected → Clean` only if its `rsp_cd` is in a positive allowlist of codes the gateway is known to return *after evaluating a read variant on merits and refusing it*; everything else (an `IGW00201` throttle, a hard-gateway `IGW50008`, any unknown code) falls to `Inconclusive → Held` and is re-probed. That allowlist is `is_read_merits_reject` (`preflight.rs:569`).

The motivating case was a throttle. `t8412` fires ~12 calls (control + 11 variants); with no inter-dispatch pace they collided in the market-data rate bucket and every variant read a self-inflicted `IGW00201` throttle. Under the old loose rule `IGW00201` is a non-success code, so it was read as a rejection → a false `Clean` — the differential was never evaluated on merits yet reported as confirming the bound. The sibling predicate `is_noneval_code` (`preflight.rs:535`, `{IGW00201}` only) routes exactly that throttle to `Inconclusive → Held`. Making the classifier a strict inversion — allowlist the merits rejects, fail everything else to `Held` — is fail-safe on throughput: a false `Held` only costs a re-probe.

But strict inversion is not free. An under-seeded allowlist silently converts a real, previously-certified `Clean` disposition into a permanent `Held`. That is the trap this learning is about.

## Guidance

**Seed a merits-allowlist reject classifier from each in-scope leg's own certified reject evidence — the per-TR `metadata/error-coverage/<tr>.yaml` "PROBED" chains — enumerating exchange business codes as well as gateway-ingress IGW codes. Do not seed it from the gateway-ingress code vocabulary alone.**

The read allowlist was initially reasoned about as if the merits-reject vocabulary were the hard-gateway ingress codes `{IGW40011, IGW40013}`. That is the ingress code family, and it is real — `IGW40011` is delegated to `is_ingress_validation_reject` (`crates/ls-core/src/error_catalog.rs:82`, `IGW40011` only), and `IGW40013` is the certified `t0425 sortgb/required → IGW40013 → Clean` anchor. But a genuine merits reject can also be an **exchange business code**. `t1101`'s certified CLEAN chain (`metadata/error-coverage/t1101.yaml`) records two distinct rejections: `shcode/format → IGW40011` and `shcode/required → rsp_cd=00009`. `00009` is a business reject ("조회할 자료 없음" / invalid query key) — the gateway *routed* the request and the exchange refused a blank `shcode` on merits. That is still a merits evaluation for probe purposes, but it is not an IGW code.

Under an ingress-only allowlist, `00009 → Inconclusive → Held`, silently regressing `t1101` — a `support.recommended: true` TR (`metadata/trs/t1101.yaml:21`) — from certified-`Clean` to permanently-`Held` on its next live probe.

The current seed (`preflight.rs:569`) is correct:

```rust
pub fn is_read_merits_reject(rsp_cd: &str) -> bool {
    crate::error_catalog::is_ingress_validation_reject(rsp_cd)  // IGW40011
        || rsp_cd == "IGW40013"
        || rsp_cd == "00009"
}
```

Follow the same discipline these predicates already model:

- **Per-code evidence in the doc-comment.** Each of the three codes carries its provenance in the `is_read_merits_reject` doc-comment: `IGW40011` (ingress input-validation reject), `IGW40013` (the t0425 §30 anchor, "must stay in this set or that certified anchor regresses `Clean → Held`"), `00009` (the t1101 error-coverage anchor, "an exchange business reject, not a gateway ingress code"). This mirrors the narrow-predicate discipline of `is_ingress_validation_reject` (`error_catalog.rs:82`) and `is_noneval_code` — each is "deliberately narrow… add a sibling only with per-code evidence."

- **Verify by walking every in-scope leg's error-coverage file.** For each leg the classifier governs, open its `metadata/error-coverage/<tr>.yaml` and enumerate every distinct reject code its PROBED chain records. Every such code must be in the allowlist. Do not infer the vocabulary from the ingress-code catalog.

- **A plan's disposition-preservation claim must be checked against the live per-leg evidence, not assumed.** The plan's own open question flagged exactly this regression risk, but its disposition-impact analysis asserted the reads returned only `00000`/`IGW40011` and missed `00009`. The meta-lesson: when a strict inversion narrows what counts as a confirmation, a claim that "no existing disposition regresses" must be walked against each leg's error-coverage evidence, code by code — an aggregate assertion is not enough.

## Why This Matters

A strict-inversion classifier trades one failure mode for another. It removes a **false `Clean`** (the throttle being read as a confirmation, which silently promotes an unconfirmed bound) at the cost of exposure to a **false `Held`** whenever the allowlist under-reports the real merits-reject vocabulary.

Those two costs are not symmetric, but neither is free:

- A false `Held` on a *newly probed* variant is cheap — it costs a re-probe, and the fail-safe direction is correct.
- A false `Held` on a *previously certified* disposition is a **silent de-certification**. `t1101` is `support.recommended: true`; regressing its `shcode/required` anchor from `Clean` to `Held` demotes a recommended TR with no diagnostic — the probe simply stops confirming a bound it used to confirm, and the TR quietly falls out of certification on the next live run.

The throttle case is *why* the inversion was done; the business-code regression is the *cost that had to be paid back* by evidence-seeding. Both `is_noneval_code` (`{IGW00201}`, the throttle carve-out) and `is_read_merits_reject` (the merits allowlist) are the two halves of the same decision: one names what the gateway never evaluated, the other names what it evaluated and refused. Everything not named by either falls safe to `Held`. That is only sound if the allowlist is complete for every leg it governs.

## When to Apply

- Whenever a classifier is reworked from "anything-not-X is Y" to a **positive allowlist with a fail-safe default** (strict inversion). The inversion narrows what counts as a confirmation, so any disposition that depended on the looser rule must be re-grounded against evidence.
- Whenever the allowlist governs **multiple legs** with independently-certified dispositions. Seed from the union of every in-scope leg's own reject evidence, not from a single leg or a code-family catalog.
- Whenever the reject vocabulary spans **more than one layer** — gateway-ingress codes (`IGW*`) *and* exchange business codes (`00009`, other `0xxxx`). A layer-scoped mental model ("these are the gateway codes") systematically misses the other layer.
- Whenever a plan asserts "no existing disposition regresses." Treat that as a claim to verify per-leg against error-coverage evidence, not a given.

Note the deliberate contrast in how the two verdict-derivation inputs are keyed (`read_variant_verdict`, `negative_probe.rs:242`): only `Accepted` gates on HTTP 2xx (`(200..300).contains(&http) && is_success(rsp_cd)`), because a divergent *acceptance* is a genuine success response. `Rejected` keys on `rsp_cd` **independent of HTTP status** — a genuine `IGW40011` ingress reject arrives `http=500`. Seeding the allowlist from HTTP-status reasoning ("2xx bodies only") would compound the miss; the merits signal lives in `rsp_cd`, across HTTP statuses.

## Examples

The seed and the resulting `t1101` disposition, before and after:

**Before (ingress-only seed — the trap):**

```rust
fn is_read_merits_reject(rsp_cd: &str) -> bool {
    rsp_cd == "IGW40011" || rsp_cd == "IGW40013"   // gateway-ingress codes only
}
```

`t1101 shcode/required → rsp_cd=00009` (a passing control):
- `00009` not in allowlist → `read_variant_verdict → Inconclusive`
- `classify_probe(control_ok=true, Inconclusive) → Held`
- Certified anchor **regresses `Clean → Held`** — `t1101` (recommended) silently de-certified.

**After (evidence-seeded — the fix, `preflight.rs:569`):**

```rust
fn is_read_merits_reject(rsp_cd: &str) -> bool {
    crate::error_catalog::is_ingress_validation_reject(rsp_cd)  // IGW40011
        || rsp_cd == "IGW40013"   // t0425 sortgb/required anchor (§30)
        || rsp_cd == "00009"      // t1101 shcode/required business anchor (error-coverage/t1101.yaml)
}
```

`t1101 shcode/required → rsp_cd=00009`:
- `00009` in allowlist → `read_variant_verdict → Rejected`
- `classify_probe(true, Rejected) → Clean`
- Anchor **restored to `Clean`** — disposition preserved.

Verification walk — every in-scope read leg's distinct reject codes must be in the allowlist:

- `metadata/error-coverage/t1101.yaml`: PROBED chain records `shcode/required → 00009`, `shcode/format → IGW40011`. Both in seed. ✓
- `t0425 sortgb/required → IGW40013` (ledger §30 anchor). In seed. ✓
- `t1102 shcode/format → IGW40011`; `shcode/required` and `exchgubun/required` are `gateway_tolerant` (accepted → `00000`, `expected-tolerant`, not a reject). ✓
- `CSPAQ12200 BalCreTp/required` is `gateway_tolerant` (accepted → `00136`, not a reject). ✓
- `t8412` is `not_probed` — its variants stay legitimately `Held`. ✓

The unit test `is_read_merits_reject_is_igw40011_igw40013_and_00009` (`preflight.rs:1118`) pins the exact set and asserts the fail-safe direction for the excluded codes (`IGW00201` throttle, `00000`/`00136` success, hard-gateway `IGW50008`, unknown `40510`) — all `Inconclusive → Held`, never a silent `Clean`.

## See also

- [`gateway_tolerant` facet — unblock the probe without weakening preflight](./gateway-tolerant-facet-preserves-preflight-while-unblocking-differential-probe.md) — the **accepted-violation** side of the same `classify_probe` verdict layer (`Divergent → expected-tolerant`); this doc is the **rejected-code** side (`Clean` vs `Held`). Both extend `classify_probe`.
- [IGW40011 ingress reject is placed-nothing, not may-rest](../logic-errors/igw40011-ingress-reject-is-placed-nothing-not-may-rest.md) — the narrow-predicate / per-code-evidence discipline this learning follows, and the source of `is_ingress_validation_reject` (IGW40011) that the allowlist reuses; also the "IGW40011 ≠ IGW00201, do not lump them" distinction the noneval carve-out formalizes.
- [Order error classifier: placed-nothing vs may-rest](./order-error-classifier-placed-nothing-vs-may-rest.md) — classify by the semantic meaning of the code, not the raw HTTP/`rsp_cd` surface; the read-leg analogue.
- [IGW00201 continuation page-bursts vs paced single reads](../integration-issues/ls-gateway-igw00201-continuation-page-bursts-vs-paced-single-reads.md) — what `IGW00201` is (the cumulative warm-sensitive throttle the noneval carve-out routes to `Held`).
- [Normalized baseline can under-report a request block](./normalized-baseline-can-underreport-request-block.md) — the same "an offline/assumed source under-reports what certified live evidence proves; live evidence is authoritative" posture, a different mechanism.

Implementing plan: `docs/plans/2026-07-13-001-fix-throttle-inconclusive-negative-probe-plan.md` (KTD2 strict inversion, KTD3 merits seed, KTD5 shared decision core). Ledger: `metadata/PROVISIONALITY-LEDGER.md` §27 (t8412 throttle-masked false-`Clean`), §30 (t0425 `IGW40013` anchor).
