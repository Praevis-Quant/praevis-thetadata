//! Process-memory profile of the typed EOD client (EOD-L3-023, deferred item 1,
//! decision D-H): operating-system counters plus libzstd's own context sizes,
//! with no new dependency.
//!
//! The synthetic fixture server runs in a child process, so only the client's
//! memory is measured. Each configuration runs in a fresh child server, client
//! and runtime, and records, relative to the connected, idle client:
//! - the OS peak: Linux `VmHWM` after a reset, or a 1 ms sampler of the Windows
//!   working set and private bytes;
//! - the peak of all Rust heap allocations in the process, which includes tonic,
//!   h2 and hyper buffers, through a counting global allocator;
//! - their difference, which bounds libzstd's C allocations plus allocator
//!   retention and fragmentation.
//!
//! It also reports `ZSTD_sizeof_DCtx` after decoding frames at each window size,
//! next to the client's fixed ZSTD accounting.
//!
//! ```text
//! cargo run --release -p praevis-thetadata-client --example eod_memory -- run
//! ```
use praevis_thetadata_client::{
    ClientConfig, DecodeLimits, EodPolicy, NaiveDate, StockEodRequest, ThetaClient,
};
use praevis_thetadata_proto::endpoints as wire;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[allow(dead_code)] // Shared with the tests; not every helper is used here.
#[path = "../tests/support/mod.rs"]
mod support;

type Failure = Box<dyn std::error::Error + Send + Sync>;

/// Process-wide Rust heap accounting: live bytes and a resettable peak.
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn grow(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(live, Ordering::Relaxed);
}
// SAFETY: every obligation is delegated unchanged to System. The counters are
// lock-free atomics and never allocate; call-site tracking (`sites`) is off
// unless the `sites` command enables it, and guards against re-entry.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            grow(layout.size());
            sites::allocated(pointer as usize, layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            grow(layout.size());
            sites::allocated(pointer as usize, layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        sites::freed(pointer as usize, layout.size());
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        sites::freed(pointer as usize, layout.size());
        let result = unsafe { System.realloc(pointer, layout, new) };
        if !result.is_null() {
            if new >= layout.size() {
                grow(new - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - new, Ordering::Relaxed);
            }
            sites::allocated(result as usize, new);
        }
        result
    }
}

