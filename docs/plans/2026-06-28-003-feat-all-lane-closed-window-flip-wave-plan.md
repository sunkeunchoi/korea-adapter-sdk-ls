---
title: All-Lane Closed-Window Flip Wave - Plan
type: feat
date: 2026-06-28
topic: all-lane-closed-window-flip-wave
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

# All-Lane Closed-Window Flip Wave - Plan

## Goal Capsule

- **Objective:** Pull every trackable read TR from the remaining raw pool — across all four instrument domains (domestic, overseas, options, futures) and both transports (REST + realtime WebSocket) — up to Tracked, and flip to Implemented every one that clears a paper smoke while KRX is closed.
- **Product authority:** Repo owner (sunkeunchoi). Premise (closure-flippable reads yield on paper) is proven across PRs #56–#64.
- **Open blockers:** None blocking planning. Per-lane yield is empirical, not pre-known — the wave's flip count is discovered by raw-probe, not committed up front.
- **Product Contract preservation:** Unchanged by planning. R11a/R11b were added during the requirements review pass, not by `ce-plan`.

## Product Contract

### Summary

A breadth wave over the ~143 raw untracked TRs. Bring every eligible read TR to Tracked regardless of lane; flip to Implemented only those the paper gateway actually serves under market closure. Tracking and implementing are decoupled per lane so honest non-yielding lanes (overseas, night-derivative REST) still raise maintained coverage without forcing false flips.

### Problem Frame

The maintained surface sits at 222 Tracked / 169 reference pages after many prior closure waves. Two pools still hold value: 54 Tracked-but-not-Implemented TRs and ~143 raw untracked TRs. Prior waves mined these narrowly — usually one lane (domestic REST) at a time — leaving overseas, F/O, and realtime raw codes un-tracked and unattributed in the inventory. KRX being closed is the operating reality, not a wait condition: it constrains *which* TRs flip (session-independent master/reference/account-capacity reads and realtime reachability) but does not constrain *which* TRs can be tracked. Mining all lanes at once closes the inventory gap and harvests every closure-viable flip in a single pass.

### Key Decisions

- **Raw → Track → Implement, full pipeline.** The wave runs the complete pipeline (raw capture → `metadata/trs/<tr>.yaml` + tr-index + projected baseline → callable Rust + paper smoke), not just flips of already-Tracked TRs. The user's "Traced + Implemented" names both rungs.
- **All four lanes, both transports.** Domestic, overseas, options, and futures are all in scope, across REST and realtime WebSocket. No lane is excluded a priori from *tracking*.
- **Tracking and flipping are decoupled per lane.** Everything eligible gets Tracked. Implemented flips happen only where a paper smoke yields under closure. A lane that tracks but cannot flip is a successful outcome for that lane, not a failure.
- **Breadth over a yield-floor.** Probe the whole flippable subset and accept honest per-lane yield. No target flip count gates the wave.
- **WebSocket realtime is closure-independent.** Realtime channels flip on connection-reachability (the gateway never signals data rejection — KTD6 not-observable), matching the standard set by the prior 31-channel realtime wave. Live market data is not required for a WS flip.
- **Faithful disposition, never force-flip.** Overseas / night-derivative REST that returns `01900` (venue-incompatible) or empty `00707` (paper-unavailable) is recorded with the correct facet and left Tracked — not coerced into Implemented.

### Requirements

**Tracking sweep (all lanes)**

- R1. Every eligible read TR in the raw capture (complete request + response blocks, read-only — no order/register/mutation) is brought to Tracked: `metadata/trs/<tr>.yaml` + `tr-index.yaml` entry + normalized baseline projected via `make api-drift-renormalize` (projected, never hand-authored). The tracking recipe is selected by transport: REST reads track under `track-tr` (which HELDs realtime/WS TRs out of scope by design); raw WebSocket push channels track under `track-realtime-tr`.
- R2. The sweep spans all four instrument domains (domestic, overseas, options, futures) and both transports. Raw codes that are realtime WebSocket channels are tracked as `owner_class: realtime` via `track-realtime-tr`; REST reads are tracked under their REST owner class via `track-tr`.
- R3. Order, register, and other mutation TRs are excluded from this wave — read-only only.

**Flip gate (REST, under closure)**

- R4. A Tracked REST read flips to Implemented only when a representative paper call builds, sends, and deserializes a **non-empty** success — the substantive modeled field is asserted as the smoke witness, not merely `body_len` or a bare `00000`.
- R5. Domestic master / reference / static reads are the primary REST flip lane and are expected to yield under closure.
- R6. F/O (options/futures) account and capacity reads flip via the per-account credential lane mechanism (`LS_SMOKE_LANE` by instrument domain, proven in §16–17); a read that is reachable but returns all-default/zero rows under closure is recorded with a deserializable all-default witness where that proves the shape, otherwise left PENDING.

