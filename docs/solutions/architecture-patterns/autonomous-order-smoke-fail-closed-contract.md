---
title: "Autonomous order-smoke: fail-closed invocation contract (CI/no-TTY + per-wave nonce, account-wide flat assertion, output scrubbing)"
date: 2026-06-26
last_updated: 2026-06-26
category: architecture-patterns
module: crates/ls-sdk, crates/ls-core
problem_type: architecture_pattern
component: orders
severity: high
applies_when:
  - "Making an irreversible-action live smoke (order placement) agent-runnable without an operator handoff"
  - "Deciding whether a live order smoke may run unattended, and how to bound it to human-present waves"
  - "Asserting an account is flat after an autonomous order run (no operator to clean up)"
  - "Scrubbing autonomous-run output of account numbers / bearer tokens / broker rsp_msg"
  - "Invoking make live-smoke-order-chain from an agent (Bash tool) and seeing it refuse"
---

# Autonomous order-smoke: fail-closed invocation contract

## Context

The chained paper-order live smoke (`order_chained_smoke` in
`crates/ls-sdk/tests/order_smoke.rs`, run via `make live-smoke-order-chain`) was
historically **operator-gated**: a human ran it in-window and the agent waited.
Making it agent-runnable removes the operator-handoff *protocol* — but placing an
order is irreversible, and there is no longer an operator to clean up a wedged run.
Autonomy is therefore a deliberate risk-acceptance that must be paid for with a
committed, fail-closed safety-invariant set, not just "drop the wait."

This doc is the invocation + safety contract. It also records the **agent-invocation
gotcha** that surprises every first run: the smoke refuses under the Bash tool.

## Guidance

### 1. Invocation requires an attended PTY + a fresh per-wave nonce

The autonomy precondition (`check_autonomy` / `validate_nonce`) is fail-closed on
EITHER of two gates:

- **CI/no-TTY marker** — refuses if `CI`/`GITHUB_ACTIONS` is set OR stdin is not a
  TTY (`std::io::IsTerminal`).
- **Per-wave human nonce** — `LS_ORDER_SMOKE_NONCE` must be a fresh unix-seconds
  timestamp within a short TTL (600 s). A static/reused/non-numeric value is rejected,
  so "valid nonce" can never degrade to "env var present" that a cron/CI/cached loop
  satisfies.

**The agent-invocation gotcha (load-bearing):** the Claude Code Bash tool runs
subprocesses **without a TTY**, so `detect_unattended_marker()` returns
`Some("no TTY on stdin")` and the smoke refuses *even with a valid nonce*. A plain
`make live-smoke-order-chain` from an agent exits with `1 passed` (a Pending/refusal
outcome) and the Makefile's `grep -q "1 passed"` check passes — so the agent thinks it
ran when it did **not place anything**. Invoke through a PTY allocator:

```bash
script -q /dev/null env LS_ORDER_SMOKE_NONCE=$(date +%s) make live-smoke-order-chain
```

Mint the nonce immediately before the call. Do **not** put `LS_ORDER_SMOKE_NONCE` in
`.env` — the Makefile sources `.env`, so a stale value there clobbers the fresh shell
value (fails closed, but confusingly). The autonomy delivered is removal of the
handoff *protocol*, not the human: a human still mints the nonce and the run still
needs an attended context.

### 2. The order kill-switch is "no new orders", not a teardown

`set_orders_enabled(false)` HALTS all order dispatch before any HTTP call — so a cancel
issued *after* it is blocked. It can never remove a resting order. Retry-cancel is the
only resting-order remover; the kill-switch is engaged **after** retry-cancel, right
before a hard-fail, so a wedged run places nothing further. Never order it before the
cleanup cancels it needs to issue.

### 3. Post-run flat assertion: account-wide, quantity-keyed, positive-confirmation-only

