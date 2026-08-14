//! `UniverseMetadata` — the per-symbol reference-data record + artifact (U1),
//! plus the pure tier-assignment, tradability-gate, and stratified-sample logic.
//!
//! Every attribute resolves to one of `{Value, Proxy, Unavailable}` and the
//! resolution is recorded per symbol (R4) — a proxy or missing value is never
//! silently defaulted to a confident boolean. The four-cell stratification
//! partition is **pre-registered and deterministic** (R6, Key Decisions):
//!
//! 1. blue-chip KOSPI   = KOSPI  × top cap tier
//! 2. mid-cap KOSPI     = KOSPI  × mid cap tier
//! 3. KOSDAQ mid/small  = KOSDAQ × on-board cap tiers
//! 4. small-cap (excl.) = any market class × below the `t1444` cap board
//!
//! A below-board KOSDAQ symbol lands in cell 4, not cell 3 — no symbol is
//! claimable by two cells. Cap-tier boundaries are fixed cap-rank quantiles
//! computed once over the capture artifact and recorded in provenance before
//! ingest; boundaries set after seeing trade density could move trades between
//! tiers and flip the U6 verdict.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How an attribute value was obtained (R4). Serialized adjacently tagged so a
/// missing value is an explicit `{"resolution":"unavailable"}` on disk, never a
/// defaulted confident value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resolution", content = "value", rename_all = "snake_case")]
pub enum Resolved<T> {
    /// Read directly from its authoritative source.
    Value(T),
    /// Derived from a proxy source (e.g. ETF holdings for index membership).
    Proxy(T),
    /// No source reached it — recorded, never defaulted (R4).
    Unavailable,
}

impl<T> Resolved<T> {
    /// The carried value when resolved (`Value` or `Proxy`).
    pub fn resolved(&self) -> Option<&T> {
        match self {
            Resolved::Value(v) | Resolved::Proxy(v) => Some(v),
            Resolved::Unavailable => None,
        }
    }

    /// Whether no source reached this attribute.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Resolved::Unavailable)
    }
}

/// KRX market segment, from the `t8430` master `gubun`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketClass {
    Kospi,
    Kosdaq,
}

/// Cap tier over the `t1444` ranked board. The board is bounded (no
/// price/volume filter, only an `idx` cursor), so it cannot reach the small-cap
/// tail: `BelowBoard` is the small-cap tier taken by **exclusion** from the
/// whole `t8430` master (R2) — its `market_cap` stays `Unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapTier {
    /// Top cap-rank quantile of the on-board names in its market class.
    Top,
    /// The remaining on-board names in its market class.
    Mid,
    /// Below the bounded cap board — small-cap by exclusion (R2).
    BelowBoard,
}

/// Liquidity tier from daily turnover (value traded, KRW). A conditioner tag
/// for Turn N+1 segmentation, never a stratum boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiquidityTier {
    High,
    Mid,
    Low,
    /// Turnover unresolved at tagging time (e.g. the deferred `t1463` walk).
    Unknown,
}

/// v1 liquidity-tier boundaries (KRW daily turnover) — segmentation labels for
/// the conditioner tag only, pre-registered here so the tag is deterministic.
pub const LIQUIDITY_HIGH_FLOOR_KRW: f64 = 50_000_000_000.0;
/// Mid/low boundary (KRW daily turnover). See [`LIQUIDITY_HIGH_FLOOR_KRW`].
pub const LIQUIDITY_MID_FLOOR_KRW: f64 = 5_000_000_000.0;

/// Index membership via the KODEX ETF-holdings proxy (`t1904`): tracked but
/// not identical to official KOSPI200/KOSDAQ150 constituency, so membership is
/// carried as [`Resolved::Proxy`] (AE4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexMembership {
    Kospi200,
    Kosdaq150,
    /// Absent from both ETF holdings — `none (proxy)`, not a confident false.
    NotMember,
}

/// A surveillance / tradability designation category (R3). `t1405` carries the
/// halt/caution/warning/risk/overheated designations; `t1404` carries the
/// managed/caution board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignationKind {
    /// Trading halt (거래정지).
    Halt,
    /// Managed issue (관리종목).
    Managed,
    /// Investment caution (투자주의).
    Caution,
    /// Investment warning (투자경고).
    Warning,
    /// Investment risk (투자위험).
    Risk,
    /// Short-term overheated (단기과열).
    Overheated,
}

