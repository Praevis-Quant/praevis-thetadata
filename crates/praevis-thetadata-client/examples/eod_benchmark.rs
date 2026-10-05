//! Synthetic baseline of the unchanged decoder and public raw EOD stream.
//! No account, Python, native-store access or live endpoint option.
use clap::{Parser, Subcommand, ValueEnum};
use praevis_thetadata_client::{
    BatchValue, DataBatch, EodError, NaiveDate, StockEodRequest, Timestamp,
};
use praevis_thetadata_client::{ClientConfig, Error, ThetaClient};
use praevis_thetadata_core::Table;
use praevis_thetadata_proto::{beta_endpoints as api, endpoints as wire};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinSet;

// Compile the production decoder directly, without adding a benchmark API or
// modifying production control flow. Its source fingerprint is embedded below.
#[path = "benchmark_support/allocation.rs"]
mod allocation;
#[path = "../src/decode.rs"]
mod decode;
// Same binary/harness exercises the retained implementation and the candidate.
// Only the private decoder entry point is included; production streaming is
// always exercised through the public client API.
#[allow(dead_code)]
#[path = "../src/bounded.rs"]
mod bounded;
#[allow(dead_code)]
#[path = "../src/envelope.rs"]
mod envelope;
mod eod {
    pub use praevis_thetadata_client::EodError;
}
#[path = "../tests/support/mod.rs"]
mod support;
#[global_allocator]
static ALLOCATOR: allocation::Allocator = allocation::Allocator;
const LIMIT: usize = 64 * 1024 * 1024;
type Failure = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, ValueEnum)]
enum Engine {
    #[default]
    Legacy,
    Table,
    Numeric,
    TableOffload,
    NumericOffload,
}
#[derive(Debug, PartialEq)]
enum Output {
    Table(Table),
    Numeric(DataBatch),
}
fn expected_output(table: &Table, engine: Engine) -> Output {
    use praevis_thetadata_core::Value;
    if !matches!(engine, Engine::Numeric | Engine::NumericOffload) {
        return Output::Table(table.clone());
    }
    let cells = table
        .rows
        .iter()
        .flatten()
        .map(|v| match v {
            Value::Null => BatchValue::Null,
            Value::Text(v) => BatchValue::Text(v.clone()),
            Value::Integer(v) => BatchValue::Integer(*v),
            Value::Price(v) => BatchValue::Price(v.clone()),
            Value::Boolean(v) => BatchValue::Boolean(*v),
            Value::Timestamp(v) => {
                let instant = chrono::DateTime::parse_from_rfc3339(v).unwrap();
                BatchValue::Timestamp(
                    Timestamp::from_wire(
                        instant.timestamp_millis() as u64,
                        if v.ends_with('Z') { 1 } else { 0 },
                    )
                    .unwrap(),
                )
            }
        })
        .collect();
    Output::Numeric(DataBatch::new(table.headers.clone().into(), cells, table.rows.len()).unwrap())
}
fn decode_output(
    response: praevis_thetadata_proto::endpoints::ResponseData,
    engine: Engine,
) -> Result<Output, Failure> {
    Ok(match engine {
        Engine::Legacy => Output::Table(decode::decode(response, LIMIT)?),
        Engine::Table | Engine::TableOffload => {
            Output::Table(bounded::decode(response, &Default::default(), None, true)?.into_table())
        }
        Engine::Numeric | Engine::NumericOffload => {
            Output::Numeric(bounded::decode(response, &Default::default(), None, false)?)
        }
    })
}
enum Stream {
    Legacy(praevis_thetadata_client::ResponseStream),
    Table(praevis_thetadata_client::EodTableStream),
    Numeric(praevis_thetadata_client::EodBatchStream),
}
impl Stream {
    async fn next(&mut self) -> Result<Option<Output>, Failure> {
        Ok(match self {
            Self::Legacy(v) => v.next_batch().await?.map(Output::Table),
            Self::Table(v) => v.next_batch().await?.map(Output::Table),
            Self::Numeric(v) => v.next_batch().await?.map(Output::Numeric),
        })
    }
}
fn typed_request() -> Result<StockEodRequest, EodError> {
    StockEodRequest::new(
        "SYNTHETIC",
        NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
    )
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Run {
        #[arg(long, value_enum, default_value_t = Engine::Legacy)]
        engine: Engine,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 12, value_parser = clap::value_parser!(u32).range(2..=10000))]
        samples: u32,
        #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=100))]
        warmups: u32,
        #[arg(long, default_value_t = 10000, value_parser = clap::value_parser!(u32).range(1..=100000))]
        rows: u32,
        #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u32).range(1..=16))]
        streams: u32,
        /// Pause after each checked batch while retaining that batch's ownership.
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=1000))]
        consumer_delay_ms: u32,
    },
    Compare {
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Provenance {
    os: String,
    cpu: String,
    logical_cpus: usize,
    host: String,
    rustc: String,
    git_revision: String,
    git_dirty: bool,
    executable_sha256: String,
    decoder_sha256: String,
    fixture_sha256: String,
    harness_sha256: String,
    core_sha256: String,
    client_sha256: String,
    lock_sha256: String,
    descriptor_sha256: String,
    recorded_unix_seconds: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Report {
    engine: Engine,
    format: String,
    synthetic: bool,
    provenance: Provenance,
    samples: u32,
    warmups: u32,
    rows: u32,
    streams: u32,
    consumer_delay_ms: u32,
    batches_per_stream: u32,
    runtime: String,
    measurement_scope: String,
    cases: Vec<Case>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Case {
    name: String,
    fixture_sha256: String,
    rows: usize,
    cells: usize,
    encoded_bytes: usize,
    decompressed_bytes: usize,
    decode_ns: Vec<u64>,
    first_batch_ns: Vec<u64>,
    delivery_ns: Vec<u64>,
    batch_wait_ns: Vec<u64>,
    executor_lateness_ns: Vec<u64>,
    allocation: allocation::Counts,
    summary: serde_json::Value,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn source_hash(bytes: &[u8]) -> String {
    hash(
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .as_bytes(),
    )
}
fn executable_hash(path: &Path) -> Result<String, Failure> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn command(program: &str, args: &[&str]) -> Result<String, Failure> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(format!("{program} failed: {}", output.status).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn provenance() -> Result<Provenance, Failure> {
    let cpu = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| {
        fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|s| s.split_once(':').map(|(_, s)| s.trim().to_string()))
            })
            .unwrap_or_else(|| std::env::consts::ARCH.into())
    });
    Ok(Provenance {
        os: std::env::consts::OS.into(),
        cpu,
        logical_cpus: std::thread::available_parallelism()?.get(),
        host: std::env::var("COMPUTERNAME")
            .or_else(|_| fs::read_to_string("/etc/hostname"))
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .into(),
        rustc: env!("EOD_BENCH_RUSTC").into(),
        git_revision: command("git", &["rev-parse", "HEAD"])?,
        git_dirty: !command("git", &["status", "--porcelain"])?.is_empty(),
        executable_sha256: executable_hash(&std::env::current_exe()?)?,
        decoder_sha256: hash(
            format!(
                "{}{}",
                source_hash(include_bytes!("../src/decode.rs")),
                source_hash(include_bytes!("../src/bounded.rs"))
            )
            .as_bytes(),
        ),
        core_sha256: hash(
            format!(
                "{}{}",
                source_hash(include_bytes!("../../praevis-thetadata-core/src/lib.rs")),
                source_hash(include_bytes!("../../praevis-thetadata-core/src/batch.rs"))
            )
            .as_bytes(),
        ),
        client_sha256: hash(
            format!(
                "{}{}{}{}",
                source_hash(include_bytes!("../src/lib.rs")),
                source_hash(include_bytes!("../src/eod.rs")),
                source_hash(include_bytes!("../src/envelope.rs")),
                source_hash(include_bytes!("../src/framing.rs"))
            )
            .as_bytes(),
        ),
        fixture_sha256: source_hash(include_bytes!("../tests/support/mod.rs")),
        harness_sha256: hash(
            format!(
                "{}{}{}",
                source_hash(include_bytes!("eod_benchmark.rs")),
                source_hash(include_bytes!("benchmark_support/allocation.rs")),
                source_hash(include_bytes!("../build.rs"))
            )
            .as_bytes(),
        ),
        lock_sha256: source_hash(include_bytes!("../../../Cargo.lock")),
        descriptor_sha256: hash(praevis_thetadata_proto::FILE_DESCRIPTOR_SET),
        recorded_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    })
}
fn nanos(duration: Duration) -> u64 {
    duration.as_nanos().try_into().unwrap()
}
fn percentile(values: &[u64], fraction: f64) -> u64 {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[((sorted.len() as f64 * fraction).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1)]
}
fn stats(values: &[u64]) -> serde_json::Value {
    serde_json::json!({"p50_ns": percentile(values, 0.5), "p95_ns": percentile(values, 0.95)})
}

struct Delivery {
    first: u64,
    total: u64,
    waits: Vec<u64>,
    lateness: u64,
}
async fn delivery(
    client: &ThetaClient,
    expected: Arc<Output>,
    streams: u32,
    engine: Engine,
    consumer_delay_ms: u32,
) -> Result<Delivery, Failure> {
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let monitor = tokio::spawn(async move {
        let mut due = tokio::time::Instant::now() + Duration::from_millis(1);
        let mut max_lateness = 0;
        loop {
            tokio::time::sleep_until(due).await;
            max_lateness = max_lateness.max(nanos(
                tokio::time::Instant::now().saturating_duration_since(due),
            ));
            if stop.load(Ordering::Relaxed) {
                return max_lateness;
            }
            due = tokio::time::Instant::now() + Duration::from_millis(1);
        }
    });
    tokio::task::yield_now().await;
    let start = Instant::now();
    let mut tasks = JoinSet::new();
    for _ in 0..streams {
        let client = client.clone();
        let expected = expected.clone();
        tasks.spawn(async move {
            let mut stream = match engine {
                Engine::Legacy => Stream::Legacy(client.stock_history_eod(support::query()).await?),
                Engine::Table | Engine::TableOffload => {
                    Stream::Table(client.stock_eod(typed_request()?).await?)
                }
                Engine::Numeric | Engine::NumericOffload => {
                    Stream::Numeric(client.stock_eod_batches(typed_request()?).await?)
                }
            };
            let mut first = None;
            let mut waits = Vec::with_capacity(3);
            let mut count = 0;
            loop {
                let waiting = Instant::now();
                let Some(batch) = stream.next().await? else {
                    break;
                };
                let elapsed = nanos(waiting.elapsed());
                if first.is_none() {
                    first = Some(nanos(start.elapsed()));
                }
                waits.push(elapsed);
                // Total delivery and timer delay include this exact-value
                // consumer. Per-batch wait ends before equality/output drop.
                assert_eq!(&batch, expected.as_ref());
                black_box(&batch);
                if consumer_delay_ms != 0 {
                    tokio::time::sleep(Duration::from_millis(consumer_delay_ms.into())).await;
                    black_box(&batch); // Retain ownership throughout the caller pause.
                }
                count += 1;
            }
            assert_eq!(count, 3);
            Ok::<_, Failure>((first.unwrap(), waits))
        });
    }
    let mut first = u64::MAX;
    let mut waits = Vec::new();
    let mut failure = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok((latency, batch_waits))) => {
                first = first.min(latency);
                waits.extend(batch_waits);
            }
            Ok(Err(error)) => {
                failure = Some(error.to_string());
            }
            Err(error) => {
                failure = Some(error.to_string());
            }
        }
    }
    let total = nanos(start.elapsed());
    stopped.store(true, Ordering::Relaxed);
    let lateness = monitor.await?;
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(Delivery {
        first,
        total,
        waits,
        lateness,
    })
}

