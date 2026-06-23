---
date: 2026-06-23
topic: readonly-tr-raw-to-implemented-sequence
---

# Read-Only TR Raw→Implemented Sequence

## Summary

Take 21 read-only LS TRs from Raw to Implemented through a bulk **Wave 0**
tracked-only expansion (all 21 to Tracked: metadata + projected baselines, no
callable Rust) followed by sequential per-domain implement waves: account →
anytime futures/options → night/session F/O → overseas-futures → overseas-stock.
Each implement wave is its own PR, gated on a Paper Live Smoke, and a TR flips to
Implemented only when its smoke goes green.

## Problem Frame

All 21 TRs in scope are currently Raw — present in the raw OpenAPI capture but
with no committed `metadata/trs/<tr>.yaml` and no normalized baseline, so they
are not observed for drift and the `implement-tr` recipe cannot derive structs
for them. Every wave is therefore a full three-rung lift, heavier than the prior
consumer-bound and discovery/screening waves whose members were already Tracked.
With 21 raw candidates, the tracking step is the cheap uncertainty reducer:
committed metadata and reviewed baselines for the whole cohort let later
implement work proceed from reviewed shapes instead of rediscovering raw
completeness one domain at a time, and surface blockers before any Rust is
written.

## Key Decisions

- **Bulk-track first, then implement.** One upfront tracked-only wave (Wave 0)
  brings all 21 to Tracked, giving drift coverage and reviewed baselines for the
  cohort immediately. Implement waves then flip Tracked→Implemented per domain.
  This reverses the sector-cluster wave's fused single-PR choice (PR #41)
  deliberately: #41 fused the rungs to forge the track-then-implement recipe,
  which is now frozen, and a 21-TR cross-domain cohort is the scale where an
  upfront reviewed-baseline pass pays off rather than rediscovering shapes per
  domain.
- **Implemented is the ceiling for this effort.** No TR is promoted to
  Recommended here; Focused Evidence and recommendation blocks are out of scope.
- **Ship-Tracked / hold-the-flip is the default smoke-fail disposition.** A TR
  flips to Implemented only on a green smoke; otherwise it ships Tracked (or
  callable-but-pending) with an explicit disposition, never a false Implemented
  claim.
- **Justification is read-only breadth, not a per-wave named consumer.** This
  campaign justifies implementation by read-only domain breadth and
  smoke-confirmed callability, not by a per-wave named downstream consumer. The
  capability bar that prior waves applied still governs higher-risk,
  side-effectful, or ambiguous TRs. This is a deliberate rule for the low-risk
  read cohort, not an exception; it needs a superseding ADR only if it becomes the
  default rule for all future SDK expansion.
- **Read-only account-state TRs are in; order/side-effectful account TRs are
  out.** The four CSPAQ/CCENQ/CFOBQ codes are Read-Only TRs. Order, registration,
  and realtime/WebSocket TRs stay held.
- **The recipes are edited to match this support model, not exempted per wave.**
  Read-only account-state REST is now a first-class part of the breadth path, so
  `track-tr` and `implement-tr` are corrected once rather than overridden in each
  account wave's plan. This is a deliberate support-model boundary change — the
  read-only/side-effectful line becomes load-bearing in the recipe gate — handled
  as a recipe edit rather than a heavyweight ADR.

## TR Inventory

The 21 read-only TRs and their wave assignment. Wave 0 tracks all 21; implement
waves flip them per this table. Only the four account TRs depend on the R9
account-recipe edit; every other TR routes through `market_session`. Session-windowed
TRs are smoked only in their window.

