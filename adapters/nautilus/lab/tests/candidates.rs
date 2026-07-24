//! Candidate pre-register integration tests (U4). The schema/loader/freeze
//! mechanics are unit-tested in `src/candidates.rs`; this asserts the committed
//! tracked example candidate loads and round-trips (R1's git-tracked home).

use std::path::Path;

use nautilus_ls_lab::candidates::{load, Candidate, Comparator, PhaseAClass};

/// The tracked example candidate dir, resolved at test-compile time.
fn example_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("candidates/example")
}

fn gap_retention_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("candidates/opening-range-gap-retention")
}

fn profit_target_075_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("candidates/profit-target-075")
}

#[test]
fn the_tracked_example_candidate_loads() {
    // Its declared diagnostic + twin content hashes must match the committed
    // scripts on disk — a drift here means the example was edited without
    // re-freezing its hashes (the exact failure the freeze discipline catches).
    let loaded = load(&example_dir()).expect("the tracked example candidate loads");
    assert_eq!(loaded.values.slug, "example");
    assert_eq!(loaded.values.family, "class-b");
    assert_eq!(loaded.values.phase_a, PhaseAClass::Bespoke);
    assert!(loaded.values.flip_matches("ratio_atr_alpha", 0.5));
    // The frozen-input set names the scripts, never the gate-verdict output.
    let inputs = loaded.frozen_inputs();
    assert!(inputs.iter().any(|p| p.ends_with("diagnostic.py")));
    assert!(inputs.iter().any(|p| p.ends_with("twin.py")));
    assert!(!inputs.iter().any(|p| p.ends_with("gate-verdict.json")));
}

#[test]
fn the_example_round_trips_through_serde() {
    let loaded = load(&example_dir()).unwrap();
    let json = serde_json::to_string(&loaded.values).unwrap();
    let back: Candidate = serde_json::from_str(&json).unwrap();
    assert_eq!(back, loaded.values, "the example candidate round-trips");
}

#[test]
fn the_profit_target_075_candidate_is_frozen() {
    // The exit-geometry Phase-A candidate (plan 2026-07-24-001): a bespoke
    // direction+materiality gate on profit_target_r, NOT a sizing collinearity gate.
    // Its declared diagnostic + twin content hashes must match the committed scripts
    // (the freeze-discipline drift check).
    let loaded = load(&profit_target_075_dir()).expect("the profit-target-075 candidate loads");
    assert_eq!(loaded.values.slug, "profit-target-075");
    assert_eq!(loaded.values.family, "exit-geometry");
    assert_eq!(loaded.values.phase_a, PhaseAClass::Bespoke);
    assert!(loaded.values.flip_matches("profit_target_r", 0.75));
    assert!(!loaded.values.flip_matches("profit_target_r", 1.0), "the head value is not the flip");

    // Both frozen thresholds are the direction + materiality pair (R4).
    let by_reading = |name: &str| {
        loaded
            .values
            .thresholds
            .iter()
            .find(|t| t.reading == name)
            .unwrap_or_else(|| panic!("threshold on {name} present"))
            .clone()
    };
    let dir = by_reading("ror_delta");
    assert_eq!(dir.comparator, Comparator::Ge);
    assert_eq!(dir.value, 0.00065);
    let mat = by_reading("exit_change_frac");
    assert_eq!(mat.comparator, Comparator::Ge);
    assert_eq!(mat.value, 0.05);
    assert_eq!(loaded.values.thresholds.len(), 2, "exactly the direction + materiality pair");

    // The frozen-input set names the scripts, never the gate-verdict output.
    let inputs = loaded.frozen_inputs();
    assert!(inputs.iter().any(|p| p.ends_with("diagnostic.py")));
    assert!(inputs.iter().any(|p| p.ends_with("twin.py")));
    assert!(!inputs.iter().any(|p| p.ends_with("gate-verdict.json")));
}

#[test]
fn the_opening_range_gap_retention_candidate_is_frozen() {
    let loaded = load(&gap_retention_dir()).expect("the gap-retention candidate loads");
    assert_eq!(loaded.values.slug, "opening-range-gap-retention");
    assert_eq!(loaded.values.family, "entry-filter");
    assert_eq!(loaded.values.phase_a, PhaseAClass::Bespoke);
    assert!(loaded.values.flip_matches("gap_retention_min", 0.50));
    assert_eq!(loaded.values.readings.len(), 11);
    assert_eq!(loaded.values.thresholds.len(), 16);
}
