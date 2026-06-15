---
date: 2026-06-15
topic: paper-live-smoke
---

# Paper Live Smoke — Requirements

## Summary

Define a credential-gated **Paper Live Smoke** harness over the existing SDK slice: a default token + `t1102` check, plus opt-in chart, account, and WebSocket targets, run through Makefile wrappers over ignored Rust integration tests. Standardize the SDK on two environments — Paper (default) and Real — and keep `.env` credential-only.

---

## Problem Frame

The maintained SDK already implements a vertical slice — `token`/`revoke`, `t1102`, `t8412`, `CSPAQ12200`, and `S3_` — but nothing yet proves that slice still reaches the live LS gateway with real credentials. The next migration step is a narrow, credential-gated check: not new TR work, and not production evidence.

Two hazards shape it. First, LS serves paper and real REST traffic from one host (`https://openapi.ls-sec.co.kr:8080`), so the environment is a credential-and-enum fact, never a URL fact — the old repo's Phase 46 research confirmed there is no server-side signal that distinguishes the two. Second, `t8412` is date-sensitive: an empty date defaults to "today" on the gateway and fails on non-trading days. The harness must make both hazards explicit rather than letting a green run hide them.

---

## Key Decisions

- **Client-side hard-refuse is the only available prod guard.** Because LS exposes no server-side environment signal, the harness cannot confirm "I am on paper" from any response. The strongest protection that can exist is a pre-flight assertion on the resolved `environment`, run before any network I/O. That makes it a real gate, not a weak fallback.

- **Two environments, Paper default, no aliases.** The SDK collapses to exactly `Environment::Paper` and `Environment::Real`. `LS_TRADING_ENV` accepts only `paper` and `real`; unset resolves to Paper. The former aliases (`simulation`, `sim`, `production`, `prod`) are removed so one concept never carries two names. This is a breaking change to `ls-core`, scoped to the maintained repo.

- **The default smoke is date-free; the chart is quarantined.** Default `live-smoke` is token + `t1102` only. `t8412` lives behind its own `live-smoke-chart` target so date-bearing evidence is always deliberate and a green default never implies the chart was exercised.

- **Holiday correctness belongs to the gateway, not a shipped calendar.** Pre-network validation enforces `YYYYMMDD` format and a weekday check. Whether a date is a real Korean trading day is the gateway's verdict (error `01715` on an empty date over a holiday); the harness does not ship or maintain a KRX holiday calendar. No TR in the current slice can supply a trading day either — `t1102OutBlock` carries no date field, and the only API-native trading-day source is the daily-chart family (`t8410`/`t8413`/`t8451`), whose latest returned row dates the last trading day but which is unimplemented and out of scope. Auto-deriving the date is therefore a future option gated on new TR work.

- **Standardize on "Paper" end to end.** The concept "Simulation Live Smoke" becomes "Paper Live Smoke" in the vocabulary and this doc, and the Real credential prefix `LS_PROD_*` becomes `LS_REAL_*`.

- **Smoke is not automatically Focused Evidence.** A green smoke supports Focused Evidence only when its target, inputs, and result are recorded — never by passing alone.

---

## Requirements

**Environment model and safety**

- R1. The SDK exposes exactly two environments: `Environment::Paper` (default) and `Environment::Real`. No other variant exists.
- R2. `LS_TRADING_ENV` accepts only `paper` and `real` (case-insensitive); when unset it resolves to Paper; any other value is a hard `LsError::Config`. The `simulation`, `sim`, `production`, and `prod` aliases are removed.
- R3. `Environment`'s `Display` renders `paper` and `real` symmetrically; the former `production` rendering for Real is removed.
- R4. The default `live-smoke` target hard-refuses before any network I/O — no token acquisition, no REST call — unless the resolved environment is Paper. The check reads the resolved `environment`, the only available signal, because LS provides no server-side environment confirmation.
- R5. The Real credential env vars are renamed `LS_PROD_*` → `LS_REAL_*` (`LS_REAL_APPKEY`, `LS_REAL_SECRET`, `LS_REAL_ACCOUNT`); the legacy unprefixed `LS_APPKEY`/`LS_SECRET`/`LS_ACCOUNT` fallback is retained. The paper smoke never reads this set.

**Harness targets and pass criteria**

- R6. Default `make live-smoke`: resolve environment as Paper, acquire an OAuth token, and perform one `t1102` quote for the selected symbol — all three must pass.
- R7. `make live-smoke-chart`: requires `LS_LIVE_SMOKE_T8412_DATE`, then fetches one `t8412` page for the selected symbol and date. Full `collect_all` pagination is excluded.
- R8. `make live-smoke-account`: runs `CSPAQ12200` as a read-only account-state inquiry. Account-state failures are reported separately from market-data failures, since they may reflect paper-account setup rather than SDK transport correctness.
- R9. `make live-smoke-ws`: connects to the Paper WebSocket URL, subscribes to `S3_` for the selected symbol, and unsubscribes cleanly. The connect/subscribe/unsubscribe lifecycle is the blocking assertion; receiving a live row is recorded as extra evidence, but its absence within the timeout is not a failure. The target is timeboxed.
- R10. No order runtime or order-capable behavior is exercised by any target.

**Configuration and `.env`**

- R11. `.env` holds credentials only: `LS_TRADING_ENV=paper`, `LS_PAPER_APPKEY`, `LS_PAPER_SECRET`, `LS_PAPER_ACCOUNT`.
- R12. Per-invocation overrides are supplied on the command line or environment, not required in `.env`: `LS_LIVE_SMOKE_SHCODE` (optional; may default to a conservative market-data symbol) and `LS_LIVE_SMOKE_T8412_DATE` (no baked-in default).
- R13. The SDK continues to read ordinary environment variables through `LsConfig::from_env()` and gains no dotenv behavior. The Makefile targets may load the gitignored `.env` for convenience.

