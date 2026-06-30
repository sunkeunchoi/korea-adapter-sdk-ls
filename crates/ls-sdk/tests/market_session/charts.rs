use super::*;


// ---- t1308 주식시간대별체결조회챠트 (closed-window more-flips; market_session, summary + time-bucket array) ----

/// `t1308` serializes to
/// `{"t1308InBlock":{"shcode":"005930","starttime":"","endtime":"","bun_term":"1","exchgubun":""}}`
/// (all five String fields, `starttime`/`endtime`/`exchgubun` empty for the full
/// session; non-paginated — no tr_cont tokens in the body).
#[test]
fn t1308_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1308Request::new("005930", "", "", "1", ""))
        .expect("serialize t1308 request");
    assert_eq!(value["t1308InBlock"]["shcode"], "005930", "shcode stays a string");
    assert_eq!(value["t1308InBlock"]["starttime"], "", "starttime stays an empty string");
    assert_eq!(value["t1308InBlock"]["endtime"], "", "endtime stays an empty string");
    assert_eq!(value["t1308InBlock"]["bun_term"], "1", "bun_term stays a string (not a number)");
    assert_eq!(value["t1308InBlock"]["exchgubun"], "", "exchgubun stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. The numeric fields tolerate a JSON
/// number or string via `string_or_number`; the time-bucket out-block is a
/// repeated array.
#[test]
fn t1308_success_body_deserializes_with_nondefault_field() {
    let as_number: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1308OutBlock": { "ex_shcode": "005930" },
        "t1308OutBlock1": [
            { "chetime": "102700", "price": 3685, "sign": "2", "change": 25, "volume": 321201, "open": 3685, "high": 3685, "low": 3685 },
            { "chetime": "090030", "price": 3660, "sign": "3", "change": 0, "volume": 19857, "open": 3660, "high": 3660, "low": 3660 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1308OutBlock": { "ex_shcode": "005930" },
        "t1308OutBlock1": [
            { "chetime": "102700", "price": "3685", "sign": "2", "change": "25", "volume": "321201", "open": "3685", "high": "3685", "low": "3685" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "time-bucket array round-trips");
    assert_eq!(as_number.outblock.ex_shcode, "005930", "summary block round-trips");
    assert_eq!(as_number.outblock1[0].price, "3685", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].chetime, "102700");
    assert_eq!(as_number.outblock1[0].price, as_string.outblock1[0].price);
    assert_eq!(as_number.outblock1[0].volume, as_string.outblock1[0].volume);
}

/// A single (non-array) `t1308OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1308_single_object_out_block_is_tolerated() {
    let resp: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1308OutBlock1": { "chetime": "102700", "price": 3685, "volume": 321201 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].chetime, "102700");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1308OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1308_empty_result_deserializes_to_defaults() {
    let empty: T1308Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1308 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.ex_shcode.is_empty(), "summary block defaults to empty");
}

// ---- t1449 가격대별매매비중조회 (closed-window more-flips; market_session, summary + price-band array) ----

/// `t1449` serializes to `{"t1449InBlock":{"shcode":"005930","dategb":"1"}}`
/// (both String fields; `dategb` non-empty; non-paginated — no tr_cont tokens
/// in the body).
#[test]
fn t1449_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1449Request::new("005930", "1"))
        .expect("serialize t1449 request");
    assert_eq!(value["t1449InBlock"]["shcode"], "005930", "shcode stays a string");
    assert_eq!(value["t1449InBlock"]["dategb"], "1", "dategb stays a string (not a number)");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. The numeric fields tolerate a JSON
