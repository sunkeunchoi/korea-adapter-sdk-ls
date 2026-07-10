//! Reference-data live capture (U2): fetch the six captured TRs via SDK
//! facades, join by `shcode` into [`InstrumentMetadata`] records, and assemble
//! the [`UniverseMetadata`] artifact.
//!
//! Sources (R1–R3):
//! - skeleton: `t8430` master (market class; equities-only — non-empty
//!   `etfgubun` rows dropped, R2), `t2522` (single-stock-futures underlying
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
use ls_sdk::paginated::{T1404Request, T1405Request};
use ls_sdk::LsSdk;

use crate::ingest::budget::{plan_dispatch, BudgetDecision, BudgetModel, SpendLedger};
use crate::reference::universe_metadata::{
    assign_cap_tiers, assign_liquidity_tier, is_tradable, CapCutoff, Designation,
    DesignationKind, InstrumentMetadata, MarketClass, MetadataProvenance, Resolved, TrFailure,
    UniverseMetadata, DEFAULT_CAP_TOP_QUANTILE, IndexMembership,
};

/// The TR set the capture joins (provenance `source_trs`).
pub const SOURCE_TRS: [&str; 6] = ["t8430", "t2522", "t1904", "t1444", "t1405", "t1404"];

/// The recorded instrument-type filter (R2): the skeleton is equities-only.
pub const INSTRUMENT_TYPE_FILTER: &str =
    "t8430 etfgubun in {empty, '0'} (equities-only; ETF '1' / ETN '2' rows dropped before \
     tiering — the live master serves '0' for common equities). t8430 exposes no flag for \
     preferred shares/SPACs/REITs — residual non-common-stock pollution in the exclusion \
     stratum is an accepted, documented limitation.";

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
}

