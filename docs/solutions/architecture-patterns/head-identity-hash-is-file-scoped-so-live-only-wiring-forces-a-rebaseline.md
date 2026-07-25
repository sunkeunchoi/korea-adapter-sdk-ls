---
title: "The ladder head identity hashes a FILE, not behavior — live-only wiring added to orb.rs fail-closes the mount until a re-baseline lands"
date: 2026-07-25
category: architecture-patterns
module: "adapters/nautilus/lab — artifacts/manifest.rs strategy_code_hash + dispatch/ladder.rs head_governed_params_pinned + runner/live.rs resolve_mount_head_params; operator data home data/turn4-fresh"
problem_type: architecture_pattern
component: development_workflow
severity: high
applies_when:
  - "Shipping a live/runtime feature that must be touched per-bar inside the strategy, so the edit necessarily lands in adapters/nautilus/lab/src/strategy/orb.rs"
  - "The edit is semantically inert for backtests (new `Option` fields left `None` on the backtest path), so no re-baseline feels warranted"
  - "The Production Ladder is armed and an operator is about to run `lab-live --mount` against the pinned head"
  - "Diagnosing `lab-live --mount` exiting 71 with `mount refused: the resolved head governed params size to ZERO`"
tags:
  - head-identity
  - strategy-code-hash
  - production-ladder
  - re-baseline
  - fail-closed
  - live-session-driver
related_components:
  - "lab-live --mount / --rung-report (dispatch CLI)"
  - "dispatch/prereg.rs frozen pre-registration"
  - "lab-research turn re-baseline path"
---

# The ladder head identity hashes a FILE, not behavior

## Context

`strategy_code_hash()` is the ORB Production Ladder's **head identity**: the token that says
"the binary in your hand is the certified head." Its definition is narrower than most readers
assume — it hashes one source file and nothing else:

```rust
// adapters/nautilus/lab/src/strategy/mod.rs:10
pub const ORB_SOURCE: &str = include_str!("orb.rs");

// adapters/nautilus/lab/src/artifacts/manifest.rs:138-139
pub fn strategy_code_hash() -> String {
    hash_bytes(crate::strategy::ORB_SOURCE.as_bytes())
```

That scope is deliberate and load-bearing for the research loop — a runner rewrite must *not*
move the head. But it means the fingerprint tracks **bytes in `orb.rs`**, not strategy
behavior. Any change to that file moves the head identity, including one that provably cannot
alter a backtest.

PR #213 (the live-session driver, merged as `ab16101`) is the first change to hit this seam
from the *live* side. To feed the runtime dead-man and the max-loss breaker's mark, the driver
had to touch state on every processed bar — which only exists inside `on_bar`, inside `orb.rs`.
The result was 99 added lines, every one of them inert for a backtest: two `Option` fields
defaulted to `None`, two builder methods the live path alone calls, and two `if let Some(..)`
blocks in `on_bar`.

```rust
// adapters/nautilus/lab/src/strategy/orb.rs — added by #213
heartbeats: Option<crate::runner::watchdog::Heartbeats>,
mark_feed: Option<MarkFeed>,
```

The hash moved anyway:

| | |
|---|---|
| before #213 | `d7a9820b7356547ac8de0d0b8b11748dea6e83be7168744ef6591a88ce31145e` |
| after #213 (current tree) | `e5bc2ae89ff217cc34c698447cedd03fc1a2cbb7ba23256a9ee39f5a94652399` |

## Guidance

**Treat "this edit lands in `orb.rs`" as equivalent to "the ladder head changes," regardless
of whether behavior changed — and ship the re-baseline inside the same unit of work.**

The head-identity move is not a diagnostic that surfaces later; it is a *fail-closed refusal
on the operator's next action*. Three consumers read the hash, and the failure is silent until
the last one:

1. **`head_governed_params_pinned`** (`adapters/nautilus/lab/src/dispatch/ladder.rs:85-95`)
   filters finalized runs by `strategy_code_hash == <running binary's>` and the
   `LS_TURN_EXPECT_VERSION` pin, then `.unwrap_or_default()`. No match returns
   `OrbParams::default()` — quietly, with no error.
2. **`resolve_mount_head_params`** (`adapters/nautilus/lab/src/runner/live.rs:1945-1962`)
   catches that default via its zero-size guard, because `default()` carries
   `risk_per_trade_krw == 0` and would size every order to zero shares. It refuses with
   `MOUNT_PRECHECK_FAILED` — **exit 71** (`live.rs:1865`).
3. **`rung_report`** (`adapters/nautilus/lab/src/dispatch/ladder.rs:555-567`) classifies any
   **live-lane** session whose manifest hash differs as `head_mismatched` — "NOT counted — ran
   under a different head" (`live.rs:2512`). Backtests are excluded by `is_live_lane`, so this
   consumer is silent until live sessions actually exist.

So the operator's first `--mount` attempt after such a merge exits 71 on a machine where
everything else is green. That is the design working: layer 1 fails open into a zero-size head,
and layer 2 is the guard that makes it loud.

**Re-baseline at the SAME version — do not bump.** The ladder pins the head by version as well
as code (`LS_TURN_EXPECT_VERSION=34`, `ladder.rs:97-103`), and the frozen pre-registration's
economic bands are derived from the named v34 backtest. A version bump would invalidate the pin
and detach the bands from their cited source. What is needed is a v34 run carrying the *new*
hash — the same-version rerun path described in
[`../workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md`](../workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md)
("Shortcut for a follow-up code turn on the same version"), not the `LS_TURN_CODE_BUMP=1`
version-bump path that document otherwise recommends.

**The acceptance bar for an inert re-baseline is equality, not plausibility.** When the change
is claimed to be semantically inert, the re-baseline run must reproduce the prior head's trades,
performance, and fingerprint exactly; only `strategy_code_hash`, run id, and timestamps may
differ. Anything else means the "inert" wiring was not inert, and that is a stop-and-surface
condition, not a number to accept.

**Budget the prose cost too.** The head hash is quoted as a *human operator check* in several
places — `adapters/nautilus/lab/RUNBOOK-rung1.md`,
`adapters/nautilus/lab/RUNG1-PREFLIGHT.md`, `adapters/nautilus/lab/README.md`, the `--head` and
`--rung-report` printed strings in `adapters/nautilus/lab/src/runner/live.rs`, and two
assertions in `adapters/nautilus/lab/tests/dispatch_cli.rs`. Those must move with the hash.
Historical mentions (`adapters/nautilus/lab/TURN-LOG.md`,
`adapters/nautilus/lab/config/PREREGISTRATION.md`, `docs/plans/*`, finalized run manifests)
must not — they record what was true at the time.

**One file is off-limits: `adapters/nautilus/lab/config/preregistration.json`.** Its `_note`
mentions the old hash, but `load()` hashes the raw file bytes into `LoadedPreReg.content_hash`
(`adapters/nautilus/lab/src/dispatch/prereg.rs:149`) and every dispatch record cites it.
Editing so much as that prose string is an unrecorded re-registration.

## Why This Matters

The failure mode this prevents is a merge that looks completely clean — CI green, gate green,
no behavior change, an honest claim of inertness — and that nonetheless leaves the ladder
unrunnable for whoever picks it up next. The operator discovers it at the worst moment: holding
a green single-use dispatch, at a live-session start time, with an exit code and no context.

Exit 71 is specifically the *recoverable, pre-consume* refusal class (`live.rs:1861-1865`) —
the dispatch survives, so this costs an attempt rather than a whole `--dispatch` cycle. That
containment is what makes the coupling survivable, not what makes it acceptable.

