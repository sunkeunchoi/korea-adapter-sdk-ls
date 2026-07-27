# Official API sources for forward KRX equity closures

Research date: 2026-07-18 (Asia/Seoul)

## Question

Does a KRX-operated API, or an official Korean public-data API with documented
KRX provenance, publish scheduled and extraordinary KRX domestic-equity closures
before the market opens? If not, what is the narrowest official-API-derived
alternative suitable for a versioned offline calendar?

## Verdict

**No qualifying API was found.** The exhaustive public service list for the
[KRX Data Marketplace Open API](https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd)
contains index, equity, security-product, bond, derivative, commodity, and ESG
datasets, but no trading-calendar, holiday, closure, market-operation, or notice
service. Its equity services are daily trading and instrument data beginning in
2010; they can witness a session only after daily data exists, not announce a
future closure.

The closest official API is the Korea Astronomy and Space Science Institute
(KASI) **Special Day Information** service's public-holiday operation,
`getRestDeInfo`. It is a useful primary input, but it is not KRX-sourced and does
not represent KRX-only rules or discretionary closures. Therefore it does not
satisfy the ticket by itself.

The narrowest defensible design is:

1. acquire government/public-office holidays from KASI's official public-data
   API;
2. deterministically apply the KRX-published equity-market rules for Saturdays,
   May 1, and the year-end closure;
3. treat future dates as **unknown with respect to an exceptional KRX closure**
   unless an operator has incorporated a first-party KRX notice; and
4. after each date, reconcile the snapshot against a KRX daily equity-data API.

This is official-API-derived and materially better than weekday arithmetic, but
it cannot certify before open that KRX has not exercised its discretionary power
to close the market. Closing that last gap requires either a new KRX API/feed or
permission to ingest a first-party KRX notice channel that is not currently a
documented Open API.

## Why a public-holiday API covers most, but not all, closures

KRX's own [equity-market operating page](https://regulation.krx.co.kr/contents/RGL/03/03010100/RGL03010100T1.jsp)
states that trading days are Monday through Friday except:

- holidays under the Regulation on Holidays of Government and Public Offices;
- May 1;
- Saturdays;
- December 31, or the preceding trading day when December 31 is a holiday or
  Saturday; and
- any other day KRX deems necessary.

The final category is decisive: a government-holiday feed cannot establish the
absence of a KRX-specific exceptional closure.

