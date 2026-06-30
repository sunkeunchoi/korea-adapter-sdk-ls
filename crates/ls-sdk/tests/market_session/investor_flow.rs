use super::*;


// ---------------------------------------------------------------------------
// Wave 2 — market-flow analytics reads (t1601, t1615, t1640, t1662, t1664).
// gubun-filter screens with documented defaults baked into ::new(). Covers AE1.
// ---------------------------------------------------------------------------

/// Covers AE1. `t1601` bakes documented defaults and deserializes the investor
/// aggregate (single object) with net-buy columns populated.
#[test]
fn t1601_request_and_response_round_trip() {
    let value = serde_json::to_value(T1601Request::new()).expect("serialize t1601");
    assert_eq!(value["t1601InBlock"]["gubun1"], "2", "amount basis");
    assert_eq!(value["t1601InBlock"]["exchgubun"], "K", "KRX");

    let resp: T1601Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1601OutBlock1": { "svolume_08": -1000, "svolume_17": "2000", "svolume_18": 500 }
    }))
    .expect("representative t1601 success must deserialize");
    assert_eq!(resp.outblock1.svolume_08, "-1000", "personal net-buy (number)");
    assert_eq!(resp.outblock1.svolume_17, "2000", "foreign net-buy (string)");
}

/// Covers AE1. `t1615` summary + per-market array round-trip; single-or-array.
#[test]
fn t1615_request_and_response_round_trip() {
    let value = serde_json::to_value(T1615Request::new()).expect("serialize t1615");
    assert_eq!(value["t1615InBlock"]["exchgubun"], "K");

    let resp: T1615Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1615OutBlock": { "sum_volume": 12345, "sum_value": "67890" },
        "t1615OutBlock1": [
            { "hname": "코스피", "sv_08": -100, "sv_17": 200, "sv_18": "-50" },
            { "hname": "코스닥", "sv_08": "10", "sv_17": "-20", "sv_18": 5 }
        ]
    }))
    .expect("representative t1615 success must deserialize");
    assert_eq!(resp.outblock.sum_value, "67890");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].hname, "코스피");

    let single: T1615Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1615OutBlock": {}, "t1615OutBlock1": { "hname": "코스피" }
    }))
    .expect("single market row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
}

/// Covers AE1. `t1640` program summary (single object) round-trips.
#[test]
fn t1640_request_and_response_round_trip() {
    let value = serde_json::to_value(T1640Request::new()).expect("serialize t1640");
    assert_eq!(value["t1640InBlock"]["gubun"], "11", "exchange-all");

    let resp: T1640Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1640OutBlock": { "volume": -500, "value": "12000", "basis": "0.35" }
    }))
    .expect("representative t1640 success must deserialize");
    assert_eq!(resp.outblock.value, "12000", "net-buy amount populated");
    assert_eq!(resp.outblock.volume, "-500");
}

/// Covers AE1. `t1662` by-time array round-trips; single-or-array tolerated.
#[test]
fn t1662_request_and_response_round_trip() {
    let value = serde_json::to_value(T1662Request::new()).expect("serialize t1662");
    assert_eq!(value["t1662InBlock"]["gubun"], "0", "KOSPI");

    let resp: T1662Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1662OutBlock": [
            { "time": "0900", "k200jisu": 350, "tot3": -1000, "volume": 5000 },
            { "time": "0901", "k200jisu": "351", "tot3": "200", "volume": "6000" }
        ]
    }))
    .expect("representative t1662 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].time, "0900");
    assert_eq!(resp.outblock[1].k200jisu, "351", "index from string");

    let empty: T1662Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1662OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty is the pending case");
}

