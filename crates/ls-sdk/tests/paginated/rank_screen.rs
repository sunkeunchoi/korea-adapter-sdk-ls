use super::*;


// ---------------------------------------------------------------------------
// t1452 — 거래량상위 (top trading volume). The single-page body-`idx` paginated
// sub-pattern: `idx` is an ordinary in-block field (a JSON number), NOT a
// `#[serde(skip)]` header cursor; dispatch is one `post_paginated` with empty
// `tr_cont`/`tr_cont_key` headers; out-rows tolerate single-or-array.
// ---------------------------------------------------------------------------

/// Covers R5. The `t1452` request serializes the body `idx` cursor INSIDE
/// `t1452InBlock` as a JSON number at the first-page convention (`0`), and the
/// `tr_cont`/`tr_cont_key` header cursors are `#[serde(skip)]` — absent from the
/// body (the divergence from `t8412` the single-page sub-pattern depends on).
#[test]
fn t1452_request_serializes_idx_in_block_and_no_continuation_in_body() {
    let value = serde_json::to_value(t1452_req()).expect("serialize t1452 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key (the in-block)");
    let inblock = &value["t1452InBlock"];

    // idx rides IN the body, as a JSON number, at the first-page convention.
    assert_eq!(inblock["idx"], 0, "idx serializes as a number in the in-block");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    // The header cursors never serialize into the body.
    assert!(value.get("tr_cont").is_none(), "tr_cont not in the body");
    assert!(value.get("tr_cont_key").is_none(), "tr_cont_key not in the body");
    assert!(inblock.get("tr_cont").is_none(), "tr_cont not in the in-block");
}

/// Covers R2, R5. A single page deserializes through REAL `post_paginated`
/// dispatch: the request sends `tr_cont: N` (empty cursor) and the response's
/// summary `idx` + ranked-row array round-trip with mixed number/string wire types.
#[tokio::test]
async fn top_volume_deserializes_single_page() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(HIGH_ITEM_PATH))
        .and(header("tr_cd", "t1452"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1452_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .paginated()
        .top_volume(&t1452_req())
        .await
        .expect("t1452 top_volume single page should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.idx, "20", "summary next-page idx round-trips");
    assert_eq!(resp.outblock1.len(), 2, "both ranked rows round-trip");
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[0].price, "71500", "price (from number)");
    assert_eq!(resp.outblock1[1].price, "185000", "price (from string)");
}

/// Covers R2. A single ranked row (not an array) is tolerated as a one-element
/// Vec via `de_vec_or_single`; an empty result set (`00707`) deserializes as the
/// pending case.
#[test]
fn t1452_single_or_array_and_empty_pending() {
    let single: T1452Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1452OutBlock": { "idx": 1 },
        "t1452OutBlock1": { "hname": "단일", "shcode": "000660", "price": 100 }
    }))
    .expect("single row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "000660");

    let empty: T1452Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1452OutBlock": { "idx": 0 },
        "t1452OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty is the pending case, not a flip");
}

// ---------------------------------------------------------------------------
// t1866 — 서버저장조건 리스트조회 (saved-condition spine producer). Body-cursor
// single-page; the cursor is the STRING pair cont/cont_key (not a numeric idx),
// and it takes caller inputs (user_id/gb/group_name). Its out-rows carry the
// query_index that keys the t1859/t1860 condition search.
// ---------------------------------------------------------------------------

/// Covers R5/R7. `t1866::new` serializes its caller inputs and the body cursor
/// INSIDE `t1866InBlock`, with the `tr_cont`/`tr_cont_key` header cursors
/// `#[serde(skip)]` — absent from the body (the single-page convention).
#[test]
fn t1866_request_serializes_inputs_in_block_and_skips_header_cursors() {
    let value = serde_json::to_value(T1866Request::new("d00000")).expect("serialize t1866 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key (the in-block)");
    let inblock = &value["t1866InBlock"];

    assert_eq!(inblock["user_id"], "d00000", "user_id rides in the in-block");
    assert_eq!(inblock["gb"], "0", "gb defaults to list-all");
    assert_eq!(inblock["group_name"], "", "group_name empty = all groups");
    // Body cursor present and EMPTY on the first page.
    assert_eq!(inblock["cont"], "", "first-page cont is empty");
    assert_eq!(inblock["cont_key"], "", "first-page cont_key is empty");
    // Header cursors never serialize into the body.
    assert!(value.get("tr_cont").is_none(), "tr_cont not in the body");
    assert!(inblock.get("tr_cont").is_none(), "tr_cont not in the in-block");
}

