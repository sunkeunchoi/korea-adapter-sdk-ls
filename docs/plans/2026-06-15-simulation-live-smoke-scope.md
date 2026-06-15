# Simulation Live Smoke Scope

Date: 2026-06-15

## Summary

The next migration step is a documentation-only scope decision for a future **Simulation Live Smoke** over the existing maintained SDK vertical slice. This is not new TR expansion and not production evidence. The goal is to prove that the already-implemented SDK path can reach the LS simulation gateway with real credentials, while keeping `.env` small and avoiding date-sensitive ambiguity.

## Decisions

- The next technical scope is a gated live-smoke harness for the existing slice, not implementation of more TRs.
- The live smoke targets LS simulation credentials only.
- The SDK must continue to read ordinary environment variables through `LsConfig::from_env()`; dotenv loading belongs only in a future repo-level convenience wrapper.
- The default smoke should be small: OAuth token acquisition plus one harmless market-data REST call (`t1102`).
- The default smoke should not include `t8412`, because `t8412` requires an explicit real Korean trading day and `.env` should stay credential-only.
- `t8412` should be a separate explicit chart smoke target requiring the date on the command line or environment for that invocation.
- `CSPAQ12200` account-state smoke is read-only and opt-in.
- `S3_` WebSocket smoke is opt-in and timeboxed; connect/subscribe/unsubscribe lifecycle is the blocking assertion, while receiving a live row is extra evidence.
- No order runtime or order-capable smoke is in scope.

## Minimal `.env`

Keep `.env` to credentials only:

```env
LS_TRADING_ENV=paper
LS_PAPER_APPKEY=...
LS_PAPER_SECRET=...
LS_PAPER_ACCOUNT=...
```

Optional overrides should be supplied at invocation time instead of being required in `.env`:

```sh
LS_LIVE_SMOKE_SHCODE=005930
LS_LIVE_SMOKE_T8412_DATE=YYYYMMDD
```

`LS_LIVE_SMOKE_SHCODE` may default to a conservative market-data symbol in the future harness. `LS_LIVE_SMOKE_T8412_DATE` must not have a baked-in default.

## Future Command Shape

The preferred future interface is a repo-level `Makefile` wrapping ignored Rust integration tests:

```sh
make live-smoke
make live-smoke-chart LS_LIVE_SMOKE_T8412_DATE=YYYYMMDD
make live-smoke-account
make live-smoke-ws
```

The Make targets may load the gitignored `.env` for convenience, but the Rust SDK should not gain dotenv behavior.

## Date Safety

The chart smoke must fail before network I/O when `LS_LIVE_SMOKE_T8412_DATE` is missing or malformed. The value must be `YYYYMMDD` and must be a real Korean trading day. A weekday check is not sufficient because holidays can still fail at the LS gateway.

The harness should not silently skip `t8412` under a green default result. Instead, `t8412` lives behind the explicit `live-smoke-chart` target so date-bearing evidence is deliberate.

## Pass Criteria

Default `live-smoke`:

- Environment resolves as simulation/paper, not production.
- OAuth token acquisition succeeds.
- `t1102` quote request succeeds for the selected symbol.

Explicit `live-smoke-chart`:

- All default live-smoke checks are allowed but not required by this target's name.
- `LS_LIVE_SMOKE_T8412_DATE` is present and validly formatted before network I/O.
- One `t8412` page succeeds for the selected symbol and date.
- Full `collect_all` pagination is not part of this smoke.

Explicit `live-smoke-account`:

- `CSPAQ12200` runs as a read-only account-state inquiry.
- Account-state failures should be reported separately from market-data smoke failures because they may reflect simulation account setup rather than SDK transport correctness.

Explicit `live-smoke-ws`:

- The SDK connects to the simulation WebSocket URL.
- The SDK subscribes to `S3_` for the selected symbol and unsubscribes cleanly.
- Receiving a row is recorded if it happens, but absence of a row during the timeout is not a failure.

## Out Of Scope

- Production live smoke.
- Order runtime or any order-capable live behavior.
- New TR implementation.
- `ls-trackers` or change-driven evidence invalidation.
- Treating smoke success as automatic **Focused Evidence** without recording target, inputs, and result.
