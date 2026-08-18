# Context Map

This repository holds **two bounded contexts**. They are two projects that were
once two repositories; the split, the collapse that caused it, and why the
boundary is now a Cargo workspace instead of a repository are recorded in
[ADR 0015](docs/adr/0015-two-projects-one-repository.md).

| Context | Lives in | Language |
|---------|----------|----------|
| **SDK Project** — the strict, maintained Rust SDK for the LS Open API | root Cargo workspace: `crates/`, `metadata/`, `docs/` | [`CONTEXT.md`](CONTEXT.md) |
| **Consuming Project** — the Korean-equities trading system built on that SDK | standalone workspace: `adapters/nautilus/` | [`adapters/nautilus/CONTEXT.md`](adapters/nautilus/CONTEXT.md) |

System-wide decisions live in [`docs/adr/`](docs/adr/). The SDK Project's own
context file sits at the repository root rather than under `crates/` because the
root Cargo workspace *is* that context.

The `.repository-engineering/` package and its `ls-repository-engineering` leaf
crate are repository-level governance tooling shared across this layout. They do
not introduce a third product context or change the one-way product dependency.
The independently locked `tools/repository-engineering-runtime/` workspace is
the package's inactive, host-neutral execution counterpart: it consumes a
copied closed bundle and has no dependency on either product runtime, no active
registry entry, and no authority over the legacy audit workflow.
Its read-only comparator writes only to a caller-owned external directory; the
inert package can validate and create-new import that bounded payload, but the
import surface cannot execute a capability or advance lifecycle authority.

## Where definitions live

[`CONCEPTS.md`](CONCEPTS.md) is the **single authoritative glossary** for both
contexts — every term is defined there, once, in depth. The two `CONTEXT.md`
files do not re-define terms; they say what each context is responsible for,
which language belongs to it, and what relationships hold inside it. On any
disagreement between a `CONTEXT.md` summary and `CONCEPTS.md`, `CONCEPTS.md`
wins.

## The boundary

The dependency runs **one way**: the Consuming Project depends on the SDK
Project, never the reverse. Two consequences follow.

- **Verified-only consumption.** The Consuming Project calls a TR only at or
  above its declared verification bar (see *Verification Bar* in the Consuming
  Project's context). The SDK's completeness is not an invitation to use all of
  it.
- **Invisible breakage crosses the boundary.** The two workspaces build
  separately, so an `ls-core`/`ls-sdk` change can redden the adapter without the
  root gate noticing. `make adapter-check` is the guard; run it whenever a
  change can reach across.

## Terms that collide across the boundary

The same word means different things on either side. Qualify it, or use the
context's own term.

| Word | In the SDK Project | In the Consuming Project |
|------|--------------------|--------------------------|
| **ladder** | the TR support ladder: Raw → Tracked → Implemented → Recommended (a TR's maturity) | the Production Ladder: rungs 0–4 (how much real money a strategy touches) |
| **genesis snapshot** | — | two distinct meanings: the *calendar* chain root, and the dispatch-chain genesis in the lab. Always qualify. |
| **evidence** | Focused Evidence: a recorded paper-live-smoke result backing a Recommended TR | run artifacts: performance report, decision stream, data-quality report, manifest |
| **version** | a TR's metadata/baseline revision | a strategy lineage's version (head identity), which gates live capital |