| TR | Domain / class | Implement wave | Window |
|----|----------------|----------------|--------|
| CSPAQ12300 | account | 1 — account | anytime |
| CSPAQ22200 | account | 1 — account | anytime |
| CFOBQ10500 | account (F/O deposit) | 1 — account | anytime |
| CCENQ90200 | account (night F/O balance) | 1 — account, in-window | night |
| t2301 | futures/options | 2 — anytime F/O | anytime |
| t2522 | futures/options | 2 — anytime F/O | anytime |
| t8401 | futures/options | 2 — anytime F/O | anytime |
| t8426 | futures/options | 2 — anytime F/O | anytime |
| t8433 | futures/options | 2 — anytime F/O | anytime |
| t8435 | futures/options | 2 — anytime F/O | anytime |
| t8467 | futures/options | 2 — anytime F/O | anytime |
| t9943 | futures/options | 2 — anytime F/O | anytime |
| t9944 | futures/options | 2 — anytime F/O | anytime |
| t8455 | F/O market-data | 2b — night F/O, in-window | night |
| t8460 | F/O market-data | 2b — night F/O, in-window | night |
| t8463 | F/O investor | 2b — night F/O, in-window | night |
| o3101 | overseas-futures | 3 — overseas-futures | spike-gated |
| o3121 | overseas-futures | 3 — overseas-futures | spike-gated |
| g3101 | overseas-stock | 4 — overseas-stock | spike-gated |
| g3104 | overseas-stock | 4 — overseas-stock | spike-gated |
| g3106 | overseas-stock | 4 — overseas-stock | spike-gated |

The F/O account reads (CCENQ90200, CFOBQ10500) may return empty on a position-less
paper account — PENDING is then the expected, non-failing outcome (R5), so neither
should be a wave's ship-floor.

## Requirements

**Wave structure**

- R1. Deliver as a Wave 0 bulk tracked-only expansion followed by sequential
  per-domain implement waves, each implement wave its own PR sized like prior
  waves.
- R2. The rung target is Implemented for every in-scope TR; no TR is promoted to
  Recommended in this effort.
- R3. Wave 0 brings all 21 read-only TRs to Tracked — authoring
  `metadata/trs/<tr>.yaml` + `tr-index.yaml` entries and projecting each
  normalized baseline via `make api-drift-renormalize` — with no callable Rust
  and no Implemented flips.
- R4. Implement waves run in this order, each shipping a green gate: account
  reads → anytime F/O reads → night/session F/O reads → overseas-futures →
  overseas-stock weak-edge anchors.

**Smoke-fail disposition**

- R5. Disposition each TR by its Paper Live Smoke outcome, mirroring the
  `implement-tr` recipe's terminal states so no failure mode falls through:
  - Green (builds, sends, deserializes a non-empty success) → `support.implemented: true`.
  - Empty result (recognized success, empty payload) → record
    `PENDING — empty result, shape unconfirmed`, leave `implemented: false`;
    author callable Rust only when it unblocks a parallel TR in the same domain
    plan, otherwise stay tracked-only.
  - Raw-probe succeeds but SDK deserialize fails → `DROPPED` (TR defect),
    tracked-only with a recorded reason — distinct from PENDING, never treated as
    environmental.
  - Gateway `01900` (paper-incompatible) → keep the TR callable, set
    `paper_incompatible: true`, and apply the recipe's AE2 rule (stays
    `implemented`, not dropped to tracked-only).
  - Closed session or missing input → leave tracked-only, record `HELD`/`PENDING`
    with a follow-up; retry in-window where applicable.
- R6. A wave's gate stays green as long as every non-green TR is explicitly
  dispositioned and no Implemented claim is made for it.
- R7. A single unavailable paper smoke never blocks other ready TRs in the same
  wave, unless the blocked TR was declared the wave's explicit anchor/ship-floor.

**Session-windowed TRs**

- R8. Pre-split known session-blocked TRs into in-window implement sub-waves from
  the start, smoked only within their session window: the account read
  `CCENQ90200` (night-session, account class, depends on R9) and the night F/O
  reads `t8455`, `t8460`, `t8463` (market-data/investor endpoints routed through
  `market_session`, not dependent on the R9 account-recipe edit).

**Preconditions and gate discipline**

- R9. Before Wave 0 tracks any account-state TR, edit the `track-tr` and
  `implement-tr` recipes, which currently scope account-state TRs out
  (`.agents/skills/track-tr/SKILL.md:44`,
  `.agents/skills/implement-tr/SKILL.md:37-39`): `track-tr` permits tracking
  read-only account-state REST TRs; `implement-tr` permits implementing them
  through the account class and account smoke pattern. Order, side-effectful, and
  realtime/WebSocket TRs stay `HELD — out of scope` in both. The `track-tr`
  authoring template must also gain an `account` owner_class branch and set
  `account_state: true` / `rate_bucket: account` for account TRs, with
  `metadata/trs/CSPAQ12200.yaml` as the authoring exemplar — otherwise its
  hardcoded `account_state: false` / `rate_bucket: market_data` defaults produce
  invalid metadata. Ship the recipe edits in a standalone commit (or pre-Wave-0
  PR) with `cargo test -p ls-core` (metadata validation) green before any Wave 0
  authoring begins. The edited gate states the read-only test explicitly: a
  request block with no order number, no registration field, and no mutation field
  is read-only and eligible; anything else stays `HELD`.
