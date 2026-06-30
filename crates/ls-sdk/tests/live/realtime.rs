use super::*;


/// `make live-smoke-ws`: generic lifecycle smoke for `S3_` (market-data, "3").
/// Covers AE6. Delegates to [`ws_lifecycle_smoke`] so the single-TR smoke and the
/// per-TR U5/U6 smokes share one code path.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-ws`"]
async fn live_smoke_ws() {
    let symbol = resolve_symbol();
    let row_note = ws_lifecycle_smoke("S3_", &symbol, WsLane::MarketData).await;
    record(
        "live-smoke-ws",
        &format!("symbol={symbol} ws_port=29443 tr_type=3"),
        &row_note,
    );
}

/// `make live-smoke-k3`: lifecycle smoke for `K3_` (KOSDAQ 체결, market-data).
///
/// The flip gate for K3_. Set `LS_LIVE_SMOKE_SHCODE` to a KOSDAQ code for a
/// venue-representative run (the migration source's cert used `005930`). Per the
/// KTD6 result (`NOT-OBSERVABLE`), a clean lifecycle here proves **connection
/// reachability only**, not per-TR reachability — flip the metadata with that
/// weaker claim.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-k3`"]
async fn live_smoke_k3() {
    let symbol = resolve_symbol();
    let row_note = ws_lifecycle_smoke("K3_", &symbol, WsLane::MarketData).await;
    record(
        "live-smoke-k3",
        &format!("symbol={symbol} ws_port=29443 tr_type=3"),
        &row_note,
    );
}

/// `make live-smoke-ws-p1`: COMBINED lifecycle smoke for the 14 P1 market-data
/// realtime TRs (the operator runs ONE command to gate the whole wave).
///
/// Iterates all 14 `(tr_cd, tr_key, WsLane::MarketData)` tuples, each on a FRESH
/// manager via [`ws_lifecycle_try`]. RESILIENT: a per-TR subscribe/lifecycle
/// failure is CAUGHT and recorded as that TR's `record(...)` line, so one bad TR
/// cannot hide the other 13. After the sweep, panics only if ANY TR failed (so
/// the make target reports red) — but every TR's line is emitted first.
///
/// Default `tr_key`s are public symbols (the migration-source cert keys — stock
/// `005930`, overseas-stock `TSLA`, overseas-futures `CLZ25`, F-O `101TC000`),
/// safe to hardcode. Override the stock key via `LS_LIVE_SMOKE_SHCODE`. Per KTD6
/// (`NOT-OBSERVABLE`), a clean lifecycle here proves **connection reachability
/// only**, not per-TR reachability — flip each TR's metadata with that weaker
/// claim. NO raw-frame logging.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-ws-p1`"]
async fn live_smoke_ws_p1() {
    let stock_key = resolve_symbol();
    // (tr_cd, tr_key) — public cert symbols; stock key overridable via env.
    let trs: [(&str, &str); 14] = [
        ("H1_", stock_key.as_str()),
        ("HA_", stock_key.as_str()),
        ("S2_", stock_key.as_str()),
        ("US3", stock_key.as_str()),
        ("UH1", stock_key.as_str()),
        ("US2", stock_key.as_str()),
        ("GSC", "TSLA"),
        ("GSH", "TSLA"),
        ("OVC", "CLZ25"),
        ("OVH", "CLZ25"),
        ("OC0", "101TC000"),
        ("OH0", "101TC000"),
        ("FC9", "101TC000"),
        ("FH9", "101TC000"),
    ];

    let mut failures = 0usize;
    for (tr_cd, tr_key) in trs {
        // Each TR on a fresh manager; a failure is recorded, never propagated.
        let result = ws_lifecycle_try(tr_cd, tr_key, WsLane::MarketData).await;
        let row_note = match result {
            Ok(note) => note,
            Err(err) => {
                failures += 1;
                format!("LIFECYCLE-FAIL: {err}")
            }
        };
        // target= names the REAL runnable make target (the combined sweep); the
        // per-TR identity is carried by tr_cd= in inputs. Avoids emitting a
        // `live-smoke-<tr>` label that maps to no Makefile target.
        record(
            "live-smoke-ws-p1",
            &format!("tr_cd={tr_cd} tr_key={tr_key} ws_port=29443 tr_type=3"),
            &row_note,
        );
    }

    assert_eq!(
        failures, 0,
        "{failures} of 14 P1 market-data TRs failed their lifecycle (see per-TR lines above)"
    );
}

