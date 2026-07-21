//! `ls-ingest` — the historical-bar backfill entry point (U3).
//!
//! Paper-only, operator-run. It resolves LS credentials from a lane env-file (or
//! the process env), loads the domestic-equity universe, writes the instrument
//! definitions + bars into a `ParquetDataCatalog`, and holds the R15 advisory lock
//! for the duration (refusing to start while a live session is running).
//!
//! Configuration (env vars):
//! - `LS_INGEST_CATALOG`: catalog directory (required).
//! - `LS_INGEST_MODE`: `range` (default) | `accumulate` (U5) | `rebase` (epoch
//!   re-base — see the README runbook) | `probe-lookback`. In `accumulate`/`rebase`
//!   modes, `SDATE`/`EDATE` are ignored; coverage grows from each instrument's
//!   watermark to the last closed session. `rebase` first marks every daily triple
//!   shifted (one atomic checkpoint save), then heals each through the same path.
//! - `LS_INGEST_SDATE` / `LS_INGEST_EDATE`: range bounds `YYYYMMDD` (required in
//!   `range` mode).
//! - `LS_INGEST_LOOKBACK`: accumulate/rebase-mode floor `YYYYMMDD` for an
//!   unseen/newly listed instrument — and, in `rebase` mode, the re-pull floor
//!   for every symbol (required; pin at or before the original backfill start).
//! - `LS_INGEST_LANE_FILE`: optional lane env-file (else the process env is used).
//! - `LS_INGEST_SYMBOLS`: optional comma-separated shcodes to bound the universe
//!   (else the whole loaded universe; minute backfills MUST be bounded).
//! - `LS_INGEST_KIND`: `daily` (default) | `minute:<n>` | `daily,minute:<n>`.
//! - `LS_INGEST_SKIP_UNIVERSE_LOAD`: `1`/`true` to skip the per-invocation universe
//!   load (`t8430` + 2× `t9945`) and the `write_instruments` re-snapshot — the
//!   dominant avoidable IGW00201 cost in a per-symbol drip loop. REQUIRES an
//!   explicit symbol list (`LS_INGEST_SYMBOLS` or the metadata selection) and a
//!   non-empty catalog (a prior full-universe pass must have persisted the
//!   instrument defs); refuses otherwise.
//! - `LS_INGEST_METADATA`: optional `UniverseMetadata` artifact path (U3, plan
//!   2026-07-10-003). When set, the ingest symbol set is a tier-stratified sample
//!   drawn via `reference::stratify` (R6) and the artifact's content hash is
//!   pinned into `<catalog>/universe-metadata-pin.json` (KTD2). Mutually
//!   exclusive with `LS_INGEST_SYMBOLS`.
//! - `LS_INGEST_PER_STRATUM`: symbols per stratum for the stratified sample
//!   (default `5`; four strata → ≤ 4× this many symbols).
//! - `LS_INGEST_STRATIFY_DRY_RUN`: `1`/`true` to print the stratified selection
//!   with per-stratum counts (the floor-reachability pre-check input) and exit
//!   without touching the gateway or the catalog.

use std::path::PathBuf;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::ingest::{
    last_closed_session, BarKind, CoverageReport, IngestConfig, Ingestor, ACCUMULATE_CLOSE_BUFFER,
    DEFAULT_OVERLAP_DAYS,
};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::reference::universe_metadata::{
    stratify, MetadataPin, Stratum, UniverseMetadata,
};
use nautilus_ls::scrub;
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};

/// The exit code for a run that carried a genuine per-triple refusal (#104): a
/// range/heal/append refusal means a triple stalled and an operator must act.
/// Distinct from the hard-error `1` the `Err` path returns (`ExitCode::FAILURE`),
/// so CI can tell "the run itself failed" from "the run completed but N triples
/// were refused".
const EXIT_REFUSALS: u8 = 2;

/// The process exit code implied by a completed run's [`CoverageReport`] (#104,
/// R8/R9): nonzero when any *genuine refusal* vec (range, heal, or append overlap)
/// is non-empty, zero otherwise. Backward-widen warnings are informational and
/// never consulted (R9) — a late-listed symbol warns forever without reddening CI.
fn exit_code_for(report: &CoverageReport) -> u8 {
    let refused = !report.range_refusals.is_empty()
        || !report.heal_refusals.is_empty()
        || !report.append_refusals.is_empty();
    if refused {
        EXIT_REFUSALS
    } else {
        0
    }
}

