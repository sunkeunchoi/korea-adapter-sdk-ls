---
name: track-tr
description: Bring one raw TR (present only in the raw OpenAPI capture, with no metadata and no normalized baseline) up to the Tracked rung by authoring its metadata/trs/<tr>.yaml + tr-index entry and projecting its normalized baseline from the committed raw capture. Use for a TR code that has no metadata/trs/<tr>.yaml yet (e.g. "track t8424"). Stops one tier earlier than implement-tr — it produces metadata + a pinned baseline only; it authors NO Rust and flips NO support state past tracked. Runs state-driven: TRACKS when the raw shape is complete, otherwise HELDs the TR with a recorded reason.
---

Bring exactly **one** raw TR — present in the raw OpenAPI capture
(`crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`) and
`code-set.json` but with no `metadata/trs/<tr>.yaml` and no
`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json` — up to the
**Tracked** rung. This is a state-driven recipe: it either TRACKS the TR
(authors metadata + projects a deserializable baseline from the raw capture) or
HELDs it (the raw shape is incomplete and a baseline cannot be pinned without
live probing).

**This recipe stops one tier earlier than `implement-tr`.** It authors
`metadata/trs/<tr>.yaml` with `support: {tracked: true, implemented: false,
recommended: false}`, adds the `tr-index.yaml` routing entry, and commits the
*projected* normalized baseline. It writes NO request/response structs, NO
`{TR}_POLICY`, NO SDK facade, and NO smoke harness — that is `implement-tr`'s
job, run next. The Tracked rung is the prerequisite `implement-tr` already
assumes (it derives structs from the pinned baseline).

**The baseline is projected, never hand-authored.** The normalizer projects
shapes only for TRs in the *maintained set* — the keys of authored metadata
(`crates/ls-trackers/src/cli.rs` `maintained_codes`). So once the metadata exists,
`make api-drift-renormalize` (network-free) emits the baseline from the committed
raw capture. Do not hand-write `normalized/trs/<tr>.json`.

**Input:** one raw TR code (the `$ARGUMENTS`, e.g. `t8424`).
**Output (last line, machine-readable):** one of
- `TRACKED <tr>` — metadata authored, baseline projected + committed, gate green.
- `HELD <tr> — <reason>` — raw shape incomplete (cannot pin a deserializable
  baseline without live probing) or out of scope.

## 0. Preconditions (decide track-eligibility before authoring anything)

- If `metadata/trs/<tr>.yaml` already exists → `HELD <tr> — already tracked`.
- Confirm the TR is present in the raw capture with **complete request *and*
  response blocks**. Locate it under `groups[].trs[]` (keyed by `code`) in
  `ls-openapi-full.json`; verify `url`, `req_example`, `res_example`, and the
  `properties` field set are all present and non-empty. A TR whose raw shape is
  missing a request or response block, or whose blocks are too sparse to pin a
  deserializable baseline, is `HELD <tr> — incomplete raw shape; needs live probe`.
- This recipe covers read-only, paper-compatible REST reads. An order/account/
  realtime/WebSocket TR is `HELD <tr> — out of scope (<reason>)`.

## 1. Author `metadata/trs/<tr>.yaml`

Mirror the closest existing exemplar: `metadata/trs/t1101.yaml` (a
`market_session` read with a caller identifier) or `metadata/trs/t1452.yaml` (a
`paginated` self-continuation read). Every enum value is `snake_case` per
`crates/ls-metadata/src/schema.rs`. Author from the raw capture's field set:

- `tr_code`, `name` (from the raw `code`/`name`).
- `owner_class`: `paginated` if the read self-continues (a body cursor field like
  `cts_date`/`idx`), else `market_session`.
- `facets`: `protocol` (`rest`), `instrument_domain` (the closed enum variant for
  the TR's market area, e.g. `sector_index` / `stock`), `venue_session`
  (`krx_regular` for session-scoped reads), `date_sensitive`, `self_paginated`
  (true ⟺ `owner_class: paginated`), `account_state: false`, `paper_incompatible:
  false`, `certification_path: none` (no Focused Evidence at this rung),
  `rate_bucket: market_data`, and `caller_supplied_identifiers` (the `required=Y`
  inputs the caller supplies — e.g. `[upcode]`, `[upcode, shcode]`; `[]` for
  no-input reads). Mode/filter flags (`gubun*`, `rate_gbn`) are NOT identifiers.
- `dependencies.self_continuation_fields`: the body cursor for a paginated read
  (e.g. `[cts_date]`), else `[]`.
- `support: {tracked: true, implemented: false, recommended: false}`.
- `maintenance.source_spec_hash` (the migration-source `spec_hash`; hand-authored,
  not validated against the baseline) and `last_reviewed` (today, absolute date).

Then add the `tr-index.yaml` routing entry under `trs:` (file, owner_class,
protocol, instrument_domain, venue_session — the validator cross-checks these
against the per-TR yaml, so they must match exactly).

## 2. Project the baseline — `make api-drift-renormalize`

Run `make api-drift-renormalize` (network-free). The normalizer re-projects every
maintained TR's shape from the current raw capture and writes the new
`normalized/trs/<tr>.json` plus an updated `normalized/manifest.json`
(`maintained_tr_count` +1 per newly-tracked TR). `code-set.json` does **not**
change — it already enumerates the full raw inventory.

## 3. Verify the clean self-diff, then commit

- **Drift guard (hard):** `git diff --stat crates/ls-trackers/baselines/api-drift/normalized/trs/`
  must show **only the new file(s)** changed. Any modified *pre-existing* baseline
  means the raw capture drifted since it was pinned — diagnose it (per
  `docs/solutions/integration-issues/fault-tolerant-fallback-masked-wrong-endpoint-bug.md`)
  before committing. Do not smuggle unrelated shape changes into this commit.
- Confirm the projected baseline is deserializable and clean per
  `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md`
  (re-projecting raw and comparing to the committed baseline yields zero findings
  for the new TR).
- Gate: `cargo test -p ls-metadata -p ls-core` (metadata validation + the policy
  cross-check) and `make docs-check` green.
- Commit the metadata yaml + `tr-index.yaml` + the projected
  `normalized/trs/<tr>.json` + `manifest.json` together. Keep the generated
  baseline diff in its own commit, apart from any hand-authored code, so a reviewer
  can read it separately.

Emit the final `TRACKED <tr>` line. The TR is now ready for `implement-tr`.

## Patterns to follow

- Metadata exemplars: `metadata/trs/t1452.yaml` (paginated), `metadata/trs/t1101.yaml`
  (market_session with a caller identifier); schema: `crates/ls-metadata/src/schema.rs`.
- Track-rung mechanism: `crates/ls-trackers/src/cli.rs` (`maintained_codes`,
  `write_normalized`); `Makefile` `api-drift-renormalize`.
- The downstream recipe: `.agents/skills/implement-tr/SKILL.md` (Tracked →
  Implemented).
