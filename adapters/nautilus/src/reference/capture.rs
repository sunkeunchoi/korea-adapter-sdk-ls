//! Reference-data live capture (U2): fetch the six captured TRs via SDK
//! facades, join by `shcode` into [`InstrumentMetadata`] records, and assemble
//! the [`UniverseMetadata`] artifact.
//!
//! Sources (R1–R3):
//! - skeleton: `t8430` master (market class; equities-only — non-empty
//!   `etfgubun` rows dropped, and preferred shares dropped by issue-sequence
//!   digit, R2/P5), `t2522` (single-stock-futures underlying
//!   set), `t1904` ×2 (KODEX 200 / KOSDAQ150 holdings = index proxy);
//! - decorate: `t1444` ranked cap boards (bounded — below-board symbols keep
//!   `market_cap = Unavailable` and take the small-cap tier by exclusion);
//!   turnover is **not** captured this turn (`t1463` walk deferred, R2);
//! - gate: `t1405` + `t1404` designation categories → hard tradability filter.
//!
//! A TR that fails on paper is recorded in provenance with its failure code
//! rather than silently dropping the attribute; the affected attribute resolves
//! `Unavailable` (R4). The capture is paced and budget-gated so the paged cap
//! reads do not starve the minute-bar ingest in the shared attended window
//! (KTD6).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use ls_core::HasPagination;
use ls_sdk::market_session::{T1904Request, T2522Request, T8430Request};
use ls_sdk::paginated::{T1404Request, T1405Request, T1444Request};
use ls_sdk::LsSdk;

use crate::ingest::budget::{plan_dispatch, BudgetDecision, BudgetModel, SpendLedger};
use crate::reference::universe_metadata::{
    assign_cap_tiers, assign_liquidity_tier, is_tradable, CapCutoff, Designation,
    DesignationKind, InstrumentMetadata, MarketClass, MetadataProvenance, Resolved, TrFailure,
    UniverseMetadata, DEFAULT_CAP_TOP_QUANTILE, IndexMembership,
};

/// The TR set the capture joins (provenance `source_trs`).
pub const SOURCE_TRS: [&str; 6] = ["t8430", "t2522", "t1904", "t1444", "t1405", "t1404"];

/// The recorded instrument-type filter (R2): the skeleton is equities-only and
/// common-issue-only — preferred shares are excluded by issue-sequence digit (P5).
pub const INSTRUMENT_TYPE_FILTER: &str =
    "t8430 etfgubun in {empty, '0'} (equities-only; ETF '1' / ETN '2' rows dropped before \
     tiering — the live master serves '0' for common equities). Letter-suffixed shcodes \
     (e.g. 02826K/33626L, 신형우선주) are dropped. Preferred shares are excluded by \
     issue-sequence digit (P5): the 6th digit of a 6-digit code encodes the issue sequence, \
     and a digit other than '0' is a non-common issue class (e.g. 005935 삼성전자우) — dropped \
     before tiering, the same rule the pit walk freezes on, with the dropped codes recorded \
     in provenance.dropped_preferred. t8430 exposes no flag for SPACs or REITs; that residual \
     is an accepted, documented limitation.";

/// What the recorded instrument-type filter (R2) does with one `t8430` master
/// row. Each drop reason is named rather than folded into a bare `continue`, so
/// the capture can *report* what it excluded instead of only asserting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterRowVerdict {
    /// A common-equity row — kept in the skeleton under this market class.
    Keep(MarketClass),
    /// Not a 6-digit numeric code: the letter-suffixed 신형우선주 class
    /// (`02826K` / `33626L`), the one preferred class the master makes
    /// detectable from its own fields.
    NotSixDigitNumeric,
    /// An ETF (`etfgubun` `"1"`) or ETN (`"2"`) row.
    NotEquity,
    /// P5's issue-sequence rule: the 6th digit encodes the issue sequence, and
    /// ≠ `0` is a preferred-share class (e.g. `005935` 삼성전자우). This is the
    /// same rule [`crate::reference::pit_walk::freeze_walk_set`] applies when it
    /// freezes a walk set — applied here at construction so the artifact itself
    /// is clean, not only the sets derived from it.
    PreferredIssueSequence,
    /// Neither KOSPI nor KOSDAQ — outside the v1 identity.
    OutsideMarketClasses,
}

