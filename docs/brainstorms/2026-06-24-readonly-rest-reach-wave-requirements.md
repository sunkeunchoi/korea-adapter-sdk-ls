---
date: 2026-06-24
topic: readonly-rest-reach-wave
follows: docs/brainstorms/2026-06-23-readonly-rest-breadth-wave-requirements.md
---

# Read-Only REST Reach Wave (P0 + P3)

## Summary

The next implementation effort is a single **read-only REST reach wave**: take the
domains the prior breadth wave deferred to "separate lanes" and bring each lane to
callable surface, mixing the already-Tracked P0 reads with the still-raw P3
extended reads of the same domain. ~29 TRs across **7 per-domain lane PRs**. Each
lane PR runs a bounded reachability spike first, then carries its Tracked and raw
members together through track-raw → implement.

This stays inside the proven read-only REST pattern and respects the standing
read-only/side-effectful boundary unchanged. Realtime/WebSocket (P1) and
order-lifecycle observation (P2) are **out** — they remain behind that boundary and
require a separate WebSocket-lifecycle-smoke effort not yet built (only `S3_` exists
as a realtime reference, and the `implement-tr` recipe explicitly HELDs realtime out
of scope, SKILL.md §0).

## Problem Frame

The prior breadth wave (PR-A #43 / PR-B #44, merged 2026-06-23) implemented 12
domestic-reachable reads and explicitly held three REST domains to their own lanes
because their **paper reachability was uncharted** — not because their metadata was
missing. This wave opens those lanes. Two facts shape it:

- **The P0 set is implement-only; the P3 set is a raw lift.** All 14 P0 TRs are
  Tracked with projected baselines (`support.implemented: false`). All 15 P3 TRs are
  raw — no `metadata/trs/<tr>.yaml`, no baseline. So P3 members require the full
  raw→Tracked→Implemented lift (`track-tr` then `implement-tr`), which bumps
  `maintained_tr_count`, forces `make api-drift-renormalize` + a `manifest.refreshed`
  re-stamp, and ripples through the ls-trackers count tests and the cli.rs 44-literal
  block. P0 members skip all of that.
- **Tracked ≠ paper-reachable.** Overseas-stock, overseas-futures, and
  night-derivatives have uncharted reachability; the paper gateway may reject them
  (`01900`) or only answer in a night window. A cheap probe verifies reachability
  before any runtime is authored.

The breadth wave maximized callable surface within one already-charted, already-Tracked
batch. This wave instead pushes into *uncharted* domains, so reachability spikes and a
raw-tracking lift are first-class parts of the work rather than absent.

## Key Decisions

- **All read-only REST reads, per-domain.** One lane = one PR carrying that domain's
  Tracked (P0) and raw (P3) members together through probe → track-raw → implement.
  Coherent per-capability; isolates a failing domain to one diff.
- **Realtime (P1) and order-lifecycle (P2) are out.** They cross the standing
  read-only/side-effectful boundary and need a WebSocket-lifecycle-smoke methodology
  that does not yet exist generically. Deferred to a dedicated effort.
- **Spike-gate per lane; the gate stops runtime, never tracking.** A bounded
  `raw-probe` (or in-window probe for krx_extended) runs before runtime authoring.
  Reachability decides whether callable Rust is authored — it never decides whether
  metadata is authored. Tracking is a maintenance-ownership decision; runtime is a
  paper-reachability decision. All in-scope raw P3 TRs become Tracked once researched,
  even when their runtime is deferred, so `maintained_tr_count` rises by the full
  in-scope raw P3 set.
- **Implemented is the ceiling.** No promotion to Recommended; no Focused Evidence,
  no recommendation blocks (inherits breadth-wave R2).
- **Per-TR independent flip, no anchor/ship-floor.** Every TR flips on its own green
  smoke; non-green TRs ship dispositioned without blocking the rest of the lane
  (inherits breadth-wave R8).
- **Implemented ≠ venue-confirmed.** krx_extended members (night-derivatives,
  CCENQ90200) may flip Implemented on a reachable probe while keeping provisional
  venue-session ledger rows; in-window venue confirmation is a later facet pass.

## TR Inventory

| Lane (PR) | Tracked (P0) | Raw (P3) | Probe |
|-----------|--------------|----------|-------|
| Account/F&O | CCENQ90200 *(krx_extended)* | CFOAQ10100, CCENQ10100 | account / in-window |
| Overseas-stock | g3101, g3104, g3106 | g3102, g3103, g3190 | one g-probe |
| Overseas-futures | o3101, o3121 | o3105, o3106, o3125, o3126 | one o-probe |
| Night-derivatives | t8455, t8460, t8463 *(krx_extended)* | — | in-window |
| F/O quote | — | t2111, t2112, t2106, t8402, t8403, t8434 | anytime F/O |
| Paginated | t1481, t1482 | — | (charted) |
| Standalone | t1988, t3102, t3320 | — | caller-input |

- `t1988` previously hit `IGW40011` (numeric request-field serialization) — audit its
  request body against the solution doc before smoking.
- Standalone TRs (`t1988`, `t3102`, `t3320`) need caller-supplied input discovered
  before a representative smoke can run.
- CFOAQ10100 / CCENQ10100 are read-only orderable-quantity reads — account-gated but
  **not** order mutation; in scope as reads.

## Requirements

**Wave structure**

- R1. Deliver one reach wave as ~7 per-domain lane PRs (above). Each PR self-gates,
  runs its own reachability spike, smokes its own TRs, and bumps its own
  count-coupled tests.
- R2. The rung target is Implemented for every reachable in-scope TR; no promotion to
  Recommended in this effort.
- R3. P0 lanes (night-derivatives, paginated, standalone) are implement-only: no
  renormalize, no `maintained_tr_count` change. Lanes that add raw P3 members
  (account/F&O, overseas-stock, overseas-futures, F/O quote) do the full raw lift.

**Reachability spike (per lane, before runtime)**

- R4. Each lane begins with a bounded probe: `make raw-probe` for the domain's prefix
  with representative identifiers (overseas-stock g-, overseas-futures o-, anytime
  F/O), or an in-window probe for krx_extended lanes (CCENQ90200, t8455/60/63).
- R5. Disposition each TR by probe + smoke outcome — and note that the probe gates
  **runtime only**:
  - Reachable → Raw→Tracked→Implemented (or Tracked→Implemented for P0).
  - `01900` (paper-incompatible) → author/keep metadata (Tracked), `implemented:false`,
    `paper_incompatible:true`. No runtime this wave.
  - Closed session / wrong window → Tracked, `implemented:false`, venue/session
    requirement documented (HELD with the required window).
  - Input-unresolved → Tracked, `implemented:false`, caller-input/ledger note retained.
  - Empty success (`00707`) → Implemented gate not opened: `PENDING — empty result,
    shape unconfirmed`, stays tracked-only.
  - Raw-probe succeeds but SDK deserialize fails → `DROPPED` (TR defect), tracked-only
    with a recorded reason.

**Per-TR lift**

- R6. Raw P3 members: follow `track-tr` (`metadata/trs/<tr>.yaml` + `tr-index.yaml`
  entry + projected baseline via `make api-drift-renormalize`) before
  `implement-tr`. Tracked P0 members go straight to `implement-tr`.
- R7. For each implemented TR, follow `implement-tr`: author InBlock/OutBlock + facade
  method + EndpointPolicy const (registered in **both** `endpoint_policy.rs` and
  `policy_index_crosscheck.rs`), add the offline deserialize test, add the
  `live_smoke_<tr>` test + `make live-smoke-<tr>` target + `smoke-map.md` row.
- R8. Numeric request-body fields serialize as JSON numbers (`string_as_number`) to
  avoid `IGW40011`; audit each TR's request fields against the solution doc
  (`docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`),
  starting with `t1988`.
- R9. Wire field names, types, and array-vs-single shapes come from the normalized
  baseline, never guesswork — for raw P3 members, read the out-block key from the raw
  capture rather than guessing single-vs-array.

**Count-coupled tests**

- R10. Each implemented flip updates the ls-docgen `banner_trs` array (currently 43)
  and the `reference.len()` literal (currently 51) in `crates/ls-docgen/src/lib.rs`
  (~:865, ~:912) and regenerates docs before merge — only for TRs that flip Implemented.
- R11. Each raw P3 TR brought to Tracked bumps `maintained_tr_count` (currently 70,
  `crates/ls-trackers/tests/api_drift.rs:106`), the ls-trackers tracking-count tests,
  the `TRACKED_TRS` list, and the cli.rs 44-literal block. **These bumps must be
  serialized across lane PRs** — stacked, each PR rebases the count forward; developed
  in parallel they collide on the same literals.

## Scope Boundaries

**Deferred for later**
- Promotion of any of these TRs to Recommended (Focused Evidence + recommendation
  blocks) — a separate effort after Implemented.
- In-window krx_extended venue-facet *confirmation* beyond what a reachability probe
  yields — masters/night-derivatives may flip Implemented venue-provisional.

**Held to separate efforts (out of this wave)**
- **Realtime/WebSocket market-data (P1)** — K3_, H1_, HA_, S2_, US3, UH1, US2, GSC,
  GSH, OVC, OVH, OC0, OH0, FC9, FH9. All 15 are raw; the `implement-tr` recipe HELDs
  realtime out of scope (SKILL.md §0); only `S3_` exists as a realtime reference. Needs
  a generic WebSocket-lifecycle-smoke methodology (connect → subscribe → unsubscribe)
  built first.
- **Order-lifecycle observation (P2)** — SC0–SC4, C01, O01, H01, AS0–AS4, TC1–TC3. All
  14 raw; side-effect-adjacent realtime feeds; folded into the same deferred realtime
  effort, observation-only, never REST order runtime.

## Dependencies / Assumptions

- All 14 P0 candidates are confirmed Tracked (`support.implemented: false`);
  `implement-tr` can derive structs from their projected baselines.
- All 15 P3 candidates are confirmed raw — no metadata, no baseline — so each requires
  `track-tr` before `implement-tr`. *(Verified by grounding scout against
  `metadata/trs/`.)*
- The account-state implement path opened by the Wave 0 recipe edit is in place, so
  CCENQ90200 / CFOAQ10100 / CCENQ10100 route through the account class.
- Paper-gateway availability gates smoke runs; deposit/quantity reads may legitimately
  return empty (`00707`, PENDING, not a defect) on a position-less paper account.
- One domain probe covers both the Tracked and raw members of that domain (one g-probe
  for all six overseas-stock codes; one o-probe for all six overseas-futures codes).

## Outstanding Questions

**Deferred to planning**
- Lane-PR **sequencing order** for the serialized count bump (R11): which raw-bearing
  lane goes first, and whether to batch all P3 tracking into a single renormalize
  rather than per-lane, to reduce `maintained_tr_count` literal churn.
- The reachability verdict per uncharted domain (overseas-stock, overseas-futures,
  night-derivatives) — drives how many lanes actually author runtime this wave.
- Caller-supplied input shapes for the standalone lane (`t1988`, `t3102`, `t3320`).
- Whether each F/O-quote raw TR fits one PR or warrants a split by out-block complexity.

## Sources / Research

- Grounding dossier: `/tmp/compound-engineering/ce-brainstorm/next-wave/grounding.md`
  — tracking status (P0 all Tracked, P3/P1/P2 all raw), realtime infra inventory,
  `implement-tr` realtime gate, count-coupled literals.
- `docs/brainstorms/2026-06-23-readonly-rest-breadth-wave-requirements.md` —
  predecessor; this wave opens the lanes it deferred (Scope Boundaries → Held to
  separate lanes).
- `CONCEPTS.md` — Raw/Tracked/Implemented ladder (Tracked = "observed for drift,
  nothing more", reachability-independent), Paper Live Smoke, Pending.
- `AGENTS.md` — gate sequence, `track-tr`/`implement-tr`/`raw-probe` recipes, IGW40011
  gotcha, do-not-`cargo fmt`-ls-trackers note.
- `.agents/skills/implement-tr/SKILL.md` §0 — read-only gate; realtime/WebSocket and
  side-effectful TRs are HELD out of scope.
- `crates/ls-docgen/src/lib.rs` (~:865 `banner_trs`=43, ~:912 `reference.len()`=51),
  `crates/ls-trackers/tests/api_drift.rs:106` (`maintained_tr_count`=70) — count-coupled
  edits.
- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md` —
  numeric request-field serialization.
