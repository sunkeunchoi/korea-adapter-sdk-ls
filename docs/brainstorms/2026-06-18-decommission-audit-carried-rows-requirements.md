---
date: 2026-06-18
topic: decommission-audit-carried-rows
---

# Decommission Audit: Verify Carried Rows Before Trusting the Gate

## Summary

Before `korea-broker-sdk-ls` is trusted as decommission-ready, verify that every `carried` row in `docs/migration-source-extraction-ledger.md` is genuinely represented in this repo. A fleet of fresh-context agents — one per ledger row — audits each row against a bar tiered by row type, while the old source is still readable, and freezes a re-checkable per-row record. The deliverable is a *trustworthy* gate, not the decommission act itself.

---

## Problem Frame

The extraction ledger is the decommission gate: the old repo only becomes a **Decommissioned Migration Source** once every knowledge asset is `carried`, `extract`ed, `defer`red, or `discard`ed. As of `2026-06-14` the ledger reports no unresolved `extract` rows — 24 rows say `carried`, 2 say `discard`. On paper the gate is green.

But every `carried` row is an *assertion*: "this knowledge is represented in ls-core / a design doc / the metadata model." None of those assertions has been independently proven. If even one is wrong — a behavior that was described but never actually migrated, a design doc that dropped a constraint the old source carried — then decommissioning silently loses that knowledge.

The cost shape is asymmetric and time-bound. The completeness comparison that would catch a bad `carried` row requires reading the old source, and decommission is exactly what removes the old source. This audit is the last moment that comparison can be made. A wrong `carried` row caught now is a re-extraction; the same row caught after decommission is unrecoverable.

---

## Key Decisions

- **Tiered verification bar, not one uniform bar.** Most carried rows are knowledge or design, not runnable behavior (order rows are design-only per ADR 0008; the TR inventory is a JSON snapshot; runbook/diagnostics/release rows are captured lessons). A single "behavior proven here" bar would falsely fail ~17 rows that have no runtime to test. Behavioral rows are held to proof; knowledge rows to completeness-vs-source; discard rows to a sound reason.

- **Fresh-context agent fleet, one agent per row.** Reuses the proven `promote-trs`/`tr-promoter` precedent in this repo and the fresh-context verifier pattern. For behavioral rows this is a true guarantee — a test passes or fails objectively. For knowledge rows it only prevents self-approval; the completeness judgment is still subjective, so those rows carry the extra discipline in R7a.

- **Audit only — the deliverable is a trustworthy gate.** The physical decommission act (archive/tag/read-only/delete) is a separate follow-on. This work ends when the gate is provably green or every failing row is re-opened.

- **Freeze a re-checkable record now.** Because completeness review depends on the soon-to-vanish source, each row's verdict and evidence are captured durably so a confirmed gate stays defensible after the source is gone.

---

## Requirements

**Coverage and disposition**

- R1. The audit evaluates every `carried` row (24) and every `discard` row (2) in the ledger. No `carried` row is exempt.
- R2. Each audited row receives exactly one verdict: `confirmed`, `refuted`, or `unverifiable`.
- R3. A refuted `carried` row is re-dispositioned to `extract`, `defer`, or `discard` and re-blocks the gate until resolved — the audit never leaves a refuted row standing as `carried`.
- R4. An `unverifiable` row is recorded as an explicit assumption with the reason it could not be verified. It does not count as `confirmed`.
- R4a. An `unverifiable` row may be promoted to `assumption-accepted` when a named maintainer records an explicit acceptance and reason. `assumption-accepted` rows count toward a trustworthy-green gate; un-accepted `unverifiable` rows do not.

**Verification bars (tiered)**

