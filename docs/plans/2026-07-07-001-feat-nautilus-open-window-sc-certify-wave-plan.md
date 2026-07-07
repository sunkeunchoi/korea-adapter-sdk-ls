---
title: Nautilus Open-Window SC Certification Wave - Plan
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
date: 2026-07-07
---

# Nautilus Open-Window SC Certification Wave - Plan

## Goal Capsule

- **Objective:** Use the open KRX window to settle the nautilus adapter's remaining live-gated question — whether the paper gateway delivers SC push-fill frames and tolerates the exec client's second WS session — and switch SC to the primary fill source if it certifies. Fold in the `cheprice` live observation and the two prerequisites that unblock clean live order work.
- **Product authority:** the Product Contract below.
- **Open blockers:** requires an **open KRX window** and an **attended** operator (Leg 2 places a real 1-lot marketable paper order + sign-aware close, bypassing the band guard). Order autonomy refuses unattended runs.
- **Product Contract preservation:** unchanged. Planning added the Planning Contract, Implementation Units, Verification Contract, and Definition of Done below; no Product Contract requirement was altered.

---

## Product Contract

### Problem & context

Three of the four originally-listed items are far smaller than they read, because the machinery already exists in the codebase:

- **`cheprice` is already wired end-to-end.** It is a field on `T0425OutBlock1` (`crates/ls-sdk/src/orders/mod.rs:818-822`) and in the t0425 baseline, and the nautilus fill path already consumes it: `adapters/nautilus/src/orders/poll.rs:171-182` uses the poll row's `cheprice` as the fill price when it parses positive (`cheprice > 0`), else falls back to the order's limit price and sets `price_approximated` (KTD4; the field lives on `FillObservation`/`FillDelta` in `orders/ledger.rs`), which the lab data-quality collector already counts. **No code to add** — the open question is empirical.
- **The SC0/SC1 lane already emits deduped fills.** Both fill sources feed one exactly-once ledger; an SC1 execution arriving after a poll fill produces no duplicate delta (`ledger.rs` AE1). The poll cadence is already designed to relax post-certification (`execution.rs:45,110-113`, `with_poll_cadence`). "Make SC primary" is therefore a cadence/config decision, not new lane code.
- **The nautilus runtime does not carry the guard bug.** Its open-order check already gates on the `cts_ordno` body cursor (`adapters/nautilus/src/execution.rs:124-130`), which is the *correct* behavior. The `tr_cont`-header bug lives only in the SDK certification-probe layer (`crates/ls-sdk/tests/negative_probe.rs`, `crates/ls-sdk/tests/order_smoke.rs`).

Consequently the README's "v-next follow-up *adds* cheprice" and "SC subordinate until the live probe certifies it" are **stale** and should be corrected. What genuinely remains is a certification + live-operation wave with two small code deltas.

### Actors

- **Attended operator** — runs the live probe legs and the stranded-order cancel against the real LS paper gateway during an open KRX window; files the verdict.
- **Nautilus exec lane** (`node_exec_tester` / exec client) — the system under certification.

### In scope

1. **Clear the stranded 005930 order.** Cancel the resting band-floor buy left on 005930 by the §27 order-quartet probe. Prerequisite: Leg 2 places a marketable order + sign-aware close on 005930; a leftover resting order on the same symbol corrupts flat-assertion / close accounting. Must be confirmed flat before the SC probe runs.
2. **Run the SC live probe (open KRX, attended).** Leg 1 (SC0, guarded resting chain, never fills) then Leg 2 (SC1, 1-lot marketable buy + sign-aware close, bypasses the band guard — the only way to witness a fill frame). File the `SC PROBE [...]` verdict in the smoke registry: SC0-seen, SC1-seen, and whether the second concurrent WS session was tolerated.
3. **Capture the `cheprice` observation from the same Leg 2 fill.** Record whether the live t0425 poll row populated `cheprice` (positive) or hit the `price_approximated` limit path. No code change — read the existing marker.
4. **Conditional SC-primary switch.** *If and only if* SC1 frames arrive AND the second WS session is tolerated, relax the t0425 poll cadence to a slow backstop via the existing `with_poll_cadence` hook, making SC the primary fill source with poll as a fail-closed backstop. This is an operator-driven config change applied after the human files a certifying verdict — not auto-wired to a runtime signal.
5. **Fix the SDK guard bug.** Gate the single-page check on the response `cts_ordno` cursor (empty / `" "` / all-default = terminal), not the `tr_cont` header, in both `negative_probe.rs::scan_symbol_working_orders` and its `order_smoke.rs` twin, so §27 order re-cert can scan a non-empty book. Offline; independent of the live probe.
6. **Correct the stale README claims** for `cheprice` (already wired) and the SC lane (already emits deduped fills; poll relaxes on certification).

