//! Rust-native LS fetch adapter and the D3 split completeness gate (U2).
//!
//! The migration source (`~/dev/korea-broker-sdk-ls/scripts/fetch_ls_specs.py`)
//! is the endpoint/retry/fallback reference; its fixed `MIN_TR_COUNT` floor is
//! replaced here by the *relative* mass-shrink proportion in [`completeness_gate`]
//! (KTD-3). All decision logic ([`parse_menu`], [`completeness_gate`],
//! [`RawInventory::code_set`]) is a pure function over parsed structures so it is
//! unit-tested transport-independently; the [`FetchClient`] HTTP layer is
//! base-URL-injected and covered by a synchronous `httpmock` test. No path here
//! touches the live network under `cargo test`.

use std::collections::BTreeMap;
use std::fmt;
use std::thread::sleep;
use std::time::Duration;

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::CodeSet;

/// Default mass-shrink proportion (KTD-3): a fetched full inventory smaller than
/// `(1 - 0.10) * committed_code_set_len` is suspected truncation (exit `2`).
/// Operator-overridable; the value is tunable once real inventory sizes are
/// observed (Open Questions §D3 proportion value).
pub const DEFAULT_TRUNCATION_PROPORTION: f64 = 0.10;

/// Hardcoded property-type fallback (mirrors the migration source) used when the
/// property-type mapping API is unavailable — a recoverable, warning-only path.
const PROPERTY_TYPE_FALLBACK: &[(&str, &str)] = &[
    ("A0001", "String"),
    ("A0003", "Long"),
    ("A0004", "Decimal"),
    ("A0005", "Binary"),
];

// ---------------------------------------------------------------------------
// Menu parsing (pure)
// ---------------------------------------------------------------------------

/// One API group discovered from the `/apiservice` menu scrape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuGroup {
    pub category_name: String,
    pub group_name: String,
    pub api_id: String,
    pub is_websocket_group: bool,
}

/// The menu/group structure failed to parse — the structural-integrity guard
/// that fires exit `2` (R12, KTD-3), distinct from an individual TR's absence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuParseError(pub String);

impl fmt::Display for MenuParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "menu/group structure failed to parse: {}", self.0)
    }
}

impl std::error::Error for MenuParseError {}

/// Parse the `/apiservice` menu HTML into its API groups, mirroring the
/// migration source's `nav#lnb` / `ul.second-depth` / `ul.third-depth` walk.
///
/// Returns [`MenuParseError`] when `nav#lnb` is absent or no API group resolves —
/// the well-formedness guard the split gate keys exit `2` on. A page that parses
/// but is *missing individual TRs* is **not** a parse error; that is a removal
/// finding handled downstream (R12).
pub fn parse_menu(html: &str) -> Result<Vec<MenuGroup>, MenuParseError> {
    // Selectors are static and known-valid; `expect` documents that invariant.
    let lnb_sel = Selector::parse("nav#lnb").expect("static selector");
    let cat_li_sel = Selector::parse("li[id]").expect("static selector");
    let cat_name_sel = Selector::parse("ul.second-depth > li > a").expect("static selector");
    let group_li_sel = Selector::parse("ul.third-depth > li[id]").expect("static selector");
    let a_sel = Selector::parse("a").expect("static selector");

    let doc = Html::parse_document(html);
    let lnb = doc
        .select(&lnb_sel)
        .next()
        .ok_or_else(|| MenuParseError("nav#lnb not found".to_string()))?;

    let mut groups = Vec::new();
    // `li[id]` matches both category and group items; a group `li` has no
    // `ul.second-depth` child, so its category-name lookup yields `None` and it
    // is skipped — the same natural filter the migration source relies on.
    for cat_li in lnb.select(&cat_li_sel) {
        let Some(name_a) = cat_li.select(&cat_name_sel).next() else {
            continue;
        };
        let category_name = clean_category(&collect_text(&name_a));

        for g_li in cat_li.select(&group_li_sel) {
            let Some(api_id) = g_li.value().attr("id") else {
                continue;
            };
            let Some(a) = g_li.select(&a_sel).next() else {
                continue;
            };
            let group_name = collect_text(&a);
            if group_name.is_empty() {
                continue;
            }
            let is_websocket_group = group_name.contains("실시간");
            groups.push(MenuGroup {
                category_name: category_name.clone(),
                group_name,
                api_id: api_id.to_string(),
                is_websocket_group,
            });
        }
    }

    if groups.is_empty() {
        return Err(MenuParseError(
            "nav#lnb present but no API groups resolved".to_string(),
        ));
    }
    Ok(groups)
}

