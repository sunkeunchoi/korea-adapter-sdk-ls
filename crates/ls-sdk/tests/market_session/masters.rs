use super::*;


// ---------------------------------------------------------------------------
// t8425 — 전체테마 (all-themes) read. Third TR in the market_session class and
// the implement-tr pilot: non-paginated, NO caller input, an array out-block.
// ---------------------------------------------------------------------------

/// Covers R5. The `t8425` request serializes to exactly `{"t8425InBlock":{...}}`
/// with only the `dummy` placeholder — no caller-supplied fields leak, and no
/// `tr_cont`/`tr_cont_key` (t8425 is not paginated).
#[test]
fn t8425_request_serializes_to_inblock_with_only_dummy() {
    let req = T8425Request::new();
    let value = serde_json::to_value(&req).expect("serialize t8425 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t8425InBlock"), "missing t8425InBlock key");

    let inblock = &value["t8425InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t8425InBlock carries only the dummy field");
    assert_eq!(inblock["dummy"], "", "dummy is an empty placeholder (no caller input)");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers R2, R5. The spec-derived fixture deserializes through REAL dispatch:
/// the all-themes array round-trips, a real (non-default) `tmname`/`tmcode` is
/// populated, and `tmcode` arriving as a JSON number (`1234`) still parses via
/// `string_or_number` — proving the representative subset round-trips, not just
/// that `serde(default)` returned `Ok`.
#[tokio::test]
async fn all_themes_deserializes_spec_fixture_with_real_values() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T8425_PATH))
        .and(header("tr_cd", "t8425"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8425_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .all_themes(&T8425Request::new())
        .await
        .expect("t8425 all_themes should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 3, "all three theme rows round-trip");
    assert_eq!(resp.outblock[0].tmname, "2차전지", "real non-default tmname");
    assert_eq!(resp.outblock[0].tmcode, "0050", "tmcode (was JSON string)");
    assert_eq!(
        resp.outblock[1].tmcode, "1234",
        "tmcode coerced from a JSON number"
    );
}

/// Covers R2, R5. `tmcode` deserializes whether it arrives as a JSON string or a
/// JSON number — the `string_or_number` round-trip guarantee, proven directly.
#[test]
fn t8425_tmcode_number_or_string_yields_same_value() {
    let as_number: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8425OutBlock": [{ "tmname": "반도체", "tmcode": 1234 }]
    }))
    .expect("number tmcode must deserialize");
    let as_string: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8425OutBlock": [{ "tmname": "반도체", "tmcode": "1234" }]
    }))
    .expect("string tmcode must deserialize");
    assert_eq!(as_number.outblock[0].tmcode, "1234");
    assert_eq!(as_number.outblock[0].tmcode, as_string.outblock[0].tmcode);
}

/// Covers R2. A single out-block object (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single` — the gateway collapses a one-row
/// result to a bare object.
#[test]
fn t8425_single_out_row_tolerated_as_array() {
    let single: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8425OutBlock": { "tmname": "단일", "tmcode": "0001" }
    }))
    .expect("single out-block object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].tmcode, "0001");
}

/// Covers R2. An empty result set (`rsp_cd 00707`, empty out-block array)
/// deserializes without error and is recognized as the empty/pending case — the
/// implement-tr gate records this as PENDING (callable but shape-unconfirmed),
/// never a flip.
#[test]
fn t8425_empty_result_set_deserializes_as_empty() {
    let empty: T8425Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t8425OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.is_empty(),
        "an empty out-block is the pending case, not a flip"
    );
}

/// Compile-time guard: `T8425Response` default envelope is empty.
#[test]
fn t8425_response_envelope_default_is_empty() {
    let resp = T8425Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t8436 — 주식종목조회 (stock master list). market_session, non-paginated, takes
// a `gubun` market-segment filter; array out-block.
// ---------------------------------------------------------------------------

/// Covers R5. The `t8436` request serializes to exactly `{"t8436InBlock":{...}}`
/// with only the `gubun` filter — no continuation fields.
#[test]
fn t8436_request_serializes_to_inblock_with_only_gubun() {
    let req = T8436Request::new("0");
    let value = serde_json::to_value(&req).expect("serialize t8436 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    let inblock = &value["t8436InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t8436InBlock carries only gubun");
    assert_eq!(inblock["gubun"], "0");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5. The spec-derived fixture deserializes through REAL dispatch:
/// the stock-master array round-trips with real `hname`/`shcode` values, and
/// numeric fields arriving as JSON numbers (row 0) or strings (row 1) both parse
/// via `string_or_number`.
#[tokio::test]
async fn stock_list_deserializes_spec_fixture_with_real_values() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T8436_PATH))
        .and(header("tr_cd", "t8436"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8436_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .stock_list(&T8436Request::new("0"))
        .await
        .expect("t8436 stock_list should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "both stock rows round-trip");
    assert_eq!(resp.outblock[0].hname, "삼성전자", "real non-default hname");
    assert_eq!(resp.outblock[0].shcode, "005930");
    assert_eq!(
        resp.outblock[0].uplmtprice, "92900",
        "uplmtprice coerced from a JSON number"
    );
    assert_eq!(
        resp.outblock[1].uplmtprice, "300000",
        "uplmtprice parsed from a JSON string"
    );
}

/// Covers R2. An empty result set (`00707`, empty array) deserializes and is
/// recognized as the empty/pending case.
#[test]
fn t8436_empty_result_set_deserializes_as_empty() {
    let empty: T8436Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t8436OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty());
}

/// Covers R2. A single out-block object (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single` (the gateway collapses a one-row
/// result to a bare object).
#[test]
fn t8436_single_out_row_tolerated_as_array() {
    let single: T8436Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8436OutBlock": { "hname": "단일", "shcode": "000660" }
    }))
    .expect("single out-block object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].shcode, "000660");
}

