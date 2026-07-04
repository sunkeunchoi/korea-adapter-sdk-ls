//! U2 offline integration: the SC0/SC1 order-event lane through the (generic) WS
//! supervisor against the mock WS server. The mock SC frame scripts ARE the offline
//! certification of the order-event lane (R11): a fill sequence flows into the fill
//! ledger with the right deltas, an ACK frame emits nothing, an unknown OrdNo emits
//! nothing + flags a reconcile, and a terminal reconnect-budget error rebuilds and
//! resubscribes SC0/SC1. No live calls.

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use ls_core::LsConfig;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token, MockWsServer};
use nautilus_ls::orders::ledger::FillLedger;
use nautilus_ls::ws::rows::OrderEventMsg;
use nautilus_ls::ws::supervisor::{RowKind, SubSpec, WsSupervisor};
use nautilus_model::enums::{OrderSide, OrderType, TimeInForce};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId};
use nautilus_model::orders::{OrderAny, OrderTestBuilder};
use nautilus_model::types::{Price, Quantity};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;
use wiremock::MockServer;

async fn sdk_over_ws(http: &MockServer, ws_url: String) -> LsSdk {
    mount_token(http).await;
    let cfg = LsConfig {
        ws_base_url: Some(ws_url),
        ..mock_config(&http.uri())
    };
    LsSdk::new(cfg).expect("sdk builds")
}

fn sc_spec(tr_cd: &str, kind: RowKind) -> SubSpec {
    SubSpec {
        tr_cd: tr_cd.to_string(),
        tr_key: String::new(), // account-wide order events
        instrument_id: InstrumentId::from("SC.XKRX"),
        kind,
    }
}

fn sc1_fill_frame(ordno: &str, execno: &str, execqty: &str, execprc: &str) -> String {
    sc1_fill_frame_sym(ordno, execno, execqty, execprc, "005930")
}

fn sc1_fill_frame_sym(ordno: &str, execno: &str, execqty: &str, execprc: &str, shtn_isuno: &str) -> String {
    serde_json::json!({
        "header": { "tr_cd": "SC1", "tr_key": "" },
        "body": { "ordno": ordno, "execno": execno, "ordqty": "100", "ordprc": "60000",
                  "execqty": execqty, "execprc": execprc, "shtnIsuno": shtn_isuno }
    })
    .to_string()
}

fn order(client_id: &str, qty: i64, price: i64) -> OrderAny {
    OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(InstrumentId::from("005930.XKRX"))
        .client_order_id(ClientOrderId::from(client_id))
        .side(OrderSide::Buy)
        .quantity(Quantity::from(qty))
        .price(Price::from(price.to_string().as_str()))
        .time_in_force(TimeInForce::Day)
        .build()
}

async fn wait_subscribe(server: &MockWsServer, tr_cd: &str, tr_type: &str, budget: Duration) -> bool {
    let start = tokio::time::Instant::now();
    while start.elapsed() < budget {
        if server.count_subscribe_frames(tr_cd, tr_type).await >= 1 {
            return true;
        }
        sleep(Duration::from_millis(25)).await;
    }
    false
}

/// SC0/SC1 register on the OrderEvent lane with tr_type "1" (NOT the market-data
/// "3") — the lane-specific behavior.
#[tokio::test]
async fn sc_lane_registers_with_order_event_tr_type() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, _rx) = mpsc::unbounded_channel::<OrderEventMsg>();
    let sup = WsSupervisor::spawn_order_events(sdk, tx);

    sup.subscribe(sc_spec("SC0", RowKind::OrderAccept));
    sup.subscribe(sc_spec("SC1", RowKind::OrderFill));

    assert!(wait_subscribe(&ws, "SC0", "1", Duration::from_secs(3)).await, "SC0 registers on the order lane (tr_type 1)");
    assert!(wait_subscribe(&ws, "SC1", "1", Duration::from_secs(3)).await, "SC1 registers on the order lane (tr_type 1)");
    // And never as a market-data (tr_type 3) subscription.
    assert_eq!(ws.count_subscribe_frames("SC1", "3").await, 0, "SC1 is not a market-data subscription");
    sup.shutdown();
}

