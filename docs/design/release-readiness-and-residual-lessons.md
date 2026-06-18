# Release Readiness And Residual Lessons

**Status:** maintained migration lesson note. Extracted from the
`korea-broker-sdk-ls` Migration Source to preserve production/readiness and
evidence-bucket lessons without preserving the old generated-surface release
gate.

This repository does not inherit the old full certification pipeline. Ordinary
work uses reviewed maintenance/expansion items, Change-Scoped Gates, Focused
Evidence for Recommended TRs, and advisory trackers. The lessons below are the
parts of the old release machinery worth retaining.

## Evidence Timing Buckets

The old repo separated evidence by the condition needed to collect it:

| Bucket | Meaning retained here |
|---|---|
| `session_independent` | Behavior should be testable without a market window. |
| `krx_regular` | KRX regular-session behavior needs an open KRX window. |
| `krx_night_derivatives` | KRX night derivatives behavior needs its own venue window. |
| `us_market` | Overseas/US behavior needs the relevant US market session. |
| `event_dependent` | Behavior needs a specific market, volatility, futures, or order event. |
| `paper_incompatible` | A live paper run returned LS's unsupported-work signal `01900`. |

These buckets are scheduling and interpretation aids. They must not silently
change severity: a failed row remains a failure unless reviewed; an independent
market-closed result is suspicious; a dependent market-closed or no-frame result
may be a timing observation rather than an SDK regression.

## Residual Discipline

The old generated-surface gate had a narrow lane for acknowledged residuals.
The maintained repo does not carry that mechanism forward, but it keeps the
discipline:

- A residual must be proven by a discriminating matrix or equivalent focused
  evidence, not by a single failed run.
- A residual remains a failure in ordinary classification.
- Any acceptance must name the TR, code, field or behavior, evidence path, date,
  reviewer, reason, and expiry or review trigger.
- Unknown signatures fail closed. A new literal, field, or code is not covered
  by an old acknowledgment.
- Residual-bearing release or recommendation claims are not clean passes; the
  claim must say what is excluded.

The old gate's match key for an acknowledged residual was the structural
signature `{tr_code, rsp_cd, field_id, error_class}` carried on the
failed-evidence row. `error_class` was a **closed** taxonomy with a fail-closed
`unknown` sentinel that matches no acknowledgment
(`non_numeric_in_numeric_slot`, `gateway_routing_error`, `non_api_error`,
`unknown`). The maintained repo does not carry the acknowledgment lane, but if a
residual-matching mechanism is ever rebuilt it must keep this shape — a closed
error-class set plus an `unknown` sentinel that fails closed — so a new signature
can never be silently treated as a known, accepted residual. (Provenance:
`korea-broker-sdk-ls/docs/certification_taxonomy.md`.)

This matches the extracted `COSOQ00201` lesson in
`docs/design/ls-gateway-response-semantics.md`.

## Production Readiness Lessons

The old production-readiness work is not a contract for this repo, but these
constraints remain valid:

- No automated real-money production order tests.
- Paper and production credentials must stay separate; LS REST does not provide
  a reliable server-side environment identity signal.
- Order-capable behavior requires explicit kill-switch, no-retry, dedup, and
  reconciliation design before runtime support.
- Live evidence is operator-initiated and credentialed; it is not default CI.
- Publish/release confidence should prefer boring local checks, docs checks,
  dependency/audit checks, and explicit live evidence records over implicit
  generated-surface promises.
- Python/Node bindings or other language surfaces need their own readiness bar;
  Rust readiness does not transfer automatically.
- Live Simulation / baseline evidence admitted for a release must be no older
  than **7 days** before tagging; release tagging should verify that the admitted
  baseline-evidence freshness window (7 days) is met, unless the release owner
  explicitly accepts the staleness. (Provenance:
  `korea-broker-sdk-ls/docs/RUST_RELEASE_CONFIDENCE_PLAN.md`,
  `docs/RELEASE_CHECKLIST_TEMPLATE.md`.)

## Convenience API Evidence Lessons

The current SDK is TR/class-oriented, so the old convenience API surface is not
carried as compatibility. One evidence lesson is still useful if convenience
helpers are added later:

- Convenience API evidence is separate from TR evidence. A helper can call a
  proven TR and still be wrong because it supplies stale defaults, loses a
  field, or changes shape.
- Local deterministic tests should prove helper request construction, default
  selection, and fixture freshness. Live smoke should prove gateway behavior.
- A stale fixture is not a live gateway failure. Treat stale fixture failures as
  test-data maintenance unless live evidence shows the same gateway behavior.
- Any future helper that wraps WebSocket subscription, quote, order, or account
  flows needs an explicit evidence seam before it can become Recommended.

## What Is Deliberately Not Carried

The following old mechanisms are intentionally not permanent architecture here:

- all-TR generated certification as the default release gate;
- Python evidence-pack scripts as maintained tooling;
- fixture-debt ratchets for a generated surface this repo no longer owns;
- release manifests as the source of truth for SDK support state;
- the old full generated API compatibility promise.

If any of those ideas become useful again, they must be reintroduced as reviewed
maintained-SDK decisions, not inherited by accident from the Migration Source.
