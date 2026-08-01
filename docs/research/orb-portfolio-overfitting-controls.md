# Defensible controls for ORB portfolio-search overfitting

**Research date:** 2026-08-01  
**Question:** What statistical controls can keep a search over many ORB variants,
filters, reference-instrument signals, allocations, and risk overlays from
certifying data-mined noise?

## Conclusion

No single statistic can certify this portfolio search. A defensible process needs
four separations that are easy to blur:

1. **search versus assessment** — every data-dependent choice belongs inside the
   search procedure, while assessment occurs on observations that did not inform
   that choice;
2. **a sleeve versus the capital-bearing bundle** — the object ultimately tested
   is the complete Portfolio Head, including membership, allocation, shared risk,
   execution, and cost rules;
3. **selection-bias evidence versus implementation evidence** — multiple-testing
   corrections, PBO, and deflated Sharpe address search bias, but not causal data
   leakage, survivorship bias, or unrealistic fills; and
4. **historical pseudo-out-of-sample evidence versus an untouched confirmation** —
   rolling tests support development, but a result repeatedly consulted during
   development is not a final holdout.

The recommended stack is therefore:

- register the search family, economic hurdle, benchmark, loss function, temporal
  splits, cost stresses, and decision rule before the relevant results are seen;
- run the *whole research-and-allocation procedure* through nested chronological
  walk-forward validation, with point-in-time inputs and interval-aware purging;
- retain every attempted configuration in a trial ledger and apply
  dependence-aware family-level inference, supported by PBO and deflated/probabilistic
  Sharpe diagnostics;
- test predeclared regime, sub-universe, parameter, cost, and capacity sensitivity;
  and
- freeze one complete Portfolio Head and evaluate it once on a sealed chronological
  holdout or, preferably, newly arriving paper-shadow sessions.

The literature does **not** supply universal values for a minimum trade count,
PBO ceiling, Sharpe threshold, holdout length, embargo percentage, or acceptable
regime dispersion. Those are governance choices. They are defensible only when
derived from the smallest economically useful net edge, the dependence and tail
properties of this portfolio's returns, and a precommitted error/power budget.

## Why ordinary backtest hygiene is insufficient

