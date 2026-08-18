//! Domain-separated RFC 8785 identities for typed repository artifacts.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};

use crate::schema::{
    ArtifactReference, CapabilityContract, NormativeLockClosure, OptionalComponent,
    OptionalComponentKind, PackageManifest, RepositoryPath, SchemaVersion, SemanticClaim,
    SemanticClaimSource, Sha256Digest, StableId, VersionSetComponent, VersionSetFixtureInput,
    WorkerRoleContract,
};

const PACKAGE_DOMAIN: &[u8] = b"ls-repository-engineering/package-lock-id/v0\0";
const VERSION_SET_DOMAIN: &[u8] = b"ls-repository-engineering/version-set-id/fixture-v0\0";
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    InvalidJson,
    DuplicateKey,
    NegativeZero,
    UnsafeInteger,
    Canonicalization,
    OperationalFixture,
    MissingRequiredComponent,
    InvalidOptionalComponent,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            Self::InvalidJson => "identity.invalid_json",
            Self::DuplicateKey => "identity.duplicate_key",
            Self::NegativeZero => "identity.negative_zero",
            Self::UnsafeInteger => "identity.unsafe_integer",
            Self::Canonicalization => "identity.canonicalization_failed",
            Self::OperationalFixture => "identity.operational_fixture_forbidden",
            Self::MissingRequiredComponent => "identity.required_component_missing",
            Self::InvalidOptionalComponent => "identity.optional_component_invalid",
        };
        formatter.write_str(code)
    }
}

impl std::error::Error for IdentityError {}

pub fn package_lock_id(normative: &NormativeLockClosure) -> Result<Sha256Digest, IdentityError> {
    let normalized = normalize_normative(normative);
    let canonical = serde_json_canonicalizer::to_vec(&normalized)
        .map_err(|_| IdentityError::Canonicalization)?;
    Ok(domain_digest(PACKAGE_DOMAIN, &canonical))
}

pub fn package_manifest_semantic_digest(
    package: &PackageManifest,
) -> Result<Sha256Digest, IdentityError> {
    let mut normalized = package.clone();
    normalized
        .declared_capability_contracts
        .sort_by(|left, right| left.id.cmp(&right.id).then(left.path.cmp(&right.path)));
    normalized
        .declared_worker_roles
        .sort_by(|left, right| left.id.cmp(&right.id).then(left.path.cmp(&right.path)));
    normalized.active_capability_contracts.sort();
    normalized.active_worker_roles.sort();
    normalized.optional_components.sort_by_key(optional_key);
    semantic_digest(&normalized)
}

pub fn capability_contract_semantic_digest(
    contract: &CapabilityContract,
) -> Result<Sha256Digest, IdentityError> {
    let mut normalized = contract.clone();
    normalized.public_description = None;
    normalized.safety_overlays.sort_by_key(serialized_key);
    normalize_typed_fields(&mut normalized.inputs);
    normalized.outcomes.sort_by_key(serialized_key);
    normalized.touched_paths.sort();
    normalized.evidence_obligations.sort();
    normalized.human_gates.sort();
    normalized
        .bounded_evidence
        .sort_by(|left, right| left.component_id.cmp(&right.component_id));
    normalized.knowledge_references.sort_by_key(artifact_key);
    normalized
        .bounded_evidence
        .sort_by(|left, right| left.component_id.cmp(&right.component_id));
    normalized.scenario_references.sort_by_key(artifact_key);
    normalized
        .external_source_requirements
        .sort_by(|left, right| left.requirement_id.cmp(&right.requirement_id));
    normalized.legacy_authority_dependencies.sort();
    normalized.worker_roles.sort();
    normalized.scenario_references.sort_by_key(artifact_key);
    normalize_semantic_claims(&mut normalized.semantic_claims);

    if let Some(boundary) = &mut normalized.credential_boundary {
        boundary.credential_free_scopes.sort();
        boundary.future_executor.review_requirements.sort();
    }
    if let Some(coordination) = &mut normalized.coordination_semantics {
        for cohort in &mut coordination.dispatch_cohorts {
            cohort.candidate_classes.sort();
        }
        coordination
            .dispatch_cohorts
            .sort_by(|left, right| left.cohort_id.cmp(&right.cohort_id));
        coordination.terminal_conditions.sort_by_key(serialized_key);
    }
    if let Some(evidence) = &mut normalized.evidence_status {
        evidence.legacy_artifacts.sort_by_key(artifact_key);
        for artifact_set in &mut evidence.legacy_artifact_sets {
            artifact_set.members.sort();
        }
        evidence
            .legacy_artifact_sets
            .sort_by(|left, right| left.artifact_set_id.cmp(&right.artifact_set_id));
    }
    semantic_digest(&normalized)
}

