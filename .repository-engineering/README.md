# Repository Engineering Package

This directory is an inert repository declaration. It does not install, activate,
execute, publish, retire, or transfer authority. Current skills, Claude aliases,
workers, and ignored run-state consumers remain legacy-authoritative.

Human-authored inputs are `package.toml`, `discovery-policy.toml`,
`migration-ledger.toml`, and registered files below `contracts/`. Rust types are
the schema authority. The generated `schemas/v0/`, `schema-registry.json`,
`conformance/v0/`, `package.lock.json`, `generated-set.json`, and reference
documentation are projections; do not edit them by hand.

Declared contract registries are not active registries. A registered Capability
Contract or Worker Role Contract is discoverable authored data only. It grants
no executor, activation eligibility, installation, certification, parity,
successor authority, or retirement status. The active registries remain empty,
and lifecycle and authority statements come from validated typed state rather
than `public_description`.

## First semantic declaration

The first declared pair is `audit-carried-rows` with
`decommission-row-auditor`. It was selected because its 26-record manifest,
credential-free record format, committed roll-up report, and validator make its
legacy behavior locally inspectable while `held` and `unverifiable` remain
truthful outcomes. The `promote-trs` / `tr-promoter` pair is deliberately not in
this wave: its paper credentials, publication, and automatic-merge behavior
must be decomposed under separate authority and evidence contracts.

The two declarations are still `unported`, `uncertified`, legacy-authoritative,
and without executors or scenarios. Their ledger rows are only `planned` with
`parity_not_proven`. Evidence obligations and human gates describe future work;
they are not evidence. The committed report and record corpus are validated
legacy evidence only and cannot satisfy successor implementation, parity, or
certification.

### Mutable-ledger knowledge-reference policy

`docs/migration-source-extraction-ledger.md` is intentionally a `touched_path`,
not a digest-bound `knowledge_reference`. The still-authoritative legacy sweep
writes that ledger during serial roll-up, so digest-binding it as knowledge
would make ordinary legacy operation silently change semantic package identity.
The typed Migration Ledger remains a normative package input, and
`capability--audit-row` plus `run-state-consumer--audit-carried-rows` remain
explicit legacy-authority dependencies. Any future change to this policy needs
review; it must not be handled as an automatic digest re-pin.

### Field-to-source reconciliation

- Coordination, inputs, outcomes, and worker coupling come from the
  `audit-carried-rows` and `audit-row` recipes.
- Autonomy, gates, safety boundaries, touched paths, and unresolved legacy
  dependencies come from the sweep recipe, record format, and typed ledger rows.
- The external-source purpose comes from the legacy row-auditor role. Its source
  is unavailable in this declaration, so it has no locator or digest and maps to
  `held` or `unverifiable`, never success.
- Assignment, success-result fields, fresh context, concurrency, and result
  validation come from the legacy worker, row recipe, and record format.
- Credential scoping, terminal correlation, cancellation, idempotency, and inert
  successor state are explicit successor requirements, not observed parity.
- The report, manifest-defined record corpus, and committed validator establish
  legacy evidence status only. Successor evidence remains absent.

Host-specific names may appear in legacy-only paths, digest-bound references,
provenance, or the unavailable-source declaration. Operational successor fields
remain host-neutral; they define semantics without choosing a transport,
command, executor, or runtime-state protocol.

For an added, renamed, moved, or removed obligation:

1. Run `make repository-engineering-check` and inspect its stable candidate ID,
   locator, digest, and remediation class.
2. Author or update the ledger row and choose its disposition in review.
3. Run `cargo run --locked -q -p ls-repository-engineering -- generate`.
4. Run `make repository-engineering-check` again without writes.

Generation never creates or overwrites a Migration Ledger disposition.
The root workspace is pinned to Rust 1.96.0 by `rust-toolchain.toml`.
