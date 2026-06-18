---
title: "feat: Promote smoke-ready implemented TRs to recommended"
date: 2026-06-17
type: feat
origin: docs/brainstorms/2026-06-17-harden-implemented-trs-requirements.md
---

# feat: Promote smoke-ready implemented TRs to recommended

## Summary

Promote `t1102`, `t8412`, `CSPAQ12200`, and `S3_` from `support:implemented` to
`support:recommended` by capturing Focused Evidence from their existing Paper Live Smokes.
One `queue:maintenance` work item; each TR promotes independently as its smoke passes, via
a repeatable recipe proven on `t1101`. One small test-harness fix first makes the `chart`
and `account` smoke lines credential-free. `revoke` is out of scope.

## Problem Frame

Of the eight maintained TRs, only `token` and `t1101` are `recommended`; five implemented
TRs carry the "not yet recommended" banner for lack of recorded Focused Evidence. Four
already have working smoke harnesses (`live-smoke`, `live-smoke-chart`, `live-smoke-account`,
`live-smoke-ws`), so the gap is recorded evidence, not capability. This raises user-facing
recommendation coverage through the cheapest repeatable loop the project has.

---

## Key Technical Decisions

- **Per-TR promotion recipe, one commit each.** Each promotion writes
  `metadata/evidence/<tr>.yaml` (verbatim credential-free `LIVE-SMOKE` line, `date` ==
  `last_reviewed`), flips `support.recommended: true` with a `recommendation` block, regen
  docs, and — in the same commit — drops that TR from the docgen banner list and bumps the
  `EVIDENCE-FRESHNESS.md` recommended count, so `cargo test` + `docs-check` stay green per
  TR (mirrors the `t1101` promotion, see origin and `metadata/trs/t1101.yaml`).
- **Credential-free evidence requires a harness fix first.** The `chart` and `account`
  smokes print `rsp_msg` in their `record()` line; `rsp_msg` can carry localized text and is
  excluded from the `token`/`t1101` evidence pattern. Tighten those two lines to a benign
  descriptor (drop `rsp_msg`) so verbatim capture stays credential-free. Test-harness only,
  no SDK runtime change.
- **`S3_`'s claim is scoped to lifecycle reachability.** The ws smoke proves
  connect/subscribe/unsubscribe, not trade-data correctness, so its `recommendation`
  `excludes` trade-data correctness and any in-session row guarantee.
- **Partial completion is first-class.** A TR whose gate can't be met this session stays
  `implemented`; its unit is left undone and recorded, never forced.

---

## Requirements Traceability

- R1 (credential-free evidence) → U1 (harness fix), U2–U5 (each evidence file)
- R2 (recommended + recommendation block) → U2–U5
- R3 (docs regen, banner drop, docs-check) → U2–U5
- R4 (passing-smoke gate, partial completion) → U2–U5, U6
- R5 (queue item + labels) → U6
- R6 (downstream count/banner updates) → U2–U5 (incremental), U6 (final reconcile)

---

## Implementation Units

### U1. Make chart/account smoke lines credential-free

- **Goal:** Drop `rsp_msg` from the `chart` and `account` smoke evidence lines so a verbatim
  capture is credential-free, matching the `token`/`t1101` evidence shape.
- **Requirements:** R1.
- **Dependencies:** none.
- **Files:** `crates/ls-sdk/tests/live_smoke.rs` (modify `live_smoke_chart`,
  `live_smoke_account` `record()` calls).
- **Approach:** Replace `rsp_msg={}` in the two `record()` result strings with a benign
  structural descriptor already present (chart keeps `rsp_cd` + `rows`; account keeps
  `rsp_cd` + `reccnt`). Also harden the account `Err`-path, which currently prints a
  `record()` line with the raw gateway error before panicking: that line could pattern-match
  the evidence-capture recipe and carry account-identifying text, so give it a distinct
  non-`LIVE-SMOKE` prefix (or drop the `record()` call) — a failed run must not produce a
  capturable line; the panic is unchanged. Do not touch `live_smoke_default` or
  `live_smoke_ws` (already `rsp_msg`-free).
- **Patterns to follow:** `record()` contract and the secret-safety note in
  `crates/ls-sdk/tests/live_smoke.rs`; `metadata/evidence/token.yaml`.
- **Test scenarios:** `cargo test -p ls-sdk --test live_smoke` compiles and the two
  non-ignored tests still pass. `Test expectation: behavioral output only` — the smoke
  bodies are `#[ignore]`; this changes only the printed descriptor, verified by reading the
  emitted line during U3/U4 runs.
- **Verification:** The `chart` and `account` `LIVE-SMOKE` lines contain no `rsp_msg`.

