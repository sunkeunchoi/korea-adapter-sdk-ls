# Maintained SDK Migration Plan

This plan re-architects the old `korea-broker-sdk-ls` generated-surface project into `korea-adapter-sdk-ls`, a Rust-first maintained SDK with complete upstream tracking and selective SDK implementation.

## Goal

Build a maintainable LS Securities Rust SDK where upstream API and documentation changes are tracked completely, but SDK code is updated intentionally through reviewed maintenance work items.

The new repository should obsolete the old generated-SDK architecture without preserving full generated API compatibility.

## Resolved Direction

- The **Maintained SDK Surface** is the source of truth for SDK behavior.
- Code ownership follows **Dependency Class**, not old generated categories like `stock` or `futures_options`.
- **Facet Metadata** routes tests, focused evidence, docs, and operator scheduling.
- Full LS TR inventory is tracked in metadata, but SDK implementation is selective.
- Trackers are advisory and produce severity-classified findings; they do not mutate SDK code directly.
- Ordinary maintenance uses **Change-Scoped Gates**, not a full multi-hour baseline.
- Old certification/generated-surface vocabulary is retired when it overlaps with the new concepts.
- The old repo is a **Migration Source** only, not a permanent dependency.

## Architecture

### Code Ownership

SDK code should be organized by dependency class:

```text
sdk/
  core/                 # auth, config, client, errors, rate buckets, transport
  standalone/           # OAuth-token-only TR behavior
  paginated/            # SELF / cts_* continuation flows
  account/              # account-state-dependent inquiries
  market_session/       # market/session/date-sensitive reads
  orders/               # guarded order TRs, order-number coupling, reconciliation
  realtime/             # WebSocket connect/auth/subscribe lifecycle
  paper_incompatible/   # documented production-only/simulation-unavailable surfaces
```

Old LS categories become metadata tags:

- domestic stock
- futures/options
- overseas stock
- overseas futures
- sector/index
- realtime invest
- misc

### Metadata

Track the full LS TR inventory in per-TR metadata files, with an index for fast routing:

```text
metadata/
  tr-index.yaml
  trs/
    t8412.yaml
    CSPAT00701.yaml
    SC1.yaml
```

`metadata/tr-index.yaml` duplicates only routing fields:

```yaml
version: 1
trs:
  t8412:
    file: trs/t8412.yaml
    owner_class: paginated
    protocol: rest
    instrument_domain: stock
    venue_session: krx_regular
```

Each per-TR file is the full source for maintenance metadata:

```yaml
tr_code: t8412
name: 주식차트(N분)
owner_class: paginated
facets:
  protocol: rest
  instrument_domain: stock
  venue_session: krx_regular
  date_sensitive: true
  self_paginated: true
  account_state: false
  paper_incompatible: false
  certification_path: automated
  rate_bucket: market_data
  caller_supplied_identifiers: [shcode]
dependencies:
  self_continuation_fields: [cts_date, cts_time]
  strong_order_fields: []
support:
  tracked: true
  implemented: false
  recommended: false
maintenance:
  source_spec_hash: 238beb842b1a
  last_reviewed: 2026-06-14
```

Rules:

- Every tracked TR has one per-TR metadata file.
- Every tracked TR has exactly one `owner_class`.
- Facets are multi-valued and explicit enough for debugging.
- The index is validated against per-TR files.
- Bootstrap may generate initial metadata once; after that metadata is maintained project data.

## Change Tracking

The project keeps two trackers:

- **API Drift Tracker**: detects TR additions, removals, request/response shape changes, rate metadata changes, and endpoint/protocol changes.
- **Specification Document Tracker**: detects upstream documentation changes that could affect SDK behavior, examples, operations, or metadata.

Tracker workflow:

1. `fetch`: capture upstream LS API/docs into timestamped staged snapshots.
2. `normalize`: convert raw snapshots into canonical normalized artifacts.
3. `diff`: compare normalized artifacts against reviewed baselines.
4. `classify`: emit support-aware tracker findings.
5. `promote`: after review, update baselines and affected metadata/docs.

Trackers do not write Rust SDK changes directly. A tracker finding may be promoted into an **SDK Maintenance Work Item**.

### Finding Severity

Tracker severity is support-aware:

- `critical`: auth, security, order-safety, or tracker-data corruption.
- `breaking`: implemented/recommended behavior is removed or changed incompatibly.
- `maintenance`: new TR, optional field, metadata facet change, or docs change requiring review.
- `evidence`: behavior unchanged but focused evidence is stale or needs refresh.
- `informational`: hash/timestamp noise or changes affecting only unimplemented tracked TRs.

## Documentation

Split upstream tracking from SDK docs:

- **Specification Document Tracker** produces maintainer-facing upstream diffs and findings.
- **SDK Reference Docs** are user-facing and generated from maintained SDK behavior, metadata, and verified examples.
- **TR Dependency Docs** are maintainer/operator-facing and generated from maintained metadata, not raw tracker output.

Do not mirror upstream LS docs directly as product docs.

## Testing And Evidence

Default verification is the **Change-Scoped Gate** selected from metadata.

Examples:

- Always run formatting/build/lint for touched crates and unit tests for touched dependency classes.
- TR shape changes run request/response serde tests for the affected TR.
- `self_paginated` runs pagination tests for that TR plus shared pagination tests.
- `date_sensitive` runs date/default handling tests.
- `account_state` runs credential-free request construction tests; credentialed evidence is scheduled separately.
- `protocol: websocket` runs subscribe/reconnect/frame tests and row evidence when scheduled.
- `owner_class: orders` runs no-retry, dedup, and reconciliation tests; guarded manual evidence requires operator confirmation.
- `recommended: true` requires focused evidence freshness sufficient for user-facing claims.

**Focused Evidence** replaces broad old TR certification vocabulary. It proves implemented or recommended behavior only. It is not required for every tracked LS TR.

**Full Baseline** is reserved for release or periodic confidence. It is not the ordinary maintenance gate.

## First Implementation Tier

Build a representative vertical slice before expanding TR coverage:

- `core`: auth, config, client, REST transport, errors, rate buckets.
- Metadata foundation: `metadata/tr-index.yaml`, representative per-TR files, schema validator.
- `standalone`: `token`, `revoke`, and a narrow OAuth-only representative if one is truly standalone.
- `market_session`: one quote flow such as `t1101` or `t1102`.
- `paginated`: one continuation flow such as `t8412` or `t0424`.
- `account`: one practical account inquiry such as `CSPAQ12200` or `t0424`.
- `realtime`: one quote subscription such as `S3_`.
- `orders`: metadata and safety/reconciliation design only at first.

Order runtime is deferred until no-retry dispatch, deduplication, reconciliation, and guarded focused evidence can ship together.

## Migration From Old Repo

Use `/Users/mini/dev/korea-broker-sdk-ls` as a migration source only.

Useful source artifacts:

- `docs/TR_DEPENDENCY_GUIDE.md`
- `docs/TR_DEPENDENCY_REFERENCE.md`
- old runtime code for auth/client/rate/error/transport patterns
- old WebSocket runtime lessons
- old order-safety and reconciliation docs
- old spec JSON for metadata bootstrap

Do not preserve:

- full generated Rust tree as the new source of truth
- old Python generation pipeline as permanent architecture
- old certification/release gates as default maintenance workflow
- compatibility with existing old SDK users as a migration constraint

Permanent tooling should be Rust-first where practical. Python is acceptable for one-time bootstrap scripts.

## Initial Work Plan

1. Create Rust workspace skeleton and core crate layout.
2. Define metadata schema and validator.
3. Bootstrap 5-10 representative per-TR metadata files and `tr-index.yaml`.
4. Implement change-scoped test planner over metadata.
5. Port or rewrite core auth/config/client/error/rate behavior.
6. Implement first representative REST TR.
7. Add focused unit and serde tests for that TR.
8. Implement one paginated TR and shared continuation helper.
9. Implement one WebSocket subscription path.
10. Generate initial SDK Reference Docs and TR Dependency Docs from metadata.
11. Add staged-snapshot tracker skeleton with no SDK mutation.
12. Write obsolete notice for old repo after the vertical slice is working.

## Open Questions

- Which exact representative TRs should be selected for the first slice?
- What crate names should the Rust workspace use?
- Which metadata schema format should be validated first: YAML with JSON Schema, Rust-owned schema, or both?
- What focused evidence freshness window is required for a `Recommended TR`?
- What is the minimum obsolete notice needed in the old repository?
