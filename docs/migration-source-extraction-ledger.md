# Migration Source Extraction Ledger

This ledger is the decommission gate for `/Users/mini/dev/korea-broker-sdk-ls`.
The old repository is not a **Decommissioned Migration Source** until every
knowledge asset below is either extracted into `korea-adapter-sdk-ls`, deferred
to an explicit future expansion item, or deliberately discarded with a reason.

## Disposition States

- `carried`: the knowledge is already represented in maintained code, metadata,
  docs, tests, or ADRs in this repository.
- `extract`: migrate the knowledge into this repository before decommission.
- `defer`: keep the knowledge tied to a named future expansion or maintenance
  item; decommission is blocked unless that deferral is explicit enough to work
  without the old repository.
- `discard`: intentionally do not carry the knowledge forward; record why.

## Decommission Rule

`korea-broker-sdk-ls` can be decommissioned only when this ledger has no
unresolved `extract` rows and every `defer` row points to a maintained issue,
plan, ADR, or metadata record in this repository.

## Row IDs

Each row carries a stable `ID` (`L1`…`L26`) in the first column. The IDs are the
dispatch key for the decommission audit (`docs/migration-source/audit/`): the
audit manifest pairs each `L<N>` with its area, and the committed validator
(`crates/ls-trackers/tests/decommission_audit.rs`) reconciles the manifest's IDs
1:1 against the IDs parsed from this table by **header name** (not column
position), so a future reorder of the table cannot silently re-map an ID. Do not
renumber or reuse an ID once assigned; a refuted row keeps its ID when it is
re-dispositioned.

## Ledger

