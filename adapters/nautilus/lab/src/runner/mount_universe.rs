//! Produce the live-mount universe file (`LS_MOUNT_UNIVERSE_FILE`) that `--mount` trades.
//!
//! `--mount` consumes an already-resolved universe: it does NOT run `select_universe`, so
//! whatever this file contains is exactly what the session trades. That makes the producer
//! part of the head's behavioral surface, not a convenience — and it is why every derived
//! value here is computed by calling the **backtest's own** helpers rather than by a parallel
//! implementation. A second implementation of the ATR window or the turnover ordering would
//! be a silent head-divergence the gate could never catch.
//!
//! # The `prior_atr` trap
//!
//! `prior_atr` is OPTIONAL in the file schema but NOT optional for head fidelity. The v34
//! head runs `or_width_max_atr = 0.666`, and the OR-width gate is deliberately
//! *skip-not-reject* when `prior_atr` is absent: a universe without it silently disables the
//! width gate for every symbol and emits no reject envelope, so the divergence leaves no
//! trace in `decisions.jsonl`. This producer therefore treats a symbol whose ATR could not be
//! computed as **not a candidate for today**, rather than emitting it un-gated.
//!
//! `prior_open_vol_mean` and `prior_illiq` are genuinely optional under v34 (`rvol_min = 0.0`
//! and `liquidity_tilt_alpha = 0.0` — both levers reverted), and are emitted when derivable so
//! the file stays correct if either lever is ever armed.
//!
//! # Offline by construction
//!
//! Everything is read from the catalog. `today_open` is the open of the session date's daily
//! bar, so **today's daily bar must already be ingested** — the producer refuses loudly rather
//! than inventing an open. It makes no gateway call and takes no nonce: it produces an input,
//! it authorizes nothing.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use nautilus_ls::ingest::{read_all_bars, read_all_instruments};
use nautilus_ls::reference::universe_metadata::{InstrumentMetadata, UniverseMetadata};
use serde::Serialize;

use crate::agent::envelope::Decision;
use crate::agent::sink::DecisionSink;
use crate::params::OrbParams;
use crate::runner::backtest::{build_candidates, is_daily, is_minute, kst_date_of, shcode_of};
use crate::runner::live::resolve_mount_head_params;
use crate::strategy::orb::{kst_time_from_nanos, select_universe};

/// One emitted row — the exact shape `--mount` parses (`MountUniverseSymbol`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MountUniverseRow {
    /// The KST session date this row was resolved FOR (`YYYY-MM-DD`).
    ///
    /// Emitted on every row so `--mount` can refuse a file resolved for another day. Without
    /// it a stale file is indistinguishable from a fresh one: the mount re-runs no selection,
    /// so yesterday's symbols and yesterday's `today_open` would simply be traded.
    pub session_date: String,
    /// The 6-digit KRX short code.
    pub shcode: String,
    /// Canonical integer prior-session close.
    pub prior_close: i64,
    /// Canonical integer open on the session date.
    pub today_open: i64,
    /// Prior ATR over the head's `atr_window`. Always `Some` in an emitted row.
    pub prior_atr: Option<f64>,
    /// Mean prior opening-window volume, when enough history exists.
    pub prior_open_vol_mean: Option<f64>,
    /// Prior Amihud illiquidity, when derivable.
    pub prior_illiq: Option<f64>,
}

/// What to produce, and from where.
#[derive(Debug, Clone)]
pub struct MountUniverseConfig {
    /// The data home whose `catalog/` is read and whose finalized runs key the head params.
    pub data_home: PathBuf,
    /// The KST session date the universe is resolved for.
    pub session_date: NaiveDate,
    /// The universe-metadata artifact, when the head run was metadata-driven. Passing `None`
    /// against a metadata-driven head silently changes the tradability gate.
    pub metadata_path: Option<PathBuf>,
}

