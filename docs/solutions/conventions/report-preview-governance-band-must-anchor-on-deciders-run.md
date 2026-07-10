---
title: "A report/preview command's governance band must anchor on the same run the decider (ProposalBoundsGuardrail) will use, not the reported run"
date: 2026-07-10
category: conventions
module: adapters/nautilus/lab strategy loop (lab/src/runner/report.rs report-mfe governance band vs lab/src/runner/research.rs ProposalBoundsGuardrail)
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Writing a report/preview subcommand whose printed verdict forecasts a decision a separate governed command (a guardrail/gate) will make"
  - "The report can be pointed at a non-latest run (e.g. LS_REPORT_RUN) while the decider always anchors on the latest finalized run's params"
  - "The report re-derives the decider's bound arithmetic locally instead of calling the decider's guardrail/policy function directly"
  - "Auditing `lab-research report mfe` output or planning a profit_target_r sweep after turn 9 (registry: latest=v11@1.05, analysis commonly reads v9@1.0 or v10@1.5)"
tags:
  - strategy-loop
  - report-command
  - proposal-bounds-guardrail
  - anchor-mismatch
  - preview-consistency
  - mfe
  - profit-target
  - tooling
---

# A report/preview command's governance band must anchor on the same run the decider (ProposalBoundsGuardrail) will use, not the reported run

## Context — what surfaced this (turn-9 review; the registry state that makes it live)

Turn-9 (PR #114) added `report mfe` — a read-only subcommand over a finalized
run's `decisions.jsonl` that derives a leg-2 profit-target candidate and prints
a governance verdict: `RUNNABLE`, `RIGHT-CENSORED` (candidate pinned at the
source run's own target — no informative signal), or `OUT-OF-BAND` (outside
the proposal-bounds band, `AE3`). The candidate and its band are computed in
`leg_two_candidate` (`adapters/nautilus/lab/src/runner/report.rs:163-180`),
banding off `profit_target_r * (1 ± PROPOSAL_BOUNDS_CAP)` where
`profit_target_r` comes from **the reported run's manifest**
(`report.rs:197`, `let profit_target_r = manifest.params.profit_target_r;`).

The actual decider is the `turn` command's `ProposalBoundsGuardrail`
(`adapters/nautilus/lab/src/agent/guardrails/proposal_bounds.rs:47-89`), which
rejects a `ProposeParameterChange` intent when
`|proposed_value - current_value| / |current_value|` exceeds
`max_relative_change` (`PROPOSAL_BOUNDS_CAP = 0.5`,
`research.rs:54`). Critically, `turn()` resolves `current_value` from the
**latest finalized run**, not from whatever run the operator happens to be
reporting on: `let prior = latest_finalized_run(&cfg.data_home)?;`, with the current
params then taken from that prior run's manifest (`match &prior { Some((_, m)) => (m.params.clone(), m.strategy_version), ... }`, `research.rs:225-229`,
`latest_finalized_run` itself at `research.rs:102-107`).

