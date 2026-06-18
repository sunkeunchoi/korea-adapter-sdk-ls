---
name: promote-trs
description: Autonomously sweep all maintained TRs and promote every implemented-but-not-recommended one that has a passing Paper Live Smoke, then reconcile and ship through merge. Use when the user says "promote all promotable TRs", "run the TR promotion sweep", or "harden implemented TRs". Discovers candidates, runs the promote-tr recipe per TR via the tr-promoter subagent (serially — promotions share files), records held TRs, files the maintenance queue issue, and ships push → PR → merge → sync. Non-interactive and state-driven.
---

Autonomous end-to-end TR promotion sweep. Promotes every reachable
`implemented && !recommended` TR this session; holds the rest with a recorded
reason (partial completion is first-class). Runs non-interactively — no questions;
everything is inferred from repo state and smoke results.

**Prerequisite:** a live session with paper credentials in `.env`
(`LS_TRADING_ENV=paper`), since the gate per TR is a real Paper Live Smoke. With
no credentials, the sweep will HOLD every candidate — that is correct behavior,
not a failure.

## 1. Discover candidates

A TR is a **recipe-candidate** iff `support.implemented: true`,
`support.recommended: false`, and its code appears in
`.agents/skills/promote-tr/references/smoke-map.md`. List candidates by reading
`metadata/trs/*.yaml`. Any `implemented && !recommended` TR *not* in the smoke map
(e.g. `revoke`) is recorded immediately as **HELD — needs ce-plan (no smoke
harness)**; do not attempt it.

Keep the discovered recipe-candidates and discovery-time holds for the sweep
ledger. Once the ledger exists, persist them to `state/candidates.json` and append
every discovery-time hold to `state/outcomes.jsonl`, for example:

```json
{"tr":"revoke","status":"held","stage":"discovery","reason":"no smoke harness; route to ce-plan"}
```

If there are zero recipe-candidates, report "sweep drained — nothing promotable"
(plus any needs-ce-plan holds) and stop before branching/shipping unless resuming
an active sweep branch.

## 2. Resume / setup

On the default branch, create a feature branch `feat/promote-trs-<YYYYMMDD>`
(`git checkout -b …` from an up-to-date default). If already on a feature branch,
continue on it.

Each sweep branch owns one uncommitted **sweep ledger** under:

```
.compound-engineering/runs/promote-trs/<branch-slug>/
  state/progress.json
  state/candidates.json
  state/outcomes.jsonl
  logs/orchestrator.jsonl
```

The ledger is operational scratch state only. It is never authoritative for TR
support status; committed metadata, evidence files, the queue issue, and the PR
are the durable records.

On start:
- If the current branch has no active ledger, create one with `phase:
  "discovering"`, the current branch, the default branch, and a timestamp.
- If the current branch has exactly one active ledger, resume it only when
  `progress.branch` matches the current branch and the working tree is clean (or
  dirty only with files expected for the recorded phase).
- If multiple active ledgers exist, or the active ledger disagrees with the
  branch/repo state, stop the sweep and report `HELD sweep — active ledger does
  not match repo state: <reason>`.
- If promotion commits exist but no ledger exists, reconstruct only objective
  facts from repo state (recommended metadata, evidence files, commit SHAs,
  current branch, `gh pr view`) and record a reconciliation event. Do not infer
  missing hold reasons; rediscover or mark them unknown.

Before creating a queue issue or PR, check `progress.json` first. If it already
records a URL, verify it still exists remotely and reuse it instead of creating a
duplicate.

## 3. Promote each candidate (serial)

Promotions all edit the same `crates/ls-docgen/src/lib.rs` banner test and
`metadata/EVIDENCE-FRESHNESS.md` count, so they **must run serially** — never in
parallel (guaranteed file collision). For each candidate, in stable order,
dispatch the **`tr-promoter`** subagent (fresh context per TR) with the TR code.
Each subagent runs the `promote-tr` recipe and returns its final line:
`PROMOTED <tr> …` or `HELD <tr> — <reason>`.

Before dispatching each candidate, update `progress.json` with `phase:
"promoting"` and `current_tr`. On resume, skip candidates already recorded as
promoted or held in `outcomes.jsonl`; for promoted records, re-read
`metadata/trs/<tr>.yaml` and `metadata/evidence/<tr>.yaml` to confirm repo truth
still agrees.

