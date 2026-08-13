//! P3 offline integration: the windowed daily backfill against wiremock-served
//! `t8410` bodies, into a real `ParquetDataCatalog` (plan
//! `2026-08-13-001-feat-daily-catalog-2016-floor-pull`). No live calls.
//!
//! The mock gateway reproduces the **measured** P4 shape: a request whose range
//! spans more sessions than the server page cap comes back truncated to the
//! newest rows with a *clean* empty `cts_date` cursor. That is the whole reason
//! this mode exists — a wide-range pull looks like a successful completion
//! while silently dropping ten years — so every test here runs against a
//! gateway that lies exactly that way.
//!
//! Covers AE1 (a marked home refuses accumulate/range at zero gateway calls),
//! AE2 (a clean zero-row window degrades rather than completing), AE3 (a
//! cross-day resume over a rewritten basis restarts the symbol), AE4 (a
//! repeated cursor degrades with nothing appended), and R6's resume-without-
//! refetch, including the kill-between-append-and-save gap.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::backfill::{
    build_plan, build_report, load_manifest, manifest_pin, BackfillPlan,
};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{BarKind, CalendarGate, IngestConfig, Ingestor, DEFAULT_OVERLAP_DAYS};
use nautilus_ls::reference::pit_walk::{
    partition_windows, ListingOutcome, PitUniverseArtifact, SymbolRecord, WalkProvenance,
    ARTIFACT_SCHEMA_VERSION, MAX_SESSIONS_PER_WINDOW,
};
use nautilus_ls::reference::universe_metadata::{CapTier, MarketClass, MetadataPin};
use nautilus_model::identifiers::InstrumentId;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHART_PATH: &str = "/stock/chart";
const DAILY: &str = "1-DAY";
/// The measured server page cap (P4): a wide-range request serves at most this
/// many rows, newest-first, on a clean empty cursor.
const SERVED_ROW_CAP: usize = 501;

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

// ---------------------------------------------------------------------------
// Calendar fixture: every civil date in the range is a proven Trading Session,
// so the 450-session window bound maps to 450 civil days and the fixture range
// partitions into three windows.
// ---------------------------------------------------------------------------

use nautilus_ls_calendar::schema::{DayRow, DayStatus as CalDayStatus};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, KrxCalendar};

fn cal_as_of() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap()
}

const FLOOR: (i32, u32, u32) = (2024, 1, 1);
const ANCHOR: (i32, u32, u32) = (2026, 6, 30);

fn floor() -> NaiveDate {
    let (y, m, d) = FLOOR;
    ymd(y, m, d)
}

fn anchor() -> NaiveDate {
    let (y, m, d) = ANCHOR;
    ymd(y, m, d)
}

fn all_sessions_calendar() -> &'static KrxCalendar {
    use std::sync::OnceLock;
    static CAL: OnceLock<KrxCalendar> = OnceLock::new();
    CAL.get_or_init(|| {
        let template = {
            let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("nautilus-ls-calendar/fixtures/base_2010_2012.json");
            KrxCalendar::load_from_path(&p, cal_as_of())
                .expect("base fixture loads")
                .snapshot()
                .clone()
        };
        let mut snap = template;
        let from = ymd(2023, 1, 1);
        let through = ymd(2027, 12, 31);
        let mut rows = Vec::new();
        let mut d = from;
        while d <= through {
            rows.push(DayRow {
                date: d,
                status: CalDayStatus::TradingSession,
                decisive_evidence: Vec::new(),
                conflicting_evidence: Vec::new(),
                alerts: Vec::new(),
            });
            d = d.succ_opt().unwrap();
        }
        snap.rows = rows;
        snap.coverage.materialized_from = from;
        snap.coverage.materialized_through = through;
        snap.coverage.retrospectively_checked_through = through;
        snap.coverage.scheduled_closure_evaluated_through = through;
        snap.artifact_id = compute_artifact_id(&snap);
        snap.calendar_id = compute_calendar_id(&snap);
        KrxCalendar::from_snapshot(snap, cal_as_of()).expect("test calendar validates")
    })
}

fn all_sessions_gate() -> CalendarGate<'static> {
    CalendarGate::new(Some(all_sessions_calendar().as_of(cal_as_of()).unwrap()))
}

// ---------------------------------------------------------------------------
// Manifest fixture
// ---------------------------------------------------------------------------

const PRE_FLOOR: &str = "005930";
const LISTED: &str = "000660";
/// The post-floor listing's first served bar — inside the SECOND window, so its
/// plan drops window 1 whole and trims window 2 forward.
fn listed_first_served() -> NaiveDate {
    ymd(2025, 6, 2)
}

fn sym(shcode: &str, outcome: ListingOutcome) -> SymbolRecord {
    SymbolRecord {
        shcode: shcode.into(),
        market_class: MarketClass::Kospi,
        cap_tier: CapTier::Top,
        outcome,
        calls: 1,
        pages: 1,
    }
}

