---
title: "An empty mount universe on a flat/red open is correct head behavior — verify with a prior-date offline run, never work around it"
date: 2026-07-28
category: workflow-issues
module: "adapters/nautilus lab mount-universe producer (lab/src/runner/mount_universe.rs) + universe selection (lab/src/strategy/orb.rs select_universe)"
problem_type: workflow_issue
component: tooling
severity: medium
applies_when:
  - "lab-mount-universe refuses with 'no symbol resolved for <date>: N candidate(s) had a daily bar for that date, 0 survived selection'"
  - "Deciding whether a session-morning mount-universe refusal is a pipeline failure or a legitimate no-trade day"
  - "Tempted to hand-author or force an LS_MOUNT_UNIVERSE_FILE because the producer refused to write one"
  - "Reasoning about which gap gate rejected a candidate — the selection-time gap floor vs the intraday gap-retention gate"
tags:
  - "mount-universe"
  - "select-universe"
  - "gap-min-pct"
  - "no-trade-day"
  - "rung-1"
  - "session-morning"
  - "empty-universe"
---

# An empty mount universe on a flat/red open is correct head behavior — verify with a prior-date offline run, never work around it

## Context

On the 2026-07-28 session morning, `lab-mount-universe` ran its live-t8407
today-open path end-to-end — all 75 catalog symbols resolved a live open, no
drops reported — and then refused:

```
Error: mount-universe: no symbol resolved for 2026-07-28: 75 candidate(s) had
a daily bar for that date, 0 survived selection, 0 of those were dropped for
having no computable prior ATR. Selection rejected every candidate: gap=73,
missing_metadata=2
```

After a morning spent unblocking the catalog advance (see the t8410
degenerate-window doc under Related), a refusal here reads like one more
pipeline failure. It is not: the head only trades symbols that gapped up at
the open, and on a market-wide flat/red open **zero qualifying candidates is
the correct answer**.

## Guidance

**Read the rejection accounting first.** The refusal message's rejection
counts sum to the full candidate set (`gap=73 + missing_metadata=2 = 75`, and
`0 ... dropped for having no computable prior ATR`). When the counts add up
and the dominant class is `gap`, selection genuinely evaluated everything and
nothing qualified — this is not a data problem.

**Know which gate rejected.** `select_universe`
(`adapters/nautilus/lab/src/strategy/orb.rs:193`) rejects a candidate when
`gap < params.gap_min_pct`, where `gap_pct = (today_open − prior_close) /
prior_close`. Two things about that floor are easy to get wrong:

- The armed value comes from the **head's finalized run manifest** via
  `resolve_mount_head_params` (`adapters/nautilus/lab/src/runner/mount_universe.rs:395`)
  — the v34 head arms `gap_min_pct: 0.6` — not from `OrbParams::default()`
  (`adapters/nautilus/lab/src/params.rs:354`, which is `3.0`). Reasoning from
  the default makes a normal morning look impossibly strict.
- **Negative gaps always reject** (any negative gap is below any positive
  floor), so a red open across the board legitimately zeroes the universe.

This selection-time gap **floor** is distinct from the intraday
**gap-retention gate** (#168) the strategy applies after the opening range
forms — clearing one says nothing about the other.

**Verify the machinery with a prior-date offline run.** The decisive check
costs one command and no gateway calls: run the producer for the PRIOR
session date. Past dates take the fully-offline catalog path (no lane env
needed):

```sh
LS_DATA_HOME=$R/data/turn4-fresh \
LS_MOUNT_UNIVERSE_DATE=2026-07-27 \
LS_MOUNT_UNIVERSE_METADATA=$R/adapters/nautilus/lab/config/universe-metadata-20260723.json \
LS_CALENDAR_SNAPSHOT=$R/adapters/nautilus/state/krx.calendar.json \
  ./target/release/lab-mount-universe --out /tmp/mount-universe-backcheck.json
# → mount-universe: wrote 40 symbol(s)
```

On 2026-07-27 (a broad gap-up day) this wrote 40 symbols with gaps
+0.61%..+10.12%. Same binary, same params, same catalog — the only variable
left is the market's open, which isolates the empty result to conditions, not
code.

**Never hand-author the universe file.** The producer refuses to write an
empty file, and the temptation is to fabricate one to keep the session alive.
A hand-authored row missing `prior_atr` silently disables the armed OR-width
gate for that symbol (the producer's own docs state this), and an empty
universe day simply means **no rung-1 attended session** — a legitimate
no-trade day, not a blocker.

## Why This Matters

The cost of misreading this refusal runs in both directions. Treating correct
behavior as a failure burns a session morning debugging healthy code — or
worse, leads to hand-authoring a universe file that trades symbols the head
would exclude, un-gated. Treating an actual failure as "just a quiet day"
silently skips sessions: the same headline can be produced by a broken prior
(stale catalog) making every gap compute wrong. The rejection accounting plus
the prior-date back-check distinguishes the two in under a minute.

Interpretation aid: the v34 head's backtest averaged ~2.6 trades per session
across its window — small daily universes, including empty ones, are the
head's normal operating texture, not an anomaly.

## When to Apply

- Any `lab-mount-universe` refusal where candidates were found but zero
  survived selection — check the rejection accounting before debugging
- Before escalating a session-morning "no universe" into a pipeline
  investigation
- Whenever a workaround for a producer refusal starts to look attractive —
  the refusal is a designed fail-closed, and the override path (hand-authored
  file) has a known silent-degradation mode

## Examples

The discriminating pair from 2026-07-28:

| run | date | path | result |
|---|---|---|---|
| live | 2026-07-28 (today) | t8407 live opens | `0 survived selection: gap=73, missing_metadata=2` |
| back-check | 2026-07-27 (prior) | offline catalog | `wrote 40 symbol(s)`, gaps +0.61%..+10.12% |

Reading the back-check file also confirmed the mount-universe file is **not**
gap-filtered per row — it carried 0.61% gap rows. The gap floor, turnover
ranking, and top-N all apply inside `select_universe` at selection time; the
file is the candidate set with prior-session facts, not the post-gate
selection.

## Related

- [`mount-universe-producer-cannot-be-fed-on-a-session-morning`](../architecture-patterns/mount-universe-producer-cannot-be-fed-on-a-session-morning.md)
  — the producer's design, its live today_open leg, and why past dates are
  fully offline
- [`ls-gateway-t8410-single-day-window-ignores-sdate-append-refused`](../integration-issues/ls-gateway-t8410-single-day-window-ignores-sdate-append-refused.md)
  — the catalog-advance failure that preceded this check on the same morning
  (a genuinely broken prior would produce a superficially similar refusal)
