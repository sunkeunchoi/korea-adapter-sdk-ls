//! Evidence-Freshness Evaluator — the 90-day age backstop **and** change-driven
//! staling over Recommended TRs.
//!
//! Two independent rules feed one per-TR report:
//!
//! * [`evaluate_recommended`] — the age rule. It selects every Recommended TR,
//!   evaluates `maintenance.last_reviewed` against an injected as-of date, and
//!   emits an `age` reason for each one past the 90-day backstop.
//! * [`evaluate_change_drift`] — the change rule (R1/R2). Per Recommended TR it
//!   diffs the frozen attested [`TrShape`](ls_metadata::TrShape) (from the
//!   evidence record) against the current committed baseline shape, keeps only
//!   changes in the R2 allow-list ([`crate::is_qualifying`]), and emits a `change`
//!   reason with a short drifted-shape summary when any survive. A normalizer
//!   version mismatch (R2a) or a missing per-TR baseline shape is a loud
//!   re-attestation advisory, never a silent fresh-by-change.
//!
//! [`evaluate_freshness`] runs both and merges them by `tr_code` into one entry
//! per TR (KTD8): a TR stale for both age and change is one entry carrying
//! `reasons: [age, change]`, clearing independently (R10). Every finding is
//! [`Severity::Evidence`] — **advisory and non-gating** ([`crate::gates_for`]
//! never trips on it, R8), and the evaluator **mutates nothing** (R7): clearing
//! is recompute-on-invocation.
//!
//! Production passes `as_of = today()` (UTC); tests inject a fixed date.

use std::collections::BTreeMap;
use std::fmt;

use chrono::{NaiveDate, Utc};
use ls_metadata::{EvidenceRecord, FreshnessState, TrMetadata, DEFAULT_WINDOW_DAYS};
use serde::Serialize;

use crate::api_drift::{diff_shapes, NormalizedRun};
use crate::types::{is_qualifying, DriftChange, Severity};

/// Why a Recommended TR's Focused Evidence is stale. The two reasons clear
/// independently (R10): refreshing `last_reviewed` clears `Age`, re-pinning the
/// attested shape clears `Change`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Past the 90-day `last_reviewed` backstop.
    Age,
    /// The committed baseline structurally diverged from the attested shape.
    Change,
}

impl Reason {
    /// The pinned lowercase token the `--json` contract and the rolling issue read.
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Age => "age",
            Reason::Change => "change",
        }
    }
}

/// The threshold past which the committed baseline's `refreshed` date warrants an
/// advisory baseline-staleness warning (R9a). Defaults to the same 90 days as the
/// evidence backstop. A stale baseline means change-detection is comparing against
/// possibly-outdated structural truth — visible, but non-gating.
pub const BASELINE_STALE_DAYS: i64 = DEFAULT_WINDOW_DAYS;

/// The committed baseline's liveness status (R9a). Reuses the single-sourced
/// [`ls_metadata::evaluate`] date rule — no new date arithmetic. A missing or
/// unparseable `refreshed` reads as *stale* (warn), surfacing the never-stamped
/// baseline, never a silent fresh.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BaselineStatus {
    /// The stamped refresh date (`""` when the manifest predates the field).
    pub refreshed: String,
    /// Age in days past `refreshed` — `Some` only when stale and parseable; `None`
    /// when fresh, missing, or unparseable.
    pub age_days: Option<i64>,
    /// `true` when the baseline is past the threshold OR its date is
    /// missing/unparseable (both warn).
    pub stale: bool,
}

/// One stale-evidence finding for a Recommended TR — a per-TR entry merged from
/// the age and change streams (KTD3). `reasons` distinguishes `age` from `change`
/// (a both-stale TR carries both); `age_days` is `Some` only when stale-by-age;
/// `change_summary` is `Some` only when stale-by-change. The payload carries only
/// structural descriptors — never raw evidence content. `severity` is always
/// [`Severity::Evidence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessFinding {
    pub tr_code: String,
    pub last_reviewed: String,
    /// `[Age]`, `[Change]`, or `[Age, Change]` (age first), never empty.
    pub reasons: Vec<Reason>,
    /// Age past the backstop in days — `Some` iff `reasons` contains `Age`.
    pub age_days: Option<i64>,
    /// A short summary of what structurally drifted — `Some` iff `reasons`
    /// contains `Change`.
    pub change_summary: Option<String>,
    pub severity: Severity,
}

impl FreshnessFinding {
    /// Whether this entry is stale by age.
    pub fn is_age(&self) -> bool {
        self.reasons.contains(&Reason::Age)
    }

    /// Whether this entry is stale by change.
    pub fn is_change(&self) -> bool {
        self.reasons.contains(&Reason::Change)
    }
}

impl fmt::Display for FreshnessFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reasons: Vec<&str> = self.reasons.iter().map(|r| r.as_str()).collect();
        write!(f, "[{}] {} ({})", self.severity, self.tr_code, reasons.join(","))?;
        if let Some(age) = self.age_days {
            write!(f, " {age}d past review (last_reviewed {})", self.last_reviewed)?;
        }
        if let Some(summary) = &self.change_summary {
            write!(f, " · {summary}")?;
        }
        Ok(())
    }
}

