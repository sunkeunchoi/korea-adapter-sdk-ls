---
date: 2026-06-26
topic: krx-closed-flip-wave
---

# Closed-Window TR Flip Wave — Requirements

## Summary

Flip the two Tracked TRs whose Paper Live Smoke can plausibly certify while KRX is closed — `t1310` (a historical tick/min chart read) and `t1404` (an administrative-designation status board), both with no session prerequisite. Re-point the other five previously-considered candidates to their real (closure-independent) blockers rather than re-probing them this wave.

## Problem Frame

Most flip waves wait for an open KRX window because the certifying Paper Live Smoke needs live data. With the market closed, that path is shut for quote, chart-session, and night/overseas reads. Two reads are plausibly the exception because neither depends on the current session: `t1310` is a historical tick/min chart pull, and `t1404` is an administrative-designation board (관리/불성실/투자유의 — management / unfaithful-disclosure / investment-caution) whose rows persist across sessions. These are the wave's genuine fresh candidates.

The other five TRs once grouped as "closed-window candidates" carry blockers that closure does not lift, and re-probing them would re-confirm prior dispositions for no new count: `t1852`/`t1856` need a ~26.8 KB `sFileData` screening blob the SDK has no producer for; `t3102` is already built (struct + policy + facade + smoke) but is input-blocked — its `sNewsno` is sourced only from the realtime NWS WebSocket feed, so it cannot be smoked over REST; `t1964` is likewise already built and was left unflipped on a documented empty-board disposition; `t1860` is a realtime-control subscription (`서버저장조건 실시간검색`), not a plain read. The honest yield of this wave is ~1–2 flips, and only if the paper gateway actually serves these reads under closure.

## Key Decisions

- **"Closed-window reachable" is a curated judgment, not a metadata facet.** All the candidates carry the same `venue_session: krx_regular` facet as the excluded `t1308`/`t2106`, so nothing in metadata marks a TR as session-independent. The split is a hand-made per-TR call about whether the gateway plausibly returns a non-empty body under closure, backstopped by the probe-then-disposition flow (R3/R4) — not derived from `owner_class` or any facet.
- **Primary candidates are `t1310` and `t1404` only.** They are the genuine fresh greenfield flips: historical-chart reads, all-string request fields, no session prerequisite. The other five are pre-decided with closure-independent blockers and are re-pointed, not re-probed, this wave (see Scope Boundaries).
- **Non-empty assertion gates every flip.** A smoke returning empty `00707` during closure does not certify — assert at least one modeled non-key field holds a real value before recording a flip. Realistic yield is ~1–2, driven by the genuine candidate count and `t1404`'s empty-board risk, not by account-state variance across seven probes.
- **The five re-pointed TRs keep their existing dispositions.** `t1852`/`t1856` → `sFileData`-input PENDING (no producer); `t3102` → `sNewsno`-input PENDING (already built, but realtime-NWS-only input so un-smokeable over REST); `t1964` → empty-board PENDING (already built); `t1860` → realtime-control HELD. Closure changes none of these, so this wave does not author or re-run their smokes.
- **Recommended stays deferred.** Every flip lands `implemented: true`, `recommended: false` (ADR 0008). Promotion to Recommended is a separate pass.

## Requirements

**Candidate set**

- R1. Attempt a Tracked→Implemented flip for the two primary candidates `t1310` and `t1404`. The five re-pointed TRs (`t1852`, `t1856`, `t3102`, `t1964`, `t1860`) are NOT smoked this wave; they keep their existing dispositions (see R6).
- R2. The 14 window-gated / paper-incompatible Tracked TRs (`g3101`, `g3102`, `g3103`, `g3104`, `g3106`, `g3190`, `t8455`, `t8460`, `t8463`, `CCENQ10100`, `CCENQ90200`, `t1308`, `t2106`, `t8411`) stay Tracked with their existing dispositions untouched. (`t8411` is the 14th member; the Key Decisions enumeration above abbreviates the group but R2 is the authoritative list.)

**Certification**

- R3. A candidate flips to Implemented only when its `live_smoke_<tr>` returns a success body that deserializes into the response type AND at least one modeled non-key field holds a non-default value.
- R4. A candidate whose smoke returns empty (`00707`), is un-smokeable for lack of a required input, or is held for non-read reasons does NOT flip; it is recorded with a faithful disposition naming the reason. `t1404` carries this risk concretely (see R7).
- R5. `t1310` and `t1404` flip without any IGW40011 numeric-field fix — their normalized baselines carry all-string request fields, so no `string_as_number` change is required.

**Disposition of the re-pointed TRs**