/// Covers R5/R8. A success body with one saved condition deserializes under the
/// `t1866OutBlock1` rename key with `query_index` populated (the value the
/// t1859/t1860 chain consumes); an empty `00707` deserializes as the pending
/// (spine-input-unavailable) case, not a flip.
#[test]
fn t1866_deserializes_query_index_rows_and_empty_pending() {
    let single: T1866Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1866OutBlock": { "result_count": 1, "cont": "N", "cont_key": "" },
        "t1866OutBlock1": { "query_index": "000000000001", "group_name": "그룹", "query_name": "조건1" }
    }))
    .expect("single saved-condition row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(
        single.outblock1[0].query_index, "000000000001",
        "query_index populated — the modeled discovery-edge value"
    );

    let empty: T1866Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1866OutBlock": { "result_count": 0, "cont": "N", "cont_key": "" },
        "t1866OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(
        empty.outblock1.is_empty(),
        "no saved condition = spine-input-unavailable pending, not a flip"
    );
}

/// Each remaining paginated Response deserializes a one-row single-page body
/// under its OWN `txxxxOutBlock1` rename key, with the row's fields populated —
/// guarding against a per-TR rename typo that would silently drop the rows.
#[test]
fn remaining_paginated_responses_deserialize_with_correct_rename_keys() {
    let r = rank_row_json();

    let t1403: T1403Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1403OutBlock": { "idx": 10 }, "t1403OutBlock1": [r.clone()]
    })).expect("t1403 body");
    assert_eq!(t1403.outblock1.len(), 1);
    assert_eq!(t1403.outblock1[0].shcode, "005930");
    assert_eq!(t1403.outblock.idx, "10");

    let t1441: T1441Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1441OutBlock": { "idx": 10 }, "t1441OutBlock1": [r.clone()]
    })).expect("t1441 body");
    assert_eq!(t1441.outblock1[0].price, "71500");

    let t1463: T1463Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1463OutBlock": { "idx": 10 }, "t1463OutBlock1": [r.clone()]
    })).expect("t1463 body");
    assert_eq!(t1463.outblock1[0].volume, "12345678");

    let t1466: T1466Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1466OutBlock": { "hhmm": "1530", "idx": 10 },
        "t1466OutBlock1": [r.clone()]
    })).expect("t1466 body");
    assert_eq!(t1466.outblock.hhmm, "1530", "t1466 summary carries hhmm");
    assert_eq!(t1466.outblock1[0].shcode, "005930");

    let t1489: T1489Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1489OutBlock": { "idx": 10 }, "t1489OutBlock1": [r.clone()]
    })).expect("t1489 body");
    assert_eq!(t1489.outblock1[0].hname, "삼성전자");

    let t1492: T1492Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1492OutBlock": { "idx": 10 }, "t1492OutBlock1": [r.clone()]
    })).expect("t1492 body");
    assert_eq!(t1492.outblock1[0].shcode, "005930");

    // A single out-row object (not array) is tolerated, and an empty set is the
    // pending case — spot-checked on t1441.
    let single: T1441Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000", "t1441OutBlock": { "idx": 0 }, "t1441OutBlock1": r
    })).expect("single row");
    assert_eq!(single.outblock1.len(), 1);
    let empty: T1492Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1492OutBlock1": []
    })).expect("empty");
    assert!(empty.outblock1.is_empty());
}

/// Each remaining paginated request serializes its `idx` cursor as a JSON number
/// INSIDE its in-block, with the header cursors absent from the body.
#[test]
fn remaining_paginated_requests_serialize_idx_in_block() {
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("t1403InBlock", serde_json::to_value(T1403Request::new("0", "202401", "202612")).unwrap()),
        ("t1441InBlock", serde_json::to_value(T1441Request::new("0","1","1","0","0","0","0","0","1")).unwrap()),
        ("t1463InBlock", serde_json::to_value(T1463Request::new("0","0","0","0","0","0","0","1")).unwrap()),
        ("t1466InBlock", serde_json::to_value(T1466Request::new("0","1","1","0","0","0","0","0","1")).unwrap()),
        ("t1489InBlock", serde_json::to_value(T1489Request::new("0","0","000000000000","0","0","0")).unwrap()),
        ("t1492InBlock", serde_json::to_value(T1492Request::new("0","1","0","0")).unwrap()),
    ];
    for (key, value) in cases {
        let obj = value.as_object().expect("request object");
        assert_eq!(obj.len(), 1, "{key}: exactly one top-level key");
        let inblock = &value[key];
        assert!(inblock["idx"].is_number(), "{key}: idx serializes as a number");
        assert_eq!(inblock["idx"], 0, "{key}: idx at first-page convention");
        assert!(value.get("tr_cont").is_none(), "{key}: no tr_cont in body");
        assert!(value.get("tr_cont_key").is_none(), "{key}: no tr_cont_key in body");
    }
}