/// The result of one evaluator run: the merged stale findings, the count of
/// Recommended TRs examined, the TR codes whose `last_reviewed` could not be
/// parsed, and the TR codes needing re-attestation (normalizer-version mismatch
/// or a missing per-TR baseline shape).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FreshnessReport {
    pub findings: Vec<FreshnessFinding>,
    pub recommended_count: usize,
    /// Recommended TRs whose `last_reviewed` was unparseable, so age-freshness
    /// could not be evaluated. A non-empty list is a loud error (non-zero exit),
    /// never a silent pass.
    pub unparseable: Vec<String>,
    /// Recommended TRs whose change-detection was suppressed and need
    /// re-attestation: an `attested_normalizer_version` that differs from the
    /// baseline manifest (R2a/KTD4), or a missing/unreadable per-TR baseline shape
    /// (KTD8). Advisory and loud — surfaced, never a silent fresh-by-change.
    pub reattest: Vec<String>,
    /// The committed baseline's liveness (R9a). Advisory — never affects exit.
    pub baseline: BaselineStatus,
}

impl FreshnessReport {
    /// Whether any Recommended TR is stale (by age, change, or both).
    pub fn has_stale(&self) -> bool {
        !self.findings.is_empty()
    }

    /// Whether any Recommended TR had an unparseable `last_reviewed`.
    pub fn has_errors(&self) -> bool {
        !self.unparseable.is_empty()
    }

    /// Whether any Recommended TR needs re-attestation (advisory).
    pub fn has_reattest(&self) -> bool {
        !self.reattest.is_empty()
    }
}

/// Today's date in UTC — the single clock read for the production default. Kept
/// as a thin seam so the rest of the evaluator stays pure and injectable.
pub fn today() -> NaiveDate {
    Utc::now().date_naive()
}

/// Evaluate the **age** rule over every Recommended TR against an injected
/// `as_of`. Selection is `support.recommended == true`; the freshness input is
/// `maintenance.last_reviewed`. Iteration over a `BTreeMap` gives deterministic
/// TR-code order. A TR whose `last_reviewed` is unparseable lands in
/// [`FreshnessReport::unparseable`] and the scan continues — one malformed date
/// must not blind the check for the rest. This is the age-only rule; change
/// detection lives in [`evaluate_change_drift`] and is merged by
/// [`evaluate_freshness`].
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
                reasons: vec![Reason::Age],
                age_days: Some(age_days),
                change_summary: None,
                severity: Severity::Evidence,
            }),
            Ok(FreshnessState::Fresh) => {}
            Err(_) => report.unparseable.push(tr_code.clone()),
        }
    }
    report
}

/// One stale-by-change entry produced by the change rule, before merge.
struct ChangeEntry {
    tr_code: String,
    last_reviewed: String,
    change_summary: String,
}

/// The change rule's output: stale-by-change entries plus the re-attestation
/// advisory set.
struct ChangeDrift {
    findings: Vec<ChangeEntry>,
    reattest: Vec<String>,
}

/// Evaluate the **change** rule over every Recommended TR. Per TR (KTD1/KTD8):
///
/// * No evidence record or no `attested_shape` → no stale-by-change (the U7
///   validator backstops absence).
/// * `attested_normalizer_version` != the baseline manifest version → no
///   stale-by-change; the TR joins the re-attestation advisory set (R2a/KTD4).
/// * No per-TR baseline shape → re-attestation advisory, never silent
///   fresh-by-change (KTD8).
/// * Otherwise diff the attested shape against the baseline shape and keep only
///   [`crate::is_qualifying`] changes (R2); if any survive, emit a change entry.
fn evaluate_change_drift(
    trs: &BTreeMap<String, TrMetadata>,
    baseline: &NormalizedRun,
    evidence: &BTreeMap<String, EvidenceRecord>,
) -> ChangeDrift {
    let manifest_version = baseline.manifest.normalizer_version;
    let mut findings = Vec::new();
    let mut reattest = Vec::new();
    for (tr_code, meta) in trs {
        if !meta.support.recommended {
            continue;
        }
        let Some(record) = evidence.get(tr_code) else {
            continue; // U7 validator catches a missing evidence record
        };
        let Some(attested) = record.attested_shape.as_ref() else {
            continue; // U7 validator catches a never-captured attested shape
        };
        if record.attested_normalizer_version != Some(manifest_version) {
            // R2a: a representation shift under a different normalizer version is
            // not a structural change — route to re-attestation, never stale.
            reattest.push(tr_code.clone());
            continue;
        }
        let Some(base_shape) = baseline.shapes.get(tr_code) else {
            // Missing per-TR baseline shape — loud advisory, never silent fresh.
            reattest.push(tr_code.clone());
            continue;
        };
        // Attested is the frozen "base"; the current committed baseline is the
        // "candidate" that may have moved ahead of it (R1).
        let changes = diff_shapes(attested, base_shape);
        let qualifying: Vec<&DriftChange> = changes.iter().filter(|c| is_qualifying(c)).collect();
        if !qualifying.is_empty() {
            findings.push(ChangeEntry {
                tr_code: tr_code.clone(),
                last_reviewed: meta.maintenance.last_reviewed.clone(),
                change_summary: summarize_drift(&qualifying),
            });
        }
    }
    ChangeDrift { findings, reattest }
}

