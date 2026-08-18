use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::adapters::{canonical_root, confined_relative, existing_real_path, reject_symlink};
use crate::model::{
    valid_digest, valid_id, CheckpointGeneration, CheckpointHead, CheckpointRow, EffectEntry,
};

const MAX_CHECKPOINT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointFault {
    BeforeGenerationCreate,
    AfterPartialGenerationWrite,
    BeforeGenerationSync,
    BeforeHeadReplace,
    AfterHeadReplace,
    BeforeDirectorySync,
    BeforeCanonicalReopen,
}

pub trait FaultInjector {
    fn should_fail(&mut self, point: CheckpointFault) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFsTrust {
    TrustedSingleHostUnix,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoFault;

impl FaultInjector for NoFault {
    fn should_fail(&mut self, _point: CheckpointFault) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedHead {
    pub sequence: u64,
    pub generation_digest: String,
    pub head_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredCheckpoint {
    pub head: PublishedHead,
    pub generation: CheckpointGeneration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointError {
    InvalidRoot,
    InvalidGeneration,
    AlreadyInitialized,
    NotInitialized,
    CallerPinMismatch,
    ConcurrentUpdate,
    InjectedFault,
    RecoveryRequired,
    Io,
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CheckpointError {}

impl From<io::Error> for CheckpointError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}

#[derive(Debug)]
pub struct CheckpointFs<F = NoFault> {
    root: PathBuf,
    faults: F,
}

impl<F: FaultInjector> CheckpointFs<F> {
    pub fn new(
        root: impl AsRef<Path>,
        _trust: LocalFsTrust,
        faults: F,
    ) -> Result<Self, CheckpointError> {
        #[cfg(not(unix))]
        return Err(CheckpointError::InvalidRoot);
        let root = canonical_root(root.as_ref()).map_err(|_| CheckpointError::InvalidRoot)?;
        Ok(Self { root, faults })
    }

    pub fn create(
        &mut self,
        generation: CheckpointGeneration,
    ) -> Result<PublishedHead, CheckpointError> {
        let root = self.root.clone();
        let _lock = acquire_lock(&root)?;
        reject_symlink(&self.head_path()).map_err(|_| CheckpointError::InvalidRoot)?;
        if self.head_path().exists() {
            return Err(CheckpointError::AlreadyInitialized);
        }
        if generation.sequence != 0 || generation.parent_generation_digest.is_some() {
            return Err(CheckpointError::InvalidGeneration);
        }
        validate_generation(&generation, None)?;
        self.publish_locked(generation)
    }

    pub fn publish(
        &mut self,
        observed_generation_digest: &str,
        generation: CheckpointGeneration,
    ) -> Result<PublishedHead, CheckpointError> {
        let root = self.root.clone();
        let _lock = acquire_lock(&root)?;
        let current = self.read_and_validate_chain()?;
        if current.head.generation_digest != observed_generation_digest {
            return Err(CheckpointError::ConcurrentUpdate);
        }
        if generation.sequence != current.generation.sequence + 1
            || generation.parent_generation_digest.as_deref()
                != Some(current.head.generation_digest.as_str())
        {
            return Err(CheckpointError::InvalidGeneration);
        }
        validate_generation(&generation, Some(&current.generation))?;
        self.publish_locked(generation)
    }

    pub fn recover(&mut self, caller_pin: &str) -> Result<RecoveredCheckpoint, CheckpointError> {
        if !valid_digest(caller_pin) {
            return Err(CheckpointError::CallerPinMismatch);
        }
        let root = self.root.clone();
        let _lock = acquire_lock(&root)?;
        let (recovered, chain) = self.read_chain()?;
        if recovered.head.head_digest != caller_pin
            && !chain
                .iter()
                .any(|(_, digest)| digest.as_str() == caller_pin)
        {
            return Err(CheckpointError::CallerPinMismatch);
        }
        Ok(recovered)
    }

    pub fn generation_path(&self, head: &PublishedHead) -> PathBuf {
        generation_path(&self.root, head.sequence, &head.generation_digest)
    }

    fn publish_locked(
        &mut self,
        generation: CheckpointGeneration,
    ) -> Result<PublishedHead, CheckpointError> {
        let bytes =
            serde_json::to_vec(&generation).map_err(|_| CheckpointError::InvalidGeneration)?;
        if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
            return Err(CheckpointError::InvalidGeneration);
        }
        let generation_digest = digest_bytes(&bytes);
        let generations = self.root.join("generations");
        reject_symlink(&generations).map_err(|_| CheckpointError::InvalidRoot)?;
        fs::create_dir_all(&generations)?;
        let metadata = fs::symlink_metadata(&generations)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CheckpointError::InvalidRoot);
        }
        let generation_path = generation_path(&self.root, generation.sequence, &generation_digest);

        if self
            .faults
            .should_fail(CheckpointFault::BeforeGenerationCreate)
        {
            return Err(CheckpointError::InjectedFault);
        }
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&generation_path)
        {
            Ok(mut file) => {
                if self
                    .faults
                    .should_fail(CheckpointFault::AfterPartialGenerationWrite)
                {
                    file.write_all(&bytes[..bytes.len() / 2])?;
                    return Err(CheckpointError::InjectedFault);
                }
                file.write_all(&bytes)?;
                if self
                    .faults
                    .should_fail(CheckpointFault::BeforeGenerationSync)
                {
                    return Err(CheckpointError::InjectedFault);
                }
                file.sync_all()?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if read_bounded(&generation_path)? != bytes {
                    return Err(CheckpointError::RecoveryRequired);
                }
            }
            Err(error) => return Err(error.into()),
        }

        let head = CheckpointHead {
            schema_version: "v0".to_owned(),
            attempt_id: generation.attempt_id,
            sequence: generation.sequence,
            generation_digest: generation_digest.clone(),
            parent_generation_digest: generation.parent_generation_digest,
        };
        let head_bytes =
            serde_json::to_vec(&head).map_err(|_| CheckpointError::RecoveryRequired)?;
        let temporary = self.root.join(format!(
            ".head-{:020}-{}.tmp",
            head.sequence,
            digest_hex(&generation_digest)?
        ));
        reject_symlink(&temporary)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                file.write_all(&head_bytes)?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if read_bounded(&temporary)? != head_bytes {
                    return Err(CheckpointError::RecoveryRequired);
                }
            }
            Err(error) => return Err(error.into()),
        }
        if self.faults.should_fail(CheckpointFault::BeforeHeadReplace) {
            return Err(CheckpointError::InjectedFault);
        }
        if fs::rename(&temporary, self.head_path()).is_err() {
            return Err(CheckpointError::RecoveryRequired);
        }
        if self.faults.should_fail(CheckpointFault::AfterHeadReplace)
            || self
                .faults
                .should_fail(CheckpointFault::BeforeDirectorySync)
        {
            return Err(CheckpointError::RecoveryRequired);
        }
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| CheckpointError::RecoveryRequired)?;
        if self
            .faults
            .should_fail(CheckpointFault::BeforeCanonicalReopen)
        {
            return Err(CheckpointError::RecoveryRequired);
        }
        if read_bounded(&self.head_path()).map_err(|_| CheckpointError::RecoveryRequired)?
            != head_bytes
        {
            return Err(CheckpointError::RecoveryRequired);
        }
        Ok(PublishedHead {
            sequence: head.sequence,
            generation_digest,
            head_digest: digest_bytes(&head_bytes),
        })
    }

    fn read_and_validate_chain(&self) -> Result<RecoveredCheckpoint, CheckpointError> {
        self.read_chain().map(|(recovered, _)| recovered)
    }

    fn read_chain(
        &self,
    ) -> Result<(RecoveredCheckpoint, Vec<(CheckpointGeneration, String)>), CheckpointError> {
        let head_bytes = match read_bounded(&self.head_path()) {
            Ok(bytes) => bytes,
            Err(CheckpointError::Io) if !self.head_path().exists() => {
                return Err(CheckpointError::NotInitialized);
            }
            Err(error) => return Err(error),
        };
        let head: CheckpointHead = strict_decode(&head_bytes)?;
        if head.schema_version != "v0"
            || !valid_id(&head.attempt_id)
            || !valid_digest(&head.generation_digest)
            || head
                .parent_generation_digest
                .as_deref()
                .is_some_and(|digest| !valid_digest(digest))
        {
            return Err(CheckpointError::RecoveryRequired);
        }

        let mut sequence = head.sequence;
        let mut digest = head.generation_digest.clone();
        let mut chain = Vec::new();
        let mut child: Option<CheckpointGeneration> = None;
        loop {
            let relative = generation_relative_path(sequence, &digest);
            let path = existing_real_path(&self.root, &relative)
                .map_err(|_| CheckpointError::RecoveryRequired)?;
            reject_symlink(&path).map_err(|_| CheckpointError::RecoveryRequired)?;
            let bytes = read_bounded(&path)?;
            if digest_bytes(&bytes) != digest {
                return Err(CheckpointError::RecoveryRequired);
            }
            let generation: CheckpointGeneration = strict_decode(&bytes)?;
            validate_generation(&generation, None)
                .map_err(|_| CheckpointError::RecoveryRequired)?;
            if generation.sequence != sequence
                || generation.attempt_id != head.attempt_id
                || child.as_ref().is_some_and(|next| {
                    next.parent_generation_digest.as_deref() != Some(digest.as_str())
                        || !same_identity(next, &generation)
                        || !valid_phase_transition(&generation.phase, &next.phase)
                })
            {
                return Err(CheckpointError::RecoveryRequired);
            }
            let parent = generation.parent_generation_digest.clone();
            chain.push((generation.clone(), digest.clone()));
            if sequence == 0 {
                if parent.is_some() {
                    return Err(CheckpointError::RecoveryRequired);
                }
                break;
            }
            digest = parent.ok_or(CheckpointError::RecoveryRequired)?;
            sequence -= 1;
            child = Some(generation);
        }
        let latest = chain
            .first()
            .expect("validated chain is nonempty")
            .0
            .clone();
        if latest.parent_generation_digest != head.parent_generation_digest {
            return Err(CheckpointError::RecoveryRequired);
        }
        Ok((
            RecoveredCheckpoint {
                head: PublishedHead {
                    sequence: head.sequence,
                    generation_digest: head.generation_digest,
                    head_digest: digest_bytes(&head_bytes),
                },
                generation: latest,
            },
            chain,
        ))
    }

    fn head_path(&self) -> PathBuf {
        self.root.join("head.json")
    }
}

