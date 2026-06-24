---
name: track-realtime-tr
description: Bring one raw realtime/WebSocket TR (present only in the raw OpenAPI capture, with no metadata and no normalized baseline) up to the Tracked rung by authoring its metadata/trs/<tr>.yaml with realtime facets (owner_class: realtime + protocol: websocket) + tr-index entry and projecting its normalized baseline from the committed raw capture. Use for a WebSocket push TR with no metadata/trs/<tr>.yaml yet (e.g. "track K3_"). This is the realtime sibling that track-tr's §0 HELDs out — it covers exactly the WebSocket subscribe-push TRs the REST track-tr recipe refuses. Stops one tier earlier than implement-realtime-tr — it produces metadata + a pinned baseline only; it authors NO Rust and flips NO support state past tracked.
---

Bring exactly **one** raw realtime/WebSocket TR — present in the raw OpenAPI
capture (`crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`) and
`code-set.json` but with no `metadata/trs/<tr>.yaml` and no
`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json` — up to the
**Tracked** rung. This is the **realtime sibling** of `track-tr`: `track-tr`'s
§0 HELDs "anything that subscribes to broker-side state" out of scope, and this
recipe is exactly that lane. The `S3_` quartet
(`metadata/trs/S3_.yaml` + `S3_POLICY` + `realtime/frame.rs::S3Trade` +
`live_smoke_ws`) is the authoritative template.

**This recipe stops one tier earlier than `implement-realtime-tr`.** It authors
`metadata/trs/<tr>.yaml` with `support: {tracked: true, implemented: false,
recommended: false}`, adds the `tr-index.yaml` routing entry, and commits the
*projected* normalized baseline. It writes NO push-row struct, NO `{TR}_POLICY`,
NO crosscheck registration, and NO lifecycle smoke — that is
`implement-realtime-tr`'s job, run next.

**The baseline is projected, never hand-authored.** The normalizer projects
shapes only for TRs in the *maintained set* — the keys of authored metadata. So
once the metadata exists, `make api-drift-renormalize` (network-free) emits the
baseline from the committed raw capture. Do not hand-write
`normalized/trs/<tr>.json`.

**Input:** one raw realtime TR code (the `$ARGUMENTS`, e.g. `K3_`).
**Output (last line, machine-readable):** one of
- `TRACKED <tr>` — metadata authored, baseline projected + committed, gate green.
- `HELD <tr> — <reason>` — raw shape incomplete (cannot pin a deserializable
  baseline without live probing) or not a WebSocket push TR.

## 0. Preconditions (decide track-eligibility before authoring anything)

- If `metadata/trs/<tr>.yaml` already exists → `HELD <tr> — already tracked`.
- Confirm the TR is present in the raw capture under `groups[].trs[]` (keyed by
  `code`) in `ls-openapi-full.json` with a **non-empty subscribe body and a
  non-empty `res_example`**. A realtime TR's subscribe body is the `tr_cd`/`tr_key`
  pair; the `res_example` is the push row. A TR whose `res_example` is missing or
  empty is `HELD <tr> — incomplete raw shape; needs live probe`.
- **This recipe covers WebSocket push TRs only** — the inverse of `track-tr`'s §0.
  Confirm the TR is a realtime subscribe channel (a `/websocket` push feed, not a
  REST request/response). A REST TR routed here is `HELD <tr> — out of scope (REST;
  use track-tr)`. Both the P1 market-data lane and the P2 order-event lane are in
  scope here; the lane only changes facets/`tr_type` downstream, not eligibility.

## 1. Author `metadata/trs/<tr>.yaml`

Mirror `metadata/trs/S3_.yaml` exactly — it is the only `owner_class: realtime`
exemplar. Every enum value is `snake_case` per `crates/ls-metadata/src/schema.rs`.
Author from the raw capture's field set, but at the Tracked rung **omit the
`recommendation` block entirely** (that ships at the Recommended tier, not here):