### Out of scope

- **Depth10 / full 10-level ladder decode** — split to its own offline follow-up plan; it does not need an open market and does not block the bar-driven scan strategy.
- **SC-primary switch when the probe does not certify** — poll stays authoritative, no change.
- **Nautilus-runtime guard changes** — none needed; the exec lane already uses `cts_ordno`.
- **Auto-wiring the fill-source switch to a runtime signal** — the switch is a deliberate operator config decision.

### Success criteria

- Stranded 005930 order confirmed flat (cancel acknowledged; symbol shows no owned resting order).
- SC probe verdict (SC0-seen / SC1-seen / 2nd-WS-tolerated) **and** the cheprice-populated-vs-fallback observation filed in the smoke registry from the same Leg 2 fill.
- Guard fix lands with a green gate; §27 order re-cert can scan a non-empty book without the fail-closed pagination trip.
- If SC certifies: poll cadence relaxed, SC documented as the primary fill source with poll as backstop, and the exec lane re-certified under the new cadence.
- If SC does not certify: verdict filed as poll-authoritative; no cadence change.
- README corrected for the two stale claims.

---

## Planning Contract

### Key technical decisions

- **KTD-1. Guard fix targets the `cts_ordno` body cursor, not the `tr_cont` header.** Root cause (§27 ledger, `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md`): t0425 self-paginates on the `cts_ordno` body cursor; `tr_cont` "rides defensively" and the gateway returns `tr_cont="0"` on *any* non-empty page, so gating single-page-ness on the header fail-closes the instant the probe places its own control order. The scan is already `chegb="2"` (unfilled-only, single page) with a first-page cursor `cts_ordno=" "` — the remaining defect is purely the terminality test. Terminal when the response `cts_ordno` cursor is empty / `" "` / all-default; paginated only when it carries a real continuation cursor.
- **KTD-2. The guard fix is duplicated in two probe files** — `crates/ls-sdk/tests/negative_probe.rs:1025` and `crates/ls-sdk/tests/order_smoke.rs:708` each carry their own `scan_symbol_working_orders` with the same `resp.tr_cont().trim()` gate (negative_probe.rs:1034-1038, order_smoke.rs:736-739). Both must change or §27 stays half-fixed.
- **KTD-3. The SC-primary switch reuses `with_poll_cadence`.** The SC0/SC1 lane already emits exactly-once deduped fills (`adapters/nautilus/src/orders/ledger.rs`, AE1) and `execution.rs:112` already exposes `with_poll_cadence`. "SC primary" = construct the live exec client with a relaxed cadence so SC carries fills and poll becomes a slow fail-closed backstop. No new lane, no dedup change.
- **KTD-4. Relaxed cadence must respect the Account/market-data bucket AND a stated detection-latency ceiling.** t0425 caps at 2/s and is charged to a bucket that throttles to `IGW00201` under burst (`docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md`). The backstop cadence must be *slower* than the current 2s default (`DEFAULT_POLL_CADENCE`, `execution.rs:46`) — a periodic reconcile safety net, not a fill path. Exact value deferred to impl; a 10–30s backstop is the expected range. **Detection ceiling (load-bearing):** the poll loop consumes `reconcile_armed` only *after* `sleep(cadence)` (no event-driven wakeup, `execution.rs` run_poll_loop), so the relaxed cadence *is* the worst-case time-to-detect a dropped/missed SC fill — during which the bar strategy could act on stale flat/position state. U6 must therefore state a maximum acceptable detection latency and derive the cadence ceiling from it, **or** add an arm-triggered short-poll wakeup so a missed SC fill reconciles promptly regardless of the relaxed steady-state cadence.
- **KTD-5. The SC-primary mechanism ships unconditionally; only its live activation is verdict-gated.** Split the work: the off-by-default mechanism (U4 — the `with_poll_cadence` selector + offline tests, whose "selector off = no-op" branch proves it is safe to ship regardless of the verdict) lands this wave; the live activation (U6 — flipping the selector on + re-cert under relaxed cadence) is gated on U3 **certifying**. **Certification is not frame-count alone:** U3 must witness, live, that the same Leg 2 fill arrived via *both* the SC1 frame and the t0425 poll and collapsed to exactly one `FillDelta`, with the SC frame's `execprc` parsing positive — i.e. the exactly-once dedup invariant (KTD-3) holds against real frames, not just the mock. A bare `sc1_fills>0 && second_ws_session_ok` verdict is insufficient to activate U6, because U6 relaxes the poll and makes that dedup load-bearing. If any leg fails or the live dedup is not witnessed, U6 is skipped and the disposition is filed as poll-authoritative. This split also removes a hazard: gating the whole mechanism on the verdict would force new adapter code to be authored + reviewed *during* the scarce attended open-KRX window.
- **KTD-6. The guard fix (`ls-sdk`) must be gated against the standalone adapter workspace too.** Per `docs/solutions/workflow-issues/cross-workspace-gate-blind-spot-sdk-preflight-changes-redden-adapter.md`, `adapters/nautilus` is a separate cargo workspace outside the root gate; SDK dispatch/order-path changes can silently redden it. Although this fix is confined to test-harness scan helpers (low blast radius), run the adapter gate as a guard.

