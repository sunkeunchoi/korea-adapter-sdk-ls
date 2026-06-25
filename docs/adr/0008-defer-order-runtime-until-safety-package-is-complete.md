# Defer order runtime until the safety package is complete

Order TRs carry duplicate-submission and ambiguous-outcome risk, so a partial runtime port would be more dangerous than leaving order execution unavailable. The first migration tier will track order TRs in metadata and document order-number coupling and reconciliation, but public order runtime behavior is deferred until no-retry dispatch, deduplication, reconciliation, and guarded focused evidence can ship together.

## Status update (2026-06-25): machinery-complete, evidence-pending

The first order package has now **built the full safety machinery** this ADR
required before any order runtime could land: no-retry `Inner::post_order`, the
`OrderDeduplicator` (the §2 opportunistic-eviction contract), the global order
kill switch, the distinct order success predicate, the redaction/tracing
contract, the six-state reconciliation matcher (`CSPAT00601` submit paired with
the `t0425` read), the re-added `LsError::DuplicateOrder` variant, and the
guarded paper-order evidence harness (`make live-smoke-order`). The order logic
is proven entirely against mocks (`order_logic_gate`).

This ADR's deferral is therefore **partially retired**: the technical blocker —
"the safety package is incomplete" — no longer holds. It is **not** marked
superseded, because the Implemented gate for an order TR is a guarded **live
paper order**, and that in-window evidence run has not yet been executed. Until
it is, `CSPAT00601` and `t0425` stay Tracked-not-Implemented and the order
success predicate is **seed-only/unconfirmed** (the `00039`/`00040` hypothesis
recorded in `order-safety-design.md` §1, not yet confirmed against observed live
codes). A clean in-window `make live-smoke-order` run flips both TRs and
supersedes this ADR; see `.agents/skills/implement-order-tr/SKILL.md`.
