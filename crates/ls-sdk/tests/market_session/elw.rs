use super::*;


/// Covers AE1. `t9907` expiry-month list round-trips; empty is the pending case.
#[test]
fn t9907_request_and_response_round_trip() {
    let value = serde_json::to_value(T9907Request::new()).expect("serialize t9907");
    assert_eq!(value["t9907InBlock"]["dummy"], "");

    let resp: T9907Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9907OutBlock1": [
            { "lastym": "202606", "lastnm": "2026년 06월" },
            { "lastym": 202609, "lastnm": "2026년 09월" }
        ]
    }))
    .expect("representative t9907 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].lastym, "202606");
    assert_eq!(resp.outblock1[1].lastym, "202609", "lastym from JSON number");

    let empty: T9907Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9907OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty());
}

/// Covers AE1. `t8431` ELW-symbol list round-trips with the `shcode` (the `t1958`
/// pair source) populated; the numeric `recprice` parses number-or-string;
/// single-or-array tolerated; empty is the pending case.
#[test]
fn t8431_request_and_response_round_trip() {
    let value = serde_json::to_value(T8431Request::new()).expect("serialize t8431");
    assert_eq!(value["t8431InBlock"]["dummy"], "");

    let resp: T8431Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8431OutBlock": [
            { "hname": "삼성전자콜ELW", "shcode": "57J123", "expcode": "KR4500001234",
              "recprice": 105 },
            { "hname": "SK하이닉스풋ELW", "shcode": "57J456", "expcode": "KR4500005678",
              "recprice": "210" }
        ]
    }))
    .expect("representative t8431 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].shcode, "57J123", "ELW code populated");
    assert_eq!(resp.outblock[0].recprice, "105", "recprice from JSON number");
    assert_eq!(resp.outblock[1].recprice, "210", "recprice from JSON string");

    let single: T8431Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8431OutBlock": { "shcode": "57J123", "hname": "삼성전자콜ELW" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock.len(), 1);

    let empty: T8431Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8431OutBlock": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock.is_empty(), "empty is the pending case");
}

/// Covers AE1. `T8431OutBlock.recprice` parses number-or-string alike.
#[test]
fn t8431_recprice_number_or_string_yields_same_value() {
    let n: T8431OutBlock =
        serde_json::from_value(serde_json::json!({ "recprice": 105 })).expect("number");
    let s: T8431OutBlock =
        serde_json::from_value(serde_json::json!({ "recprice": "105" })).expect("string");
    assert_eq!(n.recprice, "105");
    assert_eq!(n.recprice, s.recprice);
}

/// Covers AE1. `t9942` ELW master list round-trips; single-or-array tolerated;
/// empty is the pending case.
#[test]
fn t9942_request_and_response_round_trip() {
    let value = serde_json::to_value(T9942Request::new()).expect("serialize t9942");
    assert_eq!(value["t9942InBlock"]["dummy"], "");

    let resp: T9942Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9942OutBlock": [
            { "hname": "삼성전자콜ELW", "shcode": "57J123", "expcode": "KR4500001234" },
            { "hname": "SK하이닉스풋ELW", "shcode": "57J456", "expcode": "KR4500005678" }
        ]
    }))
    .expect("representative t9942 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].shcode, "57J123");

    let single: T9942Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9942OutBlock": { "shcode": "57J123", "hname": "삼성전자콜ELW" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock.len(), 1);

    let empty: T9942Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9942OutBlock": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t1958 — ELW종목비교 (ELW comparison; Wave 1). Two ELW shcodes self-sourced
// from t8431; three single-object out-blocks (two details + a comparison block).
// ---------------------------------------------------------------------------

/// Covers AE3. `T1958Request::new` serializes both shcodes under `t1958InBlock`,
/// no continuation leak.
#[test]
fn t1958_request_serializes_with_both_shcodes() {
    let value = serde_json::to_value(T1958Request::new("57J123", "57J456"))
        .expect("serialize t1958 request");
    let inblock = value["t1958InBlock"].as_object().expect("inblock object");
    assert_eq!(inblock.len(), 2, "t1958InBlock carries only shcode1 and shcode2");
    assert_eq!(value["t1958InBlock"]["shcode1"], "57J123");
    assert_eq!(value["t1958InBlock"]["shcode2"], "57J456");
    assert!(value.get("tr_cont").is_none());
}

/// Covers AE3. A representative success deserializes: both symbol detail blocks
/// and the comparison block round-trip, with `hname` (the modeled non-key signal)
/// populated and numeric fields parsing number-or-string.
#[test]
fn t1958_deserializes_success_with_real_values() {
    let resp: T1958Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1958OutBlock": { "hname": "삼성전자콜ELW", "item1": "삼성전자", "elwopt": "2",
            "price": 105, "volume": 100000, "diff": 1.5 },
        "t1958OutBlock1": { "hname": "SK하이닉스풋ELW", "item1": "SK하이닉스", "elwopt": "3",
            "price": "210", "volume": "50000", "diff": "-0.7" },
        "t1958OutBlock2": { "hnamecmp": "비교", "item1cmp": "기초", "pricecmp": 5,
            "volumecmp": 1000, "diffcmp": 0.1 }
    }))
    .expect("representative t1958 success must deserialize");
    assert_eq!(resp.outblock.hname, "삼성전자콜ELW", "symbol 1 detail populated");
    assert_eq!(resp.outblock.price, "105", "price from JSON number");
    assert_eq!(resp.outblock1.price, "210", "price from JSON string");
    assert_eq!(resp.outblock2.pricecmp, "5", "comparison block populated");
}

