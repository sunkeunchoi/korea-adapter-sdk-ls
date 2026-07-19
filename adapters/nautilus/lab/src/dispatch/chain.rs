//! The dispatch chain (U1, KTD1, KTD2) — an append-only, hash-chained JSONL store at
//! `<data_home>/dispatch/chain.jsonl` that carries pre-flight outcomes, authorization,
//! and capital-ladder rung state in one artifact family.
//!
//! **Fail-closed by construction.** Every record chains to the previous by `prev_hash`
//! (SHA-256 of the previous record's canonical body bytes) and carries its own
//! `record_hash`. [`DispatchChain::load`] verifies the whole current epoch; any defect
//! — unreadable, truncated, unknown record type, or a hash mismatch — authorizes
//! **rung 0** (no live session) rather than erroring, so a corrupt or deleted chain
//! can never silently escape the suspended state. Chain genesis is an explicit rung-1
//! registration record, never an implicit default.
//!
//! **Repair is an epoch rollover, never a rewrite** (KTD1). A defective chain is
//! archived in place — content-hashed under `dispatch/archive/`, never deleted or
//! mutated — and a fresh epoch opens with a re-registration record whose `prev_hash`
//! is the SHA-256 of the archived file's full bytes. Verification validates the
//! current epoch and keeps the archived-epoch citation.
//!
//! **Appends serialize under `LockKind::Dispatch` and only under it** (KTD2). Dispatch
//! has no counterpart, so a safety-trip or consumption append from a live session that
//! already holds the Live lock is permitted; the *gate* refuses a new `--dispatch`
//! attempt while the Live lock is held (probed explicitly, U2/U3).
//!
//! **KST trading date, computed once at append** (Dependencies). A KRX session spans
//! UTC midnight and run ids are UTC-stamped, so the date is never derived from a UTC
//! run stamp — it is the Asia/Seoul calendar date of the append instant, stored in the
//! record.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use chrono_tz::Asia::Seoul;
use serde::{Deserialize, Serialize};

use nautilus_ls::lock::{AdvisoryLock, LockKind};

use crate::artifacts::manifest::hash_bytes;
use crate::artifacts::scrub;
use crate::dispatch::{CheckRecord, Deferral, UnknownOverride, RUNG_MIN};

/// The dispatch home directory name under `<data_home>/`.
pub const DISPATCH_DIR: &str = "dispatch";
/// The active-epoch chain file name.
pub const CHAIN_FILE: &str = "chain.jsonl";
/// The archive directory for rolled-over epochs.
pub const ARCHIVE_DIR: &str = "archive";
/// The `prev_hash` sentinel of a true genesis (epoch 0's first record). A
/// re-registration's `prev_hash` is instead the archived epoch's content hash.
pub const GENESIS_PREV_HASH: &str = "GENESIS";

/// The KST (Asia/Seoul) trading date of a UTC instant, `YYYY-MM-DD`. KST has no DST, so
/// this is a fixed +9h shift; a 23:59 UTC append and a 00:01-UTC-next-day append during
/// the same KRX session share a date (Dependencies).
pub fn kst_trading_date(now: DateTime<Utc>) -> String {
    now.with_timezone(&Seoul).format("%Y-%m-%d").to_string()
}

/// Which safety mechanism fired (KTD4, R14(d)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyTripKind {
    /// The runtime kill switch was engaged.
    KillSwitch,
    /// The watchdog dead-man timer fired.
    Watchdog,
    /// The session max-loss breaker fired.
    Breaker,
}

/// Whether a safety-trip record engages or clears the mechanism (KTD4). Clearing is an
/// explicit, recorded operator action behind the same nonce/no-TTY gate as deferrals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TripAction {
    /// The mechanism fired / was engaged.
    Engage,
    /// An operator explicitly cleared a prior engagement.
    Clear,
}

/// Whether a session-dispatch was authorized (green, possibly with deferrals) or
/// refused (a non-deferrable red, or an undeferred deferrable red).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchOutcome {
    /// All checks green or explicitly deferred — the session may mount.
    Green,
    /// Refused — no session mounts; the record is chain history, not a silent exit.
    Refused,
}