/// Write the manifest artifact and return its path.
fn write_manifest(dir: &Path, symbols: Vec<SymbolRecord>) -> String {
    let view = all_sessions_calendar().as_of(cal_as_of()).unwrap();
    let range =
        partition_windows(&view, floor(), anchor(), MAX_SESSIONS_PER_WINDOW).unwrap();
    assert_eq!(range.windows.len(), 3, "the fixture range spans three windows");
    let artifact = PitUniverseArtifact {
        schema_version: ARTIFACT_SCHEMA_VERSION,
        provenance: WalkProvenance {
            tr: "t8410".into(),
            probed_at: "2026-08-13T00:00:00Z".into(),
            anchor: anchor(),
            floor: floor(),
            source_artifact: "lab/config/universe.json".into(),
            source_content_hash: "test-capture-hash".into(),
            pace_ms: 1000,
            qrycnt: 900,
            windows: range.windows.clone(),
            proven_sessions: range.sessions.len(),
            unknown_days: range.unknown_days,
            calls_made: 4,
            dropped_preferred: Vec::new(),
            dropped_malformed: Vec::new(),
            restricted: false,
        },
        symbols,
        measurements: Vec::new(),
        failed: Vec::new(),
        derived: Some(nautilus_ls::reference::pit_walk::DerivedBlock {
            proven_sessions: range.sessions.len(),
            symbols_counted: 2,
            no_served_rows: Vec::new(),
            listed_count_min: 1,
            listed_count_median: 2,
            listed_count_max: 2,
            thresholds: Vec::new(),
            mean_participation: 1.0,
            full_participation_symbols: 1,
            max_observed_rows_per_page: SERVED_ROW_CAP,
            margin_note: "test".into(),
        }),
    };
    let p = dir.join("pit-universe-test.json");
    std::fs::write(&p, serde_json::to_string_pretty(&artifact).unwrap()).unwrap();
    p.display().to_string()
}

fn default_symbols() -> Vec<SymbolRecord> {
    vec![
        sym(PRE_FLOOR, ListingOutcome::PreFloor),
        sym(
            LISTED,
            ListingOutcome::Listed {
                first_served: listed_first_served(),
            },
        ),
    ]
}

fn plan_over(manifest_path: &str) -> BackfillPlan {
    let artifact = load_manifest(Path::new(manifest_path)).unwrap();
    let view = all_sessions_calendar().as_of(cal_as_of()).unwrap();
    build_plan(&view, &artifact, manifest_path).unwrap()
}

// ---------------------------------------------------------------------------
// Mock gateway
// ---------------------------------------------------------------------------

/// How the mock gateway behaves.
#[derive(Clone, Copy, PartialEq)]
enum Behavior {
    /// Serve the truncated newest slice of the requested range (the P4 shape).
    Truncating,
    /// Serve rows, then echo the SAME cursor back — suspect truncation (AE4).
    RepeatedCursor,
    /// Serve nothing at all, on a clean empty cursor (AE2's input).
    AlwaysEmpty,
    /// Serve normally except on the Nth t8410 call, which fails.
    FailOnCall(usize),
}

struct Gateway {
    /// Per-symbol ascending session dates (`YYYYMMDD`) the vendor serves.
    dates: Mutex<std::collections::BTreeMap<String, Vec<String>>>,
    /// Additive offset applied to every close — flipping it models the
    /// server-side rewrite a split/dividend performs on the adjusted series.
    basis: Mutex<i64>,
    behavior: Behavior,
    calls: AtomicUsize,
    /// Every `(shcode, sdate, edate)` the gateway was asked for.
    requested: Mutex<Vec<(String, String, String)>>,
}

impl Gateway {
    fn new(behavior: Behavior, dates: Vec<(&str, Vec<String>)>) -> Arc<Self> {
        Arc::new(Gateway {
            dates: Mutex::new(
                dates
                    .into_iter()
                    .map(|(s, d)| (s.to_string(), d))
                    .collect(),
            ),
            basis: Mutex::new(0),
            behavior,
            calls: AtomicUsize::new(0),
            requested: Mutex::new(Vec::new()),
        })
    }

    fn set_basis(&self, offset: i64) {
        *self.basis.lock().unwrap() = offset;
    }