- `tr_code`, `name` (from the raw `code`/`name`).
- `owner_class: realtime` — always, for every TR routed through this recipe.
- `facets`:
  - `protocol: websocket` — always.
  - `instrument_domain`: the closed enum variant for the TR's market area
    (`stock`, the overseas/F-O variant, etc.) from `schema.rs`.
  - `venue_session`: the session the feed is scoped to (`krx_regular` for a
    KRX-regular-hours feed; the night/overseas variant where applicable).
  - `date_sensitive: false`, `self_paginated: false` (a push feed never
    self-paginates), `account_state: false` (even P2 order-event feeds are
    observation-only — see KTD/U6), `paper_incompatible: false`,
    `certification_path: none` (no Focused Evidence at this rung),
    `rate_bucket: market_data` (WebSocket policies carry no REST rate limits).
  - `caller_supplied_identifiers`: the subscribe `tr_key` slot — `[shcode]` for a
    stock feed, the overseas-symbol / F-O-code slot for those lanes. For a P2
    order-event feed whose `tr_key` is an account-bound slot, record it as the
    identifier; the lane stays `owner_class: realtime` (it does not reclassify to
    `account`).
- `dependencies: {self_continuation_fields: [], strong_order_fields: []}`.
- `support: {tracked: true, implemented: false, recommended: false}`.
- `maintenance.source_spec_hash` (the migration-source `spec_hash`; hand-authored)
  and `last_reviewed` (today, absolute date).

Then add the `tr-index.yaml` routing entry under `trs:` — `file`, `owner_class:
realtime`, `protocol: websocket`, `instrument_domain`, `venue_session` — matching
the per-TR yaml exactly (the validator cross-checks them).

## 2. Project the baseline — `make api-drift-renormalize`

Run `make api-drift-renormalize` (network-free). The normalizer re-projects every
maintained TR's shape from the current raw capture and writes the new
`normalized/trs/<tr>.json` plus an updated `normalized/manifest.json`
(`maintained_tr_count` +1 per newly-tracked TR). `code-set.json` does **not**
change.

**Revert `manifest.refreshed` (KTD7, hard).** `api-drift-renormalize` re-stamps
`manifest.refreshed` with today's date, which breaks the byte-identical
round-trip test. After renormalize, edit `normalized/manifest.json` to restore
`refreshed` to the **last raw-refresh date** it carried before this run (per
`docs/solutions/conventions/api-drift-renormalize-preserves-refreshed-date.md`).
The only intended `manifest.json` change is the `maintained_tr_count` bump.

## 3. Bump the Tracked-rung count assertions (KTD7)

Tracking a realtime TR bumps the same count surfaces as a REST track. Update:

- `crates/ls-trackers/tests/api_drift.rs` — the `maintained_tr_count` expectation.
- `crates/ls-docgen/src/lib.rs` — `TRACKED_TRS` (length + sorted insert of the
  new code).
- `crates/ls-trackers/src/cli.rs` — the `ls-trackers` CLI shape-count literals
  (the 44-literal-style maintained-set counts).

Do **not** touch `reference.len()` / `banner_trs` — those are *Implemented*-rung
surfaces and stay untouched here (they bump per reachable TR in
`implement-realtime-tr`). **Never `cargo fmt` the whole `ls-trackers` crate** —
main is unformatted; format only touched lines.

## 4. Verify the clean self-diff, then commit

- **Drift guard (hard):** `git diff --stat
  crates/ls-trackers/baselines/api-drift/normalized/trs/` must show **only the new
  file(s)**. A modified pre-existing baseline means the raw capture drifted —
  diagnose before committing.
- `git diff normalized/manifest.json` shows **only** `maintained_tr_count`
  changed; `refreshed` is unchanged (step 2 revert).
- Gate: `cargo test -p ls-metadata -p ls-core` (metadata validation + the policy
  cross-check) and `make docs-check` green. `maintained_tr_count` matches the new
  total; `TRACKED_TRS` length/contents updated.
- Commit the metadata yaml + `tr-index.yaml` + the projected
  `normalized/trs/<tr>.json` + `manifest.json` + the count-literal edits together.

Emit the final `TRACKED <tr>` line. The TR is now ready for
`implement-realtime-tr`.

## Patterns to follow

- Metadata exemplar: `metadata/trs/S3_.yaml` (the only `owner_class: realtime`
  TR); schema: `crates/ls-metadata/src/schema.rs` (`OwnerClass::Realtime`,
  `Protocol::Websocket`).
- The `manifest.refreshed` revert convention:
  `docs/solutions/conventions/api-drift-renormalize-preserves-refreshed-date.md`.
- The clean-self-diff convention:
  `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md`.
- The downstream recipe: `.agents/skills/implement-realtime-tr/SKILL.md` (Tracked
  → Implemented).
- The REST siblings whose §0 HELDs realtime: `.agents/skills/track-tr/SKILL.md`.
