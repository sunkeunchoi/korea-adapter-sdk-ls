---
title: A constant modelling an exchange rule is latent-wrong until history is acquired — give it an effective-date switch, and treat an adjacent switched constant as the tell
date: 2026-08-03
category: docs/solutions/conventions
module: adapters/nautilus/src/rules.rs, adapters/nautilus/src/ingest
problem_type: convention
component: ingest
severity: high
applies_when:
  - modelling a KRX market rule (session hours, tick ladder, price-limit band, trading unit) as a Rust constant
  - extending an ingest window backwards to acquire history predating the current catalog
  - reviewing a constant that stamps or bounds a bar timestamp
---

## The defect class

An exchange rule modelled as a **flat constant** is not wrong today. It becomes
wrong the moment history is acquired back past the rule's last change — and it
fails **silently**, because nothing in the pipeline knows the rule ever moved.

The live instance in this tree: `KRX_REGULAR_CLOSE = 15:30`
(`adapters/nautilus/src/rules.rs`) has **no effective-date switch**, while
`TICK_REFORM_DATE` / `TickRegime::for_date` sit a few lines below it and **do**.
KRX's regular-session close actually changed **15:00 → 15:30 on 2016-08-01**
(the open never moved). Today's catalog is entirely post-change, so the constant
is correct for every bar the repo currently holds. Acquiring history to
`2010-01-04` — the stated destination — mis-stamps roughly **1,630 sessions** by
+30 minutes, with no error, no warning and no failing test.

## The convention

**Before extending an ingest window backwards, enumerate every constant that
encodes an exchange rule and establish whether that rule changed inside the new
window.** Do it as an explicit step, not as a consequence of something failing —
by construction, nothing will fail.

Three rules of thumb this instance produced:

1. **An adjacent constant that already carries an effective-date switch is the
   tell.** If one rule in a module needed `for_date`, its neighbours model rules
   of the same kind and probably need one too. Asymmetry between siblings is a
   smell worth one grep.
2. **Distinguish the consumers, because they fail differently.** In this tree the
   same constant is read to *stamp* a daily bar (`ingest/mod.rs`), to derive
   *ingest range bounds*, and for the *watermark*. A timestamp error and a
   filter-boundary error are different defects with different blast radii and
   need separate remediation — do not treat "one constant" as "one bug".
3. **Check whether the constant is actually load-bearing before widening the
   fix.** `KRX_REGULAR_OPEN` is equally flat but **inert** — referenced only by
   its own assertion, never by ingest — and the open did not move anyway. That
   narrowed the finding instead of doubling it.

## KRX rules known to have changed in-window (established 2026-08-03)

Any of these that a constant encodes needs a date switch before history spans it:

| effective | rule |
|---|---|
| 2014-09-01 | dynamic VI introduced *(secondary source)* |
| **2015-06-15** | daily price limit **±15% → ±30%**; static VI introduced *(KRX's own 가격제한폭 변경 내역 register)* |
| **2016-08-01** | **regular-session close 15:00 → 15:30**; closing auction 14:50–15:00 → 15:20–15:30 |
| **2019-04-29** | opening-auction order entry 08:00 → **08:30**; pre-open block 07:30 → 08:00 |
| **2023-01-25** | tick-size table revision — and it **coarsened** the KOSDAQ 200,000–500,000원 tick from 100원 to 500원 |
| 2026-09-14 *(planned)* | after-market 16:00–20:00 opens; regular close stays 15:30 |

Two traps in that table. The tick reform is usually described as making ticks
finer — for one KOSDAQ band it did the opposite, so a KOSPI-only tick lookup is
wrong for pre-2023 KOSDAQ high-priced names. And the 2026-09-14 after-market
does **not** move the close, but it does break anything treating 15:30 or 18:00
as "end of day".

Full source list, with `[PRIMARY]`/`[SECONDARY]` classification per date, is in
`docs/research/krx-part-a-public-pass-findings.md` (group A1). Note the
2016-08-01 date is `PARTIALLY SETTLED`: the change and the current state are
primary, the **effective date** rests on ≥6 agreeing secondary sources because
KRX's 2016 보도자료 and the rulebook 부칙 are not publicly reachable.

## Status of the live instance

**Recorded, not fixed.** It belongs to
[#254](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/254)'s
acquisition layer; the wayfinder map ends before implementation. Filed as a
correction on
[#242](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/242#issuecomment-5164659108).
It is **not** a defect in today's catalog.
