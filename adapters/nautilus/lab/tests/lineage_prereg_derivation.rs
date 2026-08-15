//! Derivation guard + hermetic tests for the successor daily lineage's pre-registration
//! (plan 2026-08-14-001, U1–U5).
//!
//! **Hermetic by construction.** Nothing here reads the KRX calendar snapshot: the split
//! logic is exercised against SYNTHETIC calendar facts, and every frozen number is
//! reproduced from named constants in `nautilus_ls::reference::pit_walk`,
//! `nautilus_ls_lab::stats`, or a committed `config/` artifact. The suite passes on a tree
//! with no `adapters/nautilus/state/` directory. The split COUNTS themselves are
//! citation-reproducible only — the operator harness in `lineage_prereg_derive.rs` is what
//! recounts them, by hand, against the machine-local snapshot.
//!
//! Following `prereg_derivation.rs`: assert RELATIONSHIPS between fields, not only literal
//! values, so two frozen numbers cannot drift apart without the gate noticing.

use std::collections::BTreeSet;
use std::path::PathBuf;

use chrono::NaiveDate;
use tempfile::TempDir;

use nautilus_ls::reference::pit_walk::{margin_bar_n1, SE_AT_ROOT, SE_ROOT_SESSIONS, Z_95};
use nautilus_ls_lab::dispatch::checks::CalendarDateFact;
use nautilus_ls_lab::lineage_prereg::{
    count_proven_sessions, derive_split, frozen_lineage_prereg_path, judge_holdout, load,
    load_optional, specification_dry_run, JudgmentAttempt, JudgmentLedger, LineagePreRegError,
    LineagePreRegistration, LoadedLineagePreReg, Partition, SplitAnchors, SplitError,
    JUDGMENT_SCHEMA_VERSION,
};
use nautilus_ls_lab::stats::{expected_max_null, power_z, two_sided_z};

// ===========================================================================
// Audited derivation inputs
// ===========================================================================

/// ORB v35's measured per-trade GROSS edge in R, from `adapters/nautilus/lab/TURN-LOG.md`
/// ("per-trade gross r | mean **+0.028422**"). Cited rather than machine-read: the gross
/// figure lives in the committed turn log, not in a machine-readable artifact — the same
/// precedent `prereg_derivation.rs` sets with the v34 rolling-5 constants.
const ORB_GROSS_R: f64 = 0.028_422;

/// The registered one-sided power.
const POWER: f64 = 0.80;

/// Absolute tolerance for figures that route through the repo's `probit` approximation,
/// which agrees with the exact normal quantile to ~1e-12 but not bit-for-bit.
const PROBIT_TOL: f64 = 1e-11;

/// Whether `haystack` cites the integer `n` as a standalone number.
///
/// A bare `str::contains` is near-useless for small figures in prose: `"8"` occurs dozens
/// of times inside dates like `2026-08-12` and `2016-08-01`, so a citation check built on
/// it passes no matter what the document actually says. Require digit boundaries on both
/// sides so `8` does not match inside `2016-08`, `128`, or `0.856407`.
fn contains_number(haystack: &str, n: u64) -> bool {
    let needle = n.to_string();
    let bytes = haystack.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = haystack[from..].find(&needle) {
        let start = from + rel;
        let end = start + needle.len();
        let left_ok = start == 0 || !bytes[start - 1].is_ascii_digit();
        // A trailing '.' or ',' followed by a digit is a decimal or thousands separator,
        // not a boundary — `16` must not match inside `16.5` or `2,460`.
        let right_ok = match bytes.get(end) {
            None => true,
            Some(&c) if c.is_ascii_digit() => false,
            Some(&c) if (c == b'.' || c == b',') => {
                !matches!(bytes.get(end + 1), Some(d) if d.is_ascii_digit())
            }
            Some(_) => true,
        };
        if left_ok && right_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got:?}, want {want:?} (|diff| {:?} > tol {tol:?})",
        (got - want).abs()
    );
}

/// The session-block bootstrap SE projected onto `sessions`, from the same two constants
/// `margin_bar_n1` is built from.
fn se_at(sessions: usize) -> f64 {
    SE_AT_ROOT * (SE_ROOT_SESSIONS / sessions as f64).sqrt()
}

fn config_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config").join(name)
}

fn read_json(name: &str) -> serde_json::Value {
    let bytes = std::fs::read(config_path(name))
        .unwrap_or_else(|e| panic!("reading config/{name}: {e}"));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parsing config/{name}: {e}"))
}

fn frozen() -> LoadedLineagePreReg {
    load(&frozen_lineage_prereg_path()).expect("the committed lineage pre-registration loads")
}

// ===========================================================================
// U1 — split derivation against SYNTHETIC calendar facts
// ===========================================================================

/// A synthetic calendar: the listed dates are proven sessions, every other date in
/// `[covered_from, covered_to]` is proven closed, and anything outside is `Unavailable`.
struct SyntheticCalendar {
    sessions: BTreeSet<NaiveDate>,
    covered_from: NaiveDate,
    covered_to: NaiveDate,
}

impl SyntheticCalendar {
    fn fact(&self, date: NaiveDate) -> CalendarDateFact {
        if date < self.covered_from || date > self.covered_to {
            return CalendarDateFact::Unavailable;
        }
        if self.sessions.contains(&date) {
            CalendarDateFact::TradingSession
        } else {
            CalendarDateFact::Closed
        }
    }
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("a real calendar date")
}

/// Every weekday in `[from, to]` is a session, minus `holidays`. Small, dense, and shaped
/// like KRX without pretending to be it.
fn weekday_calendar(from: NaiveDate, to: NaiveDate, holidays: &[NaiveDate]) -> SyntheticCalendar {
    use chrono::Datelike;
    let holidays: BTreeSet<NaiveDate> = holidays.iter().copied().collect();
    let mut sessions = BTreeSet::new();
    let mut date = from;
    while date <= to {
        let weekday = date.weekday().num_days_from_monday();
        if weekday < 5 && !holidays.contains(&date) {
            sessions.insert(date);
        }
        date = date.succ_opt().expect("inside the calendar range");
    }
    SyntheticCalendar { sessions, covered_from: from, covered_to: to }
}

#[test]
fn u1_a_boundary_that_is_not_a_session_is_rejected_not_rounded() {
    // 2024-01-06 is a Saturday. Anchoring the holdout there must REFUSE rather than quietly
    // advance to Monday — a rounded anchor is a boundary nobody chose.
    let cal = weekday_calendar(d(2024, 1, 1), d(2024, 3, 29), &[]);
    let anchors = SplitAnchors {
        floor: d(2024, 1, 1),
        specification_to: d(2024, 1, 5),
        holdout_from: d(2024, 1, 6), // Saturday
        holdout_to: d(2024, 2, 29),
        reserved_from: d(2024, 3, 1),
        ceiling: d(2024, 3, 29),
    };
    let err = derive_split(anchors, |x| cal.fact(x)).expect_err("a closed anchor must refuse");
    assert_eq!(
        err,
        SplitError::BoundaryNotASession {
            partition: "holdout",
            date: d(2024, 1, 6),
            fact: CalendarDateFact::Closed,
        },
        "the refusal names the partition and the offending date"
    );
    assert!(
        err.to_string().contains("rounding to a neighbour"),
        "the message says what it refused to do: {err}"
    );
}

