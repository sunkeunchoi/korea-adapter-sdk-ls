# TR → Paper Live Smoke target map

A TR is **recipe-promotable** only if it has a smoke target here. A TR absent from
this map has no harness that exercises it — promotion needs a new smoke first
(route to `ce-plan`), never fabricated evidence.

The **Promotion** column is the explicit recommendation-readiness signal the
discovery query reads — presence in this table is NOT sufficient. `ready` means
the TR is cleared for `implemented → recommended` promotion; `implemented-only`
means it is callable and smoke-gated but has NOT been cleared for the Recommended
tier (a newly-implemented TR defaults here). Only an `implemented && !recommended`
TR marked `ready` is a promote-tr candidate.

| TR | `make` target | Test fn | Gate / required input | Promotion | Scope |
|----|---------------|---------|-----------------------|-----------|-------|
| `token` | `live-smoke` | `live_smoke_default` | open session | ready | paper OAuth token issuance |
| `t1102` | `live-smoke` | `live_smoke_default` | open session | ready | paper current-price quote |
| `t1101` | `live-smoke-book` | `live_smoke_book` | open session | ready | paper price + 10-level order book |
| `t8412` | `live-smoke-chart` | `live_smoke_chart` | `LS_LIVE_SMOKE_T8412_DATE=<trading day YYYYMMDD>`; gateway `01715` = non-trading day | ready | paper single chart page |
| `CSPAQ12200` | `live-smoke-account` | `live_smoke_account` | provisioned paper account (else SMOKE-FAIL) | ready | paper read-only balance inquiry |
| `S3_` | `live-smoke-ws` | `live_smoke_ws` | paper WS port reachable | ready | **websocket lifecycle reachability only** |
| `t8425` | `live-smoke-t8425` | `live_smoke_t8425` | open session | implemented-only | paper all-themes read |
| `t8436` | `live-smoke-t8436` | `live_smoke_t8436` | open session | implemented-only | paper stock-master list, gubun=0 |
| `t1531` | `live-smoke-t1531` | `live_smoke_t1531` | open session; self-sources a theme from `t8425` | implemented-only | paper theme-constituents read |
| `t1537` | `live-smoke-t1537` | `live_smoke_t1537` | open session; self-sources a theme from `t8425` | implemented-only | paper theme per-stock quotes |
| `t1452` | `live-smoke-t1452` | `live_smoke_t1452` | open session; single-page (idx=0) | implemented-only | paper top-volume rank, single page |
| `t1403` | `live-smoke-t1403` | `live_smoke_t1403` | single-page; listing-MONTH range; NOT trading-day-gated (no `01715`) | implemented-only | paper newly-listed stocks, single page |
| `t1441` | `live-smoke-t1441` | `live_smoke_t1441` | open session; single-page | implemented-only | paper top change-rate, single page |
| `t1463` | `live-smoke-t1463` | `live_smoke_t1463` | open session; single-page | implemented-only | paper top trading-value, single page |
| `t1466` | `live-smoke-t1466` | `live_smoke_t1466` | open session; single-page | implemented-only | paper volume-surge, single page |
| `t1489` | `live-smoke-t1489` | `live_smoke_t1489` | open session; single-page (auction/expected — may be empty off-session) | implemented-only | paper expected-execution-volume, single page |
| `t1492` | `live-smoke-t1492` | `live_smoke_t1492` | open session; single-page (single-price/expected — may be empty off-session) | implemented-only | paper single-price expected-change, single page |

Notes:
- `live-smoke` (default) issues the OAuth token *then* a `t1102` quote in one run,
  so it is the evidence source for **both** `token` and `t1102` — but each TR's
  evidence file records its *own* run date, so promoting one does not silently
  re-date the other.
- `revoke` is intentionally **not** here: it needs a new, destructive-ordering
  smoke (revoke invalidates the session token). Not a recipe-run.
- All targets call `paper_guard()` first and require `LS_TRADING_ENV=paper`
  explicitly. They hit the real paper gateway with real credentials from `.env`.
- `t1403` (신규상장종목조회) is **date-range-filtered, not trading-day-gated**.
  Its inputs are listing MONTHS (`styymm`/`enyymm`, `YYYYMM`), so the `01715`
  non-trading-day error cannot apply (no day field) — confirmed live across
  weekday/weekend/future ranges. Do NOT add `t8412`-style weekday-pin / `01715`
  retry handling for it. Use a wide month range (the smoke uses `202401-202612`)
  so past listings keep it non-empty. A transient `IGW00201` gateway error under
  rapid successive calls is environmental throttling (clears on retry), not a TR
  defect — classify via the R6 raw-HTTP probe, never flip on it.

## Discovery query (for the orchestrator)

Recipe-promotable candidates = TRs where `support.implemented: true`,
`support.recommended: false`, the TR code appears in the table above, **and** its
**Promotion** column is `ready`.

- An `implemented && !recommended` TR **not** in this table → HELD "needs ce-plan
  (no smoke harness)".
- An `implemented && !recommended` TR in the table but marked `implemented-only`
  → HELD "implemented-only; not cleared for recommendation". It has a harness and
  passes its smoke, but presence + a passing smoke is NOT a recommendation
  mandate. Clearing it is a deliberate act: flip its Promotion cell to `ready`
  when the Recommended tier is intentionally pursued for that TR (per the
  interim-consumer / positioning decision in the wave's Open Questions).