/// number or string via `string_or_number`; the price-band out-block is a
/// repeated array. Body shape is the captured raw `res_example`.
#[test]
fn t1449_success_body_deserializes_with_nondefault_field() {
    let as_number: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1449OutBlock": { "volume": 322192, "price": 3685, "change": 25, "msvolume": 195607, "sign": "2", "diff": "0.68", "mdvolume": 120522 },
        "t1449OutBlock1": [
            { "price": 3750, "change": 90, "msvolume": 22107, "sign": "2", "msdiff": "100.00", "diff": "6.86", "tickdiff": "2.46", "mdvolume": 0, "cvolume": 22107 },
            { "price": 3645, "change": -15, "msvolume": 0, "sign": "5", "msdiff": "0.00", "diff": "0.05", "tickdiff": "-0.41", "mdvolume": 147, "cvolume": 147 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1449OutBlock": { "volume": "322192", "price": "3685", "change": "25", "msvolume": "195607", "sign": "2", "diff": "0.68", "mdvolume": "120522" },
        "t1449OutBlock1": [
            { "price": "3750", "change": "90", "msvolume": "22107", "sign": "2", "msdiff": "100.00", "diff": "6.86", "tickdiff": "2.46", "mdvolume": "0", "cvolume": "22107" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "price-band array round-trips");
    assert_eq!(as_number.outblock.price, "3685", "summary block round-trips");
    assert_eq!(as_number.outblock1[0].price, "3750", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].cvolume, "22107");
    assert_eq!(as_number.outblock1[0].price, as_string.outblock1[0].price);
    assert_eq!(as_number.outblock.volume, as_string.outblock.volume);
}

/// A single (non-array) `t1449OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1449_single_object_out_block_is_tolerated() {
    let resp: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1449OutBlock1": { "price": 3750, "cvolume": 22107 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "3750");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1449OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1449_empty_result_deserializes_to_defaults() {
    let empty: T1449Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1449 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.price.is_empty(), "summary block defaults to empty");
}

// ---- t1621 업종별분별투자자매매동향 (closed-window more-flips; market_session, header + by-time array) ----