/// A partial-then-full SC1 fill sequence flows into the ledger with the right deltas
/// (an ACK frame in between emits nothing).
#[tokio::test]
async fn sc1_fill_sequence_feeds_ledger_with_right_deltas() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, mut rx) = mpsc::unbounded_channel::<OrderEventMsg>();
    let sup = WsSupervisor::spawn_order_events(sdk, tx);

    let mut ledger = FillLedger::new();
    ledger.register(order("O-1", 100, 60_000), "1001");

    sup.subscribe(sc_spec("SC1", RowKind::OrderFill));
    assert!(wait_subscribe(&ws, "SC1", "1", Duration::from_secs(3)).await);

    // A registration-ACK (empty body) emits nothing.
    ws.push_frame(serde_json::json!({ "header": { "tr_cd": "SC1", "tr_key": "" }, "body": {} }).to_string());
    // Partial fill (30) then full fill (70) → two deltas, second terminal.
    ws.push_frame(sc1_fill_frame("1001", "E1", "30", "60000"));
    ws.push_frame(sc1_fill_frame("1001", "E2", "70", "60050"));

    let mut deltas = Vec::new();
    while deltas.len() < 2 {
        let msg = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("an SC fill message arrives")
            .expect("channel open");
        match msg {
            OrderEventMsg::Fill(obs) => {
                let out = ledger.apply(obs);
                deltas.extend(out.deltas);
            }
            OrderEventMsg::Accept { .. } => panic!("no accept expected on SC1"),
        }
    }
    assert_eq!(deltas.len(), 2);
    assert_eq!(deltas[0].qty, 30);
    assert!(!deltas[0].terminal);
    assert_eq!(deltas[1].qty, 70);
    assert!(deltas[1].terminal);
    sup.shutdown();
}

/// An SC1 registration-ACK frame emits no order-event message at all.
#[tokio::test]
async fn sc1_ack_frame_emits_nothing() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, mut rx) = mpsc::unbounded_channel::<OrderEventMsg>();
    let sup = WsSupervisor::spawn_order_events(sdk, tx);

    sup.subscribe(sc_spec("SC1", RowKind::OrderFill));
    assert!(wait_subscribe(&ws, "SC1", "1", Duration::from_secs(3)).await);

    ws.push_frame(serde_json::json!({ "header": { "tr_cd": "SC1", "tr_key": "" }, "body": {} }).to_string());
    let got = timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(got.is_err(), "an ACK frame must not emit an order event");
    sup.shutdown();
}

/// An SC1 fill for an unknown OrdNo emits no delta and flags a reconcile (SC-only:
/// no poll lane running).
#[tokio::test]
async fn sc1_unknown_ordno_no_delta_flags_reconcile() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, mut rx) = mpsc::unbounded_channel::<OrderEventMsg>();
    let sup = WsSupervisor::spawn_order_events(sdk, tx);

    let mut ledger = FillLedger::new();
    ledger.register(order("O-1", 100, 60_000), "1001");

    sup.subscribe(sc_spec("SC1", RowKind::OrderFill));
    assert!(wait_subscribe(&ws, "SC1", "1", Duration::from_secs(3)).await);

    // The unknown fill names symbol 000660 (distinct from the ledger's open 005930).
    ws.push_frame(sc1_fill_frame_sym("9999", "E9", "10", "60000", "000660"));
    let msg = timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
    match msg {
        OrderEventMsg::Fill(obs) => {
            // U1: the observation carries the bare symbol through the ToEvent seam.
            assert_eq!(obs.symbol.as_deref(), Some("000660"), "the traded symbol survives to the observation");
            let out = ledger.apply(obs);
            assert!(out.deltas.is_empty(), "an unknown OrdNo emits no fill");
            assert!(out.reconcile_needed, "an unknown OrdNo flags a reconcile");
            // R1: the ledger recorded that symbol pending so the drive scans it.
            assert!(ledger.has_pending());
            assert_eq!(ledger.take_pending_symbols(), vec!["000660".to_string()]);
        }
        other => panic!("expected a fill, got {other:?}"),
    }
    sup.shutdown();
}

