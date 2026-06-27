---
date: 2026-06-27
topic: closed-window-more-flips
---

# Closed-window "more flips" wave — requirements

## Summary

Flip more TRs from Tracked → Implemented while KRX is closed. Audit the 73
tracked-only TRs, smoke the static/persistent ones, and flip every static read
that returns non-empty data under closure; then opportunistically track-and-flip
a bounded batch of new raw *static* reads.
There is no target count — flip what genuinely serves, and record a faithful
disposition for everything that doesn't.

## Problem Frame

The closure premise holds for the reads tested so far. Plan -003 (`f2887da`,
#60) flipped t1310 and t1404 under closure; plan -004 (`4167d09`, #61) tracked
79 reads and flipped 25 of them under closure across batches A/B/C (charts,
designation boards, master/reference, rankings). The working model is that
static and persistent reads return non-empty data regardless of session while
session-dependent reads (live quote, chart-session, night, overseas) return
empty `00707` until an open window. But static-classification is a per-TR
heuristic, not a law — plan -003 called it a load-bearing premise and plan -004
built an explicit yield floor for exactly this reason. A near-dry audit is
evidence the heuristic over-included, not a result to pre-absolve as success.

Two facts shape this wave specifically. First, today is Saturday — KRX is
closed, so the session-dependent path stays shut and only static reads are
reachable. Second, plan -004 likely already harvested the obvious closure-viable
static reads from the 79 it tracked. A large slice of today's 73 tracked-only
pool is therefore probably the -004 *leftovers* — reads parked because they came
back empty under closure, or because they carry hard blockers. The pool audit
may come back nearly dry, leaving the wave resting mostly on the raw top-up. A
dry audit is an accepted yield outcome, but it is also a signal the static
heuristic over-included — read it as such, not as silent success.

## Key Decisions

- **Honest yield over a target count.** A TR flips only when its smoke returns a
  success body that deserializes and at least one modeled non-key field holds a
  non-default value. No flip is forced to reach a number; a small honest count is
  a success. False positives are the cardinal failure here.
- **Pool before raw, raw as opportunity.** Audit and flip the existing
  tracked-only pool first (no tracking churn), then track-and-flip new raw reads
  only as far as honest closure-viability extends — the raw top-up is
  opportunistic, not a count-filler.
- **Closure-viable means static.** Only static / persistent reads are
  candidates: master/reference, administrative-designation boards, rankings, and
  historical charts. Session-dependent, night, and overseas reads are excluded
  up front because closure guarantees they return empty.
- **Empty-under-closure defaults to open-window PENDING, not paper_incompatible.**
  The classification is asserted, not observed — open-window behavior is
  unobservable while closed. Record open-window PENDING only when an independent
  signal (title/family, the normalized baseline, or a prior open-window
  observation) marks the read session-dependent; absent any such signal, record
  it as empty-under-closure, classification unconfirmed pending open-window
  re-test. `paper_incompatible: true` stays reserved for the 11 night/overseas
  feeds paper never serves (g3101/g3102/g3103/g3104/g3106/g3190,
  t8455/t8460/t8463, CCENQ10100/CCENQ90200) — note those 11 were themselves
  confirmed from empty smokes, so the PENDING-vs-incompatible line rests on the
  independent signal, not the empty result alone.

## Requirements

**Candidate selection**

- R1. Triage the 73 tracked-only TRs into: closure-viable static reads to smoke
  now; session-dependent reads deferred to an open-window wave; hard-blocked
  reads left untouched; and the 11 `paper_incompatible` reads excluded before
  candidacy (R2). A static-classified read that smokes empty is a further
  outcome — recorded as heuristic over-inclusion and dispositioned per R6
  (open-window PENDING on an independent signal, else classification-unconfirmed
  pending re-test), not silently dropped.
- R2. Exclude the 11 `paper_incompatible: true` TRs from candidacy — they never
  flip under any session.
- R3. Leave hard-blocked tracked TRs untouched and unchanged: t1860
  (realtime-control subscription, not a read), t1852/t1856 (require `sFileData`
  input), t3102 (requires `sNewsno` input), t1964 (empty-board).
- R4. Top up with new raw static reads only after the pool audit, and only for
  reads whose family is plausibly closure-viable (master/reference, designation,
  ranking, historical chart). Pre-screen raw candidates before committing to
  tracking so duds don't incur tracking churn.

**Flip gate**

- R5. A candidate flips to Implemented only when its Paper Live Smoke, run under
  closure, returns a success body that deserializes into the response type AND at
  least one modeled non-key field holds a non-default value. This certifies the
  read is callable and shape-correct under closure, not that it serves fresh
  data — for a persistent read a session-stale body passes this gate identically
  to a live one, so a closure-flipped static read may warrant an open-window
  freshness re-check before Recommended.
- R6. A candidate whose smoke returns empty (`00707`) does not flip. A
  static-classified candidate that unexpectedly smokes empty is recorded as
  open-window PENDING only on an independent session-dependent signal; absent one
  it is recorded as classification-unconfirmed pending open-window re-test.
  Session-dependent reads identified in triage (R1) are deferred unsmoked, not
  smoked-then-dispositioned this wave.
- R7. A candidate whose smoke fails to deserialize or returns an HTTP/gateway
  error is classified before any flip — a wire-type defect is fixed and
  re-smoked; an environmental failure is retried, not flipped.

**Disposition and bookkeeping**

- R8. Every candidate that does not flip carries a faithful, specific
  disposition (open-window PENDING / hard-blocked-reason / paper_incompatible) so
  no TR is silently dropped and no flippable TR is permanently excluded.
- R9. Each flip updates all registration sites and count families the flip recipe
  requires, and the full gate stays green.
- R10. Recommended promotion is out of this wave; flipped TRs land with
  `recommended: false` and no recommendation block. A closure-flipped static read
  is tagged with a deferred open-window freshness re-check (per R5) so the later
  Recommended pass inherits the obligation rather than losing it between waves.

## Acceptance Examples

- AE1. Covers R5. Given KRX is closed, when a tracked designation/master/ranking
  read's smoke returns a body that deserializes with a non-default modeled field,
  then it flips to Implemented.
- AE2. Covers R6. Given KRX is closed, when a tracked read's smoke returns empty
  `00707`, then it does not flip; it is recorded as open-window PENDING when an
  independent session-dependent signal exists, otherwise as
  classification-unconfirmed pending open-window re-test.
- AE3. Covers R1, R8. Given the pool audit yields zero closure-viable survivors,
  then the wave proceeds on the raw top-up alone and every audited pool TR still
  carries its disposition.
- AE4. Covers R7. Given a raw-tracked candidate's smoke fails to deserialize,
  when raw-probe classifies it a wire-type defect, then the wire type is fixed
  and it is re-smoked rather than dropped or flipped.

## Scope Boundaries

- Session-dependent reads (live quote, chart-session) and night/overseas feeds —
  deferred to a future open-window wave (the former) or never (the latter, the 11
  `paper_incompatible` TRs).
- Recommended promotion — its own ADR-0008 pass, not this wave.
- The hard-blocked tracked TRs (R3) — not re-attempted here; they need inputs or
  a realtime path closure does not provide.

## Dependencies / Assumptions

- Assumes plan -004 already harvested most closure-viable static reads, so the
  pool audit may yield few or zero survivors — accepted per the Problem Frame.
- The flip recipe (`.agents/skills/implement-tr/SKILL.md`) is stable; the closure
  premise is well-exercised (n=2 + 25 flips) but remains a per-TR heuristic. This
  wave reuses both without modification.
- Smokes hit the real LS paper gateway with `LS_TRADING_ENV=paper`; an
  order-capable account is not required (all candidates are reads).

## Outstanding Questions

Deferred to planning:

- PR structure — single PR vs stacked batches — depends on how many candidates
  survive the audit plus top-up; decide once the surviving count is known.
- The exact bound on the raw top-up — how many raw families to pre-screen before
  diminishing returns set in.

## Sources / Research

- `docs/plans/2026-06-26-004-feat-closed-window-breadth-flip-wave-plan.md` —
  prior breadth wave (79 tracked, 25 flipped); count-bump and exemplar-trap
  gotchas.
- `docs/plans/2026-06-26-003-feat-closed-window-flip-wave-plan.md` — closure
  premise and non-empty flip gate (R3/R4 precedent).
- `docs/brainstorms/2026-06-26-krx-closed-flip-wave-requirements.md` — the
  session-independent vs blocked distinction.
- `.agents/skills/implement-tr/SKILL.md` — the flip recipe and registration
  sites.
- Current state (verified): `crates/ls-trackers/tests/api_drift.rs:106`
  (maintained_tr_count 213), `crates/ls-docgen/src/lib.rs:1131`
  (`reference.len()` 141 = index page + 140 implemented); 140 implemented + 73
  tracked-only; 11 `paper_incompatible`.

## Deferred / Open Questions

### From 2026-06-27 review

- Beneficiary of an Implemented (non-production-endorsed) TR is unstated. Per
  CONCEPTS.md an Implemented TR is callable but explicitly not endorsed for
  production (only Recommended is), while R10 defers Recommended. Name the
  concrete beneficiary of flipping Tracked → Implemented this wave — a downstream
  caller that imports the handle, or a later Recommended pass that needs
  Implemented as a prerequisite — or state outright that the value is
  lifecycle-completeness with no current consumer. (product-lens)
- "More flips" optimizes supply (what's reachable under closure), not demand
  (what a caller needs); each flip adds permanent maintenance surface on a small
  team. Decide whether candidacy should be gated on a recorded consumer need
  rather than closure-reachability alone. (product-lens)
- Do-nothing / defer-to-open-window baseline not weighed. An open-window wave
  certifies both static and session-dependent reads at the same fixed per-flip
  cost; state why flipping the residual statics now (under closure, likely
  near-dry pool) beats folding them into the next open-window wave. Relates to the
  still-open flip-cost / cadence decision. (product-lens, adversarial)
