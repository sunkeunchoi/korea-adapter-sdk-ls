---
title: "A fault-tolerant fallback masked a wrong-endpoint bug as a chronic upstream 500"
date: 2026-06-22
category: integration-issues
module: crates/ls-trackers (api-drift fetch)
problem_type: integration_issue
component: tooling
symptoms:
  - "`make api-drift-fetch` warns `property-type mapping API failed (… HTTP 500 …); using fallback`"
  - "`property_type_fallback_served == true` on every run, indefinitely"
  - "`promote --type-only --dry-run` / `api-drift check` exit 2: `facts outage … re-fetch before comparing`"
  - "a `curl` of the endpoint returns 500 for every path/param/verb variant, looking like a chronic outage"
tags:
  - ls-trackers
  - api-drift
  - fetch
  - fallback
  - upstream-endpoint
  - misdiagnosis
  - browser-capture
---

# A fault-tolerant fallback masked a wrong-endpoint bug as a chronic upstream 500

## Context

The api-drift fetch resolves LS property-type codes (`A0001`…) to display names
(`String`, `Number`, …) via a doc-portal endpoint. The call is **fault-tolerant**: on
any failure it logs a warning and serves a hardcoded fallback table
(`property_type_fallback_served = true`), so the scrape never aborts. That fallback was
designed for a transient outage.

For the entire life of the feature the call returned **HTTP 500**. It was recorded
across `SEED-ATTESTATION.md`, `PROVISIONALITY-LEDGER.md` §4, the runbook, a brainstorm,
and a plan as a **chronic upstream outage** — and a whole "field-type re-pin, blocked on
system-codes recovery" workstream was built around waiting for it to heal. A `curl`
confirmed 500 on every path/param/verb variant, which *looked* like decisive proof of an
upstream fault.

It was not upstream. The code called the **wrong URL**:
`GET /api/codes/public/system-codes?groupCode=property_type` (500s for everyone) instead
of the real portal endpoint `GET /api/codes/public/property_type`. The response parser
expected `{ "list": [{code,name}] }` but the real shape is
`{ "codes": [{key,value}] }`, and the hardcoded fallback values were themselves wrong
(`A0004` was `Decimal`, really `Number`; `A0005` was `Binary`, really `Object Array`) —
so the baselines carried genuinely incorrect types, not merely "provisional" ones.

## Guidance

**A fault-tolerant fallback converts a hard bug into a silent, survivable degradation —
which is exactly what lets a wrong endpoint masquerade as a chronic outage.** Because the
fetch never failed loudly, "served the fallback" was read as "the endpoint is down"
rather than "our request is wrong," for weeks.

Two habits would have caught it on day one:

1. **A 500 is not proof of an outage — read the failure shape.** A well-behaved API
   answers a *wrong* request with 4xx (404 wrong path, 400 bad param, 405 wrong verb). A
   500 that is **identical for valid, param-less, and deliberately-misspelled requests**
   is a server handler that throws no matter what — which happens just as readily when
   *you* hit a route that does not exist as when the service is broken. Probe variants:
   if a clearly-wrong path 500s the same as the "right" one, suspect the path.

2. **Diff your request against what the official client actually sends.** The portal's
   own web UI fetched this data successfully. Driving a real browser to the page and
   capturing the network request (`agent-browser open <page>` then
   `agent-browser network requests --filter codes --json`) showed the true URL
   immediately — it differed from ours in both path shape and response schema. When an
   endpoint "works in the browser but not from our code," the answer is in the captured
   request, not in retrying the same one.

When a degraded-mode flag is stuck `true` indefinitely, treat that as a **bug signal**,
not a weather report: persistent fallback means "investigate the live call," not "wait."

## Why This Matters

- **Silent resilience hides the very failures it survives.** The fallback kept the
  pipeline green, so nothing forced a look at the request. A loud failure on the first
  fetch would have been fixed in minutes; the soft fallback cost an entire mis-scoped
  workstream.
- **A confident misdiagnosis propagates into permanent artifacts.** "Chronic upstream
  HTTP 500" was copied verbatim into five committed docs and shaped a plan. Each
  restatement made it look more established. Re-derive a claimed root cause from first
  principles before building process around it.
- **`curl`-returns-500 felt like proof but wasn't.** It confirmed *a* 500; it did not
  confirm *why*. The decisive evidence was the contrast (`OPTIONS` 200 = path registered;
  every doc sibling 200) and the browser's actual request — not the status code alone.

## When to Apply

- Any integration with a fallback / retry / circuit-breaker that lets a failed call
  continue in degraded mode — audit what a *permanently* tripped breaker is hiding.
- Any "the endpoint is down" conclusion drawn from status codes alone, especially 500.
- Any reverse-engineered / undocumented endpoint (here, the doc portal's internal
  frontend API): capture the real client's request rather than guessing the contract.

## Related

- [`gate-over-diff-inherits-diff-scope-blind-spot.md`](../architecture-patterns/gate-over-diff-inherits-diff-scope-blind-spot.md)
  — the *second* bug exposed once this fix produced the first non-fallback run: a promote
  integrity check that compared a deliberately-overridden field, hidden by symmetric
  fixtures. Both were latent because the live path was always blocked here.
- `crates/ls-trackers/baselines/api-drift/SEED-ATTESTATION.md` — the seed record,
  corrected 2026-06-22.
- `metadata/PROVISIONALITY-LEDGER.md` §4 — the field-`type` provisionality this fix
  retired.
- `docs/MAINTENANCE_RUNBOOK.md` → "Field-type re-pin" — procedure + the corrected
  root-cause note and endpoint health check.
