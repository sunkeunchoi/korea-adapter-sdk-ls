use super::*;


/// `make live-smoke-chart`: paper guard → offline date validation → one `t8412`
/// page (never `chart_all`). Covers AE3 (missing date), AE5 (gateway holiday).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials + a trading day; run via `make live-smoke-chart`"]
async fn live_smoke_chart() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let symbol = resolve_symbol();

    let raw_date = std::env::var("LS_LIVE_SMOKE_T8412_DATE")
        .expect("LS_LIVE_SMOKE_T8412_DATE is required for the chart smoke (no default)");
    let date = validate_t8412_date(&raw_date).expect("chart date failed offline validation");
    let d = date.format("%Y%m%d").to_string();

    // One page only: ncnt=1 (1-minute), qrycnt=20 rows, nday=1, comp_yn=N.
    let req = T8412Request::new(&symbol, "1", "20", "1", &d, &d, "N");
    let resp = sdk
        .paginated()
        .chart_page(&req)
        .await
        .expect("t8412 chart_page failed (a gateway 01715 means a non-trading day)");

    // Credential-free by construction: `rsp_msg` is dropped (it can carry
    // localized, account-identifying text and is excluded from the
    // token/t1101 evidence pattern); only the numeric `rsp_cd` proves success
    // and `rows` is a public structural count. Mirrors `live_smoke_default`.
    record(
        "live-smoke-chart",
        &format!("symbol={symbol} date={d}"),
        &format!("rsp_cd={} rows={}", resp.rsp_cd, resp.outblock1.len()),
    );
}

// ---------------------------------------------------------------------------
// t1452 — 거래량상위 (top trading volume). First single-page body-`idx` paginated
// TR (the implement-tr second-freeze sub-pattern). Intraday rank screen: on a
// non-trading day the gateway returns an empty success (00707) → PENDING.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1452`: paper guard → token → one single-page `t1452`
/// top-volume read (all-segment, permissive filters, first-page `idx`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1452`"]
async fn live_smoke_t1452() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // All-segment, no price/volume/change-rate filter, first page.
    let req = T1452Request::new("0", "0", "0", "0", "0", "0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_volume(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1452",
                &format!("env=paper gubun=0 idx=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1452 market-data failure (not evidence)");
            panic!("live-smoke-t1452 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t1403 / t1441 / t1463 / t1466 / t1489 / t1492 — the remaining single-page
// body-`idx` paginated rank/screen TRs. Same sub-pattern as t1452. Intraday
// rank screens may return an empty success (00707) outside a session → PENDING.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1403`: single-page `t1403` newly-listed stocks over a wide
/// listing-month range (a historical range query, non-empty off-session).
///
/// NOT trading-day-gated: despite `facets.date_sensitive: true`, `t1403`'s inputs
/// are listing MONTHS (`styymm`/`enyymm`, `YYYYMM`), not a trading DAY, so the
/// `01715` non-trading-day error structurally cannot apply — verified live across
/// weekday/weekend/future ranges (it never returns `01715`). Unlike `t8412`, this
/// smoke needs no weekday pin and no `01715` prior-weekday retry. A wide range is
/// used so past listings keep it non-empty regardless of when it runs; a TR-level
/// `IGW00201` gateway error is transient throttling (clears on retry / spacing),
/// classified environmental by the R6 probe, never a TR defect.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1403`"]
async fn live_smoke_t1403() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Wide listing-month range so past listings keep it non-empty regardless of
    // when it runs (no trading-day/01715 concept applies — see fn doc).
    let req = T1403Request::new("0", "202401", "202612");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().new_listings(&req).await {
        Ok(resp) => record(
            "live-smoke-t1403",
            &format!("env=paper range=202401-202612 idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1403 market-data failure (not evidence)");
            panic!("live-smoke-t1403 failed: {e}");
        }
    }
}

/// `make live-smoke-t1441`: single-page `t1441` top change-rate (up, today, KRX).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1441`"]
async fn live_smoke_t1441() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1441Request::new("0", "1", "1", "0", "0", "0", "0", "0", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_change_rate(&req).await {
        Ok(resp) => record(
            "live-smoke-t1441",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1441 market-data failure (not evidence)");
            panic!("live-smoke-t1441 failed: {e}");
        }
    }
}

/// `make live-smoke-t1463`: single-page `t1463` top trading value (KRX).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1463`"]
async fn live_smoke_t1463() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1463Request::new("0", "0", "0", "0", "0", "0", "0", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_value(&req).await {
        Ok(resp) => record(
            "live-smoke-t1463",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1463 market-data failure (not evidence)");
            panic!("live-smoke-t1463 failed: {e}");
        }
    }
}

/// `make live-smoke-t1466`: single-page `t1466` volume-surge screen (KRX).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1466`"]
async fn live_smoke_t1466() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1466Request::new("0", "1", "1", "0", "0", "0", "0", "0", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().volume_surge(&req).await {
        Ok(resp) => record(
            "live-smoke-t1466",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1466 market-data failure (not evidence)");
            panic!("live-smoke-t1466 failed: {e}");
        }
    }
}

/// `make live-smoke-t1489`: single-page `t1489` top expected-execution volume.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1489`"]
async fn live_smoke_t1489() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1489Request::new("0", "0", "000000000000", "0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_expected_volume(&req).await {
        Ok(resp) => record(
            "live-smoke-t1489",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1489 market-data failure (not evidence)");
            panic!("live-smoke-t1489 failed: {e}");
        }
    }
}

