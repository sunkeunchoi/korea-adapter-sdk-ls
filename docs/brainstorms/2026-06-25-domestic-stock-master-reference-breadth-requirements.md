---
date: 2026-06-25
topic: domestic-stock-master-reference-breadth
title: "Breadth Wave — Domestic Stock Master/Reference Reads (raw→Implemented)"
---

# Breadth Wave — Domestic Stock Master/Reference Reads

## Summary

Raise the set of raw-untracked **domestic stock master/reference reads** that pass a flip-reliability bar (expected ~10–14, not a fixed count target) from raw → Tracked → Implemented in one wave, flipping each on a clean non-empty Paper Live Smoke. The cluster is chosen to maximize *reliable* implemented-count gain: master and reference reads convert to Implemented in a single window because their non-emptiness depends on neither an open trading session nor account trade activity. No order class, no new safety surface — pure reads through the existing dispatch.

---

## Problem Frame

The SDK sits at **97 Implemented / 119 Tracked**, and the raw OpenAPI capture holds **162 raw-untracked TRs**, the overwhelming majority of them reads. The remaining tracked-but-unimplemented TRs are not new work: the order trio awaits operator live-paper gates (plans -002/-003), and the overseas/night-KRX reads await an in-window smoke re-run already scripted in plan -001. So "more breadth" cannot come from finishing the tracked backlog — it has to come from the raw pool.

Within that pool, "biggest cluster" is a trap. The largest block (t11xx–t19xx, 78 domestic stock reads) is far too big for one wave and is dense with *intraday-session-gated* reads (live quotes, time-by-time flow) that return empty off-window and land PENDING rather than Implemented — the exact failure that left chunks of plan -001 un-flipped. For a goal measured in implemented-count, the binding constraint is **reliability of flip**, not pool size. Master and reference reads (stock masters, fundamentals, sector/aggregate reference) flip the moment their smoke runs, because reference data is non-empty regardless of session or account state. That reference/master subset is the cluster this wave targets; account-history reads, whose non-emptiness depends on account trade activity, are deferred to a later wave (see Scope Boundaries).

This is maintainer-initiated coverage-debt retirement; no external consumer is pulling for any specific TR. The wave is scoped as bounded debt-retirement, not capability delivery — its value is a reliable rise in maintained read-surface, paid for only where a TR has a credible flip path. It is sequenced ahead of the deferred C-prefix account-read capability-gap wave deliberately: these reads need no new safety surface and flip reliably, whereas the C-prefix reads carry the `01900` paper-incompatibility risk that sank the two F&O account reads. The cost side is honest too — every tracked TR adds a standing per-TR count-assertion tax across three crates (R11) that persists whether or not it flips, which is why the wave tracks only probe-passing candidates (R3).

---

## Key Decisions

- **Breadth = reliable implemented-count, so the selection axis is flip-reliability.** Candidates are ranked by whether their non-emptiness depends on an open session or on account activity. Pure reference/master data (always non-empty) qualifies; account-history reads (empty in paper without trades) are deferred to a later wave; live-quote and overseas/night reads are excluded as belonging to session/window-gated waves.

- **Reads only — no new safety surface.** Every TR routes through the existing read dispatch (`Inner::post` / `post_paginated`) and reuses the established read machinery. The order-safety runtime (`post_order`, dedup, kill switch, reconciliation) is untouched. No `ls-core` change is expected.

- **Size by the reliable subset, not a count target.** Wave size is whatever passes the reliability bar (R2) after the pre-track probe — expected ~10–14, not a fixed number to hit. Tracking a TR that is not expected to flip pays the full count-assertion tax for zero implemented-count, which is the worst trade against the reliable-count goal. Partial completion is still first-class: a candidate that passes the probe but flips empty in-window is recorded PENDING honestly.

