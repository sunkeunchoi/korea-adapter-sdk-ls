# Part A — the free public pass: findings and sources

Companion evidence record for
[`krx-licensed-sample-request-package.md`](krx-licensed-sample-request-package.md)
§3. Executed 2026-08-03 against
[#255](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/255), per that
package's decision **P1** (stage the package: free public pass first, then the
paid ask narrowed by what it settles).

**What this file is.** The raw citation record for groups **A1–A7** — every fact,
its source URL, whether the source is `[PRIMARY]` or `[SECONDARY]`, and a status
of `SETTLED` / `PARTIALLY SETTLED` / `NEEDS VENDOR`. The package carries the
*verdicts* and the resulting strikes; this file carries the *evidence*, so a
later reader can re-check a claim without re-running the pass.

**What it is not.** It acquires nothing. Every product fact below is a
**vendor claim read off a public page**, not a verified property of delivered
data. Nothing here discharges an acceptance test in the package's §B.4.

**Read the status labels strictly.** `SETTLED` means established from a primary
source. `PARTIALLY SETTLED` most often means *the fact is established but its
effective date is not*, or *the mechanism is established but its per-product
behaviour is not*. Those are different residuals and the package's Part B/D
questions are narrowed accordingly.

**Standing caution.** The record on this map has been corrected repeatedly.
Where a source is secondary, it is labelled; where a document could not be
reached, that is recorded rather than glossed. Three "could not dereference"
appendices are preserved at the end of their groups so the next pass does not
repeat dead attempts.

---



<!-- ================= a1-findings.md ================= -->

# A1 — KRX equity market-microstructure regime changes (window 2010-01-04 → 2026-08-03)

Research pass A1. Read-only. All dates are KST. "In window" = on or after 2010-01-04.

**Headline for the consuming repo:** the KRX regular-session close **did** change inside the
window. It moved **15:00 → 15:30 on Monday 2016-08-01**. A flat `KRX_REGULAR_CLOSE = 15:30`
constant therefore mis-stamps **every** KOSPI/KOSDAQ/KONEX daily bar for a session on or before
**2016-07-29** (the last trading day at the 15:00 close) by +30 minutes.

---

## 1. Continuous-session close time (and other session-boundary changes)

### 1a. Regular-session close 15:00 → 15:30 — effective **2016-08-01 (Mon)** — IN WINDOW

**Fact.**
Effective Monday **2016-08-01**, KRX extended the *정규시장* (regular/continuous session) of the
securities markets by 30 minutes:

| | before 2016-08-01 | from 2016-08-01 |
|---|---|---|
| 정규시장 매매거래시간 (KOSPI / KOSDAQ / KONEX) | **09:00 – 15:00** (6h) | **09:00 – 15:30** (6h30m) |
| 호가접수시간 (order-entry), regular session | 08:00 – 15:00 | 08:00 – 15:30 (later 08:30 – 15:30, see 1b) |
| closing call auction (종가 단일가) | 14:50 – 15:00 | 15:20 – 15:30 |
| 장종료후 시간외 종가매매 | 15:10 – 15:30 | 15:40 – 16:00 |
| 장종료후 시간외 단일가 / 대량·바스켓 | … – 18:00 | 15:40 – 18:00 |
| derivatives (파생상품시장) regular session | 09:00 – 15:15 | 09:00 – 15:45 |
| KRX gold market (KRX금시장) | 09:00 – 15:00 | 09:00 – 15:30 |

The open (09:00) did **not** move. The after-hours block was shortened by 30 minutes so the overall
market end-time stayed **18:00**. The amended provisions are 유가증권시장 업무규정 제4조
(시장의 구분 및 매매거래시간), 코스닥시장 업무규정 제4조, 코넥스시장 업무규정 제4조,
파생상품시장 업무규정 제4조, and KRX금시장 운영규정 제6조. Announced by KRX at a press
conference on **2016-05-24** (KOSPI Market CEO 김원대); this was the first change to KRX regular
trading hours in 16 years (the previous change was the 2000-05 abolition of the lunch break).

**Sources.**
- **[PRIMARY]** Korea Exchange — *규정/제도 › 매매거래제도 › 유가증권시장 › 매매거래제도일반 ›
  매매거래의 일반절차·휴장일* (current state):
  <https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T1.jsp>
  Verbatim table: "정규시장 09:00 ~ 15:30(6시간30분) / 호가접수시간 08:30 ~ 15:30(7시간);
  시간외시장 장개시전 08:00 ~ 09:00(1시간); 장종료후 15:40 ~ 18:00(2시간20분) / 호가접수 15:30 ~ 18:00.
  주 1) 단, 장개시전 종가매매는 8:30~8:40(10분)". *Establishes the current 15:30 close, not the date.*
- **[PRIMARY]** 법제처 (Korea Ministry of Government Legislation), *찾기쉬운 생활법령정보* —
  "매매거래일·거래시간 및 거래 원칙 등":
  <https://easylaw.go.kr/CSP/CnpClsMain.laf?popMenu=ov&csmSeq=1701&ccfNo=2&cciNo=1&cnpClsNo=2>
  Gives 정규시장 09:00–15:30 citing 유가증권시장/코스닥시장 업무규정 제4조제3항.
  *Establishes the current 15:30 close, not the date.*
- **[PRIMARY]** 금융위원회 (FSC) press release, 2023-06-08, "7.31일부터 파생상품시장 15분 일찍 문
  연다": <https://www.fsc.go.kr/no010101/80141> — states equity market hours are "09:00~15:30"
  and derivatives move to "08:45~15:45". *Corroborates the post-2016 state from a government source.*
- **[SECONDARY, authoritative]** 삼성증권 regulatory customer notice, "한국거래소 매매거래시간 연장
  안내": <https://www.samsungpop.com/ux/kor/customer/notice/notice/noticeViewContent.do?MenuSeqNo=15126>
  — states 시행일 "2016년 8월 1일(월)", 정규시장 "09:00 ~ 15:00" → "09:00 ~ 15:30",
  시간외 종가 "15:10~15:30 → 15:40~16:00", and cites the five amended 업무규정 articles above.
  **This is the single best before/after + effective-date document I could retrieve.**
- **[SECONDARY]** The Korea Times, 2016-05-24, "Stock trading hours to be extended by 30 minutes
  beginning Aug. 1":
  <https://www.koreatimes.co.kr/economy/20160524/stock-trading-hours-to-be-extended-by-30-minutes-beginning-aug-1>
- **[SECONDARY]** The Korea Times, 2016-07-24, "Korean stock markets' trading hours to be extended":
  <https://www.koreatimes.co.kr/economy/20160724/korean-stock-markets-trading-hours-to-be-extended>
  — "From Aug. 1, the markets for stocks and gold close at 3:30 p.m., 30 minutes later than the
  previous 3 p.m.… derivatives market moved its close to 3:45 p.m. from 3:15 p.m."
- **[SECONDARY]** 헤럴드경제, "[주식거래 시간 연장①] 4시간→6시간반…60년 국내증시 매매시간 변동 역사":
  <https://biz.heraldcorp.com/article/967855> — trading-hour history: 1956 (4h, two sessions),
  1998-12 (5h), 2000-05 (lunch break abolished, 6h), 2016-08 (6h30m).

