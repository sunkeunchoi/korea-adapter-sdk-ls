# TR → Paper Live Smoke target map

A TR is **recipe-promotable** only if it has a smoke target here. A TR absent from
this map has no harness that exercises it — promotion needs a new smoke first
(route to `ce-plan`), never fabricated evidence.

| TR | `make` target | Test fn | Gate / required input | Recommendation scope |
|----|---------------|---------|-----------------------|----------------------|
| `token` | `live-smoke` | `live_smoke_default` | open session | paper OAuth token issuance |
| `t1102` | `live-smoke` | `live_smoke_default` | open session | paper current-price quote |
| `t1101` | `live-smoke-book` | `live_smoke_book` | open session | paper price + 10-level order book |
| `t8412` | `live-smoke-chart` | `live_smoke_chart` | `LS_LIVE_SMOKE_T8412_DATE=<trading day YYYYMMDD>`; gateway `01715` = non-trading day | paper single chart page |
| `CSPAQ12200` | `live-smoke-account` | `live_smoke_account` | provisioned paper account (else SMOKE-FAIL) | paper read-only balance inquiry |
| `S3_` | `live-smoke-ws` | `live_smoke_ws` | paper WS port reachable | **websocket lifecycle reachability only** |
| `t8425` | `live-smoke-t8425` | `live_smoke_t8425` | open session | paper all-themes read (Implemented, not recommended) |
| `t8436` | `live-smoke-t8436` | `live_smoke_t8436` | open session | paper stock-master list, gubun=0 (Implemented, not recommended) |
| `t1531` | `live-smoke-t1531` | `live_smoke_t1531` | open session; self-sources a theme from `t8425` | paper theme-constituents read (Implemented, not recommended) |
| `t1537` | `live-smoke-t1537` | `live_smoke_t1537` | open session; self-sources a theme from `t8425` | paper theme per-stock quotes (Implemented, not recommended) |

Notes:
- `live-smoke` (default) issues the OAuth token *then* a `t1102` quote in one run,
  so it is the evidence source for **both** `token` and `t1102` — but each TR's
  evidence file records its *own* run date, so promoting one does not silently
  re-date the other.
- `revoke` is intentionally **not** here: it needs a new, destructive-ordering
  smoke (revoke invalidates the session token). Not a recipe-run.
- All targets call `paper_guard()` first and require `LS_TRADING_ENV=paper`
  explicitly. They hit the real paper gateway with real credentials from `.env`.

## Discovery query (for the orchestrator)

Recipe-promotable candidates = TRs where `support.implemented: true`,
`support.recommended: false`, **and** the TR code appears in the table above.
Everything else with `implemented && !recommended` is HELD as "needs ce-plan".
