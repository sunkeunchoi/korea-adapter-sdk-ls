use super::*;


// ---- t1901 ETF현재가 (plan -002 Track 2; market_session single-object read) ----

/// `t1901` serializes to `{"t1901InBlock":{"shcode":"069500"}}` (shcode-only,
/// non-paginated — no tr_cont tokens in the body).
#[test]
fn t1901_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1901Request::new("069500")).expect("serialize t1901 request");
    assert_eq!(value["t1901InBlock"]["shcode"], "069500", "shcode stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// The numeric `price` field tolerates a JSON number or string via
/// `string_or_number` (the gateway sends ETF prices as integers).
#[test]
fn t1901_price_number_or_string_yields_same_value() {
    let as_number: T1901Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1901OutBlock": { "hname": "KODEX 200", "price": 135155 }
    }))
    .expect("number price must deserialize");
    let as_string: T1901Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1901OutBlock": { "hname": "KODEX 200", "price": "135155" }
    }))
    .expect("string price must deserialize");
    assert_eq!(as_number.outblock.price, "135155");
    assert_eq!(as_number.outblock.price, as_string.outblock.price);
}

/// An empty/sparse result (e.g. a `00707` no-data envelope with no out-block)
/// deserializes cleanly to defaults — no panic on a missing `t1901OutBlock`.
#[test]
fn t1901_empty_result_deserializes_to_defaults() {
    let empty: T1901Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1901 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no out-block → default hname");
    assert!(empty.outblock.price.is_empty(), "no out-block → default price");
}

// ---- t1906 ETFLP호가 (closed-window more-flips wave; market_session single-object) ----

/// `t1906` serializes to `{"t1906InBlock":{"shcode":"152100"}}` (shcode-only,
/// non-paginated — no tr_cont tokens in the body).
#[test]
fn t1906_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1906Request::new("152100")).expect("serialize t1906 request");
    assert_eq!(value["t1906InBlock"]["shcode"], "152100", "shcode stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. The numeric `price` tolerates a
/// JSON number or string via `string_or_number`.
#[test]
fn t1906_success_body_deserializes_with_nondefault_field() {
    let as_number: T1906Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1906OutBlock": {
            "hname": "TIGER 코스닥150",
            "price": 3685,
            "offerho1": 3690,
            "bidho1": 3685,
            "lp_offerrem1": 0,
            "shcode": "152100"
        }
    }))
    .expect("number price must deserialize");
    let as_string: T1906Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1906OutBlock": {
            "hname": "TIGER 코스닥150",
            "price": "3685",
            "offerho1": "3690",
            "bidho1": "3685",
            "lp_offerrem1": "0",
            "shcode": "152100"
        }
    }))
    .expect("string price must deserialize");
    assert_eq!(as_number.outblock.price, "3685", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock.offerho1, "3690");
    assert_eq!(as_number.outblock.shcode, "152100");
    assert_eq!(as_number.outblock.price, as_string.outblock.price);
    assert_eq!(as_number.outblock.offerho1, as_string.outblock.offerho1);
}

/// An empty result (`00707`, no out-block) deserializes cleanly to defaults — no
/// panic on a missing `t1906OutBlock`; recognized as the empty/pending case.
#[test]
fn t1906_empty_result_deserializes_to_defaults() {
    let empty: T1906Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1906 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no out-block → default hname");
    assert!(empty.outblock.price.is_empty(), "no out-block → default price");
}

// ---- t1902 ETF시간별추이 (open-window domestic ETF; market_session, header + intraday array) ----

/// `t1902` serializes its two caller filters under the renamed `t1902InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1902_new_serializes_filters_under_inblock_no_leak() {
    let value =
        serde_json::to_value(T1902Request::new("069500", "")).expect("serialize t1902 request");
    assert_eq!(value["t1902InBlock"]["shcode"], "069500");
    assert_eq!(value["t1902InBlock"]["time"], "");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1902InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`nav`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the intraday out-block is a repeated array.