/// A designation currently carried by a symbol, with the TR that reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Designation {
    /// The designation category.
    pub kind: DesignationKind,
    /// The reporting TR (`t1405` or `t1404`).
    pub source_tr: String,
}

/// The tradability gate (R3, KTD3): a hard filter, not a conditioner tag. Any
/// current designation excludes the symbol from the tradeable set (AE3).
pub fn is_tradable(designation: &Option<Designation>) -> bool {
    designation.is_none()
}

/// Assign the liquidity conditioner tag from a turnover attribute. Unresolved
/// turnover tags `Unknown` — never a defaulted `Low` (R4).
pub fn assign_liquidity_tier(turnover: &Resolved<f64>) -> LiquidityTier {
    match turnover.resolved() {
        None => LiquidityTier::Unknown,
        Some(v) if *v >= LIQUIDITY_HIGH_FLOOR_KRW => LiquidityTier::High,
        Some(v) if *v >= LIQUIDITY_MID_FLOOR_KRW => LiquidityTier::Mid,
        Some(_) => LiquidityTier::Low,
    }
}

/// The ingest-side liquidity floor (R5): gates only on **resolved**
/// (`Value`/`Proxy`) turnover. A symbol whose turnover is `Unavailable` at
/// capture time is admitted with its resolution recorded — fail-closed would
/// gut exactly the small-cap stratum the test targets; the backtest-side floor
/// re-gates on daily-bar `prior_turnover` once bars exist for it.
pub fn passes_liquidity_floor(turnover: &Resolved<f64>, floor_krw: f64) -> bool {
    match turnover.resolved() {
        Some(v) => *v >= floor_krw,
        None => true,
    }
}

/// One symbol's reference-data record (U1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstrumentMetadata {
    /// 6-digit KRX shcode.
    pub shcode: String,
    /// KOSPI/KOSDAQ, from the `t8430` master `gubun` (R1).
    pub market_class: MarketClass,
    /// Market cap from the `t1444` ranked board; `Unavailable` below the board.
    pub market_cap: Resolved<f64>,
    /// The pre-registered cap tier (see [`assign_cap_tiers`]).
    pub cap_tier: CapTier,
    /// Daily turnover (KRW). `Unavailable` in Turn N (the `t1463` walk is
    /// deferred, R2); the backtest join derives liquidity from daily bars.
    pub turnover: Resolved<f64>,
    /// The liquidity conditioner tag (see [`assign_liquidity_tier`]).
    pub liquidity_tier: LiquidityTier,
    /// Index membership via the KODEX ETF-holdings proxy (AE4).
    pub index_membership: Resolved<IndexMembership>,
    /// Whether a single-stock future lists the symbol as underlying (`t2522`).
    pub has_derivative: Resolved<bool>,
    /// A current surveillance designation, when one exists (R3).
    pub designation: Option<Designation>,
    /// The hard tradability gate's verdict (= `designation.is_none()`, R3).
    pub tradable: bool,
}

/// One of the four pre-registered stratification cells (R6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stratum {
    /// KOSPI × top cap tier.
    KospiBlueChip,
    /// KOSPI × mid cap tier.
    KospiMid,
    /// KOSDAQ × on-board cap tiers.
    KosdaqOnBoard,
    /// Any market class × below the cap board (small-cap by exclusion).
    SmallCapExclusion,
}

impl Stratum {
    /// All four cells, in their pre-registered order.
    pub const ALL: [Stratum; 4] = [
        Stratum::KospiBlueChip,
        Stratum::KospiMid,
        Stratum::KosdaqOnBoard,
        Stratum::SmallCapExclusion,
    ];

    /// A stable display label (used in reports and pin files).
    pub fn label(self) -> &'static str {
        match self {
            Stratum::KospiBlueChip => "kospi_blue_chip",
            Stratum::KospiMid => "kospi_mid",
            Stratum::KosdaqOnBoard => "kosdaq_on_board",
            Stratum::SmallCapExclusion => "small_cap_exclusion",
        }
    }
}

