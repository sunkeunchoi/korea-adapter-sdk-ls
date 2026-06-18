---
name: audit-carried-rows
description: Autonomously audit every carried and discard row of the migration-source extraction ledger before the decommission gate is trusted — dispatch one fresh decommission-row-auditor per row (knowledge/discard rows in parallel, behavioral rows throttled), collect a verdict per row into a resumable sweep ledger, then run the serial roll-up that re-dispositions refuted rows, escalates unverifiable rows for maintainer acceptance, writes the roll-up report, and runs the committed gate validator. Use when the user says "audit the carried rows", "run the decommission audit", or "verify the ledger before decommission". Read-only verification; non-interactive and state-driven.
---

Autonomous end-to-end decommission audit. Verifies every `carried` (24) and
`discard` (2) ledger row this session against a bar tiered by row type, freezes a
re-checkable per-row record, and produces an explicit trustworthy-green gate
verdict. Read-only verification — it never promotes TRs, regenerates docs, or
flips recommendations. Partial completion is first-class (resumable). Runs
non-interactively — everything is inferred from the manifest and the per-row
results.

**Prerequisite for behavioral rows:** a live session with paper credentials in
`.env` (`LS_TRADING_ENV=paper`) and a reachable paper gateway, since a behavioral
row's bar is a real test/smoke. With no credentials, the one live-gateway
behavioral row (WebSocket lifecycle, `live-smoke-ws`) records `unverifiable` and
routes to R4a acceptance — that is correct behavior, not a failure. Knowledge and
discard rows (the large majority) need only read access to both repos.

## 1. Discover candidates

Read `docs/migration-source/audit/manifest.yaml`. The dispatch list is its 26
rows (`L1`…`L26`), each with `id`, `area`, `disposition`, `candidate_class`, and
pointers. Partition for dispatch:
- **parallel cohort** — every `knowledge` and `discard` candidate: each only
  reads the two repos and writes its own `records/<id>.yaml`, so there is no
  shared-file collision (unlike `promote-trs`, forced serial by shared docgen /
  freshness files).
- **throttled cohort** — every `behavioral` candidate: their `cargo test` / `make`
  smoke targets contend on the cargo build lock, one shared `.env`, and the live
  paper gateway. Run these **serially** (or a small concurrency bound ≤2).

`candidate_class` only seeds dispatch routing; the auditor records its own class
(R5). If the manifest does not enumerate exactly 26 rows reconciling with the
ledger, stop and report — do not dispatch a partial manifest.

## 2. Resume / setup

On the default branch, create a feature branch
`feat/audit-carried-rows-<YYYYMMDD>` from an up-to-date default. If already on a
feature branch, continue on it.

Each sweep branch owns one uncommitted **sweep ledger** under:

```
.compound-engineering/runs/audit-carried-rows/<branch-slug>/
  state/progress.json        # phase + branch + default-branch + timestamp
  state/candidates.json      # manifest snapshot at sweep start
  state/outcomes.jsonl       # one object per returned row (ORCHESTRATOR-written only)
  logs/orchestrator.jsonl    # operational log
```

The ledger is operational scratch state only, git-ignored
(`.compound-engineering/runs/` is in `.gitignore`), so a live-run outcome line or
log entry can never be committed. It is never authoritative for the gate; the
committed records, the report, the re-dispositioned ledger, and the validator
test are the durable records.

This fleet's phases (no `current_tr` analog — dispatch is parallel):

```
discovering
dispatching
rolling_up
gate_computed
complete
```

On start:
- If the current branch has no active ledger, create one with
  `phase: "discovering"`, the current + default branch, and a timestamp; snapshot
  the manifest to `candidates.json`.
- If the current branch has exactly one active ledger, resume it only when
  `progress.branch` matches the current branch and the working tree is clean (or
  dirty only with files expected for the recorded phase). On resume, **skip rows
  already in `outcomes.jsonl`** and re-read each recorded `records/<id>.yaml` to
  confirm it still exists; dispatch only the remainder.
- If multiple active ledgers exist, or the active ledger disagrees with the
  branch/repo state, **stop** and report `HELD sweep — active ledger does not
  match repo state: <reason>`.
- If records exist but no ledger exists, reconstruct only objective facts from
  the records on disk (which rows have a `records/<id>.yaml`) and record a
  reconciliation event; rediscover the rest.

## 3. Dispatch one auditor per row

Set `phase: "dispatching"`. For each row, dispatch the **`decommission-row-auditor`**
subagent (fresh context per row, R9) with the row ID. Each subagent runs the
`audit-row` recipe and returns its final line: `AUDITED <id> <verdict>
records/<id>.yaml` or `HELD <id> — <reason>`.

