//! Live W5 memory, measured inside the process (options phase). Manual only:
//! never CI. Requires `--features live-tests`, `--confirm-live`,
//! `--api-key-env` and `THETADATA_API_KEY` (never printed or stored).
//!
//! Runs one `option_history_quote` workload from `workloads.json` through
//! `job()` to a private Parquet file, batch by batch, exactly as
//! `JobStream::to_parquet` does (each write on the blocking pool). It records:
//! - the **live Rust heap** (a counting allocator) and its peak, sampled as
//!   rows are written, so steady growth and a plateau look different;
//! - the **process peak working set / RSS** before and after, so heap and
//!   allocator retention can be told apart;
//! - the **batch sizes** (a proxy for part sizes: a full batch is 65,536
//!   rows).
//!
//! Timings, counts and sizes only: no values. The Parquet file stays under
//! the ignored `artifacts/` and is deleted after its size is recorded.
use clap::Parser;
use praevis_thetadata_client::endpoints::OptionHistoryQuote;
use praevis_thetadata_client::{
    AuthClient, AuthConfig, ClientConfig, Credentials, Environment, NaiveDate, ParquetOptions,
    ThetaClient,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

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

const WORKLOADS: &str = include_str!("../../../bench/python-baseline/workloads.json");
const MIB: f64 = (1 << 20) as f64;

/// The process's peak working set (Windows) or peak RSS (Linux), in bytes.
#[cfg(windows)]
fn peak_rss() -> Option<u64> {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut counters: Counters = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<Counters>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    (ok != 0).then_some(counters.peak_working_set_size as u64)
}
#[cfg(not(windows))]
fn peak_rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|kib| kib * 1024)
}

#[derive(Parser)]
#[command(about = "Live W5 memory inside the process (private artifacts; manual only)")]
struct Cli {
    #[arg(long, required = true)]
    confirm_live: bool,
    #[arg(long, required = true)]
    api_key_env: bool,
    #[arg(long)]
    run_id: String,
    /// One-request `option_history_quote` workload (M1-M5, M4b).
    #[arg(long)]
    workload: String,
    /// Keep the private Parquet file.
    #[arg(long)]
    keep: bool,
}

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("workload dates are ISO")
}

