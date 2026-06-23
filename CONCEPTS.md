# Concepts

Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## TR & dispatch

### TR
One LS-securities API operation, identified by a transaction code (e.g. `t1102`, `CSPAQ12200`). The SDK models each TR as a request/response pair plus a runtime endpoint policy, and tracks per-TR metadata (facets, support tier, provisional facets) independently of every other TR. The unit this whole codebase is organized around.
*Avoid:* transaction (generic), endpoint (a TR is more than its path — several TRs can share one path, distinguished by the transaction code).

### owner_class
The dispatch class a TR is routed through, naming which SDK handle exposes it and which runtime machinery it uses: market-session reads, single-page paginated reads, account-state reads, realtime/websocket, order operations, plus a placeholder used before a TR's class is confirmed. A TR's owner_class is corrected from the placeholder to its real class only when the TR becomes callable.

### Facet
A single classified property of a TR (its session scope, instrument domain, rate bucket, pagination behavior, caller-supplied identifiers, and similar). A facet is either hard-accurate (confirmed against the captured spec) or provisional (best-effort, pending live confirmation — tracked in the Provisionality Ledger).

## Support lifecycle

A TR climbs a three-rung support ladder; each rung is a deliberate, separately-gated promotion.

### Tracked
The lowest rung: the TR has committed metadata and a maintained baseline but no callable code. It is observed for drift, nothing more.

### Implemented
The middle rung: the TR has hand-authored callable Rust and has passed a Paper Live Smoke (a representative call that constructs, returns a success code, yields a non-empty result, and deserializes). An Implemented TR is callable but carries no recommendation and no recorded evidence — explicitly *not* endorsed for production use.

### Recommended
The top rung: an Implemented TR additionally cleared for production use, backed by recorded Focused Evidence and a recommendation block. Promotion to Recommended is a separate, deliberate act beyond Implemented.

### Paper Live Smoke
A credential-gated integration test that hits the real LS *paper* gateway with real credentials to prove a TR is genuinely callable. It is the gate for flipping a TR to Implemented; a smoke that returns an empty result leaves the TR callable-but-unconfirmed (pending), not Implemented.

### Focused Evidence
A recorded, credential-free result of a Paper Live Smoke that backs a Recommended TR's claim. A smoke run gates Implementation; it only becomes Focused Evidence when a TR is deliberately promoted to Recommended.

### Provisionality Ledger
The repository-level sidecar that records, per TR, which authored facets are still provisional and what must be re-verified before promotion. Rows retire as a TR is implemented and each facet is confirmed against a live call; a pending or held TR keeps its rows so nothing is silently treated as confirmed.

### Pending
A TR whose Paper Live Smoke ran but did not open the Implemented gate — callable yet shape-unconfirmed (empty result), or blocked by an unresolved input or an environmental gateway rejection. A pending TR ships without flipping to Implemented and keeps its provisional ledger rows.
