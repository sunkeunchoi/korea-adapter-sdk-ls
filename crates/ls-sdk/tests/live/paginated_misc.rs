use super::*;


/// `make live-smoke-t1310`: paper guard → token → one `t1310` today/prev tick-or-min
/// chart read for `shcode="005930"` (daygb/timegb=0, exchgubun=K). A NON-EMPTY tick
/// array proves the typed paginated read round-trips; an empty `00707` (which a
/// closed-window historical pull may return) does NOT record — it dispositions to
/// PENDING (R5/R6). The recorded line is credential-free (`rsp_cd` + row count, never
/// `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1310`"]
async fn live_smoke_t1310() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1310_SHCODE").unwrap_or_else(|_| "005930".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().daily_tick_chart(&T1310Request::new(&shcode)).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1310: empty ticks (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ticks")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1310", &format!("env=paper shcode={shcode} date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1310 market-data failure (not evidence)");
            panic!("live-smoke-t1310 failed: {e}");
        }
    }
}

/// `make live-smoke-t1404`: paper guard → token → one `t1404` administrative-
/// designation board read (gubun=0, jongchk=1, first-page cts_shcode). A NON-EMPTY
/// designation array proves the read round-trips; an empty `00707` (the concrete
/// `t1404` empty-board risk, R7) does NOT record — it dispositions to empty-board
/// PENDING. The recorded line is credential-free (`rsp_cd` + row count, never
/// `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1404`"]
async fn live_smoke_t1404() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().designation_board(&T1404Request::new()).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1404: empty board (00707) — PENDING, not Implemented (R7)"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1404", &format!("env=paper gubun=0 jongchk=1 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1404 market-data failure (not evidence)");
            panic!("live-smoke-t1404 failed: {e}");
        }
    }
}

/// `make live-smoke-t1410`: paper guard → token → one `t1410` ultra-low-liquidity
/// board read (gubun=0, first-page cts_shcode=""). A NON-EMPTY low-liquidity array
/// proves the read round-trips; an empty `00707` (the concrete `t1410` empty-board
/// risk, R7) does NOT record — it dispositions to empty-board PENDING. The recorded
/// line is credential-free (`rsp_cd` + row count, never `rsp_msg`); a failed run
/// emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1410`"]
async fn live_smoke_t1410() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().low_liquidity_board(&T1410Request::new()).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1410: empty board (00707) — PENDING, not Implemented (R7)"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1410", &format!("env=paper gubun=0 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1410 market-data failure (not evidence)");
            panic!("live-smoke-t1410 failed: {e}");
        }
    }
}

/// `make live-smoke-t1809`: paper guard → token → one `t1809` signal-search read
/// (gubun=1, jmGb=1, jmcode=1, first-page cts="1"). A NON-EMPTY signal array
/// proves the read round-trips; an empty `00707` (R7) does NOT record — it
/// dispositions to empty-result PENDING. The recorded line is credential-free
/// (`rsp_cd` + row count, never `rsp_msg`); a failed run emits a `SMOKE-FAIL`
/// stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1809`"]
async fn live_smoke_t1809() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().signal_search(&T1809Request::new("1", "1", "1")).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1809: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // Assert a non-default first-row field before recording (proves a real
            // round-trip, not a serde(default) Ok): the signal id is always present
            // on a signal row (`jmcode` can be blank for aggregate/header signals).
            assert!(
                !resp.outblock1[0].signal_id.is_empty(),
                "live-smoke-t1809: first row must carry a real signal id"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1809", &format!("env=paper gubun=1 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1809 market-data failure (not evidence)");
            panic!("live-smoke-t1809 failed: {e}");
        }
    }
}

/// `make live-smoke-t1109`: paper guard → token → one `t1109` after-hours tick
/// conclusion read (shcode=005930, first-page dan_chetime="", idx=0 as a JSON
/// number). A NON-EMPTY tick array proves the read round-trips; an empty `00707`
/// (R7) does NOT record — it dispositions to empty-result PENDING. The recorded
/// line is credential-free (`rsp_cd` + row count, never `rsp_msg`); a failed run
/// emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1109`"]
async fn live_smoke_t1109() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().after_hours_ticks(&T1109Request::new("005930")).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1109: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // NAMED market-data witness (price), not a status/count field.
            assert!(
                !resp.outblock1[0].dan_price.is_empty(),
                "live-smoke-t1109: first row must carry a real dan_price"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ticks")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1109", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1109 market-data failure (not evidence)");
            panic!("live-smoke-t1109 failed: {e}");
        }
    }
}

