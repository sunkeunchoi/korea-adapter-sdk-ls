---
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
date: 2026-07-13
type: fix
status: implementation-ready
---

# Throttle-Aware Negative-Probe Classification - Plan

**Product Contract preservation:** Product Contract unchanged — enrichment added
the Planning Contract, Implementation Units, Verification Contract, and Definition
of Done. Outstanding Questions 1–3 are resolved into planning decisions (KTDs)
below; none altered product scope.

## Goal Capsule

- **Objective.** Stop the differential negative-probe from reading an `IGW00201`
  throttle (or any code the gateway did not evaluate on merits) as a genuine
  rejection → false `Clean`. Surface it as `Held` (inconclusive) so a too-tight
  pace fails safe instead of false-passing.
- **Product authority.** Repo owner (this brainstorm). Certification semantics are
  owned by the promote-tr recipe + `metadata/PROVISIONALITY-LEDGER.md`.
- **Open blockers.** Token leg's exact genuine reject codes must be confirmed
  before the token site can flip without regression (see Outstanding Questions).

## Product Contract

### Problem

The differential negative-probe derives, per variant:

```
variant_rejected = !(http 2xx && is_success(rsp_cd))   // negative_probe.rs:377
outcome          = classify_probe(control_ok, variant_rejected)
```

`is_success` matches only `"" | "00000" | "00136" | "00707"`. Every other code —
including `IGW00201`, a warm-sensitive *cumulative* throttle that means "the
gateway never evaluated this variant" — is treated as `variant_rejected = true`
→ **`Clean`**. The throttle (non-evaluation) is conflated with a genuine
constraint rejection.

Consequence (ledger §27 reason A): t8412's control passed but all 11 variants
tripped `IGW00201`; all read false-`Clean`, so the differential was never
exercised on merits. A residual throttle on any of the five read legs
(t8412 / t1101 / t1102 / CSPAQ12200 / t0425) still reads `Clean` today — a
too-tight pace silently certifies rather than surfacing as inconclusive.

### Chosen approach: merits-allowlist inversion

A variant is **conclusive** (`Clean` or `Divergent`) only when the gateway
evaluated it on merits — a recognized success code **or** an evidence-seeded
merits-reject code. Every other outcome (throttle, hard-gateway, unknown code,
transport failure) falls to **`Held`**.

This flips the default from *unknown code = rejection = Clean* to *unknown code =
inconclusive = Held*, which is the fail-safe direction for a certification gate:
a false-`Held` costs only a re-probe; a false-`Clean` wrongly certifies a bound.

### In scope

- **Read helper** (`crates/ls-sdk/tests/negative_probe.rs:378`,
  `run_inblock_negative_probe`) — carries all five named legs.
- **Token leg** (`crates/ls-sdk/tests/negative_probe.rs:598`).
- **Merits-code core in `ls-core`** (beside `is_success` / `error_catalog`),
  called by both the live probe loops and the offline twin so the two cannot
  drift. It must distinguish three variant verdicts — accepted / merits-rejected /
  inconclusive — with **inconclusive → `Held`**. `classify_probe` (or its inputs)
  is extended to consume this three-way verdict instead of the current
  `variant_rejected: bool`.
- **Merits-reject set, seeded from observed evidence** (not guessed): `IGW40011`
  and `IGW40013` for the reads; the token leg's genuine auth-reject codes
  (candidates `IGW00002` / `IGW00121` per `auth.rs`) once confirmed. This seed is
  load-bearing — see Disposition Impact.
- **`IGW00201` → inconclusive → `Held`**, rendered as a reason-qualified label
  (e.g. `Held-throttle`), mirroring the order path's existing `Held-may-rest`
  label (`negative_probe.rs:1445`). The `ProbeOutcome` enum stays three-valued;
  the qualifier is a render-layer label like the existing `expected-tolerant`.
- **Promotion rule.** A `Held-throttle` variant line blocks promotion / forces a
  re-probe, exactly like a control-fail `Held`. The promote-tr recipe and any
  operator/agent reading NEG-PROBE lines treat any `Held*` as non-certifying.
