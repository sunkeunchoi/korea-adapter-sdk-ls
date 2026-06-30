use super::*;


// ---------------------------------------------------------------------------
// CSPAQ12300 — BEP단가조회 (read-only account BEP/balance inquiry).
// ---------------------------------------------------------------------------

/// `::new` serializes the four query-shape enums under `CSPAQ12300InBlock1` with
/// NO account number / caller leak. The account is config-supplied, never threaded
/// into the body.
#[test]
fn cspaq12300_request_serializes_inblock_only_no_account_leak() {
    let req = CSPAQ12300Request::new("1", "0", "0", "0");
    let value = serde_json::to_value(&req).expect("serialize CSPAQ12300 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CSPAQ12300InBlock1"),
        "missing CSPAQ12300InBlock1 key"
    );

    let inblock = &value["CSPAQ12300InBlock1"];
    assert_eq!(inblock["BalCreTp"], "1", "BalCreTp rides in the body");
    assert_eq!(inblock["CmsnAppTpCode"], "0");
    assert_eq!(inblock["D2balBaseQryTp"], "0");
    assert_eq!(inblock["UprcTpCode"], "0");
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        4,
        "InBlock1 carries only the four enum selectors (no account number)"
    );

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CSPAQ12300 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`MnyOrdAbleAmt` = 현금주문가능금액, KTD4) holds its exact value. Distinct fields
/// carry distinct values so a mislabel cannot be masked.
#[tokio::test]
async fn cspaq12300_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .and(header("tr_cd", "CSPAQ12300"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CSPAQ12300_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CSPAQ12300Request::new("1", "0", "0", "0");
    let resp = sdk
        .account()
        .bep(&req)
        .await
        .expect("CSPAQ12300 BEP inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.balcretp, "1");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);

    assert_eq!(resp.outblock2.len(), 1, "one balance row");
    let bal = &resp.outblock2[0];
    // Canonical field by Korean name (현금주문가능금액) — exact value, not !is_empty().
    assert_eq!(bal.mnyordableamt, "1234567", "MnyOrdAbleAmt (현금주문가능금액)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(bal.mnyoutableamt, "1200000", "MnyoutAbleAmt");
    assert_eq!(bal.balevalamt, "2500000", "BalEvalAmt");
    assert_eq!(bal.dpsasttotamt, "3500000", "DpsastTotamt");
    assert_eq!(bal.dps, "1100000", "Dps");
    assert_eq!(bal.ordableamt, "1300000", "OrdAbleAmt");
}

/// Numeric-bearing fields parse via `string_or_number` from BOTH string and number
/// JSON.
#[test]
fn cspaq12300_numeric_fields_parse_from_string_and_number() {
    // Numbers as JSON numbers.
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ12300OutBlock2": { "MnyOrdAbleAmt": 999, "BalEvalAmt": 42 }
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].mnyordableamt, "999");
    assert_eq!(resp.outblock2[0].balevalamt, "42");

    // Same fields as JSON strings.
    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ12300OutBlock2": { "MnyOrdAbleAmt": "999", "BalEvalAmt": "42" }
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].mnyordableamt, "999");
    assert_eq!(resp.outblock2[0].balevalamt, "42");
}

/// An empty result (`rsp_cd 00707`, empty out-block) deserializes and is recognized
/// as the empty/pending case.
#[test]
fn cspaq12300_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CSPAQ12300OutBlock2": []
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(json).expect("empty out-block must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty balance Vec (the PENDING case)"
    );
}

/// `CSPAQ12300OutBlock2` arriving as a SINGLE object deserializes via
/// `de_vec_or_single` into a 1-element Vec.
#[test]
fn cspaq12300_out_block2_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CSPAQ12300OutBlock2": { "MnyOrdAbleAmt": 500000, "BalEvalAmt": 750000 }
    });
    let resp: CSPAQ12300Response =
        serde_json::from_value(json).expect("single-object out-block must deserialize");
    assert_eq!(
        resp.outblock2.len(),
        1,
        "single object becomes a 1-element Vec"
    );
    assert_eq!(resp.outblock2[0].mnyordableamt, "500000");
    assert_eq!(resp.outblock2[0].balevalamt, "750000");
}

// ---------------------------------------------------------------------------
// CCENQ90200 — KRX야간파생 잔고조회 (read-only night-derivatives account balance,
// krx_extended). InBlock1: RecCnt (JSON number) + two evaluation-shape enums.
// THREE out-blocks: OutBlock1 single + OutBlock2 + OutBlock3 (a true JSON array).
// ---------------------------------------------------------------------------

