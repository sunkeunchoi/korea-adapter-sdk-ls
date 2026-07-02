//! Instrument provider: LS stock masters (t8430/t9945) → nautilus [`Equity`].
//!
//! Per KTD7: `InstrumentId = {shcode}.XKRX`; ISIN (`expcode`) carried on the
//! `Equity`; `price_precision = 0` (KRW integer ticks); `price_increment` = the
//! current band step from [`crate::rules`] for the instrument's reference price;
//! `lot_size` from `memedan`; `tick_scheme = None`. Daily price limits are
//! session-scoped facts, **not** instrument constants, so `max_price`/`min_price`
//! are omitted — the limits ride in `info` for reference and order stepping routes
//! through the rules band lookup.
//!
//! The mapping model represents instrument **domain** explicitly (R2): only
//! [`InstrumentDomain::DomesticEquity`] is built in v1; every other arm returns an
//! explicit [`AdapterError::UnsupportedDomain`] rather than a silent wrong mapping
//! (AE3).

use std::collections::HashMap;

use ls_sdk::market_session::{T8430OutBlock, T8430Request, T9945OutBlock, T9945Request};
use ls_sdk::LsSdk;
use nautilus_core::Params;
use nautilus_core::UnixNanos;
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};
use nautilus_model::instruments::{Equity, InstrumentAny};
use nautilus_model::types::{Currency, Price, Quantity};
use serde_json::Value;
use ustr::Ustr;

use crate::error::{AdapterError, AdapterResult};
use crate::rules::{tick_size, Market, TickRegime};
use crate::KRX_VENUE;

/// The instrument domain being requested. v1 builds only [`Self::DomesticEquity`];
/// the other arms exist so the mapping model accommodates them without redesign
/// (R2) and reject explicitly at resolution time (AE3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentDomain {
    /// Domestic KRX cash equities (KOSPI + KOSDAQ) — the only v1 domain.
    DomesticEquity,
    /// Domestic futures/options — mapped-for, not built in v1.
    DomesticFo,
    /// Overseas cash equities — mapped-for, not built in v1.
    OverseasStock,
    /// Overseas futures/options — mapped-for, not built in v1.
    OverseasFo,
}

impl InstrumentDomain {
    fn as_str(self) -> &'static str {
        match self {
            InstrumentDomain::DomesticEquity => "domestic_equity",
            InstrumentDomain::DomesticFo => "domestic_fo",
            InstrumentDomain::OverseasStock => "overseas_stock",
            InstrumentDomain::OverseasFo => "overseas_fo",
        }
    }
}

/// Enrichment fields sourced from t9945 (ISIN + NXT/ETF flags), keyed by shcode.
#[derive(Debug, Clone, Default)]
struct MasterEnrichment {
    isin: Option<String>,
    nxt_listed: bool,
    etf: bool,
}

/// A borrowed view of t9945 enrichment for one shcode, passed into [`map_equity`].
pub struct MasterEnrichmentView<'a> {
    /// ISIN (t9945 `expcode`), if present.
    pub isin: Option<&'a str>,
    /// NXT-listing flag (t9945 `nxt_chk == "1"`).
    pub nxt_listed: bool,
    /// ETF flag (t9945 `etfchk == "1"`).
    pub etf: bool,
}

/// Parse a stringly-typed numeric field to `i64`.
///
/// Empty (or whitespace) resolves to `0` — the LS masters leave optional numerics
/// blank. A non-empty non-numeric value is a **named** [`AdapterError::FieldParse`]
/// (never a panic). Accepts an integer or a decimal form (truncating), since the
/// gateway occasionally decorates integer prices with `.00`.
fn parse_krw(field: &str, value: &str) -> AdapterResult<i64> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(0);
    }
    if let Ok(i) = v.parse::<i64>() {
        return Ok(i);
    }
    if let Ok(f) = v.parse::<f64>() {
        return Ok(f.trunc() as i64);
    }
    Err(AdapterError::FieldParse {
        field: field.to_string(),
        value: value.to_string(),
        reason: "expected an integer KRW amount".to_string(),
    })
}

fn market_str(market: Market) -> &'static str {
    match market {
        Market::Kospi => "KOSPI",
        Market::Kosdaq => "KOSDAQ",
    }
}

