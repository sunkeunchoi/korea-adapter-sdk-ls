---
name: implement-realtime-tr
description: Convert one tracked-only realtime/WebSocket TR into an Implemented TR by authoring the S3_ quartet (push-row struct + realtime/mod.rs re-export + WebSocket {TR}_POLICY + crosscheck registration) and a thin lifecycle smoke, then gating it on Transport reachability (clean connect→subscribe→unsubscribe on a fresh WsManager). Use for a single tracked-only WebSocket push TR (e.g. "implement K3_"). This is the realtime sibling that implement-tr's §0 HELDs out — the Implemented gate is lifecycle reachability, NOT the REST non-empty-result bar. Runs state-driven: IMPLEMENTS when the lifecycle smoke is clean, otherwise leaves the TR Tracked-only with a recorded reason.
---

Convert exactly **one** tracked-only realtime/WebSocket TR into an `implemented`
TR by authoring the `S3_` quartet and proving it with a paper **lifecycle**
smoke. This is the realtime sibling of `implement-tr`: that recipe's §0 HELDs
"realtime/WebSocket" out and gates on a non-empty REST result; here the gate is
**Transport reachability** — a clean connect → subscribe → unsubscribe with no
immediate protocol error. A decoded row is **bonus, not required**.

**This recipe stops one tier earlier than the Recommended tier.** It sets
`support.implemented: true`, leaves `support.recommended: false`, writes NO
`recommendation` block, and does NOT touch `metadata/EVIDENCE-FRESHNESS.md`. A
lifecycle smoke here gates *implementation*, never Focused Evidence.

**The gate boundary, stated plainly:** *Implemented (realtime) = a fresh
`WsManager` connects, subscribes, and unsubscribes cleanly on the paper port.*
Field/row correctness is recorded **provisional** (the smoke does not observe a
row); a struct whose out-block key or array-ness can't be confirmed from the raw
capture is marked **structurally-unverified**.

**Input:** one tracked-only realtime TR code (the `$ARGUMENTS`, e.g. `K3_`).
**Output (last line, machine-readable):** one of
- `IMPLEMENTED <tr>` — quartet authored, lifecycle smoke clean, flipped, gate green.
- `PENDING <tr> — <reason>` — quartet authored but the lifecycle did not open
  (no representative `tr_key`, in-window environmental, etc.); not flipped.
- `HELD <tr> — <reason>` — not a WebSocket TR, or a missing prerequisite.

## 0. Preconditions (decide implement-eligibility before authoring anything)

Read `metadata/trs/<tr>.yaml`. Bail early if:

- `support.implemented` is already `true` → `HELD <tr> — already implemented`.
- `owner_class` is **not** `realtime` / `facets.protocol` is **not** `websocket`
  → `HELD <tr> — out of scope (REST; use implement-tr)`. This recipe is the
  realtime lane only.
- The TR is not yet Tracked (no `metadata/trs/<tr>.yaml` with `tracked: true`, no
  projected baseline) → `HELD <tr> — not tracked; run track-realtime-tr first`.

Resolve the TR's **lane** from its domain — market-data (실시간 시세) → `tr_type`
`"3"`/`"4"`; order-event (주문 체결/접수) → `tr_type` `"1"`/`"2"`. The lane drives
the smoke's `tr_type` argument (step 4). P2 order-event TRs are **observation-only
— never place, amend, or cancel an order to provoke an event**.

## 1. Author the push-row struct — modelled from the RAW capture (KTD5)

In `crates/ls-sdk/src/realtime/frame.rs`, add a `<Xx>Row` struct mirroring
`S3Trade`. **Model it from the literal `res_example` in the raw capture**
(`crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`), NOT the
normalized baseline — the normalizer relabels arrays to `response_body` and
erases the real wire key and array-ness.

- Read the TR's `res_example` from the raw capture.
  - `[ {...} ]` (array) → decode as `Vec<...>` keyed by the **true** out-block
    name with `#[serde(rename = "<key>")]` (or `ls_core::de_vec_or_single`).
  - bare `{...}` → a single struct.
- **Every field is `#[serde(deserialize_with = "ls_core::string_or_number")]`**
  (the gateway sends quote fields as strings or bare numbers), and the struct
  derives `Default` with `#[serde(default)]` so a sparse registration-ACK body
  decodes without aborting the stream. Mirror `S3Trade` exactly.
- Model a representative, spec-grounded subset of fields — not every field.
- **If the out-block key or array-vs-single shape cannot be confirmed from the
  raw capture, mark the struct structurally-unverified** (a doc comment on the
  struct + the provisional note in step 6). Do not guess single-vs-array from the
  normalized baseline.

## 2. Re-export in `realtime/mod.rs`