/// Covers AE3. An empty/degenerate result (unpopulated detail blocks) deserializes
/// and is recognized as the pending case (no comparison payload).
#[test]
fn t1958_empty_result_deserializes_as_empty() {
    let empty: T1958Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1958OutBlock": {},
        "t1958OutBlock1": {},
        "t1958OutBlock2": {}
    }))
    .expect("empty detail blocks must deserialize");
    assert!(
        empty.outblock.hname.is_empty(),
        "an unpopulated symbol-1 block is the pending case, not a flip"
    );
}

/// Covers AE3. `T1958Response` default envelope is empty.
#[test]
fn t1958_response_envelope_default_is_empty() {
    let resp = T1958Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.hname.is_empty());
    assert!(resp.outblock2.hnamecmp.is_empty());
}

// ---------------------------------------------------------------------------
// t1964 — ELW전광판 (ELW board; Wave 1). item (underlying code) self-sourced from
// t9905; broad/default filters for the remaining 10 fields.
// ---------------------------------------------------------------------------

/// `T1964Request::new` serializes the underlying `item` plus the broad/default
/// filters under `t1964InBlock`; no continuation leak.
#[test]
fn t1964_request_serializes_with_item_and_broad_defaults() {
    let value = serde_json::to_value(T1964Request::new("005930"))
        .expect("serialize t1964 request");
    let inblock = value["t1964InBlock"].as_object().expect("inblock object");
    assert_eq!(inblock.len(), 11, "t1964InBlock carries all 11 fields");
    assert_eq!(value["t1964InBlock"]["item"], "005930", "underlying code");
    assert_eq!(value["t1964InBlock"]["elwopt"], "0", "broad call/put filter");
    assert_eq!(value["t1964InBlock"]["issuercd"], "", "broad issuer (all)");
    assert!(value.get("tr_cont").is_none());
}

/// A representative success deserializes: the board rows round-trip with `shcode`
/// (ELW code) and `item1` (underlying code) populated; single-or-array tolerated.
#[test]
fn t1964_deserializes_success_with_real_values() {
    let resp: T1964Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1964OutBlock1": [
            { "shcode": "57J123", "hname": "삼성전자콜ELW", "item1": "005930",
              "itemnm": "삼성전자", "issuernmk": "한국투자" },
            { "shcode": 57456, "hname": "삼성전자풋ELW", "item1": "005930",
              "itemnm": "삼성전자", "issuernmk": "미래에셋" }
        ]
    }))
    .expect("representative t1964 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "57J123", "ELW code populated");
    assert_eq!(resp.outblock1[0].item1, "005930", "underlying code populated");
    assert_eq!(resp.outblock1[1].shcode, "57456", "shcode from JSON number");
}

/// An empty board (`00707`, empty array) deserializes and is the pending case.
#[test]
fn t1964_empty_result_deserializes_as_empty() {
    let empty: T1964Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1964OutBlock1": []
    }))
    .expect("empty board must deserialize");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case");
}

/// A single board row (not an array) is tolerated as a one-element Vec.
#[test]
fn t1964_single_out_row_tolerated_as_array() {
    let single: T1964Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1964OutBlock1": { "shcode": "57J123", "item1": "005930" }
    }))
    .expect("single board row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "57J123");
}

/// `T1964OutBlock1.shcode` parses number-or-string alike.
#[test]
fn t1964_shcode_number_or_string_yields_same_value() {
    let n: T1964OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": 57123 })).expect("number");
    let s: T1964OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": "57123" })).expect("string");
    assert_eq!(n.shcode, "57123");
    assert_eq!(n.shcode, s.shcode);
}

