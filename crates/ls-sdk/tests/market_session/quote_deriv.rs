use super::*;


// ---------------------------------------------------------------------------
// U5 (reach wave) — F/O quote/master reads. All `/futureoption/market-data`,
// `[선물/옵션] 시세`, non-paginated. Out-block keys + array-ness read from the RAW
// capture (KTD5): t2111/t2112/t8402/t8403 SINGLE; t2106 summary + ARRAY detail;
// t8434 ARRAY. The canonical field is pinned by baseline `korean_name` (KTD6) to
// an EXACT value from a fixture whose distinct numerics do not collapse.
// ---------------------------------------------------------------------------

/// `t2111` request rename + single out-block round-trips; the canonical 종합지수
/// (`pricejisu`) and KOSPI200지수 (`kospijisu`) hold DISTINCT exact values so a
/// mislabel of either is caught (KTD6). Numeric fields deserialize from a number.
#[test]
fn t2111_request_renames_and_single_out_block_round_trips() {
    let value = serde_json::to_value(T2111Request::new("A0166000")).expect("serialize t2111");
    assert_eq!(value["t2111InBlock"]["focode"], "A0166000");
    assert!(
        value.get("t2111OutBlock").is_none(),
        "no out-block / caller field leaks into the request body"
    );

    let resp: T2111Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2111OutBlock": {
            "hname": "F 202509",
            "price": 350.25,
            "sign": "2",
            "volume": 123456,
            "mgjv": 9876,
            "pricejisu": 2650.42,
            "kospijisu": 351.18,
            "focode": "A0166000"
        }
    }))
    .expect("representative t2111 success must deserialize");
    // Canonical 종합지수 / KOSPI200지수 pinned to DISTINCT exact values (KTD6).
    assert_eq!(resp.outblock.pricejisu, "2650.42", "종합지수 (composite index)");
    assert_eq!(resp.outblock.kospijisu, "351.18", "KOSPI200지수");
    assert_eq!(resp.outblock.focode, "A0166000");
    assert_eq!(resp.outblock.price, "350.25");
}

/// `t2111` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn t2111_numeric_field_string_or_number() {
    let from_num: T2111OutBlock =
        serde_json::from_value(serde_json::json!({ "pricejisu": 2650.42 }))
            .expect("number form deserializes");
    let from_str: T2111OutBlock =
        serde_json::from_value(serde_json::json!({ "pricejisu": "2650.42" }))
            .expect("string form deserializes");
    assert_eq!(from_num.pricejisu, "2650.42");
    assert_eq!(from_str.pricejisu, "2650.42");
}

/// `t2111` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn t2111_empty_result_deserializes_as_pending() {
    let empty: T2111Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t2112` request rename + single order-book out-block round-trips; the canonical
/// 매도호가1 (`offerho1`) and 매수호가1 (`bidho1`) hold DISTINCT exact values (KTD6).
#[test]
fn t2112_request_renames_and_order_book_round_trips() {
    let value = serde_json::to_value(T2112Request::new("A0166000")).expect("serialize t2112");
    assert_eq!(value["t2112InBlock"]["shcode"], "A0166000");

    let resp: T2112Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2112OutBlock": {
            "hname": "F 202509",
            "price": 350.25,
            "volume": 123456,
            "offerho1": 350.50,
            "bidho1": 350.00,
            "offerrem1": 12,
            "bidrem1": 34,
            "shcode": "A0166000"
        }
    }))
    .expect("representative t2112 success must deserialize");
    assert_eq!(resp.outblock.offerho1, "350.5", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "350", "매수호가1");
    assert_eq!(resp.outblock.shcode, "A0166000");
}

/// `t2112` numeric out-block field parses from BOTH string and number JSON; empty
/// result deserializes as pending.
#[test]
fn t2112_numeric_string_or_number_and_empty_pending() {
    let from_num: T2112OutBlock =
        serde_json::from_value(serde_json::json!({ "offerho1": 350.5 }))
            .expect("number form deserializes");
    let from_str: T2112OutBlock =
        serde_json::from_value(serde_json::json!({ "offerho1": "350.5" }))
            .expect("string form deserializes");
    assert_eq!(from_num.offerho1, "350.5");
    assert_eq!(from_str.offerho1, "350.5");

    let empty: T2112Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t2106` request rename + summary block + ARRAY memo block round-trips; a
