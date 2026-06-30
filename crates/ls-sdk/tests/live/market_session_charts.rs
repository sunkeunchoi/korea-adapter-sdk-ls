use super::*;


/// `make live-smoke-t1901`: paper guard → token → one `t1901` ETF quote read for
/// `shcode="069500"` (KODEX 200). A success `rsp_cd` with a non-empty `hname`/price
/// proves the typed read round-trips. KRX-session-dependent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1901`"]
async fn live_smoke_t1901() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1901_SHCODE").unwrap_or_else(|_| "069500".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().etf_quote(&T1901Request::new(&shcode)).await {
        Ok(resp) => {
            // Non-empty guard: a session-dependent read can return 00000 with empty
            // fields off-session — that is PENDING, not Implemented (mirror t1305).
            assert!(
                !resp.outblock.hname.is_empty() && !resp.outblock.price.is_empty(),
                "live-smoke-t1901: empty ETF quote (00707/off-session) — PENDING, not Implemented"
            );
            record(
                "live-smoke-t1901",
                &format!("env=paper shcode={shcode} date={date}"),
                &format!(
                    "rsp_cd={} hname_len={} price={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.price
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1901 market-data failure (not evidence)");
            panic!("live-smoke-t1901 failed: {e}");
        }
    }
}

/// `make live-smoke-t1906`: paper guard → token → one `t1906` ETF LP order-book
/// (LP호가) read for `shcode="152100"` (default). A success `rsp_cd` with a
/// non-empty `hname`/`price` proves the typed read round-trips. ETF LP order-book
/// is a persistent (static) read reachable under closure; an empty `00707` does NOT
/// record — it dispositions to PENDING. The recorded line is credential-free
/// (`rsp_cd` + lengths/price, never `rsp_msg`); a failed run emits a `SMOKE-FAIL`
/// stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1906`"]
async fn live_smoke_t1906() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1906_SHCODE").unwrap_or_else(|_| "152100".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().etf_lp_order_book(&T1906Request::new(&shcode)).await {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with empty fields off-data →
            // that is PENDING, not Implemented (mirror t1901). Assert a modeled
            // non-default field before recording.
            assert!(
                !resp.outblock.hname.is_empty() && !resp.outblock.price.is_empty(),
                "live-smoke-t1906: empty ETF LP order-book (00707/off-data) — PENDING, not Implemented"
            );
            record(
                "live-smoke-t1906",
                &format!("env=paper shcode={shcode} date={date}"),
                &format!(
                    "rsp_cd={} hname_len={} price={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.price
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1906 market-data failure (not evidence)");
            panic!("live-smoke-t1906 failed: {e}");
        }
    }
}

/// `make live-smoke-t1308`: paper guard → token → one `t1308` time-bucketed
/// trade-chart read for `shcode="005930"` `starttime=""` `endtime=""` `bun_term="1"`
/// `exchgubun=""` (defaults). A success `rsp_cd` with a non-empty time-bucket row
/// (modeled `chetime`/`price`) proves the typed array read round-trips. The chart
/// is reachable under closure; an empty `00707` does NOT record — it dispositions
/// to PENDING. The recorded line is credential-free (`rsp_cd` + row count + lengths,
/// never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr line, never a
/// `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1308`"]
async fn live_smoke_t1308() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1308_SHCODE").unwrap_or_else(|_| "005930".into());
    let starttime = std::env::var("LS_LIVE_SMOKE_T1308_STARTTIME").unwrap_or_else(|_| "".into());
    let endtime = std::env::var("LS_LIVE_SMOKE_T1308_ENDTIME").unwrap_or_else(|_| "".into());
    let bun_term = std::env::var("LS_LIVE_SMOKE_T1308_BUN_TERM").unwrap_or_else(|_| "1".into());
    let exchgubun = std::env::var("LS_LIVE_SMOKE_T1308_EXCHGUBUN").unwrap_or_else(|_| "".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .time_bucket_trade_chart(&T1308Request::new(&shcode, &starttime, &endtime, &bun_term, &exchgubun))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty time-bucket row with
            // a modeled non-default field before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.chetime.is_empty() && !r.price.is_empty()),
                "live-smoke-t1308: empty chart (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1308",
                &format!("env=paper shcode={shcode} starttime={starttime} endtime={endtime} bun_term={bun_term} exchgubun={exchgubun} date={date}"),
                &format!(
                    "rsp_cd={} rows={} chetime_len={} price_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.chetime.len(),
                    row.price.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1308 market-data failure (not evidence)");
            panic!("live-smoke-t1308 failed: {e}");
        }
    }
}