pub fn worker_role_contract_semantic_digest(
    contract: &WorkerRoleContract,
) -> Result<Sha256Digest, IdentityError> {
    let mut normalized = contract.clone();
    normalized.public_description = None;
    normalize_typed_fields(&mut normalized.assignment_fields);
    normalize_typed_fields(&mut normalized.result_fields);
    normalized.knowledge_references.sort_by_key(artifact_key);
    normalize_semantic_claims(&mut normalized.semantic_claims);
    if let Some(correlation) = &mut normalized.terminal_result_correlation {
        correlation.required_variants.sort_by_key(serialized_key);
    }
    semantic_digest(&normalized)
}

pub fn canonicalize_strict_json(input: &str) -> Result<Vec<u8>, IdentityError> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = StrictValue::deserialize(&mut deserializer).map_err(classify_json_error)?;
    deserializer.end().map_err(|_| IdentityError::InvalidJson)?;
    serde_json_canonicalizer::to_vec(&value.0).map_err(|_| IdentityError::Canonicalization)
}

/// Reproduces the single built-in, visibly non-operational Version Set vector.
/// No caller-controlled Version Set input is accepted by this public surface.
pub fn built_in_conformance_vector() -> Result<(VersionSetFixtureInput, Sha256Digest), IdentityError>
{
    let input = VersionSetFixtureInput {
        schema_version: SchemaVersion::V0,
        fixture_only: true,
        package_lock_id: Sha256Digest(format!("sha256:{}", "1".repeat(64))),
        package: fixture_artifact("fixture/package.lock.json", '2'),
        components: vec![
            VersionSetComponent {
                component_id: StableId("runtime".to_owned()),
                required: true,
                artifact: Some(fixture_artifact("fixture/runtime", '3')),
                disabled: false,
            },
            VersionSetComponent {
                component_id: StableId("orca-ui".to_owned()),
                required: false,
                artifact: None,
                disabled: true,
            },
        ],
    };
    let identity = version_set_id_for_fixture(&input)?;
    Ok((input, identity))
}

pub(crate) fn version_set_id_for_fixture(
    input: &VersionSetFixtureInput,
) -> Result<Sha256Digest, IdentityError> {
    if !input.fixture_only {
        return Err(IdentityError::OperationalFixture);
    }
    for component in &input.components {
        if component.required && (component.disabled || component.artifact.is_none()) {
            return Err(IdentityError::MissingRequiredComponent);
        }
        if !component.required && (component.disabled == component.artifact.is_some()) {
            return Err(IdentityError::InvalidOptionalComponent);
        }
    }
    let mut normalized = input.clone();
    normalized
        .components
        .sort_by(|left, right| left.component_id.cmp(&right.component_id));
    let canonical = serde_json_canonicalizer::to_vec(&normalized)
        .map_err(|_| IdentityError::Canonicalization)?;
    Ok(domain_digest(VERSION_SET_DOMAIN, &canonical))
}

pub(crate) fn normalize_normative(normative: &NormativeLockClosure) -> NormativeLockClosure {
    let mut normalized = normative.clone();
    normalized.capability_contracts.sort_by_key(artifact_key);
    normalized.worker_role_contracts.sort_by_key(artifact_key);
    normalized.implementation_subjects.sort_by_key(artifact_key);
    normalized.optional_components.sort_by_key(optional_key);
    normalized
}

fn semantic_digest<T: Serialize>(value: &T) -> Result<Sha256Digest, IdentityError> {
    let canonical =
        serde_json_canonicalizer::to_vec(value).map_err(|_| IdentityError::Canonicalization)?;
    Ok(Sha256Digest(format!(
        "sha256:{:x}",
        Sha256::digest(canonical)
    )))
}

fn normalize_typed_fields(fields: &mut [crate::schema::TypedField]) {
    for field in fields {
        field.allowed_values.sort();
    }
}

fn normalize_semantic_claims(claims: &mut [SemanticClaim]) {
    for claim in claims.iter_mut() {
        claim.field_groups.sort();
        for source in &mut claim.sources {
            if let SemanticClaimSource::MigrationLedgerRows { logical_ids } = source {
                logical_ids.sort();
            }
        }
        claim.sources.sort_by_key(serialized_key);
    }
    claims.sort_by_key(serialized_key);
}

