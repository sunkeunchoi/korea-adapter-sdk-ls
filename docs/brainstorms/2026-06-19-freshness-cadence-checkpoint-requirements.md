---
date: 2026-06-19
topic: freshness-cadence-checkpoint
---

# Scheduled Freshness Cadence — Requirements

## Summary

Give evidence freshness a cadence that does not depend on anyone remembering: a
scheduled, non-gating GitHub Action — the repo's first automation — runs
`make freshness-check` monthly and maintains a single rolling "Evidence freshness
status" issue (opened/updated when Recommended TRs are stale, cleared when none
are). The check touches no LS API and needs no LS credentials. The schedule *is*
the trigger, and the watcher's own failure is surfaced rather than silent. The
network-touching `api-drift` check stays operator-run at the runbook checkpoint
(R19 intact); `spec-doc` (also network-free) stays operator-run this increment, a
candidate for the same treatment later. The operator-run checks keep a manual
sweep at the checkpoint; freshness is also runnable standalone. The maintainer
escalates accepted staleness into the curated Maintenance Work Queue.

## Problem Frame

PR #21 made the 90-day backstop enforced *when run*, but `make freshness-check`
is operator-invoked, so a recommendation only surfaces as stale if a maintainer
remembers to run it. A documented operator checkpoint alone would only relocate
that dependency — "forgot to run the check" becomes "forgot to run the monthly
sweep," the same failure mode at a coarser grain. The control would stay
fundamentally memory-dependent, and its own lapse would be invisible.

The freshness check is **network-free and advisory**, which makes it the one
check that can be put on a timer without cost: a scheduled run touches no LS API
(honors the network-free-CI posture, ADR 0009) and never gates (honors the
advisory posture from PR #21). The standing R19 decision — "no cron/CI
scheduling; operator-run" — is **scoped to the network-touching `api-drift`
targets** (its rationale is not putting a live LS fetch on a timer), not a blanket
prohibition; the runbook itself anticipates "when a scheduled review is
introduced." So scheduling freshness operates outside R19's intent rather than
overturning it, and it is the only option that actually removes the memory
dependency.

## Key Decisions

- **Schedule replaces memory.** A recurring scheduled run is the freshness
  trigger, not operator habit. This is the repo's first automation under
  `.github/workflows/`.

- **R19 is clarified, not overturned.** R19 governs the network-touching
  `api-drift` check (no live fetch on a timer). A network-free, non-gating
  freshness run is outside its scope. `api-drift` stays operator-run under R19;
  `spec-doc`, though also network-free, stays operator-run this increment — a
  candidate for the same scheduled treatment later, deferred to keep scope tight,
  not blocked by R19.

- **Freshness is unbundled from api-drift.** Opposite cost profiles get different
  mechanisms: freshness (cheap, network-free) is scheduled; api-drift (live fetch,
  can gate) stays a manual checkpoint step. A manual aggregate sweep still exists
  for full local runs and keeps freshness in it for offline convenience.

- **Advisory, non-gating — unchanged.** `Severity::Evidence` stays below
  `Maintenance`; the scheduled run never fails its job on stale evidence. A job
  failure means a tooling/build error, not stale evidence.

- **Rolling status issue, not auto-filed work items.** The Action maintains one
  dedicated "Evidence freshness status" issue, distinct from the human-curated
  Maintenance Work Queue (ADR 0013). It never auto-files `SDK work item` issues —
  the maintainer reviews the rolling issue and escalates accepted items through
  the normal reviewed flow. This keeps the Work Queue clean and avoids per-run spam.

- **Credential-free.** The workflow needs only `GITHUB_TOKEN` with `issues: write`;
  no LS API credentials, consistent with the project's credential-free ethos.

## Requirements

**Scheduled freshness automation**

- R1. A scheduled GitHub Actions workflow runs `make freshness-check` on a recurring
  cadence (monthly — ample inside the 90-day backstop, ~60 days of lead before a
  lapse). The check touches no LS API and needs no LS credentials; the CI job itself
  compiles `ls-trackers` (fetching the Rust toolchain and crates).
- R2. The workflow is non-gating: stale evidence never fails the job. A job failure
  signals a tooling/build/infra error, not staleness.
- R3. When one or more Recommended TRs are stale, the workflow opens or updates a
  single rolling "Evidence freshness status" issue listing them; when none are
  stale, it clears that issue (close, or an "all clear" comment). The issue is
  idempotent — never duplicated across runs. **A still-stale condition resurfaces on
  every run regardless of the issue's prior state, including a manual close — the
  check never goes silent while a TR is stale** (the exact reopen-vs-recreate
  mechanism is deferred to planning; the never-silent invariant is not).
- R4. The rolling status issue is **not** an `SDK work item`. The maintainer reviews
  it and escalates accepted staleness into the Maintenance Work Queue via the normal
  human-reviewed flow (ADR 0013).
- R5. The workflow requires only `GITHUB_TOKEN` with `issues: write` — no LS API
  credentials or network access to LS.
- R9. The watcher's own liveness is protected: a scheduled-run failure (the
  tooling/build/infra class in R2, distinct from stale evidence) is itself surfaced
  to a maintainer, not silent. The design accounts for GitHub disabling a scheduled
  workflow after consecutive failures — the freshness watcher must not be able to die
  unnoticed.
