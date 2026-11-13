//! The real stream crate against the emulated Terminal: answers to stream
//! requests, quotes, the full trade stream, heartbeats, the close
//! handshake and the REST 478 (ADR-0022).
use praevis_thetadata_emulator::{Config, Emulator};
use praevis_thetadata_stream::{Contract, Kind, Options, Response, Right, Root, Stream};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn config() -> Config {
    // The recorded 1 s first-event delay and heartbeat, in milliseconds.
    Config {
        time_scale: 0.01,
        ..Config::default()
    }
}

fn spy() -> Contract {
    Contract {
        root: Root::new("SPY").unwrap(),
        expiration: 20240621,
        strike: 542_000,
        right: Right::Call,
    }
}

/// Events until `until` holds or 5 s pass.
fn drain(stream: &Stream, mut until: impl FnMut(&Kind) -> bool) -> Vec<Kind> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut kinds = Vec::new();
    while Instant::now() < deadline {
        if let Some(event) = stream.recv_timeout(Duration::from_millis(50)) {
            let done = until(&event.kind);
            kinds.push(event.kind);
            if done {
                break;
            }
        }
    }
    kinds
}

#[test]
fn quotes_flow_for_a_subscribed_contract_and_every_request_is_answered() {
    let emulator = Emulator::start(config());
    let stream =
        Stream::connect(Options::default().with_url(emulator.terminal.url.clone())).unwrap();
    let id = stream.subscribe_quotes(&spy()).unwrap();
    let mut quotes = 0;
    let kinds = drain(&stream, |kind| {
        if matches!(kind, Kind::Quote { contract, .. } if *contract == spy()) {
            quotes += 1;
        }
        quotes >= 20
    });
    assert!(kinds.contains(&Kind::Response {
        id: id as i64,
        response: Response::Subscribed
    }));
    assert!(kinds.iter().any(|k| matches!(k, Kind::Heartbeat { .. })));
    assert_eq!(quotes, 20);
    let id = stream.unsubscribe_quotes(&spy()).unwrap();
    let kinds = drain(&stream, |kind| matches!(kind, Kind::Response { .. }));
    assert_eq!(
        kinds.last(),
        Some(&Kind::Response {
            id: id as i64,
            response: Response::Subscribed
        })
    );
    let stats = stream.close();
    assert_eq!((stats.dropped, stats.unparsed), (0, 0));
    let sent = emulator.terminal.requests();
    assert_eq!(sent.len(), 2);
    assert!(sent[1].contains(r#""add":false"#));
}

#[test]
fn the_full_trade_stream_carries_a_quote_and_an_ohlc_per_trade() {
    let emulator = Emulator::start(config());
    let stream =
        Stream::connect(Options::default().with_url(emulator.terminal.url.clone())).unwrap();
    stream.subscribe_full_trades().unwrap();
    let mut trades = 0;
    drain(&stream, |kind| {
        if matches!(kind, Kind::Trade { .. }) {
            trades += 1;
        }
        trades >= 50
    });
    let stats = stream.close();
    assert!(stats.trades >= 50, "{stats:?}");
    assert!(
        stats.quotes >= stats.trades && stats.others >= stats.trades,
        "{stats:?}"
    );
    assert_eq!((stats.dropped, stats.unparsed), (0, 0));
    // Closing unsubscribed first, through the close handshake.
    let sent = emulator.terminal.requests();
    assert!(sent.last().unwrap().contains(r#""add":false"#), "{sent:?}");
}

fn rest_status(address: &str) -> String {
    let mut tcp = std::net::TcpStream::connect(address).unwrap();
    write!(
        tcp,
        "GET /v3/option/list/expirations?symbol=SPY HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut head = String::new();
    tcp.read_to_string(&mut head).unwrap();
    head.split_whitespace().nth(1).unwrap().to_owned()
}

#[tokio::test]
async fn another_login_turns_the_terminal_rest_to_478() {
    let emulator = Emulator::start(config());
    assert_eq!(rest_status(&emulator.terminal.rest), "200");
    praevis_thetadata_client::AuthClient::new(
        praevis_thetadata_client::AuthConfig::default()
            .with_auth_url(emulator.auth_url.clone())
            .with_allow_insecure(true),
    )
    .unwrap()
    .authenticate(&praevis_thetadata_client::Credentials::api_key(
        "emulator-key",
    ))
    .await
    .unwrap();
    assert_eq!(rest_status(&emulator.terminal.rest), "478");
}
