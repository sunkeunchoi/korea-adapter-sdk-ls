---
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
date: 2026-06-30
status: implementation-ready
---

# Open-Window Domestic Flip Wave - Plan

Product Contract preservation: unchanged — planning enriched the requirements in
place (Planning Contract, Implementation Units, Verification Contract, Definition
of Done added); no R-IDs altered.

---

## Goal Capsule

**Objective.** Capitalize on a live KRX window to flip up to **10 already-Tracked
domestic TRs** from `implemented: false` to Implemented on in-session live paper
smokes. The raw pool is exhausted (0 untracked left), so this is a **pure
Tracked→Implemented flip wave** — no track-from-raw rung. Non-empty smokes flip;
empty or blocked smokes are recorded PENDING with an updated reason.

**Product authority.** Operator confirmed (2026-06-30): in-session execution — the
agent runs raw probes + `make live-smoke-<tr>` against the paper gateway while the
window is live; target = 8 window-gated reads + 2 marginal domestic reads;
non-yielders get a PENDING disposition and **no Rust** (kept lean, not pre-staged).

**Open blockers.** Window perishability — KRX regular session (~09:00–15:30 KST)
must stay open through the window-sensitive units (U1 probe, U3 smoke). No others;
credentials are a gitignored `.env` with `LS_TRADING_ENV=paper`.

---

## Product Contract

### Problem