/// Gather the producer config from the process environment.
///
/// # Errors
///
/// If `LS_DATA_HOME` is unset, or `LS_MOUNT_UNIVERSE_DATE` is absent/unparseable.
pub fn config_from_env() -> anyhow::Result<MountUniverseConfig> {
    let data_home = std::env::var("LS_DATA_HOME")
        .map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required"))?
        .into();
    // Required, never defaulted to "today": the session date decides every derived value,
    // and a producer that guessed it could silently resolve yesterday's universe.
    let raw = std::env::var("LS_MOUNT_UNIVERSE_DATE").map_err(|_| {
        anyhow::anyhow!("LS_MOUNT_UNIVERSE_DATE is required (the KST session date, YYYY-MM-DD)")
    })?;
    let session_date = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("LS_MOUNT_UNIVERSE_DATE {raw:?} is not a YYYY-MM-DD date: {e}"))?;
    let metadata_path = std::env::var("LS_MOUNT_UNIVERSE_METADATA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);
    Ok(MountUniverseConfig { data_home, session_date, metadata_path })
}

/// Resolve the mount universe for `cfg.session_date` from the catalog.
///
/// # Errors
///
/// If the catalog is absent or unreadable, the head params cannot be resolved, the metadata
/// artifact fails validation, or no symbol survives selection.
pub async fn resolve(cfg: &MountUniverseConfig) -> anyhow::Result<Vec<MountUniverseRow>> {
    let catalog = cfg.data_home.join("catalog");
    if !catalog.exists() {
        anyhow::bail!("no catalog at {} — ingest before resolving a universe", catalog.display());
    }
    // The SAME head-params source `--mount` sizes from: a universe selected under different
    // params than the session trades under is a head divergence by construction.
    let params = resolve_mount_head_params(&cfg.data_home)?;

    // Bind the metadata artifact to the HEAD's identity, keyed off the same run the params
    // came from. Without this, omitting `LS_MOUNT_UNIVERSE_METADATA` against a metadata-driven
    // head is completely silent: absent metadata maps every candidate to
    // `CandidateMeta::Untagged`, the tradability gate disappears, and the live session trades
    // symbols the certified backtest excluded — with nothing in the artifact to show it.
    let head_metadata_hash = crate::dispatch::ladder::head_manifest_pinned(
        &cfg.data_home,
        crate::dispatch::ladder::head_version_pin(),
    )
    .and_then(|(_rid, m)| m.universe_metadata_hash);

    let metadata: Option<HashMap<String, InstrumentMetadata>> = match (&cfg.metadata_path, &head_metadata_hash) {
        (None, Some(expected)) => anyhow::bail!(
            "mount-universe refused: the head is METADATA-DRIVEN (its run carries \
             universe_metadata_hash {expected}) but LS_MOUNT_UNIVERSE_METADATA is unset — \
             producing the universe without it silently drops the tradability gate and trades \
             symbols the head excluded. Set it to the artifact that run was built from."
        ),
        (Some(path), _) => {
            let artifact = UniverseMetadata::load(path).map_err(|e| anyhow::anyhow!(e))?;
            artifact.validate().map_err(|errs| {
                anyhow::anyhow!("metadata artifact failed validation:\n  - {}", errs.join("\n  - "))
            })?;
            let hash = artifact.content_hash();
            match &head_metadata_hash {
                Some(expected) if expected != &hash => anyhow::bail!(
                    "mount-universe refused: metadata artifact hash mismatch — the head run was \
                     built from {expected} but LS_MOUNT_UNIVERSE_METADATA resolves to {hash}. A \
                     re-capture between the head run and now re-tiers symbols; point at the ONE \
                     artifact the head used."
                ),
                None => eprintln!(
                    "mount-universe: warning — LS_MOUNT_UNIVERSE_METADATA is set but the head run \
                     carries no universe_metadata_hash (not a metadata-driven head); the \
                     tradability gate will apply where the head's did not"
                ),
                _ => {}
            }
            Some(artifact.records.into_iter().map(|r| (r.shcode.clone(), r)).collect())
        }
        (None, None) => None,
    };

    let all_bars = read_all_bars(&catalog).await.map_err(|e| anyhow::anyhow!(e))?;
    let instruments = read_all_instruments(&catalog).await.map_err(|e| anyhow::anyhow!(e))?;

    let (daily_by_inst, open_vol_by_inst) = index_catalog(&all_bars, &params);

    let candidates = build_candidates(
        &instruments,
        &daily_by_inst,
        &open_vol_by_inst,
        &params,
        cfg.session_date,
        metadata.as_ref(),
    );

    // Selection is the strategy's own — ordering, turnover floor, gap floor and top-N all
    // come from `select_universe`, never re-derived here.
    let sink = DecisionSink::new();
    let selected = select_universe(&candidates, &params, &sink, 0);

    let by_symbol: HashMap<&str, &_> =
        candidates.iter().map(|c| (c.symbol.as_str(), c)).collect();
    let mut rows = Vec::new();
    let mut dropped_no_atr = Vec::new();
    for symbol in &selected {
        let Some(c) = by_symbol.get(symbol.as_str()) else { continue };
        // See the module note: an ATR-less row would silently disable the OR-width gate for
        // this symbol. Drop it loudly instead of trading it un-gated.
        let Some(atr) = c.prior_atr.filter(|a| *a > 0.0) else {
            dropped_no_atr.push(symbol.clone());
            continue;
        };
        rows.push(MountUniverseRow {
            session_date: cfg.session_date.to_string(),
            shcode: shcode_of(symbol).to_string(),
            prior_close: c.gap_prices.prior_close,
            today_open: c.gap_prices.today_open,
            prior_atr: Some(atr),
            prior_open_vol_mean: c.prior_open_vol_mean,
            prior_illiq: c.prior_illiq,
        });
    }
    if !dropped_no_atr.is_empty() {
        eprintln!(
            "mount-universe: dropped {} selected symbol(s) with no computable prior ATR \
             (insufficient daily history for the head's atr_window): {}",
            dropped_no_atr.len(),
            dropped_no_atr.join(",")
        );
    }
    if rows.is_empty() {
        // Name the ACTUAL cause. An empty result has three very different causes and only
        // one is a missing daily bar; sending the operator to re-check ingest during the
        // attended pre-session window when selection simply rejected everything wastes the
        // one window they have. The sink already holds the real reject reasons.
        anyhow::bail!(
            "mount-universe: no symbol resolved for {}: {} candidate(s) had a daily bar for \
             that date, {} survived selection, {} of those were dropped for having no \
             computable prior ATR.{}{}",
            cfg.session_date,
            candidates.len(),
            selected.len(),
            dropped_no_atr.len(),
            if candidates.is_empty() {
                " No candidate had both a daily bar ON the session date and a prior one — \
                 is the session date's daily bar ingested? (`today_open` comes from it.)"
            } else {
                ""
            },
            if selected.is_empty() && !candidates.is_empty() {
                format!(" Selection rejected every candidate: {}", reject_tally(&sink))
            } else {
                String::new()
            }
        );
    }
    Ok(rows)
}

