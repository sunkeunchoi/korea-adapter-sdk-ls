//! U3: native KRX/KASI response normalization + generated-rule evidence. All synthetic,
//! offline, fixed-clock. No real endpoint is ever hit — every parser is fed a hand-crafted
//! synthetic body (R14); no captured KRX/KASI rows live in the repo.

use chrono::{NaiveDate, TimeZone, Utc};
use nautilus_ls::calendar_refresh::{
    fixed_closure_rules, generated_rules, holiday_evidence, midnight_utc, parse_kasi_holidays_xml,
    parse_krx_daily, weekend_rules, witness_evidence, DateRange,
};
use nautilus_ls_calendar::schema::EvidenceKind;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

// ---------------------------------------------------------------------------------------
// KRX daily-market native envelope → KTD7 witness matrix
// ---------------------------------------------------------------------------------------

#[test]
fn krx_valid_kospi_response_becomes_a_witness() {
    let body = r#"{"OutBlock_1":[{"BAS_DD":"20260619","MKT_NM":"KOSPI"},{"BAS_DD":"20260619","MKT_NM":"KOSDAQ"}]}"#;
    let resp = parse_krx_daily(body, d(2026, 6, 19)).expect("valid envelope parses");
    let witness = witness_evidence(&resp).expect("a qualifying KOSPI row yields a witness");
    assert_eq!(witness.id, "krx-witness-2026-06-19");
    assert_eq!(witness.source_id, "krx-daily");
    assert_eq!(witness.kind, EvidenceKind::PositiveWitness);
    assert_eq!(witness.date, d(2026, 6, 19));
    // KTD8: deterministic midnight-UTC recorded_at.
    assert_eq!(witness.recorded_at, midnight_utc(d(2026, 6, 19)));
}

