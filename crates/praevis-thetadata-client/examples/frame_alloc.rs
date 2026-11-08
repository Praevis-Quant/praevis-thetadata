//! Item 8, option 2a: is a fresh exact-capacity buffer per message slower than
//! a reused one? Offline microbenchmark, no network.
//!
//! The hypothesis for 2a's large-message cost on Windows: tonic's retained
//! decode buffer acted as a reuse pool (its pages stay mapped and warm), while
//! 2a allocates one exact-capacity `Vec` per message. On Windows a large
//! allocation comes straight from `VirtualAlloc`, so every message pays the
//! page faults of first touch. This isolates exactly that: the same bytes,
//! copied in HTTP/2-sized chunks, into either a fresh buffer per message
//! (2a) or one buffer reused across messages (tonic's behaviour).
//!
//! ```text
//! cargo run --release -p praevis-thetadata-client --example frame_alloc [-- --rounds 5]
//! ```
use std::time::Instant;

/// HTTP/2 DATA frames arrive in chunks of about this size.
const CHUNK: usize = 16 << 10;

fn fill(target: &mut Vec<u8>, source: &[u8]) {
    for chunk in source.chunks(CHUNK) {
        target.extend_from_slice(chunk);
    }
}

/// Seconds per GiB for `messages` messages of `bytes` each.
fn run(bytes: usize, messages: usize, reuse: bool, source: &[u8]) -> f64 {
    let mut reused: Vec<u8> = Vec::new();
    let start = Instant::now();
    for _ in 0..messages {
        if reuse {
            reused.clear();
            reused.reserve_exact(bytes);
            fill(&mut reused, source);
            std::hint::black_box(&reused);
        } else {
            let mut fresh = Vec::with_capacity(bytes);
            fill(&mut fresh, source);
            std::hint::black_box(&fresh);
        }
    }
    let gib = (bytes * messages) as f64 / (1u64 << 30) as f64;
    start.elapsed().as_secs_f64() / gib
}

fn main() {
    let rounds: usize = std::env::args()
        .skip_while(|a| a != "--rounds")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    println!(
        "{} {} ({} rounds, alternating; s/GiB, lower is better)",
        std::env::consts::OS,
        std::env::consts::ARCH,
        rounds
    );
    println!("{:>8} {:>10} {:>10} {:>7}", "message", "fresh", "reused", "ratio");
    for mib in [1usize, 8, 24, 64] {
        let bytes = mib << 20;
        let source: Vec<u8> = (0..bytes).map(|i| i as u8).collect();
        // About 1 GiB per measurement.
        let messages = (1024 / mib).max(4);
        let (mut fresh, mut reused) = (Vec::new(), Vec::new());
        for round in 0..rounds {
            // Alternate the order so drift does not favour one side.
            if round % 2 == 0 {
                fresh.push(run(bytes, messages, false, &source));
                reused.push(run(bytes, messages, true, &source));
            } else {
                reused.push(run(bytes, messages, true, &source));
                fresh.push(run(bytes, messages, false, &source));
            }
        }
        let median = |v: &mut Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v[v.len() / 2]
        };
        let (f, r) = (median(&mut fresh), median(&mut reused));
        println!("{:>5} MiB {:>10.3} {:>10.3} {:>7.2}", mib, f, r, f / r);
    }
}
