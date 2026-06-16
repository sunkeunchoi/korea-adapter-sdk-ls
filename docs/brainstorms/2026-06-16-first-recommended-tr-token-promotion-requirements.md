---
title: "First Recommended TR — promote token and turn on evidence freshness"
date: 2026-06-16
topic: first-recommended-tr-token-promotion
type: requirements
origin: brainstorm from docs/plans/2026-06-16-007-post-pr5-migration-status-what.md (improvement #3, #2)
---

# First Recommended TR — promote `token` and turn on evidence freshness

## Summary

Promote `token` to the first **Recommended TR**, and in the same move make the
evidence-freshness machinery operative for the first time. The work pins what
**Focused Evidence** backs the claim, how fresh it must stay, and which
**Tracker Findings** revoke it — then updates the freshness policy, which still
states change-driven invalidation is inactive because the Specification Document
Tracker does not exist. This is the first time `recommended: true` is true
anywhere; the audience is a future maintainer re-entering the project, not
external SDK consumers.

> **Addendum (2026-06-16, added during planning).** Research while writing
> `docs/plans/2026-06-16-008-feat-token-recommended-promotion-plan.md` falsified
> this doc's "turn evidence freshness on / change-driven invalidation is ACTIVE"
> premise: PR #5 shipped the *trackers*, but no evaluator code reads
> `last_reviewed`, computes the 90-day backstop, or emits `Severity::Evidence`.
> Accordingly **R6 and R8 (the "active" wording) and AE1–AE4 are reclassified as
> deferred intent**, not satisfied as written — the policy was rewritten to state
> the truth instead. See the plan's *Requirements Traceability* and *Scope
> Boundaries* for the authoritative mapping.

## Problem Frame

After PR #1–#5 the maintained SDK has six **Implemented TRs** but no
**Recommended TR**. That keeps the project honest, but it also means the central
architectural bet — the Implemented→Recommended ladder with evidence-driven
freshness — has never run once. `support.recommended` is `false` on every TR,
and the freshness controls have no subject to act on.

The freshness policy is also now stale, and verifiably so.
`metadata/EVIDENCE-FRESHNESS.md` and the `metadata/tr-index.yaml` header both
state that change-driven invalidation (R8/R10) is INACTIVE "because the
Specification Document Tracker and its reviewed baselines... are not part of this
slice." PR #5 shipped that tracker. The documents now describe a world that no
longer exists — they will mislead a future maintainer about which freshness
controls are live.

These two facts are coupled. Refreshing the freshness policy in the abstract,
with zero Recommended TRs, is just wording — the machinery has nothing to
invalidate. Promoting one narrow TR is what gives change-driven invalidation a
real subject and proves the whole lifecycle end to end. So the right move is to
do both at once: promote `token` and turn the policy on against it.

## Key Decisions

- **`token` is the first Recommended TR, not `t1102` or `revoke`.** The first
  promotion isolates the lifecycle mechanics from session/evidence complexity:
  `token` is `standalone`, OAuth-only, paper-compatible, has no
  caller-supplied identifier and no venue/session timing. Proving the machine on
  the narrowest claim comes before proving it handles harder cases. `t1102` is
  the intended *second* promotion because it exercises `market_session` /
  `krx_regular` session-dependence.

- **Promotion bundles the evidence-policy turn-on.** Flipping `token` to
  recommended and flipping the project's freshness policy from "change-driven
  invalidation inactive" to "active" are one move, because the policy is dormant
  until a Recommended TR exists.

- **Spec-doc (example) findings stay advisory even for a Recommended TR.** A
  Specification Document Tracker example finding on `token` creates a human
  *review obligation*, but never automatically stales Focused Evidence. Only a
  maintained-TR **Structural API Shape** change (API Drift Tracker) auto-stales.
  This preserves the existing invariant that example findings never gate ordinary
  verification.

- **No per-class freshness tightening for `token`.** The 90-day backstop default
  applies unchanged; the auth bucket does not warrant a tighter window. Tightening
  is reserved for classes like `orders` when they gain recommended behavior.

- **No new live-smoke code; the existing default smoke already produces the
  evidence.** `live_smoke_default` (`make live-smoke`) already issues an OAuth
  token and records its result credential-free. The promotion reuses that run as
  `token`'s Focused Evidence rather than authoring a token-specific smoke. The
  gap is durability and attribution, not capture — see R3.

## Requirements

### Promotion

- R1. `token`'s `support.recommended` becomes `true`; no other TR's recommended
  state changes in this work.
- R2. `token`'s **Focused Evidence** is its passing automated tests (the
  change-scoped gate for `token` / the `standalone` class) plus the OAuth token
  issuance from the default **Paper Live Smoke** (`live_smoke_default`), whose
  target, inputs, and result are already recorded credential-free.
