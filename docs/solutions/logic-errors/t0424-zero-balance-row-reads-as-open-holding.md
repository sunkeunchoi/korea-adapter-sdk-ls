---
title: "t0424 keeps a net-zero janqty=0 row after a same-day round-trip → flat-start gate false-fails 'not flat'"
date: 2026-07-07
category: logic-errors
module: nautilus adapter execution (LsExecClient::verify_flat) + ls-sdk account/holdings (t0424)
problem_type: logic_error
tags: [t0424, verify-flat, flat-start-gate, holdings, janqty, fail-closed, nautilus, order-path, R14]
---

## Problem

`LsExecClient::verify_flat` (the R14 flat-start gate) refused to start — and the
marketable SC probe warned "not flat after close" — on an account that was economically
**flat**. A same-day buy+sell round-trip leaves a lingering `janqty=0` row for the symbol
in the `t0424` (주식잔고) holdings block, and the gate counted *any* row as an open
position.

## Symptoms

- `flat-start gate: 1 holding position(s) present — refusing to start (R14)` when the
  account holds no open position.
- `WARNING: not flat after close` at the end of the marketable SC probe, even though the
  buy and sell netted to zero.
- `live-smoke-t0424` reports `holdings=1`, but a per-row dump shows
  `expcode=005930 janqty=0 mdposqt=0` — a zero-balance row, not a position.
- A `chegb="2"` t0425 working-order scan of the symbol is empty (`body_len=63`): there is
  no resting order either. The account is flat by every economic measure.

## What Didn't Work

- Treating it as a real stranded position and trying to sell it out: a flatten helper
  correctly reported `already flat (no balance) — nothing to sell` (the sellable qty was
  0), yet the gate still failed — proving the "holding" was not sellable and not real.
- Re-checking `holdings=1` via the t0424 count smoke: the count is exactly the trap — it
  counts rows, not open balance, so it stays `1` while the position is zero.

## Solution

Gate flatness on the **balance field** (`janqty`), not row presence — and fail **closed**
on an unparseable balance, mirroring the sibling open-order check (which already gates on
`ordrem > 0`, not row count).

Before (`adapters/nautilus/src/execution.rs`, `verify_flat`):

```rust
let holdings = self.sdk.account()
    .stock_balance(&T0424Request::new("1", "0", "0", "0")).await?;
if !holdings.outblock1.is_empty() {
    return Err(AdapterError::Config(format!(
        "flat-start gate: {} holding position(s) present — refusing to start (R14)",
        holdings.outblock1.len()
    )));
}
```

After:

```rust
let holdings = self.sdk.account()
    .stock_balance(&T0424Request::new("1", "0", "0", "0")).await?;
// A row is OPEN only if its janqty parses > 0 OR is unparseable (fail-closed — never
// read a garbage balance as "0 = flat"). A same-day round-trip leaves a janqty=0 row
// that is NOT an open holding.
let open_holdings = holdings.outblock1.iter()
    .filter(|r| r.janqty.trim().parse::<i64>().map_or(true, |n| n > 0))
    .count();
if open_holdings > 0 {
    return Err(AdapterError::Config(format!(
        "flat-start gate: {open_holdings} holding position(s) present — refusing to \
         start (R14)"
    )));
}
```

## Why This Works

A `t0424` row is a *statement-line for the symbol this session*, not proof of an open
position. After a same-day buy then sell, the symbol's net balance is 0 but the gateway
still lists the row (with `janqty=0`, `mdposqt=0`) for the rest of the day. Row presence
therefore over-reports holdings by exactly the set of symbols touched-and-closed today.
Gating on `janqty > 0` reads the actual position; the `map_or(true, …)` keeps the check
fail-closed so a malformed/garbage balance still refuses (never silently treated as flat).

## Prevention

- **Never gate flatness on a t0424 row *count* / `!is_empty()`.** Always read the balance
  field (`janqty`, and for sellability `mdposqt`). The same trap applies to any
  holdings-derived flat/idle check.
- **Fail closed on unparseable numeric holdings** — mirror the order-side pattern
  (`ordrem.trim().parse().map_or(true, |n| n > 0)`); an unreadable balance is "open",
  never "flat".
- **Test both directions offline** with a wiremock t0424: a `janqty>0` row must refuse the
  gate, and a `janqty=0` (net-zero round-trip) row must pass it. See
  `adapters/nautilus/tests/execution_client.rs`:
  `zero_balance_lingering_holding_row_is_flat` and `unparseable_holding_balance_fails_closed`.
- When a live order path warns "not flat after close," dump the actual holding rows
  (symbol + `janqty` + `mdposqt`) before concluding a position is stranded — a `janqty=0`
  row is the benign case, and re-running a buy-then-sell probe to "fix" it only compounds
  the confusion.

## Related

- `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md`
  — the sibling t0425 flat-scan pagination gotcha (both are "row-shape vs. real-state"
  false-flat/false-HELD traps on the same order surface).
- `docs/solutions/logic-errors/fail-closed-reconcile-set-drops-symbol-on-truncated-page.md`
  — the same fail-closed discipline on a truncated read.