/// Compile-time guard: `T8436Response` default envelope is empty.
#[test]
fn t8436_response_envelope_default_is_empty() {
    let resp = T8436Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t1531 — 테마별종목 (stocks in a theme). market_session, non-paginated; keyed by
// a required tmname+tmcode pair (AE4 caller-supplied identifiers).
// ---------------------------------------------------------------------------

/// Covers R5, AE4. The `t1531` request serializes to `{"t1531InBlock":{...}}`
/// carrying BOTH required identifiers `tmname` and `tmcode` in the correct block.
#[test]
fn t1531_request_serializes_with_tmname_and_tmcode() {
    let req = T1531Request::new("2차전지", "0050");
    let value = serde_json::to_value(&req).expect("serialize t1531 request");

    let inblock = &value["t1531InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 2, "tmname + tmcode");
    assert_eq!(inblock["tmname"], "2차전지");
    assert_eq!(inblock["tmcode"], "0050");
    assert!(value.get("tr_cont").is_none());
}

/// Covers R2. The fixture deserializes through REAL dispatch; rows round-trip and
/// `tmcode`/`avgdiff` parse whether they arrive as JSON strings (row 0) or
/// numbers (row 1).
#[tokio::test]
async fn theme_stocks_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(SECTOR_PATH))
        .and(header("tr_cd", "t1531"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1531_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .theme_stocks(&T1531Request::new("2차전지", "0050"))
        .await
        .expect("t1531 theme_stocks should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].tmcode, "0050", "tmcode (string form)");
    assert_eq!(resp.outblock[1].tmcode, "50", "tmcode coerced from number");
    assert_eq!(resp.outblock[0].avgdiff, "1.23");
}

/// Covers R2. An empty result set (`00707`) deserializes as the pending case.
#[test]
fn t1531_empty_result_set_deserializes_as_empty() {
    let empty: T1531Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1531OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty());
}

/// Covers R2. A single `t1531` out-block object is tolerated as a one-element Vec
/// via `de_vec_or_single`.
#[test]
fn t1531_single_out_row_tolerated_as_array() {
    let single: T1531Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1531OutBlock": { "tmname": "단일", "tmcode": "0001" }
    }))
    .expect("single out-block object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].tmcode, "0001");
}

// ---------------------------------------------------------------------------
// Wave 1 — ELW universe/list reads (t9905, t9907, t8431, t9942). No-caller-input
// `dummy` reads; each returns a code-keyed list. Covers AE1.
// ---------------------------------------------------------------------------

/// Covers AE1. `t9905` request serializes only `dummy`; a representative success
/// deserializes with the underlying-asset `shcode` (the `t1964` `item` source)
/// populated, single-or-array tolerated.
#[test]
fn t9905_request_and_response_round_trip() {
    let value = serde_json::to_value(T9905Request::new()).expect("serialize t9905");
    assert_eq!(value["t9905InBlock"]["dummy"], "");

    let resp: T9905Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9905OutBlock1": [
            { "shcode": "005930", "expcode": "KR7005930003", "hname": "삼성전자" },
            { "shcode": 660, "expcode": "KR7000660001", "hname": "SK하이닉스" }
        ]
    }))
    .expect("representative t9905 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "005930", "underlying code populated");
    assert_eq!(resp.outblock1[1].shcode, "660", "shcode from JSON number");

    let single: T9905Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9905OutBlock1": { "shcode": "005930", "hname": "삼성전자" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T9905Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9905OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}

/// Covers AE1. `T9905OutBlock1.shcode` parses from JSON number or string alike.
#[test]
fn t9905_shcode_number_or_string_yields_same_value() {
    let n: T9905OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": 5930 })).expect("number");
    let s: T9905OutBlock1 =
        serde_json::from_value(serde_json::json!({ "shcode": "5930" })).expect("string");
    assert_eq!(n.shcode, "5930");
    assert_eq!(n.shcode, s.shcode);
}

/// Covers AE1. `t8430` stock-issue list round-trips; `gubun` request is a plain
/// code string ("0" all); numeric-bearing fields parse number-or-string;
/// single-or-array tolerated; empty `00707` is the pending case.
#[test]
fn t8430_request_and_response_round_trip() {
    let value = serde_json::to_value(T8430Request::all()).expect("serialize t8430");
    assert_eq!(value["t8430InBlock"]["gubun"], "0", "all-markets code string");

    let resp: T8430Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8430OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfgubun": "0", "uplmtprice": 91900, "dnlmtprice": "49500",
              "jnilclose": 70700, "memedan": "1", "recprice": 70700, "gubun": "1" },
            { "hname": "에코프로", "shcode": "086520", "expcode": "KR7086520004",
              "etfgubun": "0", "uplmtprice": "120000", "dnlmtprice": 64600,
              "jnilclose": "92300", "memedan": "1", "recprice": "92300", "gubun": "2" }
        ]
    }))
    .expect("representative t8430 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].shcode, "005930", "shcode populated");
    assert_eq!(
        resp.outblock[0].uplmtprice, "91900",
        "uplmtprice from JSON number"
    );
    assert_eq!(
        resp.outblock[1].uplmtprice, "120000",
        "uplmtprice from JSON string"
    );
    assert_eq!(resp.outblock[1].gubun, "2", "KOSDAQ market flag");

    let single: T8430Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8430OutBlock": { "shcode": "005930", "hname": "삼성전자" }
    }))
    .expect("single row tolerated as array");
    assert_eq!(single.outblock.len(), 1);

    let empty: T8430Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8430OutBlock": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock.is_empty(), "empty is the pending case");
}

