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
//! # Sourcing: offline for a past date, one live read for today
//!
//! Everything derived from PRIOR sessions is read from the catalog, always. The one value that
//! cannot be is `today_open`:
//!
//! - **Past session date** — the open is the session date's catalog daily bar. Fully offline,
//!   no gateway call. Unchanged.
//! - **The session date IS today (KST)** — the catalog cannot supply it. The accumulate ingest
//!   path is calendar-gated on the same-day `Unknown` status and independently refuses to
//!   advance a watermark into an in-session day, so an in-progress session never has a stored
//!   daily bar. The gateway *does* serve one (t8410 returns a row mid-session), but this
//!   producer reads the catalog, so the open is fetched from a live `t8407` quote instead.
//!
//! That live read is the ONLY gateway call this binary makes, it happens only when the session
//! date is today, and it is a market-data read. The producer still takes **no nonce and
//! authorizes nothing** — it produces an input. A symbol whose live open is missing or
//! non-positive (pre-open, or halted before it ever traded) is dropped, never defaulted.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::instruments::Instrument;
use nautilus_ls::ingest::{read_all_bars, read_all_instruments};
use nautilus_ls::reference::universe_metadata::{InstrumentMetadata, UniverseMetadata};
use serde::Serialize;

use crate::agent::envelope::Decision;
use crate::agent::sink::DecisionSink;
use crate::params::OrbParams;
use crate::runner::backtest::{
    build_candidates_with_today_open, is_daily, is_minute, kst_date_of, select_prior, shcode_of,
};
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

/// Where each row's `today_open` comes from.
///
/// The session date's daily bar is the natural source and the only one for a PAST date. It is
/// simply not obtainable for a date you are standing in: the accumulate ingest path is
/// calendar-gated on the same-day `Unknown` status and independently refuses to advance a
/// watermark into an in-session day, so an in-progress session never has a stored daily bar.
/// The gateway does serve one (t8410 returns a row for an in-progress session), but the
/// producer reads the catalog, not the gateway — so for today the open comes from a live quote.
#[derive(Clone)]
pub enum TodayOpenSource {
    /// A past session date: the open is the session date's catalog daily bar (unchanged).
    Catalog,
    /// The session date IS today in KST: the open comes from a live `t8407` quote.
    ///
    /// The credentials are resolved EAGERLY, in `config_from_env`, and carried here rather than
    /// re-read later from a path. `LsAdapterConfig::build_config` loads the lane file through
    /// `std::env::set_var`, whose safety contract is "single-threaded startup, before any SDK
    /// client or async runtime is constructed" — resolving it inside the async producer would
    /// mutate `environ` while tokio workers and reqwest's proxy lookup read it.
    LiveQuote {
        lane_env_path: PathBuf,
        config: Box<ls_core::config::LsConfig>,
    },
}

// `LsConfig` deliberately implements neither `Debug` nor `Display` (printing it risks leaking
// credentials), so the enum's `Debug` is written by hand and names only the path.
impl std::fmt::Debug for TodayOpenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodayOpenSource::Catalog => f.write_str("Catalog"),
            TodayOpenSource::LiveQuote { lane_env_path, .. } => f
                .debug_struct("LiveQuote")
                .field("lane_env_path", lane_env_path)
                .field("config", &"<redacted>")
                .finish(),
        }
    }
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
    /// Where `today_open` is sourced from — decided by whether `session_date` is today (KST).
    pub today_open_source: TodayOpenSource,
}

/// Six-digit codes packed into one `t8407` request. 27 in a single call was verified against
/// the paper gateway (`rsp_cd=00000`); 25 keeps headroom under that observed-good size while
/// still resolving a ~75-symbol catalog in three calls.
const T8407_BATCH: usize = 25;

/// Pause between `t8407` batches. The MarketData bucket's budget (`IGW00201`) is cumulative
/// and warm-sensitive, and this runs in the attended pre-session window where a throttle would
/// cost the operator their one shot.
const T8407_BATCH_PACE: std::time::Duration = std::time::Duration::from_millis(400);

/// How stale the prior session may be before a live-sourced row is refused.
///
/// On the catalog path this bound did not need stating: a candidate required a daily bar dated
/// ON the session date, which only exists if ingest reached that date, so `prior` was
/// necessarily the adjacent session. The live path drops that requirement — it needs only *some*
/// earlier bar — so without a bound `gap_pct = (today_open - prior_close)/prior_close` silently
/// becomes a multi-session return while still being compared against the head's overnight-gap
/// floor. Ten calendar days clears Korea's longest holiday clusters (Seollal / Chuseok plus
/// flanking weekends) while still catching a catalog that is weeks behind.
const MAX_PRIOR_STALENESS_DAYS: i64 = 10;

