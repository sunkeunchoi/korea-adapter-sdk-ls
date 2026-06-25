---
date: 2026-06-24
topic: tr-implement-wave-night-overseas-elw
---

# Implement wave — 17 Tracked TRs (KRX-night, F/O, overseas, ELW)

## Summary

Flip 17 already-Tracked TRs to Implemented in one wave by re-running their existing
Paper Live Smokes in-window and flipping on a clean non-empty smoke. The callable
Rust, dual-registered `{TR}_POLICY` consts, and `live_smoke_<tr>` harnesses for all
17 are **already scaffolded and committed on main** (Tracked-but-callable from prior
PENDING dispositions) — the remaining work is the in-window smoke re-run plus the
metadata flip and docgen-count bump, not authoring. Realistic landing: ~10 should
smoke clean while the KRX-night and US-regular sessions are open (3 KRX-night
derivatives `t8455`/`t8460`/`t8463`, `t2106`, 6 overseas-stock); 2 account-night TRs
(`CCENQ10100`, `CCENQ90200`) are already terminal `paper_incompatible` with no flip
expectation; and 5 PENDING-risk TRs (`o3105`, `o3106`, `o3125`, `o3126`, `t1964`)
re-run but are expected to stay PENDING, dispositioned faithfully rather than dropped.

## Problem Frame

Of the 23 candidate TRs in the batch, all are already Tracked (the 6 not in this
wave are dispositioned under Scope Boundaries) — metadata, `tr-index` entries, and
normalized baselines exist, and the smoke registry already maps targets for the
recommended 17 (units U5–U8 plus the account-night and ELW-board entries). Their
callable Rust, policy consts, and smoke harnesses are already committed on main from
prior scaffolding; the work left is only the Tracked → Implemented rung — re-run each
Paper Live Smoke in-window and flip the metadata + docgen counts on a non-empty result.

Three facts shape the wave. First, Implemented yield is **timing-bound**: the three
flippable KRX-night derivatives TRs (`t8455`/`t8460`/`t8463`) only return data during
the `krx_extended` night window (~18:00–05:00 KST), and the six overseas-stock reads
only return data while the US regular session is open. Both windows are open now
(22:29 KST), so this is the moment that maximizes clean flips; the same smokes run
off-window land empty (PENDING), which is a re-run condition, not a defect or a drop.

Second, two of the account-night TRs are already settled. `CCENQ10100` and
`CCENQ90200` carry `paper_incompatible: true` and return a terminal gateway `01900`
regardless of the night window (ledger §12) — no in-window retry changes that. They
are authored for callability but carry no flip expectation; they are not part of the
~10 clean-smoke set.

Third, the old generated SDK at `korea-broker-sdk-ls` certified `o31xx` and `t1964`
at **Transport level only** — its taxonomy means the request was exchanged without
protocol error but no real data frame was ever decoded. Its `t1964` fixture used the
same broad `"0"` filter defaults this SDK already tried. So the old source supplies
validated, transport-clean request shapes for these five but proves none was ever
data-bearing — the empty board and unprovisioned overseas-futures feed are real,
not wire-shape bugs.

## Key Decisions

- **Smoke-and-flip all 17, disposition faithfully — don't defer the PENDING-risk
  ones.** The callable code already exists on main; this wave re-runs every smoke and
  flips what comes back non-empty. The old SDK corroborates only the `o31xx`/`t1964`
  **request** shapes (transport-clean serialization) — their response structs rest on
  the normalized baseline, not on any decoded frame — which was enough to scaffold
  them. They re-run and are expected to stay PENDING (callable-but-unattested) per the
  never-drop ethos, not Tracked-and-skipped.

- **The Implemented bar stays "clean non-empty smoke."** No TR flips on an empty
  board or empty out-block to grow the count. Off-window empties, the unprovisioned
  overseas-futures feed, and the empty ELW board all resolve to PENDING — the honest
  landing for those TRs this wave.

- **Port wire shapes from `korea-broker-sdk-ls` where present.** Request/response
  field names, types, and array-vs-single shapes come from the normalized baseline
  (`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`); the old
  generated structs corroborate them and supply the `t1964` 11-field InBlock and the
  `o31xx` symbol-only / `mktgb`+`symbol` InBlocks.

- **Route by owner_class, follow the established per-domain seams.** Account-night
  TRs route through `account`; F/O and overseas reads through `market_session`
  (overseas `instrument_domain` → `market_session`, KTD3). Numeric request-body
  fields (e.g. `t8463.cnt`) serialize as JSON numbers via `string_as_number` or the
  gateway returns `IGW40011`.

## Requirements

**Authoring (already committed on main — verify, do not re-author)**

- R1. The callable Rust for all 17 — request/response structs matching each TR's
  normalized baseline, in the owner_class module (`account` for CCENQ; `market_session`
  for F/O and overseas reads; ELW board `t1964` via the KTD3 `market_session`
  correction of its standalone placeholder, no owner_class metadata flip until a
  confirming non-empty smoke) — already exists on main. Verify it; do not author a
  second copy.
