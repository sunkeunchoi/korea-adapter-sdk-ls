# Specification Document Tracker — first-baseline seed attestation

This records the review evidence for the **one-time provisional seed** of the
committed example baseline (U5, KTD-5). The reviewed commit that adds this
directory is the attestation trail; this file summarizes what was checked.

## Snapshot

- **Source:** the **shared** staged raw snapshot the API Drift Tracker already
  fetched — `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`. No
  new fetch source and no new network path (R1, KTD2). The example baseline reads
  that shared raw; it does **not** duplicate it.
- **Projection:** `ls-trackers spec-doc renormalize` over the shared raw, at
  `EXAMPLE_NORMALIZER_VERSION: 1` (independent of the API Drift normalizer
  version, KTD2).
- **Full inventory (KTD8):** 365 distinct upstream TR codes; **355** carry a
  non-empty request or response example and are projected into an `ExampleShape`.
  The projection is full-inventory, not maintained-only, so an untracked TR's
  in-place example-shape change is detectable as a visible non-gating finding
  (R3, R6/AE2).
- **`code-set.json` `provisional`:** `true` (not yet independently attested as
  complete; cleared by an operator through normal maintenance — KTD-5). The seed
  is carried **visibly provisional**, consistent with the API Drift seed stance.

## What is stored (and what is not) — secret safety (KTD7)

The baseline stores **only structural descriptors**, never a raw example value:

- JSON examples → a sorted field-path → leaf-shape map (`access_token: string`),
  with scalar sample values discarded.
- Form-encoded examples (`token`/`revoke` requests) → their **key set**
  (`appkey`, `appsecretkey`, `grant_type`, `scope`), with values discarded.
- Non-parseable examples → `opaque`, carrying no structure.

The `token`/`revoke`/`S3_` examples embed real-looking credentials (`appkey`,
`appsecretkey`, JWTs); the baseline writer's input type is `ExampleShape`, never a
raw `serde_json::Value`, so the compiler rejects writing an unprocessed payload.
The committed `normalized/examples.json` was scanned: no `appsecretkey=` form
pair, no JWT value, and no `client_credentials` literal appears in it.

## Storage layout — aggregated, not per-TR (implementation-time decision)

Unlike the API Drift baseline's per-TR `normalized/trs/{code}.json` layout, the
example shapes are stored as one sorted `code → ExampleShape` map in
`normalized/examples.json`. Per-TR files are **lossy** for the full inventory:
four upstream codes collide case-insensitively — `S3_`/`s3_`, `K1_`/`k1_`,
`S2_`/`s2_`, `YS3`/`Ys3` — so on a case-insensitive filesystem (macOS APFS) the
second write of each pair would overwrite the first and silently drop a shape,
breaking the clean self-diff (355 shapes → 351 files). A single map is
collision-proof on every filesystem and reviews as one sorted file. API Drift is
unaffected because it baselines only the 7 maintained codes, which do not collide.

## Round-trip verification (clean self-diff)

`ls-trackers spec-doc check` against the committed baseline exits `0` with no
findings, both ways:

- default (re-project the shared raw vs the committed baseline) — confirms the
  projection is deterministic over the committed raw;
- `--staged crates/ls-trackers/baselines/spec-doc` (committed vs itself) —
  confirms storage + compare wiring.

Coverage at seed time: 365 upstream, 355 carrying examples. The clean self-diff
holds across **all** payload classes (JSON shape, form key-set, opaque), not just
the 7 Tracked examples — the load-bearing bar for the full-inventory baseline.

## Tracked-TR spot check

All 7 maintained TRs project: `token` and `revoke` requests are form key-sets;
the rest (`t1102`, `t8412`, `CSPAQ12200`, `S3_`, `CSPAT00601`) and the
`token`/`revoke` responses are JSON leaf-shape maps.

## Re-seed / re-attestation (KTD-5)

The baseline is re-projected network-free with `make spec-doc-renormalize`
(`ls-trackers spec-doc renormalize`), which reads only the shared committed raw.
Run it after an `EXAMPLE_NORMALIZER_VERSION` bump, then review the
`normalized/examples.json` diff. Findings are advisory and become **SDK
Maintenance Work Items** only after human review (R8); the tracker never mutates
SDK code, docs, metadata, examples, or baselines. Clear `provisional` once an
operator has independently attested inventory completeness.
