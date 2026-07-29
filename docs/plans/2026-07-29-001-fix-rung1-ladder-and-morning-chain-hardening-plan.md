---
title: Rung-1 Ladder and Morning Chain Hardening - Plan
type: fix
date: 2026-07-29
topic: rung1-ladder-and-morning-chain-hardening
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

# Rung-1 Ladder and Morning Chain Hardening - Plan

## Goal Capsule

- **Objective:** Use the forfeited 2026-07-29 session to contain a leaked credential pair, close two ladder defects before the first live session, and pre-stage the 2026-07-30 morning chain — all without disturbing the state that chain depends on.
- **Product authority:** The operator (`sunkeunchoi`). The attended mount, the Unknown override, and any decision to promote the ladder to a live lane remain operator-only.
- **Open blockers:** None.
- **Execution profile:** Two trees. U1–U4 land in the main checkout and trigger no Rust build; U5–U6 land in a git worktree with its own `target/`. Deadline: U3 must be running by 17:00 KST 2026-07-29.
- **Product Contract preservation:** Product Contract unchanged. Planning resolved three of the four Deferred-to-Planning questions; the remaining one is recorded below.

---

## Product Contract

### Summary

Four workstreams for 2026-07-29: move the KRX/KASI credentials out of a stageable file, fix the zero-trade and stub-only-catalog defects in an isolated worktree, correct the 2026-07-30 operating prompt where it overstates what a missed opening range costs, and pre-stage tomorrow's 08:45–09:15 chain so it starts warm. Nothing lands in the operational tree before tomorrow's run.

### Problem Frame

Today's session was lost to the clock. Head v34 builds its opening range only from live bars observed between 09:00 and 09:15 KST, and the deadline was discovered after the window had already closed. Tomorrow repeats the same chain against the same clock, assembled live from six long environment-laden commands spread across two documents, gated on a KRX witness that publishes retrospectively somewhere between 07:51 and 08:51.

Two findings compound that pressure. `adapters/nautilus/lab/RUNBOOK-session-morning.md` carries a live KRX `AUTH_KEY` and a KASI service key in plaintext; the file is untracked but `git check-ignore` matches nothing, so a single `git add -A` stages both. The current defense is a written instruction not to stage it.

Separately, the ladder's evidence accounting has two holes. `is_clean_session` (`adapters/nautilus/lab/src/dispatch/ladder.rs:337-384`) checks lane, rung, head identity, limit events, safety trips, and rung-2+ reports, then returns `true` — with no trade-count floor. A finalized session that took zero trades passes every test. And `build_context` (`adapters/nautilus/lab/src/runner/live.rs:646-651`) derives the catalog-watermark verdict entirely from the `LS_DISPATCH_STUB_CATALOG` environment stub; the real catalog path is computed on the line above but used only for the lock. Unstubbed, the check is always `(false, false)` — red, deferrable, and deferred every session against a pre-registration that permits 3 deferrals in a 5-session window.

Tomorrow's mount is a paper rehearsal, which bounds the damage but leaves the defects live. `is_clean_session` requires the dispatch link's `trading_env` to equal `live` (`ladder.rs:348`), while `LS_TRADING_ENV=paper` with no `LS_DISPATCH_LANE` override resolves to `LanePosture::Paper` — the code's own comment at `live.rs:497` calls it "a paper pre-check (rung informational)". So a paper session cannot move the ladder from 0/5 and cannot burn a clean slot. The 2026-07-30 operating prompt states the opposite: it warns that a post-09:15 mount "burns 1 of the 5 rung-1 clean sessions", which would mislead the operator into treating a rehearsal miss as a costly one. Both defects still matter, but their deadline is the first live session, not 09:15 tomorrow.

### Key Decisions