/// `::new` serializes the record count (as a JSON number, KTD4) + two enums under
/// `CCENQ90200InBlock1` with NO account number / caller leak. The account is
/// config-supplied, never threaded into the body.
#[test]
fn ccenq90200_request_serializes_inblock_only_no_account_leak() {
    let req = CCENQ90200Request::new("1", "0", "0");
    let value = serde_json::to_value(&req).expect("serialize CCENQ90200 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(
        obj.contains_key("CCENQ90200InBlock1"),
        "missing CCENQ90200InBlock1 key"
    );

    let inblock = &value["CCENQ90200InBlock1"];
    // RecCnt serializes as a JSON NUMBER, not a string (KTD4 — avoids IGW40011).
    assert!(
        inblock["RecCnt"].is_number(),
        "RecCnt must serialize as a JSON number"
    );
    assert_eq!(inblock["RecCnt"], 1);
    assert_eq!(inblock["BalEvalTp"], "0");
    assert_eq!(inblock["FutsPrcEvalTp"], "0");
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        3,
        "InBlock1 carries only the three shape fields (no account number)"
    );

    // No account number anywhere in the serialized request.
    let serialized = serde_json::to_string(&req).expect("serialize CCENQ90200 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes and the canonical OutBlock2 field
/// (`EvalDpsamtTotamt` = 평가예탁금총액, KTD6) holds its exact value. Distinct
/// neighbours carry distinct values so a mislabel cannot be masked. The OutBlock3
/// array deserializes into a Vec.
#[tokio::test]
async fn ccenq90200_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FUTUREOPTION_ACCNO_PATH))
        .and(header("tr_cd", "CCENQ90200"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(CCENQ90200_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = CCENQ90200Request::new("1", "0", "0");
    let resp = sdk
        .account()
        .night_balance(&req)
        .await
        .expect("CCENQ90200 night-balance inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.acntno, TEST_ACCOUNT_NO);
    assert_eq!(resp.outblock1.balevaltp, "2");

    assert_eq!(resp.outblock2.len(), 1, "one balance row");
    let bal = &resp.outblock2[0];
    // Canonical field by Korean name (평가예탁금총액) — exact value, not !is_empty().
    assert_eq!(bal.evaldpsamttotamt, "34399538", "EvalDpsamtTotamt (평가예탁금총액)");
    // Distinct neighbours hold distinct values (no collapse / mislabel).
    assert_eq!(bal.dpsamttotamt, "31925203", "DpsamtTotamt (예탁금총액)");
    assert_eq!(bal.psnoutabletotamt, "20321010", "PsnOutAbleTotAmt (인출가능총금액)");
    assert_eq!(bal.ordabletotamt, "20327175", "OrdAbleTotAmt (주문가능총금액)");
    assert_eq!(bal.mnyordableamt, "20327176", "MnyOrdAbleAmt (현금주문가능금액)");
    assert_eq!(bal.evalpnlsum, "6288000", "EvalPnlSum (평가손익합계)");

    // OutBlock3 is a true JSON array in the raw capture → Vec.
    assert_eq!(resp.outblock3.len(), 1, "one position row");
    assert_eq!(resp.outblock3[0].fnoisuno, "105W6000", "FnoIsuNo");
    assert_eq!(resp.outblock3[0].unsttqty, "2", "UnsttQty (미결제수량)");
    assert_eq!(resp.outblock3[0].evalamt, "40658000", "EvalAmt (평가금액)");
}

/// Numeric-bearing out-block fields parse via `string_or_number` from BOTH string
/// and number JSON.
#[test]
fn ccenq90200_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ90200OutBlock2": { "EvalDpsamtTotamt": 999, "EvalPnlSum": 42 }
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock2[0].evaldpsamttotamt, "999");
    assert_eq!(resp.outblock2[0].evalpnlsum, "42");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ90200OutBlock2": { "EvalDpsamtTotamt": "999", "EvalPnlSum": "42" }
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock2[0].evaldpsamttotamt, "999");
    assert_eq!(resp.outblock2[0].evalpnlsum, "42");
}

