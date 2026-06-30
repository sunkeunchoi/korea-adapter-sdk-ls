---
title: "Grep for an existing carrier + smoke before authoring a session-gated flip candidate"
date: 2026-06-30
category: conventions
module: ls-sdk market_session/paginated carriers, implement-tr recipe
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Running a Tracked to Implemented flip wave over a deferred 'session-dependent' TR cohort"
  - "A plan or brainstorm assumes a tracked TR needs a fresh carrier authored from the baseline"
  - "Picking which flip targets need full implement-tr authoring vs a metadata-only flip"
tags:
  - implement-tr
  - flip-wave
  - finish-the-flip
  - carrier
  - paper-live-smoke
related_components:
  - tooling
---

## Context

A flip wave's plan typically lists "session-dependent" tracked TRs as needing a full
`implement-tr` pass: author the request/response carrier, register the policy in both
crosscheck lists, add a smoke, then flip. But a tracked TR can already be **fully
carried** — struct + `{TR}_POLICY` + `live_smoke_{tr}` + Makefile target all present —
and still sit at `support.implemented: false` because its certifying smoke came back
empty in a prior wave. The repo calls these **finish-the-flip** TRs: the build is done,
only a non-empty in-window smoke is missing. Re-authoring one duplicates symbols
(`{TR}InBlock` / `{TR}_POLICY` already exist) and the carrier won't compile.

In the 2026-06-30 open-window wave, 4 of 10 targets (`t1109`, `t8427`, `t2106`,
`t1964`) were already finish-the-flip wired. The plan had assumed 8 needed carriers.

## Guidance

Before authoring anything for a flip candidate, grep for an existing carrier and smoke
and route accordingly:

```sh
TR=t2106; U=$(echo $TR | tr a-z A-Z)
grep -rl "${U}InBlock\|${U}OutBlock" crates/ls-sdk/src/      # carrier struct?
grep -rl "${U}_POLICY" crates/ls-core/src/endpoint_policy/   # policy const?
grep -l  "fn live_smoke_${TR}\b" crates/ls-sdk/tests/live_smoke.rs  # smoke?
grep -o  "live-smoke-${TR}\b" Makefile                       # make target?
```

- **All present + `implemented: false`** → finish-the-flip. Skip authoring/registration
  (it already passes the crosscheck). The flip is **metadata + docgen only**: run the
  existing `make live-smoke-<tr>`; on a non-empty witness, flip `support.implemented`
  and bump `reference.len` + `banner_trs`.
- **Absent** → genuine new carrier; run the full `implement-tr` pass.

## Why This Matters

The cost asymmetry is large: the false assumption ("needs a carrier") wastes a full
authoring pass and then fails to compile on the duplicate symbols, while the check is
four greps. It also keeps the wave honest about yield — a finish-the-flip TR that
smokes empty again is a one-line PENDING (carrier untouched, stays staged for a future
session), not a build. The plan's unit count is an upper bound on authoring work, not a
contract; verify each target's actual state against the tree before building. See
`docs/solutions/conventions/implement-tr-registration-sites.md` for what a real
authoring pass touches, and `realtime-flip-artifacts-stageable-before-metadata-flip.md`
for the general "artifacts staged ahead of the metadata flip" pattern this is an
instance of.

## When to Apply

- Any flip wave drawing from a previously-deferred cohort (the provisional ledger's
  "session-dependent" / "deferred unsmoked" lists are full of finish-the-flip TRs).
- Most acute right after a prior wave staged carriers + smokes but couldn't certify
  them (closed window, empty paper feed) — those are exactly the candidates a later
  wave re-targets.

## Examples

```sh
# t2106 (F/O price-memo): struct in market_session/quote_deriv.rs, T2106_POLICY in
# endpoint_policy/order.rs (already in both crosscheck lists), live_smoke_t2106 wired.
#   -> finish-the-flip. `make live-smoke-t2106` -> empty memo -> one-line PENDING.
#   Authoring it would have collided on T2106InBlock / T2106_POLICY.

# t1954 (ELW daily): no struct, no policy, no smoke.
#   -> genuine carrier. Full implement-tr pass; flipped on rows=20 in-window smoke.
```
