# Runbook — Rung-1 Attended Session (Production Ladder)

One page for the operator running the **first live rung**. Values are frozen in
[`config/preregistration.json`](config/preregistration.json) (rationale:
[`config/PREREGISTRATION.md`](config/PREREGISTRATION.md)). Rung 1 = **0.10×** budget,
watchdog **90 s** heartbeat, **300,000 KRW** session breaker, **5** clean sessions to escalate.

> Attended only. `node.run` is never driven by the commit gate. Every session runs with an
> operator present **and** the watchdog envelope. A limit event at rung 1 → **rung-0 suspend**.

## 0. Preconditions (all must hold)

- [ ] KRX regular session open (09:00–15:30 KST).
- [ ] `LS_TRADING_ENV=paper` (today `RunSource::Live` is paper-live; the live-lane flip is separate).
- [ ] The per-lane env file present and correct (`.env.domestic` by default) — token-bound account; a wrong lane reads silently wrong.
- [ ] Account **flat**, no stranded resting orders (the gate checks this; clear or defer explicitly).
- [ ] Catalog watermark fresh with a non-empty bar sample for today's KST date.

## 1. Environment

```sh
cd adapters/nautilus/lab
export LS_TRADING_ENV=paper
export LS_LANE=domestic                                   # → .env.domestic
export LS_DATA_HOME=/ABSOLUTE/path/to/data-home           # chain, registry, catalog live here
export LS_DISPATCH_PREREG="$PWD/config/preregistration.json"
```

## 2. One-time: register the chain (genesis → rung 1)

```sh
export LS_DISPATCH_NONCE=$(date +%s)      # genesis is nonce-gated + attended (no override)
cargo run --release -p nautilus-ls-lab --bin lab-live -- --genesis
```

Creates the genesis record authorizing **rung 1**. Nonce-gated: refused without a fresh nonce
or in an unattended shell. Skip if the chain already exists (re-genesis of a valid chain is refused).

## 3. Every session: run the dispatch gate

```sh
export LS_DISPATCH_NONCE=$(date +%s)      # fresh unix-seconds nonce (600 s TTL); attended only
cargo run --release -p nautilus-ls-lab --bin lab-live -- --dispatch
```

Read the exit code — **never** infer success from log text:

| exit | meaning | action |
|---|---|---|
| **0** | Green (or all reds deferred) | proceed to the attended mount |
| **1** | Refused — a non-deferrable red, or an undeferred red | fix the named check, or defer a *deferrable* one (below); re-run |
| **75** | Throttled (IGW00201 during a live-touching check) | wait, re-run — **not** a failure, never terminal |

Non-deferrable (no override): trading-env interlock, kill-switch state, account flat-start,
rung authorization. Deferrable (explicit only) — e.g. a stranded resting order:

```sh
export LS_DISPATCH_DEFER="stranded_orders"   # named item; recorded with your nonce, per-session
```

Every attempt — green or refused — appends a chain record. A refusal is history, not a silent exit.

## 4. Attended mount + watchdog

The mounted LiveNode session is the operator-attended step (outside the commit gate). While it runs:

- **Keep the watchdog fed.** Refresh the operator keepalive within **90 s** and keep the runtime
  ticking. A stale feeder (either one) trips the dead-man → teardown (stop → cancel → flat-check →
  halt) → a safety-trip record. **At rung 1 any trip suspends to rung 0** — do not step away past 90 s.
- **Breaker:** realized + conservatively-marked open P&L worse than **−300,000 KRW** flattens and
  cancels first, then engages the kill switch. Also a rung-1 limit event → rung-0 suspend.
- On any hard-fail, finalize still runs and marks the run abnormal — the session leaves scannable
  artifacts. After the close, run the post-session catalog ingest of today's KST date (prerequisite
  for the tracking-error twin).

## 5. After the session

- A session is **clean** iff: finalized, zero limit events, required reports present, not a
  probation session. 5 clean rung-1 sessions → request escalation (an operator-nonce'd step that
  re-verifies the evidence and checks cumulative P&L sits in the rung-1 band **[−69k, +533k]**).
- A **limit event** (non-flat close, reconcile-Unknown, any safety firing, `.tmp-` residue, or
  cum P&L outside the band) auto-de-escalates on the next dispatch. From rung 1 that is **rung-0
  suspension** — re-entry needs the `rung0_requalification` terms (root cause + ≥3 clean paper
  sessions + attended re-registration).

## Stop conditions — surface, don't guess

Rung above what the chain authorizes (refused by design); a check reading opposite to reality
(e.g. flat-start green with a known open position); anything that would re-order the teardown
(halt must stay last). Stop and report.
