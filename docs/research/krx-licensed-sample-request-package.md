# KRX / Koscom licensed-sample information-request package

Authored 2026-08-03 against commit `5c53b45` (`main`, clean tree) to resolve
[Acquire and verify licensed KRX samples for the universe and fidelity contracts](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/255)
on [Wayfinder: production-ready attended ORB portfolio](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/241).

**Status: authored, not sent.** Part A is runnable now by an agent with no
licence and no money. Parts B, C and D require a human to sign and send.

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

## 3. Part A — the free public pass (AFK, runnable now)

Zero cost, zero lead time, no human. Every question here is (a). Running this
first is what makes the paid round trip small. Sources are named because the
point is to check them, not to trust this list.

**A1 — Microstructure regime dates (obligation 12).** Establish, each with an
effective date, from KRX's rulebook, KRX Data Marketplace notices, and KRX
English/Korean market-guide pages:

- **Tick-size table revisions — one is already established in this tree.**
  `TICK_REFORM_DATE = 2023-01-25` at `adapters/nautilus/src/rules.rs:21`, with
  both ladders shipped and an effective-date switch (`TickRegime::for_date`,
  `rules.rs:80-88`) selecting between them. #254's M9 stated that *"none of these
  regime dates is established anywhere in the tree"* — for the tick table that is
  **wrong**, and it is wrong in the direction that matters, because 2023-01-25
  provably falls inside any window starting at #243's 2010-01-04. **Obligation 12
  is therefore already partly answered, and answered affirmatively: at least one
  comparability-relevant regime change sits inside the candidate window.** Public
  confirmation is still worth doing; discovery is not.
- **Continuous-session close time changes** — the 15:00 → 15:30 extension and any
  others. See the finding below; this one is load-bearing twice over.
- VI (변동성완화장치) introduction — static, then dynamic, which arrived separately.
- Short-sale regime changes: the ban, its partial and full lifts, and any
  uptick-rule change.
- Price-limit band width changes (the ±15% → ±30% move and its effective date).

This is the single largest lead-time saving in the package: obligation 12 was
routed as a sample-verification item by both #249 and #254, and **it is almost
entirely public** — one dimension of it is not even external.

