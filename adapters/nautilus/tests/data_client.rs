//! U5 offline integration: the live data client + WS supervisor against the mock
//! WS server. Covers AE4 (terminal reconnect-budget error → supervisor rebuild +
//! resubscribe → the node does not end). No live calls.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use ls_core::LsConfig;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token, MockWsServer};
use nautilus_common::clients::DataClient;
use nautilus_common::messages::data::SubscribeTrades;
use nautilus_common::messages::DataEvent;
use nautilus_core::{UnixNanos, UUID4};
use nautilus_ls::data::LsDataClient;
use nautilus_ls::rules::Market;
use nautilus_ls::ws::supervisor::{RowKind, SubSpec, WsSupervisor};
use nautilus_model::data::Data;
use nautilus_model::identifiers::InstrumentId;
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

fn trade_spec(shcode: &str, kind: RowKind, tr_cd: &str) -> SubSpec {
    SubSpec {
        tr_cd: tr_cd.to_string(),
        tr_key: shcode.to_string(),
        instrument_id: InstrumentId::from(format!("{shcode}.XKRX").as_str()),
        kind,
    }
}

/// Poll `cond` until it returns true or `budget` elapses.
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

#[tokio::test]
async fn s3_frame_yields_a_trade_tick() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, mut rx) = mpsc::unbounded_channel::<DataEvent>();
    let sup = WsSupervisor::spawn(sdk, tx);

    sup.subscribe(trade_spec("005930", RowKind::Trade, "S3_"));
    assert!(
        wait_subscribe(&ws, "S3_", "3", Duration::from_secs(3)).await,
        "the subscribe frame reached the server"
    );

    ws.push_s3(
        "005930",
        serde_json::json!({
            "chetime": "090001", "cgubun": "1", "price": "60500",
            "cvolume": "10", "volume": "12345", "shcode": "005930"
        }),
    );

    let ev = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("a data event arrives")
        .expect("channel open");
    match ev {
        DataEvent::Data(Data::Trade(t)) => {
            assert_eq!(t.price, Price::from("60500"));
            assert_eq!(t.size, Quantity::from(10));
            assert!(t.ts_event.as_u64() > 0);
        }
        other => panic!("expected a trade, got {other:?}"),
    }
    sup.shutdown();
}

#[tokio::test]
async fn ack_frame_emits_nothing() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, mut rx) = mpsc::unbounded_channel::<DataEvent>();
    let sup = WsSupervisor::spawn(sdk, tx);

    sup.subscribe(trade_spec("005930", RowKind::Trade, "S3_"));
    assert!(wait_subscribe(&ws, "S3_", "3", Duration::from_secs(3)).await);

    // An all-default (empty body) registration-ACK row → filtered from emission.
    ws.push_s3("005930", serde_json::json!({}));

    let got = timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(got.is_err(), "an ACK row must not emit a data event");
    sup.shutdown();
}

#[tokio::test]
async fn h1_frame_yields_a_top_of_book_quote() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, mut rx) = mpsc::unbounded_channel::<DataEvent>();
    let sup = WsSupervisor::spawn(sdk, tx);

    sup.subscribe(trade_spec("005930", RowKind::Quote, "H1_"));
    assert!(wait_subscribe(&ws, "H1_", "3", Duration::from_secs(3)).await);

    ws.push_frame(
        serde_json::json!({
            "header": { "tr_cd": "H1_", "tr_key": "005930" },
            "body": { "hotime": "090002", "offerho1": "60600", "bidho1": "60500",
                      "offerrem1": "100", "bidrem1": "200", "shcode": "005930" }
        })
        .to_string(),
    );

    let ev = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("quote arrives")
        .unwrap();
    match ev {
        DataEvent::Data(Data::Quote(q)) => {
            assert_eq!(q.bid_price, Price::from("60500"));
            assert_eq!(q.ask_price, Price::from("60600"));
        }
        other => panic!("expected a quote, got {other:?}"),
    }
    sup.shutdown();
}

#[tokio::test]
async fn unsubscribe_sends_the_deregister_frame() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, _rx) = mpsc::unbounded_channel::<DataEvent>();
    let sup = WsSupervisor::spawn(sdk, tx);

    sup.subscribe(trade_spec("005930", RowKind::Trade, "S3_"));
    assert!(wait_subscribe(&ws, "S3_", "3", Duration::from_secs(3)).await);
    sup.unsubscribe("S3_", "005930");

    // The deregister frame is tr_type "4" on the MarketData lane.
    let saw = {
        let start = tokio::time::Instant::now();
        let mut ok = false;
        while start.elapsed() < Duration::from_secs(3) {
            if ws.count_subscribe_frames("S3_", "4").await >= 1 {
                ok = true;
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }
        ok
    };
    assert!(saw, "unsubscribe deregister frame reached the server");
    sup.shutdown();
}

