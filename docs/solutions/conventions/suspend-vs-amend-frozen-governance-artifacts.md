---
title: Suspend vs amend — when a frozen governance artifact goes stale, re-derive only if the re-derived value has a consumer
date: 2026-07-31
category: docs/solutions/conventions
module: adapters/nautilus/lab/config
problem_type: convention
component: tooling
severity: medium
applies_when:
  - a frozen pre-registration value (band, breaker, threshold) is invalidated by a re-measurement
  - choosing between a recorded stand-down and a re-registration amendment
  - the honest re-derived value would forbid the activity it exists to gate
tags:
  - pre-registration
  - re-registration
  - stand-down
  - governance
  - expectation-band
  - production-ladder
  - transaction-cost
  - orb-lab
---

# Suspend vs amend — when a frozen governance artifact goes stale, re-derive only if the re-derived value has a consumer

## Context

The Production Ladder freezes its economic values in `adapters/nautilus/lab/config/preregistration.json`,
amendable only by a recorded re-registration dispatch (KTD1). On 2026-07-31 the
transaction-cost model re-measured the head net-negative (v35, net RoR −0.0006), leaving
the frozen v2 rung-1 expectation band — derived from the zero-cost v34 distribution —
citing a number the head could no longer clear in expectation
(`rung1-prereg-band-zero-cost-inheritance`). Two honest resolutions existed: amend to a
cost-aware v3 band, or record a stand-down. Both are protocol-conformant; the question is
which one to pick.

## The convention

When a frozen governance artifact is invalidated, do not default to re-deriving it.
First run the **no-consumer test** on the would-be amendment:

1. **Who consumes the re-derived value?** A band exists to gate an activity. If the
   honest re-derived value would forbid that activity anyway (a band centered on a
   negative edge only authorizes sessions the measurement says not to run), the
   amendment's only function is to make forbidden activity look governed. That is a
   stand-down wearing amendment clothes — record the stand-down instead.
2. **Would the system's own reset rules discard what the amendment enables?** The
   ladder's `code_change_resets_to_rung_1` means the head change required to make the
   edge positive again resets the ladder and discards the current epoch's live
   evidence. Calibration data collected under the amended artifact would have no
   surviving consumer either. If the unblock condition itself invalidates the epoch,
   amendment buys nothing durable.
3. **Only if a consumer survives both tests, amend** — re-derive with the frozen
   formula, reproduce the derivation in the guard test, and move every citation site
   in the same commit.

A stand-down is itself a recorded governed act, not an absence of one: the frozen file
stays byte-identical (every existing dispatch hash citation remains valid), and the
suspension is recorded where the next operator will look — the turn log, the runbook
and preflight banners, the rationale doc's status line, and a parked queue item whose
notes name the unblock condition and the re-entry protocol.

## Applied instance (PR #238)

The 2026-07-31 stand-down: no v3 band derived; `preregistration.json` untouched at v2;
re-entry parked as `rung1-ladder-reentry-net-positive-head` — a net-positive cost-aware
head triggers a fresh v3+ re-registration (Protective formula, `prereg_derivation.rs`
extended, `RUNG1-PREFLIGHT.md` step-3 literals moved in the same commit) before any
genesis dispatch. The rejected amendment's stated payoff — live calibration of the
rung-2 tracking band and of the cost model — failed both tests: rung-2 is unauthorizable
on a negative edge, and the head change a positive edge requires discards the epoch's
rung-1 evidence.

## Prevention

- Treat "keep the instrument alive for calibration" as a claim to verify, not a reason:
  name the concrete consumer of the calibration output and check it survives the
  system's own reset rules.
- Whichever arm is chosen, it is recorded, never implied — an unrecorded edit to the
  frozen mirror (even its `_note` prose) is the violation the protocol exists to
  prevent.
- See `backtest-derivable-vs-live-calibrated-bands.md` for the upstream question this
  convention follows: whether the value should have been frozen from that data source
  at all.