- R2. Each `{TR}_POLICY` const is already registered in **both** policy cross-check
  lists per the `implement-tr` recipe. Verify; do not duplicate.
- R3. Numeric request-body fields already serialize as JSON numbers
  (`string_as_number`); response fields use the tolerant `string_or_number`. Verify.

**Smoke and flip discipline (the actionable work)**

- R4. Each TR's `make live-smoke-<tr>` / `live_smoke_<tr>` harness already exists,
  hitting the real LS paper gateway with the smoke-map inputs (see Dependencies).
  Refresh stale inputs (e.g. the o31xx contract symbols, see Dependencies) before
  re-running.
- R5. A TR flips to `support: implemented` only on a smoke that returns a
  non-empty data-bearing response; the smoke asserts non-empty before recording
  evidence.
- R6. A TR whose smoke returns empty lands **PENDING** (callable, never dropped),
  recorded with the cause that determines its recovery path:
  - `pending:off-window` — empty because the session was closed; recovers on an
    in-window re-run (the KRX-night trio, overseas-stock).
  - `pending:feed-unprovisioned` — empty because the paper feed is not provisioned;
    a re-run does **not** recover it (o31xx).
  - `pending:input-unresolved` — empty because required inputs are unsourced; needs
    new inputs, **not** a re-run (t1964 filter enums).
- R7. A TR whose gateway response is `01900` (paper-incompatible) is
  dispositioned **paper_incompatible**, distinct from PENDING.

**Gate and docs**

- R8. `make docs`, `cargo test`, `cargo test -p ls-core`, and `make docs-check`
  pass before any commit; the tree never commits red.
- R9. Docgen counts (`reference.len`, banner_trs) and any maintained-TR counts
  move consistently with the number of TRs actually flipped to Implemented.

## Acceptance Examples

- AE1. **Covers R5, R6.** A KRX-night TR (e.g. `t8460`) smoked during the night
  window returns a non-empty option board → flips to Implemented. The same smoke
  run after ~05:00 KST returns an empty board → lands PENDING (off-window re-run),
  not Implemented and not a failure.
- AE2. **Covers R6.** `t1964` smoked with the broad `"0"` filter defaults returns
  an empty board (as the old SDK saw) → ships PENDING (filter-default-unresolved),
  callable, with the `t9905 → t1964` discovery edge retained unconfirmed.
- AE3. **Covers R6.** An overseas-futures TR (e.g. `o3105`) smoked with a **current
  front-month symbol** returns an empty out-block → ships `pending:feed-unprovisioned`,
  callable. Smoked with a stale (2023-expiry) symbol the empty result is uninformative
  and must not be recorded as feed-unprovisioned — refresh the symbol and re-run.
- AE4. **Covers R5.** An overseas-stock TR (e.g. `g3101`) smoked with `82`/`TSLA`
  while US regular session is open returns a non-empty out-block → flips to
  Implemented.
- AE5. **Covers R5.** `t2106`'s non-empty gate is the populated price-memo out-block
  (memo-row count > 0), not a bare `rsp_cd=00000`. A success response with an empty
  memo array → lands PENDING, never flips. This prevents a technically-successful but
  data-empty response from silently clearing the flip gate.

## Success Criteria

The wave's flip count splits into two sets, judged separately so a low count doesn't
silently read as success:

- **Expected non-flips (7):** the 2 `paper_incompatible` CCENQ TRs, the 4 o31xx
  `pending:feed-unprovisioned`, and `t1964` `pending:input-unresolved`. Landing these
  as faithfully-recorded PENDING/paper_incompatible **is** the correct outcome.
- **Contingent flips (10):** the KRX-night trio (`t8455`/`t8460`/`t8463`), `t2106`,
  and the 6 overseas-stock reads. These flip only if their window is open and inputs
  are valid at smoke time.

The wave succeeds when the KRX-night trio **and** ≥4 of the 6 overseas-stock reads
flip while their windows are open, with every non-flip recorded under a correct R6/R7
cause. Fewer contingent flips than that triggers a **re-window retry**, not a
closeout — booking the shortfall as PENDING is only honest when the window was
actually open and the input actually valid.

- `CCENQ10100` / `CCENQ90200` — already terminal `paper_incompatible` (gateway
  `01900`, ledger §12). Authored for callability (R1 structs + smoke harness), but
  no Implemented flip is possible in paper; they remain Tracked `paper_incompatible`.

**Deferred for later**

- `t1852` / `t1856` — require an unsourced `sFileData` screening blob (~26.8 KB);
  stay Tracked PENDING until the blob is sourced.
- `t3102` (news body) and `t8430` (stock list) — unblocked standalone reads, simply
  not in this batch; eligible for a later wave.
- Cracking the `o31xx` paper feed or sourcing the `t1964` ELW-board filter enums —
  the old source confirms neither is solvable from existing material.

**Out of scope**

