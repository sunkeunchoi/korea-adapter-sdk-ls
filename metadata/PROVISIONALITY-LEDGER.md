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
| t1988 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t3102 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t3320 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
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
| t1988 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t3102 | `[sNewsno]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t3320 | `[gicode]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
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
| t1481 | paginated (body-`idx`) | **PENDING — session-unresolved** | no in-session window run; `venue_session` unresolved |
| t1482 | paginated (body-`idx`) | **PENDING — session-unresolved** | no in-session window run; `venue_session` unresolved |

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
- **t1481 / t1482** — `venue_session` (§1) retained and explicitly
  **session-unresolved**: no SDK/core field carries session phase, and an
  off-session smoke cannot resolve `krx_regular` vs `krx_extended` (the
  `t1489`/`t1492` precedent in §5). Resolving it needs an in-session live-run
  window diffed against a regular-session run.

**Follow-up roadmap (opened as issues).**
1. **sFileData sourcing wave** — source a representative ~26.8 KB `sFileData`
   screening-condition blob, then implement `t1852`/`t1856` and reconcile their
   §2 caller-input rows to `[sFileData]`.
2. **Session-semantics wave** — run an in-session window to resolve `t1481`/`t1482`'s
   `venue_session`, then implement them at single-page body-`idx` scope.
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
| t1988 | (not authored) | **PENDING — gateway rejects (IGW40011)** | the raw-HTTP probe rejects every broad-filter form with `IGW40011`; its sibling t9905 (same path) works — environmental, no in-window recovery |

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
- **t1988** — `venue_session` (§1) and `caller_supplied_identifiers` (§2, `[]`)
  retained; owner_class stays `standalone`. No SDK code was authored (the raw
  probe pre-classified it as gateway-rejected). Resolving it needs the gateway to
  accept a t1988 request form (the `IGW40011` cause is unresolved in-window).

**Follow-up roadmap.**
1. **t1964 filter-default sourcing** — source the 10 ELW-board filter enums from a
   vendor spec or an observed HTS payload, then chain t1964 off t9905 and flip.
2. **t1988 gateway-form resolution** — determine why the paper gateway returns
   `IGW40011` for every broad t1988 filter form (provisioning vs request shape),
   then implement.

**Standing cost (accepted, per Risk Analysis).** This wave adds 5 consumer-less
live-smoke targets + 5 drift-detection structs that must stay green. Disposition
rule: a consumer-less smoke may go **pending (not red)** off-session, and a drift
failure on a consumer-less Implemented TR is **triage-P3**, not a release blocker.

Field-`type` facets (§4) are already retired inventory-wide (clean re-pin); nothing
to retire here. Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at six
Recommended TRs; no `metadata/evidence/<tr>.yaml` exists for any of the 7.
