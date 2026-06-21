---
name: implement-tr
description: Convert one tracked-only TR into an Implemented TR by authoring callable Rust SDK behavior and gating it on a Paper Live Smoke. Use for a single tracked-only TR code (e.g. "implement t8425"). Stops one tier earlier than promote-tr — it flips support.implemented to true, leaves support.recommended false, writes NO recommendation block and NO Focused Evidence. Runs state-driven: IMPLEMENTS when the smoke gate opens, otherwise PENDS or DROPS the TR with a recorded, credential-free reason.
---

Convert exactly **one** tracked-only TR into an `implemented` TR by authoring
callable Rust and proving it with a Paper Live Smoke. This is a state-driven
recipe: it either IMPLEMENTS the TR (smoke returns a success `rsp_cd` with a
non-empty result that deserializes), PENDS it (callable but shape-unconfirmed, or
an environmental failure with no in-window recovery), or DROPS it (a TR defect).
It never flips `support.implemented` without a passing deserialize.

**This recipe stops one tier earlier than `promote-tr`.** It sets
`support.implemented: true`, leaves `support.recommended: false`, writes NO
`recommendation` block, creates NO `metadata/evidence/<tr>.yaml`, and does NOT
touch `metadata/EVIDENCE-FRESHNESS.md`. A smoke run here gates *implementation*;
it is never recorded as Focused Evidence. That line is the whole point — it is
what keeps these Implemented, not Recommended. Focused Evidence and the
Recommended tier are `promote-tr`'s job, run later.

**Input:** one tracked-only TR code (the `$ARGUMENTS`, e.g. `t8425`).
**Output (last line, machine-readable):** one of
- `IMPLEMENTED <tr>` — flipped, banner reference page added, gate green.
- `PENDING <tr> — <reason>` — callable but not flipped (empty result / environmental, no in-window recovery).
- `DROPPED <tr> — <reason>` — TR defect (raw HTTP succeeds, SDK deserialize fails).
- `HELD <tr> — <reason>` — out of scope or a missing prerequisite.

The gate boundary, stated plainly: **Implemented = a representative paper call
builds, sends, and deserializes a non-empty success.** Recommended (recorded
Focused Evidence) is explicitly out of scope for this recipe.

## 0. Preconditions (decide implement-eligibility before authoring anything)

Read `metadata/trs/<tr>.yaml`. Bail early as HELD if:

- `support.implemented` is already `true` → `HELD <tr> — already implemented`.
- The TR is paper-incompatible, account-state, an order, or realtime/WebSocket
  → `HELD <tr> — out of scope (<reason>)`. This recipe covers read-only,
  paper-compatible REST reads only.
