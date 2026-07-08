//! IGW00201 budget model + per-credential spend ledger (U2, KTD-2/KTD-3).
//!
//! Two pieces the budget-aware ingest (U3) and the attended budget probe (U4)
//! share:
//!
//! * [`BudgetModel`] — the committed, machine-readable IGW00201 budget model
//!   (`lab/config/gateway-budget.json`): refill/backoff seconds, the budget-window
//!   seconds, the cold-budget call count, the measured bucket scope, and a
//!   provenance string. Loading **fails open** — an absent or corrupt file yields
//!   the provisional default that reproduces today's constants (120s backoff, no
//!   plan-ahead), so nothing regresses before U6 promotes measured numbers.
//! * [`SpendLedger`] — a persistent, per-credential (hashed appkey) record of
//!   every gateway dispatch, so the ingest can plan a run's call budget against the
//!   measured model instead of retrying blind. **Advisory**: the gateway stays
//!   ground truth, so loading tolerates absent/corrupt files by warning and
//!   starting fresh, and an unpredicted `IGW00201` is recorded as a model-miss,
//!   never trusted over the gateway.
//!
//! The ledger mirrors [`super::checkpoint::Checkpoint`]: every field
//! `#[serde(default)]` (legacy files load), atomic temp+rename save. The SHA-256
//! helper mirrors the lab crate's `manifest::hash_bytes` shape (not importable
//! here — the lab crate depends on this adapter, a cycle).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Default IGW00201 backoff/refill seconds — the guessed 120s value the drip
/// runbook used before the probe (the former hard-coded `IGW00201_BACKOFF`). A
/// [`BudgetModel`] loaded from an absent/corrupt config reproduces exactly this.
pub const DEFAULT_REFILL_SECS: u64 = 120;

/// Default budget-window seconds for spend accounting — the "day-ish" guess
/// (86,400s). Only consulted for plan-ahead, which is off by default
/// (`budget_calls: None`), so this is inert until a measured model is promoted.
pub const DEFAULT_WINDOW_SECS: i64 = 86_400;

/// The default committed budget-model path, relative to the adapter crate root
/// (the CWD the ingest binaries run from). `LS_GATEWAY_BUDGET_FILE` overrides.
pub const DEFAULT_BUDGET_FILE: &str = "lab/config/gateway-budget.json";

/// Env override for the budget-model config path.
pub const BUDGET_FILE_ENV: &str = "LS_GATEWAY_BUDGET_FILE";

/// Env override for the spend-ledger path (KTD-3). The turn scripts pin this to a
/// stable location so the ledger survives across the fresh data homes turn runs
/// create.
pub const SPEND_LEDGER_ENV: &str = "LS_SPEND_LEDGER_FILE";

/// The measured scope of the IGW00201 budget bucket (probe stage 0, R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BucketScope {
    /// Not yet measured — the provisional default.
    #[default]
    Unknown,
    /// The bucket is per-credential: a spare key serves while the domestic key is
    /// exhausted (AE1). Sharding across keys would multiply throughput (scope-
    /// gated, deferred).
    PerCredential,
    /// The bucket is broader than a single credential: the spare key trips too
    /// while the domestic key is exhausted (AE2). Later stages must schedule onto
    /// cold windows of the shared budget.
    BroaderThanCredential,
}

/// The committed IGW00201 budget model (`lab/config/gateway-budget.json`).
///
/// Every field `#[serde(default)]` with a default that reproduces today's
/// constants, so a partial or absent config never breaks the load and the
/// provisional model is exactly today's behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetModel {
    /// Backoff/refill seconds after an IGW00201 (feeds `throttle_backoff`, R10).
    #[serde(default = "default_refill_secs")]
    pub refill_secs: u64,
    /// Budget-window seconds for spend accounting (the rolling window the budget
    /// refills over). Only consulted when `budget_calls` is `Some`.
    #[serde(default = "default_window_secs")]
    pub window_secs: i64,
    /// The cold-budget size: calls served from a cold bucket before the first
    /// IGW00201 (probe stage 1). `None` = unmeasured → **no plan-ahead** (fail
    /// open to today's blind-backoff behavior).
    #[serde(default)]
    pub budget_calls: Option<u32>,
    /// The measured bucket scope (probe stage 0).
    #[serde(default)]
    pub bucket_scope: BucketScope,
    /// Provenance: probe date + per-axis confidence, or "provisional" before U6.
    #[serde(default)]
    pub provenance: String,
}

fn default_refill_secs() -> u64 {
    DEFAULT_REFILL_SECS
}
fn default_window_secs() -> i64 {
    DEFAULT_WINDOW_SECS
}

