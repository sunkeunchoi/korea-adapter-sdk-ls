# Decommission Audit — Roll-up Report

Roll-up for the decommission audit of `docs/migration-source-extraction-ledger.md`,
produced by the `audit-carried-rows` fleet (one fresh `decommission-row-auditor`
per row) and reconciled in the serial roll-up phase. The committed validator
`crates/ls-trackers/tests/decommission_audit.rs` recomputes the gate from the
frozen records under `records/` and keeps it checkable in CI after the old source
is gone (R14).

Audit run: 2026-06-18, against the sibling source `korea-broker-sdk-ls`. All 26
rows returned a verdict; the 7 refuted rows have been re-dispositioned to
`extract` in the ledger.

The **trustworthy-green** verdict line below is the documented precondition the
(out-of-scope) physical-decommission work item must cite. Decommission must not
proceed while any row is `unverifiable`/un-accepted or an unresolved `extract`.

---

## GATE: NOT-GREEN — 9 blocking row(s): L5, L6, L9, L10, L13, L19, L26 (refuted → extract) + L20, L22 (unverifiable, un-accepted).

The audit caught **7 real material losses** in rows that read `carried` on paper,
plus **2 rows whose completeness could not be established**. This is the audit
working as intended — each gap was found while the old source is still readable,
so it is a recoverable re-extraction rather than an unrecoverable post-decommission
loss.

## Counts

| State | Count | Rows |
|---|---:|---|
| `confirmed` | 17 | L1–L4, L7, L8, L11, L12, L14–L18, L21, L23, L24, L25 |
| `assumption-accepted` (counts toward green, reported apart) | 0 | — |
| `refuted` → re-dispositioned `extract` | 7 | L5, L6, L9, L10, L13, L19, L26 |
| `unverifiable` (un-accepted) | 2 | L20, L22 |
| missing verdict | 0 | — |

A gate is *trustworthy-green* only when all 26 rows are `confirmed` or
`assumption-accepted`, every verdict reconciles against the ledger, and no row is
an unresolved `extract` (R15). 7 unresolved `extract` rows + 2 un-accepted
`unverifiable` rows ⇒ NOT-GREEN.

## All rows

| ID | Area | Class | Verdict | Bar | Evidence pointer |
|---|---|---|---|---|---|
| L1 | Maintained SDK architecture | knowledge | confirmed | completeness (distilled-lesson) | docs/plans/maintained-sdk-migration-plan.md |
| L2 | Auth/config/runtime transport lessons | behavioral | confirmed | `cargo test -p ls-core` (93 passed) | inline |
| L3 | Paper/production environment finding | knowledge | confirmed | completeness (distilled-lesson) | docs/design/release-readiness-and-residual-lessons.md |
| L4 | Runtime success predicate | behavioral | confirmed | ls-core inner tests (9 passed) | crates/ls-core/src/inner.rs |
| L5 | Full response-code taxonomy | knowledge | **refuted → extract** | completeness (full-transcription) | docs/design/ls-gateway-response-semantics.md |
| L6 | Order acknowledgement codes | knowledge | **refuted → extract** | completeness (summary+snapshot) | inline |
| L7 | Error enum public surface | knowledge | confirmed | completeness (distilled-lesson) | inline |
| L8 | Retry and rate-limit behavior | behavioral | confirmed | ls-core retry/rate-limit tests (4 passed) | crates/ls-core/src/inner.rs |
| L9 | Order safety runtime | knowledge | **refuted → extract** | completeness (distilled-lesson) | docs/design/order-safety-design.md |
| L10 | Order dedup eviction lesson | knowledge | **refuted → extract** | completeness (distilled-lesson) | docs/design/order-safety-design.md |
| L11 | Full 365-TR dependency inventory | knowledge | confirmed | completeness (summary+snapshot) | docs/migration-source/tr-dependencies-2026-06-14.json |
| L12 | Strong order-number coupling matrix | knowledge | confirmed | completeness (summary+snapshot) | docs/migration-source/tr-dependencies-2026-06-14.json |
| L13 | SELF pagination population | knowledge | **refuted → extract** | completeness (summary+snapshot) | inline |
| L14 | Trading-day sensitivity knowledge | knowledge | confirmed | completeness (summary+snapshot) | docs/migration-source/tr-dependencies-2026-06-14.json |
| L15 | Paper-incompatible TR set | knowledge | confirmed | completeness (summary+snapshot) | docs/design/tr-dependency-inventory-snapshot.md |
| L16 | WebSocket lifecycle runtime | behavioral | confirmed¹ | passing live smoke (lifecycle only) | inline |
| L17 | Broad WebSocket certification findings | knowledge | confirmed | completeness (full-transcription) | docs/design/websocket-certification-findings.md |
| L18 | WebSocket backpressure policy | knowledge | confirmed | completeness (distilled-lesson) | inline |
| L19 | Diagnostics/redaction contract | knowledge | **refuted → extract** | completeness (full-transcription) | docs/operations/operator-diagnostics.md |
| L20 | Operations runbook | knowledge | **unverifiable** | completeness (summary+snapshot) | inline |
| L21 | Full certification/evidence pipeline | discard | confirmed | presence-and-coherence | docs/migration-source-extraction-ledger.md |
| L22 | Evidence buckets and residual lane | knowledge | **unverifiable** | completeness (summary+snapshot) | docs/design/release-readiness-and-residual-lessons.md |
| L23 | Convenience API smoke interface | knowledge | confirmed | completeness (distilled-lesson) | docs/design/release-readiness-and-residual-lessons.md |
| L24 | Spec drift and fetch scripts | knowledge | confirmed | completeness (distilled-lesson) | crates/ls-trackers/src/api_drift.rs |
| L25 | Old generated Rust API surface | discard | confirmed | presence-and-coherence | docs/migration-source-extraction-ledger.md |
| L26 | Old release checklists and production-readiness docs | knowledge | **refuted → extract** | completeness (summary+snapshot) | inline |

