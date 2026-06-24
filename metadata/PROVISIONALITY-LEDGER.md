# Provisionality Ledger — bulk tracked-only TR expansion (36 read-only stock TRs)

This is a committed `metadata/`-level sidecar (mirroring `metadata/EVIDENCE-FRESHNESS.md`)
that records, per TR, which authored facets are **provisional** for the 36 read-only
stock TRs brought into tracked-only maintenance ownership in this batch.

It exists so a later `tracked → implemented` promotion knows exactly **what to
re-verify** before a TR gains callable behavior, an SDK Reference page, a
recommendation claim, or Focused Evidence. None of the 36 is callable, recommended,
or evidence-backed today; the *hard-accurate* facets (R5: `support`, `owner_class`,
`protocol`, `instrument_domain`, `certification_path`, `paper_incompatible`,
`account_state`, `self_paginated`, and the order/dependency risk fields) are confirmed
against the committed raw snapshot and are **not** listed here. Only the *provisional*
facets (R6) are.

This file is **not** a per-TR schema field and **not** an entry in `tr-index.yaml`
(which is closed-set parsed and would reject an unknown key). No gate scans
`metadata/` for stray files, so it is accepted by `cargo test -p ls-metadata` and
`make docs-check` while present. The future `tracked → implemented` promotion recipe
consumes or retires these rows explicitly.

## How to use this ledger

When promoting a TR from `tracked` to `implemented`:

1. Find the TR's rows below.
2. Re-verify each provisional facet against live behavior / a clean fetch, per the
   **Re-verify before implementation** column.
3. Correct the per-TR metadata (and the `tr-index.yaml` routing entry where the facet
   is duplicated — `venue_session`) if the verified value differs.
4. Retire the TR's rows from this ledger as each facet is confirmed.

---

## 1. `venue_session` (authored for all 36; rows retire as TRs implement)

`venue_session` is authored best-effort as `krx_regular` for every TR and duplicated
into the routing index (validator cross-check). It is provisional for the whole batch:
the snapshot does not pin the trading session a read is scoped to. Four after-hours /
call-auction screens are the most likely to differ (`krx_extended`).

| TR | Provisional value | Source basis | Re-verify before implementation |
|---|---|---|---|
| t1481 | `krx_regular` | `시간외`/단일가 (after-hours / call-auction) screen — likely spans an extended session | confirm `krx_extended` vs `krx_regular` against live session behavior |
| t1482 | `krx_regular` | `시간외`/단일가 (after-hours / call-auction) screen — likely spans an extended session | confirm `krx_extended` vs `krx_regular` against live session behavior |
| t1489 | `krx_regular` | `시간외`/단일가 (after-hours / call-auction) screen — likely spans an extended session | confirm `krx_extended` vs `krx_regular` against live session behavior |
| t1492 | `krx_regular` | `시간외`/단일가 (after-hours / call-auction) screen — likely spans an extended session | confirm `krx_extended` vs `krx_regular` against live session behavior |
| t1852 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1856 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1860 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1964 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| ~~t1988~~ | ~~`krx_regular`~~ | **RETIRED (U3, 2026-06-24)**: implemented, non-empty success on a live KRX-regular paper smoke (`assets=71`) | — |
| t3102 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to (HELD — input-unresolved, see §13) |
| ~~t3320~~ | ~~`krx_regular`~~ | **RETIRED (U3, 2026-06-24)**: implemented, non-empty success on a live KRX-regular paper smoke (`summary=1`) | — |
| t8430 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |

## 2. `caller_supplied_identifiers`

Authored best-effort from request-shape input fields. For filter/`gubun`-style screens
the list is empty; where an instrument or record identifier is present in the request
it is recorded. The true required-input set is confirmed at implementation.

