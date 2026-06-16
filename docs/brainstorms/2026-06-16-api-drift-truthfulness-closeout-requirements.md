---
title: "API Drift Tracker — baseline truthfulness + governance closeout"
type: requirements
date: 2026-06-16
status: resolved
origin: docs/plans/2026-06-16-003-post-pr3-migration-status-what.md
builds_on:
  - docs/brainstorms/2026-06-16-api-drift-scope-pressure-test-requirements.md
---

# API Drift Tracker — baseline truthfulness + governance closeout

## Purpose

The next item finishes PR #3's maintenance quality before tracker scope expands.
It closes the **known untruths** in the committed **Reviewed Baseline** and
**resolves the two undecided governance policies** left open after PR #3. It is
deliberately *not* robustness hardening and *not* new tracker capability.

This is the smallest slice that restores trust in the committed API Drift
baseline. Everything that is only robustness is carried as explicitly-named debt
rather than bundled in.

## Problem Frame

PR #3 shipped real API Drift staged review with a committed baseline, but left
three trust gaps:

- The committed **Structural API Shape** coverage omits the `token` `scope`
  field — a concrete untruth the baseline currently encodes.
- A whole-inventory **facts** outage is warned at fetch time but does not affect
  the gate, so a degraded fetch can silently weaken a comparison the tracker is
  trusted to make. This covers more than one facts dependency: an endpoint/rate
  facts outage, and the **property-type mapping** outage — when the mapping API
  fails the normalizer silently falls back to a hardcoded table, so maintained
  fields normalize to raw type codes and diff as false `FieldChanged` breaking
  findings. Both are the same class of silent-degradation untruth.
- The first code-set seed (~365 TR codes, derived once from the **Migration
  Source**) is still provisional, with no decided stance on how that gap is
  owned.

Left unaddressed, the baseline carries a known-false shape and an undecided
failure mode — which erodes trust in an opt-in tracker, and a distrusted tracker
is an unrun tracker.

## Scope

### In scope

- **R1 — `token` `scope` coverage.** Add the `token` `scope` field to
  **Structural API Shape** coverage for the `token` TR.
- **R2 — Reviewed Baseline refresh.** Refresh the committed **Reviewed Baseline**
  after R1 as a one-time reviewed correction. This is a human-reviewed baseline
  update, not the introduction of automatic baseline promotion. R2 includes
  re-syncing any existing token-shape acceptance fixtures/tests that encode the
  pre-`scope` shape — the refresh is not complete until those pass against the
  corrected shape, so the fixture update is in scope, not an implicit expansion.
- **R3 — Support-aware facts-outage gate.** When a whole-inventory **facts**
  dependency (endpoint/rate facts or the property-type mapping) degrades or
  outages:
  - exit `2` (error) when the degradation affects a **maintained / baselined**
    TR;
  - exit `0` + a visible **Tracker Finding** when only untracked inventory is
    affected.

  This mirrors the existing support-aware exit contract (D1/D3): the tracker
  fails only when degraded facts could corrupt a comparison it actually gates on.

  Two contract points the gate must honor — both are why R3 is a real
  requirement, not a relabel:
  - **Granularity is per-group, not per-TR.** Facts degradation is observable at
    group granularity (a group whose protocol/mapping facts failed), and each
    maintained TR belongs to a group it may share with untracked TRs. So the
    `exit 0 / untracked-only` branch is reachable only when **no maintained TR
    shares the degraded group**; otherwise the degradation affects a baselined TR
    and the gate exits `2`. R3's contract is defined at group granularity — the
    success criterion must exercise the discriminating case, not only the
    always-`2` case.
  - **The gate intercepts before normalize/compare.** A degraded maintained-TR
    fetch currently flows through normalization and surfaces as spurious
    `EndpointChanged` / `RateLimitChanged` / `ProtocolChanged` (and, via the
    property-type fallback, `FieldChanged`) findings that already gate at exit
    `1` with a false "removed/changed" signal. R3 must intercept degradation
    **before** the compare step so it produces the support-aware exit `2`
    instead of layering a second, contradictory signal on top of the existing
    false-drift path.
