//! Derivation guard for the head-**v34** re-registration (v2) of the FROZEN pre-registration
//! mirror (`config/preregistration.json`).
//!
//! This reproduces the Protective-formula derivation of every economic expectation band from
//! v34's rolling-5 backtest constants, so the frozen numbers are **derived and auditable, not
//! typed in** (KTD1 / the repo's "no invented numbers" discipline). The rolling-5 constants
//! (−1,483,240 / +1,772,900) are the audited derivation inputs, cross-checked against
//! `config/PREREGISTRATION.md § Re-registration v2 (head v34)`; the test stays hermetic and does
//! NOT re-read the live run dir (KTD1).

use std::path::PathBuf;

use nautilus_ls_lab::dispatch::prereg::load;

/// v34 rolling 5-session cumulative-P&L constants (KRW, full size), from
/// `20260724T014752Z-backtest-orb-v34` over 24 KST sessions — the documented derivation inputs.
const V34_WORST_ROLL5: f64 = -1_483_240.0;
const V34_BEST_ROLL5: f64 = 1_772_900.0;

/// The Protective rounding: nearest 1,000.
fn round_to_1000(x: f64) -> f64 {
    (x / 1_000.0).round() * 1_000.0
}

/// floor_r = worst_roll5 × f, rounded to nearest 1,000 (the "don't escalate against a bleeding
/// edge" guard).
fn protective_floor(fraction: f64) -> f64 {
    round_to_1000(V34_WORST_ROLL5 * fraction)
}

/// ceil_r = best_roll5 × 1.5 × f, rounded to nearest 1,000 (the runaway check).
fn protective_ceiling(fraction: f64) -> f64 {
    round_to_1000(V34_BEST_ROLL5 * 1.5 * fraction)
}

fn frozen_prereg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config").join("preregistration.json")
}

#[test]
fn every_v34_band_reproduces_from_the_protective_formula() {
    let v = load(&frozen_prereg_path()).expect("frozen preregistration.json parses").values;

    // Each rung's frozen band must equal the Protective-formula output from the v34 rolling-5
    // constants (four rungs × floor+ceiling assertions).
    for (rung, fraction) in [(1u8, 0.10), (2u8, 0.25), (3u8, 0.50), (4u8, 1.00)] {
        assert_eq!(
            v.rung_fraction(rung).unwrap(),
            fraction,
            "rung {rung} dose fraction (KTD6)"
        );
        let band = v.expectation_band(rung).unwrap();
        assert_eq!(
            band.min_cum_pnl,
            protective_floor(fraction),
            "rung {rung} floor = round_to_1000(worst_roll5 × {fraction})"
        );
        assert_eq!(
            band.max_cum_pnl,
            protective_ceiling(fraction),
            "rung {rung} ceiling = round_to_1000(best_roll5 × 1.5 × {fraction})"
        );
    }
}

#[test]
fn v2_file_loads_with_v34_rung_1_economics() {
    let loaded = load(&frozen_prereg_path()).unwrap();
    let v = &loaded.values;

    assert_eq!(v.version, 2, "re-registered to v2 for head v34");
    assert_eq!(v.rung_fraction(1).unwrap(), 0.10, "rung-1 dose unchanged v30→v34 (KTD5)");
    let band = v.expectation_band(1).unwrap();
    assert_eq!(band.min_cum_pnl, -148_000.0, "v34 rung-1 floor");
    assert_eq!(band.max_cum_pnl, 266_000.0, "v34 rung-1 ceiling (halved from v30's +533k)");
    assert_eq!(
        v.session_max_loss_krw().unwrap(),
        300_000.0,
        "breaker stands at 300k for rung 1 (KTD3)"
    );
    assert_eq!(loaded.content_hash.len(), 64, "SHA-256 hex citation is produced");
}

#[test]
fn v34_rung_1_floor_loosens_below_the_v30_floor() {
    // Sanity: the re-freeze documents the loosening it introduces — the v34 rung-1 floor is
    // strictly below the v30 floor (−148,000 < −69,000). v34's worse-and-wider distribution
    // widens the band; this is the deliberate correctness fix, not band-fitting.
    const V30_RUNG1_FLOOR: f64 = -69_000.0;
    let v = load(&frozen_prereg_path()).unwrap().values;
    let v34_floor = v.expectation_band(1).unwrap().min_cum_pnl;
    assert!(
        v34_floor < V30_RUNG1_FLOOR,
        "v34 rung-1 floor {v34_floor} must be below the v30 floor {V30_RUNG1_FLOOR}"
    );
}

#[test]
fn editing_the_file_changes_the_content_hash() {
    // Citation integrity (mirror of `content_hash_tracks_the_exact_file_bytes`): a byte change to
    // the frozen file changes the SHA-256 every dispatch record cites, so a silent edit cannot
    // masquerade under the old citation.
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    let original = std::fs::read(frozen_prereg_path()).unwrap();
    let a = tmp.path().join("a.json");
    let b = tmp.path().join("b.json");
    std::fs::write(&a, &original).unwrap();
    // Flip one byte's worth of content (bump a value) → different citation.
    let mutated = String::from_utf8(original).unwrap().replace("\"version\": 2", "\"version\": 3");
    std::fs::write(&b, mutated).unwrap();
    let ha = load(&a).unwrap().content_hash;
    let hb = load(&b).unwrap().content_hash;
    assert_ne!(ha, hb, "editing the file changes the citation");
    assert_eq!(ha, load(&a).unwrap().content_hash, "same bytes → same citation");
}
