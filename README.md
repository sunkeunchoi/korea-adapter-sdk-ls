# korea-adapter-sdk-ls

A **maintained Rust SDK** for the LS Securities Open API. The SDK is built by
tracking upstream API and documentation change and applying *reviewed* SDK
changes — not by regenerating a full surface from specs.

## What this is (and is not)

This SDK surface is **selective by design**. It does not aim to expose every
upstream transaction request (TR); it implements the behavior maintainers have
chosen to own, and recommends only what has current evidence behind it.

- **Implemented ≠ Recommended.** A TR can be wired and tested without being
  recommended for use. Promotion to **Recommended** requires current
  **Focused Evidence**.
- **`token` is currently the only Recommended TR** — Paper OAuth access-token
  issuance, backed by a credential-free paper live-smoke record. See
  [`docs/reference/token.md`](docs/reference/token.md) for the full contract
  (what it claims, what evidence backs it, and what it does not claim).
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

## Relationship to `korea-broker-sdk-ls`

`korea-broker-sdk-ls` is **historical Migration Source material**, not the
maintained SDK and not a dependency. Its old generated all-TR surface seeded this
project; its docs, runtime lessons, and specifications remain migration reference
only. New SDK behavior belongs here, in the maintained surface.
