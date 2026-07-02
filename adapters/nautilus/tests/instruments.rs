//! U2 offline integration: the instrument provider against wiremock-served master
//! bodies (t8430 + t9945). No live calls. Covers AE3 (unsupported-domain refusal).

use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::types::Price;
use ustr::Ustr;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const T8430_PATH: &str = "/stock/etc";
const T9945_PATH: &str = "/stock/market-data";

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

/// t8430 all-markets stock-issue list: a KOSPI equity, a KOSPI ETF, a KOSDAQ name.
fn t8430_body() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000",
        "rsp_msg": "정상",
        "t8430OutBlock": [
            {
                "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
                "etfgubun": "0", "uplmtprice": "78000", "dnlmtprice": "42000",
                "jnilclose": "60000", "memedan": "1", "recprice": "60000", "gubun": "1"
            },
            {
                "hname": "KODEX 200", "shcode": "069500", "expcode": "KR7069500007",
                "etfgubun": "1", "uplmtprice": "45000", "dnlmtprice": "25000",
                "jnilclose": "35000", "memedan": "1", "recprice": "35000", "gubun": "1"
            },
            {
                "hname": "에코프로", "shcode": "086520", "expcode": "KR7086520004",
                "etfgubun": "0", "uplmtprice": "130000", "dnlmtprice": "70000",
                "jnilclose": "100000", "memedan": "1", "recprice": "100000", "gubun": "2"
            }
        ]
    })
}

/// t9945 stock-master enrichment (ISIN + NXT flag). Returned for both gubun calls.
fn t9945_body() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000",
        "rsp_msg": "정상",
        "t9945OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfchk": "0", "nxt_chk": "1", "filler": "" },
            { "hname": "KODEX 200", "shcode": "069500", "expcode": "KR7069500007",
              "etfchk": "1", "nxt_chk": "0", "filler": "" },
            { "hname": "에코프로", "shcode": "086520", "expcode": "KR7086520004",
              "etfchk": "0", "nxt_chk": "0", "filler": "" }
        ]
    })
}

async fn provider_over_mock(server: &MockServer) -> InstrumentProvider {
    mount_token(server).await;
    Mock::given(method("POST"))
        .and(path(T8430_PATH))
        .and(header("tr_cd", "t8430"))
        .respond_with(json_response(t8430_body()))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(T9945_PATH))
        .and(header("tr_cd", "t9945"))
        .respond_with(json_response(t9945_body()))
        .mount(server)
        .await;
    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");
    InstrumentProvider::new(sdk)
}

#[tokio::test]
async fn loads_domestic_universe_from_masters() {
    let server = MockServer::start().await;
    let mut provider = provider_over_mock(&server).await;

    let count = provider
        .load_domain(InstrumentDomain::DomesticEquity)
        .await
        .expect("domestic equities load");
    assert_eq!(count, 3, "three master rows mapped");
    assert_eq!(provider.len(), 3);

    // Samsung: id, ISIN from t9945, precision 0, tick 100 (60k KOSPI post-2023).
    let samsung = provider
        .get(&InstrumentId::from("005930.XKRX"))
        .expect("samsung cached");
    assert_eq!(samsung.isin, Some(Ustr::from("KR7005930003")));
    assert_eq!(samsung.price_precision, 0);
    assert_eq!(samsung.price_increment, Price::from("100"));
    let info = samsung.info.clone().expect("info");
    assert_eq!(info.get_bool("nxt_listed"), Some(true));
    assert_eq!(info.get_str("market"), Some("KOSPI"));
}

#[tokio::test]
async fn etf_row_maps_as_equity_with_flag() {
    let server = MockServer::start().await;
    let mut provider = provider_over_mock(&server).await;
    provider
        .load_domain(InstrumentDomain::DomesticEquity)
        .await
        .unwrap();

    let etf = provider
        .get(&InstrumentId::from("069500.XKRX"))
        .expect("etf cached");
    // Still an Equity, flagged ETF in info (from both t8430 etfgubun and t9945 etfchk).
    assert_eq!(etf.info.clone().unwrap().get_bool("etf"), Some(true));
    // 35,000 KRW post-2023 → tick 50 (20k-50k band).
    assert_eq!(etf.price_increment, Price::from("50"));
}

#[tokio::test]
async fn kosdaq_row_routes_to_kosdaq_market() {
    let server = MockServer::start().await;
    let mut provider = provider_over_mock(&server).await;
    provider
        .load_domain(InstrumentDomain::DomesticEquity)
        .await
        .unwrap();

    let ecopro = provider
        .get(&InstrumentId::from("086520.XKRX"))
        .expect("kosdaq name cached");
    assert_eq!(ecopro.info.clone().unwrap().get_str("market"), Some("KOSDAQ"));
    // 100,000 KRW post-2023 → tick 100 (50k-200k band, unified).
    assert_eq!(ecopro.price_increment, Price::from("100"));
}

/// Covers AE3: requesting an overseas or F/O domain returns an explicit
/// unsupported-domain error rather than mapping as an equity.
#[tokio::test]
async fn unsupported_domain_is_refused_explicitly() {
    let server = MockServer::start().await;
    let mut provider = provider_over_mock(&server).await;

    for domain in [
        InstrumentDomain::OverseasStock,
        InstrumentDomain::OverseasFo,
        InstrumentDomain::DomesticFo,
    ] {
        let err = provider
            .load_domain(domain)
            .await
            .expect_err("non-equity domain must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("unsupported instrument domain"),
            "error should name the unsupported domain: {msg}"
        );
    }
    // The refusal never cached anything.
    assert!(provider.is_empty());
}