impl Default for BudgetModel {
    /// The provisional model: today's guessed constants, no plan-ahead.
    fn default() -> Self {
        BudgetModel {
            refill_secs: DEFAULT_REFILL_SECS,
            window_secs: DEFAULT_WINDOW_SECS,
            budget_calls: None,
            bucket_scope: BucketScope::Unknown,
            provenance: "provisional (guessed, pre-probe): 120s backoff, day-ish window, \
                         credential-shared unverified — superseded by U6"
                .to_string(),
        }
    }
}

impl BudgetModel {
    /// The IGW00201 backoff derived from the measured refill window (R10/KTD-6).
    pub fn throttle_backoff(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.refill_secs)
    }

    /// Load the model from `path`, **failing open**: an absent file returns the
    /// provisional default (a debug log, not a warning — absence is the norm before
    /// U6); a corrupt file warns and returns the default (never blocks ingest).
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => match serde_json::from_str::<BudgetModel>(&s) {
                Ok(model) => model,
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "corrupt gateway-budget.json; using the provisional default (120s backoff, no plan-ahead)"
                    );
                    BudgetModel::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!(
                    path = %path.display(),
                    "no gateway-budget.json; using the provisional default (120s backoff, no plan-ahead)"
                );
                BudgetModel::default()
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "cannot read gateway-budget.json; using the provisional default"
                );
                BudgetModel::default()
            }
        }
    }

    /// Load from the env-overridable default path ([`BUDGET_FILE_ENV`] else
    /// [`DEFAULT_BUDGET_FILE`]). Fail-open, as [`Self::load`].
    pub fn load_default() -> Self {
        let path = std::env::var(BUDGET_FILE_ENV).unwrap_or_else(|_| DEFAULT_BUDGET_FILE.to_string());
        Self::load(Path::new(&path))
    }

    /// Serialize to pretty JSON (used by the probe promotion path / tests).
    pub fn to_pretty_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("BudgetModel serializes")
    }
}

/// Per-credential spend: timestamp-bucketed call counts plus a model-miss counter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct CredentialSpend {
    /// `unix-second → calls dispatched that second`. Pruned beyond the model
    /// window on load; `BTreeMap` for deterministic serialization.
    #[serde(default)]
    buckets: BTreeMap<i64, u32>,
    /// IGW00201s the model did not predict (external spend, wrong model) — a
    /// signal the model is off, never trusted over the gateway (KTD-3).
    #[serde(default)]
    model_misses: u64,
}

/// The persistent, per-credential spend ledger (KTD-3). Keyed by SHA-256 of the
/// resolved appkey — never the raw key, the lane filename, or the account number.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpendLedger {
    /// `appkey-hash → per-credential spend`.
    #[serde(default)]
    credentials: BTreeMap<String, CredentialSpend>,
}