/// An empty result (`rsp_cd 00707`, empty out-blocks) deserializes and is recognized
/// as the empty/pending case (off the krx_extended night window or a position-less
/// account).
#[test]
fn ccenq90200_empty_00707_deserializes_as_empty() {
    let json = serde_json::json!({
        "rsp_cd": "00707",
        "CCENQ90200OutBlock2": [],
        "CCENQ90200OutBlock3": []
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(json).expect("empty out-blocks must deserialize");
    assert_eq!(resp.rsp_cd, "00707");
    assert!(
        resp.outblock2.is_empty(),
        "00707 yields an empty balance Vec (the PENDING case)"
    );
    assert!(resp.outblock3.is_empty(), "00707 yields an empty position Vec");
}

/// Both `CCENQ90200OutBlock2` and `CCENQ90200OutBlock3` arriving as a SINGLE object
/// deserialize via `de_vec_or_single` into 1-element Vecs.
#[test]
fn ccenq90200_out_blocks_single_object_deserialize_to_one_element_vecs() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "CCENQ90200OutBlock2": { "EvalDpsamtTotamt": 34399538, "EvalPnlSum": 6288000 },
        "CCENQ90200OutBlock3": { "FnoIsuNo": "105W6000", "EvalAmt": 40658000 }
    });
    let resp: CCENQ90200Response =
        serde_json::from_value(json).expect("single-object out-blocks must deserialize");
    assert_eq!(resp.outblock2.len(), 1, "OutBlock2 single → 1-element Vec");
    assert_eq!(resp.outblock2[0].evaldpsamttotamt, "34399538");
    assert_eq!(resp.outblock3.len(), 1, "OutBlock3 single → 1-element Vec");
    assert_eq!(resp.outblock3[0].fnoisuno, "105W6000");
}

// ---------------------------------------------------------------------------
// t0424 — 주식잔고2 (read-only stock balance: cash summary + per-holding array).
// The wave's U2 holdings gate: the per-holding array length proves whether the
// account carries stock positions (KTD3).
// ---------------------------------------------------------------------------

/// `::new` serializes only the gubun flags under `t0424InBlock` with NO account
/// number; the continuation echo defaults to empty.
#[test]
fn t0424_request_serializes_inblock_only_no_account_leak() {
    let req = T0424Request::new("", "0", "0", "0");
    let value = serde_json::to_value(&req).expect("serialize t0424 request");

    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "request must have exactly one top-level key");
    assert!(obj.contains_key("t0424InBlock"), "missing t0424InBlock key");

    let inblock = &value["t0424InBlock"];
    assert_eq!(inblock["chegb"], "0");
    assert_eq!(inblock["cts_expcode"], "", "continuation echo empty on first page");
    assert_eq!(
        inblock.as_object().expect("inblock is an object").len(),
        5,
        "InBlock carries only the four gubun flags + continuation (no account number)"
    );

    let serialized = serde_json::to_string(&req).expect("serialize t0424 request");
    assert!(
        !serialized.contains(TEST_ACCOUNT_NO),
        "the account number must never appear in the request body"
    );
}

/// A representative success body deserializes; the substantive cash witness
/// (`sunamt` = 추정순자산) holds a non-default value and the holdings array
/// round-trips one position with distinct field values.
#[tokio::test]
async fn t0424_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T0424_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let req = T0424Request::new("", "0", "0", "0");
    let resp = sdk
        .account()
        .stock_balance(&req)
        .await
        .expect("t0424 stock-balance inquiry should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    // The substantive cash witness is non-default (KTD5).
    assert_eq!(resp.outblock.sunamt, "80030265", "sunamt (추정순자산)");
    assert_eq!(resp.outblock.tappamt, "150283", "tappamt (평가금액)");
    assert_eq!(resp.outblock.tdtsunik, "30271", "tdtsunik distinct from dtsunik");

    assert_eq!(resp.outblock1.len(), 1, "one held position");
    let pos = &resp.outblock1[0];
    assert_eq!(pos.hname, "삼성전자", "issue name");
    assert_eq!(pos.expcode, "005930", "issue code");
    assert_eq!(pos.janqty, "2", "balance quantity (holdings witness)");
    assert_eq!(pos.mdposqt, "1", "sellable quantity distinct from janqty");
    assert_eq!(pos.price, "75300", "current price");
}

/// Numeric-bearing fields parse via `string_or_number` from BOTH string and number
/// JSON (cash-summary `sunamt` + holdings `janqty`).
#[test]
fn t0424_numeric_fields_parse_from_string_and_number() {
    let as_number = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": 80030265 },
        "t0424OutBlock1": [{ "janqty": 2 }]
    });
    let resp: T0424Response =
        serde_json::from_value(as_number).expect("number JSON must deserialize");
    assert_eq!(resp.outblock.sunamt, "80030265");
    assert_eq!(resp.outblock1[0].janqty, "2");

    let as_string = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": "80030265" },
        "t0424OutBlock1": [{ "janqty": "2" }]
    });
    let resp: T0424Response =
        serde_json::from_value(as_string).expect("string JSON must deserialize");
    assert_eq!(resp.outblock.sunamt, "80030265");
    assert_eq!(resp.outblock1[0].janqty, "2");
}

