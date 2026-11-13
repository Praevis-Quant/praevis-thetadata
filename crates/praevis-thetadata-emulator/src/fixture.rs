//! Scripted answers for the client's tests and benchmarks (moved from its
//! `tests/support`, ADR-0022): synthetic tables, a one-shot login peer, and
//! a [`Script`] that replaces the model's answer.
//!
//! Column names and data are not a captured or asserted vendor schema.
use crate::{Config, Emulator, Item, Responses};
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Session};
use praevis_thetadata_core::{Price, Table, Value};
use praevis_thetadata_proto::beta_endpoints as api;
use praevis_thetadata_proto::endpoints::{self as wire, data_value::DataType};
use prost::Message;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

pub use crate::response;

pub const TOKEN: &str = "synthetic-eod-session";
pub const EMAIL: &str = "fixture@example.invalid";

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Mixed,
    Nulls,
    Timestamps,
    Prices,
}

/// `rows` rows of eight synthetic columns in `shape`, with the table the
/// decoders must produce from them.
pub fn table(shape: Shape, rows: usize) -> (wire::DataTable, Table) {
    let headers: Vec<_> = (0..8).map(|n| format!("synthetic_{n}")).collect();
    let mut encoded = Vec::with_capacity(rows);
    let mut expected = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut cells = Vec::with_capacity(8);
        let mut values = Vec::with_capacity(8);
        for col in 0..8 {
            let kind = match shape {
                Shape::Mixed => col,
                Shape::Nulls => 0,
                Shape::Timestamps => 5,
                Shape::Prices => 3,
            };
            let (cell, value) = match kind {
                0 => (Some(DataType::NullValue(0)), Value::Null),
                1 => (
                    Some(DataType::Text("SYNTHETIC".into())),
                    Value::Text("SYNTHETIC".into()),
                ),
                2 => (
                    Some(DataType::Number(i64::MAX - row as i64)),
                    Value::Integer(i64::MAX - row as i64),
                ),
                3 => (
                    Some(DataType::Price(wire::Price {
                        value: -12345,
                        r#type: 8,
                    })),
                    Value::Price(Price {
                        mantissa: -12345,
                        exponent: -2,
                    }),
                ),
                4 => (
                    Some(DataType::Boolean(row % 2 == 0)),
                    Value::Boolean(row % 2 == 0),
                ),
                5 => {
                    let zone = (row % 2) as i32;
                    // Independent fixed oracle: no production timestamp formatter.
                    let text = if zone == 0 {
                        "1969-12-31T19:00:00.000-05:00"
                    } else {
                        "1970-01-01T00:00:00.000Z"
                    };
                    (
                        Some(DataType::Timestamp(wire::ZonedDateTime {
                            epoch_ms: 0,
                            zone,
                            ..Default::default()
                        })),
                        Value::Timestamp(text.into()),
                    )
                }
                6 => (
                    Some(DataType::Number(i64::MIN + row as i64)),
                    Value::Integer(i64::MIN + row as i64),
                ),
                _ => (None, Value::Null),
            };
            cells.push(wire::DataValue { data_type: cell });
            values.push(value);
        }
        encoded.push(wire::DataValueList { values: cells });
        expected.push(values);
    }
    (
        wire::DataTable {
            headers: headers.clone(),
            data_table: encoded,
        },
        Table {
            headers,
            rows: expected,
        },
    )
}

pub fn query() -> api::StockHistoryEodRequestQuery {
    api::StockHistoryEodRequestQuery {
        symbol: "SYNTHETIC".into(),
        start_date: "2026-01-15".into(),
        end_date: "2026-01-16".into(),
    }
}

/// Obtain a real Session via public auth against a one-shot synthetic HTTP peer.
/// Never reads credentials/environment, calls ThetaData, or touches native storage.
pub async fn session() -> Session {
    session_with_auth_counter().await.0
}

