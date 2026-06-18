# WebSocket Certification Findings

**Status:** maintained migration evidence note. Extracted from the
`korea-broker-sdk-ls` Migration Source so broad WebSocket gateway findings
survive without preserving the old full certification pipeline.

This note is historical evidence and future expansion guidance. It does not make
unmaintained WebSocket TRs part of the **Maintained SDK Surface**, and it does
not make them **Recommended TRs**. Current maintained behavior remains scoped to
the realtime slice implemented in this repository.

## Certification Levels From The Migration Source

The old evidence model separated two WebSocket levels:

| Level | Requirement | Evidence meaning |
|---|---|---|
| Transport | Paper WebSocket connects, subscribe succeeds for the TR/key, unsubscribe succeeds, and no immediate protocol error occurs. | The SDK can establish and clean up the subscription lifecycle for that WebSocket TR. |
| FrameDecode | Transport plus at least one real LS frame is received and deserializes into the generated response type within the wait window. | The SDK observed and decoded a live LS frame for that WebSocket TR. |

Transport was the baseline. FrameDecode was intentionally opportunistic because
it depends on market hours, specific market events, or guarded manual order
events.

## Phase 83/84 Finding

The Migration Source initially suspected the LS paper WebSocket endpoint closed
most subscription connections, which would have implied a systematic paper WS
limitation. That theory was disproven.

Root cause was a certification harness bug: a shared WebSocket manager sender
died after the first TR closed its connection, so later TRs failed even though
the gateway was not systematically rejecting them. Using a fresh client/manager
per certification TR resolved the issue.

Post-fix evidence recorded:

- all 65 `ws-stock` TRs certified at Transport against `LS_PAPER_*`;
- all 6 WebSocket futures/options TRs in that run certified at Transport
  against `LS_PAPER_*`;
- `SC0` (주식주문접수) and `SC1` (주식주문체결) reached FrameDecode from a
  guarded order lineage.

The maintained takeaway is: do not classify broad WebSocket paper failures as a
gateway limitation until the harness lifecycle has been ruled out. The current
repo carries this lesson in the realtime code through per-manager lifecycle,
reconnect replay, explicit terminal errors, and `LatestOnly` lost-wakeup tests.

## Timing And Event Residuals

FrameDecode absence is not automatically a failure. The old evidence separated
transport reachability from live data/event delivery:

- Market-data channels such as `S3_`, `K3_`, `H1_`, and `HA_` require useful
  market-open windows for real frames.
- Event channels such as volatility-interruption or futures-session channels
  require the relevant market event to occur.
- Order-event channels such as `SC0` and `SC1` require a real guarded order
  event during the subscription window.
- A closed-market or inactive-event run can still produce valid Transport
  evidence.

For future maintained expansion, a WebSocket TR can become implemented on
transport lifecycle evidence, but any recommendation must scope the claim:
Transport proves connect/subscribe/unsubscribe reachability; FrameDecode proves
one live row shape; neither proves gapless delivery, reconnection correctness
for every venue, or market-data completeness.

## Old Shard Map

The Migration Source grouped generated WebSocket evidence into these shards:

| Shard | Modules | TR count | Old test binary |
|---|---|---:|---|
| `ws-stock` | stock | 65 | `stock_ws_certification` |
| `ws-futures-options` | futures_options | 31 | `futures_options_ws_certification` |
| `ws-combined` | overseas_futures, overseas_stock, realtime_invest, misc, sector | 20 | `combined_ws_certification` |

These shards are not preserved as release gates in this repository. The counts
and grouping are preserved as migration knowledge for future expansion planning.

## Future Expansion Guidance

When adding a new maintained WebSocket TR:

1. Start with Transport evidence: paper connect, subscribe, unsubscribe, and no
   immediate protocol error.
2. Classify FrameDecode as timing/event-dependent unless the TR naturally emits
   rows during the test window.
3. Use a fresh or well-isolated WebSocket manager for certification-like runs so
   one closed connection cannot poison later TRs.
4. Record whether the claim is lifecycle-only or includes row decoding.
5. Keep backpressure scope explicit: `DropNewest` and `LatestOnly` are not
   gapless delivery guarantees.
6. Avoid raw frame logging because ACK frames can contain credential material.