#[test]
fn u1_a_closed_day_at_the_specification_boundary_straddles_nothing() {
    // The shape of the real freeze: the specification window ENDS on a closed day
    // (2019-12-31 in the frozen artifact) and the holdout starts on the next session, with
    // a closed day in the gap. No session may fall between them, and the counts must still
    // sum. Here 2024-01-05 (Fri) is a holiday and 2024-01-06/07 is a weekend.
    let cal = weekday_calendar(d(2024, 1, 1), d(2024, 3, 29), &[d(2024, 1, 5)]);
    let anchors = SplitAnchors {
        floor: d(2024, 1, 1),
        specification_to: d(2024, 1, 5), // closed — the 2019-12-31 case
        holdout_from: d(2024, 1, 8),
        holdout_to: d(2024, 2, 29),
        reserved_from: d(2024, 3, 1),
        ceiling: d(2024, 3, 29),
    };
    let split = derive_split(anchors, |x| cal.fact(x)).expect("a closed `to` anchor is legal");

    assert_eq!(
        split.specification.last_session,
        d(2024, 1, 4),
        "the last SESSION precedes the closed boundary day"
    );
    assert_ne!(
        split.specification.to, split.specification.last_session,
        "declared `to` and observed last session differ exactly when `to` is closed"
    );
    assert_eq!(
        split.specification.sessions + split.holdout.sessions + split.reserved.sessions,
        split.s_max,
        "the closed boundary day contributes to neither side and the split still sums"
    );
}

#[test]
fn u1_a_session_stranded_between_partitions_is_refused() {
    // Move the holdout's start one session later, stranding 2024-01-08 in the gap. The
    // deriver must refuse rather than silently drop it — that is the failure mode the
    // "sums to the ceiling" assertion alone would NOT catch, because both counts shrink
    // together only if you also recount the ceiling.
    let cal = weekday_calendar(d(2024, 1, 1), d(2024, 3, 29), &[]);
    let anchors = SplitAnchors {
        floor: d(2024, 1, 1),
        specification_to: d(2024, 1, 5),
        holdout_from: d(2024, 1, 9),
        holdout_to: d(2024, 2, 29),
        reserved_from: d(2024, 3, 1),
        ceiling: d(2024, 3, 29),
    };
    let err = derive_split(anchors, |x| cal.fact(x)).expect_err("a stranded session must refuse");
    assert_eq!(
        err,
        SplitError::SessionInGap {
            date: d(2024, 1, 8),
            left: "specification",
            right: "holdout",
        }
    );
}

#[test]
fn u1_partitions_sum_to_the_ceiling_and_no_session_appears_twice() {
    let cal = weekday_calendar(d(2024, 1, 1), d(2024, 3, 29), &[]);
    let anchors = SplitAnchors {
        floor: d(2024, 1, 1),
        specification_to: d(2024, 1, 31),
        holdout_from: d(2024, 2, 1),
        holdout_to: d(2024, 2, 29),
        reserved_from: d(2024, 3, 1),
        ceiling: d(2024, 3, 29),
    };
    let split = derive_split(anchors, |x| cal.fact(x)).expect("derives");

    assert_eq!(
        split.specification.sessions + split.holdout.sessions + split.reserved.sessions,
        split.s_max,
        "disjoint and exhaustive"
    );

    // Walk every day and confirm each session answers exactly one partition.
    let mut seen = 0usize;
    let mut date = split.from;
    while date <= split.to {
        let answers = [&split.specification, &split.holdout, &split.reserved]
            .iter()
            .filter(|p| date >= p.from && date <= p.to)
            .count();
        assert_eq!(answers, 1, "{date} must fall in exactly one partition, got {answers}");
        if cal.fact(date) == CalendarDateFact::TradingSession {
            seen += 1;
            assert!(split.partition_of(date).is_some(), "{date} is a session with no partition");
        }
        date = date.succ_opt().unwrap();
    }
    assert_eq!(seen, split.s_max, "the walk counts the same ceiling the deriver did");
}

#[test]
fn u1_sessions_after_the_holdout_land_in_reserved_never_in_holdout() {
    let cal = weekday_calendar(d(2024, 1, 1), d(2024, 3, 29), &[]);
    let anchors = SplitAnchors {
        floor: d(2024, 1, 1),
        specification_to: d(2024, 1, 31),
        holdout_from: d(2024, 2, 1),
        holdout_to: d(2024, 2, 29),
        reserved_from: d(2024, 3, 1),
        ceiling: d(2024, 3, 29),
    };
    let split = derive_split(anchors, |x| cal.fact(x)).expect("derives");

    let mut date = split.reserved.from;
    while date <= split.reserved.to {
        assert_eq!(
            split.partition_of(date),
            Some(Partition::Reserved),
            "{date} is past the holdout's end and must be reserved"
        );
        date = date.succ_opt().unwrap();
    }
    assert!(
        split.holdout.to < split.reserved.from,
        "the holdout closes strictly before the reserved tail opens"
    );
}

#[test]
fn u1_the_delta_attribution_helper_counts_an_arbitrary_sub_range() {
    // The helper KTD4's reconciliation rests on: sessions in a named sub-range.
    let cal = weekday_calendar(d(2024, 1, 1), d(2024, 3, 29), &[]);
    let whole = count_proven_sessions(d(2024, 1, 1), d(2024, 3, 29), &|x| cal.fact(x)).unwrap();
    let head = count_proven_sessions(d(2024, 1, 1), d(2024, 2, 29), &|x| cal.fact(x)).unwrap();
    let tail = count_proven_sessions(d(2024, 3, 1), d(2024, 3, 29), &|x| cal.fact(x)).unwrap();
    assert_eq!(head + tail, whole, "a range splits into its parts");
    assert_eq!(
        count_proven_sessions(d(2024, 1, 6), d(2024, 1, 7), &|x| cal.fact(x)).unwrap(),
        0,
        "a weekend-only range counts zero sessions"
    );
}

#[test]
fn u1_an_unprovable_day_refuses_rather_than_counting_zero() {
    // A day the calendar cannot answer for would make the ceiling a LOWER bound, and an
    // understated ceiling sets the margin bar too high. Fail closed.
    let cal = weekday_calendar(d(2024, 1, 1), d(2024, 3, 29), &[]);
    let err = count_proven_sessions(d(2024, 1, 1), d(2024, 4, 30), &|x| cal.fact(x))
        .expect_err("an out-of-coverage day must refuse");
    assert!(
        matches!(err, SplitError::UnprovenDay { fact: CalendarDateFact::Unavailable, .. }),
        "{err}"
    );
}

// ===========================================================================
// U2 / U3 — the committed artifact and its typed loader
// ===========================================================================

#[test]
fn u3_the_committed_artifact_loads_and_emits_a_64_hex_citation() {
    let loaded = frozen();
    assert_eq!(loaded.content_hash.len(), 64, "SHA-256 hex citation");
    assert!(
        loaded.content_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "the citation is hex: {}",
        loaded.content_hash
    );
    assert_eq!(loaded.values.schema_version, 1);
}

