---
title: Test a Makefile recipe guard offline by running the real recipe with a shimmed cargo
date: 2026-07-01
category: docs/solutions/architecture-patterns
module: smoke-harness
problem_type: architecture_pattern
component: testing_framework
severity: low
applies_when:
  - "A Makefile recipe embeds behavior (a guard, a branch, a fail-fast check) that no cargo test can reach"
  - "You want a regression net for that behavior without a live gateway / network call"
  - "The check would otherwise duplicate the guard's logic into the test and silently drift"
tags: [makefile, shell, offline-test, credential-lanes, gate, fail-fast, wrong-account]
---

# Test a Makefile recipe guard offline by running the real recipe with a shimmed cargo

## Context

The env-lane cutover (plan 2026-07-01-002) put a fail-fast guard inside the smoke
harness's `run_smoke` recipe: an empty `LS_SMOKE_LANE` resolves to `domestic`, the
recipe sources `.env.<lane>` and **exits non-zero if the lane file is missing** —
never falling back to a bare `.env` (the wrong-account hazard). That guard is the
only thing standing between a missing credential file and a smoke authenticating as
the wrong account, so it needs a regression net.

But the guard lives in a Makefile recipe, not in Rust. `cargo test` never sees it
(KTD5: the Makefile edits are behaviorally invisible to the workspace test suite),
and the smoke tests themselves are all `#[ignore]` + hit the live gateway. Two naive
options both fail: (a) re-implement the guard's shell logic inside a test script —
which then drifts from the real recipe the moment someone edits the Makefile; or
(b) run the real recipe — which sources creds and calls the live gateway.

## Guidance

Write an offline check that runs the **real, shipped recipe** — never a copy of its
logic — in an isolated temp dir, with a **shimmed `cargo` on `PATH`** so the recipe
reaches a green result without any network call:

1. `cp` the real `Makefile` into a `mktemp -d` working dir and `cd` there. Relative
   `./.env.<lane>` sourcing now resolves against the temp dir, so the repo's real
   credential files are invisible — the test controls exactly which lane files exist.
2. Put a fake `cargo` earlier on `PATH` that emits the one line the recipe greps for
   (`1 passed`) and exits 0. The recipe runs end-to-end — guard, sourcing, "test"
   invocation — but compiles nothing and touches no gateway.
3. Order the cases so the **missing-lane** cases run first (temp dir empty → guard
   fires), then create a dummy lane file and run the **present-lane** case last
   (guard passes, shim returns green).
4. Assert on the guard's observable contract: a missing file → non-zero exit + the
   `wrong-account hazard` message; a present file → exit 0, no hazard message.

Because the check drives the **actual recipe body**, the guard-under-test cannot
drift from the shipped guard — the failure mode of hand-copied test logic.

Then give the check a real home. A `.PHONY` target nobody runs is not a safety net.
When no CI runs the offline gate and there is no aggregate `make` target, the
**documented gate command-list** (here, `AGENTS.md`'s "Gate" block) is the natural
home: add `make lane-check` to the list the operator/agent already runs before
committing. That is the "wire it into the offline verification the gate already runs"
step — the documented routine, not a CI file, is what "the gate" means in this repo.

## Why This Matters

The whole value of a fail-fast guard is that a regression reintroducing the silent
fallback fails **loud**. If the guard's only test re-implements its logic, the test
keeps passing while the real recipe rots — the exact silent-drift the guard exists to
prevent, now one level up. Running the real recipe under a shim closes that gap at
near-zero cost (no compile, no network), and it stays correct through future edits to
the recipe because it never forked a copy of the logic.

The gate-wiring half matters just as much: a check that exists but is never invoked
provides false assurance. Three independent code-review lenses (maintainability,
project-standards, learnings) converged on this as the one residual risk of the
original cutover — the test was in place but ungated. Naming the documented gate
command-list as the home makes the net real without inventing CI the repo doesn't have.

## When to Apply

- Any Makefile/shell recipe that embeds a guard, branch, or fail-fast check that
  `cargo test` cannot reach and that a mistake would make silently wrong.
- When the honest alternatives are "duplicate the logic (drifts)" or "run it live
  (network/creds)" — the shim gives you the real recipe, offline.
- Pair it with adding the check to the repo's documented gate routine when no CI or
  aggregate target exists to run it automatically.

## Examples

`scripts/lane-fail-fast-check.sh` (the shipped check):

```bash
# cargo shim: pretend the #[ignore] smoke ran and passed (no network)
cat > "$work/bin/cargo" <<'SHIM'
#!/bin/sh
echo "test result: ok. 1 passed; 0 failed; 0 ignored"
exit 0
SHIM
chmod +x "$work/bin/cargo"
cp "$repo_root/Makefile" "$work/Makefile"
export PATH="$work/bin:$PATH"
run_make() { ( cd "$work" && make "$@" 2>&1 ); }

# Case A: missing default lane (temp dir has no .env.domestic yet) -> loud non-zero
out="$(run_make live-smoke)"; rc=$?
[ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q "wrong-account hazard" \
  || { echo "FAIL[A]"; fails=1; }

# ... Case B (overseas lane), Case B2 (order recipe) — all before the dummy file ...

# Case C: present lane file (dummy placeholders) -> guard passes, shim returns green
printf 'LS_TRADING_ENV=paper\nLS_PAPER_APIKEY=dummy\nLS_PAPER_SECRET=dummy\nLS_PAPER_ACCOUNT=0000000000\n' > "$work/.env.domestic"
out="$(run_make live-smoke)"; rc=$?
[ "$rc" -eq 0 ] && ! printf '%s' "$out" | grep -q "wrong-account hazard" \
  || { echo "FAIL[C]"; fails=1; }
```

Wiring it into the documented gate (`AGENTS.md`):

```
make docs            # regenerate docs/ from metadata
cargo test           # workspace
cargo test -p ls-core  # metadata validation + policy index cross-check
make docs-check      # assert generated docs match committed
make lane-check      # smoke-harness fail-fast lane guard (offline; no gateway)
```

## Related
- [ls-account-token-bound-credential-lanes](../conventions/ls-account-token-bound-credential-lanes.md) — the wrong-account hazard the guard closes
- [makefile-include-env-quotes-gateway-403](../integration-issues/makefile-include-env-quotes-gateway-403.md) — why lane loads stay shell-sourced (never make `include`)
- plan `docs/plans/2026-07-01-002-feat-env-lane-cutover-both-pool-flip-sweep-plan.md` (U2)
