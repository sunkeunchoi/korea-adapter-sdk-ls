use super::*;


/// Covers R4. `t8401` serializes to exactly `{"t8401InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t8401_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8401Request::new()).expect("serialize t8401 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8401InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t8401InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the row array round-trips and the canonical identity field `hname`
/// (종목명, the stock-futures contract name) holds its EXACT value. The fixture's
/// neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode`/`basecode` instead would surface here.
#[tokio::test]
async fn t8401_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8401"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8401_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_futures_master(&T8401Request::new())
        .await
        .expect("t8401 stock_futures_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "two stock-futures master rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "삼성전자   F 202307",
        "종목명 stock-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "111T7000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4111T70004", "확장코드 (distinct)");
    assert_eq!(row.basecode, "A005930", "기초자산코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "삼성화재   F 202512", "second row distinct");
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8401OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8401_single_object_row_deserializes_to_one_element_vec() {
    let single: T8401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8401OutBlock": { "hname": "삼성전자   F 202307" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "삼성전자   F 202307");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8401OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8401` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8401_empty_result_deserializes_as_pending() {
    let empty: T8401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

/// Covers R4. `t8426` serializes to exactly `{"t8426InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t8426_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8426Request::new()).expect("serialize t8426 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8426InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t8426InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the row array round-trips and the canonical identity field `hname`
/// (종목명, the commodity-futures contract name) holds its EXACT value. The
/// fixture's neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode` instead would surface here.
#[tokio::test]
async fn t8426_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8426"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8426_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .commodity_futures_master(&T8426Request::new())
        .await
        .expect("t8426 commodity_futures_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "two commodity-futures master rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "금          F 202306",
        "종목명 commodity-futures contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "175T6000", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4175T60003", "확장코드 (distinct)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "돈육          F 202309", "second row distinct");
}

/// Covers shared contract item 2. `shcode` (단축코드) parses via
/// `ls_core::string_or_number` from BOTH a string and a JSON number — the gateway
/// may send a numeric-looking code either way.
#[test]
fn t8426_shcode_number_or_string_yields_same_value() {
    let as_number: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8426OutBlock": [{ "hname": "금          F 202306", "shcode": 1756000 }]
    }))
    .expect("numeric shcode deserializes");
    let as_string: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8426OutBlock": [{ "hname": "금          F 202306", "shcode": "1756000" }]
    }))
    .expect("string shcode deserializes");
    assert_eq!(as_number.outblock[0].shcode, "1756000");
    assert_eq!(as_string.outblock[0].shcode, "1756000");
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8426OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8426_single_object_row_deserializes_to_one_element_vec() {
    let single: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8426OutBlock": { "hname": "금          F 202306" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "금          F 202306");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8426OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8426` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8426_empty_result_deserializes_as_pending() {
    let empty: T8426Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

/// Covers R4. `t8433` serializes to exactly `{"t8433InBlock":{"dummy":""}}` with
/// no continuation tokens (non-paginated) and no caller fields leaking — the read
/// takes no caller input.
#[test]
fn t8433_request_serializes_to_inblock() {
    let value = serde_json::to_value(T8433Request::new()).expect("serialize t8433 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["t8433InBlock"]["dummy"], "", "dummy placeholder serializes empty");
    let inblock = value["t8433InBlock"].as_object().expect("in-block is an object");
    assert_eq!(inblock.len(), 1, "only the dummy placeholder, no caller fields");
    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
}

/// Covers R2, R5 + KTD4. The spec-derived fixture deserializes through REAL
/// dispatch: the row array round-trips and the canonical identity field `hname`
/// (종목명, the index-option contract name) holds its EXACT value. The fixture's
/// neighbouring fields carry DISTINCT values, so a mislabel that picked
/// `shcode`/`expcode`/a price field instead would surface here.
#[tokio::test]
async fn t8433_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FO_MARKET_DATA_PATH))
        .and(header("tr_cd", "t8433"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T8433_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .index_option_master(&T8433Request::new())
        .await
        .expect("t8433 index_option_master should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.len(), 2, "two index-option master rows");
    let row = &resp.outblock[0];
    // The canonical identity field, by Korean name 종목명 — exact value.
    assert_eq!(
        row.hname, "C 2307 185.0",
        "종목명 index-option contract name (canonical field)"
    );
    // Distinct neighbours: a mislabel would collapse these onto hname.
    assert_eq!(row.shcode, "201T7185", "단축코드 (distinct)");
    assert_eq!(row.expcode, "KR4201T71852", "확장코드 (distinct)");
    assert_eq!(row.hprice, "175.80", "상한가 (distinct)");
    assert_eq!(row.lprice, "102.90", "하한가 (distinct)");
    assert_eq!(row.jnilclose, "127.95", "전일종가 (distinct)");
    assert_eq!(row.jnilhigh, "131.40", "전일고가 (distinct)");
    assert_eq!(row.jnillow, "124.10", "전일저가 (distinct)");
    // recprice (기준가) is distinct from jnilclose — a 기준가/전일종가 mislabel surfaces.
    assert_eq!(row.recprice, "127.90", "기준가 (distinct from 전일종가)");
    // A distinct second row, proving the array carries multiple rows.
    assert_eq!(resp.outblock[1].hname, "C 2406 330.0", "second row distinct");
}

/// Covers shared contract item 2. `hprice` (상한가) parses via
/// `ls_core::string_or_number` from BOTH a string and a JSON number — the gateway
/// may send a numeric-looking price either way.
#[test]
fn t8433_hprice_number_or_string_yields_same_value() {
    // Use a value whose JSON-number form and string form normalize identically
    // (no trailing-zero divergence), so the two forms cross-assert equal — the
    // same round-trip guarantee the sibling TRs' number-or-string tests prove.
    let as_number: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8433OutBlock": [{ "hname": "C 2307 185.0", "hprice": 175.5 }]
    }))
    .expect("numeric hprice deserializes");
    let as_string: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8433OutBlock": [{ "hname": "C 2307 185.0", "hprice": "175.5" }]
    }))
    .expect("string hprice deserializes");
    assert_eq!(as_number.outblock[0].hprice, "175.5");
    assert_eq!(
        as_number.outblock[0].hprice, as_string.outblock[0].hprice,
        "both wire forms normalize to the same string"
    );
}

