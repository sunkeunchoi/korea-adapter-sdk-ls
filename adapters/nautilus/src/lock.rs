//! Advisory lockfile for ingest ↔ live-session mutual exclusion (R15).
//!
//! Rate buckets are per-process, so running bulk ingestion and a live session
//! concurrently against the gateway would double the request rate and trip
//! `IGW00201`. `ls-ingest` and the live node each take an advisory lock beside the
//! catalog and **refuse to start while the counterpart lock is held**. The lock is
//! RAII: the file is removed on drop (normal exit). A stale file from a crash is a
//! deliberate fail-safe — it blocks until an operator clears it (documented in the
//! run-book) rather than racing.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use crate::error::{AdapterError, AdapterResult};

/// Which side of the mutual exclusion a lock represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockKind {
    /// A bulk-ingestion run (`ls-ingest`).
    Ingest,
    /// A live session (node tester binaries).
    Live,
}

impl LockKind {
    fn filename(self) -> &'static str {
        match self {
            LockKind::Ingest => ".ls-ingest.lock",
            LockKind::Live => ".ls-live.lock",
        }
    }

    fn counterpart(self) -> LockKind {
        match self {
            LockKind::Ingest => LockKind::Live,
            LockKind::Live => LockKind::Ingest,
        }
    }

    fn label(self) -> &'static str {
        match self {
            LockKind::Ingest => "ingest",
            LockKind::Live => "live-session",
        }
    }
}

/// An acquired advisory lock. Dropping it removes the lockfile.
#[derive(Debug)]
pub struct AdvisoryLock {
    path: PathBuf,
}

impl AdvisoryLock {
    /// Acquire the `kind` lock in `dir`, refusing if the counterpart lock is held
    /// or if a same-kind lock is already present.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Ingest`] if the counterpart lock is held (mutual exclusion),
    /// or a same-kind lock already exists (another run in progress), or the
    /// lockfile cannot be created.
    pub fn acquire(dir: &Path, kind: LockKind) -> AdapterResult<Self> {
        std::fs::create_dir_all(dir).map_err(|e| {
            AdapterError::Ingest(format!("cannot create lock dir {}: {e}", dir.display()))
        })?;

        let counterpart = dir.join(kind.counterpart().filename());
        if counterpart.exists() {
            return Err(AdapterError::Ingest(format!(
                "refusing to start {}: the {} lock is held ({}); ingest and live sessions are \
                 mutually exclusive (R15)",
                kind.label(),
                kind.counterpart().label(),
                counterpart.display()
            )));
        }

        let path = dir.join(kind.filename());
        // O_CREAT | O_EXCL — fail if a same-kind run already holds the lock.
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(AdvisoryLock { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(AdapterError::Ingest(
                format!(
                    "refusing to start {}: a lock already exists at {} (another run in progress, \
                     or a stale lock from a crash — clear it manually)",
                    kind.label(),
                    path.display()
                ),
            )),
            Err(e) => Err(AdapterError::Ingest(format!(
                "cannot create lock {}: {e}",
                path.display()
            ))),
        }
    }
}

impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ingest_refuses_while_live_lock_held() {
        let dir = tempdir().unwrap();
        let _live = AdvisoryLock::acquire(dir.path(), LockKind::Live).unwrap();
        let err = AdvisoryLock::acquire(dir.path(), LockKind::Ingest).unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn live_refuses_while_ingest_lock_held() {
        let dir = tempdir().unwrap();
        let _ingest = AdvisoryLock::acquire(dir.path(), LockKind::Ingest).unwrap();
        let err = AdvisoryLock::acquire(dir.path(), LockKind::Live).unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn lock_released_on_drop_allows_reacquire() {
        let dir = tempdir().unwrap();
        {
            let _ingest = AdvisoryLock::acquire(dir.path(), LockKind::Ingest).unwrap();
        }
        // Dropped — a live session can now start.
        let _live = AdvisoryLock::acquire(dir.path(), LockKind::Live).unwrap();
    }

    #[test]
    fn same_kind_double_acquire_refused() {
        let dir = tempdir().unwrap();
        let _a = AdvisoryLock::acquire(dir.path(), LockKind::Ingest).unwrap();
        let err = AdvisoryLock::acquire(dir.path(), LockKind::Ingest).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }
}