#[test]
fn u3_same_bytes_same_hash_one_field_edit_changes_it() {
    let tmp = TempDir::new().unwrap();
    let original = std::fs::read(frozen_lineage_prereg_path()).unwrap();
    let a = tmp.path().join("a.json");
    let b = tmp.path().join("b.json");
    std::fs::write(&a, &original).unwrap();
    let mutated = String::from_utf8(original)
        .unwrap()
        .replace("\"haircut_fraction\": 0.25", "\"haircut_fraction\": 0.3");
    std::fs::write(&b, &mutated).unwrap();

    let ha = load(&a).unwrap().content_hash;
    assert_eq!(ha, load(&a).unwrap().content_hash, "same bytes -> same citation");
    assert_ne!(ha, load(&b).unwrap().content_hash, "a one-field edit changes the citation");
}

#[test]
fn u3_a_missing_file_is_typed_and_load_optional_is_none() {
    let tmp = TempDir::new().unwrap();
    let absent = tmp.path().join("absent.json");
    assert!(
        matches!(load(&absent), Err(LineagePreRegError::Read { .. })),
        "a missing file is a typed Read error, not a panic"
    );
    assert!(load_optional(&absent).unwrap().is_none(), "load_optional tolerates absence");
}

#[test]
fn u3_a_missing_required_field_is_typed_and_names_the_field() {
    let tmp = TempDir::new().unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(frozen_lineage_prereg_path()).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("verdict");
    let p = tmp.path().join("no-verdict.json");
    std::fs::write(&p, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    let err = load(&p).expect_err("a missing required field must not load");
    assert!(matches!(err, LineagePreRegError::Parse { .. }), "typed, not a panic: {err}");
    assert!(err.to_string().contains("verdict"), "the error names the field: {err}");
}

#[test]
fn u2_holdout_judged_is_null_and_a_populated_one_is_refused_at_any_time() {
    // Judgments live in the ledger so the artifact's bytes never change (KTD10). A
    // populated record here is a defect whenever it appears, so the LOADER refuses it —
    // not a test that only runs before the judgment.
    assert!(frozen().values.holdout_judged.is_none(), "null in the committed artifact");

    let tmp = TempDir::new().unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(frozen_lineage_prereg_path()).unwrap()).unwrap();
    value["holdout_judged"] = serde_json::json!({ "run_id": "sneaky", "cleared": true });
    let p = tmp.path().join("judged.json");
    std::fs::write(&p, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    let err = load(&p).expect_err("a populated holdout_judged must refuse to load");
    assert!(matches!(err, LineagePreRegError::Invariant { .. }), "{err}");
}

#[test]
fn u2_the_two_null_by_design_fields_are_the_only_nulls() {
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(frozen_lineage_prereg_path()).unwrap()).unwrap();

    fn find_nulls(v: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Null => out.push(path.to_string()),
            serde_json::Value::Object(map) => {
                for (k, child) in map {
                    find_nulls(child, &format!("{path}.{k}"), out);
                }
            }
            serde_json::Value::Array(items) => {
                for (i, child) in items.iter().enumerate() {
                    find_nulls(child, &format!("{path}[{i}]"), out);
                }
            }
            _ => {}
        }
    }
    let mut nulls = Vec::new();
    find_nulls(&raw, "", &mut nulls);
    nulls.sort();
    assert_eq!(
        nulls,
        vec![".holdout_judged".to_string(), ".search.sigma_trials".to_string()],
        "exactly two fields are null by design; everything else must be populated"
    );
}

#[test]
fn u3_the_verdict_accessor_matches_the_predicate_computed_by_hand() {
    let v = frozen().values;
    let hurdle = v.bar() + v.haircut();
    for observed in [-0.05, 0.0, hurdle - 1e-9, hurdle, hurdle + 1e-9, 0.10] {
        assert_eq!(
            v.clears(observed),
            observed - v.haircut() > v.bar(),
            "the accessor and the hand predicate agree at {observed}"
        );
    }
    assert!(!v.clears(hurdle), "the predicate is strict: equal to the hurdle does NOT clear");
    assert!(v.clears(hurdle + 1e-9), "just above the hurdle clears");
}

#[test]
fn u3_no_holdout_accessor_ever_serves_the_reserved_tail() {
    let v = frozen().values;
    assert_ne!(v.holdout(), v.reserved(), "distinct partitions");
    assert!(
        v.holdout().to < v.reserved().from,
        "the holdout closes before the reserved tail opens"
    );
    // Every reserved date must answer `reserved`, never `holdout` — the quarantine only
    // means something if the judgment path cannot see it.
    for date in [v.reserved().from, v.reserved().last_session, v.reserved().to] {
        assert_eq!(v.partition_name_of(date), Some("reserved"), "{date}");
    }
    assert_eq!(v.partition_name_of(v.holdout().to), Some("holdout"));
}

#[test]
fn u2_provenance_carries_both_calendar_identities_and_says_it_is_citation_reproducible() {
    let p = &frozen().values.supply.provenance;
    for (what, id) in [
        ("artifact_id", &p.calendar_artifact_id),
        ("calendar_id", &p.calendar_id),
    ] {
        assert_eq!(id.len(), 64, "{what} is a 64-hex identity");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "{what} is hex");
    }
    assert_ne!(p.calendar_artifact_id, p.calendar_id, "two distinct identities");
    let lower = p.reproduction.to_ascii_lowercase();
    assert!(
        lower.contains("citation-reproducible") && lower.contains("gitignored"),
        "the split says how it is reproducible and why: {}",
        p.reproduction
    );
}

#[test]
fn u2_the_rederivation_trigger_names_the_catalog_and_the_universe_hash() {
    let v = frozen().values;
    let t = v.rederivation_trigger.to_ascii_lowercase();
    assert!(t.contains("fingerprint"), "names the catalog fingerprint: {t}");
    assert!(t.contains("pit-universe-20260812.json"), "names the universe artifact: {t}");
    // The universe hash in the trigger must be the ACTUAL hash of the committed artifact,
    // so an edit to that artifact reddens this gate instead of aging silently.
    let universe_bytes = std::fs::read(config_path("pit-universe-20260812.json")).unwrap();
    let universe_hash = nautilus_ls_lab::artifacts::manifest::hash_bytes(&universe_bytes);
    assert!(
        v.rederivation_trigger.contains(&universe_hash),
        "the trigger cites the committed universe artifact's real hash {universe_hash}"
    );
    assert!(
        v.rederivation_trigger.contains(&v.supply.provenance.calendar_artifact_id),
        "the trigger cites the calendar identity the split was counted under"
    );
}

