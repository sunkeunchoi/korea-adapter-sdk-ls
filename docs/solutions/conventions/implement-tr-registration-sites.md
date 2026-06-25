---
title: "Implementing/tracking a TR is a multi-site registration checklist — miss one and the gate stays green but the TR is half-wired"
date: 2026-06-25
category: conventions
module: crates/ls-core, crates/ls-trackers, crates/ls-docgen, metadata, Makefile, .agents/skills
problem_type: convention
component: tooling
severity: high
applies_when:
  - "Bringing a raw TR to Tracked (track-tr) — count-assertion tax + baseline projection"
  - "Flipping a Tracked TR to Implemented (implement-tr) — policy + crosscheck + smoke registration"
  - "A new {TR}_POLICY const compiles and tests pass, but promote-tr later cannot discover the TR"
  - "Adding a live-smoke target that appears to run but silently no-ops"
tags:
  - ls-core
  - ls-trackers
  - ls-docgen
  - endpoint-policy
  - cross-check
  - smoke-map
  - makefile-phony
  - count-assertion
  - track-tr
  - implement-tr
---

# Registering a TR touches many sites — the gate does not catch the ones it does not assert

## Context

Bringing a TR up the support lifecycle (raw → Tracked → Implemented) is not one
edit — it is a fixed set of registration sites spread across five crates plus the
Makefile and the skill recipes. The Rust gate (`cargo test`) catches *some* of
them loudly (a count assertion off by one fails a test; a `{TR}_POLICY` missing
from `policy_index_crosscheck` fails the crosscheck). But several sites are **not
asserted by any test** — omit them and the whole gate stays green while the TR is
only half-wired. In the master/reference breadth wave (plan -004) a 6-persona
Tier-2 review caught two such silent omissions (a P1 and a P2) on an otherwise
green tree. This doc is the single checklist so a future wave does not rediscover
them one persona at a time.

## Guidance

### Tracking a TR (track-tr) — the count-assertion tax

Tracking N TRs bumps the maintained-shape count at **every** assertion site. All
currently read the same number, so all move together:

- `crates/ls-trackers/tests/api_drift.rs` — `manifest.maintained_tr_count`
- `crates/ls-trackers/src/cli.rs` — **four** sites: `run.shapes.len()` (x2) and
  the `promote_committed` round-trip's `maintained_shapes ==` / `committed.shapes.len()`.
  It is easy to bump three and miss the fourth (`:2779`-area, inside the same
  promote round-trip test).
- `crates/ls-docgen/src/lib.rs` — `TRACKED_TRS` array **length** AND the new
  sorted codes inserted into the array.

Then project (never hand-author) the baseline with `make api-drift-renormalize`,
and **revert `manifest.refreshed`** to the last raw-refresh date — the renormalize
re-stamps today and a round-trip test pins the old date (see
[api-drift-renormalize-preserves-refreshed-date.md](api-drift-renormalize-preserves-refreshed-date.md)).
Do **not** `cargo fmt` the whole `ls-trackers` crate — `main` is intentionally
unformatted there and CI does not enforce it; a blanket format produces a huge
spurious diff.

### Implementing a TR (implement-tr) — policy, crosscheck, smoke, docgen

- **`{TR}_POLICY` const** in `crates/ls-core/src/endpoint_policy.rs`.
- **Both** crosscheck lists for a non-order REST read:
  - `crates/ls-core/tests/policy_index_crosscheck.rs` — the `use` import **and**
    the `policies` array (two edits in one file).
  - `crates/ls-core/src/endpoint_policy.rs` — the
    `slice_rest_policies_are_non_order_rest` test array.
  An **order** policy (`is_order: true`) or a **WebSocket** policy registers in
  the crosscheck list **only**, never in `slice_rest_policies_are_non_order_rest`.
- **docgen flip counts** (only when flipping to Implemented):
  `crates/ls-docgen/src/lib.rs` — add the code to `banner_trs` AND bump
  `reference.len()`. Regenerate with `make docs`.

### The two sites NO test asserts — the silent ones

These are the easy misses, because the tree is green without them:

1. **`.agents/skills/promote-tr/references/smoke-map.md`** — add a row per
   implemented TR with `Promotion: implemented-only`. This is the registry the
   `promote-tr` discovery query reads first; a TR absent from it is invisible to
   promotion (it would be HELD as "no smoke harness"), even though its smoke
   exists and passes. No Rust test checks this file.
2. **`Makefile` `.PHONY`** — add each `live-smoke-<tr>` target name. A recipe stub
   that is not declared `.PHONY` is silently skipped by `make` if a same-named file
   ever exists in the tree. Every prior smoke target is listed there; consistency
   is the only thing keeping it honest.

## Why This Matters

The Rust gate gives false confidence: it asserts the count tax and the
`policy_index_crosscheck`, so those *can't* be missed silently — but it says
nothing about `smoke-map.md` or `.PHONY`. A wave that stops at "green gate" ships
TRs that are callable and smoke-passing yet undiscoverable by `promote-tr`, and
smoke targets that can no-op. The cost is paid later, confusingly, by whoever runs
the promotion sweep. Treating registration as a fixed checklist — not "whatever
the compiler complains about" — closes the gap up front.

## When to Apply

- Any `track-tr` run (the count tax + revert-refreshed + no-blanket-fmt block).
- Any `implement-tr` run (policy + both crosschecks + smoke-map row + `.PHONY` +
  docgen banner/reference on flip).
- When `cargo test` is green but you have not personally confirmed the two
  un-asserted sites — assume they are missing until you have looked.

## Examples

smoke-map row + `.PHONY` entry for a flipped read (the two un-asserted sites):

```markdown
| `t9945` | `live-smoke-t9945` | `live_smoke_t9945` | any session; `gubun="1"` (KOSPI) — stock master read, non-empty off-session | implemented-only | paper stock master (plan -004) |
```

```make
.PHONY: ... live-smoke-o3126 live-smoke-t9945 live-smoke-t3202 ... raw-probe
```

Both crosscheck registrations for one non-order REST read (`policy_index_crosscheck.rs`):

```rust
// use import (top of file)
T9945_POLICY, T3202_POLICY,
// AND the policies array in slice_policies_mirror_metadata_index
T9945_POLICY,
T3202_POLICY,
```

## Related

- [api-drift-renormalize-preserves-refreshed-date.md](api-drift-renormalize-preserves-refreshed-date.md)
  — the revert-`manifest.refreshed` step of the tracking tax.
- [tr-out-block-shape-from-raw-capture.md](tr-out-block-shape-from-raw-capture.md)
  and [sdk-struct-field-from-baseline-korean-name.md](sdk-struct-field-from-baseline-korean-name.md)
  — the response-modeling decisions that happen alongside this registration.
- `../architecture-patterns/ls-sdk-pagination-modeling.md` — the `has_pagination`
  metadata mirror, one of the asserted registration fields.
- `AGENTS.md` / `metadata/PROVISIONALITY-LEDGER.md` — the prose these sites were
  scattered across before this consolidation.