There is also a ladder-bookkeeping consequence that is easy to miss: the frozen
pre-registration sets `code_change_resets_to_rung_1: true`
(`adapters/nautilus/lab/src/dispatch/prereg.rs:84`,
`adapters/nautilus/lab/config/preregistration.json:15`), so a hash move is a head-change event
by the ladder's own rules. It is cheap only while the chain sits at rung 1 and the *governed
params* are untouched
(`governed_params_hash` is computed from `OrbParams`, not from source bytes, so it is unchanged
and the rung-1 band still applies). The same merge landing at rung 3 would have cost the rungs
climbed.

The deeper pattern: **an identity fingerprint scoped to a file will be moved by every consumer
that needs to reach into that file, including consumers the fingerprint was never about.**
`strategy_code_hash` was designed to answer a research-loop question ("did the strategy logic
change between turns?"). The live ladder then reused it to answer a different question ("is this
binary the certified head?"). Those questions agree until a live-only feature needs a per-bar
hook — at which point the coarser answer wins, and there is no escape hatch by design. Reaching
for one would be worse: a "this edit doesn't count" flag is exactly the mechanism through which
an uncertified strategy reaches a live account.

## When to Apply

- Before merging any change that touches `adapters/nautilus/lab/src/strategy/orb.rs` while the
  ladder is armed — plan the re-baseline and the doc-hash refresh as part of the same unit of
  work, not as follow-ups.
- When `lab-live --mount` exits 71 with the zero-size head message and `LS_DATA_HOME` is
  demonstrably correct — suspect a moved head hash before suspecting the data home.
- When `--rung-report` starts calling previously-counted live sessions `head-mismatched`.
- Not applicable to runner-only, param-only, or config-only edits: they do not move
  `ORB_SOURCE`, which is why the live driver's own `live.rs` changes were free.

## Examples

Confirming the coupling, offline, in two commands — the binary's hash versus the head run's
recorded hash:

```bash
# What the running binary will compute (strategy_code_hash hashes exactly this file).
shasum -a 256 adapters/nautilus/lab/src/strategy/orb.rs
# e5bc2ae89ff217cc34c698447cedd03fc1a2cbb7ba23256a9ee39f5a94652399

# What the pinned v34 head run recorded.
grep -o '"strategy_code_hash": *"[^"]*"' \
  data/turn4-fresh/runs/20260724T014752Z-backtest-orb-v34/manifest.json
# "strategy_code_hash": "d7a9820b7356547ac8de0d0b8b11748dea6e83be7168744ef6591a88ce31145e"
```

Unequal ⇒ `head_governed_params_pinned` matches nothing ⇒ `OrbParams::default()` ⇒
`risk_per_trade_krw == 0` ⇒ `--mount` exits 71. Equal ⇒ the head resolves and the mount can
size.

The inert-change shape that still moves the hash (from #213 — every added line is
backtest-dead):

```rust
// Fields default to None in `new()`; only the live runner calls the builders.
if let Some(hb) = &self.heartbeats {
    hb.touch_runtime(chrono::Utc::now().timestamp());
}
if let Some(feed) = &self.mark_feed {
    // ... publish this bar's mark
}
```

A backtest never enters either branch. The hash moves regardless, because the hash is over the
file's bytes.

## Related

- [`../workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md`](../workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md)
  — the re-baseline mechanics themselves, and the same-version rerun shortcut this situation needs.
- [`live-session-teardown-must-share-the-nodes-arcs-and-capture-handles-before-build.md`](live-session-teardown-must-share-the-nodes-arcs-and-capture-handles-before-build.md)
  — the other design traps from the same live-session-driver change (PR #213).
- [`../design-patterns/build-runtime-hash-parity-via-shared-include.md`](../design-patterns/build-runtime-hash-parity-via-shared-include.md)
  — how the build and runtime agree on the hash in the first place.
- `adapters/nautilus/lab/RUNG1-PREFLIGHT.md` § "Head identity (KTD7)" — the operator-facing check.