fn run(
    output: &Path,
    samples: u32,
    warmups: u32,
    rows: u32,
    streams: u32,
    engine: Engine,
    consumer_delay_ms: u32,
) -> Result<(), Failure> {
    if cfg!(debug_assertions) {
        return Err("benchmark requires a --release build".into());
    }
    let provenance = provenance()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut cases = Vec::new();
    for (name, shape, count) in [
        ("small-mixed", support::Shape::Mixed, 16),
        ("large-mixed", support::Shape::Mixed, rows as usize),
        ("nulls", support::Shape::Nulls, rows as usize),
        ("timestamps", support::Shape::Timestamps, rows as usize),
        ("prices", support::Shape::Prices, rows as usize),
    ] {
        let (wire, expected) = support::table(shape, count);
        let expected = Arc::new(expected_output(&expected, engine));
        for compressed in [false, true] {
            let response = support::response(&wire, compressed);
            let name = format!("{name}-{}", if compressed { "zstd" } else { "none" });
            eprintln!("Measuring {name}");
            let mut decode_ns = Vec::new();
            for iteration in 0..warmups + samples {
                // Prepare owned input before timing. Generation/compression and
                // equality/drop of output are outside decode_ns.
                let input = response.clone();
                let start = Instant::now();
                let table = decode_output(black_box(input), engine)?;
                let elapsed = nanos(start.elapsed());
                assert_eq!(&table, expected.as_ref());
                black_box(&table);
                if iteration >= warmups {
                    decode_ns.push(elapsed);
                }
            }
            // Include the clone in this separate allocation probe so every freed
            // allocation originated in scope. No server/tasks run on this thread.
            let (table, allocation) =
                allocation::probe(|| decode_output(response.clone(), engine).unwrap());
            assert_eq!(&table, expected.as_ref());
            drop(table);
            let fixture = support::Fixture::start(support::Script {
                messages: vec![Ok(response.clone()); 3],
                ..Default::default()
            });
            let client = runtime.block_on(async {
                ThetaClient::with_eod_policy(
                    ClientConfig {
                        endpoint: Some(fixture.endpoint.clone()),
                        allow_insecure: true,
                        ..Default::default()
                    },
                    support::session().await,
                    praevis_thetadata_client::EodPolicy {
                        inline_bytes: if matches!(
                            engine,
                            Engine::TableOffload | Engine::NumericOffload
                        ) {
                            0
                        } else {
                            4096
                        },
                        ..Default::default()
                    },
                )
                .await
            })?;
            let mut first_batch_ns = Vec::new();
            let mut delivery_ns = Vec::new();
            let mut batch_wait_ns = Vec::new();
            let mut executor_lateness_ns = Vec::new();
            for iteration in 0..warmups + samples {
                let sample = runtime.block_on(delivery(
                    &client,
                    expected.clone(),
                    streams,
                    engine,
                    consumer_delay_ms,
                ))?;
                if iteration >= warmups {
                    first_batch_ns.push(sample.first);
                    delivery_ns.push(sample.total);
                    batch_wait_ns.extend(sample.waits);
                    executor_lateness_ns.push(sample.lateness);
                }
            }
            assert_eq!(
                fixture.requests.lock().unwrap().len(),
                ((warmups + samples) * streams) as usize
            );
            let seconds = percentile(&decode_ns, 0.5) as f64 / 1e9;
            let delivery_seconds = percentile(&delivery_ns, 0.5) as f64 / 1e9;
            let summary = serde_json::json!({
                "decode": stats(&decode_ns), "first_batch": stats(&first_batch_ns), "delivery": stats(&delivery_ns),
                "batch_wait": stats(&batch_wait_ns), "executor_lateness": stats(&executor_lateness_ns),
                "decode_rows_per_second": count as f64 / seconds,
                "decode_cells_per_second": (count * 8) as f64 / seconds,
                "decode_wire_bytes_per_second": response.compressed_data.len() as f64 / seconds,
                "delivery_rows_per_second": (count * 3 * streams as usize) as f64 / delivery_seconds,
                "requested_bytes_per_cell": allocation.requested_bytes as f64 / (count * 8) as f64,
            });
            cases.push(Case {
                name,
                fixture_sha256: hash(&response.encode_to_vec()),
                rows: count,
                cells: count * 8,
                encoded_bytes: response.compressed_data.len(),
                decompressed_bytes: wire.encoded_len(),
                decode_ns,
                first_batch_ns,
                delivery_ns,
                batch_wait_ns,
                executor_lateness_ns,
                allocation,
                summary,
            });
            drop(client);
            drop(fixture);
        }
    }
    write_json(output, &Report { engine, format: "thetadata-eod-baseline-v3".into(), synthetic: true, provenance, samples, warmups, rows, streams, consumer_delay_ms, batches_per_stream: 3,
        runtime: "one current-thread consumer runtime; separate one-thread loopback server runtime".into(),
        measurement_scope: "decode includes decompression/protobuf/conversion/validation; excludes input clone and output check/drop. Delivery includes loopback serving, transport, decoding, full equality consumer, configured pauses retaining one batch per stream, and output drop; excludes connection/auth/setup. Timed allocator calls retain an inactive thread-local probe check. Separate allocation probe includes input clone and requested Rust heap only, excludes C ZSTD allocator/RSS. Timer is maximum 1ms sleep lateness per delivery sample, including timer granularity and equality consumer.".into(), cases })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Failure> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    // Preserve previous raw evidence instead of silently overwriting it.
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

fn compatible(a: &Report, b: &Report) -> Result<(), Failure> {
    for report in [a, b] {
        if !(2..=10000).contains(&report.samples)
            || !(1..=100).contains(&report.warmups)
            || !(1..=16).contains(&report.streams)
            || report.batches_per_stream != 3
            || report.consumer_delay_ms > 1000
        {
            return Err("invalid sampling configuration".into());
        }
    }
    if a.format != "thetadata-eod-baseline-v3"
        || a.format != b.format
        || !a.synthetic
        || !b.synthetic
        || a.provenance.os != b.provenance.os
        || a.provenance.cpu != b.provenance.cpu
        || a.provenance.host != b.provenance.host
        || a.provenance.logical_cpus != b.provenance.logical_cpus
        || a.provenance.rustc != b.provenance.rustc
        || a.provenance.harness_sha256 != b.provenance.harness_sha256
        || a.provenance.fixture_sha256 != b.provenance.fixture_sha256
        || a.provenance.descriptor_sha256 != b.provenance.descriptor_sha256
        || a.provenance.lock_sha256 != b.provenance.lock_sha256
        || a.samples != b.samples
        || a.warmups != b.warmups
        || a.rows != b.rows
        || a.streams != b.streams
        || a.consumer_delay_ms != b.consumer_delay_ms
        || a.runtime != b.runtime
        || a.measurement_scope != b.measurement_scope
        || a.batches_per_stream != b.batches_per_stream
        || a.cases.len() != b.cases.len()
        || a.cases.is_empty()
    {
        return Err("incomparable host/toolchain/fixtures/harness/configuration; rerun both revisions under identical conditions".into());
    }
    for (x, y) in a.cases.iter().zip(&b.cases) {
        if x.name != y.name
            || x.fixture_sha256 != y.fixture_sha256
            || x.rows != y.rows
            || x.cells != y.cells
            || x.encoded_bytes != y.encoded_bytes
            || x.decompressed_bytes != y.decompressed_bytes
        {
            return Err("fixture identity/shape mismatch".into());
        }
        for (report, case) in [(a, x), (b, y)] {
            for values in [&case.decode_ns, &case.delivery_ns, &case.first_batch_ns] {
                if values.len() != report.samples as usize || values.contains(&0) {
                    return Err("missing/invalid timing samples".into());
                }
            }
            if case.executor_lateness_ns.len() != report.samples as usize {
                return Err("missing timer observations".into());
            }
            if case.batch_wait_ns.len()
                != (report.samples * report.streams * report.batches_per_stream) as usize
            {
                return Err("missing batch samples".into());
            }
        }
    }
    Ok(())
}
fn compare(baseline: &Path, candidate: &Path, output: &Path) -> Result<(), Failure> {
    let a: Report = serde_json::from_slice(&fs::read(baseline)?)?;
    let b: Report = serde_json::from_slice(&fs::read(candidate)?)?;
    compatible(&a, &b)?;
    let mut comparisons = Vec::new();
    for (a, b) in a.cases.iter().zip(&b.cases) {
        let baseline_ns = percentile(&a.decode_ns, 0.5) as f64;
        let candidate_ns = percentile(&b.decode_ns, 0.5) as f64;
        comparisons.push(
            serde_json::json!({"case": a.name, "decode_median_speedup": baseline_ns / candidate_ns,
            "decode_median_change_percent": 100.0 * (candidate_ns / baseline_ns - 1.0),
            "baseline_summary": a.summary, "candidate_summary": b.summary,
            "baseline_allocation": a.allocation, "candidate_allocation": b.allocation}),
        );
    }
    write_json(
        output,
        &serde_json::json!({"format": "thetadata-eod-comparison-v3", "baseline_engine": a.engine, "candidate_engine": b.engine, "baseline_report_sha256": hash(&fs::read(baseline)?), "candidate_report_sha256": hash(&fs::read(candidate)?), "baseline": a.provenance, "candidate": b.provenance, "interpretation": "descriptive comparison, not a calibrated regression gate; numeric output bypasses formatting, table output preserves it; repeat in AB/BA order", "cases": comparisons}),
    )
}
fn main() -> Result<(), Failure> {
    match Args::parse().command {
        Mode::Run {
            engine,
            output,
            samples,
            warmups,
            rows,
            streams,
            consumer_delay_ms,
        } => run(
            &output,
            samples,
            warmups,
            rows,
            streams,
            engine,
            consumer_delay_ms,
        ),
        Mode::Compare {
            baseline,
            candidate,
            output,
        } => compare(&baseline, &candidate, &output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            engine: Engine::Legacy,
            format: "thetadata-eod-baseline-v3".into(),
            synthetic: true,
            provenance: Provenance::default(),
            samples: 2,
            warmups: 1,
            rows: 1,
            streams: 1,
            consumer_delay_ms: 0,
            batches_per_stream: 3,
            runtime: "fixture".into(),
            measurement_scope: "fixture".into(),
            cases: vec![Case {
                name: "fixture".into(),
                fixture_sha256: "same".into(),
                rows: 1,
                cells: 8,
                encoded_bytes: 12,
                decompressed_bytes: 12,
                decode_ns: vec![10, 20],
                first_batch_ns: vec![30, 40],
                delivery_ns: vec![90, 100],
                batch_wait_ns: vec![20; 6],
                executor_lateness_ns: vec![2, 3],
                allocation: Default::default(),
                summary: serde_json::json!({}),
            }],
        }
    }

    #[test]
    fn comparison_rejects_incomparable_or_incomplete_evidence() {
        let baseline = report();
        assert!(compatible(&baseline, &baseline).is_ok());
        let mutations: [fn(&mut Report); 9] = [
            |r| r.consumer_delay_ms = 1,
            |r| r.provenance.host = "another-host".into(),
            |r| r.provenance.rustc = "another-toolchain".into(),
            |r| r.provenance.harness_sha256 = "different-harness".into(),
            |r| r.cases[0].fixture_sha256 = "different-fixture".into(),
            |r| {
                r.cases[0].decode_ns.pop();
            },
            |r| r.cases[0].delivery_ns[0] = 0,
            |r| r.streams = u32::MAX,
            |r| r.cases.clear(),
        ];
        for mutate in mutations {
            let mut candidate = report();
            mutate(&mut candidate);
            assert!(compatible(&baseline, &candidate).is_err());
        }
        let mut changed_code = report();
        changed_code.provenance.decoder_sha256 = "optimized-code".into();
        changed_code.cases[0].executor_lateness_ns[0] = 0;
        assert!(compatible(&baseline, &changed_code).is_ok());
    }

    #[test]
    fn percentiles_use_nearest_rank_and_handle_single_observation() {
        assert_eq!(percentile(&[30, 10, 20, 40], 0.5), 20);
        assert_eq!(percentile(&[30, 10, 20, 40], 0.95), 40);
        assert_eq!(percentile(&[7], 0.95), 7);
    }

    #[test]
    fn allocation_probe_tracks_reallocation_peak_and_returned_ownership() {
        let (buffer, counts) = allocation::probe(|| {
            let mut buffer = Vec::<u8>::with_capacity(1000);
            buffer.resize(1000, 1);
            buffer.reserve_exact(2000);
            black_box(buffer)
        });
        assert!(counts.allocation_calls >= 2);
        assert!(counts.requested_bytes >= 4000);
        assert_eq!(counts.live_bytes, buffer.capacity());
        assert_eq!(counts.peak_live_bytes, buffer.capacity());
        drop(buffer);
        let (_, counts) = allocation::probe(|| ());
        assert_eq!(counts.live_bytes, 0);
    }

    #[test]
    fn short_string_capacity_is_bounded_and_rejection_precedes_materialization() {
        use wire::{DataTable, DataValue, DataValueList, data_value::DataType};
        let wire = DataTable {
            headers: vec!["h".into()],
            data_table: vec![
                DataValueList {
                    values: vec![DataValue {
                        data_type: Some(DataType::Text("x".into()))
                    }]
                };
                1000
            ],
        };
        let response = support::response(&wire, false);
        for table in [false, true] {
            let (_, counts) = allocation::probe(|| {
                let batch =
                    bounded::decode(response.clone(), &Default::default(), None, table).unwrap();
                if table {
                    drop(batch.into_table());
                } else {
                    drop(batch);
                }
            });
            let limits = bounded::DecodeLimits {
                allocated_bytes: counts.peak_live_bytes - 1,
                ..Default::default()
            };
            let (result, rejected) =
                allocation::probe(|| bounded::decode(response.clone(), &limits, None, table));
            assert_eq!(result.unwrap_err(), EodError::Resource("allocation bytes"));
            assert_eq!(
                rejected.allocation_calls, 1,
                "only the owned input clone may allocate"
            );
            assert!(!rejected.invalid_scope);
        }
    }

    #[test]
    fn manifest_rejection_allocates_only_the_owned_input_frame() {
        let mut bytes = vec![34];
        prost::encoding::encode_varint(20000, &mut bytes);
        bytes.extend(std::iter::repeat_n([10, 0], 10000).flatten());
        let (result, counts) =
            allocation::probe(|| envelope::parse(bytes.clone(), &Default::default()));
        assert_eq!(result.unwrap_err(), EodError::Unsupported);
        assert_eq!(counts.allocation_calls, 1);
        assert_eq!(counts.peak_live_bytes, bytes.len());
        assert!(!counts.invalid_scope);
    }
}