/// Covers AE1. `t1664` cnt serializes as a JSON number; the chart array
/// round-trips.
#[test]
fn t1664_request_serializes_cnt_as_number_and_response_round_trips() {
    let value = serde_json::to_value(T1664Request::new()).expect("serialize t1664");
    assert_eq!(value["t1664InBlock"]["cnt"], 20, "cnt serializes as a JSON number");
    assert!(value["t1664InBlock"]["cnt"].is_number(), "cnt is a number, not a string");
    assert_eq!(value["t1664InBlock"]["mgubun"], "1", "KOSPI");

    let resp: T1664Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1664OutBlock1": [
            { "dt": "20260623", "tjj08": -100, "tjj17": 200, "tjj18": "-50" }
        ]
    }))
    .expect("representative t1664 success must deserialize");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].dt, "20260623");
    assert_eq!(resp.outblock1[0].tjj17, "200", "foreign net-buy");
}

// ---- t1631 프로그램매매종합조회 (open-window domestic program-trade; market_session, two single-or-array out-blocks) ----

/// `t1631` serializes its five caller filters under the renamed `t1631InBlock` key
/// with no leaked fields; non-paginated — no tr_cont tokens.
#[test]
fn t1631_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1631Request::new("0", "0", "20260629", "20260629", "1"))
        .expect("serialize t1631 request");
    assert_eq!(value["t1631InBlock"]["gubun"], "0");
    assert_eq!(value["t1631InBlock"]["dgubun"], "0");
    assert_eq!(value["t1631InBlock"]["sdate"], "20260629");
    assert_eq!(value["t1631InBlock"]["edate"], "20260629");
    assert_eq!(value["t1631InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1631InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key field
/// (`bidvolume`) holds a real, non-default value. Numeric-bearing fields tolerate a
/// JSON number or string via `string_or_number`; both out-blocks are tolerant Vecs.
#[test]
fn t1631_success_body_deserializes_with_nondefault_field() {
    let as_string: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1631OutBlock": {
            "cdhrem": "000012345", "bdhrem": "000067890", "tcdrem": "000011111",
            "tbdrem": "000022222", "cshrem": "000033333", "bshrem": "000044444",
            "tcsrem": "000055555", "tbsrem": "000066666"
        },
        "t1631OutBlock1": {
            "offervolume": "000001234", "offervalue": "000000123456",
            "bidvolume": "000005678", "bidvalue": "000000567890",
            "volume": "000004444", "value": "000000444434"
        },
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1631OutBlock": { "cdhrem": 12345 },
        "t1631OutBlock1": { "bidvolume": 5678, "value": 444434i64 }
    }))
    .expect("number body must deserialize");
    assert_eq!(as_string.outblock.len(), 1, "remainder block → one-element Vec");
    assert_eq!(as_string.outblock[0].cdhrem, "000012345");
    assert_eq!(as_string.outblock1.len(), 1, "totals block → one-element Vec");
    assert_eq!(
        as_string.outblock1[0].bidvolume, "000005678",
        "modeled non-key field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock[0].cdhrem, "12345");
    assert_eq!(as_number.outblock1[0].bidvolume, "5678");
}

/// A single (non-array) out-block object is tolerated via `de_vec_or_single` — this
/// is exactly the spec's single-object shape for both t1631 blocks.
#[test]
fn t1631_single_object_out_blocks_are_tolerated() {
    let resp: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1631OutBlock": { "cdhrem": "000012345" },
        "t1631OutBlock1": { "bidvolume": "000005678" }
    }))
    .expect("single-object out-blocks must deserialize");
    assert_eq!(resp.outblock.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].bidvolume, "000005678");
}

/// An empty result (`00707`, no out-blocks) deserializes cleanly to empty Vecs.
#[test]
fn t1631_empty_result_deserializes_to_empty() {
    let empty: T1631Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1631 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty(), "no remainder block → empty Vec");
    assert!(empty.outblock1.is_empty(), "no totals block → empty Vec");
}

// ---- t1632 프로그램매매추이(시간) (open-window domestic program-trade; cursor + intraday-trend array) ----