impl SpendLedger {
    /// The per-credential ledger key: hex SHA-256 of the resolved appkey (KTD-3).
    /// Mirrors the lab crate's `manifest::hash_bytes` shape (not importable — the
    /// lab crate depends on this adapter). Never persists the raw key.
    pub fn hash_appkey(appkey: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(appkey.as_bytes());
        let digest = hasher.finalize();
        let mut s = String::with_capacity(digest.len() * 2);
        for b in digest {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// Record one dispatch for `cred_hash` at `at_unix` (KTD-3, the pacer-acquire
    /// seam). Increments only that credential's current-second bucket — never
    /// another's.
    pub fn record_spend(&mut self, cred_hash: &str, at_unix: i64) {
        *self
            .credentials
            .entry(cred_hash.to_string())
            .or_default()
            .buckets
            .entry(at_unix)
            .or_insert(0) += 1;
    }

    /// Record a model-miss for `cred_hash`: an IGW00201 the ledger did not predict
    /// (KTD-3). Advisory — a signal the model is off, never authority over the
    /// gateway's own recovery arms.
    pub fn record_model_miss(&mut self, cred_hash: &str) {
        self.credentials.entry(cred_hash.to_string()).or_default().model_misses += 1;
    }

    /// Calls dispatched for `cred_hash` within the last `window_secs` ending at
    /// `now_unix` (the rolling budget window the plan-ahead compares against).
    pub fn spent_within(&self, cred_hash: &str, window_secs: i64, now_unix: i64) -> u64 {
        let cutoff = now_unix - window_secs;
        self.credentials
            .get(cred_hash)
            .map(|c| {
                c.buckets
                    .range((cutoff + 1)..=now_unix)
                    .map(|(_, &n)| n as u64)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// The recorded model-miss count for `cred_hash`.
    pub fn model_misses(&self, cred_hash: &str) -> u64 {
        self.credentials.get(cred_hash).map(|c| c.model_misses).unwrap_or(0)
    }

    /// Drop spend buckets strictly older than `cutoff_unix` across all credentials
    /// (rolled off the budget window). Model-miss counters and any credential with
    /// surviving buckets or misses are retained; a credential with neither is
    /// dropped so the ledger does not grow unbounded across idle keys.
    pub fn prune_before(&mut self, cutoff_unix: i64) {
        for cred in self.credentials.values_mut() {
            cred.buckets.retain(|&second, _| second >= cutoff_unix);
        }
        self.credentials
            .retain(|_, c| !c.buckets.is_empty() || c.model_misses > 0);
    }

    /// Load a ledger from `path`, **tolerantly**: an absent file yields an empty
    /// ledger (no error); a corrupt file warns and yields an empty ledger (advisory
    /// data must never block ingest).
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => match serde_json::from_str::<SpendLedger>(&s) {
                Ok(ledger) => ledger,
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "corrupt spend ledger; starting fresh (advisory data, ingest continues)"
                    );
                    SpendLedger::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => SpendLedger::default(),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "cannot read spend ledger; starting fresh"
                );
                SpendLedger::default()
            }
        }
    }

    /// Load then prune buckets older than `cutoff_unix` (rows beyond the window are
    /// pruned on load, KTD-3 test scenario 4).
    pub fn load_pruned(path: &Path, cutoff_unix: i64) -> Self {
        let mut ledger = Self::load(path);
        ledger.prune_before(cutoff_unix);
        ledger
    }

    /// Persist the ledger to `path` atomically (temp file + rename, mirroring
    /// [`super::checkpoint::Checkpoint::save`]), creating parent dirs.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] on a write/rename failure.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }
}

/// The default spend-ledger path (KTD-3): [`SPEND_LEDGER_ENV`] override, else
/// `<catalog>.parent()/state/spend-ledger.json` — derived exactly as
/// [`super::probes_dir_for`] derives the sibling `probes/` dir.
pub fn spend_ledger_path(catalog_path: &Path) -> PathBuf {
    if let Ok(over) = std::env::var(SPEND_LEDGER_ENV) {
        if !over.trim().is_empty() {
            return PathBuf::from(over);
        }
    }
    catalog_path
        .parent()
        .map(|p| p.join("state"))
        .unwrap_or_else(|| catalog_path.join("state"))
        .join("spend-ledger.json")
}

/// The pre-dispatch budget decision for one symbol/triple (AE3/KTD-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDecision {
    /// Enough remaining budget (or no measured budget) — dispatch.
    Proceed,
    /// The estimated page cost exceeds the remaining budget — stop before
    /// dispatching, persist progress, and schedule the remainder.
    Defer {
        /// The estimated page cost of this triple.
        estimated: u32,
        /// The budget remaining in the current window.
        remaining: u32,
    },
}

