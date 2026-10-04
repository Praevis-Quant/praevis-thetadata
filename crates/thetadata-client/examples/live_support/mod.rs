use crate::fixture::{self, Fixture, Script};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use thetadata_client::{
    ClientConfig, EodPolicy, NaiveDate, Session, StockEodRequest, Table, ThetaClient, Value,
    queries as api,
};
use thetadata_proto::endpoints::ResponseData;

const MAX_FRAME: usize = 8 * 1024 * 1024;
const MAX_TOTAL: usize = 32 * 1024 * 1024;
const MAX_BATCHES: usize = 16;
const FORMAT: &str = "thetadata-private-eod-capture-v1";
const MANIFEST: &str = include_str!("../../../thetadata-proto/schema/manifest.json");
const DESCRIPTOR: &[u8] = include_bytes!("../../../thetadata-proto/schema/thetadata.bin");
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Query {
    pub symbol: String,
    pub start: String,
    pub end: String,
}
impl Query {
    pub fn typed(&self) -> Result<StockEodRequest> {
        let start = NaiveDate::parse_from_str(&self.start, "%Y-%m-%d").map_err(|_| "query")?;
        let end = NaiveDate::parse_from_str(&self.end, "%Y-%m-%d").map_err(|_| "query")?;
        if start.format("%Y-%m-%d").to_string() != self.start
            || end.format("%Y-%m-%d").to_string() != self.end
            || (end - start).num_days() > 31
            || self.symbol.len() > 128
        {
            return Err("query exceeds manual-run scope");
        }
        StockEodRequest::new(&self.symbol, start, end).map_err(|_| "query")
    }
    fn wire(&self) -> api::StockHistoryEodRequestQuery {
        api::StockHistoryEodRequestQuery {
            symbol: self.symbol.clone(),
            start_date: self.start.clone(),
            end_date: self.end.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Frame {
    bytes: usize,
    sha256: String,
    original_size: i32,
    compression: Option<i32>,
}

#[derive(Serialize, Deserialize)]
pub struct Capture {
    format: String,
    captured_at: String,
    environment: String,
    query: Query,
    source_commit: String,
    source_dirty: bool,
    executable_sha256: String,
    compiled_source_sha256: String,
    rustc: String,
    platform: String,
    upstream_version: String,
    descriptor_sha256: String,
    protocol_manifest_sha256: String,
    frames: Vec<Frame>,
    // Safe terminal category, never remote status text/metadata.
    terminal: String,
    pub verification: String,
    pub auth_http_status: Option<u16>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn compiled_source_hash() -> String {
    let mut digest = Sha256::new();
    // Compile-time inputs distinguish a stale executable from the runtime checkout.
    for source in [
        include_bytes!("../live_eod.rs").as_slice(),
        include_bytes!("mod.rs").as_slice(),
        include_bytes!("../../src/lib.rs").as_slice(),
        include_bytes!("../../src/eod.rs").as_slice(),
        include_bytes!("../../src/bounded.rs").as_slice(),
        include_bytes!("../../src/framing.rs").as_slice(),
        include_bytes!("../../src/envelope.rs").as_slice(),
        include_bytes!("../../src/decode.rs").as_slice(),
        include_bytes!("../../tests/support/mod.rs").as_slice(),
        include_bytes!("../../build.rs").as_slice(),
        include_bytes!("../../../thetadata-core/src/lib.rs").as_slice(),
        include_bytes!("../../../thetadata-core/src/batch.rs").as_slice(),
        include_bytes!("../../../thetadata-auth/src/lib.rs").as_slice(),
        include_bytes!("../../../thetadata-auth/src/store.rs").as_slice(),
        include_bytes!("../../../../Cargo.lock").as_slice(),
        DESCRIPTOR,
    ] {
        digest.update((source.len() as u64).to_le_bytes());
        digest.update(source);
    }
    format!("{:x}", digest.finalize())
}

impl Capture {
    pub fn new(query: Query, environment: &str) -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let git = |args: &[&str]| -> Result<Vec<u8>> {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .map_err(|_| "source provenance")?;
            if !output.status.success() {
                return Err("source provenance");
            }
            Ok(output.stdout)
        };
        let source_commit = String::from_utf8(git(&["rev-parse", "HEAD"])?)
            .map_err(|_| "source provenance")?
            .trim()
            .to_owned();
        let mut executable =
            File::open(std::env::current_exe().map_err(|_| "executable provenance")?)
                .map_err(|_| "executable provenance")?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let count = executable
                .read(&mut buffer)
                .map_err(|_| "executable provenance")?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        let manifest: serde_json::Value =
            serde_json::from_str(MANIFEST).map_err(|_| "protocol provenance")?;
        Ok(Self {
            format: FORMAT.into(),
            captured_at: chrono::Utc::now().to_rfc3339(),
            environment: environment.into(),
            query,
            source_commit,
            source_dirty: !git(&["status", "--porcelain"])?.is_empty(),
            executable_sha256: format!("{:x}", digest.finalize()),
            compiled_source_sha256: compiled_source_hash(),
            rustc: env!("EOD_BENCH_RUSTC").into(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            upstream_version: manifest["source_version"]
                .as_str()
                .ok_or("protocol provenance")?
                .into(),
            descriptor_sha256: hash(DESCRIPTOR),
            protocol_manifest_sha256: hash(MANIFEST.as_bytes()),
            frames: Vec::new(),
            terminal: "not_started".into(),
            verification: "not_completed".into(),
            auth_http_status: None,
        })
    }
    pub fn save(&self, directory: &Path) -> Result<()> {
        write_report(directory, "manifest.json", self)
    }
    pub fn load(directory: &Path) -> Result<Self> {
        let capture: Self =
            serde_json::from_slice(&read_limited(&directory.join("manifest.json"), 65536)?)
                .map_err(|_| "capture manifest")?;
        if capture.format != FORMAT
            || capture.descriptor_sha256 != hash(DESCRIPTOR)
            || !matches!(capture.environment.as_str(), "PROD" | "STAGE")
        {
            return Err("capture format or protocol mismatch");
        }
        capture.query.typed()?;
        Ok(capture)
    }
    fn responses(&self, directory: &Path) -> Result<Vec<ResponseData>> {
        if self.terminal != "complete" {
            return Err("capture is incomplete; retained for failure research");
        }
        if self.frames.len() > MAX_BATCHES {
            return Err("capture batch limit");
        }
        let mut total = 0usize;
        self.frames
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let bytes =
                    read_limited(&directory.join(format!("batch-{index:03}.pb")), MAX_FRAME)?;
                total += bytes.len();
                if total > MAX_TOTAL || bytes.len() != frame.bytes || hash(&bytes) != frame.sha256 {
                    return Err("capture length or hash mismatch");
                }
                let response =
                    ResponseData::decode(bytes.as_slice()).map_err(|_| "capture protobuf")?;
                if response.flat_file_manifest.is_some() {
                    return Err("flat-file capture unsupported");
                }
                Ok(response)
            })
            .collect()
    }
}

pub fn run_directory(run_id: &str) -> Result<PathBuf> {
    if run_id.is_empty()
        || run_id.len() > 80
        || !run_id
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || v == b'-' || v == b'_')
    {
        return Err("run ID must contain 1-80 ASCII letters, digits, hyphens or underscores");
    }
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/live")
        .join(run_id))
}
pub fn create_directory(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory.parent().ok_or("artifact directory")?)
        .map_err(|_| "artifact directory")?;
    let mut builder = fs::DirBuilder::new();
    // Also used on Windows; mutable builder configuration is platform dependent.
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(directory)
        .map_err(|_| "run directory must be new and writable")
}
pub fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "file read")?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "file read")?;
    if bytes.len() > limit {
        return Err("file size limit");
    }
    Ok(bytes)
}
fn write_private(path: &Path, bytes: &[u8], new: bool) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    if new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|_| "artifact write")?
        .write_all(bytes)
        .map_err(|_| "artifact write")
}
pub fn write_report(directory: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "report encoding")?;
    write_private(&directory.join(name), &bytes, name != "manifest.json")
}
fn config(endpoint: Option<String>) -> ClientConfig {
    ClientConfig {
        allow_insecure: endpoint.is_some(), // Only our freshly bound loopback fixture supplies this.
        endpoint,
        request_timeout: Duration::from_secs(120),
        idle_timeout: Duration::from_secs(30),
        max_batch_bytes: MAX_FRAME,
        ..Default::default()
    }
}

