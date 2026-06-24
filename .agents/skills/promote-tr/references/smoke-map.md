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
| `CSPAQ12300` | `live-smoke-cspaq12300` | `live_smoke_cspaq12300` | provisioned paper account (else SMOKE-FAIL) | implemented-only | paper read-only BEP/balance inquiry |
| `CSPAQ22200` | `live-smoke-cspaq22200` | `live_smoke_cspaq22200` | provisioned paper account (else SMOKE-FAIL) | implemented-only | paper read-only orderable-amount/valuation inquiry |
| `CFOBQ10500` | `live-smoke-cfobq10500` | `live_smoke_cfobq10500` | provisioned paper account (empty `00707` on a position-less account → PENDING) | implemented-only | paper read-only F/O deposit/margin inquiry |
| `CCENQ90200` | `live-smoke-ccenq90200` | `live_smoke_ccenq90200` | provisioned paper account; krx_extended night window (off-window empty → PENDING, regular clock N/A) | implemented-only | paper read-only KRX night-derivatives balance inquiry |
| `CFOAQ10100` | `live-smoke-cfoaq10100` | `live_smoke_cfoaq10100` | provisioned paper account + current `LS_LIVE_SMOKE_FNOISU` (empty `00707` → PENDING) | implemented-only | paper read-only F/O orderable-quantity inquiry (조회, not an order) |
| `CCENQ10100` | `live-smoke-ccenq10100` | `live_smoke_ccenq10100` | provisioned paper account + current `LS_LIVE_SMOKE_FNOISU`; krx_extended night window | implemented-only | paper read-only KRX night-derivatives orderable-quantity inquiry (조회, not an order) |
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
| `t1481` | `live-smoke-t1481` | `live_smoke_t1481` | single-page (idx=0); after-hours rank — may be empty (`00707`) outside an after-hours session | implemented-only | paper after-hours top change-rate, single page |
| `t1482` | `live-smoke-t1482` | `live_smoke_t1482` | single-page (sort_gbn=0, idx=0); after-hours rank — may be empty (`00707`) outside an after-hours session | implemented-only | paper after-hours top volume, single page |
| `t1866` | `live-smoke-t1866` | `live_smoke_t1866` | `LS_PAPER_USER_ID=<LS login id>` + ≥1 seeded server-saved condition (else SMOKE-FAIL: spine-input-unavailable) | implemented-only | paper server-saved condition list (spine producer), single page |
| `t1859` | `live-smoke-t1859` | `live_smoke_t1859` | `LS_PAPER_USER_ID=<LS login id>`; chained — self-sources a `query_index` from `t1866` (else SMOKE-FAIL: spine-input-unavailable) | implemented-only | paper server-saved condition search (spine consumer) |
| `t1826` | `live-smoke-t1826` | `live_smoke_t1826` | open session; `search_gb=0` (핵심검색) | implemented-only | paper ThinQ Q-click search-list (Wave 3 spine producer; yields `search_cd` keys) |
| `t1825` | `live-smoke-t1825` | `live_smoke_t1825` | open session; chained — self-sources a `search_cd` from `t1826` (else SMOKE-FAIL: spine-input-unavailable); `search_cd` not recorded | implemented-only | paper ThinQ Q-click search (Wave 3 spine consumer) |
| `t9905` | `live-smoke-t9905` | `live_smoke_t9905` | open session; no caller input | implemented-only | paper full underlying-asset list (Wave 1; `shcode` keys `t1964`) |
| `t9907` | `live-smoke-t9907` | `live_smoke_t9907` | open session; no caller input | implemented-only | paper ELW expiry-month list (Wave 1) |
| `t8431` | `live-smoke-t8431` | `live_smoke_t8431` | open session; no caller input | implemented-only | paper ELW symbol list (Wave 1 spine producer; `shcode` keys `t1958`) |
| `t9942` | `live-smoke-t9942` | `live_smoke_t9942` | open session; no caller input | implemented-only | paper ELW master list (Wave 1) |
| `t1958` | `live-smoke-t1958` | `live_smoke_t1958` | open session; chained — self-sources two `shcode`s from `t8431` (else SMOKE-FAIL) | implemented-only | paper ELW symbol comparison (Wave 1 comparison member) |
| `t1964` | `live-smoke-t1964` | `live_smoke_t1964` | open session; chained — self-sources an `item` underlying from `t9905`, walks first 10 (else SMOKE-FAIL) | implemented-only | paper ELW board (Wave 1 board member) |
| `t1601` | `live-smoke-t1601` | `live_smoke_t1601` | open session; documented gubun defaults (amount, KRX) | implemented-only | paper investor-by-type aggregate (Wave 2) |
| `t1615` | `live-smoke-t1615` | `live_smoke_t1615` | open session; documented gubun defaults (amount, KRX) | implemented-only | paper investor trading aggregate (Wave 2) |
| `t1640` | `live-smoke-t1640` | `live_smoke_t1640` | open session; documented gubun defaults (exchange-all, KRX) | implemented-only | paper program-trading aggregate (Wave 2) |
| `t1662` | `live-smoke-t1662` | `live_smoke_t1662` | open session; documented gubun defaults (KOSPI, amount, today, KRX) | implemented-only | paper by-time program-trading chart (Wave 2) |
| `t1664` | `live-smoke-t1664` | `live_smoke_t1664` | open session; documented gubun defaults (KOSPI, amount, by-time, cnt=20, KRX) | implemented-only | paper investor trading chart (Wave 2) |
| `t3341` | `live-smoke-t3341` | `live_smoke_t3341` | open session; single-page (`idx`=0 as a JSON number) | implemented-only | paper financial-ranking aggregate, single page (Wave 2) |
| `t8424` | `live-smoke-t8424` | `live_smoke_t8424` | open session; no caller input (sector cluster anchor + `upcode` source) | implemented-only | paper all-sectors list (Wave A) |
| `t1511` | `live-smoke-t1511` | `live_smoke_t1511` | open session; `upcode="001"` (코스피종합) | implemented-only | paper sector index snapshot (Wave A) |
| `t1485` | `live-smoke-t1485` | `live_smoke_t1485` | open session; `upcode="001"`, `gubun="1"` (expected/auction — may be empty off-session) | implemented-only | paper sector expected-index (Wave A) |
| `t1516` | `live-smoke-t1516` | `live_smoke_t1516` | open session; `upcode="001"` + representative `shcode="005930"` | implemented-only | paper per-sector stock board (Wave A) |
| `t1514` | `live-smoke-t1514` | `live_smoke_t1514` | open session; single-page (`cts_date` first page; `cnt` as a JSON number) | implemented-only | paper sector period-trend, single page (Wave A) |
| `t2301` | `live-smoke-t2301` | `live_smoke_t2301` | any session; `yyyymm="202609"` (near-quarterly), `gubun="G"` (정규) — F/O master/board read, non-empty off-session | implemented-only | paper F/O option board (PR-B U4) |
| `t2522` | `live-smoke-t2522` | `live_smoke_t2522` | any session; no caller input (`dummy`) — F/O underlying-asset master read, non-empty off-session | implemented-only | paper F/O stock-futures underlying master (PR-B U5) |
| `t8401` | `live-smoke-t8401` | `live_smoke_t8401` | any session; no caller input (`dummy`) — F/O stock-futures master read, non-empty off-session | implemented-only | paper F/O stock-futures master (PR-B U6) |
| `t8426` | `live-smoke-t8426` | `live_smoke_t8426` | any session; no caller input (`dummy`) — F/O commodity-futures master read, non-empty off-session | implemented-only | paper F/O commodity-futures master (PR-B U7) |
| `t8433` | `live-smoke-t8433` | `live_smoke_t8433` | any session; no caller input (`dummy`) — F/O index-option master read, non-empty off-session | implemented-only | paper F/O index-option master (PR-B U8) |
| `t8435` | `live-smoke-t8435` | `live_smoke_t8435` | any session; `gubun="MF"` (미니선물/mini futures; spec MINI/weekly segments MF/MO/WK/SF/QW) — F/O derivatives master read, non-empty off-session | implemented-only | paper F/O derivatives master (PR-B U9) |
| `t8467` | `live-smoke-t8467` | `live_smoke_t8467` | any session; `gubun="Q"` (KOSDAQ150 index-futures) — F/O index-futures master read, non-empty off-session | implemented-only | paper F/O index-futures master (PR-B U10) |
| `t9943` | `live-smoke-t9943` | `live_smoke_t9943` | any session; `gubun="V"` (volatility index-futures) — F/O index-futures master read, non-empty off-session | implemented-only | paper F/O index-futures master (PR-B U11) |
| `t9944` | `live-smoke-t9944` | `live_smoke_t9944` | any session; no caller input (`dummy`) — F/O index-option master read, non-empty off-session | implemented-only | paper F/O index-option master (PR-B U12) |
| `t2111` | `live-smoke-t2111` | `live_smoke_t2111` | anytime F/O; self-sources a contract `focode` from `t8467` — F/O current-price read (empty out-block off-session → PENDING) | implemented-only | paper F/O current-price quote (U5) |
| `t2112` | `live-smoke-t2112` | `live_smoke_t2112` | anytime F/O; self-sources a contract `shcode` from `t8467` — F/O order-book read (empty out-block off-session → PENDING) | implemented-only | paper F/O current-price order book (U5) |
| `t2106` | `live-smoke-t2106` | `live_smoke_t2106` | anytime F/O; self-sources a contract `code` from `t8467` — F/O price-memo read (empty memo array off-session → PENDING) | implemented-only | paper F/O price memo (U5) |
| `t8402` | `live-smoke-t8402` | `live_smoke_t8402` | anytime F/O; self-sources a contract `focode` from `t8401` — stock-futures current-price read (empty out-block off-session → PENDING) | implemented-only | paper stock-futures current price (U5) |
| `t8403` | `live-smoke-t8403` | `live_smoke_t8403` | anytime F/O; self-sources a contract `shcode` from `t8401` — stock-futures order-book read (empty out-block off-session → PENDING) | implemented-only | paper stock-futures order book (U5) |
| `t8434` | `live-smoke-t8434` | `live_smoke_t8434` | anytime F/O; self-sources a contract `focode` from `t8467`, `qrycnt=1` (JSON number) — F/O multi current-price read (empty array off-session → PENDING) | implemented-only | paper F/O multi current-price (U5) |
| `t1988` | `live-smoke-t1988` | `live_smoke_t1988` | standalone→`market_session` (KTD3); no caller input (all markets, filters off); `from_rate`/`to_rate` JSON numbers (KTD4) — ELW underlying-asset list (empty out-block → PENDING; IGW40011 → PENDING) | implemented-only | paper ELW underlying-asset list (U3) |
| `t3320` | `live-smoke-t3320` | `live_smoke_t3320` | standalone→`market_session` (KTD3); public ticker `gicode=005930` (삼성전자, bare 6-digit) — FnGuide company summary (empty out-block → PENDING) | implemented-only | paper FnGuide company summary (U3) |
| `t8455` | `live-smoke-t8455` | `live_smoke_t8455` | `venue_session: krx_extended` night window ~18:00–05:00 KST (NOT the regular clock, KTD7); `gubun=NF` — night-derivatives master (off-window empty → PENDING re-run, not a flip/DROP; `01900` → paper-incompatible) | implemented-only | paper KRX night-derivatives master (U6) |
| `t8460` | `live-smoke-t8460` | `live_smoke_t8460` | `venue_session: krx_extended` night window ~18:00–05:00 KST (KTD7); `yyyymm`=near month, `gubun=G` — night-option board (off-window empty → PENDING re-run) | implemented-only | paper KRX night-derivatives option board (U6) |
| `t8463` | `live-smoke-t8463` | `live_smoke_t8463` | `venue_session: krx_extended` night window ~18:00–05:00 KST (KTD7); `tm_rng=N`/`fot_clsf_cd=F`/`bsc_asts_id=101`, `cnt` JSON number (KTD4) — night investor-by-timeslot (off-window empty → PENDING re-run) | implemented-only | paper KRX night-derivatives investor-by-timeslot (U6) |
| `g3101` | `live-smoke-g3101` | `live_smoke_g3101` | `instrument_domain: overseas_stock`→`market_session` (KTD3); public ticker `82`/`TSLA` (TSLA on NASDAQ); canonical 현재가 `price` (KTD6) — overseas current-price (empty out-block → PENDING; `01900` → paper-incompatible) | implemented-only | paper overseas current-price (U7) |
| `g3104` | `live-smoke-g3104` | `live_smoke_g3104` | overseas_stock→`market_session`; `82`/`TSLA`; canonical 한글종목명 `korname` (KTD6) — overseas stock-info master (empty out-block → PENDING) | implemented-only | paper overseas stock-info master (U7) |
| `g3106` | `live-smoke-g3106` | `live_smoke_g3106` | overseas_stock→`market_session`; `82`/`TSLA`; canonical 현재가 `price` (KTD6) — overseas current-price+order-book (empty out-block → PENDING) | implemented-only | paper overseas order book (U7) |
| `g3102` | `live-smoke-g3102` | `live_smoke_g3102` | overseas_stock→`market_session`; `82`/`TSLA`, `readcnt`/`cts_seq` JSON numbers (KTD4); Object-Array detail (KTD5) — overseas time-series (empty array → PENDING) | implemented-only | paper overseas time-series (U7) |
| `g3103` | `live-smoke-g3103` | `live_smoke_g3103` | overseas_stock→`market_session`; `82`/`TSLA`, monthly `gubun=4`; Object-Array bars (KTD5) — overseas period chart (empty array → PENDING) | implemented-only | paper overseas period chart (U7) |
| `g3190` | `live-smoke-g3190` | `live_smoke_g3190` | overseas_stock→`market_session`; US/exchange `2`, `readcnt` JSON number (KTD4); canonical 한글종목명 `korname` (KTD6); Object-Array master rows (KTD5) — overseas master list (empty array → PENDING) | implemented-only | paper overseas master list (U7) |
| `o3101` | `live-smoke-o3101` | `live_smoke_o3101` | `instrument_domain: overseas_futures`→`market_session` (KTD3); `gubun=all`; canonical 종목명 `symbol_nm` (KTD6); ARRAY out-block (KTD5) — overseas-futures master (non-empty on paper → IMPLEMENTED; empty array → PENDING) | implemented-only | paper overseas-futures master (U8) |
| `o3121` | `live-smoke-o3121` | `live_smoke_o3121` | overseas_futures→`market_session`; `MktGb=O`; canonical 기초상품명 `bsc_gds_nm` (KTD6); ARRAY out-block (KTD5) — overseas-future-option master (non-empty on paper → IMPLEMENTED; empty array → PENDING) | implemented-only | paper overseas-future-option master (U8) |
| `o3105` | `live-smoke-o3105` | `live_smoke_o3105` | overseas_futures→`market_session`; symbol `CUSN23`; canonical 체결가격 `trd_p` (KTD6) — overseas-futures current-price (empty out-block → PENDING, paper feed not provisioned) | implemented-only | paper overseas-futures quote (U8) |
| `o3106` | `live-smoke-o3106` | `live_smoke_o3106` | overseas_futures→`market_session`; symbol `ADM23`; canonical 현재가 `price` (KTD6) — overseas-futures current-price+order-book (empty out-block → PENDING) | implemented-only | paper overseas-futures order book (U8) |
| `o3125` | `live-smoke-o3125` | `live_smoke_o3125` | overseas_futures→`market_session`; `mktgb=F`/`HSIM23`; canonical 체결가격 `trd_p` (KTD6) — overseas-future-option current-price (empty out-block → PENDING) | implemented-only | paper overseas-future-option quote (U8) |
| `o3126` | `live-smoke-o3126` | `live_smoke_o3126` | overseas_futures→`market_session`; `mktgb=F`/`ADM23`; canonical 현재가 `price` (KTD6) — overseas-future-option current-price+order-book (empty out-block → PENDING) | implemented-only | paper overseas-future-option order book (U8) |

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
