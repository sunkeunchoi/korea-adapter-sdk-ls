//! Value-free semantic diagnostics for the provisional contract vocabulary.

use std::collections::BTreeSet;

use crate::schema::{
    ActivationEligibility, AttemptRecord, AttemptState, AuthorityState, CapabilityContract,
    CertificationState, DeclaredContractRegistration, DispatchConcurrency, FieldType,
    ImplementationState, OutcomeKind, PackageManifest, RetirementState, TypedField,
    WorkerRoleContract,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub path: String,
    pub logical_id: Option<String>,
    pub field: &'static str,
    pub code: &'static str,
    pub remediation: &'static str,
}

pub fn validate_first_slice_package(package: &PackageManifest) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (field, actual, expected) in [
        (
            "repository_id",
            package.repository_id.0.as_str(),
            "korea-adapter-sdk-ls",
        ),
        (
            "package_id",
            package.package_id.0.as_str(),
            "repository-engineering",
        ),
        (
            "compatibility.contract_family",
            package.compatibility.contract_family.0.as_str(),
            "repository-engineering",
        ),
        (
            "paths.discovery_policy",
            package.paths.discovery_policy.0.as_str(),
            ".repository-engineering/discovery-policy.toml",
        ),
        (
            "paths.migration_ledger",
            package.paths.migration_ledger.0.as_str(),
            ".repository-engineering/migration-ledger.toml",
        ),
        (
            "paths.schema_registry",
            package.paths.schema_registry.0.as_str(),
            ".repository-engineering/schema-registry.json",
        ),
    ] {
        if actual != expected {
            findings.push(finding(
                "package.toml",
                None,
                field,
                "package.declaration.mismatch",
                "restore_declared_value",
            ));
        }
    }
    if package.activation_eligibility != ActivationEligibility::None {
        findings.push(finding(
            "package.toml",
            None,
            "activation_eligibility",
            "package.activation_eligibility.forbidden",
            "set_none",
        ));
    }
    if !package.active_capability_contracts.is_empty() {
        findings.push(finding(
            "package.toml",
            None,
            "active_capability_contracts",
            "package.active_registry.forbidden",
            "remove_active_contracts",
        ));
    }
    if !package.active_worker_roles.is_empty() {
        findings.push(finding(
            "package.toml",
            None,
            "active_worker_roles",
            "package.active_workers.forbidden",
            "remove_active_workers",
        ));
    }
    validate_declared_registrations(
        &package.declared_capability_contracts,
        "declared_capability_contracts",
        &mut findings,
    );
    validate_declared_registrations(
        &package.declared_worker_roles,
        "declared_worker_roles",
        &mut findings,
    );
    let mut declared_paths = BTreeSet::new();
    let mut declared_paths_folded = BTreeSet::new();
    for registration in package
        .declared_capability_contracts
        .iter()
        .chain(&package.declared_worker_roles)
    {
        if !declared_paths.insert(registration.path.0.as_str())
            || !declared_paths_folded.insert(registration.path.0.to_ascii_lowercase())
        {
            findings.push(finding(
                "package.toml",
                Some(registration.id.0.clone()),
                "declared_contracts.path",
                "package.declared_registry.path_collision",
                "use_unique_declared_contract_path",
            ));
        }
    }
    for component in &package.optional_components {
        if matches!(component, crate::schema::OptionalComponent::Selected { .. }) {
            findings.push(finding(
                "package.toml",
                None,
                "optional_components",
                "package.optional_component.selected",
                "disable_component",
            ));
        }
    }
    let orca_disabled = package.optional_components.iter().filter(|component| {
        matches!(
            component,
            crate::schema::OptionalComponent::Disabled {
                component: crate::schema::OptionalComponentKind::OrcaUi
            }
        )
    });
    let worker_disabled = package.optional_components.iter().filter(|component| {
        matches!(
            component,
            crate::schema::OptionalComponent::Disabled {
                component: crate::schema::OptionalComponentKind::WorkerAdapter
            }
        )
    });
    if orca_disabled.count() != 1 || worker_disabled.count() != 1 {
        findings.push(finding(
            "package.toml",
            None,
            "optional_components",
            "package.optional_component.incomplete",
            "declare_each_disabled_component_once",
        ));
    }
    findings.sort();
    findings
}

