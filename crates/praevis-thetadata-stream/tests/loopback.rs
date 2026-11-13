//! The stream against the emulator's one-connection test servers standing
//! in for the Terminal (ADR-0022): requests, event order, counters, the overflow policy and the
//! end of the stream.
use praevis_thetadata_emulator::terminal;
use praevis_thetadata_stream::{
    Contract, Kind, Options, Overflow, Right, Root, Status, Stream, StreamError,
};
use std::sync::mpsc;
use std::time::Duration;

const QUOTE: &str = r#"{"header":{"status":"CONNECTED","type":"QUOTE"},"contract":{"security_type":"OPTION","root":"SPY","expiration":20261106,"strike":570000,"right":"C"},"quote":{"ms_of_day":36000000,"bid_size":10,"bid_exchange":5,"bid":1.25,"bid_condition":50,"ask_size":12,"ask_exchange":5,"ask":1.27,"ask_condition":50,"date":20261106}}"#;
const HEARTBEAT: &str = r#"{"header":{"type":"STATUS","status":"CONNECTED"}}"#;

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
    let (url, requests) = terminal::scripted(frames, 2);
    let stream = Stream::connect(Options::default().with_url(url)).unwrap();
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
    let (url, _) = terminal::scripted(frames.clone(), 0);
    let stream = Stream::connect(
        Options::default()
            .with_url(url)
            .with_capacity(3)
            .with_overflow(Overflow::DropNewest),
    )
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
    let (url, _) = terminal::scripted(frames, 0);
    let stream = Stream::connect(
        Options::default()
            .with_url(url)
            .with_capacity(3)
            .with_overflow(Overflow::Block),
    )
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
            Stream::connect(Options::default().with_url(url.into())),
            Err(StreamError::Url)
        ));
    }
    assert!(matches!(
        Stream::connect(Options::default().with_capacity(0)),
        Err(StreamError::Configuration)
    ));
    // Nothing listening: a connection error, not a hang.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert!(matches!(
        Stream::connect(Options::default().with_url(format!("ws://127.0.0.1:{port}/v1/events"))),
        Err(StreamError::Connect(_))
    ));
}