Selecting the maximum of noisy performance estimates overfits the *selection
criterion*, even when every individual model fit is regularized. Cawley and
Talbot show that this second-level overfitting can be comparable to the reported
differences between algorithms and makes evaluation on the model-selection data
optimistically biased ([Cawley and Talbot, 2010](https://www.jmlr.org/papers/v11/cawley10a.html)).
In finance, White frames repeated reuse of the same history as data snooping and
tests whether the best encountered rule actually beats a benchmark
([White, 2000](https://doi.org/10.1111/1468-0262.00152)). A portfolio optimizer
adds another search layer: Bailey, Borwein, and López de Prado demonstrate that
in-sample portfolio design can reproduce a desired return profile yet behave
erratically out of sample
([Bailey, Borwein, and López de Prado, 2016](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=2739335)).

Consequently, “we used cross-validation” is not enough. If filters, ORB windows,
reference ETF/ETN mappings, exits, sleeve membership, or portfolio weights were
chosen after viewing cross-validation results, the cross-validation result is
development evidence. The performance estimate must wrap that entire adaptive
choice in an outer assessment loop, or come from an untouched holdout.

## The operational control stack

### 1. Register the experiment and account for the complete search

Before a search generation starts, freeze a machine-readable manifest containing:

- the economic claim and smallest worthwhile **net** effect;
- the benchmark portfolios and primary loss/utility statistic;
- discovery, inner-validation, outer walk-forward, and final-holdout dates;
- point-in-time universe and reference-instrument construction rules;
- all candidate ORB definitions, filters, exits, execution policies, sleeve sets,
  allocators, constraints, and risk overlays;
- the production refit/rebalance cadence to be simulated;
- the base cost model, adverse cost cases, and capacity assumptions;
- predeclared regime and sub-universe diagnostics;
- the statistical error and power policy; and
- the exact promotion rule, including tie-breakers and catastrophic vetoes.

Maintain an append-only trial ledger for **every** evaluated choice: successful or
discarded variants, thresholds, symbol screens, reference features, data-cleaning
choices, cost models, random seeds, allocations, and human-directed retries. Store
configuration, code, and data-snapshot hashes. The family is the effective set of
questions asked of the history, not merely the charts retained for a report.
White's Reality Check is defined for the set of models encountered in a
specification search; omitting failed or informal trials understates the search
burden ([White, 2000](https://doi.org/10.1111/1468-0262.00152)). DSR likewise
exists because ignoring the number of trials inflates the reported Sharpe
([Bailey and López de Prado, 2014](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=2460551)).

Treat a material expansion of the grid after results are seen as a new search
generation. The prior outer results then become development data and cannot later
be relabelled “out of sample.”

### 2. Split by session and simulate time, not shuffled rows

Use trading **session date** as the atomic split key. Keep every stock, all
simultaneous sleeve signals, and all ETF/ETN reference observations for a session
on the same side of a split. Random trade- or symbol-row splits leak common market
shocks and exaggerate sample size.

Use nested rolling-origin evaluation:

1. At each outer origin, expose only the point-in-time history then available.
2. Inside that history, select data rules, features, filters, parameters, sleeves,
   allocations, and shared controls using chronological inner folds.
3. Refit exactly as production would, then simulate the next non-overlapping outer
   test block without alteration.
4. Advance the origin and repeat; concatenate the unique outer-test portfolio P&L
   into one pseudo-live series.

Rolling-origin tests with coefficient recalibration and multiple test periods are
more reliable than a single fixed-origin split, and heterogeneous series groups
improve the generalizability of forecast comparisons
([Tashman, 2000](https://doi.org/10.1016/S0169-2070(00)00065-0)). The choices among
expanding versus fixed-length training windows, refit frequency, and fold length
are themselves hyperparameters: either lock them from operational requirements or
select them inside the inner loop.

All pipeline state must be fit inside each training window: universe membership,
corporate-action treatment, liquidity screens, scalers, imputers, regime labels,
reference mappings, covariance estimates, and portfolio weights. The test block
must use only information whose publication timestamp precedes the simulated
decision. This temporal design measures the strategy that could have been run; it
does not repair a point-in-time data error upstream.

### 3. Purge overlaps and justify any embargo from information intervals

For every candidate observation, record its full information interval: earliest
input timestamp, decision timestamp, and final label/position-close timestamp.
Remove a training observation whenever that interval overlaps a test interval.
Where training can contain observations after a test block, embargo enough of the
post-test training region that backward-looking inputs and delayed publications no
longer contain test-period information. López de Prado's purged cross-validation
was introduced for labels that span intervals, with an embargo when purging alone
does not remove leakage
([*Advances in Financial Machine Learning*, ch. 7](https://www.wiley-vch.de/de/fachgebiete/finanzen-wirtschaft-recht/advances-in-financial-machine-learning-978-1-119-48208-6)).

The gap is not a ceremonial percentage. Derive it from the maximum relevant
feature lookback, forward label/holding horizon, delayed-data availability, and
state carried across sessions; assert mechanically that no training information
interval intersects the test interval. For a strictly intraday, flat-at-close ORB
label, the forward-label purge may be zero across adjacent sessions, while a
stateful reference feature or delayed constituent file can still require a gap.
Report both the derivation and the number of rows removed.

Purging is a leakage control, not an independence guarantee. Serial dependence
remaining in the portfolio return series must still enter its standard errors and
resampling scheme.

### 4. Separate discovery multiplicity from certification multiplicity

Use exploratory false-discovery control only to screen a broad research family.
Benjamini and Hochberg's original procedure controls the expected false-discovery
proportion under its dependence conditions, which is a different promise from
controlling the chance of *any* false promotion
([Benjamini and Hochberg, 1995](https://doi.org/10.1111/j.2517-6161.1995.tb02031.x)).
Highly correlated ORB variants need a dependence-valid procedure rather than an
automatic application of basic BH.

For a capital-bearing winner or a small set of claimed sleeves, use the stricter
family-wise question: does any member of the declared family beat the locked
benchmark on the primary net loss/utility measure? Suitable primary approaches
include:

- **White's Reality Check**, which tests whether the best model in the searched
  family has predictive superiority over a benchmark
  ([White, 2000](https://doi.org/10.1111/1468-0262.00152));
- **Hansen's Superior Predictive Ability (SPA) test**, which studentizes the loss
  differential and is less sensitive than the Reality Check to many poor or
  irrelevant alternatives
  ([Hansen, 2005](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=264569)); or
- **Romano–Wolf stepdown testing**, which asymptotically controls family-wise
  error while using the joint dependence of the statistics for power
  ([Romano and Wolf, 2005](https://doi.org/10.1111/j.1468-0262.2005.00615.x)).

Run these on session-level net portfolio loss differentials with a stationary or
other justified block bootstrap. The stationary bootstrap was designed to build
confidence regions for weakly dependent stationary observations
([Politis and Romano, 1994](https://doi.org/10.1080/01621459.1994.10476870)).
Document block-length selection and repeat across a plausible block-length range.
Too-short blocks destroy dependence; too-long blocks leave little effective
information.

These tests answer a benchmark-relative family question. They do not validate the
fill engine, guarantee stationarity, or prove that an economic mechanism will
persist.

### 5. Use PBO and adjusted Sharpe as complementary diagnostics

**Probability of Backtest Overfitting (PBO).** Build a synchronized matrix of
session returns for all searched configurations and use combinatorially symmetric
cross-validation to measure how often the in-sample winner ranks below the
out-of-sample median. Report the PBO and the distribution of out-of-sample rank
degradation for the full search, including complete Portfolio Head configurations
([Bailey et al., 2015](https://papers.ssrn.com/sol3/Papers.cfm?abstract_id=2326253)).
PBO directly diagnoses selection instability but has no literature-mandated pass
threshold. It is also not a chronological production simulation; its recombined
partitions can be optimistic under structural change. Use it beside, not instead
of, rolling-origin evidence and the final holdout.

**Deflated and probabilistic Sharpe.** Report the Deflated Sharpe Ratio (DSR) for
the selected portfolio using the complete trial count/effective search, variation
among tried Sharpe ratios, track-record length, skew, and kurtosis. DSR corrects
for selection bias and non-normality
([Bailey and López de Prado, 2014](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=2460551)).
Also compute Probabilistic Sharpe Ratio and Minimum Track Record Length against an
economically meaningful **net** benchmark rather than zero
([Bailey and López de Prado, 2012](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=1821643)).

Do not let annualized Sharpe silently assume independent returns. Lo shows that
serial correlation changes Sharpe inference and can make the usual square-root
annualization materially wrong
([Lo, 2002](https://doi.org/10.2469/faj.v58.n4.2453)). DSR/PSR moment estimates
are unstable in short, fat-tailed samples and do not fully solve dependence.
Treat a dependence-robust block-bootstrap interval for the net portfolio result as
primary and DSR, PSR/MinTRL, and PBO as mutually informative diagnostics—not as
three independent votes or p-values.

### 6. Determine sample sufficiency from the claim, not trade-count folklore

There is no defensible rule such as “100 trades per parameter.” Trades on the same
session share the opening shock, regime, liquidity, and portfolio risk budget; the
effective sample can be far closer to the number of independent session blocks
than to the number of fills.

Before seeing confirmation results:

1. state the smallest economically worthwhile incremental net return or utility
   versus the benchmark;
2. state the one-sided false-promotion budget and desired probability of detecting
   that effect;
3. estimate the null and alternative distributions with simulations or block
   resampling that preserve serial and cross-sectional dependence, volatility
   clustering, skew, and tails; and
4. solve for the outer-test and final-holdout history needed for the preregistered
   test and confidence-bound rule.

Use MinTRL as a supporting Sharpe-based calculation, not a substitute for this
portfolio-specific power analysis. Report effective session blocks, unique
sessions, trades, symbols, and regime coverage. If the required sample extends
beyond available point-in-time history, the honest outcome is “not yet powered”
and a longer prospective paper-shadow period—not a relaxed threshold.

Failure to reject a subgroup difference does not demonstrate stability when a
subgroup is underpowered. Always report confidence intervals and support counts.

### 7. Stress predeclared regimes, sub-universes, and reference dependencies

Define stability cells before the relevant results are viewed, using information
available before each session. For this project the useful dimensions include:

- KOSPI versus KOSDAQ and point-in-time listing cohorts;
- pre-session liquidity, size, price, and volatility bands;
- opening gap, market trend, volatility, and stress states;
- calendar eras and event periods; and
- the state of the non-traded ETF/ETN reference instruments.

For each cell, report exposure, unique sessions, trades, net contribution,
uncertainty, drawdown, and exit/fill quality. Add leave-one-era-out and
leave-one-sub-universe-out reruns. Decompose whether aggregate profit is dominated
by a few symbols, sessions, sleeves, or one regime. Hou, Xue, and Zhang's uniform
replication of 452 equity anomalies shows why universe construction matters:
small stocks and changed weighting procedures erase many apparent effects
([Hou, Xue, and Zhang, 2020](https://doi.org/10.1093/rfs/hhy131)).

ETF/ETN references are inputs, not traded diversifiers. Their mapping, lag,
availability, and transformation choices belong in the search ledger and each
temporal fold. Run locked ablations and lag/perturbation tests to show whether the
portfolio depends on one fragile reference. A reference ablation discovered after
looking at outcomes is another tested specification.

Do not demand positive profit in every small cell: that rewards post-hoc merging
and is statistically unrealistic. Instead predefine material cells and
concentration limits, require enough powered coverage for the claimed deployment
domain, and explain adverse cells. Post-hoc slices are exploratory and join the
next generation's multiplicity family. Specification-curve analysis provides a
general primary-source model for exposing the results of all reasonable analytical
specifications rather than highlighting one convenient path
([Simonsohn, Simmons, and Nelson, 2020](https://doi.org/10.1038/s41562-020-0912-z)).

### 8. Stress parameters, costs, execution, and capacity without re-optimizing

Evaluate a predeclared neighborhood around every continuous threshold and the
reasonable alternatives for categorical decisions. Report the whole response
surface, downside quantiles, and worst plausible joint perturbations. Prefer a
simple setting in a broad region of economically useful net results to an isolated
maximum. A smooth plateau is a diagnostic, not a formal significance test; every
point inspected still counts as a trial.

Certify only after costs. The base model must include Korean fees and taxes,
spread, slippage, latency, auction/opening behavior, partial fills, participation
and impact, rejects, price limits, and missed exits. Calibrate those elements from
paper/live shadow evidence and preserve calibration uncertainty. Real execution
data show that costs vary by trade type, stock characteristics, size, time, and
venue, so one constant haircut is not an implementation model
([Frazzini, Israel, and Moskowitz, 2018](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3229719)).
That study is not a KRX calibration source; it establishes the need for local,
conditional calibration.

Lock a base case, a conservative plausible upper-cost case, and joint stress
cases before the final assessment. Report break-even spread/slippage, latency,
fill rate, participation, and capital. Require the intended production capital to
sit inside the evidence-supported capacity range with a margin chosen by policy.
Do not tune execution assumptions until the result passes: cost-model choices are
part of the searched pipeline.

### 9. Select and test the complete Portfolio Head

The capital-bearing candidate includes:

- sleeve eligibility and conflict resolution;
- all symbol and reference filters;
- signal and exit parameters;
- capital weights, correlation estimates, and rebalance rule;
- portfolio risk budget, concentration and exposure limits;
- order scheduling, sizing, rejection/fallback behavior, and kill switches; and
- cost/capacity assumptions.

Fit or select every one of these inside each outer training window. The next outer
block then tests the *bundle*. Individually promising sleeves do not make a valid
portfolio if their weights were optimized using those sleeves' outer results.
Component evidence may diagnose the final bundle, but cannot be recombined and
still retain its former out-of-sample label.

Treat every sleeve subset, allocation, correlation cap, and shared overlay as a
portfolio candidate in the trial family. Compare the optimizer with simple locked
baselines such as equal-weight/equal-risk eligible sleeves and a constrained
low-turnover allocation. In a broad empirical comparison, DeMiguel, Garlappi, and
Uppal found that estimation error offset the theoretical benefit of optimized
portfolios and that none of 14 models consistently beat `1/N` out of sample
([DeMiguel, Garlappi, and Uppal, 2009](https://doi.org/10.1093/rfs/hhm075)). This
does not mandate equal weighting here; it mandates that complexity earn its place
against a simple allocation.

When several portfolios are statistically indistinguishable, retain a model
confidence set rather than pretending the sample identifies one precise winner.
The Model Confidence Set explicitly grows when the data are uninformative
([Hansen, Lunde, and Nason, 2011](https://doi.org/10.3982/ECTA5771)). If production
requires one head, apply a preregistered tie-breaker such as lower turnover, fewer
degrees of freedom, wider capacity, or simpler operational failure modes—not the
same noisy Sharpe estimate again.

### 10. Consume the final holdout exactly once

After the complete head and promotion rule are frozen, evaluate one locked
hypothesis on a sealed final chronological block. Stronger still, accumulate new
paper-shadow sessions after the freeze. Record the code commit, data snapshot,
configuration hash, split, primary statistic, cost case, and threshold before an
independent runner opens the data. Restrict the first result to the predeclared
decision output; per-symbol and per-day diagnostics can reveal enough information
to tune to the holdout.

Repeated adaptive access overfits a holdout just as it overfits training data;
Dwork et al. formalize this problem and show that ordinary repeated reuse does not
preserve statistical validity
([Dwork et al., 2015](https://doi.org/10.1126/science.aaa9375)). If the head fails
and any choice changes, retire the block into development history, record the new
trial generation, and wait for a new untouched period. Never rerun variants until
one passes and call that same block final.

A single final period is regime-specific and may have little power, which is why
it follows—not replaces—the rolling, family-adjusted, and stability evidence.
Historical certification should then hand off to prospective paper/live shadow
monitoring for execution calibration and drift; passing the holdout is not evidence
that market structure can no longer change.

## Evidence package a later certification decision should demand

| Artifact | Minimum content | What invalidates it |
|---|---|---|
| Search manifest | Claim, hurdle, benchmark, metrics, folds, costs, cells, power/error policy, promotion rule | Written or modified after the relevant results |
| Trial ledger | Every configuration and adaptive retry with code/data/config hashes | Missing failed, informal, allocation, or execution trials |
| Point-in-time audit | Universe, corporate actions, publication timestamps, ETF/ETN availability and lag | Full-sample membership or information not then available |
| Leakage audit | Per-observation information intervals, purge/embargo derivation, zero-overlap assertions | Arbitrary gap or overlap across folds |
| Nested walk-forward bundle | Non-overlapping outer predictions of the full selection/allocation process | Selecting weights or sleeves using their outer results |
| Family inference | Dependence-aware SPA/Reality Check or stepdown result and block sensitivity | Unlogged family or IID resampling of clustered returns |
| Search diagnostics | PBO/rank degradation, DSR, PSR/MinTRL, robust intervals | Presented as independent proof or computed on finalists only |
| Power record | Minimum useful net effect, effective sessions, dependence/tails, required versus available history | Trade count treated as independent sample size |
| Stability report | Predeclared cells, support, intervals, concentration, leave-one-group-out results | Post-hoc favorable slicing without multiplicity accounting |
| Cost/capacity report | Locally calibrated base/adverse cases, break-even and intended capital | Constant haircut or assumptions tuned to pass |
| Portfolio comparison | Full-head candidates and simple baselines under identical rules | Certifying sleeves separately then optimizing their OOS results |
| Final lockbox record | Frozen hashes and one authorized result on untouched data | Any earlier inspection or retuning after access |

## What these controls do not establish

- PBO, DSR, PSR, SPA, and a low p-value do not repair look-ahead or bad market data.
- Purging prevents interval leakage; it does not make a nonstationary market IID.
- Parameter smoothness and regime breadth are robustness evidence, not independent
  replications when found on the same history.
- A large number of fills does not create a large effective sample when portfolio
  outcomes cluster by session.
- Historical execution costs from another market cannot calibrate KRX opening
  liquidity.
- Individually certified sleeves do not certify their optimized combination.
- An untouched holdout cannot remain untouched after its output changes a choice.

The defensible result is thus not “the best backtest passed a battery of checks.”
It is: **a preregistered procedure searched transparently, selected a complete
Portfolio Head inside time-ordered training data, survived dependence-aware
family and stability tests net of locally calibrated costs, and then cleared one
previously untouched confirmation without further tuning.**