- `t1860` — HELD: a side-effectful realtime-subscription control, not a read-only
  query; not part of any read wave.
- `CSPAT00601` — cash-equity order submission; deferred to the order-safety package.

## Dependencies / Assumptions

- **Session windows must be open at smoke time.** KRX-night TRs need the
  `krx_extended` window (~18:00–05:00 KST); overseas-stock reads need US regular
  session. Both open now (22:29 KST, 2026-06-24). This assumption gates Implemented
  yield, not correctness.
- **Smoke inputs** (from `.agents/skills/promote-tr/references/smoke-map.md`):

  | TR | Owner / route | Key smoke input |
  |---|---|---|
  | CCENQ10100 | account | paper account + current `LS_LIVE_SMOKE_FNOISU`; **terminal `paper_incompatible` (01900)** — authored, no flip |
  | CCENQ90200 | account | paper account; **terminal `paper_incompatible` (01900)** — authored, no flip |
  | t2106 | market_session | self-sources contract `code` from `t8467`; anytime F/O |
  | t8455 | market_session | `gubun=NF`; night window |
  | t8460 | market_session | `yyyymm`=near month, `gubun=G`; night window |
  | t8463 | market_session | `tm_rng=N`/`fot_clsf_cd=F`/`bsc_asts_id=101`, `cnt` numeric; night window |
  | g3101 | market_session | `82`/`TSLA`; canonical 현재가 `price` |
  | g3102 | market_session | `82`/`TSLA`, `readcnt`/`cts_seq` numeric; Object-Array detail |
  | g3103 | market_session | `82`/`TSLA`, monthly `gubun=4`; Object-Array bars |
  | g3104 | market_session | `82`/`TSLA`; canonical 한글종목명 `korname` |
  | g3106 | market_session | `82`/`TSLA`; canonical 현재가 `price` |
  | g3190 | market_session | US exchange `2`, `readcnt` numeric; `korname`; Object-Array rows |
  | o3105 | market_session | symbol `CUSN23` ⚠ stale (2023-expiry); canonical 체결가격 `trd_p` |
  | o3106 | market_session | symbol `ADM23` ⚠ stale; canonical 현재가 `price` |
  | o3125 | market_session | `mktgb=F`/`HSIM23` ⚠ stale; canonical 체결가격 `trd_p` |
  | o3126 | market_session | `mktgb=F`/`ADM23` ⚠ stale; canonical 현재가 `price` |
  | t1964 | market_session (KTD3 placeholder correction) | self-sources `item` from `t9905`; broad filter defaults |

- **o31xx smoke symbols are stale and must be refreshed before the wave.** The
  registered `CUSN23`/`ADM23`/`HSIM23` codes are 2023-expiry contracts; an expired
  symbol returns success + empty out-block, which is indistinguishable from an
  unprovisioned feed. Resolve current front-month symbols (from an overseas-futures
  master) first — only then does an empty o31xx out-block actually justify the
  `pending:feed-unprovisioned` disposition rather than masking a stale-input error.
- **Old-SDK port source.** `korea-broker-sdk-ls` generated structs corroborate the
  **request** shapes; `o31xx`/`t1964` are Transport-only there (no data frame ever
  decoded), so the request serialization is clean but the response structs rest on
  the normalized baseline, with no expectation of non-empty data.
- **Discovery edges.** `t1964.item ← t9905OutBlock1.shcode` and `t2106.code ← t8467`
  are chained in their smokes; retained unconfirmed for `t1964` until a non-empty
  board call.

## Outstanding Questions

**Resolve before planning**

- None — scope and disposition policy are settled.

**Deferred to planning**

- Per-TR module placement and struct layout (which existing `market_session` /
  `account` file each TR joins, or whether any needs its own module) — read from the
  baseline and existing sibling patterns at plan time.
- Whether `t2106` ever returns a populated price-memo in paper (the empty-memo
  disposition is settled as PENDING per AE5; this is a confirm-against-live check on
  whether a clean flip is reachable at all this wave).

## Sources / Research

- `metadata/trs/<tr>.yaml`, `crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json` — per-TR metadata and wire-shape source of truth for all 17.
- `.agents/skills/implement-tr/SKILL.md` — the frozen Tracked → Implemented recipe (dual cross-check registration, Paper Live Smoke gate).
- `.agents/skills/promote-tr/references/smoke-map.md` — smoke targets and registered inputs for the 17.
- `metadata/PROVISIONALITY-LEDGER.md` — `t1860` HELD reason, `t1852`/`t1856` sFileData block, `t1964` filter-default block, discovery edges.
- `korea-broker-sdk-ls` — `crates/core/src/generated/{overseas_futures,stock}.rs` (ported wire shapes), `RELEASE_CERTIFICATION_STATUS.md` + `CONCEPTS.md` (Transport-only certification for `o31xx`/`t1964`), `scripts/certification_fixture_overrides.json` (`t1964` default-`0` filter values).
- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md` — numeric request-body serialization gotcha.