// ---- t1950 ELW현재가(시세)조회 (closed-window more-flips; market_session ELW single-instrument quote + basket array) ----

/// `t1950` serializes to `{"t1950InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1950_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1950Request::new("520012")).expect("serialize t1950 request");
    assert_eq!(value["t1950InBlock"]["shcode"], "520012");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1950InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1950Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1950InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. Numeric-bearing fields tolerate a JSON number
/// or string via `string_or_number`; the main quote is ONE object, the basket is a
/// repeated array.
#[test]
fn t1950_success_body_deserializes_with_nondefault_field() {
    let as_string: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1950OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": "000000000150", "sign": "5",
            "change": "-00000000010", "diff": "-6.25", "volume": "000000123456",
            "value": "000001234567", "bcode": "005930", "bprice": "000000061900"
        },
        "t1950OutBlock1": [
            { "bskcode": "005930", "bskbno": "100", "bskprice": "000000061900" }
        ],
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1950OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": 150, "sign": "5", "change": -10,
            "diff": -6.25, "volume": 123456, "value": 1234567i64, "bcode": "005930",
            "bprice": 61900
        }
    }))
    .expect("number body (numeric fields as JSON Numbers) must deserialize");
    assert_eq!(
        as_string.outblock.hname, "KBJ05삼성전자콜",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock.price, "000000000150");
    assert_eq!(as_string.outblock1.len(), 1, "basket array round-trips");
    assert_eq!(as_string.outblock1[0].bskcode, "005930");
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.price, "150");
    assert_eq!(as_number.outblock.diff, "-6.25");
    assert_eq!(as_number.outblock.hname, "KBJ05삼성전자콜");
}

/// A single (non-array) `t1950OutBlock1` basket object is tolerated via
/// `de_vec_or_single`.
#[test]
fn t1950_single_basket_out_block_is_tolerated() {
    let resp: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1950OutBlock": { "hname": "KBJ05삼성전자콜", "price": "000000000150" },
        "t1950OutBlock1": { "bskcode": "005930", "bskbno": "100", "bskprice": "000000061900" }
    }))
    .expect("single-object basket out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].bskcode, "005930");
}

/// An empty result (`00707`, no out-blocks) deserializes cleanly — the main quote
/// is its `Default`, the basket array is empty; recognized as the empty/pending
/// case.
#[test]
fn t1950_empty_result_deserializes_to_defaults() {
    let empty: T1950Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1950 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.hname.is_empty(), "no quote → default object");
    assert!(empty.outblock1.is_empty(), "no basket → empty Vec");
}

// ---- t1954 ELW일별주가 (open-window flip wave 2026-06-30; market_session ELW daily OHLCV series) ----

/// `t1954` serializes to `{"t1954InBlock":{"shcode":"52XXXX","date":"","cnt":20}}` —
/// shcode/date under the renamed key, and `cnt` as a JSON NUMBER (not a string) per
/// `string_as_number` (the string form risks IGW40011). Non-paginated — no tr_cont.
#[test]
fn t1954_request_serializes_with_numeric_cnt() {
    let value =
        serde_json::to_value(T1954Request::new("52L905", "", 20)).expect("serialize t1954 request");
    assert_eq!(value["t1954InBlock"]["shcode"], "52L905");
    assert_eq!(value["t1954InBlock"]["date"], "");
    assert!(
        value["t1954InBlock"]["cnt"].is_number(),
        "cnt must serialize as a JSON number, not a string"
    );
    assert_eq!(value["t1954InBlock"]["cnt"], serde_json::json!(20));
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    // for_shcode defaults to the latest session + 20 rows.
    let other = serde_json::to_value(T1954Request::for_shcode("58L034")).expect("serialize");
    assert_eq!(other["t1954InBlock"]["shcode"], "58L034");
    assert_eq!(other["t1954InBlock"]["cnt"], serde_json::json!(20));
}

