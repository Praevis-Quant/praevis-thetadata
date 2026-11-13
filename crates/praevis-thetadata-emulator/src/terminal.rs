//! The Terminal side (ADR-0022): its documented WebSocket `/v1/events` and
//! the one REST call this project makes. Nothing from the Terminal JAR's
//! internals, and no FPSS.
//!
//! - [`Terminal`] plays the model: `REQ_RESPONSE` `SUBSCRIBED` for every
//!   stream request (unsubscribes included), quotes for subscribed
//!   contracts, the full trade stream (a quote, an OHLC and a trade per
//!   trade), heartbeats, the close handshake, and REST 200 until an
//!   external login, then 478.
//! - [`scripted`], [`recording`] and [`busy`] are the stream crate's former
//!   one-connection test servers.
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tungstenite::Message;

#[derive(Deserialize)]
pub(crate) struct TerminalModel {
    heartbeat_ms: f64,
    first_event_ms: f64,
    quotes_per_contract_per_second: f64,
    full_trades_per_second: f64,
    full_trade_contracts: u64,
}

/// A running Terminal emulator. Dropping it stops it.
pub struct Terminal {
    /// `ws://127.0.0.1:…/v1/events`.
    pub url: String,
    /// The REST address, `127.0.0.1:…`.
    pub rest: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Terminal {
    /// Starts the WebSocket and REST servers. `logins` counts the account's
    /// logins elsewhere: any login ends the Terminal's historical session.
    pub(crate) fn start(logins: Arc<AtomicUsize>, time_scale: f64) -> Self {
        let ws = TcpListener::bind("127.0.0.1:0").expect("bind");
        let rest = TcpListener::bind("127.0.0.1:0").expect("bind");
        ws.set_nonblocking(true).expect("nonblocking");
        rest.set_nonblocking(true).expect("nonblocking");
        let url = format!("ws://{}/v1/events", ws.local_addr().expect("address"));
        let rest_address = rest.local_addr().expect("address").to_string();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let threads = vec![
            {
                let (requests, stop) = (requests.clone(), stop.clone());
                std::thread::spawn(move || {
                    accept(&ws, &stop, |tcp| {
                        let (requests, stop) = (requests.clone(), stop.clone());
                        std::thread::spawn(move || serve(tcp, &requests, &stop, time_scale));
                    })
                })
            },
            {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    accept(&rest, &stop, |tcp| answer_rest(tcp, &logins));
                })
            },
        ];
        Self {
            url,
            rest: rest_address,
            requests,
            stop,
            threads,
        }
    }

    /// The stream requests received so far, as sent.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("requests").clone()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// Accepts connections until `stop`, polling a non-blocking listener.
fn accept(listener: &TcpListener, stop: &AtomicBool, mut each: impl FnMut(TcpStream)) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((tcp, _)) => {
                let _ = tcp.set_nonblocking(false);
                each(tcp);
            }
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

