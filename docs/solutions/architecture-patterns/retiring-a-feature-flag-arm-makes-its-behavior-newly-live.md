---
title: "Retiring a feature-flag arm makes that arm's behavior newly LIVE, not newly written — audit the newly-authoritative arm, not just the diff"
date: 2026-07-21
category: architecture-patterns
module: "nautilus adapter — ls-ingest / KRX calendar-adoption seam (adapters/nautilus/src/ingest/mod.rs, src/bin/ls-ingest.rs)"
problem_type: architecture_pattern
component: tooling
severity: high
applies_when:
  - "collapsing a multi-arm feature flag (e.g. a Legacy/Shadow/Enforced adoption enum) down to a single always-authoritative arm"
  - "production's DEFAULT arm before the collapse was a pass-through/no-op arm (e.g. Shadow byte-identical to Legacy), not the arm being made authoritative"
  - "the diff reads as pure un-nesting/simplification (removing match arms, deleting a flag check) with no logic rewritten inside the surviving branch"
  - "the surviving arm gates conditional behaviors (detection, admission, validation) that ran under narrow or never-before-exercised production conditions"
  - "review is diff-scoped (PR-diff or correctness-equivalence review) rather than re-auditing every behavior the newly-authoritative arm now gates"
related_components:
  - nautilus-ls-calendar
  - ingest-accumulate
  - calendar-gate
tags:
  - feature-flag-retirement
  - adoption-seam
  - enforced-cutover
  - code-review
  - production-default
  - diff-review-blindspot
  - newly-live-behavior
---

# Retiring a feature-flag arm makes that arm's behavior newly LIVE, not newly written — audit the newly-authoritative arm, not just the diff

## Context

Issue #189 retires the KRX weekday-era behavior across the six calendar consumers. The
shared seam is `nautilus_ls_calendar::CalendarAdoption`, a three-arm enum — `Legacy`,
`Shadow`, `Enforced`. Consumers shipped defaulting to **Shadow**, which was engineered to
be byte-identical to the old weekday path; the actual **Enforced** cutover (where the frozen
KRX calendar proof, not `weekday_strictly_between`, decides every fetch) was deferred to
#189. PR #199 (U6) performs that cutover for `ls-ingest`: it deletes the `Legacy`/`Shadow`
arms, deletes the weekday primitive `weekday_strictly_between`, and makes the ingest binary
Enforced-only. The gate methods on `CalendarGate` now act on the injected calendar
regardless of `adoption`, and a missing view stops before dispatch.

The friction: **the diff reads as pure un-nesting.** Collapsing
`match adoption { Legacy => .. Shadow => .. Enforced => X }` down to `X` is dominated by
deletions, and a diff-scoped reviewer (correctly) verifies that the surviving `Enforced`
arm is verbatim-equivalent to the prior nested `Enforced` arm. Eight in-process reviewers
plus a correctness reviewer did exactly that, and called the `detection_authorized`
simplification "equivalent (`A && (false || fetch) == A && fetch`)". That algebra is sound —
**for callers of the Enforced arm.** But production ran Shadow. So every behavior the
now-authoritative Enforced arm gates runs in production *for the first time* at the moment
of merge. Two of those newly-live behaviors were P1 defects, invisible to a review framed as
arm-to-arm equivalence.

## Guidance

When you collapse a multi-arm, default-carrying seam (feature flag, adoption enum, strategy
toggle) down to a single always-authoritative arm, the surviving arm's behaviors become
**newly LIVE in production, not newly written.** The audit unit is therefore *"every behavior
the surviving arm gates,"* not *"the diff."* Concretely:

- **List every behavior the surviving arm gates and review each one as if it were newly
  written**, against the *production* baseline. Enumerate the decision methods that now run
  unconditionally: in this case `calendar_decision`, `action`, `range_action`,
  `accumulate_plan`/`established_prefix`, `probe_anchor`, `widen_action`, and the basis-shift
  authorization. Each is a candidate for a behavior that never executed under the default arm.