/// `make live-smoke-t1449`: paper guard → token → one `t1449` price-band
/// trade-weight read for `shcode="005930"` `dategb="1"` (defaults; `dategb` MUST
/// be non-empty — an empty `dategb` returns an empty board). A success `rsp_cd`
/// with a non-empty price-band row (modeled `price`/`cvolume`) proves the typed
/// array read round-trips. Reachable under closure; an empty `00707` does NOT
/// record — it dispositions to PENDING. The recorded line is credential-free
/// (`rsp_cd` + row count + lengths, never `rsp_msg`); a failed run emits a
/// `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1449`"]
async fn live_smoke_t1449() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1449_SHCODE").unwrap_or_else(|_| "005930".into());
    let dategb = std::env::var("LS_LIVE_SMOKE_T1449_DATEGB").unwrap_or_else(|_| "1".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .price_band_trade_weight(&T1449Request::new(&shcode, &dategb))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty price-band row with
            // a modeled non-default field before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.price.is_empty() && !r.cvolume.is_empty()),
                "live-smoke-t1449: empty board (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1449",
                &format!("env=paper shcode={shcode} dategb={dategb} date={date}"),
                &format!(
                    "rsp_cd={} rows={} price_len={} cvolume_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.price.len(),
                    row.cvolume.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1449 market-data failure (not evidence)");
            panic!("live-smoke-t1449 failed: {e}");
        }
    }
}

/// `make live-smoke-t1621`: paper guard → token → one `t1621` by-time
/// investor-trading read for `upcode="001"` `nmin=0` `cnt=20` `bgubun="0"`
/// `exchgubun=""` (defaults). `nmin`/`cnt` wire-serialize as JSON NUMBERS — the
/// string form returns IGW40011 (KTD3). A success `rsp_cd` with a non-empty
/// by-time row (modeled `date`/`indmsvol`) proves the typed array read
/// round-trips. Reachable under closure; an empty `00707` does NOT record — it
/// dispositions to PENDING. The recorded line is credential-free (`rsp_cd` + row
/// count + lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr
/// line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1621`"]
async fn live_smoke_t1621() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let upcode = std::env::var("LS_LIVE_SMOKE_T1621_UPCODE").unwrap_or_else(|_| "001".into());
    let nmin = std::env::var("LS_LIVE_SMOKE_T1621_NMIN").unwrap_or_else(|_| "0".into());
    let cnt = std::env::var("LS_LIVE_SMOKE_T1621_CNT").unwrap_or_else(|_| "20".into());
    let bgubun = std::env::var("LS_LIVE_SMOKE_T1621_BGUBUN").unwrap_or_else(|_| "0".into());
    let exchgubun = std::env::var("LS_LIVE_SMOKE_T1621_EXCHGUBUN").unwrap_or_default();
    match sdk
        .market_session()
        .investor_trading_by_time(&T1621Request::new(&upcode, &nmin, &cnt, &bgubun, &exchgubun))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty by-time row with a
            // modeled non-default key before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.date.is_empty() && !r.indmsvol.is_empty()),
                "live-smoke-t1621: empty board (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1621",
                &format!("env=paper upcode={upcode} nmin={nmin} cnt={cnt} bgubun={bgubun}"),
                &format!(
                    "rsp_cd={} rows={} date_len={} indmsvol_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.date.len(),
                    row.indmsvol.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1621 market-data failure (not evidence)");
            panic!("live-smoke-t1621 failed: {e}");
        }
    }
}

