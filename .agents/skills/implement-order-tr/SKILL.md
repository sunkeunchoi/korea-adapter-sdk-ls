---
name: implement-order-tr
description: Convert one tracked-only ORDER TR (owner_class orders, is_order true) into an Implemented TR by authoring callable no-retry order Rust (post_order dispatch, distinct order success predicate, dedup, kill switch, redaction) and gating it on a GUARDED LIVE PAPER ORDER — not an automated smoke. Use for a single tracked-only order TR (e.g. "implement CSPAT00701"). This is the order sibling that implement-tr's §0 HELDs out — the Implemented gate is a real paper-order evidence matrix, NOT the REST non-empty-result bar and NOT realtime lifecycle reachability. Runs state-driven: IMPLEMENTS only on a clean in-window guarded paper-order run, otherwise PENDS (machinery ships, evidence pending) with a recorded reason.
---

Convert exactly **one** tracked-only order TR (`owner_class: orders`,
`facets.is_order: true` mirrored by `{TR}_POLICY.is_order: true`) into an
`implemented` TR by authoring callable **no-retry order** Rust and proving it with
a **guarded live paper order**. This is the order sibling of `implement-tr`: that
recipe's §0 HELDs "order" out and gates on a non-empty REST result; here the gate
is a real, irreversible market action, so it gates on an operator-initiated
**guarded paper-order evidence matrix** (order-safety §4), never the automated
Paper Live Smoke and never realtime lifecycle reachability.

The first order package (`CSPAT00601` submit + `t0425` reconciliation read) built
the runtime this recipe assumes — `Inner::post_order`, the `OrderDeduplicator`,
the kill switch, the order success predicate, the redaction/tracing contract, the
six-state reconciliation matcher, and the guarded `order_smoke` harness. This
recipe documents the path that wave walked, frozen for the next order TR.

**This recipe stops one tier earlier than the Recommended tier.** It sets
`support.implemented: true`, leaves `support.recommended: false`, writes NO
`recommendation` block, creates NO `metadata/evidence/<tr>.yaml`, and does NOT
touch `metadata/EVIDENCE-FRESHNESS.md`. A guarded paper-order run here gates
*implementation*; it is never recorded as Focused Evidence.

**The gate boundary, stated plainly:** *Implemented (order) = an operator places a
real guarded paper order out-of-band, the harness captures a credential-free
evidence matrix pinning the order success predicate, and the order is reconciled
via the companion read.* A clean in-window run flips the TR; an in-window
inability to place records **Pending** — the machinery ships, the flip waits.

**Input:** one tracked-only order TR code (the `$ARGUMENTS`, e.g. `CSPAT00701`).
**Output (last line, machine-readable):** one of
- `IMPLEMENTED <tr>` — order Rust authored, guarded paper-order matrix clean,
  flipped, gate green.
- `PENDING <tr> — <reason>` — machinery authored but the paper account could not
  place in-window (not order-capable / `01900` / no clearing mechanism); NOT
  flipped, predicate marked seed-only/unconfirmed.
- `HELD <tr> — <reason>` — not an order TR, or a missing prerequisite (the order
  runtime, or the companion reconciliation read, is absent).

## 0. Preconditions (decide implement-eligibility before authoring anything)

Read `metadata/trs/<tr>.yaml`. Bail early if:

- `support.implemented` is already `true` → `HELD <tr> — already implemented`.
- `owner_class` is **not** `orders` / the TR is not `is_order` → `HELD <tr> — out
  of scope (use implement-tr / implement-realtime-tr)`. This recipe is the order
  lane only.
- The TR is not yet Tracked (no `metadata/trs/<tr>.yaml` with `tracked: true`, no
  projected baseline) → `HELD <tr> — not tracked; run track-tr first`.
- The order runtime is absent (`Inner::post_order`, `OrderDeduplicator`, the kill
  switch, the order success predicate) → `HELD <tr> — order runtime missing; land
  the first order package first`.
- The TR's reconciliation companion read is not Implemented (a modify/cancel
  consumes an order number that `t0425` must observe) → `HELD <tr> — companion
  reconciliation read not implemented`.

## 1. Author callable no-retry order Rust

Author into the `orders` module (`crates/ls-sdk/src/orders/`), mirroring
`CSPAT00601`. The order class differs from every read class in five load-bearing
ways — get all five right:

- **Request:** an `InBlock` struct + a request wrapper that `#[serde(rename)]`s the
  in-block under the `{tr}InBlock` key, plus a `::new(...)`/`::limit(...)`
  constructor. Model **every required** in-block field from the normalized baseline
  (an order rejects on a missing required field — unlike a read, you cannot model a
  subset of the request). The numeric **request** fields (quantity, price, and any
  numeric order field) MUST carry
  `#[serde(serialize_with = "ls_core::string_as_number")]` — a quoted numeric
  request field makes the gateway return `IGW40011` (see
  `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`).
  `RecCnt` and other counts are **response** fields — decode with
  `string_or_number`, never serialize.
- **Response:** model the out-blocks at their true wire shape read from the **raw
  capture** (`A0003` = single object, `A0005` = array — do not guess
  single-vs-array). Surface the order number (the reconciliation key) and redact
  account-sensitive fields (`AcntNo`, `AcntNm`) in a hand-written `Debug`. Derive
  `Serialize` on the response — the dedup cache round-trips it. Every numeric field
  uses `string_or_number`; every struct derives `Default` + `#[serde(default)]`.
- **Facade method:** a public method on the `Orders` handle dispatching through
  `Inner::post_order` — NEVER `post`/`post_paginated`. `post_order` is the
  single-attempt, no-retry path; routing an order through `post` would let a
  transport timeout retry and risk a double fill.
- **Policy const:** add `{TR}_POLICY: EndpointPolicy` in
  `crates/ls-core/src/endpoint_policy.rs` from the normalized baseline, with
  **`is_order: true`** and `category: RateLimitCategory::Orders` (matching
  `facets.rate_bucket: orders`).
- **Register the const in the policy-index crosscheck ONLY (R12) — this is the
  INVERSE of `implement-tr`:**
  1. add it to the `policies` array in
     `crates/ls-core/tests/policy_index_crosscheck.rs`, **and**
  2. do NOT add it to `slice_rest_policies_are_non_order_rest` in
     `crates/ls-core/src/endpoint_policy.rs` — that list asserts every member is a
     non-order endpoint, so an `is_order: true` const there fails the test.
  (A non-order companion read raised alongside — e.g. `t0425` — registers in
  BOTH lists, per `implement-tr`.)

The order success predicate, the kill switch, the dedup window, and the
redaction/tracing span are already enforced inside `post_order` — you do not
re-author them per TR. If this TR's accepted-code set differs from the seed,
widen `classify_order_rsp_cd` in `crates/ls-core/src/inner.rs` and re-run the
mock gate (§3) before any flip. The seed set, per
`docs/design/ls-gateway-response-semantics.md` §Order-Specific Codes, is
`00039` sell / `00040` buy (submit), `00462` (modify), `00463`/`00156`
(cancel). The modify/cancel codes are **seed-only/unconfirmed** until a live run
re-confirms them — confirm-or-widen from the run, never silently trust the seed.

**Modify/cancel lane (the order TRs that target an EXISTING order number).** A
modify (`CSPAT00701`) / cancel (`CSPAT00801`) references the original order via
`OrgOrdNo` (a numeric request field — `string_as_number`, KTD6) and returns a
**new** order number in `OutBlock2.OrdNo` with the original in `OutBlock2.PrntOrdNo`
(KTD4). Beyond the five load-bearing differences above, get these right:

- A modify is **absolute** — it carries the full target `OrdQty`/`OrdPrc`, no
  delta — so a blind re-send re-applies the same target (idempotent-by-target), it
  does not compound. A cancel is idempotent for free from the dedup key (the full
  body, incl. `OrgOrdNo`, is hashed, KTD5).
- Build the reconciliation intent from the request via a `reconcile_intent(account_no)`
  helper that sets `OrderIntent::{modify,cancel}(…)` with `org_order_no = OrgOrdNo`
  and the `OrderAction` discriminator, normalizing the `IsuNo` market prefix to the
  `t0425` `expcode` form. The matcher (§3) keys off `org_order_no`, not the new
  order number (which an ambiguous send never returns).

## 2. Wire the guarded evidence harness (hard prerequisite)

A missing evidence target is a hard HOLD — there is no Implemented gate without a
harness to run it. Extend the order harness (`crates/ls-sdk/tests/order_smoke.rs`)
and the Makefile:

- add the TR to the explicit `select_order_tr()` allowlist (no default selection),
- build the scenario matrix for this TR's class — for a submit: a resting
  far-from-market limit buy and sell (priced at the daily band's far edge from
  `t1102`'s `uplmtprice`/`dnlmtprice`, KTD8), one marketable order, and one
  deliberate out-of-band rejection; for a modify/cancel: a **chained** place →
  modify → observe → cancel → observe sequence keyed by the real order number the
  submit leg returns (see the chained smoke below),
- a `live-smoke-order-<tr>` (or shared `live-smoke-order` / `live-smoke-order-chain`)
  `make` target gated on `LS_TRADING_ENV=paper` **and** an explicit `LS_ORDER_SMOKE=1`
  opt-in,
- keep every scenario's order params DISTINCT so an identical re-run misses the
  dedup cache and regenerates fresh broker codes (AE3).

**The chained modify/cancel evidence sequence (extends the submit harness).** A
modify/cancel cannot be evidenced in isolation — it needs a live order number to
target. Drive one chained run: submit a resting far-from-market order (this leg is
ALSO the submit pair's gate evidence) → observe it via `t0425` → modify it keyed by
the submit's `OrdNo` → observe (a **non-ambiguous** read here PINS the KTD4 replace
shape: does the original `OrgOrdNo` row move to `정정`/`정정확인` or stay `접수`, and
does `t0425.orgordno` carry the immediate parent) → cancel the live order
(the modify's new `OrdNo`, or the original on an in-place modify) → observe the book
clean. **Cancel is the PRIMARY teardown** once a cancel TR exists; paper reset is the
**fallback** when the cancel link itself fails or a resting order fills unexpectedly.
A failure after the submit leg leaves only the modify/cancel pair Pending — the
submit gate never waits on the modify/cancel gate (independent flip gates).

The harness MUST fail closed: explicit TR selection with no default, operator
params validated before SDK construction, a degenerate `t1102` band
(halted / limit-locked / newly-listed → `up <= dn` or zero) recording
"not certified" rather than placing, and credential-free evidence only.

## 3. Mock-gate the order logic offline FIRST

Before any live order, prove the order logic against wiremock (it never submits a
live order). Extend `crates/ls-sdk/tests/order_logic_gate.rs` (and the `ls-core`
`order_dedup`/`inner` and `orders::reconcile` unit suites) so the new TR exercises:

- **no-retry:** a 5xx on this order is a single HTTP attempt (count the hits),
- **dedup:** an identical submit within the window is a cache hit (zero second
  HTTP),
- **predicate:** an ack code classifies Accepted and deserializes; a reject code
  is `ApiError` with the broker code/message; an unrecognized `2xx`/`00000` is
  `AmbiguousOrder` (never silently Rejected — the double-fill guard),
- **kill switch:** disabled orders halt before any HTTP,
- **reconciliation:** the six states (Accepted/Rejected/Duplicate/Modified/
  Canceled/Unknown) are reachable, and a failed query fails toward Unknown.

For a **modify/cancel** TR, the reconciliation matcher is **order-state-aware** and
keys off the *referenced* order number, not "did a new order appear". The matcher
`reconcile_rows` **scans ALL matched rows and takes the strongest classification**
(it must NOT early-return on the first row) — matching `OrgOrdNo` against both
`t0425.ordno` (the original) and `t0425.orgordno` (a modify/cancel child). Mock-gate
these classifications explicitly:

- **Cancel INVERTS the risk (R7, AE1):** a matched row is `Canceled` ONLY on an
  explicit `취소` row. A still-`접수`/체결 original, a `정정`, or a `취소거부` is
  "still-live / not-canceled" (Unknown or Rejected) — never `Accepted`, and it
  **never clears retry**. A cancel wrongly believed successful leaves a live order.
- **Modify is idempotent-by-target:** landed iff a `정정` row OR a non-rejected
  child row (`orgordno == OrgOrdNo`) exists. A bare still-`접수` original with
  neither is **not landed** and safe to re-send (the absolute target re-applies).
  Regression guard: a still-`접수` original must NOT classify `Accepted`/landed for
  a modify — that false "landed" verdict is exactly what this lane prevents.
- **Strongest-classification guard:** a page with BOTH a still-`접수` original row
  AND a `취소`/`정정` row must classify on the transition row (the matcher scans all
  rows), not on whichever `ordno` hits first.

Run `cargo test` and confirm green before §4.

## 4. Run the guarded paper-order matrix; interpret per the state machine

Resolve and run the target (`make live-smoke-order …`). It loads `.env`, requires
both opt-ins, fetches + validates the band, places the matrix, captures every
`rsp_cd`/`rsp_msg`, and reconciles resting orders via the companion read. Interpret
the recorded evidence:

- **clean in-window matrix** (resting orders acknowledge, the deliberate order
  rejects, reconciliation classifies the resting orders Accepted) → IMPLEMENT
  (continue). Confirm the observed accepted codes match the predicate seed; if the
  live set is wider, amend `classify_order_rsp_cd` and re-run §3 before flipping.
- **paper account cannot place in-window** (not order-capable, `01900`
  paper-incompatible, empty, or no in-window clearing mechanism for teardown) →
  PENDING. The machinery ships; record the credential-free reason; mark the
  predicate seed-only/unconfirmed. Do NOT flip.
- **ambiguous outcome** on a scenario → PENDING that scenario and reconcile; an
  ambiguous send is never evidence of an accept.

A missing in-window clearing mechanism (the resting order cannot be cleared) is a
**blocking Pending** condition, not a silent gap. The in-wave `CSPAT00801` cancel
is now the **primary** teardown (the chained smoke cancels its own resting order);
paper reset is the **fallback** when the cancel link itself fails or a resting order
fills unexpectedly. For a class with no cancel TR yet, paper reset is the only
verified teardown and its absence is the blocking Pending.

## 5. Secret-safety blocking check (before any committed/recorded line)

Every recorded evidence line and committed line MUST be credential-free: no OAuth
token, appkey, secret, or account number — only the TR, scenario, classification,
business `rsp_cd`, order number/time, reconciliation observation, lengths, and the
"production not run" statement. The reconciliation local-evidence record persists
the account and the request hash only as **HMAC-keyed hashes** (never cleartext,
never a bare `SHA256` — account numbers are low-entropy and reversible), to a known
location with a stated retention bound. If a line is not credential-free, STOP and
fix the harness first.

## 6. Flip metadata (the judgment step)

Only after a clean in-window matrix. Edit `metadata/trs/<tr>.yaml` and
`metadata/tr-index.yaml`:

- `support.implemented: true`, `support.recommended: false`.
- Write NO `recommendation` block. Create NO `metadata/evidence/<tr>.yaml`. Do NOT
  touch `metadata/EVIDENCE-FRESHNESS.md`.

On a Pending run, leave `support.implemented: false` and stop here with the
recorded reason.

## 7. Docgen: banner page + count bump (same commit as the flip)

In `crates/ls-docgen/src/lib.rs`: add the TR to `banner_trs` and bump the
`reference.len()` literal by one — ONLY on a clean in-window flip, in the SAME
commit as the `support.implemented` flip. A Pending TR contributes nothing to
`banner_trs` or the count. Then `make docs`.

## 8. Gate and commit

```
make docs
cargo test                 # workspace (includes the offline order gate)
cargo test -p ls-core      # metadata re-validation + policy cross-check
make docs-check
```

If a gate is red: fix the mechanical miss (the crosscheck single-list registration,
the banner list, the count); if substantive, `git checkout` the TR's changes and
PEND with a recorded reason. Never leave the tree red.

Stage only this TR's files and commit:

```
feat(orders): implement <tr> with a guarded paper-order evidence run
```

Body: the evidence matrix result + observed `rsp_cd` set, the scope (paper, guarded,
single matrix), the no-retry/dedup/kill-switch dispatch, the single-list crosscheck
registration, and that it stays non-recommended. Then emit the final
machine-readable line.

## Reference

- `docs/design/order-safety-design.md` — the authoritative order-safety contract
  (§1 no-retry, §2 dedup eviction, §3 reconciliation, §4 guarded manual evidence,
  §5 redaction).
- `docs/adr/0008-defer-order-runtime-until-safety-package-is-complete.md` — the
  deferral the first order package retired.
- In-repo exemplars: `crates/ls-sdk/src/orders/mod.rs` (`CSPAT00601` submit +
  `t0425` read), `crates/ls-sdk/src/orders/reconcile.rs` (the matcher + redacting
  record), `crates/ls-sdk/tests/order_smoke.rs` (the guarded harness),
  `crates/ls-sdk/tests/order_logic_gate.rs` (the offline gate),
  `crates/ls-core/src/inner.rs` (`post_order`, `classify_order_rsp_cd`),
  `crates/ls-core/src/order_dedup.rs` (the dedup window).
- `.agents/skills/implement-tr/SKILL.md` / `.agents/skills/implement-realtime-tr/SKILL.md`
  — the read and realtime sibling recipes this one mirrors.
- `.agents/skills/track-tr/SKILL.md` — the prerequisite rung (raw → Tracked).