/// Apply the recorded instrument-type filter to one `t8430` master row.
///
/// Order is load-bearing twice over: the 6-digit numeric guard runs first
/// because it is what makes indexing the 6th byte safe, and the `etfgubun`
/// check precedes the issue-sequence rule so an ETF is reported as an ETF
/// rather than counted into `dropped_preferred`.
pub fn classify_master_row(shcode: &str, etfgubun: &str, gubun: &str) -> MasterRowVerdict {
    if shcode.len() != 6 || !shcode.bytes().all(|b| b.is_ascii_digit()) {
        return MasterRowVerdict::NotSixDigitNumeric;
    }
    // The live master serves "0" (not empty) for common equities — the adapter's
    // instrument mapper reads the same convention (`instruments.rs`).
    if !matches!(etfgubun, "" | "0") {
        return MasterRowVerdict::NotEquity;
    }
    if shcode.as_bytes()[5] != b'0' {
        return MasterRowVerdict::PreferredIssueSequence;
    }
    match gubun {
        "1" => MasterRowVerdict::Keep(MarketClass::Kospi),
        "2" => MasterRowVerdict::Keep(MarketClass::Kosdaq),
        _ => MasterRowVerdict::OutsideMarketClasses,
    }
}

/// Page-walk safety cap for the `t1405`/`t1404` category walks (mirrors the
/// SDK's `market_cap_top_all` cap).
const MAX_DESIGNATION_PAGES: usize = 32;

/// One `t1444` ranked cap board to walk.
#[derive(Debug, Clone)]
pub struct CapBoard {
    /// The 업종코드 (`001` KOSPI composite; `301` KOSDAQ composite).
    pub upcode: String,
    /// The market class the board ranks.
    pub market_class: MarketClass,
    /// How deep to walk the board (the walk stops earlier on a terminal page).
    pub max_rows: usize,
}

/// One designation-category query against `t1405` or `t1404` (R3). The
/// category enum was confirmed live in the closed-window pre-flight
/// (2026-07-10, field-level row evidence) — `gubun` is the market axis
/// (`0` all / `1` KOSPI / `2` KOSDAQ) on **both** TRs, `jongchk` is the
/// category:
///
/// - `t1405`: 1 투자경고(warning) / 2 매매정지(halt, >1 page) / 3 정리매매
///   (liquidation → halt) / 4 투자주의(caution, one-day) / 5 투자위험(risk)
///   / 6 투자위험예고(pre-announce → warning) / 7 단기과열(overheated).
/// - `t1404`: 1 관리(managed, >1 page) / 2 불성실공시(caution) / 3 투자유의
///   (caution) / 4 투자환기(caution).
///
/// **`jongchk = "0"` is NOT a category**: on `t1404` it returns the whole
/// non-designated board — querying it would mark every symbol non-tradable —
/// so [`capture`] refuses it. The queried categories are recorded in
/// provenance either way.
#[derive(Debug, Clone)]
pub struct DesignationQuery {
    /// The `gubun` request field (market axis; `0` = all markets).
    pub gubun: String,
    /// The `jongchk` request field (the category — never `"0"`).
    pub jongchk: String,
    /// The designation category this query lists.
    pub kind: DesignationKind,
}

/// One `t1904` ETF-holdings proxy read.
#[derive(Debug, Clone)]
pub struct EtfProxy {
    /// The ETF shcode (`069500` KODEX 200; `229200` KODEX KOSDAQ150).
    pub shcode: String,
    /// The index the holdings proxy.
    pub index: IndexMembership,
}

/// Capture configuration. Defaults via [`CaptureConfig::new`]; the category
/// specs and upcodes are operator-overridable (the pre-flight confirms them).
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// RFC-3339 capture timestamp (stamped into provenance).
    pub captured_at: String,
    /// The KST session date the tags are as-of (`YYYYMMDD`).
    pub session_date: String,
    /// The `t1444` boards to walk (default KOSPI `001` + KOSDAQ `301`).
    pub cap_boards: Vec<CapBoard>,
    /// The `t1904` index proxies (default KODEX 200 + KODEX KOSDAQ150).
    pub etf_proxies: Vec<EtfProxy>,
    /// `t1405` designation categories (defaults documented; confirm live).
    pub t1405_categories: Vec<DesignationQuery>,
    /// `t1404` designation categories (defaults documented; confirm live).
    pub t1404_categories: Vec<DesignationQuery>,
    /// The pre-registered cap-tier top quantile.
    pub cap_top_quantile: f64,
    /// Inter-call pacing (KTD6 — shares the attended window with the ingest).
    pub pace: Duration,
    /// One-shot backoff before retrying a fetch that hit `IGW00201` (the
    /// 2026-07-10 rehearsal showed the budget refilling within ~2 minutes).
    pub throttle_backoff: Duration,
}

