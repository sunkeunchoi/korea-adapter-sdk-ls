# U1 — Closure Flip Wave (WS batch + reads sweep): worklists

Plan `docs/plans/2026-06-28-004-feat-closure-flip-ws-batch-reads-sweep-plan.md`, U1.
Sources: `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`,
`metadata/trs/*.yaml`, `docs/plans/notes/all-lane-flip-classification.md`.

Resolves OQ1 (WS batch composition) and OQ3 (no reads flip-floor — per-TR probe).

## Lane 1 — WebSocket batch selection (resolves OQ1)

The 84-channel deferred pool (from `all-lane-flip-classification.md` §WebSocket)
contains **10 channels with an EMPTY `res_example`** in the raw capture
(`DX0 DYC NPH NYS PM_ S4_ UPH UPM h2_ s4_`). Per `track-realtime-tr` §0 those are
**HELD — incomplete raw shape; needs live probe** and are NOT track-eligible
offline. That leaves **74 offline-trackable channels**.

**Selection rule (highest-value subset across all six instrument-domain groups).**
Take the four smaller domains wholesale (cross-domain coverage, all non-empty),
then fill to 31 with the canonical market-data primitives from stock +
futureoption (trade / orderbook / expected-trade / broker / index / VI / limit
bands / sensitivities).

### Selected batch — 31 channels

| TR | name | instrument_domain | venue_session | tr_key slot |
|---|---|---|---|---|
| NS3 | (NXT)체결 | stock | krx_regular | shcode |
| NH1 | (NXT)호가잔량 | stock | krx_regular | shcode |
| NS2 | (NXT)우선호가 | stock | krx_regular | shcode |
| NK1 | (NXT)거래원 | stock | krx_regular | shcode |
| NBT | (NXT)시간대별투자자매매추이 | stock | krx_regular | upcode |
| KS_ | KOSDAQ우선호가 | stock | krx_regular | shcode |
| OK_ | KOSDAQ거래원 | stock | krx_regular | shcode |
| KH_ | KOSDAQ프로그램매매종목별 | stock | krx_regular | shcode |
| KM_ | KOSDAQ프로그램매매전체집계 | stock | krx_regular | gubun |
| PH_ | KOSPI프로그램매매종목별 | stock | krx_regular | shcode |
| K1_ | KOSPI거래원 | stock | krx_regular | shcode |
| IJ_ | 지수 | stock | krx_regular | upcode |
| YS3 | KOSPI예상체결 | stock | krx_regular | shcode |
| YK3 | KOSDAQ예상체결 | stock | krx_regular | shcode |
| VI_ | VI발동해제 | stock | krx_regular | shcode |
| JC0 | 주식선물체결 | futures_options | krx_regular | futcode |
| JH0 | 주식선물호가 | futures_options | krx_regular | futcode |
| JD0 | 주식선물실시간상하한가 | futures_options | krx_regular | futcode |
| FD0 | KOSPI200선물실시간상하한가 | futures_options | krx_regular | futcode |
| OD0 | KOSPI200옵션실시간상하한가 | futures_options | krx_regular | optcode |
| OMG | KOSPI200옵션민감도 | futures_options | krx_regular | optcode |
| YF9 | 지수선물예상체결 | futures_options | krx_regular | futcode |
| YOC | 지수옵션예상체결 | futures_options | krx_regular | optcode |
| BM_ | 업종별투자자별매매현황 | sector_index | krx_regular | upcode |
| WOC | 해외옵션 체결 | overseas_futures | unspecified | symbol |
| WOH | 해외옵션 호가 | overseas_futures | unspecified | symbol |
| JIF | 장운영정보 | misc | unspecified | gubun |
| NWS | 실시간뉴스제목패킷 | misc | unspecified | code |
| BMT | 시간대별투자자매매추이 | realtime_invest | krx_regular | upcode |
| CUR | 현물정보USD실시간 | realtime_invest | unspecified | symbol |
| MK2 | US지수 | realtime_invest | unspecified | gubun |