/// `t1621` serializes to
/// `{"t1621InBlock":{"upcode":"001","nmin":0,"cnt":20,"bgubun":"0","exchgubun":""}}`
/// — `upcode`/`bgubun`/`exchgubun` stay strings, but `nmin` and `cnt` MUST
/// serialize as JSON NUMBERS (KTD3 — the string form returns IGW40011 at the
/// gateway). Non-paginated — no tr_cont tokens in the body.
#[test]
fn t1621_request_serializes_nmin_and_cnt_as_numbers() {
    let value = serde_json::to_value(T1621Request::new("001", "0", "20", "0", ""))
        .expect("serialize t1621 request");
    assert_eq!(value["t1621InBlock"]["upcode"], "001", "upcode stays a string");
    assert_eq!(value["t1621InBlock"]["bgubun"], "0", "bgubun stays a string");
    assert_eq!(value["t1621InBlock"]["exchgubun"], "", "exchgubun stays a string");
    assert_eq!(
        value["t1621InBlock"]["nmin"], 0,
        "nmin serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert_eq!(
        value["t1621InBlock"]["cnt"], 20,
        "cnt serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert!(value["t1621InBlock"]["cnt"].is_number(), "cnt is a JSON number");
    assert!(value["t1621InBlock"]["nmin"].is_number(), "nmin is a JSON number");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`indmsvol`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. Numeric fields tolerate a JSON
/// number or string via `string_or_number`; the by-time out-block is a repeated
/// array.
#[test]
fn t1621_success_body_deserializes_with_nondefault_field() {
    let as_number: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1621OutBlock": { "jisucd": "001", "jisunm": "KOSPI", "ex_upcode": "001" },
        "t1621OutBlock1": [
            { "date": "20260627", "time": "153000", "indmsvol": 12345, "indmsamt": -67890, "formsvol": -222, "formsamt": 333, "sysmsvol": 444, "sysmsamt": -555, "upclose": 2580 },
            { "date": "20260627", "time": "152900", "indmsvol": -10, "indmsamt": 20, "formsvol": 30, "formsamt": -40, "sysmsvol": -50, "sysmsamt": 60, "upclose": 2579 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1621OutBlock": { "jisucd": "001", "jisunm": "KOSPI", "ex_upcode": "001" },
        "t1621OutBlock1": [
            { "date": "20260627", "time": "153000", "indmsvol": "12345", "indmsamt": "-67890", "formsvol": "-222", "formsamt": "333", "sysmsvol": "444", "sysmsamt": "-555", "upclose": "2580" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "by-time array round-trips");
    assert_eq!(as_number.outblock.jisunm, "KOSPI", "header block round-trips");
    assert_eq!(as_number.outblock1[0].indmsvol, "12345", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].indmsamt, "-67890");
    assert_eq!(as_number.outblock1[0].indmsvol, as_string.outblock1[0].indmsvol);
    assert_eq!(as_number.outblock1[0].upclose, as_string.outblock1[0].upclose);
}

/// A single (non-array) `t1621OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1621_single_object_out_block_is_tolerated() {
    let resp: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1621OutBlock1": { "date": "20260627", "time": "153000", "indmsvol": 12345 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].indmsvol, "12345");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1621OutBlock1`; recognized as the empty/pending case.
#[test]
fn t1621_empty_result_deserializes_to_defaults() {
    let empty: T1621Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1621 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.jisunm.is_empty(), "header block defaults to empty");
}

// ---- t2545 상품선물투자자매매동향 (closed-window more-flips; market_session, header + by-time array) ----

/// `t2545` serializes to
/// `{"t2545InBlock":{"eitem":"01","sgubun":"0","upcode":"001","nmin":0,"cnt":10,"bgubun":"0"}}`
/// — `eitem`/`sgubun`/`upcode`/`bgubun` stay strings, but `nmin` and `cnt` MUST
/// serialize as JSON NUMBERS (KTD3 — the string form returns IGW40011 at the
/// gateway). Non-paginated — no tr_cont tokens in the body.
#[test]
fn t2545_request_serializes_nmin_and_cnt_as_numbers() {
    let value = serde_json::to_value(T2545Request::new("01", "0", "001", "0", "10", "0"))
        .expect("serialize t2545 request");
    assert_eq!(value["t2545InBlock"]["eitem"], "01", "eitem stays a string");
    assert_eq!(value["t2545InBlock"]["sgubun"], "0", "sgubun stays a string");
    assert_eq!(value["t2545InBlock"]["upcode"], "001", "upcode stays a string");
    assert_eq!(value["t2545InBlock"]["bgubun"], "0", "bgubun stays a string");
    assert_eq!(
        value["t2545InBlock"]["nmin"], 0,
        "nmin serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert_eq!(
        value["t2545InBlock"]["cnt"], 10,
        "cnt serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert!(value["t2545InBlock"]["cnt"].is_number(), "cnt is a JSON number");
    assert!(value["t2545InBlock"]["nmin"].is_number(), "nmin is a JSON number");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`indmsvol`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. Numeric fields tolerate a JSON
/// number or string via `string_or_number`; the by-time out-block is a repeated
/// array.
#[test]
fn t2545_success_body_deserializes_with_nondefault_field() {
    let as_number: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2545OutBlock": { "eitem": "01", "sgubun": "0", "jisucd": "001", "jisunm": "KOSPI200" },
        "t2545OutBlock1": [
            { "date": "20260627", "time": "153000", "datetime": "20260627153000", "indmsvol": 12345, "indmsamt": -67890, "formsvol": -222, "formsamt": 333, "sysmsvol": 444, "sysmsamt": -555, "upclose": 358 },
            { "date": "20260627", "time": "152900", "datetime": "20260627152900", "indmsvol": -10, "indmsamt": 20, "formsvol": 30, "formsamt": -40, "sysmsvol": -50, "sysmsamt": 60, "upclose": 357 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2545OutBlock": { "eitem": "01", "sgubun": "0", "jisucd": "001", "jisunm": "KOSPI200" },
        "t2545OutBlock1": [
            { "date": "20260627", "time": "153000", "datetime": "20260627153000", "indmsvol": "12345", "indmsamt": "-67890", "formsvol": "-222", "formsamt": "333", "sysmsvol": "444", "sysmsamt": "-555", "upclose": "358" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "by-time array round-trips");
    assert_eq!(as_number.outblock.jisunm, "KOSPI200", "header block round-trips");
    assert_eq!(as_number.outblock1[0].indmsvol, "12345", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].indmsamt, "-67890");
    assert_eq!(as_number.outblock1[0].indmsvol, as_string.outblock1[0].indmsvol);
    assert_eq!(as_number.outblock1[0].upclose, as_string.outblock1[0].upclose);
}

/// A single (non-array) `t2545OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t2545_single_object_out_block_is_tolerated() {
    let resp: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2545OutBlock1": { "date": "20260627", "time": "153000", "indmsvol": 12345 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].indmsvol, "12345");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t2545OutBlock1`; recognized as the empty/pending case.
#[test]
fn t2545_empty_result_deserializes_to_defaults() {
    let empty: T2545Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t2545 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
    assert!(empty.outblock.jisunm.is_empty(), "header block defaults to empty");
}

// ---- t8406 주식선물틱분별체결조회 (closed-window more-flips; market_session, conclusion array) ----

/// `t8406` serializes to
/// `{"t8406InBlock":{"focode":"101TC000","cgubun":"1","bgubun":0,"cnt":10}}`
/// — `focode`/`cgubun` stay strings, but `bgubun` and `cnt` MUST serialize as
/// JSON NUMBERS (KTD3 — the string form returns IGW40011 at the gateway).
/// Non-paginated — no tr_cont tokens in the body.
#[test]
fn t8406_request_serializes_bgubun_and_cnt_as_numbers() {
    let value = serde_json::to_value(T8406Request::new("101TC000", "1", "0", "10"))
        .expect("serialize t8406 request");
    assert_eq!(value["t8406InBlock"]["focode"], "101TC000", "focode stays a string");
    assert_eq!(value["t8406InBlock"]["cgubun"], "1", "cgubun stays a string");
    assert_eq!(
        value["t8406InBlock"]["bgubun"], 0,
        "bgubun serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert_eq!(
        value["t8406InBlock"]["cnt"], 10,
        "cnt serializes as a JSON NUMBER (KTD3), not a string"
    );
    assert!(value["t8406InBlock"]["bgubun"].is_number(), "bgubun is a JSON number");
    assert!(value["t8406InBlock"]["cnt"].is_number(), "cnt is a JSON number");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips,
/// not just that `serde(default)` returned `Ok`. Numeric fields tolerate a JSON
/// number or string via `string_or_number`; the conclusion out-block is a
/// repeated array. Body shape taken from the raw capture `res_example`.
#[test]
fn t8406_success_body_deserializes_with_nondefault_field() {
    let as_number: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8406OutBlock1": [
            { "chetime": "151949", "price": 70700, "sign": "5", "change": 500, "open": 0, "high": 0, "low": 0, "volume": 811347, "value": 570684700000i64, "openyak": 291595, "cvolume": 197 },
            { "chetime": "151947", "price": 70700, "sign": "5", "change": 500, "open": 0, "high": 0, "low": 0, "volume": 811150, "value": 570545421000i64, "openyak": 291595, "cvolume": 3 }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8406OutBlock1": [
            { "chetime": "151949", "price": "70700", "sign": "5", "change": "500", "open": "0", "high": "0", "low": "0", "volume": "000000811347", "value": "570684700000", "openyak": "291595", "cvolume": "197" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock1.len(), 2, "conclusion array round-trips");
    assert_eq!(as_number.outblock1[0].price, "70700", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock1[0].chetime, "151949");
    assert_eq!(as_number.outblock1[0].price, as_string.outblock1[0].price);
    assert_eq!(as_number.outblock1[0].openyak, as_string.outblock1[0].openyak);
}

/// A single (non-array) `t8406OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t8406_single_object_out_block_is_tolerated() {
    let resp: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8406OutBlock1": { "chetime": "151949", "price": 70700, "cvolume": 197 }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "70700");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t8406OutBlock1`; recognized as the empty/pending case.
#[test]
fn t8406_empty_result_deserializes_to_defaults() {
    let empty: T8406Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t8406 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "no out-block → empty Vec");
}

/// `g3102` numeric request fields serialize as JSON NUMBERS (KTD4); the header +
/// Object-Array detail round-trips; canonical 현재가 (`price`) pinned exactly from
/// a non-collapsing row (KTD6); a single detail row collapses to a one-element
/// Vec (KTD5).
#[test]
fn g3102_request_serializes_counts_as_numbers_and_array_round_trips() {
    let value = serde_json::to_value(G3102Request::new("R", "82TSLA", "82", "TSLA", "30", "0"))
        .expect("serialize g3102");
    assert!(
        value["g3102InBlock"]["readcnt"].is_number(),
        "readcnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert!(
        value["g3102InBlock"]["cts_seq"].is_number(),
        "cts_seq is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["g3102InBlock"]["readcnt"], 30);
    assert_eq!(value["g3102InBlock"]["cts_seq"], 0);

    let resp: G3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3102OutBlock": { "symbol": "TSLA", "cts_seq": 20250428014018000i64, "rec_count": 30 },
        "g3102OutBlock1": [
            { "locdate": "20250428", "loctime": "014101", "price": "283.9500", "open": "285.0900", "high": "285.3100", "low": "281.8400", "exevol": 20 },
            { "locdate": "20250428", "loctime": "014055", "price": "284.0000", "open": "285.0900", "high": "285.3100", "low": "281.8400", "exevol": 10 }
        ]
    }))
    .expect("representative g3102 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].price, "283.9500", "현재가");
    assert_ne!(resp.outblock1[0].price, resp.outblock1[0].open, "non-collapsing: price≠open");
    assert_eq!(resp.outblock.rec_count, "30", "레코드카운트");

    // single row object → one-element Vec (KTD5).
    let single: G3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3102OutBlock": { "symbol": "TSLA", "rec_count": "1" },
        "g3102OutBlock1": { "locdate": "20250428", "price": "283.9500" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: G3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "g3102OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty array is the pending case");
}

/// `g3103` rename + header/bar Object-Array round-trips; canonical 현재가
/// (`price`) pinned exactly from a non-collapsing bar (KTD6); single → Vec (KTD5).
#[test]
fn g3103_request_renames_and_bar_array_round_trips() {
    let value = serde_json::to_value(G3103Request::new("R", "82TSLA", "82", "TSLA", "4", "20250120"))
        .expect("serialize g3103");
    assert_eq!(value["g3103InBlock"]["gubun"], "4");
    assert_eq!(value["g3103InBlock"]["date"], "20250120");
    assert!(value.get("g3103OutBlock").is_none(), "no out-block leaks");

    let resp: G3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3103OutBlock": { "symbol": "TSLA", "gubun": "4", "date": "20221031" },
        "g3103OutBlock1": [
            { "chedate": "20250428", "price": "283.4300", "volume": 2568819717i64, "open": "263.8000", "high": "286.8500", "low": "214.2500" },
            { "chedate": "20250331", "price": "259.1600", "volume": 2721582212i64, "open": "300.3400", "high": "303.9400", "low": "217.0200" }
        ]
    }))
    .expect("representative g3103 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].chedate, "20250428", "영업일자");
    assert_eq!(resp.outblock1[0].price, "283.4300", "현재가");
    assert_ne!(resp.outblock1[0].price, resp.outblock1[0].high, "non-collapsing: price≠high");

    let single: G3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3103OutBlock": { "symbol": "TSLA" },
        "g3103OutBlock1": { "chedate": "20250428", "price": "283.4300" }
    }))
    .expect("single bar deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: G3103Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "g3103OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty bar array is the pending case");
}

// === plan -003 all-lane wave — o3104 / o3127 / t8462 (market_session) =========

// --- o3104 — 해외선물 일별체결 (daily executions) ------------------------------

#[test]
fn o3104_request_serializes_to_inblock() {
    let value =
        serde_json::to_value(O3104Request::new("CUSN26", "20260626")).expect("serialize o3104");
    assert_eq!(value["o3104InBlock"]["shcode"], "CUSN26");
    assert_eq!(value["o3104InBlock"]["date"], "20260626");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

#[tokio::test]
async fn o3104_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-futureoption/market-data"))
        .and(header("tr_cd", "o3104"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rsp_cd":"00000","o3104OutBlock1":[{"volume":57123,"chedate":"20230501","high":"0.66820","low":"0.66215","price":"0.66435","change":"0.00150","sign":"2","diff":"0.23","cgubun":"","open":"0.66300"}],"rsp_msg":"조회완료"}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .market_session()
        .overseas_futures_daily(&O3104Request::new("CUSN26", "20260626"))
        .await
        .expect("o3104 should succeed");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "0.66435", "체결가 round-trips");
}

#[test]
fn o3104_single_or_array_and_empty_deserialize() {
    let single: O3104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3104OutBlock1": { "price": "0.66435", "volume": 57123 }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "0.66435");

    let empty: O3104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3104OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

// === plan -004 batch A — t1302 분별주가 offline coverage =====================

/// t1302 — 주식분별주가. `cnt` is a JSON number (IGW40011 guard); a representative
/// minute-row body round-trips with a real value; empty 00707 recognized.
#[test]
fn t1302_request_and_response_round_trip() {
    let v = serde_json::to_value(T1302Request::new("001200", "0", "20")).expect("serialize t1302");
    let ib = &v["t1302InBlock"];
    assert!(ib["cnt"].is_number(), "cnt is a JSON number (IGW40011 guard)");
    assert_eq!(ib["shcode"], "001200", "shcode stays a string");
    assert_eq!(ib["exchgubun"], "K");

    let resp: T1302Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1302OutBlock": { "cts_time": "101700" },
        "t1302OutBlock1": [{ "chetime": "102700", "close": 3685, "volume": 321201, "sign": "2" }]
    })).expect("t1302 body round-trips");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "3685", "close from JSON number");
    assert_eq!(resp.outblock1[0].volume, "321201", "volume from JSON number");

    let single: T1302Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1302OutBlock": { "cts_time": "" },
        "t1302OutBlock1": { "chetime": "102700", "close": 3685 }
    })).expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T1302Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1302OutBlock": { "cts_time": "" }, "t1302OutBlock1": []
    })).expect("empty 00707 deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case");
}

