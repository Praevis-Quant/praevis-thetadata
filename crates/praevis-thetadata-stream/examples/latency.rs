//! W10 latency (ADR-0018 design note). Manual only: `--features
//! live-tests`, `--confirm-live`, with the operator's Terminal running. It
//! does not log in: the Terminal holds the credentials, and this only
//! connects to its local WebSocket.
//!
//! It subscribes to quotes for the contracts in `--contracts` (one per
//! line: `ROOT,YYYYMMDD,STRIKE_DOLLARS,C|P`), drains events on this thread
//! for `--seconds`, and records:
//! - **added latency** per event, from the frame read (`Event::received`) to
//!   this consumer receiving it, as p50, p90, p99, p99.9 and max, against
//!   the p99 ≤ 50 µs target. The Terminal's own delay is not measurable:
//!   events carry no send time;
//! - events per second, drops, heartbeat gaps and unparsed frames;
//! - **allocations per frame** in the whole process (a counting allocator):
//!   the parse and hand-off allocate nothing, so this is `tungstenite`'s
//!   per-frame buffer and anything else;
//! - **process CPU** time and its share of one core.
//!
//! Counts and times only: no prices. The report goes under `artifacts/w10/`.
use clap::Parser;
use praevis_thetadata_stream::{Contract, Kind, Options, Right, Root, Stream};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct Counting;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;

#[derive(Parser)]
struct Cli {
    #[arg(long, required = true)]
    confirm_live: bool,
    #[arg(long)]
    run_id: String,
    /// One contract per line: `ROOT,YYYYMMDD,STRIKE_DOLLARS,C|P`.
    #[arg(long)]
    contracts: std::path::PathBuf,
    #[arg(long, default_value_t = 60)]
    seconds: u64,
    /// Queue capacity in events.
    #[arg(long, default_value_t = 1 << 16)]
    capacity: usize,
    /// A label for the report, for example `dev` or `live`.
    #[arg(long, default_value = "dev")]
    feed: String,
    /// `sleep`: block in `recv_timeout` between events (a normal consumer).
    /// `spin`: poll `try_recv` in a loop, using a core, to measure what
    /// waking a sleeping consumer costs. A measurement option only.
    #[arg(long, default_value = "sleep", value_parser = ["sleep", "spin"])]
    consumer: String,
}

/// The slowest events and when they happened, to tell periodic stalls (a
/// GC, a timer) from random ones. Fixed size; recording never allocates.
struct Slowest {
    /// (latency ns, ms since the start), smallest latency first once full.
    events: [(u64, u64); 20],
    len: usize,
}
impl Slowest {
    fn new() -> Self {
        Self {
            events: [(0, 0); 20],
            len: 0,
        }
    }
    fn record(&mut self, ns: u64, at_ms: u64) {
        if self.len < self.events.len() {
            self.events[self.len] = (ns, at_ms);
            self.len += 1;
            return;
        }
        // Replace the smallest if this one is slower.
        let (index, smallest) = self
            .events
            .iter()
            .enumerate()
            .min_by_key(|(_, (ns, _))| *ns)
            .map(|(i, e)| (i, e.0))
            .unwrap_or((0, 0));
        if ns > smallest {
            self.events[index] = (ns, at_ms);
        }
    }
    /// Slowest first: (latency µs, ms since the start).
    fn sorted(&self) -> Vec<(f64, u64)> {
        let mut out: Vec<(f64, u64)> = self.events[..self.len]
            .iter()
            .map(|(ns, at)| (*ns as f64 / 1_000.0, *at))
            .collect();
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        out
    }
}

/// Latency histogram: 1 µs buckets to 10 ms, then 1 ms buckets to 1 s, then
/// one overflow bucket. Preallocated; recording never allocates.
struct Histogram {
    fine: Vec<u64>,
    coarse: Vec<u64>,
    over: u64,
    count: u64,
    max_ns: u64,
}
impl Histogram {
    fn new() -> Self {
        Self {
            fine: vec![0; 10_000],
            coarse: vec![0; 1_000],
            over: 0,
            count: 0,
            max_ns: 0,
        }
    }
    fn record(&mut self, ns: u64) {
        self.count += 1;
        self.max_ns = self.max_ns.max(ns);
        let us = ns / 1_000;
        if us < 10_000 {
            self.fine[us as usize] += 1;
        } else if us < 1_000_000 {
            self.coarse[(us / 1_000) as usize] += 1;
        } else {
            self.over += 1;
        }
    }
    /// The bucket's upper edge in µs at quantile `q`.
    fn quantile_us(&self, q: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        let target = ((self.count as f64) * q).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (us, n) in self.fine.iter().enumerate() {
            seen += n;
            if seen >= target {
                return Some((us + 1) as f64);
            }
        }
        for (ms, n) in self.coarse.iter().enumerate() {
            seen += n;
            if seen >= target {
                return Some(((ms + 1) * 1_000) as f64);
            }
        }
        Some(self.max_ns as f64 / 1_000.0)
    }
}