/// The workload's request as typed: only the fields the M workloads use.
fn request(spec: &serde_json::Value) -> Result<OptionHistoryQuote, String> {
    if spec["method"] != "option_history_quote" {
        return Err("only option_history_quote workloads".into());
    }
    let [one] = spec["requests"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    else {
        return Err("needs exactly one request".into());
    };
    let text = |k: &str| one[k].as_str().map(str::to_owned);
    let mut r = OptionHistoryQuote::new(
        text("symbol").ok_or("symbol")?,
        text("expiration").ok_or("expiration")?,
    );
    for (key, value) in one.as_object().ok_or("request")? {
        let v = value.as_str().ok_or("text values only")?;
        r = match key.as_str() {
            "symbol" | "expiration" => r,
            "date" => r.date(date(v)),
            "start_date" => r.start_date(date(v)),
            "end_date" => r.end_date(date(v)),
            "interval" => r.interval(v),
            "right" => r.right(v),
            "strike" => r.strike(v),
            "start_time" => r.start_time(v),
            "end_time" => r.end_time(v),
            other => return Err(format!("unknown field {other}")),
        };
    }
    Ok(r)
}

fn output_path(run_id: &str, extension: &str) -> Result<std::path::PathBuf, &'static str> {
    if run_id.is_empty()
        || !run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("run id must be ASCII letters, digits, '-' or '_'");
    }
    Ok(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/p1")
        .join(format!("{run_id}.{extension}")))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    if !cli.confirm_live || !cli.api_key_env || std::env::var_os("CI").is_some() {
        eprintln!("live execution requires --confirm-live and --api-key-env, outside CI");
        return std::process::ExitCode::FAILURE;
    }
    let (report_path, parquet_path) = match (
        output_path(&cli.run_id, "json"),
        output_path(&cli.run_id, "parquet"),
    ) {
        (Ok(r), Ok(p)) if !r.exists() => (r, p),
        (Ok(_), Ok(_)) => {
            eprintln!("run id already used");
            return std::process::ExitCode::FAILURE;
        }
        (Err(m), _) | (_, Err(m)) => {
            eprintln!("{m}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let workloads: serde_json::Value =
        serde_json::from_str(WORKLOADS).expect("valid workloads.json");
    let request = match request(&workloads[cli.workload.as_str()]) {
        Ok(r) => r,
        Err(m) => {
            eprintln!("workload: {m}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let key = match std::env::var("THETADATA_API_KEY") {
        Ok(k) if !k.trim().is_empty() && !k.trim().chars().any(char::is_control) => k,
        _ => {
            eprintln!("THETADATA_API_KEY is missing or invalid");
            return std::process::ExitCode::FAILURE;
        }
    };
    let session = match AuthClient::new(AuthConfig {
        environment: Environment::Prod,
        ..Default::default()
    }) {
        Ok(auth) => match auth.authenticate(&Credentials::api_key(key.trim())).await {
            Ok(s) => s,
            Err(_) => {
                eprintln!("authentication failed");
                return std::process::ExitCode::FAILURE;
            }
        },
        Err(_) => {
            eprintln!("authentication configuration");
            return std::process::ExitCode::FAILURE;
        }
    };
    drop(key);
    let identity = [
        session.session_id().to_owned(),
        session.user().email.clone(),
    ];
    let client = match ThetaClient::with_session(ClientConfig::default(), session).await {
        Ok(c) => c,
        Err(_) => {
            eprintln!("connection failed");
            return std::process::ExitCode::FAILURE;
        }
    };
    let idle_rss = peak_rss();
    let idle_heap = LIVE.load(Ordering::Relaxed);
    PEAK.store(idle_heap, Ordering::Relaxed);
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut sizes = std::collections::BTreeMap::<&'static str, u64>::new();
    let mut rows = 0u64;
    let mut next_sample = 0u64;
    let result: Result<(usize, usize, u64), String> = async {
        let mut job = client.job(&request).await.map_err(|e| format!("{e:?}"))?;
        let parts = job.parts();
        std::fs::create_dir_all(parquet_path.parent().unwrap())
            .map_err(|e| e.kind().to_string())?;
        let path = parquet_path.clone();
        let mut sink = tokio::task::spawn_blocking(move || {
            praevis_thetadata_arrow::ParquetSink::create(
                path,
                ParquetOptions {
                    overwrite: true,
                    ..Default::default()
                },
            )
        })
        .await
        .map_err(|_| "join")?
        .map_err(|e| format!("{e:?}"))?;
        while let Some(batch) = job.next_batch().await.map_err(|e| format!("{e:?}"))? {
            let n = batch.num_rows() as u64;
            *sizes
                .entry(match n {
                    0..1_000 => "under 1k",
                    1_000..10_000 => "1k-10k",
                    10_000..65_536 => "10k-65,535",
                    _ => "65,536 (full)",
                })
                .or_default() += 1;
            sink = tokio::task::spawn_blocking(move || sink.write(&batch).map(|()| sink))
                .await
                .map_err(|_| "join")?
                .map_err(|e| format!("{e:?}"))?;
            rows += n;
            if rows >= next_sample {
                samples.push(serde_json::json!({
                    "rows": rows,
                    "elapsed_ms": start.elapsed().as_millis(),
                    "heap_mib": (LIVE.load(Ordering::Relaxed) - idle_heap) as f64 / MIB,
                    "heap_peak_mib": (PEAK.load(Ordering::Relaxed) - idle_heap) as f64 / MIB,
                }));
                next_sample = rows + 500_000;
            }
        }
        let summary = tokio::task::spawn_blocking(move || sink.finish())
            .await
            .map_err(|_| "join")?
            .map_err(|e| format!("{e:?}"))?;
        Ok((parts, summary.row_groups, summary.bytes))
    }
    .await;
    let mut report = serde_json::json!({
        "run_id": cli.run_id,
        "command": "live-job-memory",
        "workload": cli.workload,
        "host": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "tier": client.tier().name(),
        "concurrency": client.scheduler_policy().concurrency,
        "rows": rows,
        "wall_ms": start.elapsed().as_millis(),
        "heap_peak_mib": (PEAK.load(Ordering::Relaxed) - idle_heap) as f64 / MIB,
        "heap_after_mib": LIVE.load(Ordering::Relaxed).saturating_sub(idle_heap) as f64 / MIB,
        "idle_peak_rss_mib": idle_rss.map(|b| b as f64 / MIB),
        "peak_rss_mib": peak_rss().map(|b| b as f64 / MIB),
        "batch_sizes": sizes,
        "samples": samples,
    });
    match result {
        Ok((parts, row_groups, bytes)) => {
            report["status"] = "OK".into();
            report["parts"] = parts.into();
            report["row_groups"] = row_groups.into();
            report["file_bytes"] = bytes.into();
        }
        Err(status) => report["status"] = status.into(),
    }
    if !cli.keep {
        let _ = std::fs::remove_file(&parquet_path);
    }
    let text = serde_json::to_string_pretty(&report).unwrap();
    if identity
        .iter()
        .any(|v| !v.is_empty() && text.contains(v.as_str()))
    {
        eprintln!("Refusing to write the report: it would contain account identity.");
        return std::process::ExitCode::FAILURE;
    }
    std::fs::create_dir_all(report_path.parent().unwrap()).expect("artifacts directory");
    std::fs::write(&report_path, text).expect("write report");
    println!(
        "{} {}: {rows} rows, heap peak {:.1} MiB, peak RSS {:?} MiB (idle {:?})",
        cli.workload,
        report["status"],
        report["heap_peak_mib"].as_f64().unwrap_or(0.0),
        report["peak_rss_mib"].as_f64().map(|v| v.round()),
        report["idle_peak_rss_mib"].as_f64().map(|v| v.round()),
    );
    println!("Wrote {}", report_path.display());
    if report["status"] == "OK" {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_m_workload_types_and_others_are_refused() {
        let workloads: serde_json::Value = serde_json::from_str(WORKLOADS).unwrap();
        for id in ["M1", "M2", "M3", "M4", "M4b", "M5"] {
            assert!(request(&workloads[id]).is_ok(), "{id}");
        }
        assert!(request(&workloads["W5c"]).is_err());
        assert!(request(&workloads["W5a"]).is_err());
    }
}
