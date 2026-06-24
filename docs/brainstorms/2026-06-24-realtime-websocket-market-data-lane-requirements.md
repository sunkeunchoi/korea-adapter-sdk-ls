---
date: 2026-06-24
topic: realtime-websocket-market-data-lane
---

# Realtime WebSocket Market-Data Lane — Requirements

## Summary

Track and implement all 31 realtime WebSocket TRs — 15 P1 market-data feeds plus 16 P2 order-lifecycle *observation* feeds — by extending the proven `S3_` pattern. Each TR is proven callable by a generic connect→subscribe→unsubscribe lifecycle smoke (Transport reachability) and ships a full typed push-row struct whose field correctness stays provisional. Implemented is the ceiling; Recommended is deferred.

## Problem Frame

The realtime SDK surface is one TR wide. `S3_` (KOSPI 체결) is the only `owner_class: realtime` TR implemented — every sibling feed (KOSDAQ trades, order-book depth, best-quote, integrated 체결/호가, overseas stock and futures, F/O, and the order-lifecycle event channels) sits raw, so a consumer cannot subscribe to anything but KOSPI trades.

The infrastructure to close that gap mostly exists: `WsManager` handles connect, subscribe/unsubscribe, record-before-send, reconnect replay, and bounded backpressure; `frame.rs` builds market-data register/deregister frames (`tr_type` "3"/"4") for any `tr_cd`. What's missing is mostly per-TR wiring — plus one small transport seam: the P2 order-event lane needs a `tr_type` "1"/"2" frame-build path the current builders lack (see R1). The prior REST reach-wave plan explicitly deferred exactly these 31 TRs to "a separate realtime effort" and named the prerequisite — *"a generic WebSocket-lifecycle-smoke methodology built first"* — because `implement-tr` HELDs realtime out of scope (SKILL.md §0). This wave is that effort.

## Key Decisions

