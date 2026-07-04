//! Poll-derived fill detection — the authoritative lane on the bare paper gateway
//! (U3, R1/R2/R4, AE3).
//!
//! Fills emit even when the SC lane delivers nothing at all: a paced, single-page
//! t0425 read per open symbol maps `cheqty` (cumulative filled) into
//! [`FillObservation`]s for the [`FillLedger`]. Everything here is fail-closed
//! (KTD5, R4): a truncated page never concludes "no fills" (it drives a reconcile),
//! and a row whose OrdNo the ledger doesn't know is **adopted** (chain repair via
//! `orgordno`, else intent-corroboration against open `RECON-` orders) before its
//! observation applies — without adoption, an ambiguous-path order's fills are
//! unemittable on both lanes.
//!
//! Poll-derived fills emit at the row's **`cheprice`** execution price (KTD4) when
//! it parses to a positive value, else fall back to the **order's limit price** with
//! `price_approximated` set so the lab discounts them (R14). A `cheprice` row carries
//! one price per order, so any beyond-first partial is also flagged approximate. The
//! SC lane supplies exact per-execution `execprc` once certified. Each poll fill
//! carries a deterministic synthetic `TradeId` (`POLL-{ordno}-{watermark}`, minted by
//! the ledger) that cannot collide with a real SC execno (`SC-{execno}`).

use std::collections::{BTreeSet, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use ls_sdk::orders::{T0425OutBlock1, T0425Request, T0425Response};
use ls_sdk::LsSdk;
use nautilus_model::enums::OrderSide;
use nautilus_model::identifiers::ClientOrderId;

use crate::error::AdapterResult;
use crate::ingest::pacer::Pacer;
use crate::orders::ledger::{FillDelta, FillLedger, FillObservation};
use crate::parse::{lossy_i64, strict_i64};

/// The gateway's t0425 per-TR cap (2/s) — tighter than the SDK's MarketData bucket
/// (KTD5). The poll pacer meters to this to avoid `IGW00201`.
pub const T0425_POLL_PER_SEC: u32 = 2;

/// One poll pass's result: the fill deltas to emit + whether anything was
/// inconclusive (truncation / regression / unresolvable row) and needs a reconcile.
/// Crate-private: external callers go through [`drive_poll_pass`] — a raw
/// single pass would bypass the bounded reconcile drive (KTD-7).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PollOutcome {
    /// Executions to emit (already deduped by the ledger).
    pub deltas: Vec<FillDelta>,
    /// A page was truncated, a cumulative regressed, or a row could not be resolved
    /// — the caller must drive a reconcile rather than trust the read (R4).
    pub reconcile_needed: bool,
}

/// Fetches a single-page t0425 order inquiry for one symbol. Abstracted so the
/// adoption/dedup matrix is unit-testable with fakes (mirroring the ingest fetcher
/// seams), while production routes through the SDK + pacer.
#[async_trait]
pub trait T0425Fetcher {
    /// Single-page t0425 inquiry scoped to `symbol` (`expcode`).
    async fn inquiry_symbol(&self, symbol: &str) -> AdapterResult<T0425Response>;
}

#[async_trait]
impl T0425Fetcher for LsSdk {
    async fn inquiry_symbol(&self, symbol: &str) -> AdapterResult<T0425Response> {
        Ok(self.orders().inquiry(&T0425Request::for_symbol(symbol)).await?)
    }
}

/// Map a t0425 `medosu` (Korean side text) to a nautilus [`OrderSide`], or `None`
/// if unrecognized (an unmatchable side never corroborates — fail-closed).
fn side_from_medosu(medosu: &str) -> Option<OrderSide> {
    match medosu.trim() {
        "매수" | "2" => Some(OrderSide::Buy),
        "매도" | "1" => Some(OrderSide::Sell),
        _ => None,
    }
}

