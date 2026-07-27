# KRX calendar snapshot publication rights

**Research date:** 2026-07-18  
**Scope:** a versioned offline table containing only a date and the project's normalized `Trading Session` status, where the status is inferred from a valid, non-empty KRX Open API `stk_bydd_trd` response. The proposed table excludes KRX response rows, securities, prices, volumes, row counts, and the authentication key.

## Conclusion

The currently published authoritative writing is **not sufficient to authorize public or collaborator redistribution of the normalized date/status table**. It unambiguously restricts sharing raw/source data and expressly permits a narrower activity—displaying processed API data on another screen, non-commercially, with no advertising or paid features and with KRX attribution. It does not say that a downloadable, forkable, versioned data file is “display,” nor classify this particular derived fact set as either (a) KRX-provided information subject to the third-party restriction or (b) independently produced “new additional information” outside it.

Until KRX gives written clarification, the defensible interim forms are:

1. a **maintainer-generated, uncommitted artifact** accessible only to the approved API user, used non-commercially while that user's API agreement remains active; or
2. for anything distributed to repository users, contributors, CI, releases, or downstream applications, an **official-source design that excludes KRX-derived rows**.

A **public normalized snapshot is not authorized** by the published terms. A public, non-commercial *screen* showing processed calendar results can fit the published FAQ, but that is not permission to publish its backing data artifact. A **repository-private snapshot is also not established as permitted** merely because access is restricted: contributors or other account holders can still be third parties, and the terms do not define an organizational-user sharing boundary. Attribution or removing all original response columns does not cure this uncertainty.

This is a conservative contractual conclusion, not a determination that calendar facts are copyrightable and not legal advice. Written KRX approval could change it.

## What the official writing does settle

The KRX Open API terms apply when a person or corporation agrees to them and obtains an authentication key. The API is described as enabling that user to build its own applications and services with KRX Data Marketplace data. The key and each API use require approval, and the stated use period is one year, renewable in one-year increments on a timely request ([KRX Open API terms, arts. 2, 3 and 5](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp); [official use procedure](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO003.jsp)).

The material restrictions are explicit:

- use is limited to **non-commercial purposes**, and the user may not charge a third party for API results (art. 6(2));
- the user may not distort API results or cause misunderstanding about their content or intent (art. 6(3)-(4));
- a screen made using API results must say that it uses “한국거래소 통계정보” (KRX statistical information), unless KRX specifies another display form (art. 10(3));
- the user “may not provide the data provided by the KRX to any third parties” (art. 11(2)); and
- after expiration or termination of the data-use agreement, the user may not use data provided by KRX (art. 11(3)).

KRX's official FAQ gives the most specific published interpretation. Its answer to “Can KRX OPEN API be used commercially?” says the API is for non-commercial purposes such as personal research and personal investment. It then says that, where there are no advertisements or paid features, processed API data may be shown on another screen if the source is marked “한국거래소 통계정보,” while the raw/source data itself may not be shared with third parties ([KRX Open API FAQ data, item 17, updated 2026-05-06](https://openapi.krx.co.kr/contents/OPP/COMM/faq/OPPCOMM004D1.cmd?pageNum=1&rowCount=100&pageCount=10&totalCount=0)). This is affirmative permission for attributed non-commercial *display* of processed data. It does not mention publishing data files, bulk download, source repositories, package distribution, forks, CI copies, or post-expiry retention.