impl CaptureConfig {
    /// The default capture over both markets for one session.
    pub fn new(captured_at: impl Into<String>, session_date: impl Into<String>) -> Self {
        CaptureConfig {
            captured_at: captured_at.into(),
            session_date: session_date.into(),
            cap_boards: vec![
                CapBoard { upcode: "001".into(), market_class: MarketClass::Kospi, max_rows: 400 },
                CapBoard { upcode: "301".into(), market_class: MarketClass::Kosdaq, max_rows: 400 },
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
            pace: Duration::from_millis(600),
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
pub async fn capture(sdk: &LsSdk, cfg: &CaptureConfig) -> Result<CaptureOutcome, String> {
    // Refuse the whole-board pseudo-category (pre-flight finding, 2026-07-10):
    // `jongchk = "0"` on t1404 returns every listed issue, designated or not —
    // treating those rows as designations would mark the entire market
    // non-tradable. Fail closed before any gateway call.
    for (tr, qs) in [("t1405", &cfg.t1405_categories), ("t1404", &cfg.t1404_categories)] {
        if let Some(q) = qs.iter().find(|q| q.jongchk.trim() == "0") {
            return Err(format!(
                "{tr} designation query with jongchk=\"0\" (gubun={}) is the whole board, not a \
                 category — it would designate every symbol; query categories individually",
                q.gubun
            ));
        }
    }
    let mut failures: Vec<TrFailure> = Vec::new();
    let mut calls_made: u32 = 0;

    // --- Skeleton: t8430 master (market class + equities-only filter, R1/R2).
    calls_made += 1;
    let master = sdk
        .market_session()
        .stock_issues(&T8430Request::all())
        .await
        .map_err(|e| format!("t8430 master failed — no skeleton, no capture: {}", fail_code(&e)))?;
    let mut skeleton: Vec<(String, MarketClass)> = Vec::new();
    for row in &master.outblock {
        let shcode = row.shcode.trim().to_string();
        if shcode.is_empty() {
            continue;
        }
        // Equities-only (R2): an ETF/ETN etfgubun ("1" ETF / "2" ETN) drops the
        // row. The live master serves "0" (not empty) for common equities — the
        // adapter's instrument mapper reads the same convention
        // (`instruments.rs`: `etfgubun == "1"` → ETF).
        if !matches!(row.etfgubun.trim(), "" | "0") {
            continue;
        }
        let market_class = match row.gubun.trim() {
            "1" => MarketClass::Kospi,
            "2" => MarketClass::Kosdaq,
            _ => continue, // neither KOSPI nor KOSDAQ — outside the v1 identity
        };
        skeleton.push((shcode, market_class));
    }
    if skeleton.is_empty() {
        return Err("t8430 returned no equity rows — cannot capture an empty skeleton".into());
    }

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
        // The walk issues up to max_rows/20 pages; count them conservatively.
        calls_made += board.max_rows.div_ceil(20) as u32;
        match sdk.paginated().market_cap_top_all(board.upcode.clone(), board.max_rows).await {
            Ok(rows) => {
                boards_served += 1;
                for row in &rows {
                    let code = row.shcode.trim().to_string();
                    if code.is_empty() {
                        continue;
                    }
                    if let Ok(total) = row.total.trim().parse::<f64>() {
                        cap_by_shcode.entry(code).or_insert(total);
                    }
                }
            }
            Err(e) => {
                failures.push(TrFailure { tr: "t1444".into(), code: fail_code(&e) });
            }
        }
    }
    if boards_served == 0 {
        return Err(format!(
            "every t1444 cap board failed — no cap axis, no stratification (failures: {failures:?})"
        ));
    }

    // --- Gate: t1405 + t1404 designation categories (R3).
    let mut designation_by_shcode: HashMap<String, Designation> = HashMap::new();
    for q in &cfg.t1405_categories {
        tokio::time::sleep(cfg.pace).await;
        match walk_t1405(sdk, q).await {
            Ok((codes, pages)) => {
                calls_made += pages;
                for code in codes {
                    designation_by_shcode
                        .entry(code)
                        .or_insert(Designation { kind: q.kind, source_tr: "t1405".into() });
                }
            }
            Err(code) => {
                calls_made += 1;
                failures.push(TrFailure { tr: "t1405".into(), code });
            }
        }
    }
    for q in &cfg.t1404_categories {
        tokio::time::sleep(cfg.pace).await;
        match walk_t1404(sdk, q).await {
            Ok((codes, pages)) => {
                calls_made += pages;
                for code in codes {
                    designation_by_shcode
                        .entry(code)
                        .or_insert(Designation { kind: q.kind, source_tr: "t1404".into() });
                }
            }
            Err(code) => {
                calls_made += 1;
                failures.push(TrFailure { tr: "t1404".into(), code });
            }
        }
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
        },
        records,
    };
    // Fail closed BEFORE the caller writes: an artifact that would not pass the
    // offline validator must never land on disk (the capture-universe pattern).
    if let Err(errs) = artifact.validate() {
        return Err(format!("captured artifact failed validation:\n  - {}", errs.join("\n  - ")));
    }
    Ok(CaptureOutcome { artifact, calls_made })
}

/// Walk one `t1405` category across its `cts_shcode` pages. Returns the
/// designated shcodes and the pages spent. The `T1405Response` carries no
/// `tr_cont` header fields (a single-page read) — continuation threads the
/// **body** cursor plus a request-side `tr_cont: Y`; the walk terminates on an
/// empty/repeated cursor, a no-progress page, or the page cap.
async fn walk_t1405(sdk: &LsSdk, q: &DesignationQuery) -> Result<(Vec<String>, u32), String> {
    let mut req = T1405Request::new(q.gubun.clone(), q.jongchk.clone());
    let mut seen: HashSet<String> = HashSet::new();
    let mut codes = Vec::new();
    let mut pages: u32 = 0;
    loop {
        pages += 1;
        let resp = sdk.paginated().trade_suspension(&req).await.map_err(|e| fail_code(&e))?;
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
async fn walk_t1404(sdk: &LsSdk, q: &DesignationQuery) -> Result<(Vec<String>, u32), String> {
    let mut req = T1404Request::new();
    req.inblock.gubun = q.gubun.clone();
    req.inblock.jongchk = q.jongchk.clone();
    let mut seen: HashSet<String> = HashSet::new();
    let mut codes = Vec::new();
    let mut pages: u32 = 0;
    loop {
        pages += 1;
        let resp = sdk.paginated().designation_board(&req).await.map_err(|e| fail_code(&e))?;
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
        // 1 t8430 + 1 t2522 + 2 t1904 + 2 boards x ceil(400/20)=20 + 7 + 4 categories.
        assert_eq!(estimated_capture_calls(&cfg), (1 + 1 + 2 + 40 + 7 + 4) as u32);
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
