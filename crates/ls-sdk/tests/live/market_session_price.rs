use super::*;


/// Default `make live-smoke`: paper guard → OAuth token → one `t1102` quote.
///
/// Covers AE2. A non-empty token proves a live round-trip (liveness signal);
/// `quote` returning `Ok` proves market-data transport (a `01900` would `Err`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke`"]
async fn live_smoke_default() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let symbol = resolve_symbol();

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(
        !token.is_empty(),
        "token must be non-empty — proves a live round-trip"
    );

    let req = T1102Request::new(&symbol, "K");
    let resp = sdk
        .market_session()
        .quote(&req)
        .await
        .expect("t1102 quote failed");

    // The recorded line is the Focused Evidence for `token` (see
    // metadata/evidence/token.yaml). It is credential-free and dated *by
    // construction*: `rsp_msg` is dropped (it carries localized,
    // account-identifying text), only the numeric `rsp_cd` proves success, and
    // the run stamps its own UTC date so a verbatim capture cannot reintroduce a
    // secret or a hand-typed date.
    let date = Utc::now().format("%Y-%m-%d");
    record(
        "live-smoke",
        &format!("env=paper symbol={symbol} date={date}"),
        &format!(
            "token_len={} rsp_cd={} price={}",
            token.len(),
            resp.rsp_cd,
            resp.outblock.price
        ),
    );
}

/// `make live-smoke-book`: paper guard → OAuth token → one `t1101` order-book
/// quote. The recorded line is the Focused Evidence candidate for `t1101`
/// (`metadata/evidence/t1101.yaml` on a green run) — credential-free and
/// self-dated by construction. `order_book` returning `Ok` proves market-data
/// transport; a `01900` would `Err` here and drive the AE2 paper-incompatible
/// reclassification (`paper_incompatible: true`, stay `implemented`).
///
/// `symbol` is a public market ticker (Samsung by default); any
/// `LS_LIVE_SMOKE_SHCODE` override must also be a public ticker, never an
/// account number or internal identifier.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-book`"]
async fn live_smoke_book() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let symbol = resolve_symbol();

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(
        !token.is_empty(),
        "token must be non-empty — proves a live round-trip"
    );

    let req = T1101Request::new(&symbol);
    let resp = sdk
        .market_session()
        .order_book(&req)
        .await
        .expect("t1101 order_book failed");

    let date = Utc::now().format("%Y-%m-%d");
    record(
        "live-smoke-book",
        &format!("env=paper symbol={symbol} date={date}"),
        &format!(
            "token_len={} rsp_cd={} price={} offerho1={} bidho1={}",
            token.len(),
            resp.rsp_cd,
            resp.outblock.price,
            resp.outblock.offerho1,
            resp.outblock.bidho1
        ),
    );
}

/// `make live-smoke-t1537`: paper guard → token → fetch one theme via `t8425` →
/// one `t1537` per-stock-quotes read for that theme code.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1537`"]
async fn live_smoke_t1537() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let themes = sdk
        .market_session()
        .all_themes(&T8425Request::new())
        .await
        .expect("t8425 all_themes (theme input source) failed");
    if themes.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1537 t8425 theme source empty (rsp_cd={})",
            themes.rsp_cd
        );
        panic!("live-smoke-t1537: no theme to key the read");
    }
    let tmcode = themes.outblock[0].tmcode.clone();

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .theme_quotes(&T1537Request::new(&tmcode))
        .await
    {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1537",
                &format!("env=paper tmcode={tmcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1537 market-data failure (not evidence)");
            panic!("live-smoke-t1537 failed: {e}");
        }
    }
}