- R3. That smoke result is persisted as a durable, `token`-attributable Focused
  Evidence record — today it is emitted only to stdout under a combined
  `live-smoke` target — and `token`'s `maintenance.last_reviewed` is set to the
  run date so the 90-day backstop has a true anchor.
- R4. No real-credential, order-capable, or session-timed evidence is required
  for `token`'s claim; Paper OAuth is the correct evidence level for an auth TR.
- R5. A single user-facing statement becomes true on promotion and is recorded
  where future-you will find it: `token` is a Recommended TR — its current
  Focused Evidence is strong enough to rely on without re-verifying, until a
  qualifying tracker finding or the 90-day backstop revokes the claim.

### Freshness policy (turn-on)

- R6. `metadata/EVIDENCE-FRESHNESS.md` and the `metadata/tr-index.yaml` header
  are updated to state that change-driven invalidation (the policy's R8/R10
  controls) is ACTIVE now that both the API Drift Tracker and the Specification
  Document Tracker, with their reviewed baselines, exist.
- R7. The policy names exactly which findings can stale Focused Evidence:
  a maintained-TR Structural API Shape change stales it; an example finding is an
  advisory review obligation that does not stale it; description / `korean_name`
  changes are informational and never stale it.
- R8. The policy states how the two controls combine: Focused Evidence is valid
  until either a qualifying structural change fires or 90 days elapse from
  `maintenance.last_reviewed`, whichever comes first.
- R9. The policy states what remains intentionally inactive: with exactly one
  Recommended TR the machinery operates on a single subject, and per-class
  freshness tightening stays deferred until those classes have recommended TRs.

## Acceptance Examples

- AE1. **Covers R7, R8.** **Given** `token` is Recommended with current Focused
  Evidence, **when** the API Drift Tracker reports a Structural API Shape change
  affecting `token`, **then** `token`'s Focused Evidence is stale and the claim
  is revoked pending review.
- AE2. **Covers R7.** **Given** `token` is Recommended, **when** the
  Specification Document Tracker emits an example finding pointing at `token`'s
  maintained artifacts, **then** a review obligation is raised but the Recommended
  claim and its evidence remain valid.
- AE3. **Covers R8.** **Given** no qualifying change has fired, **when** 90 days
  pass from `token`'s `maintenance.last_reviewed`, **then** an `evidence`-severity
  finding fires and the claim must be re-attested.
- AE4. **Covers R7.** **Given** `token` is Recommended, **when** only a
  description or `korean_name` change is detected, **then** the finding is
  informational and the claim is unaffected.

## Scope Boundaries

### Deferred for later

- The second promotion (`t1102`, then others) — proves session-dependence after
  `token` proves the mechanics.
- Migration-ownership notice in `korea-broker-sdk-ls` (#1) and the public
  orientation document (#9) — real future-you hygiene, sequenced as their own
  moves.
- Tracker-finding → **SDK Maintenance Work Item** workflow (#6) and provisional-
  baseline re-attestation (#7).

### Outside this move's identity

- No batch promotion of the implemented TRs; the first promotion is one narrow
  claim by design.
- No automatic mutation of SDK code, metadata, docs, or baselines by either
  tracker.
- No tighter-than-90-day freshness window introduced for any class here.

## Outstanding Questions

### Deferred to planning

- Q1. How the recorded token smoke result becomes a durable, `token`-attributable
  Focused Evidence record (R3) — extract a token-specific evidence line, persist
  the smoke output to a referenceable artifact, or another mechanism. The default
  smoke currently emits a combined `live-smoke` line to stdout only.
- Q2. Where the user-facing Recommended statement (R5) is recorded so future-you
  encounters it — metadata field, generated SDK Reference Doc, or both.
- Q3. The exact form of the active-vs-inactive policy wording in
  `EVIDENCE-FRESHNESS.md` and the index header (R6, R9).

## Dependencies / Assumptions

- Assumes the Specification Document Tracker and API Drift Tracker reviewed
  baselines from PR #4–#5 are present and operable, so change-driven invalidation
  has real inputs.
- Assumes `token`'s automated tests exist and pass as part of the standalone-class
  change-scoped gate.
- Assumes there are no external SDK consumers in the relevant horizon, so the
  Recommended claim is an internal honesty contract rather than a published
  guarantee.

## Sources

- `docs/plans/2026-06-16-007-post-pr5-migration-status-what.md` — origin status
  doc (improvements #2 and #3).
- `metadata/EVIDENCE-FRESHNESS.md` — current (stale) freshness policy; the R8/R9/R10
  control names.
- `metadata/tr-index.yaml`, `metadata/trs/token.yaml` — `support.recommended`
  and `maintenance.last_reviewed` fields the promotion touches.
- `CONTEXT.md` — vocabulary and the rule that a Credentialed/Paper Live Smoke
  becomes Focused Evidence only when target, inputs, and result are recorded.
