//! The frozen turn-3 liquid symbol universe (U1, KTD-1, R2/R3).
//!
//! A one-time live `t1444` (시가총액상위 — KOSPI top-market-cap) capture materializes
//! a provenance-stamped list of ~30 KOSPI shcodes; the committed file is the
//! reproducible artifact `ls-ingest` consumes via `LS_INGEST_SYMBOLS`. This module
//! owns the file schema and its **pure** validation ([`UniverseFile::validate`]) so
//! the capture binary can fail closed before writing and the offline test suite can
//! pin the schema without a network call.
//!
//! Selecting by *current* market cap for a *past* backtest window is a mild
//! look-ahead — disclosed in [`Provenance::look_ahead_caveat`] and accepted for a
//! first decisive read (KTD-1). `t1444` is not promoted by this capture.

use serde::{Deserialize, Serialize};

/// The minimum number of valid shcodes a frozen universe must carry (R2 / the U1
/// stop condition: fewer than ~20 valid names halts the turn).
pub const MIN_SHCODES: usize = 20;

/// The TR the universe is captured from (KTD-1). Pinned so a hand-edited file with
/// a wrong provenance is rejected.
pub const SOURCE_TR: &str = "t1444";

/// Provenance for a frozen capture (R3) — enough to reproduce and audit it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// The source TR — must be [`SOURCE_TR`] (`t1444`).
    pub source_tr: String,
    /// The concrete `upcode` (업종코드) used, e.g. `"001"` for the KOSPI composite.
    pub upcode: String,
    /// A human label for the `upcode` (e.g. `"코스피종합 (KOSPI composite)"`).
    pub upcode_label: String,
    /// RFC-3339 capture timestamp.
    pub captured_at: String,
    /// The declared N (must equal the shcode count).
    pub count: usize,
    /// The N the operator requested (`LS_CAPTURE_N`), recorded so a legitimately
    /// short board (captured `count` < `requested_n`, override-allowed) is audit-
    /// visible (KTD-5). `#[serde(default)]` = 0 so legacy captures (no field) load.
    #[serde(default)]
    pub requested_n: usize,
    /// The disclosed current-market-cap look-ahead caveat (KTD-1).
    pub look_ahead_caveat: String,
}

/// A frozen liquid-universe file: provenance + the ranked shcode list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseFile {
    /// Capture provenance (R3).
    pub provenance: Provenance,
    /// The shcodes, in server-returned (market-cap-descending) order.
    pub shcodes: Vec<String>,
}

/// The short-freeze guard (KTD-5, R12): a capture that froze fewer than the
/// requested N is the silent-truncation risk that started this incident (a 20-of-40
/// freeze fed a plan needing 40). Returns `Err` naming both counts — the caller must
/// exit non-zero and write nothing — unless `allow_short` (an explicit operator
/// override for a board that legitimately ends early). `Ok(())` = safe to write.
pub fn check_capture_completeness(
    captured: usize,
    requested: usize,
    allow_short: bool,
) -> Result<(), String> {
    if captured < requested && !allow_short {
        return Err(format!(
            "short freeze: captured {captured} of {requested} requested — refusing to write a \
             silently-truncated universe (the t1444 walk did not reach the requested N; the header \
             pagination fix may have regressed). If this board legitimately ends early, set \
             LS_CAPTURE_ALLOW_SHORT=1 to override (provenance then records requested_n)."
        ));
    }
    Ok(())
}

impl UniverseFile {
    /// Validate the frozen file (R2/R3): `>= MIN_SHCODES` shcodes, every one a
    /// 6-digit numeric code, no duplicates, and complete provenance whose `count`
    /// matches the list and whose `source_tr` is `t1444`. Returns every violation so
    /// the operator sees the full picture in one pass.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();

