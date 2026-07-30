//! U3 — sequence state readers (R10, KTD7): read-only adapters over the
//! heterogeneous existing stores (dispatch chain, ingest checkpoint, run
//! registry + trials ledger + stage log) that report "in-flight sequence +
//! stage + resume command" without ever mutating a store.
//!
//! Pinned contracts: a defective chain surfaces the fail-closed rung-0 verdict
//! as the chain machinery reports it (never re-derived, never an error);
//! entirely missing stores read as "no in-flight sequences"; a `.tmp-` aborted
//! run dir is reported and left untouched; turns are one-shot, so a mid-way
//! turn's resume command is the recorded next `turn governed` invocation.

use chrono::{DateTime, TimeZone, Utc};
use nautilus_ls_lab::dispatch::chain::{
    Consumption, DispatchChain, DispatchOutcome, RecordKind, SafetyTrip, SafetyTripKind,
    SessionDispatch, TripAction,
};
use nautilus_ls_lab::queue::sequences::{
    ingest_sequence, ladder_sequence, read_sequences, turn_sequence, SequenceKind, SequenceStores,
};
use nautilus_ls_lab::trials::{LookKind, SampleLineage, TrialRecord, TrialsLedger};
use std::collections::BTreeMap;
use std::path::Path;
use tempfile::TempDir;

fn a_day() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap() // 10:00 KST — mid-session
}

fn green_dispatch() -> SessionDispatch {
    SessionDispatch {
        outcome: DispatchOutcome::Green,
        checks: Vec::new(),
        deferrals: Vec::new(),
        readiness: None,
        unknown_override: None,
    }
}

fn stores(data_home: Option<&Path>, ledger: Option<&Path>, stage_log: Option<&Path>) -> SequenceStores {
    SequenceStores {
        data_home: data_home.map(Path::to_path_buf),
        trials_ledger: ledger.map(Path::to_path_buf),
        stage_log: stage_log.map(Path::to_path_buf),
    }
}

// ---------------------------------------------------------------------------
// No stores at all — the pre-first-sequence state must be quiet, not an error.
// ---------------------------------------------------------------------------

#[test]
fn missing_stores_entirely_report_no_in_flight_sequences() {
    // No data home configured at all.
    let none = read_sequences(&stores(None, None, None), a_day());
    assert!(none.is_empty(), "no stores -> no in-flight sequences, got {none:?}");

    // A data home that exists but has never hosted a chain, catalog, or run.
    let tmp = TempDir::new().unwrap();
    let ledger_path = tmp.path().join("ledger/trials.jsonl"); // never written
    let out = read_sequences(&stores(Some(tmp.path()), Some(&ledger_path), None), a_day());
    assert!(out.is_empty(), "empty data home -> no in-flight sequences, got {out:?}");
}

// ---------------------------------------------------------------------------
// AE1 — a mid-way turn names the turn, its stage, and the resume command.
// ---------------------------------------------------------------------------