fn validate_generation(
    generation: &CheckpointGeneration,
    previous: Option<&CheckpointGeneration>,
) -> Result<(), CheckpointError> {
    let digests = [
        &generation.package_lock_digest,
        &generation.implementation_subject_digest,
        &generation.capability_contract_digest,
        &generation.executor_digest,
        &generation.scenario_digest,
        &generation.repository_snapshot_digest,
        &generation.row_manifest_digest,
        &generation.base_ledger_digest,
    ];
    if generation.schema_version != "v0"
        || !valid_id(&generation.attempt_id)
        || generation
            .parent_attempt_id
            .as_deref()
            .is_some_and(|value| !valid_id(value))
        || digests.into_iter().any(|digest| !valid_digest(digest))
        || generation
            .parent_generation_digest
            .as_deref()
            .is_some_and(|digest| !valid_digest(digest))
        || generation
            .cancellation_fence
            .is_some_and(|fence| fence > generation.sequence)
        || !valid_rows(&generation.rows, &generation.attempt_id)
        || !valid_effects(
            &generation.prepared_effects,
            &generation.applied_effect_ids,
            &generation.base_ledger_digest,
        )
        || previous.is_some_and(|prior| {
            !same_identity(generation, prior)
                || !valid_phase_transition(&prior.phase, &generation.phase)
                || prior
                    .cancellation_fence
                    .is_some_and(|fence| generation.cancellation_fence != Some(fence))
        })
    {
        return Err(CheckpointError::InvalidGeneration);
    }
    Ok(())
}

