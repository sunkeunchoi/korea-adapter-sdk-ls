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

- **43 offline-trackable, not selected this batch:**
  `AFR B7_ C02 CD0 DBM DBT DC0 DD0 DH0 DH1 DHA DK3 DS3 DVI ESN FX9 H02 H2_ HB_ I5_
  JX0 NBM NPM NVI O02 OX0 SHC SHD SHI SHO UBM UBT UK1 UVI UYS YC3 YJC YJ_ Ys3 h3_
  k1_ s2_ s3_`.
- **10 EMPTY-`res_example` (HELD — `track-realtime-tr` §0, need a live probe to pin
  a deserializable baseline):** `DX0 DYC NPH NYS PM_ S4_ UPH UPM h2_ s4_`.

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

### U4 — WS flip (operator post-smoke checklist)

This branch already staged the entire offline half of U4 (mirroring the
`implement-realtime-tr` recipe), so the operator's job is just the smoke + flip:

1. Run `make live-smoke-ws-p3` under closure with paper `.env`. It sweeps all 31
   on a fresh `WsManager` each, emits a per-TR `LIVE-SMOKE` line, fails red iff any
   channel's connect→subscribe→unsubscribe (port `29443`) failed.
2. For each channel with a clean lifecycle, flip `metadata/trs/<tr>.yaml` +
   `tr-index.yaml`: `support.implemented: true` (keep `recommended: false`, **no**
   `recommendation` block), and note "connection-reachable-only". A channel whose
   lifecycle can't open → leave Tracked-only (PENDING), don't flip.
3. Bump the Implemented-rung count-sites **by the number actually flipped**:
   `reference.len()` (currently 182) and `banner_trs` in `crates/ls-docgen/src/lib.rs`.
   (`maintained_tr_count`/`TRACKED_TRS` already bumped at the Tracked rung — leave them.)
4. Flip the smoke-map Promotion column for each flipped TR from
   `tracked (pending …)` to `implemented-only`.
5. `make docs` then the full gate (`cargo test`, `cargo test -p ls-core`,
   `make docs-check`). Already-staged + gate-green: structs (`crates/ls-sdk/src/realtime/frame.rs`
   `<Code>Row` ×31 + offline decode tests), `{TR}_POLICY` ×31
   (`crates/ls-core/src/endpoint_policy.rs`) registered crosscheck-only, the sweep
   smoke `live_smoke_ws_p3` + `make live-smoke-ws-p3`, smoke-map rows.

### U5/U6/U7 — reads sweep (operator, fully probe-gated)

Run `make raw-probe LS_PROBE_TR_CD=<tr> LS_PROBE_PATH=<path> LS_PROBE_BODY=<json>`
per read above; flip data-carriers via `implement-tr` (U6 — these are already
Tracked, so no track step; assert a named non-sentinel, non-account-identifying
witness per KTD2; numeric request slots `string_as_number` per KTD5), reclassify
the rest (U7 — PENDING for `00707` session/funding-dependent, `paper_incompatible`
for `01900`/proven-dead; re-confirm prior flags). This is Lane 2 / PR B (KTD7).

## U5 probe disposition (executed 2026-06-29, market OPEN/in-session)

Ran `make raw-probe` (via direct cargo, credential-safe http/rsp_cd/body_len) for all 69.
Because KRX was **in-session** (Monday), far more reads carry data than the closure-era
plan anticipated: **44 returned 00000 with a non-trivial body**, 2 re-confirmed `01900`,
the rest empty/no-feed. Calibration: empty out-blocks ~26-98 bytes; populated >500.