fn collect_text(el: &scraper::ElementRef<'_>) -> String {
    el.text().collect::<String>().trim().to_string()
}

/// `[domestic] foo` → `domestic` (drop a leading bracketed tag), mirroring the
/// migration source's `split("]")[0].replace("[", "")`.
fn clean_category(name: &str) -> String {
    if let Some((head, _)) = name.split_once(']') {
        head.replace('[', "").trim().to_string()
    } else {
        name.to_string()
    }
}

// ---------------------------------------------------------------------------
// Split completeness gate (pure) — D3 / KTD-3
// ---------------------------------------------------------------------------

/// The outcome of the split completeness gate. Only [`GateOutcome::Pass`] lets a
/// staged run through; the other two are exit `2` (fetch incomplete), recorded
/// with enough context for `fetch-report.json`.
#[derive(Debug, Clone, PartialEq)]
pub enum GateOutcome {
    /// Menu well-formed and inventory size within tolerance — proceed. Absent
    /// individual baselined TRs become removal findings downstream (R12).
    Pass,
    /// The menu/group structure itself failed to parse — exit `2`.
    MenuParseFailed,
    /// The full fetched inventory shrank past the relative proportion of the
    /// committed code-set — suspected mass truncation, exit `2`.
    SuspectedTruncation {
        fetched: usize,
        committed: usize,
        proportion: f64,
    },
}

impl GateOutcome {
    /// Whether the staged run may proceed to comparison.
    pub fn passed(&self) -> bool {
        matches!(self, GateOutcome::Pass)
    }
}

/// The D3 split gate (R12, KTD-3). `menu_parsed` is the structural-integrity
/// guard; the truncation guard fires only when a committed code-set exists and
/// the **full** fetched inventory has shrunk past `(1 - proportion) *
/// committed_code_set_len`. The numerator is the full fetched inventory and the
/// denominator the full committed code-set — never the ~7-TR baselined subset.
/// On bootstrap (`committed_code_set_len` is `None`) only the menu guard applies.
pub fn completeness_gate(
    menu_parsed: bool,
    fetched_count: usize,
    committed_code_set_len: Option<usize>,
    proportion: f64,
) -> GateOutcome {
    if !menu_parsed {
        return GateOutcome::MenuParseFailed;
    }
    if let Some(committed) = committed_code_set_len {
        let threshold = (1.0 - proportion) * committed as f64;
        if (fetched_count as f64) < threshold {
            return GateOutcome::SuspectedTruncation {
                fetched: fetched_count,
                committed,
                proportion,
            };
        }
    }
    GateOutcome::Pass
}

// ---------------------------------------------------------------------------
// Raw inventory model (faithful capture; normalization is U3)
// ---------------------------------------------------------------------------

/// One TR's raw evidence, captured verbatim (R8). Long descriptions are kept
/// as-is; hashing into the Structural API Shape happens in U3 normalization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawTr {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub is_websocket: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_sec: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corp_rate_limit_per_sec: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// LS property rows, preserved verbatim (`propertyCd`, `propertyNm`,
    /// `bodyType`, …) so U3 normalizes without re-fetching.
    #[serde(default)]
    pub properties: Vec<Value>,
    #[serde(default)]
    pub req_example: Value,
    #[serde(default)]
    pub res_example: Value,
}

/// One API group's raw evidence: its facts plus its TRs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawGroup {
    pub category_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    pub group_name: String,
    pub is_websocket_group: bool,
    #[serde(default)]
    pub trs: Vec<RawTr>,
}

/// The full fetched inventory — raw evidence for the reviewed snapshot and the
/// source the code-set is derived from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawInventory {
    #[serde(default)]
    pub source_urls: Vec<String>,
    /// Property-type code → display-name mapping captured at fetch time, so U3
    /// normalization resolves `propertyType` codes without re-fetching.
    #[serde(default)]
    pub property_types: BTreeMap<String, String>,
    #[serde(default)]
    pub groups: Vec<RawGroup>,
}

