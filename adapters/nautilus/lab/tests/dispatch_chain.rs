//! U1 — the append-only, hash-chained dispatch chain store (KTD1, KTD2).
//!
//! Every fail-closed arm is force-executed: an empty chain, a truncated final record, a
//! flipped byte in an old record, a broken `prev_hash` link, and an unknown record type
//! each verify as defective and authorize rung 0. Repair is an epoch rollover that
//! archives the defective file content-hashed and never rewrites it. Consumption is
//! single-use; an unconsumed green dispatch from a prior KST day is expired. A KRX
//! session spanning UTC midnight keys on one KST trading date. No secret survives into a
//! chain record byte.

use chrono::{DateTime, TimeZone, Utc};
use nautilus_ls::lock::{is_held, AdvisoryLock, LockKind};
use nautilus_ls_lab::dispatch::chain::{
    ChainRecord, ChainStatus, Consumption, DeEscalation, DispatchChain, DispatchOutcome,
    Escalation, MountAuthz, RecordBody, RecordKind, SafetyTrip, SafetyTripKind, SessionDispatch,
    TripAction, GENESIS_PREV_HASH,
};
use tempfile::TempDir;

fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
}

fn a_day() -> DateTime<Utc> {
    utc(2026, 7, 16, 1, 0) // 10:00 KST — mid-session
}

fn empty_dispatch() -> SessionDispatch {
    SessionDispatch {
        outcome: DispatchOutcome::Green,
        checks: Vec::new(),
        deferrals: Vec::new(),
        readiness: None,
        unknown_override: None,
    }
}

#[test]
fn empty_chain_authorizes_rung_zero() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    let state = chain.load();
    assert_eq!(state.status, ChainStatus::NoChain);
    assert_eq!(state.authorized_rung, 0, "no chain -> suspended, never an implicit default");
    assert_eq!(state.mount_authz("2026-07-16"), MountAuthz::None);
}

#[test]
fn genesis_authorizes_rung_one() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    let state = chain.load();
    assert_eq!(state.status, ChainStatus::Valid);
    assert_eq!(state.authorized_rung, 1);
}

#[test]
fn genesis_escalation_deescalation_sequence_authorizes_expected_rung() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain
        .append(
            a_day(),
            2,
            2,
            None,
            RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec!["r1".into()] }),
        )
        .unwrap();
    chain
        .append(
            a_day(),
            3,
            3,
            None,
            RecordKind::Escalation(Escalation { from_rung: 2, to_rung: 3, evidence_run_ids: vec!["r2".into()] }),
        )
        .unwrap();
    assert_eq!(chain.load().authorized_rung, 3);

    chain
        .append(
            a_day(),
            2,
            2,
            None,
            RecordKind::DeEscalation(DeEscalation {
                from_rung: 3,
                to_rung: 2,
                events: vec!["watchdog fired".into()],
                consumed_through: "2026-07-16-0002".into(),
            }),
        )
        .unwrap();
    assert_eq!(chain.load().authorized_rung, 2, "de-escalation steps back one rung");
}

#[test]
fn truncated_final_record_verifies_defective_rung_zero() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::SessionDispatch(empty_dispatch())).unwrap();

    // Chop the final line mid-JSON (the exact state a crash mid-append leaves).
    let text = std::fs::read_to_string(chain.chain_path()).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    let last = lines.pop().unwrap();
    let torn = &last[..last.len() / 2];
    let mut rebuilt = lines.join("\n");
    rebuilt.push('\n');
    rebuilt.push_str(torn); // no trailing newline — a torn final line
    std::fs::write(chain.chain_path(), rebuilt).unwrap();

    let state = chain.load();
    assert!(matches!(state.status, ChainStatus::Defective(_)), "{:?}", state.status);
    assert_eq!(state.authorized_rung, 0);
}

#[test]
fn flipped_byte_in_old_record_verifies_defective_rung_zero() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain
        .append(
            a_day(),
            2,
            2,
            None,
            RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec![] }),
        )
        .unwrap();

    // Mutate a hashed body value in the first (old) record while keeping JSON valid:
    // its stored record_hash no longer matches the recomputed hash -> tamper.
    let text = std::fs::read_to_string(chain.chain_path()).unwrap();
    let tampered = text.replacen("\"chain_rung\":1", "\"chain_rung\":4", 1);
    assert_ne!(text, tampered, "the replacement must have taken");
    std::fs::write(chain.chain_path(), tampered).unwrap();

    let state = chain.load();
    assert!(matches!(state.status, ChainStatus::Defective(_)), "{:?}", state.status);
    assert_eq!(state.authorized_rung, 0);
}

