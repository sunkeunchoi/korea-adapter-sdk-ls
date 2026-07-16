---
title: "Producing a version-labeled code-turn re-baseline run when the lab-research CLI has no version-only-bump path"
date: 2026-07-10
last_updated: 2026-07-16
category: workflow-issues
module: "adapters/nautilus/lab strategy loop — runner/research.rs turn/rerun surface + artifacts/manifest.rs strategy_code_hash; operator data home data/turn4-fresh"
problem_type: workflow_issue
component: development_workflow
applies_when:
  - "Running a strategy-loop CODE turn — one that edits crates/strategy source (adapters/nautilus/lab/src/strategy/orb.rs) and so bumps strategy_code_hash — but changes no swept OrbParams field (or the new param stays at its default)"
  - "You need the run labeled at the next version (e.g. v9) with params otherwise identical to the prior run, over an existing operator data home"
  - "`lab-research turn` refuses: the governed param turn requires a real param change to bump the version, and the rerun path keeps the current version — neither yields a version-only bump, and there is no LS_TURN_FORCE_VERSION"
  - "ARMING a new default-off lever's merit flip (a 0.0 -> X sentinel flip): the native code-bump path re-baselines the version but does NOT arm the lever, and the governed param turn bounds-caps the infinite relative change — so the arming flip still uses seed-and-rerun (see the superseded-scope note)"
severity: medium
related_components:
  - "lab-research CLI (turn / runs compare)"
  - "strategy loop re-baseline governance"
---

> **Superseded (2026-07-16) by the native code-turn path.** `lab-research turn`
> now has a first-class version-only bump: set `LS_TURN_CODE_BUMP=1` (no
> `LS_TURN_PARAM`) and it bumps `strategy_version` by 1 with a zero param diff,
> re-serializing the resolved current params so any newer `#[serde(default)]`
> companion the prior head predates is seeded at its default automatically — no
> hand-seeded manifest, no seed-dir cleanup. `runs compare` gained
> `LS_COMPARE_MODE=code`, which **PASSes** a version-only diff with the expected
> `strategy_code_hash` delta (retiring "no `runs compare` mode PASSes a code
> turn"). The governed one-shot (`turn governed`, plan
> `docs/plans/2026-07-16-002-feat-governed-strategy-turn-command-plan.md`) drives
> bump → re-baseline → 1:1 reconcile → compare in the fresh child. The
> manual seed-and-rerun below remains valid history but should no longer be
> executed by hand **for the re-baseline step**.
>
> **Superseded-scope caveat (2026-07-16): the native path retires the RE-BASELINE
> seed-and-rerun, NOT the ARMING FLIP.** For a *new* default-off lever, arming it is
> two steps: (1) the version-authority re-baseline (`alpha` at its `0.0` sentinel,
> reconciles 1:1 to the prior head) — now `LS_TURN_CODE_BUMP=1`; and (2) the merit flip
> that moves the sentinel off `0.0` (e.g. `liquidity_tilt_alpha 0.0 → 1.0`). Step 2
> **cannot** go through a governed param turn: a `0.0 → X` change is an *infinite*
> relative change, which `ProposalBoundsGuardrail` (cap `0.5`,
> `research.rs::PROPOSAL_BOUNDS_CAP`, not env-configurable for a turn) fail-closes — the
> same reason the ratio-ATR/breakeven-trail arming flips used seed-and-rerun. So the
> arming flip **still uses the manifest seed-and-rerun below** (seed a `vN+1` manifest
> with the flipped `alpha`, rerun, remove the seed). Observed on the 2026-07-16 Amihud
> liquidity turn: v31 re-baseline via `turn governed LS_TURN_CODE_BUMP=1`, then the
> `liquidity_tilt_alpha 0→1` flip via seed-and-rerun → v32.
>
> **Companion corollary:** for the arming flip to size correctly, the lever's frozen
> non-default companions (a clamp band / reference) must be present in the re-baseline
> manifest. The native code-bump seeds newer `#[serde(default)]` fields at their *serde
> default*, so a lever whose companions must be non-default must declare them via
> `#[serde(default = "fn")]` returning the frozen values (the `default_liquidity_tilt_*`
> pattern, `params.rs`) — otherwise the re-baseline carries `0.0` companions and the flip
> either fails `validate()` or sizes against a degenerate band.

