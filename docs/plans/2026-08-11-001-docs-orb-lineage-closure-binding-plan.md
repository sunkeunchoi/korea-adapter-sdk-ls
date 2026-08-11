---
title: ORB Lineage Closure Binding - Plan
type: docs
date: 2026-08-11
topic: orb-lineage-closure-binding
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: docs
---

# ORB Lineage Closure Binding - Plan

## Goal Capsule

- **Objective.** Execute P0 of the next-lineage prerequisite ladder: formally declare the ORB [[Strategy lineage]] CLOSED under the pre-registered [[Lineage closure]] rule, create the committed record that answers "which lineage is open?", and correct the two queue items whose premises the closure changes.
- **Product authority.** This plan owns the closure declaration's form and content, the standing open-lineage marker, the additive closure marker on the frozen `PREREGISTRATION.md` status line, and the queue corrections. It does not own opening the successor lineage (P6 of `docs/plans/2026-08-10-001-docs-next-strategy-lineage-scope-plan.md`), any edit to the CONCEPTS.md rule itself, or any re-adjudication of the arm-C stand-down.
- **Execution profile.** Documentation and queue bookkeeping only — a TURN-LOG edit, one header line, and `lab-next` mutations. No strategy code, no governed param, no ingest, no backtest.
- **Open blockers.** None. Every input is already committed: the rule (`CONCEPTS.md`), the stand-down decision (TURN-LOG 2026-08-10, PR #264), and the numbers (`adapters/nautilus/lab/config/sample-margin.json`).
- **Tail ownership.** Queue transitions run through `lab-next`, never by editing `queue/items.jsonl`. The P0 item `orb-stand-down-bind-to-closure-rule` closes via `lab-next done` when this work lands.

---

## Product Contract

### Summary

Write the missing formal closure: a dated TURN-LOG declaration that evaluates the Lineage-closure rule's two sides — including the admissibility basis that makes `−0.0006` the honest "best ever" — plus a standing **Open lineage** block at the top of TURN-LOG that the successor's pre-registration gate cites, and two queue corrections that follow from the closure.

### Problem Frame

The ORB arc stood down on arm C (TURN-LOG 2026-08-10, PR #264) and the closure rule landed in CONCEPTS.md (PR #265), but the two were never bound: the word CLOSED appears nowhere in TURN-LOG — every entry says "STANDS DOWN", and only queue items ever "CLOSE". The successor lineage's pre-registration may be frozen only when "exactly one lineage is open" holds, yet no committed file anywhere names which lineage is currently open; the rule is stated abstractly in `adapters/nautilus/CONTEXT.md` and `CONCEPTS.md` while the state is left to inference. TURN-LOG has met this failure shape before — "which run is THE head" was ambiguous across dated entries until the `## Head lineage (STANDING)` block pinned the current-state answer in one place.

The declaration is also not ceremonial. Read naively, the rule's "best net RoR the lineage has ever produced" is v32's `0.1876` — which exceeds the `+0.128605` threshold and would defeat the closure. The rule holds only on a stated admissibility basis, and no artifact states it.

Separately, the P0 queue item's retire-list is stale: of the seven items it names as "premised on a reopening", five are already superseded, and the 2026-08-10 scope plan explicitly keeps one of the remaining two.

### Key Decisions

- KD1. **Declaration and state marker both live in TURN-LOG.** (session-settled: user-approved — chosen over a CONTEXT.md marker or a declaration-only shape: one file, same precedent as the `## Head lineage (STANDING)` block, no cross-file drift.) Governs R1, R4.
- KD2. **The declaration states its admissibility basis rather than assuming it.** Without the basis paragraph the declaration fails its own rule (v32's `0.1876` > threshold). Governs R3.
- KD3. **`rung1-ladder-reentry-margin-clearing-head` is re-noted, not retired.** (session-settled: user-approved — chosen over retiring it: the 2026-08-10 plan names it the carrier of rung-1 re-entry; only its ORB-margin premise dies.) Governs R6.
- KD4. **The declaration binds on the frozen threshold `+0.128605` at the 237-session ceiling.** (session-settled: user-approved — chosen over re-deriving at the re-probed 240 sessions: the re-probe is noted as direction-preserving, and both figures close by two orders of magnitude.) Governs R2.

### Requirements

**The closure declaration**

- R1. A dated governance-axis TURN-LOG entry declares the ORB lineage CLOSED under [[Lineage closure]], citing the stand-down decision (TURN-LOG 2026-08-10, plan `docs/plans/2026-08-07-002-docs-orb-sample-acquisition-close-plan.md`) as the decision it binds.
- R2. The entry evaluates both sides of the rule with no judgment calls: the frozen margin threshold at the obtainable-sample ceiling (`+0.128605` at 237 sessions, per KD4 with the 240-session re-probe noted as direction-preserving) against the best admissible net RoR (`−0.0006`, v35).
- R3. The entry states the admissibility basis for "best net RoR ever produced": measurements count only on the honest basis — real catalog, armed transaction-cost model, size-invariant net RoR — which rules out v32 (`0.1876`, old data, pre-cost) and v34 (`0.0398`, real data, pre-cost), and names why the basis must be explicit: the naive reading defeats the rule.

**The standing open-lineage marker**

- R4. A standing `## Open lineage (STANDING)` block sits at the top of TURN-LOG beside the head-lineage block, currently stating: NONE — ORB CLOSED 2026-08-10, with a pointer to the closure entry, and that the successor opens only when its pre-registration freezes (P6).
- R5. The block is the citable authority for the one-lineage-at-a-time gate: the successor's pre-registration freeze cites it, and it is edited only when a lineage opens or closes.

**Queue corrections**

- R6. `rung1-ladder-reentry-margin-clearing-head` is superseded by a successor-premised item: its unblock becomes "the successor lineage's certified head clears its own frozen margin", with no reference to ORB's margin (which no head can ever clear now), and the attended-session choreography its note carries is preserved.
- R7. `report-sample-catalog-read-cost-deferred` stays standing unchanged — its note is already stand-down-aware and its work is catalog-size-independent.
- R8. Nothing already superseded is touched: of the P0 note's seven-item retire-list, five are already superseded and require no action.

**Frozen ORB artifacts**

- R9. `adapters/nautilus/lab/config/PREREGISTRATION.md` gains one additive status line noting the lineage closure with a pointer to the declaration; every frozen artifact's existing content — `preregistration.json` (v2), `sample-margin.json` — stays byte-identical as the lineage's historical record.

### Acceptance Examples

- AE1. Covers R2, R3.
  - **Given** a reader who checks the closure against TURN-LOG history and finds v32's RoR `0.1876` above the `+0.128605` threshold,
  - **When** they read the declaration,
  - **Then** the admissibility basis answers them in place: v32 is old-data and pre-cost, so it is not an admissible "best ever", and the closure holds on `−0.0006`.
- AE2. Covers R4, R5.
  - **Given** the successor lineage reaching its pre-registration freeze (P6),
  - **When** the freeze gate needs to prove "exactly one lineage is open",
  - **Then** it cites the standing block and finds NONE with the closure's date and provenance — no excavation of dated entries required.
- AE3. Covers R6.
  - **Given** that the successor lineage's own margin does not exist until P6 freezes it,
  - **When** the re-noted rung-1 item states its unblock,
  - **Then** the unblock names the successor's future frozen margin generically rather than any number, and carries no ORB-margin reference.
- AE4. Covers R8.
  - **Given** the stale seven-item retire-list in the P0 note,
  - **When** the queue corrections execute,
  - **Then** exactly one supersede occurs (R6) and no already-superseded item is re-touched.

### Scope Boundaries

- No edit to the CONCEPTS.md [[Lineage closure]] or [[Search budget]] entries — the rule is already correct; this work applies it.
- No re-adjudication of the arm-C stand-down — settled on measurement (PR #264).
- No opening of the successor lineage and no pre-registration content — that is P6, blocked on this work.
- No content change to any frozen ORB artifact beyond R9's additive status line.
- No new state file, JSON registry, or schema for lineage status — the marker is prose in TURN-LOG, per KD1.

### Dependencies and Assumptions

- The closure evaluates at the obtainable-sample ceiling, so `+0.128605` (ceiling threshold) is the binding figure — not `+0.224823`, which is the same margin evaluated at the head's current 54-session sample and appears in the rung-1 item's note. Both are real quantities of the same frozen rule; the declaration must not conflate them.
- Queue statuses as verified 2026-08-11: of the P0 note's list, only `rung1-ladder-reentry-margin-clearing-head` and `report-sample-catalog-read-cost-deferred` are open; `orb-pivot-depth-probe-harness` was superseded by `pit-universe-depth-walk-t8410` on 2026-08-11.
- The absence claims this plan rests on — no CLOSED occurrence in TURN-LOG, no open-lineage marker in any committed file — were verified against the tree on 2026-08-11.

### Sources

- `CONCEPTS.md:241-248` — the [[Lineage closure]] and [[Search budget]] rule text this work applies.
- `adapters/nautilus/lab/TURN-LOG.md` — the 2026-08-06 / 2026-08-07 / 2026-08-10 stand-down chain; the `## Head lineage (STANDING)` precedent.
- `queue/items.jsonl` — `orb-stand-down-bind-to-closure-rule` (the P0 item and its stale retire-list), `rung1-ladder-reentry-margin-clearing-head`, `report-sample-catalog-read-cost-deferred`.
- `adapters/nautilus/lab/config/sample-margin.json` — the frozen threshold's provenance and the v35 `net_ror: -0.0006` figure.
- `adapters/nautilus/lab/config/PREREGISTRATION.md` — the dated-status-line header convention R9 extends.
- `docs/plans/2026-08-10-001-docs-next-strategy-lineage-scope-plan.md` — the P0 row, the `+0.1286` vs `−0.0006` citation, and the "Outside this lineage" bullet that keeps the rung-1 item.
- `docs/plans/2026-08-07-002-docs-orb-sample-acquisition-close-plan.md` — the arm-C decision the declaration binds.
- `docs/solutions/conventions/suspend-vs-amend-frozen-governance-artifacts.md` — the annotate-don't-amend convention KTD2 applies.

---

## Planning Contract

**Product Contract preservation:** unchanged, except Outstanding Questions Q1/Q2 — both were `Deferred to Planning` and are resolved here as KTD1 and KTD2; the section is removed rather than left stratified.

### Key Technical Decisions

- KTD1. **The standing block sits above the head-lineage block.** The `## Open lineage (STANDING)` block is inserted immediately after the file's intro paragraph and before `## Head lineage (STANDING)` — lineage state is higher-altitude than head identity, and a reader entering the file meets the open/closed answer first. The head-lineage block's own text is not modified. Resolves Q1. Governs the U1 layout.
- KTD2. **The status marker annotates; it never amends.** R9's addition to `adapters/nautilus/lab/config/PREREGISTRATION.md` is one more dated `·`-separated marker appended to the existing `**Status:**` line, pointing at the TURN-LOG closure entry only — the entry itself carries the successor-plan citation, so the marker does not name it. This follows `docs/solutions/conventions/suspend-vs-amend-frozen-governance-artifacts.md`: a frozen governance artifact records state changes as annotations, and no frozen value in `preregistration.json` or the document body changes. Resolves Q2.
- KTD3. **Verification is grep- and gate-shaped, not test-authored.** No new tests: the diff is two markdown files plus `lab-next`-mediated queue rows. The relevant existing guards are `make todo-check`, the queue/trials lab test suites (`next_cli`, `next_window`, `next_sequences`, `next_probe`, `todo_merge_block`, `trials`), and inspection of `make next` output. `lab/tests/trials.rs` counts ledger records with TURN-LOG source pointers, not TURN-LOG lines, so appending entries is safe — verified 2026-08-11 when the daily-depth entry landed.
- KTD4. **Queue mutation ordering follows the session convention.** The successor-premised item is added first, then the old item is superseded `--by` it (the CLI requires the target to exist); `lab-next done orb-stand-down-bind-to-closure-rule` fires only after the docs commit lands, so the queue never claims a declaration that is not yet in the tree.

---

## Implementation Units

### U1. TURN-LOG closure declaration and standing open-lineage block

- **Goal** — The formal CLOSED declaration exists and the "which lineage is open?" answer is committed.
- **Requirements** — R1, R2, R3 (declaration); R4, R5 (standing block). KD1, KD2, KD4 govern content; KTD1 governs layout.
- **Dependencies** — none.
- **Files** — `adapters/nautilus/lab/TURN-LOG.md`.
- **Approach**
  1. Insert the `## Open lineage (STANDING)` block after the intro paragraph (currently lines 1–5), before `## Head lineage (STANDING)`: state NONE, name ORB CLOSED 2026-08-10 with a pointer to the declaration entry, and state that the successor opens only at its pre-registration freeze (per R4, R5).
  2. Insert the declaration as the newest dated turn entry (above the 2026-08-10 daily-depth entry, currently line 63), house style `## Turn — … (2026-08-11) — plan 2026-08-11-001`, carrying the rule citation, both sides of the evaluation, and the admissibility basis per R1–R3.
- **Patterns to follow** — the existing `## Head lineage (STANDING)` block (canonical current-state section above the dated trail); the 2026-08-10 governance-axis entry (what-did-NOT-change opener, bolded decision lead).
- **Test scenarios**
  - Covers AE1. `grep -c "CLOSED" adapters/nautilus/lab/TURN-LOG.md` goes from 0 to ≥ 2 (standing block + declaration), and the declaration names v32 `0.1876` and v34 `0.0398` as inadmissible with the basis stated.
  - Covers AE2. The standing block contains NONE, the closure date, and a pointer a reader can follow to the declaration without scanning dated entries.
  - The `trials` lab test suite still passes (ledger-based counting is unaffected by appended entries).
- **Verification** — targeted lab test run green; both grep checks hold; the entry renders under the existing heading conventions (`grep "^## " TURN-LOG.md` shows the two STANDING blocks before the dated trail).

### U2. PREREGISTRATION.md closure marker

- **Goal** — The frozen ladder pre-registration visibly records that its lineage closed.
- **Requirements** — R9. KTD2 governs form.
- **Dependencies** — U1 (the marker points at the declaration entry).
- **Files** — `adapters/nautilus/lab/config/PREREGISTRATION.md`.
- **Approach** — Append one `·`-separated marker to the existing `**Status:**` line: lineage CLOSED 2026-08-10, declared 2026-08-11, pointing at the dated TURN-LOG closure declaration entry (per KTD2 — the entry, not the standing block, carries the rule evaluation and admissibility basis). No other byte in the file changes; `preregistration.json` is untouched.
- **Patterns to follow** — the line's existing dated markers (`LADDER STOOD DOWN 2026-07-31`, `RE-ENTRY CONDITION TIGHTENED 2026-08-06`).
- **Test scenarios** — Test expectation: none — one-line additive annotation to a frozen doc; U1's suite plus the diff itself are the evidence (`git diff --stat` shows exactly one changed line in this file).
- **Verification** — the Status line carries the new marker; `preregistration.json` has no diff.

### U3. Queue corrections through lab-next

- **Goal** — The queue reflects the closure: the rung-1 item is successor-premised, nothing stale is touched, and P0 closes.
- **Requirements** — R6, R7, R8; Goal Capsule tail ownership. KD3 governs the re-note; KTD4 governs ordering.
- **Dependencies** — U1, U2 committed (the re-note and the `done` both cite the landed declaration).
- **Files** — `queue/items.jsonl` (via `lab-next` only — never hand-edited).
- **Approach**
  1. `lab-next add` a successor-premised re-entry item (id in the style `rung1-ladder-reentry-successor-margin-head`): unblock is "the successor lineage's certified head clears its own frozen margin (P6 artifact)", no ORB-margin reference, preserving the attended-session choreography and exit-code contract text from the old item's note per R6.
  2. `lab-next supersede rung1-ladder-reentry-margin-clearing-head --by <new-id>`.
  3. No action on `report-sample-catalog-read-cost-deferred` (R7) or any already-superseded row (R8).
  4. After the U1/U2 commit lands: `lab-next done orb-stand-down-bind-to-closure-rule`.
- **Patterns to follow** — the 2026-08-11 ladder-staging session: add-then-supersede ordering, plan `--ref` on new items, no credentials in `--note`.
- **Test scenarios**
  - Covers AE3. The new item's note contains no ORB margin figure (`+0.224823` absent) and names the successor's future frozen margin generically.
  - Covers AE4. `git diff` on `queue/items.jsonl` shows exactly one added row and one superseded row before the `done`; no already-superseded row changes.
  - `lab-next list` shows the new item, shows the old rung-1 item as superseded, and (after step 4) shows the P0 item done. The `list` verb is window-independent; `make next` is informational only — its report filters by the live KRX window state, so an `open-attended` item is invisible outside the session window.
- **Verification** — `make todo-check` PASS; queue lab test suites green; `lab-next list` output matches the three scenario expectations.

---

## Verification Contract

| Gate | Command | Covers | Done signal |
|---|---|---|---|
| Legacy-TODO guard | `make todo-check` | U3 | `verdict PASS` |
| Queue + trials suites | `cargo test -q -p nautilus-ls-lab --test next_cli --test next_window --test next_sequences --test next_probe --test todo_merge_block --test trials` (from `adapters/nautilus`, LS_* env vars unset) | U1, U3 | all suites `0 failed` |
| Content checks | grep assertions from U1/U2 scenarios | U1, U2 | all hold |
| Queue state | `lab-next list` (from `adapters/nautilus`; window-independent) | U3 | new item present; old rung-1 item superseded |
| Queue state, post-land | `lab-next list` after U3 step 4 | U3 | P0 item done |

The full eight-step gate is not required: the diff is two markdown files and CLI-mediated queue rows, reaching no code, metadata, or generated docs. Per repo convention, queue mutations that close items happen after any gate run, and step 4 of U3 happens after the commit lands.

---

## Definition of Done

- The CLOSED declaration and `## Open lineage (STANDING)` block are committed in `adapters/nautilus/lab/TURN-LOG.md` (U1).
- The PREREGISTRATION.md Status line carries the closure marker with no other change to frozen artifacts (U2).
- The queue shows the successor-premised rung-1 item, the old item superseded, and — after landing — `orb-stand-down-bind-to-closure-rule` done (U3).
- All Verification Contract gates green.
- The change ships as one docs/governance PR; per operator preference, it takes the light review path.