- **Ladder fixes are written in an isolated git worktree and stay unmerged through tomorrow's session.** (session-settled: user-directed — chosen over editing in place: tomorrow's operating prompt pins the 16 debug binaries to `5f38144` and forbids a rebuild, so a `cargo test` in the operational tree would invalidate its premise. A worktree carries its own checkout and its own `target/`.)

- **Credentials are contained, not rotated.** (session-settled: user-approved — chosen over rotation: the keys exist on local disk in an untracked file and were never pushed, so making them unstageable removes the exposure path. Rotation stays available if the assumption about push history turns out to be wrong.)

- **Tomorrow's mount is a paper rehearsal, not a live rung-1 session.** (session-settled: user-directed — chosen over running the live lane: the ladder stays at 0/5 by design and no clean slot is at risk, so the rehearsal buys chain practice at zero evidence cost. Both ladder defects become latent rather than urgent — their deadline is the first live session, not 09:15 tomorrow.)

- **Pre-staging is sequenced ahead of the ladder work.** The witness watcher is bounded by wall clock — it must be polling before the evening publication window. The ladder work has no deadline beyond the end of the day.

### Requirements

**Credential containment**

- R1. The KRX and KASI credentials in `adapters/nautilus/lab/RUNBOOK-session-morning.md` are not stageable by an ordinary `git add -A` from the repo root.
- R2. The morning chain remains executable by someone following the runbook, with credentials supplied from a gitignored source rather than the procedure text.

**Ladder evidence integrity**

- R3. The 2026-07-30 operating prompt states what a post-09:15 mount actually costs on a paper lane, replacing its claim that the session burns 1 of the 5 rung-1 clean sessions.
- R4. A finalized session that took zero trades does not qualify as clean rung-1 evidence. This must hold before the first live-lane session; it is not required for tomorrow's rehearsal.
- R5. The `catalog_watermark` dispatch check derives its verdict from the real catalog state rather than requiring an environment stub, so a clean ingest stops consuming a deferral.

**Morning-chain readiness**

- R6. By 17:00 KST on 2026-07-29, an unattended watcher is polling for the KRX daily witness for 2026-07-29 and recording each attempt's timestamp and outcome to a file the morning session can read.
- R7. The 08:45–09:15 chain is executable from a single pre-staged entry point with absolute paths already resolved, rather than assembled from two documents under time pressure.
- R8. The chain reports and stops rather than continuing when it is no longer on pace to hand a universe to the operator by 09:10 KST.

**Isolation from tomorrow's run**

- R9. No work on 2026-07-29 writes to `adapters/nautilus/state/`, `data/turn4-fresh/catalog/`, `data/turn4-fresh/dispatch/`, or `adapters/nautilus/target/`.
- R10. No work on 2026-07-29 issues LS gateway traffic. KRX and KASI reads for R6 are permitted.

### Acceptance Examples

- AE1. Zero-trade session is not clean evidence
  - **Covers R4.**
  - **Given:** a finalized rung-1 run with zero limit events, no safety trip, matching head hashes, and zero closed trades.
  - **When:** the ladder evaluates it as escalation evidence.
  - **Then:** it does not count toward the 5 clean sessions, and the reason names the trade count.

- AE2. Catalog watermark passes without a stub
  - **Covers R5.**
  - **Given:** `LS_DISPATCH_STUB_CATALOG` is unset and the ingest checkpoint reads the previous session for every daily symbol with no gaps.
  - **When:** the dispatch gate evaluates `catalog_watermark`.
  - **Then:** the check is green without a deferral.

- AE3. Late chain stands down
  - **Covers R8.**
  - **Given:** the ingest is still running and its observed pace will not complete before 09:05 KST.
  - **When:** the pace check fires.
  - **Then:** the run reports a stand-down recommendation with the remaining minutes, and does not resolve a universe.

- AE4. Credentials survive a blanket stage
  - **Covers R1.**
  - **Given:** a clean checkout with the runbook present.
  - **When:** `git add -A` runs at the repo root.
  - **Then:** no file containing either credential is staged.

