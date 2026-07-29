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
//!   named item; a paused in-flight sequence entry (`sequence` set with a valid
//!   checkpoint) is never stale.
//!
//! Writes are whole-file read → mutate → atomic tmp+rename (mirroring the
//! ingest-checkpoint idiom at `nautilus-ls/src/ingest/checkpoint.rs`), so a crash
//! mid-write can never corrupt the live queue. A malformed line is a typed
//! per-line read error and the queue is never rewritten past it.

pub mod sequences;
pub mod window;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The current queue-item schema version. A newer-producer line is a typed
/// refusal on read (never a silent admit), mirroring the trials ledger's gate.
pub const QUEUE_SCHEMA_VERSION: u32 = 1;

/// The tracked queue file, relative to the REPO root (KTD2).
pub const QUEUE_RELPATH: &str = "queue/items.jsonl";

/// The test-time override for the queue path (KTD2).
pub const QUEUE_PATH_ENV: &str = "LS_QUEUE_PATH";

/// An item's window requirement (R6): which KRX window the work needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Window {
    /// Needs the KRX-open window and an attending operator (live sessions, rungs).
    OpenAttended,
    /// Fits the closed window (code, turns, gate runs).
    Closed,
    /// Window-agnostic — eligible even when the calendar is genuinely unknown.
    Any,
}

impl Window {
    /// Parse the CLI/wire spelling.
    ///
    /// # Errors
    ///
    /// When `s` is not one of `open-attended | closed | any`.
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        match s {
            "open-attended" => Ok(Window::OpenAttended),
            "closed" => Ok(Window::Closed),
            "any" => Ok(Window::Any),
            other => anyhow::bail!("window {other:?} not one of open-attended | closed | any"),
        }
    }

    /// The kebab-case tag rendered in reports.
    pub fn tag(self) -> &'static str {
        match self {
            Window::OpenAttended => "open-attended",
            Window::Closed => "closed",
            Window::Any => "any",
        }
    }
}

/// An item's declared completion signal (R8, KTD6): how the queue learns the
/// work is done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CompletionSignal {
    /// Operator close-out via the edit surface (`lab-next done`), for attended
    /// items with no tool artifact to witness.
    Explicit,
    /// A named tool event. When `artifact` is declared, `done` verifies the
    /// artifact exists and is non-empty before completing — an empty or absent
    /// read never completes the destructive transition (KTD6).
    ToolEvent {
        /// The event name (e.g. `ingest-complete`, `gate-green`).
        event: String,
        /// The artifact path that witnesses the event, when one exists on disk.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artifact: Option<String>,
    },
}

/// One queue item (KTD6 schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueItem {
    /// Schema version (gate on read).
    pub schema_version: u32,
    /// Stable id — the handle `done` / `supersede` / `superseded_by` use.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Which window the work needs (R6).
    pub window: Window,
    /// How completion is signalled (R8).
    pub completion: CompletionSignal,
    /// RFC3339 stamp of when the item was added.
    pub added_utc: String,
    /// Optional RFC3339 deadline; past it the item is stale (R9) unless it is a
    /// paused in-flight sequence entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    /// The id of the item that supersedes this one; set only by a completed
    /// `supersede` transition. A superseded item leaves the actionable view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    /// RFC3339 stamp of the completed `done` transition; done items leave the
    /// actionable view but stay in the store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_utc: Option<String>,
    /// A refused destructive transition's reason (KTD6): the item stays
    /// actionable and carries this flag until reconciled (R12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconcile: Option<String>,
    /// The in-flight sequence this entry tracks (turn / ladder / ingest / gate),
    /// when it is one. A paused in-flight sequence entry is never stale (R9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
    /// Free-form operator notes (rich migrated content lives here per R13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Supplementary reference paths — runbooks, plans, prompts (R13).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
}