> **Finding — the session close time is modelled as time-invariant, and it is
> used to stamp every daily bar.** `KRX_REGULAR_CLOSE` is a flat constant of
> 15:30 KST at `adapters/nautilus/src/rules.rs:37` with **no effective-date
> switch**, while the tick ladder immediately below it has one. It is not
> test-only: production ingest reads it at `adapters/nautilus/src/ingest/mod.rs:629`
> to timestamp a daily bar, at `:974-975` to derive the ingest range bounds, and
> at `:2544` for the watermark. So if KRX's close time changed inside the
> certification window — which is precisely what obligation 12 asks — then every
> daily bar ingested for a pre-change session is stamped at a wall-clock instant
> the market was not closed at, and the error is silent.
>
> This is **not** a defect in today's catalog, whose history is entirely
> post-change; it is a defect that arms the moment history is acquired back to
> 2010-01-04, which is the whole point of this ticket. It belongs to
> [#254](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/254)'s
> acquisition layer, not to this package, and is recorded rather than fixed —
> the map ends before implementation.

**A2 — Product coverage claims (partial to obligations 7, 10, 11, 14).** From the
logged-in KRX historical-product detail pages for **ST1002**, **ST5001**,
**ST5002**, **EP1004**, **EP5001**, **EP5002**: first available date, field
list, delivery method, sample availability, price. #243 established that these
pages exist behind login and expose exactly this. A free KRX membership reaches
them; a purchase is not required.

**A3 — Koscom feed specification (obligation 15).** From the KRX distribution-product
pages and any published Securities A/B/C interface specification: whether the
message headers carry a sequence number, whether a heartbeat message type
exists, and its documented interval. This may fully settle a **pass/fail
acquisition gate** for free.

**A4 — Basic-information service fields (obligations 3, 16).** The downloadable
official specifications for `stk_isu_base_info` (KOSPI) and `ksq_isu_base_info`
(KOSDAQ) already list their returned fields. Check directly whether
**증권구분 / security group** and **업종 / industry classification** are among
them, and whether the `basDd` parameter is documented as returning historical
state. #243 observed the sample returning a 2020-04-14 state, which supports
point-in-time semantics but returned only ten rows — so this establishes
**fields**, never **completeness**.

**A5 — 정리매매 mechanism (obligation 5).** KRX's rulebook documents the
liquidation-trading period: single-price call auctions at 30-minute intervals,
and whether daily price limits are suspended during it. This is published market
structure, not vendor data.

**A6 — Trade-category taxonomy (obligation 14, partial).** KRX publishes its
trade-type and condition taxonomy — negotiated/block (시간외 대량), off-hours
single-price, odd-lot, basket. Establish the taxonomy publicly; only the
**per-product in-scope mapping** needs the vendor.

**A7 — Licence terms as published (obligations 17, 18, 19).** The KRX
data-licence overview distinguishes an **end-user licence** (internal
institutional use) from a **general licence** (third-party provision), and
requires a separate **index-calculation agreement** to publish a derived index.
Koscom's market-data usage policy is a published PDF. Read both for what they
already say about retention, derived features and publication — then Part D asks
only what they do not settle.

---

## 4. Part B — the KRX / Koscom paid request (HITL)

Send after Part A returns. Strike anything Part A settled.

### B.1 — Products in scope

Quote all of the following as one bundle, and separately, so the package
discount is visible:

| Product | Scope needed |
|---|---|
| **ST1002** stock intraday (1-minute) | KOSPI **and** KOSDAQ, **active and inactive** issues, full history to the earliest available date |
| **ST5001** stock quote book (millisecond) | Representative slice — see B.3 |
| **ST5002** stock trade book (millisecond) | Same slice as ST5001 |
| **EP1004 / EP5001 / EP5002** | ETF/ETN mirrors, same shapes, Reference Instruments only |
| KRX instrument-event / market-action history | Full history, effective-dated, with corrections |
| Date-keyed KOSPI/KOSDAQ basic information | From 2010-01-04, full daily cross-section |
| Historical ETF **PDF** rows | With publication timestamps, corrections, inactive products |
| Koscom **KRX Securities A / B / C** | Prospective feed, contracted forward from certification start |

State plainly to the vendor: **the whole-market minute tier and the millisecond
tier are separable purchases.** The minute tier may go to a competitor (Part C);
the millisecond quote/trade slice will not, because it is the only source that
can answer obligation 10.

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
4. *(obl. 3)* Does the delivered master carry **증권구분** (security
   classification) as a coded field, and does it distinguish **REIT, SPAC,
   foreign-listed, DR and preferred** from common stock? Please supply the code
   dictionary.
5. *(obl. 4)* For a stated date range, what **proportion of issues** in your
   delivery have a complete listing/status/classification state on every trading
   day — and how is a symbol with an incomplete state represented? We treat an
   unestablished state as excluded, so this proportion is a scoring criterion.
6. *(obl. 16)* Do the date-keyed basic-information services carry **effective-dated
   업종 (industry) classification**, and is it included at no additional charge?

**Market state and tradability**

7. *(obl. 5)* During **정리매매** (liquidation trading), what is the trading
   mechanism, and do daily price limits apply? Is the 정리매매 state itself
   carried as a dated instrument state in your delivery?
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

12. *(obl. 10)* Is **ST5001** a top-of-book snapshot, a market-by-price
    incremental stream, or order-level? To what **depth** is it served? Can the
    book be reconstructed such that *"was this size reachable at this price at
    this instant"* is answerable — and if so, to what depth is that claim valid?
13. *(obl. 11)* How are **corrections and re-deliveries** versioned? Is a
    corrected file a replacement or a supersession record? Is there a checksum
    and a gap-notification procedure? We need to distinguish a **re-delivery of
    the same facts** from a **restatement of different facts** — they are
    different events for us.
14. *(obl. 14)* Per product, which **trade categories** are included in a
    delivered bar or aggregate — off-hours, negotiated/block, odd-lot, auction?
    We reconcile by exact match on integer prices and share volumes, so we need
    the definition, not a tolerance.
15. *(obl. 14)* For **ST1002**: what is the minute-bar boundary convention, are
    auctions included, how is a zero-trade minute represented, what time zone and
    timestamp precision, and are fields unadjusted as-traded?

**Feed (Koscom)**

16. *(obl. 15)* Do the **KRX Securities A/B/C** feeds carry **sequence numbers**
    on every message, and is there a **heartbeat** message with a documented
    interval? *We must state plainly that a feed carrying neither cannot meet our
    staleness requirement at all — an event stream is silent when nothing
    happens, so elapsed time alone cannot distinguish a quiet market from a dead
    feed.*
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
6. *(obl. 12)* Does the history span the microstructure regime changes we
   identify in Part A, and is the pre/post-change data delivered consistently?
7. *(commercial)* Does the licence permit internal model development, retained
   test artifacts, contractor and cloud access, and **post-subscription
   reproducibility**? Redistribution is presumed forbidden unless stated
   otherwise.

**Note the pricing model difference for the operator:** a one-time complete-dataset
purchase and the TickAPI leased model have materially different post-termination
positions, and obligation 18 bites hardest on the leased one.

---

## 6. Part D — commercial terms (HITL, asked in round one per P3)

Address to whoever reviews the end-user agreement. Framed as compliance and
audit, which is what they are.

**D1 — Content-hash publication (obligation 17, shape-determining).**

> We maintain an internal audit register recording that each research
> certification was gated on a specific, immutable dataset. We would like to
> record in that register a **cryptographic content hash** of the licensed data a
> certification ran against — the hash only, never the data, never any price,
> date or instrument identifier derivable from it. The register may be visible to
> parties who do not hold your licence. **Does your end-user agreement permit
> publishing such a hash?** If not, we will keep the register verdict-only, which
> is our current default — we are asking so we can document which we are doing.

*Why it matters here:* a permissive answer upgrades N4's outer reproducibility
tier from **process-auditability** to **input-verifiability**, and lets M5's
machine-enforced verdict-only register carry seal identities. A restrictive
answer leaves N4 and N7 exactly as written. **Both answers are usable; the
contract must not assume the permissive one.**

**D2 — Post-termination derived-evidence retention (obligation 18,
shape-determining).**

> If the agreement terminates, we understand licensed raw data must be deleted.
> We need a carve-out permitting indefinite retention of **derived audit
> evidence** — research outputs, verification records, and the metadata proving
> what a past decision was based on — for regulatory and internal-audit purposes.
> We are not asking to retain the raw data or anything from which it could be
> reconstructed. **Can such a clause be included?**

*Why it matters here:* without it, **every sealed certification becomes
permanently unverifiable the moment the licence lapses.** M20 accepts that as a
recorded risk if the term cannot be had — which is why this needs a definite
answer, not a best effort.

**D3 — Escrow (obligation 19).**

> Is a **third-party escrow** arrangement available for the licensed archive,
> such that the evidence base survives loss of our own copy?

*Expected answer is no* — #254 weighed escrow, judged it the least likely term a
KRX or Koscom end-user agreement grants, and declined to adopt it as design
while routing it as a question. Ask once; do not negotiate for it.

**D4 — The standard clause list (from #243, unchanged).** Confirm the agreement
covers: internal trading research; storage and backup; cloud and contractor
access; **derived features**; **audit evidence**; and the retention period. Note
that KRX distinguishes an **end-user licence** (internal institutional use) from
a **general licence** (third-party provision) — we want the former — and that
publishing a derived **index** requires a separate index-calculation agreement,
which we do not need and are not requesting.

---

## 7. The AFK / HITL boundary

Stated explicitly so it is not blurred.

| Step | Owner |
|---|---|
| Part A — the free public pass | **Agent.** No licence, no money, no human. |
| Folding Part A's returns into Part B (striking settled questions) | **Agent.** |
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