/// Render a one-line drifted-shape summary from the surviving qualifying changes.
/// Structural descriptors only — never a scalar sample value (KTD7).
fn summarize_drift(changes: &[&DriftChange]) -> String {
    let parts: Vec<String> = changes.iter().map(|c| summarize_one(c)).collect();
    format!("{} qualifying change(s): {}", parts.len(), parts.join("; "))
}

fn summarize_one(change: &DriftChange) -> String {
    match change {
        DriftChange::FieldAdded {
            direction,
            block_name,
            field_name,
            ..
        } => format!("added {direction} field {block_name}.{field_name}"),
        DriftChange::FieldRemoved {
            direction,
            block_name,
            field_name,
            ..
        } => format!("removed {direction} field {block_name}.{field_name}"),
        DriftChange::FieldChanged {
            direction,
            block_name,
            field_name,
            attributes,
            ..
        } => format!(
            "{direction} field {block_name}.{field_name} {}",
            crate::types::render_attribute_deltas(attributes)
        ),
        DriftChange::EndpointChanged { from, to } => format!(
            "endpoint {}→{}",
            from.as_deref().unwrap_or("?"),
            to.as_deref().unwrap_or("?")
        ),
        DriftChange::ProtocolChanged { from, to } => format!("protocol {from}→{to}"),
        // Non-qualifying variants never reach here (filtered by is_qualifying);
        // a debug fallback keeps the summary total rather than panicking.
        other => format!("{other:?}"),
    }
}

/// Merge the age report and the change-drift output into one report, joined by
/// `tr_code` (KTD8). A TR present in both streams becomes one entry with
/// `reasons: [Age, Change]`. The three-way reconciliation with `unparseable`
/// (KTD8): an unparseable-date TR has no age finding (it went to `unparseable`),
/// so a co-occurring change surfaces as a standalone `[Change]` entry while the
/// TR remains in the loud `unparseable` set.
fn merge(age: FreshnessReport, change: ChangeDrift) -> FreshnessReport {
    let mut by_code: BTreeMap<String, FreshnessFinding> = age
        .findings
        .into_iter()
        .map(|f| (f.tr_code.clone(), f))
        .collect();
    for entry in change.findings {
        let ChangeEntry {
            tr_code,
            last_reviewed,
            change_summary,
        } = entry;
        match by_code.get_mut(&tr_code) {
            Some(existing) => {
                existing.reasons.push(Reason::Change);
                existing.change_summary = Some(change_summary);
            }
            None => {
                by_code.insert(
                    tr_code.clone(),
                    FreshnessFinding {
                        tr_code,
                        last_reviewed,
                        reasons: vec![Reason::Change],
                        age_days: None,
                        change_summary: Some(change_summary),
                        severity: Severity::Evidence,
                    },
                );
            }
        }
    }
    FreshnessReport {
        // BTreeMap → deterministic TR-code order.
        findings: by_code.into_values().collect(),
        recommended_count: age.recommended_count,
        unparseable: age.unparseable,
        reattest: change.reattest,
        // evaluate_freshness overwrites this with the real baseline status; a bare
        // merge (no baseline context) defaults to fresh/never-stamped.
        baseline: BaselineStatus::default(),
    }
}

/// Evaluate the committed baseline's liveness against an injected `as_of` (R9a),
/// reusing the single-sourced [`ls_metadata::evaluate`] date rule (its parse, its
/// `>` boundary, its `FreshnessError`) — no new date arithmetic. A missing or
/// unparseable `refreshed` reads as *stale* (warn), surfacing the never-stamped
/// baseline rather than silently passing.
fn evaluate_baseline_staleness(refreshed: &str, as_of: NaiveDate) -> BaselineStatus {
    match ls_metadata::evaluate(refreshed, as_of, BASELINE_STALE_DAYS) {
        Ok(FreshnessState::Stale { age_days }) => BaselineStatus {
            refreshed: refreshed.to_string(),
            age_days: Some(age_days),
            stale: true,
        },
        Ok(FreshnessState::Fresh) => BaselineStatus {
            refreshed: refreshed.to_string(),
            age_days: None,
            stale: false,
        },
        // Missing (empty, serde-default) or unparseable date → warn, never silent.
        Err(_) => BaselineStatus {
            refreshed: refreshed.to_string(),
            age_days: None,
            stale: true,
        },
    }
}