/// Backfill-floor admission (#189 U6 follow-up). The startup admission validates only the
/// target date, but an automatic run also backfills fresh instruments FROM the
/// `LS_INGEST_LOOKBACK` floor. Under Enforced-only, a floor the frozen calendar cannot cover
/// makes every fresh triple's established prefix stop before it starts (`start <
/// materialized_from` → `stop_before`), so those triples skip with zero bars, the run still
/// exits 0, and — worst — a metadata pin is written attesting a selection whose bars never
/// landed. Fail closed here exactly as the target admission does, rather than skipping silently.
/// `coverage` is `None` only when no calendar view is present, which the target admission
/// (`EnforcedFailClosed`) already fails closed on for automatic modes — so a missing view is not
/// this check's concern and passes through `Ok`.
fn floor_admission(floor: NaiveDate, coverage: Option<(NaiveDate, NaiveDate)>) -> Result<(), String> {
    if let Some((from, through)) = coverage {
        if floor < from || floor > through {
            return Err(format!(
                "backfill floor {} is outside the frozen calendar coverage [{}, {}] — a fresh instrument could not be attested from it (its prefix would skip with zero bars and the run would mis-pin). Widen the calendar snapshot or raise LS_INGEST_LOOKBACK.",
                floor.format("%Y%m%d"),
                from.format("%Y%m%d"),
                through.format("%Y%m%d"),
            ));
        }
    }
    Ok(())
}