/// `t1632` serializes its seven caller filters under the renamed `t1632InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1632_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1632Request::new(
        "0", "1", "0", "0", "20260629", "", "1",
    ))
    .expect("serialize t1632 request");
    assert_eq!(value["t1632InBlock"]["gubun"], "0");
    assert_eq!(value["t1632InBlock"]["gubun1"], "1");
    assert_eq!(value["t1632InBlock"]["date"], "20260629");
    assert_eq!(value["t1632InBlock"]["time"], "");
    assert_eq!(value["t1632InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1632InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`k200jisu`) holds a real, non-default value. Numeric-bearing fields tolerate a
/// JSON number or string; the time-series out-block is a repeated array.
#[test]
fn t1632_success_body_deserializes_with_nondefault_field() {
    let as_string: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1632OutBlock": { "date": "20260629", "time": "153000", "idx": "100", "ex_gubun": "1" },
        "t1632OutBlock1": [
            { "time": "09000000", "k200jisu": "00000350.25", "sign": "2", "change": "00000001.10",
              "tot3": "000012345678", "tot1": "000023456789", "tot2": "000011111111", "cha3": "000001234567" },
            { "time": "09010000", "k200jisu": "00000351.00", "sign": "2", "change": "00000001.85",
              "tot3": "000013345678", "tot1": "000024456789", "tot2": "000011111111", "cha3": "000001334567" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1632OutBlock1": [
            { "k200jisu": 350.25, "change": 1.10, "tot3": 12345678i64 }
        ]
    }))
    .expect("number body (k200jisu as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.date, "20260629", "cursor round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "time-series array round-trips");
    assert_eq!(
        as_string.outblock1[0].k200jisu, "00000350.25",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].k200jisu, "350.25");
    assert_eq!(as_number.outblock1[0].change, "1.1");
}

/// A single (non-array) `t1632OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1632_single_object_out_block_is_tolerated() {
    let resp: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1632OutBlock1": { "time": "09000000", "k200jisu": "00000350.25" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].k200jisu, "00000350.25");
}

/// An empty result (`00707`) deserializes cleanly — default cursor, empty rows.
#[test]
fn t1632_empty_result_deserializes_to_empty() {
    let empty: T1632Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1632 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.date.is_empty(), "no cursor → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1633 프로그램매매추이(일별) (open-window domestic program-trade; cursor + daily-trend array) ----

/// `t1633` serializes its nine caller filters under the renamed `t1633InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1633_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1633Request::new(
        "0", "1", "0", "0", "20260601", "20260629", "0", "20260629", "1",
    ))
    .expect("serialize t1633 request");
    assert_eq!(value["t1633InBlock"]["gubun"], "0");
    assert_eq!(value["t1633InBlock"]["fdate"], "20260601");
    assert_eq!(value["t1633InBlock"]["tdate"], "20260629");
    assert_eq!(value["t1633InBlock"]["gubun4"], "0");
    assert_eq!(value["t1633InBlock"]["date"], "20260629");
    assert_eq!(value["t1633InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1633InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`jisu`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the daily-series out-block is a repeated array.
#[test]
fn t1633_success_body_deserializes_with_nondefault_field() {
    let as_string: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1633OutBlock": { "date": "20260629", "idx": "30" },
        "t1633OutBlock1": [
            { "date": "20260627", "jisu": "00000349.10", "sign": "5", "change": "-0000000.40",
              "tot3": "000012345678", "cha3": "000001234567", "bcha3": "000011111111", "volume": "000099999999" },
            { "date": "20260629", "jisu": "00000350.25", "sign": "2", "change": "00000001.15",
              "tot3": "000013345678", "cha3": "000001334567", "bcha3": "000012011111", "volume": "000088888888" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1633OutBlock1": [
            { "jisu": 350.25, "change": 1.15, "volume": 88888888i64 }
        ]
    }))
    .expect("number body (jisu as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.date, "20260629", "cursor round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "daily-series array round-trips");
    assert_eq!(
        as_string.outblock1[0].jisu, "00000349.10",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].jisu, "350.25");
    assert_eq!(as_number.outblock1[0].volume, "88888888");
}

