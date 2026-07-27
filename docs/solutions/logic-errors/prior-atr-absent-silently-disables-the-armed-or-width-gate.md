---
title: "prior_atr is schema-OPTIONAL but head-fidelity-MANDATORY — an ATR-less universe row silently disables the armed OR-width gate and emits no reject envelope"
date: 2026-07-27
category: logic-errors
module: "adapters/nautilus lab ORB strategy (lab/src/strategy/orb.rs: OR-width sanity gate), lab mount-universe producer (lab/src/runner/mount_universe.rs)"
problem_type: logic_error
component: strategy
severity: high
applies_when:
  - "Authoring or reviewing a live-mount universe file (LS_MOUNT_UNIVERSE_FILE)"
  - "Auditing a live session against its backtest twin and finding more entries than expected"
  - "Reading decisions.jsonl to confirm which entry gates fired"
  - "Adding a new optional field to the mount-universe row schema"
  - "Arming any strategy lever whose input is optional in the universe file"
related_components:
  - lab-mount-universe
  - orb-strategy
  - production-ladder
tags:
  - prior-atr
  - or-width-gate
  - mount-universe
  - head-fidelity
  - skip-not-reject
  - silent-divergence
  - decisions-jsonl
---

## Problem

The v34 head runs `or_width_max_atr = 0.666` — the OR-width sanity gate is **armed**. That gate
is deliberately *skip-not-reject* when `prior_atr` is absent. So a universe row without
`prior_atr` does not fail, does not warn, and does not reject: it silently switches the width
gate **off for that symbol** and emits **no reject envelope at all**.

`prior_atr` is optional in the file schema. It is not optional for head fidelity.

## Symptoms

- A live session takes entries the backtest twin rejected, with no corresponding reject record.
- `decisions.jsonl` contains no `or_width_atr` envelope for the affected symbols — not a reject
  with a different reason, but **nothing**. Any audit that reads only decision records sees a
  clean run.
- The divergence scales with how many universe rows lack ATR, so it can be small enough to look
  like noise and large enough to change the session's P&L.

The failure mode is invisible by construction: the absence of a record is the symptom.

## What Didn't Work

Reasoning about it from the file schema alone. `prior_atr: Option<f64>` reads as "nice to have,"
which is true of `prior_open_vol_mean` and `prior_illiq` (genuinely inert under v34:
`rvol_min = 0.0`, `liquidity_tilt_alpha = 0.0`) but false of `prior_atr` the moment
`or_width_max_atr > 0.0`. Optionality in the schema and optionality in the head are different
properties, and only one of them is written down in the type.

Auditing `decisions.jsonl` also does not work, for the reason above — there is nothing to find.

## Solution

The producer treats a symbol whose ATR could not be computed as **not a candidate for today**,
and drops it loudly, rather than emitting it un-gated
(`adapters/nautilus/lab/src/runner/mount_universe.rs`).

Verified 2026-07-27: a dry run for session date 2026-07-22 emitted **27 rows, all 27 carrying
`prior_atr`**, drawn from a 40-symbol metadata pin — the other 13 were dropped rather than
emitted without ATR.

Never hand-author the universe file. Produce it with `lab-mount-universe`, which reuses the
backtest's own ATR/turnover/selection helpers so it cannot drift from the head. A second
implementation of the ATR window would be a silent head-divergence no gate could catch.

## Why This Works

The gate's skip-not-reject behavior is correct on its own terms, and the source says so
(`adapters/nautilus/lab/src/strategy/orb.rs:808-830`):

```rust
// the width gate is genuinely OPTIONAL for a session that lacks a positive
// prior ATR: with nothing to normalize against, the session is simply not
// width-gated (SKIP, not reject). This is deliberate: coupling the width test
// to ATR availability conflated "too-wide opening range" with "no ATR history"
// and swamped the clean width signal with a winner-rich coverage cull
// (lever 3 / v18 was reverted for exactly this).
if params.or_width_max_atr > 0.0 {
    if let Some(atr) = self.prior_atr.filter(|a| *a > 0.0) {
        // ... reject only when range_r > or_width_max_atr * atr
    }
}
```

The KTD7 invariant — *a missing input never silently passes a REQUIRED gate* — is preserved,
because when ATR is absent the gate is **not required**. Contrast the ATR-STOP arm immediately
above it, where a stop genuinely needs its ATR and a missing one fails closed with an
`atr_unavailable` reject (`orb.rs:806`).

So the defect is not in the gate. It is in the **input contract**: the gate is sound given a
universe whose rows all carry ATR, and unsound given one that does not. Enforcing the invariant
at the producer — the single place the file is created — is what closes it.

## Prevention

**Treat "optional in the schema" and "optional for the head" as separate questions.** When adding
an optional field to the mount-universe row, ask which armed levers consume it and whether their
absent-input behavior is fail-closed or skip. A skip-behaving lever plus an optional input is a
silent-divergence pair, and it must be closed at the producer.

**Prefer a loud drop to a quiet emit.** Dropping a symbol costs one tradable name and leaves a
message. Emitting it un-gated costs head fidelity and leaves nothing.

**Watch the not-yet-armed levers.** The backtest range-clips minute bars, so its RVOL baseline is
range-dependent while a live producer has no range. That is inert under v34 (`rvol_min = 0.0`)
but becomes a live/backtest divergence the moment the RVOL lever is armed — the same
shape as this bug, one lever over.

**The guards that already exist, and are worth keeping intact:**

- `--mount` refuses a universe file whose rows carry a different `session_date`, so a leftover
  file cannot trade yesterday's symbols at yesterday's opening prices.
- The producer refuses a metadata-driven head when `LS_MOUNT_UNIVERSE_METADATA` is unset, naming
  the artifact hash the head was built from (verified 2026-07-27: it named
  `90005f882d5eac1fa00b7b8a810241a5d0ea38450fe1c10cb56f21767307ee00`). Omitting it would silently
  drop the tradability gate — the same class of failure as this one.

Before this was written up it lived only as a module comment in `mount_universe.rs`. A failure
mode that produces no artifact needs to be findable by search, because nobody will find it by
reading logs.

## Related

- [`mount-universe-producer-cannot-be-fed-on-a-session-morning`](../architecture-patterns/mount-universe-producer-cannot-be-fed-on-a-session-morning.md)
  — the producer's sourcing split: prior values from the catalog, today's open from a live t8407 quote
- `adapters/nautilus/lab/RUNBOOK-rung1.md` §4 — "Never hand-author it"
- `adapters/nautilus/lab/RUNG1-PREFLIGHT.md` §0.7 — universe resolution in the agent preflight