#[test]
fn u2_inferred_figures_say_inferred_in_their_own_text() {
    let v = frozen().values;
    // The stop rule is the plan's named hazard: its width is inferred, not measured.
    let stop = v.hypothesis.derivation.get("stop_rule").expect("stop_rule has a derivation note");
    assert!(
        stop.basis.to_ascii_uppercase().contains("INFERRED"),
        "the stop rule's own text names it inferred: {}",
        stop.basis
    );
    // The page cap is measured and must NOT be listed as inferred; the vendor floor must.
    let joined = v.not_claimed.join("\n");
    assert!(
        joined.contains("005930") && joined.to_ascii_uppercase().contains("INFERRED"),
        "the vendor floor is named as inferred: {joined}"
    );
    // Cross-read the page-cap figures from the walk's own artifact rather than trusting
    // two literals typed into this test. A wrong number in the artifact's prose is exactly
    // the failure mode the guard exists to catch, and hardcoding 501/900 here would let it
    // through: both fields are in the committed universe artifact, so read them.
    let universe = read_json("pit-universe-20260812.json");
    let observed_cap =
        universe["derived"]["max_observed_rows_per_page"].as_u64().expect("max_observed_rows_per_page");
    let requested = universe["provenance"]["qrycnt"].as_u64().expect("qrycnt");
    assert!(
        observed_cap < requested,
        "the page cap only counts as MEASURED because the observed maximum {observed_cap} is \
         strictly below the requested {requested} — that is the condition pit_walk names"
    );
    assert!(
        contains_number(&joined, observed_cap) && contains_number(&joined, requested),
        "the page cap is stated with the walk's real figures ({observed_cap} observed of \
         {requested} requested): {joined}"
    );
    assert!(
        joined.to_ascii_uppercase().contains("MEASURED"),
        "and named as measured, not inferred: {joined}"
    );
    // Every derivation note declares a basis and an input.
    for (field, note) in &v.hypothesis.derivation {
        assert!(!note.input.trim().is_empty(), "{field} names a derivation input");
        assert!(!note.basis.trim().is_empty(), "{field} declares a basis");
    }
}

// ===========================================================================
// U4 — the derivation guard
// ===========================================================================

#[test]
fn u4_the_split_sums_to_the_ceiling_and_the_ceiling_matches_the_pit_walk() {
    let v = frozen().values;
    assert_eq!(
        v.supply.split.total_sessions(),
        v.supply.s_max,
        "specification + holdout + reserved = S_max"
    );
    let universe = read_json("pit-universe-20260812.json");
    let proven = universe["derived"]["proven_sessions"].as_u64().expect("proven_sessions") as usize;
    assert_eq!(v.supply.s_max, proven, "S_max is the P4 walk's measured proven_sessions");

    // Ordering and non-overlap of the frozen dates.
    let (s, h, r) = (&v.supply.split.specification, &v.supply.split.holdout, &v.supply.split.reserved);
    assert!(s.from < s.to && s.to < h.from, "specification precedes the holdout");
    assert!(h.from < h.to && h.to < r.from, "the holdout precedes the reserved tail");
    assert!(r.from < r.to, "the reserved tail is a real range");
    // Each partition's observed session span sits inside its declared range.
    for p in [s, h, r] {
        assert!(p.from <= p.first_session && p.last_session <= p.to, "{:?}", p);
        assert!(p.sessions > 0, "no partition is empty");
    }

    // The OUTER boundaries are pinned to the walk's own measured provenance, not to
    // literals. Without this, a drifted floor or anchor would sail through every
    // relationship above — the counts would still sum, and no committed test has a
    // calendar to recount against.
    let floor: NaiveDate = universe["provenance"]["floor"]
        .as_str()
        .expect("walk provenance floor")
        .parse()
        .expect("floor is a date");
    let anchor: NaiveDate = universe["provenance"]["anchor"]
        .as_str()
        .expect("walk provenance anchor")
        .parse()
        .expect("anchor is a date");
    assert_eq!(s.from, floor, "the split opens at the walk's measured supply floor");
    assert_eq!(r.to, anchor, "the split closes at the walk's own anchor");
    assert_eq!(s.from, s.first_session, "a partition starts ON a session, never on a closed day");
    assert_eq!(r.to, r.last_session, "the ceiling is a session");

    // The holdout→reserved seam is day-contiguous: the quarantine opens the very next day.
    // This is the assertion that bites a boundary silently shifted by a day while its
    // count is edited to match.
    assert_eq!(
        r.from,
        h.to.succ_opt().expect("a real date has a successor"),
        "the reserved tail opens the day after the holdout closes — no day may fall between"
    );
    // The specification→holdout seam deliberately skips a closed day (2019-12-31 is the
    // declared end, 2020-01-01 is a holiday, 2020-01-02 is the holdout's first session),
    // so it is contiguous only up to that documented gap. Bound it rather than leaving it
    // unchecked.
    let spec_gap = (h.from - s.to).num_days();
    assert!(
        (1..=3).contains(&spec_gap),
        "the specification→holdout seam spans {spec_gap} days; anything wider is a drifted \
         boundary, not a closed-day gap"
    );
    assert!(
        s.last_session < h.from && h.last_session <= h.to,
        "no session straddles the specification→holdout seam"
    );
}

#[test]
fn u4_the_origin_ceiling_delta_lands_entirely_in_the_reserved_tail() {
    // KTD4's claim, asserted as a RELATIONSHIP rather than as the literal 3: the origin
    // plan's end date falls inside the reserved partition, so every session the walk
    // counted beyond it is reserved. The specification and holdout counts are therefore
    // identical under both readings — which is what the stop condition cares about.
    let v = frozen().values;
    let origin = &v.supply.s_max_origin_plan;
    assert!(origin.sessions < v.supply.s_max, "the walk counted further than the origin plan");
    assert_eq!(
        v.partition_name_of(origin.to),
        Some("reserved"),
        "the origin plan's end date {} sits inside the reserved tail",
        origin.to
    );
    assert!(
        v.supply.s_max - origin.sessions <= v.supply.split.reserved.sessions,
        "the whole delta fits inside the reserved tail"
    );
    assert!(
        origin.delta_note.contains("2026-08-08") && origin.delta_note.contains("2026-08-12"),
        "the note names the date range the delta is attributed to: {}",
        origin.delta_note
    );
}

#[test]
fn u4_the_bar_reproduces_from_margin_bar_n1_at_the_frozen_holdout_count() {
    let v = frozen().values;
    assert_eq!(
        v.verdict.bar,
        margin_bar_n1(v.holdout().sessions),
        "bar = 1.96 x 0.087002 x sqrt(45 / holdout sessions)"
    );
    // The label on the bar's critical value: pit_walk freezes Z_95 = 1.96, a rounding of
    // the exact two-sided quantile. Bound the gap, do not claim equality.
    assert_close(
        Z_95,
        two_sided_z(v.power.confidence).unwrap(),
        1e-4,
        "Z_95 is the frozen rounding of the two-sided critical value at the registered confidence",
    );
}

#[test]
fn u4_the_haircut_is_exactly_its_frozen_fraction_of_the_bar() {
    let v = frozen().values;
    assert_eq!(
        v.verdict.haircut,
        v.verdict.haircut_fraction * v.verdict.bar,
        "haircut = haircut_fraction x bar, to full float precision"
    );
    assert_eq!(v.verdict.hurdle, v.verdict.bar + v.verdict.haircut, "hurdle = bar + haircut");
    assert!(
        v.verdict.haircut_fraction > 0.0 && v.verdict.haircut_fraction < 1.0,
        "the haircut is a proper fraction of the bar"
    );
    assert!(
        v.verdict.predicate.contains("observed_net_ror")
            && v.verdict.predicate.contains("haircut")
            && v.verdict.predicate.contains("bar"),
        "the predicate is stated executably: {}",
        v.verdict.predicate
    );
}