// === plan -004 batch B — t2216 F/O tick chart offline coverage ===============

/// t2216 — 선물옵션틱분별체결조회차트. bgubun/cnt numbers; single trade-row array
/// round-trips with a real value; empty 00707 recognized.
#[test]
fn t2216_request_and_response_round_trip() {
    let v = serde_json::to_value(T2216Request::new("A0669000", "T", "20")).expect("serialize t2216");
    let ib = &v["t2216InBlock"];
    assert!(ib["bgubun"].is_number(), "bgubun is a JSON number");
    assert!(ib["cnt"].is_number(), "cnt is a JSON number");
    assert_eq!(ib["focode"], "A0669000", "focode stays a string");

    let resp: T2216Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2216OutBlock1": [{ "chetime": "152000", "price": 41945, "volume": 12, "openyak": 678 }]
    })).expect("t2216 body round-trips");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].price, "41945", "price from JSON number");

    let single: T2216Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2216OutBlock1": { "chetime": "152000", "price": 41945 }
    })).expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T2216Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2216OutBlock1": []
    })).expect("empty 00707 deserializes");
    assert!(empty.outblock1.is_empty());
}

/// `t8427` request: `actprice` serializes as a JSON number (IGW40011 guard); the
/// rest stay strings; no out-block leaks.
#[test]
fn t8427_request_serializes_actprice_as_number() {
    let value = serde_json::to_value(T8427Request::new("A0669000", "2026", "09", "20260629"))
        .expect("serialize t8427");
    let ib = &value["t8427InBlock"];
    assert!(ib["actprice"].is_number(), "actprice is a JSON number");
    assert_eq!(ib["fo_gbn"], "F");
    assert_eq!(ib["focode"], "A0669000");
    assert_eq!(ib["dt_gbn"], "1", "daily default");
    assert!(value.get("t8427OutBlock1").is_none(), "no out-block leaks");
}