/// A single (non-array) `t1633OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1633_single_object_out_block_is_tolerated() {
    let resp: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1633OutBlock1": { "date": "20260629", "jisu": "00000350.25" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].jisu, "00000350.25");
}

/// An empty result (`00707`) deserializes cleanly — default cursor, empty rows.
#[test]
fn t1633_empty_result_deserializes_to_empty() {
    let empty: T1633Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1633 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.date.is_empty(), "no cursor → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1702 외국인/기관별 매매추이 (open-window domestic; market_session foreign/institution date array) ----

/// `t1702` serializes its seven caller filters under the renamed `t1702InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1702_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1702Request::new(
        "005930", "20260601", "20260629", "1", "0", "0", "1",
    ))
    .expect("serialize t1702 request");
    assert_eq!(value["t1702InBlock"]["shcode"], "005930");
    assert_eq!(value["t1702InBlock"]["fromdt"], "20260601");
    assert_eq!(value["t1702InBlock"]["todt"], "20260629");
    assert_eq!(value["t1702InBlock"]["volvalgb"], "1");
    assert_eq!(value["t1702InBlock"]["msmdgb"], "0");
    assert_eq!(value["t1702InBlock"]["gubun"], "0");
    assert_eq!(value["t1702InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1702InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`close`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the date out-block is a repeated array.
#[test]
fn t1702_success_body_deserializes_with_nondefault_field() {
    let as_string: T1702Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1702OutBlock1": [
            { "date": "20260627", "close": "00072500", "sign": "2", "change": "00000300",
              "volume": "000012345678", "tjj0008": "000000123456", "tjj0016": "000000654321" },
            { "date": "20260629", "close": "00073000", "sign": "2", "change": "00000500",
              "volume": "000011345678", "tjj0008": "000000133456", "tjj0016": "000000644321" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1702Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1702OutBlock1": [ { "close": 73000, "volume": 11345678i64 } ]
    }))
    .expect("number body must deserialize");
    assert_eq!(as_string.outblock.len(), 2, "date array round-trips");
    assert_eq!(
        as_string.outblock[0].close, "00072500",
        "modeled non-key row field is non-default"
    );
    assert_eq!(as_number.outblock[0].close, "73000");
    assert_eq!(as_number.outblock[0].volume, "11345678");
}

/// A single (non-array) `t1702OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1702_single_object_out_block_is_tolerated() {
    let resp: T1702Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1702OutBlock1": { "date": "20260629", "close": "00073000" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock[0].close, "00073000");
}

/// An empty result (`00707`) deserializes cleanly — empty rows.
#[test]
fn t1702_empty_result_deserializes_to_empty() {
    let empty: T1702Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1702 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty(), "no rows → empty Vec");
}

// ---- t1717 외국인/기관 순매수추이 (open-window domestic; market_session net-buy date array) ----

/// `t1717` serializes its five caller filters under the renamed `t1717InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1717_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1717Request::new("005930", "1", "20260601", "20260629", "1"))
        .expect("serialize t1717 request");
    assert_eq!(value["t1717InBlock"]["shcode"], "005930");
    assert_eq!(value["t1717InBlock"]["gubun"], "1");
    assert_eq!(value["t1717InBlock"]["fromdt"], "20260601");
    assert_eq!(value["t1717InBlock"]["todt"], "20260629");
    assert_eq!(value["t1717InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1717InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`close`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the date out-block is a repeated array.
#[test]
fn t1717_success_body_deserializes_with_nondefault_field() {
    let as_string: T1717Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1717OutBlock": [
            { "date": "20260627", "close": "00072500", "sign": "2", "change": "00000300",
              "volume": "000012345678", "tjj0008_vol": "000000123456", "tjj0016_vol": "000000654321" },
            { "date": "20260629", "close": "00073000", "sign": "2", "change": "00000500",
              "volume": "000011345678", "tjj0008_vol": "000000133456", "tjj0016_vol": "000000644321" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1717Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1717OutBlock": [ { "close": 73000, "volume": 11345678i64 } ]
    }))
    .expect("number body must deserialize");
    assert_eq!(as_string.outblock.len(), 2, "date array round-trips");
    assert_eq!(
        as_string.outblock[0].close, "00072500",
        "modeled non-key row field is non-default"
    );
    assert_eq!(as_number.outblock[0].close, "73000");
    assert_eq!(as_number.outblock[0].volume, "11345678");
}

