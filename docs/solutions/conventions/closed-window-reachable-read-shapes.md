---
title: "The paper gateway serves session-independent read shapes (historical chart, persistent designation board) NON-EMPTY while KRX is closed — these are flippable under closure"
date: 2026-06-26
category: conventions
module: ls-sdk Paper Live Smoke harness, implement-tr recipe, closed-window flip waves
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Scoping a flip wave while KRX is closed and deciding which Tracked TRs can certify without an open session"
  - "Judging whether a candidate read is 'closed-window reachable' (a curated per-TR call, not a metadata facet)"
  - "Deciding whether to run a read-only paper smoke autonomously or hand it to an operator"
tags:
  - paper-live-smoke
  - closed-window
  - reachability
  - implement-tr
  - flip-wave
  - krx-session
---

# Closed-window reachable read shapes

## Context

Most flip waves wait for an open KRX window because the certifying Paper Live
Smoke needs live data — quote, chart-session, and expected-index reads return
empty `00707` outside the regular session (see
[[market-hours-read-empty-result-disposition]]). The open question for a
**closed-window** wave is the inverse: are there read *shapes* the paper gateway
serves non-empty even when the market is shut?

The closed-window flip wave (plan `2026-06-26-003-feat-closed-window-flip-wave`)
answered this empirically. It scoped two genuine candidates by hand — `t1310`
(주식당일전일분틱, a historical tick/min chart pull) and `t1404`
(관리/불성실/투자유의, a persistent administrative-designation board) — and both
**certified non-empty under closure**:

- `make live-smoke-t1310` → `rsp_cd=00000`, **20 ticks**
- `make live-smoke-t1404` → `rsp_cd=00000`, **100-row board**

This is the opposite outcome from the night/overseas wave
([[night-overseas-paper-unavailable]]), where the paper gateway carried zero data
for night-derivatives and overseas-stock feeds (`00707`, 0 flips). The
distinction is the *read shape*, not the clock.

## Guidance

Two read shapes are **closed-window reachable** — the paper gateway serves them
non-empty regardless of the KRX session:

1. **Historical-bar pulls** — a read whose result is a stored time series the
   caller addresses by symbol + time cursor (`t1310`'s `cts_time`), not a live
   tick stream. The bars already exist; closure doesn't unmake them.
2. **Persistent status boards** — a read whose rows are a standing administrative
   list that survives across sessions (`t1404`'s 관리/불성실/투자유의 designations,
   keyed by `cts_shcode`). The board is served whether or not trading is open.

Contrast with shapes that need an open session and disposition to PENDING under
closure: live quotes (현재가), order books (호가), expected-index/expected-price
reads (예상지수/예상체결), and any realtime snapshot.

**"Closed-window reachable" is a curated per-TR judgment, not a metadata facet.**
All these candidates carry the same `venue_session: krx_regular` facet as the
session-gated reads — nothing in metadata marks a TR as session-independent. The
split is a hand-made call backstopped by the probe-then-disposition flow: assert
at least one modeled non-key field is non-empty **before** recording a flip
([[market-hours-read-empty-result-disposition]]), so a wrong guess dispositions
to PENDING cleanly instead of flipping falsely. Reachability stays unproven until
the smoke returns — both `t1310`'s chart-pull premise and `t1404`'s
board-served-under-closure premise were asserted, then confirmed.

**Read-only paper smokes can run autonomously.** A read smoke (`is_order: false`,
no mutation) is safe to run directly with `.env` paper credentials — there is no
order surface to guard. When a plan frames a read-flip step as "operator-gated,"
that framing carries over from the order-smoke autonomy discipline (real paper
orders need an attended operator + explicit opt-in); it does **not** mean a
read smoke needs a human. Run the read smoke, interpret the non-empty gate, flip
or disposition.

## Why This Matters

A closed-window wave that assumes "market shut → no data" leaves real flips on the
table. `t1310`/`t1404` flipped to Implemented during closure precisely because
their shapes don't depend on the session — work that would otherwise have waited
for an open window. Conversely, treating every Tracked TR as a closed-window
candidate wastes the six-site implement-tr cost on reads that will only return
`00707` until the bell. The shape test is what separates the two cheaply, before
spending a single gateway call.

## When to Apply

- Scoping a flip wave while KRX is closed: pull the Tracked-TR census and keep
  only historical-bar and persistent-board shapes as fresh candidates; route
  live-snapshot reads to wait for an open window.
- Deciding autonomy: run read-only paper smokes yourself; reserve the operator
  handoff for order smokes.
- Always pair the judgment with the non-empty-assert-before-record gate so a
  mis-scoped candidate dispositions to PENDING rather than flipping on an empty
  body.

## Examples

A closed-window smoke that certified (historical chart):

```
$ make live-smoke-t1310
LIVE-SMOKE target=live-smoke-t1310 inputs=[env=paper shcode=005930 date=2026-06-26] result=[rsp_cd=00000 ticks=20]
# 20 ticks under closure -> non-empty -> flip t1310 to Implemented
```

A persistent board that certified under closure:

```
$ make live-smoke-t1404
LIVE-SMOKE target=live-smoke-t1404 inputs=[env=paper gubun=0 jongchk=1 date=2026-06-26] result=[rsp_cd=00000 rows=100]
# 100-row designation board served outside trading hours -> flip t1404
```

The non-empty gate that protects a mis-scoped guess (the smoke harness asserts
before recording):

```rust
assert!(
    !resp.outblock1.is_empty(),
    "live-smoke-t1404: empty board (00707) — PENDING, not Implemented (R7)"
);
```

Had either returned empty `00707`, the assert would have dispositioned it to
PENDING — the same clean fallback the night/overseas wave hit on every probe.
