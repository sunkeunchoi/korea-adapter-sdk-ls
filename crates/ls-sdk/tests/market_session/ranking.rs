use super::*;


// ---------------------------------------------------------------------------
// t1859 — 서버저장조건 조건검색 (server-saved condition search). market_session,
// non-paginated; the saved-condition spine CONSUMER. Keyed by a `query_index`
// self-sourced from t1866 (the modeled cross-TR discovery edge — never
// fabricated). Summary out-block + matched-issue row array.
// ---------------------------------------------------------------------------

/// Covers R5, R8. The `t1859` request serializes to exactly
/// `{"t1859InBlock":{"query_index":...}}` — the `query_index` rides in the
/// in-block under the correct key, and no `tr_cont`/`tr_cont_key` leak (t1859 is
/// not paginated).
#[test]
fn t1859_request_serializes_with_query_index_in_inblock() {
    let req = T1859Request::new("000000000123");
    let value = serde_json::to_value(&req).expect("serialize t1859 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1859InBlock"), "missing t1859InBlock key");

    let inblock = &value["t1859InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t1859InBlock carries only query_index");
    assert_eq!(inblock["query_index"], "000000000123");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers R5. A representative success response deserializes through the typed
/// path: the summary `result_count` (a modeled non-key field) holds a real
/// non-default value, the matched-issue array round-trips, and numeric fields
/// parse whether they arrive as JSON numbers (row 0) or strings (row 1) via
/// `string_or_number` — proving the subset round-trips, not just `serde(default)`.
#[test]
fn t1859_deserializes_success_with_real_values() {
    let resp: T1859Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1859OutBlock": { "result_count": 2, "result_time": "153000", "text": "전략" },
        "t1859OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "price": 71000, "sign": "2",
              "change": 500, "diff": 0.71, "volume": 1000000 },
            { "shcode": "000660", "hname": "SK하이닉스", "price": "150000", "sign": "5",
              "change": "-1000", "diff": "-0.66", "volume": "500000" }
        ]
    }))
    .expect("representative t1859 success must deserialize");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.result_count, "2", "non-key summary field populated");
    assert_eq!(resp.outblock1.len(), 2, "both matched-issue rows round-trip");
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[0].price, "71000", "price (from JSON number)");
    assert_eq!(resp.outblock1[1].price, "150000", "price (from JSON string)");
}

/// Covers R5. An empty result set (`rsp_cd 00707`, empty out-block) deserializes
/// and is recognized as the empty/pending case — the implement-tr gate records
/// this as PENDING, never a flip.
#[test]
fn t1859_empty_result_set_deserializes_as_empty() {
    let empty: T1859Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1859OutBlock": { "result_count": 0 },
        "t1859OutBlock1": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock1.is_empty(),
        "an empty matched-issue array is the pending case, not a flip"
    );
}

/// Covers R5. A single matched-issue row (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single` (the gateway collapses a one-row result
/// to a bare object).
#[test]
fn t1859_single_out_row_tolerated_as_array() {
    let single: T1859Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1859OutBlock": { "result_count": 1 },
        "t1859OutBlock1": { "shcode": "005930", "hname": "삼성전자", "price": 71000 }
    }))
    .expect("single out-row object must deserialize as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "005930");
}

/// Covers R5. The matched-issue row fields parse whether `price` arrives as a
/// JSON number or string — the `string_or_number` round-trip guarantee proven
/// directly against `T1859OutBlock1`.
#[test]
fn t1859_row_price_number_or_string_yields_same_value() {
    let as_number: T1859OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "price": 71000
    }))
    .expect("number price must deserialize");
    let as_string: T1859OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "price": "71000"
    }))
    .expect("string price must deserialize");
    assert_eq!(as_number.price, "71000");
    assert_eq!(as_number.price, as_string.price);
}

/// Compile-time guard: `T1859Response` default envelope is empty.
#[test]
fn t1859_response_envelope_default_is_empty() {
    let resp = T1859Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock1.is_empty());
    assert_eq!(resp.outblock.result_count, "");
}

// ---------------------------------------------------------------------------
// t1826 — 종목Q클릭검색리스트조회 (ThinQ Q-click search-list; Wave 3 producer).
// market_session, non-paginated; takes a `search_gb` catalog filter and returns
// the `search_cd` keys consumed by `t1825`.
// ---------------------------------------------------------------------------