The current official
[Regulation on Holidays of Government and Public Offices](https://www.law.go.kr/lsInfoP.do?lsId=002404)
includes regular-term election days and days designated ad hoc by the government.
Its replacement-holiday rules are separately codified in
[Article 3](https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1027161473).
Consequently, an up-to-date public-holiday API should cover scheduled election
days and government-designated temporary holidays as public holidays. That still
does not give it KRX provenance, nor does it cover KRX's independent discretionary
category.

KRX also publishes advance, first-party notices outside its Open API. For example,
the [2024 year-end market-operation notice](https://kind.krx.co.kr/external/2024/12/17/000102/20241216000236/70780.htm)
published the December 31 closure and January 2 delayed opening. This proves an
authoritative announcement channel exists, but the cited artifact is an HTML
notice, not a documented calendar API. Treating that channel as a machine feed
would be web scraping unless KRX documents a supported interface.

## Candidate assessment

### 1. KRX Data Marketplace Open API

| Property | Finding |
| --- | --- |
| Operator and provenance | Operated by KRX; the [service description](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO001.jsp) says it exposes KRX Data Marketplace statistical information. |
| Calendar endpoint identity | None found in the complete [published service list](https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd). The nearest equity endpoint is `유가증권 일별매매정보` (KOSPI-market daily trading information). |
| Direction of evidence | Retrospective only. The catalog describes daily trading rows from 2010 onward, not future market-operation status. |
| Authentication and approval | Data Marketplace registration, authentication-key application, API-specific utilization application, and administrator approval are required by the [official use procedure](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO003.jsp). |
| Key lifetime and limits | The [terms](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp) set a one-year usage period (renewable), a maximum of 10,000 requests per key per day, and possible deletion after 12 months of key inactivity. |
| Cost | The Open API pages reviewed do not publish a price. No claim that it is free is justified by the cited materials. |
| Terms relevant to snapshots | The same [terms](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp) limit use to non-commercial purposes, prohibit providing KRX data to third parties, and prohibit use after the agreement ends. Committing raw or readily reconstructable KRX response data to a public repository therefore requires explicit KRX confirmation; the public terms do not establish that it is permitted. |
| Publication horizon/timing | Not applicable to closures; no forward-calendar service exists in the catalog. |
| Retention/revisions | Daily services advertise coverage from 2010, but no closure-revision history or calendar retention policy exists because no calendar service exists. |
| Offline-snapshot suitability | Suitable only as a post-session reconciliation witness, subject to terms review. It cannot create an authoritative forward snapshot. |

### 2. KASI Special Day Information Open API

| Property | Finding |
| --- | --- |
| Operator and provenance | The [Public Data Portal record](https://www.data.go.kr/data/15012690/openapi.do) identifies KASI as provider and its Astronomical Computing Convergence Center as manager. It contains no assertion that the data is supplied by or certified by KRX. |
| Endpoint identity | Base URL `http://apis.data.go.kr/B090041/openapi/service/SpcdeInfoService`; public-holiday operation `getRestDeInfo`. The record describes lookups of public holidays and exposes date, name, category, and public-office-holiday status. |
| Authentication and approval | A Public Data Portal `ServiceKey` is mandatory. Development and production use are both listed as automatically approved. |
| Cost and terms | The record says free, 10,000 development requests, increased production traffic available after registering a use case, and unrestricted permission scope. |
| Update behavior | The portal labels the update cycle "real time." It publishes no service-level commitment stating how soon a newly designated temporary holiday or election change appears. |
| Publication horizon | The API record requires a four-digit year and accepts an optional month, but publishes no supported earliest/latest year or guaranteed forward horizon. KASI separately publishes official annual calendar standards and says those have been officially announced annually since 2020 (by the Ministry of Science and ICT, and since 2024 by the Korea AeroSpace Administration) on its [official almanac page](https://astro.kasi.re.kr/post/almanac). That page currently offers the 2027 official almanac; it does not document that `getRestDeInfo` has identical coverage or timing. |
| Retention | No retention guarantee is published in the API record; its time-range metadata is blank. |
| Revisions | No version, `last_modified`, supersession, or revision-history field is documented. A refresh can detect changed results only by comparing snapshots taken at different times. |
| Election days | Regular-term election days are government holidays under the [official regulation](https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1018770085), so they are in the semantic class queried by `getRestDeInfo`. The API documentation does not promise a publication lead time for them. |
| KRX year-end closure | Not covered as such. December 31 is a KRX rule, not generally a public-office holiday. |
| Extraordinary KRX closures | Not covered. Government-designated temporary holidays may enter the public-holiday dataset, but a closure selected independently by KRX is outside the dataset's documented subject. |
| Offline-snapshot suitability | Good for a versioned public-holiday input because access is free, approval is automatic, and permission scope is unrestricted. Snapshot provenance should include fetch time, request year/month, endpoint, and a content digest. It must not be labeled a complete KRX calendar. |

## Required facts that remain unestablished

No reviewed primary source establishes any of the following:

- a supported KRX API/feed for future regular-session open/closed state;
- a supported API for KRX market-operation notices;
- a guaranteed KASI forward horizon or update deadline before market open;
- a KASI revision log or retention guarantee;
- KRX provenance for KASI public-holiday rows; or
- permission under KRX Open API terms to redistribute a committed snapshot of
  KRX response data in a public SDK repository.

These are evidence gaps, not implementation details. In particular, polling an
undocumented KIND or KRX web endpoint would convert the design into scraping and
would not answer the support/SLA question.

## Recommended acquisition contract

Until KRX exposes or confirms a supported forward feed, an implementation can
make the partial guarantee explicit:

- **Authoritative government-holiday input:** `getRestDeInfo`, refreshed by a
  maintainer and committed as normalized dates plus source metadata.
- **Deterministic KRX additions:** Saturdays, May 1, and the year-end rule exactly
  as stated by KRX.
- **Scheduled elections:** consumed through the government-holiday input; verify
  every known election date in the generated snapshot because the API publishes
  no lead-time SLA.
- **Temporary government holidays:** detected by repeated refresh; flag any diff
  for review because the API supplies no revision metadata.
- **KRX-only exceptional closure:** return `Unknown` unless a first-party notice
  has been manually incorporated and cited. Do not silently classify it as open.
- **Post-date reconciliation:** query a KRX daily equity dataset and record whether
  the expected market-wide rows exist. Use this to correct history, never to claim
  forward certainty.
- **Refresh cadence:** at minimum on publication of the next official annual
  almanac, after election/temporary-holiday announcements, near year end, and
  immediately before a production run. This cadence is a risk-control proposal,
  not an API guarantee.

The implementation-ready route should therefore preserve a three-state calendar:
`TradingSession`, `Closed`, and `Unknown`. The available official APIs can prove
ordinary scheduled closures and certify sessions retrospectively; they cannot
eliminate `Unknown` for a future KRX discretionary closure.
