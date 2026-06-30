use super::*;


/// Covers R3. The `t1809` request serializes every in-block field as a JSON
/// **string** (no `string_as_number`), with the `cts` cursor as an ORDINARY
/// in-block field at its first-page `"1"` convention (NOT `#[serde(skip)]`). The
/// 종목구분 filter rides under its EXACT wire key `jmGb` (capital `G`), and the
/// header continuation is skipped from the body.
#[test]
fn t1809_request_serializes_jmgb_under_wire_key_and_cts_cursor() {
    let value = serde_json::to_value(T1809Request::new("1", "1", "1")).expect("serialize t1809");
    let obj = value.as_object().expect("request is a JSON object");
    assert_eq!(obj.len(), 1, "only t1809InBlock at the top level");
    let inblock = &value["t1809InBlock"];
    // Every in-block field serializes as a JSON string.
    for f in ["gubun", "jmGb", "jmcode", "cts"] {
        assert!(inblock[f].is_string(), "{f} serializes as a JSON string");
    }
    // The 종목구분 filter MUST appear under the exact wire key `jmGb` (capital G),
    // never `jmgb`.
    assert!(
        inblock.get("jmGb").is_some(),
        "종목구분 serializes under the exact wire key `jmGb`"
    );
    assert!(
        inblock.get("jmgb").is_none(),
        "no lowercase `jmgb` key leaks (the wire casing is `jmGb`)"
    );
    // The cursor is present as an ordinary in-block field at first page `"1"` —
    // NOT skipped. A `#[serde(skip)]` cursor would make this key absent.
    assert!(
        inblock.get("cts").is_some(),
        "cts is an ordinary in-block field, not skipped"
    );
    assert_eq!(inblock["cts"], "1", "first-page cts cursor is the string \"1\", not absent");
    assert!(value.get("tr_cont").is_none(), "header cursor skipped from body");
    assert!(value.get("tr_cont_key").is_none(), "header cursor skipped from body");
}

/// Covers R4. The raw-capture fixture deserializes through REAL paginated dispatch:
/// the `t1809OutBlock` summary cursor + the TOP-LEVEL sibling `t1809OutBlock1` array
/// round-trip, and modeled non-key fields hold real (non-default) values.
#[tokio::test]
async fn t1809_deserializes_raw_capture_shape() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(STOCK_ITEM_SEARCH_PATH))
        .and(header("tr_cd", "t1809"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(T1809_FIXTURE)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    let resp = sdk_for(&server)
        .paginated()
        .signal_search(&T1809Request::new("1", "1", "1"))
        .await
        .expect("t1809 signal_search should succeed");
    assert_eq!(resp.rsp_cd, "00000");
    assert!(!resp.outblock1.is_empty(), "signal rows round-trip");
    assert_eq!(resp.outblock1[0].signal_desc, "급등주포착", "real non-default signal name");
    assert_eq!(resp.outblock1[0].jmcode, "005930", "real non-default signal short code");
    assert_eq!(resp.outblock1[0].price, "66100", "price from JSON number");
    assert_eq!(resp.outblock1[0].volume, "12345678", "volume from JSON number");
    assert_eq!(resp.outblock.cts, "20240101120000", "next-page cursor from the summary block");
}

/// Covers R4. The signal array tolerates single-or-array + empty (the empty
/// case, R7), and `string_or_number` parses numeric fields from BOTH string and
/// number.
#[test]
fn t1809_response_round_trips_single_or_array_and_empty() {
    let single: T1809Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1809OutBlock": { "cts": "20240101120000" },
        "t1809OutBlock1": { "signal_desc": "급등주포착", "jmcode": "005930", "price": "66100", "volume": "12345678" }
    }))
    .expect("single signal row tolerated as array");
    assert_eq!(single.outblock1.len(), 1);
    assert_eq!(single.outblock.cts, "20240101120000", "next-page cursor round-trips (non-empty)");
    assert_eq!(single.outblock1[0].price, "66100", "price from JSON string");

    let number: T1809Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00000",
        "t1809OutBlock": { "cts": "1" },
        "t1809OutBlock1": [{ "signal_desc": "급등주포착", "jmcode": "005930", "price": 66100, "chgrate": 1.85, "volume": 12345678 }]
    }))
    .expect("numeric price/chgrate/volume tolerated");
    assert_eq!(number.outblock1[0].volume, "12345678", "volume from JSON number");
    assert_eq!(number.outblock1[0].price, "66100", "price from JSON number");

    let empty: T1809Response = serde_json::from_value(serde_json::json!({
        "rsp_cd": "00707", "t1809OutBlock": { "cts": "1" }, "t1809OutBlock1": []
    }))
    .expect("empty result deserializes");
    assert!(empty.outblock1.is_empty(), "empty result is the pending case (R7)");
}

/// Registration guard (R3/R8): `T1809_POLICY` must be a real paginated policy.
#[test]
fn t1809_policy_is_registered_and_paginated() {
    assert_eq!(T1809_POLICY.tr_code, "t1809");
    assert_eq!(T1809_POLICY.path, "/stock/item-search");
    assert!(
        T1809_POLICY.has_pagination,
        "t1809 self-paginates (body cts) — policy must thread continuation"
    );
    assert!(!T1809_POLICY.is_order, "t1809 is a non-order read");
}
