//! The daily path's typed **run observation** (P7, U6) — the fifth run artifact, written
//! only by the daily runner.
//!
//! Two consumers, neither of which this plan builds:
//!
//! 1. The **holdout judgment**. [`crate::lineage_prereg::judge_holdout`] takes six
//!    arguments, three of which are run-derived — the run id, the catalog fingerprint, and
//!    the observed statistic. Today a caller assembles those by hand from a bare number and
//!    a manifest read. [`RunObservation::judgment_arguments`] makes them constructible from
//!    the observation *alone*, which is what lets the marker below be enforced (R13).
//! 2. The **pre-turn admissibility re-check**, which needs the per-session series to derive
//!    an ICC and to run a session-block bootstrap.
//!
//! ## Why a series and not per-session counts or per-session RoR
//!
//! The frozen verdict statistic is `Σ realized_pnl / Σ risk_capital` — a **ratio of sums**,
//! not a mean of ratios. A session-block bootstrap over it must re-form the ratio inside
//! each resample, so it needs each session's numerator and denominator *separately*. A
//! per-session RoR column would already have collapsed them and cannot be un-collapsed;
//! per-session counts never had them. Hence [`SessionRow`] carries both sums (R14).
//!
//! ## Exit attribution (KTD13)
//!
//! A trade opening on session N and closing on N+16 is attributed to its **closing**
//! session, so a bucket's `realized_pnl` and `risk_capital` describe the same trades.
//! Entry attribution was rejected: under it a session carries deployed risk with no
//! realized P&L, and the ratio is meaningless per bucket. The visible consequence is that
//! the leading hold-length of sessions is empty — that is one bootstrap block, and it is
//! correct rather than missing data.
//!
//! `entries` and `closes` are plain event counts on their own dates and are *not*
//! exit-attributed; only the two sums are, because only they have to describe one trade set.
//!
//! ## The placeholder marker is fail-closed, not advisory
//!
//! A flag nothing reads is the same weak control as a naming convention. The judgment entry
//! point takes a bare number and claims the ledger *before* evaluating, against a frozen
//! `judgments_max` of 3 at `n_max = 1` — so a placeholder-signal run judged by accident
//! spends the holdout permanently. [`RunObservation::judgment_arguments`] is therefore the
//! only path to those three arguments *for every caller that obtains an observation the
//! normal way*, and it errors while the marker is set (KTD6). [`ObservationParts`] is
//! crate-private so nothing outside this crate can mint an observation with the marker
//! cleared. Two residual gaps are tracked rather than claimed closed: the artifact is
//! deserializable with a hand-cleared marker, and review must still catch the marker being
//! removed rather than the signal being replaced. See
//! [`RunObservation::judgment_arguments`] for the full caveat.
//!
//! This artifact lands beside the run and **not** in `lineage-preregistration.json`, whose
//! content hash is cited by its loader and by the judgment ledger (KTD8, R15).
//!
//! ## `observed_net_ror` is not bit-exact across a JSON round trip
//!
//! `serde_json`'s default float parser is not correctly rounded — the lab does not enable
//! its `float_roundtrip` feature — so reading this artifact back can land one ULP away from
//! the value written (measured: `1.0666666666666669` out, `1.0666666666666667` back). At
//! ~2e-16 relative that is far below any hurdle the statistic is compared against and
//! cannot change a verdict, but it does mean **this file is not a bit-exact record of the
//! run's float**. Do not build a content hash over it, and do not assert bit-equality
//! against an in-memory [`RunObservation`]. The authoritative statistic for a judgment is
//! the one [`RunObservation::judgment_arguments`] hands over in-process.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::artifacts::manifest::DataRange;
use crate::artifacts::performance::PerformanceReport;

/// The observation schema version. Bumped when a field's meaning changes; a new field is
/// additive and does not need one.
pub const OBSERVATION_SCHEMA_VERSION: u32 = 1;

