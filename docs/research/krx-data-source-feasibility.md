# Feasible KRX data sources for point-in-time universe and two-tier simulation

Research date: 2026-08-01 (Asia/Seoul)

## Decision

A production-grade evidence base is feasible, but no single public or broker API
meets the whole requirement. Use a licensed, layered source stack:

1. **Canonical exchange history:** procure KRX stock intraday `ST1002`
   (`1-minute`/`10-minute`), quote-book `ST5001`, and trade `ST5002`, plus the
   associated instrument-event and market-action history. Procure matching
   ETF/ETN intraday `EP1004`, quote `EP5001`, and trade `EP5002`, together with
   historical relationship files needed for reference features. KRX is the only
   source found that advertises both whole-market minute bars and millisecond
   stock quote/trade history directly from the exchange.
2. **Prospective truth and calibration:** contract for the KRX market-data feeds
   through Koscom and archive the KOSPI, KOSDAQ, and ETF/ETN streams from the
   certification start date. These feeds include stock trades, ten quote levels,
   off-hours data, end-of-day instrument events, and market-action information.
3. **Point-in-time master and cross-checks:** use KRX's date-keyed KOSPI/KOSDAQ
   basic-information APIs from 2010-01-04, licensed KRX status/event history,
   and OpenDART filings. Preserve each version and its observation/publication
   time; do not overwrite history with a vendor's latest view.
4. **Fallback for broad history:** if the KRX minute-history quote is materially
   uneconomic, buy Tick Data's complete KRX-equities history for Tier 1. It
   advertises all active and inactive symbols, one-minute OHLCV, Level-I quotes,
   trades, corporate actions, and symbol mapping since 2008-05-07. It is not a
   substitute for KRX's Level-II/auction acceptance sample.
5. **Broker API only as a witness:** LS `t8412` is suitable for spot checks and
   incremental gap repair, not initial whole-market acquisition. It is
   per-symbol, paginated, and currently paced at one request per second in this
   repository.

The defensible common start for the first certified broad simulation is
**2010-01-04**, because that is the documented start of KRX's date-keyed KOSPI
and KOSDAQ master and daily-trading APIs. A longer price history can be useful,
but it is not point-in-time-universe certified until equivalent pre-2010 master,
status, and action history is procured and accepted.

This decision is conditional on a paid-data acceptance test. KRX's public
catalog proves that products exist, but their detailed historical coverage,
book schema, auction flags, corrections, prices, and delivery terms are behind
login or contract review. Procurement must not be treated as certification.

## Requirement-to-source matrix

