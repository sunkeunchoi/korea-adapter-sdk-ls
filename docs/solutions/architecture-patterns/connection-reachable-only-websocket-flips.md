---
title: "Connection-reachable-only: flipping fire-and-forget WebSocket TRs to Implemented when the gateway never signals rejection"
date: 2026-06-24
category: architecture-patterns
module: crates/ls-sdk
problem_type: architecture_pattern
component: realtime
severity: high
applies_when:
  - "Implementing realtime/WebSocket TRs whose subscribe path is fire-and-forget (does not read the subscribe ACK)"
  - "A lifecycle smoke (connect/subscribe/unsubscribe) is the Implemented gate for owner_class realtime"
  - "Deciding what reachability claim a green WebSocket lifecycle smoke actually earns"
  - "Porting realtime push-row structs / policies from the migration-source SDK (korea-broker-sdk-ls)"
tags:
  - realtime
  - websocket
  - reachability
  - negative-control
  - migration-source-port
  - WsLane
---

## Context

The realtime SDK surface was one TR wide (`S3_`). Bringing the other 31 realtime
WebSocket TRs (15 P1 market-data + 16 P2 order-event) to Implemented raised a
load-bearing question the REST waves never faced: **what does a green WebSocket
lifecycle smoke actually prove?**

The SDK's `subscribe` path is **fire-and-forget** — it sends the register frame
and never reads the gateway's subscribe ACK. So `subscribe_typed(...)` returns
`Ok` for a valid `tr_cd` and an invalid one alike. A connect → subscribe →
unsubscribe lifecycle completing cleanly proves the *connection* works; it does
**not** prove the specific `tr_cd` names a real, subscribable channel.

The wave built an executable **negative control** to settle this empirically: a
deterministic mock-WS test proving the gate *can* fail when the server rejects,
plus a live smoke (`make live-smoke-ws-negative`) that subscribes a deliberately
invalid `tr_cd` and reports whether the paper gateway emits an observable
rejection. The live answer was **`NOT-OBSERVABLE`**: the gateway stays silent for
a bogus code within the timebox.

## Guidance

When the subscribe path is fire-and-forget and the negative control resolves
`NOT-OBSERVABLE`, a clean lifecycle smoke earns only a **connection-reachable-only**
claim — record exactly that, never per-TR reachability:

1. **Run the negative control first; it gates the claim, not the flip.** A bogus
   `tr_cd` that produces the *same* clean lifecycle as a real one means the gate
   cannot distinguish channels. This does not block flips (the lifecycle/Transport
   gate is still met), but every flip must record the weaker claim.
2. **Classify the live signal honestly.** Only a `tr_cd`-attributable inbound body
   (a non-empty `rsp_cd` routed back to the subscriber) is `OBSERVABLE`. A bare
   stream close or a decode error is `INCONCLUSIVE` (a transient disconnect
   produces the same close); pure silence is `NOT-OBSERVABLE`. Treating a close as
   "observable rejection" would *false-confirm* per-TR reachability.
3. **Expect "all clean" and don't over-read it.** With `NOT-OBSERVABLE`, every
   channel — including ones with uncertain `tr_key`s — comes back clean *because
   the gateway never rejects*, not because each channel is proven real.
4. **Earning the stronger claim requires reading the subscribe ACK.** Per-TR
   reachability is unprovable until the SDK reads the ACK frame and surfaces
   acceptance — a separate capability, deliberately out of scope. Until then,
   field-correctness stays provisional and Recommended is deferred.
5. **Port the quartet from the migration-source SDK, not raw guesswork.** The old
   `korea-broker-sdk-ls` `crates/core/src/generated/*.rs` carries every realtime
   TR's push-row struct (`<Base>Response`), `{TR}_POLICY` (module/group/category),
   and cert `tr_key`. A bare struct there ⇒ single-object out-block (resolves the
   array-vs-single question structurally). Adapt to the new conventions: old
   `Option<String>` + `option_string_or_number` → plain `String` +
   `ls_core::string_or_number` + struct `#[serde(default)]`; thread the lane as a
   `WsLane` enum (market-data `"3"/"4"`, order-event `"1"/"2"`), never a stringly
   typed `tr_type`. **Omit secret-shaped fields** (`accno`/`passwd`/`ac_no`) the
   old order-event structs carried.

## Why This Matters

A green smoke is seductive: it feels like proof. Recording it as per-TR
reachability would overstate the evidence for every realtime TR at once, and the
overstatement is invisible — the gateway's silence looks identical to success.
The negative control is the only thing that converts "the smoke passed" into a
calibrated claim. Skipping it (or misclassifying a stream close as a rejection)
silently promotes 31 TRs on evidence the harness cannot actually supply.

Porting from the migration source instead of the raw capture removes the
array-vs-single guesswork that bit earlier REST waves (a wrong single/array shape
decodes silently empty and only surfaces at a future row-observation pass).

## When to Apply

- Any `owner_class: realtime` TR flipped on a lifecycle/Transport gate while the
  subscribe path remains fire-and-forget.
- Before claiming per-TR reachability for *any* WebSocket TR — run the negative
  control and read its `OBSERVABLE | INCONCLUSIVE | NOT-OBSERVABLE` verdict.
- When authoring a realtime push-row struct or `{TR}_POLICY`: reach for the
  migration-source generated code as the authoritative shape source.

## Examples

Live negative control (`NOT-OBSERVABLE`) — the load-bearing result:

```
make live-smoke-ws-negative
LIVE-SMOKE target=live-smoke-ws-negative inputs=[invalid_tr_cd=ZZ9 ws_port=29443]
  result=[NOT-OBSERVABLE: silence in timebox — flips are connection-reachable-only]
```

The signal classification that protects the claim (`live_smoke_ws_negative`):

```rust
match timeout(Duration::from_secs(5), stream.next()).await {
    Ok(Some(Ok(row))) if !row.rsp_cd.is_empty() => "OBSERVABLE: inbound rejection rsp_cd=…",
    Ok(Some(Ok(_)))  => "INCONCLUSIVE: inbound body with no rsp_cd (not attributable)",
    Ok(Some(Err(_))) => "INCONCLUSIVE: routed frame failed to decode",
    Ok(None)         => "INCONCLUSIVE: stream closed — same as a transient disconnect",
    Err(_)           => "NOT-OBSERVABLE: silence — flips are connection-reachable-only",
}
```

Lane as a typed enum, not a `tr_type` string (the register/deregister pair lives
in one place, so a wrong-lane deregister is a compile error):

```rust
pub enum WsLane { MarketData, OrderEvent }   // "3"/"4" vs "1"/"2"
ws.subscribe_typed::<K3Trade>("K3_", &shcode, WsLane::MarketData).await?;
```

## Related

- `docs/design/websocket-certification-findings.md` — Transport vs FrameDecode
  levels, the Phase 83/84 fresh-manager-per-smoke lesson, no-raw-frame-logging.
- `docs/solutions/conventions/tr-out-block-shape-from-raw-capture.md` — the
  array-vs-single trap this port resolves structurally via the old SDK.
- `.agents/skills/implement-realtime-tr/SKILL.md` — the recipe that encodes this
  claim-strength rule and the combined-sweep smoke lane.
