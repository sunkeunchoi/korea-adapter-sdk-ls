---
title: "fix: IGW40011-as-500 is placed-nothing on the order type-variant differential (probe + live path)"
date: 2026-07-07
type: fix
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
execution: code
product_contract_source: ce-plan-bootstrap
depth: standard
---

# fix: IGW40011-as-500 is placed-nothing on the order type-variant differential

## Summary

The order negative-probe halts its type/required variant differential the instant a
malformed numeric field is rejected with `IGW40011` (a gateway **ingress** input-
validation reject that arrives as `http=500`), because the probe's fire loop treats
**any** `5xx` as "may-have-rested". This blocks the §27 order quartet
(CSPAT00601/00701/00801) from certifying. The same defect exists on the **live order
path**: `ls-core` `dispatch_once` maps every non-2xx order outcome — including
`IGW40011@500` — to `LsError::AmbiguousOrder` → `SubmitAction::Pending` (may-rest),
even though an ingress-rejected request never routes to the exchange and structurally
cannot rest.

Fix both seams narrowly: `IGW40011` is a definitive **placed-nothing** rejection.
Every other `5xx`/non-2xx and every transport failure stays may-rest (fail-closed
default preserved). Encode "which `rsp_cd` is an ingress reject" **once** in `ls-core`
so the offline probe and the live path can never drift.

**Scope decision (user-confirmed):** this PR fixes **both** the offline probe
classifier *and* the real live-order classifier. The live fix lands at the
`ls-core` `dispatch_once` variant-selection seam (`crates/ls-core/src/inner.rs`), **not**
in `adapters/nautilus` `classify_submit_error` — that function is deliberately
variant-keyed ("never `rsp_cd` alone — the documented fail-open trap", `orders/map.rs`
header) and already maps `ApiError`→`Reject` correctly. Making `IGW40011@500` surface
as `ApiError` (instead of `AmbiguousOrder`) at the dispatch seam is the correct,
minimal change and needs **no** edit to `classify_submit_error`.

**Not in scope / fail-closed preserved:** the `None` transport-failure arm; every
non-`IGW40011` `5xx`/non-2xx order outcome (stays `AmbiguousOrder`/may-rest); any live
order placement (attended open-KRX operator blocker — see Verification).

---

## Problem Frame

`crates/ls-sdk/tests/negative_probe.rs` `run_order_negative_probe` fires malformed
`type`/`required` variants against a live resting control and classifies each result.
The fire loop (`negative_probe.rs:1417`) has this arm:

```
Some((http, rsp_cd, _)) if http >= 500 => { ...Held-may-rest halt=true; teardown; return }
```

The `type` variant deliberately sends a malformed numeric field; the gateway correctly
rejects it with `IGW40011`, which arrives as `http=500`. The arm treats it as
may-have-rested and **halts the whole differential** before it completes:

- CSPAT00601: `IsuNo/required` → `01407` → Clean; `OrdQty/type` → `IGW40011 (500)` → halt.
- CSPAT00701/00801: `OrgOrdNo/type` → `IGW40011 (500)` → halt.

So all three quartet TRs never certify.

The **read** probe (`run_inblock_negative_probe`) has *no* may-rest halt arm at all: a
rejected variant (including `IGW40011@500`) simply classifies `Clean` and continues.
That asymmetry — the order path is over-conservative *only* because order endpoints
*can* place — is the root of the bug. `IGW40011` specifically **can't** place: it is
an ingress-validation reject, rejected before routing to the exchange.

The live-order path has the same over-conservatism at its root. In
`crates/ls-core/src/inner.rs` `dispatch_once`, the order non-2xx branch
(`inner.rs:335`) returns `LsError::AmbiguousOrder` for **any** non-2xx order response
"regardless of the body", which `classify_submit_error` (`adapters/nautilus/src/orders/map.rs`)
maps to `SubmitAction::Pending` (may-rest → reconcile). The existing
`post_order_does_not_cache_rejections` test only exercises `IGW40011` at `http=200`
(where `classify_order_rsp_cd`→`Rejected`→`ApiError` already works); the live `500`
case is uncovered and mis-classified.