After each subagent returns:
- If `PROMOTED`: confirm the commit landed, the working tree is clean, and record
  the commit SHA plus evidence path in `outcomes.jsonl`. Do not routinely rerun
  the full workspace gate here; the per-TR recipe already gated before commit.
  Rerun only if the returned state is uncertain or the tree is unexpectedly dirty.
- If `HELD`: append the TR + stage/reason to `outcomes.jsonl`; leave it
  `implemented`; continue.

Do not let one held TR stop the sweep.

## 4. Reconcile

The per-TR recipe bumps the freshness count incrementally, so after the sweep the
count should already be correct. Verify `metadata/EVIDENCE-FRESHNESS.md` states
the final recommended-TR count and class list, and that
`cargo test -p ls-core` + `make docs-check` are green on the final tree.

Run a deterministic evidence audit for every TR promoted in this sweep before
shipping. Until this exists as a repo command, perform the checklist directly and
record the result in the ledger:

- `support.recommended: true`
- `recommendation.evidence_ref` points to the evidence file
- evidence `tr_code` matches the TR
- evidence `date` equals `maintenance.last_reviewed`
- evidence `env` is `paper`
- evidence `line` starts with `LIVE-SMOKE`
- evidence `line` contains no `token`, `appkey`, `secret`, `account`, `rsp_msg`,
  or obvious credential/account-bearing text
- `recommendation.excludes` includes a production-credential exclusion
- class-specific excludes are present for market/account/paginated/realtime
  scopes, especially `S3_` lifecycle-only claims

If the audit fails, stop before shipping and record `phase:
"held_before_shipping"` with the audit failure reason.

## 5. Queue record

If at least one TR promoted, open the maintenance queue issue via
`gh issue create` using the `.github/ISSUE_TEMPLATE/sdk_work_item.yml` contract
and `docs/maintenance-labels.md` labels: `queue:maintenance`, `source:manual`,
`class:cross-cutting`, `support:recommended`, `gate:change-scoped`, and
`evidence:needed`. The body records each TR's outcome (promoted + evidence path,
or held + unmet gate) and names each promoted TR's owner class. Capture the issue
URL.

Held TRs that need new engineering work (for example, no smoke harness) are
reported but not auto-filed as separate issues by this sweep. Route them to a
future `ce-plan` / maintenance work item explicitly.

## 6. Review and ship (fully autonomous through merge)

1. Run the change-scoped gate one final time: `cargo test`, `cargo test -p
   ls-core`, `make docs-check`.
2. Run the available code-review workflow. Prefer `ce-code-review`
   (`base:<default>`) when available; otherwise perform a local review pass over
   the sweep diff, focusing on evidence safety, metadata consistency, generated
   docs, and shipping records. Apply only safe, verified fixes.
3. `git push -u origin <branch>`.
4. `gh pr create` against the default branch, body summarizing promoted/held TRs
   and linking the queue issue (`Closes #<n>` if appropriate).
5. When the PR is mergeable with no failing checks, `gh pr merge --merge
   --delete-branch`.
6. Sync: `git checkout <default> && git pull`, confirm the local branch is gone.

Advance `progress.json` through coarse phases as they complete:

```
discovering
promoting
reconciling
queue_issue_created
final_gate_green
code_review_done
pushed
pr_created
merged
synced
complete
```

Stop before push/PR/merge and record `held_before_merge` or
`held_before_shipping` if any of these are true: unexpected dirty files, final
gate failure, evidence audit failure, unresolved blocking review finding,
branch/ledger consistency failure, unauthenticated or failed `gh` operation,
failing required PR checks, or non-mergeable PR.

## 7. Report

Summarize: promoted TRs (with evidence paths), held TRs (with reasons), the final
recommended count, the queue issue, and the merged PR. Held TRs re-run on a future
sweep when their gate opens (trading day / provisioned account / new harness).

## Reference

- Recipe: `.agents/skills/promote-tr/SKILL.md` (the per-TR unit).
- Subagent: `tr-promoter` (`.claude/agents/tr-promoter.md`).
- Smoke map / discovery: `.agents/skills/promote-tr/references/smoke-map.md`.