/// Now, in KST.
fn now_kst() -> chrono::NaiveDateTime {
    let kst = chrono::FixedOffset::east_opt(nautilus_ls::rules::KST_UTC_OFFSET_HOURS * 3600)
        .expect("KST offset is valid");
    chrono::Utc::now().with_timezone(&kst).naive_local()
}

/// Today's KST calendar date.
fn today_kst() -> NaiveDate {
    now_kst().date()
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
    // Sourcing is decided by the date, never by a flag: an operator-set switch could point the
    // live path at a past date (trading a stale universe at TODAY's opens) or the catalog path
    // at today (which resolves nothing at all). The date is the fact; the source follows it.
    let today_open_source = if session_date == today_kst() {
        let raw = std::env::var("LS_DISPATCH_LANE_ENV").map_err(|_| {
            anyhow::anyhow!(
                "LS_MOUNT_UNIVERSE_DATE {session_date} is TODAY (KST), so today_open must come \
                 from a live t8407 quote — the catalog cannot hold an in-session daily bar — but \
                 LS_DISPATCH_LANE_ENV is unset. Point it at the ABSOLUTE path of the lane env \
                 file (the same one the dispatch gate reads)."
            )
        })?;
        let lane_env_path = PathBuf::from(raw.trim());
        if !lane_env_path.exists() {
            anyhow::bail!(
                "LS_DISPATCH_LANE_ENV points at {} which does not exist — the live today_open \
                 fetch would authenticate as no account at all",
                lane_env_path.display()
            );
        }
        // Resolve credentials HERE, synchronously, before the runtime does any work: this path
        // mutates the process environment (see `TodayOpenSource::LiveQuote`). It also moves a
        // bad-credential failure to the same fail-fast point as the path check above, instead
        // of surfacing after a full catalog read.
        let config = nautilus_ls::config::LsAdapterConfig::from_lane_file(&lane_env_path)
            .build_config()
            .map_err(|e| {
                anyhow::anyhow!("resolving lane credentials from {}: {e}", lane_env_path.display())
            })?;
        TodayOpenSource::LiveQuote { lane_env_path, config: Box::new(config) }
    } else {
        TodayOpenSource::Catalog
    };
    Ok(MountUniverseConfig { data_home, session_date, metadata_path, today_open_source })
}

/// Why a requested symbol produced no usable live open. The causes have different remedies, so
/// they are never collapsed into one "not quoted" bucket — that conflation is what sends an
/// operator chasing market state when the real fault is a wire-shape change.
#[derive(Debug, Default)]
pub(crate) struct OpenDrops {
    /// Echoed by the gateway, but the open was zero/blank — genuinely pre-open or never traded.
    pub not_yet_open: Vec<String>,
    /// Echoed, but the open did not parse as an integer — a WIRE-SHAPE change, not market state.
    pub unparseable: Vec<String>,
}

/// The result of one live-open fetch.
#[derive(Debug)]
pub(crate) struct TodayOpens {
    pub opens: HashMap<InstrumentId, i64>,
    pub drops: OpenDrops,
}