### Scope Boundaries

- No `--mount`, `--dispatch`, `--genesis`, or `--clear-killswitch` on 2026-07-29.
- No merge of the ladder fixes into the operational tree before tomorrow's session finishes. Whether they merge afterward is a separate decision.
- No ORB strategy or lever-queue work — offline and non-interfering, but it does nothing for tomorrow.
- No credential rotation, per the containment decision above.
- No change to the pre-registration. It is frozen and amendable only by an explicit re-registration dispatch.

### Dependencies / Assumptions

- The 16 debug binaries under `adapters/nautilus/target/debug/` were built from `5f38144` and the 299-test adapter suite passed. Tomorrow's run depends on this; today must not disturb it.
- Watermarks sit at `20260728` for all 75 daily symbols. Tomorrow's ingest advances them to `20260729`.
- The chain is at rung 1 with 0 of 5 clean sessions and no dispatch consumed. Genesis was registered 2026-07-29 and must not be repeated.
- IGW00201 is a warm cumulative budget. Gateway traffic today could slow tomorrow's ingest past its 09:05 pace target, which is why R10 excludes it.
- The credentials were never pushed to a remote. KTD3's containment-over-rotation choice depends on this. U1 verifies it with a history search; a positive hit invalidates the assumption and escalates to rotation, which is the operator's decision, not the implementer's.

### Outstanding Questions

**Deferred to Planning**

- What the ladder's intended path to its first live-lane session is, given that paper rehearsals never accrue clean evidence. Rung 1 needs 5 clean live sessions and has 0; the rehearsal does not shorten that. This is a product question for the operator, not a blocker on any unit below.

### Sources / Research

- `adapters/nautilus/lab/src/dispatch/ladder.rs:337-384` — `is_clean_session`; lane gate at :348, safety-trip check at :372, rung-2+ report gate at :378, unconditional `true` at :384.
- `adapters/nautilus/lab/src/dispatch/ladder.rs:175-177` — `is_live_lane`, the `trading_env == "live"` definition.
- `adapters/nautilus/lab/src/runner/live.rs:646-651` — `catalog_watermark` derived solely from `cfg.catalog_stub`.
- `adapters/nautilus/lab/src/runner/live.rs:493-506` — lane resolution; paper trading-env falls to `LanePosture::Paper`, commented as a rung-informational pre-check.
- `adapters/nautilus/lab/config/preregistration.json` — `k_window: 5`, `max_deferrals: 3`, rung-1 band `[-148000, 266000]`, and the `rung0_requalification` clause requiring paper sessions for re-entry.
- `adapters/nautilus/lab/RUNBOOK-session-morning.md` — the morning procedure, and the credential block at its end.
- `adapters/nautilus/lab/PROMPT-2026-07-30-session-morning.txt` — tomorrow's operating prompt, including the 09:15 opening-range deadline and the binary-freshness premise.
- `adapters/nautilus/lab/src/dispatch/ladder.rs:170-173` — `read_perf`, which already loads the per-run `PerformanceReport`; the seam U5 extends.
- `adapters/nautilus/lab/src/dispatch/readiness.rs:181` — `over(catalog.sum(|s| s.deferrals), thresholds.max_deferrals)`, confirming deferrals accumulate across the window against the pre-registered ceiling.
- `adapters/nautilus/lab/src/dispatch/checks.rs:184-187, 355-365` — the `watermark_fresh` / `bars_present` context fields and the three `catalog_watermark` outcomes they select.
- `adapters/nautilus/src/ingest/checkpoint.rs:163-181, 390-439, 821` — `Checkpoint` with its private `watermarks` map and public `gaps()` / `shifted_instruments()` accessors.
- `adapters/nautilus/scripts/turn3-ingest.sh`, `turn4-ingest.sh` — the existing operator-script convention U3 and U4 follow.
- Root `.gitignore` — `.env.*` with a `!.env.example` negation; the existing pattern U1 reuses.

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Ladder fixes are written in a git worktree; U1–U4 are not.** (session-settled: user-directed — chosen over a single tree: tomorrow's operating prompt pins the debug binaries to `5f38144` and forbids a rebuild, so any `cargo` invocation in the operational tree invalidates its premise. Instantiates the brainstorm's isolation decision, narrowed: the credential, prompt, watcher, and driver work touches no Rust and needs no isolation, so isolating it would only add friction.)

