use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::adapters::{
    canonical_root, confined_relative, ensure_real_parents, existing_real_path, reject_symlink,
};
pub use crate::model::EffectApplyOutcome as ApplyOutcome;
use crate::model::{valid_digest, valid_id, EffectEntry};
use crate::ports::EffectApplier;

const MAX_EFFECT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectError {
    InvalidEntry,
    DuplicateEffect,
    DuplicateTarget,
    StateConflict,
    Io,
}

impl std::fmt::Display for EffectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EffectError {}

impl From<io::Error> for EffectError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}

#[derive(Debug)]
pub struct EffectFs {
    root: PathBuf,
    base_ledger: PathBuf,
}

impl EffectFs {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, EffectError> {
        Ok(Self {
            root: canonical_root(root.as_ref())?,
            base_ledger: PathBuf::from(".repository-engineering/migration-ledger.toml"),
        })
    }

    pub fn validate_plan(&self, entries: &[EffectEntry]) -> Result<(), EffectError> {
        let mut ids = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for entry in entries {
            self.validate_entry(entry)?;
            if !ids.insert(entry.effect_id.clone()) {
                return Err(EffectError::DuplicateEffect);
            }
            if !targets.insert(entry.relative_target.clone()) {
                return Err(EffectError::DuplicateTarget);
            }
        }
        Ok(())
    }

    pub fn apply(&mut self, entry: &EffectEntry) -> Result<ApplyOutcome, EffectError> {
        self.validate_entry(entry)?;
        let relative = confined_relative(&entry.relative_target)?;
        let target = ensure_real_parents(&self.root, &relative)?;
        reject_symlink(&target)?;

        let observed = read_optional_bounded(&target)?;
        if observed.as_deref() == Some(entry.after_bytes.as_slice()) {
            return Ok(ApplyOutcome::AlreadyApplied);
        }
        let observed_digest = observed.as_deref().map(digest_bytes);
        if observed_digest != entry.expected_before_digest {
            return Err(EffectError::StateConflict);
        }

        let file_name = target
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(EffectError::InvalidEntry)?;
        let temporary = target.with_file_name(format!(".{file_name}.{}.tmp", entry.effect_id));
        reject_symlink(&temporary)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                file.write_all(&entry.after_bytes)?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if read_optional_bounded(&temporary)?.as_deref()
                    != Some(entry.after_bytes.as_slice())
                {
                    return Err(EffectError::StateConflict);
                }
            }
            Err(error) => return Err(error.into()),
        }
        fs::rename(&temporary, &target)?;
        File::open(target.parent().expect("effect target always has a parent"))?.sync_all()?;
        if read_optional_bounded(&target)?.as_deref() != Some(entry.after_bytes.as_slice()) {
            return Err(EffectError::StateConflict);
        }
        Ok(ApplyOutcome::Applied)
    }

    fn validate_entry(&self, entry: &EffectEntry) -> Result<(), EffectError> {
        if entry.schema_version != "v0"
            || !valid_id(&entry.effect_id)
            || confined_relative(&entry.relative_target).is_err()
            || entry.after_bytes.len() as u64 > MAX_EFFECT_BYTES
            || !valid_digest(&entry.after_digest)
            || !valid_digest(&entry.base_ledger_digest)
            || entry
                .expected_before_digest
                .as_deref()
                .is_some_and(|digest| !valid_digest(digest))
            || digest_bytes(&entry.after_bytes) != entry.after_digest
        {
            return Err(EffectError::InvalidEntry);
        }
        Ok(())
    }
}

impl EffectApplier for EffectFs {
    type Error = EffectError;

    fn observed_base_ledger_digest(&self) -> Result<String, Self::Error> {
        let target = existing_real_path(&self.root, &self.base_ledger)?;
        reject_symlink(&target)?;
        let bytes = read_optional_bounded(&target)?.ok_or(EffectError::StateConflict)?;
        Ok(digest_bytes(&bytes))
    }

    fn validate_plan(&self, entries: &[EffectEntry]) -> Result<(), Self::Error> {
        Self::validate_plan(self, entries)
    }

    fn apply(&mut self, entry: &EffectEntry) -> Result<ApplyOutcome, Self::Error> {
        Self::apply(self, entry)
    }
}

fn read_optional_bounded(path: &Path) -> Result<Option<Vec<u8>>, EffectError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_EFFECT_BYTES {
        return Err(EffectError::StateConflict);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_EFFECT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_EFFECT_BYTES {
        return Err(EffectError::StateConflict);
    }
    Ok(Some(bytes))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