/// Covers AE2. `T1826Request::new` serializes the `search_gb` filter under the
/// `t1826InBlock` key, with no `tr_cont`/`tr_cont_key` leak (t1826 is not
/// paginated).
#[test]
fn t1826_request_serializes_with_search_gb_in_inblock() {
    let req = T1826Request::new("0");
    let value = serde_json::to_value(&req).expect("serialize t1826 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1826InBlock"), "missing t1826InBlock key");

    let inblock = &value["t1826InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(inblock_obj.len(), 1, "t1826InBlock carries only search_gb");
    assert_eq!(inblock["search_gb"], "0");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers AE2. A representative success response deserializes through the typed
/// path: the `search_cd` catalog keys round-trip (the `t1825` discovery-edge
/// input), and `search_cd` parses whether it arrives as a JSON number (row 0) or
/// string (row 1) via `string_or_number` — proving the subset round-trips, not
/// just `serde(default)`.
#[test]
fn t1826_deserializes_success_with_real_values() {
    let resp: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1826OutBlock": [
            { "search_cd": "0001", "search_nm": "거래량급증" },
            { "search_cd": 2, "search_nm": "외국인순매수" }
        ]
    }))
    .expect("representative t1826 success must deserialize");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "both available-search rows round-trip");
    assert_eq!(resp.outblock[0].search_cd, "0001", "search_cd (from string)");
    assert_eq!(resp.outblock[1].search_cd, "2", "search_cd (from JSON number)");
}

/// Covers AE2. An empty result set (`rsp_cd 00707`, empty out-block) deserializes
/// and is recognized as the empty/pending case — the implement-tr gate records
/// this as PENDING, never a flip.
#[test]
fn t1826_empty_result_set_deserializes_as_empty() {
    let empty: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1826OutBlock": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock.is_empty(),
        "an empty search-list is the pending case, not a flip"
    );
}

/// Covers AE2. A single available-search row (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single`.
#[test]
fn t1826_single_out_row_tolerated_as_array() {
    let single: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1826OutBlock": { "search_cd": "0001", "search_nm": "거래량급증" }
    }))
    .expect("single out-row object must deserialize as a one-element Vec");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].search_cd, "0001");
}

/// Covers AE2. `search_cd` parses whether it arrives as a JSON number or string —
/// the `string_or_number` round-trip guarantee proven directly against
/// `T1826OutBlock`.
#[test]
fn t1826_search_cd_number_or_string_yields_same_value() {
    let as_number: T1826OutBlock = serde_json::from_value(serde_json::json!({
        "search_cd": 1
    }))
    .expect("number search_cd must deserialize");
    let as_string: T1826OutBlock = serde_json::from_value(serde_json::json!({
        "search_cd": "1"
    }))
    .expect("string search_cd must deserialize");
    assert_eq!(as_number.search_cd, "1");
    assert_eq!(as_number.search_cd, as_string.search_cd);
}

/// Compile-time guard: `T1826Response` default envelope is empty.
#[test]
fn t1826_response_envelope_default_is_empty() {
    let resp = T1826Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t1825 — 종목Q클릭검색 (ThinQ Q-click search; Wave 3 consumer). market_session,
// non-paginated; keyed by a `search_cd` self-sourced from t1826 (the discovery
// edge), plus a `gubun` market filter.
// ---------------------------------------------------------------------------

/// Covers AE2. `T1825Request::new` serializes both `search_cd` and `gubun` under
/// the `t1825InBlock` key, with no `tr_cont`/`tr_cont_key` leak (t1825 is not
/// paginated).
#[test]
fn t1825_request_serializes_with_search_cd_and_gubun_in_inblock() {
    let req = T1825Request::new("0001", "0");
    let value = serde_json::to_value(&req).expect("serialize t1825 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t1825InBlock"), "missing t1825InBlock key");

    let inblock = &value["t1825InBlock"];
    let inblock_obj = inblock.as_object().expect("inblock is an object");
    assert_eq!(
        inblock_obj.len(),
        2,
        "t1825InBlock carries only search_cd and gubun"
    );
    assert_eq!(inblock["search_cd"], "0001");
    assert_eq!(inblock["gubun"], "0");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(
        value.get("tr_cont_key").is_none(),
        "no tr_cont_key in the body"
    );
}

