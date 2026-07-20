//! Closeout publication-boundary scan (U4, R17/KTD9; AE8).
//!
//! The public closeout record (`adapters/nautilus/CLOSEOUT.md`) is **verdict-only**: it carries
//! gate names, pass/hold verdicts, and software/schema versions — never a snapshot identity, an
//! affected real (KRX-derived) date, or an owner-local canary fact. This test machine-enforces
//! that boundary so the gate fails on a violation instead of relying on human review: a
//! committed closeout that leaks a snapshot/artifact-identity hash or an ISO calendar date
//! reddens `cargo test` (and therefore both `make foundation-gate` and `make adapter-check`).
//!
//! The scan targets VALUES, not vocabulary: prose may name the `artifact_id` field, but a hash
//! value (a long hex run) or an ISO date (`YYYY-MM-DD`) is forbidden. Software/schema versions
//! (`1.0.0`) contain no long hex run and no ISO date, so they pass.

use std::fs;
use std::path::PathBuf;

/// The committed closeout lives one level up in the adapter workspace.
fn closeout_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../CLOSEOUT.md")
}

/// The first ISO calendar date (`YYYY-MM-DD`) in `s`, if any. A snapshot's affected real dates
/// and any owner-local gate-run date are KRX/owner-local facts that must not be published.
fn find_iso_date(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let is_digit = |b: u8| b.is_ascii_digit();
    // Scan for D D D D - D D - D D with digit boundaries so a longer number run does not match.
    for i in 0..bytes.len().saturating_sub(9) {
        let w = &bytes[i..i + 10];
        let shaped = is_digit(w[0])
            && is_digit(w[1])
            && is_digit(w[2])
            && is_digit(w[3])
            && w[4] == b'-'
            && is_digit(w[5])
            && is_digit(w[6])
            && w[7] == b'-'
            && is_digit(w[8])
            && is_digit(w[9]);
        let left_boundary = i == 0 || !is_digit(bytes[i - 1]);
        let right_boundary = i + 10 >= bytes.len() || !is_digit(bytes[i + 10]);
        if shaped && left_boundary && right_boundary {
            return Some(String::from_utf8_lossy(w).into_owned());
        }
    }
    None
}

/// The first long lowercase/uppercase hex run (>= 16 chars) in `s`, if any — the shape of a
/// snapshot `artifact_id`/`calendar_id` or a `redacted-sha256:` fingerprint. A four-part
/// version string never reaches 16 contiguous hex chars.
fn find_hash(s: &str) -> Option<String> {
    let mut run = String::new();
    let mut best: Option<String> = None;
    let flush = |run: &mut String, best: &mut Option<String>| {
        if run.len() >= 16 && best.is_none() {
            *best = Some(run.clone());
        }
        run.clear();
    };
    for ch in s.chars() {
        if ch.is_ascii_hexdigit() {
            run.push(ch);
        } else {
            flush(&mut run, &mut best);
        }
    }
    flush(&mut run, &mut best);
    best
}

#[test]
fn closeout_is_verdict_only_no_snapshot_identity_or_dates() {
    let text = fs::read_to_string(closeout_path()).expect("adapters/nautilus/CLOSEOUT.md present");

    if let Some(date) = find_iso_date(&text) {
        panic!(
            "CLOSEOUT.md contains an ISO calendar date ({date}) — the publication boundary is \
             verdict-only (R17/KTD9): no affected real date or owner-local gate-run date may be \
             committed. Record only gate verdicts and software/schema versions."
        );
    }
    if let Some(hash) = find_hash(&text) {
        panic!(
            "CLOSEOUT.md contains a snapshot/artifact-identity hash ({}…) — the publication \
             boundary is verdict-only (R17/KTD9): no snapshot identity may be committed.",
            &hash[..hash.len().min(12)]
        );
    }
}

#[cfg(test)]
mod scan_self_tests {
    use super::{find_hash, find_iso_date};

    #[test]
    fn iso_dates_are_detected_versions_are_not() {
        assert!(find_iso_date("gate ran on 2026-07-20").is_some());
        assert!(find_iso_date("schema version 1.0.0, no dates").is_none());
        assert!(find_iso_date("rung 1 of 5, 300000 KRW").is_none());
    }

    #[test]
    fn hashes_are_detected_versions_are_not() {
        assert!(find_hash("artifact_id 502f0e4529216bde is local-only").is_some());
        assert!(find_hash("redacted-sha256:aabbccddeeff0011").is_some());
        assert!(find_hash("schema 1.0.0, gate PASS, adapter 0.1.0").is_none());
    }
}
