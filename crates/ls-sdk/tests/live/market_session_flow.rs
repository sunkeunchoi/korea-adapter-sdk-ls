use super::*;


/// `make live-smoke-t1859`: paper guard → token → `t1866` saved-condition list →
/// `t1859` condition search keyed by the first saved condition's `query_index`.
///
/// CHAINED, self-sourcing (R8): the consumer never receives a fabricated
/// `query_index` — it is read from a live `t1866` call (mirrors `live_smoke_t1531`
/// self-sourcing a `tmcode` from `t8425`). `LS_PAPER_USER_ID` (the LS login id) is
/// required and never recorded; the `query_index` itself is account-saved-condition
/// data and is NOT printed. An empty `t1866` (no seeded condition) surfaces as a
/// credential-safe `SMOKE-FAIL` (spine-input-unavailable), never a fabricated key.
#[tokio::test]
#[ignore = "live smoke: needs LS_PAPER_USER_ID + a seeded server-saved condition; run via `make live-smoke-t1859`"]
async fn live_smoke_t1859() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let user_id = match std::env::var("LS_PAPER_USER_ID") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1859 LS_PAPER_USER_ID unset (not evidence)");
            panic!("live-smoke-t1859: LS_PAPER_USER_ID required (the LS login id)");
        }
    };

    // Self-source the query_index from a live t1866 saved-condition list.
    let conditions = sdk
        .paginated()
        .saved_conditions(&T1866Request::new(user_id))
        .await
        .expect("t1866 saved_conditions (query_index source) failed");
    if conditions.outblock1.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1859 t1866 spine source empty (rsp_cd={})",
            conditions.rsp_cd
        );
        panic!("live-smoke-t1859: no server-saved condition to key the search");
    }
    let query_index = conditions.outblock1[0].query_index.clone();

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .condition_search(&T1859Request::new(query_index))
        .await
    {
        Ok(resp) => {
            // The query_index is NOT recorded — it is account-saved-condition data.
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1859", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1859 market-data failure (not evidence)");
            panic!("live-smoke-t1859 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t1826 / t1825 — ThinQ Q-click search (Wave 3 spine). t1826 lists the available
// searches (producer); t1825 runs one search keyed by a `search_cd` self-sourced
// from t1826 (consumer, chained). The `search_cd` is a server-assigned catalog
// key and is NEVER recorded (treated like the saved-condition `query_index`).
// ---------------------------------------------------------------------------

/// `make live-smoke-t1826`: paper guard → OAuth token → one `t1826` Q-click
/// search-list read for `search_gb="0"` (핵심검색/core search; the Wave 3
/// producer).
///
/// `qclick_search_list` returning `Ok` with a non-empty `outblock` proves the
/// read is callable and the row shape round-trips. The recorded line is
/// credential-free (only `rsp_cd` + a public search count; `search_cd` values are
/// NOT recorded) and self-dated; a failed run emits a distinct `SMOKE-FAIL`
/// stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1826`"]
async fn live_smoke_t1826() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1826Request::new("0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().qclick_search_list(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "searches")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1826",
                &format!("env=paper search_gb=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1826 market-data failure (not evidence)");
            panic!("live-smoke-t1826 failed: {e}");
        }
    }
}