/// `make live-smoke-t1301`: paper guard → token → one `t1301` time-band tick
/// conclusion read (shcode=005930, cvolume=0 as a JSON number, starttime=0900
/// endtime=1530, first-page cts_time=""). A NON-EMPTY tick array proves the read
/// round-trips; an empty `00707` (R7) does NOT record. Credential-free line; a
/// failed run emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1301`"]
async fn live_smoke_t1301() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .time_band_ticks(&T1301Request::new("005930", "0900", "1530"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1301: empty result (00707) — PENDING, not Implemented (R7)"
            );
            assert!(
                !resp.outblock1[0].price.is_empty(),
                "live-smoke-t1301: first row must carry a real price"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ticks")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1301", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1301 market-data failure (not evidence)");
            panic!("live-smoke-t1301 failed: {e}");
        }
    }
}

/// `make live-smoke-t1486`: paper guard → token → one `t1486` expected-conclusion
/// read (shcode=005930, first-page cts_time="", cnt=20 as a JSON number,
/// exchgubun=1). A NON-EMPTY expected array proves the read round-trips; an empty
/// `00707` (R7) does NOT record. NOTE 예상체결 mainly populates during auction
/// phases — a continuous-session empty dispositions to PENDING. Credential-free
/// line; a failed run emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1486`"]
async fn live_smoke_t1486() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().expected_ticks(&T1486Request::new("005930", "1")).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1486: empty result (00707/auction-only) — PENDING, not Implemented (R7)"
            );
            assert!(
                !resp.outblock1[0].price.is_empty(),
                "live-smoke-t1486: first row must carry a real price"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1486", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1486 market-data failure (not evidence)");
            panic!("live-smoke-t1486 failed: {e}");
        }
    }
}

/// `make live-smoke-t8454`: paper guard → token → one `t8454` exchange-qualified
/// time-band tick conclusion read (shcode=005930, cvolume=0 as a JSON number,
/// starttime=0900 endtime=1530, first-page cts_time="", exchgubun=1). A NON-EMPTY
/// tick array proves the read round-trips; an empty `00707` (R7) does NOT record.
/// Credential-free line; a failed run emits a `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8454`"]
async fn live_smoke_t8454() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .time_band_ticks_ex(&T8454Request::new("005930", "0900", "1530", "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8454: empty result (00707) — PENDING, not Implemented (R7)"
            );
            assert!(
                !resp.outblock1[0].price.is_empty(),
                "live-smoke-t8454: first row must carry a real price"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ticks")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8454", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8454 market-data failure (not evidence)");
            panic!("live-smoke-t8454 failed: {e}");
        }
    }
}

/// `make live-smoke-t1637`: paper guard → token → one `t1637` per-stock
/// program-trade flow read (gubun1=0, gubun2=0, shcode=005930, date=today,
/// exchgubun=1, first-page cts_idx=0 as a JSON number). A NON-EMPTY program-flow
/// array proves the read round-trips; an empty `00707` (R7) does NOT record.
/// Credential-free line; a failed run emits a `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1637`"]
async fn live_smoke_t1637() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let yyyymmdd = Utc::now().format("%Y%m%d").to_string();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .program_trade_flow(&T1637Request::new("0", "0", "005930", &yyyymmdd, "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1637: empty result (00707) — PENDING, not Implemented (R7)"
            );
            assert!(
                !resp.outblock1[0].price.is_empty(),
                "live-smoke-t1637: first row must carry a real price"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1637", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1637 market-data failure (not evidence)");
            panic!("live-smoke-t1637 failed: {e}");
        }
    }
}