/// `make live-smoke-t8450`: paper guard → token → one `t8450` integrated
/// current-price/order-book read for `shcode="005930"` `exchgubun="N"` (defaults). A
/// success `rsp_cd` with a non-empty `hname`/`price` proves the typed read round-trips.
/// The current-price/order-book snapshot is reachable under closure; an empty `00707`
/// does NOT record — it dispositions to PENDING. The recorded line is credential-free
/// (`rsp_cd` + lengths/price, never `rsp_msg`); a failed run emits a `SMOKE-FAIL`
/// stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8450`"]
async fn live_smoke_t8450() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T8450_SHCODE").unwrap_or_else(|_| "005930".into());
    let exchgubun =
        std::env::var("LS_LIVE_SMOKE_T8450_EXCHGUBUN").unwrap_or_else(|_| "N".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .current_price_orderbook(&T8450Request::new(&shcode, &exchgubun))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with empty fields off-data →
            // that is PENDING, not Implemented (mirror t1906). Assert a modeled
            // non-default field before recording.
            assert!(
                !resp.outblock.hname.is_empty() && !resp.outblock.price.is_empty(),
                "live-smoke-t8450: empty current-price/order-book (00707/off-data) — PENDING, not Implemented"
            );
            record(
                "live-smoke-t8450",
                &format!("env=paper shcode={shcode} exchgubun={exchgubun} date={date}"),
                &format!(
                    "rsp_cd={} hname_len={} price={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.price
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8450 market-data failure (not evidence)");
            panic!("live-smoke-t8450 failed: {e}");
        }
    }
}

/// `make live-smoke-t8407`: paper guard → token → one `t8407` multi-symbol
/// current-price read for `nrec=3` `shcode="005930000660001200"` (Samsung +
/// SK Hynix + 001200; defaults). `nrec` wire-serializes as a JSON NUMBER — the
/// string form returns IGW40011 (KTD3). A success `rsp_cd` with a non-empty
/// per-symbol row (modeled `shcode`/`price`) proves the typed array read
/// round-trips. Reachable under closure; an empty `00707` does NOT record — it
/// dispositions to PENDING. The recorded line is credential-free (`rsp_cd` + row
/// count + lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr
/// line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8407`"]
async fn live_smoke_t8407() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let nrec = std::env::var("LS_LIVE_SMOKE_T8407_NREC").unwrap_or_else(|_| "3".into());
    let shcode = std::env::var("LS_LIVE_SMOKE_T8407_SHCODE")
        .unwrap_or_else(|_| "005930000660001200".into());
    match sdk
        .market_session()
        .multi_symbol_current_price(&T8407Request::new(&nrec, &shcode))
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a read can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty per-symbol row with
            // a modeled non-default key before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.shcode.is_empty()),
                "live-smoke-t8407: empty board (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t8407",
                &format!("env=paper nrec={nrec}"),
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
            eprintln!("SMOKE-FAIL target=live-smoke-t8407 market-data failure (not evidence)");
            panic!("live-smoke-t8407 failed: {e}");
        }
    }
}