/// `make live-smoke-ws-p2`: COMBINED lifecycle smoke for the 16 P2 ORDER-EVENT
/// realtime TRs (the operator runs ONE command to gate the whole wave).
///
/// Iterates all 16 `(tr_cd, tr_key, WsLane::OrderEvent)` tuples, each on a FRESH
/// manager via [`ws_lifecycle_try`]. RESILIENT: a per-TR subscribe/lifecycle
/// failure is CAUGHT and recorded as that TR's `record(...)` line, so one bad TR
/// cannot hide the other 15. After the sweep, panics only if ANY TR failed (so
/// the make target reports red) — but every TR's line is emitted first.
///
/// **OBSERVATION-ONLY:** order-event channels are 주문/체결 EVENT feeds. This smoke
/// ONLY subscribes and unsubscribes — it NEVER places, amends, or cancels an
/// order. `WsLane::OrderEvent` registers with `tr_type "1"` (계좌등록) and
/// deregisters with `"2"`.
///
/// The default `tr_key`s are the migration-source certification keys: the stock
/// `SC*` feeds are account-bound so they pass an EMPTY string `""` (the gateway
/// scopes by the registered account), while F-O `C01/O01/H01` pass `101TC000`,
/// overseas-stock `AS*` pass `TSLA`, and overseas-futures `TC*` pass `CLZ25`.
/// This inconsistency is inherited from the cert fixtures; the smoke records
/// per-TR which keys actually open a lifecycle. Per KTD6 (`NOT-OBSERVABLE`) and
/// the UNESTABLISHED paper reachability for these feeds, a clean lifecycle here
/// proves **connection reachability only** — flip each TR's metadata with that
/// weaker claim, and a meaningful share may stay Tracked-only. NO raw-frame
/// logging.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-ws-p2`"]
async fn live_smoke_ws_p2() {
    // (tr_cd, tr_key) — migration-source cert keys; SC* are account-bound (empty).
    let trs: [(&str, &str); 16] = [
        ("SC0", ""),
        ("SC1", ""),
        ("SC2", ""),
        ("SC3", ""),
        ("SC4", ""),
        ("C01", "101TC000"),
        ("O01", "101TC000"),
        ("H01", "101TC000"),
        ("AS0", "TSLA"),
        ("AS1", "TSLA"),
        ("AS2", "TSLA"),
        ("AS3", "TSLA"),
        ("AS4", "TSLA"),
        ("TC1", "CLZ25"),
        ("TC2", "CLZ25"),
        ("TC3", "CLZ25"),
    ];

    let mut failures = 0usize;
    for (tr_cd, tr_key) in trs {
        // Each TR on a fresh manager; a failure is recorded, never propagated.
        // OBSERVATION-ONLY: subscribe + unsubscribe, never an order action.
        let result = ws_lifecycle_try(tr_cd, tr_key, WsLane::OrderEvent).await;
        let row_note = match result {
            Ok(note) => note,
            Err(err) => {
                failures += 1;
                format!("LIFECYCLE-FAIL: {err}")
            }
        };
        // target= names the REAL runnable make target; per-TR identity via tr_cd=.
        record(
            "live-smoke-ws-p2",
            &format!("tr_cd={tr_cd} tr_key={tr_key} ws_port=29443 tr_type=1"),
            &row_note,
        );
    }

    assert_eq!(
        failures, 0,
        "{failures} of 16 P2 order-event TRs failed their lifecycle (see per-TR lines above)"
    );
}