- **R4 — Provisional seed carried visibly.** Mark the code-set seed explicitly
  provisional in-artifact and record it as a named, tracked residual. Rely on D5's
  incremental new-TR review loop for ongoing re-attestation. No new live or
  credentialed step is added.
- **R5 — Network-free default stays clean.** Ordinary verification remains
  network-free; the new R3 gate fires only on the live/staged fetch path.
- **R6 — Live review stays opt-in.** Operator-run API Drift review remains
  opt-in; no scheduled automation is added.

### Out of scope — carried as explicitly-named debt

These are real follow-ups, tracked and named, not silently dropped:

- serde forward-compatibility hardening
- duplicate same-code / same-group edge handling

### Out of scope — unchanged from prior decisions

- No mutation of SDK code, metadata, docs, or committed baselines beyond the
  single reviewed R2 correction.
- No Recommended TR promotion, order runtime behavior, or **Specification
  Document Tracker** work (the following item, not this one).
- No rename fingerprinting; no scheduled cron/CI automation.

## Success Criteria

- The committed baseline's **Structural API Shape** for `token` includes `scope`,
  and the baseline is clean against itself after the refresh (`api-drift check
  --staged` exits `0`, no drift).
- A simulated whole-inventory facts outage on a baselined TR exits `2`; an outage
  touching only untracked inventory exits `0` with a visible finding.
- The **discriminating** case is exercised, not only the always-`2` case: a
  degradation confined to a group with no maintained TR exits `0` + finding,
  while a co-occurring degradation touching a maintained group exits `2`. A
  property-type mapping outage on a maintained TR exits `2` (not a false
  `FieldChanged` at exit `1`).
- The provisional seed is marked provisional in-artifact and appears as a named
  residual; the carried-debt items appear as named residuals.
- `cargo test --workspace` and `make docs-check` remain clean; network-free
  verification requires no network.

## Open Questions (planning-level, not product)

- **Facts attribution granularity (now bounded, not open-ended).** Facts
  degradation *is* representable — at **group** granularity (a group whose
  protocol/mapping facts failed), confirmed against the current fetch path. R3's
  contract is defined at that granularity (see R3). The remaining planning
  question is narrower: how often maintained TRs share a group with untracked
  TRs, since that determines how often the `exit 0 / untracked-only` branch is
  actually reachable versus how often any outage lands on a maintained group.
- How R3 intercepts degradation before normalize/compare without disturbing the
  existing clean-fetch drift path — a sequencing/mechanism detail for planning.

## Dependencies / Assumptions

- R4 assumes D5's new-TR review loop remains the real re-attestation mechanism;
  "closing governance" here means the gap is *named and owned*, not
  pretended-resolved. Independent attestation is treated as infeasible for a
  solo-maintainer project and deliberately not attempted.
- R3 assumes facts degradation is detectable distinctly from a menu/group parse
  failure (which already exits `2`), so the support-aware branch is reachable.
- The carried-debt items assume no upstream change forces them sooner; if real
  drift surfaces a duplicate same-code/group case, that residual is re-prioritized.

## Following Item

After this closeout, the next product expansion remains the **Specification
Document Tracker** (LS documentation changes as advisory tracked maintenance
signals), per the origin status doc. It is explicitly not part of this item.

## Deferred / Open Questions

### From 2026-06-16 doc review

Advisory items surfaced by the persona review, carried for planning awareness
(not blocking):

- **R4 de-scopes the origin's first attestation option deliberately.** The origin
  status doc offered "independent operator attestation **OR** keep the gap
  visible." R4 ships only the visibility branch. This is a recorded solo-maintainer
  decision, not an oversight — the independent-attestation branch is intentionally
  not attempted (see Dependencies / Assumptions).
- **Carried debt can itself produce baseline untruths — the trust/robustness
  boundary is pragmatic, not categorical.** Duplicate same-code/same-group
  handling can emit a false `Breaking` "field removed" via the remove+add reorder
  fallback, and serde gaps can mis-record an upstream shape. They are carried as
  debt because they are not the *known, currently-encoded* untruths this item
  closes — but if real drift exercises one of these paths, it is re-prioritized
  (consistent with the Dependencies / Assumptions note).
