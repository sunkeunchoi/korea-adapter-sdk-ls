---
title: Production Ladder — Rung-1 Readiness (v34) — Plan
type: feat
date: 2026-07-24
topic: production-ladder-rung1
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
execution: code
---

# Production Ladder — Rung-1 Readiness (v34) — Plan

## Goal Capsule

- **Objective:** Make the FIRST live rung of the [[Production ladder]] actually runnable against the current head **v34**, and give an agent the read-only tooling to guard each attended session and verify its aftermath. This is the bridge between the shipped ladder machinery (PR #158) and an operator running rung-1 sessions.
- **What this plan fixes (two distinct problems, not co-equal):** (1) **The RUN-blocker** — the attended mount and ladder operator commands are **library-only, unwired to any `lab-live` subcommand**, so no rung-1 session can launch today. Wire them, and resolve v34's real governed params + universe into the mount (a default-params mount would trade zero-size). (2) **An escalation-correctness fix** — the frozen pre-registration is v30-derived; its economic band is quantified-invalid for v34 and binds at the rung-1→2 **escalation** gate (not at launch), so re-freeze it to v2 from the v34 backtest before genesis so escalation judges v34 against v34.
- **Execution home:** Entirely the lab crate of the standalone `adapters/nautilus/` workspace. Every unit is provable offline under `make adapter-check`; `node.run` is never driven in the commit gate — the mount's end-to-end path is proven only by an operator-attended paper session (outside this plan's Definition of Done).
- **Agent vs operator boundary:** An agent builds and lands all five units, re-freezes the pre-registration, runs the head-identity preflight, and runs the read-only post-session verification. The agent **never** drives `--genesis`, `--dispatch`, or `--mount` autonomously — those are nonce-gated, attended, and refused in a no-TTY shell.
- **Stop conditions:** Surface instead of guessing if wiring the mount CLI would re-order the teardown sequence (`stop → cancel → flat-check → halt`, halt LAST), change order-dispatch runtime semantics beyond the U6/U10 seams, or if a check reads opposite to reality (e.g. head-check green with a non-v34 binary). Paper only (`LS_TRADING_ENV=paper`); the live-lane flip is a separate later step.

---

## Problem Frame

The Production Ladder was built, pre-registered, and FROZEN in PR #158 against head **v30**. The lever queue has since STOPped three turns running (profit-target-075, failed-break reversal, slot-ranking #211), and the certified real-universe head is now **v34** (`20260724T014752Z-backtest-orb-v34`, RoR 0.0398, 119 trades; pin `LS_TURN_EXPECT_VERSION=34`). Advancing to production is the natural next move — but the code shows "ready to RUN" is untrue as stated: the mount CLI does not exist (the launch blocker), and the frozen economics are v30's (an escalation-gate correctness fix, not a launch blocker). Rung-1 is also the deliberate small-dose probe of whether v34's *real-universe* edge is live-worthy at all — v34's RoR (0.0398) and v30's (0.1248) are **not directly comparable** (different universes), so "production" here means "validate the real-universe head at 0.10× exposure," not "ship a proven edge." The two code facts:

1. **The pre-registration numbers are v30 economics.** `config/preregistration.json` (v1) and `PREREGISTRATION.md` freeze every band against v30's `turn4-fresh` closed-trade distribution. Reproducing that derivation confirms the file's values exactly, then applying the identical **Protective** formula to v34's distribution shows the frozen rung-1 economic band is wrong for v34:

   | statistic (full size) | v30 (frozen source) | v34 (current head) |
   |---|---|---|
   | per-session P&L mean / median | +236,343 / +260,000 | +49,292 / +35,000 |
   | worst single session | −1,200,450 | **−1,360,330** |
   | rolling-5 cum P&L worst / best | −689,900 / +3,555,800 | **−1,483,240 / +1,772,900** |
   | cumulative / RoR | +5,435,900 / 0.1248 | +1,183,010 / 0.0398 |
   | **rung-1 economic band @0.10× (protective)** | **[−69,000, +533,000]** | **[−148,000, +266,000]** |

   The frozen floor (−69k) is **~2× too tight** for v34: a normal-variance rung-1 five-session streak (down to −148k at 0.10×) breaches it. **Scope of the effect:** `expectation_band` is read *only* by `verify_escalation` (`adapters/nautilus/lab/src/dispatch/ladder.rs:363`), which fires at the rung-1→2 escalation decision *after* five clean sessions — it does **not** block launching or running rung-1 sessions, nor per-session clean/limit classification (that is `is_clean_session`). So the wrong floor produces false "bleeding-edge" escalation-blocks at the session-5 gate; rung-1 sessions 1–4 run identically under either band. The one launch-critical value U1 supplies is `rung_fraction(1) = 0.10`, unchanged v30→v34. The re-freeze still matters — trading v34 against v30's escalation band measures the new head against the old head's economics, and (a symmetric effect the plan must own) v34's ceiling *halves* (+533k → +266k), so a strongly-profitable rung-1 streak (cum > 266k) now also blocks escalation as "outside band." Note the band is judged against **live, slippage-laden** P&L while derived from a **zero-slippage** backtest — a conservative (floor-biased) gate documented in `PREREGISTRATION.md` v2.