- R6. The five re-pointed TRs keep their existing dispositions without a new smoke this wave: `t1852`/`t1856` → `sFileData`-input PENDING (the ~26.8 KB screening blob has no SDK producer); `t3102` → `sNewsno`-input PENDING (sourced only from the realtime NWS WebSocket feed); `t1964` → empty-board PENDING (already built; prior wave); `t1860` → realtime-control HELD. Each is re-pointed to its real unblock path (an `sFileData`-sourcing wave, a realtime effort) rather than re-probed under closure.
- R7. `t1404` (management / unfaithful-disclosure / investment-caution board) is a status-list read that may legitimately return empty; if the closed-window smoke returns empty, disposition as empty-board PENDING rather than flip — do not treat it as a guaranteed clean read alongside `t1310`.

**Per-flip mechanics and gate**

- R8. Each genuine flip touches the six standard implement-tr sites (request struct + facade, policy const, dual cross-check registration, offline tests, live smoke, metadata flip) and bumps the docgen `banner_trs` list plus the `reference.len()` literal by one per flipped TR. Because yield is variable, the count target is `114 + (TRs that actually flip)` — 114 if both disposition, 115 if one flips, 116 if both; update the assertion to the realized count, not a fixed value. (The six-site cost applies to greenfield TRs like `t1310`/`t1404`; the already-built but re-pointed `t1964`/`t3102` are out of scope this wave.)
- R9. The full gate stays green: `make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`.

## Acceptance Examples

- AE1. **Covers R3, R5.** Given KRX is closed, when `live_smoke_t1310` fires, then the historical chart body deserializes and a modeled price/volume field is non-default → `t1310` flips to Implemented.
- AE2. **Covers R4, R7.** Given KRX is closed, when `live_smoke_t1404` fires and the caution board returns empty `00707`, then `t1404` does not flip and is recorded empty-board PENDING.
- AE3. **Covers R1, R6.** Given the wave reaches `t3102`, then no smoke is authored or run because `sNewsno` has no SDK producer; `t3102` keeps its `sNewsno`-input PENDING disposition unchanged.

## Scope Boundaries

- The five re-pointed TRs are routed to their real unblock efforts, not this wave: `t1852`/`t1856` to an `sFileData`-sourcing wave, `t3102` to whatever yields an `sNewsno` (realtime NWS), `t1964` to an empty-board filter-default fix, `t1860` to the realtime-control effort.
- Window-gated reads (`t1308`, `t2106`) and night/overseas/account TRs — wait for an open window or are permanently `paper_incompatible`; not retried here.
- A durable "closed-window flippable" classifier (facet or disposition encoding the rule) — out of scope; reachability is re-judged per wave for now.
- Recommended promotion for any flipped TR — separate pass, ADR 0008.
- The open flip-cost / cadence decision from the prior wave — not resolved here, but note: a ~1–2-flip wave at full six-site cost each makes that decision more pressing, since this wave spends the cost the decision exists to evaluate.

## Dependencies / Assumptions

- Baseline is `main` (commit `cb0e66e`), docgen `reference.len()` at 114; the prior reach-wave flips are already merged, so the wave does not stack on an unpushed branch.
- A paper gateway reachable with `.env` credentials and `LS_TRADING_ENV=paper`; the operator runs the live smokes — they are not run autonomously.
- **Load-bearing premise (unproven):** the paper gateway serves both reads under closure — historical chart bars for `t1310`, and a populated designation board for `t1404`. These are two distinct premises, not one: `t1310` rests on the chart-pull exception; `t1404` rests on the designation board being served (and non-empty) outside trading hours. Both are asserted, not demonstrated — a prior night/overseas wave found some paper feeds carry zero data in certain states (`00707`, 0 flips). If either is gated on an open session, that read dispositions to PENDING; if both are, the wave yields 0.

## Outstanding Questions

**Resolve Before Planning**

- None blocking — the candidate set is narrowed to `t1310`/`t1404` and the certification rule is pinned.

**Deferred to Planning**

- Whether `t1404`'s caution board returns non-empty under closure, or dispositions empty-board per R7 (drives whether yield is 1 or 2).
- Whether the closed-window historical-bar premise (Dependencies) holds for `t1310`; if the operator's first smoke returns `00707`, the wave reduces to a disposition exercise.

## Sources / Research

- Grounding dossier: `/tmp/compound-engineering/ce-brainstorm/krx-closed-flip/grounding.md` (Tracked-TR census, session classification, flip mechanics).
- Flip recipe: `.agents/skills/implement-tr/SKILL.md` (six per-TR sites, dual cross-check registration).
- IGW40011 mechanics: `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`.
- Count assertion: `crates/ls-docgen/src/lib.rs:1032` (`reference.len()` == 114).
- Normalized baselines confirming all-string request fields for `t1310`/`t1404` (no IGW40011 fix needed): `crates/ls-trackers/baselines/api-drift/normalized/trs/t1310.json`, `t1404.json`.