/// Index the catalog the way the backtest's session loop does: daily bars bucketed per
/// instrument (ts-sorted), and opening-window volume per (instrument, date).
///
/// One deliberate difference: the backtest clips minute bars to the backtest range, which
/// makes its RVOL baseline range-dependent (a documented asymmetry). A live session has no
/// range, so every catalogued prior session counts.
fn index_catalog<'a>(
    all_bars: &'a [Bar],
    params: &OrbParams,
) -> (HashMap<InstrumentId, Vec<&'a Bar>>, HashMap<InstrumentId, BTreeMap<NaiveDate, f64>>) {
    let mut daily_by_inst: HashMap<InstrumentId, Vec<&Bar>> = HashMap::new();
    let mut minute_by_date: HashMap<NaiveDate, Vec<&Bar>> = HashMap::new();
    for b in all_bars {
        if is_daily(b) {
            daily_by_inst.entry(b.bar_type.instrument_id()).or_default().push(b);
        } else if is_minute(b) {
            minute_by_date.entry(kst_date_of(b)).or_default().push(b);
        }
    }
    for bars in daily_by_inst.values_mut() {
        bars.sort_by_key(|b| b.ts_event.as_u64());
    }

    let range_open = params.range_open;
    let range_end = params.range_end();
    let mut open_vol_by_inst: HashMap<InstrumentId, BTreeMap<NaiveDate, f64>> = HashMap::new();
    for (date, bars) in &minute_by_date {
        for b in bars {
            let t = kst_time_from_nanos(b.ts_event.as_u64());
            if t >= range_open && t < range_end {
                *open_vol_by_inst
                    .entry(b.bar_type.instrument_id())
                    .or_default()
                    .entry(*date)
                    .or_insert(0.0) += b.volume.as_f64();
            }
        }
    }
    (daily_by_inst, open_vol_by_inst)
}