/// `make live-smoke-t2545`: paper guard → token → one `t2545` F/O by-time
/// investor-trading read for `eitem="01"` `sgubun="0"` `upcode="001"` `nmin=0`
/// `cnt=10` `bgubun="0"` (defaults). `nmin`/`cnt` wire-serialize as JSON NUMBERS
/// — the string form returns IGW40011 (KTD3); `bgubun="1"` returns
/// IGW40011/IGW50008, so the default is `bgubun="0"`. A success `rsp_cd` with a
/// non-empty by-time row (modeled `date`/`indmsvol`) proves the typed array read
/// round-trips. Reachable under closure; an empty `00707` does NOT record — it
/// dispositions to PENDING. The recorded line is credential-free (`rsp_cd` + row
/// count + lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr
/// line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2545`"]
async fn live_smoke_t2545() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let eitem = std::env::var("LS_LIVE_SMOKE_T2545_EITEM").unwrap_or_else(|_| "01".into());
    let sgubun = std::env::var("LS_LIVE_SMOKE_T2545_SGUBUN").unwrap_or_else(|_| "0".into());
    let upcode = std::env::var("LS_LIVE_SMOKE_T2545_UPCODE").unwrap_or_else(|_| "001".into());
    let nmin = std::env::var("LS_LIVE_SMOKE_T2545_NMIN").unwrap_or_else(|_| "0".into());
    let cnt = std::env::var("LS_LIVE_SMOKE_T2545_CNT").unwrap_or_else(|_| "10".into());
    let bgubun = std::env::var("LS_LIVE_SMOKE_T2545_BGUBUN").unwrap_or_else(|_| "0".into());
    match sdk
        .market_session()
        .fo_investor_trading_by_time(&T2545Request::new(
            &eitem, &sgubun, &upcode, &nmin, &cnt, &bgubun,
        ))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty by-time row with a
            // modeled non-default key before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.date.is_empty() && !r.indmsvol.is_empty()),
                "live-smoke-t2545: empty board (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t2545",
                &format!("env=paper eitem={eitem} sgubun={sgubun} upcode={upcode} nmin={nmin} cnt={cnt} bgubun={bgubun}"),
                &format!(
                    "rsp_cd={} rows={} date_len={} indmsvol_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.date.len(),
                    row.indmsvol.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2545 market-data failure (not evidence)");
            panic!("live-smoke-t2545 failed: {e}");
        }
    }
}

/// `make live-smoke-t1902`: paper guard → token → one `t1902` ETF intraday NAV/price
/// trend read for a public ETF `shcode` (no account secrets). A success `rsp_cd` with
/// a non-empty `t1902OutBlock1` row (modeled `nav`) proves the typed time-series read
/// round-trips. An empty `00707` does NOT record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1902`"]
async fn live_smoke_t1902() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1902_SHCODE").unwrap_or_else(|_| "069500".to_string());
    match sdk
        .market_session()
        .etf_intraday_trend(&T1902Request::new(&shcode, ""))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.nav.is_empty()),
                "live-smoke-t1902: empty time-series (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1902",
                &format!("env=paper shcode={shcode}"),
                &format!(
                    "rsp_cd={} rows={} nav_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.nav.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1902 ETF intraday-trend failure (not evidence)");
            panic!("live-smoke-t1902 failed: {e}");
        }
    }
}

/// `make live-smoke-t1904`: paper guard → token → one `t1904` ETF PDF/constituent
/// read for a public ETF `shcode` on a recent apply date (no account secrets). A
/// success `rsp_cd` with a non-empty `t1904OutBlock1` constituent row (modeled
/// `hname`) proves the typed basket read round-trips. An empty `00707` does NOT
/// record — PENDING (retry the prior trading day via LS_LIVE_SMOKE_T1904_DATE).
/// Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1904`"]
async fn live_smoke_t1904() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1904_SHCODE").unwrap_or_else(|_| "069500".to_string());
    let date = std::env::var("LS_LIVE_SMOKE_T1904_DATE")
        .or_else(|_| std::env::var("LS_LIVE_SMOKE_DATE"))
        .unwrap_or_else(|_| "20260629".to_string());
    match sdk
        .market_session()
        .etf_constituents(&T1904Request::new(&shcode, &date, "1"))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.hname.is_empty()),
                "live-smoke-t1904: empty constituents (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1904",
                &format!("env=paper shcode={shcode} date={date}"),
                &format!(
                    "rsp_cd={} rows={} hname_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.hname.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1904 ETF constituents failure (not evidence)");
            panic!("live-smoke-t1904 failed: {e}");
        }
    }
}

