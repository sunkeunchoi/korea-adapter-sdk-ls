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
//!   transfers it to the superseder (R21) — so the marker survives the
//!   transitions its holder takes without a second store to reconcile.
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
        /// A relative path is REPO-ROOT-relative (anchored via [`repo_root`]
        /// before checking), so the same item witnesses identically whatever
        /// the invoking cwd; an absolute path passes through untouched.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artifact: Option<String>,
    },
}

/// One queue item (KTD6 schema).
///
/// `deny_unknown_fields` is load-bearing, not tidiness (R31; mirroring
/// [`crate::lineage_prereg::LineagePreRegistration`]). Serde reads a *missing*
/// field as its default, so without it a mistyped key — `prioriti`, or
/// `unblock_conditon` — would drop silently and the line would load clean as
/// "neither priority nor blocked". A marker that decides what gets worked on
/// next must fail loud, never default quietly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// The single-item priority marker (R1): this item outranks the ordinary
    /// deadline-then-file-order selection. At most ONE item in the store carries
    /// it — setting the marker elsewhere clears it — so priority is scarce by
    /// construction rather than by discipline. Priority is a first-class queue
    /// concept, never an encoded `deadline` (a deadline means a clock).
    // Shares `trials::is_false` (the `TrialRecord::backfill` idiom) rather than
    // a second copy: an unset marker stays ABSENT from the line instead of
    // writing `"priority":false` into every committed item.
    #[serde(default, skip_serializing_if = "crate::trials::is_false")]
    pub priority: bool,
    /// The act that would unblock this item; set means the item is BLOCKED (R2).
    /// The state and its condition are ONE field — never an overload of
    /// `sequence`, which already carries the paused-sequence label (KTD2) — so a
    /// blocked item can never exist without naming a reachable act an identified
    /// actor can perform (R24; a blank condition is refused by [`Queue::save`]).
    /// A blocked item is never stale (KTD3): it is waiting, not abandoned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unblock_condition: Option<String>,
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
            priority: false,
            unblock_condition: None,
            notes: None,
            refs: Vec::new(),
        }
    }

    /// Whether the item is blocked (R2): a recorded [`Self::unblock_condition`]
    /// IS the blocked state, so the two can never disagree. The edit surface and
    /// the entry report both ask through this one predicate.
    pub fn is_blocked(&self) -> bool {
        self.unblock_condition.is_some()
    }

    /// The recorded act that would unblock this item, for display (R3/R24).
    /// Lives beside [`Self::is_blocked`] because that predicate is what
    /// guarantees the fallback never renders: a blocked item always carries a
    /// non-blank condition ([`Queue::save`] refuses otherwise), so the fallback
    /// is fail-soft display for a store an older binary could have written, not
    /// an expected state.
    pub fn unblock_reason(&self) -> &str {
        self.unblock_condition.as_deref().unwrap_or("(unrecorded)")
    }

    /// Whether the item is past its deadline at `now` (R9). A paused in-flight
    /// sequence entry and a blocked item are never stale (KTD3 — waiting work is
    /// not abandoned work); an item with no deadline never expires.
    ///
    /// # Errors
    ///
    /// When the recorded deadline is not RFC3339 (a corrupt store must be loud,
    /// never a silently-immortal item).
    pub fn is_stale(&self, now: DateTime<Utc>) -> anyhow::Result<bool> {
        if self.sequence.is_some() || self.is_blocked() {
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
    /// When an item is blocked with a blank unblock condition, the parent cannot
    /// be created, or the tmp write / rename fails (the live file is untouched in
    /// every failure case).
    pub fn save(&self, items: &[QueueItem]) -> anyhow::Result<()> {
        // R24: a blocked item must name the act that would unblock it. `save` is
        // the single funnel every mutator writes through, so this is the one
        // place a blank condition cannot slip past — and it is checked before
        // any path is touched, so a refusal leaves the store exactly as it was.
        for item in items {
            if item.unblock_condition.as_deref().is_some_and(|c| c.trim().is_empty()) {
                anyhow::bail!(
                    "item {:?}: blocked with an empty unblock condition — a blocked \
                     item must name the act that would unblock it",
                    item.id
                );
            }
        }
        let mut text = String::new();
        for item in items {
            text.push_str(&serde_json::to_string(item)?);
            text.push('\n');
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("mkdir {}: {e}", parent.display()))?;
        }
        // PID-suffixed tmp (the gate-run.sh `tmp-$$` idiom): two concurrent
        // writers must never clobber each other's staging file.
        let tmp = self.path.with_extension(format!("jsonl.tmp-{}", std::process::id()));
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
    /// clears any reconcile flag and releases the priority marker (R21).
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, no item has `id`, or the write fails. A
    /// hygiene refusal is NOT an error — it is [`TransitionOutcome::Reconcile`].
    pub fn done(&self, id: &str, now_utc: &str) -> anyhow::Result<TransitionOutcome> {
        let mut items = self.read_all()?;
        let item = find_mut(&mut items, id)?;
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
            if !artifact_witnesses(&anchored(&path)) {
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
        // R21: the marker is the arc's FRONTIER, so completed work never keeps
        // it. Only this branch releases it — the reconcile refusal above did not
        // complete the work, and that item still needs the marker it holds.
        item.priority = false;
        self.save(&items)?;
        Ok(TransitionOutcome::Completed)
    }

    /// The `supersede` transition (R9, KTD6): record that `by` replaces `id`.
    /// The superseder must already exist in the queue — superseding by a
    /// not-yet-existing item leaves the target actionable with a reconcile flag
    /// (same hygiene as `done` on a missing artifact). A completed supersede
    /// clears any reconcile flag and transfers the priority marker (R21).
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
        let item = find_mut(&mut items, id)?;
        if !by_exists {
            let flag = format!("supersede refused: superseding item {by:?} not in queue");
            item.reconcile = Some(flag.clone());
            self.save(&items)?;
            return Ok(TransitionOutcome::Reconcile(flag));
        }
        item.superseded_by = Some(by.to_string());
        item.reconcile = None;
        // R21: the marker follows the work. If the superseded item held it, the
        // superseder holds it afterward — the frontier survives the transition
        // its head takes. Taking it here also ends the `item` borrow, so the
        // superseder is reachable in the same pass (`by_exists`, checked above,
        // is what makes the lookup certain to find it).
        let carried_priority = std::mem::take(&mut item.priority);
        if carried_priority {
            if let Some(superseder) = items.iter_mut().find(|i| i.id == by) {
                superseder.priority = true;
            }
        }
        self.save(&items)?;
        Ok(TransitionOutcome::Completed)
    }

    /// Set the single-item priority marker on `id` (R20), clearing it from every
    /// OTHER item in the same read-mutate-save. Returns the ids the marker was
    /// cleared from, in file order.
    ///
    /// The single-holder invariant is enforced on WRITE, never assumed of the
    /// store: a binary predating the field could have left several holders
    /// behind, so this clears ALL of them rather than the one it expects to
    /// find. Setting the marker on its current holder is a no-op that writes
    /// nothing (mirroring `done`'s idempotence).
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, no item has `id`, or the write fails.
    pub fn set_priority(&self, id: &str) -> anyhow::Result<Vec<String>> {
        self.move_priority(Some(id))
    }

    /// Clear the priority marker from every item (R20's `--clear`), leaving no
    /// holder. Returns the ids cleared, in file order; nothing held means
    /// nothing written.
    ///
    /// # Errors
    ///
    /// When the queue is unreadable or the write fails.
    pub fn clear_priority(&self) -> anyhow::Result<Vec<String>> {
        self.move_priority(None)
    }

    /// Move the single priority marker to `target`, or clear it everywhere when
    /// `target` is `None`. One body so the single-holder-on-write rule and the
    /// write-only-when-changed rule cannot diverge between the set and clear
    /// verbs; the CLI already models the choice as the same `Option<&str>`.
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, `target` names no item, or the write fails.
    fn move_priority(&self, target: Option<&str>) -> anyhow::Result<Vec<String>> {
        let mut items = self.read_all()?;
        if let Some(id) = target {
            // Checked BEFORE mutating, so a typo leaves the store untouched.
            if !items.iter().any(|i| i.id == id) {
                anyhow::bail!("no queue item with id {id:?}");
            }
        }
        let mut cleared = Vec::new();
        let mut changed = false;
        for item in &mut items {
            if target == Some(item.id.as_str()) {
                changed |= !item.priority;
                item.priority = true;
            } else if std::mem::take(&mut item.priority) {
                cleared.push(item.id.clone());
                changed = true;
            }
        }
        if changed {
            self.save(&items)?;
        }
        Ok(cleared)
    }

    /// Record the blocked state on `id` with the act that would unblock it
    /// (R5/R20). The state and its condition are ONE field, so blocking always
    /// names the act; re-blocking REPLACES the condition rather than stacking a
    /// second one.
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, no item has `id`, the condition is blank
    /// (refused by [`Self::save`] — the one funnel — which names the item
    /// before any path is touched), or the write fails.
    pub fn block(&self, id: &str, condition: &str) -> anyhow::Result<()> {
        let mut items = self.read_all()?;
        find_mut(&mut items, id)?.unblock_condition = Some(condition.to_string());
        self.save(&items)
    }

    /// Clear the blocked state on `id` (R20). Returns whether the item WAS
    /// blocked — unblocking an already-unblocked item is a reported no-op that
    /// writes nothing (mirroring `done`'s idempotence), never an error.
    ///
    /// # Errors
    ///
    /// When the queue is unreadable, no item has `id`, or the write fails.
    pub fn unblock(&self, id: &str) -> anyhow::Result<bool> {
        let mut items = self.read_all()?;
        if find_mut(&mut items, id)?.unblock_condition.take().is_none() {
            return Ok(false); // not blocked — nothing to write
        }
        self.save(&items)?;
        Ok(true)
    }
}

/// The one lookup every field-editing transition opens with, so the
/// unknown-id refusal is worded once rather than copied per verb. Returns a
/// mutable handle into the caller's already-read `items`, keeping the
/// read-mutate-save shape (and, for `supersede`, letting the borrow end where
/// that verb needs a second item in the same pass).
///
/// # Errors
///
/// When no item carries `id`.
fn find_mut<'a>(items: &'a mut [QueueItem], id: &str) -> anyhow::Result<&'a mut QueueItem> {
    items
        .iter_mut()
        .find(|i| i.id == id)
        .ok_or_else(|| anyhow::anyhow!("no queue item with id {id:?}"))
}

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
    fn relative_artifact_paths_anchor_to_the_repo_root_not_the_invoking_cwd() {
        // The anchoring rule itself: relative → repo-root-joined, absolute →
        // pass-through. Baked from CARGO_MANIFEST_DIR, so it is cwd-invariant.
        let root = repo_root().unwrap();
        assert_eq!(anchored("AGENTS.md"), root.join("AGENTS.md"));
        let abs = root.join("AGENTS.md");
        assert_eq!(anchored(abs.to_str().unwrap()), abs, "absolute paths pass through");

        // `done` witnesses a RELATIVE artifact via the same anchoring: a
        // repo-root-relative path to a tracked non-empty file completes
        // whatever the test process cwd happens to be (the harness resets it
        // per invocation — exactly the drift the anchoring exists to absorb).
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        let mut it = item("rel", Window::Any);
        it.completion = CompletionSignal::ToolEvent {
            event: "gate-green".into(),
            artifact: Some("AGENTS.md".into()), // repo-root-relative (documented contract)
        };
        q.add(it).unwrap();
        assert_eq!(
            q.done("rel", "2026-07-30T00:00:00Z").unwrap(),
            TransitionOutcome::Completed,
            "a repo-root-relative artifact witnesses regardless of the invoking cwd"
        );
    }

    #[test]
    fn the_committed_repo_queue_parses_at_the_default_path() {
        // Guards the tracked seed file (and every future migration) against a
        // line this build cannot read.
        let q = Queue::new(default_queue_path().unwrap());
        q.read_all().unwrap();
    }

    #[test]
    fn a_blocked_item_round_trips_with_its_unblock_condition() {
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        let mut blocked = item("parked", Window::OpenAttended);
        blocked.unblock_condition =
            Some("the operator authorizes a session on a margin-clearing head".into());
        q.add(blocked).unwrap();
        let back = q.read_all().unwrap();
        assert_eq!(back.len(), 1);
        assert!(back[0].is_blocked(), "the blocked state survives the round trip");
        assert_eq!(
            back[0].unblock_condition.as_deref(),
            Some("the operator authorizes a session on a margin-clearing head"),
            "the unblock condition survives verbatim"
        );
    }

    #[test]
    fn priority_round_trips_and_both_new_fields_are_skipped_when_absent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        let mut top = item("top", Window::Any);
        top.priority = true;
        q.add(top).unwrap();
        q.add(item("plain", Window::Any)).unwrap();
        let back = q.read_all().unwrap();
        assert!(back[0].priority, "the priority marker survives the round trip");
        assert!(!back[1].priority, "an unmarked item stays unmarked");
        assert!(!back[1].is_blocked());
        // Absent fields are skipped on write, so the committed store never
        // churns with `"priority":false` / `"unblock_condition":null` (KTD1).
        let text = std::fs::read_to_string(q.path()).unwrap();
        let plain = text.lines().nth(1).unwrap();
        assert!(!plain.contains("priority"), "absent priority is skipped: {plain}");
        assert!(!plain.contains("unblock_condition"), "absent blocked state is skipped: {plain}");
    }

    #[test]
    fn a_mistyped_priority_key_is_refused_naming_the_line() {
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        q.add(item("a", Window::Any)).unwrap();
        // R31: serde reads a MISSING field as the default, so without
        // `deny_unknown_fields` a mistyped key would drop silently and the item
        // would load clean as "neither priority nor blocked".
        let mut typo = serde_json::to_value(item("b", Window::Any)).unwrap();
        typo["prioriti"] = serde_json::json!(true);
        let mut text = std::fs::read_to_string(q.path()).unwrap();
        text.push_str(&format!("{typo}\n"));
        std::fs::write(q.path(), text).unwrap();
        let err = q.read_all().unwrap_err().to_string();
        assert!(err.contains("line 2"), "the refusal names the line: {err}");
        assert!(err.contains("prioriti"), "the refusal names the offending key: {err}");
    }

    #[test]
    fn a_pre_existing_line_with_neither_new_field_parses_at_version_1() {
        // A verbatim committed line shape (queue/items.jsonl): neither new key.
        // The fields are ADDITIVE at version 1 — a bump would make every
        // committed line unreadable and the queue unrewritable (KTD1).
        assert_eq!(QUEUE_SCHEMA_VERSION, 1, "the additive fields must NOT bump the version");
        let line = r#"{"schema_version":1,"id":"session-morning-root-manifest-freshness","title":"Add root ls-sdk and ls-core manifests to the morning freshness preflight","window":"any","completion":{"kind":"explicit"},"added_utc":"2026-08-19T12:04:34.452122+00:00","notes":"Separate shell-preflight residual.","refs":["adapters/nautilus/scripts/session-morning.sh"]}"#;
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        std::fs::write(q.path(), format!("{line}\n")).unwrap();
        let back = q.read_all().unwrap();
        assert_eq!(back.len(), 1);
        assert!(!back[0].priority, "a pre-existing line defaults to unmarked");
        assert!(!back[0].is_blocked(), "a pre-existing line defaults to unblocked");
    }

    #[test]
    fn blocked_with_a_blank_unblock_condition_is_refused() {
        // R24: a blocked item must name an act a reachable actor can perform.
        // The state and its condition are ONE field, so blocked-without-a-
        // condition is unrepresentable; a BLANK condition is the residual hole,
        // refused at `save` — the single funnel every mutator writes through.
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        let mut blank = item("blank", Window::Any);
        blank.unblock_condition = Some("   ".into());
        let err = q.add(blank).unwrap_err().to_string();
        assert!(err.contains("blank"), "the refusal names the item: {err}");
        assert!(err.contains("unblock condition"), "{err}");
        assert!(!q.path().exists(), "the refused write never created the queue file");
    }

    #[test]
    fn a_blocked_item_is_never_stale_and_stays_actionable() {
        // KTD3, the `QueueItem` half: a blocked item past its deadline is not
        // abandoned work — its recorded unblock condition is the whole point of
        // keeping it. `is_actionable` keeps its three clauses; withholding a
        // blocked item from the report's `next:` is the report's job (U3).
        let now: DateTime<Utc> = "2026-07-29T12:00:00Z".parse().unwrap();
        let mut blocked = item("parked", Window::OpenAttended);
        blocked.deadline = Some("2026-07-28T00:00:00Z".into());
        assert!(blocked.is_stale(now).unwrap(), "unblocked and past its deadline: stale");
        blocked.unblock_condition = Some("a certified head clears its frozen margin".into());
        assert!(!blocked.is_stale(now).unwrap(), "a blocked item is never stale");
        assert!(blocked.is_actionable(now).unwrap());
    }

    #[test]
    fn setting_priority_moves_the_marker_and_repairs_a_multi_holder_store() {
        // R20 + R1's scarcity: the set path must NOT assume the store already
        // holds at most one marker. An older binary ignored the field entirely,
        // so a store can arrive with several holders; a set converges it to
        // exactly one in the same read-mutate-save.
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        for id in ["a", "b", "c"] {
            q.add(item(id, Window::Any)).unwrap();
        }
        let mut seeded = q.read_all().unwrap();
        seeded[0].priority = true;
        seeded[1].priority = true; // what an older binary could leave behind
        q.save(&seeded).unwrap();

        let cleared = q.set_priority("c").unwrap();
        assert_eq!(cleared, ["a", "b"], "every OTHER holder is cleared, in file order");
        let back = q.read_all().unwrap();
        let held: Vec<&str> =
            back.iter().filter(|i| i.priority).map(|i| i.id.as_str()).collect();
        assert_eq!(held, ["c"], "exactly one holder after a set");

        // Re-setting the current holder is a no-op that clears nothing.
        assert!(q.set_priority("c").unwrap().is_empty(), "no other holder to clear");
        let back = q.read_all().unwrap();
        assert_eq!(back.iter().filter(|i| i.priority).count(), 1, "still exactly one holder");
    }

    #[test]
    fn clear_priority_leaves_no_holder_and_reports_what_it_cleared() {
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        q.add(item("a", Window::Any)).unwrap();
        q.add(item("b", Window::Any)).unwrap();
        q.set_priority("a").unwrap();

        assert_eq!(q.clear_priority().unwrap(), ["a"], "the cleared holder is named");
        assert!(q.read_all().unwrap().iter().all(|i| !i.priority), "no holder remains");
        assert!(q.clear_priority().unwrap().is_empty(), "clearing an unheld marker is a no-op");
        // The cleared marker leaves the line entirely (skip_serializing_if).
        let raw = std::fs::read_to_string(q.path()).unwrap();
        assert!(!raw.contains("priority"), "a cleared marker writes no key: {raw}");
    }

    #[test]
    fn the_field_editing_mutators_refuse_an_unknown_id_without_touching_the_store() {
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        q.add(item("only", Window::Any)).unwrap();
        let before = std::fs::read_to_string(q.path()).unwrap();

        let errs = [
            q.set_priority("ghost").unwrap_err(),
            q.block("ghost", "the operator acts").unwrap_err(),
            q.unblock("ghost").unwrap_err(),
        ];
        for err in &errs {
            assert!(err.to_string().contains("ghost"), "the refusal names the id: {err}");
        }
        assert_eq!(
            std::fs::read_to_string(q.path()).unwrap(),
            before,
            "a refused mutator leaves the store byte-identical"
        );
    }

    #[test]
    fn block_records_the_condition_and_unblock_restores_the_plain_item() {
        // R5/R20: the verb the U4 migration runs through. Blocked-ness and its
        // condition are ONE field, so recording the state records the act.
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        q.add(item("parked", Window::OpenAttended)).unwrap();

        q.block("parked", "the operator authorizes a margin-clearing session").unwrap();
        let back = q.read_all().unwrap();
        assert!(back[0].is_blocked(), "the item is blocked");
        assert_eq!(
            back[0].unblock_condition.as_deref(),
            Some("the operator authorizes a margin-clearing session"),
            "the condition is recorded verbatim"
        );

        // Re-blocking REPLACES the condition rather than stacking a second one.
        q.block("parked", "a certified head clears its frozen margin").unwrap();
        assert_eq!(
            q.read_all().unwrap()[0].unblock_condition.as_deref(),
            Some("a certified head clears its frozen margin")
        );

        assert!(q.unblock("parked").unwrap(), "the item was blocked");
        assert!(!q.read_all().unwrap()[0].is_blocked(), "unblocked");
        assert!(
            !q.unblock("parked").unwrap(),
            "unblocking an unblocked item is a reported no-op, not an error"
        );
        let raw = std::fs::read_to_string(q.path()).unwrap();
        assert!(!raw.contains("unblock_condition"), "the cleared state writes no key: {raw}");
    }

    #[test]
    fn block_with_a_blank_condition_is_refused_naming_the_item() {
        // R24 through the ONE funnel: `save` refuses before touching any path,
        // so `block` needs no second check and the store stays byte-identical.
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        q.add(item("parked", Window::Any)).unwrap();
        let before = std::fs::read_to_string(q.path()).unwrap();

        let err = q.block("parked", "   ").unwrap_err().to_string();
        assert!(err.contains("parked"), "the refusal names the item: {err}");
        assert!(err.contains("unblock condition"), "{err}");
        assert_eq!(std::fs::read_to_string(q.path()).unwrap(), before, "store untouched");
    }

    #[test]
    fn done_clears_the_priority_marker_only_when_it_completes() {
        // R21: the marker is the arc's frontier, so a COMPLETED head must not
        // keep it. A reconcile refusal did not complete the work — the holder
        // keeps the marker and stays actionable.
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        let mut refused = item("refused", Window::Any);
        refused.completion = CompletionSignal::ToolEvent {
            event: "gate-green".into(),
            artifact: Some(tmp.path().join("absent.json").to_string_lossy().into_owned()),
        };
        q.add(refused).unwrap();
        q.set_priority("refused").unwrap();
        assert!(
            matches!(
                q.done("refused", "2026-07-30T00:00:00Z").unwrap(),
                TransitionOutcome::Reconcile(_)
            ),
            "an absent completion artifact refuses the transition"
        );
        assert!(
            q.read_all().unwrap()[0].priority,
            "a refused done keeps the marker — the work is not done"
        );

        q.add(item("real", Window::Any)).unwrap();
        q.set_priority("real").unwrap();
        assert_eq!(q.done("real", "2026-07-30T00:00:00Z").unwrap(), TransitionOutcome::Completed);
        assert!(
            q.read_all().unwrap().iter().all(|i| !i.priority),
            "a completed done leaves no holder"
        );
    }

    #[test]
    fn supersede_transfers_the_priority_marker_only_when_it_completes() {
        // R21: the arc's frontier survives the transition its head takes. The
        // marker follows the work to the superseder; a reconcile refusal leaves
        // it where it was.
        let tmp = tempfile::TempDir::new().unwrap();
        let q = Queue::new(tmp.path().join("items.jsonl"));
        q.add(item("head", Window::Any)).unwrap();
        q.set_priority("head").unwrap();

        assert!(
            matches!(
                q.supersede("head", "successor").unwrap(),
                TransitionOutcome::Reconcile(_)
            ),
            "a not-yet-existing superseder refuses the transition"
        );
        assert!(q.read_all().unwrap()[0].priority, "a refused supersede keeps the marker");

        q.add(item("successor", Window::Any)).unwrap();
        assert_eq!(q.supersede("head", "successor").unwrap(), TransitionOutcome::Completed);
        let back = q.read_all().unwrap();
        let held: Vec<&str> =
            back.iter().filter(|i| i.priority).map(|i| i.id.as_str()).collect();
        assert_eq!(held, ["successor"], "the marker follows the work to the superseder");

        // Superseding a NON-holder never grants the marker.
        q.add(item("other", Window::Any)).unwrap();
        q.add(item("other-new", Window::Any)).unwrap();
        assert_eq!(q.supersede("other", "other-new").unwrap(), TransitionOutcome::Completed);
        let back = q.read_all().unwrap();
        let held: Vec<&str> =
            back.iter().filter(|i| i.priority).map(|i| i.id.as_str()).collect();
        assert_eq!(held, ["successor"], "still exactly one holder, unchanged");
    }
}