/// A representative success body deserializes AND a modeled non-key field (`close`)
/// holds a real, non-default value. The gateway sends OHLC as JSON numbers and the
/// analytics as strings, so every numeric-bearing field tolerates both via
/// `string_or_number`; the daily rows are under the `t1954OutBlock1` array.
#[test]
fn t1954_success_body_deserializes_with_nondefault_field() {
    // Mixed wire types: open/high/low/close/change as JSON numbers, analytics as strings.
    let body: T1954Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1954OutBlock1": [
            {
                "date": "20230608", "open": 210, "high": 210, "low": 210, "close": 210,
                "sign": "2", "change": 60, "diff": "40.00", "volume": "000000000030",
                "parity": "171.65", "egearing": "0.73", "premium": "-14.43",
                "gearing": "3.66", "mness": "2"
            }
        ],
        "rsp_msg": "조회완료"
    }))
    .expect("mixed-type body must deserialize");
    assert_eq!(body.outblock1.len(), 1, "daily array round-trips");
    assert_eq!(
        body.outblock1[0].close, "210",
        "JSON-number close coerces to the String witness"
    );
    assert_eq!(body.outblock1[0].date, "20230608");
    assert_eq!(
        body.outblock1[0].parity, "171.65",
        "string analytic round-trips"
    );
    // close also tolerates a string form.
    let as_string: T1954Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1954OutBlock1": [ { "date": "20230608", "close": "000000000210", "volume": "30" } ]
    }))
    .expect("string-close body must deserialize");
    assert_eq!(as_string.outblock1[0].close, "000000000210");
}

/// A populated `t1954OutBlock` header round-trips its base-asset rename keys
/// (`bsjgubun`/`bscode`/`bjcode`), locking the header field names against drift —
/// the success-body test only exercises the `t1954OutBlock1` rows.
#[test]
fn t1954_header_block_round_trips_base_asset_keys() {
    let resp: T1954Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1954OutBlock": { "date": "20230608", "bsjgubun": "1", "bscode": "005930", "bjcode": "201" },
        "t1954OutBlock1": [ { "date": "20230608", "close": 210 } ]
    }))
    .expect("populated-header body must deserialize");
    assert_eq!(resp.outblock.bscode, "005930", "header 현물 code rename key");
    assert_eq!(resp.outblock.bjcode, "201", "header 지수 code rename key");
    assert_eq!(resp.outblock.bsjgubun, "1");
    assert_eq!(resp.outblock1.len(), 1);
}

/// A single (non-array) `t1954OutBlock1` row is tolerated via `de_vec_or_single`.
#[test]
fn t1954_single_out_block_is_tolerated() {
    let resp: T1954Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1954OutBlock1": { "date": "20230608", "close": 210 }
    }))
    .expect("single-object row must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].close, "210");
}

/// An empty result (`00707`, no out-blocks) deserializes cleanly — header default,
/// daily array empty; the empty/pending case.
#[test]
fn t1954_empty_result_deserializes_to_defaults() {
    let empty: T1954Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1954 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
    assert!(empty.outblock.date.is_empty(), "no header → default object");
}

// ---- t1971 ELW현재가호가조회 (closed-window more-flips; market_session ELW current-price + quote board, single object) ----

/// `t1971` serializes to `{"t1971InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1971_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1971Request::new("520012")).expect("serialize t1971 request");
    assert_eq!(value["t1971InBlock"]["shcode"], "520012");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1971InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1971Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1971InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. Numeric-bearing fields tolerate a JSON number
/// or string via `string_or_number`; the quote-board is ONE object (no array
/// secondary block per the baseline).
#[test]
fn t1971_success_body_deserializes_with_nondefault_field() {
    let as_string: T1971Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1971OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": "000000000150", "sign": "5",
            "change": "-00000000010", "diff": "-6.25", "volume": "000000123456",
            "offerho1": "000000000155", "bidho1": "000000000150",
            "offerrem1": "000000010000", "bidrem1": "000000020000",
            "open": "000000000160", "high": "000000000170", "low": "000000000145",
            "invidx": "1", "koba_stdprc": "000000080000", "koba_acc_rt": "12.50",
            "koba_yn": "N"
        },
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1971Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1971OutBlock": {
            "hname": "KBJ05삼성전자콜", "price": 150, "sign": "5", "change": -10,
            "diff": -6.25, "volume": 123456, "offerho1": 155, "bidho1": 150,
            "offerrem1": 10000, "bidrem1": 20000, "open": 160, "high": 170,
            "low": 145, "invidx": 1, "koba_stdprc": 80000, "koba_acc_rt": 12.5,
            "koba_yn": "N"
        }
    }))
    .expect("number body (numeric fields as JSON Numbers) must deserialize");
    assert_eq!(
        as_string.outblock.hname, "KBJ05삼성전자콜",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock.price, "000000000150");
    assert_eq!(as_string.outblock.offerho1, "000000000155");
    assert_eq!(as_string.outblock.koba_yn, "N");
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.price, "150");
    assert_eq!(as_number.outblock.diff, "-6.25");
    assert_eq!(as_number.outblock.bidho1, "150");
    assert_eq!(as_number.outblock.hname, "KBJ05삼성전자콜");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the quote-board
/// is its `Default`; recognized as the empty/pending case.
#[test]
fn t1971_empty_result_deserializes_to_defaults() {
    let empty: T1971Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1971 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.hname.is_empty(),
        "no quote → default object"
    );
}