// ---------------------------------------------------------------------------
// t3341 — 재무순위종합 (financial ranking; Wave 2). Single-page body-idx
// sub-pattern (KTD-5): idx is an ordinary in-block field serialized as a JSON
// number, NOT #[serde(skip)]; the header cursors are skipped.
// ---------------------------------------------------------------------------

/// Covers KTD-5. The `t3341` request serializes the body `idx` INSIDE the
/// in-block as a JSON number at the first-page convention (`0`), with documented
/// gubun defaults and no header continuation leaking into the body.
#[test]
fn t3341_request_serializes_idx_in_block_as_number() {
    let value = serde_json::to_value(T3341Request::new()).expect("serialize t3341 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t3341InBlock at the top level");
    let inblock = &value["t3341InBlock"];
    assert_eq!(inblock["gubun"], "0", "all markets");
    assert_eq!(inblock["gubun1"], "1", "sales-growth rank");
    assert_eq!(inblock["gubun2"], "1", "fixed comparison");
    assert_eq!(inblock["idx"], 0, "idx serializes as a number at first-page convention");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(value.get("tr_cont_key").is_none(), "no tr_cont_key in the body");
}

/// Covers KTD-5. A representative success deserializes: the summary (count +
/// next-page `idx`) and the ranked-row array round-trip with mixed number/string
/// wire types; single-or-array tolerated; empty is the pending case.
#[test]
fn t3341_response_round_trips_single_or_array_and_empty() {
    let resp: T3341Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3341OutBlock": { "cnt": 2, "idx": "100" },
        "t3341OutBlock1": [
            { "rank": 1, "hname": "삼성전자", "shcode": "005930", "salesgrowth": 12.3,
              "eps": "5000", "roe": 15.1, "per": "10.5" },
            { "rank": "2", "hname": "SK하이닉스", "shcode": 660, "salesgrowth": "8.1",
              "eps": 3000, "roe": "12.0", "per": 8.2 }
        ]
    }))
    .expect("representative t3341 success must deserialize");
    assert_eq!(resp.outblock.cnt, "2", "summary count populated");
    assert_eq!(resp.outblock.idx, "100", "next-page idx captured");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].shcode, "005930");
    assert_eq!(resp.outblock1[1].shcode, "660", "shcode from JSON number");
    assert_eq!(resp.outblock1[0].rank, "1", "rank from JSON number");

    let single: T3341Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3341OutBlock": { "cnt": 1, "idx": "0" },
        "t3341OutBlock1": { "rank": "1", "hname": "삼성전자", "shcode": "005930" }
    }))
    .expect("single ranked row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);

    let empty: T3341Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3341OutBlock": { "cnt": 0, "idx": "0" }, "t3341OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Covers R3. The `t1411` request serializes the string filters as JSON strings,
/// with the body `idx` cursor as an ORDINARY in-block field serialized as a JSON
/// NUMBER at its first-page convention (`0`) — NOT `#[serde(skip)]` (a skipped
/// cursor would make the key absent; a string `idx` would risk IGW40011). The
/// header continuation is skipped from the body.
#[test]
fn t1411_request_serializes_idx_as_ordinary_number_cursor() {
    let value =
        serde_json::to_value(T1411Request::new("0", "1", "1", "005930")).expect("serialize t1411");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1411InBlock at the top level");
    let inblock = &value["t1411InBlock"];
    for f in ["gubun", "jongchk", "jkrate", "shcode"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["gubun"], "0");
    assert_eq!(inblock["shcode"], "005930");
    // The cursor is present as an ordinary in-block field at first-page `0` — NOT
    // skipped — and serialized as a JSON NUMBER (not a string).
    assert!(
        inblock.get("idx").is_some(),
        "idx is an ordinary in-block field, not skipped"
    );
    assert!(
        inblock["idx"].is_number(),
        "idx serializes as a JSON number (string_as_number) to avoid IGW40011"
    );
    assert_eq!(inblock["idx"], 0, "first-page idx cursor is the number 0");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1411OutBlock` summary (margin rates + next-page `idx`) + the sibling
/// `t1411OutBlock1` array round-trip, and modeled non-key fields hold real
/// (non-default) values.
#[tokio::test]
async fn t1411_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ETC_PATH))
        .and(header("tr_cd", "t1411"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1411_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .stocks_by_margin_rate(&T1411Request::new("0", "1", "1", "005930"))
        .await
        .expect("t1411 stocks_by_margin_rate should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "margin-rate rows round-trip");
    assert_eq!(resp.outblock1[0].hname, "KR모터스", "real non-default Korean name");
    assert_eq!(resp.outblock1[0].shcode, "000040", "real non-default short code");
    assert_eq!(resp.outblock1[0].jkrate, "100", "consigned margin rate from JSON number");
    assert_eq!(resp.outblock1[0].subprice, "440", "substitute price from JSON number");
    assert_eq!(resp.outblock.idx, "40", "next-page cursor from the summary block");
}

/// Covers R4. The margin-rate array tolerates single-or-array + empty (the empty
/// case, R7), and `string_or_number` parses numeric fields from BOTH string and
/// number.
#[test]
fn t1411_response_round_trips_single_or_array_and_empty() {
    let single: T1411Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1411OutBlock": { "jkrate": "20", "sjkrate": "45", "idx": "40" },
        "t1411OutBlock1": { "hname": "KR모터스", "shcode": "000040", "jkrate": "100", "price": "661", "volume": "298" }
    }))
    .expect("single margin-rate row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock.idx, "40", "next-page cursor round-trips (non-empty)");
    assert_eq!(single.outblock1[0].price, "661", "price from JSON string");

    let number: T1411Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1411OutBlock": { "jkrate": 20, "sjkrate": 45, "idx": 0 },
        "t1411OutBlock1": [{ "hname": "KR모터스", "shcode": "000040", "jkrate": 100, "price": 661, "change": 0, "volume": 298 }]
    }))
    .expect("numeric jkrate/price/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "298", "volume from JSON number");
    assert_eq!(number.outblock1[0].jkrate, "100", "jkrate from JSON number");

    let empty: T1411Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1411OutBlock": { "idx": "0" }, "t1411OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty result is the pending case (R7)");
}

/// Registration guard (R3/R8): `T1411_POLICY` must be a real paginated policy.
#[test]
fn t1411_policy_is_registered_and_paginated() {
    assert_eq!(T1411_POLICY.tr_code, "t1411");
    assert_eq!(T1411_POLICY.path, "/stock/etc");
    assert!(
        T1411_POLICY.has_pagination,
        "t1411 self-paginates (body idx) — policy must thread continuation"
    );
    assert!(!T1411_POLICY.is_order, "t1411 is a non-order read");
}

/// Covers R3. The `t1488` request serializes the string filters as JSON strings,
/// with the body `idx` cursor AND the three expected-execution numeric filters
/// (`yesprice`/`yeeprice`/`yevolume`) serialized as JSON NUMBERS via
/// `string_as_number` (a string form would risk IGW40011). `idx` is an ORDINARY
/// in-block field at its first-page convention (`0`) — NOT `#[serde(skip)]`. The
/// header continuation is skipped from the body.
#[test]
fn t1488_request_serializes_four_numeric_fields_as_numbers() {
    let value =
        serde_json::to_value(T1488Request::new("0", "1", "1", "0", "0")).expect("serialize t1488");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1488InBlock at the top level");
    let inblock = &value["t1488InBlock"];
    // String-typed filters serialize as JSON strings.
    for f in ["gubun", "sign", "jgubun", "jongchk", "volume"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    assert_eq!(inblock["gubun"], "0");
    // The four Number-typed fields serialize as JSON numbers (string_as_number).
    for f in ["idx", "yesprice", "yeeprice", "yevolume"] {
        assert!(
            inblock[f].is_number(),
            "{f} serializes as a JSON number (string_as_number) to avoid IGW40011"
        );
    }
    assert_eq!(inblock["idx"], 0, "first-page idx cursor is the number 0");
    assert_eq!(inblock["yesprice"], 0);
    assert_eq!(inblock["yeeprice"], 0);
    assert_eq!(inblock["yevolume"], 0);
    // The cursor is present (NOT skipped) as an ordinary in-block field.
    assert!(
        inblock.get("idx").is_some(),
        "idx is an ordinary in-block field, not skipped"
    );
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1488OutBlock` summary (next-page `idx`) + the sibling `t1488OutBlock1`
/// array round-trip, and modeled non-key fields hold real (non-default) values.
#[tokio::test]
async fn t1488_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_MARKET_DATA_PATH))
        .and(header("tr_cd", "t1488"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1488_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .expected_exec_top_change_rate(&T1488Request::new("0", "1", "1", "0", "0"))
        .await
        .expect("t1488 expected_exec_top_change_rate should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "change-rate rows round-trip");
    assert_eq!(resp.outblock1[0].hname, "삼성전자", "real non-default Korean name");
    assert_eq!(resp.outblock1[0].shcode, "005930", "real non-default short code");
    assert_eq!(resp.outblock1[0].price, "66100", "price from JSON number");
    assert_eq!(resp.outblock1[0].volume, "12345", "volume from JSON number");
    assert_eq!(resp.outblock.idx, "40", "next-page cursor from the summary block");
}

/// Covers R4. The change-rate array tolerates single-or-array + empty (the empty
/// case, R7), and `string_or_number` parses numeric fields from BOTH string and
/// number.
#[test]
fn t1488_response_round_trips_single_or_array_and_empty() {
    let single: T1488Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1488OutBlock": { "idx": "40" },
        "t1488OutBlock1": { "hname": "삼성전자", "shcode": "005930", "price": "66100", "volume": "12345", "jkrate": "20" }
    }))
    .expect("single change-rate row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock.idx, "40", "next-page cursor round-trips (non-empty)");
    assert_eq!(single.outblock1[0].price, "66100", "price from JSON string");

    let number: T1488Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1488OutBlock": { "idx": 0 },
        "t1488OutBlock1": [{ "hname": "삼성전자", "shcode": "005930", "price": 66100, "change": 1200, "volume": 12345 }]
    }))
    .expect("numeric price/change/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "12345", "volume from JSON number");
    assert_eq!(number.outblock1[0].price, "66100", "price from JSON number");

    let empty: T1488Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1488OutBlock": { "idx": "0" }, "t1488OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty result is the pending case (R7)");
}

/// Registration guard (R3/R8): `T1488_POLICY` must be a real paginated policy.
#[test]
fn t1488_policy_is_registered_and_paginated() {
    assert_eq!(T1488_POLICY.tr_code, "t1488");
    assert_eq!(T1488_POLICY.path, "/stock/market-data");
    assert!(
        T1488_POLICY.has_pagination,
        "t1488 self-paginates (body idx) — policy must thread continuation"
    );
    assert!(!T1488_POLICY.is_order, "t1488 is a non-order read");
}

// ---------------------------------------------------------------------------
// t1481 — 시간외등락율상위 (after-hours top change rate; U2 reach wave). Single-page
// body-`idx` sub-pattern (KTD-5/KTD-8): idx is an ordinary in-block field
// serialized as a JSON number, NOT #[serde(skip)]; the header cursors are skipped.
// Out-block shape (single `idx` summary + `t1481OutBlock1` row ARRAY) read from
// the raw capture.
// ---------------------------------------------------------------------------

/// Covers contract item 4 + KTD-4. The `t1481` request serializes the body `idx`
/// INSIDE `t1481InBlock` as a JSON number at the first-page convention (`0`), with
/// the length-1 string flags kept as strings and no header continuation leaking
/// into the body.
#[test]
fn t1481_request_serializes_idx_in_block_as_number() {
    let value = serde_json::to_value(T1481Request::new("1", "1", "1", "1"))
        .expect("serialize t1481 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1481InBlock at the top level");
    let inblock = &value["t1481InBlock"];
    assert_eq!(inblock["gubun1"], "1", "gubun1 stays a string flag");
    assert_eq!(inblock["gubun2"], "1");
    assert_eq!(inblock["jongchk"], "1");
    assert_eq!(inblock["volume"], "1", "volume is a length-1 string flag");
    assert_eq!(inblock["idx"], 0, "idx serializes as a number at first-page convention");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(value.get("tr_cont_key").is_none(), "no tr_cont_key in the body");
}

/// Covers contract items 1, 2, 6. A representative success (from the raw capture)
/// deserializes through REAL `post_paginated` dispatch: the summary next-page
/// `idx` and the `t1481OutBlock1` row array round-trip with mixed number/string
/// wire types, and the canonical row field `hname` (한글명, KTD-6) holds its EXACT
/// expected value.
#[tokio::test]
async fn t1481_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(HIGH_ITEM_PATH))
        .and(header("tr_cd", "t1481"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                // The exact wire shape from the raw capture: a single `idx` summary
                // object and a two-row `t1481OutBlock1` ARRAY (mixed number/string).
                .set_body_string(
                    r#"{
                        "rsp_cd": "00000",
                        "t1481OutBlock": { "idx": 20 },
                        "t1481OutBlock1": [
                            { "volume": 2136, "bidrem1": 301, "price": 10490, "change": 445,
                              "offerrem1": 764, "shcode": "449180", "sign": "5", "diff": "-04.07",
                              "bidho1": 10305, "value": 22493050, "hname": "KODEX 미국S&P500(H)",
                              "offerho1": 10485 },
                            { "volume": 369875, "bidrem1": 9738, "price": 935, "change": 8,
                              "offerrem1": 248, "shcode": "031820", "sign": "5", "diff": "-00.85",
                              "bidho1": 935, "value": 346240565, "hname": "콤텍시스템",
                              "offerho1": 936 }
                        ]
                    }"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .after_hours_top_change_rate(&T1481Request::new("1", "1", "1", "1"))
        .await
        .expect("t1481 after_hours_top_change_rate should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.idx, "20", "summary next-page idx round-trips");
    assert_eq!(resp.outblock1.len(), 2, "both ranked rows round-trip");
    assert_eq!(
        resp.outblock1[0].hname, "KODEX 미국S&P500(H)",
        "canonical row field hname (한글명) holds its exact value"
    );
    assert_eq!(resp.outblock1[0].shcode, "449180");
    assert_eq!(resp.outblock1[0].price, "10490", "price from JSON number");
    assert_eq!(resp.outblock1[0].diff, "-04.07", "diff from JSON string");
    assert_eq!(resp.outblock1[1].volume, "369875", "volume from JSON number");
}

/// Covers contract items 2, 3, 6. A single out-row object (not an array) is
/// tolerated as a one-element Vec via `de_vec_or_single`; `string_or_number` parses
/// a numeric field from BOTH string and number JSON; an empty result set (`00707`)
/// deserializes as the pending case (not a flip).
#[test]
fn t1481_single_or_array_string_or_number_and_empty_pending() {
    // string-form price (parsed via string_or_number) + single object → one-element Vec.
    let single: T1481Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1481OutBlock": { "idx": 1 },
        "t1481OutBlock1": { "hname": "단일", "shcode": "000660", "price": "100" }
    }))
    .expect("single row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "000660");
    assert_eq!(single.outblock1[0].price, "100", "price parsed from a JSON string");

    // number-form price (the other string_or_number branch).
    let numeric: T1481Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1481OutBlock": { "idx": 2 },
        "t1481OutBlock1": [{ "hname": "수치", "shcode": "005930", "price": 71500 }]
    }))
    .expect("number-form price deserializes");
    assert_eq!(numeric.outblock1[0].price, "71500", "price parsed from a JSON number");

    let empty: T1481Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1481OutBlock": { "idx": 0 },
        "t1481OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty is the pending case, not a flip");
}

// ---------------------------------------------------------------------------
// t1482 — 시간외거래량상위 (after-hours top volume; U2 reach wave). Same single-page
// body-`idx` sub-pattern as t1481; the in-block carries a numeric `sort_gbn` sort
// flag serialized as a JSON number. Out-block shape (single `idx` summary +
// `t1482OutBlock1` row ARRAY) read from the raw capture.
// ---------------------------------------------------------------------------

/// Covers contract item 4 + KTD-4. The `t1482` request serializes BOTH the numeric
/// `sort_gbn` sort flag and the body `idx` cursor INSIDE `t1482InBlock` as JSON
/// numbers (first-page `idx` = `0`), keeps the string flags as strings, and leaks
/// no header continuation into the body.
#[test]
fn t1482_request_serializes_sort_gbn_and_idx_as_numbers() {
    let value =
        serde_json::to_value(T1482Request::new("0", "1", "1")).expect("serialize t1482 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1482InBlock at the top level");
    let inblock = &value["t1482InBlock"];
    assert!(
        inblock["sort_gbn"].is_number(),
        "sort_gbn serializes as a JSON number, not a string"
    );
    assert_eq!(inblock["sort_gbn"], 0, "sort_gbn at the requested value");
    assert_eq!(inblock["gubun"], "1", "gubun stays a string flag");
    assert_eq!(inblock["jongchk"], "1");
    assert_eq!(inblock["idx"], 0, "idx serializes as a number at first-page convention");
    assert!(inblock["idx"].is_number(), "idx is a JSON number, not a string");

    assert!(value.get("tr_cont").is_none(), "no tr_cont in the body");
    assert!(value.get("tr_cont_key").is_none(), "no tr_cont_key in the body");
}

/// Covers contract items 1, 2, 6. A representative success (from the raw capture)
/// deserializes through REAL `post_paginated` dispatch: the summary next-page `idx`
/// and the `t1482OutBlock1` row array round-trip with mixed number/string wire
/// types, and the canonical row field `hname` (종목명, KTD-6) holds its EXACT value.
#[tokio::test]
async fn t1482_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(HIGH_ITEM_PATH))
        .and(header("tr_cd", "t1482"))
        .and(header("tr_cont", "N"))
        .respond_with(
            ResponseTemplate::new(200)
                // The exact wire shape from the raw capture: a single `idx` summary
                // object and a two-row `t1482OutBlock1` ARRAY (mixed number/string).
                .set_body_string(
                    r#"{
                        "rsp_cd": "00000",
                        "t1482OutBlock": { "idx": 20 },
                        "t1482OutBlock1": [
                            { "volume": 2413264, "vol": "000.29", "price": 2485, "change": 10,
                              "shcode": "252670", "sign": "5", "diff": "-00.40", "value": 5998142760,
                              "hname": "KODEX 200선물인버스2" },
                            { "volume": 116309, "vol": "000.03", "price": 1120, "change": 5,
                              "shcode": "530031", "sign": "2", "diff": "000.45", "value": 130067985,
                              "hname": "삼성 레버리지 WTI원유" }
                        ]
                    }"#,
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .after_hours_top_volume(&T1482Request::new("0", "1", "1"))
        .await
        .expect("t1482 after_hours_top_volume should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock.idx, "20", "summary next-page idx round-trips");
    assert_eq!(resp.outblock1.len(), 2, "both ranked rows round-trip");
    assert_eq!(
        resp.outblock1[0].hname, "KODEX 200선물인버스2",
        "canonical row field hname (종목명) holds its exact value"
    );
    assert_eq!(resp.outblock1[0].shcode, "252670");
    assert_eq!(resp.outblock1[0].price, "2485", "price from JSON number");
    assert_eq!(resp.outblock1[1].volume, "116309", "volume from JSON number");
}

/// Covers contract items 2, 3, 6. A single out-row object is tolerated as a
/// one-element Vec; `string_or_number` parses a numeric field from BOTH string and
/// number JSON; an empty result (`00707`) deserializes as the pending case.
#[test]
fn t1482_single_or_array_string_or_number_and_empty_pending() {
    let single: T1482Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1482OutBlock": { "idx": 1 },
        "t1482OutBlock1": { "hname": "단일", "shcode": "000660", "volume": "100" }
    }))
    .expect("single row tolerated as a one-element Vec");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].shcode, "000660");
    assert_eq!(single.outblock1[0].volume, "100", "volume parsed from a JSON string");

    let numeric: T1482Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1482OutBlock": { "idx": 2 },
        "t1482OutBlock1": [{ "hname": "수치", "shcode": "005930", "volume": 9999 }]
    }))
    .expect("number-form volume deserializes");
    assert_eq!(numeric.outblock1[0].volume, "9999", "volume parsed from a JSON number");

    let empty: T1482Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707",
        "t1482OutBlock": { "idx": 0 },
        "t1482OutBlock1": []
    }))
    .expect("empty result set deserializes");
    assert_eq!(empty.rsp_cd, "00707");
    assert!(empty.outblock1.is_empty(), "empty is the pending case, not a flip");
}