        if self.shcodes.len() < MIN_SHCODES {
            errs.push(format!(
                "too few shcodes: {} < {} (the frozen universe must hold at least ~20 valid names)",
                self.shcodes.len(),
                MIN_SHCODES
            ));
        }
        for (i, code) in self.shcodes.iter().enumerate() {
            if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
                errs.push(format!("shcode[{i}] {code:?} is not a 6-digit numeric code"));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for code in &self.shcodes {
            if !seen.insert(code) {
                errs.push(format!("duplicate shcode {code:?}"));
            }
        }

        let p = &self.provenance;
        if p.source_tr != SOURCE_TR {
            errs.push(format!("provenance.source_tr {:?} must be {SOURCE_TR:?}", p.source_tr));
        }
        if p.upcode.trim().is_empty() {
            errs.push("provenance.upcode is empty".to_string());
        }
        if p.captured_at.trim().is_empty() {
            errs.push("provenance.captured_at is empty".to_string());
        }
        if p.count != self.shcodes.len() {
            errs.push(format!(
                "provenance.count {} does not match the shcode list length {}",
                p.count,
                self.shcodes.len()
            ));
        }
        if p.look_ahead_caveat.trim().is_empty() {
            errs.push("provenance.look_ahead_caveat is empty (the current-market-cap look-ahead must be disclosed)".to_string());
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// The shcodes joined for `LS_INGEST_SYMBOLS` (comma-separated, no spaces).
    pub fn ingest_symbols(&self) -> String {
        self.shcodes.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance(count: usize) -> Provenance {
        Provenance {
            source_tr: SOURCE_TR.to_string(),
            upcode: "001".to_string(),
            upcode_label: "코스피종합 (KOSPI composite)".to_string(),
            captured_at: "2026-07-07T00:00:00Z".to_string(),
            count,
            requested_n: count,
            look_ahead_caveat: "current-market-cap selection for a past window; disclosed (KTD-1)".to_string(),
        }
    }

    /// A valid file of `n` distinct 6-digit codes.
    fn valid_file(n: usize) -> UniverseFile {
        let shcodes: Vec<String> = (0..n).map(|i| format!("{:06}", 100 + i)).collect();
        UniverseFile { provenance: provenance(shcodes.len()), shcodes }
    }

    #[test]
    fn a_well_formed_capture_of_thirty_validates() {
        let f = valid_file(30);
        assert!(f.validate().is_ok(), "{:?}", f.validate());
        // Round-trips through JSON (the on-disk form).
        let json = serde_json::to_string_pretty(&f).unwrap();
        let back: UniverseFile = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn exactly_the_floor_validates() {
        assert!(valid_file(MIN_SHCODES).validate().is_ok());
    }

    #[test]
    fn too_few_shcodes_is_rejected() {
        let f = valid_file(MIN_SHCODES - 1);
        let errs = f.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("too few shcodes")), "{errs:?}");
    }

    #[test]
    fn a_non_six_digit_code_is_rejected() {
        let mut f = valid_file(25);
        f.shcodes[3] = "05930".to_string(); // 5 digits
        f.shcodes[7] = "00593A".to_string(); // non-numeric
        let errs = f.validate().unwrap_err();
        assert_eq!(errs.iter().filter(|e| e.contains("not a 6-digit")).count(), 2, "{errs:?}");
    }

    #[test]
    fn a_duplicate_shcode_is_rejected() {
        let mut f = valid_file(25);
        f.shcodes[10] = f.shcodes[0].clone();
        let errs = f.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("duplicate shcode")), "{errs:?}");
    }

    #[test]
    fn a_wrong_source_tr_is_rejected() {
        let mut f = valid_file(25);
        f.provenance.source_tr = "t1463".to_string();
        let errs = f.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("source_tr")), "{errs:?}");
    }

    #[test]
    fn a_count_mismatch_is_rejected() {
        let mut f = valid_file(25);
        f.provenance.count = 24; // list is 25
        let errs = f.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("does not match")), "{errs:?}");
    }

    #[test]
    fn missing_provenance_fields_are_rejected() {
        let mut f = valid_file(25);
        f.provenance.upcode = String::new();
        f.provenance.captured_at = "  ".to_string();
        f.provenance.look_ahead_caveat = String::new();
        let errs = f.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("upcode is empty")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("captured_at is empty")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("look_ahead_caveat is empty")), "{errs:?}");
    }

    #[test]
    fn ingest_symbols_is_comma_joined() {
        let f = valid_file(3);
        assert_eq!(f.ingest_symbols(), "000100,000101,000102");
    }

    // --- KTD-5 short-freeze guard ---

    #[test]
    fn full_capture_passes_the_completeness_guard() {
        // captured == requested → write.
        assert!(check_capture_completeness(40, 40, false).is_ok());
        // captured > requested (a board over-served) is never a short freeze.
        assert!(check_capture_completeness(41, 40, false).is_ok());
    }

    #[test]
    fn short_capture_is_refused_and_names_both_counts() {
        // captured < requested, no override → refuse, naming the 20/40 shortfall.
        let err = check_capture_completeness(20, 40, false).unwrap_err();
        assert!(err.contains("20 of 40"), "message names both counts: {err}");
        assert!(err.contains("LS_CAPTURE_ALLOW_SHORT"), "message names the override: {err}");
    }

    #[test]
    fn short_capture_is_allowed_under_the_override() {
        // captured < requested WITH override → allowed (a legitimately short board).
        assert!(check_capture_completeness(20, 40, true).is_ok());
    }

    #[test]
    fn legacy_file_without_requested_n_loads_as_zero() {
        // A pre-KTD-5 committed universe file has no `requested_n`; it must still
        // deserialize (the field defaults to 0), so old captures keep loading.
        let json = r#"{
            "provenance": {
                "source_tr": "t1444", "upcode": "001", "upcode_label": "x",
                "captured_at": "2026-07-07T00:00:00Z", "count": 20,
                "look_ahead_caveat": "disclosed"
            },
            "shcodes": []
        }"#;
        let f: UniverseFile = serde_json::from_str(json).expect("legacy file loads");
        assert_eq!(f.provenance.requested_n, 0, "absent requested_n defaults to 0");
        assert_eq!(f.provenance.count, 20);
    }
}
