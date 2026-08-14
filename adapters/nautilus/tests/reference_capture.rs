//! U2 offline integration: the reference-data capture against wiremock-served
//! bodies for the six TRs. No live calls. Covers the shcode join, resolution
//! transparency (AE4), the equities-only skeleton filter (R2), the designation
//! gate (AE3), and the `t1444` multi-page dedup walk.

use std::time::Duration;

use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::reference::capture::{capture, CaptureConfig, DesignationQuery};
use nautilus_ls::reference::universe_metadata::{
    CapTier, DesignationKind, IndexMembership, LiquidityTier, MarketClass, Resolved,
};
use wiremock::matchers::{body_partial_json, header, method};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

/// t8430 all-markets master: two KOSPI equities, one KOSPI ETF (dropped), two
/// KOSDAQ equities on the cap board, one below-board KOSDAQ equity, and both
/// preferred-share spellings (letter-suffixed `02826K`, numeric-coded `005935`)
/// — each dropped by a different half of the recorded filter.
fn t8430_body() -> serde_json::Value {
    let row = |hname: &str, shcode: &str, etfgubun: &str, gubun: &str| {
        serde_json::json!({
            "hname": hname, "shcode": shcode, "expcode": "KR0000000000",
            "etfgubun": etfgubun, "uplmtprice": "0", "dnlmtprice": "0",
            "jnilclose": "0", "memedan": "1", "recprice": "0", "gubun": gubun
        })
    };
    serde_json::json!({
        "rsp_cd": "00000", "rsp_msg": "정상",
        "t8430OutBlock": [
            row("삼성전자", "005930", "0", "1"),
            row("SK하이닉스", "000660", "", "1"),
            row("KODEX 200", "069500", "1", "1"),
            row("에코프로", "086520", "0", "2"),
            row("에코프로비엠", "247540", "0", "2"),
            row("소형주", "300000", "0", "2"),
            row("삼성물산우B", "02826K", "0", "1"),
            // P5: a numeric-coded preferred share (6th digit ≠ 0) — the class the
            // master exposes no flag for, dropped by the issue-sequence rule.
            row("삼성전자우", "005935", "0", "1"),
        ]
    })
}

fn t2522_body() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000", "rsp_msg": "정상",
        "t2522OutBlock": { "cnt": "1" },
        "t2522OutBlock1": [
            { "bsc_asts_nm": "삼성전자", "bsc_asts_is_cd": "005930",
              "bsc_asts_id": "KRDRVFUEQU", "nmc_is_shrt_cd": "111V3000" }
        ]
    })
}

fn t1904_body(constituent: &str) -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000", "rsp_msg": "정상",
        "t1904OutBlock": { "date": "20260710", "etfnum": "1" },
        "t1904OutBlock1": [
            { "shcode": constituent, "hname": "구성종목", "price": "1000",
              "sign": "2", "change": "10", "volume": "100", "pvalue": "1000000", "weight": "1.0" }
        ]
    })
}

fn t1444_page(rows: &[(&str, &str, &str)], next_idx: &str) -> serde_json::Value {
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|(hname, shcode, total)| {
            serde_json::json!({
                "hname": hname, "shcode": shcode, "price": "1000", "sign": "2",
                "change": "0", "diff": "0.0", "rate": "0.0", "volume": "1000",
                "total": total, "for_rate": "0", "vol_rate": "0"
            })
        })
        .collect();
    serde_json::json!({
        "rsp_cd": "00000", "rsp_msg": "정상",
        "t1444OutBlock": { "idx": next_idx },
        "t1444OutBlock1": out
    })
}

fn designation_body(block: &str, shcodes: &[&str], cts: &str) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = shcodes
        .iter()
        .map(|s| {
            serde_json::json!({
                "hname": "지정종목", "shcode": s, "price": "1000", "sign": "5",
                "change": "0", "diff": "0.0", "volume": "0", "date": "20260701", "edate": ""
            })
        })
        .collect();
    let mut body = serde_json::json!({ "rsp_cd": "00000", "rsp_msg": "정상" });
    body[format!("{block}OutBlock")] = serde_json::json!({ "cts_shcode": cts });
    body[format!("{block}OutBlock1")] = serde_json::Value::Array(rows);
    body
}

/// A capture config with zero pacing and exactly one category per gate TR (the
/// category enum is operator-confirmable; the join logic is what this tests).
fn test_config() -> CaptureConfig {
    let mut cfg = CaptureConfig::new("2026-07-10T01:00:00Z", "20260710");
    cfg.pace = Duration::ZERO;
    cfg.t1405_categories =
        vec![DesignationQuery { gubun: "0".into(), jongchk: "2".into(), kind: DesignationKind::Halt }];
    cfg.t1404_categories = vec![DesignationQuery {
        gubun: "0".into(),
        jongchk: "1".into(),
        kind: DesignationKind::Managed,
    }];
    cfg
}