#[test]
fn u4_the_registered_effect_reproduces_at_the_registered_power_against_the_hurdle() {
    let v = frozen().values;
    let z = power_z(v.power.power).expect("registered power is a valid probability");
    assert_eq!(v.power.power, POWER, "the registered power is 0.80");

    // Net target: clears the HAIRCUT-INCLUSIVE hurdle at the registered power (KTD6).
    let want_net = v.verdict.hurdle + z * se_at(v.holdout().sessions);
    assert_close(
        v.hypothesis.effect_size_net_ror,
        want_net,
        PROBIT_TOL,
        "effect_size_net_ror = hurdle + z(power) x SE(holdout)",
    );

    // Cost, gross, and the ratio.
    assert_eq!(
        v.hypothesis.orb_measured_cost_r,
        v.hypothesis.orb_measured_gross_r - v.hypothesis.orb_measured_net_r,
        "ORB's cost is gross - net"
    );
    assert_eq!(v.hypothesis.orb_measured_gross_r, ORB_GROSS_R, "ORB's measured gross edge");
    let margin = read_json("sample-margin.json");
    assert_eq!(
        v.hypothesis.orb_measured_net_r,
        margin["provenance"]["net_r_mean"].as_f64().expect("net_r_mean"),
        "ORB's measured net edge is read from the committed sample-margin artifact"
    );
    assert_close(
        v.hypothesis.effect_size_gross_r,
        v.hypothesis.effect_size_net_ror + v.hypothesis.orb_measured_cost_r,
        PROBIT_TOL,
        "gross target = net target + round-trip cost",
    );
    assert_close(
        v.hypothesis.effect_size_ratio_to_orb_gross,
        v.hypothesis.effect_size_gross_r / v.hypothesis.orb_measured_gross_r,
        PROBIT_TOL,
        "the ratio is the gross target over ORB's measured gross",
    );
    // The origin plan's rounded quotation must still agree to its own precision.
    assert_close(
        v.hypothesis.effect_size_ratio_to_orb_gross,
        v.hypothesis.effect_size_ratio_origin_plan_rounded,
        5e-4,
        "the frozen ratio agrees with the origin plan's rounded 3.8803",
    );
}

#[test]
fn u4_the_holding_period_is_the_ceiling_of_the_ratio_squared() {
    let v = frozen().values;
    let ratio = v.hypothesis.effect_size_ratio_to_orb_gross;
    assert_eq!(
        v.hypothesis.holding_period_sessions,
        (ratio * ratio).ceil() as usize,
        "sqrt-time scaling: hold = ceil(ratio^2)"
    );
    assert!(v.hypothesis.holding_period_sessions >= 2, "a multi-session hold is the whole point");
}

#[test]
fn u4_the_bootstrap_block_is_never_shorter_than_the_hold() {
    let v = frozen().values;
    assert!(
        v.verdict.bootstrap_block_length_sessions >= v.hypothesis.holding_period_sessions,
        "block {} must not be shorter than the {}-session hold — shorter blocks assume an \
         independence the hold spans across, understating the SE",
        v.verdict.bootstrap_block_length_sessions,
        v.hypothesis.holding_period_sessions
    );
}

#[test]
fn u4_concurrency_and_breadth_reproduce_and_stay_inside_measured_supply() {
    let v = frozen().values;
    assert_eq!(
        v.hypothesis.steady_state_concurrency,
        v.hypothesis.target_m * v.hypothesis.holding_period_sessions,
        "steady-state concurrency = m x hold"
    );

    let universe = read_json("pit-universe-20260812.json");
    let thresholds = universe["derived"]["thresholds"].as_array().expect("threshold rows");
    let max_verified = thresholds
        .iter()
        .map(|t| t["concurrency"].as_u64().expect("concurrency") as usize)
        .max()
        .expect("at least one verified threshold row");
    assert!(
        v.hypothesis.steady_state_concurrency <= max_verified,
        "concurrency {} exceeds the largest VERIFIED threshold row {max_verified} — freezing \
         it would freeze supply the walk never measured",
        v.hypothesis.steady_state_concurrency
    );

    let listed_min =
        universe["derived"]["listed_count_min"].as_u64().expect("listed_count_min") as usize;
    assert_eq!(v.supply.listed_count_min, listed_min, "the floor listed count is the walk's");
    assert_eq!(
        v.hypothesis.selection_breadth,
        v.hypothesis.steady_state_concurrency as f64 / listed_min as f64,
        "selection breadth = concurrency / floor listed count"
    );
    assert!(
        v.hypothesis.selection_breadth < 1.0,
        "the strategy cannot hold more names than the universe's floor listing"
    );
}

#[test]
fn u4_the_frozen_stop_reproduces_orbs_measured_cost() {
    // KTD14: cost_R = round-trip cost of notional / stop width. Changing the stop changes
    // the required gross edge, so the frozen stop is asserted against the frozen cost.
    let v = frozen().values;
    let costs = read_json("transaction-costs.json");
    let sell_tax = costs["sell_tax_rate"].as_f64().expect("sell_tax_rate");
    let commission = costs["commission_rate_per_side"].as_f64().expect("commission_rate_per_side");
    let round_trip = sell_tax + 2.0 * commission;

    assert_close(
        round_trip / v.hypothesis.stop_implied_pct_of_price,
        v.hypothesis.orb_measured_cost_r,
        1e-12,
        "cost_R = (sell tax + 2 x commission) / stop width",
    );
    assert!(
        v.hypothesis.stop_rule.contains("1.5") && v.hypothesis.stop_rule.contains("ATR"),
        "the frozen stop rule is the 1.5x ATR width: {}",
        v.hypothesis.stop_rule
    );
    assert!(
        v.hypothesis.stop_implied_pct_of_price > 0.0
            && v.hypothesis.stop_implied_pct_of_price < 1.0,
        "the implied stop is a proper fraction of price"
    );
}

#[test]
fn u4_every_scheduled_upgrade_turn_is_clearable_at_the_registered_effect() {
    let v = frozen().values;
    let sched = &v.upgrade_schedule;

    assert!(sched.max_turns >= 1, "a schedule with no turns is not a schedule");
    assert!(sched.max_turns <= 10, "the turn count is a small bounded integer, not open-ended");
    assert_eq!(sched.turns.len(), sched.max_turns, "every permitted turn is listed");
    assert_eq!(
        v.search.lineage_multiplicity.judgments_max,
        1 + sched.max_turns,
        "lifetime judgments = the turn-one holdout judgment + the scheduled upgrade turns"
    );

    for (i, turn) in sched.turns.iter().enumerate() {
        assert_eq!(turn.turn, i + 1, "turns are numbered from 1 in order");
        assert!(
            turn.segment_sessions >= sched.segment_min_sessions,
            "turn {} segment {} is below the segment floor {}",
            turn.turn,
            turn.segment_sessions,
            sched.segment_min_sessions
        );
        assert_eq!(turn.bar, margin_bar_n1(turn.segment_sessions), "turn {} bar", turn.turn);
        assert_eq!(
            turn.haircut,
            v.verdict.haircut_fraction * turn.bar,
            "turn {} haircut uses the same frozen fraction",
            turn.turn
        );
        assert_eq!(turn.hurdle, turn.bar + turn.haircut, "turn {} hurdle", turn.turn);
        assert!(
            turn.hurdle < v.hypothesis.effect_size_net_ror,
            "turn {}'s hurdle {} is not clearable at the registered effect {} — a schedule \
             with an unclearable turn is a promise that cannot be kept",
            turn.turn,
            turn.hurdle,
            v.hypothesis.effect_size_net_ror
        );
    }

    assert!(
        sched.exhaustion.to_ascii_lowercase().contains("closure"),
        "exhausting the schedule is stated as a lineage-closure condition: {}",
        sched.exhaustion
    );
}

