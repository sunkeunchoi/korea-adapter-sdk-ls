---
title: Domestic KRX-Open Reconfirmation & Close-Out - Plan
type: chore
date: 2026-07-02
topic: domestic-krx-open-reconfirm-closeout
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

# Domestic KRX-Open Reconfirmation & Close-Out - Plan

## Goal Capsule

- **Objective:** Spend the open domestic KRX window capturing fresh live evidence that the domestic Tracked-not-Implemented residue is still gated, then record that disposition so the next wave stops re-probing a spent pool. Zero net-new flips is the successful outcome.
- **Product authority:** Operator (holds domestic credentials; runs the live raw-probes). Autonomous agent authors the ledger record and runs the gate.
- **Open blockers:** None blocking planning. Execution needs the operator to run `make raw-probe` against the two current-probeable candidates (`t0441`, `CSPBQ00200`); the ledger record consumes those results. `t1109` is not probed (after-hours-gated; cite §19/§20).

## Product Contract

### Summary

Run an operator-driven live `raw-probe` pass over the two current-probeable domestic candidates (`t0441`, `CSPBQ00200`), then write a new PROVISIONALITY-LEDGER section (§23) that re-dispositions the **full 16-TR domestic residue** as a complete close-out — fresh current-dated evidence for the 2 probed, cited §19–§22 evidence for the other 14 (including `t1109`, whose after-hours gate cannot be witnessed in a regular session). No TR flips; counts stay frozen; the gate stays green.

### Problem Frame

The request that opened this brainstorm — "flip more TRs now that KRX is open" — rests on a premise the ledger has already retired for domestic reads. The raw pool is exhausted (all 143 untracked read codes dispositioned, §18), and the 38 Tracked-not-Implemented residue is dominated by terminal or structurally-held TRs. Of the 38, exactly **16 are domestic-and-not-`paper_incompatible`** (38 − 13 `paper_incompatible` − 7 overseas-order − 2 overseas watchlist = 16); the other 22 are overseas or terminal and out of reach this session. The last three waves (§19 in-window, §20 closed-window, §22 exhaustion close-out) each concluded the movable read pool is empty. Overseas is closed this session, which removes the CIDBT order chain and the o3107/o3127 watchlist reads from reach.

None of the 16 is unlocked by an open *regular* session: `t0441` needs an F/O position (declined this wave — no order placement), `CSPBQ00200` needs a margin deposit, and `t1109` needs the *after-hours* session (15:30–17:50 KST) — so probing it in regular hours only re-derives §19/§20 and adds no fresh evidence. The two that are current-probeable (`t0441`, `CSPBQ00200`) are account-state reads, session-independent — the probes capture current-dated evidence, not window-dependent evidence. The value of acting now is not flips; it is capturing dated evidence that the gates still hold, so the disposition is defensible and the next window isn't spent re-deriving it.

### Key Decisions

- **No position manufacture this wave.** `t0441` will not be flipped — the operator declined placing real paper F/O orders (marketable buy → witness → close). It is probed as a read against the current flat account and recorded as position-gated, not certified.
- **Live evidence over documentation-only.** For the 2 current-probeable candidates, the reconfirmation captures fresh `raw-probe` results this session (http / rsp_cd / body_len) rather than citing prior proofs, so §23 stands on current-dated evidence. These are session-independent account reads, so the probes need no open window — the open session is opportunistic, not required.
- **0 flips is success.** The deliverable is the disposition record, not a count change. `reference.len`, `banner_trs`, and `maintained_tr_count` all stay unchanged.
- **Credential-safe probing only.** Use `make raw-probe` (prints only http/rsp_cd/body_len), never a smoke that would echo account data into the record.

### Requirements

**Live probe pass**