/// `make live-smoke-t1492`: single-page `t1492` single-price expected change rate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1492`"]
async fn live_smoke_t1492() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1492Request::new("0", "1", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().single_price_expected(&req).await {
        Ok(resp) => record(
            "live-smoke-t1492",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1492 market-data failure (not evidence)");
            panic!("live-smoke-t1492 failed: {e}");
        }
    }
}

/// `make live-smoke-t1481`: single-page `t1481` after-hours top change-rate
/// (시간외등락율상위; all-segment, up, permissive filters, first-page `idx`).
///
/// `after_hours_top_change_rate` returning `Ok` with a non-empty `outblock1`
/// proves the read is callable and the raw-capture row shape round-trips. The
/// recorded line carries only `rsp_cd` + a public row count (no `rsp_msg`, token,
/// or account text) and is self-dated; a failed run emits a distinct `SMOKE-FAIL`
/// stderr line, never a capturable `LIVE-SMOKE` line. An empty success (`00707`)
/// outside an after-hours session is the PENDING case, not a defect.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1481`"]
async fn live_smoke_t1481() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // All-segment, up, no min-volume filter, first page.
    let req = T1481Request::new("0", "1", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().after_hours_top_change_rate(&req).await {
        Ok(resp) if resp.outblock1.is_empty() => {
            // Empty success (`00707`) outside an after-hours session is the PENDING
            // case, not Implemented evidence — emit no capturable LIVE-SMOKE line
            // (mirrors live_smoke_t1866's non-empty guard).
            eprintln!("SMOKE-FAIL target=live-smoke-t1481 empty result (00707); PENDING not evidence");
            panic!("live-smoke-t1481: empty result (00707) — PENDING, not Implemented");
        }
        Ok(resp) => record(
            "live-smoke-t1481",
            &format!("env=paper gubun1=0 idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1481 market-data failure (not evidence)");
            panic!("live-smoke-t1481 failed: {e}");
        }
    }
}

/// `make live-smoke-t1482`: single-page `t1482` after-hours top volume
/// (시간외거래량상위; all-segment, ascending sort, permissive filters, first-page
/// `idx`).
///
/// `after_hours_top_volume` returning `Ok` with a non-empty `outblock1` proves the
/// read is callable and the raw-capture row shape round-trips. The recorded line
/// carries only `rsp_cd` + a public row count (no `rsp_msg`, token, or account
/// text) and is self-dated; a failed run emits a distinct `SMOKE-FAIL` stderr line,
/// never a capturable `LIVE-SMOKE` line. An empty success (`00707`) outside an
/// after-hours session is the PENDING case, not a defect.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1482`"]
async fn live_smoke_t1482() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // sort_gbn=0, all-segment, permissive volume flag, first page.
    let req = T1482Request::new("0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().after_hours_top_volume(&req).await {
        Ok(resp) if resp.outblock1.is_empty() => {
            // Empty success (`00707`) outside an after-hours session is the PENDING
            // case, not Implemented evidence — emit no capturable LIVE-SMOKE line
            // (mirrors live_smoke_t1866's non-empty guard).
            eprintln!("SMOKE-FAIL target=live-smoke-t1482 empty result (00707); PENDING not evidence");
            panic!("live-smoke-t1482: empty result (00707) — PENDING, not Implemented");
        }
        Ok(resp) => record(
            "live-smoke-t1482",
            &format!("env=paper sort_gbn=0 idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1482 market-data failure (not evidence)");
            panic!("live-smoke-t1482 failed: {e}");
        }
    }
}

