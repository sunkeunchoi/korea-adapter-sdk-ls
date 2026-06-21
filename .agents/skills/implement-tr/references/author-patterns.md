# implement-tr — author patterns & credential-free line shapes

Per-class skeletons for the `tracked → implemented` recipe. Mirror the closest
in-repo TR verbatim; these condense the load-bearing decisions.

## Where the field spec comes from

Each TR's request/response fields are in its normalized baseline:
`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`
(`request_blocks` / `response_blocks`). The `{tr}InBlock` / `{tr}OutBlock`
block names, field names (Korean + ascii), `type`, `length`, and `required`
all come from there. A `response_body` field whose `type` is `Binary` is the
**array-block marker**: the element fields that follow it form a repeated row →
model the out-block as `Vec<…>` with `ls_core::de_vec_or_single`.

`endpoint_path` → `EndpointPolicy.path`; `source_group_name` →
`EndpointPolicy.group`; `rate_limit_per_sec`/`corp_rate_limit_per_sec` → the
policy rate fields.

## Non-paginated (market_session) skeleton

Model after `T1102` in `crates/ls-sdk/src/market_session/mod.rs`. A no-caller-input
read (single `dummy` in-block, e.g. `t8425`) is `T1102` *minus* the identifier
fields and `::new()` takes no args. An array out-block uses `Vec` + `de_vec_or_single`.

```rust
#[derive(Serialize, Debug, Clone)]
pub struct T____InBlock {
    pub dummy: String,            // no-caller-input read: a length-1 placeholder
}

#[derive(Serialize, Debug, Clone)]
pub struct T____Request {
    #[serde(rename = "t____InBlock")]
    pub inblock: T____InBlock,
}
impl T____Request {
    pub fn new() -> Self { Self { inblock: T____InBlock { dummy: String::new() } } }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T____OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub field_a: String,
    // … representative subset only
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T____Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    // array out-block → Vec + de_vec_or_single; single out-block → bare struct
    #[serde(rename = "t____OutBlock", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock: Vec<T____OutBlock>,
}
```

Facade method dispatches through `Inner::post` (no continuation):

```rust
pub async fn <verb>(&self, req: &T____Request) -> LsResult<T____Response> {
    self.inner.post(&ls_core::endpoint_policy::T____POLICY, req).await
}
```

## Paginated single-page skeleton

Model after `T8412` in `crates/ls-sdk/src/paginated/mod.rs`, **but at single-page
scope only**: `ls-core` threads only the header `tr_cont`/`tr_cont_key` cursor,
which these TRs do not use. They carry a request-body `idx` continuation field for
which no core machinery exists. Promote single-page:

- one `post_paginated` call with empty `tr_cont`/`tr_cont_key` headers,
- `idx` modeled as an **ordinary in-block field** — NOT `#[serde(skip)]` (that
  attribute is only for `T8412`'s header cursors). `idx` serializes in the body.
- out-rows tolerate single-or-array via `de_vec_or_single`.
- confirm the first-page `idx` convention (empty / `0` / `1`) against the
  spec/gateway per TR (baseline marks `idx` required, length 4).
- NO `chart_all`-equivalent — multi-page body-`idx` collection is deferred
  follow-up work.

```rust
pub async fn <verb>(&self, req: &T____Request) -> LsResult<T____Response> {
    self.inner.post_paginated(&ls_core::endpoint_policy::T____POLICY, req).await
}
```

## Credential-free line shapes (R3a)

A committed line carries only lengths, `rsp_cd`, public tickers/dates/ports, and
structural counts. Never `rsp_msg`, token, appkey, secret, or account number.

Smoke success (stdout, the only capturable line):

```
LIVE-SMOKE target=live-smoke-<tr> inputs=[env=paper <pub-inputs> date=YYYY-MM-DD] result=[rsp_cd=<code> <count>=<n>]
```

Smoke failure — emit NO `LIVE-SMOKE` line; use a distinct stderr prefix so the
error body can't pattern-match as evidence:

```
SMOKE-FAIL target=live-smoke-<tr> <transport|account-state|environmental> failure (not evidence)
```

Raw-HTTP probe (classification only):

```
RAW-PROBE target=raw-probe inputs=[tr_cd=<tr> path=<path> body_len=<n>] result=[http=<status> rsp_cd=<code> body_len=<n>]
```

## Flip / no-flip checklist

| Smoke outcome | Action | Final line |
|---|---|---|
| success `rsp_cd` + non-empty + deserializes | flip `implemented`, banner page, count+1 | `IMPLEMENTED <tr>` |
| success but empty (`00707`) | no flip; record reason | `PENDING <tr> — empty result, shape unconfirmed` |
| raw HTTP ok + SDK deserialize fails | no flip; drop | `DROPPED <tr> — TR defect: <summary>` |
| raw HTTP also fails / peers fail in-window | no flip; retry; pend if no recovery | `PENDING <tr> — environmental, no in-window recovery` |
| out of scope / missing harness | no work | `HELD <tr> — <reason>` |

Always, regardless of outcome: `support.recommended` stays `false`, no
`recommendation` block, no `metadata/evidence/<tr>.yaml`, no
`metadata/EVIDENCE-FRESHNESS.md` edit. Field-level `type` ledger facets are NEVER
retired here.
