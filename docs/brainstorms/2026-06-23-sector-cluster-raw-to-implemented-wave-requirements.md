---
date: 2026-06-23
topic: sector-cluster-raw-to-implemented-wave
---

# Sector Cluster Raw→Implemented Wave (Wave A)

## Summary

Wave A of a per-cluster TR campaign: bring the five-TR 업종/sector cluster (`t8424` plus
`t1511`/`t1514`/`t1516`/`t1485`) from **raw all the way to Implemented in a single PR** —
author per-TR metadata and pin a normalized baseline from the raw OpenAPI capture (creating
the Tracked rung), then author callable Rust gated on a Paper Live Smoke. The wave also produces
a reusable track-then-implement path that the remaining ~75 domestic read-only candidates
inherit. The full ~80 is a roadmap of cluster-waves, not a single deliverable.

---

## Problem Frame

The campaign so far (consumer-bound, bulk-tracked, saved-condition screening, and the three
capability-closed ELW/analytics/ThinQ waves) operated entirely on the **Tracked→Implemented**
rungs: every member already had committed metadata and a normalized baseline, so the
`implement-tr` recipe could derive structs from the pinned wire shape. The sector cluster
is different. None of `t8424`/`t1511`/`t1514`/`t1516`/`t1485` carry a `metadata/trs/<tr>.yaml`
or a `crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`; they exist only in
the raw OpenAPI capture, `code-set.json`, and the migration-source dependency map. There is
no `track-tr` recipe in `.agents/skills/` — only implement, promote, and audit recipes.

So this wave inherits net-new prerequisite work no prior wave had: the **Tracked rung**
itself. Getting it right once, on a small coherent cluster, forges the path the rest of the
~80 candidates reuse — which is why the sector cluster is the right first test rather than a
larger one.

A second, independent pressure: all five members are marked `session_class: dependent` /
`dependency_reason: market_hours` in the migration source. Their live data is only meaningful
during the KRX regular session, so a smoke run off-hours risks empty results that would
otherwise be misread as a shape failure.

---

## Key Decisions

- **Cluster-as-capability, not coverage.** Each cluster is its own capability-closed wave and
  PR; the ~80 candidates are a roadmap, never a coverage justification. This preserves the bar
  the prior campaign set — breadth alone never justifies a callable-but-uncalled surface. A
  coverage-driven reframe would require a deliberate ADR superseding that rule and is not taken
  here.

- **One raw→implemented PR, not a track-then-implement split.** Wave A absorbs both rungs in a
  single PR: author metadata + pin baseline, then implement and smoke. It is the largest first
  PR of the campaign, but it forges and freezes the reusable track-then-implement path the
  remaining clusters need, which a two-PR split would defer.

- **`t8424` is the intended anchor — its window-independence is a probe, not an assumption.**
  As 전체업종 (the all-sectors reference list), `t8424` is the cluster's self-sourcing base for
  `upcode` and the intended ≥1-flip anchor. But the migration source marks it
  `session_class: dependent` / `market_hours`, the same flag as the four consumers, so its
  off-hours-non-empty behavior is *not* assured. A planning-time paper probe confirms it: if a
  non-empty 업종 list returns off-hours, `t8424` anchors without an in-session window; if not, it
  falls to the same in-session smoke bar as the consumers and the anchor guarantee is re-derived
  from an in-window flip.

- **The four live consumers are held to an in-session smoke bar.** For `t1511`/`t1514`/`t1516`/
  `t1485`, an off-hours empty or market-closed result is **not** a valid implementation attempt
  — it is re-run during the KRX regular session before any verdict. Only an in-window run that
  still returns empty or rejects the sourced `upcode`/defaults records pending. This keeps the
  wave shippable without weakening the evidence bar for live sector behavior.

- **`upcode` sourced from `t8424`, representative-code fallback — shape unconfirmed.** The four
  consumers require `upcode` (업종코드, required=Y), but the migration source models its
  `producing_tr` as null (strength WEAK), and the recorded `producer_sample` is alpha-form
  (`BMT` / `BM_` / `IJ_`), not the numeric `001` first assumed. The wave treats `t8424`'s output
  as the producer; if that list does not carry a usable `upcode`, the consumers fall back to a
  representative code drawn from the confirmed accepted shape (probe before relying on a literal
  value). Because the accepted `upcode` appears non-numeric, it must stay string-serialized — it
  must *not* receive `string_as_number`, which would be the inverse IGW40011 trap. The fallback
  is acceptable, not a blocker; its exact value is a planning probe.

---

## Requirements

**Wave scope & rung handling**

- R1. Wave A ships as a single PR covering exactly the five sector TRs `t8424`, `t1511`,
  `t1514`, `t1516`, `t1485`. Membership is bounded by the named 업종/sector capability, not by
  coverage of the ~80 backlog.
