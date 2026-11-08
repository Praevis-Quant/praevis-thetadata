//! No allocation on the hot path this crate owns: parsing a frame and
//! handing the event to the bounded queue. (`tungstenite`'s own per-frame
//! buffer is outside this and is measured separately.) Own test binary: it
//! installs a counting allocator.
//!
//! Only allocations made **by the measuring thread, inside the measured
//! window** are counted. A process-wide count also caught the test
//! harness's own thread: on a busy runner, libtest's main thread allocates
//! while the test runs (34 of 1,000 single-core runs saw 2 such allocations,
//! none from the measuring thread), which failed CI once on unchanged code.
use praevis_thetadata_stream::{Kind, parse};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct Counting;
static MEASURED: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    /// Set only on the measuring thread, only inside the window.
    static MEASURING: Cell<bool> = const { Cell::new(false) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `try_with`: no panic while the thread-local is being torn down.
        if MEASURING.try_with(Cell::get).unwrap_or(false) {
            MEASURED.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;

const QUOTE: &str = r#"{"header":{"status":"CONNECTED","type":"QUOTE"},"contract":{"security_type":"OPTION","root":"SPXW","expiration":20240315,"strike":4800000,"right":"C"},"quote":{"ms_of_day":26622025,"bid_size":7,"bid_exchange":5,"bid":110.2,"bid_condition":50,"ask_size":7,"ask_exchange":5,"ask":110.5,"ask_condition":50,"date":20231219}}"#;

/// Allocations this thread makes while running `work`.
fn allocations_in(work: impl FnOnce()) -> usize {
    let before = MEASURED.load(Ordering::Relaxed);
    MEASURING.with(|m| m.set(true));
    work();
    MEASURING.with(|m| m.set(false));
    MEASURED.load(Ordering::Relaxed) - before
}

#[test]
fn parsing_and_queueing_do_not_allocate() {
    // What the reader queues: the parsed kind and its arrival time, as an
    // `Event` holds them (`Event` is non-exhaustive, so built only in-crate).
    let (sender, receiver) = std::sync::mpsc::sync_channel::<(Kind, Instant)>(1024);
    // Warm up the channel and the parser once.
    sender
        .try_send((parse(QUOTE).unwrap(), Instant::now()))
        .unwrap();
    receiver.try_recv().unwrap();
    let hot_path = allocations_in(|| {
        for _ in 0..10_000 {
            let kind = parse(QUOTE).unwrap();
            assert!(matches!(kind, Kind::Quote { .. }));
            sender.try_send((kind, Instant::now())).unwrap();
            receiver.try_recv().unwrap();
        }
    });
    assert_eq!(hot_path, 0);
    // The counter sees this thread's allocations: one Box is one count, so
    // the zero above is not a counter that never counts.
    let boxed = allocations_in(|| drop(std::hint::black_box(Box::new(7u64))));
    assert_eq!(boxed, 1);
}