fn artifact_key(reference: &ArtifactReference) -> (RepositoryPath, Sha256Digest, String) {
    (
        reference.path.clone(),
        reference.sha256.clone(),
        reference.media_type.clone(),
    )
}

fn serialized_key<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json_canonicalizer::to_vec(value)
        .expect("schema-owned semantic values must serialize canonically")
}

fn optional_key(component: &OptionalComponent) -> u8 {
    match component {
        OptionalComponent::Disabled {
            component: OptionalComponentKind::OrcaUi,
        }
        | OptionalComponent::Selected {
            component: OptionalComponentKind::OrcaUi,
            ..
        } => 0,
        OptionalComponent::Disabled {
            component: OptionalComponentKind::WorkerAdapter,
        }
        | OptionalComponent::Selected {
            component: OptionalComponentKind::WorkerAdapter,
            ..
        } => 1,
    }
}

fn domain_digest(domain: &[u8], canonical: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(canonical);
    Sha256Digest(format!("sha256:{:x}", hasher.finalize()))
}

fn fixture_artifact(path: &str, digit: char) -> ArtifactReference {
    ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(path.to_owned()),
        sha256: Sha256Digest(format!("sha256:{}", digit.to_string().repeat(64))),
        media_type: "application/octet-stream".to_owned(),
    }
}

fn classify_json_error(error: serde_json::Error) -> IdentityError {
    match error.to_string().as_str() {
        message if message.starts_with("duplicate key") => IdentityError::DuplicateKey,
        message if message.starts_with("negative zero") => IdentityError::NegativeZero,
        message if message.starts_with("unsafe integer") => IdentityError::UnsafeInteger,
        _ => IdentityError::InvalidJson,
    }
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an I-JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value.unsigned_abs() > MAX_SAFE_INTEGER {
            return Err(E::custom("unsafe integer"));
        }
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value > MAX_SAFE_INTEGER {
            return Err(E::custom("unsafe integer"));
        }
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value == 0.0 && value.is_sign_negative() {
            return Err(E::custom("negative zero"));
        }
        Number::from_f64(value)
            .map(|number| StrictValue(Value::Number(number)))
            .ok_or_else(|| E::custom("non-finite number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(StrictValue(Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(StrictValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom("duplicate key"));
            }
            values.insert(key, map.next_value::<StrictValue>()?.0);
        }
        Ok(StrictValue(Value::Object(values)))
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{
        ArtifactReference, RepositoryPath, SchemaVersion, StableId, VersionSetComponent,
        VersionSetFixtureInput,
    };

    use super::*;

    fn artifact(path: &str, digit: char) -> ArtifactReference {
        ArtifactReference {
            schema_version: SchemaVersion::V0,
            path: RepositoryPath(path.to_owned()),
            sha256: Sha256Digest(format!("sha256:{}", digit.to_string().repeat(64))),
            media_type: "application/octet-stream".to_owned(),
        }
    }

    fn fixture() -> VersionSetFixtureInput {
        VersionSetFixtureInput {
            schema_version: SchemaVersion::V0,
            fixture_only: true,
            package_lock_id: Sha256Digest(format!("sha256:{}", "1".repeat(64))),
            package: artifact("package.lock.json", '2'),
            components: vec![
                VersionSetComponent {
                    component_id: StableId("runtime".to_owned()),
                    required: true,
                    artifact: Some(artifact("runtime", '3')),
                    disabled: false,
                },
                VersionSetComponent {
                    component_id: StableId("orca-ui".to_owned()),
                    required: false,
                    artifact: None,
                    disabled: true,
                },
            ],
        }
    }

    #[test]
    fn version_set_fixture_is_bound_and_non_operational() {
        let baseline = fixture();
        let baseline_id = version_set_id_for_fixture(&baseline).unwrap();

        let mut changed = baseline.clone();
        changed.package_lock_id = Sha256Digest(format!("sha256:{}", "9".repeat(64)));
        assert_ne!(version_set_id_for_fixture(&changed).unwrap(), baseline_id);

        let mut incomplete = baseline.clone();
        incomplete.components[0].artifact = None;
        assert_eq!(
            version_set_id_for_fixture(&incomplete),
            Err(IdentityError::MissingRequiredComponent)
        );

        let mut operational = baseline;
        operational.fixture_only = false;
        assert_eq!(
            version_set_id_for_fixture(&operational),
            Err(IdentityError::OperationalFixture)
        );
    }
}