/// Covers AE2. A representative success response deserializes through the typed
/// path: the summary `jong_cnt` (a modeled non-key field) holds a real
/// non-default value, the matched-issue array round-trips, and numeric fields
/// parse whether they arrive as JSON numbers (row 0) or strings (row 1) via
/// `string_or_number` — proving the subset round-trips, not just `serde(default)`.
#[test]
fn t1825_deserializes_success_with_real_values() {
    let resp: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1825OutBlock": { "JongCnt": 2 },
        "t1825OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "close": 71000, "change": 500,
              "diff": 0.71, "volume": 1000000 },
            { "shcode": "000660", "hname": "SK하이닉스", "close": "150000", "change": "-1000",
              "diff": "-0.66", "volume": "500000" }
        ]
    }))
    .expect("representative t1825 success must deserialize");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.jong_cnt, "2", "non-key summary field populated");
    assert_eq!(resp.outblock1.len(), 2, "both matched-issue rows round-trip");
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[0].close, "71000", "close (from JSON number)");
    assert_eq!(resp.outblock1[1].close, "150000", "close (from JSON string)");
}

/// Covers AE2. An empty result set (`rsp_cd 00707`, empty out-block) deserializes
/// and is recognized as the empty/pending case.
#[test]
fn t1825_empty_result_set_deserializes_as_empty() {
    let empty: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1825OutBlock": { "JongCnt": 0 },
        "t1825OutBlock1": []
    }))
    .expect("empty result set must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock1.is_empty(),
        "an empty matched-issue array is the pending case, not a flip"
    );
}

/// Covers AE2. A single matched-issue row (not an array) is tolerated as a
/// one-element Vec via `de_vec_or_single`.
#[test]
fn t1825_single_out_row_tolerated_as_array() {
    let single: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1825OutBlock": { "JongCnt": 1 },
        "t1825OutBlock1": { "shcode": "005930", "hname": "삼성전자", "close": 71000 }
    }))
    .expect("single out-row object must deserialize as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "005930");
}

/// Covers AE2. The matched-issue row fields parse whether `close` arrives as a
/// JSON number or string — proven directly against `T1825OutBlock1`.
#[test]
fn t1825_row_close_number_or_string_yields_same_value() {
    let as_number: T1825OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "close": 71000
    }))
    .expect("number close must deserialize");
    let as_string: T1825OutBlock1 = serde_json::from_value(serde_json::json!({
        "shcode": "005930", "close": "71000"
    }))
    .expect("string close must deserialize");
    assert_eq!(as_number.close, "71000");
    assert_eq!(as_number.close, as_string.close);
}

/// Compile-time guard: `T1825Response` default envelope is empty.
#[test]
fn t1825_response_envelope_default_is_empty() {
    let resp = T1825Response::default();
    assert_eq!(resp.rsp_cd, "");
    assert!(resp.outblock1.is_empty());
    assert_eq!(resp.outblock.jong_cnt, "");
}

/// Covers AE2 / KTD-3 contingency. The OFFLINE captured-chain fixture: validates
/// the `t1826 → t1825` chained-smoke harness *logic* independently of live data.
/// A recorded `t1826` body deserializes, its first `search_cd` is extracted, that
/// value builds a `t1825` request (proving the self-source wiring), and a recorded
/// `t1825` body deserializes — so harness correctness does not depend on the paper
/// account having seeded data (decouples "is the chain code correct" from "does
/// this account have data").
#[test]
fn t1825_chained_off_t1826_offline_fixture() {
    // Stage 1: a recorded t1826 search-list body deserializes.
    let producer: T1826Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1826OutBlock": [
            { "search_cd": "0001", "search_nm": "거래량급증" },
            { "search_cd": "0002", "search_nm": "외국인순매수" }
        ]
    }))
    .expect("recorded t1826 producer body must deserialize");
    assert!(
        !producer.outblock.is_empty(),
        "non-empty producer is the precondition for chaining"
    );

    // Stage 2: self-source the search_cd from the producer (never fabricated) and
    // build the consumer request — the exact wiring live_smoke_t1825 performs.
    let search_cd = producer.outblock[0].search_cd.clone();
    let req = T1825Request::new(&search_cd, "0");
    let value = serde_json::to_value(&req).expect("serialize chained t1825 request");
    assert_eq!(
        value["t1825InBlock"]["search_cd"], "0001",
        "the consumer request carries the self-sourced search_cd"
    );

    // Stage 3: a recorded t1825 body for that search deserializes.
    let consumer: T1825Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1825OutBlock": { "JongCnt": 1 },
        "t1825OutBlock1": [
            { "shcode": "005930", "hname": "삼성전자", "close": 71000, "change": 500,
              "diff": 0.71, "volume": 1000000 }
        ]
    }))
    .expect("recorded t1825 consumer body must deserialize");
    assert_eq!(consumer.outblock1.len(), 1, "the chained consumer body round-trips");
    assert_eq!(consumer.outblock1[0].shcode, "005930");
}