    fn requests_for(&self, shcode: &str) -> Vec<(String, String)> {
        self.requested
            .lock()
            .unwrap()
            .iter()
            .filter(|(s, _, _)| s == shcode)
            .map(|(_, sd, ed)| (sd.clone(), ed.clone()))
            .collect()
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

fn close_for(date: &str, basis: i64) -> i64 {
    50_000 + (date.parse::<i64>().unwrap_or(0) % 977) + basis
}

async fn sdk_over(server: &MockServer, gw: Arc<Gateway>) -> LsSdk {
    mount_token(server).await;
    Mock::given(method("POST"))
        .and(path(CHART_PATH))
        .and(header("tr_cd", "t8410"))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            let ib = &body["t8410InBlock"];
            let shcode = ib["shcode"].as_str().unwrap_or("").to_string();
            let s = ib["sdate"].as_str().unwrap_or("").to_string();
            let e = ib["edate"].as_str().unwrap_or("").to_string();
            let n = gw.calls.fetch_add(1, Ordering::SeqCst) + 1;
            gw.requested.lock().unwrap().push((shcode.clone(), s.clone(), e.clone()));
            if let Behavior::FailOnCall(fail_at) = gw.behavior {
                if n == fail_at {
                    return json_response(serde_json::json!({
                        "rsp_cd": "IGW00001", "rsp_msg": "gateway failure"
                    }));
                }
            }
            let basis = *gw.basis.lock().unwrap();
            let mut in_range: Vec<String> = if gw.behavior == Behavior::AlwaysEmpty {
                Vec::new()
            } else {
                gw.dates
                    .lock()
                    .unwrap()
                    .get(&shcode)
                    .map(|ds| ds.iter().filter(|d| **d >= s && **d <= e).cloned().collect())
                    .unwrap_or_default()
            };
            // The measured truncation: only the NEWEST rows, on a cursor that
            // reads as clean completion.
            if in_range.len() > SERVED_ROW_CAP {
                in_range = in_range.split_off(in_range.len() - SERVED_ROW_CAP);
            }
            let rows: Vec<serde_json::Value> = in_range
                .iter()
                .rev() // LS serves newest-first
                .map(|d| {
                    let c = close_for(d, basis);
                    serde_json::json!({
                        "date": d, "open": (c - 5).to_string(), "high": (c + 10).to_string(),
                        "low": (c - 10).to_string(), "close": c.to_string(), "jdiff_vol": "1000"
                    })
                })
                .collect();
            // A repeated cursor echoes a live cursor that never changes.
            let cursor = if gw.behavior == Behavior::RepeatedCursor && !rows.is_empty() {
                "20250101"
            } else {
                ""
            };
            json_response(serde_json::json!({
                "rsp_cd": "00000", "rsp_msg": "정상",
                "t8410OutBlock": { "shcode": shcode, "cts_date": cursor, "rec_count": rows.len().to_string() },
                "t8410OutBlock1": rows
            }))
        })
        .mount(server)
        .await;
    LsSdk::new(mock_config(&server.uri())).expect("sdk builds")
}

/// Every civil date in `[from, through]` as `YYYYMMDD`.
fn dates(from: NaiveDate, through: NaiveDate) -> Vec<String> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= through {
        out.push(d.format("%Y%m%d").to_string());
        d = d.succ_opt().unwrap();
    }
    out
}

