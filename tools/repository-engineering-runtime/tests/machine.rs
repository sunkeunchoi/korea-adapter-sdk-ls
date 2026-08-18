use repository_engineering_runtime::machine::SweepMachine;
use repository_engineering_runtime::model::{
    AuditRecord, AuditVerdict, MachineError, Phase, RowInput, RunRequest, TerminalOutcome,
    WorkerResult,
};
use sha2::{Digest, Sha256};

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn request(limit: usize) -> RunRequest {
    let rows = (1..=26)
        .map(|index| RowInput {
            row_id: format!("L{index}"),
            source_available: true,
        })
        .collect();
    RunRequest {
        schema_version: "v0".to_owned(),
        attempt_id: "attempt-1".to_owned(),
        parent_attempt_id: None,
        idempotency_key: "audit-attempt-1".to_owned(),
        package_lock_digest: digest('1'),
        implementation_subject_digest: digest('2'),
        capability_contract_digest: digest('3'),
        executor_digest: digest('4'),
        scenario_digest: digest('5'),
        repository_snapshot_digest: digest('6'),
        row_manifest_digest: digest('7'),
        base_ledger_digest: digest('8'),
        rows,
        global_concurrency_limit: limit,
    }
}

fn terminal_json(
    intent: &repository_engineering_runtime::model::DispatchIntent,
    verdict: AuditVerdict,
) -> (WorkerResult, Vec<u8>) {
    let record = AuditRecord {
        schema_version: "v0".to_owned(),
        row_id: intent.row_id.clone(),
        verdict,
    };
    let record_bytes = serde_json::to_vec(&record).unwrap();
    let record_digest = format!("sha256:{:x}", Sha256::digest(&record_bytes));
    let receipt_digest = format!(
        "sha256:{:x}",
        Sha256::digest(intent.worker_instance_id.as_bytes())
    );
    let result: WorkerResult = serde_json::from_value(serde_json::json!({
        "schema_version": "v0",
        "result": "succeeded",
        "attempt_id": intent.attempt_id,
        "invocation_id": intent.invocation_id,
        "assignment_id": intent.assignment_id,
        "worker_instance_id": intent.worker_instance_id,
        "worker_instance_receipt": {
            "schema_version": "v0",
            "path": format!("receipts/{}.json", intent.worker_instance_id),
            "sha256": receipt_digest,
            "media_type": "application/json"
        },
        "payload": {
            "row_id": intent.row_id,
            "verdict": verdict,
            "record": {
                "schema_version": "v0",
                "path": format!("records/{}.json", intent.row_id),
                "sha256": record_digest,
                "media_type": "application/json"
            }
        }
    }))
    .unwrap();
    (result, record_bytes)
}

#[test]
fn complete_sweep_is_bounded_and_projects_manifest_order() {
    let mut machine = SweepMachine::new(request(8)).unwrap();
    machine.begin_dispatch().unwrap();
    assert_eq!(machine.phase(), Phase::Dispatching);

    while !machine.all_invocations_terminal() {
        let intents = machine.request_dispatches().unwrap();
        assert!(!intents.is_empty());
        assert!(intents.len() <= 2);
        for intent in intents.into_iter().rev() {
            let (result, record) = terminal_json(&intent, AuditVerdict::Confirmed);
            machine.accept_terminal(result, Some(&record)).unwrap();
        }
    }
    assert_eq!(machine.phase(), Phase::RollingUp);
    machine.finish_roll_up(&digest('8')).unwrap();
    let terminal = machine.complete().unwrap();
    assert_eq!(terminal.outcome, TerminalOutcome::Succeeded);
    assert_eq!(terminal.rows[0].row_id, "L1");
    assert_eq!(terminal.rows[25].row_id, "L26");
}

#[test]
fn configured_limit_one_and_limit_above_two_are_both_respected() {
    let mut one = SweepMachine::new(request(1)).unwrap();
    one.begin_dispatch().unwrap();
    assert_eq!(one.request_dispatches().unwrap().len(), 1);

    let mut many = SweepMachine::new(request(99)).unwrap();
    many.begin_dispatch().unwrap();
    assert_eq!(many.request_dispatches().unwrap().len(), 2);
    assert_eq!(
        SweepMachine::new(request(0)).unwrap_err(),
        MachineError::InvalidLimit
    );
}

#[test]
fn correlation_conflict_and_replay_fail_closed() {
    let mut machine = SweepMachine::new(request(2)).unwrap();
    machine.begin_dispatch().unwrap();
    let intent = machine.request_dispatches().unwrap().remove(0);
    let (result, record) = terminal_json(&intent, AuditVerdict::Unverifiable);

    let mut stale: serde_json::Value = serde_json::to_value(&result).unwrap();
    stale["attempt_id"] = serde_json::json!("stale-attempt");
    assert_eq!(
        machine
            .accept_terminal(serde_json::from_value(stale).unwrap(), Some(&record))
            .unwrap_err(),
        MachineError::CorrelationMismatch
    );
    assert_eq!(machine.phase(), Phase::RecoveryRequired);

    let mut machine = SweepMachine::new(request(2)).unwrap();
    machine.begin_dispatch().unwrap();
    let intent = machine.request_dispatches().unwrap().remove(0);
    let (result, record) = terminal_json(&intent, AuditVerdict::Confirmed);
    machine
        .accept_terminal(result.clone(), Some(&record))
        .unwrap();
    machine
        .accept_terminal(result.clone(), Some(&record))
        .unwrap();
    let mut conflict = serde_json::to_value(result).unwrap();
    conflict["payload"]["verdict"] = serde_json::json!("refuted");
    assert_eq!(
        machine
            .accept_terminal(serde_json::from_value(conflict).unwrap(), Some(&record))
            .unwrap_err(),
        MachineError::ConflictingReplay
    );
    assert_eq!(machine.phase(), Phase::RecoveryRequired);
}

#[test]
fn refuted_or_unavailable_source_produces_held_capability_outcome() {
    let mut request = request(2);
    request.rows.truncate(1);
    request.rows[0].source_available = false;
    let mut machine = SweepMachine::new(request).unwrap();
    machine.begin_dispatch().unwrap();
    let intent = machine.request_dispatches().unwrap().remove(0);
    let (result, record) = terminal_json(&intent, AuditVerdict::Unverifiable);
    machine.accept_terminal(result, Some(&record)).unwrap();
    machine.finish_roll_up(&digest('8')).unwrap();
    assert_eq!(machine.complete().unwrap().outcome, TerminalOutcome::Held);
}
