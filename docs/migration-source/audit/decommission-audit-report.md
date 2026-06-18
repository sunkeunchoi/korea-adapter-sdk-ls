# Decommission Audit — Roll-up Report

This is the durable roll-up for the decommission audit of
`docs/migration-source-extraction-ledger.md`. It is produced/overwritten by the
`audit-carried-rows` orchestrator's serial roll-up phase after every per-row
auditor returns. The committed validator
`crates/ls-trackers/tests/decommission_audit.rs` recomputes the gate from the
frozen records under `records/` and keeps it checkable in CI after the old source
is gone (R14).

The **trustworthy-green** verdict line below is the documented precondition the
(out-of-scope) physical-decommission work item must cite. Decommission must not
proceed while any row is `unverifiable`/un-accepted or an unresolved `extract`.

---

## GATE: NOT-GREEN — audit not yet executed; 26 row(s) pending verdicts.

The audit machinery (manifest, recipe, auditor agent, orchestrator, validator) is
in place, but the `audit-carried-rows` fleet has not yet been run against the
sibling source `korea-broker-sdk-ls`, so no per-row records exist under
`records/`. Per R15 a missing verdict is not-green. Run
`audit-carried-rows` (with paper credentials for the one live-gateway behavioral
row) to populate `records/L1.yaml … L26.yaml`, then this report and the gate
verdict are recomputed from those records.

## Counts

| State | Count |
|---|---:|
| `confirmed` | 0 |
| `assumption-accepted` (counted toward green, reported apart) | 0 |
| `refuted` | 0 |
| `unverifiable` (un-accepted) | 0 |
| missing verdict (pending) | 26 |

## All rows

Verdict/classification/bar/pointer are populated by the fleet. `candidate_class`
is the manifest seed (the auditor records its own class, R5).

| ID | Area | Candidate class | Verdict | Bar | Evidence pointer |
|---|---|---|---|---|---|
| L1 | Maintained SDK architecture | knowledge | _pending_ | — | — |
| L2 | Auth/config/runtime transport lessons | behavioral | _pending_ | — | — |
| L3 | Paper/production environment finding | knowledge | _pending_ | — | — |
| L4 | Runtime success predicate | behavioral | _pending_ | — | — |
| L5 | Full response-code taxonomy | knowledge | _pending_ | — | — |
| L6 | Order acknowledgement codes | knowledge | _pending_ | — | — |
| L7 | Error enum public surface | knowledge | _pending_ | — | — |
| L8 | Retry and rate-limit behavior | behavioral | _pending_ | — | — |
| L9 | Order safety runtime | knowledge | _pending_ | — | — |
| L10 | Order dedup eviction lesson | knowledge | _pending_ | — | — |
| L11 | Full 365-TR dependency inventory | knowledge | _pending_ | — | — |
| L12 | Strong order-number coupling matrix | knowledge | _pending_ | — | — |
| L13 | SELF pagination population | knowledge | _pending_ | — | — |
| L14 | Trading-day sensitivity knowledge | knowledge | _pending_ | — | — |
| L15 | Paper-incompatible TR set | knowledge | _pending_ | — | — |
| L16 | WebSocket lifecycle runtime | behavioral | _pending_ | — | — |
| L17 | Broad WebSocket certification findings | knowledge | _pending_ | — | — |
| L18 | WebSocket backpressure policy | knowledge | _pending_ | — | — |
| L19 | Diagnostics/redaction contract | knowledge | _pending_ | — | — |
| L20 | Operations runbook | knowledge | _pending_ | — | — |
| L21 | Full certification/evidence pipeline | discard | _pending_ | — | — |
| L22 | Evidence buckets and residual lane | knowledge | _pending_ | — | — |
| L23 | Convenience API smoke interface | knowledge | _pending_ | — | — |
| L24 | Spec drift and fetch scripts | knowledge | _pending_ | — | — |
| L25 | Old generated Rust API surface | discard | _pending_ | — | — |
| L26 | Old release checklists and production-readiness docs | knowledge | _pending_ | — | — |

## Refuted rows (re-dispositioned)

_None yet._ Each refuted row will appear here with its `re_disposition`
(`extract`/`defer`/`discard`) and the named material gap, and the ledger row is
re-dispositioned away from `carried` (R3).

## Unverifiable rows (un-accepted)

_None yet._ Expect roughly the WebSocket sub-claims (reconnect replay, terminal
exhaustion, latest-only wakeup) and any production-only behavior (R6a) to land
here with a blocking reason until a named maintainer accepts them (R4a).

## Assumption-accepted rows

_None yet._ Each will appear with `accepted_by` and the specific residual risk
accepted — reported apart from `confirmed` so a gate green only via acceptances
is visibly distinct from one green via proof.

## Source-coverage reconciliation

_Pending._ After the fleet runs, every distinct old-source document referenced by
a manifest row must be claimed by at least one record's `claim_map`; any
old-source section owned by no row is surfaced here while the source is still
readable (the partition-by-row seam, R7a / U6).