/// `make live-smoke-t1866`: paper guard → server-saved condition list (the
/// saved-condition spine producer). `user_id` comes from `LS_PAPER_USER_ID`
/// (never the caller, never recorded — it is account-identifying). The recorded
/// line carries only `rsp_cd` and the structural condition count; an empty list
/// (no seeded condition) surfaces as a credential-safe `SMOKE-FAIL` so it is
/// distinguishable from a defect.
#[tokio::test]
#[ignore = "live smoke: needs LS_PAPER_USER_ID + a seeded server-saved condition; run via `make live-smoke-t1866`"]
async fn live_smoke_t1866() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let user_id = match std::env::var("LS_PAPER_USER_ID") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1866 LS_PAPER_USER_ID unset (not evidence)");
            panic!("live-smoke-t1866: LS_PAPER_USER_ID required (the LS login id)");
        }
    };
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .saved_conditions(&T1866Request::new(user_id))
        .await
    {
        Ok(resp) if resp.outblock1.is_empty() => {
            // Success transport but no saved condition exists → spine-input-unavailable.
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1866 no saved condition (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1866: no server-saved condition to yield a query_index");
        }
        Ok(resp) => record(
            "live-smoke-t1866",
            &format!("env=paper gb=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "conditions")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1866 market-data failure (not evidence)");
            panic!("live-smoke-t1866 failed: {e}");
        }
    }
}

/// `make live-smoke-t1305`: paper guard → token → one `t1305` period-price read for
/// `shcode="005930"` `dwmcode="1"` (daily) `date="<today>"` `cnt="10"`. A non-empty
/// candle array proves the typed paginated read round-trips. Session-independent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1305`"]
async fn live_smoke_t1305() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1305_SHCODE").unwrap_or_else(|_| "005930".into());
    let date = Utc::now().format("%Y%m%d").to_string();
    match sdk.paginated().stock_price_period(&T1305Request::new(&shcode, "1", &date, "10")).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1305: empty candles (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1305", &format!("env=paper shcode={shcode} dwmcode=1 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1305 market-data failure (not evidence)");
            panic!("live-smoke-t1305 failed: {e}");
        }
    }
}

/// `make live-smoke-t1411`: paper guard → token → one `t1411` stocks-by-margin-rate
/// read (gubun=0, jongchk=1, jkrate=1, shcode=005930, first-page idx=0 as a JSON
/// number). A NON-EMPTY margin-rate array proves the read round-trips; an empty
/// `00707` (R7) does NOT record — it dispositions to empty-result PENDING. The
/// recorded line is credential-free (`rsp_cd` + row count, never `rsp_msg`); a
/// failed run emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1411`"]
async fn live_smoke_t1411() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .stocks_by_margin_rate(&T1411Request::new("0", "1", "1", "005930"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1411: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // Assert a non-default first-row field before recording (proves a real
            // round-trip, not a serde(default) Ok): the short code must be non-empty.
            assert!(
                !resp.outblock1[0].shcode.is_empty(),
                "live-smoke-t1411: first row must carry a real shcode"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1411", &format!("env=paper gubun=0 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1411 market-data failure (not evidence)");
            panic!("live-smoke-t1411 failed: {e}");
        }
    }
}

/// `make live-smoke-t1488`: paper guard → token → one `t1488` expected-execution
/// top-change-rate read (gubun=0, sign=1, jgubun=1, jongchk=0, volume=0, first-page
/// idx=0 + yesprice/yeeprice/yevolume=0 all as JSON numbers). A NON-EMPTY change-rate
/// array proves the read round-trips; an empty `00707` (R7) does NOT record — it
/// dispositions to empty-result PENDING. The recorded line is credential-free
/// (`rsp_cd` + row count, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr
/// line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1488`"]
async fn live_smoke_t1488() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .expected_exec_top_change_rate(&T1488Request::new("0", "1", "1", "0", "0"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1488: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // Assert a non-default first-row field before recording (proves a real
            // round-trip, not a serde(default) Ok): the short code must be non-empty.
            assert!(
                !resp.outblock1[0].shcode.is_empty(),
                "live-smoke-t1488: first row must carry a real shcode"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1488", &format!("env=paper gubun=0 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1488 market-data failure (not evidence)");
            panic!("live-smoke-t1488 failed: {e}");
        }
    }
}

