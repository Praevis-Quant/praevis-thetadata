//! A dev-only emulator of ThetaData's documented interfaces, for tests
//! (ADR-0022). Never published.
//!
//! It serves, on 127.0.0.1 only:
//! - **gRPC:** all 100 RPCs of the checked-in descriptor;
//! - **the HTTP login,** with the profile's subscription fields.
//!
//! **Driven by repository data, never by market data:**
//! - column names and kinds from `docs/endpoints/schemas.json`, with
//!   synthetic values;
//! - refusals from `docs/endpoints/status.json`, for the chosen profile;
//! - everything else from `model.json`, each entry with its evidence.
//!
//! An RPC with no recorded schema, or never probed on the profile, is
//! answered `UNIMPLEMENTED` naming the evidence gap. The emulator never
//! invents a schema.
use praevis_thetadata_proto::beta_endpoints as api;
use praevis_thetadata_proto::endpoints::{self as wire, data_value::DataType};
use prost::Message;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::{Code, Status};

include!(concat!(env!("OUT_DIR"), "/rpcs.rs"));

const SCHEMAS: &str = include_str!("../../../docs/endpoints/schemas.json");
const STATUS: &str = include_str!("../../../docs/endpoints/status.json");
const MODEL: &str = include_str!("../model.json");

/// The session id the emulator issues. Synthetic; it grants nothing anywhere.
pub const SESSION: &str = "emulator-session";
/// The account e-mail in the login answer. A reserved, invalid domain.
pub const EMAIL: &str = "emulator@example.invalid";

/// Whose account the emulator plays: its subscription fields and the
/// refusals recorded for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Every product FREE (`status.json` column `free`).
    Free,
    /// Options PRO, stocks and indices FREE (`status.json` column `pro`).
    OptionsPro,
}

impl Profile {
    fn key(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::OptionsPro => "options-pro",
        }
    }
    /// `free` or `options-pro`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "free" => Some(Self::Free),
            "options-pro" => Some(Self::OptionsPro),
            _ => None,
        }
    }
}

/// A scripted fault that replaces the model's answer for every RPC: the
/// scenarios of the former `loopback_server` example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Two messages of two rows, then `INTERNAL`.
    Partial,
    /// Ten messages of three rows, 100 ms apart.
    Slow,
    /// `RESOURCE_EXHAUSTED` at open.
    Busy,
    /// The first answer waits 1.5 s, then one message of two rows.
    Delayed,
}

impl Fault {
    /// `partial`, `slow`, `busy` or `delayed`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "partial" => Some(Self::Partial),
            "slow" => Some(Self::Slow),
            "busy" => Some(Self::Busy),
            "delayed" => Some(Self::Delayed),
            _ => None,
        }
    }
}

/// What to emulate.
#[derive(Clone, Debug)]
pub struct Config {
    pub profile: Profile,
    pub fault: Option<Fault>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Profile::OptionsPro,
            fault: None,
        }
    }
}

#[derive(Deserialize)]
struct Schema {
    columns: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct Observation {
    class: String,
    code: String,
    run: String,
    tier: Option<String>,
}

#[derive(Deserialize)]
struct ProfileModel {
    status: String,
    user: serde_json::Value,
}

#[derive(Deserialize)]
struct ResponseModel {
    messages: usize,
    rows_per_message: usize,
}

#[derive(Deserialize)]
struct Model {
    profiles: HashMap<String, ProfileModel>,
    response: ResponseModel,
}

struct Data {
    schemas: BTreeMap<String, Schema>,
    status: BTreeMap<String, HashMap<String, serde_json::Value>>,
    model: Model,
}

fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| Data {
        schemas: serde_json::from_str(SCHEMAS).expect("schemas.json"),
        status: serde_json::from_str(STATUS).expect("status.json"),
        model: serde_json::from_str(MODEL).expect("model.json"),
    })
}

/// The columns recorded for `rpc` in `schemas.json`, as (name, kind).
pub fn schema(rpc: &str) -> Option<Vec<(String, String)>> {
    data().schemas.get(rpc).map(|s| s.columns.clone())
}

