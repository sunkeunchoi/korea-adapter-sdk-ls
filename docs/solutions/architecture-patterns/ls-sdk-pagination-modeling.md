---
title: "LS SDK pagination modeling: has_pagination is a metadata mirror (one-way implication with self_paginated), and the single-page body-idx sub-pattern"
date: 2026-06-21
last_updated: 2026-06-25
category: architecture-patterns
module: crates/ls-core, crates/ls-sdk
problem_type: architecture_pattern
component: tooling
severity: medium
applies_when:
  - "Adding a new TR EndpointPolicy const and setting its has_pagination flag"
  - "Reconciling endpoint_policy.rs consts against metadata facets (self_paginated)"
  - "Implementing a TR whose continuation cursor is a request-body field (idx), not the tr_cont/tr_cont_key headers"
  - "Deciding whether a paginated TR can be promoted at single-page scope"
  - "Implementing a header+rows cursor read (chart, period-trend) — pick the t1514 single-page shape, not t8412 multi-page"
tags:
  - ls-core
  - ls-sdk
  - pagination
  - endpoint-policy
  - self-paginated
  - has-pagination
  - serde
  - cross-check
---

# LS SDK pagination modeling

Two non-obvious decisions about how the LS SDK models pagination. Both surfaced
during the consumer-bound Implemented Expansion wave (plan
`docs/plans/2026-06-21-003-feat-consumer-bound-implemented-expansion-plan.md`) and
will recur on every future TR wave.

## 1. `has_pagination` is a metadata mirror, not a dispatch switch — and its relationship to `self_paginated` is a one-way implication

### Context

Each TR has an `EndpointPolicy` const in `crates/ls-core/src/endpoint_policy.rs`
with a `has_pagination: bool` field, and a `facets.self_paginated: bool` in
`metadata/trs/<tr>.yaml`. It is tempting to assume (a) `has_pagination` controls
whether dispatch paginates, and (b) the two flags must be equal. Both assumptions
are wrong, and a stale `T1102_POLICY.has_pagination: true` shipped undetected for
exactly that reason — no cross-check existed.

### Guidance

- **`has_pagination` drives no runtime branching.** Pagination at runtime is
  determined by *which facade method is called* (`Inner::post` vs
  `Inner::post_paginated`) and by the `ls_core::HasPagination` trait impl on the
  request type — never by reading `policy.has_pagination`. The flag is purely the
  runtime mirror of the metadata, used only by the cross-check test. Flipping it
  (as the wave did for `t1102`, `true → false`) is runtime-inert.
- **The flag relates to `self_paginated` by a ONE-WAY implication, not equality:**
  `self_paginated == true ⟹ has_pagination == true`. A TR whose result
  self-paginates must thread continuation. The converse does NOT hold:
  `CSPAQ12200` threads the `tr_cont` header cursor defensively
  (`has_pagination: true`) while its balance result is structurally single-page
  (`self_paginated: false`). Both values are intentional.
- **Enforce the implication, not equality.** The cross-check lives in
  `crates/ls-core/tests/policy_index_crosscheck.rs`:

  ```rust
  if meta.facets.self_paginated {
      assert!(policy.has_pagination, "...self-paginating TR must thread continuation");
  }
  ```

  Asserting equality would wrongly force a "fix" to `CSPAQ12200`.

### Why This Matters

`has_pagination` *looks* load-bearing but is not, so a wrong value is invisible at
runtime — it only misleads a human (or an agent) reading the const, and silently
breaks the metadata↔runtime mirror. The one-way cross-check catches a new
paginated TR that forgot `has_pagination: true` without false-flagging the
legitimate header-cursor-but-single-page case.

### When to Apply

Any time you add a `{TR}_POLICY` const or touch `self_paginated`/`has_pagination`.
Register every new const in BOTH lists (the `policies` array in
`policy_index_crosscheck.rs` and `slice_rest_policies_are_non_order_rest` in
`endpoint_policy.rs`) — neither auto-discovers it.

## 2. The single-page body-`idx` paginated sub-pattern

### Context

`ls-core`'s pagination machinery only threads the header cursor
(`tr_cont`/`tr_cont_key`) that `t8412` uses (`post_paginated`, `collect_all`,
`HasPagination`). Some TRs (the 7 stock rank/screen TRs `t1452`, `t1403`, `t1441`,
`t1463`, `t1466`, `t1489`, `t1492`) instead carry a request-**body** `idx`
continuation cursor, for which no core machinery exists. Building a body-`idx`
multi-page collector is a new `ls-core` contract, not a per-TR tweak.

### Guidance

Promote such a TR at **single-page scope** (the existing dispatch path already
supports this — see `crates/ls-sdk/src/paginated/mod.rs`):

- `idx` is an **ordinary in-block field**, serialized as a JSON number via
  `#[serde(serialize_with = "ls_core::string_as_number")]` at its first-page
  convention (`"0"`). It is **NOT** `#[serde(skip)]` — that attribute is only for
  `t8412`'s header cursors, which must never serialize into the body.
- The request struct still carries `tr_cont`/`tr_cont_key` as `#[serde(skip)]`
  fields and invokes `ls_core::impl_has_pagination!` — but **only** to satisfy the
  `Req: HasPagination` bound on `post_paginated`. They start and stay empty, so
  dispatch sends the first-page `tr_cont: N` header, identical to a plain `post`.
- Dispatch is ONE `post_paginated` call. Out-rows tolerate single-or-array via
  `ls_core::de_vec_or_single`. The response does NOT need `HasPagination`
  (`post_paginated` bounds only `Req`).