/// The one REST call we use (the login-conflict probe's list request):
/// 200 while the Terminal's historical session lives, 478 once another
/// login ended it (`w10-conflict-001`).
fn answer_rest(mut tcp: TcpStream, logins: &AtomicUsize) {
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(5)));
    let mut head = Vec::new();
    let mut buffer = [0; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16_384 {
        match tcp.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buffer[..n]),
        }
    }
    let line = String::from_utf8_lossy(&head);
    let path = line.split_whitespace().nth(1).unwrap_or_default();
    let status = if !path.starts_with("/v3/option/list/expirations") {
        "404 Not Found"
    } else if logins.load(Ordering::SeqCst) > 0 {
        "478 Session Ended"
    } else {
        "200 OK"
    };
    let body = "emulator";
    let _ = write!(
        tcp,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// A stream request, as the stream crate sends it.
#[derive(Deserialize)]
struct Request {
    msg_type: String,
    req_type: String,
    add: bool,
    id: i64,
    contract: Option<serde_json::Value>,
}

fn contract_json(contract: &serde_json::Value) -> String {
    format!(
        r#"{{"security_type":"OPTION","root":{},"expiration":{},"strike":{},"right":{}}}"#,
        contract["root"], contract["expiration"], contract["strike"], contract["right"]
    )
}

fn quote(contract: &str, n: u64) -> String {
    let bid = 1.0 + (n % 100) as f64 / 100.0;
    format!(
        r#"{{"header":{{"status":"CONNECTED","type":"QUOTE"}},"contract":{contract},"quote":{{"ms_of_day":{},"bid_size":10,"bid_exchange":5,"bid":{bid:.2},"bid_condition":50,"ask_size":12,"ask_exchange":5,"ask":{:.2},"ask_condition":50,"date":20240614}}}}"#,
        34_200_000 + n % 23_400_000,
        bid + 0.02
    )
}

fn ohlc(contract: &str, n: u64) -> String {
    format!(
        r#"{{"header":{{"status":"CONNECTED","type":"OHLC"}},"contract":{contract},"ohlc":{{"ms_of_day":{},"open":1.00,"high":1.10,"low":0.95,"close":1.05,"volume":{},"count":{},"date":20240614}}}}"#,
        34_200_000 + n % 23_400_000,
        n + 1,
        n + 1
    )
}

fn trade(contract: &str, n: u64) -> String {
    format!(
        r#"{{"header":{{"status":"CONNECTED","type":"TRADE"}},"contract":{contract},"trade":{{"ms_of_day":{},"sequence":{n},"size":1,"condition":18,"price":1.05,"exchange":65,"date":20240614}}}}"#,
        34_200_000 + n % 23_400_000
    )
}

const HEARTBEAT: &str = r#"{"header":{"type":"STATUS","status":"CONNECTED"}}"#;

/// One client connection, played from the model until it closes.
fn serve(tcp: TcpStream, requests: &Mutex<Vec<String>>, stop: &AtomicBool, scale: f64) {
    let model = &crate::data().model.terminal;
    let Ok(mut socket) = tungstenite::accept(tcp) else {
        return;
    };
    let _ = socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(2)));
    let ms = |recorded: f64| Duration::from_secs_f64((recorded * scale / 1_000.0).max(0.001));
    let mut quotes: Vec<(String, Instant)> = Vec::new();
    let mut full: Option<Instant> = None;
    let mut heartbeat = Instant::now();
    let mut last = Instant::now();
    let (mut owed_quotes, mut owed_trades) = (0.0f64, 0.0f64);
    let mut n: u64 = 0;
    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = socket.close(None);
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                requests.lock().expect("requests").push(text.to_string());
                let Ok(request) = serde_json::from_str::<Request>(text.as_str()) else {
                    continue;
                };
                // Every request is answered SUBSCRIBED, unsubscribes too
                // (observed live for the full trade stream, 2026-11-10).
                let answer = format!(
                    r#"{{"header":{{"type":"REQ_RESPONSE","response":"SUBSCRIBED","req_id":{}}}}}"#,
                    request.id
                );
                if socket.send(Message::text(answer)).is_err() {
                    return;
                }
                let starts = Instant::now() + ms(model.first_event_ms);
                match (request.msg_type.as_str(), request.req_type.as_str()) {
                    ("STREAM", "QUOTE") => {
                        if let Some(contract) = &request.contract {
                            let contract = contract_json(contract);
                            quotes.retain(|(c, _)| *c != contract);
                            if request.add {
                                quotes.push((contract, starts));
                            }
                        }
                    }
                    ("STREAM_BULK", "TRADE") => full = request.add.then_some(starts),
                    _ => {}
                }
            }
            Ok(Message::Close(_)) => {
                // tungstenite queued the reply; flush it and finish.
                let _ = socket.flush();
                while socket.read().is_ok() {}
                return;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
        let now = Instant::now();
        // Rates are real time; only the recorded delays are scaled.
        let elapsed = now.duration_since(last).as_secs_f64();
        last = now;
        let live: Vec<&String> = quotes
            .iter()
            .filter(|(_, at)| now >= *at)
            .map(|(c, _)| c)
            .collect();
        owed_quotes += elapsed * model.quotes_per_contract_per_second * live.len() as f64;
        if full.is_some_and(|at| now >= at) {
            owed_trades += elapsed * model.full_trades_per_second;
        }
        let mut frames = Vec::new();
        while owed_quotes >= 1.0 && !live.is_empty() {
            owed_quotes -= 1.0;
            frames.push(quote(live[(n % live.len() as u64) as usize], n));
            n += 1;
        }
        while owed_trades >= 1.0 {
            owed_trades -= 1.0;
            // A synthetic contract per trade, among a fixed set.
            let k = n % model.full_trade_contracts;
            let contract = format!(
                r#"{{"security_type":"OPTION","root":"EMU","expiration":20240621,"strike":{},"right":"{}"}}"#,
                500_000 + 1_000 * (k / 2),
                if k.is_multiple_of(2) { "C" } else { "P" }
            );
            frames.push(quote(&contract, n));
            frames.push(ohlc(&contract, n));
            frames.push(trade(&contract, n));
            n += 1;
        }
        if now.duration_since(heartbeat) >= ms(model.heartbeat_ms) {
            heartbeat = now;
            frames.push(HEARTBEAT.to_owned());
        }
        for frame in frames {
            if socket.send(Message::text(frame)).is_err() {
                return;
            }
        }
    }
}

/// A one-connection server: it reports the requests it reads, waits for
/// `wait_for` of them, sends `frames`, then closes.
pub fn scripted(frames: Vec<String>, wait_for: usize) -> (String, mpsc::Receiver<String>) {
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

/// A server that records every request until the client's close frame,
/// then reports "CLOSE".
pub fn recording() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/v1/events", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(tcp).unwrap();
        loop {
            match socket.read() {
                Ok(Message::Text(text)) => tx.send(text.as_str().to_owned()).unwrap(),
                Ok(Message::Close(_)) => {
                    tx.send("CLOSE".into()).unwrap();
                    while socket.read().is_ok() {}
                    return;
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });
    (url, rx)
}

/// A server that sends `frame` as fast as it can, 2,000 at a time, and
/// reports the requests it read, and how the connection ended: `CLOSE` for
/// a close frame, `RESET` if a send failed first (the peer reset the
/// connection).
pub fn busy(frame: &'static str) -> (String, mpsc::Receiver<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/v1/events", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(tcp).unwrap();
        socket
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(1)))
            .unwrap();
        let mut seen = Vec::new();
        'serve: loop {
            for _ in 0..2_000 {
                if socket.send(Message::text(frame)).is_err() {
                    seen.push("RESET".to_owned());
                    break 'serve;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
            loop {
                match socket.read() {
                    Ok(Message::Text(text)) => seen.push(text.as_str().to_owned()),
                    Ok(Message::Close(_)) => {
                        // Answer the close frame and finish the handshake.
                        let _ = socket.flush();
                        seen.push("CLOSE".to_owned());
                        break 'serve;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        let _ = tx.send(seen);
    });
    (url, rx)
}
