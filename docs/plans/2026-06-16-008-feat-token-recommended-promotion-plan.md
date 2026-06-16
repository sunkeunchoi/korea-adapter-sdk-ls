---
title: "feat: Promote token to first Recommended TR with truthful freshness policy"
date: 2026-06-16
type: feat
origin: docs/brainstorms/2026-06-16-first-recommended-tr-token-promotion-requirements.md
---

# feat: Promote `token` to the first Recommended TR with a truthful freshness policy

## Summary

Promote `token` to the first **Recommended TR**: capture a durable, credential-free
Focused Evidence record from a verified Paper OAuth smoke, flip
`support.recommended` and set `maintenance.last_reviewed` to that run's date,
regenerate the SDK docs so the "not recommended" banner disappears, and rewrite the
freshness policy to *truthfully* describe the post-PR5 state. No evaluator, tracker,
or validator code is written — the change-driven invalidation and 90-day backstop
remain documented intent because no code enforces them today.

---

## Problem Frame

The maintained SDK has six **Implemented TRs** but no **Recommended TR**, so the
Implemented→Recommended ladder has never run once. The first promotion proves that
lifecycle on the narrowest possible claim (`token`: standalone, OAuth-only,
paper-compatible). The audience is a future maintainer re-entering the project, not
external consumers (see origin).

Research changed one load-bearing assumption from the brainstorm. The brainstorm
assumed that because the Specification Document Tracker shipped (PR #5),
change-driven evidence invalidation could be turned "active." It cannot, truthfully:

- `Severity::Evidence` and `Severity::Critical` are defined but emitted by no code
  path (`crates/ls-trackers/src/types.rs` — they appear only in a unit test).
- No code parses `maintenance.last_reviewed` or computes the 90-day backstop.
- Spec-doc findings never gate (`gates: false` by construction) and never stale
  evidence.
- No Focused Evidence artifact, store, or format exists anywhere — it is net-new.
- The only wired support-aware behavior is *structural* severity, and `token` is
  already `implemented: true`, which already counts as "strong." Flipping
  `recommended: true` therefore changes **zero** tracker behavior.

So the freshness policy doc (`metadata/EVIDENCE-FRESHNESS.md`) and the index header
are stale today ("inactive because the tracker does not exist"), but the fix is to
state the truth — the tracker now exists and sees example changes, while the backstop
and invalidation remain intent-only — not to swap in a different overclaim ("active").

---

## Key Technical Decisions

- **Truthful policy over literal "active" (diverges from origin wording).** The
  brainstorm's requirement to flip change-driven invalidation to "active" is
  reinterpreted: the policy is rewritten to accurately describe what exists in code
  vs. intent. Rationale: research found the staling/backstop machinery has no
  enforcing code; the brainstorm's stated goal was honesty for a future maintainer,
  and a second false claim would defeat it (see origin: the freshness-policy
  requirements).

- **No tracker / validator / docgen *production* code changes — but two test/harness
  edits.** `token` is already `implemented`, so it is already in the strong/gating
  tier; promotion is a pure data + docs + policy change to production code. The
  metadata validator (`crates/ls-metadata/src/validator.rs`) checks only the four
  routing fields and never inspects `support`, so flipping `recommended` needs no
  validator change and no `metadata/tr-index.yaml` edit (the index carries no
  `support`). Two test/harness files do change: (1) the `live_smoke_default`
  `record()` call site emits a credential-free, dated evidence line (U6, see below);
  (2) the `ls-docgen` test
  `reference_covers_six_implemented_with_banner_and_omits_unimplemented` hardcodes
  `token` in its "not yet recommended" banner assertion and reads the *authored*
  metadata — so the flip turns `cargo test -p ls-docgen` red until that fixture is
  updated (U5). This is why the verification gate is the full network-free `cargo test`,
  not `cargo test -p ls-metadata` alone.

- **Ship the Recommended label now, not after the backstop evaluator.** The promotion
  changes no enforced behavior, so one could defer the label until the cheap
  `last_reviewed`-only backstop lands. The label ships now anyway because the value is
  proving the data + docs + policy ladder end-to-end on one live subject — which then
  gives the deferred backstop work a real Recommended TR to act on (see origin:
  "deepen first / prove the lifecycle with one narrow claim").

- **Durable evidence as a committed, credential-free convention file — manual
  capture.** The Focused Evidence record is a committed file keyed by TR code,
  populated by the operator from the smoke's stdout line (target / inputs / result /
  date). It records only structural descriptors and lengths, never secrets, matching
  the secret-safety discipline in the change-tracker baseline learning. No
  `ls-metadata` schema field links it (avoids a schema change); the filename
  convention plus the `last_reviewed` date is the link. Auto-writing it from the
  harness and a schema `evidence_ref` field are deferred.