- Multi-page body-`idx` collection (a `chart_all`-equivalent) is **deferred** — it
  needs the new `ls-core` body-continuation contract.

### Why This Matters

Confusing the two continuation mechanisms is the trap: applying `#[serde(skip)]`
to `idx` (copying `t8412`) would drop the cursor from the body and silently always
fetch page one with no way to advance; conversely, leaking `tr_cont` into the body
would send a malformed request. The sub-pattern keeps them apart and ships callable
single-page reads without prematurely building multi-page machinery.

### When to Apply

Any TR whose spec marks an `idx` (or similar) body field as the continuation
cursor. Confirm the first-page convention (empty / `0` / `1`) against the
spec/gateway per TR. Use the `rank_row!` / `idx_summary!` macros in
`paginated/mod.rs` for the uniform row+summary shape.

> Sending `idx` (or any numeric request field) as a quoted string instead of a
> JSON number makes the gateway reject the call with `IGW40011`. The failure
> signature and the `make raw-probe` diagnostic that isolates it are in
> `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`.

### Examples

```rust
// Single-page body-idx request (paginated/mod.rs):
pub struct T1452InBlock {
    pub gubun: String,
    // ... filter fields ...
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,            // body cursor, JSON number, first page = "0"
}
pub struct T1452Request {
    #[serde(rename = "t1452InBlock")]
    pub inblock: T1452InBlock,
    #[serde(skip)] pub tr_cont: String,      // header cursor — never in body, stays empty
    #[serde(skip)] pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T1452Request);  // only to satisfy post_paginated's bound

// One page, empty header cursors:
pub async fn top_volume(&self, req: &T1452Request) -> LsResult<T1452Response> {
    self.inner.post_paginated(&ls_core::endpoint_policy::T1452_POLICY, req).await
}
```

Contrast with `t8412`, where `tr_cont`/`tr_cont_key` are the real cursors
(`#[serde(skip)]` header transport) and `chart_all` walks pages.

## 3. A header-cursor read defaults to the single-page `t1514` shape, not the multi-page `t8412` shape

### Context

`t8412` is the SDK's only *multi-page* paginated TR: it implements
`HasPagination` on **both** the request and the response, exposes a `chart_all`
facade that drives `collect_all`, and walks `tr_cont`/`tr_cont_key` response
headers until they go empty. Because it is the most prominent paginated exemplar,
it is tempting to copy it for every new chart or period-trend TR with a `cts_*`
cursor. That is almost always more machinery than the wave needs — and it is more
surface to get wrong (the response-side `HasPagination`, the `collect_all`
closure, the `max_pages` cap).

The breadth wave -004 charts (`t8410`/`t8451`/`t8419`/`t4203`) and the
investment-opinion read (`t3401`) are all header-summary + `cts_*`-cursor reads,
and all were modeled on **`t1514`**, not `t8412`.

### Guidance

For a header+rows read whose continuation is a body `cts_date`/`cts_time` cursor,
mirror **`t1514`** (`crates/ls-sdk/src/paginated/sector_index.rs`) unless multi-page
collection is an actual requirement:

- `impl_has_pagination!` on the **request only** — its `tr_cont`/`tr_cont_key` are
  `#[serde(skip)]` and stay empty (first page). This exists only to satisfy
  `post_paginated`'s `Req: HasPagination` bound (same reason as §2's body-`idx`
  case). The response does **not** implement `HasPagination`.
- Model `<tr>OutBlock` as the single header summary and `<tr>OutBlock1` as a
  `Vec<...>` via `de_vec_or_single` (true wire keys + array-ness from the raw
  `res_example`, per `../conventions/tr-out-block-shape-from-raw-capture.md`).
- Numeric request counts (`qrycnt`, `ncnt`) serialize as JSON numbers via
  `string_as_number` (IGW40011 guard).
- Facade returns **one page** via `Inner::post_paginated` — no `*_all`, no
  `collect_all`. The smoke validates a single page; multi-page correctness is out
  of scope (mirroring `t8412`'s own recommendation, which excludes multi-page).

Reach for the full `t8412` shape (response `HasPagination` + `collect_all` + a
`*_all` facade) only when a caller genuinely needs every page concatenated.

### When to Apply

- Implementing any chart / period-trend / history read with a `cts_*` body cursor.
- Reviewing a new paginated TR that copied `t8412` wholesale — check whether
  single-page `t1514` would have been the lighter, correct choice.

### Examples

```rust
// t1514-shape header-cursor read (single page): HasPagination on the REQUEST only.
#[derive(Serialize, Debug, Clone)]
pub struct T8410Request {
    #[serde(rename = "t8410InBlock")]
    pub inblock: T8410InBlock,
    #[serde(skip)] pub tr_cont: String,       // header cursor — first page, empty
    #[serde(skip)] pub tr_cont_key: String,
}
ls_core::impl_has_pagination!(T8410Request);  // satisfies post_paginated's bound

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8410Response {                    // NO impl_has_pagination! — single page
    #[serde(rename = "t8410OutBlock", default)] pub outblock: T8410OutBlock,
    #[serde(rename = "t8410OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T8410OutBlock1>,
    // rsp_cd / rsp_msg ...
}

pub async fn stock_chart_period(&self, req: &T8410Request) -> LsResult<T8410Response> {
    self.inner.post_paginated(&ls_core::endpoint_policy::T8410_POLICY, req).await
}
```

`t8412` is the contrast: `impl_has_pagination!(T8412Response)` too, plus a
`chart_all` facade driving `collect_all` over the response `tr_cont` headers.