- **Offline unit tests** over the new merits-code predicate and the three-way
  `classify_probe` inputs (an `IGW00201` variant with a passing control asserts
  `Held`, not `Clean`), extending the existing offline proxy at
  `negative_probe.rs:107`.

### Out of scope

- **Order path** (`negative_probe.rs:1477`, CSPAT006/007/008). It runs its own
  `FiredVariantOutcome` classifier (Accepted / MayHaveRested→`Held-may-rest` /
  PlacedNothing) upstream of `classify_probe(control_ok, true)`. Folding
  throttle-`Held` into that safety-critical order-teardown surface needs separate
  per-code evidence; documented as a follow-up.
- **Live runtime dispatch.** `classify_probe` is probe/test-only — no `Inner::post`
  / `dispatch_once` consumer reads it (verified: callers are `negative_probe.rs`
  lines 378/598/1477 plus test asserts). No runtime behavior changes.
- **Pace constants** (`T8412_PROBE_PACE` = 1000 ms, account-lane 1500 ms, etc.).
  This guard is orthogonal to pacing; it makes a residual throttle *visible*, it
  does not try to prevent one (the throttle budget is cumulative and cannot be
  guaranteed cool by any per-dispatch pace — see the
  `igw00201-budget-characterization` learning).

### Disposition impact

**With the seeded merits-reject set, no certified disposition changes.** Every
current `Clean` verdict rests on either a success code or a seeded merits-reject
code:

