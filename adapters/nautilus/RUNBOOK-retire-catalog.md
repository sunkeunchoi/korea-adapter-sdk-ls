# Runbook — Consumer Retirement Gate: Catalog readiness (U7, AC10)

Specializes [RUNBOOK-consumer-retirement-gate.md](RUNBOOK-consumer-retirement-gate.md) for the
`lab-research catalog status` consumer. Recording `PASS` in
[`gate-verdicts/catalog.json`](gate-verdicts/README.md) is the merge trigger for the staged
catalog retirement diff (delete `last_weekday_on_or_before` + the non-Enforced branches; finalize
PAPER-CUTS #13 + the "undetectable holidays" comment; README catalog section → enforced posture).

> Live + operator-attended. `lab-research` is its own process, so a per-process
> `LS_CALENDAR_ADOPTION=enforced` for the canary affects only catalog.

## Gate steps

1. **Foundation Gate** — `make foundation-gate` green (record PASS).
2. **Snapshot validation** — [RUNBOOK-calendar-snapshot.md](RUNBOOK-calendar-snapshot.md) → PASS
   (authorized; coverage spans the operating horizon; no horizon endpoint `out_of_range`).
3. **AC10 local status canary** — run `lab-research catalog status` against the owner-local
   snapshot with `LS_CALENDAR_ADOPTION=enforced` (catalog process only) over a catalog whose
   boundaries exercise:
   - [ ] A **closed** watermark boundary (e.g. after a holiday cluster) → **does not false-flag**
     (GO, no spurious tail-undershoot).
   - [ ] A boundary-relevant **Unknown** → `NO-GO — calendar indeterminate`.
   - [ ] An **out-of-coverage** boundary → `NO-GO — calendar unavailable`.
   - [ ] A **stale-but-established** boundary → **GO** with a prominent warning naming the
     freshness dimension.
   Facts stay in the owner-local gate log.
4. **Rollback rehearsal** — [RUNBOOK-calendar-rollback.md](RUNBOOK-calendar-rollback.md) → PASS.
5. **Divergence review** — review the owner-local `divergence-catalog.log` corpus (captured over
   the Shadow window; `calendar-divergence consumer=catalog-*`). No unreviewed
   `calendar-closed-weekday-open` / `calendar-open-weekday-closed` disagreement remains.
6. **Record verdict** — owner-local gate log gets the facts; `gate-verdicts/catalog.json` gets
   `PASS`/`HOLD` only (verdict-only, R17). On `PASS`, `make merge-block-check` passes for catalog.
7. **Merge the staged catalog diff.**

## Hold conditions → stay Shadow, Legacy authoritative (R16)

- Any of the generic template's hold conditions.
- The canary false-flagged a closed boundary, or an Unknown/out-of-coverage boundary did not
  produce the expected `NO-GO`, or the stale boundary did not GO-with-warning.
