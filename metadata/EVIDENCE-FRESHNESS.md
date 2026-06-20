# Focused-Evidence Freshness Rule

This records the freshness policy for **Focused Evidence** on a **Recommended TR**.
It describes both the *intended* freshness machinery and — candidly — how little of
it is enforced by code today. Every claim below is checkable against the source.

## What is operative today

Three controls are operative:

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
3. **Change-driven staling *detection* is computed and enforced** by the same
   evaluator. Each Recommended TR's evidence record carries a frozen `attested_shape`
   (the Structural API Shape it was attested against) plus the `attested_normalizer_version`
   it was captured under. The check diffs that attested shape against the current
   committed baseline shape using the drift tracker's `diff_shapes`, keeps only
   qualifying structural changes (`FieldAdded`, `FieldRemoved`, `FieldChanged`,
   `EndpointChanged`, `ProtocolChanged`), and emits an advisory `Severity::Evidence`
   finding when any survive. A pure `NORMALIZER_VERSION` representation shift never
   qualifies — a version mismatch routes the TR to a re-attestation advisory instead
   (so a normalizer bump cannot mass-stale every TR). Reads only committed artifacts;
   no network.

The check emits a `reasons` field per stale entry distinguishing `age` from `change`
(a TR stale for both carries `["age","change"]`). The two clear **independently**
(R10): refreshing `last_reviewed` clears age-staleness; re-pinning the attested shape
to the current baseline (`ls-trackers freshness re-pin <tr>`) clears change-staleness.
Refreshing the date alone does **not** clear change-staleness, and vice versa.

Both arms are **advisory, not gating**: `Severity::Evidence` sits below
`Severity::Maintenance`, so `gates_for` never trips and `freshness-check` exits `0`
even on stale evidence. The check also surfaces an advisory **baseline-staleness**
warning when the committed baseline's stamped `refreshed` date is older than 90 days
(change-detection is only as current as the committed baseline), and a
re-attestation advisory for any TR whose normalizer version mismatches or whose
baseline shape is missing. It makes a lapsed recommendation *visible*; a human
re-attests.

What is **not** yet wired:

- **Auto-revoking the claim on a change-driven structural change.** Detection ships
  (control 3 above), but a qualifying change does **not** automatically flip
  `support.recommended` or otherwise revoke the claim — a drifted TR keeps rendering
  as Recommended until a human re-attests or demotes it. That residual exposure is
  accepted and deferred (the "revoke pending review" arm).
- **Spec-doc (example) findings never gate.** The Specification Document Tracker emits
  advisory findings only (`gates: false` by construction); they do not stale evidence
  and never will without new code.

## Intended semantics

These are the full intended semantics. Points 1, 3, 4, and 5 are now **implemented and
tested**; only the *auto-revoke* arm of point 1 remains deferred:

1. **Change-driven invalidation.** A maintained-TR **Structural API Shape** change
   (API Drift baseline) affecting a Recommended TR stales its Focused Evidence
   (`reasons` gains `change`). **Detection is implemented and tested.** The further
   step of automatically *revoking the claim pending review* (flipping
   `support.recommended`) is deliberately deferred — staling is advisory; a human
   re-attests or demotes.

2. **Example findings stay advisory.** A Specification Document Tracker example
   finding on a Recommended TR raises a human review obligation but never
   automatically stales evidence or gates verification. This preserves the existing
   invariant that example findings never gate.

3. **Informational changes never stale.** A description, `korean_name`, rate-limit,
   reorder, or cross-block move is informational/non-qualifying and leaves the claim
   unaffected (filtered out by the R2 allow-list).

4. **90-day backstop.** Absent any qualifying change, Focused Evidence stays valid
   for **90 days** from `maintenance.last_reviewed`; after that an
   `evidence`-severity finding fires and the claim must be re-attested. The backstop
   catches behavior drift the trackers cannot see (session quirks, account-state
   edge cases).

5. **How they combine.** Focused Evidence is valid until either a qualifying
   structural change fires **or** 90 days elapse from `maintenance.last_reviewed`,
   whichever comes first. The two reasons clear independently (R10).

Of these, the **90-day backstop (4), change-driven detection (1), the combine rule
(5), and the informational exclusion (3) are all enforced and tested today** via the
freshness evaluator. The advisory spec-doc point (2) holds by construction. Only the
*auto-revoke* arm of (1) remains intent.

## What stays intentionally deferred

- **Auto-revoking the claim on a change-driven structural change** — the "revoke
  pending review" arm: flipping `support.recommended` (or otherwise hard-blocking)
  when a qualifying change is detected. Detection ships and is advisory; auto-revoke
  is revisited once the advisory flag has run against real drift events.
- **Aging / escalating the advisories** — `Severity::Evidence` findings and the
  re-attestation advisory are non-gating and do not accumulate pressure over time, so
  a chronically-ignored change-stale or version-mismatched TR carries no increasing
  signal. A first-seen date carried into the rolling issue (a dead-man's-switch) is
  deferred; the normalizer-bump blind window is bounded operationally by the runbook
  re-attestation window instead.
- **Per-class freshness tightening.** With six Recommended TRs (`token`, `t1101`,
  `t1102`, `t8412`, `S3_`, `CSPAQ12200`) spanning five classes (standalone,
  market_session, paginated, realtime, account), the 90-day default applies uniformly;
  per-class tightening (e.g. a shorter window for `orders`) stays deferred until those
  classes have recommended TRs.
