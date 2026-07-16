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
    /// The dispatch chain append serializer (KTD2). Serializes chain writes among
    /// themselves; it has **no** counterpart — a dispatch append does not exclude an
    /// ingest or a live session, and the gate probes the Live lock file explicitly
    /// rather than pairing against it (KTD2).
    Dispatch,
}

impl LockKind {
    /// The lockfile name for this kind. `pub(crate)` so the gate can probe the Live
    /// lock file explicitly (KTD2) without acquiring it.
    pub(crate) fn filename(self) -> &'static str {
        match self {
            LockKind::Ingest => ".ls-ingest.lock",
            LockKind::Live => ".ls-live.lock",
            LockKind::Dispatch => ".ls-dispatch.lock",
        }
    }

    /// The mutual-exclusion counterpart, if this kind has one. `Dispatch` has none:
    /// it is a binary Ingest↔Live pairing (KTD2).
    fn counterpart(self) -> Option<LockKind> {
        match self {
            LockKind::Ingest => Some(LockKind::Live),
            LockKind::Live => Some(LockKind::Ingest),
            LockKind::Dispatch => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            LockKind::Ingest => "ingest",
            LockKind::Live => "live-session",
            LockKind::Dispatch => "dispatch",
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

        if let Some(counterpart_kind) = kind.counterpart() {
            let counterpart = dir.join(counterpart_kind.filename());
            if counterpart.exists() {
                return Err(AdapterError::Ingest(format!(
                    "refusing to start {}: the {} lock is held ({}); ingest and live sessions are \
                     mutually exclusive (R15)",
                    kind.label(),
                    counterpart_kind.label(),
                    counterpart.display()
                )));
            }
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

/// Probe whether a `kind` lock is currently held in `dir`, without acquiring it. The
/// dispatch gate uses this to check the Live lock explicitly (KTD2): a new `--dispatch`
/// gate attempt refuses while a live session holds the Live lock, but `Dispatch` takes
/// no counterpart, so the probe is a read, not an acquisition.
pub fn is_held(dir: &Path, kind: LockKind) -> bool {
    dir.join(kind.filename()).exists()
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

    #[test]
    fn dispatch_has_no_counterpart_but_serializes_against_itself() {
        // KTD2: a dispatch append does not exclude ingest or live, and vice versa —
        // Dispatch has no counterpart. But two concurrent chain appends are refused.
        let dir = tempdir().unwrap();
        let _ingest = AdvisoryLock::acquire(dir.path(), LockKind::Ingest).unwrap();
        let _live_probe_absent = !is_held(dir.path(), LockKind::Live);
        // Dispatch acquires freely even while Ingest is held (no counterpart).
        let disp = AdvisoryLock::acquire(dir.path(), LockKind::Dispatch).unwrap();
        // A second concurrent dispatch append is refused (same-kind exclusion).
        let err = AdvisoryLock::acquire(dir.path(), LockKind::Dispatch).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        drop(disp);
        // Released — a fresh append can acquire.
        let _again = AdvisoryLock::acquire(dir.path(), LockKind::Dispatch).unwrap();
    }

    #[test]
    fn is_held_probes_without_acquiring() {
        let dir = tempdir().unwrap();
        assert!(!is_held(dir.path(), LockKind::Live));
        let _live = AdvisoryLock::acquire(dir.path(), LockKind::Live).unwrap();
        assert!(is_held(dir.path(), LockKind::Live));
        // Probing does not itself acquire — a second probe still reads held, and a
        // real acquire still refuses.
        assert!(is_held(dir.path(), LockKind::Live));
        assert!(AdvisoryLock::acquire(dir.path(), LockKind::Live).is_err());
    }
}
