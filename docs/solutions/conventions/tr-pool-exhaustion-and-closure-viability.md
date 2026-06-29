---
title: "The raw TR pool is exhausted; a closed-window wave's yield is durable records, not a flip harvest"
date: 2026-06-29
category: conventions
module: TR support lifecycle (track-tr / implement-tr), Paper Live Smoke harness, metadata/PROVISIONALITY-LEDGER.md
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Planning another KRX-closed 'flip wave' to push TRs Tracked → Implemented"
  - "The unflipped pool is entirely dispositioned (no raw codes left, every Tracked-only TR carries a recorded reason)"
  - "Deciding whether to re-probe PENDING reads while the market is closed"
tags:
  - tr-lifecycle
  - closed-window
  - pool-exhaustion
  - pending
  - paper-live-smoke
  - provisionality-ledger
---

## Problem

Earlier waves harvested easy Tracked → Implemented flips from a large pool of raw
and freshly-tracked reads. That pool is gone. As of 2026-06-29 the workspace holds
**307 Tracked TRs, 278 Implemented, 0 raw remaining**, and the **29 unflipped TRs
are all dispositioned** — there is no undispositioned candidate left to "discover."
A wave that frames itself as "flip more TRs" under these conditions re-runs
known-empty sweeps and reports motion without yield.

## The accounting (the dead buckets)

The 29 unflipped TRs split three ways:

- **paper_incompatible (~12)** — gateway `01900` (`is_paper_incompatible()` fires)
  or a feed the paper gateway simply does not provision (clean `rsp_cd` + empty
  body, e.g. night-derivatives `CCENQ10100`/`CCENQ90200`, overseas-stock
  `g3101`–`g3190`, `t8455`/`t8460`). These **never flip on paper** — do not
  re-attempt them.
- **HELD — structural (4)** — blocked by a missing input or scope, not session:
  `t1852`/`t1856` need an unsourced ~27 KB `sFileData` blob; `t1860` is
  realtime-subscription *control*, not a read; `t3102` needs a news number whose
  only producer is the `NWS` realtime feed.
- **PENDING (~13)** — callable-but-empty / session-gated / off-window /
  input-unresolved (e.g. the chart-and-intraday reads `t1951`/`t1973`/`t2212`/
  `t2407`/`t8404`, plus `t1109`/`t1954`/`t2106`/`o3107`).

**Derive the exact PENDING set from current metadata**, not from a hard-coded list:
`support.implemented: false` minus the `paper_incompatible` facet minus the HELD
entries in `metadata/PROVISIONALITY-LEDGER.md`. The counts drift as TRs flip; the
predicate does not.

## The rule

**Under closure, the PENDING bucket is the wrong thing to chase.** It is dominated
by *session-gated* reads that return empty `00707` while the market is shut — and an
empty in-window precedent for a *different* TR family (daily/period charts that did
carry under closure, e.g. `t1310`/`t1404`/`t1514`) does **not** transfer to
execution/time-and-sales reads, which have no data when there are no executions. So
a closed-window wave's durable yield is the **records**, not the flips:

1. **This exhaustion record** — so the next wave does not re-litigate the dead
   buckets or re-run known-empty session-gated probes.
2. **Ledger-reason updates** — e.g. promoting `t3102` from "no REST source" to
   "feeder identified (`NWS`), awaiting a live news event."

Session-gated re-probes belong in an **open window**, where they can actually carry.
The chart/intraday reads carry Number/Object request fields (`cvolume`, `nmin`,
`cnt`) that need `string_as_number`; under closure a malformed-request `IGW40011`
rejection must not be mis-recorded as a closure/empty result.

## Dispositions recorded this wave (2026-06-29, KRX closed)

- **`t1631` — operator-gated, no code change.** Its structs, facade
  (`program_trade_summary`), `T1631_POLICY`, both crosscheck registrations, the
  `live_smoke_t1631` smoke, the Makefile target, and the smoke-map row already
  exist; it is `implemented: false` only because the smoke has not carried. Its
  request is **all-String** (no `string_as_number` applies) and its recorded
  blocker is gateway-side `IGW40014`, not the `IGW40011` numeric-serialize class —
  so it is not a request-shape defect. Operator runs `make live-smoke-t1631`; flip
  only if a modeled out-block returns non-empty, else it stays PENDING and the
  re-probe defers to an open window.
- **`t3102` — chained smoke staged, flip gated on a live frame.** `NWS` is
  Implemented and its 24-char `realkey` is the `sNewsno` feeder, so the chain
  `NWS.realkey → t3102.sNewsno` is the unblock path. `live_smoke_nws_t3102`
  subscribes `NWS`, captures a `realkey`, and threads it into `t3102`; the flip
  witness is a non-empty `t3102OutBlock2` title. The off-hours paper base rate for
  `NWS` may be ~zero, so a no-frame run is the HELD case (`SMOKE-FAIL`, not
  evidence). The WS leg is connection-reachable-only (fire-and-forget subscribe);
  only the `t3102` REST out-block justifies the flip.
