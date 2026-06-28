---
title: "Disposition a paper-unavailable read by its terminal — 01900 service-rejection, 01491 account-incapable, or 00707 in-window feed-unprovisioned — and keep facets.paper_incompatible distinct from the runtime classifier"
date: 2026-06-26
category: conventions
module: ls-sdk Paper Live Smoke harness, ls-core paper classifiers, implement-tr recipe, metadata facets
problem_type: convention
component: tooling
severity: medium
last_updated: 2026-06-28
applies_when:
  - "A make live-smoke-<tr> returns a paper-unavailable signal and you must decide whether it can ever flip on paper"
  - "Distinguishing an off-window-empty (recoverable on re-run) from an in-window-empty (feed-unprovisioned, never recovers on paper)"
  - "Setting or reading facets.paper_incompatible: true and reasoning about whether the runtime ls_core::is_paper_incompatible() will fire"
  - "Reclassifying night-derivatives, overseas, or any feed the LS paper environment may not provision"
tags:
  - paper-live-smoke
  - paper-incompatible
  - feed-unprovisioned
  - disposition
  - implement-tr
  - metadata-facets
---

# Paper-unavailable disposition terminals (and the facet vs. runtime-classifier divergence)

## Context

The LS paper (모의투자) gateway can signal "this read will not work on paper" in
several distinct ways, and they demand **different dispositions**. Conflating them
causes two failures: endlessly re-running a smoke against a feed paper will never
serve (treating a permanent gap as a timing miss), or mis-reading the
`facets.paper_incompatible` metadata flag as a promise that the runtime
`01900` classifier will fire.

This convention names the terminals and pins the one non-obvious invariant: the
metadata facet and the runtime classifier are **decoupled**, and a TR can carry
`facets.paper_incompatible: true` while `ls_core::is_paper_incompatible()` never
fires for it.

## Guidance

There are three paper-unavailable terminals. Classify by the gateway response, not
by the session clock alone:

| Terminal | Gateway signal | Runtime classifier | Disposition |
|---|---|---|---|
| **Service-rejection** | `rsp_cd = 01900` (hard reject, any window) | `ls_core::is_paper_incompatible()` **fires** (`PAPER_INCOMPATIBLE_CODE = "01900"`) | `facets.paper_incompatible: true`; never flips on paper |
| **Account-incapable** | `rsp_cd = 01491` (per-account, e.g. order-incapable paper account) | `ls_core::is_paper_order_incapable()` **fires** (`PAPER_ORDER_INCAPABLE_CODE = "01491"`) | Pending until an account that supports the operation is provisioned |
| **Feed-unprovisioned** | clean `rsp_cd` (`00000`/`00707`, or `00009 해당 자료가 없습니다`) with an **empty body**, **even when smoked in-window** | **neither fires** — the response is a success, not a rejection | `facets.paper_incompatible: true` as a doc/routing flag only; never flips on paper |

**The disambiguating test for feed-unprovisioned vs. off-window: run inside the
nominal session window.** An off-window-empty recovers on an in-window re-run
(disposition `pending:off-window`); an **in-window-empty** does not — it means the
paper environment carries no data for that feed at all (`feed-unprovisioned`,
paper-unavailable). Before recording feed-unprovisioned, rule out two other
"looks empty but isn't a feed gap" causes: a stale smoke symbol (see
[[stale-smoke-symbol-masks-provisioned-feed]]) — an expired contract symbol
returns success+empty and masks a provisioned feed — and, for an `account_state`
read, **wrong-account-binding**: the account is token-bound, so an empty `00707`
may mean the smoke authenticated as the wrong account, not a feed gap. Re-probe
under the read's credential lane before the terminal (see
[[ls-account-token-bound-credential-lanes]]).

**The facet/runtime divergence (the load-bearing nuance).**
`facets.paper_incompatible` is a **pure metadata doc/routing flag** — it is read only
as a free `bool` in the schema and rendered as a yes/no in docgen. The runtime
`ls_core::is_paper_incompatible(code)` is a **separate, response-code check** bound to
`01900`. They are intentionally decoupled:

- Setting `facets.paper_incompatible: true` documents "won't flip on paper" so the
  discovery query and future waves skip the TR. It does **not** assert the gateway
  returns `01900`.
- For a feed-unprovisioned TR (`00707`), the facet is set but the runtime classifier
  **never fires** — the response is a success, not a rejection.

