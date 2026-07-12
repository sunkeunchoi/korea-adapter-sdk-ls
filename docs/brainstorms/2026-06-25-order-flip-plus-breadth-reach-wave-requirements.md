---
date: 2026-06-25
topic: order-flip-plus-breadth-reach-wave
---

# Order Flip + Breadth Reach Wave — Requirements

## Summary

Now that the paper account can place orders, certify the four order TRs
(`CSPAT00601`/`CSPAT00701`/`CSPAT00801` + `t0425`) Tracked→Implemented via the
full order-chain smoke, and in the same wave flip a batch of breadth reads —
daytime domestic-ish reads plus the overseas-stock cluster (smoked in its own
trading window). Ships as three stacked PRs (orders / daytime reads /
overseas), each read dispositioned by its own paper smoke.

## Problem Frame

The order runtime — `post_order`, dedup, kill-switch, the six-state matcher,
and the chained order smoke — landed in prior waves (PRs #52/#53), but the four
order TRs stayed `implemented: false`. The blocker was environmental, not code:
the configured paper account returned gateway `01491` (모의투자 주문이 불가한
계좌 — order-incapable account), so the order smokes recorded Pending instead of
evidence. That account has now been replaced with an order-enabled paper
account, clearing the only thing standing between the built runtime and the
flip. Separately, 16 non-order TRs sit at Tracked with no live evidence; the
unblocked credentials are the moment to spend a coordinated wave closing both
the order surface and a slice of read breadth.

## Key Decisions

- **Full order chain, not Gate-1-only.** Run `make live-smoke-order-chain`
  (submit→modify→cancel + `t0425` reconcile) so all four order TRs flip this
  wave rather than deferring modify/cancel to a later chain wave. The trade-off
  is that the smoke places, modifies, and cancels real (paper) orders on the
  live gateway.

- **Three stacked PRs, not one combined PR.** Orders, daytime reads, and
  overseas land as separate stacked PRs. This isolates order-placement risk so a
  chain-smoke regression can't block the reads, and it separates the
  night-window overseas smokes onto their own clock. Each PR rolls back
  independently.

- **Stop at Implemented; Recommended is a separate pass.** No TR is promoted to
  Recommended this wave. Promoting orders to Recommended endorses live order
  placement and intersects the open ADR 0008 — a deliberate decision that does
  not belong inside a flip wave.

- **Every read is smoke-gated individually.** A read flips only on its own clean,
  non-empty paper smoke; otherwise it records Pending. Wave success is defined by
  the orders plus the daytime reads, not by every breadth candidate flipping.

## Requirements

**Orders**

- R1. `CSPAT00601` (submit), `CSPAT00701` (modify), `CSPAT00801` (cancel), and
  `t0425` (order inquiry/reconcile) flip Tracked→Implemented on a clean
  `live-smoke-order-chain` run that exercises submit→modify→cancel and a `t0425`
  reconcile.
- R2. Each order TR gains the registration the promote recipe expects: a
  `smoke-map.md` row and a Makefile `.PHONY` smoke entry, plus registration in
  both crosscheck lists per the implement-order-tr recipe.
- R3. A `01491` (or any order-incapable) gateway response during the chain
  records the affected TR as Pending — never as evidence — via the existing
  `ls_core::is_paper_order_incapable` classifier.
- R9. Each order TR's flip is predicated on that TR's own response (rsp_cd
  success plus the expected out-block present), not merely on the chain
  advancing — a sibling leg's success or a downstream `t0425` reconcile must
  not certify a TR whose own response did not confirm.
- R10. Order-chain evidence inherits an explicit at-rest posture: account
  identifiers HMAC-keyed (not bare), order numbers and `rsp_msg` scrubbed from
  the recorded artifact, a fixed evidence location, and a retention bound —
  the same posture the first order package committed to. "Credential-free"
  alone is not the bar.
- R11. A chain that fails mid-sequence after a real submit (timeout, ambiguous
  match, failed cancel) must attempt a best-effort cancel of any resting order
  and record all four order TRs Pending — never a partial flip. An
  unknown-outcome cancel is treated as possibly-still-live (fail toward the
  safe direction), not assumed-succeeded.

**Daytime breadth reads**

- R4. Attempt flips for the in-window candidates `t2106`, `CCENQ10100`,
  `CCENQ90200`, `t3102`, `t8430`, `t8455`; each flips Implemented on a clean
  non-empty paper smoke and records Pending otherwise.
- R5. `t1860` (saved-condition search) is attempted but expected to record
  Pending when no server-saved conditions exist for the account; it does not
  block the daytime-reads PR.

**Overseas cluster**

- R6. Attempt flips for the overseas-stock cluster `g3101`–`g3106` and `g3190`,
  with smokes executed during the overseas trading window (KR night-time); each
  flips Implemented on a clean non-empty in-window smoke and records Pending
  otherwise.

**Wave mechanics**

- R7. Each flipped TR carries its smoke target into the registries the promote
  recipe reads (`smoke-map.md` row + Makefile `.PHONY`), so the TR is
  recipe-promotable later.
- R8. All count-maintenance sites stay consistent (docgen `reference.len` /
  banner / TRACKED_TRS, `ls-trackers` `cli.rs` literal counts, `api_drift.rs`,
  manifest `refreshed` left at the last raw-refresh date), and the full gate
  (`make docs` / `cargo test` / `cargo test -p ls-core` / `make docs-check`) is
  green per PR.

## Acceptance Examples

- AE1. Covers R1, R3. **Given** the order-enabled paper account, **when**
  `live-smoke-order-chain` runs submit→modify→cancel + reconcile and every leg
  returns success, **then** all four order TRs flip Implemented with focused,
  credential-free evidence captured.
- AE2. Covers R3. **Given** an order leg returns `01491`, **when** the smoke
  classifies it, **then** the affected TR records Pending and the wave does not
  flip it.
- AE3. Covers R4, R5. **Given** a daytime read returns an empty board, **when**
  its smoke asserts non-empty before recording, **then** it records Pending
  rather than a false Implemented flip.
- AE5. Covers R11. **Given** the chain submits and modifies a real paper order
  but a later leg fails (timeout / ambiguous match / failed cancel), **when**
  the smoke handles the failure, **then** it attempts a best-effort cancel of
  any resting order and records all four order TRs Pending — not a partial flip.
- AE4. Covers R6. **Given** an overseas TR is smoked outside its trading window
  and returns empty, **when** the smoke evaluates the result, **then** it records
  Pending and is eligible for a later in-window retry — the overseas PR still
  ships with whatever flipped cleanly.

## Success Criteria

- The four order TRs are Implemented with clean chain-smoke evidence, and the
  order surface (submit/modify/cancel/inquiry) is callable end-to-end on paper.
- The daytime-reads PR flips its clean candidates; any Pending reads are recorded
  faithfully with their reason (empty board / routing / no saved conditions).
- The overseas PR flips whatever got a clean in-window capture; Pending overseas
  TRs name the night-window timing as the reason.
- Gate green on every PR; no count-test drift; Recommended set untouched.

## Scope Boundaries

**Deferred for later**

- `t1964` (empty-board), `t1852`/`t1856` (known caveats) — not attempted this
  wave.
- Recommended promotion for any TR, including the orders — its own deliberate
  pass, gated on ADR 0008 for orders.
- Overseas TRs that can't get a clean in-window smoke this wave — retried in a
  later night-window pass rather than forced.

**Outside this wave's shape**

- Net-new order-runtime building (matcher, dedup, kill-switch) — already shipped;
  this wave is certify-and-flip, not build.

## Dependencies / Assumptions

- The replaced paper account is genuinely order-capable (no `01491`); the chain
  smoke is the verification of this assumption.
- `t2106`/`t8455` route via `market_session`; `t3102`/`t8430`/`t1860` are
  `standalone` and route through `market_session`, not an OAuth-only standalone
  path (per prior-wave routing lesson).
- `t1860` is classified `standalone` in metadata (a server-saved-condition
  search), not a realtime-control TR — it is eligible as a read.
- Overseas cluster data is only populated during the overseas trading session,
  so its smokes require KR-night execution.

## Outstanding Questions

**Deferred to planning**

- Per-TR out-block keys and array-vs-single response shapes come from each
  normalized baseline (`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`),
  resolved during implementation, not guessed.
- Which numeric request-body fields each breadth read needs serialized as JSON
  numbers (`string_as_number`) to avoid `IGW40011` — determined per TR via
  `raw-probe` if a smoke fails.

## Deferred / Open Questions

### From 2026-06-25 review

The breadth-selection half of this wave needs a restructuring decision before
planning. These came from the document review and are entangled with the root
question below — resolve the root first, then the dependents largely follow.

- Root — **Breadth overlaps in-flight plan -001.** The daytime + overseas reads
  here (t2106, t8455, the CCENQ pair, the overseas sextet) are already scoped by
  the unmerged `docs/plans/2026-06-24-…-001-…-night-overseas-elw` plan. Decide:
  drop the overlap and let plan -001 own the window-gated reads, or explicitly
  supersede plan -001 and reconcile its dispositions. Do not run both against the
  same metadata + count-assertion sites.
- **CCENQ10100 / CCENQ90200 do not belong in the daytime flip list (R4).** Both
  carry `paper_incompatible: true` and are `venue_session: krx_extended`
  (night-window); they return terminal `01900` regardless of window and will
  never flip on paper. Reclassify as settled paper-incompatible, not "Pending
  otherwise."
- **g3105 is raw/untracked (R6).** The range `g3101`–`g3106` implies a 7th member
  that has no metadata, baseline, or smoke target. Replace the range with the
  explicit Tracked members (`g3101`, `g3102`, `g3103`, `g3104`, `g3106`,
  `g3190`), or add a `track-tr` step for g3105 before claiming it.
- **t8430 is gated on an upstream array-shape blocker (R4),** not on the smoke
  window — kept tracked-only across prior plans. Either drop it or note the
  blocker resolution as an explicit precondition.
- **t1860 disposition conflict (R5).** Prior plans HELD it as a side-effectful
  realtime-subscription control; this doc frames it as a standalone-eligible
  read. Pin which classification holds before attempting it.
- **t8460 / t8463 undispositioned (Scope Boundaries).** Two Tracked,
  paper-compatible KRX night-derivatives reads appear nowhere — neither in-scope
  nor deferred. Add an explicit disposition.
- **"16 non-order TRs" is inaccurate (Problem Frame).** The enumerated candidate
  set is 14; the actual on-disk Tracked non-order count is 18. Re-derive from
  metadata at planning time and state the wave touches a subset.

## Sources / Research

- Order TR metadata + state: `metadata/trs/CSPAT00601.yaml`,
  `CSPAT00701.yaml`, `CSPAT00801.yaml`, `t0425.yaml` (all `tracked: true`,
  `implemented: false`).
- Smoke targets: `Makefile` (`live-smoke-order`, `live-smoke-order-chain`);
  registry `.agents/skills/promote-tr/references/smoke-map.md`.
- `01491` classifier: `crates/ls-core/src/inner.rs`
  (`PAPER_ORDER_INCAPABLE_CODE`, `is_paper_order_incapable`), used by the order
  smoke harness.
- Breadth candidates (all `tracked: true`, `implemented: false`):
  `metadata/trs/{t2106,CCENQ10100,CCENQ90200,t3102,t8430,t8455,t1860,g3101..g3106,g3190}.yaml`.
- Recipes: `.agents/skills/implement-order-tr/SKILL.md`,
  `.agents/skills/implement-tr/SKILL.md`, `.agents/skills/promote-tr/SKILL.md`.
</content>
</invoke>