| Requirement | Canonical source | Cross-check / fallback | As-of integrity and unresolved gate |
| --- | --- | --- | --- |
| KOSPI/KOSDAQ membership, listings, security type | KRX `stk_isu_base_info` and `ksq_isu_base_info`, queried by base date; licensed KRX security master | FSC KRX-listed-instrument API; Tick Data active/inactive symbol map | KRX documents coverage from 2010-01-04. Verify production returns the complete daily cross-section and archive every response; the public sample is capped and is not a completeness witness. |
| Delistings and identifier changes | Effective-dated KRX master/event archive | Tick Data Company Info/Ticker Mapping; OpenDART filings | Derive membership only from dated states, never from today's symbol list. Require explicit delisting/effective dates and stable ISIN/issue identifiers in the purchased delivery. |
| Suspensions, administrative issues, investment-alert/surveillance states | Licensed KRX market-action history and EOD market-action feed | KRX Data Marketplace suspension, administrative-issue, investment-caution and market-alert screens | Public screens prove the categories exist, not a revision-safe bulk interface. Require start/end/effective timestamps, reason codes, corrections, and complete historical coverage in the contract/sample. |
| Corporate actions and adjustment factors | KRX instrument events, base-price/action data, and exchange effective dates | OpenDART original/amended filings; Tick Data corporate actions | Keep raw as-traded prices immutable and adjustments in a separate versioned table. A filing/announcement date is not automatically the exchange effective date. |
| Whole-market stock minute bars | KRX Stock Intraday Trading Information (`1m`, `10m`) | Tick Data complete-market one-minute bars; LS `t8412` only for spot checks | Require KOSPI and KOSDAQ, active and inactive issues, regular/open/close auction treatment, empty-minute policy, time zone, corrections, and unadjusted fields. |
| ETF/ETN reference prices and microstructure | KRX Securities Products daily/intraday/quote/trade products; KRX Securities-C feed | KRX Open API daily ETF (from 2010-01-04) and ETN (from 2014-11-17) series | Reference instruments remain non-tradable. Point-in-time features must be timestamped no later than the stock decision time. |
| ETF-to-stock relationship | Historical daily ETF Portfolio Deposit File (PDF) and relevant index constituent history, licensed from KRX/fund or index owner | KRX public ETF PDF and index-constituent screens | A PDF is a creation/redemption basket, not necessarily the exact portfolio or index. Require effective date, constituent identifier, quantity/weight, cash/derivative rows, publication time, corrections, and inactive products. |
| ETN-to-stock relationship | Historical underlying-index membership/weights from the index owner, linked through KRX ETN master | KRX ETN underlying-index constituent screen and issuer disclosures | ETNs are index-linked unsecured notes, not asset portfolios. Index-provider rights and history are product-specific; do not infer holdings from issuer hedges. |
| Representative tick/trade history | KRX Stock Trade Book and Securities Product Trade Book (millisecond) | Tick Data tick trades since 2008-05-07 | Require condition codes, cancellations/corrections, auction phase, odd/block/off-hours classification, sequence semantics, and clock definition. |
| Representative quote/order-book and auction history | KRX Stock Quote Book and Securities Product Quote Book (millisecond), plus prospective KRX/Koscom feed | Tick Data Level-I quotes for top-of-book only | Public descriptions do not prove full depth reconstruction, order-level queue data, or auction imbalance fields. The acceptance sample must prove the exact fidelity claimed or the simulator claim must be narrowed. |

## Why direct KRX history is the canonical choice

KRX's historical stock catalog explicitly sells:

- daily trading data;
- intraday prices at one- and ten-minute intervals;
- stock quote information at millisecond resolution; and
- stock executions at millisecond resolution.