- KTD2. **The watcher and morning driver ship as shell scripts under `adapters/nautilus/scripts/`, not as new lab binaries.** A new binary would require `cargo build -p nautilus-ls-lab --bins`, which is exactly the rebuild KTD1 exists to avoid. The directory already holds `turn3-ingest.sh` and `turn4-ingest.sh`, so the convention exists.

- KTD3. **Credentials move to a gitignored `.env.calendar` at the repo root; the runbook keeps its procedure and becomes committable.** The root `.gitignore` already carries `.env.*`, so a new lane-style file inherits containment with no gitignore change and matches how `.env.domestic` is already handled. Ignoring the whole runbook would protect the keys but bury a procedure three other documents reference.

- KTD4. **The zero-trade floor lives inside `is_clean_session`, not at finalize time.** That function is the single gate every escalation path already routes through, and `read_perf` is already in scope there. A finalize-time marker would add a manifest field and leave older runs unclassifiable.

- KTD5. **`watermark_fresh` requires an empty gap set as well as current watermarks; `bars_present` stays a separate signal.** `checks.rs` already distinguishes the two, producing different red messages. A recorded coverage gap means the watermark is current but the coverage behind it is not trustworthy, which is a staleness fact, not an emptiness fact.

- KTD6. **The stub stays as a test-only override.** `LS_DISPATCH_STUB_CATALOG` is read by `dispatch_cli.rs:331` and is the only way tests drive the three outcomes deterministically. The real read becomes the default when the stub is unset, inverting today's behavior without breaking the existing suite.

### Assumptions

- `Checkpoint` exposes no public watermark accessor today. U6 adds one rather than duplicating the parse — the field is `BTreeMap<String, String>` keyed `<instrument>.<bar_type>`, matching the runbook's `endswith('1-DAY')` filter.
- The daily bar-type suffix used by the checkpoint keys is `1-DAY`, per the runbook's symbol-extraction snippet. U6 should confirm against `Checkpoint::key` rather than hardcoding on the runbook's word.
- `PerformanceReport` carries both `total_trades` and a `trades` vec. U5 gates on the closed-trade count; a missing performance artifact fails closed as not-clean.
- The operator supplies the two credential values when creating `.env.calendar`. The agent never reads them out of the runbook into any command or output.

### Sequencing

U1 → U2 → U3 → U4 in the main checkout, in that order, finishing U3 before 17:00 KST. Then U5 and U6 in the worktree, independent of each other and of everything above.

U1 is the true prerequisite: U3 sources credentials from the file U1 creates, and U4 sources the same file and reads U3's log. Beyond that chain the order is deadline-driven — U3 is the only unit with a wall-clock deadline, and U2 sits ahead of it only because it is a fifteen-minute edit. U5 and U6 depend on nothing and could run at any point; they sit last because they carry no deadline.

---

## Implementation Units

### U1. Move the calendar credentials into a gitignored env file