/// `make live-smoke-ws-p3`: COMBINED lifecycle smoke for the 31 NEW market-data
/// realtime TRs of the closure-flip WS batch (plan -004). The operator runs ONE
/// command to gate the whole Lane-1 flip.
///
/// Iterates all 31 `(tr_cd, tr_key, WsLane::MarketData)` tuples, each on a FRESH
/// manager via [`ws_lifecycle_try`]. RESILIENT: a per-TR subscribe/lifecycle
/// failure is CAUGHT and recorded as that TR's `record(...)` line, so one bad TR
/// cannot hide the other 30. After the sweep, panics only if ANY TR failed (so
/// the make target reports red) — but every TR's line is emitted first.
///
/// `tr_key`s are public, spec-grounded values taken from each channel's raw
/// `res_example` header (NXT/KRX stock codes overridable via `LS_LIVE_SMOKE_SHCODE`;
/// index/sector feeds key by upcode `001`; F-O by fut/opt code; overseas-futures by
/// symbol). Per KTD6 (`NOT-OBSERVABLE`), a clean lifecycle here proves **connection
/// reachability only**, not per-TR reachability — flip each TR's metadata with that
/// weaker claim. NO raw-frame logging.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-ws-p3`"]
async fn live_smoke_ws_p3() {
    let stock_key = resolve_symbol();
    // (tr_cd, tr_key) — public spec-grounded keys; stock key overridable via env.
    let trs: [(&str, &str); 31] = [
        ("NS3", stock_key.as_str()),
        ("NH1", stock_key.as_str()),
        ("NS2", stock_key.as_str()),
        ("NK1", stock_key.as_str()),
        ("NBT", "001"),
        ("KS_", stock_key.as_str()),
        ("OK_", stock_key.as_str()),
        ("KH_", stock_key.as_str()),
        ("KM_", "1"),
        ("PH_", stock_key.as_str()),
        ("K1_", stock_key.as_str()),
        ("IJ_", "001"),
        ("YS3", stock_key.as_str()),
        ("YK3", stock_key.as_str()),
        ("VI_", stock_key.as_str()),
        ("JC0", "111T7000"),
        ("JH0", "111T7000"),
        ("JD0", "111T7000"),
        ("FD0", "101T9000"),
        ("OD0", "201T7347"),
        ("OMG", "201T7347"),
        ("YF9", "A0166000"),
        ("YOC", "201T7345"),
        ("BM_", "001"),
        ("WOC", "2ESU23_4400"),
        ("WOH", "2ESU23_4400"),
        ("JIF", "0"),
        ("NWS", "NWS001"),
        ("BMT", "001"),
        ("CUR", "USD"),
        ("MK2", "N"),
    ];

    let mut failures = 0usize;
    for (tr_cd, tr_key) in trs {
        // Each TR on a fresh manager; a failure is recorded, never propagated.
        let result = ws_lifecycle_try(tr_cd, tr_key, WsLane::MarketData).await;
        let row_note = match result {
            Ok(note) => note,
            Err(err) => {
                failures += 1;
                format!("LIFECYCLE-FAIL: {err}")
            }
        };
        // target= names the REAL runnable make target (the combined sweep); the
        // per-TR identity is carried by tr_cd= in inputs.
        record(
            "live-smoke-ws-p3",
            &format!("tr_cd={tr_cd} tr_key={tr_key} ws_port=29443 tr_type=3"),
            &row_note,
        );
    }

    assert_eq!(
        failures, 0,
        "{failures} of 31 P3 market-data TRs failed their lifecycle (see per-TR lines above)"
    );
}

