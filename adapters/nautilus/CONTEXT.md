# Consuming Project Context

The **Consuming Project** is a Korean-equities algorithmic trading system built
on the SDK maintained in this repository's root workspace. It is one of the two
bounded contexts here — see [`CONTEXT-MAP.md`](../../CONTEXT-MAP.md) for the
other one and for the words that mean different things on either side of the
boundary.

This file is this context's **language and relationships**. Definitions live in
[`CONCEPTS.md`](../../CONCEPTS.md), the single authoritative glossary for the
whole repository; where a summary here and `CONCEPTS.md` disagree,
`CONCEPTS.md` wins.

## What this context is responsible for

Turning market data into governed, evidence-backed trading decisions, and
escalating a proven strategy toward real capital. It owns the market-data
catalog and its ingest, the KRX trading calendar, the strategy lab and its
improvement loop, the live-session driver and its safety machinery, and the
Production Ladder.

It does **not** own the LS wire protocol, TR metadata, drift tracking, or the
support ladder. Those belong to the SDK Project and are consumed, not extended,
from here.

## Completion criterion

The product is a [[Strategy lineage]] — one hypothesis under continuous
versioned upgrade — carried to a rung of the Production Ladder that touches real
capital. Neither a frozen strategy nor the improvement loop as a bare capability
is the deliverable.

Exactly **one** lineage is open at a time. Parallel lineages are refused: they
are the same shape as the failure recorded in
[ADR 0015](../../docs/adr/0015-two-projects-one-repository.md) — two unfinished
things advanced together.

## Language

Defined in [`CONCEPTS.md`](../../CONCEPTS.md); grouped here by what they are for.

- **Data supply** — Accumulate-forward, Adjustment-basis splice, Catalog re-base,
  Basis-shift heal, Suspect partial, Universe metadata pin, Rolling call budget,
  Body-cursor continuation, Degenerate chart window
- **Calendar** — KRX trading-date status, Witness, Forward readiness, Genesis
  snapshot, Derived window state, Attended Unknown override, Calendar Adoption
  State
- **External data** — External Data Source
- **Universe** — Point-in-Time Research Universe, Session Tradable Universe,
  Intraday Tradability, Reference Instrument, Mount universe, Mount universe file
- **Strategy loop** — Strategy-improvement loop, Strategy lineage, Lineage
  closure, Search budget, Run registry, Latest finalized run, Merit-bearing turn,
  Param-turn governance, Strategy-logic turn, Composed data turn,
  Mechanism-harness turn, Lever queue, Return-on-risk (RoR), Two-Tier Portfolio
  Simulation
- **Live capital** — Production ladder, Head identity, Dispatch release, Limit
  event, Live-session driver, Watchdog envelope, Tracking-error band, Expectation
  band
- **Consumed from the SDK Project** — Verification Bar, Paper Live Smoke,
  Credential lane, and the order-safety vocabulary (Double fill, Order
  reconciliation, Account-flat assertion, …)

## Relationships

- The **Verification Bar** is Recommended. This context calls a TR only at or
  above it; TRs already consumed below it sit on a monotonically shrinking
  deferral list.
- An **External Data Source** is admitted only where the SDK cannot answer the
  question, one source at a time, and states what it does not claim.
- **Lineage closure** is decided on detectability, never profitability, and is
  not reopenable by acquiring data.
- A **Search budget** is registered before a lineage's first turn, because the
  selection tax it spends is irremovable.
- The **Production ladder** governs capital; the SDK Project's support ladder
  governs TR maturity. They are unrelated ladders that share a word.
- **Head identity** is scoped to the decision source file, so any edit to it —
  even a provably inert one — re-baselines the ladder.
- Nothing here is a dependency of the SDK Project. The dependency runs one way.

## Gate

This workspace opts out of the root Cargo workspace and has its own toolchain
and its own gate: `make adapter-check` from the repository root. Run it whenever
a change can reach across the boundary — including an `ls-core`/`ls-sdk` edit
made on the other side, which can redden this workspace without the root gate
noticing.
