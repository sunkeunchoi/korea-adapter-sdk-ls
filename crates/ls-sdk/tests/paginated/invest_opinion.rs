use super::*;


// --- t3401 — 투자의견 --------------------------------------------------------

#[test]
fn t3401_request_serializes_to_inblock() {
    let value = serde_json::to_value(T3401Request::new("011200")).expect("serialize t3401 request");
    let inblock = &value["t3401InBlock"];
    assert_eq!(inblock["shcode"], "011200");
    assert!(inblock["cts_date"].is_string(), "cts_date cursor stays a string");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
}

#[tokio::test]
async fn t3401_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_INVESTINFO_PATH))
        .and(header("tr_cd", "t3401"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T3401_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .investment_opinions(&T3401Request::new("011200"))
        .await
        .expect("t3401 should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(resp.outblock1.len() >= 2, "opinion rows round-trip");
    assert_eq!(resp.outblock1[0].bopn, "HOLD", "canonical 투자의견변경후");
    assert_eq!(resp.outblock1[0].tradname, "메리츠", "회원사명");
    assert_eq!(resp.outblock1[0].noga, "24000", "목표가변경후 from JSON number");
}

#[test]
fn t3401_response_round_trips_single_or_array_and_empty() {
    let single: T3401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t3401OutBlock": { "price": 17800 },
        "t3401OutBlock1": { "date": "20230209", "bopn": "BUY", "shcode": "011200", "noga": 24000 }
    }))
    .expect("single opinion row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].bopn, "BUY");

    let empty: T3401Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t3401OutBlock": {}, "t3401OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty is the pending case");
}