/// Run one poll pass over the ledger's open symbols. Paced per-symbol at ≤2/s
/// (KTD5). Never `collect_all` (page-cap trap): a single page whose `cts_ordno` is
/// non-empty is treated as truncated and drives a reconcile, never a fill
/// conclusion.
pub(crate) async fn poll_open_orders<F: T0425Fetcher>(
    fetcher: &F,
    ledger: &Mutex<FillLedger>,
    pacer: &Pacer,
) -> PollOutcome {
    // Under one lock: the open symbols, plus a drained snapshot of the pending
    // reconcile set (U2, KTD2). The drive polls their union so an unknown order's
    // symbol is scanned even on an otherwise-flat ledger (R1). A pending symbol is
    // cleared only by a completed (non-error) fetch; errored / un-scanned pending
    // symbols re-insert when the pass ends (R2).
    let (open_symbols, pending) = {
        let mut led = lock(ledger);
        (led.open_symbols(), led.take_pending_symbols())
    };
    let poll_set: BTreeSet<String> = open_symbols
        .into_iter()
        .chain(pending.iter().cloned())
        .collect();
    let mut out = PollOutcome::default();
    let mut scanned: HashSet<String> = HashSet::new();

    for symbol in &poll_set {
        pacer.acquire().await;
        let resp = match fetcher.inquiry_symbol(symbol).await {
            Ok(r) => r,
            Err(e) => {
                // An errored fetch is NOT a scan — a pending symbol here re-inserts.
                tracing::warn!(symbol, error = %e, "t0425 poll failed; will reconcile");
                out.reconcile_needed = true;
                continue;
            }
        };
        // A completed fetch (even a truncated page) counts as scanning the symbol
        // for the pending set; the truncation still drives a re-poll via
        // `reconcile_needed`, but it does not leave the symbol owed a scan (R2).
        scanned.insert(symbol.clone());
        // Fail-closed on truncation: a non-empty next-cursor means this page did not
        // show every order for the symbol — do not conclude anything (R4).
        if !resp.outblock.cts_ordno.trim().is_empty() {
            tracing::warn!(symbol, "t0425 poll page truncated; will reconcile (not concluding)");
            out.reconcile_needed = true;
            continue;
        }
        for row in &resp.outblock1 {
            apply_row(symbol, row, ledger, &mut out);
        }
    }

    // Re-record any pending symbol this pass did not conclusively scan (errored)
    // so the next pass retries it (R2/KTD2).
    let unscanned: Vec<String> = pending.into_iter().filter(|s| !scanned.contains(s)).collect();
    if !unscanned.is_empty() {
        lock(ledger).reinsert_pending_symbols(unscanned);
    }
    out
}

/// Resolve/adopt a row's OrdNo and apply its cumulative fill to the ledger.
fn apply_row(symbol: &str, row: &T0425OutBlock1, ledger: &Mutex<FillLedger>, out: &mut PollOutcome) {
    let ordno = row.ordno.trim();
    if ordno.is_empty() {
        return;
    }
    // A non-numeric cheqty is fail-closed: never read garbage as a fill quantity.
    let cheqty = match strict_i64("cheqty", &row.cheqty) {
        Ok(q) => q,
        Err(_) => {
            out.reconcile_needed = true;
            return;
        }
    };

    let mut led = lock(ledger);
    let client = match led.resolve(ordno) {
        Some(c) => Some(c),
        None => adopt_unknown_row(&mut led, symbol, row, ordno, out),
    };
    let Some(client) = client else {
        // Unknown + unadoptable → reconcile-needed (already flagged in adopt path).
        out.reconcile_needed = true;
        return;
    };

    let limit_price = led.limit_price(&client).unwrap_or_else(|| lossy_i64(&row.price));
    // Prefer the row's execution price (`cheprice`, KTD4); fall back to the limit
    // price with `price_approximated` set when it is absent/zero/garbage.
    let cheprice = lossy_i64(&row.cheprice);
    let (price, price_approximated) = if cheprice > 0 {
        (cheprice, false)
    } else {
        (limit_price, true)
    };
    // Carry the in-scope symbol on the observation (U1) for parity with the SC
    // lane; the poll path never inserts pending itself (KTD1), so this is
    // informational only.
    let symbol_opt = (!symbol.trim().is_empty()).then(|| symbol.trim().to_string());
    let outcome =
        led.apply(FillObservation::poll(ordno, cheqty, price, price_approximated).with_symbol(symbol_opt));
    out.deltas.extend(outcome.deltas);
    if outcome.reconcile_needed {
        out.reconcile_needed = true;
    }
}