/// `t8427` response: OHLCV rows round-trip through dispatch with a real `close`;
/// number/string wire forms both parse; empty is the pending case.
#[tokio::test]
async fn t8427_deserializes_through_dispatch_and_empty() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8427"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rsp_cd":"00000","t8427OutBlock1":[{"date":"20260629","time":"100000","open":"345.1","high":"346","low":"344","close":345.55,"volume":12345,"openyak":"678"}]}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .market_session()
        .fo_minute_day_chart(&T8427Request::new("A0669000", "2026", "09", "20260629"))
        .await
        .expect("t8427 should succeed");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "345.55", "close via string_or_number (number wire)");

    let empty: T8427Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8427OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty chart is the pending case");
}

/// `t2210` request: `cvolume` serializes as a JSON number; window times stay strings.
#[test]
fn t2210_request_serializes_cvolume_as_number() {
    let value = serde_json::to_value(T2210Request::new("A0669000", "0900", "1530")).expect("serialize t2210");
    let ib = &value["t2210InBlock"];
    assert!(ib["cvolume"].is_number(), "cvolume is a JSON number");
    assert_eq!(ib["stime"], "0900");
    assert_eq!(ib["etime"], "1530");
}

/// `t2210` response: the buy/sell conclusion counts round-trip; a number-form
/// `msvolume` parses via string_or_number.
#[test]
fn t2210_conclusion_counts_round_trip() {
    let resp: T2210Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2210OutBlock": { "mdvolume": "120", "mdchecnt": "3", "msvolume": 95, "mschecnt": "2" }
    }))
    .expect("conclusion counts deserialize");
    assert_eq!(resp.outblock.msvolume, "95", "buy volume witness via string_or_number");
    assert_eq!(resp.outblock.mdvolume, "120");

    let empty: T2210Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2210OutBlock": {}
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.msvolume.is_empty(), "empty counts is the pending case");
}