- R5. Each row is first classified as behavioral, knowledge, or discard; the classification is recorded alongside the verdict.
- R6. A behavioral row is confirmed only by a passing test or recorded evidence in this repo. A doc that describes the behavior is not sufficient.
- R6a. A behavioral row whose genuine behavior is reachable only in production is `unverifiable`, never `confirmed` — this repo prohibits production trading tests, so a paper-environment smoke cannot stand in for production behavior.
- R7. A knowledge/design/lesson row is confirmed by completeness-vs-source review: a fresh reviewer compares the old source against the extracted target and agrees no material knowledge was lost.
- R7a. Completeness review is claim-by-claim, not holistic. The reviewer enumerates the discrete claims and constraints in the old source, maps each to a location in the extracted target, and records that claim-map as the row's evidence — a bare "looks complete" agreement is not a confirm. "Material" means any behavioral constraint, numeric value, response code, threshold, or edge case whose loss would change an implementation decision; loss of such an item refutes the row.
- R7b. Each knowledge row declares its extraction mode — full transcription, deliberate summary plus data snapshot, or distilled lesson — and the bar is applied to that mode: full rows checked for total coverage, summary rows checked that the snapshot preserves the data and the summary preserves every decision-relevant fact. A row with no declared mode is `unverifiable`.
- R8. A discard row is confirmed by a presence-and-coherence check: a reason for non-carry is recorded and internally coherent. The audit does not judge whether the discard decision was correct — that substance is out of scope (see Scope Boundaries).

**Execution model**

- R9. The audit runs as a fleet of fresh-context agents, one per ledger row, with no shared state between agents.
- R10. Each agent has read access to both repos for the row it audits.
- R11. A behavioral-row agent runs the matching repo verification (the relevant smoke/slice/metadata target) and captures its result — unless the row is production-only per R6a, in which case the agent records the production-only classification and returns `unverifiable` without running an inadmissible test.
- R12. Any captured evidence is credential-free — no tokens, secrets, app keys, or account numbers — consistent with the repo's existing evidence non-negotiables.

**Durable record and gate trust**

- R13. The audit produces a per-row durable record: classification, verdict, the bar applied, and an evidence pointer — captured while the old source is still readable. A confirmed row's evidence pointer must resolve entirely within this repository or as a self-contained inline record; it must not depend on a path in the old source that ceases to exist after decommission.
- R14. A confirmed row's record is re-checkable after decommission without the old source.
- R15. The audit reports overall gate state explicitly. The gate is *trustworthy-green* only when all 26 ledger rows (24 carried + 2 discard) have a recorded verdict reconciled against the ledger, every row is `confirmed` or `assumption-accepted`, and no row is an unresolved `extract`. A missing verdict for any row makes the gate not-green — a dropped row must never read as a pass.

---

## Key Flows

- F1. Single-row audit (per agent)
  - **Trigger:** The orchestrator assigns one ledger row to a fresh agent.
  - **Steps:** Classify the row (behavioral / knowledge / discard); apply the matching bar — run the test/evidence target (behavioral), build the claim-map against the declared extraction mode (knowledge), or check reason presence-and-coherence (discard); reach a verdict with an evidence pointer.
  - **Outcome:** A structured per-row result: classification, verdict, bar applied, evidence pointer.
  - **Covered by:** R2, R5, R6, R7, R7a, R7b, R8, R9, R10, R11, R12, R13

- F2. Fleet roll-up and gate verdict
  - **Trigger:** All per-row audits return.
  - **Steps:** Reconcile that all 26 rows returned a verdict; for each refuted row, re-disposition it (`extract`/`defer`/`discard`); for each `unverifiable` row, record the assumption and escalate it for maintainer acceptance (→ `assumption-accepted`) or leave it blocking; compute overall gate state.
  - **Outcome:** A frozen audit record and an explicit trustworthy-green / not-yet verdict, with any missing-verdict rows surfaced as not-green.
  - **Covered by:** R1, R3, R4, R4a, R9, R13, R14, R15

---

## Acceptance Examples

- AE1. Behavioral row confirmed
  - **Covers R6, R11, R13.**
  - **Given** a row claiming WebSocket lifecycle behavior is carried in the `S3_` path,
  - **When** the agent runs the matching WebSocket smoke/test and it passes,
  - **Then** the row is `confirmed` with a credential-free evidence pointer to that run.

- AE2. Behavioral row refuted (described but unproven)
  - **Covers R3, R6.**
  - **Given** a row whose carried target is a design doc but no test or evidence exercises the behavior in this repo,
  - **When** the agent finds no passing proof,
  - **Then** the row is `refuted` and re-dispositioned to `extract` (or `defer`/`discard`), re-blocking the gate.

- AE3. Knowledge row confirmed via completeness
  - **Covers R7, R13.**
  - **Given** a captured-lessons row (e.g., operator diagnostics) pointing to an extracted doc,
  - **When** the agent compares the old source against the extracted target and finds no material loss,
  - **Then** the row is `confirmed` with a pointer to the compared source and target.

