---
title: "Shell scripts with a pure core + external I/O need stubbed-binary live-path tests"
date: 2026-06-20
category: workflow-issues
module: .github/scripts (freshness-cadence CI tooling), adapters/nautilus/scripts (operator session scripts)
problem_type: workflow_issue
component: testing_framework
severity: high
last_updated: 2026-07-31
applies_when:
  - "Writing a bash script that separates a pure decision core from external I/O (gh, curl, aws, kubectl, or a prebuilt binary under target/debug)"
  - "The script runs under `set -euo pipefail` in CI and a non-zero exit triggers an alert path"
  - "Tests only exercise a `--dry-run` / mocked-input mode and never the real I/O path"
  - "Writing an attended operator script whose exit code IS its contract (a GO/NO-GO or stand-down report)"
  - "Writing a stub that stands in for a binary whose argument contract the script must satisfy"
  - "A dry-run mode prints a hand-maintained transcript of the commands rather than constructing the argv the live path uses"
tags:
  - bash
  - shell-testing
  - set-e
  - subshell
  - process-substitution
  - ci
  - github-actions
  - gh-cli
  - operator-scripts
  - exit-code-contract
  - argv-contract
  - stub-fidelity
  - test-oracle
---

# Shell scripts with a pure core + external I/O need stubbed-binary live-path tests

## Context

The scheduled freshness-cadence work added `.github/scripts/update-freshness-issue.sh`,
which upserts a rolling GitHub issue via the `gh` CLI. Following good practice it
split a **pure decision core** (`decide_action`, marker parsing) from the **`gh`
I/O**, and shipped 21 green tests — all exercising `--dry-run` with mocked issue
state. Code review then found **two production-breaking bugs that the 21 green
tests could not see**, because both lived exclusively on the live (`gh`-calling)
path that `--dry-run` never executes. Both are generic bash traps, not
domain-specific.

## Guidance

When a shell script has a pure core plus an external-binary I/O path, **add at
least one test that runs the real script end-to-end with the external binary
stubbed on `PATH`** — do not rely on dry-run/mocked-input tests alone. A stub is
a tiny executable that logs its arguments and returns canned output:

```bash
# Put a fake `gh` first on PATH; it logs every call and emits canned JSON.
make_fake_gh() {
  mkdir -p "$1/bin"
  cat >"$1/bin/gh" <<'GH'
#!/usr/bin/env bash
echo "gh $*" >> "$GH_FAKE_LOG"
if [ "$1" = "issue" ] && [ "$2" = "list" ]; then cat "$GH_FAKE_LIST"; fi
exit 0
GH
  chmod +x "$1/bin/gh"
}
# ...then: PATH="$tmp/bin:$PATH" bash "$SCRIPT" "$args"; rc=$?
# Assert BOTH the exit code (rc) and which commands the stub logged.
```

The two specific bash traps this catches:

### Trap 1 — a global set inside `<(...)` process substitution never reaches the parent

`read x y < <(fn)` runs `fn` in a **subshell**, so any variable `fn` assigns to a
global is lost when the subshell exits. Capture everything `fn` produces on its
**stdout** instead, via a normal command substitution whose status you check:

```bash
# BAD: resolve_issue() sets a global RESOLVED_BODY internally.
read -r state number < <(resolve_issue)      # subshell — RESOLVED_BODY stays empty
prior=$(parse "$RESOLVED_BODY")               # always parses ""

# GOOD: the function prints state+number on line 1 and the body on lines 2+.
info=$(resolve_issue) || { echo "resolve failed" >&2; exit 2; }
read -r state number <<<"$(printf '%s\n' "$info" | sed -n '1p')"
prior=$(parse "$(printf '%s\n' "$info" | tail -n +2)")
```

### Trap 2 — `[ test ] && cmd` as a function's last statement exits non-zero under `set -e`

When the `[ test ]` is false, the `&&` compound evaluates to exit status 1. If
that line is the last statement of the function/script, the **whole script exits
1** under `set -e` — even on a perfectly healthy run. Use an explicit `if`:

```bash
# BAD: when action != "notify" (the common steady-state), main returns 1.
[ "$action" = "notify" ] && gh issue comment "$n" --body "$msg"

# GOOD:
if [ "$action" = "notify" ]; then
  gh issue comment "$n" --body "$msg"
fi
```

## Why This Matters

Both bugs were not just latent — they inverted the script's contract on every
real run, and the test suite reported all-green:

- Trap 1 made the prior-state always empty, so the "notify only on a *new*
  transition" rule degraded to **notify every single run** (maintainer spam).
- Trap 2 made the silent steady-state branch return 1, so a healthy run looked
  like a failure and would have **tripped the CI failure-alert path** (a watcher
  "death" notification on a watcher that was fine).

Dry-run tests give false confidence precisely because they bypass the I/O path —
which is where the subshell, exit-status, and argument-marshalling traps live. A
green dry-run suite says the *decision logic* is right; it says nothing about
whether the script *exits correctly* or *threads real I/O output* through.

## When to Apply

- Any bash script split into a pure core + external-command I/O, especially one
  whose tests are dry-run/mock-only.
- Any CI shell step under `set -euo pipefail` where a spurious non-zero exit
  triggers a notification or gate.
- Reviewing a shell script: check the **last statement of every branch/function**
  for a bare `[ ] && cmd`, and check every `< <(fn)` for globals expected to
  escape the subshell.

## Examples

The live-path regression tests that now guard both traps assert the exit code
*and* the stub's call log — the silent-edit case must exit 0 **and** must not have
called `gh issue comment`:

```bash
run_live "$fixture" '[{"number":7,"state":"OPEN","body":"<!-- ... t1102 -->"}]'
assert_eq "silent edit exits 0" "0" "$LIVE_RC"          # catches Trap 2
case "$LIVE_LOG" in
  *"issue comment"*) fail "silent edit must NOT notify" ;;  # catches Trap 1
  *) ok ;;
esac
```

Either assertion alone catches one bug; together they pin both. Neither was
expressible in the dry-run suite, which never reaches the `gh` branches.

## Recurrence: operator scripts, 2026-07-29

This learning was re-derived the expensive way outside `.github/scripts`, which is why the
`module:` and `applies_when` above now name operator scripts too.