impl CaptureConfig {
    /// The default capture over both markets for one session.
    pub fn new(captured_at: impl Into<String>, session_date: impl Into<String>) -> Self {
        CaptureConfig {
            captured_at: captured_at.into(),
            session_date: session_date.into(),
            // 200 rows/board = Top 100 + Mid 100 per market — deep enough for
            // per-stratum sampling, and half the page cost of 400: the 2026-07-10
            // rehearsal showed a 2×20-page walk exhausting a warm IGW00201
            // cumulative budget mid-walk while every single-page read served.
            cap_boards: vec![
                CapBoard { upcode: "001".into(), market_class: MarketClass::Kospi, max_rows: 200 },
                CapBoard { upcode: "301".into(), market_class: MarketClass::Kosdaq, max_rows: 200 },
            ],
            etf_proxies: vec![
                EtfProxy { shcode: "069500".into(), index: IndexMembership::Kospi200 },
                EtfProxy { shcode: "229200".into(), index: IndexMembership::Kosdaq150 },
            ],
            // Categories confirmed live in the closed-window pre-flight
            // (2026-07-10; see DesignationQuery). Overridable via
            // LS_CAPTURE_T1405_CATEGORIES / LS_CAPTURE_T1404_CATEGORIES.
            t1405_categories: vec![
                DesignationQuery { gubun: "0".into(), jongchk: "1".into(), kind: DesignationKind::Warning },
                DesignationQuery { gubun: "0".into(), jongchk: "2".into(), kind: DesignationKind::Halt },
                DesignationQuery { gubun: "0".into(), jongchk: "3".into(), kind: DesignationKind::Halt },
                DesignationQuery { gubun: "0".into(), jongchk: "4".into(), kind: DesignationKind::Caution },
                DesignationQuery { gubun: "0".into(), jongchk: "5".into(), kind: DesignationKind::Risk },
                DesignationQuery { gubun: "0".into(), jongchk: "6".into(), kind: DesignationKind::Warning },
                DesignationQuery { gubun: "0".into(), jongchk: "7".into(), kind: DesignationKind::Overheated },
            ],
            t1404_categories: vec![
                DesignationQuery { gubun: "0".into(), jongchk: "1".into(), kind: DesignationKind::Managed },
                DesignationQuery { gubun: "0".into(), jongchk: "2".into(), kind: DesignationKind::Caution },
                DesignationQuery { gubun: "0".into(), jongchk: "3".into(), kind: DesignationKind::Caution },
                DesignationQuery { gubun: "0".into(), jongchk: "4".into(), kind: DesignationKind::Caution },
            ],
            cap_top_quantile: DEFAULT_CAP_TOP_QUANTILE,
            // 2s pacing: the rehearsal showed 600ms landing calls in the
            // short-window budget hole behind the big master read.
            pace: Duration::from_millis(2000),
            throttle_backoff: Duration::from_secs(120),
        }
    }
}

/// A completed capture: the artifact plus the gateway calls it spent (recorded
/// into the shared spend ledger so the minute ingest's budget planner sees the
/// capture's cumulative cost, KTD6).
#[derive(Debug)]
pub struct CaptureOutcome {
    /// The assembled artifact.
    pub artifact: UniverseMetadata,
    /// Gateway calls made (skeleton + boards + category pages).
    pub calls_made: u32,
}

/// A failed capture. Carries the calls spent **before** the failure so the
/// caller can still record them into the shared spend ledger — a failed run
/// spends real budget (the 2026-07-10 rehearsal burned a board walk before
/// erroring), and dropping that spend would make the ingest planner
/// over-optimistic in the shared attended window (KTD6).
#[derive(Debug)]
pub struct CaptureError {
    /// What failed (credential-free).
    pub message: String,
    /// Gateway calls spent before the failure.
    pub calls_made: u32,
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (after {} gateway calls)", self.message, self.calls_made)
    }
}

impl std::error::Error for CaptureError {}

/// A conservative pre-dispatch call estimate for the whole capture (KTD6):
/// 1 (`t8430`) + 1 (`t2522`) + one per ETF proxy + the boards' worst-case page
/// counts (~20 rows/page) + one page per designation category (walks usually
/// terminate on page one; extra pages are covered by the conservative board
/// estimate's slack).
pub fn estimated_capture_calls(cfg: &CaptureConfig) -> u32 {
    let board_pages: usize =
        cfg.cap_boards.iter().map(|b| b.max_rows.div_ceil(20)).sum();
    (2 + cfg.etf_proxies.len() + board_pages + cfg.t1405_categories.len() + cfg.t1404_categories.len())
        as u32
}

/// The capture-side budget gate (KTD6): consult the measured MarketData budget
/// before spending any of the shared attended window; a [`BudgetDecision::Defer`]
/// refuses the capture with the numbers rather than starving the minute ingest.
pub fn budget_gate(
    model: &BudgetModel,
    ledger: &SpendLedger,
    cred_hash: &str,
    now_unix: i64,
    estimated_calls: u32,
) -> Result<(), String> {
    match plan_dispatch(model, ledger, cred_hash, now_unix, estimated_calls) {
        BudgetDecision::Proceed => Ok(()),
        BudgetDecision::Defer { estimated, remaining } => Err(format!(
            "budget defer: capture estimated at {estimated} calls exceeds the remaining \
             MarketData budget window ({remaining} calls) — re-run on a cold window so the \
             capture does not starve the minute-bar ingest (KTD6)"
        )),
    }
}