/// Call-site attribution of large live allocations (the `sites` command).
/// Every allocation of at least 1 MiB records its backtrace; at each new peak
/// of tracked live bytes the live set is snapshotted. Build with line tables
/// for readable frames: `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`.
mod sites {
    use std::{
        backtrace::Backtrace,
        cell::Cell,
        collections::HashMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };
    pub const THRESHOLD: usize = 1 << 20;
    static ENABLED: AtomicBool = AtomicBool::new(false);
    thread_local! { static BUSY: Cell<bool> = const { Cell::new(false) }; }
    #[derive(Default)]
    struct State {
        live: HashMap<usize, (usize, Arc<Backtrace>)>,
        bytes: usize,
        peak_bytes: usize,
        peak: Vec<(usize, Arc<Backtrace>)>,
    }
    static STATE: Mutex<Option<State>> = Mutex::new(None);
    fn guarded(f: impl FnOnce()) {
        let _ = BUSY.try_with(|busy| {
            if !busy.replace(true) {
                f();
                busy.set(false);
            }
        });
    }
    pub fn start() {
        guarded(|| *STATE.lock().unwrap() = Some(State::default()));
        ENABLED.store(true, Ordering::SeqCst);
    }
    pub fn allocated(pointer: usize, size: usize) {
        if size < THRESHOLD || !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        guarded(|| {
            let trace = Arc::new(Backtrace::force_capture());
            let mut state = STATE.lock().unwrap();
            if let Some(state) = state.as_mut() {
                state.live.insert(pointer, (size, trace));
                state.bytes += size;
                if state.bytes > state.peak_bytes {
                    state.peak_bytes = state.bytes;
                    state.peak = state.live.values().cloned().collect();
                }
            }
        });
    }
    pub fn freed(pointer: usize, size: usize) {
        if size < THRESHOLD || !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        guarded(|| {
            let mut state = STATE.lock().unwrap();
            if let Some(state) = state.as_mut()
                && let Some((size, _)) = state.live.remove(&pointer)
            {
                state.bytes -= size;
            }
        });
    }
    /// The source locations that identify a site: the first three outside the
    /// Rust standard library, this harness and the `bytes` containers, as
    /// `crate-version/file:line`.
    fn site(trace: &Backtrace) -> String {
        let text = format!("{trace}");
        let locations: Vec<String> = text
            .lines()
            .filter_map(|line| line.trim().strip_prefix("at "))
            .filter(|at| {
                !(at.starts_with("/rustc/")
                    || at.contains("examples/eod_memory.rs")
                    || at.contains("/bytes-")
                    || at.contains("/hashbrown-"))
            })
            .map(|at| {
                let path = at.rsplit_once(':').map_or(at, |(p, _)| p);
                let (file, line) = path.rsplit_once(':').unwrap_or((path, ""));
                let short = file
                    .rsplit_once("/registry/src/")
                    .and_then(|(_, rest)| rest.split_once('/').map(|(_, r)| r))
                    .or_else(|| file.split_once("/crates/").map(|(_, r)| r))
                    .unwrap_or(file);
                format!("{short}:{line}")
            })
            .collect();
        locations
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" <- ")
    }
    /// The tracked allocations live right now, grouped by site (tracking stays on).
    pub fn live() -> Vec<(String, usize, usize)> {
        let snapshot: Vec<(usize, Arc<Backtrace>)> = {
            let state = STATE.lock().unwrap();
            state
                .as_ref()
                .map(|s| s.live.values().cloned().collect())
                .unwrap_or_default()
        };
        group(&snapshot)
    }
    fn group(entries: &[(usize, Arc<Backtrace>)]) -> Vec<(String, usize, usize)> {
        // Symbolizing holds std's backtrace lock and allocates; suppress
        // tracking on this thread meanwhile, or a tracked allocation would try
        // to capture a backtrace under the same lock.
        let mut grouped: HashMap<String, (usize, usize)> = HashMap::new();
        guarded(|| {
            for (size, trace) in entries {
                let entry = grouped.entry(site(trace)).or_default();
                entry.0 += 1;
                entry.1 += size;
            }
        });
        let mut rows: Vec<_> = grouped.into_iter().map(|(s, (n, b))| (s, n, b)).collect();
        rows.sort_by(|a, b| b.2.cmp(&a.2));
        rows
    }
    /// Stop tracking; return the peak tracked bytes and the live set at that
    /// peak, grouped by site: (site, allocations, bytes), largest first.
    pub fn report() -> (usize, Vec<(String, usize, usize)>) {
        ENABLED.store(false, Ordering::SeqCst);
        let state = STATE.lock().unwrap().take().unwrap_or_default();
        (state.peak_bytes, group(&state.peak))
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Resident and private (anonymous) bytes of this process.
#[derive(Clone, Copy, Default)]
struct Sample {
    resident: usize,
    private: usize,
}

#[cfg(target_os = "linux")]
mod os {
    use super::Sample;
    fn status() -> std::collections::HashMap<String, usize> {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once(':')?;
                let kib: usize = value.trim().strip_suffix(" kB")?.parse().ok()?;
                Some((key.to_owned(), kib * 1024))
            })
            .collect()
    }
    pub fn sample() -> Sample {
        let status = status();
        Sample {
            resident: status["VmRSS"],
            private: status["RssAnon"],
        }
    }
    /// Reset the kernel's resident high-water mark (VmHWM) for this process.
    pub fn reset_peak() {
        std::fs::write("/proc/self/clear_refs", "5").unwrap();
    }
    /// Exact kernel high-water mark since the last reset.
    pub fn peak_resident() -> Option<usize> {
        Some(status()["VmHWM"])
    }
    pub const METHOD: &str = "Linux /proc/self/status: VmHWM (reset via clear_refs) and RssAnon";
}

