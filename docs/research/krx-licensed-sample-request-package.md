# KRX / Koscom licensed-sample information-request package

Authored 2026-08-03 against commit `5c53b45` (`main`, clean tree) to resolve
[Acquire and verify licensed KRX samples for the universe and fidelity contracts](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/255)
on [Wayfinder: production-ready attended ORB portfolio](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/241).

**Status: Part A executed 2026-08-03; Parts B, C and D authored and not sent.**
Part A — the free public pass — has been run; its verdicts are in §3, its
evidence and sources in the companion record
[`krx-part-a-public-pass-findings.md`](krx-part-a-public-pass-findings.md), and
Parts B/C/D below are narrowed by what it settled. **Nothing has been acquired
and nothing has been sent.** Parts B, C and D still require a human to sign and
send, and **#255 stays open** — its acceptance is *acquire and verify*.

---

## 0. What this document is

Nineteen obligations were routed onto #255 by four closed decision tickets. This
package turns every one of them into a question a KRX or Koscom account manager,
or a Tick Data sales engineer, can answer — in their vocabulary, against their
product identifiers, with the consequence of a missing answer stated beside the
question so the vendor can see which answers are dealbreakers.

It is deliberately **not** a purchase decision, a signed request, or a vendor
commitment. All three sit past this map's destination.

### The obligation count, derived rather than inherited