/// A single (non-array) `t1717OutBlock` object is tolerated via `de_vec_or_single`.
#[test]
fn t1717_single_object_out_block_is_tolerated() {
    let resp: T1717Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1717OutBlock": { "date": "20260629", "close": "00073000" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock[0].close, "00073000");
}

/// An empty result (`00707`) deserializes cleanly — empty rows.
#[test]
fn t1717_empty_result_deserializes_to_empty() {
    let empty: T1717Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1717 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty(), "no rows → empty Vec");
}

// ---- t1665 투자자별 매매추이(업종) (open-window domestic; market_session header + date array) ----

/// `t1665` serializes its seven caller filters under the renamed `t1665InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1665_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1665Request::new(
        "1", "001", "1", "1", "20260601", "20260629", "1",
    ))
    .expect("serialize t1665 request");
    assert_eq!(value["t1665InBlock"]["market"], "1");
    assert_eq!(value["t1665InBlock"]["upcode"], "001");
    assert_eq!(value["t1665InBlock"]["gubun2"], "1");
    assert_eq!(value["t1665InBlock"]["gubun3"], "1");
    assert_eq!(value["t1665InBlock"]["from_date"], "20260601");
    assert_eq!(value["t1665InBlock"]["to_date"], "20260629");
    assert_eq!(value["t1665InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1665InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`jisu`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the date out-block is a repeated array, with a single header.
#[test]
fn t1665_success_body_deserializes_with_nondefault_field() {
    let as_string: T1665Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1665OutBlock": { "mcode": "001", "mname": "KOSPI", "ex_upcode": "0001" },
        "t1665OutBlock1": [
            { "date": "20260627", "sv_08": "000000123456", "sv_17": "-00000654321",
              "sv_18": "000000222222", "jisu": "00002950.10" },
            { "date": "20260629", "sv_08": "000000133456", "sv_17": "-00000644321",
              "sv_18": "000000232222", "jisu": "00002975.45" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1665Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1665OutBlock1": [ { "jisu": 2975.45, "sv_08": 133456i64 } ]
    }))
    .expect("number body (jisu as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.mname, "KOSPI", "header round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "date array round-trips");
    assert_eq!(
        as_string.outblock1[0].jisu, "00002950.10",
        "modeled non-key row field is non-default"
    );
    assert_eq!(as_number.outblock1[0].jisu, "2975.45");
    assert_eq!(as_number.outblock1[0].sv_08, "133456");
}

/// A single (non-array) `t1665OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1665_single_object_out_block_is_tolerated() {
    let resp: T1665Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1665OutBlock1": { "date": "20260629", "jisu": "00002975.45" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].jisu, "00002975.45");
}

