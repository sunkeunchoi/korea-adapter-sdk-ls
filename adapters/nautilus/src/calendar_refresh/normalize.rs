//! Native KRX/KASI response normalization + generated-rule evidence (U3, KTD3/KTD7/KTD8).
//!
//! This module turns the maintainer transport's raw response bodies into the already-normalized
//! [`EvidenceRecord`] shapes the refresh core consumes — and generates the deterministic-rule
//! evidence (weekends + fixed KRX exchange closures) that no external feed provides. It does NO
//! transport and NO I/O: every function takes a `&str` body or a window and returns evidence, so
//! the offline gate exercises every path with hand-crafted synthetic fixtures (R14).
//!
//! ## Determinism (KTD8)
//!
//! Every generated `recorded_at` is midnight-UTC of the evidence date — never wall-clock `now()`.
//! A re-run or resumed fetch therefore produces byte-identical evidence records, so the artifact
//! id is stable and the repeatability claim holds.
//!
//! ## The KTD7 witness gate
//!
//! KRX daily-market normalization runs the parsed response through
//! [`witness_from_response`](nautilus_ls_calendar::witness::witness_from_response): only a
//! successful, structurally valid, date-matched, KOSPI-bearing response yields a witness. Every
//! other response (empty, malformed, failed, error-enveloped, mismatched) yields NO record — it
//! can never prove `Closed` and never retract a prior witness by absence.
//!
//! ## Native envelopes are reconciled at U8
//!
//! The exact KRX/KASI envelopes are captured by U8's bounded probes; these parsers are written
//! against documented shapes and hand-crafted synthetic fixtures, and a probe-gate mismatch
//! routes back here before the live bulk fetch resumes.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Deserialize;

use nautilus_ls_calendar::schema::{EvidenceKind, EvidenceRecord};
use nautilus_ls_calendar::witness::{default_witness_id, witness_from_response, KrxDailyMarketResponse, KrxDailyRow, WitnessOutcome};

use super::port::DateRange;

/// The KASI holiday-facts source id.
pub const KASI_SOURCE_ID: &str = "kasi";
/// The generated KRX deterministic-rule source id (weekends, fixed closures, holiday links).
pub const KRX_RULE_SOURCE_ID: &str = "krx-rule";
/// The KRX daily-market witness source id.
pub const KRX_DAILY_SOURCE_ID: &str = "krx-daily";

/// Midnight-UTC of `date` — the deterministic `recorded_at` for every generated record (KTD8).
pub fn midnight_utc(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
}

fn parse_yyyymmdd(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y%m%d").ok()
}

fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

// ---------------------------------------------------------------------------------------
// KRX daily-market (`stk_bydd_trd`) native envelope → witness
// ---------------------------------------------------------------------------------------

/// The native KRX `stk_bydd_trd` JSON envelope: an `OutBlock_1` row array plus an optional
/// error code. Deliberately minimal — only the two fields the witness rule needs are read.
#[derive(Debug, Deserialize)]
struct KrxDailyEnvelope {
    #[serde(rename = "OutBlock_1", default)]
    out_block_1: Vec<KrxOutRow>,
    /// KRX returns an error code (e.g. an invalid-request or throttle envelope) here on failure.
    #[serde(rename = "errCode", default)]
    err_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KrxOutRow {
    /// Base date, `YYYYMMDD`.
    #[serde(rename = "BAS_DD", default)]
    bas_dd: String,
    /// Market name (e.g. `KOSPI`).
    #[serde(rename = "MKT_NM", default)]
    mkt_nm: String,
}

/// Parse a native KRX `stk_bydd_trd` JSON body for `requested` into the normalized
/// [`KrxDailyMarketResponse`] the KTD7 witness gate consumes. An `errCode` envelope becomes a
/// failed response (→ `ErrorEnvelope`); a structurally broken row (blank `BAS_DD`) surfaces as a
/// blank-market row (→ `Malformed`); unparseable JSON is a normalization `Err` (→ Failed source).
pub fn parse_krx_daily(body: &str, requested: NaiveDate) -> Result<KrxDailyMarketResponse, String> {
    let env: KrxDailyEnvelope = serde_json::from_str(body)
        .map_err(|e| format!("KRX response could not be normalized: {e}"))?;

    if let Some(code) = env.err_code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        return Ok(KrxDailyMarketResponse {
            success: false,
            requested_date: requested,
            rows: Vec::new(),
            error_code: Some(code.to_string()),
        });
    }