#[cfg(windows)]
mod os {
    use super::Sample;
    use std::ffi::c_void;
    /// PROCESS_MEMORY_COUNTERS_EX, declared here instead of adding a crate.
    #[repr(C)]
    #[derive(Default)]
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
        private_usage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut Counters, cb: u32) -> i32;
    }
    pub fn sample() -> Sample {
        let mut counters = Counters {
            cb: size_of::<Counters>() as u32,
            ..Default::default()
        };
        // SAFETY: the pseudo-handle needs no closing, and `counters` is a
        // correctly sized, writable PROCESS_MEMORY_COUNTERS_EX.
        let ok =
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        assert_ne!(ok, 0, "K32GetProcessMemoryInfo failed");
        Sample {
            resident: counters.working_set_size,
            private: counters.private_usage,
        }
    }
    /// The Windows peak counter cannot be reset; the sampler provides the peak.
    pub fn reset_peak() {}
    pub fn peak_resident() -> Option<usize> {
        None
    }
    pub const METHOD: &str =
        "Windows K32GetProcessMemoryInfo: working set and private bytes, 1 ms sampler";
}

/// Samples OS counters every millisecond and keeps the maxima.
struct Sampler {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<Sample>,
}
impl Sampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut peak = Sample::default();
            loop {
                let now = os::sample();
                peak.resident = peak.resident.max(now.resident);
                peak.private = peak.private.max(now.private);
                if flag.load(Ordering::Relaxed) {
                    return peak;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        Self { stop, thread }
    }
    fn finish(self) -> Sample {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().unwrap()
    }
}

fn workload(name: &str) -> wire::DataTable {
    use wire::{DataValue, DataValueList, data_value::DataType};
    match name {
        "large-mixed" => support::table(support::Shape::Mixed, 10_000).0,
        "nulls" => support::table(support::Shape::Nulls, 10_000).0,
        "prices" => support::table(support::Shape::Prices, 10_000).0,
        "timestamps" => support::table(support::Shape::Timestamps, 10_000).0,
        // Rows of eight 1 MiB text cells. text-24mib is the calibration's largest
        // accepted text batch.
        "text-8mib" | "text-16mib" | "text-24mib" => {
            let rows = name[5..name.len() - 3].parse::<usize>().unwrap() / 8;
            let text = "T".repeat(DecodeLimits::default().text_bytes);
            wire::DataTable {
                headers: (0..8).map(|n| format!("synthetic_{n}")).collect(),
                data_table: (0..rows)
                    .map(|_| DataValueList {
                        values: (0..8)
                            .map(|_| DataValue {
                                data_type: Some(DataType::Text(text.clone())),
                            })
                            .collect(),
                    })
                    .collect(),
            }
        }
        other => panic!("unknown workload {other}"),
    }
}
const WORKLOADS: [&str; 5] = ["large-mixed", "nulls", "prices", "timestamps", "text-24mib"];

/// Child process: serve `messages` copies of one workload per request until stdin closes.
fn serve(name: &str, compressed: bool, messages: usize) {
    let response = support::response(&workload(name), compressed);
    let fixture = support::Fixture::start(support::Script {
        messages: vec![Ok(response); messages],
        ..Default::default()
    });
    println!("ENDPOINT {}", fixture.endpoint);
    std::io::stdout().flush().unwrap();
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
    drop(fixture);
}

struct Row {
    workload: &'static str,
    compressed: bool,
    table: bool,
    streams: usize,
    batches: usize,
    rust_peak: usize,
    os_peak: usize,
    private_peak: usize,
    elapsed: Duration,
}

fn request() -> StockEodRequest {
    StockEodRequest::new(
        "SYNTHETIC",
        NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
    )
    .unwrap()
}