// ---- t1972 ELW현재가(거래원)조회 (closed-window more-flips; market_session ELW current-price + trading-member board, single object) ----

/// `t1972` serializes to `{"t1972InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1972_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1972Request::new("52HAAM")).expect("serialize t1972 request");
    assert_eq!(value["t1972InBlock"]["shcode"], "52HAAM");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1972InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1972Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1972InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. Numeric-bearing fields tolerate a JSON number
/// or string via `string_or_number` (the gateway sends the ratios as strings and
/// the volumes/increments as JSON numbers); the board is ONE object (no array
/// secondary block per the baseline).
#[test]
fn t1972_success_body_deserializes_with_nondefault_field() {
    let as_string: T1972Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1972OutBlock": {
            "hname": "미래HAAM네이버콜", "expcode": "KRA521138CB0", "shcode": "J52HAAM",
            "offerno1": "미래에", "bidno1": "미래에",
            "dvol1": "2820", "svol1": "2130", "dcha1": "0", "scha1": "0",
            "ddiff1": "99.65", "sdiff1": "75.27",
            "fwdvl": "0", "fwsvl": "0", "fwddiff": "0.00", "fwsdiff": "0.00"
        },
        "rsp_msg": "정상적으로 조회가 완료되었습니다."
    }))
    .expect("string body must deserialize");
    let as_number: T1972Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1972OutBlock": {
            "hname": "미래HAAM네이버콜", "expcode": "KRA521138CB0", "shcode": "J52HAAM",
            "offerno1": "미래에", "bidno1": "미래에",
            "dvol1": 2820, "svol1": 2130, "dcha1": 0, "scha1": 0,
            "ddiff1": "99.65", "sdiff1": "75.27",
            "fwdvl": 0, "fwsvl": 0, "fwddiff": "0.00", "fwsdiff": "0.00"
        }
    }))
    .expect("number body (volume fields as JSON Numbers) must deserialize");
    assert_eq!(
        as_string.outblock.hname, "미래HAAM네이버콜",
        "modeled non-key field is non-default"
    );
    assert_eq!(as_string.outblock.shcode, "J52HAAM");
    assert_eq!(as_string.outblock.offerno1, "미래에");
    assert_eq!(as_string.outblock.ddiff1, "99.65");
    assert_eq!(as_string.outblock.dvol1, "2820");
    // numeric-bearing fields tolerate BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.dvol1, "2820");
    assert_eq!(as_number.outblock.svol1, "2130");
    assert_eq!(as_number.outblock.fwdvl, "0");
    assert_eq!(as_number.outblock.hname, "미래HAAM네이버콜");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the board is its
/// `Default`; recognized as the empty/pending case.
#[test]
fn t1972_empty_result_deserializes_to_defaults() {
    let empty: T1972Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1972 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.hname.is_empty(),
        "no board → default object"
    );
}

// ---- t1974 ELW기초자산동일종목 (closed-window more-flips; market_session ELWs sharing a base asset, cnt summary + per-issue array) ----

/// `t1974` serializes to `{"t1974InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1974_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1974Request::new("52HAAM")).expect("serialize t1974 request");
    assert_eq!(value["t1974InBlock"]["shcode"], "52HAAM");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1974InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1974Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1974InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key array field
/// (`outblock1[0].hname`) holds a real, non-default value — proving the subset
/// round-trips, not just `serde(default)` returning `Ok`. Numeric-bearing fields
/// tolerate a JSON number or string via `string_or_number` (the gateway sends
/// `price`/`change` as JSON numbers and `volume`/`diff` as strings). The body
/// mirrors the captured gateway example: a `t1974OutBlock` summary (`cnt`) plus the
/// `t1974OutBlock1` sibling-issue array.
#[test]
fn t1974_success_body_deserializes_with_nondefault_field() {
    let body: T1974Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1974OutBlock": { "cnt": 76 },
        "t1974OutBlock1": [
            { "volume": "000000002830", "price": 250, "shcode": "52HAAM",
              "change": 0, "sign": "3", "cpgubun": "01", "diff": "0.00",
              "hname": "미래HAAM네이버콜" },
            { "volume": "000000000000", "price": 15, "shcode": "52HALF",
              "change": 0, "sign": "3", "cpgubun": "02", "diff": "0.00",
              "hname": "미래HALF네이버풋" }
        ],
        "rsp_msg": "정상적으로 조회가 완료되었습니다."
    }))
    .expect("success body must deserialize");
    assert_eq!(body.outblock.cnt, "76", "summary count round-trips (JSON number)");
    assert_eq!(body.outblock1.len(), 2, "two sibling issues");
    assert_eq!(
        body.outblock1[0].hname, "미래HAAM네이버콜",
        "modeled non-key array field is non-default"
    );
    assert_eq!(body.outblock1[0].shcode, "52HAAM");
    assert_eq!(body.outblock1[0].cpgubun, "01");
    // numeric-bearing fields tolerate BOTH a JSON number (price) and a string (volume).
    assert_eq!(body.outblock1[0].price, "250");
    assert_eq!(body.outblock1[0].volume, "000000002830");
    assert_eq!(body.outblock1[1].hname, "미래HALF네이버풋");
}

