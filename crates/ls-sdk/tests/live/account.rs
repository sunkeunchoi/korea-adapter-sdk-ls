use super::*;


// ---------------------------------------------------------------------------
// U5 — account smoke (CSPAQ12200, read-only)
// ---------------------------------------------------------------------------

/// `make live-smoke-account`: paper guard → read-only `CSPAQ12200` balance.
///
/// Covers AE7. The account number comes from config, never the caller. An
/// account-state failure is recorded under the account label, distinct from
/// market-data / transport failures (it may reflect paper-account provisioning).
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-account`"]
async fn live_smoke_account() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    match sdk.account().balance(&CSPAQ12200Request::new("1")).await {
        // Credential-free by construction: `rsp_msg` is dropped (it can carry
        // localized, account-identifying text); only the numeric `rsp_cd`
        // proves success and `reccnt` is a structural record count. Mirrors
        // `live_smoke_default`.
        Ok(resp) => record(
            "live-smoke-account",
            "balcretp=1",
            &format!("rsp_cd={} reccnt={}", resp.rsp_cd, resp.outblock1.reccnt),
        ),
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text and would otherwise
        // pattern-match the evidence-capture recipe. Use a distinct
        // non-`LIVE-SMOKE` prefix on stderr; the panic is unchanged.
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-account account-state failure (not transport)");
            panic!("live-smoke-account failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-cspaq12300`: paper guard → read-only `CSPAQ12300` BEP/balance.