Domain split: stock 15, futureoption 8, sector 1, overseas-futures 2, misc(etc) 2,
realtime_invest(investment-info) 3 = **31**.

### Deferred remainder — 53 channels (identical follow-up waves)

- **43 offline-trackable, not selected this batch** (stock 29 + futureoption 14):
  `B7_ DH1 DHA DK3 DS3 DVI H2_ HB_ I5_ ESN h3_ k1_ s2_ s3_ NH1?`* SHC SHD SHI SHO
  Ys3 UYS UK1 UBT UBM UVI NBM AFR YJ_` (stock) and
  `CD0 FX9 OX0 YC3 YJC C02 DH0 DD0 DC0 DBM DBT O02 H02 DX0?`* (futureoption).
  (\*Authoritative per-channel list is the 84-pool minus the 31 selected minus the
  10 EMPTY; reproduce from the raw capture at follow-up time.)
- **10 EMPTY-`res_example` (HELD until live-probed):**
  `DX0 DYC NPH NYS PM_ S4_ UPH UPM h2_ s4_`.

## Lane 2 — Tracked-only reads enumeration (resolves OQ3: no flip-floor)

> **Discrepancy vs plan (surfaced for the operator).** The plan's R6 states "exactly
> 56" tracked-only reads. The current metadata set
> (`tracked: true` + `implemented: false` + `owner_class ∈ {market_session,
> paginated, account}`) yields **69**. The extra 13 reflect tracking added by
> intervening waves after the plan was authored. Lane 2 is operator-gated (every
> read needs a live `make raw-probe` under closure), so the count does not block the
> autonomous WS lane — the operator probes the **current 69** below, not a frozen 56.

69 tracked-only reads to raw-probe (U5), grouped:

- **Already `paper_incompatible: true` — re-confirm against fresh probe (R8/AE3):**
  `CCENQ10100 CCENQ90200` (futureoption account, `01900`),
  `g3101 g3102 g3103 g3104 g3106 g3190` (overseas-stock, §14 no-paper-feed),
  `t8455 t8460 t8463` (night-derivatives, session-gated). = 11.
- **Account reads (non-`paper_incompatible`):** `CSPBQ00200 t0441 o3107 o3127`
  (o3107/o3127 are the §16 account-empty PENDINGs the plan defers; probe to
  re-confirm). = 4.
- **Stock reads (market_session / paginated):** `t1109 t1301 t1308 t1411 t1449
  t1471 t1475 t1486 t1488 t1602 t1603 t1617 t1621 t1631 t1632 t1633 t1637 t1638
  t1665 t1702 t1716 t1717 t1752 t1771 t1902 t1904 t1906 t1927 t1941 t1950 t1951
  t1954 t1956 t1959 t1969 t1971 t1972 t1973 t1974 t8407 t8428 t8450 t8454`. = 43.
- **Futureoption reads:** `t2106 t2210 t2212 t2214 t2407 t2424 t2541 t2545 t8404
  t8406 t8427`. = 11.

Per-TR disposition table (U5 output) is appended below once the operator runs the
probes. Disposition classes (R7/R8): data-carrying → flip (U6); `00707` empty &
plausibly session/funding-dependent → PENDING; `01900` or proven no-paper-data →
`paper_incompatible` (U7); `IGW40011` → re-audit numeric request types (KTD5)
before classifying environmental.

## Operator-gated legs (this autonomous run does NOT execute these)

The live paper-gateway smokes/probes need `.env` credentials + `LS_TRADING_ENV=paper`
under KRX closure:

- **U4 (WS flip):** run `make live-smoke-ws-p3` (the combined sweep added in this
  branch). A clean connect→subscribe→unsubscribe per channel flips it to
  Implemented (connection-reachable-only). See the handoff in the PR body.
- **U5/U6/U7 (reads sweep):** run `make raw-probe ...` per read above; flip
  data-carriers (U6), reclassify the rest (U7).