/// Map one t8430 stock-issue row (optionally enriched from t9945) to a nautilus
/// [`Equity`], per KTD7.
///
/// # Errors
///
/// - [`AdapterError::FieldParse`] on a malformed numeric field (named).
/// - [`AdapterError::Config`] if `Equity::new_checked` rejects the mapped values.
pub fn map_equity(
    row: &T8430OutBlock,
    enrichment: Option<&MasterEnrichmentView<'_>>,
    ts: UnixNanos,
) -> AdapterResult<Equity> {
    let shcode = row.shcode.trim();
    if shcode.is_empty() {
        return Err(AdapterError::FieldParse {
            field: "shcode".to_string(),
            value: row.shcode.clone(),
            reason: "empty short code".to_string(),
        });
    }

    let market = Market::from_gubun(&row.gubun);
    let instrument_id = InstrumentId::new(Symbol::from(shcode), Venue::from(KRX_VENUE));

    // Reference price picks the current tick band (KTD7): 기준가 (recprice), else
    // 전일가 (jnilclose). The instrument's static increment uses today's regime.
    let recprice = parse_krw("recprice", &row.recprice)?;
    let jnilclose = parse_krw("jnilclose", &row.jnilclose)?;
    let reference = if recprice > 0 { recprice } else { jnilclose };
    let tick = tick_size(market, TickRegime::Post2023, reference.max(0))?;
    let price_increment = Price::from(tick.to_string().as_str());

    // Lot size from 주문수량단위 (memedan); default 1 (lot must be positive).
    let lot_raw = parse_krw("memedan", &row.memedan)?;
    let lot = if lot_raw > 0 { lot_raw } else { 1 };
    let lot_size = Some(Quantity::from(lot));

    // Daily price limits are session-scoped — carried in `info`, NOT as instrument
    // constants (KTD7). ETF/ISIN/NXT enrichment from t9945.
    let uplmt = parse_krw("uplmtprice", &row.uplmtprice)?;
    let dnlmt = parse_krw("dnlmtprice", &row.dnlmtprice)?;
    let etf_from_t8430 = row.etfgubun.trim() == "1";
    let (isin_from_t9945, nxt_listed, etf) = match enrichment {
        Some(e) => (
            e.isin
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string()),
            e.nxt_listed,
            e.etf || etf_from_t8430,
        ),
        None => (None, false, etf_from_t8430),
    };
    // Fall back to t8430's expcode as the ISIN when t9945 carries none.
    let isin = isin_from_t9945.or_else(|| {
        let e = row.expcode.trim();
        (!e.is_empty()).then(|| e.to_string())
    });

    let mut info = Params::new();
    info.insert(
        "market".to_string(),
        Value::String(market_str(market).to_string()),
    );
    info.insert(
        "hname".to_string(),
        Value::String(row.hname.trim().to_string()),
    );
    info.insert("etf".to_string(), Value::Bool(etf));
    info.insert("nxt_listed".to_string(), Value::Bool(nxt_listed));
    info.insert("daily_upper_limit".to_string(), Value::from(uplmt));
    info.insert("daily_lower_limit".to_string(), Value::from(dnlmt));
    info.insert("reference_price".to_string(), Value::from(reference));

    let equity = Equity::new_checked(
        instrument_id,
        Symbol::from(shcode),
        isin.as_deref().map(Ustr::from),
        Currency::KRW(),
        0, // price_precision — KRW integer ticks
        price_increment,
        lot_size,
        None, // max_quantity
        None, // min_quantity
        None, // max_price  — daily limits are session-scoped (KTD7)
        None, // min_price
        None, // margin_init
        None, // margin_maint
        None, // maker_fee
        None, // taker_fee
        None, // tick_scheme — KRX fits TieredTickScheme but has no custom-name registry (KTD7)
        Some(info),
        ts,
        ts,
    )
    .map_err(|e| AdapterError::Config(format!("equity {shcode}: {e}")))?;

    Ok(equity)
}

/// Whole-universe domestic-equity instrument provider (fetch-cache-emit, mirroring
/// the OKX adapter pattern).
pub struct InstrumentProvider {
    sdk: LsSdk,
    cache: HashMap<InstrumentId, Equity>,
}

impl InstrumentProvider {
    /// Build an empty provider over an SDK handle.
    pub fn new(sdk: LsSdk) -> Self {
        InstrumentProvider {
            sdk,
            cache: HashMap::new(),
        }
    }

    /// Resolve + load a domain's instruments, caching them. Only
    /// [`InstrumentDomain::DomesticEquity`] is built; every other domain returns
    /// [`AdapterError::UnsupportedDomain`] (AE3).
    ///
    /// Returns the number of instruments cached.
    pub async fn load_domain(&mut self, domain: InstrumentDomain) -> AdapterResult<usize> {
        match domain {
            InstrumentDomain::DomesticEquity => self.load_domestic_equities().await,
            other => Err(AdapterError::UnsupportedDomain {
                domain: other.as_str().to_string(),
            }),
        }
    }

