//! Resumable bulk-fetch driver + checkpoint (U4, KTD4/KTD8/KTD9).
//!
//! [`fetch_inputs`] drives the maintainer bulk acquisition: a per-weekday KRX witness loop
//! through the last closed session and a per-year KASI holiday loop, generating the weekend +
//! fixed-closure rule evidence locally, and writing the owner-local inputs artifact. It is the
//! highest-defect-risk surface, so it is built test-first against an INJECTED fetch closure —
//! the bin is the only place a real HTTP client is constructed.
//!
//! ## Resume + honesty (KTD4 / R4 / AE9)
//!
//! Progress is checkpointed (0o600 atomic write) after every completed unit. An interrupted or
//! quota-bounded run resumes from the checkpoint instead of restarting, and the per-source
//! covered ranges are derived from what was ACTUALLY completed (`*_covered_through`), never from
//! the requested window — so a partial acquisition is honestly partial in the artifact, never
//! silently presented as complete. A same-run transport error is terminal for that run (no
//! infinite retry — the liveness invariant); a fresh invocation retries the incomplete source.
//!
//! ## No secret ever persists (KTD9)
//!
//! Credentials ride only in the transient request URL passed to the injected `fetch`. Every
//! failure reason is credential-stripped ([`strip_url_credentials`] + [`MaintainerCredentials::scrub`])
//! before it enters a checkpoint or a [`SourceOutcome`]. The checkpoint + inputs artifact carry
//! only dates, normalized evidence, and scrubbed reasons.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use nautilus_ls_calendar::schema::{EvidenceRecord, Source, SourceKind};

use super::atomic_write_owner_only;
use super::normalize::{
    generated_rules, holiday_evidence, parse_kasi_holidays_xml, parse_krx_daily, witness_evidence,
    KASI_SOURCE_ID, KRX_DAILY_SOURCE_ID, KRX_RULE_SOURCE_ID,
};
use super::port::{DateRange, RefreshInputs, SourceOutcome};
use super::transport::{strip_url_credentials, MaintainerCredentials};

/// The bounds + pacing of a bulk fetch. `window` is the full materialization window (history
/// floor → operating horizon); `krx_through` is the last date KRX can witness (KRX is fetched
/// only through here — later dates have no session yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchConfig {
    /// The full materialization window.
    pub window: DateRange,
    /// The last date KRX daily-market is fetched through (last closed session).
    pub krx_through: NaiveDate,
    /// Minimum delay between network calls (pacing — a safe default in the bin).
    pub pace: Duration,
}

/// A typed reason the fetch driver could not start or persist.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// A required maintainer credential was absent from the environment.
    #[error("missing required credential: {0}")]
    MissingCredential(&'static str),
    /// A resume checkpoint's window/horizon does not match the requested run — a different
    /// window cannot resume an existing checkpoint (start a fresh state file instead).
    #[error("checkpoint window mismatch: checkpoint covers {existing:?}, run requested {requested:?}")]
    CheckpointMismatch {
        /// The `(window.from, window.through, krx_through)` the checkpoint was created for.
        existing: (NaiveDate, NaiveDate, NaiveDate),
        /// The `(window.from, window.through, krx_through)` this run requested.
        requested: (NaiveDate, NaiveDate, NaiveDate),
    },
    /// The checkpoint file could not be read/written.
    #[error("checkpoint I/O error: {0}")]
    Io(String),
    /// An existing checkpoint file was unreadable as a checkpoint.
    #[error("checkpoint is corrupt: {0}")]
    CorruptCheckpoint(String),
}

/// The resumable bulk-fetch checkpoint (KTD4). Persisted 0o600 after every completed unit;
/// carries only dates, normalized evidence, and credential-scrubbed failure reasons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchState {
    /// The full materialization window this checkpoint is for.
    pub window: DateRange,
    /// The KRX witness horizon this checkpoint is for.
    pub krx_through: NaiveDate,
    /// The next KRX date to fetch (advances past `krx_through` when KRX is complete).
    pub krx_cursor: NaiveDate,
    /// The last date KRX coverage was actually completed through (the covered-range end).
    pub krx_covered_through: Option<NaiveDate>,
    /// The accumulated KRX positive witnesses (deterministic, midnight-UTC stamped).
    pub krx_witnesses: Vec<EvidenceRecord>,
    /// The credential-scrubbed reason the KRX source failed in the last run, if any.
    pub krx_failed: Option<String>,
    /// The next KASI year to fetch.
    pub kasi_next_year: i32,
    /// The last date KASI coverage was completed through (end of the last completed year, clamped).
    pub kasi_covered_through: Option<NaiveDate>,
    /// The accumulated KASI holiday dates.
    pub kasi_holidays: Vec<NaiveDate>,
    /// The credential-scrubbed reason the KASI source failed in the last run, if any.
    pub kasi_failed: Option<String>,
}

