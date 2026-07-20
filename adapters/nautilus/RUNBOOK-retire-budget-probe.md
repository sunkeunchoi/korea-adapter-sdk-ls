# Runbook — Consumer Retirement Gate: Budget-probe (U8, AC11)

Specializes [RUNBOOK-consumer-retirement-gate.md](RUNBOOK-consumer-retirement-gate.md) for the
`budget-probe` automatic-selection consumer. Recording `PASS` in
[`gate-verdicts/budget-probe.json`](gate-verdicts/README.md) is the merge trigger for the staged
budget-probe retirement diff (delete `recent_trading_day` + the `Legacy | Shadow` arm in
`plan_probe_dates`; retire only the "holidays need an override" framing; **keep**
`LS_PROBE_SDATE`/`LS_PROBE_EDATE` bypass + `BypassAudit`).

> Live + operator-attended. `budget-probe` is its own process.

## Gate steps

1. **Foundation Gate** — `make foundation-gate` green (record PASS).
2. **Snapshot validation** — [RUNBOOK-calendar-snapshot.md](RUNBOOK-calendar-snapshot.md) → PASS.
3. **AC11 local canary** — run `budget-probe` against the owner-local snapshot with
   `LS_CALENDAR_ADOPTION=enforced` (budget-probe process only) and observe:
   - [ ] Automatic selection picks the **most-recent proven Trading Session**, skipping trailing
     Closed **and** Unknown days.
   - [ ] With **no proven session** (or an unavailable calendar), **NO gateway call** is made
     until an explicit range is supplied.
   - [ ] An explicit `LS_PROBE_SDATE`/`LS_PROBE_EDATE` range still works as an **audited bypass**
     (records operator + run context + the skipped calendar condition; a live call is attempted).
   Facts stay in the owner-local gate log.
4. **Rollback rehearsal** — [RUNBOOK-calendar-rollback.md](RUNBOOK-calendar-rollback.md) → PASS.
5. **Divergence review** — review the owner-local `divergence-budget-probe.log` corpus
   (`calendar-divergence consumer=budget-probe`). No unreviewed disagreement remains.
6. **Record verdict** — verdict-only in `gate-verdicts/budget-probe.json`; facts owner-local.
7. **Merge the staged budget-probe diff.**

## Recovery lever preserved (KTD8)

The explicit-range bypass (`LS_PROBE_SDATE`/`LS_PROBE_EDATE`) and its audit record are **kept** —
only the "holidays need an override" narrative retires. The bypass remains the documented
reproducibility/recovery path when the calendar can prove no session.

## Hold conditions → stay Shadow, Legacy authoritative (R16)

- Any of the generic template's hold conditions.
- Automatic selection did not skip Closed/Unknown, or made a gateway call with no proven session,
  or the explicit-range bypass no longer works.