/// Process CPU time (user + system) so far.
#[cfg(windows)]
fn cpu_time() -> Option<Duration> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            process: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let (mut c, mut e, mut k, mut u) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
    let ticks = |t: &FileTime| (u64::from(t.high) << 32) | u64::from(t.low);
    (ok != 0).then(|| Duration::from_nanos((ticks(&k) + ticks(&u)) * 100))
}
#[cfg(not(windows))]
fn cpu_time() -> Option<Duration> {
    // /proc/self/stat fields 14 and 15: user and system clock ticks (100 Hz
    // on Linux by convention).
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after.split_whitespace().collect();
    let ticks: u64 = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    Some(Duration::from_millis(ticks * 10))
}

fn contracts(path: &std::path::Path) -> Result<Vec<Contract>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("contracts file: {:?}", e.kind()))?;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let parts: Vec<&str> = line.split(',').map(str::trim).collect();
            let [root, expiration, strike, right] = parts.as_slice() else {
                return Err(format!("expected ROOT,YYYYMMDD,STRIKE,C|P: {line}"));
            };
            let strike: f64 = strike.parse().map_err(|_| format!("strike: {line}"))?;
            Ok(Contract {
                root: Root::new(root).ok_or_else(|| format!("root: {line}"))?,
                expiration: expiration
                    .parse()
                    .map_err(|_| format!("expiration: {line}"))?,
                strike: (strike * 1000.0).round() as i64,
                right: match *right {
                    "C" => Right::Call,
                    "P" => Right::Put,
                    _ => return Err(format!("right: {line}")),
                },
            })
        })
        .collect()
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    if !cli.confirm_live || std::env::var_os("CI").is_some() {
        eprintln!("live execution requires --confirm-live, outside CI");
        return std::process::ExitCode::FAILURE;
    }
    if cli.run_id.is_empty()
        || !cli
            .run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        eprintln!("run id must be ASCII letters, digits, '-' or '_'");
        return std::process::ExitCode::FAILURE;
    }
    let report_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/w10")
        .join(format!("{}.json", cli.run_id));
    if report_path.exists() {
        eprintln!("run id already used");
        return std::process::ExitCode::FAILURE;
    }
    let list = match contracts(&cli.contracts) {
        Ok(list) if !list.is_empty() => list,
        Ok(_) => {
            eprintln!("no contracts");
            return std::process::ExitCode::FAILURE;
        }
        Err(message) => {
            eprintln!("{message}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let stream = match Stream::connect(Options {
        capacity: cli.capacity,
        ..Default::default()
    }) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("stream: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    for contract in &list {
        if let Err(error) = stream.subscribe_quotes(contract) {
            eprintln!("subscribe: {error}");
            return std::process::ExitCode::FAILURE;
        }
    }
    let mut histogram = Histogram::new();
    let mut slowest = Slowest::new();
    let spin = cli.consumer == "spin";
    let (cpu_start, allocations_start, frames_start) = (
        cpu_time(),
        ALLOCATIONS.load(Ordering::Relaxed),
        stream.stats().frames,
    );
    let start = Instant::now();
    let end = start + Duration::from_secs(cli.seconds);
    let mut ended = false;
    while Instant::now() < end {
        let next = if spin {
            stream.try_recv()
        } else {
            stream.recv_timeout(end.saturating_duration_since(Instant::now()))
        };
        let Some(event) = next else {
            if spin {
                std::hint::spin_loop();
            }
            continue;
        };
        // The user callback would run here; the measurement is the hand-off.
        let ns = event.received.elapsed().as_nanos() as u64;
        match event.kind {
            Kind::Quote { .. } => {
                histogram.record(ns);
                slowest.record(ns, start.elapsed().as_millis() as u64);
            }
            Kind::Disconnected => {
                ended = true;
                break;
            }
            _ => {}
        }
    }
    let elapsed = start.elapsed();
    let cpu = match (cpu_start, cpu_time()) {
        (Some(a), Some(b)) => Some(b.saturating_sub(a)),
        _ => None,
    };
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - allocations_start;
    let stats = stream.close();
    let frames = stats.frames - frames_start;
    let q = |p| histogram.quantile_us(p);
    let report = serde_json::json!({
        "run_id": cli.run_id,
        "command": "w10-latency",
        "feed": cli.feed,
        "host": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "contracts": list.len(),
        "seconds": elapsed.as_secs_f64(),
        "stream_ended": ended,
        "quotes_measured": histogram.count,
        "quotes_per_second": histogram.count as f64 / elapsed.as_secs_f64(),
        "latency_us": {
            "p50": q(0.50), "p90": q(0.90), "p99": q(0.99), "p999": q(0.999),
            "max": histogram.max_ns as f64 / 1_000.0,
            "note": "bucket upper edges: 1 µs to 10 ms, then 1 ms",
        },
        "target_p99_us": 50,
        "consumer": cli.consumer,
        "slowest_us_at_ms": slowest.sorted(),
        "stats": {
            "frames": stats.frames, "quotes": stats.quotes, "heartbeats": stats.heartbeats,
            "others": stats.others, "unparsed": stats.unparsed, "dropped": stats.dropped,
            "heartbeat_gaps": stats.heartbeat_gaps,
            "heartbeat_max_interval_ms": stats.heartbeat_max_interval_ms,
        },
        "allocations_per_frame": if frames > 0 { Some(allocations as f64 / frames as f64) } else { None },
        "cpu_s": cpu.map(|c| c.as_secs_f64()),
        "cpu_share_of_one_core": cpu.map(|c| c.as_secs_f64() / elapsed.as_secs_f64()),
    });
    eprintln!(
        "{} quotes from {} contracts in {:.0} s: p50 {:?} µs, p99 {:?} µs, p99.9 {:?} µs, max {:.0} µs; dropped {}, gaps {}",
        histogram.count,
        list.len(),
        elapsed.as_secs_f64(),
        q(0.5),
        q(0.99),
        q(0.999),
        histogram.max_ns as f64 / 1_000.0,
        stats.dropped,
        stats.heartbeat_gaps
    );
    std::fs::create_dir_all(report_path.parent().expect("parent")).expect("artifacts directory");
    std::fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).expect("report"),
    )
    .expect("write report");
    println!("Wrote {}", report_path.display());
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_come_from_the_preallocated_buckets() {
        let mut h = Histogram::new();
        for us in 1..=1_000u64 {
            h.record(us * 1_000 - 1); // just under each µs edge
        }
        assert_eq!(h.quantile_us(0.5), Some(500.0));
        assert_eq!(h.quantile_us(0.99), Some(990.0));
        h.record(25_000_000); // 25 ms, a coarse bucket
        h.record(2_000_000_000); // 2 s, overflow
        assert_eq!(h.count, 1_002);
        assert_eq!(h.max_ns, 2_000_000_000);
        assert_eq!(Histogram::new().quantile_us(0.99), None);
    }

    #[test]
    fn the_slowest_events_are_kept_with_their_times() {
        let mut slowest = Slowest::new();
        for i in 0..1_000u64 {
            slowest.record(i * 1_000, i);
        }
        let top = slowest.sorted();
        assert_eq!(top.len(), 20);
        assert_eq!(top[0], (999.0, 999));
        assert_eq!(top[19], (980.0, 980));
    }

    #[test]
    fn contract_files_parse_and_refuse_bad_lines() {
        let dir = std::env::temp_dir().join(format!("w10-contracts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("c.txt");
        std::fs::write(
            &file,
            "# comment\nSPY,20261106,570,C\nSPXW,20261106,5800.5,P\n",
        )
        .unwrap();
        let list = contracts(&file).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            (list[0].strike, list[1].strike, list[1].right),
            (570_000, 5_800_500, Right::Put)
        );
        std::fs::write(&file, "SPY,20261106,570,X\n").unwrap();
        assert!(contracts(&file).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
