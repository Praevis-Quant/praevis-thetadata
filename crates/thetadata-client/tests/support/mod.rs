//! Synthetic, loopback-only fixtures shared by tests and the release benchmark.
//! Column names/data are not a captured or asserted vendor EOD schema.
use prost::Message;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use thetadata_client::{AuthClient, AuthConfig, Credentials, Session, queries as api};
use thetadata_core::{Price, Table, Value};
use thetadata_proto::endpoints::{self as wire, data_value::DataType};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot},
};
use tokio_stream::{
    StreamExt,
    wrappers::{ReceiverStream, TcpListenerStream},
};

pub const TOKEN: &str = "synthetic-eod-session";
pub const EMAIL: &str = "fixture@example.invalid";

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Mixed,
    Nulls,
    Timestamps,
    Prices,
}

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

pub fn response(table: &wire::DataTable, zstd: bool) -> wire::ResponseData {
    let bytes = table.encode_to_vec();
    wire::ResponseData {
        original_size: bytes.len().try_into().unwrap(),
        compressed_data: if zstd {
            zstd::stream::encode_all(bytes.as_slice(), 1).unwrap()
        } else {
            bytes
        },
        compression_description: Some(wire::CompressionDescription {
            algo: i32::from(zstd),
            level: if zstd { 1 } else { 0 },
        }),
        flat_file_manifest: None,
    }
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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/auth", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
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
        let body = format!(r#"{{"sessionId":"{TOKEN}","user":{{"email":"{EMAIL}"}}}}"#);
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(reply.as_bytes()).await.unwrap();
    });
    let client = AuthClient::new(AuthConfig {
        auth_url: endpoint,
        allow_insecure: true,
        ..Default::default()
    })
    .unwrap();
    let result = client
        .authenticate(&Credentials::api_key("synthetic-key"))
        .await
        .unwrap();
    task.await.unwrap();
    result
}

#[derive(Clone, Default)]
pub struct Script {
    pub messages: Vec<Result<wire::ResponseData, tonic::Status>>,
    pub message_delay: Duration,
    pub initial_error: Option<tonic::Status>,
}

type FixtureStream = ReceiverStream<Result<wire::ResponseData, tonic::Status>>;
struct FixtureService {
    script: Script,
    requests: Arc<Mutex<Vec<api::StockHistoryEodRequest>>>,
}
impl FixtureService {
    async fn eod(
        &self,
        request: tonic::Request<api::StockHistoryEodRequest>,
    ) -> Result<tonic::Response<FixtureStream>, tonic::Status> {
        self.requests.lock().unwrap().push(request.into_inner());
        if let Some(error) = &self.script.initial_error {
            return Err(error.clone());
        }
        let script = self.script.clone();
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            for item in script.messages {
                if !script.message_delay.is_zero() {
                    tokio::time::sleep(script.message_delay).await;
                }
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }
}
include!(concat!(env!("OUT_DIR"), "/eod_fixture_service.rs"));

pub struct Fixture {
    pub endpoint: String,
    pub requests: Arc<Mutex<Vec<api::StockHistoryEodRequest>>>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Fixture {
    pub fn start(script: Script) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let service = FixtureService {
            script,
            requests: requests.clone(),
        };
        let (shutdown, rx) = oneshot::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                // Custom incoming streams bypass tonic's TCP listener options.
                // Avoid measuring Linux delayed ACK/Nagle interaction as decode
                // latency for small synthetic messages.
                let incoming = TcpListenerStream::new(TcpListener::from_std(listener).unwrap())
                    .map(|socket| {
                        socket.and_then(|socket| {
                            socket.set_nodelay(true)?;
                            Ok(socket)
                        })
                    });
                let server = tonic::transport::Server::builder()
                    .add_service(
                        api::beta_theta_terminal_server::BetaThetaTerminalServer::new(service)
                            .max_encoding_message_size(65 * 1024 * 1024),
                    )
                    .serve_with_incoming(incoming);
                tokio::select! { result = server => result.unwrap(), _ = rx => {} }
            });
        });
        Self {
            endpoint,
            requests,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