    let mut rows = Vec::with_capacity(env.out_block_1.len());
    for r in env.out_block_1 {
        match parse_yyyymmdd(&r.bas_dd) {
            Some(date) => rows.push(KrxDailyRow { date, market: r.mkt_nm }),
            // A blank/broken date is a structurally malformed row: keep it (blank market) so the
            // gate classifies the response Malformed, rather than silently dropping the row.
            None => rows.push(KrxDailyRow { date: requested, market: String::new() }),
        }
    }

    Ok(KrxDailyMarketResponse {
        success: true,
        requested_date: requested,
        rows,
        error_code: None,
    })
}

/// Normalize a parsed KRX response into a positive-witness [`EvidenceRecord`] via the KTD7 gate,
/// stamped with the stable `krx-witness-<date>` id, the KRX daily source, and a deterministic
/// midnight-UTC `recorded_at`. Returns `None` for any non-qualifying response (no record ever
/// proves `Closed`).
pub fn witness_evidence(resp: &KrxDailyMarketResponse) -> Option<EvidenceRecord> {
    match witness_from_response(resp) {
        WitnessOutcome::Witness(mut w) => {
            w.id = default_witness_id(resp.requested_date);
            w.source_id = KRX_DAILY_SOURCE_ID.to_string();
            // `witness_from_response` already stamps midnight-UTC of the requested date (KTD8);
            // re-assert it so the deterministic guarantee is local to this normalizer.
            w.recorded_at = midnight_utc(resp.requested_date);
            Some(w)
        }
        WitnessOutcome::NonEvidence(_) => None,
    }
}

// ---------------------------------------------------------------------------------------
// KASI (`getRestDeInfo`) native XML → holiday facts + paired rules
// ---------------------------------------------------------------------------------------

/// One parsed page of a KASI `getRestDeInfo` response: the holiday civil dates on the page plus
/// the pagination fields the fetch loop advances on (`totalCount`/`numOfRows`/`pageNo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KasiPage {
    /// The holiday civil dates on this page (`isHoliday == Y`), ascending and deduped.
    pub holidays: Vec<NaiveDate>,
    /// The total holiday count across all pages (KASI `totalCount`).
    pub total_count: u32,
    /// The page size (KASI `numOfRows`).
    pub num_of_rows: u32,
    /// This page's number (KASI `pageNo`).
    pub page_no: u32,
}

/// Parse a native KASI `getRestDeInfo` XML body into a [`KasiPage`]. Uses a non-DTD-expanding
/// event reader (quick-xml resolves only the five predefined XML entities and never fetches an
/// external DTD/entity, KTD9), so a hostile `<!DOCTYPE …>` cannot expand. Handles the empty-year
/// (no `<item>`) and single-item (`<item>` appears once, not as an array) shapes naturally.
pub fn parse_kasi_holidays_xml(body: &str) -> Result<KasiPage, String> {
    let mut reader = Reader::from_str(body);
    let mut buf: Vec<u8> = Vec::new();

    let mut holidays: Vec<NaiveDate> = Vec::new();
    let (mut total_count, mut num_of_rows, mut page_no) = (0u32, 0u32, 0u32);

    // Per-<item> accumulation.
    let mut in_item = false;
    let mut item_is_holiday = false;
    let mut item_locdate: Option<NaiveDate> = None;
    // The leaf element whose text we are currently reading.
    let mut current: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if tag == "item" {
                    in_item = true;
                    item_is_holiday = false;
                    item_locdate = None;
                }
                current = Some(tag);
            }
            Ok(Event::Text(t)) => {
                if let Some(tag) = current.as_deref() {
                    let text = t
                        .unescape()
                        .map_err(|e| format!("KASI XML text could not be decoded: {e}"))?
                        .trim()
                        .to_string();
                    if text.is_empty() {
                        continue;
                    }
                    match tag {
                        "isHoliday" => item_is_holiday = text.eq_ignore_ascii_case("Y"),
                        "locdate" => item_locdate = parse_yyyymmdd(&text),
                        "totalCount" => total_count = text.parse().unwrap_or(0),
                        "numOfRows" => num_of_rows = text.parse().unwrap_or(0),
                        "pageNo" => page_no = text.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if tag == "item" {
                    if in_item && item_is_holiday {
                        if let Some(date) = item_locdate {
                            holidays.push(date);
                        }
                    }
                    in_item = false;
                }
                current = None;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("KASI XML parse error: {e}")),
            _ => {}
        }
        buf.clear();
    }

    holidays.sort();
    holidays.dedup();
    Ok(KasiPage {
        holidays,
        total_count,
        num_of_rows,
        page_no,
    })
}