2. **v34 is a code-hash change AND the mount CLI does not exist.** v34's `strategy_code_hash` is `d7a9820b…` vs v30's `6ae7b9f1…` — a `code_change_resets_to_rung_1` event under the frozen `head_change` rules. Two consequences:
   - Nothing at `--genesis`/`--dispatch` asserts the built `lab-live` binary embeds v34; `is_clean_session`/`verify_escalation` silently compare each session's manifest hashes against whatever code the *running binary* embeds (`ladder.rs:277`). A wrong-binary genesis is a silent head mismatch. **And the params half of that fingerprint is version-invariant:** the head params_hash is `governed_params_hash(&OrbParams::default())`, but `OrbParams::default()` (`params.rs:349`) sets every governed lever to its OFF sentinel — "byte-identical to v30" — so it encodes *none* of v34's governed values (risk 299,340, entry_confirm 1.0, or_width_max_atr 0.666, breakeven_trigger_r 0.41, gap_retention 0.5) and is identical across v9…v34. Two consequences the plan must resolve, not gloss: (i) `--head` cannot claim to "confirm v34" via `governed_params_hash(default)` — only `strategy_code_hash()` distinguishes heads; (ii) `run_mount` must build the strategy from v34's **real** governed params (its backtest manifest / the governed-param source), not `OrbParams::default()`, or the live session trades zero-size — and if a real-params manifest is then compared against `governed_params_hash(default)` at `ladder.rs:277`, *every* real v34 session mis-keys as non-clean and escalation can never fire. Reconciling the head-identity comparison for governed heads is a **stop-and-surface** seam, handled in U2/KTD7.
   - `authorize_mount`/`build_live_session_node` (U6) and `run_escalation`/`run_reregistration`/`clear_kill_switch` (U10) are **called only from `tests/`**. `lab-live` wires only `--dispatch` and `--genesis`; the bare invocation still bails with *"the mounted LiveNode session lands in U6 — see README"* (`src/runner/live.rs:882-889`). The sizing seam exists (`orb.rs::with_rung_fraction`, `src/strategy/orb.rs:1124`, multiplies the budget numerator at `:1222`, byte-identical to v30 at fraction 1.0) but nothing resolves the pre-registered 0.10 fraction into it. **A rung-1 live session cannot be launched today.**

This plan closes both: re-freeze the pre-registration to v34, and wire the operator/agent CLI surface so the shipped machinery is reachable — then hand a per-session runbook + preflight + read-only verification to the operator.

## Requirements

- **R1 — Re-freeze the pre-registration to v34 (v2).** Recompute the four economic expectation bands from the v34 backtest via the documented Protective formula, bump `version` to 2, cite the v30→v34 code-hash change as the re-registration reason, and land a checked-in derivation test that reproduces every frozen band from v34's rolling-5 statistics. Confirm the watchdog/breaker values against v34 or record why they stand.
- **R2 — Head-identity preflight.** An offline `lab-live --head` diagnostic prints the running binary's `strategy_code_hash()` and its `governed_params_hash(&OrbParams::default())`, framed so the operator/agent confirms the binary embeds v34 by **hash-equality of `strategy_code_hash()` against the documented `d7a9820b…`** — the sole head discriminator. The binary carries no hash→version map, so `--head` does not (and cannot) self-report "version 34"; and the printed params_hash is a version-invariant constant that does **not** confirm v34's governed values (see Problem Frame 2). Non-mutating, no nonce.
- **R3 — Wire the attended mount.** `lab-live --mount` drives `authorize_mount → build_live_session_node → node.run → run_teardown → finalize`, resolving the chain-authorized rung → `prereg.rung_fraction(rung)` → `OrbStrategy::with_rung_fraction`, and sourcing the live session's **universe (`Vec<SelectedSymbol>`), v34 governed `OrbParams`, and `DecisionSink`** (not `OrbParams::default()` — see KTD7), threading the dispatch↔run linkage into the manifest (KTD3). It **hard-refuses unless `LS_TRADING_ENV=paper`** (mechanized as a distinct-exit loud refusal, not just a stated stop condition). Nonce-gated, attended-only, no-TTY loud refusal; `node.run` stays live-only.
- **R4 — Wire the ladder operator commands.** `lab-live --escalate`, `--reregister`, and `--clear-killswitch` reach `run_escalation`/`run_reregistration`/`clear_kill_switch`, each nonce-gated with a distinct exit code. `--reregister` and `--clear-killswitch` each capture and scrub an operator reason (`LS_DISPATCH_REASON`) — clearing an auto-halt kill-switch is the CLI's most safety-sensitive mutation and must leave an audited who/why record. `--reregister` may only requalify to rung 0 or repair the current epoch — it must **not** authorize a rung above the chain-earned rung (upward jumps bypass the earned-escalation gate); an out-of-bound rung is a stop-and-refuse.
- **R5 — Read-only post-session verification.** `lab-live --rung-report` reports, without mutating the chain or registry: each trailing session's clean/[[Limit event]] classification, cumulative rung-1 P&L against the v34 band, N-progress toward escalation, and the readiness verdict — agent-runnable offline over a data home.
- **R6 — Operator runbook + agent preflight.** A `RUNG1-PREFLIGHT.md` that splits agent-runnable from operator-only steps (head-check, v2-prereg precondition, exit-code dry read, the post-close ingest → tracking → `--rung-report` flow), and an updated `RUNBOOK-rung1.md` reflecting the v34 band, the head-change note, and the new commands. CONCEPTS.md touched only if a new term is introduced.

