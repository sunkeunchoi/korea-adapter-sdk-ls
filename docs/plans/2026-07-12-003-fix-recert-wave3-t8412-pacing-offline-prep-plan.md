---
title: "fix: §27 t8412-pacing + guard-bookkeeping — offline prep for Monday re-cert wave 3"
date: 2026-07-12
type: fix
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
product_contract_source: ce-plan-bootstrap
origin: docs/brainstorms/2026-07-06-recert-wave2-live-followup-fix-prompt.md
tracks: [issue-117, ledger-§27]
---

# fix: §27 t8412-pacing + guard-bookkeeping — offline prep for Monday re-cert wave 3

## Summary

Land the **offline, KRX-closed-safe** prep that unblocks Monday's attended KRX-open order
re-cert window (issue #117 / ledger §27), so the in-window re-probes do not fail-closed or
throttle-mask. Two of the three items the prompt scoped are already resolved in the tree; this
plan carries the one genuine code fix, a bookkeeping correction so Monday's operator is not
misled by a stale disposition, and a merge step for an already-written PR.

**Scope reality (confirmed by `git blame` + code read on 2026-07-12):**

- **Item 4 (the `cts_ordno` single-page guard) is ALREADY LANDED.** `scan_page_is_terminal`
  (keying on the `cts_ordno` **body** cursor — terminal on empty / `" "` / `"0"`) was introduced
  in commit `a9974a9` (PR #106, merged 2026-07-07) in **both** twins, with offline terminality
  tests present and green. Issue #117 item 4 and ledger §27 follow-up (4) describe it as unfixed —
  that disposition is **stale**. → becomes a bookkeeping correction (U2), not a code fix.
- **Item 3 (t8412 pacing) is genuinely unfixed** — the real code work (U1).
- **PR #107 is `MERGEABLE` / `CLEAN`** — a review-and-merge step, sequenced first.

Live certification of all three is **out of scope** (deferred to the Monday 09:00–15:30 KST
window): the gate's live smokes are `#[ignore]`, so offline-green = landed-but-UNCERTIFIED by
design.

**Product Contract preservation:** No upstream unified-plan Product Contract exists (solo
invocation, origin is a hand-off prompt draft). Scope was confirmed with the user after the
already-landed-guard finding.

---

## Problem Frame

The re-cert wave 2 live re-probe (2026-07-06, ledger §27) left six TRs HELD for three reasons.
The order-quartet guard bug (reason C) has since been fixed in PR #106, but two follow-ups remain
before Monday's in-window re-probe can exercise the differentials on merits:

1. **t8412 throttle-masking (§27 reason A).** `live_smoke_t8412_negative`
   (`crates/ls-sdk/tests/negative_probe.rs`) runs its **own** standalone probe loop with an inline
   `fire`, firing ~12 rapid market-data calls with **no inter-dispatch pace**. Every variant
   returned `IGW00201` (a self-inflicted throttle). A throttle classifies as a non-success
   rejection → `Clean`, so t8412's "all Clean" is **FALSE** — the differential was never evaluated.
   Every other read leg (t1101 / t1102 / CSPAQ12200 / t0425) already routes through the shared,
   U6-paced `run_inblock_negative_probe(…, pace)`; t8412 is the sole holdout.

2. **Stale disposition.** §27 and issue #117 still record reason C (the guard bug) as open work.
   The guard fix landed in PR #106 as a rider **after** §27 was written, so an operator reading §27
   on Monday would re-do finished work or mis-scope the session.

Monday's operator needs: (a) a paced t8412 probe that reads on merits, and (b) an accurate
disposition telling them the guard is done and only the live re-probe remains.

---

## Requirements

- **R1** — Route `live_smoke_t8412_negative` through the shared `run_inblock_negative_probe`
  helper so it inherits the U6 inter-dispatch pacing, with a non-zero market-data-appropriate
  pace. (§27 reason A / issue #117 item 3)
- **R2** — Preserve t8412's `gateway_tolerant` `(field, class)` downgrade behavior (`shcode/required`
  + `sdate`/`edate`/format read `expected-tolerant`; `shcode/format` still DIVERGES). The shared
  helper already applies this via `reported_outcome`.
- **R3** — Correct ledger §27 (and note issue #117) to record that the reason-C guard fix landed in
  PR #106 (`a9974a9`); the only remaining reason-C work is the Monday live re-probe.
- **R4** — Do not regress the already-landed `cts_ordno` guard: the offline terminality twins in
  both `order_smoke.rs` and `negative_probe.rs` must stay green after U1's edits to the shared file.
- **R5** — Offline gate stays green (`make docs && cargo test && cargo test -p ls-core &&
  make docs-check && make lane-check`); no gateway / live smoke; no full `cargo fmt` of `ls-trackers`.
- **R6** — One branch, one PR targeting `main` for U1–U3, titled for the §27 t8412-pacing +
  bookkeeping fixes, noting it unblocks the Monday re-cert-wave-3 window (#117). PR #107 lands as a
  separate, prerequisite merge.

---

## Key Technical Decisions

- **KTD-1 — Convert, don't patch-in-place.** Replace the t8412 standalone loop body with a call to
  `run_inblock_negative_probe("t8412", "/stock/chart", "t8412InBlock", valid_seed(), <pace>)` rather
  than sprinkling a `sleep` into the bespoke inline loop. Rationale: it collapses the last bespoke
  read leg onto the shared paved path, inherits U6 pacing **and** the `reported_outcome` tolerant
  downgrade (which the standalone loop currently re-wires inline — see negative_probe.rs:236-238),
  and deletes the duplicated inline `fire` + classification. Issue #117 item 3 prescribes exactly
  this ("route the t8412 loop through the shared account-lane helper").

- **KTD-2 — Pace value: non-zero, market-data-sized, ~250 ms recommended.** The market-data bucket
  is 10/s (100 ms period), but `IGW00201` is a *warm-sensitive cumulative* budget (see
  `igw00201-budget-characterization`), and t8412 fires ~12 calls (control + 11 variants). Sibling
  market-data legs (t1101/t1102) pass `Duration::ZERO` and do not throttle at their lower variant
  count; t8412's higher count + warm budget is what trips it. Recommend `Duration::from_millis(250)`
  (2.5× the bucket period, ~3 s total) — ample headroom under 10/s, well below the 1500 ms
  account-lane pace. **The final value is confirmed by the Monday in-window re-probe;** the offline
  proof only asserts the pace is non-zero.

- **KTD-3 — Verify `fire_inblock` header parity during implementation.** The inline t8412 `fire`
  posts `{ "t8412InBlock": inblock }` with headers `tr_cd`/`tr_cont: "N"`/`tr_cont_key: ""`. The
  shared `fire_inblock` is used by t1101/t1102 against `/stock/chart`-class read paths; confirm it
  emits equivalent headers for t8412 (an execution-time check, not a plan-time unknown — see U1).

- **KTD-4 — Bookkeeping lives in the ledger; the issue is a courtesy comment.** `PROVISIONALITY-LEDGER.md`
  is the in-repo source of truth (AGENTS.md), so the durable correction is a §27 addendum (U2). The
  GitHub issue #117 comment is an outward write; carry it as a landing-step note, not an in-repo unit.

- **KTD-5 — Merge PR #107 first, then branch.** PR #107 edits the same file U1 touches
  (`negative_probe.rs`: `classify_fired_variant` / the `http>=500` arm). Merge #107 to `main`
  first, then branch U1–U3 off the freshly-merged main to avoid a same-file conflict.

---

## Implementation Units

### U1. Pace the t8412 negative-probe loop via the shared helper

**Goal:** Eliminate t8412's self-inflicted `IGW00201` throttle by routing its live differential
probe through the shared U6-paced helper, so Monday's re-probe evaluates the variants on merits.

**Requirements:** R1, R2.

**Dependencies:** PR #107 merged to main first (KTD-5). No U-unit dependency.

**Files:**
- `crates/ls-sdk/tests/negative_probe.rs` — rewrite `live_smoke_t8412_negative` (currently ~lines
  153-260) to delegate to `run_inblock_negative_probe`; introduce a `const T8412_PROBE_PACE:
  Duration` (KTD-2). Remove the now-dead inline `fire`, `responded_ok`, and inline classification
  block if no other caller uses them (grep before deleting). Keep `valid_seed()` and
  `t8412_schema()` (still referenced by offline tests).
- `crates/ls-sdk/tests/negative_probe.rs` — the test module (same file): add
  `t8412_probe_is_paced` (offline unit test, below).

**Approach:**
- Replace the body of `live_smoke_t8412_negative` with:
  `run_inblock_negative_probe("t8412", "/stock/chart", "t8412InBlock", valid_seed(), T8412_PROBE_PACE).await;`
  (directional — confirm the `#[ignore]` attribute and make-target name `live_smoke_t8412_negative`
  are preserved verbatim so `make live-smoke-t8412-negative` and smoke-map.md still resolve).
- Define `const T8412_PROBE_PACE: Duration = Duration::from_millis(250);` near the other pace
  literals, with a comment citing §27 reason A + the cumulative-budget rationale (KTD-2).
- Verify header parity (KTD-3): confirm `fire_inblock` sends `tr_cont: "N"` / `tr_cont_key: ""` for
  t8412; if it differs materially from the inline `fire`, reconcile before deleting the inline path.
- Confirm the shared helper's `reported_outcome` downgrade fires for t8412's tolerant pairs (it is
  keyed off the embedded constraint schema via `schema_for("t8412")`, which the helper already
  loads) — this is what makes the standalone loop's inline downgrade redundant (R2).

**Patterns to follow:** the existing `live_smoke_t1102_negative` / `live_smoke_cspaq12200_negative`
wrappers (negative_probe.rs:450-483) — thin delegations passing a `pace` literal. Mirror that shape.

**Execution note:** This is a `#[ignore]` live smoke, so anti-throttle behavior is only observable
in-window (Monday). Offline, prove structural correctness: the leg delegates to the shared helper
with a non-zero pace, and the suite compiles and stays green.

**Test scenarios:**
- **`t8412_probe_is_paced`** (new, offline `#[test]`): assert `!T8412_PROBE_PACE.is_zero()` — the
  offline proxy for "no longer self-throttles" (the true anti-throttle proof is the Monday
  re-probe). Optionally also assert `T8412_PROBE_PACE >= Duration::from_millis(100)` (≥ the
  market-data bucket period).
- **Regression (existing, must stay green):** `negative_probe_offline_twin` — variant generation
  + classification still holds after the inline-loop removal.
- **Compile/structure:** `cargo test -p ls-sdk --test negative_probe` compiles with the inline
  `fire`/`responded_ok` removed and no dead-code warnings promoted to errors.

**Verification:** `cargo test -p ls-sdk --test negative_probe` green; `t8412_probe_is_paced`
present and passing; the `#[ignore]` live smoke still lists under `--list`; `make live-smoke-t8412-negative`
target unchanged (name preserved).

---

### U2. Correct the §27 disposition — guard fix landed in PR #106

**Goal:** Update the durable in-repo disposition so Monday's operator sees the reason-C guard as
DONE (PR #106) with only the live re-probe remaining — not as open code work.

**Requirements:** R3.

**Dependencies:** none (independent of U1; can land in the same PR).

**Files:**
- `metadata/PROVISIONALITY-LEDGER.md` — add a dated addendum to §27 (or a short §27-follow-up note)
  recording: reason-C guard fix landed in PR #106 (`a9974a9`, 2026-07-07) in both
  `scan_symbol_working_orders` twins via `scan_page_is_terminal` (cts_ordno body cursor); offline
  terminality twins present + green; remaining reason-C work is the attended Monday live re-probe
  of `cspat00601/00701/00801` after the stranded 005930 order is cleared.

**Approach:** Preserve §27's existing narrative (it is the historical record of the 2026-07-06
probe); append the correction rather than rewriting the reason-C paragraph, so the audit trail
stays intact. Cross-reference the commit hash and PR number.

**Patterns to follow:** existing §-addendum style in `PROVISIONALITY-LEDGER.md` (dated, PR-linked,
fail-closed framing).

**Test scenarios:** `Test expectation: none -- documentation-only; no behavioral change.`

**Verification:** `make docs-check` green (ledger is not docgen-generated, but confirm no docgen
cross-reference breaks); the addendum names PR #106 / `a9974a9` and the two twin locations.

---

### U3. Guard-fix regression safety net (verification-only)

**Goal:** Confirm U1's edits to `negative_probe.rs` do not silently regress the already-landed
`cts_ordno` guard, and that the two offline terminality twins remain equivalent.

**Requirements:** R4.

**Dependencies:** U1 (this checks U1 did not disturb the guard region).

**Files:**
- `crates/ls-sdk/tests/negative_probe.rs` and `crates/ls-sdk/tests/order_smoke.rs` — no new
  production code; confirm `scan_page_terminality_keys_on_the_cts_ordno_body_cursor_not_tr_cont`
  (negative_probe.rs:1765, order_smoke.rs:776) still pass and cover the same cases
  (empty / `" "` / whitespace / `"0"` / real cursor / trimmed-real). If the negative_probe twin is
  thinner than the order_smoke twin, add the missing assertion(s) to bring them to parity.

**Approach:** Diff the two twin tests; if identical coverage, this is a pure run-and-confirm
checkpoint. Only add assertions if a genuine coverage gap exists between the twins.

**Patterns to follow:** the order_smoke.rs terminality twin (the more complete of the two).

**Test scenarios:** `Test expectation: none -- verification-only; reuses/optionally levels the
existing twin tests.` (If a gap is found and an assertion is added, that assertion is the scenario:
"a real order-number cursor is paginated / fail-closed".)

**Verification:** both terminality twins green after U1; twin coverage confirmed equivalent.

---

## Scope Boundaries

**In scope (this PR):** U1 (t8412 pacing), U2 (§27 bookkeeping), U3 (guard regression check).
All offline, KRX-closed-safe.

**Prerequisite tracked step (separate existing PR):** Review + merge PR #107
(`fix/igw40011-placed-nothing-order-differential` → main, `MERGEABLE`/`CLEAN`). Merge **before**
branching U1–U3 (KTD-5).

### Deferred to Follow-Up Work (Monday in-window, bucket B — NOT this plan)

- Live certification of all three items — the `#[ignore]` smokes run attended, open-KRX
  (09:00–15:30 KST). Offline-green here = landed-but-UNCERTIFIED by design.
- Clearing the stranded 005930 band-floor buy (#117 item 5) — live cancel, KRX-open only.
- #117 item 2 — operator decision + re-probe of `t0425 medosu/required` & `CSPAQ12200 BalCreTp/required`
  (mark `gateway_tolerant:[required]` vs `required:false`).
- #117 item 1 — promote `t1102` (the one CLEAN cert) via `promote-tr`.
- Any live smoke / gateway call — order autonomy refuses unattended live runs regardless.

### Out of scope (not planned work)

- Re-hardening or re-implementing the `cts_ordno` guard — it is already correct (PR #106); U3 only
  guards against regression.

---

## Verification Contract (offline only)

Run the AGENTS.md order-surface / SDK gate — all must be green:

- `make docs`
- `cargo test` (workspace)
- `cargo test -p ls-core` (metadata validation + policy index cross-check)
- `make docs-check`
- `make lane-check`

Focused proofs:
- `cargo test -p ls-sdk --test negative_probe` green, including new `t8412_probe_is_paced`.
- Both `scan_page_terminality_keys_on_the_cts_ordno_body_cursor_not_tr_cont` twins green (U3).
- `make live-smoke-t8412-negative` target name unchanged (leg still resolves).

Do **not** `cargo fmt` the whole `ls-trackers` crate (main is intentionally unformatted there).

**Live certification is explicitly deferred to the Monday KRX-open window** — no gateway call is
made or asserted by this plan.

---

## Landing Strategy

1. Review + merge PR #107 to `main` (prerequisite; resolves the same-file sequencing with U1).
2. Branch off the freshly-merged `main` (do not work on main): e.g.
   `fix/recert-wave3-t8412-pacing-guard-bookkeeping`.
3. Implement U1 → U2 → U3.
4. Run the full offline gate (Verification Contract).
5. One PR → `main`, titled for the §27 t8412-pacing + guard-bookkeeping fixes; body notes it
   unblocks the Monday re-cert-wave-3 window (#117) and that live certification is deferred.
6. Landing-step note (courtesy, outward write — operator-confirmable): comment on issue #117 that
   item 4 (guard) already landed in PR #106 and item 3 (t8412 pacing) is now fixed offline;
   items 1/2/5 remain the Monday in-window bucket.

---

## Risks & Dependencies

- **R-1 — `fire_inblock` header divergence (low).** If the shared helper's headers differ from the
  inline t8412 `fire`, the live control could HELD on Monday. Mitigation: KTD-3 parity check before
  deleting the inline path; the control-failure path already prints an explicit HELD (not a false
  divergence).
- **R-2 — Pace still too tight for the cumulative budget (low, deferred).** 250 ms may be
  insufficient if the budget is unusually warm Monday. Mitigation: the pace is a single named const;
  the operator can bump it in-window. Only the non-zero property is asserted offline (KTD-2).
- **R-3 — Same-file conflict with PR #107 (mitigated).** Both edit `negative_probe.rs`. Mitigation:
  merge #107 first, branch after (KTD-5).
- **R-4 — Dead-code removal breaks a hidden caller (low).** Removing the inline `fire`/`responded_ok`
  could break another reference. Mitigation: grep before delete; the compiler catches the rest.

---

## Sources & Research

- Issue #117 (full disposition) — `open`, order-quartet re-cert wave 3.
- `metadata/PROVISIONALITY-LEDGER.md` §27 — the 2026-07-06 live re-probe disposition (reasons A/B/C).
- `docs/brainstorms/2026-07-06-recert-wave2-live-followup-fix-prompt.md` — origin hand-off draft.
- `git blame` — `scan_page_is_terminal` introduced in `a9974a9` (PR #106) in both twins → item 4
  already landed.
- `gh pr view 107` — `MERGEABLE` / `CLEAN` / base `main`.
- `crates/ls-sdk/tests/negative_probe.rs` — `run_inblock_negative_probe` (U6 pacing pattern, line
  350), `live_smoke_t8412_negative` (unpaced standalone loop, line 153), `reported_outcome` tolerant
  downgrade (line 412).
- `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md` — the guard
  KTD-1 learning.
- Memory: `igw00201-budget-characterization` — IGW00201 is a large warm-sensitive *cumulative*
  budget, not a pure rate (informs KTD-2).