- **Goal:** Make the KRX and KASI keys unstageable while keeping the runbook usable and committable.
- **Requirements:** R1, R2. Cites KTD3.
- **Dependencies:** None.
- **Files:** `.env.calendar` (create, gitignored, operator-authored values), `adapters/nautilus/lab/RUNBOOK-session-morning.md` (modify), `adapters/nautilus/lab/TODO-2026-07-28-A-calendar-refresh-activate.md` (modify if it references the credential block).
- **Approach:** Strip the trailing credential block from the runbook and replace it with a one-line pointer to `.env.calendar` plus the `set -a; . .env.calendar; set +a` idiom the fetch steps need. Create `.env.calendar` mode `0600`, matching how the spend ledger and calendar snapshot are already installed. Do not echo either value into chat, a command, or a commit. Confirm `.env.calendar` matches the existing `.env.*` ignore rule rather than adding a new one.
- **Patterns to follow:** `.env.domestic` — same location, same lane-file shape, same ignore rule.
- **Test scenarios:**
  - Covers AE4. `git check-ignore -v .env.calendar` matches the `.env.*` rule and exits 0.
  - `git status --short` after the edit shows the runbook as a normal untracked or modified file with no credential content, verified by grepping the working tree for the key prefixes and finding only `.env.calendar`.
  - `git add -A --dry-run` at the repo root lists no path whose content carries either key.
  - `git log --all -S` over both key prefixes returns no commits, confirming the keys never entered history.
  - `.env.calendar` is mode `0600`.
- **Verification:** Both keys exist in exactly one gitignored, owner-only file; the runbook reads as a complete procedure without them.

### U2. Correct the clean-session claim in the 07-30 operating prompt

- **Goal:** Stop the prompt from telling tomorrow's operator that a late mount burns rung-1 evidence it cannot burn.
- **Requirements:** R3.
- **Dependencies:** None.
- **Files:** `adapters/nautilus/lab/PROMPT-2026-07-30-session-morning.txt` (modify).
- **Approach:** Rewrite the deadline section's cost claim. The 09:15 deadline itself stands — a late mount still takes zero trades and wastes the rehearsal — but the sentence asserting it consumes 1 of 5 clean sessions is wrong on a paper lane and should say so, naming `ladder.rs:348` so a future reader can check. Leave the stand-down guidance intact; it is still the right call, just for a cheaper reason.
- **Test expectation:** none — documentation correction with no behavioral surface.
- **Verification:** The prompt's stated cost of a missed window matches what `is_clean_session` actually does on a paper-lane run.

### U3. Overnight KRX witness watcher