/// `make live-smoke-t1602`: paper guard → token → one `t1602` time-band investor
/// flow read (market=1, upcode=001, gubun1=1, gubun2=0, exchgubun=1; first-page
/// cts_time="", cts_idx=0/cnt=20 as JSON numbers). A NON-EMPTY flow array proves
/// the read round-trips; an empty `00707` (R7) does NOT record. Credential-free
/// line; a failed run emits a `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1602`"]
async fn live_smoke_t1602() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .investor_flow_time_band(&T1602Request::new("1", "001", "1", "0", "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1602: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // NAMED market-data witness (sv_17, foreign net-buy), not a status/count.
            assert!(
                !resp.outblock1[0].sv_17.is_empty(),
                "live-smoke-t1602: first row must carry a real sv_17"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1602", &format!("env=paper market=1 upcode=001 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1602 market-data failure (not evidence)");
            panic!("live-smoke-t1602 failed: {e}");
        }
    }
}

/// `make live-smoke-t1603`: paper guard → token → one `t1603` investor-detail read
/// (market=1, gubun1=1, gubun2=0, upcode=001, exchgubun=1; first-page cts_time="",
/// cts_idx=0/cnt=20 as JSON numbers). A NON-EMPTY array proves the read
/// round-trips; an empty `00707` (R7) does NOT record. Credential-free line; a
/// failed run emits a `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1603`"]
async fn live_smoke_t1603() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .investor_detail(&T1603Request::new("1", "1", "0", "001", "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1603: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // NAMED market-data witness (msvolume), not a status/count.
            assert!(
                !resp.outblock1[0].msvolume.is_empty(),
                "live-smoke-t1603: first row must carry a real msvolume"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1603", &format!("env=paper market=1 upcode=001 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1603 market-data failure (not evidence)");
            panic!("live-smoke-t1603 failed: {e}");
        }
    }
}

/// `make live-smoke-t1617`: paper guard → token → one `t1617` investor time/daily
/// flow read (gubun1=1, gubun2=1, gubun3=1, exchgubun=1; first-page cts_date="",
/// cts_time="" — all-String request). A NON-EMPTY array proves the read
/// round-trips; an empty `00707` (R7) does NOT record. Credential-free line; a
/// failed run emits a `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1617`"]
async fn live_smoke_t1617() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .investor_flow_daily(&T1617Request::new("1", "1", "1", "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1617: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // NAMED market-data witness (sv_17, foreign net-buy), not a status/count.
            assert!(
                !resp.outblock1[0].sv_17.is_empty(),
                "live-smoke-t1617: first row must carry a real sv_17"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1617", &format!("env=paper gubun1=1 gubun3=1 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1617 market-data failure (not evidence)");
            panic!("live-smoke-t1617 failed: {e}");
        }
    }
}

/// `make live-smoke-t1752`: paper guard → token → one `t1752` broker-by-issue read
/// (shcode=005930, traddate1/2=today, fwgubun1=0, exchgubun=1; first-page cts_idx=0
/// as a JSON number). A NON-EMPTY broker array proves the read round-trips; an
/// empty `00707` (R7) does NOT record. Credential-free line; a failed run emits a
/// `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1752`"]
async fn live_smoke_t1752() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let yyyymmdd = Utc::now().format("%Y%m%d").to_string();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .broker_by_issue(&T1752Request::new("005930", &yyyymmdd, &yyyymmdd, "0", "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1752: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // NAMED market-data witness (tradmsvol, member buy quantity).
            assert!(
                !resp.outblock1[0].tradmsvol.is_empty(),
                "live-smoke-t1752: first row must carry a real tradmsvol"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1752", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1752 market-data failure (not evidence)");
            panic!("live-smoke-t1752 failed: {e}");
        }
    }
}

/// `make live-smoke-t1771`: paper guard → token → one `t1771` broker time-series
/// read (shcode=005930, tradno="", gubun1=0, traddate1/2=today, exchgubun=1;
/// first-page cts_idx=0/cnt=20 as JSON numbers; row array under `t1771OutBlock2`).
/// A NON-EMPTY array proves the read round-trips; an empty `00707` (R7) does NOT
/// record. Credential-free line; a failed run emits a `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1771`"]
async fn live_smoke_t1771() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let yyyymmdd = Utc::now().format("%Y%m%d").to_string();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .broker_time_series(&T1771Request::new("005930", "", "0", &yyyymmdd, &yyyymmdd, "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock2.is_empty(),
                "live-smoke-t1771: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // NAMED market-data witness (price), not a status/count.
            assert!(
                !resp.outblock2[0].price.is_empty(),
                "live-smoke-t1771: first row must carry a real price"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1771", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1771 market-data failure (not evidence)");
            panic!("live-smoke-t1771 failed: {e}");
        }
    }
}

