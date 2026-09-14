//! The rehearsal's price inputs (U9): the marketable-limit policy's feed and instrument
//! index, the paced t8407 sweep, and the 15:20 synthetic daily bar (KTD4, KTD5).

use std::collections::HashMap;
use std::time::Duration;

use ls_sdk::LsSdk;
use nautilus_ls::execution::{
    LastPriceSource, SessionInstruments,
};
use nautilus_ls::ingest::BarKind;
use nautilus_model::data::{Bar, BarType};
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::instruments::{Equity, InstrumentAny};
use nautilus_model::types::{Price, Quantity};

use crate::strategy::hooks::MarkFeed;


// ---------------------------------------------------------------------------
// The session's price + instrument sources (KTD5)
// ---------------------------------------------------------------------------

/// The marketable-limit policy's decision-time price feed, backed by the session's own
/// sweep (KTD5).
///
/// Reads the [`MarkFeed`] the sweep publishes into rather than fetching: the adapter stays
/// translation-only, and the price an order is anchored to is the same observation the
/// strategy decided on. A symbol the sweep never saw yields `None`, which DENIES the order
/// — the one behavior that makes a missing mark visible rather than priced at zero.
#[derive(Debug, Clone)]
pub struct SweptPrices {
    marks: MarkFeed,
}

impl SweptPrices {
    /// Read prices off `marks`.
    #[must_use]
    pub fn new(marks: MarkFeed) -> Self {
        SweptPrices { marks }
    }
}

impl LastPriceSource for SweptPrices {
    fn last_price(&self, instrument_id: &InstrumentId) -> Option<i64> {
        self.marks
            .get(instrument_id.symbol.as_str())
            .map(|m| m.last_close)
            .filter(|p| *p > 0)
    }
}

/// The session's cached equities — the clamp band and market segment the policy needs.
#[derive(Debug, Clone, Default)]
pub struct SessionEquities {
    by_id: HashMap<InstrumentId, Equity>,
}

impl SessionEquities {
    /// Index the session's instruments.
    #[must_use]
    pub fn new(instruments: &[InstrumentAny]) -> Self {
        SessionEquities {
            by_id: instruments
                .iter()
                .filter_map(|i| match i {
                    InstrumentAny::Equity(e) => Some((e.id, e.clone())),
                    _ => None,
                })
                .collect(),
        }
    }

    /// How many equities are indexed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether nothing is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

impl SessionInstruments for SessionEquities {
    fn equity(&self, instrument_id: &InstrumentId) -> Option<Equity> {
        self.by_id.get(instrument_id).cloned()
    }
}

// ---------------------------------------------------------------------------
// The t8407 sweep
// ---------------------------------------------------------------------------

/// t8407 packs six-character codes back to back, so a batch is a fixed width count.
pub(crate) const T8407_BATCH: usize = 25;

/// The pause between t8407 batches, matching `mount_universe`'s reader of the same endpoint.
///
/// The gateway's dispatch budget is CUMULATIVE and warm-sensitive rather than a per-second
/// rate, so the cost that matters is calls-per-session, and this loop makes far more of them
/// than the one-shot universe read does: a held book plus candidates reaches ~144 symbols,
/// which is six batches, repeated every poll tick for the whole pre-decision window. Firing
/// six back-to-back reads on every tick is the shape that earns an `IGW00201`.
const T8407_BATCH_PACE: Duration = Duration::from_millis(400);

/// One symbol's observed quote from a t8407 sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    /// Current price / 현재가 (KRW).
    pub price: i64,
    /// Session open / 시가.
    pub open: i64,
    /// Session high / 고가.
    pub high: i64,
    /// Session low / 저가.
    pub low: i64,
    /// Cumulative volume / 누적거래량.
    pub volume: i64,
}

