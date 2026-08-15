//! **Operator step** for the lineage pre-registration freeze (U1 of plan 2026-08-14-001).
//!
//! This harness counts the supply split against the REAL KRX calendar snapshot and prints
//! the numbers plus the snapshot's two identities, for transcription into
//! `config/lineage-preregistration.json`.
//!
//! It is `#[ignore]`d and stays that way. The snapshot at
//! `adapters/nautilus/state/krx.calendar.json` is machine-local and gitignored — CI checks
//! out a tree without it — so the split counts are **citation-reproducible** (via the
//! `artifact_id` / `calendar_id` this prints and the artifact records), not
//! test-reproducible. The hermetic half of U1 lives in `lineage_prereg_derivation.rs` and
//! runs on synthetic facts with no snapshot present.
//!
//! Run it by hand at freeze time, and again whenever the artifact's
//! `rederivation_trigger` fires:
//!
//! ```text
//! cd adapters/nautilus
//! LS_CALENDAR_SNAPSHOT="$PWD/state/krx.calendar.json" \
//!   cargo test -p nautilus-ls-lab --test lineage_prereg_derive -- --ignored --nocapture
//! ```
//!
//! **Pass an ABSOLUTE path.** Cargo runs a test binary with the working directory set to
//! the *crate* root (`adapters/nautilus/lab`), not the adapter root, so the usual relative
//! `state/krx.calendar.json` resolves to nothing here and the snapshot reads as
//! unavailable.

use chrono::{NaiveDate, Utc};

use nautilus_ls_lab::dispatch::checks::date_fact_from_view;
use nautilus_ls_lab::lineage_prereg::{count_proven_sessions, derive_split, SplitAnchors};

/// The frozen anchors (KTD4). The floor is the daily-catalog floor the P3 pull reached;
/// the ceiling is the P4 walk's own anchor.
const FLOOR: (i32, u32, u32) = (2016, 8, 1);
const SPECIFICATION_TO: (i32, u32, u32) = (2019, 12, 31);
const HOLDOUT_FROM: (i32, u32, u32) = (2020, 1, 2);
const HOLDOUT_TO: (i32, u32, u32) = (2026, 5, 20);
const RESERVED_FROM: (i32, u32, u32) = (2026, 5, 21);
const CEILING: (i32, u32, u32) = (2026, 8, 12);

/// The origin plan counted to this date; the P4 walk counted to `CEILING` (KTD4).
const ORIGIN_TO: (i32, u32, u32) = (2026, 8, 7);
const ORIGIN_DELTA_FROM: (i32, u32, u32) = (2026, 8, 8);

fn d(ymd: (i32, u32, u32)) -> NaiveDate {
    NaiveDate::from_ymd_opt(ymd.0, ymd.1, ymd.2).expect("a real calendar date")
}

#[test]
#[ignore = "operator step: needs the machine-local, gitignored calendar snapshot"]
fn operator_step_print_the_split_and_the_calendar_identities() {
    let as_of = Utc::now();
    let path = nautilus_ls::calendar::snapshot_path_from_env().expect(
        "set LS_CALENDAR_SNAPSHOT=state/krx.calendar.json (relative to adapters/nautilus)",
    );
    let loaded = nautilus_ls::calendar::resolve_and_load(
        Some(&path),
        as_of,
        nautilus_ls::calendar::adoption_from_env(),
    );
    let calendar = match &loaded {
        nautilus_ls::calendar::LoadedCalendar::Available(cal) => cal,
        nautilus_ls::calendar::LoadedCalendar::Unavailable(err) => panic!(
            "calendar snapshot at {} did not load: {err}\n\
             (pass an ABSOLUTE path — the test binary's CWD is the lab crate root)",
            path.display()
        ),
        nautilus_ls::calendar::LoadedCalendar::NotConfigured => {
            panic!("LS_CALENDAR_SNAPSHOT is unset or empty")
        }
    };
    let view = calendar.as_of(as_of).expect("snapshot covers the current instant");
    let fact_for = |date: NaiveDate| date_fact_from_view(Some(&view), date);

    let anchors = SplitAnchors {
        floor: d(FLOOR),
        specification_to: d(SPECIFICATION_TO),
        holdout_from: d(HOLDOUT_FROM),
        holdout_to: d(HOLDOUT_TO),
        reserved_from: d(RESERVED_FROM),
        ceiling: d(CEILING),
    };
    let split = derive_split(anchors, &fact_for).expect("the frozen anchors derive a split");

    // KTD4 — the origin-vs-walk ceiling gap attributed to its date range. The origin plan
    // counted 2,457 through 2026-08-07; the walk counted 2,460 through its own anchor.
    let origin = count_proven_sessions(d(FLOOR), d(ORIGIN_TO), &fact_for).expect("origin range counts");
    let delta = count_proven_sessions(d(ORIGIN_DELTA_FROM), d(CEILING), &fact_for)
        .expect("delta range counts");

    println!("=== lineage pre-registration — operator step (U1) ===");
    println!("calendar artifact_id: {}", calendar.artifact_id());
    println!("calendar calendar_id: {}", calendar.calendar_id());
    println!(
        "ceiling  {} ..= {}  S_max = {}",
        split.from, split.to, split.s_max
    );
    for part in [&split.specification, &split.holdout, &split.reserved] {
        println!(
            "  {:<13} {} ..= {}  sessions = {:>5}  (first {} .. last {})",
            part.partition.name(),
            part.from,
            part.to,
            part.sessions,
            part.first_session,
            part.last_session
        );
    }
    println!(
        "origin ceiling {} ..= {} = {}; delta {} ..= {} = {}",
        split.from,
        d(ORIGIN_TO),
        origin,
        d(ORIGIN_DELTA_FROM),
        split.to,
        delta
    );

    // The Goal Capsule's stop condition: if the SPECIFICATION or HOLDOUT count moves off
    // 837 / 1,566 the margin bar moves, and four downstream frozen figures move with it.
    // A moved RESERVED tail is expected and does not stop the run.
    assert_eq!(split.specification.sessions, 837, "specification window (stop condition)");
    assert_eq!(split.holdout.sessions, 1_566, "holdout (stop condition)");
    assert_eq!(split.s_max, 2_460, "S_max matches the P4 walk's proven_sessions");
    assert_eq!(
        split.specification.sessions + split.holdout.sessions + split.reserved.sessions,
        split.s_max,
        "the split sums to the ceiling"
    );
    assert_eq!(origin, 2_457, "the origin plan's ceiling reproduces at its own end date");
    assert_eq!(
        split.s_max - origin,
        delta,
        "the origin-vs-walk delta is exactly the sessions in {}..={}",
        d(ORIGIN_DELTA_FROM),
        split.to
    );
}
