---
title: "This repo has NO required CI checks — a squash-merge lands with adapter-check still pending, so the real merge gate is human attestation + make merge-block-check, not a green CI badge"
date: 2026-07-21
category: workflow-issues
module: "release / merge workflow — main (unprotected), .github/workflows/adapter-check.yml (adapter-check + mechanical merge-block jobs), make merge-block-check / make adapter-check"
problem_type: workflow_issue
component: development_workflow
severity: high
applies_when:
  - "About to merge a PR and reasoning about what 'green' means before landing it"
  - "Trusting the GitHub merge button / a green (or pending) CI badge as the gate that a change is verified"
  - "Retiring a Legacy fallback or deleting a per-consumer weekday primitive gated behind a recorded gate-verdict (issue #189 arc and its successors)"
related_components:
  - tooling
  - testing_framework
tags:
  - "ci"
  - "required-checks"
  - "branch-protection"
  - "merge-gate"
  - "merge-block"
  - "adapter-check"
  - "human-attestation"
---

# This repo has no required CI checks — the real merge gate is attestation + `make merge-block-check`

## Context

`.github/workflows/adapter-check.yml` runs two jobs on every push and PR — the
standalone adapter workspace gate (`cargo test --workspace` in `adapters/nautilus`)
and the mechanical **merge-block** (`cargo test -p nautilus-ls-calendar --test
merge_block -- --ignored`). It is easy to read that and assume "CI is green" is the
gate that stops an unverified change from landing.

It is not. The `main` branch is **not protected** — `gh api
repos/.../branches/main/protection` returns `404 Branch not protected`, and the repo
declares **no required status checks**. GitHub therefore lets you **squash-merge a PR
while `adapter-check` is still pending or running** (during the #185–#189 KRX-calendar
arc, PR #202 was squash-merged with its adapter-check run still in flight — this is
expected, not a mistake). A green badge, a pending badge, and no badge at all are all
mergeable states. CI here is an *informational signal that runs after the fact*, not a
merge blocker.

## Guidance

**Treat CI adapter-check as a correctness *signal*, never as the *gate*.** The gate that
actually authorizes a merge is two human/mechanical things, in this order:

1. **Human attestation** — a person confirms the change is verified: the relevant tests
   were run green locally (or the CI adapter-check run was watched to green *before*
   clicking merge, by choice — nothing forces the wait), the diff matches intent, and any
   live-certified behavior was actually exercised. The offline gate never runs live smokes,
   so "gate green" is *certified-offline*, not *live-certified* — see
   `../architecture-patterns/legacy-shadow-enforced-adoption-gate-playbook.md`.
2. **`make merge-block-check`** — the *mechanical* guard for any diff that retires a
   Legacy fallback. It runs the `#[ignore]`d tree-state coupling test
   (`cargo test -p nautilus-ls-calendar --test merge_block -- --ignored`, `Makefile:1133`)
   and **fails a staged retirement diff that deletes a consumer's weekday primitive without
   a present-and-`PASS` gate-verdict record — even when `make adapter-check` stays green.**
   This is a technical gate, not reviewer discipline; the PASS verdict is written only after
   the live, operator-attended Consumer Retirement Gate.

`make merge-block-check` *is* wired into `adapter-check.yml` (it runs as the second CI
job), but because that workflow is not a required check, CI running it does not *block*
the merge either. So run it (and `make adapter-check`) **locally, before you merge** — do
not outsource the retirement guard to a CI job that can't stop the button.

## Why This Matters

The failure mode is quiet and asymmetric: nothing in the GitHub UI turns red or greys out
the merge button while adapter-check is pending, so "I can merge" reads as "this is
verified" when the two are unrelated. A change that reddens the adapter workspace — the
classic case is an SDK-side preflight/constraint-schema edit that silently breaks the
standalone adapter (see
`cross-workspace-gate-blind-spot-sdk-preflight-changes-redden-adapter.md`) — can be
squash-merged in the window before CI finishes, and the red only surfaces on `main`
afterward. Likewise a retirement diff that removes a Legacy fallback without its recorded
PASS verdict is caught *only* by `merge-block-check`; if you lean on "CI is green" and CI
hadn't finished (or you never required it), the guard never fired at merge time.

## When to Apply

- **Every merge.** Before clicking squash-merge, confirm the verification you're attesting
  to actually happened — don't infer it from the badge state. If you want CI's adapter-check
  to be the evidence, *watch it to green first*; the repo won't make you.
- **Any Legacy-retirement / weekday-primitive-deletion diff.** Run `make merge-block-check`
  locally and confirm exit 0 before merging, in addition to `make adapter-check`.
- **When onboarding or automating** anything that "waits for required checks" — there are
  none here, so a bot that merges on "all required checks passed" would merge immediately.
  The human attestation step is load-bearing precisely because the mechanical enforcement
  is opt-in.

## Examples

**Confirming there are no required checks:**

```sh
$ gh api repos/sunkeunchoi/korea-adapter-sdk-ls/branches/main/protection
gh: Branch not protected (HTTP 404)      # → no required status checks; merge is never blocked by CI
```

**The mechanical gate that CI cannot substitute for** (`Makefile:1133`, mirrored as the
second job in `.github/workflows/adapter-check.yml`):

```sh
# Fails a retirement diff that deletes a weekday primitive with no PASS gate-verdict,
# even while `make adapter-check` is fully green:
$ make merge-block-check
cd adapters/nautilus && cargo test -p nautilus-ls-calendar --test merge_block -- --ignored
```

**The observed sequence during the #189 arc:** PR #202 squash-merged with adapter-check
still pending → the merge landed on `main` regardless → the operator's attestation (tests
watched green, no Legacy retirement in that diff) was the actual gate, and `make
merge-block-check` was the mechanical backstop for the retirement PRs earlier in the arc.
