# Runbook — Rung-1 Attended Session (Production Ladder)

One page for the operator running the **first live rung**. Values are frozen in
[`config/preregistration.json`](config/preregistration.json) (rationale:
[`config/PREREGISTRATION.md`](config/PREREGISTRATION.md)). Rung 1 = **0.10×** budget,
watchdog **90 s** heartbeat, **300,000 KRW** session breaker, **5** clean sessions to escalate.

> **LADDER STOOD DOWN — 2026-07-31 (recorded suspension). Do NOT authorize an attended
> session.** The documented head (v35) is net-NEGATIVE after honest costs (net RoR −0.0006),
> so an attended session buys expected losses and the rung-2 escalation it would feed can
> never be authorized. The frozen prereg stays **v2**, untouched as the historical record —
> no cost-aware band was derived. Re-entry: a net-positive cost-aware head exists → that is
> a code-hash move (`code_change_resets_to_rung_1`) → fresh re-registration (v3+) re-derives
> the bands from that head's distribution before any genesis dispatch. See the TURN-LOG
> 2026-07-31 governance entry, `config/PREREGISTRATION.md` § Stand-down, and queue
> `rung1-ladder-reentry-net-positive-head`.

> **Head v35 — cost-aware re-measurement of the v34 identity.** The documented head is **v35**
> (`strategy_code_hash 7571abef…`, run `20260731T023138Z-backtest-orb-v35` — the
> orb-transaction-cost-model turn: v34's governed params re-measured with the sourced
> statutory + commission cost model armed; the head hash was `e5bc2ae8…` (v34, run
> `20260725T112423Z-backtest-orb-v34`) before the cost model landed in `orb.rs`). **v35 read
> net-NEGATIVE** (net RoR −0.0006 on the 2026-07-31 catalog) — see TURN-LOG before authorizing
> any attended session. The v34→v35 code hash change is a `code_change_resets_to_rung_1`
> event. The frozen v2 economic band **[−148k, +266k]** is still v34-derived and **zero-cost**
> — an inheritance resolved 2026-07-31 by the recorded **stand-down** above (the file stays
> v2 as history; the amendment queue item is retired). Confirm the binary embeds
> v35 with `lab-live --head` before genesis — the code hash is the sole discriminator (see
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
export LS_TURN_EXPECT_VERSION=35                          # head-version pin: keys the head params
                                                          #   robustly to v35 (mount/escalate/report)
export LS_CALENDAR_SNAPSHOT=/ABSOLUTE/path/to/state/krx.calendar.json
                                                          # REQUIRED: the calendar is Enforced, so an
                                                          #   unset snapshot is not "no calendar" — it is
                                                          #   `enforced-fail-closed`, and every dispatch
                                                          #   refuses on an Unavailable date
export LS_DISPATCH_LANE_ENV=/ABSOLUTE/path/to/.env.domestic
                                                          # REQUIRED from this CWD: the lane path defaults
                                                          #   to the RELATIVE `.env.<lane>`, resolved against
                                                          #   the process CWD, and it is read directly with
                                                          #   no upward search. `.env.domestic` lives at the
                                                          #   REPO ROOT, so from `lab/` the default misses it
                                                          #   and the gate reads `lane_env_present=false`
```

Confirm both landed before going further — these two are silent when wrong, not loud:

```sh
cargo run --release -p nautilus-ls-lab --bin lab-live -- --head 2>&1 | head -1
# want: `... auth=authorized ... action=enforced-active`
# NOT:  `... snapshot=not-configured action=enforced-fail-closed`
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

### 3a. The Unknown calendar date — expect this on the session morning

The calendar proves a **Trading Session** only from an observed KRX witness, so a date is
`trading_session` only *after* KRX has published it. Weekends and holidays are proven `closed`
forward, but **the current day reads `Unknown` while you are standing in it** — that is the
normal morning state, not a defect. `Unknown` is a non-deferrable red: `--dispatch` refuses,
and no deferral clears it.

The only thing that proceeds an Unknown date is a **bound, audited attended override**. Author
it as a file — never a bare env var, because it must carry a structured first-party citation
you cannot write by reflex:

```sh
cat > /ABSOLUTE/path/to/unknown-override.json <<'JSON'
{
  "kst_date": "2026-07-27",
  "run_id": "rung1-2026-07-27",
  "operator": "your-operator-id",
  "authorized_at_unix": 1785000000,
  "snapshot_artifact_id": "<artifact_id from the calendar-startup line>",
  "snapshot_calendar_id": "<calendar_id from the calendar-startup line>",
  "alerts": [],
  "reason": "KRX published the regular trading-day schedule for this date; verified open",
  "citation": { "reference": "<KRX notice number or URL>", "issuer": "KRX", "note": null }
}
JSON

export LS_DISPATCH_RUN_ID=rung1-2026-07-27          # the override binds to THIS run id
export LS_DISPATCH_UNKNOWN_OVERRIDE=/ABSOLUTE/path/to/unknown-override.json
```

It is refused unless **all** of these hold — each one fails closed:

- `kst_date` equals today's KST date **and** `run_id` equals `LS_DISPATCH_RUN_ID` exactly;
- `operator`, `reason`, `citation.reference` and `citation.issuer` are all non-blank;
- `snapshot_artifact_id` / `snapshot_calendar_id` match the snapshot **actually in force**
  (copy them from the `calendar-startup` line — an override authored against a different
  snapshot reviewed different alerts and cannot speak for this run);
- the run is attended with a fresh `LS_DISPATCH_NONCE`.

A named file that is missing, unparseable, or audit-incomplete is a **hard error**, never a
quiet "no override" — a typo'd path can't masquerade as a decision not to override.

It flips **only** an Unknown-date refusal. It can never green a proven `Closed`, an
`Unavailable` calendar, or any other check, and it never changes the calendar status. The full
override lands in the chain record for review.

## 4. Attended mount — `--mount` RUNS the session

`--mount` is the live driver: it consumes the green dispatch, drives `node.run`, runs the
fail-closed teardown, and finalizes the run. Nonce-gated, attended, **paper-only**.

**Create the operator keepalive file first** — its mtime is the operator dead-man feeder, and
an absent file reads as stale, so the mount refuses without it:

```sh
export LS_MOUNT_KEEPALIVE=/ABSOLUTE/path/to/rung1.keepalive
touch "$LS_MOUNT_KEEPALIVE"
```

**Resolve the universe file first.** `--mount` does NOT run `select_universe` — it trades
exactly what this file contains, so the file is part of the head's behavioral surface. Produce
it with `lab-mount-universe` (no nonce) rather than by hand; it reuses the backtest's own
ATR/turnover/selection helpers, so it cannot drift from the head:

```sh
# Prior-session values come from the catalog, so ingest must be current through the PREVIOUS
# session. `today_open` for a same-day run comes from a live t8407 quote instead — the catalog
# cannot hold an in-session daily bar — which is this binary's one gateway call and is why
# LS_DISPATCH_LANE_ENV is required here.
export LS_MOUNT_UNIVERSE_DATE=2026-07-27                        # the KST session date
export LS_DISPATCH_LANE_ENV=/ABSOLUTE/path/to/.env.domestic     # REQUIRED when the date is today
export LS_MOUNT_UNIVERSE_METADATA=/ABSOLUTE/path/to/universe-metadata-YYYYMMDD.json
                                                                # only if the head run was
                                                                #   metadata-driven; omitting it
                                                                #   against a metadata-driven head
                                                                #   changes the tradability gate
cargo run --release -p nautilus-ls-lab --bin lab-mount-universe -- \
  --out /ABSOLUTE/path/to/universe.json
```

Never hand-author it. A row missing `prior_atr` does not fail — the OR-width gate
(`or_width_max_atr = 0.666`, armed in v34) is *skip-not-reject*, so it silently switches OFF
for that symbol and emits no reject envelope. The producer drops such symbols loudly instead.

Three refusals the PRODUCER can raise on a session morning, all before any nonce is spent:

- **Before 09:00 KST it refuses outright.** t8407 answers outside the session with the previous
  session's snapshot, whose `open` is a positive number, so producing early would resolve the
  whole file against yesterday's opens. Wait for the opening auction and re-run.
- **A prior session older than 10 calendar days is refused.** That means ingest is behind; catch
  the catalog up through the previous session first.
- **A symbol the gateway does not echo at all aborts the run** (a request/framing fault). A
  symbol that IS echoed but has no open yet is dropped and named, with pre-open and
  wire-shape causes reported on separate lines — if you see the WIRE-SHAPE warning, waiting
  will not help.

Two refusals to expect from `--mount` itself, both **pre-consume** (they cost you nothing but a
re-run):

- **Every row carries `session_date`, and `--mount` refuses a file built for another day.**
  Resolve the universe fresh each morning; a leftover file would otherwise trade yesterday's
  symbols at yesterday's opening prices, and nothing downstream would notice.
- **If the head run is metadata-driven, the producer refuses without
  `LS_MOUNT_UNIVERSE_METADATA`**, and refuses an artifact whose hash is not the one that head
  was built from. Omitting it would silently drop the tradability gate.

```sh
export LS_DISPATCH_NONCE=$(date +%s)                            # fresh nonce; refused in a no-TTY shell
export LS_MOUNT_UNIVERSE_FILE=/ABSOLUTE/path/to/universe.json   # produced by lab-mount-universe
export LS_MOUNT_SESSION_SECS=21600                              # optional; default 6 h
export LS_MOUNT_STOP_GRACE_SECS=60                              # optional; default 60 s, clamped to
                                                                #   [1, heartbeat_interval_secs]
export LS_MOUNT_STARTING_BALANCE=10000000                       # optional; recorded on the equity curve
cargo run --release -p nautilus-ls-lab --bin lab-live -- --mount
```

Order is the safety property, and it is why a bad input costs you nothing:

1. the **paper interlock** (exit `66` if `LS_TRADING_ENV != paper`);
2. the **attendance/nonce gate** (exit `77` in a no-TTY shell or without a fresh nonce), and the
   read-only mountability peek (also `77` when there is no green/unconsumed/same-day dispatch);
3. **every fail-closed precheck, all before the consume** — the pre-registered fraction, the
   **watchdog envelope arming** (a prereg missing the heartbeat interval or the max-loss threshold
   refuses: a half-envelope never runs a session), the keepalive file, v34's **real** governed
   params (a zero-size head is refused), the universe, and the node build. Any failure here exits
   `71` and **does not consume the green dispatch** — fix and re-run `--mount`, no fresh
   `--dispatch` needed;
4. only then `authorize_mount` **consumes** the dispatch and takes the Live lock;
5. the session runs; at the end (timer or trip) exactly **one** fail-closed teardown fires, and the
   run finalizes.

| exit | meaning | action |
|---|---|---|
| **0** | ran and finalized clean (teardown confirmed flat) | §5 post-session verification |
| **66** | not paper | set `LS_TRADING_ENV=paper` |
| **71** | pre-consume precheck failed — **dispatch not consumed** | fix the named input, re-run `--mount` |
| **72** | ran, finalized **ABNORMAL** — the teardown could not confirm flat, **or** the node was hard-stopped, **or both** (each cause prints its own stderr line; neither hides the other) | reconcile the account; `--clear-killswitch` when a trip is recorded |
| **77** | attendance/nonce refusal, or no mountable dispatch — nothing consumed | run attended with a fresh nonce, or re-run `--dispatch` |

While the session runs:

- **Keep the watchdog fed.** Refresh the operator keepalive (`touch "$LS_MOUNT_KEEPALIVE"`) within
  **90 s**, and the strategy touches the runtime feeder on every processed bar. A stale feeder
  (either one) trips the dead-man → teardown → a safety-trip record. **At rung 1 any trip suspends
  to rung 0** — do not step away past 90 s.
- **Breaker:** realized P&L (matched offsetting fills against cost basis) plus open positions marked
  at the **adverse edge** — and, when the market-data feed is stale or absent, at the position's
  **stop level** rather than a last-seen favorable price — worse than **−300,000 KRW** tears down
  and engages the kill switch.
- **The teardown sequence** (unchanged, and never re-ordered): stop the strategy's order emission →
  **quiesce the in-flight order dispatches** → cancel every resting order (retried, fail-closed) →
  positively confirm flat via t0424 `janqty` + t0425 `ordrem` → **halt LAST**. It never places a
  flattening order: a non-flat close is an abnormal finalize plus an operator reconcile, never an
  auto-flatten.
- On any hard-fail, finalize still runs and marks the run abnormal — the session leaves scannable
  artifacts. After the close, run the post-session catalog ingest of today's KST date (prerequisite
  for the tracking-error twin).