#[test]
fn krx_non_qualifying_responses_yield_no_witness() {
    // Empty OutBlock_1 (a closed/absent day).
    let empty = parse_krx_daily(r#"{"OutBlock_1":[]}"#, d(2026, 6, 20)).unwrap();
    assert!(witness_evidence(&empty).is_none(), "empty response is non-evidence");

    // Structurally malformed row (blank market label).
    let malformed = parse_krx_daily(r#"{"OutBlock_1":[{"BAS_DD":"20260619","MKT_NM":""}]}"#, d(2026, 6, 19)).unwrap();
    assert!(witness_evidence(&malformed).is_none(), "blank-market row is malformed");

    // A blank BAS_DD is also treated as malformed (never silently dropped).
    let blank_date = parse_krx_daily(r#"{"OutBlock_1":[{"BAS_DD":"","MKT_NM":"KOSPI"}]}"#, d(2026, 6, 19)).unwrap();
    assert!(witness_evidence(&blank_date).is_none(), "blank BAS_DD is malformed");

    // Date-mismatched rows.
    let mismatch = parse_krx_daily(r#"{"OutBlock_1":[{"BAS_DD":"20260618","MKT_NM":"KOSPI"}]}"#, d(2026, 6, 19)).unwrap();
    assert!(witness_evidence(&mismatch).is_none(), "date-mismatched response is non-evidence");

    // Error envelope.
    let errored = parse_krx_daily(r#"{"errCode":"E0001","OutBlock_1":[]}"#, d(2026, 6, 19)).unwrap();
    assert!(witness_evidence(&errored).is_none(), "error-enveloped response is non-evidence");

    // Unparseable JSON is a normalization Err (→ a Failed source, never a witness).
    assert!(parse_krx_daily("<<not json>>", d(2026, 6, 19)).is_err());
}

// ---------------------------------------------------------------------------------------
// KASI getRestDeInfo native XML → holiday facts + paired rules
// ---------------------------------------------------------------------------------------

const KASI_TWO_HOLIDAYS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<response>
  <header><resultCode>00</resultCode><resultMsg>NORMAL SERVICE.</resultMsg></header>
  <body>
    <items>
      <item><dateName>어린이날</dateName><isHoliday>Y</isHoliday><locdate>20260505</locdate><seq>1</seq></item>
      <item><dateName>부처님오신날</dateName><isHoliday>Y</isHoliday><locdate>20260524</locdate><seq>1</seq></item>
      <item><dateName>임시공휴일후보</dateName><isHoliday>N</isHoliday><locdate>20260601</locdate><seq>1</seq></item>
    </items>
    <numOfRows>10</numOfRows><pageNo>1</pageNo><totalCount>2</totalCount>
  </body>
</response>"#;

#[test]
fn kasi_page_parses_holidays_and_pagination_and_excludes_non_holidays() {
    let page = parse_kasi_holidays_xml(KASI_TWO_HOLIDAYS).expect("valid KASI XML parses");
    assert_eq!(page.holidays, vec![d(2026, 5, 5), d(2026, 5, 24)], "only isHoliday=Y items");
    assert_eq!(page.total_count, 2);
    assert_eq!(page.num_of_rows, 10);
    assert_eq!(page.page_no, 1);
}

#[test]
fn kasi_holiday_yields_one_fact_and_one_paired_rule_with_deterministic_timestamps() {
    let (fact, rule) = holiday_evidence(d(2026, 5, 5));
    assert_eq!(fact.id, "kasi-2026-05-05");
    assert_eq!(fact.kind, EvidenceKind::HolidayFact);
    assert_eq!(fact.source_id, "kasi");
    assert_eq!(rule.id, "rule-2026-05-05");
    assert_eq!(rule.kind, EvidenceKind::DeterministicRule);
    assert_eq!(rule.source_id, "krx-rule");
    // KTD8: both stamped midnight-UTC of the date.
    assert_eq!(fact.recorded_at, midnight_utc(d(2026, 5, 5)));
    assert_eq!(rule.recorded_at, midnight_utc(d(2026, 5, 5)));
}

#[test]
fn kasi_single_item_and_empty_year_edge_cases() {
    let single = r#"<response><body><items>
      <item><dateName>신정</dateName><isHoliday>Y</isHoliday><locdate>20260101</locdate></item>
    </items><totalCount>1</totalCount></body></response>"#;
    let page = parse_kasi_holidays_xml(single).unwrap();
    assert_eq!(page.holidays, vec![d(2026, 1, 1)], "single <item> (not an array) parses");
    assert_eq!(page.total_count, 1);

    let empty = r#"<response><body><items></items><totalCount>0</totalCount></body></response>"#;
    let page = parse_kasi_holidays_xml(empty).unwrap();
    assert!(page.holidays.is_empty(), "an empty year has no holidays");
    assert_eq!(page.total_count, 0);
}

#[test]
fn kasi_hostile_doctype_does_not_expand() {
    // A billion-laughs-style DTD must not expand (KTD9 non-DTD-expanding parser). The parser
    // either ignores the DOCTYPE or errors — it must never resolve `&lol;` into content.
    let hostile = r#"<?xml version="1.0"?>
<!DOCTYPE response [ <!ENTITY lol "LOL"> <!ENTITY lol2 "&lol;&lol;&lol;"> ]>
<response><body><items>
  <item><isHoliday>Y</isHoliday><locdate>20260101</locdate></item>
</items></body></response>"#;
    // Parsing must not panic or hang; the single holiday still resolves, no entity expansion.
    if let Ok(page) = parse_kasi_holidays_xml(hostile) {
        assert_eq!(page.holidays, vec![d(2026, 1, 1)]);
    }
}

// ---------------------------------------------------------------------------------------
// Generated deterministic-rule evidence (weekends + fixed closures) — KTD3
// ---------------------------------------------------------------------------------------

#[test]
fn weekend_generator_covers_exactly_the_weekends_across_a_month_boundary() {
    // 2026-05-30 (Sat) .. 2026-06-01 (Mon): weekend rules for 05-30 and 05-31 only.
    let rules = weekend_rules(DateRange::new(d(2026, 5, 30), d(2026, 6, 1)));
    let dates: Vec<NaiveDate> = rules.iter().map(|r| r.date).collect();
    assert_eq!(dates, vec![d(2026, 5, 30), d(2026, 5, 31)]);
    for r in &rules {
        assert_eq!(r.kind, EvidenceKind::DeterministicRule);
        assert_eq!(r.id, format!("rule-{}", r.date));
        assert_eq!(r.recorded_at, midnight_utc(r.date), "KTD8 deterministic timestamp");
    }
}

#[test]
fn fixed_closure_generation_covers_weekday_year_end_and_labor_day_only() {
    // 2026: May 1 is a Friday, Dec 31 is a Thursday — both weekdays → both generated.
    let rules = fixed_closure_rules(DateRange::new(d(2026, 1, 1), d(2026, 12, 31)));
    let dates: Vec<NaiveDate> = rules.iter().map(|r| r.date).collect();
    assert!(dates.contains(&d(2026, 5, 1)), "weekday Labor Day is a fixed closure");
    assert!(dates.contains(&d(2026, 12, 31)), "weekday year-end is a fixed closure");

    // 2027-05-01 is a Saturday → NO fixed closure (the weekend rule already covers it).
    let weekend_may1 = fixed_closure_rules(DateRange::new(d(2027, 5, 1), d(2027, 5, 1)));
    assert!(weekend_may1.is_empty(), "a weekend Labor Day yields no fixed rule");

    // Labor Day never shifts: the window around a weekend May 1 yields no substitute weekday.
    let around_may1 = fixed_closure_rules(DateRange::new(d(2027, 4, 28), d(2027, 5, 4)));
    assert!(around_may1.is_empty(), "a weekend Labor Day has NO substitute closure day");

    // Over that weekend window the only closure records come from the weekend generator.
    let combined = generated_rules(DateRange::new(d(2027, 5, 1), d(2027, 5, 2)));
    let dates: Vec<NaiveDate> = combined.iter().map(|r| r.date).collect();
    assert_eq!(dates, vec![d(2027, 5, 1), d(2027, 5, 2)], "weekend Sat+Sun only, no fixed May 1");
}

#[test]
fn year_end_closure_shifts_to_the_preceding_weekday_when_dec_31_is_a_weekend() {
    // The published KRX rule closes the preceding trading day when Dec 31 falls on a
    // weekend. These four dates are exactly the historical Unknown days the weekday-only
    // transcription left unresolved (P1, plan 2026-08-10-001).
    for (year, expected) in [
        (2016, d(2016, 12, 30)), // Dec 31 Sat → Fri 12-30
        (2017, d(2017, 12, 29)), // Dec 31 Sun → (Sat 12-30) → Fri 12-29
        (2022, d(2022, 12, 30)), // Dec 31 Sat → Fri 12-30
        (2023, d(2023, 12, 29)), // Dec 31 Sun → Fri 12-29
    ] {
        let window = DateRange::new(d(year, 12, 1), d(year, 12, 31));
        let rules = fixed_closure_rules(window);
        let dates: Vec<NaiveDate> = rules.iter().map(|r| r.date).collect();
        assert_eq!(dates, vec![expected], "year {year} year-end closure day");
        // The weekend Dec 31 itself is the weekend generator's, not a fixed rule.
        assert!(!dates.contains(&d(year, 12, 31)));
    }

    // A weekday Dec 31 stays put — no shift, single rule on the day itself.
    let rules = fixed_closure_rules(DateRange::new(d(2026, 12, 1), d(2026, 12, 31)));
    let dates: Vec<NaiveDate> = rules.iter().map(|r| r.date).collect();
    assert_eq!(dates, vec![d(2026, 12, 31)], "weekday year-end is unshifted");

    // A 1-day window on the shifted day alone still emits it (the refresh fetch uses
    // exactly this window shape to resolve a single historical day).
    let one_day = fixed_closure_rules(DateRange::new(d(2016, 12, 30), d(2016, 12, 30)));
    assert_eq!(one_day.iter().map(|r| r.date).collect::<Vec<_>>(), vec![d(2016, 12, 30)]);
}

#[test]
fn generated_rules_are_byte_identical_across_runs() {
    // KTD8: the same window produces byte-identical records (ids + deterministic timestamps),
    // so a resumed/re-run fetch never perturbs the artifact id.
    let window = DateRange::new(d(2026, 4, 25), d(2026, 5, 10));
    assert_eq!(generated_rules(window), generated_rules(window));
    // And the timestamps are truly midnight, not wall-clock now().
    for r in generated_rules(window) {
        assert_eq!(r.recorded_at, Utc.from_utc_datetime(&r.date.and_hms_opt(0, 0, 0).unwrap()));
    }
}
