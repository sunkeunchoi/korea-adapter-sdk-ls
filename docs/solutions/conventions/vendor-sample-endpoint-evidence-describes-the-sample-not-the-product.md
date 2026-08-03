---
title: Evidence read off a vendor's sample endpoint describes the sample endpoint, not the product — vary the input before inferring behaviour
date: 2026-08-03
category: docs/solutions/conventions
module: docs/research (KRX/Koscom procurement research), adapters/nautilus/src/calendar_refresh
problem_type: convention
component: research
severity: medium
applies_when:
  - reading a vendor's public API sample, demo or "try it" endpoint to establish a product fact
  - inferring point-in-time / as-of semantics from an observed historical response
  - inferring completeness, row limits or coverage from a sample response's size
  - writing a procurement obligation that a later session will treat as settled
---

## The trap

A vendor's **sample** endpoint is a different product from the vendor's
**production** endpoint. It usually differs in three ways at once, and each one
manufactures a plausible-but-false inference:

1. **A fixed row cap** that does not vary with the request. A sample that returns
   ten rows returns ten rows for *every* input — so the row count says nothing
   about the production result set, and in particular nothing about completeness.
2. **A hardcoded default parameter** baked into the service page's form. A caller
   who does not override it observes that default's data and may read the
   observation as evidence about the *parameter's semantics*.
3. **A different URL path**, which makes the substitution easy to miss —
   `/svc/sample/apis/...` against `/svc/apis/...` differ by one path segment.

Both false inferences were made against KRX's OPEN API basic-information
services (`stk_isu_base_info` / `ksq_isu_base_info`) and both survived into a
routed procurement obligation before Part A of the #255 request package caught
them:

- *"The sample returned a 2020-04-14 state, which supports point-in-time
  semantics."* The service page **hardcodes `basDd=20200414`** as its form
  default. Observing 2020-04-14 data is evidence of the default, not of as-of
  behaviour.
- *"…but returned only ten rows"*, read as a coverage signal. The sample endpoint
  returns **exactly ten rows for every `basDd`**, and they are not even the
  query's own `ORDER BY` first ten — it is a curated response, not a truncation.

The second inference is the more dangerous, because it is *directionally*
conservative — it looks like appropriate caution — while being about the wrong
object entirely.

## The convention

**Vary the input before inferring behaviour from a sample response, and state
which endpoint you called.**

- To establish **as-of / point-in-time semantics**, make a **differential** call:
  two or more explicit parameter values, and show the result set changes exactly
  as the semantics predict. For the KRX services that meant `basDd` ∈
  {20150602, 20240102}: an issue listed in 2022 is absent at the earlier date,
  and issues delisted since are present at it. That is evidence. A single
  observation at the page default is not.
- To establish **completeness**, a sample cannot help at all. It needs a
  production call and reconciliation against an independent census for the same
  date. Record completeness as `UNESTABLISHED` until then, and do not let a
  spec's silence be read as a guarantee — KRX publishes **no** completeness
  statement for these services, so there is no sentence that could be mistaken
  for one.
- **Separate three verdicts and never collapse them**: *the source says X*, *the
  source is silent on X*, and *I could not reach the document that would say*.
  Conflating the second and third is how a research gap gets recorded as a
  vendor limitation.
- Prefer a **structural** corroboration where one is visible. These services'
  backing query joins on `:basDd BETWEEN STRT_DD AND END_DD` — a temporal-validity
  join, which corroborates the differential result. But note it is
  **implementation shape, not a published contract**: behaviour that is real
  today can change without breaking any promise the vendor actually made.

## The cheaper path this exposes

Before writing a paid vendor question about a product's behaviour, check whether
the repository **already holds production credentials** for it. This tree
authenticates the daily calendar chain against the *production* path
`data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd` using `LS_KRX_APPKEY` from the
gitignored `.env.calendar` (`adapters/nautilus/src/calendar_refresh/fetch_state.rs`),
and that file's own comment already records that `openapi.krx.co.kr` is merely
the portal. So a question framed as "we would need production access to answer
this" was, in fact, a **per-service application away** — free — rather than a
paid round trip.

The general form: *a blocker stated as "needs credentials" deserves one grep for
credentials the tree already has.*

## See also

- `docs/research/krx-part-a-public-pass-findings.md` — the A4 group, with the
  differential calls and the sample-endpoint cap documented in full.
- `docs/research/krx-licensed-sample-request-package.md` §3 — how the corrected
  inferences changed obligations 3 and 16.
- [Gap audit corrections](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/242#issuecomment-5164659108)
  — filed as a correction to the routed record.