/// Decide whether to dispatch a triple estimated at `estimated_pages`, given the
/// measured `model` and the credential's recent spend (AE3/KTD-3). Returns
/// [`BudgetDecision::Proceed`] unconditionally when the model carries no measured
/// budget (`budget_calls: None`) — plan-ahead is inert until U6, so nothing
/// regresses.
pub fn plan_dispatch(
    model: &BudgetModel,
    ledger: &SpendLedger,
    cred_hash: &str,
    now_unix: i64,
    estimated_pages: u32,
) -> BudgetDecision {
    match model.budget_calls {
        None => BudgetDecision::Proceed,
        Some(budget) => {
            let spent = ledger.spent_within(cred_hash, model.window_secs, now_unix);
            let remaining = budget.saturating_sub(spent.min(u32::MAX as u64) as u32);
            // Defer ONLY when the triple would fit a fresh/cold budget window
            // (`estimated <= budget`) but not the remaining one. If it exceeds the
            // whole budget, no cold window can ever fit it — deferring would stall
            // that symbol forever — so proceed and let the in-process IGW00201
            // recovery arms narrow-and-retry it (the drip resumes idempotently).
            if estimated_pages > remaining && estimated_pages <= budget {
                BudgetDecision::Defer { estimated: estimated_pages, remaining }
            } else {
                BudgetDecision::Proceed
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Budget probe (U4) — the attended, staged IGW00201 measurement. The classifier,
// cold-budget driver, hard ceiling, and report are here (testable offline); the
// `budget-probe` binary wires them to live SDK calls and writes the JSON report.
// ---------------------------------------------------------------------------

/// The classification of a single probe call (U4), pure over the SDK result —
/// mirroring `collect_minute`'s IGW00201 match exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallVerdict {
    /// The gateway served the call.
    Served,
    /// The gateway returned `IGW00201` (budget exhausted).
    Throttled,
    /// The gateway returned some other API error envelope (carries the `rsp_cd` —
    /// a gateway code, never credential material).
    OtherApi(String),
    /// A transport/decode/auth error (no payload retained, to avoid leaking a raw
    /// broker message into the on-disk report).
    Transport,
}

/// Classify one probe SDK result (U4). Matches `IGW00201` exactly as the ingest
/// recovery arms do; any other `ApiError` carries its (safe) `rsp_cd`; everything
/// else is a coarse `Transport` with no retained detail.
pub fn classify_call<T>(result: &ls_core::LsResult<T>) -> CallVerdict {
    match result {
        Ok(_) => CallVerdict::Served,
        Err(ls_core::LsError::ApiError { code, .. }) if code == "IGW00201" => CallVerdict::Throttled,
        Err(ls_core::LsError::ApiError { code, .. }) => CallVerdict::OtherApi(code.clone()),
        Err(_) => CallVerdict::Transport,
    }
}

/// Map a stage-0 scope-probe verdict to a bucket scope (R2/AE1/AE2): a served
/// spare-key call while the domestic key is exhausted means per-credential; a
/// throttle means broader-than-credential; anything else is inconclusive.
pub fn scope_from_stage0(verdict: &CallVerdict) -> BucketScope {
    match verdict {
        CallVerdict::Served => BucketScope::PerCredential,
        CallVerdict::Throttled => BucketScope::BroaderThanCredential,
        _ => BucketScope::Unknown,
    }
}

/// A hard per-session call ceiling (R5) — blocking-risk protection enforced in
/// code, not operator discipline: a stage stops rather than issuing a call past
/// the ceiling, even if that leaves an axis unmeasured.
#[derive(Debug)]
pub struct CallCeiling {
    made: usize,
    ceiling: usize,
}

impl CallCeiling {
    /// A fresh ceiling admitting `ceiling` total session calls.
    pub fn new(ceiling: usize) -> Self {
        CallCeiling { made: 0, ceiling }
    }
    /// Calls issued so far this session.
    pub fn made(&self) -> usize {
        self.made
    }
    /// Reserve one call slot, returning `false` (refuse to call) once the ceiling
    /// is reached. Reserving counts the call whether it serves or throttles.
    pub fn try_reserve(&mut self) -> bool {
        if self.made >= self.ceiling {
            return false;
        }
        self.made += 1;
        true
    }
}

/// Why a probe stage stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStop {
    /// Hit an `IGW00201` — the measured signal (stage 1).
    Throttled,
    /// Hit the hard per-session ceiling before a throttle (R5).
    Ceiling,
    /// A non-throttle error aborted the stage (carries the `rsp_cd` or `transport`).
    Error(String),
}

/// The cold-budget measurement (stage 1, R3): calls served before the first throttle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColdBudget {
    /// Calls the cold bucket served before the first `IGW00201`.
    pub calls_served: u32,
    /// Why the counting loop stopped.
    pub stopped: StageStop,
}

/// The seam the probe stages call through (U4). The live impl (`budget-probe` /
/// [`SdkProbeCaller`]) issues one SDK read and records spend; test fakes return
/// canned verdicts so the stage logic is offline-testable.
#[async_trait::async_trait]
pub trait ProbeCaller {
    /// Issue one MarketData-class probe read. `Ok(())` = served.
    async fn market_data_call(&self) -> ls_core::LsResult<()>;
    /// Issue one different-bucket (non-MarketData) probe read (stage 3 cross-class).
    async fn other_class_call(&self) -> ls_core::LsResult<()>;
}

/// Stage 1 (R3): call a MarketData read (gently paced) until the first `IGW00201`
/// or the hard ceiling, counting successes. The ceiling is checked BEFORE every
/// call, so the loop never issues a call past it (R5). `pace` is the inter-call
/// sleep (zero in tests).
pub async fn measure_cold_budget<C: ProbeCaller + ?Sized>(
    caller: &C,
    ceiling: &mut CallCeiling,
    pace: std::time::Duration,
) -> ColdBudget {
    let mut served = 0u32;
    loop {
        if !ceiling.try_reserve() {
            return ColdBudget { calls_served: served, stopped: StageStop::Ceiling };
        }
        match classify_call(&caller.market_data_call().await) {
            CallVerdict::Served => {
                served += 1;
                if !pace.is_zero() {
                    tokio::time::sleep(pace).await;
                }
            }
            CallVerdict::Throttled => {
                return ColdBudget { calls_served: served, stopped: StageStop::Throttled }
            }
            CallVerdict::OtherApi(code) => {
                return ColdBudget { calls_served: served, stopped: StageStop::Error(code) }
            }
            CallVerdict::Transport => {
                return ColdBudget { calls_served: served, stopped: StageStop::Error("transport".to_string()) }
            }
        }
    }
}

/// Stage 0 scope report (R2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage0Report {
    /// The single spare-key call's verdict.
    pub verdict: CallVerdict,
    /// The scope it implies.
    pub scope: BucketScope,
}

/// Stage 2 refill report (R3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefillReport {
    /// Seconds waited before a single call served again — `None` if it never
    /// served within the sampled intervals (record and defer, don't re-burn).
    pub first_success_secs: Option<i64>,
    /// The widening sample intervals actually tried (seconds).
    pub sample_intervals_secs: Vec<i64>,
}

