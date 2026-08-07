# Sample margin — the pre-registered bar for a future ORB head

**Frozen 2026-08-06.** Machine-readable record: [`sample-margin.json`](sample-margin.json).
Derived by plan `docs/plans/2026-08-05-001-feat-orb-power-and-data-turn-plan.md` (U3; R6, KTD2/KTD3).

This is **not** the ladder pre-registration. `preregistration.json` is untouched and stays
byte-identical (KTD3) — see [§ Why not the ladder pre-registration](#why-not-the-ladder-pre-registration).

The three arithmetic traps behind this record's own figures — the calendar-versus-productive
period denominator, the paired standard error an absolute-detectability verdict does not
answer, and why cutting cluster size *raises* the session requirement — are written up as a
transferable rule in
[`docs/solutions/conventions/power-questions-three-traps-calendar-denominator-paired-se-and-cluster-size.md`](../../../../docs/solutions/conventions/power-questions-three-traps-calendar-denominator-paired-se-and-cluster-size.md).

---

## The rule

A candidate ORB head clears the sample margin **iff**

```
net RoR  >  E[max of N null trials]  +  z(confidence) · SE(candidate)
```

where

```
E[max]  =  σ_trials · [ (1 − γ)·Φ⁻¹(1 − 1/N)  +  γ·Φ⁻¹(1 − 1/(N·e)) ]     (N ≥ 2)
E[max]  =  0                                                              (N = 1)
```

- `net RoR` — the candidate's `Σ realized / Σ risk capital`, **cost-aware** (KTD4).
- `SE(candidate)` — the candidate's **own** session-block bootstrap standard error,
  block = one KST session (Q1).
- `γ` — Euler–Mascheroni, 0.5772156649015329.
- `Φ⁻¹` — the inverse standard-normal CDF.
- `N`, `σ_trials`, `confidence` — **frozen** below.

The closed form is Bailey & López de Prado's False Strategy Theorem: the expected maximum
of `N` null trials. `trials_corrected_threshold` in `src/stats.rs` is the implementation;
`margin::SampleMargin::threshold` delegates to it, so the frozen record and the statistics
core cannot disagree about the rule. `report sample` is the carrier that prints the verdict.

### Why a rule and not a level

A scalar threshold scaled at the head's own 111 trades is **unclearable at any sample size** —
it never moves as evidence accumulates, so it strands a viable strategy permanently. What is
frozen here is therefore the rule and its two *selection-bias* inputs. `N` and `σ_trials`
describe the search already spent, which more data cannot un-spend, so they do not move.
`SE(candidate)` is evaluated at judge time and shrinks as `√n` grows, so the bar is reachable.

Read the two terms separately:

- `E[max] = 0.0543` net RoR is the **selection tax** — the edge a coin-flip head is expected
  to show simply because 29 arms were tried against this sample. It is a floor no amount of
  additional data removes.
- `z · SE` is the **sampling term**. At the v35 head's SE of ≈0.087 it contributes ≈0.171;
  at four times the sample it would contribute ≈0.085.

### Why the bar is not a significance test

The queue's superseded unblock condition was the literal `net RoR > 0`. At this sample
`P(net RoR > 0)` under the null is ≈0.50 — a coin-flip head satisfies it half the time.
Replacing it with "the interval excludes zero" would still ignore the 29 arms already
evaluated against this data. Correcting for those arms is what makes the bar unfakeable.

---

## The frozen inputs

| Input | Value | Source |
|---|---|---|
| `confidence` | 0.95 (two-sided) | KTD11, pinned before any reading |
| `power` | 0.80 | KTD11 (carried for provenance; the threshold itself is a confidence statement) |
| `trial_count` `N` | 29 | `trials::count_trials` over `ledger/trials.jsonl` |
| `cross_trial_sd` `σ_trials` | 0.026367936878680807 | sample sd of the seven per-arm net RoR figures below |
| `expected_max_null` | 0.05430159509024828 | the closed form at those two |

Both derived numbers are re-derived from their inputs by
`tests/sample_margin.rs`, so the frozen values are **auditable, not typed in**.

### The trial count

`N = 29` is every record in the committed trials ledger at freeze time.

KTD2 asks for the count *scoped to the v35 catalog lineage*. **That scoping is not available
against the committed ledger.** v35's catalog fingerprint (`ac026541…`) appears in no ledger
record, and no record declares a parent link into it; the ledger's three lineage roots are
`era-167` (19 records), `3b6be31b` (8) and `363f199d` (2). Taking the 2-record `363f199d`
slice would set `E[max]` at 0.0137 instead of 0.0543 — its lowest available value — and
"an undercounted trial count sets the bar too low" is a named risk of the plan that
introduced it. The whole-ledger count is the strict reading and is what is frozen.

The plan's own assumption applies here too: the correlation-based effective-`N` reduction the
literature permits is **not** attempted. Declining it errs toward a stricter bar.

### The cross-trial dispersion

The seven per-arm figures are the single-param off-flip table recorded in
`TURN-LOG.md` under *Turn — transaction-cost model (2026-07-31)*. They are the only
same-catalog, same-cost-model, net-RoR-denominated arm set in the tree — every other
recorded sweep predates the cost model, so its RoR figures are gross and not comparable.

| Arm | net RoR |
|---|---|
| v35 baseline (all six levers ON) | −0.0006 |
| `entry_confirm` 1.0→0.0 | −0.0325 |
| `or_width_max_atr` 0.666→0.0 | −0.0243 |
| `breakeven_trigger_r` 0.41→0.0 | −0.0817 |
| `risk_per_trade_krw` 299,340→0.0 | −0.0479 |
| `ratio_atr_alpha` 1.0→0.0 | −0.0275 |
| `gap_retention_min` 0.5→1.0 (OFF) | −0.0591 |

Sample sd = **0.026367936878680807**.

### Justifying the value

The additive-stream floor precedent: anchor below the smallest historically-kept gain and
above the noise the screen itself produces.

- **Above the noise.** The v35 head's own session-block bootstrap puts the null spread of
  net RoR at roughly ±0.17 at 95%. `E[max]` at 0.0543 sits well inside that, so the
  selection tax alone is *not* the binding term at this sample — `z·SE` is. The margin is
  not a bar invented to be unreachable; it is dominated by ordinary sampling error until
  the sample grows.
- **Below the smallest kept gain.** The kept levers moved net RoR by 0.0237 to 0.0811 each
  (the off-flip deltas above). A head whose whole edge is smaller than one kept lever's
  contribution is not a head. `E[max]` at 0.0543 sits inside that range, so the bar
  discriminates among plausible heads rather than excluding all of them.

---

## Provenance and the re-derivation trigger

The dispersion inputs were read at:

| | |
|---|---|
| run | `20260731T023138Z-backtest-orb-v35` |
| catalog fingerprint | `ac0265415d79a0917239dd3749c92332bd06bb887de1c09d0d2985071219109e` |
| closed-trade session span | 20260521..20260722 (24 KST sessions, 111 closed trades) |
| per-trade net r | mean −0.033320, sd 0.641523 |
| ICC / Kish cluster size / design effect | 0.327334 / 4.5374 / 2.1579 |

> **A candidate head judged on a catalog whose fingerprint differs from the one above must
> have this margin re-derived before it binds.**

Catalog reproducibility is not assumed here: the fingerprint already moved `363f199d` →
`ac026541` as morning-chain ingests backfilled history, and v34's 119 trades re-measured as
111 on identical code and params. In-range content growth changes the trade set, so both the
per-arm figures and the dispersion are catalog-specific (AE3). `report sample` prints
`RE-DERIVATION REQUIRED` rather than adjudicating silently when the fingerprints disagree.

## Falsification — measured, not asserted

The margin is not a document claim. `tests/sample_margin.rs § calibration` builds null
replicates from the committed v35 distribution (`tests/fixtures/v35-closed-trades.json`;
`data/` is gitignored) and measures the rate at which a max-of-29 null block clears the bar:

| | |
|---|---|
| realized null clearance | **0.0140** |
| nominal | 0.0250 |
| threshold at the head's own SE (0.087002) | +0.224823 net RoR |
| a bar set at 2·SE (+0.174004) instead | clears at **0.1060** — 4.2× nominal |

That last row is the point. KTD10's concern is that a single permuted-label refusal is
satisfied by *any* bar above roughly two standard errors, including one set far too low.
Measuring the 2·SE bar directly shows this calibration discriminates: it fails, and the
frozen bar passes.

The null is built by **permuting the centred per-trade R-multiples across trades** (so the
true edge is exactly zero while cluster sizes, session structure and the risk-capital total
are untouched) and then drawing one session-block resample. It has to be the R-multiple that
moves: `Σnum/Σden` is exactly invariant under a permutation of the *numerators*, so a null
built that way would have zero dispersion and any bar would clear it vacuously.

The suite also asserts the v35 head is refused, that a synthetic head with a real edge and
~36× the sample clears while the *same* edge on the thin sample does not, and — via the
`MarginArm` seam — that **disarming the comparison in-process reds the null-rate assertion**
(observed: 1.0000 against a 0.0250 nominal). That is a standing falsifier, not a one-time
edit-and-restore.

## Why not the ladder pre-registration

`config/preregistration.json` is the frozen rung-1 ladder pre-registration. The amendment
protocol's no-consumer test
(`docs/solutions/conventions/suspend-vs-amend-frozen-governance-artifacts.md`) says not to
re-derive a frozen artifact when the honest value would forbid the activity it gates. The
ladder is stood down; folding a sample margin into its bands would be exactly that. It stays
byte-identical, and `tests/sample_margin.rs` pins its SHA-256 so a later edit cannot slip in
under this turn's cover.

## Why not `candidates/`

`candidates::load` bails on a candidate declaring neither a flip param nor a sweep-leg set,
and `diagnose` short-circuits a `minimal` Phase-A candidate to an immediate GO before
thresholds are evaluated — so a margin filed there would never be enforced by the in-tree
evaluator. Extending that schema means a version bump that invalidates the seven committed
packages, for a record that is not a candidate. The margin gets its own home instead.