In the designed flow — report on the latest run, then propose off that same
run — the two anchors are the same manifest and can't diverge. The moment an
operator pins `LS_REPORT_RUN` to an older run (exactly the state the registry
was in after turn-9: latest finalized is `v11@1.05`, but the operator wanted
to inspect `v9@1.0`'s MFE distribution), the report's band and the guardrail's
band are computed off different `profit_target_r` values, and the printed
verdict can be flatly wrong in either direction. Adversarial code review
during PR #114 caught this; an independent validator re-derived the arithmetic
and confirmed both worked examples below.

## Guidance — the rule

**A report/preview command that forecasts a governed decision must anchor on
the same state the decider will use — or explicitly print the divergence.**
There are exactly two honest implementations:

1. **Share the anchor.** Make the preview call the same resolution path the
   decider uses (here: band off `latest_finalized_run`'s params, or call
   `ProposalBoundsGuardrail::evaluate` directly instead of re-deriving
   `current * (1 ± cap)`).
2. **Name the divergence.** Keep the preview anchored on the artifact it's
   actually describing (the reported run — correct for what a distribution
   report is claiming), but print that a different run will govern the real
   decision when they differ.

PR #114 shipped option 2, deliberately, because the band's designed job is to
describe *this run's* distribution relative to *this run's* own target — that
part isn't wrong. What was missing was the silence when the reported run
isn't the one `turn` will actually gate against. The fix
(`report.rs:304-315`):

```rust
// The governance band below is anchored on THIS run's target, but a next
// `turn` proposes off the LATEST finalized run's params — when they differ,
// say so rather than letting the band read as the guardrail's answer.
if let Some((latest_id, latest_m)) = &latest {
    if *latest_id != run_id {
        lines.push(format!(
            "note: latest finalized run is {latest_id} (profit_target_r {:.2}) — the turn \
             guardrail bands off that value, not this run's",
            latest_m.params.profit_target_r
        ));
    }
}
```

Paired with a header marker for the *other* half of the same anchoring
problem — an unpinned run selection silently defaulting to "whichever run is
latest right now" (`report.rs:196`, `299-303`):

```rust
let defaulted = cfg.run_id.is_none();
...
lines.push(format!(
    "report mfe: run {run_id} (strategy v{}, profit_target_r {profit_target_r:.2}){}",
    manifest.strategy_version,
    if defaulted { " [defaulted: latest finalized]" } else { "" }
));
```

Before the fix: reporting `v9` while `v11` was latest printed only
`report mfe: run 20260101T...-v9 (strategy v9, profit_target_r 1.00)` and a
band line — nothing said a different run governs. After: an extra line names
the latest run and its target explicitly. Both behaviors are tested —
`pinned_non_latest_run_notes_the_latest_guardrail_anchor` and
`defaulted_run_selection_is_marked_in_the_header`
(`report.rs:891-928`).

## Why This Matters — verdict-flips misdirect governed sweeps in both directions

Because the guardrail's bound is a symmetric ratio band around a *moving*
current value, a fixed candidate can land inside one run's band and outside
another's — in either direction, not just "the preview is stricter than
reality." A silent divergence is worse than either single-anchor choice would
be alone: an operator who trusts the printed verdict at face value gets misled
regardless of which way it's wrong, and worse, the wrongness isn't
self-evidently a bug — the report *looks* internally consistent (the band
line and the reported run's target agree with each other), so nothing on
screen contradicts it. See the worked numbers in Examples below.

## When to Apply

Any preview / dry-run / report command that forecasts what a **governed
gate** will decide — a guardrail, a validator, a budget check, a capacity
planner — is exposed to this exact failure mode whenever the gate's decision
depends on state (current params, current budget spend, current inventory)
that can drift between "what the preview looked at" and "what the gate will
look at when it actually runs." Checklist:

- Identify the exact resolution path the decider uses to get its "current"
  anchor (here: `latest_finalized_run` → `manifest.params`).
- Check whether the preview command can be pointed at a different state than
  that anchor (a pinned run id, a stale cache, an as-of timestamp, an
  operator override).
- If yes, either (a) resolve the preview's band from the *same* call the
  decider uses, or (b) print the decider's anchor value alongside the
  preview's own, whenever they differ.
- **Extra risk when the formula is re-derived instead of the deciding code
  being called directly.** `report.rs` re-derives
  `profit_target_r * (1 ± PROPOSAL_BOUNDS_CAP)` rather than invoking
  `ProposalBoundsGuardrail` — correct today because both read the same
  constant and the same relative-change formula, but a latent drift risk: if
  `ProposalBoundsGuardrail`'s semantics change (e.g. the epsilon tolerance at
  `proposal_bounds.rs:25`, or the zero-current special case at
  `proposal_bounds.rs:56-63`) without a corresponding update to
  `leg_two_candidate`, the two silently diverge again with no test to catch
  it. This was raised in review and deliberately left open rather than fixed
  in PR #114 — calling the guardrail directly (or factoring the band formula
  into one shared function) remains the more robust fix if the guardrail's
  semantics ever move.