| TR | Provisional value | Source basis | Re-verify before implementation |
|---|---|---|---|
| t1481 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1482 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1852 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1856 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1860 | `[query_index]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t1964 | `[item, issuercd]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| ~~t1988~~ | ~~`[]`~~ | **RETIRED (U3, 2026-06-24)**: implemented; `mkt_gb`+filter-flags only, no instrument identifier, accepted live (`[]` confirmed) | — |
| t3102 | `[sNewsno]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| ~~t3320~~ | ~~`[gicode]`~~ | **RETIRED (U3, 2026-06-24)**: implemented; bare 6-digit `gicode=005930` accepted live (the `A`-prefixed FnGuide form returned a sparse body, found via raw-probe A/B, KTD9) | — |
| t8430 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |

## 3. Weak discovery-style relationships

Cross-TR discovery dependencies visible in the request shape but **not** modelled in
the per-TR `dependencies` block (which today covers only self-continuation and
order-coupling fields).

| TR | Relationship | Source basis | Re-verify before implementation |
|---|---|---|---|
| t1860 | `query_index` ← `t1866OutBlock1.query_index` | request field `query_index` is documented as sourced from `t1866`'s output — a cross-TR discovery dependency, not modelled in `dependencies` | model the `t1866 → t1860` discovery edge when either TR is implemented |
| t1964 | `item` ← `t9905OutBlock1.shcode` | t1964's `item` (기초자산코드) is the underlying-asset code `t9905` emits — modeled this wave (Wave 1). t1964 ships **PENDING** (broad/default filters returned an empty board for the first 10 underlyings; no named source for the 10 filter enums per KTD-1), so this edge is **retained, unconfirmed** | retire on a confirming non-empty `t1964` board call once defensible filter defaults are sourced |

## 4. Field-level `type` facets — re-pinned from clean `property_type` (2026-06-22) — RETIRED

Re-derived from a clean property-type fetch (`property_type_fallback_served == false`)
via an attested type-only Baseline Promotion (promotion record `attested_by:
sunkeunchoi:property_type-endpoint-fix-2026-06-22`, `raw_hash c652649aed4da411`, source
run `2026-06-22T02-37-27Z`). The post-promote self-diff is clean (`api-drift check`
exits `0`). Field `type` provisionality is **fully retired**: the Still-provisional
table below is empty.

**Root-cause correction (the "HTTP-500 outage" was a bug, not upstream).** The original
seed framing — that the LS `system-codes` endpoint suffered a chronic HTTP 500 — was a
**misdiagnosis**. `crates/ls-trackers/src/fetch.rs` called the wrong URL
(`/api/codes/public/system-codes?groupCode=property_type`, which 500s for everyone) and
parsed the wrong response shape; the live portal endpoint is
`GET /api/codes/public/property_type` (returns `{ "codes": [ { "key", "value" } ] }`).
The hardcoded fallback table was *also* wrong, so the seed's field types were genuinely
incorrect, not merely "provisional display names":

| code | wrong fallback (seed) | live value (re-pinned) |
|---|---|---|
| A0001 | String | String |
| A0002 | *(absent)* | Array |
| A0003 | Long | **Object** |
| A0004 | Decimal | **Number** |
| A0005 | Binary | **Object Array** |

The 2026-06-22 fix corrected the URL, the parser, and the fallback values; the re-pin
drift wave was a pure field-`type` change (`Decimal→Number`, `Binary→Object Array`,
`Long→Object`), gated by the opt-in type-only gate.

**Retired** — type resolved by the live `property_type` mapping:

| TR / facet | Resolved type source |
|---|---|
| All maintained shapes (field-level `type`) | live `GET /api/codes/public/property_type` mapping, clean fetch 2026-06-22 |

**Still-provisional** — none. Every `property_type` code in the committed raw inventory
(`A0001`–`A0005`) is defined by the live mapping, and the committed normalized baseline
contains zero raw-coded (`A00xx`) types.

---

## 5. Consumer-bound Implemented Expansion wave — close-out (2026-06-21)

The `tracked → implemented` wave (plan
`docs/plans/2026-06-21-003-feat-consumer-bound-implemented-expansion-plan.md`)
promoted 11 consumer-bound read-only stock TRs to **Implemented** (callable Rust,
gated by a Paper Live Smoke; each stays **non-recommended** — no Focused Evidence,
no recommendation block, no `EVIDENCE-FRESHNESS.md` edit). Every one of the 11
reached a decided end state: **all 11 implemented**, none dropped or pended.

| TR | Class | End state | Smoke gate (credential-free) |
|---|---|---|---|
| t8425 | market_session | implemented | `rsp_cd=00000 themes=265` |
| t8436 | market_session | implemented | `rsp_cd=00000 stocks=4290` |
| t1531 | market_session | implemented | `rsp_cd=00000 rows=1` (theme tmcode=0008) |
| t1537 | market_session | implemented | `rsp_cd=00000 rows=10` (theme tmcode=0008) |
| t1452 | paginated (single-page) | implemented | `rsp_cd=00000 rows=40` |
| t1403 | paginated (single-page) | implemented | `rsp_cd=00000 rows=20` |
| t1441 | paginated (single-page) | implemented | `rsp_cd=00000 rows=50` |
| t1463 | paginated (single-page) | implemented | `rsp_cd=00000 rows=50` |
| t1466 | paginated (single-page) | implemented | `rsp_cd=00000 rows=50` |
| t1489 | paginated (single-page) | implemented | `rsp_cd=00000 rows=20` |
| t1492 | paginated (single-page) | implemented | `rsp_cd=00000 rows=21` |

Classification key (none used this wave): TR-defect (raw HTTP ok, SDK deserialize
fails → dropped), environmental-pending (failure reproduces outside the TR; no
in-window recovery → pending), input-unresolved (no representative caller input).

**Residual provisionality (NOT retired by this wave):**
- `t1489` / `t1492` `venue_session`: still provisional (kept in §1). Both are
  call-auction / expected-execution screens flagged possibly `krx_extended`; the
  smokes ran off-session (a Sunday, returning last-session data), which confirms
  callability but **cannot** resolve `krx_regular` vs `krx_extended`. Re-verify
  against live in-session behavior before any Recommended promotion.
- Field-level `type` facets (§4): **now retired** (2026-06-22). The "HTTP-500 seed"
  was a wrong-endpoint bug in fetch, not an upstream outage; once corrected, the
  clean `property_type` fetch re-pinned every facet. See §4.
- Multi-page collection over body-`idx` for the 7 paginated TRs: deferred
  follow-up (these are Implemented at single-page scope only).

Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at six Recommended TRs;
no `metadata/evidence/<tr>.yaml` exists for any of the 11.

---

## 6. Saved-Condition Screening wave — close-out (2026-06-22)

The `tracked → implemented` saved-condition screening wave (plan
`docs/plans/2026-06-22-001-feat-saved-condition-screening-expansion-plan.md`)
ships as a **partial wave**: it completes the real server-saved-condition
`query_index` spine (`t1866 → t1859`) and reaches a decided end state for all 7
member TRs. Each implemented TR stays **non-recommended** (no Focused Evidence,
no recommendation block, no `EVIDENCE-FRESHNESS.md` edit). The two core file-saved
screens and the session pair could not clear their preconditions in-window and
ship pending; t1860 reclassified out of scope. Every one of the 7 is decided:
**2 implemented, 1 held, 4 pending.**

| TR | Class (first-pass) | End state | Disposition (credential-free) |
|---|---|---|---|
| t1866 | paginated (single-page) | **implemented** | `rsp_cd=00000 conditions=1` (spine producer) |
| t1859 | market_session | **implemented** | `rsp_cd=00000 rows=934` (chained off t1866; `query_index` accepted) |
| t1860 | market_session | **HELD — out of scope (realtime registration)** | not smoked (see below) |
| t1852 | market_session | **PENDING — input-unresolved** | required `sFileData` blob (~26.8 KB) unsourced |
| t1856 | market_session | **PENDING — input-unresolved** | required `sFileData` blob (~26.8 KB) unsourced |
| t1481 | paginated (body-`idx`) | **implemented (U2 reach wave)** | `rsp_cd=00000 rows=20`; `caller_supplied_identifiers: []` confirmed accepted; `venue_session` retained (regular-vs-extended unresolved by a single regular-session run, KTD7) |
| t1482 | paginated (body-`idx`) | **implemented (U2 reach wave)** | `rsp_cd=00000 rows=20`; `caller_supplied_identifiers: []` confirmed accepted; `venue_session` retained (regular-vs-extended unresolved by a single regular-session run, KTD7) |

**Spine proven end-to-end.** A live `t1866` list supplies a `query_index` that
`t1859`'s chained smoke accepts (a non-empty success), so the `t1866 → t1859`
discovery edge (§3) is retired and `t1859`'s `venue_session` (§1, `krx_regular`)
and `caller_supplied_identifiers` (§2, `[query_index]`) retire. `t1866`'s
`venue_session` + caller-input rows retired in U3.

**t1860 — HELD, not implemented (recorded reason).** The raw spec
(`crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`) resolves
t1860's fields as a **side-effectful realtime-subscription control**, not a
read: `sFlag` is `'E'`=register / `'D'`=stop, `sSysUserFlag` is `'U'` fixed, and
an `'E'` register **allocates a server-side realtime alert slot** whose returned
`sAlertNum` is the `gsRealKey` input to the separate **AFR (사용자조건검색실시간)
realtime TR** — i.e. registering opens a realtime push channel that must later be
torn down with a matching `'D'` + `sAlertNum` call. This is the recipe's §0
"realtime/WebSocket → HELD out of scope" precondition: t1860 is not a read-only
REST read, and a paper smoke would leave a dangling realtime registration (or
require a custom register/deregister lifecycle outside this read-only wave).
A future realtime/subscription wave that models the AFR channel should pick it up.

**Residual provisionality (NOT retired by this wave).** The pending/held TRs stay
tracked-only with their `§1`/`§2`/`§3` rows **retained** (none confirmed by a paper
call), so no ledger row is left silently live (R11):
- **t1860** — `venue_session` (§1), `caller_supplied_identifiers` (§2, `[query_index]`),
  and the §3 `t1866 → t1860` discovery edge: all retained, unconfirmed (held).
- **t1852 / t1856** — `venue_session` (§1) and `caller_supplied_identifiers` (§2)
  retained. Note their §2 rows still read `[]`; the baseline marks a required
  `sFileData` String, so the true caller-input set is `[sFileData]` — left
  uncorrected here because the field is unconfirmed in-window (the sourcing wave
  reconciles it on a confirming call). owner_class stays the `standalone`
  placeholder (not reclassified absent a live confirmation).
- **t1481 / t1482** — both **implemented** in the U2 reach wave on green paper
  smokes (`rsp_cd=00000 rows=20` each); their `caller_supplied_identifiers` (§2,
  `[]`) is confirmed accepted (each call sent only filter flags + the body `idx`,
  no instrument identifier, and succeeded). Their `venue_session` (§1) is **NOT
  retired** and stays explicitly **session-unresolved**: no SDK/core field carries
  session phase, and a single regular-session smoke cannot resolve `krx_regular`
  vs `krx_extended` (the `t1489`/`t1492` precedent in §5). Resolving it needs an
  in-session vs after-hours live-run window diff — deferred to the
  session-semantics follow-up below.

**Follow-up roadmap (opened as issues).**
1. **sFileData sourcing wave** — source a representative ~26.8 KB `sFileData`
   screening-condition blob, then implement `t1852`/`t1856` and reconcile their
   §2 caller-input rows to `[sFileData]`.
2. **Session-semantics wave** — `t1481`/`t1482` are now implemented (U2 reach wave,
   single-page body-`idx` scope); the residual task is to run an in-session vs
   after-hours window diff to resolve their `venue_session` (§1, `krx_regular` vs
   `krx_extended`) and retire that facet — needed before any Recommended promotion.
3. **Realtime lifecycle / AFR design** — model the `t1860` register/deregister
   lifecycle and the AFR (사용자조건검색실시간) realtime channel if that capability
   is pursued.

Field-`type` facets (§4) are already retired inventory-wide (clean re-pin); nothing
to retire here. Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at six
Recommended TRs; no `metadata/evidence/<tr>.yaml` exists for any of the 7.

---

## 7. ThinQ Q-click search wave — close-out (2026-06-23)

The `tracked → implemented` ThinQ Q-click search wave (plan
`docs/plans/2026-06-23-001-feat-capability-closed-tr-expansion-waves-plan.md`,
Wave 3 / PR #1) ships **complete**: both member TRs flip on a chained paper
smoke that proves the `t1826 → t1825` producer→consumer spine end-to-end. Each
implemented TR stays **non-recommended** (no Focused Evidence, no recommendation
block, no `EVIDENCE-FRESHNESS.md` edit). Both of the 2 are decided:
**2 implemented, 0 pending.**

| TR | Class (first-pass) | End state | Disposition (credential-free) |
|---|---|---|---|
| t1826 | market_session | **implemented** | `rsp_cd=<success> searches=23` (spine producer; `search_gb=0` 핵심검색) |
| t1825 | market_session | **implemented** | `rsp_cd=<success> rows=220` (chained off t1826; `search_cd` accepted) |

**Genuine producer→consumer edge (not a capability surface).** Unlike the ELW
(Wave 1) and analytics (Wave 2) surfaces — which clear the consumer-less hold by
being bounded market-data capabilities, not by an internal consumer edge — Wave 3
carries a **real** producer→consumer dependency: a live `t1826` list supplies the
`search_cd` that `t1825`'s chained smoke consumes (a non-empty success). This is
why Wave 3 shipped first (KTD-3): it validates the chained-smoke harness pattern
the later waves reuse.

**Spine proven end-to-end.** The chained smoke self-sources a `search_cd` from a
live `t1826` call and feeds it to `t1825` (never fabricated, never recorded — the
`search_cd` is treated as a server-assigned catalog key like the saved-condition
`query_index`). On the confirming non-empty success:
- the `search_cd ← t1826OutBlock.search_cd` discovery edge (§3) was **modeled then
  retired** — it is not left as a live §3 row (mirroring the `t1866 → t1859`
  treatment in §6);
- `t1825`'s `caller_supplied_identifiers` (§2, `[search_cd]` → `[]`) corrects in
  metadata and its §2 row retires — no metadata/ledger contradiction remains;
- both members' `venue_session` (§1, `krx_regular`) rows retire.

`t1826`'s `venue_session` + caller-input (`[]`) rows retired in U2 (the producer's
implement unit); `t1825`'s rows retired in U3 (the consumer's flip).

**venue_session disposition (R12).** Both members' §1 rows retired as
`krx_regular`: each is a ThinQ catalog/search read that returned a non-empty
success during a live paper call, and neither carries an after-hours / call-auction
facet (no `krx_extended` candidate). No member ships with a §1 row left silently
live.

**Residual provisionality.** None for this wave — both members are implemented and
their §1/§2/§3 rows are retired. No pending/held members, so no rows are retained.

**Standing cost (accepted, per Risk Analysis).** This wave adds 2 consumer-less
live-smoke targets + 2 drift-detection structs that must stay green. Disposition
rule: a consumer-less smoke is allowed to go **pending (not red)** off-session, and
a drift failure on a consumer-less Implemented TR is **triage-P3**, not a release
blocker — so the first off-session red or upstream drift is budgeted, not a
surprise.

Field-`type` facets (§4) are already retired inventory-wide (clean re-pin); nothing
to retire here. Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at six
Recommended TRs; no `metadata/evidence/<tr>.yaml` exists for either member.

---

## 8. ELW universe & instrument surface wave — close-out (2026-06-23)

The `tracked → implemented` ELW universe & instrument-surface wave (plan
`docs/plans/2026-06-23-001-feat-capability-closed-tr-expansion-waves-plan.md`,
Wave 1 / PR #2) ships as a **partial wave**: it reaches a decided end state for
all 7 member TRs and proves the ELW capability through its defining member. Each
implemented TR stays **non-recommended** (no Focused Evidence, no recommendation
block, no `EVIDENCE-FRESHNESS.md` edit). Every one of the 7 is decided:
**5 implemented, 2 pending.**

| TR | Class (first-pass) | End state | Disposition (credential-free) |
|---|---|---|---|
| t9905 | market_session | **implemented** | `rsp_cd=00000 underlyings=74` (full underlying list; `shcode` keys t1964) |
| t9907 | market_session | **implemented** | `rsp_cd=00000 months=11` (ELW expiry months) |
| t8431 | market_session | **implemented** | `rsp_cd=00000 elws=2919` (ELW symbol list; spine producer for t1958) |
| t9942 | market_session | **implemented** | `rsp_cd=00000 elws=2919` (ELW master list) |
| t1958 | market_session | **implemented** | `rsp_cd=00000 compared=2` (chained off t8431; two public shcodes; capability-defining) |
| t1964 | market_session | **PENDING — input-unresolved (filter defaults)** | callable; broad `"0"` filter defaults returned an empty board for the first 10 underlyings (no named source for the 10 filter enums, KTD-1) |
| t1988 | market_session | **implemented (U3 reach wave, 2026-06-24)** | the prior `IGW40011` was a wire-type defect, not environmental: `from_rate`/`to_rate` (the two Number-typed request fields) were quoted strings. Serializing them as JSON numbers (`string_as_number`, KTD4) cleared it — `rsp_cd=00000 assets=71`. See §13. |

**Capability surface, not a consumer edge (KTD-2).** This wave clears the
consumer-less hold for these members by being a **bounded ELW universe &
instrument-lookup surface with strict membership and live paper smokes** — *not*
by an internal producer→consumer edge. That is a deliberately different bar from
the predecessor's saved-condition screening-workflow consumer test. The one
internal edge present (t8431 → t1958, and the modeled t9905 → t1964) is a
discovery-sourcing convenience for the smoke harness, not a claim that the surface
has a downstream consumer.

**Capability proven (KTD-4).** The ≥1 required flip is a capability-**defining**
member: `t1958` (ELW comparison) flips on a chained non-empty success, so the
headline "ELW universe & instrument surface" claim holds (it is not carried by a
trivially-non-empty list read). The four universe/list reads (t9905/t9907/t8431/
t9942) are the supporting surface.

**Discovery edges.** `t1958`'s `shcode1/shcode2 ← t8431OutBlock.shcode` edge was
modeled-then-retired on the confirming chained smoke (its §1/§2/§3 rows retire;
`caller_supplied_identifiers` `[shcode1, shcode2] → []`). `t1964`'s
`item ← t9905OutBlock1.shcode` edge is **modeled and retained** (§3) because
t1964 ships pending — no silent retirement.

**venue_session disposition (R12).** The five implemented members' §1 rows retire
as `krx_regular` (each returned a non-empty success on a live paper call; none
carries an after-hours / call-auction facet). The two pending members keep their
§1 rows retained, unconfirmed.

**Residual provisionality (NOT retired by this wave).** The pending TRs stay
tracked-only with their rows **retained**:
- **t1964** — `venue_session` (§1), `caller_supplied_identifiers` (§2,
  `[item, issuercd]`), and the new §3 `t9905 → t1964` discovery edge: all retained,
  unconfirmed. owner_class stays the `standalone` placeholder. Resolving it needs
  a named source for the 10 board filter enums (or an in-session window where the
  board is non-empty under broad defaults).
- **t1988** — RESOLVED in the U3 reach wave (2026-06-24): the `IGW40011` was the
  `from_rate`/`to_rate` wire-type defect (KTD4), not environmental. Now
  **implemented** through `market_session`; `venue_session` (§1) and
  `caller_supplied_identifiers` (§2, `[]`) retired on the non-empty smoke. See §13.

**Follow-up roadmap.**
1. **t1964 filter-default sourcing** — source the 10 ELW-board filter enums from a
   vendor spec or an observed HTS payload, then chain t1964 off t9905 and flip.
2. ~~**t1988 gateway-form resolution**~~ — DONE (U3 reach wave, 2026-06-24): the
   `IGW40011` was the `from_rate`/`to_rate` wire-type defect, cleared by
   `string_as_number`. t1988 is now implemented (§13).

**Standing cost (accepted, per Risk Analysis).** This wave adds 5 consumer-less
live-smoke targets + 5 drift-detection structs that must stay green. Disposition
rule: a consumer-less smoke may go **pending (not red)** off-session, and a drift
failure on a consumer-less Implemented TR is **triage-P3**, not a release blocker.

Field-`type` facets (§4) are already retired inventory-wide (clean re-pin); nothing
to retire here. Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at six
Recommended TRs; no `metadata/evidence/<tr>.yaml` exists for any of the 7.

---

## 9. Market-flow analytics surface wave — close-out (2026-06-23)

The `tracked → implemented` market-flow analytics-surface wave (plan
`docs/plans/2026-06-23-001-feat-capability-closed-tr-expansion-waves-plan.md`,
Wave 2 / PR #3) ships **complete**: all 6 members flip on a non-empty paper
smoke. Each implemented TR stays **non-recommended** (no Focused Evidence, no
recommendation block, no `EVIDENCE-FRESHNESS.md` edit). All 6 are decided:
**6 implemented, 0 pending.**

| TR | Class (first-pass) | End state | Disposition (credential-free) |
|---|---|---|---|
| t1601 | market_session | **implemented** | `rsp_cd=00000 aggregate=populated` (investor-by-type) |
| t1615 | market_session | **implemented** | `rsp_cd=00000 markets=5` (investor trading aggregate) |
| t1640 | market_session | **implemented** | `rsp_cd=00000 aggregate=populated` (program-trading aggregate) |
| t1662 | market_session | **implemented** | `rsp_cd=00000 rows=145` (by-time program-trading chart) |
| t1664 | market_session | **implemented** | `rsp_cd=00000 rows=20` (investor trading chart) |
| t3341 | paginated (single-page) | **implemented** | `rsp_cd=00000 ranks=100` (financial ranking; body `idx`=0 number) |

**Capability surface, not a consumer edge (KTD-2).** This wave clears the
consumer-less hold by being a **bounded investor-flow / program-trading analytics
surface with strict membership and live paper smokes** — *not* by an internal
producer→consumer edge. There are no discovery edges in this wave; every member is
a standalone gubun-filter read with documented default inputs.

**Dropped exclusion prong (deliberate).** The predecessor's hold had a *second*
prong beyond the consumer-edge test: it excluded `t3341` and the analytics
aggregates for **emitting analytics**. This campaign drops that prong on purpose.
That exclusion was a *screening-workflow-consumption* test; membership here is
defined by **capability-surface coherence**, not workflow-consumption. The accepted
trade is the standing maintenance cost of a coherent read-only analytics surface
(below) — every member is a coherent part of the one named analytics surface with
a passing live smoke.

**Capability proven (KTD-4).** The capability-defining members are the investor-flow
/ program-trading aggregates (`t1601`/`t1615`/`t1640`/`t1662`), all of which flipped
— the headline "investor-flow / program-trading analytics surface" claim holds.

**Input-shape notes (KTD-5 + numeric request fields).** `t3341`'s body `idx` is an
ordinary in-block field serialized as a JSON **number** at the first-page convention
(`0`), never `#[serde(skip)]`; its `has_pagination` mirrors `facets.self_paginated`
(both true). Two members needed a numeric (not string) request field, found via the
raw-HTTP probe: `t1664.cnt` and `t3341.idx` both serialize via `string_as_number`.