/// A session-dispatch record's payload (R2, F1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionDispatch {
    /// Green or refused.
    pub outcome: DispatchOutcome,
    /// Per-check outcomes.
    pub checks: Vec<CheckRecord>,
    /// Any explicit deferrals with operator attribution.
    pub deferrals: Vec<Deferral>,
    /// The readiness verdict summary this dispatch ran under, when computed (U9).
    #[serde(default)]
    pub readiness: Option<String>,
    /// The attended Unknown-date override that authorized this dispatch, when a bound,
    /// audited override proceeded an Unknown calendar date (U12, KTD8). Absent otherwise.
    #[serde(default)]
    pub unknown_override: Option<UnknownOverride>,
}

/// An escalation record's payload (R13, F2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Escalation {
    /// The rung stepped up from.
    pub from_rung: u8,
    /// The rung authorized.
    pub to_rung: u8,
    /// The qualifying clean sessions cited as evidence (run ids).
    pub evidence_run_ids: Vec<String>,
}

/// A de-escalation record's payload (R14, F3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeEscalation {
    /// The rung stepped down from.
    pub from_rung: u8,
    /// The rung authorized after the step-down.
    pub to_rung: u8,
    /// Every limit event driving the step-down (all listed; one step per session).
    pub events: Vec<String>,
    /// The consumed-through watermark so no event double-fires (KTD8).
    pub consumed_through: String,
}

/// A re-registration record's payload (KTD1) — genesis re-open after a defect, or a
/// rung-0 re-qualification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReRegistration {
    /// The rung this re-registration authorizes (genesis re-open → 1; a rung-0
    /// re-qualification → 1).
    pub set_rung: u8,
    /// The archived defective epoch's full-bytes content hash, when this re-registration
    /// opens a fresh epoch after a rollover (`None` for an in-place re-qualification that
    /// does not roll the epoch).
    #[serde(default)]
    pub archived_epoch_hash: Option<String>,
    /// The operator-supplied reason — scrubbed before the record lands.
    pub reason: String,
}

/// A safety-trip record's payload (KTD4). Persisted at trip time, before any error
/// path can bail, so a fresh dispatch process observes the trip (the runtime kill
/// switch is a per-process `AtomicBool` that a new process would otherwise read
/// disengaged).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyTrip {
    /// Which mechanism.
    pub trip: SafetyTripKind,
    /// Engage or clear.
    pub action: TripAction,
    /// The run id whose session tripped, when known.
    #[serde(default)]
    pub run_id: Option<String>,
    /// Free-text detail — scrubbed before the record lands.
    pub detail: String,
}

/// A consumption marker on a prior session-dispatch (KTD2): a green dispatch is used by
/// exactly one session. Records the mounted run id so R14(f) residue classification is
/// chain-driven (U6, U10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consumption {
    /// The session-dispatch record id being consumed.
    pub dispatch_record_id: String,
    /// The mounted run id (recorded at mount time).
    #[serde(default)]
    pub run_id: Option<String>,
}

/// The typed record payload. Adjacently tagged so an unknown `type` string is a
/// deserialization error — which [`DispatchChain::load`] treats as a defect (rung 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RecordKind {
    /// The explicit rung-1 chain genesis.
    Genesis,
    /// A dispatch attempt (green or refused).
    SessionDispatch(SessionDispatch),
    /// An escalation to the next rung.
    Escalation(Escalation),
    /// An auto de-escalation.
    DeEscalation(DeEscalation),
    /// A chain-epoch re-open or a rung-0 re-qualification.
    ReRegistration(ReRegistration),
    /// A safety-mechanism engage/clear, written at trip time.
    SafetyTrip(SafetyTrip),
    /// A single-use consumption marker on a session-dispatch.
    Consumption(Consumption),
}