/// Covers R4, R6. `t8424` serializes to exactly `{"t8424InBlock":{"gubun1":""}}`
/// with no continuation tokens (non-paginated).
#[test]
fn t8424_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8424Request::new()).expect("serialize t8424 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8424InBlock"]["gubun1"], "", "gubun1 empty placeholder");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5, R6. The spec-derived fixture deserializes through REAL dispatch:
/// the sector array round-trips with a real `upcode`/`hname`, and `upcode` is a
/// string (never coerced numeric).
#[tokio::test]
async fn t8424_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t8424"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8424_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sectors(&T8424Request::new())
        .await
        .expect("t8424 sectors should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock.len() >= 3, "sector rows round-trip");
    assert_eq!(resp.outblock[0].upcode, "001", "first sector upcode (string)");
    assert!(!resp.outblock[0].hname.is_empty(), "real non-default hname");
}

/// Covers R4, R6. A single-object `t8424OutBlock` (one sector) still deserializes
/// via `de_vec_or_single` — not only the array form.
#[test]
fn t8424_single_object_outblock_deserializes() {
    let resp: T8424Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8424OutBlock": { "hname": "종합", "upcode": "001" }
    }))
    .expect("single-object t8424OutBlock must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].upcode, "001");
}

/// Covers R4, R7. `t1511` serializes to `{"t1511InBlock":{"upcode":"001"}}`.
#[test]
fn t1511_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1511Request::new("001")).expect("serialize t1511 request");
    assert_eq!(value["t1511InBlock"]["upcode"], "001", "upcode stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// Covers R2, R5, R7. `t1511` single-OutBlock snapshot deserializes through REAL
/// dispatch; numeric fields tolerate both string and number wire forms.
#[tokio::test]
async fn t1511_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1511"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1511_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sector_quote(&T1511Request::new("001"))
        .await
        .expect("t1511 sector_quote should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock.hname.is_empty(), "real non-default hname");
    assert_eq!(resp.outblock.pricejisu, "2610.62", "현재지수 current index (was a number)");
    assert!(!resp.outblock.firstjisu.is_empty(), "first sub-index populated");
}

/// Covers R4, R5. The `volume` field tolerates a JSON number or string via
/// `string_or_number` (the gateway sends `volume` as an integer).
#[test]
fn t1511_volume_number_or_string_yields_same_value() {
    let as_number: T1511Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1511OutBlock": { "hname": "종합", "volume": 263165 }
    }))
    .expect("number volume must deserialize");
    let as_string: T1511Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1511OutBlock": { "hname": "종합", "volume": "263165" }
    }))
    .expect("string volume must deserialize");
    assert_eq!(as_number.outblock.volume, "263165");
    assert_eq!(as_number.outblock.volume, as_string.outblock.volume);
}

// ---- t1638 종목별잔량/사전공시 (closed-window more-flips; market_session OutBlock array) ----

