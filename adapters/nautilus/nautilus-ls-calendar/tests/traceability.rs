//! Traceability drift check (U1, R2/R3; AE1, AE2).
//!
//! `TRACEABILITY.md` (beside this crate's `Cargo.toml`) is the human-readable map from
//! every named calendar fixture scenario (S1–S12), every `calendar status` diagnostic
//! outcome, and every consumer calendar policy branch to the assertion that owns it. This
//! test is the machine-verifiable half: it re-declares the same `(scenario | outcome |
//! branch) → owning anchor` pairs and fails when any anchor no longer resolves in the tree
//! — a renamed test function, a deleted fixture scenario marker, a removed render token, or
//! a moved policy branch. Renaming a scenario/branch without updating BOTH the matrix and
//! this check is exactly the drift AE1 requires the gate to catch.
//!
//! ## What an anchor is
//!
//! An anchor is a `(file, needle)` pair: the needle string must appear literally in the
//! file. Test-function anchors use the `fn <name>` form so a rename breaks the check; render
//! tokens use the quoted literal as it appears in `src/diagnostics.rs`; fixture scenarios
//! anchor on their `// --- Scenario N` construction marker in `tests/fixtures.rs` AND on the
//! representative civil date in the committed fixture JSON. Every failing anchor is collected
//! and reported together, so one run surfaces the full drift set, not just the first miss.
//!
//! ## Path model
//!
//! Anchors resolve relative to this crate's `CARGO_MANIFEST_DIR`
//! (`adapters/nautilus/nautilus-ls-calendar/`). Crate-internal files (`tests/…`, `src/…`,
//! `fixtures/…`) resolve directly; the six consumer seams live one level up in the adapter
//! workspace, so their anchors use the `../` prefix (`../src/…`, `../lab/…`, `../tests/…`).
//! Reading a sibling-crate source file by path creates no cargo dependency — this leaf crate
//! stays dependency-free — and the paths are CWD-independent because they hang off
//! `CARGO_MANIFEST_DIR`, so the check behaves identically under `cargo test -p
//! nautilus-ls-calendar` and under `make foundation-gate` / `cargo test --workspace`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// Resolve an anchor path (crate-relative, `../`-prefixed for adapter-parent files) to an
/// absolute path under this crate's manifest dir.
fn resolve(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// A tiny read-through cache so each referenced file is read from disk exactly once.
#[derive(Default)]
struct FileCache {
    files: BTreeMap<String, Option<String>>,
}

impl FileCache {
    fn get(&mut self, rel: &str) -> Option<&str> {
        let entry = self
            .files
            .entry(rel.to_string())
            .or_insert_with(|| fs::read_to_string(resolve(rel)).ok());
        entry.as_deref()
    }
}

/// One matrix row: a human label, the file its owning anchor lives in, and every needle that
/// must appear in that file for the anchor to still resolve.
struct Anchor {
    /// What this row traces (scenario id, outcome, or consumer × branch).
    label: &'static str,
    /// The file (crate- or `../`-relative) the owning assertion lives in.
    file: &'static str,
    /// Substrings that must ALL be present in `file`.
    needles: &'static [&'static str],
}

