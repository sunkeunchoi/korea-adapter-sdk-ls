---
title: "LS gateway IGW40011 — numeric request-body fields must serialize as JSON numbers, not strings"
date: 2026-06-23
category: integration-issues
module: ls-sdk TR request modeling
problem_type: integration_issue
component: tooling
symptoms:
  - "Paper gateway returns HTTP 500 with rsp_cd=IGW40011 (undocumented) and a short error body"
  - "A sibling TR on the same path with the same auth works fine, so it is not credentials or routing"
  - "Every input-value variant for the failing TR returns the identical IGW40011 — it is not a value, it is the wire type"
root_cause: wrong_api
resolution_type: code_fix
severity: high
related_components:
  - tooling
tags: [ls-gateway, igw40011, serde, string-as-number, raw-probe, request-serialization, pagination-idx]
---

# LS gateway IGW40011 — numeric request-body fields must serialize as JSON numbers, not strings

## Problem

The LS-securities (Korea) paper gateway rejects a request with **HTTP 500 / `rsp_cd=IGW40011`** when a numeric request-body field is sent as a JSON **string** (`"idx":"0"`) instead of a JSON **number** (`"idx":0`). The SDK builds a valid Rust struct, serializes it, and the whole TR call fails with a generic 500 and an undocumented code — no hint that the wire *type* was wrong.

## Symptoms

- `make live-smoke-<tr>` (or any SDK call) fails; the failure classifier shows `http=500 rsp_cd=IGW40011`.
- `IGW40011` is **not** in `docs/design/ls-gateway-response-semantics.md` (which lists `IGW40013`/`IGW40014`/`IGW50008`) — there is no documented meaning to look up.
- A structurally similar sibling TR on the same `endpoint_path` (e.g. `t9905` next to `t1988`) returns `00000`, ruling out credentials, env, and routing.
- Changing the field *values* never helps — every broad/default variant returns the same `IGW40011`.

## What Didn't Work

- Treating `IGW40011` as a value/filter problem and sweeping input values (all `"0"`, empty strings, `mkt_gb="1"`, wide ranges) — every variant reproduced it identically, because the defect was the type, not the value.
- Modeling the field as a plain `String` (the default for most LS in-block fields) — that serializes as a quoted string, which is exactly what the gateway rejects for numeric slots.

## Solution

Model the field as `String` in Rust (LS values are handled as strings everywhere else) but serialize it as a JSON number with the `ls_core::string_as_number` serializer:

```rust
// WRONG — serializes as "idx":"0"  →  HTTP 500, rsp_cd=IGW40011
#[derive(Serialize)]
pub struct T3341InBlock {
    pub gubun: String,
    pub idx: String,          // quoted on the wire
}

// CORRECT — serializes as "idx":0  →  HTTP 200, rsp_cd=00000
#[derive(Serialize)]
pub struct T3341InBlock {
    pub gubun: String,                                       // genuine string field: leave as-is
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub idx: String,                                         // numeric slot: emit a JSON number
}
```

This session hit it on two fields: `t3341.idx` (the single-page body pagination cursor; first-page value `"0"`) and `t1664.cnt` (a plain row-count field — **not** a pagination field, which is why this generalizes beyond `idx`). Mirror `T1452` in `crates/ls-sdk/src/paginated/rank_screen.rs` for the paginated case.

**Diagnose it with the raw-HTTP probe**, not by rebuilding the SDK. `make raw-probe` issues one credential-safe POST and prints only `http` / `rsp_cd` / `body_len` (never the body), so you can A/B two body shapes in seconds:

```bash
# quoted idx → rejected
make raw-probe LS_PROBE_TR_CD=t3341 LS_PROBE_PATH=/stock/investinfo \
  LS_PROBE_BODY='{"t3341InBlock":{"gubun":"0","gubun1":"1","gubun2":"1","idx":"0"}}'
#   → result=[http=500 rsp_cd=IGW40011 ...]

# numeric idx → accepted
make raw-probe LS_PROBE_TR_CD=t3341 LS_PROBE_PATH=/stock/investinfo \
  LS_PROBE_BODY='{"t3341InBlock":{"gubun":"0","gubun1":"1","gubun2":"1","idx":0}}'
#   → result=[http=200 rsp_cd=00000 body_len=26641]
```

## Why This Works

LS is asymmetric about JSON types. **Response** fields arrive inconsistently as numbers or strings and are tolerated on the way in via `ls_core::string_or_number` (deserialize). **Request** numeric fields are strict: the slot must be a JSON number or the gateway rejects the whole call. `string_as_number` (serialize) parses the `String` and emits a bare number, satisfying the strict request contract while keeping the rest of the SDK's "everything is a String" model intact. The raw probe works because it bypasses the SDK's typed layer, so it isolates *wire shape* from *struct modeling* — the 500 reproduces (or clears) on the exact bytes you send.

## Prevention

- **Add an offline assertion that the field is a JSON number**, not a string, for every numeric request field:
  ```rust
  let v = serde_json::to_value(T3341Request::new()).unwrap();
  assert!(v["t3341InBlock"]["idx"].is_number(), "idx must serialize as a JSON number");
  ```
- When a new TR's spec marks a request field numeric (cursor, count, index, price/volume bound), reach for `#[serde(serialize_with = "ls_core::string_as_number")]` by default; only plain enum/`gubun` fields stay bare `String`.
- On any `IGW40011`, suspect wire type first: A/B the body with `make raw-probe` (quoted vs unquoted) before touching values.
- **Sibling gateway codes seen the same session:**
  - `IGW00201` ("호출 거래건수를 초과") = call-count / rate throttle, usually **self-inflicted** by a tight self-sourcing loop — pace it (`tokio::time::sleep`), don't treat it as a TR defect.
  - A TR that returns `IGW40011` for *every* request form (e.g. `t1988`) after the type is correct is environmental/provisioning → ship **PENDING**, don't flip.
- The *response-side* counterpart to this request-side learning — the normalized baseline hides an out-block's true wire key and array-ness, so read them from the raw capture and model array blocks as `Vec` via `de_vec_or_single` — has its own canonical home: `docs/solutions/conventions/tr-out-block-shape-from-raw-capture.md`. (A large `body_len` from the same raw probe is the tell that a "single"-looking block is a runtime array.)

## Related Issues

- `docs/solutions/architecture-patterns/ls-sdk-pagination-modeling.md` — the single-page body-`idx` pattern (the `string_as_number` rule for `idx` specifically); this doc is the gateway-error/diagnostic counterpart and generalizes it to all numeric request fields.
- `docs/solutions/integration-issues/makefile-include-env-quotes-gateway-403.md` — the sibling gateway-diagnostic case (surface the gateway body via a raw POST to isolate the layer).
- `metadata/PROVISIONALITY-LEDGER.md` §8 — records `t1988` PENDING on persistent `IGW40011` and the `t3341.idx` / `t1664.cnt` numeric-serialization notes.
