---
title: "t1404/t1405 designation boards: gubun is the market axis, jongchk the category — and jongchk \"0\" on t1404 is the WHOLE listed board, not a category"
date: 2026-07-11
category: conventions
module: "adapters/nautilus reference capture (src/reference/capture.rs) + ls-sdk paginated t1404/t1405"
problem_type: convention
component: tooling
severity: high
applies_when: "Querying the t1404 (관리/불성실/투자유의) or t1405 (투자경고/매매정지/정리매매) designation boards — especially when the rows feed a tradability/surveillance gate that excludes designated symbols"
symptoms:
  - "The normalized baselines carry only description hashes for gubun/jongchk — the category enum is not recoverable offline"
  - "t1404 with jongchk \"0\" returns ordinary, undesignated stocks (동화약품, 경방, …) with empty designation date/edate fields — 100 rows per page across the whole listed board"
  - "Byte-size probing cannot disambiguate the axes: page caps (~100 rows) make category and market splits look alike"
tags:
  - t1404
  - t1405
  - designation-board
  - surveillance-gate
  - jongchk
  - gubun
  - enum-confirmation
  - ls-gateway
related:
  - docs/solutions/conventions/sdk-struct-field-from-baseline-korean-name.md
  - docs/solutions/conventions/normalized-baseline-can-underreport-request-block.md
  - docs/solutions/integration-issues/ls-gateway-igw00201-continuation-page-bursts-vs-paced-single-reads.md
---

# t1404/t1405 designation boards: gubun is the market axis, jongchk the category — and jongchk "0" on t1404 is the WHOLE listed board, not a category

## Context

The reference-data universe engine (plan 2026-07-10-003, PR #116, unmerged as
of this writing) gates tradability on the `t1405` (투자경고/매매정지/정리매매조회)
and `t1404` (관리/불성실/투자유의조회) designation boards: any symbol a queried
category returns is hard-excluded from the tradeable set. The normalized
baselines (`crates/ls-trackers/baselines/api-drift/normalized/trs/t140{4,5}.json`)
carry only description *hashes* for the `gubun`/`jongchk` request fields, so
the category enum had to be confirmed against the live paper gateway
(closed-window pre-flight, 2026-07-10) with **field-level row evidence** —
byte-size probing alone was actively misleading because both TRs page at ~100
rows, making "all" vs "one big category" indistinguishable by response size.

## Guidance

Confirmed axes (live rows, 2026-07-10; encoded in
`DesignationQuery`'s doc comment, `adapters/nautilus/src/reference/capture.rs:60-76`):

- **`gubun` is the market axis on BOTH TRs**: `0` all / `1` KOSPI / `2` KOSDAQ.
  (An out-of-domain value like `3` byte-identically re-serves the `0` board.)
- **`t1405 jongchk` categories**: `1` 투자경고 (warning) / `2` 매매정지 (halt —
  >1 page, ~120 rows) / `3` 정리매매 (liquidation) / `4` 투자주의 (caution,
  one-day designations all dated the query day) / `5` 투자위험 (risk, often
  empty) / `6` 투자위험예고 (risk pre-announce) / `7` 단기과열 (overheated —
  characteristically preferred shares, with `date`→`edate` windows).
- **`t1404 jongchk` categories**: `1` 관리 (managed — >1 page, ~105 rows) /
  `2` 불성실공시 (unfaithful disclosure, short penalty windows) / `3` 투자유의
  (often empty) / `4` 투자환기 (KOSDAQ alert, `6xxx` reason codes).
- **`jongchk` `"0"` on t1404 is NOT "all categories" — it is the entire listed
  board**, ordinary stocks included, with empty designation `date`/`edate`
  fields. Feeding it to a gate that treats returned rows as "designated" marks
  the whole market non-tradable. The capture refuses any `jongchk "0"` query
  before dispatching (`capture.rs:235-250`), with a wiremock test pinning the
  refusal (`adapters/nautilus/tests/reference_capture.rs`,
  `a_whole_board_jongchk_zero_category_is_refused_before_any_call`).
- **Body-cursor pagination behaves as the single-page SDK structs imply**
  (neither response carries `tr_cont` header fields): the returned
  `cts_shcode` equals the last shcode of the page; sending it back with a
  request-side `tr_cont: Y` serves the page *after* it; the terminal page
  returns an empty cursor. Verified live on `t1405 jongchk=2` (100 + 20 rows)
  and `t1404 jongchk=1` (100 + 5 rows).

Defaults encoding the confirmed 7+4 category set live in
`CaptureConfig::new` (`capture.rs:135-153`), operator-overridable via
`LS_CAPTURE_T1405_CATEGORIES` / `LS_CAPTURE_T1404_CATEGORIES`
(`gubun:jongchk:kind;...`), and whatever was actually queried is recorded in
the artifact's `tier_boundary_rule` provenance.

## Why This Matters

The designation boards feed a **hard gate** whose failure mode is silent:
a mis-mapped category doesn't error, it quietly includes suspended/managed
symbols in (or excludes the entire market from) the tradeable set. The
`jongchk "0"` trap is the extreme case — one plausible-looking "query all"
configuration flips every symbol to non-tradable, and the artifact still
validates. Conversely the per-category union is safe even if two category
*labels* were swapped, because every queried category designates
non-tradability; what must be exactly right is (a) never querying the
whole-board pseudo-category and (b) walking multi-page categories to the end
(halt and managed both exceed one page — a first-page-only read silently
drops ~20 designated symbols).

## When to Apply

- Authoring or reconfiguring any consumer of `t1404`/`t1405` (the capture's
  designation gate, future surveillance dashboards, screening tools).
- Confirming an enum the normalized baseline only carries as description
  hashes: probe with **field-level row output** (shcodes/hnames/dates parsed
  from the body), not the credential-safe byte-size classifier — sizes cannot
  separate axis semantics under page caps.
- Any "designated list" whose rows have empty `date`/`edate` fields is a
  red flag that you queried the whole board, not a category.

## Examples

The confirming probe shape (credential-safe: token stays in a shell var,
output is parsed fields only):

```text
== t1404 g=0 j=0: rows=100 cts='0011T0'
   000020 동화약품 date= edate=      ← ordinary stock, empty dates: WHOLE BOARD
== t1404 g=0 j=1: rows=100 cts='418250'
   001470 삼부토건 date=20250401 reason=1204   ← 관리종목, designated
== t1405 g=0 j=7: rows=12
   000545 흥국화재우 date=20260708 edate=20260710  ← 단기과열, windowed
```

And the refusal that keeps the trap unreachable:

```rust
// adapters/nautilus/src/reference/capture.rs:241
if let Some(q) = qs.iter().find(|q| q.jongchk.trim() == "0") {
    return Err(fail(format!(
        "{tr} designation query with jongchk=\"0\" (gubun={}) is the whole board, not a \
         category — it would designate every symbol; query categories individually", q.gubun
    ), 0));
}
```