/// Fetch today's opens for `wanted` via `t8407`, batched and paced.
///
/// Takes an already-built `sdk` so the batching, packing, echo-keying and parsing are reachable
/// from an offline wiremock test; credential resolution belongs to `config_from_env`.
///
/// A symbol the gateway does not echo at all is a **hard error**: the request named it, so
/// silence means the request or the framing was wrong, and dropping it would silently re-compose
/// the traded universe (selection is top-N, so a smaller pool does not shrink the universe — it
/// refills from lower-ranked names). A symbol that IS echoed but carries an unusable open is
/// classified and reported, never defaulted.
///
/// # Errors
///
/// If a code is not exactly six digits, if a batch fails, or if the gateway omits a requested code.
pub(crate) async fn fetch_today_opens(
    sdk: &ls_sdk::LsSdk,
    wanted: &[(InstrumentId, String)],
) -> anyhow::Result<TodayOpens> {
    use ls_sdk::market_session::T8407Request;

    // `shcode` is a FIXED-WIDTH packing: N six-character codes back to back, with `nrec` the
    // count. One off-length code shifts every code after it in the batch, so the gateway quotes
    // symbols nobody asked for. Refuse before packing rather than debug a mis-framed batch.
    if let Some((id, code)) = wanted
        .iter()
        .find(|(_, c)| c.len() != 6 || !c.bytes().all(|b| b.is_ascii_digit()))
    {
        anyhow::bail!(
            "instrument {id} yields shcode {code:?}, which is not six ASCII digits — t8407 packs \
             codes at a fixed six-character width, so one off-length code mis-frames its whole \
             batch and silently quotes the wrong symbols"
        );
    }
    let mut by_shcode: HashMap<&str, &InstrumentId> = HashMap::new();
    for (id, code) in wanted {
        if let Some(prev) = by_shcode.insert(code.as_str(), id) {
            anyhow::bail!(
                "shcode {code} maps to two instruments ({prev} and {id}) — one would silently \
                 never receive an open"
            );
        }
    }

    let mut opens = HashMap::new();
    let mut drops = OpenDrops::default();
    let batches: Vec<&[(InstrumentId, String)]> = wanted.chunks(T8407_BATCH).collect();
    for (i, batch) in batches.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(T8407_BATCH_PACE).await;
        }
        let packed: String = batch.iter().map(|(_, code)| code.as_str()).collect();
        let req = T8407Request::new(batch.len().to_string(), packed);
        // No rsp_cd re-check: `Inner::post` already classifies it and returns `LsError::ApiError`
        // for anything outside the documented read-success set, so the only codes that can reach
        // here are successes (`""`, `00000`, `00136`, `00707`). Re-testing for `== "00000"` could
        // only reject a response the SDK accepted.
        let resp = sdk
            .market_session()
            .multi_symbol_current_price(&req)
            .await
            .map_err(|e| anyhow::anyhow!("t8407 batch {}/{}: {e}", i + 1, batches.len()))?;

        let mut echoed: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for row in &resp.outblock1 {
            // A number-typed echo loses leading zeros (`string_or_number` renders 5930 as
            // "5930"), so re-pad before matching or every 0-prefixed KRX code would miss.
            let code = format!("{:0>6}", row.shcode.trim());
            let Some(id) = by_shcode.get(code.as_str()) else { continue };
            echoed.insert(
                batch
                    .iter()
                    .find(|(_, c)| *c == code)
                    .map(|(_, c)| c.as_str())
                    .unwrap_or_default(),
            );
            let raw = row.open.trim();
            match raw.parse::<i64>() {
                Ok(open) if open > 0 => {
                    opens.insert(**id, open);
                }
                Ok(_) => drops.not_yet_open.push(code.clone()),
                Err(_) if raw.is_empty() => drops.not_yet_open.push(code.clone()),
                Err(_) => drops.unparseable.push(format!("{code}(open={raw:?})")),
            }
        }
        let missing: Vec<&str> = batch
            .iter()
            .map(|(_, c)| c.as_str())
            .filter(|c| !echoed.contains(c))
            .collect();
        if !missing.is_empty() {
            anyhow::bail!(
                "t8407 batch {}/{} did not echo {} of {} requested code(s): {}. The request named \
                 them, so this is a request/framing fault, not market state — refusing rather \
                 than emitting a silently re-composed universe.",
                i + 1,
                batches.len(),
                missing.len(),
                batch.len(),
                missing.join(",")
            );
        }
    }
    Ok(TodayOpens { opens, drops })
}

/// Build the SDK from an already-resolved config. Kept tiny and separate so `resolve` never
/// touches credential resolution — that happened in `config_from_env`, before the runtime.
fn sdk_for(config: &ls_core::config::LsConfig) -> anyhow::Result<Box<ls_sdk::LsSdk>> {
    ls_sdk::LsSdk::new(config.clone())
        .map(Box::new)
        .map_err(|e| anyhow::anyhow!("building the SDK for the live today_open fetch: {e}"))
}

/// Report unusable-open symbols by CAUSE. An unparseable open is a wire-shape change and gets
/// its own loud line — reporting it as "pre-open" would send the operator off to wait for a
/// market state that has already arrived.
fn report_open_drops(drops: &OpenDrops) {
    if !drops.not_yet_open.is_empty() {
        eprintln!(
            "mount-universe: {} symbol(s) were quoted with no open yet (pre-open or never traded) \
             and are NOT candidates today: {}",
            drops.not_yet_open.len(),
            drops.not_yet_open.join(",")
        );
    }
    if !drops.unparseable.is_empty() {
        eprintln!(
            "mount-universe: WIRE-SHAPE WARNING — {} symbol(s) were quoted with an open that is \
             not an integer. This is NOT market state; t8407's `open` shape may have changed. \
             Affected: {}",
            drops.unparseable.len(),
            drops.unparseable.join(",")
        );
    }
}

/// The outcome of binding a universe-metadata artifact to the head's identity.
///
/// A closed type rather than a bare `Option` (R15/KTD14). The `Option` this
/// replaces let *refusal* and *empty result* share one representation: `None`
/// meant both "this head legitimately has no metadata" and "nobody supplied any
/// and nobody checked", and the second reading silently dropped the tradability
/// gate. Both artifact-absent arms now refuse, so **every variant of this type
/// carries records** — there is no value here that means "the gate is off", and
/// the mount cannot express one.
///
/// The two variants are the observable residue the old silence erased: which
/// binding produced the gate is now typed rather than inferable only from a
/// stderr line.
enum MetadataBinding {
    /// The head run is metadata-driven and the supplied artifact's content hash
    /// is the one that run was built from.
    HeadBound(HashMap<String, InstrumentMetadata>),
    /// An artifact was supplied against a head carrying no
    /// `universe_metadata_hash`. The gate applies where the head's did not —
    /// narrower than a mismatch, so it warns rather than refusing (see the
    /// `None` arm in [`resolve`]), and this variant records that divergence.
    UngatedHead(HashMap<String, InstrumentMetadata>),
}