- R10. New staleness produces an actual notification to a maintainer (a fresh
  comment, assignment, or mention), not only a silent in-place edit of the issue body
  — GitHub does not notify non-subscribers on body edits.

**Operator checkpoint (network-touching checks)**

- R6. `api-drift-check` (network-touching) and `spec-doc-check` (network-free) both
  stay operator-run at the runbook maintenance checkpoint this increment. Scheduling
  the live `api-drift` fetch is out of scope under R19; `spec-doc`'s deferral is a
  scope choice, not an R19 constraint (it shares freshness's network-free profile).
- R7. The manual maintenance sweep covers the operator-run checks (`api-drift-check`
  + `spec-doc-check`). Freshness is reached via the standalone `make freshness-check`
  and is **not** bundled into the sweep — its cadence guarantee is the schedule (R1).
  Whether the sweep is a new aggregate `make` target or a documented runbook checklist
  of the existing targets is a planning detail; the individual targets remain either
  way.

**Documentation**

- R8. The runbook's `Checkpoint-host gap (U7 / R19)` note is updated: freshness now
  has a scheduled trigger (its cadence gap is closed by automation), while
  `api-drift` / `spec-doc` remain operator-run at the checkpoint. The note clarifies
  that R19 is scoped to the network-touching checks.

## Acceptance Examples

- AE1. Stale → issue opened. **Covers R1, R3.** A scheduled run finds 2 Recommended
  TRs stale; it opens or updates the "Evidence freshness status" issue listing them;
  the job exits successfully.
- AE2. All fresh → issue cleared. **Covers R3.** A run finds none stale; the rolling
  issue is closed or carries an "all clear" comment; no new issue is created.
- AE3. Idempotent across runs. **Covers R3.** Two consecutive stale runs update the
  same single issue, never opening a second.
- AE4. Non-gating. **Covers R2.** Stale evidence does not fail the job; only a
  tooling/build error does.
- AE5. Escalation stays human. **Covers R4.** The rolling issue is not an `SDK work
  item`; a maintainer files one only after reviewing and accepting an item.
- AE6. Operator sweep covers the operator-run checks. **Covers R6, R7.** An operator
  runs the maintenance sweep locally; it runs `api-drift` (live fetch) and `spec-doc`;
  `freshness-check` is available standalone, not bundled into the sweep.
- AE7. Credential-free. **Covers R5.** The workflow runs with only `GITHUB_TOKEN`;
  no LS secrets are configured.
- AE8. Runbook updated. **Covers R8.** The `Checkpoint-host gap (U7 / R19)` note
  records that freshness is scheduled while `api-drift` / `spec-doc` remain
  operator-run, and clarifies R19 is scoped to the network-touching check.
- AE9. Still-stale resurfaces after a manual close. **Covers R3.** A maintainer
  closes the status issue while TRs are still stale; the next scheduled run
  resurfaces them rather than staying silent.
- AE10. The watcher's own failure is visible. **Covers R9.** A run that fails to
  build/run (not stale evidence) surfaces a failure signal to a maintainer; a watcher
  that stops succeeding does not go unnoticed.
- AE11. New staleness notifies. **Covers R10.** When a newly-stale TR appears, a
  maintainer receives an actual notification (comment / assignment / mention), not
  just a silent body edit.

## Scope Boundaries

**Deferred for later**
- Scheduling the live `api-drift` fetch — R19 stands for it (a live LS fetch on a
  timer needs separate justification: credentials, rate limits, failure handling).
- Scheduling `spec-doc` — network-free and advisory (the same profile as freshness),
  so a natural follow-up to this increment, deferred only to keep scope tight, not by
  R19.
- Change-driven (API Drift → evidence) invalidation — the next increment; shares the
  same `Severity::Evidence` surface.

**Outside this work's identity**
- Gating or failing the build on stale evidence (the advisory posture from PR #21).
- Auto-filing `SDK work item` issues from the bot — that bypasses the human-reviewed
  Maintenance Work Queue.

## Outstanding Questions

**Deferred to planning**
- Rolling-issue update mechanics: how the Action finds its existing issue (stable
  title vs. a dedicated label), whether the cleared state closes the issue or leaves
  it open with an "all clear" comment, and the reopen-vs-recreate mechanism that
  satisfies the R3 never-silent-while-stale invariant after a prior manual close.
- The mechanisms for R9 (surfacing a scheduled-run *failure* — a failure
  notification, a heartbeat, or periodic confirmation the workflow is still enabled,
  given GitHub's auto-disable of consistently-failing scheduled workflows) and R10
  (the notification channel for new staleness — fresh comment vs. assignment vs.
  mention).
- Whether the workflow caches the Rust toolchain/build so weekly runs stay fast and
  cheap (the check is network-free but the workflow still compiles `ls-trackers`).
- The manual aggregate's exit semantics when `api-drift` gates (exit 1) or hits a
  fetch error (exit 2) — the network-touching sweep's behavior, unchanged from the
  prior question set.

## Sources / Research

- `docs/MAINTENANCE_RUNBOOK.md` — the `Checkpoint-host gap (U7 / R19)` note ("when a
  scheduled review is introduced, fold the step in"; "no cron/CI scheduling is added
  (R19)") and the existing per-check review sections.
- `docs/plans/2026-06-15-004-feat-api-drift-real-fetch-plan.md` and
  `docs/plans/2026-06-16-002-feat-api-drift-real-fetch-plan.md` — **R19 defined**:
  "Add opt-in Makefile targets … do not wire scheduling/CI"; scoped to the
  network-touching api-drift targets; "a watcher nobody runs detects no drift."
- ADR `docs/adr/0009-*` — Rust-first tooling / network-free CI (why the scheduled run
  must stay network-free; it does, since `freshness-check` is network-free).
- ADR `docs/adr/0013-*` — GitHub Issues are the Maintenance Work Queue; the
  `SDK work item` template (`.github/ISSUE_TEMPLATE/sdk_work_item.yml`) the rolling
  status issue must stay distinct from.
- `Makefile` — `freshness-check` (network-free, advisory, exit 0 on stale),
  `api-drift-check` (network-touching, gating), `spec-doc-check` (network-free,
  advisory).
- PR #21 / `docs/plans/2026-06-19-001-feat-evidence-freshness-evaluator-plan.md` —
  the freshness evaluator this schedule surfaces; advisory/non-gating posture.
