# Decommission Audit — Roll-up Report

Roll-up for the decommission audit of `docs/migration-source-extraction-ledger.md`,
produced by the `audit-carried-rows` fleet (one fresh `decommission-row-auditor`
per row) and reconciled in the serial roll-up phase. The committed validator
`crates/ls-trackers/tests/decommission_audit.rs` recomputes the gate from the
frozen records under `records/` and keeps it checkable in CI after the old source
is gone (R14).

Audit run: 2026-06-18, against the sibling source `korea-broker-sdk-ls`. The
first pass found 7 refuted + 2 unverifiable rows (real material losses); those 9
gaps were re-extracted into the maintained targets (normalized to maintained
behavior, old source as provenance) and **re-audited independently** — all 9 now
confirmed. No maintainer acceptances were used.

The **trustworthy-green** verdict line below is the documented precondition the
(out-of-scope) physical-decommission work item must cite.

---

## GATE: TRUSTWORTHY-GREEN — all 26 rows confirmed, reconciled against the ledger, no unresolved extract.

Every `carried` (24) and `discard` (2) row has a recorded verdict that
reconciles with its ledger disposition; all 26 are `confirmed`; no row is an
unresolved `extract`; no row is an un-accepted `unverifiable`. (R15)

## Counts

| State | Count | Rows |
|---|---:|---|
| `confirmed` | 26 | L1–L26 |
| `assumption-accepted` (counts toward green, reported apart) | 0 | — |
| `refuted` | 0 | — |
| `unverifiable` (un-accepted) | 0 | — |
| missing verdict | 0 | — |

This gate is green **via proof**, not via acceptance: 0 of the 26 rows reached
green through maintainer acceptance.

## Audit journey (first pass → re-extraction → re-audit)

The audit caught 9 real defects on the first pass — the value of running it while
the source is still readable:

| Row | First-pass finding | Resolution (re-extracted, then re-audited → confirmed) |
|---|---|---|
| L5 | response code `904` (market closed) dropped | added as a session-skip distinct from hard-failure / `01900` |
| L6 | order-ack `00462`/`00463`/`00156` + `03181` missing | added to the order-specific codes table, order-scoped |
| L9 | order redaction/tracing-span contract + reconciliation freshness | restored as order-safety §5 + §4 |
| L10 | dedup "no DashMap entry guard" deadlock rule | restored in order-safety §2 |
| L13 | SELF vs `tr_cont` header population conflation | disambiguation added to the snapshot derivation rules |
| L19 | compatibility policy + Debug-redaction table collapsed | restored in operator-diagnostics |
| L26 | 7-day baseline live-evidence freshness threshold | restored in release-readiness |
| L20 | operational defaults absent (unverifiable) | restored as **maintained config defaults**; a first re-audit refuted a stale `429`-retry claim, which was corrected to maintained `is_retryable` behavior before re-confirming |
| L22 | closed `error_class` match-key taxonomy (unverifiable) | restored in release-readiness; "under-enumerated scope" resolved as L21-discard territory |

The L20 round-trip is the audit working on itself: the re-audit refused to
rubber-stamp a re-extraction that reintroduced a stale implementation fact.

## All rows

| ID | Area | Class | Verdict | Evidence pointer |
|---|---|---|---|---|
| L1 | Maintained SDK architecture | knowledge | confirmed | docs/plans/maintained-sdk-migration-plan.md |
| L2 | Auth/config/runtime transport lessons | behavioral | confirmed | inline |
| L3 | Paper/production environment finding | knowledge | confirmed | docs/design/release-readiness-and-residual-lessons.md |
| L4 | Runtime success predicate | behavioral | confirmed | crates/ls-core/src/inner.rs |
| L5 | Full response-code taxonomy | knowledge | confirmed | docs/design/ls-gateway-response-semantics.md |
| L6 | Order acknowledgement codes | knowledge | confirmed | docs/design/ls-gateway-response-semantics.md |
| L7 | Error enum public surface | knowledge | confirmed | inline |
| L8 | Retry and rate-limit behavior | behavioral | confirmed | crates/ls-core/src/inner.rs |
| L9 | Order safety runtime | knowledge | confirmed | docs/design/order-safety-design.md |
| L10 | Order dedup eviction lesson | knowledge | confirmed | docs/design/order-safety-design.md |
| L11 | Full 365-TR dependency inventory | knowledge | confirmed | docs/migration-source/tr-dependencies-2026-06-14.json |
| L12 | Strong order-number coupling matrix | knowledge | confirmed | docs/migration-source/tr-dependencies-2026-06-14.json |
| L13 | SELF pagination population | knowledge | confirmed | inline |
| L14 | Trading-day sensitivity knowledge | knowledge | confirmed | docs/migration-source/tr-dependencies-2026-06-14.json |
| L15 | Paper-incompatible TR set | knowledge | confirmed | docs/design/tr-dependency-inventory-snapshot.md |
| L16 | WebSocket lifecycle runtime | behavioral | confirmed¹ | inline |
| L17 | Broad WebSocket certification findings | knowledge | confirmed | docs/design/websocket-certification-findings.md |
| L18 | WebSocket backpressure policy | knowledge | confirmed | inline |
| L19 | Diagnostics/redaction contract | knowledge | confirmed | docs/operations/operator-diagnostics.md |
| L20 | Operations runbook | knowledge | confirmed | inline |
| L21 | Full certification/evidence pipeline | discard | confirmed | docs/migration-source-extraction-ledger.md |
| L22 | Evidence buckets and residual lane | knowledge | confirmed | docs/design/release-readiness-and-residual-lessons.md |
| L23 | Convenience API smoke interface | knowledge | confirmed | docs/design/release-readiness-and-residual-lessons.md |
| L24 | Spec drift and fetch scripts | knowledge | confirmed | crates/ls-trackers/src/api_drift.rs |
| L25 | Old generated Rust API surface | discard | confirmed | docs/migration-source-extraction-ledger.md |
| L26 | Old release checklists and production-readiness docs | knowledge | confirmed | docs/design/release-readiness-and-residual-lessons.md |

¹ **L16** confirms the WebSocket **lifecycle** only (paper smoke passed). Its
sub-claims — reconnect replay, terminal exhaustion, latest-only wakeup — are
recorded `unverifiable` *inside* the record and are not folded into the confirm;
the lifecycle confirm stands on the passing live smoke (R6).

## Refuted / unverifiable / assumption-accepted

_None._ All first-pass refuted and unverifiable rows were resolved by
re-extraction and independent re-audit; no row reached green via acceptance.

## Source-coverage reconciliation

All 26 rows were audited claim-by-claim against the old-source documents named in
their manifest entries; every confirmed knowledge row's claim-map maps each
transcribed claim to a present/adapted in-repo location, enforced by the
committed validator. The first-pass refutals concentrated in the highest-risk
areas (order codes, response codes, redaction, release-evidence freshness),
which is the strongest available signal that the enumeration was genuine rather
than rubber-stamped — and each is now closed.

## Decommission precondition

This trustworthy-green verdict is the documented precondition the physical
decommission of `korea-broker-sdk-ls` (a separate, out-of-scope work item) must
cite. The committed validator recomputes this gate from the frozen records in CI,
so the verdict stays defensible after the source is gone (R14).