- **`last_reviewed` anchors to a verified genuine OAuth 200.** Per the
  `make include .env` quote-contamination learning, a quoted credential yields a false
  403 that looks like bad creds. The evidence run must confirm a real 200 + non-empty
  token (shell-sourced `.env`, `LS_TRADING_ENV=paper`) before its date is recorded — a
  guard miss or false 403 must never become evidence.

---

## Requirements Traceability

Origin requirements (see origin doc) map to units as follows:

- Promotion (recommended flag, evidence definition, durability, evidence level,
  user-facing statement) → U1, U2, U4.
- Freshness-policy turn-on → U3, **but not as the origin literally specified it.** The
  origin required stating that change-driven invalidation is "active"; research
  falsified that premise (the trackers shipped, but the staling/backstop *evaluator*
  did not). So the origin's "active" requirement is **explicitly not satisfied as
  written** — U3 instead records the truthful state, and the origin's freshness
  acceptance examples (structural change stales evidence; example finding is advisory;
  90-day backstop fires; description changes are informational) are **explicitly
  unimplemented**, preserved only as documented intent in U3 with no enforcing code
  and no tests (see Scope Boundaries). The upstream premise correction is a separate
  decision (see Open Questions).

---

## Implementation Units

### U6. Make the default smoke emit a credential-free, dated evidence line

- **Goal:** So U1 can capture the smoke line **verbatim** without hand-editing, fix the
  emitted line at its source: drop `rsp_msg` and add a machine-emitted date.
- **Requirements:** unblocks U1's verbatim, credential-free, dated capture.
- **Dependencies:** none.
- **Files:** `crates/ls-sdk/tests/live_smoke.rs` (`live_smoke_default` `record()` call
  site only — not the shared `record()` signature, so `live_smoke_chart` /
  `_account` / `_ws` are untouched)
- **Approach:** Edit the `result`/`inputs` strings passed by `live_smoke_default` to
  (1) omit `rsp_msg` (keep the numeric `rsp_cd`, which proves success without carrying
  localized account-identifying text) and (2) include a UTC date emitted by the run
  itself. The point is that the printed `LIVE-SMOKE` line is then credential-free and
  dated *by construction*, so U1's verbatim capture cannot reintroduce `rsp_msg` and
  the run date is machine-attested rather than hand-typed. No production SDK code
  changes; this is the default smoke's call site only.
- **Patterns to follow:** the existing `record()` credential-free contract and call
  sites in `crates/ls-sdk/tests/live_smoke.rs:78`.
- **Test scenarios:**
  - The default `live_smoke_default` still compiles and its non-ignored sibling tests
    pass; the emitted line contains `rsp_cd`, a UTC date, and no `rsp_msg`.
  - Test expectation otherwise: none (the smoke itself is `#[ignore]`d and operator-run).
- **Verification:** A `make live-smoke` run prints a `LIVE-SMOKE` line with a date and
  no `rsp_msg`.

### U1. Capture durable Focused Evidence for `token`

- **Goal:** Produce a committed, credential-free evidence record proving a genuine
  live Paper OAuth token issuance, with its machine-emitted date.
- **Requirements:** Focused-evidence definition and durability (origin).
- **Dependencies:** U6 (the emitted line must already be credential-free and dated).
- **Files:**
  - `metadata/evidence/token.yaml` (new — exact path/format finalized at
    implementation; see Open Questions)
  - reads: `crates/ls-sdk/tests/live_smoke.rs` (`live_smoke_default`, `record()`),
    `Makefile` (`live-smoke` target)
