use super::*;


/// `make live-smoke-t8425`: paper guard → OAuth token → one `t8425` all-themes
/// read. The pilot for the `tracked → implemented` recipe.
///
/// `all_themes` returning `Ok` with a non-empty `outblock` proves the read is
/// callable and the response shape round-trips. The recorded line is
/// credential-free by construction (only `rsp_cd` + a public theme count; no
/// `rsp_msg`, token, or account text) and self-dated. A failed run emits a
/// distinct `SMOKE-FAIL` stderr line — never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8425`"]
async fn live_smoke_t8425() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(
        !token.is_empty(),
        "token must be non-empty — proves a live round-trip"
    );

    let req = T8425Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().all_themes(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "themes")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8425",
                &format!("env=paper date={date}"),
                &line,
            );
        }
        Err(e) => {
            // No capturable LIVE-SMOKE line on failure (R3a): the Err arm never
            // calls record(); the smoke_result(Err) -> None contract is proven by
            // the offline test `smoke_result_err_path_emits_no_live_smoke_line`.
            eprintln!("SMOKE-FAIL target=live-smoke-t8425 market-data failure (not evidence)");
            panic!("live-smoke-t8425 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8436 — 주식종목조회 (stock master list). market_session, non-paginated; takes
// a `gubun` market-segment filter (not an instrument identifier).
// ---------------------------------------------------------------------------

/// `make live-smoke-t8436`: paper guard → OAuth token → one `t8436` stock-list
/// read for `gubun="0"` (전체/all segments).
///
/// `stock_list` returning `Ok` with a non-empty `outblock` proves the read is
/// callable and the row shape round-trips. The recorded line is credential-free
/// (only `rsp_cd` + a public row count) and self-dated; a failed run emits a
/// distinct `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8436`"]
async fn live_smoke_t8436() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8436Request::new("0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_list(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "stocks")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8436",
                &format!("env=paper gubun=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8436 market-data failure (not evidence)");
            panic!("live-smoke-t8436 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t1531 / t1537 — theme-keyed reads. market_session, non-paginated. Each smoke
// self-sources a representative theme from t8425 (the plan's "one-off t8425 call"
// input source), so it needs no hardcoded theme code.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1531`: paper guard → token → fetch one theme via `t8425` →
/// one `t1531` theme-constituents read for that theme.
///
/// `tmcode` is public theme reference data (printed); `tmname` is not printed.
/// Credential-free, self-dated; failure emits SMOKE-FAIL, never a LIVE-SMOKE line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1531`"]
async fn live_smoke_t1531() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let themes = sdk
        .market_session()
        .all_themes(&T8425Request::new())
        .await
        .expect("t8425 all_themes (theme input source) failed");
    // all_themes returns Ok with an empty out-block on a 00707 (success-but-empty);
    // surface that as a credential-safe SMOKE-FAIL with the rsp_cd, not an opaque
    // .expect() panic, so an off-session empty is distinguishable from a defect.
    if themes.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1531 t8425 theme source empty (rsp_cd={})",
            themes.rsp_cd
        );
        panic!("live-smoke-t1531: no theme to key the read");
    }
    let theme = &themes.outblock[0];
    let (tmname, tmcode) = (theme.tmname.clone(), theme.tmcode.clone());

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .theme_stocks(&T1531Request::new(&tmname, &tmcode))
        .await
    {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1531",
                &format!("env=paper tmcode={tmcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1531 market-data failure (not evidence)");
            panic!("live-smoke-t1531 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Wave 1 — ELW universe/list reads (t9905, t9907, t8431, t9942). No-caller-input
// `dummy` reads; each gates on a non-empty success.
// ---------------------------------------------------------------------------

/// `make live-smoke-t9905`: paper guard → token → one `t9905` underlying-asset
/// list read (no caller input). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9905`"]
async fn live_smoke_t9905() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().underlying_list(&T9905Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "underlyings")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9905", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9905 market-data failure (not evidence)");
            panic!("live-smoke-t9905 failed: {e}");
        }
    }
}