/// `t1638` serializes to
/// `{"t1638InBlock":{"gubun1":"1","shcode":"","gubun2":"1","exchgubun":""}}`
/// (all four String fields, `shcode` empty for the full list; non-paginated — no
/// tr_cont tokens in the body).
#[test]
fn t1638_request_serializes_to_inblock() {
    let value = serde_json::to_value(T1638Request::new("1", "", "1", ""))
        .expect("serialize t1638 request");
    assert_eq!(value["t1638InBlock"]["gubun1"], "1", "gubun1 stays a string");
    assert_eq!(value["t1638InBlock"]["shcode"], "", "shcode stays an empty string (full list)");
    assert_eq!(value["t1638InBlock"]["gubun2"], "1", "gubun2 stays a string");
    assert_eq!(value["t1638InBlock"]["exchgubun"], "", "exchgubun stays a string");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// A representative success body deserializes AND a modeled non-key field
/// (`price`) holds a real, non-default value — proving the subset round-trips, not
/// just that `serde(default)` returned `Ok`. The numeric fields tolerate a JSON
/// number or string via `string_or_number`; the out-block is a repeated array.
#[test]
fn t1638_success_body_deserializes_with_nondefault_field() {
    let as_number: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1638OutBlock": [
            { "rank": 1, "hname": "삼성전자", "price": 60600, "obuyvol": 1200, "buyrem": 5000, "shcode": "005930" },
            { "rank": 2, "hname": "S-Oil", "price": 60400, "obuyvol": -300, "sellrem": 4000, "shcode": "010950" }
        ]
    }))
    .expect("number body must deserialize");
    let as_string: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1638OutBlock": [
            { "rank": "1", "hname": "삼성전자", "price": "60600", "obuyvol": "1200", "buyrem": "5000", "shcode": "005930" }
        ]
    }))
    .expect("string body must deserialize");
    assert_eq!(as_number.outblock.len(), 2, "array out-block round-trips");
    assert_eq!(as_number.outblock[0].price, "60600", "modeled non-key field is non-default");
    assert_eq!(as_number.outblock[0].rank, "1");
    assert_eq!(as_number.outblock[0].shcode, "005930");
    assert_eq!(as_number.outblock[0].price, as_string.outblock[0].price);
    assert_eq!(as_number.outblock[0].rank, as_string.outblock[0].rank);
}

/// A single (non-array) out-block object is tolerated via `de_vec_or_single`.
#[test]
fn t1638_single_object_out_block_is_tolerated() {
    let resp: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1638OutBlock": { "rank": 1, "hname": "삼성전자", "price": 60600, "shcode": "005930" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock[0].shcode, "005930");
}

/// An empty result (`00707`, no out-block) deserializes cleanly to an empty Vec —
/// no panic on a missing `t1638OutBlock`; recognized as the empty/pending case.
#[test]
fn t1638_empty_result_deserializes_to_defaults() {
    let empty: T1638Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1638 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.is_empty(), "no out-block → empty Vec");
}

// ---- t1475 VP대비등락률상하위 (open-window domestic; market_session numeric request slots + ranked array) ----

/// `t1475` serializes its seven caller filters under the renamed `t1475InBlock` key
/// with the numeric slots (`datacnt`/`date`/`time`/`rankcnt`) as JSON NUMBERS (else
/// the gateway returns `IGW40011`); Strings stay strings; no leaked fields.
#[test]
fn t1475_new_serializes_numeric_slots_as_numbers_no_leak() {
    let value = serde_json::to_value(T1475Request::new("005930", "1", "20", "0", "0", "0", "0"))
        .expect("serialize t1475 request");
    assert_eq!(value["t1475InBlock"]["shcode"], "005930");
    assert_eq!(value["t1475InBlock"]["vptype"], "1");
    assert_eq!(value["t1475InBlock"]["gubun"], "0");
    // numeric request slots serialize as JSON numbers, not strings.
    assert_eq!(value["t1475InBlock"]["datacnt"], 20);
    assert!(value["t1475InBlock"]["datacnt"].is_number(), "datacnt is a JSON number");
    assert_eq!(value["t1475InBlock"]["date"], 0);
    assert!(value["t1475InBlock"]["date"].is_number(), "date is a JSON number");
    assert_eq!(value["t1475InBlock"]["time"], 0);
    assert_eq!(value["t1475InBlock"]["rankcnt"], 0);
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
    assert!(
        value["t1475InBlock"].get("nrec").is_none(),
        "no leaked caller fields"
    );
}