## Context

The `adapters/nautilus/lab` strategy loop advances by **turns**, each judged against the
prior run. The `lab-research turn` CLI produces runs two ways
(`adapters/nautilus/lab/src/runner/research.rs`):

- **Governed param turn** (`LS_TURN_PARAM` set) — applies exactly one `OrbParams`
  change, bumps `strategy_version` by 1, and refuses unless the manifest diff is
  exactly `{the param, strategy_version}` (research.rs ~373-389).
- **Rerun** (no `LS_TURN_PARAM`) — reruns with the current params and **no version
  bump** (research.rs ~269-283).

A **code turn** — editing `orb.rs` (which moves `strategy_code_hash`) while changing
no swept param — fits neither. You want the next version label (v9) with params
identical to the prior run (v8). The governed path won't run without a param change;
the rerun path won't bump the version. There is no force-version env var.

This is the recurring shape for every exit-geometry / entry-logic turn (turn 5 → v6,
turn 8 → v9). The mechanic below is how to get a clean version-labeled run with the
new code hash over the existing catalog, fully offline.

## Guidance

**Seed a version-authority manifest, then rerun.** `latest_finalized_run` reads only
each run's `manifest.json` (research.rs:101-106 → `ordered_runs().last()` →
`read_manifest`), and `list_runs` accepts any run dir not prefixed `.tmp-`
(`adapters/nautilus/lab/src/artifacts/mod.rs:210`). So a manifest-only dir is a valid
params/version authority for the next rerun.

1. **Seed.** Copy the latest finalized run's `manifest.json` into a new run dir named
   `<later-timestamp>Z-backtest-orb-v<N>/`. Set `strategy_version` and
   `params.strategy_version` to `N`, and add any newly-introduced param at its default
   (e.g. `profit_target_r: 1.0`). No `performance.json` / `decisions.jsonl` needed.

2. **Order it last.** `run_order_key` sorts by `(timestamp-head, version)`
   (research.rs:84-89), so date the seed *after* the prior run (a 2026-07-10 stamp
   outranks a 2026-07-09 run) — otherwise `latest_finalized_run` won't pick it.

3. **Rerun.** Run `lab-research turn` with **no** `LS_TURN_PARAM`. Rerun mode reads the
   seed as the current authority (version `N`, new param at default), pins the seed's
   `data_range`, and runs the backtest with the compiled-in new code → a real
   `<now>Z-backtest-orb-v<N>` run whose manifest carries the **new** `strategy_code_hash`.

4. **Remove the seed dir.** The real run (later timestamp) outranks it; delete the seed
   for registry hygiene. The data home is gitignored, so nothing hits git either way.

5. **Capture the re-baseline signal.** `runs compare` param-mode `v(N-1) → vN` **FAILs**
   — `FAIL: strategy_code_hash differs` (research.rs ~571-574), plus
   `param diff must be exactly {strategy_version, one param}` when the new param's default
   equals the run value (so `param_diff` shows only `["strategy_version"]`). That FAIL
   *is* the re-baseline evidence — capture it; no `runs compare` mode PASSes a code turn.

   **Multi-param caveat (prior head predates the new companion params).** The clean
   `["strategy_version"]` diff only holds when every field the seed sets already matches the
   prior run's *resolved* params. A lever that ships several `#[serde(default)]` companions
   (a ratio-ATR tilt's `ratio_atr_ref`/`w_lo`/`w_hi` alongside the flip param
   `ratio_atr_alpha`; a clamp band alongside a strength param; …) breaks that when the prior
   head **predates** those fields: the old manifest has no key for them, so `runs compare`
   resolves them to the struct default (`0.0`), while the re-baseline seed carries their
   frozen non-default values. The re-baseline diff then lists the companions too, e.g.
   `param diff: ["ratio_atr_ref", "ratio_atr_w_hi", "ratio_atr_w_lo", "strategy_version"]` —
   **still FAIL-on-`strategy_code_hash`, which is the real re-baseline evidence**; the extra
   entries are cosmetic (they were `0.0`-by-absence before, frozen-value-by-presence now).
   Seed the companions at their final frozen values in the re-baseline anyway (per KTD-1:
   keep them visible in every run manifest) — then the **flip** compare `vN → vN+1` *is* the
   clean one-param diff, because both runs share those companion values and differ only in the
   flip param: `param diff: ["ratio_atr_alpha", "strategy_version"]`, `verdict: PASS`. Observed
   on the 2026-07-15 ratio-ATR turn: v26 (pre-tilt) → v29 re-baseline diff carried the three
   `ratio_atr_*` clamp/ref fields; v29 → v30 flip diff was exactly the two expected keys.