### Research & grounding

- Guard bug + fix rationale: `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md`, `docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md`.
- Cross-workspace gate: `docs/solutions/workflow-issues/cross-workspace-gate-blind-spot-sdk-preflight-changes-redden-adapter.md`.
- SC probe surface: `adapters/nautilus/src/probe.rs` (`ScObservation`, `format_verdict`), `adapters/nautilus/src/bin/node_exec_tester.rs` (`LS_NODE_SC_PROBE`, `LS_NODE_SC_MARKETABLE` legs).
- Fill-price consumption: `adapters/nautilus/src/orders/ledger.rs:53-60,131-137`.
- Smoke registry for evidence: `.agents/skills/promote-tr/references/smoke-map.md`.
- §27 disposition + stranded-order safety note: `metadata/PROVISIONALITY-LEDGER.md` (§27 tail).

---

## Implementation Units

### U1. Fix the `tr_cont`→`cts_ordno` single-page guard in both probe scans

- **Goal:** Make `scan_symbol_working_orders` positively confirm flat on a non-empty single page, so an order probe can scan a book that already holds its own control order without a false pagination HELD.
- **Requirements:** In-scope #5; success criterion "guard fix lands with a green gate". KTD-1, KTD-2.
- **Dependencies:** none (offline, independent of the live chain).
- **Files:**
  - `crates/ls-sdk/tests/negative_probe.rs` (modify `scan_symbol_working_orders`, ~lines 1025-1040; the offline twin test `working_orders_scan_request_is_unfilled_only_single_page` ~line 1726 and any pagination-guard test)
  - `crates/ls-sdk/tests/order_smoke.rs` (modify `scan_symbol_working_orders`, ~lines 708-745)
