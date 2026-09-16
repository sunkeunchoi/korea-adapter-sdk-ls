//! The queue's **item model** (R6/R8/R9/R20) — what one unit of operational
//! work is, and the predicates that decide where the report renders it.
//!
//! Split from the store so the two halves of the contract read separately: this
//! file answers "what is an item and when is it actionable", `store.rs` answers
//! "how does a transition reach the file safely". Everything here is a pure
//! function of the item plus `now`; nothing touches the filesystem.
//!
//! Their interaction is the load-bearing part. [`QueueItem::is_blocked`] is
//! state carried BY the unblock condition — one field, so a blocked item always
//! names the act that frees it (R24). [`QueueItem::is_stale`] deliberately
//! exempts both blocked items and paused in-flight sequence entries, because
//! waiting work is not abandoned work. [`QueueItem::is_actionable`] is the
//! composition the report's sections are cut from.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::QUEUE_SCHEMA_VERSION;

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
    ///
    /// The fallback is UNREACHABLE by construction and exists only to avoid an
    /// `expect` on a display path: [`Self::is_blocked`] is defined as the
    /// condition being present, and every caller reaches this only for an item
    /// that predicate admitted. Both read and write additionally refuse a blank
    /// condition, so a blocked item always names a real act.
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
