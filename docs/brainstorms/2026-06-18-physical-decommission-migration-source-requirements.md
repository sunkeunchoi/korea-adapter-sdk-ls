---
date: 2026-06-18
topic: physical-decommission-migration-source
---

# Physical Decommission of the Migration Source — Requirements

## Summary

Declare `korea-broker-sdk-ls` a **Decommissioned Migration Source** inside this repo: add an anchor marker at `docs/migration-source/README.md`, record the decommission in a new ADR, rewrite the one active doc that still calls the old repo a live reference, and add a regression guard that keeps it decommissioned. No new extraction; the audit evidence stays in place.

## Problem Frame

The decommission audit of `docs/migration-source-extraction-ledger.md` reached **TRUSTWORTHY-GREEN** (26/26 rows confirmed, 0 acceptances), and its committed validator `crates/ls-trackers/tests/decommission_audit.rs` recomputes that gate from frozen records so it stays defensible in CI after the source is gone. The audit report (`docs/migration-source/audit/decommission-audit-report.md`) names this physical-decommission step as the out-of-scope follow-up that must *cite* that verdict.

What is missing is the in-repo declaration that the precondition has been met. The repo still carries present-tense language treating the old repo as something maintainers reference. Until that is corrected and guarded, nothing prevents a future change from quietly reintroducing a dependency on a source that is supposed to be gone.

This PR cannot delete the sibling repo — that is an external act. It does the in-repo half: mark the relationship decommissioned, fix the stale pointer, and lock the boundary with a test.

## Key Decisions

- **New ADR `0014`, leave `0010` untouched.** Add `docs/adr/0014-migration-source-decommissioned.md` recording the decommission. ADR `0010` stays as the historical decision; `0014` supersedes only its operational posture, not the record. (`0011`–`0013` already exist, so `0014` is the next free number.)

- **The guard distinguishes retained evidence from live dependency.** Retained `Provenance: korea-broker-sdk-ls/...` citations are audit evidence and are allowed; a live dependency (a `../korea-broker-sdk-ls` filesystem path in non-test source, or an instruction to consult the old source for ordinary maintenance) is forbidden. A blunt "any mention fails" check would false-positive on the evidence the PR deliberately keeps.

- **`CONTEXT.md` is not edited.** Its `Decommissioned Migration Source` term and relationship already state that maintainers should not need to consult the old source for ordinary maintenance or expansion. The concept is current; re-touching it would be churn.

- **`README.md` is the only active doc rewritten.** The "Standalone — and the role of `korea-broker-sdk-ls`" section is the one place that still describes the old repo in present tense as "a repository we reference to pull… from." The `Provenance:` citations in `docs/design/` and `docs/operations/` are historical attribution and stay as-is.

## Requirements

**Anchor marker**

- R1. `docs/migration-source/README.md` exists and states that `korea-broker-sdk-ls` is a Decommissioned Migration Source: retained provenance and audit evidence may cite it, but ordinary maintenance must not consult, import, build against, or otherwise depend on it.
- R2. The marker points to the retained evidence under `docs/migration-source/audit/` and names the TRUSTWORTHY-GREEN audit gate as the precondition that authorized the decommission.

**Decision record**

- R3. `docs/adr/0014-migration-source-decommissioned.md` exists, status accepted 2026-06-18, recording that the old repo is now a Decommissioned Migration Source and that ordinary maintenance must not consult, import, build against, or depend on it.
- R4. ADR `0014` supersedes the operational posture of ADR `0010` without editing `0010`; it references `docs/migration-source/README.md` as the anchor and `docs/migration-source/audit/` as retained evidence.

**Active-doc correction**

- R5. The `README.md` "role of `korea-broker-sdk-ls`" section is rewritten from present-tense reference framing to the decommissioned posture, consistent with the marker and ADR.
- R6. No `Provenance:` citation in `docs/design/` or `docs/operations/`, and no content in `docs/plans/` or `docs/brainstorms/`, is altered.

**Regression guard**

- R7. A test in `crates/ls-trackers/tests/decommission_audit.rs` asserts the anchor marker `docs/migration-source/README.md` exists.
- R8. The same guard asserts no active non-test source file or active doc carries a live old-source dependency (a `../korea-broker-sdk-ls` filesystem path or an instruction to consult the old source for ordinary maintenance).
- R9. The guard's scan excludes retained evidence and history so it does not self-trip: `docs/plans/`, `docs/brainstorms/`, the audit records and manifest under `docs/migration-source/audit/`, the retained `tr-dependencies-2026-06-14.json`, `Provenance:` citation lines, and test files.

**Verification**

- R10. `cargo test -p ls-trackers` passes, including the existing gate validator and the new guard.
- R11. `cargo test --workspace` passes.

## Acceptance Examples

- AE1. **Covers R7, R8.** A non-test source file under `crates/` adds `let p = "../korea-broker-sdk-ls/...";` → the guard fails. The path is removed → the guard passes.
- AE2. **Covers R8, R9.** A `docs/design/` doc keeps `(Provenance: korea-broker-sdk-ls/docs/DIAGNOSTICS_CONTRACT.md)` → the guard passes; that line is retained evidence, not a live dependency.
- AE3. **Covers R9.** The audit manifest under `docs/migration-source/audit/` names old-source documents in its claim maps → the guard passes; the audit tree is excluded from the scan.
- AE4. **Covers R7.** `docs/migration-source/README.md` is deleted → the guard fails.

## Scope Boundaries

- Physically deleting or archiving the sibling `korea-broker-sdk-ls` repository — an external ops act, outside this repo's PR.
- Any new extraction from the old source. The audit is TRUSTWORTHY-GREEN; the only re-extraction was the audit's own re-extract step, already merged.
- Removing retained evidence: `docs/migration-source/tr-dependencies-2026-06-14.json`, the audit records/manifest/report under `docs/migration-source/audit/`, and `Provenance:` citations in design/operations docs.
- Editing `CONTEXT.md` or ADR `0010`.
- Changing what the existing gate validator recomputes — the new guard is additive, not a rewrite of the record-consistency logic.

## Dependencies / Assumptions

- The decommission precondition is already satisfied: the audit gate is TRUSTWORTHY-GREEN and recomputed in CI by the existing validator. This PR cites that verdict; it does not re-run the audit.
- Assumption: `README.md` lines 50-63 are the only active-doc occurrence of present-tense "we reference the old source" framing. A repo-wide check at implementation time confirms no other active doc (outside `docs/plans/`, `docs/brainstorms/`, and `Provenance:` lines) carries the same framing; if one is found, it falls under R5's intent.

## Sources

- `docs/migration-source/audit/decommission-audit-report.md` — TRUSTWORTHY-GREEN gate; names physical decommission as the citing follow-up.
- `crates/ls-trackers/tests/decommission_audit.rs` — existing gate validator the guard extends (record-consistency, not source re-comparison).
- `CONTEXT.md:147-149,203` — `Decommissioned Migration Source` term and relationship, already current.
- `docs/adr/0010-old-repository-is-migration-source-only.md` — the superseded operational posture.
- `README.md:50-63` — the active doc to rewrite.