/// `make live-smoke-t1825`: paper guard → token → `t1826` search-list →
/// `t1825` Q-click search keyed by the first available `search_cd`.
///
/// CHAINED, self-sourcing (R8): the consumer never receives a fabricated
/// `search_cd` — it is read from a live `t1826` call (mirrors `live_smoke_t1859`
/// self-sourcing a `query_index` from `t1866`). The `search_cd` is a
/// server-assigned catalog key and is NOT recorded. An empty `t1826` (no
/// available search) surfaces as a credential-safe `SMOKE-FAIL`
/// (spine-input-unavailable), never a fabricated key.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1825`"]
async fn live_smoke_t1825() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source the search_cd from a live t1826 search-list.
    let list = sdk
        .market_session()
        .qclick_search_list(&T1826Request::new("0"))
        .await
        .expect("t1826 qclick_search_list (search_cd source) failed");
    if list.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1825 t1826 spine source empty (rsp_cd={})",
            list.rsp_cd
        );
        panic!("live-smoke-t1825: no available search to key the Q-click search");
    }
    let search_cd = list.outblock[0].search_cd.clone();

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .qclick_search(&T1825Request::new(search_cd, "0"))
        .await
    {
        Ok(resp) => {
            // The search_cd is NOT recorded — it is a server-assigned catalog key.
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1825",
                &format!("env=paper gubun=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1825 market-data failure (not evidence)");
            panic!("live-smoke-t1825 failed: {e}");
        }
    }
}

/// `make live-smoke-t1638`: paper guard → token → one `t1638` per-stock
/// remaining-quantity / pre-disclosure ranking read for `gubun1="1"` `shcode=""`
/// (full list) `gubun2="1"` `exchgubun=""` (defaults). A success `rsp_cd` with a
/// non-empty ranking row (modeled `hname`/`shcode`) proves the typed array read
/// round-trips. The ranking is reachable under closure; an empty `00707` does NOT
/// record — it dispositions to PENDING. The recorded line is credential-free
/// (`rsp_cd` + row count + lengths, never `rsp_msg`); a failed run emits a
/// `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1638`"]
async fn live_smoke_t1638() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let gubun1 = std::env::var("LS_LIVE_SMOKE_T1638_GUBUN1").unwrap_or_else(|_| "1".into());
    let shcode = std::env::var("LS_LIVE_SMOKE_T1638_SHCODE").unwrap_or_else(|_| "".into());
    let gubun2 = std::env::var("LS_LIVE_SMOKE_T1638_GUBUN2").unwrap_or_else(|_| "1".into());
    let exchgubun =
        std::env::var("LS_LIVE_SMOKE_T1638_EXCHGUBUN").unwrap_or_else(|_| "".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .remaining_quantity_predisclosure(&T1638Request::new(&gubun1, &shcode, &gubun2, &exchgubun))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty ranking row with a
            // modeled non-default field before recording.
            let first = resp.outblock.first();
            assert!(
                first.is_some_and(|r| !r.hname.is_empty() && !r.shcode.is_empty()),
                "live-smoke-t1638: empty ranking (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1638",
                &format!("env=paper gubun1={gubun1} shcode={shcode} gubun2={gubun2} exchgubun={exchgubun} date={date}"),
                &format!(
                    "rsp_cd={} rows={} hname_len={} shcode_len={}",
                    resp.rsp_cd,
                    resp.outblock.len(),
                    row.hname.len(),
                    row.shcode.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1638 market-data failure (not evidence)");
            panic!("live-smoke-t1638 failed: {e}");
        }
    }
}