/// The hashed body of a chain record. Everything here is covered by `record_hash`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBody {
    /// A per-epoch-unique record id (`<kst-date>-<seq>`).
    pub record_id: String,
    /// The KST trading date this record keys on (Dependencies).
    pub kst_trading_date: String,
    /// SHA-256 hex of the previous record's canonical body (or the genesis/archive
    /// anchor for an epoch's first record).
    pub prev_hash: String,
    /// The rung the chain authorizes as of this record.
    pub chain_rung: u8,
    /// The effective rung a session runs at (equals `chain_rung` except under
    /// rung-1 probation, where it is forced to 1, R11).
    pub effective_rung: u8,
    /// The pre-registration file content hash this record ran under, when a values
    /// file was loaded (KTD9).
    #[serde(default)]
    pub prereg_hash: Option<String>,
    /// The typed payload.
    pub kind: RecordKind,
}

/// A full chain record: a hashed body plus its own hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainRecord {
    /// The hashed body.
    pub body: RecordBody,
    /// SHA-256 hex of the canonical body bytes.
    pub record_hash: String,
}

/// Canonical body bytes for hashing: compact serde_json (struct field order is
/// deterministic; no maps are used, so the bytes are stable across round-trips).
fn canonical_bytes(body: &RecordBody) -> Vec<u8> {
    serde_json::to_vec(body).expect("RecordBody is always serializable")
}

fn compute_record_hash(body: &RecordBody) -> String {
    hash_bytes(&canonical_bytes(body))
}

impl ChainRecord {
    /// Seal a body into a record by computing its canonical hash. The building block
    /// for appends; also lets a test construct a record whose body hashes correctly but
    /// whose `prev_hash` deliberately does not link, isolating the link-verification arm.
    pub fn sealed(body: RecordBody) -> Self {
        let record_hash = compute_record_hash(&body);
        ChainRecord { body, record_hash }
    }
}

/// The high-level status a [`load`](DispatchChain::load) resolved to.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainStatus {
    /// No chain file, or an empty one — the legitimate pre-genesis state (rung 0).
    NoChain,
    /// A fully verified current epoch.
    Valid,
    /// The current epoch failed verification — fail-closed to rung 0.
    Defective(String),
}

/// The consumption/expiry state of the most recent session-dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum MountAuthz {
    /// A green, unconsumed, same-day dispatch is available to mount at this rung.
    Ready {
        /// The session-dispatch record id (the mounter marks it consumed).
        record_id: String,
        /// The chain-authorized rung.
        chain_rung: u8,
        /// The effective rung (rung 1 under probation).
        effective_rung: u8,
    },
    /// The most recent green dispatch is already consumed by a session.
    Consumed,
    /// The most recent green dispatch is from a previous KST trading day.
    Expired,
    /// No green dispatch is available to mount.
    None,
}

/// The most recent session-dispatch's derived state.
#[derive(Debug, Clone, PartialEq)]
pub struct LastSessionDispatch {
    /// Its record id.
    pub record_id: String,
    /// Its KST trading date.
    pub kst_trading_date: String,
    /// Its outcome.
    pub outcome: DispatchOutcome,
    /// The chain-authorized rung it named.
    pub chain_rung: u8,
    /// The effective rung it named.
    pub effective_rung: u8,
    /// Whether a consumption marker has consumed it.
    pub consumed: bool,
    /// The mounted run id, if consumed.
    pub consumed_run_id: Option<String>,
}

/// The verified chain state (KTD1). Any verification failure yields
/// [`ChainStatus::Defective`] with `authorized_rung == 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainState {
    /// The verification status.
    pub status: ChainStatus,
    /// The rung the chain authorizes (0 for NoChain/Defective).
    pub authorized_rung: u8,
    /// The tip record's hash (the next append's `prev_hash`), when the epoch is valid
    /// and non-empty.
    pub tip_hash: Option<String>,
    /// The pre-registration hash the tip ran under, if any.
    pub last_prereg_hash: Option<String>,
    /// The most recent session-dispatch, if any.
    pub last_session_dispatch: Option<LastSessionDispatch>,
    /// Whether the kill switch is engaged (latest un-cleared kill-switch trip), KTD4.
    pub kill_switch_engaged: bool,
    /// Every verified record of the current epoch, in append order (downstream scans:
    /// deferral counts, de-escalation watermarks, safety-trip mining).
    pub records: Vec<ChainRecord>,
}