/// One session's row of the series (R14).
///
/// Every in-range session gets a row, including sessions with no activity. A bootstrap
/// resamples *sessions*, so a silently-absent zero session would shorten the series and
/// understate the standard error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRow {
    /// The KST session date.
    pub session_date: NaiveDate,
    /// `Σ realized_pnl` over trades **closing** on this session (KTD13).
    pub realized_pnl: f64,
    /// `Σ risk_capital` over the same trades — the same set, by construction.
    pub risk_capital: f64,
    /// Positions **opening** on this session. Not exit-attributed: a plain event count.
    pub entries: u32,
    /// Positions **closing** on this session.
    pub closes: u32,
}

/// The three run-derived arguments [`crate::lineage_prereg::judge_holdout`] takes.
///
/// The loaded pre-registration, the judgment ledger, and the claim timestamp are the call
/// site's to supply and are deliberately **not** carried here: they are not properties of a
/// run, and an artifact that pretended otherwise would invite a caller to treat a stale
/// copy as authoritative.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentArguments {
    /// The run id.
    pub run_id: String,
    /// The run's range-scoped catalog fingerprint.
    pub catalog_fingerprint: String,
    /// The observed net return-on-risk — the frozen verdict statistic.
    pub observed_net_ror: f64,
}

/// Why an observation could not be built, or could not yield judgment arguments.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObservationError {
    /// R25. The run's `return_on_risk` is `None`.
    #[error(
        "run {run_id} has no return_on_risk, so no observation is written: the frozen verdict \
         statistic is Σrealized/Σrisk_capital, and `performance.json` documents its None case \
         as a LEGACY P&L FALLBACK, not an error. One closed trade missing risk_capital — one \
         non-positive ATR in the whole run — sets all_have_risk = false for the entire run and \
         collapses the statistic. Writing an observation here would report a P&L number under \
         a verdict that names a risk-normalized one"
    )]
    ReturnOnRiskUnavailable {
        /// The run that produced no statistic.
        run_id: String,
    },
    /// KTD6. The run was made with the placeholder ranking signal.
    #[error(
        "run {run_id} was made with the PLACEHOLDER ranking signal {signal:?}, so it yields no \
         judgment arguments: the signal that carries the hypothesis is turn one's act, and the \
         holdout is spent by exactly one judgment (n_max = 1). Judging a placeholder run would \
         consume it permanently against a signal nobody registered. Replace the signal and \
         re-run — do not remove the marker"
    )]
    PlaceholderRankingSignal {
        /// The run that carries the marker.
        run_id: String,
        /// The placeholder signal's name.
        signal: String,
    },
}

/// A finished daily run's typed observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunObservation {
    /// The schema version ([`OBSERVATION_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// The run id.
    pub run_id: String,
    /// The run's own pinned bar-data range (R13) — carried so the observation is
    /// sufficient without re-reading the manifest.
    pub data_range: DataRange,
    /// The run's own range-scoped catalog fingerprint (R13).
    pub catalog_fingerprint: String,
    /// The observed net return-on-risk. Never `Option`: an observation that could not
    /// carry one is not written at all (R25).
    pub observed_net_ror: f64,
    /// The ranking signal's name, recorded rather than inferred from the run id.
    pub ranking_signal: String,
    /// Whether that signal is the placeholder (R26). Structural, not a naming convention.
    pub ranking_signal_is_placeholder: bool,
    /// Positions still open at range end. `end()` does not flatten them and
    /// `dominance_fold` folds only closed trades, so these are absent from both the
    /// numerator and the denominator — the statistic is computed over `S − hold`
    /// effective sessions and this count is how a reader knows by how much.
    pub censored_positions: u32,
    /// Positions closed within the range — the set the statistic is computed over.
    pub closed_positions: u32,
    /// Every in-range session, in date order (R14).
    pub sessions: Vec<SessionRow>,
}

