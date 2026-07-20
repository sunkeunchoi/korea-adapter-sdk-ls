# Runbook — Consumer Retirement Gate: Production Ladder (U9, AC12/AC13)

Specializes [RUNBOOK-consumer-retirement-gate.md](RUNBOOK-consumer-retirement-gate.md) for the
Production Ladder date-fact gate (`lab-live`). Recording `PASS` in
[`gate-verdicts/ladder.json`](gate-verdicts/README.md) is the merge trigger for the staged Ladder
retirement diff (delete `WeekdayKrxCalendar::date_fact` + the `Legacy | Shadow` arm in
`resolve_date_fact_and_record`; strike ONLY the holiday-confirmation clause of `RUNBOOK-rung1.md`
line 13). This gate additionally requires an **attended paper-session preflight** and a rollback
proof (AC12), and confirms every non-date Ladder check remains intact (AC13).

> Live + attended. KTD7: the time-of-day window half (`in_time_window` /
> `check_session_window`, 09:00–15:30 KST) is **preserved** — this gate retires only the
> Sat/Sun→Closed **date** decision.

## Gate steps

1. **Foundation Gate** — `make foundation-gate` green (record PASS).
2. **Snapshot validation** — [RUNBOOK-calendar-snapshot.md](RUNBOOK-calendar-snapshot.md) → PASS.
3. **AC12 attended paper-session preflight** — during a live paper session, run the Ladder
   dispatch gate with `LS_CALENDAR_ADOPTION=enforced` (lab-live process only) and confirm:
   - [ ] On a real Trading Session in the time window, the **calendar date fact and the manual
     confirmation agree** and the gate greens **with no override**.
   - [ ] On a proven Closed / Unknown date, the date gate refuses (Closed/unavailable
     non-deferrable; Unknown Red-by-default unless a bound, audited attended override).
   - [ ] **Rollback proof** — [RUNBOOK-calendar-rollback.md](RUNBOOK-calendar-rollback.md) → PASS.
4. **AC13 "everything else intact"** — confirm the following remain and behave unchanged:
   - [ ] Time-of-day window (`in_time_window` / `check_session_window`, 09:00–15:30 KST).
   - [ ] Attended-only operation, watchdog envelope, paper-lane, account flat-check, catalog
     freshness check, and the bound Unknown-override path.
5. **Divergence review** — review the owner-local `divergence-ladder.log` corpus
   (`calendar-divergence consumer=lab-live-dispatch`). No unreviewed disagreement remains.
6. **Record verdict** — verdict-only in `gate-verdicts/ladder.json`; facts owner-local.
7. **Merge the staged Ladder diff.** Confirm `RUNBOOK-rung1.md` line 13 keeps the "KRX regular
   session open (09:00–15:30 KST)" precondition with only the holiday-confirmation clause struck.

## Hold conditions → stay Shadow, Legacy authoritative (R16)

- Any of the generic template's hold conditions.
- The attended preflight showed calendar and manual confirmation disagreeing, or required an
  override to green a real session.
- Any non-date Ladder check (time window, watchdog, paper-lane, account, catalog,
  Unknown-override) is not intact.