/// `t2424` request: `nmin`/`cnt` serialize as JSON numbers; `focode` stays a string.
#[test]
fn t2424_request_serializes_nmin_and_cnt_as_numbers() {
    let value = serde_json::to_value(T2424Request::new("A0669000")).expect("serialize t2424");
    let ib = &value["t2424InBlock"];
    assert!(ib["nmin"].is_number(), "nmin is a JSON number");
    assert!(ib["cnt"].is_number(), "cnt is a JSON number");
    assert_eq!(ib["focode"], "A0669000");
}

/// `t2424` response: the header `price` + bar array round-trip; empty is pending.
#[tokio::test]
async fn t2424_deserializes_through_dispatch_and_empty() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t2424"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rsp_cd":"00000","t2424OutBlock":{"price":345.55,"volume":12345,"openyak":"678"},"t2424OutBlock1":[{"dt":"20260629100000","open":"345.1","high":"346","low":"344","close":"345.55"}]}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .market_session()
        .fo_minute_bars(&T2424Request::new("A0669000"))
        .await
        .expect("t2424 should succeed");
    assert_eq!(resp.outblock.price, "345.55", "price witness via string_or_number");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].close, "345.55");

    let empty: T2424Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2424OutBlock": {}, "t2424OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty bars is the pending case");
}

