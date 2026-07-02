---
title: "Normalized baseline can under-report a TR's request block — cross-check the certified live SDK request struct, which wins on disagreement"
date: 2026-07-02
category: conventions
module: "ls-trackers normalized baseline, ls-sdk request modeling (crates/ls-sdk)"
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Authoring or reviewing a `make raw-probe` command or a ledger request-body example for a TR"
  - "A normalized baseline's request block (InBlock) looks incomplete versus the raw OpenAPI capture or an already-implemented SDK request struct"
  - "Deciding which fields belong in a TR's request struct when the baseline and the raw capture disagree"
  - "Reviewing a PR that quotes a probe/example request body against metadata/trs or crates/ls-trackers/baselines"
tags:
  - normalized-baseline
  - request-modeling
  - raw-capture
  - ls-trackers
  - ls-sdk
  - code-review
  - wire-shape
---

# Normalized baseline can under-report a TR's request block — cross-check the certified live SDK request struct, which wins on disagreement

## Context

During code review of PR #85 (the domestic KRX-open reconfirmation close-out, ledger
§23), a standards reviewer flagged the `LS_PROBE_BODY` embedded for `CSPBQ00200`:

```
LS_PROBE_BODY='{"CSPBQ00200InBlock1":{"RecCnt":1,"BnsTpCode":"1","IsuNo":"KR7005930003","OrdPrc":75000,"RegCommdaCode":"41"}}'
```

The reviewer pointed at AGENTS.md's own rule — "Wire field names, types, and
array-vs-single shapes come from the normalized baseline
(`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`), not guesswork" —
and proposed dropping `RecCnt` and `RegCommdaCode` as invented fields, since they
don't appear in the baseline's request block. Taken literally, that "fix" would have
turned a body that is proven to work on the live gateway into one that (per the
project's own IGW40011 gotcha, and per not knowing whether the gateway requires those
fields) might not.

## Guidance

Verified directly against the repo:

**The normalized baseline** (`crates/ls-trackers/baselines/api-drift/normalized/trs/CSPBQ00200.json`)
lists exactly three fields under `CSPBQ00200InBlock1`:

```
0  BnsTpCode  String
1  IsuNo      String
2  OrdPrc     Number
```

**The certified SDK request struct** (`CSPBQ00200InBlock1` in
`crates/ls-sdk/src/account/capacity.rs`, ~lines 396-410) carries five fields:

```rust
pub struct CSPBQ00200InBlock1 {
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    #[serde(rename = "IsuNo")]
    pub isuno: String,
    #[serde(rename = "OrdPrc", serialize_with = "ls_core::string_as_number")]
    pub ordprc: String,
    #[serde(rename = "RegCommdaCode")]
    pub regcommdacode: String,
}
```

The baseline under-reports this TR's request shape by two fields (`RecCnt`,
`RegCommdaCode`). The five-field shape is not speculative: PROVISIONALITY-LEDGER.md
§16 records a live paper smoke against exactly this struct returning `rsp_cd 00136`
with a structurally-valid row — a live-gateway acceptance, not a guess. `RecCnt` and
`OrdPrc` are numeric slots (`serialize_with = "ls_core::string_as_number"`), which is
itself evidence the extra fields were hard-won: the same TR family is subject to the
project's IGW40011 gotcha (numeric request fields sent as JSON strings get rejected),
so this struct's shape was tuned against real gateway responses, not copied from docs.

The working, ledger-recorded probe body (§23) is the five-field one, unchanged:

```
LS_PROBE_BODY='{"CSPBQ00200InBlock1":{"RecCnt":1,"BnsTpCode":"1","IsuNo":"KR7005930003","OrdPrc":75000,"RegCommdaCode":"41"}}'
```

**Precedence rule, defensible from this evidence:** certified live SDK request struct
> normalized baseline, for any field the two disagree on. (A raw-capture request
example, when present, sits above the SDK struct only until something has actually
been certified live — once a struct carries a live `rsp_cd` success, as here, it is
the strongest witness available and wins.) The baseline is the right *starting point*
for an unimplemented TR — it's what `track-tr` projects and what `implement-tr`
authors a first struct from — but it is a capture of one API-drift snapshot, not a
runtime oracle. Once a struct has been certified against the live gateway, it encodes
information the baseline never had a chance to (which fields the gateway actually
requires, and in what JSON type).