**venue_session disposition (R12).** All six members' §1 rows retire as
`krx_regular` (each returned a non-empty success on a live paper call; none carries
an after-hours / call-auction facet). No member ships with a row left silently live.

**Residual provisionality.** None for this wave — all six are implemented and their
§1/§2 rows are retired. No pending/held members.

**Standing cost (accepted, per Risk Analysis).** This wave adds 6 consumer-less
live-smoke targets + 6 drift-detection structs that must stay green — the symmetric
cost of the analytics drift-readiness benefit. Disposition rule: a consumer-less
smoke may go **pending (not red)** off-session, and a drift failure on a
consumer-less Implemented TR is **triage-P3**, not a release blocker.

Field-`type` facets (§4) are already retired inventory-wide (clean re-pin); nothing
to retire here. Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at six
Recommended TRs; no `metadata/evidence/<tr>.yaml` exists for any of the 6.

## 10. Sector cluster raw→Implemented wave (Wave A) — close-out (2026-06-23)

The first **raw → Tracked → Implemented** wave (plan
`docs/plans/2026-06-23-002-feat-sector-cluster-raw-to-implemented-plan.md`). The
five `[업종] 시세` TRs began with no metadata and no normalized baseline — present
only in the raw OpenAPI capture. This wave built the **Tracked rung** in-wave
(authored `metadata/trs/*.yaml` + `tr-index.yaml`, projected the baselines via
`make api-drift-renormalize`; `maintained_tr_count` 44→49), froze the loop as the
reusable `.agents/skills/track-tr` recipe (R3), then authored callable Rust gated
on a Paper Live Smoke. All five flip on a non-empty in-window paper smoke. Each
stays **non-recommended** (no Focused Evidence, no recommendation block, no
`EVIDENCE-FRESHNESS.md` edit). All five decided: **5 implemented, 0 pending.**

