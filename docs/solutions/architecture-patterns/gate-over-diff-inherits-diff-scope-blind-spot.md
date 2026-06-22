---
title: "A gate over a diff inherits the diff's scope: pair findings checks with an explicit set-membership check"
date: 2026-06-22
category: architecture-patterns
module: crates/ls-trackers
problem_type: architecture_pattern
component: tooling
severity: high
applies_when:
  - "Building a gate or guard on top of an existing diff/compare function"
  - "A diff compares only the intersection of two sets (committed vs staged)"
  - "A downstream step writes a value the gate never evaluated"
  - "Adding an opt-in or narrowly-scoped promote/admit path"
  - "Re-derivation validation reuses the same metadata on both sides"
tags:
  - ls-trackers
  - api-drift
  - gate-design
  - diff-scope
  - set-membership
  - baseline-integrity
  - guardrail
  - adversarial-review
---

# A gate over a diff inherits the diff's scope: pair findings checks with an explicit set-membership check

## Context

In the api-drift tracker, `compare(committed, staged, trs)` produces per-TR structural drift findings, and the opt-in `--type-only` promote gate (`type_only_gate`) reasons over those findings to decide whether a promotion may advance the committed Reviewed Baseline. The gate is meant to admit **only** pure field-`type` drift (plus benign `DescriptionChanged`) on the maintained surface and refuse everything else — the guardrail for a clean, type-scoped baseline refresh.

The structural-diff stage of `compare` iterates the committed baseline's shapes and skips any code not also present in the staged run:

```rust
// crates/ls-trackers/src/api_drift.rs — compare(), structural diff
for (code, base_shape) in &committed.shapes {
    let Some(cand_shape) = staged.shapes.get(code) else {
        continue; // absence is handled by the code-set removal diff above
    };
    // ... diff_shapes(base_shape, cand_shape) -> per-field findings
}
```

This loop only ever emits findings for codes in the **intersection** `committed.shapes ∩ staged.shapes`. A maintained TR whose shape is present in the staged run but **absent from the committed baseline** (a newly-maintained, not-yet-baselined TR) is never iterated, so it produces **zero findings**. `type_only_gate(findings)` therefore sees nothing objectionable and admits the promotion — and promote then re-derives `normalize_run(staged_raw, current_maintained)` and **writes that never-evaluated shape** into the committed baseline. The re-derivation self-check (`normalized.shapes == staged_run.shapes`) does not catch it, because both sides are built from the same current metadata.

This was found by **adversarial code review, not by tests**: every existing `--type-only` test started from a committed baseline byte-equal to the staged run, so `committed.shapes` already covered every maintained code — the exact precondition that hides the gap.

## Guidance

When you build a gate, guard, or policy decision on top of a diff function, **treat the diff's scope as the gate's scope** — they share the same blind spot. If the diff compares only the intersection of two collections (a "for each item in A, look it up in B, else skip" loop), then any check reading its output is structurally blind to **set-membership changes** (items only in A, or only in B).

Pair the findings-based check with an explicit **set-boundary** check that derives its answer from the raw inputs, not from the diff's output:

```rust
// crates/ls-trackers/src/api_drift.rs — companion guard, reads key sets directly
pub fn type_only_shape_set_block(
    committed: &NormalizedRun,
    staged: &NormalizedRun,
) -> Option<String> {
    let added: Vec<&str> = staged.shapes.keys()
        .filter(|k| !committed.shapes.contains_key(*k))
        .map(String::as_str).collect();
    let removed: Vec<&str> = committed.shapes.keys()
        .filter(|k| !staged.shapes.contains_key(*k))
        .map(String::as_str).collect();
    if added.is_empty() && removed.is_empty() {
        return None; // shape-key sets match — admit
    }
    // ... build a block reason naming the added/removed shapes
    Some(reason)
}
```

It is wired into `promote_committed` immediately after the findings gate, before any mutation, returning `PromoteOutcome::RefusedTypeOnly(reason)` (exit 2, zero mutation). The division of labor:

- The **findings check** answers: "did the items present on *both* sides change in a disallowed way?"
- The **set-membership check** answers: "did the *set itself* change — anything added or dropped?"

The second check must read the raw key sets, **not** the first check's output, or it inherits the same blindness.

## Why This Matters

- **Silent admission of un-evaluated data is the worst failure mode.** Not a wrong reason — admitting something the gate was never able to reason about, then persisting it as if reviewed. Here a whole maintained shape would ride into the Reviewed Baseline with no evaluation.
- **Self-validation is not authorization.** The re-derivation equality check (`normalized.shapes == staged.shapes`) looks like a safety net but compares two artifacts built from the same current metadata. It confirms internal consistency, not that a shape was ever reviewed.
- **Symmetric test fixtures hide it.** When test baselines equal test inputs, the intersection equals the union and the blind region is empty by construction. Such tests can be 100% green and never exercise the gap. Asymmetric fixtures (different key sets on each side) are what surface set-membership bugs.

## When to Apply

Apply whenever a decision is layered over a diff/comparison and the cost of a wrong "admit" is mutation, promotion, persistence, or trust:

- Promotion / approval gates over a diff that "skips items not on both sides."
- Schema or migration guards reasoning over a per-column or per-table diff.
- Permission or policy checks driven by a "what changed" report.
- Reconciliation, sync, or merge logic where added/removed keys are handled by a different code path than changed keys.

Red flags that you may have inherited a diff's scope:

- A `continue` / early-skip on absence inside the diff loop.
- A comment like "absence is handled elsewhere" — confirm the gate actually consumes that "elsewhere."
- Tests whose baseline and candidate are byte-equal or trivially derived from each other.

## Examples

Vulnerable shape — gate reads only per-item findings from an intersection diff:

```rust
fn diff(base: &Map, cand: &Map) -> Vec<Finding> {
    let mut out = Vec::new();
    for (key, b) in base {
        let Some(c) = cand.get(key) else { continue }; // only the intersection
        if b != c { out.push(Finding::changed(key)); }
    }
    out
}
// BLIND: a key only in `cand` produces no finding, so the gate admits it.
fn gate(findings: &[Finding]) -> Decision { /* inspects findings only */ }
```

Hardened — add an independent set-membership guard from the raw key sets, and require both to pass before mutating:

```rust
fn shape_set_block(base: &Map, cand: &Map) -> Option<String> {
    let added:   Vec<_> = cand.keys().filter(|k| !base.contains_key(*k)).collect();
    let removed: Vec<_> = base.keys().filter(|k| !cand.contains_key(*k)).collect();
    (!added.is_empty() || !removed.is_empty())
        .then(|| format!("shape set changed: +{added:?} -{removed:?}"))
}

if let Decision::Block(r) = gate(&findings)      { return Refused(r); }
if let Some(r) = shape_set_block(base, cand)     { return Refused(r); }
// ... only now mutate / promote
```

Test discipline that catches it: at least one fixture where `committed` and `staged` have **different key sets** — see `type_only_shape_set_block_catches_added_and_dropped_shapes` and `type_only_promote_blocks_newly_maintained_shape_with_zero_mutation` in `crates/ls-trackers`, which deliberately break the byte-equal-baseline assumption the earlier tests all shared.

## Related

- [`change-tracker-baseline-clean-self-diff.md`](change-tracker-baseline-clean-self-diff.md) — sibling pattern on the same tracker. That doc covers the baseline-side invariant (the committed baseline must *self-diff clean*); this one covers the gate-side coverage gap. A clean self-diff (same inventory on both sides) does **not** imply complete gate coverage when the staged and committed inventories differ.
- [`docs/adr/0005-staged-snapshots-for-change-tracking.md`](../../adr/0005-staged-snapshots-for-change-tracking.md) — the ADR governing the staged-snapshot / `compare()` model the gate reasons over.
- Originating work: the `--type-only` promotion gate and `type_only_shape_set_block` guard (plan `docs/plans/2026-06-21-004-feat-field-type-repin-clean-baseline-refresh-plan.md`).