async fn consume(client: &ThetaClient, table: bool) -> Result<usize, Failure> {
    let mut batches = 0;
    if table {
        let mut stream = client.stock_eod(request()).await?;
        while stream.next_batch().await?.is_some() {
            batches += 1;
        }
    } else {
        let mut stream = client.stock_eod_batches(request()).await?;
        while stream.next_batch().await?.is_some() {
            batches += 1;
        }
    }
    Ok(batches)
}

/// Child process: measure one configuration against a running server and
/// print one result line. A fresh process keeps the OS counters free of memory
/// retained by earlier configurations.
fn measure(
    endpoint: String,
    table: bool,
    streams: usize,
    policy: EodPolicy,
) -> Result<(), Failure> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(endpoint),
                allow_insecure: true,
                ..Default::default()
            },
            support::session().await,
            policy,
        )
        .await?;
        // No warm-up query: memory it freed would stay in the allocator and hide
        // the growth being measured. The baseline is a connected, idle client.
        tokio::time::sleep(Duration::from_millis(50)).await;
        os::reset_peak();
        let baseline = os::sample();
        let live = LIVE.load(Ordering::Relaxed);
        PEAK.store(live, Ordering::Relaxed);
        if std::env::var_os("PRAEVIS_RETAIN").is_some() {
            // Retention: read and drop one batch, then hold the stream idle
            // (no further next_batch call) and list what is still live.
            sites::start();
            let mut stream = client.stock_eod_batches(request()).await?;
            drop(stream.next_batch().await?);
            tokio::time::sleep(Duration::from_millis(200)).await;
            for (site, count, bytes) in sites::live() {
                println!(
                    "RETAINED {:>6.1} MiB  x{count:<2} {site}",
                    bytes as f64 / (1 << 20) as f64
                );
            }
            while stream.next_batch().await?.is_some() {}
            drop(stream);
            tokio::time::sleep(Duration::from_millis(200)).await;
            let after: usize = sites::live().iter().map(|r| r.2).sum();
            println!(
                "RETAINED after the stream completed: {:.1} MiB",
                after as f64 / (1 << 20) as f64
            );
            let _ = sites::report();
        }
        let attribute = std::env::var_os("PRAEVIS_SITES").is_some();
        if attribute {
            sites::start();
        }
        let sampler = Sampler::start();
        let started = Instant::now();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..streams {
            let client = client.clone();
            tasks.spawn(async move { consume(&client, table).await });
        }
        let mut batches = 0;
        while let Some(result) = tasks.join_next().await {
            batches += result??;
        }
        let elapsed = started.elapsed();
        let sampled = sampler.finish();
        if attribute {
            let (peak, rows) = sites::report();
            println!("SITES peak {} MiB of allocations >= 1 MiB", peak >> 20);
            for (site, count, bytes) in rows {
                println!(
                    "SITE {:>7.1} MiB  x{count:<3} {site}",
                    bytes as f64 / (1 << 20) as f64
                );
            }
        }
        let os_peak = os::peak_resident().unwrap_or(sampled.resident);
        println!(
            "RESULT {batches} {} {} {} {}",
            PEAK.load(Ordering::Relaxed).saturating_sub(live),
            os_peak.saturating_sub(baseline.resident),
            sampled.private.saturating_sub(baseline.private),
            elapsed.as_micros()
        );
        Ok::<_, Failure>(())
    })
}