/// The pre-registered four-cell partition (R6): total over every
/// `(market_class, cap_tier)` pair, no cell overlap — the exclusion cell wins
/// on `BelowBoard` regardless of market class.
pub fn stratum_of(market_class: MarketClass, cap_tier: CapTier) -> Stratum {
    match (market_class, cap_tier) {
        (_, CapTier::BelowBoard) => Stratum::SmallCapExclusion,
        (MarketClass::Kospi, CapTier::Top) => Stratum::KospiBlueChip,
        (MarketClass::Kospi, CapTier::Mid) => Stratum::KospiMid,
        (MarketClass::Kosdaq, CapTier::Top | CapTier::Mid) => Stratum::KosdaqOnBoard,
    }
}

/// The default top-tier cap-rank quantile: the top half of each market class's
/// on-board names is `Top`, the rest `Mid`.
pub const DEFAULT_CAP_TOP_QUANTILE: f64 = 0.5;

/// One market class's recorded cap-tier cutoff (provenance, Key Decisions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapCutoff {
    /// The market class the cutoff applies to.
    pub market_class: MarketClass,
    /// How many of its symbols sat on the cap board at capture.
    pub on_board: usize,
    /// How many of those took the `Top` tier (`ceil(quantile × on_board)`).
    pub top_count: usize,
    /// The smallest market cap that still made `Top` (KRW), when any did.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub boundary_cap: Option<f64>,
}

/// Assign cap tiers in place (U1): per market class, rank the records with a
/// **resolved** market cap descending (cap-rank), take the top
/// `ceil(quantile × on_board)` as [`CapTier::Top`] and the rest as
/// [`CapTier::Mid`]; records with `Unavailable` cap take [`CapTier::BelowBoard`]
/// (small-cap by exclusion, R2). Ties break by shcode so the assignment is
/// deterministic. Returns the per-market cutoffs for provenance.
pub fn assign_cap_tiers(records: &mut [InstrumentMetadata], top_quantile: f64) -> Vec<CapCutoff> {
    let mut cutoffs = Vec::new();
    for market in [MarketClass::Kospi, MarketClass::Kosdaq] {
        // Rank this market's on-board records by cap descending, shcode tiebreak.
        let mut ranked: Vec<usize> = records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.market_class == market && r.market_cap.resolved().is_some())
            .map(|(i, _)| i)
            .collect();
        ranked.sort_by(|a, b| {
            let cap = |i: usize| *records[i].market_cap.resolved().expect("filtered resolved");
            cap(*b)
                .partial_cmp(&cap(*a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| records[*a].shcode.cmp(&records[*b].shcode))
        });
        let on_board = ranked.len();
        let top_count = (top_quantile * on_board as f64).ceil() as usize;
        for (rank, idx) in ranked.iter().enumerate() {
            records[*idx].cap_tier = if rank < top_count { CapTier::Top } else { CapTier::Mid };
        }
        let boundary_cap = top_count
            .checked_sub(1)
            .and_then(|last| ranked.get(last))
            .and_then(|i| records[*i].market_cap.resolved().copied());
        cutoffs.push(CapCutoff { market_class: market, on_board, top_count, boundary_cap });
    }
    // Below-board records (unresolved cap) take the exclusion tier, any market.
    for r in records.iter_mut() {
        if r.market_cap.resolved().is_none() {
            r.cap_tier = CapTier::BelowBoard;
        }
    }
    cutoffs
}

