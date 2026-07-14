---
title: "adapters/nautilus is a standalone cargo workspace outside the root gate — SDK-side preflight/dispatch changes silently redden it"
date: 2026-07-04
category: workflow-issues
module: "adapters/nautilus (standalone cargo workspace) vs root gate (cargo test, cargo test -p ls-core) and crates/ls-core preflight seam / metadata/constraints"
problem_type: workflow_issue
component: testing_framework
applies_when:
  - "Changing the SDK's shared dispatch seam — order/preflight logic, constraint schemas (metadata/constraints/), or the Inner::post pipeline"
  - "adapters/nautilus is a standalone cargo workspace outside crates/ — root `cargo test` structurally cannot build or run it"
  - "A change looks safe because the root gate (make docs, cargo test, cargo test -p ls-core, make docs-check, make lane-check) is green"
symptoms:
  - "PR #92's constraint-schema preflight (runs at Inner dispatch for orders too) marked CSPAT00601.MbrNo required:true, but the adapter's KRX submit sends MbrNo empty (member-routing field, default venue) so preflight false-rejected every adapter order client-side"
  - "14/24 adapters/nautilus execution_client tests went red, but the root gate stayed green and never surfaced it — invisible until the next adapter-workspace run"
  - "Second occurrence of the class: LoanDt hit the same failure mode earlier in the same wave and was caught in-wave, unlike the MbrNo case"
root_cause: missing_workflow_step
resolution_type: workflow_improvement
severity: high
related_components:
  - "orders"
  - "tooling"
tags:
  - "adapters-nautilus"
  - "cargo-workspace"
  - "cross-workspace-gate"
  - "preflight"
  - "constraint-schema"
  - "root-gate-blind-spot"
  - "execution-client"
  - "cargo-test-workspace"
---

# adapters/nautilus is a standalone cargo workspace outside the root gate — SDK-side preflight/dispatch changes silently redden it

## Context

`adapters/nautilus` is a **standalone Cargo workspace** — it has its own
`Cargo.toml` and is not a member of the root workspace. The root gate
(`make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`,
`make lane-check`) runs entirely inside the root workspace and structurally
cannot see the adapter crate at all. A change at the shared SDK dispatch seam
can therefore pass every root-gate check while silently redding the adapter,
and nothing in the normal commit flow surfaces it until someone happens to run
the adapter's own gate.

This happened concretely in PR #92 (the recommended re-certification wave).
That PR added per-TR request field-constraint schemas
(`metadata/constraints/*.yaml`) enforced as a **preflight** check at the single
shared dispatch seam in `crates/ls-core/src/inner.rs`, covering `post`,
`post_paginated`, **and `post_order`**. `metadata/constraints/CSPAT00601.yaml`
originally marked `MbrNo` `required: true` (matching the normalized baseline —
"회원사번호", member-routing code, e.g. `"NXT"` for Nextrade). But the adapter's
own order-submission path, `submit_request` in
`adapters/nautilus/src/execution.rs` (line 243):

```rust
// adapters/nautilus/src/execution.rs
let req = CSPAT00601Request::limit(isuno, qty.to_string(), price.to_string(), side, "");
//                                                                                   ^^ MbrNo = "" (default-venue KRX routing)
```

sends `MbrNo` empty (empty = default KRX venue; the adapter never routes to
Nextrade). With `required: true`, ls-core's preflight rejected every adapter
order **client-side, in 2.9ms, before any HTTP call** — 14 of 24
`execution_client` tests went red. This was only discovered when the adapter's
own gate happened to run in the next session; the root gate for PR #92 stayed
green throughout.

The wave itself had already hit and fixed the identical class for
`CSPAT00601.LoanDt` (a credit-order-only field the certified `limit()`
constructor also sends empty) and for `t0425.expcode`/`t0425.cts_ordno` — see
`docs/solutions/conventions/constraint-schema-required-false-when-certified-struct-sends-empty.md`.
It missed `MbrNo` specifically because the certified SDK-side smoke chain that
grounded the wave's schemas calls `limit()` with `mbrno = "NXT"` (real
Nextrade routing), while the adapter's own call site uses `""`. Auditing "does
any certified caller send this field empty" against the *SDK's* smoke usage
alone was not sufficient — the adapter is a second, independent caller of the
same request struct with different field usage.