/// `make live-smoke-t1631`: paper guard → token → one `t1631` program-trade综합
/// read for today's date (market-wide; no instrument/account secrets). A success
/// `rsp_cd` with a non-empty `t1631OutBlock1` totals row (modeled `bidvolume`)
/// proves the typed read round-trips. An empty `00707`/all-default row does NOT
/// record — it dispositions to PENDING. The recorded line is credential-free
/// (`rsp_cd` + lengths/counts, never `rsp_msg`); a failed run emits a `SMOKE-FAIL`
/// stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1631`"]
async fn live_smoke_t1631() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    let gubun = std::env::var("LS_LIVE_SMOKE_T1631_GUBUN").unwrap_or_else(|_| "0".to_string());
    match sdk
        .market_session()
        .program_trade_summary(&T1631Request::new(&gubun, "0", &date, &date, "1"))
        .await
    {
        Ok(resp) => {
            // Non-empty witness: a totals row with a non-empty bidvolume. An empty
            // board (00707/off-data) → PENDING, not Implemented.
            let totals = resp.outblock1.first();
            assert!(
                totals.is_some_and(|r| !r.bidvolume.is_empty()),
                "live-smoke-t1631: empty totals (00707/off-data) — PENDING, not Implemented"
            );
            let row = totals.expect("non-empty guard above");
            record(
                "live-smoke-t1631",
                &format!("env=paper gubun={gubun} date={date}"),
                &format!(
                    "rsp_cd={} remainder_rows={} totals_rows={} bidvolume_len={}",
                    resp.rsp_cd,
                    resp.outblock.len(),
                    resp.outblock1.len(),
                    row.bidvolume.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1631 program-trade failure (not evidence)");
            panic!("live-smoke-t1631 failed: {e}");
        }
    }
}

/// `make live-smoke-t1632`: paper guard → token → one `t1632` program-trade
/// intraday-trend read for today (market-wide). A success `rsp_cd` with a non-empty
/// `t1632OutBlock1` row (modeled `k200jisu`) proves the typed time-series read
/// round-trips. An empty `00707` does NOT record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1632`"]
async fn live_smoke_t1632() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .program_trade_trend_intraday(&T1632Request::new("0", "1", "0", "0", &date, "", "1"))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.k200jisu.is_empty()),
                "live-smoke-t1632: empty time-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1632",
                &format!("env=paper date={date}"),
                &format!(
                    "rsp_cd={} rows={} k200jisu_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.k200jisu.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1632 program-trade failure (not evidence)");
            panic!("live-smoke-t1632 failed: {e}");
        }
    }
}

/// `make live-smoke-t1633`: paper guard → token → one `t1633` program-trade
/// daily-trend read over a recent date range (market-wide). A success `rsp_cd` with
/// a non-empty `t1633OutBlock1` row (modeled `jisu`) proves the typed daily-series
/// read round-trips. An empty `00707` does NOT record — PENDING. Credential-free.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1633`"]
async fn live_smoke_t1633() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    let fdate = std::env::var("LS_LIVE_SMOKE_T1633_FDATE").unwrap_or_else(|_| "20260601".to_string());
    match sdk
        .market_session()
        .program_trade_trend_daily(&T1633Request::new(
            "0", "1", "0", "1", &fdate, &date, "0", &date, "1",
        ))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.jisu.is_empty()),
                "live-smoke-t1633: empty daily-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1633",
                &format!("env=paper fdate={fdate} tdate={date}"),
                &format!(
                    "rsp_cd={} rows={} jisu_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.jisu.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1633 program-trade failure (not evidence)");
            panic!("live-smoke-t1633 failed: {e}");
        }
    }
}

/// `make live-smoke-t1716`: paper guard → token → one `t1716` foreign/institution
/// by-issue trend read (a public ticker over a recent date range; no account
/// secrets; `prapp` is a numeric request field). A success `rsp_cd` with a non-empty
/// `t1716OutBlock` row (modeled `close`) proves the typed date-array read round-trips.
/// An empty `00707` does NOT record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1716`"]
async fn live_smoke_t1716() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1716_SHCODE").unwrap_or_else(|_| "005930".to_string());
    let fromdt = std::env::var("LS_LIVE_SMOKE_T1716_FROMDT").unwrap_or_else(|_| "20260601".to_string());
    let todt = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .foreign_institution_issue_trend(&T1716Request::new(
            &shcode, "0", &fromdt, &todt, "0", "0", "1", "1", "1",
        ))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock.first();
            assert!(
                first.is_some_and(|r| !r.close.is_empty()),
                "live-smoke-t1716: empty by-issue series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1716",
                &format!("env=paper shcode={shcode} fromdt={fromdt} todt={todt}"),
                &format!(
                    "rsp_cd={} rows={} close_len={}",
                    resp.rsp_cd,
                    resp.outblock.len(),
                    row.close.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1716 foreign/institution trend failure (not evidence)");
            panic!("live-smoke-t1716 failed: {e}");
        }
    }
}