The script in question is `adapters/nautilus/scripts/session-morning.sh` — an operator
session-morning chain driver. (It was uncommitted when this section was first written; it has
since landed and the path resolves.) It was written with exactly the shape this doc warns
about: a pure decision core
(`pace_verdict`) covered by a `--self-test`, a live I/O path covered only by `--dry-run`, and
no stubbed-binary layer between them. Code review then
found a P0 and six P1s, **all of them in the untested live path** — including a branch that
exited `0` (the script's documented GO code) on any `lab-mount-universe` failure, and a
stand-down path that killed the ingest and told the operator a re-run was safe while leaving
the lock that makes a re-run refuse.

Two things generalize beyond that script:

- **An exit code that encodes a verdict is a contract, and the dry-run cannot test it.** When
  `0` means GO and `40` means stand down, every non-zero branch of the real path needs a case.
  A dry run exercises none of them, because it never runs the thing that can fail.
- **The plan can point the wrong way.** The unit's execution note explicitly said to prefer a
  dry-run pass over unit coverage. That was reasonable-sounding and wrong, and nobody checked
  whether the repo had already learned otherwise. Worth searching `docs/solutions/` for the
  shape of the work, not only its domain — this doc was filed under CI tooling and the work
  was an operator script, so a domain-only search misses it.

## Second recurrence, 2026-07-31 — the prescribed remedy was never applied

The section above named the script and prescribed the fix. Nothing was written. Two days later
`session-morning.sh` ran for real for the first time, died about ten seconds into step `[3]`,
and never advanced the calendar; the chain had to be completed by hand from the runbook.

It was missing two required arguments to the same binary, both refused **before any network
call**: `--window` (required; the `--krx-through` it did pass is optional and defaults to the
window end, so the fetch range had no start) and `--state-root` (absent, so output confinement
fell back to a root resolved *relative to the current directory* and refused the absolute
output paths from every directory except one). `git log -S` showed neither flag was ever
present — an authoring omission, not a regression.

Neither was reachable by the existing checks, and the reason is worth stating flatly: **a dry
run cannot detect a missing required argument, because it never invokes the parser.** This
script's `--dry-run` prints a hand-maintained heredoc *describing* the commands, so the
transcript and the live invocation are two independent copies. Both omitted `--window`. The
dry run rendered an invalid command and exited 0.

## Stubbing alone is insufficient — validate the argv against the real contract

This is the sharper form of the lesson, and it corrects the remedy at the top of this doc.

The prescription above — stub the binary, assert the exit code *and* the stub's call log — was
applied here, faithfully, and still produced a false green. With `--state` deleted from the
script's invocation, the suite reported **8 passed, 0 failed**. The real binary rejects that
argv outright.

The reason is structural: asserting *that* a binary was called with *some* argv does not
validate the argv against the binary's actual contract. Worse, a stub that hand-mirrors the
contract (`if --window is absent, exit 1`) is a **second implementation of that contract**, and
it drifts from the first. A stale mirror greens the guard on exactly the argv the real binary
rejects — which is the failure mode the guard exists to prevent, reintroduced by the guard.

**Use the real binary as the contract oracle, disarmed.** Most CLIs validate in stages: parse
arguments, resolve and confine paths, check credentials, and only then perform I/O. That
ordering is exploitable. Strip the credential from the environment and the real binary becomes
a pure argv validator — it either rejects the argv (contract violated) or runs out of things to
check and stops at the credential refusal (contract satisfied), with no network traffic and no
side effects. Capture the argv the script actually marshalled from the stub log, then replay it:

```bash
# The stub logs argv; the REAL binary judges it. Credentials stripped, so the
# replay stops at the credential refusal -- before any client is constructed.
# Run from a FOREIGN cwd: that is what catches a cwd-relative default.
replay() {
  local out
  out="$(cd / && env -u APP_API_KEY "$REAL_BIN" $1 2>&1)"
  case "$out" in
    *"must be set"*) printf 'accepted' ;;   # parsing + confinement both passed
    *)               printf 'rejected: %s' "$out" ;;
  esac
}
```

Two properties make this better than a mirror. It **cannot drift** — the oracle is the same
code the script will call in production. And running it from a *different* working directory
than the script used is what surfaces cwd-dependent defaults; a same-cwd replay silently
inherits the one directory where the invocation happens to work.

**Then prove the harness can fail.** A guard nobody has seen fail is a guard nobody has tested.
Add negative meta-tests that mutate the script on purpose — delete a required argument, delete
the confinement flag — and assert the harness *rejects* the result:

```bash
run_chain_mutated '/--state-root "\$STATE"/d'
case "$(replay "$(fetch_argv_from_log "$CHAIN_LOG")")" in
  rejected*) ok "harness detects a step stripped of --state-root" ;;
  accepted)  no "harness detects a step stripped of --state-root" ;;
esac
```

Without these, a permissive stub is undetectable and the suite's green is unfalsifiable. Both
defects above survived a stub-based suite that asserted exit codes and call logs; both are
caught by the replay, and the meta-tests are what prove it.

Working reference: `adapters/nautilus/scripts/tests/session-morning.test.sh`, run by
`make script-check`. It runs the real script (symlinked, never copied) in a throwaway fixture
repo against stubs for the network path, then replays the captured argv against the real
compiled binary. It also states its scope limit out loud — the steps it never reaches — rather
than leaving the reader to infer coverage it does not have.

## A doc's remedy text, hardcoded into a caller's error handler, becomes a misattribution bug

Step `[3]`'s failure handler was a single `die` whose message was lifted almost verbatim from
the remedy in
[`krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot`](../integration-issues/krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md)
— "raise the HTTP timeout and re-run; the checkpoint resumes." That guidance is correct for the
failure it was written about. Applied unconditionally to *every* failure of the step, it turned
an argument-parse error into a network-degradation story with a remedy that cannot work, under
a 30-minute clock.

The trap generalizes past this pair of docs: **a solution doc describes one failure mode; an
error handler must discriminate among all of them.** Copying the remedy sentence into a handler
silently converts "here is what this specific failure means" into "here is what every failure
of this step means." When a handler covers a step with several distinct failure modes, it
should point at the underlying tool's own message and enumerate the alternatives, rather than
assert the one cause its author had in mind.

## Related

- `.github/scripts/update-freshness-issue.sh` and its tests in
  `.github/scripts/tests/` (the script this learning came from).
- [`krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md`](../integration-issues/krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md)
  — the doc whose remedy text was hardcoded into a caller's error handler; see the
  misattribution section above.
- [`operator-shell-ls-env-makes-the-adapter-suite-look-red-on-pristine-main.md`](../test-failures/operator-shell-ls-env-makes-the-adapter-suite-look-red-on-pristine-main.md)
  — surfaced during the same 2026-07-29 session; the environment half of "state outside the
  script decides its outcome."
- [`makefile-include-env-quotes-gateway-403.md`](../integration-issues/makefile-include-env-quotes-gateway-403.md)
  — a sibling shell/make quoting gotcha where a non-obvious quoting rule produced
  a silent wrong value; same family of "the shell did something subtle and the
  green path hid it."
