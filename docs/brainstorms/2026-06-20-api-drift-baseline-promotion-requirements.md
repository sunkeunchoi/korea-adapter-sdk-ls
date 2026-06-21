---
date: 2026-06-20
topic: api-drift-baseline-promotion
---

# API Drift Baseline Promotion

## Summary

Add a mutating `api-drift promote` command that performs a reviewed **Baseline Promotion**: it replaces the committed API Drift raw **Reviewed Baseline** with a reviewed **Staged Snapshot**'s raw, re-derives the committed normalized baselines from that raw, and appends one structured record to an append-only promotion log. Promotion refuses on gated **Tracker Findings** unless the operator explicitly attests. This closes the refresh lifecycle that today has no maintained command, and leaves a machine-readable trace that spec-doc automation can later watch.

## Problem Frame

The API Drift refresh lifecycle has a gap. `api-drift fetch` stages a raw snapshot plus normalized shapes under `target/ls-trackers/api-drift/runs/{timestamp}/` and updates a `latest.txt` pointer. `api-drift renormalize` rereads the *already-committed* raw and rewrites normalized baselines from it. But `api-drift promote` is parse-blocked to `--dry-run` — it prints what a promotion would touch and writes nothing, with mutating promotion explicitly declared out of scope.

So there is no maintained command that moves a reviewed staged raw snapshot into the committed baseline at `crates/ls-trackers/baselines/api-drift/raw/`. An operator who has reviewed a staged run cannot advance the comparison point without hand-editing committed files. Until that step exists, the **Reviewed Baseline** can only ever be re-derived, never moved forward — and there is no durable record of when, from what, and on whose review a promotion happened.

## Key Decisions

- **Gate-and-attest, not free-write.** Promotion runs the drift check first. A clean diff promotes directly; a diff carrying gated Tracker Findings (breaking **Structural API Shape** changes, new TRs) refuses until the operator attests. This reuses the existing **Support-Aware Severity** gating rather than inventing a new review surface.
- **Promote owns the baseline, not SDK maintenance.** A single promote writes the committed raw and re-derives the committed normalized baselines — nothing else. TR maintenance metadata, evidence dates, recommendation state, and generated docs stay separate, explicit steps selected by a work item. This honors the CONTEXT.md rule that a Baseline Promotion is a distinct review act from ordinary SDK maintenance.
- **Raw is the source of truth, promoted whole.** When a promote proceeds, the committed raw is replaced wholesale by the staged raw — not cherry-picked per TR. Normalized baselines are then re-derived from that raw. Re-derivation covers maintained codes only and preserves the code-set `provisional` flag.
- **Code-set membership stays a separate review.** New upstream TRs appear in the promoted raw but are not admitted into the maintained set. Admission to `code-set.json` remains a separate manual review act.
- **Machine log over prose trail.** Each promotion appends one structured record to a committed append-only log so automation can watch a precise path and reviewers get a compact diff. The git history and PR carry the narrative; `SEED-ATTESTATION.md` remains the one-time seed story.
- **Attestation is the explicit go-ahead; there is no bare mutation.** `promote --dry-run` previews; `promote --attest <operator-or-issue>` is the only path that writes. Invoking promote with neither flag is a usage error, never an accidental write. The term `--commit` is avoided because this command writes files, not git commits. Attesting acknowledges any gated findings; the accepted findings are recorded in the promotion log automatically, derived from the drift report, so the audit trail is complete without a separate naming flag.

## Requirements

**Promotion mechanics**