impl MetadataBinding {
    /// The bound records. Both variants carry them; the distinction is
    /// provenance, not presence.
    fn records(&self) -> &HashMap<String, InstrumentMetadata> {
        match self {
            Self::HeadBound(records) | Self::UngatedHead(records) => records,
        }
    }
}

/// Resolve the mount universe for `cfg.session_date` from the catalog.
///
/// # Errors
///
/// If the catalog is absent or unreadable, the head params cannot be resolved, no
/// universe-metadata artifact binds to the head (see [`MetadataBinding`] — BOTH
/// artifact-absent arms refuse), the metadata artifact fails validation, or no
/// symbol survives selection.
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

    let metadata: MetadataBinding = match (&cfg.metadata_path, &head_metadata_hash) {
        (None, Some(expected)) => anyhow::bail!(
            "mount-universe refused: the head is METADATA-DRIVEN (its run carries \
             universe_metadata_hash {expected}) but LS_MOUNT_UNIVERSE_METADATA is unset — \
             producing the universe without it silently drops the tradability gate and trades \
             symbols the head excluded. Set it to the artifact that run was built from."
        ),
        // R15 — this arm used to be `=> None`, and `None` proceeded. No artifact
        // and no head hash to bind against is not a legitimate "metadata-less
        // head": it is the mount producing a universe whose eligibility nothing
        // vouched for. Every candidate maps to `CandidateMeta::Untagged`, the
        // tradability gate disappears, and the failure is invisible in the emitted
        // artifact — the module comment above stated exactly that harm while the
        // code did it anyway, with no diagnostic at all.
        (None, None) => anyhow::bail!(
            "mount-universe refused: no universe-metadata artifact to bind — \
             LS_MOUNT_UNIVERSE_METADATA is unset AND the head run carries no \
             universe_metadata_hash. Producing the universe anyway maps every candidate to \
             Untagged, which drops the tradability gate silently: the session would trade \
             symbols on no eligibility evidence at all, and the emitted rows would not show \
             it. Point LS_MOUNT_UNIVERSE_METADATA at a current universe-metadata artifact \
             (lab/config/universe-metadata-*.json, the one session-morning.sh exports, or a \
             fresh capture): a head carrying no universe_metadata_hash accepts ANY valid \
             artifact and warns that the gate applies where the head's did not. Re-run the \
             head against an artifact only if you want the binding to be head-bound."
        ),
        (Some(path), _) => {
            let artifact = UniverseMetadata::load(path).map_err(|e| anyhow::anyhow!(e))?;
            artifact.validate().map_err(|errs| {
                anyhow::anyhow!("metadata artifact failed validation:\n  - {}", errs.join("\n  - "))
            })?;
            let hash = artifact.content_hash();
            let records: HashMap<String, InstrumentMetadata> =
                artifact.records.into_iter().map(|r| (r.shcode.clone(), r)).collect();
            match &head_metadata_hash {
                Some(expected) if expected != &hash => anyhow::bail!(
                    "mount-universe refused: metadata artifact hash mismatch — the head run was \
                     built from {expected} but LS_MOUNT_UNIVERSE_METADATA resolves to {hash}. A \
                     re-capture between the head run and now re-tiers symbols; point at the ONE \
                     artifact the head used."
                ),
                // The hashes agree: the artifact IS the one the head was built from.
                Some(_) => MetadataBinding::HeadBound(records),
                // Narrower than a mismatch, and deliberately not hardened (KTD14):
                // the gate applies where the head's did not, which is a divergence
                // worth warning about but not a reason to refuse a run whose head
                // never claimed a metadata identity.
                None => {
                    eprintln!(
                        "mount-universe: warning — LS_MOUNT_UNIVERSE_METADATA is set but the head \
                         run carries no universe_metadata_hash (not a metadata-driven head); the \
                         tradability gate will apply where the head's did not"
                    );
                    MetadataBinding::UngatedHead(records)
                }
            }
        }
    };

    let all_bars = read_all_bars(&catalog).await.map_err(|e| anyhow::anyhow!(e))?;
    let instruments = read_all_instruments(&catalog).await.map_err(|e| anyhow::anyhow!(e))?;

    let (daily_by_inst, open_vol_by_inst) = index_catalog(&all_bars, &params);

    // On a session morning the opens cannot come from the catalog (see `TodayOpenSource`), so
    // fetch them live for every instrument that has any prior daily history. It has to be the
    // whole prior-history set, not the eventual selection: `select_universe` applies a gap
    // floor, which reads `today_open`, so the open is an INPUT to selection and cannot be
    // resolved after it.
    // Exactly the set `build_candidates` could still admit once an open exists: it needs a
    // prior session, and nothing else from the catalog. Computed once — the live fetch asks
    // for these, and whatever comes back short is reported against the same set.
    // A symbol is eligible only if its prior session is RECENT. On the catalog path the
    // session-date bar requirement made this implicit; the live path must state it, or a
    // months-stale symbol pairs a live open against an ancient close and its "gap" is a
    // multi-session return that trivially clears the head's overnight-gap floor.
    let prior_eligible: Vec<(InstrumentId, String)> = instruments
        .iter()
        .map(|i| i.id())
        .filter(|id| {
            daily_by_inst.get(id).is_some_and(|d| {
                select_prior(d, cfg.session_date).is_some_and(|p| {
                    matches!(&cfg.today_open_source, TodayOpenSource::Catalog)
                        || (cfg.session_date - kst_date_of(p)).num_days()
                            <= MAX_PRIOR_STALENESS_DAYS
                })
            })
        })
        .map(|id| {
            let sym = id.to_string();
            (id, shcode_of(&sym).to_string())
        })
        .collect();

    let today_opens: Option<HashMap<InstrumentId, i64>> = match &cfg.today_open_source {
        TodayOpenSource::Catalog => None,
        TodayOpenSource::LiveQuote { config, .. } => {
            // Refuse before the opening auction has set an open. t8407 is a 현재가 board and
            // serves a populated snapshot outside the session (verified: it answers with data
            // after the 15:30 close), so before 09:00 its `open` is the PREVIOUS session's and
            // is a perfectly positive integer. `open > 0` cannot tell the two apart, so the
            // clock has to.
            let now = now_kst();
            if now.date() == cfg.session_date && now.time() < nautilus_ls::rules::KRX_REGULAR_OPEN {
                anyhow::bail!(
                    "mount-universe: it is {} KST and the KRX opening auction has not set an open \
                     yet. t8407 answers outside the session with the PREVIOUS session's snapshot, \
                     whose open is a positive number, so producing now would silently resolve the \
                     universe against yesterday's opens. Re-run after {}.",
                    now.time().format("%H:%M"),
                    nautilus_ls::rules::KRX_REGULAR_OPEN.format("%H:%M")
                );
            }
            let wanted = &prior_eligible;
            if wanted.is_empty() {
                anyhow::bail!(
                    "mount-universe: no instrument in the catalog has a daily bar within {} days \
                     before {} — there is no recent prior session to derive prior_close/prior_atr \
                     from, so a live today_open would only pair with stale history. Ingest the \
                     catalog forward through the previous session first.",
                    MAX_PRIOR_STALENESS_DAYS,
                    cfg.session_date
                );
            }
            eprintln!(
                "mount-universe: session date is TODAY (KST) — fetching today_open live via \
                 t8407 for {} symbol(s); the catalog cannot hold an in-session daily bar",
                wanted.len()
            );
            let fetched = fetch_today_opens(sdk_for(config)?.as_ref(), wanted).await?;
            report_open_drops(&fetched.drops);
            Some(fetched.opens)
        }
    };

    let candidates = build_candidates_with_today_open(
        &instruments,
        &daily_by_inst,
        &open_vol_by_inst,
        &params,
        cfg.session_date,
        // Always `Some` by construction now: every `MetadataBinding` carries
        // records, so the mount can no longer hand the builder the `None` that
        // used to mean "gate off". The shared builder keeps its `Option` because
        // the BACKTEST path has a legitimate metadata-less case; the mount does
        // not, and that asymmetry is the whole of R15.
        Some(metadata.records()),
        today_opens.as_ref(),
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
            // The "why is it empty" hint must match the path that was actually taken. On the
            // live path the session date's daily bar is EXPECTED to be absent, so pointing the
            // operator at ingest would send them chasing a non-problem in the one window they
            // have. Note there is no "opens resolved but no prior history" arm: every id in
            // `opens` came from `prior_eligible`, which already required a recent prior, so a
            // non-empty `opens` always yields at least that many candidates.
            match (candidates.is_empty(), &today_opens) {
                (true, Some(_)) => {
                    " No symbol had a usable live open — see the per-cause lines above. If they \
                     read 'no open yet', the opening auction has not printed for these names; if \
                     they read WIRE-SHAPE, t8407's `open` field changed and waiting will not help."
                }
                (true, None) => {
                    " No candidate had both a daily bar ON the session date and a prior one — \
                     is the session date's daily bar ingested? (`today_open` comes from it.)"
                }
                (false, _) => "",
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

    /// The live `today_open` fetch, against a wiremock gateway.
    ///
    /// This is the one gateway call the producer makes, and it decides what an attended live
    /// session buys: `--mount` runs no selection of its own. `fetch_today_opens` takes an
    /// already-built `&LsSdk` precisely so its batching, fixed-width packing, echo-keying and
    /// open-parsing are reachable offline — `LsConfig::base_url` is the only injection point in
    /// the stack and is not settable from a lane env file, so `mock_config(&server.uri())` is
    /// the supported way in.
    ///
    /// These live inside the crate rather than under `tests/`: `fetch_today_opens`, `TodayOpens`
    /// and `OpenDrops` are `pub(crate)` on purpose (an external caller has no business fetching
    /// opens outside `resolve`), and an integration test is a separate crate that cannot reach
    /// them. Widening the surface to `pub` to relocate the tests would trade the invariant for
    /// the test file's location.
    mod live_open_fetch {
        use super::*;
        use ls_sdk::LsSdk;
        use ls_sdk_test_support::{mock_config, mount_token};
        use serde_json::json;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

        fn json_response(body: serde_json::Value) -> ResponseTemplate {
            ResponseTemplate::new(200)
                .set_body_string(body.to_string())
                .insert_header("content-type", "application/json")
        }

        /// Mount the OAuth stub plus a fixed `t8407` response.
        async fn mount_t8407(server: &MockServer, body: serde_json::Value) {
            mount_token(server).await;
            Mock::given(method("POST"))
                .and(path("/stock/market-data"))
                .and(header("tr_cd", "t8407"))
                .respond_with(json_response(body))
                .mount(server)
                .await;
        }

        /// A `t8407` responder that echoes back exactly the codes the request packed, each with
        /// a usable open. It reads the packing the same fixed six-character way the gateway
        /// does, so a mis-framed batch surfaces as a missing-echo refusal rather than passing.
        struct EchoPackedCodes;

        impl Respond for EchoPackedCodes {
            fn respond(&self, req: &Request) -> ResponseTemplate {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let packed = body["t8407InBlock"]["shcode"].as_str().unwrap();
                let rows: Vec<serde_json::Value> = packed
                    .as_bytes()
                    .chunks(6)
                    .map(|c| json!({ "shcode": std::str::from_utf8(c).unwrap(), "open": "10000" }))
                    .collect();
                json_response(json!({ "rsp_cd": "00000", "t8407OutBlock1": rows }))
            }
        }

        async fn mount_echo(server: &MockServer) {
            mount_token(server).await;
            Mock::given(method("POST"))
                .and(path("/stock/market-data"))
                .and(header("tr_cd", "t8407"))
                .respond_with(EchoPackedCodes)
                .mount(server)
                .await;
        }

        fn sdk(server: &MockServer) -> LsSdk {
            LsSdk::new(mock_config(&server.uri())).unwrap()
        }

        fn inst(code: &str) -> InstrumentId {
            InstrumentId::from(format!("{code}.XKRX").as_str())
        }

        fn wanted(codes: &[&str]) -> Vec<(InstrumentId, String)> {
            codes.iter().map(|c| (inst(c), (*c).to_string())).collect()
        }

        /// `n` distinct valid six-digit codes, in a stable order.
        fn wanted_n(n: usize) -> Vec<(InstrumentId, String)> {
            (1..=n)
                .map(|i| {
                    let code = format!("{i:06}");
                    (inst(&code), code)
                })
                .collect()
        }

        /// The `t8407` request bodies the server actually received, in order. Filtered by path
        /// so the OAuth token call never counts as a quote request.
        async fn t8407_bodies(server: &MockServer) -> Vec<serde_json::Value> {
            server
                .received_requests()
                .await
                .expect("the mock server records requests")
                .iter()
                .filter(|r| r.url.path() == "/stock/market-data")
                .map(|r| serde_json::from_slice(&r.body).unwrap())
                .collect()
        }

        /// Every path the server was hit on, in order — including the OAuth token endpoint.
        /// A pre-packing refusal must not even authenticate.
        async fn all_request_paths(server: &MockServer) -> Vec<String> {
            server
                .received_requests()
                .await
                .expect("the mock server records requests")
                .iter()
                .map(|r| r.url.path().to_string())
                .collect()
        }

        // --- batching / framing -------------------------------------------------------

        /// A full batch is one call, and `nrec` must reach the wire as a JSON NUMBER — a
        /// quoted `"25"` is exactly the shape that earns `IGW40011`.
        #[tokio::test]
        async fn a_full_batch_of_25_is_one_request_whose_nrec_is_a_json_number() {
            let server = MockServer::start().await;
            mount_echo(&server).await;
            let w = wanted_n(T8407_BATCH);

            let got = fetch_today_opens(&sdk(&server), &w).await.unwrap();
            assert_eq!(got.opens.len(), T8407_BATCH);

            let bodies = t8407_bodies(&server).await;
            assert_eq!(bodies.len(), 1, "25 codes fit one batch");
            assert_eq!(
                bodies[0]["t8407InBlock"]["nrec"],
                json!(25),
                "nrec must serialize as a JSON number, not a string"
            );
            assert_eq!(
                bodies[0]["t8407InBlock"]["shcode"].as_str().unwrap().len(),
                150,
                "25 codes packed at a fixed six-character width"
            );
        }

        /// The chunk boundary is where a packing bug would silently drop or duplicate a symbol,
        /// and a smaller pool does not shrink the universe — selection is top-N, so it refills
        /// from lower-ranked names. Concatenating the two packings must reproduce the input.
        #[tokio::test]
        async fn twenty_six_codes_split_into_25_then_1_losing_nothing_at_the_boundary() {
            let server = MockServer::start().await;
            mount_echo(&server).await;
            let w = wanted_n(T8407_BATCH + 1);

            let got = fetch_today_opens(&sdk(&server), &w).await.unwrap();
            assert_eq!(got.opens.len(), T8407_BATCH + 1);

            let bodies = t8407_bodies(&server).await;
            assert_eq!(bodies.len(), 2, "26 codes need a second batch");
            assert_eq!(bodies[0]["t8407InBlock"]["nrec"], json!(25));
            assert_eq!(bodies[1]["t8407InBlock"]["nrec"], json!(1));

            let packed: String = bodies
                .iter()
                .map(|b| b["t8407InBlock"]["shcode"].as_str().unwrap())
                .collect();
            let expected: String = w.iter().map(|(_, c)| c.as_str()).collect();
            assert_eq!(packed, expected, "every requested code is packed exactly once, in order");
        }

        // --- response handling --------------------------------------------------------

        /// The regression guard for the removed `rsp_cd != "00000"` re-check. `T8407Response`
        /// is `#[serde(default)]`, so an envelope that omits `rsp_cd` entirely deserializes to
        /// `""` — which `Inner::post` already classifies as success. Re-testing for `"00000"`
        /// here could only reject a response the SDK accepted.
        #[tokio::test]
        async fn an_absent_rsp_cd_still_resolves_opens() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({ "t8407OutBlock1": [{ "shcode": "005930", "open": "71800" }] }),
            )
            .await;

            let got = fetch_today_opens(&sdk(&server), &wanted(&["005930"])).await.unwrap();
            assert_eq!(got.opens.get(&inst("005930")), Some(&71_800));
        }

        /// `00136` is success-WITH-data, not a failure. The SDK's read-success set already
        /// admits it; the producer must not narrow that.
        #[tokio::test]
        async fn an_informational_00136_still_resolves_opens() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({
                    "rsp_cd": "00136",
                    "t8407OutBlock1": [{ "shcode": "005930", "open": "71800" }]
                }),
            )
            .await;

            let got = fetch_today_opens(&sdk(&server), &wanted(&["005930"])).await.unwrap();
            assert_eq!(got.opens.get(&inst("005930")), Some(&71_800));
        }

        /// Zero and blank are market state: pre-open, or halted before it ever traded. Both
        /// drop the symbol — never defaulted — and both belong in `not_yet_open`.
        #[tokio::test]
        async fn a_zero_or_blank_open_drops_the_symbol_as_not_yet_open() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({
                    "rsp_cd": "00000",
                    "t8407OutBlock1": [
                        { "shcode": "005930", "open": "0" },
                        { "shcode": "000660", "open": "" }
                    ]
                }),
            )
            .await;

            let got = fetch_today_opens(&sdk(&server), &wanted(&["005930", "000660"])).await.unwrap();
            assert!(got.opens.is_empty(), "neither symbol has a usable open");
            assert_eq!(got.drops.not_yet_open, vec!["005930".to_string(), "000660".to_string()]);
            assert!(got.drops.unparseable.is_empty(), "market state is not a wire-shape fault");
        }

        /// A non-integer open is a WIRE-SHAPE change, not market state, and must never land in
        /// the `not_yet_open` bucket: that conflation sends the operator off to wait for a
        /// market state that has already arrived.
        ///
        /// The shape that reaches this arm is a decimal-bearing *string* — `string_or_number`
        /// passes a JSON string through verbatim (`visit_str`), so a gateway that started
        /// quoting `"57900.0"` lands here.
        #[tokio::test]
        async fn a_decimal_open_string_is_reported_as_unparseable_not_as_pre_open() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({
                    "rsp_cd": "00000",
                    "t8407OutBlock1": [{ "shcode": "005930", "open": "57900.0" }]
                }),
            )
            .await;

            let got = fetch_today_opens(&sdk(&server), &wanted(&["005930"])).await.unwrap();
            assert!(got.opens.is_empty());
            assert!(got.drops.not_yet_open.is_empty(), "a wire-shape fault is not pre-open");
            assert_eq!(got.drops.unparseable.len(), 1);
            assert!(
                got.drops.unparseable[0].starts_with("005930(open="),
                "the report names the code AND the raw value that failed to parse: {:?}",
                got.drops.unparseable[0]
            );
            assert!(got.drops.unparseable[0].contains("57900.0"));
        }

        /// An UNQUOTED JSON float is a different shape and does NOT reach the unparseable arm:
        /// `string_or_number`'s `visit_f64` renders it with `f64::to_string()`, which drops a
        /// zero fraction (`57900.0` → `"57900"`). Pinned because it is the difference between
        /// a symbol trading and a symbol being reported as a wire-shape fault, and it is
        /// invisible from this file — a change to that helper would silently move the boundary.
        #[tokio::test]
        async fn an_unquoted_json_float_open_normalizes_to_an_integer_and_still_resolves() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({
                    "rsp_cd": "00000",
                    "t8407OutBlock1": [{ "shcode": "005930", "open": 57900.0 }]
                }),
            )
            .await;

            let got = fetch_today_opens(&sdk(&server), &wanted(&["005930"])).await.unwrap();
            assert_eq!(got.opens.get(&inst("005930")), Some(&57_900));
            assert!(got.drops.unparseable.is_empty());
        }

        /// A zero-padded open is still an integer.
        #[tokio::test]
        async fn a_zero_padded_open_parses() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({
                    "rsp_cd": "00000",
                    "t8407OutBlock1": [{ "shcode": "005930", "open": "00057900" }]
                }),
            )
            .await;

            let got = fetch_today_opens(&sdk(&server), &wanted(&["005930"])).await.unwrap();
            assert_eq!(got.opens.get(&inst("005930")), Some(&57_900));
        }

        /// A number-typed echo loses the leading zeros (`string_or_number` renders 5930 as
        /// `"5930"`), so without the `{:0>6}` re-pad every 0-prefixed KRX code would fail to
        /// key — and then fail the missing-echo check, refusing a perfectly good batch.
        #[tokio::test]
        async fn a_number_typed_shcode_echo_is_re_padded_to_six_digits() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({
                    "rsp_cd": "00000",
                    "t8407OutBlock1": [{ "shcode": 5930, "open": "71800" }]
                }),
            )
            .await;

            let got = fetch_today_opens(&sdk(&server), &wanted(&["005930"])).await.unwrap();
            assert_eq!(got.opens.get(&inst("005930")), Some(&71_800));
        }

        // --- fail-closed ---------------------------------------------------------------

        /// The request named the code, so silence is a request/framing fault, not market
        /// state. Dropping it would silently re-compose the traded universe.
        #[tokio::test]
        async fn a_requested_code_the_gateway_never_echoes_is_a_hard_error() {
            let server = MockServer::start().await;
            mount_t8407(
                &server,
                json!({
                    "rsp_cd": "00000",
                    "t8407OutBlock1": [{ "shcode": "005930", "open": "71800" }]
                }),
            )
            .await;

            let err = fetch_today_opens(&sdk(&server), &wanted(&["005930", "000660"]))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("did not echo"), "{err}");
            assert!(err.contains("000660"), "the refusal names the missing code: {err}");
            assert!(!err.contains("005930"), "and not the one that answered: {err}");
        }

        /// `shcode` is a fixed-width packing, so one off-length code shifts every code after
        /// it and the gateway quotes symbols nobody asked for. Refuse before packing.
        #[tokio::test]
        async fn a_non_six_digit_code_is_refused_before_any_request_is_sent() {
            let server = MockServer::start().await;
            mount_echo(&server).await;
            let w = wanted(&["005930", "5930"]);

            let err = fetch_today_opens(&sdk(&server), &w).await.unwrap_err().to_string();
            assert!(err.contains("not six ASCII digits"), "{err}");
            assert!(err.contains("5930"), "{err}");
            assert_eq!(
                all_request_paths(&server).await,
                Vec::<String>::new(),
                "the refusal precedes the gateway entirely — not even a token is fetched"
            );
        }

        /// Two instruments behind one code means one of them can never receive an open, since
        /// the echo is keyed by code. Refuse rather than silently starve one.
        #[tokio::test]
        async fn two_instruments_sharing_one_shcode_are_refused_before_any_request() {
            let server = MockServer::start().await;
            mount_echo(&server).await;
            let w = vec![
                (InstrumentId::from("005930.XKRX"), "005930".to_string()),
                (InstrumentId::from("005930.XNXT"), "005930".to_string()),
            ];

            let err = fetch_today_opens(&sdk(&server), &w).await.unwrap_err().to_string();
            assert!(err.contains("maps to two instruments"), "{err}");
            assert_eq!(
                all_request_paths(&server).await,
                Vec::<String>::new(),
                "the refusal precedes the gateway entirely — not even a token is fetched"
            );
        }
    }
}
