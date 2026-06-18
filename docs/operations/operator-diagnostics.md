# Operator Diagnostics

**Status:** maintained operator guide. Extracted from the
`korea-broker-sdk-ls` Migration Source so operational knowledge survives without
preserving the old generated-surface release process.

This guide describes what operators should be able to observe from the
maintained SDK, how to interpret common LS gateway symptoms, and which actions
avoid making trading or credential risk worse. It does not replace downstream
application risk controls, broker account procedures, or incident management.

## Diagnostic Fields

The SDK uses `tracing` fields rather than a typed diagnostics API. Downstream
log pipelines should treat these names as the stable operational vocabulary for
maintained surfaces.

| Area | Fields | Notes |
|---|---|---|
| REST dispatch | `tr_code`, `path`, `category`, `retry_attempt`, `http_status`, `rsp_cd`, `latency_ms` | `retry_attempt` is non-order only. |
| Auth | `hit`, `url` | `hit=false` means token refresh/fetch was attempted. `url` must not contain credentials. |
| Rate limiting | `category`, `wait_ms` | `wait_ms` is logged only when the wait is operationally visible. |
| Order dispatch | `tr_code`, `category`, `dedup_hit` | Deferred runtime. When implemented, `category` must be `Orders`; `dedup_hit=true` means no HTTP dispatch. |
| WebSocket lifecycle | `tr_cd`, `tr_key`, `reconnect`, `subscription_count`, `attempt` | Used for subscribe, reconnect replay, and reconnect failures. |
| WebSocket backpressure | `tr_cd`, `tr_key`, `channel_capacity`, `overflow_policy`, `dropped_count` | `dropped_count` is cumulative evidence of lost frames. |

### Field Compatibility Policy

These contracted field names are an operator-facing stability surface even when
Rust SemVer does not catch a change (the SDK exposes `tracing` fields, not a
typed diagnostics API):

- **Adding** a field is allowed in a minor release.
- **Removing or renaming** a contracted field requires a release-note callout.
  When removing, keep the field present (possibly empty) for one minor release
  cycle before removal in a major release; when renaming, keep the old name as an
  alias for one minor release cycle. The release-note callout format is fixed for
  removed / renamed / meaning-changed fields.
- **Changing a field's meaning** requires a Public API Boundary Review even when
  Rust SemVer does not detect it. Examples: `retry_attempt` moving from 1-based
  to 0-based, or `http_status` changing from an HTTP code to an application-level
  code. Adding a new value to a field's range (e.g. a new `category` value) is
  **not** a field-meaning change.
- A future typed diagnostics API (e.g. a `DiagnosticEvent` struct) is deferred
  and must be designed separately — it is not implied by this contract. There is
  no stable diagnostics schema today; contracted field names are pinned to the
  `tracing` `instrument` annotations.

(Provenance: `korea-broker-sdk-ls/docs/DIAGNOSTICS_CONTRACT.md`.)

## Redaction Guarantees

Diagnostics must not record:

- `appkey`
- `appsecretkey`
- `account_no`
- access tokens
- refresh or revoke tokens
- full request or response bodies by default
- span fields whose names contain `appkey`, `secret`, `token`, or `account`

Use `Debug` formatting for SDK config/error types so redaction impls apply. Do
not manually log raw config fields, raw WebSocket frames, OAuth responses, or
account-bearing order evidence. LS WebSocket ACK frames can echo bearer tokens,
so malformed-frame logging must use fixed messages without frame text.

The redaction impls are enforced, not advisory: `LsConfig`'s `Debug` redacts its
app key, confidential key, and account identifier; `TokenData`'s `Debug` redacts
its access token; generated credential structs redact their app key — each
verified by a `Debug`-output test or generator-invariant test. Instrumented
dispatch methods use `skip_all`, so no call parameter is auto-recorded into a
span. Note these redaction guarantees cover **credentials**, not account-level
response data (order numbers, account references), which is not auto-redacted —
review response content before logging or persisting it. (Provenance:
`korea-broker-sdk-ls/docs/DIAGNOSTICS_CONTRACT.md`, `docs/OBSERVABILITY_AND_DIAGNOSTICS.md`.)

## Auth Failures

**Signals:** `hit=false` in auth spans, token fetch errors, HTTP 401, or
`LsError::Auth` / `LsError::Http` from the first request that needs a token.

**Immediate action:** verify `LS_TRADING_ENV`, credential namespace, credential
presence, and network reachability to `/oauth2/token`.

**Diagnosis:** distinguish stale credentials, failed credential rotation, LS auth
gateway outage, missing env vars, and token expiry after idle time.

**Recovery:** rotate credentials through the downstream secret-management
process, build a new config/client, revoke the old token when possible, drop the
old client, and retry a low-risk read-only TR.

**Do not:** log tokens or credentials, rotate credentials during a likely
transient gateway issue without review, or run multiple same-account clients
without understanding their independent token caches.

## LS Business Errors

**Signals:** `http_status=200` with a non-success `rsp_cd`, returned as
`LsError::ApiError { code, message }`.

**Immediate action:** preserve `tr_code`, `rsp_cd`, and the broker message in
redacted structured logs.