- R2. Each member is brought from raw to Implemented within the wave: author its
  `metadata/trs/<tr>.yaml`, pin a normalized baseline from the raw OpenAPI capture, then author
  callable Rust via the `implement-tr` recipe and gate it on a Paper Live Smoke. Before the
  five-TR list locks, each member's raw-capture wire shape (request *and* response blocks) is
  verified complete enough to pin a deserializable baseline without live probing; a member whose
  raw shape is incomplete is held or dropped from Wave A pre-PR. This metadata-authoring gate is
  distinct from the R12 smoke gate — an incomplete shape blocks a TR before R5 is ever attempted.
- R3. The wave produces a reusable track-then-implement path as a first-class deliverable — the
  repeatable procedure for taking a raw TR to the Tracked rung (metadata + pinned baseline) that
  the remaining ~75 candidates will reuse. Whether it extends `implement-tr` or becomes its own
  frozen recipe is deferred to planning. If planning concludes the path needs its own recipe
  file (e.g. `.agents/skills/track-tr`), it still ships inside Wave A's single PR (R1) as an
  explicitly-flagged non-TR deliverable, or as a preparatory commit on the same branch — the
  single-PR boundary holds.

**Per-TR implementation & smoke**

- R4. Each member gains callable Rust: request struct, response struct, public SDK method,
  dependency-class registration, and a per-TR paper-smoke harness. Genuinely numeric request-body
  fields serialize as JSON numbers (`string_as_number`) to avoid `IGW40011`; response fields use
  the tolerant `string_or_number`. Identifier fields that are not numeric — notably `upcode`,
  whose recorded sample is alpha-form — stay string-serialized; applying `string_as_number` to a
  non-numeric field is the inverse trap and is itself a rejection risk.
- R5. The Implemented gate per TR: the request constructs through the public SDK path, a paper
  LS call returns a recognized success `rsp_cd` with a non-empty result, and the response
  deserializes into the hand-written type.
- R6. `t8424` smokes as the anchor. A non-empty 업종 list that deserializes flips it Implemented,
  and its output is the source of `upcode` values for the consumers. Whether it can smoke
  window-independently is settled by the planning-time off-hours probe (see Key Decisions); until
  that probe confirms off-hours non-emptiness, `t8424` is smoked in-window like the consumers.
- R7. `t1511`/`t1514`/`t1516`/`t1485` smoke during the KRX regular session. An off-hours empty
  or market-closed result is not a valid attempt and is re-run in-window before any verdict. An
  in-window run that returns non-empty and deserializes flips Implemented; an in-window empty or
  `upcode`/defaults rejection records pending with a concrete reason. `t1516` (업종별종목시세)
  requires a *second* caller-supplied input, `shcode` (종목코드, required=Y, `producing_tr` null),
  in addition to `upcode`; it smokes with a representative `shcode` alongside the `upcode`, and is
  the one consumer with two unmodeled required inputs rather than one.
- R8. `t1514` is self-paginated (`self_fields: [cts_date]`) and smokes under the recipe's
  paginated path; a non-empty first-page success satisfies its gate.

**Status & bookkeeping**

- R9. Promotion sets `support.implemented: true`, leaves `support.recommended: false`, and
  writes no recommendation block and no evidence record. Each Implemented member gets a reference
  page carrying the "Implemented, not yet recommended" banner.
- R10. The wave records each member's end state (implemented / pending / dropped) with a
  credential-free disposition, including a per-member `venue_session` disposition — confirmed
  against live behavior or explicitly annotated as unconfirmable-by-smoke. Pending and dropped
  members keep their provisional rows so nothing stays live with a stale "re-verify" instruction.
- R11. The wave updates the docgen count-bearing test to reflect the number actually promoted
  and adds each newly-Implemented TR to the banner list. Recommended-tier artifacts and the
  tracked-TR count are untouched.

**Outcome**

- R12. Wave A is block-and-drop: a TR-isolated smoke failure drops that TR to tracked-only with
  a recorded reason; an in-window empty or unresolved input ships pending; an environmental
  failure keeps candidacy without flipping `support.implemented`. The wave ships only if at least
  one member flips Implemented with a passing smoke — `t8424` is the intended anchor, but the
  guarantee rests on an *in-window* flip (any member), not on an unverified off-hours `t8424`
  result; if zero members flip, the wave re-scopes rather than shipping an empty PR.

---

## Acceptance Examples

- AE1. **Covers R6, R12.** The planning probe shows `t8424` returns a non-empty 업종 list
  off-hours; it then flips Implemented off-window and anchors the wave. If the probe instead shows
  `t8424` empty off-hours, it is smoked in-window like the consumers and the anchor is re-derived
  from whichever member flips in-window.
- AE2. **Covers R7.** `t1511`'s smoke runs off-hours and returns empty → recorded as *not a valid
  attempt*, re-run during the KRX regular session; the in-window run returns non-empty and
  deserializes → it flips Implemented.
- AE3. **Covers R5, R7.** An in-session `t1485` smoke returns empty or rejects the sourced
  `upcode` → it ships pending with a concrete reason, keeping its provisional rows; the wave still
  completes on the members that flipped.
- AE4. **Covers R6, Key Decisions.** `t8424`'s returned list carries a usable `upcode`, which
  feeds the four consumers' requests; if it does not, the consumers smoke with a representative
  code drawn from the confirmed accepted shape (alpha-form per the recorded sample, not the
  numeric `001` first assumed) and the wave proceeds.

