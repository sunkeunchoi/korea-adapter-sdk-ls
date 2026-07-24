# Runbook — Rung-1 Attended Session (Production Ladder)

One page for the operator running the **first live rung**. Values are frozen in
[`config/preregistration.json`](config/preregistration.json) (rationale:
[`config/PREREGISTRATION.md`](config/PREREGISTRATION.md)). Rung 1 = **0.10×** budget,
watchdog **90 s** heartbeat, **300,000 KRW** session breaker, **5** clean sessions to escalate.

> **Head v34 — re-registered v2.** The certified real-universe head is **v34**
> (`strategy_code_hash d7a9820b…`). The v30→v34 code hash change is a
> `code_change_resets_to_rung_1` event, so the ladder starts at rung 1 and the economic band is
> re-derived from v34: **[−148k, +266k]** (v1/v30 was [−69k, +533k]). Confirm the binary embeds
> v34 with `lab-live --head` before genesis — the code hash is the sole discriminator (see
> [`RUNG1-PREFLIGHT.md`](RUNG1-PREFLIGHT.md)).

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
| **0** | Green (or all reds deferred) | proceed to the attended mount (§4) |
| **1** | Refused — a non-deferrable red, or an undeferred red | fix the named check, or defer a *deferrable* one (below); re-run |
| **75** | Throttled (IGW00201 during a live-touching check) | wait, re-run — **not** a failure, never terminal |

Non-deferrable (no override): trading-env interlock, kill-switch state, account flat-start,
rung authorization. Deferrable (explicit only) — e.g. a stranded resting order:

```sh
export LS_DISPATCH_DEFER="stranded_orders"   # named item; recorded with your nonce, per-session
```

Every attempt — green or refused — appends a chain record. A refusal is history, not a silent exit.

## 4. Attended mount + watchdog

Prepare the session with the wired command (nonce-gated, attended, **paper-only**):

```sh
export LS_DISPATCH_NONCE=$(date +%s)                       # fresh nonce; refused in a no-TTY shell
export LS_MOUNT_UNIVERSE_FILE=/ABSOLUTE/path/to/universe.json   # resolved daily/t8407 universe
cargo run --release -p nautilus-ls-lab --bin lab-live -- --mount
```

`--mount` resolves the pre-registered rung fraction (**0.10**), sources v34's **real** governed
params (never the all-levers-off default — a wrong head that sizes to zero is refused), builds the
live node at 0.10× size, and reports readiness (**exit 70** = prepared). It **hard-refuses unless
`LS_TRADING_ENV=paper`** (exit 66) and refuses loudly in a no-TTY shell (exit 77) — it never
consumes the green dispatch on a refusal.

> **Deferred:** the attended live-session **driver** (`node.run → fail-closed teardown → finalize`)
> is a follow-up — `--mount` prepares and stops at that seam without consuming the dispatch. The
> watchdog / breaker / teardown guidance below is the design for when the driver lands.

While the mounted LiveNode session runs (once the driver lands):

- **Keep the watchdog fed.** Refresh the operator keepalive within **90 s** and keep the runtime
  ticking. A stale feeder (either one) trips the dead-man → teardown (stop → cancel → flat-check →
  halt) → a safety-trip record. **At rung 1 any trip suspends to rung 0** — do not step away past 90 s.
- **Breaker:** realized + conservatively-marked open P&L worse than **−300,000 KRW** flattens and
  cancels first, then engages the kill switch. Also a rung-1 limit event → rung-0 suspend.
- On any hard-fail, finalize still runs and marks the run abnormal — the session leaves scannable
  artifacts. After the close, run the post-session catalog ingest of today's KST date (prerequisite
  for the tracking-error twin).

## 5. After the session

After the close, run the post-session catalog ingest of today's KST date, then the read-only
verification (agent-runnable, appends nothing):

```sh
cargo run --release -p nautilus-ls-lab --bin lab-live -- --rung-report
```

It prints the head hash it evaluated under, the clean/limit-event classification of the trailing
sessions, cumulative rung-1 P&L against **[−148k, +266k]**, N-progress, and the readiness verdict.

- A session is **clean** iff: finalized, zero limit events, required reports present, not a
  probation session. 5 clean rung-1 sessions → request escalation:

  ```sh
  export LS_DISPATCH_NONCE=$(date +%s)
  cargo run --release -p nautilus-ls-lab --bin lab-live -- --escalate
  ```

  `--escalate` re-verifies the evidence and checks cumulative P&L sits in the v34 rung-1 band
  **[−148k, +266k]** — note the **halved ceiling** (+533k → +266k), so a strongly-profitable
  streak (cum > +266k) also blocks escalation as "outside band."
- A **limit event** (non-flat close, reconcile-Unknown, any safety firing, `.tmp-` residue, or
  cum P&L outside the band) auto-de-escalates on the next dispatch. From rung 1 that is **rung-0
  suspension** — re-entry needs the `rung0_requalification` terms (root cause + ≥3 clean paper
  sessions + attended re-registration via `--reregister`).
- After an **auto-halt** (kill-switch trip), re-arm trading with `--clear-killswitch` (nonce +
  attended; captures a scrubbed operator who/why). `--reregister` may only requalify to rung 0 or
  repair to ≤ the chain-earned rung — an upward jump past the escalation gate is refused.

## Stop conditions — surface, don't guess

Rung above what the chain authorizes (refused by design); a check reading opposite to reality
(e.g. flat-start green with a known open position); anything that would re-order the teardown
(halt must stay last). Stop and report.