Add the new row type to the `pub use frame::{...}` line in
`crates/ls-sdk/src/realtime/mod.rs` (mirror `pub use frame::{composite_key,
S3Trade, WsLane};`). No decode-dispatch-table (`dispatch.rs`) edit — decode is
generic over the caller's type at the `subscribe_typed::<Row>(tr_cd, tr_key,
lane)` call site.

## 3. WebSocket `{TR}_POLICY` + crosscheck registration

- Add `pub const {TR}_POLICY: EndpointPolicy` in
  `crates/ls-core/src/endpoint_policy.rs`, mirroring `S3_POLICY` (around line
  1289): `tr_code`, `path: "/websocket"`, `module` + `group` from the TR's
  normalized baseline, `protocol: Protocol::WebSocket`,
  `category: RateLimitCategory::MarketData`, `is_order: false`,
  `has_pagination: false`, `rate_limit_per_sec: None`,
  `corp_rate_limit_per_sec: None` (WebSocket policies carry **no** rate limits).
- **Register `{TR}_POLICY` in the crosscheck list ONLY.** Add it to the
  `policies` array in `crates/ls-core/tests/policy_index_crosscheck.rs` (where
  `S3_POLICY` sits, around line 141). Do **NOT** add it to the
  `slice_rest_policies_are_non_order_rest` list in `endpoint_policy.rs` — that
  list is REST-only and a WebSocket policy does not belong there (the inverse of
  `implement-tr`'s dual registration).

## 4. Thin lifecycle smoke (the flip gate)

Add a thin per-TR lifecycle smoke mirroring `live_smoke_ws`
(`crates/ls-sdk/tests/live_smoke.rs`) — or call the generic
`ws_lifecycle_smoke(tr_cd, tr_key, lane)` helper. It must:

- open with the paper guard and **assert the resolved WS URL carries the paper
  port `29443`** (a wrong-target run fails fast);
- use a **FRESH / isolated `WsManager` per smoke** (KTD2 — the Phase 83/84 root
  cause was a shared manager whose sender died after the first TR, poisoning
  later TRs);
- `subscribe_typed::<{Xx}Row>(<tr_cd>, &tr_key, lane)` passing the lane as a
  `WsLane` variant (`WsLane::MarketData` for 시세 feeds, `WsLane::OrderEvent` for
  order-event channels — the enum picks the `"3"/"4"` vs `"1"/"2"` register/
  deregister pair, so an invalid lane is a compile error);
- timebox a row as **bonus** (`timeout(..)` — absence is "no row within timeout
  (not a failure)", never an error);
- `unsubscribe()` cleanly.

**The `tr_key` per lane:** a P1 market-data smoke passes a domain symbol (stock
`shcode` e.g. `005930`, overseas `symbol`, F-O `futcode`/`optcode`) sourced from
`facets.caller_supplied_identifiers` / the existing `LS_LIVE_SMOKE_*` env
convention. A **P2 order-event** subscription is account-bound, not symbol-keyed:
pass an **empty string `""`** (the account scope the deterministic SC0 tests use);
if no usable account-bound key exists, that is a `PENDING <tr> — no representative
tr_key` disposition, not a guess.

Also add the `live-smoke-<tr>` `make` target (+ `.PHONY`) and a row in
`.agents/skills/promote-tr/references/smoke-map.md`. **No raw-frame logging** —
subscribe/ACK frames can carry credentials (see
`docs/design/websocket-certification-findings.md`). Keep the offline path: a
fixture hand-authored from the raw `res_example` exercises decode (no live realtime
capture exists and the smoke never observes a row), asserting array-vs-single
matches the raw shape.

> **Combined-sweep lane (high-volume waves).** When implementing a whole lane at
> once, instead of one `make` target per TR you may add a single resilient sweep
> test (e.g. `live_smoke_ws_p1`) that iterates `(tr_cd, tr_key, lane)` tuples on a
> FRESH manager each, records a per-TR `LIVE-SMOKE` line, and fails red only if ANY
> TR failed (use a non-panicking `ws_lifecycle_try`-style helper so one bad TR
> doesn't abort the sweep). Point all that lane's `smoke-map.md` rows at the one
> combined target, and have each per-TR `record(...)` emit that SAME combined make
> target in `target=` (carry per-TR identity in `inputs=[tr_cd=…]`) — never a
> `live-smoke-<tr>` label that maps to no Makefile target.

## 5. Run the smoke; interpret per the lifecycle state machine

Run the smoke (it loads `.env` and hits the real **paper** gateway).

- **Clean connect → subscribe → unsubscribe, no immediate protocol error** →
  IMPLEMENT (continue). A row is bonus; its absence does not block.
- **Subscribe rejected / errored, OR no representative `tr_key` for the domain**
  → leave Tracked-only, `PENDING <tr> — <reason>` (KTD8). Disposition faithfully;
  P2 order-event reachability on bare paper is unestablished, so Tracked-only is a
  valid outcome, not a wave failure.
- **A numeric subscribe slot triggers IGW40011-style errors** → reach for
  `string_or_number` on that slot **before** concluding environmental (KTD8 — the
  IGW40011 numeric-typing trap is documented for REST bodies only and is untested
  for WS subscribe frames). Only call it environmental after the typing fix fails.

**Claim strength is gated on the KTD6 negative control.** Because subscribe is
fire-and-forget (no ACK read), a clean lifecycle proves per-TR reachability ONLY
if `make live-smoke-ws-negative` has returned `OBSERVABLE` (a tr_cd-attributable
`rsp_cd`). If that smoke has not been run, or returned `INCONCLUSIVE`/
`NOT-OBSERVABLE`, a clean lifecycle proves only **connection reachability** — the
flip is still valid, but the metadata/commit note must say
**connection-reachable-only**, not a per-TR-reachable claim.

**Secret-safety (blocking):** any committed line (smoke line, gate output,
pending reason) must contain no token, appkey, secret, or account number — only
lengths, public tickers/dates/ports (`29443`), and structural counts. The Err
path must emit no capturable `LIVE-SMOKE` line. **Never log a raw WS frame.**

## 6. Flip metadata (the judgment step)

Only after a clean lifecycle smoke. Edit `metadata/trs/<tr>.yaml` and
`metadata/tr-index.yaml`:

- `support.implemented: true`, `support.recommended: false`.
- Write **NO** `recommendation` block; do not touch
  `metadata/EVIDENCE-FRESHNESS.md`.
- Record field correctness as **provisional** and set the
  **structurally-unverified** note where step 1 could not confirm the out-block
  key/array-ness from the raw capture.

## 7. Docgen: banner + count bump (same commit as the flip)

In `crates/ls-docgen/src/lib.rs`: add the TR to `banner_trs` and bump the coupled
`reference.len()` literal by one — ONLY after a clean lifecycle smoke, in the SAME
commit as the `support.implemented` flip. A PENDING (Tracked-only) TR contributes
nothing here. `TRACKED_TRS` and `maintained_tr_count` were already bumped at the
Tracked rung — do not touch them. Then `make docs` to regenerate the reference
page (carrying the "Implemented, not yet recommended" banner).

> When implementing a whole lane, the `reference.len()` / `banner_trs` literals
> may be reconciled once after the lane settles rather than per TR (the implemented
> total is data-dependent and an incremental bump flaps the docgen gate
> mid-wave). For a single-TR run, bump in the flip commit as above.

## 8. Gate and commit

```
make docs
cargo test                 # workspace
cargo test -p ls-core      # metadata re-validation + policy cross-check
make docs-check
```

If a gate is red: fix a mechanical assertion you missed (banner list, count,
crosscheck registration); if the failure is substantive, `git checkout` the TR's
changes and leave it Tracked-only with a recorded reason. **Never `cargo fmt` the
`ls-trackers` crate** — format only touched lines. Never leave the tree red.

Stage only this TR's files and commit:

```
feat(realtime): implement <tr> at realtime scope with paper lifecycle smoke
```

Body: the smoke target + captured result (lifecycle clean, row bonus), the lane
`tr_type`, the incremental count bump, the provisional/structurally-unverified
note, and that it stays non-recommended. Then emit the final machine-readable
line.

## Reference

- The realtime template quartet: `metadata/trs/S3_.yaml`, `S3Trade` +
  `build_subscribe_msg`/`build_unsubscribe_msg`/`build_frame(.., tr_type)` in
  `crates/ls-sdk/src/realtime/frame.rs`, `S3_POLICY` at
  `crates/ls-core/src/endpoint_policy.rs:1289`, the crosscheck registration at
  `crates/ls-core/tests/policy_index_crosscheck.rs:141`, and `live_smoke_ws` at
  `crates/ls-sdk/tests/live_smoke.rs:1648`.
- The raw-capture out-block convention (KTD5):
  `docs/solutions/conventions/tr-out-block-shape-from-raw-capture.md`.
- The REST-only IGW40011 numeric-typing trap (KTD8):
  `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`.
- Transport vs FrameDecode, the Phase 83/84 fresh-manager lesson, no-raw-frame
  logging: `docs/design/websocket-certification-findings.md`.
- `.agents/skills/promote-tr/references/smoke-map.md` — the shared TR → smoke
  registry (append, do not fork).
- The REST sibling whose §0 HELDs realtime: `.agents/skills/implement-tr/SKILL.md`.
- The upstream recipe: `.agents/skills/track-realtime-tr/SKILL.md`.