/// The `t1974OutBlock1` array tolerates a LONE object (not just a list) via
/// `de_vec_or_single` — a single-sibling base asset still deserializes into a
/// one-element Vec.
#[test]
fn t1974_single_object_array_deserializes_as_vec() {
    let body: T1974Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1974OutBlock": { "cnt": "1" },
        "t1974OutBlock1": {
            "volume": "5", "price": "100", "shcode": "52SOLO",
            "change": "0", "sign": "3", "cpgubun": "01", "diff": "0.00",
            "hname": "단독종목콜"
        }
    }))
    .expect("a lone t1974OutBlock1 object must deserialize as a one-element Vec");
    assert_eq!(body.outblock1.len(), 1, "lone object → one-element Vec");
    assert_eq!(body.outblock1[0].shcode, "52SOLO");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the blocks take
/// their `Default`; recognized as the empty/pending case.
#[test]
fn t1974_empty_result_deserializes_to_defaults() {
    let empty: T1974Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1974 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no siblings → empty Vec");
    assert!(empty.outblock.cnt.is_empty(), "no summary → default cnt");
}

// ---- t1956 ELW현재가(확정지급액)조회 (closed-window more-flips; market_session ELW current-price/payout snapshot, single OutBlock + basket array) ----

/// `t1956` serializes to `{"t1956InBlock":{"shcode":"52XXXX"}}` — the in-block
/// carries the one caller-supplied `shcode` under the renamed key, with no leaked
/// fields. No numeric request slot (no number coercion); non-paginated — no
/// tr_cont tokens.
#[test]
fn t1956_request_serializes_shcode_under_inblock() {
    let value =
        serde_json::to_value(T1956Request::new("52HAAM")).expect("serialize t1956 request");
    assert_eq!(value["t1956InBlock"]["shcode"], "52HAAM");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1956InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
    // for_shcode is the same constructor.
    let other = serde_json::to_value(T1956Request::for_shcode("580034")).expect("serialize");
    assert_eq!(other["t1956InBlock"]["shcode"], "580034");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`outblock.hname`, the NAME witness) holds a real, non-default value — proving the
/// subset round-trips, not just `serde(default)` returning `Ok`. Numeric-bearing
/// fields tolerate a JSON number or string via `string_or_number` (the gateway sends
/// `price`/`givemoney` as JSON numbers and `volume`/`diff` as strings). The body
/// mirrors the captured gateway example: a single `t1956OutBlock` snapshot plus the
/// `t1956OutBlock1` basket array.
#[test]
fn t1956_success_body_deserializes_with_nondefault_field() {
    let body: T1956Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1956OutBlock": {
            "hname": "미래HAAM네이버콜", "price": 250, "diff": "1.23",
            "volume": "000000002830", "elwexec": 35000, "impv": "0.18",
            "delt": "0.42", "bcode": "035420", "givemoney": 0
        },
        "t1956OutBlock1": [
            { "bskcode": "035420", "bskbno": "1", "bskprice": 198500, "bskvolume": "120000" }
        ],
        "rsp_msg": "정상적으로 조회가 완료되었습니다."
    }))
    .expect("success body must deserialize");
    assert_eq!(
        body.outblock.hname, "미래HAAM네이버콜",
        "modeled non-key NAME field is non-default"
    );
    // numeric-bearing fields tolerate BOTH a JSON number (price) and a string (volume).
    assert_eq!(body.outblock.price, "250");
    assert_eq!(body.outblock.volume, "000000002830");
    assert_eq!(body.outblock.elwexec, "35000");
    assert_eq!(body.outblock.bcode, "035420");
    assert_eq!(body.outblock1.len(), 1, "one basket constituent");
    assert_eq!(body.outblock1[0].bskcode, "035420");
    assert_eq!(body.outblock1[0].bskprice, "198500");
}