/// Stage 3 cross-TR-class report (R3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossClassReport {
    /// The post-exhaustion different-class call's verdict.
    pub verdict: CallVerdict,
    /// Whether exhaustion spans classes (the other-class call also throttled).
    pub spans_classes: bool,
}

/// The persisted probe report (U4, R4) — machine-readable numbers the operator
/// promotes into `gateway-budget.json`, provenance-stamped by the binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    /// RFC-3339 probe timestamp.
    pub probed_at: String,
    /// The hard per-session call ceiling in force (R5).
    pub ceiling: usize,
    /// Total gateway calls the session made across all stages.
    pub total_calls: usize,
    /// Stage 0 scope (R2), if run.
    #[serde(default)]
    pub stage0_scope: Option<Stage0Report>,
    /// Stage 1 cold-budget (R3), if run.
    #[serde(default)]
    pub stage1_cold_budget: Option<ColdBudget>,
    /// Stage 2 refill (R3), if run.
    #[serde(default)]
    pub stage2_refill: Option<RefillReport>,
    /// Stage 3 cross-class (R3), if run.
    #[serde(default)]
    pub stage3_cross_class: Option<CrossClassReport>,
    /// Free-form operator notes (warmth caveats, AE2 branch, deferred axes).
    #[serde(default)]
    pub notes: Vec<String>,
}

impl ProbeReport {
    /// Pretty-print for the on-disk report.
    pub fn to_pretty_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ProbeReport serializes")
    }
}

/// The live probe caller (U4): issues one `t1102` (MarketData) or `CSPAQ12200`
/// (Account — a different rate bucket) read per call and records the dispatch in
/// the shared spend ledger. The payload is discarded (`Ok(())`) — the probe only
/// needs the serve/throttle verdict. Held in the binary; here so its wiremock test
/// lives beside the stage logic.
pub struct SdkProbeCaller {
    sdk: ls_sdk::LsSdk,
    ledger: Arc<Mutex<SpendLedger>>,
    cred_hash: String,
    shcode: String,
    exchgubun: String,
    balcretp: String,
}

impl SdkProbeCaller {
    /// Build a caller for `shcode` on the `K` (KRX) exchange, recording spend into
    /// `ledger` under `cred_hash`.
    pub fn new(
        sdk: ls_sdk::LsSdk,
        ledger: Arc<Mutex<SpendLedger>>,
        cred_hash: String,
        shcode: String,
    ) -> Self {
        SdkProbeCaller {
            sdk,
            ledger,
            cred_hash,
            shcode,
            exchgubun: "K".to_string(),
            balcretp: "0".to_string(),
        }
    }

    fn record(&self) {
        if let Ok(mut l) = self.ledger.lock() {
            l.record_spend(&self.cred_hash, chrono::Utc::now().timestamp());
        }
    }
}

#[async_trait::async_trait]
impl ProbeCaller for SdkProbeCaller {
    async fn market_data_call(&self) -> ls_core::LsResult<()> {
        self.record();
        let req = ls_sdk::market_session::T1102Request::new(self.shcode.clone(), self.exchgubun.clone());
        self.sdk.market_session().quote(&req).await.map(|_| ())
    }

    async fn other_class_call(&self) -> ls_core::LsResult<()> {
        self.record();
        let req = ls_sdk::account::CSPAQ12200Request::new(self.balcretp.clone());
        self.sdk.account().balance(&req).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // --- BudgetModel ---

    #[test]
    fn absent_config_yields_provisional_default() {
        let dir = tempdir().unwrap();
        let model = BudgetModel::load(&dir.path().join("nope.json"));
        // Scenario 6: absent config → default reproducing today's 120s backoff, no
        // plan-ahead.
        assert_eq!(model, BudgetModel::default());
        assert_eq!(model.refill_secs, DEFAULT_REFILL_SECS);
        assert_eq!(model.throttle_backoff(), std::time::Duration::from_secs(120));
        assert!(model.budget_calls.is_none(), "no plan-ahead by default");
        assert_eq!(model.bucket_scope, BucketScope::Unknown);
    }

    #[test]
    fn corrupt_config_falls_back_to_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("gateway-budget.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        assert_eq!(BudgetModel::load(&path), BudgetModel::default());
    }

    #[test]
    fn measured_config_round_trips_and_drives_backoff() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("gateway-budget.json");
        let model = BudgetModel {
            refill_secs: 300,
            window_secs: 3600,
            budget_calls: Some(200),
            bucket_scope: BucketScope::PerCredential,
            provenance: "measured 2026-07-15".to_string(),
        };
        std::fs::write(&path, model.to_pretty_json()).unwrap();
        let loaded = BudgetModel::load(&path);
        assert_eq!(loaded, model);
        // Scenario 5 (U3): throttle_backoff reflects the config value.
        assert_eq!(loaded.throttle_backoff(), std::time::Duration::from_secs(300));
    }