**WebSocket realtime lane**

- R7. Tracked realtime channels flip to Implemented on a clean lifecycle smoke (connection-reachable, subscribe/unsubscribe), independent of market hours. The flip is reachability-only; no per-channel data proof is required or available.
- R8. Each new WebSocket `{TR}_POLICY` (`owner_class: realtime`) registers in the crosscheck list **only**, never the REST-only `slice_rest_policies_are_non_order_rest` list.

**Disposition discipline**

- R9. A Tracked REST read that returns `01900` (venue-incompatible) is recorded `paper_incompatible: true` and left Tracked — no flip attempt, no re-probe of already-confirmed `01900` reads (CCENQ10100/CCENQ90200 and the §14/§15 overseas/night set stay as-is).
- R10. A Tracked REST read that returns empty `00707` under in-window closure is recorded paper-unavailable / PENDING with the witness reason, and left Tracked.
- R11. Every candidate probed reaches one disposition — Implemented, PENDING (with reason), or paper_incompatible — recorded in `metadata/PROVISIONALITY-LEDGER.md`. No candidate is silently dropped.
- R11a. The raw-probe pre-screen drops any candidate whose probe matches an already-recorded dry terminal (`01900`, or empty `00707` on a lane proven empty in §14–17) from the **tracking** target, not just the flip target — the wave does not re-author metadata + baseline for codes prior waves already dispositioned as non-yielding.

**Credential & evidence safety**

- R11b. Every new smoke harness routes all output, panic, and `metadata/PROVISIONALITY-LEDGER.md` witness/record paths through the shared secret-scrubber (account-number + bearer-token masking), and suppresses the `ls_core` dispatch debug events that log whole response bodies during flip smokes. Asserting the modeled witness (R4) must not echo a raw account identifier or `rsp_msg` into committed evidence or operator logs — closing the documented leak class across all newly-smoked lanes (domestic, overseas, F/O account formats).

**Gate & registration hygiene**

- R12. Each new REST `{TR}_POLICY` const is registered in **both** cross-check lists per the `implement-tr` recipe; the typed in-window smoke fires before registrations since the crosscheck lists are test-only.
- R13. All count sites are kept internally consistent per the tracking-vs-implementing split (see KTD7): tracking a TR bumps `maintained_tr_count` (manifest + `api_drift.rs` + `cli.rs` literals) and docgen `TRACKED_TRS`; implementing a TR bumps docgen `banner_trs` + `reference.len` only. The baseline manifest `refreshed` date stays at the last raw-refresh date (do not bump it).
- R14. The full gate stays green at every commit: `make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`. Never commit on a red gate. Do not `cargo fmt` the whole `ls-trackers` crate.

### Acceptance Examples

- AE1. Domestic master/reference flip (covers R4, R5).
  - **Given:** a Tracked domestic master read whose raw baseline shows a populated out-block.
  - **When:** the in-window paper smoke runs under closure.
  - **Then:** it returns a non-empty success, the modeled witness field is asserted, and the TR flips to Implemented — `reference.len` increments.

- AE2. Overseas REST under closure (covers R9, R10).
  - **Given:** a raw overseas read tracked this wave.
  - **When:** its paper smoke runs and returns `01900` (or empty `00707`).
  - **Then:** it stays Tracked, is recorded `paper_incompatible`/PENDING with the witness, and `maintained_tr_count` increments while `reference.len` does not.

- AE3. WebSocket realtime flip (covers R7, R8).
  - **Given:** a Tracked realtime channel.
  - **When:** the lifecycle smoke connects and subscribes cleanly.
  - **Then:** it flips to Implemented on reachability alone, registered in the crosscheck list only.

- AE4. F/O account read, funded-but-empty (covers R6).
  - **Given:** a tracked F/O balance read on a funded cash-only account with no open positions.
  - **When:** the per-lane smoke runs under closure.
  - **Then:** if a deserializable all-default row proves the shape it flips; otherwise it is recorded PENDING (funded, no positions), not force-flipped.

### Scope Boundaries

- Promotion to Recommended is out of scope — that is a separate `promote-tr` pass with Focused Evidence (ADR 0008 lineage). Every flip lands at `implemented: true`, `recommended: false`.
- Order / mutation / register TRs are out of scope (read-only wave).
- Already-dispositioned `paper_incompatible` reads re-confirmed in prior waves (CCENQ10100/CCENQ90200 venue `01900`) are not re-probed.
- Hard-blocked Tracked reads needing a non-closure input (t1852/t1856 `sFileData`, t3102 `sNewsno`, t1860 realtime-control, t1964 empty ELW board) are not in the flip target — track-state only.
- No new schema, endpoint, or runtime architecture — this wave uses the existing `track-tr` / `implement-tr` / `implement-realtime-tr` recipes and dispatch path.

