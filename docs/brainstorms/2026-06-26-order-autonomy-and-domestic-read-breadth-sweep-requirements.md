---
date: 2026-06-26
topic: order-autonomy-and-domestic-read-breadth-sweep
---

# Order-Smoke Autonomy + Domestic Read-Breadth Sweep

## Summary

Two sequenced tracks. First, make the order live-smoke agent-runnable: drop the
operator-handoff requirement while keeping every fail-closed guard and adding a
post-run flat-account assertion. Then, a large breadth sweep over the untracked
domestic read pool — qualify to read-only, paper-reachable TRs, batch-track them,
and flip every one that smokes clean in the current KRX window. Realized count is
emergent; non-flips are tracked and dispositioned Pending.

## Problem Frame

The order surface is already callable and Implemented on paper (submit `CSPAT00601`,
modify `CSPAT00701`, cancel `CSPAT00801`, reconcile `t0425`), and the newly-fixed
paper credentials clear the `01491` account-not-order-capable block that previously
stalled order placement. But the order live-smoke is deliberately **operator-gated**:
a human runs `make live-smoke-order-chain` in-window and the agent waits. That gate
forces a human into the loop on every order wave and re-certification — a recurring
cost each time the order TRs need re-attestation. Track 1 is justified on that
re-certification-cadence cost alone; the reads sweep below does not depend on it (read
smokes are already agent-runnable), so the two tracks are weighed independently.

Separately, only 126 of the ~700 TR codes in the raw OpenAPI capture are tracked, and
of those just 17 are tracked-but-unimplemented — nearly all stuck for real reasons
(paper-empty overseas/night feeds, unresolved request inputs, realtime-only control),
not for lack of an open window. The genuine breadth frontier is the **untracked raw
domestic read pool** (~270 `t`-prefixed codes), which the standard track → implement →
in-window-smoke loop can reach now that the KRX session is open.

## Key Decisions

- **Fish the untracked raw domestic pool, not the 17 tracked-unimplemented.** The
  tracked-blocked set yields near zero on paper (overseas/night feeds the gateway
  carries no data for, input-unresolved reads, realtime-held TRs). New domestic reads
  are where Implemented count actually grows. Before skipping the 17, re-scan only for
  any whose blocker was the `01491`-stalled placement path the new creds now clear —
  the overseas/night/input-unresolved/realtime classes are categorically out of reach of
  creds + an open window, so the re-check is a narrow `01491` sweep, not a generic yield
  source.
- **Emergent count over a hard target.** Flip what smokes clean this window rather than
  cherry-pick to a number, so coverage stays honest and non-flips are dispositioned
  rather than abandoned.
- **Autonomy removes the human handoff, not the guards.** The double opt-in
  (`LS_TRADING_ENV=paper` + `LS_ORDER_SMOKE=1`), band validation, and fail-closed
  evidence all stay; autonomy *adds* a post-run flat-account assertion.
- **Order autonomy is a deliberate risk-acceptance, not a convenience default.**
  Removing the operator gate trades a human pre-placement checkpoint for post-run
  detection (the flat-account assertion). The wave accepts that trade only because
  placement stays paper-only and fail-closed; the reversal cost of an unattended bad
  order is bounded by paper reset. This is recorded as a conscious posture, not an
  unexamined side effect of "make it autonomous."
- **Orders → Recommended stays a separate pass.** Recommended endorses live order
  placement (ADR 0008) and is out of scope here; this wave makes the smoke autonomous,
  it does not promote the order TRs.

## Requirements

**Track 1 — Order-smoke autonomy**

- R1. The order live-smoke runs without an operator handoff — the agent invokes the
  chained smoke directly during a wave. Autonomy is bounded to interactive,
  human-present waves via a concrete refusal precondition the harness checks — a
  detected CI/unattended marker (no TTY, `CI`/`GITHUB_ACTIONS` env) or a per-wave
  human-issued nonce distinct from the standing `LS_ORDER_SMOKE` opt-in (which cannot
  by itself tell an agent wave from CI). The exact signal is resolved in planning; the
  requirement is that one exists and fails closed, so removing the handoff never
  authorizes recurring unattended order placement.