///
/// The account number comes from config, never the caller — the four query-shape
/// enums (`balcretp,cmsnapptpcode,d2balbaseqrytp,uprctpcode`) are the only inputs.
/// A success records a credential-free line (only the numeric `rsp_cd` and a
/// structural `outblock2` row count; `rsp_msg` is dropped because it can carry
/// account-identifying text). A failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cspaq12300`"]
async fn live_smoke_cspaq12300() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CSPAQ12300Request::new("1", "0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().bep(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "balrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cspaq12300",
                &format!("env=paper balcretp=1 date={date}"),
                &line,
            );
        }
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text. Use a distinct
        // non-`LIVE-SMOKE` stderr prefix; the panic is unchanged.
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cspaq12300 account-state failure (not transport)"
            );
            panic!("live-smoke-cspaq12300 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-cspaq22200`: paper guard → read-only `CSPAQ22200`
/// orderable-amount / valuation inquiry.
///
/// The account number comes from config, never the caller — `balcretp` is the only
/// input. A success records a credential-free line (only the numeric `rsp_cd` and a
/// structural `outblock2` row count; `rsp_msg` is dropped because it can carry
/// account-identifying text). A failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cspaq22200`"]
async fn live_smoke_cspaq22200() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CSPAQ22200Request::new("1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().orderable(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "balrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cspaq22200",
                &format!("env=paper balcretp=1 date={date}"),
                &line,
            );
        }
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text. Use a distinct
        // non-`LIVE-SMOKE` stderr prefix; the panic is unchanged.
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cspaq22200 account-state failure (not transport)"
            );
            panic!("live-smoke-cspaq22200 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-cfobq10500`: paper guard → read-only `CFOBQ10500` F/O account
/// deposit / margin inquiry.
///
/// The account number comes from config, never the caller — this is a header-only
/// read with no caller input. A success records a credential-free line (only the
/// numeric `rsp_cd` and a structural `outblock2` row count; `rsp_msg` is dropped
/// because it can carry account-identifying text). A position-less paper account
/// may return an empty `00707` deposit (the PENDING case), which still records a
/// credential-free line with the row count = 0. A failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cfobq10500`"]
async fn live_smoke_cfobq10500() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CFOBQ10500Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().fo_deposit(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "deprows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cfobq10500",
                &format!("env=paper date={date}"),
                &line,
            );
        }
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text. Use a distinct
        // non-`LIVE-SMOKE` stderr prefix; the panic is unchanged.
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cfobq10500 account-state failure (not transport)"
            );
            panic!("live-smoke-cfobq10500 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-ccenq90200`: paper guard → read-only `CCENQ90200`
/// KRX night-derivatives account balance inquiry (krx_extended session).
///
/// The account number comes from config, never the caller — the only inputs are
/// the record count and two evaluation-shape enums. This is a night (krx_extended)
/// read: an empty `00707`/empty result OFF the night window is the PENDING case
/// (callable, shape unconfirmed), NOT a defect — the regular ~09:00–15:30 KST clock
/// does not apply. A success records a credential-free line (only the numeric
/// `rsp_cd` and a structural `outblock2` row count; `rsp_msg` is dropped because it
/// can carry account-identifying text). A failed run emits a distinct `SMOKE-FAIL`
/// stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-ccenq90200`"]
async fn live_smoke_ccenq90200() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CCENQ90200Request::new("1", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().night_balance(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "balrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-ccenq90200",
                &format!("env=paper balevaltp=0 futsprcevaltp=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-ccenq90200 account-state failure (not transport)"
            );
            panic!("live-smoke-ccenq90200 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-t0424`: paper guard → read-only `t0424` stock balance (the U2
/// holdings gate).
///
/// The account comes from config, never the caller — only the gubun flags are
/// inputs. The recorded line is credential-free: the numeric `rsp_cd`, the holdings
/// array LENGTH (the U2 gate — 0 means a cash-only account), and a boolean flag for
/// whether the cash witness (`sunamt`) is non-default. NO `rsp_msg`, NO account
/// number, NO balance value. The fail-closed dispatch-log suppressor is installed
/// before the first dispatch (KTD7). A failed run emits a distinct `SMOKE-FAIL`
/// stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-t0424`"]
async fn live_smoke_t0424() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = T0424Request::new("", "0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().stock_balance(&req).await {
        Ok(resp) => {
            // Credential-free: rsp_cd + holdings count + a non-default cash flag.
            let cash_nondefault = !resp.outblock.sunamt.is_empty() && resp.outblock.sunamt != "0";
            let line = format!(
                "rsp_cd={} holdings={} cash_nondefault={}",
                resp.rsp_cd,
                resp.outblock1.len(),
                cash_nondefault
            );
            record("live-smoke-t0424", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t0424 account-state failure (not transport)");
            // Do NOT interpolate the LsError: ApiError Displays its rsp_msg, which
            // carries account-identifying text the KTD7 suppressor drops from tracing
            // but cannot scrub from a panic payload. The SMOKE-FAIL line above already
            // classifies the failure credential-free.
            panic!("live-smoke-t0424 failed (account-state, may be paper-account setup) — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-cspbq00200`: paper guard → read-only `CSPBQ00200` order-capacity
/// inquiry. The instrument is `LS_LIVE_SMOKE_ISU` (a stable ISIN, default Samsung
/// `KR7005930003`); `OrdPrc="0"` requests broad capacity. The recorded line is
/// credential-free: `rsp_cd`, the capacity row count, and a flag for whether the
/// `SeOrdAbleAmt` capacity witness is non-default. The dispatch-log suppressor is
/// installed first (KTD7). A failed run emits a distinct `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cspbq00200`"]
async fn live_smoke_cspbq00200() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let isu = std::env::var("LS_LIVE_SMOKE_ISU").unwrap_or_else(|_| "KR7005930003".to_string());
    // A capacity inquiry needs a valid order price to compute orderable amounts; a
    // static plausible price is closure-compatible (margin capacity does not need a
    // LIVE quote, just a valid price). Default ≈ a recent Samsung price.
    let ordprc = std::env::var("LS_LIVE_SMOKE_ORDPRC").unwrap_or_else(|_| "75000".to_string());
    let req = CSPBQ00200Request::new("1", &isu, &ordprc, "41");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().order_capacity(&req).await {
        Ok(resp) => {
            let (dps_nd, prdps_nd, se_nd) = resp
                .outblock2
                .first()
                .map(|c| {
                    (
                        is_non_default_str(&c.dps),
                        is_non_default_str(&c.prsmptdpsd1),
                        is_non_default_str(&c.seordableamt),
                    )
                })
                .unwrap_or((false, false, false));
            let line = format!(
                "rsp_cd={} caprows={} dps_nd={dps_nd} prsmptdps_nd={prdps_nd} se_nd={se_nd}",
                resp.rsp_cd,
                resp.outblock2.len(),
            );
            record("live-smoke-cspbq00200", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cspbq00200 account-state failure (not transport)"
            );
            // No `{e}`: a leaked ApiError rsp_msg would re-introduce account text (KTD7).
            panic!("live-smoke-cspbq00200 failed (account-state, may be paper-account setup) — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-clnaq00100`: paper guard → read-only `CLNAQ00100` loanable-stock
/// list (full-list mode). Persistent reference data — the loanable universe is
/// populated regardless of market hours. The recorded line is credential-free:
/// `rsp_cd`, the stock-list LENGTH, and a flag for whether the first entry carries a
/// non-empty issue name. The dispatch-log suppressor is installed first (KTD7). A
/// failed run emits a distinct `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs LS paper credentials; run via `make live-smoke-clnaq00100`"]
async fn live_smoke_clnaq00100() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().loanable_stocks(&CLNAQ00100Request::full_list()).await {
        Ok(resp) => {
            let name_nondefault = resp
                .outblock2
                .first()
                .map(|s| !s.isunm.is_empty())
                .unwrap_or(false);
            let line = format!(
                "rsp_cd={} stocks={} name_nondefault={}",
                resp.rsp_cd,
                resp.outblock2.len(),
                name_nondefault
            );
            record("live-smoke-clnaq00100", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!("SMOKE-FAIL target=live-smoke-clnaq00100 reference-read failure (not transport)");
            // No `{e}`: a leaked ApiError rsp_msg would re-introduce account text (KTD7).
            panic!("live-smoke-clnaq00100 failed — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-cfoeq11100`: paper guard → read-only `CFOEQ11100` F/O
/// provisional-settlement deposit detail. `BnsDt` defaults to today (the deposit is
/// account state, not date-gated). The recorded line is credential-free: `rsp_cd`,
/// the deposit row count, and a flag for whether the `Dps` deposit witness is
/// non-default. The dispatch-log suppressor is installed first (KTD7). A failed run
/// emits a distinct `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cfoeq11100`"]
async fn live_smoke_cfoeq11100() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let date = Utc::now().format("%Y-%m-%d");
    let bnsdt = std::env::var("LS_LIVE_SMOKE_BNSDT")
        .unwrap_or_else(|_| Utc::now().format("%Y%m%d").to_string());
    match sdk.account().fo_deposit_detail(&CFOEQ11100Request::new(&bnsdt)).await {
        Ok(resp) => {
            let (dps_nd, opnmk_nd, csgn_nd) = resp
                .outblock2
                .first()
                .map(|d| {
                    (
                        is_non_default_str(&d.dps),
                        is_non_default_str(&d.opnmkdpsamttotamt),
                        is_non_default_str(&d.csgnmgn),
                    )
                })
                .unwrap_or((false, false, false));
            let line = format!(
                "rsp_cd={} deprows={} dps_nd={dps_nd} opnmk_nd={opnmk_nd} csgn_nd={csgn_nd}",
                resp.rsp_cd,
                resp.outblock2.len(),
            );
            record("live-smoke-cfoeq11100", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cfoeq11100 account-state failure (not transport)"
            );
            // No `{e}`: a leaked ApiError rsp_msg would re-introduce account text (KTD7).
            panic!("live-smoke-cfoeq11100 failed (account-state, may be paper-account setup) — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-t0441`: paper guard → read-only `t0441` F/O balance valuation.
///
/// On a position-less paper account the position array is empty and the valuation
/// summary is zero (the AE2 expected-empty case under the U2 holdings gate). The
/// recorded line is credential-free: `rsp_cd`, the position count, and a flag for
/// whether the `tappamt` summary witness is non-default. The dispatch-log suppressor
/// is installed first (KTD7). A failed run emits a distinct `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-t0441`"]
async fn live_smoke_t0441() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().fo_balance_eval(&T0441Request::new()).await {
        Ok(resp) => {
            let tappamt_nondefault =
                !resp.outblock.tappamt.is_empty() && resp.outblock.tappamt != "0";
            let line = format!(
                "rsp_cd={} positions={} tappamt_nondefault={}",
                resp.rsp_cd,
                resp.outblock1.len(),
                tappamt_nondefault
            );
            record("live-smoke-t0441", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t0441 account-state failure (not transport)");
            // No `{e}`: a leaked ApiError rsp_msg would re-introduce account text (KTD7).
            panic!("live-smoke-t0441 failed (account-state, may be paper-account setup) — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-cidbq01400`: paper guard → read-only `CIDBQ01400` overseas-
/// futures orderable-quantity inquiry. The contract is `LS_LIVE_SMOKE_OVRSISU`
/// (default the spec example `ADM23`); overseas paper feeds are historically empty,
/// so an empty/zero quantity is the expected PENDING case. The recorded line is
/// credential-free: `rsp_cd`, the row count, and a flag for whether `OrdAbleQty` is
/// non-default. The dispatch-log suppressor is installed first (KTD7). A failed run
/// emits a distinct `SMOKE-FAIL` stderr line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account + overseas eligibility; run via `make live-smoke-cidbq01400`"]
async fn live_smoke_cidbq01400() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let isu = std::env::var("LS_LIVE_SMOKE_OVRSISU").unwrap_or_else(|_| "ADM23".to_string());
    let req = CIDBQ01400Request::new("1", &isu, "2", "1", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().overseas_fo_order_qty(&req).await {
        Ok(resp) => {
            let qty_nondefault = resp
                .outblock2
                .first()
                .map(|q| !q.ordableqty.is_empty() && q.ordableqty != "0")
                .unwrap_or(false);
            let line = format!(
                "rsp_cd={} rows={} qty_nondefault={}",
                resp.rsp_cd,
                resp.outblock2.len(),
                qty_nondefault
            );
            record("live-smoke-cidbq01400", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cidbq01400 account-state failure (not transport)"
            );
            // No `{e}`: a leaked ApiError rsp_msg would re-introduce account text (KTD7).
            panic!("live-smoke-cidbq01400 failed (account-state, may be paper-account setup) — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-cidbq03000`: paper guard → read-only `CIDBQ03000` overseas-
/// futures deposit/balance status. Runs under the `overseas_option` lane (account
/// `…71`); on the wrong account it returns empty/all-default. The witness is a
/// non-default `EvalAssetAmt` (평가자산금액); an empty or all-default result is the
/// PENDING case and emits a `SMOKE-FAIL` (never a capturable evidence line). The
/// recorded line is credential-free: `rsp_cd`, the row count, and the witness flag.
///
/// `TrdDt` selects the settlement snapshot and MUST be a trading day — a weekend or
/// holiday returns `01715` (non-trading day). The smoke walks back to the most recent
/// weekday (KST); override with `LS_LIVE_SMOKE_CIDBQ03000_TRDDT=YYYYMMDD` (e.g. on a
/// holiday). A `01715` surfaces via the Err path as a SMOKE-FAIL (re-run on a trading
/// day), not a flip.
#[tokio::test]
#[ignore = "live smoke: needs the overseas-futures account lane; run via `make live-smoke-cidbq03000`"]
async fn live_smoke_cidbq03000() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let date = Utc::now().format("%Y-%m-%d");
    let trddt = std::env::var("LS_LIVE_SMOKE_CIDBQ03000_TRDDT").unwrap_or_else(|_| {
        let kst = FixedOffset::east_opt(9 * 3600).expect("KST offset is valid");
        let mut d = Utc::now().with_timezone(&kst).date_naive();
        while matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            d = d.pred_opt().expect("previous day exists");
        }
        d.format("%Y%m%d").to_string()
    });
    match sdk.account().overseas_fo_balance(&CIDBQ03000Request::new("1", &trddt)).await {
        Ok(resp) => {
            let asset_nd = resp
                .outblock2
                .iter()
                .any(|r| is_non_default_str(&r.evalassetamt));
            if resp.outblock2.is_empty() || !asset_nd {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-cidbq03000 empty/all-default (rsp_cd={}); PENDING not evidence",
                    resp.rsp_cd
                );
                panic!("live-smoke-cidbq03000: empty/all-default balance — PENDING, not Implemented");
            }
            let line = format!(
                "rsp_cd={} rows={} asset_nd={asset_nd}",
                resp.rsp_cd,
                resp.outblock2.len(),
            );
            record("live-smoke-cidbq03000", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cidbq03000 account-state failure (not transport)"
            );
            panic!("live-smoke-cidbq03000 failed (account-state, may be paper-account setup) — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-cidbq05300`: paper guard → read-only `CIDBQ05300` overseas-
/// futures deposited-assets inquiry. Runs under the `overseas_option` lane (account
/// `…71`); the cash account returned `IGW40013` here (a wrong-account artifact). The
/// witness is a non-default `OvrsFutsDps` (해외선물예수금) on any currency row; an empty
/// or all-default result is the PENDING case and emits a `SMOKE-FAIL`. The recorded
/// line is credential-free: `rsp_cd`, the row count, and the witness flag.
#[tokio::test]
#[ignore = "live smoke: needs the overseas-futures account lane; run via `make live-smoke-cidbq05300`"]
async fn live_smoke_cidbq05300() {
    install_dispatch_log_suppressor().expect("dispatch-log suppressor must install (KTD7)");
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .account()
        .overseas_fo_deposited_assets(&CIDBQ05300Request::new("1", "ALL"))
        .await
    {
        Ok(resp) => {
            let dps_nd = resp
                .outblock2
                .iter()
                .any(|r| is_non_default_str(&r.ovrsfutsdps));
            if resp.outblock2.is_empty() || !dps_nd {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-cidbq05300 empty/all-default (rsp_cd={}); PENDING not evidence",
                    resp.rsp_cd
                );
                panic!("live-smoke-cidbq05300: empty/all-default deposited assets — PENDING, not Implemented");
            }
            let line = format!(
                "rsp_cd={} rows={} dps_nd={dps_nd}",
                resp.rsp_cd,
                resp.outblock2.len(),
            );
            record("live-smoke-cidbq05300", &format!("env=paper date={date}"), &line);
        }
        Err(_) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cidbq05300 account-state failure (not transport)"
            );
            panic!("live-smoke-cidbq05300 failed (account-state, may be paper-account setup) — see the SMOKE-FAIL line above");
        }
    }
}

/// `make live-smoke-cfoaq10100`: paper guard → read-only `CFOAQ10100` F/O
/// orderable-quantity INQUIRY (조회, NOT an order).
///
/// The account number comes from config, never the caller. The F/O instrument is
/// taken from `LS_LIVE_SMOKE_FNOISU` (a current KOSPI200-futures code, e.g.
/// `101V6000`) so the smoke targets a live contract; the rest are conservative
/// query-shape inputs. A position-less paper account may return an empty `00707`
/// (the PENDING case). A success records a credential-free line (only the numeric
/// `rsp_cd` and a structural `outblock2` row count; `rsp_msg` is dropped). A failed
/// run emits a distinct `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE`
/// line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account + current FnoIsuNo; run via `make live-smoke-cfoaq10100`"]
async fn live_smoke_cfoaq10100() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let fnoisu = std::env::var("LS_LIVE_SMOKE_FNOISU").unwrap_or_else(|_| "101V6000".to_string());
    let req = CFOAQ10100Request::new("1", "1", "0", "0", &fnoisu, "1", "0", "00");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().fo_orderable_qty(&req).await {
        Ok(resp) if resp.outblock2.is_empty() => {
            // Empty success (`00707`) on a position-less paper account is the PENDING
            // case, not Implemented evidence — emit no capturable LIVE-SMOKE line
            // (mirrors live_smoke_t1866's non-empty guard).
            eprintln!("SMOKE-FAIL target=live-smoke-cfoaq10100 empty result (00707); PENDING not evidence");
            panic!("live-smoke-cfoaq10100: empty result (00707) — PENDING, not Implemented");
        }
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "qtyrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cfoaq10100",
                &format!("env=paper fnoisu={fnoisu} qrytp=1 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cfoaq10100 account-state failure (not transport)"
            );
            panic!("live-smoke-cfoaq10100 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-ccenq10100`: paper guard → read-only `CCENQ10100` KRX
/// night-derivatives orderable-quantity INQUIRY (조회, NOT an order; krx_extended).
///
/// The account number comes from config, never the caller. The F/O instrument is
/// taken from `LS_LIVE_SMOKE_FNOISU`. This is a night (krx_extended) read: an empty
/// `00707`/empty result off the night window is the PENDING case, NOT a defect —
/// the regular clock does not apply. A success records a credential-free line (only
/// the numeric `rsp_cd` and a structural `outblock2` row count; `rsp_msg` dropped).
/// A failed run emits a distinct `SMOKE-FAIL` stderr line, never a capturable
/// `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account + current FnoIsuNo; run via `make live-smoke-ccenq10100`"]
async fn live_smoke_ccenq10100() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let fnoisu = std::env::var("LS_LIVE_SMOKE_FNOISU").unwrap_or_else(|_| "101V6000".to_string());
    let req = CCENQ10100Request::new("1", "1", "0", "0", &fnoisu, "1", "0", "00");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().night_orderable_qty(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "qtyrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-ccenq10100",
                &format!("env=paper fnoisu={fnoisu} qrytp=1 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-ccenq10100 account-state failure (not transport)"
            );
            panic!("live-smoke-ccenq10100 failed (account-state, may be paper-account setup): {e}");
        }
    }
}