¹ **L16** confirms the WebSocket **lifecycle** only (paper smoke passed: connect/
subscribe/unsubscribe reachable, port `29443`). Its sub-claims — reconnect replay,
terminal exhaustion, latest-only wakeup — are recorded `unverifiable` inside the
record and are **not** folded into the confirm (the live smoke is
lifecycle-reachability only).

## Refuted rows (re-dispositioned to `extract`)

Each row's ledger disposition was changed `carried` → `extract`; each re-blocks
the gate until separately re-extracted (out of this audit's scope).

- **L5 — Full response-code taxonomy.** The old taxonomy's `904` (market closed)
  code is absent from every maintained doc; the target's framing would silently
  reclassify it as a hard failure. (full-transcription: a distinct code/meaning is
  a material loss.)
- **L6 — Order acknowledgement codes.** Targets preserve only `00039`/`00040`/`01427`;
  the modify (`00462`), cancel (`00463`/`00156`), and the `03181` modify-rejection
  codes are missing — numeric response codes a future order success predicate
  would need.
- **L9 — Order safety runtime.** The order redaction / tracing-span contract
  (which span fields are recorded vs the credential field-names never recorded;
  non-auto-redaction of account-level order responses) and the 7-day
  reconciliation-evidence freshness window are unrepresented in the target.
- **L10 — Order dedup eviction lesson.** The target omits the "sweep holds **no**
  DashMap entry guard" deadlock-avoidance rule — a decision-relevant concurrency
  constraint.
- **L13 — SELF pagination population.** The 61-TR `cts_*` snapshot is exact, but
  the caveat that this is a *distinct* population from the ~246-TR
  header-continuation set (which excludes orders) is missing — a conflation risk.
- **L19 — Diagnostics/redaction contract.** Under full-transcription, the target
  collapses the compatibility policy (field removal/rename callout, additive-minor
  rule, deprecation/alias rules), the field-meaning-change definition + examples,
  and the Debug-redaction enforcement table to a single sentence.
- **L26 — Old release checklists.** The concrete 7-day live-evidence
  freshness-before-tagging threshold (a numeric release-decision constraint) is
  carried nowhere in the target and is not among the deliberately-not-carried items.

## Unverifiable rows (un-accepted — excluded from green until accepted, R4a)

- **L20 — Operations runbook.** Several numeric operational defaults (rate quotas
  5/3/1/1, reconnect budget 4 with 1s/2s/3s/4s backoff, channel capacity 64,
  up-to-3 non-order retry, 5-minute token-refresh threshold) are absent from
  L20's single bounded target; whether their preservation is owned by sibling rows
  (L8/L16/L18) cannot be adjudicated from L20 alone, so completeness cannot be
  established (not folded into a confirm).
- **L22 — Evidence buckets and residual lane.** The closed `error_class` match-key
  taxonomy `{tr_code, rsp_cd, field_id, error_class}` is missing from the target,
  and the row's summary-plus-snapshot source scope is itself under-enumerated (no
  pinned snapshot of the exact carried-fact set), so completeness cannot be
  established either way.

These two await a named maintainer's explicit acceptance (with a reason naming the
residual risk) to become `assumption-accepted` and count toward green; the auditor
never self-accepts.

## Assumption-accepted rows

_None._ No `unverifiable` row has been accepted yet.

## Source-coverage reconciliation

All 26 rows were audited claim-by-claim against the old-source documents named in
their manifest entries; every confirmed knowledge row's claim-map maps each
transcribed claim to a present/adapted in-repo location, and the committed
validator enforces that mapping. The material gaps found are enumerated above (the
7 refutals + 2 unverifiable). Note the audit's standing limitation (Risks): a
claim-map bounds completeness to what each agent *enumerated*; the highest-risk
rows already surfaced concrete misses (order codes L6, response codes L5,
redaction L9/L19), which is the strongest available signal that the enumeration
was not merely rubber-stamped.
