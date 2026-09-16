//! The window-aware work queue (U1, R6/R8/R9; KTD2/KTD6) — the tool-owned store
//! that replaces the window-named TODO files at cutover.
//!
//! One git-tracked JSONL at `queue/items.jsonl` (repo root) holds every
//! operational task; each item carries its window requirement as data (R6) and a
//! declared completion signal (R8). State changes flow through the edit surface
//! here — never hand-edited prose — so "file describes finished work" is
//! structurally impossible rather than a discipline.
//!
//! Lifecycle hygiene (KTD6):
//! - Destructive transitions (`done`, `supersede`) never complete from an empty
//!   or absent read: a missing/empty completion artifact or a not-yet-existing
//!   superseder leaves the item actionable with a `reconcile` flag (mirroring
//!   docs/solutions/logic-errors/empty-repull-completing-destructive-heal-destroys-history.md).
//! - Completed and stale items leave the actionable view but stay in the store
//!   (append-forward history, R9); stale = past `deadline` or `superseded_by` a
//!   named item; neither a paused in-flight sequence entry (`sequence` set with
//!   a valid checkpoint) nor a blocked item (`unblock_condition` set) is ever
//!   stale — waiting work is not abandoned work.
//! - The priority marker (R1) is single-holder ON WRITE: `set_priority` clears
//!   every OTHER holder in the same read-mutate-save (a binary predating the
//!   field could have left several behind), `done` releases it and `supersede`
//!   transfers it to the superseder when that superseder is itself open,
//!   RELEASING it otherwise (R21) — the marker follows the work its holder's
//!   transition leads to, and no transition can park it on terminal work, which
//!   the report renders nowhere. `save` refuses to persist a second holder, so
//!   the invariant does not depend on which verb reached the write.
//!
//! Writes are whole-file read → mutate → atomic tmp+rename (mirroring the
//! ingest-checkpoint idiom at `nautilus-ls/src/ingest/checkpoint.rs`), so a crash
//! mid-write can never corrupt the live queue. A malformed line is a typed
//! per-line read error and the queue is never rewritten past it.

pub mod item;
pub mod sequences;
pub mod store;
pub mod window;

pub use item::{CompletionSignal, QueueItem, Window};
pub use store::{Queue, TransitionOutcome};

use std::path::{Path, PathBuf};

/// The current queue-item schema version. A newer-producer line is a typed
/// refusal on read (never a silent admit), mirroring the trials ledger's gate.
pub const QUEUE_SCHEMA_VERSION: u32 = 1;

/// The tracked queue file, relative to the REPO root (KTD2).
pub const QUEUE_RELPATH: &str = "queue/items.jsonl";

/// The test-time override for the queue path (KTD2).
pub const QUEUE_PATH_ENV: &str = "LS_QUEUE_PATH";

/// Whether an artifact path witnesses completion: it exists and is non-empty
/// (a non-empty file, or a directory with at least one entry). An unreadable
/// path is treated as absent — fail toward keeping the item actionable.
/// Crate-visible so the entry report's R12 reconciliation pre-checks with the
/// SAME predicate `done` enforces.
pub(crate) fn artifact_witnesses(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(md) if md.is_dir() => {
            std::fs::read_dir(path).map(|mut d| d.next().is_some()).unwrap_or(false)
        }
        Ok(md) => md.len() > 0,
        Err(_) => false,
    }
}

/// Anchor a declared artifact path for witnessing: a relative path resolves
/// against [`repo_root`] (never the invoking cwd — `make next` runs from
/// `adapters/nautilus` while direct invocations run from anywhere, and the
/// same item must witness identically at both); an absolute path passes
/// through. With no findable repo root the raw path is kept — witnessing then
/// fails toward keeping the item actionable, same as any unreadable path.
/// Crate-visible so `done` and the entry report's R12 auto-close pre-check
/// share ONE predicate + anchoring and can never disagree.
pub(crate) fn anchored(path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root().map(|r| r.join(p)).unwrap_or_else(|_| p.to_path_buf())
    }
}

/// The repo root: the first ancestor of this crate's manifest dir holding a
/// `.git` entry (a dir in a normal clone, a file in a worktree). Baked from
/// `CARGO_MANIFEST_DIR` (mirroring the trials-ledger idiom) so the answer is
/// stable regardless of the invoking cwd. Shared by every repo-root artifact
/// the `lab-next` surfaces touch (the queue file, the gate-run state).
///
/// # Errors
///
/// When no ancestor of the manifest dir holds a `.git` entry.
pub fn repo_root() -> anyhow::Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .find(|a| a.join(".git").exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("no repo root (.git) above {}", manifest.display()))
}

/// The default tracked queue path: [`repo_root`] + [`QUEUE_RELPATH`].
///
/// # Errors
///
/// When no repo root is findable (see [`repo_root`]).
pub fn default_queue_path() -> anyhow::Result<PathBuf> {
    let root = repo_root()
        .map_err(|e| anyhow::anyhow!("{e} — set {QUEUE_PATH_ENV} explicitly"))?;
    Ok(root.join(QUEUE_RELPATH))
}

#[cfg(test)]
mod tests;