/// `make live-smoke-t1636`: paper guard → token → one `t1636` per-stock
/// program-trading-trend read (gubun=0, gubun1=0, gubun2=0, shcode=005930,
/// exchgubun="", first-page cts_idx=0 as a JSON number). A NON-EMPTY
/// program-trading array proves the read round-trips; an empty `00707` (R7) does
/// NOT record — it dispositions to empty-result PENDING. The recorded line is
/// credential-free (`rsp_cd` + row count, never `rsp_msg`); a failed run emits a
/// `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1636`"]
async fn live_smoke_t1636() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .program_trade_trend_by_stock(&T1636Request::new("0", "0", "0", "005930", ""))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1636: empty result (00707) — PENDING, not Implemented (R7)"
            );
            // Assert a non-default first-row field before recording (proves a real
            // round-trip, not a serde(default) Ok): the short code must be non-empty.
            assert!(
                !resp.outblock1[0].shcode.is_empty(),
                "live-smoke-t1636: first row must carry a real shcode"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1636", &format!("env=paper shcode=005930 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1636 market-data failure (not evidence)");
            panic!("live-smoke-t1636 failed: {e}");
        }
    }
}

/// `make live-smoke-t3341`: token → one single-page `t3341` financial-ranking
/// read (body `idx`=0 as a number; single-page scope, KTD-5).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3341`"]
async fn live_smoke_t3341() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().financial_ranking(&T3341Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ranks")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t3341", &format!("env=paper gubun=0 idx=0 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3341 market-data failure (not evidence)");
            panic!("live-smoke-t3341 failed: {e}");
        }
    }
}

// === plan -003 all-lane wave — overseas-futures(-option) + night-deriv smokes ==
// Lane: LS_SMOKE_LANE=overseas_option for the o31xx (…71 account); CUSN26 is a
// CURRENT front-month contract that persists last-session data under KRX closure.
// t8462 runs on LS_SMOKE_LANE=domestic_option (…51) over a recent date range. Each:
// suppressor first (U2/KTD5) → paper guard → token → call → non-empty witness (R4;
// an empty out-block / 00707 is the PENDING case) → record (env/symbol/date only) →
// scrubbed panic path.

