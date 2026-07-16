//! Candidate pre-register integration tests (U4). The schema/loader/freeze
//! mechanics are unit-tested in `src/candidates.rs`; this asserts the committed
//! tracked example candidate loads and round-trips (R1's git-tracked home).

use std::path::Path;

use nautilus_ls_lab::candidates::{load, Candidate, PhaseAClass};

/// The tracked example candidate dir, resolved at test-compile time.
fn example_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("candidates/example")
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