| TR | Class (first-pass) | End state | Disposition (credential-free) |
|---|---|---|---|
| t8424 | market_session | **implemented** | `rsp_cd=00000 sectors=252` (전체업종; anchor + `upcode` source) |
| t1511 | market_session | **implemented** | `rsp_cd=00000 snapshot=populated` (업종현재가; `upcode=001`) |
| t1485 | market_session | **implemented** | `rsp_cd=00000 rows=61` (예상지수; `upcode=001`, `gubun=1`) |
| t1516 | market_session | **implemented** | `rsp_cd=00000 stocks=40` (업종별종목시세; `upcode=001` + `shcode=005930`) |
| t1514 | paginated (single-page) | **implemented** | `rsp_cd=00000 rows=1` (업종기간별추이; `cts_date` cursor, `cnt` number) |

---

## 11. Wave 0 read-only TR raw→Tracked bulk expansion (21 TRs) — provisional facets (2026-06-23)

The first stage of a staged read-only expansion (plan
`docs/plans/2026-06-23-003-feat-wave0-readonly-tr-tracking-plan.md`). 21 TRs across
account, futures/options, overseas-futures, and overseas-stock were brought from
raw → **Tracked** (metadata + `tr-index.yaml` + projected baselines via
`make api-drift-renormalize`; `maintained_tr_count` 49→70). No callable Rust, no
Implemented flips. The hard-accurate facets (`support`, `owner_class`, `protocol`,
`instrument_domain`, `account_state`, `self_paginated`, `paper_incompatible`,
`certification_path`) are confirmed against the committed raw snapshot and are not
listed; only the provisional facets are.