**Status.** `PARTIALLY SETTLED`.
The **fact and the direction** (15:00 → 15:30) and the **current state** are established from
primary sources (KRX's own regulation portal, 법제처, FSC). The **effective date 2016-08-01** is
established only from high-quality secondary sources — a licensed broker's regulatory notice that
cites the amended rule articles verbatim, plus contemporaneous national press. I could **not**
retrieve a primary document stating the date: the KRX 2016 press release is no longer reachable on
`open.krx.co.kr` (the board is AJAX-only and not indexed), `law.krx.co.kr` / `rule.krx.co.kr`
(KRX 법무포털) require a session and reject direct GETs, and the Internet Archive has no pre-2016
capture of the KRX trading-hours page (only 2023-03-06). Confidence in 2016-08-01 is nonetheless
very high: ≥6 mutually independent sources agree on the exact date, and no source disputes it.
**To upgrade to SETTLED**: pull 유가증권시장 업무규정 부칙 (2016 amendment) from KRX 법무포털
`rule.krx.co.kr` in a browser session, or the KRX 보도자료 of 2016-05-24 / 2016-07-31.

**Note.**
- Applies to **KOSPI (유가증권), KOSDAQ, and KONEX alike**, plus derivatives and the KRX gold market.
- The **open never moved** — 09:00 has been the regular-session open across the whole window.
- Because the after-hours block was shortened by exactly the amount the regular session grew, the
  **18:00 end-of-day** is invariant across the window. A pipeline keyed on 18:00 is safe; one keyed
  on 15:30 is not.
- Watch the **last pre-change trading day**: 2016-07-31 was a Sunday, so the final 15:00-close
  session was **Friday 2016-07-29**, and the first 15:30-close session was **Monday 2016-08-01**.

### 1b. 장개시전 시간외시장 + 시가단일가 shortened — effective **2019-04-29 (Mon)** — IN WINDOW

**Fact.** Approved at the FSC's 6th regular meeting on **2019-04-03**, effective **2019-04-29**:

| | before | from 2019-04-29 |
|---|---|---|
| 장개시전 시간외 대량·바스켓매매 | 07:30 – 09:00 | **08:00 – 09:00** |
| 장개시전 시간외 종가매매 | 07:30 – 08:30 | **08:30 – 08:40** |
| 시가 단일가 호가접수 (opening call auction) | 08:00 – 09:00 | **08:30 – 09:00** |
| 시가 단일가 예상체결가 공개 | from 08:00 | **08:40 – 09:00** |

The pre-open after-hours session went from 1h30m to 1h. Rationale: in 2018 only 6.5% of pre-open
block trades printed in 07:30–08:00 vs 93.5% in 08:00–09:00. Amended 유가증권시장 / 코스닥시장 /
코넥스시장 업무규정 (+ 시행세칙 for the indicative-price disclosure window).

**Sources.**
- **[PRIMARY]** 금융위원회 보도자료 2019-04-03, "장개시전 시간외시장 및 시가단일가 시간 단축":
  <https://www.fsc.go.kr/po010101/73613> (also indexed at `fsc.go.kr/no010101/73613`)
- **[PRIMARY]** 대한민국 정책브리핑 (korea.kr) republication of the same FSC release:
  <https://www.korea.kr/news/pressReleaseView.do?newsId=156324779> (detail is in the attached HWP/PDF)
- **[SECONDARY]** 삼성증권, "장개시전 시간외시장 및 시가단일가 시간 변경 안내 (4월 29일)":
  <https://www.samsungpop.com/ux/kor/customer/notice/notice/noticeViewContent.do?MenuSeqNo=16276>
- **[SECONDARY]** 자본시장연구원 (KCMI) 「월간 자본시장 제도동향」 2019년 5월호:
  <https://www.kcmi.re.kr/kcmifile/weekly_jedo/1459/webzinepdf_1459.PDF>

**Status.** `SETTLED` (FSC primary establishes both the decision date and the 2019-04-29 시행일;
the exact before/after clock times come from the attachment + broker notice).

**Note.** This does **not** touch the regular session's open or close. It changes the *pre-market*
block and the opening-auction order-entry start (08:00 → 08:30). If daily-bar or intraday ingest
uses an 08:00 "session start", that boundary **is** date-dependent across 2019-04-29.

### 1c. Derivatives market open 09:00 → 08:45 — effective **2023-07-31 (Mon)** — IN WINDOW, NOT EQUITY

**Fact.** Derivatives regular session became **08:45 – 15:45** (was 09:00 – 15:45); the derivatives
opening call auction shortened 30m → 15m (08:30 – 08:45). **Equity hours unchanged at 09:00–15:30.**

**Source.** **[PRIMARY]** 금융위원회 보도자료, "7.31일부터 파생상품시장 15분 일찍 문 연다":
<https://www.fsc.go.kr/no010101/80141>

**Status.** `SETTLED`. **Note.** Derivatives only — no effect on KOSPI/KOSDAQ equity bars.

### 1d. Opening indicative-price window 08:40 → 08:50 — effective **2025-03-04 (Tue)** — IN WINDOW

**Fact.** With the launch of **Nextrade (NXT)**, Korea's first ATS, on **2025-03-04**, KRX shortened
the disclosure of the opening call auction's indicative execution price from 20 min (08:40–09:00) to
10 min (**08:50–09:00**), to avoid distortion against NXT's pre-market (08:00–08:50). **KRX session
boundaries themselves (09:00–15:30) did not change.** NXT runs a separate venue: pre-market
08:00–08:50, main 09:00:30–15:20, after-market 15:40–20:00.

**Sources.** **[SECONDARY]** 신한투자증권 NXT 제도 가이드:
<https://open.shinhansec.com/mobilealpha/html/CS/NXTPolicyGuide_v2.html> · 미래에셋 NXT 안내:
<https://securities.miraeasset.com/mw/event_ats/main.html>

**Status.** `PARTIALLY SETTLED` (the date 2025-03-04 and the change are consistent across broker
notices; no KRX/FSC primary retrieved). **Note.** Data-disclosure change, not a session boundary.
Relevant only if you consume pre-open indicative prices. NXT prints are a **different venue** — they
do not belong in a KRX daily bar.

### 1e. Planned: after-market 16:00–20:00 — **2026-09-14** (NOT YET EFFECTIVE as of 2026-08-03)

**Fact.** KRX announced on **2026-06-19** that it will open an **애프터마켓 16:00–20:00 from
2026-09-14**, while the **프리마켓 (07:00–07:50) is deferred to end-2027**. The **regular session
stays 09:00–15:30**. Originally slated for 2026-06-29, pushed to 2026-09-14, then split.

**Sources.** **[SECONDARY]** 뉴스핌 2026-06-19, "한국거래소, 프리마켓·애프터마켓 개설 일정 조정":
<https://www.newspim.com/news/view/20260619000738> · 비즈워치:
<https://news.bizwatch.co.kr/article/market/2026/06/19/0028>

**Status.** `PARTIALLY SETTLED` (announced, future-dated, subject to further slippage — it has
already slipped twice; no KRX primary retrieved).

**Note.** **This is a live risk for the consuming repo.** The regular close stays 15:30, so a
15:30-stamped daily bar remains correct — but from 2026-09-14 there will be KRX trading *after*
15:30, and anything that treats 15:30 (or 18:00) as "end of day" needs re-checking.

---

## 2. Tick-size (호가가격단위) table revisions

**Fact.**

**Revision A — effective 2023-01-25 (Wed) — IN WINDOW. CONFIRMED.**
Applied to KOSPI, KOSDAQ, KONEX, single-stock futures, and K-OTC/K-OTCBB, launched together with
KRX's next-generation trading system (EXTURE 3.0). Amendment pre-announced by KRX on **2022-11-01**
(증권·파생상품시장 업무규정 시행세칙 개정예고), comment period 2022-11-02 → 11-08.

| price band | before (2010-10-04 table) | from 2023-01-25 |
|---|---|---|
| < 1,000 | 1 | 1 |
| 1,000 – 2,000 | 5 | **1** |
| 2,000 – 5,000 | 5 | 5 |
| 5,000 – 10,000 | 10 | 10 |
| 10,000 – 20,000 | 50 | **10** |
| 20,000 – 50,000 | 50 | 50 |
| 50,000 – 100,000 | 100 | 100 |
| 100,000 – 200,000 | 500 | **100** |
| 200,000 – 500,000 | 500 | 500 |
| ≥ 500,000 | 1,000 | 1,000 |

Post-change table (KRX's own wording): `<2,000 → 1원; 2,000–5,000 → 5원; 5,000–20,000 → 10원;
20,000–50,000 → 50원; 50,000–200,000 → 100원; 200,000–500,000 → 500원; ≥500,000 → 1,000원`
(7 tiers). **ETF / ETN / ELW keep a flat 5원** (1원 below 2,000 for ETF/ETN).

**Revision B — effective 2010-10-04 (Mon) — ON THE WINDOW BOUNDARY (in window, 9 months after 2010-01-04).**
The prior revision, which established the 7-tier table that stood until 2023-01-24. Reported change:
sub-1,000원 stocks went **5원 → 1원**. KRX itself dates the previous reform as "2010년 10월"; the
precise day 2010-10-04 comes from a market-data aggregation, not a KRX document. A related
유가증권시장 업무규정 시행세칙 amendment (개정 2010-07-29, 시행 2010-11-29) covered 경쟁대량매매
호가 규제, VWAP, and "호가가격단위 개선" — the relationship between the two dates is unresolved.

**Sources.**
- **[PRIMARY]** Korea Exchange — *규정/제도 › 매매거래제도 › 유가증권시장 › 매매거래제도일반 › 호가*:
  <https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T3.jsp>
  Verbatim current table: "2,000원 미만 1원 / 2,000원 이상 5,000원 미만 5원 / 5,000원 이상 20,000원
  미만 10원 / 20,000원 이상 50,000원 미만 50원 / 50,000원 이상 200,000원 미만 100원 / 200,000원 이상
  500,000원 미만 500원 / 500,000원 이상 1,000원", plus the ETF·ETN·ELW single-tick table.
  *Establishes the post-2023 table, not the date.*
- **[SECONDARY, authoritative]** 대신증권, "한국거래소 제도개편 안내" (2023):
  <https://money2.daishin.com/html/Notice/2023/n_07.html> — 시행일 "2023년 1월 25일(수)",
  scope "주식(코스피, 코스닥, 코넥스, K-OTC, K-OTCBB) 및 주식선물", ETF/ETN/ELW keep 5원.
- **[SECONDARY]** 삼성증권, "국내 증권/파생시장 호가가격단위 변경 안내":
  <https://samsungpop.com/ux/kor/customer/notice/notice/noticeViewContent.do?MenuSeqNo=19236>
- **[SECONDARY]** 이코노미스트 2022-11-01, "내년부터 주식 호가단위 낮아진다…12년만의 개선":
  <https://economist.co.kr/2022/11/01/stock/stockNormal/20221101112425954.html> — reports the KRX
  2022-11-01 개정예고 and dates the previous reform to "2010년 10월".
- **[SECONDARY]** 비즈워치 2022-11-01: <https://news.bizwatch.co.kr/article/market/2022/11/01/0005>
- **[SECONDARY]** 뉴데일리 2023-01-17, "'현대차 100원 단위 주문'…25일부터 주식거래 호가 단위 축소":
  <https://biz.newdaily.co.kr/site/data/html/2023/01/17/2023011700164.html>
- **[SECONDARY]** 한화투자증권 호가가격단위 reference: <https://www.hanwhawm.com/main/center/info/CS181_3p.cmd>

**Status.** `PARTIALLY SETTLED`.
- **2023-01-25: CONFIRMED** (the date in the brief is correct). Established from KRX's own current
  tick table (primary, for the *content*) + KRX's 2022-11-01 개정예고 as reported + two independent
  licensed-broker regulatory notices (for the *date*). No KRX press release retrieved directly.
- **2010-10-04: NOT established.** Only aggregator/secondary support for the exact day; KRX
  confirms only the month ("2010년 10월"). Treat the day as provisional. `NEEDS VENDOR` for the day.

**Note.**
- **KOSDAQ diverged from KOSPI before 2023-01-25 in the high-price bands.** Pre-2023 KOSDAQ used a
  finer tick above 100,000원 (100원) than KOSPI (500원). The 2023 unification therefore **coarsened**
  the KOSDAQ tick in the 200,000–500,000원 band from 100원 to **500원** — the one band where the
  2023 "reform" made ticks *bigger*. Any tick-size lookup that is KOSPI-only will be wrong for
  pre-2023 KOSDAQ high-priced names. (Reported secondarily; not confirmed from a KRX document.)
- There was a **basis-price mismatch on 2023-01-25 itself**: 2023-01-20 closes that were not on the
  new grid (e.g. 200,100원) could only trade at the prior close during 08:30–08:40, with the new
  grid (200,000 bid / 200,500 ask) applying 08:40–18:00. Automatic and reserved orders could be
  rejected that day. Expect a one-day artefact.
- KRX states it has revised the tick table **six times since the 1980s**; only the 2010 and 2023
  revisions fall in or near this window.

---

## 3. VI (변동성완화장치 / Volatility Interruption) introduction — arrived in two stages

**Fact.**

| | effective | trigger | action |
|---|---|---|---|
| **동적 VI (dynamic)** | **2014-09-01 (Mon)** | deviation from the **immediately preceding execution price** (≈2–3% continuous / 3–6% depending on session and stock class) | 2-minute call auction |
| **정적 VI (static)** | **2015-06-15 (Mon)** | ±**10%** from the **previous close or the last call-auction price** | 2-minute call auction; the ±10% band then resets |

Dynamic VI was Korea's first individual-stock price-stabilisation device short of the daily price
limit. Static VI was introduced **on the same day as, and as the explicit precondition for**, the
±15% → ±30% price-limit widening (§5), together with the 3-step circuit breaker. The overlapping
단기과열완화장치 was retired at that point. There is **no daily cap on VI activations**.

**Sources.**
- **[PRIMARY]** Korea Exchange — *규정/제도 › 매매거래제도 › 유가증권시장 › 시장운영 및 관리 ›
  종목별 변동성완화장치*: <https://regulation.krx.co.kr/contents/RGL/03/03010409/RGL03010409.jsp>
  (describes the mechanism and the 2-minute cooling period; does not carry effective dates)
- **[PRIMARY]** 금융위원회, 「주식시장 발전방안」 (2014-11) — announces static VI *to be added* to the
  already-introduced dynamic VI, alongside the ±30% price limit and the 3-step CB:
  <https://fsc.go.kr/po010101/71357> · policy-briefing mirror:
  <https://www.korea.kr/news/policyNewsView.do?newsId=148787762>
- **[PRIMARY]** Korea Exchange VI activation data service (operational evidence both VIs are live):
  <http://data.krx.co.kr/contents/MDC/MDI/mdiLoader/index.cmd?menuId=MDC02021501>
- **[SECONDARY]** 뉴시스 2022-05-19, "[금알못] 주식 VI 발동…'변동성 완화장치' 알고 계신가요":
  <https://www.newsis.com/view/NISX20220519_0001878246> — "2014년 9월 1일부터 … 도입되어 시행".
- **[SECONDARY]** 신한금융투자 고객공지 2015-06-05, "주식/파생시장 가격안정화장치 개선(가격제한폭 확대 등)":
  <https://open.shinhansec.com/notice/notice_150605_02.html> — static VI shipping with the 2015-06-15 package.
- **[SECONDARY]** 한국재무학회 『재무연구』, "KRX 정적 VI(종목별 변동성완화장치) 도입의 가격안정화 및
  가격발견 효과: 동적 VI와 비교 분석": <https://www.dbpia.co.kr/journal/articleDetail?nodeId=NODE10774708>
  — academic study treating static VI's introduction as a 2015 event.

**Status.** `PARTIALLY SETTLED`.
- **Static VI 2015-06-15**: effectively settled — it shipped in the same rule package as the ±30%
  price limit, and KRX's own price-limit change table (§5) dates that package to **'15. 6. 15**.
- **Dynamic VI 2014-09-01**: **not** established from a primary source. Multiple independent
  secondary sources agree on 2014-09-01 and none dispute it, but no KRX/FSC document was retrieved
  that states the day. `NEEDS VENDOR` to nail the day.

**Note.**
- **Both KOSPI and KOSDAQ.** KONEX has its own (narrower) regime.
- Static VI applies a **single uniform 10% rate** to all stocks and all sessions; dynamic VI's rate
  varies by stock class and by session (continuous vs call auction) — so a single "VI threshold"
  constant is wrong for dynamic VI.
- **NXT operates dynamic VI only, not static VI** (post-2025-03-04). Cross-venue VI behaviour is
  therefore asymmetric.
- Because static VI (2015-06-15) forces at least two 2-minute auction pauses before a stock can walk
  from flat to +30%, **intraday bar shapes and the time-to-limit distribution are not comparable
  across 2015-06-15**, independently of the price-limit change itself.

---

## 4. Short-sale (공매도) regime changes

**Fact.** Six in-window events, plus one pre-window ban for context.

| # | date | event |
|---|---|---|
| 0 | 2008-10-01 → 2009-05-31 | 1st full ban (GFC). **PRE-WINDOW** (ends before 2010-01-04). |
| 1 | **2011-08-10** | 2nd full ban begins (Eurozone crisis). All listed stocks. |
| 2 | **2011-11-09** | 2nd ban lifted; short selling resumes on all stocks. |
| 3 | **2020-03-16** | 3rd full ban begins (COVID-19). FSC decision **2020-03-13**; initially 6 months to 2020-09-15. Extended **2020-08-27** for a further 6 months (2020-09-16 → 2021-03-15), then again to 2021-05-02. |
| 4 | **2021-05-03** | **Partial** lift: shorting resumes for **KOSPI 200** and **KOSDAQ 150** constituents only (~350 names). All other stocks remain banned indefinitely. Decided at the FSC's 1st extraordinary meeting **2021-02-03**. |
| 5 | **2023-11-06** | 4th full ban begins — KOSPI + KOSDAQ + KONEX **all** stocks, through 2024-06-30 (later extended to 2025-03-30). FSC extraordinary decision **2023-11-05**. Motive was regulatory (eradicating naked shorting after global-IB breaches), not crisis stabilisation. |
| 6 | **2025-03-31** | **Full** resumption across all ~2,700 KRX-listed stocks. Ends 17 months for KOSPI200/KOSDAQ150 constituents and ~5 years for everything else. |

Market makers and liquidity providers were **exempt from every full ban from 2020 onward** — so
"ban" never meant zero short volume.

**Uptick rule (업틱룰).** Korea has a standing uptick rule: a short-sale quote may not be entered at
or below the last execution price. The in-window change is the **abolition of the equity market
maker's uptick-rule exemption**, announced by FSC in **December 2020** and implemented **in H1 2021**
(the number of uptick-rule exceptions went from 12 to 7, reported as taking effect **2021-03**).
Remaining exceptions: arbitrage, derivatives-MM hedging, ETF/ETN/ELW LP hedging, and negotiated
(off-auction block) trades.

**Sources.**
- **[PRIMARY]** 금융위원회 보도참고자료 2025-02, "3월 31일부터 공매도를 전면 재개합니다.":
  <https://www.fsc.go.kr/no010101/84216> · full attachment:
  <https://www.fsc.go.kr/comm/getFile?srvcId=BBSTY1&upperNo=84216&fileTy=ATTACH&fileNo=1>
  — verbatim: "'25.3.31일(월)부터 예정대로 공매도가 전면 재개… 코스피200·코스닥150 종목은
  17개월('23.11.6~), 그 외 종목은 약 5년('20.3.16~) 만의 재개". **This single FSC document pins
  events 3, 5 and 6.**
- **[PRIMARY]** 금융위원회 보도자료 2023-11-05, "내년 상반기까지 공매도 전면금지":
  <https://www.fsc.go.kr/no010101/81013> — start **2023-11-06 (Mon)**, end 2024-06-30,
  scope "코스피·코스닥·코넥스 전종목", MM/LP exempt.
- **[PRIMARY]** 금융위원회 보도참고자료 2021-02-03, "5월 2일까지 주식시장 공매도 금지조치 연장…":
  <https://www.fsc.go.kr/no010101/75290> — 2020-03-16 start, 2020-08-27 extension, and the
  2021-05-03 partial resumption for KOSPI200/KOSDAQ150.
- **[PRIMARY]** 금융위원회 보도자료 2021-04-29, "5월 3일부터 공매도를 부분 재개합니다.":
  <https://www.fsc.go.kr/no010101/75830> · policy-briefing mirror:
  <https://www.korea.kr/news/pressReleaseView.do?newsId=156449411>
- **[PRIMARY]** 금융위원회 보도참고자료 2020-03, "공매도 금지 및 자기주식 취득한도 확대":
  <https://www.fsc.go.kr/no010101/74509>
- **[PRIMARY]** 금융위원회 용어사전, "업틱룰(uptick rule)":
  <https://www.fsc.go.kr/in090301/view?dicId=1757>
- **[PRIMARY]** 금융위원회 보도자료 (시장조성자 제도개선, 2020-12): <https://www.fsc.go.kr/no010101/74979>
- **[SECONDARY]** 자본시장연구원 (KCMI), "Short Selling Resumption in Korea: Regulatory Reform and
  Market Implications":
  <https://www.kcmi.re.kr/en/publications/pub_detail_view?syear=2025&zcd=002001017&zno=1840&cno=6517>
  — the four-ban taxonomy; **also flags an internal date conflict for 2011 (see Note)**.
- **[SECONDARY]** 자본시장연구원, "공매도 규제효과 분석": <https://www.kcmi.re.kr/report/report_view?report_no=1521>
- **[SECONDARY]** CNBC 2025-03-31, "South Korea ends its longest short-selling ban after systemic reforms":
  <https://www.cnbc.com/2025/03/31/south-korea-ends-its-longest-short-selling-ban-in-history-after-systemic-reforms.html>
- **[SECONDARY]** 노컷뉴스, "개미 분노 산 '시장조성자' 혜택 줄인다…업틱룰 면제 폐지":
  <https://www.nocutnews.co.kr/news/5468103>

**Status.** `PARTIALLY SETTLED`.
- Events **3, 4, 5, 6 (2020-03-16 / 2021-05-03 / 2023-11-06 / 2025-03-31): SETTLED** from FSC
  primary sources.
- Events **1 and 2 (2011-08-10 / 2011-11-09)**: secondary only (KCMI). No 2011 FSC release retrieved
  — FSC's site does not surface 2011 material in search. **And the start date is contested** (below).
- **Uptick-rule change: NEEDS VENDOR** for a precise effective date — sources give
  "2021년 상반기" / "2021년 3월" but no day.

**Note.**
- **CONTESTED: the 2011 ban start.** KCMI publishes **both** 2011-08-10 and **2011-08-11** in
  different reports. The most plausible reconciliation is that 2011-08-10 is the FSC decision/
  announcement date and 2011-08-11 the first banned session, but this is inference, not sourced.
  **Record both.** The lift date 2011-11-09 is not contested.
- **The 2023 ban start is 2023-11-06, not 2023-11-03.** Several English outlets print 2023-11-03;
  the FSC's own release says the extraordinary decision was **2023-11-05** (a Sunday) and the ban
  ran **from 2023-11-06 (Mon)**. Prefer 2023-11-06.
- **2021-05-03 → 2023-11-05 is a split-universe regime**, not a clean "shorting allowed" period:
  only KOSPI200 / KOSDAQ150 constituents were shortable, and **index membership was rebalanced
  semi-annually (June and December)**, so the shortable set *changed twice a year within the
  regime*. Any per-symbol shortability flag must be date- **and** index-membership-aware.
- Full bans applied to **KOSPI, KOSDAQ and KONEX** alike.
- 2025-03-31 shipped alongside the **NSDS** (Naked Short-selling Detection System) at KRX, plus a
  temporary 2-month (to 2025-05-31) expansion of the 공매도 과열종목 designation regime — so
  2025-03-31 → 2025-05-31 is itself a transitional sub-regime.

---

## 5. Daily price-limit band width

**Fact.** KRX publishes its **own change history**. Verbatim from the KRX regulation portal:

| effective | 가격제한폭 |
|---|---|
| before 1995-04 | 정액제 — 17 bands by price level, avg 4.6% (2.2–6.7%) |
| 1995-04-01 | 정률제 6% |
| 1996-11-24 | 8% |
| 1998-03-02 | 12% |
| 1998-12-07 | 15% |
| **2015-06-15** | **30%** |

So the **only in-window change is ±15% → ±30%, effective 2015-06-15 (Mon)**, applying to
**유가증권시장 (KOSPI) and 코스닥시장 (KOSDAQ)** — 주권, DR, ETF, ETN, 수익증권.
It shipped as one package with static VI (§3) and the 3-step circuit breaker (8% / 15% / 20%,
with 20% = same-day shutdown), replacing the old single 10% CB.

**KOSDAQ-specific difference:** none *after* 2015-06-15 — KOSDAQ is also ±30%, and KRX's KOSDAQ page
states the limit as "기준가격 대비 상하 30%". The KOSDAQ difference is *historical*: KOSDAQ reached
±15% only in 2005 (KOSPI in 1998-12-07), so a pre-2015 KOSDAQ band history diverges from the KOSPI
table above before 2005 — outside this window.

**KONEX difference (still live):** KONEX remained at **±15%** and is ±15% today, except
시간외 대량매매 which uses ±30%.

**Other still-live exclusions:** no price limit at all for 정리매매종목 (delisting-liquidation),
ELW, 신주인수권증서, 신주인수권증권. **Leverage ETFs get the band multiplied by leverage**
(a 2× leveraged ETF has a ±60% band) — a flat ±30% validator will false-reject leveraged ETFs.

**Sources.**
- **[PRIMARY]** Korea Exchange — *규정/제도 › 매매거래제도 › 유가증권시장 › 매매거래제도일반 ›
  가격제한폭 제도*: <https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T5.jsp>
  Carries the **"가격제한폭 변경 내역"** table quoted above verbatim, including "'15. 6. 15 | 30".
  This is KRX's own change register — the strongest single artefact found in this whole pass.
- **[PRIMARY]** Korea Exchange — 코스닥시장 › 기준가격/가격제한폭/상하한가:
  <https://regulation.krx.co.kr/contents/RGL/03/03020201/RGL03020201.jsp> — "가격제한폭 기준가격
  대비 상하 30%임".
- **[PRIMARY]** Korea Exchange — 코넥스시장 › 기준가격/가격제한폭:
  <https://regulation.krx.co.kr/contents/RGL/03/03030207/RGL03030207.jsp> — "코넥스시장은 …
  1일 가격제한폭을 기준가격 대비 상하 15%(유가증권∙코스닥시장은 상하 30%)로 제한".
- **[PRIMARY]** 금융위원회 「주식시장 발전방안」 (2014-11): <https://fsc.go.kr/po010101/71357> ·
  <https://www.korea.kr/news/policyNewsView.do?newsId=148787762>
- **[SECONDARY]** 서울경제 2015, "'가격제한폭 ±30% 확대' 6월 15일 시행 확정":
  <https://www.sedaily.com/NewsView/1HMWUIEL3W>
- **[SECONDARY]** 글로벌이코노믹 2015-06-14, "한국거래소, 내일부터 가격제한폭 ±30%로 확대":
  <https://www.g-enews.com/article/Securities/2015/06/201506141049131993556_1>
- **[SECONDARY]** 한울회계법인/Crowe, "증권·파생상품시장 가격제한폭 확대시행(한국거래소, 2015.5.19)":
  <https://www.crowe.com/kr/news/news20150529_kr>
- **[SECONDARY]** 신한금융투자 고객공지 2015-06-05:
  <https://open.shinhansec.com/notice/notice_150605_02.html> (scope + exclusions)

**Status.** `SETTLED`. Effective date, prior value, current value, KOSDAQ parity and KONEX
divergence are all established from Korea Exchange's own regulation pages.

**Note.** Derivatives were widened in the same package (from ±10–30% to ±8–60%) and moved to a
**3-stage sequential** band that expands as equity circuit breakers fire — so a single derivatives
band constant is wrong post-2015-06-15 in both value and shape.

---

## In-window regime changes, chronologically

Every established change with an effective date on or after 2010-01-04, sorted. This is the
comparability floor: a backtest or statistical study that spans any of these rows is not
apples-to-apples across it.

| # | effective date | dimension | change | markets | status |
|---|---|---|---|---|---|
| 1 | **2010-10-04** *(day provisional)* | Tick size | tick table revised to the 7-tier structure that stood until 2023-01-24; sub-1,000원 tick 5원 → 1원 | KOSPI, KOSDAQ | provisional (secondary; KRX confirms month only) |
| 2 | **2011-08-10** *(or 2011-08-11 — contested)* | Short sale | 2nd full ban begins (Eurozone crisis) | all | secondary; date contested |
| 3 | **2011-11-09** | Short sale | 2nd ban lifted, all stocks | all | secondary |
| 4 | **2014-09-01** | VI | **dynamic VI** introduced (2-min call auction on deviation from last trade) | KOSPI, KOSDAQ | secondary |
| 5 | **2015-06-15** | Price limit **+** VI **+** CB | daily band **±15% → ±30%**; **static VI** introduced (±10% vs prev. close); circuit breaker → 3 steps (8/15/20%) | KOSPI, KOSDAQ (KONEX stays ±15%) | **PRIMARY (KRX)** |
| 6 | **2016-08-01** | **Session hours** | **regular-session close 15:00 → 15:30**; open unchanged at 09:00; closing auction 14:50–15:00 → 15:20–15:30; after-hours shortened 30m; day still ends 18:00 | KOSPI, KOSDAQ, KONEX, derivatives, gold | primary for state, secondary for date |
| 7 | **2019-04-29** | Session hours (pre-market) | pre-open after-hours 07:30–09:00 → **08:00–09:00**; pre-open closing-price trade → 08:30–08:40; opening-auction order entry 08:00 → **08:30** | KOSPI, KOSDAQ, KONEX | **PRIMARY (FSC)** |
| 8 | **2020-03-16** | Short sale | 3rd full ban begins (COVID); extended 2020-09-16 and again to 2021-05-02 | all | **PRIMARY (FSC)** |
| 9 | **2021-03** *(day unknown)* | Short sale / uptick | equity market-maker **uptick-rule exemption abolished**; exceptions 12 → 7 | KOSPI, KOSDAQ | NEEDS VENDOR (day) |
| 10 | **2021-05-03** | Short sale | **partial** lift — KOSPI 200 + KOSDAQ 150 constituents only (~350 names); membership rebalanced each June/December | KOSPI, KOSDAQ | **PRIMARY (FSC)** |
| 11 | **2023-01-25** | Tick size | tick table revised — 1,000–2,000원 5→1; 10,000–20,000원 50→10; 100,000–200,000원 500→100; KOSPI/KOSDAQ/KONEX unified (KOSDAQ 200,000–500,000원 **coarsened** 100→500); ETF/ETN/ELW keep 5원 | KOSPI, KOSDAQ, KONEX, stock futures, K-OTC | primary for table, secondary for date |
| 12 | **2023-07-31** | Session hours (derivatives) | derivatives open 09:00 → **08:45**; equity unchanged | derivatives only | **PRIMARY (FSC)** |
| 13 | **2023-11-06** | Short sale | 4th full ban begins, all stocks (KOSPI+KOSDAQ+KONEX); MM/LP exempt | all | **PRIMARY (FSC)** |
| 14 | **2025-03-04** | Market structure | **Nextrade (NXT)** ATS launches (separate venue); KRX opening indicative-price window 08:40 → **08:50**; KRX session boundaries unchanged | KOSPI, KOSDAQ (~800 NXT-eligible names) | secondary |
| 15 | **2025-03-31** | Short sale | **full** resumption, all ~2,700 listed stocks; NSDS live; 과열종목 regime temporarily expanded to 2025-05-31 | all | **PRIMARY (FSC)** |
| — | *2026-09-14 (planned)* | Session hours | **after-market 16:00–20:00** to open; regular session stays 09:00–15:30; pre-market 07:00–07:50 deferred to end-2027 | KOSPI, KOSDAQ | announced, not yet effective |

---

## Direct answer

> **Did the KRX regular-session close time change on or after 2010-01-04? — YES.**
>
> **Date: 2016-08-01 (Monday). The close moved from 15:00 KST to 15:30 KST.** The open did not
> change (09:00 throughout the window). The last session that closed at 15:00 was **Friday
> 2016-07-29**; the first session that closed at 15:30 was **Monday 2016-08-01**. The change applied
> to KOSPI (유가증권시장), KOSDAQ and KONEX simultaneously, via amendments to 제4조 of each market's
> 업무규정.
>
> Consequence for a flat `KRX_REGULAR_CLOSE = 15:30` constant: **every daily bar for a session on or
> before 2016-07-29 is stamped 30 minutes after the market actually closed.** For the window opening
> at 2010-01-04 that is roughly **1,630 trading sessions** (2010-01-04 … 2016-07-29) mis-stamped —
> about **six and a half years** of the history. The fix is a date-switched close:
> `close = 15:00 if session_date < 2016-08-01 else 15:30`.
>
> Secondary boundary to fix at the same time: if anything in the pipeline treats **08:00** as the
> session/pre-market start, that boundary moved to **08:30** on **2019-04-29** (opening-auction order
> entry), with the pre-open block moving 07:30 → 08:00.


<!-- ================= a2-a3-findings.md ================= -->

# A2 / A3 — KRX market-data procurement research findings

Research date: 2026-08-03. Read-only. All facts below carry a source URL and a
reachability marker. Nothing here is asserted without a source.

---

# A2 — KRX historical-data product coverage claims

## Headline: the login wall is thinner than the prior investigation concluded

The prior finding — that product detail sits behind a free KRX membership login —
is **correct about the catalog *page* layer and wrong about the *data* layer.**

- The *list* pages (`viewNm=dataProdList&prodType=ST|EP`, and every `viewNm=MDCDATA0xx`
  per-product page) return HTTP 200 with a **407–435 byte body containing only a
  JavaScript redirect** — no product names, no codes, nothing:
  `alert('로그인 또는 회원가입이 필요합니다.'); location.href='/contents/MDC/COMS/client/MDCCOMS001.cmd?...'`
- But the **detail page** `index.cmd?viewNm=dataProdDetail&prodSpecId=<CODE>&prodType=<ST|EP>`
  renders **fully, anonymously** (43 KB for ST5001), and every AJAX endpoint it
  calls is **unauthenticated**.
- And the official price book — **"KRX 데이터 상품 안내", 2026.1, 20 pp** — is
  downloadable without an account.

KRX's own purchase flow ([openapi.krx.co.kr/contents/OPP/DATA/OPPDATA001.jsp](https://openapi.krx.co.kr/contents/OPP/DATA/OPPDATA001.jsp))
declares STEP 01 = 회원가입 및 로그인, STEP 02 = 상세 확인 (기본사양, 제공기간, 데이터
항목, 데이터 전달방식, 샘플파일 및 가격) — i.e. KRX *intends* the six facts to be
post-login. They are obtainable anyway.

### Public routes that actually yield the facts (all `[PRIMARY]`, no login)

| Route | Yields |
|---|---|
| `GET https://data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd?viewNm=dataProdDetail&prodSpecId=<CODE>&prodType=<ST\|EP>` | Official 개요 + 기본사양 + delivery terms |
| `POST https://data.krx.co.kr/contents/MDC/DATA/datasale/getDatasaleProdItmList.cmd` (`prodSpecId`, `mandtryYn=Y`, `locale=ko_KR`\|`en_US`) | Complete field list (`DATA_ITM_COL_ID` + `DATA_ITM_KOR_NM`), Korean and English |
| `POST https://data.krx.co.kr/contents/MDC/DATA/datasale/getSampleList.cmd` (`prodSpecId`, `prodKind=A`) | 5 real sample rows (T−30 trading days) |
| `GET https://data.krx.co.kr/inc/js/mdc.util.js` → function `getStrtDt1()` | Authoritative first-available-date table, every product × market × cycle |
| `Market_Data_List_And_Price.pdf` via `/comm/fileDn/GenerateOTP/generate.cmd` → `/comm/fileDn/download_att/download.cmd` | The 2026.1 price book: field specs + data types + full price tables + delivery/refund terms (an anonymous `JSESSIONID` is required; a bare cookieless request 403s) |

### Still login-gated

| Route | Behaviour |
|---|---|
| `POST .../getPredictionInfo.cmd` | `{"resultCd":"E002"}` — **LOGIN-GATED**. Quoted price for a *specific* market/date-range/field selection. |
| `bld=dbms/MDC/DATA/selectDataProdList` | `{"output":[]}` — **LOGIN-GATED**. The in-UI product-name + 제공가능기간 grid. |
| `.../datasale/MDCDATA100.cmd` (비정형 / custom-extract data) | HTTP **302** → `MDCCOMS001.cmd` — **LOGIN-GATED**. |

### Verification and provenance note

Every A2 fact below was **independently re-derived** during this pass, not merely
relayed: the ST5001 detail page was re-fetched (43,008 bytes, `로그인 또는 회원가입`
absent), `getDatasaleProdItmList.cmd` was re-called for all six codes, the
`getStrtDt1()` table was re-read out of `mdc.util.js`, and the price rows were
re-read out of the PDF text. Field counts and the Level-II determination below
are first-hand.

One provenance caveat, stated because it matters for the source standard: the
**code ↔ page mapping** (`MDCDATA0xx` → product code) was initially recovered
from Wayback captures of KRX's *own* pages, because the live list pages are
behind the wall. That mapping was then used only to *locate* live endpoints —
**every reported fact traces to a live primary KRX endpoint**, not to an archive.
No fact below rests on a cached page.

---

## Load-bearing sub-question: is ST5001 Level I or Level II?

**Neither — it is *stronger* than Level II. ST5001 is order-by-order (MBO /
market-by-order) event data with a synchronized 10-level book snapshot attached
to every event, at millisecond resolution.**

Official KRX 개요, verbatim, PUBLIC at
`https://data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd?viewNm=dataProdDetail&prodSpecId=ST5001&prodType=ST`:

> 「주식 호가장」은 한국거래소에 상장된 주식의 장중 호가 정보를 **밀리초 단위**로
> 제공합니다. **개별주문 단위의 전체 호가정보**와 그에 따른 매수/매도 호가정보가
> **10단계까지** 제공되며, 매수/매도 총호가잔량, 누적체결수량 및 누적거래대금 등의
> 항목이 포함됩니다.

The 104-field list (re-fetched first-hand) corroborates three distinct layers:

1. **Per-order event identity** — `ORD_ACPT_NO` (호가접수번호, order receipt ID),
   `ORD_PRIOR_NO` (queue priority), `MODCANCL_TP_CD` (new / modify / cancel),
   `ORGN_ORD_ACPT_NO` (links a modify/cancel back to its parent order),
   `ORD_QTY`, `ORD_PRC`, `ORD_TP_CD`, `ORD_COND_CD`, `ORD_ACPT_TM`.
2. **10-level book snapshot per event** — `ASK_STEP1..10_BSTORD_PRC/_RQTY`,
   `BID_STEP1..10_BSTORD_PRC/_RQTY`, side totals, 10-step aggregates, prior-state
   mirrors, plus LP-only residual size at 5 levels per side.
3. **Participant attribution** — `INVST_TP_CD` (investor type), `CNTR_CD`
   (country), `TRST_PRINC_TP_CD` (agency vs principal), `FORNINVST_TP_CD`,
   `ORD_MEDIA_TP_CD` (order channel), `MBR_ORD_TM`.

Mechanically verified across all six codes (first-hand, `getDatasaleProdItmList.cmd`):

| Code | Fields | Order-level (`ORD_ACPT_NO`) | 10-level depth (`STEP10`) | Best-quote-only |
|---|---|---|---|---|
| ST1002 | 34 | no | no | **yes** (Level I) |
| ST5001 | 104 | **yes** | **yes** | no |
| ST5002 | 122 | **yes** | **yes** | no |
| EP1004 | 11 | no | no | no (no quote fields at all) |
| EP5001 | 105 | **yes** | **yes** | no |
| EP5002 | 122 | **yes** | **yes** | no |

**Caveat, from the price book's Appendix (호가체결장 상세정보):** `BRD_ID` /
`SESS_ID` / `MKT_ID` are only produced **after the EOS migration of 2018-07-16**;
`REGUL_OFFHR_TP_CD` / `BLKTRD_TP_CD` / `MKT_TP_CD` are **discontinued after** that
date. KRX further warns that pre-2010 data may omit items or use different
definitions and **explicitly declines to treat that as a defect** — relevant to
any refund or fitness claim.

---

## ST1002 — 주식 일중 매매정보 (Stock Intraday Trading Information)

| Fact | Value | Source | Reach |
|---|---|---|---|
| What it is | 1-minute and 10-minute **bars**. 34 fields: OHLCV + accumulated volume/value + **best bid/ask price and size only** (`BID_FSTBSTORD_PRC`, `ASK_FSTBSTORD_PRC`, `_RQTY`) → **Level I**. Plus ~17 KRX-derived microstructure metrics flagged 가공분석 정보 (quoted/effective/realized spread, IOC & FOK cancel volume, cancel rates, order and trade imbalance, adverse-selection cost, depth by size and by count). | `dataProdDetail&prodSpecId=ST1002&prodType=ST`; `getDatasaleProdItmList.cmd` | PUBLIC |
| First available date | 유가증권 **1999-10**, 코스닥 **1999-10**, 코넥스 **2013-07** (Min cycle) | `mdc.util.js` `getStrtDt1()` | PUBLIC |
| Field list | 34 fields, Korean + English names | `getDatasaleProdItmList.cmd` | PUBLIC |
| Delivery | CSV. Web download ≤500MB · email 500MB–5GB · external HDD >5GB · sFTP ≤30MB by arrangement | detail page + price book §3–6 | PUBLIC |
| Sample | **Yes, free** — 5 rows; observed first row `20260703 09:00 STK KR7000210005` | `getSampleList.cmd` | PUBLIC |
| Price (기준가격, 전체항목/1년) | 10분: 유가/코스닥 **990,000원** (개별 49,000), 코넥스 495,000 (49,000). 1분: 유가/코스닥 **1,485,000원** (개별 74,000), 코넥스 742,000 (74,000) | KRX 데이터 상품 안내 2026.1 | PUBLIC |
| English product name | — | — | **NOT FOUND** |

**Status: SETTLED**

Naming note: a 2023 catalog page titled this 「일중 시세정보」; the 2026.1 price
book and the current detail page both say 「일중 매매정보」. Same code.

## ST5001 — 주식 호가장 (Stock Order/Quote Book)

| Fact | Value | Source | Reach |
|---|---|---|---|
| What it is | **Millisecond, per-order (MBO) full quote-event stream with a 10-level bid/ask book snapshot on every event.** 104 fields. Not Level I; richer than Level II. | `dataProdDetail&prodSpecId=ST5001&prodType=ST` | PUBLIC |
| First available date | 유가증권 **1999-10**, 코스닥 **1999-10**, 코넥스 **2013-07**; cycle `1TICK` | `mdc.util.js` `getStrtDt1()` | PUBLIC |
| Field list | 104 fields (see three-layer breakdown above) | `getDatasaleProdItmList.cmd` | PUBLIC |
| Delivery | CSV via **cloud download — 호가장/체결장 bypass the normal size caps**. Extraction ~2 weeks, can exceed a month. | detail page (`* 단 호가장, 체결장 … 용량에 관계없이 클라우드 다운로드`) + price book | PUBLIC |
| Sample | **Yes, free** — 5 rows, observed dated `20260703` | `getSampleList.cmd` | PUBLIC |
| Price (기준가격, 전체항목/1년) | 선택형: 유가증권 **10,000,000원** · 코스닥 **5,800,000원** · 코넥스 **3,000,000원** · 개별종목 **500,000원**. 구독형: 유가 20,000,000 / 코스닥 11,600,000 / 코넥스 6,000,000원/yr | KRX 데이터 상품 안내 2026.1 | PUBLIC |
| English product name | — | — | **NOT FOUND** |

**Status: SETTLED** — including the Level-I/Level-II question, which resolves
decisively in favour of full depth plus order-level detail.

## ST5002 — 주식 체결장 (Stock Trade Book)

| Fact | Value | Source | Reach |
|---|---|---|---|
| What it is | Millisecond **trade** records + 10-level book snapshot at trade time, **with order attribution on both sides** (`ASKORD_ACPT_NO` / `BIDORD_ACPT_NO`, priority numbers, receipt times, and investor type / country / capacity / channel per side). 122 fields. The shared receipt IDs make **trade↔order joins against ST5001 feasible**. | `dataProdDetail&prodSpecId=ST5002&prodType=ST`; field list | PUBLIC |
| First available date | 유가증권 **1999-10**, 코스닥 **1999-10**, 코넥스 **2013-07** | `mdc.util.js` `getStrtDt1()` | PUBLIC |
| Field list | 122 fields | `getDatasaleProdItmList.cmd` | PUBLIC |
| Delivery | CSV, cloud download (size caps bypassed) | detail page + price book | PUBLIC |
| Sample | **Yes, free** — 5 rows, `20260703` | `getSampleList.cmd` | PUBLIC |
| Price | Identical table to 호가장: 유가 **10,000,000** · 코스닥 **5,800,000** · 코넥스 **3,000,000** · 개별 **500,000원**; 구독형 20,000,000 / 11,600,000 / 6,000,000 | KRX 데이터 상품 안내 2026.1 | PUBLIC |
| English product name | — | — | **NOT FOUND** |

**Status: SETTLED**

## EP1004 — 증권상품 일중 매매정보 (ETP Intraday Trading Information)

| Fact | Value | Source | Reach |
|---|---|---|---|
| What it is | 1-minute and 10-minute bars for ETF / ETN / ELW. **11 fields only**: 거래일자, 거래시각, 증권그룹ID, 종목코드, 종목명, 시가, 고가, 저가, 종가, 거래량, 거래대금. **No quote fields at all** — not even best bid/ask — and **none** of ST1002's derived microstructure metrics. | `dataProdDetail&prodSpecId=EP1004&prodType=EP`; field list | PUBLIC |
| First available date | ETF **2002-10**, ETN **2014-11**, ELW **2005-12** (Min cycle) | `mdc.util.js` `getStrtDt1()` | PUBLIC |
| Field list | 11 fields | `getDatasaleProdItmList.cmd` | PUBLIC |
| Delivery | CSV, standard download ladder | detail page + price book | PUBLIC |
| Sample | **Yes, free** — 5 rows, `20260703`, e.g. TIGER 엔비디아미국채커버드콜밸런스(합성) | `getSampleList.cmd` | PUBLIC |
| Price | 10분 **400,000원**/yr (개별 40,000); 1분 **600,000원**/yr (개별 60,000) | KRX 데이터 상품 안내 2026.1 | PUBLIC |
| English product name | — | — | **NOT FOUND** |

**Status: SETTLED.** Note the asymmetry worth flagging: EP1004 is materially
thinner than its stock counterpart ST1002 — no quote data whatsoever.

## EP5001 — 증권상품 호가장 (ETP Order/Quote Book)

| Fact | Value | Source | Reach |
|---|---|---|---|
| What it is | Same architecture as ST5001 — millisecond per-order events + 10-level book — for **ETF, ETN, ELW**. **105 fields** (one more than ST5001: adds `THEO_PRC` 이론가격). | `dataProdDetail&prodSpecId=EP5001&prodType=EP`; field list | PUBLIC |
| First available date | ETF **2002-10**, ETN **2014-11**, ELW **2005-12** (Tick) | `mdc.util.js` `getStrtDt1()` | PUBLIC |
| Field list | 105 fields | `getDatasaleProdItmList.cmd` | PUBLIC |
| Delivery | CSV, cloud download (size caps bypassed) | detail page + price book | PUBLIC |
| Sample | **Yes, free** — 5 rows, `20260703`, `KR7455850008` | `getSampleList.cmd` | PUBLIC |
| Price | 선택형 **3,000,000원**/yr (priced in the 코넥스/ETF/ETN/ELW row), 개별종목 500,000원; 구독형 **6,000,000원**/yr | KRX 데이터 상품 안내 2026.1 | PUBLIC |
| English product name | — | — | **NOT FOUND** |

**Status: SETTLED**

## EP5002 — 증권상품 체결장 (ETP Trade Book)

| Fact | Value | Source | Reach |
|---|---|---|---|
| What it is | 122 fields, structurally identical to ST5002, for ETF / ETN / ELW: millisecond trades + 10-level snapshot + both-side order attribution. | field list; sample | PUBLIC |
| First available date | ETF **2002-10**, ETN **2014-11**, ELW **2005-12** (Tick) | `mdc.util.js` `getStrtDt1()` | PUBLIC |
| Field list | 122 fields | `getDatasaleProdItmList.cmd` | PUBLIC |
| Delivery | CSV, cloud download (size caps bypassed) | detail page + price book | PUBLIC |
| Sample | **Yes, free** — 5 rows, ETP ISIN `KR7485690002` | `getSampleList.cmd` | PUBLIC |
| Price | 선택형 **3,000,000원**/yr, 개별 500,000원; 구독형 **6,000,000원**/yr | KRX 데이터 상품 안내 2026.1 | PUBLIC |
| English product name | — | — | **NOT FOUND** |

**Status: SETTLED**

> ⚠️ **Defect in KRX's own copy — raise with the account manager.** The public
> 개요 for EP5002 reads `한국거래소 국채전문유통시장(KRX KTB)의 장중 체결 정보…`
> — that is the **bond** product's description, pasted into the ETP slot. The
> field list and the live sample (an ETP ISIN) both confirm the product is in
> fact the ETF/ETN/ELW trade book. Do not let a procurement decision rest on
> that paragraph.

---

## A2 cross-cutting commercial terms (KRX 데이터 상품 안내, 2026.1 — PUBLIC)

- **Format**: CSV throughout. Selective data becomes available the day *after*
  purchase and is retained **7 days**.
- **Delivery ladder**: web download ≤500 MB · email 500 MB–5 GB · external HDD
  >5 GB (85,000원 per extra 1 TB, purchased separately) · sFTP ≤30 MB by prior
  arrangement. **호가장/체결장 and 1-minute annual subscriptions bypass the size
  caps entirely via cloud download.**
- **Lead time**: 호가장/체결장/비정형 take ~2 weeks and **can exceed a month**.
- **Discounts**: 50% for academic / public-interest buyers, proof required.
  Listed figures are 기준가격; rounded down below 1,000원, floor 1,000원.
- **Refunds**: re-extraction or refund within D+10 trading days, max 2
  re-extractions; refunds only where KRX cannot repair a gap or error.
- **Restrictions**: KRX **refuses purchases for resale or redistribution**, may
  demand proof of the stated purpose, and may order destruction plus pursue civil
  liability for out-of-purpose use.
- **Corporate buyers** must submit additional documents.
- **Contact**: 미래사업본부 데이터사업부 — `krxdata@krx.co.kr`, 02-3774-8904.

## A2 — what is genuinely not publicly establishable

1. **Official English product names** for all six codes. `locale=en_US` leaves
   the datasale menu in Korean, and no English edition of the brochure exists at
   the obvious filenames (403). English **field** names *are* available
   (`locale=en_US` on `getDatasaleProdItmList.cmd`, e.g. `ORD_ACPT_NO` →
   `OrderReceiptID`, `ASK_TRST_PRINC_TP_CD` → `AskOrderCapacityCode`). Marked
   NOT FOUND above; the Korean names are authoritative.
2. **The exact quoted price for a specific selection** (market × date range ×
   field subset). `getPredictionInfo.cmd` returns `E002` without login. The
   figures above are KRX's published **reference** table, which is what a
   procurement decision needs.

Neither gap changes any product's status. **All six codes: SETTLED.**

---

# A3 — Koscom "KRX Securities A/B/C" (증권A/B/C) real-time feed specification

## Why this section matters (stated up front)

This is a **pass/fail acquisition gate, not a tuning parameter.** An event stream
is silent when nothing happens. If a feed carries neither a message-header
sequence number nor a heartbeat, then time-since-last-event cannot distinguish a
quiet market from a dead feed, and **liveness detection is impossible at any
threshold**. A conclusive negative would have been as decisive as a positive.

**The finding is positive, and it is conclusive.** Both mechanisms are present,
both are documented, and the specification is **publicly downloadable without a
login or a contract**.

## Product identification (prerequisite)

`증권A/B/C` are confirmed as the KRX real-time distribution information products
("정보상품"), and they map as follows:

| Product | Contents | Source | Class |
|---|---|---|---|
| 증권A (Securities A) | 유가증권시장 주식 — KOSPI stocks | [KRX brochure PDF](https://data.krx.co.kr/inc/datasale/Market%20Data%20Product%20Brochure.pdf?v=20250732) p.2; [Koscom MDCS realtime product page](https://data.koscom.co.kr/product/realtime-product) | `[PRIMARY]` PUBLIC |
| 증권B (Securities B) | 코스닥시장 주식, 코넥스시장 주식 — KOSDAQ + KONEX stocks | same | `[PRIMARY]` PUBLIC |
| 증권C (Securities C) | ETF, ETN, ELW | same | `[PRIMARY]` PUBLIC |

Depth carried by these feeds, from the KRX brochure: "(주식시장) 매수·매도 우선호가
**10단계**" — 10 price levels per side for the stock market. This is corroborated
field-by-field in the interface spec (see below): `IFMSRPD0002` carries
매도/매수 1단계…10단계 우선호가가격 + 우선호가잔량 (590 bytes), and `IFMSRPD0003`
(증권C) carries the same 10 levels plus per-level LP quantities (830 bytes).

Transport, from the same brochure and the Koscom product page:
- **전용선 / STOCK-NET** — UDP **multicast**, one channel per information product.
- **인터넷망** — TCP/IP direct datafeed over the public internet.

### Bandwidth tier changes what you actually receive (read before pricing anything)

From `접속표준서 가이드_v0.01.xlsm` (https://data.koscom.co.kr/bbs/02/100557/bbs-contents-detail),
sheet `접속표준서 목록`, section `□전용회선 고속 UDP` / `□인터넷` — `[PRIMARY]` PUBLIC:

| Tier | 증권A / 증권B / 증권C behaviour (verbatim) |
|---|---|
| **100M** (증권C: 200M) dedicated line | `KRX 발생 우선호가, 체결 전문 모두 제공` — **all** quote and trade messages delivered; `12M 대비 추가 전문 제공` |
| **12M** (증권C: 45M) dedicated line | `KRX 발생 우선호가 상시 필터링` — **the order-book (우선호가) messages are permanently filtered** |
| **Internet (TCP)** | `인터넷분배(AA열), 증권A/12M(K열) ● 필터링` + `KRX 발생 우선호가 상시 필터링` — the internet feed inherits the **12M filtering profile** |

Consequence: **only the 100M/200M dedicated multicast line carries the full,
unfiltered 10-level book.** The 12M/45M lines and the public-internet TCP feed
permanently filter 우선호가. This also explains the `(※ 대용량 서비스에서 제공)`
("provided in the high-capacity service") caveat attached to the per-symbol
sequence-number rule in A3.1 — 대용량 is the 100M tier.

## The specification document (question 4 — answer first, it grounds 1–3)

| Fact | Value |
|---|---|
| Document | `접속표준서(정보분배-UDP_TCP실시간)_v2.019-배포용.xlsx` ("Connection Standard (Information Distribution — UDP/TCP real-time)", v2.019, marked 배포용 = *for distribution*) |
| Publisher | Koscom MDCS — KRX Market Data IT Support |
| Size / shape | 694 KB XLSX; sheets: 표지, 변경이력 (250 rows of change history), 인터페이스목록 (~120 interfaces), 인터페이스정의서 (~3,370 field rows), 별첨-정보구분코드, 참고-가격표시정보 |
| Board | `data.koscom.co.kr` → 접속표준서 (SPEC Guide) → KRX 시장정보상품 → 실시간 정보 → 시장정보 (`bbsCode` 05, 46 items) |
| Landing URL | https://data.koscom.co.kr/bbs/05/110655/bbs-contents-detail |
| Status | **`[PRIMARY]` PUBLIC — downloadable with no login and no contract** |

Companion documents on the same site, also public:

| Document | Board | ID | URL |
|---|---|---|---|
| `접속표준서(UDP) 공통정보(송신채널_정보구분)_v1.26.xlsx` — per-product UDP multicast group/port map | `bbsCode` 02 | 110590 | https://data.koscom.co.kr/bbs/02/110590/bbs-contents-detail |
| `접속표준서(TCP) 공통정보(송신채널_정보구분)_v1.07.xlsx` — TCP internet feed guide + channel map | `bbsCode` 02 | 110534 | https://data.koscom.co.kr/bbs/02/110534/bbs-contents-detail |
| `접속표준서 가이드_v0.01.xlsm` — reading guide for the above | `bbsCode` 02 | 100557 | https://data.koscom.co.kr/bbs/02/100557/bbs-contents-detail |
| `코드값 모음집` — code-value tables (149 items) | `bbsCode` 04 | various | https://data.koscom.co.kr/bbs/04/bbs-contents-list |

### Access note — important, and slightly counter-intuitive

The Koscom MDCS site is a Vue SPA whose **client-side router** marks the board
*list* routes `authorized: true` (login) while marking the board *detail* routes
`authorized: false` (no login). The **server-side REST API enforces neither** for
these boards. Both the list search and the attachment download return 200
anonymously:

- List: `POST https://data.koscom.co.kr/apis/v1/user/bbss/bbsContents/search`
  body `{"bbsCode":"05","sortType":"CREATE_DATE","page":{"pageNumber":0,"pageSize":50}}`
- Detail: `POST https://data.koscom.co.kr/apis/v1/user/bbss/bbsContents/{bbsContentId}` (same body)
- File: `GET  https://data.koscom.co.kr/apis/v1/common/files?fileUUID={urlencoded fileUUID}`

So the answer to "is the interface spec gated?" is **no** — contrary to the
common assumption (and contrary to what several secondary write-ups claim) that
it is contract-only. The *data licence* is contract-only; the *protocol spec* is
not. All facts in sections 1–3 below were read directly out of these files.

---

## A3.1 — Do the message headers carry a sequence number?

| | |
|---|---|
| **Fact** | **YES.** Field #3 of every real-time market-data message is `정보분배일련번호` ("information-distribution serial number"), `Int`, length **8**, at cumulative offset 5–13, immediately after `데이터구분값`(2) + `정보구분값`(3). |
| **Spec definition (verbatim)** | `정보분배에서 부여하는 일련번호` / `시세 : 종목별 보드별 부여 (※ 대용량 서비스에서 제공)` / `종목정보 : 정보구분값별 부여` / `기타 : 데이터구분값별 부여` |
| **Verified on** | `IFMSRPD0002` 증권 우선호가 (MM/LP호가 제외) — 증권A/B, 590 B; `IFMSRPD0004` 증권 체결 — 증권A/B/C, 186 B; `IFMSRPD0003` 증권 우선호가 (MM/LP호가 포함) — 증권C, 830 B |
| **Carried on which lines** | Marked ● for 정보이용사, 증권A 100M, 증권A 12M, 증권B 100M, 증권B 12M, 증권C 200M, 증권C 45M, **and 인터넷분배 (TCP)** |
| **Source** | `접속표준서(정보분배-UDP_TCP실시간)_v2.019-배포용.xlsx`, sheet `인터페이스정의서`, rows for IFMSRPD0002/0003/0004 — https://data.koscom.co.kr/bbs/05/110655/bbs-contents-detail |
| **Class / Status** | `[PRIMARY]` **PUBLIC — SETTLED** |

### Load-bearing caveat on the sequence number

The numbering scope changed and is **not** a channel-level counter. From the same
file's `변경이력` sheet:

> `* 시세 송신 일련번호 제공 변경` / `- 변경전 : 송신 port별 일련번호 제공` / `- 변경후 : 종목별 보드별 일련번호 제공`

i.e. it was **formerly per-send-port**, and is **now per-symbol, per-board**. For
downstream design this means:

- A **per-instrument gap** is detectable (the counter for that symbol+board skips).
- A **whole-channel outage is NOT detectable from the sequence number alone**,
  because a silent channel emits no sequence at all. Gap detection needs the
  next message to arrive before it can fire.
- Therefore the sequence number **does not by itself close the liveness gate**.
  The heartbeat (A3.2) is what closes it. Both are needed; both exist.

Also note the parenthetical `(※ 대용량 서비스에서 제공)` — "provided in the
high-capacity service" — attached to the 시세 (quote/trade) numbering rule. The
per-line ● marks nonetheless show the field present on the reduced-bandwidth
lines (증권A 12M, 증권B 12M, 증권C 45M) too; the caveat appears to scope the
*per-symbol-per-board granularity*, not the field's presence. **Worth one vendor
confirmation** if the 12M/45M lines are the intended purchase.

## A3.2 — Does a heartbeat message type exist?

| | |
|---|---|
| **Fact** | **YES — one on each transport, and they are different mechanisms.** |
| **(a) UDP multicast (전용선 / STOCK-NET)** | Interface `IFMSRPD0001`, name **`Polling Data`**, TR-CODE **`I2000`**, total length **10 bytes**. Layout: `데이터구분값`(2) + `정보구분값`(3, literal `"000"`) + `1분단위시각`(4) + `정보분배메세지종료키워드`(1, `0xFF`). |
| Coverage | Marked ● on **every** dedicated-line product column: 증권A 100M/12M, 증권B 100M/12M, 증권C 200M/45M, 채권A, 파생A, 주식파생, 일반A. Change log confirms: `* Polling 삽입 : 전체 채널에 I2000`. The `송신채널정보` sheet lists `Polling` / `I2000` on each individual multicast group+port row (e.g. 증권A 유가증권주식100M, group `233.38.231.14`, ports 10045–10049). |
| Not on | The `인터넷분배` (TCP) column is `0` for IFMSRPD0001, and `I2000`/`Polling` appear **nowhere** in the TCP channel document. The UDP Polling message is multicast-only. |
| **(b) TCP internet feed** | Control packet **`Link`**, described verbatim as `세션 유지 패킷` ("session-keepalive packet"). Layout: `LK`(2) + `000`(3) + `Filler`(6) + `0xFF`(1) = 12 bytes. It is one of four documented control packets: `Link`, `복구요청 전문` (recovery request), `Header`, `Footer`. |
| **Source** | (a) `접속표준서(정보분배-UDP_TCP실시간)_v2.019`, sheets `인터페이스목록` r3 + `인터페이스정의서` rows 3–6 — https://data.koscom.co.kr/bbs/05/110655/bbs-contents-detail ; and `접속표준서(UDP) 공통정보(송신채널_정보구분)_v1.26`, sheet `전용선(UDP) 시세상품별 회선별 송신채널정보` — https://data.koscom.co.kr/bbs/02/110590/bbs-contents-detail . (b) `접속표준서(TCP) 공통정보(송신채널_정보구분)_v1.07`, sheet `실시간인터넷 개발가이드`, section `■ 컨트롤 패킷` — https://data.koscom.co.kr/bbs/02/110534/bbs-contents-detail |
| **Class / Status** | `[PRIMARY]` **PUBLIC — SETTLED** |

## A3.3 — Documented heartbeat interval / cadence

| | |
|---|---|
| **TCP feed — EXPLICITLY DOCUMENTED** | `- Link : 정상 송수신 시 사용. 세션 유지 패킷. **데이터가 1분 동안 미 발생 시** 링크 유지를 위한 전송 패킷` — "a packet sent to keep the link alive when no data has occurred for 1 minute." The send-flow diagram repeats it: `1분 이내 Data가 없을 경우 Link 송신(반복)` — if there is no data within 1 minute, send Link, **repeating**. |
| **TCP feed — the spec also assigns the detection duty** | `- 1분 이내 데이터 또는 LINK가 수신되지 않는 경우 장애 처리는 **수신자**가 한다` — "if neither data nor LINK is received within 1 minute, fault handling is **the receiver's** responsibility." This is the exchange side explicitly specifying a **1-minute liveness threshold** and placing it on the consumer. It is precisely the guarantee the acquisition gate was asking for. |
| Related TCP semantics (relevant to failure design) | Transmission is **one-way with no application-level ACK** (`데이터 송신시 코스콤과 수신사 간 어플리케이션 ack를 주고 받지 않는 방식`). On disconnect there is **no automatic retransmission** of missed data (`접속이 끊긴 경우 미수신 데이터에 대한 재전송은 없다`) — recovery is an explicit pull via the 복구요청 flow, available `06:00~21:00` on business days only, and for 시세 data it returns **only the latest snapshot per symbol**, not the missed stream. |
| **UDP multicast — NOT explicitly stated in seconds** | The spec gives `제공 주기: 24-365` (i.e. continuously, year-round) for `Polling Data`, and the message's only payload field is `1분단위시각` — a **4-character minute-resolution clock (HHMM)**. A minute-granularity timestamp as the sole content strongly implies a ≥1/minute cadence consistent with the TCP side, but **the UDP document does not state an interval numerically.** |
| **Source** | https://data.koscom.co.kr/bbs/02/110534/bbs-contents-detail (TCP, explicit) ; https://data.koscom.co.kr/bbs/05/110655/bbs-contents-detail (UDP, field definition) |
| **Class / Status** | `[PRIMARY]` **PUBLIC — SETTLED for TCP (1 minute, explicit). PARTIALLY SETTLED for UDP** (mechanism and universal coverage confirmed; numeric interval inferred from the `1분단위시각` field, not stated). One vendor question closes it. |

## A3.4 — Published interface specification document

Answered in full above. Summary: **four** relevant specification workbooks are
published by Koscom MDCS and are **downloadable anonymously**. The authoritative
one is `접속표준서(정보분배-UDP_TCP실시간)_v2.019-배포용.xlsx`. `[PRIMARY]` PUBLIC — **SETTLED**.

No secondary source was needed. For the record, a search for third-party client
libraries implementing this wire protocol found **none** — the public Korean-market
GitHub projects (pykrx and similar) are REST/scraping clients, not multicast
decoders. That absence is *not* evidence against the protocol facts above, which
rest on the vendor's own published spec.

---

## **Can the acquisition gate (sequence number AND heartbeat) be settled from public sources? — YES-PASS**

Both required mechanisms are documented in Koscom's own publicly downloadable
connection standard:

1. **Sequence number — present.** `정보분배일련번호`, Int(8), field #3 of every
   증권A/B/C real-time message, on both UDP and TCP transports.
2. **Heartbeat — present on both transports.** UDP: `Polling Data` / `I2000`,
   10 bytes, on every multicast channel without exception. TCP: `Link` keepalive,
   12 bytes.
3. **Cadence — explicitly 1 minute on TCP**, with the spec itself stating that a
   receiver seeing neither data nor LINK within 1 minute must treat it as a fault.
   UDP cadence is not numerically stated but the heartbeat's presence is certain.

Liveness detection is therefore fully supportable. The two residual questions are
**refinements, not gate items**, and both are single vendor emails:

- (i) the numeric UDP `Polling`/`I2000` emission interval;
- (ii) whether the `(※ 대용량 서비스에서 제공)` caveat degrades per-symbol-per-board
  sequence granularity on the reduced-bandwidth lines (증권A 12M / 증권B 12M / 증권C 45M).

Contact of record, from the KRX brochure: `marketdata@koscom.co.kr` (product/contract),
`idist@koscom.co.kr` (internet-distribution technical).

---


<!-- ================= a4-a5-a6-findings.md ================= -->

# A4 / A5 / A6 — KRX market-structure research findings

Research date: 2026-08-03. Read-only. No repository file was modified.

**Portal-name correction up front.** The task names `open.krx.co.kr` as the OPEN API host.
That host exists but is KRX's *legacy* market portal (it still serves e.g. the
정리매매종목 screen at `open.krx.co.kr/contents/MKD/04/0403/04030200/MKD04030200.jsp`).
The OPEN API described in A4 lives at **`openapi.krx.co.kr`** (the portal / spec pages)
and serves from **`data-dbg.krx.co.kr`** (the API host). All A4 findings below are from
`openapi.krx.co.kr` and `data-dbg.krx.co.kr`.

---

# A4 — KRX OPEN API basic-information service fields

## Method note (read before the findings)

Every A4 fact below is sourced from KRX's **own** material, in two forms:

1. **The official downloadable specification.** Each service page carries a
   `개발 명세서 다운로드` (development specification download) button which POSTs to
   `https://openapi.krx.co.kr/contents/OPP/USES/service/downloadApiDoc.cmd` and returns
   `Spec.docx`. That document is KRX's published field list. Retrieved for
   `stk_isu_base_info` and quoted verbatim below. `[PRIMARY]`
2. **The service page's own embedded transaction definition.** Each page at
   `openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=<id>` embeds a
   base64 variable `var bld = '...'` which decodes to the full BLD transaction XML —
   including the backing SQL and the `<output>` field block. This is KRX-served content,
   so `[PRIMARY]` as to *what the service does*, but it is **implementation detail, not a
   published spec statement**. It is used below only to answer questions the published
   spec is silent on (point-in-time semantics, scope filters), and is labelled as such
   each time.

Service page IDs (`BO_ID`) discovered:

| Service | API ID | Service page |
|---|---|---|
| 유가증권 종목기본정보 (KOSPI) | `stk_isu_base_info` | `.../OPPUSES002_S2.cmd?BO_ID=PiwgMdTwmsenXhmqqxuj` |
| 코스닥 종목기본정보 (KOSDAQ) | `ksq_isu_base_info` | `.../OPPUSES002_S2.cmd?BO_ID=CifLHplnUFMgpHIMMPXs` |
| 코넥스 종목기본정보 (KONEX) | `knx_isu_base_info` | `.../OPPUSES002_S2.cmd?BO_ID=COgTLqgmGlqyJvaEFNIc` |

Production endpoint (from `Spec.docx`): `https://data-dbg.krx.co.kr/svc/apis/sto/stk_isu_base_info`
Sample endpoint (from the service page): `https://data-dbg.krx.co.kr/svc/sample/apis/sto/stk_isu_base_info`
— **these are different paths and behave differently. See A4.5.**

---

## A4.1 — Complete returned field list for each service

**Fact.** Both services return an identical 12-field `OutBlock_1`. The KOSPI, KOSDAQ and
KONEX base-info services are field-for-field the same; only the market filter differs.
Request block `InBlock_1` has exactly one field: `basDd` (기준일자), `string`, size 8.

| # | API name | 항목명 (Korean label) | Type |
|---|---|---|---|
| 1 | `ISU_CD` | 표준코드 (ISIN / standard code) | string |
| 2 | `ISU_SRT_CD` | 단축코드 (short code, 6-digit) | string |
| 3 | `ISU_NM` | 한글 종목명 (full Korean name) | string |
| 4 | `ISU_ABBRV` | 한글 종목약명 (abbreviated Korean name) | string |
| 5 | `ISU_ENG_NM` | 영문 종목명 (English name) | string |
| 6 | `LIST_DD` | 상장일 (listing date) | string |
| 7 | `MKT_TP_NM` | 시장구분 (market: `KOSPI` / `KOSDAQ` / `KONEX`) | string |
| 8 | `SECUGRP_NM` | **증권구분** (security group) | string |
| 9 | `SECT_TP_NM` | 소속부 (market segment / section; default `-`) | string |
| 10 | `KIND_STKCERT_TP_NM` | 주식종류 (share class — 보통주 / 우선주) | string |
| 11 | `PARVAL` | 액면가 (par value; literal `무액면` for no-par) | string |
| 12 | `LIST_SHRS` | 상장주식수 (listed shares) | string |

Verbatim from the official spec (`Spec.docx`, §1.4.1 OutBlock_1) for `stk_isu_base_info`:
`ISU_CD 표준코드 / ISU_SRT_CD 단축코드 / ISU_NM 한글 종목명 / ISU_ABBRV 한글 종목약명 /
ISU_ENG_NM 영문 종목명 / LIST_DD 상장일 / MKT_TP_NM 시장구분 / SECUGRP_NM 증권구분 /
SECT_TP_NM 소속부 / KIND_STKCERT_TP_NM 주식종류 / PARVAL 액면가 / LIST_SHRS 상장주식수`.

Confirmed empirically — a live sample call returned exactly these 12 keys per row, e.g.
`{"ISU_CD":"KR7338100001","ISU_SRT_CD":"338100","ISU_NM":"NH프라임리츠보통주",
"ISU_ABBRV":"NH프라임리츠","ISU_ENG_NM":"NH Prime REIT","LIST_DD":"20191205",
"MKT_TP_NM":"KOSPI","SECUGRP_NM":"부동산투자회사","SECT_TP_NM":"",
"KIND_STKCERT_TP_NM":"보통주","PARVAL":"500","LIST_SHRS":"18660000"}`.

**Source.**
- `Spec.docx` — "API Spec / 1.1. 유가증권 종목기본정보", downloaded from
  https://openapi.krx.co.kr/contents/OPP/USES/service/downloadApiDoc.cmd (POST with
  `BO_ID=PiwgMdTwmsenXhmqqxuj`, reachable from the 유가증권 종목기본정보 service page).
  `[PRIMARY]`
- Service pages (embedded `<output>` block, all three markets):
  https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=PiwgMdTwmsenXhmqqxuj
  (유가증권), `...BO_ID=CifLHplnUFMgpHIMMPXs` (코스닥), `...BO_ID=COgTLqgmGlqyJvaEFNIc` (코넥스)
  — "KRX | Data Marketplace OPEN API — 주식". `[PRIMARY]`
- Live sample response: `GET https://data-dbg.krx.co.kr/svc/sample/apis/sto/stk_isu_base_info?basDd=20200414`
  with the sample `AUTH_KEY` published on the service page. `[PRIMARY]`

**Status.** `SETTLED`

**Note.** There is no code/name pairing anywhere in the payload: `MKT_TP_NM`,
`SECUGRP_NM`, `SECT_TP_NM` and `KIND_STKCERT_TP_NM` are all delivered as **Korean display
names only** — the underlying KRX code values (`SECT_TP_CD`, `KIND_STKCERT_TP_CD`,
`SECUGRP_ID`) exist in the backing query but are decoded to names before output. Anything
downstream must key on Korean strings, not codes. Also note `ISU_SRT_CD` is the KRX
short code with its leading market prefix character stripped (`SUBSTR(ISU_SRT_CD,2)`),
i.e. the familiar 6-digit ticker. Spec version at time of research: page 최근 수정일
2026/01/16, BO_VER 1.0.

---

## A4.2 — Is 증권구분 (security group) among the returned fields?

**Fact.** **Yes.** The field is **`SECUGRP_NM`**, labelled **증권구분** in KRX's own spec.
It is a Korean **name** string, not a code.

Scope: the backing query restricts to seven security groups —
`B.SECUGRP_ID IN ('ST','MF','RT','SC','IF','DR','FS')`. Values observed live in
`SECUGRP_NM`: `투자회사` (MF, mutual fund / investment company), `부동산투자회사`
(RT, REIT), `사회간접자본투융자회사` (IF, infrastructure fund), `주식예탁증권` (DR),
`외국주권` (FS, foreign share). `주권` (ST, ordinary share certificate) and
`선박투자회사` (SC, ship investment company) are the two remaining groups in the filter
that the 10-row sample did not surface.

**Important scope caveat on what 증권구분 does and does not separate:**
- `SECUGRP_NM` separates **주권 vs REIT vs 투자회사 vs 선박/인프라 fund vs DR vs foreign share**.
- It does **not** separate **common vs preferred**. That distinction is carried by a
  *different* field, `KIND_STKCERT_TP_NM` (주식종류), observed values `보통주` / `우선주`.
- It does **not** flag **SPAC**. A SPAC is `SECUGRP_ID = 'ST'` (주권) like any other
  ordinary share; no field in this payload identifies a SPAC. I could not source any
  SPAC marker in these 12 fields — treat SPAC identification as **not available** from
  this service.

**Source.**
- `Spec.docx` §1.4.1 (`SECUGRP_NM` / `증권구분`), via
  https://openapi.krx.co.kr/contents/OPP/USES/service/downloadApiDoc.cmd `[PRIMARY]`
- Embedded BLD `<output>` block on
  https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=PiwgMdTwmsenXhmqqxuj
  `[PRIMARY]` (the `SECUGRP_ID IN (...)` filter is implementation detail, not a spec statement)
- Live values: `https://data-dbg.krx.co.kr/svc/sample/apis/sto/stk_isu_base_info?basDd=20150602`
  and `?basDd=20240102` `[PRIMARY]`

**Status.** `SETTLED` (that the field exists and its exact name). The **complete
enumeration of `SECUGRP_NM` values** is `PARTIALLY SETTLED` — five of the seven groups
were observed directly; the other two (`주권`, `선박투자회사`) are inferred from the
`SECUGRP_ID` filter in the served query, not observed.

---

## A4.3 — Is 업종 (industry / sector classification) among the returned fields?

**Fact.** **No.** There is no industry/sector field in either service. The 12-field
output contains no 업종 code, no 업종 name, and no classification-scheme identifier of
any kind (no KRX 업종분류, no KSIC, no FICS, no GICS).

The nearest-looking field, **`SECT_TP_NM` (소속부)**, is *not* an industry classification.
소속부 is the market **section/segment** an issue is assigned to (KOSPI's 소속부, KOSDAQ's
우량기업부 / 벤처기업부 / 중견기업부 / 기술성장기업부 family). It is derived from
`SECT_TP_CD`, a segment code, and it defaults to `-`; in the live sample it came back as
an empty string for every fund/REIT/DR/foreign row.

Broader check: the string 업종 does not occur **anywhere** in the KRX OPEN API service
catalogue. The catalogue lists ~40 services across 지수 / 주식 / 증권상품 / 채권 /
파생상품 / 일반상품 / ESG, and none is an industry-classification service. So 업종 is
absent not merely from this service but from the whole OPEN API surface.

**Source.**
- `Spec.docx` §1.4.1 (12 fields, none an industry field) `[PRIMARY]`
- https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd — "서비스 목록 —
  KRX | Data Marketplace OPEN API" (full catalogue; zero occurrences of 업종) `[PRIMARY]`

**Status.** `SETTLED`

**Note.** KRX *does* publish industry classification, but through a different channel —
the 업종분류 screens on `data.krx.co.kr` (KRX 정보데이터시스템), not through the OPEN API.
If 업종 is needed, it is a separate acquisition problem and cannot be satisfied by
`stk_isu_base_info` / `ksq_isu_base_info`.

---

## A4.4 — Does `basDd` deliver point-in-time state, or is it just a filter?

**Fact.** The published specification says only `basDd / string / 기준일자` — it makes
**no statement at all** about point-in-time semantics. However, two independent
KRX-sourced lines of evidence show it is genuine as-of-date state, not a filter over
current membership:

1. **Structural (KRX-served query).** The backing SQL selects from an effective-dated
   issue master with `WHERE :basDd BETWEEN B.STRT_DD AND B.END_DD` (and the same
   `BETWEEN` predicate against the security-group table). That is a temporal-validity
   join: the row returned is the one *in force on `basDd`*, not the current row.
2. **Empirical.** Varying `basDd` changes the result set exactly as as-of-date semantics
   predicts. `KB스타리츠` (`432320`, 상장일 20221006) is absent at `basDd=20150602` and
   present at `basDd=20240102`; `맥쿼리인프라` (`088980`, 상장일 20060315) is present at
   20150602. Conversely, issues that have since delisted are returned for historic dates
   — see A4.5.

Additional documented `basDd` constraints from the same served query (implementation
detail, again not a spec statement): `basDd < TO_CHAR(SYSDATE,'YYYYMMDD')` (**today is
never available**); `basDd >= '20100104'` for KOSPI/KOSDAQ (matching the published
"'10년01월04일 데이터부터 제공"; KONEX is `'13년07월01일` per its description); and the
most recent market-open day only becomes available from 08:00 the following morning
(`SFMC_PRE_RECENT_MKTOPN_DD(...)` + `TO_CHAR(SYSDATE,'HH24') >= '08'`).

**Source.**
- `Spec.docx` §1.3.1 InBlock_1 (`basDd / string / 기준일자` — and nothing more) `[PRIMARY]`
- Embedded BLD transaction XML (SQL with `:basDd BETWEEN STRT_DD AND END_DD`) on
  https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=PiwgMdTwmsenXhmqqxuj
  `[PRIMARY]`
- Live differential calls, `basDd` ∈ {20150602, 20200414, 20240102} against
  `https://data-dbg.krx.co.kr/svc/sample/apis/sto/stk_isu_base_info` `[PRIMARY]`
- Coverage-start dates: https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd `[PRIMARY]`

**Status.** `PARTIALLY SETTLED`

**Note on why not SETTLED.** The *behaviour* is point-in-time and that is now
demonstrated, not assumed. What is missing is a **KRX statement** that it is point-in-time
— the published spec is silent, so there is no documented contract, only an observed and
structurally-supported implementation. A future KRX change could alter it without
breaking any published promise.

**A trap worth naming.** The service page hardcodes `20200414` as the default `basDd` in
its sample form. A prior investigation that observed "a 2020-04-14 state" was very likely
just receiving that default. Observing 2020-04-14 data is therefore **not by itself**
evidence of point-in-time behaviour — it is evidence of the sample default. The evidence
that *does* count is the differential across explicitly-varied `basDd` values above.

---

## A4.5 — Documented statements about completeness, and delisted issues

**Fact — completeness: KRX publishes NO completeness statement.** The service page
description is exactly `유가증권 종목기본정보 ('10년01월04일 데이터부터 제공)`, and
`Spec.docx` §1.2 repeats the same one line. Neither says "all listed issues", neither
gives a row count, neither mentions pagination or truncation. There is no documented
completeness guarantee to cite.

**Fact — a known scope restriction does exist (undocumented).** The served query filters
`SECUGRP_ID IN ('ST','MF','RT','SC','IF','DR','FS')` and `MKT_ID = 'STK'` (resp. `'KSQ'`,
`'KNX'`). So even at best the service covers only those seven security groups on one
market. ETFs, ETNs, ELWs, 신주인수권증권/증서 and 수익증권 are **out of scope** by
construction — they have their own separate OPEN API services. This restriction appears
nowhere in the published spec.

**Fact — delisted issues ARE returned for dates when they were listed.** Directly
demonstrated: at `basDd=20150602` the KOSPI service returned `중국원양자원` (`900050`,
외국주권), `평산차업 KDR` (`950010`, 주식예탁증권), `하나니켈1호` (`099340`),
`하나니켈2호` (`099350`) and `베트남개발1` (`096300`) — none of which appear at
`basDd=20240102`. These are issues that were listed in 2015 and have since been delisted.
So historical delisted issues are **included**, at least in principle.

**Fact — the sample endpoint caps at 10 rows.** Every call to
`https://data-dbg.krx.co.kr/svc/sample/apis/sto/...` returned exactly 10 rows regardless
of `basDd`, and the rows were not the alphabetically-first 10 despite the query's
`ORDER BY ISU_NM` — i.e. it is a curated/limited sample response, not a truncation of the
real result. This is a **different URL path** from the production endpoint
(`/svc/apis/...` vs `/svc/sample/apis/...`) documented in `Spec.docx`.

> ### ⚠️ Explicit limitation, as requested
>
> **This group establishes WHICH FIELDS EXIST. It can NEVER establish COMPLETENESS.**
>
> The prior investigation's ten-row observation is now explained: that is the fixed
> behaviour of the **sample** endpoint (`/svc/sample/apis/...`) with the sample
> `AUTH_KEY`, which returns 10 rows for every `basDd`. It says nothing whatsoever about
> what the production endpoint (`/svc/apis/...`, requiring an approved per-service
> application and a personal `AUTH_KEY`) returns.
>
> Nothing above is evidence of *delivered completeness*. KRX makes no completeness
> statement, so there is no spec sentence that could be mistaken for one — and the
> structural facts (an effective-dated master, no visible row limit in the query) are
> **implementation shape, not delivered data**. Verifying completeness requires
> provisioning a real `AUTH_KEY`, calling the production endpoint for a known date, and
> reconciling the returned row count against an independent listed-issue census for that
> same date. Until that is done, completeness is **UNESTABLISHED**.
>
> The same applies to delisted-issue coverage: inclusion is *demonstrated*, exhaustive
> coverage of delisted issues is *not*.

**Source.**
- `Spec.docx` §1.2 Description + "Server endpoint url" (production path) `[PRIMARY]`
- https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd (service descriptions,
  no completeness language) `[PRIMARY]`
- Service page 샘플 URL block declaring `https://data-dbg.krx.co.kr/svc/sample/apis/sto/stk_isu_base_info`
  and the shared 샘플 인증키 `[PRIMARY]`
- Live differential sample calls at `basDd` ∈ {20150602, 20240102} `[PRIMARY]`

**Status.** `PARTIALLY SETTLED` — completeness `NEEDS VENDOR` (here "vendor" = a real KRX
OPEN API key and a production call, plus an independent census to reconcile against);
delisted-issue inclusion `SETTLED` as *inclusion*, `PARTIALLY SETTLED` as *coverage*.

---

## A4 overall status

`PARTIALLY SETTLED` — the field list is fully settled from KRX's own specification;
point-in-time behaviour is demonstrated but undocumented; completeness is not
establishable from public sources and remains open.

**증권구분 present: YES** — field name `SECUGRP_NM` (Korean display name, not a code;
separates 주권 / 투자회사 / 부동산투자회사 / 선박투자회사 / 사회간접자본투융자회사 /
주식예탁증권 / 외국주권 — but **not** common-vs-preferred, which is `KIND_STKCERT_TP_NM`,
and **not** SPAC, which no field identifies).

**업종 present: NO** — no industry/sector field in either service, and no 업종 service
anywhere in the KRX OPEN API catalogue. `SECT_TP_NM` (소속부) is a market *segment*, not
an industry classification.

---

# A5 — 정리매매 (liquidation / delisting trading) mechanism

## A5.1 — Duration of the 정리매매 period

**Fact.** **7 trading days.** KRX (KOSPI page): "거래소는 상장폐지가 확정된 종목을
소유하고 있는 주주에게 마지막 환금의 기회를 부여하기 위하여 **7일간(매매거래일 기준)**
매매거래를 허용할 수 있는데 이를 정리매매종목이라 합니다."

KRX (KOSDAQ page): "상장폐지가 확정된 종목의 소유주주에게 마지막 환금의 기회를 부여하기
위해 **7매매거래일 이내에서** 매매거래를 허용."

The rulebook basis is the **listing** rules, not the business rules: 「유가증권시장
상장규정」 제9조 and 「코스닥시장 상장규정」 제23조 provide for trading to be permitted
for a period not exceeding 7 trading days on delisting.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010306/RGL03010306.jsp — "Regulation |
  매매거래제도 | 유가증권시장 | 매매계약체결의 특례 | 정리매매종목의 매매방법" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020308/RGL03020308.jsp — "Regulation |
  매매거래제도 | 코스닥시장 | 매매계약체결의 특례 | 정리매매 종목의 매매방법" `[PRIMARY]`
- 「유가증권시장 상장규정」 제9조 / 「코스닥시장 상장규정」 제23조, cited via
  https://easylaw.go.kr/CSP/CnpClsMain.laf?csmSeq=1701&ccfNo=1&cciNo=2&cnpClsNo=2 —
  법제처 찾기쉬운 생활법령정보, "관리종목 지정 및 상장폐지" `[PRIMARY]` (a Korean
  government legal-information service quoting the KRX listing rules; the KRX rulebook
  full text itself is behind the frame-based `law.krx.co.kr` viewer)

**Status.** `SETTLED`

**Note.** KOSDAQ's wording is "**이내에서**" (*within* 7 trading days) and KOSPI's is
"허용할 **수 있는데**" (*may* permit) — so 7 is a ceiling that KRX may set shorter, and a
정리매매 period may be **omitted entirely** for some delisting causes. Do not model 7 as
an invariant.

## A5.2 — Trading mechanism: 30-minute single-price call auctions?

**Fact. CONFIRMED.** KRX (KOSPI): "정리매매종목은 급격한 가격변동에 따른 투자위험성을
최소화하기 위하여 **30분단위의 단일가 매매방법**으로 매매체결 (**1일 14회**)이
이루어지며…"

KRX (KOSDAQ): "…정규시장 중 **30분씩 경과한 시점(총 14회)**마다 체결시키며 호가유형 중
**지정가호가만 제출 가능**."

So: **30-minute interval, 14 auctions per regular-session day**, replacing continuous
(접속) trading. The 14 count is consistent with the regular session 09:00–15:30 and a
call at each 30-minute mark from 09:00 through 15:30 inclusive. Only **limit orders**
(지정가호가) may be submitted — market orders are not accepted.

Off-hours behaviour for a 정리매매 issue (KOSDAQ page, explicit): 시간외종가매매 is the
**same as for a normal issue**; 시간외단일가매매 runs **with no price limit**.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010306/RGL03010306.jsp `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020308/RGL03020308.jsp `[PRIMARY]`
- Regular-session hours 09:00–15:30: https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T1.jsp
  — "매매거래시간 및 호가접수시간" `[PRIMARY]`

**Status.** `SETTLED`

**Note.** The 30-minute call interval during 시간외단일가 for a 정리매매 issue is **not**
stated on the KRX pages (KRX states only that the price limit is absent there); the normal
시간외단일가 cycle is 10 minutes / 12 auctions. Some secondary write-ups claim the cycle
also lengthens to 30 minutes for 정리매매 issues; I could not source that from KRX and it
is left `UNESTABLISHED`. Also note every single-price auction — 정리매매 included — ends
at a **random point within 30 seconds** of its nominal close (Random End), so the auction
timestamps are not exactly on the half-hour.

## A5.3 — Do daily price limits (±30%) apply during 정리매매? ← the load-bearing question

**Fact. NO. Price limits are SUSPENDED for 정리매매종목.** Three mutually reinforcing
KRX statements:

1. **KRX general price-limit page (KOSPI):** "우리 시장에서는 … 하루 동안 가격이 변동할
   수 있는 폭을 기준가격 대비 상하 30%로 제한하고 있습니다. **다만, 정리매매종목,
   주식워런트증권(ELW), 신주인수권증서, 신주인수권증권의 경우에는 가격제한폭이
   적용되지 않습니다.**"
2. **KRX 정리매매 page (KOSPI):** "…30분단위의 단일가 매매방법으로 매매체결(1일 14회)이
   이루어지며 **가격제한폭을 두지 않고 있습니다**."
3. **KRX 정리매매 page (KOSDAQ):** "**가격제한폭 없이** 정규시장 중 30분씩 경과한
   시점(총 14회)마다 체결…" and, separately, "시간외단일가매매는 **가격제한폭 없음**."

**Rulebook citation.** 「유가증권시장 업무규정」 **제20조(가격제한폭) 제3항**:
"제1항의 규정에도 불구하고 상장규정 제9조에 따라 일정기간 매매거래를 허용하는
종목(이하 "정리매매종목")의 경우에는 **가격을 제한하지 아니하며**, 세칙이 정하는 경우에는
제2항에도 불구하고 가격제한폭을 달리 정할 수 있다."

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T5.jsp — "Regulation |
  매매거래제도 | 유가증권시장 | 매매거래제도일반 → 가격제한폭 제도" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03010306/RGL03010306.jsp `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020308/RGL03020308.jsp `[PRIMARY]`
- 「유가증권시장 업무규정」 제20조제3항, text reproduced at
  https://lbox.kr/v2/rule/한국거래소_규정_2026128_유가증권시장_업무규정_2026129_2405_210163555
  — "유가증권시장 업무규정" `[SECONDARY]` (a legal-database reproduction; KRX's own
  authoritative full text is at `law.krx.co.kr/las/` and `rule.krx.co.kr`, both of which
  are frame/session-based and could not be dereferenced to a stable citable URL — the
  **substance** of this article is independently confirmed by the three KRX pages above,
  so the secondary source is corroboration for the article number and wording only)

**Status.** `SETTLED`

## A5.4 — KOSPI vs KOSDAQ differences

**Fact.** The mechanism is **materially identical** on both markets: ≤7 trading days,
30-minute single-price auctions, 14 per day, no price limit. Differences are presentational
plus one substantive detail KRX states only on the KOSDAQ page:

| | KOSPI (유가증권시장) | KOSDAQ (코스닥시장) |
|---|---|---|
| Period | 7일간 (매매거래일 기준) | 7매매거래일 **이내에서** |
| Regular session | 30분 단위 단일가, 1일 14회 | 30분씩 경과한 시점, 총 14회 |
| Price limit | 두지 않음 | 없음 |
| Order types | not stated on the KRX page | **지정가호가만 제출 가능** |
| 시간외종가매매 | not stated on the KRX page | 일반종목과 동일 |
| 시간외단일가매매 | not stated on the KRX page | 가격제한폭 없음 |

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010306/RGL03010306.jsp `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020308/RGL03020308.jsp `[PRIMARY]`

**Status.** `PARTIALLY SETTLED` — the equivalence of the core mechanism is settled. The
three rows where KOSPI's page is silent are almost certainly the same on both markets
(the limit-order-only restriction is a natural consequence of single-price auction with no
price limit — a market order has no reference to price against), but KRX does not say so
on the KOSPI page, so I have not asserted it.

## A5.5 — Has the mechanism changed over time?

**Fact.** No **effective-dated change to the 정리매매 mechanism itself** is published by
KRX. The KRX 정리매매 page carries exactly one dated historical note, and it is about a
*different* instrument: "과거에는 … **관리종목**의 경우에도 주기적인 단일가매매를
실시하였으나 … **2002년 7월부터는** 일반종목과 동일한 매매방법을 적용하고 있습니다."
That is 관리종목 (administrative-issue designation), **not** 정리매매 — the two must not
be conflated.

Two dated changes that *touch* 정리매매 indirectly, both sourced:

- **2015-06-15** — the general price limit widened from ±15% to ±30%. 정리매매종목 were an
  exception both before and after, so this changed nothing for 정리매매 — but it does mean
  that "the ±30% figure" is only correct for dates on or after 2015-06-15. KRX publishes
  the full history: pre-'95.4 정액제 (avg 4.6%), '95.4.1 6%, '96.11.24 8%, '98.3.2 12%,
  '98.12.7 15%, '15.6.15 30%.
- **Regular-session close moved to 15:30** (from 15:00). This directly determines the
  **number of 30-minute auctions per day**: the currently-published count is 14, which is
  arithmetically consistent with a 09:00–15:30 session. A pre-extension session would
  yield 13. So the "14 auctions" figure is **date-dependent** and must not be applied to
  historical sessions before the extension.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010306/RGL03010306.jsp (the 2002-07
  관리종목 note) `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T5.jsp ("가격제한폭
  변경 내역" table with all six effective dates) `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T1.jsp (current
  09:00–15:30 regular session) `[PRIMARY]`

**Status.** `PARTIALLY SETTLED`

**Note.** The **effective date of the regular-session extension to 15:30** is *not*
sourced here to a KRX primary page — I found it only in secondary write-ups (given as
2016-08-01). Anyone who needs to apply the 30-minute-auction schedule to pre-2016
sessions must nail that date down from a KRX notice first. Likewise, the date at which
30-minute single-price auctions were *first* introduced for 정리매매 is
**not publicly establishable** from the sources reachable here — KRX's page states the
current mechanism with no effective date. Separately, a live **policy** thread exists:
FSC/KRX's 상장폐지 제도개선 방안 proposes routing post-정리매매 trading to a new
K-OTC 상장폐지기업부 — that is *after* 정리매매 and does not alter the 정리매매 mechanism,
but it signals the area is not frozen.

---

## A5 overall status

`SETTLED` on the three load-bearing questions (7 trading days, 30-minute single-price
auctions ×14/day, no price limit); `PARTIALLY SETTLED` on historical effective-dating.

**Do price limits apply during 정리매매? NO** — 가격제한폭 is expressly not applied to
정리매매종목, per 「유가증권시장 업무규정」 제20조제3항 and three separate KRX pages
(KOSPI 가격제한폭 제도, KOSPI 정리매매, KOSDAQ 정리매매). This holds in the regular
session **and** in 시간외단일가매매.

### ⚠️ WHY IT MATTERS — stated explicitly

Downstream logic needs to know whether an open position held into 정리매매 can be marked
at "that day's lower price limit" (하한가). **It cannot.**

Because 가격제한폭 is not applied to 정리매매종목, **there is no 상한가 and no 하한가 for
a 정리매매 issue on any day of the 정리매매 period.** The quantity the rule wants to read
**does not exist** — it is not merely unavailable in some feed, it is undefined by
regulation. Any "mark at the day's lower limit" rule therefore has **no value to compute**
for a 정리매매 issue and must not silently fall back to ±30% off the base price: doing so
would fabricate a bound the market does not impose, in exactly the situation (a delisting
stock, which routinely moves tens of percent inside a single 30-minute auction) where the
error is largest. The rule needs an explicit 정리매매 branch — either refuse to mark, or
mark on an observed auction price, but not on a limit.

Two secondary consequences worth carrying:
- Fills arrive at **≤14 discrete 30-minute auction points**, not continuously — any
  intraday mark-to-market or stop logic keyed to continuous quoting is wrong here.
- **Limit orders only** on KOSDAQ (KRX states it explicitly there): any order path that
  emits a market order will be rejected.

---

# A6 — KRX trade-category / trade-condition taxonomy

## A6.0 — Session frame (the spine everything below hangs on)

**Fact.**

| Session | 호가접수시간 (order acceptance) | 매매거래시간 (trading) |
|---|---|---|
| 정규시장 (Regular Session) | 08:30–15:30 (7h) | 09:00–15:30 (6h30m) |
| 시간외시장 장개시전 (pre-hours) | 08:00–09:00 | 08:00–09:00 (장개시전 종가매매만 08:30–08:40) |
| 시간외시장 장종료후 (after-hours) | 15:30–18:00 (2h30m) | 15:40–18:00 (2h20m) |

Within the regular session: **시가 단일가매매** (opening call) collects from 08:30 and
prices at 09:00; **접속매매** (continuous auction) runs from just after the opening call
to just before the closing-call order window; **종가 단일가매매** (closing call) collects
over the last 10 minutes and prices at 15:30. Every single-price auction terminates at a
random instant within 30 seconds of its nominal close (Random End).

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T1.jsp — "매매거래시간
  및 호가접수시간" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03010201/RGL03010201.jsp — "단일가매매시
  체결방법" (Random End; 수량배분 only when 시가 prints at 상·하한가) `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03010203/RGL03010203.jsp — "접속매매시
  매매체결방법" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020205/RGL03020205.jsp — KOSDAQ
  "매매체결방법", which enumerates where 단일가 applies: 시가 결정, 종가 결정, 신규/재상장
  시초가 결정, CB 재개, 종목별 매매거래정지 후 재개 `[PRIMARY]`
- https://global.krx.co.kr/contents/GLB/01/0109/0109000000/guide_to_trading_in_the_korean_stock_market.pdf
  — KRX, "Guide to Trading in the Korean Stock Market" (English names) `[PRIMARY]`

**Status.** `PARTIALLY SETTLED`

**Note — a real discrepancy to carry.** KRX's Korean regulation pages give pre-hours as
**08:00–09:00**; KRX's own English guide PDF gives pre-hours as **07:30–09:00** (and
off-hours block/basket as `07:30~09:00 / 15:40~18:00` against the Korean pages'
`08:00~09:00 / 15:40~18:00`). Both are KRX-published. They cannot both be current. The
English PDF is the likelier-newer document, but I could not date either page, so the
pre-hours start is `UNESTABLISHED` — resolve before hard-coding. Separately, the exact
**closing-call window boundary (15:20)** is not stated on the KRX pages I could reach —
KRX states the regular session ends 15:30 and that a closing single-price auction exists;
that it opens at 15:20 comes from KRX's English VI table, which labels the Dynamic-VI
"Closing auction" band as **(15:20 – 15:30)** and the continuous-auction band as
**(09:00 – 15:20)**. That is a strong KRX-internal corroboration but is stated in a VI
context, not as a session definition.

## A6.1 — 정규시장 continuous session — 접속매매

**Fact.** Korean **접속매매** (formally 복수가격에 의한 개별경쟁매매); English **Continuous
auction** (KRX's own term in the English guide: "Regular session : Continuous auction").
Occurs from just after the opening call to the start of the closing-call order window
(≈09:00–15:20). Executes immediately on arrival of a matchable order, at the **resting
(earlier) order's price**, under price priority then time priority.

**Prints as a trade in a consolidated feed: YES.** This is the baseline print — the
overwhelming majority of equity volume.

**Source.** https://regulation.krx.co.kr/contents/RGL/03/03010203/RGL03010203.jsp
`[PRIMARY]`; KRX English guide, "Method of Trade Execution" `[PRIMARY]`

**Status.** `SETTLED`

**Note.** KRX flags "not applied to issues with low liquidity" — low-liquidity issues may
be traded by periodic call instead of continuously. So "regular session ⇒ continuous"
is not universal even within 정규시장.

## A6.2 — 시가/종가 단일가매매 — opening and closing call auctions

**Fact.** Korean **단일가매매** (단일가격에 의한 개별경쟁매매); English **Periodic call
auction**; KRX's English guide names the sessions **Opening Auction Session** and
**Closing Auction Session**. Opening call collects 08:30–09:00, prices at 09:00; closing
call collects over the final 10 minutes, prices at 15:30. Both end at a random instant
within 30 seconds of nominal close.

KRX enumerates every place a single-price auction is used: 시가 결정, 종가 결정,
신규상장/재상장 시초가 결정, CB(시장 일시중단) 후 매매재개, 종목별 매매거래정지 후
매매재개 — plus 시간외단일가 (A6.4), 정리매매 (A5), and VI activation (a continuous
session switches to a 2-minute single-price auction when VI fires).

**Prints as a trade in a consolidated feed: YES.** The closing call auction is what
produces the official 종가; the opening call produces 시가. Both are ordinary prints.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010201/RGL03010201.jsp `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020205/RGL03020205.jsp (the 적용 list) `[PRIMARY]`
- KRX English guide, "Method of Trade Execution" + VI section `[PRIMARY]`

**Status.** `SETTLED`

**Note.** Two behavioural quirks that matter for bar construction. (a) At the **opening**
call only, if the price prints at 상한가 or 하한가, KRX **allocates** (수량배분) across
orders at that price rather than executing strictly by time priority; this explicitly does
**not** apply at the close. (b) **VI turns a stretch of the continuous session into a
2-minute single-price auction** — so "single-price auction print" is not confined to the
open and close, and a mid-session bar can contain a call-auction print.

## A6.3 — 시간외 종가매매 — after-hours (and pre-hours) closing-price session

**Fact.** Korean **시간외종가매매**; KRX English **Pre/After-hours Closing Price Trade**.
Two windows: **08:30–08:40** (pre-open, executes at the **previous** day's close) and
**15:40–16:00** (post-close, executes at **that day's** close). Order acceptance is
08:30–08:40 and 15:30–16:00 respectively. Only **time priority** applies — price, 위탁 and
quantity priority are all disabled, because there is only one possible price. Order type
is a 종가주문 / Limit Order in KRX's English matrix; **price amendment is prohibited**
(quantity reduction and cancellation are allowed). Trading unit 1 share.

Eligible: 주권, ETF, ETN, 외국주식예탁증권 — **excluding issues that did not trade that
day** (including issues whose close was formed by 기세 only).

**Prints as a trade in a consolidated feed: YES**, and always at exactly the close price
(or previous close, pre-open) — so these prints are price-flat by construction and will
distort any OHLC/VWAP that includes them without discrimination.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010301/RGL03010301.jsp — KOSPI
  "시간외종가/단일가매매" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020301/RGL03020301.jsp — KOSDAQ
  "시간외 종가 매매" `[PRIMARY]`
- KRX English guide, quotation-type matrix `[PRIMARY]`

**Status.** `SETTLED`

## A6.4 — 시간외 단일가매매 — after-hours periodic call auction

**Fact.** Korean **시간외단일가매매**; KRX English **After-hours Periodic Call Auction
Trade**. **16:00–18:00**, **10-minute** cycles, **12 auctions**. Price band: **당일종가
±10%, and additionally capped inside that day's 상·하한가**. **Limit orders only** (no
market orders). Trading unit 1 share. Eligible: 주권, 외국주식예탁증권 (KOSPI page also
lists ETF/ETN), excluding issues that did not trade that day and issues an ATS
(다자간매매체결회사) has notified as its own trading target that day.

**Prints as a trade in a consolidated feed: YES**, at up to 12 discrete price points
between 16:00 and 18:00 — genuinely price-forming within ±10%, unlike 시간외종가.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010301/RGL03010301.jsp `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020302/RGL03020302.jsp `[PRIMARY]`

**Status.** `SETTLED`

**Note.** A separate Dynamic VI regime applies here (KRX English guide: "After-hours
periodic call auction (16:00 - 18:00)", ±3% / ±6%). And per A5.2, 정리매매종목 trade in
this session **with no price limit at all**.

## A6.5 — 시간외 대량매매 / 바스켓매매, and 장중 대량/바스켓매매

**Fact.** Korean **시간외대량매매 / 시간외바스켓매매** and **장중대량매매 /
장중바스켓매매**; KRX English **Off-hours block/basket trade** and **Block/basket trade of
Regular session**. Applied for by a member through KRX's **K-Blox** system as *matched
two-sided orders* — a negotiated cross, not an auction. **Either the buy or the sell side
must be a single member.**

| | 장중 (regular-session) | 시간외 (off-hours) |
|---|---|---|
| Hours | 09:00–15:30 | 08:00–09:00 and 15:40–18:00 (KRX Korean pages) / 07:30–09:00 and 15:40–18:00 (KRX English guide — see A6.0 discrepancy) |
| Price range | within the high/low established **up to the moment the application is received** | within **that day's price limit** (상·하한가 이내) |
| Eligible | 주권, ETF, ETN, 외국주식예탁증권/KDR | same, **excluding issues that did not trade in the regular session** |
| Trading unit | 1 share | 1 share |

Minimum size (block): KOSPI 5,000× trading unit (500× for ETF/ETN) **or** ≥100,000,000
KRW *(KRX English guide)* — the Korean KOSPI page gives the alternative threshold as
≥50,000,000 KRW; KOSDAQ ≥50,000,000 KRW. Minimum size (basket): KOSPI ≥5 issues **and**
≥1,000,000,000 KRW *(English guide)* / ≥200,000,000 KRW *(Korean KOSPI page)*; KOSDAQ
≥5 issues and ≥200,000,000 KRW.

**Prints as a trade in a consolidated feed: YES** — these execute on-exchange at the
negotiated price. Because 장중대량 executes at an agreed price bounded only by the day's
running high/low, a block print can sit **away from the prevailing quote** and will
create spurious OHLC extremes and volume spikes if not separated out.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010302/RGL03010302.jsp — KOSPI
  "시간외대량/바스켓매매" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03010303/RGL03010303.jsp — KOSPI
  "장중대량/바스켓매매" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020303/RGL03020303.jsp — KOSDAQ
  "대량/바스켓매매" `[PRIMARY]`
- KRX English guide, "Block/basket trade" table (K-Blox, single-member requirement,
  thresholds) `[PRIMARY]`

**Status.** `PARTIALLY SETTLED` — the categories, hours, price ranges and mechanism are
settled; the **KRW minimum thresholds disagree between KRX's Korean and English pages**
(100M vs 50M for KOSPI block; 1,000M vs 200M for KOSPI basket). Both are KRX-published.
The English guide is likely the newer, but resolve before relying on a threshold.

## A6.6 — 경쟁대량매매 (A-Blox) — auction-based block trade

**Fact.** Korean **경쟁대량매매**; KRX English **Auction-based Block Trade** (product name
**A-Blox**; the English guide lists "Auction-based Block Trade Order" as a distinct
quotation type). Introduced **2010-11-29**. Anonymous large orders are concentrated in a
book **separate from the regular-session book** and matched against each other.

- Hours: **08:00–09:00** (시간외시장) and **09:00–15:00** (정규시장).
- Matching: continuous, on **time priority only**, filled in full.
- **Execution price is a VWAP computed after the close** and notified to each party:
  off-hours orders get the **whole-day** VWAP; in-session orders get the VWAP **from the
  match point to the close**. If no VWAP is available (no ordinary trading that day), that
  day's close (or the base price) is used.
- Minimum order 500,000,000 KRW (base price × quantity); trading unit 100 shares.
- Eligible: 주권, ETF, ETN, 외국주식예탁증권 — **투자주의/관리종목 and 정리매매종목 are
  excluded**.
- Disclosure: during the session, 경쟁대량매매 order quantity and execution information are
  **not published**; only the *existence* of a bid/ask for a given issue is published, and
  only during the regular session.

**Prints as a trade in a consolidated feed: SPECIAL CASE — the price is not known until
after the close.** A 경쟁대량매매 match occurs intraday but is *priced* post-close from a
VWAP. It therefore cannot appear as a normal timestamped print at its match time with a
final price. How (or whether) it appears in any given feed is a per-product question.

**Source.** https://regulation.krx.co.kr/contents/RGL/03/03010304/RGL03010304.jsp — KOSPI
"경쟁대량매매(A-Blox) 제도 개요" `[PRIMARY]`;
https://regulation.krx.co.kr/contents/RGL/03/03020304/RGL03020304.jsp — KOSDAQ
"경쟁대량매매" `[PRIMARY]`; KRX English guide, quotation-type matrix `[PRIMARY]`

**Status.** `SETTLED` as to the mechanism; `NEEDS VENDOR` as to how it surfaces in data.

## A6.7 — 협의대량매매 / 신고대량매매 (negotiated block trades)

**Fact.** **협의대량매매 is not current KRX terminology for equities.** KRX's equity
market-guide taxonomy has exactly two block families, and neither is named 협의대량매매:

- the **negotiated** family — 장중대량/바스켓매매 and 시간외대량/바스켓매매 (A6.5), which
  KRX describes as "매수자 및 매도자간 **협상**에 의한 가격으로 체결(**상대매매방식**)";
- the **auction** family — 경쟁대량매매 (A6.6).

KRX *does* use the literal term **협의대량매매** — but for the **KRX 금시장 (gold market)**,
where it is a listed sub-topic of that market's 거래제도, not of the equity market.
**신고대량매매** is an older colloquial label for the negotiated family and does not appear
in KRX's current equity market-guide pages.

**Prints as a trade in a consolidated feed:** whatever 장중/시간외 대량·바스켓매매 does
(A6.5) — there is no separate equity category here.

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03020303/RGL03020303.jsp (협상/상대매매방식
  wording) `[PRIMARY]`
- KRX regulation site navigation, which places 협의대량매매 under 일반상품제도 → 금시장 →
  거래제도 and **not** under 매매거래제도 → 유가증권시장/코스닥시장, e.g.
  https://regulation.krx.co.kr/contents/RGL/04/04010102/RGL04010102.jsp `[PRIMARY]`

**Status.** `SETTLED` (as a terminology finding: for KRX equities, use 장중/시간외
대량·바스켓매매 and 경쟁대량매매; 협의대량매매 is a gold-market term).

## A6.8 — 단주 / 소량 (odd-lot) trading

**Fact.** **There is no odd-lot category on KRX equities.** The 매매수량단위 (trading unit)
for 주권 is **1 share**, effective **2014-06-02**; DR 1 증권 (2014-06-02); ETF 1 share;
수익증권 1좌 (2014-06-02); 신주인수권증권/증서 1 each (2009-08-03); ELW 10 warrants
(2005-08-26). KOSDAQ states flatly "코스닥시장에서의 매매수량단위는 1주(1증서, 1좌)임".
In the off-hours market the unit is 1 share for everything.

Since the minimum tradable quantity is one share, there is no sub-unit quantity that could
constitute an odd lot, and consequently **no odd-lot session, order type, or print
category**. (단주 in Korean market usage refers to fractional entitlements arising from
corporate actions, which are cash-settled by the issuer, not traded.)

**Prints as a trade in a consolidated feed: N/A — the category does not exist.**

**Source.**
- https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T4.jsp — "매매수량단위",
  with the effective-date column `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03020100/RGL03020100.jsp — KOSDAQ
  "매매거래제도일반 → 매매수량 단위" `[PRIMARY]`
- KRX English guide, "Trading units" `[PRIMARY]`

**Status.** `SETTLED`

**Note.** ELW is the one exception with a 10-warrant unit — irrelevant to equities but
relevant if ELW ever enters the same pipeline.

## A6.9 — 자기주식매매 (treasury / own-share trading)

**Fact.** 자기주식매매 is **not a separate print category** — it is a *constrained way of
participating in existing categories*. KRX imposes: a dedicated 자기주식매매거래계좌; a
자기주식매매신청서 filed with KRX between the close of the prior day and 18:00; **limit
orders only**; a price band (pre-open: prior close to prior close +5%; intraday: ±5 ticks
around max(직전가격, 최우선매수호가)); **no order submission after 15:00**; cancellation
prohibited (repricing within the band allowed); and a daily quantity cap (≤1% of shares
outstanding, and within that the greater of 10% of the notified acquisition quantity or
25% of the last month's average daily volume).

**Disposals** may additionally be done via **시간외대량매매**, at within ±5% of the close
(and inside that day's 상·하한가), exempt from the daily quantity cap. Treasury
**acquisitions** via 시간외대량매매 are permitted only when acquiring from the government
etc. or under FSC approval — and in those specific cases **no price restriction applies at
all, price limits included**.

**Prints as a trade in a consolidated feed: YES, but indistinguishably** — a treasury buy
executes as an ordinary 접속매매/단일가 print, or as an ordinary 시간외대량매매 print. The
regulatory identity is in the *order*, not in the *trade*. It is visible ex-ante through
the pre-filed 자기주식매매신청서, not from the tape.

**Source.** https://regulation.krx.co.kr/contents/RGL/03/03010305/RGL03010305.jsp — KOSPI
"자기주식 매매 및 취득" `[PRIMARY]`; https://regulation.krx.co.kr/contents/RGL/03/03020306/RGL03020306.jsp
— KOSDAQ "자기주식 매매" `[PRIMARY]`

**Status.** `SETTLED`

## A6.10 — Other categories worth naming

**Fact.** Beyond the above, KRX's own 매매계약체결의 특례 (execution-method exceptions)
navigation for equities lists exactly: 시간외종가/단일가매매, 시간외대량/바스켓매매,
장중대량/바스켓매매, 경쟁대량매매, 자기주식 매매 및 취득, 정리매매종목의 매매방법. That
is the complete published exception set — all six are covered above (정리매매 in A5).

Three further mechanisms **change how the regular session executes** without being trade
categories of their own, and each one alters what a bar contains:

- **VI (변동성완화장치)** — a continuous session switches to a **2-minute single-price
  auction**; if VI fires during a periodic call, the call is extended 2 minutes. Bands:
  Dynamic VI ±3% (KOSPI200 constituents) / ±6% (others) in continuous auction, ±2%/±4% in
  the closing auction, ±3%/±6% in the after-hours periodic call; Static VI ±10% across all
  regular sessions. Exempt: 정리매매종목, 단기과열종목, 신규상장종목.
- **단기과열완화제도 (short-term overheating)** — the regular session's 접속매매 is replaced
  by **30-minute single-price auctions**, and the 시간외단일가 cycle lengthens from 10 to 30
  minutes. KOSPI 3 trading days; KOSDAQ 10 trading days, extendable by 10. Only 지정가호가,
  시장가호가 and 경쟁대량매매호가 may be submitted; IOC/FOK are disabled on the first two.
  **This produces the same 30-minute-auction bar shape as 정리매매 in a stock that is NOT
  delisting** — do not use bar shape to infer 정리매매.
- **Circuit Breakers (시장 일시중단)** — 3 phases (−8% / −15% +1% / −20% +1%, each sustained
  one minute). Phase 1–2: 20-minute halt, then a **10-minute single-price auction** before
  continuous trading resumes. Phase 3: market closed for the day, and **all after-hours
  trading and treasury purchases are cancelled**.

**Prints as a trade in a consolidated feed:** all three change the *shape* of ordinary
prints (call vs continuous, cadence) rather than introducing a new print category.

**Source.**
- KRX English guide, "Volatility Interruption", "Market Suspension (Circuit Breakers)",
  "Sidecar" `[PRIMARY]`
- https://regulation.krx.co.kr/contents/RGL/03/03010408/RGL03010408.jsp — KOSPI
  "단기과열완화제도" `[PRIMARY]`
- http://regulation.krx.co.kr/contents/RGL/03/03020406/RGL03020406.jsp — KOSDAQ
  "단기과열완화제도" `[PRIMARY]`
- KRX regulation-site navigation for 매매계약체결의 특례 (the complete exception list),
  reachable from https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100.jsp `[PRIMARY]`

**Status.** `PARTIALLY SETTLED` — the mechanisms are settled; their interaction with any
specific data product is not.

## A6.11 — Does KRX publish a CODE identifying the trade category in market data?

**Fact.** **Not publicly.** No KRX-published document reachable from `krx.co.kr`,
`data.krx.co.kr`, `openapi.krx.co.kr`, `regulation.krx.co.kr` or `global.krx.co.kr`
defines a trade-category / session code carried per trade in a market-data feed.

What *is* public:
- The **taxonomy in prose**, on the regulation pages cited throughout A6. Categories are
  named, but no code point is assigned to any of them anywhere in public KRX material.
- The **OPEN API**, which is **daily-aggregate only** — `stk_bydd_trd` / `ksq_bydd_trd`
  return `TDD_OPNPRC / TDD_HGPRC / TDD_LWPRC / TDD_CLSPRC / ACC_TRDVOL / ACC_TRDVAL` per
  issue per day. There is **no trade-level service and no category field** in the entire
  OPEN API catalogue. So the OPEN API cannot answer the category question even in
  principle.
- One suggestive but **uninterpretable** artefact: the OPEN API's daily-trading services
  filter on a column named `AGG_BAS_TP_CD` (an "aggregation basis type code"), always
  pinned to `'0'`. Its meaning and its other permissible values are **not published
  anywhere**. I flag it only because it is direct evidence that KRX's own daily
  aggregates are computed against a selectable aggregation basis — i.e. "which trades are
  in this number" is a real, parameterised choice inside KRX. **Do not infer what `'0'`
  means.**

The real-time feed message specification (the document that would carry such a code) is
distributed under an information-distribution agreement to contracted vendors and is not
published. Real-time KRX distribution in practice runs through Koscom.

**Source.**
- https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd — full OPEN API
  catalogue: ~40 services, all daily-aggregate, none trade-level, none with a category
  field `[PRIMARY]`
- Embedded BLD transaction XML on
  https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=erXKnEAzTqcGnkcoSdGA
  (`AGG_BAS_TP_CD = '0'`) `[PRIMARY]` — implementation detail, meaning undocumented
- https://www.koscom.co.kr/portal/main/contents.do?menuNo=200611 — Koscom, "시세정보 서비스"
  (real-time KRX 체결·호가 distribution) `[SECONDARY]`

**Status.** `NEEDS VENDOR`

---

## A6 overall status

`PARTIALLY SETTLED` — the taxonomy is fully public and settled; per-product inclusion and
any category code are `NEEDS VENDOR`.

### ⚠️ THE LINE, STATED CLEARLY

**The taxonomy is public. Which categories a given data product includes in its bars is a
per-product question that public sources cannot answer — that part still needs the vendor.**

Everything in A6 comes from KRX's own market-guide and rulebook material, and it settles
*what kinds of trade can print on KRX equities*. It settles **nothing** about any specific
data product. Concretely, for any candidate feed or historical bar dataset, **none** of
the following is answerable from public sources:

1. Do the bars include **시간외종가매매** prints? (They are always exactly at the close —
   including them inflates close-bar volume with zero price information.)
2. Do they include **시간외단일가매매** (16:00–18:00, ±10%) prints, and if so are they
   stamped into 10-minute bars or collapsed?
3. Do they include **장중대량/바스켓매매** prints? These can print away from the prevailing
   quote and will manufacture false intraday highs/lows and volume spikes.
4. Do they include **경쟁대량매매**, which has no price until after the close — and if so,
   at what timestamp?
5. Does the daily volume/value equal 정규시장 only, or 정규시장 + 시간외, or everything?
   (KRX's own `AGG_BAS_TP_CD` proves this is a parameterised choice at the source.)
6. Is there a **per-trade category flag** available at all in the product, so that the
   above can be filtered downstream rather than having to be trusted upstream?

Each of these must be obtained from the data provider in writing. They cannot be inferred
from KRX's published taxonomy, and they should not be inferred by inspecting sample data —
a category can be absent from a sample simply because it did not occur.

---

## Appendix — sources that could NOT be dereferenced

Recorded so the next pass does not repeat the attempts:

- **`law.krx.co.kr/las/`** (KRX 법규검색서비스) — frame-based JSP; content frames
  (`LawList.jsp`, `LawSearch.jsp`, `BonFrame.jsp`, …) return "죄송합니다. 에러가
  발생했습니다." without a session established through the frameset. `PrtView.jsp` renders
  full text but requires a `(lawid, pubno, pubdt)` triple that is only obtainable through
  the UI. This is why 「유가증권시장 업무규정」 제20조제3항 is cited to a secondary legal
  database, corroborated by three KRX pages.
- **`rule.krx.co.kr/out/regulation/regulationMain.do`** (KRX 법무포털) — returns
  "오류가 발생되었습니다" on direct GET; requires a POST/session from the portal index.
- **`data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd`** — returned an internal server
  error; the data-products catalogue could not be read.
- **`global.krx.co.kr/contents/GLB/05/...`** trading-hours pages — 404. The English guide
  PDF at `global.krx.co.kr/contents/GLB/01/0109/0109000000/guide_to_trading_in_the_korean_stock_market.pdf`
  was retrieved successfully and used instead.
- **KRX OPEN API production endpoint** `data-dbg.krx.co.kr/svc/apis/sto/...` — requires a
  registered account, per-service 이용신청 and an issued `AUTH_KEY`. Only the sample
  endpoint (10-row cap) was exercised. **This is the single blocker on A4.5.**


<!-- ================= a7-findings.md ================= -->

# A7 — KRX / Koscom published market-data licence terms

Research pass date: 2026-08-03. Read-only; published primary sources only.

## Reading note — the single most important structural fact

**KRX does not contract for market data. Koscom does.** KRX's own licensing page describes
the licence *taxonomy*, but every operative document (the Policy, the Terms Definitions, the
Order Form, the Fees Schedule) is a **Koscom** document, and the counterparty on every
market-data agreement is **코스콤 / KOSCOM**. KRX contracts directly only for (a) bulk
historical data sold through the KRX Data Marketplace shop and (b) the **index product
licence** (지수 라이센스), which is a different thing from the index-*calculation* agreement.

**There are therefore two distinct licensing regimes, and they answer the A7.2 questions
differently.** Keep them apart throughout:

- **Route A — real-time / end-of-day feed, contracted with Koscom.** Governed by the *Market
  Information Policies* + *Terms Definitions*. This is the regime with the favourable
  **Original Work** carve-out.
- **Route B — historical / bulk marketdata bought from the KRX Data Marketplace, contracted
  with KRX.** Governed by KRX's **마켓데이터 이용약관** (Market Data Terms of Use, eff.
  2023-10-30). This regime has **no** Original Work carve-out, expressly pulls **derived
  information** inside its restrictions, and carries a **survival clause**.

For an SDK consuming historical KRX bars, **Route B is the operative regime** and is the more
restrictive of the two on derived data.

A second structural fact governs everything below: the Route A agreement is assembled
from four separable documents, and **only two of them are published**:

| Document | Published? | Where |
|---|---|---|
| **Market Information Policies** (정보이용정책 / 정책서) | **YES** | Koscom doc store + KRX mirror |
| **Terms Definitions** (용어정의) | **YES** | Koscom doc store |
| **Terms and Conditions** (이용조건) — the contract body | **NO** | login-gated ordering flow |
| **Fees Schedule** (가격표, "Annex A of this Terms and Conditions") | **NO** | not published |
| **Order Form** (주문서) | **NO** | per-customer |
| Appendix 1 "Non-Account Subscriber Policy" (비계좌가입자 정책) | **NO** | referenced only |

Every "the published terms are silent" verdict below traces to the same cause: **termination,
survival, return-or-destroy and escrow are contract-body subjects, and the contract body is
the one document Koscom does not publish.** That is a clean, reportable finding rather than a
research failure.

---

# A7.1 — The KRX licence taxonomy

## A7.1.1 — The taxonomy, from KRX's own page

**Fact.** KRX publishes a "데이터 라이센스 / Data License" page that names five agreement
types in two tiers: two ordinary licences (General, End-user) and three "Special Licences"
(Index calculation, Tradable-product creation, Own-company stock data). The page gives an
English name in parentheses for each.

**Operative text (KR):**

> **데이터 라이센스 개요(About Data License)**
> 정보상품은 정보를 이용하는 목적에 따라 아래와 같이 계약하여 이용할 수 있습니다.
>
> **일반 이용계약(General license)** — 제3자에게도 정보를 제공할 수 있는 라이센스
> \* 일반이용계약 라이센스에는 허용된 정보이용 범위에 따라 일반용∙소매사업전용∙웹사이트∙체결가서비스용∙방송매체 전용의 4가지 외부제공 권한 옵션이 존재합니다.
>
> **최종이용사용 계약(End user license)** — 기관내부에서 이용하기 위한 라이센스
>
> **특수 라이센스(Special License)**
> **지수산출목적 이용계약(Index calculation agreement)** — 지수 산출용 계약
> \* 한국거래소의 정보를 이용 또는 가공하여 지수를 생성하거나 공표하고자 할 경우, 별도의 지수산출목적 이용계약을 통한 한국거래소의 사전 승인이 필요합니다.
> \* 가공지표를 한국거래소의 공식적 지표 또는 자료로 오인 또는 혼동시키는 행위는 항상 금지됩니다.
>
> **매매용금융상품 생성 목적 정보이용계약(The creation of tradable products agreement)** — CFD(차익결제거래) 생성용 계약
> \* 매매용금융상품이란 국내외 거래소, 시장플랫폼 등에서 매매될 수 있는 ETF, 지분증권, 파생계약(선물, 옵션, CFD 등 포함) 및 기타증권을 말하며, 이중 CFD 생성용 계약만 허용됩니다.
> \* 한국거래소의 정보를 이용 또는 가공하여 매매용금융상품을 생성하고자 할 경우, 별도의 정보이용계약을 통한 한국거래소의 사전 승인이 필요합니다.
>
> **자사주식정보(Own Company stock data)** — 자사 종목시세 수신 및 게재용 계약
> \* 이용사가 한국거래소 상장사일 경우, 이용사의 디지털 사이니지 장비에 게재하는 용도에 한하여 해당사의 종목 시세를 제공합니다.

**English rendering:**

> **About Data License.** Information products may be used under a contract chosen according to the purpose for which the information is used, as follows.
> **General license** — a licence under which information may also be provided to third parties. (The general-use licence carries four external-provision permission options, according to the permitted scope of information use: general / retail-business-only / website closing-price service / broadcast-media only.)
> **End user license** — a licence for use inside the institution.
> **Special License:**
> **Index calculation agreement** — a contract for index calculation. Where you intend to use or process KRX's information to create or publish an index, KRX's prior approval by way of a separate index-calculation agreement is required. Acts that cause a processed indicator to be mistaken for or confused with KRX's official indicator or material are prohibited at all times.
> **The creation of tradable products agreement** — a contract for CFD creation. A "tradable product" means an ETF, equity security, derivative contract (including futures, options, CFD, etc.) or other security that can be traded on a domestic or overseas exchange or market platform; **of these, only a contract for CFD creation is permitted.** Where you intend to use or process KRX's information to create a tradable product, KRX's prior approval by way of a separate information-use agreement is required.
> **Own Company stock data** — a contract for receiving and displaying one's own listed issue's quote. Where the user is a KRX-listed company, that company's own issue quote is supplied solely for display on the user's digital-signage equipment.

**Source.** `https://data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd?viewNm=MDCDATA402` —
"데이터 라이센스 - KRX | 정보데이터시스템". **[PRIMARY]**
The live page is **login-gated** (returns `alert('로그인 또는 회원가입이 필요합니다.')` and
redirects to `MDCCOMS001.cmd`). Text above read from the Internet Archive capture of the same
KRX-served page, snapshot **2025-12-16** (`https://web.archive.org/web/20251216084456/https://data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd?viewNm=MDCDATA402`).
Cross-checked against the 2022-05-24 capture, which is identical except that the
**tradable-products agreement did not yet exist** in 2022 — it was added between 2022-05 and 2025-12.

**Status.** `SETTLED`

**Note.** Read the two "Special Licence" bullets as *prohibitions with a named cure*, not as
product offerings. The default position is that index creation and tradable-product creation
are forbidden; the special agreement is the only route to prior approval. Also note the
tradable-products agreement is **narrower than its own definition**: the definition covers
ETFs, shares and all derivatives, but "of these, only a contract for CFD creation is
permitted" — so a published KRX-derived ETF or futures contract has *no* available licence
route on this page at all.

## A7.1.2 — Who the counterparty actually is, and the operative English names

**Fact.** For real-time and end-of-day KRX market data, both the general licence and the
end-user licence are concluded **with Koscom**, not KRX — including where the data is received
indirectly through a vendor.

**Operative text (KR), KRX-published brochure:**

> **• 외부제공사**
> 외부제공사가 실시간 KRX 시장정보를 수신 및 재분배 하려면 코스콤과 **"일반 정보이용계약"** 을 체결해야 합니다.
> **• 최종이용사**
> 최종이용사가 내부 이용 목적으로 실시간 KRX 시장정보를 수신하려면 코스콤과 **"최종이용사 정보이용계약"** 을 체결해야 합니다. **본 계약 의무는 정보사업자를 통해 시장정보를 수신하는 경우에도 동일하게 적용됩니다.**

**English rendering:** *External providers* — to receive and redistribute real-time KRX market
information, an external provider must conclude a "General Information Use Agreement" with
Koscom. *End-users* — to receive real-time KRX market information for internal use, an
end-user must conclude an "End-User Information Use Agreement" with Koscom. **This contracting
obligation applies equally where the market information is received via an information vendor.**

**Operative text (EN), Koscom's own product page:**

> Customers who wish to use or distribute KRX Information must complete the Market Information Service Agreement with KOSCOM depending on the purpose and scope of using the Information. […] a Customer who completed a 'General Market Information Service Agreement' with KOSCOM is a 'Vendor'; a Customer who concluded an End-Customer Market Information Agreement' is an 'End-Customer.'
> **General Market Information Service Agreement: License to provide Information to third parties**
> **End-Customer Market Information Service Agreement: License to use the Information within the company**

**Source.**
- `https://data.krx.co.kr/inc/datasale/Market%20Data%20Product%20Brochure.pdf` — "KRX 실시간 시장정보 상품 및 서비스 구성" (KRX-hosted, Koscom-authored; contact `marketdata@koscom.co.kr`). **[PRIMARY]**
- `https://data.koscom.co.kr/product/information-product-outline` — "Information Products / 정보상품 개요", Koscom MDCS. Content served from `https://data.koscom.co.kr/apis/v1/user/contents/01_01/latest-version`; `contentsVersion 1.0`, `createDateTime 2021-12-29`. **[PRIMARY]**

**Status.** `SETTLED`

**Note.** The English name is unstable across Koscom's own documents: the Policy calls it
"Market Information Agreement for End-Customer", the product page calls it "End-Customer
Market Information Service Agreement", and the 2010 policy called it "Indirect Datafeed User
Agreement". Ask the vendor for the **exact document title** before referring to it in
correspondence. The Korean names — 일반 정보이용계약 / 최종이용사 정보이용계약 — are stable.

## A7.1.3 — What the four external-provision options permit and forbid

**Fact.** The General licence's four options are defined in §2 of the published Policy. Option 3
(website closing-price service) is the only one whose fee is expressly a **flat rate independent
of user count**, and it is also the one that expressly forbids supplying any API.

**Operative text (KR), Policy §2:**

> **[옵션 1] 일반용** — (1) 내부용도로 정보 이용 (2) 내부용도로 이용하는 가입자에게 정보를 제공 ․ 단, 코스콤의 사전승인 없이 가입자에게 실시간정보를 데이터피드 방식으로 제공할 수 없다. (3) 방송매체를 통하여 일반대중에게 정보를 제공 (4) 코스콤의 사전승인을 득하고 2차 정보사업자에게 정보를 제공
> **[옵션 2] 소매사업 전용** — (1) 정책서(섹션 7)에 명시된 개인가입자의 자격을 충족하는 가입자에 한하여 정보를 제공 […] ․ 내부용도의 정보 이용은 정보의 외부 제공서비스를 위한 개발, 운영, 테스트 및 품질관리 용도에 한하여 허용된다.
> **[옵션 3] 웹사이트 체결가서비스용** — (1) 웹사이트 및 모바일용 애플리케이션을 통해 일반대중에게 호가 관련 정보(가격, 수량 등)을 제외한 정보를 조회 방식으로 제공. […] ․ **정보를 이용하는 제3자(일반대중 등)가 전자적인 방법으로 정보를 가공, 저장 또는 재분배할 수 있는 수단, 도구 또는 지원(API, DDE 등을 포함)의 제공은 엄격히 금지된다.** ․ 본 권한 옵션에 의한 정보 제공은 전문가형 사용자를 대상으로 하는 서비스에는 허용되지 않는다.
> **[옵션 4] 방송매체 전용** — (1) 일반대중을 대상으로 신문, TV, 라디오 및 이와 유사한 매체를 통해 정보를 제공. […] ․ 정보의 수신자가 전자적 방식을 통해 정보를 저장 또는 가공할 수 없도록 기술적으로 합리적 방법으로 방지되어야 한다. […] 명확성을 기하자면, **공용 인터넷을 통한 정보 제공 또는 웹 기반의 정보 제공서비스는 방송매체로 간주되지 않는다.**

**English rendering (decisive sentences):** Option 3 — "The provision of any means, tool or
support (including API, DDE, etc.) by which a third party using the Information (the general
public, etc.) could electronically process, store or redistribute the Information is strictly
prohibited"; and provision under this option "is not permitted for services targeting
professional users." Option 4 — "For the avoidance of doubt, provision of Information over the
public internet, or a web-based information provision service, shall not be regarded as a
broadcast medium."

The flat-rate character of Option 3 is stated in the KRX brochure:

> [옵션 3] 웹사이트 체결가서비스용 — 웹사이트 및 모바일용 애플리케이션을 통해 일반대중에게 체결가 정보 제공, **사용자 수에 관계없이 정액제 적용** ("a flat rate applies regardless of the number of users")

**Source.** 정보이용정책 / Market Information Policies, 2024-01-01, §2 (KR pp.2–3, EN pp.2–3);
KRX Market Data Product Brochure. **[PRIMARY]**

**Status.** `SETTLED`

## A7.1.4 — How fees are structured

**Fact.** The published documents establish the **shape** of the fee model but not a single
current number. The model has five components.

1. **기본료 / basic fee** — per Information Product. Referenced but not quantified in any published document.
2. **변동료 / Variable Fee** — "a variable Fee which is payable for use of Information per relevant **Unit of Count**". The Unit of Count is one of: **(1) ID** (unique per user or device, counted as at the first day of each month; netting across multiple sources expressly forbidden), **(2) 조회요청건 / query request** (per-issue, per-point-in-time; expressly *not* usable where the service streams, auto-updates, or broadcasts to multiple users), or **(3)** a similar basis approved by Koscom. (Policy §4)
3. **Flat rate** — Option 3 website closing-price service only, "사용자 수에 관계없이 정액제" (regardless of user count).
4. **비조회형목적 이용료 / Non-Display Purpose Fee** — charged **per application**, not per user. "If an application can use the Information multiple times simultaneously (e.g. multiple instances), it shall be counted as one application." Development/maintenance/test/QC-only and DR-only applications are excluded from the count. **Chargeable retroactively** on incomplete or missing declaration. (Policy §8)
5. **Discounts and waivers** — a preferential Variable Fee for **Private Subscribers** (six cumulative criteria, Policy §7); waivers for development/operation/test/QC users and for disaster-recovery-site users (Policy §6); a surcharge-bearing **"관계사 확장 옵션" / "Extension of Right to Affiliates"** option, limited to entities in which the customer or its holding company "owns more than 50% of issued share capital and exercises control".

**Fact — no revenue-share.** No published KRX or Koscom market-data document read in this pass
contains a revenue-share, percentage-of-turnover, or percentage-of-AUM fee term. The published
model is entirely count-based (per ID, per query, per application) plus flat rates.
*(This is a "silent on" finding, not a "no such term exists" finding — the unpublished Fees
Schedule could contain one, and the index-*product* licence almost certainly does.)*

**Operative text (EN), Terms Definitions:**

> **Fees Schedule** shall mean the Annex A of this Terms and Conditions that specifies Fees applicable to this Agreement;
> **Variable Fee** shall mean a variable Fee which is payable for use of Information per relevant Unit of Count; and
> **Unit of Count** shall mean the units of count specified in the Policy, which are to be used as the basis for generating Reports and calculating Variable Fees under this Agreement;

**Source.** Terms Definitions (Koscom), 3pp, §1.1; Market Information Policies 2024-01-01 §§4, 6, 7, 8;
Koscom order-system UI strings (`data.koscom.co.kr/js/app.e5751e3a.js`, i18n key
`affiliateExpansionOptionDescription`). **[PRIMARY]**

**Status.** `PARTIALLY SETTLED` — structure settled, **all current amounts unpublished**.

**Note.** The **only** published KRX/Koscom fee *numbers* found anywhere are in a **2009/2010**
Koscom vendor notice, and they are long superseded: Indirect Access Fees per site (e.g. KOSPI
Market–Stock USD 900 / KRW 720,000 monthly; KOSPI200 Index Futures USD 600 / KRW 480,000) and
Terminal User Fees per terminal (e.g. KOSPI Market–Stock USD 25/month; KOSDAQ USD 13;
KOSTAR Index Futures USD 3). Source:
`https://english.koscom.co.kr/upload/downloads/(Koscom)_New_Market_Data_Policy.doc` — "정보
데이터피드 상품에 대한 정책 / New and Changed Policy on Real-time Datafeed Service", dated
3.2.2009, effective 2010-07-01 (overseas) / 2010-10-01 (Korea), Appendix 1. **[PRIMARY, SUPERSEDED]**
Cite these only as evidence of *fee shape*, never as current pricing.

## A7.1.5 — The index *product* licence is a different agreement from the index *calculation* agreement

**Fact.** KRX publishes a separate "지수 라이센스 / Index License" page, contracted with KRX's
Index Marketing Team, covering the right to **issue financial products on a KRX index** and to
use KRX trademarks. It is non-exclusive. It is **not** the 지수산출목적 이용계약 (index
calculation agreement) of A7.1.1, and it does not carry the market-data feed.

**Operative text (KR):**

> **라이센스 정책**
> KRX 지수를 기초로 한 구조화 상품 또는 각종 펀드를 개발 및 발행하기 위하여 KRX 지수 라이센스 계약을 맺어야 합니다.
> KRX는 특정 지역 또는 국가에 대한 지수 라이센스의 독점적 사용권을 부여하지는 않습니다.
> **라이센스 부여범위 및 권한**
> 라이센스 계약을 통하여, 이용자(Licensee)는 KRX의 지수 및 상표에 대한 다음과 같은 권한을 얻을 수 있습니다.
> \* KRX 지수를 기초로 한 지수연계 금융상품의 발행 및 매매에 이용할 수 있는 권리
> \* 지수연계 금융상품의 마케팅 및 홍보 등의 목적으로 KRX 상표를 이용할 수 있는 권리
> **연락처** 인덱스마케팅팀 : index_marketing@krx.co.kr Tel +82-2-3774-4143, +82-51-662-2363
> ※ **지수의 (실시간)시세 데이터를 제공하는 서비스는 KRX지수라이센스 체결과는 별도로 코스콤 데이터사업팀(02-767-8618)에 문의하시기 바랍니다.**

**English rendering:** To develop and issue structured products or funds based on a KRX index,
a KRX Index Licence agreement must be concluded. KRX does not grant exclusive rights to an
index licence for any particular region or country. Under the licence the Licensee may obtain
the right to use KRX indices and trademarks for the issuance and trading of index-linked
financial products and for their marketing and promotion. **Note: the service supplying the
index's (real-time) quote data is separate from concluding a KRX Index Licence — contact
Koscom's Data Business Team.**

**Source.** `https://data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd?viewNm=MDCDATA403` —
"지수 라이센스", read via `https://web.archive.org/web/20251006172035/…` (live page login-gated). **[PRIMARY]**

**Status.** `SETTLED`

**Note.** Three separate agreements can therefore be in play at once for one index-linked
product: the Koscom **data** licence (to receive the feed), the KRX **index-calculation**
agreement (to compute your own index from KRX data), and the KRX **index-product** licence
(to issue a product on a *KRX* index). Do not let a vendor conversation collapse them.

## A7.1.6 — The site-level terms, and the market-data terms KRX references but does not publish

**Fact.** KRX's Data Marketplace publishes a 홈페이지 이용약관 (site terms of use). Those terms
contain a general no-reproduction clause and then **expressly defer the market-data terms to a
separate document that is not published on the site.**

**Operative text (KR):**

> **제12조(이용자의 의무)** ② 이용자는 당 사이트의 정보를 거래소의 사전 허락 없이 복사·복제·배포·전송·공중송신하여서는 아니 됩니다.
> **제12조의2(마켓데이터 이용에 관한 준수사항)** 이용자가 당 사이트를 통해 마켓데이터를 구매하거나 이용하는 경우, **별도로 정한 「마켓데이터 이용약관」을 준수하여야 하며**, 해당 약관에 동의하지 아니하는 경우 마켓데이터의 구매 및 이용이 제한될 수 있습니다.
> **제10조(금지행위)** ② 자동화 수단을 이용하여 정보를 무단 수집·복제·배포하는 행위
> **제16조(관할법원)** 본 약관과 관련하여 분쟁이 발생하는 경우 대한민국 법을 적용하며, 관할 법원은 서울남부지방법원으로 합니다.

**English rendering:** Art. 12(2) — a user shall not copy, reproduce, distribute, transmit or
publicly transmit the site's information without the Exchange's prior permission. Art. 12-2 —
where a user purchases or uses market data through this site, the user **shall comply with the
separately prescribed "Market Data Terms of Use"**, and purchase and use of market data may be
restricted if the user does not agree to those terms. Art. 10(2) prohibits unauthorised
collection, reproduction or distribution of information **by automated means**. Art. 16 —
Korean law; exclusive venue Seoul Southern District Court.

**Source.** `https://data.krx.co.kr/contents/MDC/INFO/informationController/MDCINFO003.cmd` —
"홈페이지 이용약관 - KRX | KRX Data Marketplace". Retrieved live 2026-08-03, no login required.
No revision date is printed on the page. **[PRIMARY]**

**Status.** `SETTLED` (the referenced 「마켓데이터 이용약관」 is recovered in full at A7.1.7 below)

**Note.** The live `MDCINFO003.cmd` today renders the *homepage* terms above; **the same URL
previously served the 마켓데이터 이용약관**, and KRX replaced it in place at some point after
2025-12-16. I probed `MDCINFO001`–`MDCINFO013` live: 001/002/003/008 render, 004/005/006/009–013
return `에러페이지`. **Art. 10(2) is worth flagging to any SDK/automation consumer: automated
collection of Marketplace data is prohibited by the site terms independently of any data licence.**

## A7.1.7 — 마켓데이터 이용약관 / KRX Market Data Terms of Use (Route B — the historical-data contract)

**Fact.** KRX's terms for **historical/bulk marketdata purchased through the Data Marketplace**
exist, are a full 15-article contract, and were **recovered and read in full**. Effective
**2023년 10월 30일**. This is the document that governs a purchased-bars use case, and it is
materially more restrictive on derived data than the Koscom Route A policy.

**Operative text (KR) — scope and product types, Art. 2:**

> ① "마켓데이터"라 함은 거래소가 증권·파생상품시장 등의 운영, 청산결제, 시장감시, 상장공시 등의 업무를 영위하며 직·간접으로 생산하여 데이터베이스에 저장하고 관리하는 **히스토리컬 마켓데이터**를 말한다.
> ③ "상품"이란 거래소가 판매를 목적으로 마켓데이터에 추출, 취합, 가공, 분석 등의 용역을 더하여 생산한 마켓데이터 상품을 말한다. […] 1. 선택형 […] 2. 비정형 […] 3. 구독형

**Operative text (KR) — Art. 9, user obligations. The decisive clause for derived data:**

> ① 이용자는 거래소에 제출한 **이용계획 등에 부합하는 목적 이외의 용도로** 마켓데이터를 사용할 수 없다.
> ② 법인 이용자는 이용계획 등에 기재한 목적을 위하여 필요한 범위 이내인 이용자의 임·직원에 한하여 동 마켓데이터 **또는 동 데이터를 가공한 정보**를 제공할 수 있다.
> ③ 법인 이용자는 마켓데이터 **또는 동 데이터를 가공한 정보**를 제공받은 이용자의 임·직원이 이용계획 등에 기재한 목적 이외의 용도로 동 데이터 또는 동 데이터를 가공한 정보를 사용하거나, **제3자에게 이를 제공할 수 없도록** 적절한 조치를 하여야 한다.
> ④ 법인 이용자는 […] 임·직원으로부터 제3항에 따른 의무를 명시한 **서약서를 제출받아 관리**해야 한다.
> ⑤ 이용자는 마켓데이터의 단순 복제, 게시, 표출 및 재분배 등의 행위를 통하여 **수익을 창출할 수 없다.** 단, 다음 각 호에 해당한다고 판단하는 경우 **일부 허용할 수 있다.**
> 1. 이용자가 구매 마켓데이터의 **가공을 통해 독창성 있는 저작물을 생산한 경우**
> 2. 이용자가 운영 및 관리하는 웹 페이지에 마켓데이터를 단순 게시, 표출하여 판매 등의 직접 수익 발생이 아닌 제3자의 방문, 가입을 통한 간접 수익 발생을 목적으로 하는 경우

**English rendering:** (1) The user may not use the marketdata for any purpose other than one
consistent with the usage plan submitted to the Exchange. (2) A corporate user may provide the
marketdata **or information processed from that data** only to its own officers and employees,
and only within the scope necessary for the purpose stated in the usage plan. (3) The corporate
user shall take appropriate measures to ensure that officers and employees who received the
data **or information processed from it** do not use it for any purpose outside the usage plan
**or provide it to third parties**. (4) The corporate user shall obtain and manage written
undertakings from those officers and employees. (5) The user **may not generate revenue**
through mere reproduction, posting, display or redistribution of the marketdata; provided that
the Exchange **may partially allow** it where it judges that either: (i) the user has produced
**an original creative work through processing** of the purchased marketdata, or (ii) the user
merely posts/displays it on its own web page for indirect revenue (third-party visits or
sign-ups) rather than direct revenue such as sales.

**Operative text (KR) — Art. 10(3)(4), audit with cost-shifting; Art. 11, Exchange's rights:**

> **제10조** ③ 거래소는 이용자의 본 약관 또는 계약사항 등의 준수여부를 판단할 목적으로 이용자에게 필요한 자료의 제출을 요구하거나 **컴퓨터 설비 등에 대한 조사**를 할 수 있으며 필요한 경우 이를 **대리인에게 위임**할 수 있다. 이 경우 이용자는 최대한 협조하여야 한다.
> ④ 본 조 제3항의 조사에 필요한 비용은 거래소가 부담하되, 조사결과 이용자가 본 약관을 위반하여 마켓데이터를 부당히 사용한 사실이 발견되는 경우에는 **이용자가 그 조사에 "소요된 비용"을 전액 부담**하여야 한다. "소요된 비용"이라 함은 인건비 및 장비사용료 등 본 조사와 관련되어 발생한 일체의 비용을 말한다.
>
> **제11조(거래소의 권리)**
> ① 거래소는 이용자가 제9조를 위반할 우려가 있다고 판단할 경우 이용자에게 마켓데이터 **이용 내역 제출을 요구**할 수 있다.
> ② 거래소는 이용자가 이용자의 의무를 위반하였음을 확인하거나, 본 조 제1항의 제출을 통해 위반을 확인할 경우 **즉시 마켓데이터 파기를 요청할 수 있다.**
> ④ 거래소는 마켓데이터에 대한 생산, 판매에 있어 마켓데이터의 내용, 형식, 기간, 전달방식 및 판매여부 등을 **임의로 변경할 수 있다.**
> ⑤ **본 조는 이용자의 마켓데이터 이용 목적 달성 이후에도 계속하여 효력을 발휘한다.**

**English rendering:** Art. 10(3) The Exchange may require submission of necessary materials or
**conduct an inspection of computer equipment etc.** to determine compliance, and **may delegate
this to an agent**; the user shall cooperate to the fullest. (4) The Exchange bears the cost of
that inspection, **but if the inspection finds the user has improperly used the marketdata in
breach of these terms, the user shall bear the entire "cost incurred"** — meaning all costs
related to the inspection including labour and equipment-usage charges. Art. 11(1) Where the
Exchange judges there is a risk the user has breached Art. 9, it may **require submission of the
user's marketdata usage records**. (2) Where the Exchange confirms a breach, it **may
immediately demand destruction of the marketdata**. (4) The Exchange may **unilaterally change**
the content, format, period, delivery method and availability of the marketdata.
**(5) This Article shall continue in force even after the user's purpose in using the
marketdata has been achieved.**

**Other operative terms:** Art. 3(4) — adverse amendments require **at least 15 days'** advance
notice. Art. 8(5) — **no refunds**, save under Art. 14 of the unpublished 「마켓데이터 관리지침」.
Art. 13(2) — the Exchange's liability is **capped at the purchase price received**.
Art. 14(3) — the Exchange **does not warrant accuracy or completeness**. Art. 15(2) — exclusive
first-instance venue, **Seoul Southern District Court**.

**Source.** "마켓데이터 이용약관 - KRX | 정보데이터시스템", served from
`https://data.krx.co.kr/contents/MDC/INFO/informationController/MDCINFO003.cmd`; closing line
`이 약관은 2023년 10월 30일부터 시행한다.` The live URL now serves the homepage terms instead;
full text read from the Internet Archive capture of that KRX-served page, snapshot **2025-12-16**
(`https://web.archive.org/web/20251216085801id_/https://data.krx.co.kr/contents/MDC/INFO/informationController/MDCINFO003.cmd`).
Retrieved and read end-to-end (Arts. 1–15). **[PRIMARY]**

**Status.** `SETTLED`

**Note.** Two structural points carry into A7.2. First, this is a **purchase**, not a
subscription: the document defines no 계약기간, no 해지, and no auto-renewal, so **there is no
termination event** — which is why the only destruction trigger is *breach*, not expiry.
Second, **Art. 11(5) is a one-way survival clause**: the *Exchange's* rights (usage-record
production, destruction demand, unilateral change) survive indefinitely after the user's purpose
is achieved. The user receives **no** corresponding surviving right. Also note Art. 9 is keyed
throughout to the **이용계획 (usage plan)** submitted at purchase — an unpublished,
per-purchase document that is the real scope boundary.

---

# A7.2 — Three specific contractual questions

## A7.2.1 — Does the licence permit publishing a content hash (e.g. a SHA-256 digest) of licensed data?

**Fact — part 1: "Information" is defined by recoverability, not by provenance.** The scope of
the licensed subject matter turns on whether the original prices/quotes can be got back out.

**Operative text (EN), Terms Definitions §1.1:**

> **Information**
> shall mean the market related data as transmitted and described in the Order Form as well as **the data processed or edited from which originally disseminated prices, quotes or other data can be determined through automated calculation or recalculated**;

**Fact — part 2: there is a defined term for exactly the opposite case, and it is carved out.**

**Operative text (EN), Terms Definitions §1.1 — decisive:**

> **Original Work**
> shall mean any work/product created from Information **where the underlying Information cannot be identified, reverse-engineered by automated process or recalculated. Original Work is not considered Information under this Agreement**;

**Fact — part 3: creating an Original Work is an enumerated, declarable, chargeable Non-Display
Usage — i.e. it is permitted, conditionally, not prohibited.**

**Operative text (EN), Policy §8:**

> **8. Non-Display Usage**
> Non-Display Usage shall mean the access, processing or use of Real-time Information for purposes other than displaying or disseminating the Information in respect of the following categories of usage:
> (1) Trading Based Usage […]
> (2) **Other Non-Display Usage**: use of Real-time Information within applications for the following activities.
>  Risk Management
>  Quantitative Analysis
>  Portfolio Management
>  **Creation of Original Work** or Other Non-Display Usage
>
> Unless otherwise provided in the Order Form, any Vendor or End-Customer who intends to use or uses Information for Non-Display Usage **shall declare this usage to KOSCOM** […]
> Non-Display Usage is subject to payment of Non-Display Purpose Fees […]
> **KOSCOM reserves the right to determine at its sole discretion whether the Vendor or End-Customer's Non-Display Usage is subject to declaration obligation.**

**Operative text (KR), Policy §8 — note the divergence:**

> **8. 비조회형 정보이용**
> 비조회형 정보이용은 조회 또는 분배 외의 목적으로 실시간 정보를 이용, 가공 또는 활용하는 형태로서 […]
> (2) 기타 비조회형 정보이용 : 다음 용도의 비조회형 어플리케이션을 통한 실시간정보 이용
>  ․ 위험관리 ․ 계량분석 ․ 포트폴리오관리 ․ **가공지표 산출 등**

**Source.**
- Terms Definitions (Koscom), 3pp, no printed revision date; record `createDateTime 2024-01-08`. Retrieved from Koscom's document store: `https://data.koscom.co.kr/apis/v1/user/standard-documents/12_01/latest-version` → file `Terms Definitions(deftnc).pdf`; reachable in the UI at `https://data.koscom.co.kr/deftnc` (KR: `https://data.koscom.co.kr/tncdef`, file `용어정의(tncdef).hwp`). **[PRIMARY]**
- Market Information Policies, January 1, 2024, §8 (EN p.8) / 정보이용정책, 2024년 1월 1일, §8 (KR p.8). **[PRIMARY]**

**Status.** `PARTIALLY SETTLED`

**Note — read this carefully; it is the finding most easily over-read.**

What the published terms **do** settle:
- There is an operative, defined category — **Original Work** — whose test is precisely
  one-wayness, and which is **expressly excluded from "Information"**. A cryptographic digest
  satisfies that test on its face: the underlying prices cannot be identified,
  reverse-engineered by automated process, or recalculated from a SHA-256 output.
- Producing such an artifact is **not prohibited**. It is enumerated as a permitted
  Non-Display Usage category, subject to **declaration** and a **Non-Display Purpose Fee**
  metered per application.
- It is **not** an Index. "Index" is separately defined as "any numerical information into
  which the Information is processed based on certain calculation standards **to recognize or
  value the whole or part of market**" — a digest recognises and values nothing, so the §9
  index prohibition does not reach it.

What the published terms **do not** settle:
- The words **hash, digest, checksum, fingerprint** appear **zero** times in either language
  version of the Policy and zero times in the Terms Definitions. There is no worked example.
- **§8 is scoped to Real-time Information.** The Original Work carve-out sits in the
  Definitions and is not so limited, but the permission-and-fee machinery is. Hashing
  *end-of-day* or *purchased historical* data is not addressed by §8 at all.
- **Creating** an Original Work and **publishing** one are different acts. §8 licenses the
  creation. Nothing in the published Policy expressly addresses onward publication of an
  Original Work — the inference is that publication is unconstrained *because* Original Work
  "is not considered Information", so the distribution controls in §2 and §10 (which govern
  "Information") do not bite. **That is an inference from a definition, not a granted right.**
- Koscom holds **sole discretion** over whether a given non-display usage is declarable, and
  the classification of a specific artifact as Original Work is **self-applied by the licensee
  with no published pre-clearance route**.

Do **not** report this as "publishing a content hash is permitted." Report it as: *the
published terms supply a named, favourable category and place the artifact outside the
definition of "Information"; they never name a hash, and the classification is unadjudicated.*

### Route B — historical/bulk data bought from KRX: the opposite answer

**Fact.** For purchased historical marketdata, KRX's 마켓데이터 이용약관 **expressly pulls
derived information inside the restriction** rather than carving it out. There is no Original
Work concept. Art. 9(2)–(3) repeatedly says "마켓데이터 **또는 동 데이터를 가공한 정보**"
("the marketdata **or information processed from that data**"), and confines both to the
purpose stated in the usage plan and to the licensee's own officers and employees, with third-
party provision prohibited.

The nearest thing to a carve-out is discretionary, not definitional:

> ⑤ 이용자는 마켓데이터의 단순 복제, 게시, 표출 및 재분배 등의 행위를 통하여 수익을 창출할 수 없다. 단, 다음 각 호에 해당한다고 판단하는 경우 **일부 허용할 수 있다.** 1. 이용자가 구매 마켓데이터의 **가공을 통해 독창성 있는 저작물을 생산한 경우**

**English:** The user may not generate revenue through mere reproduction, posting, display or
redistribution; provided that the Exchange **may partially allow** it where it judges that the
user has produced **an original creative work through processing** of the purchased marketdata.

**Status (Route B).** `NEEDS VENDOR`

**Note.** Three differences from Route A that must not be blurred:
- The Route A test is **definitional and objective** ("cannot be identified, reverse-engineered
  or recalculated" → not Information). The Route B test is **discretionary and subjective**
  ("where the Exchange **judges**… it **may partially** allow").
- Route B's 독창성 있는 저작물 ("original creative work") is a **copyright-flavoured
  originality** standard, not a one-wayness standard. A SHA-256 digest is a mechanical output
  with no creative authorship — it may well **fail** the 독창성 test while comfortably passing
  Route A's Original Work test. **A hash is likelier to qualify under Route A than Route B.**
- Art. 9(5) governs **revenue generation**, not publication as such. Publishing a hash for
  audit purposes with no revenue attached is not obviously within Art. 9(5) at all — but it
  remains within Art. 9(1)'s purpose limitation and Art. 9(3)'s third-party bar, both of which
  expressly reach 가공한 정보.

**Bottom line across both routes:** the licensed-data hash question is settled *favourably in
structure* only under Route A, and is *unsettled and discretionary* under Route B — which is
the route a historical-bars consumer actually sits on.

## A7.2.2 — Can a post-termination right to retain derived audit evidence be secured?

**Fact.** **The published terms are silent.** The Market Information Policies contains **no
termination clause, no expiry clause, no survival clause, no return-or-destroy obligation, and
no regulatory-retention carve-out.**

**Verification (exhaustive keyword scan of both language versions of the 2024-01-01 Policy):**

| EN term | hits | KR term | hits |
|---|---|---|---|
| terminat* | 0 | 해지 | 0 (contract sense) |
| expir* | 0 | 계약기간 | 0 |
| surviv* | 0 | 존속 | 0 |
| return | 0 | 반환 | 0 |
| destroy | 0 | 파기 / 폐기 | 0 / 0 |
| retention | 0 | 보존 | 0 |
| escrow | 0 | 에스크로 / 임치 | 0 / 0 |
| archiv* | 0 | 백업 | 0 |

**Fact.** The only retention provisions anywhere in the published corpus are **obligations
imposed on the licensee**, not rights granted to it, and they attach to *compliance records*,
never to data or derived artifacts:

**Operative text (KR), Policy §7(2):**

> (2) 정보사업자는 확약서 또는 개인가입자 자격 관련서류를 **최소 3년간 보관**하여야 하며, 코스콤이 요청시 제출할 것

**English (Policy §7(2), official EN text):**

> (2) The declarations of Subscribers and further documents on Private Subscriber qualification should be **retained by Vendor at least 3 years** and should be made available upon KOSCOM's request.

And, in the 2010 predecessor policy (KR + EN, Koscom-published):

> 시세정보 이용내역 확인에 필요한 사항(시세정보 이용권한의 부여 및 변경내역 등)은 **최소 3년간 보관**되어야 하며 코스콤이 요청할 경우 제출해야 한다.
> Matters necessary for confirming the usage of Information, including history of establishment and change of access rights to Information, shall be **retained at least three (3) years** and submitted upon the request of KOSCOM.

**Source.** Market Information Policies / 정보이용정책, 2024-01-01, §7 (both language versions);
`(Koscom)_New_Market_Data_Policy.doc`, §IV Entitlement Control. **[PRIMARY]**

**Status.** `NEEDS VENDOR`

**Note.** The reason for the silence is structural and is itself the useful finding:
termination and survival are **Terms and Conditions** subjects, and the Terms and Conditions
are not published. The Terms Definitions confirms the missing document exists and is
referenced throughout ("Fees Schedule shall mean the **Annex A of this Terms and Conditions**";
"**Commencement Date** shall mean the commencement date of this Agreement set out in the Order
Form"). A **Commencement Date** is defined; **no termination date term is defined** in the
published Definitions — consistent with the term/termination mechanics living in the
unpublished body.

**One published hook is worth taking into the vendor conversation**, and it is the same hook as
A7.2.1: if the audit evidence you wish to retain qualifies as **Original Work**, then it "is
not considered Information under this Agreement", and a return-or-destroy obligation drafted
over *Information* would not, on its face, reach it. That is an argument to put to the vendor
about the drafting of their own unpublished clause — **it is not a published permission**, and
must not be represented as one.

### Route B — historical/bulk data bought from KRX: partially answered, and the answer is asymmetric

**Fact.** Unlike Route A, KRX's 마켓데이터 이용약관 **does** address destruction and survival —
and it resolves both **in KRX's favour only**.

**Fact — there is no termination event, so no expiry-triggered destruction.** The document
defines no 계약기간, no 해지 and no auto-renewal (0 hits for each). It is a purchase. The only
destruction trigger is **breach**:

> ② 거래소는 이용자가 이용자의 의무를 위반하였음을 확인하거나, 본 조 제1항의 제출을 통해 위반을 확인할 경우 **즉시 마켓데이터 파기를 요청할 수 있다.** (제11조)

> **English:** Where the Exchange confirms the user has breached its obligations — whether
> directly or through the usage-record production required under paragraph (1) — it **may
> immediately demand destruction of the marketdata**.

**Fact — there is a survival clause, and it runs one way:**

> ⑤ **본 조는 이용자의 마켓데이터 이용 목적 달성 이후에도 계속하여 효력을 발휘한다.** (제11조)

> **English:** This Article shall continue in force **even after the user's purpose in using
> the marketdata has been achieved.**

**Source.** 마켓데이터 이용약관, eff. 2023-10-30, Arts. 9, 11. **[PRIMARY]** (full citation at A7.1.7)

**Status (Route B).** `PARTIALLY SETTLED`

**Note — this is the most consequential single finding for the audit-evidence question.**
Read the three facts together:
- **Favourable:** there is **no** general return-or-destroy obligation on expiry, because there
  is no expiry. Absent breach, nothing in the published terms requires a purchaser of
  historical KRX data to destroy anything, ever. That is a materially better starting position
  than a subscription licence with a standard wind-down clause.
- **Unfavourable:** Art. 11(5) makes the Exchange's destruction-demand power, usage-record
  production power, and unilateral-change power **survive indefinitely**. The licensee acquires
  **no** reciprocal surviving right — in particular, no right to retain anything.
- **The asymmetry is the finding.** A retention right is not *denied* by the published terms;
  it is simply *never granted*, while the countervailing power is expressly perpetual. Since
  Art. 11(2) is breach-triggered and Art. 9(3) reaches 가공한 정보 (processed information),
  **derived audit artifacts are exposed to a destruction demand on a finding of breach with no
  published carve-out** — which is precisely the scenario in which you would most need them.

**Bottom line across both routes:** Route A is silent because the clause lives in an
unpublished contract body; Route B is explicit, and what it makes explicit is a one-way
survival of the licensor's powers. Neither route publishes a licensee retention right.

## A7.2.3 — Is a licence-compatible escrow arrangement available?

**Fact.** **No published escrow provision found.** No KRX or Koscom published document read in
this pass contains any escrow, deposit-with-third-party, or 임치 arrangement under which a
third party would hold licensed data or a derived archive against the licensee losing its copy.

- Zero hits for `escrow` in the English Policy and Terms Definitions.
- Zero hits for `에스크로` and `임치` in the Korean Policy.
- The KRX Data License, Index License, 구입안내 and 홈페이지 이용약관 pages contain no such provision.
- **Route B confirmed negative:** zero hits for 에스크로 and 임치 across all 15 articles of KRX's 마켓데이터 이용약관 (eff. 2023-10-30), now read in full.

**Fact — the nearest published analogue is not escrow.** The Policy's only third-party-custody
concept is the **기술대리인 / Service Facilitator**, and it is a *delegation* device that
increases the licensee's exposure rather than insulating it:

**Operative text (KR), Policy §11:**

> **11. 기술대리인**
> 기술대리인은 정보사업자 또는 최종이용사의 정보 이용 및 분배와 관련하여 기술적/기능적 역할의 일부를 위탁받은 자를 말한다. 기술대리인은 코스콤과의 직접 계약 체결없이 정보사업자 또는 최종이용사의 서비스를 지원하는 범위에서 정보를 이용할 수 있다.
> 기술대리인에 의한 정보의 수신 또는 이용이 필요할 경우, 정보사업자는 코스콤의 **사전 서면 승인**을 득하여야 한다. […]
> ․ 정보사업자/최종이용사는 정보이용계약에 기재된 조건과 동일한 조건으로 기술대리인이 **코스콤의 조사(Audit)를 수용할 것임을 보증한다.**
> ․ 코스콤은 기술대리인을 승인 또는 불인정할 전적인 권한을 지니며, 코스콤의 단독적인 판단으로 기술대리인에 대한 승인을 철회할 수 있다.

**English (Policy §11, official EN text — decisive lines):**

> Vendor/End-Customer shall guarantee that Service Facilitator agrees to the same terms as Vendor/End-Customer has agreed with KOSCOM in respect of **KOSCOM's audit rights**;
> KOSCOM reserves all rights to accept or reject Service Facilitator and **may withdraw approval** [at its sole discretion].

**Source.** Market Information Policies / 정보이용정책, 2024-01-01, §11 (EN pp.9–10 / KR pp.9–10). **[PRIMARY]**

**Status.** `NEEDS VENDOR` (with a clean negative on the published record)

**Note — Route B makes escrow structurally harder still.** KRX's 마켓데이터 이용약관 Art. 9(2)–(3)
confines the marketdata **and information processed from it** to the licensee's own officers and
employees and requires the licensee to prevent provision **to third parties** — with no
approved-third-party mechanism anywhere in the document (Route A at least has the 기술대리인
route). On the published Route B terms, handing an archive to *any* external custodian appears
to be a breach on its face, and breach is exactly what triggers the Art. 11(2) destruction
demand. An escrow under Route B therefore needs an express written variation, not merely
consent.

**Note.** A third party holding a licensed-data archive would be a **Service Facilitator** in
Koscom's scheme, and would therefore require **prior written approval**, must accept Koscom's
audit rights on identical terms, and holds a status Koscom may **withdraw at sole
discretion** — the precise opposite of what an escrow is meant to guarantee. An escrow whose
custodian's authority can be unilaterally revoked by the licensor does not survive the event it
exists to survive. **If an escrow is needed, the Original Work route — depositing a derived
artifact that is "not considered Information" — is structurally more promising than seeking a
data escrow, and is the version of the question worth putting to the vendor.**

---

# A7.3 — Koscom's published usage policy

## A7.3.1 — Identification

**Fact.** The document exists, is public, and was **reached and read in full**.

| Field | Value |
|---|---|
| **Title (KR)** | 정 보 이 용 정 책 (정책서) |
| **Title (EN)** | Market Information Policies |
| **Revision date** | **2024년 1월 1일 / January 1, 2024** (printed on p.1 of both versions) |
| **Length** | 10 pages (KR), 10 pages (EN); 12 numbered sections |
| **Publisher** | KOSCOM (코스콤). Constitutes part of the agreement between the recipient and Koscom. |
| **KR file** | `정책서_20240101.pdf` (159,748 bytes) |
| **EN file** | `Policy_20240101.pdf` (234,186 bytes) |

**Reachable URLs (all verified 2026-08-03):**

1. **KRX mirror, direct and unauthenticated — the most citable link:**
   `https://data.krx.co.kr/inc/datasale/Market%20Data%20Usage%20Polices_ko.pdf`
   (also served as `…_ko.pdf?v=20250732`). Byte-identical (159,748) to Koscom's own
   `정책서_20240101.pdf`. Note the filename typo "Polices". **No English counterpart exists on
   the KRX host** — `…_en.pdf` returns 404.
2. **Koscom document store (canonical):**
   metadata `https://data.koscom.co.kr/apis/v1/user/standard-documents/11_01/latest-version`
   (classification `11_01` "주문서 정책서", `documentVersion 1.0`, `agreeEssentialYn: Y`),
   file `https://data.koscom.co.kr/apis/v1/common/files?fileUUID=<uuid>`.
   Serves KR or EN per the `KMBS-LANGUAGE` header.
3. **UI entry points:** `https://data.koscom.co.kr/product/information-product-outline`,
   with aliases `/policy` and `/tncdef` (KR) and `/epolicy` and `/deftnc` (EN).

**Operative text (EN, p.1):**

> The requirements specified in this Market Information Policies (hereinafter "Policy") **constitute a part of the Agreement** between recipient of Information ("You") and KOSCOM.
> […] The current version of the Policy may be viewed and downloaded on the website (https://data.koscom.co.kr/epolicy) and **if updated will be published and sent to You at least 90 days in advance.**

**Operative text (KR, p.1):**

> 정책서에 기재된 요건은 정보이용자와 코스콤간 체결하는 **정보이용계약의 일부를 구성**합니다.
> […] 정책서의 최신버전은 웹사이트(https://data.koscom.co.kr/policy)에서 조회 또는 다운로드할 수 있으며, **업데이트될 경우 최소 90일 이전에 공표될 것입니다.**

**Status.** `SETTLED`

**Note — a real defect worth knowing before you cite it.** The Policy's own self-declared
canonical URLs, `https://data.koscom.co.kr/policy` and `/epolicy`, **return HTTP 404** when
fetched directly. They are client-side Vue router *aliases* of
`/product/information-product-outline` and resolve only inside a JavaScript-enabled browser.
Any link-checker, archive, or `curl` will report the Policy's stated home as dead. **Cite the
KRX mirror (URL 1) for the Korean text; there is no equally stable public URL for the English
text — it must be pulled from the Koscom document API.** Also note the **90-day advance
publication** commitment: it is the licensee's only published protection against a unilateral
policy change, and it is a commitment to *publish*, not a right to *object*.

## A7.3.2 — What the Policy says on the three questions

**Q1 — content hash / derived data.** Addressed generically and favourably but never by name.
The Policy's §8 enumerates "**Creation of Original Work**" as a Non-Display Usage — permitted
subject to declaration and a per-application fee — and the companion Terms Definitions places
Original Work **outside** the definition of "Information". Zero occurrences of
hash/digest/checksum/fingerprint. See A7.2.1 for full text. **Status: `PARTIALLY SETTLED`.**

**Note a KR/EN divergence that matters.** The English §8 category reads "**Creation of Original
Work** or Other Non-Display Usage"; the Korean §8 category reads "**가공지표 산출 등**"
("calculation of processed indicators, etc."). These are not the same concept — "가공지표"
suggests a *derived indicator* (something that still measures the market), whereas "Original
Work" is the defined one-way carve-out. **The Korean text is the one that governs a Korean-law
contract.** A hash maps cleanly onto the English wording and awkwardly onto the Korean. This
divergence should be put to the vendor explicitly.

**Q2 — post-termination retention.** **The Policy is silent.** No termination, expiry,
survival, return, destroy, or regulatory-retention clause. The only retention term is an
obligation to keep *Private Subscriber qualification documents* for **at least 3 years**
(§7(2)). See A7.2.2. **Status: `NEEDS VENDOR`.**

**Q3 — escrow.** **The Policy is silent.** No escrow provision. The nearest analogue — the
기술대리인 / Service Facilitator of §11 — requires Koscom's prior written approval, imports
Koscom's audit rights, and is revocable at Koscom's sole discretion. See A7.2.3.
**Status: `NEEDS VENDOR`.**

## A7.3.3 — Retention periods

**Fact.** The Policy imposes exactly **one** retention period, of **3 years**, on
**qualification documents**, and it is an obligation, not a right.

> (2) 정보사업자는 확약서 또는 개인가입자 자격 관련서류를 **최소 3년간 보관**하여야 하며, 코스콤이 요청시 제출할 것 (§7(2))
> — "The declarations of Subscribers and further documents on Private Subscriber qualification should be retained by Vendor at least 3 years and should be made available upon KOSCOM's request."

The 2010 predecessor policy imposed the same 3-year period on **entitlement records**
(권한 부여 및 변경내역). The Policy prescribes **no retention period for the market data
itself**, and **no maximum retention period** either.

**Status.** `SETTLED` (as to what is stated) / `NEEDS VENDOR` (as to data retention, on which it is silent)

## A7.3.4 — Derived features and derived products

**Fact.** Three distinct published rules, in ascending severity:

1. **Derived work that is one-way → outside the licence's subject matter.** "Original Work […]
   **is not considered Information under this Agreement**" (Terms Definitions §1.1).
2. **Derived analytics that remain market-measuring → permitted, but declarable and
   chargeable.** Risk Management, Quantitative Analysis, Portfolio Management, 가공지표 산출 /
   Creation of Original Work are Non-Display Usage: declare on the Order Form, declare again on
   change, pay per application, **retroactively chargeable if under-declared** (Policy §8).
3. **Derived indices and derived tradable products → prohibited absent prior written
   approval.** Policy §9, the most restrictive clause in the document:

> **9. 지수 산출 및 매매용금융상품 생성 목적의 정보 이용**
> 주문서에서 별도로 명시되지 않은 한, 정보이용자는 정보사업자와 최종이용사를 막론하고 **코스콤의 사전 동의 없이 (1) 지수 산출 또는 (2) 매매용금융상품의 생성 내지 가격도출 용도로 정보를 이용하여서는 아니된다.**

> **9. Use of Information for Index Calculation and Pricing of Tradable Product**
> Unless otherwise provided in the Order Form, recipient of Information, whether Vendor or End-Customer, **is prohibited from using Information for the purpose of (1) calculating or publishing Index or (2) creating Tradable Product or deriving price of Tradable Product without prior written approval of KOSCOM.**

The two defined terms that fix §9's reach:

> **Index** shall mean any numerical information into which the Information is processed based on certain calculation standards **to recognize or value the whole or part of market**;
> **Tradable Product** shall mean any exchange traded fund(ETF), share, derivative contract(including futures, options, CFD) or other financial instrument, which is traded or available for trading at any domestic/overseas exchanges, trading platforms or other similar facilities;

**Status.** `SETTLED`

**Note.** §9 binds the **End-Customer as well as the Vendor** — an internal-use-only licensee
gets no relief from it. The "**to recognize or value the whole or part of market**" limb of the
Index definition is the load-bearing boundary: a derived number that measures the market is an
Index and is prohibited without approval; a derived number that identifies a dataset is not.

## A7.3.5 — Publication of results computed from the data

**Fact.** The Policy regulates onward provision of **Information**; it does not regulate
publication of **conclusions** computed from Information. The governing clause for an
End-Customer is §10, and its three cumulative conditions are the published boundary:

**Operative text (KR), Policy §10:**

> **10. 최종이용사에 의한 정보의 제한적 재분배**
> 최종이용사는 **제한적으로 발췌된 정보에 한하여** 자신의 일상적 금융서비스 활동과 연관하여 고객에게 구두 또는 서면의 형태로 제공할 수 있다. 단, 이러한 제한적 정보 제공은 다음의 요건을 모두 충족시켜야 한다.
> (1) 정보의 제공이 **비시스템적인 방법**으로 행해질 것. 여기서 "비시스템적"이란 비정기적이고 드문 주기의 정보 제공으로서 **정기적인 업데이트를 수반하지 않는 것**을 말한다.
> (2) 정보의 제공이 정보의 판매 또는 상업적 분배 활동과 관련되어 행해지지 않을 것
> (3) 정보를 유상으로 이용하는 자에게 **정보의 대체물로 이용되지 않으며**, 또한 그러한 대체 이용의 취지가 있지 않을 것
> 코스콤은 최종이용사에 의한 정보 제공이 본 섹션의 요건을 충족시키는지 여부를 판단하고, 이러한 정보 제공을 **제한하거나 허용하지 않을 권한을 가진다.**

**English (Policy §10, official EN text):**

> **10. Distribution of Limited Extracts of Information**
> End-Customer may distribute **limited extracts** of Information in written or oral communications with his clients in connection with the End-Customer's ordinary business as a provider of financial service, provided that the distribution of Information:
> (1) is done in a **non-systematic manner**. "Non-systematic" means the irregular and infrequent provision which **does not have effect of regular updating**;
> (2) is not made in connection with any sale of Information or commercial publishing activity; and
> (3) **does not have the result of substituting for, and is not intended to substitute for**, any Subscriber paying for access to the Information.
> KOSCOM reserves the all right to determine whether distribution of Information by End-Customer meets the conditions of this section and to limit or withdraw the rights to distribute limited extracts of Information.

**Status.** `PARTIALLY SETTLED`

**Note.** §10 is a **non-substitution** test, not a non-disclosure test — the question it asks
is whether your publication removes someone's reason to pay Koscom. Three consequences worth
carrying into the vendor conversation:
- The three conditions are **cumulative**; condition (2) alone rules out publication "in
  connection with any … commercial publishing activity", which is broad and undefined.
- A **regular** publication is disqualified by condition (1) regardless of how little it
  contains — periodicity, not volume, is the trigger.
- §10 governs distribution of *Information*. If what you publish is **Original Work**, §10 does
  not reach it by its own terms — the same inference as A7.2.1, and subject to the same caveat
  that it is an inference from a definition rather than an express permission.

---

# Appendix — source reachability (so Part D is not re-run over dead ground)

| Source | Result |
|---|---|
| `data.krx.co.kr/inc/datasale/Market Data Usage Polices_ko.pdf` | **200**, public, 10pp — the Policy (KR) |
| `data.krx.co.kr/inc/datasale/Market Data Product Brochure.pdf` | **200**, public — the brochure |
| Koscom doc API `…/standard-documents/{11_01,12_01}/latest-version` + `…/common/files?fileUUID=` | **200**, public — Policy (KR+EN), Terms Definitions (KR+EN) |
| `data.krx.co.kr/…/datasale/index.cmd?viewNm=MDCDATA{001,400,401,402,403}` | **200 header but JS login gate**; recovered via Wayback captures of the KRX-served pages |
| `data.krx.co.kr/…/MDCINFO003.cmd` | **200** live = 홈페이지 이용약관; the 마켓데이터 이용약관 recovered from the 2025-12-16 capture of the same URL |
| `eindex.krx.co.kr/…/GLB0508030000.jsp` · `index.krx.co.kr/…/MKD11030100.jsp` | **200**, public — Index License (EN / KR twins) |
| `data.koscom.co.kr/policy` · `/epolicy` | **404** — the Policy's own self-declared canonical URLs are client-side router aliases only |
| `global.krx.co.kr` | **200**, but its full sitemap contains **zero** Market Data / Data License / Information Products entries — no licensing content on this host |
| `tglobal.krx.co.kr` | **DNS NXDOMAIN** — host does not exist; disregard any citation to it |
| `data.krx.co.kr/inc/datasale/` directory listing; `…Polices_en.pdf` and spelling variants | **404** — a Wayback CDX sweep of that directory returns only the two PDFs above: **no price list, no contract template** has ever been published there |
| KRX price list ("판매 데이터 목록 및 가격") | JS-driven download behind login — **not retrieved** |

**Not published anywhere, on either route:** the Terms and Conditions (이용조건), the Fees
Schedule / 가격표, the Order Form (주문서), the 이용계획 (usage plan) form, Appendix 1
"비계좌가입자 정책", and 「마켓데이터 관리지침」.

---

# What Part D must still ask

The published record settles the taxonomy (A7.1), the fee *shape*, the derived-data hierarchy,
the non-substitution test for publication, and — on the KRX historical route — the destruction
trigger and the survival asymmetry. It leaves exactly these open. Each is phrased ready to put
to Koscom (`marketdata@koscom.co.kr`) or KRX (`krxdata@krx.co.kr` / `index_marketing@krx.co.kr`).

**Questions 1–13 concern Route A (Koscom feed licence); 14–22 concern Route B (KRX historical
purchase); 23 concerns the interaction.** If only one route is in scope, ask only that block —
that is the main way to shrink the paid conversation further.

1. **Will Koscom confirm in writing that a cryptographic one-way digest (e.g. SHA-256) of
   licensed data is "Original Work" as defined in the Terms Definitions — and therefore "not
   considered Information under this Agreement"?** If Koscom will not pre-classify it, what is
   the process by which a specific artifact is adjudicated, and who bears the risk of a later
   reclassification?

2. **The Korean §8 category reads "가공지표 산출 등" while the English §8 reads "Creation of
   Original Work". Which text governs, and does the Korean wording carry the same one-way
   carve-out?** (The Korean text governs a Korean-law contract; a hash maps cleanly onto the
   English and awkwardly onto the Korean.)

3. **Does the Original Work carve-out extend to end-of-day and purchased historical data, or
   only to Real-time Information?** §8's Non-Display machinery is expressly scoped to Real-time
   Information; the Definitions are not so scoped.

4. **Is *publishing* an Original Work — as distinct from creating one — unrestricted?** Confirm
   that §2's external-distribution options and §10's limited-extract conditions apply only to
   "Information" and therefore do not reach an Original Work.

5. **Please supply the Terms and Conditions (이용조건) and the Fees Schedule (Annex A).**
   Neither is published; every question below depends on their text.

6. **What is the contract term, and what are the termination rights on each side (including
   termination for convenience and notice period)?** The published Definitions define a
   Commencement Date but no termination term.

7. **On termination or lapse, is there a return-or-destroy obligation, and what exactly does it
   bite on?** Specifically: does it reach derived artifacts, or only "Information" as defined?

8. **Will Koscom grant an express post-termination survival right to retain derived audit
   evidence** — artifacts that are not Information and disclose no licensed content — **for a
   stated period, for the sole purpose of verifying a historical audit record?** If yes, for
   how long, and on what conditions (e.g. no onward disclosure, audit access preserved)?

9. **Is there any regulatory-retention carve-out** for records a licensee is required by
   Korean financial-services law to keep beyond the licence term? The published Policy has none.

10. **Is any escrow arrangement available** — Koscom-operated or Koscom-approved — under which
    a third party holds licensed data or a derived archive so it survives the licensee losing
    its own copy? Nothing is published.

11. **If escrow is only available via the 기술대리인 / Service Facilitator route (§11), can the
    approval be made irrevocable — or at minimum notice-bound — for the escrow use case?** As
    published, Koscom "may withdraw approval" at sole discretion, which defeats the purpose of
    an escrow.

12. **What are the current fee amounts** — basic fee per Information Product, Variable Fee per
    Unit of Count, and Non-Display Purpose Fee per application — and **which Unit of Count
    applies to a headless, non-display, single-application consumer?** (Neither "ID" nor
    "조회요청건" fits cleanly; §4(3) allows "a similar basis approved by Koscom".)

13. **Does any fee component depend on the licensee's revenue, AUM, or turnover?** No published
    document contains a revenue-share term, but the Fees Schedule is unpublished.

14. **(KRX) Is the 마켓데이터 이용약관 of 2023-10-30 still the current version?** It was served
    at `MDCINFO003.cmd` as recently as 2025-12-16 and that URL now serves the homepage terms
    instead. Please confirm the current effective version and supply a stable public URL.

15. **(KRX, Route B) Does a cryptographic one-way digest count as "구매 마켓데이터의 가공을 통해
    생산한 독창성 있는 저작물" under Art. 9(5)1?** A digest is mechanical and has no creative
    authorship, so it may fail an originality test while being the least disclosive artifact
    possible. If it does not qualify, is there any other basis on which a hash may be published?

16. **(KRX, Route B) Does Art. 9(3)'s bar on providing 가공한 정보 to third parties reach a
    published content hash?** If derived information is caught regardless of one-wayness, that
    is a materially stricter position than Koscom's Original Work carve-out and we need it
    stated.

17. **(KRX, Route B) Will KRX confirm that, absent breach, there is no obligation to destroy
    purchased marketdata or derived artifacts at any point?** The terms define no contract
    term, no termination and no expiry; the only destruction trigger we can find is Art. 11(2)
    on confirmed breach.

18. **(KRX, Route B) Art. 11(5) makes the Exchange's rights survive indefinitely. Will KRX
    grant a reciprocal surviving right to retain derived audit evidence** — artifacts
    disclosing no marketdata content — **including after a finding of breach, so that the audit
    record remains verifiable?** As drafted, Art. 11(2) could compel destruction of the very
    evidence needed to resolve the dispute.

19. **(KRX, Route B) What is the 이용계획 (usage plan), and what latitude does it allow?**
    Art. 9 keys every restriction to it, it is completed per purchase, and it is not published.
    Supply the form and confirm whether "internal quantitative research and backtesting with
    published hash-only audit attestations" is an acceptable stated purpose.

20. **(KRX, Route B) Please supply 「마켓데이터 관리지침」 Art. 14**, cross-referenced by Art. 8(5)
    as the sole exception to the no-refund rule, and confirm whether that internal directive
    contains any further retention, destruction or derived-data provisions.

21. **(KRX) The tradable-products agreement is stated to permit CFD creation only. What is the
    route — if any — for a KRX-data-derived product that is not a CFD?** The page's own
    definition covers ETFs, shares and derivatives generally, but the licence covers only CFDs.

22. **(KRX) Confirm the boundary between the 지수산출목적 이용계약 (index calculation) and the
    지수 라이센스 (index product licence), and whether a derived number that does not "recognize
    or value the whole or part of market" falls outside both.** The Index definition's
    market-measuring limb is the load-bearing boundary and has no published worked example.

23. **Where a single consumer sits on both routes — a Koscom feed licence for live data and a
    KRX Marketplace purchase for history — which regime governs an artifact derived from
    both?** Route A carves out Original Work; Route B pulls 가공한 정보 in. A hash over a
    dataset spanning both sources currently has two contradictory published answers.

---

# Addendum, 2026-08-04 — the production pass: the 이용신청 verified live

Part A ran entirely against the **sample** endpoint and closed with one free
acquisition step recorded but untaken: a per-service 이용신청 for the base-info
services. That application has since been made and **approved for six services**.
This addendum records the verification probe, which discharges the sample-endpoint
limitation stated at A6 and **corrects one Part A finding**.

## What was probed

All six services answer on the **production** path with this repository's existing
production `AUTH_KEY` (`LS_KRX_APPKEY`, `.env.calendar`). `basDd=20260803`, the last
session with a POSITIVE KRX witness.

| Service | Path | http | rows |
|---|---|---|---|
| 유가증권 종목기본정보 | `sto/stk_isu_base_info` | 200 | 943 |
| 코스닥 종목기본정보 | `sto/ksq_isu_base_info` | 200 | 1,820 |
| 코넥스 종목기본정보 | `sto/knx_isu_base_info` | 200 | 109 |
| 유가증권 일별매매정보 | `sto/stk_bydd_trd` | 200 | 943 |
| 코스닥 일별매매정보 | `sto/ksq_bydd_trd` | 200 | 1,820 |
| 코넥스 일별매매정보 | `sto/knx_bydd_trd` | 200 | 109 |

**Path trap, recorded so it is not rediscovered.** The market group in the path is
`sto` for **all** equity services, including KOSDAQ and KONEX. `ksq/ksq_isu_base_info`
and `knx/knx_isu_base_info` both return `404 {"respMsg":"API referenced by the path
does not exist."}` — which is indistinguishable at a glance from an unapproved
service, and would have been misread as a failed 이용신청. Vary the path before
concluding anything about entitlement.

**The 10-row sample cap is gone.** A6 could not distinguish "the master is complete"
from "the sample endpoint returns 10 rows for every `basDd`". Production returns
943 / 1,820 / 109 — whole-market magnitudes. That limitation is **discharged**.

> **Completeness is still not fully established, and this addendum does not claim it.**
> A6 set the bar at three steps: a real `AUTH_KEY`, a production call, and
> reconciliation against an **independent** listed-issue census. The first two are
> done. The third is not. Base-info and 일별매매정보 agree exactly per market
> (943/1,820/109 both ways), but both are KRX-sourced, so that is **internal
> consistency, not independent corroboration**. Completeness remains `UNESTABLISHED`
> in A6's strict sense.

## obl. 16 — 업종 is NOT served. Confirmed on production, whole market.

The base-info record carries **12 fields**: `ISU_CD`, `ISU_SRT_CD`, `ISU_NM`,
`ISU_ABBRV`, `ISU_ENG_NM`, `LIST_DD`, `MKT_TP_NM`, `SECUGRP_NM`, `SECT_TP_NM`,
`KIND_STKCERT_TP_NM`, `PARVAL`, `LIST_SHRS`. None is an industry classification.

`SECT_TP_NM` is the only field that could be mistaken for one, and it is not: its
values are **board sections** (`중견기업부`, `우량기업부`, `벤처기업부`,
`기술성장기업부`, `일반기업부`) plus status labels. Part A's NEGATIVE answer holds,
now on production evidence at n=2,872 rather than a 10-row sample. **업종 stays its
own acquisition line; M17's near-zero-marginal-cost premise stays false.**

## obl. 3 — CORRECTION: a SPAC *is* identifiable, partially, and not where Part A looked

Part A recorded: *"nothing identifies a SPAC (SPAC = `SECUGRP_ID='ST'` like any
ordinary share)."* That is **true of `SECUGRP_NM` and false of the record as a whole.**

Whole-market distributions at `basDd=20260803`, n=2,872:

- **`SECUGRP_NM`** — `주권` 2,825, `부동산투자회사` 23, `외국주권` 12,
  `주식예탁증권` 9, `사회간접자본투융자회사` 2, `투자회사` 1. So it **does** separate
  REIT, foreign and DR, and **does not** separate 보통주/우선주 or mark a SPAC.
  Part A correct on both counts.
- **`KIND_STKCERT_TP_NM`** — `보통주` 2,759, `구형우선주` 78, `신형우선주` 23,
  `종류주권` 12. Preferred separation lives here, 113 non-보통주 issues, every one of
  them `SECUGRP_NM='주권'`. Part A correct.
- **`SECT_TP_NM`** — carries an explicit **`SPAC(소속부없음)`** value: **68 issues.**
  This is the correction.

**Three structural limits on that discriminator, all load-bearing for D3:**

1. **It is KOSPI-blind.** `SECT_TP_NM` is empty for **all 943** KOSPI issues and
   populated for all 1,820 KOSDAQ and all 109 KONEX. A KOSPI-listed SPAC is not
   identifiable by this field at all. (All 68 labelled SPACs are KOSDAQ.)
2. **It is a single slot, so status DISPLACES class.** 71 issues are named
   `기업인수목적`; only 68 are labelled `SPAC(소속부없음)`. The 3 missing ones —
   `465320` 교보15호기업인수목적, `471050` 대신밸런스제17호기업인수목적, `472220`
   신영해피투모로우제10호기업인수목적 — all read `관리종목(소속부없음)` instead. **An
   administrative designation overwrites the SPAC classification.**
3. Therefore the discriminator **fails precisely on distressed names**, silently, and
   fails in the direction that matters: a universe contract loses the knowledge that
   an issue is a SPAC exactly when that issue enters 관리종목. This is the same shape
   as #245's `is_tradable = designation.is_none()` concern — a class fact and a status
   fact competing for one field.

**Consequence for D3.** SPAC class authority is now *partially* available where Part A
said it was absent, but it cannot be taken from `SECT_TP_NM` alone: doing so
misclassifies every KOSPI SPAC and every 관리종목 SPAC. A name-pattern fallback on
`기업인수목적` recovered exactly the 3 missing issues here, but that is a heuristic on
a free-text field and is offered as an observation, not a rule.

> **Method note.** A first pass matched `'SPAC'` against `ISU_ENG_NM` and returned 78 —
> contaminated by 7 substring hits on **AEROSPACE**/**Space** (한화에어로스페이스,
> 한국항공우주산업, 나라스페이스테크놀로지, …). The figures above use
> `SECT_TP_NM == 'SPAC(소속부없음)'` and the Korean `기업인수목적` on `ISU_NM`. Recorded
> because the contaminated number was briefly believed.

**Source.** Live production calls to `data-dbg.krx.co.kr/svc/apis/sto/{stk,ksq,knx}_isu_base_info`
and `.../{stk,ksq,knx}_bydd_trd` at `basDd=20260803`, 2026-08-04 `[PRIMARY]`.

**Status.** 이용신청 verification `SETTLED` — six services live on production. obl. 16
`SETTLED, NEGATIVE` on whole-market evidence. obl. 3 `PARTIALLY SETTLED` — class
authority mapped for REIT/foreign/DR/preferred/SPAC, with the SPAC discriminator's
three limits above as the open residual. Base-info **completeness** remains
`UNESTABLISHED` pending an independent census. `may_begin` is **unchanged and still
false**: neither obligation 17 nor 18 is touched by any of this.