fn daily_config(catalog: &Path) -> IngestConfig {
    IngestConfig {
        catalog_path: catalog.to_path_buf(),
        bar_kinds: vec![BarKind::Daily],
        sdate: floor().format("%Y%m%d").to_string(),
        edate: anchor().format("%Y%m%d").to_string(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    }
}

fn instrument(shcode: &str) -> String {
    format!("{shcode}.{}", nautilus_ls::KRX_VENUE)
}

fn checkpoint_at(catalog: &Path) -> Checkpoint {
    Checkpoint::load(&catalog.join("ingest-checkpoint.json")).unwrap()
}

/// Stand in for the KTD5 bootstrap's `write_instruments` pass. A home with bars
/// but no instrument definitions makes every lab backtest read EMPTY with no
/// error, so the report treats their absence as an anomaly (R11) — which means
/// every GO fixture has to write them.
async fn write_instrument_defs(catalog: &Path, shcodes: &[String]) {
    use nautilus_model::instruments::{Equity, InstrumentAny};
    use nautilus_model::types::{Currency, Price, Quantity};
    let defs: Vec<InstrumentAny> = shcodes
        .iter()
        .map(|s| {
            let id = InstrumentId::from(instrument(s).as_str());
            InstrumentAny::Equity(Equity::new(
                id,
                id.symbol,
                None,
                Currency::KRW(),
                0,
                Price::from("1"),
                Some(Quantity::from("1")),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                0.into(),
                0.into(),
            ))
        })
        .collect();
    nautilus_ls::ingest::write_instruments(catalog, defs).await.unwrap();
}

async fn stored_dates(catalog: &Path, shcode: &str) -> Vec<NaiveDate> {
    let bar_type = BarKind::Daily
        .bar_type(InstrumentId::from(instrument(shcode).as_str()))
        .unwrap();
    let mut bars = nautilus_ls::ingest::read_bars_scoped(catalog, bar_type, None, None)
        .await
        .unwrap();
    bars.sort_by_key(|b| b.ts_event.as_u64());
    bars.iter()
        .map(|b| nautilus_ls::ingest::kst_date_of(b.ts_event))
        .collect()
}

async fn stored_closes(catalog: &Path, shcode: &str) -> Vec<i64> {
    let bar_type = BarKind::Daily
        .bar_type(InstrumentId::from(instrument(shcode).as_str()))
        .unwrap();
    let mut bars = nautilus_ls::ingest::read_bars_scoped(catalog, bar_type, None, None)
        .await
        .unwrap();
    bars.sort_by_key(|b| b.ts_event.as_u64());
    bars.iter().map(|b| b.close.to_string().parse().unwrap()).collect()
}

/// The full vendor dataset for the fixture manifest.
fn full_dates() -> Vec<(&'static str, Vec<String>)> {
    vec![
        (PRE_FLOOR, dates(floor(), anchor())),
        (LISTED, dates(listed_first_served(), anchor())),
    ]
}

// ---------------------------------------------------------------------------
// The happy path + resume (R6, and the Verification Contract's end-to-end)
// ---------------------------------------------------------------------------

/// A whole-manifest session pulls every window, restores the full history that
/// a wide-range pull would have truncated, and clears the marker.
#[tokio::test]
async fn a_full_session_pulls_every_window_and_clears_the_marker() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let report = ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();

    let expected_pre = dates(floor(), anchor()).len();
    let expected_listed = dates(listed_first_served(), anchor()).len();
    assert_eq!(report.bars_written, expected_pre + expected_listed);
    assert_eq!(report.symbols_complete, 2);
    assert!(report.degraded.is_empty(), "{:?}", report.degraded);
    assert!(report.marker_cleared, "every manifest symbol reached the anchor");

    // The whole history is present — not the newest ~501 rows a wide-range
    // pull would have served on a clean cursor.
    assert_eq!(stored_dates(&catalog, PRE_FLOOR).await.len(), expected_pre);
    assert!(expected_pre > SERVED_ROW_CAP, "the fixture must exceed the served cap");
    assert_eq!(stored_dates(&catalog, LISTED).await.len(), expected_listed);

    let cp = checkpoint_at(&catalog);
    assert!(!cp.backfill_incomplete(), "the marker cleared");
    for s in [PRE_FLOOR, LISTED] {
        assert_eq!(cp.watermark(&instrument(s), DAILY), Some(anchor()));
    }
    // No request spans more than the window bound.
    for (sd, ed) in gw.requests_for(PRE_FLOOR) {
        let sd = NaiveDate::parse_from_str(&sd, "%Y%m%d").unwrap();
        let ed = NaiveDate::parse_from_str(&ed, "%Y%m%d").unwrap();
        assert!((ed - sd).num_days() + 1 <= 450, "a wide-range request escaped: {sd}..{ed}");
        assert_ne!(sd, ed, "a degenerate single-day request is never emitted");
    }
}

/// A batch that leaves other members pending keeps the marker set — the marker
/// tracks the MANIFEST, never the batch.
#[tokio::test]
async fn a_partial_batch_keeps_the_marker_and_the_next_batch_clears_it() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
    let first = ing
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();
    assert_eq!(first.symbols_complete, 1);
    assert!(!first.marker_cleared, "one member is still pending");
    assert!(checkpoint_at(&catalog).backfill_incomplete());

    let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
    let second = ing2
        .run_backfill(&plan, &[LISTED.to_string()], ymd(2026, 7, 2))
        .await
        .unwrap();
    assert!(second.marker_cleared);
    assert!(!checkpoint_at(&catalog).backfill_incomplete());
}

/// R6 end-to-end: a session killed mid-symbol resumes from the watermark. No
/// completed window is re-fetched and no append is refused.
#[tokio::test]
async fn a_kill_between_windows_resumes_without_refetch_or_overlap() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    // Fail the second window: the symbol degrades with window 1 committed.
    let gw = Gateway::new(Behavior::FailOnCall(2), full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let first = ing
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();
    assert_eq!(first.windows_pulled, 1);
    assert_eq!(first.degraded.len(), 1);
    let windows = &plan.symbol(PRE_FLOOR).unwrap().windows;
    assert_eq!(
        checkpoint_at(&catalog).watermark(&instrument(PRE_FLOOR), DAILY),
        Some(windows[0].edate),
        "the watermark pins at the last COMPLETED window"
    );

    // Resume on a healthy gateway, SAME day (so no overlap check).
    let server2 = MockServer::start().await;
    let gw2 = Gateway::new(Behavior::Truncating, full_dates());
    let sdk2 = sdk_over(&server2, Arc::clone(&gw2)).await;
    let mut ing2 = Ingestor::new(sdk2, daily_config(&catalog));
    let second = ing2
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();

    assert!(second.append_refusals.is_empty(), "{:?}", second.append_refusals);
    assert!(second.degraded.is_empty(), "{:?}", second.degraded);
    assert_eq!(second.windows_pulled, windows.len() - 1, "only the remaining windows");
    for (sd, _) in gw2.requests_for(PRE_FLOOR) {
        assert_ne!(
            sd,
            windows[0].sdate.format("%Y%m%d").to_string(),
            "a completed window was re-fetched"
        );
    }
    assert_eq!(
        stored_dates(&catalog, PRE_FLOOR).await.len(),
        dates(floor(), anchor()).len()
    );
}

