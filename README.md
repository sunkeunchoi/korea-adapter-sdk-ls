# korea-adapter-sdk-ls

A **maintained Rust SDK** for the LS Securities Open API. The SDK is built by
tracking upstream API and documentation change and applying *reviewed* SDK
changes — not by regenerating a full surface from specs.

## Start here

New to the codebase? These maintainer/contributor docs orient you:

- [`USER_GUIDE.md`](USER_GUIDE.md) — build, the gate, and how to work on a TR.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — the workspace, the dispatch runtime, and
  how metadata projects into docs.
- [`TR_LIFECYCLE.md`](TR_LIFECYCLE.md) — how a TR climbs Raw → Tracked →
  Implemented → Recommended, and the gate at each rung.
- [`AGENTS.md`](AGENTS.md) — the working agreement for agents in this repo (gate,
  layout, gotchas).
- [`CONCEPTS.md`](CONCEPTS.md) — the authoritative domain glossary.

## What this is (and is not)

This SDK surface is **selective by design**. It does not aim to expose every
upstream transaction request (TR); it implements the behavior maintainers have
chosen to own, and recommends only what has current evidence behind it.

- **Implemented ≠ Recommended.** A TR can be wired and tested without being
  recommended for use. Promotion to **Recommended** requires current
  **Focused Evidence**.
- **Six TRs are currently Recommended** (`token`, `t1101`, `t1102`, `t8412`,
  `S3_`, `CSPAQ12200`) — each backed by a credential-free paper live-smoke
  record. See the generated [`docs/reference/`](docs/reference/) pages for each
  TR's full contract (what it claims, what evidence backs it, and what it does
  not claim). A 90-day evidence-freshness backstop (`make freshness-check`) flags
  any Recommended TR whose evidence has gone stale.
- **Order runtime is deferred by design.** The order safety design is written
  and one order TR is tracked, but no order placement ships today; it will move
  only as a complete safety package.

The authoritative source of truth for SDK behavior is the maintained Rust code
and the per-TR metadata under [`metadata/`](metadata/) — not this README. For
live tracked / implemented / recommended counts, read the generated
[`docs/reference/`](docs/reference/) and
[`docs/tr-dependencies/`](docs/tr-dependencies/) pages, which are projected from
metadata by `make docs`.

## Change tracking

Two **advisory** change trackers watch upstream and surface findings for human
review:

- **API Drift Tracker** — upstream API *shape* changes (TR additions/removals,
  request/response field changes).
- **Specification Document Tracker** — documentation *example* drift.

Both are **advisory**: they emit severity-classified findings and **never mutate
SDK code, metadata, docs, evidence, or baselines** on their own. Tracker runs are
operator-invoked (`make api-drift-check`, `make spec-doc-check`); see the
[maintenance runbook](docs/MAINTENANCE_RUNBOOK.md).

## Vocabulary

The project's authoritative terms (Maintained SDK Surface, Recommended TR,
Focused Evidence, Migration Source, …) are defined in [`CONTEXT.md`](CONTEXT.md).

## Standalone — and the role of `korea-broker-sdk-ls`

This repository is **standalone**: it builds, tests, and ships on its own, with
no build or runtime dependency on any other SDK repository. Every crate
dependency is internal to this workspace.

`korea-broker-sdk-ls` is a **Decommissioned Migration Source**. Its gateway, TR,
runtime, and operational knowledge has already been extracted into this
maintained surface — or deliberately rejected with a recorded reason — so
ordinary maintenance no longer needs the old repo at all. The old repo is **not
a dependency** and not the maintained SDK; this SDK does not import, link, or
build against it, and new SDK behavior belongs here, in the maintained surface.

What is retained is **evidence, not a live dependency**: historical
`Provenance:` citations, the frozen extraction ledger, and the audit tree may
still cite the old repo as attribution. The decommission was authorized by a
TRUSTWORTHY-GREEN audit of the extraction ledger; see
[`docs/migration-source/README.md`](docs/migration-source/README.md) and ADR
[`0014`](docs/adr/0014-migration-source-decommissioned.md).
