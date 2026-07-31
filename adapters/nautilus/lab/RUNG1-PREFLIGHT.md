# Rung-1 Preflight — agent-runnable vs operator-only

The contract for a rung-1 attended session against head **v35**. It splits what an **agent** may
run autonomously (no-TTY, read-only or non-mutating — offline except the one market-data read in
§0.7) from what only an **operator** may
run (nonce-gated, attended, refused in a no-TTY shell). Run the operator sequence from
[`RUNBOOK-rung1.md`](RUNBOOK-rung1.md); the frozen numbers live in
[`config/preregistration.json`](config/preregistration.json) (rationale
[`config/PREREGISTRATION.md`](config/PREREGISTRATION.md)).

> **The agent NEVER drives `--genesis`, `--dispatch`, `--mount`, `--escalate`, `--reregister`, or
> `--clear-killswitch`.** Those are nonce-gated, attended, and refuse loudly in a no-TTY shell
> (distinct exit codes — never look-like-ran). Build from `adapters/nautilus`; `make` breaks in
> spawned shells, so call `cargo` directly.

## §0 — Agent preflight (no nonce, no TTY; one market-data read when the date is today)

1. **Build `lab-live` from v35.** From `adapters/nautilus` (the CWD trap: from the repo root the
   lab crate is skipped):
   ```sh
   cd adapters/nautilus
   cargo build --release -p nautilus-ls-lab --bin lab-live
   ```
2. **Confirm the binary embeds v35** (`--head`, read-only, no nonce):
   ```sh
   cargo run --release -p nautilus-ls-lab --bin lab-live -- --head
   ```
   The binary embeds v35 **iff** the printed `strategy_code_hash` equals the documented head
   `7571abef…`. That hash is the **sole** discriminator — the binary carries no hash→version map,
   so `--head` prints no version, and the `governed_params_hash(default)` line is a
   **version-invariant constant** (identical across v9…v35), explicitly **not** a v35 confirmation.
3. **Confirm the pre-registration is v2.** `config/preregistration.json` has `"version": 2` and the
   rung-1 band `[-148000, +266000]`; its SHA-256 is the citation each dispatch records. The
   derivation is reproduced by `cargo test -p nautilus-ls-lab --test prereg_derivation`.
4. **Dry-read the exit-code contract** (do not infer success from log text):

   | command | exit | meaning |
   |---|---|---|
   | `--dispatch` | 0 / 1 / 75 | green (proceed) / refused / throttled (re-run, never terminal) |
   | `--mount` | 0 / 66 / 71 / 72 / 77 | **ran + finalized clean** / not-paper refusal / **pre-consume precheck failed (dispatch NOT consumed)** / ran but finalized **ABNORMAL** (teardown could not confirm flat) / no-TTY (or no mountable dispatch) refusal |
   | `--escalate` / `--reregister` / `--clear-killswitch` | 78 / 79 / 80 | that arm's loud refusal in a no-TTY shell |
   | `--head` / `--rung-report` | 0 | read-only diagnostics (no nonce, no chain append) |

   `--mount` **drives the session** now (it is no longer prepare-and-stop; exit `70` is retired).
   Only `71` and `77` leave the green dispatch unconsumed; `0` and `72` mean a session ran. `72` is
   never to be treated as success — the kill switch is engaged and its persisted record reds the
   next `--dispatch` until a nonce-gated `--clear-killswitch`.

5. **Inspect the chain head before the operator starts** (read-only): run `--rung-report` and read
   the printed head hash. A pre-existing chain under a **non-v35** head is a **stop-and-reconcile**
   (archive / epoch-repair or a fresh data home) — never a silent proceed.

6. **Confirm the two silent-when-wrong env vars are set.** Both fail quietly rather than loudly:
   - `LS_CALENDAR_SNAPSHOT` — unset reads as `snapshot=not-configured action=enforced-fail-closed`,
     which makes the date `Unavailable` and refuses **every** dispatch. Check the first line of
     `--head`: want `auth=authorized … action=enforced-active`.
   - `LS_DISPATCH_LANE_ENV` — the lane path defaults to the CWD-relative `.env.<lane>`, read
     directly with no upward search, while `.env.domestic` lives at the repo root. From
     `adapters/nautilus/lab` the default misses it and the gate reads `lane_env_present=false`.