- Reads: t1102 / t1101 variants returned `00000` (downgraded `expected-tolerant`)
  or `IGW40011` rejects — all merits. **t0425's `sortgb/required → IGW40013 →
  Clean`** (ledger lines 1699, 1996) is the "gateway-enforced negative anchor" of
  a now-`recommended` TR; `IGW40013` **must** be in the merits-reject seed or this
  anchor regresses `Clean → Held`. This is the single most important seeding
  constraint.
- t8412 stays legitimately `Held` — the guard formalizes what §27 caught by hand.
- **Token leg regression risk.** Token variants return auth-reject codes, not
  `IGW40011`. If those codes are not seeded, the token leg's `Clean` verdicts flip
  to `Held`, regressing a *recommended* disposition. Bringing token in-scope
  therefore requires enumerating its genuine reject codes into the merits set
  first (Outstanding Question below).

### Success criteria / acceptance signals

- An `IGW00201` variant against a passing control classifies `Held` (offline unit
  test), where today it classifies `Clean`.
- The five read legs and the token leg, re-run live in-window with a *deliberately
  too-tight pace*, surface `Held-throttle` lines instead of all-`Clean` — the
  inconclusive result is visible to the operator.
- Re-running the existing certified legs at their real paces reproduces their
  prior `Clean` / `expected-tolerant` dispositions unchanged (no regression).
- The merits-code sets live in one `ls-core` location consumed by both live and
  offline paths (drift-proof), with the seed contents justified per-code by
  observed evidence.

### Outstanding Questions

1. **Token reject model (not just codes).** The token endpoint fails
   *structurally differently* from the InBlock reads (see `auth.rs:160`): a
   genuine OAuth rejection surfaces as either a **non-2xx HTTP status** (the
   `/oauth2/token` server refuses `bad grant_type` / `bad scope` / missing
   `appkey` at the HTTP layer, `auth.rs:173`) **or** a 2xx carrying a
   `{ code, message }` envelope whose `code` is a non-success OAuth code
   (`check_envelope`, `auth.rs:248`; codes like `IGW00002` / `IGW00121`). Both are
   merits evaluations. So the token leg does **not** reuse the read leg's
   `is_success(rsp_cd)` allowlist as-is — its merits-vs-inconclusive split must
   treat HTTP-4xx and non-success envelope codes as genuine rejects (`Clean`) and
   carve out only `IGW00201` / transport / 5xx as inconclusive (`Held`). Confirm
   the exact reject codes/statuses via a token negative-probe run before flipping
   the token site. Until confirmed, the token site flip is blocked; the read-helper
   flip is not.
2. **Render label spelling.** `Held-throttle` vs `Held-inconclusive` (or a broader
   `Held-noneval`) for the reason-qualified label. Cosmetic; pick during planning
   to match the `Held-may-rest` precedent.
3. **Merits-reject set representation.** Positive allowlist of merits codes vs a
   closed inconclusive set with everything else merits — the chosen inversion
   means unknown → `Held`, so the merits set is the allowlist; confirm no
   in-scope leg emits a genuine reject code outside `{IGW40011, IGW40013, token
   auth codes}` (else that leg becomes un-certifiable).

All three are resolved into KTDs below (KTD1 render label, KTD2 reads/token split,
KTD3 seed contents). OQ1's live token-code confirmation is carried as an execution
note on U3, not a planning blocker.

---

## Planning Contract

### Key Technical Decisions

- **KTD1 — Reuse `Held`; extend `classify_probe` to a 3-way verdict, no 4th
  outcome.** Replace `classify_probe(control_succeeded: bool, variant_rejected:
  bool)` with `classify_probe(control_succeeded: bool, verdict: VariantVerdict)`
  where `VariantVerdict { Accepted, Rejected, Inconclusive }` maps: control-fail →
  `Held`; `Accepted` → `Divergent`; `Rejected` → `Clean`; `Inconclusive` → `Held`.
  `ProbeOutcome` stays three-valued. A throttle-`Held` renders as the reason-
  qualified label `Held-throttle`, following the order path's existing
  `Held-may-rest` label (`negative_probe.rs:1445`) and the `expected-tolerant`
  render-layer precedent — the operator distinguishes it from a control-fail
  `Held` by the label + the printed `rsp_cd`. Rationale: `Held` already means
  "inconclusive"; §27 categorizes `Held` by reason (A/B/C) editorially, not by
  enum, so no new variant is warranted.

- **KTD2 — Reads and token use different reject models, one shared principle.**
  The principle: *inconclusive = the gateway did not evaluate the variant on
  merits.* Reads realize it with a **positive merits-reject allowlist** (strict
  inversion): `Accepted` iff `2xx && is_success`; `Rejected` iff `rsp_cd ∈
  {IGW40011, IGW40013}`; **everything else → `Inconclusive` → `Held`**, so an
  *unknown* read reject code surfaces loud and re-probeable rather than silent
  `Clean`. Token realizes it with a **noneval carve-out**: `is_noneval_code(rsp_cd)`
  → `Inconclusive`; otherwise its existing `ok` (HTTP-2xx + success envelope) →
  `Accepted`/`Rejected`, so genuine OAuth refusals (non-2xx HTTP, or a non-success
  `{code,message}` envelope) stay `Clean`. Rationale: reads have a small, fully-
  characterized reject vocabulary and are the throttle-masking motivating case
  (§27 reason A); token's OAuth-refusal vocabulary cannot be allowlisted and is
  not the motivating risk. The token guard is therefore weaker (catches only
  catalogued noneval codes) — recorded, accepted.

- **KTD3 — Deliberately narrow seeds, evidence-backed.** `is_noneval_code` is
  seeded **`{IGW00201}` only** and extended solely with per-code evidence,
  mirroring `is_ingress_validation_reject`'s IGW40011-only discipline
  (`error_catalog.rs:82`). The read merits-reject allowlist is seeded
  **`{IGW40011, IGW40013}`** — both observed as genuine rejects in live probes
  (`IGW40011` per `error_catalog`; `IGW40013` per ledger `t0425.sortgb/required →
  IGW40013 → Clean`, lines 1699/1996). `IGW40013` **must** be in the set or
  t0425's certified `sortgb` anchor regresses `Clean → Held`. **Note the
  deliberate split:** `error_catalog.rs:80` groups `IGW40013` and `IGW50008` as
  one "generic hard gateway failure" category, but on the *read* path they are
  split by evidence — `IGW40013` is a merits reject (Clean, per the t0425 anchor)
  while `IGW50008` stays inconclusive (Held). The "hard-gateway → Held" framing in
  the approach refers to `IGW50008`-class codes; do **not** drop `IGW40013` from
  the seed by reading it as a hard-gateway code. Transport failure
  (`None`) already routes to `Held` today (`negative_probe.rs:389`) and stays so —
  it is an `Inconclusive` by construction.

- **KTD4 — Behavior-preserving refactor first, behavior change second.** A Rust
  signature change is atomic: all three `classify_probe` callers (378 read, 598
  token, 1477 order) must update in the same commit to keep the tree green. U1
  makes that change behavior-preservingly — each caller maps its current boolean
  to `Accepted`/`Rejected` with **zero disposition change** — then U2 (reads) and
  U3 (token) layer the `Inconclusive` branch on top. This keeps each commit green
  (AGENTS.md gate) and separates the mechanical refactor from the reviewable
  behavior change.

- **KTD5 — Shared decision core in `ls-core`, twinned offline; derivation
  factored, not inlined.** `classify_probe`, `is_noneval_code`, and
  `is_read_merits_reject` live in `crates/ls-core/src/preflight.rs` with offline
  unit tests. Beyond the decision core, each leg's `VariantVerdict` **derivation**
  (transport-None → `is_success` → merits-allowlist → else `Inconclusive`) is
  factored into a single shared helper per leg (e.g. `read_variant_verdict(http,
  rsp_cd)` / `token_variant_verdict(...)` in the test module) that **both** the
  live loop and the offline proxy call — mirroring how `reported_outcome` /
  `is_gateway_tolerant` are already shared. This matters because the drift-proof
  guarantee must cover the *branch ordering*, not just the code sets: today the
  offline proxy (`negative_probe.rs:107`) calls `classify_probe` with hand-picked
  booleans, so asserting "`IGW00201` variant → `Held`" without a shared derivation
  helper would re-implement the branch logic in the test and let it silently
  diverge from the live site.

### High-Level Technical Design

Core decision table (`classify_probe`):

| `control_succeeded` | `verdict` | `ProbeOutcome` | render label |
|---|---|---|---|
| `false` | any | `Held` | `Held` (control-fail) |
| `true` | `Accepted` | `Divergent` | `Divergent` / `expected-tolerant` |
| `true` | `Rejected` | `Clean` | `Clean` |
| `true` | `Inconclusive` | `Held` | `Held-throttle` |

Per-leg `VariantVerdict` derivation (the only part that differs by leg):

```text
READ leg (line 378) — strict inversion:
  transport None                 -> Inconclusive
  2xx && is_success(rsp_cd)      -> Accepted
  rsp_cd in {IGW40011, IGW40013} -> Rejected          (merits-reject allowlist)
  otherwise                      -> Inconclusive       (incl. IGW00201, unknown)

