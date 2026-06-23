---
title: "Model an SDK struct's canonical field from the baseline korean_name, not field-name proximity — and never trust a single-instrument fixture"
date: 2026-06-23
category: conventions
module: ls-sdk TR response modeling (crates/ls-sdk)
problem_type: convention
component: tooling
severity: high
applies_when:
  - "Authoring a representative-subset response struct for a TR from its normalized baseline"
  - "Choosing which of several similarly-named numeric fields is the canonical value (e.g. a current price/index)"
  - "Building an offline deserialize fixture from one captured instrument (one symbol, one sector)"
tags:
  - ls-sdk
  - response-modeling
  - normalized-baseline
  - representative-subset
  - test-fixture
  - implement-tr
related_components:
  - tooling
---

## Context

LS TR response blocks carry many numeric fields with terse, similar English names.
The SDK models a *representative subset* of them as a hand-written struct, so the
author must pick which fields are the canonical ones a caller wants. Field names
alone are misleading: `t1511` (업종현재가) exposes both `pricejisu` and `firstjisu`,
and the obvious-looking `firstjisu` ("first index") is **not** the current index.

During Wave A this was a P1 caught only in Tier-2 review: `T1511OutBlock` modeled
`firstjisu` with the doc comment "Current index," and omitted `pricejisu` entirely.
The captured fixture was KOSPI composite (`upcode=001`), where `firstjisu` and
`pricejisu` happen to carry the *same* value (`2610.62`) — so every offline test and
the live smoke passed while the struct exposed the wrong field as the current index.

## Guidance

1. **Resolve the canonical field from the baseline `korean_name`, not the English
   field name.** Open `crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`
   and read the `korean_name` per field. For `t1511`: `pricejisu` = **현재지수**
   (current index, field_index 2); `firstjisu` = **첫번째지수** (first comparison
   *sub*-index, field_index 38). Model `pricejisu` as the current index; if you keep
   `firstjisu`, label it as the sub-index it is.
2. **Never build an offline fixture from a single instrument and assume it
   generalizes.** A one-symbol / one-sector capture collapses fields that diverge for
   other instruments. KOSPI composite makes `firstjisu == pricejisu`; a non-composite
   sector separates them. The coincidence hides a wrong-field bug from every assertion.
3. **Assert the canonical field's exact value, not just `!is_empty()`.** A presence
   check passes on the wrong field. Pin the value (`assert_eq!(resp.outblock.pricejisu,
   "2610.62", ...)`) so a future rename or a string_or_number regression fails loudly.
4. The representative-subset pattern (dropping fields you don't model) is fine — but
   the *canonical* field must be in the subset and correctly named. Dropping
   `pricejisu` is the bug; dropping a genuinely secondary field is not.

## Why This Matters

A mislabeled canonical field is a silent wrong-value contract: callers reading
`outblock.firstjisu` for "the current index" get a meaningful value for KOSPI
composite and a different sub-index for every other sector — no panic, no test
failure, no smoke failure. The live smoke even *records* the wrong field as evidence,
cementing the mistake as the public API. Field-name proximity ("first" sounds
primary) and a single-instrument fixture conspire to make the bug invisible until a
human or a second reviewer reads the baseline's Korean names.

## When to Apply

- Every time you author or review a TR response struct from the baseline — especially
  index/quote TRs with multiple `*jisu` / price-like fields.
- When the only available capture is one instrument: either capture a second,
  structurally-different instrument, or add a `// fixture is KOSPI composite where
  firstjisu==pricejisu` note so the next author knows the fixture under-tests divergence.
- During code review of any new SDK surface: cross-check the modeled fields' meaning
  against the baseline `korean_name`, not the English identifier.

## Examples

Before (the P1 — wrong field labeled as current index, canonical field omitted):

```rust
pub struct T1511OutBlock {
    pub hname: String,
    /// Current index / 지수 (firstjisu in the full spec).   // WRONG: 첫번째지수 is a sub-index
    pub firstjisu: String,
    // pricejisu (현재지수) — the real current index — is missing
}
```

After (canonical field from the baseline korean_name, sub-index relabeled):

```rust
pub struct T1511OutBlock {
    pub hname: String,
    /// Current index / 현재지수 — the canonical composite index value.
    pub pricejisu: String,
    /// First comparison sub-index / 첫번째지수 (distinct from `pricejisu`; for
    /// KOSPI composite the two coincide, but they diverge for other sectors).
    pub firstjisu: String,
}
```

Test that would have caught it (pin the value, not just presence):

```rust
assert_eq!(resp.outblock.pricejisu, "2610.62", "현재지수 current index");
```

See also `docs/solutions/architecture-patterns/ls-sdk-pagination-modeling.md` (the
sibling ls-sdk modeling pattern),
`docs/solutions/conventions/market-hours-read-empty-result-disposition.md`, and
`docs/solutions/conventions/tr-out-block-shape-from-raw-capture.md` (the sibling
out-block *container* convention: this doc picks the right field, that one picks
the right block key + array-ness — the normalized baseline is reliable for a
field's `korean_name` but lossy for out-block name/shape).