---

## Requirements

- **R1** — In the order negative-probe fire loop, `http >= 500 && rsp_cd == "IGW40011"`
  classifies as `Clean` (placed-nothing) and **continues** firing variants, letting the
  type-variant differential complete.
- **R2** — Every **other** `5xx` and the `None` transport-failure arm stay may-rest/halt
  (fail-closed default preserved). A 2xx order-acceptance ack still trips `WAVE BLOCKED`,
  unchanged.
- **R3** — The probe's `5xx`/`IGW40011` classification decision is extracted into a
  **pure** function with an offline unit test, mirroring the existing pure+tested helpers
  (`scan_page_is_terminal`, `classify_control_disposition`).
- **R4** — On the live order path, `IGW40011@500` surfaces as
  `LsError::ApiError` (definitive placed-nothing rejection) rather than
  `LsError::AmbiguousOrder`, so `classify_submit_error` yields `Reject` not `Pending`.
- **R5** — Every other non-2xx order outcome still surfaces as `AmbiguousOrder`
  (may-rest → reconcile). The order-safety §1/§3 invariant is narrowed for `IGW40011`
  only, never removed.
- **R6** — "Which `rsp_cd` is a definitive ingress reject" is defined **once** in
  `ls-core` (single source of truth) and consumed by both the probe classifier and the
  live dispatch seam, so they cannot drift.
- **R7** — The disposition is filed in `metadata/PROVISIONALITY-LEDGER.md` as a new §29,
  recording the live re-probe of the §27 quartet as the remaining operator blocker.

---

## High-Level Technical Design

`IGW40011@500` is the only outcome whose classification changes; the decision at both
seams is a narrow exemption layered over the existing fail-closed default.

**Probe fire loop — `classify_fired_variant(http, rsp_cd)`** (three outcomes):

| `http`   | `rsp_cd`                     | Outcome           | Action                     |
|----------|------------------------------|-------------------|----------------------------|
| 2xx      | order-ack (`00039`/`00040`/…)| `Accepted`        | WAVE BLOCKED (unchanged)   |
| ≥ 500    | `IGW40011`                   | `PlacedNothing`   | **Clean → continue** (new) |
| ≥ 500    | anything else                | `MayHaveRested`   | halt + reconcile (unchanged)|
| 2xx/4xx  | non-ack business reject      | `PlacedNothing`   | Clean → continue (unchanged)|
| (transport failure `None`)   |              | —                 | halt + reconcile (unchanged, separate arm)|

**Live dispatch — `inner.rs:335` order non-2xx branch:**

```
non-2xx order response
    ├─ is_ingress_validation_reject(rsp_cd)  → LsError::ApiError   → Reject   (new: IGW40011 only)
    └─ otherwise                             → LsError::AmbiguousOrder → Pending (unchanged)
```

Single source of truth: `is_ingress_validation_reject(rsp_cd) -> bool` in `ls-core`
(currently `rsp_cd == "IGW40011"`), consumed by both `dispatch_once` and the probe's
`classify_fired_variant`. Directly calling the shared predicate is stronger than the
mirrored-constant crosscheck used for the ack set — no drift is possible.

---

## Key Technical Decisions