/// R6's hard case: the kill lands AFTER `append_bars_checked` succeeded but
/// BEFORE the checkpoint save. The parquet write and the checkpoint save cannot
/// be atomic together, so the resume must reconcile the watermark forward from
/// stored coverage — otherwise it re-fetches an already-appended window and
/// stalls forever on the overlap refusal.
#[tokio::test]
async fn a_kill_between_append_and_save_reconciles_instead_of_stalling() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
    ing.run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();

    // Rewind the checkpoint to simulate the kill: the bars for every window are
    // on disk, but the watermark still reads the FIRST window's edate.
    let windows = plan.symbol(PRE_FLOOR).unwrap().windows.clone();
    let cp_path = catalog.join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    cp.set_watermark(&instrument(PRE_FLOOR), DAILY, windows[0].edate);
    cp.set_backfill_incomplete(true);
    cp.save(&cp_path).unwrap();

    let before = gw.call_count();
    let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
    let report = ing2
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();

    assert!(
        report.append_refusals.is_empty(),
        "the stale watermark must not re-fetch an appended window: {:?}",
        report.append_refusals
    );
    assert!(report.degraded.is_empty(), "{:?}", report.degraded);
    assert_eq!(report.windows_pulled, 0, "nothing was re-fetched");
    assert_eq!(gw.call_count(), before, "zero gateway calls for a fully-covered symbol");
    assert_eq!(
        checkpoint_at(&catalog).watermark(&instrument(PRE_FLOOR), DAILY),
        Some(anchor()),
        "the watermark reconciled forward to the anchor"
    );
}

// ---------------------------------------------------------------------------
// Fail-closed arms (AE2, AE4)
// ---------------------------------------------------------------------------

/// AE4: a repeated cursor is suspect truncation. Nothing from the window is
/// appended, the watermark holds at the previous window, and the symbol
/// degrades — the run continues.
#[tokio::test]
async fn a_repeated_cursor_degrades_the_symbol_with_nothing_appended() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::RepeatedCursor, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let report = ing
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();

    assert_eq!(report.bars_written, 0, "nothing from a suspect window is appended");
    assert_eq!(report.windows_pulled, 0);
    assert_eq!(report.degraded.len(), 1);
    assert!(
        report.degraded[0].reason.contains("repeated cursor"),
        "{}",
        report.degraded[0].reason
    );
    let cp = checkpoint_at(&catalog);
    assert_eq!(cp.watermark(&instrument(PRE_FLOOR), DAILY), None, "no watermark advanced");
    assert!(cp.backfill_degraded(&instrument(PRE_FLOOR), DAILY).is_some());
    assert!(stored_dates(&catalog, PRE_FLOOR).await.is_empty());
}

/// AE2: a window that returns zero rows on a clean cursor is never completion.
/// It is re-fetched to the bound, then recorded as an uncovered gap; the
/// watermark stays below it and the run continues with the next symbol.
#[tokio::test]
async fn a_clean_zero_row_window_becomes_an_uncovered_gap_and_the_run_continues() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::AlwaysEmpty, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk, daily_config(&catalog)).with_empty_retry_max(2);
    let report = ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();

    assert_eq!(report.bars_written, 0);
    assert_eq!(report.uncovered_gaps.len(), 2, "one per symbol, loud");
    assert_eq!(report.degraded.len(), 2, "the run continued to the second symbol");
    assert!(!report.marker_cleared);
    let cp = checkpoint_at(&catalog);
    assert_eq!(
        cp.watermark(&instrument(PRE_FLOOR), DAILY),
        None,
        "the watermark never advances over an empty window"
    );
    // Bounded: 2 attempts per symbol, and the run stopped after the FIRST
    // window rather than walking the rest of the plan.
    assert_eq!(gw.requests_for(PRE_FLOOR).len(), 2);
}

// ---------------------------------------------------------------------------
// AE1 — the marker forbids every other bar-writing mode
// ---------------------------------------------------------------------------