pub fn validate_capability_contract_vocabulary(contract: &CapabilityContract) -> Vec<Finding> {
    let mut findings =
        validate_first_slice_contract_state(&contract.state, &contract.capability_id.0);
    validate_typed_fields(
        &contract.inputs,
        &contract.capability_id.0,
        "inputs",
        &mut findings,
    );
    if contract.credential_boundary.is_none() {
        findings.push(contract_finding(
            &contract.capability_id.0,
            "credential_boundary",
            "capability.credential_boundary.missing",
        ));
    }
    if let Some(coordination) = &contract.coordination_semantics {
        for cohort in &coordination.dispatch_cohorts {
            let valid_bound = match cohort.concurrency {
                DispatchConcurrency::BoundedParallel => {
                    cohort.max_concurrency.is_some_and(|value| value > 0)
                }
                DispatchConcurrency::Serial | DispatchConcurrency::Parallel => {
                    cohort.max_concurrency.is_none()
                }
            };
            if !valid_bound {
                findings.push(contract_finding(
                    &contract.capability_id.0,
                    "coordination_semantics.dispatch_cohorts.max_concurrency",
                    "coordination.concurrency.bound_invalid",
                ));
            }
        }
    } else {
        findings.push(contract_finding(
            &contract.capability_id.0,
            "coordination_semantics",
            "capability.coordination_semantics.missing",
        ));
    }
    if let Some(evidence) = &contract.evidence_status {
        for artifact_set in &evidence.legacy_artifact_sets {
            if has_exact_or_case_folded_duplicate(
                artifact_set.members.iter().map(|member| member.0.as_str()),
            ) {
                findings.push(contract_finding(
                    &contract.capability_id.0,
                    "evidence_status.legacy_artifact_sets.members",
                    "artifact_set.members.duplicate",
                ));
            }
        }
    } else {
        findings.push(contract_finding(
            &contract.capability_id.0,
            "evidence_status",
            "capability.evidence_status.missing",
        ));
    }
    for requirement in &contract.external_source_requirements {
        if requirement.locator.is_some() || requirement.digest.is_some() {
            findings.push(contract_finding(
                &contract.capability_id.0,
                "external_source_requirements",
                "external_source.unavailable_has_location",
            ));
        }
    }
    validate_semantic_claims(
        &contract.semantic_claims,
        &contract.capability_id.0,
        &mut findings,
    );
    findings.sort();
    findings.dedup();
    findings
}

pub fn validate_worker_role_contract_vocabulary(contract: &WorkerRoleContract) -> Vec<Finding> {
    let mut findings = validate_first_slice_contract_state(&contract.state, &contract.role_id.0);
    validate_typed_fields(
        &contract.assignment_fields,
        &contract.role_id.0,
        "assignment_fields",
        &mut findings,
    );
    validate_typed_fields(
        &contract.result_fields,
        &contract.role_id.0,
        "result_fields",
        &mut findings,
    );
    if let Some(correlation) = &contract.terminal_result_correlation {
        let expected = [
            OutcomeKind::Succeeded,
            OutcomeKind::Held,
            OutcomeKind::Cancelled,
            OutcomeKind::PolicyViolated,
            OutcomeKind::Failed,
            OutcomeKind::RecoveryRequired,
        ];
        let actual: BTreeSet<_> = correlation.required_variants.iter().copied().collect();
        if actual.len() != correlation.required_variants.len()
            || actual != expected.into_iter().collect()
        {
            findings.push(contract_finding(
                &contract.role_id.0,
                "terminal_result_correlation.required_variants",
                "worker.terminal_correlation.incomplete",
            ));
        }
    } else {
        findings.push(contract_finding(
            &contract.role_id.0,
            "terminal_result_correlation",
            "worker.terminal_correlation.missing",
        ));
    }
    validate_semantic_claims(
        &contract.semantic_claims,
        &contract.role_id.0,
        &mut findings,
    );
    findings.sort();
    findings.dedup();
    findings
}