impl ChainState {
    fn suspended(status: ChainStatus) -> Self {
        ChainState {
            status,
            authorized_rung: 0,
            tip_hash: None,
            last_prereg_hash: None,
            last_session_dispatch: None,
            kill_switch_engaged: false,
            records: Vec::new(),
        }
    }

    /// Whether a live session may mount today, honoring single-use consumption and
    /// same-day expiry (KTD2). `today_kst` is the current KST trading date.
    pub fn mount_authz(&self, today_kst: &str) -> MountAuthz {
        if self.authorized_rung == 0 {
            return MountAuthz::None;
        }
        let Some(last) = &self.last_session_dispatch else {
            return MountAuthz::None;
        };
        if last.outcome != DispatchOutcome::Green {
            return MountAuthz::None;
        }
        if last.consumed {
            return MountAuthz::Consumed;
        }
        if last.kst_trading_date != today_kst {
            return MountAuthz::Expired;
        }
        MountAuthz::Ready {
            record_id: last.record_id.clone(),
            chain_rung: last.chain_rung,
            effective_rung: last.effective_rung,
        }
    }
}

/// The append-only, hash-chained dispatch chain store.
#[derive(Debug, Clone)]
pub struct DispatchChain {
    dir: PathBuf,
}

impl DispatchChain {
    /// Open (creating the dispatch dir if needed) the chain under `<data_home>/dispatch/`.
    pub fn open(data_home: &Path) -> anyhow::Result<Self> {
        let dir = data_home.join(DISPATCH_DIR);
        std::fs::create_dir_all(&dir)?;
        Ok(DispatchChain { dir })
    }

    /// The dispatch home directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The active-epoch chain file path.
    pub fn chain_path(&self) -> PathBuf {
        self.dir.join(CHAIN_FILE)
    }

