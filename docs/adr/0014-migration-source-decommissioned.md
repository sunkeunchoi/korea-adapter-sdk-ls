# Migration source is decommissioned

**Status:** Accepted 2026-06-18. Supersedes the operational posture of ADR
[`0010`](0010-old-repository-is-migration-source-only.md) (which remains the
historical record, unedited).

`korea-broker-sdk-ls` is now a **Decommissioned Migration Source**. Its gateway,
TR, runtime, and operational knowledge has been extracted into this maintained
SDK surface, or deliberately rejected with a recorded reason, so ordinary
maintenance no longer needs it. Concretely: for ordinary SDK maintenance or
expansion the project must not consult, import, build against, or otherwise
depend on the old repo. Retained provenance and audit evidence — historical
`Provenance:` citations, the frozen extraction ledger, and the audit tree — may
still cite it as attribution; that is evidence, not a live dependency.

ADR `0010` declared the old repo a *migration source only* and is left
unmodified as the decision of record. This ADR supersedes only its operational
posture: where `0010` still framed the old repo as something maintainers would
reference during migration, the relationship is now closed. We chose a new ADR
over editing `0010` so the historical decision stays legible and the supersession
is explicit.

The decommission was authorized by the decommission audit of the extraction
ledger, which reached **TRUSTWORTHY-GREEN** (all 26 rows confirmed, reconciled
against the ledger, zero maintainer acceptances). The committed validator
`crates/ls-trackers/tests/decommission_audit.rs` recomputes that gate from the
frozen records, so the precondition stays defensible in CI after the old repo is
gone. The anchor for this posture is
[`docs/migration-source/README.md`](../migration-source/README.md); the retained
evidence lives under [`docs/migration-source/audit/`](../migration-source/audit/).

Physically deleting or archiving the sibling repository is an external operations
act and is out of scope for this repo.