Success criteria: an operator can run the RUNBOOK end-to-end and actually mount a rung-1 session at 0.10× v34 size; every rung-1 economic judgment reads against v34's distribution; an agent can verify a completed session's cleanliness and escalation-readiness without touching the chain; `make adapter-check` stays green throughout.

## Scope Boundaries

- **In scope:** the pre-registration v2 re-freeze + derivation test; the `--head`, `--mount`, `--escalate`, `--reregister`, `--clear-killswitch`, `--rung-report` CLI wiring over existing library functions; the rung-fraction → sizing resolution at mount; the preflight/runbook docs.
- **Deferred to Follow-Up Work:** the rung-2 tracking-error band and per-rung breaker re-derivation (both scheduled from LIVE rung-1 data per the amendment protocol — cannot be grounded now); any change to the checks, chain, watchdog, or ladder *logic* (this plan only wires and re-parameterizes what shipped).
- **Out of scope (operational acts this plan enables, not done criteria):** running the first rung-1 live session; the genesis/dispatch/mount attended commands themselves; the live-lane credential flip. `node.run` is never driven in the commit gate.
- **Handled elsewhere:** the KRX calendar (landed — genesis snapshot LIVE 2026-07-23); SC-primary fill-lane certification.

---

## Planning Contract

### Key Technical Decisions

- **KTD1 — Re-freeze, don't hand-edit; reproduce the derivation in a test.** The v34 bands are authored into `preregistration.json` v2 and mirrored in `PREREGISTRATION.md`, and a checked-in test recomputes each band from v34's rolling-5 worst/best via the Protective formula (`floor = worst_roll5 × f`, `ceil = best_roll5 × 1.5 × f`, round to nearest 1,000) and asserts equality with the frozen file. This keeps the repo's "no invented numbers" discipline: the bands are derived and auditable, not typed in. Target values (to be reproduced by the test):

  | rung | fraction | economic band (v34) |
  |---|---|---|
  | 1 | 0.10 | **[−148,000, +266,000]** |
  | 2 | 0.25 | [−371,000, +665,000] |
  | 3 | 0.50 | [−742,000, +1,330,000] |
  | 4 | 1.00 | [−1,483,000, +2,659,000] |

