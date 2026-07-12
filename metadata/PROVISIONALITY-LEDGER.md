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
| ~~o3101~~ | ~~`[]`~~ | **RETIRED (U8, 2026-06-24)**: implemented; the futures-master paper smoke returned 85 rows with `gubun=""` and no instrument identifier, confirming the empty caller-input set (`[]`). | — |
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

> **Re-confirmed by §17 (2026-06-28).** Re-probed under the F/O-capable
> domestic_option lane (account …51), both reads STILL return `01900` — confirming
> this is a venue rejection, not the wrong-account artifact that affected the §16
> account reads. `paper_incompatible` retained. See §17.

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
| t3102 | **HELD — feeder identified (`NWS`), awaiting a live news event** | 뉴스본문 (news body) requires a news number `sNewsno`. Its feeder is now identified and Implemented: `NWS` (실시간뉴스제목패킷, realtime WebSocket) emits a 24-char `realkey` that is structurally the `sNewsno` input. A chained WS→REST smoke (`live_smoke_nws_t3102`) is staged: subscribe `NWS`, capture a `realkey`, thread it into `t3102`. No REST producer of `sNewsno` exists, so the flip remains gated on a **live** news frame — and the off-hours paper base rate may be ~zero. SDK structs + offline tests authored (title block round-trips); flip awaits a carrying chained smoke. |

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

**t3102 — HELD, feeder now identified (2026-06-29 update).** The original REST-only
wave recorded t3102 HELD because its sole required input (`sNewsno`) had no REST
source. That blocker is now partly resolved: `NWS` is Implemented and its `realkey`
is the news-number feeder, so the chain `NWS.realkey → t3102.sNewsno` is the unblock
path (documented at `crates/ls-sdk/src/market_session/mod.rs:7704–11538`). A chained
WS→REST smoke (`live_smoke_nws_t3102`) is staged. The flip stays HELD until that
smoke carries — it depends on a live news frame on the paper feed, whose off-hours
base rate may be ~zero. Its `venue_session` (§1) and `caller_supplied_identifiers`
(§2, `[sNewsno]`) rows are **retained**, unconfirmed; `owner_class` stays the
`standalone` placeholder (not reclassified absent a live confirmation).

**Field-`type` facets (§4)** stay inventory-wide retired; nothing to retire here.
Recommended tier untouched (no Focused Evidence, no `recommendation` block, no
`metadata/evidence/<tr>.yaml`, no `EVIDENCE-FRESHNESS.md` edit).

---

## 14. Night-overseas implement wave — paper-unavailable reclassification (2026-06-26)

Plan `docs/plans/2026-06-25-001-feat-night-overseas-elw-implement-wave-plan.md`
re-ran the Paper Live Smokes for the KRX-night derivatives trio and the
overseas-stock sextet **inside their nominal session windows** (01:11 KST — inside
the `krx_extended` ~18:00–05:00 window; 12:11 ET — inside the US regular session).
**Every contingent feed returned empty**, so none flipped; the nine are reclassified
**paper-unavailable** (callable, Tracked, never flip on paper). **0 implemented, 9
reclassified.**

| TR | Window at smoke | Disposition (credential-free) |
|---|---|---|
| t8455 | in `krx_extended` (01:11 KST) | `rsp_cd=00000` empty master array (`00707`) — KRX 야간파생 master, no paper feed |
| t8460 | in `krx_extended` | `rsp_cd=00000` empty option board (`00707`) — KRX 야간파생 option board, no paper feed |
| t8463 | in `krx_extended` | `rsp_cd=00000` empty investor-by-time array (`00707`) — KRX 야간파생, no paper feed |
| g3101 | in US regular session (12:11 ET) | empty out-block (`00707`) — overseas current-price, no paper feed |
| g3102 | in US regular session | empty result array (`00707`) — overseas time-series, no paper feed |
| g3103 | in US regular session | `rsp_cd=00009 해당 자료가 없습니다` — overseas period chart, no paper data |
| g3104 | in US regular session | empty out-block (`00707`) — overseas stock-info master, no paper feed |
| g3106 | in US regular session | empty out-block (`00707`) — overseas order book, no paper feed |
| g3190 | in US regular session | `rsp_cd=00000` empty result array (`00707`) — overseas master list, no paper feed |

