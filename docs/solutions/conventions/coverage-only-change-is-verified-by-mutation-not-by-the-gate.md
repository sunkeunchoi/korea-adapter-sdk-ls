---
title: "A coverage-only change is verified by MUTATION, not by the gate — the suite was green before and after, so green proves nothing"
date: 2026-07-27
last_updated: 2026-08-05
category: conventions
problem_type: convention
module: "any test-only PR; adapters/nautilus (make adapter-check, make script-check), root cargo test"
severity: medium
applies_when:
  - "Shipping a PR that adds or strengthens tests without changing production behavior"
  - "Reviewing a test-only diff and looking for the evidence that it works"
  - "A new test guards a bug that was already fixed (a regression guard)"
  - "REMOVING or collapsing an assertion — coverage removal is as gate-invisible as coverage addition"
  - "Working in a harness that drives a script against a throwaway fixture, where the mutation can live in the suite permanently"
related_components:
  - nautilus-ls-lab
  - ls-core
  - ls-sdk
tags:
  - testing
  - verification
  - gate
  - mutation-testing
  - silent-pass
  - regression-guard
  - meta-test
  - shell-harness
---

## Context

This repo's standing proof of correctness is the gate: run `cargo test` (or
`make adapter-check`), count the `test result` lines, confirm every one reads `0 failed`.
That protocol is sound for a behavior change — the diff moves the code, and the gate tells you
whether anything broke.

It tells you **nothing** about a coverage-only change.

A test-only diff is green before it lands and green after. The gate cannot distinguish twelve
carefully targeted tests from twelve `assert!(true)`; both add passing lines to the count. The
usual signal — "the number went up and nothing went red" — is satisfied identically by a test
that guards a real invariant and by one that guards nothing at all. Worse, the passed-count
delta *looks* like evidence, which is how an untested test suite acquires a reputation for
coverage it does not have.

This is sharpest for a **regression guard**, the most common reason to add a test after the
fact. The bug is already fixed, so the guard passes on arrival. Whether it would have caught
the original bug is exactly the question the gate does not answer.

## Guidance

**Verify a coverage-only change by mutating the production code and confirming the intended
test goes red — then revert the mutation.** Green is the control; red under mutation is the
measurement.

For each load-bearing test, name the production behavior it claims to protect, break precisely
that behavior, and check two things:

1. **The intended test reds.** The guard actually observes the thing it names.
2. **Nothing else moves.** The test is targeted, not incidentally coupled to unrelated
   behavior — a mutation that reds half the suite tells you little about any one test.

Then restore the file and re-run the gate on the final bytes, so what ships is what was
measured. Keep a pre-mutation copy rather than hand-reverting; an un-reverted mutant that
survives review is a live production defect wearing a green gate.

Not every test needs this — an obvious assertion over a pure function does not. Spend it on
the tests whose failure would matter: regression guards, fail-closed refusals, and anything
covering a path that decides real-world behavior.

**Coverage REMOVAL needs it too.** Deleting or collapsing an assertion is equally invisible to the
gate — the suite is green before and after, and the passed count moving *down* looks like exactly
what you intended. The question "would the surviving assertion still catch the thing the deleted one
named?" is the same question a regression guard poses, and only a mutation answers it.

### Prefer a PERMANENT mutation over a one-time one

The `cp` / apply / revert recipe above is a *measurement taken once*, at review time. Its result
lives in the PR body and then decays: nothing re-runs it, so a later refactor can quietly make the
guard unfalsifiable again and no gate notices.

When the harness drives its subject through a **disposable copy**, the mutation can instead live in
the suite permanently, and the falsifier re-runs forever. In
`adapters/nautilus/scripts/tests/session-morning.test.sh`, `make_fixture [sed_expr]`
(`:124-126`) builds a throwaway fixture repo under `mktemp -d`, then applies the sed while *copying*
the script into it — `sed "$mutation" "$REAL_SCRIPT" >"$naut/scripts/session-morning.sh"` (`:161`),
which reads the tracked file and redirects into the fixture. No write to the tracked file exists
anywhere in the harness, so `run_chain_mutated` (`:352-356`) never touches the working tree:

```sh
run_chain_mutated() {
  local mutation="$1"; shift
  CHAIN_ROOT="$(make_fixture "$mutation")"
  _run_in "$CHAIN_ROOT" "$@"
}
```

That single property — mutate a copy, not the file under version control — is what turns "revert it
before you commit" into "ship it and let it run every gate". The harness now carries **eleven** such
meta-tests. Two caveats, both load-bearing:

- **The mutation must fail closed on a zero match.** These sed expressions are text-exact (some
  line-anchored, some merely literal) and carry no replacement-count guard, so a refactor that
  reformats the target silently mutates nothing. That is only safe because the *unmutated* script
  produces the assertion's red condition anyway — a no-op mutation reds the meta-test rather than
  greening it. That property was checked to hold for all eleven. Verify it before relying on the
  pattern; a meta-test that greens on a failed mutation asserts nothing. The `:1228` site states the
  reasoning in-tree: asserting the exact exit code rather than `!= 41` "is what stops a broken
  fixture from greening this by failing early."