- **KTD2 — This is a legitimate re-registration, not band-fitting.** The economic band is defined as backtest-derived (R14(e) in the ladder plan), the head genuinely changed (documented, code-hash differs), and `head_change.code_change_resets_to_rung_1 = true` anticipates exactly this. Re-deriving from v34's *backtest* (still zero-slippage, same Protective method) is the same discipline v30's original freeze used — it is NOT fitting to live data. Pre-genesis, with no chain yet in the target data home, the re-freeze is simply authoring the initial frozen values for the v34 epoch; the genesis dispatch cites the v2 file hash. (If a v30-era chain already exists in the target data home, that is a stop-and-reconcile — see Risks.)
- **KTD3 — The breaker stands for rung 1; the schedule is unchanged.** `session_max_loss_krw = 300,000` is ~2.2× v34's worst rung-1 session (−1,360,330 × 0.10 = −136,033) — still real protection. Keep it, and keep the existing note to re-register a larger breaker before rungs 3–4 (same scheduled idiom as the tracking bands). Do not silently shrink it.
- **KTD4 — CLI wiring is thin argv over existing library functions; zero new logic.** Each new subcommand is an argv arm in `dispatch_main` (`research.rs`/existing `--dispatch`/`--genesis` shape) that gathers config from env and calls the already-tested library function, mapping the outcome to an `ExitCode`. `scrub::install()` stays the first statement; nonce/attendance gating is reused verbatim (`OperatorGate`); `node.run` and teardown remain the mount caller's live-only responsibility. No check, chain, watchdog, or ladder logic is added or re-ordered — if wiring appears to require logic changes, stop and surface it.
- **KTD5 — The mount resolves the fraction; sizing threading is the one real code seam.** `--mount` reads the chain's authorized (effective) rung, calls `prereg.rung_fraction(rung)` (fail-closed if absent), sets `MountConfig.rung_fraction`, and threads it into the strategy via `OrbStrategy::with_rung_fraction` at node build — replacing U6's hardcoded `1.0` metadata stub. This is the only place new behavior lands; it is offline-testable up to node construction (the `live_wiring.rs` precedent), and a rung change must produce zero manifest param diff (head identity stable, KTD6 of the ladder plan).
- **KTD6 — Verification is read-only and head-pinned.** `--rung-report` reuses `is_clean_session`, the readiness reducer, and a read-only view of `verify_escalation` (evidence + band check) without appending any record. Because those functions compare against the *running binary's* head, the report is only valid when run with the v34 binary — the report prints the head hash it evaluated under so a stale-binary reading is self-evident (the same discipline as R2's `--head`).
- **KTD7 — The mount sources v34's real governed params, and the head-identity comparison must accept them.** `run_mount` builds the strategy from v34's actual governed `OrbParams` (its backtest manifest is the source of truth: risk 299,340 / entry_confirm 1.0 / or_width_max_atr 0.666 / breakeven_trigger_r 0.41 / gap_retention 0.5), **never** `OrbParams::default()` (which is all-levers-OFF, v30-identical, zero-size). This collides with the shipped ladder: `is_clean_session`/`run_escalation` key the head params_hash on `governed_params_hash(&OrbParams::default())` (`ladder.rs`), so a real-params v34 manifest would mis-key as non-clean and escalation could never fire. This is a **stop-and-surface** decision, not a silent wiring choice — resolve it one of two ways and record which: **(a)** make the head params_hash key on the *actual* head governed params (the mount's source), so clean-session matching compares like-for-like; or **(b)** document that head identity rests on `strategy_code_hash()` alone and drop the params-hash leg of the clean-session predicate. Option (a) is preferred (it preserves the params-change→re-run-N rule). Whichever is chosen, `--head` and `--rung-report` print the head they evaluated under (KTD6). If resolving this requires changing shipped ladder *logic* beyond a params source, stop and surface per the ladder plan's stop conditions.

### High-Level Technical Design

The rung-1 session lifecycle after this plan — who does what, and where the agent/operator boundary sits:

```mermaid
sequenceDiagram
  participant A as Agent (offline, no-TTY)
  participant O as Operator (attended, nonce)
  participant L as lab-live
  participant C as dispatch chain + prereg v2
  A->>L: --head (verify binary embeds v34 d7a9820b)
  A->>A: RUNG1-PREFLIGHT §0 (env, flat, watermark, v2 present)
  O->>L: --genesis (nonce)  ->> C: rung-1 genesis record
  O->>L: --dispatch (nonce) ->> C: exit 0 green / 1 refused / 75 throttled
  O->>L: --mount (nonce): authorize -> resolve rung_fraction 0.10 -> node.run -> teardown -> finalize
  L-->>C: consumption marker + safety-trip records (if any)
  Note over O,L: attended + watchdog 90s; limit event -> rung-0 suspend
  A->>A: post-close catalog ingest of today's KST date
  A->>L: (tracking pass, existing) + --rung-report (read-only)
  L-->>A: clean/limit-event status, cum P&L vs [-148k,+266k], N-progress, readiness verdict
```

Exit-code contract carried verbatim from the shipped gate (`dispatch_main`): **0** = green or all reds deferred → proceed; **1** = refused (fix or defer a *deferrable*) ; **75** = throttled (IGW00201, wait + re-run, never terminal). Nonce-gated commands additionally emit a distinct loud refusal in a no-TTY shell — never look-like-ran.

### Implementation Constraints

- All work is in `adapters/nautilus/` (own Cargo.toml, Rust 1.96); the root gate cannot see it — `make adapter-check` (= `cd adapters/nautilus && cargo test --workspace`) is the primary gate. No `crates/` files are expected to change, so the root gate is not expected to run. Never run two adapter `cargo test`/build invocations concurrently (target-lock); a SIGKILL'd cargo → `rm -rf target/debug/incremental`.
- `make` breaks in spawned shells — call cargo directly (`cargo test -p nautilus-ls-lab`, `cargo run --release -p nautilus-ls-lab --bin lab-live -- …`). Build the `lab-live` bin from `adapters/nautilus` (CWD trap: from the root the lab crate is skipped).
- `LS_DATA_HOME`/`LS_DISPATCH_PREREG` are ABSOLUTE paths. macOS case-insensitive FS: env-var/path comparisons stay case-exact.
- `node.run` is never driven offline (documented invariant). Offline tests for `--mount` stop at node construction and drive the consumption/finalize seams directly (`live_wiring.rs` precedent).
- Scrub discipline: `scrub::install()` first in every entry point; operator free-text (re-registration reason) is scrubbed before it lands; no secret appears in any record, report, or output line.

### Sequencing

U1 (re-freeze) is independent and lands first — it unblocks the fraction resolution and the report band. U2 (mount) and U3 (ladder/diagnostic CLI) both depend on U1; U2 carries the one real code seam (fraction → sizing). U4 (report) depends on U1 + the records U2/U3 produce. U5 (docs) lands last, referencing the real commands. Natural PR boundaries: {U1}, {U2, U3}, {U4}, {U5} — or one PR if kept tight.

---

## Implementation Units

| U-ID | Title | Key files | Depends on |
|---|---|---|---|
| U1 | Re-freeze pre-registration to v34 (v2) + derivation test | `lab/config/preregistration.json`, `lab/config/PREREGISTRATION.md`, `lab/tests/prereg_derivation.rs` | — |
| U2 | `--mount` CLI + rung-fraction sizing resolution | `lab/src/runner/live.rs`, `lab/src/strategy/orb.rs`, `lab/tests/live_wiring.rs` | U1 |
| U3 | Ladder + diagnostic operator CLI (`--head`/`--escalate`/`--reregister`/`--clear-killswitch`) | `lab/src/runner/live.rs`, `lab/tests/ladder.rs` | U1 |
| U4 | `--rung-report` read-only post-session verification | `lab/src/runner/live.rs`, `lab/src/dispatch/readiness.rs`, `lab/src/dispatch/ladder.rs`, `lab/tests/rung_report.rs` | U1, U2, U3 |
| U5 | Rung-1 preflight + runbook + CONCEPTS | `lab/RUNG1-PREFLIGHT.md`, `lab/RUNBOOK-rung1.md`, `lab/README.md`, `CONCEPTS.md` | U1–U4 |

All paths relative to `adapters/nautilus/` unless they start with `crates/` or are repo-root docs.

**Identifier namespace:** this plan's own identifiers are **U1–U5**, **R1–R6**, and **KTD1–KTD7**. References to the shipped ladder plan's machinery use *its* identifiers — `U6`–`U10`, `R13`/`R14(x)`/`R15`, `KD1`–`KD6`, `AE1`–`AE5`, and its own KTDs (always written "… of the ladder plan") — and point at `docs/plans/2026-07-16-001-feat-production-ladder-plan.md` (PR #158), not this document. They name the machinery this plan wires and re-parameterizes.

### U1. Re-freeze pre-registration to v34 (v2) + derivation test

- **Goal:** Replace the v30-derived economic bands with v34-derived ones, recorded as an explicit re-registration, with the derivation reproduced by a test.
- **Requirements:** R1; KTD1, KTD2, KTD3.
- **Dependencies:** none.
- **Files:** `lab/config/preregistration.json` (version → 2; rung `expectation_band`s → v34 values; `_note` updated to cite v34 source + code-hash reason; breaker/heartbeat/K unchanged), `lab/config/PREREGISTRATION.md` (new "Re-registration v2 (head v34)" section: v34 backtest source of truth `20260724T014752Z-backtest-orb-v34` **with its session count next to the rolling-5 constants** so the tail-window estimator's fragility is auditable, the per-session/rolling-5 table, the recomputed band table, and notes covering: the halved ceiling (+266k, so a strongly-profitable rung-1 streak also blocks escalation), the zero-slippage-vs-live-P&L conservative-gate caveat, and the breaker's explicit target-headroom rationale), `lab/tests/prereg_derivation.rs` (new).
- **Approach:** Author the four v34 bands from the Protective formula (KTD1). Keep `session_max_loss_krw = 300,000` (KTD3) with the existing rung-3 re-derivation note. The derivation test asserts, for each rung, that `round_to_1000(worst_roll5 × f)` and `round_to_1000(best_roll5 × 1.5 × f)` equal the frozen file's `min_cum_pnl`/`max_cum_pnl`, using v34's rolling-5 constants (−1,483,240 / +1,772,900) as the documented inputs; and that `prereg::load` reads the v2 file with `version == 2` and a stable content hash. Do NOT recompute from the live run dir in-test (keep the test hermetic — the constants are the audited derivation inputs, cross-checked against `PREREGISTRATION.md`).
- **Patterns to follow:** the v1 `PREREGISTRATION.md` derivation tables; `prereg.rs` load + content-hash tests.
- **Test scenarios:**
  - Each rung's frozen band equals the Protective-formula output from the v34 rolling-5 constants (four assertions).
  - `prereg::load(v2)` returns `version == 2`, `rung_fraction(1) == 0.10`, `expectation_band(1) == [-148000, +266000]`, `session_max_loss_krw == 300000`.
  - Editing the file changes the content hash (citation integrity — mirror the existing `content_hash_tracks_the_exact_file_bytes` test intent).
  - Sanity: rung-1 floor is strictly below the v30 floor (−148,000 < −69,000), documenting the loosening the re-freeze introduces.
- **Verification:** `cargo test -p nautilus-ls-lab prereg` green; `PREREGISTRATION.md` v2 numbers match the JSON byte-for-byte.

### U2. `--mount` CLI + rung-fraction sizing resolution

- **Goal:** The operator command that actually launches a rung-1 session, sizing at the pre-registered fraction.
- **Requirements:** R3; KTD4, KTD5, KTD7.
- **Dependencies:** U1 (fraction is `prereg.rung_fraction(rung)`).
- **Files:** `lab/src/runner/live.rs` (new `Some("--mount")` arm in `dispatch_main`; a `run_mount` entry gathering `MountConfig` from env, resolving the session inputs, and driving `authorize_mount → build_live_session_node → node.run → run_teardown → finalize`), `lab/tests/live_wiring.rs` (extend). `build_live_session_node` already applies `.with_rung_fraction`, so `orb.rs` needs no edit — U2's real work is (i) resolving `prereg.rung_fraction(rung)` into `MountConfig`, (ii) sourcing the session inputs (below), and (iii) the paper interlock.
- **Approach:** Mirror `--dispatch`'s env-gather → library-call → `ExitCode` shape. **Hard-refuse unless `LS_TRADING_ENV=paper`** (distinct-exit loud refusal, mechanized before any authorization — the bare-invocation arm's paper check must be preserved, not dropped, on the new `--mount` path). Gather data home, lane hash, trading env, nonce, now-unix. **Source the live session inputs** `build_live_session_node` requires: the universe (`Vec<SelectedSymbol>`), v34's governed `OrbParams` (KTD7 — the v34 backtest manifest, never `OrbParams::default()`), and the `DecisionSink` — name each source explicitly (universe resolution reuses the dispatch lane's daily/t8407 read or an explicit universe-file env var; distinct from the offline test path that passes symbols directly). Load prereg (fail-closed if the authorized rung's fraction is absent, this plan's KTD5); `authorize_mount` returns the authorization + held Live lock (its nonce/consumed/TOCTOU refusals are already tested); build the LiveNode via `build_live_session_node` with the sourced params + `.with_rung_fraction(fraction)`; on the live path only, `node.run` then `run_teardown` then finalize with the `DispatchLink`. `scrub::install()` first; loud distinct-exit refusal without a fresh nonce / in a no-TTY shell. `node.run` is live-only — offline tests stop at construction and drive consumption/finalize.
- **Execution note:** Resolve KTD7 (v34 governed-param source + the `ladder.rs` head-identity comparison) first and prove the zero-param-diff invariant for the *fraction* (a rung change leaves the manifest params byte-identical) before wiring the argv arm — the param source and fraction threading are the only behavior changes, so pin them test-first.
- **Test scenarios:**
  - Offline: a green-dispatch fixture + prereg v2 → node builds with `rung_fraction = 0.10` threaded into the strategy; the finalized manifest carries `dispatch_id`, `rung = 1`, `rung_fraction = 0.10`, lane hash, trading env.
  - Fraction resolution: authorized rung 1 → `0.10`; a prereg missing the rung-1 fraction → mount refuses (fail-closed), never defaults to 1.0.
  - Zero-param-diff: the manifest's governed-params hash at fraction 0.10 equals the fraction-1.0 hash (rung fraction is numerator-only, never a param — reuse/extend `rung_fraction_scales_the_risk_budget_numerator_with_zero_param_diff`).
  - Refusals inherited from `authorize_mount` still fire through the CLI: no/stale nonce, no-TTY marker, already-consumed dispatch, Live lock held by another process → distinct-exit loud refusal, no mount, no consume.
  - Paper interlock: `LS_TRADING_ENV` unset or ≠ `paper` → `--mount` hard-refuses with a distinct exit code *before* any authorization or lock acquisition; no mount, no consume.
  - Governed params (KTD7): the built strategy carries v34's governed `OrbParams` (risk 299,340, entry_confirm 1.0, …), **not** `OrbParams::default()` (which would size to zero); the finalized manifest's governed-params hash matches the v34 head, and — per the KTD7 resolution — `is_clean_session` counts that session as clean rather than mis-keying it against `default()`.
  - `node.run` is not invoked in any offline test (invariant assertion / construction-only stop).