## Why This Matters

- **A probe body derived baseline-only can omit fields the gateway needs.** Had the
  review suggestion been applied, the next `raw-probe` run against `CSPBQ00200` would
  likely have regressed from a clean `00136` to an `IGW40011`/validation failure,
  wasting a live-gateway attempt and miscasting a working shape as broken.
- **A reviewer following the letter of AGENTS.md can still demote a working shape.**
  The rule ("baseline, not guesswork") was written to stop invented fields, but a
  literal read treats *baseline-only* as the ceiling instead of the floor, punishing a
  TR whose SDK struct is a superset for good reason (IGW40011 avoidance, gateway
  necessity) rather than sloppiness.
- **Future TRs may have the same silent under-reporting.** The normalized baseline is
  projected from one raw OpenAPI capture; nothing guarantees it enumerates every field
  the gateway will accept or require. Any TR whose SDK struct was hardened after a
  live IGW40011 fix is a candidate for this same baseline/struct gap, and each one
  looks, out of context, like "invented" fields to a reviewer who hasn't checked.

## When to Apply

- Authoring or reviewing a `make raw-probe LS_PROBE_BODY=...` invocation for a TR that
  already has a shipped SDK request struct.
- Implementing a TR (`implement-tr`) whose SDK carrier already exists from an earlier
  wave — don't re-derive the body from the baseline alone when a certified struct is
  sitting right there.
- Reviewing any wire-shape claim ("field X isn't in the baseline, drop it") — check
  the certified SDK struct and PROVISIONALITY-LEDGER for a live `rsp_cd` success
  before treating the baseline as final.
- Resolving a baseline-vs-SDK-struct disagreement of any kind: default to the
  certified struct, and document in the ledger/metadata *why* it's a superset so the
  next reader doesn't re-litigate it as a bug.

## Examples

**Flagged (rejected) review suggestion:** drop `RecCnt` and `RegCommdaCode` from the
`CSPBQ00200` probe body because the normalized baseline's `CSPBQ00200InBlock1` block
lists only `BnsTpCode`/`IsuNo`/`OrdPrc`.

**Applied resolution:** keep the five-field body exactly as it was (`RecCnt`,
`BnsTpCode`, `IsuNo`, `OrdPrc`, `RegCommdaCode`), and add an explicit note in
PROVISIONALITY-LEDGER.md §23 that the body's source of truth is the proven-live SDK
struct in `crates/ls-sdk/src/account/capacity.rs`, which is a superset of the
baseline — "the baseline under-reports `RecCnt`/`RegCommdaCode` for this TR, so mirror
the SDK struct, not the baseline alone, when re-deriving this body."

**Contrast case (baseline and struct agree — no special-casing needed):** `t0441`'s
normalized baseline lists exactly two request fields under `t0441InBlock`
(`cts_expcode`, `cts_medocd`, both `String`), and the shipped struct in
`crates/ls-sdk/src/account/holdings.rs` (~lines 559-578) carries exactly those same
two fields, nothing more. For `t0441` the baseline alone would have been sufficient —
which is precisely why this is a *per-TR* hazard to check, not a universal "the
baseline is always wrong" rule.

## Related

- `docs/solutions/conventions/tr-out-block-shape-from-raw-capture.md` — the response-side sibling of this rule: the normalized baseline also collapses OUT-block wire shape (key + array-vs-single); model responses from the raw capture. Together the two docs bracket the baseline's lossiness in both directions.
- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md` — the request-side TYPE rule (numeric slots must serialize as JSON numbers); this doc is the request-side EXISTENCE rule. CSPBQ00200's `RecCnt`/`OrdPrc` are subject to both.
- `docs/solutions/conventions/sdk-struct-field-from-baseline-korean-name.md` — same cautionary posture (the baseline is not the sole authority when authoring a TR struct), different mechanism (response field selection via `korean_name`).
- `docs/solutions/conventions/closed-window-account-capacity-reads-all-default.md` — same TR (`CSPBQ00200`) and file, unrelated problem (all-default capacity READ rows / `00136` witnessing).
- PROVISIONALITY-LEDGER.md §16 (live `00136` certification of the five-field struct) and §23 (the probe body + source-of-truth note this review produced).