### U2. Promote t1102 to recommended

- **Goal:** Record Focused Evidence and recommend `t1102` (current-price quote).
- **Requirements:** R1, R2, R3, R4, R6.
- **Dependencies:** none (the default smoke line is already credential-free).
- **Files:** `metadata/evidence/t1102.yaml` (create), `metadata/trs/t1102.yaml` (modify),
  `crates/ls-docgen/src/lib.rs` (drop `t1102` from `banner_trs`),
  `metadata/EVIDENCE-FRESHNESS.md` (recommended count), regenerated `docs/reference/t1102.md`
  + `docs/reference/index.md` + `docs/tr-dependencies/t1102.md`.
- **Approach:** Run `make live-smoke` during an open session; capture the `LIVE-SMOKE` line
  verbatim into `metadata/evidence/t1102.yaml` (mirror `metadata/evidence/t1101.yaml`). Set
  `recommended: true`, `last_reviewed` == evidence date, add a `recommendation` block
  (behavior: paper current-price quote; `evidence_ref: evidence/t1102.yaml`; excludes
  production creds, fields outside the modeled subset). `make docs`; move `t1102` out of the
  docgen `banner_trs` list to the recommended-no-banner assertion.
- **Patterns to follow:** the `t1101` promotion (`metadata/trs/t1101.yaml`,
  `metadata/evidence/t1101.yaml`, the `banner_trs` edit in `crates/ls-docgen/src/lib.rs`).
- **Test scenarios:**
  - `Covers R2, R3.` `cargo test` workspace green; `t1102` reference page has no banner;
    `docs-check` passes.
  - The evidence `date` equals `maintenance.last_reviewed` (validator cross-check).
  - `cargo test -p ls-core` (the cross-check test re-validates the real metadata, including
    the new recommendation block resolving its `evidence_ref`).
- **Verification:** `t1102` is `recommended` with a resolving evidence file; suite green.

### U3. Promote t8412 to recommended

- **Goal:** Record Focused Evidence and recommend `t8412` (paginated chart).
- **Requirements:** R1, R2, R3, R4, R6.
- **Dependencies:** U1 (credential-free chart line).
- **Files:** `metadata/evidence/t8412.yaml` (create), `metadata/trs/t8412.yaml` (modify),
  `crates/ls-docgen/src/lib.rs`, `metadata/EVIDENCE-FRESHNESS.md`, regenerated `t8412` docs.
- **Approach:** Same recipe as U2. Gate: `make live-smoke-chart
  LS_LIVE_SMOKE_T8412_DATE=<real trading day>` during an open session; if no trading day is
  available, hold the unit (R4). `recommendation` behavior: paper N-minute chart page;
  excludes multi-page `chart_all` correctness beyond a single page.
- **Patterns to follow:** U2; `metadata/trs/t8412.yaml` current facets (`self_paginated`,
  `date_sensitive`).
- **Test scenarios:**
  - `Covers R2, R3.` Workspace green; `t8412` banner gone; `docs-check` passes.
  - `Covers R4.` If the chart smoke can't run (no trading day), `t8412` stays `implemented`
    and the unit is recorded incomplete — not promoted without evidence.
- **Verification:** `t8412` recommended with resolving evidence, or held with the unmet gate
  recorded.

### U4. Promote CSPAQ12200 to recommended

- **Goal:** Record Focused Evidence and recommend `CSPAQ12200` (account balance).
- **Requirements:** R1, R2, R3, R4, R6.
- **Dependencies:** U1 (credential-free account line).
- **Files:** `metadata/evidence/CSPAQ12200.yaml` (create), `metadata/trs/CSPAQ12200.yaml`
  (modify), `crates/ls-docgen/src/lib.rs`, `metadata/EVIDENCE-FRESHNESS.md`, regenerated
  `CSPAQ12200` docs.
- **Approach:** Same recipe. Gate: `make live-smoke-account` needs a provisioned paper
  account; if unprovisioned the smoke fails on account-state and the TR stays `implemented`
  (R4) with provisioning logged as follow-up. `recommendation` behavior: paper read-only
  balance inquiry; excludes order/position-mutating account state.
- **Patterns to follow:** U2; `CSPAQ12200`'s account-state handling note in
  `crates/ls-sdk/src/account/mod.rs`.
- **Test scenarios:**
  - `Covers R2, R3.` Workspace green; banner gone; `docs-check` passes.
  - `Covers R4.` Account-state failure (unprovisioned) → held at `implemented`, recorded,
    not promoted.
- **Verification:** `CSPAQ12200` recommended with resolving evidence, or held with the
  provisioning gate recorded.

### U5. Promote S3_ to recommended (lifecycle-scoped)