/// `make live-smoke-t1927`: paper guard → token → one `t1927` short-selling daily
/// trend read for a public ticker over a recent range (no account secrets). A success
/// `rsp_cd` with a non-empty `t1927OutBlock1` row (modeled `price`) proves the typed
/// daily-series read round-trips. An empty `00707` does NOT record — PENDING.
/// Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1927`"]
async fn live_smoke_t1927() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1927_SHCODE").unwrap_or_else(|_| "005930".to_string());
    let sdate = std::env::var("LS_LIVE_SMOKE_T1927_SDATE").unwrap_or_else(|_| "20260601".to_string());
    let edate = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .short_sale_daily_trend(&T1927Request::new(&shcode, "", &sdate, &edate))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.price.is_empty()),
                "live-smoke-t1927: empty daily-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1927",
                &format!("env=paper shcode={shcode} sdate={sdate} edate={edate}"),
                &format!(
                    "rsp_cd={} rows={} price_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.price.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1927 short-sale daily-trend failure (not evidence)");
            panic!("live-smoke-t1927 failed: {e}");
        }
    }
}

/// `make live-smoke-t1941`: paper guard → token → one `t1941` per-issue stock-loan
/// (대차) daily trend read for a public ticker over a recent range (no account
/// secrets). A success `rsp_cd` with a non-empty `t1941OutBlock1` row (modeled
/// `price`) proves the typed daily-series read round-trips. An empty `00707` does NOT
/// record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1941`"]
async fn live_smoke_t1941() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1941_SHCODE").unwrap_or_else(|_| "005930".to_string());
    let sdate = std::env::var("LS_LIVE_SMOKE_T1941_SDATE").unwrap_or_else(|_| "20260601".to_string());
    let edate = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .stock_loan_daily_trend(&T1941Request::new(&shcode, &sdate, &edate))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.price.is_empty()),
                "live-smoke-t1941: empty daily-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1941",
                &format!("env=paper shcode={shcode} sdate={sdate} edate={edate}"),
                &format!(
                    "rsp_cd={} rows={} price_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.price.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1941 stock-loan daily-trend failure (not evidence)");
            panic!("live-smoke-t1941 failed: {e}");
        }
    }
}

/// `make live-smoke-t1702`: paper guard → token → one `t1702` foreign/institution
/// by-issue trend read (a public ticker; no account secrets). A success `rsp_cd` with
/// a non-empty `t1702OutBlock1` row (modeled `close`) proves the typed date-array read
/// round-trips. An empty `00707` does NOT record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1702`"]
async fn live_smoke_t1702() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1702_SHCODE").unwrap_or_else(|_| "005930".to_string());
    let fromdt = std::env::var("LS_LIVE_SMOKE_T1702_FROMDT").unwrap_or_else(|_| "20260601".to_string());
    let todt = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .foreign_institution_trend(&T1702Request::new(&shcode, &fromdt, &todt, "1", "0", "0", "1"))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock.first();
            assert!(
                first.is_some_and(|r| !r.close.is_empty()),
                "live-smoke-t1702: empty date-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1702",
                &format!("env=paper shcode={shcode} fromdt={fromdt} todt={todt}"),
                &format!(
                    "rsp_cd={} rows={} close_len={}",
                    resp.rsp_cd,
                    resp.outblock.len(),
                    row.close.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1702 foreign/institution failure (not evidence)");
            panic!("live-smoke-t1702 failed: {e}");
        }
    }
}

