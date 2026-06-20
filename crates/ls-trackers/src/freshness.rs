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
use ls_metadata::{FreshnessState, TrMetadata, DEFAULT_WINDOW_DAYS};
use serde::Serialize;

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

/// The result of one evaluator run: the stale findings, the count of Recommended
/// TRs examined (the denominator for an "N of M stale" summary), and the TR codes
/// whose `last_reviewed` could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FreshnessReport {
    pub findings: Vec<FreshnessFinding>,
    pub recommended_count: usize,
    /// Recommended TRs whose `last_reviewed` was unparseable, so freshness could
    /// not be evaluated. Collected rather than propagated, so one malformed date
    /// does not blind the check for the rest — but a non-empty list is a loud
    /// error (the CLI exits non-zero), never a silent pass.
    pub unparseable: Vec<String>,
}

impl FreshnessReport {
    /// Whether any Recommended TR is stale.
    pub fn has_stale(&self) -> bool {
        !self.findings.is_empty()
    }

    /// Whether any Recommended TR had an unparseable `last_reviewed`.
    pub fn has_errors(&self) -> bool {
        !self.unparseable.is_empty()
    }
}

/// Today's date in UTC — the single clock read for the production default. Kept
/// as a thin seam so the rest of the evaluator stays pure and injectable.
pub fn today() -> NaiveDate {
    Utc::now().date_naive()
}

// ---------------------------------------------------------------------------
// JSON contract (the scheduled-cadence workflow's machine-readable interface).
//
// A **dedicated serialization DTO**, not a bare `#[derive(Serialize)]` on
// `FreshnessReport`: the domain struct's field names and shape do not match the
// contract the workflow consumes (`jq .stale[].tr_code`, the rolling-issue marker
// diff), so the DTO performs four explicit transformations — rename `findings` →
// `stale`, materialize `has_errors` (a *method* on the report, not a field), thread
// in `as_of` + `window_days` (neither is stored on the report), and serialize the
// `Severity` enum to its lowercase string. The field names below are load-bearing:
// they *are* the workflow contract and are pinned by `json_field_names_are_pinned`.
// ---------------------------------------------------------------------------

/// One stale entry in the JSON contract — a borrowed view of a [`FreshnessFinding`]
/// with the pinned key set the workflow reads.
#[derive(Debug, Serialize)]
struct FreshnessFindingJson<'a> {
    tr_code: &'a str,
    last_reviewed: &'a str,
    age_days: i64,
    /// Reuses the `Severity` `#[serde(rename_all = "snake_case")]` derive, which
    /// already emits `"evidence"` — no new serialization impl.
    severity: Severity,
}

/// The top-level JSON contract object. `findings` is renamed to `stale`; `as_of`
/// and `window_days` are threaded in at construction (neither lives on the report).
#[derive(Debug, Serialize)]
struct FreshnessReportJson<'a> {
    as_of: String,
    window_days: i64,
    recommended_count: usize,
    has_errors: bool,
    stale: Vec<FreshnessFindingJson<'a>>,
    unparseable: &'a [String],
}

/// Serialize a [`FreshnessReport`] to the pinned JSON contract the scheduled
/// freshness-cadence workflow consumes. `as_of` and `window_days` are supplied by
/// the caller (the dispatch site passes today's UTC date and
/// [`DEFAULT_WINDOW_DAYS`]); `stale[]` preserves the report's deterministic
/// TR-code order. Pretty-printed for human-readable run logs; `jq`-parseable as a
/// single object. Exit semantics are unchanged — this only formats output.
pub fn report_to_json(report: &FreshnessReport, as_of: NaiveDate, window_days: i64) -> String {
    let dto = FreshnessReportJson {
        as_of: as_of.to_string(),
        window_days,
        recommended_count: report.recommended_count,
        has_errors: report.has_errors(),
        stale: report
            .findings
            .iter()
            .map(|f| FreshnessFindingJson {
                tr_code: &f.tr_code,
                last_reviewed: &f.last_reviewed,
                age_days: f.age_days,
                severity: f.severity,
            })
            .collect(),
        unparseable: &report.unparseable,
    };
    serde_json::to_string_pretty(&dto).expect("freshness report DTO serializes")
}