impl QueueItem {
    /// A fresh open item at the current schema version.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        window: Window,
        completion: CompletionSignal,
        added_utc: impl Into<String>,
    ) -> Self {
        QueueItem {
            schema_version: QUEUE_SCHEMA_VERSION,
            id: id.into(),
            title: title.into(),
            window,
            completion,
            added_utc: added_utc.into(),
            deadline: None,
            superseded_by: None,
            done_utc: None,
            reconcile: None,
            sequence: None,
            notes: None,
            refs: Vec::new(),
        }
    }

    /// Whether the item is past its deadline at `now` (R9). A paused in-flight
    /// sequence entry is never stale; an item with no deadline never expires.
    ///
    /// # Errors
    ///
    /// When the recorded deadline is not RFC3339 (a corrupt store must be loud,
    /// never a silently-immortal item).
    pub fn is_stale(&self, now: DateTime<Utc>) -> anyhow::Result<bool> {
        if self.sequence.is_some() {
            return Ok(false);
        }
        match &self.deadline {
            None => Ok(false),
            Some(d) => {
                let deadline = DateTime::parse_from_rfc3339(d).map_err(|e| {
                    anyhow::anyhow!("item {}: deadline {d:?} is not RFC3339: {e}", self.id)
                })?;
                Ok(now > deadline.with_timezone(&Utc))
            }
        }
    }

    /// Whether the item is in the actionable view at `now` (R9): not done, not
    /// superseded, not stale. A reconcile flag does NOT remove actionability —
    /// that is the point of the flag.
    ///
    /// # Errors
    ///
    /// When the recorded deadline is unparseable (see [`Self::is_stale`]).
    pub fn is_actionable(&self, now: DateTime<Utc>) -> anyhow::Result<bool> {
        Ok(self.done_utc.is_none() && self.superseded_by.is_none() && !self.is_stale(now)?)
    }
}

/// The outcome of a destructive transition: either it completed, or hygiene
/// refused it and the item now carries a reconcile flag (KTD6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionOutcome {
    /// The transition completed; the item left the actionable view.
    Completed,
    /// The transition was refused; the item stays actionable with this flag.
    Reconcile(String),
}

/// The queue store at a given path (library-functions-over-config: the CLI
/// resolves the fixed tracked path, tests point [`QUEUE_PATH_ENV`] at a tempdir).
#[derive(Clone, Debug)]
pub struct Queue {
    path: PathBuf,
}

