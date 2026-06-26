---
module: ls-core
date: 2026-06-26
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Authoring a new {TR}_POLICY const in crates/ls-core/src/endpoint_policy.rs"
  - "Mirroring an existing sibling policy const to implement a new TR"
related_components:
  - ls-trackers
tags:
  - endpoint-policy
  - rate-limit
  - implement-tr
  - baseline
  - silent-drift
---

# Re-pin EndpointPolicy rate limits from the TR's own baseline, never the mirror exemplar

## Context

The `implement-tr` recipe authors a new `{TR}_POLICY` const by mirroring an
existing sibling read (e.g. copy `T8431_POLICY`, rename to `T8430_POLICY`, adjust
`tr_code`/`path`/`module`/`group`). The numeric fields `rate_limit_per_sec` and
`corp_rate_limit_per_sec` ride along in the copy and are easy to leave at the
sibling's values.

During the t8430 flip, `corp_rate_limit_per_sec: Some(3)` was copied verbatim from
the adjacent `T8431_POLICY`, but t8430's own normalized baseline specifies `5`.
The mistake shipped a green gate and was caught only by `ce-code-review`.

## Guidance

When authoring or editing a `{TR}_POLICY`, pin **both** rate-limit fields from the
TR's **own** normalized baseline, not the const you mirrored:

```
crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json
  -> "rate_limit_per_sec"        => EndpointPolicy.rate_limit_per_sec: Some(n)
  -> "corp_rate_limit_per_sec"   => EndpointPolicy.corp_rate_limit_per_sec: Some(n)
```

Quick check before committing a new/edited policy const:

```bash
python3 -c "import json; d=json.load(open('crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json')); print(d['rate_limit_per_sec'], d['corp_rate_limit_per_sec'])"
```

and confirm the two numbers match the `Some(...)` values in the const.

## Why This Matters

`crates/ls-core/tests/policy_index_crosscheck.rs` (`slice_policies_mirror_metadata_index`)
validates only **protocol**, **rate-category bucket**, and **pagination** against
the metadata index — it does **not** compare the numeric rate-limit values. So a
wrong `rate_limit_per_sec`/`corp_rate_limit_per_sec` is **silent**: the workspace
gate (`cargo test` + `make docs-check`) stays green with the wrong throttle.

A too-low corp limit under-throttles corp-tier callers against the published spec
(not a hard outage, but a spec divergence); a too-high limit could trip gateway
rate limiting. Either way the const no longer mirrors the wire contract the rest
of the toolchain treats as source of truth.

## When to Apply

Every time a `{TR}_POLICY` const is created or edited — most acutely when the const
was produced by copying a sibling. Sibling policies in the same endpoint group
(e.g. `[주식] 기타` / `[주식] ELW`) frequently have **different** corp limits, so
proximity is not a safe proxy.

## Examples

Wrong — copied from the `T8431` sibling (corp `3`):

```rust
pub const T8430_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8430",
    // ...
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3), // t8431's value, not t8430's
};
```

Right — pinned from `normalized/trs/t8430.json` (`rate_limit_per_sec: 2`,
`corp_rate_limit_per_sec: 5`):

```rust
pub const T8430_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8430",
    // ...
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};
```

Prevention idea (not yet implemented): extend `slice_policies_mirror_metadata_index`
to assert the policy's numeric rate limits equal the baseline's, closing the silent
gap so the gate — not a reviewer — catches the drift.
