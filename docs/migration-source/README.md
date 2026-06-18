# Migration Source — Decommissioned

`korea-broker-sdk-ls` is a **Decommissioned Migration Source**.

In the project's vocabulary (see [`CONTEXT.md`](../../CONTEXT.md)), this is
stronger than a read-only or obsolete source: its gateway, TR, runtime, and
operational knowledge has been extracted into the maintained SDK surface — or
deliberately rejected with a recorded reason — so the maintained SDK no longer
needs it even as read-only reference material.

## What this means

- **Retained provenance and audit evidence may cite the old repo.** Historical
  `Provenance:` citations in `docs/design/` and `docs/operations/`, the frozen
  extraction ledger, and the audit tree below remain as attribution.
- **Ordinary maintenance must not depend on it.** Do not consult, import, build
  against, or otherwise depend on the old repo for ordinary SDK maintenance or
  expansion. New SDK behavior belongs here, in the maintained surface — not in a
  source that is being decommissioned.

This repository is standalone: it builds, tests, and ships on its own, with no
build or runtime dependency on any other SDK repository.

## Retained evidence

The decommission was authorized by a completed audit of the extraction ledger.
The retained evidence lives under [`audit/`](audit/):

- [`audit/decommission-audit-report.md`](audit/decommission-audit-report.md) —
  the roll-up report.
- [`audit/manifest.yaml`](audit/manifest.yaml) — the audited row manifest.
- [`audit/records/`](audit/records/) — the frozen per-row verdicts, `L1`–`L26`.

## Authorizing precondition

The audit reached **TRUSTWORTHY-GREEN** — all 26 rows confirmed and reconciled
against the ledger, with zero maintainer acceptances. The committed validator
`crates/ls-trackers/tests/decommission_audit.rs` recomputes that gate from the
frozen records in CI, so the verdict stays defensible after the old repo is gone.
That green gate is the documented precondition that authorized this decommission;
ADR [`0014`](../adr/0014-migration-source-decommissioned.md) records the decision.
