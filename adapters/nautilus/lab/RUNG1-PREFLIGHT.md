# Rung-1 Preflight — agent-runnable vs operator-only

The contract for a rung-1 attended session against head **v34**. It splits what an **agent** may
run autonomously (offline, no-TTY, read-only or non-mutating) from what only an **operator** may
run (nonce-gated, attended, refused in a no-TTY shell). Run the operator sequence from
[`RUNBOOK-rung1.md`](RUNBOOK-rung1.md); the frozen numbers live in
[`config/preregistration.json`](config/preregistration.json) (rationale
[`config/PREREGISTRATION.md`](config/PREREGISTRATION.md)).

> **The agent NEVER drives `--genesis`, `--dispatch`, `--mount`, `--escalate`, `--reregister`, or
> `--clear-killswitch`.** Those are nonce-gated, attended, and refuse loudly in a no-TTY shell
> (distinct exit codes — never look-like-ran). Build from `adapters/nautilus`; `make` breaks in
> spawned shells, so call `cargo` directly.

## §0 — Agent preflight (offline, no gateway)

1. **Build `lab-live` from v34.** From `adapters/nautilus` (the CWD trap: from the repo root the
   lab crate is skipped):
   ```sh
   cd adapters/nautilus
   cargo build --release -p nautilus-ls-lab --bin lab-live
   ```
2. **Confirm the binary embeds v34** (`--head`, read-only, no nonce):
   ```sh
   cargo run --release -p nautilus-ls-lab --bin lab-live -- --head
   ```
   The binary embeds v34 **iff** the printed `strategy_code_hash` equals the documented head
   `d7a9820b…`. That hash is the **sole** discriminator — the binary carries no hash→version map,
   so `--head` prints no version, and the `governed_params_hash(default)` line is a
   **version-invariant constant** (identical across v9…v34), explicitly **not** a v34 confirmation.
3. **Confirm the pre-registration is v2.** `config/preregistration.json` has `"version": 2` and the
   rung-1 band `[-148000, +266000]`; its SHA-256 is the citation each dispatch records. The
   derivation is reproduced by `cargo test -p nautilus-ls-lab --test prereg_derivation`.
4. **Dry-read the exit-code contract** (do not infer success from log text):

   | command | exit | meaning |
   |---|---|---|
   | `--dispatch` | 0 / 1 / 75 | green (proceed) / refused / throttled (re-run, never terminal) |
   | `--mount` | 66 / 77 / 70 | not-paper refusal / no-TTY (or no mountable dispatch) refusal / **prepared** (node built; live driver deferred) |
   | `--escalate` / `--reregister` / `--clear-killswitch` | 78 / 79 / 80 | that arm's loud refusal in a no-TTY shell |
   | `--head` / `--rung-report` | 0 | read-only diagnostics (no nonce, no chain append) |

5. **Inspect the chain head before the operator starts** (read-only): run `--rung-report` and read
   the printed head hash. A pre-existing chain under a **non-v34** head is a **stop-and-reconcile**
   (archive / epoch-repair or a fresh data home) — never a silent proceed.

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

`--genesis` (register the rung-1 chain) → `--dispatch` (pre-flight gate) → `--mount` (prepare the
session at 0.10× v34 size) → after 5 clean sessions, `--escalate`. `--reregister` (rung-0
requalification / epoch repair, bounded to ≤ the chain-earned rung) and `--clear-killswitch`
(re-arm after an auto-halt, with a scrubbed who/why) are the recovery paths. Full sequence +
watchdog/breaker discipline: [`RUNBOOK-rung1.md`](RUNBOOK-rung1.md).

## Head identity (KTD7) — how "clean" is keyed

Clean-session matching and escalation key the head params-hash on the **actual head governed
params** (the data home's latest finalized run — the v34 backtest), not the all-levers-off
`default()`. So a real v34 session (sized from risk 299,340 / entry_confirm 1.0 / or_width_max_atr
0.666 / breakeven_trigger_r 0.41 / gap_retention 0.5) matches the head like-for-like and counts as
clean; a governed-param change flips the head and re-runs N. `--mount` sizes from that same head
source (and refuses a zero-size default head), and both `--head` and `--rung-report` print the head
they evaluated under so a stale-binary reading is self-evident.