Do not write code that assumes `facets.paper_incompatible == true` implies the
runtime classifier fires, or vice versa. The facet is documentation; the classifier
is wire behavior.

## Why This Matters

- **Stops wasted re-window retries.** A feed-unprovisioned read looks identical to an
  off-window read at a glance (both empty). Without the in-window test, a wave keeps
  re-scheduling smokes against a feed paper will never serve.
- **Keeps facet semantics honest.** Treating `paper_incompatible` as "returns 01900"
  would make a future consumer mis-handle the `00707` TRs (which never return 01900).
  The facet means "paper-unavailable, do not expect a flip," nothing more.
- **Preserves an accurate disposition record.** The terminal (and its evidence)
  belongs in `metadata/PROVISIONALITY-LEDGER.md` with the precise code, so a later
  reader can tell a service-rejection from a feed gap without re-smoking.

## When to Apply

- Classifying a paper live-smoke that returns empty or a rejection and deciding
  whether the TR can ever flip on paper.
- Reclassifying night-derivatives (`t8455`/`t8460`/`t8463`), overseas reads
  (`g3101`–`g3190`), or any instrument domain the paper environment may not carry.
- Setting `facets.paper_incompatible: true`, or reading it in docgen / a discovery
  query, and reasoning about runtime behavior.

## Examples

**Night-derivatives + overseas reads — in-window empties → feed-unprovisioned
(2026-06-26, ledger §14).** The night trio was smoked at 01:11 KST (inside the
`krx_extended` ~18:00–05:00 window) and the overseas sextet at 12:11 ET (inside the
US regular session). All returned empty `00707` (g3103 returned
`00009 해당 자료가 없습니다`). Because they were in-window, this is **not** off-window:
the paper gateway provisions no data for these feeds. Disposition: set
`facets.paper_incompatible: true` on all nine (doc flag), `support.implemented` stays
`false`, record the per-TR evidence in PROVISIONALITY-LEDGER §14. Zero flips, so docgen
counts (`reference.len()` / `banner_trs`) are unchanged.

**Night trio re-probed under the F/O lane (2026-06-28, ledger §17) — basis
refined, terminal held.** The §14 night masters were smoked on the domestic
account; re-probed under `.env.domestic_option` (the F/O-capable `…51` account),
`t8455`/`t8460`/`t8463` now return `rsp_cd=00000` (the venue *accepts* the request —
no longer the §14 `00707`), but the typed out-block is empty **off** the
`krx_extended` window. The wrong-account cause is ruled out (the account is
entitled), so the remaining question is purely in-window data: `paper_incompatible`
is **retained conservatively** (no positive data observed yet) with an in-window
`…51` re-smoke recorded as the outstanding flip gate. Contrast the CCENQ pair, which
still returns a hard `01900` under the same `…51` account — a true venue rejection,
not a wrong-account or off-window artifact.

**CCENQ night pair — 01900 service-rejection (ledger §12).** `CCENQ90200` /
`CCENQ10100` return a hard gateway `01900` regardless of the night window.
`ls_core::is_paper_incompatible()` fires. Same facet (`paper_incompatible: true`), but
a *different terminal*: here the runtime classifier confirms it, where the
feed-unprovisioned case above leaves the classifier silent.

**Order account — 01491 account-incapable.** A paper account provisioned
read/inquiry-only returns `01491` on an order submit;
`ls_core::is_paper_order_incapable()` fires. This is per-account, not per-service:
the same TR flips once an order-capable paper account is in `.env`. See
[[ls-paper-01491-account-not-order-capable]].

## Related

- [[market-hours-read-empty-result-disposition]] — the off-window vs. in-window
  branch this convention extends to the paper-unavailable terminal.
- [[stale-smoke-symbol-masks-provisioned-feed]] — rule out a rolled contract symbol
  before recording feed-unprovisioned.
- [[ls-account-token-bound-credential-lanes]] — rule out wrong-account-binding (an
  account read is token-bound) before recording the `00707` terminal for an
  `account_state` read.
- [[ls-paper-01491-account-not-order-capable]] — the 01491 (per-account) vs. 01900
  (per-service) distinction.
- `metadata/PROVISIONALITY-LEDGER.md` §12 (01900 terminal) and §14 (00707
  feed-unprovisioned terminal).
- `crates/ls-core/src/inner.rs` — `PAPER_INCOMPATIBLE_CODE` / `is_paper_incompatible`
  and `PAPER_ORDER_INCAPABLE_CODE` / `is_paper_order_incapable`.
