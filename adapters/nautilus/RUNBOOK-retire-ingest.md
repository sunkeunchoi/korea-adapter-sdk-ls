# Runbook — Consumer Retirement Gate: Ingest (U6, AC8/AC9)

Specializes [RUNBOOK-consumer-retirement-gate.md](RUNBOOK-consumer-retirement-gate.md) for the
three ingest boundaries (accumulate/probe, checkpoint continuity, backward-widen), which share
the `ls-ingest` process and one `LS_CALENDAR_ADOPTION` value — so they retire together under one
gate (KTD3). Recording `PASS` in [`gate-verdicts/ingest.json`](gate-verdicts/README.md) is the
merge trigger for the staged ingest retirement diff (delete `weekday_strictly_between`,
`last_closed_session` weekday usage, the `Legacy | Shadow` arms + un-gated `legacy()` wrappers;
replace the README `0 17 * * 1-5` weekday cron with a daily KST post-close recipe).

> **BLOCKED on issue #186.** The ingest Shadow seams are landed, but #186 (the ingest
> proof/composition closeout) must land first — the staged ingest retirement diff and its
> failure-inversion assertions assume #186's proof suite exists (see the plan's Deferred /
> Dependencies). Do not author or merge the ingest retirement until #186 is on `main`.

> Live + operator-attended. `ls-ingest` is one process covering all three ingest boundaries.

## Gate steps

1. **Foundation Gate** — `make foundation-gate` green (record PASS).
2. **Snapshot validation** — [RUNBOOK-calendar-snapshot.md](RUNBOOK-calendar-snapshot.md) → PASS.
3. **AC8 accumulate post-close canary** — a real post-close local canary with
   `LS_CALENDAR_ADOPTION=enforced` (ingest process only) observing the safety invariants:
   - [ ] **Trading** session date → exactly one fetch request.
   - [ ] **Closed** date → advance without a request (no false coverage).
   - [ ] **Unknown** date → no request and no watermark advance (provenance guard).
   - [ ] **Stale** calendar → usable, with the staleness surfaced.
   - [ ] **Unavailable** calendar → stop and preserve state (fail-closed, no weekday fallback).
4. **AC9 checkpoint + backward-widen canaries** — observe:
   - [ ] Checkpoint continuity merges an all-Closed gap, keeps ranges separate across a proven
     session or an Unknown gap (no silent weekday hole test).
   - [ ] Backward-widen warns+persists on a proven pre-coverage session, is silent on all-Closed,
     and is uncertain (non-persisted) on Unknown.
5. **Rollback rehearsal** — [RUNBOOK-calendar-rollback.md](RUNBOOK-calendar-rollback.md) → PASS.
6. **Divergence review** — review the owner-local `divergence-ingest.log` corpus
   (`calendar-divergence consumer=ingest-*`). No unreviewed disagreement remains.
7. **Record verdict** — verdict-only in `gate-verdicts/ingest.json`; facts owner-local.
8. **Merge the staged ingest diff** (only after #186 has landed and this gate is recorded).

## Recovery lever preserved (KTD8)

The daily-KST post-close **semantics** are kept — only the `1-5` weekday cron restriction goes.
The accumulate recipe becomes a daily KST post-close invocation whose calendar policy decides
safety.

## Hold conditions → stay Shadow, Legacy authoritative (R16)

- Any of the generic template's hold conditions.
- #186 not yet landed.
- Any AC8/AC9 safety invariant not observed (a request on a Closed/Unknown date, a watermark
  advance on Unknown, a failure to stop+preserve on unavailable, a bad checkpoint/widen verdict).