### 11.1 `venue_session` (authored best-effort; rows retire as TRs implement)

The raw snapshot does not pin the trading session a read is scoped to. The four
account reads are session-agnostic; the night-derivatives reads
(`CCENQ90200`/`t8455`/`t8460`/`t8463`) are authored `krx_extended` from their
`KRX야간` name only — **unconfirmed**; the overseas reads carry `unspecified` because
the LS overseas gateway/session shape is uncharted in the repo.

| TR | Provisional value | Source basis | Re-verify before implementation |
|---|---|---|---|
| ~~CSPAQ12300~~ | ~~`unspecified`~~ | ~~account-state read, session-agnostic~~ | **RETIRED (PR-A U1)** — paper BEP read returned `rsp_cd=00136` non-empty regardless of session, confirming session-independence |
| ~~CSPAQ22200~~ | ~~`unspecified`~~ | ~~account-state read, session-agnostic~~ | **RETIRED (PR-A U2)** — paper orderable/valuation read returned `rsp_cd=00136` non-empty regardless of session, confirming session-independence |
| ~~CFOBQ10500~~ | ~~`unspecified`~~ | ~~account-state read, session-agnostic~~ | **RETIRED (PR-A U3)** — paper F/O deposit read returned `rsp_cd=00136` non-empty regardless of session, confirming session-independence |
| CCENQ90200 | `krx_extended` | `KRX야간파생` night-derivatives balance — session from name only, not snapshot-pinned | confirm `krx_extended` vs `unspecified` against live night-session behavior |
| t2301 | `krx_regular` | F/O board/master read, KRX regular assumed | confirm the session the read is scoped to |
| t2522 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t8401 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t8426 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t8433 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t8435 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t8467 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t9943 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t9944 | `krx_regular` | F/O master read, KRX regular assumed | confirm the session the read is scoped to |
| t8455 | `krx_extended` | `KRX야간파생` master — session from name only | confirm `krx_extended` against live night-session behavior |
| t8460 | `krx_extended` | `KRX야간파생` option board — session from name only | confirm `krx_extended` against live night-session behavior |
| t8463 | `krx_extended` | `KRX야간파생` investor-by-time — session from name only | confirm `krx_extended` against live night-session behavior |
| o3101 | `unspecified` | overseas-futures read; LS overseas gateway/session uncharted | confirm the overseas session model against live behavior |
| o3121 | `unspecified` | overseas-futures read; LS overseas gateway/session uncharted | confirm the overseas session model against live behavior |
| g3101 | `unspecified` | overseas-stock read; LS overseas gateway/session uncharted | confirm the overseas session model against live behavior |
| g3104 | `unspecified` | overseas-stock read; LS overseas gateway/session uncharted | confirm the overseas session model against live behavior |
| g3106 | `unspecified` | overseas-stock read; LS overseas gateway/session uncharted | confirm the overseas session model against live behavior |