- R1. The operator runs `make raw-probe` against the two current-probeable candidates — `t0441` (선물/옵션잔고평가, `domestic_option` lane) and `CSPBQ00200` (증거금률별주문가능수량, `domestic` lane) — capturing http status, `rsp_cd`, and `body_len` for each. Both are session-independent account reads; the open session is opportunistic, not a prerequisite. `t1109` is NOT live-probed this wave — its after-hours gate cannot be witnessed in a regular session, so it is dispositioned by citing §19/§20 (see R4).
- R2. Each probe result is classified against its known gate: `t0441` → position-gated (flat account, empty/all-default balance), `CSPBQ00200` → funding-gated (cash-only account, margin rejection). A result that contradicts its expected gate (unexpected populated data) is flagged as a re-open candidate rather than filed as reconfirmation.
- R3. No order-placing or position-manufacturing smoke runs. Probes are read-only against the account's current flat, cash-only, watchlist-empty state.

**Disposition record**

- R4. A new PROVISIONALITY-LEDGER section (§23) records this wave as a complete domestic close-out: date, the session state at probe time (or a note that U1 was skipped, per KTD3), the 2 fresh probe results with their gate classification, and a re-disposition of the **full 16-TR domestic residue** grouped by blocker class (current-probeable, after-hours-gated, intraday paper-empty, structurally-held, gateway-defect). §23 records only the http / rsp_cd / body_len triple plus a gate label for each probe — never response-body contents. The 14 non-probed TRs cite their existing §19–§22 evidence rather than being re-probed. Each terminal/held TR (including `t1631`) records its reopen trigger, so a future gateway-side or feeder fix is not masked by the close-out. The conclusion: the domestic read residue is fully dispositioned and unmovable without account-state, session-specific, input-sourcing, feeder, realtime-design, or gateway-side triggers.
- R5. §23 supersedes the relevant per-TR reasons in place where this session's evidence refines them (do not stack a parallel resolution layer); all 16 keep `implemented: false` with their gate reason pointing to §23.

The 16-TR domestic residue and its blocker class:

| Class | TRs | Blocker (regular window cannot move) | Probed this wave |
|---|---|---|---|
| Current-probeable (account reads) | `t0441`, `CSPBQ00200` | Open F/O position / margin deposit — session-independent | Yes (live) |
| After-hours-gated | `t1109` | Needs the 15:30–17:50 KST after-hours session | No (cite §19/§20) |
| Intraday paper-empty (§19) | `t1951`, `t1973`, `t2106`, `t2212`, `t2407`, `t8404`, `t8427` | Paper carries no data; proven empty in-window | No (cite §19) |
| Structurally held | `t1852`, `t1856`, `t1860`, `t1964`, `t3102` | `sFileData` input / realtime-control not a read / filter-enum defaults / `NWS` feeder | No (cite §20) |
| Gateway defect | `t1631` | Server-side `IGW40014`; permanent PENDING (reopen on gateway-side fix) | No (cite §19/§20) |

**Invariants**

- R6. No metadata `implemented` flip occurs; docgen `reference.len`, the `banner_trs` allowlist, `maintained_tr_count`, and all `cli.rs` count literals are unchanged. The full gate (`make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`, `make lane-check`) is green before the change is committed.

### Acceptance Examples

- AE1. **Covers R1, R2.** Given the account is flat, when the operator runs `make raw-probe LS_PROBE_TR_CD=t0441 ...`, then the probe returns a success http/rsp_cd with an empty or all-default balance (recorded as `body_len` + gate label, not the body itself), and the result is filed as position-gated reconfirmation in §23.
- AE2. **Covers R4.** Given `t1109`'s gate is the after-hours session (15:30–17:50 KST), when this regular-session wave runs, then `t1109` is dispositioned by citing §19/§20 (not live-probed), because a regular-session probe would only re-derive the prior after-hours-gated finding.
- AE3. **Covers R2.** Given a probe returns unexpectedly populated, deserializable data for its modeled fields, when classified, then it is flagged as a re-open candidate that **exits this wave's 0-flip scope** — handed to a separate certify-flip decision rather than flipped inline — so R6's frozen-counts invariant holds for this wave.