/// An empty result (`00707`) deserializes cleanly — default header, empty rows.
#[test]
fn t1665_empty_result_deserializes_to_empty() {
    let empty: T1665Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1665 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.mname.is_empty(), "no header → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1716 외인기관종목별동향 (open-window domestic foreign/institution; market_session, by-issue array) ----

/// `t1716` serializes its nine caller filters under the renamed `t1716InBlock` key
/// with no leaked fields; the numeric `prapp` serializes as a JSON number (not a
/// string) so the gateway does not return `IGW40011`. Non-paginated.
#[test]
fn t1716_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1716Request::new(
        "005930", "0", "20260601", "20260629", "0", "0", "1", "1", "1",
    ))
    .expect("serialize t1716 request");
    assert_eq!(value["t1716InBlock"]["shcode"], "005930");
    assert_eq!(value["t1716InBlock"]["gubun"], "0");
    assert_eq!(value["t1716InBlock"]["fromdt"], "20260601");
    assert_eq!(value["t1716InBlock"]["todt"], "20260629");
    // prapp is a numeric request field — serialized as a JSON number, not a string.
    assert_eq!(value["t1716InBlock"]["prapp"], 0);
    assert!(
        value["t1716InBlock"]["prapp"].is_number(),
        "prapp must serialize as a JSON number (else IGW40011)"
    );
    assert_eq!(value["t1716InBlock"]["exchgubun"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1716InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`close`) holds a real, non-default value. Numeric-bearing out fields tolerate a
/// JSON number or string; the by-issue out-block is a repeated array.
#[test]
fn t1716_success_body_deserializes_with_nondefault_field() {
    let as_string: T1716Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1716OutBlock": [
            { "date": "20260627", "close": "00060500", "sign": "2", "change": "00000500",
              "volume": "012345678", "krx_0008": "001111111", "krx_0018": "002222222",
              "krx_0009": "003333333", "pgmvol": "000444444" },
            { "date": "20260629", "close": "00061000", "sign": "2", "change": "00000500",
              "volume": "011345678", "krx_0008": "001211111", "krx_0018": "002322222",
              "krx_0009": "003433333", "pgmvol": "000544444" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1716Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1716OutBlock": [ { "close": 60500i64, "volume": 12345678i64 } ]
    }))
    .expect("number body (close as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.len(), 2, "by-issue array round-trips");
    assert_eq!(
        as_string.outblock[0].close, "00060500",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock[0].close, "60500");
    assert_eq!(as_number.outblock[0].volume, "12345678");
}

/// A single (non-array) `t1716OutBlock` object is tolerated via `de_vec_or_single`.
#[test]
fn t1716_single_object_out_block_is_tolerated() {
    let resp: T1716Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1716OutBlock": { "date": "20260629", "close": "00061000" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock[0].close, "00061000");
}

/// An empty result (`00707`) deserializes cleanly to an empty Vec.
#[test]
fn t1716_empty_result_deserializes_to_empty() {
    let empty: T1716Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1716 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty(), "no rows → empty Vec");
}

// ---- t1927 공매도일별추이 (open-window domestic short-selling; market_session, cursor + daily array) ----

/// `t1927` serializes its four caller filters under the renamed `t1927InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1927_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1927Request::new("005930", "", "20260601", "20260629"))
        .expect("serialize t1927 request");
    assert_eq!(value["t1927InBlock"]["shcode"], "005930");
    assert_eq!(value["t1927InBlock"]["date"], "");
    assert_eq!(value["t1927InBlock"]["sdate"], "20260601");
    assert_eq!(value["t1927InBlock"]["edate"], "20260629");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1927InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`price`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the daily out-block is a repeated array.
#[test]
fn t1927_success_body_deserializes_with_nondefault_field() {
    let as_string: T1927Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1927OutBlock": { "date": "20260629" },
        "t1927OutBlock1": [
            { "date": "20260627", "price": "00060500", "sign": "2", "change": "00000500",
              "volume": "012345678", "value": "000099999999", "gm_vo": "000111111", "gm_va": "000222222" },
            { "date": "20260629", "price": "00061000", "sign": "2", "change": "00000500",
              "volume": "011345678", "value": "000088888888", "gm_vo": "000121111", "gm_va": "000232222" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1927Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1927OutBlock1": [ { "price": 60500i64, "volume": 12345678i64 } ]
    }))
    .expect("number body (price as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.date, "20260629", "cursor round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "daily array round-trips");
    assert_eq!(
        as_string.outblock1[0].price, "00060500",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].price, "60500");
    assert_eq!(as_number.outblock1[0].volume, "12345678");
}