/// AE2: an empty holdings array on a populated cash summary is the cash-only case —
/// it deserializes, the holdings Vec is empty, but the cash witness is non-default.
#[test]
fn t0424_empty_holdings_is_cash_only_not_a_defect() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": 80030265, "tappamt": 0 },
        "t0424OutBlock1": []
    });
    let resp: T0424Response =
        serde_json::from_value(json).expect("cash-only body must deserialize");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(
        resp.outblock1.is_empty(),
        "empty holdings array = cash-only account (the U2 cash-summary case)"
    );
    assert_eq!(
        resp.outblock.sunamt, "80030265",
        "the cash witness is still non-default — a cash-summary flip, not a positions flip"
    );
}

/// `t0424OutBlock1` arriving as a SINGLE object deserializes via `de_vec_or_single`
/// into a 1-element Vec.
#[test]
fn t0424_out_block1_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0424OutBlock": { "sunamt": 80030265 },
        "t0424OutBlock1": { "hname": "삼성전자", "janqty": 5 }
    });
    let resp: T0424Response =
        serde_json::from_value(json).expect("single-object holdings must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock1[0].janqty, "5");
}

// ---------------------------------------------------------------------------
// t0441 — 선물/옵션잔고평가(이동평균) (F/O balance valuation: position array + summary).
// No numeric request slots; on a position-less account both blocks are empty/zero.
// ---------------------------------------------------------------------------

/// `::new` serializes to exactly `{"t0441InBlock":{...}}` with empty continuation
/// echoes and NO account number.
#[test]
fn t0441_request_serializes_inblock_only_no_account_leak() {
    let req = T0441Request::new();
    let value = serde_json::to_value(&req).expect("serialize t0441 request");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    let inblock = &value["t0441InBlock"];
    assert_eq!(inblock["cts_expcode"], "", "continuation empty on first page");
    let serialized = serde_json::to_string(&req).expect("serialize t0441 request");
    assert!(!serialized.contains(TEST_ACCOUNT_NO), "no account number in the body");
}

/// A representative non-empty body deserializes; the position array round-trips one
/// row and the valuation summary witness (`tappamt`) holds a non-default value.
#[tokio::test]
async fn t0441_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(FUTUREOPTION_ACCNO_PATH))
        .and(header("tr_cd", "t0441"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T0441_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let resp = sdk
        .account()
        .fo_balance_eval(&T0441Request::new())
        .await
        .expect("t0441 balance valuation should succeed");

    assert_eq!(resp.rsp_cd, "00000");
    assert_eq!(resp.outblock1.len(), 1, "one F/O position");
    assert_eq!(resp.outblock1[0].expcode, "101T9000", "issue code");
    assert_eq!(resp.outblock1[0].jqty, "10", "balance quantity (position witness)");
    assert_eq!(resp.outblock.tappamt, "850000000", "tappamt (summary witness)");
    assert_eq!(resp.outblock.tsunik, "-5625000", "tsunik distinct");
}

/// AE2: an empty position array + zero summary deserializes — the position-less
/// account case (the U2 downgrade), not a defect.
#[test]
fn t0441_empty_positions_is_position_less_not_a_defect() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0441OutBlock1": [],
        "t0441OutBlock": { "tappamt": 0, "tsunik": 0 }
    });
    let resp: T0441Response =
        serde_json::from_value(json).expect("position-less body must deserialize");
    assert!(resp.outblock1.is_empty(), "no F/O positions");
    assert_eq!(resp.outblock.tappamt, "0", "zero valuation summary (expected-empty)");
}

/// `t0441OutBlock1` arriving as a SINGLE object deserializes via `de_vec_or_single`.
#[test]
fn t0441_out_block1_single_object_deserializes_to_one_element_vec() {
    let json = serde_json::json!({
        "rsp_cd": "00000",
        "t0441OutBlock1": { "expcode": "101T9000", "jqty": 3 }
    });
    let resp: T0441Response =
        serde_json::from_value(json).expect("single-object position must deserialize");
    assert_eq!(resp.outblock1.len(), 1, "single object becomes a 1-element Vec");
    assert_eq!(resp.outblock1[0].jqty, "3");
}