/// The inputs [`RunObservation::build`] cannot derive for itself.
///
/// **Crate-private on purpose.** The placeholder marker below is a governance gate, and a
/// publicly constructible parts struct would let any caller mint an observation with the
/// marker cleared while reusing a real placeholder run's statistic — which is precisely the
/// bypass KTD6 exists to prevent. Narrowing construction to this crate does not make the
/// marker unforgeable (see the caveat on [`RunObservation::judgment_arguments`]); it removes
/// the accidental path, which is the part this plan can actually close.
#[derive(Debug, Clone)]
pub(crate) struct ObservationParts<'a> {
    /// The run id.
    pub run_id: &'a str,
    /// The run's pinned range.
    pub data_range: &'a DataRange,
    /// The run's range-scoped catalog fingerprint.
    pub catalog_fingerprint: &'a str,
    /// The finished run's performance report — the source of both the statistic and the
    /// trade ledger the series folds.
    pub performance: &'a PerformanceReport,
    /// Every in-range session date, in order. Supplied by the runner's selection phase
    /// rather than derived from the trades, so a session with no activity still gets a row.
    pub session_dates: &'a [NaiveDate],
    /// The ranking signal's name.
    pub ranking_signal: &'a str,
    /// Whether that signal is the placeholder.
    pub ranking_signal_is_placeholder: bool,
}

impl RunObservation {
    /// Build the observation, or refuse.
    ///
    /// # Errors
    ///
    /// [`ObservationError::ReturnOnRiskUnavailable`] when the run's `return_on_risk` is
    /// `None` (R25). A placeholder marker does **not** block construction — the run is real
    /// and its series is the re-check's input; what the marker blocks is
    /// [`Self::judgment_arguments`].
    pub(crate) fn build(parts: ObservationParts<'_>) -> Result<RunObservation, ObservationError> {
        let observed_net_ror = parts
            .performance
            .edge_evaluation()
            .return_on_risk
            .ok_or_else(|| ObservationError::ReturnOnRiskUnavailable {
                run_id: parts.run_id.to_string(),
            })?;

        // One row per in-range session, pre-seeded to zero so an inactive session is
        // present-and-zero rather than absent.
        let mut rows: Vec<SessionRow> = parts
            .session_dates
            .iter()
            .map(|d| SessionRow {
                session_date: *d,
                realized_pnl: 0.0,
                risk_capital: 0.0,
                entries: 0,
                closes: 0,
            })
            .collect();
        let index_of = |date: NaiveDate| rows_index(parts.session_dates, date);

        let mut censored = 0u32;
        let mut closed = 0u32;
        for t in &parts.performance.trades {
            if let Some(i) = index_of(kst_date_of_ns(t.ts_opened)) {
                rows[i].entries += 1;
            }
            match t.ts_closed {
                Some(ts) => {
                    closed += 1;
                    if let Some(i) = index_of(kst_date_of_ns(ts)) {
                        rows[i].closes += 1;
                        // Exit attribution (KTD13): the pair moves together, always.
                        rows[i].realized_pnl += t.realized_pnl;
                        rows[i].risk_capital += t.risk_capital.unwrap_or(0.0);
                    }
                }
                None => censored += 1,
            }
        }

        Ok(RunObservation {
            schema_version: OBSERVATION_SCHEMA_VERSION,
            run_id: parts.run_id.to_string(),
            data_range: parts.data_range.clone(),
            catalog_fingerprint: parts.catalog_fingerprint.to_string(),
            observed_net_ror,
            ranking_signal: parts.ranking_signal.to_string(),
            ranking_signal_is_placeholder: parts.ranking_signal_is_placeholder,
            censored_positions: censored,
            closed_positions: closed,
            sessions: rows,
        })
    }

    /// The run-derived arguments for [`crate::lineage_prereg::judge_holdout`] — the only
    /// path to them (KTD6).
    ///
    /// # Errors
    ///
    /// [`ObservationError::PlaceholderRankingSignal`] while the placeholder marker is set.
    /// This is the fail-closed edge for every caller that obtains an observation the normal
    /// way — from the runner, or by deserializing the artifact the runner wrote.
    ///
    /// **What this does not stop.** `RunObservation` derives `Deserialize` and its fields are
    /// public, so a caller who is willing to hand-write an artifact with
    /// `ranking_signal_is_placeholder: false` can still obtain judgment arguments for a
    /// placeholder run. Closing that requires binding the marker to run provenance the caller
    /// cannot restate — the holdout is spent by exactly one judgment, so it is worth doing —
    /// and it is deliberately out of this plan's scope, which builds the producer rather than
    /// the judgment call site. Construction through [`ObservationParts`] is crate-private so
    /// the *accidental* path is closed; the adversarial one is a tracked follow-up.
    pub fn judgment_arguments(&self) -> Result<JudgmentArguments, ObservationError> {
        if self.ranking_signal_is_placeholder {
            return Err(ObservationError::PlaceholderRankingSignal {
                run_id: self.run_id.clone(),
                signal: self.ranking_signal.clone(),
            });
        }
        Ok(JudgmentArguments {
            run_id: self.run_id.clone(),
            catalog_fingerprint: self.catalog_fingerprint.clone(),
            observed_net_ror: self.observed_net_ror,
        })
    }