impl RawInventory {
    /// Derive the full-inventory code-set (R3b): every distinct, non-empty TR
    /// code across all groups, sorted and de-duplicated.
    pub fn code_set(&self, provisional: bool) -> CodeSet {
        CodeSet::new(
            self.groups
                .iter()
                .flat_map(|g| g.trs.iter())
                .map(|tr| tr.code.trim().to_string())
                .filter(|c| !c.is_empty()),
            provisional,
        )
    }

    /// The full fetched inventory size — the truncation gate's numerator (R12).
    pub fn tr_count(&self) -> usize {
        self.code_set(false).len()
    }

    /// TR codes that live under a facts-degraded group — one whose protocol
    /// endpoint failed, so its `group_id` is `None` (U4, KTD-4a). The protocol
    /// UUID is exactly the field that goes missing on an endpoint/rate facts
    /// outage, so the gate joins these *codes* against the maintained set rather
    /// than the (now-`None`) group id. A degraded group still lists its TRs.
    pub fn facts_degraded_tr_codes(&self) -> std::collections::BTreeSet<String> {
        self.groups
            .iter()
            .filter(|g| g.group_id.is_none())
            .flat_map(|g| g.trs.iter())
            .map(|tr| tr.code.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// HTTP layer (blocking reqwest, base-URL-injected)
// ---------------------------------------------------------------------------

/// A fetch failure that maps to exit `2`. Carries a human-readable context for
/// `fetch-report.json`; the menu-parse case is represented separately by
/// [`GateOutcome::MenuParseFailed`].
#[derive(Debug)]
pub struct FetchError {
    pub context: String,
}

impl FetchError {
    fn new(context: impl Into<String>) -> Self {
        FetchError {
            context: context.into(),
        }
    }
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fetch error: {}", self.context)
    }
}

impl std::error::Error for FetchError {}

/// Bounded exponential-backoff retry policy (migration-source parity). Tests set
/// `base_delay` to zero to exercise the retry-exhaustion path without sleeping.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(8),
        }
    }
}

/// A base-URL-injected blocking LS fetch client. Injection is what makes the
/// HTTP layer testable against a local `httpmock` server.
pub struct FetchClient {
    base_url: String,
    http: reqwest::blocking::Client,
    retry: RetryConfig,
}

impl FetchClient {
    /// Build a client against `base_url` with the default retry policy and a
    /// 10-second per-request timeout (migration-source parity).
    pub fn new(base_url: impl Into<String>) -> Result<Self, FetchError> {
        Self::with_retry(base_url, RetryConfig::default())
    }