async fn mount_common(server: &MockServer) {
    mount_token(server).await;
    Mock::given(method("POST"))
        .and(header("tr_cd", "t8430"))
        .respond_with(json_response(t8430_body()))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(header("tr_cd", "t2522"))
        .respond_with(json_response(t2522_body()))
        .mount(server)
        .await;
    // KODEX 200 holds 005930; KODEX KOSDAQ150 holds 086520.
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1904"))
        .and(body_partial_json(serde_json::json!({"t1904InBlock": {"shcode": "069500"}})))
        .respond_with(json_response(t1904_body("005930")))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1904"))
        .and(body_partial_json(serde_json::json!({"t1904InBlock": {"shcode": "229200"}})))
        .respond_with(json_response(t1904_body("086520")))
        .mount(server)
        .await;
    // Gate: t1405 halts 247540; t1404 manages 000660. Single page each.
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1405"))
        .respond_with(json_response(designation_body("t1405", &["247540"], "")))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1404"))
        .respond_with(json_response(designation_body("t1404", &["000660"], "")))
        .mount(server)
        .await;
}

#[tokio::test]
async fn capture_joins_six_trs_by_shcode_into_resolved_records() {
    let server = MockServer::start().await;
    mount_common(&server).await;
    // Single-page cap boards: KOSPI ranks 005930 > 000660; KOSDAQ 086520 > 247540.
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1444"))
        .and(body_partial_json(serde_json::json!({"t1444InBlock": {"upcode": "001"}})))
        .respond_with(json_response(t1444_page(
            &[("삼성전자", "005930", "4000000"), ("SK하이닉스", "000660", "3000000")],
            "",
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1444"))
        .and(body_partial_json(serde_json::json!({"t1444InBlock": {"upcode": "301"}})))
        .respond_with(json_response(t1444_page(
            &[("에코프로", "086520", "200000"), ("에코프로비엠", "247540", "100000")],
            "",
        )))
        .mount(&server)
        .await;

    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");
    let outcome = capture(&sdk, &test_config()).await.expect("capture joins");
    let artifact = outcome.artifact;

    // The ETF master row, the letter-suffixed preferred share, and the
    // numeric-coded preferred share are all dropped (R2/P5); five common
    // equities remain, sorted by shcode.
    let codes: Vec<&str> = artifact.records.iter().map(|r| r.shcode.as_str()).collect();
    assert_eq!(codes, ["000660", "005930", "086520", "247540", "300000"]);
    // P5: the issue-sequence drop is recorded, not merely applied — this is the
    // provenance half of the filter, and the only place the live capture path
    // (rather than the pure classifier) is asserted to populate it.
    assert_eq!(artifact.provenance.dropped_preferred, ["005935"]);
    assert!(
        artifact.provenance.instrument_type_filter.contains("issue-sequence digit"),
        "the recorded filter declares the rule it applied"
    );
    let by = |code: &str| artifact.records.iter().find(|r| r.shcode == code).unwrap();

    // Market class from the master gubun.
    assert_eq!(by("005930").market_class, MarketClass::Kospi);
    assert_eq!(by("086520").market_class, MarketClass::Kosdaq);

    // Derivative flag: on the t2522 underlying set → Value(true); a symbol the
    // set was read for but does not contain → Value(false), never Unavailable.
    assert_eq!(by("005930").has_derivative, Resolved::Value(true));
    assert_eq!(by("000660").has_derivative, Resolved::Value(false));

    // Index proxy (AE4): holdings → Proxy(index); absent from both served ETF
    // reads → Proxy(NotMember) — a disclosed proxy, not a confident false.
    assert_eq!(by("005930").index_membership, Resolved::Proxy(IndexMembership::Kospi200));
    assert_eq!(by("086520").index_membership, Resolved::Proxy(IndexMembership::Kosdaq150));
    assert_eq!(by("000660").index_membership, Resolved::Proxy(IndexMembership::NotMember));

    // Cap tiers: per-market top-half → Top; below-board → BelowBoard + Unavailable.
    assert_eq!(by("005930").cap_tier, CapTier::Top);
    assert_eq!(by("000660").cap_tier, CapTier::Mid);
    assert_eq!(by("086520").cap_tier, CapTier::Top);
    assert_eq!(by("247540").cap_tier, CapTier::Mid);
    assert_eq!(by("300000").cap_tier, CapTier::BelowBoard);
    assert!(by("300000").market_cap.is_unavailable());
    assert_eq!(by("005930").market_cap, Resolved::Value(4_000_000.0));

    // Turnover is deferred this turn (R2): Unavailable, liquidity Unknown.
    for r in &artifact.records {
        assert!(r.turnover.is_unavailable(), "{}: turnover deferred", r.shcode);
        assert_eq!(r.liquidity_tier, LiquidityTier::Unknown);
    }

    // The gate (AE3): the halted and managed names are excluded from the
    // tradeable set; clean names pass.
    let halted = by("247540");
    assert!(!halted.tradable);
    assert_eq!(halted.designation.as_ref().unwrap().kind, DesignationKind::Halt);
    assert_eq!(halted.designation.as_ref().unwrap().source_tr, "t1405");
    let managed = by("000660");
    assert!(!managed.tradable);
    assert_eq!(managed.designation.as_ref().unwrap().kind, DesignationKind::Managed);
    assert!(by("005930").tradable);
    assert!(by("300000").tradable);

    // Every source served → no paper-incompatible records; provenance complete.
    assert!(artifact.provenance.paper_incompatible.is_empty());
    assert_eq!(artifact.provenance.session_date, "20260710");
    assert_eq!(artifact.provenance.cap_cutoffs.len(), 2);
    assert!(artifact.provenance.tier_boundary_rule.contains("jongchk=2"), "queried categories recorded");
}

#[tokio::test]
async fn t1444_multi_page_walk_dedups_on_shcode() {
    let server = MockServer::start().await;
    mount_common(&server).await;
    // KOSPI board pages on the body idx cursor: page 1 (idx=0) serves A+B with a
    // live cursor; page 2 (idx=2) re-serves B (the overlap) plus C, terminal.
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1444"))
        .and(body_partial_json(serde_json::json!({"t1444InBlock": {"upcode": "001"}})))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            let idx = body["t1444InBlock"]["idx"].as_i64().unwrap_or(0);
            if idx == 0 {
                json_response(t1444_page(
                    &[("삼성전자", "005930", "4000000"), ("SK하이닉스", "000660", "3000000")],
                    "2",
                ))
                .insert_header("tr_cont", "Y")
            } else {
                json_response(t1444_page(
                    &[("SK하이닉스", "000660", "3000000"), ("현대차", "005380", "2000000")],
                    "",
                ))
                .insert_header("tr_cont", "N")
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1444"))
        .and(body_partial_json(serde_json::json!({"t1444InBlock": {"upcode": "301"}})))
        .respond_with(json_response(t1444_page(&[("에코프로", "086520", "200000")], "")))
        .mount(&server)
        .await;

    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");
    let mut cfg = test_config();
    // 005380 is not in the t8430 master body — the walk still dedups; the join
    // simply has no skeleton row for it.
    cfg.cap_boards[0].max_rows = 3;
    let outcome = capture(&sdk, &cfg).await.expect("capture walks pages");
    let artifact = outcome.artifact;

    let by = |code: &str| artifact.records.iter().find(|r| r.shcode == code).unwrap();
    // The overlapped 000660 resolves once, to its first-seen cap value.
    assert_eq!(by("000660").market_cap, Resolved::Value(3_000_000.0));
    assert_eq!(by("005930").market_cap, Resolved::Value(4_000_000.0));
    // A cap row with no master row never fabricates a record.
    assert!(artifact.records.iter().all(|r| r.shcode != "005380"));
}

#[tokio::test]
async fn a_failed_reference_tr_is_recorded_not_silently_dropped() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(header("tr_cd", "t8430"))
        .respond_with(json_response(t8430_body()))
        .mount(&server)
        .await;
    // t2522 fails with an API error envelope; everything else serves.
    Mock::given(method("POST"))
        .and(header("tr_cd", "t2522"))
        .respond_with(json_response(serde_json::json!({
            "rsp_cd": "IGW00121", "rsp_msg": "유효하지 않은 요청"
        })))
        .mount(&server)
        .await;
    for (tr, body) in [
        ("t1904", t1904_body("005930")),
        ("t1405", designation_body("t1405", &[], "")),
        ("t1404", designation_body("t1404", &[], "")),
    ] {
        Mock::given(method("POST"))
            .and(header("tr_cd", tr))
            .respond_with(json_response(body))
            .mount(&server)
            .await;
    }
    Mock::given(method("POST"))
        .and(header("tr_cd", "t1444"))
        .respond_with(json_response(t1444_page(&[("삼성전자", "005930", "4000000")], "")))
        .mount(&server)
        .await;

    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");
    let outcome = capture(&sdk, &test_config()).await.expect("capture tolerates one failed source");
    let artifact = outcome.artifact;

    // The failure is recorded with its code (Success Criteria: no silent holes)…
    assert!(
        artifact
            .provenance
            .paper_incompatible
            .iter()
            .any(|f| f.tr == "t2522" && f.code == "IGW00121"),
        "{:?}",
        artifact.provenance.paper_incompatible
    );
    // …and the affected attribute resolves Unavailable, never a defaulted false (R4).
    for r in &artifact.records {
        assert!(r.has_derivative.is_unavailable(), "{}: derivative unresolved", r.shcode);
    }
}

#[tokio::test]
async fn a_whole_board_jongchk_zero_category_is_refused_before_any_call() {
    // Pre-flight finding (2026-07-10): t1404 jongchk="0" returns every listed
    // issue — treating it as a designation category would mark the whole
    // market non-tradable. The capture fails closed before any gateway call
    // (no mocks mounted: a dispatched request would error differently).
    let server = MockServer::start().await;
    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");
    let mut cfg = test_config();
    cfg.t1404_categories = vec![DesignationQuery {
        gubun: "0".into(),
        jongchk: "0".into(),
        kind: DesignationKind::Managed,
    }];
    let err = capture(&sdk, &cfg).await.unwrap_err();
    assert_eq!(err.calls_made, 0, "refused before any gateway call");
    let msg = err.to_string();
    assert!(msg.contains("whole board"), "{msg}");
    assert!(msg.contains("t1404"), "{msg}");
}
