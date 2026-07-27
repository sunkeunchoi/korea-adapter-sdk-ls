# Official API witness for historical KRX equity sessions

Research date: 2026-07-18 (Asia/Seoul)

## Verdict

KRX's **유가증권 일별매매정보** (Securities Market daily trading
information) API is a strong **positive historical-session witness** from
2010-01-04 onward:

- production endpoint:
  `GET https://data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd?basDd=YYYYMMDD`
- request header: `AUTH_KEY: <approved key>`
- a non-empty `OutBlock_1` whose rows carry the requested `BAS_DD` proves KRX
  published domestic-equity market records for that date.

It is **not sufficient by itself to classify an empty result as a market
closure**. KRX's public sample returns the identical `{"OutBlock_1":[]}` for a
known holiday, a date before coverage, an invalid calendar date, a future date,
and an omitted date. Authentication and unknown-endpoint failures do have
explicit `respCode`/`respMsg` objects, but the documented interface supplies no
status or completeness marker that distinguishes every legitimate empty
non-session result from bad input or missing data.

Therefore use this endpoint to establish **TradingSession** facts, but do not
derive **Closed** merely from an empty array. A complete historical calendar
needs either a separate official closure/calendar source or a refresh protocol
that validates the requested date, coverage, response shape, and expected
cross-source completeness before interpreting absence.

There is also a licensing blocker for a public, committed snapshot: KRX's terms
restrict the API to non-commercial use, prohibit providing received information
to third parties, and prohibit continued use of received information after the
use agreement ends. The terms do not say whether an open/closed date set derived
from daily rows may be redistributed. Obtain a written KRX interpretation or a
compatible licence before committing such a derived snapshot to this public
repository.

## Endpoint identity, authority, and provenance

The operator is Korea Exchange itself. KRX describes its Open API as an
interface through which KRX Data Marketplace statistical information can be
used in web and mobile applications, and defines an “API service” as a service
provided by KRX from information on KRX Data Marketplace
([service introduction](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO001.jsp),
[terms, articles 2 and 10](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp)).
No secondary provenance claim is needed.

The official service catalog names **유가증권 일별매매정보**, describes it as
trading information for shares listed on the Securities Market, and states that
data is provided from 2010-01-04
([service catalog](https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd)).
The service detail page gives API ID `stk_bydd_trd`, exposes the official sample
endpoint, and provides a downloadable development specification. That
specification names the production endpoint shown above
([service detail and specification download](https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=JvJFzlAENzZlPBDNGAWC)).
The detail page reports registration on 2020-09-22 and last modification on
2026-01-16; these are service-metadata dates, not data-coverage dates.

This endpoint covers the **유가증권시장** (KOSPI/Securities Market), which is
enough to witness the project's domestic-equity regular-session date when rows
exist. KRX separately lists **코스닥 일별매매정보**. A production refresh may
query both as a consistency check, but no official source found in this
research states that dual empty results constitute an authoritative closure.

## Access, approval, cost, and terms

Access is practical for an ordinary maintainer but is neither anonymous nor
automatic. KRX documents this sequence:

1. join and log into Data Marketplace (an individual may use identity
   verification or social login);
2. request an authentication key and wait for administrator approval;
3. apply to use the desired API and wait for administrator approval;
4. call the API after approval
   ([official usage procedure](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO003.jsp)).

The key is passed in the `AUTH_KEY` request header. The terms set the API-use
period at one year, renewable in one-year increments, and permit KRX to reject
applications or renewals. The service application UI offers 1, 3, 6, or 12
months for a particular service. KRX says service is normally available 24/7,
subject to announced maintenance and service-specific hours
([terms, article 5](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp),
[service detail](https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=JvJFzlAENzZlPBDNGAWC)).

No fee schedule or explicit “free of charge” promise was found on the official
service page, usage procedure, catalog, or terms, so the research cannot assert
that access is contractually free. The application UI does not present a price.
More importantly, the published terms require non-commercial use, forbid
charging third parties for API results, forbid providing received information
to third parties, require attribution as “한국거래소 통계정보” on screens made
from results, and disallow use of received information after expiration or
termination
([terms, articles 6, 10, and 11](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp)).

## Request and response contract

The official downloadable specification documents one request field:

| Field | Type | Meaning |
| --- | --- | --- |
| `basDd` | string | 기준일자 (base/reference date) |

The UI constrains the sample value to eight characters and illustrates a GET
request. Neither the detail page nor downloaded specification explicitly states
the date grammar, so `YYYYMMDD` is the demonstrated convention rather than a
documented validation guarantee
([service detail](https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=JvJFzlAENzZlPBDNGAWC)).

The JSON response has a repeating `OutBlock_1`. Its documented fields are:

`BAS_DD`, `ISU_CD`, `ISU_NM`, `MKT_NM`, `SECT_TP_NM`, `TDD_CLSPRC`,
`CMPPREVDD_PRC`, `FLUC_RT`, `TDD_OPNPRC`, `TDD_HGPRC`, `TDD_LWPRC`,
`ACC_TRDVOL`, `ACC_TRDVAL`, `MKTCAP`, and `LIST_SHRS`. For calendar acquisition,
the required positive-witness checks are:

1. HTTP success and the expected JSON top-level shape;
2. non-empty `OutBlock_1`;
3. every accepted row's `BAS_DD` equals the requested date;
4. at least one row identifies `MKT_NM` as `KOSPI`.