/// Fixture scenarios S1–S12: each anchors on its `// --- Scenario N` construction marker in
/// `tests/fixtures.rs` (proving the scenario is still built) and on the representative civil
/// date in the committed fixture JSON (proving the materialized row survives). The owning
/// resolve/alert tests are separate rows below so a renamed test also trips the check.
const FIXTURE_SCENARIOS: &[Anchor] = &[
    Anchor { label: "S1 ordinary session", file: "tests/fixtures.rs", needles: &["Scenario 1 + 7:"] },
    Anchor { label: "S1 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2010-06-15\""] },
    Anchor { label: "S2 weekend closure", file: "tests/fixtures.rs", needles: &["Scenario 2:"] },
    Anchor { label: "S2 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2010-06-19\""] },
    Anchor { label: "S3 weekday election closure", file: "tests/fixtures.rs", needles: &["Scenario 3:"] },
    Anchor { label: "S3 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2010-06-02\""] },
    Anchor { label: "S4 Labor Day closure", file: "tests/fixtures.rs", needles: &["Scenario 4:"] },
    Anchor { label: "S4 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2012-05-01\""] },
    Anchor { label: "S5 Lunar New Year cluster", file: "tests/fixtures.rs", needles: &["Scenario 5:"] },
    Anchor { label: "S5 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2011-02-02\""] },
    Anchor { label: "S6 cited first-party closure", file: "tests/fixtures.rs", needles: &["Scenario 6:"] },
    Anchor { label: "S6 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2011-09-21\""] },
    Anchor { label: "S7 isolated Unknown", file: "tests/fixtures.rs", needles: &["Scenario 1 + 7:"] },
    Anchor { label: "S7 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2010-06-16\""] },
    Anchor { label: "S8 year-end closure", file: "tests/fixtures.rs", needles: &["Scenario 8:"] },
    Anchor { label: "S8 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2010-12-31\""] },
    Anchor { label: "S9 first materialization boundary", file: "tests/fixtures.rs", needles: &["Scenario 9:"] },
    Anchor { label: "S9 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2010-01-01\""] },
    Anchor { label: "S9b last materialization boundary", file: "tests/fixtures.rs", needles: &["Scenario 9b:"] },
    Anchor { label: "S9b date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2012-12-31\""] },
    Anchor { label: "S10 witness-over-inference", file: "tests/fixtures.rs", needles: &["Scenario 10:"] },
    Anchor { label: "S10 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2011-06-15\""] },
    Anchor { label: "S11 first-party disagreement", file: "tests/fixtures.rs", needles: &["Scenario 11:"] },
    Anchor { label: "S11 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2011-10-05\""] },
    Anchor { label: "S12 retrospective correction", file: "tests/fixtures.rs", needles: &["Scenario 12:"] },
    Anchor { label: "S12 date materialized", file: "fixtures/base_2010_2012.json", needles: &["\"2012-03-14\""] },
];

/// The fixture scenarios' owning `#[test]` assertions.
const FIXTURE_TESTS: &[Anchor] = &[
    Anchor { label: "scenarios resolve to expected status", file: "tests/fixtures.rs", needles: &["fn named_scenarios_resolve_to_their_expected_status"] },
    Anchor { label: "alert-bearing scenarios carry alerts", file: "tests/fixtures.rs", needles: &["fn alert_bearing_scenarios_carry_their_alerts"] },
    Anchor { label: "materialization boundaries", file: "tests/fixtures.rs", needles: &["fn materialization_boundaries_are_first_and_last_rows"] },
    Anchor { label: "fixture loads with correct identities", file: "tests/fixtures.rs", needles: &["fn base_fixture_loads_through_the_real_loader_with_correct_identities"] },
    Anchor { label: "fixture is not mistakable for real KRX", file: "tests/fixtures.rs", needles: &["fn base_fixture_cannot_be_mistaken_for_a_real_krx_calendar"] },
];