- **Approach:** Replace the `resp.tr_cont().trim()`-based paginated test with a terminality check on the response `cts_ordno` body cursor: terminal when empty / `" "` / all-default; paginated (→ `Err`, fail-closed) only when the cursor carries a real continuation value. Keep the `chegb="2"` unfilled-only single-page request and the first-page `cts_ordno=" "` input unchanged — only the response-side terminality decision changes. Preserve the loud `reconcile-scan failed` / `cannot positively confirm flat` error text for the genuine paginated case. **Extract the terminality decision into a pure sync function** (e.g. `fn scan_page_is_terminal(cts_ordno: &str) -> bool`, mirroring `execution.rs:130`'s `cts_ordno.trim().is_empty()`) and have both async helpers call it — the `async` `scan_symbol_working_orders` is live-only (its callers are `#[ignore]` order-probe/teardown flows, `negative_probe.rs:1108/1304/1475`, `order_smoke.rs:1196/1246`) and neither test file has a wiremock/MockServer t0425 harness, so the terminality logic must be extracted to be unit-testable offline without the gateway.
- **Patterns to follow:** the nautilus runtime already does this correctly — `adapters/nautilus/src/execution.rs:124-130` reads `orders.outblock.cts_ordno.trim()` as the single-page terminality signal. Mirror that predicate. The pure-fn extraction matches the files' existing shape (e.g. `flat_verdict`, `classify_control_disposition` are already pure and offline-tested).
- **Test scenarios:**
  - Happy path: a single non-empty page (one resting control row) with a terminal `cts_ordno` (`" "`) → scan returns flat/working-set, NOT an error. *Covers the §27 root cause — the scenario that currently false-HELDs.*
  - Edge: empty page (`body_len`-63 flat, `cts_ordno=" "`) → still confirms flat (regression guard on the already-working empty case).
  - Error path: response carries a real non-blank continuation `cts_ordno` → scan fails closed with the paginated error (a genuinely truncated book must not be read as flat).
  - Edge: `tr_cont="0"` header present on a terminal-cursor page → treated as terminal (the header is no longer load-bearing).
  - Both files: assert the same behavior against the extracted pure `scan_page_is_terminal` fn in the `negative_probe.rs` and `order_smoke.rs` twins so neither regresses independently.
- **Verification:** `cargo test -p ls-sdk` green including both probe test files. Note: no existing offline twin encodes the response-side gate today — the cited `working_orders_scan_request_is_unfilled_only_single_page` (negative_probe.rs:1726) asserts *request* shape only (`chegb`/`expcode`/`cts_ordno` on the builder) — so this unit **adds** a new offline unit test for the extracted `cts_ordno`-terminality fn; it does not merely edit an existing one.

### U2. Clear the stranded 005930 resting order (operator live-op, prerequisite)

- **Goal:** Confirm the paper account holds no owned resting order on 005930 before any live order work, so Leg 2's flat-assert and sign-aware close account cleanly.
- **Requirements:** In-scope #1; success criterion "stranded 005930 order confirmed flat".
- **Dependencies:** none; must complete before U3.
- **Files:** none (operational — cancel via the existing order path / a raw cancel against the paper gateway).
- **Approach:** Cancel the stranded band-floor buy on 005930 left by the §27 order-quartet probe (it is non-marketable so it cannot fill, but it blocks/skews scans). Confirm the symbol shows zero owned resting rows afterward. The order was stranded because the teardown scan itself hit the pagination bug fixed in U1 — running U1 first is not required to cancel, but makes the confirming re-scan reliable.
- **Execution note:** operational, attended; not a code change. Land the confirmation as filed evidence, not a commit.
- **Test scenarios:** `Test expectation: none — operational live-op.`
- **Verification:** an unfilled-only scan of 005930 returns zero owned resting rows; recorded alongside the U3 evidence.

### U3. Run the SC live probe + file the verdict and cheprice observation (operator live-op, open KRX)

- **Goal:** Settle empirically whether the paper gateway delivers SC0/SC1 push frames and tolerates the exec client's second concurrent WS session, and whether the live t0425 poll row populates `cheprice`.
- **Requirements:** In-scope #2, #3; success criteria for the filed verdict + cheprice observation. KTD-5.
- **Dependencies:** U2 (stranded order cleared). Independent of U1/U5.
- **Files:** none new — the probe is already coded: `adapters/nautilus/src/bin/node_exec_tester.rs` (Leg 1 `LS_NODE_SC_PROBE=1`; Leg 2 `LS_NODE_SC_MARKETABLE=1`), verdict via `adapters/nautilus/src/probe.rs::format_verdict`.
- **Approach:** Run Leg 1 (SC0 resting chain, never fills) then Leg 2 (SC1 1-lot marketable buy + sign-aware close) against the real paper gateway in an open KRX window. Capture the `SC PROBE [...]` line (SC0-seen / SC1-seen / 2nd-WS tolerated). From the same Leg 2 fill, read whether the poll fill used the row's `cheprice` (positive) or fell back to the limit price (`price_approximated`, `orders/ledger.rs:131-137`) — and record *which* branch set the marker, since `price_approximated` is also set true on any beyond-first-partial poll fill independent of `cheprice`. **Live-dedup witness (gates U6, KTD-5):** additionally record whether the same Leg 2 execution was observed via *both* the SC1 frame and the t0425 poll and collapsed to exactly one `FillDelta`, and whether the SC frame's `execprc` parsed positive. This live-dedup witness — not the bare frame count — is what authorizes U6; a certify without it files as poll-authoritative. File all of the above in `.agents/skills/promote-tr/references/smoke-map.md`.
- **Execution note:** attended live-op; a bare "silent" on the resting leg is NOT evidence SC frames don't arrive — only Leg 2 can certify SC1.
- **Test scenarios:** `Test expectation: none — live-op; the probe code and its offline verdict-format tests already exist in probe.rs.`
- **Verification:** verdict + cheprice observation + live-dedup witness filed in the smoke registry; the account is left flat after the sign-aware close.

### U4. Ship the off-by-default SC-primary mechanism (offline, unconditional)

- **Goal:** Land the SC-primary cadence mechanism — an off-by-default operator selector that constructs the exec client with a relaxed backstop cadence — so it is reviewed and tested *before* the scarce open-KRX window, independent of the probe verdict.
- **Requirements:** In-scope #4; success criterion "if SC certifies: poll cadence relaxed". KTD-3, KTD-4, KTD-5.
- **Dependencies:** none (offline). Ships regardless of the U3 verdict — the "selector off = no-op" branch makes it safe to land unconditionally.
- **Files:**
  - `adapters/nautilus/src/bin/node_exec_tester.rs` and/or the live-node bootstrap that constructs the exec client (apply `with_poll_cadence` with the backstop value behind an explicit off-by-default certified-mode selector, e.g. an env flag — not an automatic runtime signal)
  - `adapters/nautilus/src/execution.rs` (only if a named backstop constant is added alongside `DEFAULT_POLL_CADENCE`; if KTD-4's arm-triggered wakeup option is taken, the `run_poll_loop` arm-consume path is here too)
  - test file: `adapters/nautilus/tests/` (the exec-client/cadence test module)
- **Approach:** Add the operator-selected "SC-primary" mode that constructs the exec client via the existing `with_poll_cadence` hook at a relaxed backstop cadence (KTD-4). Default off → identical to today. SC frames carry fills; the slow poll remains a reconcile safety net, and exactly-once dedup between the two sources is unchanged. Do not remove or disable the poll loop — poll stays as the fail-closed backstop. Resolve KTD-4's detection-latency ceiling here (cadence ceiling or arm-wakeup); the value is set now, activation is U6.
- **Execution note:** build test-first for the cadence-selection behavior; the mechanism is fully unit-testable offline even though its live activation is U6.
- **Test scenarios:**
  - Happy path: certified-mode selector on → exec client is constructed with the relaxed backstop cadence, not the 2s default.
  - Default: selector off → exec client keeps `DEFAULT_POLL_CADENCE` (poll authoritative), i.e. shipping this unit is a no-op until U6 flips it.
  - Backstop invariant: with SC primary, the poll loop still exists and can still reconcile a fill (SC-primary does not delete the poll path).
  - Dedup regression: a fill observed via SC then the same execution via poll (or vice versa) produces exactly one delta under the relaxed cadence.
  - Detection ceiling: a missed SC fill is reconciled within the stated latency ceiling (either the cadence bound, or promptly via the arm-triggered wakeup if that option was taken).
- **Verification:** offline exec-client tests green; the mechanism is off by default (no behavior change until U6).

### U5. Correct the stale README claims

- **Goal:** Bring `adapters/nautilus/README.md` in line with the code: `cheprice` is already wired end-to-end, and the SC lane already emits exactly-once deduped fills with poll relaxing on certification.
- **Requirements:** In-scope #6; success criterion "README corrected for the two stale claims".
- **Dependencies:** none (offline). If U6 activates, reflect the certified SC-primary state; otherwise reflect the poll-authoritative disposition from U3.
- **Files:** `adapters/nautilus/README.md` (the "Execution lane" ~line 272 and "Staged SC live probe" ~line 285 sections; the `cheprice` "v-next follow-up" line ~283).
- **Approach:** Replace "a v-next SDK follow-up adds `cheprice`" with the accurate statement that `cheprice` is consumed today with a limit-price fallback (`price_approximated`). Update the SC section to record the filed verdict outcome (certified SC-primary, or poll-authoritative) rather than "unknown/subordinate until the probe certifies it".
- **Execution note:** doc-only; land after U3 (and U6 if it activates) so the corrected text matches the filed disposition.
- **Test scenarios:** `Test expectation: none — documentation.`
- **Verification:** README no longer claims cheprice is unimplemented; the SC section reflects the filed verdict.

### U6. Activate SC-primary under the certifying verdict (operator live-op, gated)

- **Goal:** After U3 files a *certifying* verdict, flip the U4 selector on and re-certify the exec lane running SC as the primary fill source with the poll as backstop.
- **Requirements:** In-scope #4; success criterion "if SC certifies: poll cadence relaxed … re-certified". KTD-4, KTD-5.
- **Dependencies:** U4 (mechanism shipped) **and** U3 with a certifying verdict — SC1 fills observed, 2nd WS session tolerated, **and the live cross-source dedup + positive `execprc` witnessed** (KTD-5). If any of these is absent, U6 is skipped and the disposition is filed as poll-authoritative; no cadence changes.
- **Files:** none new — U4 already shipped the mechanism. Operational: set the certified-mode selector (env flag) for the live-node run.
- **Approach:** Enable the off-by-default selector from U4 for an attended live run under an open KRX window, confirm fills are SC-sourced with the poll demoted to the relaxed backstop cadence, and file the re-cert evidence. No code change unless the live run surfaces a defect (which would loop back to U4 offline).
- **Execution note:** attended live-op, open KRX; the mechanism and its dedup/latency invariants were already proven offline in U4, so this run only witnesses activation, not new behavior.
- **Test scenarios:** `Test expectation: none — live-op; mechanism coverage is U4's.`
- **Verification:** attended re-cert under the relaxed cadence shows SC-sourced fills with the poll as backstop, filed in the smoke registry; account left flat.

---

## Verification Contract

- **Offline gate (U1, U4, U5):** root gate green — `make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`, `make lane-check` — with no red tree.
- **Cross-workspace gate (U1, U4):** run the `adapters/nautilus` workspace tests as well (`cargo test` inside `adapters/nautilus`), per KTD-6, since the root gate structurally cannot build the standalone adapter workspace. U1 is confined to SDK test harnesses (low risk), but the adapter gate is the guard that would catch any accidental order-path coupling.
- **Live gate (U2, U3, U6 activation):** attended, open KRX, `LS_TRADING_ENV=paper` with the domestic lane env file. Evidence (stranded-order-flat confirmation, SC verdict, cheprice observation, live-dedup witness, and — if U6 activates — the re-cert under relaxed cadence) filed in `.agents/skills/promote-tr/references/smoke-map.md`.
- **Never** run the live legs unattended — order autonomy refuses it, and Leg 2 places a real marketable paper order.

---

## Definition of Done

- U1 landed: both probe scans gate on `cts_ordno` terminality; `cargo test -p ls-sdk` and the adapter workspace gate green; §27 order re-cert can scan a non-empty book.
- U2 done: 005930 confirmed flat, recorded.
- U3 done: SC verdict + cheprice observation + live-dedup witness filed in the smoke registry; account left flat.
- U4 landed unconditionally: off-by-default SC-primary mechanism shipped, offline exec-client tests green, no behavior change until U6 flips it.
- U6 resolved: **either** U3 certified (incl. live dedup + positive `execprc` witnessed) → selector flipped on and re-cert under relaxed cadence filed, **or** not certified → U6 skipped and poll-authoritative disposition filed (both are valid DoD outcomes).
- U5 done: README corrected to match the filed disposition.
- Ledger updated with the wave's disposition (new §, mirroring §27's format).

---

## Open Questions

- **Relaxed backstop cadence value + detection-latency ceiling** (KTD-4) — resolve in U4 against the Account/market-data bucket pacing (expected 10–30s) *and* the maximum acceptable time-to-detect a dropped SC fill; decide whether to bound the cadence or add an arm-triggered wakeup. Deferred to execution.
- **Partial-certification disposition** — if the 2nd WS session is tolerated but SC1 is silent, or the live cross-source dedup is not witnessed (or vice versa), the default is: any failed criterion → SC stays subordinate, poll authoritative, U6 skipped. Confirm this default holds when the live verdict lands.