    /// Load the whole domestic KRX equity universe: t8430 (all markets) enriched
    /// with per-market t9945 ISIN/NXT flags.
    pub async fn load_domestic_equities(&mut self) -> AdapterResult<usize> {
        let session = self.sdk.market_session();

        // Enrichment: t9945 per market ("1" KOSPI, "2" KOSDAQ) → shcode map.
        let mut enrich: HashMap<String, MasterEnrichment> = HashMap::new();
        for gubun in ["1", "2"] {
            let resp = session.stock_master(&T9945Request::new(gubun)).await?;
            for r in &resp.outblock {
                enrich_insert(&mut enrich, r);
            }
        }

        // Master list: t8430 all markets.
        let issues = session.stock_issues(&T8430Request::all()).await?;
        let ts = UnixNanos::default();
        let mut count = 0usize;
        for row in &issues.outblock {
            if row.shcode.trim().is_empty() {
                continue;
            }
            let e = enrich.get(row.shcode.trim());
            let view = e.map(|m| MasterEnrichmentView {
                isin: m.isin.as_deref(),
                nxt_listed: m.nxt_listed,
                etf: m.etf,
            });
            let equity = map_equity(row, view.as_ref(), ts)?;
            self.cache.insert(equity.id, equity);
            count += 1;
        }
        Ok(count)
    }

    /// Look up a cached instrument by id.
    pub fn get(&self, id: &InstrumentId) -> Option<&Equity> {
        self.cache.get(id)
    }

    /// Iterate all cached instruments.
    pub fn all(&self) -> impl Iterator<Item = &Equity> {
        self.cache.values()
    }

    /// All cached instruments wrapped as [`InstrumentAny`] (for the catalog/engine).
    pub fn all_any(&self) -> Vec<InstrumentAny> {
        self.cache
            .values()
            .cloned()
            .map(InstrumentAny::Equity)
            .collect()
    }

    /// Number of cached instruments.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

fn enrich_insert(map: &mut HashMap<String, MasterEnrichment>, r: &T9945OutBlock) {
    let shcode = r.shcode.trim();
    if shcode.is_empty() {
        return;
    }
    map.insert(
        shcode.to_string(),
        MasterEnrichment {
            isin: {
                let e = r.expcode.trim();
                (!e.is_empty()).then(|| e.to_string())
            },
            nxt_listed: r.nxt_chk.trim() == "1",
            etf: r.etfchk.trim() == "1",
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> T8430OutBlock {
        T8430OutBlock {
            hname: "삼성전자".to_string(),
            shcode: "005930".to_string(),
            expcode: "KR7005930003".to_string(),
            etfgubun: "0".to_string(),
            uplmtprice: "78000".to_string(),
            dnlmtprice: "42000".to_string(),
            jnilclose: "60000".to_string(),
            memedan: "1".to_string(),
            recprice: "60000".to_string(),
            gubun: "1".to_string(),
        }
    }

    #[test]
    fn maps_row_to_equity_per_ktd7() {
        let eq = map_equity(&sample_row(), None, UnixNanos::default()).unwrap();
        assert_eq!(eq.id, InstrumentId::from("005930.XKRX"));
        assert_eq!(eq.raw_symbol, Symbol::from("005930"));
        assert_eq!(eq.price_precision, 0);
        // 60,000 KRW KOSPI post-2023 → tick 100.
        assert_eq!(eq.price_increment, Price::from("100"));
        assert_eq!(eq.lot_size, Some(Quantity::from(1)));
        assert_eq!(eq.isin, Some(Ustr::from("KR7005930003")));
        // Daily limits are NOT baked into the instrument (KTD7).
        assert!(eq.max_price.is_none());
        assert!(eq.min_price.is_none());
        assert!(eq.tick_scheme.is_none());
    }

    #[test]
    fn etf_row_maps_as_equity_with_etf_flag_in_info() {
        let mut row = sample_row();
        row.shcode = "069500".to_string();
        row.etfgubun = "1".to_string();
        let eq = map_equity(&row, None, UnixNanos::default()).unwrap();
        let info = eq.info.expect("info present");
        assert_eq!(info.get_bool("etf"), Some(true));
        assert_eq!(info.get_str("market"), Some("KOSPI"));
    }

    #[test]
    fn malformed_numeric_field_errors_with_name_not_panic() {
        let mut row = sample_row();
        row.recprice = "not-a-number".to_string();
        let err = map_equity(&row, None, UnixNanos::default()).unwrap_err();
        match err {
            AdapterError::FieldParse { field, .. } => assert_eq!(field, "recprice"),
            other => panic!("expected FieldParse(recprice), got {other:?}"),
        }
    }

    #[test]
    fn kosdaq_reference_uses_market_ladder() {
        let mut row = sample_row();
        row.gubun = "2".to_string();
        row.shcode = "035720".to_string();
        let eq = map_equity(&row, None, UnixNanos::default()).unwrap();
        assert_eq!(eq.price_increment, Price::from("100"));
        assert_eq!(eq.info.unwrap().get_str("market"), Some("KOSDAQ"));
    }

    #[test]
    fn empty_reference_price_defaults_to_smallest_tick() {
        let mut row = sample_row();
        row.recprice = String::new();
        row.jnilclose = String::new();
        let eq = map_equity(&row, None, UnixNanos::default()).unwrap();
        assert_eq!(eq.price_increment, Price::from("1"));
    }
}
