//! Item 8, option 2a: the per-message cost of the old and new receive paths,
//! offline, with no network.
//!
//! The two real sequences, for each message of `n` bytes arriving in
//! network-sized chunks:
//! - **old** (tonic's buffer + `FrameDecoder` before 2a): the chunks fill one
//!   buffer reused across messages, then `vec![0; n]` and one full copy;
//! - **2a**: `Vec::with_capacity(n)`, then `extend_from_slice` per chunk.
//!
//! Two diagnostic variants separate the differences between them:
//! - **2a-zeroed**: `vec![0; n]` filled chunk by chunk (zeroed versus
//!   uninitialised allocation, and how each faults its pages in);
//! - **2a-one-copy**: `with_capacity(n)` and one copy of the whole message
//!   (many small copies versus one tight copy).
//!
//! Chunk sizes cover HTTP/2 frames from 16 KiB up to 2 MiB. This runs
//! synchronously; it does not model page faults taken on an async task.
//!
//! ```text
//! cargo run --release -p praevis-thetadata-client --example frame_alloc [-- --rounds 7]
//! ```
use std::time::Instant;

#[derive(Clone, Copy)]
enum Mode {
    Old,
    New,
    NewZeroed,
    NewOneCopy,
}
const MODES: [(Mode, &str); 4] = [
    (Mode::Old, "old"),
    (Mode::New, "2a"),
    (Mode::NewZeroed, "2a-zeroed"),
    (Mode::NewOneCopy, "2a-one-copy"),
];

/// One message through one path; returns it so the work is not optimised away.
fn receive(mode: Mode, source: &[u8], chunk: usize, reused: &mut Vec<u8>) -> Vec<u8> {
    let n = source.len();
    match mode {
        Mode::Old => {
            reused.clear();
            for part in source.chunks(chunk) {
                reused.extend_from_slice(part);
            }
            let mut owned = vec![0; n];
            owned.copy_from_slice(&reused[..n]);
            owned
        }
        Mode::New => {
            let mut message = Vec::with_capacity(n);
            for part in source.chunks(chunk) {
                message.extend_from_slice(part);
            }
            message
        }
        Mode::NewZeroed => {
            let mut message = vec![0; n];
            for (i, part) in source.chunks(chunk).enumerate() {
                message[i * chunk..i * chunk + part.len()].copy_from_slice(part);
            }
            message
        }
        Mode::NewOneCopy => {
            let mut message = Vec::with_capacity(n);
            message.extend_from_slice(source);
            message
        }
    }
}

/// Seconds per GiB for `messages` messages.
fn run(mode: Mode, source: &[u8], chunk: usize, messages: usize, reused: &mut Vec<u8>) -> f64 {
    let start = Instant::now();
    for _ in 0..messages {
        // Dropped at once, as the decoder drops each frame after decoding it.
        std::hint::black_box(receive(mode, source, chunk, reused));
    }
    let gib = (source.len() * messages) as f64 / (1u64 << 30) as f64;
    start.elapsed().as_secs_f64() / gib
}

fn main() {
    let rounds: usize = std::env::args()
        .skip_while(|a| a != "--rounds")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    println!(
        "{} {} ({rounds} rounds, modes rotated each round; median s/GiB, lower is better)",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    println!(
        "{:>7} {:>9} {:>8} {:>8} {:>10} {:>12} {:>8}",
        "message", "chunk", "old", "2a", "2a-zeroed", "2a-one-copy", "2a/old"
    );
    for mib in [1usize, 8, 24, 64] {
        let bytes = mib << 20;
        let source: Vec<u8> = (0..bytes).map(|i| i as u8).collect();
        let messages = (1024 / mib).max(4);
        for chunk in [16 << 10, 256 << 10, 2 << 20] {
            let mut reused = Vec::new();
            let mut times = vec![Vec::new(); MODES.len()];
            for round in 0..rounds {
                // Rotate the order so drift does not favour one mode.
                for k in 0..MODES.len() {
                    let i = (k + round) % MODES.len();
                    times[i].push(run(MODES[i].0, &source, chunk, messages, &mut reused));
                }
            }
            let medians: Vec<f64> = times
                .iter_mut()
                .map(|v| {
                    v.sort_by(f64::total_cmp);
                    v[v.len() / 2]
                })
                .collect();
            println!(
                "{:>3} MiB {:>5} KiB {:>8.3} {:>8.3} {:>10.3} {:>12.3} {:>8.2}",
                mib,
                chunk >> 10,
                medians[0],
                medians[1],
                medians[2],
                medians[3],
                medians[1] / medians[0]
            );
        }
    }
}