#[test]
fn u4_the_segment_floor_is_the_smallest_clearable_segment() {
    // KTD13: a segment is only real if the registered effect clears its own bar + haircut
    // at the registered power. The floor is therefore the SMALLEST such segment — assert
    // minimality, not the literal 1,566.
    let v = frozen().values;
    let z = power_z(v.power.power).unwrap();
    let required = |sessions: usize| -> f64 {
        let bar = margin_bar_n1(sessions);
        bar + v.verdict.haircut_fraction * bar + z * se_at(sessions)
    };
    let floor = v.upgrade_schedule.segment_min_sessions;
    let effect = v.hypothesis.effect_size_net_ror;

    assert!(
        required(floor) <= effect + PROBIT_TOL,
        "the registered effect clears the floor segment: required {} vs effect {effect}",
        required(floor)
    );
    assert!(
        required(floor - 1) > effect,
        "segment {} is the SMALLEST clearable one — {} would also clear",
        floor,
        floor - 1
    );
    // KTD13's observation: the floor falls out equal to the turn-one holdout, because the
    // power calculus is the same.
    assert_eq!(
        floor,
        v.holdout().sessions,
        "the segment floor equals the holdout by construction of the same power calculus"
    );
    // And the plan's counter-example: a 500-session segment could never be passed.
    assert!(
        required(500) > effect,
        "a 500-session segment must be unclearable at the registered effect"
    );
}

#[test]
fn u4_expected_max_of_null_is_exactly_zero_at_n_max_one_and_sigma_is_null() {
    let v = frozen().values;
    assert_eq!(v.search.n_max, 1, "one look at the holdout, ever");
    assert!(v.search.sigma_trials.is_none(), "sigma_trials is null at N_max = 1");
    // The reason it may be null: E[max] of a single draw is exactly zero for ANY
    // dispersion, so no value of sigma_trials could change a single judgment.
    for sigma in [0.0, 0.0263679, 1.0, 12.5] {
        assert_eq!(
            expected_max_null(v.search.n_max, sigma).unwrap(),
            0.0,
            "E[max of 1 draw] is exactly zero at sigma = {sigma}"
        );
    }
    assert!(
        expected_max_null(2, 0.05).unwrap() > 0.0,
        "dispersion WOULD enter above one trial — which is what the trigger names"
    );
    assert!(
        v.search.sigma_trials_trigger.to_ascii_lowercase().contains("n_max"),
        "the trigger names the condition that would make sigma load-bearing: {}",
        v.search.sigma_trials_trigger
    );
    assert!(
        v.search
            .lineage_multiplicity
            .lifetime_correction
            .to_ascii_lowercase()
            .contains("none"),
        "the lifetime correction is stated as none, not left to inference: {}",
        v.search.lineage_multiplicity.lifetime_correction
    );
}

#[test]
fn u4_target_participation_is_the_clustering_p_not_the_listing_depth() {
    // KTD8: two different quantities that a single field would conflate.
    let v = frozen().values;
    assert_eq!(
        v.hypothesis.target_session_participation, 1.0,
        "a take-top-N-every-session ranking trades on every session by construction"
    );
    let universe = read_json("pit-universe-20260812.json");
    assert_eq!(
        v.supply.universe_listing_depth,
        universe["derived"]["mean_participation"].as_f64().expect("mean_participation"),
        "listing depth is read from the walk's derived block"
    );
    assert_ne!(
        v.supply.universe_listing_depth, v.hypothesis.target_session_participation,
        "the two quantities are distinct and must not be conflated"
    );
    assert!(
        v.supply.universe_listing_depth_basis.to_ascii_uppercase().contains("UPPER BOUND"),
        "listing depth declares itself a survivorship upper bound: {}",
        v.supply.universe_listing_depth_basis
    );
}

// ===========================================================================
// U5 — claim-then-evaluate refusal
// ===========================================================================

fn ledger_in(tmp: &TempDir) -> JudgmentLedger {
    JudgmentLedger::new(tmp.path().join("ledger/lineage-holdout-judgments.jsonl"))
}

fn judge(
    prereg: &LoadedLineagePreReg,
    ledger: &JudgmentLedger,
    run_id: &str,
    observed: f64,
) -> Result<nautilus_ls_lab::lineage_prereg::HoldoutVerdict, LineagePreRegError> {
    judge_holdout(prereg, ledger, run_id, &"a".repeat(64), "2026-08-15T00:00:00+00:00", observed)
}

#[test]
fn u5_an_empty_ledger_admits_exactly_one_judgment() {
    let tmp = TempDir::new().unwrap();
    let ledger = ledger_in(&tmp);
    let prereg = frozen();

    assert!(ledger.claim().unwrap().is_none(), "an absent ledger reads as unclaimed");
    let verdict = judge(&prereg, &ledger, "run-one", 0.05).expect("the first judgment returns");
    assert_eq!(ledger.read_all().unwrap().len(), 1, "exactly one attempt appended");
    assert_eq!(verdict.cleared, prereg.values.clears(0.05), "the verdict is the predicate");
    assert_eq!(verdict.prereg_content_hash, prereg.content_hash, "the verdict cites the freeze");
}

