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

## OPERATOR-OWNERSHIP DECISION — settle at hand-off (surfaced, not decided here)

The plan flags an open decision that is **not an agent task** (Risks: "the migration stalls
half-migrated"). Settle it before running the first live gate:

- **Who owns and schedules the four live Consumer Retirement Gates** (ingest — after #186 lands;
  catalog; budget-probe; Ladder), including the attended sessions they require.
- **A by-when / escalation trigger** for each deferred live gate.
- **The interim policy for a partially-enforced system**: how long a consumer may sit in Shadow,
  and who periodically reviews that the shared `Legacy`/`Shadow` scaffold is still carried (it is
  removed only in U10, after all four gates are recorded — leaving it indefinitely is the
  "permanent dual implementation" the plan lists as outside its identity).

Record the owner + by-when here (or in the team's ops tracker) before the first gate runs.