- **KTD1 — Live fix lands at the dispatch variant-selection seam, not `classify_submit_error`.**
  `classify_submit_error` is keyed on the `LsError` **variant** by explicit design
  (`orders/map.rs` header calls `rsp_cd`-sniffing "the documented fail-open trap two
  reviewers caught"). The right place to distinguish `IGW40011`-placed-nothing from a
  genuinely-ambiguous 5xx is where the variant is *chosen* — `dispatch_once`'s order
  non-2xx branch. Making `IGW40011@500` an `ApiError` means the existing
  `ApiError`→`Reject` mapping does the rest; `classify_submit_error` is untouched.
- **KTD2 — Exempt exactly `IGW40011`, nothing else.** `IGW40011` is the confirmed
  ingress numeric-field validation reject (AGENTS.md; `error_catalog`). Sibling `IGW*`
  codes (`IGW40013`, `IGW00201`, …) are not confirmed pre-routing rejects, so they stay
  may-rest. The predicate is deliberately narrow and documented as such.
- **KTD3 — One predicate in `ls-core`, consumed by both seams (R6).** Prevents the probe
  and the live path from disagreeing on which codes are placed-nothing — the exact drift
  class the codebase already guards against for the order-ack set.
- **KTD4 — Preserve the pure+tested-helper convention.** The probe decision is a pure
  `classify_fired_variant` with an offline unit test (mirrors `scan_page_is_terminal`);
  the `ls-core` predicate is pure with its own unit test plus mock-server dispatch
  regression tests for both the `IGW40011@500`→`ApiError` and other-5xx→`AmbiguousOrder`
  cases.

---

## Implementation Units

### U1. `ls-core` canonical ingress-reject predicate (single source of truth)

**Goal:** Introduce `is_ingress_validation_reject(rsp_cd) -> bool` in `ls-core` —
`true` only for `IGW40011` — with doc comment stating it means the gateway rejected the
request at ingress *before* routing to the exchange (placed nothing), and that it is
deliberately narrow.

**Requirements:** R6.
**Dependencies:** none.
**Files:**
- `crates/ls-core/src/inner.rs` (or the module where `classify_order_rsp_cd` lives) — add the pure predicate + `pub(crate)` or `pub` as needed for the probe to reach it.
- Same file, `#[cfg(test)]` — unit test.

**Approach:** Pure `fn`, no allocation. Expose it where `crates/ls-sdk/tests/negative_probe.rs`
can call it (the probe already imports `ls_core::…`); a `pub fn` on `ls-core` is
acceptable, kept minimal and documented as an internal-classification helper.

**Patterns to follow:** `classify_order_rsp_cd` / `rsp_cd_is_order_success` in
`crates/ls-core/src/inner.rs` (pure `rsp_cd` predicates with doc comments).

**Test scenarios:**
- `is_ingress_validation_reject("IGW40011")` is `true`.
- `is_ingress_validation_reject("IGW40013")`, `"IGW00201"`, `"40510"`, `"00040"`, `""` are all `false` (deliberately narrow).

**Verification:** `cargo test -p ls-core` passes; the predicate is referenced by U2 and U3.

### U2. Live order path: `IGW40011@500` → `ApiError` (placed-nothing), not `AmbiguousOrder`

**Goal:** In `dispatch_once`'s order non-2xx branch, return `LsError::ApiError` when the
body `rsp_cd` is an ingress-validation reject; keep `AmbiguousOrder` for every other
non-2xx order outcome. This makes `classify_submit_error` yield `Reject` (placed-nothing)
for `IGW40011` with no change to that function.

**Requirements:** R4, R5.
**Dependencies:** U1.
**Files:**
- `crates/ls-core/src/inner.rs` — the `if policy.is_order { … return Err(LsError::AmbiguousOrder …) }` block at the non-2xx site (~`inner.rs:335`). Add the `is_ingress_validation_reject(code)` → `ApiError` branch ahead of the `AmbiguousOrder` return. Update the surrounding order-safety comment to record the narrow `IGW40011` exemption and its rationale (ingress reject → never reached the exchange).
- `crates/ls-core/src/inner.rs`, `#[cfg(test)]` — new mock-server tests (mirror `post_order_does_not_cache_rejections`).

**Approach:** Only the non-2xx order branch changes. The 2xx order path
(`classify_order_rsp_cd`) is untouched — `IGW40011@200` already maps `Rejected`→`ApiError`,
which the existing `post_order_does_not_cache_rejections` test (mounts `IGW40011` at
`200`) still covers. Do **not** widen to non-`IGW40011` codes.

**Patterns to follow:** existing `dispatch_once` non-2xx `ApiError`/`AmbiguousOrder`
construction; `post_order_does_not_cache_rejections` wiremock test shape.

**Test scenarios:**
- Order response `http=500` + `rsp_cd=IGW40011` → `post_order` errors with
  `LsError::ApiError { code: "IGW40011", .. }` (regression: was `AmbiguousOrder`). Both
  attempts dispatch (rejection not cached), mirroring the existing 200-case test.
- Order response `http=500` + `rsp_cd=IGW00201` (or empty body) → `LsError::AmbiguousOrder`
  (fail-closed may-rest preserved). Covers R5.
- Order response `http=200` + `rsp_cd=IGW40011` → still `LsError::ApiError` (unchanged;
  existing test remains green).
- (Optional, documenting end-to-end intent in `adapters/nautilus/src/orders/map.rs`)
  `classify_submit_error(&LsError::ApiError { code: "IGW40011", .. })` == `SubmitAction::Reject`
  — already implied by the generic `ApiError`→`Reject` arm; add only if it reads as a
  useful named regression.

**Verification:** `cargo test -p ls-core` green, including the two new dispatch tests and
the unchanged 200-case test. `make lane-check` unaffected.

### U3. Order negative-probe: extract `classify_fired_variant` and exempt `IGW40011@500`

**Goal:** Replace the inline `http >= 500` halt arm in `run_order_negative_probe`'s fire
loop with a pure `classify_fired_variant(http, rsp_cd)` that returns
`PlacedNothing | MayHaveRested | Accepted`, exempting `IGW40011@500` (via the U1
predicate) to `PlacedNothing` so the differential continues.

**Requirements:** R1, R2, R3.
**Dependencies:** U1.
**Files:**
- `crates/ls-sdk/tests/negative_probe.rs` — add the `FiredVariantOutcome` enum + pure
  `classify_fired_variant`; rewrite the `Some((http, rsp_cd, ord_no)) =>` match arms
  (~`negative_probe.rs:1417-1473`) to dispatch on it. The `None` transport arm is
  unchanged. Add a `#[test]` for the pure fn near the existing helper tests (~line 1784).

**Approach:** `classify_fired_variant`:
- `is_order_placement_success(http, rsp_cd)` → `Accepted` (2xx ack).
- else `http >= 500 && !ls_core::is_ingress_validation_reject(rsp_cd)` → `MayHaveRested`.
- else → `PlacedNothing` (a non-ack business reject at 2xx/4xx, **or** `IGW40011@500`).

Match arms:
- `MayHaveRested` → the existing may-rest log line + `order_reconcile_teardown(.., false)` + `return`.
- `Accepted` → the existing WAVE-BLOCKED path (owned-set insert, `owned_fully_constructed`, teardown, return) — unchanged.
- `PlacedNothing` → `classify_probe(control_ok, true)` + the existing per-variant Clean log line — continue the loop.

**Patterns to follow:** `scan_page_is_terminal`, `classify_control_disposition`,
`is_order_placement_success` (pure, doc-commented, unit-tested) in the same file.

**Execution note:** Keep the log-line wording and the teardown/owned-set semantics of each
arm byte-identical to today — only the *routing* of `IGW40011@500` changes.

**Test scenarios:**
- `classify_fired_variant(500, "IGW40011")` == `PlacedNothing` (clean → continue).
- `classify_fired_variant(500, "IGW00201")` == `MayHaveRested` (any other 5xx → halt).
- `classify_fired_variant(200, "00040")` == `Accepted` (2xx ack → WAVE-BLOCKED, unchanged).
- `classify_fired_variant(200, "40510")` == `PlacedNothing` (non-ack 2xx business reject → Clean, unchanged).

**Verification:** `cargo test -p ls-sdk` compiles + the new pure-fn test passes. The live
probe test stays `#[ignore]` (not run here).

### U4. Ledger disposition — new §29

**Goal:** File the disposition in `metadata/PROVISIONALITY-LEDGER.md` as a new `## 29.`
section: the `IGW40011-as-500 halts the order type-variant differential` root cause, the
two-seam fix (probe + live path, single `ls-core` predicate), and the remaining operator
blocker (attended open-KRX re-probe of CSPAT00601/00701/00801 before promotion).

**Requirements:** R7.
**Dependencies:** U1, U2, U3.
**Files:**
- `metadata/PROVISIONALITY-LEDGER.md` — append `## 29.` after §28.

**Approach:** Match the prose/format of §27/§28. Record: quartet status (fix landed,
certification pending an attended live re-probe), that the live re-probe places REAL
paper orders and is order-autonomy-gated, and the promote-tr next step. Credential-free.

**Test expectation: none — documentation.**

**Verification:** `make docs && make docs-check` green (ledger is not docgen-projected,
but the gate must stay green); no count-test impact (no TR support-tier flip here).

---

## Verification Contract

Offline gate (must stay green — no red tree):

```
make docs && cargo test && cargo test -p ls-core && make docs-check && make lane-check
```

Cross-workspace adapter gate (KTD-6 — separate workspace, run from inside it so rustc
1.96 is selected; the `ls-core` fix propagates via path-dep to the live nautilus order
path):

```
cd adapters/nautilus && cargo test --workspace
```

Specifically confirm:
- The new `ls-core` predicate unit test and the two `dispatch_once` regression tests pass.
- The probe `classify_fired_variant` offline unit test passes.
- The existing `post_order_does_not_cache_rejections` (200-case) test stays green.
- `adapters/nautilus` `classify_submit_error` tests stay green (unchanged behavior; now
  reached by `ApiError{IGW40011}`→`Reject`).

**Operator blocker (do NOT run in this PR):** certifying the §27 quartet requires an
**attended, open-KRX** re-probe that places REAL paper orders:

```
LS_ORDER_SMOKE=1 LS_ORDER_SMOKE_NONCE=$(date +%s) make live-smoke-cspat00601-negative   # and 00701 / 00801
```

Order autonomy refuses unattended runs. Stop after the offline fix + gates and surface
the attended re-probe as an operator blocker. After a human runs a CLEAN re-probe, the
TRs promote via the `promote-tr` recipe (`.agents/skills/promote-tr/SKILL.md`) — a later
step, not this PR.

---

## Definition of Done

- U1–U4 landed; both seams route `IGW40011@500` to placed-nothing via the one `ls-core`
  predicate; all other 5xx/non-2xx and transport failures stay may-rest.
- Full offline gate green; `adapters/nautilus cargo test --workspace` green.
- Ledger §29 filed.
- No live order legs run; attended re-probe surfaced as the operator blocker.
- Branch off `main`; squash-merge PR. Commit message ends with the required
  `Co-Authored-By` trailer.

---

## Open Questions

- **Resolved (this PR, user-confirmed):** whether to also fix the live classifier — yes,
  at the `ls-core` dispatch seam (KTD1). Memory context:
  `order-error-classifier-placed-nothing-vs-may-rest`.
- **Deferred:** should other `IGW*` ingress codes (`IGW40013`, …) also be treated as
  placed-nothing? Not without per-code confirmation that they are pre-routing rejects;
  the predicate is structured to extend safely (add a code to one list) when evidence
  lands. Not in scope here.

---

## Sources & Research

- `metadata/PROVISIONALITY-LEDGER.md` §27 (reason C), §28.
- `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md` (prior §27 pagination fix — same file, adjacent seam).
- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md` (IGW40011 = ingress numeric-field validation reject).
- Code: `crates/ls-sdk/tests/negative_probe.rs:1417` (fire loop), `crates/ls-core/src/inner.rs:335` (order non-2xx branch), `crates/ls-core/src/inner.rs:134` (`classify_order_rsp_cd`), `adapters/nautilus/src/orders/map.rs:35` (`classify_submit_error`, variant-keyed).
- PR #106 (`a9974a9`, plan `2026-07-07-001`) — §27 pagination fix + live confirmation that surfaced this blocker.