impl Queue {
    /// A queue at `path` (created lazily on first `add`).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Queue { path: path.into() }
    }

    /// Resolve the queue path (KTD2): [`QUEUE_PATH_ENV`] overrides; otherwise
    /// the tracked repo-root file.
    ///
    /// # Errors
    ///
    /// When no override is set and no repo root is findable (see
    /// [`default_queue_path`]).
    pub fn from_env() -> anyhow::Result<Self> {
        match std::env::var(QUEUE_PATH_ENV).ok().filter(|s| !s.trim().is_empty()) {
            Some(p) => Ok(Queue::new(p)),
            None => Ok(Queue::new(default_queue_path()?)),
        }
    }

    /// The queue file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read every item back in file order. An absent file reads empty; a
    /// malformed or newer-schema line is a typed per-line error naming the line
    /// number — never a silent skip (and the mutating paths all read first, so
    /// a malformed queue is never rewritten).
    ///
    /// # Errors
    ///
    /// When the file cannot be read, a line fails to parse, or a line carries an
    /// unsupported schema version.
    pub fn read_all(&self) -> anyhow::Result<Vec<QueueItem>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| anyhow::anyhow!("reading queue {}: {e}", self.path.display()))?;
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let item: QueueItem = serde_json::from_str(line)
                .map_err(|e| anyhow::anyhow!("queue {} line {}: {e}", self.path.display(), i + 1))?;
            if item.schema_version != QUEUE_SCHEMA_VERSION {
                anyhow::bail!(
                    "queue {} line {}: unsupported schema version {} (this build reads {})",
                    self.path.display(),
                    i + 1,
                    item.schema_version,
                    QUEUE_SCHEMA_VERSION
                );
            }
            out.push(item);
        }
        Ok(out)
    }

    /// Persist the whole queue atomically: write a sibling tmp file, then rename
    /// over the target (mirroring the ingest-checkpoint idiom) — a crash
    /// mid-write must never corrupt the live queue.
    ///
    /// # Errors
    ///
    /// When the parent cannot be created, or the tmp write / rename fails (the
    /// live file is untouched in every failure case).
    pub fn save(&self, items: &[QueueItem]) -> anyhow::Result<()> {
        let mut text = String::new();
        for item in items {
            text.push_str(&serde_json::to_string(item)?);
            text.push('\n');
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("mkdir {}: {e}", parent.display()))?;
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, text)
            .map_err(|e| anyhow::anyhow!("write queue tmp {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| anyhow::anyhow!("commit queue {}: {e}", self.path.display()))
    }

    /// Append a fresh item (R8's creation path — every item declares its
    /// completion signal here).
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, the id already exists, or the item carries
    /// an unparseable deadline.
    pub fn add(&self, item: QueueItem) -> anyhow::Result<()> {
        if let Some(d) = &item.deadline {
            DateTime::parse_from_rfc3339(d)
                .map_err(|e| anyhow::anyhow!("deadline {d:?} is not RFC3339: {e}"))?;
        }
        let mut items = self.read_all()?;
        if items.iter().any(|i| i.id == item.id) {
            anyhow::bail!("an item with id {:?} already exists in {}", item.id, self.path.display());
        }
        items.push(item);
        self.save(&items)
    }

    /// The `done` transition (R8/R9, KTD6). For a [`CompletionSignal::ToolEvent`]
    /// with a declared artifact, the artifact must exist and be non-empty —
    /// otherwise the item stays actionable with a reconcile flag (an empty or
    /// absent read never completes a destructive transition). A completed `done`
    /// clears any reconcile flag.
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, no item has `id`, or the write fails. A
    /// hygiene refusal is NOT an error — it is [`TransitionOutcome::Reconcile`].
    pub fn done(&self, id: &str, now_utc: &str) -> anyhow::Result<TransitionOutcome> {
        let mut items = self.read_all()?;
        let item = items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow::anyhow!("no queue item with id {id:?}"))?;
        if item.done_utc.is_some() {
            return Ok(TransitionOutcome::Completed); // already done — idempotent
        }
        let declared = match &item.completion {
            CompletionSignal::ToolEvent { artifact: Some(path), event } => {
                Some((path.clone(), event.clone()))
            }
            _ => None,
        };
        if let Some((path, event)) = declared {
            if !artifact_witnesses(Path::new(&path)) {
                let flag = format!(
                    "done refused: completion artifact {path} for event {event:?} is absent or empty"
                );
                item.reconcile = Some(flag.clone());
                self.save(&items)?;
                return Ok(TransitionOutcome::Reconcile(flag));
            }
        }
        item.done_utc = Some(now_utc.to_string());
        item.reconcile = None;
        self.save(&items)?;
        Ok(TransitionOutcome::Completed)
    }

    /// The `supersede` transition (R9, KTD6): record that `by` replaces `id`.
    /// The superseder must already exist in the queue — superseding by a
    /// not-yet-existing item leaves the target actionable with a reconcile flag
    /// (same hygiene as `done` on a missing artifact). A completed supersede
    /// clears any reconcile flag.
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, no item has `id`, `by` names the target
    /// itself, or the write fails. A hygiene refusal is NOT an error.
    pub fn supersede(&self, id: &str, by: &str) -> anyhow::Result<TransitionOutcome> {
        let mut items = self.read_all()?;
        if id == by {
            anyhow::bail!("an item cannot supersede itself ({id:?})");
        }
        let by_exists = items.iter().any(|i| i.id == by);
        let item = items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow::anyhow!("no queue item with id {id:?}"))?;
        if !by_exists {
            let flag = format!("supersede refused: superseding item {by:?} not in queue");
            item.reconcile = Some(flag.clone());
            self.save(&items)?;
            return Ok(TransitionOutcome::Reconcile(flag));
        }
        item.superseded_by = Some(by.to_string());
        item.reconcile = None;
        self.save(&items)?;
        Ok(TransitionOutcome::Completed)
    }
}