### Scope Boundaries

**Deferred for later**
- `t0441` certify-flip via in-window position manufacture (`make live-smoke-fo-position`) — a separate operator-gated order wave.
- `t1109` flip via an after-hours (15:30–17:50 KST) probe run.
- `CSPBQ00200` flip after an out-of-band margin deposit on the spot lane.
- Overseas-F/O CIDBT chain certification and `o3107`/`o3127` — blocked until overseas markets are open.

**Outside this wave**
- Read-flip prospecting — the raw pool is exhausted; there is no new read surface to track.
- Re-probing the 14 non-current-probeable domestic residue TRs — they are re-dispositioned in §23 by citing existing §19–§22 evidence, not by burning credentials on a probe that cannot surface new information (`t1109` after-hours-gated; intraday feeds proven empty in-window; held TRs blocked on non-window levers; `t1631` a permanent gateway defect).
- Any change to `paper_incompatible: true` TRs (overseas-stock `g3101`–`g3106`, night derivatives `t8455`/`t8460`/`CCENQ*`/`CCENT*`) — terminal, never flip on paper.
- Overseas residue (`CIDBT*`, `COSAT*`/`COSMT*`, `o3107`/`o3127`) — market closed this session.

### Outstanding Questions

**Resolve before planning**
- None. The scope, candidates, and expected outcome are fixed.

**Deferred to planning**
- Exact `LS_PROBE_PATH` / `LS_PROBE_BODY` shapes per candidate — sourced from each TR's normalized baseline during execution (U1).

**Resolved during planning**
- §23 folds a reopen-triggers block mirroring §22's (position manufacture for `t0441`, after-hours run for `t1109`, margin deposit for `CSPBQ00200`, overseas-open for the deferred chains) — resolved in U2 rather than left to a future wave.

### Sources / Research

- `metadata/PROVISIONALITY-LEDGER.md` §18 (raw pool close-out), §19 (in-window intraday-empty proof), §20 (residue partition), §21 (domestic F/O order flips), §22 (exhaustion close-out) — the disposition history this wave extends.
- `metadata/trs/t0441.yaml` — confirms `tracked: true / implemented: false`, `instrument_domain: futures_options`, `account_state: true`, `paper_incompatible: false`.
- `Makefile` — `raw-probe` target (credential-safe classifier); `live-smoke-fo-position` (the deferred manufacture path, `LS_SMOKE_LANE = domestic_option`).
- `docs/plans/2026-07-01-002-feat-env-lane-cutover-both-pool-flip-sweep-plan.md` — prior wave's honesty note that the sweep's likeliest outcome is reconfirmation, not unlock.
- `AGENTS.md` — gate command set and the `raw-probe` credential-safety guidance.

---

## Planning Contract

**Product Contract preservation:** unchanged. Planning added the how-to-build sections below; no R-IDs, scope boundaries, or acceptance examples were altered.

### Key Technical Decisions