/// Run the live capture (U2): fetch, join by `shcode`, tier, gate, and return
/// the artifact. Fails hard only when the skeleton (`t8430`) or **every** cap
/// board fails — everything else records a [`TrFailure`] and resolves the
/// affected attribute `Unavailable` (R4), so the join has no silent holes.
pub async fn capture(sdk: &LsSdk, cfg: &CaptureConfig) -> Result<CaptureOutcome, CaptureError> {
    let fail = |message: String, calls_made: u32| CaptureError { message, calls_made };
    // Refuse the whole-board pseudo-category (pre-flight finding, 2026-07-10):
    // `jongchk = "0"` on t1404 returns every listed issue, designated or not —
    // treating those rows as designations would mark the entire market
    // non-tradable. Fail closed before any gateway call.
    for (tr, qs) in [("t1405", &cfg.t1405_categories), ("t1404", &cfg.t1404_categories)] {
        if let Some(q) = qs.iter().find(|q| q.jongchk.trim() == "0") {
            return Err(fail(
                format!(
                    "{tr} designation query with jongchk=\"0\" (gubun={}) is the whole board, not a \
                     category — it would designate every symbol; query categories individually",
                    q.gubun
                ),
                0,
            ));
        }
    }
    let mut failures: Vec<TrFailure> = Vec::new();
    let mut calls_made: u32 = 0;

    // --- t2522: single-stock-futures underlying set (derivative flag, R1).
    tokio::time::sleep(cfg.pace).await;
    calls_made += 1;
    let derivative_set: Option<HashSet<String>> =
        match sdk.market_session().stock_futures_underlying_master(&T2522Request::new()).await {
            Ok(resp) => Some(
                resp.outblock1.iter().map(|r| r.bsc_asts_is_cd.trim().to_string()).collect(),
            ),
            Err(e) => {
                failures.push(TrFailure { tr: "t2522".into(), code: fail_code(&e) });
                None
            }
        };

    // --- t1904 ×2: ETF-holdings index proxy (R1, AE4).
    let mut index_by_shcode: HashMap<String, IndexMembership> = HashMap::new();
    let mut any_etf_failed = false;
    for proxy in &cfg.etf_proxies {
        tokio::time::sleep(cfg.pace).await;
        calls_made += 1;
        let req = T1904Request::new(proxy.shcode.clone(), cfg.session_date.clone(), "1");
        match sdk.market_session().etf_constituents(&req).await {
            Ok(resp) if resp.outblock1.is_empty() => {
                // Empty-Ok holdings (review finding): treating this as served
                // would resolve EVERY symbol Proxy(NotMember) — an all-false
                // index axis whose recorded resolution falsely implies data.
                any_etf_failed = true;
                failures.push(TrFailure { tr: "t1904".into(), code: "empty".into() });
            }
            Ok(resp) => {
                for row in &resp.outblock1 {
                    let code = row.shcode.trim().to_string();
                    if !code.is_empty() {
                        index_by_shcode.entry(code).or_insert(proxy.index);
                    }
                }
            }
            Err(e) => {
                any_etf_failed = true;
                failures.push(TrFailure { tr: "t1904".into(), code: fail_code(&e) });
            }
        }
    }

    // --- t1444: ranked cap boards (R2). Below-board symbols keep Unavailable.
    let mut cap_by_shcode: HashMap<String, f64> = HashMap::new();
    let mut boards_served = 0usize;
    for board in &cfg.cap_boards {
        tokio::time::sleep(cfg.pace).await;
        match walk_t1444(sdk, board, cfg.pace, cfg.throttle_backoff).await {
            Ok((rows, pages)) => {
                calls_made += pages;
                boards_served += 1;
                for (code, total) in rows {
                    cap_by_shcode.entry(code).or_insert(total);
                }
            }
            Err((code, pages)) => {
                calls_made += pages;
                if code == "IGW00201" {
                    // Transient throttle (even after the page retry): abort
                    // rather than tier a whole market below-board (rehearsal
                    // finding, 2026-07-10) — re-run on a colder window.
                    return Err(fail(
                        format!(
                            "t1444 board upcode {} throttled (IGW00201) after retry — a missing                              board mis-tiers its whole market; re-run when the budget is colder",
                            board.upcode
                        ),
                        calls_made,
                    ));
                }
                failures.push(TrFailure { tr: "t1444".into(), code });
            }
        }
    }
    if boards_served == 0 {
        return Err(fail(
            format!(
                "every t1444 cap board failed — no cap axis, no stratification (failures: {failures:?})"
            ),
            calls_made,
        ));
    }

    // --- Gate: t1405 + t1404 designation categories (R3).
    let mut designation_by_shcode: HashMap<String, Designation> = HashMap::new();
    for q in &cfg.t1405_categories {
        tokio::time::sleep(cfg.pace).await;
        let mut attempt = walk_t1405(sdk, q).await;
        if let Err((code, pages)) = &attempt {
            if code == "IGW00201" {
                // Count attempt 1's real spend before retrying (review
                // finding: dropped pages under-record the shared ledger, KTD6).
                calls_made += pages;
                tokio::time::sleep(cfg.throttle_backoff).await;
                attempt = walk_t1405(sdk, q).await;
            }
        }
        match attempt {
            Ok((codes, pages)) => {
                calls_made += pages;
                for code in codes {
                    designation_by_shcode
                        .entry(code)
                        .or_insert(Designation { kind: q.kind, source_tr: "t1405".into() });
                }
            }
            Err((code, pages)) => {
                calls_made += pages;
                if code == "IGW00201" {
                    return Err(fail(
                        format!(
                            "t1405 category gubun={} jongchk={} throttled (IGW00201) after retry — a transient hole in the hard tradability gate must not be baked into the artifact; re-run when the budget is colder",
                            q.gubun, q.jongchk
                        ),
                        calls_made,
                    ));
                }
                failures.push(TrFailure { tr: "t1405".into(), code });
            }
        }
    }
    for q in &cfg.t1404_categories {
        tokio::time::sleep(cfg.pace).await;
        let mut attempt = walk_t1404(sdk, q).await;
        if let Err((code, pages)) = &attempt {
            if code == "IGW00201" {
                // Count attempt 1's real spend before retrying (review
                // finding: dropped pages under-record the shared ledger, KTD6).
                calls_made += pages;
                tokio::time::sleep(cfg.throttle_backoff).await;
                attempt = walk_t1404(sdk, q).await;
            }
        }
        match attempt {
            Ok((codes, pages)) => {
                calls_made += pages;
                for code in codes {
                    designation_by_shcode
                        .entry(code)
                        .or_insert(Designation { kind: q.kind, source_tr: "t1404".into() });
                }
            }
            Err((code, pages)) => {
                calls_made += pages;
                if code == "IGW00201" {
                    return Err(fail(
                        format!(
                            "t1404 category gubun={} jongchk={} throttled (IGW00201) after retry — a transient hole in the hard tradability gate must not be baked into the artifact; re-run when the budget is colder",
                            q.gubun, q.jongchk
                        ),
                        calls_made,
                    ));
                }
                failures.push(TrFailure { tr: "t1404".into(), code });
            }
        }
    }

    // --- Skeleton: t8430 master (market class + equities-only filter, R1/R2).
    // Fetched LAST deliberately (rehearsal finding, 2026-07-10): its ~800KB
    // response drains the gateway's short-window budget and throttled the very
    // next reads; the join is in-memory, so fetch order is free.
    tokio::time::sleep(cfg.pace).await;
    calls_made += 1;
    let master = sdk.market_session().stock_issues(&T8430Request::all()).await.map_err(|e| {
        fail(format!("t8430 master failed — no skeleton, no capture: {}", fail_code(&e)), calls_made)
    })?;
    let mut skeleton: Vec<(String, MarketClass)> = Vec::new();
    // Counted, not silently dropped (P4's precedent): provenance carries the
    // codes the issue-sequence rule removed, so the declared filter is evidenced
    // by what it excluded rather than merely asserted.
    let mut dropped_preferred: Vec<String> = Vec::new();
    for row in &master.outblock {
        let shcode = row.shcode.trim().to_string();
        match classify_master_row(&shcode, row.etfgubun.trim(), row.gubun.trim()) {
            MasterRowVerdict::Keep(market_class) => skeleton.push((shcode, market_class)),
            MasterRowVerdict::PreferredIssueSequence => dropped_preferred.push(shcode),
            MasterRowVerdict::NotSixDigitNumeric
            | MasterRowVerdict::NotEquity
            | MasterRowVerdict::OutsideMarketClasses => {}
        }
    }
    dropped_preferred.sort();
    if skeleton.is_empty() {
        return Err(fail(
            "t8430 returned no equity rows — cannot capture an empty skeleton".to_string(),
            calls_made,
        ));
    }

    // --- Join by shcode into U1 records (R4: resolution recorded per symbol).
    let mut records: Vec<InstrumentMetadata> = skeleton
        .into_iter()
        .map(|(shcode, market_class)| {
            let market_cap = match cap_by_shcode.get(&shcode) {
                Some(total) => Resolved::Value(*total),
                None => Resolved::Unavailable,
            };
            // Turnover is not captured this turn (t1463 deferred, R2).
            let turnover = Resolved::Unavailable;
            let has_derivative = match &derivative_set {
                Some(set) => Resolved::Value(set.contains(&shcode)),
                None => Resolved::Unavailable,
            };
            // The ETF-holdings proxy: membership is Proxy; absence is Proxy(NotMember)
            // only when every proxy read served — else Unavailable (AE4/R4).
            let index_membership = match index_by_shcode.get(&shcode) {
                Some(idx) => Resolved::Proxy(*idx),
                None if any_etf_failed => Resolved::Unavailable,
                None => Resolved::Proxy(IndexMembership::NotMember),
            };
            let designation = designation_by_shcode.get(&shcode).cloned();
            let tradable = is_tradable(&designation);
            InstrumentMetadata {
                shcode,
                market_class,
                market_cap,
                cap_tier: crate::reference::universe_metadata::CapTier::BelowBoard,
                liquidity_tier: assign_liquidity_tier(&turnover),
                turnover,
                index_membership,
                has_derivative,
                designation,
                tradable,
            }
        })
        .collect();
    records.sort_by(|a, b| a.shcode.cmp(&b.shcode));

    let cap_cutoffs: Vec<CapCutoff> = assign_cap_tiers(&mut records, cfg.cap_top_quantile);

    let artifact = UniverseMetadata {
        provenance: MetadataProvenance {
            captured_at: cfg.captured_at.clone(),
            session_date: cfg.session_date.clone(),
            source_trs: SOURCE_TRS.iter().map(|s| s.to_string()).collect(),
            instrument_type_filter: INSTRUMENT_TYPE_FILTER.to_string(),
            tier_boundary_rule: format!(
                "per-market cap-rank quantile {}: top ceil(q x on_board) = Top, rest = Mid; \
                 unresolved cap = BelowBoard (small-cap by exclusion). Designation categories \
                 queried: t1405 [{}], t1404 [{}].",
                cfg.cap_top_quantile,
                describe_categories(&cfg.t1405_categories),
                describe_categories(&cfg.t1404_categories),
            ),
            cap_cutoffs,
            paper_incompatible: failures,
            dropped_preferred,
        },
        records,
    };
    // Fail closed BEFORE the caller writes: an artifact that would not pass the
    // offline validator must never land on disk (the capture-universe pattern).
    if let Err(errs) = artifact.validate() {
        return Err(fail(
            format!("captured artifact failed validation:\n  - {}", errs.join("\n  - ")),
            calls_made,
        ));
    }
    Ok(CaptureOutcome { artifact, calls_made })
}