    #[test]
    fn partial_config_fills_defaults() {
        // A config carrying only a measured budget still loads (other fields
        // default), so an operator can promote one axis at a time.
        let dir = tempdir().unwrap();
        let path = dir.path().join("gateway-budget.json");
        std::fs::write(&path, r#"{"budget_calls": 500}"#).unwrap();
        let m = BudgetModel::load(&path);
        assert_eq!(m.budget_calls, Some(500));
        assert_eq!(m.refill_secs, DEFAULT_REFILL_SECS, "absent refill defaults");
        assert_eq!(m.window_secs, DEFAULT_WINDOW_SECS);
    }

    // --- SpendLedger ---

    #[test]
    fn hash_is_stable_hex_and_never_the_raw_key() {
        let h = SpendLedger::hash_appkey("super-secret-appkey");
        assert_eq!(h.len(), 64, "hex SHA-256");
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!h.contains("secret"), "the raw key never appears in the hash");
        assert_eq!(h, SpendLedger::hash_appkey("super-secret-appkey"), "stable");
        assert_ne!(h, SpendLedger::hash_appkey("other-key"));
    }

    #[test]
    fn round_trip_preserves_rows() {
        // Scenario 1: load/save round-trip preserves rows.
        let dir = tempdir().unwrap();
        let path = dir.path().join("state/spend-ledger.json");
        let mut led = SpendLedger::default();
        let a = SpendLedger::hash_appkey("key-a");
        led.record_spend(&a, 1000);
        led.record_spend(&a, 1000);
        led.record_spend(&a, 1005);
        led.record_model_miss(&a);
        led.save(&path).unwrap();

        // Deterministic serialization (byte-identical double-save).
        let first = std::fs::read_to_string(&path).unwrap();
        led.save(&path).unwrap();
        let second = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first, second, "serialization is deterministic");