| Source | Items |
|---|---|
| #255 body — universe contract ([#245](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/245)) | 5 (conditions 1–5) |
| #255 body — fidelity contract ([#247](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/247)) | 4 (conditions 6–9) |
| #255 body — certification protocol ([#249](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/249)) | 1 (condition 10) |
| Comment from [#254](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/254) | 9 numbered + 1 in the coverage note = 10 |
| Comment from [#248](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/248) | 0 — one *consequence*, explicitly "no new acquisition obligation" |
| **Routed, raw** | **20** |

**Two merges.** #254's item 3 (M9 microstructure regime-change dates) and the
body's condition 10 (E3 microstructure changes inside the window) are the same
obligation asked twice; they merge, taking the **union** of their dimensions —
condition 10 names VI introduction, session close time, the tick-size table and
the short-sale regime, item 3 adds price-limit band width. #254's item 8 says of
itself *"This is already your acceptance condition 1"*, and merges into it.

**One unbundle.** Item 8 carries a second deliverable condition 1 does not
contain: an **effective-dated ISIN↔`shcode` alias mapping**. Condition 1
establishes *facts about* identifiers; the mapping is a *table to acquire*.
Different deliverable, different vendor question, so it splits back out.

**20 routed → 18 after merges → 19 after the unbundle.** Several of the
nineteen need more than one vendor question; the package therefore has more line
items than obligations, which is expected and is not double-counting.

> **Record note.** The number is stated three different ways in the closed
> record. #254's own *Still open* section says it routed **six** obligations
> here; the comment it actually posted on #255 carries **nine** numbered items
> plus a tenth in its coverage note; #248's #255 comment repeats "**six**" while
> its own *Still open on close* says "**nine** items". None of the three is the
> count of the comment as posted. Filed as a correction on
> [#242](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/242).

### The three-way split, and why it exists

Every item is tagged **(a)**, **(b)** or **(c)**. They have different owners and
wildly different lead times, and #243 already warned that *"the public catalog
proves none of these"* — which is true of the (b) items and **false of several
(a) items**, so failing to split them means paying round-trip latency for facts
that are free.

- **(a) — public.** Answerable from the KRX/Koscom public catalog, published
  product specifications, KRX Open API service documents, KRX Data Marketplace,
  FSC/OpenDART pages, or KRX's own rulebook. **No licence, no money, no human.**
  An agent can attempt every one of these today.
- **(b) — sample-only.** Answerable only from a real licensed delivery. These
  carry the unbounded external lead time that makes #255 the critical path.
- **(c) — commercial term.** A clause in an end-user agreement. Answered by a
  contract review or an account manager, not by data.

---

## 1. Session decisions (P1–P4)

Recorded because later sessions must not re-litigate them.

**P1 — The package is staged: a free public pass, then a paid ask narrowed by
what it returns.** Part A is agent-runnable at zero cost and settles the (a)
items; Part B is authored in full now but sent only after Part A's returns are
folded in. Rejected: one undifferentiated document (pays round-trip latency on
facts the public catalog already carries, and a long flat question list invites
partial answers); staging by counterparty (duplicates the shared commercial
terms across four documents and makes answers unreconcilable).

**P2 — Tick Data's quote is priced into the same round, not held as a
contingency.** #243's own procurement instruction is to send one RFP to
KRX/Koscom and one to Tick Data. Two live quotes is a negotiating position; one
is a price take. The requests do not overlap on the high-fidelity items: #243
requires the KRX millisecond quote/trade slice **either way**, so the fallback
can only ever displace the whole-market minute tier. Rejected: serial approach
(puts two unbounded external lead times end-to-end on the map's critical path).

**P3 — Both shape-determining commercial terms are asked in the first round,
framed as compliance.** Under [#248](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/248)'s
**N10** these two are the only items that make `may_begin` false, so every week
they are held back is a week the archive and register design is blocked. They
are framed as *"what does your licence permit us to record for audit"* rather
than *"may we publish your data"* — a retention and audit-evidence clause is
ordinary in an end-user agreement, and #243 already lists both among the clauses
a production licence should cover. Rejected: holding them until a quote is on
the table (keeps `may_begin` false through the entire first round trip for no
gain).

**P4 — Every item carries its own pre-declared consequence.** The closed
decisions already differ on what a missing answer costs — M11's heartbeat is a
pass/fail acquisition gate, M12's absent cadence evaluates as `Unevaluated` and
drops the feature, D9's missing apply-dates drop the R2 tier, F3's missing
auction phase *narrows* a claim rather than killing it — so a uniform rule would
misstate most of them. Writing the consequence beside the question also tells
the vendor which answers are dealbreakers. Rejected: uniform fail-closed
(overstates items the closed decisions treat as narrowing); record-and-decide-later
(reopens M12, which already chose fail-closed).

---

## 2. The obligation register

Nineteen obligations, each with its deciding source, its split, and its
pre-declared consequence. **`may_begin`** column follows #248's **N10**: `value`
= lands in a slot the specification already shapes, construction may start now;
`shape` = shape-determining, construction may not.

| # | Obligation | Decides | Split | `may_begin` | Consequence if unanswered |
|---|---|---|---|---|---|
| 1 | 6-digit code **reassignment** after delisting, and code **change** on market transfer and reverse split; whether ISIN changes traceably under the same events | D1 → [#256](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/256) | (a)+(b) | shape (via #256) | #256 stays open. **Not blocking** — M8's alias mapping is required whichever way the key goes, and #248's register found #256 blocks zero acceptance conditions. |
| 2 | Effective-dated **ISIN↔`shcode` alias mapping** table, covering inactive issues | M8 | (b) | value | M7's bitemporal store cannot key across a rename. Fatal to the store. |
| 3 | REIT, SPAC, foreign, DR and preferred each present, with **증권구분** demonstrably distinguishing them | D3 | (a)+(b) | value | D3 has no class authority; D4 fail-closed then excludes every symbol whose class is unestablished. **The universe empties. Fatal.** |
| 4 | The **proportion of symbols whose as-of eligibility cannot be established** in the delivery | D4 | (b) | value | D4's cost is unmeasured and vendors cannot be scored against each other. Procurement proceeds uninformed; not fatal. |
| 5 | **정리매매** trading mechanism, and whether daily price limits apply during it | D8, F6 | (a)+(b) | value | D8's structural gate cannot classify it — fail-closed excludes 정리매매 symbols, which is survivable for *entry*. For an **open position** F6's "mark at that day's lower price limit" has no value to read. Returns to #247 as an amendment; not resolved here. |
| 6 | Whether historical **ETF PDF apply-dates** are actually served, with publication timestamps and corrections | D9 | (b) | value | R2 tier drops; every ETF/ETN constituent-level feature drops. **Pre-decided** by #243: remove the feature, never reconstruct with hindsight. |
| 7 | **Auction phase and imbalance fields** — opening and closing call auctions, any single-price state, indicative price, imbalance | F3 | (b) | value | F3's auction arm is inoperable. Its other four arms stand. #243: *do not claim auction realism until the sample proves the fields* — so the Tier-2 claim narrows and the silent arm goes to N6's residual register. |
| 8 | **VI (변동성완화장치) trigger and release timestamps**, static and dynamic | F3, F9 | (b) | value | F3's VI arm is inoperable and F9's stream loses its VI events. Same shape as row 7 — narrow, record, do not pretend. |
| 9 | **Halt onset and resumption timestamps, and last-tradable-date** | F9, F6 | (b) | value | F9's event-grained stream **has no other source**. F6 degrades to unconditionally pessimistic with no resume path — a change to the fidelity contract, so it returns to #247 as an amendment. |
| 10 | **Book-reconstruction semantics at depth** in ST5001: is "was this size reachable at this price at this instant" answerable, and to what depth | F4 | (b) | value | F4's scale-indexed verdict cannot be computed. F1 makes Tier 2 **fail-closed at rung 2**, so this is **fatal to dose escalation past rung 1**. #243: do not approve a Level-I vendor as the high-fidelity tier. |
| 11 | ST5001/ST5002 **correction and re-delivery handling** — versioning, supersession, gap notification | F4, M2 | (a)+(b) | value | M2's two-part seal identity cannot distinguish a re-delivery from a restatement, which is the exact thing C3.4 needed and never had. |
| 12 | **KRX microstructure regime changes with effective dates**: VI introduction, continuous-session close time, tick-size table, short-sale regime, price-limit band width | E3, M9 | **(a)** | value | **Partly answered already — see A1.** The tick reform is fixed at **2023-01-25** in the tree itself (`rules.rs:21`), inside any window starting 2010-01-04, so a comparability-relevant change is **confirmed present**. For the rest: the floor evaluates as `Unevaluated`; under fail-closed the sample cannot start before the latest documented change, which collides with E7's power requirement — a collision that returns to #249, not resolved here. |
| 13 | **R2 publication cadence, per source** — declared, documented, per product | F8, M12 | (a)+(c) | value | **Pre-decided** by M12: a source with no declared cadence evaluates as `Unevaluated`, and under fail-closed the feature drops. |
| 14 | **In-scope trade-category definition, per product** — which trades belong in a bar (off-hours, negotiated, block, odd-lot, auction) | M19 | (a)+(b) | value | Reconciliation is exact-match on integers, so legitimate category differences read as gaps and **every day fails**. Fatal to acquisition acceptance. |
| 15 | Do the Koscom **KRX Securities A/B/C** feeds carry **sequence numbers *and* a heartbeat**, with documented cadence | M11, K5 | (a)+(b) | value | **Acquisition gate, not a parameter.** A feed with neither cannot satisfy K5 at all — an event stream is silent when nothing happens, so time-since-last-event cannot separate a quiet market from a dead feed. Reject the feed. |
| 16 | Do the **date-keyed basic-information services** carry effective-dated **industry classification (업종)** | M17 | (a)+(b) | value | M17 acquires it with no bound registered, expecting near-zero marginal cost. If it does not ride along, M9's single-procurement argument needs correcting before it holds. |
| 17 | Does the licence permit **publishing a content hash** of licensed data | M5, N4 | **(c)** | **shape** | N4's outer tier stays *process-auditability*; M5's register stays verdict-only. **Either answer is usable** — but until answered, `may_begin` refuses the archive and register design. |
| 18 | Can a **post-termination right to retain derived audit evidence** be secured | M20 | **(c)** | **shape** | Without a carve-out, **every sealed certification becomes permanently unverifiable when the licence lapses**. M20 records that as an accepted risk if the term cannot be had — so it needs an answer either way, not a best effort. |
| 19 | Is a **licence-compatible escrow** available | M5 | **(c)** | value | Considered and not adopted as design. Only material if available — it is the sole option that survives the operator losing the archive. No answer = no escrow, as assumed. |

### Part A verdicts against the register (executed 2026-08-03)

Sources for every row are in
[`krx-part-a-public-pass-findings.md`](krx-part-a-public-pass-findings.md);
the reasoning is in §3. Only obligations Part A touched appear here — the rest
are unchanged and still (b) sample-only.

| # | Part A verdict | what moved |
|---|---|---|
| 3 | **PARTIALLY SETTLED** | 증권구분 = `SECUGRP_NM`, **served**. But it does not separate 보통주/우선주 (that is `KIND_STKCERT_TP_NM`) and **no field identifies a SPAC**. Three of five classes covered. Completeness untested. |
| 5 | **SETTLED (public half)** | **가격제한폭 is expressly not applied to 정리매매종목** (업무규정 제20조제3항). ≤7 trading days, 30-min single-price auctions ×14, limit orders only. **F6 has no value to compute — the quantity is undefined by regulation, not missing from a feed.** Returns to #247 as an amendment. |
| 7 | untouched | Auction phase/imbalance fields remain (b). A2 confirms ST5001/5002 exist at order level; whether they carry auction-phase identifiers is unestablished. |
| 10 | **NARROWED — materially** | **ST5001 is order-level (MBO), millisecond, with a synchronized 10-level book snapshot on every event** (104 fields); ST5002 carries both-side order attribution (122 fields) and joins to it on receipt IDs. A product that can answer obligation 10 **exists and is priced**. Still a vendor claim, not verified data. |
| 12 | **PARTIALLY SETTLED** | **15 in-window regime changes enumerated with effective dates.** The load-bearing one: **regular-session close 15:00 → 15:30 on 2016-08-01**, which **arms** the latent `KRX_REGULAR_CLOSE` defect. Price limits ±15%→±30% on 2015-06-15 is `SETTLED` from KRX's own change register. |
| 14 | **PARTIALLY SETTLED** | Taxonomy fully public and enumerated. **No public per-trade category code exists**, and the OPEN API is daily-aggregate only. Odd-lot **struck** — the trading unit has been 1 share since 2014-06-02. Per-product mapping still (b). |
| 15 | **SETTLED — GATE PASSES** | Sequence number (`정보분배일련번호`, field #3, every message) **and** heartbeat (UDP `I2000` on every channel; TCP `Link` at **1 minute**, with fault detection assigned to the receiver) are both in Koscom's **publicly downloadable** 접속표준서. **The feed is not rejected; K5 is satisfiable.** |
| 16 | **SETTLED — NEGATIVE** | **업종 is not served** by the basic-information services, and appears nowhere in the ~40-service OPEN API catalogue. It is a **separate acquisition**, not a rider. **M17's near-zero-marginal-cost premise is false, so M9's single-procurement argument needs correcting** — the pre-declared consequence has fired. Belongs to #254. |
| 17 | **NOT RELEASED** (`shape` holds) | **Two regimes discovered.** Route A (Koscom) has an objective **Original Work** carve-out; Route B (KRX historical, the operative one) pulls **가공한 정보 inside** the restriction and offers only a discretionary 독창성 test a mechanical digest may fail. `may_begin` **stays refused**. |
| 18 | **NOT RELEASED** (`shape` holds) | Route B defines **no term, no 해지, no expiry** — absent breach nothing requires destruction, ever. But **Art. 11(5) makes KRX's destruction-demand power survive indefinitely with no reciprocal right**, and Art. 9(3) reaches derived information. The asymmetry is the finding. |
| 19 | **SETTLED — NEGATIVE** | **No published escrow provision on either route** (zero hits for 에스크로/임치 across all 15 Route B articles and both language versions of Route A's Policy). Route B has no approved-third-party mechanism at all. **M5's no-escrow assumption holds.** |

**Net effect on the paid ask:** one question struck outright (obl. 15), one
mostly struck (obl. 5), two narrowed to a residue (obl. 3, 10), one inverted into
a new acquisition line (obl. 16), one sharpened into named sub-questions
(obl. 14), one made nameable (obl. 12), and Part D restructured by counterparty.
**Neither `shape` obligation was released — `may_begin` is still false**, which
is the honest headline: Part A shrank the ask substantially without unblocking
construction.

### Sample-composition requirements

Not questions — constraints on what the delivered sample must **contain**. #243's
acceptance bundle already requires current/delisted/renamed/action-affected
stocks, suspensions and surveillance states, VI and price-limit days, open/close
auctions and off-hours, and ETF/ETN rebalance and correction cases. **M11 adds
one**, and it is the one most likely to be quietly dropped:

> **At least one retrospectively-corrected halt or VI record.** The tradability
> log is bitemporal, and the correction case is the one that distinguishes *"was
> this fill reachable"* from *"was this decision defensible."* A sample without
> it leaves the bitemporality claim untested. **Consequence: the sample is
> incomplete — re-request rather than accept.**

---

## 3. Part A — the free public pass (**EXECUTED 2026-08-03**)

Zero cost, zero lead time, no human — as designed by **P1**. **Part A has now
been run.** Every fact, source URL, `[PRIMARY]`/`[SECONDARY]` classification and
per-question status lives in the companion evidence record
[`krx-part-a-public-pass-findings.md`](krx-part-a-public-pass-findings.md). This
section carries the verdicts and what they strike.

**It settled more than it was scoped to.** Two of the seven groups closed
questions that were routed as *paid* items, and one closed a **pass/fail
acquisition gate** outright.

| group | verdict | effect on the paid ask |
|---|---|---|
| **A1** microstructure regime dates (obl. 12) | **PARTIALLY SETTLED** — 15 in-window changes enumerated with dates | **B/C obl. 12 questions become a confirmation, not a discovery.** Part C Q6 can now name the dates. |
| **A2** product coverage (obl. 7, 10, 11, 14 partial) | **SETTLED — all six codes**, including field counts, first-available dates, delivery, samples **and list prices** | **B.1's "quote us" collapses to a price *confirmation*.** |
| **A3** Koscom sequence + heartbeat (obl. 15) | **SETTLED — the gate PASSES** | **B.2 Q16 struck.** Two refinements remain. |
| **A4** basic-info fields (obl. 3, 16) | 증권구분 **YES**; 업종 **NO** | **B.2 Q6 inverted** — 업종 is a *separate acquisition*, not a rider. Q4 narrowed to SPAC. |
| **A5** 정리매매 (obl. 5) | **SETTLED** — no price limits, 30-min auctions ×14 | **B.2 Q7 reduced** to the state-carriage half. |
| **A6** trade-category taxonomy (obl. 14 partial) | taxonomy **SETTLED**; per-product mapping **NEEDS VENDOR** | B.2 Q14 sharpened into six named sub-questions. |
| **A7** licence terms (obl. 17, 18, 19) | **two regimes discovered**; obl. 19 **negative**; 17/18 unreleased | **Part D restructured by route.** |

### A1 — the answer that changes the tree, not just the ask

> **The KRX regular-session close DID change inside the window: 15:00 → 15:30,
> effective Monday 2016-08-01.** The open never moved (09:00 throughout). The
> last 15:00 session was Friday 2016-07-29.

The package's previously-recorded finding — that `KRX_REGULAR_CLOSE` is a flat
15:30 constant at `adapters/nautilus/src/rules.rs:37` with no effective-date
switch, read by production ingest at `ingest/mod.rs:629`, `:974-975` and `:2544`
— was recorded as **latent**, conditional on exactly this question. **It is now
armed.** Acquiring history to #243's `2010-01-04` mis-stamps roughly **1,630
sessions** (2010-01-04 … 2016-07-29) by +30 minutes, silently.

Three refinements the execution added, none of which were assumed:

- **The exposure is close-only.** `KRX_REGULAR_OPEN` (`rules.rs:31`) is equally
  flat but is **inert** — referenced only by its own assertion at `rules.rs:267`,
  never by ingest. The open did not move anyway.
- **`rules.rs:37` is the sole production definition** of a session close in the
  repository; a repo-wide search for other `15:30`/`153000` literals returns only
  SDK test fixtures.
- **Two distinct failure modes, not one.** `:629` *stamps* a bar; `:974-975`
  derives *ingest range bounds*. A timestamp error and a filter-boundary error
  need separate remediation.

Status is `PARTIALLY SETTLED` rather than `SETTLED` for an honest reason: the
*fact* and the *current state* are primary (KRX's own regulation portal, 법제처,
FSC), but the **effective date** rests on a licensed broker's regulatory notice
citing the five amended 업무규정 제4조 articles verbatim, plus contemporaneous
national press. ≥6 mutually independent sources agree; none dissent. KRX's 2016
보도자료 and the rulebook 부칙 are not publicly reachable (`law.krx.co.kr` and
`rule.krx.co.kr` are session-gated; no pre-2016 Wayback capture). **To upgrade:
pull the 부칙 in a browser session.**

The other four dimensions, and the full 15-row chronological table of in-window
changes, are in the findings record. Highlights:

- **Price limits ±15% → ±30% on 2015-06-15 — `SETTLED`** from KRX's own
  **가격제한폭 변경 내역** register, a published change-history table. This is
  the strongest single artefact in the pass. KONEX remains ±15% today; leveraged
  ETFs get the band **multiplied by leverage**, so a flat ±30% validator
  false-rejects them.
- **Tick reform 2023-01-25 — CONFIRMED**, and #254's **M9** ("none of those
  regime dates is established anywhere in this tree") stands corrected, as this
  package already argued. New: the 2023 unification **coarsened** KOSDAQ's
  200,000–500,000원 tick from 100원 to 500원 — the one band where the reform made
  ticks *bigger*. A KOSPI-only tick lookup is wrong for pre-2023 KOSDAQ.
- **Short-sale**: four of six events `SETTLED` from FSC primary sources. Two
  cautions — the 2023 ban began **2023-11-06**, not the 2023-11-03 several English
  outlets print; and 2021-05-03 → 2023-11-05 is a **split-universe** regime
  (only KOSPI200/KOSDAQ150 shortable, membership rebalanced each June/December),
  so a per-symbol shortability flag must be date- *and* membership-aware.
- **VI arrived in two stages**: dynamic **2014-09-01**, static **2015-06-15**.
  Static VI shipped in the same package as the ±30% widening.
- **Forward-looking risk**: KRX has announced an **after-market 16:00–20:00 from
  2026-09-14**. The regular close stays 15:30, so a 15:30-stamped bar stays
  correct — but anything treating 15:30 or 18:00 as "end of day" needs re-checking.

**This belongs to [#254](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/254)'s
acquisition layer and is recorded, not fixed — the map ends before implementation.**

### A2 — all six products settled, prices included

Every product fact the paid round trip was going to ask for is **public**. The
prior "behind a free membership login" conclusion was **half right**: the wall
covers the catalog *page* layer, but the AJAX endpoints those pages call are
unauthenticated, and the official price book (*KRX 데이터 상품 안내*, 2026.1)
downloads without an account.

| code | product | shape | first date | list price (기준가격, 전체항목/1yr) |
|---|---|---|---|---|
| **ST1002** | 주식 일중 매매정보 | 1/10-min bars, 34 fields, **Level I** + 17 derived microstructure metrics | 1999-10 (KNX 2013-07) | 1분 **1,485,000원** · 10분 990,000원 |
| **ST5001** | 주식 호가장 | **per-order (MBO) millisecond events + 10-level book snapshot on every event**, 104 fields | 1999-10 | 유가 **10,000,000원** · 코스닥 5,800,000 |
| **ST5002** | 주식 체결장 | ms trades + 10-level snapshot + **both-side order attribution**, 122 fields | 1999-10 | same table as ST5001 |
| **EP1004** | 증권상품 일중 매매정보 | **11 fields, no quote data at all** | ETF 2002-10 | 1분 **600,000원** |
| **EP5001** | 증권상품 호가장 | ST5001 architecture for ETP, 105 fields (adds 이론가격) | ETF 2002-10 | **3,000,000원** |
| **EP5002** | 증권상품 체결장 | ST5002 architecture for ETP, 122 fields | ETF 2002-10 | **3,000,000원** |

> **The load-bearing answer: ST5001 is neither Level I nor Level II — it is
> stronger than both.** Order-by-order (MBO) event data with a synchronized
> 10-level book snapshot attached to every event, at millisecond resolution.
> `ORD_ACPT_NO` / `ORD_PRIOR_NO` / `MODCANCL_TP_CD` / `ORGN_ORD_ACPT_NO` give
> per-order identity and modify/cancel lineage; ST5002 carries the same receipt
> IDs on **both** sides, so **trade↔order joins across the two products are
> feasible**.

This bears directly on **obligation 10**, whose unanswered consequence was
*"fatal to dose escalation past rung 1"*. Public sources now establish that a
product exists which, on its face, can answer *"was this size reachable at this
price at this instant"* **to 10 levels**. That is a vendor claim, not a verified
property — B.2 Q12 is narrowed to the residue, not struck.

Three cautions carried forward:

- **A schema discontinuity at 2018-07-16 (EOS migration).** `BRD_ID` / `SESS_ID`
  / `MKT_ID` exist only **after** it; `REGUL_OFFHR_TP_CD` / `BLKTRD_TP_CD` /
  `MKT_TP_CD` are **discontinued after** it. KRX further warns pre-2010 data may
  omit items or use different definitions and **expressly declines to treat that
  as a defect** — which matters for any refund or fitness claim.
- **EP1004 is materially thinner than ST1002** — no quote fields whatsoever, and
  none of the derived metrics. An ETF/ETN minute tier is not the equal of the
  stock one.
- **A defect in KRX's own copy**: the public 개요 for **EP5002** contains the
  *bond* product's description (국채전문유통시장). The field list and a live
  sample confirm it is the ETP trade book. Raise it; do not let a procurement
  decision rest on that paragraph.

Delivery: CSV throughout; 호가장/체결장 bypass the size caps via cloud download;
**extraction takes ~2 weeks and can exceed a month** — a real lead time to plan
around. 50% academic/public-interest discount exists. KRX **refuses purchases for
resale or redistribution** and may demand proof of stated purpose.

Genuinely not establishable: **official English product names** (English *field*
names are available), and the **quoted price for a specific selection** — the
figures above are KRX's published reference table.

### A3 — the acquisition gate PASSES, for free

> **Obligation 15 asked a pass/fail question: do the Koscom KRX Securities A/B/C
> feeds carry sequence numbers *and* a heartbeat? Both are documented in
> Koscom's own publicly downloadable connection standard — no login, no
> contract.**

- **Sequence number — present.** `정보분배일련번호`, `Int(8)`, **field #3 of
  every** 증권A/B/C message, on both UDP and TCP.
- **Heartbeat — present on both transports, by different mechanisms.** UDP:
  `Polling Data` / TR-CODE `I2000`, 10 bytes, on **every** multicast channel
  without exception. TCP: the `Link` 세션 유지 패킷.
- **Cadence — explicitly 1 minute on TCP**, and the spec itself assigns the
  detection duty: *"1분 이내 데이터 또는 LINK가 수신되지 않는 경우 장애 처리는
  수신자가 한다"*. That is the exchange specifying a 1-minute liveness threshold
  and placing it on the consumer — precisely the guarantee the gate wanted.

**The feed is not rejected. K5 is satisfiable.** Two caveats that are
*procurement* facts rather than gate items, and one that changes what to buy:

- The sequence is **per-symbol-per-board, not channel-level** (it was formerly
  per-send-port). It catches per-instrument gaps but **cannot detect a silent
  channel on its own** — the heartbeat is what closes liveness. Both are needed;
  both exist.
- **Only the 100M/200M dedicated multicast line carries the full unfiltered
  10-level book.** The 12M/45M lines and the public-internet TCP feed apply
  `우선호가 상시 필터링` — permanent order-book filtering.
- The TCP feed has **no automatic retransmission** on disconnect; recovery is an
  explicit pull, business days 06:00–21:00, and for 시세 data returns **only the
  latest snapshot per symbol**, not the missed stream.

### A4 — one field present, one absent, and the absent one costs

**증권구분: YES — `SECUGRP_NM`.** But it does **not** do everything obligation 3
asks. It separates 주권 / 투자회사 / 부동산투자회사 / 선박투자회사 /
사회간접자본투융자회사 / 주식예탁증권 / 외국주권. It does **not** separate common
from preferred — that is a *different* field, `KIND_STKCERT_TP_NM` (보통주/우선주)
— and **nothing in the payload identifies a SPAC**, which carries
`SECUGRP_ID = 'ST'` like any ordinary share. Obligation 3 names five classes:
three come from 증권구분, one needs a second field, and **SPAC is unavailable
from this service at all**.

> **업종: NO.** There is no industry/sector field in `stk_isu_base_info` or
> `ksq_isu_base_info`, and the string 업종 occurs **nowhere in the ~40-service
> KRX OPEN API catalogue**. `SECT_TP_NM` (소속부) is a market *segment*, not an
> industry classification.

**This inverts obligation 16.** M17 acquired 업종 "with no bound registered,
expecting near-zero marginal cost" — i.e. assuming it rides along. It does not.
KRX does publish 업종, but through the `data.krx.co.kr` 업종분류 screens: a
**separate acquisition problem**. Per obligation 16's own pre-declared
consequence, **M9's single-procurement argument now needs correcting before it
holds** — that consequence has fired, and it belongs to #254.

Two further results, both of which correct this package's own premises:

- **`basDd` is genuinely point-in-time — but the prior evidence for it was
  wrong.** The service page hardcodes `20200414` as its sample default, so
  #243's "observed a 2020-04-14 state" was very likely just receiving that
  default. The evidence that *does* count is differential: varying `basDd`
  changes the result set exactly as as-of semantics predict, and the backing
  query joins on `:basDd BETWEEN STRT_DD AND END_DD`. Delisted issues **are**
  returned for dates when they were listed (demonstrated). Status is
  `PARTIALLY SETTLED`: the behaviour is demonstrated, but **KRX publishes no
  statement that it is point-in-time**, so there is no documented contract — only
  an observed implementation that could change without breaking a published promise.
- **#243's ten-row observation was an artifact.** The *sample* endpoint
  (`/svc/sample/apis/...`) returns exactly 10 rows for **every** `basDd`. It says
  nothing whatever about the production endpoint. **This group establishes which
  fields exist; it can never establish completeness.**

> **The completeness blocker is smaller than it looks, and it is free to clear.**
> Settling completeness needs a production `AUTH_KEY` and a production call —
> and **this repository already holds one**. `LS_KRX_APPKEY` in the gitignored
> `.env.calendar` authenticates the daily calendar chain against the *production*
> path `data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd`
> (`adapters/nautilus/src/calendar_refresh/fetch_state.rs:387`), whose own code
> comment already records that `openapi.krx.co.kr` is merely the portal. What
> remains is a **per-service 이용신청** for the two base-info services — free,
> operator-runnable, and it would settle obligations 3 and 16's completeness
> *before* the paid ask. **Recorded as available, not taken: this is an
> acquisition act and #255's acceptance is acquire-and-verify.**

### A5 — 정리매매 settled, and the answer is a hard NO

> **Price limits do not apply during 정리매매.** 「유가증권시장 업무규정」
> 제20조제3항: *"…정리매매종목의 경우에는 **가격을 제한하지 아니하며**…"*,
> corroborated by three separate KRX pages (KOSPI 가격제한폭 제도, KOSPI
> 정리매매, KOSDAQ 정리매매). It holds in the regular session **and** in
> 시간외단일가매매.

Mechanism, also `SETTLED`: **≤7 trading days**, **30-minute single-price
auctions, 14 per day**, **limit orders only** (KOSDAQ states it explicitly).

**What this does to obligation 5 is stronger than "unanswered".** F6's rule
*"mark at that day's lower price limit"* has **no value to compute** — and the
reason is regulatory, not a data gap. There is no 상한가 and no 하한가 for a
정리매매 issue on any day of the period; the quantity is **undefined by
regulation**, not merely missing from some feed. A silent ±30% fallback would
**fabricate a bound the market does not impose**, in exactly the situation — a
delisting stock moving tens of percent inside one 30-minute auction — where the
error is largest. The rule needs an explicit 정리매매 branch. **This returns to
[#247](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/247) as an
amendment, as obligation 5 already routed it — the package does not resolve it.**

Two secondary consequences: fills arrive at **≤14 discrete auction points**, not
continuously; and 7 days is a **ceiling, not an invariant** (KOSPI's rule says
"may permit", KOSDAQ's says "이내에서"), with some delisting causes carrying no
정리매매 period at all.

One cross-link to A1: the published **"14 auctions"** count is arithmetically
consistent with an 09:00–15:30 session. A pre-2016-08-01 session yields **13**.
The figure is date-dependent.

### A6 — the taxonomy is public; the mapping is not

KRX's complete published exception set for equities is
시간외종가/단일가매매, 시간외대량/바스켓매매, 장중대량/바스켓매매, 경쟁대량매매,
자기주식 매매 및 취득, and 정리매매 — all now documented with hours, price rules
and mechanism in the findings record. Selected results that bear on bar
construction:

- **No odd-lot category exists.** The trading unit for 주권 has been **1 share
  since 2014-06-02**, so there is no sub-unit quantity that could constitute an
  odd lot, and no odd-lot session, order type or print category. Strike it from
  the taxonomy.
- **협의대량매매 is not equity terminology.** KRX uses it for the **gold market**.
  For equities the families are 장중/시간외 대량·바스켓매매 (negotiated) and
  경쟁대량매매 (auction). 신고대량매매 is an obsolete colloquial label.
- **경쟁대량매매 (A-Blox) has no price until after the close** — it matches
  intraday but is *priced* from a post-close VWAP. It therefore cannot appear as
  a normal timestamped print at its match time.
- **자기주식매매 is not a separate print category** — the regulatory identity is
  in the *order*, not the *trade*; it is invisible on the tape.
- **단기과열완화제도 produces the same 30-minute-auction bar shape as 정리매매 in
  a stock that is not delisting.** Do not infer 정리매매 from bar shape.

> **The line, stated plainly: KRX publishes no per-trade category code in any
> public document, and the OPEN API is daily-aggregate only.** One suggestive but
> uninterpretable artefact: KRX's own daily-trading services filter on a column
> `AGG_BAS_TP_CD` ("aggregation basis type code") pinned to `'0'`, with no
> published meaning or value set. It is evidence that **"which trades are in this
> number" is a real parameterised choice inside KRX** — it is *not* a basis for
> inferring what `'0'` means.

### A7 — two licensing regimes, not one

The single most consequential structural finding of the pass:

> **KRX does not contract for market data. Koscom does** — except for bulk
> historical data sold through the KRX Data Marketplace, and the index product
> licence. **There are therefore two regimes, and they answer the same question
> oppositely.**

- **Route A — real-time / EOD feed, contracted with Koscom.** Carries an
  **Original Work** carve-out: work from which the underlying data *"cannot be
  identified, reverse-engineered by automated process or recalculated"* **is not
  considered Information under this Agreement**. Definitional and objective.
- **Route B — historical / bulk marketdata bought from KRX**, governed by
  **마켓데이터 이용약관** (eff. 2023-10-30, 15 articles). **No** Original Work
  concept. Art. 9(2)–(3) repeatedly binds *"마켓데이터 **또는 동 데이터를 가공한
  정보**"* — derived information is pulled **inside** the restriction.

**For an SDK consuming historical bars, Route B is operative — and it is the
stricter of the two.** Both are in scope here: obligation 15's Koscom feed is
Route A, the ST/EP historical products are Route B.

**Obligation 17 (content hash) — NOT released; `may_begin` stays refused.**
Route B's only relief is discretionary and copyright-flavoured: KRX *"may
partially allow"* where the user produced **독창성 있는 저작물** (an original
creative work). A SHA-256 digest is mechanical with no creative authorship, so it
may **fail** the 독창성 test while comfortably passing Route A's one-wayness
test. **A hash is likelier to qualify under Route A than under Route B.** Route
A's own carve-out is favourable *in structure* but the classification is
self-applied with no published pre-clearance route — an inference from a
definition, **not a granted right**. Do not report either as "permitted".

**Obligation 18 (post-termination retention) — NOT released, but its shape is
now known, and it is asymmetric.**

- *Favourable:* Route B defines **no 계약기간, no 해지, no expiry**. It is a
  purchase. **Absent breach, nothing requires destruction, ever** — a materially
  better starting position than a subscription licence with a wind-down clause.
- *Unfavourable:* Art. 11(5) — *"본 조는 이용자의 마켓데이터 이용 목적 달성
  이후에도 **계속하여 효력을 발휘한다**"* — makes KRX's destruction-demand and
  record-production powers **survive indefinitely**, with **no reciprocal right**
  for the licensee. Art. 11(2) triggers on breach; Art. 9(3) reaches 가공한 정보.
- **The asymmetry is the finding.** A retention right is not *denied*; it is
  never *granted*, while the countervailing power is expressly perpetual. Derived
  audit evidence is exposed to a destruction demand **precisely when it is most
  needed** — during a dispute.

**Obligation 19 (escrow) — SETTLED NEGATIVE on the published record.** Zero hits
for 에스크로/임치 across all 15 Route B articles and both language versions of
Route A's Policy. Worse under Route B, which has **no approved-third-party
mechanism at all** (Route A at least has a 기술대리인 route), so external custody
reads as breach on its face. M5's assumption — *no escrow* — holds. **Ask once
in Part D for completeness; do not negotiate.**

**Why Route A is silent, stated as a finding rather than a failure:**
termination, survival and return-or-destroy are **Terms and Conditions**
subjects, and the contract body is the one document Koscom does **not** publish.
The published Definitions define a *Commencement Date* and no termination term —
consistent with the mechanics living in the unpublished body. Part D therefore
opens by **requesting the Terms and Conditions and the Fees Schedule**; every
other Route A question depends on their text.

**One self-flagged limitation.** The Route B text was read from a 2025-12-16
Wayback capture of the KRX-served URL; KRX has since replaced that page in place.
**Part D Q14 asks KRX to confirm the version is current** — it cannot be verified
from the public record.

### What Part A did not settle

Recorded so the boundary is not blurred:

- **Completeness of anything.** No delivered dataset was inspected. Every A2
  product fact is a vendor claim on a public page.
- **Per-product trade-category inclusion** (obl. 14) — the taxonomy is public,
  the mapping is not, and it must be obtained **in writing**. It cannot be
  inferred from sample data: a category can be absent from a sample simply
  because it did not occur.
- **Obligations 17 and 18** remain `shape` under N10; **`may_begin` is still
  refused for the archive and register design.**
- **The exact quoted price** for a specific market × date-range × field-subset
  selection.
- Effective dates for **dynamic VI (2014-09-01)**, the **2010 tick revision day**,
  the **2011 short-sale ban start** (KCMI publishes both 08-10 and 08-11), and
  the **uptick-rule exemption abolition** — all secondary-only.

---

## 4. Part B — the KRX / Koscom paid request (HITL)

**Part A has been run (§3); this Part is narrowed accordingly.** Struck and
narrowed items are marked inline rather than deleted, so the reader can see what
the free pass bought.

### B.1 — Products in scope

Quote all of the following as one bundle, and separately, so the package
discount is visible. **A2 established every list price publicly**, so this is a
**confirmation**, not a discovery — say so, and ask them to confirm or correct
the published figure rather than quoting from scratch.

| Product | Scope needed | published list price (2026.1) |
|---|---|---|
| **ST1002** stock intraday (1-minute) | KOSPI **and** KOSDAQ, **active and inactive** issues, full history to the earliest available date (**1999-10**) | 1분 **1,485,000원**/yr |
| **ST5001** stock quote book (millisecond) | Representative slice — see B.3 | 유가 **10,000,000원** · 코스닥 **5,800,000원**/yr |
| **ST5002** stock trade book (millisecond) | Same slice as ST5001 | same table as ST5001 |
| **EP1004 / EP5001 / EP5002** | ETF/ETN mirrors, same shapes, Reference Instruments only. **Note EP1004 is 11 fields with no quote data** — it is not an equal of ST1002 | EP1004 **600,000원** · EP5001/5002 **3,000,000원** each |
| KRX instrument-event / market-action history | Full history, effective-dated, with corrections | — |
| Date-keyed KOSPI/KOSDAQ basic information | From 2010-01-04, full daily cross-section | free OPEN API (see §3 A4) |
| Historical ETF **PDF** rows | With publication timestamps, corrections, inactive products | — |
| **업종 (industry) classification history** | **NEW — A4 established it is *not* in the basic-information service.** Effective-dated, from `data.krx.co.kr` 업종분류 or wherever KRX licenses it | — |
| Koscom **KRX Securities A / B / C** | Prospective feed, contracted forward from certification start. **Specify the 100M/200M dedicated line** — A3 found the 12M/45M and TCP lines apply permanent order-book filtering | — |

State plainly to the vendor: **the whole-market minute tier and the millisecond
tier are separable purchases.** The minute tier may go to a competitor (Part C);
the millisecond quote/trade slice will not, because it is the only source that
can answer obligation 10.

**Two lead-time facts to plan around, both from A2:** 호가장/체결장 extraction
takes **~2 weeks and can exceed a month**; and selective data is retained only
**7 days** after delivery.

### B.2 — The written questions

Ask each in the vendor's own terms. Numbers are the obligation register's.

**Identity and universe**

1. *(obl. 1)* Does KRX ever **reassign** a retired 6-digit issue code to a
   different issuer or a different issue after delisting? If so, how frequently,
   and is the reassignment itself an event in your instrument-event history?
2. *(obl. 1)* Does the 6-digit code change on **market transfer** (KOSDAQ→KOSPI)
   or on **reverse split**? Does the **ISIN** change under those same events, and
   if it does, does your history carry a traceable predecessor→successor link?
3. *(obl. 2)* Can you deliver an **effective-dated ISIN↔short-code mapping**
   covering inactive as well as active issues, with valid-from/valid-to dates?
4. *(obl. 3)* **— NARROWED by A4.** We have established from KRX's published
   spec that `SECUGRP_NM` (증권구분) is served and separates 주권 / 투자회사 /
   부동산투자회사 / 선박투자회사 / 사회간접자본투융자회사 / 주식예탁증권 /
   외국주권, and that 보통주-vs-우선주 is carried separately by
   `KIND_STKCERT_TP_NM`. **What remains:** (a) **no field we can find identifies
   a SPAC** — a SPAC appears to carry `SECUGRP_ID = 'ST'` like any ordinary share.
   How is a SPAC identified in your delivery, if at all? (b) Please supply the
   **code dictionary** for `SECUGRP_ID`/`SECUGRP_NM` and
   `KIND_STKCERT_TP_NM` — the API returns Korean display names only, with the
   underlying codes decoded away. (c) Are the fields delivered as **codes** in
   the paid product, or as display names as in the free API?
5. *(obl. 4)* For a stated date range, what **proportion of issues** in your
   delivery have a complete listing/status/classification state on every trading
   day — and how is a symbol with an incomplete state represented? We treat an
   unestablished state as excluded, so this proportion is a scoring criterion.
6. *(obl. 16)* **— INVERTED by A4. This is now an acquisition question, not a
   rider question.** We have established that **업종 is not returned by
   `stk_isu_base_info` / `ksq_isu_base_info`, and that no service in the KRX
   OPEN API catalogue carries it** (`SECT_TP_NM`/소속부 is a market segment, not
   an industry classification). So: **how is effective-dated 업종 classification
   licensed, at what price, and with what history depth?** Which classification
   scheme, and does the history carry reclassification events with effective
   dates? *We had assumed this rode along at near-zero marginal cost; it does not,
   and we need it priced as its own line.*

**Market state and tradability**

7. *(obl. 5)* **— MOSTLY STRUCK by A5.** The mechanism and the price-limit
   question are settled from KRX's own rulebook: ≤7 trading days, 30-minute
   single-price auctions (14/day at a 09:00–15:30 session), limit orders only,
   and **가격제한폭 is expressly not applied** (「유가증권시장 업무규정」
   제20조제3항). **What remains is only carriage:** is the **정리매매 state
   itself carried as a dated instrument state** in your delivery, with start and
   end dates — and is a 정리매매 print distinguishable from an ordinary
   single-price auction print? *(We ask because 단기과열완화제도 produces the same
   30-minute-auction shape in a stock that is not delisting, so bar shape cannot
   be used to infer the state.)*
8. *(obl. 9)* Does your market-action history carry **halt onset and resumption
   timestamps** and a **last-tradable-date**, per instrument, with reason codes?
9. *(obl. 8)* Are **VI (변동성완화장치)** trigger and release timestamps served,
   and are **static** and **dynamic** VI distinguishable?
10. *(obl. 7)* Does ST5001 or ST5002 identify the **opening and closing call
    auction phases** and any other single-price state? Are **indicative price**
    and **order imbalance** served during those phases, or absent?
11. *(sample composition)* Can the sample include **at least one
    retrospectively-corrected halt or VI record** — an event whose published
    state was later amended?

**Fidelity and reconstruction**

12. *(obl. 10)* **— NARROWED by A2.** Your public product page and the 104-field
    list establish that ST5001 is **order-level (MBO) at millisecond resolution
    with a synchronized 10-level book snapshot on every event**, and that ST5002
    carries order receipt IDs on both sides. We are not asking what it is. **What
    remains:** (a) Confirm the book can be reconstructed such that *"was this
    size reachable at this price at this instant"* is answerable, and **state the
    depth to which that claim is valid** — is the 10-level snapshot the full book
    or a truncation of a deeper one? (b) Are `ORD_ACPT_NO` / `ORGN_ORD_ACPT_NO`
    sufficient to follow a **modify/cancel chain** end to end, and are they
    stable across a delivery? (c) Confirm the **ST5001↔ST5002 join** on receipt
    IDs is supported and lossless. (d) **The 2018-07-16 EOS discontinuity**:
    `BRD_ID`/`SESS_ID`/`MKT_ID` exist only after it and
    `REGUL_OFFHR_TP_CD`/`BLKTRD_TP_CD`/`MKT_TP_CD` are discontinued after it —
    please state exactly what changes at that boundary and what the pre-2018
    equivalents are. *(Your published terms decline to treat pre-2010 definitional
    differences as a defect; we need the boundary documented so we can bound our
    own comparability claim rather than discover it.)*
13. *(obl. 11)* How are **corrections and re-deliveries** versioned? Is a
    corrected file a replacement or a supersession record? Is there a checksum
    and a gap-notification procedure? We need to distinguish a **re-delivery of
    the same facts** from a **restatement of different facts** — they are
    different events for us.
14. *(obl. 14)* **— SHARPENED by A6, which settled the taxonomy but not the
    mapping.** Per product, which **trade categories** are included in a delivered
    bar or aggregate? We reconcile by exact match on integer prices and share
    volumes, so we need the definition, not a tolerance. Specifically, for each
    product: (a) Are **시간외종가매매** prints included? *(They print at exactly
    the close, so they add volume with zero price information.)* (b) Are
    **시간외단일가매매** (16:00–18:00, ±10%) prints included, and how are they
    stamped? (c) Are **장중/시간외 대량·바스켓매매** prints included? *(A block
    can print away from the prevailing quote and manufacture false intraday
    extremes.)* (d) Is **경쟁대량매매** included — and at what timestamp, given it
    has no price until the post-close VWAP is struck? (e) Does daily volume/value
    equal 정규시장 only, 정규시장 + 시간외, or everything? *(We ask because your
    own daily services carry an `AGG_BAS_TP_CD` "aggregation basis type code"
    pinned to `'0'`, with no published value set — which tells us the aggregation
    basis is a real parameterised choice at source. Please supply that code's
    meaning and its permissible values.)* (f) Is a **per-trade category flag**
    available in the product, so we can filter downstream rather than trust
    upstream? *Note we do not ask about odd-lot: A6 established the trading unit
    has been 1 share since 2014-06-02, so no odd-lot category exists.*
15. *(obl. 14)* For **ST1002**: what is the minute-bar boundary convention, are
    auctions included, how is a zero-trade minute represented, what time zone and
    timestamp precision, and are fields unadjusted as-traded?

**Feed (Koscom)**

16. *(obl. 15)* **— STRUCK. A3 settled this from Koscom's own publicly
    downloadable 접속표준서, and the acquisition gate PASSES.** `정보분배일련번호`
    `Int(8)` is field #3 of every 증권A/B/C message on both transports; the UDP
    heartbeat is `Polling Data`/`I2000` on every multicast channel and the TCP
    keepalive is `Link`, explicitly at **1 minute** with fault detection assigned
    to the receiver. **Two refinements remain, both single questions:** (a) What
    is the **numeric emission interval of the UDP `Polling`/`I2000`** message?
    The spec gives `제공 주기: 24-365` and a `1분단위시각` field but no interval.
    (b) The sequence-number rule is annotated `(※ 대용량 서비스에서 제공)` —
    **does the reduced-bandwidth line (증권A 12M / 증권B 12M / 증권C 45M) lose
    per-symbol-per-board sequence granularity?** *(Related, and material to what
    we buy: we understand only the 100M/200M dedicated line carries the full
    unfiltered 10-level book, the others applying 우선호가 상시 필터링. Please
    confirm.)*
17. *(obl. 13)* For each product with a publication schedule — ETF PDF, index
    rebalance, EOD market action — what is the **documented publication
    cadence**? We derive our staleness bound from your declared cadence rather
    than choosing a number, so an undeclared cadence causes us to drop the
    feature.

**Reference instruments**

18. *(obl. 6)* Are historical **ETF Portfolio Deposit File** rows served with
    both an **effective date** and a **publication timestamp**, including
    corrections and terminated products? A relationship we cannot timestamp is
    one we must discard rather than use.
19. *(obl. 6)* For ETN underlying-index membership and weights: does KRX license
    the history, or does the index rights owner? If the latter, whom do we
    approach?

### B.3 — The representative slice to request as a sample

Small enough to be granted, wide enough that a failure shows. Per #243's
acceptance bundle plus M11's addition:

- **Instruments:** ~30 issues spanning mega/mid/small/micro-cap, including at
  least one each of: delisted, renamed, merged, reverse-split, market-transferred,
  newly-listed, REIT, SPAC, foreign-listed, DR, preferred. Include
  **`005930`/`KR7005930003` and `005935`/`KR7005931001`** by name — the repo's
  own preferred-share cross-check pair, and the one that already leaked into a
  backtest.
- **Sessions:** ~20 trading days including a limit-up day, a limit-down day, a
  VI day, a halt-and-resume day, a 정리매매 day, and a corporate-action effective
  day.
- **Products:** ST1002 + ST5001 + ST5002 over the same instrument-session cross
  product, so tick-to-bar reconciliation is runnable on the sample itself.
- **Plus:** one corrected/re-published market-action file, and one ETF PDF
  rebalance with a correction.

### B.4 — Acceptance tests, pre-registered before the sample is viewed

Lifted from #243's reconciliation thresholds, unchanged, because pre-registering
them is the point:

- Daily traded volume and value aggregated from ticks equals KRX daily totals,
  with documented exclusions.
- Bars rebuilt from accepted trades equal delivered ST1002 bars, **exactly** —
  integer prices and share volumes admit no tolerance.
- Quote sequences never create negative sizes or crossed books except in
  documented auction/market states.
- Master membership and status intervals reconcile with KRX official daily states.
- Corporate-action base-price changes reconcile with KRX effective actions and
  OpenDART filings.
- No ETF/ETN relationship carries a publication timestamp later than the
  simulated decision time.

**Any unexplained gap is a failed sample, not a cost-model parameter.**

---

## 5. Part C — Tick Data, in parallel (HITL)

Per **P2**. This is a Tier-1 fallback for the whole-market minute tier only; it
**cannot** displace Part B's millisecond quote/trade slice, because its KRX
product page advertises Level-I only and therefore cannot answer obligation 10.

**A2 widened that gap rather than narrowing it.** ST5001 is not Level II either —
it is **order-level (MBO) with a 10-level book snapshot on every event**. Any
Tier-1 alternative must be assessed against *that* bar, not against "full depth".

**Request:** a complete-market quote for KRX equities, all active **and
inactive** symbols, one-minute OHLCV plus tick trades, as-traded (unadjusted),
with corporate actions and symbol mapping — priced as a complete dataset, not
extrapolated from the symbol-month schedule.

**Written questions:**

1. What is the **actual KRX provenance** — official exchange archive, or a
   direct-feed provider? Which, for which period?
2. Supply the **condition-code dictionary** and confirm **unfiltered** output is
   available; your published default applies proprietary filtering, which we
   cannot accept without seeing what it removes.
3. Which **issue types** are covered — are REIT, SPAC, DR and preferred issues
   present and distinguishable?
4. Supply a **gap report** and the **corrections policy**.
5. *(obl. 2)* Does the symbol mapping carry **effective dates** and cover
   delisted issues?
6. *(obl. 12)* **— now nameable, per A1.** Does the history span these
   established KRX regime changes, and is the pre/post-change data delivered
   consistently across each? **2015-06-15** (daily price limit ±15% → ±30%, and
   static VI introduced); **2016-08-01** (regular-session close 15:00 → 15:30);
   **2019-04-29** (opening-auction order entry 08:00 → 08:30); **2023-01-25**
   (tick-size table revision, which *coarsened* the KOSDAQ 200,000–500,000원 tick
   from 100원 to 500원); and the short-sale regime changes of **2020-03-16**,
   **2021-05-03**, **2023-11-06** and **2025-03-31**. In particular: **what
   session close time do your pre-2016-08-01 daily bars carry**, and is the tick
   grid applied per-regime?
7. *(commercial)* Does the licence permit internal model development, retained
   test artifacts, contractor and cloud access, and **post-subscription
   reproducibility**? Redistribution is presumed forbidden unless stated
   otherwise.

**Note the pricing model difference for the operator:** a one-time complete-dataset
purchase and the TickAPI leased model have materially different post-termination
positions, and obligation 18 bites hardest on the leased one.

---

## 6. Part D — commercial terms (HITL, asked in round one per P3)

**A7 restructured this Part.** It is no longer one conversation with one
counterparty. **KRX does not contract for market data — Koscom does**, except for
bulk historical bought from the KRX Data Marketplace and the index product
licence. So there are **two regimes, they answer the same questions oppositely,
and this package sits on both**:

| | **Route A** | **Route B** |
|---|---|---|
| What | real-time / EOD feed (obl. 15's Koscom 증권A/B/C) | historical / bulk marketdata (the ST/EP products in B.1) |
| Counterparty | **Koscom** — `marketdata@koscom.co.kr` | **KRX** — `krxdata@krx.co.kr` |
| Governing text | *Market Information Policies* + *Terms Definitions* (**published**) + *Terms and Conditions* (**not published**) | **마켓데이터 이용약관**, eff. 2023-10-30, 15 articles (**published**) |
| Derived data | **Original Work** carve-out — work from which the data "cannot be identified, reverse-engineered by automated process or recalculated" **is not "Information" under the agreement**. Definitional, objective. | **The opposite.** Art. 9(2)–(3) bind "마켓데이터 **또는 동 데이터를 가공한 정보**" — derived information is **inside** the restriction. Relief is discretionary only. |

**Route B is the stricter regime and the operative one for historical bars.**
Ask each block of the counterparty that owns it; if a route falls out of scope,
drop its block entire — that is the main remaining way to shrink this
conversation.

**Open Route A by requesting the missing documents.** Every Route A silence below
traces to one cause: termination, survival, return-or-destroy and escrow are
**Terms and Conditions** subjects, and the contract body is the one document
Koscom does not publish. **D0 — please supply the Terms and Conditions (이용조건)
and the Fees Schedule (Annex A).**

**D1 — Content-hash publication (obligation 17, shape-determining).**

*Unchanged in substance; now asked twice, because the two routes answer it
differently and a hash over a dataset spanning both has two contradictory
published answers.*

> We maintain an internal audit register recording that each research
> certification was gated on a specific, immutable dataset. We would like to
> record in that register a **cryptographic content hash** of the licensed data a
> certification ran against — the hash only, never the data, never any price,
> date or instrument identifier derivable from it. The register may be visible to
> parties who do not hold your licence. **Does your agreement permit publishing
> such a hash?** If not, we will keep the register verdict-only, which is our
> current default — we are asking so we can document which we are doing.

- **D1-A (Koscom).** Will Koscom confirm **in writing** that a one-way
  cryptographic digest is **"Original Work"** as defined in the Terms
  Definitions, and therefore "not considered Information under this Agreement"?
  If Koscom will not pre-classify it, **what is the adjudication process, and who
  bears the risk of a later reclassification?** Two sub-points: the Korean §8
  reads **"가공지표 산출 등"** while the English reads **"Creation of Original
  Work"** — **which text governs**, and does the Korean wording carry the same
  one-way carve-out? And does the carve-out reach **end-of-day and historical**
  data, or only Real-time Information?
- **D1-B (KRX, Route B).** Does a one-way digest count as **"구매 마켓데이터의
  가공을 통해 생산한 독창성 있는 저작물"** under Art. 9(5)1? *A digest is
  mechanical and has no creative authorship, so it may fail an originality test
  while being the least disclosive artifact possible.* If it does not qualify, is
  there any other basis on which a hash may be published? And does **Art. 9(3)'s
  bar on providing 가공한 정보 to third parties reach a published content hash**?
- **D1-C (both).** Where one consumer sits on both routes, **which regime governs
  an artifact derived from both?**

*Why it matters here:* a permissive answer upgrades N4's outer reproducibility
tier from **process-auditability** to **input-verifiability**, and lets M5's
machine-enforced verdict-only register carry seal identities. A restrictive
answer leaves N4 and N7 exactly as written. **Both answers are usable; the
contract must not assume the permissive one.** **A7 did not release this — it
remains `shape` under N10, and `may_begin` stays refused.**

**D2 — Post-termination derived-evidence retention (obligation 18,
shape-determining).**

*A7 changed the shape of this question materially. Do not ask it as originally
drafted — its premise ("if the agreement terminates") is **false on Route B**.*

- **D2-A (Koscom, Route A).** The published Policy contains **no** termination,
  expiry, survival, return-or-destroy or retention clause — they live in the
  unpublished Terms and Conditions. So: **What is the contract term and what are
  the termination rights on each side?** **On termination or lapse, is there a
  return-or-destroy obligation, and what exactly does it bite on** — derived
  artifacts, or only "Information" as defined? **Will Koscom grant an express
  post-termination survival right to retain derived audit evidence** — artifacts
  that are not Information and disclose no licensed content — for a stated
  period, for the sole purpose of verifying a historical audit record? Is there
  any **regulatory-retention carve-out**? *(Note the hook: if the evidence
  qualifies as Original Work it is not "Information", and a return-or-destroy
  clause drafted over Information would not on its face reach it. That is an
  argument about the drafting of their own unpublished clause — not a published
  permission.)*
- **D2-B (KRX, Route B).** **The asymmetry is the ask.** Your terms define no
  계약기간, no 해지 and no expiry — it is a purchase. **Will KRX confirm that,
  absent breach, there is no obligation to destroy purchased marketdata or
  derived artifacts at any point?** And: **Art. 11(5) makes the Exchange's
  destruction-demand and record-production powers survive indefinitely, with no
  reciprocal right for the user. Will KRX grant a reciprocal surviving right to
  retain derived audit evidence** — artifacts disclosing no marketdata content —
  **including after a finding of breach**? *As drafted, Art. 11(2) could compel
  destruction of the very evidence needed to resolve the dispute.*
- **D2-C (KRX, Route B).** **What is the 이용계획 (usage plan) and what latitude
  does it allow?** Art. 9 keys every restriction to it, it is completed per
  purchase, and it is not published. Supply the form, and confirm whether
  *"internal quantitative research and backtesting with published hash-only audit
  attestations"* is an acceptable stated purpose.

*Why it matters here:* without a retention right, **every sealed certification
becomes permanently unverifiable the moment the licence lapses.** M20 accepts
that as a recorded risk if the term cannot be had — which is why this needs a
definite answer, not a best effort.

**D3 — Escrow (obligation 19). — ASK ONCE, FOR THE RECORD ONLY.**

> Is a **third-party escrow** arrangement available for the licensed archive,
> such that the evidence base survives loss of our own copy?

**A7 settled this negatively on the published record** — zero hits for 에스크로
and 임치 across all 15 Route B articles and both language versions of Route A's
Policy. Route B has **no approved-third-party mechanism at all**, so external
custody reads as breach on its face; Route A has only the **기술대리인 / Service
Facilitator** route, and Koscom "may withdraw approval" at sole discretion, which
defeats an escrow's purpose. **M5's assumption — no escrow — holds.** #254
already judged this the least likely term to be granted and declined to adopt it
as design. Ask once; **do not negotiate for it.** If Koscom offers the 기술대리인
route, the only follow-up worth making is whether approval can be made
**irrevocable or notice-bound**.

**D4 — Housekeeping and the standard clause list.**

- **(KRX) Is the 마켓데이터 이용약관 of 2023-10-30 still current?** We read it
  from an archived capture of a KRX-served URL that now serves different content.
  **Please confirm the current effective version and supply a stable public URL.**
- **(KRX) Please supply 「마켓데이터 관리지침」 Art. 14**, cross-referenced by
  Art. 8(5) as the sole exception to the no-refund rule — and confirm whether that
  internal directive carries any further retention, destruction or derived-data
  provisions.
- **(Koscom) What are the current fee amounts**, and **which Unit of Count applies
  to a headless, non-display, single-application consumer?** Neither "ID" nor
  "조회요청건" fits; §4(3) allows "a similar basis approved by Koscom". **Does any
  fee component depend on revenue, AUM or turnover?**
- **(Both) Confirm the agreement covers:** internal trading research; storage and
  backup; cloud and contractor access; **derived features**; **audit evidence**;
  and the retention period. KRX distinguishes an **end-user licence** (internal
  institutional use) from a **general licence** (third-party provision) — **we
  want the former** — and publishing a derived **index** requires a separate
  index-calculation agreement, which we do not need and are not requesting.

---

## 7. The AFK / HITL boundary

Stated explicitly so it is not blurred.

| Step | Owner |
|---|---|
| Part A — the free public pass | **Agent. ✅ DONE 2026-08-03.** No licence, no money, no human. |
| Folding Part A's returns into Part B (striking settled questions) | **Agent. ✅ DONE 2026-08-03.** |
| Applying for the two free OPEN API base-info services (§3 A4) | **Human** — an acquisition act, and free. Would settle obligations 3 and 16's completeness before the paid ask. |
| Translating Parts B/C/D into the sent artifact (letterhead, entity name, contacts) | **Agent drafts, human supplies the identifiers.** |
| **Signing and sending** | **Human.** |
| Negotiating price, scope, and the Part D clauses | **Human.** |
| Running the B.4 acceptance tests against a returned sample | **Agent.** |
| Accepting or rejecting a vendor; signing a licence | **Human — and past this map's destination.** |

---

## 8. What this package deliberately does not do

- **It does not decide the primary identity key.** That is
  [#256](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/256)'s, and it
  stays blocked on #255. Obligation 1 produces the evidence; reading a verdict
  off it is a different act.
- **It does not commit to a vendor or sign anything.** Both sit past the map's
  destination.
- **It does not re-decide any closed answer.** Where a sample answer would
  contradict a closed decision — obligations 5, 9 and 12 each can — the package
  routes the contradiction back to its owning ticket as an amendment rather than
  resolving it here.