### 11.2 `caller_supplied_identifiers` (authored best-effort from request shape)

Filter/`gubun`/`dummy`-style master and board reads carry `[]`. Where the request
carries an instrument/underlying/market code it is recorded. The overseas
identifiers are **uncharted** — the gateway has not been probed, so the true
required-input set (and identifier wire names) is unconfirmed.

| TR | Provisional value | Source basis | Re-verify before implementation |
|---|---|---|---|
| t8463 | `[bsc_asts_id]` | `기초자산코드` underlying-asset code in the request | confirm the required caller-input set against a live request |
| o3101 | `[]` | overseas-futures master read, no obvious identifier (reserve `gubun` only); gateway uncharted | confirm the overseas request shape against a live probe |
| o3121 | `[BscGdsCd]` | overseas option underlying-product code (optional; blank lists all); gateway uncharted | confirm the overseas request shape + identifier names against a live probe |
| g3101 | `[keysymbol, exchcd, symbol]` | overseas-stock symbol + exchange code; gateway uncharted | confirm the overseas request shape + identifier names against a live probe |
| g3104 | `[keysymbol, exchcd, symbol]` | overseas-stock symbol + exchange code; gateway uncharted | confirm the overseas request shape + identifier names against a live probe |
| g3106 | `[keysymbol, exchcd, symbol]` | overseas-stock symbol + exchange code; gateway uncharted | confirm the overseas request shape + identifier names against a live probe |

