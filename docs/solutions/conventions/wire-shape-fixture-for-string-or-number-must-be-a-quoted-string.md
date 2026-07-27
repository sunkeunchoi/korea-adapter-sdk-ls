---
title: "A wire-shape fixture for a `string_or_number` field must be a QUOTED string — an unquoted JSON number normalizes away the very shape under test"
date: 2026-07-27
category: conventions
module: "crates/ls-core (lib.rs: string_or_number), any offline wiremock test asserting a response field's parse behavior"
problem_type: convention
severity: medium
applies_when:
  - "Writing an offline wiremock fixture that must exercise a NON-INTEGER response value"
  - "A test asserting a value lands in an 'unparseable' / wire-shape-fault bucket passes when it should fail"
  - "Reasoning about what a gateway number-vs-string drift actually looks like to a consumer"
related_components:
  - ls-core
  - ls-sdk
  - nautilus-ls-lab
tags:
  - string-or-number
  - wiremock
  - test-fixture
  - wire-shape
  - serde
  - silent-pass
---

## Context

Every numeric-bearing response field in the SDK deserializes through
`ls_core::string_or_number`, which tolerates the gateway sending either `"57900"` or `57900`.
A consumer that then does `raw.parse::<i64>()` usually classifies a parse failure as a
**wire-shape change** rather than market state.

Testing that classifier means writing a fixture whose `open` is not an integer. The obvious
move — `json!({ "open": 57900.0 })` — does not do that.

`string_or_number`'s float arm is:

```rust
fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> { Ok(value.to_string()) }
```

Rust's `f64::to_string()` drops a zero fraction: `57900.0` renders as `"57900"`. `serde_json`
writes `json!(57900.0)` as the JSON number `57900.0`, serde hands it to `visit_f64`, and the
consumer receives `"57900"` — a perfectly good integer. The test passes for the wrong reason:
the value under test never existed.

## Guidance

**To exercise a non-integer wire value, write the fixture as a quoted string.** `visit_str`
passes it through verbatim, so `"57900.0"` reaches the consumer as `"57900.0"` and fails
`parse::<i64>()` exactly as intended.

```rust
// WRONG — visit_f64 + f64::to_string() normalize this to "57900"; the unparseable
// arm is never reached and the test green-lights code that may be broken.
json!({ "shcode": "005930", "open": 57900.0 })

// RIGHT — visit_str passes the text through; the consumer sees "57900.0".
json!({ "shcode": "005930", "open": "57900.0" })
```

The reverse direction is worth a fixture too: an unquoted JSON number is exactly how a
number-typed echo of a zero-padded code loses its zeros (`5930`, not `"005930"`), which is why
consumers re-pad with `{:0>6}` before keying. Cover both shapes rather than assuming one stands
in for the other.

## Why This Matters

This is a **silent-pass** failure: the test is green, the coverage report counts the branch, and
nothing indicates the assertion never ran against the shape it names. On a path like the
`lab-mount-universe` live `today_open` fetch — whose output IS what an attended live session
buys — a "we tested the wire-shape guard" belief that was never actually tested is worse than no
test, because it stops anyone from looking again.

Note the precise boundary, since it is not "floats are safe": only a **zero-fraction** float
normalizes. `57900.5` renders as `"57900.5"` and does fail to parse. Relying on that distinction
in a fixture is needlessly subtle — use a quoted string and the intent is explicit.

## When to Apply

- Authoring any offline test for a consumer that parses a `string_or_number` field
- Reviewing a test whose fixture uses an unquoted JSON number to represent a "bad" value
- Diagnosing a wire-shape classifier that never seems to fire in tests but does in production

## Examples

Both shapes pinned side by side, in
`adapters/nautilus/lab/src/runner/mount_universe.rs` (`tests::live_open_fetch`):

```rust
// The unparseable arm — a decimal-bearing STRING is what actually reaches it.
async fn a_decimal_open_string_is_reported_as_unparseable_not_as_pre_open() { /* "57900.0" */ }

// The normalization itself, pinned so a change to the helper cannot silently move
// the boundary between "trades" and "reported as a wire-shape fault".
async fn an_unquoted_json_float_open_normalizes_to_an_integer_and_still_resolves() { /* 57900.0 */ }
```

## Related

- [`never-re-check-rsp-cd-in-a-consumer-dispatch-already-classified-it`](never-re-check-rsp-cd-in-a-consumer-dispatch-already-classified-it.md)
  — the other half of the same response-handling surface on this path
- [`ls-gateway-igw40011-numeric-request-fields`](../integration-issues/ls-gateway-igw40011-numeric-request-fields.md)
  — the request-side counterpart: `string_as_number` must emit a JSON number
