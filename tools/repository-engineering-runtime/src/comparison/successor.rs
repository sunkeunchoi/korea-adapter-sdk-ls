use sha2::{Digest, Sha256};

use crate::machine::SweepMachine;
use crate::model::{
    AcceptedResultCapsule, ArtifactReference, AuditRecord, AuditSuccessPayload, AuditVerdict,
    MachineError, RowInput, RunRequest, TerminalOutcome, WorkerResult,
};

use super::{ComparisonCase, ComparisonError, ComparisonPolicy, ConformanceResult};

#[derive(Debug)]
pub(super) struct SuccessorRow {
    pub case_id: String,
    pub verdict: AuditVerdict,
    pub completed: bool,
    pub blocking: bool,
    pub roll_up: String,
    pub credential_rule: bool,
    pub path_rule: bool,
}

#[derive(Debug)]
pub(super) struct SuccessorNormalization {
    pub rows: Vec<SuccessorRow>,
    pub outcome: TerminalOutcome,
}

pub(super) fn normalize(
    policy: &ComparisonPolicy,
    cases: &[ComparisonCase],
) -> Result<SuccessorNormalization, ComparisonError> {
    let mut machine = SweepMachine::new(request(policy, cases, "bounded-comparison"))
        .map_err(|_| ComparisonError::InvalidInput)?;
    machine
        .begin_dispatch()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    while !machine.all_invocations_terminal() {
        let intents = machine
            .request_dispatches()
            .map_err(|_| ComparisonError::SemanticDifference)?;
        if intents.is_empty() {
            return Err(ComparisonError::SemanticDifference);
        }
        for intent in intents {
            let case = cases
                .iter()
                .find(|case| case.row_id == intent.row_id)
                .ok_or(ComparisonError::IncompleteCorpus)?;
            machine
                .accept_capsule(&capsule(&intent, case.expected_verdict))
                .map_err(|_| ComparisonError::SemanticDifference)?;
        }
    }
    let effects = machine
        .prepare_roll_up_effects()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    if effects.len() != 1
        || effects[0].base_ledger_digest != policy.legacy_artifact_set_digest
        || digest_bytes(&effects[0].after_bytes) != effects[0].after_digest
    {
        return Err(ComparisonError::SemanticDifference);
    }
    machine
        .finish_roll_up(&policy.legacy_artifact_set_digest)
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let terminal = machine
        .complete()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let rows = cases
        .iter()
        .zip(terminal.rows.iter())
        .map(|(case, row)| {
            let verdict = row.verdict.ok_or(ComparisonError::SemanticDifference)?;
            let (blocking, roll_up) = match verdict {
                AuditVerdict::Confirmed => (!row.source_available, "unchanged"),
                AuditVerdict::Refuted => (true, "redisposition_required"),
                AuditVerdict::Unverifiable => (true, "unchanged_blocked"),
            };
            Ok(SuccessorRow {
                case_id: case.case_id.clone(),
                verdict,
                completed: row.completed,
                blocking,
                roll_up: roll_up.to_owned(),
                credential_rule: true,
                path_rule: true,
            })
        })
        .collect::<Result<Vec<_>, ComparisonError>>()?;
    Ok(SuccessorNormalization {
        rows,
        outcome: terminal.outcome,
    })
}

