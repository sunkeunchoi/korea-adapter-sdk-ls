---
title: "An LS account read is token-bound, not number-bound — an empty 00707 / IGW40013 / all-default 00136 may be the WRONG account, not no-data; route a per-account credential lane by instrument_domain and re-probe before disposition"
date: 2026-06-28
category: conventions
module: ls-core config (from_env), ls-sdk account owner_class, Paper Live Smoke harness, implement-tr recipe
problem_type: convention
component: tooling
severity: high
applies_when:
  - "An account_state read (deposit, balance, orderable-qty, deposited-assets) smokes empty 00707 / all-default 00136 / a gateway error (IGW40013) and you are about to disposition it PENDING or paper_incompatible"
  - "Scoping a flip wave over F/O or overseas-F/O account reads that prior waves deferred as paper carries no data"
  - "Deciding whether a paper account read failure is a feed gap, an unfunded account, or a wrong-account-binding artifact"
  - "Adding per-account paper credentials and routing a smoke to the right account"
tags:
  - paper-live-smoke
  - account-state
  - credential-lane
  - wrong-account-artifact
  - rsp-cd-signature
  - implement-tr
  - flip-wave
related_components:
  - tooling
---

# LS account reads are token-bound — rule out the wrong account before "no data"

## Context

The LS gateway resolves the target account **entirely from the OAuth token (the
appkey the token was issued for)**. The account number is **never sent on the
wire** — dispatch sends only `tr_cd`, continuation, content-type, and the bearer
token (`crates/ls-core/src/inner.rs`), and LS's own account-read request bodies
carry no account field. So **one appkey reaches exactly one account**; forcing
`LS_PAPER_ACCOUNT` to a different number does nothing, and reaching a different
account requires a *different appkey*.

This was proven by a 2026-06-28 diagnostic that read the gateway-echoed `AcntNo`
per credential file: the default appkey resolved `…01` for both domestic and F/O
reads; forcing the option account number changed nothing; each per-account appkey
resolved to its own account (`…01` domestic, `…51` domestic-option, `…71`
overseas-option). See [[Credential lane]] in `CONCEPTS.md`.

## Guidance

**An `account_state` read that comes back empty `00707`, all-default `00136`, or a
gateway error like `IGW40013` may be authenticating as the WRONG account, not
carrying no data.** Before dispositioning it PENDING ("unfunded") or
`paper_incompatible` ("feed-unprovisioned"), rule out wrong-account-binding by
re-probing under the credential lane whose account actually owns that read's data.

Multi-account access is **one credential FILE per account**, not a number swap.
Each lane file is a `(LS_PAPER_APIKEY, LS_PAPER_SECRET, LS_PAPER_ACCOUNT)` triple;
the smoke harness sources the right file by the TR's `instrument_domain` facet:

| `instrument_domain` | lane file | account |
|---|---|---|
| `stock` / unmapped (default lane) | `.env.domestic` | `…3701` |
| `futures_options` | `.env.domestic_option` | `…51` |
| `overseas_stock` | `.env.overseas` | `…` |
| `overseas_futures` | `.env.overseas_option` | `…71` |

Routing lives in the Makefile (a target-specific `LS_SMOKE_LANE`), not the SDK —
the SDK stays single-config ("one client = one account/token"); the Makefile
resolves an empty `LS_SMOKE_LANE` to the `domestic` lane, then sources
`.env.<lane>` in the recipe shell and **fails fast if the lane file is missing**
— every lane (including the default) runs the guard; there is no bare-`.env`
fallback (the legacy `.env` was deleted in the env-lane cutover, plan
2026-07-01-002; a silent fall-back would re-introduce the wrong-account bug).

### The `rsp_cd` flip-signature for account reads

Classify by `rsp_cd` AND a substantive modeled witness — never by `body_len`:

| Signal | Meaning | Disposition |
|---|---|---|
| `00136` "조회가 완료되었습니다" + a **non-default modeled witness** | success **with** real account state | **FLIP** |
| `00136` / `00000` + an all-default / empty modeled out-block | reachable but empty (unfunded, no positions, or wrong account) | re-probe under the right lane; else PENDING |
| `00707` | empty / no-data | re-probe under the right lane; else PENDING / feed-unprovisioned |
| `01900` | hard venue rejection (persists under any account) | `paper_incompatible` (see [[paper-unavailable-disposition-terminals]]) |
| `IGW40013` / other gateway error | may be wrong-account too — re-probe under the lane before calling it a defect | re-probe; else defer |

`00136` is **not** "≠ data": it carries real account state **when authenticated as
the right account**. The earlier reading that `00136` ≈ `00707` came from smoking
F/O reads on the domestic cash account, where they legitimately had nothing.