/// The ten `calendar status` diagnostic outcomes. Each anchors on its render token literal in
/// `src/diagnostics.rs` (the token both `render_human` and `render_json` emit) AND on the
/// owning `#[test]` in `tests/diagnostics.rs`, so a renamed token or test trips the check. The
/// human form renders `load:<x>`; the JSON form renders a nested `{"load":"<x>"}` — both derive
/// from the same `token()` literal, so anchoring the literal covers both render forms.
const DIAGNOSTIC_OUTCOMES: &[Anchor] = &[
    Anchor { label: "healthy (human+JSON)", file: "src/diagnostics.rs", needles: &["\"healthy\""] },
    Anchor { label: "healthy owning test", file: "tests/diagnostics.rs", needles: &["fn healthy_case_renders_stably"] },
    Anchor { label: "stale (human+JSON)", file: "src/diagnostics.rs", needles: &["\"stale\""] },
    Anchor { label: "stale owning test", file: "tests/diagnostics.rs", needles: &["fn stale_case_is_stale_not_a_status_rewrite"] },
    Anchor { label: "unknown (human+JSON)", file: "src/diagnostics.rs", needles: &["\"unknown\""] },
    Anchor { label: "unknown owning test", file: "tests/diagnostics.rs", needles: &["fn unknown_case_reports_unknown"] },
    Anchor { label: "conflict (human+JSON)", file: "src/diagnostics.rs", needles: &["\"conflict\""] },
    Anchor { label: "conflict owning test", file: "tests/diagnostics.rs", needles: &["fn conflict_case_reports_conflict_with_alerts"] },
    Anchor { label: "out_of_range (human+JSON)", file: "src/diagnostics.rs", needles: &["\"out_of_range\""] },
    Anchor { label: "out_of_range owning test", file: "tests/diagnostics.rs", needles: &["fn out_of_range_case_reports_out_of_range"] },
    Anchor { label: "missing (human+JSON)", file: "src/diagnostics.rs", needles: &["\"missing\""] },
    Anchor { label: "corrupt (human+JSON)", file: "src/diagnostics.rs", needles: &["\"corrupt\""] },
    Anchor { label: "incompatible (human+JSON)", file: "src/diagnostics.rs", needles: &["\"incompatible\""] },
    Anchor { label: "unauthorized (human+JSON)", file: "src/diagnostics.rs", needles: &["\"unauthorized\""] },
    Anchor { label: "expired (human+JSON)", file: "src/diagnostics.rs", needles: &["\"expired\""] },
    Anchor { label: "load-failure outcomes owning test", file: "tests/diagnostics.rs", needles: &["fn load_failure_cases_each_map_to_a_typed_outcome"] },
    Anchor { label: "incompatible names the version", file: "tests/diagnostics.rs", needles: &["fn incompatible_schema_names_the_version_but_stays_typed"] },
    Anchor { label: "redaction: no authority leak (both forms)", file: "tests/diagnostics.rs", needles: &["fn view_diagnostic_never_leaks_the_authority_identity"] },
    Anchor { label: "redaction: fingerprint non-reversible", file: "tests/diagnostics.rs", needles: &["fn masked_fingerprint_is_deterministic_but_non_reversible"] },
];

