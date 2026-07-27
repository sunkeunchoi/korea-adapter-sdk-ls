# TODO — offline (agent-runnable)

No open-market window, no nonce, no TTY. §1 does hit the gateway, but as a historical
market-data read that is not window-bound; everything below it is fully deterministic.

Rewritten 2026-07-27, replacing `TODO.KRX.md` / the old `TODO.OFFLINE.md` / `TODO.0716.1.mv` /
`todo.2.txt` — all four had drifted into describing finished work. Attended counterpart:
[`TODO.ATTENDED.md`](TODO.ATTENDED.md).

---

## 1. Ingest the catalog forward through the previous session — *critical path, morning-of*

- **Cost:** small · **Autonomy:** agent-runnable *after* an operator calendar refresh
- **Blocks:** [`TODO.ATTENDED.md`](TODO.ATTENDED.md) §1. Nothing else matters until this is done.

As of 2026-07-27 23:07 KST the newest daily bar in `data/turn4-fresh/catalog` ends **2026-07-24
(Friday)** — all 75 daily series uniform at `2026-05-18..2026-07-24`, zero warnings. That is
**already as current as the calendar permits**; the Monday 2026-07-27 session is not ingestible.

**This is not a night-before task.** Being past the 16:30 KST close buffer sets the ingest's
*target* date but grants no permission to fetch it. `CalendarGate::range_action` independently
requires the target to be a **proven** `trading_session`, and a session is proven only by a
retrospective KRX witness — so today's row reads `unknown` for its entire duration and the
accumulate run skips every triple (exit `0`, `0 bars`, zero gateway calls, checkpoint
byte-identical). Verified 2026-07-27; see
[`docs/solutions/workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md`](docs/solutions/workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md).

The morning-of chain:

1. **Operator** — `calendar-refresh` (needs `LS_KRX_APPKEY` + `LS_KASI_SERVICE_KEY`, owner-local
   and absent from the lane env files). Once KRX publishes the previous day's record its row
   flips `unknown` → `trading_session`.
2. **Agent** — re-run the bounded accumulate ingest (recipe in the solution doc above): mode
   `accumulate`, `LS_INGEST_SYMBOLS` = the catalog's existing shcodes, `LS_INGEST_SKIP_UNIVERSE_LOAD=1`.
   Never unbounded — that re-composes the head's tradable universe.
3. **Agent, after 09:00 KST** — `lab-mount-universe` for the session date. It needs
   `LS_MOUNT_UNIVERSE_METADATA` (the head is metadata-driven, pin `90005f88…`) and, when the date
   is today, `LS_DISPATCH_LANE_ENV`. `RUNG1-PREFLIGHT.md` §0.7 omits the metadata var; the
   producer fails closed without it.

**If step 1 cannot happen before the window, the session is still runnable** — eligibility is
staleness (`<= MAX_PRIOR_STALENESS_DAYS = 10`), and a Tuesday session with Friday on disk is 4
days. But `prior_close` / `prior_atr` would then be Friday's, making the head's overnight-gap term
a multi-session return. That is a fidelity call to make deliberately, not to discover at 09:05.

**Self-pace it.** `IGW00201` is a *cumulative*, warm-sensitive budget, not a pure rate limit — a
page-burst trips it even under the per-second cap. Put `t8430` **last**. Write the pin only
after a refusal-free ingest.

## 2. ~~Close the two issues whose work already landed~~ — DONE 2026-07-27

Both closed with their outcomes: **#118** GREEN (pin `90005f88`, gate run `…-orb-v33`, 259
trades, 3 tiers); **#119** documented **NO-BUILD** (PR #204) — 4 signals passed collinearity, the
kill was materiality, stop location is CLASS-B-absorbed.

## 3. Reconcile eight unmerged branches / four worktrees

- **Cost:** small · **Autonomy:** offline decision, then agent-runnable cleanup

Re-measured 2026-07-27 after #221/#222 — the previous table listed **five**; there are **eight**,
and three were missing entirely. `local-only` means never pushed, so the worktree is the only copy.

| branch | ahead | where | disposition |
|---|---|---|---|
| `fix/ingest-krx-calendar-proof` | 8 | remote + worktree | shipped via #192 — confirm, then prune |
| `feat/strategy-loop-turn-4-widen-param-flip` | 5 | remote | **was missing from this table**; turn 4 was FALSIFIED — confirm nothing unlanded, then prune |
| `feat/nautilus-reingest-overlap-write-hardening` | 3 | remote | **was missing**; overlaps the shipped #101/#105 write-hardening — confirm, then prune |
| `docs/paper-live-smoke-evidence-cleanup` | 2 | **local-only** | never had a PR; land or discard |
| `feat/paper-reset-utility` | 1 | remote | **was missing**; relates to `TODO.ATTENDED.md` §2 (`make paper-reset`) — check before discarding |
| `prototype/gap-retention-cohort` | 1 | remote + worktree | prototype; superseded by the #169 KEEP (v32)? |
| `research/krx-calendar-forward-closures-api` | 1 | **local-only** + worktree | research note; fold into `docs/research/` or drop |
| `research/krx-calendar-historical-api` | 1 | **local-only** + worktree | same |

Four live worktrees under `.worktrees/` each carry a full `target/`, so this is disk as well as
clarity. Decide per branch; **do not bulk-delete** — three branches were never pushed, so for
those the local worktree is the only copy of the work.

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
