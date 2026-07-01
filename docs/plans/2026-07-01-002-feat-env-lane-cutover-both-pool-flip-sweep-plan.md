---
title: Env-Lane Cutover + Both-Pool Flip Sweep - Plan
type: feat
date: 2026-07-01
topic: env-lane-cutover-both-pool-flip-sweep
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

<!-- Product Contract preservation: unchanged. Planning enriched R1–R14 in place; no product-scope edits. -->


# Env-Lane Cutover + Both-Pool Flip Sweep - Plan

## Goal Capsule

- **Objective:** Retire the legacy `.env` from the smoke harness in favor of the named per-lane env files (the durable win), then run a triage-gated disposition pass over the tracked-PENDING backlog and the untracked residue — expecting few net-new flips, because the wrong-account lever was largely spent on 2026-06-28.
- **Product authority:** Repo owner (sunkeunchoi). Paper-only; no real-money lanes.
- **Open blockers:** Live flips require KRX open (open at authoring: 2026-07-01 12:00 KST) and the correct lane file present per instrument domain. None blocking the offline cutover.

## Product Contract

### Summary

Cut the Paper Live Smoke harness over so every target sources a named per-lane env file — the default domestic lane hard-requires `.env.domestic` (no silent fallback), the futures-options lane keeps `.env.domestic_option`, and overseas_stock (`.env.overseas`) gets a dedicated lane distinct from overseas_futures (`.env.overseas_option`) — then delete the legacy `.env`. That cutover ships as a standalone landable unit. On top of it, a triage-gated disposition pass classifies both flip pools by cause with a credential-safe probe first, spends the scarce open window only on the buckets a live smoke can actually move, and records each TR as flipped, PENDING, paper_incompatible, or excluded.

### Problem Frame

The smoke harness's default lane (empty `LS_SMOKE_LANE`) still sources the legacy `.env`. That file carries the old `LS_PAPER_APPKEY` name plus three dead multi-account number vars (`LS_PAPER_ACCOUNT_OPTION` / `_OVERSEAS` / `_OVERSEAS_OPTION`) that the SDK never reads — a residue of the abandoned "multi-account in one file" model, retired once accounts were proven token-bound not number-bound. Only `domestic_option` and `overseas_option` lanes are wired via target-specific `LS_SMOKE_LANE`; overseas_stock silently falls through to `.env` (the domestic account), and the clean canonical `.env.domestic` sits unused.

The cost is a live wrong-account hazard: overseas_stock reads authenticate as the domestic account, and a missing lane file silently falls back to `.env` rather than failing loud. Fixing that — plus deleting the dead multi-account config — is the durable value of this wave and stands on its own, independent of any flip.

A prior framing of this wave treated correct lane wiring as an untapped lever for new flips. That is largely wrong, and the plan must not lean on it: the 2026-06-28 wave (plan `-002`, ledger §16/§17) already built these per-lane files, already re-smoked the wrong-account PENDINGs, and already extracted the lane-attributable flips (CFOEQ11100, CIDBQ01400, CIDBQ03000, CIDBQ05300 — now Implemented). The residue is what remained *after* that lever was pulled. Its futures-options/overseas weighting predicts **lower** yield, not higher: those domains already failed on their correct lanes for terminal reasons — `01900` venue rejection (CCENQ persists even on the F/O-capable account), `00707` paper-empty (overseas `g3xxx`), or no-position/funding gating. The pool a live re-smoke can still move may in fact be empty. The candidates once thought session-movable — the t845x night-derivatives — are already terminal: `t8455`/`t8460` are `paper_incompatible: true` and §14 re-probed the trio *inside* the krx_extended night window (01:11 KST) to empty `00707`. They are not reachable in a KRX-regular noon window and, per §14, not reachable on paper at all. So the honest expectation is that the movable pool is a small wrong-lane remainder at most, quite possibly zero. The sweep is therefore an empirically-gated probe pass (R8) whose most likely outcome is a re-confirmation of §20's disposition, not a lane- or session-unlock — and the plan says so rather than promising flips.

### Key Decisions