The other 15 TRs authored `caller_supplied_identifiers: []` best-effort (master/board
reads with only `dummy`/`gubun`/month/mode inputs); confirm no caller-supplied
identifier is required when each implements.

**Anchor guarantee (R12).** The ship-floor — ≥1 member flips via an *in-window*
smoke — is met by all five (KRX regular session, 14:22 KST 2026-06-23). `t8424`
is the intended anchor and flipped non-empty (252 sectors); the guarantee did not
rest on an unverified off-hours result.

**`upcode` resolved to the numeric-string `"001"` (not alpha).** The raw
`req_example` value `upcode:"001"` (코스피종합) is accepted live by every consumer
(U1 probe + smokes); it **supersedes** the origin's alpha-form hedge
(`BMT`/`BM_`/`IJ_`), which came only from the migration-source WEAK heuristic
(`producing_tr: null`). The consumers smoke **standalone** with `"001"`; the
`t8424 → consumers` producer edge is optional convenience, not modeled (deferred
follow-up). `upcode`/`shcode`/`cts_date` stay **string-serialized** — applying
`string_as_number` to them would be the inverse `IGW40011` trap.

**Input-shape notes (numeric request fields).** The only genuinely-numeric request
field in the cluster is **`t1514.cnt`**, serialized as a JSON **number** via
`string_as_number`. The U1 raw-probe A/B confirmed it empirically: `cnt` as a
number → `rsp_cd=00000`; `cnt` as a string → **`http=500 IGW40011`**. `t1514`'s
`has_pagination` mirrors `facets.self_paginated` (both true); its `cts_date` body
cursor rides the in-block (header cursors `#[serde(skip)]`).

**`venue_session` disposition (R12).** This is a net-new cluster — its members
were never in §1's original 36, so there are no pre-existing rows to retire.
Their `venue_session: krx_regular` was **authored in U2 and confirmed live in the
same wave**: all five returned a non-empty success on an in-window paper call, so
none ships with a session facet left unverified. The only premise unconfirmable by
this session is `t8424`'s *off-hours* non-emptiness (we ran in-window) — recorded
as deferred and non-blocking, since the ship-floor is an in-window flip.

**Weak `upcode`/`shcode` edges (§3-style).** `upcode` (업종코드, `producing_tr:
null`, WEAK) and `t1516`'s second input `shcode` (종목코드, `producing_tr: null`)
were both resolved by a confirmed-accepted literal (`"001"` / `"005930"`), not a
modeled producer→consumer edge. No weak-edge row is left live: each is dispositioned
by a passing smoke.

**Residual provisionality.** None for this wave — all five are implemented; no
pending/held members. Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at
six Recommended TRs; no `metadata/evidence/<tr>.yaml` exists for any of the five.

---

## 12. Reach wave U4 — Account/F&O lane (CCENQ90200, CFOAQ10100, CCENQ10100) (2026-06-24)

Three account-gated read-only inquiries routed through `account` (mirroring
`CSPAQ12200`'s account-identity discipline — the account number comes from
config, never a caller field; verified absent from each serialized in-block).
**1 implemented, 2 Tracked/paper-incompatible.**

| TR | End state | Disposition (credential-free) |
|---|---|---|
| CFOAQ10100 | **implemented** | `rsp_cd=00136 qtyrows=1` (선물옵션 주문가능수량조회; `FnoIsuNo=A0169000` KOSPI200 Sep-2026 index future, accepted live; canonical out-block field `OrdAbleQty`/주문가능수량, single object → 1-element Vec). A read-only inquiry (조회), not an order. |
| CCENQ90200 | Tracked, **paper_incompatible** | gateway `rsp_cd=01900` (`is_paper_incompatible()` true) — KRX 야간파생 night-derivatives balance is not provided in paper trading. No runtime flip this wave; `venue_session: krx_extended` row (§11.1) **retained** (ships venue-provisional, never confirmed). |
| CCENQ10100 | Tracked, **paper_incompatible** | gateway `rsp_cd=01900` (`is_paper_incompatible()` true) — KRX 야간파생 night-derivatives orderable-quantity is not provided in paper trading. No runtime flip this wave; `venue_session: krx_extended` retained. |