/// Covers the array single-or-Vec case (shared contract item 6): a single-object
/// `t8433OutBlock` body deserializes to a one-element `Vec` via
/// `de_vec_or_single`.
#[test]
fn t8433_single_object_row_deserializes_to_one_element_vec() {
    let single: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8433OutBlock": { "hname": "C 2307 185.0" }
    }))
    .expect("single-object row deserializes");
    assert_eq!(single.outblock.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock[0].hname, "C 2307 185.0");
    // The standalone row struct also default-constructs cleanly.
    assert!(T8433OutBlock::default().hname.is_empty());
}

/// Covers R5. An empty `t8433` master (00707, empty out-block) deserializes as
/// the pending case — the row array is empty.
#[test]
fn t8433_empty_result_deserializes_as_pending() {
    let empty: T8433Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707"
    }))
    .expect("empty master deserializes");
    assert!(empty.outblock.is_empty(), "empty master is the pending case");
}

/// `t3102` request rename + title block round-trips. This read ships HELD
/// (input-unresolved: `sNewsno` is sourced only from the realtime `NWS`
/// WebSocket feed — no REST producer), so only the offline shape is pinned;
/// no live smoke flips it.
#[test]
fn t3102_request_renames_and_title_round_trips() {
    // The in-block serializes `sNewsno` under its exact wire key.
    let value = serde_json::to_value(T3102Request::new("20260624123456")).expect("serialize t3102");
    assert_eq!(value["t3102InBlock"]["sNewsno"], "20260624123456");
    assert!(
        value.get("t3102OutBlock2").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T3102Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3102OutBlock2": { "sTitle": "삼성전자, 신규 투자 발표" }
    }))
    .expect("representative t3102 success must deserialize");
    assert_eq!(resp.outblock2.title, "삼성전자, 신규 투자 발표", "뉴스타이틀");
}

/// `t3102` input-unresolved HELD path: with no REST producer of a news number,
/// the caller input cannot be discovered, so the read is dispositioned HELD —
/// the empty result still deserializes (the pending/empty case).
#[test]
fn t3102_empty_result_deserializes_as_pending() {
    let empty: T3102Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock2.title.is_empty(), "empty title is the held/pending case");
}

/// `t3320` request rename + summary + ratios round-trip; the canonical 한글기업명
/// (`company`) and 현재가 (`price`) hold DISTINCT exact values so a mislabel is
/// caught (KTD6). `gicode` echoes back in the ratios block.
#[test]
fn t3320_request_renames_and_summary_round_trips() {
    let value = serde_json::to_value(T3320Request::new("005930")).expect("serialize t3320");
    assert_eq!(value["t3320InBlock"]["gicode"], "005930");
    assert!(
        value.get("t3320OutBlock").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T3320Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3320OutBlock": {
            "company": "삼성전자",
            "marketnm": "코스피",
            "price": 71000,
            "jnilclose": "70500",
            "sigavalue": 4238000
        },
        "t3320OutBlock1": {
            "gicode": "A005930",
            "per": 12.34,
            "eps": "5700",
            "pbr": 1.45,
            "bps": "49000"
        }
    }))
    .expect("representative t3320 success must deserialize");
    // Canonical 한글기업명 / 현재가 pinned to DISTINCT exact values (KTD6).
    assert_eq!(resp.outblock.company, "삼성전자", "한글기업명");
    assert_eq!(resp.outblock.price, "71000", "현재가");
    assert_eq!(resp.outblock.jnilclose, "70500", "전일종가 from string preserved");
    assert_eq!(resp.outblock1.gicode, "A005930", "기업코드 echoes the caller gicode");
    assert_eq!(resp.outblock1.per, "12.34", "PER from number");
}

