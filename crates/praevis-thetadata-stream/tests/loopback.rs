//! The stream against a loopback WebSocket server standing in for the
//! Terminal: requests, event order, counters, the overflow policy and the
//! end of the stream.
use praevis_thetadata_stream::{
    Contract, Kind, Options, Overflow, Right, Root, Status, Stream, StreamError,
};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;
use tungstenite::Message;

const QUOTE: &str = r#"{"header":{"status":"CONNECTED","type":"QUOTE"},"contract":{"security_type":"OPTION","root":"SPY","expiration":20261106,"strike":570000,"right":"C"},"quote":{"ms_of_day":36000000,"bid_size":10,"bid_exchange":5,"bid":1.25,"bid_condition":50,"ask_size":12,"ask_exchange":5,"ask":1.27,"ask_condition":50,"date":20261106}}"#;
const HEARTBEAT: &str = r#"{"header":{"type":"STATUS","status":"CONNECTED"}}"#;

/// A one-connection server: it reports the requests it reads, waits for
/// `wait_for` of them, sends `frames`, then closes.
fn server(frames: Vec<String>, wait_for: usize) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/v1/events", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(tcp).unwrap();
        for _ in 0..wait_for {
            if let Ok(Message::Text(text)) = socket.read() {
                tx.send(text.as_str().to_owned()).unwrap();
            }
        }
        for frame in frames {
            socket.send(Message::text(frame)).unwrap();
        }
        let _ = socket.close(None);
        // Let the close handshake finish.
        while socket.read().is_ok() {}
    });
    (url, rx)
}

fn spy() -> Contract {
    Contract {
        root: Root::new("SPY").unwrap(),
        expiration: 20261106,
        strike: 570_000,
        right: Right::Call,
    }
}

#[test]
fn requests_events_and_counters() {
    let frames = vec![
        HEARTBEAT.into(),
        QUOTE.into(),
        "not json".into(),
        HEARTBEAT.into(),
    ];
    let (url, requests) = server(frames, 2);
    let stream = Stream::connect(Options {
        url,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(stream.subscribe_quotes(&spy()).unwrap(), 0);
    assert_eq!(stream.unsubscribe_quotes(&spy()).unwrap(), 1);
    let sent: Vec<String> = (0..2)
        .map(|_| requests.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    assert!(sent[0].contains(r#""add":true,"id":0"#) && sent[0].contains(r#""root":"SPY""#));
    assert!(sent[0].contains(r#""strike":570000"#) && sent[0].contains(r#""right":"C""#));
    assert!(sent[1].contains(r#""add":false,"id":1"#));
    let mut kinds = Vec::new();
    while let Some(event) = stream.recv_timeout(Duration::from_secs(5)) {
        let end = event.kind == Kind::Disconnected;
        kinds.push(event.kind);
        if end {
            break;
        }
    }
    assert_eq!(kinds.len(), 4, "{kinds:?}");
    assert_eq!(
        kinds[0],
        Kind::Heartbeat {
            status: Status::Connected
        }
    );
    let Kind::Quote { contract, quote } = kinds[1] else {
        panic!("{kinds:?}")
    };
    assert_eq!(contract, spy());
    assert_eq!(quote.bid.to_scale_10(), Some(12_500_000_000));
    assert_eq!(kinds[3], Kind::Disconnected);
    let stats = stream.close();
    assert_eq!(
        (
            stats.frames,
            stats.quotes,
            stats.heartbeats,
            stats.unparsed,
            stats.dropped
        ),
        (4, 1, 2, 1, 0)
    );
}

#[test]
fn a_full_queue_drops_the_newest_and_counts_by_default() {
    let frames: Vec<String> = (0..20).map(|_| QUOTE.to_owned()).collect();
    let (url, _) = server(frames.clone(), 0);
    let stream = Stream::connect(Options {
        url,
        capacity: 3,
        overflow: Overflow::DropNewest,
    })
    .unwrap();
    // Read nothing until the reader has seen all 20 (polled: a slow CI
    // runner must not decide the result).
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while stream.stats().quotes < 20 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let stats = stream.stats();
    assert_eq!(stats.quotes, 20);
    assert_eq!(stats.dropped, 17);
    let mut received = 0;
    while let Some(event) = stream.recv_timeout(Duration::from_millis(500)) {
        if matches!(event.kind, Kind::Quote { .. }) {
            received += 1;
        }
    }
    assert_eq!(received, 3);
    // Block (tests only) delivers every event.
    let (url, _) = server(frames, 0);
    let stream = Stream::connect(Options {
        url,
        capacity: 3,
        overflow: Overflow::Block,
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut received = 0;
    while let Some(event) = stream.recv_timeout(Duration::from_secs(2)) {
        if matches!(event.kind, Kind::Quote { .. }) {
            received += 1;
        }
    }
    assert_eq!((received, stream.stats().dropped), (20, 0));
}

#[test]
fn only_a_local_ws_url_is_accepted() {
    for url in [
        "wss://127.0.0.1:25520/v1/events",
        "ws://example.com:25520/v1/events",
        "http://127.0.0.1:25520/v1/events",
    ] {
        assert!(matches!(
            Stream::connect(Options {
                url: url.into(),
                ..Default::default()
            }),
            Err(StreamError::Url)
        ));
    }
    assert!(matches!(
        Stream::connect(Options {
            capacity: 0,
            ..Default::default()
        }),
        Err(StreamError::Configuration)
    ));
    // Nothing listening: a connection error, not a hang.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert!(matches!(
        Stream::connect(Options {
            url: format!("ws://127.0.0.1:{port}/v1/events"),
            ..Default::default()
        }),
        Err(StreamError::Connect(_))
    ));
}

#[test]
fn the_full_trade_stream_request_and_trade_events() {
    let trade = r#"{"header":{"status":"CONNECTED","type":"TRADE"},"contract":{"security_type":"OPTION","root":"QQQ","expiration":20231110,"strike":360000,"right":"P"},"trade":{"ms_of_day":49531278,"sequence":-563040482,"size":5,"condition":18,"price":1.06,"exchange":65,"date":20231103}}"#;
    let (url, requests) = server(vec![trade.into()], 2);
    let stream = Stream::connect(Options {
        url,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(stream.subscribe_full_trades().unwrap(), 0);
    assert_eq!(stream.unsubscribe_full_trades().unwrap(), 1);
    let sent: Vec<String> = (0..2)
        .map(|_| requests.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    assert_eq!(
        sent[0],
        r#"{"msg_type":"STREAM_BULK","sec_type":"OPTION","req_type":"TRADE","add":true,"id":0}"#
    );
    assert!(sent[1].contains(r#""add":false,"id":1"#));
    let event = stream.recv_timeout(Duration::from_secs(5)).unwrap();
    let Kind::Trade { contract, trade } = event.kind else {
        panic!("{:?}", event.kind)
    };
    assert_eq!((contract.root.as_str(), trade.size), ("QQQ", 5));
    assert_eq!(stream.close().trades, 1);
}