        let loaded = SpendLedger::load(&path);
        assert_eq!(loaded.spent_within(&a, 100, 1005), 3, "all three calls in window");
        assert_eq!(loaded.model_misses(&a), 1);
    }

    #[test]
    fn absent_file_loads_empty_no_error() {
        // Scenario 2.
        let dir = tempdir().unwrap();
        let led = SpendLedger::load(&dir.path().join("nope.json"));
        assert_eq!(led.spent_within("anything", 100, 1000), 0);
    }

    #[test]
    fn corrupt_file_starts_fresh() {
        // Scenario 3.
        let dir = tempdir().unwrap();
        let path = dir.path().join("spend-ledger.json");
        std::fs::write(&path, "{ not json at all").unwrap();
        let led = SpendLedger::load(&path);
        assert_eq!(led.spent_within("x", 100, 1000), 0, "corrupt → fresh, no panic");
    }

    #[test]
    fn rows_older_than_window_pruned_on_load() {
        // Scenario 4: buckets older than the window are pruned on load.
        let dir = tempdir().unwrap();
        let path = dir.path().join("spend-ledger.json");
        let mut led = SpendLedger::default();
        let a = SpendLedger::hash_appkey("key-a");
        led.record_spend(&a, 100); // old
        led.record_spend(&a, 5_000); // recent
        led.save(&path).unwrap();
        // Window = last 1000s ending at now=5000 → cutoff 4000; the second-100 row
        // is pruned, the second-5000 row survives.
        let loaded = SpendLedger::load_pruned(&path, 4_000);
        assert_eq!(loaded.spent_within(&a, 100_000, 5_000), 1, "only the recent row survives");
    }

    #[test]
    fn spend_is_scoped_to_the_right_credential() {
        // Scenario 5: recording for one hash never touches another's window.
        let mut led = SpendLedger::default();
        let a = SpendLedger::hash_appkey("key-a");
        let b = SpendLedger::hash_appkey("key-b");
        led.record_spend(&a, 2000);
        led.record_spend(&a, 2000);
        led.record_spend(&b, 2000);
        assert_eq!(led.spent_within(&a, 100, 2000), 2);
        assert_eq!(led.spent_within(&b, 100, 2000), 1);
        assert_eq!(led.spent_within("unknown", 100, 2000), 0);
    }

    #[test]
    fn prune_drops_idle_credentials_but_keeps_misses() {
        let mut led = SpendLedger::default();
        let a = SpendLedger::hash_appkey("key-a");
        let b = SpendLedger::hash_appkey("key-b");
        led.record_spend(&a, 100);
        led.record_model_miss(&b);
        led.prune_before(1_000);
        // a had only an old bucket and no misses → dropped; b kept for its miss.
        assert_eq!(led.spent_within(&a, 100_000, 2_000), 0);
        assert_eq!(led.model_misses(&b), 1);
    }

    #[test]
    fn ledger_path_prefers_env_override() {
        std::env::set_var(SPEND_LEDGER_ENV, "/tmp/pinned/spend-ledger.json");
        let p = spend_ledger_path(Path::new("/data/home/catalog"));
        assert_eq!(p, PathBuf::from("/tmp/pinned/spend-ledger.json"));
        std::env::remove_var(SPEND_LEDGER_ENV);
        // Default derives beside the catalog like probes_dir_for.
        let p = spend_ledger_path(Path::new("/data/home/catalog"));
        assert_eq!(p, PathBuf::from("/data/home/state/spend-ledger.json"));
    }

    // --- plan_dispatch (AE3) ---

    #[test]
    fn no_measured_budget_always_proceeds() {
        let model = BudgetModel::default(); // budget_calls None
        let led = SpendLedger::default();
        assert_eq!(
            plan_dispatch(&model, &led, "h", 0, 9_999),
            BudgetDecision::Proceed,
            "plan-ahead is inert without a measured budget"
        );
    }

    #[test]
    fn measured_budget_defers_when_estimate_exceeds_remainder() {
        // AE3: ledger shows fewer remaining calls than the symbol's estimate → Defer.
        let model = BudgetModel {
            budget_calls: Some(100),
            window_secs: 1000,
            ..BudgetModel::default()
        };
        let h = SpendLedger::hash_appkey("key");
        let mut led = SpendLedger::default();
        for t in 0..95 {
            led.record_spend(&h, 500 + t); // 95 calls spent in-window
        }
        // remaining = 100 - 95 = 5; a symbol estimated at 13 pages defers.
        match plan_dispatch(&model, &led, &h, 600, 13) {
            BudgetDecision::Defer { estimated, remaining } => {
                assert_eq!(estimated, 13);
                assert_eq!(remaining, 5);
            }
            other => panic!("expected Defer, got {other:?}"),
        }
        // A cheap symbol (estimate ≤ remaining) proceeds.
        assert_eq!(plan_dispatch(&model, &led, &h, 600, 5), BudgetDecision::Proceed);
    }

    #[test]
    fn triple_too_big_for_the_whole_budget_proceeds_not_defers_forever() {
        // A triple estimated larger than the ENTIRE budget can never fit any cold
        // window; deferring it would stall the symbol forever. It must Proceed (the
        // in-process recovery arms narrow it), even on a fully cold budget.
        let model = BudgetModel { budget_calls: Some(10), window_secs: 1000, ..BudgetModel::default() };
        let led = SpendLedger::default(); // cold: remaining == budget == 10
        assert_eq!(
            plan_dispatch(&model, &led, "h", 0, 25),
            BudgetDecision::Proceed,
            "estimate > whole budget must proceed, never defer into a permanent gap"
        );
        // Boundary: estimate == budget on a cold budget fits → proceed.
        assert_eq!(plan_dispatch(&model, &led, "h", 0, 10), BudgetDecision::Proceed);
    }

    // --- U4 probe: classifier, ceiling, cold-budget, report ---

    use ls_core::LsError;

    #[test]
    fn classify_maps_each_outcome() {
        assert_eq!(classify_call::<()>(&Ok(())), CallVerdict::Served);
        let throttled: ls_core::LsResult<()> =
            Err(LsError::ApiError { code: "IGW00201".into(), message: "over".into() });
        assert_eq!(classify_call(&throttled), CallVerdict::Throttled);
        let other: ls_core::LsResult<()> =
            Err(LsError::ApiError { code: "IGW00301".into(), message: "x".into() });
        assert_eq!(classify_call(&other), CallVerdict::OtherApi("IGW00301".into()));
        let transport: ls_core::LsResult<()> = Err(LsError::Auth("no token".into()));
        assert_eq!(classify_call(&transport), CallVerdict::Transport);
    }

    #[test]
    fn scope_mapping_covers_ae1_and_ae2() {
        assert_eq!(scope_from_stage0(&CallVerdict::Served), BucketScope::PerCredential); // AE1
        assert_eq!(scope_from_stage0(&CallVerdict::Throttled), BucketScope::BroaderThanCredential); // AE2
        assert_eq!(scope_from_stage0(&CallVerdict::Transport), BucketScope::Unknown); // inconclusive
    }

    /// A fake caller that serves its first `serve_first` MarketData calls, then
    /// throttles. `other_class_call` always serves.
    struct FakeCaller {
        serve_first: usize,
        calls: std::sync::atomic::AtomicUsize,
    }
    #[async_trait::async_trait]
    impl ProbeCaller for FakeCaller {
        async fn market_data_call(&self) -> ls_core::LsResult<()> {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n < self.serve_first {
                Ok(())
            } else {
                Err(LsError::ApiError { code: "IGW00201".into(), message: "over".into() })
            }
        }
        async fn other_class_call(&self) -> ls_core::LsResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn cold_budget_counts_serves_until_throttle() {
        // Stage 1: serves 7 then throttles → cold budget 7, stopped Throttled.
        let caller = FakeCaller { serve_first: 7, calls: std::sync::atomic::AtomicUsize::new(0) };
        let mut ceiling = CallCeiling::new(100);
        let cold = measure_cold_budget(&caller, &mut ceiling, std::time::Duration::ZERO).await;
        assert_eq!(cold.calls_served, 7);
        assert_eq!(cold.stopped, StageStop::Throttled);
        assert_eq!(ceiling.made(), 8, "7 serves + the throttling call");
    }

    #[tokio::test]
    async fn ceiling_stops_the_stage_and_refuses_further_calls() {
        // R5: a cold budget larger than the ceiling stops AT the ceiling, reporting
        // partial data and issuing no call past it.
        let caller = FakeCaller { serve_first: 1000, calls: std::sync::atomic::AtomicUsize::new(0) };
        let mut ceiling = CallCeiling::new(5);
        let cold = measure_cold_budget(&caller, &mut ceiling, std::time::Duration::ZERO).await;
        assert_eq!(cold.stopped, StageStop::Ceiling);
        assert_eq!(cold.calls_served, 5, "exactly the ceiling's worth served");
        assert_eq!(ceiling.made(), 5, "never issued a call past the ceiling");
    }

    #[test]
    fn report_round_trips_through_serde() {
        let report = ProbeReport {
            probed_at: "2026-07-15T02:00:00Z".to_string(),
            ceiling: 40,
            total_calls: 9,
            stage0_scope: Some(Stage0Report {
                verdict: CallVerdict::Served,
                scope: BucketScope::PerCredential,
            }),
            stage1_cold_budget: Some(ColdBudget { calls_served: 8, stopped: StageStop::Throttled }),
            stage2_refill: Some(RefillReport {
                first_success_secs: Some(120),
                sample_intervals_secs: vec![30, 60, 120],
            }),
            stage3_cross_class: Some(CrossClassReport {
                verdict: CallVerdict::Served,
                spans_classes: false,
            }),
            notes: vec!["spare key cold (untouched 24h)".to_string()],
        };
        let json = report.to_pretty_json();
        let back: ProbeReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
        // Carries per-stage counts + timestamp (scenario 4).
        assert!(json.contains("calls_served"), "per-stage counts present");
        assert!(json.contains("2026-07-15T02:00:00Z"), "timestamp present");
    }
}