- **Verification:** `cargo test -p nautilus-ls-lab` green; the bare `lab-live` "lands in U6" bail is gone; first end-to-end proof is an operator-attended paper session (outside the gate).

### U3. Ladder + diagnostic operator CLI

- **Goal:** Reach the remaining shipped-but-unwired operator functions, plus a head-identity diagnostic for preflight. Scope note: `--head` and `--clear-killswitch` are reachable *during* rung-1 (preflight and after an auto-halt); `--escalate`/`--reregister` fire only at the rung-1→2 boundary / after a suspension — wired now (user-directed) so the loop is complete and the shipped library isn't left with dead, untested-at-CLI entry points, not because rung-1 exercises them.
- **Requirements:** R2, R4; KTD4.
- **Dependencies:** U1 (escalation reads the prereg band/N).
- **Files:** `lab/src/runner/live.rs` (new argv arms: `--head`, `--escalate`, `--reregister`, `--clear-killswitch`), `lab/tests/ladder.rs` + CLI-level tests (extend).
- **Approach:** Thin argv arms over existing functions: `--head` (no nonce) prints `strategy_code_hash()` and `governed_params_hash(&OrbParams::default())` as verbatim structured fact lines, framed as a code-hash-equality check against `d7a9820b…` (no version readout — the binary has no hash→version map; the params line is a version-invariant constant, not a v34 confirmation, KTD7); `--escalate` → `run_escalation` (nonce; prints the `EscalationCheck::Ready` evidence or the `Blocked` reason, AE5 of the ladder plan); `--reregister` → `run_reregistration` (nonce; scrubbed `LS_DISPATCH_REASON`) — bounded to rung-0 requalification or current-epoch repair, refusing any `set_rung` above the chain-earned rung (an upward jump would bypass the earned-escalation gate); `--clear-killswitch` → `clear_kill_switch` (`src/runner/live.rs:187`, nonce + attendance) with a scrubbed `LS_DISPATCH_REASON` captured and recorded — re-arming trading after an auto-halt must leave an audited who/why. Each maps to an `ExitCode`; each nonce-gated arm refuses loudly with a distinct code in a no-TTY shell. No check/chain/ladder *logic* is added — only the reregister bound and the clear-reason capture, which are guard rails over the existing calls; if either needs new library behavior, stop and surface.
- **Test scenarios:**
  - `--head` prints a line equal to `strategy_code_hash()` (matching `d7a9820b…` for a v34 binary), prints the params-hash as a labeled version-invariant constant, and does not append to the chain (read-only, no nonce needed).
  - `--escalate` with N−1 clean sessions → blocked, output names the missing evidence (AE5 of the ladder plan, through the CLI); with N clean + cum P&L inside the v34 band → appends an escalation record.
  - `--escalate` where cum P&L sits below the v34 floor → blocked citing the band (exercises the re-frozen number end-to-end).
  - `--reregister` with a planted secret in the reason → scrubbed before the record lands; without a nonce → loud refusal, nothing appended.
  - `--reregister` with `set_rung` above the chain-earned rung → refused (no upward jump past the escalation gate); rung-0 / current-epoch targets proceed.
  - `--clear-killswitch` without a fresh nonce / in a no-TTY env → refusal with a distinct exit code; with a nonce → the persisted trip is cleared, the next dispatch reads the kill switch disengaged, and a scrubbed operator reason is recorded (a planted secret in the reason never lands).