#[test]
fn the_full_trade_stream_request_and_trade_events() {
    let trade = r#"{"header":{"status":"CONNECTED","type":"TRADE"},"contract":{"security_type":"OPTION","root":"QQQ","expiration":20231110,"strike":360000,"right":"P"},"trade":{"ms_of_day":49531278,"sequence":-563040482,"size":5,"condition":18,"price":1.06,"exchange":65,"date":20231103}}"#;
    let (url, requests) = terminal::scripted(vec![trade.into()], 2);
    let stream = Stream::connect(Options::default().with_url(url)).unwrap();
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

/// Every request the server saw, up to and including "CLOSE".
fn requests_until_close(rx: &mpsc::Receiver<String>) -> Vec<String> {
    let mut seen = Vec::new();
    while let Ok(text) = rx.recv_timeout(Duration::from_secs(5)) {
        let end = text == "CLOSE";
        seen.push(text);
        if end {
            return seen;
        }
    }
    panic!("no close frame; saw {seen:?}");
}

fn subscribe_and_end(close: bool) -> Vec<String> {
    let (url, requests) = terminal::recording();
    let stream = Stream::connect(Options::default().with_url(url)).unwrap();
    let qqq = Contract {
        root: Root::new("QQQ").unwrap(),
        ..spy()
    };
    stream.subscribe_quotes(&spy()).unwrap(); // id 0, still subscribed
    stream.subscribe_quotes(&qqq).unwrap(); // id 1
    stream.unsubscribe_quotes(&qqq).unwrap(); // id 2, so not again on close
    stream.subscribe_full_trades().unwrap(); // id 3, still subscribed
    if close {
        stream.close();
    } else {
        drop(stream);
    }
    requests_until_close(&requests)
}

fn assert_unsubscribed_before_close(seen: &[String]) {
    assert_eq!(seen.len(), 7, "{seen:?}");
    let ending = &seen[4..6];
    let trades = ending
        .iter()
        .filter(|t| t.contains(r#""msg_type":"STREAM_BULK""#) && t.contains(r#""add":false"#))
        .count();
    let spy = ending
        .iter()
        .filter(|t| t.contains(r#""root":"SPY""#) && t.contains(r#""add":false"#))
        .count();
    assert_eq!((trades, spy), (1, 1), "{seen:?}");
    assert!(!ending.iter().any(|t| t.contains("QQQ")), "{seen:?}");
    assert_eq!(seen[6], "CLOSE");
}

#[test]
fn close_unsubscribes_what_is_still_subscribed() {
    assert_unsubscribed_before_close(&subscribe_and_end(true));
}

#[test]
fn drop_unsubscribes_what_is_still_subscribed() {
    assert_unsubscribed_before_close(&subscribe_and_end(false));
}

// The handles' thread-safety, checked at compile time.
const _: () = {
    const fn send_sync<T: Send + Sync>() {}
    const fn send<T: Send>() {}
    const fn clone<T: Clone>() {}
    send_sync::<praevis_thetadata_stream::StreamControl>();
    clone::<praevis_thetadata_stream::StreamControl>();
    send::<praevis_thetadata_stream::StreamEvents>();
    send::<Stream>();
};

#[test]
fn one_thread_subscribes_while_another_waits_for_events() {
    // The server waits for the subscription, then sends a quote.
    let (url, requests) = terminal::scripted(vec![QUOTE.into()], 1);
    let (control, events) = Stream::connect(Options::default().with_url(url))
        .unwrap()
        .split();
    let consumer = std::thread::spawn(move || {
        // Blocks here, before anything is subscribed.
        let event = events.recv_timeout(Duration::from_secs(10));
        (event.map(|e| e.kind), events)
    });
    std::thread::sleep(Duration::from_millis(100));
    let other = control.clone();
    std::thread::spawn(move || other.subscribe_quotes(&spy()).unwrap())
        .join()
        .unwrap();
    let sent = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(sent.contains(r#""add":true"#), "{sent}");
    let (kind, events) = consumer.join().unwrap();
    assert!(matches!(kind, Some(Kind::Quote { .. })), "{kind:?}");
    assert_eq!(control.stats().quotes, 1);
    drop(events);
}

#[test]
fn dropping_the_events_closes_the_stream_and_unsubscribes_first() {
    let (url, requests) = terminal::recording();
    let (control, events) = Stream::connect(Options::default().with_url(url))
        .unwrap()
        .split();
    control.subscribe_quotes(&spy()).unwrap();
    // The consumer goes away while the control handle lives on.
    drop(events);
    let seen = requests_until_close(&requests);
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert!(seen[1].contains(r#""add":false"#) && seen[1].contains("SPY"));
    assert_eq!(seen[2], "CLOSE");
    // The stream is closed for every handle; closing again is harmless.
    assert!(matches!(
        control.subscribe_quotes(&spy()),
        Err(StreamError::Closed)
    ));
    let _ = control.close();
}

#[test]
fn closing_through_a_control_handle_unsubscribes_before_the_close_frame() {
    let (url, requests) = terminal::recording();
    let (control, _events) = Stream::connect(Options::default().with_url(url))
        .unwrap()
        .split();
    control.subscribe_full_trades().unwrap();
    let closer = control.clone();
    std::thread::spawn(move || closer.close()).join().unwrap();
    let seen = requests_until_close(&requests);
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert!(seen[1].contains("STREAM_BULK") && seen[1].contains(r#""add":false"#));
    assert_eq!(seen[2], "CLOSE");
}

#[test]
fn closing_while_data_arrives_finishes_the_close_handshake() {
    // Dropping a socket with data unread resets the connection, and a
    // Terminal can lose the unsubscribes sent just before (live 2026-11-10:
    // the full trade stream kept flowing into the next program).
    for _ in 0..5 {
        let (url, ended) = terminal::busy(QUOTE);
        let stream = Stream::connect(Options::default().with_url(url)).unwrap();
        stream.subscribe_full_trades().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        stream.close();
        let seen = ended.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(
            seen.iter()
                .any(|s| s.contains(r#""req_type":"TRADE","add":false"#)),
            "{seen:?}"
        );
        assert_eq!(seen.last().map(String::as_str), Some("CLOSE"), "{seen:?}");
    }
}