- The TR has an unresolved structural blocker recorded in
  `metadata/PROVISIONALITY-LEDGER.md` or an open `docs/plans/` document for this TR
  (e.g. `t8430`'s array-shape blocker) → `HELD <tr> — blocked: <reason>; needs ce-plan`.

## 1. Author callable Rust

Route by reading `facets.self_paginated` from `metadata/trs/<tr>.yaml`:
`false` → the `market_session` module (non-paginated reads); `true` → the
`paginated` module (single-page body-`idx` reads). See
`references/author-patterns.md` for the per-class skeletons; mirror the closest
existing TR (`T1102` non-paginated, `T8412`/`T1452` paginated single-page).

- **Request:** an `InBlock` struct + a request wrapper that `#[serde(rename)]`s
  the in-block under the `{tr}InBlock` key, plus a `::new(...)` constructor.
  Model only the caller-supplied fields the spec marks; do not leak fields the
  caller never sets. No-caller-input reads (e.g. `t8425`'s single `dummy` field)
  take a no-argument `::new()`.
- **Response:** an `OutBlock` struct (or `Vec<OutBlock>` with
  `ls_core::de_vec_or_single` when the spec block is a repeated/array block) plus
  a response envelope with `rsp_cd`/`rsp_msg`/the out-block. Every numeric-bearing
  out-block field uses `#[serde(deserialize_with = "ls_core::string_or_number")]`;
  every struct derives `Default` and carries `#[serde(default)]` so a sparse or
  empty out-block deserializes cleanly. Model a representative subset of fields,
  not every field — a passing deserialize does not validate the HTTP-500-seeded
  field types, so over-modeling buys nothing.
- **Facade method:** a public method on the owner-class handle dispatching through
  `Inner::post` (non-paginated) or `Inner::post_paginated` (paginated single-page).
  Reuse the existing facade accessor on `LsSdk`; no new accessor is needed.
- **Policy const:** add `{TR}_POLICY: EndpointPolicy` in
  `crates/ls-core/src/endpoint_policy.rs`, sourced from the TR's normalized
  baseline (`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`):
  `tr_code`, `path` (= `endpoint_path`), `module`, `group` (= `source_group_name`),
  `protocol`, `category` (matching `facets.rate_bucket`), `is_order: false`,
  `has_pagination` (= `facets.self_paginated`), and the per-TR rate limits.
- **Register the const in BOTH cross-check lists — neither auto-discovers it:**
  1. the `policies` array in `crates/ls-core/tests/policy_index_crosscheck.rs`, and
  2. the `slice_rest_policies_are_non_order_rest` list in
     `crates/ls-core/src/endpoint_policy.rs`.
  An unregistered const is silently skipped by the cross-check — registration is
  a step, not an afterthought.
- **has_pagination ↔ self_paginated cross-check:** the policy cross-check test
  must tie each const's `has_pagination` to the per-TR `facets.self_paginated`,
  so a mis-set pagination flag fails CI instead of shipping silently. (The legacy
  `T1102_POLICY`/`CSPAQ12200_POLICY` flags are reconciled when this assertion is
  first added.)

## 2. Build the per-TR smoke harness (hard prerequisite)

A missing smoke target is a hard HOLD — there is no Implemented gate without a
harness to run it. Build all three:

- a `live_smoke_<tr>` test fn in `crates/ls-sdk/tests/live_smoke.rs`,
- a `live-smoke-<tr>` `make` target (+ `.PHONY` entry) in the `Makefile`,
- a row in `.agents/skills/promote-tr/references/smoke-map.md` (the single shared
  registry — do NOT duplicate it under this skill).

The smoke fn opens with `paper_sdk()` (never `paper_guard()` alone —
`paper_sdk()` adds the resolved-environment defense-in-depth check). It calls the
new facade method, then `record(...)` a single credential-free `LIVE-SMOKE` line
on success. See step 4 for the Err-path discipline.

## 3. Offline deserialize test FIRST

Before running any live call, write the offline tests (a struct-shape error must
be caught without burning a gateway call):

- a representative success body deserializes into the response type **and at least
  one modeled non-key field holds a real (non-default) value** — proving the
  subset round-trips, not just that `serde(default)` returned `Ok`;
- numeric-bearing fields parse via `string_or_number` from BOTH string and number
  JSON;
- an empty result set (`rsp_cd 00707`, empty out-block) deserializes and is
  recognized as the empty/pending case;
- `::new(...)` serializes the in-block under the correct `serde(rename)` key with
  no caller fields leaking;
- (paginated) `idx` serializes as an ordinary in-block field at its first-page
  convention — NOT `#[serde(skip)]`.

Run `cargo test` and confirm these pass before step 4.

## 4. Run the Paper Live Smoke; interpret per the state machine

Resolve the target from `smoke-map.md` and run it (it loads `.env` and hits the
real **paper** gateway). Capture the single `LIVE-SMOKE …` stdout line. Interpret:

- **success `rsp_cd` + non-empty result + deserializes** → IMPLEMENT (continue).
  The success set is `00000`, empty, `00136`, `00707` — see
  `crates/ls-core/src/inner.rs::rsp_cd_is_success`.
- **success `rsp_cd` but empty result (`00707`)** → PENDING. Callable, but the
  response shape is unconfirmed; do not flip. Record the credential-free reason.
- **failure** → run the **raw-HTTP probe** (step 4a) to classify before deciding.

### 4a. Classify a failure (environmental vs TR defect)

Run the credential-safe raw-HTTP probe (`make raw-probe`, `raw_http_probe` in
`live_smoke.rs`) for the TR. It acquires the OAuth token through the SDK (never a
hand-built auth header) and issues one bare `reqwest` POST. Then:

- **raw HTTP succeeds + SDK deserialize fails** → **TR DEFECT** → DROP to
  tracked-only with a recorded reason. Never environmental.
- **raw HTTP also fails, OR the same failure reproduces across TRs in the same
  window** → **ENVIRONMENTAL** → keep the TR a candidate, do NOT flip. Retry
  in-window; a retry that confirms request construction, a non-empty success, and
  deserialization flips it. No in-window recovery → PENDING (recorded).
- Before classifying any `403`, compare credential **lengths** (never print
  secrets) to rule out `.env` quote contamination
  (`docs/solutions/integration-issues/makefile-include-env-quotes-gateway-403.md`).

No deserialize confirmation, no Implemented TR — the tier boundary holds even for
environmental failures.

## 5. Secret-safety blocking check (R3a — before any committed line)

Any line about to be committed (smoke line, probe line, gate output, drop/pending
reason) MUST contain no OAuth token, appkey, secret, or account number — only
lengths, business `rsp_cd`, public tickers/dates/ports, and structural counts.

- Never reference `rsp_msg` in a committed line — it carries localized,
  account-bearing text.
- The Err-path must emit NO capturable `LIVE-SMOKE` line: mirror
  `live_smoke_account`'s `SMOKE-FAIL` (stderr) pattern so a panic/error body
  cannot pattern-match as evidence. Each new `live_smoke_*` fn carries an offline
  test asserting its Err branch emits no `LIVE-SMOKE` line.

If a line is not credential-free, STOP and fix the harness first.

## 6. Flip metadata (the judgment step)

Only after a passing smoke (success + non-empty + deserialize). Edit
`metadata/trs/<tr>.yaml` and `metadata/tr-index.yaml`:

- `support.implemented: true`, `support.recommended: false`.
- Correct `owner_class`/index routing fields if the first-pass assignment was a
  placeholder (e.g. `standalone` → `market_session`).
- Write NO `recommendation` block. Create NO `metadata/evidence/<tr>.yaml`. Do
  NOT touch `metadata/EVIDENCE-FRESHNESS.md`.

## 7. Retire confirmed ledger facets only

In `metadata/PROVISIONALITY-LEDGER.md`, retire ONLY facets a paper call genuinely
confirms:

- `venue_session` (the session the read is actually scoped to — observed),
- `caller_supplied_identifiers` (the identifier the call accepted), where present.

Do NOT retire field-level `type` facets (ledger section 4): a clean deserialize
passes on null/absent/permissive fields, so it does not confirm the
HTTP-500-seeded types. Those stay flagged for the separate clean-fetch re-pin PR.

## 8. Docgen: banner page + count bump (same commit as the flip)

In `crates/ls-docgen/src/lib.rs`: add the TR to `banner_trs` and bump the
`reference.len()` literal by one — but ONLY after the live smoke returned
success + non-empty + deserialize, and in the SAME commit as the
`support.implemented` flip. A pending/environmental TR contributes nothing to
`banner_trs` or the count until its deferred retry succeeds. Keep the count test
name and any assertion-message string free of a stale ordinal (rename e.g.
`reference_covers_seven_implemented_…` → drop the ordinal). Then `make docs` to
regenerate; the TR gets a reference page carrying the "Implemented, not yet
recommended" banner.

## 9. Gate and commit

```
make docs
cargo test                 # workspace
cargo test -p ls-core      # metadata re-validation + policy cross-check
make docs-check
```

If a gate is red: fix a mechanical assertion you missed (banner list, count,
dual registration); if the failure is substantive, `git checkout` the TR's
changes and PEND/DROP with a recorded reason. Never leave the tree red.

Stage only this TR's files and commit:

```
feat(metadata): implement <tr> at <class> scope with paper smoke
```

Body: the smoke target + captured result, the scope (paper, single call), the
incremental count bump, and that it stays non-recommended (no evidence, no
recommendation block). Then emit the final machine-readable line.

## Reference

- `references/author-patterns.md` — per-class Rust skeletons, the credential-free
  line shapes, and the flip/no-flip checklist.
- `.agents/skills/promote-tr/references/smoke-map.md` — the shared TR → smoke
  target registry (this recipe appends to it; it does not fork it).
- `.agents/skills/promote-tr/SKILL.md` — the sibling `implemented → recommended`
  recipe this one mirrors and stops short of.
- In-repo exemplars: `crates/ls-sdk/src/market_session/mod.rs` (`T1102`,
  non-paginated), `crates/ls-sdk/src/paginated/mod.rs` (`T8412`, paginated),
  `crates/ls-core/src/endpoint_policy.rs` (`{TR}_POLICY` consts).