- **Goal:** Record Focused Evidence and recommend `S3_` (realtime trade feed), scoped to
  websocket lifecycle reachability.
- **Requirements:** R1, R2, R3, R4, R6.
- **Dependencies:** none (ws line is already credential-free).
- **Files:** `metadata/evidence/S3_.yaml` (create), `metadata/trs/S3_.yaml` (modify),
  `crates/ls-docgen/src/lib.rs`, `metadata/EVIDENCE-FRESHNESS.md`, regenerated `S3_` docs.
- **Approach:** Same recipe. Gate: `make live-smoke-ws` (connect/subscribe/unsubscribe;
  a received row is bonus, not required). `recommendation` behavior: paper websocket
  subscribe lifecycle reachability on the paper port; **excludes** trade-data correctness,
  in-session row delivery guarantees, and reconnection semantics. This narrower claim is the
  KTD; do not overstate it as live-data recommendation.
- **Patterns to follow:** U2; the lifecycle assertion in `live_smoke_ws`.
- **Test scenarios:**
  - `Covers R2, R3.` Workspace green; `S3_` banner gone; `docs-check` passes.
  - The `recommendation.excludes` explicitly disclaims trade-data correctness.
- **Verification:** `S3_` recommended with a lifecycle-scoped recommendation + resolving
  evidence, or held with the unmet gate recorded.

### U6. Queue work-item record and final reconciliation

- **Goal:** Track the work as a `queue:maintenance` item and reconcile the final recommended
  set.
- **Requirements:** R5, R4, R6.
- **Dependencies:** U2–U5 (whichever promoted).
- **Files:** none in-repo (GitHub issue via `.github/ISSUE_TEMPLATE/sdk_work_item.yml`);
  final consistency pass over `metadata/EVIDENCE-FRESHNESS.md`.
- **Approach:** Open the issue labelled `queue:maintenance`, `source:manual`, the promoted
  `class:*` set, `support:recommended`, `gate:change-scoped`, `evidence:needed` (baseline
  not needed). Record per-TR outcome: promoted (with evidence path) or held (with unmet
  gate). Confirm `EVIDENCE-FRESHNESS.md` states the final recommended-TR count.
- **Test scenarios:** `Test expectation: none -- process/queue record, no code.`
- **Verification:** Issue completion checklist reflects each TR's promoted/held state; the
  freshness doc count matches the recommended set.

---

## Change-Scoped Gate

Per promotion and at completion: `cargo test` (workspace), `cargo test -p ls-core` (metadata
re-validation + policy cross-check), `make docs-check`. The live smokes
(`make live-smoke[-chart|-account|-ws]`) are the session-bound Focused Evidence step per TR.

---

## Scope Boundaries

- Coverage expansion (new TRs) and orders (`CSPAT00601`) — out of scope (see origin).

### Deferred to Follow-Up Work

- `revoke` promotion — needs a new smoke harness and destructive-ordering care (revoke
  invalidates the session token).
- The evidence-freshness evaluator (90-day backstop / change-driven invalidation) — still
  deferred (see origin and `metadata/EVIDENCE-FRESHNESS.md`).
- Paper-account provisioning for `CSPAQ12200`, if U4 is held on the account-state gate.
- Any TR held this session re-runs its smoke when its gate opens (trading day / session /
  provisioning).

---

## Risks & Dependencies

- **Session/gate availability.** Most units are session-bound; `t8412` needs a trading day,
  `CSPAQ12200` needs a provisioned account. Mitigation: partial completion (R4) — promote
  what's reachable, record the rest.
- **Overstating `S3_`.** A lifecycle-only smoke must not become a live-data recommendation.
  Mitigation: the scoped `recommendation.excludes` (U5 KTD).
- **Evidence secret-safety.** Mitigated by U1 (credential-free lines) before any chart/account
  evidence is committed.

---

## Sources & Research

- Proven pattern: `docs/plans/2026-06-17-001-feat-t1101-stage2-expansion-plan.md`,
  `metadata/evidence/t1101.yaml`, `metadata/trs/t1101.yaml` (recommendation block),
  `metadata/trs/token.yaml`.
- Smokes: `crates/ls-sdk/tests/live_smoke.rs`, `Makefile` (`live-smoke*`).
- Banner/count assertions: `crates/ls-docgen/src/lib.rs`; freshness:
  `metadata/EVIDENCE-FRESHNESS.md`; validator rules: `crates/ls-metadata/src/validator.rs`.
- Queue contract: `docs/MAINTENANCE_RUNBOOK.md`, `docs/maintenance-labels.md`,
  `.github/ISSUE_TEMPLATE/sdk_work_item.yml`.