/// The `t1956OutBlock1` basket array tolerates a LONE object (not just a list) via
/// `de_vec_or_single` — a single-constituent basket still deserializes into a
/// one-element Vec.
#[test]
fn t1956_single_object_array_deserializes_as_vec() {
    let body: T1956Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1956OutBlock": { "hname": "단독종목콜", "price": "100" },
        "t1956OutBlock1": {
            "bskcode": "005930", "bskbno": "1", "bskprice": "70000", "bskvolume": "5"
        }
    }))
    .expect("a lone t1956OutBlock1 object must deserialize as a one-element Vec");
    assert_eq!(body.outblock1.len(), 1, "lone object → one-element Vec");
    assert_eq!(body.outblock1[0].bskcode, "005930");
    assert_eq!(body.outblock.hname, "단독종목콜");
}

/// An empty result (`00707`, no out-block) deserializes cleanly — the blocks take
/// their `Default`; recognized as the empty/pending case.
#[test]
fn t1956_empty_result_deserializes_to_defaults() {
    let empty: T1956Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1956 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no basket → empty Vec");
    assert!(empty.outblock.hname.is_empty(), "no snapshot → default hname");
}

// ---- t1969 ELW지표검색 (closed-window more-flips; market_session ELW screener, summary + per-issue array) ----

/// `t1969::new()` serializes the all-ELWs screen under `t1969InBlock`. The numeric
/// range bounds MUST serialize as JSON NUMBERS (not strings) — the string form
/// returns `IGW40011` at the gateway. The chk*/cb*/date filters stay JSON strings.
/// Non-paginated — no tr_cont tokens, no leaked caller fields.
#[test]
fn t1969_new_serializes_numeric_ranges_as_json_numbers() {
    let value = serde_json::to_value(T1969Request::new()).expect("serialize t1969 request");
    let inblock = &value["t1969InBlock"];
    // Numeric range bounds → JSON numbers.
    for key in [
        "elwexecs", "elwexece", "volumes", "volumee", "rates", "ratee", "premiums", "premiume",
        "paritys", "paritye", "berates", "beratee", "capts", "capte", "egearings", "egearinge",
        "gearings", "gearinge", "deltas", "deltae", "thetas", "thetae",
    ] {
        assert!(
            inblock[key].is_number(),
            "{key} must serialize as a JSON number (string form → IGW40011)"
        );
        assert_eq!(inblock[key], 0, "{key} defaults to 0 in the all-ELWs screen");
    }
    // chk*/cb*/date filters stay JSON strings.
    assert_eq!(inblock["chkitem"], "0");
    assert_eq!(inblock["cbitem"], "000000000000");
    assert_eq!(inblock["duedates"], "000000");
    assert_eq!(inblock["duedatee"], "999999");
    assert_eq!(inblock["cbexec"], "1");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field (`hname`)
/// holds a real, non-default value — proving the subset round-trips, not just
/// `serde(default)` returning `Ok`. The summary `t1969OutBlock.cnt` and the repeated
/// `t1969OutBlock1` array both round-trip; numeric-bearing fields tolerate a JSON
/// number or string via `string_or_number`.
#[test]
fn t1969_success_body_deserializes_with_nondefault_field() {
    let as_string: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1969OutBlock": { "cnt": "2" },
        "t1969OutBlock1": [
            { "hname": "한국SK001콜", "shcode": "5XX001", "issuernmk": "SK증권", "itemcode": "000000000001", "cpgubun": "01", "price": "000000000050", "sign": "5", "change": "-00000000005", "diff": "-9.09", "volume": "000000100000", "elwexec": "000000030000", "item": "KOSPI200", "lastdate": "20260619", "lpname": "SK증권" },
            { "hname": "미래001풋", "shcode": "5YY002", "issuernmk": "미래에셋", "itemcode": "000000000002", "cpgubun": "02", "price": "000000000035", "sign": "2", "change": "00000000003", "diff": "9.38", "volume": "000000050000", "elwexec": "000000031000", "item": "KOSPI200", "lastdate": "20260619", "lpname": "미래에셋" }
        ],
        "rsp_msg": "조회완료"
    }))
    .expect("string body must deserialize");
    let as_number: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1969OutBlock": { "cnt": 1 },
        "t1969OutBlock1": [
            { "hname": "한국SK001콜", "shcode": "5XX001", "price": 50, "change": -5, "diff": -9.09, "volume": 100000, "elwexec": 30000 }
        ]
    }))
    .expect("number body (numeric fields as JSON Numbers) must deserialize");
    assert_eq!(as_string.outblock.cnt, "2", "summary count round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "per-issue array round-trips");
    assert_eq!(as_string.outblock1[0].shcode, "5XX001", "modeled key round-trips");
    assert_eq!(
        as_string.outblock1[0].hname, "한국SK001콜",
        "modeled non-key field is non-default"
    );
    // Numeric out-block fields tolerated from BOTH a JSON number and a string.
    assert_eq!(as_number.outblock.cnt, "1");
    assert_eq!(as_number.outblock1[0].price, "50");
    assert_eq!(as_number.outblock1[0].diff, "-9.09");
}