- **A permanent falsifier is only as good as its oracle.** Four of the eleven assert only
  `rejected*` — the step [3] pair (`:529`, `:545`) and the step [4] pair (`:565`, `:583`) — so they
  would pass against a binary that rejects *everything*. The mutation fires correctly, but the
  assertion cannot tell the intended cause from a wrong one. See
  [`assert-on-a-fact-the-parent-emits-not-the-childs-own-marker`](assert-on-a-fact-the-parent-emits-not-the-childs-own-marker.md).

## Why This Matters

The failure mode is **silent** and **long-lived**. A test that cannot fail is worse than no
test, because it stops the next person from looking: the path reads as covered, the coverage
tooling agrees, and the gap is only discovered when the regression it was supposed to catch
reaches production instead.

Test code is also where this repo's normal safety net does not apply. A wrong line in `orb.rs`
moves the head hash and forces a re-baseline; a wrong line in a test moves nothing and is
invisible to every gate, count, and hash the repo maintains.

## Examples

From the `fetch_today_opens` coverage PR (#219) — the live `today_open` fetch that decides
what an attended session buys. All 64 suites were green both before and after the twelve new
tests, so the gate was not the evidence. The mutations were:

| mutation applied to `mount_universe.rs` | expected to red |
| --- | --- |
| reintroduce `if resp.rsp_cd != "00000" { bail!() }` | the absent-`rsp_cd` and `00136` tests |
| replace `format!("{:0>6}", row.shcode.trim())` with `row.shcode.trim().to_string()` | the number-typed-echo test |

Both mutations red exactly the named tests and nothing else, which is what made the regression
guard for #218 credible. The first mutation is the important one: that guard exists solely to
stop a removed bug from returning, so it passes on a healthy tree by construction, and only a
mutation can show it is load-bearing.

```bash
cp lab/src/runner/mount_universe.rs "$SCRATCH/mount_universe.rs.orig"   # control
# ...apply mutation, run the targeted test filter, read which tests red...
cp "$SCRATCH/mount_universe.rs.orig" lab/src/runner/mount_universe.rs   # restore
rg -n MUTANT lab/src/runner/mount_universe.rs                           # prove it is gone
```

### A permanent falsifier for a coverage REMOVAL (PR #261)

The flake fix in PR #261 *deleted* an assertion arm — the purest coverage-only diff there is, and
one where the passed count going down is the intended outcome. The evidence shipped as a permanent
meta-test rather than a one-time measurement
(`adapters/nautilus/scripts/tests/session-morning.test.sh:1238-1260`):

```sh
run_chain_mutated 's|^    kill "\$ingest_pid" 2>/dev/null; |    |'
```

Note what it disarms: the **kill call only**, leaving `wait "$ingest_pid"` in place. `wait` then
blocks until the 10s stub finishes, so the stub reaches its `COMPLETED` marker — the surviving
assertion's red condition — while the exit code and the stand-down report are asserted **unmoved**.

That scoping is rule 2 ("nothing else moves") applied at authoring time rather than checked
afterward. Disarming the whole LATE branch would have moved all three assertions at once and proved
little about any one of them; disarming the single mechanism the surviving assertion names proves
exactly that assertion is load-bearing.

## When to Apply

- Any PR whose diff is tests, fixtures, or mocks only
- Adding a regression guard for an already-fixed bug
- **Deleting or collapsing an assertion** — ask whether the survivor still catches what the deleted
  arm named
- Reviewing a test-only diff — ask for the mutation result, not the passed count
- Strengthening an existing assertion (was the old one already sufficient? mutate and see)
- Working in a fixture-copy harness — prefer shipping the mutation as a permanent meta-test over
  measuring it once

## Related

- [`assert-on-a-fact-the-parent-emits-not-the-childs-own-marker`](assert-on-a-fact-the-parent-emits-not-the-childs-own-marker.md)
  — the coverage removal whose permanent meta-test is the example above, and the reason a falsifier's
  own oracle needs scrutiny
- [`wire-shape-fixture-for-string-or-number-must-be-a-quoted-string`](wire-shape-fixture-for-string-or-number-must-be-a-quoted-string.md)
  — a fixture that silently tested nothing; found while writing the tests this note is about
- [`never-re-check-rsp-cd-in-a-consumer-dispatch-already-classified-it`](never-re-check-rsp-cd-in-a-consumer-dispatch-already-classified-it.md)
  — the bug whose regression guard the first mutation above validates
- [`shell-script-live-path-needs-stubbed-binary-tests`](../workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md)
  — the doc that built the fixture-copy harness and introduced `run_chain_mutated`