/// A single (non-array) `t1927OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1927_single_object_out_block_is_tolerated() {
    let resp: T1927Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1927OutBlock1": { "date": "20260629", "price": "00061000" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "00061000");
}

/// An empty result (`00707`) deserializes cleanly — default cursor, empty rows.
#[test]
fn t1927_empty_result_deserializes_to_empty() {
    let empty: T1927Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1927 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.date.is_empty(), "no cursor → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1941 종목별대차거래일간추이 (open-window domestic stock-loan; market_session, response_body → daily array) ----

/// `t1941` serializes its three caller filters under the renamed `t1941InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1941_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1941Request::new("005930", "20260601", "20260629"))
        .expect("serialize t1941 request");
    assert_eq!(value["t1941InBlock"]["shcode"], "005930");
    assert_eq!(value["t1941InBlock"]["sdate"], "20260601");
    assert_eq!(value["t1941InBlock"]["edate"], "20260629");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1941InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`price`) holds a real, non-default value. The out-rows arrive under the
/// `t1941OutBlock1` key (the `response_body` wrapper is transparent). Numeric-bearing
/// fields tolerate a JSON number or string; the daily out-block is a repeated array.
#[test]
fn t1941_success_body_deserializes_with_nondefault_field() {
    let as_string: T1941Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1941OutBlock1": [
            { "date": "20260627", "price": "00060500", "sign": "2", "change": "00000500",
              "volume": "012345678", "upvolume": "000111111", "dnvolume": "000022222",
              "tovolume": "000333333", "tovalue": "000000444444" },
            { "date": "20260629", "price": "00061000", "sign": "2", "change": "00000500",
              "volume": "011345678", "upvolume": "000121111", "dnvolume": "000032222",
              "tovolume": "000343333", "tovalue": "000000544444" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1941Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1941OutBlock1": [ { "price": 60500i64, "tovolume": 333333i64 } ]
    }))
    .expect("number body (price as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock1.len(), 2, "daily array round-trips");
    assert_eq!(
        as_string.outblock1[0].price, "00060500",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].price, "60500");
    assert_eq!(as_number.outblock1[0].tovolume, "333333");
}

/// A single (non-array) `t1941OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1941_single_object_out_block_is_tolerated() {
    let resp: T1941Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1941OutBlock1": { "date": "20260629", "price": "00061000" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "00061000");
}

/// An empty result (`00707`) deserializes cleanly to an empty Vec.
#[test]
fn t1941_empty_result_deserializes_to_empty() {
    let empty: T1941Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1941 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

/// `t8463` request: `cnt` serializes as a JSON NUMBER (KTD4 — `string_as_number`,
/// avoids IGW40011); the single header + ARRAY time-series block round-trip and a
/// single row collapses to a one-element Vec (KTD5). Canonical 일자 (`date`)
/// pinned exactly (KTD6).
#[test]
fn t8463_request_serializes_cnt_as_number_and_header_plus_array_round_trips() {
    let value = serde_json::to_value(T8463Request::new("N", "F", "101")).expect("serialize t8463");
    assert_eq!(value["t8463InBlock"]["tm_rng"], "N");
    assert_eq!(value["t8463InBlock"]["fot_clsf_cd"], "F");
    assert_eq!(value["t8463InBlock"]["bsc_asts_id"], "101");
    assert!(
        value["t8463InBlock"]["cnt"].is_number(),
        "cnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["t8463InBlock"]["cnt"], 20);
    // bgubun stays a genuine string.
    assert!(value["t8463InBlock"]["bgubun"].is_string());

    let resp: T8463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8463OutBlock": { "tm_rng": "N", "indcode": "1000", "forcode": "2000" },
        "t8463OutBlock1": [
            { "date": "20260624", "time": "190000", "indmsvol": 1234, "formsvol": "5678" },
            { "date": "20260624", "time": "191000", "indmsvol": "4321", "formsvol": 8765 }
        ]
    }))
    .expect("representative t8463 success must deserialize");
    assert_eq!(resp.outblock.tm_rng, "N", "시간대");
    assert_eq!(resp.outblock.indcode, "1000", "개인투자자코드");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].date, "20260624", "일자");
    assert_eq!(resp.outblock1[1].indmsvol, "4321", "개인순매수거래량 from string preserved");

    // single row object → one-element Vec (KTD5).
    let single: T8463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8463OutBlock1": { "date": "20260624", "time": "190000", "indmsvol": 1234 }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].date, "20260624");
}

