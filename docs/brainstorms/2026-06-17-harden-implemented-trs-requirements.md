---
date: 2026-06-17
topic: harden-implemented-trs
---

# Harden the Implemented Surface: Promote Smoke-Ready TRs to Recommended

## Summary

Promote the four smoke-ready implemented TRs — `t1102`, `t8412`, `CSPAQ12200`, `S3_` — from
`support:implemented` to `support:recommended` by capturing Focused Evidence from their
existing Paper Live Smokes. One `queue:maintenance` work item; each TR promotes
independently as its smoke passes in a session. `revoke` is excluded (separate later item).

## Problem Frame

Foundation Complete is reached and the Maintenance Work Queue is empty. Of the eight
maintained TRs, only `token` and `t1101` are `recommended`; five more are implemented and
tested but carry the "not yet recommended" banner because they lack recorded Focused
Evidence. Four of those five already have working Paper Live Smoke harnesses
(`live-smoke`, `live-smoke-chart`, `live-smoke-account`, `live-smoke-ws`), so the gap is
recorded evidence, not capability. Closing it raises user-facing recommendation coverage
through the cheapest repeatable loop the project has — the same flow just proven on
`t1101`.

## Key Decisions

- **Batch the four smoke-ready TRs in one `queue:maintenance` item.** `revoke` is split
  out because it has no smoke and its semantics are destructive (it invalidates the token).
- **Reuse the existing smoke harnesses.** Each promotion is evidence-capture plus metadata
  (a `metadata/evidence/<tr>.yaml` record, a `recommendation` contract block, `recommended:
  true`, `last_reviewed` matching the evidence date), then regenerated docs — no new SDK
  code.
- **Partial completion is allowed.** A TR whose gate cannot be satisfied this session stays
  `implemented` and is recorded as such; it does not block the others.
- **`S3_`'s claim is scoped to lifecycle reachability** — its smoke proves
  connect/subscribe/unsubscribe, not live trade-data correctness, so its `recommendation`
  excludes trade-data correctness.

## Requirements

- R1. Each promoted TR gains a `metadata/evidence/<tr>.yaml` record capturing its
  `LIVE-SMOKE` line verbatim — credential-free (no token, appkey, secret, or account
  number), with `date` equal to the TR's `maintenance.last_reviewed`.
- R2. Each promoted TR's metadata sets `support.recommended: true` and adds a
  `recommendation` block (`behavior`, `evidence_ref`, `excludes`) scoped to what the smoke
  actually proves.
- R3. SDK Reference / TR Dependency Docs are regenerated; each promoted TR drops the
  "Implemented, not yet recommended" banner. `docs-check` passes.
- R4. A TR is promoted only on a passing smoke. `CSPAQ12200` requires a provisioned paper
  account; `t8412` requires a real trading-day date; `t1102` and `S3_` require an open KRX
  session. A TR whose gate cannot be met this session stays `implemented`, and the item
  records why.
- R5. The work item is a `queue:maintenance` issue labelled `class:*` per the promoted TRs,
  `support:recommended` (target), `gate:change-scoped`, `evidence:needed`. Baseline
  Promotion is not needed (no new TR).
- R6. Downstream count/banner assertions are updated to match the new recommended set
  (the docgen banner list shrinks; `EVIDENCE-FRESHNESS.md`'s recommended-TR count rises).

## Success Criteria

- Every TR whose smoke passes this session is `recommended` with a committed evidence file;
  the full workspace gate (`cargo test`, `docs-check`) stays green.
- Any TR held at `implemented` is explicitly recorded with the unmet gate, not silently
  skipped.
- The item closes when all four reachable TRs are promoted or explicitly held with reason.

## Scope Boundaries

- `revoke` promotion — deferred to its own item (needs a new smoke harness and
  destructive-ordering care, since revoke invalidates the session token).
- The evidence-freshness evaluator (90-day backstop / change-driven invalidation) — still
  deferred (see `metadata/EVIDENCE-FRESHNESS.md`).
- Coverage expansion (new TRs) and orders (`CSPAT00601`) — out of scope.

## Dependencies / Assumptions

- The four smoke harnesses pass against the paper gateway during their gating windows.
- **Assumption:** the paper account is provisioned enough for `CSPAQ12200`'s read-only
  balance smoke; if not, that TR stays `implemented` (R4) and provisioning becomes a
  follow-up.
- Each `recommendation` block's `excludes` honestly scopes the claim to what its smoke
  proves — most pointedly `S3_` (lifecycle reachability, not trade-data correctness).

## Outstanding Questions

**Deferred to planning / execution**

- The exact `behavior` and `excludes` text for each TR's `recommendation` block.
- Whether `S3_`'s websocket-lifecycle evidence is a sufficient bar for a `recommended`
  claim, or whether the claim should stay narrower than the other three.

## Sources / Research

- Prior promotion pattern: `docs/plans/2026-06-17-001-feat-t1101-stage2-expansion-plan.md`,
  `metadata/evidence/t1101.yaml`, `metadata/trs/token.yaml` (recommendation block shape).
- Smoke harnesses: `crates/ls-sdk/tests/live_smoke.rs`, `Makefile` (`live-smoke*` targets).
- Banner / count assertions: `crates/ls-docgen/src/lib.rs`; freshness policy:
  `metadata/EVIDENCE-FRESHNESS.md`.
- Queue contract: `docs/MAINTENANCE_RUNBOOK.md`, `docs/maintenance-labels.md`.