/// Walk one `t1444` ranked cap board with **inter-page pacing** (rehearsal
/// finding, 2026-07-10): the SDK's `market_cap_top_all` fires continuation
/// pages back-to-back at the client rate cap, and a 10-page burst trips the
/// gateway's short-window `IGW00201` even though every paced single-page read
/// serves — so the capture pages itself, sleeping `pace` between pages, with
/// one backoff-retry per throttled page. Termination mirrors the SDK walk:
/// terminal `idx` (empty/`"0"`/header `tr_cont: N`), a repeated cursor, a
/// no-progress page, `max_rows`, or the page safety cap. Returns
/// `(shcode, market_cap)` rows (deduped, first-seen) and the pages spent.
async fn walk_t1444(
    sdk: &LsSdk,
    board: &CapBoard,
    pace: Duration,
    backoff: Duration,
) -> Result<(Vec<(String, f64)>, u32), (String, u32)> {
    let mut req = T1444Request::new(board.upcode.clone());
    let mut seen: HashSet<String> = HashSet::new();
    let mut rows: Vec<(String, f64)> = Vec::new();
    let mut pages: u32 = 0;
    loop {
        if pages > 0 {
            tokio::time::sleep(pace).await;
        }
        pages += 1;
        let resp = match sdk.paginated().market_cap_top(&req).await {
            Ok(r) => r,
            Err(e) if is_throttled(&e) => {
                // One page-level backoff-retry: the short-window budget refills
                // in ~2 minutes (rehearsal finding).
                tokio::time::sleep(backoff).await;
                pages += 1;
                match sdk.paginated().market_cap_top(&req).await {
                    Ok(r) => r,
                    Err(e) => return Err((fail_code(&e), pages)),
                }
            }
            Err(e) => return Err((fail_code(&e), pages)),
        };
        let mut progressed = false;
        for row in &resp.outblock1 {
            let code = row.shcode.trim().to_string();
            if code.is_empty() || !seen.insert(code.clone()) {
                continue;
            }
            progressed = true;
            if let Ok(total) = row.total.trim().parse::<f64>() {
                rows.push((code, total));
            }
            if rows.len() >= board.max_rows {
                return Ok((rows, pages));
            }
        }
        let next_idx = resp.outblock.idx.trim().to_string();
        let terminal =
            next_idx.is_empty() || next_idx == "0" || resp.tr_cont.trim() == "N";
        if terminal || !progressed || next_idx == req.inblock.idx.trim() {
            return Ok((rows, pages));
        }
        if pages as usize >= MAX_DESIGNATION_PAGES {
            // A live cursor at the safety cap means the requested depth cannot
            // be served — mirroring the SDK walk's PaginationLimit rather than
            // silently truncating (a truncated board mis-tiers every symbol
            // below the cutoff into the exclusion stratum).
            return Err(("pagination_limit".to_string(), pages));
        }
        req.inblock.idx = next_idx;
        req.set_tr_cont("Y".to_string());
        req.set_tr_cont_key(resp.tr_cont_key.clone());
    }
}