#[test]
fn midway_turn_names_the_turn_stage_and_resume_command() {
    let tmp = TempDir::new().unwrap();
    // Fixture registry: one finalized run plus an aborted `.tmp-` residue dir —
    // the mid-way turn's crash marker.
    let runs = tmp.path().join("runs");
    std::fs::create_dir_all(runs.join("20260715T010000Z-backtest-orb-v34")).unwrap();
    std::fs::create_dir_all(runs.join(".tmp-20260716T010000Z-backtest-orb-v35")).unwrap();

    // Stage log: the governed run recorded stages up to `rebaseline`.
    let stage_log = tmp.path().join("stagelog.txt");
    std::fs::write(&stage_log, "bump\nrebaseline\n").unwrap();

    // Trials ledger: the last recorded look names the candidate the turn ran.
    let ledger_path = tmp.path().join("ledger/trials.jsonl");
    let ledger = TrialsLedger::new(&ledger_path);
    ledger
        .append(&TrialRecord::new(
            "orb-slot-ranking",
            "class-b",
            LookKind::GateReading,
            SampleLineage { catalog_fingerprint: "f".repeat(64), parent_fingerprint: None },
            BTreeMap::new(),
            "GO",
            "2026-07-16T00:30:00+00:00",
        ))
        .unwrap();

    let turn = turn_sequence(&stores(Some(tmp.path()), Some(&ledger_path), Some(&stage_log)))
        .expect("a mid-way turn is an in-flight sequence");
    assert_eq!(turn.kind, SequenceKind::Turn);
    assert_eq!(turn.kind.tag(), "turn", "report names the turn sequence");
    assert!(turn.stage.contains("rebaseline"), "stage from the stage log: {}", turn.stage);
    assert!(
        turn.resume.contains("turn governed"),
        "resume is the recorded next one-shot turn invocation (KTD7): {}",
        turn.resume
    );
    assert!(
        turn.resume.contains("orb-slot-ranking"),
        "resume carries the recorded candidate: {}",
        turn.resume
    );
}

// ---------------------------------------------------------------------------
// AE5 — a consumed-but-unfinished session prep names the prep sequence,
// its recorded stage, and the resume step.
// ---------------------------------------------------------------------------

