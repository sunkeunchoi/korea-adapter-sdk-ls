//! Value-free semantic diagnostics for the provisional contract vocabulary.

use std::collections::BTreeSet;

use crate::schema::{
    ActivationEligibility, AttemptRecord, AttemptState, AuthorityState, CertificationState,
    ImplementationState, PackageManifest, RetirementState,
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
    findings.sort();
    findings
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

    for event in &record.events {
        if !sequences.insert(event.sequence.as_str()) {
            findings.push(finding(
                "attempt-record.json",
                Some(record.attempt_id.0.clone()),
                "events.sequence",
                "attempt.sequence.duplicate",
                "renumber_events",
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
    findings.sort();
    findings.truncate(256);
    findings
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
