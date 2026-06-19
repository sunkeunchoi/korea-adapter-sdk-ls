//! Evidence-Freshness Evaluator — the operative 90-day backstop over Recommended
//! TRs.
//!
//! This is the live consumer of [`ls_metadata::freshness`]: it selects every
//! Recommended TR, evaluates its `maintenance.last_reviewed` against an injected
//! as-of date, and emits a [`FreshnessFinding`] at [`Severity::Evidence`] for each
//! one past the backstop. The finding is **advisory and non-gating** — `Evidence`
//! sits below `Maintenance`, so [`crate::gates_for`] never trips on it (R6).
//!
//! The evaluator **mutates nothing** (R7): it reads metadata and returns a report;
//! it never writes metadata, evidence, baselines, or docs. Clearing is
//! recompute-on-invocation — re-attestation updates `last_reviewed`, and the next
//! run simply finds the TR fresh (R12).
//!
//! Production passes `as_of = today()` (UTC); tests inject a fixed date so the
//! verdict is deterministic and stale behaviour is proven without wall-clock
//! waiting.

use std::collections::BTreeMap;
use std::fmt;

use chrono::{NaiveDate, Utc};
use ls_metadata::{FreshnessError, FreshnessState, TrMetadata, DEFAULT_WINDOW_DAYS};

use crate::types::Severity;

/// One stale-evidence finding for a Recommended TR past the 90-day backstop. The
/// payload carries only structural descriptors — TR code, the freshness date, and
/// the age in days — never raw evidence content. `severity` is always
/// [`Severity::Evidence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessFinding {
    pub tr_code: String,
    pub last_reviewed: String,
    pub age_days: i64,
    pub severity: Severity,
}

impl fmt::Display for FreshnessFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} {}d past review (last_reviewed {})",
            self.severity, self.tr_code, self.age_days, self.last_reviewed
        )
    }
}

/// The result of one evaluator run: the stale findings plus the count of
/// Recommended TRs examined (the denominator for an "N of M stale" summary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessReport {
    pub findings: Vec<FreshnessFinding>,
    pub recommended_count: usize,
}

impl FreshnessReport {
    /// Whether any Recommended TR is stale.
    pub fn has_stale(&self) -> bool {
        !self.findings.is_empty()
    }
}

/// Today's date in UTC — the single clock read for the production default. Kept
/// as a thin seam so the rest of the evaluator stays pure and injectable.
pub fn today() -> NaiveDate {
    Utc::now().date_naive()
}

/// Evaluate every Recommended TR against an **injected** `as_of` date.
///
/// Selection is `support.recommended == true` (recommended TRs are also
/// implemented, so reading the boolean is correct where `SupportState` would
/// project them to `Implemented`). The freshness input is
/// `maintenance.last_reviewed`; no Focused Evidence record is consumed. Iteration
/// is over a `BTreeMap`, so findings are emitted in deterministic TR-code order.
pub fn evaluate_recommended(
    trs: &BTreeMap<String, TrMetadata>,
    as_of: NaiveDate,
) -> Result<FreshnessReport, FreshnessError> {
    let mut findings = Vec::new();
    let mut recommended_count = 0;
    for (tr_code, meta) in trs {
        if !meta.support.recommended {
            continue;
        }
        recommended_count += 1;
        if let FreshnessState::Stale { age_days } =
            ls_metadata::evaluate(&meta.maintenance.last_reviewed, as_of, DEFAULT_WINDOW_DAYS)?
        {
            findings.push(FreshnessFinding {
                tr_code: tr_code.clone(),
                last_reviewed: meta.maintenance.last_reviewed.clone(),
                age_days,
                severity: Severity::Evidence,
            });
        }
    }
    Ok(FreshnessReport {
        findings,
        recommended_count,
    })
}

/// Evaluate against `today()` — the production default. Delegates to
/// [`evaluate_recommended`] with a single clock read.
pub fn evaluate_recommended_today(
    trs: &BTreeMap<String, TrMetadata>,
) -> Result<FreshnessReport, FreshnessError> {
    evaluate_recommended(trs, today())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gates_for, SupportState};
    use std::path::PathBuf;

    fn real_trs() -> BTreeMap<String, TrMetadata> {
        let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("metadata");
        ls_metadata::validate_dir(&metadata)
            .expect("authored metadata validates")
            .trs
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    #[test]
    fn all_recommended_fresh_emits_no_finding() {
        // AE1: as-of just after the latest last_reviewed — every Recommended TR
        // is within the 90-day window.
        let report = evaluate_recommended(&real_trs(), date(2026, 6, 18)).unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.recommended_count, 6);
    }

    #[test]
    fn stale_emits_one_evidence_finding_per_recommended_tr() {
        // AE3: as-of well past 90 days from every last_reviewed — all stale.
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1)).unwrap();
        assert_eq!(report.findings.len(), 6);
        assert_eq!(report.recommended_count, 6);
        for f in &report.findings {
            assert_eq!(f.severity, Severity::Evidence);
            assert!(f.age_days > 90);
        }
        // Deterministic TR-code order.
        let codes: Vec<&str> = report.findings.iter().map(|f| f.tr_code.as_str()).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted);
    }

    #[test]
    fn evidence_severity_never_gates() {
        // AE3: Severity::Evidence is below Maintenance, so the exit gate is false
        // even for a recommended TR.
        assert!(!gates_for(Severity::Evidence, SupportState::Recommended, false));
    }

    #[test]
    fn non_recommended_trs_are_exempt() {
        // AE5: only the six Recommended TRs are examined; implemented-but-not-
        // recommended (e.g. `revoke`) and tracked/untracked TRs never appear.
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1)).unwrap();
        assert_eq!(report.recommended_count, 6);
        let recommended: std::collections::BTreeSet<&str> =
            ["token", "t1101", "t1102", "t8412", "S3_", "CSPAQ12200"]
                .into_iter()
                .collect();
        for f in &report.findings {
            assert!(
                recommended.contains(f.tr_code.as_str()),
                "non-recommended TR {} leaked into findings",
                f.tr_code
            );
        }
    }

    #[test]
    fn default_today_path_delegates_to_injectable() {
        // AE7: the no-arg path reads a real, forward-moving clock (a hardcoded
        // past date would fail this) and delegates to the injectable evaluator.
        // recommended_count is calendar-independent, so the comparison is
        // midnight-safe (no two-clock-read flake on the verdict).
        assert!(today() >= date(2026, 6, 19));
        let trs = real_trs();
        let by_default = evaluate_recommended_today(&trs).unwrap();
        let by_injected = evaluate_recommended(&trs, today()).unwrap();
        assert_eq!(by_default.recommended_count, 6);
        assert_eq!(by_default.recommended_count, by_injected.recommended_count);
    }

    #[test]
    fn re_attestation_clears_the_finding() {
        // AE4: a TR stale at an injected as_of, after advancing last_reviewed to
        // that as_of, re-evaluates fresh with no finding (recompute-on-invocation).
        let mut trs = real_trs();
        let as_of = date(2026, 10, 1);
        let stale = evaluate_recommended(&trs, as_of).unwrap();
        assert!(stale.has_stale());

        // Re-attest every recommended TR to the as_of date.
        for meta in trs.values_mut() {
            if meta.support.recommended {
                meta.maintenance.last_reviewed = "2026-10-01".to_string();
            }
        }
        let cleared = evaluate_recommended(&trs, as_of).unwrap();
        assert!(!cleared.has_stale());
        assert_eq!(cleared.recommended_count, 6);
    }
}