- **Goal:** Have the answer to "did the 2026-07-29 witness publish?" already on disk when the morning session starts at 08:45.
- **Requirements:** R6, R10.
- **Dependencies:** U1 (reads credentials from `.env.calendar`).
- **Files:** `adapters/nautilus/scripts/krx-witness-watch.sh` (create), `adapters/nautilus/scripts/krx-witness-watch.log` (written at runtime; already ignored by that workspace's `*.log` rule, so it satisfies the gitignore requirement without touching any path R9 forbids).
- **Approach:** Poll the KRX daily endpoint hourly for `basDd=20260729`, appending one line per attempt — timestamp, HTTP status, row count — to the log above. Source credentials from `.env.calendar`; never hardcode them in the script. The `curl -H "AUTH_KEY: …"` header does expose the key in the process table, which is accepted on this single-user host and is the runbook's existing pattern. Row count above zero is the positive signal; a `200` with zero rows is a clean negative and must be logged as such rather than treated as failure. Exit conditions: stop after a positive result, or keep polling into the morning. KRX only — no LS gateway calls.
- **Execution note:** Verify the probe shape against the runbook's Step 1 before scheduling it; a wrong `basDd` silently logs clean negatives all night.
- **Patterns to follow:** `adapters/nautilus/scripts/turn4-ingest.sh` for env sourcing and logging shape; the runbook's Step 1 probe for the exact request and its row-count parse.
- **Test scenarios:**
  - A `200` response with an empty `OutBlock_1` logs a zero-row negative and continues polling, rather than exiting or logging an error.
  - A `200` with rows logs the count and stops polling.
  - A `401` logs distinctly from a zero-row result, so a bad key is not mistaken for an unpublished witness.
  - The log file path resolves under a gitignored directory.
- **Verification:** Started before 17:00 KST, the log accumulates one hourly line and the morning session can read publication state without issuing a probe of its own.

### U4. Pre-staged morning chain driver with a pace abort

- **Goal:** Reduce the 08:45–09:15 chain from six documents-assembled commands to one entry point that already knows every absolute path.
- **Requirements:** R7, R8.
- **Dependencies:** U1 (credentials), U3 (reads the watcher log for publication state).
- **Files:** `adapters/nautilus/scripts/session-morning.sh` (create).
- **Approach:** Sequence probe → fetch-inputs → refresh → activate → ingest → catalog status → mount universe, invoking the prebuilt binaries under `adapters/nautilus/target/debug/` directly rather than through `cargo run`. Resolve every path absolutely from a single repo-root variable. Between the ingest and universe steps, compare elapsed time and watermark progress against the 09:05 ingest-completion target and stop with a stand-down report when the pace will not make it. The two clocks are distinct: 09:05 is when the ingest must be done, 09:10 is when the resolved universe must be in the operator's hands, and the 09:15 opening range is what both exist to protect. The script must not run `--mount`, `--dispatch`, or `--genesis`, and must not author the Unknown override's operator, reason, or citation fields.
- **Execution note:** This is orchestration over existing binaries; prefer a dry-run pass that prints its resolved command sequence over unit coverage.
- **Patterns to follow:** `adapters/nautilus/scripts/turn4-ingest.sh` for the env-block shape; the runbook Steps 1–6 for the command sequence and each step's success signal.
- **Test scenarios:**
  - Covers AE3. With the pace check fed a simulated late clock, the script reports stand-down with remaining minutes and exits before the universe step.
  - A dry-run mode prints the full resolved command sequence with absolute paths and issues no gateway traffic.
  - An `APPEND REFUSED` from the ingest stops the run and reports, rather than retrying or rolling a watermark back.
  - The script refuses to run if `LS_TRADING_ENV` is anything other than `paper`.
- **Verification:** A dry run reproduces the runbook's sequence exactly, with no path left relative and no mount-class command present.

### U5. Trade-count floor in `is_clean_session`

- **Goal:** Stop a finalized zero-trade session from qualifying as clean rung evidence.
- **Requirements:** R4. Cites KTD4.
- **Dependencies:** None (worktree).
- **Files:** `adapters/nautilus/lab/src/dispatch/ladder.rs` (modify, including its `#[cfg(test)]` module).
- **Approach:** Add a closed-trade check to `is_clean_session` before its final `true`, sourced from the `PerformanceReport` that `read_perf` already loads. A missing performance artifact fails closed. The existing blocked-escalation message should name the trade shortfall the way it already names the clean-session shortfall, so an operator reading `--rung-report` sees why a session did not count.
- **Execution note:** Write the failing zero-trade test first — the function has six existing gates and a floor added without a test that isolates it can pass for the wrong reason.
- **Patterns to follow:** The existing gates at `ladder.rs:348-383` for early-return shape; `read_perf` at `:170` for the artifact read; the inline test at `:728` for fixture construction.
- **Test scenarios:**
  - Covers AE1. A run passing every existing gate but carrying zero closed trades is not clean, and the blocked reason names the trade count.
  - A run with one closed trade and no limit events remains clean — the floor does not regress the happy path.
  - A run whose performance artifact is missing or unparseable is not clean.
  - `verify_escalation` over a mix of qualifying and zero-trade runs counts only the qualifying ones and reports the correct shortfall.
- **Verification:** `cargo test --workspace` green in the worktree, with the new cases failing against the pre-change function.

### U6. Real catalog-watermark evaluation

- **Goal:** Let a clean ingest satisfy `catalog_watermark` on its own, so the check stops consuming one of three permitted deferrals every session.
- **Requirements:** R5. Cites KTD5, KTD6.
- **Dependencies:** None (worktree).
- **Files:** `adapters/nautilus/lab/src/runner/live.rs` (modify), `adapters/nautilus/src/ingest/checkpoint.rs` (modify — add a watermark accessor), `adapters/nautilus/lab/tests/dispatch_checks.rs` (modify), `adapters/nautilus/lab/tests/dispatch_cli.rs` (modify).
- **Approach:** In `build_context`, read the ingest checkpoint from the already-computed catalog path when `LS_DISPATCH_STUB_CATALOG` is unset. Derive `watermark_fresh` from whether every daily watermark reads the last closed trading session with an empty gap set, and `bars_present` from whether the catalog holds bars at all. Resolve "last closed trading session" from the calendar snapshot already reaching `build_context` through its `date_fact` argument — do not infer it from the clock, which would read a holiday or weekend as a stale watermark. Keep the stub branch ahead of the real read so existing tests keep their deterministic control. Add a public watermark accessor to `Checkpoint` rather than re-parsing the file in the lab crate.
- **Execution note:** Confirm the daily bar-type key suffix against `Checkpoint::key` before filtering on it; the runbook's `1-DAY` string is documentation, not the source of truth.
- **Patterns to follow:** `Checkpoint::gaps()` at `checkpoint.rs:821` for accessor shape; `backtest.rs:953` for how the lab crate already loads a checkpoint; the three outcomes at `checks.rs:355-365` for what each boolean must mean.
- **Test scenarios:**
  - Covers AE2. With the stub unset and a checkpoint whose daily watermarks all read the previous session with no gaps, `catalog_watermark` is green and no deferral is applied.
  - A checkpoint with a stale watermark on one symbol yields the stale red, not the bars-missing red.
  - A checkpoint with current watermarks but a recorded coverage gap yields the stale red — coverage the gap set contradicts is not fresh.
  - An empty or absent catalog yields the bars-missing red.
  - Every existing `LS_DISPATCH_STUB_CATALOG` value (`ok`, `empty`, `stale`) still selects its outcome unchanged.
- **Verification:** `cargo test --workspace` green in the worktree, with the pre-existing stub-driven dispatch tests untouched.

---

## Verification Contract

| Gate | Where | Applies to | Done signal |
|---|---|---|---|
| `git check-ignore -v .env.calendar` | main checkout | U1 | Exits 0 naming the `.env.*` rule |
| Working-tree credential grep | main checkout | U1 | Key prefixes appear only in `.env.calendar` |
| Watcher log inspection | main checkout | U3 | One line per hourly attempt, started before 17:00 KST |
| `session-morning.sh` dry run | main checkout | U4 | Full sequence printed, absolute paths, zero gateway calls |
| `cargo test --workspace` | worktree only | U5, U6 | Every `test result:` line ends `0 failed` |

`cargo test` runs **only** in the worktree. Running it in the main checkout would rewrite `adapters/nautilus/target/debug/` and break the binary premise tomorrow's run depends on.

No live smoke, no `make adapter-check` from the operational tree, and no gateway traffic of any kind today beyond U3's KRX polling.

---

## Definition of Done

- Both credentials live in exactly one gitignored file, and a blanket `git add -A` at the repo root stages neither (R1, R2).
- The 07-30 operating prompt's stated cost of a missed opening range matches what the ladder actually does on a paper lane (R3).
- The witness watcher is running and logging before 17:00 KST 2026-07-29 (R6).
- `session-morning.sh` dry-runs the full chain with resolved absolute paths and stands down on a simulated late clock (R7, R8).
- U5 and U6 are committed to a branch in the worktree with `cargo test --workspace` green, and that branch is not merged (R9).
- `adapters/nautilus/target/debug/` is byte-unchanged from its `5f38144` build, and `adapters/nautilus/state/`, `data/turn4-fresh/catalog/`, and `data/turn4-fresh/dispatch/` are untouched (R9, R10).