> **Re-probed by §17 (2026-06-28) — night trio only.** Under the F/O-capable
> domestic_option lane (account …51), `t8455`/`t8460`/`t8463` now return `rsp_cd=00000`
> (the venue **accepts** the request — no longer the `00707` recorded here), but the
> modeled array is empty **off** the krx_extended window. The §14 "no paper feed"
> basis is weakened (request accepted, account entitled); the outstanding flip gate is
> an **in-window …51 re-smoke**. `paper_incompatible` retained conservatively (no
> positive data yet). The overseas-stock sextet (g31xx) was not re-probed (overseas
> stock runs on …01; out of this wave's scope). See §17.

**Empty/no-data, NOT `01900` service-rejection (the §12 distinction, inverted).**
Unlike the CCENQ night pair (§12), which returns a hard gateway `01900`, these nine
return a paper-unavailable empty result even when smoked inside the correct session
window: eight return a *clean* `rsp_cd=00000` with an **empty body** (`00707`), and
g3103 returns `rsp_cd=00009 해당 자료가 없습니다` — both are no-data terminals, neither is
`01900`. The request shape is accepted (no `01900`, no `IGW40011`); the paper
environment simply carries no data for these feeds. An in-window re-run does not recover
them — the plan's `pending:off-window` premise (a timing miss) was falsified by these
in-window-empty smokes, so they land at the paper-unavailable terminal instead.

**Facet vs. runtime classifier — a deliberate divergence.**
`facets.paper_incompatible: true` is set on all nine as the machine-readable
"won't flip on paper" documentation/routing signal, so the discovery query and future
waves skip them. **This does NOT imply the runtime `ls_core::is_paper_incompatible()`
fires** — that check is `01900`-specific and these return `00707`. The facet here means
"no paper data feed (feed-unprovisioned)", distinct from §12's "gateway 01900". The
pre-existing `venue_session` rows are **retained**, unconfirmed: §11.1 covers the night
trio (`krx_extended`) and the Wave-0 overseas reads g3101/g3104/g3106 (`unspecified`).
g3102/g3103/g3190 were batch-tracked later (no §11.1 row); their `venue_session:
unspecified` facets were set at tracking time and are recorded here in §14 for the first
time.

**No flip, no docgen change.** `support.implemented` stays `false` for all nine;
`reference.len()` and `banner_trs` are unchanged (zero flips this wave). The four
overseas-futures reads (`o3105`/`o3106`/`o3125`/`o3126`) were already Implemented in a
prior wave (front-month symbol refresh) and are untouched. `t2106` (domestic F/O
price-memo, empty memo) and `t1964` (ELW board, input-unresolved) keep their existing
PENDING dispositions — both are domestic, not part of this night/overseas
reclassification. Recommended tier untouched.

---

## 15. Closed-window more-flips wave — tracked-only pool triage (2026-06-27)

Plan `docs/plans/2026-06-27-001-feat-closed-window-more-flips-plan.md`, U1. The 73
tracked-only TRs are classified into exactly one bucket each. KRX is closed (Saturday),
so only static/persistent reads are reachable; the static bucket is the input to the
U2/U3 flip batches. Every non-candidate carries its reason here so no TR is silently
dropped (R1, R2, R7).

**Static-flippable candidates (22) — smoked under closure in U2/U3.** A candidacy
heuristic (master/reference, designation, ranking, ELW/F-O persistent quote, historical
chart), confirmed per-TR by the flip gate (R4/R5: deserializes + a non-default modeled
field). A static-classified read that smokes empty is recorded as heuristic
over-inclusion, not flipped.

- `market_session` (17, batch A): `t1308` `t1449` `t1621` `t1638` `t1906` `t1950`
  `t1956` `t1959` `t1969` `t1971` `t1972` `t1974` `t2106` `t2545` `t8406` `t8407`
  `t8450`. (`t2106` is finish-the-flip — request/response/facade/smoke already wired,
  prior wave left it PENDING on an empty memo; re-smoked under closure here.)
- `paginated` (5, batch B): `t1410` `t1411` `t1488` `t1636` `t1809`.

**`paper_incompatible: true` (11) — excluded before candidacy (R2).** They never flip on
paper under any session (recorded in §12/§14): `CCENQ10100` `CCENQ90200` `g3101`
`g3102` `g3103` `g3104` `g3106` `g3190` `t8455` `t8460` `t8463`.

**Hard-blocked (5) — left untouched, need an input or a non-read path closure does not
provide (R3).** `t1860` (realtime-control subscription, not a read), `t1852`/`t1856`
(require `sFileData` input), `t3102` (requires `sNewsno` input), `t1964` (empty ELW
board, input-unresolved).

**Session-dependent (35) — deferred unsmoked to a future open-window wave (R1/R6).**
Live quote/orderbook, time-and-sales, intraday session flow, and other reads closure
guarantees return empty `00707`; deferred, not smoked-then-dispositioned this wave:
`t1109` `t1301` `t1471` `t1475` `t1486` `t1602` `t1603` `t1617` `t1631` `t1632` `t1633`
`t1637` `t1665` `t1702` `t1716` `t1717` `t1752` `t1771` `t1902` `t1904` `t1927` `t1941`
`t1951` `t1954` `t1973` `t2210` `t2212` `t2214` `t2407` `t2424` `t2541` `t8404` `t8427`
`t8428` `t8454`.

22 + 11 + 5 + 35 = 73 — every tracked-only TR carries exactly one disposition.

**Wave outcome (U2/U3 close-out).** Of the 22 static-flippable candidates, **21
flipped to Implemented** under closure on non-empty paper smokes (`reference.len()`
141 → 162) — a far-from-dry pool, so the U4 raw top-up was dropped per the plan's
follow-up guidance (the wave stands on the pool audit alone). The static-classified
heuristic over-included exactly **one**: `t2106` (선물/옵션현재가시세메모, F/O
price-memo) stayed PENDING — its memo array smoked empty (`rsp_cd=00000`, empty
`t2106OutBlock1`) even with a live contract sourced via t8467, an independent
session-dependent signal (memo entries populate during the session), consistent
with its prior §14 PENDING. Two candidates first looked blocked but flipped after
faithful re-classification: `t2545` (IGW40011 was a bad `bgubun="1"` value, not a
wire-type defect — `bgubun="0"` returns non-empty) and `t8406` (the static
raw-capture `focode` was an expired contract; a live front-month contract sourced
via t8467 returned non-empty last-session rows). The numeric-request gotcha (KTD3)
applied to t1621/t8407/t1969/t2545/t8406 and the paginated cursors
t1411/t1488/t1636 — all serialized via `ls_core::string_as_number`. Every flip
landed `recommended: false` with a deferred open-window freshness re-check note (R9)
for the later Recommended pass. All 21 flips were closure-flips; a session-stale
persistent body passes the R4 gate identically to a live one (R5).

## 16. Closed-window account-lane flip wave — account raw pool retired (2026-06-28)

Plan `docs/plans/2026-06-28-001-feat-closed-window-account-lane-flip-plan.md`. The
market-data static pool was drained by wave #62, so this wave prospects the **account
lane** — the residual untracked `account_state` reads — under KRX closure (Sunday).
Every one of the ~30 account-read candidates was raw-probed credential-safe (R3) and
carries exactly one disposition here (R11), so a future wave does not re-prospect the
same dry codes. KRX closed; account-state persistence is what makes the subset
reachable.

**U2 holdings gate (R4/KTD3).** `t0424`'s typed smoke returned `holdings=0` with a
non-default cash summary (`sunamt`) — the paper account is **cash-only, no securities
positions**, and (corroborated by `cfofq02400` 00707 OI + `CFOEQ11100` all-zero
deposit) **no F/O funding**. So the cash/reference reads certify, but every
positions-/deposit-dependent read is downgraded to expected-empty (AE2). This is NOT
the stop condition: the cash/reference reads still certify, so the "best odds" premise
held; only the positions sub-lanes collapsed.

> **Corrected by §17 (2026-06-28).** The "no F/O funding" conclusion was a
> **wrong-account artifact** — every §16 account read authenticated as the domestic
> cash account (…01) because the SDK is one-token=one-account. Under per-account
> credential lanes the F/O account (…51) IS funded: `CFOEQ11100` `Dps` is non-default.
> `CFOEQ11100`/`CIDBQ01400` flip to Implemented and `CIDBQ03000`/`CIDBQ05300` (§16
> `00707`/`IGW40013`) become reachable; t0441 stays PENDING for a different reason
> (no open positions). See §17.

**FLIPPED → Implemented (3) — non-default substantive field certified under closure:**
- `t0424` (주식잔고2, account) — cash-summary flip, dispositioned distinctly (holdings
  array empty; cash witness `sunamt` non-default). `reference.len()` 162→163.
- `t0167` (서버시간조회, market_session/utility) — server time non-default. 163→164.
- `CLNAQ00100` (예탁담보융자가능종목, account, `/stock/etc`) — 20 loanable stocks,
  non-default `IsuNm` (the `IGW40013` raw-probe failure was a value issue: an
  `A`-prefixed `IsuNo` is rejected; empty `IsuNo` / full-list mode returns the list).
  164→165.

**PENDING (4) — callable + deserializes, but all substantive fields default on THIS
cash-only/position-less/overseas-ineligible paper account (R6).** Each carries callable
Rust + offline tests + a paper smoke + a registered `{TR}_POLICY`; re-test open-window
or on a funded/eligible account:
- `CSPBQ00200` (증거금률별주문가능수량, account) — `00136` 1 row, but all
  capacity/deposit fields (`Dps`/`SeOrdAbleAmt`/`PrsmptDpsD1`) zero across `OrdPrc`
  0/75000/10000 and ISIN Samsung + `KR7000020008`; the margin-capacity computation is
  session/data-dependent under closure.
- `CFOEQ11100` (선물옵션가정산예탁금상세, account) — `00136` 1 row, but `Dps`/`OpnmkDps…`/
  `CsgnMgn` all zero (no F/O funding; confirms the U2 cash-only gate).
- `t0441` (선물/옵션잔고평가, account) — `00000`, positions=0, `tappamt`=0 (AE2
  expected-empty, exactly as the U2 gate predicted).
- `CIDBQ01400` (해외선물 주문가능수량, account) — `00136` 1 row, but `OrdAbleQty` default
  (overseas paper historically empty/ineligible).

**Empty under closure → deferred PENDING (no flip, raw-probe only).** History-dependent
or no-position reads that smoke empty are the expected case (R6, defer without ceremony):
`cspaq13700` `cdpcq04700` (00707 history), `cfofq02400` `cfoaq00600` (00707 F/O
history/OI), `cidbq01500` `cidbq01800` `cidbq02400` `cidbq03000` `cideq00800`
`cosaq01400` (00707 overseas), `t0150` `t0151` `t0434` (00000 bare-envelope, no data
block).

**`paper_incompatible` (01900) — excluded, never flip on paper (R2).** `cspaq00600`
(신용한도) `foccq33600` `cfoaq50600` `cfobq10800` `cfoeq82600` `foccq33700` `cosaq00102`
`cosoq02701`.

**Gateway error / proven residual (R7).** `cidbq05300` (overseas-futures 예탁자산) —
`IGW40013` persists across body variants → environmental, defer. `cosoq00201`
(해외주식 종합잔고) — `IGW40014` is a **documented proven residual** (server-derived
`002US` literal in a numeric field, `docs/design/ls-gateway-response-semantics.md`) →
defer, not an SDK defect.

**Excluded at triage (not account reads / out of scope, R1).** Order TRs (`cfoat*`
`cidbt*` `cosat*` `cosmt*` `ccent*`); overseas market-data (`g3202`–`g3204`
`o3103`/`o3104`/`o3107`/`o3108` `t3518`/`t3521`); KRX night-derivatives market-data
(`t8456`–`t8462`); `ccenq30100` (night history); `mmdaq91200` (known `01900`).

**Wave outcome.** 3 of 7 certifying-candidates flipped (`reference.len()` 162→165); the
4 PENDING are the cash-only/position-less/overseas-ineligible paper account's expected
shape, not defects. The account raw pool is **retired** — every account-read candidate
carries a disposition above. A near-dry-but-positive close-out: the cash/reference lane
yielded the domestic persistent reads, and the holdings gate proved the positions lanes
are unreachable without a funded paper account. All flips land `recommended: false`
(separate ADR-gated pass).

## 17. Paper account credential lanes — wrong-account correction wave (2026-06-28)

Plan `docs/plans/2026-06-28-002-feat-paper-account-credential-lanes-plan.md`. §16's
"U2 holdings gate" concluded the paper account has **no F/O funding** from
`CFOEQ11100`'s all-zero deposit — but that smoke (like every §16 account read)
authenticated as the **domestic cash account (…01)**, because the SDK is one-token =
one-account and the account number is never on the wire. A 2026-06-28 diagnostic
proved each LS paper account is bound to its **own appkey**: sourcing a per-account
lane file switches the resolved account. Re-smoked under the correct lane (U1 var
rename `LS_PAPER_APIKEY` + real-money interlock; U2 Makefile maps `instrument_domain`
→ `.env.<lane>`), the §16 "all-default" account reads carry real data. The §16
"no F/O funding" finding is **a wrong-account artifact, retracted**: the F/O account
(…51) is funded (CFOEQ11100 `Dps` non-default).

**U3 — three tracked reads re-smoked under their lane.**

| TR | lane (acct) | smoke (credential-free) | End state |
|---|---|---|---|
| CFOEQ11100 | domestic_option (…51) | `rsp_cd=00136 deprows=1 dps_nd=true` (선물옵션가정산예탁금상세; `Dps` deposit non-default) | **implemented** — §16 PENDING retracted (was all-zero on …01). `reference.len()` 165→166. |
| CIDBQ01400 | overseas_option (…71) | `rsp_cd=00136 rows=1 qty_nondefault=true` (해외선물 주문가능수량; `OrdAbleQty` non-default; `IsuCodeVal=ADM23` accepted) | **implemented** — §16 PENDING retracted (was default on …01). 166→167. `caller_supplied_identifiers: [IsuCodeVal]` confirmed accepted. |
| t0441 | domestic_option (…51) | `rsp_cd=00000 positions=0 tappamt=0` (선물/옵션잔고평가) | **PENDING (corrected)** — now reachable on its own lane; the …51 account is funded (deposit present) but holds **no open F/O positions**, so the valuation is genuinely empty (reachable-but-no-positions, not wrong-account). Flip pending an open F/O position. |

**U4 — night-derivatives re-probed under domestic_option (…51).**

| TR | re-probe | End state |
|---|---|---|
| CCENQ10100 | raw `rsp_cd=01900` | **paper_incompatible retained** — `01900` persists even on the F/O-capable …51 account, so it is a **venue rejection, not account-binding** (§12 re-confirmed under the F/O lane). |
| CCENQ90200 | raw `rsp_cd=01900` | **paper_incompatible retained** — same; §12 re-confirmed under the F/O lane. |
| t8455 | raw `rsp_cd=00000` (body 1498); typed → empty master array | **paper_incompatible retained, basis corrected** — under …51 the venue **accepts** (`00000`, no longer the §14 `00707`), but the modeled array is empty **off the krx_extended night window**; the §14 "no paper feed" basis is weakened (request accepted, account entitled) but unproven without data. The outstanding flip gate is an **in-window (~18:00–05:00 KST) re-smoke under domestic_option**. |
| t8460 | raw `rsp_cd=00000` (body 60); typed → empty board | **paper_incompatible retained, basis corrected** — same. |
| t8463 | raw `rsp_cd=00000` (body 4631); typed → empty time-series | **paper_incompatible retained, basis corrected** — same. |

The CCENQ pair and the t845x trio diverge: CCENQ is a hard `01900` (true venue
rejection), t845x now returns `00000`-but-empty (session-gated). Neither flips this
wave; the t845x facet is kept conservatively (no positive data observed) with the
in-window …51 re-smoke recorded as the remaining gate.

**U5 — bounded track-and-flip of newly-reachable raw account reads (≤8).**
The June-28 raw candidate pool was re-probed under its lane. F/O (domestic_option)
came back dry — `01900` (CFOAQ50600, CFOEQ82600, CFOBQ10800, FOCCQ33700) or empty
`00707` (CFOFQ02400, CFOAQ00600) — **0 qualify**, held. Overseas-F/O
(overseas_option, …71) yielded **2** with the `00136`+populated-body signature the
flipped reads share:

| TR | lane (acct) | smoke (credential-free) | End state |
|---|---|---|---|
| CIDBQ03000 | overseas_option (…71) | `rsp_cd=00136 rows=5 asset_nd=true` (해외선물 예수금/잔고현황; `EvalAssetAmt` non-default) | **implemented** — was `00707` on …01 (§16); resolves with data on …71. `TrdDt` must be a **trading day** (a weekend returns `01715`); the smoke walks back to the most recent weekday. 167→168. |
| CIDBQ05300 | overseas_option (…71) | `rsp_cd=00136 rows=5 dps_nd=true` (해외선물 예탁자산; per-currency `OvrsFutsDps` non-default) | **implemented** — was `IGW40013` on …01 (§16); the gateway error was a **wrong-account artifact**, cleared under the correct account. 168→169. |

The remaining overseas candidates smoked empty `00707` (CIDBQ01500, CIDEQ00800,
CIDBQ01800, CIDBQ02400) — held, not tracked. No overflow beyond the cap.

**§16 corrections (R11).** CFOEQ11100 and CIDBQ01400 move from §16 PENDING to
Implemented; t0441 stays PENDING but its reason changes from "no F/O funding /
cash-only account" to "reachable on its own lane, account funded, no open
positions." CIDBQ03000 (§16 `00707`) and CIDBQ05300 (§16 `IGW40013`) move from the
§16 deferred/error lists to Implemented. The §16 "no F/O funding" gate conclusion
is retracted as a wrong-account artifact. Recommended tier untouched for all.

## 18. All-lane closed-window flip wave — REST lane close-out (2026-06-28)

Plan `docs/plans/2026-06-28-003-feat-all-lane-closed-window-flip-wave-plan.md`. A
breadth sweep over the 143-code raw untracked pool across all four instrument
domains and both transports. U1 raw-probed every read survivor credential-safe
(http/rsp_cd/body_len only); the full classification is
`docs/plans/notes/all-lane-flip-classification.md`. Every one of the 143 codes
carries exactly one disposition here (R11). KRX closed (weekend); session-independent
master/reference/chart-persistence reads are what flips.

**Owner scope decision (2026-06-28).** The trackable pool came in ~2× the plan's
30–50 estimate. The owner chose to **ship the REST lane this session and stage the
84-channel WebSocket track+flip as a separate follow-up realtime wave** (mirroring
the prior 31-channel realtime wave's own 2-PR delivery). The WS classification is
recorded below; no WS metadata/flips were authored this wave.

**FLIPPED → Implemented (13) — non-empty modeled witness certified under closure.**
Each builds/sends/deserializes a non-empty paper success with a substantive modeled
field asserted (R4); each new smoke routes record/panic through the shared scrubber
and installs the dispatch-log suppressor (R11b). `reference.len()` 169 → 182
(+13: +2 in commit `fe5efa7`, +11 in `d1c89d5`); `recommended:false` on all.

- **Domestic /stock/investinfo (2, lane …01):** `t3518` (해외실시간지수 time-series; 20
  index-tick rows, non-default `price`), `t3521` (해외지수조회 snapshot; non-default
  `close`). Overseas-index data served via the domestic endpoint persists under KRX
  closure. `t3521` out-block modeled from `res_example` (raw has no `res_b` props).
- **Overseas-futures (10, lane overseas_option …71):** `o3103` `o3104` `o3108` `o3116`
  `o3117` `o3123` `o3128` `o3136` `o3137` `o3139` (분/일주월/tick/NTick charts +
  daily-fills). KEY: these serve last-session data on paper under closure **only with
  a current front-month contract** (`CUSN26`); the raw `req_example`'s stale 2023
  contract (`ADM23`) returns empty — a contract-staleness confound, not a feed gap
  (the §15 `t8406` lesson, repeated). `o3104` additionally needs a recent `date`.
- **KRX night-derivative (1, lane domestic_option …51):** `t8462` (야간파생
  투자자기간별; 19 investor rows with a recent date range). The investor-by-period
  aggregation persists across the night window — unlike the night quote/board feed
  (see drops below).

**PENDING — tracked, callable, but empty/all-default on this account under closure
(R6/R10).** Carry callable Rust + `{TR}_POLICY` + offline tests + a paper smoke;
`implemented:false`, excluded from `reference.len`/`banner_trs` (the §16
PENDING-with-policy convention, e.g. CSPBQ00200). Re-test open-window / on a
populated watchlist:
- `o3107` (해외선물 관심종목, single-symbol watchlist) — empty `00000` (98 bytes); no
  registered symbols on the paper account.
- `o3127` (해외선물옵션 관심종목 board) — `00000` board rows all `price=0`; account-state
  watchlist, no registered symbols (the holdings-gate analogue for a board read).

**DROPPED from tracking (R11a — probe matched an already-recorded dry terminal;
recorded here, no metadata authored).**
- **Night-derivative quote/chart feed — off-window + weekend empty (§17 t845x
  precedent):** `t8456` `t8457` `t8458` `t8459` `t8461` — all `00000` empty off the
  krx_extended night window (stale focode `101W6000`), same session-gated feed §17
  proved empty for t8455/t8460/t8463.
- **Overseas-stock charts — no paper feed (§14 overseas-stock precedent):** `g3202`
  `g3203` (empty `rsp_cd`, 26-byte error envelope), `g3204` (`00000`, 61-byte empty)
  — overseas-stock carries no paper feed (§14 g31xx sextet).
- **Venue rejection `01900` (§12 precedent):** `CCENQ30100` (KRX 야간파생 주문/체결내역;
  raw `01900`), `MMDAQ91200` (파생상품증거금율; known `01900`, §16).

**EXCLUDED — R3 order/mutation (14, never read-only):** `CFOAT00100/00200/00300`
`CCENT00100/00200/00300` `CIDBT00100/00900/01000` `COSAT00301/00311/00400`
`COSMT00300` `CFOBQ10800` (옵션매도 주문증거금조회 under /order; also §16 `01900`).

**EXCLUDED — already-dispositioned account reads (§16/§17, R11a, 22):**
`01900` (8): `CSPAQ00600` `FOCCQ33600` `CFOAQ50600` `CFOEQ82600` `FOCCQ33700`
`COSAQ00102` `COSOQ02701`, plus `COSOQ00201` (`IGW40014` proven residual).
empty `00707` across all lanes, retired (12): `CSPAQ13700` `CDPCQ04700` `CFOFQ02400`
`CFOAQ00600` `CIDBQ01500` `CIDBQ01800` `CIDBQ02400` `CIDEQ00800` `COSAQ01400` `t0150`
`t0151` `t0434`.

**WebSocket — 84 channels DEFERRED to a follow-up realtime wave (owner decision).**
All `owner_class: realtime` push channels (stock 52, futureoption 24, sector 1,
overseas-futures 2, etc 2, investment-info 3). Connection-reachable-only flips
(KTD6 NOT-OBSERVABLE). Classified in the U1 note; not authored this wave. Tracking
+ flip is the follow-up's scope.

**Count tally (R13).** `maintained_tr_count` 222 → 237 (+15 tracked: 13 flipped +
o3107 + o3127); manifest + `api_drift.rs` + `cli.rs` (×4) + docgen `TRACKED_TRS`
(`[&str; 237]`) all consistent; `manifest.refreshed` held at 2026-06-22 (KTD7).
`reference.len()` 169 → 182 (+13 flips); `banner_trs` +13. WebSocket channels add
nothing this wave (deferred). The §14/§16/§17 retired terminals are NOT re-probed.

## 19. Open-window flip wave — ELW daily flip + session-residual dispositions (2026-06-30)

Plan `docs/plans/2026-06-30-001-feat-open-window-domestic-flip-wave-plan.md`. The
raw pool was exhausted, so this wave targeted the residual of the §15
"session-dependent (35)" cohort under a live KRX regular session (10:xx KST). The
window's real unlock was narrow: **ELW daily-price data is live on paper; F/O
index-futures intraday feeds and ELW *intraday* tick feeds are paper-empty even
mid-session, and the after-hours read needs the after-hours session.** Probed all 10
targets in-window (raw-probe + the 4 already-wired typed smokes); every target now
carries one terminal disposition (R11), so a future wave does not re-prospect them.

**Flipped (1).** `t1954` (ELW일별주가) — open-window paper smoke `rsp_cd=00000
rows=20`, non-empty first-row `close` witness. market_session ELW read, `cnt` numeric
request slot (`string_as_number`). `reference.len()` 279 → 280; `banner_trs` +1;
`maintained_tr_count` unchanged (tracked→implemented). `recommended: false`
(open-window freshness re-check deferred to a later Recommended pass, R9). No
per-facet ledger entries existed for t1954 (clean projected baseline) — nothing to
retire.

**PENDING — paper-empty under the open window (5).** Confirmed empty on a live
in-window probe/smoke, not a closure artifact: `t1951` (ELW시간대별체결, tick array
body_len≈112 ≈ empty), `t2212`/`t8404` (F/O 시간대별체결) and `t2407` (F/O
호가잔량비율챠트) — same paper-empty family as the already-wired `t8427` (F/O day
chart, live front-month contract → empty) and `t2106` (F/O 시세메모, empty memo).
`t1973` (ELW시간대별예상체결, body_len≈424) is auction-period data, near-empty in
continuous session — held PENDING (no carrier per KTD2). Paper carries no data for
these intraday derivative feeds regardless of session; do not re-attempt as
breadth.

**PENDING — wrong session (1).** `t1109` (주식시간외체결, after-hours ticks) returns
empty `00707` during the regular session by construction; it would require the
after-hours window (after 15:30 KST). Retriable then; not a paper-data gap.

**HELD (1).** `t1964` (ELW board) — its blocker is the 10 unresolved filter-enum
defaults (§ prior HELD), not the window; the in-window smoke found no non-empty
board. Stays HELD per `implement-tr` §0.

**Count tally (R13).** Only `reference.len()` (279→280) and `banner_trs` (+1) move;
`maintained_tr_count`, `cli.rs` literals, `api_drift`, and `TRACKED_TRS` are
unchanged (a tracked→implemented flip is not a tracking event). The 4 already-wired
targets (t1109/t8427/t2106/t1964) stay `implemented: false` — their carriers and
smokes remain in place for a future qualifying session.

## 20. Closed-window probe-and-flip sweep — full-residue disposition pass (2026-06-30)

Plan `docs/plans/2026-06-30-004-feat-closed-window-probe-flip-sweep-plan.md`. Goal:
drive every one of the **41 Tracked-not-Implemented TRs** to exactly one terminal
disposition under KRX closure. Outcome: **0 flips** — every flip requires a live
non-empty deserializable witness (KTD2/R4), which the autonomous closed-window run
cannot certify; the gate does not run live smokes, so a metadata flip without a
passing `make live-smoke-<tr>` would be green-but-uncertified (forbidden). The
deliverable is this consolidated, current-dated disposition ledger for all 41 plus
a handoff of the 5 genuinely probe-gated candidates.

**D5 honesty note: this wave is predominantly RE-CONFIRMATION, ~0 net-new
dispositions.** By execution time every one of the 41 already carried a current
terminal disposition (most from §13–§19; the intraday-feed cohort and t1109 were
freshly probed *the same day* in the §19 open window). The wave's genuine value is
(a) proving the Tracked-not-Implemented residue is fully and currently
dispositioned, (b) confirming both the raw pool *and* the offline tracked-flip pool
are exhausted under closure, and (c) surfacing the 5 probe-gated candidates an
operator (creds + right session) could still move. A 0-flip pure-reconfirmation
outcome is a successful wave per the plan DoD.

**Partition of the 41 (KTD1):** 19 confirm-only + 10 deferred-orders + 7
§19-reconfirm intraday feeds + 5 probe-gated = 41.

**Lane A — confirm-only (19), re-affirmed, no live attempt (R6/R7).**
- *paper_incompatible (13):* `g3101` `g3102` `g3103` `g3104` `g3106` `g3190`
  (overseas-stock, no paper feed — §14); `t8455` `t8460` (KRX night-derivative
  quote/board, off-window paper-empty — §17); `CCENT00100/00200/00300` `CCENQ10100`
  `CCENQ90200` (KRX 야간파생 order/account, `krx_extended` + `01900` — §16/§17/§18).
  Facet `paper_incompatible: true` holds; reason unchanged.
- *carried-forward terminal (3, plan-explicit):* `t1631` permanent PENDING (gateway
  `IGW40014` — server fails to serialize its own `bidvolume`; recorded in
  `docs/solutions/conventions/tr-pool-exhaustion-and-closure-viability.md`); `t3102`
  HELD (no off-hours `NWS` frame; feeder identified, flip awaits a live news event —
  §13); `t1964` HELD (10 unresolved filter-enum defaults; §19 in-window found an
  empty board — §7/§19).
- *de-facto terminal — structural/scope (3, re-routed here by judgment, not in the
  plan's explicit confirm-only list):* `t1852` / `t1856` PENDING (required `sFileData`
  screening blob ~26.8 KB unsourced — a probe cannot construct a valid request, so a
  fresh closed-window probe cannot change the outcome — §6); `t1860` HELD
  (realtime-registration control, not a read — §6). Routed to confirm-only because
  the blocker is structural/scope, not session/funding; **no operator probe needed.**

**Lane B — deferred-orders (10), re-confirmed `deferred` (R3/KTD1).** F/O order chain
`CFOAT00100/00200/00300`; overseas-futures orders `CIDBT00100/00900/01000`;
overseas-stock orders `COSAT00301/00311/00400` `COSMT00300`. All `owner_class:
orders`, already recorded EXCLUDED-order in §18. Not probed — orders reject
off-window (only re-derives `01458 장종료`). Flip is an operator-run open-window F/O
order smoke (deferred wave), out of this wave's identity.

**Lane C1 — §19-reconfirm intraday feeds (7), PENDING, no re-probe.** `t1951`
`t1973` `t2212` `t2407` `t8404` `t8427` `t2106` were all probed **the same day** in
the §19 open window and recorded PENDING paper-empty. §19 concluded paper carries no
data for these intraday F/O/ELW feeds *regardless of session* — so a closed-window
re-probe cannot beat an in-window empty. Disposition unchanged (PENDING).
*Note:* the plan scope-boundary anticipated a `deferred` label for these as "genuine
open-window reads"; §19's same-day **in-window** empty evidence overrides that to
PENDING — they are paper-feed-absent, not merely session-gated, so they would not
flip on an open window either.

**Lane C2 — probe-gated, BLOCKED, handed back to operator (5).** Disposition carried
forward; an operator with credentials and the right session could still move these.
- `t1109` (주식시간외체결) — **`deferred` to an after-hours run (KTD5).** §19 confirmed
  its last probe was the regular session (10:xx KST); it needs the post-15:30
  after-hours window, untested. If the operator runs in the after-hours window, probe
  and flip only on non-empty 시간외체결 ticks; otherwise it stays `deferred`. Carrier
  is already fully wired (finish-the-flip — metadata + docgen only on a non-empty
  witness).
- `CSPBQ00200` (증거금률별주문가능수량, account) — carry-forward PENDING (§16; all-default
  `00136` on a zero-deposit account). Needs a funded margin context. Credential-gated.
- `o3107` (해외선물 관심종목) / `o3127` (해외선물옵션 관심종목 board) — carry-forward PENDING
  (§18; empty/`price=0`, no registered watchlist symbols). `overseas_option` lane +
  account watchlist state. Per the plan Assumption + R7, carried forward absent a
  plausible account-state change since §18 (2026-06-28/29); **not re-probed** in this
  autonomous run. Operator may re-probe under the `.env.overseas_option` lane
  (holdings/board gate, KTD3) to harden or flip.
- `t0441` (선물/옵션잔고평가, account) — carry-forward PENDING (§18; `positions=0` on the
  funded …51 account). Needs an open F/O position. `domestic_option` lane,
  position-state-gated; carried forward absent a plausible position change. Operator
  may re-probe under `.env.domestic_option`.

**Count tally (R8/R13).** 0 flips → nothing moves. `reference.len()` stays **280**,
`banner_trs` unchanged, `maintained_tr_count` stays **320**, `cli.rs` literals,
`api_drift`, and `TRACKED_TRS` all unchanged. No `metadata/trs/*.yaml` facets edited
(every reason on file still holds). `recommended` deferred for all (no flips). The
41-TR residue is fully and currently dispositioned; the offline tracked-flip pool is
exhausted under closure.

## 21. KRX-open domestic F/O order certify-and-flip wave (2026-07-01)

Plan `docs/plans/2026-07-01-001-feat-krx-open-domestic-fo-order-certify-flip-plan.md`.
Goal: certify and flip the staged domestic F/O order chain
`CFOAT00100/00200/00300` (deferred-order Lane B of §20) on a live in-window run, plus
the conditional funded-margin read `CSPBQ00200`. Outcome: **3 flips** (the F/O order
chain → Implemented); `CSPBQ00200` and `t0441` carried forward PENDING.

**Prerequisite (U1) — F/O order-smoke credential lane.** `make live-smoke-fo-order`
sourced the default `.env` while the F/O reads (incl. `t0441`) authenticate on
`.env.domestic_option` (…51). Corrected to the `domestic_option` lane with a
fail-closed guard (refuses to fall back to `.env` when the lane file is absent) so the
order chain and `t0441` read the same F/O-capable account. Also repaired three
decomposition-drift bugs the first live run exposed (test-decomposition PR #78 renamed
the order-smoke tests into `#[path]` submodules, so the Makefile `--exact` filters
matched 0 tests) and self-sourced the F/O contract from the `t8467` index-futures
master (front-month) so no stale contract is hand-supplied.

**Certification (R4/R5) — three operator in-window runs, KRX open 2026-07-01.** The
first two runs were **non-certifying but diagnostic** (R7): they proved the plan's
seed ack codes and the modify leg were wrong, and that `t0441` returns an EMPTY array
(not a `positions=0` row) on a flat account. Corrected wire truth (F/O shares the
domestic-stock ack family): **submit `00040`, modify `00462`, cancel `00463`** (the
plan's `00132`/`00156` seeds were both wrong); the modify is a quantity REDUCTION
(submit qty 2 → 1; an INCREASE is rejected `01442` 정정수량 초과); and an empty `t0441`
read is `Flat` (no position), not fail-closed `NotFlat` (else the always-flat chain
could never certify — the resting daily-limit order is unfillable and the clean cancel
proves removal). The **third run certified clean**: each leg acked from its own
response (submit `00040`/27158, modify `00462`/27159, cancel `00463`/27160), `t0441`
positively confirmed no fill, account left flat.

**Flips (R8) — 3.**
- `CFOAT00100` (선물옵션 정상주문, submit) → **Implemented** (rsp_cd `00040` 매수주문 완료).
- `CFOAT00200` (선물옵션 정정주문, modify) → **Implemented** (rsp_cd `00462` 정정주문 완료).
- `CFOAT00300` (선물옵션 취소주문, cancel) → **Implemented** (rsp_cd `00463` 취소주문 완료).
  `recommended` deferred for all three (live order-placement endorsement is a separate
  pass). Policies were already crosscheck-registered (order TRs, `is_order: true`,
  excluded from `slice_rest_policies_are_non_order_rest`) — no `ls-core` change.

**Carry-forward PENDING (R9/R10).**
- `CSPBQ00200` (현물계좌증거금률별주문가능수량, account) — **carry-forward PENDING** (§16/§20).
  R9 flips it only on a funded-margin witness; no funded-margin context was smoked this
  wave (the funded account is the `…51` F/O lane, whereas `CSPBQ00200` is a 현물/spot read
  on the default lane, which carries no cash deposit). Reason unchanged; re-attempt when
  a funded spot-margin context exists.
- `t0441` (선물/옵션잔고평가, account) — **carry-forward PENDING** (§18/§20). Needs a
  deliberately-held open F/O position, which the flatness-preserving chain never holds
  (a deliberate-position leg was considered and rejected for this wave). The corrected
  `t0441` empty→Flat verdict is the *no-fill* confirmation used inside the chain, not a
  balance-row witness — `t0441`'s own flip still awaits a non-empty position read.

**Count tally (R8/R13).** 3 flips (Tracked → Implemented). `reference.len()` **280 →
283** and `banner_trs` gains `CFOAT00100/00200/00300` (hand-edited in
`crates/ls-docgen/src/lib.rs::reference_covers_implemented_with_banner_and_omits_unimplemented`
— not caught by `make docs`). Tracked → Implemented does **not** move
`maintained_tr_count` (stays **320**), `cli.rs` literals, `api_drift`, or `TRACKED_TRS`.
`recommended` deferred for all. The domestic F/O order chain is now callable and
Implemented on paper; the deferred-order residue drops the three domestic F/O TRs
(overseas-stock/overseas-futures order chains remain deferred — other sessions/lanes).

## 22. Domestic account-state flip + exhaustion close-out (2026-07-01)

Plan `docs/plans/2026-07-01-003-feat-domestic-account-state-flip-exhaustion-closeout-plan.md`.
Goal: flip `t0441` (선물/옵션잔고평가) by MANUFACTURING a transient domestic F/O position
(Track A), and write the honest TERMINAL disposition for the remaining
Tracked-not-Implemented residue (Track B). Outcome: **0 flips this pass** — Track A's
live certification is operator- and window-gated and did not run autonomously; the
`fo_position_manufacture_smoke` harness (U2) and this close-out (U4) landed offline. If
the operator later runs `make live-smoke-fo-position` in an open KRX F/O window (after
the U1 feasibility probe proves flatten-in-session), `t0441` flips as a follow-up
(metadata + docgen only; see U3).

**D-honesty note: this is a TERMINAL exhaustion close-out, not a probe pass.** The raw
pool is exhausted (0 untracked TRs) and the offline tracked-flip pool is spent. As of
§21 the inventory is **320 Tracked, 282 Implemented** (docgen `reference.len` is **283**
— it counts the index page plus the implemented reference pages, so it is NOT the
residue divisor), 0 Recommended, leaving a **38-TR Tracked-not-Implemented residue**
(320 − 282). Every one of the 38 already carried a current terminal disposition from
§13–§21; this section consolidates them into one current-dated partition and records
that BOTH pools are exhausted. Only two genuine Implemented-tier levers remain, and both
are account-state-gated, not wave-blocked: `t0441` (reachable, needs a manufactured
position) and `CSPBQ00200` (needs an out-of-band spot-lane deposit). Repeated "flip
more" waves past this point re-run disposition passes that yield nothing — the honest
close-out IS the deliverable.

**Partition of the 38 (R5):** 13 `paper_incompatible` + 7 intraday paper-empty + 6
HELD-structural + 7 deferred overseas-order + 5 account-gated = 38.

**Lane A — `paper_incompatible` (13), terminal (§14/§16/§17).** The paper gateway
carries no feed / no service for these; facet `paper_incompatible: true` holds, reason
unchanged.
- *Overseas-stock, no paper feed (6):* `g3101` `g3102` `g3103` `g3104` `g3106` `g3190`.
- *KRX night-derivative quote/board, off-window paper-empty (2):* `t8455` `t8460`.
- *KRX 야간파생 order/account, `krx_extended` + `01900` (5):* `CCENT00100` `CCENT00200`
  `CCENT00300` `CCENQ10100` `CCENQ90200`.

**Lane B — intraday paper-empty (7), PENDING (§19/§20).** `t1951` `t1973` `t2212`
`t2407` `t8404` `t8427` `t2106` — all probed IN-window in the §19 open session and
recorded empty. Paper carries no data for these intraday F/O/ELW feeds *regardless of
session*, so neither a closed- nor an open-window re-probe beats an in-window empty.
Disposition unchanged.

**Lane C — HELD-structural (6), terminal by structure/scope (§6/§7/§13/§19).** The
blocker is structural, not session/funding — no operator probe can change it as-is.
- `t1852` / `t1856` — required `sFileData` screening blob (~26.8 KB) unsourced; a probe
  cannot construct a valid request.
- `t1860` — realtime-registration CONTROL, not a read.
- `t1964` — 10 unresolved filter-enum defaults; the §19 in-window read found an empty
  board even once callable.
- `t1109` — after-hours 시간외체결; every probe to date was the regular continuous
  session (wrong window). Carries a concrete reopen trigger (an after-hours run), so it
  also appears under Deferred, but stays in the HELD-structural count here.
- `t3102` — no off-hours `NWS` news frame; the feeder is scaffolded (`live_smoke_nws_t3102`),
  the flip awaits a live news event. Reopen-triggered, counted here.

**Lane D — deferred overseas-order (7), `deferred` (§18/§20/§21).** `CIDBT00100`
`CIDBT00900` `CIDBT01000` (overseas-futures orders); `COSAT00301` `COSAT00311`
`COSAT00400` `COSMT00300` (overseas-stock orders). All `owner_class: orders`; orders
reject off-window (only re-derive `01458 장종료`). The flip is an operator-run
open-**overseas**-window order smoke on the correct lane — out of this wave's identity
(the §21 domestic F/O order flip is the template; the overseas windows/lanes are the
gate). The domestic F/O order chain that shared this bucket in §20 flipped in §21.

**Lane E — account-gated (5), the only genuine remaining levers.** An operator with the
right account STATE could still move the first two; the last three are terminal absent
an external event.
- `t0441` (선물/옵션잔고평가, account) — **carry-forward PENDING, feasibility/window-gated.**
  Reachable on the funded `domestic_option` (…51) lane (returns `00000`, empty only
  because the account holds no open F/O position). This wave STAGED the manufacture path
  (`fo_position_manufacture_smoke` + `make live-smoke-fo-position` + smoke-map row) to
  flip it from a *manufactured* non-default `jqty` read (R1), but the live certification
  is operator- and window-gated (an open KRX F/O window) AND pre-gated on the U1
  feasibility probe (can a FILLED F/O paper position flatten in-session, or does it need
  an out-of-band reset?). Neither ran autonomously → 0 flips this pass. Reopen trigger:
  operator runs U1 then `make live-smoke-fo-position` in-window; on a certified
  non-empty read, flip is metadata + docgen only (`reference.len` 283→284, `banner_trs`
  +`t0441`).
- `CSPBQ00200` (현물계좌증거금률별주문가능수량, account) — **carry-forward PENDING, funding-gated**
  (§16/§20/§21). A 현물/spot read on the default `.env.domestic` lane, which carries no
  cash deposit; all deposit fields default to zero (`00136`, not a defect). No SDK path
  funds it — a paper deposit is an out-of-band operator action on the LS portal. Reopen
  trigger: the operator funds the spot lane, then a re-smoke witnesses a non-default
  deposit/orderable-quantity field.
- `o3107` (해외선물 관심종목) / `o3127` (해외선물옵션 관심종목 board) — **carry-forward PENDING,
  watchlist-gated** (§18/§20). Empty / `price=0` with no registered watchlist symbols;
  need the `overseas_option` lane + account watchlist state + an open overseas window.
- `t1631` (프로그램매매 종목별, domestic) — **permanent PENDING, gateway defect** (§19/§20).
  Gateway `IGW40014`: the server fails to serialize its OWN `bidvolume` response field
  (environmental, all-String request — NOT a request-shape `IGW40011`). No client-side
  fix; recorded in `docs/solutions/conventions/tr-pool-exhaustion-and-closure-viability.md`.

**Pool exhaustion + reopen triggers (R6).** Both the **raw pool** (0 untracked TRs — no
new REST/WS TR to track) and the **offline tracked-flip pool** (every Tracked TR whose
flip needs only offline artifacts is already Implemented) are **EXHAUSTED**. Further
Implemented-tier yield requires a CONCRETE external event, not another disposition pass:
(a) **new account state** — a manufactured/funded F/O position flips `t0441`; a funded
spot deposit flips `CSPBQ00200`; a registered overseas watchlist flips `o3107`/`o3127`;
(b) a **live `NWS` news event** flips `t3102`; (c) an **open overseas window** on the
correct lane flips the 7 deferred overseas-order TRs; (d) an **entitlement/gateway fix**
would be needed for `t1631` (server-side `IGW40014`) and the 13 `paper_incompatible`
feeds. Absent one of these, the residue is fully and currently dispositioned; no future
"flip more" wave will find offline-stageable yield.

**Count tally (R-count).** **0 flips** this pass → nothing moves. `reference.len()` stays
**283**, `banner_trs` unchanged, `maintained_tr_count` stays **320**, `cli.rs` literals,
`api_drift`, and `TRACKED_TRS` all unchanged. No `metadata/trs/*.yaml` facets edited
(every reason on file still holds). The 38-TR residue (13 + 7 + 6 + 7 + 5) is fully and
currently dispositioned; a `t0441` flip is teed up as an operator-gated follow-up
(harness staged, live certification pending an open window + the U1 verdict).

## 23. Domestic KRX-open reconfirmation & close-out (2026-07-02)

Plan `docs/plans/2026-07-02-001-chore-domestic-krx-open-reconfirm-closeout-plan.md`.
Goal: spend the open domestic KRX session capturing fresh, current-dated gate evidence
for the domestic Tracked-not-Implemented residue, then record the disposition so the
next wave stops re-probing a spent pool. **0 flips is the successful outcome** — the
deliverable is this record, not a count change.

**Honesty note: §22 is already the terminal close-out; §23's delta is narrow.** §22
consolidated the full 38-TR residue into a current terminal partition. This section
(a) isolates the **16-TR domestic slice** (38 − 13 `paper_incompatible` − 7 deferred
overseas-order − 2 overseas watchlist `o3107`/`o3127` = 16, all
`paper_incompatible: false`), (b) re-partitions it by blocker class and reconciles that
finer partition to §22's lanes, and (c) reserves fresh raw-probe slots for the only two
current-probeable candidates. Overseas is closed this session, which keeps the CIDBT
order chain and the watchlist reads out of reach; no order placement or position
manufacture runs this wave.

**U1 probe status (KTD3 fail-open).** The two live raw-probes (`t0441`, `CSPBQ00200`)
are operator-run and credential-gated; they did **not** run in the autonomous pass that
authored this section, so this close-out stands documentation-only on the §16–§22
evidence cited per row below. Both are session-independent account-state reads — the
open session is opportunistic, not required — so the probes remain runnable at any time:
- `t0441` — `make raw-probe LS_SMOKE_LANE=domestic_option LS_PROBE_TR_CD=t0441
  LS_PROBE_PATH=/futureoption/accno
  LS_PROBE_BODY='{"t0441InBlock":{"cts_expcode":"","cts_medocd":""}}'` — expected: a
  success `rsp_cd` with a small `body_len` (empty/all-default balance on the flat …51
  account) → filed as **position-gated** reconfirmation.
- `CSPBQ00200` — `make raw-probe LS_PROBE_TR_CD=CSPBQ00200 LS_PROBE_PATH=/stock/accno
  LS_PROBE_BODY='{"CSPBQ00200InBlock1":{"RecCnt":1,"BnsTpCode":"1","IsuNo":"KR7005930003","OrdPrc":75000,"RegCommdaCode":"41"}}'`
  (`RecCnt`/`OrdPrc` are JSON **numbers** — string slots return `IGW40011`) — expected:
  `00136` with all-default deposit fields on the cash-only default lane → filed as
  **funding-gated** reconfirmation. Body source of truth: the proven-live SDK request
  struct (`CSPBQ00200InBlock1`, `crates/ls-sdk/src/account/capacity.rs` — the shape the
  §16 live smoke certified with `00136`), which is a SUPERSET of the normalized
  baseline's request block: the baseline under-reports `RecCnt`/`RegCommdaCode` for this
  TR, so mirror the SDK struct, not the baseline alone, when re-deriving this body.

A run records ONLY the `http` / `rsp_cd` / `body_len` triple plus the gate label in the
matching row below — never response-body contents or account identifiers. A probe that
instead returns unexpectedly populated data for its modeled fields is a **re-open
candidate** that exits this wave's 0-flip scope: hand it to a separate certify-flip
decision; never flip inline (AE3).

**The 16-TR domestic partition (R4), reconciled to §22's lanes** — §22 Lane B (7) +
Lane C (6) + Lane E's 3 domestic entries (`t0441`, `CSPBQ00200`, `t1631`) = 16; Lane E's
other 2 (`o3107`/`o3127`) are overseas-watchlist, outside the domestic slice:

- **Current-probeable account reads (2) — §22 Lane E.**
  - `t0441` (선물/옵션잔고평가) — **position-gated** (§16/§20/§22). Reachable on the funded
    `domestic_option` (…51) lane; returns success with an empty balance because the
    account holds no open F/O position. Probe this wave: not run (operator-gated; see
    U1 status above). The manufacture path is fully staged
    (`make live-smoke-fo-position`, §22 Lane E); no order placement this wave (declined).
  - `CSPBQ00200` (증거금률별주문가능수량) — **funding-gated** (§16/§20/§21/§22). Spot read on
    the cash-only default lane; every capacity field defaults to zero (`00136`, not a
    defect). Probe this wave: not run (operator-gated; see U1 status above). No SDK
    path funds it — a paper deposit is an out-of-band LS-portal action.
- **After-hours-gated (1) — §22 Lane C.**
  - `t1109` (시간외체결) — **session-gated** (§19/§20). Needs the 15:30–17:50 KST
    after-hours session; deliberately NOT probed this wave — a regular-session probe
    can only re-derive the §19/§20 wrong-window finding and adds no fresh evidence.
- **Intraday paper-empty (7) — §22 Lane B.** `t1951` `t1973` `t2106` `t2212` `t2407`
  `t8404` `t8427` — all probed IN-window in the §19 open session and recorded empty;
  paper carries no data for these intraday F/O/ELW feeds regardless of session, so no
  re-probe (open or closed) beats an in-window empty. Not re-probed; §19 cited.
- **Structurally held (5) — §22 Lane C minus `t1109`.** Blockers are structural, not
  session/funding; not re-probed, §20 (and priors) cited.
  - `t1852` / `t1856` — required `sFileData` screening blob (~26.8 KB) unsourced; no
    valid request is constructible.
  - `t1860` — realtime-registration CONTROL, not a read.
  - `t1964` — 10 unresolved filter-enum defaults; §19's in-window read found an empty
    board even once callable.
  - `t3102` — no off-hours `NWS` news frame; feeder scaffolded
    (`live_smoke_nws_t3102`), flip awaits a live news event.
- **Gateway defect (1) — §22 Lane E.**
  - `t1631` (프로그램매매 종목별) — **permanent PENDING, gateway-side `IGW40014`**
    (§19/§20/§22): the server fails to serialize its own `bidvolume` response field
    (environmental; all-String request, NOT a request-shape `IGW40011`). No client-side
    fix exists; not re-probed.

**Supersession (R5).** For these 16, this section is now the current disposition
record — the per-TR reasons in §16–§22 are refined in place by the rows above, not
stacked under a parallel resolution layer. All 16 keep `implemented: false` with their
gate reason pointing here; no `metadata/trs/*.yaml` facet is edited.

**Reopen triggers (mirrors §22 R6).** The residue moves only on a CONCRETE external
event, never on another disposition pass: (a) **position manufacture** — operator runs
the §22 U1 feasibility probe then `make live-smoke-fo-position` in an open KRX F/O
window → `t0441` flips (metadata + docgen only); (b) an **out-of-band spot-lane
deposit** + re-smoke (`make live-smoke-cspbq00200`) witnessing a non-default capacity
field → `CSPBQ00200` flips;
(c) an **after-hours (15:30–17:50 KST) run** (`make live-smoke-t1109`) → `t1109` flips
or is re-dispositioned on fresh in-window evidence; (d) a **live `NWS` news event** → `t3102` flips via the
staged feeder; (e) a **gateway-side `IGW40014` fix** reopens `t1631`; (f) the
`sFileData`-sourcing / realtime-design / filter-enum levers for `t1852`/`t1856`/
`t1860`/`t1964` are design-scoped, not session-scoped; (g) an **open overseas window**
gates the out-of-scope overseas residue (§22 Lanes D/E). Absent one of these, the
domestic read residue is fully dispositioned and unmovable.

**Count tally (R6).** **0 flips** this pass → nothing moves. `reference.len()` stays
**283**, `banner_trs` unchanged, `maintained_tr_count` stays **320**, `cli.rs` literals,
`api_drift`, and `TRACKED_TRS` all unchanged; no `metadata/trs/*.yaml` `implemented`
facet edited. The only tree change is this section's prose. The 16-TR domestic residue
(2 + 1 + 7 + 5 + 1) is fully and currently dispositioned against §22's 38 (13 + 7 + 6 +
7 + 5); the next domestic window should be spent on a reopen trigger above, not on
another re-probe of this pool.

## 24. Domestic trigger-run certify wave — armed-trigger prep + smoke-target defect fix (2026-07-02)

Plan `docs/plans/2026-07-02-002-feat-domestic-trigger-run-certify-wave-plan.md`.
Goal: spend an open KRX F/O window executing the two §23 reopen triggers that are armed
without out-of-band action — position manufacture → `t0441`, and the live-`NWS` listener
→ `t3102` — for 1–2 domestic flips, refreshing the account-gated PENDING evidence either
way. **Outcome: 0 flips this pass** — every certifying leg (the raw-probes, the typed
`t0441`/`CSPBQ00200` read smokes, `make live-smoke-fo-position`, and the `NWS` listener
loop) is credential-gated and operator-run in an attended PTY (never autonomous, per the
plan's execution profile); none ran in the autonomous pass that authored this section.
The deliverable is this record plus a real smoke-harness defect fix (below); the flips
remain armed for the operator.

**Honesty note: the certify legs are operator-pending, but this wave is not empty.** §22
is the terminal exhaustion close-out and §23 re-confirmed the 16-TR domestic slice; the
account state that produced the current PENDINGs is unchanged (no probe ran to move it),
so the §23 partition below is re-confirmed verbatim. What *did* land is desk prep (U1)
and a latent-defect correction: three wave-critical `make live-smoke-*` targets matched
**zero** tests and would have exited FAIL before touching the gateway.

**Smoke-target defect fix (U1, KTD9).** The `live_smoke` test binary was decomposed into
per-family submodules (`account::`, `market_session_charts::`, …), so the `run_smoke`
recipe's `--exact` filter — which takes the full `module::test` path — no longer matched
the bare test names still hard-coded in most targets. A bare name silently matches 0
tests while the `1 passed` grep reads the empty run as failure, so every affected target
was dead. This wave fixed the **three wave-critical** call sites to their real paths:
- `live-smoke-t0441` → `account::live_smoke_t0441`
- `live-smoke-cspbq00200` → `account::live_smoke_cspbq00200`
- `live-smoke-nws-t3102` → `market_session_charts::live_smoke_nws_t3102`
Also amended the stale `live-smoke-fo-position` gating comment (KTD1): the harness's own
preflight flat-gate + bounded flatten + kill-switch-after-teardown machinery IS the
flatten-feasibility gate — the separate hand-run feasibility spike (plan 2026-07-01-003
U1) is superseded. **Repo-wide follow-up (flagged, not swept):** cross-checking all 198
`run_smoke` call sites against the decomposed `--list` output, **194 remain broken** by
the same bare-name-vs-`module::` mismatch. A separate mechanical sweep PR should repoint
every target from its `--list` path; out of scope for this certify wave.

**U2 probe/branch status (operator-pending, KTD2/KTD3).** The two session-independent
raw-probes remain runnable at any time and record ONLY the `http`/`rsp_cd`/`body_len`
triple plus the gate label — never response bodies or account identifiers. The `t0441`
flat/positioned branch does **not** ride the probe's `body_len`; the operator runs the
typed read and the reported `positions=` count decides (KTD2): `positions=0` before the
15:15 KST cutoff → flat, proceed to manufacture; `positions>0` with `tappamt_nondefault`
→ that read is itself the R10 certifying witness, manufacture skipped. Commands (verbatim
from §23):
- `t0441` — `make raw-probe LS_SMOKE_LANE=domestic_option LS_PROBE_TR_CD=t0441
  LS_PROBE_PATH=/futureoption/accno
  LS_PROBE_BODY='{"t0441InBlock":{"cts_expcode":"","cts_medocd":""}}'` (the
  `LS_SMOKE_LANE=domestic_option` is mandatory — `raw-probe` has no lane mapping and
  silently authenticates as the domestic cash account …3701 without it), then the typed
  `make live-smoke-t0441` read for the branch decision.
- `CSPBQ00200` — `make raw-probe LS_PROBE_TR_CD=CSPBQ00200 LS_PROBE_PATH=/stock/accno
  LS_PROBE_BODY='{"CSPBQ00200InBlock1":{"RecCnt":1,"BnsTpCode":"1","IsuNo":"KR7005930003","OrdPrc":75000,"RegCommdaCode":"41"}}'`
  (`RecCnt`/`OrdPrc` are JSON **numbers** — string slots return `IGW40011`; body mirrors
  the certified SDK struct `CSPBQ00200InBlock1`, a superset of the under-reporting
  normalized baseline). Expected `00136` all-default → funding-gated PENDING re-confirmed;
  unexpectedly populated → operator runs `make live-smoke-cspbq00200`, and per KTD8 (R2)
  a non-default capacity flag flips it inline — this wave's Product Contract supersedes
  §23's "never flip inline (AE3)" prohibition. KTD8 did **not** fire this pass (no probe
  ran).

**Certify legs (operator-run, staged and armed).**
- `t0441` (선물/옵션잔고평가) — flat branch: operator mints a fresh `LS_ORDER_SMOKE_NONCE`
  and runs `make live-smoke-fo-position` (domestic_option lane, …51) in the open F/O
  window before 15:15 KST; flips only on the `ORDER-MANUFACTURE-FO result=certified`
  witness line (KTD3), never the make exit status. Fail-closed arms (non-flat preflight,
  degenerate band, rejected buy, no-fill clean-cancel, panic) → PENDING with the arm
  recorded; a panic *without* a preceding `flatten=confirmed` (or carrying `MANUAL
  flatten required`) is the stranded arm that halts all order legs (KTD4). Not run.
- `t3102` (뉴스본문) — operator runs `LS_NWS_SMOKE_SECS=1800 make live-smoke-nws-t3102`
  (default domestic lane, …3701) as a looped long timebox; flips only on a `LIVE-SMOKE`
  record with non-empty `title_len`. HELD re-confirmed absent a live news frame. Not run.

**The 16-TR domestic partition — re-confirmed from §23 (no probe moved it):** 2
current-probeable account reads (`t0441`, `CSPBQ00200`) + 1 after-hours-gated (`t1109`) +
7 intraday paper-empty (`t1951` `t1973` `t2106` `t2212` `t2407` `t8404` `t8427`) + 5
structurally held (`t1852` `t1856` `t1860` `t1964` `t3102`) + 1 gateway defect (`t1631`,
`IGW40014`) = 16. Every row keeps `implemented: false` with its §16–§23 reason intact; no
`metadata/trs/*.yaml` facet edited this pass.

**Supersession (R5).** For these 16 this section is the current disposition record,
refining §23's reasons in place (not stacking a parallel layer). The reopen triggers of
§23 carry forward unchanged: (a) position manufacture → `t0441`; (b) out-of-band spot
deposit + re-smoke → `CSPBQ00200`; (c) after-hours run → `t1109`; (d) live `NWS` event →
`t3102`; (e) gateway `IGW40014` fix → `t1631`; (f) design-scoped levers for
`t1852`/`t1856`/`t1860`/`t1964`; (g) open overseas window → the overseas residue. The two
armed-this-window triggers (a) and (d) stay armed for the operator; the harnesses are
staged on `main` and now reachable via the fixed Makefile targets.

**Count tally (R6).** **0 flips** → nothing moves. `reference.len()` stays **283**,
`banner_trs` unchanged, `maintained_tr_count` stays **320**, the four `cli.rs` literals,
`api_drift`, and `TRACKED_TRS` all unchanged; no `metadata/trs/*.yaml` `implemented`
facet edited. The only tree changes this wave are the three Makefile target fixes, the
one Makefile gating-comment amendment, and this section's prose. The 16-TR domestic
residue remains fully and currently dispositioned; the next domestic window should still
be spent on an armed reopen trigger, now that the targets that run them are un-broken.

## 25. Recommended re-certification wave — gate-mechanism armed, all 10 HELD (2026-07-03)

Plan `docs/plans/2026-07-03-003-feat-recommended-recert-wave-plan.md`. Goal: re-certify the
ten TRs the error-resilience gate (§24 context / PR #83, plan 2026-07-01-004 R12) demoted
to Implemented and restore the Recommended tier from its current count of **0**. Each TR
re-promotes only after the full gate — grounded `metadata/constraints/<tr>.yaml`, a live
differential negative probe (or the realtime error-coverage substitute, KTD2), captured
error-coverage evidence, and the gate-extended promote-tr recipe.

**Outcome: 0 flips this pass; the gate MECHANISM is now fully authored and armed.** The
offline units (U1, U2) landed gate-green; every live leg (U3–U5, U7) is HELD because its
session/attendance prerequisite was unavailable in this autonomous run. The Recommended
count stays **0**; the KTD5 one-time re-wirings deliberately did NOT land (they land "with
the first flip", and there was none) — `recommended_no_banner` stays `[&str; 0]`
(`crates/ls-docgen/src/lib.rs`) and `freshness_check_over_empty_recommended_set_exits_zero`
(`crates/ls-trackers/src/cli.rs`) stays asserting count 0. No `metadata/trs/*.yaml`,
`banner_trs`, `reference.len()`, or freshness-count site was edited.

**What landed (U1 + U2, offline, gate-green — DoD-complete regardless of live outcomes):**
- **U1 — 9 grounded constraint schemas** (`metadata/constraints/{token,t1101,t1102,S3_,CSPAQ12200,CSPAT00601,CSPAT00701,CSPAT00801,t0425}.yaml`),
  joining the pre-existing `t8412.yaml`. Every field carries `type` + `required` +
  explicit `enum`/`range`/`format` applicability; each grounds against its normalized
  baseline (`cargo test -p ls-core` green). **Order-TR grounding caveat (the struct wins):**
  `CSPAT00601.LoanDt`, `t0425.expcode`, and `t0425.cts_ordno` are declared caller-optional
  (`required: false`) even though the wire marks them required — the certified request
  structs send them empty (cash-order `LoanDt`, all-symbols `expcode`, first-page
  `cts_ordno`), and preflight runs at the single `Inner` dispatch seam for orders too, so
  a `required: true` there would false-reject every certified order. An ls-core order-mechanics
  unit helper (`inner.rs::order_policy`) was repointed off the literal `"CSPAT00601"` to a
  schema-less `"ORDER_TEST"` so its deliberately-empty synthetic bodies still exercise
  post-preflight classification rather than tripping the new schema.
- **U2 — 8 negative-probe legs + Makefile targets + smoke-map rows** (`crates/ls-sdk/tests/negative_probe.rs`,
  `Makefile`, `.agents/skills/promote-tr/references/smoke-map.md`). Four read legs
  (`t1101`/`t1102`/`CSPAQ12200`/`t0425`) mirror the `t8412` differential leg; the `token`
  leg is a bespoke `/oauth2/token` FORM probe mutating only the non-credential `grant_type`/`scope`
  fields (credential class by removal only, KTD2); the three order legs (`CSPAT00601`/`00701`/`00801`)
  place a real band-safe resting control, cancel + flat-verify it before any variant, fire
  only type/required variants (KTD3), and halt-may-rest on any order-endpoint transport
  failure — behind the full fail-closed autonomy chain copied from `order_smoke.rs`
  (`LS_TRADING_ENV=paper` + `LS_ORDER_SMOKE=1` + no CI/TTY + fresh `LS_ORDER_SMOKE_NONCE`).
  Offline twins assert variant-generation determinism for all 8 schemas and that the order
  legs fire type/required only; all live legs stay `#[ignore]`
  (`cargo test -p ls-sdk --test negative_probe` green, `make lane-check` green).

**Why every live leg is HELD (session/attendance, KTD4/KTD6).** This wave executed
autonomously at **2026-07-03 15:48 KST** — 18 minutes after the KRX regular close (15:30),
with no attended TTY. Promotion to `recommended: true` is an outward-facing support-tier
claim the wave gates on an operator witnessing the control + differential-probe terminal
table in-session; an unattended post-close run cannot supply that witness (and for the
order quartet, the autonomy chain refuses by construction). Per-TR terminal state +
armed reopen trigger:

- **`t8412`** — HELD. Its control (`live-smoke-chart`, historical `LS_LIVE_SMOKE_T8412_DATE`)
  is closure-safe, but its differential probe (`make live-smoke-t8412-negative`) is
  smoke-map-gated "open session + valid seed", which the closed window blocks. *Reopen:*
  next open KRX session — run the chart control + the negative probe, then promote.
- **`CSPAQ12200`** — HELD. Closure-viable read; not run autonomously (outward-facing flip
  needs an attended witness). *Reopen:* attended session — `make live-smoke-account` control
  + `make live-smoke-cspaq12200-negative`, then promote.
- **`S3_`** — HELD. Closure-viable WS-lifecycle reachability; not run autonomously (same
  reason). No live negative probe (KTD2: realtime excludes — trade-data correctness,
  in-session delivery, reconnection — recorded in its error-coverage file at promotion).
  *Reopen:* attended session — `make live-smoke-ws` lifecycle, then promote.
- **`token`, `t1102`** — HELD (session-closed). One open-session `make live-smoke` run issues
  the shared control for both; the `token` bespoke negative leg runs LAST among live legs
  (a token-flow probe can disturb the session token, KTD2). *Reopen:* next open KRX session.
- **`t1101`** — HELD (session-closed). Control is `make live-smoke-book`. *Reopen:* next open
  KRX session.
- **`CSPAT00601`, `CSPAT00701`, `CSPAT00801`, `t0425`** — HELD (window closed + unattended).
  The order quartet needs an open KRX window (09:00–15:30 KST) AND the order legs' fail-closed
  autonomy chain refuses without an attended TTY + a fresh human `LS_ORDER_SMOKE_NONCE`
  (covers plan AE2: a window miss leaves the TR Implemented with a HELD record and the wave
  still closes). *Reopen:* attended in-window session with the order-capable account —
  `LS_ORDER_SMOKE=1 LS_ORDER_SMOKE_NONCE=$(date +%s)` then the order-chain / matrix control
  smokes + the three `make live-smoke-cspat00{6,7,8}01-negative` probes, then promote the
  quartet from the clean chain.
- **`t1109`** (U7, opportunistic, NOT one of the ten) — skipped; the after-hours window was
  not entered this pass. Skipping does not affect the DoD. *Reopen:* run the §23/§24-recorded
  after-hours `t1109` command; flip to Implemented only on a non-empty typed witness.

**Supersession (R5).** For the ten re-cert TRs this section is the current disposition
record; §24's "current Recommended count is 0" holds, now with the re-cert gate mechanism
authored and armed rather than merely staged in plan. The tier restores on the operator's
next attended open KRX window (reads + token) and attended in-window order session (the
quartet); every harness is on-branch and reachable via the Makefile targets above.

## 26. Re-cert wave LIVE execution — Recommended tier restored to 3 (2026-07-06)

Plan `docs/plans/2026-07-06-001-feat-recert-wave-order-probe-gap-close-plan.md`. Executed the
§25 armed gate live in an **attended open-KRX session (Mon 2026-07-06, regular window)**: the
six read controls + differential negative probes agent-run and operator-witnessed; the order
quartet operator-run in their own terminal (the fail-closed autonomy chain refuses a non-TTY
sandbox by construction). U1–U3 shipped first as offline harness hardening (the three
order-probe gap fixes; commit `ee94b4c`). **This section is the current disposition record for
the ten re-cert TRs and supersedes §25 (R5).**

**Outcome: the Recommended tier is restored from 0 to 3.** Three reads promoted on clean live
differential chains; the remaining seven stayed HELD (fail-closed, §25 AE2 / KTD4). The KTD5
one-time re-wirings landed with the first flip (`recommended_no_banner` `[&str;0]→3`, freshness
count assertion `0→3`, `EVIDENCE-FRESHNESS.md` `0→3`).

**PROMOTED to `recommended: true` (clean differential chain; commit `943a081`):**
- **`t1101`** — control `price=307500` + 10-level book; probe `shcode/required` + `shcode/format`
  → Clean. Owner class `market_session`.
- **`token`** — shared control `token_len=380`; the bespoke OAuth-form probe's 7 variants all
  `http=403` → Clean. Owner class `standalone` (auth).
- **`S3_`** — WS subscribe lifecycle `row received`; **no live differential** (realtime is
  NOT-OBSERVABLE, KTD2), so the realtime error-coverage substitute is recorded and the
  recommendation excludes trade-data correctness / in-session delivery / reconnection. Owner
  class `realtime`.

**HELD (left Implemented with a recorded arm):**
- **`t1102`** — **DIVERGENT**. `shcode/required` and `exchgubun/required` both → `rsp_cd=00000`
  (the gateway accepts the request with a required field removed). The constraint schema
  over-claims `required` relative to gateway reality; promotion blocked until the schema is
  reconciled (out of scope — Scope Boundaries). *Reopen:* reconcile `constraints/t1102.yaml`
  against observed gateway tolerance, re-probe, promote.
- **`t8412`** — **DIVERGENT**. `shcode/required`, `sdate/format`, `edate/format` → `00000` (the
  gateway tolerates a removed symbol and malformed dates). Same reconcile-then-promote reopen.
- **`CSPAQ12200`** — HELD (thin evidence). Its sole variant `BalCreTp/required` returned
  `IGW00201` (a self-inflicted Account-bucket throttle) on both runs — never a crisp
  merits-rejection, so the differential contract was not actually exercised. Operator declined
  to promote an outward-facing tier claim on throttle-only evidence. *Reopen:* re-probe with the
  Account bucket cool so the variant is genuinely evaluated.
- **`t0425`** — **DIVERGENT**. `chegb/required` → `00000` (the gateway accepts the working-order
  read with `chegb` removed). It is a READ (`is_order:false`), unaffected by the U1–U3 order
  harness. Reconcile-then-promote reopen.
- **`CSPAT00601` / `CSPAT00701` / `CSPAT00801`** — HELD (order-probe pagination, fail-closed;
  **no order placed by the probe**). The `live-smoke-order-chain` CONTROL certified the full
  happy-path chain (submit `00040` ordno=20719 / modify `00462` ordno=20720 / cancel `00463`
  ordno=20721, `flat=confirmed [zero live rows]`), but the required hardened DIFFERENTIAL probe
  HELD at pre-assert-flat: the gap-(b) fill-inclusive `chegb="0"` scan returns 005930's entire
  accumulated order history and paginates (`tr_cont=0`), and the single-page guard fail-closes.
  Confirmed by a `make raw-probe` A/B (`chegb="2"` body_len=63 vs `chegb="0"` body_len=1186).
  This is a real defect in the gap-(b) fix surfaced live — see
  `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md`.
  *Reopen:* decouple fill-detection (ordno-targeted lookup) from the single-page flatness scan,
  then re-run the hardened probe attended in-window; the order-chain control already certifies
  the happy path.

**Gap-close (R1) outcome.** (a) reorder — landed and witnessed indirectly (the probe reaches
pre-assert-flat before firing variants; the resting-control variant lines could not print
because pre-assert-flat HELD first). (b) fill-inclusive scan — landed offline-green but its
`chegb="0"` widening is the direct cause of the order-probe pagination HELD above; the fix is
net-incomplete for the order probe and carries a documented follow-up. (c) pre-assert-flat +
unconditional teardown — landed and **fired correctly live** (it is exactly what fail-closed on
the paginated scan, refusing to place). The smoke-map (rows 66–68) was reconciled to the
hardened sequence.

**Count tally (R6).** **3 flips.** `recommended` count 0→3 (`t1101`/`token`/`S3_`);
`recommended_no_banner` now `["token","t1101","S3_"]`; the freshness-count assertion and
`EVIDENCE-FRESHNESS.md` reflect 3; the `slice_metadata` tripwire tightened from "empty" to
"exactly {S3_,t1101,token}". `reference.len()` unchanged (promoted TRs stay implemented). No
`banner_trs` entry for the seven HELD TRs was removed. The full root gate is green.

## 27. Re-cert wave 2 — reopen the 7 §26-HELD TRs; live re-probe (2026-07-06)

Plan `docs/plans/2026-07-06-002-feat-recert-wave-reopen-held-trs-plan.md`. The offline hardening
(U1–U7) landed first as PR #99 (squash `dad2c2e`): the order-probe `chegb="0"`→`"2"` revert +
bounded ordno fill-check + owned-only teardown; the per-class `gateway_tolerant` facet
(preflight unchanged, KTD3; probe downgrades `Divergent`→`expected-tolerant`, KTD4); and the
CSPAQ12200/t0425 Account-bucket pacing. The seven reopened TRs were then re-probed **attended,
open-KRX (Mon 2026-07-06, regular window)**. **This section is the current disposition record for
the seven and supersedes §26's reopen arms.**

**Outcome: 1 CLEAN (t1102), 6 HELD — but for THREE new reasons, each a real finding, not a
session artifact. Two mechanisms shipped in #99 are validated live: the `gateway_tolerant`
downgrade fires correctly (t1102/t0425 tolerant pairs read `expected-tolerant`), and the
CSPAQ12200 pacing works (a merits response, no more `IGW00201`).**

**CLEAN (promotable):**
- **`t1102`** — control `rsp_cd=00000`; `shcode/required` + `exchgubun/required` → `00000`
  accepted, **downgraded to `expected-tolerant`** (the facet works); `shcode/format` → `IGW40011`
  rejected → Clean. A fully clean/tolerant chain. Promotion staged (see Follow-up).

**HELD — reason A: probe throttle-masked (not evaluated):**
- **`t8412`** — control `00000`, but **all 11 variants → `IGW00201`** (Account/market-data bucket
  throttle). The U6 pacing was added only to the shared account-lane loop
  (`run_inblock_negative_probe`); t8412 runs its **own** standalone loop (it carries the tolerant
  pairs and does not route through the shared helper), which fires ~12 rapid market-data calls
  **unpaced** → every variant throttled. A throttle classifies as a (non-success) rejection →
  `Clean`, so the "all Clean" here is FALSE — the differential was never exercised on merits.
  *Reopen:* pace the t8412 standalone loop (mirror U6), re-probe with the bucket cool.

**HELD — reason B: new gateway-tolerant `(field, class)` pair, unmarked (a schema-reconcile
decision — the plan says handle a newly-observed tolerant pair when the live probe surfaces it,
not pre-mark):**
- **`t0425`** — `chegb/required` → `expected-tolerant` (the §26-marked pair works); but
  **`medosu/required` → `00000` accepted = `Divergent`** (unmarked). `chegb/enum` + `medosu/enum`
  → `IGW40011` rejected → Clean; `sortgb/required` → `IGW40013` rejected → Clean. The single
  `medosu` divergence blocks promotion. *Reopen:* decide `medosu/required` — mark
  `gateway_tolerant:[required]` (consistent with the `chegb`/`exchgubun` precedent, a
  gateway-defaulted filter field kept stricter as a caller contract) or correct to
  `required:false`; then re-probe.
- **`CSPAQ12200`** — **the pacing fix WORKED**: the sole `BalCreTp/required` variant returned a
  merits `rsp_cd=00136` (not `IGW00201`) — AE5 satisfied, U6/R12 validated live. But that merits
  response is an **acceptance** (`00136`) of the removed field = **`Divergent`** (unmarked).
  *Reopen:* same decision as `medosu` for `BalCreTp/required`.

**HELD — reason C: order-quartet single-page guard bug (a real defect, DISTINCT from §26's
`chegb="0"` pagination; SAFETY: a stranded control order was left resting):**
- **`CSPAT00601`** — control **placed** (band-floor resting buy, `ok=true resting`), `IsuNo/required`
  → `01407` Clean, `OrdQty/type` → `IGW40011` may-rest halt; then the teardown scan **failed on
  pagination** (`tr_cont=0`) → the control could NOT be canceled → **it is stranded, resting on
  005930**.
- **`CSPAT00701` / `CSPAT00801`** — HELD at pre-assert-flat, same `tr_cont=0` pagination (they see
  the stranded 00601 control) → no placement.
- **Root cause.** `scan_symbol_working_orders` gates single-page-ness on the **`tr_cont` HEADER**,
  but t0425 self-paginates on the **`cts_ordno` BODY cursor** (`tr_cont` "rides defensively",
  per `orders/mod.rs`). The gateway returns `tr_cont="0"` on ANY non-empty page (not a real
  continuation), so the guard (`!empty && !"N"` = paginated) fail-closes on any non-empty book.
  The `chegb="2"` revert correctly shrank the row COUNT, but the guard trips on the HEADER, which
  is orthogonal to actual pagination. It "worked" in §26 only because the book was empty
  (`tr_cont="N"`); the instant the probe places its own control, every later scan sees data →
  `tr_cont="0"` → fail. *Reopen:* gate the single-page check on the response `cts_ordno` cursor
  (empty/`" "`/all-default = terminal), not the `tr_cont` header — in BOTH
  `negative_probe.rs::scan_symbol_working_orders` and the twin in `order_smoke.rs`. **SAFETY:
  manually cancel the stranded 005930 band-floor buy in the paper account before re-running** (it
  is non-marketable, so it cannot fill, but it blocks every subsequent order-probe scan).

**Count tally.** **0 flips in this entry** — no promotion executed yet; `recommended` stays 3
(`t1101`/`token`/`S3_`), count sites unchanged. `t1102` is CLEAN-certified this session; its
promotion + the three reopen fixes above are the staged follow-up.

**Follow-up (staged, one PR):** (1) promote `t1102` (clean); (2) after an operator decision, mark
`t0425 medosu/required` + `CSPAQ12200 BalCreTp/required` `gateway_tolerant:[required]` (or
`required:false`) → re-probe → promote what certifies; (3) pace the t8412 standalone loop; (4)
fix the `tr_cont`→`cts_ordno` single-page guard for the order quartet; (5) clear the stranded
005930 order. (2)–(4) each need an attended in-window re-probe before their flip.

**Addendum (2026-07-12, recert-wave-3 offline prep — plan `2026-07-12-003`).** Two of the reason-C
/ reason-A follow-ups above are RESOLVED in code; only their attended live re-probe remains, so an
operator scoping the Monday KRX-open window (issue #117) should NOT re-do them:

- **Follow-up (4) — the `tr_cont`→`cts_ordno` single-page guard fix (reason C) LANDED in PR #106
  (`a9974a9`, 2026-07-07).** See §28 U1: both `scan_symbol_working_orders` twins
  (`crates/ls-sdk/tests/negative_probe.rs` + the `crates/ls-sdk/tests/order_smoke.rs` twin) now gate
  single-page terminality on the response **`cts_ordno` body cursor** via the pure
  `scan_page_is_terminal(cts_ordno)` (terminal on empty / `" "` / numeric-default `"0"`; a real
  order-number continuation cursor = paginated → fail-closed), not the `tr_cont` header. The offline
  terminality twins (`scan_page_terminality_keys_on_the_cts_ordno_body_cursor_not_tr_cont`) are
  present and green in both files. The remaining reason-C work is the **attended Monday live
  re-probe** of `CSPAT00601`/`00701`/`00801` — and, per §29, that re-probe already ran once
  (2026-07-07) and UNMASKED a *separate* constraint-schema required-ness divergence (`BnsTpCode` /
  `IsuNo`), which is §29's follow-up, not a guard defect.
- **Follow-up (3) — the t8412 standalone-loop pacing (reason A) LANDED offline in this wave.**
  `live_smoke_t8412_negative` now delegates to the shared U6-paced `run_inblock_negative_probe`
  (`T8412_PROBE_PACE = 250 ms`, non-zero market-data-sized), so it no longer self-inflicts the
  `IGW00201` throttle that masked all 11 variants as a false `Clean`. Offline proof
  `t8412_probe_is_paced` asserts the non-zero pace; the true anti-throttle behavior is confirmed by
  the Monday in-window re-probe (the leg is `#[ignore]`, so offline-green = landed-but-UNCERTIFIED).
- **Follow-up (5) — the stranded 005930 band-floor buy was already cleared** (§28 U2, 2026-07-07:
  a `chegb="2"` t0425 scan returned the empty/flat signature, zero owned resting rows).

Net: of §27's reason-A/-C follow-ups, only the **attended live re-probes** (3)+(4) remain, plus the
independent §29 required-ness divergence. Follow-ups (1) `t1102` promotion and (2) the
`medosu`/`BalCreTp` operator decision are unchanged and still open.

## 28. Nautilus open-window SC certification wave — SC CERTIFIED live; U6 authorized (2026-07-07)

Plan `docs/plans/2026-07-07-001-feat-nautilus-open-window-sc-certify-wave-plan.md`. A mixed
offline/live wave: settle whether the paper gateway delivers SC push-fill frames and tolerates
the exec client's second WS session, switch SC to the primary fill source if it certifies, and
fold in the two prerequisites (§27 item 4 guard fix, item 5 stranded-order clear). **Executed
end-to-end this session in an attended open-KRX window (2026-07-07): the offline half (U1, U4, U5)
landed and gated green, and the live probe (U2/U3) ran attended and returned a full CERTIFIED
verdict — SC push-fills are delivered, the 2nd WS session is tolerated, and the same fill via both
the SC1 frame and the t0425 poll collapsed to exactly one `FillDelta` — which authorizes U6.**

**LANDED offline (this session):**
- **U1 — the `tr_cont`→`cts_ordno` single-page guard fix (§27 reason C / item 4).** Both probe
  scans (`negative_probe.rs::scan_symbol_working_orders` and its `order_smoke.rs` twin) now gate
  single-page terminality on the response **`cts_ordno` body cursor** (empty / `" "` /
  numeric-default `"0"` = terminal; a real order-number continuation cursor = paginated →
  fail-closed), not the `tr_cont` header (which reads `"0"` on any non-empty page). Extracted a
  pure `scan_page_is_terminal(cts_ordno)` fn in each file, mirroring the nautilus runtime's
  proven `execution.rs` predicate, with a new offline unit test each. `cargo test -p ls-sdk` green;
  the cross-workspace adapter gate green (KTD-6). This unblocks the §27 order-quartet re-cert —
  once its attended live re-probe runs, an order probe can now scan a book that already holds its
  own control row without a false pagination HELD.
- **U4 — off-by-default SC-primary cadence mechanism (KTD-3/4/5).** New pure
  `resolve_poll_cadence(sc_primary)` selector in `execution.rs`: OFF (default) → the 2s
  `DEFAULT_POLL_CADENCE` (poll authoritative, byte-identical to today); ON →
  `SC_PRIMARY_BACKSTOP_CADENCE` (15s), demoting the poll to a fail-closed reconcile backstop while
  SC carries fills. Wired to `LS_NODE_SC_PRIMARY=1` (exact "1") in `node_exec_tester`, applied via
  the existing `with_poll_cadence` hook. **KTD-4 resolved by bounding the cadence:** the poll loop
  consumes `reconcile_armed` only after `sleep(cadence)`, so the cadence *is* the worst-case
  dropped-SC-fill detection latency — the 15s backstop is held below a new
  `SC_FILL_DETECTION_CEILING` (30s, < one 1-minute bar) by an offline invariant test. Five new
  offline tests + the cadence-independent exactly-once dedup (AE1) assertion. Poll loop is never
  disabled, only slowed. Ships unconditionally; live activation is U6.
- **U5 — README corrections.** `adapters/nautilus/README.md`: replaced the stale "a v-next SDK
  follow-up adds `cheprice`" with the accurate "`cheprice` wired end-to-end today, consumed with a
  limit-price `price_approximated` fallback"; corrected the SC lane from "subordinate/un-deduped"
  to "already flows through the one exactly-once ledger seam (AE1); poll authoritative today,
  relaxes to the SC-primary backstop when the operator certifies SC."

**LIVE EXECUTED (attended, open KRX 2026-07-07, `LS_TRADING_ENV=paper`, `.env.domestic`):**
- **U2 — flat confirmed.** The §27 stranded 005930 buy was already cleared; a `chegb="2"` t0425
  scan of 005930 returned the empty/flat signature (`body_len=63`), zero owned resting rows.
- **U3 — SC CERTIFIED.** Leg 1 (`LS_NODE_SC_PROBE=1`, resting chain) returned `SC0-seen (2 accept
  frames); 2nd-WS-session tolerated`. Leg 2 (marketable) returned `SC1-seen (1 fill frame)`. A new
  **`LS_NODE_SC_CERTIFY=1`** leg then drove one marketable 1-lot buy and witnessed the SAME fill
  through **both** the SC1 frame and the t0425 poll via one production `FillLedger`, printing:
  `sc1_frames=1 sc_execprc_positive=true poll_saw_fill=true cheprice_populated=true
  total_fill_deltas=1 dedup_collapsed_to_one=true 2nd_ws_tolerated=true => CERTIFIED`. All KTD-5
  criteria met live: SC delivers fills with a positive `execprc`, the poll corroborates and carried
  a **positive `cheprice`** (exact, not the limit-price fallback), and the dual-source dedup
  collapsed to exactly one `FillDelta`. Account left flat after the sign-aware close.
- **U6 — authorized.** The CERTIFIED verdict + the U4 mechanism satisfy KTD-5; SC-primary is safe
  to activate. Go-forward activation is `LS_NODE_SC_PRIMARY=1` on the live node (constructs the
  exec client at the `SC_PRIMARY_BACKSTOP_CADENCE` 15s backstop; poll demoted, never disabled). The
  dedup that makes the relaxation safe is now proven against real frames, not just the mock.

**Defects surfaced live and fixed offline (all gated green, KTD-6):**
1. **`AccountId` panic** — `LsExecClient::new` passed the bare LS account number to nautilus
   `AccountId::from`, which panics without an `ISSUER-ID` (`-`); it blocked the first live run.
   Fixed with `normalize_account_id` (synthetic `LS-` issuer when absent; gateway-facing account
   number never rewritten) + a unit test.
2. **`verify_flat` over-counted holdings** — a same-day buy+sell leaves a lingering `janqty=0`
   t0424 row; the gate counted any row as an open position, false-failing "not flat" (the source of
   the marketable probe's misleading "not flat after close" warning). Now gates on `janqty > 0`,
   fail-closed on unparseable (mirrors the order check) + 2 tests.
3. **Marketable-probe fill-witness gap** — `run_marketable_probe` never ran the poll/ledger, so it
   could not witness the cheprice/dedup evidence KTD-5 requires. Added `LS_NODE_SC_CERTIFY=1` (dual-
   source witness through the production ledger; ord_no-targeted poll presence detection tolerates a
   paginating `chegb="0"` symbol) and `LS_NODE_CLOSE_ONLY=1` (fail-closed flatten recovery, never
   oversells, never buys) for stuck positions.

**Count tally.** **0 support-tier flips in this entry** — the work is a test-harness guard fix
(U1), an off-by-default runtime mechanism now certified for activation (U4/U6), README corrections
(U5), three live-surfaced defect fixes, and two new operator harness legs; no TR changed support
tier. `recommended` stays 3 (`t1101`/`token`/`S3_`), count sites unchanged. The §27 order-quartet
promotions remain their own operator-gated live tail (the U1 fix unblocks their re-probe).

## 29. IGW40011-as-500 is placed-nothing on the order type-variant differential (2026-07-07)

Plan `docs/plans/2026-07-07-002-fix-igw40011-placed-nothing-order-differential-plan.md`. The §28
attended re-probe confirmed the §27 reason-C pagination fix works live (controls placed and torn
down flat, no pagination HELD) — but it surfaced a **different** blocker that stops the §27 order
quartet (CSPAT00601/00701/00801) from certifying for `recommended`.

**Root cause.** The order negative-probe fires a `type` variant that deliberately sends a malformed
numeric field; the gateway correctly rejects it with `IGW40011` — a gateway **ingress** input-
validation reject (a numeric request field sent as a quoted string; NOT the rate-limit code
`IGW00201`) — which arrives as `http=500`. The probe fire loop's arm
`Some((http, rsp_cd, _)) if http >= 500 => …Held-may-rest halt=true` treated **any** 5xx as
may-have-rested and HALTED the differential before it completed, so all three quartet TRs halted on
their first `type` variant and never certified. Observed live: CSPAT00601 `OrdQty/type` → `IGW40011
(500)` → halt; CSPAT00701/00801 `OrgOrdNo/type` → `IGW40011 (500)` → halt. The **live order path**
carried the same defect at its root: `ls-core` `dispatch_once`'s order non-2xx branch mapped every
non-2xx order outcome — including `IGW40011@500` — to `LsError::AmbiguousOrder` →
`SubmitAction::Pending` (may-rest), even though an ingress-rejected request never routes to the
exchange and structurally cannot rest.

**Fix (landed offline this session, both seams).**
- **U1 — single source of truth.** New pure `ls_core::is_ingress_validation_reject(rsp_cd)` (`true`
  only for `IGW40011`, deliberately narrow; excludes the rate-limit `IGW00201` and hard gateway
  failures `IGW40013`/`IGW50008`, which may have reached the exchange and stay may-rest) +
  unit test. Consumed by BOTH the live path and the probe so they can never drift.
- **U2 — live order path (`ls-core inner.rs::dispatch_once`).** The order non-2xx branch now returns
  `LsError::ApiError` (a clean placed-nothing rejection → `classify_submit_error` → `Reject`) when
  `is_ingress_validation_reject(code)`, else the existing `AmbiguousOrder` (may-rest). The
  `adapters/nautilus` `classify_submit_error` is **unchanged** — it is deliberately variant-keyed
  ("never `rsp_cd` alone — the documented fail-open trap"), so the correct fix is at the seam that
  *chooses* the `LsError` variant, and the existing `ApiError`→`Reject` mapping does the rest. Two
  new mock-server regression tests (`IGW40011@500`→`ApiError`; other-5xx→`AmbiguousOrder`); the
  existing `IGW40011@200`→`ApiError` test stays green.
- **U3 — offline probe (`crates/ls-sdk/tests/negative_probe.rs`).** The inline `http >= 500` halt
  arm is replaced by a pure `classify_fired_variant(http, rsp_cd)` → `PlacedNothing | MayHaveRested
  | Accepted` that exempts `IGW40011@500` to `PlacedNothing` (Clean, continue) via the U1 predicate.
  Every other 5xx stays `MayHaveRested` (halt), the transport-failure `None` arm is unchanged, and a
  2xx ack still trips WAVE-BLOCKED. New offline unit test covers the four cases.

**Fail-closed preserved.** The exemption is exactly `IGW40011`, nothing else. Every other 5xx/non-2xx
order outcome and every transport failure stays may-rest/reconcile. A genuine throttle (`IGW00201`),
if it ever surfaced as a 5xx, stays on the may-rest default.

**Count tally.** **0 support-tier flips in this entry** — a probe-classifier fix, a live-dispatch
classification narrowing, a shared predicate, and their tests; no TR changed support tier.
`recommended` stays 3 (`t1101`/`token`/`S3_`), count sites unchanged.

**Remaining operator blocker.** Certifying the §27 quartet requires an **attended, open-KRX**
re-probe that places REAL paper orders (`LS_ORDER_SMOKE=1 LS_ORDER_SMOKE_NONCE=$(date +%s) make
live-smoke-cspat00601-negative` and 00701/00801). Order autonomy refuses unattended runs.

**Live re-probe result (attended, open KRX, 2026-07-07).** The operator ran all three legs.
**The IGW40011 fix is CONFIRMED live:** every `type`/`required` variant returning `IGW40011 (500)`
(`OrdQty`, `OrdPrc`, `OrgOrdNo`) now prints `outcome=Clean` and the differential **completes** instead
of halting — the exact intended behavior. **But the now-completing differential UNMASKED a separate,
previously-hidden blocker: `WAVE BLOCKED` on the `required` variants** (the halt used to stop the
probe before it ever reached them). The gateway **accepts** a request with a field the constraint
schema marks `required: true`:
- **CSPAT00701** (modify) — `IsuNo/required` removed → `00462` (clean modify ack, real ordno). A
  **real over-claim**: `IsuNo` is not gateway-required for a modify (`OrgOrdNo` identifies the order).
- **CSPAT00801** (cancel) — `IsuNo/required` removed → `00463` (clean cancel ack, real ordno). Same
  real over-claim for cancel.
- **CSPAT00601** (submit) — `BnsTpCode/required` (buy/sell direction) removed → `00000` (ambiguous
  generic-success, ordno unsurfaced) → the tripwire conservatively fails-closed. **Ambiguous, not
  proven**: `00000` needs a `raw-probe` A/B to decide whether `BnsTpCode`-removed places a real
  directional order or is rejected — **safety-relevant** (direction code), resolve before touching
  that schema. Teardown ran on every leg with no `UNEXPECTED-FILL`/`UNOWNED-RESTING` alarm (fallback
  cancel on 00601 since the ordno was unsurfaced); the operator confirms 005930 flat as a backstop.

**Disposition.** The IGW40011 fix (this entry / PR #107) is **validated live and stands on its own
merits** — it is not a promotion, and the WAVE BLOCKED is downstream of it, not a defect in it. The
§27 quartet **stays HELD**, now on the constraint-schema required-ness divergence rather than the
IGW40011 halt. That divergence is a **separate follow-up** (raw-probe A/B on `CSPAT00601 BnsTpCode`;
relax `CSPAT00701`/`00801` `IsuNo` to `required: false`; re-probe), and promotion via the
`promote-tr` recipe is that follow-up's tail — not this entry's.
