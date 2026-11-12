//! Offline: peak Rust heap of a per-strike job written to Parquet, against
//! the loopback fixture, as the parts grow. Explains whether memory under W5
//! plateaus (bounded by the job's read-ahead and batch limits) or keeps
//! growing (an accumulating buffer). The fixture answers at once, the worst
//! case for read-ahead: every open part fills its buffer.
//!
//! `cargo run --release --example job_memory -- [rows per part ...]`, or
//! `-- --sink` for the Parquet sink alone (the same batch written repeatedly,
//! no network or job). `JOB_MEMORY_DECIMAL_STATISTICS=1` writes decimal
//! min/max statistics (the opt-in), which makes the sink's memory grow with
//! the file; `JOB_MEMORY_CONCURRENCY` sets the job's concurrency
//! (default 8). The fixture runs in a child process so only the client's
//! heap is counted.
use praevis_thetadata_client::endpoints::OptionHistoryQuote;
use praevis_thetadata_client::{ClientConfig, ParquetOptions, RequestOptions, ThetaClient};
use praevis_thetadata_proto::endpoints::{
    DataTable, DataValue, DataValueList, Price, ZonedDateTime, data_value::DataType as Wire,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;

fn strikes() -> i32 {
    std::env::var("JOB_MEMORY_STRIKES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(232)
}

/// The process's peak resident memory (Linux `VmHWM`), if available.
fn peak_rss_mib() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kib: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib / 1024.0)
}

fn decimal_statistics() -> bool {
    std::env::var("JOB_MEMORY_DECIMAL_STATISTICS").is_ok_and(|v| v == "1")
}
const ROWS_PER_MESSAGE: usize = 4096;

/// A tick-quote-like message: 12 columns (4 prices, 6 integers, a
/// timestamp, a symbol), about the width of the live rows.
fn message(rows: usize, strike: i32) -> praevis_thetadata_proto::endpoints::ResponseData {
    let table = DataTable {
        headers: (0..12).map(|i| format!("c{i}")).collect(),
        data_table: (0..rows as i64)
            .map(|r| DataValueList {
                values: (0..12)
                    .map(|c| DataValue {
                        data_type: Some(match c {
                            0..4 => Wire::Price(Price {
                                value: 54_000 + r as i32 % 500 + strike,
                                r#type: 8,
                            }),
                            4..10 => Wire::Number(r * 7 + c),
                            10 => Wire::Timestamp(ZonedDateTime {
                                epoch_ms: 1_718_373_600_000 + r as u64,
                                zone: 0,
                                ..Default::default()
                            }),
                            _ => Wire::Text("SPY".into()),
                        }),
                    })
                    .collect(),
            })
            .collect(),
    };
    support::response(&table, true)
}