/// The KASI holiday fact + its paired KRX deterministic-rule record for `date` (KTD3): the fact
/// carries the holiday, the rule connects it to the scoped session so reconciliation classifies
/// the date `Closed`. Both stamped with a deterministic midnight-UTC `recorded_at` (KTD8) and the
/// stable `kasi-<date>` / `rule-<date>` ids.
pub fn holiday_evidence(date: NaiveDate) -> (EvidenceRecord, EvidenceRecord) {
    (
        record(format!("kasi-{date}"), KASI_SOURCE_ID, date, EvidenceKind::HolidayFact),
        rule_record(date),
    )
}

// ---------------------------------------------------------------------------------------
// Generated deterministic-rule evidence (weekends + fixed exchange closures) — KTD3
// ---------------------------------------------------------------------------------------

/// One weekend-closure [`DeterministicRule`](EvidenceKind::DeterministicRule) record per Saturday
/// and Sunday in `window`. Without these every weekend since the history floor is Unknown and the
/// R12 zero-Unknown gate can never pass (research confirmed no weekend logic exists elsewhere).
pub fn weekend_rules(window: DateRange) -> Vec<EvidenceRecord> {
    let mut out = Vec::new();
    let mut cur = window.from;
    while cur <= window.through {
        if matches!(cur.weekday(), Weekday::Sat | Weekday::Sun) {
            out.push(rule_record(cur));
        }
        match cur.succ_opt() {
            Some(next) => cur = next,
            None => break,
        }
    }
    out
}

/// The KRX year-end closing day for `year`: December 31, or — per the exchange's published
/// rule (「유가증권시장 업무규정 시행세칙」: 12월 31일이 공휴일 또는 토요일인 경우 직전
/// 매매거래일) — the preceding weekday when December 31 falls on a weekend. Stepping over
/// weekends only is a bounded under-approximation of "preceding trading day": if that weekday
/// were itself a public holiday the true closure would sit one day earlier, but the claimed
/// day is Closed either way (holiday + rule), so the approximation can never mark a real
/// trading day Closed — and a positive witness overrides the rule regardless.
fn year_end_closure_day(year: i32) -> Option<NaiveDate> {
    let mut date = NaiveDate::from_ymd_opt(year, 12, 31)?;
    while !is_weekday(date) {
        date = date.pred_opt()?;
    }
    Some(date)
}

/// One fixed-closure rule per KRX exchange-only holiday in `window` that no external feed
/// provides (KTD3): the year-end closing day (December 31, shifted to the preceding weekday
/// when it falls on a weekend — see [`year_end_closure_day`]) and Labor Day (May 1, NEVER
/// shifted: KRX publishes no substitute closure when it falls on a weekend, which the
/// weekend rule already covers). Any residual exchange-only closure is adjudicated in the
/// discrepancy flow, not guessed here.
pub fn fixed_closure_rules(window: DateRange) -> Vec<EvidenceRecord> {
    let mut out = Vec::new();
    for year in window.from.year()..=window.through.year() {
        if let Some(date) = year_end_closure_day(year) {
            if window.contains(date) {
                out.push(rule_record(date));
            }
        }
        if let Some(date) = NaiveDate::from_ymd_opt(year, 5, 1) {
            if window.contains(date) && is_weekday(date) {
                out.push(rule_record(date));
            }
        }
    }
    out
}

/// All generated deterministic-rule evidence over `window` (weekends + fixed exchange closures),
/// sorted by date then id and deduped by id — the rule set the genesis/refresh build consumes
/// alongside the KRX witnesses and KASI holiday facts.
pub fn generated_rules(window: DateRange) -> Vec<EvidenceRecord> {
    let mut out = weekend_rules(window);
    out.extend(fixed_closure_rules(window));
    out.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

/// A `rule-<date>` [`DeterministicRule`](EvidenceKind::DeterministicRule) record (weekend / fixed
/// closure / holiday link) with a deterministic midnight-UTC `recorded_at`.
fn rule_record(date: NaiveDate) -> EvidenceRecord {
    record(format!("rule-{date}"), KRX_RULE_SOURCE_ID, date, EvidenceKind::DeterministicRule)
}

fn record(id: String, source_id: &str, date: NaiveDate, kind: EvidenceKind) -> EvidenceRecord {
    EvidenceRecord {
        id,
        source_id: source_id.to_string(),
        date,
        kind,
        valid: true,
        superseded_by: None,
        citation: None,
        recorded_at: midnight_utc(date),
    }
}
