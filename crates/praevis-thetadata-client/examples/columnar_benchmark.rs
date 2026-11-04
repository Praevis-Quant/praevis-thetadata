//! CPU per row of the columnar Arrow decoder against the legacy and bounded
//! decoders (P1 PR 3), on synthetic vendor-shaped workloads: one row per
//! message, each an independent ZSTD frame, with the column kinds and counts
//! observed in P0 (W1: 16 columns, W2: 20). Values are synthetic; nothing is
//! captured. Single-threaded and offline: no network, account or service.
//!
//! ```text
//! cargo run --release --locked -p praevis-thetadata-client --example columnar_benchmark -- \
//!   --rows 20000 --samples 7 --output artifacts/columnar-cpu.json
//! ```
//!
//! `--replay <dir>` times the same decoders on private captures instead: the
//! vendor Python baseline's `--save-captures` files (`<workload>-<n>.bin`, each
//! one request's responses as 4-byte big-endian length-prefixed
//! `ResponseData`). It prints timings, counts and ZSTD frame header shapes
//! only, never values. This replay is P1 exit criterion (c)'s measurement.
//!
//! Timings use the system allocator directly; allocation counts come from a
//! separate counted run. `--inputs prebuilt --timing-allocator counting`
//! reproduces the first PR 3 method, which held every sample's input copies
//! at once and counted allocations while timing, for comparison.
use clap::Parser;
use praevis_thetadata_arrow::{ColumnarDecoder, Limits};
use praevis_thetadata_proto::endpoints::{
    self as wire, CompressionDescription, DataTable, DataValue, DataValueList, ResponseData,
    data_value::DataType,
};
use prost::Message;
use serde::Serialize;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering::Relaxed},
    time::Instant,
};

#[path = "benchmark_support/allocation.rs"]
mod allocation;
#[allow(dead_code)]
#[path = "../src/bounded.rs"]
mod bounded;
#[path = "../src/decode.rs"]
mod decode;
// The bounded decoder's unit tests use the envelope parser.
#[allow(dead_code)]
#[path = "../src/envelope.rs"]
mod envelope;
mod eod {
    pub use praevis_thetadata_client::EodError;
}
use praevis_thetadata_client::Error;
// The loopback gRPC fixture shared with the tests (`--loopback`).
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;

/// Routes to the counting allocator only while `COUNTING` is set, so timed
/// runs use the system allocator with no accounting. Both paths allocate
/// from `System`, so memory may be freed on either.
static COUNTING: AtomicBool = AtomicBool::new(false);
struct Switch;
// SAFETY: every call is forwarded unchanged to `System`, directly or through
// the counting allocator, which itself forwards to `System`.
unsafe impl GlobalAlloc for Switch {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Relaxed) {
            unsafe { allocation::Allocator.alloc(layout) }
        } else {
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Relaxed) {
            unsafe { allocation::Allocator.alloc_zeroed(layout) }
        } else {
            unsafe { System.alloc_zeroed(layout) }
        }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if COUNTING.load(Relaxed) {
            unsafe { allocation::Allocator.dealloc(pointer, layout) }
        } else {
            unsafe { System.dealloc(pointer, layout) }
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        if COUNTING.load(Relaxed) {
            unsafe { allocation::Allocator.realloc(pointer, layout, new) }
        } else {
            unsafe { System.realloc(pointer, layout, new) }
        }
    }
}
#[global_allocator]
static ALLOCATOR: Switch = Switch;