/// `make live-smoke-t9907`: paper guard → token → one `t9907` ELW expiry-month
/// list read (no caller input). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9907`"]
async fn live_smoke_t9907() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .elw_expiry_months(&T9907Request::new())
        .await
    {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "months")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9907", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9907 market-data failure (not evidence)");
            panic!("live-smoke-t9907 failed: {e}");
        }
    }
}

/// `make live-smoke-t8431`: paper guard → token → one `t8431` ELW-symbol list
/// read (no caller input; the Wave 1 spine producer). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8431`"]
async fn live_smoke_t8431() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().elw_symbols(&T8431Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "elws")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8431", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8431 market-data failure (not evidence)");
            panic!("live-smoke-t8431 failed: {e}");
        }
    }
}

/// `make live-smoke-t8430`: paper guard → token → one `t8430` stock-issue list
/// read (no caller input; `gubun="0"` = all markets). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8430`"]
async fn live_smoke_t8430() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_issues(&T8430Request::all()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "issues")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8430", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8430 market-data failure (not evidence)");
            panic!("live-smoke-t8430 failed: {e}");
        }
    }
}

/// `make live-smoke-t1950`: paper guard → token → `t8431` ELW-symbol list → `t1950`
/// ELW current-price/quote for the first live ELW `shcode`.
///
/// CHAINED, self-sourcing (R8): the `shcode` comes from a live `t8431` call, never
/// fabricated — ELW codes EXPIRE, so a hard-coded one would silently rot. ELW
/// `shcode`s are public market identifiers (may appear in `inputs`). The gate is
/// the single-instrument quote (`outblock.hname`) being populated — the quote
/// resolved. A success `rsp_cd` with an empty quote is the `00707`/off-data case →
/// PENDING (does NOT record). An empty/short `t8431` surfaces as a credential-safe
/// `SMOKE-FAIL` (spine-input-unavailable). The recorded line is credential-free
/// (`rsp_cd` + lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr
/// line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1950`"]
async fn live_smoke_t1950() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source a fresh ELW shcode from a live t8431 list (codes expire).
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1950 t8431 spine source empty (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1950: need an ELW shcode to quote");
    }
    let shcode = elws.outblock[0].shcode.clone();

    match sdk
        .market_session()
        .elw_quote(&T1950Request::for_shcode(&shcode))
        .await
    {
        Ok(resp) if resp.outblock.hname.is_empty() => {
            // 00000 with an empty quote (off-data) → PENDING, not Implemented.
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1950 empty quote payload (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1950: quote block empty (00707/off-data — PENDING)");
        }
        Ok(resp) => {
            // shcode is a public ELW identifier — OK to record.
            record(
                "live-smoke-t1950",
                &format!("env=paper shcode={shcode}"),
                &format!(
                    "rsp_cd={} hname_len={} price_len={} basket_rows={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.price.len(),
                    resp.outblock1.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1950 elw failure (not evidence)");
            panic!("live-smoke-t1950 failed: {e}");
        }
    }
}

/// `make live-smoke-t1954`: paper guard → token → `t8431` ELW-symbol list → `t1954`
/// ELW daily-price series for the first live ELW `shcode`.
///
/// CHAINED, self-sourcing (R8): the `shcode` comes from a live `t8431` call, never
/// fabricated — ELW codes EXPIRE, so a hard-coded one would silently rot. ELW
/// `shcode`s are public market identifiers (may appear in `inputs`). The gate is a
/// non-empty `t1954OutBlock1` daily row carrying a real `close` (a NAMED market-data
/// witness, not a status field). A success `rsp_cd` with no daily rows is the
/// `00707`/off-data case → PENDING (does NOT record). The recorded line is
/// credential-free (`rsp_cd` + row count, never `rsp_msg`); a failed run emits a
/// `SMOKE-FAIL` stderr line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1954`"]
async fn live_smoke_t1954() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source a fresh ELW shcode from a live t8431 list (codes expire).
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1954 t8431 spine source empty (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1954: need an ELW shcode for the daily series");
    }
    let shcode = elws.outblock[0].shcode.clone();

    match sdk
        .market_session()
        .elw_daily(&T1954Request::for_shcode(&shcode))
        .await
    {
        Ok(resp) => {
            // NAMED market-data witness (close), not a status/count field.
            let witnessed = resp.outblock1.first().is_some_and(|r| !r.close.is_empty());
            if !witnessed {
                // 00000 with no daily row carrying a close (off-data) → PENDING.
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t1954 empty daily series (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t1954: empty daily series (00707/off-data — PENDING)");
            }
            // shcode is a public ELW identifier — OK to record.
            record(
                "live-smoke-t1954",
                &format!("env=paper shcode={shcode}"),
                &format!(
                    "rsp_cd={} rows={} close_len={}",
                    resp.rsp_cd,
                    resp.outblock1.len(),
                    resp.outblock1[0].close.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1954 elw failure (not evidence)");
            panic!("live-smoke-t1954 failed: {e}");
        }
    }
}

/// `make live-smoke-t1971`: paper guard → token → `t8431` ELW-symbol list → `t1971`
/// ELW current-price + quote board for the first live ELW `shcode`.
///
/// CHAINED, self-sourcing (R8): the `shcode` comes from a live `t8431` call, never
/// fabricated — ELW codes EXPIRE, so a hard-coded one would silently rot. ELW
/// `shcode`s are public market identifiers (may appear in `inputs`). The gate is
/// the single-instrument quote (`outblock.hname`) being populated — the quote-board
/// resolved. A success `rsp_cd` with an empty quote is the `00707`/off-data case →
/// PENDING (does NOT record). An empty/short `t8431` surfaces as a credential-safe
/// `SMOKE-FAIL` (spine-input-unavailable). The recorded line is credential-free
/// (`rsp_cd` + lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr
/// line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1971`"]
async fn live_smoke_t1971() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source a fresh ELW shcode from a live t8431 list (codes expire).
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1971 t8431 spine source empty (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1971: need an ELW shcode to quote");
    }
    let shcode = elws.outblock[0].shcode.clone();

    match sdk
        .market_session()
        .elw_quote_board(&T1971Request::for_shcode(&shcode))
        .await
    {
        Ok(resp) if resp.outblock.hname.is_empty() => {
            // 00000 with an empty quote (off-data) → PENDING, not Implemented.
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1971 empty quote payload (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1971: quote block empty (00707/off-data — PENDING)");
        }
        Ok(resp) => {
            // shcode is a public ELW identifier — OK to record.
            record(
                "live-smoke-t1971",
                &format!("env=paper shcode={shcode}"),
                &format!(
                    "rsp_cd={} hname_len={} price_len={} offerho1_len={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.price.len(),
                    resp.outblock.offerho1.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1971 elw failure (not evidence)");
            panic!("live-smoke-t1971 failed: {e}");
        }
    }
}

/// `make live-smoke-t1972`: paper guard → token → `t8431` ELW-symbol list → `t1972`
/// ELW current-price + trading-member (거래원) board for the first live ELW `shcode`.
///
/// CHAINED, self-sourcing (R8): the `shcode` comes from a live `t8431` call, never
/// fabricated — ELW codes EXPIRE, so a hard-coded one would silently rot. ELW
/// `shcode`s are public market identifiers (may appear in `inputs`). The gate is
/// the member-board (`outblock.hname`) being populated — the board resolved. A
/// success `rsp_cd` with an empty board is the `00707`/off-data case → PENDING
/// (does NOT record). An empty/short `t8431` surfaces as a credential-safe
/// `SMOKE-FAIL` (spine-input-unavailable). The recorded line is credential-free
/// (`rsp_cd` + lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr
/// line, never a `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1972`"]
async fn live_smoke_t1972() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source a fresh ELW shcode from a live t8431 list (codes expire).
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1972 t8431 spine source empty (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1972: need an ELW shcode to quote");
    }
    let shcode = elws.outblock[0].shcode.clone();

    match sdk
        .market_session()
        .elw_member_board(&T1972Request::for_shcode(&shcode))
        .await
    {
        Ok(resp) if resp.outblock.hname.is_empty() => {
            // 00000 with an empty board (off-data) → PENDING, not Implemented.
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1972 empty board payload (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1972: board block empty (00707/off-data — PENDING)");
        }
        Ok(resp) => {
            // shcode is a public ELW identifier — OK to record.
            record(
                "live-smoke-t1972",
                &format!("env=paper shcode={shcode}"),
                &format!(
                    "rsp_cd={} hname_len={} offerno1_len={} dvol1_len={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.offerno1.len(),
                    resp.outblock.dvol1.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1972 elw failure (not evidence)");
            panic!("live-smoke-t1972 failed: {e}");
        }
    }
}

/// `make live-smoke-t1974`: paper guard → token → `t8431` ELW-symbol list → `t1974`
/// ELWs-sharing-a-base-asset (ELW기초자산동일종목) for the first live ELW `shcode`.
///
/// CHAINED, self-sourcing (R8): the `shcode` comes from a live `t8431` call, never
/// fabricated — ELW codes EXPIRE, so a hard-coded one would silently rot. ELW
/// `shcode`s are public market identifiers (may appear in `inputs`). The gate is the
/// sibling-issue array (`outblock1[0].hname`) being populated — the same-base set
/// resolved (a board/name witness, not a session-only field). A success `rsp_cd` with
/// an empty array is the `00707`/off-data case → PENDING (does NOT record). An
/// empty/short `t8431` surfaces as a credential-safe `SMOKE-FAIL`
/// (spine-input-unavailable). The recorded line is credential-free (`rsp_cd` + counts
/// + lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr line, never a
/// `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1974`"]
async fn live_smoke_t1974() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source a fresh ELW shcode from a live t8431 list (codes expire).
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1974 t8431 spine source empty (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1974: need an ELW shcode to query");
    }
    let shcode = elws.outblock[0].shcode.clone();

    match sdk
        .market_session()
        .elw_same_base_issues(&T1974Request::for_shcode(&shcode))
        .await
    {
        Ok(resp) if resp.outblock1.is_empty() => {
            // 00000 with an empty sibling array (off-data) → PENDING, not Implemented.
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1974 empty sibling array (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1974: sibling array empty (00707/off-data — PENDING)");
        }
        Ok(resp) => {
            // shcode is a public ELW identifier — OK to record.
            record(
                "live-smoke-t1974",
                &format!("env=paper shcode={shcode}"),
                &format!(
                    "rsp_cd={} cnt_len={} rows={} hname_len={}",
                    resp.rsp_cd,
                    resp.outblock.cnt.len(),
                    resp.outblock1.len(),
                    resp.outblock1[0].hname.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1974 elw failure (not evidence)");
            panic!("live-smoke-t1974 failed: {e}");
        }
    }
}

/// `make live-smoke-t1956`: paper guard → token → `t8431` ELW-symbol list → `t1956`
/// ELW current-price / contracted-payout snapshot (ELW현재가(확정지급액)조회) for the
/// first live ELW `shcode`.
///
/// CHAINED, self-sourcing (R8): the `shcode` comes from a live `t8431` call, never
/// fabricated — ELW codes EXPIRE, so a hard-coded one would silently rot. ELW
/// `shcode`s are public market identifiers (may appear in `inputs`). The gate is the
/// snapshot's NAME witness (`outblock.hname`) being populated — the issue resolved (a
/// board/name witness, NOT a session-only orderbook field). A success `rsp_cd` with
/// an empty/blank `hname` is the `00707`/off-data case → PENDING (does NOT record).
/// An empty/short `t8431` surfaces as a credential-safe `SMOKE-FAIL`
/// (spine-input-unavailable). The recorded line is credential-free (`rsp_cd` +
/// lengths, never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr line, never a
/// `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1956`"]
async fn live_smoke_t1956() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source a fresh ELW shcode from a live t8431 list (codes expire).
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1956 t8431 spine source empty (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1956: need an ELW shcode to query");
    }
    let shcode = elws.outblock[0].shcode.clone();

    match sdk
        .market_session()
        .elw_current_price(&T1956Request::for_shcode(&shcode))
        .await
    {
        Ok(resp) if resp.outblock.hname.trim().is_empty() => {
            // 00000 with a blank name (off-data) → PENDING, not Implemented.
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1956 empty snapshot name (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1956: snapshot name blank (00707/off-data — PENDING)");
        }
        Ok(resp) => {
            // shcode is a public ELW identifier — OK to record.
            record(
                "live-smoke-t1956",
                &format!("env=paper shcode={shcode}"),
                &format!(
                    "rsp_cd={} hname_len={} price_len={} basket_rows={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.price.len(),
                    resp.outblock1.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1956 elw failure (not evidence)");
            panic!("live-smoke-t1956 failed: {e}");
        }
    }
}

/// `make live-smoke-t1969`: paper guard → token → one `t1969` ELW screener run with
/// the all-ELWs default screen ([`T1969Request::new`] — every chk* off, numeric
/// ranges 0/0, dates 000000..999999). A success `rsp_cd` with a non-empty screened
/// row (modeled `shcode`/`hname`) proves the typed summary+array read round-trips.
/// Reachable under closure; an empty `00707` does NOT record — it dispositions to
/// PENDING. The recorded line is credential-free (`rsp_cd` + counts + lengths,
/// never `rsp_msg`); a failed run emits a `SMOKE-FAIL` stderr line, never a
/// `LIVE-SMOKE` one.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1969`"]
async fn live_smoke_t1969() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    match sdk
        .market_session()
        .elw_screener(&T1969Request::new())
        .await
    {
        Ok(resp) => {
            // Non-empty guard: a screen can return 00000 with an empty array off-data →
            // that is PENDING, not Implemented. Assert a non-empty screened row with a
            // modeled non-default key before recording.
            let first = resp.outblock1.first();
            assert!(
                first.is_some_and(|r| !r.shcode.is_empty()),
                "live-smoke-t1969: empty board (00707/off-data) — PENDING, not Implemented"
            );
            let row = first.expect("non-empty guard above");
            record(
                "live-smoke-t1969",
                "env=paper screen=all-elws",
                &format!(
                    "rsp_cd={} cnt_len={} rows={} shcode_len={} hname_len={}",
                    resp.rsp_cd,
                    resp.outblock.cnt.len(),
                    resp.outblock1.len(),
                    row.shcode.len(),
                    row.hname.len()
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1969 elw failure (not evidence)");
            panic!("live-smoke-t1969 failed: {e}");
        }
    }
}

/// `make live-smoke-t9942`: paper guard → token → one `t9942` ELW master list
/// read (no caller input). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9942`"]
async fn live_smoke_t9942() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().elw_master(&T9942Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "elws")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9942", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9942 market-data failure (not evidence)");
            panic!("live-smoke-t9942 failed: {e}");
        }
    }
}

/// `make live-smoke-t1958`: paper guard → token → `t8431` ELW-symbol list →
/// `t1958` comparison of the first two ELW `shcode`s.
///
/// CHAINED, self-sourcing (R8): the two `shcode`s come from a live `t8431` call,
/// never fabricated. ELW `shcode`s are public market identifiers (may appear in
/// `inputs`). The gate is the symbol-1 detail block (`outblock.hname`) being
/// populated — the comparison ran. An empty/short `t8431` surfaces as a
/// credential-safe `SMOKE-FAIL` (spine-input-unavailable).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1958`"]
async fn live_smoke_t1958() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source two ELW shcodes from a live t8431 list.
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.len() < 2 {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1958 t8431 spine source <2 codes (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1958: need two ELW shcodes to compare");
    }
    let (shcode1, shcode2) = (elws.outblock[0].shcode.clone(), elws.outblock[1].shcode.clone());

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .elw_compare(&T1958Request::new(&shcode1, &shcode2))
        .await
    {
        Ok(resp) if resp.outblock.hname.is_empty() => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1958 empty comparison payload (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1958: comparison block empty (shape-unconfirmed)");
        }
        Ok(resp) => {
            // shcodes are public ELW identifiers — OK to record.
            record(
                "live-smoke-t1958",
                &format!("env=paper shcode1={shcode1} shcode2={shcode2} date={date}"),
                &format!("rsp_cd={} compared=2", resp.rsp_cd),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1958 market-data failure (not evidence)");
            panic!("live-smoke-t1958 failed: {e}");
        }
    }
}

/// `make live-smoke-t1964`: paper guard → token → `t9905` underlying list →
/// `t1964` ELW board for the first underlying (broad/default filters).
///
/// CHAINED, self-sourcing (R8): the `item` underlying code comes from a live
/// `t9905` call, never fabricated. The smoke walks the first several underlyings
/// until one returns a non-empty board (an underlying with no listed ELWs is not
/// a failure). An empty `t9905`, or no underlying yielding a board, surfaces as a
/// credential-safe `SMOKE-FAIL`.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1964`"]
async fn live_smoke_t1964() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let underlyings = sdk
        .market_session()
        .underlying_list(&T9905Request::new())
        .await
        .expect("t9905 underlying_list (item source) failed");
    if underlyings.outblock1.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1964 t9905 spine source empty (rsp_cd={})",
            underlyings.rsp_cd
        );
        panic!("live-smoke-t1964: no underlying to key the board");
    }

    let date = Utc::now().format("%Y-%m-%d");
    // Walk the first several underlyings until one has a non-empty board. Pace
    // the calls (t1964 is 2/sec) so the walk does not self-trigger IGW00201
    // throttling (transient, environmental — not a TR defect).
    for u in underlyings.outblock1.iter().take(10) {
        tokio::time::sleep(Duration::from_millis(700)).await;
        let item = u.shcode.clone();
        match sdk
            .market_session()
            .elw_board(&T1964Request::new(&item))
            .await
        {
            Ok(resp) if !resp.outblock1.is_empty() => {
                record(
                    "live-smoke-t1964",
                    &format!("env=paper item={item} date={date}"),
                    &format!("rsp_cd={} elws={}", resp.rsp_cd, resp.outblock1.len()),
                );
                return;
            }
            Ok(_) => continue, // this underlying has no listed ELWs; try the next
            Err(e) => {
                eprintln!("SMOKE-FAIL target=live-smoke-t1964 market-data failure (not evidence)");
                panic!("live-smoke-t1964 failed: {e}");
            }
        }
    }
    eprintln!("SMOKE-FAIL target=live-smoke-t1964 no underlying yielded a non-empty board");
    panic!("live-smoke-t1964: no non-empty board among the first underlyings (shape-unconfirmed)");
}

// ---------------------------------------------------------------------------
// t2522 — 주식선물기초자산조회 (stock-futures underlying-asset master; F/O).
// market_session, non-paginated, no caller input (a single `dummy` placeholder).
// Master read — non-empty regardless of the KRX session (venue facet stays
// provisional). The structural signal is the canonical field's length (a single
// out-block, not an array), kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t2522`: paper guard → OAuth token → one `t2522`
/// underlying-asset master read (no caller input). A success `rsp_cd` with a
/// populated `t2522OutBlock1` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2522`"]
async fn live_smoke_t2522() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T2522Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_underlying_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t2522: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(
                Ok((resp.rsp_cd.clone(), resp.outblock1.len())),
                "rows",
            )
            .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2522",
                &format!("env=paper date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2522 market-data failure (not evidence)");
            panic!("live-smoke-t2522 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8435 — 파생종목마스터조회API용 (derivatives master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector — the LS spec defines these
// as the MINI/weekly segments: `"MF"` 미니선물 / `"MO"` 미니옵션 /
// `"WK"` 코스피200위클리옵션 / `"SF"` 코스닥150선물 / `"QW"` 코스닥150위클리옵션.
// Master read — non-empty regardless of the KRX session (venue facet stays
// provisional). The out-block is a row array (KTD3), so the structural signal is
// the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8435`: paper guard → OAuth token → one `t8435` derivatives
/// master read for `gubun="MF"` (미니선물/mini futures). A success `rsp_cd` with a
/// populated `t8435OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8435`"]
async fn live_smoke_t8435() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8435Request::new("MF");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().derivatives_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8435: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8435",
                &format!("env=paper gubun=MF date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8435 market-data failure (not evidence)");
            panic!("live-smoke-t8435 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8467 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"` volatility / `"S"`
// sector / `"Q"` KOSDAQ150 / any other value → KOSPI200 index futures). Master
// read — non-empty regardless of the KRX session (venue facet stays
// provisional). The out-block is a row array (KTD3), so the structural signal is
// the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8467`: paper guard → OAuth token → one `t8467` index-futures
/// master read for `gubun="Q"` (KOSDAQ150-index futures). A success `rsp_cd` with
/// a populated `t8467OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8467`"]
async fn live_smoke_t8467() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8467Request::new("Q");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_futures_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8467: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8467",
                &format!("env=paper gubun=Q date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8467 market-data failure (not evidence)");
            panic!("live-smoke-t8467 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t9943 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"` volatility / `"S"`
// sector / any other value → KOSPI200 index futures). Master read — non-empty
// regardless of the KRX session (venue facet stays provisional). The out-block is
// a row array (KTD3, true wire key `t9943OutBlock` from the raw capture), so the
// structural signal is the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t9943`: paper guard → OAuth token → one `t9943` index-futures
/// master read for `gubun="V"` (volatility-index futures). A success `rsp_cd` with
/// a populated `t9943OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9943`"]
async fn live_smoke_t9943() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T9943Request::new("V");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_futures_master_codes(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t9943: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t9943",
                &format!("env=paper gubun=V date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9943 market-data failure (not evidence)");
            panic!("live-smoke-t9943 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t9944 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// out-block is a row array (KTD3, true wire key `t9944OutBlock` from the raw
// capture), so the structural signal is the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t9944`: paper guard → OAuth token → one `t9944` index-option
/// master read (no caller input). A success `rsp_cd` with a populated
/// `t9944OutBlock` row array proves the read is callable and round-trips. The
/// recorded line is credential-free (only `rsp_cd` + the row count, never
/// `rsp_msg`) and self-dated; a failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9944`"]
async fn live_smoke_t9944() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T9944Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_option_master_codes(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t9944: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9944", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9944 market-data failure (not evidence)");
            panic!("live-smoke-t9944 failed: {e}");
        }
    }
}

/// `make live-smoke-t1988`: paper guard → token → one ELW underlying-asset list
/// read (all markets, filters off). Routes through `market_session` (KTD3).
/// Numeric request fields `from_rate`/`to_rate` serialize as JSON numbers (KTD4),
/// the prior IGW40011 wire-type fix.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1988`"]
async fn live_smoke_t1988() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().elw_underlying_list(&T1988Request::new("0")).await {
        Ok(resp) => {
            if resp.outblock.ksp_cnt.is_empty() && resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t1988 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t1988: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "assets")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1988",
                &format!("env=paper mkt_gb=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1988 market-data failure (not evidence)");
            panic!("live-smoke-t1988 failed: {e}");
        }
    }
}

/// `make live-smoke-t8455`: paper guard → token → one KRX night-derivatives
/// master read (`gubun="NF"` 야간선물). `venue_session: krx_extended` (KTD7) — the
/// night session is ~18:00–05:00 KST, NOT the regular clock; an off-window empty
/// result is NOT a valid attempt (re-run in-window, do NOT flip, do NOT DROP). A
/// definitive `01900` is paper-incompatible regardless of window.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8455`"]
async fn live_smoke_t8455() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().night_derivatives_master(&T8455Request::new("NF")).await {
        Ok(resp) => {
            if resp.outblock.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8455 empty master array (rsp_cd={}) — night window closed? re-run ~18:00–05:00 KST",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8455: empty master array (00707) — off-window/PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8455",
                &format!("env=paper gubun=NF date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8455 market-data failure (not evidence)");
            panic!("live-smoke-t8455 failed: {e}");
        }
    }
}

/// `make live-smoke-t8460`: paper guard → token → one KRX night-derivatives
/// option-board read (`gubun="G"` 원지수, near contract month). `venue_session:
/// krx_extended` (KTD7) — off-window empty is a re-run, not a flip/DROP.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8460`"]
async fn live_smoke_t8460() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let now = Utc::now();
    let yyyymm = now.format("%Y%m").to_string();
    let date = now.format("%Y-%m-%d");
    match sdk.market_session().night_option_board(&T8460Request::new(&yyyymm, "G")).await {
        Ok(resp) => {
            let rows = resp.outblock1.len() + resp.outblock2.len();
            if rows == 0 && resp.outblock.gmprice.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8460 empty board (rsp_cd={}) — night window closed? re-run ~18:00–05:00 KST",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8460: empty board (00707) — off-window/PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), rows)), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8460",
                &format!("env=paper yyyymm={yyyymm} gubun=G date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8460 market-data failure (not evidence)");
            panic!("live-smoke-t8460 failed: {e}");
        }
    }
}

/// `make live-smoke-g3104`: paper guard → token → one overseas stock-info master
/// read (`82`/`TSLA`). Routes through `market_session` (KTD3). Empty `korname`
/// out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3104`"]
async fn live_smoke_g3104() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_stock_info(&G3104Request::new("R", "82TSLA", "82", "TSLA"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.korname.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3104 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3104: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "master")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3104",
                &format!("env=paper exchcd=82 symbol=TSLA date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3104 market-data failure (not evidence)");
            panic!("live-smoke-g3104 failed: {e}");
        }
    }
}

/// `make live-smoke-g3190`: paper guard → token → one overseas master-list read
/// (US, exchange `2`, 10 rows, first page). `readcnt` serializes as a JSON
/// number (KTD4). Empty row array is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3190`"]
async fn live_smoke_g3190() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_master(&G3190Request::new("R", "US", "2", "10", ""))
        .await
    {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3190 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3190: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3190",
                &format!("env=paper natcode=US exgubun=2 readcnt=10 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3190 market-data failure (not evidence)");
            panic!("live-smoke-g3190 failed: {e}");
        }
    }
}

/// `make live-smoke-o3101`: paper guard → token → one overseas-futures master
/// read (`gubun=""` = all). Domain `overseas_futures`, routes through
/// `market_session` (KTD3). Empty row array is the `00707` PENDING case, not
/// Implemented.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3101`"]
async fn live_smoke_o3101() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_futures_master(&O3101Request::new(""))
        .await
    {
        Ok(resp) => {
            if resp.outblock.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3101 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3101: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3101",
                &format!("env=paper gubun=all date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3101 market-data failure (not evidence)");
            panic!("live-smoke-o3101 failed: {e}");
        }
    }
}