fn valid_rows(rows: &[CheckpointRow], attempt_id: &str) -> bool {
    if rows.is_empty() || rows.len() > 4096 {
        return false;
    }
    let mut ids = BTreeSet::new();
    rows.iter().all(|row| {
        valid_id(&row.row_id)
            && ids.insert(row.row_id.clone())
            && row.dispatch_intent.as_ref().is_none_or(|intent| {
                intent.row_id == row.row_id
                    && intent.assignment_id == row.row_id
                    && intent.attempt_id == attempt_id
                    && valid_id(&intent.attempt_id)
                    && valid_id(&intent.invocation_id)
                    && valid_id(&intent.worker_instance_id)
            })
            && row.result_capsule.as_ref().is_none_or(|reference| {
                reference.schema_version == "v0"
                    && valid_digest(&reference.sha256)
                    && confined_relative(&reference.path).is_ok()
                    && !reference.media_type.is_empty()
                    && reference.media_type.len() <= 128
            })
            && (row.result_capsule.is_none() || row.dispatch_intent.is_some())
            && (!row.completed || row.result_capsule.is_some())
    })
}

fn valid_effects(entries: &[EffectEntry], applied: &[String], base_ledger_digest: &str) -> bool {
    if entries.len() > 1024 || applied.len() > entries.len() {
        return false;
    }
    let mut ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    if !entries.iter().all(|entry| {
        entry.schema_version == "v0"
            && valid_id(&entry.effect_id)
            && ids.insert(entry.effect_id.clone())
            && targets.insert(entry.relative_target.clone())
            && valid_digest(&entry.after_digest)
            && valid_digest(&entry.base_ledger_digest)
            && entry.base_ledger_digest == base_ledger_digest
            && entry
                .expected_before_digest
                .as_deref()
                .is_none_or(valid_digest)
            && confined_relative(&entry.relative_target).is_ok()
            && entry.after_bytes.len() as u64 <= MAX_CHECKPOINT_BYTES
            && digest_bytes(&entry.after_bytes) == entry.after_digest
    }) {
        return false;
    }
    let mut applied_ids = BTreeSet::new();
    applied
        .iter()
        .all(|id| applied_ids.insert(id.clone()) && ids.contains(id.as_str()))
}