fn profile(
    name: &'static str,
    compressed: bool,
    table: bool,
    streams: usize,
    messages: usize,
    stream_cap: Option<usize>,
) -> Result<Row, Failure> {
    let exe = std::env::current_exe()?;
    let mut server = Command::new(&exe)
        .args([
            "serve",
            name,
            if compressed { "zstd" } else { "none" },
            &messages.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap()).read_line(&mut line)?;
    let endpoint = line
        .trim()
        .strip_prefix("ENDPOINT ")
        .ok_or("server did not report an endpoint")?
        .to_owned();
    let mut arguments = vec![
        "measure".to_owned(),
        endpoint,
        if table { "table" } else { "numeric" }.to_owned(),
        streams.to_string(),
    ];
    arguments.extend(stream_cap.map(|cap| cap.to_string()));
    let output = Command::new(&exe)
        .args(&arguments)
        .stderr(Stdio::inherit())
        .output()?;
    drop(server.stdin.take());
    server.wait()?;
    let stdout = String::from_utf8(output.stdout)?;
    let fields: Vec<u128> = stdout
        .lines()
        .find_map(|l| l.strip_prefix("RESULT "))
        .ok_or("measurement failed")?
        .split(' ')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    Ok(Row {
        workload: name,
        compressed,
        table,
        streams,
        batches: fields[0] as usize,
        rust_peak: fields[1] as usize,
        os_peak: fields[2] as usize,
        private_peak: fields[3] as usize,
        elapsed: Duration::from_micros(fields[4] as u64),
    })
}

/// `ZSTD_sizeof_DCtx` after decoding a frame that uses the full window.
fn zstd_context_sizes() -> Vec<(u32, usize, usize)> {
    use zstd::zstd_safe::{CParameter, DCtx, DParameter, InBuffer, OutBuffer};
    let limits = DecodeLimits::default();
    [10u32, 15, 17, 20, 23]
        .into_iter()
        .map(|window_log| {
            // Incompressible-ish data spanning the window, with no declared
            // content size, so the decoder must allocate the full window.
            let mut state = 0x9e37_79b9_7f4a_7c15u64;
            let data: Vec<u8> = (0..(1usize << window_log) + 4096)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    (state % 16) as u8
                })
                .collect();
            let mut encoder = zstd::stream::Encoder::new(Vec::new(), 3).unwrap();
            encoder
                .set_parameter(CParameter::WindowLog(window_log))
                .unwrap();
            encoder.include_contentsize(false).unwrap();
            encoder.write_all(&data).unwrap();
            let frame = encoder.finish().unwrap();
            let mut context = DCtx::create();
            context
                .set_parameter(DParameter::WindowLogMax(limits.zstd_window_log))
                .unwrap();
            let mut input = InBuffer::around(&frame);
            let mut chunk = vec![0u8; 64 * 1024];
            let mut produced = 0;
            loop {
                let mut output = OutBuffer::around(&mut chunk[..]);
                let remaining = context.decompress_stream(&mut output, &mut input).unwrap();
                produced += output.pos();
                if remaining == 0 && input.pos() == frame.len() {
                    break;
                }
            }
            assert_eq!(produced, data.len());
            let accounted = (1usize << window_log) + (16 << 20);
            (window_log, context.sizeof(), accounted)
        })
        .collect()
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1 << 20) as f64
}