    /// `Σ risk_capital` over the series — the closure check against
    /// `performance.json`'s `risk_capital_total`.
    #[must_use]
    pub fn series_risk_capital_total(&self) -> f64 {
        self.sessions.iter().map(|s| s.risk_capital).sum()
    }

    /// `Σ realized_pnl` over the series.
    #[must_use]
    pub fn series_realized_pnl_total(&self) -> f64 {
        self.sessions.iter().map(|s| s.realized_pnl).sum()
    }
}

/// The KST calendar date of a UTC-nanosecond timestamp, via the adapter's single KST
/// conversion — the same one session slicing and ingest agree on.
fn kst_date_of_ns(ns: u64) -> NaiveDate {
    nautilus_ls::ingest::kst_date_of(nautilus_core::UnixNanos::from(ns))
}

/// The row index for `date`, or `None` when the date is outside the run's sessions.
///
/// Out-of-range is possible and is not an error: an exit can land on a date the selection
/// phase never listed (a session with no daily bar for any candidate). Such a trade is
/// still counted in `closed_positions`; it simply has no bucket. The closure check between
/// the series total and `risk_capital_total` is what would surface it if it ever mattered.
fn rows_index(dates: &[NaiveDate], date: NaiveDate) -> Option<usize> {
    dates.iter().position(|d| *d == date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::performance::{FillRecord, TradeRecord};

    /// KST is UTC+9, so `2024-01-10 00:00 KST` is `2024-01-09 15:00 UTC`.
    fn kst_noon(date: NaiveDate) -> u64 {
        use chrono::TimeZone;
        let naive = date.and_hms_opt(12, 0, 0).unwrap();
        let utc = chrono::Utc.from_utc_datetime(&(naive - chrono::Duration::hours(9)));
        utc.timestamp_nanos_opt().unwrap() as u64
    }

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    /// A closed trade opening on `open_day` and closing on `close_day`.
    fn closed(open_day: u32, close_day: u32, pnl: f64, risk: Option<f64>) -> TradeRecord {
        TradeRecord {
            symbol: "005930.XKRX".to_string(),
            entry_side: "BUY".into(),
            quantity: 10.0,
            avg_px_open: 60_000.0,
            avg_px_close: Some(60_000.0 + pnl / 10.0),
            realized_pnl: pnl,
            ts_opened: kst_noon(d(open_day)),
            ts_closed: Some(kst_noon(d(close_day))),
            fills: vec![FillRecord {
                ts_event: kst_noon(d(open_day)),
                side: "BUY".into(),
                qty: 10.0,
                price: 60_000.0,
                trade_id: "T-1".into(),
                commission: 0.0,
            }],
            risk_capital: risk,
            realized_r: risk.map(|r| pnl / r),
        }
    }

    /// A position still open at range end.
    fn censored(open_day: u32) -> TradeRecord {
        TradeRecord { ts_closed: None, avg_px_close: None, ..closed(open_day, open_day, 0.0, None) }
    }

    fn parts<'a>(
        perf: &'a PerformanceReport,
        dates: &'a [NaiveDate],
        range: &'a DataRange,
        placeholder: bool,
    ) -> ObservationParts<'a> {
        ObservationParts {
            run_id: "20240110T000000Z-backtest-daily-ms-v0",
            data_range: range,
            catalog_fingerprint: "cafe1234",
            performance: perf,
            session_dates: dates,
            ranking_signal: "prior_turnover_desc",
            ranking_signal_is_placeholder: placeholder,
        }
    }

    fn range() -> DataRange {
        DataRange { start: "20240103".into(), end: "20240112".into() }
    }

    /// Exit attribution (KTD13): a trade opening on session N and closing later puts its
    /// P&L **and** its risk capital on the closing session, together — and its entry count
    /// on the opening one.
    #[test]
    fn the_pnl_and_risk_pair_lands_on_the_closing_session() {
        let dates: Vec<NaiveDate> = (3..=12).map(d).collect();
        let perf = PerformanceReport::assemble(
            vec![closed(3, 11, 1_000.0, Some(4_000.0))],
            100_000_000.0,
        );
        let obs = RunObservation::build(parts(&perf, &dates, &range(), false)).unwrap();

        let row = |day: u32| obs.sessions.iter().find(|s| s.session_date == d(day)).unwrap();
        assert_eq!(row(3).entries, 1, "the entry counts on its own session");
        assert_eq!(row(3).realized_pnl, 0.0, "but carries no P&L — that is entry attribution");
        assert_eq!(row(3).risk_capital, 0.0, "nor risk, or the bucket ratio is meaningless");
        assert_eq!(row(11).closes, 1);
        assert_eq!(row(11).realized_pnl, 1_000.0);
        assert_eq!(row(11).risk_capital, 4_000.0);
        // The leading hold-length of sessions is empty. That is one bootstrap block, and
        // it is a correct consequence of exit attribution rather than missing data.
        assert!(
            obs.sessions.iter().take(8).all(|s| s.realized_pnl == 0.0 && s.risk_capital == 0.0),
            "sessions before the first exit are empty"
        );
    }

    /// The series is the closure check against `performance.json`: both sums reconcile.
    #[test]
    fn the_series_sums_reconcile_with_the_performance_report() {
        let dates: Vec<NaiveDate> = (3..=12).map(d).collect();
        let perf = PerformanceReport::assemble(
            vec![
                closed(3, 11, 1_000.0, Some(4_000.0)),
                closed(4, 11, -500.0, Some(2_000.0)),
                closed(5, 12, 250.0, Some(1_000.0)),
            ],
            100_000_000.0,
        );
        let obs = RunObservation::build(parts(&perf, &dates, &range(), false)).unwrap();
        let edge = perf.edge_evaluation();

        assert_eq!(obs.series_risk_capital_total(), edge.risk_capital_total.unwrap());
        assert_eq!(obs.series_realized_pnl_total(), 750.0);
        // …and the statistic is the ratio of those two sums, not a mean of per-session
        // ratios. This is why the series carries both columns (R14).
        let ror = obs.series_realized_pnl_total() / obs.series_risk_capital_total();
        assert!((ror - obs.observed_net_ror).abs() < 1e-12, "{ror} vs {}", obs.observed_net_ror);
    }

    /// `closes` sum to the closed count, and the censored count accounts for the rest.
    #[test]
    fn closes_and_censored_account_for_every_position() {
        let dates: Vec<NaiveDate> = (3..=12).map(d).collect();
        let perf = PerformanceReport::assemble(
            vec![closed(3, 11, 1_000.0, Some(4_000.0)), censored(10), censored(12)],
            100_000_000.0,
        );
        let obs = RunObservation::build(parts(&perf, &dates, &range(), false)).unwrap();

        let closes: u32 = obs.sessions.iter().map(|s| s.closes).sum();
        assert_eq!(closes, obs.closed_positions);
        assert_eq!(obs.censored_positions, 2);
        assert_eq!(
            obs.closed_positions + obs.censored_positions,
            perf.trades.len() as u32,
            "every position is either closed or censored — none is silently dropped"
        );
    }

    /// R25. No `return_on_risk` → no observation, and the error says why.
    #[test]
    fn a_run_without_return_on_risk_writes_no_observation() {
        let dates: Vec<NaiveDate> = (3..=12).map(d).collect();
        // One closed trade missing risk_capital sets all_have_risk = false for the WHOLE
        // run — that is the collapse this refusal exists to catch.
        let perf = PerformanceReport::assemble(
            vec![closed(3, 11, 1_000.0, Some(4_000.0)), closed(4, 11, 10.0, None)],
            100_000_000.0,
        );
        assert!(perf.edge_evaluation().return_on_risk.is_none(), "the fixture collapses RoR");

        let err = RunObservation::build(parts(&perf, &dates, &range(), false)).unwrap_err();
        assert!(matches!(err, ObservationError::ReturnOnRiskUnavailable { .. }));
        let msg = err.to_string();
        assert!(msg.contains("LEGACY P&L FALLBACK"), "names the trap: {msg}");
    }

    /// KTD6. The placeholder marker is fail-closed at the only accessor.
    #[test]
    fn a_placeholder_marked_observation_yields_no_judgment_arguments() {
        let dates: Vec<NaiveDate> = (3..=12).map(d).collect();
        let perf =
            PerformanceReport::assemble(vec![closed(3, 11, 1_000.0, Some(4_000.0))], 100_000_000.0);

        let marked = RunObservation::build(parts(&perf, &dates, &range(), true)).unwrap();
        assert!(marked.ranking_signal_is_placeholder, "the run is still built and readable");
        let err = marked.judgment_arguments().unwrap_err();
        assert!(matches!(err, ObservationError::PlaceholderRankingSignal { .. }));
        assert!(
            err.to_string().contains("do not remove the marker"),
            "the message names the wrong fix: {err}"
        );

        // …and the same run with a real signal yields all three arguments.
        let real = RunObservation::build(parts(&perf, &dates, &range(), false)).unwrap();
        let args = real.judgment_arguments().unwrap();
        assert_eq!(args.run_id, real.run_id);
        assert_eq!(args.catalog_fingerprint, "cafe1234");
        assert_eq!(args.observed_net_ror, real.observed_net_ror);
    }

    /// A run with zero sessions or zero trades is a defined result, not a division
    /// artifact — `build` refuses it for the *stated* reason (no statistic), rather than
    /// emitting a `NaN` observation.
    #[test]
    fn an_empty_run_is_a_defined_refusal_not_a_nan() {
        let empty = PerformanceReport::assemble(Vec::new(), 100_000_000.0);
        assert!(empty.edge_evaluation().return_on_risk.is_none());

        for dates in [Vec::new(), (3..=12).map(d).collect::<Vec<_>>()] {
            let err = RunObservation::build(parts(&empty, &dates, &range(), false)).unwrap_err();
            assert!(
                matches!(err, ObservationError::ReturnOnRiskUnavailable { .. }),
                "zero trades refuses rather than dividing by zero: {err}"
            );
        }

        // A run with sessions and trades but zero deployed risk is the same refusal, and
        // notably NOT an infinity: `dominance_fold` treats degenerate zero risk as absent.
        let dates: Vec<NaiveDate> = (3..=12).map(d).collect();
        let zero_risk =
            PerformanceReport::assemble(vec![closed(3, 11, 1_000.0, Some(0.0))], 100_000_000.0);
        let err = RunObservation::build(parts(&zero_risk, &dates, &range(), false)).unwrap_err();
        assert!(matches!(err, ObservationError::ReturnOnRiskUnavailable { .. }), "{err}");
    }

    /// The observation round-trips, and an inactive session survives as a zero row rather
    /// than being dropped — a shortened series understates a block bootstrap's error.
    #[test]
    fn every_in_range_session_gets_a_row_and_the_artifact_round_trips() {
        let dates: Vec<NaiveDate> = (3..=12).map(d).collect();
        let perf =
            PerformanceReport::assemble(vec![closed(3, 11, 1_000.0, Some(4_000.0))], 100_000_000.0);
        let obs = RunObservation::build(parts(&perf, &dates, &range(), false)).unwrap();

        assert_eq!(obs.sessions.len(), dates.len(), "one row per in-range session");
        assert_eq!(
            obs.sessions.iter().map(|s| s.session_date).collect::<Vec<_>>(),
            dates,
            "in date order"
        );
        let json = serde_json::to_string_pretty(&obs).unwrap();
        let back: RunObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, obs);
        assert_eq!(back.schema_version, OBSERVATION_SCHEMA_VERSION);
    }
}