/// Evaluate every Recommended TR against an **injected** `as_of` date.
///
/// Selection is `support.recommended == true` (recommended TRs are also
/// implemented, so reading the boolean is correct where `SupportState` would
/// project them to `Implemented`). The freshness input is
/// `maintenance.last_reviewed`; no Focused Evidence record is consumed. Iteration
/// is over a `BTreeMap`, so findings are emitted in deterministic TR-code order.
///
/// A TR whose `last_reviewed` is unparseable is collected into
/// [`FreshnessReport::unparseable`] and the scan continues — one malformed date
/// must not blind the check for the rest. The caller treats a non-empty
/// `unparseable` list as a loud error (non-zero exit), never a silent pass.
pub fn evaluate_recommended(
    trs: &BTreeMap<String, TrMetadata>,
    as_of: NaiveDate,
) -> FreshnessReport {
    let mut report = FreshnessReport::default();
    for (tr_code, meta) in trs {
        if !meta.support.recommended {
            continue;
        }
        report.recommended_count += 1;
        match ls_metadata::evaluate(&meta.maintenance.last_reviewed, as_of, DEFAULT_WINDOW_DAYS) {
            Ok(FreshnessState::Stale { age_days }) => report.findings.push(FreshnessFinding {
                tr_code: tr_code.clone(),
                last_reviewed: meta.maintenance.last_reviewed.clone(),
                age_days,
                severity: Severity::Evidence,
            }),
            Ok(FreshnessState::Fresh) => {}
            Err(_) => report.unparseable.push(tr_code.clone()),
        }
    }
    report
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
        let report = evaluate_recommended(&real_trs(), date(2026, 6, 18));
        assert!(report.findings.is_empty());
        assert!(!report.has_errors());
        assert_eq!(report.recommended_count, 6);
    }

    #[test]
    fn stale_emits_one_evidence_finding_per_recommended_tr() {
        // AE3: as-of well past 90 days from every last_reviewed — all stale.
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1));
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
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1));
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
    fn default_today_uses_a_real_forward_moving_clock() {
        // AE7: the production default reads a real, forward-moving clock (a
        // hardcoded past date would fail the first assert) and feeds it to the
        // injectable evaluator. recommended_count is calendar-independent, so the
        // run is midnight-safe (no two-clock-read flake on the verdict).
        assert!(today() >= date(2026, 6, 19));
        let report = evaluate_recommended(&real_trs(), today());
        assert_eq!(report.recommended_count, 6);
        assert!(!report.has_errors());
    }

    #[test]
    fn re_attestation_clears_the_finding() {
        // AE4: a TR stale at an injected as_of, after advancing last_reviewed to
        // that as_of, re-evaluates fresh with no finding (recompute-on-invocation).
        let mut trs = real_trs();
        let as_of = date(2026, 10, 1);
        let stale = evaluate_recommended(&trs, as_of);
        assert!(stale.has_stale());

        // Re-attest every recommended TR to the as_of date.
        for meta in trs.values_mut() {
            if meta.support.recommended {
                meta.maintenance.last_reviewed = "2026-10-01".to_string();
            }
        }
        let cleared = evaluate_recommended(&trs, as_of);
        assert!(!cleared.has_stale());
        assert_eq!(cleared.recommended_count, 6);
    }

    // --- JSON contract (U1) -------------------------------------------------

    fn parse_json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("emitted JSON is a single valid object")
    }

    #[test]
    fn json_all_fresh_emits_empty_stale_and_no_errors() {
        // AE4: all fresh → stale [], has_errors false, correct denominator; the
        // process exits 0 (the dispatch path maps Ok-without-errors to Exit::Ok).
        let report = evaluate_recommended(&real_trs(), date(2026, 6, 18));
        let v = parse_json(&report_to_json(&report, date(2026, 6, 18), DEFAULT_WINDOW_DAYS));
        assert_eq!(v["stale"].as_array().unwrap().len(), 0);
        assert_eq!(v["has_errors"], serde_json::json!(false));
        assert_eq!(v["recommended_count"], serde_json::json!(6));
        assert_eq!(v["window_days"], serde_json::json!(90));
        assert_eq!(v["as_of"], serde_json::json!("2026-06-18"));
    }

    #[test]
    fn json_stale_lists_entries_in_deterministic_tr_code_order() {
        // AE1, AE4: every Recommended TR stale at this as-of → stale[] carries all
        // six in sorted TR-code order with correct age_days/last_reviewed; severity
        // serializes to the lowercase "evidence" string.
        let as_of = date(2026, 10, 1);
        let report = evaluate_recommended(&real_trs(), as_of);
        let v = parse_json(&report_to_json(&report, as_of, DEFAULT_WINDOW_DAYS));
        let stale = v["stale"].as_array().unwrap();
        assert_eq!(stale.len(), 6);
        let codes: Vec<&str> = stale.iter().map(|e| e["tr_code"].as_str().unwrap()).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "stale[] preserves deterministic TR-code order");
        for e in stale {
            assert_eq!(e["severity"], serde_json::json!("evidence"));
            assert!(e["age_days"].as_i64().unwrap() > 90);
            assert!(e["last_reviewed"].as_str().unwrap().len() == 10);
        }
    }

    #[test]
    fn json_unparseable_sets_has_errors_and_lists_offender() {
        // AE10: an unparseable last_reviewed surfaces has_errors true with the
        // offending code in unparseable[] — the tooling-error signal the workflow
        // reads (the dispatch path maps this to exit 2).
        let mut trs = real_trs();
        let as_of = date(2026, 10, 1);
        if let Some(meta) = trs.get_mut("token") {
            meta.maintenance.last_reviewed = "not-a-date".to_string();
        }
        let report = evaluate_recommended(&trs, as_of);
        let v = parse_json(&report_to_json(&report, as_of, DEFAULT_WINDOW_DAYS));
        assert_eq!(v["has_errors"], serde_json::json!(true));
        assert_eq!(v["unparseable"], serde_json::json!(["token"]));
    }

    #[test]
    fn json_field_names_are_pinned() {
        // The emitted object's keys ARE the workflow contract. A bare derive on
        // FreshnessReport would emit "findings" and fail this — the test guards U2's
        // `jq .stale[].tr_code` and the marker diff against a silent rename.
        let as_of = date(2026, 10, 1);
        let report = evaluate_recommended(&real_trs(), as_of);
        let v = parse_json(&report_to_json(&report, as_of, DEFAULT_WINDOW_DAYS));
        let top: std::collections::BTreeSet<&str> =
            v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            top,
            ["as_of", "has_errors", "recommended_count", "stale", "unparseable", "window_days"]
                .into_iter()
                .collect()
        );
        let entry: std::collections::BTreeSet<&str> = v["stale"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            entry,
            ["age_days", "last_reviewed", "severity", "tr_code"]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn unparseable_last_reviewed_is_collected_not_propagated() {
        // One Recommended TR with a malformed last_reviewed must not blind the
        // check for the rest: it lands in `unparseable` while the others still
        // evaluate. (The validator's date check is equality-only, so a malformed
        // last_reviewed can reach the evaluator.)
        let mut trs = real_trs();
        let as_of = date(2026, 10, 1);
        if let Some(meta) = trs.get_mut("token") {
            meta.maintenance.last_reviewed = "not-a-date".to_string();
        }
        let report = evaluate_recommended(&trs, as_of);
        assert_eq!(report.recommended_count, 6);
        assert_eq!(report.unparseable, vec!["token".to_string()]);
        assert!(report.has_errors());
        // The other five Recommended TRs still evaluated (stale at this as_of).
        assert_eq!(report.findings.len(), 5);
        assert!(report.findings.iter().all(|f| f.tr_code != "token"));
    }
}