/// AE1: `accumulate` and `range` both refuse a mid-backfill home at ZERO
/// gateway calls. Either one would issue a wide daily range that the gateway
/// serves as the newest ~501 rows on a clean cursor, attesting a multi-year
/// hole.
#[tokio::test]
async fn accumulate_and_range_refuse_a_mid_backfill_home_with_zero_fetches() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::FailOnCall(2), full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    // A partial backfill leaves the marker set.
    let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
    ing.run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();
    assert!(checkpoint_at(&catalog).backfill_incomplete());

    let universe = [InstrumentId::from(instrument(PRE_FLOOR).as_str())];
    let before = gw.call_count();

    let mut acc = Ingestor::new(sdk.clone(), daily_config(&catalog));
    let err = acc
        .run_accumulate_gated(&universe, anchor(), floor(), all_sessions_gate())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("BACKFILL INCOMPLETE"), "{err}");
    assert!(err.to_string().contains("accumulate"), "{err}");

    let mut rng = Ingestor::new(sdk.clone(), daily_config(&catalog));
    let err = rng.run(&universe).await.unwrap_err();
    assert!(err.to_string().contains("BACKFILL INCOMPLETE"), "{err}");
    assert!(err.to_string().contains("range"), "{err}");

    let mut reb = Ingestor::new(sdk, daily_config(&catalog));
    let err = reb
        .run_rebase_gated(&universe, anchor(), floor(), all_sessions_gate())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("BACKFILL INCOMPLETE"), "{err}");
    assert!(err.to_string().contains("rebase"), "{err}");

    assert_eq!(gw.call_count(), before, "not one gateway call was dispatched");
    // The rebase refusal must land BEFORE the durable mark-all boundary.
    assert!(
        !checkpoint_at(&catalog).is_shifted(&instrument(PRE_FLOOR), DAILY),
        "a refused rebase must not have marked anything shifted"
    );
}

// ---------------------------------------------------------------------------
// AE3 — the cross-day mid-symbol resume
// ---------------------------------------------------------------------------

/// The overlap check's request shape: it ends AT the watermark and spans far
/// fewer days than any window.
fn overlap_requests(gw: &Gateway, shcode: &str, watermark: NaiveDate) -> usize {
    let wm = watermark.format("%Y%m%d").to_string();
    gw.requests_for(shcode)
        .into_iter()
        .filter(|(sd, ed)| {
            let sd = NaiveDate::parse_from_str(sd, "%Y%m%d").unwrap();
            let ed = NaiveDate::parse_from_str(ed, "%Y%m%d").unwrap();
            *ed.format("%Y%m%d").to_string() == wm && (ed - sd).num_days() < 60
        })
        .count()
}

/// Drive one symbol to a mid-plan watermark stamped on `day`, and return the
/// windows plus the resulting watermark.
async fn partial_pull(
    catalog: &Path,
    plan: &BackfillPlan,
    day: NaiveDate,
) -> NaiveDate {
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::FailOnCall(2), full_dates());
    let sdk = sdk_over(&server, gw).await;
    let mut ing = Ingestor::new(sdk, daily_config(catalog));
    ing.run_backfill(plan, &[PRE_FLOOR.to_string()], day).await.unwrap();
    let wm = checkpoint_at(catalog)
        .watermark(&instrument(PRE_FLOOR), DAILY)
        .expect("window 1 committed");
    assert_eq!(wm, plan.symbol(PRE_FLOOR).unwrap().windows[0].edate);
    wm
}

/// AE3: a run killed mid-symbol on one day and resumed days later, after the
/// vendor rewrote the adjusted series, detects the basis shift and restarts the
/// symbol from its range start rather than splicing two adjustment bases into
/// one series.
#[tokio::test]
async fn a_cross_day_resume_over_a_rewritten_basis_wipes_and_restarts() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let wm = partial_pull(&catalog, &plan, ymd(2026, 7, 1)).await;

    // A corporate action rewrote the whole adjusted series server-side.
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    gw.set_basis(-10_000);
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let report = ing
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 6))
        .await
        .unwrap();

    assert_eq!(report.restarted, vec![PRE_FLOOR.to_string()]);
    assert_eq!(report.symbols_complete, 1);
    assert_eq!(overlap_requests(&gw, PRE_FLOOR, wm), 1, "the cross-day check ran once");
    // EVERY stored bar is on the new basis — no splice.
    let closes = stored_closes(&catalog, PRE_FLOOR).await;
    let all = dates(floor(), anchor());
    assert_eq!(closes.len(), all.len());
    let expected: Vec<i64> = all.iter().map(|d| close_for(d, -10_000)).collect();
    assert_eq!(closes, expected, "the series is on ONE adjustment basis");
}

/// A cross-day resume whose overlap is CLEAN continues mid-symbol: the check
/// runs, finds nothing, and the completed windows are neither wiped nor
/// re-fetched.
#[tokio::test]
async fn a_clean_cross_day_overlap_continues_mid_symbol() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let wm = partial_pull(&catalog, &plan, ymd(2026, 7, 1)).await;

    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let report = ing
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 6))
        .await
        .unwrap();

    assert!(report.restarted.is_empty(), "a clean overlap never wipes");
    assert_eq!(overlap_requests(&gw, PRE_FLOOR, wm), 1, "the check still ran");
    let windows = &plan.symbol(PRE_FLOOR).unwrap().windows;
    assert_eq!(report.windows_pulled, windows.len() - 1, "only the remaining windows");
    assert_eq!(
        stored_dates(&catalog, PRE_FLOOR).await.len(),
        dates(floor(), anchor()).len()
    );
}

