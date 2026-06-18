# Broker SDK Decommission Closeout — Requirements

**Date:** 2026-06-18
**Status:** Ready for planning / execution
**Scope tier:** Standard (cross-repo operations sequence)

## Problem & Context

The adapter repo (`korea-adapter-sdk-ls`) has fully decommissioned the old
`korea-broker-sdk-ls` as a migration source. The decommission audit reached
**TRUSTWORTHY-GREEN** (26/26 rows confirmed, 0 acceptances), the evidence is
frozen under `docs/migration-source/audit/`, and ADR
`docs/adr/0014-migration-source-decommissioned.md` records the closed posture.

ADR 0014 explicitly states: *"Physically deleting or archiving the sibling
repository is an external operations act and is out of scope for this repo."*
**This effort is that out-of-scope external half.** The in-repo work is done;
what remains is communicating and enforcing that the old repo is no longer
ordinary reference material.

### Verified facts (grounding)

- The old repo **is** hosted remotely: `git@github.com:sunkeunchoi/korea-broker-sdk-ls`.
- Its `README.md` still opens as a live SDK (`ls-sdk = "0.3"`, quickstart) with
  **no** closeout notice.
- Local broker checkout has **only `main`** locally (clean, in sync); the **8
  other branches are remote-only** — archiving freezes them.
- **No active workflow reads the live sibling checkout at runtime.** The audit
  validator `crates/ls-trackers/tests/decommission_audit.rs` recomputes the gate
  from frozen in-repo records under `docs/migration-source/audit/`
  (manifest-relative paths). Every remaining `korea-broker-sdk-ls` mention in
  active code is either a guard test-fixture string or an attribution comment
  (e.g. `crates/ls-trackers/src/fetch.rs:3` "Ported from…"). → Removing the
  local checkout is verifiably safe.

## Goals

- Anyone who lands on the old repo immediately learns it is historical and where
  the maintained SDK lives.
- The old repo is enforced read-only so it cannot drift back into being live.
- The local sibling checkout is removed once confirmed unneeded (it is).
- Convenient leftovers (local branches) are tidied.

## Non-Goals

- No re-running or re-auditing the decommission gate — it is already
  TRUSTWORTHY-GREEN and recomputed in CI.
- No edits to ADR 0010/0014 or the frozen audit/ledger artifacts.
- No code changes in the adapter repo (the harness-scan-gap follow-up is a
  separate, non-blocking issue — see below).

## Requirements

Execute in this order; each step gates the next where noted.

### R1 — README closeout notice (broker repo) **[required]**

Add a top-of-README closeout banner stating: the maintained SDK is
`korea-adapter-sdk-ls`, this repo is **historical only**, and the decommission
audit is recorded in **PR #18 / `docs/migration-source/audit/`**.

**Also neutralize the now-misleading live instructions:** mark the
Installation / Quickstart (and any other "how to use this SDK" sections) as
historical / no-longer-maintained so a reader cannot follow stale steps. A
short redirect note ("Historical — do not use. See `korea-adapter-sdk-ls`.")
under each is sufficient; full deletion is not required.

Commit to `main` of `korea-broker-sdk-ls`. Banner shape (final text is a
planning/wording detail):

```
> ⚠️ DECOMMISSIONED — historical only.
> The maintained SDK is korea-adapter-sdk-ls.
> Decommission audit: PR #18 / docs/migration-source/audit/.
```

### R2 — Archive the GitHub repo **[required, after R1 merges]**

Make `sunkeunchoi/korea-broker-sdk-ls` read-only via **GitHub Archive**
(`gh repo archive …`). This freezes the 8 remote-only branches (satisfying the
remote half of branch cleanup), disables new issues/PRs, and is fully
reversible. Must land **after** R1 so the notice is the first thing an archived
visitor sees.

### R3 — Remove the local sibling checkout **[required, after R2]**

Delete `/Users/mini/dev/korea-broker-sdk-ls`. Safety is already verified (see
grounding) — no active workflow depends on it; the archived remote is the
canonical historical copy. Order after R2 so a read-only remote exists before
the local copy goes.

### R4 — File the harness-scan-gap follow-up issue **[optional, non-blocking]**

File a small issue in the **adapter** repo (`korea-adapter-sdk-ls`, *not* the
broker repo — its issues are disabled by R2, and the guard lives in adapter)
tracking **KTD-4**: the decommission guard in
`crates/ls-trackers/tests/decommission_audit.rs` does not scan the agent-harness
dirs (`.claude/`, `.agents/`, `.compound-engineering/`), so a future hardcoded
`~/dev/korea-broker-sdk-ls` path or "consult the old source" instruction added
there would be invisible to the guard. Documented as an accepted gap in
`docs/plans/2026-06-18-002-feat-physical-decommission-migration-source-plan.md`
(Open Questions). **Do not block decommission on this.**

### R5 — Clean stale local branches **[when convenient]**

In the **adapter** repo, delete the 4 local branches already merged into `main`:
`docs/maintenance-two-stage-foundation`, `feat/sdk-first-vertical-slice`,
`feat/t1101-recommended`, `feat/t1101-stage2-expansion`. The broker repo has no
stale *local* branches (only `main`); its remote branches are frozen by R2.
(`/ce-clean-gone-branches` can assist.)

## Success Criteria

- Opening the old repo's README (locally or on GitHub) shows the closeout notice
  at the top, and its install/usage sections no longer read as live instructions.
- `sunkeunchoi/korea-broker-sdk-ls` shows as **Archived** on GitHub and rejects
  pushes.
- `/Users/mini/dev/korea-broker-sdk-ls` no longer exists locally, and the adapter
  repo's CI / audit validator still passes (it never depended on the checkout).
- (If R4 taken) one tracking issue exists in the adapter repo for KTD-4.
- Adapter repo has no merged-but-undeleted local branches.

## Dependencies & Assumptions

- **Assumes** push access to `main` of the broker repo and `gh` auth with
  archive rights on `sunkeunchoi/korea-broker-sdk-ls`.
- **Assumes** the broker `main` is the branch that should carry the notice
  (verified: clean, in sync with origin).
- The audit gate precondition is already satisfied and CI-recomputed; this
  effort cites it, does not re-run it.

## Outstanding Questions

- **R4 scope (deferred, non-blocking):** whether to also *extend* the guard to
  scan the harness dirs, or just file the tracking issue. The plan's default is
  "leave as documented gap, revisit if product logic is added under those dirs."
  Decide if/when R4 is picked up.

## Handoff

Mechanical, well-bounded ops sequence — suitable for direct execution
(`/ce-work`) or a short `/ce-plan`. R1–R3 are the required spine; R4–R5 are
opportunistic.