/// `make live-smoke-t1717`: paper guard → token → one `t1717` foreign/institution
/// net-buy trend read (a public ticker). A success `rsp_cd` with a non-empty
/// `t1717OutBlock` row (modeled `close`) proves the typed date-array read round-trips.
/// An empty `00707` does NOT record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1717`"]
async fn live_smoke_t1717() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1717_SHCODE").unwrap_or_else(|_| "005930".to_string());
    let fromdt = std::env::var("LS_LIVE_SMOKE_T1717_FROMDT").unwrap_or_else(|_| "20260601".to_string());
    let todt = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .foreign_institution_net_buy_trend(&T1717Request::new(&shcode, "1", &fromdt, &todt, "1"))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock.first();
            assert!(
                first.is_some_and(|r| !r.close.is_empty()),
                "live-smoke-t1717: empty date-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1717",
                &format!("env=paper shcode={shcode} fromdt={fromdt} todt={todt}"),
                &format!(
                    "rsp_cd={} rows={} close_len={}",
                    resp.rsp_cd,
                    resp.outblock.len(),
                    row.close.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1717 foreign/institution failure (not evidence)");
            panic!("live-smoke-t1717 failed: {e}");
        }
    }
}

/// `make live-smoke-t1665`: paper guard → token → one `t1665` investor-by-sector
/// trend read (KOSPI sector `001`; no account secrets). A success `rsp_cd` with a
/// non-empty `t1665OutBlock1` row (modeled `jisu`) proves the typed date-array read
/// round-trips. An empty `00707` does NOT record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1665`"]
async fn live_smoke_t1665() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let upcode = std::env::var("LS_LIVE_SMOKE_T1665_UPCODE").unwrap_or_else(|_| "001".to_string());
    let from_date = std::env::var("LS_LIVE_SMOKE_T1665_FROMDATE").unwrap_or_else(|_| "20260601".to_string());
    let to_date = std::env::var("LS_LIVE_SMOKE_DATE").unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .sector_investor_trend(&T1665Request::new("1", &upcode, "1", "1", &from_date, &to_date, "1"))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.jisu.is_empty()),
                "live-smoke-t1665: empty sector-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1665",
                &format!("env=paper upcode={upcode} from={from_date} to={to_date}"),
                &format!(
                    "rsp_cd={} rows={} jisu_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.jisu.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1665 sector-investor failure (not evidence)");
            panic!("live-smoke-t1665 failed: {e}");
        }
    }
}

/// `make live-smoke-t1475`: paper guard → token → one `t1475` VP-relative ranking
/// read (a public ticker; NUMERIC request slots datacnt/date/time/rankcnt serialize
/// as JSON numbers). A success `rsp_cd` with a non-empty `t1475OutBlock1` row (modeled
/// `price`) proves the typed ranked-array read round-trips. An empty `00707` does NOT
/// record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1475`"]
async fn live_smoke_t1475() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1475_SHCODE").unwrap_or_else(|_| "005930".to_string());
    let datacnt = std::env::var("LS_LIVE_SMOKE_T1475_DATACNT").unwrap_or_else(|_| "20".to_string());
    match sdk
        .market_session()
        .vp_change_ranking(&T1475Request::new(&shcode, "1", &datacnt, "0", "0", "0", "0"))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.price.is_empty()),
                "live-smoke-t1475: empty ranking (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1475",
                &format!("env=paper shcode={shcode} datacnt={datacnt}"),
                &format!(
                    "rsp_cd={} rows={} price_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.price.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1475 vp-ranking failure (not evidence)");
            panic!("live-smoke-t1475 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Wave 2 — market-flow analytics reads (t1601, t1615, t1640, t1662, t1664).
// Standalone gubun-filter reads with documented defaults; non-empty success gate.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1601`: token → one `t1601` investor-by-type aggregate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1601`"]
async fn live_smoke_t1601() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().investor_aggregate(&T1601Request::new()).await {
        Ok(resp) if resp.outblock1.svolume_08.is_empty() && resp.outblock1.svolume_17.is_empty() => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1601 empty aggregate (rsp_cd={})", resp.rsp_cd);
            panic!("live-smoke-t1601: empty investor aggregate (shape-unconfirmed)");
        }
        Ok(resp) => record(
            "live-smoke-t1601",
            &format!("env=paper exchgubun=K date={date}"),
            &format!("rsp_cd={} aggregate=populated", resp.rsp_cd),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1601 market-data failure (not evidence)");
            panic!("live-smoke-t1601 failed: {e}");
        }
    }
}

