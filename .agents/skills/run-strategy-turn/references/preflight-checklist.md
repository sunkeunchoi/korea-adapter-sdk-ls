# run-strategy-turn — preflight checklist

An **incident-traceable** checklist: each line exists because a specific gotcha or
solution doc created it. Confirm every line before running the governed turn; a
line you cannot confirm is a **STOP**, not a proceed.

**Standing rule (R14):** any new `docs/solutions/workflow-issues/*` doc about a
strategy-loop workflow issue must add a line here in the **same PR** that
documents it. The checklist is the loop's incident memory; a new gotcha that
ships without a line here silently decays the guard.

## Checklist

1. **Build from the standalone workspace, never repo root.** Run all `cargo`
   commands from `adapters/nautilus` (or with `-p nautilus-ls-lab`). Building
   `nautilus-ls-lab` from the repo root fails (`package ID specification … did not
   match`), and a stale binary silently backtests old code.
   — *cites:* `docs/solutions/workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md` (step 5 shortcut / stale-binary trap).

2. **Trust the binary-fingerprint gate, don't bypass it.** The governed command
   rebuilds foreground and verifies the built binary's embedded
   `LAB_SRC_FINGERPRINT` against the recomputed `lab/src/**` tree hash before any
   flip; a `HELD` StaleBinary means the binary is stale — rebuild, never re-run
   the flip by hand. This deletes the stale-binary gotcha that bit twice
   (2026-07-12, 2026-07-15) — the second time through *params* code, which the
   `strategy_code_hash` alone did not cover.
   — *cites:* the same stale-binary incidents + this plan's KTD5 (full-tree fingerprint).

3. **Use the native code-turn bump, not the manual seed-and-rerun.** For a code
   turn (changed `orb.rs`, no swept param) set `LS_TURN_CODE_BUMP=1` — do **not**
   hand-seed a version-authority manifest into `runs/`. The native path subsumes
   that workaround (and its registry-hygiene cleanup step).
   — *cites:* `docs/solutions/workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md`.

4. **Companion-field seeding is automatic — confirm it landed.** A code bump
   re-serializes the resolved current params, so any newer `#[serde(default)]`
   companion the prior head predates is seeded at its default in the bumped
   manifest. Confirm the bumped manifest carries the expected companion values
   (the manual "seed the companions at their frozen values" step is no longer
   needed).
   — *cites:* the multi-param caveat in `docs/solutions/workflow-issues/code-turn-rebaseline-run-via-manifest-seed-and-rerun.md`.

5. **Read `runs/` before archiving legs — never after.** A two-sided sweep is a
   FAN-OUT: archive each losing leg out of `runs/` only *after* the compare/analyze
   step reads it, or `latest_finalized_run` resolves the wrong head. The command
   records each leg as a sweep-leg trial; do not archive until the ledger + compare
   have read the run.
   — *cites:* `docs/solutions/conventions/strategy-loop-param-turn-governance-and-fresh-home-seeding.md` (fan-out / fresh-home seeding discipline); TURN-LOG fan-out entries.

6. **Diagnostics ride `uv run --with pyarrow` — pyarrow is absent locally.** A
   bespoke Phase-A diagnostic/twin reads the closed-trade parquet via
   `uv run --with pyarrow python3 …`; a bare `python3` fails on the missing
   pyarrow. Declare the argv with `uv` in the candidate; the wrapper is
   interpreter-agnostic and appends the readings output path.
   — *cites:* `adapters/nautilus/lab/candidates/README.md` (diagnostic contract); the Phase-A gate in `docs/solutions/conventions/pre-code-collinearity-gate-before-a-second-normalizer-lever.md`.

7. **Never re-derive the verdict — echo the decider's.** The governed run's last
   line is the fresh child's verdict; every layer above it (skill, operator) is
   transport. Do not paraphrase, re-rank, or re-compute KEEP/REVERT.
   — *cites:* `docs/solutions/conventions/report-preview-governance-band-must-anchor-on-deciders-run.md` (anchor-on-decider).

8. **Do not edit a frozen pre-register after its GO.** Editing `candidate.json` /
   the diagnostic / the twin after a GO changes the content hash; the flip refuses
   with `PreRegisterHashMismatch` (exit 21). Re-registering a softened clone is a
   *disclosed* event (the gate verdict embeds the prior trials) — never an
   invisible one.
   — *cites:* this plan's R5 / the forbidden-overfit rule in `docs/solutions/conventions/pre-code-collinearity-gate-before-a-second-normalizer-lever.md`.