- R2. The existing fail-closed guards are retained unchanged: double opt-in, daily-band
  validation via `t1102`, and "not certified" evidence (never a submit) on unset
  selection or invalid operator params. Autonomous placement additionally asserts the
  resolved environment is paper *after* credential load (never trusting the env var
  alone) — `is_paper()` is the enforceable runtime invariant. Account order-capability
  is orthogonal to environment (the `01491` history shows a paper env can hold an
  order-capable account), so "the configured account is a paper account, not real-money"
  is a credential-provisioning assumption recorded in Dependencies, not a runtime check.
- R3. After the chained run, the harness asserts the account is **flat** — cancel
  teardown succeeded and no resting or filled order remains — and hard-fails loudly if
  it is not. A failed, timed-out, or ambiguous order-state read is treated as NOT flat
  (hard-fail): "flat" is concluded only from positive confirmation, never from absence
  of evidence. Because autonomy removes the operator who previously cleared a failed
  teardown, a NOT-flat result first triggers a best-effort autonomous cleanup (retry
  cancel, then the order kill-switch) before the hard-fail, and the failure names the
  order that may remain resting — detection alone does not satisfy R4's
  no-order-left-resting promise.
- R4. When invoked outside an open KRX regular session, the smoke places nothing and
  records Pending — no order is left resting unattended.
- R5. An unexpected fill is surfaced as a loud, actionable failure carrying the order
  identifiers, not folded into a silent Pending. All autonomous-run output stays
  credential- and account-free, covering non-numeric secrets (bearer tokens, appkeys)
  and the account product suffix — not only account-number digit runs. The known
  unscrubbed `dispatch_once` whole-body/`rsp_msg` debug log is **suppressed** for
  autonomous runs (not merely digit-scrubbed), since whole-body logging carries secrets
  the digit-run scrubber never sees.

**Track 2 — Domestic read-breadth sweep**

- R6. Qualify the untracked domestic `t`-read pool to read-only, paper-reachable
  candidates; exclude overseas (`g`/`o`), order/account-write, realtime, and
  night-only feeds.
- R7. Batch-track qualified candidates — metadata entry plus projected normalized
  baseline — before any flip is attempted. Gate the full batch behind a sampling probe:
  qualify and smoke ~15–20 candidates first; if the read-only paper-reachable yield
  falls below the floor set in planning (see Outstanding Questions), stop and re-scope
  rather than batch-tracking all ~270.
- R8. Flip every tracked candidate that returns a clean in-window paper smoke
  (non-error response, non-empty payload) to Implemented; the realized count is
  emergent.
- R9. Non-flips are tracked and faithfully dispositioned — Pending for input-unresolved
  or paper-empty results — and never silently dropped.
- R10. Each flipped TR carries its smoke-map row, Makefile target, and the emergent
  count-test updates, keeping the gate green.

**Cross-cutting — sequencing and delivery**

- R11. The order-autonomy change lands first as a deliberate risk-sequencing choice —
  ship and verify the higher-risk order change before fanning out — *not* a data
  dependency. The reads sweep is independently deliverable and could run concurrently;
  orders-first is a process preference, not a technical gate.
- R12. The reads sweep ships as stacked, reviewable PRs, not one mega-PR.

## Acceptance Examples

- AE1. Covers R1. **Given** the order smoke is invoked in a CI or otherwise unattended
  context (no human nonce / detected CI marker), **then** it refuses to run and places
  nothing.
- AE2. Covers R2. **Given** credential load resolves a non-paper environment, **then**
  placement is refused even with `LS_TRADING_ENV=paper` set in the shell.
- AE3. Covers R3. **Given** the chained order smoke runs and the cancel link fails
  leaving a resting order, **then** the harness attempts autonomous cleanup, then
  hard-fails reporting the order number, and does not record a clean pass.