### Dependencies / Assumptions

- Paper gateway reachable with valid `.env` credentials, `LS_TRADING_ENV=paper`, and the per-account credential lanes (`LS_SMOKE_LANE`) established in plan -002.
- `make raw-probe` is the credential-safe pre-screen used to classify each candidate (http / rsp_cd / body_len only) before committing to a track+build.
- KRX remains closed for the duration of the wave — this is the operating assumption that scopes which REST lanes can flip. A mid-wave open window only *expands* what flips; it does not invalidate any closure-flipped TR.
- Assumption (unverified per-candidate until probed): the raw pool's short alphanumeric codes (`B7_`, `DH0`, `C02`, …) are realtime WS channels routing to the WS lane; the `CIDBQ*`/`COSAQ*`/`CSPAQ*`/`t01xx` codes are domestic REST reads. Raw-probe confirms each before tracking.

### Outstanding Questions

**Resolve before planning**

- None blocking. The wave is structurally defined; candidate-level classification is execution work.

**Deferred to planning**

- PR structure: single wave PR vs. stacked-by-lane PRs. The breadth (potentially 30–50 tracked + a smaller flipped subset) likely warrants stacking by domain to keep diffs reviewable — `ce-plan` decides the cut.
- Candidate enumeration: the exact eligible subset of the ~143 raw codes per lane is produced by the raw-probe pre-screen during execution, not pinned here.
- Whether the 54 already-Tracked TRs' still-flippable members (if any survive prior dispositions) get folded in opportunistically or left for a follow-up — the user scoped this wave to the raw pool, but adjacent already-Tracked flips are cheap if encountered.

### Sources / Research

- Grounding dossier: `/tmp/compound-engineering/ce-brainstorm/krx-flip-1782634131/grounding.md` (raw/tracked pool counts, current count assertions, prior-wave dispositions).
- Raw capture: `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json` (365 codes).
- Normalized baselines (wire source of truth): `crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`; manifest `crates/ls-trackers/baselines/api-drift/normalized/manifest.json` (`maintained_tr_count: 222`).
- Count assertions: `crates/ls-docgen/src/lib.rs` (banner_trs array, `reference.len()` assert); `ls-trackers` `cli.rs` count literals.
- Disposition ledger: `metadata/PROVISIONALITY-LEDGER.md` §14–17 (overseas/night paper-unavailable, closure batches, account-lane corrections).
- Recipes: `.agents/skills/track-tr/SKILL.md`, `.agents/skills/track-realtime-tr/SKILL.md`, `.agents/skills/implement-tr/SKILL.md`, `.agents/skills/implement-realtime-tr/SKILL.md`.
- Prior closure waves: PRs #56 (order certify), #58–#64 (closure flip / account-lane / credential-lane waves).
- Implementation grounding (this plan): recipe step sequences and count-site locations extracted from `.agents/skills/{track-tr,track-realtime-tr,implement-tr,implement-realtime-tr}/SKILL.md`; exemplars `metadata/trs/t8425.yaml` (REST) and `metadata/trs/K3_.yaml` (realtime).

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Classify before authoring.** A raw-probe pre-screen (U1) produces a committed lane/transport classification table — each candidate code mapped to transport (REST vs WebSocket), `owner_class`, `instrument_domain`, and predicted disposition — *before* any metadata is authored. This resolves the "which short codes are WS channels vs REST reads" routing risk at classification time rather than after authoring metadata + baseline that must then be reworked. Candidates whose probe matches an already-recorded dry terminal (`01900`, or `00707` on a lane proven empty in §14–17) are dropped from the **tracking** target, not just the flip target (R11a).
- KTD2. **Two recipe families, selected by transport.** REST reads use `track-tr` then `implement-tr`; WebSocket channels use `track-realtime-tr` then `implement-realtime-tr`. A WebSocket `{TR}_POLICY` registers in the `policies` crosscheck array **only** (`crates/ls-core/tests/policy_index_crosscheck.rs`), never the REST-only `slice_rest_policies_are_non_order_rest` list in `crates/ls-core/src/endpoint_policy.rs`.
- KTD3. **Numeric request-body fields serialize as JSON numbers.** Any numeric request field (pagination `idx`, counts, bounds) uses `#[serde(serialize_with = "ls_core::string_as_number")]` or the gateway returns `IGW40011` (a wire-type defect, not environmental — see `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`). Response fields stay tolerant via `ls_core::string_or_number`. Offline tests assert `.is_number()` on each numeric request field.
- KTD4. **Flip gates differ by transport.** REST flip = a paper smoke deserializes a **non-empty** success and a substantive modeled field is asserted as the witness — `body_len`, a bare `00000`, an all-default/all-zero row, or `00136` do **not** count as data (route to PENDING). WebSocket flip = a clean connect/subscribe/unsubscribe lifecycle smoke; because subscribe is fire-and-forget (KTD6 not-observable), the flip is **connection-reachable-only** and the metadata records it as such, gated on the `live-smoke-ws-negative` control returning its expected silence.
- KTD5. **Credential safety is mandatory on every new smoke (R11b).** All new smoke harnesses route output/panic/ledger-record paths through the `scrub_secrets` helper (masks account numbers + 20+ char tokens, spares order numbers) and suppress the `ls_core` dispatch debug events via the existing `install_dispatch_log_suppressor()` (process-global tracing subscriber, fail-closed if a foreign subscriber exists) so account-bearing response bodies never reach `RUST_LOG=debug` output. Note: `scrub_secrets` currently lives as a test-local fn in `crates/ls-sdk/tests/order_smoke.rs` and the suppressor already exists in `crates/ls-sdk/tests/live_smoke.rs` — U2 hoists the scrubber to a shared location so both the read-smoke and WebSocket-lifecycle harnesses reuse it. Read smokes record only credential-free `LIVE-SMOKE …` lines; realtime smokes never log raw frames.
- KTD6. **F/O reads route through per-account credential lanes.** LS account binding is token-bound, not number-bound: F/O and overseas reads smoke under the matching lane file (`.env.domestic_option` …51, `.env.overseas_option` …71) selected by a Makefile target-variable `LS_SMOKE_LANE` (fail-fast if the lane file is absent — never silent fallback). Rule out wrong-account binding before recording any `00707`; model the holdings TR first and assert its typed array is empty to downgrade a positions-dependent cohort to expected-empty (the holdings gate).
- KTD7. **Count-site discipline.** Tracking a TR bumps `maintained_tr_count` (`crates/ls-trackers/baselines/api-drift/normalized/manifest.json`, `crates/ls-trackers/tests/api_drift.rs`, `crates/ls-trackers/src/cli.rs`) and `TRACKED_TRS` (`crates/ls-docgen/src/lib.rs`) — never `banner_trs`/`reference.len`. Implementing a TR bumps `banner_trs` + `reference.len` only (the TR was already counted as tracked). After `make api-drift-renormalize`, revert `manifest.refreshed` to the prior raw-refresh date. Do not `cargo fmt` the whole `ls-trackers` crate. Repoint any support-aware test fixture that hard-codes a now-flipped TR as tracked-only (the "exemplar trap").

