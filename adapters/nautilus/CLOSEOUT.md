# Calendar Migration Closeout — verdict-only

The public closeout record for the "certify, enforce, and retire KRX weekday-era behavior"
migration (issue #189). It is **verdict-only** (R17, KTD9): it carries gate names, pass/hold
verdicts, and software/schema versions — and NOTHING ELSE. Owner-local canary facts, snapshot
identities (`artifact_id`/`calendar_id`/fingerprint hashes), and affected real (KRX-derived)
dates live only in the operator's local gate log, never here.

A machine scan (`nautilus-ls-calendar/tests/closeout_scan.rs`, part of `make foundation-gate`
and `make adapter-check`) fails the gate if this file contains a snapshot-identity hash or an
ISO calendar date — the publication boundary is enforced, not merely reviewed. Record verdicts
and versions only; keep dates and identities in the owner-local log.

## Software / schema versions

| Component | Version |
|-----------|---------|
| `nautilus-ls-calendar` schema | `<schema-version>` |
| Adapter workspace | `<adapter-version>` |
| Calendar migration plan | issue #189 |

## Calendar Foundation Gate (offline)

| Gate | Verdict |
|------|---------|
| `make foundation-gate` (core, refresh, activation, diagnostics, fixtures, six consumers, composition-root, failure-inversion, traceability, rollback rehearsal, divergence classification) | `<PASS / HOLD>` |
| `make adapter-check` (standalone workspace, offline) | `<PASS / HOLD>` |
| Traceability drift check | `<PASS / HOLD>` |
| Closeout publication-boundary scan | `<PASS / HOLD>` |

## Consumer Retirement Gates (live, operator-attended)

One row per gate. Record only the PASS/HOLD verdict — the owner-local canary facts, snapshot
identities, and affected dates that justify it stay in the operator's local gate log.

| Consumer Retirement Gate | Verdict |
|--------------------------|---------|
| Ingest (accumulate/probe + checkpoint + backward-widen) | `<PASS / HOLD>` |
| Catalog readiness | `<PASS / HOLD>` |
| Budget-probe automatic selection | `<PASS / HOLD>` |
| Production Ladder date-fact gate | `<PASS / HOLD>` |

## Retirement completion

| Milestone | Verdict |
|-----------|---------|
| Shared adoption scaffold removed (Enforced-only) | `<PASS / HOLD>` |
| README + `CONCEPTS.md` reflect completed retirement | `<PASS / HOLD>` |
| Final offline gate green | `<PASS / HOLD>` |
