# Focused-Evidence Freshness Rule

This records the freshness policy for **Focused Evidence** on a **Recommended TR**.
It describes both the *intended* freshness machinery and — candidly — how little of
it is enforced by code today. Every claim below is checkable against the source.

## What is operative today

There is exactly one operative control: **human review discipline**, anchored on
the manually-set per-TR `maintenance.last_reviewed` date. A maintainer attests a
TR's evidence on a dated run and records that date; nothing else is automated.

Concretely, **no code enforces any of the controls described in the next section**:

- **The 90-day backstop is not computed.** No code reads `maintenance.last_reviewed`
  or compares it against today. The field is an input waiting for an evaluator, not
  a wired trigger.
- **Change-driven evidence invalidation is not wired.** No code path emits a
  `Severity::Evidence` finding (`crates/ls-trackers/src/types.rs` declares the variant
  but states it is unreachable — mirror that candor). No tracker stales evidence.
- **Spec-doc (example) findings never gate.** The Specification Document Tracker
  emits advisory findings only (`gates: false` by construction); they do not stale
  evidence and never will without new code.

The Specification Document Tracker and the API Drift Tracker (with their reviewed
baselines) **do** exist and **do** see changes — that much is now true, and is why
the previous "the tracker does not exist" framing is retracted. But seeing a change
and *acting on it to revoke a claim* are different things, and only the former is
wired.

## Intended semantics (documented intent — no enforcing code, no tests)

When an evidence-freshness evaluator is built (deferred — see below), these are the
semantics it should implement:

1. **Change-driven invalidation.** A maintained-TR **Structural API Shape** change
   (API Drift Tracker) affecting a Recommended TR stales its Focused Evidence and
   revokes the claim pending review.

2. **Example findings stay advisory.** A Specification Document Tracker example
   finding on a Recommended TR raises a human review obligation but never
   automatically stales evidence or gates verification. This preserves the existing
   invariant that example findings never gate.

3. **Informational changes never stale.** A description or `korean_name` change is
   informational and leaves the claim unaffected.

4. **90-day backstop.** Absent any qualifying change, Focused Evidence stays valid
   for **90 days** from `maintenance.last_reviewed`; after that an
   `evidence`-severity finding fires and the claim must be re-attested. The backstop
   catches behavior drift the trackers cannot see (session quirks, account-state
   edge cases).

5. **How they combine.** Focused Evidence is valid until either a qualifying
   structural change fires **or** 90 days elapse from `maintenance.last_reviewed`,
   whichever comes first.

These five points are intent, not behavior. None of them is enforced and none is
tested today.

## What stays intentionally deferred

- The **evidence-freshness evaluator** itself: parsing `last_reviewed`, computing
  the backstop, emitting `Severity::Evidence`, and mapping a spec-doc finding to
  evidence-staling. A `last_reviewed`-only backstop is the cheap half and the single
  piece that would give a Recommended claim any automated revocation.
- **Per-class freshness tightening.** With exactly one Recommended TR (`token`) the
  policy has a single subject; the 90-day default applies and per-class tightening
  (e.g. a shorter window for `orders`) stays deferred until those classes have
  recommended TRs.