/// The fixture answering every strike with `rows` rows, in this process.
fn fixture(rows: usize) -> support::Fixture {
    let full = Arc::new(message(ROWS_PER_MESSAGE, 0));
    let tail = Arc::new(message(rows % ROWS_PER_MESSAGE, 0));
    let handler: support::Handler = Arc::new(move |rpc, _request| match rpc {
        "GetOptionListStrikes" => {
            let table = DataTable {
                headers: vec!["strike".into()],
                data_table: (0..strikes())
                    .map(|s| DataValueList {
                        values: vec![DataValue {
                            data_type: Some(Wire::Price(Price {
                                value: 40_000 + s * 100,
                                r#type: 8,
                            })),
                        }],
                    })
                    .collect(),
            };
            Ok(vec![support::response(&table, false)])
        }
        "GetOptionHistoryQuote" => {
            let mut out = vec![(*full).clone(); rows / ROWS_PER_MESSAGE];
            if !rows.is_multiple_of(ROWS_PER_MESSAGE) {
                out.push((*tail).clone());
            }
            Ok(out)
        }
        _ => Err(tonic::Status::unimplemented("synthetic")),
    });
    support::Fixture::start(support::Script {
        other: Some(handler),
        ..Default::default()
    })
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Child: serve the fixture in its own process (so its memory is not
    // counted), print the endpoint, and stop when the parent closes stdin.
    if args.first().map(String::as_str) == Some("--serve") {
        let server = fixture(args[1].parse().expect("rows"));
        println!("{}", server.endpoint);
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        return;
    }
    // Sink only: the same decoded batch written N times, no network or job.
    if args.first().map(String::as_str) == Some("--sink") {
        let mut decoder =
            praevis_thetadata_arrow::ColumnarDecoder::new(Default::default()).unwrap();
        for _ in 0..16 {
            decoder.push(&message(ROWS_PER_MESSAGE, 0)).unwrap();
        }
        let batch = decoder.flush().unwrap();
        for total in [1_000_000usize, 10_000_000, 40_000_000, 140_000_000] {
            let path = std::env::temp_dir()
                .join(format!("job-memory-sink-{}.parquet", std::process::id()));
            let before = LIVE.load(Ordering::Relaxed);
            PEAK.store(before, Ordering::Relaxed);
            let mut sink = praevis_thetadata_arrow::ParquetSink::create(
                &path,
                ParquetOptions::default()
                    .with_overwrite(true)
                    .with_decimal_statistics(decimal_statistics()),
            )
            .unwrap();
            let mut written = 0;
            while written < total {
                sink.write(&batch).unwrap();
                written += batch.num_rows();
            }
            let summary = sink.finish().unwrap();
            let mib = |b: usize| b as f64 / (1 << 20) as f64;
            println!(
                "sink only: {written:>10} rows, peak heap {:.1} MiB, row groups {}",
                mib(PEAK.load(Ordering::Relaxed) - before),
                summary.row_groups
            );
            let _ = std::fs::remove_file(&path);
        }
        return;
    }
    let sizes: Vec<usize> = args
        .iter()
        .map(|a| a.parse().expect("rows per part"))
        .collect();
    let sizes = if sizes.is_empty() {
        vec![4_000, 13_000, 22_000, 65_536, 150_000, 300_000, 600_000]
    } else {
        sizes
    };
    println!(
        "rows/part  parts  rows        peak heap MiB  heap after MiB  parquet MiB  process peak RSS MiB"
    );
    for rows in sizes {
        use std::io::BufRead;
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--serve", &rows.to_string()])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("fixture process");
        let mut endpoint = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut endpoint)
            .unwrap();
        let client = ThetaClient::with_session(
            ClientConfig::default()
                .with_endpoint(Some(endpoint.trim().to_owned()))
                .with_allow_insecure(true),
            support::session().await,
        )
        .await
        .unwrap();
        let concurrency = std::env::var("JOB_MEMORY_CONCURRENCY")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(8);
        let request = OptionHistoryQuote::new("SPY", "2024-06-21")
            .date(chrono::NaiveDate::from_ymd_opt(2024, 6, 14).unwrap())
            .interval("tick");
        let path =
            std::env::temp_dir().join(format!("job-memory-{}-{rows}.parquet", std::process::id()));
        let before = LIVE.load(Ordering::Relaxed);
        PEAK.store(before, Ordering::Relaxed);
        let options = RequestOptions::default().with_concurrency(Some(concurrency));
        let job = client.job_with(&request, options).await.unwrap();
        let parts = job.parts();
        let summary = job
            .to_parquet(
                &path,
                ParquetOptions::default()
                    .with_overwrite(true)
                    .with_decimal_statistics(decimal_statistics()),
            )
            .await
            .unwrap();
        let peak = PEAK.load(Ordering::Relaxed) - before;
        let after = LIVE.load(Ordering::Relaxed).saturating_sub(before);
        let mib = |b: usize| b as f64 / (1 << 20) as f64;
        println!(
            "{rows:>9}  {parts:>5}  {:>10}  {:>13.1}  {:>14.1}  {:>11.1}  {}",
            summary.rows,
            mib(peak),
            mib(after),
            mib(summary.bytes as usize),
            peak_rss_mib().map_or("-".into(), |r| format!("{r:.1}"))
        );
        let _ = std::fs::remove_file(&path);
        drop(client);
        drop(child.stdin.take());
        let _ = child.wait();
    }
}
