---
title: "Running a strategy-loop param turn: the 0.5 proposal-bounds cap, fresh-home v3 seeding, and offline mechanics"
date: 2026-07-09
category: conventions
module: adapters/nautilus lab-research turn (lab/src/runner/research.rs), strategy loop
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Executing a strategy-loop parameter turn with `lab-research turn` (e.g. widening universe_top_n, flipping gap_min_pct)"
  - "Running a turn against a FRESH data home (a new LS_DATA_HOME with no prior runs)"
  - "A turn refuses with `proposal_bounds ... exceeds bound 0.5000` or `the fresh home is missing the seeded v3 manifest`"
  - "Comparing two runs with `lab-research runs compare` and getting a data-mode FAIL"
tags:
  - strategy-loop
  - lab-research-turn
  - proposal-bounds
  - fresh-home-seeding
  - param-turn
  - runs-compare
  - offline-backtest
---

## Context

A strategy-loop turn changes one governed parameter, re-runs the backtest, and
authors a keep/revert/insufficient verdict. Executed via `lab-research turn` with
`LS_TURN_PARAM`/`LS_TURN_VALUE` and the seed-assertion pins
`LS_TURN_EXPECT_VERSION`/`LS_TURN_EXPECT_GAP`. Turn 4 (widen `universe_top_n`
20→40 on a fresh `data/turn4-fresh` home) hit three non-obvious guards that each
refuse-and-run-nothing rather than fail loudly mid-run. This documents them so a
future turn doesn't rediscover them.

## Guidance

**1. A single param turn cannot change a parameter more than 50% (relative).**
`PROPOSAL_BOUNDS_CAP = 0.5` is a committed constant wired into the CLI pipeline
(`adapters/nautilus/lab/src/runner/research.rs:54`, R4/KTD3) — **not**
env-overridable. Attempting `universe_top_n` 20→40 (+100%) refuses:

```
turn ran no backtest: proposal_bounds: parameter 'universe_top_n' relative change
1.0000 exceeds bound 0.5000 (current 20.0000, proposed 40.0000)
```

Reach a large change via **multiple legged turns**, each within the bound:
20→30 (v3→v4, +50% exactly — on-bound passes via a float tolerance), then 30→40
(v4→v5, +33%). Leg 2's `LS_TURN_EXPECT_VERSION` is the *new* base (4), not 3.

**2. A fresh data home must be seeded with a v3 manifest before the turn.** On a
fresh home `latest_finalized_run()` is `None`, so params fall back to
`OrbParams::default()` (v0, gap 3.0) — a silent wrong-strategy run. The KTD-5
seed-assertion (`research.rs:234-253`) refuses instead:

```
v3-param resolution failed: resolved strategy v0 ... expected v3 — the fresh home
is missing the seeded v3 manifest (KTD-5). Copy the turn-2b v3 run's manifest.json
into runs/ before rerunning
```

Fix: copy a prior v3 run **directory** into `<LS_DATA_HOME>/runs/`. `list_runs`
(`adapters/nautilus/lab/src/artifacts/mod.rs:210`) accepts any non-`.tmp-` dir; `latest_finalized_run`
reads `runs/<id>/manifest.json` and orders by stamp + numeric `-vN`. In this repo
the source is `adapters/nautilus/data/turn3/runs/*-orb-v3` (verify the manifest is
`strategy_version:3`, `universe_top_n:20`, `gap_min_pct:0.6`).

**3. `lab-research turn` is OFFLINE.** It reads the local catalog under
`LS_DATA_HOME`; it needs no `LS_TRADING_ENV=paper` and never touches the gateway.
Ingest is the only online step; the turn that consumes the data is not.

**4. `runs compare` data-mode requires a zero-key param diff.** A pure param-turn
series (v3→v4→v5) has no same-param pair, so data-mode always FAILs on it:

```
FAIL: data turn requires a zero-key param diff, got ["strategy_version", "universe_top_n"]
no data deltas (fingerprint/range/universe all equal)
```

The `no data deltas` line still confirms data consistency. Use **param-mode**
(the default) to compare param turns — it PASSes on a param-only delta.

**5. Two data-home locations exist and are easy to confuse:** repo-root `data/`
(where this turn's `turn4-fresh` lives, matching the ingest's `LS_DATA_HOME`) vs
`adapters/nautilus/data/` (where `turn3` lives, the seed source). Point
`LS_DATA_HOME` at the same absolute path the ingest wrote to.

## Why This Matters

All five guards *refuse and run nothing* rather than producing a wrong result — a
turn that silently ran defaults (v0/gap-3.0) or bypassed the bounds cap would
corrupt the loop's evidence chain. The failure messages are precise but only
appear at run time; knowing them up front turns a multi-attempt debugging loop
into a two-line setup (seed the home, plan the legs). Bypassing any of them (e.g.
editing `PROPOSAL_BOUNDS_CAP`, or dropping the `EXPECT_` guards to force a run)
defeats the governance the loop exists to enforce — don't.

## When to Apply

Any `lab-research turn` invocation, especially against a fresh data home or when
the intended parameter change exceeds ±50% of its current value.

## Examples

Turn 4, reaching `universe_top_n=40` from a turn-3 baseline of 20, on a fresh home:

```bash
# 0. seed the fresh home with a v3 run (else the turn refuses)
cp -R adapters/nautilus/data/turn3/runs/<stamp>-backtest-orb-v3 \
      <repo>/data/turn4-fresh/runs/

# 1. leg one: 20 -> 30 (v3 -> v4, +50%, on-bound OK)
LS_DATA_HOME=<repo>/data/turn4-fresh \
LS_TURN_PARAM=universe_top_n LS_TURN_VALUE=30 \
LS_TURN_EXPECT_VERSION=3 LS_TURN_EXPECT_GAP=0.6 \
LS_TURN_SDATE=20260526 LS_TURN_EDATE=20260703 \
  ./target/debug/lab-research turn

# 2. leg two: 30 -> 40 (v4 -> v5, +33%); note EXPECT_VERSION is now 4
LS_DATA_HOME=<repo>/data/turn4-fresh \
LS_TURN_PARAM=universe_top_n LS_TURN_VALUE=40 \
LS_TURN_EXPECT_VERSION=4 LS_TURN_EXPECT_GAP=0.6 \
LS_TURN_SDATE=20260526 LS_TURN_EDATE=20260703 \
  ./target/debug/lab-research turn
```

Attempting the jump in one turn (`LS_TURN_VALUE=40` from base 20) refuses with the
`proposal_bounds ... exceeds bound 0.5000` message and writes no run.