/// A SAME-day mid-symbol resume runs no overlap check at all — the vendor
/// cannot have rewritten the series between two windows of one session, and the
/// check would cost a gateway call per symbol for nothing.
#[tokio::test]
async fn a_same_day_resume_runs_no_overlap_check() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let wm = partial_pull(&catalog, &plan, ymd(2026, 7, 1)).await;

    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let report = ing
        .run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();

    assert!(report.restarted.is_empty());
    assert_eq!(overlap_requests(&gw, PRE_FLOOR, wm), 0, "no overlap check on a same-day resume");
    assert_eq!(report.windows_pulled, plan.symbol(PRE_FLOOR).unwrap().windows.len() - 1);
}

// ---------------------------------------------------------------------------
// The advisory lock (operational guard)
// ---------------------------------------------------------------------------

/// A stale/held ingest lock refuses the run at startup and names the lock path,
/// so a concurrent morning-chain ingest cannot share the per-credential
/// `IGW00201` budget with a backfill session.
#[tokio::test]
async fn a_held_lock_refuses_the_run_and_names_the_lock_path() {
    use nautilus_ls::lock::{AdvisoryLock, LockKind};
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    let _live = AdvisoryLock::acquire(&catalog, LockKind::Live).unwrap();
    let err = AdvisoryLock::acquire(&catalog, LockKind::Ingest).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains(&catalog.display().to_string()), "the refusal names the path: {msg}");
}

// ---------------------------------------------------------------------------
// U5 — the completeness report and pin
// ---------------------------------------------------------------------------

async fn report_over(catalog: &Path, plan: &BackfillPlan) -> nautilus_ls::ingest::backfill::BackfillReport {
    build_report(catalog, plan, &checkpoint_at(catalog), "2026-07-01T00:00:00Z".into())
        .await
        .unwrap()
}

/// A complete home reads GO with zero anomalies, and the manifest pin is
/// written from that refusal-free state only.
#[tokio::test]
async fn a_complete_home_reads_go_and_pins_the_manifest() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, Gateway::new(Behavior::Truncating, full_dates())).await;
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();
    write_instrument_defs(&catalog, &plan.shcodes()).await;

    let report = report_over(&catalog, &plan).await;
    assert!(report.go, "anomalies: {:?}", report.all_anomalies());
    assert!(report.all_anomalies().is_empty());
    assert_eq!(report.provenance.manifest_hash, plan.manifest_hash);
    assert!(!report.provenance.backfill_incomplete);

    // A post-floor listing is measured against ITS OWN front, so it is not
    // flagged as front truncation — the failure mode the uniform
    // `expected_range` form of `catalog status` produces on all 108 of them.
    let listed = report.symbols.iter().find(|s| s.shcode == LISTED).unwrap();
    assert_eq!(listed.expected_front, listed_first_served());
    assert_eq!(listed.front, Some(listed_first_served()));
    assert!(listed.anomalies.is_empty(), "{:?}", listed.anomalies);
    assert!(listed.expected_sessions < report.provenance.proven_sessions);

    manifest_pin(&plan, "2026-07-01T00:00:00Z".into())
        .write(&catalog)
        .unwrap();
    let pin = MetadataPin::load(&catalog).unwrap().expect("pin written");
    assert_eq!(pin.content_hash, plan.manifest_hash);
    assert_eq!(pin.symbols, plan.shcodes());
}

/// One symbol short at the tail is an anomaly, GO is withheld, and no pin is
/// written — a pin from a non-clean state would attest a membership whose bars
/// never fully landed.
#[tokio::test]
async fn a_symbol_short_at_the_tail_withholds_go_and_the_pin() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    // The listed symbol stops serving before the anchor.
    let short = vec![
        (PRE_FLOOR, dates(floor(), anchor())),
        (LISTED, dates(listed_first_served(), ymd(2026, 6, 1))),
    ];
    let sdk = sdk_over(&server, Gateway::new(Behavior::Truncating, short)).await;
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();

    let report = report_over(&catalog, &plan).await;
    assert!(!report.go);
    let listed = report.symbols.iter().find(|s| s.shcode == LISTED).unwrap();
    assert!(
        listed.anomalies.iter().any(|a| a.contains("coverage tail")),
        "{:?}",
        listed.anomalies
    );
    assert!(
        listed.anomalies.iter().any(|a| a.contains("session shortfall")),
        "{:?}",
        listed.anomalies
    );
    // The clean symbol is still clean — anomalies are per symbol, enumerated.
    let pre = report.symbols.iter().find(|s| s.shcode == PRE_FLOOR).unwrap();
    assert!(pre.anomalies.is_empty(), "{:?}", pre.anomalies);
    assert!(MetadataPin::load(&catalog).unwrap().is_none(), "no pin on a NO-GO");
}