/// Sweep `wanted` in 25-symbol batches, returning every row that parsed into a usable quote.
///
/// Partial by design: a symbol the gateway omits or quotes unusably is ABSENT from the map
/// rather than defaulted, and the caller decides what its absence means (a held symbol gets
/// a `held_symbol_gaps` row and keeps its position; a candidate simply is not entered). A
/// batch that fails outright propagates — a swept sweep that silently lost a batch would
/// look identical to a market where nothing traded.
///
/// # Errors
///
/// A malformed shcode, or a failed batch.
pub async fn sweep_quotes(
    sdk: &LsSdk,
    wanted: &[InstrumentId],
) -> anyhow::Result<HashMap<InstrumentId, Quote>> {
    use ls_sdk::market_session::T8407Request;

    let mut by_shcode: HashMap<String, InstrumentId> = HashMap::new();
    for id in wanted {
        let code = id.symbol.as_str().to_string();
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            anyhow::bail!(
                "instrument {id} yields shcode {code:?}, which is not six ASCII digits — t8407 \
                 packs codes at a fixed six-character width, so one off-length code mis-frames \
                 its whole batch and silently quotes the wrong symbols"
            );
        }
        by_shcode.insert(code, *id);
    }

    let mut out: HashMap<InstrumentId, Quote> = HashMap::new();
    let codes: Vec<&String> = by_shcode.keys().collect();
    let batches: Vec<&[&String]> = codes.chunks(T8407_BATCH).collect();
    for (i, batch) in batches.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(T8407_BATCH_PACE).await;
        }
        let packed: String = batch.iter().map(|c| c.as_str()).collect();
        let req = T8407Request::new(batch.len().to_string(), packed);
        let resp = sdk
            .market_session()
            .multi_symbol_current_price(&req)
            .await
            .map_err(|e| anyhow::anyhow!("t8407 batch {}/{}: {e}", i + 1, batches.len()))?;
        for row in &resp.outblock1 {
            // A number-typed echo loses leading zeros, so re-pad before matching or every
            // 0-prefixed KRX code would miss.
            let code = format!("{:0>6}", row.shcode.trim());
            let Some(id) = by_shcode.get(&code) else { continue };
            let num = |s: &str| s.trim().parse::<i64>().ok().filter(|v| *v > 0);
            let (Some(price), Some(open), Some(high), Some(low)) =
                (num(&row.price), num(&row.open), num(&row.high), num(&row.low))
            else {
                continue;
            };
            out.insert(
                *id,
                Quote {
                    price,
                    open,
                    high,
                    low,
                    // Volume alone may legitimately be zero (a symbol that has not traded),
                    // so it is the one field a zero does not disqualify.
                    volume: row.volume.trim().parse::<i64>().unwrap_or(0).max(0),
                },
            );
        }
    }
    Ok(out)
}

/// The synthetic 1-DAY bar for one symbol's decision read (KTD4).
///
/// `ts_event` is the DECISION instant, not the read's arrival time: the strategy counts
/// nothing off it, but every artifact that records when a leg opened does, and a bar stamped
/// at wall-clock arrival would date a session's entries by how long a retry took.
///
/// # Errors
///
/// A bar-type construction failure.
pub fn synthetic_bar(bar_type: BarType, quote: &Quote, decision_unix: i64) -> anyhow::Result<Bar> {
    let ns = u64::try_from(decision_unix.max(0)).unwrap_or(0) * 1_000_000_000;
    Ok(Bar::new(
        bar_type,
        Price::new(quote.open as f64, 0),
        Price::new(quote.high as f64, 0),
        Price::new(quote.low as f64, 0),
        Price::new(quote.price as f64, 0),
        Quantity::from(quote.volume.max(0) as u64),
        ns.into(),
        ns.into(),
    ))
}

/// The bar type the daily strategy subscribes for `id`.
///
/// # Errors
///
/// A bar-type construction failure.
pub fn daily_bar_type(id: InstrumentId) -> anyhow::Result<BarType> {
    BarKind::Daily.bar_type(id).map_err(|e| anyhow::anyhow!(e))
}