- R1. `api-drift promote`, when it proceeds, replaces the committed API Drift raw baseline with the selected staged run's raw and re-derives the committed normalized baselines (manifest + per-TR shapes) from that raw.
- R2. Promote selects the most recent staged run by default (via the `latest.txt` pointer) and accepts an explicit staged-run path (`--staged <run>`). It pins that exact run — resolving the path first and running the drift check against it, never a live fetch — and verifies the staged raw against the gated hash immediately before writing, so the promoted bytes match what the gate evaluated.
- R3. `api-drift promote --dry-run` is the non-mutating preview: it reports what a promotion would change — raw hash, normalized shape changes, code-set changes, and gated findings — and writes nothing. Dry-run also applies the completeness / facts-outage gate and reports any gate failure before reporting shape changes, consistent with `fetch`.
- R4. Mutation requires `--attest <operator-or-issue>`. Invoking promote with neither `--dry-run` nor `--attest` is a usage error and writes nothing.
- R5. Promotion uses derive-then-write: the re-derivation is computed and validated from the staged raw before any committed file is written, so a re-derivation failure aborts with zero mutation. The committed baseline — raw plus re-derived normalized shapes and prunes (R10) — is committed together; the promotion-log append (R14) is the final step, ordered after the baseline is durable. A baseline advanced without its matching log record is a detectable, re-appendable inconsistency, not a silent loss. The exact commit mechanism (e.g. staging and swap) is a planning decision.

**Review gate and attestation**

- R6. Before mutating, promote runs the drift check and reports any gated Tracker Findings. Mutation requires `--attest` (R4), which acknowledges any gated findings; without `--attest`, promote refuses.
- R7. Attestation is `--attest <operator-or-issue>`: it is the mutation go-ahead, carries the attested-by value, and triggers the promotion-log write (R14). It does not bypass the derive-then-write rule (R5).
- R8. Attesting acknowledges any gated Tracker Findings; the accepted findings are recorded in the promotion log automatically (R14), derived from the drift report — the operator does not name them with a separate flag.
- R9. Raw is promoted as a whole snapshot when a promote proceeds — the reviewed staged raw becomes the new comparison point in full, not as a per-TR subset.

**Re-derivation boundary**

- R10. Re-derivation produces normalized baselines for maintained codes only, preserves the code-set `provisional` flag, and removes committed per-TR shapes whose code is no longer maintained (pruning) — these removals are committed together with the rest of the baseline (R5).
- R11. Promote does not admit new TRs into the maintained set. New TRs present in the promoted raw remain unmaintained until a separate `code-set.json` review.
- R12. Promote does not modify TR maintenance metadata, evidence dates, recommendation state, or generated docs.
- R13. Promote does not clear or suppress freshness or evidence staleness introduced by re-derivation. Such staleness is left for the freshness evaluator to surface afterward.

**Attestation record**

- R14. Each successful promote appends exactly one structured record to a committed append-only promotion log under `crates/ls-trackers/baselines/api-drift/`. Each record carries: promotion timestamp, source staged-run identity, raw snapshot hash, attested-by value, the gated findings accepted (if any), and the affected TR codes, plus an optional free-form note.
- R15. `SEED-ATTESTATION.md` is unchanged by promote — it remains the one-time bootstrap seed narrative, not a per-promotion log.

## Key Flows

- F1. Clean promote
  - **Trigger:** Operator runs promote against a staged run whose drift check has no gated findings.
  - **Steps:** Select staged run (latest or `--staged`) → run drift check → check is clean → operator attests (`--attest`) → replace committed raw → re-derive normalized baselines → append one promotion record.
  - **Outcome:** Committed baseline advanced; promotion recorded.
  - **Covers:** R1, R2, R4, R5, R6, R7, R10, R14.

- F2. Gated promote with attestation
  - **Trigger:** Drift check returns gated findings and the operator attests with `--attest`.
  - **Steps:** Drift check gates → operator attests (`--attest`) → promote proceeds → record captures the auto-derived accepted findings and attested-by value.
  - **Outcome:** Baseline advanced over a reviewed breaking change; the accepted findings are durably recorded.
  - **Covers:** R4, R5, R6, R7, R8, R14.

- F3. Gated promote without attestation
  - **Trigger:** Drift check returns gated findings and no attestation is given.
  - **Steps:** Drift check gates → promote refuses → nothing is written.
  - **Outcome:** Committed baseline unchanged; operator must review and attest, or resolve the findings.
  - **Covers:** R5, R6.