## Why This Matters

`strategy_code_hash = sha256(include_str!("orb.rs"))`
(`adapters/nautilus/lab/src/artifacts/manifest.rs:97-99`,
`adapters/nautilus/lab/src/strategy/mod.rs:10` `ORB_SOURCE`). It fingerprints **only
`orb.rs`** — runner or param edits do not move it (that's why turn 5's runner rewrite
kept v5's hash). A code turn is therefore judged on the **edge bar** (expectancy /
dominance), not on a green compare: the compare *is expected to fail*, and treating that
FAIL as a stop instead of the signal would block every code turn.

Doing this via seed+rerun (rather than a governed turn to a non-default param value)
keeps the code change as the turn's single change — the new param stays at its provisional
default, and its sweep is deferred to a later governed param turn.

## When to Apply

Any strategy-loop code turn (edits `orb.rs`) that needs a version-labeled re-baseline run
over an existing data home, where the change touches no swept param (or the new param
holds its default). Not needed for a governed single-param turn — that bumps the version
through `LS_TURN_PARAM` normally.

## Examples

Turn 8 (v8 → v9, fixed profit target; `profit_target_r` default 1.0):

```bash
# Data home lives at REPO ROOT: /data (gitignored), NOT under adapters/nautilus/.
DH=$PWD/data/turn4-fresh

# 1. Seed a v9 params-authority manifest from the v8 run (Python: copy + edit).
python3 - <<'PY'
import json, os
v8 = json.load(open(f'{DH}/runs/<v8-run-id>/manifest.json'))
seed = dict(v8, run_id='20260710T000000Z-backtest-orb-v9', strategy_version=9)
seed['params'] = dict(v8['params'], strategy_version=9, profit_target_r=1.0)
os.makedirs(f'{DH}/runs/20260710T000000Z-backtest-orb-v9', exist_ok=True)
json.dump(seed, open(f'{DH}/runs/20260710T000000Z-backtest-orb-v9/manifest.json','w'), indent=2)
PY

# 2. Rerun (no LS_TURN_PARAM) — reads the seed as authority, runs the new code.
LS_DATA_HOME=$DH ./adapters/nautilus/target/release/lab-research turn
# -> "rerun: current params (strategy v9) ... finalized run 20260710T013757Z-backtest-orb-v9"

# 3. Remove the seed; the real run outranks it.
rm -rf $DH/runs/20260710T000000Z-backtest-orb-v9

# 4. Capture the re-baseline FAIL (the intended signal, exit 1).
LS_DATA_HOME=$DH LS_COMPARE_A=<v8-run-id> LS_COMPARE_B=20260710T013757Z-backtest-orb-v9 \
  LS_COMPARE_MODE=param ./adapters/nautilus/target/release/lab-research runs compare
# param diff: ["strategy_version"]
# FAIL: strategy_code_hash differs
# verdict: FAIL
```

**Shortcut for a follow-up code turn on the same version:** if the latest finalized run
is *already* at version `N` (e.g. you re-run vN after applying a code-review fix), you can
skip the seed entirely — a plain rerun reads that run as authority and reproduces vN with
the new hash. Confirm the edit is behavior-neutral by checking the summary is byte-identical
where expected. Rebuild the release binary from `adapters/nautilus/lab` first
(`cargo build --release -p nautilus-ls-lab --bin lab-research`) — building from repo root
fails with `package ID specification nautilus-ls-lab did not match any packages`, and a
stale binary silently produces a run with the old hash.