/// Walk one `t1405` category across its `cts_shcode` pages. Returns the
/// designated shcodes and the pages spent. The `T1405Response` carries no
/// `tr_cont` header fields (a single-page read) — continuation threads the
/// **body** cursor plus a request-side `tr_cont: Y`; the walk terminates on an
/// empty/repeated cursor, a no-progress page, or the page cap.
async fn walk_t1405(sdk: &LsSdk, q: &DesignationQuery) -> Result<(Vec<String>, u32), (String, u32)> {
    let mut req = T1405Request::new(q.gubun.clone(), q.jongchk.clone());
    let mut seen: HashSet<String> = HashSet::new();
    let mut codes = Vec::new();
    let mut pages: u32 = 0;
    loop {
        pages += 1;
        let resp = match sdk.paginated().trade_suspension(&req).await {
            Ok(r) => r,
            Err(e) => return Err((fail_code(&e), pages)),
        };
        let mut progressed = false;
        for row in &resp.outblock1 {
            let code = row.shcode.trim().to_string();
            if !code.is_empty() && seen.insert(code.clone()) {
                codes.push(code);
                progressed = true;
            }
        }
        let next = resp.outblock.cts_shcode.trim().to_string();
        if next.is_empty()
            || !progressed
            || next == req.inblock.cts_shcode.trim()
            || pages as usize >= MAX_DESIGNATION_PAGES
        {
            return Ok((codes, pages));
        }
        req.inblock.cts_shcode = next;
        req.set_tr_cont("Y".to_string());
    }
}

