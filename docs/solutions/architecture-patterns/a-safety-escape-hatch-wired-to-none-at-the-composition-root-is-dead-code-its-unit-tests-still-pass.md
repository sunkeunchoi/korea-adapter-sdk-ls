---
title: "A safety escape hatch hardcoded to `None` at the composition root is dead code — and its pure-layer tests all still pass, so the gate reports full coverage of a feature the shipped binary cannot reach"
date: 2026-07-26
category: architecture-patterns
module: "lab dispatch gate (adapters/nautilus/lab/src/runner/live.rs `dispatch_gate_config_from_env` / `run_dispatch`), the attended Unknown calendar override (adapters/nautilus/lab/src/dispatch/mod.rs `UnknownOverride`, checks.rs `check_calendar_date`)"
problem_type: architecture_pattern
component: tooling
severity: critical
applies_when:
  - "Reviewing or adding an escape hatch, override, or break-glass path that bypasses a safety refusal"
  - "A capability is reachable only through an `Option` field the composition root supplies"
  - "Tests for a feature all construct its config struct directly rather than driving the real entry point"
  - "A code comment describes an intended entry point ('enters via an operator tool path') that may never have been built"
  - "Adding a fallible call to a composition root that already owes a mandatory side effect"
related_components:
  - lab-dispatch-gate
  - nautilus-ls-calendar
  - production-ladder
tags:
  - composition-root
  - dead-code
  - escape-hatch
  - test-reachability
  - fail-closed
  - coverage-illusion
  - unknown-override
---

## The problem

The rung-1 production ladder shipped with an attended **Unknown-date override** that was, by
every local measure, finished: a documented `UnknownOverride` struct with a structured
first-party citation, `is_well_formed()` + `covers(date, run_id)` binding, a fresh-nonce and
attendance gate, chain-record scrubbing, and **eight passing tests** across
`dispatch_checks.rs` and `dispatch_cli.rs` — including one asserting it greens an Unknown date
with a full audit record.

It was unreachable. The composition root read:

```rust
// Never sourced from the environment: the narrow attended Unknown override enters
// via an operator tool path, not a blunt env toggle.
unknown_override: None,
```

No operator tool path was ever built. Every production construction site was `None`, so
`effective_override` matched `None => None` before the nonce gate was reached, and
`check_calendar_date`'s override arm could never be taken outside tests.

This was not cosmetic. The current day always reads `Unknown` — see
[KRX trading-session status is provable only retrospectively](krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md)
— and `Unknown` is a non-deferrable red. The override was the *only* thing that could proceed
it, so with the override unreachable **no live session could ever be dispatched on any date**.
The ladder was structurally unrunnable, and nothing in the gate said so.

## Why the tests did not catch it

Every test constructed `DispatchCliConfig` directly and set `cfg.unknown_override = Some(ov)`.
That is the correct way to test the *check*, and the check was correct. But the tests entered
the system **below** the composition root, at exactly the seam where the defect lived. The
tests and the defect were disjoint by construction:

- pure-check tests proved "given an override, Unknown greens" — true, and irrelevant;
- CLI tests proved "given a config with an override, Unknown greens" — also true, also
  irrelevant, because nothing in production ever builds that config.

Coverage tooling would have shown the override arm as covered. It was — by tests that supply
the one input production cannot.

## The fix

Give the escape hatch a real entry point, and test *that*:

```rust
unknown_override: unknown_override_from_env()?,
```

reading an operator-authored JSON file named by `LS_DISPATCH_UNKNOWN_OVERRIDE` — a file, not a
bare env var, because the override must carry a structured citation that cannot be written by
reflex. Fail-closed at every step: a named file that is unreadable, unparseable, or
audit-incomplete is a hard error, never a silent `None` (a typo'd path must not be able to
masquerade as "the operator chose not to override").

Two things worth copying:

**Bind the override to the snapshot actually in force.** The audit fields record which snapshot
the operator reviewed; if that is not the snapshot this run loaded, the operator reviewed
different alerts and the override cannot speak for this run. Check it *before* the nonce gate —
a fresh nonce must never launder a stale-snapshot override.

**Watch the offline stub seam when adding a composition-root check.** The `date_fact_stub` path
builds a `StartupRecord` with `diagnostic: None`, so a naive snapshot-identity check rejects the
override in every stubbed test and reds `u12_enforced_unknown_override_greens_and_records_full_audit`
— a change that would have looked like "my new check found a bug." The binding is gated on
`cfg.date_fact_stub.is_none()`: real runs always resolve a snapshot, so the check always applies
to them.

## The fix re-committed a documented mistake — at the same composition root

Worth recording, because it is the sharpest evidence for the rule below. The repo already
documents
[a mandatory always-emit at a composition root must fire before any early exit](../conventions/composition-root-always-emit-before-fallible-parse.md).
The fix above **violated it**, in the same function, on the same day the convention was
available to read.

`unknown_override_from_env()` is fallible — it reads and parses an operator file. It was added
to `dispatch_gate_config_from_env`, which runs *before* `run_dispatch`'s mandatory
`emit_startup_record`. So a typo'd `LS_DISPATCH_UNKNOWN_OVERRIDE` exited non-zero with **zero**
`calendar-startup` lines: the operator loses the calendar diagnostic at exactly the moment they
are debugging a calendar-adjacent refusal. Code review caught it; the repair was to emit on the
gather's error path (`live.rs:1080`, `inspect_err`), leaving the success path emitting exactly
once from inside `run_dispatch`.

Why the existing doc did not prevent it: the convention is filed as a rule about *emit
ordering*, and the author was at that root thinking about a different defect entirely
(reachability). Nobody re-reads the emit-ordering convention while fixing a dead-code bug. The
trigger is not the topic, it is the edit:

> **Adding a fallible call to a composition root re-opens every mandatory side effect
> downstream of it.** When you introduce a `?` into a config gather, enumerate what the old
> exit paths guaranteed and check each one still fires.

That is now an `applies_when` entry on this doc, so the two docs surface together for the edit
shape rather than only for their separate topics.

## The generalizable rule

**A feature's tests must enter the system where production enters it.** When a capability is
reachable only through a value the composition root hardcodes, its tests prove the mechanism and
say nothing about the reachability — and a green gate reads as "this works" when the truth is
"this would work, if anything called it."

Two cheap detectors:

- Grep every production construction site of an `Option` that gates a capability. If they are
  **all** `None`/`Default`, the capability is dead however well-tested it is. A comment
  describing the intended entry point ("enters via an operator tool path") is a *claim*, not an
  entry point — treat one as a TODO until a caller exists.
- For any escape hatch, write one test that drives it through the **real** entry point (env,
  file, CLI arg). That test is the only one that fails when the hatch is unwired.

This is the sibling of
[a mandatory always-emit at a composition root must fire before any early exit](../conventions/composition-root-always-emit-before-fallible-parse.md):
both are defects that live *only* at the composition root and are invisible to every test that
starts below it. As the section above records, fixing this one re-committed that one — the
kinship is not thematic, it is the same blind spot firing twice.