The current terms took effect on 2025-12-26. KRX may amend them; restrictive amendments are to be announced at least 15 days before their effective date and individually notified, but the user bears responsibility for checking changes (art. 3(4)-(5)). KRX also disclaims continuous provision and the accuracy or completeness of the statistics, limits each key to 10,000 calls per day, and may delete a key after 12 months of non-use (arts. 8 and 12). The FAQ says the service is free, individual API approval is required in addition to a key, the maximum selected period is 12 months, renewal is not automatic, and a renewal application becomes available 30 days before expiry ([KRX Open API FAQ data, items 12, 14, 15 and 22](https://openapi.krx.co.kr/contents/OPP/COMM/faq/OPPCOMM004D1.cmd?pageNum=1&rowCount=100&pageCount=10&totalCount=0)). These provisions make an active agreement, terms review, provenance, and repeatable refresh prerequisites; they do not create a perpetual snapshot right.

KRX's separate Data Marketplace website terms reinforce the conservative reading. They prohibit copying, reproducing, distributing, transmitting, or publicly transmitting site information without prior KRX permission, and prohibit automated unauthorized collection or distribution ([KRX Data Marketplace membership terms, arts. 10 and 12](https://data.krx.co.kr/contents/MMC/COMS/client/MMCCOMS002_S1.cmd?isPreviousMember=N&type=C)). The Open API's own terms are the more specific source for approved API calls, but neither text grants an open-data or open-source redistribution license.

## What remains unresolved: payload versus derived date/status facts

### Source payload

Publishing an `stk_bydd_trd` response, selected response rows, or response fields is straightforward third-party provision of information supplied by KRX and is barred by article 11(2) absent separate permission. The FAQ independently states that raw/source data itself may not be shared with third parties. The project should continue not to commit, attach, quote, or otherwise publish those payloads.

### Normalized date/status set

The proposed table is materially different: it retains only the proposition that a regular KRX domestic-equity session occurred on a date, inferred from qualifying response presence. KRX's terms acknowledge that a user may edit KRX statistics to produce “new additional information,” but that clause only disclaims KRX responsibility for the result; it does **not** grant a right to redistribute it or say whether article 11 still covers it (art. 12(3)). The FAQ confirms that processed data may be *displayed* under strict non-commercial and attribution conditions, but draws the prohibited line only at sharing “raw/source data”; it does not classify a processed data file. At the same time, the terms define API service broadly as building applications and services with supplied data (art. 2(2)). There is no definition of “data provided,” “third party,” “derived,” “additional information,” “display,” aggregation threshold, download, or permissible repository audience.

The Copyright Act does not answer that contractual classification. It gives a database producer rights over all or a substantial part of a database and can treat repeated or systematic extraction of smaller portions as substantial when it conflicts with normal exploitation or unreasonably harms the producer; it also says protection does not extend to the individual materials themselves ([Copyright Act, art. 93](https://www.law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1029423451)). Thus a single calendar fact and systematic construction of a multi-year fact set are treated differently in the statute, but the KRX agreement's “information provided” restriction can be broader than copyright. Neither source establishes that this exact Boolean/date transformation is redistributable.

KRX's market-data distribution guidance also says a separate agreement is required when an individual wants redistribution, program development, commercial use, or another purpose beyond simple reference, and directs professional subscribers/vendors through the KRX-approved Koscom contracting process ([KRX data-receipt guidance](https://openapi.krx.co.kr/contents/OPP/DATA/OPPDATA003.jsp)). That page concerns real-time or delayed market information rather than expressly classifying `stk_bydd_trd`-derived calendar facts, so it is evidence that KRX licenses redistribution separately, not an answer for this snapshot.

## Implications by proposed form

| Form | Current written basis | Audience | Attribution | Retention / expiry | Commercial use | Refresh consequence |
|---|---|---|---|---|---|---|
| Public normalized snapshot | **Do not use.** No public artifact-redistribution grant; derived-file classification is unresolved. The FAQ permits a narrower attributed, ad-free and unpaid screen display of processed results. | A public screen can fit the FAQ conditions; repository files, package registries, releases, forks, and downloads are not addressed and give third parties copies. | The public screen must say “한국거래소 통계정보”; the required form for a data file is unspecified. Attribution is not artifact permission. | A perpetual public version conflicts with the post-agreement-use restriction if the set is covered. | Not permitted. The FAQ limits use to personal research/investment and treats ads or paid features as outside its non-commercial display permission; an open-source distribution also cannot enforce non-commercial downstream use. | An approved, active key is required; terms and corrections must be rechecked each refresh. Refresh does not cure publication rights. |
| Maintainer-generated uncommitted artifact | **Conservative interim only.** Keep isolated to the approved API user; do not sync or hand it to others. | The approved individual or corporate API user only. Whether personnel within a corporate user may share it is not defined. | Preserve internal provenance; use the required KRX wording if a screen displays results. | Use only while the agreement is active. The terms prohibit post-expiry use but do not state whether deletion is required, so quarantine/delete rather than continue using it. | Non-commercial only; no third-party charge. | Renew before the one-year expiry, keep the key active, diff refreshed evidence, and stop use if approval lapses. |
| Repository-private artifact | **Do not assume permitted.** Private visibility is not a license. | Collaborators, hosted CI, contractors, or separate accounts may be third parties. A corporate-user boundary is unspecified. | Same ambiguity as above. | Copies and caches are hard to disable at expiry; retention policy is unspecified. | Non-commercial restriction still applies. | Every holder/cache creates revocation and refresh implications; written KRX approval must name the audience. |
| Official-source design excluding redistributable KRX-derived rows | **Permitted fallback for the repository because it publishes no KRX-derived row.** Each replacement source still needs its own permission review. | Governed by the replacement official source's terms, not by a presumed KRX Open API redistribution right. | Follow each source's stated attribution. Do not describe excluded dates as KRX-API-certified. | No KRX-derived artifact survives API expiry; local evidence used by the API user must still stop being used at expiry. | Depends on the replacement source. | KRX may remain an operator-only diagnostic if its output never enters the shared artifact; distributed rows must be reproducible from separately authorized sources. |

## Exact written clarification required

Send the following request from the actual approved API user's identity. Ask KRX to answer in writing and retain the response with the approval record (credential-free in project provenance):

> We use the KRX Open API service **유가증권 일별매매정보** (`stk_bydd_trd`) only to determine whether a historical date had a KRX domestic-equity regular trading session. For each date, our generated file would contain only `date` and a normalized status such as `Trading Session`. It would contain no KRX response row, security identifier, market value, price, volume, row count, response text, or authentication key.
>
> 1. Is that date/status file “information provided by KRX” under Open API Terms article 11(2)-(3), or “new additional information” under article 12(3) that the API user may redistribute?
> 2. Does committing that file to a repository or package count as the FAQ's permitted display of processed data, or as third-party sharing/distribution? If distribution is allowed, may it be (a) committed to a public open-source GitHub repository, (b) distributed in source/binary packages and forks, and (c) used by commercial downstream users without a separate KRX agreement or fee?
> 3. If public redistribution is not allowed, may it be stored in a private repository and used by named project contributors and hosted CI? Please define whether those persons/services are third parties and whether approval of a corporate API user covers its personnel and contractors.
> 4. What exact KRX attribution must appear in the data file, documentation, and application screen?
> 5. May existing generated date/status rows be retained and used after the authentication key or one-year API agreement expires or is terminated? If not, must every copy and cache be deleted, and may a newly approved/renewed user regenerate the same historical set?
> 6. Does non-commercial-only article 6(2) apply to the generated date/status file and to downstream applications that use it? Is a separate data-distribution agreement available, and what audience, term, fee, and refresh conditions would it impose?

The KRX Open API site publishes **krxdata@krx.co.kr** in its official footer as the Data Marketplace contact ([KRX Open API terms page/footer](https://openapi.krx.co.kr/contents/OPP/INFO/OPPINFO002.jsp)). That is the first channel for this API-specific classification. If KRX classifies the use as market-data redistribution, its official distribution page directs contracting inquiries to Koscom's Market Information Sales team and lists its current email and telephone contact; request that KRX route or confirm the appropriate agreement rather than assuming that the market-data contract applies ([KRX data-receipt guidance](https://openapi.krx.co.kr/contents/OPP/DATA/OPPDATA003.jsp)).

## Decision gate

Do not publish KRX response data or the derived date/status rows unless the written reply expressly covers the proposed audience and redistribution medium. A reply that only says “API use is allowed,” “non-commercial use is free,” or “facts are not copyrighted” is insufficient: it must classify the described transformation under articles 11 and 12 and answer audience, attribution, retention, commercial use, expiry, and refresh/regeneration.
