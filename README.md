# korea-adapter-sdk-ls

This repository holds **two projects**:

- the **SDK Project** — a maintained Rust SDK for the LS Securities Open API,
  built by tracking upstream API and documentation change and applying
  *reviewed* SDK changes, not by regenerating a surface from specs; and
- the **Consuming Project** — a Korean-equities algorithmic trading system built
  on that SDK, in the standalone `adapters/nautilus/` workspace.

They were once two repositories. Both were unfinished, and advancing two
unfinished things across a repository boundary collapsed the effort, so they now
live together with the boundary drawn at a Cargo workspace instead —
[ADR 0015](docs/adr/0015-two-projects-one-repository.md) records why, and
[`CONTEXT-MAP.md`](CONTEXT-MAP.md) maps the two contexts and the words that mean
different things on either side.

## Start here

- [`CONTEXT-MAP.md`](CONTEXT-MAP.md) — the two contexts and the boundary between
  them. Read this first; it decides which of the docs below apply to you.
- [`CONCEPTS.md`](CONCEPTS.md) — **the** domain glossary, authoritative for both
  contexts.
- [`USER_GUIDE.md`](USER_GUIDE.md) — build, the gate, and how to work on a TR.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — the workspaces, the dispatch runtime,
  and how metadata projects into docs.
- [`TR_LIFECYCLE.md`](TR_LIFECYCLE.md) — how a TR climbs Raw → Tracked →
  Implemented → Recommended, and the gate at each rung.
- [`AGENTS.md`](AGENTS.md) — the working agreement for agents (gate, layout,
  gotchas, and the work queue).

## The SDK Project

The SDK surface is **selective by design**. It tracks the upstream transaction
request (TR) universe *completely* — so drift is observed everywhere — but
implements and recommends only what the Consuming Project needs.

- **Implemented ≠ Recommended.** A TR can be wired and smoke-tested without
  being recommended. Promotion to **Recommended** requires current **Focused
  Evidence** and passing the error-resilience gate.
- **Order runtime ships.** It was deferred until the safety package was complete
  ([ADR 0008](docs/adr/0008-defer-order-runtime-until-safety-package-is-complete.md));
  that condition has since been met, and the domestic cash-equity and
  futures/options order chains are implemented behind no-retry dispatch, a
  deduplicator, a kill switch, and reconciliation.
- **A tracked-but-never-implemented TR is the design working**, not a gap.

Live tracked / implemented / recommended counts are projected from metadata by
`make docs` — read [`docs/reference/`](docs/reference/) and
[`docs/tr-dependencies/`](docs/tr-dependencies/) rather than any prose. A 90-day
evidence-freshness backstop (`make freshness-check`) flags any Recommended TR
whose evidence has gone stale.

The authoritative source of truth for SDK behavior is the maintained Rust code
and the per-TR metadata under [`metadata/`](metadata/) — not this README.

### Change tracking

Two **advisory** change trackers watch upstream and surface findings for human
review: the **API Drift Tracker** (upstream API *shape* changes) and the
**Specification Document Tracker** (documentation *example* drift). Both are
advisory — they emit severity-classified findings and **never mutate SDK code,
metadata, docs, evidence, or baselines** on their own. Runs are operator-invoked
(`make api-drift-check`, `make spec-doc-check`); see the
[maintenance runbook](docs/MAINTENANCE_RUNBOOK.md).

## The Consuming Project

`adapters/nautilus/` is a **standalone Cargo workspace** — its own manifest, its
own pinned toolchain, its own gate (`make adapter-check`) — so the root
`cargo test` never touches it. It owns the market-data catalog and ingest, the
KRX trading calendar, the strategy lab and its improvement loop, the
live-session driver and its safety machinery, and the Production Ladder that
governs how much real capital a strategy may touch.

Two things about it are easy to get wrong from the SDK side:

- **It consumes the SDK at a declared bar.** The verification bar is
  *Recommended*, not merely Implemented. The SDK's completeness is not an
  invitation to use all of it.
- **It reaches outside LS where it must.** A small, reluctantly admitted set of
  external data sources (the KRX daily-market host and the KASI holiday API
  behind the calendar; KRX Open API services for instrument eligibility) answer
  questions the LS gateway does not. Minimizing that set is a standing posture,
  not an oversight.

Its own entry points are [`adapters/nautilus/CONTEXT.md`](adapters/nautilus/CONTEXT.md)
and [`adapters/nautilus/README.md`](adapters/nautilus/README.md).

## What is happening right now

`make next` answers it. It derives the KRX window state, reads the single work
queue at `queue/items.jsonl`, and reports any in-flight resumable sequence with
its exact resume command. Queue state changes go through
`lab-next add / done / supersede` — never a hand edit.

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
