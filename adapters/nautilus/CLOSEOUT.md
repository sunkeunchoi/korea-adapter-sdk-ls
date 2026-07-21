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
| `nautilus-ls-calendar` schema | `1.0.0` |
| Adapter workspace | `0.1.0` |
| Calendar migration plan | issue #189 |

## Calendar Foundation Gate (offline)

| Gate | Verdict |
|------|---------|
| `make foundation-gate` (core, refresh, activation, diagnostics, fixtures, six consumers, composition-root, failure-inversion, traceability, rollback rehearsal) | `PASS` |
| `make adapter-check` (standalone workspace, offline) | `PASS` |
| Traceability drift check | `PASS` |
| Closeout publication-boundary scan | `PASS` |

## Consumer Retirement Gates (live, operator-attended)

One row per gate. Record only the PASS/HOLD verdict — the owner-local canary facts, snapshot
identities, and affected dates that justify it stay in the operator's local gate log.

| Consumer Retirement Gate | Verdict |
|--------------------------|---------|
| Ingest (accumulate/probe + checkpoint + backward-widen) | `PASS` |
| Catalog readiness | `PASS` |
| Budget-probe automatic selection | `PASS` |
| Production Ladder date-fact gate | `PASS` |

## Retirement completion

| Milestone | Verdict |
|-----------|---------|
| Shared adoption scaffold removed (Enforced-only) | `PASS` |
| README + `CONCEPTS.md` reflect completed retirement | `PASS` |
| Final offline gate green | `PASS` |
