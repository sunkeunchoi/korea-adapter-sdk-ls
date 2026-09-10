---
title: Adjusted prices sit on no tick grid, so a backtest venue silently declines the fill — an effective-date ladder does not fix it, and the fixture that satisfies the validator cannot test it
date: 2026-09-08
category: docs/solutions/logic-errors
module: adapters/nautilus/lab/src/runner/backtest_daily.rs, adapters/nautilus/src/instruments.rs
problem_type: logic-error
component: backtest
severity: high
applies_when:
  - running a nautilus backtest over a deep-history catalog for the first time
  - reusing a live-trading instrument object as a historical-simulation object
  - a backtest finalizes green but trades far fewer positions than its parameters ask for
  - writing a fixture whose values are chosen to satisfy a validator the fixture also tests
tags:
  - nautilus
  - backtest
  - price-increment
  - tick-size
  - adjusted-prices
  - silent-failure
  - daily-lineage
---

# Adjusted prices sit on no tick grid, so a backtest venue silently declines the fill

## What happened

The first run of the daily-resolution backtest over the real 352-symbol catalog
(2016-08-01 .. 2019-12-31) exited 0, wrote a data-quality report clean on every axis it
covers — zero coverage gaps, zero shallow-history symbols, zero adjustment-basis shifts,
zero approximated fills — and printed a healthy summary. It had **silently traded
roughly a fifth of the book it was configured for**: `target_m = 8` entries per session
delivered **1.57**, because 5,167 of the entry orders it submitted never opened a
position.

The only trace was 7,981 WARN lines:

```
Skipping fill for O-20160803-063000-001-ms-4: fill price 30340 is not compatible
  with 005930.XKRX price_precision=0 price_increment=500
```

nautilus's matching engine runs every fill price through
`normalize_price_for_current_instrument`. When the price is not a multiple of the
instrument's `price_increment` it logs at WARN and **leaves the order unfilled**. It is
not an error and nothing downstream reports it.

## The two causes, and why only naming the first would have shipped a half-fix

**Cause 1 — a live instrument reused as a historical one.**
`nautilus_ls::instruments::map_equity` derives `price_increment` **once**, from the
master row's *current* reference price (`recprice`, else `jnilclose`) under a hardcoded
`TickRegime::Post2023`. Its own comment says so: *"The instrument's static increment uses
today's regime."* For live order placement that is correct — today's order is priced in
today's band under today's ladder. For a 2016 bar it is simply the wrong band:
`000660` closed at 33,550 on a 50 KRW grid and carries today's 1,000 KRW tick.
`rules.rs::TickRegime::for_date` exists and is correct; `instruments.rs` is its only
production caller and does not use it.

That reads like a complete diagnosis. It is not, and stopping there produces a fix that
closes less than half the defect while looking finished.

**Cause 2 — the catalog is adjustment-adjusted, so its prices are on no grid at all.**
With `checkpoint.adjusted_prices == true`, every price has been divided through by the
symbol's later corporate actions. Checked against the ladder that actually governed each
date:

| symbol | 2016 price | tick then | on grid? |
|---|---|---|---|
| 000660 | 33,550 | 50 | yes |
| 005930 | 30,340 | 50 | **no** |
| 006400 | 112,589 | 500 | **no** |
| 011200 | 6,333 | 10 | **no** |
| 068270 | 92,700 | 100 | yes |

`005930`'s 30,340 is its pre-split price carried through the 2018 50:1 split. Three of
five sampled prices sit on **no exchange tick**. An effective-dated ladder lookup — the
obvious fix, and the one this repo's own
[`exchange-rule-constants-need-an-effective-date-switch-before-history-is-acquired`](../conventions/exchange-rule-constants-need-an-effective-date-switch-before-history-is-acquired.md)
convention points at — would still have refused those three. **Adjustment destroys the
exchange grid, and no ladder can describe the series afterwards.**

## The fix

Per symbol, mount `gcd(g, f)` where `g` is the GCD of every in-range OHLC price and `f`
is the adapter's increment (`regrid_instruments_for_range`, lab-side, run-scoped). Three
properties, and the first is why it is the right shape:

1. It **divides every price the engine will see, by construction** — no fill can be
   skipped as a structural fact, not as a runtime check that has to fire to help.
2. It is never coarser than `f` (when `f` already divides everything, `gcd(g, f) == f`
   and nothing moves), so a sparse symbol cannot invent an absurd grid from one bar.
3. For a symbol no corporate action touched it **recovers the real exchange tick** —
   `000660` re-grids to 50, not to 1 — so the increment keeps as much meaning as the
   data still supports.

Nothing is rounded and no price is invented: the daily path submits market orders and
fills at real catalog bar prices, so the increment is only a fill-price *validator*
here. The live path is untouched; `map_equity` still does the right thing for orders.

Result on the same range and binary profile: skipped fills 7,981 → **0**, unopened entry
orders 5,167 → **0**, positions 1,311 → **5,922**, 243 of 286 instruments re-gridded.

## Three transferable lessons

**A live-trading object and a historical-simulation object are not the same object,
even when they share a type.** `price_increment` is the seam where the two requirements
collide: one wants today's band, the other wants a grid that admits history. The bug is
not in either derivation, it is in reusing one object for both roles.

**Do not swap a nautilus instrument mid-run to fix this.** In 0.60,
`SimulatedExchange::add_instrument` on an id that already exists constructs a **new**
`OrderMatchingEngine` and inserts it over the old one, discarding that engine's book and
state, and derives `raw_id` from `self.instruments.len()`, which does not grow on a
replace — so every symbol swapped within one session collides on a single raw id. A
run-scoped increment per symbol needs none of that.

**A fixture whose values are chosen to satisfy a validator cannot test that validator.**
This defect was reachable from the day the daily path shipped, and the path's own fixture
already documented the mechanism:

> *"Every price is a multiple of 100 — the KRX instrument masters this fixture ingests
> carry `price_increment = 100`, and the matching engine skips the fill (a WARN, not an
> error) for any price off that grid, so an off-grid fixture silently trades nothing."*

The behaviour was known and worked around **in the fixture**, which is exactly why first
contact with a real deep-history catalog was the first time it could bite. When a fixture
comment explains how to avoid tripping a check, that check has no coverage — and the
comment is the tell.

## A related trap when repairing the tests

One existing scenario used the off-grid price *deliberately*, as the only lever that
produced a submitted entry order which never opens a position and could still be aimed at
one symbol. The fix removes that lever. The replacement is a **zero-volume session**:
the matching engine derives its per-bar tick sizes from the bar's volume
(`BarTickSizes::from_volume`) and `process_bar_trade_tick` returns early on a zero size,
so a bar that prints prices but no volume generates no trade tick and an order submitted
on it is never filled. A starting balance too small for the notional is not a substitute
— it denies at the *risk engine*, which is a rejection rather than a silent non-fill, and
it is account-wide.
