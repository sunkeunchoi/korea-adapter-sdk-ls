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
| t1601 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1615 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1640 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1662 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1664 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1825 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1826 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1852 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1856 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1859 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1860 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1866 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1958 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1964 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t1988 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t3102 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t3320 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t3341 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t8430 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t8431 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t9905 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t9907 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |
| t9942 | `krx_regular` | best-effort: stock (`[주식]`) read, KRX regular session assumed | confirm the session the read is actually scoped to |

## 2. `caller_supplied_identifiers`

Authored best-effort from request-shape input fields. For filter/`gubun`-style screens
the list is empty; where an instrument or record identifier is present in the request
it is recorded. The true required-input set is confirmed at implementation.

| TR | Provisional value | Source basis | Re-verify before implementation |
|---|---|---|---|
| t1481 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1482 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1601 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1615 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1640 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1662 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1664 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1825 | `[search_cd]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t1826 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1852 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1856 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1859 | `[query_index]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t1860 | `[query_index]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t1866 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t1958 | `[shcode1, shcode2]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t1964 | `[item, issuercd]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t1988 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t3102 | `[sNewsno]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t3320 | `[gicode]` | best-effort: request-shape input fields that look like instrument/record identifiers | confirm the true caller-supplied identifier set against a live request |
| t3341 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t8430 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t8431 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t9905 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t9907 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |
| t9942 | `[]` | best-effort: no obvious instrument/record identifier in the request shape (filter/`gubun`-style screen) | confirm no caller-supplied identifier is required |

## 3. Weak discovery-style relationships

Cross-TR discovery dependencies visible in the request shape but **not** modelled in
the per-TR `dependencies` block (which today covers only self-continuation and
order-coupling fields).

| TR | Relationship | Source basis | Re-verify before implementation |
|---|---|---|---|
| t1859 | `query_index` ← `t1866OutBlock1.query_index` | request field `query_index` is documented as sourced from `t1866`'s output — a cross-TR discovery dependency, not modelled in `dependencies` | model the `t1866 → t1859` discovery edge when either TR is implemented |
| t1860 | `query_index` ← `t1866OutBlock1.query_index` | request field `query_index` is documented as sourced from `t1866`'s output — a cross-TR discovery dependency, not modelled in `dependencies` | model the `t1866 → t1860` discovery edge when either TR is implemented |

## 4. Field-level `type` facets — HTTP-500 system-codes seed (all 36)

Every one of the 36 normalized baselines inherits the **system-codes HTTP-500
property-type fallback** that the committed raw snapshot was fetched under (see
`crates/ls-trackers/baselines/api-drift/SEED-ATTESTATION.md`). Field-level `type`
values in the normalized shapes therefore fall back to raw type codes and are
provisional for the whole batch.

This is **not** re-pinned in this batch. The field-`type` re-pin lands as a **separate
later PR** after a clean `system-codes` fetch resolves the real type names and emits a
planned one-time `type` drift wave (see `docs/MAINTENANCE_RUNBOOK.md` and the plan's
Key Technical Decisions). Until then, do not treat any of the 36 shapes' field `type`
values as type-level ground truth.

| Scope | Provisional value | Source basis | Re-verify before implementation |
|---|---|---|---|
| All 36 TRs (field-level `type`) | normalized shape `type` codes served under the `system-codes` HTTP-500 fallback | committed raw fetched while `system-codes` returned HTTP 500 (SEED-ATTESTATION) | resolved batch-wide by the separate field-`type` re-pin PR after a clean `system-codes` fetch — not per-TR work |

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
- Field-level `type` facets (§4): unchanged for all 36 — a clean deserialize does
  not confirm the HTTP-500-seeded types. Stays with the separate field-`type`
  re-pin PR.
- Multi-page collection over body-`idx` for the 7 paginated TRs: deferred
  follow-up (these are Implemented at single-page scope only).

Recommended tier untouched: `EVIDENCE-FRESHNESS.md` stays at six Recommended TRs;
no `metadata/evidence/<tr>.yaml` exists for any of the 11.