/// U4 stage-1 over a real SDK against a mocked gateway: the cold-budget driver
/// counts exactly the N successes the mock serves before it starts returning
/// `IGW00201` (scenario 3). Offline — no live gateway.
#[cfg(test)]
mod probe_wiremock_tests {
    use super::*;
    use ls_sdk::LsSdk;
    use ls_sdk_test_support::{mock_config, mount_token};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn json_response(body: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_string(body.to_string())
            .insert_header("content-type", "application/json")
    }

    #[tokio::test]
    async fn stage1_counts_mocked_successes_before_igw00201() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        // Serve t1102 successfully the first 4 times (higher precedence), then the
        // catch-all returns IGW00201.
        Mock::given(method("POST"))
            .and(path("/stock/market-data"))
            .and(header("tr_cd", "t1102"))
            .respond_with(json_response(serde_json::json!({
                "rsp_cd": "00000", "rsp_msg": "정상",
                "t1102OutBlock": { "hname": "삼성전자", "price": "60000" }
            })))
            .up_to_n_times(4)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/stock/market-data"))
            .and(header("tr_cd", "t1102"))
            .respond_with(json_response(serde_json::json!({
                "rsp_cd": "IGW00201", "rsp_msg": "호출 거래건수를 초과하였습니다."
            })))
            .with_priority(2)
            .mount(&server)
            .await;

        let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");
        let ledger = Arc::new(Mutex::new(SpendLedger::default()));
        let cred_hash = SpendLedger::hash_appkey("mock-appkey");
        let caller = SdkProbeCaller::new(sdk, Arc::clone(&ledger), cred_hash.clone(), "005930".to_string());

        let mut ceiling = CallCeiling::new(50);
        let cold = measure_cold_budget(&caller, &mut ceiling, std::time::Duration::ZERO).await;
        assert_eq!(cold.calls_served, 4, "the mock served exactly 4 before IGW00201");
        assert_eq!(cold.stopped, StageStop::Throttled);
        // Every dispatch was recorded against the probe credential (5 = 4 + throttle).
        assert_eq!(
            ledger.lock().unwrap().spent_within(&cred_hash, 100_000, chrono::Utc::now().timestamp()),
            5,
            "all 5 dispatches recorded in the ledger"
        );
    }
}