/// Evaluate both the age and change rules, merge them into one report (KTD8), and
/// attach the committed baseline's liveness (R9a). The single entry point the CLI
/// calls.
pub fn evaluate_freshness(
    trs: &BTreeMap<String, TrMetadata>,
    baseline: &NormalizedRun,
    evidence: &BTreeMap<String, EvidenceRecord>,
    as_of: NaiveDate,
) -> FreshnessReport {
    let age = evaluate_recommended(trs, as_of);
    let change = evaluate_change_drift(trs, baseline, evidence);
    let mut report = merge(age, change);
    report.baseline = evaluate_baseline_staleness(&baseline.manifest.refreshed, as_of);
    report
}

// ---------------------------------------------------------------------------
// JSON contract (the scheduled-cadence workflow's machine-readable interface).
//
// A **dedicated serialization DTO**, not a bare `#[derive(Serialize)]` on
// `FreshnessReport`: the contract renames `findings` → `stale`, materializes
// `has_errors` (a method, not a field), threads in `as_of`/`window_days`,
// serializes `reasons` to their pinned lowercase tokens, and serializes
// `age_days`/`change_summary` as `null` when absent (the keys are always present
// so the `jq`/shell consumer sees a stable key set). The field names are
// load-bearing — they ARE the workflow contract, pinned by
// `json_field_names_are_pinned`.
// ---------------------------------------------------------------------------

/// One stale entry in the JSON contract — a borrowed view of a [`FreshnessFinding`]
/// with the pinned key set. `age_days`/`change_summary` are `null` when their
/// reason is absent (key always present, never `skip_serializing_if`).
#[derive(Debug, Serialize)]
struct FreshnessFindingJson<'a> {
    tr_code: &'a str,
    last_reviewed: &'a str,
    reasons: Vec<&'static str>,
    age_days: Option<i64>,
    change_summary: Option<&'a str>,
    /// Reuses the `Severity` `#[serde(rename_all = "snake_case")]` derive, which
    /// already emits `"evidence"` — no new serialization impl.
    severity: Severity,
}

/// The top-level JSON contract object. `findings` is renamed to `stale`; `as_of`
/// and `window_days` are threaded in at construction; `reattest` lists the TRs
/// needing re-attestation (advisory).
#[derive(Debug, Serialize)]
struct FreshnessReportJson<'a> {
    as_of: String,
    window_days: i64,
    recommended_count: usize,
    has_errors: bool,
    stale: Vec<FreshnessFindingJson<'a>>,
    reattest: &'a [String],
    unparseable: &'a [String],
    /// The committed baseline's stamped refresh date (`""` when never stamped).
    baseline_refreshed: &'a str,
    /// Baseline age in days past `refreshed` — `null` when fresh, missing, or
    /// unparseable (the key is always present).
    baseline_age_days: Option<i64>,
    /// `true` when the baseline is past the threshold or its date is
    /// missing/unparseable (advisory warning, R9a).
    baseline_stale: bool,
}