- **Hard-require `.env.domestic`, delete legacy `.env`.** The default domestic lane sources `.env.domestic` and fails fast if it is missing — no silent fallback to `.env`, mirroring the existing `_option`-lane guard. The legacy `.env` file is deleted from disk. This is safe because `.env.domestic` carries the same working domestic credential — the same appkey value, secret, and resolved account (`...3701`; the `...01` string in the current Makefile comment is a stale label, not a different account) — which the SDK resolves identically (`.env.domestic` exposes the appkey as `LS_PAPER_APIKEY`, resolved the same as `.env`'s `LS_PAPER_APPKEY`); only the dead multi-account vars are lost. Deletion happens only after the cutover verifies the named lane authenticates (value-equality, not just name-equality).
- **Cutover ships as a standalone landable unit, verified before any flip smoke.** The cutover's value depends on no flip, so it lands and gates green on its own (ideally its own PR). A mis-wired lane would authenticate flip smokes as the wrong account and corrupt evidence, so the lane rewire is verified (a per-lane token/quote smoke) before the sweep begins ("fix lane first"). The sweep rides on top and can be truncated when the window closes without stranding the cutover.
- **Probe before the window; disposition is the floor, flips are not expected.** A credential-safe `raw-probe` triage buckets both pools by cause *before* live smokes run, so the scarce open window is spent only on TRs a live smoke can move. Success is every TR carrying an honest disposition — flipped, PENDING, paper_incompatible, or excluded — with the probe rsp_cd as evidence. Because the lane lever is largely spent, a mostly- or all-reconfirmation outcome is expected and is a legitimate stop-signal, not a failure; the wave states its net-new count so re-confirmation is visible rather than counted as fresh progress.
- **Reads only; order-TR logic untouched.** Both pools are read TRs and the order chain is already Implemented and certified — no order-TR work and no Recommended promotions this wave. (The order smoke *recipes* still get their credential sourcing rewired under R3 as harness plumbing — that is a sourcing change, not an order-TR change.) The read sweep needs no operator gating beyond an open window.

### Requirements

**Lane cutover (offline, non-negotiable precondition)**

- R1. Every smoke target that sources credentials sources a named per-lane env file (`.env.domestic`, `.env.domestic_option`, `.env.overseas`, `.env.overseas_option`) — never the legacy `.env`.
- R2. The default lane (empty `LS_SMOKE_LANE`) sources `.env.domestic` and fails fast with a wrong-account-hazard message if it is absent; it must not fall back to `.env`.
- R3. Every remaining `.env`-sourcing branch — the `run_smoke` default-lane branch, `live-smoke-order`, `live-smoke-order-chain`, and `live-smoke-fo-order`'s default branch — is rewired to the same fail-fast named-lane sourcing; no `.env` reference remains in any recipe after the cutover.
- R4. overseas_stock TRs are mapped to an `overseas` lane (`.env.overseas`) via target-specific `LS_SMOKE_LANE`, so they no longer authenticate as the domestic account.
- R5. The legacy `.env` file is deleted, and the Makefile header/comment block and any docs that describe the `.env`-sourcing model (including the stale `account ...01` note — the real domestic account is `...3701`) are updated to reflect named lanes.
- R6. The offline gate stays green after the cutover (`make docs`, `cargo test`, `cargo test -p ls-core`, `make docs-check`); smoke tests remain `#[ignore]` and are unaffected by the sourcing change at compile time.

**Triage-gated disposition pass (live, window- and account-bound)**

- R7. The cutover is landed and verified against each lane it touches before any flip smoke runs.
- R8. Before any live smoke, a credential-safe `raw-probe` pass (prints only http / rsp_cd / body_len) classifies every TR in both pools — the tracked-PENDING backlog (currently 38 `implemented: false`) and the untracked residue (~30) — on its correct lane into cause buckets: **lane-fixable** (authenticates and returns non-empty), **`01900`-terminal** (venue/paper-incompatible), **`00707`-paper-empty**, **session-gated** (needs a different session window), and **no-position/funding-gated**. This bucketing is the sweep's denominator.
- R9. Live smokes are spent only on the buckets a smoke can move — lane-fixable, plus any session-gated TR whose required window is currently open. Terminal buckets (`01900`, paper-empty, funding/position-gated) are dispositioned directly from the probe without consuming the open window.
- R10. Each tracked-PENDING TR is dispositioned per its bucket: flipped to Implemented on a clean non-empty live smoke, or recorded PENDING / paper_incompatible with the probe rsp_cd as evidence.
- R11. Each untracked residue code is qualified against its bucket: a code in a movable bucket is tracked (struct + policy + facade + offline tests) then flipped or dispositioned; a code in a terminal bucket is recorded excluded with its rsp_cd and is **not** tracked (tracking cost is not spent on unreachable codes).
- R12. Each flip carries a non-empty smoke witness on the substantive modeled field (not an empty `00707` / not body_len alone); a TR that only returns an empty or session-gated response is recorded PENDING, not flipped.
- R13. A TR that cannot be smoked before the window closes is recorded PENDING rather than flipped on stale or wrong-window evidence; the wave reports which TRs were left unsmoked and its net-new-flip count versus re-confirmations.
- R14. Registration and count-bookkeeping stay consistent for every flip/track (docgen `reference.len` / `banner_trs`, `maintained_tr_count`, both crosscheck lists, cli.rs count literals, revert-manifest `refreshed`) so the gate stays green.

### Scope Boundaries

- **In scope:** the harness lane cutover + legacy `.env` deletion; the two-pool read sweep with per-TR disposition.
- **Deferred for later:** any TR that cannot be smoked this window (recorded PENDING); Recommended promotions of newly-Implemented reads.
- **Outside this wave's identity:** order-TR *logic/metadata* (already Implemented/certified) — but order-recipe `.env`-sourcing plumbing is rewired under R3 and is in scope; real-money lanes, a new raw-capture refresh, and edits to `.env.example`.

### Dependencies / Assumptions

- The four named lane files exist and each carries a working, token-bound credential for its account; `.env.domestic` is confirmed to carry the same credential values (appkey, secret, account `...3701`) as the live `.env`, exposed under the `LS_PAPER_APIKEY` name.
- The SDK resolves the appkey through `LS_PAPER_APIKEY` → `LS_PAPER_APPKEY` → legacy `LS_APPKEY`, so both the old and new key names authenticate; the account is whatever the token resolves to (single `LS_PAPER_ACCOUNT` per file).
- Live flips require KRX open and paper data actually flowing for the feed; overseas_stock reads are known paper-empty (`g3xxx` → `00707`) and are expected to disposition PENDING/paper_incompatible even with the lane wired.
- The gate never runs live smokes (all `#[ignore]`), so an offline-only run flips nothing by construction — flips require the operator/agent to run the live smokes in-window.

### Outstanding Questions

**Deferred to planning:**
- Exact per-lane mapping of each residue code to its instrument_domain (which of `cfo*`/`coso*`/`cido*`/`cspaq*` route to `domestic_option` vs `overseas_option` vs `overseas` vs `domestic`) — resolved concretely by the R8 probe.
- Which of the 38 tracked-PENDING carry a recorded cause of authentication/wrong-account (the only lane-fixable class) versus `01900` / `00707` / no-position (terminal) — the R8 probe produces this; planning decides the smoke order for the movable subset.
- How large the movable pool actually is once the R8 probe buckets both pools — this sets whether the sweep is worth a live window at all this session.

**Resolved in this brainstorm:**
- Cutover vs sweep packaging — the cutover ships as a standalone landable unit (its own PR), verified green before the sweep runs; the sweep is a follow-on that can be truncated at window close (R7, Key Decisions).

**Review-flagged (round 2) — the R8 probe is the arbiter; the operator elected to keep the sweep despite these priors:**
- The sweep may have **no movable pool**. Two review rounds falsified the flip levers: the lane lever was spent on 2026-06-28 (§16/§17), and the session lever (t845x) is terminal per §14 (`paper_incompatible`/`00707`). Treat t845x as pre-classified terminal in R8 — do not schedule it for a live smoke. If R8 buckets every candidate as terminal, R9 runs no live smokes and R13 reports a pure re-confirmation (0 net-new); that is the expected outcome, not a defect.
- R8 **overlaps §20's completed partition** (2026-06-30 dispositioned all 41 Tracked-not-Implemented; only 5 probe-gated candidates remained: t1109 / CSPBQ00200 / o3107 / o3127 / t0441, all operator/session/funding-gated). Planning should inherit §20's partition as R8's starting denominator and re-probe only cells whose gating condition may have changed, rather than re-triaging from scratch.
- The **residue counts are soft**. "38 tracked-PENDING" and "~30 untracked residue" are not the same pool; the untracked F/O/overseas codes that exist (`cfoaq00600`, `cfoeq82600`, `foccq33600`, `cidbq*`…) were already probed terminal (`01900`/`00707`) in §17, so R11's tracking arm is expected to fire for **zero** codes. Planning should reconcile the denominators against §20 before deciding the sweep is worth a live window.

---

## Planning Contract

### Key Technical Decisions

- **KTD1. Sourcing stays in the recipe shell — never make `include`.** Every lane load keeps the `set -a; . "./$$lane_file"; set +a` shell-sourcing already used by `run_smoke`. Make's `include` keeps surrounding quotes literally and reaches the SDK with quote chars (403); the shell strips them. See `docs/solutions/integration-issues/makefile-include-env-quotes-*` and the Makefile header rationale. This is a constraint on every edit in U1/U3, not a new decision.
- **KTD2. Lift the fail-fast guard so the default lane is `.env.domestic`, not `.env`.** Today `run_smoke`'s `else` branch (empty `LS_SMOKE_LANE`) sources `.env`. The cutover makes the default lane resolve to `domestic` and run through the *same* fail-fast guard the `_option` lanes already use — the guard moves out of the `if [ -n "$(LS_SMOKE_LANE)" ]` conditional and applies to every lane including the default. Mechanically: default `LS_SMOKE_LANE` to `domestic` so `lane_file=".env.domestic"` for the empty case, and drop the `.env` fallback branch entirely. The legacy `.env`'s multi-account variant keys (`LS_PAPER_ACCOUNT_OPTION` / `_OVERSEAS` / `_OVERSEAS_OPTION`) are unread by the SDK (accounts are token-bound per lane file; grep confirms zero readers), so dropping them with the `.env.domestic` default is intentional and lossless.
- **KTD3. overseas_stock is a new target-specific lane group.** The `g3101`/`g3102`/`g3103`/`g3104`/`g3106`/`g3190` targets (overseas-stock reads) get a `LS_SMOKE_LANE = overseas` target-specific assignment, mirroring the existing `domestic_option` / `overseas_option` grouped lines. They currently fall through to the domestic default — this is the live wrong-account fix. **The lane token is `overseas`** (the file is `.env.overseas`), not `overseas_stock`: `run_smoke` derives `lane_file=".env.$(LS_SMOKE_LANE)"`, so using the instrument_domain name `overseas_stock` would resolve a nonexistent `.env.overseas_stock` and hard-fail all six smokes under the fail-fast guard. Follows the existing abbreviation convention (`futures_options` → `domestic_option`).
- **KTD4. `.env` deletion is gated on lane-auth verification (irreversible-op ordering).** U4 runs a per-lane token/quote smoke that confirms each lane file *authenticates and resolves the expected account* (value-equality, not just that the file exists) **before** `git rm`/deleting `.env`. Deleting first and discovering a bad lane file would strand the harness.
- **KTD5. The cutover touches no TR-count sites; only a real flip does.** `reference.len` (`crates/ls-docgen/src/lib.rs:1416`), `banner_trs` (`:1116`), `maintained_tr_count`, the `policy_index_crosscheck` lists, and the `cli.rs` literals move only when a TR flips or is tracked. The sweep expects zero flips, so the cutover PR (U1–U4) changes none of them and the offline gate stays green by construction. No Rust test asserts `.env` sourcing, so the Makefile edits are behaviorally invisible to `cargo test`.
- **KTD6. t845x is pre-classified terminal — excluded from live smokes.** Per ledger §14, `t8455`/`t8460` are `paper_incompatible: true` and the night trio was re-probed empty (`00707`) inside its own krx_extended window. U5's triage records them terminal from the ledger; it does **not** schedule them for a live smoke. This is the round-2 review correction.

### Execution posture

The cutover units (U1–U4) are offline and gate-verified. The sweep units (U5–U6) are **operator-run, non-autonomous, and window/session-bound** — they hit the real LS paper gateway and must not run in an unattended pipeline. `lfg`/`ce-work` should land U1–U4 and stop; U5–U6 are an operator follow-on. U4's live token smoke is a single credential-safe read, safe for an attended run.

---

## Implementation Units

### U1. Rewire the smoke harness to named per-lane sourcing

- **Goal:** Every credential-sourcing recipe sources a named lane file; the default lane hard-requires `.env.domestic` with no `.env` fallback.
- **Requirements:** R1, R2, R3, R4, R6.
- **Dependencies:** none.
- **Files:** `Makefile`.
- **Approach:** Per KTD2, default `LS_SMOKE_LANE` to `domestic` and restructure the `run_smoke` define so the fail-fast lane guard applies to every lane (drop the `else … . ./.env` branch). Rewire the three order recipes that still source `.env` directly — `live-smoke-order` (`Makefile:81`), `live-smoke-order-chain` (`Makefile:109`), and `live-smoke-fo-order`'s default branch (`Makefile:136`) — to the same guarded named-lane sourcing (domestic lane for order/order-chain; `live-smoke-fo-order` already maps `domestic_option`). Add the `overseas` lane group (KTD3) for the `g31xx`/`g3190` targets. After this unit no `.env` token remains in any recipe body.
- **Patterns to follow:** the existing `_option`-lane guard inside `run_smoke` (`Makefile:37-43`) and the grouped target-specific `LS_SMOKE_LANE` assignments (`Makefile:51-66`).
- **Test scenarios:**
  - Covers R1/R3. `grep -n '\.env\b' Makefile` returns only comment lines and `.env.<lane>` references — no recipe sources bare `.env`.
  - Covers R2. `make live-smoke` (default lane) with `.env.domestic` present sources it and authenticates; the recipe never reads `.env`.
  - Covers R4. `make live-smoke-g3101` resolves `LS_SMOKE_LANE=overseas` → `.env.overseas` (assert via a dry echo of `lane_file`, not a live call).
- **Verification:** every recipe's lane resolution is correct by inspection + the U2 guard test passes; `cargo test`, `make docs-check` unaffected (KTD5).

### U2. Fail-fast guard regression check

- **Goal:** A missing lane file makes the recipe exit non-zero with a wrong-account-hazard message — never a silent `.env` fallback.
- **Requirements:** R2, R3.
- **Dependencies:** U1.
- **Files:** `scripts/` (new lightweight shell check, e.g. `scripts/lane-fail-fast-check.sh`) or a `.PHONY` Makefile target that self-tests; wire it into the offline verification the gate already runs if a natural home exists.
- **Approach:** With a lane file temporarily absent (e.g. `LS_SMOKE_LANE=domestic` and no `.env.domestic`, run in a temp dir or with a renamed file), assert the recipe exits non-zero and prints the `refusing to fall back to .env` message. This is the safety net that replaces the deleted `.env` — without it, a future regression that reintroduces the fallback would be silent.
- **Test scenarios:**
  - Missing `.env.domestic` on the default lane → non-zero exit, message contains `wrong-account hazard`.
  - Missing `.env.overseas` on `LS_SMOKE_LANE=overseas` → non-zero exit.
  - Present lane file → guard passes (no false positive). Uses a dummy lane file with placeholder values; no live gateway call.
- **Verification:** the check fails loudly when a lane file is absent and passes when present, with no network dependency.

### U3. Update Makefile header/comments and docs to the named-lane model

- **Goal:** The Makefile header block and any docs describing the `.env`-sourcing model reflect named lanes; the stale `account ...01` label is corrected.
- **Requirements:** R5.
- **Dependencies:** U1.
- **Files:** `Makefile` (header comment block `1-33`), `AGENTS.md` and `docs/solutions/*` only if they describe `.env` sourcing (grep first; edit only real references).
- **Approach:** Rewrite the header rationale (default lane now `.env.domestic`; no `.env` fallback; overseas_stock → `.env.overseas`). Fix the `stock / overseas_stock / unmapped -> .env (account ...01)` comment: the domestic account is `...3701`, and unmapped now hard-fails rather than falling back.
- **Test scenarios:** `Test expectation: none — documentation/comment-only unit.` Verified by review that no comment still claims a `.env` default or the `...01` account.
- **Verification:** no comment or doc describes the retired `.env`-default behavior.

### U4. Verify each lane authenticates, then delete legacy `.env`

- **Goal:** Confirm every named lane file authenticates and resolves its expected account, then remove `.env` from disk.
- **Requirements:** R5, R7.
- **Dependencies:** U1, U3.
- **Files:** `.env` (deleted); no source files.
- **Approach:** Run a credential-safe token/quote smoke per lane (`make live-smoke` for domestic; a single read per `_option`/`overseas` lane) confirming HTTP 200 **and** the resolved account tail matches the lane's intended account (KTD4). The harness has no account-tail assertion today, so the value-equality check needs a concrete read surface: run each lane smoke with the existing debug output (or add a one-line `echo` of the resolved `LS_PAPER_ACCOUNT` in the recipe) so the operator can eyeball the printed tail against the per-lane expected value — a 200 alone does not prove the *right* account. Only after all lanes verify, `git rm`/delete `.env`. Because `.env` is gitignored, deletion is a local-file removal; the value-equality of `.env.domestic` vs the old `.env` domestic credential was confirmed during brainstorm grounding (account `...3701`, identical appkey/secret values).
- **Execution note:** operator-run, attended; live gateway calls. Do not run unattended.
- **Test scenarios:**
  - Each lane's token smoke returns 200 and the expected account tail before deletion.
  - Post-deletion, `make live-smoke` still passes (proves the default no longer depends on `.env`).
- **Verification:** `.env` absent; all lane smokes green sourcing only named files.

### U5. Pre-window `raw-probe` triage over both pools *(operator, live)*

- **Goal:** Classify every TR in the tracked-PENDING pool and the untracked residue by cause, inheriting §20's partition, to size the movable pool before spending the open window.
- **Requirements:** R8, R9.
- **Dependencies:** U1–U4 (cutover landed and verified).
- **Files:** none (produces a disposition record, not code); update `metadata/PROVISIONALITY-LEDGER.md` with the refreshed partition.
- **Approach:** Start from §20's partition of the 41 Tracked-not-Implemented (19 confirm-only / 10 deferred-orders / 7 §19-reconfirm / 5 probe-gated) rather than re-triaging from scratch. Run `make raw-probe LS_SMOKE_LANE=<lane>` (credential-safe: http/rsp_cd/body_len only) for the cells whose gating condition could have changed — chiefly the 5 probe-gated candidates (t1109 / CSPBQ00200 / o3107 / o3127 / t0441) on their correct lanes. Bucket each into lane-fixable / `01900`-terminal / `00707`-paper-empty / session-gated / no-position. Pre-classify t845x terminal from the ledger (KTD6) — do not probe. For the untracked residue, record that the F/O/overseas codes were already probed terminal in §17.
- **Execution note:** operator-run, attended; credential-safe probe (no orders, no fills).
- **Test scenarios:** `Test expectation: none — investigative disposition pass, not a code change.` Evidence is the per-TR rsp_cd bucket recorded in the ledger.
- **Verification:** every TR in both pools carries a cause bucket; the movable subset (if any) is named with its lane.

### U6. Disposition and flip-bookkeeping on any movable TR *(operator, live)*

- **Goal:** Live-smoke only the movable bucket, flip cleanly-witnessed TRs, and disposition the rest honestly; report net-new vs re-confirmation.
- **Requirements:** R10, R11, R12, R13, R14.
- **Dependencies:** U5.
- **Files (only if a flip actually lands):** `metadata/trs/<tr>.yaml` (+ new struct/policy/facade + offline tests per `implement-tr` recipe for a tracked residue code); `crates/ls-docgen/src/lib.rs` (`reference.len` `:1416`, `banner_trs` `:1116`); the `policy_index_crosscheck` lists; `crates/ls-trackers/src/cli.rs` count literals; revert-manifest `refreshed`.
- **Approach:** For each movable TR, run its live smoke in-window; flip to Implemented only on a non-empty witness on the substantive modeled field (R12) — an empty/`00707`/session-gated response records PENDING, never a flip (R10). Track an untracked residue code only if U5 bucketed it movable (expected zero — R11). Apply the full count-bookkeeping (R14) per the `implement-tr` recipe **only** when a flip lands; if the window closes first, record unsmoked TRs as PENDING (R13). Report the net-new-flip count versus re-confirmations so a pure-reconfirmation outcome is visible (Key Decision: probe-before-window).
- **Execution note:** operator-run, non-autonomous, window/session-bound. Most likely outcome per two review rounds: 0 net-new flips, pure re-confirmation of §20.
- **Test scenarios:**
  - Covers R12. A flipped TR's Paper Live Smoke asserts the substantive modeled field non-empty (per `implement-tr`); the offline gate (`cargo test`, `make docs`, `make docs-check`) is green after count-bookkeeping.
  - Covers R10/R11. A TR returning empty/`00707` is recorded PENDING/excluded with its rsp_cd, not flipped/tracked.
- **Verification:** offline gate green; every movable TR dispositioned with evidence; ledger + net-new count updated.

---

## Verification Contract

- **Offline gate (U1–U4, U6-on-flip):** `make docs` && `cargo test` && `cargo test -p ls-core` && `make docs-check` all green. The cutover changes no count sites (KTD5), so the gate is green immediately after U1–U4.
- **Fail-fast guard (U2):** the lane-missing check exits non-zero with the wrong-account-hazard message; the lane-present check passes. No network dependency.
- **Lane authentication (U4):** each named lane's token smoke returns 200 and the expected account tail; `make live-smoke` passes after `.env` deletion.
- **No residual `.env` (U1/U3):** no recipe body or comment sources or references bare `.env` as a credential default.
- **Sweep (U5–U6):** every TR in both pools carries a recorded cause bucket; any flip carries a non-empty witness and passes the offline gate with count-bookkeeping applied; the wave reports its net-new-flip count.

## Definition of Done

- U1–U4 landed as a standalone, gate-green PR: named-lane sourcing everywhere, default lane hard-requires `.env.domestic`, overseas_stock on `.env.overseas`, `.env` deleted, comments/docs updated, fail-fast guard test in place.
- No recipe sources the legacy `.env`; a missing lane file fails loud.
- Sweep (U5–U6), operator-run: both pools bucketed, movable subset smoked in-window, each TR dispositioned (flipped / PENDING / paper_incompatible / excluded) with evidence, ledger + net-new count updated. Zero net-new flips is an acceptable, expected DoD outcome.
- Offline gate green at every landed step.

---

## Sources & Research

- `Makefile:36-66` — `run_smoke` lane logic + target-specific `LS_SMOKE_LANE` mapping (domestic_option / overseas_option); default branch (`Makefile:42`) and `live-smoke-order` / `live-smoke-order-chain` (`Makefile:81`, `Makefile:109`) source `.env`; `live-smoke-fo-order` guard (`Makefile:130-141`).
- `crates/ls-core/src/config.rs:323-360` — paper appkey fallback chain + real-money interlock; single `LS_PAPER_ACCOUNT` resolution (the multi-account `.env` vars are dead).
- `metadata/trs/*.yaml` — 320 tracked, 38 `implemented: false` (the PENDING pool).
- `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json` — raw capture; ~30 real untracked residue codes remain (futures-options / overseas weighted).
- Prior waves: `docs/plans/2026-06-28-002-feat-paper-account-credential-lanes-plan.md` + `metadata/PROVISIONALITY-LEDGER.md` §16/§17 — the wave that already built these per-lane files and extracted the lane-attributable flips (CFOEQ11100/CIDBQ01400/CIDBQ03000/CIDBQ05300) and recorded the F/O/overseas terminal `01900`/`00707` results; this is the evidence the lane lever is largely spent. And `docs/plans/2026-07-01-001-feat-krx-open-domestic-fo-order-certify-flip-plan.md` (fo-order lane guard, already landed).