- **Approach:** Run `make live-smoke` with `.env` shell-sourced and
  `LS_TRADING_ENV=paper`. Confirm the run is a genuine success (HTTP 200, non-empty
  token) and not a guard miss or quote-contaminated 403. `grep` the `LIVE-SMOKE` line
  out of the smoke stdout (e.g. `make live-smoke | tee` then grep) and store it
  **verbatim** — because U6 already made that line credential-free and dated, verbatim
  capture needs no hand-editing and cannot introduce a secret or fabricate a value.
  The line carries `env=paper`, a non-empty `token_len`, `rsp_cd`, and the run date.
- **Patterns to follow:** the `record()` credential-free contract in
  `crates/ls-sdk/tests/live_smoke.rs:78`; structural-descriptors-only persistence from
  `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md`.
- **Test scenarios:** Test expectation: none (operator-run live smoke + committed data
  artifact; no SDK behavior changes). Verification gate is the live smoke's own pass
  assertion plus a confirm that the verbatim recorded line contains `env=paper`, a
  non-empty `token_len`, the run date, and no `rsp_msg` / credentials.
- **Note (acknowledged weakness):** even with U6, manual capture is weaker than a
  type-enforced, secret-safe-by-construction writer. Auto-writing the record from the
  harness is the real integrity control and is deferred (see Scope Boundaries); until
  it lands, U6 plus the operator discipline above are the guard.
- **Verification:** The evidence file exists, references a real dated paper OAuth
  success, and contains no credentials.

### U2. Promote `token` in metadata

- **Goal:** Mark `token` recommended and anchor its review date.
- **Requirements:** recommended flag; evidence freshness anchor.
- **Dependencies:** U1 (the date must reflect the verified evidence run).
- **Files:** `metadata/trs/token.yaml`
- **Approach:** Set `support.recommended: true` and `maintenance.last_reviewed` to
  U1's run date. No change to `metadata/tr-index.yaml` (no `support` field there) and
  no validator change. `token` already has `implemented: true`, so the state is
  internally consistent.
- **Patterns to follow:** existing per-TR YAML shape in `metadata/trs/*.yaml`.
- **Test scenarios:**
  - Covers the recommended-flag requirement. The `slice_metadata` validator gate
    parses the edited file and passes (presence + routing agreement still hold with
    `recommended: true`).
  - Consistency guard: an integration test under `crates/ls-metadata/tests/` (it
    already owns metadata loading with a `CARGO_MANIFEST_DIR`→`metadata/` helper)
    that raw-parses the evidence file's machine-emitted `date:` field (the dated line
    from U6) and asserts it equals `token`'s `maintenance.last_reviewed` from
    `validate_dir(...)`, so the two cannot silently drift before a schema link
    (`evidence_ref`) exists. Specifiable only once the evidence file's `date:` field is
    pinned (see Open Questions) — until then it is blocked-on-format, not a live gate.
  - Optional regression guard: a test asserting `token`'s `support.recommended` is
    `true` (decide at implementation — see Open Questions).
- **Verification:** the full network-free `cargo test` passes (not `-p ls-metadata`
  alone — the flip is consumed cross-crate by `ls-docgen`); `token.yaml` shows
  `recommended: true` and the new `last_reviewed`.

### U3. Rewrite the freshness policy truthfully

- **Goal:** Make `metadata/EVIDENCE-FRESHNESS.md` and the `metadata/tr-index.yaml`
  header describe the real post-PR5 state.
- **Requirements:** truthful active/inactive description; which findings stale; how
  controls combine; what stays inactive (origin freshness-policy requirements).
- **Dependencies:** none (pairs conceptually with U2).
- **Files:** `metadata/EVIDENCE-FRESHNESS.md`, `metadata/tr-index.yaml` (header
  comment only)
- **Approach:** Retract **both** false claims currently in the doc, not just one:
  (1) the "inactive because the Specification Document Tracker does not exist" framing,
  and (2) the existing assertion — in both `EVIDENCE-FRESHNESS.md` and the
  `tr-index.yaml` header — that "the 90-day backstop is the sole operative freshness
  control," which is itself untrue (no code reads `last_reviewed`). Replace them with
  the truth: the tracker now exists and can see example changes, but (a) change-driven
  evidence invalidation is not wired — no code emits `Severity::Evidence`; (b) the
  90-day backstop is not computed — no code reads `last_reviewed`; (c) the only truly
  operative control today is human review discipline anchored on a manually-set
  `last_reviewed` date. State the *intended* semantics (structural change would
  stale; example findings stay advisory and never gate; description/`korean_name`
  changes are informational; backstop + change-driven invalidation combine
  whichever-first) explicitly as documented intent pending an evaluator. Note that
  with one Recommended TR the policy has a single subject and per-class tightening
  stays deferred.