/// Covers AE1. `T8430OutBlock` numeric-bearing fields parse number-or-string alike.
#[test]
fn t8430_price_number_or_string_yields_same_value() {
    let n: T8430OutBlock =
        serde_json::from_value(serde_json::json!({ "uplmtprice": 91900 })).expect("number");
    let s: T8430OutBlock =
        serde_json::from_value(serde_json::json!({ "uplmtprice": "91900" })).expect("string");
    assert_eq!(n.uplmtprice, "91900");
    assert_eq!(n.uplmtprice, s.uplmtprice);
}

/// Covers R4. `t2522` serializes to exactly `{"t2522InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t2522_request_serializes_to_inblock() {
    let value = serde_json::to_value(T2522Request::new()).expect("serialize t2522 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t2522InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t2522InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the count header round-trips and the canonical identity field
/// `bsc_asts_nm` (기초자산명, underlying-asset name) — which lives in the
/// `t2522OutBlock1` row array, not the count header — holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `bsc_asts_is_cd`/`nmc_is_shrt_cd` instead would surface here.
#[tokio::test]
async fn t2522_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t2522"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T2522_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_futures_underlying_master(&T2522Request::new())
        .await
        .expect("t2522 stock_futures_underlying_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.cnt, "2", "건수 count header (was a JSON number)");
    assert_eq!(resp.outblock1.len(), 2, "two underlying-asset rows");
    let row = &resp.outblock1[0];
    // The canonical identity field, by Korean name 기초자산명 — exact value.
    assert_eq!(
        row.bsc_asts_nm, "삼성전자",
        "기초자산명 underlying-asset name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto bsc_asts_nm.
    assert_eq!(row.bsc_asts_is_cd, "005930", "기초자산종목코드 (distinct)");
    assert_eq!(row.bsc_asts_id, "KR7", "기초자산ID (distinct)");
    assert_eq!(row.nmc_is_shrt_cd, "111W9000", "최근월물종목코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock1[1].bsc_asts_nm, "SK하이닉스", "second row distinct");
}

/// Covers R4, R5. The numeric fields tolerate a JSON number or string via
/// `string_or_number` (the gateway sends `cnt` as an integer).
#[test]
fn t2522_numeric_number_or_string_yields_same_value() {
    let as_number: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2522OutBlock": { "cnt": 42 },
        "t2522OutBlock1": [{ "bsc_asts_nm": "삼성전자" }]
    }))
    .expect("number cnt must deserialize");
    let as_string: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2522OutBlock": { "cnt": "42" },
        "t2522OutBlock1": [{ "bsc_asts_nm": "삼성전자" }]
    }))
    .expect("string cnt must deserialize");
    assert_eq!(as_number.outblock.cnt, "42");
    assert_eq!(as_number.outblock.cnt, as_string.outblock.cnt);
    assert_eq!(
        as_number.outblock1[0].bsc_asts_nm, as_string.outblock1[0].bsc_asts_nm,
        "bsc_asts_nm both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t2522OutBlock1` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t2522_single_object_row_deserializes_to_one_element_vec() {
    let single: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t2522OutBlock": { "cnt": 1 },
        "t2522OutBlock1": { "bsc_asts_nm": "삼성전자" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].bsc_asts_nm, "삼성전자");
    // The standalone row struct also default-constructs cleanly.
    assert!(T2522OutBlock1::default().bsc_asts_nm.is_empty());
}

/// Covers R5. An empty `t2522` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t2522_empty_result_deserializes_as_pending() {
    let empty: T2522Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2522OutBlock": {}
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock1.is_empty(), "empty master is the pending case");
}

