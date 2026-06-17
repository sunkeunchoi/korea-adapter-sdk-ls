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

## 1. Setup

On the default branch, create a feature branch `feat/promote-trs-<YYYYMMDD>`
(`git checkout -b …` from an up-to-date default). If already on a feature branch,
continue on it.

## 2. Discover candidates

A TR is a **recipe-candidate** iff `support.implemented: true`,
`support.recommended: false`, and its code appears in
`.agents/skills/promote-tr/references/smoke-map.md`. List candidates by reading
`metadata/trs/*.yaml`. Any `implemented && !recommended` TR *not* in the smoke map
(e.g. `revoke`) is recorded immediately as **HELD — needs ce-plan (no smoke
harness)**; do not attempt it.

If there are zero recipe-candidates, report "sweep drained — nothing promotable"
(plus any needs-ce-plan holds) and stop before branching/shipping.

## 3. Promote each candidate (serial)

Promotions all edit the same `crates/ls-docgen/src/lib.rs` banner test and
`metadata/EVIDENCE-FRESHNESS.md` count, so they **must run serially** — never in
parallel (guaranteed file collision). For each candidate, in stable order,
dispatch the **`tr-promoter`** subagent (fresh context per TR) with the TR code.
Each subagent runs the `promote-tr` recipe and returns its final line:
`PROMOTED <tr> …` or `HELD <tr> — <reason>`.

After each subagent returns:
- If `PROMOTED`: confirm the commit landed and the tree is green before
  dispatching the next (the recipe runs the gate, but verify `cargo test` +
  `make docs-check` once more if a prior TR's edit could interact).
- If `HELD`: record the TR + reason; leave it `implemented`; continue.

Do not let one held TR stop the sweep.

## 4. Reconcile

The per-TR recipe bumps the freshness count incrementally, so after the sweep the
count should already be correct. Verify `metadata/EVIDENCE-FRESHNESS.md` states
the final recommended-TR count and class list, and that
`cargo test -p ls-core` + `make docs-check` are green on the final tree.

## 5. Queue record

If at least one TR promoted, open the maintenance queue issue via
`gh issue create` using the `.github/ISSUE_TEMPLATE/sdk_work_item.yml` contract
and `docs/maintenance-labels.md` labels: `queue:maintenance`, `source:manual`,
`support:recommended`, `gate:change-scoped`, `evidence:needed`, plus one
`class:*` per promoted TR's owner class. The body records each TR's outcome
(promoted + evidence path, or held + unmet gate). Capture the issue URL.

## 6. Review and ship (fully autonomous through merge)

1. Run the change-scoped gate one final time: `cargo test`, `cargo test -p
   ls-core`, `make docs-check`.
2. Run `ce-code-review` (`base:<default>`); apply safe verified fixes it commits.
3. `git push -u origin <branch>`.
4. `gh pr create` against the default branch, body summarizing promoted/held TRs
   and linking the queue issue (`Closes #<n>` if appropriate).
5. When the PR is mergeable with no failing checks, `gh pr merge --merge
   --delete-branch`.
6. Sync: `git checkout <default> && git pull`, confirm the local branch is gone.

## 7. Report

Summarize: promoted TRs (with evidence paths), held TRs (with reasons), the final
recommended count, the queue issue, and the merged PR. Held TRs re-run on a future
sweep when their gate opens (trading day / provisioned account / new harness).

## Reference

- Recipe: `.agents/skills/promote-tr/SKILL.md` (the per-TR unit).
- Subagent: `tr-promoter` (`.claude/agents/tr-promoter.md`).
- Smoke map / discovery: `.agents/skills/promote-tr/references/smoke-map.md`.