- **Do not accept "equivalent for callers of arm X" when production defaulted to a different
  arm Y.** Arm-to-arm equivalence is the wrong equivalence class. The load-bearing question is
  "what changes for a run that previously took arm Y?" — and the answer is "everything arm X
  gates."
- **The tell:** a diff that is *mostly deletions of `match <flag> { .. }` arms* is a
  default-flip in disguise, not a no-op refactor. Whenever the removed arms include the one a
  runtime default selected, treat the PR as a behavioral change of the full surviving-arm
  surface, and size the review to that surface.
- **A review technique that worked here:** an independent **cross-model, different-frame
  adversarial pass** (a different model family, in a separate process, not sharing the panel's
  "compare Enforced-arm to Enforced-arm" mental frame) proposed both defect cascades; separate
  per-finding validators then confirmed each against the tree. A panel that all shares one
  frame will miss what that frame excludes by construction — so deliberately introduce a
  reviewer that does not share it.

## Why This Matters

Two P1 data-fidelity/integrity defects reached a staged, merge-blocked PR *precisely because
the review reasoning stopped at arm-equivalence.*

**Defect 1 — silent basis-shift heal skip (data fidelity).** The basis-shift heal is gated on
`detection_authorized`. As kept in the diff it required
`matches!(calendar.calendar_decision(wm), CalendarDecision::Fetch)` — i.e. the watermark date
*itself* must be a proven Trading Session. But the new daily Enforced cron drops the weekday
restriction (`0 17 * * *`), so after a weekend or holiday the all-Closed skip-advance path
advances the watermark onto a proven **Closed** date: `established_prefix` records
`advance_through` across the Closed rows (`adapters/nautilus/src/ingest/mod.rs:326`), and the
skip branch commits `set_watermark(.., advance)` (`mod.rs:1878-1883`). On the first trading
day back, `calendar_decision(wm) != Fetch` → detection skipped → a split/dividend bar appends
onto stale basis until a later run happens to heal it. Old Shadow never hit this: it bypassed
via the now-removed `calendar.adoption() != CalendarAdoption::Enforced ||` clause, *and* the
old weekday-only cron left `wm` on a Friday session. This behavior had never run in
production. Fixed in the review-followup commit on #199 by re-anchoring authorization on the
new `proven_session_at_or_before(wm)` — a strict superset of the retired `Fetch`-at-`wm`
check that still fails closed on Unknown/unavailable/out-of-coverage (it reuses
`select_recent_session`, `mod.rs:350-371`).

**Defect 2 — false metadata pin on zero bars (data integrity).** Startup admission validated
only the target date (`calendar_target_for_mode` in `adapters/nautilus/src/bin/ls-ingest.rs`),
never the `LS_INGEST_LOOKBACK` floor. Under always-Enforced, when the floor precedes the
calendar coverage `established_prefix` returns `stop_before` with no request/advance
(`adapters/nautilus/src/ingest/mod.rs:307-311`); the skip path drops `stop_before`; the
accumulate `CoverageReport` hardcodes `range_refusals: Vec::new()`; `exit_code_for` consults
only the refusal vectors → exit 0 (`adapters/nautilus/src/bin/ls-ingest.rs:68-77`); and the
metadata pin gates on exit 0. Net: a pin attesting
bars that never landed, written onto an empty catalog. Under Shadow the legacy plan hardcoded
`request_through = ceiling` and always fetched, so the floor was never out-of-coverage in a
way that mattered. Fixed by a `floor_admission` helper that fails closed *before gateway
construction* when the floor is outside coverage, mirroring the existing target admission.

Both defects were real, both were merge-blocked (the mechanical merge-block held), but the
merge-block only guards weekday-primitive deletion — **the live ingest gate would have
certified these defects into production.** That is the exact failure mode #189's staged
"certify before enforce" posture exists to prevent: the Shadow arm ships byte-identical so the
Enforced arm can be *certified* before it becomes authoritative. Certification only works if
the audit covers the whole surviving-arm surface; scoping it to the diff defeats the staging.

## When to Apply

Apply this whenever you are:

- Retiring a feature flag, adoption enum, or strategy toggle down to a single arm.
- Flipping a default from a pass-through / no-op / "shadow" arm to an active arm — even when
  the flip and the deletion land in the same PR.