/// `make live-smoke-o3103`: overseas-futures 분봉 chart (`shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3103`"]
async fn live_smoke_o3103() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3103Request::new("CUSN26");
    match sdk.paginated().overseas_futures_minute_chart(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3103: empty out-block (00707) — PENDING");
            assert_nonempty_witness("close", &resp.outblock1[0].close)
                .expect("live-smoke-o3103: candle close must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-fut-min")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3103", "env=paper symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3103 market-data failure (not evidence)");
            panic!("live-smoke-o3103 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3108`: overseas-futures D/W/M chart (`shcode=CUSN26`,
/// `gubun=0`, `sdate=20260101`, `edate`=recent weekday).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3108`"]
async fn live_smoke_o3108() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3108Request::new("CUSN26", "0", "20260101", "20260626");
    match sdk.paginated().overseas_futures_period_chart(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3108: empty out-block (00707) — PENDING");
            assert_nonempty_witness("close", &resp.outblock1[0].close)
                .expect("live-smoke-o3108: candle close must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-fut-dwm")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3108", "env=paper symbol=CUSN26 range=20260101-20260626", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3108 market-data failure (not evidence)");
            panic!("live-smoke-o3108 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3116`: overseas-futures tick (`gubun=0`, `shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3116`"]
async fn live_smoke_o3116() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3116Request::new("0", "CUSN26");
    match sdk.paginated().overseas_futures_tick(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3116: empty out-block (00707) — PENDING");
            assert_nonempty_witness("price", &resp.outblock1[0].price)
                .expect("live-smoke-o3116: 체결가 must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-fut-tick")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3116", "env=paper symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3116 market-data failure (not evidence)");
            panic!("live-smoke-o3116 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3117`: overseas-futures NTick chart (`shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3117`"]
async fn live_smoke_o3117() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3117Request::new("CUSN26");
    match sdk.paginated().overseas_futures_ntick(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3117: empty out-block (00707) — PENDING");
            assert_nonempty_witness("close", &resp.outblock1[0].close)
                .expect("live-smoke-o3117: candle close must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-fut-ntick")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3117", "env=paper symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3117 market-data failure (not evidence)");
            panic!("live-smoke-o3117 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3123`: overseas-futopt 분봉 chart (`mktgb=F`, `shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3123`"]
async fn live_smoke_o3123() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3123Request::new("F", "CUSN26");
    match sdk.paginated().overseas_futopt_minute_chart(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3123: empty out-block (00707) — PENDING");
            assert_nonempty_witness("close", &resp.outblock1[0].close)
                .expect("live-smoke-o3123: candle close must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-futopt-min")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3123", "env=paper mktgb=F symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3123 market-data failure (not evidence)");
            panic!("live-smoke-o3123 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3128`: overseas-futopt D/W/M chart (`mktgb=F`, `shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3128`"]
async fn live_smoke_o3128() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3128Request::new("F", "CUSN26", "1", "20250525", "20260626");
    match sdk.paginated().overseas_futopt_period_chart(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3128: empty out-block (00707) — PENDING");
            assert_nonempty_witness("close", &resp.outblock1[0].close)
                .expect("live-smoke-o3128: candle close must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-futopt-dwm")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3128", "env=paper mktgb=F symbol=CUSN26 range=20250525-20260626", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3128 market-data failure (not evidence)");
            panic!("live-smoke-o3128 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3136`: overseas-futopt tick (`gubun=0`, `mktgb=F`, `shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3136`"]
async fn live_smoke_o3136() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3136Request::new("0", "F", "CUSN26");
    match sdk.paginated().overseas_futopt_tick(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3136: empty out-block (00707) — PENDING");
            assert_nonempty_witness("price", &resp.outblock1[0].price)
                .expect("live-smoke-o3136: 체결가 must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-futopt-tick")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3136", "env=paper mktgb=F symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3136 market-data failure (not evidence)");
            panic!("live-smoke-o3136 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3137`: overseas-futopt NTick chart (`mktgb=F`, `shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3137`"]
async fn live_smoke_o3137() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3137Request::new("F", "CUSN26");
    match sdk.paginated().overseas_futopt_ntick(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3137: empty out-block (00707) — PENDING");
            assert_nonempty_witness("close", &resp.outblock1[0].close)
                .expect("live-smoke-o3137: candle close must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-futopt-ntick")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3137", "env=paper mktgb=F symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3137 market-data failure (not evidence)");
            panic!("live-smoke-o3137 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-o3139`: overseas-futopt NTick fixed chart (`mktgb=F`, `shcode=CUSN26`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3139`"]
async fn live_smoke_o3139() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3139Request::new("F", "CUSN26");
    match sdk.paginated().overseas_futopt_ntick_fixed(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3139: empty out-block (00707) — PENDING");
            assert_nonempty_witness("close", &resp.outblock1[0].close)
                .expect("live-smoke-o3139: candle close must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-futopt-ntickf")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3139", "env=paper mktgb=F symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3139 market-data failure (not evidence)");
            panic!("live-smoke-o3139 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t8410`: paper guard → token → one `t8410` daily stock chart
/// (`shcode=078020`, gubun=2). A non-empty candle array proves the read round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8410`"]
async fn live_smoke_t8410() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8410Request::new("078020", "2", "20", "", "99999999");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().stock_chart_period(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8410: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8410", &format!("env=paper shcode=078020 gubun=2 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8410 market-data failure (not evidence)");
            panic!("live-smoke-t8410 failed: {e}");
        }
    }
}

/// `make live-smoke-t8451`: paper guard → token → one `t8451` integrated daily
/// stock chart (`shcode=010950`, gubun=2). Non-empty candle array → round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8451`"]
async fn live_smoke_t8451() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8451Request::new("010950", "2", "20", "", "99999999");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().stock_chart_period_unified(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8451: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8451", &format!("env=paper shcode=010950 gubun=2 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8451 market-data failure (not evidence)");
            panic!("live-smoke-t8451 failed: {e}");
        }
    }
}

/// `make live-smoke-t8419`: paper guard → token → one `t8419` daily sector chart
/// (`shcode=001`, gubun=2). Non-empty candle array → round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8419`"]
async fn live_smoke_t8419() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8419Request::new("001", "2", "20", "", "99999999");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().sector_chart_period(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8419: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8419", &format!("env=paper shcode=001 gubun=2 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8419 market-data failure (not evidence)");
            panic!("live-smoke-t8419 failed: {e}");
        }
    }
}

/// `make live-smoke-t4203`: paper guard → token → one `t4203` composite daily
/// sector chart (`shcode=001`, gubun=2). Non-empty candle array → round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t4203`"]
async fn live_smoke_t4203() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T4203Request::new("001", "2", "1", "20", "", "99999999");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().sector_chart_composite(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t4203: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t4203", &format!("env=paper shcode=001 gubun=2 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t4203 market-data failure (not evidence)");
            panic!("live-smoke-t4203 failed: {e}");
        }
    }
}

/// `make live-smoke-t8417`: one `t8417` sector tick chart (업종차트 틱/n틱).
/// Non-empty candle array under closure → round-trips. Plan -004 batch A.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8417`"]
async fn live_smoke_t8417() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8417Request::new("001", "1", "20", "0", "", "99999999", "N");
    match sdk.paginated().sector_chart_tick(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8417: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8417", "env=paper shcode=001 ncnt=1", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8417 market-data failure (not evidence)");
            panic!("live-smoke-t8417 failed: {e}");
        }
    }
}