The API is a daily cross-section, not a date-range endpoint. There is no page,
cursor, offset, limit, total-count, revision, or as-of input/output in the
official specification. The public **sample** returns ten rows for a valid date;
KRX does not document whether that sample cap applies to production, nor does it
publish a production row ceiling on the service page. Consequently a calendar
refresh should inspect only the positive-witness invariants above and must not
assume a particular row count.

KRX limits a key to at most 10,000 requests per calendar day and may stop the
service when the limit is exceeded. It may separately restrict scope, hours, or
request counts
([terms, article 8](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp)).
One request per civil date from 2010-01-04 through mid-2026 fits under that daily
limit, but retries and any additional endpoint cross-checks must share the same
budget.

## Observed official sample behavior

The following calls were made on 2026-07-18 against the sample endpoint and
sample key published directly in the KRX service detail page. These observations
are reproducible API evidence, not promises in the written specification.

| Request case | HTTP / JSON result |
| --- | --- |
| `basDd=20100104` (documented first coverage date) | HTTP 200; ten sample rows; first `BAS_DD` is `20100104` |
| `basDd=20100101` (before documented coverage) | HTTP 200; `{"OutBlock_1":[]}` |
| `basDd=20100301` | HTTP 200; `{"OutBlock_1":[]}` |
| `basDd=20100302` | HTTP 200; ten sample rows; first `BAS_DD` is `20100302` |
| `basDd=20221230` (year-end closure under KRX's published holiday rule) | HTTP 200; `{"OutBlock_1":[]}` |
| `basDd=20230102` | HTTP 200; ten sample rows; first `BAS_DD` is `20230102` |
| `basDd=20260230` (invalid date) | HTTP 200; `{"OutBlock_1":[]}` |
| `basDd=20990101` (future) | HTTP 200; `{"OutBlock_1":[]}` |
| omitted `basDd` | HTTP 200; `{"OutBlock_1":[]}` |
| invalid `AUTH_KEY` | HTTP 200; `{"respMsg":"Unauthorized Key","respCode":"401"}` |
| nonexistent API path | HTTP 200; JSON `respCode` `404` and explanatory `respMsg` |

Sample URL:
`https://data-dbg.krx.co.kr/svc/sample/apis/sto/stk_bydd_trd?basDd=YYYYMMDD`
([official sample host, API ID, request header, and sample key](https://openapi.krx.co.kr/contents/OPP/USES/service/OPPUSES002_S2.cmd?BO_ID=JvJFzlAENzZlPBDNGAWC)).
KRX's KOSPI holiday rules state that December 31, or the closest preceding
business day when December 31 is a holiday or Saturday, is closed
([KRX KOSPI trading and holiday rules](https://global.krx.co.kr/contents/GLB/06/0602/0602010201/GLB0602010201T1.jsp)).

Two consequences follow:

- Consumers must inspect JSON error envelopes even when HTTP status is 200.
- Empty `OutBlock_1` is not a self-authenticating “closed” fact. Client-side date
  validation removes one ambiguity, but the interface still lacks a documented
  completeness/error marker for empty successful responses.

The sample observations validate behavior only for KRX's sample service. An
approved production key is still required to certify production response size,
latency, throttling behavior, and the same empty/error semantics.

## Corrections, completeness, and operational risk

The response contains no revision number, publication timestamp, correction
flag, checksum, or stable dataset version. The official pages found do not
document correction timing, retention of prior versions, or a change feed.
Repeated acquisition can therefore capture KRX's then-current values, but cannot
prove from the payload whether an older date changed or why. A refresh process
would need to diff a newly acquired view against the prior snapshot and retain
its own fetch provenance.

KRX expressly reserves the ability to restrict or interrupt the service and
disclaims guarantees of accuracy, completeness, continuous provision, and
continued provision of additional statistics
([terms, articles 8 and 12](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp)).
Those terms reinforce the semantic evidence above: transport success plus an
empty list cannot be promoted to a proven closure without another completeness
control.

## Recommended acquisition role

Adopt `stk_bydd_trd` as the official KRX-operated **positive witness** for
historical Securities Market sessions beginning 2010-01-04, subject to these
gates:

- secure an approved maintainer key and service approval;
- obtain KRX confirmation that storing and redistributing a derived open/closed
  calendar in this public repository is permitted;
- reject error envelopes despite HTTP 200;
- validate requested dates locally and validate returned `BAS_DD`/`MKT_NM`;
- treat empty results as `Unknown`, not `Closed`, until corroborated by a
  separately authoritative closure/completeness source;
- record endpoint, acquisition timestamp, requested coverage, and source terms,
  then diff full refreshes to expose retrospective changes.

## Remaining blockers

1. **Licence/redistribution:** the published restrictions appear incompatible
   with blindly committing API-derived data; KRX must clarify the treatment of a
   derived trading-session calendar.
2. **Negative evidence:** no documented contract makes an empty daily response
   equivalent to “exchange closed.”
3. **Production certification:** no approved production key was available in
   this research session, so production row limits and empty/error behavior were
   not live-tested.
4. **Correction contract:** KRX publishes no correction/version semantics for
   this endpoint in the official materials reviewed.
5. **Cost:** official materials reviewed state conditions and limits but do not
   explicitly state a price or guarantee zero-cost access.
