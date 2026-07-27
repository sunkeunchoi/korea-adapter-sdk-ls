# TODO — offline (agent-runnable)

No open-market window, no nonce, no TTY. §1 does hit the gateway, but as a historical
market-data read that is not window-bound; everything below it is fully deterministic.

Rewritten 2026-07-27, replacing `TODO.KRX.md` / the old `TODO.OFFLINE.md` / `TODO.0716.1.mv` /
`todo.2.txt` — all four had drifted into describing finished work. Attended counterpart:
[`TODO.ATTENDED.md`](TODO.ATTENDED.md).

---

## 1. Ingest the catalog forward through the previous session — *critical path*

- **Cost:** small · **Autonomy:** agent-runnable, but self-paced gateway traffic
- **Blocks:** [`TODO.ATTENDED.md`](TODO.ATTENDED.md) §1. Nothing else matters until this is done.

As of 2026-07-27 the newest daily bar in `data/turn4-fresh/catalog` ends **2026-07-24 (Friday)**.
The Monday 2026-07-27 session is **not ingested**.

`RUNG1-PREFLIGHT.md` §0.7 requires the catalog current through the **previous** session: the
mount-universe producer needs a prior daily bar per symbol for `prior_close` / `prior_atr`, and
refuses a prior older than `MAX_PRIOR_STALENESS_DAYS = 10`. A Tuesday session therefore needs
Monday on disk. Discovering this at 09:05 costs the attended window.

**Self-pace it.** `IGW00201` is a *cumulative*, warm-sensitive budget, not a pure rate limit — a
page-burst trips it even under the per-second cap. Put `t8430` **last**. Write the pin only
after a refusal-free ingest.

## 2. Close the two issues whose work already landed

- **Cost:** ~2 min · **Autonomy:** agent-runnable
- **#118** — universe engine first real run. Executed 2026-07-24, verdict GREEN. Close with the
  outcome (pin `90005f88`, gate run `…-orb-v33`, 259 trades, 3 tiers).
- **#119** — Turn 11 stop-width-geometry lever. Merged as a documented **NO-BUILD** (PR #204);
  4 signals passed collinearity, the kill was materiality — stop location is CLASS-B-absorbed.
  Close with that verdict, not silently.

Leaving them open is what let `todo.2.txt` keep recommending Lever 8 days after it had run.

## 3. Reconcile five unmerged branches / four worktrees

- **Cost:** small · **Autonomy:** offline decision, then agent-runnable cleanup

| branch | ahead of `main` | disposition |
|---|---|---|
| `docs/paper-live-smoke-evidence-cleanup` | 2 | never had a PR; land or discard |
| `fix/ingest-krx-calendar-proof` | 8 | shipped via #192 — confirm, then prune |
| `prototype/gap-retention-cohort` | 1 | prototype; superseded by the #169 KEEP (v32)? |
| `research/krx-calendar-forward-closures-api` | 1 | research note; fold into `docs/research/` or drop |
| `research/krx-calendar-historical-api` | 1 | same |

Four live worktrees under `.worktrees/` hold the last four. Each carries a full `target/`, so
this is disk as well as clarity. Decide per branch; do not bulk-delete — one of them is the only
copy of unlanded work.

## 4. ORB strategy loop — converged; do not queue another micro-lever

- **Autonomy:** n/a — this is a standing note, not a task

The incremental lever queue is **spent**, and the recent verdicts say so plainly: ratio-ATR tilt
KEEP (v30) → equity-compounding REVERT → gap-retention KEEP (v32) → failed-break reversal STOP
(2026-07-22) → stop-geometry NO-BUILD (#119) → profit-target 0.75 STOP → slot-ranking STOP. Both
the entry-filter and CLASS-B sizing axes are closed, and the stop/exit-geometry axis falsified in
two independent directions.

`todo.2.txt` recommended Lever 8 (failed-break reversal) as "the highest-value move left" — it
ran 2026-07-22 and STOPped at Phase A (73.6% stop-out, `ror_shift −0.063`). That axis is gone too.

The honest next move is **not another lever**. It is either a data/breadth turn (wider universe,
or a new instrument domain) or stepping to another track. Anything else is fitting noise — and
the head is v34 (`e5bc2ae8…`), which the ladder is now pinned to, so a KEEP would force a rung-1
re-baseline mid-climb.

## 5. *(defer)* #34 / #36 roadmap design waves

- **Cost:** large / vague · **Autonomy:** `/ce-brainstorm` (design-heavy)
- #36 realtime lifecycle / AFR for t1860; #34 sFileData sourcing for t1852/t1856. Both were
  marked defer in the retired queue and nothing since has raised their priority.

---

### Done — do NOT redo

- **Ratio-ATR CLASS B sizing lever** — the retired `TODO.OFFLINE.md` §1 called this "the one
  live strategy-loop direction left". It was built and **KEPT** as head v30 (RoR 0.1262, PR
  #153), and the head has moved on twice since (v32 → v34).
- **KRX trading-calendar / weekday-holiday gap** (retired §2, issues #120/#36/#102) — shipped:
  the shared offline calendar leaf crate (#185/PR #190), the zero-gateway-call proof (#192), and
  the enforced cutover (#189). A production snapshot is installed and owner-local.
- **Handoffs #154 / #155** (`TODO.0716.1.mv`) — the governed strategy-turn command (PR #155) and
  the frozen production-ladder pre-registration (PR #158) both shipped; the prereg has since been
  re-frozen to v2 against v34.