/// `make live-smoke-t8418`: one `t8418` sector N-minute chart (업종차트 N분).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8418`"]
async fn live_smoke_t8418() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8418Request::new("001", "1", "20", "0", "", "99999999", "N");
    match sdk.paginated().sector_chart_minute(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8418: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8418", "env=paper shcode=001 ncnt=1", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8418 market-data failure (not evidence)");
            panic!("live-smoke-t8418 failed: {e}");
        }
    }
}

/// `make live-smoke-t8411`: one `t8411` stock tick chart (주식차트 틱/n틱).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8411`"]
async fn live_smoke_t8411() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8411Request::new("005930", "1", "20", "0", "", "99999999", "N");
    match sdk.paginated().stock_chart_tick(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8411: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8411", "env=paper shcode=005930 ncnt=1", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8411 market-data failure (not evidence)");
            panic!("live-smoke-t8411 failed: {e}");
        }
    }
}

/// `make live-smoke-t8452`: one `t8452` integrated stock N-minute chart.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8452`"]
async fn live_smoke_t8452() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8452Request::new("010950", "1", "20", "0", "", "99999999", "N", "K");
    match sdk.paginated().stock_chart_minute_unified(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8452: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8452", "env=paper shcode=010950 ncnt=1", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8452 market-data failure (not evidence)");
            panic!("live-smoke-t8452 failed: {e}");
        }
    }
}

/// `make live-smoke-t8453`: one `t8453` integrated stock tick chart.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8453`"]
async fn live_smoke_t8453() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8453Request::new("010950", "1", "20", "0", "", "99999999", "N", "K");
    match sdk.paginated().stock_chart_tick_unified(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t8453: empty chart (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8453", "env=paper shcode=010950 ncnt=1", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8453 market-data failure (not evidence)");
            panic!("live-smoke-t8453 failed: {e}");
        }
    }
}

/// `make live-smoke-t8464`: one `t8464` F/O tick chart for a current contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8464`"]
async fn live_smoke_t8464() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = current_index_future(&sdk, "live-smoke-t8464").await;
    let req = T8464Request::new(&shcode, "1", "20", "0", "", "99999999", "N");
    match sdk.paginated().fo_chart_tick(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t8464: empty chart (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8464", &format!("env=paper shcode={shcode}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8464 market-data failure (not evidence)");
            panic!("live-smoke-t8464 failed: {e}");
        }
    }
}

/// `make live-smoke-t8465`: one `t8465` F/O N-minute chart for a current contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8465`"]
async fn live_smoke_t8465() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = current_index_future(&sdk, "live-smoke-t8465").await;
    let req = T8465Request::new(&shcode, "1", "20", "0", "", "99999999", "N");
    match sdk.paginated().fo_chart_minute(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t8465: empty chart (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8465", &format!("env=paper shcode={shcode}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8465 market-data failure (not evidence)");
            panic!("live-smoke-t8465 failed: {e}");
        }
    }
}