    /// Read the raw lines of the current epoch (empty if absent).
    fn raw_lines(&self) -> anyhow::Result<Vec<String>> {
        let path = self.chain_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&path)?;
        Ok(text.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect())
    }

    /// The tip record's hash and the current epoch's record count, read under the caller's
    /// lock. Bails if the tip line is unparseable — the caller must repair via
    /// [`reregister`](Self::reregister) first, never extend a corrupt chain.
    fn tip(&self) -> anyhow::Result<(String, usize)> {
        let lines = self.raw_lines()?;
        match lines.last() {
            None => Ok((GENESIS_PREV_HASH.to_string(), 0)),
            Some(last) => {
                let rec: ChainRecord = serde_json::from_str(last).map_err(|e| {
                    anyhow::anyhow!("refusing to append onto an unparseable chain tip: {e}")
                })?;
                Ok((rec.record_hash, lines.len()))
            }
        }
    }

    /// Append a record at `now`, authorizing `chain_rung`/`effective_rung`, citing
    /// `prereg_hash`. `prev_hash` resolves from the current tip (the genesis sentinel
    /// for an empty epoch). Free-text payload fields are scrubbed. Serialized under
    /// `LockKind::Dispatch`.
    pub fn append(
        &self,
        now: DateTime<Utc>,
        chain_rung: u8,
        effective_rung: u8,
        prereg_hash: Option<String>,
        kind: RecordKind,
    ) -> anyhow::Result<ChainRecord> {
        let _lock = AdvisoryLock::acquire(&self.dir, LockKind::Dispatch)
            .map_err(|e| anyhow::anyhow!("dispatch chain append refused (lock): {e}"))?;
        let (prev_hash, seq) = self.tip()?;
        let kst = kst_trading_date(now);
        let body = RecordBody {
            record_id: format!("{kst}-{seq:04}"),
            kst_trading_date: kst,
            prev_hash,
            chain_rung,
            effective_rung,
            prereg_hash,
            kind: scrub_kind(kind),
        };
        self.write_body(body)
    }

    /// Epoch-rollover repair (KTD1): archive the current (defective) chain file
    /// content-hashed under `dispatch/archive/`, open a fresh epoch, and append a
    /// re-registration whose `prev_hash` is the archived file's full-bytes hash. The
    /// archived file is never deleted or rewritten.
    pub fn reregister(
        &self,
        now: DateTime<Utc>,
        set_rung: u8,
        prereg_hash: Option<String>,
        reason: &str,
    ) -> anyhow::Result<ChainRecord> {
        let _lock = AdvisoryLock::acquire(&self.dir, LockKind::Dispatch)
            .map_err(|e| anyhow::anyhow!("dispatch chain re-registration refused (lock): {e}"))?;
        let path = self.chain_path();
        let archived_hash = if path.exists() {
            let bytes = std::fs::read(&path)?;
            let hash = hash_bytes(&bytes);
            let archive_dir = self.dir.join(ARCHIVE_DIR);
            std::fs::create_dir_all(&archive_dir)?;
            let archived = archive_dir.join(format!("chain.{hash}.jsonl"));
            // Never overwrite an existing archive of the same content (idempotent).
            if !archived.exists() {
                std::fs::rename(&path, &archived)?;
            } else {
                // Same content already archived — drop the defective live file.
                std::fs::remove_file(&path)?;
            }
            Some(hash)
        } else {
            None
        };
        let kst = kst_trading_date(now);
        let prev_hash = archived_hash.clone().unwrap_or_else(|| GENESIS_PREV_HASH.to_string());
        let body = RecordBody {
            record_id: format!("{kst}-0000"),
            kst_trading_date: kst,
            prev_hash,
            chain_rung: set_rung,
            effective_rung: set_rung,
            prereg_hash,
            kind: RecordKind::ReRegistration(ReRegistration {
                set_rung,
                archived_epoch_hash: archived_hash,
                reason: scrub(reason),
            }),
        };
        self.write_body(body)
    }

    fn write_body(&self, body: RecordBody) -> anyhow::Result<ChainRecord> {
        let record = ChainRecord::sealed(body);
        // One compact line + newline in a single write_all, so a crash mid-write can
        // only leave a torn (unparseable) final line — which load() rejects — never a
        // silently-admitted partial record.
        let line = format!("{}\n", serde_json::to_string(&record)?);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.chain_path())?;
        file.write_all(line.as_bytes())?;
        Ok(record)
    }

    /// Load and verify the current epoch. Any defect — unreadable, truncated, unknown
    /// record type, or a hash mismatch — yields [`ChainStatus::Defective`] with
    /// `authorized_rung == 0` (fail-closed). An absent/empty chain is
    /// [`ChainStatus::NoChain`], also rung 0.
    pub fn load(&self) -> ChainState {
        let lines = match self.raw_lines() {
            Ok(l) => l,
            Err(e) => return ChainState::suspended(ChainStatus::Defective(format!("unreadable: {e}"))),
        };
        if lines.is_empty() {
            return ChainState::suspended(ChainStatus::NoChain);
        }

        let mut records: Vec<ChainRecord> = Vec::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            match serde_json::from_str::<ChainRecord>(line) {
                Ok(rec) => records.push(rec),
                Err(e) => {
                    return ChainState::suspended(ChainStatus::Defective(format!(
                        "record {} unparseable (truncation or unknown record type): {e}",
                        i + 1
                    )));
                }
            }
        }

        // Verify per-record hashes and the prev_hash link.
        for (i, rec) in records.iter().enumerate() {
            if compute_record_hash(&rec.body) != rec.record_hash {
                return ChainState::suspended(ChainStatus::Defective(format!(
                    "record {} hash mismatch (tamper)",
                    i + 1
                )));
            }
            if i == 0 {
                // An epoch must open with a registration record (genesis or
                // re-registration); its prev_hash is the accepted anchor.
                match &rec.body.kind {
                    RecordKind::Genesis | RecordKind::ReRegistration(_) => {}
                    _ => {
                        return ChainState::suspended(ChainStatus::Defective(
                            "epoch does not open with a genesis/re-registration record".to_string(),
                        ));
                    }
                }
            } else if rec.body.prev_hash != records[i - 1].record_hash {
                return ChainState::suspended(ChainStatus::Defective(format!(
                    "record {} prev_hash does not link to record {}",
                    i + 1,
                    i
                )));
            }
        }

        // Walk the verified records to derive rung state, the last session-dispatch,
        // and kill-switch state.
        let mut authorized_rung: u8 = 0;
        let mut last_prereg_hash: Option<String> = None;
        let mut last_session_dispatch: Option<LastSessionDispatch> = None;
        let mut kill_switch_engaged = false;
        for rec in &records {
            if rec.body.prereg_hash.is_some() {
                last_prereg_hash = rec.body.prereg_hash.clone();
            }
            match &rec.body.kind {
                RecordKind::Genesis => authorized_rung = rec.body.chain_rung.max(RUNG_MIN),
                RecordKind::ReRegistration(r) => authorized_rung = r.set_rung,
                RecordKind::Escalation(e) => authorized_rung = e.to_rung,
                RecordKind::DeEscalation(d) => authorized_rung = d.to_rung,
                RecordKind::SessionDispatch(s) => {
                    last_session_dispatch = Some(LastSessionDispatch {
                        record_id: rec.body.record_id.clone(),
                        kst_trading_date: rec.body.kst_trading_date.clone(),
                        outcome: s.outcome,
                        chain_rung: rec.body.chain_rung,
                        effective_rung: rec.body.effective_rung,
                        consumed: false,
                        consumed_run_id: None,
                    });
                }
                RecordKind::Consumption(c) => {
                    if let Some(last) = &mut last_session_dispatch {
                        if last.record_id == c.dispatch_record_id {
                            last.consumed = true;
                            last.consumed_run_id = c.run_id.clone();
                        }
                    }
                }
                RecordKind::SafetyTrip(t) => {
                    if t.trip == SafetyTripKind::KillSwitch {
                        kill_switch_engaged = matches!(t.action, TripAction::Engage);
                    }
                }
            }
        }

        ChainState {
            status: ChainStatus::Valid,
            authorized_rung,
            tip_hash: records.last().map(|r| r.record_hash.clone()),
            last_prereg_hash,
            last_session_dispatch,
            kill_switch_engaged,
            records,
        }
    }
}