/// Consumer calendar policy branches. Each of the six consumer boundaries maps its
/// Legacy/Shadow/Enforced Trading/Closed/Unknown/unavailable branches to an owning assertion
/// in the consumer's test file (in the adapter workspace, `../`-prefixed). Every branch below
/// resolves to a real assertion — there are no gap rows (a branch with no owning assertion
/// would appear here as an explicit `GAP:` row, never a silent omission).
const CONSUMER_BRANCHES: &[Anchor] = &[
    // Accumulate / probe (ingest CalendarGate::action / range_action / probe_anchor).
    Anchor { label: "accumulate Shadow records but proceeds", file: "../tests/ingest.rs", needles: &["fn shadow_records_the_disagreeing_decision_but_proceeds"] },
    Anchor { label: "accumulate Shadow byte-identical to Legacy", file: "../tests/ingest.rs", needles: &["fn shadow_disagreement_is_byte_identical_to_legacy"] },
    Anchor { label: "accumulate Shadow unavailable byte-identical", file: "../tests/ingest.rs", needles: &["fn shadow_unavailable_is_byte_identical_to_legacy"] },
    Anchor { label: "accumulate Enforced Unknown → no request/advance", file: "../tests/ingest.rs", needles: &["fn enforced_unknown_target_makes_no_request_and_no_advance"] },
    Anchor { label: "accumulate Enforced Trading → fetch", file: "../tests/ingest.rs", needles: &["fn enforced_trading_session_target_fetches"] },
    Anchor { label: "accumulate Enforced Closed → advance no request", file: "../tests/ingest.rs", needles: &["fn enforced_closed_target_advances_without_request"] },
    Anchor { label: "accumulate Enforced unavailable → stop + preserve", file: "../tests/ingest.rs", needles: &["fn enforced_unavailable_calendar_stops_and_preserves_state"] },
    Anchor { label: "probe Enforced Unknown anchor → no request", file: "../tests/ingest.rs", needles: &["fn enforced_probe_unknown_anchor_makes_no_request"] },
    Anchor { label: "probe Enforced session anchor → probes", file: "../tests/ingest.rs", needles: &["fn enforced_probe_session_anchor_probes"] },
    // Checkpoint continuity.
    Anchor { label: "checkpoint Shadow migration byte-identical", file: "../tests/ingest.rs", needles: &["fn shadow_migration_is_byte_identical_to_legacy_even_when_calendar_disagrees"] },
    Anchor { label: "checkpoint Enforced merges all-closed gap", file: "../tests/ingest.rs", needles: &["fn enforced_merges_an_all_closed_gap_that_legacy_splits"] },
    Anchor { label: "checkpoint Enforced Trading in gap prevents merge", file: "../tests/ingest.rs", needles: &["fn enforced_trading_session_in_the_gap_prevents_merge"] },
    Anchor { label: "checkpoint Enforced Unknown gap kept separate", file: "../tests/ingest.rs", needles: &["fn enforced_keeps_separate_across_an_unknown_gap_that_legacy_merges"] },
    // Backward-widen.
    Anchor { label: "backward-widen Shadow byte-identical", file: "../tests/ingest.rs", needles: &["fn shadow_backward_widen_is_byte_identical_to_legacy"] },
    Anchor { label: "backward-widen Enforced Trading → warn + persist", file: "../tests/ingest.rs", needles: &["fn enforced_backward_widen_trading_session_warns_and_persists"] },
    Anchor { label: "backward-widen Enforced all-closed → silent", file: "../tests/ingest.rs", needles: &["fn enforced_backward_widen_all_closed_region_is_silent"] },
    Anchor { label: "backward-widen Enforced Unknown → uncertain", file: "../tests/ingest.rs", needles: &["fn enforced_backward_widen_unknown_region_is_uncertain_and_reevaluates"] },
    // Catalog readiness.
    Anchor { label: "catalog Shadow byte-identical", file: "../lab/tests/research_cli.rs", needles: &["fn shadow_is_byte_identical_to_legacy_while_recording_the_calendar_verdict"] },
    Anchor { label: "catalog Enforced Closed boundary no false-flag", file: "../lab/tests/research_cli.rs", needles: &["fn enforced_closed_watermark_boundary_does_not_false_flag"] },
    Anchor { label: "catalog Enforced Unknown → NO-GO indeterminate", file: "../lab/tests/research_cli.rs", needles: &["fn enforced_boundary_relevant_unknown_is_a_no_go_indeterminate"] },
    Anchor { label: "catalog Enforced out-of-coverage → NO-GO unavailable", file: "../lab/tests/research_cli.rs", needles: &["fn enforced_out_of_coverage_watermark_is_a_no_go_unavailable"] },
    Anchor { label: "catalog Enforced stale → GO + warning", file: "../lab/tests/research_cli.rs", needles: &["fn enforced_stale_but_established_is_a_go_with_a_prominent_warning"] },
    // Budget-probe automatic selection.
    Anchor { label: "budget-probe Shadow byte-identical", file: "../src/bin/budget-probe.rs", needles: &["fn shadow_default_and_request_are_byte_identical_to_legacy"] },
    Anchor { label: "budget-probe Enforced selects proven, skips closed/unknown", file: "../src/bin/budget-probe.rs", needles: &["fn enforced_selects_most_recent_proven_session_skipping_trailing_closed_unknown"] },
    Anchor { label: "budget-probe Enforced no session → no live call", file: "../src/bin/budget-probe.rs", needles: &["fn enforced_no_session_makes_no_live_call_until_explicit_range"] },
    Anchor { label: "budget-probe Enforced unavailable records condition", file: "../src/bin/budget-probe.rs", needles: &["fn enforced_unavailable_records_unavailable_condition_and_still_calls"] },
    Anchor { label: "budget-probe explicit-range bypass unchanged", file: "../src/bin/budget-probe.rs", needles: &["fn legacy_and_shadow_bypass_audit_does_not_change_range_or_request"] },
    Anchor { label: "budget-probe Enforced no-session refuses before gateway", file: "../tests/budget_probe_composition.rs", needles: &["fn enforced_no_session_refuses_before_any_gateway_call"] },
    // Production Ladder date-fact gate.
    Anchor { label: "ladder Shadow records, weekday authoritative", file: "../lab/src/runner/live.rs", needles: &["fn u188_shadow_over_fixture_records_but_weekday_stays_authoritative"] },
    Anchor { label: "ladder Legacy weekday-authoritative", file: "../lab/src/runner/live.rs", needles: &["fn u188_legacy_over_fixture_is_weekday_authoritative_and_still_loads"] },
    Anchor { label: "ladder Enforced Trading from calendar", file: "../lab/src/runner/live.rs", needles: &["fn u188_enforced_trading_session_from_calendar_not_weekday"] },
    Anchor { label: "ladder Enforced Closed fails + records", file: "../lab/src/runner/live.rs", needles: &["fn u188_enforced_closed_from_calendar_fails_and_records_active"] },
    Anchor { label: "ladder Enforced missing snapshot → fail-closed", file: "../lab/src/runner/live.rs", needles: &["fn u188_enforced_missing_snapshot_is_unavailable_and_fail_closed"] },
    Anchor { label: "ladder Enforced Unknown refuses / Trading greens", file: "../lab/tests/dispatch_checks.rs", needles: &["fn u12_failure_inversion_unknown_refuses_but_trading_session_greens"] },
    Anchor { label: "ladder time-window preserved (KTD7)", file: "../lab/tests/dispatch_checks.rs", needles: &["fn u12_time_window_preserved_for_a_proven_session_and_an_overridden_unknown"] },
    Anchor { label: "ladder weekday seam splits date-fact from time-window", file: "../lab/tests/dispatch_checks.rs", needles: &["fn weekday_seam_splits_date_fact_from_time_window"] },
];

