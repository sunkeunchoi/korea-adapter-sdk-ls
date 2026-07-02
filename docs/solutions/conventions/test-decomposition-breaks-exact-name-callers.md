---
title: "Decomposing a test file into submodules silently breaks external --exact name callers (Makefile live-smoke targets)"
date: 2026-07-02
category: conventions
module: Makefile
problem_type: convention
component: tooling
severity: high
applies_when:
  - "A Makefile / script / CI job invokes a specific test by bare name with `cargo test ... --exact <name>`"
  - "That test's file was (or will be) decomposed into per-family submodules, so its real `--list` path gained a `module::` prefix"
  - "A `make live-smoke-*` target prints `did not run (0 tests) or did not pass` even though the test exists and compiles"
  - "Auditing whether a test-tree refactor left any external references to bare test names stale"
tags:
  - ls-sdk
  - tests
  - cargo
  - makefile
  - module-decomposition
  - live-smoke
  - refactor
---

# Decomposing a test file into submodules silently breaks external `--exact` name callers

## Context

When a `crates/ls-sdk/tests/*.rs` integration-test monolith is decomposed into per-family
submodules (see
[`decomposing-rust-integration-test-monoliths`](../architecture-patterns/decomposing-rust-integration-test-monoliths.md)),
each moved test's `cargo test --list` path gains a `module::` prefix — `live_smoke_t0441`
becomes `account::live_smoke_t0441`. That doc notes the prefix is "run-semantics-preserving"
because `cargo test foo` still **substring**-matches `balance::foo`. That is true only for
*substring* invocation. It is **false** for any caller that passes `--exact`.

The repo's `make live-smoke-*` targets all funnel through a `run_smoke` helper that runs:

```make
cargo test -p ls-sdk --test live_smoke -- --ignored --exact --nocapture $(1)
```

with `$(1)` a bare test name (`live_smoke_t0441`). After decomposition the real test is
`account::live_smoke_t0441`, and `--exact` requires the **full module path** — so the bare
name matches **zero** tests. The helper then does `grep -q "1 passed"`, which fails on the
empty run, so the target exits `FAIL: … did not run (0 tests) or did not pass`. The target
is dead, but it *looks* like a red smoke, not a broken filter.

This is doubly silent: these targets are all `#[ignore]` live smokes never exercised by the
offline gate (`cargo test` / `make docs-check` / `make lane-check` all stayed green), so the
decomposition PR merged with **194 of 198** `run_smoke` call sites broken and nobody noticed
until a wave tried to run one.

## Guidance

**A test-tree decomposition is not internally contained — it is an API change for every
external caller that names a test with `--exact`.** Before decomposing (or when auditing a
decomposition), grep the whole repo — `Makefile`, `scripts/`, CI YAML — for bare test-name
references and repoint each to its real `--list` path.

Derive the correct name from the decomposed binary, never by guessing the prefix:

```bash
# The full path is whatever --list prints; --exact matches THAT, verbatim.
cargo test -p ls-sdk --test live_smoke -- --list | grep ': test$' | sed 's/: test//'
#  → account::live_smoke_t0441
#  → account::live_smoke_cspbq00200
#  → market_session_charts::live_smoke_nws_t3102
```

```make
# before (dead after decomposition — matches 0 tests under --exact)
live-smoke-t0441:
	$(call run_smoke,live_smoke_t0441)

# after
live-smoke-t0441:
	$(call run_smoke,account::live_smoke_t0441)
```

The `module::name` form works fine with `--exact` — it is the *bare* name that stops
matching. The already-decomposed order-smoke targets prove the pattern
(`fo::fo_position_manufacture_smoke`, `chain::order_smoke_matrix`).

### Audit one-liner

Cross-check every `run_smoke` argument against the real test-path set and report the count
that no longer matches:

```bash
grep -oE "run_smoke,[a-zA-Z0-9_:]+" Makefile | sed 's/run_smoke,//' | sort -u > /tmp/args
cargo test -p ls-sdk --test live_smoke -- --list | grep ': test$' | sed 's/: test//' | sort -u > /tmp/paths
# also list order_smoke / negative_probe binaries if their targets exist
broken=0; while read a; do grep -qxF "$a" /tmp/paths || broken=$((broken+1)); done < /tmp/args
echo "run_smoke args not matching an exact test path: $broken"
```

## Why This Matters

The `grep -q "1 passed"` guard was designed to fail closed on an empty run — which is
correct — but it cannot tell "the gateway rejected the smoke" from "the filter matched
nothing." A broken filter therefore masquerades as a legitimate smoke failure, so an
operator burns a live window diagnosing a gateway that was never contacted. And because live
smokes are `#[ignore]`, the offline gate gives zero protection: the only signal is a human
running the exact target long after the refactor merged. The blast radius is every external
`--exact` caller in the repo at once, not one test.

## When to Apply

- Any time you decompose (or rename modules within) a `tests/*.rs` file that external
  tooling names by test path — repoint the callers in the **same** PR and note the audit.
- When a `make live-smoke-*` (or any `--exact` cargo invocation) reports `0 tests` / `did
  not run`: suspect a bare-name-vs-`module::` mismatch before suspecting the gateway; run
  `--list` and compare.
- When reviewing a test-tree refactor: the internal base-name snapshot proves no test was
  dropped, but it does **not** prove external `--exact` references still resolve — that is a
  separate grep.

## Examples

Plan 2026-07-02-002 (domestic trigger-run certify wave) hit this when three wave-critical
targets — `live-smoke-t0441`, `live-smoke-cspbq00200`, `live-smoke-nws-t3102` — all matched
0 tests. Fixed to `account::live_smoke_t0441`, `account::live_smoke_cspbq00200`,
`market_session_charts::live_smoke_nws_t3102`. The audit one-liner then showed **194 of 198**
`run_smoke` sites still broken by the same mismatch (fallout of the plan 2026-06-30-005
decomposition), flagged as a follow-up mechanical sweep PR.

See also:
[`decomposing-rust-integration-test-monoliths`](../architecture-patterns/decomposing-rust-integration-test-monoliths.md)
(the decomposition mechanics; its "run-semantics-preserving" note is the substring-only
half of this story).