- F4. Dry-run preview
  - **Trigger:** Operator runs promote with `--dry-run`.
  - **Steps:** Run the check, report raw hash, normalized and code-set changes, and gated findings, write nothing.
  - **Outcome:** Operator sees the scope of a would-be promotion without mutating the baseline.
  - **Covers:** R3.

## Acceptance Examples

- AE1. **Covers R4, R5, R7, R14.** Given a staged run with a clean drift check, when promote runs with `--attest`, then the committed raw is replaced, the normalized baselines are re-derived, and exactly one promotion record is appended.
- AE2. **Covers R5, R6.** Given a staged run whose check gates on a breaking shape change and no attestation, when promote runs, then it refuses and neither raw nor normalized baselines are written.
- AE3. **Covers R5, R6, R7, R8, R14.** Given the same gated run with `--attest`, when promote runs, then it proceeds and the record's accepted-findings field — auto-derived from the drift report — names the breaking change alongside the attested-by value.
- AE4. **Covers R9, R11.** Given a staged raw that contains a new upstream TR, when promote proceeds, then the new TR is present in the committed raw but is not added to the maintained set, and it continues to surface as a finding on the next check until separately admitted.
- AE5. **Covers R12, R13.** Given a re-derivation that changes the Structural API Shape of a **Recommended TR**, when promote proceeds, then no TR metadata, evidence date, or recommendation state is touched, and the resulting staleness is left for the freshness evaluator to flag.
- AE6. **Covers R4.** Given promote invoked with neither `--dry-run` nor `--attest`, when it runs, then it exits with a usage error and writes nothing.

## Scope Boundaries

**Deferred for later**
- Orchestrating the downstream SDK-maintenance steps (metadata stamping, evidence/recommendation updates, doc regeneration) that a promotion's derived changes may warrant.
- Automating new-TR admission into `code-set.json` as part of, or triggered by, promotion.

**Outside this product's identity**
- Promotion of the Specification Document Tracker baseline. This command promotes the API Drift baseline only, even though a symmetric spec-doc command surface exists.
- Any SDK maintenance effect — promote advances the comparison point; it does not change the Maintained SDK Surface or its supporting metadata and evidence.

## Dependencies / Assumptions

- Builds on existing capabilities: `fetch` staging, the `run_check` drift gate (R6), and the `renormalize`-style re-derivation that already preserves the `provisional` flag and prunes stale shapes (R1, R10).
- R14's raw snapshot hash is a new whole-raw digest computed at promote time (a capability to build) — it is not the existing per-TR `maintenance.source_spec_hash`, which is hand-authored and left untouched by R12 (so those per-TR values may go stale).
- `latest.txt` is currently written but not read by any command; promote's run-selection step must add the reader, so the default selector (R2) is new work, not a reused affordance. An explicit `--staged` path overrides it.

## Outstanding Questions

**Deferred to planning**
- Exact serialization and append semantics of the promotion log records. R14 specifies the field set; only the format is open.
- Whether the attested-by value is free-form or validated against a known operator identity.

## Sources / Research

- `crates/ls-trackers/src/cli.rs` — current `promote` parse block (rejects mutating promote), `PromoteDryRun` dispatch (writes nothing), `renormalize_committed` (re-derive from committed raw, preserve `provisional`), `fetch` staging path and `latest.txt` update.
- `crates/ls-trackers/src/stages.rs` — `promote_targets`, which enumerates the normalized shapes, TR metadata fields, and generated docs a promotion would conceptually touch; this brief deliberately narrows promote's writes to the raw + normalized subset.
- `crates/ls-trackers/baselines/api-drift/` — committed baseline layout (`raw/`, `code-set.json`, `normalized/manifest.json`, `normalized/trs/`) and `SEED-ATTESTATION.md` (seed narrative).
- `crates/ls-trackers/tests/api_drift.rs` — existing gate behavior (self-diff does not gate; field removal gates; new-TR gate) that R6 reuses.
- `CONTEXT.md` — vocabulary for **Baseline Promotion**, **Reviewed Baseline**, **Staged Snapshot**, **Structural API Shape**, and the rule that Baseline Promotion is a separate review act from ordinary SDK maintenance.