---

## Scope Boundaries

**Deferred for later (roadmapped cluster-waves)**

- The remaining domestic read-only clusters — stock quote/time-series, ETF, remaining ELW
  (the t195x/t196x/t197x band), and investor/program/foreign flows — are named in the roadmap
  and ship as their own capability-closed waves, not in Wave A.

**Outside this wave's identity**

- Recommended tier, Focused Evidence, and any `metadata/EVIDENCE-FRESHNESS.md` edit — Wave A is
  Implemented-only.
- A coverage-driven justification for the campaign — excluded by decision; adopting it would
  require a separate ADR superseding the capability-closed rule.
- Account, order, realtime/WebSocket, futures/options, overseas, and side-effectful TRs — out of
  the entire ~80 read-only campaign, not just Wave A.

---

## Dependencies / Assumptions

- The raw OpenAPI capture (`crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`)
  contains usable request/response wire shapes for all five sector TRs, sufficient to author
  metadata and pin a normalized baseline without live probing. (Presence verified; completeness
  to be confirmed in planning.)
- `t8424` is *assumed* to return a non-empty 업종 list on paper as the anchor and `upcode` source,
  but it shares the consumers' `session_class: dependent` / `market_hours` flag, so its
  off-hours non-emptiness is unverified — confirmed (or not) by the planning probe below before
  the window-independent anchor is relied on.
- `t1516` carries two caller-supplied required inputs (`upcode` and `shcode`), not one; both need
  a representative value for its smoke.
- A KRX regular-session window is available to run the four live consumers' smokes; without one,
  those members ship pending rather than failing the wave.
- The sector reads are paper-callable read-only stock TRs with no `account_state` /
  `paper_incompatible` flags; any that turns out otherwise surfaces through R5/R12 block-and-drop.

---

## Outstanding Questions

**Resolve before planning**

Three cheap paper probes settle premises the wave's shape depends on; each can independently
drive the wave toward zero flips if it fails, so they precede planning rather than deferring into
it.

- Confirm whether `t8424` returns a non-empty 업종 list off-hours on paper. If not, its
  window-independent-anchor role is invalid and it drops to the in-session bar (R6, R12).
- Confirm at least one consumer accepts a constructable `upcode` (sourced from `t8424` or a
  representative alpha-form code), and capture its exact accepted shape — the recorded sample is
  alpha (`BMT`/`BM_`/`IJ_`), not numeric `001`.
- Confirm each member's raw-capture wire shape is complete enough to pin a deserializable
  baseline (R2's metadata-authoring gate); any member that fails is held/dropped before the
  five-TR list locks.

**Deferred to planning**

- The concrete form of the track-then-implement path (R3): extend the `implement-tr` recipe with
  a preceding tracking step, or author a separate frozen `track-tr` recipe. (Scope-guardian dissent,
  not adopted: downgrade R3 to a best-effort documented procedure and defer freezing the recipe to
  Wave B, on the argument that the frozen recipe is infrastructure for the other ~75, not needed to
  ship these five. Held as first-class per the wave's deliberate setup-value choice.)
- Whether `t8424`'s output is modeled as an explicit producer→consumer discovery edge for the
  four `upcode` consumers (mirroring the `t1826`→`t1825` edge) or the consumers are smoked
  standalone with a representative `upcode`. Default: source from `t8424`, fall back to a
  representative code.
- `owner_class` confirmation per member against the metadata validator (`t1514` is paginated;
  the others non-paginated).
- Per-member numeric request-field audit for `IGW40011` exposure (e.g. paginated `cts_date`,
  any rate/range filters) before the live smoke.

---

## Sources / Research

- Raw wire shapes for the cluster: `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json`,
  `crates/ls-trackers/baselines/api-drift/code-set.json` (all five codes present; none have a
  per-TR normalized baseline or `metadata/trs/<tr>.yaml`).
- Cluster facets, `upcode` edges (`producing_tr: null`, required=Y, WEAK), `session_class:
  dependent` / `market_hours`, and `t1514` self-pagination (`cts_date`):
  `docs/migration-source/tr-dependencies-2026-06-14.json`.
- Frozen `tracked → implemented` recipe and the absence of a `track-tr` recipe:
  `.agents/skills/` (implement-tr, promote-tr, promote-trs, audit-row, audit-carried-rows,
  grill-with-docs).
- Reference implementation pattern (request/response structs, `string_or_number`, paper-smoke
  double-guard): `t1101` and `crates/ls-sdk/` per AGENTS.md.
- `IGW40011` numeric request-field gotcha and `make raw-probe` classifier:
  `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`, AGENTS.md.
- Predecessor campaign and its capability-closed bar:
  `docs/brainstorms/2026-06-23-capability-closed-tr-expansion-waves-requirements.md` and the
  earlier 2026-06-21/2026-06-22 wave docs.
- Support lifecycle and Provisionality Ledger discipline: `CONCEPTS.md`,
  `metadata/PROVISIONALITY-LEDGER.md`.
