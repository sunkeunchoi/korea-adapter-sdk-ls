---
title: "A coverage-only change is verified by MUTATION, not by the gate — the suite was green before and after, so green proves nothing"
date: 2026-07-27
category: conventions
problem_type: convention
module: "any test-only PR; adapters/nautilus (make adapter-check), root cargo test"
severity: medium
applies_when:
  - "Shipping a PR that adds or strengthens tests without changing production behavior"
  - "Reviewing a test-only diff and looking for the evidence that it works"
  - "A new test guards a bug that was already fixed (a regression guard)"
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

## When to Apply

- Any PR whose diff is tests, fixtures, or mocks only
- Adding a regression guard for an already-fixed bug
- Reviewing a test-only diff — ask for the mutation result, not the passed count
- Strengthening an existing assertion (was the old one already sufficient? mutate and see)

## Related

- [`wire-shape-fixture-for-string-or-number-must-be-a-quoted-string`](wire-shape-fixture-for-string-or-number-must-be-a-quoted-string.md)
  — a fixture that silently tested nothing; found while writing the tests this note is about
- [`never-re-check-rsp-cd-in-a-consumer-dispatch-already-classified-it`](never-re-check-rsp-cd-in-a-consumer-dispatch-already-classified-it.md)
  — the bug whose regression guard the first mutation above validates