**Date safety**

- R14. The chart smoke fails before any network I/O when `LS_LIVE_SMOKE_T8412_DATE` is missing or malformed; the value must be `YYYYMMDD`.
- R15. Pre-network validation covers format and a weekday check only. A date that passes offline validation but falls on a market holiday fails at the gateway and is reported as such.
- R16. The harness never silently skips `t8412` under a green default result; date-bearing chart evidence exists only behind the explicit `live-smoke-chart` target.

**Terminology**

- R17. "Simulation Live Smoke" is renamed "Paper Live Smoke" in `CONTEXT.md` and this document, and "Simulation" moves to the avoid list. The rename lands in the maintained `korea-adapter-sdk-ls` repo only; the frozen `korea-broker-sdk-ls` Migration Source is untouched.

**Evidence semantics**

- R18. Smoke success does not automatically count as Focused Evidence; it may support Focused Evidence only when its target, inputs, and result are recorded.

---

## Acceptance Examples

- AE1. Prod guard fires.
  - **Covers R4.**
  - **Given** `LS_TRADING_ENV=real`, **when** `make live-smoke` runs, **then** it aborts with a config error before any token acquisition or REST call.

- AE2. Default smoke passes.
  - **Covers R6.**
  - **Given** Paper credentials in `.env`, **when** `make live-smoke` runs, **then** environment resolves as Paper, an OAuth token is acquired, and one `t1102` quote for the selected symbol succeeds.

- AE3. Missing chart date fails early.
  - **Covers R14.**
  - **Given** `LS_LIVE_SMOKE_T8412_DATE` is unset, **when** `make live-smoke-chart` runs, **then** it fails before any network I/O with a message naming the missing date.

- AE4. Malformed or weekend chart date fails early.
  - **Covers R14, R15.**
  - **Given** `LS_LIVE_SMOKE_T8412_DATE=20260613` (a Saturday) or a non-`YYYYMMDD` value, **when** `make live-smoke-chart` runs, **then** it fails before any network I/O.

- AE5. Holiday chart date fails at the gateway, reported.
  - **Covers R15.**
  - **Given** a well-formed weekday date that is a market holiday, **when** `make live-smoke-chart` runs, **then** offline validation passes, the `t8412` request reaches the gateway, and the gateway rejection is reported as a chart failure (not an offline-validation failure).

- AE6. WebSocket lifecycle passes without a row.
  - **Covers R9.**
  - **Given** the `S3_` subscription connects, subscribes, and unsubscribes cleanly but no live row arrives within the timeout, **when** `make live-smoke-ws` runs, **then** the target passes and records that no row was received.

- AE7. Account failure is isolated.
  - **Covers R8.**
  - **Given** `CSPAQ12200` returns an account-state error, **when** `make live-smoke-account` runs, **then** the failure is reported as an account-state failure, distinct from market-data smoke results.

---

## Scope Boundaries

- Production (Real) live smoke.
- Order runtime or any order-capable live behavior.
- New TR implementation.
- `ls-trackers` and change-driven evidence invalidation.
- A shipped or maintained KRX trading-day calendar — holiday correctness stays the gateway's job.
- Auto-deriving the chart date from a daily-chart TR (`t8410`/`t8413`/`t8451`) — that source is unimplemented, and adding it is new TR work.
- Treating smoke success as automatic Focused Evidence.
- The frozen `korea-broker-sdk-ls` Migration Source — no rename or edit there.

---

## Dependencies / Assumptions

- The five slice TRs (`token`/`revoke`, `t1102`, `t8412`, `CSPAQ12200`, `S3_`) are implemented and reachable; the smoke verifies live transport, not new behavior.
- Operators keep Paper and Real credentials separate. A `LS_TRADING_ENV=paper` run with Real keys in `LS_PAPER_*` is undetectable by the harness or the SDK — there is no server-side signal — so this is an operator-discipline assumption, not an enforced guard.
- A real Paper account is provisioned for `CSPAQ12200`; account-state failures may reflect that provisioning rather than SDK correctness (the reason R8 isolates them).
- The Makefile is the convenience layer that may load `.env`; the SDK's env contract (`from_env`) stays dotenv-free.

---

## Outstanding Questions

**Deferred to planning**

- The conservative default symbol for `LS_LIVE_SMOKE_SHCODE`.
- Whether the weekday pre-check pins Korean local time explicitly to avoid day-boundary timezone edges.
- Where the ignored integration tests live and how the Makefile targets select them.

---

## Sources

- `docs/plans/2026-06-15-simulation-live-smoke-scope.md` — the scope decisions this doc formalizes.
- `CONTEXT.md` — Paper Live Smoke / Credentialed Live Smoke / Focused Evidence vocabulary.
- `crates/ls-core/src/config.rs` — current `Environment` model, `from_env`, alias list, and `Display` (the rename targets).
- `korea-broker-sdk-ls/docs/ENVIRONMENT_VERIFICATION_RESEARCH.md` — Phase 46 finding that no server-side paper/real signal exists.
- `docs/brainstorms/2026-06-15-sdk-first-slice-decisions-requirements.md` — the slice TR membership this smoke covers.
- `korea-broker-sdk-ls/docs/TR_DEPENDENCY_GUIDE.md` — trading-day sensitivity (`01715`) across TRs; confirms `t1102` carries no date and the daily-chart family is the API-native trading-day source.
