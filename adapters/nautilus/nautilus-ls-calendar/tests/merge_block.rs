//! Mechanical merge-block (U5, KTD1/R7; the plan's central safety guarantee made technical).
//!
//! A consumer's weekday primitive (and its `Legacy | Shadow` arm) may be deleted ONLY when that
//! consumer's committed, non-sensitive gate-verdict record
//! (`adapters/nautilus/gate-verdicts/<consumer>.json`) is present and `PASS`. That verdict is
//! written only after the live, operator-attended Consumer Retirement Gate (owner-local canary,
//! restart-after-activation, rehearsed rollback; the Ladder also an attended paper-session
//! preflight). This turns "merge is blocked on the recorded gate" into a TECHNICAL gate — a
//! batch merge or an uninformed approver cannot remove a Legacy fallback before its live canary
//! runs.
//!
//! ## Two checks, deliberately split
//!
//! - [`merge_block_blocks_deletion_without_a_pass_verdict`] tests the pure coupling RULE both
//!   directions (deletion-without-PASS is blocked; deletion-with-PASS is allowed). It runs in
//!   the default gate (`make foundation-gate` / `make adapter-check`) so the rule itself is
//!   always covered.
//! - [`tree_respects_the_merge_block`] enforces the rule against the REAL tree, and is
//!   `#[ignore]`d so it is NOT part of the default gate — a staged retirement diff therefore
//!   keeps `cargo test --workspace` GREEN (the retirement code is correct) while
//!   `make merge-block-check` (this test, run with `--ignored`, wired into CI) goes RED until
//!   the operator records the consumer's PASS verdict. Recording that PASS is the merge trigger.

use std::fs;
use std::path::PathBuf;

/// Resolve an adapter-workspace path (relative to this crate's manifest dir).
fn resolve(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// The committed gate verdict for a consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The live Consumer Retirement Gate is recorded as passed — deletion is authorized.
    Pass,
    /// The gate is on hold (default): the consumer stays Shadow, Legacy authoritative.
    Hold,
}

/// Read a consumer's verdict record. `None` when the record is absent or carries neither token.
fn read_verdict(rel: &str) -> Option<Verdict> {
    let text = fs::read_to_string(resolve(rel)).ok()?;
    // Manual scan (no serde dep in this leaf crate). PASS wins if both somehow appear.
    let has = |needle: &str| text.contains(needle);
    if has("\"verdict\": \"PASS\"") || has("\"verdict\":\"PASS\"") {
        Some(Verdict::Pass)
    } else if has("\"verdict\": \"HOLD\"") || has("\"verdict\":\"HOLD\"") {
        Some(Verdict::Hold)
    } else {
        None
    }
}

/// The merge-block RULE: a consumer's weekday primitive may be deleted ONLY when its committed
/// gate-verdict record is present and `PASS`. While the primitive is still present (the
/// Shadow/Legacy phase) any verdict state is fine — nothing has been retired.
fn weekday_retirement_allowed(primitive_present: bool, verdict: Option<Verdict>) -> Result<(), String> {
    if primitive_present {
        return Ok(());
    }
    match verdict {
        Some(Verdict::Pass) => Ok(()),
        Some(Verdict::Hold) => {
            Err("weekday primitive deleted but the gate-verdict record is HOLD".to_string())
        }
        None => Err("weekday primitive deleted but no gate-verdict record is present".to_string()),
    }
}

/// One consumer boundary and the marker whose disappearance means its weekday primitive was
/// retired, plus the verdict record that must then be PASS.
struct Consumer {
    /// The `gate-verdicts/<name>.json` stem (also the human label).
    name: &'static str,
    /// The source file the primitive lives in (adapter-relative).
    primitive_file: &'static str,
    /// A distinctive marker present iff the weekday primitive still exists.
    primitive_marker: &'static str,
    /// The committed gate-verdict record (adapter-relative).
    verdict_file: &'static str,
}

/// The four Consumer Retirement Gates (KTD3: the three ingest boundaries share one process and
/// one gate). Each marker is a distinctive token the corresponding Phase C retirement diff
/// deletes.
const CONSUMERS: &[Consumer] = &[
    Consumer {
        name: "ingest",
        primitive_file: "../src/ingest/checkpoint.rs",
        primitive_marker: "fn weekday_strictly_between",
        verdict_file: "../gate-verdicts/ingest.json",
    },
    Consumer {
        name: "catalog",
        primitive_file: "../lab/src/runner/research.rs",
        primitive_marker: "fn last_weekday_on_or_before",
        verdict_file: "../gate-verdicts/catalog.json",
    },
    Consumer {
        name: "budget-probe",
        primitive_file: "../src/bin/budget-probe.rs",
        primitive_marker: "fn recent_trading_day",
        verdict_file: "../gate-verdicts/budget-probe.json",
    },
    Consumer {
        name: "ladder",
        primitive_file: "../lab/src/runner/live.rs",
        primitive_marker: "WeekdayKrxCalendar.date_fact(now_utc)",
        verdict_file: "../gate-verdicts/ladder.json",
    },
];

#[test]
fn merge_block_blocks_deletion_without_a_pass_verdict() {
    // Primitive still present (Shadow/Legacy phase) → always allowed, whatever the verdict.
    assert!(weekday_retirement_allowed(true, None).is_ok());
    assert!(weekday_retirement_allowed(true, Some(Verdict::Hold)).is_ok());
    assert!(weekday_retirement_allowed(true, Some(Verdict::Pass)).is_ok());

    // Primitive deleted → allowed ONLY with a present-and-PASS verdict.
    assert!(
        weekday_retirement_allowed(false, None).is_err(),
        "deletion without a verdict record must be blocked"
    );
    assert!(
        weekday_retirement_allowed(false, Some(Verdict::Hold)).is_err(),
        "deletion with a HOLD verdict must be blocked"
    );
    assert!(
        weekday_retirement_allowed(false, Some(Verdict::Pass)).is_ok(),
        "deletion with a PASS verdict is authorized"
    );
}

/// Every consumer's verdict record parses to a definite verdict (present + well-formed), so the
/// tree check below can never silently pass on a malformed record.
#[test]
fn every_consumer_has_a_parseable_verdict_record() {
    let mut missing = Vec::new();
    for c in CONSUMERS {
        if read_verdict(c.verdict_file).is_none() {
            missing.push(format!("{} ({})", c.name, c.verdict_file));
        }
    }
    assert!(
        missing.is_empty(),
        "these gate-verdict records are absent or unparseable: {}",
        missing.join(", ")
    );
}

/// The REAL-tree merge-block. `#[ignore]`d so it is not part of the default gate (a staged
/// retirement diff keeps `cargo test --workspace` green); run it via `make merge-block-check`
/// (and CI) with `--ignored`. It fails if any consumer's weekday primitive is gone while its
/// verdict record is absent or HOLD.
#[test]
#[ignore = "tree-state merge-block; run via `make merge-block-check` / CI, not the default gate"]
fn tree_respects_the_merge_block() {
    let mut failures = Vec::new();
    for c in CONSUMERS {
        let present = fs::read_to_string(resolve(c.primitive_file))
            .map(|s| s.contains(c.primitive_marker))
            .unwrap_or(false);
        let verdict = read_verdict(c.verdict_file);
        if let Err(reason) = weekday_retirement_allowed(present, verdict) {
            failures.push(format!(
                "[{}] {reason} — marker {:?} absent from {}; record {} must be present and PASS",
                c.name, c.primitive_marker, c.primitive_file, c.verdict_file
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "mechanical merge-block violated ({} consumer(s)):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
