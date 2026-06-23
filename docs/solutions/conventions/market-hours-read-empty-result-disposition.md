---
title: "Disposition a market-hours-dependent read's empty paper smoke by the session clock, not as a shape failure"
date: 2026-06-23
category: conventions
module: ls-sdk Paper Live Smoke harness, implement-tr / track-tr recipes
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Smoking a TR whose metadata is session_class: dependent / dependency_reason: market_hours (a realtime quote/board/expected-index read)"
  - "A make live-smoke-<tr> returns rsp_cd=00707 (or a success code with an empty out-block) outside the KRX regular session"
  - "Deciding whether an empty smoke result means DROP (TR defect), PENDING, or re-run"
tags:
  - paper-live-smoke
  - market-hours
  - session-dependent
  - implement-tr
  - track-tr
  - disposition
related_components:
  - tooling
---

## Context

The five sector TRs in Wave A (`t8424`/`t1511`/`t1514`/`t1516`/`t1485`) — and most
domestic read-only TRs — are marked `session_class: dependent` /
`dependency_reason: market_hours` in the migration source. Their live data is only
meaningful during the KRX regular session (~09:00–15:30 KST). A `make
live-smoke-<tr>` run off-hours can return a **success** `rsp_cd` (`00000` or the
empty-set code `00707`) with an **empty** out-block. The Implemented gate wants
"success + non-empty + deserializes," so an empty result looks like a failure — but
off-hours it is not evidence of anything about the TR's shape.

## Guidance

Treat an empty result from a market-hours-dependent read as a **session-clock
question first, a TR question second**. Record the KRX session clock at every smoke
run and disposition mechanically — never as a judgment call:

1. **Empty + off-hours** → **not a valid attempt.** Re-run during the KRX regular
   session before any verdict. Do not DROP and do not flip anything.
2. **Empty + in-window** → **PENDING** with a concrete reason (e.g. "in-window
   empty board"); keep the TR's provisional ledger rows so nothing ships with a
   stale "re-verify" instruction.
3. **Non-empty + in-window + deserializes** → **IMPLEMENTED.**
4. **A `00707` (empty success) still deserializes** — only the non-empty *arm*
   fails the gate. So an empty result is a deserialize success: it proves the wire
   shape parses, it just doesn't prove non-emptiness. Don't classify it as a shape
   defect.

A genuine TR defect (DROP) is when the **raw HTTP probe succeeds but the SDK
deserialize fails** — diagnose with `make raw-probe LS_PROBE_TR_CD=.. LS_PROBE_PATH=..
LS_PROBE_BODY=..` (credential-safe; prints only `http`/`rsp_cd`/`body_len`), not from
an empty in-window smoke.

## Why This Matters

Without the session-clock branch, an off-hours empty smoke gets misread as a shape
failure and a perfectly callable TR is dropped to tracked-only — or worse, the wave
"can't flip anything" and re-scopes when the only real blocker was the wall clock.
Conversely, recording an off-hours empty as Implemented would ship an unverified
non-empty claim. The session clock is the only thing that distinguishes "the market
is closed" from "the TR is broken," and `rsp_cd=00707` looks identical in both cases.
The ship-floor for a wave of these reads is therefore an **in-window** flip of at
least one member, never an off-hours result.

## When to Apply

- Any TR whose metadata carries `venue_session: krx_regular` and `session_class:
  dependent` — realtime snapshots, expected/auction screens, per-sector boards.
- Most acute for expected/auction screens (e.g. `t1485` 예상지수), which are the most
  likely to be empty off-session even when perfectly callable.
- Historical/period reads (e.g. `t1514` 업종기간별추이) and static-ish list reads
  (e.g. `t8424` 전체업종) are usually non-empty regardless of the clock — they are the
  members most likely to flip window-free and make good anchors.

## Examples

Mechanical disposition recorded at smoke time:

```
# off-hours (KST 18:40) — NOT a valid attempt, schedule a re-run
make live-smoke-t1485   # -> rsp_cd=00707 rows=0
#   disposition: re-run in KRX session (do not DROP, do not flip)

# in-window (KST 14:22) — valid attempt
make live-smoke-t1485   # -> rsp_cd=00000 rows=61   -> IMPLEMENTED
make live-smoke-t1511   # -> rsp_cd=00000 ...         -> IMPLEMENTED
#   (an in-window rows=0 here would be PENDING, not DROP)
```

The same `00707` returned off-hours and in-window means two different things; only
the recorded session clock tells them apart. See also
`docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`
(the raw-probe classifier that separates a real defect from environmental noise) and
`docs/solutions/conventions/sdk-struct-field-from-baseline-korean-name.md`.
