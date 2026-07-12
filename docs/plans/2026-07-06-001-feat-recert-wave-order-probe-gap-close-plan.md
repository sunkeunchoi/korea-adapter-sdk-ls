---
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
date: 2026-07-06
type: feat
status: implementation-ready
---

# Recommended Re-Cert Wave + Order-Probe Gap-Close - Plan

**Product Contract preservation:** Product Contract unchanged — planning enriched the
requirements-only artifact in place with the Planning Contract, Implementation Units,
Verification Contract, and Definition of Done.

---

## Goal Capsule

- **Objective.** Use this attended open-KRX window (Mon 2026-07-06, regular session
  09:00–15:30 KST) to restore the Recommended support tier — currently **0** — by running
  the §25 armed gate live for its ten TRs. Before promoting the order quartet, close the
  three deferred live-only order-probe coverage gaps so the promotion rests on
  non-overstated evidence.
- **Product authority.** Operator (the human running this session) is the certifying
  witness. Promotion to `recommended: true` flips only on an in-session witnessed terminal
  certify-line — never on a `make` exit status.
- **Open blockers.** Requires an attended TTY, a fresh human `LS_ORDER_SMOKE_NONCE`, and an
  order-capable domestic paper account for the quartet legs. `LS_TRADING_ENV=paper`. Any
  live leg whose prerequisite is unavailable stays HELD with a recorded arm; the wave still
  closes (fail-closed, §25 AE2).

---

## Problem Frame