#[tokio::test]
async fn never_delivered_diagnostic_flags_a_silent_subscription() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, _rx) = mpsc::unbounded_channel::<DataEvent>();
    let sup = WsSupervisor::spawn(sdk, tx);

    sup.subscribe(trade_spec("005930", RowKind::Trade, "S3_"));
    assert!(wait_subscribe(&ws, "S3_", "3", Duration::from_secs(3)).await);

    // No frame pushed → after a short age, the subscription is flagged.
    sleep(Duration::from_millis(150)).await;
    let stale = sup.never_delivered(Duration::from_millis(100));
    assert!(
        stale.contains(&"S3_:005930".to_string()),
        "a silent subscription is surfaced, got {stale:?}"
    );

    // Once a frame is delivered, it is no longer never-delivered.
    ws.push_s3(
        "005930",
        serde_json::json!({ "price": "60500", "cvolume": "1", "shcode": "005930" }),
    );
    assert!(
        wait_until(|| sup.never_delivered(Duration::from_millis(0)).is_empty(), Duration::from_secs(2)).await,
        "delivery clears the diagnostic"
    );
    sup.shutdown();
}

/// KOSDAQ symbols route to K3_ (the `LsDataClient` routing decision).
#[tokio::test]
async fn kosdaq_symbol_routes_to_k3() {
    let http = MockServer::start().await;
    let ws = MockWsServer::start().await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, _rx) = mpsc::unbounded_channel::<DataEvent>();
    let sup = WsSupervisor::spawn(sdk.clone(), tx);

    let kosdaq_id = InstrumentId::from("086520.XKRX");
    let mut market_map = HashMap::new();
    market_map.insert(kosdaq_id, Market::Kosdaq);
    let mut client = LsDataClient::with_supervisor("LS-KRX", sdk, market_map, sup);

    client
        .subscribe_trades(SubscribeTrades::new(
            kosdaq_id,
            None,
            Some(nautilus_model::identifiers::Venue::from(nautilus_ls::KRX_VENUE)),
            UUID4::new(),
            UnixNanos::default(),
            None,
            None,
        ))
        .unwrap();

    assert!(
        wait_subscribe(&ws, "K3_", "3", Duration::from_secs(3)).await,
        "a KOSDAQ symbol subscribed on the K3_ lane"
    );
    let _ = client.stop();
}

// ---------------------------------------------------------------------------
// AE4 — terminal reconnect-budget error → rebuild + resubscribe. Uses a
// restartable fixed-port WS mock so the SDK's budget exhausts (server down) and
// then the supervisor's rebuild reconnects (server back up on the same port).
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
                            _ = krx.recv() => break, // sever the socket on stop
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

    /// Sever active connections + stop accepting + free the port (for a same-port
    /// restart). Closing the active socket gives the SDK an EOF, so its reconnect
    /// loop runs and — with the port now unbound — exhausts its budget.
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

#[tokio::test]
async fn reconnect_budget_exhaustion_rebuilds_and_resubscribes() {
    let port = free_port().await;
    let http = MockServer::start().await;
    let ws = FixedPortWs::start(port).await;
    let sdk = sdk_over_ws(&http, ws.ws_url()).await;
    let (tx, _rx) = mpsc::unbounded_channel::<DataEvent>();
    let sup = WsSupervisor::spawn(sdk, tx);

    sup.subscribe(trade_spec("005930", RowKind::Trade, "S3_"));
    // First subscribe frame lands + connected.
    let saw_first = {
        let start = tokio::time::Instant::now();
        let mut ok = false;
        while start.elapsed() < Duration::from_secs(3) {
            if ws.count_subscribe("S3_", "3").await >= 1 {
                ok = true;
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }
        ok
    };
    assert!(saw_first, "initial subscribe reached the server");
    assert!(sup.is_connected());

    // Take the server down: the SDK exhausts its 4-attempt reconnect budget
    // (linear 1+2+3+4s backoff) and delivers the terminal error, purging the sub.
    ws.stop();
    assert!(
        wait_until(|| !sup.is_connected(), Duration::from_secs(20)).await,
        "supervisor observes the terminal error and goes disconnected"
    );

    // Bring the server back on the SAME port; the supervisor's rebuild loop
    // reconnects and resubscribes — the node does not end.
    let ws2 = FixedPortWs::start(port).await;
    assert!(
        wait_until(|| sup.is_connected(), Duration::from_secs(20)).await,
        "supervisor rebuilds the session and reconnects"
    );
    let saw_resub = {
        let start = tokio::time::Instant::now();
        let mut ok = false;
        while start.elapsed() < Duration::from_secs(5) {
            if ws2.count_subscribe("S3_", "3").await >= 1 {
                ok = true;
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }
        ok
    };
    assert!(saw_resub, "the active subscription was re-established on the new session");
    sup.shutdown();
    ws2.stop();
}