| ID | Area | Old source | Current disposition | Notes / required extraction |
|---|---|---|---:|---|
| L1 | Maintained SDK architecture | `docs/plans/maintained-sdk-migration-plan.md`, old runtime layout | `carried` | Represented by this repo's ADRs, `CONTEXT.md`, crate layout, metadata model, and maintained-surface stance. |
| L2 | Auth/config/runtime transport lessons | `crates/core/src/{auth,config,inner,client,rate_limiter,error}.rs`, runtime tests | `carried` | Core token handling, env resolution, HTTP dispatch, retry, rate buckets, `tr_cont` headers, timeout config, and parse helpers are represented in `ls-core` and slice tests. |
| L3 | Paper/production environment finding | `docs/ENVIRONMENT_VERIFICATION_RESEARCH.md` | `carried` | Paper live smoke guard and docs preserve the finding that LS has no reliable REST-side server signal distinguishing paper from production. |
| L4 | Runtime success predicate | `docs/certification_taxonomy.md`, `crates/core/src/inner.rs`, `crates/core/tests/api_error_tests.rs` | `carried` | `00000`, empty, `00136`, and `00707` are success in `ls-core`; `01900` is preserved as the sole paper-incompatible signal. |
| L5 | Full response-code taxonomy | `docs/certification_taxonomy.md`, release remediation docs | `carried` | Extracted into `docs/design/ls-gateway-response-semantics.md`: success predicate, `01900` discipline, hard-failure codes, proven residual rule, `01715`, and future order-code constraints. |
| L6 | Order acknowledgement codes | `crates/core/tests/api_error_tests.rs` | `carried` | Captured in `docs/design/ls-gateway-response-semantics.md` and `docs/design/order-safety-design.md` as future order-runtime constraints: `00039`/`00040` are order-path successes, not read-only successes. |
| L7 | Error enum public surface | `crates/core/tests/error_tests.rs` | `carried` | Captured in `docs/design/order-safety-design.md`: order runtime must restore fail-closed serialization behavior and reconsider order-specific error variants such as duplicate/serialization failures before implementation. |
| L8 | Retry and rate-limit behavior | `crates/core/tests/resilience_tests.rs`, `docs/OPERATIONS_RUNBOOK.md` | `carried` | Non-order retry and rate-limit waiting are represented in `ls-core`; order no-retry is captured as design but not runtime. |
| L9 | Order safety runtime | `docs/ORDER_SAFETY_DESIGN.md`, `docs/ORDER_RECONCILIATION_DESIGN.md`, order tests | `carried` | Extended `docs/design/order-safety-design.md` with the runtime seam, kill switch, dedup key/TTL/fail-closed behavior, unknown-state model, `t0425` reconciliation, guarded manual evidence, and production-test prohibition. |
| L10 | Order dedup eviction lesson | `docs/ORDER_SAFETY_DESIGN.md`, solution note referenced by current design | `carried` | Current `docs/design/order-safety-design.md` records opportunistic write-path sweeping and no background worker. |
| L11 | Full 365-TR dependency inventory | `docs/TR_DEPENDENCY_GUIDE.md`, `docs/TR_DEPENDENCY_REFERENCE.md`, `scripts/analysis/derive_tr_dependencies.py` | `carried` | Extracted into `docs/migration-source/tr-dependencies-2026-06-14.json` with index note `docs/design/tr-dependency-inventory-snapshot.md`. Covers all old TR classes, strong couplings, `cts_*` self fields, date sensitivity, paper incompatibility, market/session classes, and weak identifiers. |
| L12 | Strong order-number coupling matrix | `docs/TR_DEPENDENCY_REFERENCE.md` | `carried` | The full 130-edge matrix is preserved in `docs/migration-source/tr-dependencies-2026-06-14.json`; representative consuming TRs are summarized in `docs/design/tr-dependency-inventory-snapshot.md`. |
| L13 | SELF pagination population | `docs/TR_DEPENDENCY_REFERENCE.md` | `carried` | The full 61-TR `cts_*` population is preserved as `self_fields` in `docs/migration-source/tr-dependencies-2026-06-14.json`. |
| L14 | Trading-day sensitivity knowledge | `docs/TR_DEPENDENCY_GUIDE.md`, certification runs | `carried` | The 73 date-sensitive TRs are preserved as `date_fields` and `environmental.trading_day_sensitive` in `docs/migration-source/tr-dependencies-2026-06-14.json`; `01715` semantics are in `docs/design/ls-gateway-response-semantics.md`. |
| L15 | Paper-incompatible TR set | `docs/TR_DEPENDENCY_GUIDE.md`, `docs/SIMULATION_EVIDENCE_MATRIX.md`, fixture debt scripts | `carried` | The seven observed paper-incompatible TRs are preserved in `docs/design/tr-dependency-inventory-snapshot.md` and the JSON snapshot; runtime classification remains `01900`-only. |
| L16 | WebSocket lifecycle runtime | old WS manager/tests, `docs/SIMULATION_EVIDENCE_MATRIX.md` | `carried` | Current `S3_` path carries connect/auth/subscribe/unsubscribe, reconnect replay, terminal exhaustion, and latest-only wakeup lessons. |
| L17 | Broad WebSocket certification findings | `docs/SIMULATION_EVIDENCE_MATRIX.md`, WS certification tests | `carried` | Extracted into `docs/design/websocket-certification-findings.md`: Transport vs FrameDecode semantics, Phase 83/84 harness-bug reversal, old shard counts, timing/event residuals, and future expansion guidance. |
| L18 | WebSocket backpressure policy | `docs/WEBSOCKET_BACKPRESSURE_POLICY.md`, WS tests | `carried` | Current realtime code carries `DropNewest` and `LatestOnly`; confirm diagnostics completeness separately. |
| L19 | Diagnostics/redaction contract | `docs/DIAGNOSTICS_CONTRACT.md`, `docs/OBSERVABILITY_AND_DIAGNOSTICS.md` | `carried` | Extracted into `docs/operations/operator-diagnostics.md`: stable diagnostic fields, redaction guarantees, field compatibility, and operator interpretation. |
| L20 | Operations runbook | `docs/OPERATIONS_RUNBOOK.md` | `carried` | Extracted into `docs/operations/operator-diagnostics.md`: auth failures, LS business errors, rate limiting, WS reconnect/backpressure, credential rotation, emergency order disablement, and tracker-drift handling. |
| L21 | Full certification/evidence pipeline | `docs/SIMULATION_EVIDENCE_MATRIX.md`, `scripts/local_evidence_runner.py`, evidence pack scripts/tests | `discard` | Do not preserve the old full generated-surface release gate as permanent architecture. Extract only reusable gateway facts, response semantics, and operator lessons. |
| L22 | Evidence buckets and residual lane | `docs/SIMULATION_EVIDENCE_MATRIX.md`, `docs/certification_taxonomy.md`, release docs | `carried` | Extracted into `docs/design/release-readiness-and-residual-lessons.md`: timing buckets, residual proof discipline, fail-closed signatures, and scoped residual-bearing claims. |
| L23 | Convenience API smoke interface | `docs/CONVENIENCE_API_EVIDENCE_INTERFACE.md`, workflow tests | `carried` | Extracted into `docs/design/release-readiness-and-residual-lessons.md` as future convenience-helper evidence discipline: helper evidence is separate from TR evidence, stale fixtures are not live gateway failures, and future helpers need an explicit evidence seam. |
| L24 | Spec drift and fetch scripts | `docs/SPEC_DRIFT_REVIEW.md`, `scripts/fetch_ls_specs.py`, `scripts/change_detector.py` | `carried` | Current repo has Rust-owned API Drift and Specification Document trackers. Old Python scripts are not permanent tooling. |
| L25 | Old generated Rust API surface | generated crates/tests for all categories | `discard` | The current repo intentionally rejects full generated-surface ownership and compatibility. Retain only selected behavior and extracted gateway knowledge. |
| L26 | Old release checklists and production-readiness docs | `docs/RELEASE_CHECKLIST_TEMPLATE.md`, `docs/RUST_RELEASE_CONFIDENCE_PLAN.md`, release evidence docs | `carried` | Extracted into `docs/design/release-readiness-and-residual-lessons.md`: production-order prohibition, paper/prod credential separation, operator live evidence, release-confidence lessons, and what old release machinery is intentionally not carried. |

## Immediate Extraction Queue

No immediate extraction or deferral items remain. The remaining `discard` rows
are intentional non-carry decisions for the old generated-surface machinery and
generated API compatibility promise.