- **Lifecycle reachability is the Implemented gate for WebSocket TRs.** A green connect→subscribe→unsubscribe smoke (the migration source's "Transport" level) opens the Implemented gate; row delivery is bonus evidence, never required. This overrides CONCEPTS.md's REST "non-empty result" bar for `owner_class: realtime`, and is already precedented — `S3_` reached Implemented and Recommended on lifecycle-only evidence, with row contents explicitly excluded.

- **Full typed push-row struct per TR, field correctness provisional.** Each implemented TR ships a complete typed decode struct projected from its normalized baseline (like `S3Trade`), with tolerant `string_or_number` coercion, so consumers get usable types now. Because the smoke never observes a row, field correctness is booked provisional in the ledger and the per-TR exclusion language mirrors `S3_`. A shared per-shape struct or raw-frame passthrough was rejected: it loses per-TR field naming the baseline already carries.

- **Capability spine built before any TR flips.** Because `implement-tr` HELDs realtime, the wave's first deliverable is a generic, parameterized lifecycle smoke plus a realtime-capable track/implement recipe — frozen for reuse, the same way earlier waves built and froze the `track-tr` rung in-flight. Per-TR flips fan out only after the spine lands.

- **P1 and P2 ship in one wave; P2 is observation-only.** One generic lifecycle smoke covers both sets since neither requires row delivery — but P2 order-event channels register with `tr_type` "1"/"2" (vs market-data "3"/"4"), so the smoke and frame builders take a `tr_type` parameter; this is one small transport seam, not pure per-TR wiring. P2 order-lifecycle feeds are subscribed as event observers — the smoke never places, corrects, or cancels an order to force a row, and no REST order runtime is introduced.

## Requirements

### Capability spine

- R1. A generic WebSocket lifecycle smoke, parameterized by `tr_cd`, a representative `tr_key`, and `tr_type`, proves connect→subscribe→unsubscribe with no immediate protocol error, using an isolated manager per TR (per the Phase 83/84 lesson that a shared manager poisons later TRs). The `tr_type` parameter is load-bearing: P1 market-data feeds register/deregister with `tr_type` "3"/"4" (what the current builders emit), but P2 order-event channels register with "1"/"2", which the current `frame.rs` builders do not support — so the smoke and the frame builders both take `tr_type`. It is reusable across all 31 TRs.
- R1a. The spine includes a negative control: a deliberately-invalid `tr_cd` that MUST stay Tracked-only. If the control also passes the lifecycle, the gate is proven incapable of validating per-TR reachability, and no flip is trusted until that is resolved (see R5).
- R2. A realtime-capable track/implement recipe path exists so realtime TRs can flip; the current `implement-tr` §0 HELD for realtime is lifted or superseded by a sibling recipe, and the methodology is frozen as a reusable skill.

### Tracking and implementation

- R3. All 31 raw TRs are brought to Tracked — committed `metadata/trs/<tr>.yaml` + `tr-index.yaml` entry + projected normalized baseline — with `owner_class: realtime` and `protocol: websocket`.
- R4. Each reachable TR flips to Implemented on a green lifecycle smoke. A subscription that succeeds but delivers no row within the wait window still flips the TR.
- R5. The Implemented gate for `owner_class: realtime` is Transport reachability, explicitly distinct from the REST non-empty-result gate. Because the current subscribe path sends the frame fire-and-forget without reading the gateway ACK, the gate must first establish that the paper gateway actually rejects an unknown `tr_cd` (via R1a's negative control). Until that holds, a green smoke proves connection reachability, not per-TR reachability, and no flip is trusted.

### Decode and metadata fidelity

- R6. Each implemented TR ships a full typed push-row struct projected from its normalized baseline, with `string_or_number` coercion on all fields.
- R7. Field correctness is recorded provisional — decode-field ledger rows stay open, and the per-TR exclusion language scopes the claim to lifecycle reachability, mirroring `S3_`. A struct whose out-block key or array-vs-single shape cannot be confirmed from a raw capture is marked structurally-unverified — a stronger flag than field-value-provisional — so a consumer can distinguish "field types unconfirmed" from "may not decode at all." (Memory: KTD3-style single/array guesses are unreliable; read the true out-block key from a raw capture.)

### Dispositioning and scope guards

- R8. A member whose subscribe is rejected, errors immediately, or has no available representative `tr_key` ships Tracked-only with a recorded disposition reason (`paper_incompatible` / pending / held), never Implemented.
- R9. P2 order-lifecycle feeds are observation-only: the wave subscribes to event channels and never places, amends, or cancels an order, and introduces no REST order runtime.

### Gate, counts, and ceiling

- R10. `maintained_tr_count` (currently 85) and the dependent count assertions — docgen banner/reference, `ls-trackers` CLI literal counts, `api_drift.rs` — are bumped consistently for every tracked and implemented TR, and the full gate (`make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`) stays green.
- R11. No TR is promoted to Recommended in this wave. Implemented is the ceiling; Focused Evidence and recommendation blocks are a separate later effort. This is a batch-sequencing deferral, not a stricter evidence bar than `S3_` met — `S3_`'s lifecycle-only Recommended remains the template; these TRs simply defer Recommended to a focused later pass.

## TR Roster

| TR | Set | Lane | Feed |
|---|---|---|---|
| K3_ | P1 | realtime stock | KOSDAQ 체결 (closest sibling to `S3_`) |
| H1_, HA_ | P1 | realtime stock | KOSPI / KOSDAQ order-book depth (호가) |
| S2_ | P1 | realtime stock | KOSPI best-quote (우선호가) |
| US3, UH1, US2 | P1 | realtime stock | integrated 체결 / 호가 / 우선호가 |
| GSC, GSH | P1 | overseas stock | overseas 체결 / 호가 |
| OVC, OVH | P1 | overseas futures | overseas futures 체결 / 호가 |
| OC0, OH0, FC9, FH9 | P1 | F/O | options / futures 체결 / 호가 |
| SC0–SC4 | P2 | stock order lifecycle | order receipt / execution / correct / cancel / reject observation |
| C01, O01, H01 | P2 | F/O order lifecycle | F/O order / execution event observation |
| AS0–AS4 | P2 | overseas stock order lifecycle | US order lifecycle observation |
| TC1–TC3 | P2 | overseas futures order lifecycle | overseas futures order lifecycle observation |

## Acceptance Examples

- AE1. Lifecycle-only flip. **Covers R4, R5.** **Given** a tracked market-data TR with a valid `tr_key`, **when** the smoke connects, subscribes, and unsubscribes cleanly but no row arrives within the window, **then** the TR flips to Implemented and the absent row is recorded as bonus-not-required.
- AE2. Subscribe rejected. **Covers R8.** **Given** a tracked TR, **when** subscribe returns a protocol error or immediate rejection, **then** the TR stays Tracked-only with a recorded disposition reason and does not flip.
- AE3. No representative key. **Covers R8.** **Given** an overseas or F/O member with no available paper `tr_key`, **when** no smoke can be constructed, **then** the member ships Tracked-only (held), not Implemented.
- AE4. Order feed without an order. **Covers R9.** **Given** a P2 order-lifecycle TR, **when** the smoke subscribes during a window with no guarded order placed, **then** the connect→subscribe→unsubscribe lifecycle still proves Transport and the smoke never places an order to force an event.

## Scope Boundaries

**Deferred for later**

- Promotion of any TR to Recommended — Focused Evidence + recommendation blocks, a separate effort after Implemented.
- FrameDecode-level proof — observing and validating a real row's field contents during a market-open or guarded-event window. Retires the provisional decode-field ledger rows; out of scope here.

**Held out of this wave**

- REST order runtime (placing, amending, or cancelling orders) for the P2 lane — observation feeds only, never order execution.
- Reconnection-correctness-per-venue and gapless-delivery guarantees — the existing `DropNewest` / `LatestOnly` policies are not gapless-delivery claims and this wave makes none.

## Dependencies / Assumptions

- The realtime transport stack (`WsManager` lifecycle, reconnect replay, decode dispatch) is generic for the P1 market-data lane (`tr_type` "3"/"4"). The P2 order-event lane needs a `tr_type` "1"/"2" frame-build path the current `frame.rs` builders lack, so the wave adds that one transport seam plus per-TR structs and smoke wiring.
- Transport reachability is established for the P1 market-data domains: the migration source certified 65 ws-stock and 6 F/O WebSocket TRs at Transport on paper. It is NOT yet established for the 16 P2 order-event channels — those certs do not cover them, and the migration source reached SC0/SC1 only via a guarded order lineage — so P2 reachability is unknown and a meaningful share may land R8 Tracked-only. R8 is the disposition path where reachability does not hold.
- Each smoke runs on a fresh/isolated manager (Phase 83/84 root cause: a shared manager's sender died after the first TR, failing later TRs).
- Provisional decode-field ledger rows persist until a future FrameDecode effort — the ledger grows by ~31 open rows by design.

## Outstanding Questions

**Deferred to planning / execution discovery**

- Representative `tr_key` per domain — stock `shcode`, overseas symbol, F/O code, and the P2 order-feed key shape, which is account-bound (empty string vs account number), not a public symbol. Each is discovered before its smoke runs; an unresolved key drives an R8 disposition rather than blocking the wave. If the P2 key turns out to be an account number, confirm it does not reclassify those TRs from realtime-class to account-class.
- Whether to opportunistically prove a real decoded row for SC0/SC1 — the migration source reached FrameDecode there via a guarded order lineage — as a pattern-validating anchor within an otherwise lifecycle-only wave, without introducing REST order runtime (R9).
- PR shape — one wave-wide PR vs a split by lane (stock / overseas-stock / overseas-futures / F/O / order-lifecycle), decided by review size once the spine and first few flips land.

## Sources / Research

- `metadata/trs/S3_.yaml`, `metadata/evidence/S3_.yaml` — the reference realtime TR: lifecycle-only recommendation with row correctness, delivery, and reconnection explicitly excluded.
- `crates/ls-sdk/src/realtime/mod.rs`, `frame.rs` — `WsManager` lifecycle, record-before-send, reconnect replay, composite routing key; `tr_type` "3"/"4" register/deregister frame build.
- `crates/ls-sdk/tests/live_smoke.rs` (`live_smoke_ws`) — the existing single-TR lifecycle smoke this wave generalizes; paper WS port 29443.
- `docs/design/websocket-certification-findings.md` — Transport vs FrameDecode levels, the Phase 83/84 shared-manager lesson, and future-expansion guidance.
- `docs/plans/2026-06-24-001-feat-readonly-rest-reach-wave-plan.md` (§ Scope Boundaries) — the deferral that scoped this wave, with the full 15 + 16 TR roster (note: the plan's prose says "14" but lists 16 codes).
- `.agents/skills/implement-tr/SKILL.md` (§0) — the current realtime HELD this wave lifts.
- `crates/ls-trackers/tests/api_drift.rs:106` — `maintained_tr_count` baseline (85).