#[test]
fn consumed_but_unfinished_session_prep_reports_stage_and_resume_step() {
    let tmp = TempDir::new().unwrap();
    let day = a_day();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    let d = chain.append(day, 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();
    chain
        .append(
            day,
            1,
            1,
            None,
            RecordKind::Consumption(Consumption {
                dispatch_record_id: d.body.record_id.clone(),
                run_id: Some("20260716T001000Z-live-orb-v34".into()),
            }),
        )
        .unwrap();
    // No finalized `runs/20260716T001000Z-live-orb-v34` exists — the mounted
    // session never finished: the prep sequence is in flight.

    let ladder = ladder_sequence(tmp.path(), day).expect("consumed-but-unfinished prep is in flight");
    assert_eq!(ladder.kind, SequenceKind::Ladder);
    assert!(
        ladder.stage.contains(&d.body.record_id) && ladder.stage.contains("consumed"),
        "stage names the consumed dispatch: {}",
        ladder.stage
    );
    assert!(
        ladder.stage.contains("20260716T001000Z-live-orb-v34"),
        "stage names the mounted run: {}",
        ladder.stage
    );
    assert!(!ladder.resume.is_empty(), "a resume step is always named");
    assert!(
        ladder.resume.contains("lab-live"),
        "resume is an executable step, not a runbook pointer alone: {}",
        ladder.resume
    );
}

#[test]
fn green_unconsumed_dispatch_offers_the_mount_step() {
    let tmp = TempDir::new().unwrap();
    let day = a_day();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(day, 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();

    let ladder = ladder_sequence(tmp.path(), day).expect("an unconsumed green dispatch is in flight");
    assert!(ladder.stage.contains("unconsumed"), "stage: {}", ladder.stage);
    assert!(ladder.resume.contains("--mount"), "resume is the mount step: {}", ladder.resume);
}

#[test]
fn finished_session_prep_is_not_in_flight() {
    // Consumption whose run id IS finalized in the registry: the prep sequence
    // completed — nothing to resume.
    let tmp = TempDir::new().unwrap();
    let day = a_day();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    let d = chain.append(day, 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();
    chain
        .append(
            day,
            1,
            1,
            None,
            RecordKind::Consumption(Consumption {
                dispatch_record_id: d.body.record_id.clone(),
                run_id: Some("20260716T001000Z-live-orb-v34".into()),
            }),
        )
        .unwrap();
    std::fs::create_dir_all(tmp.path().join("runs/20260716T001000Z-live-orb-v34")).unwrap();

    assert_eq!(ladder_sequence(tmp.path(), day), None, "finalized session -> prep not in flight");
}

#[test]
fn engaged_kill_switch_after_an_abnormal_finalized_session_is_reported_not_silent() {
    // ABNORMAL exit-72 shape: consumed dispatch, the run FINALIZED under the
    // registry, and the kill switch tripped ENGAGED on the way down. Without
    // the switch this reads "prep complete" (None) — with it engaged the
    // report must name the switch and the deliberate clear step, never
    // "no in-flight sequences".
    let tmp = TempDir::new().unwrap();
    let day = a_day();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    let d = chain.append(day, 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();
    chain
        .append(
            day,
            1,
            1,
            None,
            RecordKind::Consumption(Consumption {
                dispatch_record_id: d.body.record_id.clone(),
                run_id: Some("20260716T001000Z-live-orb-v34".into()),
            }),
        )
        .unwrap();
    chain
        .append(
            day,
            1,
            1,
            None,
            RecordKind::SafetyTrip(SafetyTrip {
                trip: SafetyTripKind::KillSwitch,
                action: TripAction::Engage,
                run_id: Some("20260716T001000Z-live-orb-v34".into()),
                detail: "session max-loss breaker escalated to kill switch".into(),
            }),
        )
        .unwrap();
    std::fs::create_dir_all(tmp.path().join("runs/20260716T001000Z-live-orb-v34")).unwrap();

    let ladder = ladder_sequence(tmp.path(), day)
        .expect("an engaged kill switch is a reportable state, not 'no in-flight sequences'");
    assert_eq!(ladder.kind, SequenceKind::Ladder);
    assert!(
        ladder.stage.contains("kill switch ENGAGED"),
        "stage names the engaged switch: {}",
        ladder.stage
    );
    assert!(
        ladder.resume.contains("--clear-killswitch"),
        "resume is the deliberate clear step, never a fresh --dispatch: {}",
        ladder.resume
    );
    assert!(
        ladder.detail.iter().any(|d| d.contains("kill switch ENGAGED")),
        "the existing kill-switch detail line rides along: {:?}",
        ladder.detail
    );
}

// ---------------------------------------------------------------------------
// A defective chain surfaces the rung-0 fail-closed verdict — never an error.
// ---------------------------------------------------------------------------

#[test]
fn defective_chain_surfaces_rung0_fail_closed_verdict_without_error() {
    let tmp = TempDir::new().unwrap();
    let day = a_day();
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(day, 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();

    // Tamper a hashed body byte while keeping the JSON valid (the dispatch_chain.rs idiom).
    let text = std::fs::read_to_string(chain.chain_path()).unwrap();
    let tampered = text.replacen("\"chain_rung\":1", "\"chain_rung\":4", 1);
    assert_ne!(text, tampered, "the tamper must have taken");
    std::fs::write(chain.chain_path(), tampered).unwrap();

    let ladder = ladder_sequence(tmp.path(), day).expect("a defective chain is a reportable state");
    assert_eq!(ladder.kind, SequenceKind::Ladder);
    assert!(
        ladder.stage.contains("rung 0"),
        "the fail-closed rung-0 verdict is surfaced as the chain machinery reports it: {}",
        ladder.stage
    );
    assert!(
        ladder.resume.contains("--reregister"),
        "resume is the epoch-rollover repair step: {}",
        ladder.resume
    );
}

// ---------------------------------------------------------------------------
// `.tmp-` aborted-run residue: reported, never touched.
// ---------------------------------------------------------------------------

#[test]
fn tmp_aborted_run_dir_is_reported_and_left_untouched() {
    let tmp = TempDir::new().unwrap();
    let aborted = tmp.path().join("runs/.tmp-20260716T020000Z-backtest-orb-v35");
    std::fs::create_dir_all(&aborted).unwrap();
    std::fs::write(aborted.join("manifest.json"), "{}").unwrap();

    let out = read_sequences(&stores(Some(tmp.path()), None, None), a_day());
    let turn = out
        .iter()
        .find(|s| s.kind == SequenceKind::Turn)
        .expect("aborted residue makes the turn leg in-flight: {out:?}");
    assert!(
        turn.detail.iter().any(|d| d.contains("20260716T020000Z-backtest-orb-v35")),
        "the aborted run id is reported: {:?}",
        turn.detail
    );

    // Report-only: the residue dir and its contents are untouched.
    assert!(aborted.exists(), "the .tmp- dir must never be deleted");
    assert!(aborted.join("manifest.json").exists(), "residue contents untouched");
}

// ---------------------------------------------------------------------------
// Ingest leg: watermark / basis-shift (refusal) state from the checkpoint.
// ---------------------------------------------------------------------------

#[test]
fn ingest_checkpoint_watermarks_and_shift_marks_are_reported() {
    let tmp = TempDir::new().unwrap();
    let catalog = tmp.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(
        catalog.join("ingest-checkpoint.json"),
        r#"{
            "completed": [],
            "gaps": [],
            "watermarks": {
                "KR7005930003.KRX|1-DAY-LAST": "20260725",
                "KR7000660001.KRX|1-DAY-LAST": "20260728"
            },
            "shifted": { "KR7005930003.KRX|1-DAY-LAST": "20260720" },
            "adjusted_prices": true
        }"#,
    )
    .unwrap();

    let ingest = ingest_sequence(tmp.path()).expect("a checkpoint on disk is reportable state");
    assert_eq!(ingest.kind, SequenceKind::Ingest);
    assert!(ingest.stage.contains("2"), "watermark count reported: {}", ingest.stage);
    assert!(
        ingest.stage.contains("20260725") && ingest.stage.contains("20260728"),
        "the watermark frontier range is reported: {}",
        ingest.stage
    );
    assert!(
        ingest.detail.iter().any(|d| d.contains("KR7005930003.KRX|1-DAY-LAST")),
        "the basis-shift (append-refusal) mark is reported: {:?}",
        ingest.detail
    );
    assert!(ingest.resume.contains("ls-ingest"), "resume names the tool: {}", ingest.resume);
}

#[test]
fn unreadable_ingest_checkpoint_is_reported_not_an_error() {
    let tmp = TempDir::new().unwrap();
    let catalog = tmp.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(catalog.join("ingest-checkpoint.json"), "{ torn").unwrap();

    let ingest = ingest_sequence(tmp.path()).expect("an unreadable checkpoint is still a report");
    assert!(ingest.stage.contains("unreadable"), "stage says why: {}", ingest.stage);
    // Report-only: the corrupt file is left exactly as found.
    assert_eq!(std::fs::read_to_string(catalog.join("ingest-checkpoint.json")).unwrap(), "{ torn");
}

// ---------------------------------------------------------------------------
// Composition: every in-flight leg appears once, in a deterministic order.
// ---------------------------------------------------------------------------

#[test]
fn read_sequences_composes_all_legs_deterministically() {
    let tmp = TempDir::new().unwrap();
    let day = a_day();
    // Ladder: green unconsumed dispatch.
    let chain = DispatchChain::open(tmp.path()).unwrap();
    chain.append(day, 1, 1, None, RecordKind::Genesis).unwrap();
    chain.append(day, 1, 1, None, RecordKind::SessionDispatch(green_dispatch())).unwrap();
    // Ingest: an empty-but-present checkpoint.
    let catalog = tmp.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(catalog.join("ingest-checkpoint.json"), "{\"adjusted_prices\":false}").unwrap();
    // Turn: aborted residue.
    std::fs::create_dir_all(tmp.path().join("runs/.tmp-20260716T020000Z-backtest-orb-v35")).unwrap();

    let st = stores(Some(tmp.path()), None, None);
    let first = read_sequences(&st, day);
    let second = read_sequences(&st, day);
    assert_eq!(first, second, "the report is deterministic");
    let kinds: Vec<SequenceKind> = first.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![SequenceKind::Turn, SequenceKind::Ladder, SequenceKind::Ingest],
        "R10 legs in a fixed order"
    );
}