/// Scrub the free-text fields of a record payload before it lands on disk (KTD1: chain
/// records identify credentials by hash only and never contain secrets). Structured
/// fields (rungs, hashes, run ids, check names) pass through untouched; only operator/
/// detail free text is masked.
fn scrub_kind(kind: RecordKind) -> RecordKind {
    match kind {
        RecordKind::SessionDispatch(mut s) => {
            for c in &mut s.checks {
                c.detail = scrub(&c.detail);
            }
            for d in &mut s.deferrals {
                d.reason = scrub(&d.reason);
            }
            if let Some(ov) = &mut s.unknown_override {
                ov.reason = scrub(&ov.reason);
                ov.operator = scrub(&ov.operator);
                if let Some(note) = &ov.citation.note {
                    ov.citation.note = Some(scrub(note));
                }
            }
            RecordKind::SessionDispatch(s)
        }
        RecordKind::DeEscalation(mut d) => {
            d.events = d.events.iter().map(|e| scrub(e)).collect();
            RecordKind::DeEscalation(d)
        }
        RecordKind::ReRegistration(mut r) => {
            r.reason = scrub(&r.reason);
            RecordKind::ReRegistration(r)
        }
        RecordKind::SafetyTrip(mut t) => {
            t.detail = scrub(&t.detail);
            RecordKind::SafetyTrip(t)
        }
        other => other,
    }
}
