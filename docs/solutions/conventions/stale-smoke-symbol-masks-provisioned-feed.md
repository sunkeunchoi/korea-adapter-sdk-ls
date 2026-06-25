---
title: "Refresh an overseas-futures/-option read's smoke symbol to the current front-month before dispositioning an empty paper smoke as feed-unprovisioned"
date: 2026-06-25
category: conventions
module: ls-sdk Paper Live Smoke harness, implement-tr recipe
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Smoking an overseas-futures or overseas-future-option read (o3105/o3106/o3125/o3126) keyed by a registered contract symbol"
  - "A make live-smoke-<tr> returns a success rsp_cd (00000/00707) with an empty out-block and you are about to record pending:feed-unprovisioned"
  - "The registered smoke symbol carries an expiry (e.g. CUSN23, HSIM23) that may have rolled since it was registered"
tags:
  - paper-live-smoke
  - stale-symbol
  - front-month
  - overseas-futures
  - o3101-master
  - disposition
  - implement-tr
related_components:
  - tooling
---

## Context

Overseas-futures and overseas-future-option reads (`o3105`/`o3106` quote and order-book,
`o3125`/`o3126` future-option quote and order-book) are keyed by a **dated contract
symbol** (e.g. `CUSN23` = crude/Renminbi 2023.07, `HSIM23` = Hang Seng 2023.06). A
contract symbol expires: once its month rolls, the gateway returns a **success**
`rsp_cd` (`00000`) with an **empty** out-block for that symbol — byte-for-byte
indistinguishable from a feed that is genuinely not provisioned in paper.

A wave of these four TRs was about to be dispositioned `pending:feed-unprovisioned` on
empty smokes, with the old generated SDK's "Transport-only" certification cited as
corroboration that no data frame was ever decoded. The real cause was that the
registered symbols (`CUSN23`/`ADM23`/`HSIM23`) were **2023-expiry**. Refreshing them
to the current front-month made all four return non-empty and flip to **Implemented** —
the feed had been provisioned the whole time. The stale symbol masked it.

This is the **second false-empty axis**. The first — an empty result that is really the
market clock, not the TR — is covered in
[`market-hours-read-empty-result-disposition.md`](./market-hours-read-empty-result-disposition.md).
A stale dated symbol is a distinct cause that the session-clock branch does not catch:
the symbol is wrong even when the session is open.

## Guidance

Before recording `pending:feed-unprovisioned` (or any "feed not provisioned" verdict)
on an empty overseas-futures/-option smoke, **re-resolve the smoke symbol to the
current front-month** and re-run. Only an empty result on a confirmed-current symbol
justifies the feed-unprovisioned disposition.

1. **Resolve front-month from the live master, not by guessing.** Run
   `make live-smoke-o3101` (the `overseas_futures_master`, `O3101Request::new("")` = all
   products). It returns the full contract list on paper anytime — it is not
   session-bound. Each `O3101OutBlock` carries `symbol` (e.g. `CUSN26`) and `symbol_nm`
   with the listing month in the name (e.g. `Renminbi_USD/CNH(2026.07)`); pick the
   nearest-expiry contract for the target underlying.
2. **The decoded `O3101OutBlock` does not expose the structured expiry.** The
   normalized baseline carries `LstngYr`/`LstngM`/`ApplDate`, but the struct decodes
   only `symbol`/`symbol_nm`/`bsc_gds_*`/`exch_cd`/etc. So front-month selection is
   currently a parse of the `(YYYY.MM)` suffix in `symbol_nm` (or the month-code letter
   in `symbol`). Extending the struct to decode `LstngYr`/`LstngM` and re-projecting via
   `make api-drift-renormalize` is the structured alternative if this recurs.
3. **The option master (`o3121`, `mktgb="O"`) returns base-goods rows only** — its
   `symbol` field is empty (e.g. `O_HSI`, `O_HCEI` with no per-contract code). It does
   **not** yield a dated option symbol to refresh to. For the `mktgb="F"` future-option
   reads (`o3125`/`o3126`), drive them off a current **futures** front-month from
   `o3101` instead.
4. **An underlying may be absent from the master.** The registered `ADM` underlying
   (`o3106`/`o3126`) was not in the `o3101` list at all; a confirmed-current symbol from
   a present underlying (`HSIM26`, Hang Seng) was substituted so the smoke exercises a
   real contract. Note the substitution at the smoke site and in the smoke-map.
5. **Only then disposition.** Empty on a current symbol → `pending:feed-unprovisioned`
   is now defensible. Non-empty → **Implemented** (the same clean-non-empty bar as any
   read). The smoke's existing `out-block.is_empty()` guard before `record()` keeps an
   empty result from recording false evidence either way.

## Why This Matters

A success-`rsp_cd` + empty out-block has at least three causes — closed session, stale
dated symbol, and genuinely unprovisioned feed — and they are byte-identical on the
wire. Treating "empty" as "unprovisioned" without first ruling out the symbol books a
callable, provisioned TR as permanently PENDING and silently understates how much of
the surface is actually reachable in paper. In the wave that produced this doc, that
mistake would have left four Implemented-eligible reads stranded as `feed-unprovisioned`
— a 4-TR undercount — and would have "confirmed" the wrong root cause by leaning on the
old SDK's Transport-only note, which proves the request serialized cleanly but says
nothing about whether the symbol was live.

## When to Apply

- Any overseas-futures/-option read keyed by a dated contract symbol (`o31xx` family)
  that smokes empty, **before** writing a feed-unprovisioned or environmental verdict.
- Most acute when the registered smoke symbol's embedded year is in the past relative
  to the smoke date — a strong signal the symbol has rolled.
- Not needed for reads keyed by a non-expiring identifier (equities by ticker, master
  list reads); those have no front-month to refresh.

## Examples

```
# Stale registered symbol (2023-expiry) — empty result is uninformative
make live-smoke-o3105   # O3105Request::new("CUSN23  ") -> rsp_cd=00000 quote=0
#   do NOT record feed-unprovisioned: the symbol, not the feed, may be the cause

# Resolve the current front-month from the master (anytime, not session-bound)
make live-smoke-o3101   # rows=85; O3101OutBlock symbol=CUSN26 nm=...(2026.07)

# Re-run with the refreshed front-month symbol
make live-smoke-o3105   # O3105Request::new("CUSN26  ") -> rsp_cd=00000 quote=1 -> IMPLEMENTED
```

The same empty `o3105` smoke means two different things depending on whether its symbol
is current; only re-resolving against the `o3101` master tells them apart. See also
[`market-hours-read-empty-result-disposition.md`](./market-hours-read-empty-result-disposition.md)
(the session-clock false-empty axis this complements),
[`tr-out-block-shape-from-raw-capture.md`](./tr-out-block-shape-from-raw-capture.md)
(the "assert non-empty in the smoke" rule that keeps a fake-empty from recording), and
`docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`
(the `raw-probe` classifier for separating a real defect from environmental noise).