/// `make live-smoke-t1959`: paper guard → token → one `t1959` LP-target ELW
/// issue-list read with an EMPTY `shcode` (the full LP-target list — this is a
/// list/ranking read). A success `rsp_cd` with a non-empty per-issue row (modeled
/// `shcode`/`price`) proves the typed array read round-trips. Reachable under
/// closure; an empty `00707` does NOT record — it dispositions to PENDING. The
/// recorded line is credential-free (`rsp_cd` + row count + lengths, never
/// `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE`
/// one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1959`"]
async fn live_smoke_t1959() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    // Empty shcode → the full LP-target list. Override with LS_LIVE_SMOKE_T1959_SHCODE.
    let shcode = std::env::var("LS_LIVE_SMOKE_T1959_SHCODE").unwrap_or_default();
    match sdk
        .market_session()
        .lp_target_issues(&T1959Request::for_shcode(&shcode))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty per-issue row with
            // a modeled non-default key before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.shcode.is_empty()),
                "live-smoke-t1959: empty board (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1959",
                &format!("env=paper shcode_len={}", shcode.len()),
                &format!(
                    "rsp_cd={} rows={} shcode_len={} price_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.shcode.len(),
                    row.price.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1959 elw failure (not evidence)");
            panic!("live-smoke-t1959 failed: {e}");
        }
    }
}