fn backward_widen_uncertainty_line(
    uncertainty: &nautilus_ls::ingest::BackwardWidenUncertainty,
) -> String {
    format!(
        "BACKWARD WIDEN UNCERTAIN: {} {} — lookback floor {} precedes earliest stored coverage {}; the interval contains Unknown/unavailable calendar evidence (calendar_stale={}) and no marker was persisted. Resolve calendar evidence and re-run.",
        uncertainty.instrument,
        uncertainty.bar_type,
        uncertainty.floor,
        uncertainty.earliest_stored,
        uncertainty.calendar_stale,
    )
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Credential hygiene before any output (mirrors the repo's smoke convention).
    scrub::install();
    // Scrub the terminal error too — a `?`-propagated SDK error would otherwise be
    // printed unscrubbed by the runtime, leaking a raw broker message.
    match run().await {
        // Probe mode carries no coverage report — nothing to refuse, exit 0.
        Ok(None) => std::process::ExitCode::SUCCESS,
        // A completed run: exit nonzero iff it carried a genuine refusal (#104).
        Ok(Some(report)) => std::process::ExitCode::from(exit_code_for(&report)),
        Err(e) => {
            eprintln!("error: {}", scrub::scrub_secrets(&e.to_string()));
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<Option<CoverageReport>, Box<dyn std::error::Error>> {
    require_paper()?;

    let catalog: PathBuf = env_required("LS_INGEST_CATALOG")?.into();
    let mode = std::env::var("LS_INGEST_MODE").unwrap_or_else(|_| "range".into());
    // `accumulate` and `rebase` share the watermark/floor arithmetic; rebase
    // additionally marks every daily triple shifted first (the epoch re-base,
    // KTD-4 — see the README runbook before running it).
    let accumulate = match mode.as_str() {
        "range" => false,
        "accumulate" | "rebase" => true,
        "probe-lookback" => false, // handled early, below
        other => {
            return Err(format!(
                "unknown LS_INGEST_MODE {other:?} (want range | accumulate | rebase | probe-lookback)"
            )
            .into())
        }
    };
    let bar_kinds = parse_kinds(&std::env::var("LS_INGEST_KIND").unwrap_or_else(|_| "daily".into()))?;

    // U3 (plan 2026-07-10-003): tier-stratified symbol selection from the
    // metadata artifact (R6). Resolved before the lock and the client so the
    // dry-run (the floor-reachability pre-check input) stays fully offline.
    let symbols_env = std::env::var("LS_INGEST_SYMBOLS").ok().filter(|s| !s.trim().is_empty());
    let metadata_selection = resolve_metadata_selection()?;
    if let Some(sel) = &metadata_selection {
        if symbols_env.is_some() {
            return Err(
                "both LS_INGEST_METADATA and LS_INGEST_SYMBOLS are set — the stratified \
                 selection and an explicit symbol list are mutually exclusive"
                    .into(),
            );
        }
        for (label, count) in &sel.per_stratum {
            println!("stratum {label}: {count} symbols");
        }
        println!(
            "stratified selection: {} symbols from {} (hash {})",
            sel.symbols.len(),
            sel.artifact_path,
            sel.content_hash
        );
        println!("LS_INGEST_SYMBOLS={}", sel.symbols.join(","));
    }
    if env_flag("LS_INGEST_STRATIFY_DRY_RUN") {
        if metadata_selection.is_none() {
            return Err("LS_INGEST_STRATIFY_DRY_RUN requires LS_INGEST_METADATA".into());
        }
        println!("dry run: no gateway calls, no catalog writes, no pin");
        return Ok(None);
    }

    // Take the R15 ingest lock FIRST — before any gateway request — so a live
    // session holding the counterpart lock blocks us before we issue the universe
    // load (t8430 + 2x t9945) against the shared per-process rate buckets.
    let _lock = AdvisoryLock::acquire(&catalog, LockKind::Ingest)?;

    // Freeze one invocation clock, path, and loaded snapshot after the lock wait and before
    // constructing anything gateway-capable. Enforced-only after the ingest Consumer Retirement
    // Gate (#189 U6): the date decision no longer consults LS_CALENDAR_ADOPTION — accumulate /
    // rebase / probe resolve against proven Trading Sessions and fail closed when the calendar
    // is unavailable. The startup record is emitted BEFORE the SDK / runtime build and before
    // any gateway request (always-emit-before-fallible-build, KTD2), so an admission refusal is
    // recorded before anything gateway-capable exists. NOTE: in manual `range` mode the record's
    // target IS `LS_INGEST_EDATE`, so that one env parse necessarily precedes the emit — a
    // missing/malformed EDATE returns before the record is written. Range mode requires no
    // calendar admission, so this is an audit gap, not a safety gap; see
    // docs/solutions/conventions/composition-root-always-emit-before-fallible-parse.md.
    let calendar = nautilus_ls::calendar::IngestCalendarContext::resolve(
        nautilus_ls::calendar::snapshot_path_from_env(),
        Utc::now(),
        nautilus_ls_calendar::CalendarAdoption::Enforced,
    );
    let calendar_target = calendar_target_for_mode(&mode, calendar.as_of())?;
    let startup = calendar.startup_record("ls-ingest", calendar_target);
    nautilus_ls::calendar::emit_startup_record(&startup);
    if automatic_mode_requires_calendar(&mode)
        && startup.action == nautilus_ls::calendar::ResultingAction::EnforcedFailClosed
    {
        return Err(format!(
            "calendar admission refused {mode} before gateway construction: {}",
            startup.render_line()
        )
        .into());
    }
    // Backfill-floor admission (#189 U6): the check above validates only the target date, but an
    // automatic run also backfills fresh instruments FROM the LS_INGEST_LOOKBACK floor. Fail
    // closed here — before the universe load / any gateway construction — when that floor is
    // outside calendar coverage, so a fresh triple cannot silently skip with zero bars and then
    // mis-pin a selection whose bars never landed (see `floor_admission`).
    if accumulate {
        let floor = parse_yyyymmdd(&env_required("LS_INGEST_LOOKBACK")?)?;
        let coverage = calendar.view().map(|view| {
            let c = view.calendar().coverage();
            (c.materialized_from, c.materialized_through)
        });
        if let Err(msg) = floor_admission(floor, coverage) {
            return Err(
                format!("calendar admission refused {mode} before gateway construction: {msg}").into(),
            );
        }
    }

    let adapter_cfg = match std::env::var("LS_INGEST_LANE_FILE") {
        Ok(path) => LsAdapterConfig::from_lane_file(path),
        Err(_) => LsAdapterConfig::from_env(),
    };
    let sdk = adapter_cfg.build_sdk()?;

    // Staged max-lookback probe (KTD10, R10): locate the earliest served minute date
    // for a pilot symbol and write <data>/probes/minute-lookback.json. No universe
    // load — the probe walks a single pilot symbol. Operator-gated.
    if mode == "probe-lookback" {
        run_probe(&sdk, catalog, &calendar).await?;
        return Ok(None);
    }

    // Resolve the universe. A per-symbol drip loop re-invokes `ls-ingest` many times;
    // the universe load (t8430 + 2× t9945) is identical every time and charges the
    // shared IGW00201 budget, so `LS_INGEST_SKIP_UNIVERSE_LOAD` (with explicit
    // symbols + instruments already persisted by the drip's daily pass) skips it —
    // the dominant avoidable per-invocation cost (KTD5 budget). The stratified
    // metadata selection counts as an explicit symbol list.
    let explicit_symbols: Option<String> = metadata_selection
        .as_ref()
        .map(|sel| sel.symbols.join(","))
        .or(symbols_env);
    let skip_load = should_skip_universe_load(
        env_flag("LS_INGEST_SKIP_UNIVERSE_LOAD"),
        explicit_symbols.is_some(),
    )?;

    let universe: Vec<InstrumentId> = if skip_load {
        // Skipping the load also skips `write_instruments`, which is only safe when a
        // prior full-universe pass persisted instrument defs. Refuse on an empty/
        // missing catalog rather than writing bars with no instruments (a silent
        // failure that only surfaces when the backtest can't resolve the instrument).
        let catalog_populated = catalog.exists()
            && std::fs::read_dir(&catalog).map(|mut d| d.next().is_some()).unwrap_or(false);
        if !catalog_populated {
            return Err(format!(
                "LS_INGEST_SKIP_UNIVERSE_LOAD is set but catalog {} is empty/missing — run a \
                 full-universe pass (without the flag) first so instrument definitions are persisted",
                catalog.display()
            )
            .into());
        }
        let syms = explicit_symbols.as_deref().unwrap_or_default();
        let u = parse_symbol_ids(syms);
        println!(
            "skipping universe load (LS_INGEST_SKIP_UNIVERSE_LOAD): {} explicit symbols \
             (catalog already populated)",
            u.len()
        );
        u
    } else {
        // Load the domestic-equity universe (t8430 + 2× t9945).
        let mut provider = InstrumentProvider::new(sdk.clone());
        provider.load_domain(InstrumentDomain::DomesticEquity).await?;
        println!("loaded {} domestic-equity instruments", provider.len());
        // Bound the universe if requested (required for minute backfills).
        let u: Vec<InstrumentId> = match &explicit_symbols {
            Some(list) => parse_symbol_ids(list),
            None => provider.all().map(|e| e.id).collect(),
        };
        // Persist the instrument definitions beside the bars (the universe re-snapshot,
        // R7 — newly-listed symbols enter coverage from this run forward).
        nautilus_ls::ingest::write_instruments(&catalog, provider.all_any()).await?;
        u
    };

    // Resolve the per-mode date range.
    let (sdate, edate) = if accumulate {
        let floor = env_required("LS_INGEST_LOOKBACK")?;
        automatic_date_range(floor, calendar_target)
    } else {
        (env_required("LS_INGEST_SDATE")?, env_required("LS_INGEST_EDATE")?)
    };

    let config = IngestConfig {
        catalog_path: catalog.clone(),
        bar_kinds,
        sdate: sdate.clone(),
        edate: edate.clone(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    };
    // The ingest lock is already held (`_lock`), so run without re-acquiring it.
    let mut ingestor = Ingestor::new(sdk, config);
    let report = if accumulate {
        let floor = parse_yyyymmdd(&sdate)?;
        let last_closed = parse_yyyymmdd(&edate)?;
        let gate = nautilus_ls::ingest::CalendarGate::new(calendar.view());
        if mode == "rebase" {
            ingestor
                .run_rebase_gated(&universe, last_closed, floor, gate)
                .await?
        } else {
            ingestor
                .run_accumulate_gated(&universe, last_closed, floor, gate)
                .await?
        }
    } else {
        ingestor.run(&universe).await?
    };

    // Pin the artifact identity into the catalog (KTD2) ONLY now that the
    // ingest completed: a pin written before/despite failure would attest a
    // selection whose bars never landed, and the per-tier report's hash
    // handshake (pin == manifest == artifact) would pass on un-ingested data.
    // A run carrying genuine refusals withholds the pin for the same reason.
    if let Some(sel) = &metadata_selection {
        if exit_code_for(&report) == 0 {
            MetadataPin {
                artifact_path: sel.artifact_path.clone(),
                content_hash: sel.content_hash.clone(),
                per_stratum: sel.per_stratum.clone(),
                symbols: sel.symbols.clone(),
                pinned_at: Utc::now().to_rfc3339(),
            }
            .write(&catalog)?;
            println!("metadata pin written: {} (hash {})", sel.artifact_path, sel.content_hash);
        } else {
            println!(
                "metadata pin WITHHELD: the run carried refusals — resolve them and re-run \
                 before backtesting against this artifact (KTD2)"
            );
        }
    }

    println!(
        "ingest complete: {} bars across {} triples ({} skipped), {} coverage gaps, {} refused pending heal",
        report.bars_written,
        report.triples_ingested,
        report.triples_skipped,
        report.gaps.len(),
        report.range_refusals.len()
    );
    if !report.range_refusals.is_empty() {
        for r in &report.range_refusals {
            println!(
                "REFUSED PENDING HEAL: {} {} carries an unhealed basis-shift mark (detected {}); range mode will not serve it on a stale basis — run accumulate/rebase to heal",
                r.instrument, r.bar_type, r.detected
            );
        }
    }
    if !report.heal_refusals.is_empty() {
        for r in &report.heal_refusals {
            println!(
                "HEAL REFUSED: {} {} — run floor {} is later than earliest stored bar {}; re-run with LS_INGEST_LOOKBACK at or before it (symbol stays marked)",
                r.instrument, r.bar_type, r.floor, r.earliest_stored
            );
        }
    }
    if !report.append_refusals.is_empty() {
        for r in &report.append_refusals {
            println!(
                "APPEND REFUSED (overlap): {} {} — attempted {} overlaps stored coverage [{}]; run `lab-research catalog compact` (duplicate pollution) or wipe + full re-pull / fresh catalog (disjoint coverage). Watermark not advanced.",
                r.instrument, r.bar_type, r.attempted, r.stored
            );
        }
    }
    if !report.backward_widen_warnings.is_empty() {
        for w in &report.backward_widen_warnings {
            println!(
                "BACKWARD WIDEN NO-OP: {} {} — lookback floor {} precedes earliest stored coverage {}; accumulate never fetches below the watermark. Recover the pre-coverage region with a fresh catalog at the wider lookback, or wipe + full re-pull.",
                w.instrument, w.bar_type, w.floor, w.earliest_stored
            );
        }
    }
    for uncertainty in &report.backward_widen_uncertainties {
        println!("{}", backward_widen_uncertainty_line(uncertainty));
    }
    if !report.budget_deferrals.is_empty() {
        for d in &report.budget_deferrals {
            println!(
                "SCHEDULED REMAINDER (budget): {} {} — estimated {} pages exceeds the remaining budget window ({} calls); stopped before the cliff, no bars fetched. Re-run on a cold budget window to resume (per-symbol idempotent).",
                d.instrument, d.bar_type, d.estimated_pages, d.remaining_budget
            );
        }
    }
    println!(
        "budget: {} symbols x {} bar-kinds, paced to {}/s (>= {:.0}s wall clock)",
        report.budget.symbols,
        report.budget.bar_kinds,
        report.budget.per_sec_cap,
        report.budget.min_seconds()
    );
    Ok(Some(report))
}

/// Staged max-lookback probe (KTD10). Uses a single liquid pilot symbol (default
/// `005930`) and a windowed backward search anchored at the last closed session,
/// writing the result to `<data>/probes/minute-lookback.json`.
async fn run_probe(
    sdk: &ls_sdk::LsSdk,
    catalog: PathBuf,
    calendar: &nautilus_ls::calendar::IngestCalendarContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let pilot = std::env::var("LS_PROBE_SYMBOL").unwrap_or_else(|_| "005930".into());
    let ncnt: u32 = std::env::var("LS_PROBE_NCNT").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let now_kst = (calendar.as_of() + Duration::hours(9)).naive_utc();
    let anchor = last_closed_session(now_kst, ACCUMULATE_CLOSE_BUFFER);
    let probed_at = calendar.as_of().to_rfc3339();

    // A dummy config carrying the catalog path (the probe uses the fetcher, not the
    // range fields).
    let config = IngestConfig {
        catalog_path: catalog,
        bar_kinds: vec![BarKind::Minute(ncnt)],
        sdate: String::new(),
        edate: String::new(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    };
    let ingestor = Ingestor::new(sdk.clone(), config);
    let gate = nautilus_ls::ingest::CalendarGate::new(calendar.view());
    match ingestor
        .run_probe_lookback_gated(&pilot, ncnt, anchor, probed_at, gate)
        .await?
    {
        Some(lb) => {
            println!(
                "probe: pilot {pilot} earliest minute date {} (depth {} days) — recorded to <data>/probes/minute-lookback.json",
                lb.earliest_date, lb.depth_days
            );
            println!("derive the backfill floor: LS_INGEST_LOOKBACK={} (or anchor − {} days)", lb.earliest_date, lb.depth_days);
        }
        None => {
            println!("probe: pilot {pilot} served no minute history — nothing recorded");
        }
    }
    Ok(())
}

fn env_required(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("missing required env var {key}"))
}

fn parse_yyyymmdd(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y%m%d").map_err(|e| format!("bad date {s:?}: {e}"))
}

fn automatic_mode_requires_calendar(mode: &str) -> bool {
    matches!(mode, "accumulate" | "rebase" | "probe-lookback")
}

fn calendar_target_for_mode(mode: &str, as_of: DateTime<Utc>) -> Result<NaiveDate, String> {
    if automatic_mode_requires_calendar(mode) {
        let now_kst = (as_of + Duration::hours(9)).naive_utc();
        Ok(last_closed_session(now_kst, ACCUMULATE_CLOSE_BUFFER))
    } else {
        parse_yyyymmdd(&env_required("LS_INGEST_EDATE")?)
    }
}

fn automatic_date_range(floor: String, calendar_target: NaiveDate) -> (String, String) {
    (floor, calendar_target.format("%Y%m%d").to_string())
}

fn require_paper() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("LS_TRADING_ENV").as_deref() {
        Ok("paper") => Ok(()),
        _ => Err("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only in v1)".into()),
    }
}

fn parse_kinds(spec: &str) -> Result<Vec<BarKind>, String> {
    let mut kinds = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if part == "daily" {
            kinds.push(BarKind::Daily);
        } else if let Some(n) = part.strip_prefix("minute:") {
            let n: u32 = n.parse().map_err(|_| format!("bad minute spec: {part}"))?;
            kinds.push(BarKind::Minute(n));
        } else {
            return Err(format!("unknown bar kind: {part} (want daily | minute:<n>)"));
        }
    }
    if kinds.is_empty() {
        kinds.push(BarKind::Daily);
    }
    Ok(kinds)
}

/// Read a boolean-ish env flag: present and `"1"`/`"true"` (case-insensitive) → true.
fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
        .unwrap_or(false)
}

/// Whether to skip the 3-call universe load (`t8430` + 2× `t9945`) and its
/// `write_instruments` re-snapshot for this invocation.
///
/// Skipping is the dominant avoidable IGW00201 saving in a per-symbol drip loop
/// (KTD5 budget): the universe load runs on *every* `ls-ingest` invocation but the
/// masters don't change minute-to-minute, so a 20-symbol minute drip re-fetches the
/// identical universe ~20× (3 calls each) on top of the bar fetches. Skipping is
/// only valid with an explicit `LS_INGEST_SYMBOLS` list (the load is otherwise the
/// only way to enumerate the universe), and assumes a prior full-universe
/// invocation already persisted the instrument definitions (the drip daily pass).
fn should_skip_universe_load(skip_requested: bool, has_symbols: bool) -> Result<bool, String> {
    match (skip_requested, has_symbols) {
        (true, true) => Ok(true),
        (true, false) => Err(
            "LS_INGEST_SKIP_UNIVERSE_LOAD requires an explicit LS_INGEST_SYMBOLS list \
             (the universe load is the only way to enumerate the full universe)"
                .to_string(),
        ),
        (false, _) => Ok(false),
    }
}

/// A resolved tier-stratified ingest selection (U3, plan 2026-07-10-003).
#[derive(Debug, Clone)]
struct StratifiedSelection {
    artifact_path: String,
    content_hash: String,
    /// Per-stratum selected counts, keyed by `Stratum::label`.
    per_stratum: std::collections::BTreeMap<String, usize>,
    /// The selected shcodes, in stratum order.
    symbols: Vec<String>,
}

/// Draw the tier-stratified sample from a `UniverseMetadata` artifact (R6):
/// up to `per_stratum` tradable symbols per pre-registered stratum, via the
/// reference module's deterministic `stratify`. A thin stratum contributes all
/// it has; the total is bounded by `4 × per_stratum`.
fn stratified_selection(
    artifact: &UniverseMetadata,
    artifact_path: &str,
    per_stratum: usize,
) -> Result<StratifiedSelection, String> {
    let sample = stratify(&artifact.records, per_stratum);
    let mut per_stratum_counts = std::collections::BTreeMap::new();
    let mut symbols = Vec::new();
    for stratum in Stratum::ALL {
        let picked = sample.get(&stratum).cloned().unwrap_or_default();
        per_stratum_counts.insert(stratum.label().to_string(), picked.len());
        symbols.extend(picked);
    }
    if symbols.is_empty() {
        return Err(format!(
            "metadata artifact {artifact_path} yields no tradable symbols in any stratum"
        ));
    }
    Ok(StratifiedSelection {
        artifact_path: artifact_path.to_string(),
        content_hash: artifact.content_hash(),
        per_stratum: per_stratum_counts,
        symbols,
    })
}

/// Resolve the optional metadata-driven selection from the env (`None` when
/// `LS_INGEST_METADATA` is unset — the existing `LS_INGEST_SYMBOLS` behavior
/// is preserved).
fn resolve_metadata_selection() -> Result<Option<StratifiedSelection>, String> {
    let Some(path) = std::env::var("LS_INGEST_METADATA").ok().filter(|s| !s.trim().is_empty())
    else {
        return Ok(None);
    };
    let per_stratum: usize = match std::env::var("LS_INGEST_PER_STRATUM") {
        Ok(v) => v
            .parse()
            .map_err(|_| format!("LS_INGEST_PER_STRATUM must be a positive integer, got {v:?}"))?,
        Err(_) => 5,
    };
    if per_stratum == 0 {
        return Err("LS_INGEST_PER_STRATUM must be at least 1".to_string());
    }
    let artifact = UniverseMetadata::load(std::path::Path::new(&path))?;
    artifact
        .validate()
        .map_err(|errs| format!("metadata artifact {path} failed validation:\n  - {}", errs.join("\n  - ")))?;
    stratified_selection(&artifact, &path, per_stratum).map(Some)
}

/// Parse a comma-separated shcode list into KRX-venue instrument ids.
fn parse_symbol_ids(list: &str) -> Vec<InstrumentId> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| InstrumentId::new(Symbol::from(s), Venue::from(nautilus_ls::KRX_VENUE)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use nautilus_ls::ingest::{
        AppendRefusal, BackwardWidenUncertainty, BackwardWidenWarning, BudgetEstimate,
        HealRefusal, RangeRefusal,
    };

    /// A zero-refusal, zero-warning coverage report — the base each case mutates.
    fn empty_report() -> CoverageReport {
        CoverageReport {
            bars_written: 0,
            triples_ingested: 0,
            triples_skipped: 0,
            gaps: Vec::new(),
            heal_refusals: Vec::new(),
            range_refusals: Vec::new(),
            append_refusals: Vec::new(),
            backward_widen_warnings: Vec::new(),
            backward_widen_uncertainties: Vec::new(),
            budget_deferrals: Vec::new(),
            budget: BudgetEstimate { symbols: 0, bar_kinds: 0, per_sec_cap: 1, min_requests: 0 },
        }
    }

    #[test]
    fn exit_zero_for_empty_report() {
        assert_eq!(exit_code_for(&empty_report()), 0);
    }

    /// #189 U6 follow-up: an automatic run whose LS_INGEST_LOOKBACK floor is outside calendar
    /// coverage fails closed BEFORE dispatch — otherwise a fresh instrument's prefix skips with
    /// zero bars, the run exits 0, and a metadata pin is mis-written on un-ingested data.
    #[test]
    fn floor_admission_refuses_a_floor_below_coverage() {
        let from = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let through = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        let cov = Some((from, through));
        // Floor before coverage → refused (the exact #1 cascade trigger).
        let err = floor_admission(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), cov)
            .expect_err("a floor below coverage must be refused");
        assert!(err.contains("outside the frozen calendar coverage"), "actionable message: {err}");
        // Floor after coverage → refused.
        assert!(floor_admission(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(), cov).is_err());
        // Floor at each coverage boundary and inside → admitted.
        assert!(floor_admission(from, cov).is_ok(), "floor == materialized_from is in coverage");
        assert!(floor_admission(through, cov).is_ok(), "floor == materialized_through is in coverage");
        assert!(floor_admission(NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(), cov).is_ok());
        // No calendar view → not this check's concern (the target admission already fails closed
        // on an unavailable calendar for automatic modes).
        assert!(floor_admission(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), None).is_ok());
    }

    /// R9: a report carrying only backward-widen warnings is still exit 0 —
    /// warnings never redden CI (a late-listed symbol warns every run forever).
    #[test]
    fn exit_zero_for_warning_only_report() {
        let mut report = empty_report();
        report.backward_widen_warnings.push(BackwardWidenWarning {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            floor: "20240101".to_string(),
            earliest_stored: "20240618".to_string(),
        });
        assert_eq!(exit_code_for(&report), 0, "backward-widen warnings never affect the exit code");
    }

    #[test]
    fn exit_zero_for_backward_widen_uncertainty() {
        let mut report = empty_report();
        report.backward_widen_uncertainties.push(BackwardWidenUncertainty {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            floor: "20100615".to_string(),
            earliest_stored: "20100618".to_string(),
            calendar_stale: false,
        });
        assert_eq!(exit_code_for(&report), 0);
    }

    /// R8: each genuine refusal vec independently forces a nonzero exit — and it is
    /// the distinct refusal code (2), separate from the hard-error FAILURE (1).
    #[test]
    fn exit_nonzero_for_each_genuine_refusal() {
        let mut append = empty_report();
        append.append_refusals.push(AppendRefusal {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            attempted: "20240103..20240105".to_string(),
            stored: "20240103..20240105".to_string(),
        });
        assert_eq!(exit_code_for(&append), EXIT_REFUSALS, "append refusal → nonzero");

        let mut heal = empty_report();
        heal.heal_refusals.push(HealRefusal {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            floor: "20240104".to_string(),
            earliest_stored: "20240103".to_string(),
        });
        assert_eq!(exit_code_for(&heal), EXIT_REFUSALS, "heal refusal → nonzero");

        let mut range = empty_report();
        range.range_refusals.push(RangeRefusal {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            detected: "20240105".to_string(),
        });
        assert_eq!(exit_code_for(&range), EXIT_REFUSALS, "range refusal → nonzero");

        assert_ne!(EXIT_REFUSALS, 1, "the refusal code stays distinct from the hard-error FAILURE");
    }

    #[test]
    fn skip_universe_load_decision() {
        // Flag + explicit symbols → skip the 3-call load (the drip-loop saving).
        assert_eq!(should_skip_universe_load(true, true), Ok(true));
        // No flag → always load (default, backward-compatible).
        assert_eq!(should_skip_universe_load(false, true), Ok(false));
        assert_eq!(should_skip_universe_load(false, false), Ok(false));
        // Flag WITHOUT explicit symbols is an error: the load is the only way to
        // enumerate the universe, so skipping it would leave nothing to ingest.
        assert!(
            should_skip_universe_load(true, false).is_err(),
            "skip without an explicit symbol list must be refused, not silently skipped"
        );
    }

    #[test]
    fn env_flag_parses_truthy_values() {
        // (env mutation is process-global; use a key unique to this test.)
        std::env::remove_var("LS_TEST_FLAG_XYZ");
        assert!(!env_flag("LS_TEST_FLAG_XYZ"), "unset → false");
        std::env::set_var("LS_TEST_FLAG_XYZ", "1");
        assert!(env_flag("LS_TEST_FLAG_XYZ"), "1 → true");
        std::env::set_var("LS_TEST_FLAG_XYZ", "TRUE");
        assert!(env_flag("LS_TEST_FLAG_XYZ"), "TRUE (case-insensitive) → true");
        std::env::set_var("LS_TEST_FLAG_XYZ", "0");
        assert!(!env_flag("LS_TEST_FLAG_XYZ"), "0 → false");
        std::env::remove_var("LS_TEST_FLAG_XYZ");
    }

    #[test]
    fn automatic_calendar_target_is_the_close_buffer_civil_ceiling() {
        let after = Utc.with_ymd_and_hms(2026, 7, 18, 8, 0, 0).unwrap(); // 17:00 KST Saturday
        let before = Utc.with_ymd_and_hms(2026, 7, 18, 6, 0, 0).unwrap(); // 15:00 KST Saturday

        assert_eq!(
            calendar_target_for_mode("accumulate", after).unwrap(),
            NaiveDate::from_ymd_opt(2026, 7, 18).unwrap()
        );
        assert_eq!(
            calendar_target_for_mode("probe-lookback", before).unwrap(),
            NaiveDate::from_ymd_opt(2026, 7, 17).unwrap()
        );
    }

    #[test]
    fn automatic_date_range_uses_the_frozen_calendar_target() {
        assert_eq!(
            automatic_date_range(
                "20100101".to_string(),
                NaiveDate::from_ymd_opt(2026, 7, 18).unwrap(),
            ),
            ("20100101".to_string(), "20260718".to_string())
        );
    }

    #[test]
    fn backward_widen_uncertainty_output_is_distinct_and_complete() {
        let line = backward_widen_uncertainty_line(&BackwardWidenUncertainty {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            floor: "20100615".to_string(),
            earliest_stored: "20100618".to_string(),
            calendar_stale: true,
        });

        assert!(line.starts_with("BACKWARD WIDEN UNCERTAIN:"));
        for expected in ["005930.XKRX", "1-DAY", "20100615", "20100618", "calendar_stale=true"] {
            assert!(line.contains(expected), "missing {expected}: {line}");
        }
    }

    mod stratified {
        use super::super::*;
        use nautilus_ls::reference::universe_metadata::{
            assign_cap_tiers, CapTier, InstrumentMetadata, LiquidityTier, MarketClass,
            MetadataProvenance, Resolved, IndexMembership,
        };

        fn record(shcode: &str, market: MarketClass, cap: Resolved<f64>) -> InstrumentMetadata {
            InstrumentMetadata {
                shcode: shcode.to_string(),
                market_class: market,
                market_cap: cap,
                cap_tier: CapTier::BelowBoard,
                turnover: Resolved::Unavailable,
                liquidity_tier: LiquidityTier::Unknown,
                index_membership: Resolved::Proxy(IndexMembership::NotMember),
                has_derivative: Resolved::Value(false),
                designation: None,
                tradable: true,
            }
        }

        /// 6 on-board KOSPI + 4 on-board KOSDAQ + 3 below-board equities.
        fn artifact() -> UniverseMetadata {
            let mut records: Vec<InstrumentMetadata> = Vec::new();
            for i in 0..6 {
                records.push(record(
                    &format!("{:06}", 100 + i),
                    MarketClass::Kospi,
                    Resolved::Value((1000 - i) as f64),
                ));
            }
            for i in 0..4 {
                records.push(record(
                    &format!("{:06}", 200 + i),
                    MarketClass::Kosdaq,
                    Resolved::Value((100 - i) as f64),
                ));
            }
            for i in 0..3 {
                records.push(record(&format!("{:06}", 300 + i), MarketClass::Kosdaq, Resolved::Unavailable));
            }
            let cutoffs = assign_cap_tiers(&mut records, 0.5);
            UniverseMetadata {
                provenance: MetadataProvenance {
                    captured_at: "2026-07-10T00:00:00Z".to_string(),
                    session_date: "20260710".to_string(),
                    source_trs: vec!["t8430".into()],
                    instrument_type_filter: "equities-only".to_string(),
                    tier_boundary_rule: "test quantile 0.5".to_string(),
                    cap_cutoffs: cutoffs,
                    paper_incompatible: Vec::new(),
                },
                records,
            }
        }

        #[test]
        fn selection_is_equal_per_stratum_and_bounded() {
            let sel = stratified_selection(&artifact(), "x.json", 2).unwrap();
            // 4 strata × 2 — every stratum here has ≥ 2 candidates.
            assert_eq!(sel.per_stratum["kospi_blue_chip"], 2);
            assert_eq!(sel.per_stratum["kospi_mid"], 2);
            assert_eq!(sel.per_stratum["kosdaq_on_board"], 2);
            assert_eq!(sel.per_stratum["small_cap_exclusion"], 2);
            assert_eq!(sel.symbols.len(), 8, "total respects the 4×per_stratum bound");
            assert_eq!(sel.content_hash, artifact().content_hash());
        }

        #[test]
        fn a_thin_stratum_contributes_all_it_has() {
            let sel = stratified_selection(&artifact(), "x.json", 10).unwrap();
            assert_eq!(sel.per_stratum["kospi_blue_chip"], 3, "top half of 6");
            assert_eq!(sel.per_stratum["kospi_mid"], 3);
            assert_eq!(sel.per_stratum["kosdaq_on_board"], 4);
            assert_eq!(sel.per_stratum["small_cap_exclusion"], 3);
            assert_eq!(sel.symbols.len(), 13);
        }

        #[test]
        fn an_all_designated_artifact_is_refused() {
            let mut a = artifact();
            for r in &mut a.records {
                r.designation = Some(nautilus_ls::reference::universe_metadata::Designation {
                    kind: nautilus_ls::reference::universe_metadata::DesignationKind::Halt,
                    source_tr: "t1405".to_string(),
                });
                r.tradable = false;
            }
            let err = stratified_selection(&a, "x.json", 2).unwrap_err();
            assert!(err.contains("no tradable symbols"), "{err}");
        }
    }

    #[test]
    fn parse_symbol_ids_builds_krx_venue_ids() {
        let ids = parse_symbol_ids(" 005930, 000660 ,, 402340 ");
        assert_eq!(ids.len(), 3, "blank/whitespace entries skipped");
        assert_eq!(ids[0].symbol.as_str(), "005930");
        assert_eq!(ids[0].venue.as_str(), nautilus_ls::KRX_VENUE);
        assert_eq!(ids[2].symbol.as_str(), "402340");
    }
}