// ---------------------------------------------------------------------------
// Terminal reconnect-budget error → rebuild + resubscribe SC0/SC1 (the SAME
// supervisor lifecycle the market-data lane uses; here proven on the order lane).
// ---------------------------------------------------------------------------

struct FixedPortWs {
    port: u16,
    received: Arc<Mutex<Vec<String>>>,
    #[allow(dead_code)]
    bcast: broadcast::Sender<String>,
    kill: broadcast::Sender<()>,
    accept: tokio::task::JoinHandle<()>,
}

impl FixedPortWs {
    async fn start(port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind fixed port");
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let (bcast, _) = broadcast::channel::<String>(64);
        let (kill, _) = broadcast::channel::<()>(16);
        let recv_loop = Arc::clone(&received);
        let bcast_loop = bcast.clone();
        let kill_loop = kill.clone();
        let accept = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { break };
                let recv = Arc::clone(&recv_loop);
                let mut brx = bcast_loop.subscribe();
                let mut krx = kill_loop.subscribe();
                tokio::spawn(async move {
                    let Ok(mut wss) = tokio_tungstenite::accept_async(stream).await else { return };
                    loop {
                        tokio::select! {
                            frame = wss.next() => match frame {
                                Some(Ok(Message::Text(t))) => recv.lock().await.push(t.to_string()),
                                _ => break,
                            },
                            msg = brx.recv() => if let Ok(text) = msg {
                                let _ = wss.send(Message::Text(text.into())).await;
                            },
                            _ = krx.recv() => break,
                        }
                    }
                });
            }
        });
        FixedPortWs { port, received, bcast, kill, accept }
    }

    fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    async fn count_subscribe(&self, tr_cd: &str, tr_type: &str) -> usize {
        self.received
            .lock()
            .await
            .iter()
            .filter_map(|f| serde_json::from_str::<serde_json::Value>(f).ok())
            .filter(|v| {
                v["body"]["tr_cd"].as_str() == Some(tr_cd)
                    && v["header"]["tr_type"].as_str() == Some(tr_type)
            })
            .count()
    }

    fn stop(self) {
        let _ = self.kill.send(());
        self.accept.abort();
    }
}

async fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_until<F: Fn() -> bool>(cond: F, budget: Duration) -> bool {
    let start = tokio::time::Instant::now();
    while start.elapsed() < budget {
        if cond() {
            return true;
        }
        sleep(Duration::from_millis(25)).await;
    }
    cond()
}

#[tokio::test]
async fn order_lane_reconnect_rebuilds_and_resubscribes() {
    let port = free_port().await;
    let http = MockServer::start().await;
    let ws = FixedPortWs::start(port).await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, _rx) = mpsc::unbounded_channel::<OrderEventMsg>();
    let sup = WsSupervisor::spawn_order_events(sdk, tx);

    sup.subscribe(sc_spec("SC1", RowKind::OrderFill));
    let saw_first = {
        let start = tokio::time::Instant::now();
        let mut ok = false;
        while start.elapsed() < Duration::from_secs(3) {
            if ws.count_subscribe("SC1", "1").await >= 1 {
                ok = true;
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }
        ok
    };
    assert!(saw_first, "initial SC1 subscribe reached the server");
    assert!(sup.is_connected());

    // Take the server down: the SDK exhausts its reconnect budget and delivers the
    // terminal error, purging the subscription.
    ws.stop();
    assert!(
        wait_until(|| !sup.is_connected(), Duration::from_secs(20)).await,
        "supervisor observes the terminal error and goes disconnected"
    );

    // Bring it back on the same port; the rebuild resubscribes SC1 on tr_type "1".
    let ws2 = FixedPortWs::start(port).await;
    assert!(
        wait_until(|| sup.is_connected(), Duration::from_secs(20)).await,
        "supervisor rebuilds the order-event session"
    );
    let saw_resub = {
        let start = tokio::time::Instant::now();
        let mut ok = false;
        while start.elapsed() < Duration::from_secs(5) {
            if ws2.count_subscribe("SC1", "1").await >= 1 {
                ok = true;
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }
        ok
    };
    assert!(saw_resub, "SC1 was re-established on the new order-event session");
    sup.shutdown();
    ws2.stop();
}