/// `make live-smoke-t8466`: one `t8466` F/O day/week/month chart for a current contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8466`"]
async fn live_smoke_t8466() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = current_index_future(&sdk, "live-smoke-t8466").await;
    let req = T8466Request::new(&shcode, "2", "20", "", "99999999");
    match sdk.paginated().fo_chart_period(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t8466: empty chart (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "candles")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8466", &format!("env=paper shcode={shcode}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8466 market-data failure (not evidence)");
            panic!("live-smoke-t8466 failed: {e}");
        }
    }
}

/// `make live-smoke-t8405`: one `t8405` stock-futures period price for a current
/// stock-futures contract (sourced from `t8401`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8405`"]
async fn live_smoke_t8405() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let masters = sdk
        .market_session()
        .stock_futures_master(&ls_sdk::market_session::T8401Request::new())
        .await
        .expect("t8401 stock-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!("SMOKE-FAIL target=live-smoke-t8405 t8401 contract source empty (rsp_cd={})", masters.rsp_cd);
        panic!("live-smoke-t8405: no contract to key the read");
    }
    let shcode = masters.outblock[0].shcode.clone();
    let req = T8405Request::new(&shcode, "20");
    match sdk.paginated().stock_futures_period(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t8405: empty board (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8405", &format!("env=paper shcode={shcode}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8405 market-data failure (not evidence)");
            panic!("live-smoke-t8405 failed: {e}");
        }
    }
}

/// `make live-smoke-t1444`: one `t1444` market cap top read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1444`"]
async fn live_smoke_t1444() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1444Request::new("001");
    match sdk.paginated().market_cap_top(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1444: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1444", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1444 market-data failure (not evidence)");
            panic!("live-smoke-t1444 failed: {e}");
        }
    }
}

/// `make live-smoke-t1422`: one `t1422` price limit read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1422`"]
async fn live_smoke_t1422() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1422Request::new();
    match sdk.paginated().price_limit(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1422: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1422", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1422 market-data failure (not evidence)");
            panic!("live-smoke-t1422 failed: {e}");
        }
    }
}

/// `make live-smoke-t1427`: one `t1427` price limit imminent read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1427`"]
async fn live_smoke_t1427() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1427Request::new();
    match sdk.paginated().price_limit_imminent(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1427: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1427", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1427 market-data failure (not evidence)");
            panic!("live-smoke-t1427 failed: {e}");
        }
    }
}

/// `make live-smoke-t1442`: one `t1442` new high low read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1442`"]
async fn live_smoke_t1442() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1442Request::new();
    match sdk.paginated().new_high_low(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1442: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1442", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1442 market-data failure (not evidence)");
            panic!("live-smoke-t1442 failed: {e}");
        }
    }
}

/// `make live-smoke-t1405`: one `t1405` trade suspension read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1405`"]
async fn live_smoke_t1405() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1405Request::new("0", "1");
    match sdk.paginated().trade_suspension(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1405: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1405", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1405 market-data failure (not evidence)");
            panic!("live-smoke-t1405 failed: {e}");
        }
    }
}

/// `make live-smoke-t1960`: one `t1960` elw change rank read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1960`"]
async fn live_smoke_t1960() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1960Request::new();
    match sdk.paginated().elw_change_rank(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1960: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1960", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1960 market-data failure (not evidence)");
            panic!("live-smoke-t1960 failed: {e}");
        }
    }
}

/// `make live-smoke-t1961`: one `t1961` elw volume rank read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1961`"]
async fn live_smoke_t1961() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1961Request::new();
    match sdk.paginated().elw_volume_rank(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1961: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1961", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1961 market-data failure (not evidence)");
            panic!("live-smoke-t1961 failed: {e}");
        }
    }
}

/// `make live-smoke-t1966`: one `t1966` elw value rank read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1966`"]
async fn live_smoke_t1966() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1966Request::new();
    match sdk.paginated().elw_value_rank(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1966: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1966", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1966 market-data failure (not evidence)");
            panic!("live-smoke-t1966 failed: {e}");
        }
    }
}

/// `make live-smoke-t1921`: one `t1921` credit trend read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1921`"]
async fn live_smoke_t1921() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1921Request::new("005930");
    match sdk.paginated().credit_trend(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1921: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1921", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1921 market-data failure (not evidence)");
            panic!("live-smoke-t1921 failed: {e}");
        }
    }
}