- **KTD1. §23 mirrors §22's structure, and reconciles its finer partition to §22's lanes.** `metadata/PROVISIONALITY-LEDGER.md` ends at §22 (line 1195); §23 is the next `## 23. <title> (2026-07-02)` section. §22 already recorded a *terminal* close-out of the full 38-TR residue; §23's delta is fresh current-dated probe evidence for the 2 current-probeable domestic candidates, plus an explicit reconcile of this wave's 5-way domestic partition (current-probeable / after-hours-gated / intraday-empty / structurally-held / gateway-defect) to §22's lane labels (Lane B intraday 7 + Lane C held 6 incl. `t1109`/`t3102` + Lane E domestic 3 = the 16). This prevents the partition drift a reviewer would otherwise hit reading §22 and §23 side by side.
- **KTD2. Credential-safe recording is a hard constraint on §23 text.** §23 records only the `http` / `rsp_cd` / `body_len` triple plus a gate label per probe — never response-body contents, never account identifiers. This is the repo's `raw-probe` norm (`make raw-probe` prints a `RAW-PROBE` line, not a `LIVE-SMOKE` evidence line); it is also the security-review constraint carried into the Product Contract (R4).
- **KTD3. The wave is fail-open on U1 and never flips inline.** If the operator runs U1 and both probes reconfirm their gates, §23 stands on fresh evidence. If U1 does not run (operator unavailable, window closed for the F/O lane), §23 still lands as a documentation-only close-out citing §22 for all 16 — the wave completes with 0 flips either way. A probe that returns unexpectedly populated data (AE3) does NOT flip inline; it is recorded as a re-open candidate and handed to a separate certify-flip decision, preserving the frozen-counts invariant (R6).
- **KTD4. Zero count-site edits.** Because nothing flips, none of the count sites move: docgen `reference.len` (stays 283), the `banner_trs` allowlist, `maintained_tr_count` (stays 320), the `cli.rs` literals, `api_drift`, and `TRACKED_TRS`. No `metadata/trs/*.yaml` `implemented` facet is edited. The only tree change is the new §23 prose in the ledger.

### Sequencing

U1 (operator probe) → U2 (author §23) → U3 (verify invariants + gate). U2 can author the 14 cite-prior TRs and the full structure offline before U1 returns, slotting the 2 fresh probe results in once U1 completes; U3 runs last.

---

## Implementation Units

### U1. Operator live raw-probe pass (2 current-probeable candidates)

- **Goal:** Capture fresh current-dated gate evidence for the two session-independent domestic account reads.
- **Requirements:** R1, R2, R3.
- **Dependencies:** none (operator-run; domestic session open is opportunistic, not required — both are account-state reads).
- **Files:** none created/modified — this unit produces captured `RAW-PROBE` output lines consumed by U2.
- **Approach:** The operator runs `make raw-probe` twice, sourcing `LS_PROBE_PATH` / `LS_PROBE_BODY` from each TR's normalized baseline (`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`):
  - `t0441` — `make raw-probe LS_SMOKE_LANE=domestic_option LS_PROBE_TR_CD=t0441 LS_PROBE_PATH=<from baseline> LS_PROBE_BODY='{"t0441InBlock":{...}}'` (F/O balance on the funded …51 account; expect success rsp_cd with an empty/all-default balance because no position is held).
  - `CSPBQ00200` — `make raw-probe LS_SMOKE_LANE=domestic LS_PROBE_TR_CD=CSPBQ00200 LS_PROBE_PATH=<from baseline> LS_PROBE_BODY='{"CSPBQ00200InBlock":{...}}'` (spot margin on the cash-only default lane; expect a funding-gated result — all-default deposit fields / `00136`, or the documented margin rejection).
  - `t1109` is NOT probed (its after-hours gate cannot be witnessed in a regular session — cite §19/§20 in U2).
- **Execution note:** Operator-run, never autonomous. `raw-probe` is credential-safe (prints only http / rsp_cd / body_len); numeric request-body fields must serialize as JSON numbers or the gateway returns `IGW40011` (source shapes from the baseline, not guesswork — and cross-check against the proven-live SDK request struct where one exists: the CSPBQ00200 baseline under-reports `RecCnt`/`RegCommdaCode`, which the certified SDK shape sends).
- **Test scenarios:** Test expectation: none — this is an operator probe that emits classifier output, not code under test. Its correctness gate is U2's classification (R2) and the AE3 escalation branch.
- **Verification:** Two `RAW-PROBE` lines captured, each classified against its expected gate per R2; any contradicting (unexpectedly populated) result flagged as a re-open candidate per AE3 rather than filed as reconfirmation.

### U2. Author PROVISIONALITY-LEDGER §23 (full 16-TR domestic close-out)

