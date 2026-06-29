---
module: ls-metadata
date: 2026-06-29
problem_type: convention
component: tooling
severity: medium
applies_when: "Tracking an LS TR code (esp. a WebSocket channel) whose code differs only by letter case from an already-tracked code, on a case-insensitive filesystem (macOS APFS default, Windows NTFS default)"
tags: [websocket, tr-tracking, filesystem, case-sensitivity, metadata, track-realtime-tr, held]
---

# Case-distinct TR codes cannot coexist on a case-insensitive filesystem

## Context

The LS OpenAPI capture distinguishes some TR codes **only by letter case** — e.g.
the WebSocket channels `k1_` / `s2_` / `s3_` / `Ys3` are genuinely different feeds
from the already-tracked `K1_` / `S2_` / `S3_` / `YS3`. They are distinct entries
in `code-set.json` and `ls-openapi-full.json`.

This repo keys TR metadata **by filename**: `metadata/trs/<code>.yaml`,
`tr-index.yaml` points each entry at `file: trs/<code>.yaml`, and the normalizer
derives the code from the file stem (`crates/ls-trackers/src/cli.rs` `load_normalized`
uses `path.file_stem()`). The realtime flip artifacts are likewise code-named: the
`<Code>Row` struct in `crates/ls-sdk/src/realtime/frame.rs` and the `{CODE}_POLICY`
const in `crates/ls-core/src/endpoint_policy.rs`.

On the default macOS filesystem (APFS, **case-insensitive**), `metadata/trs/k1_.yaml`
and `metadata/trs/K1_.yaml` resolve to the **same file**. Git itself is
case-sensitive and can store both, but the working tree — and therefore `cargo`,
the normalizer, and `make docs` — can only materialize one of the pair.

This surfaced during the open-window WS wave (plan -001): of 43 raw WS channels
slated to track, 4 (`k1_ s2_ s3_ Ys3`) collided with already-tracked uppercase
codes and could not be tracked. 39 were tracked; the 4 were excluded.

## Guidance

When the track set contains a code that differs only by case from an
already-tracked code, **exclude the lower-priority variant** and document it as a
case-collision HELD (treat it like the empty-`res_example` HELD set, not as a
defect). Do not attempt to author its `<code>.yaml`, `<Code>Row`, or `{CODE}_POLICY`
— all three name-collide, not just the file.

Before tracking a batch of case-mixed codes, screen for collisions up front:

```bash
# list new codes whose UPPER form matches an existing tracked metadata file
for c in $NEW_CODES; do
  uc=$(printf '%s' "$c" | tr a-z A-Z)
  for f in metadata/trs/*.yaml; do
    b=$(basename "$f" .yaml); ub=$(printf '%s' "$b" | tr a-z A-Z)
    [ "$ub" = "$uc" ] && [ "$b" != "$c" ] && echo "COLLISION: new '$c' vs tracked '$b'"
  done
done
```

A collision found here is **scope**, not a bug: report it, track the rest, and
record the excluded codes alongside the HELD list in the wave's notes/PR body.

## Why This Matters

Silently letting `track-realtime-tr` author `trs/k1_.yaml` would overwrite (or
no-op against) the existing `trs/K1_.yaml` on a case-insensitive volume, corrupting
the already-tracked channel's metadata and/or leaving the count-sites
(`maintained_tr_count`, `TRACKED_TRS`, the `cli.rs` literals) inconsistent with the
files actually on disk. The collision is invisible in `git status` on the authoring
machine until the baseline self-diff or a count assertion goes red. Recognizing it
as a hard filesystem limitation up front avoids a confusing mid-wave gate failure
and a wrong "track all N" count.

## When to Apply

- Authoring `track-realtime-tr` / `track-tr` for any batch that includes
  case-mixed codes (most common in the WS channel pool).
- Reconciling a "track all N" target against what actually landed — if the on-disk
  count is short, check for case collisions before assuming a missed file.

## Examples

Open-window WS wave (plan -001, 2026-06-29): 43 raw WS channels →
**39 trackable + 4 case-collision-excluded** (`k1_ s2_ s3_ Ys3`) +
10 already-HELD empty-`res_example` (`DX0 DYC NPH NYS PM_ S4_ UPH UPM h2_ s4_`).
The 4 excluded codes would each need a case-sensitive volume (or a code-aliasing
scheme that decouples the on-disk filename from the wire `tr_code`) to recover —
deferred as out of scope.

Note the inverse case is safe: `H2_` (tracked this wave) and `h2_` (HELD,
empty-`res_example`) do **not** collide on disk because `h2_` is never authored as
a file — only one of any case-pair may exist.