/// Like [`session`], but the auth listener stays up for the rest of the test and counts
/// every connection it accepts (EOD-L3-005 injected counter). The first is the login;
/// any later connection means something tried to authenticate again.
pub async fn session_with_auth_counter() -> (Session, Arc<std::sync::atomic::AtomicUsize>) {
    session_with_user(&format!(r#"{{"email":"{EMAIL}"}}"#)).await
}

/// Like [`session_with_auth_counter`], with this `user` object in the login
/// response (for example its subscription fields).
pub async fn session_with_user(user: &str) -> (Session, Arc<std::sync::atomic::AtomicUsize>) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let user = user.to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/auth", listener.local_addr().unwrap());
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = connections.clone();
    let (first_done, first_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        counter.fetch_add(1, Ordering::SeqCst);
        let mut request = Vec::new();
        let mut buffer = [0; 2048];
        loop {
            let count = socket.read(&mut buffer).await.unwrap();
            assert_ne!(count, 0, "auth request ended early");
            request.extend_from_slice(&buffer[..count]);
            assert!(request.len() < 16384, "unexpected fixture request size");
            if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let body = format!(r#"{{"sessionId":"{TOKEN}","user":{user}}}"#);
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(reply.as_bytes()).await.unwrap();
        let _ = first_done.send(());
        // Keep counting any later authentication attempt; never answer it.
        while let Ok((socket, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            drop(socket);
        }
    });
    let client = AuthClient::new(
        AuthConfig::default()
            .with_auth_url(endpoint)
            .with_allow_insecure(true),
    )
    .unwrap();
    let result = client
        .authenticate(&Credentials::api_key("synthetic-key"))
        .await
        .unwrap();
    first_done_rx.await.unwrap();
    (result, connections)
}

/// Answers any RPC other than stock EOD: the RPC name and the encoded request
/// in, the messages to stream (or an initial status) out.
pub type Handler =
    Arc<dyn Fn(&str, Vec<u8>) -> Result<Vec<wire::ResponseData>, Status> + Send + Sync>;

/// What the stock EOD RPC answers, and how. Every other RPC goes to
/// `other`, or is `UNIMPLEMENTED`.
#[derive(Clone, Default)]
pub struct Script {
    /// Every other RPC; without it they are `UNIMPLEMENTED`.
    pub other: Option<Handler>,
    pub messages: Vec<Result<wire::ResponseData, Status>>,
    pub message_delay: Duration,
    pub initial_error: Option<Status>,
    /// With `initial_error`: fail only the first this many requests (0: all).
    pub fail_first: usize,
    pub header_delay: Duration,
    /// With `header_delay`: delay only these requests (1-based, in order of
    /// arrival); empty delays every request.
    pub delay_requests: Vec<usize>,
    pub pause_before: Option<(usize, Duration)>,
}

/// Answers one request from `script`.
pub(crate) async fn serve(
    script: &Script,
    rpc: &str,
    request: Vec<u8>,
    timeout: Option<String>,
    requests: &Mutex<Vec<api::StockHistoryEodRequest>>,
    timeouts: &Mutex<Vec<Option<String>>>,
) -> Result<tonic::Response<Responses>, Status> {
    if rpc != "GetStockHistoryEod" {
        let Some(handler) = script.other.clone() else {
            return Err(Status::unimplemented("synthetic EOD fixture only"));
        };
        let messages = handler(rpc, request)?;
        return Ok(stream(
            messages.into_iter().map(Ok).collect(),
            None,
            Duration::ZERO,
        ));
    }
    timeouts.lock().unwrap().push(timeout);
    let request = api::StockHistoryEodRequest::decode(request.as_slice())
        .map_err(|_| Status::invalid_argument("synthetic fixture: undecodable request"))?;
    let calls = {
        let mut requests = requests.lock().unwrap();
        requests.push(request);
        requests.len()
    };
    if !script.header_delay.is_zero()
        && (script.delay_requests.is_empty() || script.delay_requests.contains(&calls))
    {
        tokio::time::sleep(script.header_delay).await;
    }
    if let Some(error) = &script.initial_error
        && (script.fail_first == 0 || calls <= script.fail_first)
    {
        return Err(error.clone());
    }
    Ok(stream(
        script.messages.clone(),
        script.pause_before,
        script.message_delay,
    ))
}

/// Streams `messages`, waiting `delay` before each and pausing before one.
fn stream(
    messages: Vec<Item>,
    pause_before: Option<(usize, Duration)>,
    delay: Duration,
) -> tonic::Response<Responses> {
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        for (index, item) in messages.into_iter().enumerate() {
            if let Some((before, pause)) = pause_before
                && index == before
            {
                tokio::time::sleep(pause).await;
            }
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if tx.send(item).await.is_err() {
                break;
            }
        }
    });
    tonic::Response::new(ReceiverStream::new(rx))
}

/// A scripted emulator, shaped as the client's former fixture.
pub struct Fixture {
    pub endpoint: String,
    pub requests: Arc<Mutex<Vec<api::StockHistoryEodRequest>>>,
    /// Raw `grpc-timeout` metadata per request, as the server received it.
    pub timeouts: Arc<Mutex<Vec<Option<String>>>>,
    _emulator: Emulator,
}

impl Fixture {
    pub fn start(script: Script) -> Self {
        Self::start_with(script, None)
    }
    /// Serve over TLS with the given PEM certificate chain and private key,
    /// generated by the test itself. No key material is committed.
    pub fn start_tls(script: Script, cert_pem: String, key_pem: String) -> Self {
        Self::start_with(script, Some((cert_pem, key_pem)))
    }
    fn start_with(script: Script, tls: Option<(String, String)>) -> Self {
        let emulator = Emulator::start(Config {
            script: Some(script),
            tls,
            ..Config::default()
        });
        Self {
            endpoint: emulator.endpoint.clone(),
            requests: emulator.eod_requests.clone(),
            timeouts: emulator.timeouts.clone(),
            _emulator: emulator,
        }
    }
}
