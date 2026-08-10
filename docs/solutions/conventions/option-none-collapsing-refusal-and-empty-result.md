---
title: "An Option whose None means BOTH 'refused to act' and 'acted, found nothing' makes a refusal indistinguishable from evidence — return a three-state outcome, and read two tests both asserting is_none() as the smell"
date: 2026-08-10
category: conventions
module: nautilus-ls ingest (probe-lookback), any gated read whose gate can decline
problem_type: convention
component: tooling
severity: high
applies_when:
  - "Writing or reviewing a function that can decline to act AND can act-and-find-nothing, and returning Option<T> for both"
  - "A CLI or log line reports 'no data' and you are about to conclude something about the upstream source"
  - "Adding a gate, guard, or admission check in front of an existing fallible read that already returns Option"
  - "Reviewing tests where two cases on the same surface both assert is_none() or is_empty()"
tags:
  - api-shape
  - option-collapse
  - diagnostics
  - gated-read
  - test-smell
  - probe-lookback
related_components:
  - tooling
---

# An Option that collapses a refusal into an empty result

## Context

`Ingestor::run_probe_lookback_gated` (`adapters/nautilus/src/ingest/mod.rs:3407`) answers one
question: how deep does the vendor serve minute history? It has a calendar gate in front of it.
Before 2026-08-10 it returned `AdapterResult<Option<MinuteLookback>>`, and **two opposite
outcomes both produced `Ok(None)`**:

- the calendar refused the anchor — **zero gateway requests issued**, nothing measured;
- the walk ran against the gateway and the pilot served nothing — a real statement about supply.

`ls-ingest` could therefore only print one line for both: `probe: pilot 005930 served no minute
history — nothing recorded`. On a probe whose entire purpose is to measure how deep the vendor
serves, an operator reading that line after a refusal would conclude **"the vendor serves
nothing"** — the opposite of the truth, and unfalsifiable from the output alone.

This survived review and shipped because of how it was tested. Two tests pinned the behavior:
`probe_reports_nothing_when_pilot_serves_no_history` (`adapters/nautilus/tests/ingest.rs:1497`) and
`enforced_probe_unknown_anchor_makes_no_request` (`adapters/nautilus/tests/ingest.rs:3268`). **Both asserted
`out.is_none()`.** Each was individually correct and the pair was collectively blind: they
asserted the same thing for opposite reasons, so no test could fail if the two outcomes were
confused — which is exactly the confusion in the type.

## Guidance

**When a function can both decline to act and act-with-an-empty-result, those are different
return values, not the same one.** Return a closed enum naming each outcome, not `Option<T>`:

```rust
pub enum ProbeOutcome {
    /// The calendar refused the anchor: zero gateway requests, nothing recorded.
    /// Says nothing about what the vendor serves.
    CalendarStop,
    /// The anchor resolved and the walk ran, but the pilot served no history.
    /// This one IS a statement about supply.
    NoHistory,
    /// The pilot served history; the reading was recorded.
    Recorded(MinuteLookback),
}
```

(`adapters/nautilus/src/ingest/mod.rs:3292`, landed in PR #264.)

Three rules follow:

1. **Fix it in the library, not the print statement.** The binary cannot distinguish outcomes
   its dependency already merged — no amount of rewording the log line recovers the lost
   information. Put the discrimination where a test can assert it.
2. **Name what each variant does and does not license.** `CalendarStop` carries "zero gateway
   requests" in its own doc comment because the whole defect was a reader inferring supply from
   a refusal.
3. **Two tests asserting the same emptiness for opposite reasons is the smell.** Grep for it.
   When you find a pair, the type underneath them is almost certainly collapsing two outcomes.

## Why This Matters

The failure is silent and directionally wrong, which is the worst combination. A crash gets
fixed; a diagnostic that confidently reports the opposite of the truth gets **believed and acted
on**. Here the false conclusion — "the vendor serves no minute history" — would have been read
as a hard supply finding in an arc whose whole open question was supply depth.

It is also cheap to get wrong. `Option` is the path of least resistance for "might not produce a
value", and a gate added in front of an existing fallible read inherits its return type by
default. Nobody decides to conflate the outcomes; the type just absorbs the new one.

## When to Apply

- Adding a gate, admission check, or guard in front of an existing read that already returns
  `Option` or an empty collection — the new refusal path must not reuse the existing empty value.
- Reviewing any "no data / nothing found / empty" diagnostic that a human will act on. Ask: can
  this line be produced without the underlying source ever being consulted? If yes, it needs to
  say so.
- Writing tests for a surface with more than one way to produce nothing. If two tests both assert
  `is_none()`, add the distinguishing assertion or change the type.

Not applicable when the two paths are genuinely the same fact for the caller — a cache miss and
an absent key can legitimately both be `None` when no caller can act differently on them.

The refusing gate on the other side of this same function has its own trap — a precondition that
tests the boundary row while the scan walks a range — recorded in
[[proof-preserving-scan-precondition-must-test-the-gap]].

## Examples

**Verify the discrimination by mutation, because a passing test proves little here.** After the
fix, reverting only the Stop arm:

```rust
// mutation: collapse the two outcomes again
ProbeAnchor::Stop => return Ok(ProbeOutcome::NoHistory),
```

reds exactly two tests and no others:

```
test enforced_probe_unknown_anchor_makes_no_request ... FAILED
test enforced_probe_stop_leaves_a_prior_reading_untouched ... FAILED
test result: FAILED. 3 passed; 2 failed; 88 filtered out
```

That is the proof the tests are falsifiable. The same suite passed 93/93 before the mutation and
after reverting it.

**The third test earns its place separately.** `enforced_probe_stop_leaves_a_prior_reading_
untouched` (`adapters/nautilus/tests/ingest.rs:3303`) pre-writes a prior reading, triggers a refusal, and asserts
the artifact is byte-identical afterward. The absent-file case cannot catch a refusal that
clobbers an existing recorded value — and the recorded depth is what downstream session counts
derive from, so a silent overwrite would corrupt figures whose live source is gone.