After teardown, assert the account is flat via an **account-wide** `t0425` scan (empty
`expcode` = all symbols), not the per-intent `reconcile_rows` (which only matches the
smoke's own order and would miss a leftover from a prior aborted run).

- **Key on quantities, NOT status text.** `cheqty > 0` → an unrecoverable fill
  (hard-fail immediately; paper reset is the only remedy). `ordrem > 0` (no fill) → a
  cancelable resting remainder (retry-cancel, re-scan, hard-fail if still resting).
  `Fill` outranks `Resting` (a partial fill `cheqty>0 && ordrem>0` is a Fill). A
  genuinely canceled order releases its remainder (`ordrem == 0`) so it needs no
  status-text filter — and crucially a **cancel-rejected (`취소거부`) / modify-rejected
  (`정정거부`) order is STILL RESTING** despite the 취소/거부 marker, so a status-text
  "terminal" filter would wrongly conclude flat while an order rests.
- **Positive confirmation only.** A failed / timed-out / truncated read is treated as
  NOT flat → engage the kill-switch and hard-fail. Never conclude flat from absence of
  evidence.

### 4. An ambiguous SUBMIT must run the flat assertion

`post_order` returns `LsError::AmbiguousOrder` precisely when an order **may** have
reached the gateway (5xx / empty non-2xx ack). With no operator, a submit error must
not early-return Pending and skip the flat check — that leaves a possibly-resting order
unverified. Run the account-wide flat assertion before recording Pending on any
post-submit error **except** the proven-not-placed rejections (`01900` service-not-in-
paper, `01491` account-not-order-capable), which cleanly placed nothing.

### 5. Scrub every output path; fail closed on the debug log

All autonomous output must be account-/secret-free. The widened scrubber
(`scrub_secrets`) masks account numbers + their `-NN` product suffix (one token) and
20+-char bearer/appkey tokens, while letting short order numbers survive so failures
stay actionable. Route order numbers through a dedicated structured field and free text
(including `LsError` Display, which carries the broker `rsp_msg`) through the scrubber —
**every** `println!`/`panic!`/record path, with no exceptions (a single un-scrubbed
diagnostic `println!("...: {e}")` is the classic leak). Suppressing the `ls_core`
dispatch debug events (which log whole bodies the digit scrubber never sees) requires a
process-global tracing subscriber; install it fail-closed (refuse the run if a foreign
global subscriber already exists, since `tracing` allows one global default — a silent
install-failure would fail *open* on a known leak).

## Why This Matters

Autonomy trades a human pre-placement checkpoint for post-run detection. That trade is
only acceptable because every failure mode is loud and fail-closed: a resting order
hard-fails naming it, a fill hard-fails with paper-reset, an unreadable account hard-
fails, and refusal (CI/no-TTY/stale-nonce) places nothing. Get any gate wrong — order
the kill-switch before cleanup, key the flat check on status text, skip the flat check
on an ambiguous submit, or leak one un-scrubbed `rsp_msg` — and the wave either leaves a
real paper order resting unattended or leaks an account number into evidence.

## When to Apply

- Making any irreversible-action live smoke agent-runnable (orders, transfers, anything
  that mutates broker-side state).
- Reviewing or re-running `make live-smoke-order-chain` — start from the PTY+nonce
  invocation above.

## Examples

Refusal under the Bash tool (the surprise), vs. a correct PTY-wrapped run:

```bash
# Agent Bash tool — NO TTY → refuses, places nothing, "1 passed" (Pending). Misleading.
LS_ORDER_SMOKE_NONCE=$(date +%s) make live-smoke-order-chain

# Correct: allocate a PTY so detect_unattended_marker() sees a terminal.
script -q /dev/null env LS_ORDER_SMOKE_NONCE=$(date +%s) make live-smoke-order-chain
```

Flat verdict keys on quantities, never status text:

```rust
// A 취소거부 (cancel-REJECTED) order is STILL RESTING — ordrem>0 surfaces it.
// A status-text "is it 취소/거부?" filter would skip it and wrongly read Flat.
if cheqty > 0      { fills.push(ordno) }        // unrecoverable fill -> hard-fail
else if ordrem > 0 { resting.push(ordno) }      // cancelable        -> retry-cancel
// cheqty==0 && ordrem==0 -> contributes nothing (a genuinely-canceled order)
```