The provisional ledger (`metadata/PROVISIONALITY-LEDGER.md:754`) carries a
**"session-dependent (35)"** cohort deferred *unsmoked* to "a future open-window
wave" — reads that closure guarantees return empty `00707`. Most of that cohort has
since flipped (waves #68–#70). The residual still waiting on an open window is this
wave's target. With the raw pool drained, these session-gated flips are the
remaining yield-bearing pool; after them, what's left is structurally blocked.

### Target set (10 domestic TRs)

| TR | Owner | Domain | Confidence | Note |
|----|-------|--------|-----------|------|
| t1109 | paginated | stock | high | session-dependent residual |
| t1951 | paginated | stock | high | intraday/session read |
| t1954 | market_session | stock | high | session-dependent |
| t1973 | paginated | stock | high | session-dependent |
| t2212 | paginated | futures_options | high | session-dependent chart |
| t2407 | paginated | futures_options | high | session-dependent chart |
| t8404 | paginated | futures_options | high | session-dependent chart |
| t8427 | market_session | futures_options | high | session-dependent |
| t2106 | market_session | futures_options | low | F/O price-memo — **already fully carried** (structs in `market_session/quote_deriv.rs`, `T2106_POLICY` registered, `live_smoke_t2106` exists). Metadata-flip-only — no U2/U3/U5 work. Smoked empty even open (wave #66); memo populates intra-session, re-attempt |
| t1964 | standalone* | stock | low | ELW board — real blocker is 10 filter-enum defaults, not the window. `*`owner_class is a placeholder; route to **market_session** if it yields (the `standalone` handle is OAuth-only). Likely terminal state is **HELD** (input-unresolved), not a flip |

### Requirements

- **R1.** Each of the 10 TRs reaches exactly one terminal disposition: flipped
  Implemented (carried by a non-empty smoke witness on a substantive modeled field),
  recorded PENDING with an updated, faithful reason, or — for a TR the `implement-tr`
  recipe §0 bails on a recorded structural blocker (t1964's input-unresolved filter
  enums) — recorded HELD.
- **R2.** Yield floor is informational, not a gate — a low-yield wave that faithfully
  dispositions all 10 is a success. Realistic expectation: most of the 8 high-
  confidence reads flip; the 2 marginals likely stay PENDING.
- **R3.** Full gate green: `make docs`, `cargo test`, `cargo test -p ls-core`,
  `make docs-check`.
- **R4.** Each new REST `{TR}_POLICY` const is registered in **both** crosscheck
  lists per the `implement-tr` recipe, with the matching use-import.
- **R5.** Count families move correctly on flips: docgen `reference.len` and
  `banner_trs` bump by the flip count; `maintained_tr_count`, `cli.rs` shape-count
  literals, and `api_drift` counts **do not** move (tracked→implemented is not a
  tracking event).
- **R6.** `recommended: false` stays for every flip (Recommended is a separate
  promote pass).
- **R7.** Every committed smoke line is credential-free (only `rsp_cd`, structural
  counts, public tickers); never reference `rsp_msg`; err-path emits no capturable
  `LIVE-SMOKE` line.
- **R8.** Every captured response body that becomes a committed offline test fixture
  (U2) is first run through `scrub_secrets` (`crates/ls-sdk/tests/order_smoke.rs:506`)
  and manually inspected for `rsp_msg`/account/identifier content before it is
  committed. The U1 raw-probe screen and the U2 fixture are the only paths that touch
  a raw body; `raw_http_probe` itself emits only `http`/`rsp_cd`/`body_len`, so the
  body-capture step is explicit and scrubbed, not implied by the probe.

### Scope boundaries

**Deferred for later (not this wave):**
- Not window-gated: overseas `o3107`/`o3127`, account `t0441`/`CSPBQ00200`
  (account/contract/funding-dependent — an open KRX window does not unblock them).
- Recommended promotion for any flipped TR (separate `promote-tr` pass).

**Outside this wave's identity (will not flip here):**
- `paper_incompatible: true` set (CCENQ pair, g3101–g3106, g3190, t8455, t8460) —
  never flip on paper.
- Structurally blocked: `t1631` (IGW40014 — gateway fails to serialize its own
  response; environmental, all-String request, not a wire-type fix), `t3102`
  (needs a live `NWS` realkey; off-hours base rate ~0), `t1852`/`t1856` (require
  `sFileData` input), `t1860` (realtime-control subscription, not a read).

### Deferred to Follow-Up Work

- Pre-staging Rust carriers for non-yielders (t2106/t1964 if empty) — explicitly
  out per operator decision; a future open-window wave re-probes and builds them.

### Dependencies / assumptions

- Market stays open through U1 (probe) and U3 (smoke) (perishable; user-asserted
  open 2026-06-30).
- Paper gateway carries live data for these domestic feeds during the session
  (held for the prior open-window cohort #68–#70).

---

## Planning Contract

### Key Technical Decisions

**KTD1 — Raw-probe-first window strategy.** The 8 reads were deferred *unsmoked* and
have no Rust carrier yet. Authoring 10 carriers before knowing which yield would risk
burning the window. Instead, U1 raw-probes all 10 credential-safe (no struct needed)
and captures the response body of each yielder *during the window*. That captured
body becomes both the U2 offline deserialize-test fixture and the flip witness — a
session-stale non-empty body gates the flip identically to a live one (ledger R5).
The typed live smoke (U3) confirms the round-trip and should also run in-window, but
the captured non-empty body is the load-bearing witness.

**KTD2 — Non-yielders get PENDING, no Rust** (operator-confirmed). A TR that
U1 raw-probes empty/no-data (expected for t2106 memo and t1964 board) is recorded
PENDING with an updated reason and skipped for all Rust units — keeping the wave lean
over pre-staging a future flip.

**KTD3 — Numeric request fields serialize as JSON numbers.** Any request-body numeric
(cnt/idx/cursor/nrec) uses `#[serde(serialize_with = "ls_core::string_as_number")]`
or the gateway returns `IGW40011`. Treat an IGW40011 in U1/U3 as a wire-type fix to
apply, not an environmental PENDING. Response fields use `ls_core::string_or_number`
(tolerant). Source field names/types/array-shape from the normalized baseline
`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json` (`response_blocks`;
`<tr>OutBlock` = scalar, `<tr>OutBlock1` = repeated row), never guesswork.

**KTD4 — Fire the typed smoke before crosscheck registration.** The crosscheck lists
are test-only; the carrier compiles and smokes without them. So U3 (window-sensitive
smoke) depends only on U2, and U5 (registration) + U6 (docgen/gate) complete offline
after the window closes. This salvages the window if registration/docgen runs long.

**KTD5 — Owner-class routing.** market_session non-paginated reads (t1954, t8427)
route through `Inner::post` with structs in `crates/ls-sdk/src/market_session/`;
paginated reads (t1109, t1951, t1973, t2212, t2407, t8404) route through
`Inner::post_paginated` with structs in `crates/ls-sdk/src/paginated/` and
`ls_core::impl_has_pagination!` + `has_pagination: true` on the policy. **t2106 is
already fully carried** (structs `T2106InBlock`/`OutBlock`/`OutBlock1` in
`market_session/quote_deriv.rs`, facade `market_session/mod.rs:856`, `T2106_POLICY`
in `endpoint_policy/order.rs` already in *both* crosscheck lists, `live_smoke_t2106`
wired) — for t2106, skip U2/U3/U5; a yielding t2106 needs only the U4 metadata flip +
U6 count bump. **t1964** has `owner_class: standalone` as a placeholder, but the
`standalone` handle is OAuth-only (no `post`/`post_paginated`) — if it yields, author
it under market_session and correct `owner_class` to `market_session` in U4 (per
`implement-tr` SKILL). New chart-read policy consts go in
`crates/ls-core/src/endpoint_policy/market_data.rs`. Mirror existing carriers:
`crates/ls-sdk/src/market_session/quote.rs` (T1102),
`crates/ls-sdk/src/paginated/designation_board.rs` (T1404).

### Count-site checklist (apply per flip, in the flip commit)

| Site | File:line | Moves on flip? |
|------|-----------|----------------|
| `banner_trs` array | `crates/ls-docgen/src/lib.rs:1103` | **yes** — add TR code |
| `reference.len()` assertion | `crates/ls-docgen/src/lib.rs:1385` | **yes** — +1 per flip |
| `maintained_tr_count` | `crates/ls-trackers/tests/api_drift.rs:106` | no |
| shape-count literals | `crates/ls-trackers/src/cli.rs` | no |

---

## Implementation Units

### U1. In-window raw-probe screen [WINDOW-SENSITIVE]

- **Goal.** Determine which of the 10 TRs return live non-empty data right now, and
  capture each yielder's response body as a fixture + witness.
- **Requirements.** R1, R2, KTD1, KTD2.
- **Dependencies.** none (run first, while window open).
- **Files.** none committed in this unit (probe outputs captured to scratch for U2
  fixtures); uses the credential-safe `raw_http_probe` path (per AGENTS.md
  `make raw-probe` / direct cargo invocation).
- **Approach.** For each TR, build the request body from the normalized baseline
  request fields (KTD3 numeric handling) and probe the gateway. **Two-step probe:**
  `raw_http_probe` (per AGENTS.md) emits only `http`/`rsp_cd`/`body_len` — use it to
  classify yield without surfacing the body. For a yielder, capture the body with an
  explicit body-emitting call (a throwaway typed/raw fetch in the session) and
  immediately run it through `scrub_secrets` (R8) before writing it to scratch as a
  U2 fixture seed. Classify each: **yielder** (success `rsp_cd` + non-empty out-block)
  → carry to U2 with its scrubbed captured body; **empty** (`00707`/empty out-block)
  → PENDING (no Rust, KTD2); **IGW40011** → fix the numeric request field and re-probe
  before classifying. Probe directly via cargo (not `make` — `make` breaks in
  spawned/eval shells per prior waves). t2106 already has `live_smoke_t2106`; probe it
  via that path. t1964 is expected empty/HELD (filter-enum blocker).
- **Patterns to follow.** `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`;
  AGENTS.md raw-probe section.
- **Test scenarios.** Test expectation: none — this is a credential-safe live probe,
  not committed code. Verification is the captured yielder/empty classification list.
- **Verification.** Each of the 10 TRs is classified yielder or empty, with yielder
  bodies captured for U2.

### U2. Author callable carriers + offline decode tests (yielders only)

- **Goal.** For each U1 yielder **that needs a carrier** (all except t2106), author
  the request/response structs, facade method, and policy const, with offline
  deserialize tests passing against the scrubbed captured body. t2106 is already
  carried — skip it here (flip-only in U4/U6).
- **Requirements.** R1, R7, R8, KTD3, KTD5.
- **Dependencies.** U1.
- **Files (per yielder, by owner class):**
  - market_session yielders (t1954/t8427/t2106 if yielding): structs in the
    appropriate `crates/ls-sdk/src/market_session/*.rs`; facade method on the
    market_session handle.
  - paginated yielders (t1109/t1951/t1973/t2212/t2407/t8404 if yielding): structs in
    the appropriate `crates/ls-sdk/src/paginated/*.rs` with
    `ls_core::impl_has_pagination!`; facade method on the paginated handle.
  - t1964 (if yielding, not HELD): author under `crates/ls-sdk/src/market_session/`,
    NOT the OAuth-only standalone handle (KTD5); correct `owner_class` in U4.
  - policy const `{TR}_POLICY` in `crates/ls-core/src/endpoint_policy/market_data.rs`.
  - offline tests colocated with each carrier module.
- **Approach.** Request: `InBlock` + wrapper with `#[serde(rename = "<tr>InBlock")]` +
  `::new()` constructor; numeric request fields use `string_as_number` (KTD3).
  Response: `OutBlock`/`Vec<OutBlock1>` (via `ls_core::de_vec_or_single` where the row
  is repeated), every numeric field `deserialize_with = "ls_core::string_or_number"`,
  all structs derive `Default` + `#[serde(default)]`. Policy const fields sourced from
  the baseline (`tr_code`, `path`, `module`, `group`, `protocol: Rest`, `is_order:
  false`, `has_pagination` = `self_paginated`, rate limits).
- **Execution note.** Author the offline deserialize test first against the U1-captured
  body, then make it pass — the recipe requires offline tests before the live call.
- **Patterns to follow.** `crates/ls-sdk/src/market_session/quote.rs` (T1102),
  `crates/ls-sdk/src/paginated/designation_board.rs` (T1404),
  `crates/ls-core/src/endpoint_policy/market_data.rs` (T1102_POLICY / T1452_POLICY).
- **Test scenarios (per yielder carrier):**
  - Captured success body deserializes; at least one non-key field holds a real
    (non-default) value.
  - Each numeric field parses via `string_or_number` from **both** string and number
    JSON forms.
  - Empty result (`rsp_cd` 00707, empty out-block) deserializes cleanly.
  - `::new(...)` serializes the in-block under the correct `serde(rename)` key.
  - (paginated) the pagination cursor field serializes as an ordinary in-block field,
    not `#[serde(skip)]`.
  - **Fixture safety (R8):** the committed fixture body contains no `rsp_msg` field
    and no account-shaped token (assert the fixture was scrubbed).
- **Verification.** `cargo test -p ls-sdk` green for the new offline tests; carriers
  compile; no committed fixture carries `rsp_msg` or account-identifying content.

### U3. Smoke harness + in-window typed paper smokes [WINDOW-SENSITIVE]

- **Goal.** Add a typed live-smoke per yielder and run it against the paper gateway
  while the window is open, capturing a credential-free non-empty witness.
- **Requirements.** R1, R7, KTD1, KTD4.
- **Dependencies.** U2 (carrier must exist to fire a typed call).
- **Files.** `crates/ls-sdk/tests/live_smoke.rs` (one `live_smoke_<tr>` fn per
  yielder); `Makefile` (`live-smoke-<tr>` target + `.PHONY`);
  `.agents/skills/promote-tr/references/smoke-map.md` (one row per yielder,
  Promotion = `implemented-only`).
- **Approach.** Each smoke opens `paper_sdk()`, fires the typed facade call, and on
  success records a `LIVE-SMOKE` line via `smoke_result`/`record` containing only
  `rsp_cd` + out-block length; on error emits the `SMOKE-FAIL` stderr pattern and
  panics (no capturable success line). Run the smokes in-window; a yielder whose smoke
  returns non-empty is cleared to flip in U4. If the window closes before a typed
  smoke runs, the U1-captured non-empty body still gates the flip (KTD1/R5).
- **Patterns to follow.** `crates/ls-sdk/tests/live_smoke.rs` (`live_smoke_t8425`);
  `Makefile` run_smoke macro + `live-smoke-t8425` target; `smoke-map.md` row format.
- **Test scenarios.** Covers R1. The smoke itself is the witness: assert a non-empty
  out-block before recording a flip-eligible result; empty `00707` records PENDING.
  Offline, no separate unit tests for the smoke fns.
- **Verification.** Each yielder smoke ran in-window with a recorded credential-free
  result; flip-eligible set identified.

### U4. Flip metadata + retire ledger facets; record PENDING dispositions

- **Goal.** Flip each cleared yielder to Implemented; record a PENDING disposition for
  every non-yielder.
- **Requirements.** R1, R6, KTD2.
- **Dependencies.** U3.
- **Files.** `metadata/trs/<tr>.yaml` + `metadata/tr-index.yaml` (per flip:
  `support.implemented: true`, `support.recommended: false`, no `recommendation`
  block, no `metadata/evidence/<tr>.yaml`, no `EVIDENCE-FRESHNESS.md` touch);
  `metadata/PROVISIONALITY-LEDGER.md` (retire `venue_session` +
  `caller_supplied_identifiers` for flips; update the PENDING row reason for
  non-yielders — e.g. "session-dependent, empty board under live window"; keep t1964
  filter-enum / t2106 intra-session reasons).
- **Approach.** Flip only TRs with a non-empty witness. Retire only the confirmed
  facets (`venue_session`, `caller_supplied_identifiers`); do **not** retire
  field-level `type` facets (deserialize does not confirm HTTP-500-seeded types).
- **Test scenarios.** Test expectation: none — metadata/ledger edits validated by the
  U6 gate (`cargo test -p ls-core` metadata validation + crosscheck).
- **Verification.** Every one of the 10 TRs carries exactly one terminal disposition
  (Implemented or updated PENDING).

### U5. Register policy consts in crosscheck + endpoint-policy lists

- **Goal.** Register each flipped TR's `{TR}_POLICY` const so the policy-index
  crosscheck stays green.
- **Requirements.** R4.
- **Dependencies.** U2 (const exists); semantically pairs with U4 flips.
- **Files.** `crates/ls-core/tests/policy_index_crosscheck.rs` (add const to the
  `policies` array + its use-import); `crates/ls-core/src/endpoint_policy/mod.rs` (add
  const to the `slice_rest_policies_are_non_order_rest` test array + its use-import).
- **Approach.** Both lists, both use-imports, per flipped REST read. All targets are
  non-order REST reads (`is_order: false`), so each belongs in both lists.
- **Test scenarios.** Covers R4. Validated by `cargo test -p ls-core`
  (`slice_policies_mirror_metadata_index`, `slice_rest_policies_are_non_order_rest`).
- **Verification.** `cargo test -p ls-core` green.

### U6. Docgen banner/count bump + full gate

- **Goal.** Bump docgen counts for the flips, regenerate docs, and run the full gate
  green.
- **Requirements.** R3, R5.
- **Dependencies.** U4, U5.
- **Files.** `crates/ls-docgen/src/lib.rs` (`banner_trs` array at line 1103 — add each
  flipped TR; `reference.len()` assertion at line 1385 — +1 per flip); regenerated
  `docs/` artifacts.
- **Approach.** Bump only `banner_trs` + `reference.len()` (R5 + Count-site checklist);
  `maintained_tr_count`, `cli.rs` literals, and `api_drift` counts must NOT change —
  if they drift, a tracking-vs-flip mistake was made. Run the full gate.
- **Test scenarios.** Test expectation: none beyond the gate — the count assertions
  are the test.
- **Verification.** `make docs`, `cargo test`, `cargo test -p ls-core`,
  `make docs-check` all green.

---

## Verification Contract

- **Gate (run before commit):** `make docs` → `cargo test` → `cargo test -p ls-core`
  → `make docs-check`, all green (AGENTS.md gate).
- **Per-flip witness:** a credential-free non-empty out-block from U1 raw-probe and/or
  U3 typed smoke.
- **Count integrity:** `banner_trs` + `reference.len()` increased by exactly the flip
  count; `maintained_tr_count` / `cli.rs` literals / `api_drift` unchanged.
- **Crosscheck integrity:** `slice_policies_mirror_metadata_index` and
  `slice_rest_policies_are_non_order_rest` green with every new const registered.
- **Secret-safety (R7):** no committed line references `rsp_msg`; err-path emits no
  capturable `LIVE-SMOKE` line; only `rsp_cd`, counts, and public tickers appear.

## Definition of Done

1. All 10 target TRs carry exactly one terminal disposition (Implemented with a
   non-empty witness; PENDING with an updated faithful reason; or HELD where the
   recipe §0 bails on a recorded structural blocker, e.g. t1964).
2. Every flip: `support.implemented: true`, `support.recommended: false`, no evidence
   block; policy const registered in both crosscheck lists + use-imports.
3. Ledger facets retired for flips (`venue_session`, `caller_supplied_identifiers`);
   PENDING rows updated for non-yielders.
4. Docgen `banner_trs` + `reference.len()` bumped by the flip count; no other count
   family moved.
5. Full gate green; all committed smoke output credential-free.