- **Verification:** `cargo test -p nautilus-ls-lab` green; every new arm has a bin-level (`CARGO_BIN_EXE_lab-live`) exit-code test plus a library-level assertion.

### U4. `--rung-report` read-only post-session verification

- **Goal:** The agent's after-session read: is this session clean, where does cumulative rung-1 P&L sit against the v34 band, how many clean sessions accumulated, what's the readiness verdict — with zero mutation.
- **Requirements:** R5; KTD6.
- **Dependencies:** U1 (band), U2/U3 (the records + sessions to read).
- **Files:** `lab/src/runner/live.rs` (new `--rung-report` arm), `lab/src/dispatch/readiness.rs` (reuse the reducer), `lab/src/dispatch/ladder.rs` (a read-only escalation-readiness view — evidence + band check without append), `lab/tests/rung_report.rs` (new).
- **Approach:** Load the chain + registry + report sidecar read-only; for each trailing live-lane session classify clean vs limit-event via `is_clean_session` and the limit-event scan (read-only, no de-escalation append); sum cumulative realized P&L across the clean rung-1 sessions and compare against `expectation_band(1)` from prereg v2; count N-progress toward the rung's N; run the readiness reducer for the green/red verdict. Print the head hash evaluated under (KTD6) so a stale-binary reading is self-evident. Refuse nothing beyond a malformed data home; append nothing.
- **Execution note:** Assert the read-only invariant explicitly — the chain file bytes and registry are byte-identical before and after `--rung-report`.
- **Test scenarios:**
  - Fixture data home with 3 clean rung-1 sessions → report shows 3/N, cumulative P&L, and its position inside/outside [−148k, +266k].
  - A session carrying a limit event (safety-trip record / `.tmp-` residue / non-flat close) → classified limit-event, excluded from the clean count.
  - Cumulative P&L below the v34 floor → report flags "outside band (bleeding edge)"; above the v30 floor but below the v34 floor is treated per the v34 band, not v30.
  - Backtest/research runs interleaved → excluded from the trailing live-lane window.
  - Read-only: chain + registry bytes unchanged after the report; no record appended.
  - The report prints the evaluated head hash; a fixture manifest under a different head is shown as head-mismatched, not silently counted.
  - A planted secret never appears in any report byte (scrub test).