#[test]
fn broken_prev_hash_link_verifies_defective_rung_zero() {
    // Force-execute the LINK arm specifically: a record whose body hashes correctly
    // (so the per-record hash check passes) but whose prev_hash does not match the
    // predecessor.
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    let genesis = chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();

    let body = RecordBody {
        record_id: "2026-07-16-0001".into(),
        kst_trading_date: "2026-07-16".into(),
        prev_hash: "deadbeef_not_the_predecessor".into(), // wrong link
        chain_rung: 2,
        effective_rung: 2,
        prereg_hash: None,
        kind: RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec![] }),
    };
    let sealed = ChainRecord::sealed(body); // correct record_hash for this body
    assert_ne!(sealed.body.prev_hash, genesis.record_hash);
    let line = format!("{}\n", serde_json::to_string(&sealed).unwrap());
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new().append(true).open(chain.chain_path()).unwrap();
    f.write_all(line.as_bytes()).unwrap();
    drop(f);

    let state = chain.load();
    match state.status {
        ChainStatus::Defective(ref why) => assert!(why.contains("link"), "{why}"),
        other => panic!("expected a link defect, got {other:?}"),
    }
    assert_eq!(state.authorized_rung, 0);
}

#[test]
fn unknown_record_type_verifies_defective_rung_zero() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();

    // A newer-schema producer's record type this reader does not know.
    let text = std::fs::read_to_string(chain.chain_path()).unwrap();
    let bogus = text.replacen("\"genesis\"", "\"quantum_rung\"", 1);
    assert_ne!(text, bogus);
    std::fs::write(chain.chain_path(), bogus).unwrap();

    let state = chain.load();
    assert!(matches!(state.status, ChainStatus::Defective(_)), "{:?}", state.status);
    assert_eq!(state.authorized_rung, 0);
}

#[test]
fn reregister_after_defect_rolls_epoch_archiving_content_hashed() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain
        .append(
            a_day(),
            2,
            2,
            None,
            RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec![] }),
        )
        .unwrap();

    // Corrupt the chain.
    let good = std::fs::read_to_string(chain.chain_path()).unwrap();
    let corrupted = good.replacen("\"chain_rung\":1", "\"chain_rung\":9", 1);
    std::fs::write(chain.chain_path(), &corrupted).unwrap();
    assert_eq!(chain.load().authorized_rung, 0, "corrupt chain suspends");

    // Repair: epoch rollover.
    let rr = chain.reregister(a_day(), 1, None, "operator repair after tamper").unwrap();
    let RecordKind::ReRegistration(rr_payload) = &rr.body.kind else {
        panic!("re-registration record expected");
    };
    let archive_hash = rr_payload.archived_epoch_hash.clone().expect("archived epoch hash cited");

    // The archived file preserves the DEFECTIVE bytes byte-for-byte, content-hashed.
    let archived = chain.dir().join("archive").join(format!("chain.{archive_hash}.jsonl"));
    assert!(archived.exists(), "defective epoch archived, never deleted");
    assert_eq!(std::fs::read_to_string(&archived).unwrap(), corrupted, "archive is unmodified");

    // The new epoch authorizes again from the re-registration.
    let state = chain.load();
    assert_eq!(state.status, ChainStatus::Valid);
    assert_eq!(state.authorized_rung, 1);
    // The new epoch's first record is the re-registration citing the archive hash.
    assert!(matches!(state.records[0].body.kind, RecordKind::ReRegistration(_)));
}

#[test]
fn true_genesis_prev_hash_is_the_sentinel() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    let g = chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    assert_eq!(g.body.prev_hash, GENESIS_PREV_HASH);
}

#[test]
fn concurrent_append_refused_while_dispatch_lock_held() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    // Hold the dispatch lock manually (a second concurrent gate process).
    let _held = AdvisoryLock::acquire(chain.dir(), LockKind::Dispatch).unwrap();
    let err = chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap_err();
    assert!(err.to_string().contains("lock"), "{err}");
}

