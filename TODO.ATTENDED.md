# TODO — attended (operator-only)

Items that need **you**: an open KRX window (Mon–Fri 09:00–15:30 KST; after-hours 16:00–18:00
where noted), live credentials, and/or a nonce-gated arm that refuses in a no-TTY shell. An
agent must never drive these.

Rewritten 2026-07-27, replacing `TODO.KRX.md` / `TODO.OFFLINE.md` / `TODO.0716.1.mv` /
`todo.2.txt` — all four had drifted into describing finished work. Offline counterpart:
[`TODO.OFFLINE.md`](TODO.OFFLINE.md).

---

## 1. Rung-1 attended paper session — *the live task; everything else is support*

- **Cost:** one full session · **Autonomy:** operator-only, start to finish
- **Blocked on:** [`TODO.OFFLINE.md`](TODO.OFFLINE.md) §1 (catalog current through the
  **previous** session). Do not open the window without it.

The production ladder is code-complete. PR #216 closed the last agent-runnable safety gap
(timed hard-stop on `node.run`); #217/#218/#219 closed the universe producer and its offline
coverage. Nothing is left to build — what remains is running it.

Follow [`adapters/nautilus/lab/RUNG1-PREFLIGHT.md`](adapters/nautilus/lab/RUNG1-PREFLIGHT.md),
which is current (it already carries head `e5bc2ae8…` and the §0.7 live-quote path). The
operator sequence lives in
[`adapters/nautilus/lab/RUNBOOK-rung1.md`](adapters/nautilus/lab/RUNBOOK-rung1.md).

Shape of the run:

1. §0.1–§0.6 preflight — build `lab-live` release, `--head` must print `e5bc2ae8…`, prereg is
   v2 with band `[−148000, +266000]`, `--rung-report` to inspect the chain head. **Agent-runnable
   — do it the night before**, so the window is spent trading, not checking.
2. §0.7 `lab-mount-universe` — must run **after 09:00 KST**. Before the opening auction, t8407
   answers with the *previous* session's snapshot whose `open` is a perfectly positive integer,
   so the producer refuses on the clock rather than silently resolving yesterday's opens.
3. `--genesis` → `--dispatch` → `--mount` (nonce-gated, attended, TTY).
4. Post-close: ingest → tracking → `--rung-report`.

**Exit codes are the contract — never read success from log text.** `--mount`: `0` ran and
finalized clean · `66` not-paper refusal · `71` precheck failed, dispatch NOT consumed · `72`
ran but finalized **ABNORMAL** · `77` no-TTY / no mountable dispatch. `72` is never success: the
kill switch is engaged and reds the next `--dispatch` until a nonce-gated `--clear-killswitch`.

**Two env vars fail quietly rather than loudly** (preflight §0.6): `LS_CALENDAR_SNAPSHOT` unset
reads as `enforced-fail-closed` and refuses every dispatch; `LS_DISPATCH_LANE_ENV` defaults to a
CWD-relative `.env.<lane>` with no upward search, so from `adapters/nautilus/lab` it misses the
repo-root `.env.domestic` and the gate reads `lane_env_present=false`. Check `--head`'s first
line for `action=enforced-active`.

## 2. Flatten the stranded 3-share 005930 paper buy

- **Cost:** ~1 min · **Autonomy:** manual cancel / `make paper-reset` at 09:00
- Non-marketable but resting, and it **blocks every order-probe scan** — it must clear before
  any order-placing re-probe.
- **Verify it is still resting first.** Last confirmed open 2026-07-14; the attended sessions
  since may already have cleared it. Carried forward unverified from the retired `TODO.KRX.md`.

## 3. *(optional, low)* #35 — t1481/t1482 after-hours ranking

- **Cost:** small · **Autonomy:** after-hours 16:00–18:00 KST
- Roadmap item, no dependency on anything above. Only if you specifically want after-hours
  ranking that day.

---

### Done — do NOT redo

- **#118 universe engine, first tier-stratified real run** — executed 2026-07-24, verdict
  **GREEN** (refusal-free ingest 40/40 symbols, pin `90005f88`, gate run `…-orb-v33`, 259 trades
  across 3 tiers). Issue closed 2026-07-27.
- Re-cert wave 3 attended live — 4 promotions (#131–#133/#135); t8412 → recommended (#138);
  IGW00000/CSPAT00701 modify promoted (#139/#146–#151); CSPAT00601 → recommended (#205/#206).
- KRX calendar genesis snapshot — executed 2026-07-23 against real KRX/KASI.