### Sequencing

U1 (classify) and U2 (safety scaffolding) gate everything — no metadata is authored and no smoke runs until both land. The lane sweeps then stack in yield-density order: domestic REST (U3→U4), F/O REST (U5→U6), overseas REST track-only (U7), WebSocket (U8→U9). U10 reconciles counts, runs the full gate, and closes dispositions. Each lane's track→flip pair is independently shippable, supporting stacked-by-lane PRs.

### Assumptions

- The paper gateway is reachable with valid `.env` + per-lane credential files; `LS_TRADING_ENV=paper`.
- KRX stays closed for the wave's duration. A mid-wave open window only expands what flips; it never invalidates a closure-flipped TR.
- The ~143 raw untracked codes include order/register/mutation TRs that R3 excludes; the eligible read-only subset (the wave's real tracking volume) is unknown until U1's pre-screen runs.

---

## Implementation Units

### U1. Raw-probe lane/transport classification table

- **Goal:** Produce a committed pre-screen artifact mapping every raw untracked candidate to transport, owner_class, instrument_domain, and predicted disposition — the wave's work-list and the routing source of truth.
- **Requirements:** R1, R2, R11a; resolves the lane-routing risk behind R7/R8/R12.
- **Dependencies:** none.
- **Files:** `docs/plans/notes/all-lane-flip-classification.md` (new working artifact); reads `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`, `metadata/PROVISIONALITY-LEDGER.md` (§14–17).
- **Approach:** Enumerate the ~143 raw untracked codes; drop mutation/order/register TRs (R3) and already-dispositioned dry terminals (§14–17, R11a). For each survivor run `make raw-probe LS_PROBE_TR_CD=.. LS_PROBE_PATH=.. LS_PROBE_BODY=..` (credential-safe: http/rsp_cd/body_len only) to confirm transport and a live/empty signal. Classify each as REST (domestic/overseas/F/O) or WebSocket; record predicted disposition (likely-flip / track-only / drop-as-dry).
- **Patterns to follow:** raw-probe A/B shape testing per `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`.
- **Test scenarios:** Test expectation: none — produces a classification artifact, not code. Verification is that every probed code resolves to exactly one transport + disposition and no candidate is unclassified.
- **Verification:** The table lists each in-scope code once with transport, owner_class, instrument_domain, and disposition; dry-terminal and mutation codes are explicitly excluded with reason.

### U2. Smoke-harness credential-safety scaffolding

- **Goal:** Guarantee every new smoke this wave adds is leak-safe before any smoke runs — shared scrub + debug-suppression + non-empty-witness assertion, covering both the read-smoke and WebSocket-lifecycle harnesses.
- **Requirements:** R4, R11, R11b.
- **Dependencies:** none (parallel with U1).
- **Files:** `crates/ls-sdk-test-support/` or a shared `crates/ls-sdk/tests/` module (hoist target for the scrubber), `crates/ls-sdk/tests/live_smoke.rs` (shared helpers + existing `install_dispatch_log_suppressor()`); source `scrub_secrets` from `crates/ls-sdk/tests/order_smoke.rs`.
- **Approach:** This is largely confirm-and-extend, not net-new: the fail-closed dispatch-log suppressor already exists as `install_dispatch_log_suppressor()` in `crates/ls-sdk/tests/live_smoke.rs`, and `scrub_secrets` already exists as a test-local fn in `crates/ls-sdk/tests/order_smoke.rs`. Hoist `scrub_secrets` into a shared location reusable by `live_smoke.rs` (sibling test binaries can't import each other's private fns), and confirm both the read-smoke helper AND the WebSocket-lifecycle helper (`ws_lifecycle_smoke`) route their recorded lines + panic output through it and install the suppressor. Add a reusable `assert_nonempty_witness(field)` helper that fails on default/zero/`00136`/empty so each lane's flip smoke asserts a substantive modeled field, not `body_len`.
- **Patterns to follow:** `docs/solutions/architecture-patterns/autonomous-order-smoke-fail-closed-contract.md` (scrubber + fail-closed subscriber); existing `live_smoke_t8425` record shape; existing `install_dispatch_log_suppressor()`.
- **Test scenarios:**
  - Happy path: a smoke line containing an account number + a 20+ char token is recorded scrubbed (masked), order-number-shaped digits preserved.
  - Edge: witness helper rejects an all-zero row and a `00136` response; accepts a populated modeled field.
  - Error path: when a foreign tracing subscriber is already installed, the suppressor fails closed (smoke aborts rather than logging unscrubbed bodies).
- **Verification:** A deliberately account-bearing response produces no unscrubbed account number in recorded output or at `RUST_LOG=debug`; the witness helper gates on a substantive field.

### U3. Track domestic REST candidates

- **Goal:** Bring the domestic-REST survivors from U1 to Tracked.
- **Requirements:** R1, R2, R13.
- **Dependencies:** U1.
- **Files:** `metadata/trs/<tr>.yaml` (new per TR), `metadata/tr-index.yaml`, `crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json` (projected), `manifest.json`; count sites `crates/ls-trackers/tests/api_drift.rs`, `crates/ls-trackers/src/cli.rs`, `crates/ls-docgen/src/lib.rs` (`TRACKED_TRS`).
- **Approach:** Per `track-tr`: author metadata (owner_class market_session/paginated/account; `protocol: rest`; `support.tracked: true`), add tr-index entry, run `make api-drift-renormalize`, confirm the drift guard shows only new baseline files, bump the tracked count sites (KTD7), revert `manifest.refreshed`.
- **Patterns to follow:** `metadata/trs/t1101.yaml` (market_session), `metadata/trs/t1452.yaml` (paginated), `metadata/trs/CSPAQ12200.yaml` (account).
- **Test scenarios:** Test expectation: none — metadata + projected baseline. Verified by `cargo test -p ls-metadata -p ls-core` + `make docs-check`.
- **Verification:** `cargo test -p ls-core` (metadata validation) green; `git diff` shows only new baseline files and a `maintained_tr_count` bump.

### U4. Flip domestic REST reads to Implemented

- **Goal:** Flip the domestic master/reference/static reads that clear a paper smoke under closure.
- **Requirements:** R4, R5, R12, R13; covers AE1.
- **Dependencies:** U2, U3.
- **Files:** `crates/ls-sdk/src/{market_session,paginated,account}/mod.rs` (structs + facade), `crates/ls-core/src/endpoint_policy.rs` (`{TR}_POLICY` + REST-only slice list), `crates/ls-core/tests/policy_index_crosscheck.rs` (policies array), `crates/ls-sdk/tests/market_session_tests.rs` (offline), `crates/ls-sdk/tests/live_smoke.rs` (`live_smoke_<tr>`), `Makefile` (`live-smoke-<tr>` + `.PHONY`), `.agents/skills/promote-tr/references/smoke-map.md`, `metadata/trs/<tr>.yaml` (flip), `crates/ls-docgen/src/lib.rs` (`banner_trs` + `reference.len`).
- **Approach:** Per `implement-tr`: author InBlock/OutBlock (numeric request fields via `string_as_number` per KTD3; response via `string_or_number`; `Default` + `#[serde(default)]`), facade method through `Inner::post`/`post_paginated`, `{TR}_POLICY` registered in **both** crosscheck lists. Fire the typed smoke before registrations (crosscheck lists are test-only). Flip on a non-empty witness; record PENDING on empty `00707`/all-default. Bump `banner_trs` + `reference.len` only (KTD7).
- **Execution note:** Write the offline deserialize tests first, then run the paper smoke.
- **Patterns to follow:** `t8425` — `crates/ls-sdk/src/market_session/mod.rs` (`T8425*` structs), `crates/ls-core/src/endpoint_policy.rs` (`T8425_POLICY`), offline tests in `crates/ls-sdk/tests/market_session_tests.rs`, `live_smoke_t8425`.
- **Test scenarios:**
  - Covers AE1. Success body deserializes and a substantive modeled field holds a real (non-default) value.
  - Numeric fields parse via `string_or_number` from both string- and number-shaped JSON; each numeric request field serializes as a JSON number (`.is_number()`).
  - Single out-row deserializes whether the gateway returns a bare object or a one-element array (`de_vec_or_single`).
  - Empty result (`rsp_cd 00707`) deserializes as the empty/pending case without panicking.
  - `::new(...)` serializes the in-block under the correct `serde(rename)` key.
- **Verification:** Paper smoke records a non-empty `LIVE-SMOKE` witness line; `cargo test -p ls-core` crosscheck green; `reference.len` incremented by the flip count.

### U5. Track F/O (options/futures) REST candidates

- **Goal:** Bring options/futures REST survivors from U1 to Tracked.
- **Requirements:** R1, R2, R13.
- **Dependencies:** U1.
- **Files:** as U3 (per-TR metadata + baseline + tracked count sites).
- **Approach:** Same as U3, with F/O `instrument_domain` facets; flag account/capacity reads that U6 will smoke under a credential lane.
- **Patterns to follow:** `metadata/trs/CSPAQ12200.yaml` (account), prior F/O TRs CIDBQ*/CFOEQ* from §17.
- **Test scenarios:** Test expectation: none — metadata + baseline; verified by metadata gate.
- **Verification:** `cargo test -p ls-core` green; `maintained_tr_count` bumped; drift guard clean.

### U6. Flip F/O reads via credential lanes

- **Goal:** Flip the F/O account/capacity reads that yield under their own credential lane; faithfully PEND the funded-but-empty ones.
- **Requirements:** R4, R6, R10, R12, R13; covers AE4.
- **Dependencies:** U2, U5.
- **Files:** as U4, plus `Makefile` lane wiring (`LS_SMOKE_LANE` target-variable per smoke).
- **Approach:** Per `implement-tr` with KTD6 credential-lane routing. Model the holdings TR first and assert its typed array is empty (holdings gate) before smoking positions-dependent reads. Rule out wrong-account binding before recording `00707`. A deserializable all-default row proves the shape only where that is the genuine empty shape; otherwise record PENDING (funded, no positions) — not a force-flip.
- **Patterns to follow:** `docs/solutions/conventions/ls-account-token-bound-credential-lanes.md`, `docs/solutions/conventions/closed-window-account-capacity-reads-all-default.md`.
- **Test scenarios:**
  - Covers AE4. Funded cash-only account, no positions: read either flips on a genuine all-default shape witness or records PENDING with reason — never force-flipped on a zero row.
  - Offline: out-block deserializes with all-default values without panicking; substantive field asserted when present.
  - Lane routing: a missing lane credential file fails the smoke fast (no silent fallback to the default account).
- **Verification:** Each F/O read reaches Implemented or PENDING with a recorded witness; no wrong-account `00707` recorded as a terminal; the recorded all-default witness line is scrubbed (R11b) — no account number or `rsp_msg` in the deserialized-shape record.

### U7. Track + dispose overseas REST candidates

- **Goal:** Track the overseas-REST survivors and record their honest disposition under closure (expected: mostly `paper_incompatible`/PENDING); flip any that unexpectedly serve data.
- **Requirements:** R1, R2, R9, R10, R11; R12 (only for a yielding overseas read); covers AE2.
- **Dependencies:** U1, U2.
- **Files:** track files as U3; smoke + disposition via `crates/ls-sdk/tests/live_smoke.rs`, `metadata/trs/<tr>.yaml` (`paper_incompatible` facet where `01900`), `metadata/PROVISIONALITY-LEDGER.md`. For a yielding overseas read, additionally the full U4 flip surface (`crates/ls-sdk/src/market_session/mod.rs`, `crates/ls-core/src/endpoint_policy.rs` `{TR}_POLICY` + both crosscheck lists, offline test file, `crates/ls-docgen/src/lib.rs` `banner_trs` + `reference.len`).
- **Approach:** Track each candidate, run its paper smoke under the overseas credential lane. On `01900` set `paper_incompatible: true` and leave Tracked (no flip, no re-probe of already-confirmed `01900`); on empty `00707` record paper-unavailable/PENDING with the witness reason. An overseas read that *does* serve a non-empty witness applies U4's complete flip machinery (struct + policy + dual registration + offline tests + `banner_trs`/`reference.len` bump) — it is not flipped by metadata alone.
- **Patterns to follow:** §14 (overseas/night `00707`), §12 (`01900` venue-reject), `docs/solutions/conventions/paper-unavailable-disposition-terminals.md`.
- **Test scenarios:**
  - Covers AE2. An overseas read returning `01900` stays Tracked, recorded `paper_incompatible`; `maintained_tr_count` up, `reference.len` unchanged.
  - Empty `00707` under the correct lane records PENDING with reason, not a flip.
- **Verification:** Every overseas candidate has a recorded disposition; the ledger reflects each terminal; no force-flips.

### U8. Track raw WebSocket channels

- **Goal:** Bring the WebSocket survivors from U1 to Tracked via the realtime recipe.
- **Requirements:** R1, R2, R7, R13.
- **Dependencies:** U1.
- **Files:** `metadata/trs/<tr>.yaml` (`owner_class: realtime`, `protocol: websocket`), `metadata/tr-index.yaml`, projected baseline, `manifest.json`; tracked count sites `crates/ls-trackers/tests/api_drift.rs`, `crates/ls-trackers/src/cli.rs`, `crates/ls-docgen/src/lib.rs` (`TRACKED_TRS`).
- **Approach:** Per `track-realtime-tr`: author realtime metadata (omit the recommendation block at Tracked tier; `rate_bucket: market_data`; subscribe-slot `caller_supplied_identifiers`), renormalize, bump the tracked count sites, revert `manifest.refreshed` (KTD7).
- **Patterns to follow:** `metadata/trs/S3_.yaml`, `metadata/trs/K3_.yaml`.
- **Test scenarios:** Test expectation: none — metadata + baseline; verified by metadata gate.
- **Verification:** `cargo test -p ls-core` green; `maintained_tr_count` + `TRACKED_TRS` bumped; `reference.len`/`banner_trs` untouched.

### U9. Flip WebSocket channels (connection-reachable-only)

- **Goal:** Flip the Tracked WebSocket channels on a clean lifecycle smoke, closure-independent.
- **Requirements:** R4, R7, R8, R12, R13; covers AE3.
- **Dependencies:** U2, U8.
- **Files:** `crates/ls-sdk/src/realtime/frame.rs` (`<Xx>Row` struct), `crates/ls-sdk/src/realtime/mod.rs` (re-export), `crates/ls-core/src/endpoint_policy.rs` (`{TR}_POLICY`, WebSocket), `crates/ls-core/tests/policy_index_crosscheck.rs` (policies array **only**), `crates/ls-sdk/tests/live_smoke.rs` (lifecycle smoke), `Makefile` (`live-smoke-<tr>` + `.PHONY`), `.agents/skills/promote-tr/references/smoke-map.md`, `metadata/trs/<tr>.yaml` (flip), `crates/ls-docgen/src/lib.rs` (`banner_trs` + `reference.len`).
- **Approach:** Per `implement-realtime-tr`: model the push-row struct from the **raw** `res_example` (not the normalized baseline), mirror `S3Trade`; WebSocket `{TR}_POLICY` registered in the crosscheck array only (KTD2). Run the `live-smoke-ws-negative` control first; with a fresh isolated `WsManager`, `subscribe_typed` on the resolved paper URL (assert port `29443`), timebox a row as bonus, `unsubscribe`. The lifecycle harness routes its recorded `LIVE-SMOKE` lines and panic output through the shared scrubber and installs the dispatch-log suppressor (U2/KTD5) — not only the "no raw frame logged" guarantee — since the connect/subscribe path can surface account-bearing context. Flip on a clean lifecycle; metadata records **connection-reachable-only** and marks field correctness provisional / structurally-unverified where the raw out-block key/array-ness is unconfirmed. Bump `banner_trs` + `reference.len` only.
- **Patterns to follow:** `K3_` — `crates/ls-sdk/src/realtime/frame.rs` (`S3Trade`), `S3_POLICY`/`K3_` policy, `ws_lifecycle_smoke(...)` helper.
- **Test scenarios:**
  - Covers AE3. A clean connect/subscribe/unsubscribe with no protocol error flips the channel.
  - Offline: the push-row struct deserializes the raw `res_example` (object or array via `de_vec_or_single`); every field tolerant via `string_or_number`.
  - Negative control returns its expected silence before any flip is recorded; no raw frame is logged.
- **Verification:** `live-smoke-ws-negative` ran; lifecycle smoke clean on the paper port; metadata says connection-reachable-only; crosscheck green; the WS policy is absent from the REST-only slice list.

### U10. Disposition close-out, count reconciliation, and gate

- **Goal:** Reconcile every count site, record final dispositions, regenerate docs, and bring the full gate green.
- **Requirements:** R11, R13, R14.
- **Files:** `metadata/PROVISIONALITY-LEDGER.md`, `crates/ls-docgen/src/lib.rs`, `crates/ls-trackers/tests/api_drift.rs`, `crates/ls-trackers/src/cli.rs`, `.agents/skills/promote-tr/references/smoke-map.md`, `Makefile`.
- **Dependencies:** U3–U9.
- **Approach:** Confirm every probed candidate has a ledger disposition (Implemented / PENDING / paper_incompatible) — no silent drops (R11). Reconcile `maintained_tr_count`, `TRACKED_TRS`, `banner_trs`, `reference.len` against actual tracked/flipped totals. Verify `manifest.refreshed` is unchanged from the prior raw-refresh date. Add the smoke-map row + Makefile `.PHONY` per flipped TR (CI-silent sites). Repoint any support-aware fixture caught by the exemplar trap. Run the full gate; do not `cargo fmt` `ls-trackers`.
- **Test scenarios:** Test expectation: none directly — this unit's product is a green gate and a complete ledger. Verified by the Verification Contract below.
- **Verification:** `make docs && cargo test && cargo test -p ls-core && make docs-check` all green; every candidate dispositioned; count sites internally consistent.

---

## Verification Contract

| Gate | Command | Applies to | Done signal |
|---|---|---|---|
| Metadata + policy crosscheck | `cargo test -p ls-core` | U3–U9 | Metadata validates; policy index mirrors metadata; REST policies in both lists, WS in crosscheck only |
| Workspace tests | `cargo test` | U2, U4, U6, U7 (if it flips), U9 | Offline deserialize tests + crosscheck green |
| Docs regen | `make docs` | U4, U6, U7, U9, U10 | Reference pages regenerate from metadata |
| Docs match committed | `make docs-check` | U3–U10 | Generated docs equal committed |
| Per-TR paper smoke | `make live-smoke-<tr>` (lane-scoped where F/O/overseas) | U4, U6, U7, U9 | Non-empty witness (REST) / clean lifecycle (WS), credential-free recorded line |
| WS negative control | `make live-smoke-ws-negative` | U9 | Returns expected NOT-OBSERVABLE silence before any WS flip |
| Drift guard | `git diff --stat …/normalized/trs/` | U3, U5, U8 | Only new baseline files; `manifest.json` shows only `maintained_tr_count` change |

---

## Definition of Done

- Every in-scope raw candidate from U1 reaches exactly one disposition — Tracked-only, Implemented, PENDING (with reason), or paper_incompatible — recorded in `metadata/PROVISIONALITY-LEDGER.md`; no silent drops.
- Every flipped TR builds, sends, and deserializes a non-empty success (REST) or passes a clean lifecycle smoke (WebSocket), with a credential-free recorded witness; F/O flips ran under the correct credential lane.
- No new smoke leaks an account number or `rsp_msg` into committed evidence or `RUST_LOG=debug` output (R11b verified).
- All count sites (`maintained_tr_count`, `TRACKED_TRS`, `banner_trs`, `reference.len`) are internally consistent with actual totals; `manifest.refreshed` unchanged; each flipped TR has a smoke-map row and Makefile `.PHONY` entry.
- The full gate — `make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check` — is green at every commit.
- `recommended: false` on every flip (promotion to Recommended is out of scope); the `ls-trackers` crate was not blanket-formatted.

## Deferred / Open Questions

### From 2026-06-28 review

- **Cheapest flips (54 already-Tracked) scoped out** — Outstanding Questions / Scope Boundaries (P2, product-lens, confidence 75)

  The 54 already-Tracked-but-not-Implemented TRs are the lowest-cost flips available — already past the raw→track rung, needing only a paper smoke plus callable Rust — yet the wave scopes them to "opportunistic/follow-up" and commits effort to the more expensive full raw→track→implement pipeline. If the wave's product goal is harvesting closure-viable flips per unit effort, deferring the cheapest-per-flip pool while running the most expensive one inverts the leverage order. Decision for planning: promote the still-flippable survivors of prior dispositions among the 54 to an explicit early flip pass, or confirm the wave stays scoped to the raw pool by design.