- **Goal:** Record the complete domestic residue disposition with this session's evidence, so the next window isn't spent re-deriving it.
- **Requirements:** R4, R5.
- **Dependencies:** U1 (for the 2 fresh probe results; the 14 cite-prior rows and structure can be drafted before U1 returns).
- **Files:** `metadata/PROVISIONALITY-LEDGER.md` (append `## 23. Domestic KRX-open reconfirmation & close-out (2026-07-02)`).
- **Approach:** Mirror §22's shape (KTD1). Include: a one-line goal + the honest framing (§22 already terminal; §23 adds fresh current-dated evidence for the 2 probeable candidates); the 5-way domestic partition reconciled to §22's lanes; the 2 fresh probe results as `http`/`rsp_cd`/`body_len` + gate label only (KTD2); the 14 cite-prior rows referencing their §19–§22 evidence; a reopen-triggers block mirroring §22's R6 (the forward-pointer to next window's genuine levers — position manufacture, after-hours run, margin deposit — resolved here rather than deferred); and a count tally stating 0 flips with every count site unchanged. Supersede per-TR reasons in place (KTD from Product Contract R5), do not stack a parallel resolution layer.
- **Patterns to follow:** `metadata/PROVISIONALITY-LEDGER.md` §22 (lines 1195–1303) — section heading, lane partition, per-TR bullets, reopen-triggers block, count tally.
- **Test scenarios:** Test expectation: none — §23 is a prose disposition record. Correctness is enforced by U3 (counts unchanged, gate green) and by KTD2 (no body/account data in the text).
- **Verification:** §23 exists, dispositions all 16 domestic TRs, its partition sums to 16 and reconciles to §22's lanes, and it records only credential-safe fields; each of the 16 keeps `implemented: false` with its reason pointing to §23.

### U3. Verify frozen-count invariants and run the gate

- **Goal:** Prove the wave changed nothing but the ledger prose, and the tree is green.
- **Requirements:** R6.
- **Dependencies:** U2.
- **Files:** none modified — verification only.
- **Approach:** Confirm no count site moved: docgen `reference.len` still 283, `banner_trs` unchanged, `maintained_tr_count` still 320, `cli.rs` literals / `api_drift` / `TRACKED_TRS` unchanged, and no `metadata/trs/*.yaml` `implemented` facet edited. Then run the full gate.
- **Test scenarios:** Test expectation: none new — the gate IS the test. Assert: `git diff --stat` shows only `metadata/PROVISIONALITY-LEDGER.md` (and this plan) changed; the gate below passes.
- **Verification:** `make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`, and `make lane-check` all green; diff scope limited to the ledger + plan.

---

## Verification Contract

| Gate | Command | Applies to | Done signal |
|---|---|---|---|
| Docs regenerate | `make docs` | U2, U3 | clean regen, no unexpected diff |
| Workspace tests | `cargo test` | U3 | all pass |
| Metadata + policy cross-check | `cargo test -p ls-core` | U2, U3 | validation + crosscheck green |
| Docs match committed | `make docs-check` | U3 | no diff (counts unchanged) |
| Smoke lane guard | `make lane-check` | U3 | offline lane guard passes |
| Diff scope | `git diff --stat` | U3 | only `metadata/PROVISIONALITY-LEDGER.md` + this plan |

Live probes (U1) are operator-run and credential-gated; they are never part of the autonomous gate (`raw-probe` runs the `#[ignore]`-gated `raw_http_probe` test via `--ignored`, so a default `cargo test` never executes it).

## Definition of Done

- U1 run by the operator (or explicitly skipped → §23 falls back to documentation-only citing §22, per KTD3).
- §23 written: full 16-TR domestic close-out, credential-safe, partition reconciled to §22, reopen triggers recorded.
- **0 flips** — every count site unchanged; no `implemented` facet edited.
- Full gate green; diff scope limited to the ledger + this plan.
- A probe that returned unexpectedly populated data (if any) is recorded as a re-open candidate, NOT flipped inline.