/// Allocation calls of one run, counted in isolation.
fn count(run: impl FnOnce() -> usize) -> u64 {
    COUNTING.store(true, Relaxed);
    let (_, counts) = allocation::probe(run);
    COUNTING.store(false, Relaxed);
    counts.allocation_calls
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Inputs {
    /// Clone one sample's inputs just before timing it (as P0's replay did).
    Fresh,
    /// Clone every sample's inputs up front and hold them all.
    Prebuilt,
}
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum TimingAllocator {
    System,
    /// The counting wrapper stays active during timing.
    Counting,
}
const LIMIT: usize = 64 * 1024 * 1024;
/// Rows per Arrow batch (the P1 design's default `batch_rows`).
const BATCH_ROWS: usize = 65_536;

#[derive(Parser)]
#[command(about = "Columnar decoder CPU per row (synthetic, offline)")]
struct Cli {
    /// Messages (one row each) per workload.
    #[arg(long, default_value_t = 20_000)]
    rows: usize,
    /// Timed samples per engine and workload (median, min and max reported).
    #[arg(long, default_value_t = 7)]
    samples: usize,
    #[arg(long, value_enum, default_value_t = Inputs::Fresh)]
    inputs: Inputs,
    #[arg(long, value_enum, default_value_t = TimingAllocator::System)]
    timing_allocator: TimingAllocator,
    /// ZSTD frames of the synthetic workloads.
    #[arg(long, value_enum, default_value_t = Frames::Sized)]
    frames: Frames,
    /// Replay private captures from this directory instead of synthetic data.
    #[arg(long)]
    replay: Option<PathBuf>,
    /// End to end over a loopback gRPC server instead of decoding in memory:
    /// the typed stock EOD stream (bounded numeric) against `query()`
    /// (Arrow). Reports µs/row and time to the first batch.
    #[arg(long)]
    loopback: bool,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Frames {
    /// Content size declared: the window is the message size.
    Sized,
    /// No content size: the decoder must provide the level's 2 MiB window.
    Streaming,
}

#[derive(Serialize)]
struct Report {
    format: &'static str,
    synthetic: bool,
    os: &'static str,
    arch: &'static str,
    rows: usize,
    samples: usize,
    frames: Option<Frames>,
    inputs: Inputs,
    timing_allocator: TimingAllocator,
    results: Vec<Result_>,
}

#[derive(Serialize)]
struct Result_ {
    workload: String,
    engine: &'static str,
    us_per_row_median: f64,
    us_per_row_min: f64,
    us_per_row_max: f64,
    /// Rust heap allocation calls per message over one full run. Owned
    /// engines include one copy of each input message.
    allocations_per_message: f64,
    /// Replay only: messages, rows and the frame header shapes.
    #[serde(skip_serializing_if = "Option::is_none")]
    replay: Option<ReplayShape>,
    /// Loopback only: median time from the request to the first batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    first_batch_ms: Option<f64>,
    /// Replay only: the engine's first decode error, if it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize, Clone, Default)]
struct ReplayShape {
    files: usize,
    messages: usize,
    rows: usize,
    zstd_frames: usize,
    /// Frames declaring their content size (the window can be that small).
    sized_frames: usize,
    single_segment_frames: usize,
    /// Largest window declared by a frame without a single segment.
    max_declared_window: u64,
}

/// ZSTD frame header shape (RFC 8878 §3.1.1.1): content size and window.
fn frame_shape(shape: &mut ReplayShape, message: &ResponseData) {
    let algo = message
        .compression_description
        .as_ref()
        .map_or(0, |d| d.algo);
    let frame = &message.compressed_data;
    if algo != 1 || frame.len() < 6 || frame[..4] != [0x28, 0xb5, 0x2f, 0xfd] {
        return;
    }
    shape.zstd_frames += 1;
    let descriptor = frame[4];
    let single = descriptor & 0x20 != 0;
    if single {
        shape.single_segment_frames += 1;
    }
    if single || descriptor >> 6 != 0 {
        shape.sized_frames += 1;
    }
    if !single {
        let window = frame[5];
        let base = 1u64 << (10 + (window >> 3));
        let size = base + (base / 8) * u64::from(window & 7);
        shape.max_declared_window = shape.max_declared_window.max(size);
    }
}

fn ts(epoch_ms: u64) -> Option<DataType> {
    Some(DataType::Timestamp(wire::ZonedDateTime {
        epoch_ms,
        zone: 0,
        ..Default::default()
    }))
}
fn price(value: i32) -> Option<DataType> {
    Some(DataType::Price(wire::Price { value, r#type: 8 }))
}
fn number(value: i64) -> Option<DataType> {
    Some(DataType::Number(value))
}

/// One row per message. W1: 2 timestamps, 6 prices and 8 integers (16 columns).
/// W2 adds an option contract's symbol, expiration, strike and right (20).
fn workload(columns: usize, rows: usize, frames: Frames) -> Vec<ResponseData> {
    let headers: Vec<String> = (0..columns).map(|c| format!("c{c}")).collect();
    (0..rows)
        .map(|row| {
            let r = row as i64;
            let mut values = Vec::with_capacity(columns);
            if columns == 20 {
                values.push(Some(DataType::Text("SPY".into())));
                values.push(number(20_240_621));
                values.push(price(50_000 + (r % 400) as i32 * 100));
                values.push(Some(DataType::Text(
                    if r % 2 == 0 { "C" } else { "P" }.into(),
                )));
            }
            values.push(ts(1_718_380_800_000 + r as u64));
            values.push(ts(1_718_402_400_000 + r as u64));
            for k in 0..4 {
                values.push(price(18_900 + (r % 97) as i32 + k));
            }
            values.push(number(50_000_000 + r));
            values.push(number(400_000 + r % 1000));
            values.push(number(100 + r % 50));
            values.push(number(r % 70));
            values.push(price(18_950 + (r % 89) as i32));
            values.push(number(r % 20));
            values.push(number(200 + r % 40));
            values.push(number(r % 65));
            values.push(price(18_960 + (r % 83) as i32));
            values.push(number(r % 21));
            let table = DataTable {
                headers: headers.clone(),
                data_table: vec![DataValueList {
                    values: values
                        .into_iter()
                        .map(|data_type| DataValue { data_type })
                        .collect(),
                }],
            };
            let bytes = table.encode_to_vec();
            ResponseData {
                original_size: bytes.len() as i32,
                compressed_data: match frames {
                    Frames::Sized => zstd::bulk::compress(&bytes, 3).unwrap(),
                    Frames::Streaming => zstd::stream::encode_all(bytes.as_slice(), 3).unwrap(),
                },
                compression_description: Some(CompressionDescription { algo: 1, level: 3 }),
                flat_file_manifest: None,
            }
        })
        .collect()
}

fn legacy(messages: Vec<ResponseData>) -> usize {
    messages
        .into_iter()
        .map(|m| black_box(decode::decode(m, LIMIT).unwrap()).rows.len())
        .sum()
}
fn bounded_numeric(messages: Vec<ResponseData>) -> usize {
    let limits = bounded::DecodeLimits::default();
    messages
        .into_iter()
        .map(|m| black_box(bounded::decode(m, &limits, None, false).unwrap()).row_count())
        .sum()
}
fn columnar(messages: &[ResponseData]) -> usize {
    let mut decoder = ColumnarDecoder::new(Limits::default()).unwrap();
    let mut rows = 0;
    for message in messages {
        decoder.push(message).unwrap();
        if decoder.buffered_rows() >= BATCH_ROWS {
            rows += black_box(decoder.flush().unwrap()).num_rows();
        }
    }
    rows + decoder.flush().map_or(0, |b| black_box(b).num_rows())
}

fn try_columnar(messages: &[ResponseData]) -> Result<usize, String> {
    let mut decoder = ColumnarDecoder::new(Limits::default()).map_err(|e| format!("{e:?}"))?;
    let mut rows = 0;
    for message in messages {
        decoder.push(message).map_err(|e| format!("{e:?}"))?;
        if decoder.buffered_rows() >= BATCH_ROWS {
            rows += decoder.flush().map_or(0, |b| b.num_rows());
        }
    }
    Ok(rows + decoder.flush().map_or(0, |b| b.num_rows()))
}

/// End to end through the public client on a loopback server that sends
/// `messages` for every request: the typed stock EOD stream (bounded numeric,
/// its default offload) against `query()` (Arrow, coalesced). The server's
/// encoding and sending cost is in both.
fn loopback(name: &str, messages: Vec<ResponseData>, samples: usize) -> Vec<Result_> {
    use praevis_thetadata_client::{ClientConfig, StockEodRequest, ThetaClient, endpoints};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let fixture = support::Fixture::start(support::Script {
            messages: messages.iter().cloned().map(Ok).collect(),
            ..Default::default()
        });
        let client = ThetaClient::with_session(
            ClientConfig {
                endpoint: Some(fixture.endpoint.clone()),
                allow_insecure: true,
                ..Default::default()
            },
            support::session().await,
        )
        .await
        .unwrap()
        // Measure transport and decoding, not the per-minute request limit.
        .with_scheduler(praevis_thetadata_client::SchedulerPolicy {
            per_minute: None,
            ..Default::default()
        })
        .unwrap();
        let day = chrono_day();
        let mut out = Vec::new();
        for engine in ["stock-eod-numeric", "query-arrow"] {
            let mut per_row = Vec::new();
            let mut first = Vec::new();
            for _ in 0..samples + 1 {
                let started = Instant::now();
                let mut first_at = None;
                let mut rows = 0;
                if engine == "query-arrow" {
                    let request = endpoints::StockHistoryEod::new("SYNTHETIC", day, day);
                    let mut stream = client.query(&request).await.unwrap();
                    while let Some(batch) = stream.next_batch().await.unwrap() {
                        first_at.get_or_insert_with(|| started.elapsed());
                        rows += black_box(batch).num_rows();
                    }
                } else {
                    let request = StockEodRequest::new("SYNTHETIC", day, day).unwrap();
                    let mut stream = client.stock_eod_batches(request).await.unwrap();
                    while let Some(batch) = stream.next_batch().await.unwrap() {
                        first_at.get_or_insert_with(|| started.elapsed());
                        rows += black_box(batch).row_count();
                    }
                }
                assert_eq!(rows, messages.len(), "one row per message");
                per_row.push(started.elapsed().as_secs_f64() * 1e6 / rows as f64);
                first.push(first_at.unwrap().as_secs_f64() * 1e3);
            }
            // The first run warms the connection and is not counted.
            per_row.remove(0);
            first.remove(0);
            per_row.sort_by(f64::total_cmp);
            first.sort_by(f64::total_cmp);
            let (median, min, max) = (
                per_row[per_row.len() / 2],
                per_row[0],
                per_row[per_row.len() - 1],
            );
            let first_ms = first[first.len() / 2];
            println!(
                "{name} {engine:<18} {median:7.3} µs/row (min {min:.3}, max {max:.3}), \
                 first batch {first_ms:.2} ms"
            );
            out.push(Result_ {
                workload: name.to_owned(),
                engine,
                us_per_row_median: median,
                us_per_row_min: min,
                us_per_row_max: max,
                allocations_per_message: f64::NAN,
                replay: None,
                first_batch_ms: Some(first_ms),
                error: None,
            });
        }
        out
    })
}
fn chrono_day() -> praevis_thetadata_client::NaiveDate {
    praevis_thetadata_client::NaiveDate::from_ymd_opt(2024, 6, 14).unwrap()
}

/// Each `<workload>-<n>.bin` file is one request: 4-byte big-endian
/// length-prefixed `ResponseData` messages. Files are grouped by workload.
fn load_captures(directory: &std::path::Path) -> Vec<(String, Vec<Vec<ResponseData>>)> {
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .expect("readable capture directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "bin"))
        .collect();
    files.sort();
    let mut groups: Vec<(String, Vec<Vec<ResponseData>>)> = Vec::new();
    for path in files {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let workload = stem.split('-').next().unwrap().to_owned();
        let bytes = std::fs::read(&path).expect("readable capture");
        let mut cursor = bytes.as_slice();
        let mut messages = Vec::new();
        while !cursor.is_empty() {
            let (length, rest) = cursor.split_at(4);
            let length = u32::from_be_bytes(length.try_into().unwrap()) as usize;
            let (message, rest) = rest.split_at(length);
            messages.push(ResponseData::decode(message).expect("a ResponseData message"));
            cursor = rest;
        }
        match groups.iter_mut().find(|(name, _)| *name == workload) {
            Some((_, requests)) => requests.push(messages),
            None => groups.push((workload, vec![messages])),
        }
    }
    assert!(
        !groups.is_empty(),
        "no .bin captures in {}",
        directory.display()
    );
    groups
}

/// Time `run` on inputs taken from `inputs` before each sample's clock starts.
fn time(
    samples: usize,
    rows: usize,
    inputs: &mut dyn FnMut() -> Vec<ResponseData>,
    mut run: impl FnMut(Option<Vec<ResponseData>>) -> usize,
) -> (f64, f64, f64) {
    let mut per_row: Vec<f64> = (0..samples)
        .map(|_| {
            let input = inputs();
            let input = (!input.is_empty()).then_some(input);
            let started = Instant::now();
            assert_eq!(run(input), rows);
            started.elapsed().as_secs_f64() * 1e6 / rows as f64
        })
        .collect();
    per_row.sort_by(f64::total_cmp);
    (
        per_row[per_row.len() / 2],
        per_row[0],
        per_row[per_row.len() - 1],
    )
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    assert!(
        cli.rows > 0 && cli.samples > 0,
        "rows and samples must be positive"
    );
    let mut results = Vec::new();
    let workloads: Vec<(String, Vec<Vec<ResponseData>>)> = match &cli.replay {
        Some(directory) => load_captures(directory),
        None => [("W1-shaped", 16), ("W2-shaped", 20)]
            .into_iter()
            .map(|(name, columns)| {
                (
                    name.to_owned(),
                    vec![workload(columns, cli.rows, cli.frames)],
                )
            })
            .collect(),
    };
    if cli.loopback {
        for (name, requests) in &workloads {
            let messages: Vec<ResponseData> = requests.iter().flatten().cloned().collect();
            results.extend(loopback(name, messages, cli.samples));
        }
    }
    for (name, requests) in workloads.into_iter().filter(|_| !cli.loopback) {
        let messages: Vec<ResponseData> = requests.iter().flatten().cloned().collect();
        let mut shape = ReplayShape {
            files: requests.len(),
            messages: messages.len(),
            ..Default::default()
        };
        messages.iter().for_each(|m| frame_shape(&mut shape, m));
        // Every engine must decode every message once before it is timed; a
        // failing engine is reported with its error class, not timed.
        let checks = [
            messages.iter().try_fold(0, |rows, m| {
                decode::decode(m.clone(), LIMIT)
                    .map(|t| rows + t.rows.len())
                    .map_err(|e| format!("{e:?}"))
            }),
            messages.iter().try_fold(0, |rows, m| {
                bounded::decode(m.clone(), &bounded::DecodeLimits::default(), None, false)
                    .map(|b| rows + b.row_count())
                    .map_err(|e| format!("{e:?}"))
            }),
            requests
                .iter()
                .try_fold(0, |rows, r| try_columnar(r).map(|n| rows + n)),
        ];
        let Some(rows) = checks.iter().find_map(|c| c.as_ref().ok().copied()) else {
            eprintln!("{name}: no engine decoded the workload; skipped");
            continue;
        };
        shape.rows = rows;
        // Owned engines consume their inputs, as the transport hands them over.
        let mut prebuilt: Vec<Vec<ResponseData>> = match cli.inputs {
            Inputs::Prebuilt => (0..2 * cli.samples).map(|_| messages.clone()).collect(),
            Inputs::Fresh => Vec::new(),
        };
        let mut owned = || prebuilt.pop().unwrap_or_else(|| messages.clone());
        let ok = |i: usize| checks[i].is_ok();
        COUNTING.store(cli.timing_allocator == TimingAllocator::Counting, Relaxed);
        let timings = [
            ok(0).then(|| time(cli.samples, rows, &mut owned, |m| legacy(m.unwrap()))),
            ok(1).then(|| {
                time(cli.samples, rows, &mut owned, |m| {
                    bounded_numeric(m.unwrap())
                })
            }),
            ok(2).then(|| {
                time(cli.samples, rows, &mut Vec::new, |_| {
                    requests.iter().map(|r| columnar(r)).sum()
                })
            }),
        ];
        COUNTING.store(false, Relaxed);
        drop(prebuilt);
        let allocations = [
            ok(0).then(|| count(|| legacy(messages.clone()))),
            ok(1).then(|| count(|| bounded_numeric(messages.clone()))),
            ok(2).then(|| count(|| requests.iter().map(|r| columnar(r)).sum())),
        ];
        let engines = ["legacy-table", "bounded-numeric", "columnar-arrow"];
        for (index, engine) in engines.into_iter().enumerate() {
            let (median, min, max) = timings[index].unwrap_or((f64::NAN, f64::NAN, f64::NAN));
            let per_message =
                allocations[index].map_or(f64::NAN, |n| n as f64 / shape.messages as f64);
            match &checks[index] {
                Ok(_) => println!(
                    "{name} {engine:<16} {median:7.3} µs/row (min {min:.3}, max {max:.3}), \
                     {per_message:.2} allocations/message"
                ),
                Err(error) => println!("{name} {engine:<16} failed: {error}"),
            }
            results.push(Result_ {
                workload: name.clone(),
                engine,
                us_per_row_median: median,
                us_per_row_min: min,
                us_per_row_max: max,
                allocations_per_message: per_message,
                replay: cli.replay.is_some().then(|| shape.clone()),
                first_batch_ms: None,
                error: checks[index].as_ref().err().cloned(),
            });
        }
        if cli.replay.is_some() {
            println!(
                "{name}: {} files, {} messages, {} rows; ZSTD frames {}: {} declare their size \
                 ({} single-segment), largest declared window {} bytes",
                shape.files,
                shape.messages,
                shape.rows,
                shape.zstd_frames,
                shape.sized_frames,
                shape.single_segment_frames,
                shape.max_declared_window
            );
        }
    }
    if let Some(path) = cli.output {
        let report = Report {
            format: "praevis-columnar-cpu-v2",
            synthetic: cli.replay.is_none(),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            rows: cli.rows,
            samples: cli.samples,
            frames: cli.replay.is_none().then_some(cli.frames),
            inputs: cli.inputs,
            timing_allocator: cli.timing_allocator,
            results,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        println!("Wrote {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_decodes_every_row_of_both_workloads() {
        for (columns, frames) in [(16, Frames::Sized), (20, Frames::Streaming)] {
            let messages = workload(columns, 50, frames);
            assert_eq!(legacy(messages.clone()), 50);
            assert_eq!(bounded_numeric(messages.clone()), 50);
            assert_eq!(columnar(&messages), 50);
            let mut decoder = ColumnarDecoder::new(Limits::default()).unwrap();
            for message in &messages {
                decoder.push(message).unwrap();
            }
            let batch = decoder.flush().unwrap();
            assert_eq!(batch.num_columns(), columns);
            assert!(batch.columns().iter().all(|c| c.null_count() == 0));
        }
    }

    #[test]
    fn captures_load_in_the_python_baseline_format_and_frame_shapes_are_read() {
        let directory = std::env::temp_dir().join(format!("praevis-replay-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let write = |name: &str, messages: &[ResponseData]| {
            let mut bytes = Vec::new();
            for message in messages {
                let encoded = message.encode_to_vec();
                bytes.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
                bytes.extend_from_slice(&encoded);
            }
            std::fs::write(directory.join(name), bytes).unwrap();
        };
        write("W1-0.bin", &workload(16, 3, Frames::Sized));
        write("W1-1.bin", &workload(16, 2, Frames::Sized));
        write("W2-0.bin", &workload(20, 4, Frames::Streaming));
        std::fs::write(directory.join("report.json"), b"{}").unwrap();
        let groups = load_captures(&directory);
        std::fs::remove_dir_all(&directory).unwrap();
        let summary: Vec<_> = groups
            .iter()
            .map(|(name, requests)| {
                (
                    name.as_str(),
                    requests.iter().map(Vec::len).collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(summary, [("W1", vec![3, 2]), ("W2", vec![4])]);
        let mut sized = ReplayShape::default();
        groups[0]
            .1
            .iter()
            .flatten()
            .for_each(|m| frame_shape(&mut sized, m));
        assert_eq!(
            (
                sized.zstd_frames,
                sized.sized_frames,
                sized.single_segment_frames
            ),
            (5, 5, 5)
        );
        assert_eq!(sized.max_declared_window, 0);
        let mut streaming = ReplayShape::default();
        groups[1]
            .1
            .iter()
            .flatten()
            .for_each(|m| frame_shape(&mut streaming, m));
        assert_eq!((streaming.zstd_frames, streaming.sized_frames), (4, 0));
        assert_eq!(streaming.max_declared_window, 2 << 20);
        assert_eq!(try_columnar(&groups[1].1[0]), Ok(4));
    }
}