- Reviewing any "un-nesting a `match` to a single branch" diff where a **runtime default
  selected a different branch than the one being kept.** The giveaway is that the branch you
  kept is not the branch production ran.

If the kept arm is the one production already defaulted to, the un-nesting really is a no-op
and diff-scoped review is fine. The danger is exclusively the case where kept-arm ≠
default-arm.

## Examples

**Defect 1 — the `detection_authorized` condition.**

Before (as kept in the un-nesting diff — authorized only when the watermark date is itself a
proven session):

```rust
let detection_authorized = candidate
    .destructive_request_through()
    .is_some_and(|heal_through| heal_through >= wm)
    && matches!(calendar.calendar_decision(wm), CalendarDecision::Fetch);
```

After (`adapters/nautilus/src/ingest/mod.rs` — authorized on a proven session at or before the
watermark, so detection still runs when the watermark legitimately sits on a weekend/holiday
Closed date):

```rust
let detection_authorized = candidate
    .destructive_request_through()
    .is_some_and(|heal_through| heal_through >= wm)
    && calendar.proven_session_at_or_before(wm);
```

The doc-comment on the new predicate states the equivalence class explicitly: it is "a strict
superset of the retired `Fetch`-at-`wm` check, adding only the proven-Closed watermark case."

**Defect 2 — the floor-admission gap.** The startup admission only ever validated the target:

```rust
let calendar_target = calendar_target_for_mode(&mode, calendar.as_of())?;   // target only
```

The fix adds a floor gate that fails closed before any gateway is built
(`adapters/nautilus/src/bin/ls-ingest.rs`):

```rust
fn floor_admission(floor: NaiveDate, coverage: Option<(NaiveDate, NaiveDate)>) -> Result<(), String> {
    if let Some((from, through)) = coverage {
        if floor < from || floor > through {
            return Err(format!(
                "backfill floor {} is outside the frozen calendar coverage [{}, {}] — a fresh \
                 instrument could not be attested from it (its prefix would skip with zero bars \
                 and the run would mis-pin). Widen the calendar snapshot or raise LS_INGEST_LOOKBACK.",
                floor.format("%Y%m%d"), from.format("%Y%m%d"), through.format("%Y%m%d"),
            ));
        }
    }
    Ok(())
}
```

The regression test `floor_admission_refuses_a_floor_below_coverage` pins the exact cascade
trigger: a floor below coverage is refused, boundaries are admitted, and a missing view passes
through (already fail-closed by the target admission).

## Related

- `docs/solutions/architecture-patterns/gate-over-diff-inherits-diff-scope-blind-spot.md` —
  the same meta-pattern family: *a check inherits the scope of the input it was built to
  inspect and silently misses what falls outside that scope.* There a change-tracker's diff
  scope; here a code review's newly-changed-lines scope vs. the newly-authoritative arm's full
  gated surface. Same blind-spot shape, different domain.
- `docs/solutions/logic-errors/per-date-gate-on-a-range-op-silently-advances-over-unchecked-dates.md`
  — the same ingest gate, a cousin of Defect 1's shape: a per-*date* check standing in for a
  *range* obligation silently advances over dates it never inspected. Also a concrete
  Enforced-arm defect this newly-live-arm review discipline would have caught.
- `docs/solutions/conventions/composition-root-always-emit-before-fallible-parse.md` — the
  composition-root ordering the ingest startup record follows (emit before the fallible
  SDK/runtime build); Defect 2's fix places `floor_admission` in that same pre-gateway
  admission window. Sibling class: a structural refactor changing which paths execute.
- `docs/solutions/logic-errors/safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md`
  — same #185/#189 KRX-calendar lineage and the broader "leaf proof ≠ caller-level
  correctness" family; loosely related precedent.
- Issue #189 (KRX weekday retirement) and PR #199 (U6 Enforced-only ingest cutover). Both
  defects were fixed in the review-followup commit on #199 (`ac6502d` on branch
  `feat/189-retire-ingest`, likely squashed on merge).