- **Verification:** deterministic fixture tests; read-only invariant asserted; `cargo test -p nautilus-ls-lab rung_report` green.

### U5. Rung-1 preflight + runbook + CONCEPTS

- **Goal:** The one-page operator/agent contract for a rung-1 session, aligned to the real commands and the v34 numbers.
- **Requirements:** R6.
- **Dependencies:** U1–U4 (documents the real commands + band).
- **Files:** `lab/RUNG1-PREFLIGHT.md` (new), `lab/RUNBOOK-rung1.md` (update), `lab/README.md` (retire the "mount lands in U6" note), `CONCEPTS.md` (only if a new term is introduced — likely none; the ladder terms already exist).
- **Approach:** `RUNG1-PREFLIGHT.md` splits **agent-runnable** (build `lab-live` from v34 in `adapters/nautilus`; `--head` confirms `d7a9820b…`; confirm `preregistration.json` is v2 and its hash; dry-read the exit-code contract; the post-close `ingest → tracking → --rung-report` sequence) from **operator-only** (`--genesis`, `--dispatch`, `--mount`, `--escalate`, `--reregister`, `--clear-killswitch` — all nonce-gated, attended, no-TTY-refused). Update `RUNBOOK-rung1.md`: the rung-1 band reference `[−69k, +533k]` → **`[−148k, +266k]`**, add a "head v34 — re-registered v2" note and the `code_change_resets_to_rung_1` reminder, and replace the manual-recipe pointer with `--mount`. Keep the teardown order and stop conditions verbatim.
- **Test scenarios:** `Test expectation: none — docs only.` (Correctness is enforced by U1–U4's tests and `make docs-check` is not applicable to lab-local runbooks.)
- **Verification:** the runbook's commands match the wired argv arms; the band number matches `preregistration.json` v2; a reviewer can follow preflight → genesis → dispatch → mount → post-session verification without a gap.

---

## Verification Contract

| Gate | Command | Applies to |
|---|---|---|
| Adapter workspace (primary) | `make adapter-check` (= `cd adapters/nautilus && cargo test --workspace`) | every unit |
| Targeted iteration | `cd adapters/nautilus && cargo test -p nautilus-ls-lab` | U1–U4 during development |
| Bin-level CLI | `CARGO_BIN_EXE_lab-live` exit-code tests | U2, U3, U4 |
| Root workspace gate | `cargo test` + `cargo test -p ls-core` | not expected — no `crates/` change planned; run only if one is touched |

Rules carried from repo conventions: two full root `cargo test` runs never overlap (target-lock); a SIGKILL'd cargo → `rm -rf target/debug/incremental`; `make` breaks in spawned shells (call cargo directly); build `lab-live` from `adapters/nautilus` (CWD trap). The commit gate never drives `node.run` — phase-2 live behavior is proven only by an operator-attended paper session through `--mount` (`LS_TRADING_ENV=paper`), an operational act outside this plan's Definition of Done. Fail-closed arms unreachable in happy-path fixtures are force-executed per convention.

---

## Risks & Mitigations

- **A v30-era dispatch chain already exists in the target data home.** Genesis refuses to re-genesis a valid chain; a v30 chain would bind rung state to v30's head. **Mitigation:** the preflight's first operator step inspects the chain (via `--rung-report`, which prints the head evaluated under); a pre-existing chain under a non-v34 head is a **stop-and-reconcile** (archive/epoch-repair or a fresh data home), never a silent proceed. Surfaced, not guessed.
- **Wrong binary at genesis/report.** Nothing structurally forces the running `lab-live` to embed v34. **Mitigation:** R2/`--head` is a mandatory preflight step and U4's report prints the head hash it evaluated under; a mismatch is self-evident. (A future hardening — an `EXPECT_VERSION`-style pin on the dispatch path — is noted as follow-up, not built here.)
- **The re-freeze loosens the floor — is that masking a worse edge?** v34's RoR (0.0398) is genuinely lower than v30's; the wider band reflects v34's real distribution, and the floor still fires on a *worse-than-ever* 5-session streak. The economic band is a runaway/bleeding-edge guard, not the edge itself; rung-1 is the calibration rung and its LIVE data (not this backtest band) sets the rung-2 tracking band. Documented in `PREREGISTRATION.md` v2.
- **The escalation band is zero-slippage but judged against live P&L.** `verify_escalation` sums *live* realized P&L against a band derived from v34's *zero-slippage* backtest, so live rung-1 cum P&L will sit systematically below the band once fills/fees/slippage are real — biasing the rung-1→2 gate toward false floor breaches. **Mitigation:** `PREREGISTRATION.md` v2 records the band as a deliberately conservative (floor-biased) gate and flags that the rung-2 decision may need a live-cost allowance — the same live-grounding the tracking bands already get. Not a launch risk (the band binds only at session-5 escalation).
- **Frozen thresholds rest on thin/optimistic inputs.** The floor is `worst_roll5 × f` — a single worst rolling-5 window over ~24 v34 sessions (a high-variance tail order-statistic), and the 300k breaker is a 2.2× cushion over a *zero-slippage* worst rung-1 session. **Mitigation:** `PREREGISTRATION.md` v2 records the v34 session count beside the rolling-5 constants and states the breaker's target headroom explicitly, so the fragility is auditable rather than implied; both are re-derived from LIVE data on the scheduled cadence (breaker before rung 3).
- **CLI wiring drifts into logic changes.** KTD4 forbids it; KTD7 is the one seam that may legitimately require a shipped-ladder change (the head-identity comparison), and it is an explicit stop-and-surface. **Mitigation:** if any arm cannot be a thin call over the existing library function without re-ordering teardown or changing a check/chain/ladder behavior beyond KTD7's named seam, stop and surface — do not extend the shipped logic under cover of "wiring."

---

## Definition of Done

- All five units landed; `make adapter-check` green; tree never committed red; no `crates/` change (root gate not required).
- `preregistration.json` is v2 with v34 economic bands, and `lab/tests/prereg_derivation.rs` reproduces every band from the documented v34 statistics; `PREREGISTRATION.md` v2 section matches the JSON.
- `lab-live` wires `--head`, `--mount`, `--escalate`, `--reregister`, `--clear-killswitch`, and `--rung-report`; the bare-invocation "lands in U6" bail is gone; every nonce-gated arm refuses loudly with a distinct exit code in a no-TTY shell.
- `--head` confirms the head by `strategy_code_hash()` equality against `d7a9820b…` (no version readout; the params-hash line is labeled version-invariant, KTD7).
- `--mount` builds the strategy from v34's **real** governed params (not `OrbParams::default()`), threads the pre-registered rung fraction into sizing with zero manifest param diff, hard-refuses unless `LS_TRADING_ENV=paper`, and offline tests never drive `node.run`; KTD7's head-identity comparison is resolved (option (a) or (b)) and recorded, so real v34 sessions count as clean.
- `--reregister` refuses a `set_rung` above the chain-earned rung; `--clear-killswitch` records a scrubbed operator reason.
- `--rung-report` is provably read-only (chain + registry bytes unchanged) and prints the head hash it evaluated under; classifications and the band comparison use v34's numbers.
- `RUNG1-PREFLIGHT.md` and the updated `RUNBOOK-rung1.md` reflect the real commands and the v34 band; the agent/operator boundary is explicit; no secret appears in any record, report, or output line (scrub tests pass).
- **Not in scope for done:** running the first rung-1 live session; the genesis/dispatch/mount attended commands; freezing the rung-2 tracking band or a larger breaker; the live-lane credential flip — operational acts this plan enables.
