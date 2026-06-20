---
title: "Shell scripts with a pure core + external I/O need stubbed-binary live-path tests"
date: 2026-06-20
category: workflow-issues
module: .github/scripts (freshness-cadence CI tooling)
problem_type: workflow_issue
component: testing_framework
severity: high
applies_when:
  - "Writing a bash script that separates a pure decision core from external I/O (gh, curl, aws, kubectl)"
  - "The script runs under `set -euo pipefail` in CI and a non-zero exit triggers an alert path"
  - "Tests only exercise a `--dry-run` / mocked-input mode and never the real I/O path"
tags:
  - bash
  - shell-testing
  - set-e
  - subshell
  - process-substitution
  - ci
  - github-actions
  - gh-cli
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

## Related

- `.github/scripts/update-freshness-issue.sh` and its tests in
  `.github/scripts/tests/` (the script this learning came from).
- [`makefile-include-env-quotes-gateway-403.md`](../integration-issues/makefile-include-env-quotes-gateway-403.md)
  — a sibling shell/make quoting gotcha where a non-obvious quoting rule produced
  a silent wrong value; same family of "the shell did something subtle and the
  green path hid it."