#[test]
fn t1902_success_body_deserializes_with_nondefault_field() {
    let as_string: T1902Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1902OutBlock": { "time": "153000", "hname": "KODEX 200", "upname": "KOSPI200" },
        "t1902OutBlock1": [
            { "time": "09000000", "price": "00035120", "sign": "2", "change": "00000110",
              "volume": "001234567", "nav": "00035100", "jisu": "00000350.25" },
            { "time": "09010000", "price": "00035200", "sign": "2", "change": "00000190",
              "volume": "001334567", "nav": "00035180", "jisu": "00000351.00" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1902Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1902OutBlock1": [ { "nav": 35100i64, "price": 35120i64 } ]
    }))
    .expect("number body (nav as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock.hname, "KODEX 200", "header round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "intraday array round-trips");
    assert_eq!(
        as_string.outblock1[0].nav, "00035100",
        "modeled non-key row field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].nav, "35100");
    assert_eq!(as_number.outblock1[0].price, "35120");
}

/// A single (non-array) `t1902OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1902_single_object_out_block_is_tolerated() {
    let resp: T1902Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1902OutBlock1": { "time": "09000000", "nav": "00035100" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].nav, "00035100");
}

/// An empty result (`00707`) deserializes cleanly — default header, empty rows.
#[test]
fn t1902_empty_result_deserializes_to_empty() {
    let empty: T1902Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1902 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no header → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1904 ETF구성종목조회 (open-window domestic ETF; market_session, header + constituent array) ----

/// `t1904` serializes its three caller filters under the renamed `t1904InBlock` key
/// with no leaked fields; non-paginated.
#[test]
fn t1904_new_serializes_filters_under_inblock_no_leak() {
    let value = serde_json::to_value(T1904Request::new("069500", "20260629", "1"))
        .expect("serialize t1904 request");
    assert_eq!(value["t1904InBlock"]["shcode"], "069500");
    assert_eq!(value["t1904InBlock"]["date"], "20260629");
    assert_eq!(value["t1904InBlock"]["sgb"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1904InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key constituent
/// field (`hname`) + header field (`price`) hold real, non-default values.
/// Numeric-bearing fields tolerate a JSON number or string; the constituent
/// out-block is a repeated array.
#[test]
fn t1904_success_body_deserializes_with_nondefault_field() {
    let as_string: T1904Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1904OutBlock": {
            "date": "20260629", "price": "00035120", "volume": "001234567", "nav": "00035100",
            "upname": "KOSPI200", "etftotcap": "000000123456", "etfnum": "0200",
            "opcom_nmk": "삼성자산운용"
        },
        "t1904OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "price": "00061000", "sign": "2",
              "change": "00000500", "volume": "012345678", "pvalue": "000000999999", "weight": "0025.50" },
            { "shcode": "000660", "hname": "SK하이닉스", "price": "00185000", "sign": "5",
              "change": "-0001000", "volume": "002345678", "pvalue": "000000555555", "weight": "0012.30" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1904Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1904OutBlock": { "price": 35120i64 },
        "t1904OutBlock1": [ { "price": 61000i64 } ]
    }))
    .expect("number body must deserialize");
    assert_eq!(as_string.outblock.price, "00035120", "header round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "constituent array round-trips");
    assert_eq!(
        as_string.outblock1[0].hname, "삼성전자",
        "modeled non-key constituent field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.price, "35120");
    assert_eq!(as_number.outblock1[0].price, "61000");
}

/// A single (non-array) `t1904OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1904_single_object_out_block_is_tolerated() {
    let resp: T1904Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1904OutBlock1": { "shcode": "005930", "hname": "삼성전자" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].hname, "삼성전자");
}

/// An empty result (`00707`) deserializes cleanly — default header, empty rows.
#[test]
fn t1904_empty_result_deserializes_to_empty() {
    let empty: T1904Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1904 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.price.is_empty(), "no header → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

// ---- t1959 LP대상종목정보조회 (closed-window more-flips; market_session ELW, per-issue array) ----

/// `t1959` serializes to `{"t1959InBlock":{"shcode":""}}` — `::new()` defaults
/// `shcode` to the empty string (the full LP-target list). No numeric request slot,
/// so no number coercion; non-paginated — no tr_cont tokens.
#[test]
fn t1959_new_serializes_empty_shcode_under_inblock() {
    let value = serde_json::to_value(T1959Request::new()).expect("serialize t1959 request");
    assert_eq!(
        value["t1959InBlock"]["shcode"], "",
        "::new() defaults shcode to the empty string (full LP-target list)"
    );
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    // for_shcode narrows to one issue, still under the renamed in-block key.
    let one = serde_json::to_value(T1959Request::for_shcode("000250")).expect("serialize");
    assert_eq!(one["t1959InBlock"]["shcode"], "000250");
    assert!(one["t1959InBlock"].get("nrec").is_none(), "no leaked caller fields");
}

/// A representative success body (the spec res_example: 삼천당제약 000250 / 이녹스
/// 088390) deserializes AND a modeled non-key field (`price`) holds a real,
/// non-default value — proving the subset round-trips, not just `serde(default)`
/// returning `Ok`. Numeric-bearing fields (incl. the spec-`Number` `rate`) tolerate
/// a JSON number or string via `string_or_number`; the per-issue out-block is a
/// repeated array.
#[test]
fn t1959_success_body_deserializes_with_nondefault_field() {
    let as_string: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1959OutBlock1": [
            { "shcode": "000250", "hname": "삼천당제약", "price": "000000061900", "sign": "5", "change": "-00000000200", "rate": "-0.32", "volume": "000000097361", "value": "006010435800", "lp_gb": "가능" },
            { "shcode": "088390", "hname": "이녹스", "price": "000000035950", "sign": "5", "change": "-00000000150", "rate": "-0.42", "volume": "000000019443", "value": "000704468650", "lp_gb": "가능" }
        ],
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1959OutBlock1": [
            { "shcode": "000250", "hname": "삼천당제약", "price": 61900, "sign": "5", "change": -200, "rate": -0.32, "volume": 97361, "value": 6010435800i64, "lp_gb": "가능" }
        ]
    }))
    .expect("number body (rate as a JSON Number) must deserialize");
    assert_eq!(as_string.outblock1.len(), 2, "per-issue array round-trips");
    assert_eq!(as_string.outblock1[0].shcode, "000250", "modeled key round-trips");
    assert_eq!(
        as_string.outblock1[0].hname, "삼천당제약",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock1[0].lp_gb, "가능");
    // `rate` is a spec Number — tolerated from BOTH a JSON number and a string.
    assert_eq!(as_number.outblock1[0].rate, "-0.32");
    assert_eq!(as_number.outblock1[0].shcode, "000250");
}

/// A single (non-array) `t1959OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1959_single_object_out_block_is_tolerated() {
    let resp: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1959OutBlock1": { "shcode": "000250", "hname": "삼천당제약", "price": "000000061900" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].shcode, "000250");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1959OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1959_empty_result_deserializes_to_defaults() {
    let empty: T1959Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1959 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
}

/// t1903 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1903_request_and_response_round_trip() {
    let v = serde_json::to_value(T1903Request::new("448330")).expect("serialize t1903");
    let _ = &v;
    let resp: T1903Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1903OutBlock": {"date": "x", "upname": "x", "hname": "x"}, "t1903OutBlock1": [{"date": "X1", "price": 41945}]}"#).expect("t1903 body round-trips");
    assert_eq!(resp.outblock1[0].date, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number via string_or_number");
    let empty: T1903Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1903OutBlock":{},"t1903OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}