/// `make live-smoke-t1615`: token → one `t1615` investor trading aggregate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1615`"]
async fn live_smoke_t1615() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().investor_trading(&T1615Request::new()).await {
        Ok(resp) if resp.outblock1.is_empty() && resp.outblock.sum_value.is_empty() => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1615 empty aggregate (rsp_cd={})", resp.rsp_cd);
            panic!("live-smoke-t1615: empty trading aggregate (shape-unconfirmed)");
        }
        Ok(resp) => record(
            "live-smoke-t1615",
            &format!("env=paper exchgubun=K date={date}"),
            &format!("rsp_cd={} markets={}", resp.rsp_cd, resp.outblock1.len()),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1615 market-data failure (not evidence)");
            panic!("live-smoke-t1615 failed: {e}");
        }
    }
}

/// `make live-smoke-t1640`: token → one `t1640` program-trading aggregate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1640`"]
async fn live_smoke_t1640() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().program_aggregate(&T1640Request::new()).await {
        Ok(resp) if resp.outblock.value.is_empty() && resp.outblock.volume.is_empty() => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1640 empty aggregate (rsp_cd={})", resp.rsp_cd);
            panic!("live-smoke-t1640: empty program aggregate (shape-unconfirmed)");
        }
        Ok(resp) => record(
            "live-smoke-t1640",
            &format!("env=paper gubun=11 date={date}"),
            &format!("rsp_cd={} aggregate=populated", resp.rsp_cd),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1640 market-data failure (not evidence)");
            panic!("live-smoke-t1640 failed: {e}");
        }
    }
}

/// `make live-smoke-t1662`: token → one `t1662` by-time program-trading chart.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1662`"]
async fn live_smoke_t1662() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().program_chart(&T1662Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1662", &format!("env=paper gubun=0 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1662 market-data failure (not evidence)");
            panic!("live-smoke-t1662 failed: {e}");
        }
    }
}

/// `make live-smoke-t1664`: token → one `t1664` investor trading chart.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1664`"]
async fn live_smoke_t1664() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().investor_chart(&T1664Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1664", &format!("env=paper mgubun=1 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1664 market-data failure (not evidence)");
            panic!("live-smoke-t1664 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// [업종] 시세 — sector/index cluster (Wave A). All on /indtp/market-data.
// t8424 is the anchor + upcode source; the four consumers smoke standalone with
// upcode="001" (코스피종합), confirmed accepted by the U1 raw-probe.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8424`: paper guard → OAuth token → one `t8424` all-sectors
/// read. A non-empty sector array proves the anchor is callable and round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8424`"]
async fn live_smoke_t8424() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8424Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sectors(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "sectors")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8424", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8424 market-data failure (not evidence)");
            panic!("live-smoke-t8424 failed: {e}");
        }
    }
}

/// `make live-smoke-t1511`: paper guard → OAuth token → one `t1511` index
/// snapshot for `upcode="001"`. A single OutBlock with a success `rsp_cd` proves
/// the read is callable and the snapshot round-trips. KRX-session-dependent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1511`"]
async fn live_smoke_t1511() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1511Request::new("001");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sector_quote(&req).await {
        Ok(resp) => {
            record(
                "live-smoke-t1511",
                &format!("env=paper upcode=001 date={date}"),
                &format!(
                    "rsp_cd={} hname_len={} pricejisu={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.pricejisu
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1511 market-data failure (not evidence)");
            panic!("live-smoke-t1511 failed: {e}");
        }
    }
}

/// `make live-smoke-t1485`: paper guard → OAuth token → one `t1485` expected-index
/// read for `upcode="001"`, `gubun="1"`. The time-row array `t1485OutBlock1`
/// proves the read round-trips. Expected/auction screen — KRX-session-dependent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1485`"]
async fn live_smoke_t1485() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1485Request::new("001", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sector_expected_index(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1485",
                &format!("env=paper upcode=001 gubun=1 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1485 market-data failure (not evidence)");
            panic!("live-smoke-t1485 failed: {e}");
        }
    }
}