    /// Build a client with an explicit retry policy (tests pass a zero delay).
    pub fn with_retry(base_url: impl Into<String>, retry: RetryConfig) -> Result<Self, FetchError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| FetchError::new(format!("building HTTP client: {e}")))?;
        Ok(FetchClient {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
            retry,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// GET `path` with bounded exponential backoff, returning the body text.
    /// Used for the TR list / property endpoints, whose failure is a fetch error.
    fn get_text_retry(&self, path: &str) -> Result<String, FetchError> {
        let url = self.url(path);
        let mut attempt = 0;
        loop {
            match self
                .http
                .get(&url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text())
            {
                Ok(body) => return Ok(body),
                Err(e) => {
                    // A 4xx is the server rejecting the request, not a transient
                    // fault — retrying only wastes the backoff budget before the
                    // inevitable fetch error.
                    let client_error = e.status().is_some_and(|s| s.is_client_error());
                    if client_error || attempt >= self.retry.max_retries {
                        return Err(FetchError::new(format!(
                            "GET {url} failed after {} attempt(s): {e}",
                            attempt + 1
                        )));
                    }
                    // Cap the shift so the backoff stays well-defined even if
                    // `RetryConfig` is given a large `max_retries`; the result is
                    // clamped to `max_delay` anyway.
                    let backoff = self
                        .retry
                        .base_delay
                        .saturating_mul(1u32 << attempt.min(31))
                        .min(self.retry.max_delay);
                    if !backoff.is_zero() {
                        sleep(backoff);
                    }
                    attempt += 1;
                }
            }
        }
    }

    /// GET `path` once (no retry), returning the body text or an error.
    fn get_text_once(&self, path: &str) -> Result<String, FetchError> {
        let url = self.url(path);
        self.http
            .get(&url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text())
            .map_err(|e| FetchError::new(format!("GET {url}: {e}")))
    }

    /// Fetch the `/apiservice` menu HTML. A transport failure here is a fetch
    /// error; a *parse* failure of the returned HTML is [`MenuParseError`].
    pub fn menu_html(&self) -> Result<String, FetchError> {
        self.get_text_retry("/apiservice")
    }

    /// Property-type code → name mapping. On any failure or empty response,
    /// falls back to the hardcoded map with a stderr warning and continues — a
    /// recoverable path (migration-source parity).
    ///
    /// The second tuple element is `true` when the fallback table was served
    /// (U4): unlike endpoint/rate facts, the mapping is a single whole-inventory
    /// call, so its fallback substitutes raw type codes for *every* TR with no
    /// per-group granularity — the facts-outage gate (U5) keys on this signal.
    pub fn property_type_mapping(&self) -> (BTreeMap<String, String>, bool) {
        let fallback = || -> (BTreeMap<String, String>, bool) {
            (
                PROPERTY_TYPE_FALLBACK
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                true,
            )
        };
        let body =
            match self.get_text_once("/api/codes/public/system-codes?groupCode=property_type") {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("warning: property-type mapping API failed ({e}); using fallback");
                    return fallback();
                }
            };
        let parsed: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("warning: property-type mapping unparseable ({e}); using fallback");
                return fallback();
            }
        };
        // Accept `{ "list": [...] }` or a bare array of `{ code, name }`.
        let items = parsed.get("list").unwrap_or(&parsed);
        let mut map = BTreeMap::new();
        if let Some(arr) = items.as_array() {
            for item in arr {
                if let (Some(code), Some(name)) = (
                    item.get("code").and_then(Value::as_str),
                    item.get("name").and_then(Value::as_str),
                ) {
                    map.insert(code.to_string(), name.to_string());
                }
            }
        }
        if map.is_empty() {
            eprintln!("warning: property-type mapping empty; using fallback");
            return fallback();
        }
        (map, false)
    }

    /// TR list for a group (`/api/apis/guide/tr/{api_id}`). Failure after retries
    /// is a fetch error.
    pub fn tr_list(&self, api_id: &str) -> Result<Vec<Value>, FetchError> {
        let body = self.get_text_retry(&format!("/api/apis/guide/tr/{api_id}"))?;
        let parsed: Value = serde_json::from_str(&body)
            .map_err(|e| FetchError::new(format!("TR list {api_id} unparseable: {e}")))?;
        Ok(parsed.as_array().cloned().unwrap_or_default())
    }

    /// TR property rows (`/api/apis/guide/tr/property/{tr_id}`). A JSON `null`
    /// response is treated as an empty list (migration-source parity).
    pub fn tr_properties(&self, tr_id: &str) -> Result<Vec<Value>, FetchError> {
        let body = self.get_text_retry(&format!("/api/apis/guide/tr/property/{tr_id}"))?;
        let parsed: Value = serde_json::from_str(&body)
            .map_err(|e| FetchError::new(format!("TR property {tr_id} unparseable: {e}")))?;
        Ok(parsed.as_array().cloned().unwrap_or_default())
    }

    /// Group protocol facts (`/api/apis/public/{api_id}`). Best-effort: returns
    /// an empty object on failure (migration-source parity).
    pub fn group_protocol(&self, api_id: &str) -> Value {
        match self.get_text_once(&format!("/api/apis/public/{api_id}")) {
            Ok(body) => serde_json::from_str(&body).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        }
    }

    /// Orchestrate a full inventory scrape: menu → per-group TR lists → per-TR
    /// properties + protocol facts. The live path; exercised end-to-end against
    /// `httpmock` in tests and against LS only under the operator-run seed (U6).
    ///
    /// Menu-parse failure surfaces as [`MenuParseError`]; transport failure on a
    /// required endpoint surfaces as [`FetchError`]. Both are exit `2`. The
    /// truncation guard is applied by the caller via [`completeness_gate`].
    pub fn fetch_full_inventory(&self) -> Result<FetchOutcome, FetchInventoryError> {
        let html = self.menu_html().map_err(FetchInventoryError::Fetch)?;
        let menu = parse_menu(&html).map_err(FetchInventoryError::Menu)?;
        let (prop_types, property_type_fallback_served) = self.property_type_mapping();

        let mut source_urls = vec![self.url("/apiservice")];
        let mut groups = Vec::new();
        for g in &menu {
            let tr_list = self
                .tr_list(&g.api_id)
                .map_err(FetchInventoryError::Fetch)?;
            source_urls.push(self.url(&format!("/api/apis/guide/tr/{}", g.api_id)));
            let protocol = self.group_protocol(&g.api_id);
            let rate_limits = parse_rate_limits(&protocol);

            let mut trs = Vec::new();
            for tr in &tr_list {
                let code = tr
                    .get("trCode")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if code.is_empty() {
                    continue;
                }
                let tr_id = tr.get("id").and_then(value_as_id);
                let properties = match &tr_id {
                    Some(id) => self.tr_properties(id).map_err(FetchInventoryError::Fetch)?,
                    None => Vec::new(),
                };
                let (rate, corp_rate) = rate_limits.get(&code).copied().unwrap_or((None, None));
                trs.push(RawTr {
                    code,
                    name: tr.get("trName").and_then(Value::as_str).map(str::to_string),
                    is_websocket: g.is_websocket_group,
                    http_method: protocol
                        .get("httpMethod")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    url: protocol
                        .get("accessUrl")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    protocol_type: protocol
                        .get("protocolType")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    rate_limit_per_sec: rate,
                    corp_rate_limit_per_sec: corp_rate,
                    description: tr
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    properties,
                    req_example: tr.get("reqExample").cloned().unwrap_or(Value::Null),
                    res_example: tr.get("resExample").cloned().unwrap_or(Value::Null),
                });
            }

            groups.push(RawGroup {
                category_name: g.category_name.clone(),
                group_id: protocol
                    .get("id")
                    .and_then(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .or_else(|| Some(v.to_string()))
                    })
                    .filter(|s| s != "null"),
                group_name: g.group_name.clone(),
                is_websocket_group: g.is_websocket_group,
                trs,
            });
        }

        Ok(FetchOutcome {
            inventory: RawInventory {
                source_urls,
                property_types: prop_types,
                groups,
            },
            property_type_fallback_served,
        })
    }
}