fn run() -> Result<(), Failure> {
    if cfg!(debug_assertions) {
        return Err("memory profile requires a --release build".into());
    }
    let policy = EodPolicy::default();
    let limits = DecodeLimits::default();
    println!("# EOD client memory profile");
    println!();
    println!(
        "Platform: {} {}. Method: {}.",
        std::env::consts::OS,
        std::env::consts::ARCH,
        os::METHOD
    );
    println!(
        "Default policy: {} jobs, {} streams, {} MiB shared budget, {} MiB accounted per batch, ZSTD window log {}.",
        policy.concurrent_jobs,
        policy.concurrent_streams,
        policy.shared_bytes >> 20,
        limits.allocated_bytes >> 20,
        limits.zstd_window_log
    );
    println!();
    println!("## libzstd decompression context (`ZSTD_sizeof_DCtx`)");
    println!();
    println!("| Window log | Context size (MiB) | Client accounting: window + 16 MiB (MiB) |");
    println!("| ---: | ---: | ---: |");
    for (window_log, size, accounted) in zstd_context_sizes() {
        println!(
            "| {window_log} | {:.2} | {:.2} |",
            mib(size),
            mib(accounted)
        );
    }
    println!();
    println!("## Peak growth over an idle connected client (MiB)");
    println!();
    println!(
        "| Workload | Mode | Engine | Streams | Batches | Rust heap peak | OS resident peak | OS private peak | Resident minus Rust heap | Time (ms) |"
    );
    println!("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    let mut worst = (0usize, String::new());
    for name in WORKLOADS {
        let messages = if name == "text-24mib" { 2 } else { 8 };
        for compressed in [false, true] {
            for table in [false, true] {
                for streams in [1, 16] {
                    let row = profile(name, compressed, table, streams, messages, None)?;
                    let gap = row.os_peak as isize - row.rust_peak as isize;
                    if row.os_peak > worst.0 {
                        worst = (
                            row.os_peak,
                            format!(
                                "{name} {} {} x{streams}",
                                if compressed { "zstd" } else { "none" },
                                if table { "table" } else { "numeric" }
                            ),
                        );
                    }
                    println!(
                        "| {} | {} | {} | {} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {} |",
                        row.workload,
                        if row.compressed { "zstd" } else { "none" },
                        if row.table { "table" } else { "numeric" },
                        row.streams,
                        row.batches,
                        mib(row.rust_peak),
                        mib(row.os_peak),
                        mib(row.private_peak),
                        gap as f64 / (1 << 20) as f64,
                        row.elapsed.as_millis()
                    );
                }
            }
        }
    }
    println!();
    println!(
        "Largest OS resident peak growth: {:.1} MiB ({}).",
        mib(worst.0),
        worst.1
    );
    println!();
    println!("## Stream-count sweep: text-24mib, none, numeric, 2 messages per stream (MiB)");
    println!();
    println!("| Streams | Batches | Rust heap peak | OS resident peak | Per stream (resident) |");
    println!("| ---: | ---: | ---: | ---: | ---: |");
    for streams in [1, 2, 4, 8, 16] {
        let row = profile("text-24mib", false, false, streams, 2, None)?;
        println!(
            "| {streams} | {} | {:.1} | {:.1} | {:.1} |",
            row.batches,
            mib(row.rust_peak),
            mib(row.os_peak),
            mib(row.os_peak) / streams as f64
        );
    }
    println!();
    println!("## Message-size sweep: 16 streams, none, numeric, 2 messages per stream (MiB)");
    println!();
    println!("| Message | Rust heap peak | OS resident peak | Rust heap per stream |");
    println!("| --- | ---: | ---: | ---: |");
    for name in ["text-8mib", "text-16mib", "text-24mib"] {
        let row = profile(name, false, false, 16, 2, None)?;
        println!(
            "| {name} | {:.1} | {:.1} | {:.1} |",
            mib(row.rust_peak),
            mib(row.os_peak),
            mib(row.rust_peak) / 16.0
        );
    }
    println!();
    println!("## Stream-cap sweep: 16 concurrent queries, text-24mib, none, numeric (MiB)");
    println!();
    println!("`EodPolicy::concurrent_streams` limits how many queries are dispatched at once.");
    println!();
    println!("| concurrent_streams | Rust heap peak | OS resident peak | Time (ms) |");
    println!("| ---: | ---: | ---: | ---: |");
    for cap in [16, 4, 2, 1] {
        let row = profile("text-24mib", false, false, 16, 2, Some(cap))?;
        println!(
            "| {cap} | {:.1} | {:.1} | {} |",
            mib(row.rust_peak),
            mib(row.os_peak),
            row.elapsed.as_millis()
        );
    }
    Ok(())
}

fn main() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {
            serve(&args[1], args[2] == "zstd", args[3].parse()?);
            Ok(())
        }
        Some("measure") => {
            // Optional fifth argument: EodPolicy::concurrent_streams.
            let mut policy = EodPolicy::default();
            if let Some(cap) = args.get(4) {
                policy.concurrent_streams = cap.parse()?;
            }
            measure(
                args[1].clone(),
                args[2] == "table",
                args[3].parse()?,
                policy,
            )
        }
        Some("run") | None => run(),
        Some(other) => Err(format!("unknown command {other}; use `run`").into()),
    }
}