/// Draw a tier-stratified sample (R6): up to `per_stratum` **tradable** symbols
/// from each of the four cells. Within a cell the candidates are ordered
/// deterministically (cap descending where resolved, then shcode) and sampled
/// at evenly spaced indices — a stride, not a head-take, so an on-board cell is
/// not all top-of-tier and the exclusion cell is not all one listing era. A
/// thin cell contributes everything it has (graceful degradation).
pub fn stratify(
    records: &[InstrumentMetadata],
    per_stratum: usize,
) -> BTreeMap<Stratum, Vec<String>> {
    let mut cells: BTreeMap<Stratum, Vec<&InstrumentMetadata>> =
        Stratum::ALL.iter().map(|s| (*s, Vec::new())).collect();
    for r in records.iter().filter(|r| r.tradable) {
        cells
            .get_mut(&stratum_of(r.market_class, r.cap_tier))
            .expect("all strata pre-seeded")
            .push(r);
    }
    let mut out = BTreeMap::new();
    for (stratum, mut members) in cells {
        members.sort_by(|a, b| {
            let cap = |r: &InstrumentMetadata| r.market_cap.resolved().copied().unwrap_or(0.0);
            cap(b)
                .partial_cmp(&cap(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.shcode.cmp(&b.shcode))
        });
        let n = members.len();
        let selected: Vec<String> = if n <= per_stratum {
            members.iter().map(|r| r.shcode.clone()).collect()
        } else {
            (0..per_stratum).map(|i| members[i * n / per_stratum].shcode.clone()).collect()
        };
        out.insert(stratum, selected);
    }
    out
}

/// The R9 conditioner-tag set: the five per-symbol tags that ride the
/// universe-accept envelope and propagate onto every resulting trade (KTD4), so
/// Turn N+1 can segment expectancy by any axis with no further reference-data
/// calls. Kept off the envelope's numeric `values` map; the stratification
/// axis is derivable via [`ConditionerTags::stratum`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConditionerTags {
    /// The pre-registered cap tier.
    pub cap_tier: CapTier,
    /// The liquidity conditioner tag (daily-bar derived at the backtest join).
    pub liquidity_tier: LiquidityTier,
    /// KOSPI/KOSDAQ.
    pub market_class: MarketClass,
    /// Index membership (ETF-holdings proxy, AE4).
    pub index_membership: Resolved<IndexMembership>,
    /// Single-stock-futures underlying flag.
    pub has_derivative: Resolved<bool>,
}

impl ConditionerTags {
    /// The stratification cell these tags imply (the U6 bucketing axis).
    pub fn stratum(&self) -> Stratum {
        stratum_of(self.market_class, self.cap_tier)
    }
}

/// A reference TR that failed on paper, recorded with its failure code rather
/// than silently dropping the attribute (U2 execution note).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrFailure {
    /// The TR code (e.g. `t1904`).
    pub tr: String,
    /// The gateway failure code (e.g. `IGW40011`), or a coarse class.
    pub code: String,
}

/// Capture provenance (R2/R4, Key Decisions): enough to reproduce and audit
/// the artifact, including the pre-registered tier-boundary rule and the
/// applied instrument-type filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataProvenance {
    /// RFC-3339 capture timestamp.
    pub captured_at: String,
    /// The KST session the tags are as-of (`YYYYMMDD`) — tags are point-in-time.
    pub session_date: String,
    /// The TRs the capture joined.
    pub source_trs: Vec<String>,
    /// The applied instrument-type filter (R2): equities-only, non-empty
    /// `etfgubun` rows dropped, and preferred shares excluded by issue-sequence
    /// digit (P5). SPACs and REITs remain undetectable from `t8430` alone — that
    /// residual is an accepted, documented limitation.
    pub instrument_type_filter: String,
    /// The pre-registered cap-tier boundary rule, in words.
    pub tier_boundary_rule: String,
    /// The concrete per-market cutoffs the rule produced (Key Decisions).
    pub cap_cutoffs: Vec<CapCutoff>,
    /// Reference TRs that failed on paper, with their failure codes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paper_incompatible: Vec<TrFailure>,
    /// The codes the issue-sequence rule dropped (P5), sorted — the evidence
    /// that the declared filter was actually applied, mirroring the pit walk's
    /// `FrozenSet::dropped_preferred`. Absent on artifacts captured before P5,
    /// which is why it defaults rather than being required.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped_preferred: Vec<String>,
}

/// The one metadata artifact both consumers read (KTD2): provenance + the
/// per-symbol records. Its [`UniverseMetadata::content_hash`] is stamped into
/// the ingest pin and the backtest run manifest; the U6 report fails when the
/// two differ (a re-capture between ingest and backtest would silently re-tier
/// symbols and corrupt the per-tier counts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseMetadata {
    /// Capture provenance.
    pub provenance: MetadataProvenance,
    /// One record per skeleton symbol.
    pub records: Vec<InstrumentMetadata>,
}

impl UniverseMetadata {
    /// SHA-256 over the artifact's canonical JSON serialization, hex-encoded —
    /// the identity both consumers pin (KTD2).
    pub fn content_hash(&self) -> String {
        let json = serde_json::to_string(self).expect("artifact serializes");
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        hex(&hasher.finalize())
    }

