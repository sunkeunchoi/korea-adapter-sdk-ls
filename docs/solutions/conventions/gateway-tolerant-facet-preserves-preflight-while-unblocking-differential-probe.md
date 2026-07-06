---
title: "The `gateway_tolerant` constraint facet: unblock the differential probe on a field the gateway does not enforce, WITHOUT weakening preflight"
date: 2026-07-06
category: conventions
module: "ls-core preflight (crates/ls-core/src/preflight.rs), ls-metadata schema, ls-sdk negative_probe"
problem_type: convention
tags: [constraint-schema, preflight, negative-probe, differential, gateway_tolerant, re-cert, split-facet]
---

## Problem

The differential negative probe blocks promotion when the gateway ACCEPTS an injected
constraint violation that the SDK's constraint schema declares invalid (a `Divergent` outcome).
Live evidence (ledger §26) showed the LS gateway returns `rsp_cd=00000` when a schema-`required`
field is removed (t1102 `shcode`/`exchgubun`, t8412 `shcode`, t0425 `chegb`) or when a date is
malformed (t8412 `sdate`/`edate`). The schema over-claims relative to gateway reality, so the
probe reads a divergence and the `promote-tr` recipe blocks — even though the field is a genuine
**caller contract** the SDK deliberately keeps stricter than the gateway.

The two naive fixes are both wrong:

- **Set `required: false`** to match the gateway — weakens preflight: a caller who omits the field
  no longer gets a local `LsError::Invalid`, and the drift-detector loses the field entirely.
- **A probe-local tolerance allowlist** — loses single-source grounding and the docgen
  "Errors & validation" audit surface.

## The convention

Add a per-field `gateway_tolerant: Vec<String>` facet to the constraint schema — the **class names**
(`"required"`, `"format"`, …) whose accepted violation the gateway is known to tolerate for that
field. It is a **per-class** set (not a field-level bool) because divergence is reported per
`(field, class)` pair: t8412 `shcode` is tolerant on `required` but NOT on `format`, so a
field-level flag would wrongly suppress the `format` divergence too.

Three properties, by construction:

1. **Preflight is untouched.** `validate_field` still fails a `required:true` field when it is
   omitted, regardless of `gateway_tolerant`. The facet informs only the probe. The caller
   contract stays enforced locally (this is the whole point of the split-facet decision).
2. **The probe downgrades, it does not reclassify.** `classify_probe` stays a pure 3-way verdict.
   A thin tolerance layer (`is_gateway_tolerant` / `reported_outcome`) maps `(field, class)`
   against the set and prints `expected-tolerant` (non-blocking) *only* when a `Divergent` lands
   on a marked class. Every other class on the same field still reports its real divergence.
3. **It is additive and backward-compatible.** `#[serde(default, skip_serializing_if =
   "Vec::is_empty")]` on both `FieldConstraint` copies (ls-core + ls-metadata); absent/empty = no
   tolerance, so every existing schema round-trips unchanged.

## Decision criterion (do NOT let this become a default escape hatch)

Mark `gateway_tolerant` **only** when the SDK deliberately keeps a field stricter than the gateway
as a genuine caller contract (the caller SHOULD send it; the gateway merely tolerates its absence).
When the field is *not* a real caller contract, correct the schema to `required: false` instead —
do not paper over a mis-declared field with the facet, or the differential probe loses its
drift-detection value on that field.

## Known limitation — cross-field pseudo-fields

The facet is per-field: `is_gateway_tolerant` matches a real schema field, so a
**cross-field** variant (whose `field` is a pseudo-name like `"sdate/edate"`) can
never be marked tolerant. If a gateway that tolerates a field-level violation also
accepts the cross-field variant (e.g. ignoring malformed dates → ignoring their
ordering), that cross-field result stays `Divergent` with no way to downgrade it.
This is deferred by design: the plan's rule is to handle a *newly-observed* tolerant
pair when the live probe surfaces it, not to pre-mark speculatively. A validated
class-consistency test (`gateway_tolerant_classes_are_real_generatable_classes`)
guards the per-field case; extend the facet to cross-field rules only when a live
probe actually exhibits the case.

## Certification-claim scope

A Recommended claim resting on a `gateway_tolerant` pair rests on the **preflight-enforced caller
contract** (offline-tested) plus crisp differential certification of the field's *other* classes.
It does **not** claim gateway-side enforcement of the tolerant `(field, class)` pair — the gateway
does not enforce it. Every split-facet promotion must carry that exclusion in its recommendation
text; no promotion may imply the gateway enforces a tolerant pair.

## References

- `crates/ls-core/src/preflight.rs` — the `FieldConstraint::gateway_tolerant` field; preflight is
  provably unchanged (`gateway_tolerant_does_not_weaken_preflight`).
- `crates/ls-sdk/tests/negative_probe.rs` — `is_gateway_tolerant` / `reported_outcome` +
  `gateway_tolerant_downgrade_fires_only_on_marked_class`.
- `metadata/constraints/{t1102,t8412,t0425}.yaml` — the live-observed tolerant pairs.
- Plan `docs/plans/2026-07-06-002-feat-recert-wave-reopen-held-trs-plan.md` (KTD2–KTD5).