/// `make live-smoke-o3121`: paper guard → token → one overseas-future-option
/// master read (`MktGb="O"` = option, all base products). Routes through
/// `market_session` (KTD3). Empty row array is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3121`"]
async fn live_smoke_o3121() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_option_master(&O3121Request::new("O", ""))
        .await
    {
        Ok(resp) => {
            if resp.outblock.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3121 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3121: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3121",
                &format!("env=paper mktgb=O date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3121 market-data failure (not evidence)");
            panic!("live-smoke-o3121 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Domestic stock master/reference breadth wave (plan -004). Seven reads flipped
// on a clean non-empty paper smoke. Each MUST assert the out-block non-empty
// BEFORE record(): a success rsp_cd with an empty block (00707) deserializes
// fine and would green-flip on empty data (the 00707 trap).
// ---------------------------------------------------------------------------

/// `make live-smoke-t9945`: paper guard → token → one `t9945` KOSPI stock-master
/// read. A non-empty master array proves the read is callable and round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9945`"]
async fn live_smoke_t9945() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T9945Request::new("1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t9945: empty master (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "tickers")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9945", &format!("env=paper gubun=1 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9945 market-data failure (not evidence)");
            panic!("live-smoke-t9945 failed: {e}");
        }
    }
}

/// `make live-smoke-t1532`: one `t1532` stock themes read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1532`"]
async fn live_smoke_t1532() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1532Request::new("078020");
    match sdk.market_session().stock_themes(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock.is_empty(), "live-smoke-t1532: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "themes")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1532", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1532 market-data failure (not evidence)");
            panic!("live-smoke-t1532 failed: {e}");
        }
    }
}

/// `make live-smoke-t1533`: one `t1533` special themes read (plan -004 batch C).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1533`"]
async fn live_smoke_t1533() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let req = T1533Request::new("1");
    match sdk.market_session().special_themes(&req).await {
        Ok(resp) => {
            assert!(!resp.outblock1.is_empty(), "live-smoke-t1533: empty (00707) — PENDING, not Implemented");
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "themes")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1533", "env=paper", &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1533 market-data failure (not evidence)");
            panic!("live-smoke-t1533 failed: {e}");
        }
    }
}