impl FetchState {
    fn fresh(cfg: &FetchConfig) -> Self {
        FetchState {
            window: cfg.window,
            krx_through: cfg.krx_through,
            krx_cursor: cfg.window.from,
            krx_covered_through: None,
            krx_witnesses: Vec::new(),
            krx_failed: None,
            kasi_next_year: cfg.window.from.year(),
            kasi_covered_through: None,
            kasi_holidays: Vec::new(),
            kasi_failed: None,
        }
    }

    fn krx_complete(&self) -> bool {
        self.krx_cursor > self.krx_through
    }

    fn kasi_complete(&self) -> bool {
        self.kasi_next_year > self.window.through.year()
    }
}

/// The maximum KASI pages fetched per year — a hard bound so a broken pagination signal can
/// never loop forever (a year has ~15 holidays; one 100-row page always suffices).
const KASI_MAX_PAGES: u32 = 20;

/// Drive the bulk fetch to completion (or a terminal per-source failure), resuming from any
/// checkpoint at `state_path`, and return the normalized [`RefreshInputs`]. `fetch` performs one
/// HTTP GET (injected — offline in tests); `sleep` applies pacing (injected — a no-op in tests).
pub fn fetch_inputs<Fetch, Sleep>(
    cfg: &FetchConfig,
    creds: &MaintainerCredentials,
    state_path: &Path,
    fetch: Fetch,
    mut sleep: Sleep,
) -> Result<RefreshInputs, FetchError>
where
    Fetch: Fn(&str) -> Result<String, String>,
    Sleep: FnMut(Duration),
{
    // Credentials are required but are NOT embedded in any URL — the composition-root `fetch`
    // closure applies them (KRX Open API authenticates via the `AUTH_KEY` header; KASI via the
    // `serviceKey` query param). fetch_state only builds credential-free URLs (KTD9).
    if creds.krx_appkey.is_none() {
        return Err(FetchError::MissingCredential(super::transport::KRX_APPKEY_ENV));
    }
    if creds.kasi_service_key.is_none() {
        return Err(FetchError::MissingCredential(super::transport::KASI_SERVICE_KEY_ENV));
    }

    let mut state = load_or_init(cfg, state_path)?;

    // A fresh invocation retries a source that is incomplete but was marked failed last run —
    // the liveness bound ("no infinite retry") holds WITHIN a run, not across operator re-runs.
    if !state.krx_complete() {
        state.krx_failed = None;
    }
    if !state.kasi_complete() {
        state.kasi_failed = None;
    }

    // ---- KRX per-weekday loop through the witness horizon ----
    while !state.krx_complete() && state.krx_failed.is_none() {
        let date = state.krx_cursor;
        if is_weekday(date) {
            let url = krx_url(date);
            sleep(cfg.pace);
            match fetch(&url) {
                Ok(body) => match parse_krx_daily(&body, date) {
                    Ok(resp) => {
                        if let Some(w) = witness_evidence(&resp) {
                            state.krx_witnesses.push(w);
                        }
                    }
                    Err(message) => {
                        state.krx_failed = Some(scrub_reason(&url, &message, creds));
                    }
                },
                Err(message) => {
                    state.krx_failed = Some(scrub_reason(&url, &message, creds));
                }
            }
            if state.krx_failed.is_some() {
                // Persist the failure + progress-so-far and stop this run (liveness: no retry).
                checkpoint(&state, state_path)?;
                break;
            }
        }
        // The date is completed either way (a weekday witnessed/empty, or a rule-covered weekend).
        state.krx_covered_through = Some(date);
        state.krx_cursor = date.succ_opt().unwrap_or(date);
        if state.krx_cursor == date {
            break; // civil-date overflow guard
        }
        checkpoint(&state, state_path)?;
    }

    // ---- KASI per-year loop ----
    while !state.kasi_complete() && state.kasi_failed.is_none() {
        let year = state.kasi_next_year;
        match fetch_kasi_year(year, cfg, creds, &fetch, &mut sleep) {
            Ok(mut holidays) => {
                state.kasi_holidays.append(&mut holidays);
                state.kasi_covered_through =
                    Some(year_end_clamped(year, cfg.window.through));
                state.kasi_next_year = year + 1;
                checkpoint(&state, state_path)?;
            }
            Err(message) => {
                state.kasi_failed = Some(message);
                checkpoint(&state, state_path)?;
                break;
            }
        }
    }

    Ok(assemble_inputs(&state, cfg))
}

