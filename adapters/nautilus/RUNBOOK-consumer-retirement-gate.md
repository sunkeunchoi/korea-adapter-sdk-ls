# Runbook — Consumer Retirement Gate (generic template)

The reusable spine each Phase C consumer specializes (issue #189, R16/KTD1/KTD9; F2). A
Consumer Retirement Gate authorizes cutting **one** consumer boundary from Shadow to **Enforced**
and merging its staged retirement diff (which deletes that consumer's weekday primitive + its
`Legacy | Shadow` arm + stale guidance). It is **operator-attended and live** — outside the
offline slice — so the agent authored the staged diff and this runbook; a human runs the ceremony
and records the verdict.

> Per-consumer specializations: `RUNBOOK-retire-ingest.md` (AC8/AC9), `RUNBOOK-retire-catalog.md`
> (AC10), `RUNBOOK-retire-budget-probe.md` (AC11), `RUNBOOK-retire-ladder.md` (AC12/AC13). Each
> fills in the consumer-specific canary in step 3.

## Independence (KTD3)

There are **four** Consumer Retirement Gates covering the six consumer boundaries: the ingest
gate retires the three ingest boundaries together (they share the `ls-ingest` process and one
`LS_CALENDAR_ADOPTION` value); catalog (`lab-research`), budget-probe, and Ladder (`lab-live`)
each have their own. Advancing one consumer must leave the other five on their prior adoption
state — the staged diff proves this offline (`AE5`), and this gate never uses one consumer's
retirement as evidence for another.

## The `LS_CALENDAR_ADOPTION` global-env footgun

`LS_CALENDAR_ADOPTION` is **process-wide**. Setting it to `enforced` during a canary flips
**every** not-yet-retired consumer in that process to Enforced at once. Run canaries with
per-consumer scoping (a dedicated process/invocation), **never** a global `enforced` flip across
a multi-consumer process. Independence is achieved by per-consumer arm removal in the staged
diff, not by the env var.

## Steps

1. **Foundation Gate recorded.** `make foundation-gate` is green (offline: core, refresh,
   activation, diagnostics, fixtures, six consumers, composition-root, failure-inversion,
   traceability, rollback rehearsal, divergence classification) — record its PASS. This proves
   the machinery is trustworthy; it does NOT by itself authorize this consumer (CONCEPTS.md).
2. **Production snapshot validated.** Run **RUNBOOK-calendar-snapshot.md** → PASS (authorized for
   the current agreement; coverage spans the operating horizon; no horizon endpoint
   `out_of_range`). A HOLD here stops the gate.
3. **Owner-local canary.** Run the consumer-specific live canary (see the per-consumer runbook)
   and observe the required safety invariants first-hand (e.g. Trading / Closed / Unknown / stale
   / unavailable outcomes). Facts stay in your owner-local log.
4. **Restart-after-activation + rehearsed rollback.** Run **RUNBOOK-calendar-rollback.md** → PASS
   (restart loads the expected new `artifact_id`; rollback restores the prior artifact + adoption
   state; a lapsed/unusable prior is refused, not installed).
5. **Classified-divergence review.** Review the **owner-local divergence-log corpus** captured
   over the Shadow observation window (see "Divergence-log capture" below). Every classified
   divergence (`calendar-divergence … class=…`) is accounted for; no unreviewed or unsafe
   `calendar-closed-weekday-open` / `calendar-open-weekday-closed` disagreement remains. An
   unreviewed divergence is a HOLD.
6. **Record the verdict.** Split the record:
   - **Owner-local gate log** (never committed): the canary facts, snapshot identities, affected
     dates, and divergence-corpus review.
   - **Committed, non-sensitive** `gate-verdicts/<consumer>.json`: flip `"verdict"` to `"PASS"`
     (or leave `HOLD`) plus software/schema versions. **No** KRX dates or identities (R17/KTD9).
7. **Merge the staged diff.** With the committed verdict at `PASS`, `make merge-block-check`
   passes for this consumer, so its staged retirement diff (delete weekday primitive + arm +
   stale guidance, flip default to Enforced) may merge. Until `PASS`, the merge-block fails the
   PR mechanically (KTD1/R7).

## Divergence-log capture (U3 → gate review, R5/AC4)

The classified Shadow-divergence observations are emitted to the **non-persisted** redacted
diagnostic channel (stderr, `calendar-divergence …`) so Shadow stays byte-identical to Legacy.
For the gate to sign off against a **durable** corpus (not a transient stream), capture that
channel over the Shadow observation window into an **owner-local, gitignored** divergence log:

```sh
# Owner-local capture — the target MUST be under a gitignored path (e.g. /state), never committed.
<run the consumer under LS_CALENDAR_ADOPTION=shadow> 2>> /state/divergence-<consumer>.log
grep 'calendar-divergence' /state/divergence-<consumer>.log   # the corpus step 5 reviews
```

This capture is compatible with KTD6 (neither persisted runtime state nor a stdout data product)
and KTD9 (owner-local, redacted). Reviewing the collected corpus — not a live tail — prevents a
vacuous sign-off and catches a calendar-vs-weekday split before the weekday fallback is removed.

## Hold conditions → stay Shadow, Legacy authoritative (R16)

Any of the following forces **HOLD** (leave `gate-verdicts/<consumer>.json` at `HOLD`; the
consumer stays Shadow and the Legacy weekday path stays authoritative until corrected):

- Foundation Gate not green.
- Snapshot validation HOLD (unauthorized / expired / coverage does not reach the horizon /
  load failure).
- Canary did not show the required safety invariants.
- Rollback rehearsal refused, or a restart did not load the expected identity.
- An unreviewed or unresolved Shadow divergence in the corpus.
- (Ladder only) the attended paper-session preflight showed calendar and manual confirmation
  disagreeing, or required an override.

## Legacy-restore fallback

Before this gate's `PASS` and merge, rollback = **restore Legacy behavior** (the weekday path is
still in the tree; set `LS_CALENDAR_ADOPTION=shadow`/`legacy` and restart). After merge, the
weekday primitive is gone, so rollback = **deploy the prior software release** (R8). The final
shared-scaffold removal (U10) is where dual-implementation rollback ends.