/// `t3320` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t3320_numeric_field_string_or_number() {
    let from_num: T3320OutBlock =
        serde_json::from_value(serde_json::json!({ "price": 71000 }))
            .expect("number form deserializes");
    let from_str: T3320OutBlock =
        serde_json::from_value(serde_json::json!({ "price": "71000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.price, "71000");
    assert_eq!(from_str.price, "71000");
}

/// `t3320` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn t3320_empty_result_deserializes_as_pending() {
    let empty: T3320Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.company.is_empty(), "empty summary is the pending case");
}

/// Covers R4, R7. `t3202` serializes to `{"t3202InBlock":{"shcode":"...","date":""}}`.
#[test]
fn t3202_request_serializes_to_inblock() {
    let value = serde_json::to_value(T3202Request::new("001200")).expect("serialize t3202 request");
    assert_eq!(value["t3202InBlock"]["shcode"], "001200");
    assert_eq!(value["t3202InBlock"]["date"], "", "date defaults empty (all)");
    assert!(value.get("tr_cont").is_none(), "non-paginated: no tr_cont");
}

/// Covers R4, R6. The schedule array deserializes through REAL dispatch; the
/// canonical event label reads its exact expected value.
#[tokio::test]
async fn t3202_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTINFO_PATH))
        .and(header("tr_cd", "t3202"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T3202_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .market_session()
        .stock_schedule(&T3202Request::new("001200"))
        .await
        .expect("t3202 stock_schedule should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock.len() >= 2, "schedule rows round-trip");
    assert_eq!(resp.outblock[0].upunm, "주주총회", "canonical 업무명 event label");
    assert_eq!(resp.outblock[0].custnm, "유진투자증권(주)", "발행회사명");
}

/// Covers R4. A single-object `t3202OutBlock` still deserializes via `de_vec_or_single`.
#[test]
fn t3202_single_object_outblock_deserializes() {
    let resp: T3202Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3202OutBlock": { "shcode": "001200", "upunm": "배당", "recdt": "20240101" }
    }))
    .expect("single-object t3202OutBlock must deserialize");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].upunm, "배당");
}

/// Covers R6. An empty `t3202` schedule (00707) deserializes as the pending case.
#[test]
fn t3202_empty_result_set_deserializes_as_pending() {
    let empty: T3202Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3202OutBlock": []
    }))
    .expect("empty schedule deserializes");
    assert!(empty.outblock.is_empty(), "empty schedule is the pending case");
}

/// t1764 — representative body round-trips (response numerics via string_or_number); empty 00707.
#[test]
fn t1764_request_and_response_round_trip() {
    let v = serde_json::to_value(T1764Request::new("001200")).expect("serialize t1764");
    let _ = &v;
    let resp: T1764Response = serde_json::from_str(r#"{"rsp_cd": "00000", "t1764OutBlock": [{"tradno": "X1", "rank": 41945}]}"#).expect("t1764 body round-trips");
    assert_eq!(resp.outblock[0].tradno, "X1");
    assert_eq!(resp.outblock.len(), 1);
    assert_eq!(resp.outblock[0].rank, "41945", "rank from JSON number via string_or_number");
    let empty: T1764Response = serde_json::from_str(r#"{"rsp_cd":"00707","t1764OutBlock":[]}"#).expect("empty deserializes");
    assert!(empty.outblock.is_empty());
}

// ---------------------------------------------------------------------------
// t0167 — 서버시간조회 (server-time utility read). Stateless, closure-viable.
// ---------------------------------------------------------------------------

/// `::new` serializes to exactly `{"t0167InBlock":{"id":""}}` (no caller input).
#[test]
fn t0167_request_serializes_inblock_only() {
    let req = T0167Request::new();
    let value = serde_json::to_value(&req).expect("serialize t0167 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert!(obj.contains_key("t0167InBlock"), "missing t0167InBlock key");
    assert_eq!(value["t0167InBlock"]["id"], "", "id slot is empty");
}

/// The spec-derived fixture deserializes; the substantive `time` witness holds a
/// non-default value and `dt` is the server date.
#[tokio::test]
async fn t0167_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/etc/time-search"))
        .and(header("tr_cd", "t0167"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T0167_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .market_session()
        .server_time(&T0167Request::new())
        .await
        .expect("t0167 server-time should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.time, "102652926435", "time (substantive witness)");
    assert_eq!(resp.outblock.dt, "20260628", "server date");
}

/// `dt`/`time` parse via `string_or_number` from BOTH string and number JSON.
#[test]
fn t0167_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "t0167OutBlock": { "dt": 20260628i64, "time": 102652926435i64 }
    });
    let resp: T0167Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock.dt, "20260628");
    assert_eq!(resp.outblock.time, "102652926435");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "t0167OutBlock": { "dt": "20260628", "time": "102652926435" }
    });
    let resp: T0167Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock.time, "102652926435");
}