/// Covers R4. `t8435` serializes to exactly `{"t8435InBlock":{"gubun":"MF"}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t8435_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8435Request::new("MF")).expect("serialize t8435 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8435InBlock"]["gubun"], "MF", "gubun selector serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the derivatives contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`recprice` instead would surface here.
#[tokio::test]
async fn t8435_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8435"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8435_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .derivatives_master(&T8435Request::new("MF"))
        .await
        .expect("t8435 derivatives_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two derivatives rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "KQF 2306",
        "종목명 derivatives contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "106T6000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4106T60005", "확장코드 (distinct)");
    assert_eq!(row.uplmtprice, "1456.5", "상한가 (distinct)");
    assert_eq!(row.dnlmtprice, "1240.9", "하한가 (distinct)");
    assert_eq!(row.jnilclose, "1348.7", "전일종가 (distinct)");
    assert_eq!(row.jnilhigh, "1349.8", "전일고가 (distinct)");
    assert_eq!(row.jnillow, "1323.9", "전일저가 (distinct)");
    // recprice (기준가) is distinct from jnilclose — a 기준가/전일종가 mislabel surfaces.
    assert_eq!(row.recprice, "1348.6", "기준가 (distinct from 전일종가)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "KQF 2309", "second row distinct");
}

/// Covers shared contract item 2. `uplmtprice` (상한가) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// types it `Number` and may send either form.
#[test]
fn t8435_numeric_number_or_string_yields_same_value() {
    let as_number: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8435OutBlock": [{ "hname": "KQF 2306", "uplmtprice": 1456.5 }]
    }))
    .expect("numeric uplmtprice deserializes");
    let as_string: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8435OutBlock": [{ "hname": "KQF 2306", "uplmtprice": "1456.5" }]
    }))
    .expect("string uplmtprice deserializes");
    assert_eq!(as_number.outblock[0].uplmtprice, "1456.5");
    assert_eq!(as_string.outblock[0].uplmtprice, "1456.5");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8435OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8435_single_object_row_deserializes_to_one_element_vec() {
    let single: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8435OutBlock": { "hname": "KQF 2306" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "KQF 2306");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8435OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8435` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8435_empty_result_deserializes_as_pending() {
    let empty: T8435Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

/// Covers R4. `t8467` serializes to exactly `{"t8467InBlock":{"gubun":"Q"}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t8467_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8467Request::new("Q")).expect("serialize t8467 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8467InBlock"]["gubun"], "Q", "gubun selector serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the index-futures contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`recprice`/`jnilclose` instead would surface here.
#[tokio::test]
async fn t8467_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8467"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8467_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index_futures_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two index-futures rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "F 2606",
        "종목명 index-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "A0166000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4A01660005", "확장코드 (distinct)");
    assert_eq!(row.uplmtprice, "1214.75", "상한가 (distinct)");
    assert_eq!(row.dnlmtprice, "1034.85", "하한가 (distinct)");
    assert_eq!(row.jnilclose, "1124.80", "전일종가 (distinct)");
    assert_eq!(row.jnilhigh, "1125.65", "전일고가 (distinct)");
    assert_eq!(row.jnillow, "1124.55", "전일저가 (distinct)");
    // recprice (기준가) is distinct from jnilclose — a 기준가/전일종가 mislabel surfaces.
    assert_eq!(row.recprice, "1124.70", "기준가 (distinct from 전일종가)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "F 2609", "second row distinct");
}

/// Covers shared contract item 2. `uplmtprice` (상한가) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// types it `Number` and may send either form.
#[test]
fn t8467_numeric_number_or_string_yields_same_value() {
    let as_number: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8467OutBlock": [{ "hname": "F 2606", "uplmtprice": 1214.75 }]
    }))
    .expect("numeric uplmtprice deserializes");
    let as_string: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8467OutBlock": [{ "hname": "F 2606", "uplmtprice": "1214.75" }]
    }))
    .expect("string uplmtprice deserializes");
    assert_eq!(as_number.outblock[0].uplmtprice, "1214.75");
    assert_eq!(as_string.outblock[0].uplmtprice, "1214.75");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8467OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8467_single_object_row_deserializes_to_one_element_vec() {
    let single: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8467OutBlock": { "hname": "F 2606" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "F 2606");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8467OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8467` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8467_empty_result_deserializes_as_pending() {
    let empty: T8467Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

/// Covers R4. `t9943` serializes to exactly `{"t9943InBlock":{"gubun":"V"}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t9943_request_serializes_to_inblock() {
    let value = serde_json::to_value(T9943Request::new("V")).expect("serialize t9943 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t9943InBlock"]["gubun"], "V", "gubun selector serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the index-futures contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode` instead would surface here.
#[tokio::test]
async fn t9943_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t9943"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T9943_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_futures_master_codes(&T9943Request::new("V"))
        .await
        .expect("t9943 index_futures_master_codes should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two index-futures rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "VF 2306",
        "종목명 index-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "104T6000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4104T60000", "확장코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "VF 2307", "second row distinct");
    assert_eq!(resp.outblock[1].shcode, "104T7000", "second row 단축코드 distinct");
}

