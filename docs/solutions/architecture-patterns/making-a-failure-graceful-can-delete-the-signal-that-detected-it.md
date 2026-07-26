---
title: "Making a failure mode graceful can DELETE the signal that was detecting it — the wreckage was load-bearing"
date: 2026-07-26
category: architecture-patterns
module: "adapters/nautilus/lab — runner/live.rs run_live_session + artifacts/data_quality.rs + dispatch/ladder.rs scan_limit_events + dispatch/readiness.rs readiness_verdict"
problem_type: architecture_pattern
component: development_workflow
severity: high
applies_when:
  - "Adding a timeout, backstop, retry, or graceful-degradation path to something that used to crash, hang, or abort"
  - "A fix converts an UNFINALIZED artifact into a finalized one, or a crash into a clean exit"
  - "A downstream scan, gate, or health metric consumes the artifacts of the thing you are making more graceful"
  - "Reviewing a change whose stated win is 'now it fails cleanly instead of leaving a mess behind'"
tags:
  - accidental-signal
  - fail-safe
  - production-ladder
  - graceful-degradation
  - observability
  - live-session-driver
---

# Making a failure mode graceful can delete the signal that was detecting it

## Context

The rung-1 live-session driver (`adapters/nautilus/lab/src/runner/live.rs`) awaited `node.run`
with no deadline. `handle.stop()` is a request the node may ignore, so a wedged node blocked the
driver forever: the exactly-one teardown and `stage_and_finalize` were both unreachable. The
session left a `.tmp-<run_id>` staging directory behind and an operator interrupting the process
was the only catch.

The fix is obviously good: a stop-relative hard-stop deadline abandons the node after a grace and
runs the same teardown + finalize, so the session ends in a **finalized, scannable run** instead of
residue. It shipped with tests, a green gate, and a runbook update.

It also silently disabled two safety scans.

## The pattern

`.tmp-<run_id>` residue was never designed as a safety signal. It is the *accident* of a
crash-shaped failure — a staging directory that never got renamed. But over time, two independent
consumers came to depend on it:

- `scan_limit_events` (`adapters/nautilus/lab/src/dispatch/ladder.rs:260`) emits a `tmp_residue`
  limit event for residue that matches a consumed dispatch, which **de-escalates the capital
  ladder** — at rung 1, suspending to rung 0.
- `readiness_verdict` (`adapters/nautilus/lab/src/dispatch/readiness.rs`) reds the trailing-K
  readiness window when `catalog.aborted_runs > 0`.

Neither reads the driver. Both read the wreckage. So when the fix stopped producing wreckage, both
went quiet — and the *only* remaining trace of an abandoned node was a free-text line in the run's
`observations` vector, which nothing scans.

The consequence is worse than the bug it replaced. A hung driver is loud: an operator notices a
session that will not end. A hard-stopped session that scores as **clean** is silent, and it counts
toward the K clean trailing sessions that authorize promoting the ladder to larger capital. The fix
converted a visible failure into an invisible one that actively argues for more risk.

**The general shape:** a failure mode's *debris* is often doing detection work that nobody wrote
down, because the detector was authored against the debris rather than against the event. Making
the failure graceful removes the debris and the detection with it. Nothing errors — the scans keep
running and keep returning "clean."

## Guidance

When a change makes a failure mode more graceful, ask: **what was detecting this failure before,
and does it still fire?**

Concretely, for anything that used to crash / hang / abort / leave a partial artifact:

1. **Name the old failure's observable residue.** A `.tmp-` directory, a non-zero exit, an
   unfinalized row, a missing heartbeat, a stuck queue depth, a process that never exits.
2. **Grep for consumers of that residue.** Anything that scans for the artifact's *absence* or
   *incompleteness* is a detector you are about to blind. `rg` for the path pattern, the status
   enum, the "aborted"/"orphan"/"stale"/"partial" vocabulary.
3. **Give every blinded detector a typed replacement before the graceful path ships.** A typed
   field, not free text — free-form observations are for humans, and no gate greps prose.
4. **Test the replacement at the detector's altitude, not the fix's.** A test proving the timeout
   fires says nothing about whether the ladder still de-escalates.

Note the asymmetry that makes this easy to miss in review: the change is *local* (one await gets a
deadline) and the damage is *remote* (two scans in a different module go quiet). Nothing in the
diff of the fix touches the scans, so a reviewer reading only the diff cannot see it. The question
has to be asked deliberately.

## Why this matters

This is a **fail-open in a fail-closed system**, and it is the most dangerous kind: every
individual component behaves exactly as written. `scan_limit_events` correctly finds no residue.
`readiness_verdict` correctly counts zero aborted runs. The session correctly finalizes. The gate
correctly passes. The system as a whole has stopped protecting anything, and nothing anywhere
reports an error.

It is also a fix-shaped regression, which review is poorly calibrated for. The change reads as
strictly-better ("it used to hang, now it finalizes"), so the reviewer's attention goes to whether
the new mechanism works — not to what the old mechanism's failure was quietly doing for someone
else.

## The fix, as an example of the replacement shape

The typed carrier goes on the artifact the scans already read, alongside the fields they already
consume (`teardown_retries`, `dedup_hits`), so it inherits their absent-vs-zero discipline:

```rust
// adapters/nautilus/lab/src/artifacts/data_quality.rs
// Absent on a backtest or a pre-hard-stop artifact, never `false`.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub hard_stopped: Option<bool>,
```

Then each blinded scan gets an arm, sited next to the residue arm it succeeds so the relationship
survives a future reader:

```rust
// adapters/nautilus/lab/src/dispatch/ladder.rs:184
// The typed successor to the `tmp_residue` arm below it.
if dq.hard_stopped.unwrap_or(false) {
    out.push(ev("hard_stop"));
}

// adapters/nautilus/lab/src/dispatch/readiness.rs
// `hard_stopped` joins the existing safety-signal disjunction.
let safety_tripped = catalog.aborted_runs > 0
    || catalog.sessions.iter().any(|s| {
        s.dedup_hits > 0 || s.teardown_retries > 1 || s.twin_failed || s.hard_stopped
    });
```

And the test asserts the *detector's* behavior, explicitly pinning that the residue is gone — which
is what makes the test fail loudly if someone later "simplifies" the typed field away:

```rust
assert_eq!(
    catalog.aborted_runs, 0,
    "the hard-stopped run FINALIZED — there is no `.tmp-` residue to catch it"
);
assert_eq!(v, ReadinessVerdict::Red, "and it still reds the window on its own");
```

## When to apply

Any change described as "now it fails cleanly", "no more orphaned X", "graceful shutdown",
"timeout instead of hang", or "we now always write the record". The stronger the cleanup story, the
more likely something was detecting the mess.

The inverse is also worth remembering when *authoring* a detector: keying a health check on an
artifact's absence or incompleteness silently couples it to the current failure shape. Keying it on
a typed event survives the day someone improves that failure.

## Related

- `docs/solutions/architecture-patterns/live-session-teardown-must-share-the-nodes-arcs-and-capture-handles-before-build.md`
  — the same driver's four other fail-opens. Shares this doc's theme: in a fail-closed system the
  characteristic bug is not an error, it is a safety act that runs and protects nothing.
- `docs/solutions/architecture-patterns/retiring-a-feature-flag-arm-makes-its-behavior-newly-live.md`
  — the sibling shape on the other side: removing a *guard* rather than removing *debris*.
- `adapters/nautilus/lab/RUNBOOK-rung1.md` § "One operator-visible residual, and the backstop that
  covers the other" — the operator-facing statement, including that a hard-stop is still a limit
  event.