/// `make live-smoke-ws-p4`: COMBINED lifecycle smoke for the 39 NEW market-data
/// realtime TRs of the open-window WS track/flip wave (plan 2026-06-29-001). The
/// operator runs ONE command to gate the whole flip.
///
/// Iterates all 39 `(tr_cd, tr_key, WsLane::MarketData)` tuples, each on a FRESH
/// manager via [`ws_lifecycle_try`]. RESILIENT: a per-TR subscribe/lifecycle
/// failure is CAUGHT and recorded as that TR's `record(...)` line, so one bad TR
/// cannot hide the other 38. After the sweep, panics only if ANY TR failed (so
/// the make target reports red) — but every TR's line is emitted first.
///
/// `tr_key`s are public, spec-grounded values taken from each channel's raw
/// `req_example` body (stock `shcode`s overridable via `LS_LIVE_SMOKE_SHCODE`;
/// index/sector feeds key by upcode; F-O by fut/opt code; overseas-futures by
/// symbol). AFR carries only a placeholder key in the raw capture, so it uses the
/// representative stock symbol. Per KTD6 (`NOT-OBSERVABLE`), a clean lifecycle
/// here proves **connection reachability only**, not per-TR reachability — flip
/// each TR's metadata with that weaker claim. NO raw-frame logging.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-ws-p4`"]
async fn live_smoke_ws_p4() {
    let stock_key = resolve_symbol();
    // (tr_cd, tr_key) — public spec-grounded keys; stock key overridable via env.
    let trs: [(&str, &str); 39] = [
        ("AFR", stock_key.as_str()),
        ("B7_", "069500"),
        ("C02", "101W6000"),
        ("CD0", "165T6000"),
        ("DBM", "UFK2I"),
        ("DBT", "UFK2I"),
        ("DC0", "101W6000"),
        ("DD0", "101W6000"),
        ("DH0", "101W6000"),
        ("DH1", stock_key.as_str()),
        ("DHA", "086520"),
        ("DK3", "086520"),
        ("DS3", stock_key.as_str()),
        ("DVI", "086520"),
        ("ESN", "52HAAA"),
        ("FX9", "A0166000"),
        ("H02", "101W6000"),
        ("H2_", stock_key.as_str()),
        ("HB_", "086520"),
        ("I5_", "069500"),
        ("JX0", "111T7000"),
        ("NBM", "N003"),
        ("NPM", "N0"),
        ("NVI", "0000000000"),
        ("O02", "101W6000"),
        ("OX0", "201T7395"),
        ("SHC", "1"),
        ("SHD", "1"),
        ("SHI", "1"),
        ("SHO", "1"),
        ("UBM", "U001"),
        ("UBT", "U001"),
        ("UK1", "U000080"),
        ("UVI", "0000000000"),
        ("UYS", "U005930"),
        ("YC3", "165T6000"),
        ("YJC", "111T7000"),
        ("YJ_", "001"),
        ("h3_", "52HAAA"),
    ];

    let mut failures = 0usize;
    for (tr_cd, tr_key) in trs {
        // Each TR on a fresh manager; a failure is recorded, never propagated.
        let result = ws_lifecycle_try(tr_cd, tr_key, WsLane::MarketData).await;
        let row_note = match result {
            Ok(note) => note,
            Err(err) => {
                failures += 1;
                format!("LIFECYCLE-FAIL: {err}")
            }
        };
        // target= names the REAL runnable make target (the combined sweep); the
        // per-TR identity is carried by tr_cd= in inputs.
        record(
            "live-smoke-ws-p4",
            &format!("tr_cd={tr_cd} tr_key={tr_key} ws_port=29443 tr_type=3"),
            &row_note,
        );
    }

    assert_eq!(
        failures, 0,
        "{failures} of 39 P4 market-data TRs failed their lifecycle (see per-TR lines above)"
    );
}