/// Covers shared contract item 2. `shcode` (단축코드) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// may send a code field as either form.
#[test]
fn t9943_numeric_number_or_string_yields_same_value() {
    let as_number: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9943OutBlock": [{ "hname": "VF 2306", "shcode": 1046000 }]
    }))
    .expect("numeric shcode deserializes");
    let as_string: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9943OutBlock": [{ "hname": "VF 2306", "shcode": "1046000" }]
    }))
    .expect("string shcode deserializes");
    assert_eq!(as_number.outblock[0].shcode, "1046000");
    assert_eq!(as_string.outblock[0].shcode, "1046000");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t9943OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t9943_single_object_row_deserializes_to_one_element_vec() {
    let single: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9943OutBlock": { "hname": "VF 2306" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "VF 2306");
    // The standalone row struct also default-constructs cleanly.
    assert!(T9943OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t9943` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t9943_empty_result_deserializes_as_pending() {
    let empty: T9943Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

/// Covers R4. `t9944` serializes to exactly `{"t9944InBlock":{"dummy":""}}`
/// with no continuation tokens (non-paginated) and no caller fields leaking.
#[test]
fn t9944_request_serializes_to_inblock() {
    let value = serde_json::to_value(T9944Request::new()).expect("serialize t9944 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t9944InBlock"]["dummy"], "", "dummy placeholder serialized");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the master row array round-trips and the canonical identity field
/// `hname` (종목명, the index-option contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode` instead would surface here.
#[tokio::test]
async fn t9944_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t9944"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T9944_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_option_master_codes(&T9944Request::new())
        .await
        .expect("t9944 index_option_master_codes should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "fixture carries two index-option rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "C 2306 160.0",
        "종목명 index-option contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "201T6160", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4201T61606", "확장코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "C 2306 162.5", "second row distinct");
    assert_eq!(resp.outblock[1].shcode, "201T6162", "second row 단축코드 distinct");
}

/// Covers shared contract item 2. `shcode` (단축코드) parses via
/// `ls_core::string_or_number` from BOTH a JSON number and a string — the gateway
/// may send a code field as either form.
#[test]
fn t9944_numeric_number_or_string_yields_same_value() {
    let as_number: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9944OutBlock": [{ "hname": "C 2306 160.0", "shcode": 2016160 }]
    }))
    .expect("numeric shcode deserializes");
    let as_string: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9944OutBlock": [{ "hname": "C 2306 160.0", "shcode": "2016160" }]
    }))
    .expect("string shcode deserializes");
    assert_eq!(as_number.outblock[0].shcode, "2016160");
    assert_eq!(as_string.outblock[0].shcode, "2016160");
    assert_eq!(
        as_number.outblock[0].hname, as_string.outblock[0].hname,
        "hname both forms"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t9944OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t9944_single_object_row_deserializes_to_one_element_vec() {
    let single: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9944OutBlock": { "hname": "C 2306 160.0" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "C 2306 160.0");
    // The standalone row struct also default-constructs cleanly.
    assert!(T9944OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t9944` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t9944_empty_result_deserializes_as_pending() {
    let empty: T9944Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

// ---------------------------------------------------------------------------
// Night-derivatives lane (reach wave U6), routed through `market_session` (KTD3).
// `venue_session: krx_extended` — the night session (~18:00–05:00 KST), NOT the
// regular clock (KTD7): an off-window empty result is NOT a valid attempt, so it
// is a re-run-in-window disposition (not a flip, not a DROP). Out-block shape
// from the raw capture (KTD5): t8455 master is an ARRAY (A0005); t8460 carries a
// single near-month header (A0003) + call/put option ARRAYS (A0005); t8463
// carries a single investor-code header (A0003) + a time-series row ARRAY
// (A0005). Canonical field by baseline `korean_name` (KTD6); t8463's `cnt`
// request field serializes as a JSON number (KTD4).
// ---------------------------------------------------------------------------

/// `t8455` request rename + ARRAY master out-block round-trips; a single row
/// collapses to a one-element Vec via `de_vec_or_single` (KTD5). Canonical 종목명
/// (`hname`) pinned exactly (KTD6).
#[test]
fn t8455_request_renames_and_master_array_round_trips() {
    let value = serde_json::to_value(T8455Request::new("NF")).expect("serialize t8455");
    assert_eq!(value["t8455InBlock"]["gubun"], "NF");
    assert_eq!(value.as_object().expect("object").len(), 1, "exactly one top-level key");

    let resp: T8455Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8455OutBlock": [
            { "hname": "야간 F 202509", "shcode": "111VC000", "expcode": "KR4111VC0001", "tradeunit": 250000 },
            { "hname": "야간 F 202512", "shcode": "111VF000", "expcode": "KR4111VF0008", "tradeunit": "250000" }
        ]
    }))
    .expect("representative t8455 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    assert_eq!(resp.outblock[0].hname, "야간 F 202509", "종목명");
    assert_eq!(resp.outblock[1].shcode, "111VF000", "종목코드");
    assert_eq!(resp.outblock[0].tradeunit, "250000", "거래승수 from number");

    // single row object → one-element Vec (KTD5 single-or-array).
    let single: T8455Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8455OutBlock": { "hname": "야간 F 202509", "shcode": "111VC000" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "야간 F 202509");
}

/// `t8455` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t8455_numeric_field_string_or_number() {
    let from_num: T8455OutBlock =
        serde_json::from_value(serde_json::json!({ "tradeunit": 250000 }))
            .expect("number form deserializes");
    let from_str: T8455OutBlock =
        serde_json::from_value(serde_json::json!({ "tradeunit": "250000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.tradeunit, "250000");
    assert_eq!(from_str.tradeunit, "250000");
}

/// `t8455` off-window empty (`00707`, empty array) deserializes — the night
/// session (~18:00–05:00 KST) is closed (KTD7), so this is a RE-RUN-IN-WINDOW
/// disposition (NOT a flip, NOT a DROP), recognized as the empty/pending case.
#[test]
fn t8455_off_window_empty_is_rerun_disposition_not_flip_not_drop() {
    let empty: T8455Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8455OutBlock": []
    }))
    .expect("off-window empty still deserializes (the night window is closed)");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.is_empty(),
        "an empty master array off the night window is the re-run case, not Implemented"
    );
}

/// `t8460` request rename + single header + call/put option ARRAYS round-trip; a
/// single option row collapses to a one-element Vec via `de_vec_or_single`
/// (KTD5). Canonical 근월물현재가 (`gmprice`) pinned exactly (KTD6).
#[test]
fn t8460_request_renames_and_header_plus_option_arrays_round_trip() {
    let value = serde_json::to_value(T8460Request::new("202509", "G")).expect("serialize t8460");
    assert_eq!(value["t8460InBlock"]["yyyymm"], "202509");
    assert_eq!(value["t8460InBlock"]["gubun"], "G");

    let resp: T8460Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8460OutBlock": { "gmprice": 350.25, "gmchange": "1.50", "gmvolume": 12345, "gmshcode": "111VC000" },
        "t8460OutBlock1": [
            { "actprice": 350.0, "optcode": "201VC350", "price": 4.55, "offerho1": 4.60, "bidho1": 4.50 },
            { "actprice": "352.5", "optcode": "201VC352", "price": "3.10", "offerho1": "3.15", "bidho1": "3.05" }
        ],
        "t8460OutBlock2": [
            { "actprice": 350.0, "optcode": "301VC350", "price": 3.20, "offerho1": 3.25, "bidho1": 3.15 }
        ]
    }))
    .expect("representative t8460 success must deserialize");
    assert_eq!(resp.outblock.gmprice, "350.25", "근월물현재가");
    assert_eq!(resp.outblock.gmshcode, "111VC000", "근월물선물코드");
    assert_eq!(resp.outblock1.len(), 2, "call-option array");
    assert_eq!(resp.outblock1[0].optcode, "201VC350", "콜옵션코드");
    assert_eq!(resp.outblock1[1].price, "3.10", "price from string preserved verbatim");
    assert_eq!(resp.outblock2.len(), 1, "put-option array");
    assert_eq!(resp.outblock2[0].optcode, "301VC350", "풋옵션코드");

    // single call-option row → one-element Vec (KTD5).
    let single: T8460Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8460OutBlock1": { "actprice": 350.0, "optcode": "201VC350", "price": 4.55 }
    }))
    .expect("single option row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
}

/// `t8460` off-window empty (`00707`, empty arrays) deserializes — the night
/// window is closed (KTD7), so this is a RE-RUN-IN-WINDOW disposition (NOT a
/// flip, NOT a DROP).
#[test]
fn t8460_off_window_empty_is_rerun_disposition_not_flip_not_drop() {
    let empty: T8460Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8460OutBlock1": [], "t8460OutBlock2": []
    }))
    .expect("off-window empty still deserializes (the night window is closed)");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.gmprice.is_empty(), "empty header is the re-run case");
    assert!(empty.outblock1.is_empty() && empty.outblock2.is_empty(), "empty boards");
}

/// `g3104` rename + canonical 한글종목명 (`korname`, KTD6) pinned exactly.
#[test]
fn g3104_request_renames_and_korname_round_trips() {
    let value = serde_json::to_value(G3104Request::new("R", "82TSLA", "82", "TSLA"))
        .expect("serialize g3104");
    assert_eq!(value["g3104InBlock"]["symbol"], "TSLA");
    assert!(value.get("g3104OutBlock").is_none(), "no out-block leaks");

    let resp: G3104Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3104OutBlock": {
            "korname": "테슬라", "engname": "TESLA INC", "symbol": "TSLA",
            "exchange_name": "나스닥", "nation_name": "미국", "currency": "USD",
            "share": 3216520000i64, "pcls": "284.9500"
        }
    }))
    .expect("representative g3104 success must deserialize");
    assert_eq!(resp.outblock.korname, "테슬라", "한글종목명");
    assert_eq!(resp.outblock.engname, "TESLA INC", "영문종목명");
    assert_ne!(resp.outblock.korname, resp.outblock.engname, "non-collapsing: kor≠eng");
}

/// `g3104` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn g3104_numeric_field_string_or_number() {
    let from_num: G3104OutBlock =
        serde_json::from_value(serde_json::json!({ "share": 3216520000i64 })).expect("number");
    let from_str: G3104OutBlock =
        serde_json::from_value(serde_json::json!({ "share": "3216520000" })).expect("string");
    assert_eq!(from_num.share, "3216520000");
    assert_eq!(from_str.share, "3216520000");
}

/// `g3104` empty result (00707) deserializes as the pending case.
#[test]
fn g3104_empty_result_deserializes_as_pending() {
    let empty: G3104Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.korname.is_empty(), "empty master is the pending case");
}

/// `g3190` numeric request field serializes as a JSON NUMBER (KTD4); header +
/// master Object-Array round-trips; canonical 한글종목명 (`korname`) pinned exactly
/// (KTD6); single → Vec (KTD5).
#[test]
fn g3190_request_serializes_count_as_number_and_array_round_trips() {
    let value = serde_json::to_value(G3190Request::new("R", "US", "2", "10", ""))
        .expect("serialize g3190");
    assert!(
        value["g3190InBlock"]["readcnt"].is_number(),
        "readcnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["g3190InBlock"]["readcnt"], 10);
    assert_eq!(value["g3190InBlock"]["natcode"], "US");
    // cts_value is a genuine string token (first page = "").
    assert!(value["g3190InBlock"]["cts_value"].is_string());

    let resp: G3190Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3190OutBlock": { "natcode": "US", "cts_value": "0000000000000011", "rec_count": 10 },
        "g3190OutBlock1": [
            { "keysymbol": "82AACB", "symbol": "AACB", "korname": "ARTIUS II ACQUISITION INC", "engname": "ARTIUS II ACQUISITION INC", "currency": "USD", "pcls": "9.9200" },
            { "keysymbol": "82AACG", "symbol": "AACG", "korname": "ATA 크리에티비티 글로벌(ADR)", "engname": "ATA CREATIVITY GLOBAL", "currency": "USD", "pcls": "0.9050" }
        ]
    }))
    .expect("representative g3190 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[1].korname, "ATA 크리에티비티 글로벌(ADR)", "한글종목명");
    assert_eq!(resp.outblock.rec_count, "10", "레코드카운트");

    let single: G3190Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3190OutBlock": { "natcode": "US", "rec_count": "1" },
        "g3190OutBlock1": { "keysymbol": "82AACB", "symbol": "AACB", "korname": "ARTIUS II ACQUISITION INC" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: G3190Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "g3190OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty master array is the pending case");
}

// ---------------------------------------------------------------------------
// Overseas-futures (`o`-prefix) reads — U8 reach wave.
//
// Surface `/overseas-futureoption/market-data`, group `[해외선물] 시세`,
// instrument_domain overseas_futures, venue_session unspecified. One `o`-probe +
// KTD9 A/B (wrong path → http=404, wrong tr_cd → http=500 IGW00215, intended →
// http=200; NO 01900) confirmed the domain REACHABLE and our contract CORRECT.
// The two MASTER reads (o3101/o3121) return non-empty data on paper → IMPLEMENT;
// the four live quote/order-book reads (o3105/o3106/o3125/o3126) answer empty on
// paper → PENDING. Canonical fields by baseline `korean_name`, pinned exactly
// from a NON-COLLAPSING fixture (KTD6); numeric out fields via `string_or_number`
// from BOTH string and number JSON (KTD4); array out-blocks single→one-element
// Vec via `de_vec_or_single` (KTD5). The `01900` disposition is covered
// explicitly on o3101 (representative). No numeric REQUEST fields in this lane.
// ---------------------------------------------------------------------------

/// `o3101` request rename (no caller leak) + a NON-COLLAPSING master row array:
/// `symbol_nm` (종목명, canonical KTD6) pinned exactly and distinct from the
/// base-product name so a mislabel cannot hide; the ARRAY out-block round-trips
/// (KTD5).
#[test]
fn o3101_request_renames_and_master_array_round_trips() {
    let value = serde_json::to_value(O3101Request::new("")).expect("serialize o3101");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["o3101InBlock"]["gubun"], "");
    assert!(value.get("o3101OutBlock").is_none(), "no out-block leaks into the request");

    let resp: O3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3101OutBlock": [
            { "Symbol": "ADM23", "SymbolNm": "Australian Dollar(2023.06)", "BscGdsCd": "AD",
              "BscGdsNm": "Australian Dollar", "ExchCd": "CME", "CrncyCd": "USD",
              "UntPrc": "0.000050000", "DotGb": 5 },
            { "Symbol": "M6EZ23", "SymbolNm": "E-micro EUR/USD(2023.12)", "BscGdsCd": "M6E",
              "BscGdsNm": "E-micro EUR/USD", "ExchCd": "CME", "CrncyCd": "USD",
              "UntPrc": "0.000100000", "DotGb": 5 }
        ]
    }))
    .expect("representative o3101 success must deserialize");
    assert_eq!(resp.outblock.len(), 2);
    // Canonical 종목명 pinned exactly, distinct from 기초상품명 (KTD6).
    assert_eq!(resp.outblock[0].symbol_nm, "Australian Dollar(2023.06)", "종목명");
    assert_eq!(resp.outblock[0].bsc_gds_nm, "Australian Dollar", "기초상품명");
    assert_ne!(
        resp.outblock[0].symbol_nm, resp.outblock[0].bsc_gds_nm,
        "non-collapsing: 종목명≠기초상품명"
    );

    // single row object → one-element Vec (KTD5).
    let single: O3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3101OutBlock": { "Symbol": "ADM23", "SymbolNm": "Australian Dollar(2023.06)" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
}

/// `o3101` numeric out-block field (`DotGb`/`dot_gb`) parses from BOTH string and
/// number JSON (KTD4).
#[test]
fn o3101_numeric_field_string_or_number() {
    let from_num: O3101OutBlock =
        serde_json::from_value(serde_json::json!({ "DotGb": 5 })).expect("number form");
    let from_str: O3101OutBlock =
        serde_json::from_value(serde_json::json!({ "DotGb": "5" })).expect("string form");
    assert_eq!(from_num.dot_gb, "5");
    assert_eq!(from_str.dot_gb, "5");
}

/// `o3101` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn o3101_empty_result_deserializes_as_pending() {
    let empty: O3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3101OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty master array is the pending case");
}

/// `o3101` `01900` classifies as paper-incompatible — the member stays Tracked,
/// no flip (disposition state machine). Representative for the lane.
#[tokio::test]
async fn o3101_code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-futureoption/market-data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01900",
            "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let err = sdk
        .market_session()
        .overseas_futures_master(&O3101Request::new(""))
        .await
        .expect_err("01900 must surface as an error");
    match &err {
        LsError::ApiError { code, .. } => {
            assert_eq!(code, "01900", "exact code preserved");
            assert!(ls_core::is_paper_incompatible(code), "01900 paper-incompatible");
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

/// `o3121` rename (no caller leak) + a NON-COLLAPSING option-master row array:
/// `bsc_gds_nm` (기초상품명, canonical KTD6) pinned exactly; ARRAY out-block
/// round-trips (KTD5).
#[test]
fn o3121_request_renames_and_master_array_round_trips() {
    let value = serde_json::to_value(O3121Request::new("O", "")).expect("serialize o3121");
    assert_eq!(value["o3121InBlock"]["MktGb"], "O");
    assert_eq!(value["o3121InBlock"]["BscGdsCd"], "");
    assert!(value.get("o3121OutBlock").is_none(), "no out-block leaks");

    let resp: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3121OutBlock": [
            { "Symbol": "", "BscGdsCd": "O_E1A", "BscGdsNm": "W1 Monday E-mini S&P 500 Option",
              "ExchCd": "CME", "XrcPrc": "", "OptTpCode": "", "DotGb": 0 }
        ]
    }))
    .expect("representative o3121 success must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].bsc_gds_nm, "W1 Monday E-mini S&P 500 Option", "기초상품명");
    assert_eq!(resp.outblock[0].exch_cd, "CME", "거래소코드");
    assert_ne!(
        resp.outblock[0].bsc_gds_nm, resp.outblock[0].exch_cd,
        "non-collapsing: 기초상품명≠거래소코드"
    );

    let single: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3121OutBlock": { "BscGdsCd": "O_E1A", "BscGdsNm": "W1 Monday E-mini S&P 500 Option" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
}

/// `o3121` numeric out-block field (`DotGb`) parses from BOTH string and number.
#[test]
fn o3121_numeric_field_string_or_number() {
    let from_num: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "o3121OutBlock": [{ "DotGb": 0 }]
    }))
    .expect("number form");
    let from_str: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "o3121OutBlock": [{ "DotGb": "0" }]
    }))
    .expect("string form");
    assert_eq!(from_num.outblock[0].dot_gb, "0");
    assert_eq!(from_str.outblock[0].dot_gb, "0");
}

/// `o3121` empty result (00707) deserializes as the pending case.
#[test]
fn o3121_empty_result_deserializes_as_pending() {
    let empty: O3121Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3121OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty option-master array is the pending case");
}

/// Covers R4, R7. `t9945` serializes to `{"t9945InBlock":{"gubun":"1"}}`; no
/// continuation tokens (non-paginated).
#[test]
fn t9945_request_serializes_to_inblock() {
    let value = serde_json::to_value(T9945Request::new("1")).expect("serialize t9945 request");
    assert_eq!(value["t9945InBlock"]["gubun"], "1", "gubun stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// Covers R4, R6. The stock-master array deserializes through REAL dispatch; the
/// canonical fields read their exact expected values (cross-checked vs korean_name).
#[tokio::test]
async fn t9945_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1102_PATH))
        .and(header("tr_cd", "t9945"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T9945_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_master(&T9945Request::new("1"))
        .await
        .expect("t9945 stock_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock.len() >= 3, "master rows round-trip");
    assert_eq!(resp.outblock[0].shcode, "000020", "first 단축코드");
    assert_eq!(resp.outblock[0].hname, "동화약품", "first 종목명");
    assert_eq!(resp.outblock[2].etfchk, "1", "ETF flag on the ETF row");
}

/// Covers R4. A single-object `t9945OutBlock` (one ticker) still deserializes via
/// `de_vec_or_single` — guards the array-vs-single mis-model.
#[test]
fn t9945_single_object_outblock_deserializes() {
    let resp: T9945Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t9945OutBlock": { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003" }
    }))
    .expect("single-object t9945OutBlock must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].shcode, "005930");
}

/// Covers R6. An empty `t9945` master list (00707) deserializes as the pending case.
#[test]
fn t9945_empty_result_set_deserializes_as_pending() {
    let empty: T9945Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t9945OutBlock": []
    }))
    .expect("empty master list deserializes");
    assert!(empty.outblock.is_empty(), "empty list is the pending case");
}

/// Error: a `01900` response surfaces as `LsError::ApiError` with the exact
/// broker code preserved, classified paper-incompatible.
#[tokio::test]
async fn t9945_code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(T1102_PATH))
        .and(header("tr_cd", "t9945"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"rsp_cd\":\"01900\",\"rsp_msg\":\"모의투자 미지원\"}")
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let err = sdk_for(&server)
        .market_session()
        .stock_master(&T9945Request::new("1"))
        .await
        .expect_err("01900 must surface as an error");
    match err {
        LsError::ApiError { ref code, .. } => {
            assert_eq!(code, "01900", "exact code preserved");
            assert!(err.is_paper_incompatible(), "01900 is paper-incompatible");
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

// === plan -004 batch C — market_session reference offline coverage ==========

/// t1532 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1532_request_and_response_round_trip() {
    let v = serde_json::to_value(T1532Request::new("078020")).expect("serialize t1532");
    let _ = &v;
    let resp: T1532Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1532OutBlock": [{"tmname": "X1", "avgdiff": 41945}]}"#).expect("t1532 body round-trips");
    assert_eq!(resp.outblock[0].tmname, "X1");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].avgdiff, "41945", "avgdiff from JSON number via string_or_number");
    let empty: T1532Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1532OutBlock":[]}"#).expect("empty deserializes");
    assert!(empty.outblock.is_empty());
}

/// t1533 — numeric request fields are JSON numbers; representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1533_request_and_response_round_trip() {
    let v = serde_json::to_value(T1533Request::new("1")).expect("serialize t1533");
    assert!(v["t1533InBlock"]["chgdate"].is_number(), "chgdate numeric");
    let _ = &v;
    let resp: T1533Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1533OutBlock": {"bdate": "x"}, "t1533OutBlock1": [{"tmname": "X1", "avgdiff": 41945}]}"#).expect("t1533 body round-trips");
    assert_eq!(resp.outblock1[0].tmname, "X1");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].avgdiff, "41945", "avgdiff from JSON number via string_or_number");
    let empty: T1533Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1533OutBlock":{},"t1533OutBlock1":[]}"#).expect("empty deserializes");
    assert!(empty.outblock1.is_empty());
}