/// `t8463` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t8463_numeric_field_string_or_number() {
    let from_num: T8463OutBlock =
        serde_json::from_value(serde_json::json!({ "indcode": 1000 }))
            .expect("number form deserializes");
    let from_str: T8463OutBlock =
        serde_json::from_value(serde_json::json!({ "indcode": "1000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.indcode, "1000");
    assert_eq!(from_str.indcode, "1000");
}

/// `t8463` off-window empty (`00707`, empty array) deserializes — the night
/// window is closed (KTD7), so this is a RE-RUN-IN-WINDOW disposition (NOT a
/// flip, NOT a DROP).
#[test]
fn t8463_off_window_empty_is_rerun_disposition_not_flip_not_drop() {
    let empty: T8463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8463OutBlock1": []
    }))
    .expect("off-window empty still deserializes (the night window is closed)");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty time-series array is the re-run case");
}

// --- t8462 — KRX야간파생 투자자기간별 (investor-period table) -----------------

#[test]
fn t8462_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8462Request::new("K2I", "20260601", "20260626"))
        .expect("serialize t8462");
    let ib = &value["t8462InBlock"];
    assert_eq!(ib["bsc_asts_id"], "K2I");
    assert_eq!(ib["from_date"], "20260601");
    assert_eq!(ib["tm_rng"], "N", "night time-range default");
}

#[tokio::test]
async fn t8462_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/futureoption/investor"))
        .and(header("tr_cd", "t8462"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"t8462OutBlock":{"tm_rng":"N","fot_clsf_cd":"F","bsc_asts_id":"K2I"},"t8462OutBlock1":[{"date":"20250610","sv_08":-299,"sv_17":335,"sv_18":-69,"sv_01":-69,"sa_08":"-287","sa_17":"321","sa_18":"-66","sa_01":"-66"}],"rsp_cd":"00000","rsp_msg":"정상적으로 조회가 완료되었습니다."}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .market_session()
        .night_derivatives_investor_period(&T8462Request::new("K2I", "20250609", "20250610"))
        .await
        .expect("t8462 should succeed");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].sv_01, "-69", "개인 순매수수량 round-trips (numeric→string)");
    assert_eq!(resp.outblock.bsc_asts_id, "K2I");
}

#[test]
fn t8462_single_or_array_and_empty_deserialize() {
    let single: T8462Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8462OutBlock": { "bsc_asts_id": "K2I" },
        "t8462OutBlock1": { "date": "20250610", "sv_01": -69 }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].sv_01, "-69");

    let empty: T8462Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8462OutBlock": {}, "t8462OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

/// t1926 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1926_request_and_response_round_trip() {
    let v = serde_json::to_value(T1926Request::new("005930")).expect("serialize t1926");
    let _ = &v;
    let resp: T1926Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1926OutBlock": {"mmdate": "X1", "close": 41945}}"#).expect("t1926 body round-trips");
    assert_eq!(resp.outblock.mmdate, "X1");
    assert_eq!(resp.outblock.close, "41945", "close from JSON number via string_or_number");
    let empty: T1926Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1926OutBlock":{}}"#).expect("empty deserializes");
    assert!(empty.outblock.mmdate.is_empty());
}