/// `make live-smoke-ws-negative`: LIVE half of the KTD6 negative control.
///
/// Subscribes a deliberately-INVALID `tr_cd` and reports whether the paper
/// gateway emits an OBSERVABLE rejection within the timebox. This is the live
/// complement of the DETERMINISTIC mock-WS negative control in
/// `realtime_tests.rs::negative_control_rejected_tr_cd_is_observably_distinct_from_accepted`.
///
/// WHAT "observable" means here, and WHY it is uncertain: the SDK subscribe path
/// is FIRE-AND-FORGET — it never reads the subscribe ACK — so `subscribe_typed`
/// returns `Ok` for a valid AND an invalid `tr_cd` alike. The only live signal a
/// rejection can produce, given today's code, is an INBOUND frame the gateway
/// pushes whose `header.tr_cd`/`tr_key` route it back to this subscriber's
/// stream, surfacing as a body with a non-empty `rsp_cd` — that is the ONLY
/// tr_cd-attributable signal. A closed stream or a decode error is INCONCLUSIVE
/// (a transient disconnect produces the same close), and pure silence is
/// NOT-OBSERVABLE. If the result is anything but a clean `rsp_cd`, a rejected and
/// an accepted subscribe are indistinguishable on the live paper path, so every
/// U5/U6 flip can claim only CONNECTION-REACHABLE-ONLY, not per-TR reachability.
///
/// OPEN-QUESTION STATUS: UNRESOLVED in this environment — there is no market
/// session or live credentials here, so this smoke is `#[ignore]` and a human
/// runs it later via `make live-smoke-ws-negative`. The recorded `result=[...]`
/// line is the answer; we do NOT fabricate it. Until then the wave's flip claim
/// stays the weaker connection-reachable-only per KTD6.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials + a session to observe a rejection; run via `make live-smoke-ws-negative`"]
async fn live_smoke_ws_negative() {
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

    // A deliberately-invalid TR code: not a real LS realtime channel. The market-
    // data lane is used arbitrarily — the lane is irrelevant when the code itself
    // is bogus.
    const INVALID_TR_CD: &str = "ZZ9";

    let sdk = LsSdk::new(config).expect("sdk construction");
    let ws = sdk.realtime();

    // subscribe_typed may well return Ok even for an invalid code (fire-and-forget).
    let subscribe_outcome = ws
        .subscribe_typed::<WsLifecycleRow>(INVALID_TR_CD, &resolve_symbol(), WsLane::MarketData)
        .await;

    let observation = match subscribe_outcome {
        Err(e) => format!("subscribe returned Err immediately: {e}"),
        Ok((handle, mut stream)) => {
            // Timebox for a tr_cd-ATTRIBUTABLE rejection signal. Only an inbound
            // body routed to THIS subscriber by composite key carrying a non-empty
            // `rsp_cd` is OBSERVABLE — that is the one signal a rejection produces
            // that an acceptance does not. A bare stream close or a decode error is
            // INCONCLUSIVE: a transient gateway disconnect / reconnect-budget
            // exhaustion produces the same close and is NOT attributable to the
            // invalid tr_cd, so treating it as OBSERVABLE would false-confirm the
            // stronger per-TR reachability claim KTD6 exists to gate. Silence is
            // NOT-OBSERVABLE. INCONCLUSIVE and NOT-OBSERVABLE both leave flips at
            // connection-reachable-only.
            let note = match timeout(Duration::from_secs(5), stream.next()).await {
                Ok(Some(Ok(row))) if !row.rsp_cd.is_empty() => {
                    format!("OBSERVABLE: inbound rejection body rsp_cd={}", row.rsp_cd)
                }
                Ok(Some(Ok(_))) => {
                    "INCONCLUSIVE: inbound body with no rsp_cd (not attributable)".to_string()
                }
                Ok(Some(Err(_))) => {
                    "INCONCLUSIVE: routed frame failed to decode (not a clean rejection signal)"
                        .to_string()
                }
                Ok(None) => {
                    "INCONCLUSIVE: stream closed — indistinguishable from a transient \
                     disconnect; NOT attributable to the invalid tr_cd"
                        .to_string()
                }
                Err(_) => {
                    "NOT-OBSERVABLE: silence in timebox — flips are connection-reachable-only"
                        .to_string()
                }
            };
            // Clean up regardless of outcome.
            let _ = handle.unsubscribe().await;
            note
        }
    };

    record(
        "live-smoke-ws-negative",
        &format!("invalid_tr_cd={INVALID_TR_CD} ws_port=29443"),
        &observation,
    );
}