/// A degraded symbol recorded in the checkpoint is surfaced by the report —
/// the short coverage reads as a known degradation, not a mystery.
#[tokio::test]
async fn a_degraded_symbol_is_surfaced_by_the_report() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, Gateway::new(Behavior::RepeatedCursor, full_dates())).await;
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let run = ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();
    assert_eq!(run.degraded.len(), 2);

    let report = report_over(&catalog, &plan).await;
    assert!(!report.go);
    for s in &report.symbols {
        assert!(
            s.anomalies.iter().any(|a| a.starts_with("degraded:")),
            "{}: {:?}",
            s.shcode,
            s.anomalies
        );
    }
    assert!(
        report.anomalies.iter().any(|a| a.contains("incomplete-backfill marker")),
        "the standing marker is a run-level anomaly: {:?}",
        report.anomalies
    );
}

/// The evidence record round-trips through JSON — it is the committed, durable
/// form (`data/` is gitignored, so the catalog itself is not evidence).
#[tokio::test]
async fn the_evidence_record_round_trips() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, Gateway::new(Behavior::Truncating, full_dates())).await;
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();
    write_instrument_defs(&catalog, &plan.shcodes()).await;

    let report = report_over(&catalog, &plan).await;
    let out = dir.path().join("evidence/daily-catalog.json");
    report.write(&out).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    let back: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(back["go"], serde_json::Value::Bool(true));
    assert_eq!(back["provenance"]["manifest_hash"], plan.manifest_hash);
    assert_eq!(back["symbols"].as_array().unwrap().len(), 2);
}

/// R11: a home whose bars landed but whose instrument definitions never did is
/// NOT a GO. The lab would read it as an empty backtest with no error anywhere
/// — a silent failure the completeness proof has to catch, not the backtest.
#[tokio::test]
async fn a_home_with_no_instrument_definitions_is_not_a_go() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, Gateway::new(Behavior::Truncating, full_dates())).await;
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();

    // Bars are complete; the bootstrap never ran.
    let report = report_over(&catalog, &plan).await;
    assert!(report.symbols.iter().all(|s| s.anomalies.is_empty()), "the BARS are complete");
    assert!(!report.go);
    assert!(
        report.anomalies.iter().any(|a| a.contains("no instrument definitions")),
        "{:?}",
        report.anomalies
    );

    // Bootstrapping them flips the verdict.
    write_instrument_defs(&catalog, &plan.shcodes()).await;
    assert!(report_over(&catalog, &plan).await.go);
}

/// A backfill SEEDS a fresh home. Pointed at a catalog that already holds daily
/// bars from somewhere else, it refuses at the door: R6's watermark
/// reconciliation would otherwise map that foreign coverage onto this plan's
/// windows and skip every window below it — the same silent hole a wide-range
/// pull produces, arrived at from the other direction.
#[tokio::test]
async fn backfill_refuses_a_home_that_already_holds_foreign_daily_coverage() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);

    // Foreign coverage: a shallow recent slice, as a `range`-mode seed of the
    // OTHER lineage's home would leave.
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;
    let mut seed = Ingestor::new(sdk.clone(), daily_config(&catalog));
    seed.run_backfill(&plan, &[PRE_FLOOR.to_string()], ymd(2026, 7, 1))
        .await
        .unwrap();
    // Erase every trace of the backfill from the checkpoint, keeping the bars —
    // i.e. bars whose provenance is not a backfill session.
    let cp_path = catalog.join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    cp.set_backfill_incomplete(false);
    cp.clear_watermark(&instrument(PRE_FLOOR), DAILY);
    cp.save(&cp_path).unwrap();

    let before = gw.call_count();
    let mut ing = Ingestor::new(sdk, daily_config(&catalog));
    let err = ing
        .run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 2))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("fresh home"), "{err}");
    assert_eq!(gw.call_count(), before, "the refusal costs zero gateway calls");
    assert!(
        !checkpoint_at(&catalog).backfill_incomplete(),
        "a refused run must not leave the marker behind"
    );
}

/// Re-running a backfill against a COMPLETED home is a no-op, not a refusal —
/// every watermark is already at the anchor, so the fresh-home guard is exempt
/// there and the session makes zero gateway calls.
#[tokio::test]
async fn backfill_on_a_completed_home_is_a_no_op() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let manifest = write_manifest(dir.path(), default_symbols());
    let plan = plan_over(&manifest);
    let server = MockServer::start().await;
    let gw = Gateway::new(Behavior::Truncating, full_dates());
    let sdk = sdk_over(&server, Arc::clone(&gw)).await;

    let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
    ing.run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 1)).await.unwrap();
    assert!(!checkpoint_at(&catalog).backfill_incomplete());

    let before = gw.call_count();
    let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
    let report = ing2
        .run_backfill(&plan, &plan.shcodes(), ymd(2026, 7, 2))
        .await
        .unwrap();
    assert_eq!(report.symbols_skipped_complete, 2);
    assert_eq!(gw.call_count(), before, "a completed home costs nothing to re-run");
}
