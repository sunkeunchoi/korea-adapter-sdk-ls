# Focused-Evidence Freshness Rule

This records the freshness policy for **Focused Evidence** on a **Recommended TR**.
It describes both the *intended* freshness machinery and — candidly — how little of
it is enforced by code today. Every claim below is checkable against the source.

## What is operative today

Two controls are operative:

1. **Human review discipline**, anchored on the manually-set per-TR
   `maintenance.last_reviewed` date. A maintainer attests a TR's evidence on a dated
   run and records that date.
2. **The 90-day backstop is computed and enforced** by the evidence-freshness
   evaluator. `make freshness-check` (`ls-trackers freshness check`) loads metadata,
   evaluates each Recommended TR's `last_reviewed` against today (UTC), and emits an
   advisory `Severity::Evidence` finding for any past the window — exactly `> 90` days
   is stale; 90 is still fresh. `ls-docgen` renders the deterministic review-by date
   (`last_reviewed` + 90 days) into each recommendation contract, so the freshness
   bound is visible in docs without breaking byte-determinism. The evaluator mutates
   nothing; clearing is recompute-on-invocation (re-attest, then the next run finds the
   TR fresh).

The backstop is **advisory, not gating**: `Severity::Evidence` sits below
`Severity::Maintenance`, so `gates_for` never trips on it and `freshness-check` exits
`0` even on stale evidence. It makes a lapsed recommendation *visible*; a human
re-attests.

What is **not** yet wired:

- **Change-driven evidence invalidation.** No code path stales evidence from a
  maintained-TR Structural API Shape change on a Recommended TR (the heavier half —
  deferred). The API Drift Tracker and the Specification Document Tracker (with their
  reviewed baselines) **do** see changes, but *acting on a change to revoke a claim* is
  not wired; only the 90-day half above acts.
- **Spec-doc (example) findings never gate.** The Specification Document Tracker emits
  advisory findings only (`gates: false` by construction); they do not stale evidence
  and never will without new code.

## Intended semantics (documented intent — no enforcing code, no tests)

These are the full intended semantics. Point 4 (the 90-day backstop) is now
**implemented and tested** by the freshness evaluator; the change-driven points remain
intent until that increment ships:

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

Of these, the **90-day backstop (4) is enforced and tested today** via the freshness
evaluator, and its `last_reviewed` arm of the combine rule (5) is live. The
change-driven points (1, the structural arm of 5) and the advisory spec-doc point (2)
remain intent — change-driven invalidation is not yet wired. The informational rule (3)
holds by construction (no code stales on description / `korean_name`).

## What stays intentionally deferred

- **Change-driven evidence invalidation** — the heavier half of the evaluator: mapping
  a maintained-TR Structural API Shape change (or a spec-doc finding) to evidence-staling
  and emitting `Severity::Evidence` on that path. The cheap `last_reviewed`-only backstop
  is built (`make freshness-check`); this change-driven half remains deferred.
- **Per-class freshness tightening.** With six Recommended TRs (`token`, `t1101`,
  `t1102`, `t8412`, `S3_`, `CSPAQ12200`) spanning five classes (standalone,
  market_session, paginated, realtime, account), the 90-day default applies uniformly;
  per-class tightening (e.g. a shorter window for `orders`) stays deferred until those
  classes have recommended TRs.