/// Whether an artifact path witnesses completion: it exists and is non-empty
/// (a non-empty file, or a directory with at least one entry). An unreadable
/// path is treated as absent — fail toward keeping the item actionable.
fn artifact_witnesses(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(md) if md.is_dir() => {
            std::fs::read_dir(path).map(|mut d| d.next().is_some()).unwrap_or(false)
        }
        Ok(md) => md.len() > 0,
        Err(_) => false,
    }
}

/// The default tracked queue path: ascend from this crate's manifest dir to the
/// repo root (the first ancestor holding a `.git` entry — a dir in a normal
/// clone, a file in a worktree) and join [`QUEUE_RELPATH`]. Baked from
/// `CARGO_MANIFEST_DIR` (mirroring the trials-ledger idiom) so the path is
/// stable regardless of the invoking cwd.
///
/// # Errors
///
/// When no ancestor of the manifest dir holds a `.git` entry.
pub fn default_queue_path() -> anyhow::Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .ancestors()
        .find(|a| a.join(".git").exists())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no repo root (.git) above {} — set {QUEUE_PATH_ENV} explicitly",
                manifest.display()
            )
        })?;
    Ok(root.join(QUEUE_RELPATH))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, window: Window) -> QueueItem {
        QueueItem::new(id, format!("title {id}"), window, CompletionSignal::Explicit, "2026-07-29T00:00:00Z")
    }

    #[test]
    fn absent_file_reads_empty_and_save_roundtrips_in_order() {
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("queue/items.jsonl"));
        assert!(q.read_all().unwrap().is_empty(), "absent file reads empty");
        q.add(item("a", Window::Closed)).unwrap();
        q.add(item("b", Window::Any)).unwrap();
        let back = q.read_all().unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].id, "a", "file order preserved");
        assert_eq!(back[1].id, "b");
        // No tmp sibling survives a completed save.
        let names: Vec<_> = std::fs::read_dir(tmp.path().join("queue"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["items.jsonl"], "no tmp residue: {names:?}");
    }

    #[test]
    fn unknown_schema_version_is_refused_naming_the_line() {
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        q.add(item("a", Window::Any)).unwrap();
        let mut bumped = serde_json::to_value(item("b", Window::Any)).unwrap();
        bumped["schema_version"] = serde_json::json!(999);
        let mut text = std::fs::read_to_string(q.path()).unwrap();
        text.push_str(&format!("{bumped}\n"));
        std::fs::write(q.path(), text).unwrap();
        let err = q.read_all().unwrap_err();
        assert!(err.to_string().contains("line 2"), "{err}");
        assert!(err.to_string().contains("schema version 999"), "{err}");
    }

    #[test]
    fn staleness_respects_deadline_supersede_and_the_sequence_exemption() {
        let now: DateTime<Utc> = "2026-07-29T12:00:00Z".parse().unwrap();
        let mut past = item("past", Window::Closed);
        past.deadline = Some("2026-07-28T00:00:00Z".into());
        assert!(past.is_stale(now).unwrap());
        assert!(!past.is_actionable(now).unwrap());

        let mut seq = past.clone();
        seq.sequence = Some("turn".into());
        assert!(!seq.is_stale(now).unwrap(), "a paused in-flight sequence entry is never stale");
        assert!(seq.is_actionable(now).unwrap());

        let mut superseded = item("old", Window::Closed);
        superseded.superseded_by = Some("new".into());
        assert!(!superseded.is_actionable(now).unwrap());

        let mut flagged = item("flagged", Window::Closed);
        flagged.reconcile = Some("done refused: …".into());
        assert!(flagged.is_actionable(now).unwrap(), "a reconcile flag keeps the item actionable");
    }

    #[test]
    fn the_committed_repo_queue_parses_at_the_default_path() {
        // Guards the tracked seed file (and every future migration) against a
        // line this build cannot read.
        let q = Queue::new(default_queue_path().unwrap());
        q.read_all().unwrap();
    }
}