/// What the profile's recorded evidence says `rpc` answers: a refusal's
/// status, or `None` when the emulator serves rows.
fn refusal(profile: Profile, rpc: &str) -> Option<Status> {
    let data = data();
    let column = &data.model.profiles[profile.key()].status;
    let observed = data
        .status
        .get(rpc)
        .and_then(|entry| entry.get(column))
        .and_then(|value| serde_json::from_value::<Observation>(value.clone()).ok());
    let Some(observed) = observed else {
        return Some(Status::unimplemented(format!(
            "emulator: evidence gap: {rpc} was never probed on the {} profile",
            profile.key()
        )));
    };
    if observed.class == "entitled" {
        if data.schemas.contains_key(rpc) {
            return None;
        }
        return Some(Status::unimplemented(format!(
            "emulator: evidence gap: {rpc} has no recorded schema ({} was entitled in {})",
            profile.key(),
            observed.run
        )));
    }
    let tier = observed
        .tier
        .map(|tier| format!(", {tier} required"))
        .unwrap_or_default();
    Some(Status::new(
        code(&observed.code),
        format!(
            "emulator: {}{tier} (recorded in {})",
            observed.class, observed.run
        ),
    ))
}

fn code(name: &str) -> Code {
    match name {
        "Cancelled" => Code::Cancelled,
        "InvalidArgument" => Code::InvalidArgument,
        "DeadlineExceeded" => Code::DeadlineExceeded,
        "NotFound" => Code::NotFound,
        "AlreadyExists" => Code::AlreadyExists,
        "PermissionDenied" => Code::PermissionDenied,
        "ResourceExhausted" => Code::ResourceExhausted,
        "FailedPrecondition" => Code::FailedPrecondition,
        "Aborted" => Code::Aborted,
        "OutOfRange" => Code::OutOfRange,
        "Unimplemented" => Code::Unimplemented,
        "Internal" => Code::Internal,
        "Unavailable" => Code::Unavailable,
        "DataLoss" => Code::DataLoss,
        "Unauthenticated" => Code::Unauthenticated,
        _ => Code::Unknown,
    }
}

/// `rows` synthetic rows with `rpc`'s recorded columns, starting at row
/// `first`. Deterministic; the values mean nothing.
pub fn table(columns: &[(String, String)], first: usize, rows: usize) -> wire::DataTable {
    let data_table = (first..first + rows)
        .map(|row| {
            let n = row as i64;
            wire::DataValueList {
                values: columns
                    .iter()
                    .map(|(_, kind)| wire::DataValue {
                        data_type: Some(match kind.as_str() {
                            "number" => DataType::Number(1_000 + n),
                            "price" => DataType::Price(wire::Price {
                                value: 10_000 + row as i32,
                                r#type: 8,
                            }),
                            "boolean" => DataType::Boolean(row % 2 == 0),
                            "timestamp-new-york" | "timestamp-utc" => {
                                DataType::Timestamp(wire::ZonedDateTime {
                                    // 2024-01-02 14:30 UTC, then one minute a row.
                                    epoch_ms: 1_704_205_800_000 + row as u64 * 60_000,
                                    zone: i32::from(kind == "timestamp-utc"),
                                    ..Default::default()
                                })
                            }
                            _ => DataType::Text(format!("EMULATED{row}")),
                        }),
                    })
                    .collect(),
            }
        })
        .collect();
    wire::DataTable {
        headers: columns.iter().map(|(name, _)| name.clone()).collect(),
        data_table,
    }
}

/// One response message, ZSTD-compressed or not (`NONE`).
pub fn response(table: &wire::DataTable, zstd: bool) -> wire::ResponseData {
    let bytes = table.encode_to_vec();
    wire::ResponseData {
        original_size: bytes.len().try_into().expect("message size"),
        compressed_data: if zstd {
            zstd::stream::encode_all(bytes.as_slice(), 1).expect("zstd")
        } else {
            bytes
        },
        compression_description: Some(wire::CompressionDescription {
            algo: i32::from(zstd),
            level: i32::from(zstd),
        }),
        flat_file_manifest: None,
    }
}