fn same_identity(left: &CheckpointGeneration, right: &CheckpointGeneration) -> bool {
    left.attempt_id == right.attempt_id
        && left.parent_attempt_id == right.parent_attempt_id
        && left.package_lock_digest == right.package_lock_digest
        && left.implementation_subject_digest == right.implementation_subject_digest
        && left.capability_contract_digest == right.capability_contract_digest
        && left.executor_digest == right.executor_digest
        && left.scenario_digest == right.scenario_digest
        && left.repository_snapshot_digest == right.repository_snapshot_digest
        && left.row_manifest_digest == right.row_manifest_digest
        && left.base_ledger_digest == right.base_ledger_digest
        && left
            .rows
            .iter()
            .map(|row| (&row.row_id, row.source_available))
            .eq(right
                .rows
                .iter()
                .map(|row| (&row.row_id, row.source_available)))
}

fn valid_phase_transition(previous: &crate::model::Phase, next: &crate::model::Phase) -> bool {
    use crate::model::Phase;

    matches!(
        (previous, next),
        (
            Phase::Discovering,
            Phase::Discovering | Phase::Dispatching | Phase::RecoveryRequired
        ) | (
            Phase::Dispatching,
            Phase::Dispatching | Phase::RollingUp | Phase::RecoveryRequired
        ) | (
            Phase::RollingUp,
            Phase::RollingUp | Phase::GateComputed | Phase::RecoveryRequired
        ) | (
            Phase::GateComputed,
            Phase::GateComputed | Phase::Complete | Phase::RecoveryRequired
        ) | (Phase::RecoveryRequired, Phase::RecoveryRequired)
    )
}

fn generation_path(root: &Path, sequence: u64, digest: &str) -> PathBuf {
    root.join(generation_relative_path(sequence, digest))
}

fn generation_relative_path(sequence: u64, digest: &str) -> PathBuf {
    let suffix = digest.strip_prefix("sha256:").unwrap_or("invalid");
    Path::new("generations").join(format!("{sequence:020}-{suffix}.json"))
}

fn strict_decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CheckpointError> {
    if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
        return Err(CheckpointError::RecoveryRequired);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = T::deserialize(&mut deserializer).map_err(|_| CheckpointError::RecoveryRequired)?;
    deserializer
        .end()
        .map_err(|_| CheckpointError::RecoveryRequired)?;
    Ok(value)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, CheckpointError> {
    reject_symlink(path).map_err(|_| CheckpointError::RecoveryRequired)?;
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_CHECKPOINT_BYTES {
        return Err(CheckpointError::RecoveryRequired);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_CHECKPOINT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
        return Err(CheckpointError::RecoveryRequired);
    }
    Ok(bytes)
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn digest_hex(digest: &str) -> Result<&str, CheckpointError> {
    digest
        .strip_prefix("sha256:")
        .filter(|hex| hex.len() == 64)
        .ok_or(CheckpointError::RecoveryRequired)
}

struct FileLock(File);

fn acquire_lock(root: &Path) -> Result<FileLock, CheckpointError> {
    let path = root.join("checkpoint.lock");
    reject_symlink(&path).map_err(|_| CheckpointError::InvalidRoot)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    #[cfg(unix)]
    {
        // SAFETY: `file` owns a valid descriptor for the duration of the lock.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(CheckpointError::Io);
        }
    }
    #[cfg(not(unix))]
    {
        return Err(CheckpointError::InvalidRoot);
    }
    Ok(FileLock(file))
}

impl Drop for FileLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // SAFETY: the descriptor is still valid and unlock is best-effort on drop.
            unsafe {
                libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}