- **Classify by raw shape; use the probe as a wire-health check, not a session-independence proof.** Each candidate is first **raw-shape-classified** (master vs live-quote, from the capture's out-block keys) — that classification, not the probe, is the always-on signal. `make raw-probe` then runs as a wire-health + non-empty-now check whose body is built from the capture's in-block shape (not guessed). A single in-window probe shows "non-empty now," never session-independence, so it *reduces* — not eliminates — PENDING risk; the residual session-gated case is owned in R8/Dependencies. The probe's value is catching `01900` (paper-incompatible) and `IGW40011` (a fixable numeric-serialization defect) before the tracking tax is paid.

- **Reuse the frozen recipes.** raw→Tracked via `track-tr`, Tracked→Implemented via `implement-tr`. No recipe authoring; this wave is an application of existing ones, not an extension.

---

## Requirements

**Candidate set & selection**

- R1. The wave raises the raw-untracked **domestic stock master/reference read** TRs that pass R2 and the pre-track gate (R3) from raw → Tracked → Implemented — expected ~10–14, not a fixed count target. Exact membership is pinned at planning against the raw capture.
- R2. TRs are selected by a **flip-reliability criterion**: only reads whose non-emptiness depends on neither an open trading session nor account trade activity, established by raw-shape classification (R3). Reference/master/aggregate reads qualify; account-history, live-quote, overseas, and night-window reads are excluded from this wave.
- R3. Each candidate is **raw-shape-classified** (out-block keys → master vs live-quote) and then `raw-probe`d, with the probe body built from the capture's in-block shape, before tracking. Raw-shape classification is the always-on signal; the probe is a wire-health + non-empty-now check, not proof of session-independence. Probe verdict: a non-empty `00000` admits; a probe-visible `01900` excludes pre-track; an `IGW40011` triggers a fix to the numeric request-field serialization (`string_as_number`) and a re-probe — never an auto-exclude; an empty body excludes as session-gated unless raw shape confirms a master.
- R4. The wave is reads-only. No order or control TRs, and no new order-safety surface — every TR routes through the existing read dispatch and reuses established read machinery.

**Tracking & implementation**

- R5. Each TR is raised raw→Tracked via `track-tr`: a `metadata/trs/<tr>.yaml` + `tr-index.yaml` entry, with the normalized baseline **projected** via `make api-drift-renormalize` (never hand-authored).
- R6. Each Tracked TR is flipped to Implemented via `implement-tr`: callable Rust gated on a clean **non-empty** Paper Live Smoke.
- R7. `owner_class` routing (`standalone` vs `market_session` vs `paginated`) is assigned per TR from its wire shape, not assumed from the cluster.

**Dispositions & honesty**

- R8. A TR whose smoke returns **empty** in paper is recorded PENDING (off-window / no-activity), not flipped.
- R9. A TR that passes a clean probe but returns a **terminal `01900`** at implement-time is recorded `paper_incompatible` — authored for callability, no flip expectation. (A probe-visible `01900` is excluded pre-track per R3 and never reaches this state.)
- R10. The wave flips only the subset returning clean non-empty data; a partial flip with honestly-recorded PENDING/incompatible remainders is success, not failure. **Flip floor:** if fewer than a planning-set minimum of candidates pass R3's gate, the cluster is mis-chosen and is re-scoped rather than shipped as a token wave.

**Metadata & count tax**

- R11. Tracking N TRs bumps the maintained-count assertions at `crates/ls-trackers/tests/api_drift.rs`, `crates/ls-trackers/src/cli.rs`, and `crates/ls-docgen/src/lib.rs` (`TRACKED_TRS` length); each Implemented flip bumps the docgen `reference`/`banner` counts.
- R12. Recommended promotion is out of scope. Every flip lands `implemented: true`, `recommended: false`.

---

## Acceptance Examples

- AE1. **Covers R6, R8.** A master/reference read (e.g. a stock master or sector-classification read) returns non-empty in paper regardless of session → flips to Implemented; the gate stays green.
- AE2. **Covers R2, R3.** A candidate's pre-track probe returns a visible `01900` (or an empty body with no raw-shape master signal) → excluded from the tracked set before any tracking tax is paid, rather than tracked-then-recorded-PENDING.
- AE3. **Covers R3, R9.** A candidate's probe returns `IGW40011` → the numeric request field is fixed to serialize as a JSON number and re-probed, never auto-excluded; only a `01900` that survives a clean probe to implement-time is recorded `paper_incompatible`.
- AE4. **Covers R2, R8.** A candidate inferred as a "master" passes an in-window probe but turns out session-gated and is empty off-window → reclassified PENDING, never force-flipped on an empty result.

---

## Candidate Pool

Illustrative, not final — planning pins exact membership by applying R2 and the pre-track gate (R3) against the raw capture.

- Stock master `t9945`; fundamentals/reference `t3518` (재무정보), `t3521` (경영진정보), `t3401` (투자의견), `t4203` (업종분류), `t3202` (종목별증시일정); market aggregate `MMDAQ91200` (시장통계); plus the master-leaning entries of the `t8450`–`t8466` block (per raw capture; raw-shape classification of the out-block keys, corroborated by the probe, separates true masters from live-quote variants and drops the latter).

All named candidates are confirmed raw-untracked and present in the raw capture (verified). Account-history reads (`t0150`/`t0151`/`t0167`/`t0424`) are deferred to a later wave — see Scope Boundaries.

---

## Scope Boundaries

**Deferred for later**

- Account-history reads (`t0150`/`t0151`/`t0167`/`t0424`) — deferred to a wave that can seed a paper trade so they return non-empty; carrying them here would pay the tracking tax for predictably-PENDING TRs.
- The rest of the 162-TR raw pool — subsequent breadth waves (t11xx–t19xx flow/analytics, t19xx ETF/warrant/bond, o31xx overseas futures, t2xxx F&O market data).
- The C-prefix account-read inquiries (balance, buying-power, credit limit) — a capability-gap wave, and they carry the same `01900` paper-incompatibility risk as the dead F&O account reads.
- Recommended promotion of any TR in this wave — a separate act (Focused Evidence ≤7 days + recommendation block).

**Outside this wave**

- Orders of any class (domestic stock orders are in-flight on plans -002/-003; F&O and overseas orders need a distinct safety design).
- The overseas-stock and night-KRX reads — already callable on `main`, awaiting plan -001's in-window smoke re-run.
- Any change to the order-safety runtime or to non-read dispatch.

---

## Dependencies / Assumptions

- Maintainer-initiated coverage-debt retirement; no external consumer pulls for a specific TR. Success is measured in reliable implemented-count.
- The existing read dispatch (`Inner::post` / `post_paginated`) and the `track-tr` / `implement-tr` recipes are sufficient — **no `ls-core` change expected** (assumption; pinned at planning per TR).
- Starting state: 119 Tracked / 97 Implemented (verified).
- Per-TR paper-gateway behavior is probed before tracking (R3) rather than assumed; the wave still plans for residual PENDING on a candidate that passes the probe but flips empty in-window.
- A candidate whose wire shape would require a new dispatch or pagination shape is dropped from this reads-only wave (deferred), not absorbed — that preserves the no-`ls-core`-change boundary as a hard filter.
- The raw capture's field names, types, and array-vs-single shapes are the source of truth for each TR (not guesswork); some candidate purposes above are inferred from the capture and confirmed at tracking.

---

## Outstanding Questions

**Deferred to planning** (none block starting)

- Exact membership (~10–14) and per-TR `owner_class`, pinned against the raw capture by R2 and the pre-track gate (R3).
- Which `t8450`–`t8466` entries are true masters (always-on) versus live-quote variants (session-gated) — the raw shapes (corroborated by the probe) disambiguate.
- The flip-floor value (R10) below which the cluster is treated as mis-chosen — set at planning once the probe-passing count is known.
- For the deferred account-history wave: whether those reads can be made non-empty via a seeded paper trade (out of scope here).

---

## Sources / Research

- `docs/plans/2026-06-25-001-feat-night-overseas-elw-implement-wave-plan.md` — the queued in-window smoke-rerun wave (why the tracked backlog is not new work).
- `docs/plans/2026-06-25-002-feat-order-runtime-first-package-plan.md`, `docs/plans/2026-06-25-003-feat-order-modify-cancel-wave-plan.md` — the operator-gated order waves (why orders are out of scope here).
- `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json` — the raw capture; source of truth for wire shape and the raw-untracked pool.
- `metadata/tr-index.yaml`, `metadata/trs/*.yaml` — the Tracked set (119) and per-TR support flags.
- Count-assertion sites to bump on tracking: `crates/ls-trackers/tests/api_drift.rs:106` (`maintained_tr_count`), `crates/ls-trackers/src/cli.rs:1811` (`shapes.len()`), `crates/ls-docgen/src/lib.rs:677` (`TRACKED_TRS` length).
- `.agents/skills/track-tr/SKILL.md`, `.agents/skills/implement-tr/SKILL.md` — the recipes this wave applies.
- `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md` — `make raw-probe` failure classifier for diagnosing empty/`01900`/`IGW40011` outcomes.