/// The rollback rehearsal (U2) and its owning assertions — part of the Foundation Gate matrix
/// so removing the rehearsal makes the gate verifiably incomplete (U4 test scenario 2).
const ROLLBACK_REHEARSAL: &[Anchor] = &[
    Anchor { label: "rollback operation exists", file: "../src/calendar_refresh/activate.rs", needles: &["fn rollback", "struct RollbackRecord"] },
    Anchor { label: "rollback restores prior artifact identity", file: "../tests/calendar_activate.rs", needles: &["fn rollback_restores_prior_artifact_and_adoption_identity"] },
    Anchor { label: "rollback preserves 0o600", file: "../tests/calendar_activate.rs", needles: &["fn rollback_preserves_owner_only_permissions"] },
    Anchor { label: "rollback refuses corrupt/unauthorized prior", file: "../tests/calendar_activate.rs", needles: &["fn rollback_of_an_unusable_prior_snapshot_is_refused"] },
    Anchor { label: "rollback refuses prior not covering as_of", file: "../tests/calendar_activate.rs", needles: &["fn rollback_of_a_prior_snapshot_not_covering_as_of_is_refused"] },
    Anchor { label: "rollback refuses blank approval", file: "../tests/calendar_activate.rs", needles: &["fn rollback_with_blank_approval_is_refused"] },
];

