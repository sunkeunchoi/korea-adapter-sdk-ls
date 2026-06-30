---
module: ls-metadata
date: 2026-06-30
problem_type: convention
component: tooling
severity: low
applies_when:
  - "Comparing two values of a typed enum (owner_class, protocol, instrument_domain, venue_session, support tier, etc.) for equality"
  - "Building a ValidationError / mismatch report that needs a human-readable rendering of both sides"
  - "Reviewing or simplifying code that does format!(\"{:?}\", a) != format!(\"{:?}\", b)"
related_components:
  - ls-core
tags:
  - rust
  - enums
  - validation
  - simplification
---

# Compare typed enums with PartialEq, not Debug-string formatting

## Context

`check_routing` in `crates/ls-metadata/src/validator.rs` decided whether an
index entry's routing facets matched its per-TR record by comparing the two
sides as `Debug` strings:

```rust
let checks: [(&'static str, String, String); 4] = [
    ("owner_class", format!("{:?}", entry.owner_class), format!("{:?}", meta.owner_class)),
    // protocol, instrument_domain, venue_session ...
];
for (field, index_value, file_value) in checks {
    if index_value != file_value { errors.push(RoutingMismatch { .. }); }
}
```

The four facets (`OwnerClass`, `Protocol`, `InstrumentDomain`, `VenueSession`)
are fieldless enums that already derive `PartialEq, Eq, Copy, Debug`.

## Guidance

Compare the enum values directly with `==` / `!=`. Render the `Debug` strings
only on the mismatch path, where they are actually needed for the error
message. A small generic helper removes the four-way repetition:

```rust
fn check<T: PartialEq + std::fmt::Debug>(
    code: &str,
    field: &'static str,
    index: T,
    file: T,
    errors: &mut Vec<ValidationError>,
) {
    if index != file {
        errors.push(ValidationError::RoutingMismatch {
            tr_code: code.to_string(),
            field,
            index_value: format!("{index:?}"),
            file_value: format!("{file:?}"),
        });
    }
}

check(tr_code, "owner_class", entry.owner_class, meta.owner_class, errors);
// protocol, instrument_domain, venue_session ...
```

The enums are `Copy`, so passing them by value into the generic helper copies
out of the `&IndexEntry` / `&TrMetadata` borrows without consuming them.

## Why This Matters

- **Correctness contract.** `format!("{:?}", a) != format!("{:?}", b)` is only
  equivalent to `a != b` when `Debug` is *injective* across variants. That holds
  for today's fieldless enums, but it is an unstated assumption the type system
  does not guard: add a variant (or a `#[derive]`/manual `Debug` that renders two
  values the same) and the string compare silently misfires while `PartialEq`
  stays correct by construction. Compare the values you mean to compare.
- **No hot-path allocation.** The string form built eight `String`s per TR on
  every validation run (~320 TRs through `cargo test -p ls-core`), purely to do
  an equality test. The `PartialEq` form allocates only when a mismatch is
  actually being reported.
- **The error message is unchanged.** `index_value` / `file_value` are still the
  `{:?}` renderings, so `ValidationError::RoutingMismatch` and every test/
  cross-check that matches on it are unaffected.

## When to Apply

Any time code stringifies two typed values solely to compare them — enums,
newtypes, or anything deriving `PartialEq`. The Debug (or `Display`) rendering
is for *reporting* the difference, not for *detecting* it. If you find yourself
writing `format!(..) == format!(..)`, reach for the real `==` first and defer
formatting to the branch that needs the text.