fn validate_declared_registrations(
    registrations: &[DeclaredContractRegistration],
    field: &'static str,
    findings: &mut Vec<Finding>,
) {
    let mut ids = BTreeSet::new();
    let mut ids_folded = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut paths_folded = BTreeSet::new();
    for registration in registrations {
        if !ids.insert(registration.id.0.as_str())
            || !ids_folded.insert(registration.id.0.to_ascii_lowercase())
            || !paths.insert(registration.path.0.as_str())
            || !paths_folded.insert(registration.path.0.to_ascii_lowercase())
        {
            findings.push(finding(
                "package.toml",
                Some(registration.id.0.clone()),
                field,
                "package.declared_registry.duplicate",
                "remove_duplicate_declared_registration",
            ));
        }
    }
}

fn validate_typed_fields(
    fields: &[TypedField],
    logical_id: &str,
    field_name: &'static str,
    findings: &mut Vec<Finding>,
) {
    let mut names = BTreeSet::new();
    let mut names_folded = BTreeSet::new();
    for field in fields {
        if !names.insert(field.name.0.as_str())
            || !names_folded.insert(field.name.0.to_ascii_lowercase())
        {
            findings.push(finding(
                "contract",
                Some(logical_id.to_owned()),
                field_name,
                "typed_field.name.duplicate",
                "remove_duplicate_typed_field",
            ));
        }
        if field.field_type != FieldType::StableId && !field.allowed_values.is_empty() {
            findings.push(finding(
                "contract",
                Some(logical_id.to_owned()),
                field_name,
                "typed_field.allowed_values.type_invalid",
                "use_allowed_values_only_for_stable_id",
            ));
        }
        if has_exact_or_case_folded_duplicate(
            field.allowed_values.iter().map(|value| value.0.as_str()),
        ) {
            findings.push(finding(
                "contract",
                Some(logical_id.to_owned()),
                field_name,
                "typed_field.allowed_values.duplicate",
                "remove_duplicate_allowed_value",
            ));
        }
    }
}

fn validate_semantic_claims(
    claims: &[crate::schema::SemanticClaim],
    logical_id: &str,
    findings: &mut Vec<Finding>,
) {
    for claim in claims {
        if claim.field_groups.is_empty() || claim.sources.is_empty() {
            findings.push(contract_finding(
                logical_id,
                "semantic_claims",
                "semantic_claim.incomplete",
            ));
        }
    }
}

fn has_exact_or_case_folded_duplicate<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut exact = BTreeSet::new();
    let mut folded = BTreeSet::new();
    for value in values {
        if !exact.insert(value) || !folded.insert(value.to_ascii_lowercase()) {
            return true;
        }
    }
    false
}

pub fn validate_first_slice_contract_state(
    state: &crate::schema::ContractState,
    logical_id: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    if state.implementation != ImplementationState::Unported {
        findings.push(contract_finding(
            logical_id,
            "implementation",
            "implementation.forbidden",
        ));
    }
    if state.certification != CertificationState::Uncertified {
        findings.push(contract_finding(
            logical_id,
            "certification",
            "certification.forbidden",
        ));
    }
    if state.authority != AuthorityState::Legacy {
        findings.push(contract_finding(
            logical_id,
            "authority",
            "authority.transfer.forbidden",
        ));
    }
    if state.retirement != RetirementState::NotStarted {
        findings.push(contract_finding(
            logical_id,
            "retirement",
            "retirement.complete.forbidden",
        ));
    }
    findings.sort();
    findings
}