/// Walk one `t1404` category across its `cts_shcode` pages (same body-cursor
/// protocol as [`walk_t1405`]).
async fn walk_t1404(sdk: &LsSdk, q: &DesignationQuery) -> Result<(Vec<String>, u32), (String, u32)> {
    let mut req = T1404Request::new();
    req.inblock.gubun = q.gubun.clone();
    req.inblock.jongchk = q.jongchk.clone();
    let mut seen: HashSet<String> = HashSet::new();
    let mut codes = Vec::new();
    let mut pages: u32 = 0;
    loop {
        pages += 1;
        let resp = match sdk.paginated().designation_board(&req).await {
            Ok(r) => r,
            Err(e) => return Err((fail_code(&e), pages)),
        };
        let mut progressed = false;
        for row in &resp.outblock1 {
            let code = row.shcode.trim().to_string();
            if !code.is_empty() && seen.insert(code.clone()) {
                codes.push(code);
                progressed = true;
            }
        }
        let next = resp.outblock.cts_shcode.trim().to_string();
        if next.is_empty()
            || !progressed
            || next == req.inblock.cts_shcode.trim()
            || pages as usize >= MAX_DESIGNATION_PAGES
        {
            return Ok((codes, pages));
        }
        req.inblock.cts_shcode = next;
        req.set_tr_cont("Y".to_string());
    }
}

fn describe_categories(qs: &[DesignationQuery]) -> String {
    qs.iter()
        .map(|q| format!("gubun={} jongchk={} -> {:?}", q.gubun, q.jongchk, q.kind))
        .collect::<Vec<_>>()
        .join("; ")
}

/// A credential-free failure code for provenance: the gateway `rsp_cd` when the
/// error is an API envelope, else a coarse class (mirrors the budget probe's
/// retention discipline — never a raw broker message).
/// Whether an SDK error is the gateway budget throttle (`IGW00201`).
fn is_throttled(e: &ls_core::LsError) -> bool {
    matches!(e, ls_core::LsError::ApiError { code, .. } if code == "IGW00201")
}