TOKEN leg (line 598) — noneval carve-out:
  transport None                 -> Inconclusive
  is_noneval_code(rsp_cd)        -> Inconclusive       ({IGW00201})
  ok  (2xx && success envelope)  -> Accepted
  otherwise                      -> Rejected           (real OAuth refusals: Clean)

ORDER leg (line 1477) — UNCHANGED behavior:
  FiredVariantOutcome::PlacedNothing -> Rejected        (was `true`)
```

Directional guidance, not implementation specification — exact helper names and
the `is_success` reuse-vs-duplicate call are U1/U2 implementer choices. Each
leg's block above is realized as one shared derivation helper (KTD5) called by
both the live loop and the offline proxy, not duplicated inline.

---

## Implementation Units

### U1. Extend the classification core in `ls-core` (behavior-preserving)

- **Goal.** Introduce the 3-way `VariantVerdict` and the throttle predicate in
  `ls-core`, change `classify_probe`'s signature, and update all three callers to
  compile — with **no disposition change** on any leg.
- **Requirements.** Enables the merits-allowlist inversion (Product Contract
  "Chosen approach"); realizes KTD1, KTD4, KTD5.
- **Dependencies.** none.
- **Files.**
  - `crates/ls-core/src/preflight.rs` — add `pub enum VariantVerdict { Accepted,
    Rejected, Inconclusive }`; change `classify_probe` to the 3-way signature per
    the KTD1 table; add `pub fn is_noneval_code(rsp_cd: &str) -> bool` seeded
    `{IGW00201}` with the narrow-by-design doc-comment; update the existing
    `classify_probe` unit tests + add `Inconclusive → Held` and `is_noneval_code`
    coverage.
  - `crates/ls-core/src/lib.rs` — re-export `VariantVerdict` and `is_noneval_code`
    alongside the existing `ProbeOutcome` / `classify_probe` re-exports.
  - `crates/ls-sdk/tests/negative_probe.rs` — update the three callers to the new
    signature preserving behavior: read (378) maps
    `!(2xx && is_success) ? Rejected : Accepted`; token (598) maps
    `ok ? Accepted : Rejected`; order (1477) passes `VariantVerdict::Rejected`
    where it passed `true`. Update the offline proxy asserts (107–109) to the new
    signature.
- **Approach.** Pure mechanical widening — `bool` → a 3-value enum with the third
  value unused by any caller yet. The `Inconclusive` arm exists in
  `classify_probe` and is unit-tested in `ls-core`, but no call site produces it
  until U2/U3, so every leg's live disposition is byte-identical to today.
- **Execution note.** Land as a behavior-preserving refactor; the offline gate
  (`cargo test`, `cargo test -p ls-core`) must stay green with zero probe-output
  change. Do not add a call-site `Inconclusive` branch in this unit.
- **Patterns to follow.** `is_ingress_validation_reject` (`error_catalog.rs:82`)
  for the narrow-predicate doc-comment discipline; the existing `ProbeOutcome`
  enum + doc-comments in `preflight.rs`.
- **Test scenarios.**
  - `classify_probe(false, Inconclusive)` → `Held`; `(false, Accepted)` → `Held`;
    `(false, Rejected)` → `Held` (control-fail dominates regardless of verdict).
  - `classify_probe(true, Accepted)` → `Divergent`; `(true, Rejected)` → `Clean`;
    `(true, Inconclusive)` → `Held`.
  - `is_noneval_code("IGW00201")` → true; `is_noneval_code` false for
    `["", "00000", "00136", "IGW40011", "IGW40013", "IGW40014", "IGW50008"]`
    (deliberately narrow — one code).
  - Offline proxy (`negative_probe.rs:107`) recompiles and its existing
    assertions hold under the new signature.
- **Verification.** `cargo test -p ls-core` and `cargo test` green; no NEG-PROBE
  output byte-changes (the three legs still map to their prior outcomes).

### U2. Read helper — merits-allowlist inversion + `Held-throttle`

- **Goal.** Make the shared read helper (all five named legs) route a non-merits
  variant — `IGW00201`, hard-gateway, unknown — to `Held-throttle` instead of
  false-`Clean`, while preserving every current `Clean`/`expected-tolerant`.
- **Requirements.** Product Contract in-scope bullets 1, 4, 5, 6; realizes KTD2
  (read arm), KTD3 (merits-reject seed).
- **Dependencies.** U1.
- **Files.**
  - `crates/ls-sdk/tests/negative_probe.rs` — factor the read `VariantVerdict`
    derivation (HTD read block: `Accepted` on `2xx && is_success`; `Rejected` on
    the merits-reject allowlist `{IGW40011, IGW40013}`; else `Inconclusive`) into a
    shared `read_variant_verdict(http, rsp_cd)` helper (KTD5), and call it from
    both `run_inblock_negative_probe` (line 371–392, replacing the
    `variant_rejected` boolean) and the offline proxy. Route the `Inconclusive`
    render through a `Held-throttle` label (extend `reported_outcome` or the print
    site). Extend the offline proxy to drive `read_variant_verdict` with an
    `IGW00201` result and assert `classify_probe` → `Held`.
  - `crates/ls-core/src/preflight.rs` — add `pub fn is_read_merits_reject(rsp_cd:
    &str) -> bool` seeded `{IGW40011, IGW40013}` (may delegate `IGW40011` to
    `is_ingress_validation_reject`), with unit tests; keeps the seed offline-
    twinnable and co-located with `is_noneval_code`.
  - `crates/ls-core/src/lib.rs` — re-export `is_read_merits_reject` (the U1
    re-export bullet covers only `VariantVerdict` / `is_noneval_code`; without this
    the `ls-sdk` test referencing `ls_core::is_read_merits_reject` fails to
    compile and the DoD's "all three re-exported" is unmet).
- **Approach.** The `rsp_cd` carries the merits signal independent of HTTP status
  (a genuine `IGW40011` ingress reject arrives `http=500` per CONCEPTS "Ambiguous
  order outcome"), so `Rejected` keys on `rsp_cd`, not the 2xx gate; only
  `Accepted` requires 2xx. Transport `None` continues to print `outcome=Held`
  (now semantically `Inconclusive`). The `Held-throttle` label is display-only;
  the promotion rule (below) reads it.
- **Execution note.** Verify no regression by reasoning the seed against ledger
  §27/§30: t1102 (`00000`/`IGW40011`), t1101, t0425 (`IGW40013` sortgb) all stay
  `Clean`/`expected-tolerant`; t8412's `IGW00201` variants flip false-`Clean` →
  `Held-throttle`.
- **Patterns to follow.** The existing `reported_outcome` tolerance-layer
  (`negative_probe.rs:170`) for adding a render-label over a raw `ProbeOutcome`
  without touching `classify_probe`; the `Held-may-rest` label string
  (`negative_probe.rs:1445`).
- **Test scenarios.**
  - Offline proxy: control `Accepted` + variant `is_noneval_code` (`IGW00201`) →
    `classify_probe` `Held`, rendered `Held-throttle`. Covers the §27 reason-A
    false-`Clean`.
  - Offline: control `Accepted` + variant `IGW40011` → `Clean`; + variant
    `IGW40013` → `Clean` (t0425 sortgb anchor preserved).
  - Offline: control `Accepted` + variant `2xx && 00000` → `Divergent` (accepted
    invalid) — unchanged.
  - Offline: control `Accepted` + variant unknown code (e.g. `"40510"`) → `Held`
    (strict inversion: unknown read reject is now inconclusive, not `Clean`).
  - `is_read_merits_reject` true for `IGW40011`/`IGW40013`, false for
    `IGW00201`/`00000`/unknown.
  - `Held-throttle` label renders only for a noneval `Inconclusive`, not for a
    control-fail `Held`.
- **Verification.** `cargo test` green; the offline proxy proves an `IGW00201`
  variant reads `Held`, where before U1/U2 it read `Clean`. Live proof deferred
  to the operator re-probe (legs are `#[ignore]`).