- **Patterns to follow:** the existing honest "defined but unreachable" prose in
  `crates/ls-trackers/src/types.rs:124` — mirror that candor.
- **Test scenarios:** Test expectation: none (documentation-only; no behavior). 
- **Verification:** Each claim in the doc is checkable against code reality — no
  sentence asserts an enforced behavior that does not exist.

### U4. Regenerate and commit the SDK docs

- **Goal:** Reflect `token`'s recommended status in generated docs and keep the drift
  gate green.
- **Requirements:** the user-facing recommendation statement (origin).
- **Dependencies:** U2 (metadata drives docgen output).
- **Files (generated — do not hand-edit):**
  - `docs/reference/token.md` (the "⚠️ implemented, not yet recommended" banner is
    removed)
  - `docs/reference/index.md` (token status → recommended)
  - `docs/tr-dependencies/token.md` (`Recommended: yes`)
  - `docs/tr-dependencies/index.md` (support column → recommended)
- **Approach:** Run `make docs` to regenerate, commit the output. `make docs-check`
  (the `--check` drift gate) fails until the regenerated files are committed.
- **Patterns to follow:** existing `make docs` / `make docs-check` workflow;
  `crates/ls-docgen` `support_label` and `NOT_RECOMMENDED_BANNER` rendering.
- **Test scenarios:**
  - Covers the user-facing-statement requirement. After `make docs`, `make docs-check`
    passes (the `ls-docgen` determinism gate is satisfied).
  - The not-recommended banner is absent from `docs/reference/token.md`; both index
    pages show token as recommended.
- **Verification:** `make docs-check` is green; the generated docs state token is
  recommended.

### U5. Update the docgen banner test fixture

- **Goal:** Keep `cargo test -p ls-docgen` green after `token` loses its
  not-recommended banner.
- **Requirements:** none new — unblocks the U2 flip's verification.
- **Dependencies:** U2.
- **Files:** `crates/ls-docgen/src/lib.rs` (test
  `reference_covers_six_implemented_with_banner_and_omits_unimplemented`)
- **Approach:** This test reads authored metadata and asserts each of a hardcoded TR
  list (including `token`) carries "Implemented, not yet recommended." Remove `token`
  from the *banner-asserting* set and add a positive assertion that `token`'s reference
  page does *not* carry the banner (mirroring `reference_banner_is_keyed_on_recommended_flag`).
  **Critical:** `token` stays `implemented: true`, so it still renders a reference page —
  the page-count assertion (`reference.len() == implemented.len() + 1`, currently 7)
  must still expect 7, not drop to 6. Keep the count at 7; only the banner expectation
  for `token` changes. No production docgen code changes.
- **Patterns to follow:** the sibling test `reference_banner_is_keyed_on_recommended_flag`.
- **Test scenarios:**
  - The full `cargo test` passes, including `ls-docgen`, with `token` recommended.
  - `token`'s reference page asserts banner-absent; the five still-unrecommended
    implemented TRs still assert banner-present; the reference page count stays 7
    (token still rendered, just banner-free).
- **Verification:** `cargo test -p ls-docgen` (and the full `cargo test`) is green.

---

## Scope Boundaries

### Deferred to Follow-Up Work

- An **evidence-freshness evaluator**: parsing `last_reviewed`, computing the 90-day
  backstop, emitting `Severity::Evidence`, and mapping a spec-doc finding on a
  Recommended TR to evidence-staling. This is what would make the policy literally
  "active"; deferred by decision. Note the two halves differ in cost: a
  `last_reviewed`-only backstop (parse the date, compare to today−90, emit one finding)
  is small and is the single piece that would give the Recommended claim *any*
  automated revocation; the spec-doc-finding-to-staling mapping is heavier. A future
  move may pull the cheap backstop half forward ahead of the rest.
- **Auto-writing** the evidence record from the smoke harness (`record()` → file,
  secret-safe by construction) instead of manual capture.
- A schema `evidence_ref` field on `ls-metadata` linking a TR to its evidence record.
- A regression test asserting recommended status (optional; decide in U2).