---

## OPERATOR-OWNERSHIP DECISION — SETTLED (recorded 2026-07-20)

The plan flagged an open decision that is **not an agent task** (Risks: "the migration stalls
half-migrated"). It is settled as follows; this is the governing record until #189 closes at U10.

### Owner (all four gates)

The **operator-maintainer** (the adapter operator/maintainer, `sunkeunchoi`) owns and schedules
**all four** live Consumer Retirement Gates — ingest, catalog, budget-probe, Ladder — and runs
their attended sessions (Ladder's AC12 attended paper-session preflight included). There is no
split owner: the ingest gate has the same owner, it is only *sequenced* after #186 lands (its PR
#191 must merge first). One person holds the verdict pen for every `gate-verdicts/<consumer>.json`
flip (R17/KTD9 verdict-only).

### By-when + escalation (per gate)

Each gate's target date is set by the operator **when its preconditions are met**, not fixed in
advance. The escalation trigger prevents a silent stall:

| Gate | Precondition (must hold before scheduling) | By-when after precondition met | Escalation if not scheduled |
|---|---|---|---|
| catalog | Foundation Gate green; snapshot PASS | schedule the live session within **2 weeks** | operator logs the reason it slipped in the owner-local gate log and re-targets |
| budget-probe | Foundation Gate green; snapshot PASS | within **2 weeks** | same |
| Ladder | Foundation Gate green; snapshot PASS; catalog + budget-probe recorded (run Ladder last) | within **2 weeks** | same |
| ingest | **#186 merged** (PR #191) + its proof suite on main; Foundation Gate green; snapshot PASS | within **2 weeks** of #186 landing | same |

"Within 2 weeks" is the target, not a hard deadline; the escalation trigger is *the absence of a
scheduled date* past that window, which forces an explicit, logged decision rather than drift.

### Interim policy for the partially-enforced system

- A consumer may sit in **Shadow** (Legacy authoritative, calendar counterfactual recorded)
  indefinitely **only while its gate's precondition is unmet or a HOLD condition stands**. Once
  the precondition holds, the by-when + escalation above applies — Shadow is not a resting state.
- **Scaffold-retention review:** the operator reviews, on a **monthly** cadence, that the shared
  `Legacy`/`Shadow` scaffold (the `CalendarAdoption::Legacy|Shadow` enum, the divergence types,
  the un-gated weekday primitives on not-yet-retired consumers) is still justified — i.e. at least
  one consumer is still legitimately pre-gate. The scaffold is removed only in **U10**, after all
  four `gate-verdicts/*.json` are `PASS` and their PRs merged. Carrying it past that point is the
  "permanent dual implementation" the plan lists as outside its identity; the monthly review is
  the check against that outcome.
- Each review's outcome (which consumers remain pre-gate, whether the scaffold is still warranted,
  any re-targeted by-when) is noted in the **owner-local** gate log, never committed here.