/// `make live-smoke-t0167`: paper guard → read-only `t0167` server-time utility.
///
/// A stateless utility, closure-viable (the gateway clock is always populated).
/// Not account-scoped, so no dispatch-log suppressor is needed. The recorded line
/// is credential-free: `rsp_cd` + a boolean for whether the `time` witness is
/// non-empty. A failed run emits a distinct `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs LS paper credentials; run via `make live-smoke-t0167`"]
async fn live_smoke_t0167() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().server_time(&T0167Request::new()).await {
        Ok(resp) => {
            let line = format!(
                "rsp_cd={} time_nondefault={}",
                resp.rsp_cd,
                !resp.outblock.time.is_empty()
            );
            record("live-smoke-t0167", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t0167 server-time failure (not evidence)");
            // No `{e}`: keep the panic payload free of any gateway message text.
            panic!("live-smoke-t0167 failed — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-nws-t3102`: chained `NWS`→`t3102` smoke — the `t3102` unblock
/// path (뉴스본문).
///
/// Subscribes to the realtime `NWS` (실시간뉴스제목패킷) title feed on a FRESH,
/// isolated `WsManager`, timeboxes a single news frame, and — if one arrives with a
/// non-empty `realkey` — threads that 24-char key into the `t3102` REST read as
/// `sNewsno`. The flip witness is a non-empty `t3102OutBlock2` title (the only
/// modeled block). The WS leg is connection-reachable-only (fire-and-forget
/// subscribe, KTD6), so it never justifies the flip on its own — only the REST
/// out-block does.
///
/// Off-hours the paper `NWS` feed may emit nothing; a no-frame run is the HELD case
/// (no off-hours news), surfaced as a credential-safe `SMOKE-FAIL`, never a
/// capturable `LIVE-SMOKE` line. Records only `rsp_cd` + a title LENGTH — never the
/// `realkey`, the title text, or the body. `LS_NWS_SMOKE_SECS` overrides the wait
/// window (default 30s); `LS_NWS_TR_KEY` overrides the subscribe key (default: all
/// news).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials + a live NWS frame; run via `make live-smoke-nws-t3102`"]
async fn live_smoke_nws_t3102() {
    paper_guard().expect("paper guard must pass for a paper run");
    let config = LsConfig::from_env().expect("config from env");
    assert!(
        config.environment.is_paper(),
        "resolved environment must be Paper"
    );

    let ws_url = ls_core::config::Environment::resolve_ws_url(&config);
    assert!(
        ws_url.contains("29443"),
        "expected the paper WS port 29443, got {ws_url}"
    );

    // Fresh SDK → fresh, isolated WsManager (KTD2 — no shared-manager poisoning).
    let sdk = LsSdk::new(config).expect("sdk construction");
    let ws = sdk.realtime();

    // News title feed: tr_key is permissive (empty = all news; override via
    // LS_NWS_TR_KEY). NWS is not symbol-scoped, so the default catches any frame.
    let tr_key = std::env::var("LS_NWS_TR_KEY").unwrap_or_default();
    let (handle, mut stream) = ws
        .subscribe_typed::<NwsRow>("NWS", &tr_key, WsLane::MarketData)
        .await
        .unwrap_or_else(|e| {
            panic!("subscribe_typed NWS failed (connect/subscribe lifecycle): {e}")
        });

    // Timebox a single news frame; absence is the HELD case (no off-hours emission).
    let secs: u64 = std::env::var("LS_NWS_SMOKE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let frame = timeout(Duration::from_secs(secs), stream.next()).await;

    // Always unsubscribe cleanly before the flip decision.
    handle
        .unsubscribe()
        .await
        .expect("unsubscribe must complete cleanly");

    let realkey = match frame {
        Ok(Some(Ok(row))) if !row.realkey.is_empty() => row.realkey,
        _ => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-nws-t3102 no NWS news frame within {secs}s; \
                 HELD (no off-hours emission, not evidence)"
            );
            panic!("live-smoke-nws-t3102: no live NWS frame — HELD, not Implemented");
        }
    };

    // Chain: feed the captured realkey into t3102 as sNewsno. Never log the key.
    let req = T3102Request::new(realkey);
    match sdk.market_session().news_body(&req).await {
        Ok(resp) if resp.outblock2.title.is_empty() => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-nws-t3102 t3102 empty title block (rsp_cd={}); \
                 HELD (key not queryable, not evidence)",
                resp.rsp_cd
            );
            panic!("live-smoke-nws-t3102: t3102 returned an empty title block — HELD, not Implemented");
        }
        Ok(resp) => record(
            "live-smoke-nws-t3102",
            "env=paper feed=NWS chain=realkey->sNewsno",
            &format!("rsp_cd={} title_len={}", resp.rsp_cd, resp.outblock2.title.len()),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-nws-t3102 t3102 REST failure (not evidence)");
            panic!("live-smoke-nws-t3102 t3102 call failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8401 — 주식선물마스터조회 (stock-futures master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// structural signal is the out-block row count (a single row-array out-block),
// kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8401`: paper guard → OAuth token → one `t8401`
/// stock-futures master read (no caller input). A success `rsp_cd` with a
/// populated `t8401OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8401`"]
async fn live_smoke_t8401() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8401Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8401: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8401", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8401 market-data failure (not evidence)");
            panic!("live-smoke-t8401 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8426 — 상품선물마스터조회 (commodity-futures master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// structural signal is the out-block row count (a single row-array out-block),
// kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8426`: paper guard → OAuth token → one `t8426`
/// commodity-futures master read (no caller input). A success `rsp_cd` with a
/// populated `t8426OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8426`"]
async fn live_smoke_t8426() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8426Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().commodity_futures_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8426: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8426", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8426 market-data failure (not evidence)");
            panic!("live-smoke-t8426 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8433 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// structural signal is the out-block row count (a single row-array out-block),
// kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8433`: paper guard → OAuth token → one `t8433` index-option
/// master read (no caller input). A success `rsp_cd` with a populated
/// `t8433OutBlock` row array proves the read is callable and round-trips. The
/// recorded line is credential-free (only `rsp_cd` + the row count, never
/// `rsp_msg`) and self-dated; a failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8433`"]
async fn live_smoke_t8433() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8433Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_option_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8433: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8433", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8433 market-data failure (not evidence)");
            panic!("live-smoke-t8433 failed: {e}");
        }
    }
}

/// `make live-smoke-t8406`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t8406` F/O by-tick conclusion read for that contract
/// (`cgubun="1"`, `bgubun=0`, `cnt=10`).
///
/// The example `focode` in the raw capture is a stale/expired contract (a static
/// probe returned a clean-empty 60-byte body), so the smoke self-sources a live
/// front-month contract from the t8467 master. `bgubun`/`cnt` wire-serialize as
/// JSON NUMBERS — the string form returns IGW40011 (KTD3). A success `rsp_cd`
/// with a non-empty conclusion row (modeled `chetime`/`price`) proves the typed
/// array read round-trips. F/O conclusion is session-dependent: an empty `00707`
/// even with a live contract does NOT record — it dispositions to PENDING. The
/// recorded line is credential-free (`focode` is public contract reference data,
/// `rsp_cd` + row count + lengths, never `rsp_msg`); a failed run emits a
/// `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8406`"]
async fn live_smoke_t8406() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t8406 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t8406: no contract to key the read");
    }
    let focode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .fo_tick_conclusion(&T8406Request::new(&focode, "1", "0", "10"))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: F/O conclusion can return 00000 with an empty array
            // under closure → that is PENDING, not Implemented. Assert a non-empty
            // conclusion row with a modeled non-default key before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.chetime.is_empty() && !r.price.is_empty()),
                "live-smoke-t8406: empty conclusion board (00707/closure) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t8406",
                &format!("env=paper focode={focode} cgubun=1 bgubun=0 cnt=10 date={date}"),
                &format!(
                    "rsp_cd={} rows={} chetime_len={} price_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    row.chetime.len(),
                    row.price.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8406 market-data failure (not evidence)");
            panic!("live-smoke-t8406 failed: {e}");
        }
    }
}