/// The result of a full inventory scrape: the raw inventory plus the fetch-time
/// signal that the whole-inventory property-type mapping fell back to the
/// hardcoded table (U4). The flag lives here rather than inside [`RawInventory`]
/// so the persisted raw evidence stays pure inventory; the gate reads it from the
/// `fetch-report.json` the caller writes.
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub inventory: RawInventory,
    pub property_type_fallback_served: bool,
}

/// `id` may arrive as a string or a number; accept either as a path segment.
fn value_as_id(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A whole-inventory fetch failure, distinguishing a structural menu-parse
/// failure (its own `fetch-report` reason) from a transport-level fetch error.
/// Both map to exit `2`.
#[derive(Debug)]
pub enum FetchInventoryError {
    Menu(MenuParseError),
    Fetch(FetchError),
}

impl fmt::Display for FetchInventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchInventoryError::Menu(e) => write!(f, "{e}"),
            FetchInventoryError::Fetch(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FetchInventoryError {}

/// Parse `extraParam`'s throughput rules into a per-TR-code rate-limit lookup
/// `(per_sec, corp_per_sec)` (migration-source parity). `extraParam` may be a
/// JSON string or an inline object; anything unparseable yields no limits.
fn parse_rate_limits(protocol: &Value) -> BTreeMap<String, (Option<u32>, Option<u32>)> {
    let raw = match protocol.get("extraParam") {
        Some(v) => v,
        None => return BTreeMap::new(),
    };
    let extra: Value = match raw {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        other => other.clone(),
    };

    let mut result: BTreeMap<String, (Option<u32>, Option<u32>)> = BTreeMap::new();
    let to_u32 = |v: &Value| -> Option<u32> {
        v.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
    };
    if let Some(rules) = extra.get("ThroughputQuotaRule").and_then(Value::as_array) {
        for rule in rules {
            if let Some(code) = rule.get("tr_cd").and_then(Value::as_str) {
                let code = code.trim();
                if !code.is_empty() {
                    result.entry(code.to_string()).or_default().0 =
                        rule.get("requestLimit").and_then(to_u32);
                }
            }
        }
    }
    if let Some(rules) = extra
        .get("CorpThroughputQuotaRule")
        .and_then(Value::as_array)
    {
        for rule in rules {
            if let Some(code) = rule.get("tr_cd").and_then(Value::as_str) {
                let code = code.trim();
                if !code.is_empty() {
                    result.entry(code.to_string()).or_default().1 =
                        rule.get("requestLimit").and_then(to_u32);
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Split completeness gate (the unit's core risk — test-first) -------

    /// Menu well-formed, inventory above the relative floor → pass; absence of
    /// individual TRs is NOT a parse error.
    #[test]
    fn gate_passes_when_menu_ok_and_inventory_within_tolerance() {
        let g = completeness_gate(true, 365, Some(365), DEFAULT_TRUNCATION_PROPORTION);
        assert_eq!(g, GateOutcome::Pass);
        assert!(g.passed());
    }

    /// AE6: a menu/group parse failure is exit `2`, regardless of counts.
    #[test]
    fn gate_fails_on_menu_parse_failure() {
        let g = completeness_gate(false, 365, Some(365), DEFAULT_TRUNCATION_PROPORTION);
        assert_eq!(g, GateOutcome::MenuParseFailed);
        assert!(!g.passed());
    }

    /// A single baselined TR absent from a well-formed menu does NOT trip the
    /// gate — it passes through to become a removal finding downstream (R12).
    #[test]
    fn gate_passes_when_one_tr_absent_but_menu_well_formed() {
        // 364 of a committed 365 present, menu parsed → pass (threshold is 328.5).
        let g = completeness_gate(true, 364, Some(365), DEFAULT_TRUNCATION_PROPORTION);
        assert_eq!(g, GateOutcome::Pass);
    }

    /// The truncation boundary: just at/above `(1 - p) * committed` passes; one
    /// below trips suspected truncation (exit `2`).
    #[test]
    fn gate_boundary_just_above_vs_just_below_threshold() {
        // committed 100, p = 0.10 → threshold 90.0.
        assert_eq!(
            completeness_gate(true, 90, Some(100), 0.10),
            GateOutcome::Pass,
            "exactly at the threshold passes (90 < 90.0 is false)"
        );
        assert_eq!(
            completeness_gate(true, 89, Some(100), 0.10),
            GateOutcome::SuspectedTruncation {
                fetched: 89,
                committed: 100,
                proportion: 0.10,
            },
            "one below the threshold is suspected truncation"
        );
    }

    /// Bootstrap: with no committed code-set only the menu-parse guard applies —
    /// any inventory size passes when the menu parsed.
    #[test]
    fn gate_bootstrap_applies_only_menu_guard() {
        assert_eq!(completeness_gate(true, 1, None, 0.10), GateOutcome::Pass);
        assert_eq!(
            completeness_gate(false, 999, None, 0.10),
            GateOutcome::MenuParseFailed
        );
    }

    // --- Menu parsing (pure) ----------------------------------------------

    const MENU_HTML: &str = r#"
      <html><body>
      <nav id="lnb">
        <li id="cat1">
          <ul class="second-depth"><li><a>[국내주식] 시세</a></li></ul>
          <ul class="third-depth">
            <li id="g100"><a>주식시세</a></li>
            <li id="g200"><a>주식 실시간시세</a></li>
          </ul>
        </li>
      </nav>
      </body></html>
    "#;

    #[test]
    fn parse_menu_extracts_groups_with_category_and_ws_flag() {
        let groups = parse_menu(MENU_HTML).expect("well-formed menu parses");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].api_id, "g100");
        assert_eq!(groups[0].group_name, "주식시세");
        // Migration-source parity: a leading `[tag]` is reduced to the tag and
        // the trailing text is dropped (`split("]")[0].replace("[", "")`).
        assert_eq!(groups[0].category_name, "국내주식");
        assert!(!groups[0].is_websocket_group);
        assert!(
            groups[1].is_websocket_group,
            "실시간 marks a websocket group"
        );
    }

    #[test]
    fn parse_menu_errors_when_lnb_absent() {
        let err = parse_menu("<html><body>no menu</body></html>").unwrap_err();
        assert!(err.to_string().contains("nav#lnb"));
    }

    #[test]
    fn parse_menu_errors_when_no_groups_resolve() {
        let html = r#"<nav id="lnb"><li id="c"><ul class="second-depth"><li><a>cat</a></li></ul></li></nav>"#;
        assert!(
            parse_menu(html).is_err(),
            "a menu with no groups is a parse error"
        );
    }

    // --- Code-set derivation ----------------------------------------------

    fn raw_tr(code: &str) -> RawTr {
        RawTr {
            code: code.to_string(),
            name: None,
            is_websocket: false,
            http_method: None,
            url: None,
            protocol_type: None,
            rate_limit_per_sec: None,
            corp_rate_limit_per_sec: None,
            description: None,
            properties: vec![],
            req_example: Value::Null,
            res_example: Value::Null,
        }
    }

    #[test]
    fn code_set_collects_distinct_sorted_codes() {
        let inv = RawInventory {
            source_urls: vec![],
            property_types: BTreeMap::new(),
            groups: vec![
                RawGroup {
                    category_name: "c".into(),
                    group_id: None,
                    group_name: "g1".into(),
                    is_websocket_group: false,
                    trs: vec![raw_tr("t8412"), raw_tr("t1102")],
                },
                RawGroup {
                    category_name: "c".into(),
                    group_id: None,
                    group_name: "g2".into(),
                    is_websocket_group: false,
                    trs: vec![raw_tr("t8412"), raw_tr("token")],
                },
            ],
        };
        let cs = inv.code_set(true);
        assert_eq!(cs.len(), 3, "duplicate t8412 collapses across groups");
        assert!(cs.provisional);
        assert_eq!(inv.tr_count(), 3);
        let v: Vec<_> = cs.codes.iter().cloned().collect();
        assert_eq!(v, vec!["t1102", "t8412", "token"], "sorted");
    }

    // --- Rate-limit parsing -----------------------------------------------

    #[test]
    fn parse_rate_limits_reads_string_extra_param() {
        let protocol = serde_json::json!({
            "extraParam": "{\"ThroughputQuotaRule\":[{\"tr_cd\":\"t8412\",\"requestLimit\":3}],\
                            \"CorpThroughputQuotaRule\":[{\"tr_cd\":\"t8412\",\"requestLimit\":10}]}"
        });
        let limits = parse_rate_limits(&protocol);
        assert_eq!(limits.get("t8412"), Some(&(Some(3), Some(10))));
    }

    // --- HTTP layer (httpmock, synchronous, no live network) --------------

    #[test]
    fn property_type_mapping_parses_list_response() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/codes/public/system-codes");
            then.status(200).json_body(serde_json::json!({ "list": [
                { "code": "A0001", "name": "String" },
                { "code": "A0003", "name": "Long" }
            ]}));
        });
        let client = FetchClient::new(server.base_url()).unwrap();
        let (map, fell_back) = client.property_type_mapping();
        m.assert();
        assert_eq!(map.get("A0001"), Some(&"String".to_string()));
        assert_eq!(map.len(), 2);
        assert!(!fell_back, "a real mapping response did not fall back");
    }

    #[test]
    fn property_type_mapping_falls_back_on_error_and_continues() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/codes/public/system-codes");
            then.status(500);
        });
        let client = FetchClient::new(server.base_url()).unwrap();
        let (map, fell_back) = client.property_type_mapping();
        // Fallback map is non-empty and the call does not error out.
        assert_eq!(map.get("A0001"), Some(&"String".to_string()));
        assert_eq!(map.len(), PROPERTY_TYPE_FALLBACK.len());
        assert!(fell_back, "a 500 response served the fallback table (U4)");
    }

    #[test]
    fn tr_list_returns_fetch_error_after_retries_exhaust() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/guide/tr/g100");
            then.status(500);
        });
        // Zero delay so retry exhaustion does not sleep the test.
        let retry = RetryConfig {
            max_retries: 2,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        };
        let client = FetchClient::with_retry(server.base_url(), retry).unwrap();
        let err = client.tr_list("g100").unwrap_err();
        assert!(err.to_string().contains("after 3 attempt"));
        m.assert_hits(3); // initial + 2 retries
    }

    /// A full mock scrape: menu → one group → one TR → properties + protocol →
    /// inventory + code-set, with the gate passing.
    #[test]
    fn fetch_full_inventory_against_mock_builds_code_set_and_gate_passes() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/apiservice");
            then.status(200).body(MENU_HTML);
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/guide/tr/g100");
            then.status(200).json_body(serde_json::json!([
                { "trCode": "t8412", "trName": "차트", "id": "tr-1", "description": "주식 차트" }
            ]));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/guide/tr/g200");
            then.status(200).json_body(serde_json::json!([]));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/guide/tr/property/tr-1");
            then.status(200).json_body(serde_json::json!([
                { "propertyCd": "shcode", "bodyType": "req_b", "propertyType": "A0001" }
            ]));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/public/g100");
            then.status(200).json_body(serde_json::json!({
                "id": "grp-100", "httpMethod": "POST", "accessUrl": "/stock/chart",
                "protocolType": "REST",
                "extraParam": "{\"ThroughputQuotaRule\":[{\"tr_cd\":\"t8412\",\"requestLimit\":1}]}"
            }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/public/g200");
            then.status(200).json_body(serde_json::json!({}));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/codes/public/system-codes");
            then.status(200)
                .json_body(serde_json::json!({ "list": [] }));
        });

        let client = FetchClient::new(server.base_url()).unwrap();
        let outcome = client.fetch_full_inventory().expect("mock scrape succeeds");
        let inv = &outcome.inventory;
        let cs = inv.code_set(true);
        assert!(cs.contains("t8412"));
        assert_eq!(cs.len(), 1);

        let tr = &inv.groups[0].trs[0];
        assert_eq!(tr.rate_limit_per_sec, Some(1));
        assert_eq!(
            tr.properties.len(),
            1,
            "raw property rows captured verbatim"
        );

        // Healthy protocol facts (g100 has an `id`) → no degradation; the
        // property-type mapping returned `{ "list": [] }` → empty → fallback.
        assert!(inv.facts_degraded_tr_codes().is_empty());
        assert!(
            outcome.property_type_fallback_served,
            "an empty mapping response served the fallback table"
        );

        let gate = completeness_gate(true, inv.tr_count(), None, DEFAULT_TRUNCATION_PROPORTION);
        assert_eq!(gate, GateOutcome::Pass);
    }

    /// U4: a group whose protocol facts failed has no `group_id`; its TR codes
    /// are recorded as degraded TR codes (not merely counted), so the facts gate
    /// can join on code (KTD-4a). The mapping 500 also flags the fallback.
    #[test]
    fn fetch_records_degraded_tr_codes_and_property_type_fallback() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/apiservice");
            then.status(200).body(MENU_HTML);
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/guide/tr/g100");
            then.status(200).json_body(serde_json::json!([
                { "trCode": "t8412", "trName": "차트", "id": "tr-1" }
            ]));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/guide/tr/g200");
            then.status(200).json_body(serde_json::json!([]));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/guide/tr/property/tr-1");
            then.status(200).json_body(serde_json::json!([]));
        });
        // Protocol facts fail for g100 → its group_id is None (degraded).
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/public/g100");
            then.status(500);
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/apis/public/g200");
            then.status(500);
        });
        // Property-type mapping fails → fallback served.
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/codes/public/system-codes");
            then.status(500);
        });

        let client = FetchClient::new(server.base_url()).unwrap();
        let outcome = client.fetch_full_inventory().expect("scrape succeeds");
        assert!(
            outcome.property_type_fallback_served,
            "the mapping 500 served the fallback (U4)"
        );
        let degraded = outcome.inventory.facts_degraded_tr_codes();
        assert!(
            degraded.contains("t8412"),
            "t8412 lives under a facts-degraded group: {degraded:?}"
        );
    }
}
