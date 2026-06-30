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

## Status update (2026-06-30): evidence ran — deferral retired (SUPERSEDED)

Both gates this ADR deferred have now run on real in-window paper order
placement, so the deferral is **fully retired**:

- **Implemented gate (plan 2026-06-25-005).** A clean in-window guarded
  order-chain certified the submit/modify/cancel legs from observed live broker
  codes (`00040` / `00462` / `00463`), confirming the order success predicate
  that was seed-only/unconfirmed above. `CSPAT00601`, `CSPAT00701`, `CSPAT00801`,
  and `t0425` flipped Tracked → **Implemented**.
- **Recommended gate (plan 2026-06-30-002, this retirement).** The Recommended
  tier is the point at which the SDK endorses *live order placement* — the
  endorsement this ADR held back pending an in-window evidence run. A fresh clean
  in-window `make live-smoke-order-chain` placed a real paper submit→modify→cancel
  lifecycle (`cert=certified`, `00040`/`00462`/`00463`) and the harness flat
  assertion positively confirmed the account flat afterward. On that evidence the
  four order TRs flipped Implemented → **Recommended** with credential-free
  Focused Evidence (`metadata/evidence/{CSPAT00601,CSPAT00701,CSPAT00801,t0425}.yaml`).

The technical blocker ("the safety package is incomplete") and the evidence
blocker ("no in-window order-placement run has happened") are both resolved. This
ADR is **SUPERSEDED** — the order surface is callable, Implemented, and now
Recommended on paper. (Note: the live-placement evidence was sourced from the
autonomous `live-smoke-order-chain`, not the `live-smoke-order` matrix — the
matrix's marketable scenario fills on an open market and would leave a position
needing an out-of-band paper reset; the chain leaves the account confirmed flat.)
