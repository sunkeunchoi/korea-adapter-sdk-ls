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

As of 2026-07-08 the sibling test `slice_rest_rate_pins_mirror_official_quota_baselines`
(same file) now compares **both** numeric rate fields of every REST policy against
its own normalized baseline and fails `cargo test -p ls-core` on any disagreement —
so this class of drift is caught by the gate, not a reviewer. (The reconciliation
pass that landed the test found **no** live mismatches: the historical t8430 corp
`3`-vs-`5` divergence had already been fixed, and pin and baseline both read `5`
today.) The manual quick-check below remains a useful pre-commit sanity step, but
the gate is now the backstop.

Historically, `slice_policies_mirror_metadata_index` validated only **protocol**,
**rate-category bucket**, and **pagination** against the metadata index — it did
**not** compare the numeric rate-limit values, so a wrong
`rate_limit_per_sec`/`corp_rate_limit_per_sec` was **silent**: the workspace gate
(`cargo test` + `make docs-check`) stayed green with the wrong throttle. That gap
is what the new sibling test closes.

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

Prevention idea (**implemented 2026-07-08**, KTD-1): the sibling test
`slice_rest_rate_pins_mirror_official_quota_baselines` asserts each REST policy's
numeric rate limits equal its baseline's (both ways — a `Some` pin against a `null`
baseline is also a failure), closing the silent gap so the gate — not a reviewer —
catches the drift. WebSocket policies (no gateway REST rate contract) are skipped;
a REST policy whose baseline file is missing fails loudly.
