# Maintenance Work Queue — Pre-Close Pressure Test (Issue #9)

**Date:** 2026-06-17
**Status:** Requirements — ready for planning
**Scope:** Correct the framing and the proof item before closing issue #9; do not change the queue mechanics.

## Problem

Issue #9 (`Prove Maintenance Work Queue end-to-end`) is built and labelled, and the
runbook claims that closing it proves **Foundation Complete**. A pre-close review found
three gaps between what #9 claims and what it actually exercises:

1. **#9 nulls out every load-bearing field.** Affected TRs `none`, Baseline Promotion
   `Not needed`, Focused Evidence `Not needed`, and a self-review gate. The fields most
   likely to be wrong in real use (dependency-class selection, Change-Scoped Gate
   selection, baseline-as-separate-review, evidence refresh) are never tested. #9 proves
   the *form is fillable*, not that the *flow carries weight* — yet
   `MAINTENANCE_RUNBOOK.md:33-36` and `CONTEXT.md:87-89` claim it proves Foundation
   Complete.

2. **The completion contract lives in unenforced free text.** The issue template's
   "required" fields bind only the GitHub web creation form. Once an issue exists, the
   body is editable markdown. #9 already drifted from its own template (see gap 3). The
   only durable, queryable part of the contract is **labels** — and the three decision
   fields (gate / baseline / evidence) are not labels.

3. **#9's body diverged from the template in two ways**, because the template gave it no
   honest option: it invented an artifact category ("Issue template and label taxonomy")
   that `sdk_work_item.yml:89-95` does not offer, and it softened the required
   "Selected Change-Scoped Gate passed." checklist line (`sdk_work_item.yml:137`) to
   "passed or recorded not applicable."

## Decisions

### D1 — Foundation Complete is a two-stage gate

- **Stage 1 (issue #9):** proves the queue *plumbing* is self-consistent — labels,
  template fields, completion checklist, runbook path.
- **Stage 2 (first real SDK-facing item):** proves the *flow carries weight* — a real TR,
  a real Change-Scoped Gate, and real Baseline Promotion / Focused Evidence decisions.
- **Foundation Complete is claimed only after Stage 2 closes**, not at #9.
- Update `CONTEXT.md:87-89` and `MAINTENANCE_RUNBOOK.md:33-36` so the Foundation Complete
  language reflects the two stages and stops overclaiming what #9 proves.

### D2 — Labels are the contract; the body is reviewed prose

- Document plainly that **labels are the only machine-checkable part** of the Maintenance
  Work Queue contract, and the issue body (gate / baseline / evidence decisions) is
  **human-reviewed prose**, not enforced.
- "Closed cleanly" therefore means *a reviewer read the body and confirmed it*, not that
  any tool verified it. Make that explicit so future maintainers don't trust the body as a
  machine contract.
- **No new tooling, no new labels, no body-validation check** — rejected on carrying cost.

### D3 — Conform #9 to today's template (no template change)

- Drop the invented "Issue template and label taxonomy" artifact line; rely on the
  existing **"Runbook or operator documentation"** checkbox #9 already has.
- Replace the conditional gate with a **concrete, runnable gate that passes**:
  `make docs-check`. Record its result.
- Restore the template's exact completion wording: "Selected Change-Scoped Gate
  **passed.**" (drop the "or recorded not applicable" softener).
- Net effect: #9 becomes a stronger Stage-1 proof — it runs one real command that passes
  instead of self-certifying — without becoming a full SDK item.

## Out of scope (deliberately)

Rejected on carrying cost or deferred until evidence justifies them:

- Promoting gate / baseline / evidence decisions to labels.
- A body-validation CI/script check.
- Adding a "process / template / tooling" artifact category to the template.
- Sanctioning a "gate not applicable, with reason" completion path.
- A separate, lighter contract for cross-cutting / meta work.

## Outstanding questions (defer to Stage 2)

- **Single-select dependency class and support state** (`sdk_work_item.yml:57-82`) cannot
  represent a finding that spans multiple dependency classes or TRs. #9 does not test
  this. Let the first real SDK-facing item reveal whether forcing one class per issue is a
  real constraint before changing the template.

## Success criteria

- #9's body matches the committed template field-for-field, with `make docs-check` recorded
  as passed and the strict completion checklist intact.
- Foundation Complete language in `CONTEXT.md` and `MAINTENANCE_RUNBOOK.md` describes the
  two-stage gate and does not claim #9 alone proves it.
- The runbook states that labels are the machine-checkable contract and the body is
  reviewed prose.
- #9 can then be closed as Stage 1, with the next proof explicitly scoped as the first
  real SDK-facing maintenance or expansion item.
