---
title: "Five 'closed-window candidate' TRs are re-pointed to closure-independent blockers, not re-probed — t1852/t1856 (sFileData), t3102 (sNewsno), t1964 (empty-board), t1860 (realtime-control)"
date: 2026-06-26
category: conventions
module: ls-sdk Paper Live Smoke harness, implement-tr recipe, metadata facets
problem_type: convention
component: tooling
severity: low
applies_when:
  - "Scoping a closed-window (KRX-closed) flip wave and tempted to re-probe the prior wave's 'session-independent' candidate list"
  - "Deciding whether t1852/t1856/t3102/t1964/t1860 are fresh flip candidates or already-dispositioned blockers"
  - "Confirming a re-pointed TR's metadata/ledger row is intentionally left intact (no smoke, no flip) for this wave"
tags:
  - paper-live-smoke
  - closed-window
  - disposition
  - implement-tr
  - sfiledata
  - realtime
---

# Closed-window flip-candidate re-points (don't re-probe these five)

## Context

A KRX-closed flip wave (plan `2026-06-26-003-feat-closed-window-flip-wave`) can
only certify reads the paper gateway plausibly serves while the market is closed.
Of the seven TRs once grouped as "session-independent closed-window candidates",
only **`t1310`** (today/prev tick-or-min chart) and **`t1404`** (administrative-
designation board) are genuine fresh greenfield flips — all-string request fields,
no session prerequisite, built this wave.

The other **five carry blockers that closure does not lift.** Re-probing them under
closure only re-confirms a prior disposition for no new count. They are
**re-pointed to their real unblock paths, not re-smoked.** This note exists so a
future closed-window wave does not re-litigate them as fresh candidates.

## Guidance

Leave each of these five `support.implemented: false` with its metadata and
`metadata/PROVISIONALITY-LEDGER.md` row intact. Author no smoke and run none this
wave. The real blocker and unblock path per TR:

| TR | Real (closure-independent) blocker | Unblock path |
|----|------------------------------------|--------------|
| `t1852` | Requires a ~26.8 KB `sFileData` screening blob as request input — the SDK has no producer for it. | An `sFileData`-sourcing wave (build the screening-blob producer first). |
| `t1856` | Same `sFileData`-input blocker as `t1852`. | Same `sFileData`-sourcing wave. |
| `t3102` | Already built (struct + policy + facade + smoke), but input-blocked: its `sNewsno` key is sourced only from the realtime NWS WebSocket feed, so it cannot be smoked over REST. | A realtime-NWS producer that yields a live `sNewsno`. |
| `t1964` | Already built (prior wave); left unflipped on a documented empty-board disposition (ELW전광판 returns empty on the position-less paper board). | An empty-board filter-default fix that yields a non-empty board. |
| `t1860` | A realtime-control subscription (`서버저장조건 실시간검색`), not a plain read — HELD as out of scope for read-only implement-tr. | The realtime-control effort (subscription lifecycle, not a REST flip). |

Closure changes none of these. The `t1310`/`t1404` reachability premise is itself
unproven (the gateway may gate even historical/board reads on an open session) —
but that is gated by the non-empty-assert-before-record rule
([[market-hours-read-empty-result-disposition]]), which dispositions an empty
closed-window smoke to PENDING cleanly rather than flipping it falsely.

## Why this matters

The honest yield of a closed-window wave is ~1–2 flips (`t1310`/`t1404` only), and
only if the gateway serves them under closure. Spending the six-site implement-tr
cost (see [[implement-tr-registration-sites]]) on the five re-pointed TRs would buy
nothing — their blockers are structural, not timing. Recording the re-point once,
here, keeps the next wave from re-paying the probe cost to rediscover it.