/// `make live-smoke-t1471`: paper guard → token → one `t1471` intraday
/// quote-remainder trend read (a public ticker). A success `rsp_cd` with a non-empty
/// scalar `t1471OutBlock.price` proves the typed read round-trips. An empty `00707`
/// does NOT record — PENDING. Credential-free line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1471`"]
async fn live_smoke_t1471() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1471_SHCODE").unwrap_or_else(|_| "005930".to_string());
    let cnt = std::env::var("LS_LIVE_SMOKE_T1471_CNT").unwrap_or_else(|_| "20".to_string());
    match sdk
        .market_session()
        .intraday_quote_remainder_trend(&T1471Request::new(&shcode, "0", "", &cnt, "1"))
        .await
    {
        Ok(resp) => {
            assert!(
                !resp.outblock.price.is_empty(),
                "live-smoke-t1471: empty quote (00707/off-data) — PENDING, not Implemented"
            );
            record(
                "live-smoke-t1471",
                &format!("env=paper shcode={shcode} cnt={cnt}"),
                &format!(
                    "rsp_cd={} price_len={} book_rows={}",
                    resp.rsp_cd,
                    resp.outblock.price.len(),
                    resp.outblock1.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1471 quote-remainder failure (not evidence)");
            panic!("live-smoke-t1471 failed: {e}");
        }
    }
}

/// `make live-smoke-t1105`: paper guard → token → one `t1105` pivot/demark read for
/// `shcode="005930"` `exchgubun="K"`. Success `rsp_cd` + non-empty shcode → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1105`"]
async fn live_smoke_t1105() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let shcode = std::env::var("LS_LIVE_SMOKE_T1105_SHCODE").unwrap_or_else(|_| "005930".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().pivot_demark(&T1105Request::new(&shcode, "K")).await {
        Ok(resp) => {
            // Non-empty guard: off-session can return 00000 with empty fields → PENDING.
            assert!(
                !resp.outblock.shcode.is_empty() && !resp.outblock.pbot.is_empty(),
                "live-smoke-t1105: empty pivot/demark (00707/off-session) — PENDING, not Implemented"
            );
            record(
                "live-smoke-t1105",
                &format!("env=paper shcode={shcode} date={date}"),
                &format!("rsp_cd={} shcode={} pbot={}", resp.rsp_cd, resp.outblock.shcode, resp.outblock.pbot),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1105 market-data failure (not evidence)");
            panic!("live-smoke-t1105 failed: {e}");
        }
    }
}

/// `make live-smoke-t1104`: paper guard → token → one `t1104` price-memo read for
/// `code="005930"` `nrec="1"` `exchgubun="K"`. Success `rsp_cd` → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1104`"]
async fn live_smoke_t1104() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let code = std::env::var("LS_LIVE_SMOKE_T1104_CODE").unwrap_or_else(|_| "005930".into());
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().price_memo(&T1104Request::new(&code, "1", "K")).await {
        Ok(resp) => {
            // Non-empty guard: a success rsp_cd with zero memo rows is an empty result
            // (off-session / 00707) → PENDING, not Implemented.
            assert!(
                resp.rsp_cd == "00000" && !resp.outblock1.is_empty(),
                "live-smoke-t1104: empty price memo (rsp_cd={}, rows=0) — PENDING, not Implemented",
                resp.rsp_cd
            );
            record(
                "live-smoke-t1104",
                &format!("env=paper code={code} date={date}"),
                &format!("rsp_cd={} nrec={} rows={}", resp.rsp_cd, resp.outblock.nrec, resp.outblock1.len()),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1104 market-data failure (not evidence)");
            panic!("live-smoke-t1104 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t2301 — 옵션전광판 (option board; F/O). market_session, non-paginated. Keyed by
// a near-quarterly contract month (`yyyymm`) + a `gubun` mini/regular selector.
// Master/board read — non-empty regardless of the KRX session (venue facet stays
// provisional). The structural signal is the canonical field's length (a single
// out-block, not an array), kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t2301`: paper guard → OAuth token → one `t2301` option-board
/// read for `yyyymm="202609"`, `gubun="G"` (정규/regular). A success `rsp_cd`
/// with a populated board header proves the read is callable and round-trips. The
/// recorded line is credential-free (only `rsp_cd` + the canonical `gmprice`
/// field's length, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2301`"]
async fn live_smoke_t2301() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T2301Request::new("202609", "G");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().option_board(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.gmprice.is_empty(),
                "live-smoke-t2301: empty result (00707) — PENDING, not Implemented"
            );
            let line =
                smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.gmprice.len())), "gmprice_len")
                    .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2301",
                &format!("env=paper yyyymm=202609 gubun=G date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2301 market-data failure (not evidence)");
            panic!("live-smoke-t2301 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// U5 (reach wave) — F/O quote/master reads. All `/futureoption/market-data`,
// `[선물/옵션] 시세`, non-paginated market_session. Each smoke self-sources a live
// contract code from an F/O master (t8467 index-futures master / t8401
// stock-futures master) so it needs no hardcoded contract code; the example
// codes in the raw capture are stale. One "anytime F/O" probe covers the lane.
// Out-block keys + array-ness were read from the RAW capture (KTD5).
// ---------------------------------------------------------------------------

/// `make live-smoke-t2111`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t2111` F/O current-price read for that contract.
///
/// `focode` is public contract reference data (printed); credential-free, self-dated.
/// A failure emits SMOKE-FAIL, never a LIVE-SMOKE line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2111`"]
async fn live_smoke_t2111() {
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
            "SMOKE-FAIL target=live-smoke-t2111 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t2111: no contract to key the read");
    }
    let focode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_quote(&T2111Request::new(&focode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.focode.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t2111 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t2111: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2111",
                &format!("env=paper focode={focode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2111 market-data failure (not evidence)");
            panic!("live-smoke-t2111 failed: {e}");
        }
    }
}

/// `make live-smoke-t2112`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t2112` F/O order-book read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2112`"]
async fn live_smoke_t2112() {
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
            "SMOKE-FAIL target=live-smoke-t2112 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t2112: no contract to key the read");
    }
    let shcode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_order_book(&T2112Request::new(&shcode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.shcode.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t2112 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t2112: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2112",
                &format!("env=paper shcode={shcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2112 market-data failure (not evidence)");
            panic!("live-smoke-t2112 failed: {e}");
        }
    }
}