pub async fn capture(evidence: &mut Capture, directory: &Path, session: Session) -> Result<()> {
    let client = ThetaClient::with_eod_policy(config(None), session, EodPolicy::default())
        .await
        .map_err(|_| "connection")?;
    capture_from_client(evidence, directory, &client).await
}

async fn capture_from_client(
    evidence: &mut Capture,
    directory: &Path,
    client: &ThetaClient,
) -> Result<()> {
    evidence.terminal = "in_progress".into();
    evidence.save(directory)?;
    let operation = async {
        let mut request = tonic::Request::new(api::StockHistoryEodRequest {
            query_info: Some(client.query_info()),
            params: Some(evidence.query.wire()),
        });
        request.set_timeout(Duration::from_secs(120));
        let mut raw = client.raw_client().max_decoding_message_size(MAX_FRAME);
        let mut stream = match raw.get_stock_history_eod(request).await {
            Ok(response) => response.into_inner(),
            Err(status) => {
                evidence.terminal = format!("status_headers_{}", status.code() as i32);
                return Err("gRPC status");
            }
        };
        let mut total = 0;
        loop {
            let response =
                match tokio::time::timeout(Duration::from_secs(30), stream.message()).await {
                    Err(_) => return Err("idle timeout"),
                    Ok(Err(status)) => {
                        // Tonic may also produce local transport/size statuses: do not assert remote provenance.
                        evidence.terminal = format!("status_stream_{}", status.code() as i32);
                        return Err("gRPC status");
                    }
                    Ok(Ok(None)) => break,
                    Ok(Ok(Some(response))) => response,
                };
            if response.flat_file_manifest.is_some() {
                return Err("flat-file response excluded from capture");
            }
            let bytes = response.encode_to_vec();
            total += bytes.len();
            if bytes.len() > MAX_FRAME || total > MAX_TOTAL || evidence.frames.len() >= MAX_BATCHES
            {
                return Err("capture limit");
            }
            let index = evidence.frames.len();
            write_private(
                &directory.join(format!("batch-{index:03}.pb")),
                &bytes,
                true,
            )?;
            evidence.frames.push(Frame {
                bytes: bytes.len(),
                sha256: hash(&bytes),
                original_size: response.original_size,
                compression: response
                    .compression_description
                    .as_ref()
                    .map(|value| value.algo),
            });
            evidence.save(directory)?;
        }
        Ok(())
    };
    let result = tokio::time::timeout(Duration::from_secs(120), operation)
        .await
        .unwrap_or(Err("query deadline"));
    if evidence.terminal == "in_progress" {
        evidence.terminal = match result {
            Ok(()) => "complete",
            Err(reason) => reason,
        }
        .into();
    }
    evidence.save(directory)?;
    result
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Summary {
    headers: Option<Vec<String>>,
    rows: usize,
    batches: usize,
    value_kinds: BTreeMap<String, usize>,
    semantic_sha256: String,
}
#[derive(Default)]
struct Accumulator {
    headers: Option<Vec<String>>,
    rows: usize,
    batches: usize,
    kinds: BTreeMap<String, usize>,
    digest: Sha256,
}
impl Accumulator {
    fn push(&mut self, table: Table) -> Result<()> {
        table.validate().map_err(|_| "invalid table")?;
        self.batches += 1;
        if self.batches > MAX_BATCHES {
            return Err("comparison batch limit");
        }
        if !table.headers.is_empty() {
            if let Some(headers) = &self.headers {
                if *headers != table.headers {
                    return Err("schema drift");
                }
            } else {
                self.digest.update(b"headers:");
                self.digest
                    .update(serde_json::to_vec(&table.headers).map_err(|_| "summary encoding")?);
                self.digest.update(b"\n");
                self.headers = Some(table.headers);
            }
        }
        for row in table.rows {
            self.rows += 1;
            self.digest.update(b"row:");
            self.digest
                .update(serde_json::to_vec(&row).map_err(|_| "summary encoding")?);
            self.digest.update(b"\n");
            for value in row {
                let kind = match value {
                    Value::Null => "null",
                    Value::Text(_) => "text",
                    Value::Integer(_) => "integer",
                    Value::Price(_) => "price",
                    Value::Boolean(_) => "boolean",
                    Value::Timestamp(_) => "timestamp",
                };
                *self.kinds.entry(kind.into()).or_default() += 1;
            }
        }
        Ok(())
    }
    fn finish(self) -> Summary {
        Summary {
            headers: self.headers,
            rows: self.rows,
            batches: self.batches,
            value_kinds: self.kinds,
            semantic_sha256: format!("{:x}", self.digest.finalize()),
        }
    }
}
impl Summary {
    fn same_values(&self, other: &Self) -> bool {
        self.headers == other.headers
            && self.rows == other.rows
            && self.value_kinds == other.value_kinds
            && self.semantic_sha256 == other.semantic_sha256
    }
}
async fn numeric(client: &ThetaClient, query: &Query) -> Result<Summary> {
    let mut stream = client
        .stock_eod_batches(query.typed()?)
        .await
        .map_err(|_| "numeric query")?;
    let mut result = Accumulator::default();
    while let Some(batch) = stream.next_batch().await.map_err(|_| "numeric stream")? {
        result.push(batch.into_table())?;
    }
    Ok(result.finish())
}
async fn table(client: &ThetaClient, query: &Query) -> Result<Summary> {
    let mut stream = client
        .stock_eod(query.typed()?)
        .await
        .map_err(|_| "Table query")?;
    let mut result = Accumulator::default();
    while let Some(batch) = stream.next_batch().await.map_err(|_| "Table stream")? {
        result.push(batch)?;
    }
    Ok(result.finish())
}

#[derive(Serialize)]
pub struct Comparison {
    context: &'static str,
    checked_at: String,
    compiled_source_sha256: String,
    capture_frames_sha256: String,
    numeric: Summary,
    table: Summary,
    raw: Option<Summary>,
    pub matches: bool,
}
pub async fn replay(evidence: &Capture, directory: &Path) -> Result<Comparison> {
    let messages = evidence.responses(directory)?.into_iter().map(Ok).collect();
    let fixture = Fixture::start(Script {
        messages,
        ..Default::default()
    });
    let client = ThetaClient::with_eod_policy(
        config(Some(fixture.endpoint.clone())),
        fixture::session().await,
        EodPolicy::default(),
    )
    .await
    .map_err(|_| "replay connection")?;
    // Bounded typed decoding validates captured data before the legacy path runs.
    let numeric = numeric(&client, &evidence.query).await?;
    let table = table(&client, &evidence.query).await?;
    let mut stream = client
        .stock_history_eod(evidence.query.wire())
        .await
        .map_err(|_| "raw replay query")?;
    let mut raw = Accumulator::default();
    while let Some(batch) = stream.next_batch().await.map_err(|_| "raw replay stream")? {
        raw.push(batch)?;
    }
    let raw = raw.finish();
    let matches = numeric.same_values(&table) && numeric.same_values(&raw);
    let report = Comparison {
        context: "loopback replay of captured responses",
        checked_at: chrono::Utc::now().to_rfc3339(),
        compiled_source_sha256: compiled_source_hash(),
        capture_frames_sha256: hash(
            &serde_json::to_vec(&evidence.frames).map_err(|_| "report encoding")?,
        ),
        numeric,
        table,
        raw: Some(raw),
        matches,
    };
    let report_name = format!(
        "replay-{}-{}.json",
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .ok_or("report time")?,
        std::process::id()
    );
    write_report(directory, &report_name, &report)?;
    if !matches {
        return Err("replay interface mismatch");
    }
    Ok(report)
}
pub async fn verify_live(
    evidence: &Capture,
    session: Session,
    reference: &Comparison,
) -> Result<Comparison> {
    let client = ThetaClient::with_eod_policy(config(None), session, EodPolicy::default())
        .await
        .map_err(|_| "typed live connection")?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let numeric = numeric(&client, &evidence.query).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let table = table(&client, &evidence.query).await?;
    let matches = numeric.same_values(&reference.numeric) && table.same_values(&reference.numeric);
    Ok(Comparison {
        context: "separate live queries; service data may change between calls",
        checked_at: chrono::Utc::now().to_rfc3339(),
        compiled_source_sha256: compiled_source_hash(),
        capture_frames_sha256: hash(
            &serde_json::to_vec(&evidence.frames).map_err(|_| "report encoding")?,
        ),
        numeric,
        table,
        raw: None,
        matches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn initial_status_manifest_and_batch_cap_fail_without_false_completion() {
        for scenario in ["headers", "manifest", "limit"] {
            let directory = run_directory(&format!(
                "bounds-test-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ))
            .unwrap();
            create_directory(&directory).unwrap();
            let response = fixture::response(&fixture::table(fixture::Shape::Mixed, 1).0, false);
            let script = match scenario {
                "headers" => Script {
                    initial_error: Some(tonic::Status::unauthenticated("SECRET-HEADER")),
                    ..Default::default()
                },
                "manifest" => Script {
                    messages: vec![Ok(ResponseData {
                        flat_file_manifest: Some(Default::default()),
                        ..response
                    })],
                    ..Default::default()
                },
                _ => Script {
                    messages: vec![Ok(response); MAX_BATCHES + 1],
                    ..Default::default()
                },
            };
            let server = Fixture::start(script);
            let client = ThetaClient::with_eod_policy(
                config(Some(server.endpoint.clone())),
                fixture::session().await,
                EodPolicy::default(),
            )
            .await
            .unwrap();
            let mut evidence = Capture::new(
                Query {
                    symbol: "SYNTHETIC".into(),
                    start: "2024-01-02".into(),
                    end: "2024-01-02".into(),
                },
                "PROD",
            )
            .unwrap();
            assert!(
                capture_from_client(&mut evidence, &directory, &client)
                    .await
                    .is_err()
            );
            assert_eq!(
                evidence.frames.len(),
                if scenario == "limit" { MAX_BATCHES } else { 0 }
            );
            assert_ne!(evidence.terminal, "complete");
            assert!(evidence.responses(&directory).is_err());
            assert!(
                !fs::read_to_string(directory.join("manifest.json"))
                    .unwrap()
                    .contains("SECRET-HEADER")
            );
            assert!(read_limited(&directory.join("manifest.json"), 2).is_err());
        }
    }
    #[tokio::test]
    async fn capture_rpc_records_completion_and_safe_partial_status_without_identity() {
        for fail in [false, true] {
            let directory = run_directory(&format!(
                "capture-test-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ))
            .unwrap();
            create_directory(&directory).unwrap();
            assert!(create_directory(&directory).is_err());
            let mut messages = vec![Ok(fixture::response(
                &fixture::table(fixture::Shape::Mixed, 2).0,
                true,
            ))];
            if fail {
                messages.push(Err(tonic::Status::permission_denied(
                    "SECRET-REMOTE-DETAIL",
                )));
            }
            let server = Fixture::start(Script {
                messages,
                message_delay: Duration::from_millis(10),
                ..Default::default()
            });
            let client = ThetaClient::with_eod_policy(
                config(Some(server.endpoint.clone())),
                fixture::session().await,
                EodPolicy::default(),
            )
            .await
            .unwrap();
            let mut evidence = Capture::new(
                Query {
                    symbol: "SYNTHETIC".into(),
                    start: "2024-01-02".into(),
                    end: "2024-01-02".into(),
                },
                "PROD",
            )
            .unwrap();
            let result = capture_from_client(&mut evidence, &directory, &client).await;
            assert_eq!(result.is_err(), fail);
            assert_eq!(evidence.frames.len(), 1);
            assert_eq!(
                evidence.terminal,
                if fail { "status_stream_7" } else { "complete" }
            );
            let manifest = fs::read_to_string(directory.join("manifest.json")).unwrap();
            for forbidden in [fixture::TOKEN, fixture::EMAIL, "SECRET-REMOTE-DETAIL"] {
                assert!(!manifest.contains(forbidden));
            }
            if !fail {
                assert!(replay(&evidence, &directory).await.unwrap().matches);
            }
            assert_eq!(server.requests.lock().unwrap().len(), 1);
        }
    }
    #[test]
    fn unsafe_paths_and_unbounded_queries_are_rejected() {
        for id in ["", "../escape", "x/y", "x\\y", ".", "secret.txt"] {
            assert!(run_directory(id).is_err());
        }
        assert!(run_directory("prod-eod_001").is_ok());
        for (start, end) in [
            ("2024-01-02", "2024-03-01"),
            ("2024-1-2", "2024-01-03"),
            ("2024-01-03", "2024-01-02"),
        ] {
            assert!(
                Query {
                    symbol: "AAPL".into(),
                    start: start.into(),
                    end: end.into()
                }
                .typed()
                .is_err()
            );
        }
    }
    #[test]
    fn fingerprint_ignores_batch_boundaries_but_preserves_order_and_exact_values() {
        let (_, original) = fixture::table(fixture::Shape::Mixed, 3);
        let mut whole = Accumulator::default();
        whole.push(original.clone()).unwrap();
        let mut split = Accumulator::default();
        for row in original.rows.clone() {
            split
                .push(Table {
                    headers: original.headers.clone(),
                    rows: vec![row],
                })
                .unwrap();
        }
        let whole = whole.finish();
        assert!(whole.same_values(&split.finish()));
        let mut changed = original;
        changed.rows.reverse();
        let mut reversed = Accumulator::default();
        reversed.push(changed).unwrap();
        assert!(!whole.same_values(&reversed.finish()));
    }
    #[tokio::test]
    async fn captured_synthetic_payloads_replay_and_tampering_is_rejected() {
        let directory = run_directory(&format!(
            "test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ))
        .unwrap();
        create_directory(&directory).unwrap();
        let query = Query {
            symbol: "SYNTHETIC".into(),
            start: "2024-01-02".into(),
            end: "2024-01-05".into(),
        };
        let mut evidence = Capture::new(query, "PROD").unwrap();
        for (index, compressed) in [false, true].into_iter().enumerate() {
            let response =
                fixture::response(&fixture::table(fixture::Shape::Mixed, 3).0, compressed);
            let bytes = response.encode_to_vec();
            write_private(
                &directory.join(format!("batch-{index:03}.pb")),
                &bytes,
                true,
            )
            .unwrap();
            evidence.frames.push(Frame {
                bytes: bytes.len(),
                sha256: hash(&bytes),
                original_size: response.original_size,
                compression: response
                    .compression_description
                    .as_ref()
                    .map(|value| value.algo),
            });
        }
        evidence.terminal = "complete".into();
        evidence.save(&directory).unwrap();
        let loaded = Capture::load(&directory).unwrap();
        let report = replay(&loaded, &directory).await.unwrap();
        assert!(report.matches);
        assert_eq!(report.numeric.rows, 6);
        let manifest = fs::read_to_string(directory.join("manifest.json")).unwrap();
        assert!(!manifest.contains(fixture::TOKEN));
        assert!(!manifest.contains(fixture::EMAIL));
        write_private(&directory.join("batch-000.pb"), b"tampered", false).unwrap();
        assert!(loaded.responses(&directory).is_err());
        evidence.terminal = "status_stream_7".into();
        assert!(evidence.responses(&directory).is_err());
        // Retain in ignored artifacts/live; never delete user-owned directories.
    }
}