/// Serialize a [`FreshnessReport`] to the pinned JSON contract the scheduled
/// freshness-cadence workflow consumes. `as_of` and `window_days` are supplied by
/// the caller; `stale[]` preserves the report's deterministic TR-code order.
/// Pretty-printed for human-readable run logs; `jq`-parseable as a single object.
/// Exit semantics are unchanged — this only formats output.
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
                reasons: f.reasons.iter().map(|r| r.as_str()).collect(),
                age_days: f.age_days,
                change_summary: f.change_summary.as_deref(),
                severity: f.severity,
            })
            .collect(),
        reattest: &report.reattest,
        unparseable: &report.unparseable,
        baseline_refreshed: &report.baseline.refreshed,
        baseline_age_days: report.baseline.age_days,
        baseline_stale: report.baseline.stale,
    };
    serde_json::to_string_pretty(&dto).expect("freshness report DTO serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BlockField, CodeSet, Direction, Manifest, Protocol, TrShape};
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

    // --- age rule (carried from the 90-day backstop) ------------------------

    #[test]
    fn all_recommended_fresh_emits_no_finding() {
        let report = evaluate_recommended(&real_trs(), date(2026, 6, 18));
        assert!(report.findings.is_empty());
        assert!(!report.has_errors());
        assert_eq!(report.recommended_count, 10);
    }

    #[test]
    fn stale_emits_one_evidence_finding_per_recommended_tr() {
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1));
        assert_eq!(report.findings.len(), 10);
        assert_eq!(report.recommended_count, 10);
        for f in &report.findings {
            assert_eq!(f.severity, Severity::Evidence);
            assert_eq!(f.reasons, vec![Reason::Age]);
            assert!(f.age_days.unwrap() > 90);
            assert!(f.change_summary.is_none());
        }
        let codes: Vec<&str> = report.findings.iter().map(|f| f.tr_code.as_str()).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted);
    }

    #[test]
    fn evidence_severity_never_gates() {
        assert!(!gates_for(Severity::Evidence, SupportState::Recommended, false));
    }

    #[test]
    fn non_recommended_trs_are_exempt() {
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1));
        assert_eq!(report.recommended_count, 10);
        let recommended: std::collections::BTreeSet<&str> = [
            "token", "t1101", "t1102", "t8412", "S3_", "CSPAQ12200", "CSPAT00601",
            "CSPAT00701", "CSPAT00801", "t0425",
        ]
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
        assert!(today() >= date(2026, 6, 19));
        let report = evaluate_recommended(&real_trs(), today());
        assert_eq!(report.recommended_count, 10);
        assert!(!report.has_errors());
    }

    #[test]
    fn re_attestation_clears_the_finding() {
        let mut trs = real_trs();
        let as_of = date(2026, 10, 1);
        let stale = evaluate_recommended(&trs, as_of);
        assert!(stale.has_stale());
        for meta in trs.values_mut() {
            if meta.support.recommended {
                meta.maintenance.last_reviewed = "2026-10-01".to_string();
            }
        }
        let cleared = evaluate_recommended(&trs, as_of);
        assert!(!cleared.has_stale());
        assert_eq!(cleared.recommended_count, 10);
    }

    #[test]
    fn unparseable_last_reviewed_is_collected_not_propagated() {
        let mut trs = real_trs();
        let as_of = date(2026, 10, 1);
        if let Some(meta) = trs.get_mut("token") {
            meta.maintenance.last_reviewed = "not-a-date".to_string();
        }
        let report = evaluate_recommended(&trs, as_of);
        assert_eq!(report.recommended_count, 10);
        assert_eq!(report.unparseable, vec!["token".to_string()]);
        assert!(report.has_errors());
        assert_eq!(report.findings.len(), 9);
        assert!(report.findings.iter().all(|f| f.tr_code != "token"));
    }

    // --- change rule (U4) ---------------------------------------------------

    /// A minimal Recommended TR metadata fixture, parsed from YAML so the full
    /// schema stays consistent. `last_reviewed` is fresh (2026-06-16) so the age
    /// rule contributes nothing — isolating change detection.
    fn rec_meta(code: &str) -> TrMetadata {
        let yaml = format!(
            "\
tr_code: {code}
owner_class: standalone
facets:
  protocol: rest
  instrument_domain: misc
  venue_session: unspecified
  date_sensitive: false
  self_paginated: false
  account_state: false
  paper_incompatible: false
  certification_path: automated
  rate_bucket: auth
  caller_supplied_identifiers: []
support:
  tracked: true
  implemented: true
  recommended: true
maintenance:
  source_spec_hash: aaaa1111bbbb
  last_reviewed: 2026-06-16
recommendation:
  behavior: x
  evidence_ref: evidence/{code}.yaml
  excludes: []
"
        );
        ls_metadata::parse_tr_metadata(code, std::path::Path::new("inline"), &yaml)
            .expect("fixture parses")
    }

    fn field(dir: Direction, block: &str, index: u32, name: &str, ty: &str, len: u32) -> BlockField {
        BlockField {
            direction: dir,
            block_name: block.to_string(),
            field_index: index,
            field_name: name.to_string(),
            korean_name: Some("라벨".to_string()),
            r#type: Some(ty.to_string()),
            length: Some(len),
            required: true,
            description_hash: None,
        }
    }

    fn shape(code: &str, response: Vec<BlockField>) -> TrShape {
        TrShape {
            tr_code: code.to_string(),
            tr_name: None,
            protocol: Protocol::Rest,
            is_websocket: false,
            endpoint_path: Some(format!("/{code}")),
            api_group_id: None,
            source_group_name: None,
            request_blocks: vec![],
            response_blocks: response,
            rate_limit_per_sec: None,
            corp_rate_limit_per_sec: None,
            rate_source_group: None,
            description_hash: None,
        }
    }

    fn baseline(shapes: Vec<TrShape>, version: u32) -> NormalizedRun {
        let map: BTreeMap<String, TrShape> =
            shapes.into_iter().map(|s| (s.tr_code.clone(), s)).collect();
        NormalizedRun {
            code_set: CodeSet::new(map.keys().cloned(), false),
            manifest: Manifest {
                upstream_tr_count: map.len(),
                maintained_tr_count: map.len(),
                source_urls: vec![],
                normalizer_version: version,
                refreshed: "2026-06-20".to_string(),
            },
            shapes: map,
        }
    }

    fn evidence(code: &str, attested: Option<TrShape>, version: Option<u32>) -> EvidenceRecord {
        EvidenceRecord {
            tr_code: code.to_string(),
            date: "2026-06-16".to_string(),
            env: "paper".to_string(),
            target: None,
            line: None,
            attested_shape: attested,
            attested_normalizer_version: version,
        }
    }

    fn trs_of(codes: &[&str]) -> BTreeMap<String, TrMetadata> {
        codes.iter().map(|c| (c.to_string(), rec_meta(c))).collect()
    }

    fn ev_map(records: Vec<EvidenceRecord>) -> BTreeMap<String, EvidenceRecord> {
        records
            .into_iter()
            .map(|r| (r.tr_code.clone(), r))
            .collect()
    }

    /// Evaluate change drift at a fresh-by-age as-of so only change reasons appear.
    fn change_only(
        trs: &BTreeMap<String, TrMetadata>,
        base: &NormalizedRun,
        ev: &BTreeMap<String, EvidenceRecord>,
    ) -> FreshnessReport {
        evaluate_freshness(trs, base, ev, date(2026, 6, 18))
    }

    /// AE1: a baseline with a field the attested shape lacks (FieldAdded) → a
    /// `change` reason with a non-empty summary.
    #[test]
    fn field_added_in_baseline_is_stale_by_change() {
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let base = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        let trs = trs_of(&["t1102"]);
        let ev = ev_map(vec![evidence("t1102", Some(attested), Some(2))]);
        let report = change_only(&trs, &baseline(vec![base], 2), &ev);
        assert_eq!(report.findings.len(), 1);
        let f = &report.findings[0];
        assert!(f.is_change() && !f.is_age());
        assert_eq!(f.reasons, vec![Reason::Change]);
        assert!(f.age_days.is_none());
        assert!(f.change_summary.as_ref().unwrap().contains("volume"));
        assert_eq!(f.severity, Severity::Evidence);
        assert!(report.reattest.is_empty());
    }

    /// FieldRemoved, FieldChanged (type), EndpointChanged, ProtocolChanged each
    /// independently mark stale-by-change.
    #[test]
    fn each_qualifying_change_marks_stale() {
        // FieldRemoved.
        let attested = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        let base = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let r = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(r.findings.iter().any(|f| f.is_change()), "FieldRemoved stales");

        // FieldChanged (type) — same identity, different type.
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let base = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Long", 8)]);
        let r = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(r.findings.iter().any(|f| f.is_change()), "FieldChanged stales");

        // EndpointChanged.
        let mut attested = shape("t1102", vec![]);
        attested.endpoint_path = Some("/old".to_string());
        let mut base = shape("t1102", vec![]);
        base.endpoint_path = Some("/new".to_string());
        let r = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(r.findings.iter().any(|f| f.is_change()), "EndpointChanged stales");

        // ProtocolChanged.
        let attested = shape("t1102", vec![]);
        let mut base = shape("t1102", vec![]);
        base.protocol = Protocol::Websocket;
        base.is_websocket = true;
        let r = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(r.findings.iter().any(|f| f.is_change()), "ProtocolChanged stales");
    }

    /// AE2: a description-only divergence does not stale (filtered out).
    #[test]
    fn description_only_change_does_not_stale() {
        let mut attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        attested.description_hash = Some("aaaa".to_string());
        let mut base = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        base.description_hash = Some("bbbb".to_string()); // TR description changed only
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(report.findings.is_empty(), "DescriptionChanged must not stale");
    }

    /// AE3: a rate-limit-only divergence does not stale.
    #[test]
    fn rate_limit_only_change_does_not_stale() {
        let mut attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        attested.rate_limit_per_sec = Some(1);
        let mut base = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        base.rate_limit_per_sec = Some(5);
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(report.findings.is_empty(), "RateLimitChanged must not stale");
    }

    /// A same-block reorder (unique names) reconciles to FieldReordered and is
    /// filtered out — not a raw remove+add mass-stale.
    #[test]
    fn reorder_does_not_stale() {
        let attested = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "a", "String", 4),
                field(Direction::Response, "Out", 1, "b", "String", 4),
            ],
        );
        let base = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "b", "String", 4),
                field(Direction::Response, "Out", 1, "a", "String", 4),
            ],
        );
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(report.findings.is_empty(), "a reorder must not stale (R1/R2)");
    }

    /// R1 representation-invariance: an identical attested-vs-baseline shape under
    /// the same version yields zero qualifying changes (fresh-by-change) — proving
    /// detection rides on diff_shapes+filter, not raw TrShape equality over
    /// field_index/description_hash.
    #[test]
    fn identical_shape_same_version_is_fresh_by_change() {
        let s = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![s.clone()], 2),
            &ev_map(vec![evidence("t1102", Some(s), Some(2))]),
        );
        assert!(report.findings.is_empty());
        assert!(report.reattest.is_empty());
    }

    /// R2a (dual outcome): an attested normalizer version that differs from the
    /// baseline manifest → no stale-by-change AND the TR appears in `reattest`,
    /// even when the shapes structurally diverge.
    #[test]
    fn version_mismatch_suppresses_change_and_advises_reattest() {
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let base = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(1))]), // attested v1 vs manifest v2
        );
        assert!(report.findings.is_empty(), "version mismatch suppresses stale-by-change");
        assert_eq!(report.reattest, vec!["t1102".to_string()]);
    }

    /// Name-reprojection guard: a same-version baseline whose only difference is a
    /// field-name re-projection (no upstream change) surfaces as stale-by-change —
    /// the intended loud signal that a NORMALIZER_VERSION bump was missed (KTD4),
    /// not a silent mass-stale.
    #[test]
    fn field_name_reprojection_same_version_surfaces_as_change() {
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "oldname", "String", 4)]);
        let base = shape("t1102", vec![field(Direction::Response, "Out", 0, "newname", "String", 4)]);
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(
            report.findings.iter().any(|f| f.is_change()),
            "an un-versioned name re-projection is a loud caught discipline failure"
        );
    }

    /// Baseline-absent: a Recommended TR with no per-TR baseline shape → a
    /// re-attestation advisory, never a silent fresh-by-change.
    #[test]
    fn missing_per_tr_baseline_is_reattest_advisory() {
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        // Baseline has version 2 but NO shape for t1102.
        let base = NormalizedRun {
            code_set: CodeSet::new(["t1102".to_string()], false),
            manifest: Manifest {
                upstream_tr_count: 1,
                maintained_tr_count: 0,
                source_urls: vec![],
                normalizer_version: 2,
                refreshed: "2026-06-20".to_string(),
            },
            shapes: BTreeMap::new(),
        };
        let report = change_only(
            &trs_of(&["t1102"]),
            &base,
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert!(report.findings.is_empty());
        assert_eq!(report.reattest, vec!["t1102".to_string()]);
    }

    /// attested_shape: None → no stale-by-change (no panic); the U7 validator owns
    /// the absence error.
    #[test]
    fn attested_none_is_not_stale_by_change() {
        let base = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", None, None)]),
        );
        assert!(report.findings.is_empty());
        assert!(report.reattest.is_empty());
    }

    /// A TR stale for both age and change → one merged entry with reasons
    /// [Age, Change], age_days set, change_summary set (AE6).
    #[test]
    fn both_reasons_merge_into_one_entry() {
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let base = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        // as_of well past the 90-day backstop → also age-stale.
        let report = evaluate_freshness(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
            date(2026, 12, 1),
        );
        assert_eq!(report.findings.len(), 1, "one merged entry, not two");
        let f = &report.findings[0];
        assert_eq!(f.reasons, vec![Reason::Age, Reason::Change]);
        assert!(f.age_days.unwrap() > 90);
        assert!(f.change_summary.is_some());
    }

    /// Three-way merge: a TR that is both unparseable-date and stale-by-change →
    /// a standalone change entry (reasons [Change], age_days None) AND membership
    /// in the loud `unparseable` set (KTD8).
    #[test]
    fn unparseable_and_change_reconcile_three_way() {
        let mut trs = trs_of(&["t1102"]);
        trs.get_mut("t1102").unwrap().maintenance.last_reviewed = "not-a-date".to_string();
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let base = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        let report = evaluate_freshness(
            &trs,
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
            date(2026, 12, 1),
        );
        assert_eq!(report.unparseable, vec!["t1102".to_string()], "stays in loud error set");
        assert!(report.has_errors());
        assert_eq!(report.findings.len(), 1);
        let f = &report.findings[0];
        assert_eq!(f.reasons, vec![Reason::Change], "change entry, no age (unparseable)");
        assert!(f.age_days.is_none());
    }

    /// Two-lens severity: a change that would classify `Breaking` in api-drift
    /// compare (a removed field on an implemented TR) surfaces as `Evidence` here.
    #[test]
    fn change_drift_emits_evidence_not_breaking() {
        let attested = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        let base = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        assert_eq!(report.findings[0].severity, Severity::Evidence);
    }

    // --- baseline staleness (U5, R9a) ---------------------------------------

    #[test]
    fn baseline_at_threshold_is_fresh() {
        // 2026-06-17 + 90 days = 2026-09-15 → still fresh (inherited `>` boundary).
        let b = evaluate_baseline_staleness("2026-06-17", date(2026, 9, 15));
        assert!(!b.stale);
        assert_eq!(b.refreshed, "2026-06-17");
        assert!(b.age_days.is_none());
    }

    #[test]
    fn baseline_one_day_past_threshold_is_stale_with_age() {
        let b = evaluate_baseline_staleness("2026-06-17", date(2026, 9, 16));
        assert!(b.stale);
        assert_eq!(b.age_days, Some(91));
    }

    #[test]
    fn missing_baseline_refreshed_warns() {
        // Empty (serde-default for a pre-R9a manifest) → warn, never silent fresh.
        let b = evaluate_baseline_staleness("", date(2026, 9, 16));
        assert!(b.stale, "a never-stamped baseline warns");
        assert!(b.age_days.is_none());
    }

    #[test]
    fn unparseable_baseline_refreshed_warns() {
        let b = evaluate_baseline_staleness("not-a-date", date(2026, 9, 16));
        assert!(b.stale);
        assert!(b.age_days.is_none());
    }

    #[test]
    fn baseline_staleness_does_not_affect_exit_or_findings() {
        // A stale baseline (old refreshed) with otherwise fresh evidence yields an
        // advisory baseline warning but no stale findings — exit stays clean.
        let s = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let mut base = baseline(vec![s.clone()], 2);
        base.manifest.refreshed = "2026-01-01".to_string(); // long stale
        let report = evaluate_freshness(
            &trs_of(&["t1102"]),
            &base,
            &ev_map(vec![evidence("t1102", Some(s), Some(2))]),
            date(2026, 6, 18),
        );
        assert!(report.baseline.stale, "old baseline warns");
        assert!(report.findings.is_empty(), "no evidence findings");
        assert!(!report.has_errors(), "baseline staleness is not an error");
    }

    // --- JSON contract ------------------------------------------------------

    fn parse_json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("emitted JSON is a single valid object")
    }

    #[test]
    fn json_all_fresh_emits_empty_stale_and_no_errors() {
        let report = evaluate_recommended(&real_trs(), date(2026, 6, 18));
        let v = parse_json(&report_to_json(&report, date(2026, 6, 18), DEFAULT_WINDOW_DAYS));
        assert_eq!(v["stale"].as_array().unwrap().len(), 0);
        assert_eq!(v["has_errors"], serde_json::json!(false));
        assert_eq!(v["recommended_count"], serde_json::json!(10));
        assert_eq!(v["window_days"], serde_json::json!(90));
        assert_eq!(v["as_of"], serde_json::json!("2026-06-18"));
        assert_eq!(v["reattest"], serde_json::json!([]));
    }

    #[test]
    fn json_age_only_entry_has_null_change_summary() {
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1));
        let v = parse_json(&report_to_json(&report, date(2026, 10, 1), DEFAULT_WINDOW_DAYS));
        let entry = &v["stale"][0];
        assert_eq!(entry["reasons"], serde_json::json!(["age"]));
        assert!(entry["age_days"].as_i64().unwrap() > 90);
        assert_eq!(entry["change_summary"], serde_json::Value::Null);
        assert_eq!(entry["severity"], serde_json::json!("evidence"));
    }

    #[test]
    fn json_change_only_entry_has_null_age_days() {
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let base = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        let report = change_only(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
        );
        let v = parse_json(&report_to_json(&report, date(2026, 6, 18), DEFAULT_WINDOW_DAYS));
        let entry = &v["stale"][0];
        assert_eq!(entry["reasons"], serde_json::json!(["change"]));
        assert_eq!(entry["age_days"], serde_json::Value::Null);
        assert!(entry["change_summary"].as_str().unwrap().contains("volume"));
    }

    #[test]
    fn json_both_reasons_entry_carries_age_and_change() {
        // AE6 at the contract layer: a TR stale for both reasons serializes one
        // entry with reasons ["age","change"] and both age_days and change_summary
        // non-null.
        let attested = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let base = shape(
            "t1102",
            vec![
                field(Direction::Response, "Out", 0, "price", "Decimal", 8),
                field(Direction::Response, "Out", 1, "volume", "Long", 12),
            ],
        );
        let report = evaluate_freshness(
            &trs_of(&["t1102"]),
            &baseline(vec![base], 2),
            &ev_map(vec![evidence("t1102", Some(attested), Some(2))]),
            date(2026, 12, 1),
        );
        let v = parse_json(&report_to_json(&report, date(2026, 12, 1), DEFAULT_WINDOW_DAYS));
        let entry = &v["stale"][0];
        assert_eq!(entry["reasons"], serde_json::json!(["age", "change"]));
        assert!(entry["age_days"].as_i64().unwrap() > 90);
        assert!(entry["change_summary"].as_str().unwrap().contains("volume"));
    }

    #[test]
    fn json_carries_baseline_staleness_fields() {
        let s = shape("t1102", vec![field(Direction::Response, "Out", 0, "price", "Decimal", 8)]);
        let mut base = baseline(vec![s.clone()], 2);
        base.manifest.refreshed = "2026-01-01".to_string();
        let report = evaluate_freshness(
            &trs_of(&["t1102"]),
            &base,
            &ev_map(vec![evidence("t1102", Some(s), Some(2))]),
            date(2026, 6, 18),
        );
        let v = parse_json(&report_to_json(&report, date(2026, 6, 18), DEFAULT_WINDOW_DAYS));
        assert_eq!(v["baseline_refreshed"], serde_json::json!("2026-01-01"));
        assert_eq!(v["baseline_stale"], serde_json::json!(true));
        assert!(v["baseline_age_days"].as_i64().unwrap() > 90);
    }

    #[test]
    fn json_field_names_are_pinned() {
        // The emitted object's keys ARE the workflow contract.
        let report = evaluate_recommended(&real_trs(), date(2026, 10, 1));
        let v = parse_json(&report_to_json(&report, date(2026, 10, 1), DEFAULT_WINDOW_DAYS));
        let top: std::collections::BTreeSet<&str> =
            v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            top,
            [
                "as_of",
                "baseline_age_days",
                "baseline_refreshed",
                "baseline_stale",
                "has_errors",
                "reattest",
                "recommended_count",
                "stale",
                "unparseable",
                "window_days"
            ]
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
            ["age_days", "change_summary", "last_reviewed", "reasons", "severity", "tr_code"]
                .into_iter()
                .collect()
        );
    }
}