#[test]
fn u5_a_second_evaluation_errors_naming_the_recorded_run_and_utc() {
    let tmp = TempDir::new().unwrap();
    let ledger = ledger_in(&tmp);
    let prereg = frozen();

    judge(&prereg, &ledger, "run-one", 0.05).expect("first");
    let err = judge(&prereg, &ledger, "run-two", 0.09).expect_err("the second must refuse");
    assert!(
        matches!(&err, LineagePreRegError::AlreadyJudged { run_id, .. } if run_id == "run-one"),
        "{err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("run-one"), "the refusal names the claiming run: {msg}");
    assert!(msg.contains("2026-08-15T00:00:00+00:00"), "and when it claimed: {msg}");
    assert_eq!(ledger.read_all().unwrap().len(), 1, "the refused call appends nothing");
}

#[test]
fn u5_the_claim_survives_a_verdict_that_never_returned() {
    // AE2: the operator evaluates, never writes a verdict back, and tries again. The claim
    // was appended BEFORE the verdict, so the second attempt still refuses. Simulated by
    // appending a claim with no verdict fields — exactly the row `judge_holdout` writes.
    let tmp = TempDir::new().unwrap();
    let ledger = ledger_in(&tmp);
    let prereg = frozen();

    ledger
        .append(&JudgmentAttempt {
            schema_version: JUDGMENT_SCHEMA_VERSION,
            run_id: "crashed-run".to_string(),
            catalog_fingerprint: "b".repeat(64),
            claimed_utc: "2026-08-15T01:02:03+00:00".to_string(),
            prereg_content_hash: prereg.content_hash.clone(),
            observed_net_ror: None,
            cleared: None,
        }, true)
        .unwrap();

    let stored = &ledger.read_all().unwrap()[0];
    assert!(stored.observed_net_ror.is_none(), "the claim carries no verdict");
    let err = judge(&prereg, &ledger, "second-look", 0.09)
        .expect_err("a crash mid-verdict must not buy a second look");
    assert!(
        matches!(&err, LineagePreRegError::AlreadyJudged { run_id, .. } if run_id == "crashed-run"),
        "{err}"
    );
}

#[test]
fn u5_a_partial_ledger_line_refuses_rather_than_reading_as_absent() {
    let prereg = frozen();
    // A line missing everything but a claim timestamp, and a line that is not JSON at all.
    for (label, line) in [
        ("partial payload", "{\"claimed_utc\":\"2026-08-15T00:00:00+00:00\"}"),
        ("torn line", "{\"schema_version\":1,\"run_i"),
        ("not json", "garbage"),
    ] {
        let tmp = TempDir::new().unwrap();
        let ledger = ledger_in(&tmp);
        std::fs::create_dir_all(ledger.path().parent().unwrap()).unwrap();
        std::fs::write(ledger.path(), format!("{line}\n")).unwrap();

        let outcome = judge(&prereg, &ledger, "second-look", 0.09);
        assert!(
            matches!(outcome, Err(LineagePreRegError::AlreadyJudged { .. })),
            "a {label} is still a recorded attempt and must refuse, got {outcome:?}"
        );
        assert_eq!(
            std::fs::read_to_string(ledger.path()).unwrap(),
            format!("{line}\n"),
            "a refused call appends nothing to a {label}"
        );
    }
}

#[test]
fn u5_the_refusal_is_returned_not_logged_and_a_caller_cannot_get_a_second_verdict() {
    let tmp = TempDir::new().unwrap();
    let ledger = ledger_in(&tmp);
    let prereg = frozen();

    let first = judge(&prereg, &ledger, "run-one", 0.05).expect("first");
    // Ten further attempts, each ignoring the previous result. None yields a verdict.
    for i in 0..10 {
        let outcome = judge(&prereg, &ledger, &format!("run-{i}"), 0.09);
        assert!(outcome.is_err(), "attempt {i} obtained a verdict it must not have");
    }
    assert_eq!(ledger.read_all().unwrap().len(), 1, "still exactly one claim");
    assert_eq!(first.run_id, "run-one");
}

#[test]
fn u5_the_frozen_artifacts_content_hash_is_unchanged_by_a_judgment() {
    // AE4 / R12: judgments live in the ledger, so the freeze's bytes never move.
    let tmp = TempDir::new().unwrap();
    let ledger = ledger_in(&tmp);
    let before = frozen().content_hash;
    judge(&frozen(), &ledger, "run-one", 0.05).expect("first");
    let after = frozen().content_hash;
    assert_eq!(before, after, "the committed artifact's citation survives the single judgment");
    assert!(
        frozen().values.holdout_judged.is_none(),
        "and holdout_judged is still null after a judgment"
    );
}

#[test]
fn u5_the_claim_is_atomic_so_a_lost_race_cannot_also_get_a_verdict() {
    // The refusal must not rest on check-then-act. Simulate the interleaving directly:
    // caller A and caller B both observe an unclaimed ledger (both `claim()` calls happen
    // before either append), then both try to append. Exactly one may win.
    let tmp = TempDir::new().unwrap();
    let ledger = ledger_in(&tmp);
    let prereg = frozen();

    assert!(ledger.claim().unwrap().is_none(), "A sees an unclaimed ledger");
    assert!(ledger.claim().unwrap().is_none(), "B sees the same unclaimed ledger");

    let attempt = |run: &str| JudgmentAttempt {
        schema_version: JUDGMENT_SCHEMA_VERSION,
        run_id: run.to_string(),
        catalog_fingerprint: "c".repeat(64),
        claimed_utc: "2026-08-15T00:00:00+00:00".to_string(),
        prereg_content_hash: prereg.content_hash.clone(),
        observed_net_ror: None,
        cleared: None,
    };
    ledger.append(&attempt("racer-a"), true).expect("A wins the exclusive create");
    let err = ledger
        .append(&attempt("racer-b"), true)
        .expect_err("B must lose the race, not append a second claim");
    assert!(
        matches!(&err, LineagePreRegError::AlreadyJudged { run_id, .. } if run_id == "racer-a"),
        "the loser is refused and told who won: {err}"
    );
    assert_eq!(ledger.read_all().unwrap().len(), 1, "exactly one claim survives the race");
}

#[test]
fn u5_a_blank_ledger_is_a_claim_that_died_mid_write_not_an_unclaimed_one() {
    // `append` creates the file exclusively BEFORE writing, so a zero-byte ledger is the
    // fingerprint of a run that claimed and then crashed. Reading it as unclaimed would
    // hand out a second verdict for the one failure the claim-first ordering exists to
    // survive.
    let prereg = frozen();
    for (label, contents) in [("zero-byte", ""), ("newline-only", "\n"), ("blank lines", "\n\n  \n")] {
        let tmp = TempDir::new().unwrap();
        let ledger = ledger_in(&tmp);
        std::fs::create_dir_all(ledger.path().parent().unwrap()).unwrap();
        std::fs::write(ledger.path(), contents).unwrap();

        assert!(
            ledger.claim().unwrap().is_some(),
            "a {label} ledger is a crashed claim, not an absent one"
        );
        let outcome = judge(&prereg, &ledger, "second-look", 0.09);
        assert!(
            matches!(outcome, Err(LineagePreRegError::AlreadyJudged { .. })),
            "a {label} ledger must refuse, got {outcome:?}"
        );
    }
}

#[test]
fn u5_an_unreadable_ledger_refuses_rather_than_reading_as_unjudged() {
    // The fail-open that a `Path::exists()` pre-check would introduce: `exists()` maps a
    // permission error to `false`, so an unreadable ledger would read as "never judged".
    // Only NotFound may mean absent.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let ledger = ledger_in(&tmp);
        std::fs::create_dir_all(ledger.path().parent().unwrap()).unwrap();
        std::fs::write(ledger.path(), "{\"schema_version\":1}\n").unwrap();
        std::fs::set_permissions(ledger.path(), std::fs::Permissions::from_mode(0o000)).unwrap();

        // Root ignores the mode bits, so only assert when the chmod actually bites.
        if std::fs::read_to_string(ledger.path()).is_err() {
            let err = ledger.claim().expect_err("an unreadable ledger must refuse");
            assert!(matches!(err, LineagePreRegError::Ledger { .. }), "{err}");
            let outcome = judge(&frozen(), &ledger, "second-look", 0.09);
            assert!(outcome.is_err(), "and no verdict is produced: {outcome:?}");
        }
        std::fs::set_permissions(ledger.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn u3_a_renamed_or_unknown_field_is_refused_not_silently_defaulted() {
    // Serde reads a MISSING `Option` field as `None`, so without `deny_unknown_fields` a
    // rename would drop the unknown key, default the real field, and load a drifted
    // artifact clean — the exact silent edit the freeze exists to prevent.
    let tmp = TempDir::new().unwrap();
    let original = std::fs::read_to_string(frozen_lineage_prereg_path()).unwrap();

    let renamed = original.replace("\"holdout_judged\": null", "\"holdoutJudged\": null");
    assert_ne!(renamed, original, "the rename fixture actually changed something");
    let p = tmp.path().join("renamed.json");
    std::fs::write(&p, &renamed).unwrap();
    let err = load(&p).expect_err("a renamed field must not load clean");
    assert!(matches!(err, LineagePreRegError::Parse { .. }), "typed refusal: {err}");

    // A newer schema version is a typed refusal, never a silent partial read.
    let bumped = original.replace("\"schema_version\": 1,", "\"schema_version\": 2,");
    let p2 = tmp.path().join("v2.json");
    std::fs::write(&p2, &bumped).unwrap();
    let err2 = load(&p2).expect_err("an unsupported schema version must refuse");
    assert!(
        matches!(&err2, LineagePreRegError::Invariant { detail } if detail.contains("schema version 2")),
        "{err2}"
    );
}

#[test]
fn u7_the_turn_log_cites_the_committed_artifacts_real_content_hash() {
    // The freeze entry and its staged opening text both quote the artifact's SHA-256. A
    // hash transcribed into prose is exactly the citation that rots silently on the next
    // edit — which is the failure the content-hash mechanic exists to prevent, so it must
    // not be reintroduced by the record OF that mechanic.
    let turn_log = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("TURN-LOG.md"),
    )
    .expect("the turn log is committed");
    let hash = frozen().content_hash;
    let cites = turn_log.matches(hash.as_str()).count();
    assert!(
        cites >= 2,
        "the freeze entry and its staged opening block must both cite the artifact's real \
         hash {hash}; found {cites} citation(s)"
    );
    assert!(
        turn_log.contains("currently open: NONE"),
        "the standing block still reads 'currently open: NONE' — this freeze does not open \
         the lineage (KTD11)"
    );
}

#[test]
fn the_companion_citation_helper_is_not_itself_vacuous() {
    // This helper is the guard's guard — if it matched substrings, every citation check
    // built on it would pass regardless of what the companion says.
    assert!(contains_number("the hold is 16 sessions", 16));
    assert!(contains_number("(2460)", 2460));
    assert!(contains_number("m = 8.", 8));
    assert!(!contains_number("2026-08-12", 8), "must not match a digit inside a date");
    assert!(!contains_number("concurrency 128", 8), "must not match the tail of a longer number");
    assert!(!contains_number("0.856407", 8), "must not match inside a decimal");
    assert!(!contains_number("a 16.5 session hold", 16), "must not match a truncated decimal");
    assert!(!contains_number("2,460 sessions", 2), "must not match across a thousands separator");
}

#[test]
fn u5_the_specification_dry_run_refuses_holdout_dates() {
    let v = frozen().values;
    // Inside the specification window: a verdict, no ledger, no claim.
    let ok = specification_dry_run(&v, v.specification().from, v.specification().to, 0.05)
        .expect("a specification-window dry run is permitted");
    assert_eq!(ok, v.clears(0.05), "the dry run uses the same predicate");

    // Any holdout or reserved date refuses.
    for (date, expected) in [
        (v.holdout().from, "holdout"),
        (v.holdout().to, "holdout"),
        (v.reserved().from, "reserved"),
    ] {
        let err = specification_dry_run(&v, v.specification().from, date, 0.05)
            .expect_err("the dry run must refuse a non-specification date");
        assert!(
            matches!(&err, LineagePreRegError::DryRunOutsideSpecification { partition, .. }
                if partition == expected),
            "{date} should refuse as {expected}: {err}"
        );
    }
    // And a date past the ceiling entirely.
    let past = v.reserved().to.succ_opt().unwrap();
    assert!(specification_dry_run(&v, past, past, 0.05).is_err(), "past the ceiling refuses");
}

#[test]
fn u5_the_committed_judgment_ledger_never_exceeds_one_attempt() {
    // The freeze must not ship a spent holdout — but this gate has to keep telling the
    // truth AFTER the one legitimate judgment lands, or it reds the tree permanently at
    // exactly the moment the artifact does its job. So assert the durable invariant
    // (`N_max = 1`, and any recorded attempt binds to THIS freeze), not "never judged".
    let ledger = JudgmentLedger::new(nautilus_ls_lab::lineage_prereg::judgment_ledger_path());
    let attempts = ledger.read_all().expect("the committed ledger reads");
    assert!(
        attempts.len() <= 1,
        "N_max = 1: the holdout admits one attempt, found {}",
        attempts.len()
    );
    if let Some(attempt) = attempts.first() {
        assert_eq!(
            attempt.prereg_content_hash,
            frozen().content_hash,
            "a recorded judgment must cite the committed freeze it was taken under"
        );
        assert_eq!(attempt.schema_version, JUDGMENT_SCHEMA_VERSION);
    }
}

// ===========================================================================
// U6 — the prose companion cites only frozen numbers
// ===========================================================================

#[test]
fn u6_every_number_the_companion_cites_appears_in_the_frozen_artifact() {
    let companion = std::fs::read_to_string(config_path("LINEAGE-PREREGISTRATION.md"))
        .expect("the prose companion is committed");
    let v: LineagePreRegistration = frozen().values;

    // Each load-bearing figure must appear in the prose as a STANDALONE number, so a value
    // cannot drift between the artifact and its explanation. `contains_number` is the
    // point: a bare substring check passes vacuously here, because a companion full of
    // dates like `2026-08-12` already contains "8", "2", "1", and "16" many times over.
    for (what, n) in [
        ("S_max", v.supply.s_max as u64),
        ("holdout sessions", v.holdout().sessions as u64),
        ("specification sessions", v.specification().sessions as u64),
        ("reserved sessions", v.reserved().sessions as u64),
        ("holding period", v.hypothesis.holding_period_sessions as u64),
        ("target m", v.hypothesis.target_m as u64),
        ("concurrency", v.hypothesis.steady_state_concurrency as u64),
        ("listed count min", v.supply.listed_count_min as u64),
        ("max turns", v.upgrade_schedule.max_turns as u64),
        ("judgments max", v.search.lineage_multiplicity.judgments_max as u64),
        ("segment floor", v.upgrade_schedule.segment_min_sessions as u64),
    ] {
        assert!(
            contains_number(&companion, n),
            "the companion must cite the frozen {what} ({n}) as a standalone number"
        );
    }
    // And the named honesty claims the guard cannot check by arithmetic.
    for phrase in ["inferred", "git-auditable", "citation-reproducible", "005930"] {
        assert!(
            companion.to_ascii_lowercase().contains(&phrase.to_ascii_lowercase()),
            "the companion must state '{phrase}' plainly"
        );
    }
}