- AE4. Covers R4. **Given** the order smoke is invoked outside an open KRX regular
  session, **then** it places nothing and records Pending.
- AE5. Covers R5. **Given** a submit fills unexpectedly before teardown, **then** the
  run surfaces a loud, actionable failure carrying the order identifiers — not a silent
  Pending.
- AE6. Covers R6. **Given** a candidate requires overseas data or an order-capable
  account, **then** it is excluded at qualification and never smoked.
- AE7. Covers R8, R9. **Given** a tracked candidate read returns a non-error, non-empty
  response in-window, **then** it flips to Implemented; **given** it returns empty
  `00707` or is input-unresolved, **then** it stays Tracked and is recorded Pending.

## Scope Boundaries

**Deferred for later**

- Orders → Recommended (separate ADR 0008 pass endorsing live order placement).
- The 17 tracked-but-unimplemented TRs — skipped only after a re-check against current
  conditions (see Key Decisions); any whose blocker the new creds + open window now
  clear are flipped, the rest stay deferred pending their specific blocker.
- Resolving request-field inputs for input-unresolved domestic reads — those record
  Pending this wave rather than being unblocked.

**Outside this wave's identity**

- Overseas stock and derivative feeds (`g`/`o` prefixes) — paper carries no data.
- Night-only derivative TRs and `krx_extended` account TRs (e.g. `CCENQ` family).
- Realtime registration / control TRs (`t1860`-class) — not read-only.

**Pulled into this wave (reconciled)**

- Gating/suppressing the unscrubbed `dispatch_once` whole-body/`rsp_msg` debug log for
  autonomous order runs (per R5). This was a deferred core-dispatch follow-up; autonomy
  raises run frequency and removes the human curating output, so it is deliberately
  in-scope for autonomous runs this wave (the broader dispatch-log hardening for all
  callers stays a separate follow-up).

## Dependencies / Assumptions

- The fixed paper credentials clear `01491` (order-capable account) — verified for the
  order surface; required for R1–R5 to certify rather than record Pending.
- The `.env` must point at a paper account, never a real-money one (R2). The runtime can
  only assert the *environment* is paper, not that the account is real-money-incapable —
  so not provisioning a real-money account is a credential-supply obligation, not a
  runtime guard.
- An open KRX regular session during the sweep window — required for live read depth
  and order-band validity.
- Untracked candidates have no normalized baseline yet; baselines must be **projected**
  from the raw capture (`make api-drift-renormalize`) before tracking (R7).
- Assumption (unproven until sampled): a meaningful fraction of the ~270 untracked
  domestic `t`-codes are genuinely paper-reachable read-only TRs. The realized flip
  yield is unknown until the qualification pass runs.

## Outstanding Questions

**Deferred to planning**

- The precise qualification filter that separates read-only domestic reads from
  realtime / order / overseas within the `t`-pool (owner_class and domain signals).
- The numeric yield floor for the R7 sampling gate (absolute clean-smoke count or a
  fraction of the ~15–20 probe) below which the sweep stops and re-scopes — and that the
  probe is drawn across code families, not a contiguous block, so its yield generalizes.
- Batch size per stacked PR for the reads sweep.
- The concrete unattended-context signal R1 refuses on (CI/no-TTY marker vs. per-wave
  human nonce).
- The exact "flat-account" post-run check shape for R3 (what state the harness reads to
  confirm no resting/filled order remains).

**Surfaced by review (resolve before planning)**

- Should flips be gated on plausible consumer-value rather than pure clean-smoke?
  Every flip is a permanent Makefile/smoke/count commitment, so Implemented-count can
  grow with callable-but-unused reads. This sits in tension with the confirmed
  emergent-count sweep — decide whether to add a value gate or sequence as
  track-all-now / implement-on-demand. (product-lens, adversarial, scope-guardian)
- Should Track 1 (order-smoke autonomy) and Track 2 (read sweep) be split into separate
  requirements docs / plans, given they share no goal or data dependency (R11)?
  (scope-guardian)