/// The Shadow-divergence classification (U3) and its owning assertions — part of the
/// Foundation Gate matrix (U4 test scenario 2). Every consumer emits a classified, redacted,
/// non-persisted divergence observation, asserted by the rows below.
const DIVERGENCE_CLASSIFICATION: &[Anchor] = &[
    Anchor { label: "DivergenceClass type + classify", file: "../src/calendar.rs", needles: &["enum DivergenceClass", "fn classify_divergence"] },
    Anchor { label: "divergence redaction + non-persistence test", file: "../src/calendar.rs", needles: &["fn divergence_observation_is_redacted_and_classified"] },
    Anchor { label: "ingest emits classified divergence", file: "../tests/ingest.rs", needles: &["fn shadow_divergence_is_classified_and_redacted"] },
    Anchor { label: "catalog emits classified divergence", file: "../lab/tests/research_cli.rs", needles: &["fn shadow_divergence_is_classified_and_redacted"] },
    Anchor { label: "budget-probe emits classified divergence", file: "../src/bin/budget-probe.rs", needles: &["fn shadow_divergence_is_classified_and_redacted"] },
    Anchor { label: "ladder emits classified divergence", file: "../lab/src/runner/live.rs", needles: &["fn shadow_divergence_is_classified_and_redacted"] },
];

/// Run one group of anchors, appending a human-readable failure line per unresolved needle.
fn check_group(cache: &mut FileCache, group: &str, anchors: &[Anchor], failures: &mut Vec<String>) {
    for anchor in anchors {
        match cache.get(anchor.file) {
            None => failures.push(format!(
                "[{group}] {}: file not found or unreadable: {}",
                anchor.label, anchor.file
            )),
            Some(contents) => {
                for needle in anchor.needles {
                    if !contents.contains(needle) {
                        failures.push(format!(
                            "[{group}] {}: missing anchor {needle:?} in {}",
                            anchor.label, anchor.file
                        ));
                    }
                }
            }
        }
    }
}

/// The drift check: every matrix row resolves to a live anchor, or the whole set of misses is
/// reported at once. A renamed/removed fixture scenario, render token, consumer test, rollback
/// rehearsal, or divergence assertion fails here — the check the Foundation Gate runs.
#[test]
fn traceability_matrix_has_no_drift() {
    let mut cache = FileCache::default();
    let mut failures = Vec::new();

    check_group(&mut cache, "fixture-scenario", FIXTURE_SCENARIOS, &mut failures);
    check_group(&mut cache, "fixture-test", FIXTURE_TESTS, &mut failures);
    check_group(&mut cache, "diagnostic-outcome", DIAGNOSTIC_OUTCOMES, &mut failures);
    check_group(&mut cache, "consumer-branch", CONSUMER_BRANCHES, &mut failures);
    check_group(&mut cache, "rollback-rehearsal", ROLLBACK_REHEARSAL, &mut failures);
    check_group(&mut cache, "divergence-classification", DIVERGENCE_CLASSIFICATION, &mut failures);

    assert!(
        failures.is_empty(),
        "TRACEABILITY.md has drifted from the tree — {} unresolved anchor(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Sanity: the human-readable `TRACEABILITY.md` exists and names each anchor group, so the
/// matrix a reviewer reads and the drift check this file runs cannot silently diverge.
#[test]
fn traceability_document_covers_every_group() {
    let mut cache = FileCache::default();
    let doc = cache
        .get("TRACEABILITY.md")
        .expect("TRACEABILITY.md must exist beside the crate Cargo.toml");
    for section in [
        "Fixture scenarios",
        "Diagnostic outcomes",
        "Consumer policy branches",
        "Rollback rehearsal",
        "Shadow-divergence classification",
    ] {
        assert!(
            doc.contains(section),
            "TRACEABILITY.md is missing the '{section}' section that the drift check enforces"
        );
    }
}
