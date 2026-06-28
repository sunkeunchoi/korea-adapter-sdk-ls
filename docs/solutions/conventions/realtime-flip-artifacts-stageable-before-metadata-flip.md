---
title: "Realtime flip artifacts (crosscheck-only policy + push-row struct) can be staged while the TR stays implemented:false — the gate stays green"
date: 2026-06-28
category: conventions
module: crates/ls-core, crates/ls-sdk, crates/ls-docgen, metadata, .agents/skills
problem_type: convention
component: tooling
severity: medium
applies_when: "Staging the implement-realtime-tr offline artifacts (push-row struct + {TR}_POLICY + crosscheck registration + sweep smoke) ahead of an operator-gated live lifecycle smoke, so the live flip is reduced to running the smoke and flipping metadata"
related_components: "policy_index_crosscheck, endpoint_policy, TRACKED_TRS, reference.len, banner_trs"
tags:
  - realtime
  - websocket
  - endpoint-policy
  - crosscheck
  - tracked-rung
  - connection-reachable-only
  - operator-gated
---

## Context

`implement-realtime-tr` couples four things into one flip: a push-row struct, a
`{TR}_POLICY` const, its crosscheck registration, and the metadata
`support.implemented: true`. But the realtime flip's gate is a **live paper
lifecycle smoke** (`make live-smoke-ws-*`, connection-reachable-only per KTD6) —
which an autonomous run cannot execute (no `.env`/session). The question that
arises when pre-staging the offline half of the flip ahead of an operator running
the smoke: **can a `{TR}_POLICY` exist and be registered while the TR's metadata
still says `implemented: false`, without turning the gate red?**

Yes. This decouples the *offline modeling* (structs + policies + smoke) from the
*metadata flip* (+ `reference.len`/`banner_trs` bump), so an autonomous run can
complete everything except the live smoke and leave the operator a one-step flip.

## Guidance

A registered policy for a Tracked-only (`implemented: false`) TR keeps the gate
green. When staging realtime flip artifacts ahead of the live smoke:

- **Track the TR first** (`track-realtime-tr`): metadata at `tracked: true,
  implemented: false`, projected baseline, and the **Tracked-rung** count bumps
  only — `maintained_tr_count`, `TRACKED_TRS`, the four `cli.rs` literals.
- **Stage the struct + policy + crosscheck registration** without flipping
  metadata. Register `{TR}_POLICY` in the `policy_index_crosscheck.rs` `policies`
  array **and** add it to that test's `use ls_core::endpoint_policy::{...}` import
  (a missing import is the only thing that breaks — `error[E0425]: cannot find
  value`, not a logic failure).
- **Do NOT bump `reference.len()` / `banner_trs`** — those are the Implemented-rung
  surfaces and the docgen reference page is keyed on `implemented: true`. Leave
  them for the operator to bump per channel **after** the live smoke flips
  metadata.
- **Const naming for trailing-underscore codes:** the const is `{CODE}POLICY`
  (`BM_` -> `BM_POLICY`, `S3_` -> `S3_POLICY`), because the code already ends in
  `_`; non-underscore codes get `{CODE}_POLICY` (`NS3` -> `NS3_POLICY`).

## Why This Matters

`slice_policies_mirror_metadata_index` only requires that each registered
policy's `tr_code` is present in `tr-index.yaml` + metadata, with matching
`protocol` / `rate_bucket` / `has_pagination`. It does **not** require
`implemented: true`. There is no orphan-policy completeness test (no assertion
that every `_POLICY` const appears in the crosscheck, nor that every registered
policy maps to an Implemented TR), and the crates carry no `#![deny(warnings)]`,
so an unused `pub const` policy and `pub` struct raise no dead-code error. The
net effect: the full surface area of a flip can exist on a branch with the
metadata still at the Tracked rung, gate fully green, waiting only for the
live-smoke flip.

This is what lets a closure wave ship "track + stage the flip" as gate-green work
and hand the operator a checklist (run smoke -> flip metadata -> bump
`reference.len`/`banner_trs` -> flip smoke-map Promotion column -> `make docs`)
instead of authoring code under the live window.

## When to Apply

- WebSocket realtime flips, where the Implemented gate is a live lifecycle smoke
  the autonomous run can't execute (connection-reachable-only, KTD6
  NOT-OBSERVABLE).
- Any flip whose final step is environment-gated (a live paper smoke / probe) but
  whose struct/policy/test modeling is fully determinable offline from the raw
  capture.
- NOT for the count surfaces: `reference.len`/`banner_trs` must move only when
  `implemented` actually flips, or the docgen gate flaps.

## Examples

Closure-flip WS batch (plan -004), branch `feat/closure-flip-ws-batch`: 31
channels tracked to Tracked, then 31 `<Code>Row` structs + 31 crosscheck-only
`{TR}_POLICY` + a combined sweep smoke `live_smoke_ws_p3` staged — all on
`implemented: false` metadata. `cargo test` green (1034 passed),
`cargo test -p ls-core` green (the crosscheck mirror-metadata assertion passed
with 31 policies pointing at Tracked-only TRs), `make docs-check` green
(`reference.len` untouched at 182). The operator runs `make live-smoke-ws-p3`,
then flips metadata + bumps `reference.len`/`banner_trs` per reachable channel.

Two `use`-site gotcha — the crosscheck consts must be added in **both** places:

```rust
// crates/ls-core/tests/policy_index_crosscheck.rs
use ls_core::endpoint_policy::{
    // ...
    NS3_POLICY, NH1_POLICY, /* ... */ BM_POLICY, MK2_POLICY,  // (1) import
};
// ...
let policies: &[EndpointPolicy] = &[
    // ...
    NS3_POLICY, NH1_POLICY, /* ... */ BM_POLICY, MK2_POLICY,  // (2) array
];
```

Related: [[implement-tr-registration-sites]] is the full per-rung count/crosscheck
checklist; this doc adds that the **implement-rung** artifacts can be registered
while metadata is still at the Tracked rung. See also
[[api-drift-renormalize-preserves-refreshed-date]] (revert `manifest.refreshed`
after renormalize) and [[tr-out-block-shape-from-raw-capture]] (modelling the
push-row struct from the raw `res_example`).