### U3. Token leg — noneval carve-out + `Held-throttle`

- **Goal.** Apply the same throttle-inconclusive guard to the token negative
  probe via the noneval carve-out, without regressing token's certified `Clean`
  dispositions (genuine OAuth refusals).
- **Requirements.** Product Contract in-scope bullet 2; realizes KTD2 (token arm).
- **Dependencies.** U1.
- **Files.**
  - `crates/ls-sdk/tests/negative_probe.rs` — factor the token `VariantVerdict`
    derivation (HTD token block: `is_noneval_code(rsp_cd)` → `Inconclusive`; else
    `ok ? Accepted : Rejected`) into a shared `token_variant_verdict(...)` helper
    (KTD5) called by both the token variant loop (line 595–608) and its offline
    proxy assertion. Render `Held-throttle` for the `Inconclusive` case.
- **Approach.** Token's genuine-refusal signal is HTTP-4xx / non-success envelope
  code (`auth.rs:173`/`248`), already collapsed into `ok`; the only carve-out is
  the shared `is_noneval_code`. This keeps the exact current `Accepted`/`Rejected`
  split for every real auth outcome and only diverts a throttle to `Held`.
- **Execution note.** OQ1 — confirm via a token negative-probe run (operator, in
  session) that token's variant reject codes (`bad grant_type`, `bad scope`,
  removed `appkey`/`appsecretkey`) are HTTP-4xx / envelope codes (candidates
  `IGW00002`/`IGW00121`) and that none is `IGW00201` under normal pacing — so the
  carve-out changes nothing for genuine refusals. This is an execution-time
  verification, not a planning blocker; the offline change is safe by
  construction (only `IGW00201`/transport divert).
