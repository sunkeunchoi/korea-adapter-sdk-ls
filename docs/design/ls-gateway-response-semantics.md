# LS Gateway Response Semantics

**Status:** maintained design note. Extracted from the `korea-broker-sdk-ls`
Migration Source so gateway response knowledge survives without preserving the
old generated-surface certification system.

This note records how the maintained SDK interprets LS Open API gateway result
codes. It is about gateway semantics, not release certification. The current
runtime implements the read-only subset described below; order-specific
semantics are deferred until the order runtime package ships.

## Runtime Success Predicate

For maintained non-order REST TRs, an LS response is successful when `rsp_cd` is:

| `rsp_cd` | Meaning | Maintained SDK outcome |
|---|---|---|
| empty or missing | No business error reported | `Ok` |
| `00000` | Normal success | `Ok` |
| `00136` | Inquiry completed, possibly with empty data | `Ok` |
| `00707` | Inquiry completed with no records | `Ok` |

All other `rsp_cd` values are business errors and surface as
`LsError::ApiError { code, message }`. The runtime preserves the exact code and
message instead of collapsing them into a generic failure.

`00136` and `00707` are intentionally successes. They mean the gateway processed
the query and returned a valid response envelope, even when the result set is
empty. A caller may still decide that an empty result is operationally
interesting, but it is not a transport or SDK error.

## Paper-Incompatible Signal

`01900` is the only maintained paper-incompatible response code.

When LS returns `01900` (`모의투자에서는 해당업무가 제공되지 않습니다`), the SDK still
surfaces `LsError::ApiError`; callers identify the special case through
`LsError::is_paper_incompatible()` or `ls_core::is_paper_incompatible`.

No other code should be treated as paper-incompatible by policy. The old
Migration Source briefly allowed per-TR paper-incompatible overrides for other
codes, then reversed that decision after credentialed remediation showed the
overrides hid real fixture, account, or gateway problems. That reversal carries
forward here: non-`01900` gateway failures stay failures until fixed, explicitly
accepted as residuals, or deferred by a reviewed maintenance decision.

## Hard-Failure Codes From The Migration Source

The old certification work identified these codes as hard failures, not
paper-incompatible skips:

| Code | Observed meaning | Maintained interpretation |
|---|---|---|
| `IGW40013` | Gateway query failure | Failure requiring investigation |
| `IGW40014` | Server-side value/type error | Failure requiring investigation |
| `IGW50008` | Gateway routing error | Failure requiring investigation |
| `405` | Permission rejection | Failure requiring investigation |
| `80001` | Account privilege/type issue | Failure requiring investigation |

These codes may have different root causes per TR: malformed request shape,
missing account privilege, stale fixture value, market/session timing, or an LS
gateway defect. The key rule is classification discipline: none of these become
paper-incompatible unless LS returns `01900`.

## Proven Residuals

A **proven residual** is an LS-side or account-side behavior that remains a
failure but has been investigated enough that maintainers can explain why an SDK
patch is not the immediate remedy.

The Migration Source's strongest example is `COSOQ00201`: the gateway returned
`IGW40014` with a non-numeric `002US` value in a numeric field. Follow-up
credentialed probes varied every controllable spec-valid input that could have
explained the literal, including the currency, balance-type code, and the
flagged `RecCnt` field itself. The gateway still reported `002US`, proving the
literal was server-derived rather than emitted by the SDK.

The maintained SDK does not inherit the old release-gate acknowledgment
machinery. It does inherit the reasoning rule:

- a residual must be proven by a discriminating matrix or equivalent focused
  evidence, not by one failed run;
- the failure remains a failure in ordinary classification;
- any decision to ship or recommend behavior with a residual must be explicit,
  attributed, scoped, and reviewable.

## Trading-Day And Empty-Data Signals

`01715` is the gateway symptom associated with date-sensitive inquiries when an
empty date defaults to a non-trading day. The current maintained SDK carries
this knowledge in `t8412` smoke/date handling. Future date-sensitive TRs should
pin explicit trading days in tests and evidence rather than relying on an empty
"today" default.

`00707` means the query completed with no records. It is not a failure by
itself. For account or order workflows, callers may still need to decide whether
an empty result is expected account state or a missing prerequisite.

## Order-Specific Codes

No order runtime ships today. The order package must not reuse the read-only
success predicate blindly.

The Migration Source observed successful domestic-stock order acknowledgements
using order-specific codes:

| `rsp_cd` | Meaning in old order tests | Required future treatment |
|---|---|---|
| `00039` | Sell order accepted/completed by the gateway | Success for the relevant order path |
| `00040` | Buy order accepted/completed by the gateway | Success for the relevant order path |

Order rejections remain errors. One old test used `01427` as a rejected-order
example (`모의투자 상/하한가를 확인하세요`).

Before any order TR becomes `implemented`, the order runtime package must define
a dedicated order success predicate, preserve order rejection codes as
`LsError::ApiError`, and keep order dispatch on the no-retry path described in
`docs/design/order-safety-design.md`.

## Tests And Drift Guard

Current maintained coverage:

- `ls-core` implements the read-only success predicate for empty/`00000`/`00136`/`00707`.
- `ls-core` preserves `01900` and exposes paper-incompatible helpers.
- `ls-sdk` slice tests exercise `01900` classification on maintained market data.
- Paper Live Smoke evidence records credential-free `rsp_cd` values for
  Recommended TRs.

Future extraction work should add focused tests or metadata checks when a
deferred code becomes load-bearing:

- hard-failure codes should remain failures in any future evidence classifier;
- order acknowledgement codes should be accepted only by order dispatch;
- residual acceptance, if rebuilt, must fail closed on unknown signatures and
  must not silently downgrade ordinary failures.