/// Field 1 of every request: who is asking.
#[derive(Clone, PartialEq, Message)]
struct Envelope {
    #[prost(message, optional, tag = "1")]
    query_info: Option<api::QueryInfo>,
}

type Item = Result<wire::ResponseData, Status>;
type Responses = ReceiverStream<Item>;

struct Service {
    config: Config,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Service {
    async fn serve(
        &self,
        rpc: &'static str,
        request: Vec<u8>,
    ) -> Result<tonic::Response<Responses>, Status> {
        self.requests.lock().expect("requests").push(rpc.to_owned());
        let session = Envelope::decode(request.as_slice())
            .ok()
            .and_then(|e| e.query_info)
            .and_then(|q| q.auth_token)
            .map(|t| t.session_uuid);
        if session.as_deref() != Some(SESSION) {
            return Err(Status::unauthenticated("emulator: unknown session"));
        }
        let columns = schema(rpc).unwrap_or_else(|| {
            // A fault answers every RPC; without a recorded schema it uses one
            // synthetic column, which is never a claim about the service.
            vec![("emulated".into(), "number".into())]
        });
        let (messages, delay, wait, end): (Vec<Item>, _, _, _) = match self.config.fault {
            Some(Fault::Busy) => return Err(Status::resource_exhausted("emulator: busy")),
            Some(Fault::Partial) => (
                vec![
                    Ok(response(&table(&columns, 0, 2), false)),
                    Ok(response(&table(&columns, 2, 2), true)),
                ],
                Duration::ZERO,
                Duration::ZERO,
                Some(Status::internal("emulator: mid-stream failure")),
            ),
            Some(Fault::Slow) => (
                (0..10)
                    .map(|n| Ok(response(&table(&columns, n * 3, 3), false)))
                    .collect(),
                Duration::from_millis(100),
                Duration::ZERO,
                None,
            ),
            Some(Fault::Delayed) => (
                vec![Ok(response(&table(&columns, 0, 2), false))],
                Duration::ZERO,
                Duration::from_millis(1_500),
                None,
            ),
            None => {
                if let Some(status) = refusal(self.config.profile, rpc) {
                    return Err(status);
                }
                let model = &data().model.response;
                (
                    (0..model.messages)
                        .map(|n| {
                            let rows = model.rows_per_message;
                            // Alternate the framing, so both are always served.
                            Ok(response(&table(&columns, n * rows, rows), n % 2 == 1))
                        })
                        .collect(),
                    Duration::ZERO,
                    Duration::ZERO,
                    None,
                )
            }
        };
        tokio::time::sleep(wait).await;
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            for message in messages.into_iter().chain(end.map(Err)) {
                tokio::time::sleep(delay).await;
                if tx.send(message).await.is_err() {
                    return;
                }
            }
        });
        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }
}

include!(concat!(env!("OUT_DIR"), "/service.rs"));

/// The login answer for the profile.
fn login_body(profile: Profile) -> String {
    let mut user = data().model.profiles[profile.key()].user.clone();
    user["email"] = EMAIL.into();
    serde_json::json!({ "sessionId": SESSION, "user": user }).to_string()
}

/// Answers every login with the profile's session.
async fn auth_server(
    listener: tokio::net::TcpListener,
    profile: Profile,
    logins: Arc<AtomicUsize>,
) {
    while let Ok((mut socket, _)) = listener.accept().await {
        let logins = logins.clone();
        tokio::spawn(async move {
            let mut request = Vec::new();
            let mut buffer = [0; 2048];
            loop {
                let Ok(count) = socket.read(&mut buffer).await else {
                    return;
                };
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
                if request.len() > 16_384 {
                    return;
                }
            }
            logins.fetch_add(1, Ordering::SeqCst);
            let body = login_body(profile);
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
        });
    }
}