- **Patterns to follow.** Same `Held-throttle` render as U2; `check_envelope`
  semantics (`auth.rs:248`) for what counts as a token success vs refusal.
- **Test scenarios.**
  - Offline: control `ok` + token variant `is_noneval_code` (`IGW00201`) →
    `Held` (`Held-throttle`).
  - Offline: control `ok` + token variant `ok=false` with a non-noneval code
    (e.g. `IGW00121`) → `Clean` (genuine OAuth refusal unchanged).
  - Offline: control `ok` + token variant `ok=true` → `Divergent` (invalid
    accepted) — unchanged.
  - Transport `None` → `Held` — unchanged.
- **Verification.** `cargo test` green; offline proof that an `IGW00201` token
  variant reads `Held` while an auth-reject code still reads `Clean`. Live
  confirmation of token's actual reject codes is the operator re-probe (OQ1).

---

## Verification Contract

- **Offline gate (CI-enforced, every unit):** `make docs && cargo test &&
  cargo test -p ls-core && make docs-check && make lane-check` all green
  (AGENTS.md gate). No `metadata/` or docgen changes are expected — this is
  test-harness + `ls-core` logic only.
- **No-regression proof (offline):** the extended offline proxy asserts each
  currently-certified variant outcome is preserved (success → `Divergent` on
  acceptance, `IGW40011`/`IGW40013` → `Clean`) and that `IGW00201` → `Held`.