**`01900`, not off-window empty.** Both night reads return a definitive gateway
`01900` (paper-incompatible) regardless of the krx_extended window — a hard venue
rejection, not a `00707`/off-window empty result. By the disposition state machine
this is the `gateway 01900 paper-incompatible` terminal: Tracked with
`paper_incompatible: true`, no runtime authored. The SDK structs/policies/smoke
harnesses for both ship anyway (callable the day paper supports them), but they are
NOT flipped to Implemented. The night window therefore did not gate the outcome;
no in-window retry would change a `01900`.

**`caller_supplied_identifiers` (CFOAQ10100, `[FnoIsuNo]`).** Confirmed accepted —
`A0169000` (the live KOSPI200 Sep-2026 index future, discovered via the t8467/t9943
index-futures masters; the raw-capture `101*6000` codes are obsolete and return
`01414`/`01706`). The provisional caller-input facet is **retired** for CFOAQ10100.

**Residual provisionality (CFOAQ10100).** `venue_session: unspecified` is
session-agnostic (account read); the F/O orderable-quantity read returned a
non-empty success during the KRX regular session, consistent with
session-independence. Field-level `type` facets stay flagged (a clean deserialize
does not confirm the HTTP-500-seeded types). Recommended tier untouched.

---

## 13. Reach wave U3 — Standalone lane (t1988, t3102, t3320) (2026-06-24)

Three reads carrying a placeholder `owner_class: standalone` — but the
`standalone` module is OAuth-only (token/revoke) and cannot host a data read, so
all three route through `market_session` (non-paginated, `category: MarketData`),
correcting `owner_class` from `standalone` to `market_session` at flip time
(KTD3). **2 implemented, 1 HELD (input-unresolved).**

| TR | End state | Disposition (credential-free) |
|---|---|---|
| t1988 | **implemented** | `rsp_cd=00000 assets=71` (기초자산리스트조회 ELW underlying-asset list; `mkt_gb="0"` all markets, all filters off). The prior `IGW40011` (§8) was the `from_rate`/`to_rate` **wire-type defect** (KTD4): the two Number-typed request fields were quoted strings; serializing them as JSON numbers via `string_as_number` cleared it. Canonical out-block field 코스피종목건수 (`ksp_cnt`); detail rows under `t1988OutBlock1` (Object-Array, `de_vec_or_single`). |
| t3320 | **implemented** | `rsp_cd=00000 summary=1` (FNG_요약 FnGuide company summary; `gicode="005930"` bare 6-digit 삼성전자, accepted live — the `A005930` FnGuide form returned a sparse body, the bare 6-digit form returns the populated summary, found via a raw-probe A/B per KTD9). Single objects under `t3320OutBlock` (summary) + `t3320OutBlock1` (ratios); canonical 한글기업명 (`company`) + 현재가 (`price`) pinned to distinct values (KTD6). |
| t3102 | **HELD — input-unresolved** | 뉴스본문 (news body) requires a news number `sNewsno`. The ONLY catalog producer of a news number is `NWS` (실시간뉴스제목패킷), a realtime **WebSocket** feed held to the separate realtime effort (out of scope). No REST producer of `sNewsno` exists and no implemented TR yields one, so the caller input cannot be discovered in this REST-only wave. SDK structs + offline tests authored (title block round-trips) but no smoke target, no flip. |

**t1988 — IGW40011 resolved, not environmental.** The §8 disposition recorded
t1988 PENDING on persistent `IGW40011` and called for "gateway-form resolution".
This wave resolved it: the cause was wire-type (request `from_rate`/`to_rate` sent
as strings), not provisioning. The `string_as_number` fix (the same KTD4 defect as
`t3341.idx` / `t1664.cnt`) cleared it on the first smoke. Its `venue_session` (§1,
`krx_regular`) and `caller_supplied_identifiers` (§2, `[]`) rows are **retired** on
the non-empty success.

**t3320 — gicode form found via raw-probe A/B (KTD9).** The first smoke returned
`rsp_cd=00000` but an empty SDK out-block for `gicode=A005930`. A credential-safe
raw-probe A/B showed `A005930` → body_len=638 vs bare `005930` → body_len=943: the
bare 6-digit ticker returns the populated summary. The smoke + tests use the bare
form; its `caller_supplied_identifiers` (§2, `[gicode]`) and `venue_session` (§1,
`krx_regular`) rows are **retired** on the non-empty success.

**t3102 — HELD, not PENDING (recorded reason).** PENDING is for callable-but-empty
or environmental; t3102 is neither — it is structurally un-callable in a REST-only
wave because its sole required input has no REST source. Its `venue_session` (§1)
and `caller_supplied_identifiers` (§2, `[sNewsno]`) rows are **retained**,
unconfirmed; `owner_class` stays the `standalone` placeholder (not reclassified
absent a live confirmation). A future realtime wave that models the `NWS` channel
can source a news number and implement it.

**Field-`type` facets (§4)** stay inventory-wide retired; nothing to retire here.
Recommended tier untouched (no Focused Evidence, no `recommendation` block, no
`metadata/evidence/<tr>.yaml`, no `EVIDENCE-FRESHNESS.md` edit).
