---
title: "The raw TR pool is exhausted; a closed-window wave's yield is durable records, not a flip harvest"
date: 2026-06-29
last_updated: 2026-06-30
category: conventions
module: TR support lifecycle (track-tr / implement-tr), Paper Live Smoke harness, metadata/PROVISIONALITY-LEDGER.md
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Planning another KRX-closed 'flip wave' to push TRs Tracked → Implemented"
  - "The unflipped pool is entirely dispositioned (no raw codes left, every Tracked-only TR carries a recorded reason)"
  - "Deciding whether to re-probe PENDING reads while the market is closed"
  - "An open-window wave already probed the same residue earlier the same day"
  - "Tempted to flip metadata to implemented:true offline because the gate stays green"
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

- **`t1631` — PENDING, gateway-side defect (live-confirmed 2026-06-29).** Its
  structs, facade (`program_trade_summary`), `T1631_POLICY`, both crosscheck
  registrations, the `live_smoke_t1631` smoke, the Makefile target, and the
  smoke-map row already exist; it is `implemented: false` because the smoke does
  not carry. Its request is **all-String** (no `string_as_number` applies) and its
  blocker is gateway-side `IGW40014`, not the `IGW40011` numeric-serialize class —
  so it is not a request-shape defect. The live smoke reproduces it exactly:
  `IGW40014: id=[매수수량(bidvolume)] in.data=[@..]` — the gateway fails to parse
  **its own response** field `bidvolume`, whose bytes come back as garbage
  (a literal `@`), "neither a decimal digit ... nor e-notation." This is a defect
  in what the gateway *returns*; neither the SDK nor session timing can fix it.
  **Do not re-attempt `t1631` as a flip candidate** — it is permanently PENDING
  pending an upstream gateway fix.
- **`t3102` — chained smoke staged, flip gated on a live frame.** `NWS` is
  Implemented and its 24-char `realkey` is the `sNewsno` feeder, so the chain
  `NWS.realkey → t3102.sNewsno` is the unblock path. `live_smoke_nws_t3102`
  subscribes `NWS`, captures a `realkey`, and threads it into `t3102`; the flip
  witness is a non-empty `t3102OutBlock2` title. The off-hours paper base rate for
  `NWS` may be ~zero, so a no-frame run is the HELD case (`SMOKE-FAIL`, not
  evidence). The WS leg is connection-reachable-only (fire-and-forget subscribe);
  only the `t3102` REST out-block justifies the flip.

## The next escalation: re-confirmation, and why you cannot flip offline (2026-06-30)

By 2026-06-30 the residue had grown to **41 Tracked-not-Implemented TRs** (up from
29 — the order-surface track wave added 13 `owner_class: orders` F/O + overseas
order TRs, 307 → 320 maintained). Two refinements harden the rule above:

**1. A same-day open-window probe makes a closed-window sweep pure re-confirmation.**
When an earlier wave *that same day* probed the residue under a live session and
recorded it empty (here: the §19 open-window wave settled the intraday F/O/ELW feeds
`t1951`/`t1973`/`t2212`/`t2407`/`t8404`/`t8427`/`t2106` as PENDING paper-empty
"regardless of session"), a closed-window re-probe **cannot beat an in-window
empty** — it only re-derives the same result. The honest disposition is to
*re-confirm* and cite the same-day evidence, not re-probe. Distinguish **net-new**
dispositions (a TR moved from implicit/stale to a freshly-evidenced terminal reason)
from **re-confirmations** (a label already current in metadata): a sweep whose entire
output is re-confirmation is a complete, successful wave, but call it what it is so a
pure-reconfirmation pass is visible rather than counted as fresh progress.

**2. You cannot land a flip offline — the gate does not run live smokes.** The full
gate (`make docs` / `cargo test` / `cargo test -p ls-core` / `make docs-check`) never
executes `make live-smoke-<tr>` (the smoke is `#[ignore]`d). So flipping a TR's
metadata to `support.implemented: true` offline produces a **green-but-uncertified**
flip: the gate passes, the docgen `reference.len`/`banner_trs` move, but no non-empty
deserializable witness was ever observed. This is the certification gap that the
"finish-the-flip is one operator step" affordance
([[realtime-flip-artifacts-stageable-before-metadata-flip]]) leaves open in the
*other* direction — staging artifacts while `implemented:false` is safe and green;
flipping to `implemented:true` without the passing smoke is dishonest, not safe. An
autonomous closed-window run therefore lands **0 flips by construction** and hands
the live/timing-gated probes (after-hours `t1109`; credential-lane account reads
`o3107`/`o3127`/`t0441`/`CSPBQ00200`) back to an operator. 0 flips with a complete,
honestly-dated ledger is the successful outcome.
