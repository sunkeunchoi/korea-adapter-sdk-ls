---
title: "The `gateway_tolerant` constraint facet: unblock the differential probe on a field the gateway does not enforce, WITHOUT weakening preflight"
date: 2026-07-06
category: conventions
module: "ls-core preflight (crates/ls-core/src/preflight.rs), ls-metadata schema, ls-sdk negative_probe"
problem_type: convention
tags: [constraint-schema, preflight, negative-probe, differential, gateway_tolerant, re-cert, split-facet, cross-field]
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

## Cross-field extension (§30, PR #135) — same downgrade, keyed on the rule not a field

The facet started per-field only: `is_gateway_tolerant` matched a real schema field, so a
**cross-field** variant (whose `field` is a pseudo-name like `"sdate/edate"`) could never be
marked tolerant. The §30 t8412 live re-probe (once paced past its `IGW00201` throttle) exhibited
exactly the deferred case — the gateway accepts a start>end `date_order` violation (`rsp_cd 00000`,
defaulting the empty range), so `sdate/edate` read `Divergent` with no downgrade path. That was the
sole reason t8412 stayed HELD after its `nday/required` divergence was resolved.

The extension mirrors the per-field mechanism exactly, keyed on the **rule** instead of a `(field,
class)` pair:

- `CrossFieldRule::DateOrder` gains a `gateway_tolerant: bool` (`#[serde(default)]`), mirrored in
  the ls-core `preflight` copy **and** the ls-metadata `schema` copy (the two are deliberately
  duplicated — ls-core cannot depend on ls-metadata at runtime).
- `is_gateway_tolerant` falls through to the `cross_field` rules **only** for `class ==
  "cross_field"`: it reconstructs each rule's pseudo-field as `format!("{start}/{end}")` and
  matches the *same* string `generate_invalid_variants` emits, so a marked rule downgrades **only**
  its own accepted violation — never another rule, and never either endpoint's own
  `required`/`format` class (a negative anchor test pins this).
- Preflight is still untouched: `validate_cross_field` binds `gateway_tolerant: _` and ignores it,
  so a `confirmed` ordering still blocks locally regardless of tolerance. Tolerance is probe-side
  only — identical invariant to the per-field case.
- Docgen renders the note in the "Errors & validation" section (a `gateway-tolerant (preflight
  enforces; gateway does not)` clause on the cross-field line), keeping the audit surface.

The decision criterion above still governs: mark a cross-field rule tolerant **only** when a live
probe actually exhibits the accepted violation (t8412 did in §30), never speculatively. A field
whose *own* class is tolerant does not imply its cross-field rule is — the §30 probe confirmed the
ordering pair independently. **Promotion still requires a live in-window re-probe** confirming the
downgrade reads `expected-tolerant` on merits; the offline mechanism alone does not promote (t8412
remained HELD after PR #135 pending that re-probe).

## Certification-claim scope

A Recommended claim resting on a `gateway_tolerant` pair rests on the **preflight-enforced caller
contract** (offline-tested) plus crisp differential certification of the field's *other* classes.
It does **not** claim gateway-side enforcement of the tolerant `(field, class)` pair — the gateway
does not enforce it. Every split-facet promotion must carry that exclusion in its recommendation
text; no promotion may imply the gateway enforces a tolerant pair.

## References

- `crates/ls-core/src/preflight.rs` — the `FieldConstraint::gateway_tolerant` field and the
  `CrossFieldRule::DateOrder::gateway_tolerant` bool; preflight is provably unchanged
  (`gateway_tolerant_does_not_weaken_preflight`) and the cross-field flag round-trips from embedded
  metadata (`cross_field_gateway_tolerant_flag_round_trips_from_embedded_t8412`).
- `crates/ls-sdk/tests/negative_probe.rs` — `is_gateway_tolerant` / `reported_outcome` +
  `gateway_tolerant_downgrade_fires_only_on_marked_class` (covers both the per-field pairs and the
  cross-field rule, with an unmarked negative anchor).
- `crates/ls-metadata/src/schema.rs` — the mirrored `CrossFieldRule::DateOrder::gateway_tolerant`.
- `metadata/constraints/{t1102,t8412,t0425}.yaml` — the live-observed tolerant pairs; t8412's
  `sdate/edate` cross-field rule carries the cross-field flag (§30).
- Plan `docs/plans/2026-07-06-002-feat-recert-wave-reopen-held-trs-plan.md` (KTD2–KTD5); §30
  follow-up shipped in PR #135 (cross-field extension + the paper-reset `01458` messaging polish).