**Assert the substantive modeled field, not `body_len`.** A large body can still
deserialize to an empty modeled array — the off-window night masters `t8455`
(body 1498) and `t8463` (body 4631) returned `rsp_cd=00000` with substantial
bytes but an **empty** typed out-block. The typed smoke (R10 witness gate) is
authoritative; `make raw-probe` `body_len` is not a substance signal.

## Why This Matters

Earlier waves deferred `CFOEQ11100`, `t0441`, `CIDBQ01400` (PR #63, ledger §16) and
left `CIDBQ03000` / `CIDBQ05300` in the deferred/`IGW40013` lists as "paper carries
no data / no F/O funding." Every one of those smokes authenticated as the domestic
cash account. Re-smoked under their own credential lane (plan -002, ledger §17),
**four flipped to Implemented** on `00136` + a non-default deposit/orderable witness
(`CFOEQ11100`, `CIDBQ01400`, `CIDBQ03000`, `CIDBQ05300`), and the §16 "no F/O
funding" gate conclusion was **retracted as a wrong-account artifact**. `t0441`
stayed PENDING for a genuinely different reason (the `…51` account is funded but
holds no open F/O positions). Without the token-bound insight, a future wave would
re-prospect these dry codes forever.

## When to Apply

- Any flip wave over F/O / overseas-F/O `account_state` reads. Re-probe under the
  lane **before** recording PENDING or `paper_incompatible`.
- Whenever a paper account read returns `00707` / all-default `00136` / `IGW40013`
  — distinguish three causes: wrong-account-binding (fixed by the lane), genuinely
  unfunded / no-positions (the [[closed-window-account-capacity-reads-all-default]]
  Holdings gate), and feed-unprovisioned (venue carries no data, in-window-empty
  under the *right* account — see [[paper-unavailable-disposition-terminals]]).
- Adding a new account read: set its `instrument_domain` so the harness maps the
  lane; assert the substantive witness in both the offline fixture test and the
  live smoke.

## Examples

```make
# Makefile: target-specific lane, fail-fast on a missing lane file (no .env fallback).
live-smoke-cfoeq11100 ... live-smoke-t8406: LS_SMOKE_LANE = domestic_option
live-smoke-cidbq01400 live-smoke-cidbq03000 live-smoke-cidbq05300 ...: LS_SMOKE_LANE = overseas_option
# run_smoke sources .env.$(LS_SMOKE_LANE), defaulting an empty lane to .env.domestic; missing lane file -> FAIL.
```

```
# Same TR, same gateway code, different ACCOUNT — the flip is the lane, not the data:
CFOEQ11100 on .env (…01, cash):           rsp_cd=00136 Dps all-zero            -> PENDING (§16)
CFOEQ11100 on .env.domestic_option (…51): rsp_cd=00136 dps_nd=true             -> FLIP    (§17)
CIDBQ05300 on .env (…01):                 IGW40013 gateway error               -> deferred (§16)
CIDBQ05300 on .env.overseas_option (…71): rsp_cd=00136 rows=5 OvrsFutsDps nz   -> FLIP    (§17)
```

## Prevention

- **Never conclude "paper carries no data" for an account read from a single
  credential.** It may be the wrong account. The fix is a credential lane, not a
  number change, and not a re-window.
- **`body_len` is not a substance signal** — assert the typed modeled witness.
- A date-keyed account read (e.g. `CIDBQ03000` `TrdDt`) needs a **trading day**; a
  weekend returns `01900`-lookalike `01715` (non-trading day). Walk the smoke back
  to the most recent weekday, or pass an override.
- Keep the real-money interlock: a paper run must never resolve to a real-money
  appkey. `from_env`'s `resolve_paper_appkey` fails fast when a paper key and a
  bare `LS_APPKEY` / `LS_REAL_APPKEY` are both set.

## Related

- [[Credential lane]] (`CONCEPTS.md`) — the per-account credential set this routes.
- [[closed-window-account-capacity-reads-all-default]] — the Holdings gate; its F/O
  "all-default = unfunded" finding is **corrected** here (those were wrong-account).
- [[paper-unavailable-disposition-terminals]] — rule out wrong-account-binding
  before recording the `00707` feed-unprovisioned terminal.
- [[stale-smoke-symbol-masks-provisioned-feed]] — a sibling "looks empty but isn't a
  feed gap" cause (a rolled contract symbol) to rule out alongside wrong-account.
- [[ls-gateway-igw40011-numeric-request-fields]] — numeric request-field serializer.
- `metadata/PROVISIONALITY-LEDGER.md` §16 (the deferred reads) and §17 (their
  wrong-account correction + flips).