/// `make live-smoke-t3320`: paper guard → token → one FnGuide company-summary
/// read keyed by a public FnGuide company code (`A005930` = 삼성전자). Routes
/// through `market_session` (KTD3).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3320`"]
async fn live_smoke_t3320() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let gicode = "005930";
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().company_summary(&T3320Request::new(gicode)).await {
        Ok(resp) => {
            if resp.outblock.company.is_empty() && resp.outblock1.gicode.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t3320 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t3320: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "summary")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t3320",
                &format!("env=paper gicode={gicode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3320 market-data failure (not evidence)");
            panic!("live-smoke-t3320 failed: {e}");
        }
    }
}

/// `make live-smoke-g3102`: paper guard → token → one overseas time-series read
/// (`82`/`TSLA`, 30 rows, first page). `readcnt`/`cts_seq` serialize as JSON
/// numbers (KTD4). Empty row array is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3102`"]
async fn live_smoke_g3102() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_time_series(&G3102Request::new("R", "82TSLA", "82", "TSLA", "30", "0"))
        .await
    {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3102 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3102: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ticks")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3102",
                &format!("env=paper exchcd=82 symbol=TSLA readcnt=30 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3102 market-data failure (not evidence)");
            panic!("live-smoke-g3102 failed: {e}");
        }
    }
}

/// `make live-smoke-g3103`: paper guard → token → one overseas period-chart read
/// (`82`/`TSLA`, monthly `gubun="4"`). Empty bar array is the `00707` PENDING
/// case. `date` is the reference date; the public ticker keys the read.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3103`"]
async fn live_smoke_g3103() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let today = Utc::now().format("%Y%m%d").to_string();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_period_chart(&G3103Request::new("R", "82TSLA", "82", "TSLA", "4", &today))
        .await
    {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3103 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3103: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "bars")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3103",
                &format!("env=paper exchcd=82 symbol=TSLA gubun=4 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3103 market-data failure (not evidence)");
            panic!("live-smoke-g3103 failed: {e}");
        }
    }
}

/// `make live-smoke-t3202`: paper guard → token → one `t3202` schedule read for
/// `shcode=001200`. A non-empty schedule array proves the read round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3202`"]
async fn live_smoke_t3202() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T3202Request::new("001200");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_schedule(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t3202: empty schedule (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "events")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t3202", &format!("env=paper shcode=001200 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3202 market-data failure (not evidence)");
            panic!("live-smoke-t3202 failed: {e}");
        }
    }
}