/// A running emulator. Dropping it stops it.
pub struct Emulator {
    /// The login URL (plain HTTP on 127.0.0.1).
    pub auth_url: String,
    /// The gRPC endpoint (plain HTTP/2 on 127.0.0.1).
    pub endpoint: String,
    requests: Arc<Mutex<Vec<String>>>,
    logins: Arc<AtomicUsize>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Emulator {
    /// Starts the login and gRPC servers on their own thread and runtime.
    pub fn start(config: Config) -> Self {
        let grpc = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        grpc.set_nonblocking(true).expect("nonblocking");
        let auth = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        auth.set_nonblocking(true).expect("nonblocking");
        let endpoint = format!("http://{}", grpc.local_addr().expect("address"));
        let auth_url = format!("http://{}/auth", auth.local_addr().expect("address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let logins = Arc::new(AtomicUsize::new(0));
        let profile = config.profile;
        let service = Service {
            config,
            requests: requests.clone(),
        };
        let (shutdown, stop) = oneshot::channel();
        let login_count = logins.clone();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(async move {
                let auth = tokio::net::TcpListener::from_std(auth).expect("listener");
                tokio::spawn(auth_server(auth, profile, login_count));
                use tokio_stream::StreamExt;
                // Small messages: no Nagle delay on loopback.
                let incoming = TcpListenerStream::new(
                    tokio::net::TcpListener::from_std(grpc).expect("listener"),
                )
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
                tokio::select! { result = server => result.expect("server"), _ = stop => {} }
            });
        });
        Self {
            auth_url,
            endpoint,
            requests,
            logins,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }

    /// The RPCs requested so far, in order of arrival.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("requests").clone()
    }

    /// The logins answered so far.
    pub fn logins(&self) -> usize {
        self.logins.load(Ordering::SeqCst)
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_descriptor_has_100_rpcs() {
        assert_eq!(RPCS.len(), 100);
    }

    #[test]
    fn every_recorded_schema_and_status_names_a_descriptor_rpc() {
        for rpc in data().schemas.keys().chain(data().status.keys()) {
            assert!(RPCS.contains(&rpc.as_str()), "{rpc}");
        }
    }

    #[test]
    fn every_entitled_rpc_with_a_schema_is_served_and_the_rest_name_their_evidence() {
        for profile in [Profile::Free, Profile::OptionsPro] {
            let mut served = 0;
            for rpc in RPCS {
                match refusal(profile, rpc) {
                    None => {
                        assert!(schema(rpc).is_some(), "{rpc}");
                        served += 1;
                    }
                    Some(status) => {
                        assert!(status.message().starts_with("emulator: "), "{rpc}");
                        if status.code() == Code::Unimplemented {
                            assert!(status.message().contains("evidence gap"), "{rpc}");
                        }
                    }
                }
            }
            assert!(served > 0, "{profile:?}");
        }
    }

    #[test]
    fn tier_refusals_name_the_recorded_tier() {
        let status = refusal(Profile::OptionsPro, "GetStockSnapshotOhlc").unwrap();
        assert_eq!(status.code(), Code::PermissionDenied);
        assert!(
            status.message().contains("VALUE required"),
            "{}",
            status.message()
        );
    }

    #[test]
    fn synthetic_rows_follow_the_recorded_columns_and_kinds() {
        let columns = schema("GetStockHistoryEod").unwrap();
        let table = table(&columns, 0, 3);
        assert_eq!(table.headers.len(), columns.len());
        assert_eq!(table.data_table.len(), 3);
        for row in &table.data_table {
            for (value, (_, kind)) in row.values.iter().zip(&columns) {
                let ok = matches!(
                    (kind.as_str(), &value.data_type),
                    ("number", Some(DataType::Number(_)))
                        | ("price", Some(DataType::Price(_)))
                        | ("text", Some(DataType::Text(_)))
                        | ("timestamp-new-york", Some(DataType::Timestamp(_)))
                );
                assert!(ok, "{kind}");
            }
        }
    }

    #[test]
    fn both_framings_round_trip() {
        let table = table(&schema("GetStockHistoryEod").unwrap(), 0, 2);
        for zstd in [false, true] {
            let message = response(&table, zstd);
            let bytes = if zstd {
                zstd::stream::decode_all(message.compressed_data.as_slice()).unwrap()
            } else {
                message.compressed_data
            };
            assert_eq!(wire::DataTable::decode(bytes.as_slice()).unwrap(), table);
        }
    }
}