/// `t8428` request: `cnt` serializes as a JSON number; the date range stays strings.
#[test]
fn t8428_request_serializes_cnt_as_number() {
    let value = serde_json::to_value(T8428Request::new("20260601", "20260629", "001")).expect("serialize t8428");
    let ib = &value["t8428InBlock"];
    assert!(ib["cnt"].is_number(), "cnt is a JSON number");
    assert_eq!(ib["fdate"], "20260601");
    assert_eq!(ib["upcode"], "001");
    assert_eq!(ib["gubun"], "1");
}

/// `t8428` response: deposit-trend rows round-trip with a real `jisu`/`custmoney`;
/// single-or-array tolerated; empty is pending.
#[tokio::test]
async fn t8428_deserializes_through_dispatch_and_empty() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INVESTINFO_PATH))
        .and(header("tr_cd", "t8428"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rsp_cd":"00000","t8428OutBlock":{"date":"20260629","idx":1},"t8428OutBlock1":[{"date":"20260627","jisu":"2610.62","sign":"2","change":"-3.1","volume":263165,"custmoney":"550000","yecha":"100"}]}"#,
        ).insert_header("content-type", "application/json"))
        .mount(&server)
        .await;
    let resp = sdk_for(&server)
        .market_session()
        .deposit_balance_trend(&T8428Request::new("20260601", "20260629", "001"))
        .await
        .expect("t8428 should succeed");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].jisu, "2610.62", "index witness round-trips");
    assert_eq!(resp.outblock1[0].custmoney, "550000", "customer-deposit witness");

    let single: T8428Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8428OutBlock1": { "date": "20260627", "jisu": 2610.62, "custmoney": 550000 }
    }))
    .expect("single tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T8428Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8428OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty trend is the pending case");
}