/// Tally the rejecting filters `select_universe` recorded, most frequent first, so an
/// empty-universe refusal can name why selection rejected everything instead of guessing.
fn reject_tally(sink: &DecisionSink) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for env in sink.snapshot() {
        if let Some(d) = &env.decision_detail {
            if d.decision == Some(Decision::Reject) {
                if let Some(f) = &d.filter {
                    *counts.entry(f.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    let mut pairs: Vec<(String, usize)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if pairs.is_empty() {
        return "(no reject envelopes recorded)".to_string();
    }
    pairs.iter().map(|(f, n)| format!("{f}={n}")).collect::<Vec<_>>().join(", ")
}

/// Serialize the resolved rows as the JSON array `--mount` parses.
///
/// # Errors
///
/// If serialization fails.
pub fn to_json(rows: &[MountUniverseRow]) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(rows)?)
}

/// Write the resolved rows to `path`, or to stdout when `path` is `None`.
///
/// # Errors
///
/// If serialization or the write fails.
pub fn emit(rows: &[MountUniverseRow], path: Option<&Path>) -> anyhow::Result<()> {
    let json = to_json(rows)?;
    match path {
        Some(p) => {
            std::fs::write(p, json.as_bytes())
                .map_err(|e| anyhow::anyhow!("writing {}: {e}", p.display()))?;
            eprintln!("mount-universe: wrote {} symbol(s) → {}", rows.len(), p.display());
        }
        None => println!("{json}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shcode_is_taken_from_the_instrument_symbol() {
        assert_eq!(shcode_of("005930.XKRX"), "005930");
        assert_eq!(shcode_of("005930"), "005930");
    }

    /// The emitted JSON must deserialize as the exact shape `--mount` parses — the whole
    /// point of the producer. A field rename on either side breaks this.
    #[test]
    fn emitted_json_parses_as_the_mount_universe_mount_itself_reads() {
        let rows = vec![MountUniverseRow {
            session_date: "2026-07-27".into(),
            shcode: "005930".into(),
            prior_close: 71_000,
            today_open: 71_800,
            prior_atr: Some(1234.5),
            prior_open_vol_mean: Some(9876.0),
            prior_illiq: None,
        }];
        let json = to_json(&rows).unwrap();
        let parsed = crate::runner::live::parse_mount_universe(json.as_bytes()).unwrap();
        assert_eq!(parsed.len(), 1, "round-trips through mount's own parser");
    }

    /// An empty universe is a hard error, never an empty file — `--mount` would otherwise
    /// refuse later with a less specific message, after the operator had burned a nonce.
    #[test]
    fn an_empty_row_set_serializes_but_is_never_what_resolve_returns() {
        let json = to_json(&[]).unwrap();
        assert_eq!(json.trim(), "[]");
        assert!(
            crate::runner::live::parse_mount_universe(json.as_bytes()).is_err(),
            "mount refuses an empty universe, so resolve() must never emit one"
        );
    }
}
