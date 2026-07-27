---
title: "A client-side HTTP timeout on the KRX daily endpoint manufactures a 'dead source' — the calendar candidate goes partial with ZERO witnesses and reads as 'KRX has no data'"
date: 2026-07-27
category: integration-issues
module: "adapters/nautilus calendar fetch (src/bin/calendar-fetch-inputs.rs: the single real HTTP client), calendar refresh/activate (src/calendar_refresh/)"
problem_type: integration_issue
component: tooling
severity: high
applies_when:
  - "`calendar-fetch-inputs` reports `source krx-daily ok=false covered=[] failed=error sending request`"
  - "A `calendar-refresh` candidate comes back `partial=true` with no new witnesses"
  - "Deciding whether to acknowledge `partial:source-failure` at activation"
  - "Any maintainer fetch against data-dbg.krx.co.kr appears to hang or fail"
related_components:
  - nautilus-ls-calendar
  - calendar-fetch-inputs
  - calendar-refresh
  - ls-ingest
tags:
  - krx-open-api
  - calendar-snapshot
  - http-timeout
  - partial-candidate
  - failure-attribution
  - witness-evidence
---

## Problem

`calendar-fetch-inputs` had a hardcoded 30-second request timeout. The KRX Open API's
`stk_bydd_trd` endpoint, under load, takes **14–59 seconds per day**. Every request aborted
client-side and was recorded as:

```
source krx-daily ok=false covered=[] failed=error sending request for url (https://data-dbg.krx.co.kr/...)
```

`krx-daily` is the **only** source that produces a *witness* — the evidence that proves a date
was a trading session. KASI supplies holidays and `krx-rule` supplies the weekday rule; neither
can prove a session happened. So the refresh produced a candidate that was `partial=true` with
**zero new witnesses**, in which the target dates still read `unknown`.

The failure is indistinguishable at the call site from "KRX has no data for this window".

## Symptoms

- `partial=true` on a refresh candidate, `high_risk=0`, and a diff containing nothing useful.
- The dates you refreshed *for* still read `unknown` in the candidate.
- Re-running produces the identical result, because the checkpoint faithfully resumes a source
  that never succeeded.

The trap: `partial:source-failure` is an *acknowledgeable* key at activation. Acknowledging it
and activating would consume the genesis→successor chain transition, change `artifact_id`
(invalidating any attended Unknown-override authored against the old one), and leave every
consumer exactly as blocked as before.

## What Didn't Work

**Blaming the credential.** The natural first read of a hanging authenticated request is a bad
or unapproved key. It was not.

**Blaming the host.** `data-dbg.krx.co.kr` looked unreachable — a `/dev/tcp` probe reported the
port closed and curl returned "0 bytes received". Both were misleading: TCP connect actually
succeeded in ~15 ms.

**Trusting the run's own summary.** `ok=false ... failed=error sending request` names a
transport failure, which reads as the *server's* fault. The client hung up first.

## Solution

Diagnose by **varying one thing at a time against the same endpoint** and reading the timing,
not just the status:

| probe | result | what it proves |
|---|---|---|
| no `AUTH_KEY` header | `401` in 0.02 s | host is alive, reachable, and auth-gating works |
| deliberately bogus key | `401 Unauthorized Key` in **13.9 s** | the endpoint is *severely degraded*, not down |
| real key, 20–45 s budget | hangs, 0 bytes | inconclusive — budget too small |
| real key, **240 s** budget | `200`, 943 rows | the key is fine; we were timing out |

The 401-without-a-key is the load-bearing probe: it separates "unreachable/blocked" from
"reachable but slow", which is what every other symptom was ambiguous about.

Then raise the timeout and resume — the checkpoint means only un-fetched days cost anything:

```sh
LS_CALENDAR_HTTP_TIMEOUT_SECS=180 cargo run --release --bin calendar-fetch-inputs -- \
  --window <from..through> --krx-through <last closed session> \
  --inputs-out state/<...>.json --state state/<...>.ckpt --pace-ms 500
```

Success looks like `source krx-daily ok=true covered=[...]`, and the resulting candidate is
`partial=false` with `status_established` / `new_evidence` diff entries naming each date.

## Why This Works

A bounded read timeout is correct hardening, but a bound set *below the source's real latency
ceiling* does not harden anything — it converts a slow success into a fabricated failure, and
the fabrication is indistinguishable from real absence at every layer downstream. The timeout
is now `LS_CALENDAR_HTTP_TIMEOUT_SECS` (default **120**, above the observed 59 s ceiling;
clamped to 600 so a typo cannot disable the bound), the effective value is echoed at startup,
and `reqwest::Error::is_timeout()` labels the case explicitly:

```
client-side timeout after 120s — the source did NOT refuse; raise
LS_CALENDAR_HTTP_TIMEOUT_SECS and re-run (the checkpoint resumes)
```

## Prevention

**Never let a client-side abort be reported as a source failure.** They have opposite remedies
— wait/retry versus raise the bound — and conflating them costs a full operator cycle. Any
place a transport error is stringified into a persisted "reason", branch on `is_timeout()` /
`is_connect()` first.

**Check what a `partial` candidate is missing before acknowledging it.** `partial:source-failure`
is acknowledgeable by design, which makes it easy to wave through. Read the candidate's rows for
the dates you refreshed *for*; if they are still `unknown`, the refresh achieved nothing and
activating it spends a chain transition for no evidence:

```sh
python3 -c "
import json; c=json.load(open('state/krx.calendar.json.candidate'))
rows={r['date']:r for r in c['rows']}
for d in ['<target dates>']: print(d, rows[d]['status'], rows[d]['decisive_evidence'])
"
```

**Know which source carries which evidence class.** Only `krx-daily` yields witnesses; a run
where KASI and `krx-rule` succeed and `krx-daily` fails will look two-thirds healthy and prove
nothing about whether a day traded.

**Set a maintainer-fetch timeout from the source's measured p-max, not from a general-purpose
default.** A bulk, resumable, operator-attended fetch is not latency-sensitive; `connect_timeout`
still guards a genuinely dead host.

## Related

- [`krx-session-status-is-retrospective-only-unknown-is-not-a-defect`](../architecture-patterns/krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md)
  — why today always reads `unknown`, which is a *different* cause of the same status
- [`mount-universe-producer-cannot-be-fed-on-a-session-morning`](../architecture-patterns/mount-universe-producer-cannot-be-fed-on-a-session-morning.md)
  — the downstream consumer blocked by a stale snapshot
- `adapters/nautilus/RUNBOOK-calendar-snapshot.md` §G2 — the fetch step and its resume semantics
