//! No allocation on the hot path this crate owns: parsing a frame and
//! handing the event to the bounded queue. (`tungstenite`'s own per-frame
//! buffer is outside this and is measured separately.) Own test binary: it
//! installs a counting allocator.
use praevis_thetadata_stream::{Event, Kind, parse};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct Counting;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
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

const QUOTE: &str = r#"{"header":{"status":"CONNECTED","type":"QUOTE"},"contract":{"security_type":"OPTION","root":"SPXW","expiration":20240315,"strike":4800000,"right":"C"},"quote":{"ms_of_day":26622025,"bid_size":7,"bid_exchange":5,"bid":110.2,"bid_condition":50,"ask_size":7,"ask_exchange":5,"ask":110.5,"ask_condition":50,"date":20231219}}"#;

#[test]
fn parsing_and_queueing_do_not_allocate() {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<Event>(1024);
    // Warm up the channel and the parser once.
    let event = Event {
        kind: parse(QUOTE).unwrap(),
        received: Instant::now(),
    };
    sender.try_send(event).unwrap();
    receiver.try_recv().unwrap();
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..10_000 {
        let kind = parse(QUOTE).unwrap();
        assert!(matches!(kind, Kind::Quote { .. }));
        sender
            .try_send(Event {
                kind,
                received: Instant::now(),
            })
            .unwrap();
        receiver.try_recv().unwrap();
    }
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed) - before, 0);
}
