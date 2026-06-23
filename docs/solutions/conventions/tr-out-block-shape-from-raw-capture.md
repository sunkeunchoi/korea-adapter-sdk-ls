---
title: "Model a TR out-block's wire key and array-ness from the RAW capture, not the normalized api-drift baseline"
date: 2026-06-23
category: conventions
module: ls-sdk TR response modeling (crates/ls-sdk)
problem_type: convention
component: tooling
severity: high
applies_when:
  - "Authoring a TR response struct that has an out-block holding repeated rows"
  - "Choosing the #[serde(rename = ...)] wire key for a TR out-block"
  - "Deciding whether an out-block is a single struct or a Vec<...>"
  - "A live Paper smoke returns an empty/default canonical field or 'invalid type: map, expected a string'"
tags:
  - ls-sdk
  - response-modeling
  - api-drift-baseline
  - raw-capture
  - out-block
  - de-vec-or-single
  - serde-rename
related_components:
  - tooling
---

## Context

When authoring a TR's response struct in `crates/ls-sdk/src/market_session/mod.rs`
(or `account/mod.rs`), you read the out-block shape from the api-drift baseline.
There are TWO baselines and they are **not** equivalent for response modeling:

- **Normalized** — `crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`.
  The renormalized drift-comparison view. It **collapses out-block structure**: it
  relabels an Object-Array out-block's name to the literal `response_body` and
  flattens its row fields. The real wire key (e.g. `t8426OutBlock`) is gone, and
  the array-vs-single distinction is erased. (Some TRs survive partially: `t2522`
  keeps its `t2522OutBlock` count header but flattens the data-bearing
  `t2522OutBlock1` rows up under it, so you still cannot read the second block's
  true key or its array-ness from normalized.)
- **Raw** — `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`. The
  raw OpenAPI capture. Its `res_example` shows the literal wire payload, e.g.
  `"t8426OutBlock": [ { "shcode": "165T6000", ... }, ... ]` — the **true**
  `#[serde(rename)]` key AND the array-vs-single shape.

