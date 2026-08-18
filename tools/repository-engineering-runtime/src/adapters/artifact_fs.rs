use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::adapters::{
    canonical_root, confined_relative, ensure_real_parents, existing_real_path, reject_symlink,
};
use crate::ports::ArtifactStore;

const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub struct ArtifactFs {
    root: PathBuf,
}

impl ArtifactFs {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            root: canonical_root(root.as_ref())?,
        })
    }

    fn path(&self, artifact_id: &str) -> io::Result<PathBuf> {
        let relative = confined_relative(artifact_id)?;
        ensure_real_parents(&self.root, &relative)
    }

    fn sync_parent(path: &Path) -> io::Result<()> {
        File::open(path.parent().expect("artifact always has a parent"))?.sync_all()
    }
}

impl ArtifactStore for ArtifactFs {
    type Error = io::Error;

    fn create(&mut self, artifact_id: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact is oversized",
            ));
        }
        let path = self.path(artifact_id)?;
        reject_symlink(&path)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Self::sync_parent(&path)
    }

    fn read(&self, artifact_id: &str) -> Result<Vec<u8>, Self::Error> {
        let relative = confined_relative(artifact_id)?;
        let path = existing_real_path(&self.root, &relative)?;
        reject_symlink(&path)?;
        let file = File::open(path)?;
        let length = file.metadata()?.len();
        if length > MAX_ARTIFACT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "artifact is oversized",
            ));
        }
        let mut bytes = Vec::with_capacity(length as usize);
        file.take(MAX_ARTIFACT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "artifact grew while reading",
            ));
        }
        Ok(bytes)
    }
}