#[test]
fn safety_trip_append_succeeds_while_live_lock_held() {
    // KTD2: Dispatch has no counterpart, so a safety-trip append from the live session
    // process (which holds the Live lock) is permitted mid-session.
    let tmp = TempDir::new().unwrap();
    let catalog = tmp.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    let _live = AdvisoryLock::acquire(&catalog, LockKind::Live).unwrap();
    assert!(is_held(&catalog, LockKind::Live), "gate would refuse a new attempt");

    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    chain
        .append(
            a_day(),
            1,
            1,
            None,
            RecordKind::SafetyTrip(SafetyTrip {
                trip: SafetyTripKind::Watchdog,
                action: TripAction::Engage,
                run_id: Some("run-x".into()),
                detail: "dead-man fired".into(),
            }),
        )
        .expect("safety-trip append is permitted while Live lock is held");
}

#[test]
fn consumed_dispatch_cannot_authorize_a_second_session() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    let day = a_day();
    let today = nautilus_ls_lab::dispatch::chain::kst_trading_date(day);
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(day, 1, 1, None, RecordKind::SessionDispatch(empty_dispatch())).unwrap();

    // Unconsumed, same-day -> ready.
    let state = chain.load();
    let record_id = match state.mount_authz(&today) {
        MountAuthz::Ready { record_id, chain_rung, effective_rung } => {
            assert_eq!(chain_rung, 1);
            assert_eq!(effective_rung, 1);
            record_id
        }
        other => panic!("expected Ready, got {other:?}"),
    };

    // Consume it, then it cannot authorize a second session.
    chain
        .append(
            day,
            1,
            1,
            None,
            RecordKind::Consumption(Consumption { dispatch_record_id: record_id, run_id: Some("run-1".into()) }),
        )
        .unwrap();
    let state = chain.load();
    assert!(state.last_session_dispatch.as_ref().unwrap().consumed);
    assert_eq!(state.mount_authz(&today), MountAuthz::Consumed);
}

#[test]
fn unconsumed_green_dispatch_from_a_previous_kst_day_is_expired() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    let day = a_day();
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(day, 1, 1, None, RecordKind::SessionDispatch(empty_dispatch())).unwrap();

    let state = chain.load();
    // Same day -> Ready; the next KST day -> Expired.
    assert!(matches!(state.mount_authz("2026-07-16"), MountAuthz::Ready { .. }));
    assert_eq!(state.mount_authz("2026-07-17"), MountAuthz::Expired);
}

#[test]
fn krx_session_spanning_utc_midnight_carries_one_kst_trading_date() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    // 23:59 UTC (08:59 KST next day) and 00:01 UTC next day (09:01 KST) — the same KRX
    // premarket→open session, one KST trading date.
    let before_mid = utc(2026, 7, 15, 23, 59);
    let after_mid = utc(2026, 7, 16, 0, 1);
    let g = chain.append(before_mid, 1, 1, None, RecordKind::Genesis).unwrap();
    let d = chain.append(after_mid, 1, 1, None, RecordKind::SessionDispatch(empty_dispatch())).unwrap();
    assert_eq!(g.body.kst_trading_date, "2026-07-16");
    assert_eq!(d.body.kst_trading_date, "2026-07-16");
}

#[test]
fn no_secret_survives_into_a_chain_record_byte() {
    let tmp = TempDir::new().unwrap();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(a_day(), 1, 1, None, RecordKind::Genesis).unwrap();
    // Plant an account number and a bearer-like token in free-text payload fields.
    chain
        .append(
            a_day(),
            1,
            1,
            None,
            RecordKind::SafetyTrip(SafetyTrip {
                trip: SafetyTripKind::KillSwitch,
                action: TripAction::Engage,
                run_id: None,
                detail: "acct 20187511401 token abcdefghijklmnopqrstuvwx1234 tripped".into(),
            }),
        )
        .unwrap();
    chain
        .append(
            a_day(),
            0,
            0,
            None,
            RecordKind::ReRegistration(nautilus_ls_lab::dispatch::chain::ReRegistration {
                set_rung: 1,
                archived_epoch_hash: None,
                reason: "cleared for acct 20187511401".into(),
            }),
        )
        .unwrap();

    let bytes = std::fs::read_to_string(chain.chain_path()).unwrap();
    assert!(!bytes.contains("20187511401"), "account number leaked: {bytes}");
    assert!(!bytes.contains("abcdefghijklmnopqrstuvwx1234"), "bearer token leaked");
    assert!(bytes.contains("***"), "scrub marker present");
}