| TR | owner_class | domain | http | rsp_cd | body_len | disposition |
|---|---|---|---|---|---|---|
| CCENQ10100 | account | futures_options | 200 | 01900 | 95 | paper_incompatible (01900) — re-confirmed (prev paper_incompat) |
| CCENQ90200 | account | futures_options | 200 | 01900 | 95 | paper_incompatible (01900) — re-confirmed (prev paper_incompat) |
| CSPBQ00200 | account | stock | 200 | 00136 | 1076 | 00136 success-no-data → PENDING |
| g3101 | market_session | overseas_stock | 200 | EMPTY | 26 | empty/no-feed → PENDING / paper_incompatible (re-confirm) (prev paper_incompat) |
| g3102 | market_session | overseas_stock | 200 | EMPTY | 26 | empty/no-feed → PENDING / paper_incompatible (re-confirm) (prev paper_incompat) |
| g3103 | market_session | overseas_stock | 200 | 00009 | 61 | 00009 → PENDING (param/session) (prev paper_incompat) |
| g3104 | market_session | overseas_stock | 200 | EMPTY | 26 | empty/no-feed → PENDING / paper_incompatible (re-confirm) (prev paper_incompat) |
| g3106 | market_session | overseas_stock | 200 | EMPTY | 26 | empty/no-feed → PENDING / paper_incompatible (re-confirm) (prev paper_incompat) |
| g3190 | market_session | overseas_stock | 200 | 00000 | 187 | small-body → verify witness before flip (prev paper_incompat) |
| o3107 | market_session | overseas_futures | 200 | 00000 | 98 | success-empty (all-default) → PENDING |
| o3127 | market_session | overseas_futures | 200 | 00000 | 5237 | DATA-CARRIER → flip (U6) |
| t0441 | account | futures_options | 200 | 00000 | 170 | small-body → verify witness before flip |
| t1109 | paginated | stock | 200 | 00000 | 1305 | DATA-CARRIER → flip (U6) |
| t1301 | paginated | stock | 200 | 00000 | 4431 | DATA-CARRIER → flip (U6) |
| t1308 | market_session | stock | 200 | 00000 | 16373 | DATA-CARRIER → flip (U6) |
| t1411 | paginated | stock | 200 | 00000 | 7011 | DATA-CARRIER → flip (U6) |
| t1449 | market_session | stock | 200 | 00000 | 5123 | DATA-CARRIER → flip (U6) |
| t1471 | market_session | stock | 200 | 00000 | 2094 | DATA-CARRIER → flip (U6) |
| t1475 | market_session | stock | 200 | 00000 | 3361 | DATA-CARRIER → flip (U6) |
| t1486 | paginated | stock | 200 | 00000 | 3425 | DATA-CARRIER → flip (U6) |
| t1488 | paginated | stock | 200 | 00000 | 4324 | DATA-CARRIER → flip (U6) |
| t1602 | paginated | stock | 200 | 00000 | 1115 | DATA-CARRIER → flip (U6) |
| t1603 | paginated | stock | 200 | 00000 | 2767 | DATA-CARRIER → flip (U6) |
| t1617 | paginated | stock | 200 | 00000 | 1994 | DATA-CARRIER → flip (U6) |
| t1621 | market_session | stock | 200 | 00000 | 10769 | DATA-CARRIER → flip (U6) |
| t1631 | market_session | stock | 200 | 00000 | 1160 | DATA-CARRIER → flip (U6) |
| t1632 | market_session | stock | 200 | 00000 | 3608 | DATA-CARRIER → flip (U6) |
| t1633 | market_session | stock | 200 | 00000 | 23997 | DATA-CARRIER → flip (U6) |
| t1637 | paginated | stock | 200 | 00000 | 8065 | DATA-CARRIER → flip (U6) |
| t1638 | market_session | stock | 200 | 00000 | 172742 | DATA-CARRIER → flip (U6) |
| t1665 | market_session | stock | 200 | 00000 | 50471 | DATA-CARRIER → flip (U6) |
| t1702 | market_session | stock | 200 | 00000 | 991 | DATA-CARRIER → flip (U6) |
| t1716 | market_session | stock | 200 | 00000 | 30365 | DATA-CARRIER → flip (U6) |
| t1717 | market_session | stock | 200 | 00000 | 664 | DATA-CARRIER → flip (U6) |
| t1752 | paginated | stock | 200 | 00000 | 6570 | DATA-CARRIER → flip (U6) |
| t1771 | paginated | stock | 200 | 00000 | 31412 | DATA-CARRIER → flip (U6) |
| t1902 | market_session | stock | 200 | 00000 | 4437 | DATA-CARRIER → flip (U6) |
| t1904 | market_session | stock | 200 | 00000 | 2406 | DATA-CARRIER → flip (U6) |
| t1906 | market_session | stock | 200 | 00000 | 1784 | DATA-CARRIER → flip (U6) |
| t1927 | market_session | stock | 200 | 00000 | 5090 | DATA-CARRIER → flip (U6) |
| t1941 | market_session | stock | 200 | 00000 | 18272 | DATA-CARRIER → flip (U6) |
| t1950 | market_session | stock | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t1951 | paginated | stock | 200 | 00000 | 64 | success-empty (all-default) → PENDING |
| t1954 | market_session | stock | 200 | 00000 | 61 | success-empty (all-default) → PENDING |
| t1956 | market_session | stock | 200 | 00000 | 57 | success-empty (all-default) → PENDING |
| t1959 | market_session | stock | 200 | 00000 | 156990 | DATA-CARRIER → flip (U6) |
| t1969 | market_session | stock | 200 | EMPTY | 1733536 | empty/no-feed → PENDING / paper_incompatible (re-confirm) |
| t1971 | market_session | stock | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t1972 | market_session | stock | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t1973 | paginated | stock | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t1974 | market_session | stock | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t2106 | market_session | futures_options | 200 | 00000 | 62 | success-empty (all-default) → PENDING |
| t2210 | market_session | futures_options | 200 | 00000 | 149 | small-body → verify witness before flip |
| t2212 | paginated | futures_options | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t2214 | paginated | futures_options | 200 | 00000 | 170 | small-body → verify witness before flip |
| t2407 | paginated | futures_options | 200 | 00000 | 80 | success-empty (all-default) → PENDING |
| t2424 | market_session | futures_options | 200 | 00000 | 200 | small-body → verify witness before flip |
| t2541 | paginated | futures_options | 200 | 00000 | 1170 | DATA-CARRIER → flip (U6) |
| t2545 | market_session | futures_options | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t8404 | paginated | futures_options | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t8406 | market_session | futures_options | 200 | 00000 | 60 | success-empty (all-default) → PENDING |
| t8407 | market_session | stock | 200 | 00000 | 1163 | DATA-CARRIER → flip (U6) |
| t8427 | market_session | futures_options | 200 | 00000 | 4687 | DATA-CARRIER → flip (U6) |
| t8428 | market_session | stock | 200 | 00000 | 421 | small-body → verify witness before flip |
| t8450 | market_session | stock | 200 | 00000 | 2151 | DATA-CARRIER → flip (U6) |
| t8454 | paginated | stock | 200 | 00000 | 4806 | DATA-CARRIER → flip (U6) |
| t8455 | market_session | futures_options | 200 | 00000 | 1498 | DATA-CARRIER → flip (U6) (prev paper_incompat) |
| t8460 | market_session | futures_options | 200 | 00000 | 60 | success-empty (all-default) → PENDING (prev paper_incompat) |
| t8463 | market_session | futures_options | 200 | 00000 | 4759 | DATA-CARRIER → flip (U6) (prev paper_incompat) |