/// A single (non-array) `t1969OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1969_single_object_out_block_is_tolerated() {
    let resp: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1969OutBlock": { "cnt": "1" },
        "t1969OutBlock1": { "hname": "한국SK001콜", "shcode": "5XX001", "price": "000000000050" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].shcode, "5XX001");
}

/// An empty result (`00707`, no out-blocks) deserializes cleanly — no panic on a
/// missing `t1969OutBlock`/`t1969OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1969_empty_result_deserializes_to_defaults() {
    let empty: T1969Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1969 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert_eq!(empty.outblock.cnt, "", "absent summary → default");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
}

// ---------------------------------------------------------------------------
// Standalone-lane reads (reach wave U3), routed through `market_session` (KTD3).
// Out-block shape from the raw capture (KTD5): t1988 summary + Object-Array
// detail; t3102 title (Object); t3320 summary + ratios (both Object). Canonical
// field pinned by baseline `korean_name` (KTD6). t1988's numeric request fields
// `from_rate`/`to_rate` assert `.is_number()` (KTD4).
// ---------------------------------------------------------------------------

/// `t1988` request: the numeric rate bounds serialize as JSON NUMBERS (KTD4 —
/// `string_as_number`, avoids IGW40011); the summary + Object-Array detail
/// round-trips and a single detail row collapses to a one-element Vec (KTD5).
/// Canonical 코스피종목건수 (`ksp_cnt`) pinned exactly (KTD6).
#[test]
fn t1988_request_serializes_rate_bounds_as_numbers_and_round_trips() {
    let value = serde_json::to_value(T1988Request::new("0")).expect("serialize t1988");
    assert_eq!(value["t1988InBlock"]["mkt_gb"], "0");
    assert!(
        value["t1988InBlock"]["from_rate"].is_number(),
        "from_rate is a JSON number, not a string (IGW40011 guard)"
    );
    assert!(
        value["t1988InBlock"]["to_rate"].is_number(),
        "to_rate is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["t1988InBlock"]["from_rate"], 0);
    // String filter flags stay quoted (genuine strings).
    assert!(value["t1988InBlock"]["chk_rate"].is_string());
    assert!(
        value.get("t1988OutBlock").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T1988Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1988OutBlock": { "ksp_cnt": "120", "ksd_cnt": 45 },
        "t1988OutBlock1": [
            { "shcode": "005930", "expcode": "KR7005930003", "hname": "삼성전자", "price": 71000, "sign": "2", "volume": "1234567" },
            { "shcode": "000660", "expcode": "KR7000660001", "hname": "SK하이닉스", "price": "128000", "sign": "5", "volume": 987654 }
        ]
    }))
    .expect("representative t1988 success must deserialize");
    // Canonical 코스피종목건수 pinned exactly (KTD6); 코스닥종목건수 from a number form.
    assert_eq!(resp.outblock.ksp_cnt, "120", "코스피종목건수");
    assert_eq!(resp.outblock.ksd_cnt, "45", "코스닥종목건수 from number");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "005930", "단축코드");
    assert_eq!(resp.outblock1[1].price, "128000", "price from string preserved verbatim");

    // single row object → one-element Vec (KTD5).
    let single: T1988Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1988OutBlock": { "ksp_cnt": "1", "ksd_cnt": "0" },
        "t1988OutBlock1": { "shcode": "005930", "hname": "삼성전자", "price": 71000 }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].shcode, "005930");
}

/// `t1988` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t1988_numeric_field_string_or_number() {
    let from_num: T1988OutBlock =
        serde_json::from_value(serde_json::json!({ "ksp_cnt": 120 }))
            .expect("number form deserializes");
    let from_str: T1988OutBlock =
        serde_json::from_value(serde_json::json!({ "ksp_cnt": "120" }))
            .expect("string form deserializes");
    assert_eq!(from_num.ksp_cnt, "120");
    assert_eq!(from_str.ksp_cnt, "120");
}

/// `t1988` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn t1988_empty_result_deserializes_as_pending() {
    let empty: T1988Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.ksp_cnt.is_empty(), "empty summary is the pending case");
    assert!(empty.outblock1.is_empty(), "empty detail array is the pending case");
}
