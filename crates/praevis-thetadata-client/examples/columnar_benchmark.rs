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
//! The P1 exit criterion (c) is measured separately, by replaying the private
//! P0 captures on the maintainer's machine.
//!
//! Timings use the system allocator directly; allocation counts come from a
//! separate counted run. `--inputs prebuilt --timing-allocator counting`
//! reproduces the first PR 3 method, which held every sample's input copies
//! at once and counted allocations while timing, for comparison.
use clap::Parser;
use praevis_thetadata_arrow::{ColumnarDecoder, Limits};
use praevis_thetadata_proto::beta_endpoints as api;
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
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Serialize)]
struct Report {
    format: &'static str,
    synthetic: bool,
    os: &'static str,
    arch: &'static str,
    rows: usize,
    samples: usize,
    inputs: Inputs,
    timing_allocator: TimingAllocator,
    results: Vec<Result_>,
}

#[derive(Serialize)]
struct Result_ {
    workload: &'static str,
    engine: &'static str,
    us_per_row_median: f64,
    us_per_row_min: f64,
    us_per_row_max: f64,
    /// Rust heap allocation calls per message over one full run. Owned
    /// engines include one copy of each input message.
    allocations_per_message: f64,
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
fn workload(columns: usize, rows: usize) -> Vec<ResponseData> {
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
                compressed_data: zstd::stream::encode_all(bytes.as_slice(), 3).unwrap(),
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
    for (name, columns) in [("W1-shaped", 16), ("W2-shaped", 20)] {
        let messages = workload(columns, cli.rows);
        // Warm-up, and a check that every engine returns every row.
        assert_eq!(columnar(&messages), cli.rows);
        // Owned engines consume their inputs, as the transport hands them over.
        let mut prebuilt: Vec<Vec<ResponseData>> = match cli.inputs {
            Inputs::Prebuilt => (0..2 * cli.samples).map(|_| messages.clone()).collect(),
            Inputs::Fresh => Vec::new(),
        };
        let mut owned = || prebuilt.pop().unwrap_or_else(|| messages.clone());
        COUNTING.store(cli.timing_allocator == TimingAllocator::Counting, Relaxed);
        let timings = [
            time(cli.samples, cli.rows, &mut owned, |m| legacy(m.unwrap())),
            time(cli.samples, cli.rows, &mut owned, |m| {
                bounded_numeric(m.unwrap())
            }),
            time(cli.samples, cli.rows, &mut || Vec::new(), |_| {
                columnar(&messages)
            }),
        ];
        COUNTING.store(false, Relaxed);
        drop(prebuilt);
        let [legacy_time, bounded_time, columnar_time] = timings;
        let engines: [(&'static str, (f64, f64, f64), f64); 3] = [
            (
                "legacy-table",
                legacy_time,
                count(|| legacy(messages.clone())) as f64,
            ),
            (
                "bounded-numeric",
                bounded_time,
                count(|| bounded_numeric(messages.clone())) as f64,
            ),
            (
                "columnar-arrow",
                columnar_time,
                count(|| columnar(&messages)) as f64,
            ),
        ];
        for (engine, (median, min, max), allocations) in engines {
            println!(
                "{name} {engine:<16} {median:7.3} µs/row (min {min:.3}, max {max:.3}), \
                 {:.2} allocations/message",
                allocations / cli.rows as f64
            );
            results.push(Result_ {
                workload: name,
                engine,
                us_per_row_median: median,
                us_per_row_min: min,
                us_per_row_max: max,
                allocations_per_message: allocations / cli.rows as f64,
            });
        }
    }
    if let Some(path) = cli.output {
        let report = Report {
            format: "praevis-columnar-cpu-v1",
            synthetic: true,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            rows: cli.rows,
            samples: cli.samples,
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
        for columns in [16, 20] {
            let messages = workload(columns, 50);
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
}