It also offers a stock package at a stated 10% package discount. The matching
securities-product catalog offers daily, one-/ten-minute, millisecond quote, and
millisecond trade products for ETFs, ETNs, and ELWs
([KRX stock historical-product catalog](https://data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd?prodType=ST&viewNm=dataProdList),
[KRX securities-product historical catalog](https://data.krx.co.kr/contents/MDC/DATA/datasale/index.cmd?prodType=EP&viewNm=dataProdList)).

KRX says its purchase service provides processed market price and trading data.
After payment and purpose review, delivery is by email or web download depending
on size. Logged-in product details expose basic specification, coverage period,
fields, delivery method, samples, and price. Academic/public-interest buyers can
receive a 50% discount, which does not apply to this production use unless the
buyer independently qualifies
([KRX data purchase guide](https://openapi.krx.co.kr/contents/OPP/DATA/OPPDATA001.jsp)).

The public catalog does **not** establish:

- the first available date for each paid product;
- whether `Quote Book` is snapshot, incremental market-by-price, or order-level;
- whether pre-open and closing-auction imbalance/indicative prices are present;
- whether trade condition, cancellation, correction, VI, price-limit, block,
  odd-lot, and off-hours events are separable;
- whether minute bars include auctions and how empty minutes are represented;
- how corrections and re-deliveries are versioned; or
- exact commercial price and retention/derived-data rights.

Those are acceptance questions, not details to guess from the product names.

## Prospective KRX/Koscom feed

KRX's distribution catalog separates KOSPI (`Securities A`), KOSDAQ/KONEX
(`Securities B`), and ETF/ETN/ELW (`Securities C`). It says stock real-time data
includes executions, ten levels of quotes, off-hours trading, program trading,
short selling, foreign trading, market indices, and member buy/sell data. Its
end-of-day feed includes closing values and instrument events; separate EOD
reference feeds include KOSPI/KOSDAQ market-action information
([KRX distribution products](https://openapi.krx.co.kr/contents/OPP/DATA/OPPDATA002.jsp)).

This feed should become the prospective calibration source because it can be
captured exactly as observed before each trading decision. It is not a shortcut
to back history. KRX directs professional subscribers and vendors to contract
through Koscom, requires KRX approval, and describes automatic annual renewal
after the initial contract year
([KRX/Koscom ordering process](https://openapi.krx.co.kr/contents/OPP/DATA/OPPDATA003.jsp)).

For internal research and trading, request an **end-user license**. KRX describes
that license as internal institutional use; a general license is the path for
third-party provision. Creating or publishing a derived index requires a
separate index-calculation agreement and advance approval
([KRX data-license overview](https://openapi.krx.co.kr/contents/OPP/DATA/OPPDATA004.jsp)).
Koscom's usage policy makes the usage terms part of the contract and imposes
approval/reporting controls on feeds and downstream users
([Koscom market-data usage policy](https://data.krx.co.kr/inc/datasale/Market%20Data%20Usage%20Polices_ko.pdf)).

Therefore raw KRX data, samples, and licensed relationship files must live in
access-controlled object storage, never in this public repository. The contract
must explicitly settle retention after termination, cloud/contractor access,
backup copies, model-derived features, publication of aggregate research, and
whether reproducibility hashes/metadata may be public.

## Point-in-time universe, status, and actions

### KRX date-keyed master

KRX documents KOSPI and KOSDAQ basic-information APIs from 2010-01-04
([KOSPI basic-information service](https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=PiwgMdTwmsenXhmqqxuj),
[KOSDAQ basic-information service](https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=CifLHplnUFMgpHIMMPXs)).
The downloadable official specifications take a `basDd` and return issue/short
codes, Korean/English names, listing date, market, security group, section,
stock-certificate kind, par value, and listed shares. The official sample was
also observed on the research date returning an historical 2020-04-14 state,
which supports the point-in-time interface semantics; it returned only ten rows,
so it cannot certify production completeness.

These APIs are feasible for a daily point-in-time security master, but only
under strict controls:

- obtain a production key and approval for both services;
- prove full-market row counts against a licensed daily master for a sample of
  normal, listing, delisting, and holiday dates;
- store the requested base date, acquisition time, endpoint/version, response
  hash, row count, and raw response in the licensed archive;
- treat a missing/empty/short response as `Unknown`, not an authoritative empty
  universe; and
- diff later re-fetches rather than silently replacing the first-seen snapshot.

KRX Open API is not a redistributable production-data license. The terms limit
use to non-commercial purposes, cap a key at 10,000 requests per day, prohibit
providing received information to third parties, and prohibit use after the
agreement ends. KRX also disclaims accuracy, completeness, and continued
availability
([KRX Open API terms, articles 6, 8, 11, and 12](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp)).
Access requires membership, an approved key, then separate service approval
([KRX Open API procedure](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO003.jsp)).
Obtain written confirmation that internal trading research and retained
historical snapshots are allowed, or acquire the same master under the paid
end-user agreement.

The Financial Services Commission's KRX listed-instrument API is a useful
independent current-state witness. It is free, automatically approved, limited
to 10,000 development calls (higher production traffic can be requested), and
states unrestricted reuse. It is updated once per business day after receiving
data from the source institution. Its published fields are base date, short
code, ISIN, market, name, corporate-registration number, and corporation name;
the page does not promise a complete immutable history or delisting/status
events, so it cannot be the canonical point-in-time universe
([FSC KRX listed-instrument API](https://www.data.go.kr/data/15094775/openapi.do)).

### Suspensions and surveillance states

KRX Data Marketplace exposes official screens for:

- all-instrument designations;
- trading-suspension status and per-instrument history;
- administrative-issue status and per-instrument history;
- KOSDAQ investment-caution designations; and
- per-instrument market-alert designations.

It also exposes volatility-interruption and other issue statistics
([KRX Data Marketplace menus](https://data.krx.co.kr/contents/MDC/MAIN/main/index.cmd)).
These screens establish that KRX maintains the necessary classifications, but
the public site does not document a stable bulk API, full coverage start,
publication timestamp, revision history, or redistribution right. Use them for
discovery and sample reconciliation. Require a licensed, bulk, effective-dated
delivery—preferably the EOD market-action feed plus back history—before the
Tradable Universe can exclude an instrument without look-ahead.

### Corporate actions

KRX adjusts the trading base price for actions including rights/new-share
issues, bonus issues, stock dividends, splits, and reverse splits
([KRX base-price rules](https://global.krx.co.kr/contents/GLB/06/0602/0602010201/GLB0602010201T6.jsp)).
That exchange-effective event/base-price history should own simulation
adjustments.

OpenDART is a valuable second witness. The Financial Supervisory Service allows
individuals, companies, and institutions to use disclosure originals and
structured major-report data through its API
([OpenDART introduction](https://opendart.fss.or.kr/intro/main.do)). Its
structured major-event APIs include splits and other corporate actions with
filing date ranges, generally from 2015, and provide a filing receipt number
that links to the original disclosure
([OpenDART split-decision API](https://opendart.fss.or.kr/guide/detail.do?apiGrpCd=DS005&apiId=2020051),
[OpenDART disclosure API catalog](https://opendart.fss.or.kr/guide/main.do?apiGrpCd=DS001)).
The service is normally free and authenticated; it has a posted, changeable
usage allowance and can interrupt service. FSS does not warrant submitter data's
accuracy or completeness
([OpenDART terms](https://opendart.fss.or.kr/intro/terms.do)).

Use the filing receipt/publication timestamp and amendment chain to model what
was known, but do not turn a board decision or filing date into a split factor or
tradability change until KRX's effective event confirms it.

## ETF and ETN reference instruments

KRX states that ETF transparency information includes the Portfolio Deposit
File, NAV, index components, tracking error, and management reports
([KRX ETF disclosure information](https://global.krx.co.kr/contents/GLB/06/0605/0605010101/GLB0605010101T2.jsp)).
KRX Data Marketplace exposes ETF PDF and ETN underlying-index constituent
screens, as well as ETF/ETN basic, price, liquidity-provider, and deviation
statistics
([KRX ETF/ETN menu](https://data.krx.co.kr/contents/MDC/MAIN/main/index.cmd)).

The semantic distinction matters:

- an **ETF PDF** is the basket used for creation/redemption as of its
  announcement day; KRX describes in-kind creation/redemption by that daily PDF
  ([KRX ETF issuing process](https://global.krx.co.kr/contents/GLB/03/0303/0303090203/GLB0303090203.jsp));
- an **ETF's tracked index** can have components different from the practical
  creation basket, and synthetic/cash-created products may not expose stock
  ownership through the PDF; and
- an **ETN** promises the return of an underlying index and is unsecured. KRX
  requires an index-use agreement and real-time availability of index
  information; the issuer's hedge is not the ETN's constituent portfolio
  ([KRX ETN underlying-index requirements](https://global.krx.co.kr/contents/GLB/03/0303/0303100100/GLB0303100100.jsp)).

For stock reference features, acquire two effective-dated relationship tables:

1. `reference_instrument -> underlying_index -> stock constituent/weight`, from
   the index rights owner; and
2. `ETF -> PDF row/quantity/cash/derivative`, from the fund/KRX source.

Both need publication time, effective date, revision identifier, stable ISIN,
inactive/delisted instruments, and a license for automated internal use. For a
non-KRX index, the index provider—not KRX—may own the historical composition
rights. If a licensable history is unavailable, that ETF/ETN cannot generate a
constituent-level feature in certified history; it may still contribute an
aggregate reference return known at the decision timestamp.

ETF/ETN reference prices can come from KRX's Securities Products one-minute or
millisecond products and the prospective Securities-C feed. The Open API daily
catalog starts ETF data on 2010-01-04 and ETN data on 2014-11-17
([KRX Open API service list](https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd)).
Nothing in this design makes ETFs or ETNs tradable by the strategy.

## Tick Data fallback

Tick Data's own KRX product page advertises all KRX-listed equities since
2008-05-07 with:

- millisecond tick trades and Level-I bid/ask quotes with size;
- prebuilt one-minute OHLCV;
- corporate-action data and symbol mapping;
- orders by complete market/year or selected symbols/date ranges;
- active and inactive symbols to reduce survivorship bias; and
- as-traded files, with optional mapped/adjusted time series
  ([Tick Data KRX equities](https://www.tickdata.com/equity-data/korea-exchange-equities)).

This is a credible Tier-1 fallback because its all-symbol delivery directly
addresses delisted instruments and its raw-as-traded form avoids forced
back-adjustment. It is not exchange-authored data: Tick Data says it obtains
official exchange archives where possible and otherwise uses direct-feed
providers, then applies proprietary validation and default filtering. Require
the actual KRX provenance, condition-code dictionary, unfiltered output, issue
type coverage, gap report, corrections policy, and a sample that reconciles to
KRX before acceptance.

It supplies only Level I for KRX on the cited page, so it cannot certify a
ten-level order book, queue position, or opening/closing auction state. Keep the
representative KRX quote/trade purchase even if Tick Data wins the broad-minute
procurement.

Pricing is material. Tick Data publishes a $1,000 minimum for a new one-time
buyer and $500 for returning buyers, with complete-market and symbol-year
pricing quoted through its estimator
([Tick Data fee estimate](https://www.tickdata.com/fee-estimate)). TickAPI uses a
leased-data model with an active subscription, a one-year minimum, at least
$250/month, an annual support fee, and tiered symbol-month charges
([TickAPI pricing and license model](https://www.tickdata.com/data-delivery/tickapi)).
For a whole-market multi-year backtest, request a complete-dataset quote rather
than extrapolating the low-volume API schedule. The contract must explicitly
permit internal model development, retained test artifacts, contractors/cloud,
and post-subscription reproducibility; redistribution must be presumed forbidden
until the contract says otherwise.

## Why the LS API is not the initial data lake

This repository's official-capture-backed `t8412` implementation is a
per-symbol paginated N-minute chart call. The current public recommendation is
only a paper, single-page, one-symbol round trip and explicitly does not claim
multi-page correctness, halt/VI correctness, or outside-session behavior
([local `t8412` reference](../reference/t8412.md)). The captured policy is paced
at one request per second. At that rate, one request for each of roughly 2,500
symbols already takes about 42 minutes before pagination, retries, multiple
dates, or validation.

Use LS for:

- daily spot checks against the canonical minute archive;
- a bounded recent gap repair after completeness comparison;
- paper/live shadow calibration using the same broker path as execution; and
- prospective observations for fields not purchased elsewhere.

Do not use it to infer delisted membership, historical surveillance state,
whole-market completeness, or historical Level-II/auction state.

## Paid-data acceptance test

No source becomes certification-grade until a fixed acceptance bundle passes.
Request samples before committing to full history.

### Coverage sample

Include KOSPI and KOSDAQ common stocks across:

- mega-, mid-, small-, and micro-cap/liquidity buckets;
- current, delisted, renamed, merged, split, and newly listed issues;
- normal, suspended, administrative, investment-alert, VI, limit-up/down, and
  no-trade sessions;
- ordinary sessions plus known opening/closing-auction and off-hours trades; and
- at least one corporate-action effective day for every action type used in
  adjustment logic.

Add ETFs and ETNs with domestic-equity, foreign, leveraged/inverse, synthetic,
cash-created, and terminated products. For relationships, include a
constituent rebalance and a corrected/re-published file.

### Mechanical acceptance

The supplier must provide or the sample must prove:

1. stable issue identifiers and point-in-time symbol mapping;
2. documented market, board, session, time zone, timestamp precision, and
   sequence ordering;
3. quote depth/message semantics sufficient to reconstruct the exact book claim;
4. auction-phase and indicative/imbalance semantics, or an explicit absence;
5. trade condition, cancel, correction, off-hours, negotiated/block, and odd-lot
   flags;
6. minute-bar boundary, auction inclusion, zero-trade, volume/value, and
   adjustment rules;
7. complete active **and inactive** universe plus list/delist/status intervals;
8. revision, correction, replacement-file, checksum, and gap-notification
   procedures;
9. historical ETF PDF/index membership with publication/effective timestamps;
10. a license covering internal trading research, storage, backup, cloud,
    derived features, audit evidence, and the required retention period.

### Reconciliation thresholds

Pre-register the tests before viewing the full history:

- daily traded volume/value aggregated from ticks equals KRX daily totals, with
  documented exclusions;
- bars rebuilt from accepted trades equal delivered one-minute bars;
- quote sequences never create impossible negative sizes or crossed books except
  documented auction/market states;
- master membership and status intervals reconcile with KRX official daily
  states;
- split/base-price changes reconcile with KRX effective actions and OpenDART
  filings; and
- ETF/ETN stock links never use a relationship published after the simulated
  decision time.

Any unexplained gap is a failed sample, not a cost-model parameter.

## Reproducibility contract for the local data lake

Store three time axes instead of a mutable latest view:

- `effective_at`: when the market fact applies;
- `published_at` or `available_at`: when the strategy could first know it; and
- `observed_at`: when this system acquired that version.

Each immutable source object should record supplier, product/schema version,
contract dataset identifier, request/order parameters, delivery timestamp,
byte length, cryptographic hash, row count, min/max event time, and parser
version. Corrections append a new object and supersession link. They never
rewrite the evidence used by an earlier run.

Research manifests should identify exact raw-object hashes, universe/status
snapshot versions, relationship versions, action-factor versions, bar-builder,
simulator, and cost-model versions. Raw licensed bytes remain private; the
public repository may contain schemas, code, hashes, counts, and aggregate
results only to the extent the contract permits.

## Cost and operational constraints

| Source | Material constraint |
| --- | --- |
| KRX historical purchase | Product details and price require login; purpose review occurs after payment. Full-market millisecond quote/trade history can be large. Buy a representative high-fidelity slice first, then size storage/compute from measured rows and compressed bytes per symbol-session. |
| KRX/Koscom feed | Contract, KRX approval, feed engineering, monitoring, gap recovery, entitlement/reporting, and continuous archival operations. It builds history only from capture start. |
| KRX Open API | One-year access, separate approvals, 10,000 calls/day/key, restrictive non-commercial/no-third-party/post-termination terms, and no completeness/continuity warranty. |
| KRX relationship/index data | ETF PDFs may be available from KRX/funds, while non-KRX index composition can require a separate owner license. Historical publication timestamps and inactive products are the hard part. |
| Tick Data | Quote-based complete-market cost; one-time minimums or a leased one-year TickAPI subscription. Proprietary cleaning means unfiltered source delivery and reconciliation are required. |
| OpenDART | Free by default, authenticated and rate-limited; structured action endpoints generally begin in 2015 and filings can be amended. It is evidence of disclosure, not exchange-effective market state. |
| LS broker API | Per-symbol pagination and pacing make whole-history ingestion operationally slow; paper evidence does not establish full historical or market-state completeness. |

## Procurement outcome required before architecture is locked

Send one RFP to KRX/Koscom covering the exact product IDs and one to Tick Data
for the alternative Tier-1 bundle. Require sample data and written answers to
the acceptance test above. Choose between these two implementable shapes:

- **Preferred:** KRX full-market one-minute history + KRX representative
  millisecond quote/trade history + KRX master/status/actions/relationships.
- **Cost fallback:** Tick Data complete-market one-minute/as-traded history + KRX
  representative millisecond quote/trade history + the same KRX
  master/status/actions/relationships.

Do not approve a minute-only vendor without inactive symbols and point-in-time
master/status evidence. Do not approve a Level-I vendor as the high-fidelity
order-book tier. Do not claim auction realism until the KRX sample proves the
auction fields and reconstruction semantics. If no licensable historical ETF or
ETN constituent relationship exists, remove constituent-level ETF/ETN features
from the certification spec rather than reconstructing them with hindsight.