/// Fetch one KASI year, paginating up to [`KASI_MAX_PAGES`] with a progress guard (a page that
/// adds nothing while the total is unreached still terminates — never an infinite loop).
fn fetch_kasi_year<Fetch, Sleep>(
    year: i32,
    cfg: &FetchConfig,
    creds: &MaintainerCredentials,
    fetch: &Fetch,
    sleep: &mut Sleep,
) -> Result<Vec<NaiveDate>, String>
where
    Fetch: Fn(&str) -> Result<String, String>,
    Sleep: FnMut(Duration),
{
    let mut holidays: Vec<NaiveDate> = Vec::new();
    let mut page: u32 = 1;
    loop {
        let url = kasi_url(year, page);
        sleep(cfg.pace);
        let body = fetch(&url).map_err(|m| scrub_reason(&url, &m, creds))?;
        let parsed = parse_kasi_holidays_xml(&body).map_err(|m| creds.scrub(&m))?;
        let before = holidays.len();
        holidays.extend(parsed.holidays.iter().copied());
        let reached_total = parsed.total_count as usize <= holidays.len();
        let made_progress = holidays.len() > before;
        if reached_total || !made_progress || page >= KASI_MAX_PAGES {
            break;
        }
        page += 1;
    }
    holidays.sort();
    holidays.dedup();
    Ok(holidays)
}

/// Assemble the normalized [`RefreshInputs`] from the checkpoint. Covered ranges come from what
/// was ACTUALLY completed (`*_covered_through`); a failed source is a `failed_covering` outcome
/// carrying its partial coverage. The generated rule evidence (weekends + fixed closures) always
/// spans the whole window — it is produced deterministically, not fetched.
fn assemble_inputs(state: &FetchState, cfg: &FetchConfig) -> RefreshInputs {
    let mut evidence: Vec<EvidenceRecord> = Vec::new();
    evidence.extend(state.krx_witnesses.iter().cloned());
    for &date in &state.kasi_holidays {
        let (fact, rule) = holiday_evidence(date);
        evidence.push(fact);
        evidence.push(rule);
    }
    evidence.extend(generated_rules(cfg.window));
    // Deterministic ordering (dedup by id: a KASI holiday's paired rule and a generated weekend/
    // fixed rule on the same date share `rule-<date>`).
    evidence.sort_by(|a, b| a.id.cmp(&b.id));
    evidence.dedup_by(|a, b| a.id == b.id);

    let sources = vec![
        source(KRX_DAILY_SOURCE_ID, SourceKind::KrxDailyMarket),
        source(KASI_SOURCE_ID, SourceKind::KasiHoliday),
        source(KRX_RULE_SOURCE_ID, SourceKind::KrxRule),
    ];

    let krx_outcome = source_outcome(
        KRX_DAILY_SOURCE_ID,
        SourceKind::KrxDailyMarket,
        cfg.window.from,
        state.krx_covered_through,
        &state.krx_failed,
    );
    let kasi_outcome = source_outcome(
        KASI_SOURCE_ID,
        SourceKind::KasiHoliday,
        cfg.window.from,
        state.kasi_covered_through,
        &state.kasi_failed,
    );
    // Generated rules are deterministic and always span the whole window.
    let rule_outcome =
        SourceOutcome::ok_covering(KRX_RULE_SOURCE_ID, SourceKind::KrxRule, vec![cfg.window]);

    RefreshInputs {
        sources,
        evidence,
        outcomes: vec![krx_outcome, kasi_outcome, rule_outcome],
    }
}