/// `make live-smoke-t2106`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t2106` F/O price-memo read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2106`"]
async fn live_smoke_t2106() {
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
            "SMOKE-FAIL target=live-smoke-t2106 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t2106: no contract to key the read");
    }
    let code = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_price_memo(&T2106Request::new(&code)).await {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t2106 empty memo array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t2106: empty memo array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "memos")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2106",
                &format!("env=paper code={code} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2106 market-data failure (not evidence)");
            panic!("live-smoke-t2106 failed: {e}");
        }
    }
}

/// `make live-smoke-t8402`: paper guard → token → fetch one stock-futures contract
/// via `t8401` → one `t8402` stock-futures current-price read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8402`"]
async fn live_smoke_t8402() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .stock_futures_master(&T8401Request::new())
        .await
        .expect("t8401 stock-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t8402 t8401 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t8402: no contract to key the read");
    }
    let focode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_quote(&T8402Request::new(&focode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.hname.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8402 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8402: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8402",
                &format!("env=paper focode={focode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8402 market-data failure (not evidence)");
            panic!("live-smoke-t8402 failed: {e}");
        }
    }
}

/// `make live-smoke-t8403`: paper guard → token → fetch one stock-futures contract
/// via `t8401` → one `t8403` stock-futures order-book read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8403`"]
async fn live_smoke_t8403() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .stock_futures_master(&T8401Request::new())
        .await
        .expect("t8401 stock-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t8403 t8401 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t8403: no contract to key the read");
    }
    let shcode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_order_book(&T8403Request::new(&shcode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.hname.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8403 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8403: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8403",
                &format!("env=paper shcode={shcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8403 market-data failure (not evidence)");
            panic!("live-smoke-t8403 failed: {e}");
        }
    }
}

/// `make live-smoke-t8434`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t8434` F/O multi current-price read (qrycnt=1) for it.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8434`"]
async fn live_smoke_t8434() {
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
            "SMOKE-FAIL target=live-smoke-t8434 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t8434: no contract to key the read");
    }
    let focode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_multi_quote(&T8434Request::new("1", &focode)).await {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8434 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8434: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8434",
                &format!("env=paper qrycnt=1 focode={focode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8434 market-data failure (not evidence)");
            panic!("live-smoke-t8434 failed: {e}");
        }
    }
}

/// `make live-smoke-g3101`: paper guard → token → one overseas current-price
/// read keyed by a public US ticker (`82`/`TSLA` = TSLA on NASDAQ). Domain
/// `overseas_stock`, routes through `market_session` (KTD3). An empty out-block
/// (`price` empty) is the `00707` PENDING case, not Implemented.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3101`"]
async fn live_smoke_g3101() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_quote(&G3101Request::new("R", "82TSLA", "82", "TSLA"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3101 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3101: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3101",
                &format!("env=paper exchcd=82 symbol=TSLA date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3101 market-data failure (not evidence)");
            panic!("live-smoke-g3101 failed: {e}");
        }
    }
}