- Dispatch the **parallel cohort** concurrently (knowledge + discard).
- Dispatch the **throttled cohort** serially (or ≤2 at once) after/around the
  parallel cohort — never let behavioral runs collide on the build lock / `.env`
  / gateway.

The orchestrator — **never an agent** — appends each returned outcome to
`outcomes.jsonl`, so the ledger has no parallel-write hazard, e.g.:

```json
{"id":"L16","verdict":"unverifiable","record":"records/L16.yaml","reason":"no reachable paper WS gateway this session"}
```

After each subagent returns: confirm `records/<id>.yaml` exists and parses; append
the outcome; on `HELD`, record the row as not-yet-audited (it re-runs on resume).
Do not let one held/failed row stop the sweep. The roll-up runs only after every
dispatched row has returned (or is recorded as held).

## 4. Roll-up (serial) — reconcile, re-disposition, accept

Set `phase: "rolling_up"`. This phase runs serially after all agents return:

1. **Reconcile coverage.** Assert all 26 rows have a `records/<id>.yaml`. A
   missing record is a blocking `missing verdict` row (R15) — never a pass.
2. **Re-disposition refuted rows (R3).** For each record with `verdict: refuted`,
   edit `docs/migration-source-extraction-ledger.md` to change that row's
   `Current disposition` cell to the record's `re_disposition`
   (`extract`/`defer`/`discard`). An `extract` re-blocks the gate. Keep the
   row's stable ID.
3. **Record unverifiable assumptions (R4).** Each `unverifiable` row stands with
   its blocking reason and does not count toward green.
4. **Maintainer acceptance (R4a / AE6).** Escalate each `unverifiable` row for
   maintainer acceptance. When a named maintainer records an explicit acceptance
   and a reason naming the specific residual risk, edit that record to
   `verdict: assumption-accepted` and **add** an `acceptance:` block
   (`accepted_by` / `acceptance_reason` / `accepted_date`) while **keeping** the
   existing `unverifiable_reason` (the validator requires both on an accepted
   record — the reason it was unverifiable plus who accepted that residual risk).
   `assumption-accepted` counts toward green; un-accepted `unverifiable` does
   not. Acceptance is a deliberate maintainer act — never auto-applied by the
   sweep.

## 5. Compute the gate and write the report + run the validator

Set `phase: "gate_computed"`.

1. Write `docs/migration-source/audit/decommission-audit-report.md` per the
   report format in `references/record-format.md`: the explicit gate verdict
   line, counts (with `assumption-accepted` reported **apart from** `confirmed`),
   the all-26-rows table, and the refuted / unverifiable / accepted /
   source-coverage lists.
2. Run the committed validator: `cargo test -p ls-trackers` (includes
   `decommission_audit.rs`). It recomputes the gate from the frozen records —
   pointer integrity, inline transcription, verdict-vs-ledger reconciliation,
   credential scan, source coverage, manifest↔ledger ID reconciliation. If it is
   red, the gate is not trustworthy — fix or surface the failing rows; never
   declare green over a red validator.

The gate is **trustworthy-green** only when all 26 rows are `confirmed` or
`assumption-accepted`, every verdict reconciles against the ledger, and no row is
an unresolved `extract` (R15).

## 6. Commit and report

Set `phase: "complete"`. Commit the durable artifacts (the 26 records, the
report, any ledger re-dispositions, the validator) — never the git-ignored sweep
ledger. Summarize: confirmed / accepted / refuted (with re-disposition) /
unverifiable (un-accepted) / missing counts, the explicit gate verdict, and any
under-enumerated old-source documents. The trustworthy-green report line is the
documented precondition the (out-of-scope) physical-decommission work item must
cite — decommission must not proceed while any row is `unverifiable`/un-accepted.

## Reference

- Recipe: `.agents/skills/audit-row/SKILL.md` (the per-row unit).
- Subagent: `decommission-row-auditor` (`.claude/agents/decommission-row-auditor.md`).
- Manifest: `docs/migration-source/audit/manifest.yaml` (the dispatch input).
- Record + report schema: `references/record-format.md`; worked example:
  `references/record-format.example.yaml`.
- Committed gate validator: `crates/ls-trackers/tests/decommission_audit.rs`.
- Precedent: `.agents/skills/promote-trs/SKILL.md` (sweep ledger, phase state,
  resume validation), which this models but dispatches in parallel for the
  non-colliding cohort.