7. **Resolve the mount universe** (agent-runnable, no nonce):
   ```sh
   LS_DATA_HOME=/ABSOLUTE/path/to/data-home \
   LS_MOUNT_UNIVERSE_DATE=<session KST date> \
   LS_MOUNT_UNIVERSE_METADATA=/ABSOLUTE/path/to/universe-metadata-YYYYMMDD.json \
   LS_DISPATCH_LANE_ENV=/ABSOLUTE/path/to/.env.domestic \
     cargo run --release -p nautilus-ls-lab --bin lab-mount-universe -- --out <path>
   ```
   **All four are load-bearing, and three of them fail closed rather than degrading** (verified
   2026-07-27):

   - `LS_DATA_HOME` — unconditionally required; without it the binary exits immediately with
     `LS_DATA_HOME is required`.
   - `LS_MOUNT_UNIVERSE_METADATA` — required **because the head is metadata-driven**. The
     producer refuses with `the head is METADATA-DRIVEN (its run carries universe_metadata_hash
     90005f88…) but LS_MOUNT_UNIVERSE_METADATA is unset — producing the universe without it
     silently drops the tradability gate and trades symbols the head excluded`. Point it at the
     artifact the head's run was built from (currently
     `lab/config/universe-metadata-20260723.json`); the binary validates the hash itself.
   - `LS_DISPATCH_LANE_ENV` — required only when the session date is **today** (see below), but
     harmless otherwise.

   Every prior-session value comes from the catalog. `today_open` depends on the date:

   - **A past date** — from that date's catalog daily bar. Fully offline, no gateway call.
   - **Today (KST)** — from a live `t8407` quote, because the catalog *cannot* hold an
     in-session daily bar (the accumulate path is calendar-gated on the same-day `Unknown`
     status and independently refuses to advance a watermark into an in-session day). This is
     the binary's only gateway call and it requires `LS_DISPATCH_LANE_ENV`; it still takes no
     nonce. A symbol with no usable live open (pre-open, halted) is dropped, never defaulted.

   The producer reuses the backtest's selection + ATR helpers so the file cannot drift from the
   head; never hand-author it (a row missing `prior_atr` silently disables the armed OR-width
   gate).

   It still requires prior-session history in the catalog — but "current through the previous
   session" is **not** achievable the night before: the ingest cannot advance into a date the
   calendar has not yet witnessed, so the previous session lands only after an operator
   `calendar-refresh` on the morning of. Eligibility is staleness, not same-day currency
   (`<= MAX_PRIOR_STALENESS_DAYS = 10`; `select_prior` takes the latest daily bar strictly before
   the session date), so a Tuesday session with only Friday on disk resolves — at a fidelity cost,
   since `prior_close` / `prior_atr` are then Friday's and the overnight-gap term becomes a
   multi-session return. See
   [`docs/solutions/workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md`](../../../docs/solutions/workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md).

## The Unknown calendar date is the normal morning state

A date is `trading_session` only from an observed KRX witness, so the **current day reads
`Unknown` while you are standing in it**. `Unknown` is a non-deferrable red: `--dispatch`
refuses, and no deferral clears it. The only thing that proceeds it is the bound, audited
attended override — an operator-authored file at `LS_DISPATCH_UNKNOWN_OVERRIDE`, bound to the
exact KST date + `LS_DISPATCH_RUN_ID` + the in-force snapshot identity, carrying a structured
first-party citation, and gated on a fresh nonce in an attended shell. **The agent never
authors or supplies it** — see `RUNBOOK-rung1.md` § 3a.

## §Post-close — Agent verification (read-only)

After the operator's session closes:

1. Run the post-session **catalog ingest** of today's KST date (prerequisite for the tracking twin).
2. Run the existing **tracking** pass.
3. Run the read-only report:
   ```sh
   cargo run --release -p nautilus-ls-lab --bin lab-live -- --rung-report
   ```
   It appends nothing (the chain + registry bytes are byte-identical before and after) and prints:
   the head hash it evaluated under, each trailing session's **clean / limit-event / head-mismatched**
   class, cumulative rung-1 P&L against **[−148k, +266k]**, N-progress toward the 5-session
   escalation, and the readiness verdict. A session under a different head is shown
   **head-mismatched**, never silently counted — if you see one, the wrong binary evaluated it.

## Operator-only (nonce-gated, attended, no-TTY refused)

`--genesis` (register the rung-1 chain) → `--dispatch` (pre-flight gate) → `--mount` (**run** the
attended session at 0.10× v35 size: consume → drive → fail-closed teardown → finalize; requires
`LS_MOUNT_KEEPALIVE` to name an existing operator keepalive file, whose mtime the operator refreshes
within the pre-registered heartbeat interval) → after 5 clean sessions, `--escalate`. `--reregister` (rung-0
requalification / epoch repair, bounded to ≤ the chain-earned rung) and `--clear-killswitch`
(re-arm after an auto-halt, with a scrubbed who/why) are the recovery paths. Full sequence +
watchdog/breaker discipline: [`RUNBOOK-rung1.md`](RUNBOOK-rung1.md).

## Head identity (KTD7) — how "clean" is keyed

Clean-session matching and escalation key the head params-hash on the **actual head governed
params** — the newest finalized run whose `strategy_code_hash` matches the running binary, pinned
to `strategy_version == LS_TURN_EXPECT_VERSION` (set `LS_TURN_EXPECT_VERSION=35`) so an older-version
same-code run in the data home can never revert the key — not the all-levers-off `default()`. So a real v35 session (sized from risk 299,340 / entry_confirm 1.0 / or_width_max_atr
0.666 / breakeven_trigger_r 0.41 / gap_retention 0.5) matches the head like-for-like and counts as
clean; a governed-param change flips the head and re-runs N. `--mount` sizes from that same head
source (and refuses a zero-size default head), and both `--head` and `--rung-report` print the head
they evaluated under so a stale-binary reading is self-evident.
