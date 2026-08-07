---
title: "Assert on a fact the PARENT emits — a child's self-reported marker races the child's own startup"
date: 2026-08-05
category: conventions
module: "adapters/nautilus/scripts (session-morning.sh live-path harness, make script-check)"
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Writing or reviewing an assertion that reads a marker a spawned child process writes about itself"
  - "The behavior under test is the parent starting, killing, or signalling a child"
  - "Triaging an intermittent failure in `make script-check` or session-morning.test.sh"
  - "Tempted to fix a subprocess-marker flake by raising a poll interval or adding a second marker"
  - "Reading a recorded cause — in a comment, a queue note, or a commit message — as input to a fix"
tags:
  - flaky-test
  - subprocess-race
  - test-oracle
  - shell-harness
  - script-check
  - session-morning
  - sigterm
  - mutation-testing
---

# Assert on a fact the PARENT emits — a child's self-reported marker races the child's own startup

## Context

`make script-check` (`Makefile:1176`) runs `adapters/nautilus/scripts/tests/session-morning.test.sh`,
which drives the morning chain `adapters/nautilus/scripts/session-morning.sh` against stubbed
binaries in a throwaway fixture repo. One of its step [7] assertions —
`normal mode: the stalled ingest is killed` — failed roughly **1 run in 3: 4 failures across 11
runs on 2026-08-04**, loaded and unloaded alike. That flake was the stated blocker on wiring
`script-check` into `make gate-run`, because importing a 1-in-3 flake into the commit gate is how a
gate stops being believed.

**The recorded cause was wrong, in two places.** The `Makefile` block and the queue item
`gate-run-wire-script-check` both said the same thing: *the step [7] poll races the stub's own 10s
sleep*. That explanation predicts the stub reached its `COMPLETED` marker before the kill landed —
which is the assertion's own red condition.

Every observed failure showed the **opposite** branch. There was no `ls-ingest` line in the stub log
at all, while those same runs reported exit 40 and `STAND DOWN — not on pace`. The pre-fix `case` had
three arms, and the one that fired was the catch-all reading an empty log as *never started*:

```sh
case "$CHAIN_LOG" in
  *"ls-ingest COMPLETED"*) no  ... "no COMPLETED marker" ;;
  *"ls-ingest "*)          ok  ... ;;
  *)                       no  ... "ls-ingest to have been started" ;;   # ← this one
esac
```

The ingest **was** launched and it **was** killed. What was lost was the *stub's own start marker*.
The stub writes that marker as its first act (`session-morning.test.sh:253`,
`echo "ls-ingest $*" >>"$STUB_LOG"`) and installs its `TERM` trap only at `:259` — six lines later,
as the first executable statement inside the `secs != 0` branch. The fixture sets
`LS_SM_POLL_SECS=1` against an already-elapsed `LS_SM_INGEST_BY=00:00` (`:1119-1126`), so the first
poll kills roughly a second after launch — and **bash process startup can exceed that second**.
SIGTERM legitimately arrives before the marker exists and before the trap does. The assertion was
measuring process-startup latency, not kill behavior.

## Guidance

**Before theorizing about timing on a flaky assertion, ask who emits the fact being asserted on.**
A marker written by the *child* is observable only if the child lives long enough to write it, so it
is entangled with scheduling. A line emitted by the *parent* — an exit code, a report line — is
emitted on the parent's own control flow and cannot be pre-empted by the child's startup.
**Prefer the parent-emitted fact.**

Corollary: prefer a **negative** assertion whose red condition is a marker whose *presence* is
unambiguous, over a positive assertion that a marker is present. "`COMPLETED` present ⇒ the kill did
not land" is a fact the race cannot fabricate; "start marker absent ⇒ never started" is a fact the
race fabricates about once in three runs.

This narrows — it does not contradict — the rule in
[`shell-script-live-path-needs-stubbed-binary-tests`](../workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md),
which prescribes asserting *both* the exit code and which commands the stub logged. The asymmetry:

| stub-log assertion | safe? |
| --- | --- |
| **Negative** — a forbidden call must be ABSENT | Safe. A call that never happened cannot appear. |
| **Positive** — an expected marker must be PRESENT | Safe **only** when the parent cannot terminate the child before it writes. |

When the behavior under test *is* the parent killing the child, the stub log is the wrong oracle.

The asymmetry also settles which fixes are worth trying. Two "obvious" ones would **not** have
worked here:

- **Add a `KILLED` marker to the stub's trap.** Any positive marker the stub could write races
  identically — the trap is not installed yet when the signal arrives.
- **Raise the poll interval.** That widens the window but does not close it, and it trades a flake
  for a slower suite. The failing runs were both loaded and unloaded, so there is no interval that is
  provably enough.

**Test the proposed fix against the runs that failed.** If a candidate fix would still have been
racing on those runs, it is a delay, not a fix.

Finally: a collapse that removes an arm is **coverage-only**, so a green run proves nothing about it.
It needs mutation evidence — see
[`coverage-only-change-is-verified-by-mutation-not-by-the-gate`](coverage-only-change-is-verified-by-mutation-not-by-the-gate.md).

## Why This Matters

A flaky assertion that names the right behavior costs more than a missing one, because it buys a
wrong story. Here the wrong story was written down twice and survived long enough to become a stated
prerequisite in the work queue — and the fix it implied (chase the poll/sleep interaction) would not
have removed the flake.

The deeper cost is scope. `script-check` is the morning chain's only live-path harness: it is what
stands between a reworded probe literal and a hard exit 64 on the 08:45 chain, when clock is the
scarcest resource in the day. A harness that flakes 1-in-3 cannot be a gate step, so the flake was
suppressing coverage of a path with a real deadline attached, not merely annoying a developer.

And the general shape recurs. On 2026-08-05 the same harness picked up a **sharper instance of the
same family**: the two negative meta-tests for the step [4] `calendar-refresh` argv replay assert
only `rejected*` (`session-morning.test.sh:565` and `:583`) when replaying a mutated argv against a
real compiled binary — so they would *pass* against an impostor binary that rejects everything. The
mechanism is different (no race is involved), but the family is the same: **the marker you assert on
can be satisfied by the wrong cause.** A race is one way the marker lies about its cause; an
over-broad oracle is another.

## When to Apply

- Triaging any intermittent assertion over a subprocess, background job, or spawned container —
  before reaching for sleeps, retries, or longer intervals.
- Writing a test that asserts a process was *started*, *killed*, or *signalled*. Ask what the parent
  emits on that path and assert on that instead.
- Reviewing a fix for a flake: check that the proposed fix would have survived the specific runs that
  failed, and that the fact it asserts on has only one possible cause.
- Reading a recorded cause as input to a fix. **Reproduce before trusting it.** This one was recorded
  wrong in two places and was falsified by reproduction, not inherited.

## Examples

### The fix (PR #261, merged 2026-08-05)

Drop the `*"ls-ingest "*)` arm and its catch-all counterpart, leaving the `COMPLETED` arm and a
catch-all that passes (`session-morning.test.sh:1147-1151`):

```sh
case "$CHAIN_LOG" in
  *"ls-ingest COMPLETED"*)
    no "normal mode: the stalled ingest is killed" "no COMPLETED marker" "$CHAIN_LOG" ;;
  *) ok "normal mode: the stalled ingest is killed" ;;
esac
```

What survives is the one fact the race cannot fabricate: a `COMPLETED` marker means the kill did
**not** land.

The launch and the kill are proven instead by two sibling assertions in the same block that cannot
race the stub's startup — **exit 40** and **`STAND DOWN — not on pace`** — both emitted by the
*parent script*, inside the LATE branch and after `kill "$ingest_pid"` (`session-morning.sh:1004`;
`step "STAND DOWN — not on pace"` at `:1014`, `exit 40` at `:1028`). Two other `exit 40` sites exist
(`:806`, `:1066`) but neither is reachable in this block — `:806` needs a witness-probe rc=10, which
the fixture forecloses by pre-seeding a POSITIVE witness line on every build
(`session-morning.test.sh:170-172`), and `:1066` needs an elapsed universe deadline, which *this
block* pins to 23:59 (`:1144`) and which the LATE-branch `exit 40` pre-empts anyway. (Elsewhere in
the suite `:1227` pins `00:00` deliberately, to reach `:1066` — the unreachability is a property of
this fixture block, not of the suite.) **Both sibling assertions
passed on every failing run** — that is the evidence the collapse lost no coverage, and it is the
same evidence that falsified the recorded cause.