- R10. Each wave updates every count-coupled test it perturbs — docgen
  `reference.len()`, the `banner_trs` array, `TRACKED_TRS`, the dependency page
  count, and the `api_drift.rs` `maintained_tr_count` literal — and regenerates
  docs before merge. Wave 0's tracking bump is large (21 TRs at once).
- R11. Numeric request-body fields serialize as JSON numbers
  (`string_as_number`) to avoid gateway `IGW40011`; audit each implemented TR's
  request fields per the existing solution doc.

**Overseas domains**

- R12. Treat overseas gateway/session behavior (o31xx, g31xx) as uncharted:
  Waves 3–4 begin with a bounded raw-probe spike — one `make raw-probe` per
  gateway prefix within a single session window, producing a yes/no reachability
  verdict plus the observed endpoint base — before implement work is planned. If a
  prefix is unreachable on the paper gateway, its wave ships those TRs `HELD`
  rather than blocking the rest of the cohort. g31xx market/symbol identifiers are
  caller-supplied, not prerequisite-producer chains to build.

## Scope Boundaries

**Deferred for later**

- Promotion of any of these TRs to Recommended (Focused Evidence + recommendation
  blocks) — a separate effort after Implemented.
- Building producer-TR discovery chains for g31xx if the spike reveals genuine
  hard prerequisites rather than caller-supplied identifiers.

**Outside this effort**

- Side-effectful, order, registration, and realtime/WebSocket account TRs — only
  read-only account-state reads are in scope.

## Dependencies / Assumptions

- Recipe reconciliation (R9) is a confirmed precondition: both recipes currently
  exclude account-state TRs.
- `CCENQ90200`'s night-session / `krx_extended` requirement comes from the raw
  OpenAPI capture, not yet from any committed ledger or metadata; treat it as an
  assumption to confirm during Wave 0 tracking, and place the TR in the
  session-windowed cohort meanwhile.
- Overseas gateway/session shape for o31xx and g31xx is unverified in the repo;
  Waves 3–4 assume nothing from domestic patterns until the spike confirms.
- Implement and session-windowed waves depend on paper-gateway availability and,
  for night/overseas TRs, smoking within the correct trading window.
- All 21 TRs are confirmed Raw (only `CSPAQ12200`, a sibling code, exists in
  `metadata/trs/`).

## Outstanding Questions

**Deferred to planning**

- The exact anytime-vs-session split inside Wave 2 — confirmed from the reviewed
  baselines produced in Wave 0; any "anytime" F/O TR that returns empty on paper
  drops to PENDING per R5.
- Raw-probe spike outcomes for o31xx/g31xx (endpoint base, session window,
  caller-supplied identifier shape).

## Sources / Research

- `CONCEPTS.md` — Raw / Tracked / Implemented / Recommended ladder, Paper Live
  Smoke, Read-Only vs Side-Effectful TR, Provisionality Ledger.
- `AGENTS.md` — gate sequence, `track-tr` / `implement-tr` / `promote-tr`
  recipes, baseline projection, IGW40011 gotcha.
- `.agents/skills/track-tr/SKILL.md:44`,
  `.agents/skills/implement-tr/SKILL.md:37-39` — current account-out-of-scope
  language (R9).
- `crates/ls-docgen/src/lib.rs` — `reference.len()` (~:879), `banner_trs`
  (~:844), `TRACKED_TRS` (~:677), dependency page count (~:764);
  `crates/ls-trackers/tests/api_drift.rs:105-106` — `maintained_tr_count` (R10).
- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`,
  `crates/ls-sdk/src/paginated/sector_index.rs` — numeric request-field pattern
  (R11).
- `docs/brainstorms/2026-06-23-sector-cluster-raw-to-implemented-wave-requirements.md`,
  `docs/brainstorms/2026-06-22-discovery-screening-implemented-expansion-requirements.md`
  — prior wave patterns (full lift, HELD/PENDING dispositions).