/// Adopt an unknown-OrdNo row: chain-repair via `orgordno`, else intent-corroborate
/// against open `RECON-` orders for this symbol. A unique corroboration registers
/// the real OrdNo; an ambiguous one flags reconcile and adopts nothing.
fn adopt_unknown_row(
    led: &mut FillLedger,
    symbol: &str,
    row: &T0425OutBlock1,
    ordno: &str,
    out: &mut PollOutcome,
) -> Option<ClientOrderId> {
    // 1. Chain repair: a row whose ordno is unknown but whose orgordno IS known
    // adopts into that chain (a modify whose new OrdNo the SC lane never delivered).
    let org = row.orgordno.trim();
    if !org.is_empty() && led.resolve(org).is_some() && led.adopt(ordno, org) {
        return led.resolve(ordno);
    }

    // 2. Intent-corroboration against open RECON- orders for this symbol
    // (symbol + side + qty + price), mirroring the SDK's reconcile matching.
    let Some(side) = side_from_medosu(&row.medosu) else {
        out.reconcile_needed = true;
        return None;
    };
    let qty = lossy_i64(&row.qty);
    let price = lossy_i64(&row.price);
    let matches: Vec<ClientOrderId> = led
        .open_recon_candidates()
        .into_iter()
        .filter(|c| c.symbol == symbol && c.side == side && c.qty == qty && c.price == price)
        .map(|c| c.client_order_id)
        .collect();

    match matches.as_slice() {
        [only] => {
            led.adopt_for_client(ordno, *only);
            Some(*only)
        }
        // Zero or ambiguous (>1) matches: adopt nothing, keep the reconcile signal.
        _ => {
            out.reconcile_needed = true;
            None
        }
    }
}

fn lock(ledger: &Mutex<FillLedger>) -> std::sync::MutexGuard<'_, FillLedger> {
    ledger.lock().unwrap_or_else(|e| e.into_inner())
}

/// A poll pacer at the t0425 gateway cap (KTD5).
pub fn poll_pacer() -> Pacer {
    Pacer::per_sec(T0425_POLL_PER_SEC)
}

// ---------------------------------------------------------------------------
// Bounded reconcile drive (data-fidelity U7, R9/KTD-7): transient poll
// inconclusiveness self-heals inside the pass; only exhaustion reaches the
// data-quality report.
// ---------------------------------------------------------------------------

/// Re-polls a drive makes after an inconclusive pass (2-3 attempts total — the
/// cadence loop re-fires anyway, so the budget stays small).
pub const DRIVE_MAX_REPOLLS: u32 = 2;

/// Base backoff between drive re-polls. Kept small relative to the 2-second
/// cadence so a sequential drive does not starve other symbols' fills.
const DRIVE_BACKOFF: std::time::Duration = std::time::Duration::from_millis(150);

/// The explicit terminal state of a driven poll pass (KTD-7) — without it,
/// callers would resolve the healed-vs-exhausted distinction inconsistently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveTerminal {
    /// A pass concluded cleanly (possibly after re-polls) — nothing is reported.
    Resolved,
    /// The re-poll budget ran out with the pass still inconclusive — the live
    /// runner records a reconcile-advised condition.
    Exhausted,
}

/// A driven poll pass's result: fill deltas accumulated across the pass and its
/// re-polls (each already exactly-once via the ledger's per-OrdNo watermarks +
/// execno dedup), plus the terminal state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrivenOutcome {
    /// Executions to emit, in observation order across attempts.
    pub deltas: Vec<FillDelta>,
    /// How the drive ended.
    pub terminal: DriveTerminal,
}

impl DrivenOutcome {
    /// Whether the drive exhausted its budget still inconclusive.
    pub fn exhausted(&self) -> bool {
        self.terminal == DriveTerminal::Exhausted
    }
}