/// `make live-smoke-t1514`: paper guard → OAuth token → one first-page `t1514`
/// period-trend read for `upcode="001"`. Self-paginated (`cts_date` cursor, `cnt`
/// serialized as a number); a non-empty first page proves the paginated path.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1514`"]
async fn live_smoke_t1514() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1514Request::new("001");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().sector_trend(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1514",
                &format!("env=paper upcode=001 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1514 market-data failure (not evidence)");
            panic!("live-smoke-t1514 failed: {e}");
        }
    }
}

/// `make live-smoke-t3401`: paper guard → token → one `t3401` investment-opinion
/// read for `shcode=011200`. A non-empty opinion array proves the read round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3401`"]
async fn live_smoke_t3401() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T3401Request::new("011200");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().investment_opinions(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t3401: empty opinions (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "opinions")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t3401", &format!("env=paper shcode=011200 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3401 market-data failure (not evidence)");
            panic!("live-smoke-t3401 failed: {e}");
        }
    }
}

/// `make live-smoke-t3518`: paper guard → suppressor → token → one `t3518`
/// overseas-index time-series read (`kind="S"`, `symbol="NAS@IXIC"`). Flip gate
/// (R4): the first index-tick row's `price` (현재지수) must be a substantive
/// (non-default) value via [`assert_nonempty_witness`]; an empty out-block is the
/// `00707` PENDING case. The dispatch-log suppressor (U2/KTD5) drops any
/// account-bearing debug body; the panic path scrubs untrusted error text.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3518`"]
async fn live_smoke_t3518() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T3518Request::new("S", "NAS@IXIC");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().overseas_index_series(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t3518: empty out-block (00707) — PENDING, not Implemented"
            );
            assert_nonempty_witness("price", &resp.outblock1[0].price)
                .expect("live-smoke-t3518: index price must be a substantive witness (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "index-series")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t3518", &format!("env=paper kind=S symbol=NAS@IXIC date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3518 market-data failure (not evidence)");
            panic!("live-smoke-t3518 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t2214`: t8467 front-month source → one page of `t2214` F/O
/// daily OHLCV. Witness: a non-empty daily row (`close`/`volume`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2214`"]
async fn live_smoke_t2214() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = fo_front_month_shcode(&sdk, "live-smoke-t2214").await;
    let date = Utc::now().format("%Y%m%d").to_string();
    match sdk.paginated().fo_daily_chart(&T2214Request::new(&shcode, &date)).await {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.close.is_empty() || !r.volume.is_empty()),
                "live-smoke-t2214: empty daily chart (00707/off-data) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "fo-daily")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t2214", &format!("env=paper shcode={shcode} date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2214 market-data failure (not evidence)");
            panic!("live-smoke-t2214 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t2541`: one page of `t2541` F/O investor-by-time net-buy for
/// `upcode="001"`. Witness: a non-empty per-time row (`sv_17` 외국인순매수) OR the
/// summary `ms_08` 개인매수.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2541`"]
async fn live_smoke_t2541() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().fo_investor_by_time(&T2541Request::new("001")).await {
        Ok(resp) => {
            let row_ok = resp.outblock1.first().is_some_and(|r| !r.sv_17.is_empty() || !r.sv_08.is_empty());
            let sum_ok = !resp.outblock.ms_08.is_empty() || !resp.outblock.svolume_17.is_empty();
            assert!(
                row_ok || sum_ok,
                "live-smoke-t2541: empty/no-witness investor data (00707/off-data) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "fo-investor")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t2541", &format!("env=paper eitem=01 market=1 upcode=001 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2541 market-data failure (not evidence)");
            panic!("live-smoke-t2541 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}