/// `make live-smoke-g3106`: paper guard → token → one overseas current-price +
/// order-book read (`82`/`TSLA`). Routes through `market_session` (KTD3). Empty
/// `price` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3106`"]
async fn live_smoke_g3106() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_order_book(&G3106Request::new("R", "82TSLA", "82", "TSLA"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3106 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3106: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3106",
                &format!("env=paper exchcd=82 symbol=TSLA date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3106 market-data failure (not evidence)");
            panic!("live-smoke-g3106 failed: {e}");
        }
    }
}

/// `make live-smoke-o3105`: paper guard → token → one overseas-futures
/// current-price read keyed by a public symbol (`CUSN26`). Routes through
/// `market_session` (KTD3). Empty `trd_p` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3105`"]
async fn live_smoke_o3105() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_futures_quote(&O3105Request::new("CUSN26  "))
        .await
    {
        Ok(resp) => {
            if resp.outblock.trd_p.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3105 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3105: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3105",
                &format!("env=paper symbol=CUSN26 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3105 market-data failure (not evidence)");
            panic!("live-smoke-o3105 failed: {e}");
        }
    }
}

/// `make live-smoke-o3106`: paper guard → token → one overseas-futures
/// current-price + order-book read (`HSIM26`). Routes through `market_session`
/// (KTD3). Empty `price` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3106`"]
async fn live_smoke_o3106() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_futures_order_book(&O3106Request::new("HSIM26"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3106 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3106: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3106",
                &format!("env=paper symbol=HSIM26 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3106 market-data failure (not evidence)");
            panic!("live-smoke-o3106 failed: {e}");
        }
    }
}

/// `make live-smoke-o3125`: paper guard → token → one overseas-future-option
/// current-price read (`mktgb="F"`, `HSIM26`). Routes through `market_session`
/// (KTD3). Empty `trd_p` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3125`"]
async fn live_smoke_o3125() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_option_quote(&O3125Request::new("F", "HSIM26          "))
        .await
    {
        Ok(resp) => {
            if resp.outblock.trd_p.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3125 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3125: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3125",
                &format!("env=paper mktgb=F symbol=HSIM26 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3125 market-data failure (not evidence)");
            panic!("live-smoke-o3125 failed: {e}");
        }
    }
}

/// `make live-smoke-o3126`: paper guard → token → one overseas-future-option
/// current-price + order-book read (`mktgb="F"`, `HSIM26`). Routes through
/// `market_session` (KTD3). Empty `price` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3126`"]
async fn live_smoke_o3126() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_option_order_book(&O3126Request::new("F", "HSIM26"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3126 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3126: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3126",
                &format!("env=paper mktgb=F symbol=HSIM26 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3126 market-data failure (not evidence)");
            panic!("live-smoke-o3126 failed: {e}");
        }
    }
}

/// `make live-smoke-o3127`: overseas-futopt watchlist board for one watched symbol
/// (`mktgb=0`, `symbol=CUSN26` — the current overseas-futures front-month). An
/// `nrec`-only request returns placeholder rows with a zero `price`; the watched
/// `o3127InBlock1` symbol is what makes the board return a real quote.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3127`"]
async fn live_smoke_o3127() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (fail-closed)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = O3127Request::new("0", "CUSN26");
    match sdk.market_session().overseas_futopt_watchlist(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock.is_empty(), "live-smoke-o3127: empty out-block (00707) — PENDING");
            assert_nonempty_witness("price", &resp.outblock[0].price)
                .expect("live-smoke-o3127: 현재가 must be substantive (R4)");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "ovs-futopt-board")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-o3127", "env=paper mktgb=0 symbol=CUSN26", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3127 market-data failure (not evidence)");
            panic!("live-smoke-o3127 failed: {}", scrub_secrets(&e.to_string()));
        }
    }
}
