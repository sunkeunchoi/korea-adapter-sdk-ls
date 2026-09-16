//! The queue **store** (U1, R1/R8/R9/R20/R21; KTD6) — every path that reads or
//! writes `queue/items.jsonl`, and the hygiene a transition must clear.
//!
//! Split from the item model so the durability contract reads on its own. Three
//! properties live here and nowhere else:
//!
//! - **Whole-file read → mutate → atomic tmp+rename.** Mirrors the ingest
//!   checkpoint idiom, so a crash mid-write cannot corrupt the live queue. A
//!   malformed line is a typed per-line read error and the file is never
//!   rewritten past it.
//! - **Destructive transitions never complete from an empty read** (KTD6). A
//!   missing or empty completion artifact, or a superseder not yet in the queue,
//!   comes back as [`TransitionOutcome::Reconcile`] and leaves the item
//!   actionable — the refusal is returned, never logged and swallowed.
//! - **The priority marker is single-holder ON WRITE** (R1/R21). `set_priority`
//!   clears every other holder in the same read-mutate-save, and `save` refuses
//!   to persist a second one, so the invariant does not depend on which verb
//!   reached the write.

use std::path::{Path, PathBuf};

use chrono::DateTime;

use super::{
    anchored, artifact_witnesses, default_queue_path, CompletionSignal, QueueItem,
    QUEUE_PATH_ENV, QUEUE_SCHEMA_VERSION,
};

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

    /// Read every item back in file order. An absent file reads empty.
    ///
    /// A malformed line, an unknown key, a newer schema version, or a blank
    /// unblock condition is a typed error NAMING THE LINE — never a silent skip,
    /// and the mutating paths all read first, so a malformed queue is never
    /// rewritten past the problem.
    ///
    /// Note the blast radius, because it is wider than per-line: the error aborts
    /// the WHOLE read, so one bad line disables every `lab-next` surface —
    /// including the verbs that would repair it — until it is corrected by hand.
    /// That is deliberate for a store whose whole purpose is to be authoritative
    /// about what to do next: rendering a partial queue would answer that question
    /// wrongly. It does mean a field added by a future binary is a hard stop for
    /// this one (`deny_unknown_fields`), so a field addition must ship with a
    /// rebuild of every binary that touches the file.
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
            if item.unblock_condition.as_deref().is_some_and(|c| c.trim().is_empty()) {
                // `save` refuses this, but a hand-edit or a future writer could
                // plant it. A blank condition reads as BLOCKED — exempt from
                // staleness and from both reconciliation passes — while naming no
                // act, so R24's guarantee of a reachable path back would silently
                // not hold. Refuse on read too, naming the line.
                anyhow::bail!(
                    "queue {} line {}: item {:?} is blocked with a blank unblock condition — \
                     a blocked item must name the act that would unblock it (R24)",
                    self.path.display(),
                    i + 1,
                    item.id
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
    /// When an item is blocked with a blank unblock condition, when more than one
    /// item carries the priority marker, when the parent cannot be created, or when
    /// the tmp write / rename fails. Every refusal happens before the tmp file is
    /// written, so the live queue is never modified by a failed save (a failed
    /// rename can leave an orphan `.tmp-<pid>` sibling, which nothing reads).
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
        // R1: at most ONE holder, enforced at the funnel rather than only in the
        // verb that sets it. `move_priority` converges a store it reads, but it is
        // not the only writer — `supersede` transfers the marker onto its
        // superseder, which would ADD a second holder to a store that already
        // carried a stale one. Checking here means no path can persist an
        // ambiguous frontier, whatever route reached the write.
        let holders: Vec<&str> =
            items.iter().filter(|i| i.priority).map(|i| i.id.as_str()).collect();
        if holders.len() > 1 {
            anyhow::bail!(
                "refusing to write {} priority holders ({}) — exactly one item holds the \
                 marker at a time (R1); clear it with `lab-next priority --clear` and set it \
                 on the one item that should hold it",
                holders.len(),
                holders.join(", ")
            );
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
        if item.is_blocked() {
            // R22 at the STORE, not only in the report. The report excludes blocked
            // items from its auto-close pass, but it reads the store, decides, and
            // then calls back in — so a `block` landing inside that window would
            // let the auto-close complete work whose external act never happened.
            // Refusing here closes that race at the one funnel both paths share.
            return Ok(TransitionOutcome::Reconcile(format!(
                "blocked: {} — the act that would unblock it has not happened, so `done` \
                 refuses; `lab-next unblock {id}` first if it really is finished",
                item.unblock_reason()
            )));
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
        // superseder holds it afterward — the frontier survives the transition its
        // head takes. Taking it here also ends the `item` borrow, so the superseder
        // is reachable in the same pass (`by_exists`, checked above, makes the
        // lookup certain to find it).
        //
        // It is RELEASED rather than transferred when the superseder is itself
        // terminal: the report renders only actionable items, so parking the marker
        // there would lose the frontier with no holder visible anywhere. Leaving no
        // holder is the honest outcome — the operator re-pins the live successor.
        let carried_priority = std::mem::take(&mut item.priority);
        if carried_priority {
            if let Some(superseder) = items.iter_mut().find(|i| i.id == by) {
                if superseder.done_utc.is_none() && superseder.superseded_by.is_none() {
                    superseder.priority = true;
                }
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
            let Some(t) = items.iter().find(|i| i.id == id) else {
                anyhow::bail!("no queue item with id {id:?}");
            };
            // Existence is not enough. The report renders only ACTIONABLE items, so
            // parking the one scarce marker on completed or superseded work would
            // displace the real holder, report success, and leave no holder visible
            // in any section — losing the frontier silently, which is the opposite
            // of the scarcity R1 exists to provide.
            if let Some(done) = &t.done_utc {
                anyhow::bail!("item {id:?} completed at {done} — priority marks open work");
            }
            if let Some(by) = &t.superseded_by {
                anyhow::bail!("item {id:?} was superseded by {by:?} — set priority on {by:?}");
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