## Examples

Concrete post-turn-9 registry state (latest finalized = `v11`, target `1.05`;
band width `±50%` = `PROPOSAL_BOUNDS_CAP`):

| Reported run | Reported target | Candidate | Report's band (off reported run) | Report prints | Guardrail actually decides (off `v11@1.05`) |
|---|---|---|---|---|---|
| v9 | 1.00 | 1.55 | [0.50, 1.50] | `OUT-OF-BAND` | **Approved** — `|1.55-1.05|/1.05 = 0.476 <= 0.5` |
| v9 | 1.00 | 0.51 | [0.50, 1.50] | `RUNNABLE` | **Rejected** — `|0.51-1.05|/1.05 = 0.514 > 0.5` |

Both directions flip: the report can tell an operator to abandon a candidate
the guardrail would actually approve, or to run one it would actually refuse.

Shipped note line (per `report.rs:309-313`, exact wording):

```
note: latest finalized run is 20260101T000000Z-backtest-orb-v10 (profit_target_r 1.50) — the turn guardrail bands off that value, not this run's
```

**Deliberately not changed** (open, named as a known drift risk rather than
fixed): the band itself still anchors on the *reported* run's
`profit_target_r`, which is correct for what the report is describing (this
run's own censoring point); and `leg_two_candidate` re-derives the guardrail's
relative-change formula instead of calling `ProposalBoundsGuardrail::evaluate`
directly. The deeper fix — anchor the runnability band off the latest run, or
call the guardrail function itself — remains open per the review session's
conclusion, tracked as a drift risk if `ProposalBoundsGuardrail`'s bound
formula or epsilon handling ever changes independently of `report.rs`.

Reference: PR #114 ("feat(lab): turn-9 — MFE distribution report + profit-target sweep"); fix landed as its review-follow-up commit on `feat/turn9-mfe-report-target-sweep` (squash-merges rewrite SHAs, so cite the PR, not the bare commit).

## Related

- [Turn 9: profit-target sweep falsified exit geometry as the lever; the MFE distribution names an entry-side breakout-strength band-pass filter](../conventions/strategy-loop-turn-9-profit-target-sweep-and-mfe-distribution.md) — Same tool/component (lab-research `report mfe`, adapters/nautilus/lab/src/runner/report.
- [Running a strategy-loop param turn: the 0.5 proposal-bounds cap, fresh-home v3 seeding, and offline mechanics](../conventions/strategy-loop-param-turn-governance-and-fresh-home-seeding.md) — Documents how `lab-research turn` resolves the governed params it acts on via `latest_finalized_run()` (research.
- [A numeric-bound guardrail comparing at full float precision denies intended on-bound values by accumulated rounding dust](../logic-errors/bound-comparison-at-full-float-precision-denies-on-bound-values.md) — Same guardrail family (proposal-bounds / governance in the strategy loop) and same 'governance'/'guardrail' tags, but an unrelated bug class (float precision at a comparison boundary vs.
- [Producing a version-labeled code-turn re-baseline run when the lab-research CLI has no version-only-bump path](../workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md) — Also keys off `latest_finalized_run()` / manifest ordering (research.
- [Reading a strategy-loop param-turn outcome: win rate moves but expectancy flat means the lever is exit geometry, not the entry param](../conventions/strategy-loop-reading-param-turn-outcomes-win-rate-vs-expectancy.md) — Adjacent strategy-loop verdict-authoring convention (how to read turn outcomes), same module family, but addresses metric interpretation rather than report/decider anchor consistency.
- PR #114: https://github.com/sunkeunchoi/korea-adapter-sdk-ls/pull/114