pub(super) fn conformance(
    policy: &ComparisonPolicy,
) -> Result<Vec<ConformanceResult>, ComparisonError> {
    let case = ComparisonCase {
        case_id: "conformance-row".to_owned(),
        row_id: "L1".to_owned(),
        legacy_record: policy.legacy_report.clone(),
        expected_classification: "knowledge".to_owned(),
        expected_verdict: AuditVerdict::Confirmed,
        expected_disposition: "carried".to_owned(),
        source_available: true,
    };

    let mut durability = SweepMachine::new(request(policy, std::slice::from_ref(&case), "durable"))
        .map_err(|_| ComparisonError::InvalidInput)?;
    durability
        .begin_dispatch()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let durability_intent = durability
        .request_dispatches()
        .map_err(|_| ComparisonError::SemanticDifference)?
        .remove(0);
    durability
        .accept_capsule(&capsule(&durability_intent, AuditVerdict::Confirmed))
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let first_plan = durability
        .prepare_roll_up_effects()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let second_plan = durability
        .prepare_roll_up_effects()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let durability_passed = first_plan == second_plan
        && first_plan.len() == 1
        && digest_bytes(&first_plan[0].after_bytes) == first_plan[0].after_digest;

    let mut cancellation =
        SweepMachine::new(request(policy, std::slice::from_ref(&case), "cancel"))
            .map_err(|_| ComparisonError::InvalidInput)?;
    cancellation
        .begin_dispatch()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let _ = cancellation
        .request_dispatches()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    cancellation
        .cancel()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let cancelled = cancellation
        .finish_cancel()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let cancellation_passed = cancelled.outcome == TerminalOutcome::Cancelled
        && cancelled.rows.iter().all(|row| !row.completed);

    let mut correlation =
        SweepMachine::new(request(policy, std::slice::from_ref(&case), "correlation"))
            .map_err(|_| ComparisonError::InvalidInput)?;
    correlation
        .begin_dispatch()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let correlation_intent = correlation
        .request_dispatches()
        .map_err(|_| ComparisonError::SemanticDifference)?
        .remove(0);
    let mut wrong = capsule(&correlation_intent, AuditVerdict::Confirmed);
    if let WorkerResult::Succeeded { assignment_id, .. } = &mut wrong.result {
        *assignment_id = "L2".to_owned();
    }
    let correlation_passed =
        correlation.accept_capsule(&wrong) == Err(MachineError::CorrelationMismatch);

    let mut first = SweepMachine::new(request(policy, std::slice::from_ref(&case), "fresh-a"))
        .map_err(|_| ComparisonError::InvalidInput)?;
    let mut second = SweepMachine::new(request(policy, std::slice::from_ref(&case), "fresh-b"))
        .map_err(|_| ComparisonError::InvalidInput)?;
    first
        .begin_dispatch()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    second
        .begin_dispatch()
        .map_err(|_| ComparisonError::SemanticDifference)?;
    let first_intent = first
        .request_dispatches()
        .map_err(|_| ComparisonError::SemanticDifference)?
        .remove(0);
    let second_intent = second
        .request_dispatches()
        .map_err(|_| ComparisonError::SemanticDifference)?
        .remove(0);
    let freshness_passed = first_intent.assignment_id == second_intent.assignment_id
        && first_intent.invocation_id != second_intent.invocation_id
        && first_intent.worker_instance_id != second_intent.worker_instance_id;

    let confinement_passed = first_plan.iter().all(|effect| {
        confined(&effect.relative_target)
            && effect
                .expected_before_digest
                .as_deref()
                .is_none_or(valid_digest)
    });

    Ok(vec![
        conformance_result(
            "durability",
            "deterministic-digest-bound-roll-up",
            durability_passed,
        ),
        conformance_result(
            "cancellation",
            "cancelled-row-remains-incomplete",
            cancellation_passed,
        ),
        conformance_result(
            "correlation",
            "wrong-assignment-fails-closed",
            correlation_passed,
        ),
        conformance_result(
            "freshness",
            "child-attempt-uses-fresh-invocation",
            freshness_passed,
        ),
        conformance_result(
            "confinement",
            "roll-up-target-is-confined",
            confinement_passed,
        ),
    ])
}

fn request(policy: &ComparisonPolicy, cases: &[ComparisonCase], attempt: &str) -> RunRequest {
    RunRequest {
        schema_version: "v0".to_owned(),
        attempt_id: attempt.to_owned(),
        parent_attempt_id: None,
        idempotency_key: format!("{attempt}-idempotency"),
        package_lock_digest: policy.wave1_package_lock_id.clone(),
        implementation_subject_digest: policy.implementation_subject.sha256.clone(),
        capability_contract_digest: policy.capability_contract.sha256.clone(),
        worker_role_digest: policy.worker_role_contract.sha256.clone(),
        executor_digest: policy.executor.sha256.clone(),
        scenario_digest: policy.successor_scenario.sha256.clone(),
        repository_snapshot_digest: policy.implementation_subject.sha256.clone(),
        row_manifest_digest: policy.migration_source_manifest.sha256.clone(),
        base_ledger_digest: policy.legacy_artifact_set_digest.clone(),
        output_root_id: format!("{attempt}-output"),
        rows: cases
            .iter()
            .map(|case| RowInput {
                row_id: case.row_id.clone(),
                source_available: case.source_available,
            })
            .collect(),
        global_concurrency_limit: 8,
    }
}

fn capsule(intent: &crate::model::DispatchIntent, verdict: AuditVerdict) -> AcceptedResultCapsule {
    let record_bytes = serde_json::to_vec(&AuditRecord {
        schema_version: "v0".to_owned(),
        row_id: intent.row_id.clone(),
        verdict,
    })
    .expect("closed audit record serializes");
    let receipt_bytes = format!("receipt:{}", intent.worker_instance_id).into_bytes();
    AcceptedResultCapsule {
        schema_version: "v0".to_owned(),
        result: WorkerResult::Succeeded {
            schema_version: "v0".to_owned(),
            attempt_id: intent.attempt_id.clone(),
            invocation_id: intent.invocation_id.clone(),
            assignment_id: intent.assignment_id.clone(),
            worker_instance_id: intent.worker_instance_id.clone(),
            worker_instance_receipt: ArtifactReference {
                schema_version: "v0".to_owned(),
                path: format!("receipts/{}.json", intent.worker_instance_id),
                sha256: digest_bytes(&receipt_bytes),
                media_type: "application/json".to_owned(),
            },
            payload: AuditSuccessPayload {
                row_id: intent.row_id.clone(),
                verdict,
                record: ArtifactReference {
                    schema_version: "v0".to_owned(),
                    path: format!("records/{}.json", intent.row_id),
                    sha256: digest_bytes(&record_bytes),
                    media_type: "application/json".to_owned(),
                },
            },
        },
        record_bytes: Some(record_bytes),
        worker_instance_receipt_bytes: receipt_bytes,
    }
}

fn conformance_result(dimension: &str, case_id: &str, passed: bool) -> ConformanceResult {
    ConformanceResult {
        dimension: dimension.to_owned(),
        case_id: case_id.to_owned(),
        passed,
    }
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn confined(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}