### Outside this move's identity

- The second promotion (`t1102`, `revoke`, others) — proves session-dependence later.
- Migration-ownership notice in the old repo, the public orientation README, the
  finding→work-item workflow, and provisional-baseline re-attestation (all deferred in
  origin).
- Any automatic mutation of SDK code, metadata, docs, or baselines by a tracker.

---

## Open Questions (Deferred to Implementation)

- Exact location and format of the durable evidence file (e.g.,
  `metadata/evidence/token.yaml` vs. a `docs/` location) — pick at implementation;
  the binding convention is filename keyed to TR code plus the `last_reviewed` date.
  The format must include a parseable `date:` field carrying U6's machine-emitted run
  date, since the U2 consistency guard asserts against it.
- Whether to add the optional regression test asserting `token.support.recommended`
  in U2, or rely on the existing validator gate.
- Whether to record the upstream premise correction in the origin brainstorm — a
  one-line addendum noting research falsified the "active" requirement (trackers
  shipped, evaluator did not), so its freshness requirement and acceptance examples
  are reclassified as deferred intent. This keeps origin and plan from silently
  conflicting for a future reader. (Decision belongs to the user — see handoff.)

---

## Risks & Dependencies

- **Drift gate breakage.** Promoting in metadata without regenerating docs makes
  `make docs-check` fail. Mitigated by U4 in the same change.
- **False evidence from a quote-contaminated 403.** A `make include`-loaded `.env`
  quotes credentials and yields a 403 that looks like bad creds (see
  `docs/solutions/integration-issues/makefile-include-env-quotes-gateway-403.md`).
  Mitigated by shell-sourcing `.env`, asserting `LS_TRADING_ENV=paper`, and confirming
  a genuine 200 before recording (U1).
- **Secret leakage into a committed artifact.** Mitigated by U6 making the emitted line
  credential-free (no `rsp_msg`) and dated *by construction*, so U1's verbatim capture
  records only structural descriptors, lengths, `rsp_cd`, and the date — never raw
  payloads.
- **Dependency:** the live Paper credentials and gateway must be reachable to produce
  genuine evidence; without them, U1 cannot honestly complete and the promotion should
  not proceed on a guard-miss date.

---

## System-Wide Impact

Generated docs begin asserting `token` is recommended (banner removed, status flips),
visible to any future maintainer reading `docs/reference/`. Tracker and SDK runtime
behavior is unchanged. Blast radius is limited to four generated doc files, one
metadata file, one policy doc, the index header, one new evidence file, and two
test/harness files (the `live_smoke_default` call site and the docgen banner fixture).

**What this promotion does and does not guarantee.** It asserts that a human attested
a genuine Paper OAuth 200 on a dated run and committed credential-free evidence. It
does **not** provide automated staleness detection: the Recommended claim can silently
go stale (an upstream spec change, behavior drift, or `>90` days elapsed) with zero
signal until the deferred evaluator ships. The operative trust anchor is review
discipline, and the doc's "rely without re-verifying" statement is bounded by that.
This is a deliberate, named limitation — promoting the *editing convention* now, ahead
of the *enforcement lifecycle*.

---

## Sources & Research

- Origin requirements: `docs/brainstorms/2026-06-16-first-recommended-tr-token-promotion-requirements.md`
- Freshness model is code vs. intent: `crates/ls-trackers/src/types.rs` (`Severity`
  unreachable tiers), `crates/ls-trackers/src/api_drift.rs` (`is_strong`),
  `crates/ls-trackers/src/spec_doc.rs` (`gates: false`),
  `crates/ls-metadata/src/{schema.rs,validator.rs}`,
  `crates/ls-docgen/src/lib.rs` (`support_label`, `NOT_RECOMMENDED_BANNER`).
- Evidence recording today: `crates/ls-sdk/tests/live_smoke.rs:78` (`record()`,
  stdout-only), `Makefile` (`live-smoke`, `docs`, `docs-check`).
- Learnings: `docs/solutions/integration-issues/makefile-include-env-quotes-gateway-403.md`,
  `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md`.
- Vocabulary: `CONTEXT.md`. Layout/authority: `docs/adr/0011-ls-crate-layout.md`,
  `docs/adr/0012-rust-owned-metadata-schema-authority.md`.