Because the normalized view hides both the key and the array-ness, guessing
"single" off it (or off a plan's per-TR shape table) produces a silently wrong
struct. In the read-only REST breadth wave (PRs #43/#44), **4 of 9 F/O TRs
(`t2522`, `t8401`, `t8435`, `t8467`) were mis-guessed as single-struct out-blocks
when all four were Object-Arrays.** The 4 TRs that read straight from the raw
capture (`t8426`, `t8433`, `t9943`, `t9944`) were correct from the start. The
plan's own "single (N fields)" guesses were unreliable — only the raw capture is
authoritative.

## Guidance

**Read the true out-block key and shape from the RAW capture, never the normalized
baseline.**

1. For each out-block, open
   `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`, find the TR's
   `res_example`, and read the literal JSON. Block value `[ {...}, ... ]` →
   Object-Array → model it as `Vec<...>`. Bare `{...}` count/header object →
   single struct.

2. Model every array out-block as `Vec<...>` with the tolerant adapter, keyed by
   the **true wire name** from the raw capture:

   ```rust
   #[serde(
       rename = "t8426OutBlock",
       default,
       deserialize_with = "ls_core::de_vec_or_single"
   )]
   pub outblock: Vec<T8426OutBlock>,
   ```

   `de_vec_or_single` (`crates/ls-core/src/lib.rs`) tolerates array, bare object,
   `null`, and empty-string `""`, so a one-row block still decodes — but you must
   still get the **rename key right** from the raw capture; the adapter cannot
   recover a wrong key.

3. For a count-header + row-array TR (e.g. `t2522`), the two blocks have
   **distinct** wire keys that normalized hides. Read both from the raw
   `res_example` and split them — the header (`t2522OutBlock`, single) and the
   data rows (`t2522OutBlock1`, array). `T2522Response` is the shipped exemplar:

   ```rust
   pub struct T2522Response {
       #[serde(default)]
       pub rsp_cd: String,
       #[serde(default)]
       pub rsp_msg: String,
       #[serde(rename = "t2522OutBlock", default)]
       pub outblock: T2522OutBlock,           // count header
       #[serde(rename = "t2522OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
       pub outblock1: Vec<T2522OutBlock1>,    // 기초자산명 rows
   }
   ```

4. **Diagnose a suspected shape bug with the raw-HTTP probe** — don't rebuild the
   SDK to bisect:

   ```bash
   make raw-probe LS_PROBE_TR_CD=t8467 LS_PROBE_PATH=/futureoption/market-data \
     LS_PROBE_BODY='{"t8467InBlock":{"gubun":""}}'
   ```

   Populated `body_len` + success `rsp_cd`, but the SDK call empties or fails to
   deserialize → **SHAPE bug** (fix the rename key / `Vec` modeling). This is
   distinct from a genuine `00707` empty result, which is a real **PENDING**
   disposition — ship PENDING, do not force a model change.

### Prevention

- **Empty-`00707` smokes must assert non-empty before `record(...)`.** `00707`
  (empty) classifies as a SUCCESS `rsp_cd`, so a smoke that records without first
  asserting the out-block is non-empty passes GREEN on an empty/default result and
  falsely flips the TR to Implemented. Gate the record (pattern from
  `crates/ls-sdk/tests/live_smoke.rs`):

  ```rust
  Ok(resp) => {
      assert!(
          !resp.outblock1.is_empty(),
          "live-smoke-t2522: empty result (00707) — PENDING, not Implemented"
      );
      let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
          .expect("an Ok outcome yields a result line");
      record("live-smoke-t2522", &format!("env=paper date={date}"), &line);
  }
  ```

  For a single-out-block TR, assert the canonical field instead
  (`!resp.outblock.gmprice.is_empty()`, as in `live_smoke_t2301`).

- **Don't `_v2`-name sibling TRs that share a Korean name.** Name by the real
  schema/`gubun` distinction. The shipped pair for 지수선물마스터조회 is
  `index_futures_master` (`t8467`, price-bearing rows: 상한가/하한가/전일종가/기준가)
  vs `index_futures_master_codes` (`t9943`, codes-only: 3 identity fields). A
  `_v2` suffix loses the price-vs-codes distinction callers select on. (Tier-2
  review flagged the `_v2` form as a P1.)

## Why This Matters

A wrong "single" guess does not fail at compile time — it fails at runtime as an
empty canonical field or a `invalid type: map, expected a string` deserialize
error, often only on a live Paper smoke. Worse, if the smoke records without an
emptiness assertion, the empty decode reads as a clean success and the TR is
**wrongly stamped Implemented**. Reading the raw `res_example` up front (one
lookup) prevents an entire class of silent shape bugs and false dispositions. In
the breadth wave this single discipline difference cleanly separated the 4 correct
TRs from the 4 mis-guessed ones.

## When to Apply

- Authoring or reviewing any TR response struct whose out-block holds repeated rows.
- A live Paper smoke returns an empty/default canonical field, or panics with
  `invalid type: map, expected a string`.
- You only have the normalized baseline open and are about to infer "single" vs
  "array" from it — stop and open the raw capture instead.
- Naming two TRs that share a Korean display name — name by schema/`gubun`, never `_v2`.

## Examples

Mis-guess symptom and fix, for a row-array out-block:

```rust
// WRONG — guessed "single" from the normalized baseline's `response_body` collapse.
// Live smoke empties, or serde errors: "invalid type: map, expected a string".
#[serde(rename = "t8401OutBlock", default)]
pub outblock: T8401OutBlock,

// RIGHT — raw res_example shows `"t8401OutBlock": [ {…}, … ]`, an Object-Array.
#[serde(rename = "t8401OutBlock", default, deserialize_with = "ls_core::de_vec_or_single")]
pub outblock: Vec<T8401OutBlock>,
```

Confirming a shape bug vs a real PENDING with the raw probe:

```bash
make raw-probe LS_PROBE_TR_CD=t8467 LS_PROBE_PATH=/futureoption/market-data \
  LS_PROBE_BODY='{"t8467InBlock":{"gubun":""}}'
# populated body_len + success rsp_cd, but SDK empties => SHAPE bug (fix Vec/rename)
# rsp_cd 00707 / empty body                            => genuine empty => ship PENDING
```

## Related

- `docs/solutions/conventions/sdk-struct-field-from-baseline-korean-name.md` (KTD4)
  — picks the right *field* (canonical value by `korean_name`); this doc picks the
  right *block key and shape*. Note the complement: the normalized baseline is
  reliable for a field's `korean_name` meaning (KTD4) but **lossy for out-block
  name and array-ness** (this doc — read RAW).
- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`
  — the *request-side* (`string_as_number` on serialize) counterpart to this
  doc's *response-side* (`de_vec_or_single` on deserialize); shares the
  `make raw-probe` diagnostic.
- `docs/solutions/conventions/market-hours-read-empty-result-disposition.md` — for
  the empty-result branch; this doc's "assert non-empty" rule complements its
  "empty isn't always a defect / a real DROP is raw-probe-OK-but-SDK-deserialize-fails"
  disposition.
- `docs/solutions/architecture-patterns/ls-sdk-pagination-modeling.md` — pagination
  is one consumer of the `de_vec_or_single` array-out-block rule.