The §25 re-cert gate mechanism is authored and gate-green on `main` (PR #92, `4dd6901`),
but all ten TRs are HELD because the wave ran unattended at 15:48 KST — 18 minutes after
close. The Recommended tier has been **0** since the error-resilience gate demoted all ten
(PR #83). This window is the first attended open-KRX session available to execute the live
legs and restore the tier.

Additionally, PR #92 deferred three live-only order-probe coverage gaps that cannot be
verified offline. The order quartet's smoke-map claims currently overstate live coverage
(rows 67/68 say "variants vs a live control OrgOrdNo" but the code fires them against an
already-canceled control). Closing the gaps this window makes the quartet's promotion
evidence match its claims.

---

## Product Contract

### Primary actor & outcome

The operator restores the Recommended tier in one attended window. Each of the ten §25
re-cert TRs is either promoted on a clean witnessed chain or HELD with a recorded arm; the
order quartet's promotion is backed by a hardened probe whose smoke-map claims are true.

### The ten re-cert TRs (§25)

- **Reads (6):** `t8412`, `CSPAQ12200`, `S3_` (WebSocket lifecycle), `token`, `t1102`,
  `t1101`. Each (except `S3_`) runs its control smoke + differential negative probe, then
  promotes. The `token` negative leg runs **last** among live legs (it can disturb the
  session token). `S3_` has no live negative probe (a realtime error-coverage substitute
  stands in — realtime excludes trade-data correctness, in-session delivery, and
  reconnection from a differential probe) — its error-coverage file is recorded at
  promotion.
- **Order quartet (4):** `CSPAT00601`, `CSPAT00701`, `CSPAT00801` are certified via the
  hardened order negative-probe chain; `t0425` is a READ (`is_order: false`) certified via
  its own read differential probe (`run_inblock_negative_probe`, unaffected by the U1–U3
  order-harness hardening). All four then promote from their clean chains.

### R1 — close the 3 order-probe gaps (harden + live-verify, no deliberate fills)

- **(a)** `CSPAT00701`/`CSPAT00801` variants must fire against a **still-resting** control
  OrgOrdNo, then cancel it — not an already-canceled one. Smoke-map claim must match.
- **(b)** Flat-verify must not be blind to a filled order: `chegb` coverage spans filled
  **and** unfilled so a fill cannot hide. Safety no longer rests solely on the control
  never filling.
- **(c)** No foreign `005930` row is canceled, and no resting order is left behind. A
  fill-inclusive **pre-assert-flat** guard refuses to start if the symbol is not already
  flat and fill-free; given that proven-clean baseline, teardown then cancels every resting
  row (all of which are the probe's by construction).

### R2 — live certify run & promotion

Run the hardened order chain + the six read controls/probes live-attended this window.
Promote every reachable TR to `recommended: true`. Land the KTD5 one-time re-wirings with
the first flip. Record the disposition in ledger §26.

### Success criteria / DoD

See the Definition of Done section.

### Scope Boundaries

**In scope:** the 3 gap fixes + offline twins; live-attended certify + promotion of the
ten §25 TRs; docgen/freshness/count reconciliation; ledger §26.

#### Deferred to Follow-Up Work

- The `negative_probe.rs` `#[path]` split (file is 1355 lines; mechanical reorg, separate
  maintainability P1).

#### Outside this window's scope

- The 16-TR domestic residue reopen triggers: `t0441` F/O manufacture, `CSPBQ00200`
  (deposit-gated), `t3102` (live news), `t1109` (after-hours), the 5 structurally-held,
  `t1631` (`IGW40014` gateway defect).
- Deliberate fills / real positions of any kind.

---

## Planning Contract

### Key Technical Decisions

- **KTD1 — Gap (a): reorder, don't duplicate.** In `run_order_negative_probe`, move the
  variant-firing loop to run **while the control still rests**, then cancel + flat-verify
  after. The submit leg (`CSPAT00601`) is unaffected (its variants place new orders, not
  references to the control); only the referencing legs (`00701`/`00801`) needed the live
  control. Preserve the existing "control placed + canceled cleanly" proof by keeping the
  final cancel + flat-verify — it just moves after the variants. Rationale: the smoke-map
  "variants vs a live control OrgOrdNo" claim must become true.
  - **Fill-vector bound (new exposure introduced by the reorder).** Firing modify (`00701`)
    variants against a *live* resting control is a fill vector the current dead-ordno order
    does not have: a tolerated/coerced `OrdPrc` mutation could reprice the control to a
    marketable level and fill. It is bounded, not merely detected: the fired set is
    **type/required only** (KTD3 filter) — a removed or wrong-typed `OrdPrc` is rejected by
    the gateway, not silently coerced to a marketable price — and the modify seed price is
    already band-floor+1 tick. The fill-inclusive flat-verify (KTD2) catches any fill
    post-hoc. Do not add value-class (`enum`/`range`) price variants to the referencing legs.
- **KTD2 — Gap (b): observability widening; safe only because the baseline is proven clean
  (KTD3).** The fill-blindness is in the **scan input**, not the verdict:
  `scan_symbol_working_orders` hardcodes `chegb: "2"` (unfilled-only), so `flat_verdict`'s
  `Fill` branch is unreachable live. Widen the shared flat-verify scan to fill-inclusive
  (`chegb: "0"` = all rows) so a fill surfaces. **This scan feeds three consumers** — the
  inline control flat-verify (`negative_probe.rs:1026`), `order_reconcile_teardown` (the
  previously-dead `UNEXPECTED-FILL` branch at line 880 becomes reachable), and the new KTD3
  pre-assert-flat guard — so it changes the *effective* flatness classification across all
  three, not just the `flat_verdict` function body (which is unchanged). This is only sound
  because KTD3's pre-assert-flat proves the symbol flat **and fill-free** at probe start:
  otherwise a foreign/historical filled `005930` row would make `flat_verdict` return `Fill`
  and false-HELD the whole quartet. **Verify `chegb="0"` value semantics with an
  `make raw-probe LS_PROBE_TR_CD=t0425` A/B (chegb 0 vs 2 row counts) or the LS API doc —
  NOT the normalized baseline**, which records `chegb` only as a 1-char String with no value
  meanings. The existing single-page pagination guard still applies.
- **KTD3 — Gap (c): fill-inclusive pre-assert-flat + unconditional teardown (no ownership
  set).** Before placing the control, scan the symbol fill-inclusive; if **any** resting OR
  filled `005930` row exists, HELD (refuse to start). Given that proven flat+fill-free
  baseline, teardown keeps its **unconditional** cancel of every resting row — all are the
  probe's by construction. An ownership set (`placed_ordnos`) is deliberately **not** used:
  `fire_inblock` returns only `(http, rsp_cd)` and never parses the accepted variant's
  OrdNo, so an owned-only teardown would *strand* an accepted WAVE-BLOCKED submit variant
  (regressing "never leave a resting order") — exactly the case the harness exists to catch.
  Residual accepted: a foreign order/fill arriving *mid-probe* (after pre-assert-flat) would
  be canceled by teardown or read as the control's fill; this race is tolerated (attended,
  single-account paper, seconds-long) and noted, not guarded. **Cross-leg coupling:** a
  stranded control from one order leg leaves a resting `005930` row that trips the *next*
  leg's pre-assert-flat → that leg HELDs. The operator must confirm the book is flat (and
  manually clear any stranded control) between order legs; a pre-assert-flat HELD may be
  self-inflicted, distinct from a genuine foreign order.
- **KTD4 — Promotion gates on the witnessed certify-line.** Each promotion flips only on
  the operator-witnessed terminal certify-line, never the `make` exit status. Any
  fail-closed arm (degenerate band, rejected control, no-fill clean-cancel failure,
  transport may-rest, panic) → that TR stays HELD with the arm recorded; the wave still
  closes (§25 AE2).
- **KTD5 — One-time re-wirings land with the first flip only, applied serially.** On the
  first successful promotion, edit `recommended_no_banner` (`crates/ls-docgen/src/lib.rs:1413`,
  currently `[&str; 0]`) and the freshness count assertion
  (`freshness_check_over_empty_recommended_set_exits_zero`,
  `crates/ls-trackers/src/cli.rs:2373`, currently asserts 0). Subsequent promotions only
  extend `banner_trs` / bump the count. If there is no flip, none of these land. **The
  first-flip transition and every count/banner edit are a single serialized step** — do NOT
  delegate them to independent fresh-context `tr-promoter` subagents, which cannot observe
  whether the zero-state has already been converted (two would double-edit). If subagents
  run per-TR, they run strictly sequentially with the first-flip edit committed before the
  next starts, OR the count/docgen reconciliation is pulled out to U6 and applied once by
  the orchestrator.
- **KTD6 — Constraint-schema `required:false` caveat is carried, not changed.**
  `CSPAT00601.LoanDt`, `t0425.expcode`/`cts_ordno` stay `required: false` (certified struct
  sends them empty; struct wins over baseline) — a `required: true` would false-reject
  certified order flows at the shared preflight seam. Do not edit these schemas.

### Open Questions

- **Window-risk fallback (the at-risk leg is the order quartet, not the reads).** U1–U3 are
  non-trivial offline coding placed before the only time-critical work — the live order
  quartet, which needs the open KRX window + attended nonce. Reads have no window
  dependency, so "run reads first" does *not* protect the at-risk leg. Set a **hard cutover
  time by which the live order legs must start** (recommend ~14:30 KST, leaving margin
  before 15:30 close). If harness hardening threatens that cutover, choose: **(1)** fire the
  quartet against the *current* probe with the smoke-map corrected to describe actual
  coverage, and defer gap (a) hardening (evidence-honesty only — not a live-order safety
  gate like (b)/(c)) to follow-up; or **(2)** accept the quartet stays HELD this window and
  re-attend a later window. Gaps (b)/(c) are safety-relevant and should not be skipped if
  the quartet fires; gap (a) may be deferred under fallback (1).

---

## High-Level Technical Design

Gap (a) is a reordering of the order-probe lifecycle. Current vs. hardened:

```
CURRENT (gap a):                        HARDENED (KTD1–KTD3):
  place control                           pre-assert flat + fill-free (KTD3)  ← refuse if not
  cancel control                          place control
  flat-verify  ── proves cancel works     fire type/required variants
  fire variants ── against DEAD ordno       └─ against STILL-RESTING control ✔
  final reconcile                         cancel control
                                          flat-verify (fill-inclusive, KTD2) ✔
                                          teardown: cancel ALL resting rows ✔ (all ours)
```

Submit (`CSPAT00601`) variants do not reference the control, so their placement in the loop
is immaterial. An accepted (WAVE-BLOCKED) submit variant rests at an OrdNo the fire path
never surfaces (`fire_inblock` returns only `(http, rsp_cd)`); the **unconditional**
teardown cancel — sound because pre-assert-flat proved the book clean at start — is what
covers it. An owned-only teardown would strand it.

---

## Implementation Units

### U1. Gap (a) — fire modify/cancel variants against a still-resting control

- **Goal:** The `CSPAT00701`/`CSPAT00801` variant legs exercise a live control OrgOrdNo, so
  the smoke-map's "live control" claim is true.
- **Requirements:** R1(a).
- **Dependencies:** none.
- **Files:** `crates/ls-sdk/tests/negative_probe.rs`,
  `.agents/skills/promote-tr/references/smoke-map.md`.
- **Approach:** Reorder `run_order_negative_probe` per KTD1 — the variant-firing loop moves
  ahead of the control cancel + flat-verify. Keep the may-rest halt and WAVE-BLOCKED
  branches. After the loop, cancel the control and flat-verify (the "cancel works" proof
  now happens post-variants). Correct smoke-map rows 67/68 to describe "variants vs a
  still-resting control, canceled after" and adjust the `#[ignore]` reason strings if they
  assert the old order.
- **Execution note:** Refactor the sequencing first; keep the offline twin
  (`order_tr_variants_are_type_and_required_only`) green throughout — it pins the fired
  class set, which must not change.
- **Patterns to follow:** the existing may-rest / WAVE-BLOCKED / reconcile structure in
  `run_order_negative_probe`.
- **Test scenarios:**
  - `Test expectation:` the reorder itself is **not** offline-observable — the referencing
    seed is built from an OrdNo *string* that is byte-identical whether or not the order was
    canceled, so no CI twin can distinguish "seed built before cancel" from "after." The
    ordering is verified only by the **U5 live witness** (a modify/cancel variant line prints
    while the control is resting). Do not add a tautological phase-list twin unless
    `run_order_negative_probe` is genuinely refactored into a phase-driven executor the twin
    consumes.
  - Existing `order_tr_variants_are_type_and_required_only` stays green (fired set
    unchanged — the fill-vector bound in KTD1 depends on it).
  - Existing `new_schema_offline_twins` stays green.
- **Verification:** the class twins stay green; smoke-map rows 67/68 no longer overstate; the
  U5 live leg prints a modify/cancel variant line while the control is resting.

### U2. Gap (b) — fill-aware flat-verify

- **Goal:** A filled control is visible to flat-verify; a real fill halts as unrecoverable
  instead of reading Flat.
- **Requirements:** R1(b).
- **Dependencies:** none (independent of U1).
- **Files:** `crates/ls-sdk/tests/negative_probe.rs`.
- **Approach:** Per KTD2, widen `scan_symbol_working_orders` to a fill-inclusive `chegb`
  (`"0"` = all rows) so `flat_verdict` can return `Fill`. `flat_verdict`'s body is untouched,
  but the change alters *effective* flatness for all three scan consumers (pre-assert-flat,
  inline flat-verify, teardown) — this is sound **only** paired with U3's fill-inclusive
  pre-assert-flat, which guarantees no foreign/historical fill is present to misattribute.
  Verify `chegb="0"` value semantics with a **`make raw-probe LS_PROBE_TR_CD=t0425` A/B
  (chegb 0 vs 2 row counts)** or the LS API doc — the normalized baseline carries no value
  meanings and cannot answer this. Ensure the `Fill` verdict routes to the existing
  HELD/"reset the paper book" unrecoverable path in the pre-variant flat-verify and
  `order_reconcile_teardown`.
- **Execution note:** Observability change gated on U3's clean baseline; confirm the
  single-page pagination guard still holds for an all-rows scan of one symbol.
- **Patterns to follow:** existing `flat_verdict` `Fill` handling at
  `order_reconcile_teardown` (the `UNEXPECTED-FILL` branch).
- **Test scenarios:**
  - `flat_verdict` returns `Fill` for a row with `cheqty>0` (existing twin
    `flat_verdict_keys_on_quantities_and_a_fill_outranks_a_resting_remainder` — keep green).
  - Add: the scan-request builder emits the fill-inclusive `chegb` value (assert the request
    `chegb` field, not a live call).
  - Add: a synthesized `Fill` row drives the unrecoverable/HELD outcome in **each** of the
    three consumers (pre-assert-flat → refuse; inline flat-verify → HELD no-variants;
    teardown → `UNEXPECTED-FILL` alarm), not just `flat_verdict` in isolation.
- **Verification:** twins green; the scan request carries the fill-inclusive `chegb`; a
  synthesized `Fill` row drives the unrecoverable path across all three consumers.

### U3. Gap (c) — fill-inclusive pre-assert-flat + unconditional teardown

- **Goal:** No foreign `005930` order is canceled and no probe-placed order is left resting,
  without an ownership set the fire path cannot populate.
- **Requirements:** R1(c).
- **Dependencies:** none (independent of U1; pairs with U2 — the pre-assert-flat scan and the
  flat-verify scan must both be fill-inclusive). Integrates with both in U5.
- **Files:** `crates/ls-sdk/tests/negative_probe.rs`.
- **Approach:** Per KTD3, add a **pre-assert-flat guard** at the top of the placement path:
  scan the symbol fill-inclusive (U2's `chegb`); if `flat_verdict` returns anything but
  `Flat` (any resting **or** filled `005930` row), HELD — do not place. Keep
  `order_reconcile_teardown`'s **unconditional** cancel of every resting row (do NOT
  introduce an ownership set): because pre-assert-flat proved the book flat+fill-free at
  start, every resting row at teardown is the probe's, including an accepted WAVE-BLOCKED
  submit variant whose OrdNo `fire_inblock` never surfaces (an owned-only teardown would
  strand it). Record the tolerated mid-probe-foreign race and the cross-leg pre-assert-flat
  coupling (KTD3) in comments so a reader knows the residual is deliberate.
- **Execution note:** Extract the pre-assert-flat decision as a pure function over a row set
  so it is unit-testable without network; it reuses `flat_verdict`.
- **Patterns to follow:** the existing resting-row filter in `order_reconcile_teardown`
  (`parse_qty(&r.cheqty) == 0 && parse_qty(&r.ordrem) > 0`); `flat_verdict`.
- **Test scenarios:**
  - Pre-assert-flat refuses on a pre-existing **resting** row and on a pre-existing
    **filled** row (fill-free requirement), and proceeds on an empty/flat scan.
  - Teardown still selects every resting row for cancel (unconditional — no owned-set
    filtering), matching current behavior.
  - `Fill` at pre-assert-flat → refuse (distinct from a fill discovered mid-probe, which is
    the control's and routes to `UNEXPECTED-FILL`).
- **Verification:** twins green; pre-assert-flat refuses on any non-flat/non-fill-free
  pre-state; teardown remains unconditional; no ownership set exists in the code.

### U4. Promote the 6 read re-cert TRs to Recommended

- **Goal:** `t8412`, `CSPAQ12200`, `S3_`, `token`, `t1102`, `t1101` restored to
  `recommended: true` on witnessed clean chains.
- **Requirements:** R2.
- **Dependencies:** none (reads do not depend on the harness hardening); land the KTD5
  one-time wirings here if a read is the first flip.
- **Files:** `metadata/trs/{t8412,cspaq12200,s3_,token,t1102,t1101}.yaml`,
  `metadata/error-coverage/*.yaml`, `crates/ls-docgen/src/lib.rs` (banner + `recommended_no_banner`),
  `crates/ls-trackers/src/cli.rs` (freshness count), regenerated `docs/reference/`.
- **Approach:** Per TR, run the promote-tr recipe
  (`.agents/skills/promote-tr/SKILL.md`) — control smoke + differential negative probe
  (`token` leg LAST), capture credential-free error-coverage evidence, flip
  `support.recommended: true`, ensure `constraints_ref`/`error_coverage_ref` present, update
  the docgen banner + freshness count, regen docs. `S3_` records its realtime
  error-coverage substitute (no live negative probe — realtime excludes differential
  probing). The KTD5 first-flip re-wiring and count/banner edits are applied **serially by
  the orchestrator**, not delegated to independent fresh-context subagents (per KTD5); if
  `tr-promoter` subagents run per-TR, they run strictly sequentially with the first-flip
  edit committed before the next, or the count/docgen reconciliation is deferred to U6.
- **Execution note:** Live-attended; witness each terminal certify-line before flipping. A
  failed/inconclusive probe → leave that TR Implemented with a HELD record (KTD4).
- **Patterns to follow:** the `promote-tr` recipe end-to-end; §25 per-TR reopen commands.
- **Test scenarios:**
  - `cargo test -p ls-core` green (constraint-schema grounding + policy cross-check) after
    each flip.
  - `make docs-check` green (generated docs match committed).
  - Docgen banner test reflects each newly-Recommended read; freshness count matches the
    flip count.
  - `Test expectation:` the live smokes themselves are `#[ignore]` and operator-run — the
    gate never runs them; certification is the witnessed terminal line, not a CI assertion.
- **Verification:** each promoted read carries `recommended: true` + refs; docgen/freshness
  reconciled; gate green; HELD reads carry a recorded arm.

### U5. Promote the order quartet using the hardened harness

- **Goal:** `CSPAT00601`, `CSPAT00701`, `CSPAT00801`, `t0425` restored to
  `recommended: true`, certified by the hardened (U1–U3) order probe.
- **Requirements:** R1, R2.
- **Dependencies:** U1, U2, U3 (the hardened harness is the certify evidence); KTD5 wirings
  (here if the quartet contains the first flip).
- **Files:** `metadata/trs/{cspat00601,cspat00701,cspat00801,t0425}.yaml`,
  `metadata/error-coverage/*.yaml`, `crates/ls-docgen/src/lib.rs`,
  `crates/ls-trackers/src/cli.rs`, regenerated `docs/reference/`.
- **Approach:** With the attended autonomy chain armed (`LS_ORDER_SMOKE=1`,
  `LS_ORDER_SMOKE_NONCE=$(date +%s)`, attended TTY), run the order control smokes + the
  three hardened `make live-smoke-cspat00{6,7,8}01-negative` legs and the `t0425` read
  probe. Witness that: **(a)** modify/cancel variant lines print against a *resting* control;
  **(c)** pre-assert-flat gated placement and teardown left no residue. **(b) is closed by
  offline twin + a live check that the scan request carries the fill-inclusive `chegb` — the
  fill-detection branch is NOT exercised live** (the no-fill posture guarantees the control
  never fills, so a `chegb="0"` scan returns the same empty/resting set a `chegb="2"` scan
  would). Between order legs, confirm the book is flat (clear any stranded control) so the
  next leg's pre-assert-flat does not self-HELD (KTD3). Capture credential-free evidence,
  flip the quartet, update docgen/freshness, regen docs. Any fail-closed arm → that TR HELD
  (KTD4).
- **Execution note:** Live-attended, time-critical (needs the open regular window; observe
  the Open Questions hard cutover time). A stranded arm (panic without a preceding
  `flatten=confirmed`) halts remaining order legs; a stranded resting control will also trip
  the next leg's pre-assert-flat until manually cleared.
- **Patterns to follow:** `run_order_negative_probe` (hardened); the §25 quartet reopen
  command; the `promote-tr` recipe.
- **Test scenarios:**
  - `cargo test -p ls-sdk --test negative_probe` green (all offline twins, incl. U1–U3
    additions).
  - `cargo test -p ls-core` green after each flip.
  - Docgen banner + freshness count reflect each newly-Recommended order TR.
  - `Test expectation:` live order legs are `#[ignore]`; certification is the witnessed
    terminal certify-line, not a CI assertion.
- **Verification:** promoted quartet carries `recommended: true` + refs; gaps (a) and (c)
  are witnessed closed in the live output, gap (b) by offline twin + live fill-inclusive
  `chegb` assertion; gate green; HELD quartet members carry a recorded arm.

### U6. Ledger §26 disposition + full gate + smoke-map reconciliation

- **Goal:** The wave is recorded and the tree is green regardless of how many TRs flipped.
- **Requirements:** R2, DoD.
- **Dependencies:** U4, U5.
- **Files:** `metadata/PROVISIONALITY-LEDGER.md` (new §26),
  `.agents/skills/promote-tr/references/smoke-map.md`, regenerated `docs/`.
- **Approach:** Write ledger §26 as the current disposition record for the ten re-cert TRs,
  superseding §25 in place: per-TR promoted-or-HELD terminal state + arm, the final
  Recommended count, and the gap-close outcome. Reconcile the smoke-map status column for
  every flipped TR. Confirm the count tally (`reference.len()` unchanged unless a flip also
  changed the reference set; `recommended_no_banner` / freshness count reflect actual
  flips).
- **Execution note:** If **0** TRs flipped (window missed / account unavailable), §26 still
  closes the wave with all-HELD and the KTD5 wirings do NOT land — mirror the §25 AE2 path.
- **Patterns to follow:** §24/§25 ledger section structure; the promote-tr count-tally
  discipline.
- **Test scenarios:**
  - Full root gate green: `make docs; cargo test; cargo test -p ls-core; make docs-check;
    make lane-check`.
  - `Test expectation:` §26 prose + smoke-map are docs — covered by `make docs-check` for
    generated artifacts; the ledger/smoke-map edits are hand-authored and reviewed.
- **Verification:** §26 present and internally consistent with the metadata flips; smoke-map
  status matches; full gate green.

---

## Verification Contract

- **Offline gate (every commit):** `make docs; cargo test; cargo test -p ls-core;
  make docs-check; make lane-check` — all green. Clippy is **not** in the root gate.
- **Harness offline twins:** `cargo test -p ls-sdk --test negative_probe` green, including
  the U1–U3 additions and the unchanged `flat_verdict` / `order_probe_classes` /
  `is_order_placement_success` spine.
- **Live certification (operator-witnessed, not gated):** each promoted TR flips only on its
  in-session terminal certify-line. Gaps (a) and (c) are witnessed live (resting-control
  modify/cancel; pre-assert-flat gated placement + residue-free teardown); gap (b) is closed
  by offline twin + a live assertion that the scan request carries the fill-inclusive
  `chegb` — its fill-detection branch cannot be exercised under the no-fill posture.
- **Fail-closed:** any live leg that cannot certify leaves its TR Implemented with a HELD
  arm; the wave still closes. Order legs share one book — clear any stranded control between
  legs so a later leg's pre-assert-flat is not self-HELD.

## Definition of Done

- The 3 order-probe gaps are closed in `negative_probe.rs`: (a) reorder verified by the U5
  live witness (offline cannot observe it), (b) fill-inclusive scan + per-consumer twins,
  (c) pre-assert-flat + unconditional teardown twins — all offline twins green; the
  smoke-map no longer overstates order-leg coverage.
- Each of the ten §25 TRs is either promoted to `recommended: true` on a witnessed chain or
  HELD with a recorded arm.
- The Recommended count reflects actual flips; the KTD5 one-time re-wirings landed iff there
  was at least one flip.
- Ledger §26 records the disposition and supersedes §25 for the ten TRs.
- The full root gate is green.

---

## Sources & Research

- Origin brainstorm: this file's requirements-only revision (ce-brainstorm, 2026-07-06).
- Ledger §24 (16-TR domestic residue) and §25 (re-cert wave armed, all HELD):
  `metadata/PROVISIONALITY-LEDGER.md`.
- Deferred order-probe gaps (verbatim): memory `lab-and-recert-waves-landed-2026-07-04`
  (PR #92, squash `4dd6901`).
- Harness: `crates/ls-sdk/tests/negative_probe.rs` — `run_order_negative_probe` (order
  lifecycle), `scan_symbol_working_orders` / `flat_verdict` (flat-verify),
  `order_reconcile_teardown` (teardown).
- Recipe + registry: `.agents/skills/promote-tr/SKILL.md`,
  `.agents/skills/promote-tr/references/smoke-map.md` (rows 60–68 = re-cert legs).
- Count/docgen sites: `crates/ls-docgen/src/lib.rs:1413` (`recommended_no_banner`),
  `crates/ls-trackers/src/cli.rs:2373` (freshness count assertion).