    /// Validate the artifact before writing (fail-closed, mirroring
    /// `UniverseFile::validate`): 6-digit unique shcodes, `tradable` consistent
    /// with the designation gate, and complete provenance. Returns every
    /// violation in one pass.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        if self.records.is_empty() {
            errs.push("no records — refusing an empty universe".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        for (i, r) in self.records.iter().enumerate() {
            if r.shcode.len() != 6 || !r.shcode.bytes().all(|b| b.is_ascii_digit()) {
                errs.push(format!("records[{i}].shcode {:?} is not a 6-digit numeric code", r.shcode));
            }
            if !seen.insert(&r.shcode) {
                errs.push(format!("duplicate shcode {:?}", r.shcode));
            }
            if r.tradable != is_tradable(&r.designation) {
                errs.push(format!(
                    "records[{i}] ({}) tradable={} contradicts its designation {:?}",
                    r.shcode, r.tradable, r.designation
                ));
            }
        }
        let p = &self.provenance;
        if p.captured_at.trim().is_empty() {
            errs.push("provenance.captured_at is empty".to_string());
        }
        if p.session_date.trim().is_empty() {
            errs.push("provenance.session_date is empty".to_string());
        }
        if p.instrument_type_filter.trim().is_empty() {
            errs.push("provenance.instrument_type_filter is empty (the applied filter must be recorded, R2)".to_string());
        }
        if p.tier_boundary_rule.trim().is_empty() {
            errs.push("provenance.tier_boundary_rule is empty (boundaries are pre-registered)".to_string());
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Load and parse an artifact file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
    }
}

/// The pin-file name written into the catalog directory by a stratified ingest
/// (KTD2): the ingest-side half of the artifact-hash handshake the U6 report
/// asserts against the run manifest.
pub const METADATA_PIN_FILE: &str = "universe-metadata-pin.json";

/// The ingest provenance pin (KTD2): which artifact the stratified selection
/// was drawn from, its content hash, and the per-stratum composition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataPin {
    /// The artifact path the ingest read (informational — the hash is the identity).
    pub artifact_path: String,
    /// The artifact's [`UniverseMetadata::content_hash`] at selection time.
    pub content_hash: String,
    /// Per-stratum selected-symbol counts (keys are [`Stratum::label`]s).
    pub per_stratum: BTreeMap<String, usize>,
    /// The selected shcodes, in stratum order.
    pub symbols: Vec<String>,
    /// RFC-3339 pin timestamp.
    pub pinned_at: String,
}

impl MetadataPin {
    /// Write the pin beside the catalog's checkpoint (`<catalog>/universe-metadata-pin.json`).
    pub fn write(&self, catalog_path: &Path) -> Result<(), String> {
        let path = catalog_path.join(METADATA_PIN_FILE);
        std::fs::create_dir_all(catalog_path)
            .map_err(|e| format!("creating {}: {e}", catalog_path.display()))?;
        let json = serde_json::to_string_pretty(self).expect("pin serializes");
        std::fs::write(&path, format!("{json}\n")).map_err(|e| format!("writing {}: {e}", path.display()))
    }