fn source_outcome(
    id: &str,
    kind: SourceKind,
    from: NaiveDate,
    covered_through: Option<NaiveDate>,
    failed: &Option<String>,
) -> SourceOutcome {
    let ranges = match covered_through {
        Some(through) if through >= from => vec![DateRange::new(from, through)],
        _ => vec![], // nothing completed → present-but-empty (fetched nothing)
    };
    match failed {
        Some(reason) => SourceOutcome::failed_covering(id, kind, reason.clone(), ranges),
        None => SourceOutcome::ok_covering(id, kind, ranges),
    }
}

fn source(id: &str, kind: SourceKind) -> Source {
    Source {
        id: id.to_string(),
        kind,
        label: id.to_string(),
        synthetic: false,
    }
}

fn load_or_init(cfg: &FetchConfig, state_path: &Path) -> Result<FetchState, FetchError> {
    match std::fs::read(state_path) {
        Ok(bytes) => {
            let state: FetchState = serde_json::from_slice(&bytes)
                .map_err(|e| FetchError::CorruptCheckpoint(e.to_string()))?;
            if state.window != cfg.window || state.krx_through != cfg.krx_through {
                return Err(FetchError::CheckpointMismatch {
                    existing: (state.window.from, state.window.through, state.krx_through),
                    requested: (cfg.window.from, cfg.window.through, cfg.krx_through),
                });
            }
            Ok(state)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FetchState::fresh(cfg)),
        Err(e) => Err(FetchError::Io(e.to_string())),
    }
}

fn checkpoint(state: &FetchState, state_path: &Path) -> Result<(), FetchError> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|e| FetchError::Io(e.to_string()))?;
    atomic_write_owner_only(state_path, &bytes).map_err(|e| FetchError::Io(e.to_string()))
}

/// Credential-scrub a failure `message` that may echo the request `url`: replace the raw URL
/// with its credential-stripped form, then mask any standalone secret value (KTD9).
fn scrub_reason(url: &str, message: &str, creds: &MaintainerCredentials) -> String {
    let stripped = message.replace(url, &strip_url_credentials(url));
    creds.scrub(&stripped)
}

/// The KRX daily-market URL for one date — credential-free. The KRX Open API is served from
/// `data-dbg.krx.co.kr` (confirmed at the U8 probe gate; `openapi.krx.co.kr` is only the portal)
/// and authenticates via the `AUTH_KEY` HTTP header the composition-root closure adds, NOT a
/// query param.
fn krx_url(date: NaiveDate) -> String {
    format!(
        "https://data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd?basDd={}",
        date.format("%Y%m%d")
    )
}

/// The KASI holiday URL for one year/page — credential-free. The composition-root closure adds
/// the `serviceKey` query param (URL-encoded by the HTTP client).
fn kasi_url(year: i32, page: u32) -> String {
    format!(
        "https://apis.data.go.kr/B090041/openapi/service/SpcdeInfoService/getRestDeInfo?solYear={year}&numOfRows=100&pageNo={page}"
    )
}

fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

fn year_end_clamped(year: i32, window_through: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, 12, 31)
        .unwrap_or(window_through)
        .min(window_through)
}

// ---------------------------------------------------------------------------------------
// Path confinement (KTD9 / R3 / R14) — tool-enforced publication boundary
// ---------------------------------------------------------------------------------------

/// Resolve `target` beneath the owner-local `state_root`, refusing any path (including one that
/// escapes via `..` or a symlink) that lands outside it. The parent directory is canonicalized
/// so a symlinked parent pointing outside the root is caught BEFORE any fetch or write; the file
/// itself need not exist yet. The publication boundary is thus tool-enforced, not discipline.
pub fn confine(state_root: &Path, target: &Path) -> Result<PathBuf, String> {
    let root = state_root
        .canonicalize()
        .map_err(|e| format!("state root {} is unusable: {e}", state_root.display()))?;
    let parent = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => root.clone(),
    };
    let parent = parent
        .canonicalize()
        .map_err(|e| format!("path {} has no resolvable parent under the state root: {e}", target.display()))?;
    if !parent.starts_with(&root) {
        return Err(format!(
            "refused: {} resolves outside the owner-local state root {}",
            target.display(),
            root.display()
        ));
    }
    let name = target
        .file_name()
        .ok_or_else(|| format!("refused: {} has no file name", target.display()))?;
    Ok(parent.join(name))
}