- AE4. Knowledge row refuted (material loss found)
  - **Covers R3, R7.**
  - **Given** a knowledge row where the extracted target omits a constraint the old source carried,
  - **When** the agent identifies the missing material,
  - **Then** the row is `refuted`, the gap is named, and the row re-opens as `extract`.

- AE5. Unverifiable row
  - **Covers R4.**
  - **Given** a row whose proof would require a credential or environment not available during the audit,
  - **When** the agent cannot confirm or refute it,
  - **Then** the row is `unverifiable`, recorded as an explicit assumption with the blocking reason, and excluded from a green gate until accepted.

- AE6. Unverifiable row accepted
  - **Covers R4a, R15.**
  - **Given** a production-only behavioral row that is `unverifiable` per R6a,
  - **When** a named maintainer records an explicit acceptance and reason,
  - **Then** the row becomes `assumption-accepted` and counts toward a trustworthy-green gate.

---

## Scope Boundaries

- The physical decommission act (archive, tag, read-only, or delete the old repo) is out of scope — a separate follow-on once the gate is trustworthy-green.
- Re-litigating the *substance* of `discard` decisions is out of scope; the audit validates only that each discard reason is recorded and internally coherent (R8), not whether the decision was correct.
- Re-running the original extraction is out of scope; the audit verifies what was extracted, it does not redo it.
- Building a generalized, reusable multi-source decommission policy or skill is out of scope (a possible low-cost future byproduct, not this work).

---

## Dependencies / Assumptions

- The old source repo `korea-broker-sdk-ls` (sibling checkout) is readable for the full duration of the audit. The whole tiered approach depends on this.
- Behavioral-row proof relies on the repo's existing verification machinery (smoke targets, the metadata/slice validator, the evidence format) being runnable.
- Behavioral rows whose real behavior is production-only drop to `unverifiable` rather than forcing a false green (R6a), since production trading tests are prohibited.

---

## Outstanding Questions

**Deferred to planning**

- Where the durable record lives: a verdict/evidence column added to the existing ledger, a separate audit report, or per-row evidence files mirroring `metadata/evidence/`.
- Whether the fleet reuses the `promote-trs` orchestrator directly or a new audit recipe modeled on it.

---

## Deferred / Open Questions

### From 2026-06-18 review

- Reconcile the urgency framing with the staged boundary. Problem Frame calls this "the last moment" to compare against the source, but the physical decommission is a deferred follow-on, so the source does not vanish at gate-green. State how long the source stays readable (and that the audit must complete before the physical act), or drop the "last moment" framing. (product-lens)
- Behavioral evidence may prove less than the ledger asserts. The only WebSocket smoke is scoped to "lifecycle reachability only" and explicitly does not prove reconnect semantics, yet ledger row 43 claims reconnect replay, terminal exhaustion, and latest-only wakeup. Expect such rows to land `unverifiable` under R6/R6a rather than `confirmed`; confirm the trustworthy-green bar accounts for this. (feasibility)

---

## Sources / Research

- `docs/migration-source-extraction-ledger.md` — the gate, disposition states, and the 24 carried / 2 discard rows under audit.
- Extracted targets: `docs/design/ls-gateway-response-semantics.md`, `docs/design/order-safety-design.md`, `docs/design/release-readiness-and-residual-lessons.md`, `docs/design/websocket-certification-findings.md`, `docs/design/tr-dependency-inventory-snapshot.md`, `docs/operations/operator-diagnostics.md`, `docs/migration-source/tr-dependencies-2026-06-14.json`.
- ADR 0008 (no order runtime in first slice), ADR 0010 (old repo is migration source only), ADR 0012 (Rust-owned metadata schema).
- Precedent machinery: `.claude/agents/tr-promoter.md`, `.agents/skills/promote-trs/`, `.agents/skills/promote-tr/`, the `Makefile` smoke/docs/drift gates, `crates/ls-metadata/tests/slice_metadata.rs`, the `metadata/evidence/<tr>.yaml` format.
- `CONTEXT.md` — the **Decommissioned Migration Source** definition and the carried/evidence vocabulary.
- Grounding dossier: `/tmp/compound-engineering/ce-brainstorm/decomm-audit/grounding.md`.