**Diagnosis:** classify the likely cause as request-shape mistake,
account/permission state, market/session timing, stale fixture/default, or LS
gateway rejection. Use `docs/design/ls-gateway-response-semantics.md` for the
maintained response-code rules.

**Recovery:** fix request parameters or account permissions before retrying.
Non-order read calls may be retried when the error is known transient. Order
calls must never be blindly retried; reconcile first.

**Do not:** treat non-`01900` errors as paper-incompatible, log raw response
bodies, or assume `00000` is the only success code.

## Rate Limiting

**Signals:** repeated `rate_limiter::wait` events with sustained `wait_ms`, or
LS-side HTTP 429 responses.

**Immediate action:** reduce caller-side concurrency and pause non-essential
polling for the saturated category.

**Diagnosis:** SDK-side throttling appears as wait events before dispatch.
LS-side throttling appears as HTTP 429 from the gateway. The response determines
whether the application should lower its own rate or investigate LS account
limits.

**Recovery:** lengthen poll intervals, split high-priority workflows from noisy
market-data polling, and configure rate limits only when LS account limits are
understood.

**Do not:** raise local limits as a first response or allow market-data polling
to starve order/account workflows.

**Maintained defaults (configurable):** the per-category token-bucket quotas
default to 5/s (MarketData), 3/s (Orders), 1/s (Account), 1/s (Auth) —
`DEFAULT_MARKET_DATA_PER_SEC` / `DEFAULT_ORDERS_PER_SEC` and siblings in
`ls-core`, overridable per client config. On an LS-side HTTP 429 the REST
dispatch path retries up to 3 times (≤4 total calls) with exponential backoff;
sustained LS-side throttling needs a real rate reduction, not more retries.
(Provenance: `korea-broker-sdk-ls/docs/OPERATIONS_RUNBOOK.md`; values normalized
to the maintained `ls-core` defaults.)

## WebSocket Reconnect

**Signals:** connection-lost logs, reconnect attempts with `attempt`, reconnect
success with `subscription_count`, or terminal
`LsError::WebSocket("reconnect budget exhausted")`.

**Immediate action:** treat state derived from the affected stream as stale
during the reconnect window.

**Diagnosis:** brief reconnects that replay subscriptions are usually transient.
Budget exhaustion means subscriber streams terminate and local subscription
state has been cleaned up.

**Recovery:** after budget exhaustion, create new subscriptions and wire the new
streams into downstream processing. After a successful reconnect, rebuild any
state that cannot tolerate missed frames.

**Do not:** manually resubscribe during an active reconnect cycle or build an
unbounded external reconnect loop around the SDK's bounded reconnect behavior.

**Maintained reconnect budget:** auto-reconnect is bounded at 4 attempts with
1s/2s/3s/4s backoff (`RECONNECT_MAX_ATTEMPTS` in `ls-sdk` realtime); on
exhaustion every active subscriber receives the terminal
`LsError::WebSocket("reconnect budget exhausted")`. Alert when more than 2
reconnects occur within a 5-minute window — that indicates network or LS gateway
instability rather than a transient blip. (Provenance:
`korea-broker-sdk-ls/docs/OPERATIONS_RUNBOOK.md`.)

## WebSocket Backpressure

**Signals:** `dropped_count` or dropped-frame warnings for a `(tr_cd, tr_key)`.

**Immediate action:** treat stream-derived state as potentially stale and rebuild
from a read-only REST snapshot when the downstream decision requires current
state.

**Diagnosis:** repeated drops for one subscription mean the consumer is slower
than frame arrival. `DropNewest` preserves queued frames and loses incoming
frames; `LatestOnly` favors freshness but is not gapless. The per-subscriber
dispatch channel capacity defaults to 64 (`DEFAULT_WS_CHANNEL_CAPACITY` in
`ls-core`, configurable via `ws_channel_capacity`, minimum 1).

**Recovery:** move blocking work off the stream consumer, reduce subscription
count, batch downstream processing, or use periodic REST reads when gapless
state is required.

**Do not:** claim WebSocket delivery is gapless, log raw frames, or solve a slow
consumer only by hiding drops behind larger buffers.

## Credential Rotation

Rotate credentials by creating a new config/client from the new secret source,
revoking the old token when possible, and dropping the old client. Environment
selection is client-side: LS REST uses the same public host for paper and
production, and LS does not provide a reliable REST response field proving the
active environment. Operators must keep paper and production credentials
separate and verify `LS_TRADING_ENV` before any live evidence or order-capable
workflow.

## Emergency Order Disablement

Order runtime is deferred, but the operational rule is already fixed:
operators need a kill switch that disables all order dispatch before dedup,
rate limiting, or HTTP I/O. Once orders ship, emergency disablement should:

1. flip the runtime order switch off;
2. stop all order submission queues;
3. reconcile any ambiguous in-flight order using read-only order/account state;
4. confirm no retry loop is still submitting orders;
5. re-enable orders only after operator review.

The kill switch is not a reconciliation tool and reconciliation must not
silently re-enable it.

## Tracker Drift Findings

This repo's API Drift Tracker and Specification Document Tracker are advisory.
They surface findings for review; they do not mutate SDK code, metadata, docs,
evidence, or baselines on their own. Operators should treat a tracked
implemented/recommended TR drift finding as a maintenance review trigger, not as
automatic runtime failure.