pub fn validate_attempt_record(record: &AttemptRecord) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut sequences = BTreeSet::new();
    let mut previous = None;
    let mut previous_sequence = None;

    if record.events.len() > 10_000 {
        findings.push(finding(
            "attempt-record.json",
            Some(record.attempt_id.0.clone()),
            "events",
            "attempt.events.limit_exceeded",
            "reduce_events",
        ));
    }

    for (index, event) in record.events.iter().take(10_000).enumerate() {
        if !sequences.insert(event.sequence.as_str()) {
            findings.push(finding(
                "attempt-record.json",
                Some(record.attempt_id.0.clone()),
                "events.sequence",
                "attempt.sequence.duplicate",
                "renumber_events",
            ));
        }
        match parse_sequence(&event.sequence) {
            Some(sequence) if previous_sequence.is_none_or(|value| sequence > value) => {
                previous_sequence = Some(sequence);
            }
            Some(_) => findings.push(finding(
                "attempt-record.json",
                Some(record.attempt_id.0.clone()),
                "events.sequence",
                "attempt.sequence.not_monotonic",
                "renumber_events",
            )),
            None => findings.push(finding(
                "attempt-record.json",
                Some(record.attempt_id.0.clone()),
                "events.sequence",
                "attempt.sequence.invalid",
                "use_decimal_sequence",
            )),
        }
        if index == 0 && event.state != AttemptState::NotEvaluated {
            findings.push(finding(
                "attempt-record.json",
                Some(record.attempt_id.0.clone()),
                "events.state",
                "attempt.initial_state.invalid",
                "start_not_evaluated",
            ));
        }
        if !is_utc_timestamp(&event.occurred_at_utc) {
            findings.push(finding(
                "attempt-record.json",
                Some(record.attempt_id.0.clone()),
                "events.occurred_at_utc",
                "attempt.timestamp.invalid",
                "use_utc_rfc3339",
            ));
        }
        if let Some(from) = previous {
            if !valid_transition(from, event.state) {
                findings.push(finding(
                    "attempt-record.json",
                    Some(record.attempt_id.0.clone()),
                    "events.state",
                    "attempt.transition.invalid",
                    "repair_transition",
                ));
            }
        }
        previous = Some(event.state);
    }

    if previous != Some(record.outcome) {
        findings.push(finding(
            "attempt-record.json",
            Some(record.attempt_id.0.clone()),
            "outcome",
            "attempt.outcome.mismatch",
            "match_terminal_event",
        ));
    }
    if !matches!(
        record.outcome,
        AttemptState::Held
            | AttemptState::Cancelled
            | AttemptState::PolicyViolated
            | AttemptState::Failed
            | AttemptState::RecoveryRequired
            | AttemptState::Succeeded
    ) {
        findings.push(finding(
            "attempt-record.json",
            Some(record.attempt_id.0.clone()),
            "outcome",
            "attempt.outcome.non_terminal",
            "record_terminal_outcome",
        ));
    }
    if let Some(checkpoint) = &record.checkpoint {
        let matches_event = record
            .events
            .iter()
            .any(|event| event.sequence == checkpoint.sequence && event.state == checkpoint.state);
        if !matches_event {
            findings.push(finding(
                "attempt-record.json",
                Some(record.attempt_id.0.clone()),
                "checkpoint",
                "attempt.checkpoint.unbound",
                "bind_checkpoint_to_event",
            ));
        }
    }
    findings.sort();
    findings.truncate(256);
    findings
}

fn parse_sequence(value: &str) -> Option<u64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse().ok()
}

fn is_utc_timestamp(value: &str) -> bool {
    value.len() >= 20
        && value.ends_with('Z')
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
}

fn valid_transition(from: AttemptState, to: AttemptState) -> bool {
    use AttemptState::{
        Cancelled, Failed, Held, NotEvaluated, PolicyViolated, RecoveryRequired, Running, Succeeded,
    };
    matches!(
        (from, to),
        (NotEvaluated, Running)
            | (
                Running,
                Held | Cancelled | PolicyViolated | Failed | RecoveryRequired | Succeeded
            )
            | (Held, Running | Cancelled)
            | (RecoveryRequired, Running | Failed | Cancelled)
    )
}

fn contract_finding(logical_id: &str, field: &'static str, code: &'static str) -> Finding {
    finding(
        "contract",
        Some(logical_id.to_owned()),
        field,
        code,
        "restore_first_slice_state",
    )
}

fn finding(
    path: impl Into<String>,
    logical_id: Option<String>,
    field: &'static str,
    code: &'static str,
    remediation: &'static str,
) -> Finding {
    Finding {
        path: path.into(),
        logical_id,
        field,
        code,
        remediation,
    }
}
