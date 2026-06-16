---
title: "Building a change-tracker baseline that self-diffs clean: aggregate full-inventory files, make secrets type-unrepresentable, discard values for determinism"
date: 2026-06-16
category: architecture-patterns
module: crates/ls-trackers
problem_type: architecture_pattern
component: tooling
severity: high
applies_when:
  - "Adding a committed reviewed-baseline that must produce a clean self-diff"
  - "Persisting per-entity files keyed by identifiers over a full upstream inventory"
  - "Baseline inputs include real-looking secrets (appkey, appsecretkey, JWTs)"
  - "Target filesystem is case-insensitive and case-preserving (macOS APFS/HFS+)"
  - "Projecting raw payloads into normalized, value-discarding shapes"
tags:
  - ls-trackers
  - baseline-layout
  - case-insensitive-filesystem
  - self-diff
  - type-safety
  - secret-safety
  - apfs
---

# Building a change-tracker baseline that self-diffs clean

## Context

`ls-trackers` already had one working change-tracker — the **API Drift Tracker** — that stores a committed, reviewable snapshot of upstream LS API shapes and diffs new fetches against it. Building the **second** tracker (the Specification Document Tracker, which projects the latent `req_example`/`res_example` facet the first tracker never reads), the obvious move was to mirror the first tracker's proven storage layout: a per-entity file `normalized/trs/{code}.json`.

The whole tracker rests on one property — a **clean self-diff**: re-projecting the committed raw and comparing it against the committed baseline must yield zero findings when nothing upstream changed. Three things had to hold for that property to survive, and naively mirroring the first tracker broke or risked each one — at a scale the first tracker never reached (7 maintained codes vs. the full ~355-code example inventory).

The clean self-diff is net-new for every new tracker baseline; it is **not** inherited from the first tracker.

## Guidance

**1. Aggregate a full-inventory baseline into one file — don't give each entity its own file.** A per-entity file layout is only safe when the key cannot collide under the *filesystem's* casing rules. At full-inventory scale it can.

Before (mirrored from the first tracker — lossy at full-inventory scale):

```rust
// one file per code — silently overwrites on a case-insensitive filesystem
for (code, shape) in &run.shapes {
    write_json(&dir.join(TRS_DIR).join(format!("{code}.json")), shape)?;
}
```

After (`write_example_baseline`, `crates/ls-trackers/src/cli.rs`):

```rust
const EXAMPLES_FILE: &str = "normalized/examples.json";
// one aggregated, sorted code -> ExampleShape map (BTreeMap = deterministic, collision-proof)
write_json(&dir.join(EXAMPLES_FILE), &run.shapes)?;
```

A bonus: the aggregated map needs no stale-file pruning — re-projection rewrites the whole map, so a key that loses its data just disappears (the per-entity layout needs an explicit prune pass to avoid ghost files).

**2. Make secret-safety a type-level guarantee, not a runtime scrub.** Give the baseline writer a value type that *cannot represent* a raw payload. Here the writer takes `ExampleShape`, whose facets carry only structural descriptors — never a raw `serde_json::Value`:

```rust
pub enum ExampleFacet {
    #[default] Absent,
    Json { shape: BTreeMap<String, FieldShape> }, // field-path -> shape; no values
    Form { keys: BTreeSet<String> },              // key set; no values
    Opaque,                                        // present but carries no structure
}
```

There is no code path from a raw credential-bearing string to a committed file that doesn't pass through the classifier returning an `ExampleFacet`. The secret value has no type it could be stored as.

**3. Make the projection deterministic by discarding scalar sample values.** Re-projecting the same raw must yield byte-identical shapes across *every* payload class: JSON → leaf-path → `FieldShape` (drop the scalar); form → key set (drop values); unparseable text → `Opaque` (no structure). Two payloads that differ only in a timestamp, account number, or rotated secret then normalize to the same shape, so value churn is never false drift.

## Why This Matters

- **Lossy per-entity files → a broken self-diff that masquerades as real drift.** Mirroring `normalized/trs/{code}.json` onto the full ~355-code inventory put 4 case-colliding codes in one directory. On macOS APFS (case-insensitive, case-preserving) the second write of each pair overwrites the first: 355 shapes persisted as 351 files, while the manifest still recorded 355. On reload the 4 lost shapes look *removed* upstream — spurious findings on every run, turning the clean self-diff into permanent false drift. The bug is invisible on a case-sensitive CI box and invisible at small scale, so it ships silently. "The first tracker worked fine" was not evidence of safety — it only writes 7 non-colliding codes.
- **A raw `Value` in the writer → leaked credentials in git, forever.** The LS example payloads embed real-looking secrets — `token`/`revoke` form requests carry `appkey`/`appsecretkey`, responses carry JWT `access_token` values. A runtime scrubber can be forgotten or out-paced by a new payload field; a type that cannot hold a value cannot leak one.
- **A non-deterministic projection → no baseline at all.** If value churn changed the stored shape, every re-projection would self-diff dirty and the gate would be pure noise. Determinism is the precondition for a baseline being a baseline.

## When to Apply

- **Any committed per-entity baseline keyed on case-varying identifiers.** If two keys can differ only by case — or by any character the target filesystem folds — don't give each its own file; aggregate, or encode/hash the filename. The trap only appears once the key space is large enough to contain a collision.
- **Any baseline that must self-diff clean.** Silent write loss, non-deterministic projection, and casing collisions all surface identically — as findings that look like real upstream change. Treat a clean self-diff as a tested invariant (assert it over the *whole* inventory and every payload class, not just the maintained subset).
- **Any baseline derived from secret-bearing payloads.** Push secret-safety to the type at the write boundary rather than scrubbing values at runtime.

## Examples

**The 4-collision trap.** In the full ~355-code inventory these pairs collide case-insensitively: `S3_`/`s3_`, `K1_`/`k1_`, `S2_`/`s2_`, `YS3`/`Ys3`. With per-entity files on APFS, `s3_.json` overwrites `S3_.json` (and so on), so the committed baseline silently loses 4 shapes and the self-diff reports them as drift forever. The fix is the single aggregated `normalized/examples.json` map — collision-proof and reviewing as one sorted file. (The first tracker, API Drift, still uses per-TR files including `normalized/trs/S3_.json` — safe only because it writes 7 non-colliding maintained codes.)

**The type guarantee.** The committed baseline was scanned and proven free of `appsecretkey=` form pairs and JWT values; a test asserts placeholder secrets never appear in the serialized bytes. But the *type* — `ExampleShape` at the write boundary, never `serde_json::Value` — is what makes that hold by construction, not the test.

## Related

- `docs/adr/0005-staged-snapshots-for-change-tracking.md` — the governing ADR (fetch → normalize → diff against reviewed baselines); this learning is its concrete second realization.
- `crates/ls-trackers/baselines/spec-doc/SEED-ATTESTATION.md` — records the type-safe secret handling and the full-inventory clean self-diff for the second baseline.
- `crates/ls-trackers/baselines/api-drift/SEED-ATTESTATION.md` — the first baseline; uses the per-TR layout (incl. `normalized/trs/S3_.json`) that the second tracker had to abandon at full-inventory scale.
- `docs/plans/2026-06-16-006-feat-specification-document-tracker-plan.md` — KTD7 (type-unrepresentable secrets + clean self-diff), KTD8 (full-inventory baseline).
- `CONTEXT.md` — defines Staged Snapshot, Reviewed Baseline, Specification Document Tracker.