> **Line-number note.** The in-tree comment and PR #261's commit message both cite
> `session-morning.sh:1012` for the stand-down line. That was accurate when written; commit
> `008859d` shifted it to `:1014` three commits later (two after PR #261 landed). Cite the
> construct, not just the line.

### The mutation that makes the collapse credible

The diff is coverage-only, so its evidence is a permanent falsifier, not a green run
(`session-morning.test.sh:1238-1260`). It disarms the **kill call**, not the LATE branch:

```sh
run_chain_mutated 's|^    kill "\$ingest_pid" 2>/dev/null; |    |'
```

Deleting `kill "$ingest_pid"` while leaving `wait "$ingest_pid"` in place holds the rest of the block
still: `wait` now blocks until the 10s stub finishes, so the stub reaches its `COMPLETED` marker —
the surviving assertion's red condition — while the exit code and the stand-down report are asserted
**unmoved**. Disarming the whole LATE branch would have moved all three at once and proved little
about any one of them. That scoping choice is the reusable part: mutate the single mechanism the
surviving assertion names, and assert its siblings did not move.

### Verification

`make script-check` green across a **20-run soak (20/20)**, 88 passed / 0 failed each at the time (93
after the later `calendar-refresh` replay landed). Eleven of those runs were concurrent with a full
root `cargo test` on the same machine — 219–252s against 53–98s unloaded, so the load was real.
Pre-fix baseline: **4 failures in 11**.

### A residual, recorded honestly

The collapsed assertion's soundness rests on `STAND DOWN — not on pace` being unique to the post-kill
LATE branch of `session-morning.sh`. That is true today and is checked by inspection, but it is **not
itself pinned by a test** — a future second emitter of that string would weaken the sibling assertion
without reddening anything.

## Related

- [`coverage-only-change-is-verified-by-mutation-not-by-the-gate`](coverage-only-change-is-verified-by-mutation-not-by-the-gate.md)
  — why the arm-removal needed a mutation, not a green run. This case extends it in three ways:
  coverage *removal* is as gate-invisible as coverage addition; a **permanent** in-suite meta-test is
  an alternative to a temporary apply-and-revert; and its "nothing else moves" rule is exactly why
  the mutation disarmed only the kill call.
- [`shell-script-live-path-needs-stubbed-binary-tests`](../workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md)
  — the doc that built this harness and introduced `run_chain_mutated`. Its "assert the exit code AND
  the stub's call log" prescription is narrowed by the positive/negative asymmetry above.
- [`nautilus-livenode-tests-race-on-the-global-logger-init`](../test-failures/nautilus-livenode-tests-race-on-the-global-logger-init.md)
  — the repo's other flaky-test doc, and the useful contrast. That race is **deterministic per (SHA,
  CI-env)** in-process global state, so re-running the same commit cannot flip it; this one is
  genuinely non-deterministic and load-independent. Both were misattributed until reproduced.
- [`first-run-of-a-new-guard-prove-the-binary-then-discharge-its-residual`](../workflow-issues/first-run-of-a-new-guard-prove-the-binary-then-discharge-its-residual.md)
  — the same harness on a different axis (binary freshness and guard presence, not assertion
  reliability).
- [`krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot`](../integration-issues/krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md)
  — the same absent-signal-read-as-a-negative confusion, at the fetch layer instead of the assertion
  layer.
- [`status-only-gate-is-not-evidence-and-all-over-empty-is-true`](../logic-errors/status-only-gate-is-not-evidence-and-all-over-empty-is-true.md)
  — an assertion satisfied by the absence of evidence rather than by the behavior under test.