/// A representative success body deserializes AND a modeled non-key row field
/// (`price`) holds a real, non-default value. Numeric-bearing fields tolerate a JSON
/// number or string; the ranked out-block is a repeated array, with an echo header.
#[test]
fn t1475_success_body_deserializes_with_nondefault_field() {
    let as_string: T1475Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1475OutBlock": { "date": "20260629", "time": "153000", "rankcnt": "20" },
        "t1475OutBlock1": [
            { "datetime": "20260629", "price": "00073000", "sign": "2", "change": "00000500",
              "volume": "000012345678", "todayvp": "00000123", "ma5vp": "00000120" },
            { "datetime": "20260629", "price": "00045500", "sign": "5", "change": "-0000300",
              "volume": "000009345678", "todayvp": "00000099", "ma5vp": "00000101" }
        ],
        "rsp_msg": "정상처리"
    }))
    .expect("string body must deserialize");
    let as_number: T1475Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1475OutBlock": { "date": 20260629i64 },
        "t1475OutBlock1": [ { "price": 73000, "volume": 12345678i64 } ]
    }))
    .expect("number body must deserialize");
    assert_eq!(as_string.outblock.date, "20260629", "echo header round-trips");
    assert_eq!(as_string.outblock1.len(), 2, "ranked array round-trips");
    assert_eq!(
        as_string.outblock1[0].price, "00073000",
        "modeled non-key row field is non-default"
    );
    assert_eq!(as_number.outblock.date, "20260629");
    assert_eq!(as_number.outblock1[0].price, "73000");
    assert_eq!(as_number.outblock1[0].volume, "12345678");
}

/// A single (non-array) `t1475OutBlock1` object is tolerated via `de_vec_or_single`.
#[test]
fn t1475_single_object_out_block_is_tolerated() {
    let resp: T1475Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1475OutBlock1": { "datetime": "20260629", "price": "00073000" }
    }))
    .expect("single-object out-block must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object → one-element Vec");
    assert_eq!(resp.outblock1[0].price, "00073000");
}

/// An empty result (`00707`) deserializes cleanly — default header, empty rows.
#[test]
fn t1475_empty_result_deserializes_to_empty() {
    let empty: T1475Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "rsp_msg": "조회할 자료가 없습니다."
    }))
    .expect("an empty t1475 envelope must deserialize");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock.date.is_empty(), "no header → default object");
    assert!(empty.outblock1.is_empty(), "no rows → empty Vec");
}

/// Covers R4, R7. `t1485` serializes to `{"t1485InBlock":{"upcode":"001","gubun":"1"}}`.
#[test]
fn t1485_request_serializes_to_inblock() {
    let value =
        serde_json::to_value(T1485Request::new("001", "1")).expect("serialize t1485 request");
    assert_eq!(value["t1485InBlock"]["upcode"], "001");
    assert_eq!(value["t1485InBlock"]["gubun"], "1");
}

/// Covers R2, R5, R7. `t1485` summary block + time-row array round-trip through
/// REAL dispatch; the `t1485OutBlock1` array (and single-object form) deserialize.
#[tokio::test]
async fn t1485_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1485"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1485_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sector_expected_index(&T1485Request::new("001", "1"))
        .await
        .expect("t1485 sector_expected_index should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    // Summary block round-trips (separate struct from the time array — a rename
    // typo on t1485OutBlock would silently zero these without this assertion).
    assert_eq!(resp.outblock.pricejisu, "2610.62", "summary 예상지수");
    assert_eq!(resp.outblock.volume, "263165", "summary volume (was a JSON number)");
    assert!(resp.outblock1.len() >= 2, "expected-index time rows round-trip");
    assert!(!resp.outblock1[0].jisu.is_empty(), "real non-default jisu");
}

/// Covers R4, R7. `t1485OutBlock1` single-object form deserializes via `de_vec_or_single`.
#[test]
fn t1485_single_object_outblock1_deserializes() {
    let resp: T1485Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1485OutBlock1": { "jisu": "2617.03", "volume": 7372, "chetime": "장  전" }
    }))
    .expect("single-object t1485OutBlock1 must deserialize");
    assert_eq!(resp.outblock1.len(), 1);
    assert_eq!(resp.outblock1[0].chetime, "장  전", "non-numeric chetime label");
}