### Flip batches (U6) — carriers are ~32 domestic stock + overseas/F-O; multi-PR mechanical work
- **High-confidence domestic stock carriers (body>500, 00000):** t1109 t1301 t1308 t1411 t1449 t1471 t1475 t1486 t1488 t1602 t1603 t1617 t1621 t1631 t1632 t1633 t1637 t1638 t1665 t1702 t1717 t1752 t1771 t1902 t1904 t1906 t1927 t1941 t1959 t8407 t8450 t8454 — each needs a full implement-tr (struct+facade+policy+offline+typed smoke, witness per KTD2).
- **F/O carriers (domestic_option lane to re-probe for account ones):** t1716 t2210 t2214 t2424 t2541 t8427 t8428 t8463(prev paper_incompat — now 4759B, re-verify night-deriv) t8455(prev paper_incompat — now 1498B).
- **Overseas (need overseas_option lane re-probe):** o3127(5237 market-data), g3190(187).
- **Account (need account context / lane):** CSPBQ00200(00136 no-data→PENDING), t0441(170 all-default→PENDING).
### U7 reclassify (probe-backed)
- **paper_incompatible re-confirmed (01900):** CCENQ10100, CCENQ90200 — flag stands.
- **overseas-stock g31xx re-confirmed no paper feed:** g3101/g3102/g3104/g3106 (26B error), g3103 (00009) — flag stands (§14).
- **success-empty / session-empty → stay PENDING (already tracked-not-implemented):** o3107 t1950 t1951 t1954 t1956 t1969 t1971 t1972 t1973 t1974 t2106 t2212 t2407 t2545 t8404 t8406 t8460.