/// single memo object collapses to a one-element Vec via `de_vec_or_single`
/// (KTD5). Canonical 출력값 (`vals`) pinned to its exact value (KTD6).
#[test]
fn t2106_request_renames_and_memo_array_round_trips() {
    let value = serde_json::to_value(T2106Request::new("101T6000")).expect("serialize t2106");
    assert_eq!(value["t2106InBlock"]["code"], "101T6000");
    assert_eq!(value["t2106InBlock"]["nrec"], "", "default count is empty");

    let resp: T2106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2106OutBlock": { "nrec": "2" },
        "t2106OutBlock1": [
            { "indx": "1", "gubn": "1", "vals": "12345" },
            { "indx": "2", "gubn": "2", "vals": "67890" }
        ]
    }))
    .expect("representative t2106 success must deserialize");
    assert_eq!(resp.outblock.nrec, "2", "출력건수");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].vals, "12345", "출력값");
    assert_eq!(resp.outblock1[1].indx, "2");

    // single memo object → one-element Vec (KTD5 single-or-array).
    let single: T2106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t2106OutBlock1": { "indx": "1", "gubn": "1", "vals": "12345" }
    }))
    .expect("single memo object deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");

    let empty: T2106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t2106OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty memo array is the pending case");
}

/// `t8402` request rename + single out-block round-trips; the futures 한글명
/// (`hname`) and underlying 기초자산한글명 (`basehname`) hold DISTINCT exact
/// values so a mislabel of either is caught (KTD6).
#[test]
fn t8402_request_renames_and_single_out_block_round_trips() {
    let value = serde_json::to_value(T8402Request::new("111T6000")).expect("serialize t8402");
    assert_eq!(value["t8402InBlock"]["focode"], "111T6000");

    let resp: T8402Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8402OutBlock": {
            "hname": "삼성전자 F 202509",
            "price": 9100,
            "sign": "2",
            "volume": 5000,
            "mgjv": 1200,
            "shcode": "005930",
            "basehname": "삼성전자",
            "baseprice": 91000
        }
    }))
    .expect("representative t8402 success must deserialize");
    assert_eq!(resp.outblock.basehname, "삼성전자", "기초자산한글명 (underlying name)");
    assert_eq!(resp.outblock.hname, "삼성전자 F 202509", "한글명 (futures name)");
    assert_eq!(resp.outblock.shcode, "005930", "기초자산단축코드");

    let from_num: T8402OutBlock =
        serde_json::from_value(serde_json::json!({ "baseprice": 91000 }))
            .expect("number form deserializes");
    let from_str: T8402OutBlock =
        serde_json::from_value(serde_json::json!({ "baseprice": "91000" }))
            .expect("string form deserializes");
    assert_eq!(from_num.baseprice, "91000");
    assert_eq!(from_str.baseprice, "91000");

    let empty: T8402Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t8403` request rename + single order-book out-block round-trips; 매도호가1 and
/// 매수호가1 hold DISTINCT exact values (KTD6); numeric field string-or-number.
#[test]
fn t8403_request_renames_and_order_book_round_trips() {
    let value = serde_json::to_value(T8403Request::new("111T6000")).expect("serialize t8403");
    assert_eq!(value["t8403InBlock"]["shcode"], "111T6000");

    let resp: T8403Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8403OutBlock": {
            "hname": "삼성전자 F 202509",
            "price": 9100,
            "volume": 5000,
            "offerho1": 9105,
            "bidho1": 9095,
            "offerrem1": 7,
            "bidrem1": 9,
            "shcode": "005930"
        }
    }))
    .expect("representative t8403 success must deserialize");
    assert_eq!(resp.outblock.offerho1, "9105", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "9095", "매수호가1");

    let from_str: T8403OutBlock =
        serde_json::from_value(serde_json::json!({ "offerho1": "9105" }))
            .expect("string form deserializes");
    assert_eq!(from_str.offerho1, "9105");

    let empty: T8403Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" }))
            .expect("empty deserializes");
    assert!(empty.outblock.price.is_empty(), "empty out-block is the pending case");
}

/// `t8434` request: `qrycnt` serializes as a JSON NUMBER (KTD4 — `string_as_number`,
/// avoids IGW40011); the ARRAY out-block round-trips and a single row collapses to
/// a one-element Vec (KTD5). Canonical 단축코드 (`focode`) pinned exactly (KTD6).
#[test]
fn t8434_request_serializes_qrycnt_as_number_and_array_round_trips() {
    let value = serde_json::to_value(T8434Request::new("1", "101T6000")).expect("serialize t8434");
    assert_eq!(value["t8434InBlock"]["qrycnt"], 1, "qrycnt serializes as a JSON number");
    assert!(
        value["t8434InBlock"]["qrycnt"].is_number(),
        "qrycnt is a JSON number, not a string (IGW40011 guard)"
    );
    assert_eq!(value["t8434InBlock"]["focode"], "101T6000");

    let resp: T8434Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8434OutBlock1": [
            { "hname": "F 202509", "price": 350.25, "sign": "2", "change": 1.50, "volume": 123, "focode": "101T6000" },
            { "hname": "F 202512", "price": "352.00", "sign": "2", "change": "2.00", "volume": "456", "focode": "101T9000" }
        ]
    }))
    .expect("representative t8434 success must deserialize");
    assert_eq!(resp.outblock1.len(), 2);
    assert_eq!(resp.outblock1[0].focode, "101T6000", "단축코드");
    assert_eq!(resp.outblock1[1].price, "352.00", "price from string preserved verbatim");

    // single row object → one-element Vec (KTD5).
    let single: T8434Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t8434OutBlock1": { "hname": "F 202509", "price": 350.25, "focode": "101T6000" }
    }))
    .expect("single row deserializes");
    assert_eq!(single.outblock1.len(), 1, "single object becomes a one-element Vec");
    assert_eq!(single.outblock1[0].focode, "101T6000");
    assert!(T8434OutBlock1::default().hname.is_empty());

    let empty: T8434Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t8434OutBlock1": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock1.is_empty(), "empty array is the pending case");
}

// ---------------------------------------------------------------------------
// Overseas-stock reads (reach wave U7). Domain `overseas_stock` (`g`-prefix).
// Out-block keys/array-ness from the raw capture (KTD5): g3101/g3104/g3106 are
// single Object out-blocks; g3102/g3103/g3190 carry a header Object + an
// Object-Array detail (`…OutBlock1`). Canonical price/name field pinned by
// baseline `korean_name` from a NON-COLLAPSING fixture (price≠open≠high≠low),
// KTD6. Numeric request counts (`readcnt`/`cts_seq`) assert `.is_number()`,
// KTD4. The `01900` paper-incompatible disposition is covered explicitly on
// g3101 (representative): the member stays Tracked, no flip.
// ---------------------------------------------------------------------------

/// `g3101` request rename (no caller leak) + a NON-COLLAPSING success body:
/// `price` (현재가, canonical KTD6) is pinned exactly and is distinct from
/// open/high/low so a mislabel cannot hide.
#[test]
fn g3101_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(G3101Request::new("R", "82TSLA", "82", "TSLA"))
        .expect("serialize g3101");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "exactly one top-level key");
    assert_eq!(value["g3101InBlock"]["exchcd"], "82");
    assert_eq!(value["g3101InBlock"]["symbol"], "TSLA");
    assert!(value.get("g3101OutBlock").is_none(), "no out-block leaks into the request");

    let resp: G3101Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3101OutBlock": {
            "korname": "테슬라", "symbol": "TSLA", "price": "283.8200", "sign": "5",
            "diff": "1.1300", "volume": 414175, "open": "285.0900", "high": "285.3100",
            "low": "281.8400", "currency": "USD"
        }
    }))
    .expect("representative g3101 success must deserialize");
    // Canonical 현재가 pinned exactly, distinct from open/high/low (KTD6).
    assert_eq!(resp.outblock.price, "283.8200", "현재가");
    assert_eq!(resp.outblock.open, "285.0900", "시가");
    assert_eq!(resp.outblock.high, "285.3100", "고가");
    assert_eq!(resp.outblock.low, "281.8400", "저가");
    assert_ne!(resp.outblock.price, resp.outblock.open, "non-collapsing: price≠open");
    assert_eq!(resp.outblock.korname, "테슬라", "한글종목명");
}

/// `g3101` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn g3101_numeric_field_string_or_number() {
    let from_num: G3101OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 414175 })).expect("number form");
    let from_str: G3101OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "414175" })).expect("string form");
    assert_eq!(from_num.volume, "414175");
    assert_eq!(from_str.volume, "414175");
}

/// `g3101` empty result (00707, empty out-block) deserializes as the pending case.
#[test]
fn g3101_empty_result_deserializes_as_pending() {
    let empty: G3101Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty snapshot is the pending case");
}

/// `g3101` `01900` classifies as paper-incompatible — the member stays Tracked,
/// no flip (KTD5/disposition state machine). Representative for the lane.
#[tokio::test]
async fn g3101_code_01900_classifies_as_paper_incompatible() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/overseas-stock/market-data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rsp_cd": "01900",
            "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
        })))
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let err = sdk
        .market_session()
        .overseas_quote(&G3101Request::new("R", "82TSLA", "82", "TSLA"))
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

/// `g3106` rename + canonical 현재가 (`price`, KTD6) from a non-collapsing body.
#[test]
fn g3106_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(G3106Request::new("R", "82TSLA", "82", "TSLA"))
        .expect("serialize g3106");
    assert_eq!(value["g3106InBlock"]["symbol"], "TSLA");
    assert!(value.get("g3106OutBlock").is_none(), "no out-block leaks");

    let resp: G3106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "g3106OutBlock": {
            "korname": "테슬라", "symbol": "TSLA", "price": "283.0200", "sign": "5",
            "diff": "1.9300", "volume": 431173, "offerho1": "283.1100", "bidho1": "283.0200"
        }
    }))
    .expect("representative g3106 success must deserialize");
    assert_eq!(resp.outblock.price, "283.0200", "현재가");
    assert_eq!(resp.outblock.offerho1, "283.1100", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "283.0200", "매수호가1");
    assert_ne!(resp.outblock.offerho1, resp.outblock.bidho1, "non-collapsing: offer≠bid");
}

/// `g3106` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn g3106_numeric_field_string_or_number() {
    let from_num: G3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 431173 })).expect("number");
    let from_str: G3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "431173" })).expect("string");
    assert_eq!(from_num.volume, "431173");
    assert_eq!(from_str.volume, "431173");
}

/// `g3106` empty result (00707) deserializes as the pending case.
#[test]
fn g3106_empty_result_deserializes_as_pending() {
    let empty: G3106Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty order book is the pending case");
}

/// `o3105` rename + canonical 체결가격 (`trd_p`, KTD6) from a non-collapsing
/// single out-block; numeric out fields parse from BOTH forms (KTD4).
#[test]
fn o3105_request_renames_and_trade_price_round_trips() {
    let value = serde_json::to_value(O3105Request::new("CUSN23  ")).expect("serialize o3105");
    assert_eq!(value["o3105InBlock"]["symbol"], "CUSN23  ");
    assert!(value.get("o3105OutBlock").is_none(), "no out-block leaks");

    let resp: O3105Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3105OutBlock": {
            "Symbol": "CUSN23", "SymbolNm": "Renminbi_USD/CNH(2023.07)", "TrdP": "7.2011",
            "OpenP": "7.2081", "HighP": "7.2081", "LowP": "7.1907", "TotQ": 1011, "TrdQ": 1,
            "SeqNo": 1, "CrncyCd": "CNY"
        }
    }))
    .expect("representative o3105 success must deserialize");
    assert_eq!(resp.outblock.trd_p, "7.2011", "체결가격");
    assert_eq!(resp.outblock.open_p, "7.2081", "시가");
    assert_eq!(resp.outblock.low_p, "7.1907", "저가");
    assert_ne!(resp.outblock.trd_p, resp.outblock.open_p, "non-collapsing: 체결가≠시가");
}

/// `o3105` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3105_numeric_field_string_or_number() {
    let from_num: O3105OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": 1011 })).expect("number form");
    let from_str: O3105OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": "1011" })).expect("string form");
    assert_eq!(from_num.tot_q, "1011");
    assert_eq!(from_str.tot_q, "1011");
}

/// `o3105` empty result (00707) deserializes as the pending case.
#[test]
fn o3105_empty_result_deserializes_as_pending() {
    let empty: O3105Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.trd_p.is_empty(), "empty snapshot is the pending case");
}

/// `o3106` rename + canonical 현재가 (`price`, KTD6) from a non-collapsing single
/// out-block (book level-1 distinct from price).
#[test]
fn o3106_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(O3106Request::new("ADM23")).expect("serialize o3106");
    assert_eq!(value["o3106InBlock"]["symbol"], "ADM23");
    assert!(value.get("o3106OutBlock").is_none(), "no out-block leaks");

    let resp: O3106Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3106OutBlock": {
            "symbol": "ADM23", "symbolname": "Australian Dollar(2023.06)", "price": "0.67670",
            "change": "0.00135", "volume": 18844, "offerho1": "0.67670", "bidho1": "0.67665",
            "offer": 149, "bid": 220
        }
    }))
    .expect("representative o3106 success must deserialize");
    assert_eq!(resp.outblock.price, "0.67670", "현재가");
    assert_eq!(resp.outblock.offerho1, "0.67670", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "0.67665", "매수호가1");
    assert_ne!(resp.outblock.offerho1, resp.outblock.bidho1, "non-collapsing: offer≠bid");
}

/// `o3106` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3106_numeric_field_string_or_number() {
    let from_num: O3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 18844 })).expect("number form");
    let from_str: O3106OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "18844" })).expect("string form");
    assert_eq!(from_num.volume, "18844");
    assert_eq!(from_str.volume, "18844");
}

/// `o3106` empty result (00707) deserializes as the pending case.
#[test]
fn o3106_empty_result_deserializes_as_pending() {
    let empty: O3106Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty order book is the pending case");
}

/// `o3125` rename + canonical 체결가격 (`trd_p`, KTD6) from a non-collapsing
/// single out-block; the two-field in-block round-trips.
#[test]
fn o3125_request_renames_and_trade_price_round_trips() {
    let value =
        serde_json::to_value(O3125Request::new("F", "HSIM23          ")).expect("serialize o3125");
    assert_eq!(value["o3125InBlock"]["mktgb"], "F");
    assert_eq!(value["o3125InBlock"]["symbol"], "HSIM23          ");
    assert!(value.get("o3125OutBlock").is_none(), "no out-block leaks");

    let resp: O3125Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3125OutBlock": {
            "Symbol": "HSIM23", "SymbolNm": "Hang Seng(2023.06)", "TrdP": "18922.0",
            "OpenP": "18877.0", "HighP": "19022.0", "LowP": "18676.0", "TotQ": 93965, "TrdQ": 3,
            "SeqNo": 1, "CrncyCd": "HKD"
        }
    }))
    .expect("representative o3125 success must deserialize");
    assert_eq!(resp.outblock.trd_p, "18922.0", "체결가격");
    assert_eq!(resp.outblock.high_p, "19022.0", "고가");
    assert_eq!(resp.outblock.low_p, "18676.0", "저가");
    assert_ne!(resp.outblock.trd_p, resp.outblock.high_p, "non-collapsing: 체결가≠고가");
}

/// `o3125` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3125_numeric_field_string_or_number() {
    let from_num: O3125OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": 93965 })).expect("number form");
    let from_str: O3125OutBlock =
        serde_json::from_value(serde_json::json!({ "TotQ": "93965" })).expect("string form");
    assert_eq!(from_num.tot_q, "93965");
    assert_eq!(from_str.tot_q, "93965");
}

/// `o3125` empty result (00707) deserializes as the pending case.
#[test]
fn o3125_empty_result_deserializes_as_pending() {
    let empty: O3125Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.trd_p.is_empty(), "empty snapshot is the pending case");
}

/// `o3126` rename + canonical 현재가 (`price`, KTD6) from a non-collapsing single
/// out-block.
#[test]
fn o3126_request_renames_and_price_round_trips() {
    let value = serde_json::to_value(O3126Request::new("F", "ADM23")).expect("serialize o3126");
    assert_eq!(value["o3126InBlock"]["mktgb"], "F");
    assert_eq!(value["o3126InBlock"]["symbol"], "ADM23");
    assert!(value.get("o3126OutBlock").is_none(), "no out-block leaks");

    let resp: O3126Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3126OutBlock": {
            "symbol": "ADM23", "symbolname": "Australian Dollar(2023.06)", "price": "0.67670",
            "change": "0.00135", "volume": 18844, "offerho1": "0.67670", "bidho1": "0.67665",
            "offer": 150, "bid": 220
        }
    }))
    .expect("representative o3126 success must deserialize");
    assert_eq!(resp.outblock.price, "0.67670", "현재가");
    assert_eq!(resp.outblock.offerho1, "0.67670", "매도호가1");
    assert_eq!(resp.outblock.bidho1, "0.67665", "매수호가1");
    assert_ne!(resp.outblock.offerho1, resp.outblock.bidho1, "non-collapsing: offer≠bid");
}

/// `o3126` numeric out-block field parses from BOTH string and number JSON.
#[test]
fn o3126_numeric_field_string_or_number() {
    let from_num: O3126OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": 18844 })).expect("number form");
    let from_str: O3126OutBlock =
        serde_json::from_value(serde_json::json!({ "volume": "18844" })).expect("string form");
    assert_eq!(from_num.volume, "18844");
    assert_eq!(from_str.volume, "18844");
}

/// `o3126` empty result (00707) deserializes as the pending case.
#[test]
fn o3126_empty_result_deserializes_as_pending() {
    let empty: O3126Response =
        serde_json::from_value(serde_json::json!({ "rsp_cd": "00707" })).expect("empty");
    assert!(empty.outblock.price.is_empty(), "empty order book is the pending case");
}

// --- o3127 — 해외선물옵션 관심종목 (watchlist board) --------------------------

#[test]
fn o3127_request_serializes_nrec_as_number_and_carries_inblock1_symbol() {
    let value = serde_json::to_value(O3127Request::new("0", "CUSN26")).expect("serialize o3127");
    assert!(value["o3127InBlock"]["nrec"].is_number(), "nrec is a JSON number (IGW40011 guard)");
    let entries = value["o3127InBlock1"].as_array().expect("repeated InBlock1 is an array");
    assert_eq!(entries.len(), 1, "one watched symbol");
    assert_eq!(entries[0]["mktgb"], "0");
    assert_eq!(entries[0]["symbol"], "CUSN26", "the watched symbol is carried (a quote-bearing request)");
}

#[test]
fn o3127_single_or_array_and_empty_deserialize() {
    let single: O3127Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "o3127OutBlock": { "symbol": "CUSN26", "price": "0.66435", "volume": 100 }
    }))
    .expect("single board row tolerated as array");
    assert_eq!(single.outblock.len(), 1);
    assert_eq!(single.outblock[0].price, "0.66435");

    let empty: O3127Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "o3127OutBlock": []
    }))
    .expect("empty deserializes");
    assert!(empty.outblock.is_empty(), "empty board is the pending case");
}
