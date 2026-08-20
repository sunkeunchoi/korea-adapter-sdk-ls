# Two projects, one repository

This repository holds **two** projects, not one: the *SDK Project* (the strict,
maintained Rust SDK for the LS Open API, in the root Cargo workspace) and the
*Consuming Project* (the Korean-equities trading system, in the standalone
`adapters/nautilus/` workspace). They previously lived in two separate
repositories. Neither was finished, and advancing two unfinished things across a
repository boundary collapsed the effort — every change needed a coordinated
release on the other side, and neither side could ever be declared done. We
moved both into one repository and put the boundary at a **Cargo workspace**
instead: `adapters/nautilus/` opts out of the root workspace, pins its own
toolchain, and has its own gate (`make adapter-check`). The coupling is now
visible and changeable in a single commit, while the two projects keep separate
completion criteria — the SDK is held strict and tracks the upstream API
completely, and the Consuming Project takes only what it needs and only at its
declared verification bar.

## Consequences

- **The dependency is one-way and must stay that way.** The Consuming Project
  depends on the SDK; the SDK never depends on the Consuming Project. Because
  the two workspaces build separately, an `ls-core`/`ls-sdk` edit can redden the
  adapter invisibly — which is exactly why `make adapter-check` exists and must
  be run whenever a change can reach across the boundary.
- **Do not re-split.** A future reader will notice that a trading system living
  inside an API-client repository is unusual and may propose extracting it. The
  extraction is the failure this ADR records.
- **The same shape is rejected elsewhere.** "Two unfinished things advanced in
  parallel" is a failure mode, not a repository-layout detail. It is also why
  the Consuming Project runs **one** strategy lineage at a time rather than
  several in parallel (see `adapters/nautilus/CONTEXT.md`).
- **Do not grow a second data platform through convenience.** The public KRX
  Open API leg admitted for issue #255 stops at instrument eligibility. It must
  never expand into quotes, financials, or news; any such use needs a separate,
  explicit source-admission decision. This restriction applies to the Open API
  leg, not to #255's separately governed licensed-sample procurement.
