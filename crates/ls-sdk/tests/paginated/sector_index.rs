use super::*;


/// Covers R4, R8. The genuinely-numeric `cnt` serializes as a JSON **number**
/// (the string form returns `IGW40011`, confirmed by the U1 probe), while the
/// `cts_date` cursor and the identifier fields stay **strings**. Header cursors
/// are skipped (self-paginated body cursor).
#[test]
fn t1514_request_serializes_cnt_as_number_cts_date_as_string() {
    let value = serde_json::to_value(T1514Request::new("001")).expect("serialize t1514 request");
    let inblock = &value["t1514InBlock"];
    assert!(inblock["cnt"].is_number(), "cnt is a JSON number, not a string");
    assert!(
        inblock["cts_date"].is_string(),
        "cts_date cursor stays a string"
    );
    assert!(inblock["upcode"].is_string(), "upcode stays a string");
    assert_eq!(inblock["upcode"], "001");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R2, R5, R8. The first-page fixture deserializes through REAL paginated
/// dispatch: the `t1514OutBlock` cursor + the `t1514OutBlock1` array round-trip.
#[tokio::test]
async fn t1514_deserializes_spec_fixture() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(INDTP_PATH))
        .and(header("tr_cd", "t1514"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1514_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .sector_trend(&T1514Request::new("001"))
        .await
        .expect("t1514 sector_trend should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "first-page trend rows round-trip");
    assert!(!resp.outblock1[0].date.is_empty(), "real non-default date");
    assert_eq!(resp.outblock1[0].upcode, "001", "per-row sector code (string)");
}

/// Covers R5, R8. The trend array tolerates single-or-array + empty (pending) forms.
#[test]
fn t1514_response_round_trips_single_or_array_and_empty() {
    let single: T1514Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1514OutBlock": { "cts_date": "20230605" },
        "t1514OutBlock1": { "date": "20230605", "jisu": "2610.62", "volume": 263165, "upcode": "001" }
    }))
    .expect("single trend row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock1[0].volume, "263165", "volume from JSON number");

    let empty: T1514Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1514OutBlock": { "cts_date": "" }, "t1514OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty first page is the pending case");
}

/// Registration guard (origin R8): `T1514_POLICY` must be a real paginated policy
/// — a self-paginating TR shipping `has_pagination: false` would dispatch
/// single-page silently. The `policy_index_crosscheck` test enforces the
/// `self_paginated ⟹ has_pagination` mirror only for consts in its array; this
/// asserts the const's own shape so a regression fails a test, never silently.
#[test]
fn t1514_policy_is_registered_and_paginated() {
    assert_eq!(T1514_POLICY.tr_code, "t1514");
    assert_eq!(T1514_POLICY.path, "/indtp/market-data");
    assert!(
        T1514_POLICY.has_pagination,
        "t1514 self-paginates (cts_date) — policy must thread continuation"
    );
}
