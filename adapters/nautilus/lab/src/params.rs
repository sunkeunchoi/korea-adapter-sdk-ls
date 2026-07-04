//! ORB v0 parameter set (KTD6) — every value the strategy uses, serde-round-tripped
//! so the whole set lands in the run manifest (R3, R8). These are *starter defaults*
//! the loop exists to revise, never tuned claims.

use std::collections::BTreeMap;

use chrono::NaiveTime;
use nautilus_ls::rules::KRX_REGULAR_OPEN;
use serde::{Deserialize, Serialize};

use crate::agent::context::AgentContext;

/// The strategy identifier recorded in every run id + manifest.
pub const STRATEGY_ID: &str = "orb";

/// The opening-range-breakout parameter set. All fields are manifest-recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbParams {
    /// Strategy identifier (stable across versions).
    pub strategy_id: String,
    /// Strategy version — bumped by a loop turn when the strategy changes (KTD8).
    pub strategy_version: u32,
    /// Universe gap filter: today's open must be at least this % above the prior
    /// session close (a "stock in play"). KTD6 default 3.0.
    pub gap_min_pct: f64,
    /// Universe cap: keep the top-N candidates ranked by prior-session turnover.
    pub universe_top_n: usize,
    /// Risk cap: never hold more than this many positions at once.
    pub max_concurrent: usize,
    /// Opening-range window start (KST). Default the adapter's regular-session open
    /// (09:00) — never re-declared here (KTD6).
    #[serde(with = "hhmmss")]
    pub range_open: NaiveTime,
    /// Opening-range window length in minutes (09:00 + 15 = 09:15 default).
    pub range_minutes: i64,
    /// Time-flat deadline (KST): any open position is closed at/after this time.
    #[serde(with = "hhmmss")]
    pub flat_time: NaiveTime,
    /// Fixed notional (KRW) targeted per position; the entry quantity is
    /// `floor(notional / entry_price)`.
    pub notional_per_position: f64,
}

impl Default for OrbParams {
    fn default() -> Self {
        OrbParams {
            strategy_id: STRATEGY_ID.to_string(),
            strategy_version: 0,
            gap_min_pct: 3.0,
            universe_top_n: 20,
            max_concurrent: 5,
            range_open: KRX_REGULAR_OPEN,
            range_minutes: 15,
            // 15:00 KST time-flat (before the 15:30 regular close).
            flat_time: NaiveTime::from_hms_opt(15, 0, 0).expect("valid time"),
            notional_per_position: 10_000_000.0,
        }
    }
}

impl OrbParams {
    /// The opening-range window end (KST): `range_open + range_minutes`.
    pub fn range_end(&self) -> NaiveTime {
        self.range_open + chrono::Duration::minutes(self.range_minutes)
    }

    /// The number of shares to buy for a `notional_per_position` budget at `price`
    /// (floored). Zero when the price exceeds the notional — the sizing gate then
    /// rejects the entry rather than placing a zero-quantity order.
    pub fn position_qty(&self, price: f64) -> i64 {
        if price <= 0.0 {
            return 0;
        }
        (self.notional_per_position / price).floor() as i64
    }

    /// Whether a new position may be opened given the current open-position count
    /// (the `max_concurrent` risk cap, KTD6).
    pub fn sizing_allows(&self, open_positions: usize) -> bool {
        open_positions < self.max_concurrent
    }

    /// The numeric (f64-able) fields of this parameter set, keyed by serde
    /// field name. String-typed fields (strategy id, `HH:MM:SS` times) are
    /// omitted — context params maps are `f64`-valued.
    pub fn numeric_summary(&self) -> BTreeMap<String, f64> {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(map)) => map
                .into_iter()
                .filter_map(|(k, v)| v.as_f64().map(|n| (k, n)))
                .collect(),
            _ => BTreeMap::new(),
        }
    }

    /// The minimal in-run telemetry context (R5) for a decision made under this
    /// parameter set: strategy id + version, the [`OrbParams::numeric_summary`]
    /// as the params summary, and the caller's running counts. Constructible at
    /// the universe scan (before the engine) and inside the engine thread — no
    /// account or position state (R9).
    pub fn telemetry_context(&self, counts: BTreeMap<String, u64>) -> AgentContext {
        AgentContext::telemetry(
            self.strategy_id.clone(),
            self.strategy_version,
            self.numeric_summary(),
            counts,
        )
    }
}

/// Serialize/deserialize a `NaiveTime` as `"HH:MM:SS"` so the manifest is readable
/// and diff-friendly (chrono's default is the same, but pinning the format keeps the
/// manifest stable across chrono versions).
mod hhmmss {
    use chrono::NaiveTime;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FMT: &str = "%H:%M:%S";

    pub fn serialize<S: Serializer>(t: &NaiveTime, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&t.format(FMT).to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveTime, D::Error> {
        let raw = String::deserialize(d)?;
        NaiveTime::parse_from_str(&raw, FMT).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_ktd6() {
        let p = OrbParams::default();
        assert_eq!(p.strategy_id, "orb");
        assert_eq!(p.strategy_version, 0);
        assert_eq!(p.gap_min_pct, 3.0);
        assert_eq!(p.universe_top_n, 20);
        assert_eq!(p.max_concurrent, 5);
        assert_eq!(p.range_open, KRX_REGULAR_OPEN);
        assert_eq!(p.range_end(), NaiveTime::from_hms_opt(9, 15, 0).unwrap());
        assert_eq!(p.flat_time, NaiveTime::from_hms_opt(15, 0, 0).unwrap());
    }

    #[test]
    fn params_round_trip_through_json() {
        let p = OrbParams::default();
        let json = serde_json::to_string(&p).unwrap();
        let back: OrbParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
        // Time fields are human-readable in the manifest.
        assert!(json.contains("\"09:00:00\""), "json: {json}");
        assert!(json.contains("\"15:00:00\""), "json: {json}");
    }

    #[test]
    fn position_qty_floors_and_guards_zero() {
        let mut p = OrbParams { notional_per_position: 1_000_000.0, ..Default::default() };
        assert_eq!(p.position_qty(60_000.0), 16); // 1_000_000 / 60_000 = 16.6 → 16
        assert_eq!(p.position_qty(0.0), 0);
        p.notional_per_position = 100.0;
        assert_eq!(p.position_qty(60_000.0), 0, "price above notional → zero shares");
    }

    #[test]
    fn telemetry_context_carries_numeric_params_only() {
        let p = OrbParams::default();
        let summary = p.numeric_summary();
        assert_eq!(summary.get("gap_min_pct"), Some(&3.0));
        assert!(summary.contains_key("notional_per_position"));
        assert!(!summary.contains_key("strategy_id"), "string fields omitted");
        assert!(!summary.contains_key("range_open"), "HH:MM:SS fields omitted");

        let counts = BTreeMap::from([("decisions".to_string(), 7u64)]);
        let ctx = p.telemetry_context(counts.clone());
        let AgentContext::Telemetry {
            strategy_id, strategy_version, params_hash_or_summary, counts: got,
        } = ctx
        else {
            panic!("expected the Telemetry form");
        };
        assert_eq!(strategy_id, "orb");
        assert_eq!(strategy_version, 0);
        assert_eq!(params_hash_or_summary, summary);
        assert_eq!(got, counts);
    }

    #[test]
    fn sizing_gate_caps_concurrency() {
        let p = OrbParams::default(); // max_concurrent 5
        assert!(p.sizing_allows(4));
        assert!(!p.sizing_allows(5));
        assert!(!p.sizing_allows(6));
    }
}