fn fail_code(e: &ls_core::LsError) -> String {
    match e {
        ls_core::LsError::ApiError { code, .. } => code.clone(),
        _ => "transport".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimated_calls_cover_skeleton_boards_and_categories() {
        let cfg = CaptureConfig::new("2026-07-10T00:00:00Z", "20260710");
        // 1 t8430 + 1 t2522 + 2 t1904 + 2 boards x ceil(200/20)=10 + 7 + 4 categories.
        assert_eq!(estimated_capture_calls(&cfg), (1 + 1 + 2 + 20 + 7 + 4) as u32);
    }

    #[test]
    fn default_categories_never_query_the_whole_board() {
        // Pre-flight finding (2026-07-10): jongchk "0" is the whole board on
        // t1404, not a category — the defaults must never include it.
        let cfg = CaptureConfig::new("2026-07-10T00:00:00Z", "20260710");
        for q in cfg.t1405_categories.iter().chain(&cfg.t1404_categories) {
            assert_ne!(q.jongchk, "0", "gubun={} kind={:?}", q.gubun, q.kind);
        }
        // The confirmed category counts: t1405 1..=7, t1404 1..=4.
        assert_eq!(cfg.t1405_categories.len(), 7);
        assert_eq!(cfg.t1404_categories.len(), 4);
    }

    /// P5: the issue-sequence digit excludes preferred shares at construction.
    /// Mirrors `pit_walk`'s `freeze_takes_board_tiers_and_applies_the_preferred_rule`
    /// — the two must not drift, since the pit walk freezes over this artifact.
    #[test]
    fn issue_sequence_digit_excludes_preferred_shares() {
        // The common issue is kept; its preferred class (same stem, 6th digit
        // ≠ 0) is dropped — 005930 삼성전자 vs 005935 삼성전자우.
        assert_eq!(
            classify_master_row("005930", "0", "1"),
            MasterRowVerdict::Keep(MarketClass::Kospi)
        );
        assert_eq!(
            classify_master_row("005935", "0", "1"),
            MasterRowVerdict::PreferredIssueSequence
        );
        // Every non-zero digit is an issue sequence, not just '5' (the artifact
        // carries 000087, 000155, 000225, … across the range).
        for c in ['1', '2', '3', '4', '5', '6', '7', '8', '9'] {
            let code = format!("00008{c}");
            assert_eq!(
                classify_master_row(&code, "0", "1"),
                MasterRowVerdict::PreferredIssueSequence,
                "{code}"
            );
        }
        // An empty etfgubun is the other common-equity spelling (R2).
        assert_eq!(
            classify_master_row("035720", "", "2"),
            MasterRowVerdict::Keep(MarketClass::Kosdaq)
        );
    }

    /// The filter's precedence: the numeric guard is what makes the 6th-byte
    /// index safe, and an ETF is reported as an ETF rather than counted into
    /// `dropped_preferred` (which would overstate the preferred-share evidence).
    #[test]
    fn master_row_filter_precedence_is_stable() {
        // Letter-suffixed 신형우선주 — dropped by the numeric guard, and it must
        // not panic reaching for a 6th ASCII digit that is not there.
        assert_eq!(classify_master_row("02826K", "0", "1"), MasterRowVerdict::NotSixDigitNumeric);
        assert_eq!(classify_master_row("33626L", "0", "1"), MasterRowVerdict::NotSixDigitNumeric);
        // Multi-byte input must not panic on the byte index either.
        assert_eq!(classify_master_row("삼성전자", "0", "1"), MasterRowVerdict::NotSixDigitNumeric);
        // ETF / ETN classify as non-equity even when the 6th digit is non-zero.
        assert_eq!(classify_master_row("069500", "1", "1"), MasterRowVerdict::NotEquity);
        assert_eq!(classify_master_row("500055", "2", "1"), MasterRowVerdict::NotEquity);
        // Neither KOSPI nor KOSDAQ (e.g. KONEX) — outside the v1 identity.
        assert_eq!(classify_master_row("123450", "0", "3"), MasterRowVerdict::OutsideMarketClasses);
    }

    /// The recorded filter must describe what the code applies. Before P5 the
    /// string declared the numeric-coded residual an accepted limitation while
    /// the code let it through; that sentence is exactly what P5 retires.
    #[test]
    fn recorded_filter_declares_the_applied_preferred_rule() {
        assert!(INSTRUMENT_TYPE_FILTER.contains("issue-sequence digit"), "{INSTRUMENT_TYPE_FILTER}");
        assert!(INSTRUMENT_TYPE_FILTER.contains("005935"), "names the worked example");
        assert!(INSTRUMENT_TYPE_FILTER.contains("dropped_preferred"), "points at the evidence");
        // The surviving limitation is SPACs/REITs only — preferred shares are no
        // longer part of it.
        assert!(
            !INSTRUMENT_TYPE_FILTER.contains("numeric-coded preferred"),
            "the retired limitation must not survive: {INSTRUMENT_TYPE_FILTER}"
        );
    }

    #[test]
    fn budget_defer_is_honored() {
        // A measured budget of 100 with 95 already spent cannot fit a 47-call
        // capture that WOULD fit a cold window — the gate refuses (KTD6).
        let model = BudgetModel { budget_calls: Some(100), ..BudgetModel::default() };
        let mut ledger = SpendLedger::default();
        for _ in 0..95 {
            ledger.record_spend("cred", 1_000);
        }
        let err = budget_gate(&model, &ledger, "cred", 1_001, 47).unwrap_err();
        assert!(err.contains("47"), "names the estimate: {err}");
        assert!(err.contains("cold window"), "{err}");
        // A cold ledger proceeds.
        assert!(budget_gate(&model, &SpendLedger::default(), "cred", 1_001, 47).is_ok());
        // No measured budget → inert, always proceed.
        let inert = BudgetModel { budget_calls: None, ..BudgetModel::default() };
        assert!(budget_gate(&inert, &ledger, "cred", 1_001, 47).is_ok());
    }
}
