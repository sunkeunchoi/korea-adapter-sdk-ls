# U1 — All-Lane Flip Wave: lane/transport classification table

Plan `docs/plans/2026-06-28-003-feat-all-lane-closed-window-flip-wave-plan.md`, U1.
Source pool: 143 raw untracked codes (`crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`).
Probed 2026-06-28 (KRX closed, weekend) via `make raw-probe` (credential-safe: http/rsp_cd/body_len only).
Lanes: `.env` (…01 domestic), `.env.domestic_option` (…51 F/O), `.env.overseas_option` (…71 overseas-F/O).

## Transport split

- **REST: 59** — 14 order/mutation (R3 excluded), 22 already-dispositioned account reads (§16/§17, R11a drop), 23 read survivors probed.
- **WebSocket: 84** — all realtime push market-data/investment-info channels (`owner_class: realtime`), track via `track-realtime-tr`, flip connection-reachable-only.

## REST — dispositions

### FLIP candidates (probed populated under closure) → track + implement

| TR | endpoint | lane | unit | probe body_len | witness |
|---|---|---|---|---|---|
| t3518 | /stock/investinfo | …01 | U4 | 6242 | 해외지수 time-series (NAS@IXIC), 20 rows |
| t3521 | /stock/investinfo | …01 | U4 | 185 | 해외지수 single index (DJI@DJI) — verify single-row at flip |
| o3103 | /overseas-futureoption/chart | …71 | U7 | 2423 | overseas-fut 분봉 chart (CUSN26) |
| o3104 | /overseas-futureoption/market-data | …71 | U7 | 4826 | overseas-fut 일별체결 (date req'd) |
| o3108 | /overseas-futureoption/chart | …71 | U7 | 2338 | overseas-fut 일주월 chart (current dates) |
| o3116 | /overseas-futureoption/market-data | …71 | U7 | 2723 | overseas-fut tick |
| o3117 | /overseas-futureoption/chart | …71 | U7 | 2402 | overseas-fut NTick |
| o3123 | /overseas-futureoption/market-data | …71 | U7 | 2423 | overseas-futopt 분봉 chart |
| o3127 | /overseas-futureoption/market-data | …71 | U7 | 5237 | overseas-futopt 관심종목 board (nrec) |
| o3128 | /overseas-futureoption/market-data | …71 | U7 | 445 | overseas-futopt 일주월 chart |
| o3136 | /overseas-futureoption/market-data | …71 | U7 | 2723 | overseas-futopt tick |
| o3137 | /overseas-futureoption/market-data | …71 | U7 | 2402 | overseas-futopt NTick |
| o3139 | /overseas-futureoption/chart | …71 | U7 | 2417 | overseas-futopt NTick fixed |
| t8462 | /futureoption/investor | …51 | U6 | 6139 | KRX 야간파생 투자자기간별 (recent date range) |

Calibration: implemented siblings o3105=1140, o3106=809; empty out-blocks ≈ 60–99 bytes.
The overseas-futures chart/market-data reads persist last-session data on paper under
closure when given a **current front-month contract** (CUSN26); the raw `req_example`
ships a stale 2023 contract (ADM23) that returns empty — not a feed gap.

### PENDING / track-only (reachable, empty on this account under closure)

| TR | endpoint | lane | probe | reason |
|---|---|---|---|---|
| o3107 | /overseas-futureoption/market-data | …71 | 00000, body 98 | 해외선물 관심종목 (single-symbol watchlist) — empty; account-state watchlist, no registered symbols |

### DROP from tracking (R11a — matches an already-recorded dry terminal; recorded reason, no metadata authored)

**Night-derivative market-data/chart — off-window + weekend empty (§17 t845x precedent):**
`t8456` (80), `t8457` (60), `t8458` (60), `t8459` (60), `t8461` (60) — all `00000` empty
off the krx_extended night window (stale focode `101W6000`); same feed §17 proved
session-gated-empty for t8455/t8460/t8463.

**Overseas-stock charts — no paper feed (§14 overseas-stock precedent):**
`g3202` (rsp_cd empty, body 26 error), `g3203` (rsp_cd empty, body 26 error),
`g3204` (00000, body 61 empty) — overseas-stock carries no paper feed (§14: g31xx sextet).

**Venue rejection `01900` (§12 precedent):**
`CCENQ30100` (KRX 야간파생 주문/체결내역; raw `01900`), `MMDAQ91200` (파생증거금율; known `01900`, §16).

### DROP — R3 order/mutation (14)

`CFOAT00100/00200/00300`, `CCENT00100/00200/00300`, `CIDBT00100/00900/01000`,
`COSAT00301/00311/00400`, `COSMT00300`, `CFOBQ10800` (옵션매도 주문증거금조회 under /order;
also §16 `01900`).

### DROP — already-dispositioned account reads (§16/§17, R11a — 22)

- **paper_incompatible `01900` (§16):** `CSPAQ00600` `FOCCQ33600` `CFOAQ50600`
  `CFOEQ82600` `FOCCQ33700` `COSAQ00102` `COSOQ02701`.
- **IGW40014 proven residual (§16):** `COSOQ00201` (server `002US` literal).
- **empty `00707` across all lanes, retired (§16/§17):** `CSPAQ13700` `CDPCQ04700`
  `CFOFQ02400` `CFOAQ00600` `CIDBQ01500` `CIDBQ01800` `CIDBQ02400` `CIDEQ00800`
  `COSAQ01400` `t0150` `t0151` `t0434`.

## WebSocket — 84 channels → DEFERRED to a follow-up realtime wave (owner decision 2026-06-28)

> **Scope decision (2026-06-28).** U1 probing found the trackable pool (~99) ~2× the
> plan's 30–50 estimate; the 84 WS channels alone would double the realtime surface, each
> a connection-reachable-only proof (KTD6 NOT-OBSERVABLE). The owner chose to **ship the
> REST lane this session and stage the 84-channel WS track+flip as a separate follow-up
> wave** (mirroring the prior 31-channel realtime wave's own 2-PR delivery). The full WS
> classification is recorded below and in the ledger so no candidate is silently dropped
> (R11); WS metadata/flips are **not** authored this session.

All `owner_class: realtime`, `protocol: websocket`, `rate_bucket: market_data`. Flip on a
clean lifecycle smoke (KTD6 NOT-OBSERVABLE → connection-reachable-only); register
`{TR}_POLICY` in the crosscheck array **only**. Grouped by instrument_domain:

- **stock (52):** `B7_ DH1 DHA DK3 DS3 DVI H2_ HB_ I5_ IJ_ K1_ KH_ KM_ KS_ OK_ PH_ PM_
  S4_ SHC SHD SHI SHO VI_ YJ_ YK3 YS3 ESN h2_ h3_ k1_ s2_ s3_ s4_ Ys3 NS3 NH1 NS2 NYS
  NVI NK1 NPH NPM NBT NBM UYS UPH UK1 UBT UBM UPM UVI AFR`
- **futureoption (24):** `CD0 FD0 FX9 JC0 JD0 JH0 JX0 OD0 OMG OX0 YC3 YF9 YJC YOC DC0
  O02 C02 DH0 H02 DD0 DX0 DYC DBM DBT`
- **sector/업종 (1):** `BM_`
- **overseas-futures (2):** `WOC WOH`
- **etc/기타 (2):** `JIF NWS`
- **investment-info (3):** `BMT CUR MK2`

## Summary

- **This session (REST lane):** track + flip **14** REST reads (t3518, t3521,
  o3103/04/08/16/17/23/27/28/36/37/39, t8462) + **1** tracked-PENDING (o3107).
- **Deferred to follow-up WS wave:** 84 WebSocket channels (classified above, recorded
  in the ledger).
- **Dropped (no metadata, recorded reason):** 14 orders (R3) + 22 §16/§17-retired
  account reads + 10 dry REST terminals (5 night-deriv empty, 3 overseas-stock no-feed,
  CCENQ30100/MMDAQ91200 `01900`).
- Every one of the 143 raw codes carries exactly one disposition above (R11).