## Guidance

1. **Whenever a change touches the SDK's dispatch/preflight seam
   (`crates/ls-core/src/inner.rs`) or `metadata/constraints/`, also run**
   `cd adapters/nautilus && cargo test --workspace` **before considering the
   change gate-clean.** `--workspace` is mandatory — a plain `cargo test`
   inside `adapters/nautilus` misses the `lab` member crate.
2. **Longer-term, fold the adapter gate into the root gate or CI** so this
   class of cross-cutting break can't reach a merged PR silently. Until that
   exists, the adapter gate is an opt-in step that must be remembered, not an
   enforced one.
3. **When relaxing (or tightening) a constraint schema's `required` flag,
   audit *every* in-repo caller of that TR's request struct**, not just the
   certified smoke chain that motivated the schema. Grep the constructor
   (e.g. `CSPAT00601Request::limit(` or `::new(`) across the whole repo,
   including `adapters/`, and check what each call site actually sends for
   the field in question. The fix for `MbrNo` mirrors the existing `LoanDt`
   pattern — see the corrected schema entry in
   `metadata/constraints/CSPAT00601.yaml`:

```yaml
  - name: MbrNo
    type: string          # baseline: String, required on the wire (회원사번호,
    required: false        # e.g. "NXT" = Nextrade routing) — but a plain KRX
    enum: { applicable: false }   # order sends it EMPTY (the nautilus adapter's
    range: { applicable: false }  # submit path does; default-venue routing), so
    format: { applicable: false } # caller-optional like LoanDt: the struct wins
                                  # on disagreement (permissive direction).
```

## Why This Matters

The blind spot converts a fully green root gate into false confidence: a
client-side preflight change is exactly the kind of cross-cutting edit that
can pass every SDK-side test while breaking every adapter order, because the
adapter's `cargo test` simply never ran as part of the change's validation.
The diagnosis cost was real — the red baseline surfaced mid-cycle, in a later
session, and took a dedicated probe test to localize the failure down to the
single `MbrNo` field among 24 failing `execution_client` tests. Because
grounding only checks the constraint schema against the baseline in the
*permissive* direction (a schema may declare a baseline-required field
caller-optional, never the reverse), an over-strict `required: true` passes
grounding cleanly and only ever surfaces as a runtime false-rejection — never
a compile-time or gate-time failure at authoring time. That makes this
specific class of bug systematically likely to slip past whoever authors the
schema, especially when the schema author is reasoning from the SDK's own
smoke chain and has no visibility into `adapters/`.

## When to Apply

- Any PR that edits `crates/ls-core/src/inner.rs` (dispatch, preflight, rate
  limiting, pagination), `metadata/constraints/*.yaml`, or any `ls-sdk` request
  struct that `adapters/nautilus` also constructs directly.
- Any PR that adds a *new* constraint schema for a TR that has an adapter-side
  caller — check `adapters/nautilus/src/**` for direct constructor usage
  before merging, not just the SDK's own tests/smokes.
- Post-merge triage: if `adapters/nautilus`'s own gate goes red with no local
  edits in `adapters/`, suspect a preflight/constraint-schema change from the
  root repo first.

**Now automated (PR #143, commit `0dbd522`).** The blind spot is no longer
detected only by manual triage: `make adapter-check`
(`cd adapters/nautilus && cargo test --workspace`) is a documented root-gate
step (see AGENTS.md "Gate"), and `.github/workflows/adapter-check.yml` runs the
full unfiltered adapter workspace test on every push/PR. Guidance #2 above
("longer-term, fold the adapter gate into the root gate or CI") is now done. The
manual triage above is a backstop for when the gate is skipped locally, not the
primary detector.

## Examples

- **`CSPAT00601.MbrNo`** (this case) — `metadata/constraints/CSPAT00601.yaml`,
  fixed to `required: false` with a struct-wins comment, as shown above.
- **`CSPAT00601.LoanDt`** (the precedent one entry above it in the same file)
  — the certified `CSPAT00601Request::limit()` constructor sends `LoanDt`
  empty for every cash order (loan date only applies to credit orders); see
  `docs/solutions/conventions/constraint-schema-required-false-when-certified-struct-sends-empty.md`
  for the original writeup and the `t0425.expcode`/`t0425.cts_ordno` siblings.
