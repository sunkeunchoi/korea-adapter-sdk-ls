---
title: "Revert manifest.refreshed after api-drift-renormalize — it stamps today, the pinned round-trip test expects the last raw-refresh date"
date: 2026-06-23
category: conventions
module: ls-trackers api-drift renormalize, track-tr recipe
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Running make api-drift-renormalize during a raw->Tracked wave (bulk or single-TR) to project normalized baselines"
  - "Renormalizing without a genuine raw-capture refresh (only the maintained set changed, not the upstream snapshot)"
  - "A workspace gate goes red on a manifest round-trip / byte-identical assertion after adding tracked TRs"
tags:
  - api-drift
  - renormalize
  - manifest-refreshed
  - tracked-only-wave
  - byte-identical-round-trip
  - evidence-freshness
related_components:
  - tooling
---

## Context

A raw->Tracked wave authors `metadata/trs/<tr>.yaml`, then runs `make
api-drift-renormalize` to project each TR's normalized baseline from the
**committed** raw capture (the normalizer projects shapes only for the maintained
set, so the baseline appears once the metadata exists). The renormalize CLI also
re-stamps `manifest.refreshed` with **today's date** (`crate::freshness::today()`),
even though the raw capture itself did not change. A byte-identical round-trip test
in `crates/ls-trackers/tests/api_drift.rs` pins `manifest.refreshed` to the last
raw-refresh date, so the unrelated date bump turns the gate red — and it surfaces as
a serialization / round-trip assertion failure, not an obvious "wrong date."

## Guidance

After `make api-drift-renormalize`, **revert `manifest.refreshed` to the value it
had before renormalizing** (the last raw-refresh date) before committing. On a
maintained-set expansion the manifest's only intended value change is
`maintained_tr_count`; `refreshed` moves only on an actual raw-capture refresh.

```
make api-drift-renormalize          # projects new baselines + stamps refreshed=<today>
# revert just that one field, e.g.:
#   "refreshed": "<today>"  ->  "<last raw-refresh date>"
git diff <baseline>/normalized/manifest.json   # ONLY maintained_tr_count should differ
cargo test -p ls-trackers                       # round-trip / count tests green
```

Confirm with `git diff` that the manifest shows exactly two kinds of change: the
`maintained_tr_count` bump and (separately) the new `normalized/trs/<tr>.json`
files. If `refreshed` also changed, you have not reverted it yet.

## Why This Matters

`manifest.refreshed` is **evidence-freshness metadata** — when the upstream raw
snapshot was last refreshed — not a "last touched" timestamp. Tracking a new TR
re-projects shapes from the same committed raw; nothing upstream got fresher.
Letting renormalize stamp today (1) misreports freshness, and (2) breaks the pinned
round-trip test that guards byte-identical manifest serialization. Because the
failure presents as a round-trip/serialization mismatch rather than a date error,
it costs disproportionate time to diagnose if you do not already know renormalize
stamps the field.

## When to Apply

- Every raw->Tracked wave (bulk-tracked-only or a single `track-tr` run) that runs
  `make api-drift-renormalize`.
- Any renormalize that is not paired with a genuine raw-capture refresh / Baseline
  Promotion.

Pairs with the clean-self-diff discipline: the renormalize drift guard wants
`normalized/trs/` to show **only new files** (no modified pre-existing baselines),
and the manifest to show **only** the count bump — the `refreshed` revert is what
keeps the manifest half of that guard honest.

## Examples

A clean Wave 0 manifest diff after the revert (21 TRs tracked, 49->70):

```diff
-  "maintained_tr_count": 49,
+  "maintained_tr_count": 70,
   ...
-  "refreshed": "2026-06-22"   # renormalize stamped today; reverted back
+  "refreshed": "2026-06-22"   # (no net change — correct)
```

See also `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md`
(the clean-self-diff invariant this complements) and
`docs/solutions/conventions/market-hours-read-empty-result-disposition.md`.