/// `make live-smoke-t1516`: paper guard → OAuth token → one `t1516` per-sector
/// stock-board read for `upcode="001"` + a representative `shcode="005930"`. The
/// per-stock array `t1516OutBlock1` proves the read round-trips. Session-dependent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1516`"]
async fn live_smoke_t1516() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1516Request::new("001", "1", "005930");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sector_stocks(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "stocks")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1516",
                &format!("env=paper upcode=001 shcode=005930 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1516 market-data failure (not evidence)");
            panic!("live-smoke-t1516 failed: {e}");
        }
    }
}

/// `make live-smoke-t8463`: paper guard → token → one KRX night-derivatives
/// investor-by-timeslot read (`tm_rng="N"` 야간, `fot_clsf_cd="F"` 선물,
/// `bsc_asts_id="101"` KOSPI200). `venue_session: krx_extended` (KTD7) —
/// off-window empty is a re-run, not a flip/DROP.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8463`"]
async fn live_smoke_t8463() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .night_investor_timeslot(&T8463Request::new("D", "F", "K2I"))
        .await
    {
        Ok(resp) => {
            // Named witness: a real investor row carries a net-buy volume
            // (외국인순매수거래량 formsvol / 개인순매수거래량 indmsvol). An empty
            // array, or rows with only sentinel/zero volumes, is the off-data case.
            let witnessed = resp
                .outblock1
                .iter()
                .any(|r| !r.formsvol.is_empty() || !r.indmsvol.is_empty());
            if resp.outblock1.is_empty() || !witnessed {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8463 empty/no-witness investor array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8463: no named investor witness (00707/off-data) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8463",
                &format!("env=paper tm_rng=D fot_clsf_cd=F bsc_asts_id=K2I date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8463 market-data failure (not evidence)");
            panic!("live-smoke-t8463 failed: {e}");
        }
    }
}

/// `make live-smoke-t3521`: paper guard → suppressor → token → one `t3521`
/// overseas-index snapshot (`kind="S"`, `symbol="DJI@DJI"`). Flip gate (R4): the
/// snapshot `close` (현재지수) must be a substantive non-default value; an empty
/// out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3521`"]
async fn live_smoke_t3521() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T3521Request::new("S", "DJI@DJI");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().overseas_index_quote(&req).await {
        Ok(resp) => {
            assert_nonempty_witness("close", &resp.outblock.close)
                .expect("live-smoke-t3521: index close must be a substantive witness (R4 / 00707 PENDING)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "index-snapshot")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t3521", &format!("env=paper kind=S symbol=DJI@DJI date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3521 market-data failure (not evidence)");
            panic!("live-smoke-t3521 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t8462`: KRX night-derivatives investor-period table
/// (`bsc_asts_id=K2I`, recent date range). LS_SMOKE_LANE=domestic_option (…51).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8462`"]
async fn live_smoke_t8462() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T8462Request::new("K2I", "20260601", "20260626");
    match sdk.market_session().night_derivatives_investor_period(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t8462: empty out-block (00707) — PENDING");
            assert_nonempty_witness("sv_01", &resp.outblock1[0].sv_01)
                .expect("live-smoke-t8462: individual net-buy volume (sv_01) must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "krx-night-inv")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8462", "env=paper bsc_asts_id=K2I range=20260601-20260626", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8462 market-data failure (not evidence)");
            panic!("live-smoke-t8462 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t1926`: one `t1926` credit info read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1926`"]
async fn live_smoke_t1926() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1926Request::new("005930");
    match sdk.market_session().credit_info(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock.mmdate.is_empty(), "live-smoke-t1926: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1926", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1926 market-data failure (not evidence)");
            panic!("live-smoke-t1926 failed: {e}");
        }
    }
}