- **Live proof (operator-run, deferred):** the in-window re-probe of the five
  read legs + token (`make live-smoke-<tr>-negative`, `#[ignore]`) surfaces
  `Held-throttle` under a deliberately-too-tight pace instead of all-`Clean`, and
  reproduces prior dispositions at real paces. Agents run read legs; the
  throttle-visibility proof is operator-gated.
- **Promotion rule (operator/recipe):** any `Held*` variant line — including
  `Held-throttle` — is non-certifying and blocks promotion / forces a re-probe,
  same as a control-fail `Held`. The promote-tr recipe's "all Clean/expected-
  tolerant" reading already implies this; the new label just makes the throttle
  case visible.

## Definition of Done

- `classify_probe` takes `VariantVerdict`; `is_noneval_code` and
  `is_read_merits_reject` exist in `ls-core` with offline unit tests; all three
  re-exported.
- Read helper and token leg derive a 3-way verdict and render `Held-throttle` for
  a noneval `Inconclusive`; the order caller compiles unchanged in behavior.
- Offline proxy proves `IGW00201` → `Held` and preserves every currently-certified
  disposition (t0425 `IGW40013` anchor included).
- Full offline gate green; no docgen/metadata diff.
- OQ1 (token reject codes) recorded as the one operator execution-time check; the
  order path is documented as a follow-up, not implemented here.

---

## Sources & Research

- `crates/ls-core/src/preflight.rs:468` — `ProbeOutcome`, `classify_probe`.
- `crates/ls-sdk/tests/negative_probe.rs` — call sites 378 (read), 598 (token),
  1477 (order); `is_success` (133), `reported_outcome` (170), `Held-may-rest`
  label (1445), `T8412_PROBE_PACE` note (188).
- `crates/ls-core/src/error_catalog.rs:82` — `is_ingress_validation_reject`
  narrow-predicate discipline; `IGW00201`/`IGW40013`/`IGW50008` classification
  (`:80` groups `IGW40013`+`IGW50008`, split by evidence on the read path — KTD3).
- `crates/ls-core/src/inner.rs:79` — `rsp_cd_is_success` read-success set.
- `crates/ls-core/src/auth.rs:160` — token failure model (HTTP-4xx +
  `check_envelope`).
- `metadata/PROVISIONALITY-LEDGER.md` §27 reason A (1685) — t8412 throttle-masked
  false-`Clean`; §30 (1699/1996) — `t0425.sortgb → IGW40013 → Clean` anchor.
- `CONCEPTS.md` "Differential negative probe" (56), "Ambiguous order outcome"
  (130) — the "classify by whether it could rest, not HTTP status" principle this
  extends to reads; `IGW00201` stays inconclusive.
- `docs/solutions/…/order-error-classifier-placed-nothing-vs-may-rest.md`,
  `…/gateway-tolerant-facet-preserves-preflight-while-unblocking-differential-probe.md`,
  `…/ls-gateway-igw00201-*` — related classification + throttle-budget learnings.