    /// Load the pin from a catalog directory, when one exists.
    pub fn load(catalog_path: &Path) -> Result<Option<Self>, String> {
        let path = catalog_path.join(METADATA_PIN_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("parsing {}: {e}", path.display()))
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(shcode: &str, market: MarketClass, cap: Resolved<f64>) -> InstrumentMetadata {
        InstrumentMetadata {
            shcode: shcode.to_string(),
            market_class: market,
            market_cap: cap,
            cap_tier: CapTier::BelowBoard, // assigned by assign_cap_tiers
            turnover: Resolved::Unavailable,
            liquidity_tier: LiquidityTier::Unknown,
            index_membership: Resolved::Proxy(IndexMembership::NotMember),
            has_derivative: Resolved::Value(false),
            designation: None,
            tradable: true,
        }
    }

    fn provenance(cutoffs: Vec<CapCutoff>) -> MetadataProvenance {
        MetadataProvenance {
            captured_at: "2026-07-10T00:00:00Z".to_string(),
            session_date: "20260710".to_string(),
            source_trs: vec!["t8430".into(), "t2522".into(), "t1904".into(), "t1444".into(), "t1405".into(), "t1404".into()],
            instrument_type_filter: "t8430 etfgubun empty (equities-only; ETF/ETN dropped)".to_string(),
            tier_boundary_rule: format!(
                "per-market cap-rank quantile {DEFAULT_CAP_TOP_QUANTILE}: top ceil(q*on_board) = Top, rest = Mid; unresolved cap = BelowBoard"
            ),
            cap_cutoffs: cutoffs,
            paper_incompatible: Vec::new(),
            dropped_preferred: Vec::new(),
        }
    }

    // --- cap tiers (U1: boundary values) ---

    #[test]
    fn cap_tiers_split_each_market_at_the_quantile() {
        // 4 on-board KOSPI (caps 400..100) + 2 on-board KOSDAQ + 1 below-board.
        let mut records = vec![
            record("000001", MarketClass::Kospi, Resolved::Value(400.0)),
            record("000002", MarketClass::Kospi, Resolved::Value(300.0)),
            record("000003", MarketClass::Kospi, Resolved::Value(200.0)),
            record("000004", MarketClass::Kospi, Resolved::Value(100.0)),
            record("100001", MarketClass::Kosdaq, Resolved::Value(50.0)),
            record("100002", MarketClass::Kosdaq, Resolved::Value(40.0)),
            record("200001", MarketClass::Kosdaq, Resolved::Unavailable),
        ];
        let cutoffs = assign_cap_tiers(&mut records, 0.5);
        // KOSPI: top 2 of 4 → Top; the boundary value (rank 2, cap 300) is Top,
        // rank 3 (cap 200) is exactly the first Mid — the boundary case.
        assert_eq!(records[0].cap_tier, CapTier::Top);
        assert_eq!(records[1].cap_tier, CapTier::Top);
        assert_eq!(records[2].cap_tier, CapTier::Mid);
        assert_eq!(records[3].cap_tier, CapTier::Mid);
        // KOSDAQ: ceil(0.5 * 2) = 1 → exactly one Top.
        assert_eq!(records[4].cap_tier, CapTier::Top);
        assert_eq!(records[5].cap_tier, CapTier::Mid);
        // Unresolved cap → BelowBoard regardless of market.
        assert_eq!(records[6].cap_tier, CapTier::BelowBoard);
        // Cutoffs are recorded per market with the boundary cap value.
        let kospi = cutoffs.iter().find(|c| c.market_class == MarketClass::Kospi).unwrap();
        assert_eq!((kospi.on_board, kospi.top_count), (4, 2));
        assert_eq!(kospi.boundary_cap, Some(300.0));
        let kosdaq = cutoffs.iter().find(|c| c.market_class == MarketClass::Kosdaq).unwrap();
        assert_eq!((kosdaq.on_board, kosdaq.top_count), (2, 1));
    }

    #[test]
    fn cap_tie_breaks_by_shcode_deterministically() {
        let mut records = vec![
            record("000002", MarketClass::Kospi, Resolved::Value(100.0)),
            record("000001", MarketClass::Kospi, Resolved::Value(100.0)),
        ];
        assign_cap_tiers(&mut records, 0.5);
        // Equal caps: the lower shcode ranks first → Top.
        assert_eq!(records[1].cap_tier, CapTier::Top, "000001 wins the tie");
        assert_eq!(records[0].cap_tier, CapTier::Mid);
    }

    // --- liquidity tiers (U1: boundary values) ---

    #[test]
    fn liquidity_tier_boundaries() {
        let tier = |v: f64| assign_liquidity_tier(&Resolved::Value(v));
        assert_eq!(tier(LIQUIDITY_HIGH_FLOOR_KRW), LiquidityTier::High, "at the boundary → High");
        assert_eq!(tier(LIQUIDITY_HIGH_FLOOR_KRW - 1.0), LiquidityTier::Mid);
        assert_eq!(tier(LIQUIDITY_MID_FLOOR_KRW), LiquidityTier::Mid, "at the boundary → Mid");
        assert_eq!(tier(LIQUIDITY_MID_FLOOR_KRW - 1.0), LiquidityTier::Low);
        assert_eq!(
            assign_liquidity_tier(&Resolved::Unavailable),
            LiquidityTier::Unknown,
            "unresolved turnover is Unknown, never a defaulted Low (R4)"
        );
        assert_eq!(assign_liquidity_tier(&Resolved::Proxy(LIQUIDITY_HIGH_FLOOR_KRW)), LiquidityTier::High);
    }

    #[test]
    fn liquidity_floor_gates_only_on_resolved_turnover() {
        // R5: Value/Proxy below the floor → excluded; Unavailable → admitted.
        assert!(!passes_liquidity_floor(&Resolved::Value(1.0), 10.0));
        assert!(passes_liquidity_floor(&Resolved::Value(10.0), 10.0), "at the floor passes");
        assert!(!passes_liquidity_floor(&Resolved::Proxy(9.9), 10.0));
        assert!(
            passes_liquidity_floor(&Resolved::Unavailable, 10.0),
            "Unavailable is admitted with its resolution recorded — fail-closed would gut the small-cap stratum"
        );
    }

    // --- tradability gate (Covers AE3) ---

    #[test]
    fn every_designation_category_gates_and_a_clean_symbol_passes() {
        for kind in [
            DesignationKind::Halt,
            DesignationKind::Managed,
            DesignationKind::Caution,
            DesignationKind::Warning,
            DesignationKind::Risk,
            DesignationKind::Overheated,
        ] {
            let d = Some(Designation { kind, source_tr: "t1405".to_string() });
            assert!(!is_tradable(&d), "{kind:?} must exclude");
        }
        assert!(is_tradable(&None), "a clean symbol is tradable");
    }

    // --- resolution transparency (Covers AE4) ---

    #[test]
    fn missing_source_resolves_unavailable_or_proxy_never_a_default() {
        let r = record("000001", MarketClass::Kospi, Resolved::Unavailable);
        assert!(r.market_cap.is_unavailable());
        // Index membership from the ETF proxy is Proxy(NotMember), not a bare false.
        assert_eq!(r.index_membership, Resolved::Proxy(IndexMembership::NotMember));
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains(r#""market_cap":{"resolution":"unavailable"}"#), "{json}");
        assert!(
            json.contains(r#""index_membership":{"resolution":"proxy","value":"not_member"}"#),
            "{json}"
        );
    }

    // --- stratification (U1) ---

    #[test]
    fn below_board_lands_in_the_exclusion_stratum_not_dropped() {
        // A below-board KOSDAQ symbol lands in cell 4, NOT KOSDAQ mid/small.
        assert_eq!(
            stratum_of(MarketClass::Kosdaq, CapTier::BelowBoard),
            Stratum::SmallCapExclusion
        );
        assert_eq!(stratum_of(MarketClass::Kospi, CapTier::BelowBoard), Stratum::SmallCapExclusion);
        assert_eq!(stratum_of(MarketClass::Kospi, CapTier::Top), Stratum::KospiBlueChip);
        assert_eq!(stratum_of(MarketClass::Kospi, CapTier::Mid), Stratum::KospiMid);
        assert_eq!(stratum_of(MarketClass::Kosdaq, CapTier::Top), Stratum::KosdaqOnBoard);
        assert_eq!(stratum_of(MarketClass::Kosdaq, CapTier::Mid), Stratum::KosdaqOnBoard);
        // And stratify keeps it: one below-board symbol is selected, not dropped.
        let mut records = vec![record("200001", MarketClass::Kosdaq, Resolved::Unavailable)];
        assign_cap_tiers(&mut records, 0.5);
        let sample = stratify(&records, 2);
        assert_eq!(sample[&Stratum::SmallCapExclusion], vec!["200001".to_string()]);
    }

    #[test]
    fn stratify_returns_equal_counts_and_degrades_on_a_thin_stratum() {
        // 6 per on-board cell, but only 1 KOSDAQ on-board and 0 below-board.
        let mut records = vec![
            record("000001", MarketClass::Kospi, Resolved::Value(600.0)),
            record("000002", MarketClass::Kospi, Resolved::Value(500.0)),
            record("000003", MarketClass::Kospi, Resolved::Value(400.0)),
            record("000004", MarketClass::Kospi, Resolved::Value(300.0)),
            record("100001", MarketClass::Kosdaq, Resolved::Value(50.0)),
        ];
        assign_cap_tiers(&mut records, 0.5);
        let sample = stratify(&records, 2);
        assert_eq!(sample[&Stratum::KospiBlueChip].len(), 2);
        assert_eq!(sample[&Stratum::KospiMid].len(), 2);
        // KOSDAQ has one on-board name (Top by ceil) — it contributes all it has.
        assert_eq!(sample[&Stratum::KosdaqOnBoard].len(), 1);
        assert!(sample[&Stratum::SmallCapExclusion].is_empty(), "empty stratum yields empty, no error");
        // Total respects the per-stratum bound.
        let total: usize = sample.values().map(Vec::len).sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn stratify_excludes_non_tradable_and_strides_deterministically() {
        let mut records: Vec<InstrumentMetadata> = (0..10)
            .map(|i| record(&format!("{:06}", 100 + i), MarketClass::Kospi, Resolved::Value((1000 - i) as f64)))
            .collect();
        // Mark the top-cap name halted: it must not be sampled (AE3 at selection).
        records[0].designation =
            Some(Designation { kind: DesignationKind::Halt, source_tr: "t1405".to_string() });
        records[0].tradable = false;
        assign_cap_tiers(&mut records, 0.5);
        let a = stratify(&records, 2);
        let b = stratify(&records, 2);
        assert_eq!(a, b, "sampling is deterministic");
        for symbols in a.values() {
            assert!(!symbols.contains(&"000100".to_string()), "halted symbol never sampled");
        }
        // Stride, not head-take: 5 tradable Top names sampled at 2 → indices 0 and 2.
        let blue = &a[&Stratum::KospiBlueChip];
        assert_eq!(blue.len(), 2);
        assert_ne!(blue[1], "000102", "second pick is strided past the head");
    }

    // --- artifact (U1: serde round-trip, byte-stable) ---

    #[test]
    fn artifact_round_trips_byte_stable_and_hash_pins_content() {
        let mut records = vec![
            record("000001", MarketClass::Kospi, Resolved::Value(400.0)),
            record("100001", MarketClass::Kosdaq, Resolved::Unavailable),
        ];
        let cutoffs = assign_cap_tiers(&mut records, DEFAULT_CAP_TOP_QUANTILE);
        let artifact = UniverseMetadata { provenance: provenance(cutoffs), records };
        assert!(artifact.validate().is_ok(), "{:?}", artifact.validate());

        let json = serde_json::to_string_pretty(&artifact).unwrap();
        let back: UniverseMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(artifact, back);
        assert_eq!(
            serde_json::to_string_pretty(&back).unwrap(),
            json,
            "byte-stable round trip"
        );
        assert_eq!(artifact.content_hash(), back.content_hash());

        // Any content change moves the hash (the KTD2 re-capture tripwire).
        let mut re_captured = artifact.clone();
        re_captured.records[0].cap_tier = CapTier::Mid;
        assert_ne!(artifact.content_hash(), re_captured.content_hash());
    }

    #[test]
    fn validation_rejects_bad_shcodes_gate_contradictions_and_empty_provenance() {
        let mut records = vec![
            record("00001", MarketClass::Kospi, Resolved::Value(1.0)), // 5 digits
            record("000002", MarketClass::Kospi, Resolved::Value(1.0)),
            record("000002", MarketClass::Kospi, Resolved::Value(1.0)), // duplicate
        ];
        // A designated symbol claiming tradable contradicts the gate (R3).
        records[1].designation =
            Some(Designation { kind: DesignationKind::Managed, source_tr: "t1404".to_string() });
        records[1].tradable = true;
        let mut p = provenance(Vec::new());
        p.instrument_type_filter = String::new();
        let artifact = UniverseMetadata { provenance: p, records };
        let errs = artifact.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("not a 6-digit")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("duplicate shcode")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("contradicts its designation")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("instrument_type_filter")), "{errs:?}");
    }

    #[test]
    fn pin_round_trips_through_a_catalog_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = tmp.path().join("catalog");
        assert_eq!(MetadataPin::load(&catalog), Ok(None), "no pin yet");
        let pin = MetadataPin {
            artifact_path: "lab/config/universe-metadata.json".to_string(),
            content_hash: "abc123".to_string(),
            per_stratum: BTreeMap::from([
                ("kospi_blue_chip".to_string(), 2usize),
                ("small_cap_exclusion".to_string(), 1usize),
            ]),
            symbols: vec!["000001".to_string(), "000002".to_string(), "200001".to_string()],
            pinned_at: "2026-07-10T00:00:00Z".to_string(),
        };
        pin.write(&catalog).unwrap();
        assert_eq!(MetadataPin::load(&catalog), Ok(Some(pin)));
    }
}