/// Covers R4, R7. `t1516` carries TWO caller identifiers — serializes to
/// `{"t1516InBlock":{"upcode":"001","gubun":"1","shcode":"005930"}}`.
#[test]
fn t1516_request_serializes_two_identifiers() {
    let value = serde_json::to_value(T1516Request::new("001", "1", "005930"))
        .expect("serialize t1516 request");
    assert_eq!(value["t1516InBlock"]["upcode"], "001");
    assert_eq!(value["t1516InBlock"]["shcode"], "005930", "second required input");
}

/// Covers R2, R5, R7. `t1516` summary + per-stock array round-trip through REAL
/// dispatch.
#[tokio::test]
async fn t1516_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1516"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1516_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .sector_stocks(&T1516Request::new("001", "1", ""))
        .await
        .expect("t1516 sector_stocks should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    // Summary header round-trips (separate struct from the per-stock array).
    assert_eq!(resp.outblock.shcode, "000640", "echoed board shcode");
    assert_eq!(resp.outblock.pricejisu, "000002610.62", "summary 지수");
    assert!(resp.outblock1.len() >= 2, "per-stock rows round-trip");
    assert!(!resp.outblock1[0].shcode.is_empty(), "real per-stock shcode");
    assert!(!resp.outblock1[0].hname.is_empty(), "real per-stock name");
}

/// Covers R5, R7. `t1516OutBlock1` single-object form deserializes via
/// `de_vec_or_single`, and an empty board (00707) is the pending case.
#[test]
fn t1516_single_and_empty_outblock1_deserialize() {
    let single: T1516Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1516OutBlock": { "shcode": "000640", "pricejisu": "2610.62" },
        "t1516OutBlock1": { "shcode": "005930", "hname": "삼성전자", "price": 70000 }
    }))
    .expect("single per-stock row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].price, "70000", "price from JSON number");

    let empty: T1516Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1516OutBlock": { "shcode": "" }, "t1516OutBlock1": []
    }))
    .expect("empty board deserializes");
    assert!(empty.outblock1.is_empty(), "empty board is the pending case");
}

/// Covers R5, R6. An empty `t8424` sector list (00707) deserializes as the
/// pending case, mirroring every prior array-bearing TR.
#[test]
fn t8424_empty_result_set_deserializes_as_pending() {
    let empty: T8424Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8424OutBlock": []
    }))
    .expect("empty sector list deserializes");
    assert!(empty.outblock.is_empty(), "empty list is the pending case");
}

// --- t3521 — 해외지수조회 (overseas index snapshot) --------------------------

/// `t3521` serializes to `{"t3521InBlock":{"kind":"...","symbol":"..."}}`; no numeric
/// request fields, non-paginated.
#[test]
fn t3521_request_serializes_to_inblock() {
    let value = serde_json::to_value(T3521Request::new("S", "DJI@DJI")).expect("serialize t3521");
    assert_eq!(value["t3521InBlock"]["kind"], "S");
    assert_eq!(value["t3521InBlock"]["symbol"], "DJI@DJI");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// The snapshot out-block deserializes through REAL dispatch; the substantive
/// `close` (현재지수) reads its exact value.
#[tokio::test]
async fn t3521_deserializes_through_dispatch() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTINFO_PATH))
        .and(header("tr_cd", "t3521"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"{"rsp_cd":"00000","rsp_msg":"조회완료","t3521OutBlock":{"date":"20230602","symbol":"DJI@DJI","change":"701.19","sign":"2","diff":"2.12","close":"33762.76","hname":"다우 산업"}}"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .overseas_index_quote(&T3521Request::new("S", "DJI@DJI"))
        .await
        .expect("t3521 overseas_index_quote should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.close, "33762.76", "현재지수 round-trips");
    assert_eq!(resp.outblock.hname, "다우 산업", "지수명 round-trips");
}

/// A numeric `close` from a JSON number still decodes (string_or_number tolerance);
/// an empty snapshot is the pending case.
#[test]
fn t3521_numeric_close_and_empty_deserialize() {
    let numeric: T3521Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3521OutBlock": { "symbol": "DJI@DJI", "close": 33762.76 }
    }))
    .expect("numeric close tolerated");
    assert_eq!(numeric.outblock.close, "33762.76");

    let empty: T3521Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3521OutBlock": {}
    }))
    .expect("empty snapshot deserializes");
    assert!(empty.outblock.close.is_empty(), "empty close is the pending case");
}
