---
title: "A proof-preserving scan stops at the first Unknown instead of stepping past it — so its precondition must test the whole gap to the nearest proven anchor, never the boundary row alone"
date: 2026-08-10
category: conventions
module: nautilus-ls ingest calendar gate (select_recent_session / probe_anchor), any backward scan that refuses on uncertainty
problem_type: convention
component: tooling
severity: high
applies_when:
  - "Writing a precondition or run-day gate in front of a backward scan that can refuse"
  - "A guard reports the target date/row is valid and the downstream operation still declines"
  - "Reviewing a check that tests one boundary value against a scan that walks a range"
  - "Reasoning about whether an operation can run today, where 'today' is derived rather than supplied"
tags:
  - precondition
  - calendar-gate
  - proof-preserving
  - boundary-vs-range
  - probe-lookback
related_components:
  - tooling
---

# A proof-preserving scan's precondition is the gap, not the boundary

## Context

`CalendarGate::probe_anchor` (`adapters/nautilus/src/ingest/mod.rs:387`) delegates to
`select_recent_session`, which walks backward from an anchor date looking for the most recent
proven trading session. Its termination is deliberately **proof-preserving**
(`adapters/nautilus/src/ingest/mod.rs:343`):

```rust
match row.status {
    DayStatus::TradingSession => return Some(row.date),
    DayStatus::Closed => {}          // skip and keep walking
    DayStatus::Unknown => return None, // refuse — never step past uncertainty
}
```

An `Unknown` does not pause the walk; it ends it. The scan will not step *over* an unproven day
to reach a proven one further back, because doing so would manufacture an anchor from an
unestablished fact.

A gate was written for this — refuse to start an expensive operation unless the probe's anchor
is establishable — and it tested **the anchor row itself**: "is the civil date the operation will
anchor on already proven `closed` or witnessed?" On 2026-08-10 that check passed and was wrong.
The anchor resolved to Sunday `2026-08-09`, a legitimately proven `closed` row. The gate said
go. The walk then went `08-09` closed → `08-08` closed → `08-07` **`unknown`** → `None`. The
operation refused, having spent the gate's entire purpose.

The gate and the scan were asking different questions. The gate asked "is the boundary sound?"
The scan asks "is *every* step from the boundary to the nearest proven anchor sound?"

## Guidance

**When the downstream operation is a scan that refuses on uncertainty, the precondition must
cover the same range the scan will traverse — not its starting point.**

Concretely, for a backward scan:

1. Compute the anchor exactly as the operation will (see "When to Apply" — often the operation
   derives it internally and will not accept yours).
2. Enumerate every row between the last proven point and that anchor.
3. Refuse only on rows that are *unestablishable*, not on rows that are merely currently
   unproven — the difference is whether the run can fix them. Here, past weekdays have
   retrospective witnesses that can be fetched; the current session's witness cannot exist yet,
   so a gap containing today is the genuine refusal.

The corrected gate reads: *refuse unless every `unknown` day between the last witness and the
anchor is establishable — none of them being the current session.*

**State the post-condition separately.** The gap check licenses *starting*; it does not prove the
scan will now succeed. Re-run the actual walk after the establishing work lands, and confirm the
first proven row reached is the one you expected.

## Why This Matters

A boundary-only precondition on a range-scanning operation fails in the most expensive direction:
it passes, so the caller commits the work the gate existed to protect, and *then* the operation
refuses. The gate converts from a safeguard into pure overhead.

It is also very easy to write. The boundary is the value in hand — the anchor, the target date,
the requested row — and checking it feels like checking the thing. The range is implicit in the
callee, one level down, and invisible at the call site.

The failure mode is quiet: nothing errors. The gate logs a pass and the operation logs a refusal,
and the two are consistent with each other under the wrong mental model.

## When to Apply

- Any guard in front of a scan whose termination condition is "stop on uncertainty" rather than
  "skip uncertainty" — the two produce opposite answers on exactly the inputs a guard exists for.
- **Especially when the operation derives its own anchor.** `run_probe` computes
  `last_closed_session(now_kst, ACCUMULATE_CLOSE_BUFFER)` internally
  (`adapters/nautilus/src/bin/ls-ingest.rs:444`) and takes no operator override, so a gate that
  checks a date the *caller* chose is testing a value the callee will never see. Derive the
  anchor the same way the callee does, or the check is untethered.
- When the derived anchor moves on a clock boundary. The same buffer means the anchor advances at
  16:30 KST, so a long operation can start inside the gate and finish outside it. Re-derive after
  any step that could cross the boundary.

Not applicable to a scan that skips uncertainty and continues — there, a boundary check is
sufficient because the intervening rows cannot refuse.

A refusal from this gate also has to stay legible downstream: when the refusing path and an
empty-result path share a return value, the refusal reads as evidence. See
[[option-none-collapsing-refusal-and-empty-result]], which is the same function's other half.

## Examples

Boundary-only check — passes, and the operation still refuses:

```python
anchor = last_closed_session(now, buffer)     # 2026-08-09 (Sunday)
rows[anchor] in ("closed", "trading_session") # True  -> GATE PASS
# walk: 08-09 closed -> 08-08 closed -> 08-07 unknown -> None  -> operation REFUSES
```

Gap check — refuses for the right reason, or clears the run honestly:

```python
anchor  = last_closed_session(now, buffer)             # 2026-08-09
gap     = [d for d in days_between(last_proven, anchor) if rows[d] == "unknown"]
                                                        # ['2026-08-05','-06','-07']
blocked = [d for d in gap if d == current_session]      # [] -> establishable
# GATE PASS, and the establishing fetch makes the walk succeed:
# 08-09 closed -> 08-08 closed -> 08-07 trading_session -> RESOLVES
```

The distinction that makes the gap check correct is `d == current_session`: those three unknown
weekdays were *establishable* (their retrospective witnesses already existed and one fetch
proved all three), whereas the same gate on a weekday evening would find today in the gap and
correctly refuse — today's witness cannot exist yet, so no amount of fetching would clear it.