/// `make live-smoke-o3104`: overseas-futures daily executions (`shcode=CUSN26`,
/// `date`=recent weekday).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3104`"]
async fn live_smoke_o3104() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3104Request::new("CUSN26", "20260626");
    match sdk.market_session().overseas_futures_daily(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-o3104: empty out-block (00707) — PENDING");
            assert_nonempty_witness("price", &resp.outblock1[0].price)
                .expect("live-smoke-o3104: 체결가 must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ovs-fut-daily")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3104", "env=paper symbol=CUSN26 date=20260626", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3104 market-data failure (not evidence)");
            panic!("live-smoke-o3104 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t8427`: t8467 front-month source → one `t8427` F/O day chart
/// for that contract. Witness: a non-empty OHLCV row (`close`/`volume`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8427`"]
async fn live_smoke_t8427() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let focode = fo_front_month_shcode(&sdk, "live-smoke-t8427").await;
    let now = Utc::now();
    let (yyyy, mm, date) = (now.format("%Y").to_string(), now.format("%m").to_string(), now.format("%Y%m%d").to_string());
    match sdk
        .market_session()
        .fo_minute_day_chart(&T8427Request::new(&focode, &yyyy, &mm, &date))
        .await
    {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.close.is_empty() || !r.volume.is_empty()),
                "live-smoke-t8427: empty chart (00707/off-data) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "fo-chart")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8427", &format!("env=paper focode={focode} yyyy={yyyy} mm={mm} date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8427 market-data failure (not evidence)");
            panic!("live-smoke-t8427 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t2424`: t8467 front-month source → one `t2424` F/O N-minute
/// bars. Witness: header `price` substantive OR a non-empty bar `close`.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2424`"]
async fn live_smoke_t2424() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let focode = fo_front_month_shcode(&sdk, "live-smoke-t2424").await;
    match sdk.market_session().fo_minute_bars(&T2424Request::new(&focode)).await {
        Ok(resp) => {
            let price_ok = !resp.outblock.price.is_empty() && resp.outblock.price != "0";
            let bar_ok = resp.outblock1.first().is_some_and(|r| !r.close.is_empty());
            assert!(
                price_ok || bar_ok,
                "live-smoke-t2424: empty/no-witness bars (00707/off-data) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "fo-bars")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t2424", &format!("env=paper focode={focode}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2424 market-data failure (not evidence)");
            panic!("live-smoke-t2424 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t2210`: t8467 front-month source → one `t2210` F/O
/// unusual-volume conclusion-count read over the regular window. Witness: a
/// NON-ZERO buy/sell 체결수량 (`msvolume`/`mdvolume`); all-zero is PENDING (AE4 —
/// body length alone never justifies a flip).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2210`"]
async fn live_smoke_t2210() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let focode = fo_front_month_shcode(&sdk, "live-smoke-t2210").await;
    match sdk.market_session().fo_unusual_volume(&T2210Request::new(&focode, "0900", "1530")).await {
        Ok(resp) => {
            let nonzero = |s: &str| !s.is_empty() && s != "0";
            assert!(
                nonzero(&resp.outblock.msvolume) || nonzero(&resp.outblock.mdvolume),
                "live-smoke-t2210: all-zero conclusion counts (off-data) — PENDING, not Implemented (AE4)"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "fo-unusual-vol")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t2210", &format!("env=paper focode={focode} stime=0900 etime=1530"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2210 market-data failure (not evidence)");
            panic!("live-smoke-t2210 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t8428`: one `t8428` deposit-balance trend over a recent date
/// range for `upcode="001"` (MAIN .env, domestic). Witness: a non-empty row
/// (`jisu` 지수 / `custmoney` 고객예탁금).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8428`"]
async fn live_smoke_t8428() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().deposit_balance_trend(&T8428Request::new("20260601", "20260629", "001")).await {
        Ok(resp) => {
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.jisu.is_empty() || !r.custmoney.is_empty()),
                "live-smoke-t8428: empty/no-witness deposit trend (00707/off-data) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "deposit-trend")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8428", &format!("env=paper fdate=20260601 tdate=20260629 upcode=001 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8428 market-data failure (not evidence)");
            panic!("live-smoke-t8428 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}

/// `make live-smoke-t1302`: one `t1302` minute-by-minute price (분별주가).
/// Non-empty minute-row array under closure → round-trips. Plan -004 batch A.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1302`"]
async fn live_smoke_t1302() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1302Request::new("001200", "0", "20");
    match sdk.market_session().minute_prices(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t1302: empty board (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "minutes")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1302", "env=paper shcode=001200 gubun=0", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1302 market-data failure (not evidence)");
            panic!("live-smoke-t1302 failed: {e}");
        }
    }
}

/// `make live-smoke-t2216`: one `t2216` F/O tick trade chart for a current contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2216`"]
async fn live_smoke_t2216() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let focode = current_index_future(&sdk, "live-smoke-t2216").await;
    let req = T2216Request::new(&focode, "T", "20");
    match sdk.market_session().fo_trade_chart(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t2216: empty chart (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "trades")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t2216", &format!("env=paper focode={focode}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2216 market-data failure (not evidence)");
            panic!("live-smoke-t2216 failed: {e}");
        }
    }
}

/// `make live-smoke-t1764`: one `t1764` member firms read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1764`"]
async fn live_smoke_t1764() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1764Request::new("001200");
    match sdk.market_session().member_firms(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock.is_empty(), "live-smoke-t1764: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "firms")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1764", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1764 market-data failure (not evidence)");
            panic!("live-smoke-t1764 failed: {e}");
        }
    }
}

/// `make live-smoke-t1903`: one `t1903` etf daily trend read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1903`"]
async fn live_smoke_t1903() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1903Request::new("448330");
    match sdk.market_session().etf_daily_trend(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1903: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1903", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1903 market-data failure (not evidence)");
            panic!("live-smoke-t1903 failed: {e}");
        }
    }
}