### One operator-visible residual, and the backstop that covers the other

- **Residual.** A market-data lull or a `node.run` drain that exceeds the heartbeat interval **can
  trip the dead-man**. That fails safe (halt + kill switch), but it reds the next `--dispatch` until
  a nonce-gated `--clear-killswitch`. Set the pre-registered interval with that in mind.
- **Covered: a `node.run` that hangs on stop.** `handle.stop()` is a request, not a guarantee, and
  the dead-man does not cover this case — a node hung on stop while the strategy keeps touching the
  runtime heartbeat never goes stale. The driver therefore applies its own **timed hard-stop**: once
  *any* party has asked the node to stop (the session timer, the watchdog, or the mutual-liveness
  loop), the node has `LS_MOUNT_STOP_GRACE_SECS` to return. If it does not, the driver abandons it
  and runs the same fail-closed teardown and finalize, so the session leaves a **finalized,
  scannable run** — not `.tmp-<run_id>` residue — with a `HARD STOP` line in its `data_quality` and
  exit code `72`.
  - Default 60 s, **clamped to `[1 s, heartbeat_interval_secs]`** — it cannot be disabled at either
    end. The ceiling is the pre-registered interval (90 s) because a longer grace hands the race
    back to the dead-man, which trips first on the stalled drain and reds the next `--dispatch`
    for what the driver would otherwise have handled on its own. Setting it higher does nothing.
  - A hard-stop engages the kill switch **in-process only** and appends no chain record, so
    `--clear-killswitch` is needed only when a watchdog trip is also recorded. Reconcile the account
    against the run's `data_quality` before the next dispatch either way.
  - **It is still a limit event.** The run's `data_quality` carries a typed `hard_stopped: true`,
    which `scan_limit_events` reads as a `hard_stop` event (so the ladder de-escalates — at rung 1,
    to rung 0) and which reds the readiness window. This is deliberate: it is the typed successor
    to the `.tmp-` residue an un-backstopped hang used to leave. A session whose node had to be
    abandoned never counts as one of your K clean sessions.

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