/// Run a poll pass and, on `reconcile_needed`, re-poll with bounded backoff
/// (R9/KTD-7). Shares the caller's single `pacer` — constructing a second pacer
/// here could jointly exceed t0425's 2/s gateway cap (IGW00201). Any
/// inconclusive cause (truncation, regression, unresolvable row, request
/// failure) consumes the same attempt budget — a regression during a re-poll
/// counts toward exhaustion rather than restarting it.
pub async fn drive_poll_pass<F: T0425Fetcher>(
    fetcher: &F,
    ledger: &Mutex<FillLedger>,
    pacer: &Pacer,
) -> DrivenOutcome {
    let mut pass = poll_open_orders(fetcher, ledger, pacer).await;
    let mut deltas = std::mem::take(&mut pass.deltas);
    let mut attempts = 0u32;
    while pass.reconcile_needed {
        if attempts >= DRIVE_MAX_REPOLLS {
            return DrivenOutcome {
                deltas,
                terminal: DriveTerminal::Exhausted,
            };
        }
        attempts += 1;
        tokio::time::sleep(DRIVE_BACKOFF * attempts).await;
        pass = poll_open_orders(fetcher, ledger, pacer).await;
        deltas.extend(std::mem::take(&mut pass.deltas));
    }
    DrivenOutcome {
        deltas,
        terminal: DriveTerminal::Resolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::enums::{OrderType, TimeInForce};
    use nautilus_model::identifiers::InstrumentId;
    use nautilus_model::orders::{OrderAny, OrderTestBuilder};
    use nautilus_model::types::{Price, Quantity};

    fn order(client_id: &str, qty: i64, price: i64, side: OrderSide) -> OrderAny {
        OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(InstrumentId::from("005930.XKRX"))
            .client_order_id(ClientOrderId::from(client_id))
            .side(side)
            .quantity(Quantity::from(qty))
            .price(Price::from(price.to_string().as_str()))
            .time_in_force(TimeInForce::Day)
            .build()
    }

    fn row(ordno: &str, orgordno: &str, medosu: &str, qty: &str, price: &str, cheqty: &str, ordrem: &str) -> T0425OutBlock1 {
        row_px(ordno, orgordno, medosu, qty, price, cheqty, ordrem, "")
    }

    #[allow(clippy::too_many_arguments)]
    fn row_px(
        ordno: &str,
        orgordno: &str,
        medosu: &str,
        qty: &str,
        price: &str,
        cheqty: &str,
        ordrem: &str,
        cheprice: &str,
    ) -> T0425OutBlock1 {
        T0425OutBlock1 {
            ordno: ordno.into(),
            expcode: "005930".into(),
            medosu: medosu.into(),
            qty: qty.into(),
            price: price.into(),
            cheqty: cheqty.into(),
            cheprice: cheprice.into(),
            ordrem: ordrem.into(),
            status: "체결".into(),
            orgordno: orgordno.into(),
            ordtime: "0900".into(),
        }
    }

    fn resp(cts: &str, rows: Vec<T0425OutBlock1>) -> T0425Response {
        let mut r = T0425Response {
            rsp_cd: "00000".into(),
            outblock1: rows,
            ..Default::default()
        };
        r.outblock.cts_ordno = cts.into();
        r
    }

    struct FakeFetcher {
        resp: T0425Response,
    }
    #[async_trait]
    impl T0425Fetcher for FakeFetcher {
        async fn inquiry_symbol(&self, _symbol: &str) -> AdapterResult<T0425Response> {
            Ok(self.resp.clone())
        }
    }

    fn ledger_with_order(client_id: &str, qty: i64, price: i64, ord_no: &str) -> Mutex<FillLedger> {
        let mut led = FillLedger::new();
        led.register(order(client_id, qty, price, OrderSide::Buy), ord_no);
        Mutex::new(led)
    }

    /// U2 fallback: a row with no `cheprice` emits the poll fill at the order's limit
    /// price with `price_approximated` set and a synthetic TradeId.
    #[tokio::test]
    async fn poll_fallback_emits_at_limit_price_flagged() {
        let led = ledger_with_order("O-1", 100, 60_000, "1001");
        let fetcher = FakeFetcher {
            resp: resp("", vec![row("1001", "", "매수", "100", "60000", "40", "60")]),
        };
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].qty, 40);
        assert_eq!(out.deltas[0].price, 60_000, "a cheprice-less row falls back to the limit price");
        assert!(out.deltas[0].price_approximated, "the limit-price fallback is flagged approximate");
        assert!(out.deltas[0].trade_id.to_string().starts_with("POLL-1001-"));
        assert!(!out.reconcile_needed);
    }

    /// U2 happy path (AE5): a first fill whose `cheprice` differs from the order limit
    /// emits at `cheprice`, unflagged.
    #[tokio::test]
    async fn poll_emits_first_fill_at_cheprice_unflagged() {
        let led = ledger_with_order("O-CP", 100, 60_000, "1001");
        let fetcher = FakeFetcher {
            resp: resp("", vec![row_px("1001", "", "매수", "100", "60000", "40", "60", "60050")]),
        };
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].price, 60_050, "the fill emits at the row's cheprice");
        assert!(!out.deltas[0].price_approximated, "a first fill at cheprice is exact");
    }

    /// U2 multi-fill: a second delta against the same OrdNo (watermark already
    /// positive) is flagged approximate even with a positive `cheprice`.
    #[tokio::test]
    async fn poll_second_partial_is_flagged_even_with_cheprice() {
        let led = ledger_with_order("O-MP", 100, 60_000, "1001");
        let f1 = FakeFetcher {
            resp: resp("", vec![row_px("1001", "", "매수", "100", "60000", "30", "70", "60050")]),
        };
        let out1 = poll_open_orders(&f1, &led, &poll_pacer()).await;
        assert!(!out1.deltas[0].price_approximated, "first partial exact");
        let f2 = FakeFetcher {
            resp: resp("", vec![row_px("1001", "", "매수", "100", "60000", "100", "0", "60070")]),
        };
        let out2 = poll_open_orders(&f2, &led, &poll_pacer()).await;
        assert_eq!(out2.deltas[0].qty, 70);
        assert!(out2.deltas[0].price_approximated, "beyond-first partial is approximate");
    }

    /// U2 edge: a modify-chained order's new OrdNo emits at the NEW row's `cheprice`
    /// (per-OrdNo watermark unchanged, so its first fill is exact).
    #[tokio::test]
    async fn poll_modify_chain_new_ordno_uses_new_cheprice() {
        let led = ledger_with_order("O-MC", 100, 60_000, "1001");
        // A modify chained OrdNo 1002 (parent 1001) whose row carries its own cheprice.
        let fetcher = FakeFetcher {
            resp: resp("", vec![row_px("1002", "1001", "매수", "100", "60000", "50", "50", "60120")]),
        };
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].ord_no, "1002");
        assert_eq!(out.deltas[0].price, 60_120, "the new OrdNo emits at its own cheprice");
        assert!(!out.deltas[0].price_approximated, "first fill on the new OrdNo is exact");
    }

    /// A RECON--accepted order fills: the unknown-ordno row intent-corroborates
    /// against the open RECON- entry, registers the real OrdNo, and the fill emits.
    #[tokio::test]
    async fn recon_order_corroborates_and_fills() {
        let led = ledger_with_order("O-2", 100, 60_000, "RECON-O-2");
        // A partial fill (40 of 100) so the chain is not yet forgotten (terminal).
        let fetcher = FakeFetcher {
            resp: resp("", vec![row("5001", "", "매수", "100", "60000", "40", "60")]),
        };
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].qty, 40);
        assert!(!out.deltas[0].terminal);
        // The real OrdNo was adopted into the chain.
        assert!(lock(&led).resolve("5001").is_some());
    }

    /// Chain repair via orgordno: a row whose ordno is unknown but whose orgordno
    /// resolves adopts into that chain.
    #[tokio::test]
    async fn chain_repair_via_orgordno() {
        let led = ledger_with_order("O-3", 100, 60_000, "1001");
        let fetcher = FakeFetcher {
            // A modify chained OrdNo 1002 (parent 1001) the SC lane never delivered.
            resp: resp("", vec![row("1002", "1001", "매수", "100", "60000", "30", "70")]),
        };
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].qty, 30);
        assert_eq!(out.deltas[0].ord_no, "1002");
    }

    /// Ambiguous corroboration (two open RECON- entries match) → no adoption,
    /// reconcile-needed.
    #[tokio::test]
    async fn ambiguous_corroboration_reconciles() {
        let mut led = FillLedger::new();
        led.register(order("O-A", 100, 60_000, OrderSide::Buy), "RECON-O-A");
        led.register(order("O-B", 100, 60_000, OrderSide::Buy), "RECON-O-B");
        let led = Mutex::new(led);
        let fetcher = FakeFetcher {
            resp: resp("", vec![row("5001", "", "매수", "100", "60000", "50", "50")]),
        };
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert!(out.deltas.is_empty(), "an ambiguous match adopts nothing");
        assert!(out.reconcile_needed);
    }

    /// A truncated page (non-empty cts_ordno) → no fill conclusion, reconcile path.
    #[tokio::test]
    async fn truncated_page_reconciles_no_conclusion() {
        let led = ledger_with_order("O-4", 100, 60_000, "1001");
        let fetcher = FakeFetcher {
            resp: resp("NEXT", vec![row("1001", "", "매수", "100", "60000", "40", "60")]),
        };
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert!(out.deltas.is_empty(), "a truncated page must not conclude a fill");
        assert!(out.reconcile_needed);
    }

    /// A flat ledger makes zero t0425 calls (loop idles).
    #[tokio::test]
    async fn flat_ledger_polls_nothing() {
        let led = Mutex::new(FillLedger::new());
        // A fetcher that panics if ever called proves no poll happens.
        struct Never;
        #[async_trait]
        impl T0425Fetcher for Never {
            async fn inquiry_symbol(&self, _s: &str) -> AdapterResult<T0425Response> {
                panic!("a flat ledger must not poll");
            }
        }
        let out = poll_open_orders(&Never, &led, &poll_pacer()).await;
        assert!(out.deltas.is_empty());
        assert!(!out.reconcile_needed);
    }

    // --- bounded reconcile drive (data-fidelity U7, R9/KTD-7) ---

    /// Serves a scripted sequence of responses; the last repeats forever. Counts
    /// calls so tests can assert the drive's attempt budget.
    struct ScriptedFetcher {
        responses: Mutex<std::collections::VecDeque<T0425Response>>,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ScriptedFetcher {
        fn new(responses: Vec<T0425Response>) -> Self {
            assert!(!responses.is_empty());
            ScriptedFetcher {
                responses: Mutex::new(responses.into()),
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl T0425Fetcher for ScriptedFetcher {
        async fn inquiry_symbol(&self, _symbol: &str) -> AdapterResult<T0425Response> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut q = self.responses.lock().unwrap();
            Ok(if q.len() > 1 {
                q.pop_front().unwrap()
            } else {
                q.front().cloned().unwrap()
            })
        }
    }

    /// AE4 (resolved arm): a pass that truncates once then completes on the
    /// re-poll resolves — the deltas emit and nothing is left to report.
    #[tokio::test]
    async fn drive_resolves_a_transient_truncation() {
        let led = ledger_with_order("O-D1", 100, 60_000, "1001");
        let fetcher = ScriptedFetcher::new(vec![
            resp("NEXT", vec![]),
            resp("", vec![row("1001", "", "매수", "100", "60000", "40", "60")]),
        ]);
        let out = drive_poll_pass(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.terminal, DriveTerminal::Resolved);
        assert_eq!(out.deltas.len(), 1, "the re-poll's fill emits from the drive");
        assert_eq!(out.deltas[0].qty, 40);
        assert_eq!(fetcher.calls(), 2, "one re-poll healed it");
    }

    /// AE4 (exhausted arm): truncation persisting through the whole budget ends
    /// Exhausted after exactly 1 + DRIVE_MAX_REPOLLS attempts.
    #[tokio::test]
    async fn drive_exhausts_on_persistent_truncation() {
        let led = ledger_with_order("O-D2", 100, 60_000, "1001");
        let fetcher = ScriptedFetcher::new(vec![resp("NEXT", vec![])]);
        let out = drive_poll_pass(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.terminal, DriveTerminal::Exhausted);
        assert!(out.deltas.is_empty());
        assert_eq!(
            fetcher.calls() as u32,
            1 + DRIVE_MAX_REPOLLS,
            "the budget is bounded and consumed exactly once"
        );
    }

    /// A regression row during a re-poll counts toward exhaustion rather than
    /// restarting the budget: mixed inconclusive causes (truncation, then
    /// regressions) still end after 1 + DRIVE_MAX_REPOLLS attempts.
    #[tokio::test]
    async fn regression_counts_toward_exhaustion_not_a_reset() {
        let led = ledger_with_order("O-D3", 100, 60_000, "1001");
        // Pre-establish a watermark of 50 on OrdNo 1001.
        lock(&led).apply(FillObservation::poll("1001", 50, 60_000, false));
        let fetcher = ScriptedFetcher::new(vec![
            resp("NEXT", vec![]), // attempt 1: truncated
            // Attempts 2..: cheqty 20 < watermark 50 — a cumulative regression.
            resp("", vec![row("1001", "", "매수", "100", "60000", "20", "80")]),
        ]);
        let out = drive_poll_pass(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.terminal, DriveTerminal::Exhausted);
        assert!(out.deltas.is_empty(), "a regression never emits");
        assert_eq!(
            fetcher.calls() as u32,
            1 + DRIVE_MAX_REPOLLS,
            "the regression consumed budget instead of resetting it"
        );
    }

    /// Exactly-once across a drive: a fill observed on the first pass and
    /// re-observed (same cumulative) on a re-poll emits once — the per-OrdNo
    /// watermark + execno dedup cover the drive's re-poll path explicitly.
    #[tokio::test]
    async fn fills_reobserved_during_a_drive_emit_once() {
        let led = ledger_with_order("O-D4", 100, 60_000, "1001");
        // Every pass carries the SAME cumulative fill (40) plus a garbage-cheqty
        // row that keeps the pass inconclusive so the drive re-polls.
        let inconclusive = resp(
            "",
            vec![
                row("1001", "", "매수", "100", "60000", "40", "60"),
                row("1001", "", "매수", "100", "60000", "N/A", "60"),
            ],
        );
        let fetcher = ScriptedFetcher::new(vec![inconclusive]);
        let out = drive_poll_pass(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.terminal, DriveTerminal::Exhausted, "the garbage row exhausts the drive");
        assert_eq!(out.deltas.len(), 1, "the re-observed cumulative emits exactly once");
        assert_eq!(out.deltas[0].qty, 40);
    }

    /// A clean pass never re-polls — the drive is free when nothing is wrong.
    #[tokio::test]
    async fn clean_pass_makes_no_repolls() {
        let led = ledger_with_order("O-D5", 100, 60_000, "1001");
        let fetcher = ScriptedFetcher::new(vec![resp(
            "",
            vec![row("1001", "", "매수", "100", "60000", "40", "60")],
        )]);
        let out = drive_poll_pass(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(out.terminal, DriveTerminal::Resolved);
        assert_eq!(fetcher.calls(), 1, "a conclusive pass is not re-polled");
    }

    // --- U2: pending-reconcile set drive union (R1/R2/R3, AE1) ---

    /// Records every symbol it is asked to poll, and serves one scripted response
    /// per call (last repeats). Lets the union/scan-tracking be asserted directly.
    struct RecordingFetcher {
        symbols: Mutex<Vec<String>>,
        resp: T0425Response,
    }
    impl RecordingFetcher {
        fn new(resp: T0425Response) -> Self {
            RecordingFetcher { symbols: Mutex::new(Vec::new()), resp }
        }
        fn polled(&self) -> Vec<String> {
            self.symbols.lock().unwrap().clone()
        }
    }
    #[async_trait]
    impl T0425Fetcher for RecordingFetcher {
        async fn inquiry_symbol(&self, symbol: &str) -> AdapterResult<T0425Response> {
            self.symbols.lock().unwrap().push(symbol.to_string());
            Ok(self.resp.clone())
        }
    }

    /// AE1: an unknown fill left a symbol pending on an otherwise-flat ledger; the
    /// next drive pass polls exactly that symbol, and a completed (empty) fetch
    /// clears it so it is not re-polled.
    #[tokio::test]
    async fn pending_symbol_is_polled_on_a_flat_ledger_then_cleared() {
        let led = Mutex::new(FillLedger::new());
        lock(&led).record_pending_symbol("000660");
        let fetcher = RecordingFetcher::new(resp("", vec![]));
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert_eq!(fetcher.polled(), vec!["000660".to_string()], "the drive polls the pending symbol");
        assert!(!out.reconcile_needed);
        assert!(!lock(&led).has_pending(), "a completed fetch clears the pending symbol");
    }

    /// The pending symbol unions with (and dedups against) the open symbols.
    #[tokio::test]
    async fn pending_unions_with_open_symbols() {
        // Open order on 005930; pending on 000660 → both polled once, no dup.
        let led = ledger_with_order("O-U", 100, 60_000, "1001");
        lock(&led).record_pending_symbol("000660");
        lock(&led).record_pending_symbol("005930"); // already open → dedup in the union
        let fetcher = RecordingFetcher::new(resp("", vec![]));
        poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        let mut polled = fetcher.polled();
        polled.sort();
        assert_eq!(polled, vec!["000660".to_string(), "005930".to_string()], "union polls each symbol once");
    }

    /// R2: a pending symbol whose fetch errors is re-inserted (pending again next
    /// pass); an open order is unaffected.
    #[tokio::test]
    async fn errored_pending_symbol_reinserts() {
        struct ErrFetcher;
        #[async_trait]
        impl T0425Fetcher for ErrFetcher {
            async fn inquiry_symbol(&self, _s: &str) -> AdapterResult<T0425Response> {
                Err(crate::error::AdapterError::Ingest("boom".into()))
            }
        }
        let led = Mutex::new(FillLedger::new());
        lock(&led).record_pending_symbol("000660");
        let out = poll_open_orders(&ErrFetcher, &led, &poll_pacer()).await;
        assert!(out.reconcile_needed);
        assert!(lock(&led).has_pending(), "an errored pending symbol re-inserts for the next pass");
        assert_eq!(lock(&led).take_pending_symbols(), vec!["000660".to_string()]);
    }

    /// KTD1 no-self-sustain: a pending symbol whose poll response contains only a
    /// FOREIGN (unknown) order clears — the completed scan is the reconcile, and the
    /// poll path never re-inserts, so the loop idles next pass.
    #[tokio::test]
    async fn pending_symbol_with_only_a_foreign_order_clears_no_reinsert() {
        let led = Mutex::new(FillLedger::new()); // flat: no tracked orders
        lock(&led).record_pending_symbol("000660");
        // A foreign resting order (ordno 7777 unknown to the ledger, no recon match).
        let fetcher = RecordingFetcher::new(resp("", vec![row("7777", "", "매수", "100", "60000", "0", "100")]));
        let out = poll_open_orders(&fetcher, &led, &poll_pacer()).await;
        assert!(out.reconcile_needed, "an unknown order flags reconcile");
        assert!(
            !lock(&led).has_pending(),
            "the completed scan clears the symbol; a foreign order must not self-sustain the pending set"
        );
    }

    /// Partial then full fill across two polls → two deltas, second terminal.
    #[tokio::test]
    async fn partial_then_full_across_polls() {
        let led = ledger_with_order("O-5", 100, 60_000, "1001");
        let f1 = FakeFetcher { resp: resp("", vec![row("1001", "", "매수", "100", "60000", "30", "70")]) };
        let out1 = poll_open_orders(&f1, &led, &poll_pacer()).await;
        assert_eq!(out1.deltas[0].qty, 30);
        assert!(!out1.deltas[0].terminal);
        let f2 = FakeFetcher { resp: resp("", vec![row("1001", "", "매수", "100", "60000", "100", "0")]) };
        let out2 = poll_open_orders(&f2, &led, &poll_pacer()).await;
        assert_eq!(out2.deltas[0].qty, 70);
        assert!(out2.deltas[0].terminal);
    }
}
